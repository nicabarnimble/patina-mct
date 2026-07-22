use anyhow::{Context as _, Result, bail};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write as _},
    os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _},
    path::{Component, Path, PathBuf},
};

pub const MCT_DAEMON_RELEASE_ARCHIVE_MAX_BYTES: u64 = 256 * 1024 * 1024;
pub const MCT_DAEMON_RELEASE_EXTRACTED_MAX_BYTES: u64 = 512 * 1024 * 1024;
pub const MCT_DAEMON_RELEASE_MAX_ENTRIES: usize = 32;
pub const MCT_DAEMON_RELEASE_METADATA_FILE_MAX_BYTES: u64 = 8 * 1024 * 1024;

const RELEASE_MANIFEST_FILE: &str = "RELEASE-MANIFEST.json";
const RELEASE_NOTES_FILE: &str = "RELEASE-NOTES.md";
const RELEASE_SBOM_FILE: &str = "SBOM.cdx.json";
const RELEASE_FIXTURE_PROVENANCE_FILE: &str = "FIXTURE-PROVENANCE.json";
const RELEASE_LICENSE_FILE: &str = "LICENSE";
const RELEASE_CHECKSUMS_FILE: &str = "CHECKSUMS";
const RELEASE_INFO_PLIST: &str = "payload/mct-daemon.app/Contents/Info.plist";
const RELEASE_EXECUTABLE: &str = "payload/mct-daemon.app/Contents/MacOS/mct-daemon";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifestV1 {
    pub schema_version: u32,
    pub package_format_version: u32,
    pub release_mode: String,
    pub product: String,
    pub product_version: String,
    pub target_triple: String,
    pub source_commit: String,
    pub source_epoch: u64,
    pub rust_toolchain: String,
    pub rust_version: String,
    pub cargo_version: String,
    pub lockfile_sha256: String,
    pub executable_relative_path: String,
    pub executable_sha256: String,
    pub executable_blake3: String,
    pub release_notes_sha256: String,
    pub sbom_sha256: String,
    pub fixture_provenance_sha256: String,
    pub distribution_license: String,
    pub signing_mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedDaemonReleaseArchive {
    pub manifest: ReleaseManifestV1,
    pub archive_sha256: String,
    pub archive_blake3: String,
    pub archive_size_bytes: u64,
    pub release_root: PathBuf,
    pub executable_path: PathBuf,
    pub release_notes: String,
}

#[derive(Clone, Debug)]
struct EntryFact {
    path: String,
    is_dir: bool,
    size: u64,
    mode: u32,
    uid: u64,
    gid: u64,
    mtime: u64,
    sha256: Option<String>,
    blake3: Option<String>,
    metadata_bytes: Option<Vec<u8>>,
}

struct ArchiveScan {
    facts: BTreeMap<String, EntryFact>,
    manifest: ReleaseManifestV1,
    release_notes: String,
}

pub fn verify_and_extract_daemon_release_archive(
    archive_path: &Path,
    destination: &Path,
    expected_sha256: Option<&str>,
    expected_target: &str,
) -> Result<VerifiedDaemonReleaseArchive> {
    let archive_path = canonical_regular_file(archive_path, "release archive")?;
    let archive_parent = archive_path
        .parent()
        .context("release archive has no canonical parent")?;
    let archive_name = archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .context("release archive filename is not UTF-8")?;

    let metadata = fs::metadata(&archive_path)?;
    if metadata.len() > MCT_DAEMON_RELEASE_ARCHIVE_MAX_BYTES {
        bail!("release archive exceeds named byte bound");
    }
    let (archive_sha256, archive_blake3, archive_size_bytes) = hash_file(&archive_path)?;
    let tagged_archive_sha256 = format!("sha256:{archive_sha256}");
    if let Some(expected) = expected_sha256
        && expected != tagged_archive_sha256
    {
        bail!("release archive does not match expected SHA-256");
    }

    verify_external_sidecar(
        archive_parent,
        &archive_path.with_file_name(format!("{archive_name}.sha256")),
        archive_name,
        &archive_sha256,
        "SHA-256",
    )?;
    verify_external_sidecar(
        archive_parent,
        &archive_path.with_file_name(format!("{archive_name}.blake3")),
        archive_name,
        &archive_blake3,
        "BLAKE3",
    )?;

    let scan = scan_archive(&archive_path, expected_target)?;
    let expected_archive_name = format!("{}.tar.gz", release_root_name(&scan.manifest));
    if archive_name != expected_archive_name {
        bail!("release archive basename does not match manifest identity");
    }
    let gzip_mtime = gzip_header_mtime(&archive_path)?;
    if gzip_mtime != scan.manifest.source_epoch {
        bail!("release gzip header does not use the source epoch");
    }
    extract_verified_archive(&archive_path, destination, &scan.facts)?;
    let final_hashes = hash_file(&archive_path)?;
    if final_hashes.0 != archive_sha256
        || final_hashes.1 != archive_blake3
        || final_hashes.2 != archive_size_bytes
    {
        let _ = fs::remove_dir_all(destination);
        bail!("release archive changed during verification");
    }
    let release_root = destination.join(release_root_name(&scan.manifest));
    let executable_path = release_root.join(&scan.manifest.executable_relative_path);

    Ok(VerifiedDaemonReleaseArchive {
        manifest: scan.manifest,
        archive_sha256: tagged_archive_sha256,
        archive_blake3: format!("blake3:{archive_blake3}"),
        archive_size_bytes,
        release_root,
        executable_path,
        release_notes: scan.release_notes,
    })
}

fn canonical_regular_file(path: &Path, label: &str) -> Result<PathBuf> {
    let link_metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect {label} {}", path.display()))?;
    if !link_metadata.file_type().is_file() {
        bail!("{label} must be a regular non-symlink file");
    }
    let canonical = fs::canonicalize(path)
        .with_context(|| format!("canonicalize {label} {}", path.display()))?;
    if !fs::metadata(&canonical)?.is_file() {
        bail!("{label} canonical target is not a regular file");
    }
    Ok(canonical)
}

fn hash_file(path: &Path) -> Result<(String, String, u64)> {
    let mut file = fs::File::open(path)?;
    let mut sha256 = Sha256::new();
    let mut blake3 = blake3::Hasher::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .context("release archive size overflow")?;
        if total > MCT_DAEMON_RELEASE_ARCHIVE_MAX_BYTES {
            bail!("release archive exceeds named byte bound");
        }
        sha256.update(&buffer[..read]);
        blake3.update(&buffer[..read]);
    }
    Ok((
        format!("{:x}", sha256.finalize()),
        blake3.finalize().to_hex().to_string(),
        total,
    ))
}

fn gzip_header_mtime(path: &Path) -> Result<u64> {
    let mut header = [0_u8; 10];
    fs::File::open(path)?.read_exact(&mut header)?;
    if header[0..3] != [0x1f, 0x8b, 8] || header[3] != 0 {
        bail!("release archive gzip header is not canonical");
    }
    Ok(u32::from_le_bytes(header[4..8].try_into().unwrap()).into())
}

fn verify_external_sidecar(
    expected_parent: &Path,
    sidecar_path: &Path,
    archive_name: &str,
    expected_digest: &str,
    algorithm: &str,
) -> Result<()> {
    let sidecar = canonical_regular_file(sidecar_path, "release checksum sidecar")?;
    if sidecar.parent() != Some(expected_parent) {
        bail!("release checksum sidecar escapes archive parent");
    }
    let metadata = fs::metadata(&sidecar)?;
    if metadata.len() > 256 {
        bail!("release checksum sidecar exceeds byte bound");
    }
    let text = fs::read_to_string(&sidecar).context("release checksum sidecar is not UTF-8")?;
    let expected = format!("{expected_digest}  {archive_name}\n");
    if text != expected {
        bail!("release {algorithm} sidecar does not match archive");
    }
    Ok(())
}

fn scan_archive(archive_path: &Path, expected_target: &str) -> Result<ArchiveScan> {
    let file = fs::File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut facts = BTreeMap::new();
    let mut entry_order = Vec::new();
    let mut extracted_size = 0_u64;

    for entry in archive.entries().context("read release archive entries")? {
        let mut entry = entry.context("read release archive entry")?;
        if facts.len() >= MCT_DAEMON_RELEASE_MAX_ENTRIES {
            bail!("release archive exceeds named entry bound");
        }
        let path = validated_entry_path(&entry)?;
        if facts.contains_key(&path) {
            bail!("release archive contains duplicate path {path}");
        }
        let header = entry.header();
        let entry_type = header.entry_type();
        let is_dir = entry_type.is_dir();
        if !is_dir && !entry_type.is_file() {
            bail!("release archive contains forbidden entry type at {path}");
        }
        let size = header.size().context("read release archive entry size")?;
        if is_dir && size != 0 {
            bail!("release archive directory has non-zero size");
        }
        if !is_dir {
            extracted_size = extracted_size
                .checked_add(size)
                .context("release extracted size overflow")?;
            if extracted_size > MCT_DAEMON_RELEASE_EXTRACTED_MAX_BYTES {
                bail!("release archive exceeds named extracted byte bound");
            }
        }
        let mode = header.mode().context("read release archive entry mode")?;
        let uid = header.uid().context("read release archive entry uid")?;
        let gid = header.gid().context("read release archive entry gid")?;
        let mtime = header.mtime().context("read release archive entry mtime")?;
        if header.username()?.is_some_and(|name| !name.is_empty())
            || header.groupname()?.is_some_and(|name| !name.is_empty())
        {
            bail!("release archive user/group names are not normalized");
        }

        let (sha256, blake3, metadata_bytes) = if is_dir {
            (None, None, None)
        } else {
            let capture = is_metadata_path(&path);
            if capture && size > MCT_DAEMON_RELEASE_METADATA_FILE_MAX_BYTES {
                bail!("release metadata file exceeds named byte bound");
            }
            let mut sha = Sha256::new();
            let mut b3 = blake3::Hasher::new();
            let mut bytes = capture.then(Vec::new);
            let mut total = 0_u64;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = entry.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                total = total
                    .checked_add(read as u64)
                    .context("entry size overflow")?;
                if total > size {
                    bail!("release archive entry exceeds declared size");
                }
                sha.update(&buffer[..read]);
                b3.update(&buffer[..read]);
                if let Some(bytes) = &mut bytes {
                    bytes.extend_from_slice(&buffer[..read]);
                }
            }
            if total != size {
                bail!("release archive entry size does not match header");
            }
            (
                Some(format!("{:x}", sha.finalize())),
                Some(b3.finalize().to_hex().to_string()),
                bytes,
            )
        };

        entry_order.push(path.clone());
        facts.insert(
            path.clone(),
            EntryFact {
                path,
                is_dir,
                size,
                mode,
                uid,
                gid,
                mtime,
                sha256,
                blake3,
                metadata_bytes,
            },
        );
    }

    let manifest_path = facts
        .keys()
        .filter(|path| path.ends_with(&format!("/{RELEASE_MANIFEST_FILE}")))
        .cloned()
        .collect::<Vec<_>>();
    if manifest_path.len() != 1 {
        bail!("release archive must contain exactly one release manifest");
    }
    let manifest_path = &manifest_path[0];
    let manifest_bytes = facts[manifest_path]
        .metadata_bytes
        .as_deref()
        .context("release manifest bytes were not captured")?;
    validate_display_bytes(manifest_bytes, "release manifest")?;
    let manifest: ReleaseManifestV1 =
        serde_json::from_slice(manifest_bytes).context("decode release manifest")?;
    if serde_json::to_vec(&manifest)? != manifest_bytes {
        bail!("release manifest is not canonical compact JSON");
    }
    validate_manifest(&manifest, expected_target)?;
    let root = release_root_name(&manifest);
    if manifest_path != &format!("{root}/{RELEASE_MANIFEST_FILE}") {
        bail!("release manifest is not under its exact package root");
    }

    validate_exact_layout(&facts, &entry_order, &manifest)?;
    validate_internal_checksums(&facts, &manifest)?;
    validate_metadata(&facts, &manifest)?;

    let notes_path = format!("{root}/{RELEASE_NOTES_FILE}");
    let notes_bytes = facts[&notes_path]
        .metadata_bytes
        .as_deref()
        .context("release notes bytes were not captured")?;
    let release_notes = std::str::from_utf8(notes_bytes)
        .context("release notes are not UTF-8")?
        .to_owned();

    Ok(ArchiveScan {
        facts,
        manifest,
        release_notes,
    })
}

fn validated_entry_path<R: Read>(entry: &tar::Entry<'_, R>) -> Result<String> {
    let path = entry.path().context("decode release archive entry path")?;
    if path.as_os_str().is_empty() || path.is_absolute() {
        bail!("release archive path must be non-empty and relative");
    }
    if !path
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        bail!("release archive path contains an unsafe component");
    }
    let text = path
        .to_str()
        .context("release archive path is not UTF-8")?
        .to_owned();
    if text.contains('\\') || text.bytes().any(|byte| byte == 0) {
        bail!("release archive path contains an ambiguous separator");
    }
    Ok(text)
}

fn is_metadata_path(path: &str) -> bool {
    [
        RELEASE_MANIFEST_FILE,
        RELEASE_NOTES_FILE,
        RELEASE_SBOM_FILE,
        RELEASE_FIXTURE_PROVENANCE_FILE,
        RELEASE_LICENSE_FILE,
        RELEASE_CHECKSUMS_FILE,
        "Info.plist",
    ]
    .iter()
    .any(|name| path.ends_with(&format!("/{name}")))
}

fn validate_manifest(manifest: &ReleaseManifestV1, expected_target: &str) -> Result<()> {
    if manifest.schema_version != 1
        || manifest.package_format_version != 1
        || !matches!(manifest.release_mode.as_str(), "release" | "smoke")
        || manifest.product != "mct-daemon"
        || manifest.product_version != crate::version()
        || manifest.target_triple != expected_target
        || manifest.distribution_license != "MIT"
        || manifest.signing_mode != "adhoc"
        || manifest.executable_relative_path != RELEASE_EXECUTABLE
    {
        bail!("release manifest product, version, target, license, signing, or format mismatch");
    }
    if !is_lower_hex(&manifest.source_commit, 40)
        || !is_tagged_lower_hex(&manifest.lockfile_sha256, "sha256")
        || !is_tagged_lower_hex(&manifest.executable_sha256, "sha256")
        || !is_tagged_lower_hex(&manifest.executable_blake3, "blake3")
        || !is_tagged_lower_hex(&manifest.release_notes_sha256, "sha256")
        || !is_tagged_lower_hex(&manifest.sbom_sha256, "sha256")
        || !is_tagged_lower_hex(&manifest.fixture_provenance_sha256, "sha256")
        || manifest.source_epoch == 0
        || manifest.rust_toolchain.trim().is_empty()
        || manifest.rust_version.trim().is_empty()
        || manifest.cargo_version.trim().is_empty()
    {
        bail!("release manifest contains malformed provenance or digest fields");
    }
    Ok(())
}

fn validate_exact_layout(
    facts: &BTreeMap<String, EntryFact>,
    entry_order: &[String],
    manifest: &ReleaseManifestV1,
) -> Result<()> {
    let root = release_root_name(manifest);
    let expected_dirs = [
        root.clone(),
        format!("{root}/payload"),
        format!("{root}/payload/mct-daemon.app"),
        format!("{root}/payload/mct-daemon.app/Contents"),
        format!("{root}/payload/mct-daemon.app/Contents/MacOS"),
    ];
    let expected_files = [
        format!("{root}/{RELEASE_MANIFEST_FILE}"),
        format!("{root}/{RELEASE_NOTES_FILE}"),
        format!("{root}/{RELEASE_SBOM_FILE}"),
        format!("{root}/{RELEASE_FIXTURE_PROVENANCE_FILE}"),
        format!("{root}/{RELEASE_LICENSE_FILE}"),
        format!("{root}/{RELEASE_CHECKSUMS_FILE}"),
        format!("{root}/{RELEASE_INFO_PLIST}"),
        format!("{root}/{}", manifest.executable_relative_path),
    ];
    let expected = expected_dirs
        .iter()
        .chain(expected_files.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let actual = facts.keys().cloned().collect::<BTreeSet<_>>();
    if actual != expected {
        bail!("release archive layout is not exact");
    }
    let mut expected_order = expected_dirs.to_vec();
    let mut files_in_order = expected_files.to_vec();
    files_in_order.sort();
    expected_order.extend(files_in_order);
    if entry_order != expected_order {
        bail!("release archive member order is not canonical");
    }
    for directory in expected_dirs {
        let fact = &facts[&directory];
        if !fact.is_dir || fact.mode != 0o755 {
            bail!("release archive directory mode/type mismatch at {directory}");
        }
    }
    for file in expected_files {
        let fact = &facts[&file];
        let expected_mode = if file.ends_with(RELEASE_EXECUTABLE) {
            0o755
        } else {
            0o644
        };
        if fact.is_dir || fact.mode != expected_mode {
            bail!("release archive file mode/type mismatch at {file}");
        }
    }
    for fact in facts.values() {
        if fact.uid != 0 || fact.gid != 0 || fact.mtime != manifest.source_epoch {
            bail!("release archive ownership or source epoch is not normalized");
        }
    }
    Ok(())
}

fn validate_internal_checksums(
    facts: &BTreeMap<String, EntryFact>,
    manifest: &ReleaseManifestV1,
) -> Result<()> {
    let root = release_root_name(manifest);
    let checksums_path = format!("{root}/{RELEASE_CHECKSUMS_FILE}");
    let bytes = facts[&checksums_path]
        .metadata_bytes
        .as_deref()
        .context("release checksums bytes were not captured")?;
    validate_display_bytes(bytes, "release checksums")?;
    let text = std::str::from_utf8(bytes)?;
    let mut actual_lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
    if !text.ends_with('\n') || actual_lines.iter().any(|line| line.is_empty()) {
        bail!("release checksums must be non-empty newline-terminated records");
    }
    let mut sorted = actual_lines.clone();
    sorted.sort();
    if actual_lines != sorted {
        bail!("release checksums are not sorted");
    }

    let expected_lines = facts
        .values()
        .filter(|fact| !fact.is_dir && fact.path != checksums_path)
        .flat_map(|fact| {
            [
                format!(
                    "blake3 {} {}",
                    fact.blake3.as_deref().expect("file BLAKE3 must exist"),
                    fact.path.strip_prefix(&format!("{root}/")).unwrap()
                ),
                format!(
                    "sha256 {} {}",
                    fact.sha256.as_deref().expect("file SHA-256 must exist"),
                    fact.path.strip_prefix(&format!("{root}/")).unwrap()
                ),
            ]
        })
        .collect::<BTreeSet<_>>();
    let line_count = actual_lines.len();
    let actual = actual_lines.drain(..).collect::<BTreeSet<_>>();
    if actual.len() != line_count || actual != expected_lines {
        bail!("release checksums do not cover the exact package files");
    }
    Ok(())
}

fn validate_metadata(
    facts: &BTreeMap<String, EntryFact>,
    manifest: &ReleaseManifestV1,
) -> Result<()> {
    let root = release_root_name(manifest);
    let file_sha = |relative: &str| -> &str {
        facts[&format!("{root}/{relative}")]
            .sha256
            .as_deref()
            .expect("file SHA-256 must exist")
    };
    if manifest.executable_sha256 != format!("sha256:{}", file_sha(RELEASE_EXECUTABLE))
        || manifest.executable_blake3
            != format!(
                "blake3:{}",
                facts[&format!("{root}/{RELEASE_EXECUTABLE}")]
                    .blake3
                    .as_deref()
                    .expect("executable BLAKE3 must exist")
            )
        || manifest.release_notes_sha256 != format!("sha256:{}", file_sha(RELEASE_NOTES_FILE))
        || manifest.sbom_sha256 != format!("sha256:{}", file_sha(RELEASE_SBOM_FILE))
        || manifest.fixture_provenance_sha256
            != format!("sha256:{}", file_sha(RELEASE_FIXTURE_PROVENANCE_FILE))
    {
        bail!("release manifest file digests do not match package bytes");
    }

    for relative in [
        RELEASE_MANIFEST_FILE,
        RELEASE_NOTES_FILE,
        RELEASE_SBOM_FILE,
        RELEASE_FIXTURE_PROVENANCE_FILE,
        RELEASE_LICENSE_FILE,
        RELEASE_CHECKSUMS_FILE,
        RELEASE_INFO_PLIST,
    ] {
        let bytes = facts[&format!("{root}/{relative}")]
            .metadata_bytes
            .as_deref()
            .context("release metadata bytes were not captured")?;
        validate_display_bytes(bytes, relative)?;
    }
    serde_json::from_slice::<serde_json::Value>(
        facts[&format!("{root}/{RELEASE_SBOM_FILE}")]
            .metadata_bytes
            .as_deref()
            .unwrap(),
    )
    .context("decode release SBOM")?;
    serde_json::from_slice::<serde_json::Value>(
        facts[&format!("{root}/{RELEASE_FIXTURE_PROVENANCE_FILE}")]
            .metadata_bytes
            .as_deref()
            .unwrap(),
    )
    .context("decode fixture provenance")?;
    Ok(())
}

fn validate_display_bytes(bytes: &[u8], label: &str) -> Result<()> {
    let text = std::str::from_utf8(bytes).with_context(|| format!("{label} is not UTF-8"))?;
    if text.chars().any(|character| {
        (character.is_control() && character != '\n')
            || matches!(
                character,
                '\u{202A}'
                    ..='\u{202E}'
                        | '\u{2066}'
                        ..='\u{2069}'
                        | '\u{061C}'
                        | '\u{200E}'
                        | '\u{200F}'
            )
    }) {
        bail!("{label} contains forbidden terminal control text");
    }
    Ok(())
}

fn extract_verified_archive(
    archive_path: &Path,
    destination: &Path,
    facts: &BTreeMap<String, EntryFact>,
) -> Result<()> {
    if destination.exists() {
        bail!("release extraction destination must not already exist");
    }
    fs::create_dir(destination).with_context(|| {
        format!(
            "create release extraction destination {}",
            destination.display()
        )
    })?;
    let result = (|| -> Result<()> {
        let mut directories = facts
            .values()
            .filter(|fact| fact.is_dir)
            .collect::<Vec<_>>();
        directories.sort_by_key(|fact| fact.path.matches('/').count());
        for fact in directories {
            let path = destination.join(&fact.path);
            fs::create_dir(&path)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(fact.mode))?;
        }

        let file = fs::File::open(archive_path)?;
        let decoder = GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = validated_entry_path(&entry)?;
            let fact = facts.get(&path).context("archive changed between passes")?;
            if fact.is_dir {
                continue;
            }
            let output_path = destination.join(&path);
            let mut output = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(fact.mode)
                .open(&output_path)?;
            let mut sha = Sha256::new();
            let mut b3 = blake3::Hasher::new();
            let mut total = 0_u64;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = entry.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                total += read as u64;
                output.write_all(&buffer[..read])?;
                sha.update(&buffer[..read]);
                b3.update(&buffer[..read]);
            }
            let sha = format!("{:x}", sha.finalize());
            let b3 = b3.finalize().to_hex().to_string();
            if total != fact.size
                || fact.sha256.as_deref() != Some(sha.as_str())
                || fact.blake3.as_deref() != Some(b3.as_str())
            {
                bail!("archive changed during verified extraction");
            }
            output.sync_all()?;
            fs::set_permissions(&output_path, fs::Permissions::from_mode(fact.mode))?;
        }
        sync_tree_directories(destination, facts)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

fn sync_tree_directories(destination: &Path, facts: &BTreeMap<String, EntryFact>) -> Result<()> {
    let mut directories = facts
        .values()
        .filter(|fact| fact.is_dir)
        .map(|fact| destination.join(&fact.path))
        .collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        fs::File::open(&directory)?.sync_all()?;
    }
    fs::File::open(destination)?.sync_all()?;
    Ok(())
}

fn release_root_name(manifest: &ReleaseManifestV1) -> String {
    format!(
        "mct-daemon-v{}-{}",
        manifest.product_version, manifest.target_triple
    )
}

fn is_tagged_lower_hex(value: &str, algorithm: &str) -> bool {
    value
        .strip_prefix(&format!("{algorithm}:"))
        .is_some_and(|digest| is_lower_hex(digest, 64))
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

use flate2::{Compression, GzBuilder};
use mct_daemon::{
    MCT_DAEMON_RELEASE_ARCHIVE_MAX_BYTES, MCT_DAEMON_RELEASE_EXTRACTED_MAX_BYTES,
    MCT_DAEMON_RELEASE_MAX_ENTRIES, MCT_DAEMON_RELEASE_METADATA_FILE_MAX_BYTES, ReleaseManifestV1,
    verify_and_extract_daemon_release_archive,
};
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, fs, io::Write, os::unix::fs::symlink, path::PathBuf};
use tempfile::TempDir;

const TARGET: &str = "aarch64-apple-darwin";
const SOURCE_EPOCH: u64 = 1_700_000_000;

#[derive(Clone, Copy)]
enum ArchiveMutation {
    None,
    ExtraFile,
    DuplicateNotes,
    SymlinkLicense,
    Traversal,
    BadInternalChecksums,
    TerminalEscapeNotes,
}

struct ReleaseFixture {
    _temp: TempDir,
    archive: PathBuf,
    sha256_sidecar: PathBuf,
}

#[test]
fn release_archive_verifier_enforces_closed_layout_and_bounds() {
    assert_eq!(MCT_DAEMON_RELEASE_ARCHIVE_MAX_BYTES, 256 * 1024 * 1024);
    assert_eq!(MCT_DAEMON_RELEASE_EXTRACTED_MAX_BYTES, 512 * 1024 * 1024);
    assert_eq!(MCT_DAEMON_RELEASE_MAX_ENTRIES, 32);
    assert_eq!(MCT_DAEMON_RELEASE_METADATA_FILE_MAX_BYTES, 8 * 1024 * 1024);

    let fixture = release_fixture(ArchiveMutation::None);
    let destination = fixture._temp.path().join("verified");
    let verified =
        verify_and_extract_daemon_release_archive(&fixture.archive, &destination, None, TARGET)
            .unwrap();

    assert_eq!(verified.manifest.product, "mct-daemon");
    assert_eq!(verified.manifest.product_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(verified.manifest.target_triple, TARGET);
    assert_eq!(verified.release_notes, "# MCT 0.2.0\n\nRelease notes.\n");
    assert!(verified.executable_path.is_file());
    assert!(verified.release_root.starts_with(&destination));
    assert_eq!(
        fs::read(&verified.executable_path).unwrap(),
        b"#!/bin/sh\nexit 0\n"
    );
}

#[test]
fn hostile_release_archives_and_sidecars_leave_no_extracted_tree() {
    for mutation in [
        ArchiveMutation::ExtraFile,
        ArchiveMutation::DuplicateNotes,
        ArchiveMutation::SymlinkLicense,
        ArchiveMutation::Traversal,
        ArchiveMutation::BadInternalChecksums,
        ArchiveMutation::TerminalEscapeNotes,
    ] {
        let fixture = release_fixture(mutation);
        let destination = fixture._temp.path().join("rejected");
        assert!(
            verify_and_extract_daemon_release_archive(
                &fixture.archive,
                &destination,
                None,
                TARGET,
            )
            .is_err(),
            "mutation unexpectedly verified"
        );
        assert!(!destination.exists());
    }

    let fixture = release_fixture(ArchiveMutation::None);
    let real_sidecar = fixture.sha256_sidecar.with_extension("real");
    fs::rename(&fixture.sha256_sidecar, &real_sidecar).unwrap();
    symlink(&real_sidecar, &fixture.sha256_sidecar).unwrap();
    let destination = fixture._temp.path().join("sidecar-rejected");
    let error =
        verify_and_extract_daemon_release_archive(&fixture.archive, &destination, None, TARGET)
            .unwrap_err();
    assert!(error.to_string().contains("non-symlink"));
    assert!(!destination.exists());
}

#[test]
fn release_archive_expected_digest_and_target_are_additive_gates() {
    let fixture = release_fixture(ArchiveMutation::None);
    let destination = fixture._temp.path().join("wrong-digest");
    let wrong = format!("sha256:{}", "0".repeat(64));
    assert!(
        verify_and_extract_daemon_release_archive(
            &fixture.archive,
            &destination,
            Some(&wrong),
            TARGET,
        )
        .unwrap_err()
        .to_string()
        .contains("expected SHA-256")
    );
    assert!(!destination.exists());

    let destination = fixture._temp.path().join("wrong-target");
    assert!(
        verify_and_extract_daemon_release_archive(
            &fixture.archive,
            &destination,
            None,
            "x86_64-unknown-linux-gnu",
        )
        .is_err()
    );
    assert!(!destination.exists());
}

fn release_fixture(mutation: ArchiveMutation) -> ReleaseFixture {
    let temp = tempfile::tempdir().unwrap();
    let version = env!("CARGO_PKG_VERSION");
    let root = format!("mct-daemon-v{version}-{TARGET}");
    let archive_name = format!("{root}.tar.gz");
    let archive = temp.path().join(&archive_name);

    let mut files = BTreeMap::<String, Vec<u8>>::new();
    files.insert(
        "payload/mct-daemon.app/Contents/Info.plist".into(),
        b"<?xml version=\"1.0\"?>\n<plist version=\"1.0\"><dict/></plist>\n".to_vec(),
    );
    files.insert(
        "payload/mct-daemon.app/Contents/MacOS/mct-daemon".into(),
        b"#!/bin/sh\nexit 0\n".to_vec(),
    );
    files.insert(
        "RELEASE-NOTES.md".into(),
        if matches!(mutation, ArchiveMutation::TerminalEscapeNotes) {
            b"# MCT 0.2.0\n\x1b[31mforged\n".to_vec()
        } else {
            b"# MCT 0.2.0\n\nRelease notes.\n".to_vec()
        },
    );
    files.insert(
        "SBOM.cdx.json".into(),
        br#"{"bomFormat":"CycloneDX","specVersion":"1.6"}"#.to_vec(),
    );
    files.insert(
        "FIXTURE-PROVENANCE.json".into(),
        br#"{"fixtures":[]}"#.to_vec(),
    );
    files.insert("LICENSE".into(), b"MIT License\n".to_vec());

    let executable = &files["payload/mct-daemon.app/Contents/MacOS/mct-daemon"];
    let manifest = ReleaseManifestV1 {
        schema_version: 1,
        package_format_version: 1,
        release_mode: "release".into(),
        product: "mct-daemon".into(),
        product_version: version.into(),
        target_triple: TARGET.into(),
        source_commit: "1".repeat(40),
        source_epoch: SOURCE_EPOCH,
        rust_toolchain: "1.96.0".into(),
        rust_version: "rustc 1.96.0".into(),
        cargo_version: "cargo 1.96.0".into(),
        lockfile_sha256: tagged_sha256(b"lockfile"),
        executable_relative_path: "payload/mct-daemon.app/Contents/MacOS/mct-daemon".into(),
        executable_sha256: tagged_sha256(executable),
        executable_blake3: format!("blake3:{}", blake3::hash(executable).to_hex()),
        release_notes_sha256: tagged_sha256(&files["RELEASE-NOTES.md"]),
        sbom_sha256: tagged_sha256(&files["SBOM.cdx.json"]),
        fixture_provenance_sha256: tagged_sha256(&files["FIXTURE-PROVENANCE.json"]),
        distribution_license: "MIT".into(),
        signing_mode: "adhoc".into(),
    };
    files.insert(
        "RELEASE-MANIFEST.json".into(),
        serde_json::to_vec(&manifest).unwrap(),
    );

    let mut checksum_lines = files
        .iter()
        .flat_map(|(path, bytes)| {
            [
                format!("blake3 {} {path}", blake3::hash(bytes).to_hex()),
                format!("sha256 {} {path}", sha256_hex(bytes)),
            ]
        })
        .collect::<Vec<_>>();
    checksum_lines.sort();
    let mut checksums = checksum_lines.join("\n");
    checksums.push('\n');
    if matches!(mutation, ArchiveMutation::BadInternalChecksums) {
        checksums = checksums.replacen('0', "f", 1);
    }
    files.insert("CHECKSUMS".into(), checksums.into_bytes());

    let archive_file = fs::File::create(&archive).unwrap();
    let encoder = GzBuilder::new()
        .mtime(SOURCE_EPOCH as u32)
        .write(archive_file, Compression::best());
    let mut builder = tar::Builder::new(encoder);
    for directory in [
        root.clone(),
        format!("{root}/payload"),
        format!("{root}/payload/mct-daemon.app"),
        format!("{root}/payload/mct-daemon.app/Contents"),
        format!("{root}/payload/mct-daemon.app/Contents/MacOS"),
    ] {
        append_directory(&mut builder, &directory);
    }
    for (path, bytes) in &files {
        let full_path = format!("{root}/{path}");
        if matches!(mutation, ArchiveMutation::SymlinkLicense) && path == "LICENSE" {
            append_symlink(&mut builder, &full_path, "RELEASE-NOTES.md");
        } else {
            append_file(
                &mut builder,
                &full_path,
                bytes,
                if path.ends_with("/mct-daemon") {
                    0o755
                } else {
                    0o644
                },
            );
        }
        if matches!(mutation, ArchiveMutation::DuplicateNotes) && path == "RELEASE-NOTES.md" {
            append_file(&mut builder, &full_path, bytes, 0o644);
        }
    }
    if matches!(mutation, ArchiveMutation::ExtraFile) {
        append_file(&mut builder, &format!("{root}/EXTRA"), b"extra", 0o644);
    }
    if matches!(mutation, ArchiveMutation::Traversal) {
        append_raw_path_file(&mut builder, "../escape", b"escape", 0o644);
    }
    let encoder = builder.into_inner().unwrap();
    encoder.finish().unwrap().sync_all().unwrap();

    let archive_bytes = fs::read(&archive).unwrap();
    let sha256 = sha256_hex(&archive_bytes);
    let blake3 = blake3::hash(&archive_bytes).to_hex().to_string();
    let sha256_sidecar = archive.with_file_name(format!("{archive_name}.sha256"));
    fs::write(&sha256_sidecar, format!("{sha256}  {archive_name}\n")).unwrap();
    fs::write(
        archive.with_file_name(format!("{archive_name}.blake3")),
        format!("{blake3}  {archive_name}\n"),
    )
    .unwrap();

    ReleaseFixture {
        _temp: temp,
        archive,
        sha256_sidecar,
    }
}

fn append_directory<W: Write>(builder: &mut tar::Builder<W>, path: &str) {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Directory);
    header.set_path(path).unwrap();
    header.set_size(0);
    normalized_header(&mut header, 0o755);
    header.set_cksum();
    builder.append(&header, &[][..]).unwrap();
}

fn append_file<W: Write>(builder: &mut tar::Builder<W>, path: &str, bytes: &[u8], mode: u32) {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_path(path).unwrap();
    header.set_size(bytes.len() as u64);
    normalized_header(&mut header, mode);
    header.set_cksum();
    builder.append(&header, bytes).unwrap();
}

fn append_symlink<W: Write>(builder: &mut tar::Builder<W>, path: &str, target: &str) {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_path(path).unwrap();
    header.set_link_name(target).unwrap();
    header.set_size(0);
    normalized_header(&mut header, 0o777);
    header.set_cksum();
    builder.append(&header, &[][..]).unwrap();
}

fn append_raw_path_file<W: Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
    bytes: &[u8],
    mode: u32,
) {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(bytes.len() as u64);
    normalized_header(&mut header, mode);
    let raw = header.as_mut_bytes();
    raw[..100].fill(0);
    raw[..path.len()].copy_from_slice(path.as_bytes());
    header.set_cksum();
    builder.append(&header, bytes).unwrap();
}

fn normalized_header(header: &mut tar::Header, mode: u32) {
    header.set_mode(mode);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(SOURCE_EPOCH);
}

fn tagged_sha256(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

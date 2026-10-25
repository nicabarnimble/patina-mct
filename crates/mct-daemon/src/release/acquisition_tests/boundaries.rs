//! M2/M3: source mutation and tar extension allocation boundaries, without races.
use super::*;
use std::{cell::Cell, rc::Rc};

struct Counted<R> {
    inner: R,
    bytes: Rc<Cell<u64>>,
}
impl<R: Read> Read for Counted<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(out)?;
        self.bytes.set(self.bytes.get() + n as u64);
        Ok(n)
    }
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzBuilder::new()
        .mtime(EPOCH as u32)
        .write(Vec::new(), Compression::best());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

fn header(kind: u8, size: u64) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.set_path("extension").unwrap();
    header.set_entry_type(tar::EntryType::new(kind));
    header.set_size(size);
    normalize(&mut header, 0o644);
    header
}

fn extension_tar(kind: u8, body: &[u8], rest: &[u8]) -> Vec<u8> {
    let mut raw = header(kind, body.len() as u64).as_bytes().to_vec();
    raw.extend_from_slice(body);
    raw.resize(raw.len().next_multiple_of(512), 0);
    raw.extend_from_slice(rest);
    raw
}

#[test]
fn hard_budget_exact_eof_overflow_and_growth_are_not_truncated_success() {
    let count = Rc::new(Cell::new(0));
    let input = Counted {
        inner: io::repeat(0),
        bytes: count.clone(),
    };
    let mut reader = HardByteBudget::new(input, 16, "budget exceeded");
    let mut bytes = Vec::new();
    assert!(reader.read_to_end(&mut bytes).is_err());
    assert_eq!(bytes.len(), 16);
    assert_eq!(count.get(), 17, "one overflow probe only");
    assert!(reader.read(&mut [0; 1]).is_err());
    assert_eq!(count.get(), 17, "sticky error must not read more input");
    assert_eq!(reader.read(&mut []).unwrap(), 0);
    assert_eq!(
        read_bounded_bytes(&[1; 16][..], 16, "bound").unwrap(),
        [1; 16]
    );
    assert!(read_bounded_bytes(&[1; 17][..], 16, "bound").is_err());
    assert!(read_bounded_bytes(&[][..], 0, "bound").unwrap().is_empty());
    assert!(read_bounded_bytes(&[1][..], 0, "bound").is_err());
}

#[test]
fn tar_extension_streams_fail_inside_the_decompressed_budget() {
    const CAP: u64 = 4096;
    for kind in [b'L', b'K', b'x', b'g'] {
        // Small compressed fixtures, 64 KiB of extension data. The following
        // ordinary entry makes the unguarded iterator consume all metadata.
        let mut rest = header(b'0', 0).as_bytes().to_vec();
        rest.extend_from_slice(&[0; 1024]);
        let compressed = gzip(&extension_tar(kind, &vec![b'a'; 64 * 1024], &rest));
        if let Some(dir) = std::env::var_os("MCT_RELEASE_EVIDENCE_DIR") {
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                Path::new(&dir).join(format!("extension-{}.tar.gz", kind as char)),
                &compressed,
            )
            .unwrap();
        }
        let count = Rc::new(Cell::new(0));
        let decoder = Counted {
            inner: GzDecoder::new(compressed.as_slice()),
            bytes: count.clone(),
        };
        let mut guarded =
            tar::Archive::new(HardByteBudget::new(decoder, CAP, DECOMPRESSED_BUDGET_ERROR));
        let result = (|| -> Result<()> {
            for entry in guarded.entries()? {
                io::copy(&mut entry?, &mut io::sink())?;
            }
            Ok(())
        })();
        assert!(format!("{:#}", result.unwrap_err()).contains(DECOMPRESSED_BUDGET_ERROR));
        assert_eq!(count.get(), CAP + 1, "kind {}", kind as char);
        let error = scan_archive_reader_with_budget(Cursor::new(&compressed), TARGET, CAP)
            .err()
            .unwrap();
        assert!(
            format!("{error:#}").contains(DECOMPRESSED_BUDGET_ERROR),
            "{error:#}"
        );

        // Bounded-size negative control reproduces the old iterator's blind
        // spot without allocating the audit's reported gigabyte-sized buffer.
        let unguarded_count = Rc::new(Cell::new(0));
        let mut unguarded = tar::Archive::new(Counted {
            inner: GzDecoder::new(compressed.as_slice()),
            bytes: unguarded_count.clone(),
        });
        for entry in unguarded.entries().unwrap() {
            io::copy(&mut entry.unwrap(), &mut io::sink()).unwrap();
        }
        assert!(unguarded_count.get() > CAP);
        eprintln!(
            "extension {}: unguarded={} decoded bytes, guarded={} (cap={CAP}, one probe)",
            kind as char,
            unguarded_count.get(),
            count.get()
        );
    }
}

#[test]
fn oversized_declared_metadata_is_rejected_before_reading_extension_bodies() {
    for kind in [b'L', b'K', b'x', b'g'] {
        // No body at all: the raw preflight must report the size violation,
        // not enter the cooked iterator's allocation/read and report EOF.
        let compressed = gzip(header(kind, 1024 * 1024 * 1024).as_bytes());
        let error = scan_archive_reader(Cursor::new(compressed), TARGET)
            .err()
            .unwrap();
        assert!(
            format!("{error:#}").contains("tar extension metadata exceeds"),
            "{error:#}"
        );
    }
    // The metadata bound is aggregate, not a per-header loophole.
    let body = vec![0; (MCT_DAEMON_RELEASE_TAR_METADATA_MAX_BYTES / 2) as usize];
    let last = header(b'x', 1);
    let raw = extension_tar(b'L', &body, &extension_tar(b'K', &body, last.as_bytes()));
    let error = preflight_tar_metadata(
        gzip(&raw).as_slice(),
        MCT_DAEMON_RELEASE_DECOMPRESSED_MAX_BYTES,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("tar extension metadata exceeds"));
}

#[test]
fn release_decompressed_exact_boundary_padding_and_truncation_are_checked() {
    let dir = tempfile::tempdir().unwrap();
    let source = release_fixture(dir.path());
    let bytes = fs::read(source).unwrap();
    let mut raw = Vec::new();
    GzDecoder::new(bytes.as_slice())
        .read_to_end(&mut raw)
        .unwrap();
    let cap = raw.len() as u64;
    assert!(scan_archive_reader_with_budget(Cursor::new(&bytes), TARGET, cap).is_ok());
    let error = scan_archive_reader_with_budget(Cursor::new(&bytes), TARGET, cap - 1)
        .err()
        .unwrap();
    assert!(format!("{error:#}").contains(DECOMPRESSED_BUDGET_ERROR));
    raw.extend_from_slice(&[0; 512]);
    let padded = gzip(&raw);
    let error = scan_archive_reader_with_budget(Cursor::new(padded), TARGET, cap)
        .err()
        .unwrap();
    assert!(
        format!("{error:#}").contains(DECOMPRESSED_BUDGET_ERROR),
        "padding after tar EOF is still charged"
    );
    assert!(
        scan_archive_reader(Cursor::new(&bytes[..bytes.len() - 8]), TARGET).is_err(),
        "missing gzip trailer cannot pass after tar EOF"
    );
}

#[test]
fn ordinary_gnu_and_pax_metadata_still_pass_and_raw_entry_count_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let source = release_fixture(dir.path());
    let mut raw = Vec::new();
    GzDecoder::new(fs::File::open(&source).unwrap())
        .read_to_end(&mut raw)
        .unwrap();
    let root = format!("mct-daemon-v{}-{TARGET}/", crate::version());
    let gnu = format!("{root}\0").into_bytes();
    let record = format!("path={root}\n");
    let mut length = record.len() + 2;
    loop {
        let next = record.len() + length.to_string().len() + 1;
        if next == length {
            break;
        }
        length = next;
    }
    let pax = format!("{length} {record}").into_bytes();
    for (kind, body) in [(b'L', gnu), (b'x', pax)] {
        assert!(
            scan_archive_reader(Cursor::new(gzip(&extension_tar(kind, &body, &raw))), TARGET)
                .is_ok()
        );
    }
    let headers = header(b'5', 0)
        .as_bytes()
        .repeat(RELEASE_MAX_RAW_ENTRIES + 1);
    let error = preflight_tar_metadata(
        gzip(&headers).as_slice(),
        MCT_DAEMON_RELEASE_DECOMPRESSED_MAX_BYTES,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("raw entry bound"));
}

fn snapshot(path: &Path) -> ReleaseArchiveSnapshot {
    ReleaseArchiveSnapshot::read(
        fs::File::open(path).unwrap(),
        path.file_name().unwrap().to_str().unwrap().into(),
    )
    .unwrap()
}

#[test]
fn source_replacement_or_in_place_write_after_staging_cannot_change_installed_bytes() {
    for replace in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let source = release_fixture(dir.path());
        let replacement = release_fixture_with_executable(other.path(), b"other valid executable");
        let staged = snapshot(&source);
        let expected = tagged_sha(&staged.bytes);
        if replace {
            fs::rename(&replacement, &source).unwrap();
        } else {
            fs::write(&source, fs::read(&replacement).unwrap()).unwrap();
        }
        assert_ne!(tagged_sha(&fs::read(&source).unwrap()), expected);
        let verified = staged
            .verify_and_extract(&dir.path().join("output"), Some(&expected), TARGET)
            .unwrap();
        assert_eq!(verified.archive_sha256, expected);
        assert_eq!(
            fs::read(verified.executable_path).unwrap(),
            b"test executable"
        );
    }
}

struct MutatingSource {
    file: fs::File,
    path: PathBuf,
    replacement: PathBuf,
    replace: bool,
    mutated: bool,
}
impl Read for MutatingSource {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if !self.mutated && !out.is_empty() {
            // Both valid gzip archives start with this byte. Mutation occurs
            // inside staging, after a read, with no sleep or racing thread.
            let n = self.file.read(&mut out[..1])?;
            if self.replace {
                fs::rename(&self.replacement, &self.path)?;
            } else {
                fs::write(&self.path, fs::read(&self.replacement)?)?;
            }
            self.mutated = true;
            return Ok(n);
        }
        self.file.read(out)
    }
}

#[test]
fn mutation_during_staging_is_either_original_snapshot_or_digest_refusal() {
    for replace in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let source = release_fixture(dir.path());
        let expected = tagged_sha(&fs::read(&source).unwrap());
        let replacement = release_fixture_with_executable(other.path(), b"other valid executable");
        let staged = ReleaseArchiveSnapshot::read(
            MutatingSource {
                file: fs::File::open(&source).unwrap(),
                path: source.clone(),
                replacement,
                replace,
                mutated: false,
            },
            source.file_name().unwrap().to_str().unwrap().into(),
        )
        .unwrap();
        let dest = dir.path().join("output");
        let result = staged.verify_and_extract(&dest, Some(&expected), TARGET);
        if replace {
            let verified = result.unwrap();
            assert_eq!(verified.archive_sha256, expected);
            assert_eq!(
                fs::read(verified.executable_path).unwrap(),
                b"test executable"
            );
        } else {
            assert!(result.unwrap_err().to_string().contains("expected SHA-256"));
            assert!(!dest.exists());
        }
    }
}

#[test]
fn growing_source_and_sidecar_reads_cannot_escape_their_caps() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("growing");
    let replacement = dir.path().join("larger");
    fs::write(&path, [1; 16]).unwrap();
    fs::write(&replacement, [2; 1024]).unwrap();
    let source = MutatingSource {
        file: fs::File::open(&path).unwrap(),
        path,
        replacement,
        replace: false,
        mutated: false,
    };
    assert!(
        read_bounded_bytes(source, 16, "compressed cap")
            .unwrap_err()
            .to_string()
            .contains("compressed cap")
    );
    let archive = release_fixture(dir.path());
    let name = archive.file_name().unwrap().to_str().unwrap();
    fs::write(
        archive.with_file_name(format!("{name}.sha256")),
        [b'a'; 257],
    )
    .unwrap();
    let dest = dir.path().join("output");
    let error =
        verify_and_extract_daemon_release_archive(&archive, &dest, None, TARGET).unwrap_err();
    assert!(error.to_string().contains("sidecar exceeds byte bound"));
    assert!(!dest.exists());
}

#[test]
fn publication_sidecars_use_verified_snapshot_not_mutable_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = release_fixture(dir.path());
    let original = fs::read(&source).unwrap();
    let name = source.file_name().unwrap().to_str().unwrap();
    let request = MctDaemonReleaseAcquisitionRequest {
        source_path: source.clone(),
        expected_archive_identity: Some(tagged_sha(&original)),
        target_triple: TARGET.into(),
        releases_dir: dir.path().join("releases"),
        state_path: dir.path().join("state.sqlite"),
        ledger_path: dir.path().join("ledger.jsonl"),
        authenticated_uid: 501,
        policy_revision: 1,
    };
    let mut ledger =
        JsonlObservationLedger::open(&request.ledger_path, "ledger-local", "local-mct").unwrap();
    let report = acquire_operator_file_daemon_release_with_platform_verifier(
        &request,
        |_| Ok(()),
        |observation| {
            if observation.kind == ObservationKind::ArtifactVerified {
                fs::write(&source, b"replaced after verification")?;
                for algorithm in ["sha256", "blake3"] {
                    fs::write(
                        source.with_file_name(format!("{name}.{algorithm}")),
                        b"unverified substituted sidecar",
                    )?;
                }
            }
            append_release_observation(&mut ledger, observation)
        },
    )
    .unwrap();
    let root = report.artifact.immutable_release_path;
    assert_eq!(
        fs::read_to_string(root.join("archive.sha256")).unwrap(),
        format!("{}  {name}\n", sha_hex(&original))
    );
    assert_eq!(
        fs::read_to_string(root.join("archive.blake3")).unwrap(),
        format!("{}  {name}\n", blake3::hash(&original).to_hex())
    );
    assert_eq!(
        fs::read(root.join(RELEASE_EXECUTABLE)).unwrap(),
        b"test executable"
    );
}

#[test]
fn extraction_errors_clean_only_new_destination_and_preserve_existing_paths() {
    let dir = tempfile::tempdir().unwrap();
    let staged = snapshot(&release_fixture(dir.path()));
    let dest = dir.path().join("output");
    fs::create_dir(&dest).unwrap();
    fs::write(dest.join("sentinel"), b"preserve").unwrap();
    assert!(staged.verify_and_extract(&dest, None, TARGET).is_err());
    assert_eq!(fs::read(dest.join("sentinel")).unwrap(), b"preserve");
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&dest, &link).unwrap();
    assert!(staged.verify_and_extract(&link, None, TARGET).is_err());
    assert_eq!(fs::read(dest.join("sentinel")).unwrap(), b"preserve");
    let mut scan = scan_archive_reader(Cursor::new(&staged.bytes), TARGET).unwrap();
    let file = scan.facts.values_mut().find(|fact| !fact.is_dir).unwrap();
    file.sha256 = Some("0".repeat(64));
    let partial = dir.path().join("partial");
    assert!(extract_verified_archive(&staged.bytes, &partial, &scan.facts).is_err());
    assert!(!partial.exists());
    let verified = staged
        .verify_and_extract(&dir.path().join("private"), None, TARGET)
        .unwrap();
    assert_eq!(
        fs::metadata(verified.release_root.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

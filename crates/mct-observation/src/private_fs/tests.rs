use super::*;

#[test]
fn sqlite_sidecar_rechecks_an_unlinked_private_snapshot() {
    let root = private_tempdir();
    let path = root.path().canonicalize().unwrap().join("state-wal");
    let (dir, name) = parent(&path, false, true, false).unwrap();
    let old = create_new(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(rfs::fstat(&old).unwrap().st_nlink, 0);
    for replacement in [false, true] {
        let _current = replacement.then(|| create_new(&path).unwrap());
        let mut lookups = 0;
        let result = verify_optional_stat(|| {
            lookups += 1;
            if lookups == 1 {
                return Ok(rfs::fstat(&old)?);
            }
            Ok(rfs::statat(&dir, &name, rfs::AtFlags::SYMLINK_NOFOLLOW)?)
        });
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(lookups, 2);
    }
}

#[test]
fn sqlite_sidecar_recheck_never_accepts_unsafe_metadata_or_io_errors() {
    let root = private_tempdir();
    let path = root.path().canonicalize().unwrap().join("state-wal");
    let file = create_new(&path).unwrap();
    for bad in ["mode", "owner", "type", "hardlink", "io"] {
        let mut lookups = 0;
        let result = verify_optional_stat(|| {
            lookups += 1;
            let mut stat = rfs::fstat(&file)?;
            if lookups == 1 {
                stat.st_nlink = 0;
                return Ok(stat);
            }
            match bad {
                "mode" => stat.st_mode = 0o100644,
                "owner" => stat.st_uid = current_uid().wrapping_add(1),
                "type" => stat.st_mode = 0o120600,
                "hardlink" => stat.st_nlink = 2,
                "io" => return Err(io::Error::other("injected storage failure")),
                _ => unreachable!(),
            }
            Ok(stat)
        });
        assert!(result.is_err(), "accepted {bad}");
        assert_eq!(lookups, 2);
    }
    for bad in ["mode", "owner", "type"] {
        let mut lookups = 0;
        assert!(
            verify_optional_stat(|| {
                lookups += 1;
                let mut stat = rfs::fstat(&file)?;
                stat.st_nlink = 0;
                match bad {
                    "mode" => stat.st_mode = 0o100644,
                    "owner" => stat.st_uid = current_uid().wrapping_add(1),
                    "type" => stat.st_mode = 0o120600,
                    _ => unreachable!(),
                }
                Ok(stat)
            })
            .is_err()
        );
        assert_eq!(lookups, 1, "unsafe zero-link snapshot was retried");
    }
}

#[test]
fn sqlite_sidecar_unstable_path_is_bounded_and_database_guard_stays_strict() {
    let root = private_tempdir();
    let path = root.path().canonicalize().unwrap().join("state-wal");
    let file = create_new(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(validate_private_stat(&rfs::fstat(&file).unwrap()).is_err());
    let mut lookups = 0;
    let error = verify_optional_stat(|| {
        lookups += 1;
        Ok(rfs::fstat(&file)?)
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(lookups, 3);
}

#[test]
fn sqlite_sidecar_public_path_recovers_after_churn_stops() {
    let root = private_tempdir();
    let path = root.path().canonicalize().unwrap().join("state-wal");
    std::thread::scope(|scope| {
        let writer = scope.spawn(|| {
            for _ in 0..256 {
                drop(create_new(&path).unwrap());
                std::fs::remove_file(&path).unwrap();
            }
        });
        while !writer.is_finished() {
            if let Err(error) = verify_optional_metadata(&path) {
                assert_eq!(error.kind(), io::ErrorKind::WouldBlock, "{error}");
            }
        }
        writer.join().unwrap();
    });
    verify_optional_metadata(&path).unwrap();
    drop(create_new(&path).unwrap());
    verify_optional_metadata(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        verify_optional_metadata(&path).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o7777, 0o644);
}

fn private_tempdir() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt as _;
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
use std::os::unix::fs::{PermissionsExt, symlink};

#[test]
fn l1_private_creation_under_permissive_umask() {
    const CHILD: &str = "MCT_PRIVATE_FS_UMASK_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let path = root.join("mother/nested/config.json");
        write_atomic(&path, b"private").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o7777, 0o600);
        for p in [root.join("mother"), root.join("mother/nested")] {
            assert_eq!(std::fs::metadata(p).unwrap().mode() & 0o7777, 0o700);
        }
        let ledger = root.join("observations/ledger.jsonl");
        drop(crate::JsonlObservationLedger::open(&ledger, "ledger", "mother").unwrap());
        assert_eq!(std::fs::metadata(ledger).unwrap().mode() & 0o7777, 0o600);
        return;
    }
    let dir = private_tempdir();
    let status = std::process::Command::new("/bin/sh")
        .args(["-c", "umask 000; exec \"$1\" --exact private_fs::tests::l1_private_creation_under_permissive_umask --nocapture", "mct-umask-test"])
        .arg(std::env::current_exe().unwrap()).env(CHILD, dir.path().canonicalize().unwrap())
        .status().unwrap();
    assert!(status.success());
}

#[test]
fn l1_symlinks_hardlinks_and_unsafe_modes_are_refused_without_mutation() {
    let dir = private_tempdir();
    let root = dir.path().canonicalize().unwrap();
    let target = root.join("target");
    write_atomic(&target, b"preserve").unwrap();
    let link = root.join("link");
    symlink(&target, &link).unwrap();
    assert!(read(&link).is_err());
    assert!(crate::JsonlObservationLedger::open(&link, "ledger", "mother").is_err());
    assert!(write_atomic(&link, b"overwrite").is_err());
    std::fs::remove_file(&link).unwrap();
    symlink(root.join("absent"), &link).unwrap();
    assert!(open_or_create(&link).is_err());
    std::fs::remove_file(&link).unwrap();
    std::fs::hard_link(&target, &link).unwrap();
    assert!(read(&target).is_err());
    std::fs::remove_file(&link).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(read(&target).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"preserve");
    assert_eq!(
        std::fs::metadata(&target).unwrap().mode() & 0o777,
        0o644,
        "no silent chmod repair"
    );
    let alias = root.join("alias");
    symlink(&root, &alias).unwrap();
    assert!(write_atomic(&alias.join("new"), b"no").is_err());
    assert!(!root.join("new").exists());
    assert!(!private_file_facts(
        current_uid(),
        0o600,
        current_uid().wrapping_add(1)
    ));
}

#[test]
fn l4_executable_leaf_and_ancestor_permissions_are_checked_before_hashing() {
    // Unlike private data storage, executable ancestry rejects even a sticky
    // shared temporary parent. Keep the positive fixture out of Linux /tmp.
    let home = PathBuf::from(std::env::var_os("HOME").expect("test HOME"))
        .canonicalize()
        .unwrap();
    ensure_publication_directory(&home).expect("trusted executable fixture parent");
    let dir = tempfile::Builder::new()
        .prefix("mct-executable-test-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in(home)
        .unwrap();
    let root = dir.path().canonicalize().unwrap();
    let parent = root.join("bin");
    ensure_directory(&parent).unwrap();
    let path = parent.join("mother");
    write_atomic(&path, b"host executable fixture").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(executable(&path).is_ok());
    for mode in [0o770, 0o1777] {
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(mode)).unwrap();
        assert!(executable(&path).is_err());
    }
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    for mode in [0o722, 0o600, 0o4700] {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        assert!(executable(&path).is_err());
    }
    assert!(trusted_file_facts(0, 0o755, 501));
    assert!(trusted_file_facts(501, 0o700, 501));
    assert!(!trusted_file_facts(502, 0o755, 501));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let alias = root.join("alias");
    symlink(&parent, &alias).unwrap();
    assert!(executable(&alias.join("mother")).is_err());
    let alias_file = parent.join("alias-mother");
    symlink(&path, &alias_file).unwrap();
    assert!(executable(&alias_file).is_err());
}

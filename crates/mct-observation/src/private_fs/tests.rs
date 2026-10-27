use super::*;
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
    let dir = private_tempdir();
    let root = dir.path().canonicalize().unwrap();
    let parent = root.join("bin");
    ensure_directory(&parent).unwrap();
    let path = parent.join("mother");
    write_atomic(&path, b"host executable fixture").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(executable(&path).is_ok());
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o770)).unwrap();
    assert!(executable(&path).is_err());
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

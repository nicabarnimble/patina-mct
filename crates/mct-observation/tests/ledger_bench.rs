#[cfg(unix)]
#[test]
fn benchmark_provisions_private_storage_under_permissive_umask() {
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new("/bin/sh")
        .args([
            "-c",
            "umask 000; exec \"$1\" --sizes 1",
            "ledger-bench-test",
            env!("CARGO_BIN_EXE_ledger-bench"),
        ])
        .env("TMPDIR", root.path().canonicalize().unwrap())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("entries=1 "));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

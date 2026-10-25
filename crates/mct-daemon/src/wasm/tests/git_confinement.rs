//! Composed WASI-write -> WIT Git -> production confined Toy -> native Git.
//! Raw Git is an independent positive control, never an adapter backend.
use super::*;
use sha2::{Digest, Sha256};

fn fixture_path() -> PathBuf {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/h1-git-child/child.wasm");
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(&path).unwrap())),
        "bba25cd1fa6c437a41f98e7f2897c0c25f1077d5c520f3331abdcfa8ce6263ca",
        "fixture must match reviewed/rebuilt source provenance",
    );
    path
}

fn adapters(repo: &Path, filesystem: bool) -> MctWitHostImportAdapters {
    let mut adapters = git_host_adapters(repo.to_owned());
    adapters.wasi = Some(MctWasiHostConfig {
        preopens: if filesystem {
            vec![MctWasiPreopen {
                host_path: repo.to_owned(),
                guest_path: "/project".into(),
                access: MctWasiPreopenAccess::ReadWrite,
                authorized_toy_call: crate::authority_test_fixture::authorized_toy_for_call(
                    &call(),
                    "toy-filesystem",
                    ChildInstanceId::new("instance-wasm").unwrap(),
                    "preopen-project-root",
                    "h1-wasm-filesystem",
                ),
            }]
        } else {
            Vec::new()
        },
    });
    adapters
}

fn invoke(mode: &str, adapters: MctWitHostImportAdapters) -> MctWitComponentInvocationReport {
    let child = loaded_typed_child(fixture_path(), vec!["patina:demo/control@0.1.0.run".into()]);
    runtime()
        .invoke_authorized_child_wit_export_with_host_adapters(
            authorized(),
            &child,
            &typed_call("run"),
            &serde_json::json!([mode]),
            adapters,
            ids(),
        )
        .unwrap()
}

#[test]
fn h1_wasm_git_composition_verifies_unconfined_control_and_blocks_both_filters_in_production() {
    for mode in ["clean", "process"] {
        let repo = init_git_repo();
        let outside = tempfile::tempdir().unwrap();
        let inside_marker = repo.path().join("wasm-helper-ran-inside");
        let outside_marker = outside.path().join("wasm-helper-ran-outside");
        fs::write(
            repo.path().join("marker-paths"),
            format!(
                "{}\n{}\n",
                inside_marker.display(),
                outside_marker.display()
            ),
        )
        .unwrap();

        // First let the WASM Child write the payload and call the production
        // adapter. Then run raw Git on those exact bytes as a positive control,
        // clear its markers, and repeat through the production adapter.
        for attempt in 0..2 {
            let before_config = fs::read(repo.path().join(".git/config")).unwrap();
            let report = invoke(mode, adapters(repo.path(), true));
            assert_eq!(report.result.outcome, ResultOutcome::Success, "{report:?}");
            assert_eq!(
                fs::read(repo.path().join("README.md")).unwrap(),
                b"written by authorized WASM Child\n"
            );
            assert_ne!(
                fs::read(repo.path().join(".git/config")).unwrap(),
                before_config,
                "the WASM Child, not the host fixture, must write the repository config"
            );
            assert!(
                fs::read_to_string(repo.path().join(".gitattributes"))
                    .unwrap()
                    .contains("test-filter")
            );
            assert!(!inside_marker.exists(), "{mode}: {report:?}");
            assert!(!outside_marker.exists(), "{mode}: {report:?}");
            assert!(
                report
                    .observations
                    .iter()
                    .any(|o| o.kind == ObservationKind::ToyCallStarted)
            );
            assert!(
                report
                    .observations
                    .iter()
                    .any(|o| o.kind == ObservationKind::ToyCallFailed)
            );
            let durable = serde_json::to_string(&report.observations).unwrap();
            assert!(!durable.contains("test-filter"));
            assert!(!durable.contains(repo.path().to_str().unwrap()));
            assert!(!durable.contains(outside.path().to_str().unwrap()));

            if attempt == 0 {
                let control = std::process::Command::new("git")
                    .current_dir(repo.path())
                    .env_clear()
                    .env("PATH", "/usr/bin:/bin")
                    .env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .args(["add", "--", "README.md"])
                    .output()
                    .unwrap();
                assert!(!control.status.success(), "{control:?}");
                assert!(
                    inside_marker.exists(),
                    "{mode}: positive control must execute"
                );
                assert!(
                    outside_marker.exists(),
                    "{mode}: positive control must escape"
                );
                fs::remove_file(&inside_marker).unwrap();
                fs::remove_file(&outside_marker).unwrap();
            }
        }
    }
}

#[test]
fn h1_wasm_git_composition_preserves_authorized_edit_stage_commit() {
    let repo = init_git_repo();
    let before_metadata = fs::read(repo.path().join(".git/config")).unwrap();
    let report = invoke("ordinary", adapters(repo.path(), true));
    assert_eq!(report.result.outcome, ResultOutcome::Success, "{report:?}");
    assert_eq!(
        report
            .observations
            .iter()
            .filter(|o| o.kind == ObservationKind::ToyCallCompleted)
            .count(),
        2,
        "{report:?}"
    );
    let committed = std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["show", "HEAD:README.md"])
        .output()
        .unwrap();
    assert!(committed.status.success());
    assert_eq!(committed.stdout, b"written by authorized WASM Child\n");
    assert_eq!(
        fs::read(repo.path().join(".git/config")).unwrap(),
        before_metadata,
        "no enrollment, metadata relocation, or repository configuration rewrite"
    );
}

#[test]
fn h1_wasm_git_composition_without_filesystem_delegation_cannot_modify_repo_config() {
    let repo = init_git_repo();
    let before = fs::read(repo.path().join(".git/config")).unwrap();
    let report = invoke("clean", adapters(repo.path(), false));
    assert_eq!(fs::read(repo.path().join(".git/config")).unwrap(), before);
    assert!(!repo.path().join(".gitattributes").exists());
    assert!(
        !report
            .observations
            .iter()
            .any(|o| o.kind == ObservationKind::ToyCallStarted)
    );
}

//! H1 regressions at the admitted Git Toy boundary. These do not claim that
//! writable Git metadata or arbitrary clean/process filters are isolated.
use super::*;
use std::os::unix::fs::PermissionsExt;

fn raw_git(repo: &Path, args: &[&str]) -> std::process::Output {
    // Independent of the production command builder: prove each hostile fixture
    // really executes with ordinary Git, rather than testing a broken payload.
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(args)
        .output()
        .unwrap()
}

fn config(repo: &Path, key: &str, value: &str) {
    let output = raw_git(repo, &["config", "--local", key, value]);
    assert!(output.status.success(), "{output:?}");
}

fn helper(repo: &Path, evidence: &Path, name: &str) -> (PathBuf, PathBuf) {
    let path = repo.join(".h1-helpers").join(name);
    let marker = evidence.join(format!("{name}.executed"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Only a sentinel in this test's private sibling directory is written.
    // No network, real project files, credentials, or persistent daemons.
    let quoted_marker = marker.to_str().unwrap().replace('\'', "'\\''");
    std::fs::write(
        &path,
        format!("#!/bin/sh\nprintf executed > '{quoted_marker}'\nexit 1\n"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    (path, marker)
}

fn prove_fixture(repo: &Path, args: &[&str], marker: &Path) {
    let _output = raw_git(repo, args);
    assert!(
        marker.exists(),
        "ordinary Git must execute the hostile fixture"
    );
    std::fs::remove_file(marker).unwrap();
}

fn assert_safe_success(repo: &Path, request: Value, marker: &Path, stem: &str) {
    let report = git_report(repo, &request.to_string(), stem);
    assert!(
        !marker.exists(),
        "Git Toy executed repository-selected native code: {stem}"
    );
    assert_eq!(report.outcome, MctToyAdapterOutcome::Success, "{report:?}");
    assert_eq!(
        report.observations[1].kind,
        ObservationKind::ToyCallCompleted
    );
}

#[test]
fn h1_git_toy_never_executes_default_or_redirected_hooks() {
    for redirected in [false, true] {
        let repo = init_git_repo();
        let evidence = tempfile::tempdir().unwrap();
        let (script, marker) = helper(repo.path(), evidence.path(), "pre-commit");
        if redirected {
            config(
                repo.path(),
                "core.hooksPath",
                script.parent().unwrap().to_str().unwrap(),
            );
        } else {
            std::fs::copy(&script, repo.path().join(".git/hooks/pre-commit")).unwrap();
        }
        std::fs::write(repo.path().join("README.md"), "authorized edit\n").unwrap();
        let stage = git_report(
            repo.path(),
            r#"{"interface":"patina:git/git@0.1.0","function":"add-paths","paths":["README.md"]}"#,
            "h1-stage",
        );
        assert_eq!(stage.outcome, MctToyAdapterOutcome::Success);
        prove_fixture(repo.path(), &["commit", "-m", "control"], &marker);
        assert_safe_success(
            repo.path(),
            serde_json::json!({"interface":"patina:git/git@0.1.0", "function":"commit", "message":"authorized commit"}),
            &marker,
            "h1-hooks",
        );
        let committed = raw_git(repo.path(), &["show", "HEAD:README.md"]);
        assert_eq!(committed.stdout, b"authorized edit\n");
    }
}

#[test]
fn h1_git_toy_never_executes_repository_fsmonitor() {
    let repo = init_git_repo();
    let evidence = tempfile::tempdir().unwrap();
    let (script, marker) = helper(repo.path(), evidence.path(), "fsmonitor");
    config(repo.path(), "core.fsmonitor", script.to_str().unwrap());
    prove_fixture(repo.path(), &["status", "--porcelain"], &marker);
    assert_safe_success(
        repo.path(),
        serde_json::json!({"interface":"patina:git/git@0.1.0", "function":"status-porcelain"}),
        &marker,
        "h1-fsmonitor",
    );
}

#[test]
fn h1_git_toy_never_executes_repository_commit_or_tag_signers() {
    for tag in [false, true] {
        let repo = init_git_repo();
        let evidence = tempfile::tempdir().unwrap();
        let (script, marker) = helper(repo.path(), evidence.path(), "signer");
        config(repo.path(), "gpg.format", "openpgp");
        config(repo.path(), "gpg.program", script.to_str().unwrap());
        config(repo.path(), "commit.gpgSign", "true");
        config(repo.path(), "tag.gpgSign", "true");
        config(repo.path(), "tag.forceSignAnnotated", "true");
        let request = if tag {
            prove_fixture(
                repo.path(),
                &["tag", "-a", "control", "-m", "control"],
                &marker,
            );
            serde_json::json!({"interface":"patina:git/git@0.1.0", "function":"create-tag", "name":"authorized-tag"})
        } else {
            std::fs::write(repo.path().join("README.md"), "authorized edit\n").unwrap();
            assert!(raw_git(repo.path(), &["add", "README.md"]).status.success());
            prove_fixture(repo.path(), &["commit", "-m", "control"], &marker);
            serde_json::json!({"interface":"patina:git/git@0.1.0", "function":"commit", "message":"authorized commit"})
        };
        assert_safe_success(repo.path(), request, &marker, "h1-signers");
    }
}

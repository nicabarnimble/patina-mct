//! Production macOS upstream queries plus deterministic command-output tests.
use super::*;

const QUERIES: [&str; 2] = ["commits-behind-upstream", "is-diverged"];

fn query(repo: &Path, function: &str) -> MctToyCallReport {
    let mut registry = MctToyAdapterRegistry::new();
    registry.register(
        ToyId::new("toy-git").unwrap(),
        MctToyBackend::GitCommand {
            repo_root: repo.to_owned(),
        },
    );
    registry.call_authorized_toy_at(
        &authorized("toy-git"),
        &call(),
        &effect_time(),
        &serde_json::json!({"interface":"patina:git/git@0.1.0", "function":function}).to_string(),
        ids("upstream-query"),
    )
}

fn counts(repo: &Path, behind: u32, diverged: bool) {
    for (function, expected) in QUERIES.into_iter().zip([
        serde_json::json!({"ok":behind}),
        serde_json::json!({"ok":diverged}),
    ]) {
        let report = query(repo, function);
        assert_eq!(report.outcome, MctToyAdapterOutcome::Success, "{report:?}");
        assert_eq!(
            serde_json::from_str::<Value>(report.output_json.as_ref().unwrap()).unwrap(),
            expected
        );
    }
}

fn failed(repo: &Path) {
    for function in QUERIES {
        let report = query(repo, function);
        assert_eq!(
            report.outcome,
            MctToyAdapterOutcome::Failed,
            "{function}: {report:?}"
        );
        assert_eq!(report.output_json, None);
        assert_eq!(report.observations[1].kind, ObservationKind::ToyCallFailed);
        assert_eq!(report.safe_message, "git command failed");
        let durable = serde_json::to_string(&report).unwrap();
        assert!(!durable.contains(repo.to_str().unwrap()));
        assert!(!durable.contains("PRIVATE-UPSTREAM"));
    }
}

#[test]
fn upstream_queries_distinguish_absence_equal_ahead_behind_and_diverged() {
    let repo = init_git_repo();
    let root = repo.path();
    run_git_for_test(root, &["branch", "-m", "query-branch"]);
    // Newlines in a value must not manufacture a branch.merge key.
    run_git_for_test(
        root,
        &[
            "config",
            "test.multiline",
            "value\nbranch.query-branch.merge\nrefs/heads/missing",
        ],
    );
    counts(root, 0, false);
    run_git_for_test(root, &["branch", "upstream"]);
    run_git_for_test(root, &["branch", "--set-upstream-to=upstream"]);
    counts(root, 0, false);
    run_git_for_test(root, &["commit", "--allow-empty", "-m", "ahead"]);
    counts(root, 0, false);
    run_git_for_test(root, &["checkout", "upstream"]);
    run_git_for_test(root, &["commit", "--allow-empty", "-m", "other side"]);
    run_git_for_test(root, &["checkout", "query-branch"]);
    counts(root, 1, true);
    run_git_for_test(root, &["reset", "--hard", "HEAD~1"]);
    counts(root, 1, false);
}

#[test]
fn upstream_queries_fail_for_unborn_detached_and_non_repository_roots() {
    let empty = tempfile::tempdir().unwrap();
    failed(empty.path());
    run_git_for_test(empty.path(), &["init", "-b", "unborn"]);
    failed(empty.path());
    run_git_for_test(empty.path(), &["config", "branch.unborn.remote", "."]);
    run_git_for_test(
        empty.path(),
        &[
            "config",
            "branch.unborn.merge",
            "refs/heads/PRIVATE-UPSTREAM",
        ],
    );
    failed(empty.path());
    let repo = init_git_repo();
    run_git_for_test(repo.path(), &["checkout", "--detach"]);
    failed(repo.path());
}

#[test]
fn upstream_queries_fail_for_broken_configuration_and_missing_upstream() {
    let repo = init_git_repo();
    let root = repo.path();
    run_git_for_test(root, &["branch", "-m", "query-branch"]);
    run_git_for_test(
        root,
        &[
            "config",
            "branch.query-branch.merge",
            "refs/heads/PRIVATE-UPSTREAM",
        ],
    );
    // A merge setting without a remote is broken, not an absent upstream.
    failed(root);
    run_git_for_test(root, &["config", "branch.query-branch.remote", "."]);
    failed(root);
    run_git_for_test(root, &["branch", "PRIVATE-UPSTREAM"]);
    counts(root, 0, false);
    // Configured remote without a fetch mapping must not look unconfigured.
    run_git_for_test(root, &["config", "branch.query-branch.remote", "origin"]);
    run_git_for_test(
        root,
        &[
            "config",
            "remote.origin.url",
            "https://invalid.example/repo",
        ],
    );
    failed(root);
    std::fs::write(root.join(".git/config"), "[PRIVATE-UPSTREAM\n").unwrap();
    failed(root);
}

#[cfg(unix)]
#[test]
fn upstream_queries_propagate_late_command_errors_and_reject_invalid_counts() {
    // Deterministic process seam: only the named stage fails, after all prior
    // commands succeeded. This is a command-output test, not sandbox evidence.
    fn stub(root: &Path) -> Result<Command, GitToyError> {
        let mut command = Command::new("/bin/sh");
        command.current_dir(root).arg(root.join("git-stub"));
        Ok(command)
    }
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(
        repo.path().join("git-stub"),
        r#"
read -r stage < stage
if [ "$1" = "$stage" ]; then
    printf PRIVATE-UPSTREAM >&2
    exit 128
fi
case "$1" in
    rev-parse) printf '%040d\n' 1 ;;
    symbolic-ref) printf 'refs/heads/main\n' ;;
    config) printf 'branch.main.remote\n.\000branch.main.merge\nrefs/heads/upstream\000' ;;
    rev-list) cat counts ;;
    *) exit 99 ;;
esac
"#,
    )
    .unwrap();
    let executor = GitExecutor {
        repo_root: repo.path(),
        command_builder: stub,
    };
    for stage in ["rev-parse", "symbolic-ref", "config", "rev-list"] {
        std::fs::write(repo.path().join("stage"), format!("{stage}\n")).unwrap();
        std::fs::write(repo.path().join("counts"), "0\t0\n").unwrap();
        for function in QUERIES {
            let result = call_git_toy(
                &executor,
                &serde_json::json!({
                    "interface":"patina:git/git@0.1.0", "function":function
                })
                .to_string(),
            );
            assert!(
                matches!(result, Err(GitToyError::GitFailed)),
                "{stage}: {result:?}"
            );
        }
    }
    std::fs::write(repo.path().join("stage"), "none\n").unwrap();
    for output in [
        "",
        "0",
        "garbage\t0",
        "0\tgarbage",
        "-1\t0",
        "0\t4294967296",
        "0\t0\t0",
    ] {
        std::fs::write(repo.path().join("counts"), output).unwrap();
        assert!(
            matches!(git_upstream_counts(&executor), Err(GitToyError::GitFailed)),
            "{output:?}"
        );
    }
    std::fs::write(repo.path().join("counts"), "2\t3\n").unwrap();
    assert_eq!(git_upstream_counts(&executor).unwrap(), (2, 3));
}

//! Production macOS Git confinement through the ordinary admitted Toy registry.
use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

fn raw(repo: &Path, args: &[&str]) -> std::process::Output {
    // Same executable, independent command builder, no OS confinement.
    Command::new(confinement::executable().unwrap())
        .current_dir(repo)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .output()
        .unwrap()
}

fn configure(repo: &Path, key: &str, value: &str) {
    let output = raw(repo, &["config", key, value]);
    assert!(output.status.success(), "{output:?}");
}

fn confined(repo: &Path, request: Value) -> MctToyCallReport {
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
        &request.to_string(),
        ids("h1-confinement"),
    )
}

fn invoke(repo: &Path, function: &str, mut fields: Value) -> MctToyCallReport {
    fields["interface"] = Value::from("patina:git/git@0.1.0");
    fields["function"] = Value::from(function);
    confined(repo, fields)
}

fn success(repo: &Path, function: &str, fields: Value) -> Value {
    let report = invoke(repo, function, fields);
    assert_eq!(
        report.outcome,
        MctToyAdapterOutcome::Success,
        "{function}: {report:?}"
    );
    assert_eq!(
        report.observations[1].kind,
        ObservationKind::ToyCallCompleted
    );
    serde_json::from_str(report.output_json.as_ref().unwrap()).unwrap()
}

fn configure_filter_fixture(
    repo: &Path,
    outside: &Path,
    driver: &str,
    required: bool,
) -> (PathBuf, PathBuf) {
    let inside_marker = repo.join("helper-ran-inside");
    let outside_marker = outside.join("helper-ran-outside");
    let quote = |p: &Path| p.to_str().unwrap().replace('\'', "'\\''");
    let script = repo.join("FILTER-FIXTURE-PATH");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf executed > '{}'\nprintf executed > '{}'\nexit 1\n",
            quote(&inside_marker),
            quote(&outside_marker),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    configure(
        repo,
        &format!("filter.arbitrary-name.{driver}"),
        script.to_str().unwrap(),
    );
    configure(
        repo,
        "filter.arbitrary-name.required",
        if required { "true" } else { "false" },
    );
    std::fs::write(
        repo.join(".gitattributes"),
        "README.md filter=arbitrary-name\n",
    )
    .unwrap();
    std::fs::write(repo.join("README.md"), "untrusted worktree edit\n").unwrap();
    (inside_marker, outside_marker)
}

fn upstream_queries_fail(repo: &Path) {
    upstream_queries_fail_with_message(repo, "git command failed");
}

fn upstream_queries_fail_with_message(repo: &Path, safe_message: &str) {
    for function in ["commits-behind-upstream", "is-diverged"] {
        let report = invoke(repo, function, serde_json::json!({}));
        assert_eq!(
            report.outcome,
            MctToyAdapterOutcome::Failed,
            "{function}: {report:?}"
        );
        assert_eq!(report.output_json, None);
        assert_eq!(report.safe_message, safe_message);
        assert_eq!(report.observations[1].kind, ObservationKind::ToyCallFailed);
        let durable = serde_json::to_string(&report).unwrap();
        assert!(!durable.contains(repo.to_str().unwrap()));
        assert!(!durable.contains("PRIVATE-UPSTREAM"));
    }
}

#[test]
fn h1_confinement_upstream_queries_fail_for_non_repository_and_redirected_metadata() {
    let original = init_git_repo();
    let child = original.path().join("not-a-repo");
    std::fs::create_dir(&child).unwrap();
    assert!(
        raw(&child, &["rev-parse", "--verify", "HEAD"])
            .status
            .success()
    );
    upstream_queries_fail(&child);
    for gitfile in [false, true] {
        let root = tempfile::tempdir().unwrap();
        upstream_queries_fail(root.path());
        let metadata = original.path().join(".git");
        if gitfile {
            std::fs::write(
                root.path().join(".git"),
                format!("gitdir: {}\n", metadata.display()),
            )
            .unwrap();
        } else {
            symlink(&metadata, root.path().join(".git")).unwrap();
        }
        assert!(
            raw(root.path(), &["rev-parse", "--verify", "HEAD"])
                .status
                .success()
        );
        upstream_queries_fail_with_message(root.path(), "git repository layout unsupported");
    }
}

#[test]
fn h1_confinement_upstream_queries_fail_for_unborn_head() {
    let repo = tempfile::tempdir().unwrap();
    assert!(raw(repo.path(), &["init", "-b", "unborn"]).status.success());
    upstream_queries_fail(repo.path());
    configure(repo.path(), "branch.unborn.remote", ".");
    configure(
        repo.path(),
        "branch.unborn.merge",
        "refs/heads/PRIVATE-UPSTREAM",
    );
    upstream_queries_fail(repo.path());
}

#[test]
fn h1_confinement_upstream_queries_fail_for_unreadable_or_denied_config() {
    let repo = init_git_repo();
    let config = repo.path().join(".git/config");
    let permissions = std::fs::metadata(&config).unwrap().permissions();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Refuse to count a root/privileged run as evidence of unreadability.
    assert!(!raw(repo.path(), &["config", "--list"]).status.success());
    upstream_queries_fail(repo.path());
    std::fs::set_permissions(&config, permissions).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let external = outside.path().join("PRIVATE-UPSTREAM-config");
    std::fs::rename(&config, &external).unwrap();
    symlink(&external, &config).unwrap();
    assert!(raw(repo.path(), &["config", "--list"]).status.success());
    upstream_queries_fail(repo.path());
}

#[test]
fn h1_confinement_upstream_queries_propagate_history_denial_after_head_verification() {
    let repo = init_git_repo();
    let root = repo.path();
    let base = String::from_utf8(raw(root, &["rev-parse", "HEAD"]).stdout).unwrap();
    run_git_for_test(root, &["branch", "-m", "query-branch"]);
    run_git_for_test(root, &["branch", "upstream"]);
    run_git_for_test(root, &["branch", "--set-upstream-to=upstream"]);
    run_git_for_test(root, &["commit", "--allow-empty", "-m", "local"]);
    run_git_for_test(root, &["checkout", "upstream"]);
    run_git_for_test(root, &["commit", "--allow-empty", "-m", "remote"]);
    run_git_for_test(root, &["checkout", "query-branch"]);
    assert_eq!(
        success(root, "commits-behind-upstream", serde_json::json!({}))["ok"],
        1
    );
    assert_eq!(
        success(root, "is-diverged", serde_json::json!({}))["ok"],
        true
    );

    // Both tip commits stay readable. Only traversal of their common parent is
    // denied, proving a successful HEAD/upstream check is not the boundary.
    let base = base.trim();
    let object = root.join(".git/objects").join(&base[..2]).join(&base[2..]);
    let outside = tempfile::tempdir().unwrap();
    let external = outside.path().join("PRIVATE-UPSTREAM-object");
    std::fs::rename(&object, &external).unwrap();
    symlink(&external, &object).unwrap();
    for revision in ["HEAD^{commit}", "@{upstream}^{commit}"] {
        let output = confinement::git_command(root)
            .unwrap()
            .args(["rev-parse", "--verify", revision])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    let control = raw(
        root,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    );
    assert!(control.status.success(), "{control:?}");
    assert_eq!(String::from_utf8(control.stdout).unwrap().trim(), "1\t1");
    upstream_queries_fail(root);
}

#[test]
fn h1_confinement_clean_and_process_filters_cannot_start_even_inside_the_allowed_root() {
    for driver in ["clean", "process"] {
        for required in [false, true] {
            let repo = init_git_repo();
            let outside = tempfile::tempdir().unwrap();
            let (inside, outside) =
                configure_filter_fixture(repo.path(), outside.path(), driver, required);
            let _control = raw(repo.path(), &["add", "README.md"]);
            assert!(
                inside.exists() && outside.exists(),
                "positive control: {driver}"
            );
            std::fs::remove_file(&inside).unwrap();
            std::fs::remove_file(&outside).unwrap();
            let report = invoke(
                repo.path(),
                "add-paths",
                serde_json::json!({"paths":["README.md"]}),
            );
            assert!(
                !inside.exists() && !outside.exists(),
                "{driver}: {report:?}"
            );
            if required {
                assert_eq!(
                    report.outcome,
                    MctToyAdapterOutcome::Failed,
                    "{driver}: {report:?}"
                );
            } else {
                // Native Git falls back to the unfiltered bytes for OPTIONAL
                // filters. Do not misreport this as a failed staging operation.
                assert_eq!(
                    report.outcome,
                    MctToyAdapterOutcome::Success,
                    "{driver}: {report:?}"
                );
                assert_eq!(
                    raw(repo.path(), &["show", ":README.md"]).stdout,
                    b"untrusted worktree edit\n"
                );
            }
            let durable = serde_json::to_string(&report).unwrap();
            assert!(!durable.contains("FILTER-FIXTURE-PATH"));
            assert!(!durable.contains(repo.path().to_str().unwrap()));
        }
    }
}

#[test]
fn h1_confinement_preserves_all_thirteen_git_operations_for_an_ordinary_checkout() {
    let repo = init_git_repo();
    let root = repo.path();
    std::fs::write(root.join("README.md"), "ordinary authorized edit\n").unwrap();
    std::fs::write(root.join("remove-me"), "ordinary file\n").unwrap();
    assert_eq!(
        success(root, "is-clean-tracked", serde_json::json!({}))["ok"],
        false
    );
    assert!(
        success(root, "status-porcelain", serde_json::json!({}))["ok"]
            .as_str()
            .unwrap()
            .contains("README.md")
    );
    assert!(
        success(root, "diff-stat", serde_json::json!({}))["ok"]
            .as_str()
            .unwrap()
            .contains("README.md")
    );
    success(
        root,
        "add-paths",
        serde_json::json!({"paths":["README.md", "remove-me"]}),
    );
    let commit = success(
        root,
        "commit",
        serde_json::json!({"message":"ordinary commit"}),
    );
    assert_eq!(commit["ok"].as_str().unwrap().len(), 40);
    assert_eq!(
        raw(root, &["show", "HEAD:README.md"]).stdout,
        b"ordinary authorized edit\n"
    );
    assert_eq!(
        success(root, "is-clean-tracked", serde_json::json!({}))["ok"],
        true
    );
    assert!(
        success(root, "log-oneline", serde_json::json!({"limit":2}))["ok"][0]
            .as_str()
            .unwrap()
            .contains("ordinary commit")
    );
    success(root, "create-tag", serde_json::json!({"name":"first"}));
    success(
        root,
        "create-tag-at",
        serde_json::json!({"name":"second", "git_ref":"HEAD"}),
    );
    assert_eq!(
        success(root, "tag-exists", serde_json::json!({"name":"first"}))["ok"],
        true
    );
    success(root, "delete-tag", serde_json::json!({"name":"first"}));
    assert_eq!(
        success(root, "tag-exists", serde_json::json!({"name":"first"}))["ok"],
        false
    );
    assert_eq!(
        success(root, "commits-behind-upstream", serde_json::json!({}))["ok"],
        0
    );
    assert_eq!(
        success(root, "is-diverged", serde_json::json!({}))["ok"],
        false
    );
    success(
        root,
        "remove-paths",
        serde_json::json!({"paths":["remove-me"]}),
    );
    assert!(!root.join("remove-me").exists());
}

#[test]
fn h1_confinement_is_enforced_after_configuration_changes_not_by_a_preflight_scan() {
    let repo = init_git_repo();
    let outside = tempfile::tempdir().unwrap();
    let mut command = confinement::git_command(repo.path()).unwrap();
    // The production command and all policy parameters already exist. Mutate the
    // repository only AFTER that point, immediately before native Git starts.
    let (inside, outside) = configure_filter_fixture(repo.path(), outside.path(), "clean", true);
    let _control = raw(repo.path(), &["add", "README.md"]);
    assert!(inside.exists() && outside.exists());
    std::fs::remove_file(&inside).unwrap();
    std::fs::remove_file(&outside).unwrap();
    let output = command.args(["add", "--", "README.md"]).output().unwrap();
    assert!(!output.status.success());
    assert!(!inside.exists() && !outside.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Operation not permitted"));
}

#[test]
fn h1_confinement_does_not_discover_a_parent_repository() {
    let repo = init_git_repo();
    let child = repo.path().join("not-a-repository");
    std::fs::create_dir(&child).unwrap();
    assert!(raw(&child, &["status", "--porcelain"]).status.success());
    let report = invoke(&child, "status-porcelain", serde_json::json!({}));
    assert_eq!(report.outcome, MctToyAdapterOutcome::Failed);
}

#[test]
fn h1_confinement_denies_redirected_metadata_without_mutating_the_external_repository() {
    for gitfile in [false, true] {
        let original = init_git_repo();
        let root = tempfile::tempdir().unwrap();
        let metadata = original.path().join(".git");
        if gitfile {
            std::fs::write(
                root.path().join(".git"),
                format!("gitdir: {}\n", metadata.display()),
            )
            .unwrap();
        } else {
            symlink(&metadata, root.path().join(".git")).unwrap();
        }
        assert!(
            raw(root.path(), &["tag", "-a", "control", "-m", "control"])
                .status
                .success()
        );
        let before = raw(original.path(), &["show-ref"]).stdout;
        let report = invoke(
            root.path(),
            "create-tag",
            serde_json::json!({"name":"must-not-exist"}),
        );
        assert_eq!(report.outcome, MctToyAdapterOutcome::Failed, "{report:?}");
        assert_eq!(raw(original.path(), &["show-ref"]).stdout, before);
    }

    // Real linked worktrees also use external metadata; there is no implicit
    // authority to traverse the main repository's common directory.
    let original = init_git_repo();
    let parent = tempfile::tempdir().unwrap();
    let linked = parent.path().join("linked-worktree");
    let output = raw(
        original.path(),
        &["worktree", "add", "--detach", linked.to_str().unwrap()],
    );
    assert!(output.status.success(), "{output:?}");
    assert!(raw(&linked, &["status", "--porcelain"]).status.success());
    let before = raw(original.path(), &["show-ref"]).stdout;
    let report = invoke(
        &linked,
        "create-tag",
        serde_json::json!({"name":"must-not-exist"}),
    );
    assert_eq!(report.outcome, MctToyAdapterOutcome::Failed);
    assert_eq!(report.safe_message, "git repository layout unsupported");
    upstream_queries_fail_with_message(&linked, "git repository layout unsupported");
    let no_op = invoke(&linked, "remove-paths", serde_json::json!({"paths":[]}));
    assert_eq!(no_op.outcome, MctToyAdapterOutcome::Failed);
    assert_eq!(no_op.safe_message, "git repository layout unsupported");
    assert_eq!(raw(original.path(), &["show-ref"]).stdout, before);

    // A layout preflight alone is insufficient: redirect .git AFTER the
    // production command is built and prove OS enforcement still refuses it.
    let repo = init_git_repo();
    let mut command = confinement::git_command(repo.path()).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let metadata = outside.path().join("metadata");
    std::fs::rename(repo.path().join(".git"), &metadata).unwrap();
    symlink(&metadata, repo.path().join(".git")).unwrap();
    assert!(
        raw(repo.path(), &["status", "--porcelain"])
            .status
            .success()
    );
    let output = command.args(["status", "--porcelain"]).output().unwrap();
    assert!(!output.status.success(), "{output:?}");
}

#[test]
fn h1_confinement_policy_parameters_are_data_not_policy_source() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("repo\") (allow default) ;\nwith quote");
    let repo = init_git_repo();
    std::fs::rename(repo.path(), &root).unwrap();
    success(
        &root,
        "create-tag",
        serde_json::json!({"name":"ordinary-tag"}),
    );
    let outside = tempfile::tempdir().unwrap();
    let (inside, outside) = configure_filter_fixture(&root, outside.path(), "clean", true);
    // Quote the command itself because this test deliberately uses a path with
    // whitespace and shell metacharacters; ordinary Git must verify the positive control.
    let script = root.join("FILTER-FIXTURE-PATH");
    configure(
        &root,
        "filter.arbitrary-name.clean",
        &format!("'{}'", script.display()),
    );
    let _control = raw(&root, &["add", "README.md"]);
    assert!(inside.exists() && outside.exists());
    std::fs::remove_file(&inside).unwrap();
    std::fs::remove_file(&outside).unwrap();
    let report = invoke(
        &root,
        "add-paths",
        serde_json::json!({"paths":["README.md"]}),
    );
    assert_eq!(report.outcome, MctToyAdapterOutcome::Failed);
    assert!(!inside.exists() && !outside.exists());
}

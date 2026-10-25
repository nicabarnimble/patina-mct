//! Platforms without an OS confinement backend must never dispatch native Git.
use super::*;

#[test]
fn git_toy_unsupported_platform_fails_every_operation_with_redacted_observations() {
    let repo = tempfile::tempdir().unwrap();
    assert!(matches!(
        GitExecutor::confined(repo.path()),
        Err(GitToyError::ConfinementUnsupported)
    ));
    for function in [
        "create-tag",
        "create-tag-at",
        "delete-tag",
        "tag-exists",
        "commit",
        "log-oneline",
        "diff-stat",
        "status-porcelain",
        "add-paths",
        "remove-paths",
        "is-clean-tracked",
        "commits-behind-upstream",
        "is-diverged",
    ] {
        let report = git_report(
            repo.path(),
            &serde_json::json!({
                "interface": "patina:git/git@0.1.0",
                "function": function,
                "name": "PRIVATE-REF-MARKER",
                "git_ref": "HEAD",
                "message": "PRIVATE-MESSAGE-MARKER",
                "limit": 1,
                // Even an otherwise successful no-op must fail unsupported.
                "paths": [],
            })
            .to_string(),
            "git-unsupported",
        );
        assert_eq!(
            report.outcome,
            MctToyAdapterOutcome::Failed,
            "{function}: {report:?}"
        );
        assert_eq!(report.output_json, None);
        assert_eq!(
            report.safe_message,
            "git confinement unavailable on this platform"
        );
        assert_eq!(report.authority_denial, None);
        assert_eq!(report.observations.len(), 2);
        assert_eq!(report.observations[0].kind, ObservationKind::ToyCallStarted);
        assert_eq!(report.observations[1].kind, ObservationKind::ToyCallFailed);
        assert_eq!(report.observations[1].outcome, ObservationOutcome::Failed);
        assert_eq!(report.observations[1].safe_message, report.safe_message);
        let durable = serde_json::to_string(&report).unwrap();
        assert!(!durable.contains(repo.path().to_str().unwrap()));
        assert!(!durable.contains("PRIVATE-REF-MARKER"));
        assert!(!durable.contains("PRIVATE-MESSAGE-MARKER"));
    }
    assert_eq!(std::fs::read_dir(repo.path()).unwrap().count(), 0);
}

use mct_kernel::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    process::Command,
};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MctToyBackend {
    EchoJson,
    StaticFailure { safe_message: String },
    GitCommand { repo_root: PathBuf },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctToyAdapterRegistry {
    backends: BTreeMap<ToyId, MctToyBackend>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctToyAdapterOutcome {
    Success,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctToyCallIds {
    pub started_observation_id: ObservationId,
    pub completed_observation_id: ObservationId,
    pub started_at: Timestamp,
    pub completed_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctToyCallReport {
    pub outcome: MctToyAdapterOutcome,
    pub output_json: Option<String>,
    pub safe_message: String,
    pub observations: Vec<MctObservation>,
}

impl MctToyAdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, toy_id: ToyId, backend: MctToyBackend) {
        self.backends.insert(toy_id, backend);
    }

    pub fn call_authorized_toy(
        &self,
        authorized: &AuthorizedToyCall,
        call: &MctCall,
        input_json: &str,
        ids: MctToyCallIds,
    ) -> MctToyCallReport {
        if authorized.policy_revision() != call.authority_context.policy_revision
            || authorized.grants_revision() != call.authority_context.grants_revision
        {
            return stale_toy_authority_report(authorized, call, ids);
        }

        let started = toy_observation(
            ids.started_observation_id,
            ids.started_at,
            ObservationKind::ToyCallStarted,
            ObservationOutcome::Started,
            call,
            authorized,
            "toy call started",
        );

        let (outcome, output_json, safe_message, kind, observation_outcome) =
            match self.backends.get(authorized.toy_id()) {
                Some(MctToyBackend::EchoJson) => (
                    MctToyAdapterOutcome::Success,
                    Some(input_json.to_owned()),
                    "toy call completed".to_owned(),
                    ObservationKind::ToyCallCompleted,
                    ObservationOutcome::Completed,
                ),
                Some(MctToyBackend::StaticFailure { safe_message }) => (
                    MctToyAdapterOutcome::Failed,
                    None,
                    safe_message.clone(),
                    ObservationKind::ToyCallFailed,
                    ObservationOutcome::Failed,
                ),
                Some(MctToyBackend::GitCommand { repo_root }) => {
                    match call_git_toy(repo_root, input_json) {
                        Ok(output_json) => (
                            MctToyAdapterOutcome::Success,
                            Some(output_json),
                            "git toy call completed".to_owned(),
                            ObservationKind::ToyCallCompleted,
                            ObservationOutcome::Completed,
                        ),
                        Err(error) => (
                            MctToyAdapterOutcome::Failed,
                            None,
                            error.safe_message(),
                            ObservationKind::ToyCallFailed,
                            ObservationOutcome::Failed,
                        ),
                    }
                }
                None => (
                    MctToyAdapterOutcome::Failed,
                    None,
                    "toy backend unavailable".to_owned(),
                    ObservationKind::ToyCallFailed,
                    ObservationOutcome::Failed,
                ),
            };
        let completed = toy_observation(
            ids.completed_observation_id,
            ids.completed_at,
            kind,
            observation_outcome,
            call,
            authorized,
            &safe_message,
        );

        MctToyCallReport {
            outcome,
            output_json,
            safe_message,
            observations: vec![started, completed],
        }
    }
}

fn stale_toy_authority_report(
    authorized: &AuthorizedToyCall,
    call: &MctCall,
    ids: MctToyCallIds,
) -> MctToyCallReport {
    let observation = toy_observation(
        ids.started_observation_id,
        ids.started_at,
        ObservationKind::ToyCallFailed,
        ObservationOutcome::Denied,
        call,
        authorized,
        "toy call authority stale",
    );
    MctToyCallReport {
        outcome: MctToyAdapterOutcome::Failed,
        output_json: None,
        safe_message: "toy call authority stale".into(),
        observations: vec![observation],
    }
}

fn call_git_toy(repo_root: &Path, input_json: &str) -> Result<String, GitToyError> {
    if !repo_root.is_dir() {
        return Err(GitToyError::RepoRootNotDirectory {
            repo_root: repo_root.to_path_buf(),
        });
    }
    let input: Value =
        serde_json::from_str(input_json).map_err(|source| GitToyError::InputJson { source })?;
    let interface = required_str(&input, "interface")?;
    if interface != "patina:git/git@0.1.0" {
        return Err(GitToyError::UnsupportedInterface {
            interface: interface.to_owned(),
        });
    }
    let function = required_str(&input, "function")?;
    let output = match function {
        "create-tag" => {
            let name = required_git_ref_arg(&input, "name")?;
            let message = format!("mct git toy tag: {}", name.as_str());
            run_git(repo_root, &["tag", "-a", name.as_str(), "-m", &message])?;
            serde_json::json!({"ok": null})
        }
        "create-tag-at" => {
            let name = required_git_ref_arg(&input, "name")?;
            let git_ref = required_git_ref_arg(&input, "git_ref")?;
            let message = format!("mct git toy tag: {}", name.as_str());
            run_git(
                repo_root,
                &["tag", "-a", name.as_str(), "-m", &message, git_ref.as_str()],
            )?;
            serde_json::json!({"ok": null})
        }
        "delete-tag" => {
            let name = required_git_ref_arg(&input, "name")?;
            run_git(repo_root, &["tag", "-d", name.as_str()])?;
            serde_json::json!({"ok": null})
        }
        "tag-exists" => {
            let name = required_git_ref_arg(&input, "name")?;
            let tags = run_git_capture(repo_root, &["tag", "--list", name.as_str()])?;
            serde_json::json!({"ok": tags.lines().any(|line| line.trim() == name.as_str())})
        }
        "commit" => {
            let message = required_str(&input, "message")?;
            run_git(repo_root, &["commit", "-m", message])?;
            let sha = run_git_capture(repo_root, &["rev-parse", "HEAD"])?;
            serde_json::json!({"ok": sha.trim()})
        }
        "log-oneline" => {
            let limit = required_u32(&input, "limit")?;
            let limit = limit.to_string();
            let log = run_git_capture(repo_root, &["log", "--oneline", "--max-count", &limit])?;
            let lines: Vec<&str> = log.lines().filter(|line| !line.trim().is_empty()).collect();
            serde_json::json!({"ok": lines})
        }
        "diff-stat" => serde_json::json!({"ok": run_git_capture(repo_root, &["diff", "--stat"])?}),
        "status-porcelain" => {
            serde_json::json!({"ok": run_git_capture(repo_root, &["status", "--porcelain"])?})
        }
        "add-paths" => {
            let paths = required_paths(&input, "paths")?;
            let mut args = vec!["add".to_owned(), "--".to_owned()];
            args.extend(paths);
            run_git_owned(repo_root, &args)?;
            serde_json::json!({"ok": null})
        }
        "remove-paths" => {
            let paths = required_paths(&input, "paths")?;
            if !paths.is_empty() {
                let mut args = vec!["rm".to_owned(), "-rf".to_owned(), "--".to_owned()];
                args.extend(paths);
                run_git_owned(repo_root, &args)?;
            }
            serde_json::json!({"ok": null})
        }
        "is-clean-tracked" => {
            let status = run_git_capture(repo_root, &["status", "--porcelain", "-uno"])?;
            serde_json::json!({"ok": status.trim().is_empty()})
        }
        "commits-behind-upstream" => {
            let count = run_git_capture_allow_failure(
                repo_root,
                &["rev-list", "--count", "HEAD..@{upstream}"],
            )?;
            let count = count.trim().parse::<u32>().unwrap_or(0);
            serde_json::json!({"ok": count})
        }
        "is-diverged" => {
            let has_upstream =
                run_git(repo_root, &["rev-parse", "--abbrev-ref", "@{upstream}"]).is_ok();
            let diverged = if has_upstream {
                let ahead = run_git_capture_allow_failure(
                    repo_root,
                    &["rev-list", "--count", "@{upstream}..HEAD"],
                )?;
                let behind = run_git_capture_allow_failure(
                    repo_root,
                    &["rev-list", "--count", "HEAD..@{upstream}"],
                )?;
                ahead.trim().parse::<u32>().unwrap_or(0) > 0
                    && behind.trim().parse::<u32>().unwrap_or(0) > 0
            } else {
                false
            };
            serde_json::json!({"ok": diverged})
        }
        other => {
            return Err(GitToyError::UnsupportedFunction {
                function: other.to_owned(),
            });
        }
    };
    Ok(output.to_string())
}

#[derive(Debug, Error)]
enum GitToyError {
    #[error("git toy repo root '{repo_root}' is not a directory", repo_root = repo_root.display())]
    RepoRootNotDirectory { repo_root: PathBuf },
    #[error("git toy input must be JSON: {source}")]
    InputJson {
        #[source]
        source: serde_json::Error,
    },
    #[error("git toy received unsupported interface '{interface}'")]
    UnsupportedInterface { interface: String },
    #[error("unsupported git toy function '{function}'")]
    UnsupportedFunction { function: String },
    #[error("git toy input missing string field '{field}'")]
    MissingStringField { field: &'static str },
    #[error("git toy input missing u32 field '{field}'")]
    MissingU32Field { field: &'static str },
    #[error("git toy field '{field}' exceeds u32")]
    U32FieldOverflow { field: &'static str },
    #[error("git toy input missing path list field '{field}'")]
    MissingPathListField { field: &'static str },
    #[error("git toy path field '{field}' contains a non-string")]
    NonStringPath { field: &'static str },
    #[error("{source}")]
    InvalidPath {
        #[source]
        source: GitToyPathError,
    },
    #[error("{source}")]
    InvalidRef {
        #[source]
        source: GitRefArgError,
    },
    #[error("running git: {source}")]
    RunGit {
        #[source]
        source: std::io::Error,
    },
    #[error("{stderr}")]
    GitFailed { stderr: String },
}

impl GitToyError {
    fn safe_message(&self) -> String {
        self.to_string()
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
enum GitToyPathError {
    #[error("git toy path must not be empty")]
    Empty,
    #[error("git toy path must be repository-relative")]
    NotRelative,
    #[error("git toy path must not escape the repository")]
    EscapesRepository,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GitRefArg<'a> {
    value: &'a str,
}

impl<'a> GitRefArg<'a> {
    fn new(field: &'static str, value: &'a str) -> Result<Self, GitRefArgError> {
        if value.trim().is_empty() {
            return Err(GitRefArgError::Empty { field });
        }
        if value.starts_with('-') {
            return Err(GitRefArgError::LeadingDash { field });
        }
        Ok(Self { value })
    }

    fn as_str(&self) -> &'a str {
        self.value
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
enum GitRefArgError {
    #[error("git ref argument '{field}' must not be empty")]
    Empty { field: &'static str },
    #[error("git ref argument '{field}' must not start with '-'")]
    LeadingDash { field: &'static str },
}

fn required_git_ref_arg<'a>(
    input: &'a Value,
    field: &'static str,
) -> Result<GitRefArg<'a>, GitToyError> {
    let value = required_str(input, field)?;
    GitRefArg::new(field, value).map_err(|source| GitToyError::InvalidRef { source })
}

fn required_str<'a>(input: &'a Value, field: &'static str) -> Result<&'a str, GitToyError> {
    input
        .get(field)
        .and_then(Value::as_str)
        .ok_or(GitToyError::MissingStringField { field })
}

fn required_u32(input: &Value, field: &'static str) -> Result<u32, GitToyError> {
    let value = input
        .get(field)
        .and_then(Value::as_u64)
        .ok_or(GitToyError::MissingU32Field { field })?;
    u32::try_from(value).map_err(|_| GitToyError::U32FieldOverflow { field })
}

fn required_paths(input: &Value, field: &'static str) -> Result<Vec<String>, GitToyError> {
    let values = input
        .get(field)
        .and_then(Value::as_array)
        .ok_or(GitToyError::MissingPathListField { field })?;
    values
        .iter()
        .map(|value| {
            let path = value.as_str().ok_or(GitToyError::NonStringPath { field })?;
            validate_repo_relative_path(path)
                .map_err(|source| GitToyError::InvalidPath { source })?;
            Ok(path.to_owned())
        })
        .collect()
}

fn validate_repo_relative_path(path: &str) -> Result<(), GitToyPathError> {
    if path.trim().is_empty() {
        return Err(GitToyPathError::Empty);
    }
    let path = Path::new(path);
    if !path.is_relative() {
        return Err(GitToyPathError::NotRelative);
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(GitToyPathError::EscapesRepository);
            }
        }
    }
    Ok(())
}

fn run_git(repo_root: &Path, args: &[&str]) -> Result<(), GitToyError> {
    let output = git_command(repo_root)
        .args(args)
        .output()
        .map_err(|source| GitToyError::RunGit { source })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_stderr(&output))
    }
}

fn run_git_owned(repo_root: &Path, args: &[String]) -> Result<(), GitToyError> {
    let output = git_command(repo_root)
        .args(args)
        .output()
        .map_err(|source| GitToyError::RunGit { source })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_stderr(&output))
    }
}

fn run_git_capture(repo_root: &Path, args: &[&str]) -> Result<String, GitToyError> {
    let output = git_command(repo_root)
        .args(args)
        .output()
        .map_err(|source| GitToyError::RunGit { source })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(git_stderr(&output))
    }
}

fn run_git_capture_allow_failure(repo_root: &Path, args: &[&str]) -> Result<String, GitToyError> {
    let output = git_command(repo_root)
        .args(args)
        .output()
        .map_err(|source| GitToyError::RunGit { source })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Ok("0".into())
    }
}

fn git_command(repo_root: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(repo_root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_PREFIX");
    command
}

fn git_stderr(output: &std::process::Output) -> GitToyError {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    GitToyError::GitFailed {
        stderr: if stderr.is_empty() {
            "git command failed".into()
        } else {
            stderr
        },
    }
}

fn toy_observation(
    observation_id: ObservationId,
    observed_at: Timestamp,
    kind: ObservationKind,
    outcome: ObservationOutcome,
    call: &MctCall,
    authorized: &AuthorizedToyCall,
    safe_message: &str,
) -> MctObservation {
    MctObservation {
        observation_id,
        observed_at,
        kind,
        source_plane: SourcePlane::Toy,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: Some(authorized.authority_decision_id().clone()),
        subject_id: Some(authorized.child_instance_id().to_string()),
        resource_id: Some(authorized.toy_id().to_string()),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: Some(format!(
            "authorized_toy_call:{}",
            authorized.authorized_toy_call_id()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call() -> MctCall {
        MctCall {
            call_id: CallId::new("call-toy-adapter")
                .expect("string ID literal/generated value must be non-empty"),
            caller: CallerIdentity {
                node_id: MctNodeId::new("mother-a")
                    .expect("string ID literal/generated value must be non-empty"),
                user_id: None,
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                project_id: None,
            },
            target: OperationTarget {
                namespace: "patina".into(),
                interface_name: "echo".into(),
                function_name: "echo".into(),
            },
            payload_metadata: PayloadMetadata {
                data_classification: "public".into(),
                size_bytes: 11,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 1,
                grants_revision: 2,
                vision_policy_revision: 1,
            },
            deadline: Timestamp::new("2026-05-31T00:01:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-toy-adapter")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: SpanId::new("span-toy-adapter")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            origin: CallOrigin::Cli,
        }
    }

    fn authorized(toy_id: &str) -> AuthorizedToyCall {
        crate::authority_test_fixture::authorized_toy_for_call(
            &call(),
            toy_id,
            ChildInstanceId::new("instance-toy-adapter")
                .expect("string ID literal/generated value must be non-empty"),
            "use",
            "adapter",
        )
    }

    fn ids(stem: &str) -> MctToyCallIds {
        MctToyCallIds {
            started_observation_id: ObservationId::new(format!("obs-{stem}-started"))
                .expect("string ID literal/generated value must be non-empty"),
            completed_observation_id: ObservationId::new(format!("obs-{stem}-completed"))
                .expect("string ID literal/generated value must be non-empty"),
            started_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            completed_at: Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
        }
    }

    fn init_git_repo() -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        run_git_for_test(repo.path(), &["init"]);
        run_git_for_test(repo.path(), &["config", "user.name", "MCT Test"]);
        run_git_for_test(repo.path(), &["config", "user.email", "mct@example.com"]);
        std::fs::write(repo.path().join("README.md"), "mct\n").unwrap();
        run_git_for_test(repo.path(), &["add", "README.md"]);
        run_git_for_test(repo.path(), &["commit", "-m", "init"]);
        repo
    }

    fn run_git_for_test(repo_root: &Path, args: &[&str]) {
        let output = git_command(repo_root).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn toy_adapter_requires_authorized_toy_call_and_records_success() {
        let mut registry = MctToyAdapterRegistry::new();
        registry.register(
            ToyId::new("toy-echo").expect("string ID literal/generated value must be non-empty"),
            MctToyBackend::EchoJson,
        );

        let report = registry.call_authorized_toy(
            &authorized("toy-echo"),
            &call(),
            "{\"ok\":true}",
            ids("toy-success"),
        );

        assert_eq!(report.outcome, MctToyAdapterOutcome::Success);
        assert_eq!(report.output_json, Some("{\"ok\":true}".into()));
        assert_eq!(report.observations[0].kind, ObservationKind::ToyCallStarted);
        assert_eq!(
            report.observations[1].kind,
            ObservationKind::ToyCallCompleted
        );
        assert_eq!(report.observations[1].source_plane, SourcePlane::Toy);
        assert_eq!(
            report.observations[1].decision_id,
            Some(
                DecisionId::new("decision-toy-adapter")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
    }

    #[test]
    fn toy_adapter_denies_stale_toy_capability_before_backend_call() {
        let registry = MctToyAdapterRegistry::new();
        let mut stale_call = call();
        stale_call.authority_context.grants_revision += 1;

        let report = registry.call_authorized_toy(
            &authorized("toy-echo"),
            &stale_call,
            "{\"ok\":true}",
            ids("toy-stale"),
        );

        assert_eq!(report.outcome, MctToyAdapterOutcome::Failed);
        assert_eq!(report.safe_message, "toy call authority stale");
        assert_eq!(report.observations.len(), 1);
        assert_eq!(report.observations[0].outcome, ObservationOutcome::Denied);
        assert_eq!(report.observations[0].kind, ObservationKind::ToyCallFailed);
    }

    #[test]
    fn git_toy_backend_runs_in_configured_repo_and_records_success() {
        let repo = init_git_repo();
        let mut registry = MctToyAdapterRegistry::new();
        registry.register(
            ToyId::new("toy-git").expect("string ID literal/generated value must be non-empty"),
            MctToyBackend::GitCommand {
                repo_root: repo.path().to_path_buf(),
            },
        );

        let report = registry.call_authorized_toy(
            &authorized("toy-git"),
            &call(),
            r#"{"interface":"patina:git/git@0.1.0","function":"create-tag","name":"mct-toy-test"}"#,
            ids("toy-git-success"),
        );

        assert_eq!(report.outcome, MctToyAdapterOutcome::Success);
        assert_eq!(report.safe_message, "git toy call completed");
        assert_eq!(
            report.observations[1].kind,
            ObservationKind::ToyCallCompleted
        );
        let tags = run_git_capture(repo.path(), &["tag", "--list", "mct-toy-test"]).unwrap();
        assert!(tags.contains("mct-toy-test"));
    }

    fn git_report(repo: &Path, input_json: &str, stem: &str) -> MctToyCallReport {
        let mut registry = MctToyAdapterRegistry::new();
        registry.register(
            ToyId::new("toy-git").expect("string ID literal/generated value must be non-empty"),
            MctToyBackend::GitCommand {
                repo_root: repo.to_path_buf(),
            },
        );
        registry.call_authorized_toy(&authorized("toy-git"), &call(), input_json, ids(stem))
    }

    #[test]
    fn git_toy_rejects_option_like_untrusted_refs_before_invoking_git() {
        let repo = init_git_repo();
        let cases = [
            (
                "create-tag-name",
                r#"{"interface":"patina:git/git@0.1.0","function":"create-tag","name":"--force"}"#,
            ),
            (
                "create-tag-at-name",
                r#"{"interface":"patina:git/git@0.1.0","function":"create-tag-at","name":"--force","git_ref":"HEAD"}"#,
            ),
            (
                "create-tag-at-ref",
                r#"{"interface":"patina:git/git@0.1.0","function":"create-tag-at","name":"safe-tag","git_ref":"--all"}"#,
            ),
            (
                "delete-tag-name",
                r#"{"interface":"patina:git/git@0.1.0","function":"delete-tag","name":"--force"}"#,
            ),
            (
                "tag-exists-name",
                r#"{"interface":"patina:git/git@0.1.0","function":"tag-exists","name":"--force"}"#,
            ),
        ];

        for (stem, input_json) in cases {
            let report = git_report(repo.path(), input_json, stem);
            assert_eq!(report.outcome, MctToyAdapterOutcome::Failed, "{stem}");
            assert!(
                report.safe_message.contains("git ref argument"),
                "{stem}: {}",
                report.safe_message
            );
            assert_eq!(report.observations[1].kind, ObservationKind::ToyCallFailed);
        }
    }

    #[test]
    fn git_toy_ref_validation_remains_typed_before_safe_message_projection() {
        let repo = init_git_repo();

        let error = call_git_toy(
            repo.path(),
            r#"{"interface":"patina:git/git@0.1.0","function":"create-tag","name":"--force"}"#,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            GitToyError::InvalidRef {
                source: GitRefArgError::LeadingDash { field: "name" }
            }
        ));
    }

    #[test]
    fn git_toy_backend_rejects_paths_that_escape_repo() {
        let repo = init_git_repo();
        let mut registry = MctToyAdapterRegistry::new();
        registry.register(
            ToyId::new("toy-git").expect("string ID literal/generated value must be non-empty"),
            MctToyBackend::GitCommand {
                repo_root: repo.path().to_path_buf(),
            },
        );

        let report = registry.call_authorized_toy(
            &authorized("toy-git"),
            &call(),
            r#"{"interface":"patina:git/git@0.1.0","function":"add-paths","paths":["../outside"]}"#,
            ids("toy-git-reject"),
        );

        assert_eq!(report.outcome, MctToyAdapterOutcome::Failed);
        assert!(report.safe_message.contains("must not escape"));
        assert_eq!(report.observations[1].kind, ObservationKind::ToyCallFailed);
    }

    #[test]
    fn toy_backend_failure_is_adapter_observation_not_kernel_denial() {
        let registry = MctToyAdapterRegistry::new();

        let report = registry.call_authorized_toy(
            &authorized("missing-toy"),
            &call(),
            "{}",
            ids("toy-failed"),
        );

        assert_eq!(report.outcome, MctToyAdapterOutcome::Failed);
        assert_eq!(report.safe_message, "toy backend unavailable");
        assert_eq!(report.observations[1].kind, ObservationKind::ToyCallFailed);
        assert_eq!(report.observations[1].outcome, ObservationOutcome::Failed);
        assert_eq!(
            report.observations[1].call_id,
            Some(
                CallId::new("call-toy-adapter")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
    }
}

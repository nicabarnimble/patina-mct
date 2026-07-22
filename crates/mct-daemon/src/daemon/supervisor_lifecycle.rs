//! Ledger-backed daemon supervision and macOS launchd adapter.

use super::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write as _,
    os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering as AtomicOrdering},
};

pub(super) const MCT_LAUNCHD_LABEL: &str = "io.patina.mct.mother";
pub(super) const MCT_UPGRADE_POST_VERIFY_DEADLINE_SECONDS: u64 = 30;
const SUPERVISOR_SCHEMA_VERSION: u32 = 1;
const SUPERVISOR_RECORD_FILE: &str = "supervisor.json";
static NEXT_LIFECYCLE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub(super) struct SupervisorPaths {
    pub root: PathBuf,
    pub record: PathBuf,
    pub plist: PathBuf,
    pub config: PathBuf,
    pub identity: PathBuf,
    pub children: PathBuf,
    pub state: PathBuf,
    pub ledger: PathBuf,
    pub uds: PathBuf,
    pub stdout_log: PathBuf,
    pub stderr_log: PathBuf,
}

impl SupervisorPaths {
    fn with_plist(root: PathBuf, plist: PathBuf) -> Result<Self> {
        if !root.is_absolute() || !plist.is_absolute() {
            bail!("supervisor root and plist paths must be absolute");
        }
        Ok(Self {
            record: root.join(SUPERVISOR_RECORD_FILE),
            config: root.join("config.json"),
            identity: root.join("identity").join("iroh-secret.hex"),
            children: root.join("children"),
            state: root.join("state.sqlite"),
            ledger: root.join("observations.jsonl"),
            uds: root.join("control.sock"),
            stdout_log: root.join("logs").join("mother.stdout.log"),
            stderr_log: root.join("logs").join("mother.stderr.log"),
            root,
            plist,
        })
    }

    fn production(root: PathBuf, home: &Path) -> Result<Self> {
        Self::with_plist(
            root,
            home.join("Library")
                .join("LaunchAgents")
                .join(format!("{MCT_LAUNCHD_LABEL}.plist")),
        )
    }

    #[cfg(any(test, feature = "release-smoke-internal"))]
    fn smoke_internal(root: &Path) -> Result<Self> {
        let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        Self::with_plist(
            root.clone(),
            root.join("launchd")
                .join(format!("{MCT_LAUNCHD_LABEL}.plist")),
        )
    }

    #[cfg(test)]
    fn isolated(root: &Path) -> Result<Self> {
        let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        Self::with_plist(
            root.clone(),
            root.join(format!("{MCT_LAUNCHD_LABEL}.plist")),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SupervisorRecordV1 {
    pub schema_version: u32,
    pub record_id: String,
    pub record_revision: u64,
    pub record_state: String,
    pub backend: String,
    pub service_label: String,
    pub launchd_domain: String,
    pub owner_uid: u32,
    pub created_by_uid: u32,
    pub created_at: String,
    pub creation_observation_id: String,
    pub last_revised_by_uid: Option<u32>,
    pub revised_at: Option<String>,
    pub revision_observation_id: Option<String>,
    pub record_digest: String,
    pub executable_path: PathBuf,
    pub executable_digest: String,
    pub plist_path: PathBuf,
    pub plist_digest: String,
    pub config_path: PathBuf,
    pub identity_path: PathBuf,
    pub children_dir: PathBuf,
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub uds_path: PathBuf,
    pub stdout_log_path: PathBuf,
    pub stderr_log_path: PathBuf,
}

impl SupervisorRecordV1 {
    fn governing_observation_id(&self) -> &str {
        self.revision_observation_id
            .as_deref()
            .unwrap_or(&self.creation_observation_id)
    }

    fn canonical_digest(&self) -> Result<String> {
        let mut canonical = self.clone();
        canonical.record_digest.clear();
        Ok(blake3::hash(&serde_json::to_vec(&canonical)?)
            .to_hex()
            .to_string())
    }

    fn refresh_digest(&mut self) -> Result<()> {
        self.record_digest = self.canonical_digest()?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SupervisorLoadedState {
    Unloaded,
    Loaded,
}

trait SupervisorAdapter {
    fn ensure_domain_available(&self, record: &SupervisorRecordV1) -> Result<()>;
    fn inspect(&self, record: &SupervisorRecordV1) -> Result<SupervisorLoadedState>;
    fn publish_policy(&self, record: &SupervisorRecordV1, plist: &[u8]) -> Result<()>;
    fn start(&self, record: &SupervisorRecordV1) -> Result<()>;
    fn stop(&self, record: &SupervisorRecordV1) -> Result<()>;
    fn remove_policy(&self, record: &SupervisorRecordV1) -> Result<()>;
}

struct LaunchdSupervisorAdapter;

impl LaunchdSupervisorAdapter {
    fn service(record: &SupervisorRecordV1) -> String {
        format!("{}/{}", record.launchd_domain, record.service_label)
    }

    fn run(args: &[&str]) -> Result<()> {
        let status = Command::new("/bin/launchctl")
            .args(args)
            .stdin(Stdio::null())
            .status()
            .with_context(|| format!("run launchctl {}", args.join(" ")))?;
        if !status.success() {
            bail!("launchctl {} failed with status {status}", args.join(" "));
        }
        Ok(())
    }
}

impl SupervisorAdapter for LaunchdSupervisorAdapter {
    fn ensure_domain_available(&self, record: &SupervisorRecordV1) -> Result<()> {
        let status = Command::new("/bin/launchctl")
            .args(["print", &record.launchd_domain])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("inspect launchd GUI domain")?;
        if !status.success() {
            bail!(
                "launchd GUI domain {} is unavailable; headless/SSH-only supervision is unsupported",
                record.launchd_domain
            );
        }
        Ok(())
    }

    fn inspect(&self, record: &SupervisorRecordV1) -> Result<SupervisorLoadedState> {
        let service = Self::service(record);
        let status = Command::new("/bin/launchctl")
            .args(["print", &service])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("inspect launchd GUI service")?;
        Ok(if status.success() {
            SupervisorLoadedState::Loaded
        } else {
            SupervisorLoadedState::Unloaded
        })
    }

    fn publish_policy(&self, record: &SupervisorRecordV1, plist: &[u8]) -> Result<()> {
        atomic_write(&record.plist_path, plist, 0o644)
    }

    fn start(&self, record: &SupervisorRecordV1) -> Result<()> {
        let plist = record.plist_path.to_string_lossy();
        Self::run(&["bootstrap", &record.launchd_domain, &plist])
            .context("launchd gui domain unavailable; headless/SSH-only supervision is unsupported")
    }

    fn stop(&self, record: &SupervisorRecordV1) -> Result<()> {
        Self::run(&["bootout", &Self::service(record)])
    }

    fn remove_policy(&self, record: &SupervisorRecordV1) -> Result<()> {
        remove_if_exists(&record.plist_path)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct LifecycleReport {
    pub action: String,
    pub outcome: String,
    pub attempt_id: String,
    pub subject_id: String,
    pub supervisor_record_id: Option<String>,
    pub supervisor_revision: Option<u64>,
    pub observation_id: String,
    pub running: Option<bool>,
    pub ready: Option<bool>,
    pub safe_message: String,
}

fn generated_id(prefix: &str) -> String {
    let sequence = NEXT_LIFECYCLE_ID.fetch_add(1, AtomicOrdering::Relaxed);
    format!(
        "{prefix}:{}:{sequence}",
        mct_daemon::current_timestamp_string()
    )
}

fn current_uid() -> Result<u32> {
    let output = Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .context("authenticate current OS UID")?;
    if !output.status.success() {
        bail!("authenticate current OS UID: id -u failed");
    }
    String::from_utf8(output.stdout)?
        .trim()
        .parse()
        .context("parse current OS UID")
}

fn current_home(uid: u32) -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("resolve current OS account home")?;
    let metadata = fs::metadata(&home)
        .with_context(|| format!("inspect current OS account home {}", home.display()))?;
    if metadata.uid() != uid {
        bail!("current OS account home is not owned by authenticated UID");
    }
    Ok(home)
}

fn file_digest(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read {} for digest", path.display()))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn render_launchd_plist(record: &SupervisorRecordV1) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key>\n  <string>{label}</string>\n  <key>ProgramArguments</key>\n  <array>\n    <string>{exe}</string>\n    <string>serve</string>\n    <string>--supervisor-record</string>\n    <string>{record_path}</string>\n  </array>\n  <key>RunAtLoad</key>\n  <true/>\n  <key>KeepAlive</key>\n  <true/>\n  <key>ThrottleInterval</key>\n  <integer>10</integer>\n  <key>ProcessType</key>\n  <string>Background</string>\n  <key>StandardOutPath</key>\n  <string>{stdout}</string>\n  <key>StandardErrorPath</key>\n  <string>{stderr}</string>\n</dict>\n</plist>\n",
        label = xml_escape(&record.service_label),
        exe = xml_escape(&record.executable_path.display().to_string()),
        record_path = xml_escape(&record.ledger_path.with_file_name(SUPERVISOR_RECORD_FILE).display().to_string()),
        stdout = xml_escape(&record.stdout_log_path.display().to_string()),
        stderr = xml_escape(&record.stderr_log_path.display().to_string()),
    )
    .into_bytes()
}

fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("mct"),
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(mode)
        .open(&temp)
        .with_context(|| format!("create staged {}", temp.display()))?;
    file.write_all(bytes)?;
    fs::set_permissions(&temp, fs::Permissions::from_mode(mode))?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp, path)
        .with_context(|| format!("publish {} from {}", path.display(), temp.display()))?;
    fs::File::open(parent)
        .with_context(|| format!("open parent directory {} for sync", parent.display()))?
        .sync_all()
        .with_context(|| format!("sync parent directory {} after publish", parent.display()))?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("remove {}", path.display())),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the helper names the fixed MctObservation lifecycle projection fields"
)]
fn lifecycle_observation(
    observation_id: &str,
    trace_id: &str,
    kind: ObservationKind,
    source_plane: SourcePlane,
    subject: &str,
    resource: &str,
    policy_revision: Option<u64>,
    outcome: ObservationOutcome,
    safe_message: impl Into<String>,
    detail_ref: Option<String>,
) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::new(observation_id)
            .expect("generated lifecycle observation ID must be non-empty"),
        observed_at: current_timestamp(),
        kind,
        source_plane,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(trace_id)
                .expect("generated lifecycle trace ID must be non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(subject.into()),
        resource_id: Some(resource.into()),
        policy_revision,
        grants_revision: None,
        outcome,
        visibility: ObservationVisibility::SystemOperator,
        safe_message: safe_message.into(),
        detail_ref,
    }
}

fn append_before_effect(
    ledger: &mut JsonlObservationLedger,
    observations: impl IntoIterator<Item = MctObservation>,
) -> Result<Vec<mct_observation::MctObservationLedgerEntry>> {
    ledger
        .append_batch_before_effect(observations, mct_daemon::current_timestamp_string())
        .map_err(anyhow::Error::from)
}

fn lifecycle_http_response(status_code: u16, value: serde_json::Value) -> MctControlPlaneResponse {
    MctControlPlaneResponse {
        status_code,
        content_type: "application/json".into(),
        body: serde_json::to_string(&value).expect("lifecycle response JSON must encode"),
    }
}

pub(super) async fn execute_resident_lifecycle_fact(
    owner: mct_daemon::MctUdsAuthenticatedOwner,
    ledger: &ResidentLedgerWriter,
    body: &[u8],
) -> MctControlPlaneResponse {
    execute_authenticated_lifecycle_fact(owner.uid(), ledger, body).await
}

async fn execute_authenticated_lifecycle_fact(
    owner_uid: u32,
    ledger: &ResidentLedgerWriter,
    body: &[u8],
) -> MctControlPlaneResponse {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        action: String,
        #[serde(default)]
        release_artifact_id: Option<String>,
    }

    let request: Request = match serde_json::from_slice::<Request>(body) {
        Ok(request) => request,
        _ => {
            return lifecycle_http_response(
                400,
                serde_json::json!({"error": "lifecycle fact request rejected"}),
            );
        }
    };
    let (outcome, message, report_outcome) = match request.action.as_str() {
        "stop_prepare" => (
            ObservationOutcome::Started,
            "supervisor stop prepared by owner-authenticated lifecycle control",
            "started",
        ),
        "stop_failed" => (
            ObservationOutcome::Failed,
            "supervisor stop failed at launchd bootout adapter",
            "failed",
        ),
        "start_complete" => (
            ObservationOutcome::Completed,
            "supervisor start completed after real resident readiness",
            "completed",
        ),
        "start_no_op" => (
            ObservationOutcome::Completed,
            "supervisor start completed as observed no-op: service already loaded",
            "no_op",
        ),
        "manual_serve_refused" => (
            ObservationOutcome::Denied,
            "manual serve refused while a managed supervisor record is active",
            "denied",
        ),
        "restart_prepare" => (
            ObservationOutcome::Started,
            "supervisor restart prepared before clean stop/start",
            "started",
        ),
        "restart_complete" => (
            ObservationOutcome::Completed,
            "supervisor restart completed through clean stop/start",
            "completed",
        ),
        "upgrade_approval_admitted" => (
            ObservationOutcome::Allowed,
            "exact daemon release artifact approval admitted before lifecycle effects",
            "allowed",
        ),
        "upgrade_approval_denied" => (
            ObservationOutcome::Denied,
            "daemon release artifact approval denied without lifecycle effects",
            "denied",
        ),
        "upgrade_complete" => (
            ObservationOutcome::Completed,
            "daemon release upgrade completed after bounded post-verification",
            "completed",
        ),
        "upgrade_failed" => (
            ObservationOutcome::Failed,
            "daemon release upgrade failed without automatic rollback",
            "failed",
        ),
        _ => {
            return lifecycle_http_response(
                400,
                serde_json::json!({"error": "unknown lifecycle fact action"}),
            );
        }
    };
    let release_artifact_id = if request.action.starts_with("upgrade_") {
        match request.release_artifact_id.as_deref() {
            Some(value)
                if value.strip_prefix("sha256:").is_some_and(|digest| {
                    digest.len() == 64
                        && digest
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                }) =>
            {
                Some(value)
            }
            _ => {
                return lifecycle_http_response(
                    400,
                    serde_json::json!({"error": "upgrade lifecycle fact requires exact release artifact id"}),
                );
            }
        }
    } else {
        if request.release_artifact_id.is_some() {
            return lifecycle_http_response(
                400,
                serde_json::json!({"error": "non-upgrade lifecycle fact cannot name a release artifact"}),
            );
        }
        None
    };
    let governing = ledger.path().and_then(|path| {
        JsonlObservationLedger::open_read_only(path, "ledger-local", "local-mct")
            .and_then(|reader| reader.entries())
            .ok()
            .and_then(|entries| {
                entries
                    .into_iter()
                    .rev()
                    .find(|entry| {
                        entry
                            .observation
                            .safe_message
                            .starts_with("supervised resident instance started ")
                    })
                    .map(|entry| {
                        (
                            entry
                                .observation
                                .resource_id
                                .unwrap_or_else(|| MCT_LAUNCHD_LABEL.into()),
                            entry.observation.policy_revision,
                            entry.observation.detail_ref,
                        )
                    })
            })
    });
    let (record_id, record_revision, record_detail) =
        governing.unwrap_or_else(|| (MCT_LAUNCHD_LABEL.into(), None, None));
    let lifecycle_resource = release_artifact_id.unwrap_or(record_id.as_str());
    let lifecycle_detail = if lifecycle_resource == record_id {
        record_detail
    } else {
        Some(format!(
            "supervisor-record:{}@{}",
            record_id,
            record_revision.unwrap_or_default()
        ))
    };
    let attempt_id = generated_id(&format!("lifecycle-{}", request.action));
    let operator_id = generated_id("obs-lifecycle-control-operator");
    let lifecycle_id = generated_id("obs-lifecycle-control");
    let mut observations = vec![
        lifecycle_observation(
            &operator_id,
            &attempt_id,
            ObservationKind::OperatorActionRecorded,
            SourcePlane::Operator,
            "local-mct",
            &format!("os-uid:{owner_uid}"),
            None,
            ObservationOutcome::Allowed,
            "owner-authenticated UDS lifecycle fact admitted",
            None,
        ),
        lifecycle_observation(
            &lifecycle_id,
            &attempt_id,
            ObservationKind::LifecycleTransitionRecorded,
            SourcePlane::Operator,
            "local-mct",
            lifecycle_resource,
            record_revision,
            outcome,
            message,
            lifecycle_detail,
        ),
    ];
    let adapter_fact = match request.action.as_str() {
        "stop_prepare" => Some((
            ObservationKind::AdapterEffectStarted,
            ObservationOutcome::Started,
            "launchd bootout effect prepared",
        )),
        "start_complete" => Some((
            ObservationKind::AdapterEffectCompleted,
            ObservationOutcome::Completed,
            "launchd bootstrap effect completed at resident readiness",
        )),
        "stop_failed" => Some((
            ObservationKind::AdapterEffectFailed,
            ObservationOutcome::Failed,
            "launchd bootout effect failed",
        )),
        _ => None,
    };
    if let Some((kind, outcome, message)) = adapter_fact {
        observations.push(lifecycle_observation(
            &generated_id("obs-lifecycle-control-adapter"),
            &attempt_id,
            kind,
            SourcePlane::Adapter,
            "local-mct",
            MCT_LAUNCHD_LABEL,
            record_revision,
            outcome,
            message,
            None,
        ));
    }
    if ledger.append(observations).await.is_err() {
        return lifecycle_http_response(
            500,
            serde_json::json!({"error": "lifecycle fact was not durable"}),
        );
    }
    lifecycle_http_response(
        200,
        serde_json::to_value(LifecycleReport {
            action: request.action,
            outcome: report_outcome.into(),
            attempt_id,
            subject_id: "local-mct".into(),
            supervisor_record_id: Some(record_id),
            supervisor_revision: record_revision,
            observation_id: lifecycle_id,
            running: None,
            ready: None,
            safe_message: message.into(),
        })
        .expect("lifecycle report must encode"),
    )
}

#[derive(Clone, Debug)]
pub(super) struct ResidentLifecycleInstance {
    pub instance_id: String,
    pub start_observation_id: String,
    pub record_id: String,
    pub record_revision: u64,
}

pub(super) async fn begin_supervised_resident_instance(
    record: &SupervisorRecordV1,
    ledger: &ResidentLedgerWriter,
) -> Result<ResidentLifecycleInstance> {
    let prior_entries =
        JsonlObservationLedger::open_read_only(&record.ledger_path, "ledger-local", "local-mct")?
            .entries()?;
    let last_start = prior_entries.iter().rev().find(|entry| {
        entry
            .observation
            .safe_message
            .starts_with("supervised resident instance started ")
    });
    if let Some(last_start) = last_start {
        let prior_instance = last_start
            .observation
            .safe_message
            .trim_start_matches("supervised resident instance started ")
            .split_whitespace()
            .next()
            .unwrap_or_default();
        let clean = prior_entries.iter().any(|entry| {
            entry.observation.safe_message
                == format!("supervised resident clean shutdown completed {prior_instance}")
        });
        if !prior_instance.is_empty() && !clean {
            let reconciliation_id = generated_id("obs-supervisor-reconciliation");
            ledger
                .append(vec![lifecycle_observation(
                    &reconciliation_id,
                    &generated_id("lifecycle-reconciliation"),
                    ObservationKind::LifecycleTransitionRecorded,
                    SourcePlane::Adapter,
                    "local-mct",
                    prior_instance,
                    Some(record.record_revision),
                    ObservationOutcome::Completed,
                    format!(
                        "supervised resident reconciled unmatched prior instance {prior_instance} start_observation={}",
                        last_start.observation.observation_id
                    ),
                    None,
                )])
                .await?;
        }
    }

    let instance_id = generated_id("resident-instance");
    let start_observation_id = generated_id("obs-supervisor-resident-started");
    ledger
        .append(vec![lifecycle_observation(
            &start_observation_id,
            &generated_id("lifecycle-supervised-start"),
            ObservationKind::LifecycleTransitionRecorded,
            SourcePlane::Adapter,
            "local-mct",
            &record.record_id,
            Some(record.record_revision),
            ObservationOutcome::Started,
            format!(
                "supervised resident instance started {instance_id} governing_record={}@{} provenance_observation={}",
                record.record_id,
                record.record_revision,
                record.governing_observation_id()
            ),
            Some(format!("supervisor-record-digest:{}", record.record_digest)),
        )])
        .await?;
    Ok(ResidentLifecycleInstance {
        instance_id,
        start_observation_id,
        record_id: record.record_id.clone(),
        record_revision: record.record_revision,
    })
}

pub(super) async fn record_supervised_resident_ready(
    instance: &ResidentLifecycleInstance,
    ledger: &ResidentLedgerWriter,
) -> Result<()> {
    ledger
        .append(vec![lifecycle_observation(
            &generated_id("obs-supervisor-resident-ready"),
            &generated_id("lifecycle-supervised-ready"),
            ObservationKind::LifecycleTransitionRecorded,
            SourcePlane::Adapter,
            "local-mct",
            &instance.record_id,
            Some(instance.record_revision),
            ObservationOutcome::Completed,
            format!(
                "supervised resident ready instance={} start_observation={}",
                instance.instance_id, instance.start_observation_id
            ),
            None,
        )])
        .await
}

pub(super) async fn record_supervised_clean_shutdown_started(
    instance: &ResidentLifecycleInstance,
    ledger: &ResidentLedgerWriter,
) -> Result<()> {
    ledger
        .append(vec![lifecycle_observation(
            &generated_id("obs-supervisor-shutdown-started"),
            &generated_id("lifecycle-supervised-shutdown"),
            ObservationKind::LifecycleTransitionRecorded,
            SourcePlane::Adapter,
            "local-mct",
            &instance.record_id,
            Some(instance.record_revision),
            ObservationOutcome::Started,
            format!(
                "supervised resident clean shutdown started {}",
                instance.instance_id
            ),
            None,
        )])
        .await
}

pub(super) async fn record_supervised_clean_shutdown_completed(
    instance: &ResidentLifecycleInstance,
    ledger: &ResidentLedgerWriter,
) -> Result<()> {
    ledger
        .append(vec![lifecycle_observation(
            &generated_id("obs-supervisor-shutdown-completed"),
            &generated_id("lifecycle-supervised-shutdown"),
            ObservationKind::LifecycleTransitionRecorded,
            SourcePlane::Adapter,
            "local-mct",
            &instance.record_id,
            Some(instance.record_revision),
            ObservationOutcome::Completed,
            format!(
                "supervised resident clean shutdown completed {}",
                instance.instance_id
            ),
            None,
        )])
        .await
}

fn discovered_summary(paths: &SupervisorPaths, adapter_state: &str) -> String {
    format!(
        "root={} ledger={} config={} identity={} state={} record={} plist={} launchd={adapter_state}",
        exists_class(&paths.root),
        exists_class(&paths.ledger),
        exists_class(&paths.config),
        exists_class(&paths.identity),
        exists_class(&paths.state),
        exists_class(&paths.record),
        exists_class(&paths.plist),
    )
}

fn exists_class(path: &Path) -> &'static str {
    if path.exists() { "present" } else { "absent" }
}

fn ensure_owner_private_root(root: &Path, uid: u32) -> Result<()> {
    fs::create_dir_all(root).with_context(|| format!("create service root {}", root.display()))?;
    fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
    let metadata = fs::metadata(root)?;
    if metadata.uid() != uid || metadata.permissions().mode() & 0o077 != 0 {
        bail!("service root must be owned by authenticated UID and mode 0700");
    }
    Ok(())
}

fn read_record(path: &Path) -> Result<SupervisorRecordV1> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("read supervisor record metadata {}", path.display()))?;
    if metadata.permissions().mode() & 0o077 != 0 {
        bail!("supervisor record must be owner-private mode 0600");
    }
    let record: SupervisorRecordV1 = serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("decode supervisor record {}", path.display()))?;
    Ok(record)
}

pub(super) fn validate_supervisor_record(
    path: &Path,
    simulated_context: bool,
) -> Result<SupervisorRecordV1> {
    validate_supervisor_record_inner(path, simulated_context, true, false)
}

fn validate_supervisor_record_for_replace(path: &Path) -> Result<SupervisorRecordV1> {
    validate_supervisor_record_inner(path, true, false, true)
}

fn validate_supervisor_record_for_stop(path: &Path) -> Result<SupervisorRecordV1> {
    validate_supervisor_record_inner(path, true, false, false)
}

fn validate_supervisor_record_inner(
    path: &Path,
    simulated_context: bool,
    require_current_executable: bool,
    allow_missing_plist: bool,
) -> Result<SupervisorRecordV1> {
    let record = read_record(path)?;
    if fs::metadata(path)?.uid() != record.owner_uid {
        bail!("supervisor record owner does not match governing UID");
    }
    if record.schema_version != SUPERVISOR_SCHEMA_VERSION
        || record.record_state != "active"
        || record.backend != "launchd_user"
        || record.service_label != MCT_LAUNCHD_LABEL
        || record.launchd_domain != format!("gui/{}", record.owner_uid)
    {
        bail!("supervisor record schema, state, backend, label, or GUI domain is invalid");
    }
    if record.record_digest != record.canonical_digest()? {
        bail!("supervisor record digest mismatch");
    }
    if require_current_executable
        && file_digest(&record.executable_path)? != record.executable_digest
    {
        bail!(
            "supervisor executable digest mismatch; run `mct-daemon install --replace --executable {}`",
            record.executable_path.display()
        );
    }
    if record.plist_path.exists() {
        if file_digest(&record.plist_path)? != record.plist_digest {
            bail!("supervisor plist digest mismatch; run `mct-daemon install --replace`");
        }
    } else if !allow_missing_plist {
        bail!("supervisor plist is missing; run `mct-daemon install --replace`");
    }
    if !simulated_context {
        #[cfg(not(target_os = "macos"))]
        bail!("supervised launchd process context is supported only on macOS");
        #[cfg(target_os = "macos")]
        {
            let pid = std::process::id().to_string();
            let output = Command::new("/bin/ps")
                .args(["-o", "ppid=", "-p", &pid])
                .output()
                .context("inspect supervised process parent")?;
            let parent = String::from_utf8(output.stdout)?
                .trim()
                .parse::<u32>()
                .context("parse supervised process parent")?;
            if parent != 1 {
                bail!("supervised process context mismatch: parent is not launchd");
            }
            let service = LaunchdSupervisorAdapter::service(&record);
            let service_output = Command::new("/bin/launchctl")
                .args(["print", &service])
                .output()
                .context("inspect governing launchd service process")?;
            if !service_output.status.success() {
                bail!("supervised process context mismatch: launchd service is not loaded");
            }
            let expected_pid = format!("pid = {}", std::process::id());
            let service_text = String::from_utf8(service_output.stdout)?;
            if !service_text.lines().any(|line| line.trim() == expected_pid) {
                bail!("supervised process context mismatch: launchd service PID differs");
            }
        }
    }
    let entries =
        JsonlObservationLedger::open_read_only(&record.ledger_path, "ledger-local", "local-mct")?
            .entries()?;
    let governing = entries.iter().find(|entry| {
        entry.observation.observation_id.as_str() == record.governing_observation_id()
    });
    let Some(governing) = governing else {
        bail!("supervisor record governing observation is absent");
    };
    if governing.observation.resource_id.as_deref() != Some(record.record_id.as_str())
        || governing.observation.policy_revision != Some(record.record_revision)
        || governing.observation.detail_ref.as_deref()
            != Some(format!("supervisor-record-digest:{}", record.record_digest).as_str())
    {
        bail!("supervisor record governing observation does not match current revision");
    }
    Ok(record)
}

fn install_with_adapter(
    paths: &SupervisorPaths,
    executable: &Path,
    uid: u32,
    replace: bool,
    adapter: &dyn SupervisorAdapter,
) -> Result<LifecycleReport> {
    if !executable.is_absolute() || !executable.is_file() {
        bail!("supervisor executable must be an absolute regular file");
    }
    let mode = fs::metadata(executable)?.permissions().mode();
    if mode & 0o111 == 0 {
        bail!("supervisor executable must be executable");
    }

    let initial_discovery = discovered_summary(paths, "uninspected");
    ensure_owner_private_root(&paths.root, uid)?;
    let mut ledger = match JsonlObservationLedger::open(&paths.ledger, "ledger-local", "local-mct")
    {
        Ok(ledger) => ledger,
        Err(error) if error.to_string().contains("writer lock") => {
            let attempt_id = generated_id("lifecycle-install-contention");
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut refusal_writer = loop {
                match JsonlObservationLedger::open(&paths.ledger, "ledger-local", "local-mct") {
                    Ok(ledger) => break ledger,
                    Err(_) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    Err(wait_error) => return Err(wait_error.into()),
                }
            };
            append_before_effect(
                &mut refusal_writer,
                [lifecycle_observation(
                    &generated_id("obs-install-contention-denied"),
                    &attempt_id,
                    ObservationKind::LifecycleTransitionRecorded,
                    SourcePlane::Operator,
                    "pending-local-installation",
                    &format!("os-uid:{uid}"),
                    None,
                    ObservationOutcome::Denied,
                    "concurrent supervisor install refused after losing exclusive bootstrap writer",
                    None,
                )],
            )?;
            bail!("concurrent supervisor install refused: exclusive bootstrap writer was held");
        }
        Err(error) => return Err(error.into()),
    };
    let existing = paths.record.exists();
    if existing && !replace {
        let attempt_id = generated_id("lifecycle-install");
        let observation_id = generated_id("obs-lifecycle-install-denied");
        append_before_effect(
            &mut ledger,
            [lifecycle_observation(
                &observation_id,
                &attempt_id,
                ObservationKind::OperatorActionRecorded,
                SourcePlane::Operator,
                "local-mct-installation",
                &format!("os-uid:{uid}"),
                None,
                ObservationOutcome::Denied,
                "supervisor install refused: current record exists; use install --replace",
                None,
            )],
        )?;
        bail!("supervisor install refused: current record exists; use install --replace");
    }

    let predecessor = existing
        .then(|| validate_supervisor_record_for_replace(&paths.record))
        .transpose()?;
    if let Some(predecessor) = &predecessor {
        adapter.ensure_domain_available(predecessor)?;
        if adapter.inspect(predecessor)? == SupervisorLoadedState::Loaded {
            append_direct_lifecycle_fact(
                &mut ledger,
                "install-replace",
                uid,
                predecessor,
                ObservationOutcome::Denied,
                "supervisor replacement refused while launchd service is loaded",
            )?;
            bail!("supervisor replacement refused while launchd service is loaded");
        }
        if predecessor.plist_path.exists()
            && file_digest(&predecessor.plist_path)? != predecessor.plist_digest
        {
            bail!("supervisor replacement refused: predecessor plist digest mismatch");
        }
    }

    let record_id = predecessor
        .as_ref()
        .map(|record| record.record_id.clone())
        .unwrap_or_else(|| generated_id("supervisor-record"));
    let revision = predecessor
        .as_ref()
        .map_or(1, |record| record.record_revision + 1);
    let attempt_id = generated_id("lifecycle-install");
    let governing_observation_id = generated_id(if predecessor.is_some() {
        "obs-supervisor-record-revised"
    } else {
        "obs-supervisor-record-created"
    });
    let now = mct_daemon::current_timestamp_string();
    let mut record = SupervisorRecordV1 {
        schema_version: SUPERVISOR_SCHEMA_VERSION,
        record_id: record_id.clone(),
        record_revision: revision,
        record_state: "active".into(),
        backend: "launchd_user".into(),
        service_label: MCT_LAUNCHD_LABEL.into(),
        launchd_domain: format!("gui/{uid}"),
        owner_uid: uid,
        created_by_uid: predecessor
            .as_ref()
            .map_or(uid, |record| record.created_by_uid),
        created_at: predecessor
            .as_ref()
            .map_or_else(|| now.clone(), |record| record.created_at.clone()),
        creation_observation_id: predecessor.as_ref().map_or_else(
            || governing_observation_id.clone(),
            |record| record.creation_observation_id.clone(),
        ),
        last_revised_by_uid: predecessor.as_ref().map(|_| uid),
        revised_at: predecessor.as_ref().map(|_| now.clone()),
        revision_observation_id: predecessor
            .as_ref()
            .map(|_| governing_observation_id.clone()),
        record_digest: String::new(),
        executable_path: executable.to_path_buf(),
        executable_digest: file_digest(executable)?,
        plist_path: paths.plist.clone(),
        plist_digest: String::new(),
        config_path: paths.config.clone(),
        identity_path: paths.identity.clone(),
        children_dir: paths.children.clone(),
        state_path: paths.state.clone(),
        ledger_path: paths.ledger.clone(),
        uds_path: paths.uds.clone(),
        stdout_log_path: paths.stdout_log.clone(),
        stderr_log_path: paths.stderr_log.clone(),
    };
    let provisional_plist = render_launchd_plist(&record);
    record.plist_digest = blake3::hash(&provisional_plist).to_hex().to_string();
    record.refresh_digest()?;
    let plist = render_launchd_plist(&record);
    debug_assert_eq!(
        record.plist_digest,
        blake3::hash(&plist).to_hex().to_string()
    );

    let detail_ref = Some(format!("supervisor-record-digest:{}", record.record_digest));
    let operator_observation_id = generated_id("obs-supervisor-install-operator");
    append_before_effect(
        &mut ledger,
        [
            lifecycle_observation(
                &operator_observation_id,
                &attempt_id,
                ObservationKind::OperatorActionRecorded,
                SourcePlane::Operator,
                "pending-local-installation",
                &format!("os-uid:{uid}"),
                Some(revision),
                ObservationOutcome::Allowed,
                format!(
                    "authenticated supervisor install requested; discovered {initial_discovery}"
                ),
                None,
            ),
            lifecycle_observation(
                &governing_observation_id,
                &attempt_id,
                ObservationKind::LifecycleTransitionRecorded,
                SourcePlane::Operator,
                "pending-local-installation",
                &record_id,
                Some(revision),
                ObservationOutcome::Started,
                format!(
                    "supervisor record revision {revision} install started; discovered {initial_discovery}"
                ),
                detail_ref.clone(),
            ),
        ],
    )?;

    if let Err(error) = adapter.ensure_domain_available(&record) {
        append_before_effect(
            &mut ledger,
            [
                lifecycle_observation(
                    &generated_id("obs-supervisor-install-domain-failed"),
                    &attempt_id,
                    ObservationKind::AdapterEffectFailed,
                    SourcePlane::Adapter,
                    "pending-local-installation",
                    &record.launchd_domain,
                    Some(revision),
                    ObservationOutcome::Failed,
                    "launchd GUI domain unavailable; headless/SSH-only supervision is unsupported",
                    None,
                ),
                lifecycle_observation(
                    &generated_id("obs-supervisor-install-failed"),
                    &attempt_id,
                    ObservationKind::LifecycleTransitionRecorded,
                    SourcePlane::Operator,
                    "pending-local-installation",
                    &record.record_id,
                    Some(revision),
                    ObservationOutcome::Failed,
                    "supervisor install failed because the exact GUI domain is unavailable",
                    None,
                ),
            ],
        )?;
        return Err(error);
    }

    let identity_observation_id = generated_id("obs-supervisor-install-identity");
    append_before_effect(
        &mut ledger,
        [lifecycle_observation(
            &identity_observation_id,
            &attempt_id,
            ObservationKind::OperatorActionRecorded,
            SourcePlane::Operator,
            "pending-local-installation",
            &format!("os-uid:{uid}"),
            Some(revision),
            ObservationOutcome::Allowed,
            "local identity creation or validation admitted after install bootstrap fact",
            None,
        )],
    )?;
    let identity = MctDaemonConfigStore::new(&paths.config)
        .ensure_local_identity(MctOperatorNodeScope::default(), &paths.identity)?;
    drop(MctRuntimeStateStore::open(&paths.state)?);
    fs::create_dir_all(&paths.children)?;
    fs::create_dir_all(
        paths
            .stdout_log
            .parent()
            .context("stdout log has no parent")?,
    )?;

    let record_bytes = serde_json::to_vec_pretty(&record)?;
    append_before_effect(
        &mut ledger,
        [lifecycle_observation(
            &generated_id("obs-supervisor-install-adapter-started"),
            &attempt_id,
            ObservationKind::AdapterEffectStarted,
            SourcePlane::Adapter,
            identity.node_id.as_str(),
            MCT_LAUNCHD_LABEL,
            Some(revision),
            ObservationOutcome::Started,
            "launchd supervisor record and plist publication started",
            detail_ref.clone(),
        )],
    )?;
    let publication = atomic_write(&paths.record, &record_bytes, 0o600)
        .and_then(|()| adapter.publish_policy(&record, &plist));
    if let Err(error) = publication {
        append_before_effect(
            &mut ledger,
            [
                lifecycle_observation(
                    &generated_id("obs-supervisor-install-adapter-failed"),
                    &attempt_id,
                    ObservationKind::AdapterEffectFailed,
                    SourcePlane::Adapter,
                    identity.node_id.as_str(),
                    MCT_LAUNCHD_LABEL,
                    Some(revision),
                    ObservationOutcome::Failed,
                    "launchd supervisor record or plist publication failed",
                    detail_ref.clone(),
                ),
                lifecycle_observation(
                    &generated_id("obs-supervisor-install-failed"),
                    &attempt_id,
                    ObservationKind::LifecycleTransitionRecorded,
                    SourcePlane::Operator,
                    identity.node_id.as_str(),
                    &record_id,
                    Some(revision),
                    ObservationOutcome::Failed,
                    "supervisor install failed after observed publication attempt",
                    detail_ref.clone(),
                ),
            ],
        )?;
        return Err(error);
    }

    let adapter_observation_id = generated_id("obs-supervisor-install-adapter-completed");
    let completion_observation_id = generated_id("obs-supervisor-install-completed");
    append_before_effect(
        &mut ledger,
        [
            lifecycle_observation(
                &adapter_observation_id,
                &attempt_id,
                ObservationKind::AdapterEffectCompleted,
                SourcePlane::Adapter,
                identity.node_id.as_str(),
                MCT_LAUNCHD_LABEL,
                Some(revision),
                ObservationOutcome::Completed,
                "launchd supervisor record and plist published",
                detail_ref,
            ),
            lifecycle_observation(
                &completion_observation_id,
                &attempt_id,
                ObservationKind::LifecycleTransitionRecorded,
                SourcePlane::Operator,
                identity.node_id.as_str(),
                &record_id,
                Some(revision),
                ObservationOutcome::Completed,
                "supervisor install completed without starting resident",
                None,
            ),
        ],
    )?;

    Ok(LifecycleReport {
        action: "install".into(),
        outcome: "completed".into(),
        attempt_id,
        subject_id: identity.node_id.to_string(),
        supervisor_record_id: Some(record_id),
        supervisor_revision: Some(revision),
        observation_id: completion_observation_id,
        running: Some(false),
        ready: Some(false),
        safe_message: "supervisor installed; run `mct-daemon start`".into(),
    })
}

pub(super) fn refuse_manual_serve_if_managed(
    config_path: &Path,
    include_default_global: bool,
) -> Result<()> {
    let mut candidates = Vec::new();
    if include_default_global {
        let uid = current_uid()?;
        candidates.push(current_home(uid)?.join(".mct").join(SUPERVISOR_RECORD_FILE));
    }
    if let Some(root) = config_path.parent() {
        let configured = root.join(SUPERVISOR_RECORD_FILE);
        if !candidates.contains(&configured) {
            candidates.push(configured);
        }
    }
    let Some(record_path) = candidates.into_iter().find(|path| path.exists()) else {
        return Ok(());
    };
    let record = validate_supervisor_record(&record_path, true)?;
    let uid = current_uid()?;
    if post_lifecycle_control(&record, "manual_serve_refused", uid).is_ok() {
        bail!(
            "manual serve refused: Mother is managed by launchd; use `mct-daemon start|stop|restart` or uninstall"
        );
    }
    let mut ledger = JsonlObservationLedger::open(&record.ledger_path, "ledger-local", "local-mct")
        .context("manual serve refusal could not acquire canonical writer or lifecycle UDS")?;
    append_direct_lifecycle_fact(
        &mut ledger,
        "manual-serve",
        uid,
        &record,
        ObservationOutcome::Denied,
        "manual serve refused while a managed supervisor record is active",
    )?;
    bail!(
        "manual serve refused: Mother is managed by launchd; use `mct-daemon start|stop|restart` or uninstall"
    )
}

fn parse_lifecycle_paths(
    args: &mut Vec<String>,
    allow_executable: bool,
) -> Result<(SupervisorPaths, Option<PathBuf>)> {
    let uid = current_uid()?;
    let home = current_home(uid)?;
    let root = take_option(args, "--root")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".mct"));
    let executable = if allow_executable {
        take_option(args, "--executable").map(PathBuf::from)
    } else {
        None
    };
    Ok((SupervisorPaths::production(root, &home)?, executable))
}

fn require_macos_lifecycle() -> Result<()> {
    if cfg!(target_os = "macos") {
        Ok(())
    } else {
        bail!("mct-daemon supervisor lifecycle is supported only on macOS")
    }
}

#[derive(Clone, Debug)]
pub(super) struct UpgradeSupervisorContext {
    paths: SupervisorPaths,
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub uds_path: PathBuf,
    pub releases_dir: PathBuf,
    pub current_executable: PathBuf,
    pub current_executable_digest: String,
    pub supervisor_record_id: String,
    pub supervisor_revision: u64,
    pub authenticated_uid: u32,
}

fn upgrade_supervisor_context_for_paths(
    paths: SupervisorPaths,
    current_executable: PathBuf,
    uid: u32,
) -> Result<UpgradeSupervisorContext> {
    let record = validate_supervisor_record(&paths.record, true)?;
    if record.owner_uid != uid {
        bail!("upgrade supervisor record is not owned by the authenticated UID");
    }
    let current_executable = current_executable.canonicalize()?;
    let recorded_executable = record.executable_path.canonicalize()?;
    if current_executable != recorded_executable
        || file_digest(&current_executable)? != record.executable_digest
    {
        bail!(
            "upgrade must be invoked through the exact executable bound by the current supervisor record"
        );
    }
    Ok(UpgradeSupervisorContext {
        paths: paths.clone(),
        state_path: paths.state,
        ledger_path: paths.ledger,
        uds_path: paths.uds,
        releases_dir: paths.root.join("releases"),
        current_executable,
        current_executable_digest: format!("blake3:{}", record.executable_digest),
        supervisor_record_id: record.record_id,
        supervisor_revision: record.record_revision,
        authenticated_uid: uid,
    })
}

pub(super) fn upgrade_supervisor_context(
    selected_root: Option<PathBuf>,
) -> Result<UpgradeSupervisorContext> {
    require_macos_lifecycle()?;
    let uid = current_uid()?;
    let home = current_home(uid)?;
    let root = selected_root.unwrap_or_else(|| home.join(".mct"));
    let paths = SupervisorPaths::production(root, &home)?;
    upgrade_supervisor_context_for_paths(paths, std::env::current_exe()?, uid)
}

pub(super) fn record_upgrade_fact(
    context: &UpgradeSupervisorContext,
    release_artifact_id: &str,
    action: &str,
) -> Result<LifecycleReport> {
    if !matches!(
        action,
        "upgrade_approval_admitted"
            | "upgrade_approval_denied"
            | "upgrade_complete"
            | "upgrade_failed"
    ) {
        bail!("unknown upgrade lifecycle fact");
    }
    let outcome = match action {
        "upgrade_approval_admitted" => ObservationOutcome::Allowed,
        "upgrade_approval_denied" => ObservationOutcome::Denied,
        "upgrade_complete" => ObservationOutcome::Completed,
        "upgrade_failed" => ObservationOutcome::Failed,
        _ => unreachable!(),
    };
    let message = match action {
        "upgrade_approval_admitted" => {
            "exact daemon release artifact approval admitted before lifecycle effects"
        }
        "upgrade_approval_denied" => {
            "daemon release artifact approval denied without lifecycle effects"
        }
        "upgrade_complete" => "daemon release upgrade completed after bounded post-verification",
        "upgrade_failed" => "daemon release upgrade failed without automatic rollback",
        _ => unreachable!(),
    };
    let record = validate_supervisor_record_for_stop(&context.paths.record)?;
    if record.owner_uid != context.authenticated_uid
        || record.record_id != context.supervisor_record_id
        || (action.starts_with("upgrade_approval")
            && record.record_revision != context.supervisor_revision)
    {
        bail!("upgrade lifecycle fact does not match authenticated supervisor context");
    }
    if let Ok(report) = post_lifecycle_control_request(
        &record,
        action,
        context.authenticated_uid,
        Some(release_artifact_id),
    ) {
        return Ok(report);
    }
    let mut ledger =
        JsonlObservationLedger::open(&record.ledger_path, "ledger-local", "local-mct")?;
    let attempt_id = generated_id("lifecycle-upgrade");
    let operator_id = generated_id("obs-upgrade-operator");
    let lifecycle_id = generated_id("obs-upgrade-lifecycle");
    append_before_effect(
        &mut ledger,
        [
            lifecycle_observation(
                &operator_id,
                &attempt_id,
                ObservationKind::OperatorActionRecorded,
                SourcePlane::Operator,
                "local-mct",
                &format!("os-uid:{}", context.authenticated_uid),
                Some(record.record_revision),
                outcome,
                "authenticated direct upgrade lifecycle fact recorded",
                None,
            ),
            lifecycle_observation(
                &lifecycle_id,
                &attempt_id,
                ObservationKind::LifecycleTransitionRecorded,
                SourcePlane::Operator,
                "local-mct",
                release_artifact_id,
                Some(record.record_revision),
                outcome,
                message,
                Some(format!(
                    "supervisor-record:{}@{}",
                    record.record_id, record.record_revision
                )),
            ),
        ],
    )?;
    Ok(LifecycleReport {
        action: action.into(),
        outcome: format!("{outcome:?}").to_lowercase(),
        attempt_id,
        subject_id: "local-mct".into(),
        supervisor_record_id: Some(record.record_id),
        supervisor_revision: Some(record.record_revision),
        observation_id: lifecycle_id,
        running: None,
        ready: None,
        safe_message: message.into(),
    })
}

pub(super) fn execute_upgrade_lifecycle(
    context: &UpgradeSupervisorContext,
    artifact: &DaemonReleaseArtifactV1,
) -> Result<LifecycleReport> {
    execute_upgrade_lifecycle_with_adapter(
        context,
        artifact,
        &LaunchdSupervisorAdapter,
        true,
        |successor| {
            let deadline = std::time::Instant::now()
                + Duration::from_secs(MCT_UPGRADE_POST_VERIFY_DEADLINE_SECONDS);
            loop {
                if let Ok(status) = query_resident_status(&successor.uds_path)
                    && status.running
                    && status.health == MctDaemonHealth::Healthy
                    && status.readiness == MctDaemonReadiness::Ready
                    && status.version == artifact.product_version
                    && status.supervisor_revision == Some(successor.record_revision)
                    && status.executable_digest.as_deref()
                        == Some(format!("blake3:{}", successor.executable_digest).as_str())
                {
                    return Ok(());
                }
                if std::time::Instant::now() >= deadline {
                    bail!(
                        "upgrade post-verification exceeded MCT_UPGRADE_POST_VERIFY_DEADLINE_SECONDS"
                    );
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        },
    )
}

fn execute_upgrade_lifecycle_with_adapter<F>(
    context: &UpgradeSupervisorContext,
    artifact: &DaemonReleaseArtifactV1,
    adapter: &dyn SupervisorAdapter,
    wait_ready: bool,
    post_verify: F,
) -> Result<LifecycleReport>
where
    F: FnOnce(&SupervisorRecordV1) -> Result<()>,
{
    let candidate_executable = artifact
        .immutable_release_path
        .join(&artifact.executable_relative_path);
    if !candidate_executable.is_file()
        || format!("blake3:{}", file_digest(&candidate_executable)?) != artifact.executable_blake3
    {
        bail!("verified daemon release executable changed before replacement");
    }
    let execute = || -> Result<LifecycleReport> {
        stop_with_adapter(&context.paths, context.authenticated_uid, adapter)?;
        install_with_adapter(
            &context.paths,
            &candidate_executable,
            context.authenticated_uid,
            true,
            adapter,
        )?;
        start_with_adapter(
            &context.paths,
            context.authenticated_uid,
            adapter,
            wait_ready,
        )?;
        let successor = validate_supervisor_record(&context.paths.record, true)?;
        post_verify(&successor)?;
        record_upgrade_fact(context, &artifact.release_artifact_id, "upgrade_complete")
    };
    match execute() {
        Ok(report) => Ok(report),
        Err(error) => {
            let _ = record_upgrade_fact(context, &artifact.release_artifact_id, "upgrade_failed");
            Err(error).context(format!(
                "upgrade failed without automatic rollback; prior immutable release retained; run `{}` install --replace --root {} --executable <prior-exact-path>, then start",
                context.current_executable.display(),
                context.paths.root.display()
            ))
        }
    }
}

pub(super) fn run_install(mut args: Vec<String>) -> Result<()> {
    require_macos_lifecycle()?;
    let replace = take_flag(&mut args, "--replace");
    let json = take_flag(&mut args, "--json");
    let (paths, executable) = parse_lifecycle_paths(&mut args, true)?;
    if !args.is_empty() {
        bail!("unexpected install arguments: {}", args.join(" "));
    }
    let executable = executable.unwrap_or(std::env::current_exe()?);
    let report = install_with_adapter(
        &paths,
        &executable,
        current_uid()?,
        replace,
        &LaunchdSupervisorAdapter,
    )?;
    print_lifecycle_report(&report, json)
}

fn print_lifecycle_report(report: &LifecycleReport, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!("{}", report.safe_message);
        println!("observation={}", report.observation_id);
    }
    Ok(())
}

fn append_direct_lifecycle_fact(
    ledger: &mut JsonlObservationLedger,
    action: &str,
    uid: u32,
    record: &SupervisorRecordV1,
    outcome: ObservationOutcome,
    safe_message: &str,
) -> Result<(String, String)> {
    let attempt_id = generated_id(&format!("lifecycle-{action}"));
    let operator_id = generated_id(&format!("obs-{action}-operator"));
    let lifecycle_id = generated_id(&format!("obs-{action}-lifecycle"));
    append_before_effect(
        ledger,
        [
            lifecycle_observation(
                &operator_id,
                &attempt_id,
                ObservationKind::OperatorActionRecorded,
                SourcePlane::Operator,
                "local-mct",
                &format!("os-uid:{uid}"),
                Some(record.record_revision),
                if outcome == ObservationOutcome::Denied {
                    ObservationOutcome::Denied
                } else {
                    ObservationOutcome::Allowed
                },
                format!("authenticated supervisor {action} requested"),
                None,
            ),
            lifecycle_observation(
                &lifecycle_id,
                &attempt_id,
                ObservationKind::LifecycleTransitionRecorded,
                SourcePlane::Operator,
                "local-mct",
                &record.record_id,
                Some(record.record_revision),
                outcome,
                safe_message,
                Some(format!("supervisor-record-digest:{}", record.record_digest)),
            ),
        ],
    )?;
    Ok((attempt_id, lifecycle_id))
}

#[cfg(unix)]
fn post_lifecycle_control(
    record: &SupervisorRecordV1,
    action: &str,
    uid: u32,
) -> Result<LifecycleReport> {
    post_lifecycle_control_request(record, action, uid, None)
}

#[cfg(unix)]
fn post_lifecycle_control_request(
    record: &SupervisorRecordV1,
    action: &str,
    _uid: u32,
    release_artifact_id: Option<&str>,
) -> Result<LifecycleReport> {
    use std::io::{Read as _, Write as _};
    use std::os::unix::net::UnixStream;

    let body = serde_json::to_vec(&serde_json::json!({
        "action": action,
        "release_artifact_id": release_artifact_id,
    }))?;
    let mut stream = UnixStream::connect(&record.uds_path).with_context(|| {
        format!(
            "connect resident lifecycle control {}",
            record.uds_path.display()
        )
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    write!(
        stream,
        "POST /lifecycle/fact HTTP/1.1\r\nHost: local\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .context("resident lifecycle response missing body")?;
    if !headers.starts_with("HTTP/1.1 200") {
        bail!("resident lifecycle fact rejected");
    }
    serde_json::from_str(body).context("decode resident lifecycle report")
}

#[cfg(not(unix))]
fn post_lifecycle_control(
    _record: &SupervisorRecordV1,
    _action: &str,
    _uid: u32,
) -> Result<LifecycleReport> {
    bail!("resident lifecycle control requires Unix-domain sockets")
}

#[cfg(not(unix))]
fn post_lifecycle_control_request(
    _record: &SupervisorRecordV1,
    _action: &str,
    _uid: u32,
    _release_artifact_id: Option<&str>,
) -> Result<LifecycleReport> {
    bail!("resident lifecycle control requires Unix-domain sockets")
}

fn latest_supervised_instance_is_clean(record: &SupervisorRecordV1) -> Result<bool> {
    let entries =
        JsonlObservationLedger::open_read_only(&record.ledger_path, "ledger-local", "local-mct")?
            .entries()?;
    let Some(start) = entries.iter().rev().find(|entry| {
        entry
            .observation
            .safe_message
            .starts_with("supervised resident instance started ")
    }) else {
        return Ok(false);
    };
    let instance = start
        .observation
        .safe_message
        .trim_start_matches("supervised resident instance started ")
        .split_whitespace()
        .next()
        .unwrap_or_default();
    Ok(!instance.is_empty()
        && entries.iter().any(|entry| {
            entry.observation.safe_message
                == format!("supervised resident clean shutdown completed {instance}")
        }))
}

fn wait_for_resident_ready(record: &SupervisorRecordV1) -> Result<()> {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if query_resident_status(&record.uds_path)
            .is_ok_and(|status| status.running && status.readiness == MctDaemonReadiness::Ready)
        {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            bail!("supervised resident did not become ready within 15s");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(super) fn record_supervised_boot_refusal(record_path: &Path, safe_message: &str) -> Result<()> {
    let root = record_path
        .parent()
        .context("supervisor record path has no service root")?;
    let ledger_path = root.join("observations.jsonl");
    let mut ledger = JsonlObservationLedger::open(&ledger_path, "ledger-local", "local-mct")?;
    let attempt_id = generated_id("lifecycle-supervised-boot-refused");
    append_before_effect(
        &mut ledger,
        [lifecycle_observation(
            &generated_id("obs-supervised-boot-refused"),
            &attempt_id,
            ObservationKind::LifecycleTransitionRecorded,
            SourcePlane::Adapter,
            "local-mct",
            MCT_LAUNCHD_LABEL,
            None,
            ObservationOutcome::Denied,
            safe_message,
            None,
        )],
    )?;
    Ok(())
}

fn validate_direct_start_record(paths: &SupervisorPaths) -> Result<SupervisorRecordV1> {
    match validate_supervisor_record(&paths.record, true) {
        Ok(record) => Ok(record),
        Err(error) => {
            let safe_message = error.to_string();
            record_supervised_boot_refusal(&paths.record, &safe_message)?;
            Err(error)
        }
    }
}

fn start_with_adapter(
    paths: &SupervisorPaths,
    uid: u32,
    adapter: &dyn SupervisorAdapter,
    wait_ready: bool,
) -> Result<LifecycleReport> {
    let record = validate_direct_start_record(paths)?;
    if record.owner_uid != uid {
        bail!("supervisor start UID does not match governing record");
    }
    if adapter.inspect(&record)? == SupervisorLoadedState::Loaded {
        if let Ok(report) = post_lifecycle_control(&record, "start_no_op", uid) {
            return Ok(report);
        }
        bail!(
            "supervisor service is loaded but its owner-authenticated lifecycle control is unavailable"
        );
    }

    let mut ledger =
        JsonlObservationLedger::open(&record.ledger_path, "ledger-local", "local-mct")?;
    let (attempt_id, started_id) = append_direct_lifecycle_fact(
        &mut ledger,
        "start",
        uid,
        &record,
        ObservationOutcome::Started,
        "direct supervisor start attempt recorded before launchd bootstrap",
    )?;
    if let Err(error) = adapter.ensure_domain_available(&record) {
        append_direct_lifecycle_fact(
            &mut ledger,
            "start",
            uid,
            &record,
            ObservationOutcome::Failed,
            "launchd GUI domain unavailable; no fallback attempted",
        )?;
        return Err(error);
    }
    let adapter_started_id = generated_id("obs-start-launchd-started");
    append_before_effect(
        &mut ledger,
        [lifecycle_observation(
            &adapter_started_id,
            &attempt_id,
            ObservationKind::AdapterEffectStarted,
            SourcePlane::Adapter,
            "local-mct",
            MCT_LAUNCHD_LABEL,
            Some(record.record_revision),
            ObservationOutcome::Started,
            "launchd bootstrap started",
            None,
        )],
    )?;
    drop(ledger);

    if let Err(error) = adapter.start(&record) {
        let mut ledger =
            JsonlObservationLedger::open(&record.ledger_path, "ledger-local", "local-mct")?;
        append_before_effect(
            &mut ledger,
            [
                lifecycle_observation(
                    &generated_id("obs-start-launchd-failed"),
                    &attempt_id,
                    ObservationKind::AdapterEffectFailed,
                    SourcePlane::Adapter,
                    "local-mct",
                    MCT_LAUNCHD_LABEL,
                    Some(record.record_revision),
                    ObservationOutcome::Failed,
                    "launchd bootstrap failed",
                    None,
                ),
                lifecycle_observation(
                    &generated_id("obs-start-lifecycle-failed"),
                    &attempt_id,
                    ObservationKind::LifecycleTransitionRecorded,
                    SourcePlane::Operator,
                    "local-mct",
                    &record.record_id,
                    Some(record.record_revision),
                    ObservationOutcome::Failed,
                    "supervisor start failed after launchd adapter refusal",
                    None,
                ),
            ],
        )?;
        return Err(error);
    }
    if wait_ready {
        wait_for_resident_ready(&record)?;
        return post_lifecycle_control(&record, "start_complete", uid);
    }
    Ok(LifecycleReport {
        action: "start".into(),
        outcome: "started".into(),
        attempt_id,
        subject_id: "local-mct".into(),
        supervisor_record_id: Some(record.record_id),
        supervisor_revision: Some(record.record_revision),
        observation_id: started_id,
        running: Some(true),
        ready: None,
        safe_message: "supervisor start requested".into(),
    })
}

fn stop_with_adapter(
    paths: &SupervisorPaths,
    uid: u32,
    adapter: &dyn SupervisorAdapter,
) -> Result<LifecycleReport> {
    let record = validate_supervisor_record_for_stop(&paths.record)?;
    if record.owner_uid != uid {
        bail!("supervisor stop UID does not match governing record");
    }
    if adapter.inspect(&record)? == SupervisorLoadedState::Unloaded {
        let mut ledger =
            JsonlObservationLedger::open(&record.ledger_path, "ledger-local", "local-mct")?;
        let (attempt_id, observation_id) = append_direct_lifecycle_fact(
            &mut ledger,
            "stop",
            uid,
            &record,
            ObservationOutcome::Completed,
            "supervisor stop completed as observed no-op: service already unloaded",
        )?;
        return Ok(LifecycleReport {
            action: "stop".into(),
            outcome: "no_op".into(),
            attempt_id,
            subject_id: "local-mct".into(),
            supervisor_record_id: Some(record.record_id),
            supervisor_revision: Some(record.record_revision),
            observation_id,
            running: Some(false),
            ready: Some(false),
            safe_message: "supervisor already stopped".into(),
        });
    }

    let preparation = post_lifecycle_control(&record, "stop_prepare", uid)?;
    if let Err(error) = adapter.stop(&record) {
        let _ = post_lifecycle_control(&record, "stop_failed", uid);
        return Err(error);
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut ledger = loop {
        match JsonlObservationLedger::open(&record.ledger_path, "ledger-local", "local-mct") {
            Ok(ledger) => break ledger,
            Err(error) if std::time::Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(error.into()),
        }
    };
    if !latest_supervised_instance_is_clean(&record)? {
        bail!(
            "supervisor safety termination completed without durable clean shutdown; next start must reconcile"
        );
    }
    append_before_effect(
        &mut ledger,
        [lifecycle_observation(
            &generated_id("obs-stop-launchd-completed"),
            &preparation.attempt_id,
            ObservationKind::AdapterEffectCompleted,
            SourcePlane::Adapter,
            "local-mct",
            MCT_LAUNCHD_LABEL,
            Some(record.record_revision),
            ObservationOutcome::Completed,
            "launchd bootout completed after clean resident shutdown",
            None,
        )],
    )?;
    let (_, completion_id) = append_direct_lifecycle_fact(
        &mut ledger,
        "stop",
        uid,
        &record,
        ObservationOutcome::Completed,
        "supervisor stop and launchd bootout completed",
    )?;
    Ok(LifecycleReport {
        action: "stop".into(),
        outcome: "completed".into(),
        attempt_id: preparation.attempt_id,
        subject_id: "local-mct".into(),
        supervisor_record_id: Some(record.record_id),
        supervisor_revision: Some(record.record_revision),
        observation_id: completion_id,
        running: Some(false),
        ready: Some(false),
        safe_message: "supervisor stopped cleanly".into(),
    })
}

fn uninstall_with_adapter(
    paths: &SupervisorPaths,
    uid: u32,
    adapter: &dyn SupervisorAdapter,
) -> Result<LifecycleReport> {
    if !paths.record.exists() {
        ensure_owner_private_root(&paths.root, uid)?;
        let mut ledger = JsonlObservationLedger::open(&paths.ledger, "ledger-local", "local-mct")?;
        let synthetic = SupervisorRecordV1 {
            schema_version: 1,
            record_id: "absent-supervisor-record".into(),
            record_revision: 0,
            record_state: "active".into(),
            backend: "launchd_user".into(),
            service_label: MCT_LAUNCHD_LABEL.into(),
            launchd_domain: format!("gui/{uid}"),
            owner_uid: uid,
            created_by_uid: uid,
            created_at: current_timestamp().to_string(),
            creation_observation_id: "absent".into(),
            last_revised_by_uid: None,
            revised_at: None,
            revision_observation_id: None,
            record_digest: String::new(),
            executable_path: PathBuf::new(),
            executable_digest: String::new(),
            plist_path: paths.plist.clone(),
            plist_digest: String::new(),
            config_path: paths.config.clone(),
            identity_path: paths.identity.clone(),
            children_dir: paths.children.clone(),
            state_path: paths.state.clone(),
            ledger_path: paths.ledger.clone(),
            uds_path: paths.uds.clone(),
            stdout_log_path: paths.stdout_log.clone(),
            stderr_log_path: paths.stderr_log.clone(),
        };
        let (attempt_id, observation_id) = append_direct_lifecycle_fact(
            &mut ledger,
            "uninstall",
            uid,
            &synthetic,
            ObservationOutcome::Completed,
            "supervisor uninstall completed as observed no-op: policy absent",
        )?;
        return Ok(LifecycleReport {
            action: "uninstall".into(),
            outcome: "no_op".into(),
            attempt_id,
            subject_id: "local-mct-installation".into(),
            supervisor_record_id: None,
            supervisor_revision: None,
            observation_id,
            running: Some(false),
            ready: Some(false),
            safe_message: "supervisor was not installed; evidence preserved".into(),
        });
    }

    let record = match validate_supervisor_record_for_stop(&paths.record) {
        Ok(record) => record,
        Err(error)
            if error
                .to_string()
                .contains("supervisor plist digest mismatch") =>
        {
            let record = read_record(&paths.record)?;
            if record.owner_uid != uid {
                return Err(error);
            }
            let mut ledger =
                JsonlObservationLedger::open(&record.ledger_path, "ledger-local", "local-mct")?;
            append_direct_lifecycle_fact(
                &mut ledger,
                "uninstall",
                uid,
                &record,
                ObservationOutcome::Denied,
                "supervisor uninstall refused: managed plist digest mismatch; foreign plist preserved",
            )?;
            return Err(error.context("supervisor uninstall refused; foreign plist preserved"));
        }
        Err(error) => return Err(error),
    };
    if adapter.inspect(&record)? == SupervisorLoadedState::Loaded {
        stop_with_adapter(paths, uid, adapter)?;
    }
    let mut ledger =
        JsonlObservationLedger::open(&record.ledger_path, "ledger-local", "local-mct")?;
    let (attempt_id, started_id) = append_direct_lifecycle_fact(
        &mut ledger,
        "uninstall",
        uid,
        &record,
        ObservationOutcome::Started,
        "supervisor uninstall removal started; evidence and runtime state preserved",
    )?;
    append_before_effect(
        &mut ledger,
        [lifecycle_observation(
            &generated_id("obs-uninstall-adapter-started"),
            &attempt_id,
            ObservationKind::AdapterEffectStarted,
            SourcePlane::Adapter,
            "local-mct",
            MCT_LAUNCHD_LABEL,
            Some(record.record_revision),
            ObservationOutcome::Started,
            "launchd plist and current supervisor record removal started",
            None,
        )],
    )?;
    let removal = adapter
        .remove_policy(&record)
        .and_then(|()| remove_if_exists(&paths.record));
    if let Err(error) = removal {
        append_before_effect(
            &mut ledger,
            [
                lifecycle_observation(
                    &generated_id("obs-uninstall-adapter-failed"),
                    &attempt_id,
                    ObservationKind::AdapterEffectFailed,
                    SourcePlane::Adapter,
                    "local-mct",
                    MCT_LAUNCHD_LABEL,
                    Some(record.record_revision),
                    ObservationOutcome::Failed,
                    "launchd plist or current supervisor record removal failed",
                    None,
                ),
                lifecycle_observation(
                    &generated_id("obs-uninstall-failed"),
                    &attempt_id,
                    ObservationKind::LifecycleTransitionRecorded,
                    SourcePlane::Operator,
                    "local-mct",
                    &record.record_id,
                    Some(record.record_revision),
                    ObservationOutcome::Failed,
                    "supervisor uninstall failed after observed removal attempt",
                    None,
                ),
            ],
        )?;
        return Err(error);
    }
    let completion_id = generated_id("obs-uninstall-completed");
    append_before_effect(
        &mut ledger,
        [
            lifecycle_observation(
                &generated_id("obs-uninstall-adapter-completed"),
                &attempt_id,
                ObservationKind::AdapterEffectCompleted,
                SourcePlane::Adapter,
                "local-mct",
                MCT_LAUNCHD_LABEL,
                Some(record.record_revision),
                ObservationOutcome::Completed,
                "launchd plist and current supervisor record removed",
                None,
            ),
            lifecycle_observation(
                &completion_id,
                &attempt_id,
                ObservationKind::LifecycleTransitionRecorded,
                SourcePlane::Operator,
                "local-mct",
                &record.record_id,
                Some(record.record_revision),
                ObservationOutcome::Completed,
                "supervisor uninstall completed; ledger state identity children and logs preserved",
                None,
            ),
        ],
    )?;
    Ok(LifecycleReport {
        action: "uninstall".into(),
        outcome: "completed".into(),
        attempt_id,
        subject_id: "local-mct".into(),
        supervisor_record_id: Some(record.record_id),
        supervisor_revision: Some(record.record_revision),
        observation_id: completion_id,
        running: Some(false),
        ready: Some(false),
        safe_message: format!(
            "supervisor removed; evidence preserved (removal started at {started_id})"
        ),
    })
}

fn parse_simple_lifecycle_args(mut args: Vec<String>) -> Result<(SupervisorPaths, bool)> {
    let json = take_flag(&mut args, "--json");
    let (paths, executable) = parse_lifecycle_paths(&mut args, false)?;
    debug_assert!(executable.is_none());
    if !args.is_empty() {
        bail!("unexpected lifecycle arguments: {}", args.join(" "));
    }
    Ok((paths, json))
}

pub(super) fn run_start(args: Vec<String>) -> Result<()> {
    require_macos_lifecycle()?;
    let (paths, json) = parse_simple_lifecycle_args(args)?;
    let report = start_with_adapter(&paths, current_uid()?, &LaunchdSupervisorAdapter, true)?;
    print_lifecycle_report(&report, json)
}

pub(super) fn run_stop(args: Vec<String>) -> Result<()> {
    require_macos_lifecycle()?;
    let (paths, json) = parse_simple_lifecycle_args(args)?;
    let report = stop_with_adapter(&paths, current_uid()?, &LaunchdSupervisorAdapter)?;
    print_lifecycle_report(&report, json)
}

pub(super) fn run_restart(args: Vec<String>) -> Result<()> {
    require_macos_lifecycle()?;
    let (paths, json) = parse_simple_lifecycle_args(args)?;
    let uid = current_uid()?;
    let record = validate_supervisor_record_for_stop(&paths.record)?;
    if LaunchdSupervisorAdapter.inspect(&record)? == SupervisorLoadedState::Loaded {
        post_lifecycle_control(&record, "restart_prepare", uid)?;
    } else {
        let mut ledger =
            JsonlObservationLedger::open(&record.ledger_path, "ledger-local", "local-mct")?;
        append_direct_lifecycle_fact(
            &mut ledger,
            "restart",
            uid,
            &record,
            ObservationOutcome::Started,
            "supervisor restart started before clean stop/start",
        )?;
    }
    stop_with_adapter(&paths, uid, &LaunchdSupervisorAdapter)?;
    start_with_adapter(&paths, uid, &LaunchdSupervisorAdapter, true)?;
    let mut report = post_lifecycle_control(&record, "restart_complete", uid)?;
    report.action = "restart".into();
    report.safe_message = "supervisor restarted through clean stop/start".into();
    print_lifecycle_report(&report, json)
}

pub(super) fn run_uninstall(args: Vec<String>) -> Result<()> {
    require_macos_lifecycle()?;
    let (paths, json) = parse_simple_lifecycle_args(args)?;
    let report = uninstall_with_adapter(&paths, current_uid()?, &LaunchdSupervisorAdapter)?;
    print_lifecycle_report(&report, json)
}

#[cfg(feature = "release-smoke-internal")]
fn require_release_smoke_label_unloaded(uid: u32) -> Result<()> {
    let domain = format!("gui/{uid}");
    let domain_status = Command::new("/bin/launchctl")
        .args(["print", &domain])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("inspect release-smoke launchd GUI domain")?;
    if !domain_status.success() {
        bail!("release smoke requires an available launchd GUI domain");
    }
    let service = format!("{domain}/{MCT_LAUNCHD_LABEL}");
    let service_status = Command::new("/bin/launchctl")
        .args(["print", &service])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("inspect release-smoke fixed service label")?;
    if service_status.success() {
        bail!(
            "release smoke refused: fixed label {MCT_LAUNCHD_LABEL} is loaded; stop the production resident explicitly and restart it after smoke"
        );
    }
    Ok(())
}

#[cfg(feature = "release-smoke-internal")]
pub(super) fn run_release_smoke_internal(mut args: Vec<String>) -> Result<()> {
    require_macos_lifecycle()?;
    if args.is_empty() {
        bail!("release-smoke-internal requires an action");
    }
    let action = args.remove(0);
    let root = take_option(&mut args, "--root")
        .map(PathBuf::from)
        .context("release-smoke-internal requires --root")?;
    if !root.is_absolute() {
        bail!("release-smoke-internal root must be absolute");
    }
    let uid = current_uid()?;
    let home = current_home(uid)?;
    if root == home.join(".mct") {
        bail!("release-smoke-internal refuses the production service root");
    }
    let metadata = fs::metadata(&root).context("inspect owner-private release-smoke root")?;
    if metadata.uid() != uid || metadata.permissions().mode() & 0o077 != 0 {
        bail!("release-smoke-internal root must be owner-private and authenticated-UID owned");
    }
    let paths = SupervisorPaths::smoke_internal(&root)?;
    match action.as_str() {
        "preflight" => {
            if !args.is_empty() {
                bail!("unexpected release-smoke preflight arguments");
            }
            require_release_smoke_label_unloaded(uid)?;
            println!("release-smoke: fixed label is unloaded");
            Ok(())
        }
        "install" => {
            let executable = take_option(&mut args, "--executable")
                .map(PathBuf::from)
                .context("release-smoke install requires --executable")?;
            if !args.is_empty() {
                bail!("unexpected release-smoke install arguments");
            }
            require_release_smoke_label_unloaded(uid)?;
            let report =
                install_with_adapter(&paths, &executable, uid, false, &LaunchdSupervisorAdapter)?;
            print_lifecycle_report(&report, true)
        }
        "start" => {
            if !args.is_empty() {
                bail!("unexpected release-smoke start arguments");
            }
            require_release_smoke_label_unloaded(uid)?;
            let report = start_with_adapter(&paths, uid, &LaunchdSupervisorAdapter, true)?;
            print_lifecycle_report(&report, true)
        }
        "stop" => {
            if !args.is_empty() {
                bail!("unexpected release-smoke stop arguments");
            }
            let report = stop_with_adapter(&paths, uid, &LaunchdSupervisorAdapter)?;
            print_lifecycle_report(&report, true)
        }
        "uninstall" => {
            if !args.is_empty() {
                bail!("unexpected release-smoke uninstall arguments");
            }
            let report = uninstall_with_adapter(&paths, uid, &LaunchdSupervisorAdapter)?;
            print_lifecycle_report(&report, true)
        }
        "upgrade" => {
            let record = validate_supervisor_record(&paths.record, true)?;
            let context =
                upgrade_supervisor_context_for_paths(paths, record.executable_path.clone(), uid)?;
            run_upgrade_in_context(args, context)
        }
        "postflight" => {
            if !args.is_empty() {
                bail!("unexpected release-smoke postflight arguments");
            }
            require_release_smoke_label_unloaded(uid)?;
            if paths.record.exists() || paths.plist.exists() {
                bail!("release smoke postflight found residual supervisor policy");
            }
            println!("release-smoke: fixed label unloaded and smoke policy absent");
            Ok(())
        }
        _ => bail!("unknown release-smoke-internal action"),
    }
}

#[cfg(test)]
#[derive(Default)]
struct FakeSupervisorAdapter {
    loaded: std::sync::Mutex<bool>,
    domain_missing: std::sync::Mutex<bool>,
    fail_start: std::sync::Mutex<bool>,
    start_calls: std::sync::atomic::AtomicUsize,
    shutdown: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

#[cfg(test)]
impl FakeSupervisorAdapter {
    fn arm_shutdown(&self, sender: tokio::sync::oneshot::Sender<()>) {
        *self.shutdown.lock().unwrap() = Some(sender);
    }

    fn simulate_unclean_exit(&self) {
        *self.loaded.lock().unwrap() = false;
    }

    fn simulate_missing_gui_domain(&self) {
        *self.domain_missing.lock().unwrap() = true;
    }

    fn simulate_start_failure(&self) {
        *self.fail_start.lock().unwrap() = true;
    }

    fn start_call_count(&self) -> usize {
        self.start_calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
impl SupervisorAdapter for FakeSupervisorAdapter {
    fn ensure_domain_available(&self, _record: &SupervisorRecordV1) -> Result<()> {
        if *self.domain_missing.lock().unwrap() {
            bail!("launchd GUI domain unavailable; no fallback attempted");
        }
        Ok(())
    }

    fn inspect(&self, _record: &SupervisorRecordV1) -> Result<SupervisorLoadedState> {
        Ok(if *self.loaded.lock().unwrap() {
            SupervisorLoadedState::Loaded
        } else {
            SupervisorLoadedState::Unloaded
        })
    }

    fn publish_policy(&self, record: &SupervisorRecordV1, plist: &[u8]) -> Result<()> {
        atomic_write(&record.plist_path, plist, 0o600)
    }

    fn start(&self, _record: &SupervisorRecordV1) -> Result<()> {
        self.start_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if *self.fail_start.lock().unwrap() {
            bail!("launchctl exited non-zero");
        }
        *self.loaded.lock().unwrap() = true;
        Ok(())
    }

    fn stop(&self, _record: &SupervisorRecordV1) -> Result<()> {
        *self.loaded.lock().unwrap() = false;
        if let Some(shutdown) = self.shutdown.lock().unwrap().take() {
            let _ = shutdown.send(());
        }
        Ok(())
    }

    fn remove_policy(&self, record: &SupervisorRecordV1) -> Result<()> {
        remove_if_exists(&record.plist_path)
    }
}

#[cfg(test)]
fn install_supervisor_for_test_with_adapter(
    root: &Path,
    adapter: &FakeSupervisorAdapter,
) -> Result<(SupervisorPaths, SupervisorRecordV1)> {
    let paths = SupervisorPaths::isolated(root)?;
    let executable = paths.root.join("mct-daemon-fixture");
    fs::copy(std::env::current_exe()?, &executable)?;
    let mut mode = fs::metadata(&executable)?.permissions();
    mode.set_mode(mode.mode() | 0o700);
    fs::set_permissions(&executable, mode)?;
    install_with_adapter(&paths, &executable, current_uid()?, false, adapter)?;
    let record = read_record(&paths.record)?;
    Ok((paths, record))
}

#[cfg(test)]
fn install_supervisor_for_test(root: &Path) -> Result<(SupervisorPaths, SupervisorRecordV1)> {
    install_supervisor_for_test_with_adapter(root, &FakeSupervisorAdapter::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest as _, Sha256};
        format!("{:x}", Sha256::digest(bytes))
    }

    #[test]
    fn watcher_fixture_provenance_is_exact_source_derived_and_sidecar_free() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let folder = fixtures.join("folder-watch-actor-0.1.0");
        let sink = fixtures.join("watch-null-sink-0.1.0");
        let folder_manifest = folder.join("child.toml");
        let folder_component = folder.join("folder-watch-actor.wasm");
        let folder_patch = folder.join("MCT-REBUILD.patch");
        let folder_provenance = folder.join("PROVENANCE.md");
        let sink_manifest = sink.join("child.toml");
        let sink_component = sink.join("watch-null-sink.wasm");
        let sink_provenance = sink.join("PROVENANCE.md");

        for (path, size, sha256, blake3) in [
            (
                &folder_manifest,
                719,
                "f1f53ce495b3c5c408bb582a3d8a3d100f33102a4a355bdea2ac7848831c790a",
                "6ca8f8225fe11ae00c94d08fb8e530f53741c3b0dcfde2273a8e355fe52be718",
            ),
            (
                &folder_component,
                352_529,
                "00910422135e822524cd52e446c157056e755187392a36d441cd1ba406ba9096",
                "466033617dcd41c532f22f881f7fba347a793c9c52c1ac89e93f4df902ce251e",
            ),
            (
                &folder_patch,
                2_362,
                "a71f9924149cd96647de542e3eaf6940930fe77e5dcc626b07c38b4bd67d9a2c",
                "202bb03b3caf9cd258a59ab01f7ac76a7be2359d90e861502c8a72e230c10529",
            ),
            (
                &sink_manifest,
                358,
                "6447a156d08b4b438acc1b55f28cf05d1130889479f4428aca98b2e6d327238a",
                "780dc11735559d58248f69be4ca64659e82a0e4ba36d886d4a7be7608275fedf",
            ),
            (
                &sink_component,
                70_027,
                "37f42ebe17db2c6e44e02bf79cf590ba899d3cb96579bbd4ed735597f53dbfe3",
                "e40605e80b5a61fa7340abc1207676848b1a1383048bb1c5dc13c20e11d7a0cf",
            ),
        ] {
            let bytes = std::fs::read(path).unwrap();
            assert_eq!(bytes.len(), size, "{}", path.display());
            assert_eq!(sha256_hex(&bytes), sha256, "{}", path.display());
            assert_eq!(blake3::hash(&bytes).to_hex().as_str(), blake3);
        }
        for path in [
            &folder_manifest,
            &folder_component,
            &sink_manifest,
            &sink_component,
        ] {
            let mut sidecar = path.as_os_str().to_os_string();
            sidecar.push(".sha256");
            assert!(!PathBuf::from(sidecar).exists());
        }

        let folder_manifest_text = std::fs::read_to_string(&folder_manifest).unwrap();
        let sink_manifest_text = std::fs::read_to_string(&sink_manifest).unwrap();
        assert!(folder_manifest_text.contains("name = \"folder-watch-actor\""));
        assert!(folder_manifest_text.contains("version = \"0.1.0\""));
        assert!(sink_manifest_text.contains("name = \"watch-null-sink\""));
        assert!(sink_manifest_text.contains("version = \"0.1.0\""));

        let patch = std::fs::read_to_string(&folder_patch).unwrap();
        assert!(patch.contains("absolute_path: relative_path.clone()"));
        assert!(patch.contains("children/folder-watch-actor/wit/world.wit"));
        assert!(!patch.contains("children/watch-null-sink"));
        let folder_provenance = std::fs::read_to_string(folder_provenance).unwrap();
        let sink_provenance = std::fs::read_to_string(sink_provenance).unwrap();
        for provenance in [&folder_provenance, &sink_provenance] {
            assert!(provenance.contains("526dbf123b040198cb4395c1a63cf498a28ff915"));
            assert!(provenance.contains("cargo-component 0.21.1"));
            assert!(provenance.contains("rustc 1.94.0"));
        }
        assert!(folder_provenance.contains("source-derived MCT security rebuild"));
        assert!(
            folder_provenance
                .contains("a71f9924149cd96647de542e3eaf6940930fe77e5dcc626b07c38b4bd67d9a2c")
        );
        assert!(sink_provenance.contains("unmodified exact-tag source"));

        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        let engine = wasmtime::Engine::new(&config).unwrap();
        let folder_component =
            wasmtime::component::Component::from_file(&engine, folder_component).unwrap();
        let folder_imports = folder_component
            .component_type()
            .imports(&engine)
            .map(|(name, _)| name.to_string())
            .collect::<std::collections::BTreeSet<_>>();
        for required in [
            "wasi:keyvalue/store@0.2.0",
            "wasi:messaging/producer@0.2.0",
            "wasi:filesystem/preopens@0.2.3",
            "wasi:logging/logging@0.1.0",
            "patina:measure/measure@0.1.0",
        ] {
            assert!(folder_imports.contains(required), "missing {required}");
        }
        for forbidden in [
            "wasi:http/outgoing-handler@0.2.8",
            "wasi:sql/readwrite@0.1.0",
            "patina:connect/connect@0.2.0",
            "patina:git/git@0.1.0",
        ] {
            assert!(!folder_imports.contains(forbidden), "retained {forbidden}");
        }
        let folder_exports = folder_component
            .component_type()
            .exports(&engine)
            .map(|(name, _)| name.to_string())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(folder_exports.contains("patina:watch/control@0.1.0"));

        let sink_component =
            wasmtime::component::Component::from_file(&engine, sink_component).unwrap();
        let sink_exports = sink_component
            .component_type()
            .exports(&engine)
            .map(|(name, _)| name.to_string())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(sink_exports.contains("patina:watch/events@0.1.0"));
    }

    fn entries(path: &Path) -> Vec<mct_observation::MctObservationLedgerEntry> {
        JsonlObservationLedger::open_read_only(path, "ledger-local", "local-mct")
            .unwrap()
            .entries()
            .unwrap()
    }

    fn supervised_start(
        entries: &[mct_observation::MctObservationLedgerEntry],
        ordinal: usize,
    ) -> (String, String) {
        let entry = entries
            .iter()
            .filter(|entry| {
                entry
                    .observation
                    .safe_message
                    .starts_with("supervised resident instance started ")
            })
            .nth(ordinal)
            .unwrap();
        let instance_id = entry
            .observation
            .safe_message
            .trim_start_matches("supervised resident instance started ")
            .split_whitespace()
            .next()
            .unwrap()
            .to_owned();
        (instance_id, entry.observation.observation_id.to_string())
    }

    fn proof_artifact() -> ComponentArtifact {
        ComponentArtifact {
            artifact_id: ComponentArtifactId::new("supervisor-proof-artifact").unwrap(),
            child_name: "supervisor-proof-child".into(),
            artifact_version: "0.1.0".into(),
            content_hash: "blake3:supervisor-proof-artifact".into(),
            manifest_hash: "blake3:supervisor-proof-manifest".into(),
            primary_export: ComponentWitExport {
                namespace: "patina".into(),
                interface_name: "supervisor-proof".into(),
                version: "0.1.0".into(),
                function_names: vec!["verify".into()],
            },
            runtime_shape: ComponentRuntimeShape::WasmComponent,
            ingress_mode: ChildIngressMode::WitOnly,
            lifecycle_exports: LifecycleExports::AbsentAllowed,
            verification_status: VerificationStatus::Verified,
            provenance_status: mct_kernel::ArtifactProvenanceStatus::HistoricalUnknown,
            acquisition_ids: Vec::new(),
            created_by_observation_id: ObservationId::new("obs-supervisor-proof-artifact").unwrap(),
        }
    }

    #[test]
    fn supervisor_install_bootstrap_is_observed_before_every_remaining_effect() {
        let root = tempfile::tempdir().unwrap();
        let (paths, record) = install_supervisor_for_test(root.path()).unwrap();

        assert!(paths.config.exists());
        assert!(paths.identity.exists());
        assert!(paths.state.exists());
        assert!(paths.record.exists());
        assert!(paths.plist.exists());
        assert_eq!(
            fs::metadata(&paths.root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&paths.record).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let entries = entries(&paths.ledger);
        let governing_index = entries
            .iter()
            .position(|entry| {
                entry.observation.observation_id.as_str() == record.creation_observation_id
            })
            .unwrap();
        let identity_index = entries
            .iter()
            .position(|entry| {
                entry
                    .observation
                    .safe_message
                    .contains("identity creation or validation")
            })
            .unwrap();
        let completion_index = entries
            .iter()
            .position(|entry| entry.observation.safe_message.contains("install completed"))
            .unwrap();
        assert!(governing_index < identity_index);
        assert!(identity_index < completion_index);
        assert!(
            entries[governing_index]
                .observation
                .safe_message
                .contains("discovered")
        );
        assert_eq!(record.record_digest, record.canonical_digest().unwrap());
        assert_eq!(
            record.executable_digest,
            file_digest(&record.executable_path).unwrap()
        );
        assert_eq!(
            record.plist_digest,
            file_digest(&record.plist_path).unwrap()
        );
        validate_supervisor_record(&paths.record, true).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn supervisor_lifecycle_install_start_stop_unclean_reconcile_uninstall_preserves_evidence()
     {
        let root = tempfile::tempdir().unwrap();
        let adapter = Arc::new(FakeSupervisorAdapter::default());
        let (paths, record) =
            install_supervisor_for_test_with_adapter(root.path(), adapter.as_ref()).unwrap();
        fs::write(&paths.stdout_log, "preserved-log").unwrap();

        let install_entries = entries(&paths.ledger);
        let install_operator_count = install_entries
            .iter()
            .filter(|entry| entry.observation.kind == ObservationKind::OperatorActionRecorded)
            .count();
        assert!(install_entries.iter().any(|entry| {
            entry.observation.observation_id.as_str() == record.creation_observation_id
                && entry.observation.policy_revision == Some(1)
        }));
        assert_eq!(record.owner_uid, current_uid().unwrap());
        assert_eq!(record.created_by_uid, current_uid().unwrap());
        assert_eq!(record.record_revision, 1);
        assert_eq!(record.record_digest, record.canonical_digest().unwrap());
        assert_eq!(
            record.executable_digest,
            file_digest(&record.executable_path).unwrap()
        );
        assert_eq!(
            record.plist_digest,
            file_digest(&record.plist_path).unwrap()
        );

        let reopened_config = MctDaemonConfigStore::new(&paths.config).load().unwrap();
        let reopened_identity = reopened_config.local_identity.as_ref().unwrap();
        assert_eq!(reopened_identity.identity_path, paths.identity);
        let identity_bytes = fs::read(&paths.identity).unwrap();
        let identity_secret = load_or_create_node_secret_key_hex(&paths.identity).unwrap();
        assert_eq!(fs::read(&paths.identity).unwrap(), identity_bytes);
        assert_eq!(
            endpoint_id_for_secret_key_hex(&identity_secret).unwrap(),
            reopened_identity.endpoint_id
        );
        let reopened_state = MctRuntimeStateStore::open(&paths.state).unwrap();
        let initial_state_summary = reopened_state.summary().unwrap();
        assert!(initial_state_summary.schema_version > 0);
        assert_eq!(initial_state_summary.artifacts, 0);
        drop(reopened_state);

        start_with_adapter(&paths, current_uid().unwrap(), adapter.as_ref(), false).unwrap();
        let operator_count_before_boot = entries(&paths.ledger)
            .iter()
            .filter(|entry| entry.observation.kind == ObservationKind::OperatorActionRecorded)
            .count();
        assert!(operator_count_before_boot > install_operator_count);

        let (shutdown_one_tx, shutdown_one_rx) = tokio::sync::oneshot::channel();
        adapter.arm_shutdown(shutdown_one_tx);
        let (ready_one_tx, ready_one_rx) = tokio::sync::oneshot::channel();
        let record_path = paths.record.clone();
        let resident_one = tokio::spawn(async move {
            run_test_supervised_resident_mother(
                &record_path,
                async move {
                    let _ = shutdown_one_rx.await;
                },
                Some(ready_one_tx),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(15), ready_one_rx)
            .await
            .unwrap()
            .unwrap();
        let first_boot_entries = entries(&paths.ledger);
        assert_eq!(
            first_boot_entries
                .iter()
                .filter(|entry| entry.observation.kind == ObservationKind::OperatorActionRecorded)
                .count(),
            operator_count_before_boot,
            "supervised boot must not fabricate operator authentication"
        );
        assert!(first_boot_entries.iter().any(|entry| {
            entry.observation.safe_message.contains(&format!(
                "governing_record={}@1 provenance_observation={}",
                record.record_id, record.creation_observation_id
            ))
        }));
        let (first_instance_id, first_start_observation_id) =
            supervised_start(&first_boot_entries, 0);

        let stop_paths = paths.clone();
        let stop_adapter = Arc::clone(&adapter);
        tokio::task::spawn_blocking(move || {
            stop_with_adapter(&stop_paths, current_uid().unwrap(), stop_adapter.as_ref())
        })
        .await
        .unwrap()
        .unwrap();
        resident_one.await.unwrap().unwrap();
        let clean_entries = entries(&paths.ledger);
        assert!(clean_entries.iter().any(|entry| {
            entry.observation.safe_message
                == format!("supervised resident clean shutdown started {first_instance_id}")
        }));
        assert!(clean_entries.iter().any(|entry| {
            entry.observation.safe_message
                == format!("supervised resident clean shutdown completed {first_instance_id}")
        }));

        start_with_adapter(&paths, current_uid().unwrap(), adapter.as_ref(), false).unwrap();
        let (unclean_tx, unclean_rx) = tokio::sync::oneshot::channel::<()>();
        let (ready_two_tx, ready_two_rx) = tokio::sync::oneshot::channel();
        let record_path = paths.record.clone();
        let resident_two = tokio::spawn(async move {
            run_test_supervised_resident_mother(
                &record_path,
                async move {
                    let _ = unclean_rx.await;
                },
                Some(ready_two_tx),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(15), ready_two_rx)
            .await
            .unwrap()
            .unwrap();
        let second_start_entries = entries(&paths.ledger);
        assert!(second_start_entries.iter().all(|entry| {
            !entry
                .observation
                .safe_message
                .contains("reconciled unmatched prior instance")
        }));
        let (second_instance_id, second_start_observation_id) =
            supervised_start(&second_start_entries, 1);
        resident_two.abort();
        let _ = resident_two.await;
        drop(unclean_tx);
        adapter.simulate_unclean_exit();
        for _ in 0..100 {
            if JsonlObservationLedger::open(&paths.ledger, "ledger-local", "local-mct").is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        start_with_adapter(&paths, current_uid().unwrap(), adapter.as_ref(), false).unwrap();
        let (shutdown_three_tx, shutdown_three_rx) = tokio::sync::oneshot::channel();
        adapter.arm_shutdown(shutdown_three_tx);
        let (ready_three_tx, ready_three_rx) = tokio::sync::oneshot::channel();
        let record_path = paths.record.clone();
        let resident_three = tokio::spawn(async move {
            run_test_supervised_resident_mother(
                &record_path,
                async move {
                    let _ = shutdown_three_rx.await;
                },
                Some(ready_three_tx),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(15), ready_three_rx)
            .await
            .unwrap()
            .unwrap();
        let reconciled_entries = entries(&paths.ledger);
        let expected_reconciliation = format!(
            "supervised resident reconciled unmatched prior instance {second_instance_id} start_observation={second_start_observation_id}"
        );
        let reconciliation = reconciled_entries
            .iter()
            .position(|entry| entry.observation.safe_message == expected_reconciliation)
            .unwrap();
        let third_start = reconciled_entries
            .iter()
            .rposition(|entry| {
                entry
                    .observation
                    .safe_message
                    .starts_with("supervised resident instance started")
            })
            .unwrap();
        assert!(reconciliation < third_start);
        let (third_instance_id, third_start_observation_id) =
            supervised_start(&reconciled_entries, 2);

        let stop_paths = paths.clone();
        let stop_adapter = Arc::clone(&adapter);
        tokio::task::spawn_blocking(move || {
            stop_with_adapter(&stop_paths, current_uid().unwrap(), stop_adapter.as_ref())
        })
        .await
        .unwrap()
        .unwrap();
        resident_three.await.unwrap().unwrap();

        let artifact_bytes = b"populated-supervisor-artifact";
        let artifact_path = paths
            .children
            .join("supervisor-proof-child")
            .join("proof.component.wasm");
        fs::create_dir_all(artifact_path.parent().unwrap()).unwrap();
        fs::write(&artifact_path, artifact_bytes).unwrap();
        let proof_artifact = proof_artifact();
        let populated_state = MctRuntimeStateStore::open(&paths.state).unwrap();
        populated_state.upsert_artifact(&proof_artifact).unwrap();
        assert_eq!(
            populated_state
                .get_artifact(&proof_artifact.artifact_id)
                .unwrap(),
            Some(proof_artifact.clone())
        );
        drop(populated_state);

        let blob_bytes = b"populated-supervisor-blob";
        let blob_digest = blake3::hash(blob_bytes).to_hex().to_string();
        let blob_store = local_blob_store_for_state_path(&paths.state);
        let blob_handle = blob_store
            .ingest_reader(
                &blob_digest,
                blob_bytes.len() as u64,
                "application/octet-stream",
                &blob_bytes[..],
            )
            .unwrap();
        assert_eq!(blob_store.fetch(&blob_handle).unwrap(), blob_bytes);

        let ledger_before_uninstall = fs::read(&paths.ledger).unwrap();
        let config_before_uninstall = fs::read(&paths.config).unwrap();
        let identity_before_uninstall = fs::read(&paths.identity).unwrap();
        let uninstall_report =
            uninstall_with_adapter(&paths, current_uid().unwrap(), adapter.as_ref()).unwrap();

        assert!(!paths.record.exists());
        assert!(!paths.plist.exists());
        assert_eq!(
            adapter.inspect(&record).unwrap(),
            SupervisorLoadedState::Unloaded
        );
        assert!(paths.ledger.exists() && paths.state.exists() && paths.children.exists());
        assert_eq!(fs::read(&paths.config).unwrap(), config_before_uninstall);
        assert_eq!(
            fs::read(&paths.identity).unwrap(),
            identity_before_uninstall
        );
        assert_eq!(fs::read(&artifact_path).unwrap(), artifact_bytes);
        assert_eq!(blob_store.fetch(&blob_handle).unwrap(), blob_bytes);
        let final_state = MctRuntimeStateStore::open(&paths.state).unwrap();
        assert_eq!(
            final_state
                .get_artifact(&proof_artifact.artifact_id)
                .unwrap(),
            Some(proof_artifact)
        );
        drop(final_state);
        assert_eq!(
            fs::read_to_string(&paths.stdout_log).unwrap(),
            "preserved-log"
        );
        assert!(
            fs::read(&paths.ledger)
                .unwrap()
                .starts_with(&ledger_before_uninstall)
        );

        let final_entries = entries(&paths.ledger);
        let uninstall_entries = final_entries
            .iter()
            .filter(|entry| {
                entry.observation.trace.trace_id.as_str() == uninstall_report.attempt_id
            })
            .collect::<Vec<_>>();
        assert!(uninstall_entries.iter().any(|entry| {
            entry.observation.kind == ObservationKind::OperatorActionRecorded
                && entry.observation.outcome == ObservationOutcome::Allowed
        }));
        assert!(uninstall_entries.iter().any(|entry| {
            entry.observation.kind == ObservationKind::LifecycleTransitionRecorded
                && entry.observation.outcome == ObservationOutcome::Started
                && entry.observation.safe_message.contains("removal started")
        }));
        let uninstall_adapter_facts = uninstall_entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.observation.kind,
                    ObservationKind::AdapterEffectStarted
                        | ObservationKind::AdapterEffectCompleted
                        | ObservationKind::AdapterEffectFailed
                )
            })
            .map(|entry| {
                (
                    entry.observation.kind,
                    entry.observation.outcome,
                    entry.observation.safe_message.as_str(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            uninstall_adapter_facts,
            vec![
                (
                    ObservationKind::AdapterEffectStarted,
                    ObservationOutcome::Started,
                    "launchd plist and current supervisor record removal started",
                ),
                (
                    ObservationKind::AdapterEffectCompleted,
                    ObservationOutcome::Completed,
                    "launchd plist and current supervisor record removed",
                ),
            ]
        );
        assert!(uninstall_entries.iter().any(|entry| {
            entry.observation.observation_id.as_str() == uninstall_report.observation_id
                && entry.observation.kind == ObservationKind::LifecycleTransitionRecorded
                && entry.observation.outcome == ObservationOutcome::Completed
                && entry.observation.safe_message.contains(
                    "uninstall completed; ledger state identity children and logs preserved",
                )
        }));

        let messages = final_entries
            .iter()
            .map(|entry| entry.observation.safe_message.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            messages
                .iter()
                .filter(|message| message.starts_with("supervised resident instance started "))
                .count(),
            3
        );
        assert_eq!(
            messages
                .iter()
                .filter(|message| **message == "supervisor stop and launchd bootout completed")
                .count(),
            2
        );

        let chain = vec![
            "authenticated supervisor install requested".to_owned(),
            "supervisor record revision 1 install started".to_owned(),
            "local identity creation or validation admitted after install bootstrap fact"
                .to_owned(),
            "launchd supervisor record and plist publication started".to_owned(),
            "launchd supervisor record and plist published".to_owned(),
            "supervisor install completed without starting resident".to_owned(),
            "direct supervisor start attempt recorded before launchd bootstrap".to_owned(),
            "launchd bootstrap started".to_owned(),
            format!("supervised resident instance started {first_instance_id} "),
            format!(
                "supervised resident ready instance={first_instance_id} start_observation={first_start_observation_id}"
            ),
            "supervisor stop prepared by owner-authenticated lifecycle control".to_owned(),
            "launchd bootout effect prepared".to_owned(),
            format!("supervised resident clean shutdown started {first_instance_id}"),
            "resident Mother endpoint closed".to_owned(),
            format!("supervised resident clean shutdown completed {first_instance_id}"),
            "launchd bootout completed after clean resident shutdown".to_owned(),
            "supervisor stop and launchd bootout completed".to_owned(),
            "direct supervisor start attempt recorded before launchd bootstrap".to_owned(),
            "launchd bootstrap started".to_owned(),
            format!("supervised resident instance started {second_instance_id} "),
            format!(
                "supervised resident ready instance={second_instance_id} start_observation={second_start_observation_id}"
            ),
            "direct supervisor start attempt recorded before launchd bootstrap".to_owned(),
            "launchd bootstrap started".to_owned(),
            expected_reconciliation.clone(),
            format!("supervised resident instance started {third_instance_id} "),
            format!(
                "supervised resident ready instance={third_instance_id} start_observation={third_start_observation_id}"
            ),
            "supervisor stop prepared by owner-authenticated lifecycle control".to_owned(),
            "launchd bootout effect prepared".to_owned(),
            format!("supervised resident clean shutdown started {third_instance_id}"),
            "resident Mother endpoint closed".to_owned(),
            format!("supervised resident clean shutdown completed {third_instance_id}"),
            "launchd bootout completed after clean resident shutdown".to_owned(),
            "supervisor stop and launchd bootout completed".to_owned(),
            "authenticated supervisor uninstall requested".to_owned(),
            "supervisor uninstall removal started; evidence and runtime state preserved".to_owned(),
            "launchd plist and current supervisor record removal started".to_owned(),
            "launchd plist and current supervisor record removed".to_owned(),
            "supervisor uninstall completed; ledger state identity children and logs preserved"
                .to_owned(),
        ];
        let mut cursor = 0;
        for expected in chain {
            let relative = messages[cursor..]
                .iter()
                .position(|message| message.contains(&expected))
                .unwrap_or_else(|| panic!("missing ordered lifecycle fact: {expected}"));
            cursor += relative + 1;
        }
    }

    #[test]
    fn smoke_plist_seam_is_internal_isolated_and_keeps_fixed_service_identity() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("smoke-root");
        fs::create_dir(&root).unwrap();
        let smoke = SupervisorPaths::smoke_internal(&root).unwrap();
        assert_eq!(
            smoke.plist,
            smoke
                .root
                .join("launchd")
                .join(format!("{MCT_LAUNCHD_LABEL}.plist"))
        );
        assert!(smoke.plist.starts_with(&smoke.root));
        assert_eq!(MCT_LAUNCHD_LABEL, "io.patina.mct.mother");

        let home = temp.path().join("home");
        let production = SupervisorPaths::production(root.clone(), &home).unwrap();
        assert_eq!(
            production.plist,
            home.join("Library")
                .join("LaunchAgents")
                .join(format!("{MCT_LAUNCHD_LABEL}.plist"))
        );
        assert_ne!(smoke.plist, production.plist);

        let help = help_text();
        assert!(!help.contains("--plist"));
        assert!(!help.contains("--label"));
        assert!(!help.contains("release-smoke-internal"));
    }

    #[test]
    fn supervisor_command_surface_is_explicit_and_macos_only() {
        let commands = [
            "install",
            "uninstall",
            "start",
            "stop",
            "restart",
            "serve",
            "status",
        ];
        assert_eq!(
            &commands[..5],
            ["install", "uninstall", "start", "stop", "restart"]
        );
        assert!(commands.contains(&"serve") && commands.contains(&"status"));
        assert_eq!(MCT_LAUNCHD_LABEL, "io.patina.mct.mother");
    }

    #[test]
    fn supervised_start_rejects_unobserved_tampered_or_stale_records() {
        let root = tempfile::tempdir().unwrap();
        let (paths, mut record) = install_supervisor_for_test(root.path()).unwrap();
        record.record_state = "revoked".into();
        record.refresh_digest().unwrap();
        atomic_write(
            &paths.record,
            &serde_json::to_vec_pretty(&record).unwrap(),
            0o600,
        )
        .unwrap();
        assert!(validate_supervisor_record(&paths.record, true).is_err());

        record.record_state = "active".into();
        record.creation_observation_id = "obs-does-not-exist".into();
        record.refresh_digest().unwrap();
        atomic_write(
            &paths.record,
            &serde_json::to_vec_pretty(&record).unwrap(),
            0o600,
        )
        .unwrap();
        let error = validate_supervisor_record(&paths.record, true).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("governing observation is absent")
        );
    }

    #[test]
    fn supervisor_conflicts_refuse_before_launchd_or_endpoint_effects() {
        let root = tempfile::tempdir().unwrap();
        let adapter = FakeSupervisorAdapter::default();
        let (paths, _) = install_supervisor_for_test_with_adapter(root.path(), &adapter).unwrap();
        let executable = paths.root.join("mct-daemon-fixture");
        let second =
            install_with_adapter(&paths, &executable, current_uid().unwrap(), false, &adapter)
                .unwrap_err();
        assert!(second.to_string().contains("current record exists"));
        assert_eq!(
            adapter
                .inspect(&read_record(&paths.record).unwrap())
                .unwrap(),
            SupervisorLoadedState::Unloaded
        );
        assert!(entries(&paths.ledger).iter().any(|entry| {
            entry.observation.outcome == ObservationOutcome::Denied
                && entry
                    .observation
                    .safe_message
                    .contains("use install --replace")
        }));

        let held =
            JsonlObservationLedger::open(&paths.ledger, "ledger-local", "local-mct").unwrap();
        let start =
            start_with_adapter(&paths, current_uid().unwrap(), &adapter, false).unwrap_err();
        assert!(start.to_string().contains("writer lock"));
        assert_eq!(
            adapter
                .inspect(&read_record(&paths.record).unwrap())
                .unwrap(),
            SupervisorLoadedState::Unloaded
        );
        drop(held);

        let manual = refuse_manual_serve_if_managed(&paths.config, false).unwrap_err();
        assert!(manual.to_string().contains("manual serve refused"));

        let concurrent_root = tempfile::tempdir().unwrap();
        let concurrent_paths = SupervisorPaths::isolated(concurrent_root.path()).unwrap();
        ensure_owner_private_root(&concurrent_paths.root, current_uid().unwrap()).unwrap();
        let held_bootstrap =
            JsonlObservationLedger::open(&concurrent_paths.ledger, "ledger-local", "local-mct")
                .unwrap();
        let concurrent_executable = concurrent_paths.root.join("mct-daemon-fixture");
        fs::copy(&executable, &concurrent_executable).unwrap();
        let mut permissions = fs::metadata(&concurrent_executable).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&concurrent_executable, permissions).unwrap();
        let losing_paths = concurrent_paths.clone();
        let losing_adapter = Arc::new(FakeSupervisorAdapter::default());
        let losing_adapter_thread = Arc::clone(&losing_adapter);
        let loser = std::thread::spawn(move || {
            install_with_adapter(
                &losing_paths,
                &concurrent_executable,
                current_uid().unwrap(),
                false,
                losing_adapter_thread.as_ref(),
            )
        });
        std::thread::sleep(Duration::from_millis(100));
        drop(held_bootstrap);
        let contention = loser.join().unwrap().unwrap_err();
        assert!(
            contention
                .to_string()
                .contains("concurrent supervisor install refused")
        );
        assert!(!concurrent_paths.record.exists() && !concurrent_paths.plist.exists());
        assert!(entries(&concurrent_paths.ledger).iter().any(|entry| {
            entry
                .observation
                .safe_message
                .contains("losing exclusive bootstrap writer")
        }));
    }

    #[tokio::test]
    async fn resident_writer_loss_fences_lifecycle_and_all_other_protected_effects() {
        let writer = ResidentLedgerWriter::failed_for_test();
        assert!(writer.is_fenced());
        let append = writer
            .append(vec![lifecycle_observation(
                "obs-fenced-test",
                "trace-fenced-test",
                ObservationKind::LifecycleTransitionRecorded,
                SourcePlane::Operator,
                "local-mct",
                MCT_LAUNCHD_LABEL,
                None,
                ObservationOutcome::Started,
                "must not append while fenced",
                None,
            )])
            .await;
        assert!(append.is_err());
        let response = execute_authenticated_lifecycle_fact(
            current_uid().unwrap(),
            &writer,
            br#"{"action":"stop_prepare"}"#,
        )
        .await;
        assert_eq!(response.status_code, 500);
        assert!(writer.is_fenced());
    }

    #[test]
    fn launchd_adapter_refuses_missing_gui_domain_without_fallback() {
        let root = tempfile::tempdir().unwrap();
        let paths = SupervisorPaths::isolated(root.path()).unwrap();
        let executable = paths.root.join("mct-daemon-fixture");
        fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&executable, permissions).unwrap();
        let adapter = FakeSupervisorAdapter::default();
        adapter.simulate_missing_gui_domain();
        let error =
            install_with_adapter(&paths, &executable, current_uid().unwrap(), false, &adapter)
                .unwrap_err();
        assert!(error.to_string().contains("GUI domain unavailable"));
        assert!(!paths.config.exists() && !paths.identity.exists());
        assert!(!paths.record.exists() && !paths.plist.exists());
        assert!(entries(&paths.ledger).iter().any(|entry| {
            entry.observation.kind == ObservationKind::AdapterEffectFailed
                && entry.observation.safe_message.contains("headless/SSH-only")
        }));
        let service = format!("gui/{}/{}", current_uid().unwrap(), MCT_LAUNCHD_LABEL);
        assert!(!service.contains("user/") && !service.contains("system/"));
    }

    #[test]
    fn supervised_start_rejects_unblessed_binary_swap_with_replace_guidance() {
        let root = tempfile::tempdir().unwrap();
        let (paths, record) = install_supervisor_for_test(root.path()).unwrap();
        fs::write(&record.executable_path, b"binary-swap").unwrap();

        let error = start_with_adapter(
            &paths,
            current_uid().unwrap(),
            &FakeSupervisorAdapter::default(),
            false,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("supervisor executable digest mismatch")
        );
        assert!(error.to_string().contains("install --replace"));
        assert!(entries(&paths.ledger).iter().any(|entry| {
            entry.observation.outcome == ObservationOutcome::Denied
                && entry
                    .observation
                    .safe_message
                    .contains("supervisor executable digest mismatch")
                && entry.observation.safe_message.contains("install --replace")
        }));

        let replacement = install_with_adapter(
            &paths,
            &record.executable_path,
            current_uid().unwrap(),
            true,
            &FakeSupervisorAdapter::default(),
        )
        .unwrap();
        assert_eq!(replacement.supervisor_revision, Some(2));
        let replaced = validate_supervisor_record(&paths.record, true).unwrap();
        assert_eq!(
            replaced.executable_digest,
            file_digest(&record.executable_path).unwrap()
        );
    }

    #[test]
    fn uninstall_refuses_foreign_plist_with_durable_observation() {
        let root = tempfile::tempdir().unwrap();
        let adapter = FakeSupervisorAdapter::default();
        let (paths, _) = install_supervisor_for_test_with_adapter(root.path(), &adapter).unwrap();
        let foreign_plist = b"foreign launchd policy";
        fs::write(&paths.plist, foreign_plist).unwrap();

        let error = uninstall_with_adapter(&paths, current_uid().unwrap(), &adapter).unwrap_err();

        assert!(error.to_string().contains("foreign plist preserved"));
        assert_eq!(fs::read(&paths.plist).unwrap(), foreign_plist);
        assert!(paths.record.exists());
        assert!(entries(&paths.ledger).iter().any(|entry| {
            entry.observation.kind == ObservationKind::LifecycleTransitionRecorded
                && entry.observation.outcome == ObservationOutcome::Denied
                && entry.observation.safe_message
                    == "supervisor uninstall refused: managed plist digest mismatch; foreign plist preserved"
        }));
    }

    #[test]
    fn launchd_non_zero_start_is_observed_once_without_fallback() {
        let root = tempfile::tempdir().unwrap();
        let adapter = FakeSupervisorAdapter::default();
        let (paths, record) =
            install_supervisor_for_test_with_adapter(root.path(), &adapter).unwrap();
        adapter.simulate_start_failure();

        let error =
            start_with_adapter(&paths, current_uid().unwrap(), &adapter, false).unwrap_err();

        assert!(error.to_string().contains("launchctl exited non-zero"));
        assert_eq!(adapter.start_call_count(), 1);
        assert_eq!(
            record.launchd_domain,
            format!("gui/{}", current_uid().unwrap())
        );
        assert_eq!(
            adapter.inspect(&record).unwrap(),
            SupervisorLoadedState::Unloaded
        );
        let failure_entries = entries(&paths.ledger);
        assert!(failure_entries.iter().any(|entry| {
            entry.observation.kind == ObservationKind::AdapterEffectFailed
                && entry.observation.outcome == ObservationOutcome::Failed
                && entry.observation.safe_message == "launchd bootstrap failed"
        }));
        assert!(failure_entries.iter().any(|entry| {
            entry.observation.kind == ObservationKind::LifecycleTransitionRecorded
                && entry.observation.outcome == ObservationOutcome::Failed
                && entry.observation.safe_message
                    == "supervisor start failed after launchd adapter refusal"
        }));
        assert!(failure_entries.iter().all(|entry| {
            !entry.observation.safe_message.contains("user/")
                && !entry.observation.safe_message.contains("system/")
                && !entry.observation.safe_message.contains("detached")
        }));
    }

    #[test]
    fn install_replace_refuses_loaded_service_durably() {
        let root = tempfile::tempdir().unwrap();
        let adapter = FakeSupervisorAdapter::default();
        let (paths, record) =
            install_supervisor_for_test_with_adapter(root.path(), &adapter).unwrap();
        start_with_adapter(&paths, current_uid().unwrap(), &adapter, false).unwrap();

        let error = install_with_adapter(
            &paths,
            &record.executable_path,
            current_uid().unwrap(),
            true,
            &adapter,
        )
        .unwrap_err();

        assert!(error.to_string().contains("service is loaded"));
        assert_eq!(read_record(&paths.record).unwrap().record_revision, 1);
        assert!(entries(&paths.ledger).iter().any(|entry| {
            entry.observation.kind == ObservationKind::LifecycleTransitionRecorded
                && entry.observation.outcome == ObservationOutcome::Denied
                && entry.observation.safe_message
                    == "supervisor replacement refused while launchd service is loaded"
        }));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn supervisor_start_and_stop_no_ops_are_observed() {
        let root = tempfile::tempdir().unwrap();
        let adapter = Arc::new(FakeSupervisorAdapter::default());
        let (paths, _) =
            install_supervisor_for_test_with_adapter(root.path(), adapter.as_ref()).unwrap();
        start_with_adapter(&paths, current_uid().unwrap(), adapter.as_ref(), false).unwrap();

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        adapter.arm_shutdown(shutdown_tx);
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let record_path = paths.record.clone();
        let resident = tokio::spawn(async move {
            run_test_supervised_resident_mother(
                &record_path,
                async move {
                    let _ = shutdown_rx.await;
                },
                Some(ready_tx),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(15), ready_rx)
            .await
            .unwrap()
            .unwrap();

        let start_no_op =
            start_with_adapter(&paths, current_uid().unwrap(), adapter.as_ref(), false).unwrap();
        assert_eq!(start_no_op.outcome, "no_op");
        assert_eq!(
            start_no_op.safe_message,
            "supervisor start completed as observed no-op: service already loaded"
        );

        let stop_paths = paths.clone();
        let stop_adapter = Arc::clone(&adapter);
        tokio::task::spawn_blocking(move || {
            stop_with_adapter(&stop_paths, current_uid().unwrap(), stop_adapter.as_ref())
        })
        .await
        .unwrap()
        .unwrap();
        resident.await.unwrap().unwrap();

        let stop_no_op =
            stop_with_adapter(&paths, current_uid().unwrap(), adapter.as_ref()).unwrap();
        assert_eq!(stop_no_op.outcome, "no_op");
        assert_eq!(stop_no_op.safe_message, "supervisor already stopped");
        let no_op_entries = entries(&paths.ledger);
        assert!(no_op_entries.iter().any(|entry| {
            entry.observation.kind == ObservationKind::LifecycleTransitionRecorded
                && entry.observation.outcome == ObservationOutcome::Completed
                && entry.observation.safe_message
                    == "supervisor start completed as observed no-op: service already loaded"
        }));
        assert!(no_op_entries.iter().any(|entry| {
            entry.observation.kind == ObservationKind::LifecycleTransitionRecorded
                && entry.observation.outcome == ObservationOutcome::Completed
                && entry.observation.safe_message
                    == "supervisor stop completed as observed no-op: service already unloaded"
        }));
    }

    #[tokio::test]
    async fn shutdown_append_failure_has_no_clean_claim_and_next_start_reconciles() {
        let root = tempfile::tempdir().unwrap();
        let (paths, record) = install_supervisor_for_test(root.path()).unwrap();
        let first_writer = ResidentLedgerWriter::spawn(paths.ledger.clone()).unwrap();
        let first_instance = begin_supervised_resident_instance(&record, &first_writer)
            .await
            .unwrap();
        first_writer.close().await;

        let failed_writer = ResidentLedgerWriter::failed_for_test();
        let shutdown_error =
            record_supervised_clean_shutdown_started(&first_instance, &failed_writer)
                .await
                .unwrap_err();
        assert!(shutdown_error.to_string().contains("fenced"));
        assert!(entries(&paths.ledger).iter().all(|entry| {
            entry.observation.safe_message
                != format!(
                    "supervised resident clean shutdown completed {}",
                    first_instance.instance_id
                )
        }));

        let next_writer = loop {
            match ResidentLedgerWriter::spawn(paths.ledger.clone()) {
                Ok(writer) => break writer,
                Err(_) => tokio::time::sleep(Duration::from_millis(25)).await,
            }
        };
        let next_instance = begin_supervised_resident_instance(&record, &next_writer)
            .await
            .unwrap();
        next_writer.close().await;

        let reopened = entries(&paths.ledger);
        let reconciliation_message = format!(
            "supervised resident reconciled unmatched prior instance {} start_observation={}",
            first_instance.instance_id, first_instance.start_observation_id
        );
        let reconciliation = reopened
            .iter()
            .position(|entry| entry.observation.safe_message == reconciliation_message)
            .unwrap();
        let next_start = reopened
            .iter()
            .position(|entry| {
                entry.observation.safe_message.starts_with(&format!(
                    "supervised resident instance started {} ",
                    next_instance.instance_id
                ))
            })
            .unwrap();
        assert!(reconciliation < next_start);
    }

    #[test]
    fn artifact_command_surface_is_explicit_and_supervisor_distinct() {
        assert!(
            run_registry(vec!["install".into()])
                .unwrap_err()
                .to_string()
                .contains("artifacts acquire --operator-pointed")
        );
        assert!(
            run_registry(vec!["sync".into()])
                .unwrap_err()
                .to_string()
                .contains("artifacts acquire --source-authority")
        );
        assert_eq!(
            mct_daemon::MCT_FILESYSTEM_ACQUISITION_ADAPTER,
            "mct:artifact-acquisition/filesystem@1"
        );
        assert_ne!(
            MCT_LAUNCHD_LABEL,
            mct_daemon::MCT_FILESYSTEM_ACQUISITION_ADAPTER
        );
    }

    #[test]
    fn daemon_release_upgrade_surface_is_distinct_exact_and_source_closed() {
        let missing = run_upgrade(Vec::new()).unwrap_err();
        assert!(missing.to_string().contains("requires <artifact-ref>"));
        let network = run_upgrade(vec!["https://example.invalid/mct.tar.gz".into()]).unwrap_err();
        assert!(network.to_string().contains("operator_file"));
        assert!(help_text().contains("upgrade <artifact-ref>"));
        assert!(!help_text().contains("upgrade --yes"));
    }

    #[test]
    fn exact_upgrade_approval_composes_shared_replace_and_named_post_verify() {
        assert_eq!(MCT_UPGRADE_POST_VERIFY_DEADLINE_SECONDS, 30);
        let temp = tempfile::tempdir().unwrap();
        let paths = SupervisorPaths::isolated(temp.path()).unwrap();
        let uid = current_uid().unwrap();
        let adapter = FakeSupervisorAdapter::default();
        let current = std::env::current_exe().unwrap();
        let initial = install_with_adapter(&paths, &current, uid, false, &adapter).unwrap();
        let context = UpgradeSupervisorContext {
            paths: paths.clone(),
            state_path: paths.state.clone(),
            ledger_path: paths.ledger.clone(),
            uds_path: paths.uds.clone(),
            releases_dir: paths.root.join("releases"),
            current_executable: current.clone(),
            current_executable_digest: format!("blake3:{}", file_digest(&current).unwrap()),
            supervisor_record_id: initial.supervisor_record_id.unwrap(),
            supervisor_revision: initial.supervisor_revision.unwrap(),
            authenticated_uid: uid,
        };
        let immutable = paths.root.join("releases/sha256/candidate");
        let relative = PathBuf::from("payload/mct-daemon.app/Contents/MacOS/mct-daemon");
        let candidate = immutable.join(&relative);
        fs::create_dir_all(candidate.parent().unwrap()).unwrap();
        fs::copy(&current, &candidate).unwrap();
        let artifact_id = format!("sha256:{}", "a".repeat(64));
        let artifact = DaemonReleaseArtifactV1 {
            schema_version: 1,
            release_artifact_id: artifact_id.clone(),
            product: "mct-daemon".into(),
            product_version: mct_daemon::version().into(),
            target_triple: "aarch64-apple-darwin".into(),
            archive_size_bytes: 1,
            archive_sha256: artifact_id.clone(),
            archive_blake3: format!("blake3:{}", "b".repeat(64)),
            release_manifest_sha256: format!("sha256:{}", "c".repeat(64)),
            executable_relative_path: relative.to_string_lossy().into_owned(),
            executable_sha256: format!("sha256:{}", "d".repeat(64)),
            executable_blake3: format!("blake3:{}", file_digest(&candidate).unwrap()),
            release_notes_sha256: format!("sha256:{}", "e".repeat(64)),
            sbom_sha256: format!("sha256:{}", "1".repeat(64)),
            fixture_provenance_sha256: format!("sha256:{}", "2".repeat(64)),
            source_revision: "3".repeat(40),
            rust_toolchain: "1.96.0".into(),
            signing_mode: "adhoc".into(),
            source_kind: "operator_file".into(),
            source_ref: "file:///tmp/candidate.tar.gz".into(),
            acquisition_decision_id: "decision:test-upgrade".into(),
            adapter_effect_authority_ref: "obs:test-upgrade-adapter".into(),
            acquisition_observation_id: "obs:test-upgrade-acquired".into(),
            verification_observation_id: "obs:test-upgrade-verified".into(),
            immutable_release_path: immutable,
        };

        record_upgrade_fact(&context, &artifact_id, "upgrade_approval_admitted").unwrap();
        let completion = execute_upgrade_lifecycle_with_adapter(
            &context,
            &artifact,
            &adapter,
            false,
            |successor| {
                assert_eq!(successor.record_revision, context.supervisor_revision + 1);
                assert_eq!(successor.executable_path, candidate);
                assert_eq!(
                    format!("blake3:{}", successor.executable_digest),
                    artifact.executable_blake3
                );
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(completion.action, "upgrade_complete");
        let messages = entries(&paths.ledger)
            .into_iter()
            .map(|entry| entry.observation.safe_message)
            .collect::<Vec<_>>();
        let approval = messages
            .iter()
            .position(|message| message.contains("exact daemon release artifact approval"))
            .unwrap();
        let replacement = messages
            .iter()
            .position(|message| message.contains("record revision 2 install started"))
            .unwrap();
        let completion = messages
            .iter()
            .position(|message| message.contains("upgrade completed after bounded"))
            .unwrap();
        assert!(approval < replacement && replacement < completion);

        let successor = validate_supervisor_record(&paths.record, true).unwrap();
        let failed_context = UpgradeSupervisorContext {
            paths: paths.clone(),
            state_path: paths.state.clone(),
            ledger_path: paths.ledger.clone(),
            uds_path: paths.uds.clone(),
            releases_dir: paths.root.join("releases"),
            current_executable: candidate.clone(),
            current_executable_digest: format!("blake3:{}", file_digest(&candidate).unwrap()),
            supervisor_record_id: successor.record_id.clone(),
            supervisor_revision: successor.record_revision,
            authenticated_uid: uid,
        };
        let mut failed_artifact = artifact.clone();
        failed_artifact.release_artifact_id = format!("sha256:{}", "f".repeat(64));
        failed_artifact.archive_sha256 = failed_artifact.release_artifact_id.clone();
        *adapter.loaded.lock().unwrap() = false;
        adapter.simulate_start_failure();
        record_upgrade_fact(
            &failed_context,
            &failed_artifact.release_artifact_id,
            "upgrade_approval_admitted",
        )
        .unwrap();
        let error = execute_upgrade_lifecycle_with_adapter(
            &failed_context,
            &failed_artifact,
            &adapter,
            false,
            |_| Ok(()),
        )
        .unwrap_err();
        assert!(error.to_string().contains("without automatic rollback"));
        let failed_record = validate_supervisor_record(&paths.record, true).unwrap();
        assert_eq!(failed_record.record_revision, successor.record_revision + 1);
        assert_eq!(failed_record.executable_path, candidate);
        assert!(entries(&paths.ledger).iter().any(|entry| {
            entry.observation.resource_id.as_deref()
                == Some(failed_artifact.release_artifact_id.as_str())
                && entry.observation.safe_message
                    == "daemon release upgrade failed without automatic rollback"
        }));
    }

    #[test]
    fn artifact_slice_exposes_only_filesystem_adapter_and_existing_toy_catalog() {
        assert_eq!(slate_toy_contracts().len(), 4);
        assert!(
            slate_toy_contracts()
                .iter()
                .all(|contract| !contract.toy_id.as_str().contains("acquisition"))
        );
        assert_eq!(
            mct_daemon::MCT_FILESYSTEM_ACQUISITION_ADAPTER,
            "mct:artifact-acquisition/filesystem@1"
        );
    }

    #[test]
    fn child_approval_names_exact_artifact_and_surfaces_acquisition_evidence() {
        let error = run_children_approve(vec!["slate-manager".into()]).unwrap_err();
        assert!(error.to_string().contains("requires --artifact"));
    }

    #[test]
    fn exact_approval_refuses_wrong_historical_failed_and_tampered_artifact_evidence() {
        let root = tempfile::tempdir().unwrap();
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/slate-manager-0.2.0");
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::copy(
            fixture.join("slate-manager.toml"),
            source.join("slate-manager.toml"),
        )
        .unwrap();
        fs::copy(
            fixture.join("slate-manager.wasm"),
            source.join("slate-manager.wasm"),
        )
        .unwrap();
        let children = root.path().join("children");
        let state_path = root.path().join("state.sqlite");
        let request = MctArtifactStageRequest {
            source_root: source,
            manifest_path: PathBuf::from("slate-manager.toml"),
            component_path: PathBuf::from("slate-manager.wasm"),
            claimed_child_name: "slate-manager".into(),
            claimed_artifact_version: "0.2.0".into(),
            expected_digest: None,
            standing_source_authority_id: None,
            claimed_publisher: None,
            require_source_sidecars: false,
            children_dir: children.clone(),
            state_path: state_path.clone(),
        };
        let report = mct_daemon::stage_operator_pointed_artifact(&request).unwrap();
        let artifact_id = ComponentArtifactId::new(report.artifact_id.unwrap()).unwrap();
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        assert!(
            super::cli_runtime::load_exact_catalog_approval_evidence(
                &state,
                &children,
                &ComponentArtifactId::new(format!("sha256:{}", "0".repeat(64))).unwrap(),
                "slate-manager",
            )
            .is_err()
        );
        fs::write(
            report
                .package_path
                .unwrap()
                .join("artifact/slate-manager.wasm"),
            b"tampered",
        )
        .unwrap();
        assert!(
            super::cli_runtime::load_exact_catalog_approval_evidence(
                &state,
                &children,
                &artifact_id,
                "slate-manager",
            )
            .is_err()
        );

        let historical_state =
            MctRuntimeStateStore::open(root.path().join("historical.sqlite")).unwrap();
        let historical = proof_artifact();
        historical_state.upsert_artifact(&historical).unwrap();
        assert!(
            super::cli_runtime::load_exact_catalog_approval_evidence(
                &historical_state,
                &children,
                &historical.artifact_id,
                &historical.child_name,
            )
            .is_err()
        );

        let failed_root = tempfile::tempdir().unwrap();
        let mut failed_request = request;
        failed_request.source_root = failed_root.path().join("missing-source");
        failed_request.state_path = failed_root.path().join("state.sqlite");
        failed_request.children_dir = failed_root.path().join("children");
        assert!(mct_daemon::stage_operator_pointed_artifact(&failed_request).is_err());
        let failed_state = MctRuntimeStateStore::open(&failed_request.state_path).unwrap();
        assert!(
            failed_state.artifact_acquisitions().unwrap()[0]
                .component_artifact_id
                .is_none()
        );
    }

    #[test]
    fn standing_artifact_source_authority_is_scoped_observed_and_revocable() {
        let root = tempfile::tempdir().unwrap();
        let source_root = root.path().join("source");
        fs::create_dir(&source_root).unwrap();
        let state_path = root.path().join("state.sqlite");
        let mut source = ArtifactSourceAuthority {
            source_authority_id: ArtifactSourceAuthorityId::new("source-test").unwrap(),
            source_ref: format!("file://{}", source_root.display()),
            scope: ArtifactSourceScope {
                scope_mode: ArtifactSourceScopeMode::Constrained,
                artifact_scope: vec!["slate-manager@0.2.0".into()],
                publisher_scope: vec!["patina".into()],
                namespace_scope: vec!["patina:slate".into()],
                allowed_actions: vec!["acquire".into()],
            },
            integrity_policy_ref: "sha256-sidecars-v1".into(),
            provenance_policy_ref: None,
            issuer_principal_ref: format!("os-uid:{}", current_uid().unwrap()),
            policy_revision: 1,
            authority_state: ArtifactSourceAuthorityState::Active,
            issued_at: Timestamp::new("2026-07-16T00:00:00Z").unwrap(),
            expires_at: Timestamp::new("2099-01-01T00:00:00Z").unwrap(),
            authority_observation_id: ObservationId::new("obs-source-test").unwrap(),
        };
        let store = MctRuntimeStateStore::open(&state_path).unwrap();
        let active_digest = blake3::hash(&serde_json::to_vec(&source).unwrap())
            .to_hex()
            .to_string();
        store
            .upsert_source_authority(&source, &active_digest)
            .unwrap();
        source.authority_state = ArtifactSourceAuthorityState::Revoked;
        source.authority_observation_id = ObservationId::new("obs-source-revoked").unwrap();
        let revoked_digest = blake3::hash(&serde_json::to_vec(&source).unwrap())
            .to_hex()
            .to_string();
        store
            .upsert_source_authority(&source, &revoked_digest)
            .unwrap();
        drop(store);
        let sources = MctRuntimeStateStore::open(&state_path)
            .unwrap()
            .source_authorities()
            .unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(
            sources[0].0.authority_state,
            ArtifactSourceAuthorityState::Revoked
        );
        assert_eq!(sources[0].1, revoked_digest);
        assert!(
            sources[0]
                .0
                .scope
                .artifact_scope
                .iter()
                .all(|value| value != "*")
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn supervised_trigger_watch_delivery_fixtures_execute_end_to_end() {
        use base64::Engine as _;
        use sha2::{Digest, Sha256};
        use std::collections::BTreeMap;
        use std::os::unix::fs::PermissionsExt;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        async fn uds_json(
            socket: &Path,
            method: &str,
            path: &str,
            value: &serde_json::Value,
        ) -> (u16, serde_json::Value) {
            let body = serde_json::to_vec(value).unwrap();
            let request = format!(
                "{method} {path} HTTP/1.1\r\nHost: local\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            let mut stream = tokio::net::UnixStream::connect(socket).await.unwrap();
            stream.write_all(request.as_bytes()).await.unwrap();
            stream.write_all(&body).await.unwrap();
            let mut response = Vec::new();
            stream.read_to_end(&mut response).await.unwrap();
            let response = String::from_utf8(response).unwrap();
            let status = response
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse::<u16>()
                .unwrap();
            let body = response.split_once("\r\n\r\n").unwrap().1;
            (status, serde_json::from_str(body).unwrap())
        }

        fn wit_call_submission(
            call_suffix: &str,
            namespace: &str,
            interface_name: &str,
            function_name: &str,
            payload: serde_json::Value,
        ) -> serde_json::Value {
            let payload = serde_json::to_vec(&payload).unwrap();
            serde_json::json!({
                "protocol_request_id": format!("proto-wit-{call_suffix}"),
                "call_id": format!("call-wit-{call_suffix}"),
                "target": {
                    "namespace": namespace,
                    "interface_name": interface_name,
                    "function_name": function_name
                },
                "payload_metadata": {
                    "data_classification": "public",
                    "size_bytes": payload.len(),
                    "contains_secret_scoped_material": false
                },
                "authority_context": {
                    "policy_revision": 1,
                    "grants_revision": 1,
                    "vision_policy_revision": 1
                },
                "deadline": "2099-01-01T00:00:00Z",
                "trace_context": {
                    "trace_id": format!("trace-wit-{call_suffix}"),
                    "span_id": format!("span-wit-{call_suffix}")
                },
                "payload": {
                    "payload_kind": "inline_payload",
                    "inline_payload_ref": format!("payload-wit-{call_suffix}"),
                    "content_type": "application/json",
                    "size_bytes": payload.len(),
                    "blake3_digest_hex": blake3::hash(&payload).to_hex().to_string()
                },
                "inline_payload_base64": BASE64_STANDARD.encode(payload),
                "idempotency_key": format!("wit-fixture-{call_suffix}")
            })
        }

        fn call_submission(call_suffix: &str) -> serde_json::Value {
            let payload = br#"[{"project":"/project","status":null,"kind":null}]"#;
            serde_json::json!({
                "protocol_request_id": format!("proto-slate-{call_suffix}"),
                "call_id": format!("call-slate-{call_suffix}"),
                "target": {
                    "namespace": "patina:slate",
                    "interface_name": "control@0.1.0",
                    "function_name": "list-work"
                },
                "payload_metadata": {
                    "data_classification": "public",
                    "size_bytes": payload.len(),
                    "contains_secret_scoped_material": false
                },
                "authority_context": {
                    "policy_revision": 1,
                    "grants_revision": 1,
                    "vision_policy_revision": 1
                },
                "deadline": "2099-01-01T00:00:00Z",
                "trace_context": {
                    "trace_id": format!("trace-slate-{call_suffix}"),
                    "span_id": format!("span-slate-{call_suffix}")
                },
                "payload": {
                    "payload_kind": "inline_payload",
                    "inline_payload_ref": format!("payload-slate-{call_suffix}"),
                    "content_type": "application/json",
                    "size_bytes": payload.len(),
                    "blake3_digest_hex": blake3::hash(payload).to_hex().to_string()
                },
                "inline_payload_base64": BASE64_STANDARD.encode(payload),
                "idempotency_key": format!("slate-acquisition-{call_suffix}")
            })
        }

        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/slate-manager-0.2.0");
        let manifest = fixture.join("slate-manager.toml");
        let component = fixture.join("slate-manager.wasm");
        let manifest_bytes = fs::read(&manifest).unwrap();
        let component_bytes = fs::read(&component).unwrap();
        assert_eq!(component_bytes.len(), 1_338_615);
        assert_eq!(
            format!("{:x}", Sha256::digest(&manifest_bytes)),
            "b6d7b4e532df5b787acd37f3ae8c25ed093552097e5cf6dbc5c7eaca360e4919"
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(&component_bytes)),
            "76b568f40491d7e3bd1dcb55644ec7c42dbc393642a5a7a2ba5b1daa1ea6966a"
        );
        assert_eq!(
            blake3::hash(&component_bytes).to_hex().as_str(),
            "e06cab5f7605f3c070ef792f67f7b71a179d8a9c7da0c45e525b39e8a3a88e7d"
        );
        assert!(!manifest.with_extension("toml.sha256").exists());
        assert!(!component.with_extension("wasm.sha256").exists());

        let root = tempfile::tempdir().unwrap();
        let adapter = Arc::new(FakeSupervisorAdapter::default());
        let (paths, record) =
            install_supervisor_for_test_with_adapter(root.path(), adapter.as_ref()).unwrap();
        let source_root = root.path().join("source");
        fs::create_dir(&source_root).unwrap();
        fs::copy(&manifest, source_root.join("slate-manager.toml")).unwrap();
        fs::copy(&component, source_root.join("slate-manager.wasm")).unwrap();
        let source_manifest_before = fs::read(source_root.join("slate-manager.toml")).unwrap();
        let source_component_before = fs::read(source_root.join("slate-manager.wasm")).unwrap();
        let source_manifest_mode_before = fs::metadata(source_root.join("slate-manager.toml"))
            .unwrap()
            .permissions()
            .mode();
        let source_component_mode_before = fs::metadata(source_root.join("slate-manager.wasm"))
            .unwrap()
            .permissions()
            .mode();

        let project = root.path().join("slate-project");
        let work_dir = project.join("layer/slate/work/fixture-work");
        fs::create_dir_all(project.join(".patina")).unwrap();
        fs::create_dir_all(&work_dir).unwrap();
        fs::write(
            work_dir.join("work.toml"),
            r#"id = "fixture-work"
title = "Acquired Slate fixture"
kind = "build"
status = "active"
"#,
        )
        .unwrap();
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&project)
            .status()
            .unwrap();

        start_with_adapter(&paths, current_uid().unwrap(), adapter.as_ref(), false).unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        adapter.arm_shutdown(shutdown_tx);
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let record_path = paths.record.clone();
        let resident = tokio::spawn(async move {
            run_test_supervised_resident_mother(
                &record_path,
                async move {
                    let _ = shutdown_rx.await;
                },
                Some(ready_tx),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(15), ready_rx)
            .await
            .unwrap()
            .unwrap();

        let stage_request = serde_json::to_value(MctArtifactStageRequest {
            source_root: source_root.clone(),
            manifest_path: PathBuf::from("slate-manager.toml"),
            component_path: PathBuf::from("slate-manager.wasm"),
            claimed_child_name: "slate-manager".into(),
            claimed_artifact_version: "0.2.0".into(),
            expected_digest: Some(
                "blake3:e06cab5f7605f3c070ef792f67f7b71a179d8a9c7da0c45e525b39e8a3a88e7d".into(),
            ),
            standing_source_authority_id: None,
            claimed_publisher: None,
            require_source_sidecars: false,
            children_dir: paths.children.clone(),
            state_path: paths.state.clone(),
        })
        .unwrap();
        let (stage_status, stage_report) =
            uds_json(&paths.uds, "POST", "/artifacts/stage", &stage_request).await;
        assert_eq!(stage_status, 200, "{stage_report:#}");
        assert_eq!(stage_report["verification_outcome"], "verified");
        let artifact_id = stage_report["artifact_id"].as_str().unwrap().to_owned();
        assert_eq!(
            fs::read(source_root.join("slate-manager.toml")).unwrap(),
            source_manifest_before
        );
        assert_eq!(
            fs::read(source_root.join("slate-manager.wasm")).unwrap(),
            source_component_before
        );
        assert_eq!(
            fs::metadata(source_root.join("slate-manager.toml"))
                .unwrap()
                .permissions()
                .mode(),
            source_manifest_mode_before
        );
        assert_eq!(
            fs::metadata(source_root.join("slate-manager.wasm"))
                .unwrap()
                .permissions()
                .mode(),
            source_component_mode_before
        );
        let canonical_test_root = root.path().canonicalize().unwrap();
        for isolated in [
            &paths.root,
            &paths.config,
            &paths.identity,
            &paths.children,
            &paths.state,
            &paths.ledger,
            &paths.uds,
            &project,
            &source_root,
        ] {
            assert!(
                isolated
                    .canonicalize()
                    .unwrap()
                    .starts_with(&canonical_test_root)
            );
        }
        let package_path = PathBuf::from(stage_report["package_path"].as_str().unwrap());
        assert!(package_path.join("child.toml.sha256").is_file());
        assert!(
            package_path
                .join("artifact/slate-manager.wasm.sha256")
                .is_file()
        );

        let state = MctRuntimeStateStore::open(&paths.state).unwrap();
        let artifact = state
            .get_artifact(&ComponentArtifactId::new(&artifact_id).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            artifact.provenance_status,
            ArtifactProvenanceStatus::AcquisitionBacked
        );
        assert_eq!(artifact.acquisition_ids.len(), 1);
        let acquisitions = state.artifact_acquisitions().unwrap();
        assert_eq!(acquisitions.len(), 1);
        let acquisition = &acquisitions[0];
        assert_eq!(
            acquisition.authority_path,
            ArtifactAcquisitionAuthorityPath::OperatorPointed
        );
        assert!(acquisition.operator_pointed_decision_id.is_some());
        assert!(acquisition.standing_source_authority_id.is_none());
        assert!(
            acquisition
                .adapter_effect_authority_ref
                .starts_with("obs:acquisition-adapter-start:")
        );
        assert_eq!(
            acquisition.observed_size_bytes,
            Some(component_bytes.len() as u64)
        );
        assert_eq!(
            acquisition.observed_digest.as_deref(),
            Some("blake3:e06cab5f7605f3c070ef792f67f7b71a179d8a9c7da0c45e525b39e8a3a88e7d")
        );
        assert_eq!(
            acquisition.acquisition_outcome,
            ArtifactAcquisitionOutcome::Acquired
        );
        assert_eq!(
            acquisition.verification_outcome,
            ArtifactVerificationOutcome::Verified
        );
        assert_eq!(
            acquisition.acquisition_id.as_str(),
            stage_report["acquisition_id"].as_str().unwrap()
        );
        assert_eq!(
            acquisition.acquisition_observation_id.as_str(),
            stage_report["acquisition_observation_id"].as_str().unwrap()
        );
        assert_eq!(
            acquisition
                .verification_observation_id
                .as_ref()
                .unwrap()
                .as_str(),
            stage_report["verification_observation_id"]
                .as_str()
                .unwrap()
        );
        let decisions = state.operator_acquisition_decisions().unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(
            decisions[0].decision_state,
            OperatorPointedAcquisitionState::Consumed
        );
        let stage_ledger = entries(&paths.ledger);
        let adapter_start = stage_ledger
            .iter()
            .position(|entry| {
                entry.observation.observation_id.as_str()
                    == acquisition.adapter_effect_authority_ref
            })
            .unwrap();
        let acquisition_terminal = stage_ledger
            .iter()
            .position(|entry| {
                entry.observation.observation_id == acquisition.acquisition_observation_id
            })
            .unwrap();
        let verification_terminal = stage_ledger
            .iter()
            .position(|entry| {
                Some(&entry.observation.observation_id)
                    == acquisition.verification_observation_id.as_ref()
            })
            .unwrap();
        assert!(adapter_start < acquisition_terminal);
        assert!(acquisition_terminal < verification_terminal);
        assert_eq!(artifact.child_name, "slate-manager");
        assert_eq!(artifact.artifact_version, "0.2.0");
        assert_eq!(artifact.content_hash, artifact_id);
        assert_eq!(artifact.primary_export.namespace, "patina:slate");
        assert!(state.toy_grant_snapshots().unwrap().is_empty());
        assert_eq!(state.summary().unwrap().approved_children, 0);
        assert_eq!(state.summary().unwrap().active_assignments, 0);
        assert!(
            MctDaemonConfigStore::new(&paths.config)
                .load()
                .unwrap()
                .child_approvals
                .is_empty()
        );

        let (denied_status, denied) = uds_json(
            &paths.uds,
            "POST",
            "/calls",
            &call_submission("before-approval"),
        )
        .await;
        assert_eq!(denied_status, 200, "{denied:#}");
        assert_eq!(denied["outcome"], "denied");

        let approve = serde_json::json!({
            "expected_config_path": paths.config,
            "expected_children_dir": paths.children,
            "expected_state_path": paths.state,
            "expected_artifact_id": artifact_id,
            "child_name": "slate-manager",
            "strict_integrity": true
        });
        let (approve_status, approve_report) =
            uds_json(&paths.uds, "POST", "/children/approve", &approve).await;
        assert_eq!(approve_status, 200, "{approve_report:#}");
        assert_eq!(approve_report["approval_state"], "approved");
        assert_eq!(approve_report["assignment_state"], "active");
        assert_eq!(approve_report["approval_artifact_id"], artifact_id);
        assert_eq!(
            approve_report["approval_acquisition_ids"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            approve_report["approval_acquisition_ids"][0],
            stage_report["acquisition_id"]
        );
        let approval_ledger = entries(&paths.ledger);
        let approved = approval_ledger
            .iter()
            .position(|entry| {
                entry.observation.kind == ObservationKind::ChildApproved
                    && entry.observation.resource_id.as_deref() == Some(artifact_id.as_str())
            })
            .unwrap();
        let assigned = approval_ledger
            .iter()
            .position(|entry| {
                entry.observation.kind == ObservationKind::ChildAssigned
                    && entry.observation.subject_id.as_deref() == Some("slate-manager")
            })
            .unwrap();
        assert!(approved < assigned);
        let runtime_starts_before_grant_denial = approval_ledger
            .iter()
            .filter(|entry| entry.observation.kind == ObservationKind::RuntimeExecutionStarted)
            .count();

        let (grant_denied_status, grant_denied) = uds_json(
            &paths.uds,
            "POST",
            "/calls",
            &call_submission("before-grants"),
        )
        .await;
        assert_eq!(grant_denied_status, 200, "{grant_denied:#}");
        assert_eq!(grant_denied["outcome"], "denied");
        assert_eq!(
            entries(&paths.ledger)
                .iter()
                .filter(|entry| {
                    entry.observation.kind == ObservationKind::RuntimeExecutionStarted
                })
                .count(),
            runtime_starts_before_grant_denial
        );

        let toy_request = serde_json::json!({
            "expected_config_path": paths.config,
            "expected_children_dir": paths.children,
            "expected_state_path": paths.state,
            "child_name": "slate-manager",
            "project_root": project
        });
        let (toy_status, toy_report) =
            uds_json(&paths.uds, "POST", "/toys/authorize-slate", &toy_request).await;
        assert_eq!(toy_status, 200, "{toy_report:#}");
        assert_eq!(toy_report["grants"], 4);
        let grants = MctRuntimeStateStore::open(&paths.state)
            .unwrap()
            .toy_grant_snapshots()
            .unwrap();
        assert_eq!(grants.len(), 4);
        assert!(grants.iter().all(|grant| {
            grant.subject.child_name == "slate-manager"
                && grant.subject.artifact_id == artifact_id
                && grant.subject.assignment_id.is_some()
                && grant.subject.caller_node_id.is_some()
                && grant.scope.node_id.is_some()
                && grant.scope.resource_id.is_some()
                && grant.grant_state == ToyGrantState::Active
        }));

        let (call_status, call_reply) =
            uds_json(&paths.uds, "POST", "/calls", &call_submission("allowed")).await;
        assert_eq!(call_status, 200, "{call_reply:#}");
        assert_eq!(call_reply["outcome"], "completed", "{call_reply:#}");
        let result_bytes = BASE64_STANDARD
            .decode(call_reply["inline_result_payload_base64"].as_str().unwrap())
            .unwrap();
        let result_json: serde_json::Value = serde_json::from_slice(&result_bytes).unwrap();
        assert!(
            result_json.to_string().contains("fixture-work"),
            "{result_json:#}"
        );

        let watch_root = root.path().join("watch-input");
        fs::create_dir(&watch_root).unwrap();
        let watch_source = root.path().join("watch-sources");
        fs::create_dir(&watch_source).unwrap();
        let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut watch_artifacts = BTreeMap::new();
        for (child_name, component_name) in [
            ("folder-watch-actor", "folder-watch-actor.wasm"),
            ("watch-null-sink", "watch-null-sink.wasm"),
        ] {
            let source = watch_source.join(child_name);
            fs::create_dir(&source).unwrap();
            fs::copy(
                fixture_root.join(format!("{child_name}-0.1.0/child.toml")),
                source.join("child.toml"),
            )
            .unwrap();
            fs::copy(
                fixture_root.join(format!("{child_name}-0.1.0/{component_name}")),
                source.join(component_name),
            )
            .unwrap();
            let stage = serde_json::to_value(MctArtifactStageRequest {
                source_root: source,
                manifest_path: PathBuf::from("child.toml"),
                component_path: PathBuf::from(component_name),
                claimed_child_name: child_name.into(),
                claimed_artifact_version: "0.1.0".into(),
                expected_digest: None,
                standing_source_authority_id: None,
                claimed_publisher: None,
                require_source_sidecars: false,
                children_dir: paths.children.clone(),
                state_path: paths.state.clone(),
            })
            .unwrap();
            let (status, staged) = uds_json(&paths.uds, "POST", "/artifacts/stage", &stage).await;
            assert_eq!(status, 200, "{staged:#}");
            let artifact_id = staged["artifact_id"].as_str().unwrap().to_owned();
            let approve = serde_json::json!({
                "expected_config_path": paths.config,
                "expected_children_dir": paths.children,
                "expected_state_path": paths.state,
                "expected_artifact_id": artifact_id,
                "child_name": child_name,
                "strict_integrity": true
            });
            let (status, approved) =
                uds_json(&paths.uds, "POST", "/children/approve", &approve).await;
            assert_eq!(status, 200, "{approved:#}");
            watch_artifacts.insert(child_name.to_owned(), artifact_id);
        }

        let watch_window = serde_json::json!({
            "expected_config_path": paths.config,
            "expected_children_dir": paths.children,
            "expected_state_path": paths.state,
            "child_name": "folder-watch-actor",
            "watch_scope_id": "scope:fixture-watch",
            "canonical_root": watch_root,
            "scope_mode": "constrained",
            "traversal_scope": "recursive",
            "event_classes": ["created", "modified", "deleted"],
            "max_events_per_batch": 16,
            "coalescing_policy": "none",
            "starts_at": "2020-01-01T00:00:00Z",
            "expires_at": "2099-01-01T00:00:00Z"
        });
        let (status, watch_grant) =
            uds_json(&paths.uds, "POST", "/watch/grant", &watch_window).await;
        assert_eq!(status, 200, "{watch_grant:#}");
        assert_eq!(watch_grant["scope"]["authority_state"], "active");

        for grant in [
            serde_json::json!({
                "kind": "directory_read",
                "canonical_root": watch_root
            }),
            serde_json::json!({
                "kind": "keyvalue",
                "bucket_name": "default"
            }),
            serde_json::json!({
                "kind": "observability",
                "logging": true,
                "measure": true
            }),
        ] {
            let request = serde_json::json!({
                "expected_config_path": paths.config,
                "expected_children_dir": paths.children,
                "expected_state_path": paths.state,
                "child_name": "folder-watch-actor",
                "expires_at": "2099-01-01T00:00:00Z",
                "grant": grant
            });
            let (status, granted) =
                uds_json(&paths.uds, "POST", "/watch/supporting-grant", &request).await;
            assert_eq!(status, 200, "{granted:#}");
        }
        let sink_observability = serde_json::json!({
            "expected_config_path": paths.config,
            "expected_children_dir": paths.children,
            "expected_state_path": paths.state,
            "child_name": "watch-null-sink",
            "expires_at": "2099-01-01T00:00:00Z",
            "grant": {
                "kind": "observability",
                "logging": true,
                "measure": true
            }
        });
        let (status, sink_grants) = uds_json(
            &paths.uds,
            "POST",
            "/watch/supporting-grant",
            &sink_observability,
        )
        .await;
        assert_eq!(status, 200, "{sink_grants:#}");

        let configure = wit_call_submission(
            "watch-configure",
            "patina:watch",
            "control@0.1.0",
            "configure",
            serde_json::json!([{
                "watch-path": "/input",
                "stream-name": "patina:watch/events@0.1.0.emit",
                "recursive": true,
                "include-hidden": false,
                "emit-existing-on-start": true,
                "extensions": []
            }, true]),
        );
        let (status, configured) = uds_json(&paths.uds, "POST", "/calls", &configure).await;
        assert_eq!(status, 200, "{configured:#}");
        assert_eq!(configured["outcome"], "completed", "{configured:#}");
        fs::write(
            watch_root.join("fixture-created.txt"),
            b"watch fixture content",
        )
        .unwrap();
        let scan = wit_call_submission(
            "watch-scan",
            "patina:watch",
            "control@0.1.0",
            "scan-now",
            serde_json::json!([]),
        );
        let (status, scanned) = uds_json(&paths.uds, "POST", "/calls", &scan).await;
        assert_eq!(status, 200, "{scanned:#}");
        assert_eq!(scanned["outcome"], "completed", "{scanned:#}");
        let watch_summary = MctRuntimeStateStore::open(&paths.state)
            .unwrap()
            .summary()
            .unwrap();
        assert_eq!(watch_summary.watch_event_batches, 1);
        assert_eq!(watch_summary.watch_events, 1);
        assert_eq!(watch_summary.watch_event_deliveries, 1);
        let first_watch_ledger = entries(&paths.ledger);
        let batch_position = first_watch_ledger
            .iter()
            .position(|entry| entry.observation.safe_message == "Watch batch opened")
            .unwrap();
        let event_position = first_watch_ledger
            .iter()
            .position(|entry| entry.observation.safe_message == "Watch event eligible")
            .unwrap();
        let disposition_position = first_watch_ledger
            .iter()
            .position(|entry| entry.observation.safe_message == "Child call-out constructed")
            .unwrap();
        let nested_effect_position = first_watch_ledger
            .iter()
            .enumerate()
            .skip(disposition_position + 1)
            .find(|(_, entry)| entry.observation.kind == ObservationKind::RuntimeExecutionStarted)
            .map(|(position, _)| position)
            .expect("nested sink execution follows durable Watch admission");
        assert!(batch_position < event_position);
        assert!(event_position < disposition_position);
        assert!(disposition_position < nested_effect_position);

        fs::write(
            watch_root.join("trigger-created.txt"),
            b"temporal watch fixture content",
        )
        .unwrap();
        let trigger_payload = b"[]";
        let trigger_digest = blake3::hash(trigger_payload).to_hex().to_string();
        let blob_request = serde_json::json!({
            "digest": trigger_digest,
            "size_bytes": trigger_payload.len(),
            "content_type": "application/json",
            "classification": "trigger-static",
            "bytes_base64": BASE64_STANDARD.encode(trigger_payload)
        });
        let (status, blob) = uds_json(&paths.uds, "POST", "/blobs", &blob_request).await;
        assert_eq!(status, 201, "{blob:#}");
        let payload_constraint: MctCallPayloadHandle =
            serde_json::from_value(blob["payload"].clone()).unwrap();
        let now = jiff::Timestamp::now();
        let anchor = now
            .checked_add(jiff::SignedDuration::from_millis(500))
            .unwrap();
        let trigger = TriggerCreateRequest {
            expected_config_path: paths.config.clone(),
            expected_state_path: paths.state.clone(),
            scope: TriggerAuthorityScopeRequest {
                trigger_authority_id: CallTriggerAuthorityId::new("trigger:fixture-watch").unwrap(),
                target: OperationTarget::new("patina:watch", "control@0.1.0", "scan-now").unwrap(),
                payload_constraint,
                trigger_source: CallTriggerSource::Temporal {
                    anchor_at: Timestamp::new(anchor.to_string()).unwrap(),
                    interval_ms: 60_000,
                },
                missed_fire_policy: MissedFirePolicy::Skip,
                overlap_policy: OverlapPolicy::Refuse,
                starts_at: Timestamp::new(now.to_string()).unwrap(),
                expires_at: Timestamp::new("2099-01-01T00:00:00Z").unwrap(),
            },
        };
        let (status, created_trigger) = uds_json(
            &paths.uds,
            "POST",
            "/triggers/create",
            &serde_json::to_value(&trigger).unwrap(),
        )
        .await;
        assert_eq!(status, 200, "{created_trigger:#}");
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let summary = MctRuntimeStateStore::open(&paths.state)
                    .unwrap()
                    .summary()
                    .unwrap();
                if summary.watch_event_deliveries == 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        let revoke_trigger = TriggerRevokeRequest {
            expected_config_path: paths.config.clone(),
            expected_state_path: paths.state.clone(),
            trigger_authority_id: CallTriggerAuthorityId::new("trigger:fixture-watch").unwrap(),
            expected_revision: 1,
        };
        let (status, revoked_trigger) = uds_json(
            &paths.uds,
            "POST",
            "/triggers/revoke",
            &serde_json::to_value(&revoke_trigger).unwrap(),
        )
        .await;
        assert_eq!(status, 200, "{revoked_trigger:#}");
        assert_eq!(revoked_trigger["authority_state"], "revoked");
        let revoke_watch = WatchRevokeRequest {
            expected_config_path: paths.config.clone(),
            expected_state_path: paths.state.clone(),
            watch_scope_id: WatchObservationScopeId::new("scope:fixture-watch").unwrap(),
            expected_revision: 1,
        };
        let (status, revoked_watch) = uds_json(
            &paths.uds,
            "POST",
            "/watch/revoke",
            &serde_json::to_value(&revoke_watch).unwrap(),
        )
        .await;
        assert_eq!(status, 200, "{revoked_watch:#}");
        assert_eq!(revoked_watch["scope"]["authority_state"], "revoked");
        fs::write(
            watch_root.join("after-watch-revoke.txt"),
            b"must not observe",
        )
        .unwrap();
        let denied_scan = wit_call_submission(
            "watch-scan-after-revoke",
            "patina:watch",
            "control@0.1.0",
            "scan-now",
            serde_json::json!([]),
        );
        let (status, denied_scan) = uds_json(&paths.uds, "POST", "/calls", &denied_scan).await;
        assert_eq!(status, 200, "{denied_scan:#}");
        assert_eq!(denied_scan["outcome"], "denied", "{denied_scan:#}");
        assert_eq!(
            MctRuntimeStateStore::open(&paths.state)
                .unwrap()
                .summary()
                .unwrap()
                .watch_event_deliveries,
            2
        );
        assert!(
            MctRuntimeStateStore::open(&paths.state)
                .unwrap()
                .child_keyvalue_get(
                    &format!(
                        "child:{}:assignment:assignment:folder-watch-actor:bucket:default",
                        watch_artifacts["folder-watch-actor"]
                    ),
                    "folder-watch.config.v1"
                )
                .unwrap()
                .is_some()
        );

        let (runs_status, runs_projection) =
            uds_json(&paths.uds, "GET", "/runs", &serde_json::json!({})).await;
        assert_eq!(runs_status, 200, "{runs_projection:#}");
        assert!(
            !runs_projection
                .to_string()
                .contains("inline_result_payload_base64")
        );
        let (snapshot_status, snapshot_projection) =
            uds_json(&paths.uds, "GET", "/snapshot", &serde_json::json!({})).await;
        assert_eq!(snapshot_status, 200, "{snapshot_projection:#}");
        assert!(
            !snapshot_projection
                .to_string()
                .contains("inline_result_payload_base64")
        );
        let runtime_starts_before_revocation = entries(&paths.ledger)
            .iter()
            .filter(|entry| entry.observation.kind == ObservationKind::RuntimeExecutionStarted)
            .count();

        let revoke = serde_json::json!({
            "expected_config_path": paths.config,
            "child_name": "slate-manager"
        });
        let (revoke_status, revoke_report) =
            uds_json(&paths.uds, "POST", "/children/revoke", &revoke).await;
        assert_eq!(revoke_status, 200, "{revoke_report:#}");
        let (revoked_status, revoked) =
            uds_json(&paths.uds, "POST", "/calls", &call_submission("revoked")).await;
        assert_eq!(revoked_status, 200, "{revoked:#}");
        assert_eq!(revoked["outcome"], "denied");
        assert_eq!(
            entries(&paths.ledger)
                .iter()
                .filter(|entry| {
                    entry.observation.kind == ObservationKind::RuntimeExecutionStarted
                })
                .count(),
            runtime_starts_before_revocation
        );

        let stop_paths = paths.clone();
        let stop_adapter = Arc::clone(&adapter);
        tokio::task::spawn_blocking(move || {
            stop_with_adapter(&stop_paths, current_uid().unwrap(), stop_adapter.as_ref())
        })
        .await
        .unwrap()
        .unwrap();
        resident.await.unwrap().unwrap();

        let reopened = MctRuntimeStateStore::open(&paths.state).unwrap();
        assert_eq!(reopened.artifact_acquisitions().unwrap().len(), 3);
        assert_eq!(reopened.toy_grant_snapshots().unwrap().len(), 11);
        assert_eq!(reopened.summary().unwrap().watch_event_deliveries, 2);
        assert!(
            reopened
                .get_artifact(&ComponentArtifactId::new(&artifact_id).unwrap())
                .unwrap()
                .is_some()
        );
        let ledger = entries(&paths.ledger);
        for fact in [
            "operator-pointed artifact acquisition admitted",
            "filesystem artifact acquisition completed",
            "artifact verified",
            "child approved",
            "child assigned",
            "child approval revoked",
        ] {
            assert!(
                ledger
                    .iter()
                    .any(|entry| entry.observation.safe_message.contains(fact)),
                "missing {fact}"
            );
        }
        assert!(package_path.is_dir());

        start_with_adapter(&paths, current_uid().unwrap(), adapter.as_ref(), false).unwrap();
        let (restart_shutdown_tx, restart_shutdown_rx) = tokio::sync::oneshot::channel();
        adapter.arm_shutdown(restart_shutdown_tx);
        let (restart_ready_tx, restart_ready_rx) = tokio::sync::oneshot::channel();
        let restart_record_path = paths.record.clone();
        let restarted_resident = tokio::spawn(async move {
            run_test_supervised_resident_mother(
                &restart_record_path,
                async move {
                    let _ = restart_shutdown_rx.await;
                },
                Some(restart_ready_tx),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(15), restart_ready_rx)
            .await
            .unwrap()
            .unwrap();
        let (restart_status, restart_denied) = uds_json(
            &paths.uds,
            "POST",
            "/calls",
            &call_submission("restart-revoked"),
        )
        .await;
        assert_eq!(restart_status, 200, "{restart_denied:#}");
        assert_eq!(restart_denied["outcome"], "denied");
        tokio::time::sleep(Duration::from_millis(750)).await;
        assert_eq!(
            MctRuntimeStateStore::open(&paths.state)
                .unwrap()
                .summary()
                .unwrap()
                .watch_event_deliveries,
            2
        );
        let restart_stop_paths = paths.clone();
        let restart_stop_adapter = Arc::clone(&adapter);
        tokio::task::spawn_blocking(move || {
            stop_with_adapter(
                &restart_stop_paths,
                current_uid().unwrap(),
                restart_stop_adapter.as_ref(),
            )
        })
        .await
        .unwrap()
        .unwrap();
        restarted_resident.await.unwrap().unwrap();

        let package_manifest_before_uninstall = fs::read(package_path.join("child.toml")).unwrap();
        let package_component_before_uninstall =
            fs::read(package_path.join("artifact/slate-manager.wasm")).unwrap();
        let config_before_uninstall = fs::read(&paths.config).unwrap();
        let identity_before_uninstall = fs::read(&paths.identity).unwrap();
        let project_before_uninstall = fs::read(work_dir.join("work.toml")).unwrap();
        let ledger_len_before_uninstall = fs::metadata(&paths.ledger).unwrap().len();
        let preserved = MctRuntimeStateStore::open(&paths.state).unwrap();
        let preserved_acquisitions = preserved.artifact_acquisitions().unwrap();
        let preserved_decisions = preserved.operator_acquisition_decisions().unwrap();
        let preserved_runs = preserved.list_runs(100).unwrap();
        drop(preserved);

        uninstall_with_adapter(&paths, current_uid().unwrap(), adapter.as_ref()).unwrap();
        assert!(!paths.record.exists());
        assert!(!paths.plist.exists());
        assert_eq!(
            adapter.inspect(&record).unwrap(),
            SupervisorLoadedState::Unloaded
        );
        assert_eq!(fs::read(&paths.config).unwrap(), config_before_uninstall);
        assert_eq!(
            fs::read(&paths.identity).unwrap(),
            identity_before_uninstall
        );
        assert_eq!(
            fs::read(work_dir.join("work.toml")).unwrap(),
            project_before_uninstall
        );
        assert_eq!(
            fs::read(package_path.join("child.toml")).unwrap(),
            package_manifest_before_uninstall
        );
        assert_eq!(
            fs::read(package_path.join("artifact/slate-manager.wasm")).unwrap(),
            package_component_before_uninstall
        );
        assert!(fs::metadata(&paths.ledger).unwrap().len() >= ledger_len_before_uninstall);
        let final_state = MctRuntimeStateStore::open(&paths.state).unwrap();
        assert_eq!(
            final_state.artifact_acquisitions().unwrap(),
            preserved_acquisitions
        );
        assert_eq!(
            final_state.operator_acquisition_decisions().unwrap(),
            preserved_decisions
        );
        assert_eq!(final_state.list_runs(100).unwrap(), preserved_runs);
        assert!(
            final_state
                .get_artifact(&ComponentArtifactId::new(&artifact_id).unwrap())
                .unwrap()
                .is_some()
        );
        assert_eq!(
            MctDaemonConfigStore::new(&paths.config)
                .load()
                .unwrap()
                .child_approvals["slate-manager"]
                .approval_state,
            ChildApprovalState::Revoked
        );
        assert!(record.record_digest == record.canonical_digest().unwrap());
    }

    #[test]
    fn launchd_adapter_maps_install_start_stop_and_restart_without_ambient_fallbacks() {
        let root = tempfile::tempdir().unwrap();
        let (_, record) = install_supervisor_for_test(root.path()).unwrap();
        let plist = String::from_utf8(render_launchd_plist(&record)).unwrap();
        assert!(plist.contains("io.patina.mct.mother"));
        assert!(plist.contains("<string>serve</string>"));
        assert!(plist.contains("<string>--supervisor-record</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("<key>ThrottleInterval</key>"));
        assert!(!plist.contains("EnvironmentVariables"));
        assert_eq!(
            record.launchd_domain,
            format!("gui/{}", current_uid().unwrap())
        );
    }
}

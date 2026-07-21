use crate::{
    MctDaemonConfig, MctLoadedChild, MctOperatorChildScope, MctPeerAddressBookEntry,
    current_timestamp_string,
};
use anyhow::{Context, Result, bail};
use mct_kernel::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: i64 = 10;

pub const MCT_IDEMPOTENCY_TTL_SECONDS: i64 = 12 * 60;
pub const MCT_IDEMPOTENCY_MAX_ENTRIES_PER_CALLER: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctRecordedCallReply {
    pub result_ref: Option<ResultRef>,
    pub result_payload: MctCallPayloadHandle,
    #[serde(skip)]
    pub inline_result_payload: Option<Vec<u8>>,
    pub route_decision_id: Option<DecisionId>,
    pub route_taken: Option<RouteTaken>,
    pub outcome: CallProtocolOutcome,
    pub protocol_reason: Option<CallProtocolReason>,
    pub safe_message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MctIdempotencyReservation {
    ExecuteFresh,
    Replay(Box<MctRecordedCallReply>),
    Refused(MctIdempotencyReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctTriggerOccurrenceDisposition {
    Pending,
    Fired,
    Skipped,
    Suppressed,
    CapacityRefused,
}

impl MctTriggerOccurrenceDisposition {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Fired | Self::Skipped | Self::Suppressed | Self::CapacityRefused
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctTriggerOccurrenceRecord {
    pub occurrence_id: CallTriggerOccurrenceId,
    pub trigger_authority_id: CallTriggerAuthorityId,
    pub record_revision: u64,
    pub nominal_at: Option<Timestamp>,
    pub represented_set: CallTriggerRepresentedSet,
    pub missed_fire_result: String,
    pub overlap_result: Option<String>,
    pub final_disposition: MctTriggerOccurrenceDisposition,
    pub disposition_observation_id: ObservationId,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctTriggerPendingOccurrenceRecord {
    pub pending_occurrence_id: CallTriggerPendingOccurrenceId,
    pub occurrence_id: CallTriggerOccurrenceId,
    pub trigger_authority_id: CallTriggerAuthorityId,
    pub record_revision: u64,
    pub policy_revision: u64,
    pub admission_sequence: u64,
    pub pending_reason: CallTriggerPendingReason,
    pub represented_set: CallTriggerRepresentedSet,
    pub admission_observation_id: ObservationId,
    pub state: String,
    pub admitted_at: Timestamp,
    pub consumed_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctTriggerFiringRecord {
    pub firing_id: CallTriggerFiringId,
    pub occurrence_id: CallTriggerOccurrenceId,
    pub trigger_authority_id: CallTriggerAuthorityId,
    pub record_revision: u64,
    pub policy_revision: u64,
    pub call_id: CallId,
    pub idempotency_key_ref: String,
    pub firing_observation_id: ObservationId,
    pub target_result_ref: Option<ResultRef>,
    pub state: String,
    pub fired_at: Timestamp,
    pub completed_at: Option<Timestamp>,
}

/// Project-local durable runtime state for one standalone MCT node.
///
/// This is an adapter: it enforces storage invariants and persists facts, but it
/// does not create authority. Callers still need kernel authorization facts such
/// as `ChildApproval`, `ChildAssignment`, and `ChildInstance`, plus freshly
/// evaluated invocation and toy-call capabilities before effects run.
#[derive(Debug)]
pub struct MctRuntimeStateStore {
    path: PathBuf,
    conn: Connection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctRuntimeStateSummary {
    pub schema_version: i64,
    pub artifacts: u64,
    pub approved_children: u64,
    pub active_assignments: u64,
    pub ready_instances: u64,
    pub peers: u64,
    pub runs: u64,
    pub completed_runs: u64,
    pub failed_runs: u64,
    pub metric_points: u64,
    pub queued_tasks: u64,
    pub child_state_keys: u64,
    pub child_subscriptions: u64,
    pub toy_catalog_contracts: u64,
    pub toy_grant_snapshots: u64,
    #[serde(default)]
    pub trigger_records: u64,
    #[serde(default)]
    pub current_trigger_records: u64,
    #[serde(default)]
    pub pending_trigger_occurrences: u64,
    #[serde(default)]
    pub active_trigger_firings: u64,
    #[serde(default)]
    pub watch_scope_records: u64,
    #[serde(default)]
    pub current_watch_scopes: u64,
    #[serde(default)]
    pub watch_event_batches: u64,
    #[serde(default)]
    pub watch_events: u64,
    #[serde(default)]
    pub watch_event_deliveries: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctRemoteCallableSurfaceRecord {
    pub peer_node_id: MctNodeId,
    pub binding_id: PeerBindingId,
    pub endpoint_id: EndpointIdText,
    pub vision_id: VisionId,
    pub publisher_policy_revision: u64,
    pub child_name: String,
    pub operation_id: String,
    pub runtime_kind: RuntimeKind,
    pub surface_policy_revision: u64,
    pub visibility: String,
    pub received_at: Timestamp,
    pub stale_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctRemoteSurfaceRefresh<'a> {
    pub peer_node_id: &'a MctNodeId,
    pub binding_id: &'a PeerBindingId,
    pub endpoint_id: &'a EndpointIdText,
    pub view: &'a MctHelloCapabilityView,
    pub received_at: &'a Timestamp,
    pub stale_at: &'a Timestamp,
    pub view_observation_id: &'a ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctRuntimeRunState {
    Queued,
    Running,
    Completed,
    Failed,
    TimedOut,
    Cancelled,
    Denied,
}

impl MctRuntimeRunState {
    pub fn terminal_for_result(result: &MctResult) -> Self {
        match result.outcome {
            ResultOutcome::Success => Self::Completed,
            ResultOutcome::Denied => Self::Denied,
            ResultOutcome::Failed => Self::Failed,
            ResultOutcome::TimedOut => Self::TimedOut,
            ResultOutcome::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctTaskStatus {
    Queued,
    Leased,
    Running,
    Succeeded,
    Failed,
    DeadLetter,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctTaskIntentRecord {
    pub kind: String,
    pub payload_json: String,
    pub dedupe_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctQueuedTaskRecord {
    pub task_id: String,
    pub child_name: String,
    pub kind: String,
    pub payload_json: String,
    pub dedupe_key: Option<String>,
    pub status: MctTaskStatus,
    pub lease_owner: Option<String>,
    pub lease_until: Option<String>,
    pub attempts: u64,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildInvocationProvenance {
    pub authorized_child_invocation_id: AuthorizedChildInvocationId,
    pub call_id: CallId,
    pub evaluation_id: ChildCallEvaluationId,
    pub authority_decision_id: DecisionId,
    pub authority_observation_id: ObservationId,
    pub assignment_id: ChildAssignmentId,
    pub approval_id: ChildApprovalId,
    pub artifact_id: ComponentArtifactId,
    pub child_instance_id: ChildInstanceId,
    pub child_name: String,
}

impl ChildInvocationProvenance {
    pub fn from_authorized(
        authorized: &AuthorizedChildInvocation,
        authority_observation_id: ObservationId,
    ) -> Self {
        Self {
            authorized_child_invocation_id: authorized.authorized_child_invocation_id().clone(),
            call_id: authorized.call_id().clone(),
            evaluation_id: authorized.evaluation_id().clone(),
            authority_decision_id: authorized.authority_decision_id().clone(),
            authority_observation_id,
            assignment_id: authorized.assignment_id().clone(),
            approval_id: authorized.approval_id().clone(),
            artifact_id: authorized.artifact_id().clone(),
            child_instance_id: authorized.child_instance_id().clone(),
            child_name: authorized.child_name().to_owned(),
        }
    }

    fn from_legacy_authorized(
        authorized: LegacyAuthorizedChildInvocation,
        authority_observation_id: ObservationId,
    ) -> Self {
        Self {
            authorized_child_invocation_id: authorized.authorized_child_invocation_id,
            call_id: authorized.call_id,
            evaluation_id: authorized.evaluation_id,
            authority_decision_id: authorized.authority_decision_id,
            authority_observation_id,
            assignment_id: authorized.assignment_id,
            approval_id: authorized.approval_id,
            artifact_id: authorized.artifact_id,
            child_instance_id: authorized.child_instance_id,
            child_name: authorized.child_name,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct LegacyAuthorizedChildInvocation {
    authorized_child_invocation_id: AuthorizedChildInvocationId,
    call_id: CallId,
    evaluation_id: ChildCallEvaluationId,
    assignment_id: ChildAssignmentId,
    approval_id: ChildApprovalId,
    artifact_id: ComponentArtifactId,
    child_instance_id: ChildInstanceId,
    child_name: String,
    authority_decision_id: DecisionId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctRuntimeRunRecord {
    pub run_id: String,
    pub call_id: CallId,
    pub runtime_kind: RuntimeKind,
    pub child_name: Option<String>,
    pub child_instance_id: Option<ChildInstanceId>,
    pub authority_decision_id: Option<DecisionId>,
    pub trace_id: TraceId,
    pub state: MctRuntimeRunState,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub result: Option<MctResult>,
    pub call: MctCall,
    pub child_invocation_provenance: Option<ChildInvocationProvenance>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctMetricPoint {
    pub metric_name: String,
    pub metric_value: i64,
    pub labels: serde_json::Value,
    pub observed_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctRegistrySourceRecord {
    pub source_id: String,
    pub source_path: PathBuf,
    pub last_sync_at: Option<String>,
    pub last_loaded: u64,
    pub last_failed: u64,
    pub state: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctArtifactPackageRecord {
    pub artifact_id: ComponentArtifactId,
    pub package_path: PathBuf,
    pub published_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctCompositionRunRecord {
    pub composition_id: String,
    pub state: String,
    pub steps_json: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

impl MctRuntimeStateStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create runtime state dir {}", parent.display()))?;
        }
        let conn = Connection::open(&path)
            .with_context(|| format!("open runtime state {}", path.display()))?;
        let store = Self { path, conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS mct_state_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS component_artifacts (
                artifact_id TEXT PRIMARY KEY,
                child_name TEXT NOT NULL,
                artifact_version TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                manifest_hash TEXT NOT NULL,
                primary_export_json TEXT NOT NULL,
                runtime_shape TEXT NOT NULL,
                ingress_mode TEXT NOT NULL,
                lifecycle_exports TEXT NOT NULL,
                verification_status TEXT NOT NULL,
                provenance_status TEXT NOT NULL DEFAULT 'historical_unknown',
                acquisition_ids_json TEXT NOT NULL DEFAULT '[]',
                primary_acquisition_id TEXT,
                created_by_observation_id TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS child_approvals (
                approval_id TEXT PRIMARY KEY,
                artifact_id TEXT NOT NULL REFERENCES component_artifacts(artifact_id),
                child_name TEXT NOT NULL,
                artifact_version TEXT NOT NULL,
                scope_vision_id TEXT,
                scope_node_id TEXT,
                scope_project_id TEXT,
                approval_state TEXT NOT NULL,
                policy_revision INTEGER NOT NULL,
                authority_observation_id TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS child_assignments (
                assignment_id TEXT PRIMARY KEY,
                approval_id TEXT NOT NULL REFERENCES child_approvals(approval_id),
                artifact_id TEXT NOT NULL REFERENCES component_artifacts(artifact_id),
                child_name TEXT NOT NULL,
                vision_id TEXT NOT NULL,
                node_id TEXT,
                project_id TEXT,
                assignment_state TEXT NOT NULL,
                pinned_artifact_version TEXT NOT NULL,
                assignment_observation_id TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS child_instances (
                instance_id TEXT PRIMARY KEY,
                assignment_id TEXT NOT NULL REFERENCES child_assignments(assignment_id),
                artifact_id TEXT NOT NULL REFERENCES component_artifacts(artifact_id),
                child_name TEXT NOT NULL,
                generation INTEGER NOT NULL,
                node_id TEXT NOT NULL,
                instance_state TEXT NOT NULL,
                readiness_observation_id TEXT,
                last_lifecycle_observation_id TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS peers (
                peer_node_id TEXT PRIMARY KEY,
                binding_id TEXT NOT NULL,
                endpoint_id TEXT NOT NULL,
                vision_id TEXT NOT NULL,
                ticket_json TEXT,
                binding_state TEXT NOT NULL,
                policy_revision INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS remote_surface_views (
                peer_node_id TEXT NOT NULL,
                vision_id TEXT NOT NULL,
                binding_id TEXT NOT NULL,
                endpoint_id TEXT NOT NULL,
                publisher_policy_revision INTEGER NOT NULL,
                published_at TEXT NOT NULL,
                received_at TEXT NOT NULL,
                stale_at TEXT NOT NULL,
                view_observation_id TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (peer_node_id, vision_id)
            );

            CREATE TABLE IF NOT EXISTS remote_callable_surfaces (
                peer_node_id TEXT NOT NULL,
                vision_id TEXT NOT NULL,
                child_name TEXT NOT NULL,
                operation_id TEXT NOT NULL,
                binding_id TEXT NOT NULL,
                endpoint_id TEXT NOT NULL,
                publisher_policy_revision INTEGER NOT NULL,
                runtime_kind TEXT NOT NULL,
                surface_policy_revision INTEGER NOT NULL,
                visibility TEXT NOT NULL,
                received_at TEXT NOT NULL,
                stale_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (peer_node_id, vision_id, child_name, operation_id),
                FOREIGN KEY(peer_node_id, vision_id)
                    REFERENCES remote_surface_views(peer_node_id, vision_id)
                    ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_remote_callable_surfaces_operation
            ON remote_callable_surfaces(vision_id, operation_id, stale_at);

            CREATE TABLE IF NOT EXISTS call_idempotency_entries (
                caller_scope TEXT NOT NULL,
                idempotency_key TEXT NOT NULL,
                target_identity TEXT NOT NULL,
                call_id TEXT NOT NULL,
                payload_digest TEXT NOT NULL,
                entry_state TEXT NOT NULL CHECK(entry_state IN ('in_flight', 'completed')),
                recorded_reply_json TEXT,
                inline_result_payload BLOB,
                created_at TEXT NOT NULL,
                completed_at TEXT,
                expires_at TEXT NOT NULL,
                PRIMARY KEY (caller_scope, idempotency_key)
            );

            CREATE INDEX IF NOT EXISTS idx_call_idempotency_expiry
            ON call_idempotency_entries(caller_scope, expires_at);

            CREATE TABLE IF NOT EXISTS runtime_runs (
                run_id TEXT PRIMARY KEY,
                call_id TEXT NOT NULL,
                runtime_kind TEXT NOT NULL,
                child_name TEXT,
                child_instance_id TEXT,
                authority_decision_id TEXT,
                trace_id TEXT NOT NULL,
                state TEXT NOT NULL,
                started_at TEXT NOT NULL,
                completed_at TEXT,
                call_json TEXT NOT NULL,
                child_invocation_provenance_json TEXT,
                result_json TEXT
            );

            CREATE TABLE IF NOT EXISTS runtime_run_observations (
                run_id TEXT NOT NULL REFERENCES runtime_runs(run_id) ON DELETE CASCADE,
                observation_id TEXT NOT NULL,
                observation_kind TEXT NOT NULL,
                observation_json TEXT NOT NULL,
                PRIMARY KEY (run_id, observation_id)
            );

            CREATE TABLE IF NOT EXISTS child_state (
                child_name TEXT NOT NULL,
                key TEXT NOT NULL,
                value_json TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (child_name, key)
            );

            CREATE TABLE IF NOT EXISTS child_checkpoints (
                child_name TEXT NOT NULL,
                stream TEXT NOT NULL,
                checkpoint_json TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (child_name, stream)
            );

            CREATE TABLE IF NOT EXISTS child_subscriptions (
                child_name TEXT NOT NULL,
                stream TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (child_name, stream)
            );

            CREATE TABLE IF NOT EXISTS child_offsets (
                child_name TEXT NOT NULL,
                stream TEXT NOT NULL,
                acked_offset INTEGER NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (child_name, stream)
            );

            CREATE TABLE IF NOT EXISTS runtime_tasks (
                task_id TEXT PRIMARY KEY,
                child_name TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                dedupe_key TEXT,
                status TEXT NOT NULL,
                lease_owner TEXT,
                lease_until TEXT,
                attempts INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_runtime_tasks_dedupe
            ON runtime_tasks(child_name, dedupe_key)
            WHERE dedupe_key IS NOT NULL;

            CREATE INDEX IF NOT EXISTS idx_runtime_tasks_child_status
            ON runtime_tasks(child_name, status, updated_at);

            CREATE TABLE IF NOT EXISTS metric_points (
                metric_name TEXT NOT NULL,
                metric_value INTEGER NOT NULL,
                labels_json TEXT NOT NULL,
                observed_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS child_registry_sources (
                source_id TEXT PRIMARY KEY,
                source_path TEXT NOT NULL,
                last_sync_at TEXT,
                last_loaded INTEGER NOT NULL DEFAULT 0,
                last_failed INTEGER NOT NULL DEFAULT 0,
                state TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS artifact_source_authorities (
                source_authority_id TEXT PRIMARY KEY,
                source_ref TEXT NOT NULL,
                scope_json TEXT NOT NULL,
                integrity_policy_ref TEXT NOT NULL,
                provenance_policy_ref TEXT,
                issuer_principal_ref TEXT NOT NULL,
                policy_revision INTEGER NOT NULL,
                authority_state TEXT NOT NULL,
                issued_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                authority_observation_id TEXT NOT NULL,
                record_digest TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS operator_pointed_artifact_acquisition_decisions (
                decision_id TEXT PRIMARY KEY,
                source_ref TEXT NOT NULL,
                claimed_child_name TEXT NOT NULL,
                claimed_artifact_version TEXT NOT NULL,
                expected_digest TEXT,
                issuer_principal_ref TEXT NOT NULL,
                policy_revision INTEGER NOT NULL,
                decision_state TEXT NOT NULL,
                authority_observation_id TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS artifact_acquisitions (
                acquisition_id TEXT PRIMARY KEY,
                authority_path TEXT NOT NULL,
                standing_source_authority_id TEXT,
                operator_pointed_decision_id TEXT,
                adapter_effect_authority_ref TEXT NOT NULL,
                source_ref TEXT NOT NULL,
                claimed_child_name TEXT NOT NULL,
                claimed_artifact_version TEXT NOT NULL,
                observed_size_bytes INTEGER,
                observed_digest TEXT,
                acquisition_outcome TEXT NOT NULL,
                verification_outcome TEXT NOT NULL,
                verification_observation_id TEXT,
                acquisition_observation_id TEXT NOT NULL,
                component_artifact_id TEXT,
                created_at TEXT NOT NULL,
                CHECK (
                    (authority_path = 'standing_source' AND standing_source_authority_id IS NOT NULL AND operator_pointed_decision_id IS NULL)
                    OR
                    (authority_path = 'operator_pointed' AND standing_source_authority_id IS NULL AND operator_pointed_decision_id IS NOT NULL)
                ),
                CHECK (
                    component_artifact_id IS NULL
                    OR (acquisition_outcome = 'acquired' AND verification_outcome = 'verified')
                )
            );

            CREATE TABLE IF NOT EXISTS component_artifact_acquisitions (
                artifact_id TEXT NOT NULL REFERENCES component_artifacts(artifact_id),
                acquisition_id TEXT NOT NULL UNIQUE REFERENCES artifact_acquisitions(acquisition_id),
                PRIMARY KEY (artifact_id, acquisition_id)
            );

            CREATE TABLE IF NOT EXISTS component_artifact_packages (
                artifact_id TEXT PRIMARY KEY REFERENCES component_artifacts(artifact_id),
                package_path TEXT NOT NULL UNIQUE,
                published_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS composition_runs (
                composition_id TEXT PRIMARY KEY,
                state TEXT NOT NULL,
                steps_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS toy_catalog_contracts (
                toy_id TEXT PRIMARY KEY,
                contract_json TEXT NOT NULL,
                authority_bearing INTEGER NOT NULL CHECK(authority_bearing IN (0, 1)),
                catalog_revision INTEGER NOT NULL,
                admitted_by_observation_id TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS toy_grant_snapshots (
                grant_id TEXT PRIMARY KEY,
                toy_id TEXT NOT NULL REFERENCES toy_catalog_contracts(toy_id),
                subject_json TEXT NOT NULL,
                scope_json TEXT NOT NULL,
                constraints_json TEXT NOT NULL,
                grant_state TEXT NOT NULL,
                issuer_id TEXT NOT NULL,
                policy_revision INTEGER NOT NULL,
                grants_revision INTEGER NOT NULL,
                authority_observation_id TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_toy_grants_toy_state
            ON toy_grant_snapshots(toy_id, grant_state, grants_revision);

            CREATE TABLE IF NOT EXISTS call_trigger_authorities (
                trigger_authority_id TEXT NOT NULL,
                record_revision INTEGER NOT NULL CHECK(record_revision >= 1),
                policy_revision INTEGER NOT NULL CHECK(policy_revision >= 1),
                trigger_class TEXT NOT NULL CHECK(trigger_class IN ('temporal', 'event')),
                authority_state TEXT NOT NULL CHECK(authority_state IN ('active', 'revoked', 'superseded')),
                starts_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                authority_observation_id TEXT NOT NULL,
                canonical_record_digest TEXT NOT NULL,
                record_json TEXT NOT NULL,
                is_current INTEGER NOT NULL CHECK(is_current IN (0, 1)),
                projected_at TEXT NOT NULL,
                PRIMARY KEY (trigger_authority_id, record_revision)
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_call_trigger_one_current_revision
            ON call_trigger_authorities(trigger_authority_id)
            WHERE is_current = 1;

            CREATE INDEX IF NOT EXISTS idx_call_trigger_current_schedule
            ON call_trigger_authorities(is_current, authority_state, starts_at, expires_at);

            CREATE TABLE IF NOT EXISTS call_trigger_occurrences (
                occurrence_id TEXT PRIMARY KEY,
                trigger_authority_id TEXT NOT NULL,
                record_revision INTEGER NOT NULL,
                nominal_at TEXT,
                represented_set_json TEXT NOT NULL,
                missed_fire_result TEXT NOT NULL,
                overlap_result TEXT,
                final_disposition TEXT NOT NULL CHECK(final_disposition IN ('pending', 'fired', 'skipped', 'suppressed', 'capacity_refused')),
                disposition_observation_id TEXT NOT NULL,
                terminal INTEGER NOT NULL CHECK(terminal IN (0, 1)),
                created_at TEXT NOT NULL,
                FOREIGN KEY(trigger_authority_id, record_revision)
                    REFERENCES call_trigger_authorities(trigger_authority_id, record_revision)
            );

            CREATE INDEX IF NOT EXISTS idx_call_trigger_occurrence_record
            ON call_trigger_occurrences(trigger_authority_id, record_revision, nominal_at);

            CREATE TABLE IF NOT EXISTS call_trigger_pending_occurrences (
                pending_occurrence_id TEXT PRIMARY KEY,
                occurrence_id TEXT NOT NULL UNIQUE REFERENCES call_trigger_occurrences(occurrence_id),
                trigger_authority_id TEXT NOT NULL,
                record_revision INTEGER NOT NULL,
                policy_revision INTEGER NOT NULL,
                admission_sequence INTEGER NOT NULL CHECK(admission_sequence >= 1),
                pending_reason TEXT NOT NULL CHECK(pending_reason IN ('overlap_coalesced', 'overlap_queued')),
                represented_set_json TEXT NOT NULL,
                admission_observation_id TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('pending', 'consumed', 'suppressed')),
                admitted_at TEXT NOT NULL,
                consumed_at TEXT,
                UNIQUE(trigger_authority_id, admission_sequence)
            );

            CREATE INDEX IF NOT EXISTS idx_call_trigger_pending_state
            ON call_trigger_pending_occurrences(state, trigger_authority_id, admission_sequence);

            CREATE TABLE IF NOT EXISTS call_trigger_firings (
                firing_id TEXT PRIMARY KEY,
                occurrence_id TEXT NOT NULL UNIQUE REFERENCES call_trigger_occurrences(occurrence_id),
                trigger_authority_id TEXT NOT NULL,
                record_revision INTEGER NOT NULL,
                policy_revision INTEGER NOT NULL,
                call_id TEXT NOT NULL UNIQUE,
                idempotency_key_ref TEXT NOT NULL,
                firing_observation_id TEXT NOT NULL,
                target_result_ref TEXT,
                state TEXT NOT NULL CHECK(state IN ('active', 'terminal')),
                fired_at TEXT NOT NULL,
                completed_at TEXT
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_call_trigger_one_active_firing
            ON call_trigger_firings(trigger_authority_id)
            WHERE state = 'active';

            CREATE TABLE IF NOT EXISTS call_trigger_projection_meta (
                projection_name TEXT PRIMARY KEY,
                ledger_identity TEXT NOT NULL,
                last_ledger_sequence INTEGER NOT NULL CHECK(last_ledger_sequence >= 0),
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS watch_observation_scopes (
                watch_scope_id TEXT NOT NULL,
                scope_revision INTEGER NOT NULL CHECK(scope_revision >= 1),
                policy_revision INTEGER NOT NULL CHECK(policy_revision >= 1),
                child_name TEXT NOT NULL,
                artifact_id TEXT NOT NULL,
                assignment_id TEXT NOT NULL,
                canonical_root_ref TEXT NOT NULL,
                traversal_scope TEXT NOT NULL CHECK(traversal_scope IN ('root_only', 'recursive')),
                authority_state TEXT NOT NULL CHECK(authority_state IN ('active', 'revoked', 'superseded')),
                starts_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                authority_observation_id TEXT NOT NULL,
                canonical_record_digest TEXT NOT NULL,
                record_json TEXT NOT NULL,
                is_current INTEGER NOT NULL CHECK(is_current IN (0, 1)),
                projected_at TEXT NOT NULL,
                PRIMARY KEY (watch_scope_id, scope_revision)
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_watch_scope_one_current_revision
            ON watch_observation_scopes(watch_scope_id)
            WHERE is_current = 1;

            CREATE INDEX IF NOT EXISTS idx_watch_scope_current_observer
            ON watch_observation_scopes(is_current, authority_state, child_name, artifact_id, assignment_id);

            CREATE TABLE IF NOT EXISTS watch_scope_sequence_counters (
                watch_scope_id TEXT NOT NULL,
                scope_revision INTEGER NOT NULL,
                next_sequence INTEGER NOT NULL CHECK(next_sequence >= 1),
                updated_at TEXT NOT NULL,
                PRIMARY KEY (watch_scope_id, scope_revision),
                FOREIGN KEY(watch_scope_id, scope_revision)
                    REFERENCES watch_observation_scopes(watch_scope_id, scope_revision)
            );

            CREATE TABLE IF NOT EXISTS watch_event_batches (
                batch_id TEXT PRIMARY KEY,
                watch_scope_id TEXT NOT NULL,
                scope_revision INTEGER NOT NULL,
                sequence INTEGER NOT NULL CHECK(sequence >= 1),
                parent_call_id TEXT NOT NULL,
                batch_json TEXT NOT NULL,
                sealed_observation_id TEXT,
                created_at TEXT NOT NULL,
                UNIQUE(watch_scope_id, scope_revision, sequence),
                FOREIGN KEY(watch_scope_id, scope_revision)
                    REFERENCES watch_observation_scopes(watch_scope_id, scope_revision)
            );

            CREATE TABLE IF NOT EXISTS watch_scope_observed_subjects (
                watch_scope_id TEXT NOT NULL,
                scope_revision INTEGER NOT NULL,
                relative_path TEXT NOT NULL,
                last_event_id TEXT NOT NULL,
                last_observed_at TEXT NOT NULL,
                present INTEGER NOT NULL CHECK(present IN (0, 1)),
                PRIMARY KEY (watch_scope_id, scope_revision, relative_path),
                FOREIGN KEY(watch_scope_id, scope_revision)
                    REFERENCES watch_observation_scopes(watch_scope_id, scope_revision)
            );

            CREATE TABLE IF NOT EXISTS watch_events (
                event_id TEXT PRIMARY KEY,
                batch_id TEXT NOT NULL REFERENCES watch_event_batches(batch_id),
                batch_position INTEGER NOT NULL CHECK(batch_position >= 0),
                relative_path TEXT NOT NULL,
                event_json TEXT NOT NULL,
                event_observation_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(batch_id, batch_position)
            );

            CREATE TABLE IF NOT EXISTS watch_event_delivery_dispositions (
                disposition_id TEXT PRIMARY KEY,
                event_id TEXT NOT NULL UNIQUE REFERENCES watch_events(event_id),
                disposition TEXT NOT NULL CHECK(disposition IN ('fired', 'coalesced', 'suppressed', 'capacity_refused')),
                planned_call_id TEXT,
                compatibility_validation TEXT NOT NULL CHECK(compatibility_validation IN ('matched', 'mismatch_refused')),
                disposition_json TEXT NOT NULL,
                disposition_observation_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                CHECK ((disposition = 'fired' AND planned_call_id IS NOT NULL)
                    OR (disposition != 'fired' AND planned_call_id IS NULL))
            );

            CREATE TABLE IF NOT EXISTS watch_event_deliveries (
                delivery_id TEXT PRIMARY KEY,
                disposition_id TEXT NOT NULL UNIQUE REFERENCES watch_event_delivery_dispositions(disposition_id),
                target_call_id TEXT NOT NULL UNIQUE,
                target_result_ref TEXT NOT NULL,
                target_result_observation_id TEXT NOT NULL,
                delivered INTEGER NOT NULL CHECK(delivered IN (0, 1)),
                delivery_json TEXT NOT NULL,
                completed_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS watch_projection_meta (
                projection_name TEXT PRIMARY KEY,
                ledger_identity TEXT NOT NULL,
                last_ledger_sequence INTEGER NOT NULL CHECK(last_ledger_sequence >= 0),
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS child_keyvalue_entries (
                bucket_resource_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value BLOB NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (bucket_resource_id, key)
            );

            CREATE TRIGGER IF NOT EXISTS active_toy_grant_requires_authority_bearing_toy_insert
            BEFORE INSERT ON toy_grant_snapshots
            WHEN NEW.grant_state = 'active'
            BEGIN
                SELECT RAISE(ABORT, 'active toy grant requires authority-bearing catalog contract')
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM toy_catalog_contracts catalog
                    WHERE catalog.toy_id = NEW.toy_id
                      AND catalog.authority_bearing = 1
                );
            END;

            CREATE TRIGGER IF NOT EXISTS active_toy_grant_requires_authority_bearing_toy_update
            BEFORE UPDATE OF toy_id, grant_state ON toy_grant_snapshots
            WHEN NEW.grant_state = 'active'
            BEGIN
                SELECT RAISE(ABORT, 'active toy grant requires authority-bearing catalog contract')
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM toy_catalog_contracts catalog
                    WHERE catalog.toy_id = NEW.toy_id
                      AND catalog.authority_bearing = 1
                );
            END;

            CREATE TRIGGER IF NOT EXISTS authority_bearing_toy_cannot_be_disabled_with_active_grants
            BEFORE UPDATE OF authority_bearing ON toy_catalog_contracts
            WHEN NEW.authority_bearing = 0
            BEGIN
                SELECT RAISE(ABORT, 'authority-bearing toy has active grants')
                WHERE EXISTS (
                    SELECT 1
                    FROM toy_grant_snapshots grants
                    WHERE grants.toy_id = NEW.toy_id
                      AND grants.grant_state = 'active'
                );
            END;

            CREATE TRIGGER IF NOT EXISTS active_assignment_requires_approved_child_insert
            BEFORE INSERT ON child_assignments
            WHEN NEW.assignment_state = 'active'
            BEGIN
                SELECT RAISE(ABORT, 'active assignment requires approved child approval')
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM child_approvals approvals
                    JOIN component_artifacts artifacts ON artifacts.artifact_id = approvals.artifact_id
                    WHERE approvals.approval_id = NEW.approval_id
                      AND approvals.artifact_id = NEW.artifact_id
                      AND approvals.child_name = NEW.child_name
                      AND approvals.artifact_version = NEW.pinned_artifact_version
                      AND approvals.approval_state = 'approved'
                      AND artifacts.verification_status = 'verified'
                );
            END;

            CREATE TRIGGER IF NOT EXISTS active_assignment_requires_approved_child_update
            BEFORE UPDATE OF assignment_state, approval_id, artifact_id, child_name, pinned_artifact_version ON child_assignments
            WHEN NEW.assignment_state = 'active'
            BEGIN
                SELECT RAISE(ABORT, 'active assignment requires approved child approval')
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM child_approvals approvals
                    JOIN component_artifacts artifacts ON artifacts.artifact_id = approvals.artifact_id
                    WHERE approvals.approval_id = NEW.approval_id
                      AND approvals.artifact_id = NEW.artifact_id
                      AND approvals.child_name = NEW.child_name
                      AND approvals.artifact_version = NEW.pinned_artifact_version
                      AND approvals.approval_state = 'approved'
                      AND artifacts.verification_status = 'verified'
                );
            END;

            CREATE TRIGGER IF NOT EXISTS ready_instance_requires_active_assignment_insert
            BEFORE INSERT ON child_instances
            WHEN NEW.instance_state = 'ready'
            BEGIN
                SELECT RAISE(ABORT, 'ready instance requires active assignment')
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM child_assignments assignments
                    WHERE assignments.assignment_id = NEW.assignment_id
                      AND assignments.artifact_id = NEW.artifact_id
                      AND assignments.child_name = NEW.child_name
                      AND assignments.assignment_state = 'active'
                );
            END;

            CREATE TRIGGER IF NOT EXISTS ready_instance_requires_active_assignment_update
            BEFORE UPDATE OF instance_state, assignment_id, artifact_id, child_name ON child_instances
            WHEN NEW.instance_state = 'ready'
            BEGIN
                SELECT RAISE(ABORT, 'ready instance requires active assignment')
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM child_assignments assignments
                    WHERE assignments.assignment_id = NEW.assignment_id
                      AND assignments.artifact_id = NEW.artifact_id
                      AND assignments.child_name = NEW.child_name
                      AND assignments.assignment_state = 'active'
                );
            END;
            "#,
        )?;
        self.migrate_runtime_run_child_invocation_provenance()?;
        self.migrate_artifact_acquisition_provenance()?;
        self.conn.execute(
            "INSERT OR REPLACE INTO mct_state_meta(key, value) VALUES('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    }

    fn migrate_runtime_run_child_invocation_provenance(&self) -> Result<()> {
        if !self.column_exists("runtime_runs", "child_invocation_provenance_json")? {
            self.conn.execute(
                "ALTER TABLE runtime_runs ADD COLUMN child_invocation_provenance_json TEXT",
                [],
            )?;
        }
        if !self.column_exists("runtime_runs", "authorized_child_invocation_json")? {
            return Ok(());
        }

        let mut stmt = self.conn.prepare(
            r#"
            SELECT run_id, authorized_child_invocation_json
            FROM runtime_runs
            WHERE authorized_child_invocation_json IS NOT NULL
              AND child_invocation_provenance_json IS NULL
            "#,
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (run_id, authorized_json) in rows {
            let authorized: LegacyAuthorizedChildInvocation = from_json_cell(&authorized_json)
                .with_context(|| {
                    format!("decode legacy authorized child invocation for {run_id}")
                })?;
            let provenance = ChildInvocationProvenance::from_legacy_authorized(
                authorized,
                ObservationId::new(format!("obs:migrated-child-authority:{run_id}"))
                    .expect("generated migration observation id must be non-empty"),
            );
            self.conn.execute(
                r#"
                UPDATE runtime_runs
                SET child_invocation_provenance_json = ?1
                WHERE run_id = ?2
                "#,
                params![json_string(&provenance)?, run_id],
            )?;
        }
        Ok(())
    }

    fn migrate_artifact_acquisition_provenance(&self) -> Result<()> {
        if !self.column_exists("component_artifacts", "provenance_status")? {
            self.conn.execute(
                "ALTER TABLE component_artifacts ADD COLUMN provenance_status TEXT NOT NULL DEFAULT 'historical_unknown'",
                [],
            )?;
        }
        if !self.column_exists("component_artifacts", "acquisition_ids_json")? {
            self.conn.execute(
                "ALTER TABLE component_artifacts ADD COLUMN acquisition_ids_json TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
        }
        if !self.column_exists("component_artifacts", "primary_acquisition_id")? {
            self.conn.execute(
                "ALTER TABLE component_artifacts ADD COLUMN primary_acquisition_id TEXT",
                [],
            )?;
        }
        Ok(())
    }

    fn column_exists(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(columns.iter().any(|name| name == column))
    }

    pub fn summary(&self) -> Result<MctRuntimeStateSummary> {
        Ok(MctRuntimeStateSummary {
            schema_version: self.schema_version()?,
            artifacts: self.count("component_artifacts", None)?,
            approved_children: self
                .count("child_approvals", Some("approval_state = 'approved'"))?,
            active_assignments: self
                .count("child_assignments", Some("assignment_state = 'active'"))?,
            ready_instances: self.count("child_instances", Some("instance_state = 'ready'"))?,
            peers: self.count("peers", None)?,
            runs: self.count("runtime_runs", None)?,
            completed_runs: self.count("runtime_runs", Some("state = 'completed'"))?,
            failed_runs: self.count(
                "runtime_runs",
                Some("state IN ('failed', 'timed_out', 'cancelled', 'denied')"),
            )?,
            metric_points: self.count("metric_points", None)?,
            queued_tasks: self.count(
                "runtime_tasks",
                Some("status IN ('queued', 'leased', 'running', 'failed')"),
            )?,
            child_state_keys: self.count("child_state", None)?,
            child_subscriptions: self.count("child_subscriptions", None)?,
            toy_catalog_contracts: self.count("toy_catalog_contracts", None)?,
            toy_grant_snapshots: self.count("toy_grant_snapshots", None)?,
            trigger_records: self.count("call_trigger_authorities", None)?,
            current_trigger_records: self
                .count("call_trigger_authorities", Some("is_current = 1"))?,
            pending_trigger_occurrences: self.count(
                "call_trigger_pending_occurrences",
                Some("state = 'pending'"),
            )?,
            active_trigger_firings: self.count("call_trigger_firings", Some("state = 'active'"))?,
            watch_scope_records: self.count("watch_observation_scopes", None)?,
            current_watch_scopes: self.count("watch_observation_scopes", Some("is_current = 1"))?,
            watch_event_batches: self.count("watch_event_batches", None)?,
            watch_events: self.count("watch_events", None)?,
            watch_event_deliveries: self.count("watch_event_deliveries", None)?,
        })
    }

    pub fn schema_version(&self) -> Result<i64> {
        let value: String = self.conn.query_row(
            "SELECT value FROM mct_state_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )?;
        value.parse().context("parse schema version")
    }

    pub fn insert_call_trigger_authority(&self, authority: &CallTriggerAuthority) -> Result<()> {
        authority.validate().map_err(anyhow::Error::from)?;
        if authority.authority_state == CallTriggerAuthorityState::Superseded {
            bail!("new trigger revision cannot be born superseded");
        }
        let transaction =
            rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let current_json = transaction
            .query_row(
                r#"
                SELECT record_json
                FROM call_trigger_authorities
                WHERE trigger_authority_id = ?1 AND is_current = 1
                "#,
                params![authority.trigger_authority_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match current_json {
            None => {
                if authority.record_revision != 1
                    || authority.authority_state != CallTriggerAuthorityState::Active
                {
                    bail!("first trigger revision must be active revision one");
                }
            }
            Some(current_json) => {
                let current: CallTriggerAuthority = from_json_cell(&current_json)?;
                if current.authority_state == CallTriggerAuthorityState::Revoked {
                    bail!("revoked trigger id cannot be resurrected");
                }
                if authority.record_revision != current.record_revision + 1 {
                    bail!("trigger revision must exactly increment current revision");
                }
                transaction.execute(
                    r#"
                    UPDATE call_trigger_authorities
                    SET is_current = 0
                    WHERE trigger_authority_id = ?1 AND record_revision = ?2 AND is_current = 1
                    "#,
                    params![
                        current.trigger_authority_id.as_str(),
                        current.record_revision as i64
                    ],
                )?;
            }
        }
        transaction.execute(
            r#"
            INSERT INTO call_trigger_authorities(
                trigger_authority_id, record_revision, policy_revision,
                trigger_class, authority_state, starts_at, expires_at,
                authority_observation_id, canonical_record_digest, record_json,
                is_current, projected_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11)
            "#,
            params![
                authority.trigger_authority_id.as_str(),
                authority.record_revision as i64,
                authority.policy_revision as i64,
                json_atom(&authority.trigger_source.class())?,
                json_atom(&authority.authority_state)?,
                authority.starts_at.as_str(),
                authority.expires_at.as_str(),
                authority.authority_observation_id.as_str(),
                authority.canonical_record_digest,
                json_string(authority)?,
                current_timestamp_string(),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn current_call_trigger_authority(
        &self,
        trigger_authority_id: &CallTriggerAuthorityId,
    ) -> Result<Option<CallTriggerAuthority>> {
        let record_json = self
            .conn
            .query_row(
                r#"
                SELECT record_json
                FROM call_trigger_authorities
                WHERE trigger_authority_id = ?1 AND is_current = 1
                "#,
                params![trigger_authority_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        record_json
            .map(|json| {
                let authority: CallTriggerAuthority = from_json_cell(&json)?;
                authority.validate().map_err(anyhow::Error::from)?;
                Ok(authority)
            })
            .transpose()
    }

    pub fn call_trigger_authority_projected_at(
        &self,
        trigger_authority_id: &CallTriggerAuthorityId,
        record_revision: u64,
    ) -> Result<Option<Timestamp>> {
        let value = self
            .conn
            .query_row(
                r#"
                SELECT projected_at FROM call_trigger_authorities
                WHERE trigger_authority_id = ?1 AND record_revision = ?2
                "#,
                params![trigger_authority_id.as_str(), record_revision as i64],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        value.map(Timestamp::new).transpose().map_err(Into::into)
    }

    pub fn call_trigger_authorities(&self) -> Result<Vec<CallTriggerAuthority>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT record_json
            FROM call_trigger_authorities
            ORDER BY trigger_authority_id, record_revision
            "#,
        )?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .map(|json| {
                let authority: CallTriggerAuthority = from_json_cell(&json?)?;
                authority.validate().map_err(anyhow::Error::from)?;
                Ok(authority)
            })
            .collect()
    }

    pub fn current_call_trigger_authorities(&self) -> Result<Vec<CallTriggerAuthority>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT record_json
            FROM call_trigger_authorities
            WHERE is_current = 1
            ORDER BY trigger_authority_id
            "#,
        )?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .map(|json| {
                let authority: CallTriggerAuthority = from_json_cell(&json?)?;
                authority.validate().map_err(anyhow::Error::from)?;
                Ok(authority)
            })
            .collect()
    }

    pub fn current_active_call_trigger_authorities(&self) -> Result<Vec<CallTriggerAuthority>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT record_json
            FROM call_trigger_authorities
            WHERE is_current = 1 AND authority_state = 'active'
            ORDER BY trigger_authority_id
            "#,
        )?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .map(|json| {
                let authority: CallTriggerAuthority = from_json_cell(&json?)?;
                authority.validate().map_err(anyhow::Error::from)?;
                Ok(authority)
            })
            .collect()
    }

    pub fn insert_watch_observation_scope(&self, scope: &WatchObservationScope) -> Result<()> {
        scope.validate().map_err(anyhow::Error::from)?;
        if scope.authority_state == WatchObservationScopeState::Superseded {
            bail!("new Watch scope revision cannot be born superseded");
        }
        let transaction =
            rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let current_json = transaction
            .query_row(
                r#"
                SELECT record_json FROM watch_observation_scopes
                WHERE watch_scope_id = ?1 AND is_current = 1
                "#,
                params![scope.watch_scope_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match current_json {
            None => {
                if scope.scope_revision != 1
                    || scope.authority_state != WatchObservationScopeState::Active
                {
                    bail!("first Watch scope revision must be active revision one");
                }
            }
            Some(current_json) => {
                let current: WatchObservationScope = from_json_cell(&current_json)?;
                if current.authority_state == WatchObservationScopeState::Revoked {
                    bail!("revoked Watch scope id cannot be resurrected");
                }
                if scope.scope_revision != current.scope_revision + 1 {
                    bail!("Watch scope revision must exactly increment current revision");
                }
                transaction.execute(
                    r#"
                    UPDATE watch_observation_scopes SET is_current = 0
                    WHERE watch_scope_id = ?1 AND scope_revision = ?2 AND is_current = 1
                    "#,
                    params![
                        current.watch_scope_id.as_str(),
                        current.scope_revision as i64
                    ],
                )?;
            }
        }
        transaction.execute(
            r#"
            INSERT INTO watch_observation_scopes(
                watch_scope_id, scope_revision, policy_revision, child_name,
                artifact_id, assignment_id, canonical_root_ref, traversal_scope,
                authority_state, starts_at, expires_at, authority_observation_id,
                canonical_record_digest, record_json, is_current, projected_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 1, ?15)
            "#,
            params![
                scope.watch_scope_id.as_str(),
                scope.scope_revision as i64,
                scope.policy_revision as i64,
                scope.observer_ref.child_name.as_str(),
                scope.observer_ref.artifact_id.as_str(),
                scope.observer_ref.assignment_id.as_str(),
                scope.canonical_root_ref.as_str(),
                json_atom(&scope.traversal_scope)?,
                json_atom(&scope.authority_state)?,
                scope.starts_at.as_str(),
                scope.expires_at.as_str(),
                scope.authority_observation_id.as_str(),
                scope.canonical_record_digest.as_str(),
                json_string(scope)?,
                current_timestamp_string(),
            ],
        )?;
        transaction.execute(
            r#"
            INSERT OR IGNORE INTO watch_scope_sequence_counters(
                watch_scope_id, scope_revision, next_sequence, updated_at
            ) VALUES (?1, ?2, 1, ?3)
            "#,
            params![
                scope.watch_scope_id.as_str(),
                scope.scope_revision as i64,
                current_timestamp_string(),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn current_watch_observation_scope(
        &self,
        watch_scope_id: &WatchObservationScopeId,
    ) -> Result<Option<WatchObservationScope>> {
        let record_json = self
            .conn
            .query_row(
                r#"
                SELECT record_json FROM watch_observation_scopes
                WHERE watch_scope_id = ?1 AND is_current = 1
                "#,
                params![watch_scope_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        record_json
            .map(|json| {
                let scope: WatchObservationScope = from_json_cell(&json)?;
                scope.validate().map_err(anyhow::Error::from)?;
                Ok(scope)
            })
            .transpose()
    }

    pub fn watch_observation_scopes(&self) -> Result<Vec<WatchObservationScope>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT record_json FROM watch_observation_scopes
            ORDER BY watch_scope_id, scope_revision
            "#,
        )?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .map(|json| {
                let scope: WatchObservationScope = from_json_cell(&json?)?;
                scope.validate().map_err(anyhow::Error::from)?;
                Ok(scope)
            })
            .collect()
    }

    pub fn child_keyvalue_get(
        &self,
        bucket_resource_id: &str,
        key: &str,
    ) -> Result<Option<Vec<u8>>> {
        if key.is_empty() || key.len() > MCT_KEYVALUE_KEY_MAX_BYTES {
            bail!("keyvalue key is outside the bounded contract");
        }
        self.conn
            .query_row(
                "SELECT value FROM child_keyvalue_entries WHERE bucket_resource_id = ?1 AND key = ?2",
                params![bucket_resource_id, key],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn child_keyvalue_set(
        &self,
        bucket_resource_id: &str,
        key: &str,
        value: &[u8],
    ) -> Result<()> {
        if key.is_empty() || key.len() > MCT_KEYVALUE_KEY_MAX_BYTES {
            bail!("keyvalue key is outside the bounded contract");
        }
        if value.len() > MCT_KEYVALUE_VALUE_MAX_BYTES {
            bail!("keyvalue value is outside the bounded contract");
        }
        let transaction =
            rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM child_keyvalue_entries WHERE bucket_resource_id = ?1 AND key = ?2)",
            params![bucket_resource_id, key],
            |row| Ok(row.get::<_, i64>(0)? != 0),
        )?;
        if !exists {
            let count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM child_keyvalue_entries WHERE bucket_resource_id = ?1",
                params![bucket_resource_id],
                |row| row.get(0),
            )?;
            if count >= MCT_KEYVALUE_MAX_KEYS_PER_BUCKET as i64 {
                bail!("keyvalue bucket capacity exhausted");
            }
        }
        transaction.execute(
            r#"
            INSERT INTO child_keyvalue_entries(bucket_resource_id, key, value, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(bucket_resource_id, key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at
            "#,
            params![bucket_resource_id, key, value, current_timestamp_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn reserve_watch_batch_sequence(
        &self,
        watch_scope_id: &WatchObservationScopeId,
        scope_revision: u64,
    ) -> Result<u64> {
        let transaction =
            rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let sequence: i64 = transaction.query_row(
            r#"
            SELECT next_sequence FROM watch_scope_sequence_counters
            WHERE watch_scope_id = ?1 AND scope_revision = ?2
            "#,
            params![watch_scope_id.as_str(), scope_revision as i64],
            |row| row.get(0),
        )?;
        transaction.execute(
            r#"
            UPDATE watch_scope_sequence_counters
            SET next_sequence = next_sequence + 1, updated_at = ?1
            WHERE watch_scope_id = ?2 AND scope_revision = ?3 AND next_sequence = ?4
            "#,
            params![
                current_timestamp_string(),
                watch_scope_id.as_str(),
                scope_revision as i64,
                sequence,
            ],
        )?;
        transaction.commit()?;
        Ok(sequence as u64)
    }

    pub fn watch_subject_was_present(
        &self,
        watch_scope_id: &WatchObservationScopeId,
        scope_revision: u64,
        relative_path: &str,
    ) -> Result<bool> {
        Ok(self.conn.query_row(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM watch_scope_observed_subjects
                WHERE watch_scope_id = ?1 AND scope_revision = ?2
                  AND relative_path = ?3 AND present = 1
            )
            "#,
            params![
                watch_scope_id.as_str(),
                scope_revision as i64,
                relative_path
            ],
            |row| row.get::<_, i64>(0),
        )? != 0)
    }

    pub fn record_watch_subject_presence(
        &self,
        watch_scope_id: &WatchObservationScopeId,
        scope_revision: u64,
        event_id: &WatchEventId,
        relative_path: &str,
        present: bool,
    ) -> Result<()> {
        validate_safe_watch_relative_path(relative_path).map_err(anyhow::Error::from)?;
        self.conn.execute(
            r#"
            INSERT INTO watch_scope_observed_subjects(
                watch_scope_id, scope_revision, relative_path,
                last_event_id, last_observed_at, present
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(watch_scope_id, scope_revision, relative_path) DO UPDATE SET
                last_event_id = excluded.last_event_id,
                last_observed_at = excluded.last_observed_at,
                present = excluded.present
            "#,
            params![
                watch_scope_id.as_str(),
                scope_revision as i64,
                relative_path,
                event_id.as_str(),
                current_timestamp_string(),
                i64::from(present),
            ],
        )?;
        Ok(())
    }

    pub fn insert_watch_event_plan(
        &self,
        batch: &WatchEventBatchEvidence,
        events: &[WatchEventEvidence],
        dispositions: &[WatchEventDeliveryDisposition],
    ) -> Result<()> {
        if events.len() != dispositions.len()
            || batch.eligible_event_count as usize != events.len()
            || events.len() > MCT_WATCH_MAX_EVENTS_PER_BATCH as usize
        {
            bail!("Watch event plan is inconsistent or exceeds capacity");
        }
        let transaction =
            rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        transaction.execute(
            r#"
            INSERT INTO watch_event_batches(
                batch_id, watch_scope_id, scope_revision, sequence, parent_call_id,
                batch_json, sealed_observation_id, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)
            "#,
            params![
                batch.batch_id.as_str(),
                batch.watch_scope_id.as_str(),
                batch.scope_revision as i64,
                batch.sequence as i64,
                batch.parent_call_id.as_str(),
                json_string(batch)?,
                current_timestamp_string(),
            ],
        )?;
        for (event, disposition) in events.iter().zip(dispositions) {
            if event.batch_id != batch.batch_id || disposition.event_id != event.event_id {
                bail!("Watch event/disposition does not belong to batch");
            }
            disposition.validate().map_err(anyhow::Error::from)?;
            transaction.execute(
                r#"
                INSERT INTO watch_events(
                    event_id, batch_id, batch_position, relative_path, event_json,
                    event_observation_id, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    event.event_id.as_str(),
                    event.batch_id.as_str(),
                    event.batch_position as i64,
                    event.relative_path.as_str(),
                    json_string(event)?,
                    format!("obs:watch-event:{}", event.event_id),
                    current_timestamp_string(),
                ],
            )?;
            transaction.execute(
                r#"
                INSERT INTO watch_event_delivery_dispositions(
                    disposition_id, event_id, disposition, planned_call_id,
                    compatibility_validation, disposition_json,
                    disposition_observation_id, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    disposition.disposition_id.as_str(),
                    disposition.event_id.as_str(),
                    json_atom(&disposition.disposition)?,
                    disposition.planned_call_id.as_ref().map(CallId::as_str),
                    json_atom(&disposition.compatibility_validation)?,
                    json_string(disposition)?,
                    disposition.disposition_observation_id.as_str(),
                    current_timestamp_string(),
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn insert_watch_delivery_evidence(
        &self,
        evidence: &WatchEventDeliveryEvidence,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO watch_event_deliveries(
                delivery_id, disposition_id, target_call_id, target_result_ref,
                target_result_observation_id, delivered, delivery_json, completed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                evidence.delivery_id.as_str(),
                evidence.disposition_id.as_str(),
                evidence.target_call_id.as_str(),
                evidence.target_result_ref.as_str(),
                evidence.target_result_observation_id.as_str(),
                i64::from(evidence.delivered),
                json_string(evidence)?,
                current_timestamp_string(),
            ],
        )?;
        Ok(())
    }

    pub fn latest_call_trigger_nominal_at(
        &self,
        trigger_authority_id: &CallTriggerAuthorityId,
        record_revision: u64,
    ) -> Result<Option<Timestamp>> {
        let value = self.conn.query_row(
            r#"
            SELECT MAX(nominal_at)
            FROM call_trigger_occurrences
            WHERE trigger_authority_id = ?1 AND record_revision = ?2
            "#,
            params![trigger_authority_id.as_str(), record_revision as i64],
            |row| row.get::<_, Option<String>>(0),
        )?;
        value.map(Timestamp::new).transpose().map_err(Into::into)
    }

    pub fn call_trigger_occurrence_exists(
        &self,
        occurrence_id: &CallTriggerOccurrenceId,
    ) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM call_trigger_occurrences WHERE occurrence_id = ?1)",
            params![occurrence_id.as_str()],
            |row| row.get::<_, i64>(0),
        )? != 0)
    }

    pub fn insert_call_trigger_occurrence(
        &self,
        occurrence: &MctTriggerOccurrenceRecord,
    ) -> Result<()> {
        if occurrence.record_revision == 0
            || occurrence.represented_set.count == 0
            || occurrence.final_disposition == MctTriggerOccurrenceDisposition::Pending
        {
            bail!("terminal trigger occurrence projection is malformed");
        }
        self.conn.execute(
            r#"
            INSERT INTO call_trigger_occurrences(
                occurrence_id, trigger_authority_id, record_revision, nominal_at,
                represented_set_json, missed_fire_result, overlap_result,
                final_disposition, disposition_observation_id, terminal, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10)
            "#,
            params![
                occurrence.occurrence_id.as_str(),
                occurrence.trigger_authority_id.as_str(),
                occurrence.record_revision as i64,
                occurrence.nominal_at.as_ref().map(Timestamp::as_str),
                json_string(&occurrence.represented_set)?,
                occurrence.missed_fire_result,
                occurrence.overlap_result,
                json_atom(&occurrence.final_disposition)?,
                occurrence.disposition_observation_id.as_str(),
                occurrence.created_at.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn next_call_trigger_pending_sequence(
        &self,
        trigger_authority_id: &CallTriggerAuthorityId,
    ) -> Result<u64> {
        let sequence: i64 = self.conn.query_row(
            r#"
            SELECT COALESCE(MAX(admission_sequence), 0) + 1
            FROM call_trigger_pending_occurrences
            WHERE trigger_authority_id = ?1
            "#,
            params![trigger_authority_id.as_str()],
            |row| row.get(0),
        )?;
        Ok(sequence.max(1) as u64)
    }

    pub fn call_trigger_pending_counts(
        &self,
        trigger_authority_id: &CallTriggerAuthorityId,
    ) -> Result<(usize, usize)> {
        let per_record: i64 = self.conn.query_row(
            r#"
            SELECT COUNT(*) FROM call_trigger_pending_occurrences
            WHERE trigger_authority_id = ?1 AND state = 'pending'
            "#,
            params![trigger_authority_id.as_str()],
            |row| row.get(0),
        )?;
        let resident: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM call_trigger_pending_occurrences WHERE state = 'pending'",
            [],
            |row| row.get(0),
        )?;
        Ok((per_record.max(0) as usize, resident.max(0) as usize))
    }

    pub fn insert_call_trigger_pending_occurrence(
        &self,
        occurrence: &MctTriggerOccurrenceRecord,
        pending: &MctTriggerPendingOccurrenceRecord,
    ) -> Result<()> {
        if occurrence.final_disposition != MctTriggerOccurrenceDisposition::Pending
            || pending.state != "pending"
            || occurrence.occurrence_id != pending.occurrence_id
            || occurrence.trigger_authority_id != pending.trigger_authority_id
            || occurrence.record_revision != pending.record_revision
        {
            bail!("pending trigger occurrence projection is inconsistent");
        }
        let transaction =
            rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        transaction.execute(
            r#"
            INSERT INTO call_trigger_occurrences(
                occurrence_id, trigger_authority_id, record_revision, nominal_at,
                represented_set_json, missed_fire_result, overlap_result,
                final_disposition, disposition_observation_id, terminal, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8, 0, ?9)
            "#,
            params![
                occurrence.occurrence_id.as_str(),
                occurrence.trigger_authority_id.as_str(),
                occurrence.record_revision as i64,
                occurrence.nominal_at.as_ref().map(Timestamp::as_str),
                json_string(&occurrence.represented_set)?,
                occurrence.missed_fire_result,
                occurrence.overlap_result,
                occurrence.disposition_observation_id.as_str(),
                occurrence.created_at.as_str(),
            ],
        )?;
        transaction.execute(
            r#"
            INSERT INTO call_trigger_pending_occurrences(
                pending_occurrence_id, occurrence_id, trigger_authority_id,
                record_revision, policy_revision, admission_sequence,
                pending_reason, represented_set_json, admission_observation_id,
                state, admitted_at, consumed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', ?10, NULL)
            "#,
            params![
                pending.pending_occurrence_id.as_str(),
                pending.occurrence_id.as_str(),
                pending.trigger_authority_id.as_str(),
                pending.record_revision as i64,
                pending.policy_revision as i64,
                pending.admission_sequence as i64,
                json_atom(&pending.pending_reason)?,
                json_string(&pending.represented_set)?,
                pending.admission_observation_id.as_str(),
                pending.admitted_at.as_str(),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn call_trigger_occurrences(&self) -> Result<Vec<MctTriggerOccurrenceRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT occurrence_id, trigger_authority_id, record_revision,
                   nominal_at, represented_set_json, missed_fire_result,
                   overlap_result, final_disposition,
                   disposition_observation_id, created_at
            FROM call_trigger_occurrences ORDER BY created_at, occurrence_id
            "#,
        )?;
        stmt.query_map([], trigger_occurrence_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn call_trigger_occurrence(
        &self,
        occurrence_id: &CallTriggerOccurrenceId,
    ) -> Result<Option<MctTriggerOccurrenceRecord>> {
        self.conn
            .query_row(
                r#"
                SELECT occurrence_id, trigger_authority_id, record_revision,
                       nominal_at, represented_set_json, missed_fire_result,
                       overlap_result, final_disposition,
                       disposition_observation_id, created_at
                FROM call_trigger_occurrences
                WHERE occurrence_id = ?1
                "#,
                params![occurrence_id.as_str()],
                trigger_occurrence_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn call_trigger_firings(&self) -> Result<Vec<MctTriggerFiringRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT firing_id, occurrence_id, trigger_authority_id, record_revision,
                   policy_revision, call_id, idempotency_key_ref,
                   firing_observation_id, target_result_ref, state, fired_at, completed_at
            FROM call_trigger_firings ORDER BY fired_at, firing_id
            "#,
        )?;
        stmt.query_map([], trigger_firing_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn call_trigger_firing(
        &self,
        firing_id: &CallTriggerFiringId,
    ) -> Result<Option<MctTriggerFiringRecord>> {
        self.conn
            .query_row(
                r#"
                SELECT firing_id, occurrence_id, trigger_authority_id, record_revision,
                       policy_revision, call_id, idempotency_key_ref,
                       firing_observation_id, target_result_ref, state, fired_at, completed_at
                FROM call_trigger_firings WHERE firing_id = ?1
                "#,
                params![firing_id.as_str()],
                trigger_firing_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn active_call_trigger_firings(&self) -> Result<Vec<MctTriggerFiringRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT firing_id, occurrence_id, trigger_authority_id, record_revision,
                   policy_revision, call_id, idempotency_key_ref,
                   firing_observation_id, target_result_ref, state, fired_at, completed_at
            FROM call_trigger_firings
            WHERE state = 'active'
            ORDER BY fired_at, firing_id
            "#,
        )?;
        stmt.query_map([], trigger_firing_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn active_call_trigger_firing(
        &self,
        trigger_authority_id: &CallTriggerAuthorityId,
    ) -> Result<Option<MctTriggerFiringRecord>> {
        self.conn
            .query_row(
                r#"
                SELECT firing_id, occurrence_id, trigger_authority_id, record_revision,
                       policy_revision, call_id, idempotency_key_ref,
                       firing_observation_id, target_result_ref, state, fired_at, completed_at
                FROM call_trigger_firings
                WHERE trigger_authority_id = ?1 AND state = 'active'
                "#,
                params![trigger_authority_id.as_str()],
                trigger_firing_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn insert_call_trigger_firing(
        &self,
        occurrence: &MctTriggerOccurrenceRecord,
        firing: &MctTriggerFiringRecord,
    ) -> Result<()> {
        if occurrence.final_disposition != MctTriggerOccurrenceDisposition::Fired
            || firing.state != "active"
            || occurrence.occurrence_id != firing.occurrence_id
            || occurrence.trigger_authority_id != firing.trigger_authority_id
        {
            bail!("trigger firing projection is inconsistent");
        }
        let transaction =
            rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let pending = transaction
            .query_row(
                "SELECT state FROM call_trigger_pending_occurrences WHERE occurrence_id = ?1",
                params![occurrence.occurrence_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(state) = pending {
            if state != "pending" {
                bail!("trigger pending occurrence is not consumable");
            }
            transaction.execute(
                r#"
                UPDATE call_trigger_occurrences
                SET final_disposition = 'fired', terminal = 1,
                    disposition_observation_id = ?1, overlap_result = ?2
                WHERE occurrence_id = ?3 AND terminal = 0 AND final_disposition = 'pending'
                "#,
                params![
                    occurrence.disposition_observation_id.as_str(),
                    occurrence.overlap_result,
                    occurrence.occurrence_id.as_str(),
                ],
            )?;
            transaction.execute(
                r#"
                UPDATE call_trigger_pending_occurrences
                SET state = 'consumed', consumed_at = ?1
                WHERE occurrence_id = ?2 AND state = 'pending'
                "#,
                params![
                    occurrence.created_at.as_str(),
                    occurrence.occurrence_id.as_str()
                ],
            )?;
        } else {
            transaction.execute(
                r#"
                INSERT INTO call_trigger_occurrences(
                    occurrence_id, trigger_authority_id, record_revision, nominal_at,
                    represented_set_json, missed_fire_result, overlap_result,
                    final_disposition, disposition_observation_id, terminal, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'fired', ?8, 1, ?9)
                "#,
                params![
                    occurrence.occurrence_id.as_str(),
                    occurrence.trigger_authority_id.as_str(),
                    occurrence.record_revision as i64,
                    occurrence.nominal_at.as_ref().map(Timestamp::as_str),
                    json_string(&occurrence.represented_set)?,
                    occurrence.missed_fire_result,
                    occurrence.overlap_result,
                    occurrence.disposition_observation_id.as_str(),
                    occurrence.created_at.as_str(),
                ],
            )?;
        }
        transaction.execute(
            r#"
            INSERT INTO call_trigger_firings(
                firing_id, occurrence_id, trigger_authority_id, record_revision,
                policy_revision, call_id, idempotency_key_ref,
                firing_observation_id, target_result_ref, state, fired_at, completed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, 'active', ?9, NULL)
            "#,
            params![
                firing.firing_id.as_str(),
                firing.occurrence_id.as_str(),
                firing.trigger_authority_id.as_str(),
                firing.record_revision as i64,
                firing.policy_revision as i64,
                firing.call_id.as_str(),
                firing.idempotency_key_ref,
                firing.firing_observation_id.as_str(),
                firing.fired_at.as_str(),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn complete_call_trigger_firing(
        &self,
        firing_id: &CallTriggerFiringId,
        target_result_ref: Option<&ResultRef>,
        completed_at: &Timestamp,
    ) -> Result<()> {
        let changed = self.conn.execute(
            r#"
            UPDATE call_trigger_firings
            SET target_result_ref = ?1, state = 'terminal', completed_at = ?2
            WHERE firing_id = ?3 AND state = 'active'
            "#,
            params![
                target_result_ref.map(ResultRef::as_str),
                completed_at.as_str(),
                firing_id.as_str(),
            ],
        )?;
        if changed != 1 {
            bail!("trigger firing completion did not match one active firing");
        }
        Ok(())
    }

    pub fn call_trigger_pending_occurrence(
        &self,
        pending_occurrence_id: &CallTriggerPendingOccurrenceId,
    ) -> Result<Option<MctTriggerPendingOccurrenceRecord>> {
        self.conn
            .query_row(
                r#"
                SELECT pending_occurrence_id, occurrence_id, trigger_authority_id,
                       record_revision, policy_revision, admission_sequence,
                       pending_reason, represented_set_json, admission_observation_id,
                       state, admitted_at, consumed_at
                FROM call_trigger_pending_occurrences
                WHERE pending_occurrence_id = ?1
                "#,
                params![pending_occurrence_id.as_str()],
                trigger_pending_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn coalesced_call_trigger_pending_occurrence(
        &self,
        trigger_authority_id: &CallTriggerAuthorityId,
    ) -> Result<Option<MctTriggerPendingOccurrenceRecord>> {
        self.conn
            .query_row(
                r#"
                SELECT pending_occurrence_id, occurrence_id, trigger_authority_id,
                       record_revision, policy_revision, admission_sequence,
                       pending_reason, represented_set_json, admission_observation_id,
                       state, admitted_at, consumed_at
                FROM call_trigger_pending_occurrences
                WHERE trigger_authority_id = ?1 AND state = 'pending'
                  AND pending_reason = 'overlap_coalesced'
                ORDER BY admission_sequence LIMIT 1
                "#,
                params![trigger_authority_id.as_str()],
                trigger_pending_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn update_coalesced_call_trigger_pending_occurrence(
        &self,
        pending_occurrence_id: &CallTriggerPendingOccurrenceId,
        represented_set: &CallTriggerRepresentedSet,
        nominal_at: Option<&Timestamp>,
        observation_id: &ObservationId,
    ) -> Result<()> {
        let transaction =
            rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let occurrence_id: String = transaction.query_row(
            r#"
            SELECT occurrence_id FROM call_trigger_pending_occurrences
            WHERE pending_occurrence_id = ?1 AND state = 'pending'
              AND pending_reason = 'overlap_coalesced'
            "#,
            params![pending_occurrence_id.as_str()],
            |row| row.get(0),
        )?;
        transaction.execute(
            r#"
            UPDATE call_trigger_pending_occurrences
            SET represented_set_json = ?1, admission_observation_id = ?2
            WHERE pending_occurrence_id = ?3 AND state = 'pending'
            "#,
            params![
                json_string(represented_set)?,
                observation_id.as_str(),
                pending_occurrence_id.as_str(),
            ],
        )?;
        transaction.execute(
            r#"
            UPDATE call_trigger_occurrences
            SET represented_set_json = ?1, nominal_at = COALESCE(?2, nominal_at),
                disposition_observation_id = ?3
            WHERE occurrence_id = ?4 AND terminal = 0
            "#,
            params![
                json_string(represented_set)?,
                nominal_at.map(Timestamp::as_str),
                observation_id.as_str(),
                occurrence_id
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn suppress_call_trigger_pending_occurrence(
        &self,
        pending_occurrence_id: &CallTriggerPendingOccurrenceId,
        observation_id: &ObservationId,
        suppressed_at: &Timestamp,
    ) -> Result<()> {
        let transaction =
            rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let occurrence_id: String = transaction.query_row(
            r#"
            SELECT occurrence_id FROM call_trigger_pending_occurrences
            WHERE pending_occurrence_id = ?1 AND state = 'pending'
            "#,
            params![pending_occurrence_id.as_str()],
            |row| row.get(0),
        )?;
        transaction.execute(
            r#"
            UPDATE call_trigger_pending_occurrences
            SET state = 'suppressed', consumed_at = ?1
            WHERE pending_occurrence_id = ?2 AND state = 'pending'
            "#,
            params![suppressed_at.as_str(), pending_occurrence_id.as_str()],
        )?;
        transaction.execute(
            r#"
            UPDATE call_trigger_occurrences
            SET final_disposition = 'suppressed', terminal = 1,
                disposition_observation_id = ?1, overlap_result = 'dequeue_current_law_denied'
            WHERE occurrence_id = ?2 AND terminal = 0
            "#,
            params![observation_id.as_str(), occurrence_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn call_trigger_pending_occurrences(
        &self,
    ) -> Result<Vec<MctTriggerPendingOccurrenceRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT pending_occurrence_id, occurrence_id, trigger_authority_id,
                   record_revision, policy_revision, admission_sequence,
                   pending_reason, represented_set_json, admission_observation_id,
                   state, admitted_at, consumed_at
            FROM call_trigger_pending_occurrences
            WHERE state = 'pending'
            ORDER BY trigger_authority_id, admission_sequence
            "#,
        )?;
        stmt.query_map([], trigger_pending_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn update_call_trigger_projection_checkpoint(
        &self,
        ledger_identity: &str,
        last_ledger_sequence: u64,
    ) -> Result<()> {
        if ledger_identity.trim().is_empty() {
            bail!("trigger projection ledger identity must not be blank");
        }
        self.conn.execute(
            r#"
            INSERT INTO call_trigger_projection_meta(
                projection_name, ledger_identity, last_ledger_sequence, updated_at
            ) VALUES ('trigger-runtime-v1', ?1, ?2, ?3)
            ON CONFLICT(projection_name) DO UPDATE SET
                ledger_identity = excluded.ledger_identity,
                last_ledger_sequence = excluded.last_ledger_sequence,
                updated_at = excluded.updated_at
            "#,
            params![
                ledger_identity,
                last_ledger_sequence as i64,
                current_timestamp_string(),
            ],
        )?;
        Ok(())
    }

    pub fn call_idempotency_entry_state(
        &self,
        caller_scope: &str,
        idempotency_key: &str,
    ) -> Result<Option<MctIdempotencyEntryState>> {
        let value = self
            .conn
            .query_row(
                r#"
                SELECT entry_state FROM call_idempotency_entries
                WHERE caller_scope = ?1 AND idempotency_key = ?2
                "#,
                params![caller_scope, idempotency_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        value
            .map(|state| match state.as_str() {
                "in_flight" => Ok(MctIdempotencyEntryState::InFlight),
                "completed" => Ok(MctIdempotencyEntryState::Completed),
                other => bail!("unknown idempotency entry state '{other}'"),
            })
            .transpose()
    }

    pub fn reserve_call_idempotency(
        &self,
        caller_scope: &str,
        idempotency_key: &str,
        fingerprint: &MctIdempotencyFingerprint,
        now: &Timestamp,
        expires_at: &Timestamp,
        caller_budget: usize,
    ) -> Result<MctIdempotencyReservation> {
        if caller_scope.trim().is_empty() || idempotency_key.trim().is_empty() {
            bail!("idempotency scope and key must not be blank");
        }
        if expires_at <= now {
            bail!("idempotency expiry must follow reservation time");
        }

        let transaction =
            rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        transaction.execute(
            "DELETE FROM call_idempotency_entries WHERE expires_at <= ?1",
            params![now.as_str()],
        )?;
        let stored = transaction
            .query_row(
                r#"
                SELECT target_identity, call_id, payload_digest, entry_state,
                       recorded_reply_json, inline_result_payload, expires_at
                FROM call_idempotency_entries
                WHERE caller_scope = ?1 AND idempotency_key = ?2
                "#,
                params![caller_scope, idempotency_key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<Vec<u8>>>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .optional()?;
        let entry_count = transaction.query_row(
            "SELECT COUNT(*) FROM call_idempotency_entries WHERE caller_scope = ?1",
            params![caller_scope],
            |row| row.get::<_, i64>(0),
        )? as usize;
        let stored_facts = stored
            .as_ref()
            .map(
                |(target, call_id, payload_digest, state, _, _, expires_at)| {
                    Ok(MctIdempotencyStoredEntry {
                        fingerprint: MctIdempotencyFingerprint {
                            target: target.clone(),
                            call_id: CallId::new(call_id.clone())?,
                            payload_digest: payload_digest.clone(),
                        },
                        state: match state.as_str() {
                            "in_flight" => MctIdempotencyEntryState::InFlight,
                            "completed" => MctIdempotencyEntryState::Completed,
                            other => bail!("unknown idempotency entry state '{other}'"),
                        },
                        expires_at: Timestamp::new(expires_at.clone())?,
                    })
                },
            )
            .transpose()?;
        let decision = evaluate_idempotency_request(
            fingerprint,
            stored_facts.as_ref(),
            entry_count,
            caller_budget,
            now,
        );

        let reservation = match decision.reason {
            MctIdempotencyReason::ExecuteFresh => {
                transaction.execute(
                    r#"
                    INSERT INTO call_idempotency_entries(
                        caller_scope, idempotency_key, target_identity, call_id,
                        payload_digest, entry_state, recorded_reply_json,
                        inline_result_payload, created_at, completed_at, expires_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, 'in_flight', NULL, NULL, ?6, NULL, ?7)
                    "#,
                    params![
                        caller_scope,
                        idempotency_key,
                        fingerprint.target,
                        fingerprint.call_id.as_str(),
                        fingerprint.payload_digest,
                        now.as_str(),
                        expires_at.as_str(),
                    ],
                )?;
                MctIdempotencyReservation::ExecuteFresh
            }
            MctIdempotencyReason::ReplayCompleted => {
                let (_, _, _, _, reply_json, inline_payload, _) = stored
                    .ok_or_else(|| anyhow::anyhow!("completed idempotency entry disappeared"))?;
                let mut reply: MctRecordedCallReply =
                    serde_json::from_str(reply_json.as_deref().ok_or_else(|| {
                        anyhow::anyhow!("completed idempotency reply is missing")
                    })?)?;
                reply.inline_result_payload = inline_payload;
                MctIdempotencyReservation::Replay(Box::new(reply))
            }
            reason => MctIdempotencyReservation::Refused(reason),
        };
        transaction.commit()?;
        Ok(reservation)
    }

    pub fn complete_call_idempotency(
        &self,
        caller_scope: &str,
        idempotency_key: &str,
        fingerprint: &MctIdempotencyFingerprint,
        reply: &MctRecordedCallReply,
        completed_at: &Timestamp,
    ) -> Result<()> {
        if reply
            .inline_result_payload
            .as_ref()
            .is_some_and(|payload| payload.len() > mct_iroh::MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES)
        {
            bail!("recorded idempotency reply payload exceeds inline result cap");
        }
        let changed = self.conn.execute(
            r#"
            UPDATE call_idempotency_entries
            SET entry_state = 'completed', recorded_reply_json = ?1,
                inline_result_payload = ?2, completed_at = ?3
            WHERE caller_scope = ?4 AND idempotency_key = ?5
              AND target_identity = ?6 AND call_id = ?7 AND payload_digest = ?8
              AND entry_state = 'in_flight'
            "#,
            params![
                serde_json::to_string(reply)?,
                reply.inline_result_payload,
                completed_at.as_str(),
                caller_scope,
                idempotency_key,
                fingerprint.target,
                fingerprint.call_id.as_str(),
                fingerprint.payload_digest,
            ],
        )?;
        if changed != 1 {
            bail!("idempotency completion did not match one in-flight reservation");
        }
        Ok(())
    }

    pub fn upsert_artifact(&self, artifact: &ComponentArtifact) -> Result<()> {
        validate_artifact_provenance_shape(artifact)?;
        self.conn.execute(
            r#"
            INSERT INTO component_artifacts(
                artifact_id, child_name, artifact_version, content_hash, manifest_hash,
                primary_export_json, runtime_shape, ingress_mode, lifecycle_exports,
                verification_status, provenance_status, acquisition_ids_json,
                primary_acquisition_id, created_by_observation_id, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(artifact_id) DO NOTHING
            "#,
            params![
                artifact.artifact_id.as_str(),
                artifact.child_name,
                artifact.artifact_version,
                artifact.content_hash,
                artifact.manifest_hash,
                json_string(&artifact.primary_export)?,
                json_atom(&artifact.runtime_shape)?,
                json_atom(&artifact.ingress_mode)?,
                json_atom(&artifact.lifecycle_exports)?,
                json_atom(&artifact.verification_status)?,
                json_atom(&artifact.provenance_status)?,
                json_string(&artifact.acquisition_ids)?,
                artifact
                    .acquisition_ids
                    .first()
                    .map(ArtifactAcquisitionId::as_str),
                artifact.created_by_observation_id.as_str(),
                current_timestamp_string(),
            ],
        )?;
        let persisted = self
            .get_artifact(&artifact.artifact_id)?
            .context("artifact insert did not produce a persisted row")?;
        if &persisted != artifact {
            bail!("immutable component artifact conflicts with persisted artifact");
        }
        Ok(())
    }

    pub fn get_artifact(
        &self,
        artifact_id: &ComponentArtifactId,
    ) -> Result<Option<ComponentArtifact>> {
        self.conn
            .query_row(
                r#"
                SELECT artifact_id, child_name, artifact_version, content_hash, manifest_hash,
                       primary_export_json, runtime_shape, ingress_mode, lifecycle_exports,
                       verification_status, provenance_status, acquisition_ids_json,
                       created_by_observation_id
                FROM component_artifacts WHERE artifact_id = ?1
                "#,
                params![artifact_id.as_str()],
                artifact_from_row,
            )
            .optional()
            .context("read component artifact")
    }

    pub fn record_artifact_acquisition(&self, acquisition: &ArtifactAcquisition) -> Result<()> {
        validate_acquisition_shape(acquisition)?;
        self.conn.execute(
            r#"
            INSERT INTO artifact_acquisitions(
                acquisition_id, authority_path, standing_source_authority_id,
                operator_pointed_decision_id, adapter_effect_authority_ref, source_ref,
                claimed_child_name, claimed_artifact_version, observed_size_bytes,
                observed_digest, acquisition_outcome, verification_outcome,
                verification_observation_id, acquisition_observation_id,
                component_artifact_id, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            "#,
            params![
                acquisition.acquisition_id.as_str(),
                json_atom(&acquisition.authority_path)?,
                acquisition
                    .standing_source_authority_id
                    .as_ref()
                    .map(ArtifactSourceAuthorityId::as_str),
                acquisition
                    .operator_pointed_decision_id
                    .as_ref()
                    .map(ArtifactAcquisitionDecisionId::as_str),
                acquisition.adapter_effect_authority_ref,
                acquisition.source_ref,
                acquisition.claimed_child_name,
                acquisition.claimed_artifact_version,
                acquisition.observed_size_bytes,
                acquisition.observed_digest,
                json_atom(&acquisition.acquisition_outcome)?,
                json_atom(&acquisition.verification_outcome)?,
                acquisition
                    .verification_observation_id
                    .as_ref()
                    .map(ObservationId::as_str),
                acquisition.acquisition_observation_id.as_str(),
                acquisition
                    .component_artifact_id
                    .as_ref()
                    .map(ComponentArtifactId::as_str),
                current_timestamp_string(),
            ],
        )?;
        Ok(())
    }

    pub fn record_operator_acquisition_decision(
        &self,
        decision: &OperatorPointedArtifactAcquisitionDecision,
    ) -> Result<()> {
        if decision.source_ref.trim().is_empty()
            || decision.claimed_child_name.trim().is_empty()
            || decision.claimed_artifact_version.trim().is_empty()
            || decision.policy_revision == 0
        {
            bail!("operator-pointed acquisition decision is incomplete");
        }
        self.conn.execute(
            r#"
            INSERT INTO operator_pointed_artifact_acquisition_decisions(
                decision_id, source_ref, claimed_child_name, claimed_artifact_version,
                expected_digest, issuer_principal_ref, policy_revision, decision_state,
                authority_observation_id, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                decision.decision_id.as_str(),
                decision.source_ref,
                decision.claimed_child_name,
                decision.claimed_artifact_version,
                decision.expected_digest,
                decision.issuer_principal_ref,
                decision.policy_revision,
                json_atom(&decision.decision_state)?,
                decision.authority_observation_id.as_str(),
                current_timestamp_string(),
            ],
        )?;
        Ok(())
    }

    pub fn consume_operator_acquisition_decision(
        &self,
        decision_id: &ArtifactAcquisitionDecisionId,
    ) -> Result<()> {
        let changed = self.conn.execute(
            r#"
            UPDATE operator_pointed_artifact_acquisition_decisions
            SET decision_state = 'consumed', updated_at = ?1
            WHERE decision_id = ?2 AND decision_state = 'active'
            "#,
            params![current_timestamp_string(), decision_id.as_str()],
        )?;
        if changed != 1 {
            bail!("operator-pointed acquisition decision is absent or already consumed");
        }
        Ok(())
    }

    pub fn operator_acquisition_decisions(
        &self,
    ) -> Result<Vec<OperatorPointedArtifactAcquisitionDecision>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT decision_id, source_ref, claimed_child_name, claimed_artifact_version,
                   expected_digest, issuer_principal_ref, policy_revision, decision_state,
                   authority_observation_id
            FROM operator_pointed_artifact_acquisition_decisions
            ORDER BY updated_at, decision_id
            "#,
        )?;
        stmt.query_map([], |row| {
            let state: String = row.get(7)?;
            Ok(OperatorPointedArtifactAcquisitionDecision {
                decision_id: ArtifactAcquisitionDecisionId::new(row.get::<_, String>(0)?)
                    .expect("stored operator decision id is non-empty"),
                source_ref: row.get(1)?,
                claimed_child_name: row.get(2)?,
                claimed_artifact_version: row.get(3)?,
                expected_digest: row.get(4)?,
                issuer_principal_ref: row.get(5)?,
                policy_revision: row.get::<_, i64>(6)?.max(0) as u64,
                decision_state: from_json_atom(&state).map_err(to_sql_error)?,
                authority_observation_id: ObservationId::new(row.get::<_, String>(8)?)
                    .expect("stored operator authority observation id is non-empty"),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("read operator-pointed artifact acquisition decisions")
    }

    pub fn artifact_acquisitions(&self) -> Result<Vec<ArtifactAcquisition>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT acquisition_id, authority_path, standing_source_authority_id,
                   operator_pointed_decision_id, adapter_effect_authority_ref, source_ref,
                   claimed_child_name, claimed_artifact_version, observed_size_bytes,
                   observed_digest, acquisition_outcome, verification_outcome,
                   verification_observation_id, acquisition_observation_id,
                   component_artifact_id
            FROM artifact_acquisitions ORDER BY created_at, acquisition_id
            "#,
        )?;
        stmt.query_map([], acquisition_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("read artifact acquisitions")
    }

    pub fn record_verified_acquisition_and_artifact(
        &self,
        acquisition: &ArtifactAcquisition,
        artifact: &ComponentArtifact,
        package_path: &Path,
    ) -> Result<()> {
        if acquisition.acquisition_outcome != ArtifactAcquisitionOutcome::Acquired
            || acquisition.verification_outcome != ArtifactVerificationOutcome::Verified
            || acquisition.component_artifact_id.as_ref() != Some(&artifact.artifact_id)
            || artifact.provenance_status != ArtifactProvenanceStatus::AcquisitionBacked
            || !artifact
                .acquisition_ids
                .contains(&acquisition.acquisition_id)
        {
            bail!("verified artifact transaction requires matching successful acquisition");
        }
        validate_artifact_provenance_shape(artifact)?;
        validate_acquisition_shape(acquisition)?;
        if let Some(existing) = self.get_artifact(&artifact.artifact_id)?
            && !same_immutable_artifact_facts(&existing, artifact)
        {
            bail!("immutable component artifact conflicts with persisted artifact");
        }
        let tx = self.conn.unchecked_transaction()?;
        insert_acquisition_on(&tx, acquisition)?;
        insert_artifact_on(&tx, artifact)?;
        tx.execute(
            r#"
            INSERT INTO component_artifact_acquisitions(artifact_id, acquisition_id)
            VALUES (?1, ?2)
            "#,
            params![
                artifact.artifact_id.as_str(),
                acquisition.acquisition_id.as_str()
            ],
        )?;
        tx.execute(
            r#"
            INSERT INTO component_artifact_packages(artifact_id, package_path, published_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(artifact_id) DO NOTHING
            "#,
            params![
                artifact.artifact_id.as_str(),
                package_path.display().to_string(),
                current_timestamp_string(),
            ],
        )?;
        tx.commit()?;
        let persisted = self
            .get_artifact(&artifact.artifact_id)?
            .context("verified artifact transaction did not persist artifact")?;
        if !same_immutable_artifact_facts(&persisted, artifact) {
            bail!("immutable component artifact conflicts with persisted artifact");
        }
        let package = self
            .artifact_package(&artifact.artifact_id)?
            .context("verified artifact transaction did not persist package")?;
        if package.package_path != package_path {
            bail!("immutable artifact package path conflicts with persisted package");
        }
        Ok(())
    }

    pub fn rollback_artifact_publication(
        &self,
        acquisition_id: &ArtifactAcquisitionId,
    ) -> Result<Option<PathBuf>> {
        let artifact_id = self
            .conn
            .query_row(
                "SELECT component_artifact_id FROM artifact_acquisitions WHERE acquisition_id = ?1",
                params![acquisition_id.as_str()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM component_artifact_acquisitions WHERE acquisition_id = ?1",
            params![acquisition_id.as_str()],
        )?;
        tx.execute(
            "DELETE FROM artifact_acquisitions WHERE acquisition_id = ?1",
            params![acquisition_id.as_str()],
        )?;
        let mut remove_package = None;
        if let Some(artifact_id) = artifact_id {
            let remaining: i64 = tx.query_row(
                "SELECT COUNT(*) FROM component_artifact_acquisitions WHERE artifact_id = ?1",
                params![artifact_id],
                |row| row.get(0),
            )?;
            if remaining == 0 {
                remove_package = tx
                    .query_row(
                        "SELECT package_path FROM component_artifact_packages WHERE artifact_id = ?1",
                        params![artifact_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                    .map(PathBuf::from);
                tx.execute(
                    "DELETE FROM component_artifact_packages WHERE artifact_id = ?1",
                    params![artifact_id],
                )?;
                tx.execute(
                    "DELETE FROM component_artifacts WHERE artifact_id = ?1",
                    params![artifact_id],
                )?;
            }
        }
        tx.commit()?;
        Ok(remove_package)
    }

    pub fn artifact_package(
        &self,
        artifact_id: &ComponentArtifactId,
    ) -> Result<Option<MctArtifactPackageRecord>> {
        self.conn
            .query_row(
                r#"
                SELECT artifact_id, package_path, published_at
                FROM component_artifact_packages WHERE artifact_id = ?1
                "#,
                params![artifact_id.as_str()],
                |row| {
                    Ok(MctArtifactPackageRecord {
                        artifact_id: ComponentArtifactId::new(row.get::<_, String>(0)?)
                            .expect("stored artifact id is non-empty"),
                        package_path: PathBuf::from(row.get::<_, String>(1)?),
                        published_at: row.get(2)?,
                    })
                },
            )
            .optional()
            .context("read component artifact package")
    }

    pub fn validate_source_authority_projection(
        &self,
        source: &ArtifactSourceAuthority,
        record_digest: &str,
    ) -> Result<()> {
        validate_source_authority(source)?;
        let expected_digest = blake3::hash(&serde_json::to_vec(source)?)
            .to_hex()
            .to_string();
        if record_digest != expected_digest {
            bail!("artifact source authority record digest does not match its projection");
        }
        if let Some((persisted, _)) = self
            .source_authorities()?
            .into_iter()
            .find(|(persisted, _)| persisted.source_authority_id == source.source_authority_id)
            && (persisted.source_ref != source.source_ref
                || persisted.scope != source.scope
                || persisted.integrity_policy_ref != source.integrity_policy_ref
                || persisted.provenance_policy_ref != source.provenance_policy_ref
                || persisted.issuer_principal_ref != source.issuer_principal_ref
                || persisted.policy_revision != source.policy_revision
                || persisted.issued_at != source.issued_at
                || persisted.expires_at != source.expires_at)
        {
            bail!("artifact source authority immutable facts conflict with persisted record");
        }
        Ok(())
    }

    pub fn upsert_source_authority(
        &self,
        source: &ArtifactSourceAuthority,
        record_digest: &str,
    ) -> Result<()> {
        self.validate_source_authority_projection(source, record_digest)?;
        self.conn.execute(
            r#"
            INSERT INTO artifact_source_authorities(
                source_authority_id, source_ref, scope_json, integrity_policy_ref,
                provenance_policy_ref, issuer_principal_ref, policy_revision,
                authority_state, issued_at, expires_at, authority_observation_id,
                record_digest, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(source_authority_id) DO UPDATE SET
                authority_state = excluded.authority_state,
                authority_observation_id = excluded.authority_observation_id,
                record_digest = excluded.record_digest,
                updated_at = excluded.updated_at
            "#,
            params![
                source.source_authority_id.as_str(),
                source.source_ref,
                json_string(&source.scope)?,
                source.integrity_policy_ref,
                source.provenance_policy_ref,
                source.issuer_principal_ref,
                source.policy_revision,
                json_atom(&source.authority_state)?,
                source.issued_at.as_str(),
                source.expires_at.as_str(),
                source.authority_observation_id.as_str(),
                record_digest,
                current_timestamp_string(),
            ],
        )?;
        Ok(())
    }

    pub fn source_authorities(&self) -> Result<Vec<(ArtifactSourceAuthority, String)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT source_authority_id, source_ref, scope_json, integrity_policy_ref,
                   provenance_policy_ref, issuer_principal_ref, policy_revision,
                   authority_state, issued_at, expires_at, authority_observation_id,
                   record_digest
            FROM artifact_source_authorities ORDER BY source_authority_id
            "#,
        )?;
        stmt.query_map([], source_authority_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("read artifact source authorities")
    }

    pub fn upsert_child_approval(&self, approval: &ChildApproval) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO child_approvals(
                approval_id, artifact_id, child_name, artifact_version, scope_vision_id,
                scope_node_id, scope_project_id, approval_state, policy_revision,
                authority_observation_id, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(approval_id) DO UPDATE SET
                artifact_id = excluded.artifact_id,
                child_name = excluded.child_name,
                artifact_version = excluded.artifact_version,
                scope_vision_id = excluded.scope_vision_id,
                scope_node_id = excluded.scope_node_id,
                scope_project_id = excluded.scope_project_id,
                approval_state = excluded.approval_state,
                policy_revision = excluded.policy_revision,
                authority_observation_id = excluded.authority_observation_id,
                updated_at = excluded.updated_at
            "#,
            params![
                approval.approval_id.as_str(),
                approval.artifact_id.as_str(),
                approval.child_name,
                approval.artifact_version,
                approval.scope_vision_id.as_ref().map(VisionId::as_str),
                approval.scope_node_id.as_ref().map(MctNodeId::as_str),
                approval.scope_project_id.as_ref().map(ProjectId::as_str),
                json_atom(&approval.approval_state)?,
                approval.policy_revision,
                approval.authority_observation_id.as_str(),
                current_timestamp_string(),
            ],
        )?;
        Ok(())
    }

    pub fn upsert_child_assignment(&self, assignment: &ChildAssignment) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO child_assignments(
                assignment_id, approval_id, artifact_id, child_name, vision_id, node_id,
                project_id, assignment_state, pinned_artifact_version,
                assignment_observation_id, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(assignment_id) DO UPDATE SET
                approval_id = excluded.approval_id,
                artifact_id = excluded.artifact_id,
                child_name = excluded.child_name,
                vision_id = excluded.vision_id,
                node_id = excluded.node_id,
                project_id = excluded.project_id,
                assignment_state = excluded.assignment_state,
                pinned_artifact_version = excluded.pinned_artifact_version,
                assignment_observation_id = excluded.assignment_observation_id,
                updated_at = excluded.updated_at
            "#,
            params![
                assignment.assignment_id.as_str(),
                assignment.approval_id.as_str(),
                assignment.artifact_id.as_str(),
                assignment.child_name,
                assignment.vision_id.as_str(),
                assignment.node_id.as_ref().map(MctNodeId::as_str),
                assignment.project_id.as_ref().map(ProjectId::as_str),
                json_atom(&assignment.assignment_state)?,
                assignment.pinned_artifact_version,
                assignment.assignment_observation_id.as_str(),
                current_timestamp_string(),
            ],
        )?;
        Ok(())
    }

    pub fn upsert_child_instance(&self, instance: &ChildInstance) -> Result<()> {
        upsert_child_instance_on(&self.conn, instance)
    }

    /// Atomically commits a verified replacement generation and retires its ready predecessor.
    ///
    /// The replacement is written first inside the transaction. The predecessor must still be
    /// durably ready, so readers observe either the old ready generation or the committed pair of
    /// stopped predecessor and ready replacement, never a child with no ready generation.
    pub fn swap_ready_child_generation(
        &self,
        stopped_predecessor: &ChildInstance,
        ready_replacement: &ChildInstance,
    ) -> Result<()> {
        if stopped_predecessor.instance_state != ChildInstanceState::Stopped {
            bail!("predecessor generation must be stopped before persisted swap");
        }
        if ready_replacement.instance_state != ChildInstanceState::Ready {
            bail!("replacement generation must be ready before persisted swap");
        }
        if stopped_predecessor.instance_id == ready_replacement.instance_id
            || stopped_predecessor.child_name != ready_replacement.child_name
            || stopped_predecessor.assignment_id != ready_replacement.assignment_id
            || stopped_predecessor.artifact_id != ready_replacement.artifact_id
            || stopped_predecessor.node_id != ready_replacement.node_id
            || stopped_predecessor.generation.checked_add(1) != Some(ready_replacement.generation)
        {
            bail!("replacement generation does not directly succeed its predecessor");
        }

        let transaction = self.conn.unchecked_transaction()?;
        let persisted_state: String = transaction
            .query_row(
                "SELECT instance_state FROM child_instances WHERE instance_id = ?1",
                params![stopped_predecessor.instance_id.as_str()],
                |row| row.get(0),
            )
            .context("read persisted predecessor generation")?;
        if persisted_state != "ready" {
            bail!("persisted predecessor generation is not ready");
        }

        upsert_child_instance_on(&transaction, ready_replacement)?;
        upsert_child_instance_on(&transaction, stopped_predecessor)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn upsert_peer(&self, peer: &MctPeerAddressBookEntry) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO peers(
                peer_node_id, binding_id, endpoint_id, vision_id, ticket_json,
                binding_state, policy_revision, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(peer_node_id) DO UPDATE SET
                binding_id = excluded.binding_id,
                endpoint_id = excluded.endpoint_id,
                vision_id = excluded.vision_id,
                ticket_json = excluded.ticket_json,
                binding_state = excluded.binding_state,
                policy_revision = excluded.policy_revision,
                updated_at = excluded.updated_at
            "#,
            params![
                peer.peer_node_id.as_str(),
                peer.binding_id.as_str(),
                peer.endpoint_id.as_str(),
                peer.vision_id.as_str(),
                peer.ticket.as_ref().map(json_string).transpose()?,
                json_atom(&peer.binding_state)?,
                peer.policy_revision,
                peer.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn ingest_config(&self, config: &MctDaemonConfig) -> Result<()> {
        for peer in config.peers.values() {
            self.upsert_peer(peer)?;
        }
        Ok(())
    }

    pub fn refresh_remote_callable_surfaces(
        &self,
        refresh: MctRemoteSurfaceRefresh<'_>,
    ) -> Result<()> {
        let MctRemoteSurfaceRefresh {
            peer_node_id,
            binding_id,
            endpoint_id,
            view,
            received_at,
            stale_at,
            view_observation_id,
        } = refresh;
        validate_remote_capability_view(peer_node_id, view)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM remote_callable_surfaces WHERE peer_node_id = ?1 AND vision_id = ?2",
            params![peer_node_id.as_str(), view.vision_id.as_str()],
        )?;
        tx.execute(
            r#"
            INSERT INTO remote_surface_views(
                peer_node_id, vision_id, binding_id, endpoint_id, publisher_policy_revision,
                published_at, received_at, stale_at, view_observation_id, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(peer_node_id, vision_id) DO UPDATE SET
                binding_id = excluded.binding_id,
                endpoint_id = excluded.endpoint_id,
                publisher_policy_revision = excluded.publisher_policy_revision,
                published_at = excluded.published_at,
                received_at = excluded.received_at,
                stale_at = excluded.stale_at,
                view_observation_id = excluded.view_observation_id,
                updated_at = excluded.updated_at
            "#,
            params![
                peer_node_id.as_str(),
                view.vision_id.as_str(),
                binding_id.as_str(),
                endpoint_id.as_str(),
                view.policy_revision,
                view.published_at.as_str(),
                received_at.as_str(),
                stale_at.as_str(),
                view_observation_id.as_str(),
                current_timestamp_string(),
            ],
        )?;
        for surface in &view.callable_surfaces {
            tx.execute(
                r#"
                INSERT INTO remote_callable_surfaces(
                    peer_node_id, vision_id, child_name, operation_id, binding_id, endpoint_id,
                    publisher_policy_revision, runtime_kind, surface_policy_revision, visibility,
                    received_at, stale_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                "#,
                params![
                    peer_node_id.as_str(),
                    view.vision_id.as_str(),
                    surface.child_name.as_str(),
                    surface.operation_id.as_str(),
                    binding_id.as_str(),
                    endpoint_id.as_str(),
                    view.policy_revision,
                    json_atom(&surface.runtime_kind)?,
                    surface.policy_revision,
                    surface.visibility.as_str(),
                    received_at.as_str(),
                    stale_at.as_str(),
                    current_timestamp_string(),
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remote_callable_surfaces(
        &self,
        peer_node_id: &MctNodeId,
        vision_id: &VisionId,
    ) -> Result<Vec<MctRemoteCallableSurfaceRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT peer_node_id, binding_id, endpoint_id, vision_id, publisher_policy_revision,
                   child_name, operation_id, runtime_kind, surface_policy_revision, visibility,
                   received_at, stale_at
            FROM remote_callable_surfaces
            WHERE peer_node_id = ?1 AND vision_id = ?2
            ORDER BY operation_id, child_name
            "#,
        )?;
        stmt.query_map(params![peer_node_id.as_str(), vision_id.as_str()], |row| {
            remote_callable_surface_from_row(row)
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("read remote callable surfaces")
    }

    pub fn fresh_remote_callable_surfaces_for_operation(
        &self,
        vision_id: &VisionId,
        operation_id: &str,
        now: &Timestamp,
    ) -> Result<Vec<MctRemoteCallableSurfaceRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT peer_node_id, binding_id, endpoint_id, vision_id, publisher_policy_revision,
                   child_name, operation_id, runtime_kind, surface_policy_revision, visibility,
                   received_at, stale_at
            FROM remote_callable_surfaces
            WHERE vision_id = ?1 AND operation_id = ?2 AND stale_at > ?3
            ORDER BY peer_node_id, child_name
            "#,
        )?;
        stmt.query_map(
            params![vision_id.as_str(), operation_id, now.as_str()],
            remote_callable_surface_from_row,
        )?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("read fresh remote callable surfaces")
    }

    pub fn record_loaded_child_candidate(
        &self,
        child: &MctLoadedChild,
        scope: MctOperatorChildScope,
    ) -> Result<ComponentArtifact> {
        let artifact = ComponentArtifact {
            artifact_id: ComponentArtifactId::new(child.artifact_id.clone())
                .expect("string ID literal/generated value must be non-empty"),
            child_name: child.name.clone(),
            artifact_version: child.version.clone(),
            content_hash: format!("sha256:{}", child.wasm_digest.sha256),
            manifest_hash: format!("sha256:{}", child.manifest_digest.sha256),
            primary_export: component_export_from_allowed_operations(&child.allowed_operations),
            runtime_shape: ComponentRuntimeShape::WasmComponent,
            ingress_mode: match child.ingress_mode {
                crate::MctChildIngressMode::Handle => ChildIngressMode::Handle,
                crate::MctChildIngressMode::Hybrid => ChildIngressMode::Hybrid,
                crate::MctChildIngressMode::WitOnly => ChildIngressMode::WitOnly,
            },
            lifecycle_exports: LifecycleExports::AbsentAllowed,
            verification_status: if child.wasm_digest.verified && child.manifest_digest.verified {
                VerificationStatus::Verified
            } else {
                VerificationStatus::Rejected
            },
            provenance_status: ArtifactProvenanceStatus::HistoricalUnknown,
            acquisition_ids: Vec::new(),
            created_by_observation_id: ObservationId::new(format!("obs:artifact:{}", child.name))
                .expect("string ID literal/generated value must be non-empty"),
        };
        self.upsert_artifact(&artifact)?;
        let candidate = ChildApproval {
            approval_id: ChildApprovalId::new(format!("candidate:{}", child.name))
                .expect("string ID literal/generated value must be non-empty"),
            artifact_id: artifact.artifact_id.clone(),
            child_name: child.name.clone(),
            artifact_version: child.version.clone(),
            scope_vision_id: Some(scope.vision_id),
            scope_node_id: Some(scope.node_id),
            scope_project_id: scope.project_id,
            approval_state: ChildApprovalState::Candidate,
            policy_revision: scope.policy_revision,
            authority_observation_id: ObservationId::new(format!("obs:candidate:{}", child.name))
                .expect("string ID literal/generated value must be non-empty"),
        };
        self.upsert_child_approval(&candidate)?;
        Ok(artifact)
    }

    pub fn insert_run_started(
        &self,
        run_id: impl Into<String>,
        call: &MctCall,
        runtime_kind: RuntimeKind,
        provenance: Option<&ChildInvocationProvenance>,
        started_at: impl Into<String>,
    ) -> Result<MctRuntimeRunRecord> {
        let run_id = run_id.into();
        let started_at = started_at.into();
        let child_name = provenance.map(|auth| auth.child_name.clone());
        let child_instance_id = provenance.map(|auth| auth.child_instance_id.clone());
        let authority_decision_id = provenance.map(|auth| auth.authority_decision_id.clone());
        self.conn.execute(
            r#"
            INSERT INTO runtime_runs(
                run_id, call_id, runtime_kind, child_name, child_instance_id,
                authority_decision_id, trace_id, state, started_at, completed_at,
                call_json, child_invocation_provenance_json, result_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'running', ?8, NULL, ?9, ?10, NULL)
            "#,
            params![
                run_id,
                call.call_id.as_str(),
                json_atom(&runtime_kind)?,
                child_name,
                child_instance_id.as_ref().map(ChildInstanceId::as_str),
                authority_decision_id.as_ref().map(DecisionId::as_str),
                call.trace_context.trace_id.as_str(),
                started_at,
                json_string(call)?,
                provenance.map(json_string).transpose()?,
            ],
        )?;
        self.get_run(&run_id)?
            .ok_or_else(|| anyhow::anyhow!("run disappeared after insert"))
    }

    pub fn complete_run(
        &self,
        run_id: &str,
        result: &MctResult,
        completed_at: impl Into<String>,
    ) -> Result<MctRuntimeRunRecord> {
        let state = MctRuntimeRunState::terminal_for_result(result);
        self.conn.execute(
            r#"
            UPDATE runtime_runs
            SET state = ?1, completed_at = ?2, result_json = ?3
            WHERE run_id = ?4
            "#,
            params![
                json_atom(&state)?,
                completed_at.into(),
                json_string(result)?,
                run_id
            ],
        )?;
        self.get_run(run_id)?
            .ok_or_else(|| anyhow::anyhow!("unknown runtime run '{run_id}'"))
    }

    pub fn append_run_observations(
        &self,
        run_id: &str,
        observations: &[MctObservation],
    ) -> Result<()> {
        for observation in observations {
            self.conn.execute(
                r#"
                INSERT OR REPLACE INTO runtime_run_observations(
                    run_id, observation_id, observation_kind, observation_json
                ) VALUES (?1, ?2, ?3, ?4)
                "#,
                params![
                    run_id,
                    observation.observation_id.as_str(),
                    json_atom(&observation.kind)?,
                    json_string(observation)?,
                ],
            )?;
        }
        Ok(())
    }

    pub fn get_run(&self, run_id: &str) -> Result<Option<MctRuntimeRunRecord>> {
        self.conn
            .query_row(
                r#"
                SELECT run_id, call_id, runtime_kind, child_name, child_instance_id,
                       authority_decision_id, trace_id, state, started_at, completed_at,
                       call_json, child_invocation_provenance_json, result_json
                FROM runtime_runs WHERE run_id = ?1
                "#,
                params![run_id],
                run_from_row,
            )
            .optional()
            .context("read runtime run")
    }

    pub fn get_run_by_call_id(&self, call_id: &CallId) -> Result<Option<MctRuntimeRunRecord>> {
        self.conn
            .query_row(
                r#"
                SELECT run_id, call_id, runtime_kind, child_name, child_instance_id,
                       authority_decision_id, trace_id, state, started_at, completed_at,
                       call_json, child_invocation_provenance_json, result_json
                FROM runtime_runs WHERE call_id = ?1
                ORDER BY started_at DESC LIMIT 1
                "#,
                params![call_id.as_str()],
                run_from_row,
            )
            .optional()
            .context("read runtime run by call id")
    }

    pub fn list_runs(&self, limit: u32) -> Result<Vec<MctRuntimeRunRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT run_id, call_id, runtime_kind, child_name, child_instance_id,
                   authority_decision_id, trace_id, state, started_at, completed_at,
                   call_json, child_invocation_provenance_json, result_json
            FROM runtime_runs ORDER BY started_at DESC, run_id DESC LIMIT ?1
            "#,
        )?;
        stmt.query_map(params![limit], run_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("list runtime runs")
    }

    pub fn put_child_state(&self, child_name: &str, key: &str, value_json: &str) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO child_state(child_name, key, value_json, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(child_name, key) DO UPDATE SET
                value_json = excluded.value_json,
                updated_at = excluded.updated_at
            "#,
            params![child_name, key, value_json, current_timestamp_string()],
        )?;
        Ok(())
    }

    pub fn get_child_state(&self, child_name: &str, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value_json FROM child_state WHERE child_name = ?1 AND key = ?2",
                params![child_name, key],
                |row| row.get(0),
            )
            .optional()
            .context("read child state")
    }

    pub fn delete_child_state(&self, child_name: &str, key: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM child_state WHERE child_name = ?1 AND key = ?2",
            params![child_name, key],
        )?;
        Ok(())
    }

    pub fn list_child_state_prefix(&self, child_name: &str, prefix: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT key FROM child_state WHERE child_name = ?1 AND key LIKE ?2 ORDER BY key",
        )?;
        let like = format!("{prefix}%");
        stmt.query_map(params![child_name, like], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("list child state prefix")
    }

    pub fn put_child_checkpoint(
        &self,
        child_name: &str,
        stream: &str,
        checkpoint_json: &str,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO child_checkpoints(child_name, stream, checkpoint_json, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(child_name, stream) DO UPDATE SET
                checkpoint_json = excluded.checkpoint_json,
                updated_at = excluded.updated_at
            "#,
            params![
                child_name,
                stream,
                checkpoint_json,
                current_timestamp_string()
            ],
        )?;
        Ok(())
    }

    pub fn get_child_checkpoint(&self, child_name: &str, stream: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT checkpoint_json FROM child_checkpoints WHERE child_name = ?1 AND stream = ?2",
                params![child_name, stream],
                |row| row.get(0),
            )
            .optional()
            .context("read child checkpoint")
    }

    pub fn ensure_child_subscription(&self, child_name: &str, stream: &str) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT OR IGNORE INTO child_subscriptions(child_name, stream, created_at)
            VALUES (?1, ?2, ?3)
            "#,
            params![child_name, stream, current_timestamp_string()],
        )?;
        Ok(())
    }

    pub fn list_child_subscriptions(&self, child_name: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT stream FROM child_subscriptions WHERE child_name = ?1 ORDER BY stream",
        )?;
        stmt.query_map(params![child_name], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("list child subscriptions")
    }

    pub fn ack_child_offset(&self, child_name: &str, stream: &str, offset: u64) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO child_offsets(child_name, stream, acked_offset, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(child_name, stream) DO UPDATE SET
                acked_offset = MAX(child_offsets.acked_offset, excluded.acked_offset),
                updated_at = excluded.updated_at
            "#,
            params![
                child_name,
                stream,
                offset as i64,
                current_timestamp_string()
            ],
        )?;
        Ok(())
    }

    pub fn get_child_offset(&self, child_name: &str, stream: &str) -> Result<Option<u64>> {
        let value: Option<i64> = self
            .conn
            .query_row(
                "SELECT acked_offset FROM child_offsets WHERE child_name = ?1 AND stream = ?2",
                params![child_name, stream],
                |row| row.get(0),
            )
            .optional()
            .context("read child offset")?;
        Ok(value.map(|value| value.max(0) as u64))
    }

    pub fn enqueue_task(
        &self,
        child_name: &str,
        intent: &MctTaskIntentRecord,
    ) -> Result<MctQueuedTaskRecord> {
        let task_id = intent
            .dedupe_key
            .as_ref()
            .map(|dedupe| format!("task:{child_name}:{dedupe}"))
            .unwrap_or_else(|| {
                format!(
                    "task:{}:{}:{}",
                    child_name,
                    intent.kind,
                    current_timestamp_string()
                )
            });
        let now = current_timestamp_string();
        self.conn.execute(
            r#"
            INSERT OR IGNORE INTO runtime_tasks(
                task_id, child_name, kind, payload_json, dedupe_key, status,
                lease_owner, lease_until, attempts, last_error, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, 'queued', NULL, NULL, 0, NULL, ?6, ?6)
            "#,
            params![
                task_id,
                child_name,
                intent.kind,
                intent.payload_json,
                intent.dedupe_key,
                now,
            ],
        )?;
        self.get_task(&task_id)?
            .ok_or_else(|| anyhow::anyhow!("task disappeared after enqueue"))
    }

    pub fn lease_next_task(
        &self,
        child_name: &str,
        lease_owner: &str,
        lease_until: &str,
    ) -> Result<Option<MctQueuedTaskRecord>> {
        let task = self
            .conn
            .query_row(
                r#"
                SELECT task_id, child_name, kind, payload_json, dedupe_key, status,
                       lease_owner, lease_until, attempts, last_error, created_at, updated_at
                FROM runtime_tasks
                WHERE child_name = ?1 AND status = 'queued'
                ORDER BY created_at, task_id
                LIMIT 1
                "#,
                params![child_name],
                task_from_row,
            )
            .optional()
            .context("lease next task")?;
        let Some(task) = task else {
            return Ok(None);
        };
        self.conn.execute(
            r#"
            UPDATE runtime_tasks
            SET status = 'leased', lease_owner = ?1, lease_until = ?2,
                attempts = attempts + 1, updated_at = ?3
            WHERE task_id = ?4 AND status = 'queued'
            "#,
            params![
                lease_owner,
                lease_until,
                current_timestamp_string(),
                task.task_id
            ],
        )?;
        self.get_task(&task.task_id)
    }

    pub fn mark_task_running(&self, task_id: &str) -> Result<()> {
        self.update_task_status(task_id, MctTaskStatus::Running, None)
    }

    pub fn mark_task_succeeded(&self, task_id: &str) -> Result<()> {
        self.update_task_status(task_id, MctTaskStatus::Succeeded, None)
    }

    pub fn mark_task_failed(&self, task_id: &str, error: &str) -> Result<()> {
        self.update_task_status(task_id, MctTaskStatus::Failed, Some(error))
    }

    pub fn mark_task_dead_letter(&self, task_id: &str, error: &str) -> Result<()> {
        self.update_task_status(task_id, MctTaskStatus::DeadLetter, Some(error))
    }

    pub fn get_task(&self, task_id: &str) -> Result<Option<MctQueuedTaskRecord>> {
        self.conn
            .query_row(
                r#"
                SELECT task_id, child_name, kind, payload_json, dedupe_key, status,
                       lease_owner, lease_until, attempts, last_error, created_at, updated_at
                FROM runtime_tasks WHERE task_id = ?1
                "#,
                params![task_id],
                task_from_row,
            )
            .optional()
            .context("read task")
    }

    pub fn list_tasks(&self, child_name: &str, limit: u32) -> Result<Vec<MctQueuedTaskRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT task_id, child_name, kind, payload_json, dedupe_key, status,
                   lease_owner, lease_until, attempts, last_error, created_at, updated_at
            FROM runtime_tasks WHERE child_name = ?1 ORDER BY created_at DESC, task_id DESC LIMIT ?2
            "#,
        )?;
        stmt.query_map(params![child_name, limit], task_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("list tasks")
    }

    fn update_task_status(
        &self,
        task_id: &str,
        status: MctTaskStatus,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE runtime_tasks
            SET status = ?1, last_error = ?2, updated_at = ?3
            WHERE task_id = ?4
            "#,
            params![
                json_atom(&status)?,
                error,
                current_timestamp_string(),
                task_id
            ],
        )?;
        Ok(())
    }

    pub fn append_metric_point(&self, point: MctMetricPoint) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO metric_points(metric_name, metric_value, labels_json, observed_at)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![
                point.metric_name,
                point.metric_value,
                point.labels.to_string(),
                point.observed_at,
            ],
        )?;
        Ok(())
    }

    pub fn metric_points(&self) -> Result<Vec<MctMetricPoint>> {
        let mut stmt = self.conn.prepare(
            "SELECT metric_name, metric_value, labels_json, observed_at FROM metric_points ORDER BY observed_at",
        )?;
        stmt.query_map([], |row| {
            let labels: String = row.get(2)?;
            Ok(MctMetricPoint {
                metric_name: row.get(0)?,
                metric_value: row.get(1)?,
                labels: serde_json::from_str(&labels).unwrap_or(serde_json::Value::Null),
                observed_at: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("read metric points")
    }

    pub fn upsert_registry_source(&self, source: MctRegistrySourceRecord) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO child_registry_sources(source_id, source_path, last_sync_at, last_loaded, last_failed, state)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(source_id) DO UPDATE SET
                source_path = excluded.source_path,
                last_sync_at = excluded.last_sync_at,
                last_loaded = excluded.last_loaded,
                last_failed = excluded.last_failed,
                state = excluded.state
            "#,
            params![
                source.source_id,
                source.source_path.display().to_string(),
                source.last_sync_at,
                source.last_loaded,
                source.last_failed,
                source.state,
            ],
        )?;
        Ok(())
    }

    pub fn insert_composition_run(&self, composition: MctCompositionRunRecord) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO composition_runs(composition_id, state, steps_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(composition_id) DO UPDATE SET
                state = excluded.state,
                steps_json = excluded.steps_json,
                updated_at = excluded.updated_at
            "#,
            params![
                composition.composition_id,
                composition.state,
                composition.steps_json.to_string(),
                composition.created_at,
                composition.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_toy_contract(&self, contract: &CanonicalToyContract) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO toy_catalog_contracts(
                toy_id, contract_json, authority_bearing, catalog_revision,
                admitted_by_observation_id, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(toy_id) DO UPDATE SET
                contract_json = excluded.contract_json,
                authority_bearing = excluded.authority_bearing,
                catalog_revision = excluded.catalog_revision,
                admitted_by_observation_id = excluded.admitted_by_observation_id,
                updated_at = excluded.updated_at
            "#,
            params![
                contract.toy_id.as_str(),
                json_string(&contract.contract)?,
                if contract.authority_bearing {
                    1_i64
                } else {
                    0_i64
                },
                contract.catalog_revision,
                contract.admitted_by_observation_id.as_str(),
                current_timestamp_string(),
            ],
        )?;
        Ok(())
    }

    pub fn toy_contracts(&self) -> Result<Vec<CanonicalToyContract>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT toy_id, contract_json, authority_bearing, catalog_revision, admitted_by_observation_id
            FROM toy_catalog_contracts
            ORDER BY toy_id
            "#,
        )?;
        stmt.query_map([], toy_contract_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("read toy catalog contracts")
    }

    pub fn upsert_toy_grant_snapshot(&self, grant: &ToyGrant) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO toy_grant_snapshots(
                grant_id, toy_id, subject_json, scope_json, constraints_json,
                grant_state, issuer_id, policy_revision, grants_revision,
                authority_observation_id, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(grant_id) DO UPDATE SET
                toy_id = excluded.toy_id,
                subject_json = excluded.subject_json,
                scope_json = excluded.scope_json,
                constraints_json = excluded.constraints_json,
                grant_state = excluded.grant_state,
                issuer_id = excluded.issuer_id,
                policy_revision = excluded.policy_revision,
                grants_revision = excluded.grants_revision,
                authority_observation_id = excluded.authority_observation_id,
                updated_at = excluded.updated_at
            "#,
            params![
                grant.grant_id.as_str(),
                grant.toy_id.as_str(),
                json_string(&grant.subject)?,
                json_string(&grant.scope)?,
                json_string(&grant.constraints)?,
                json_atom(&grant.grant_state)?,
                grant.issuer_id,
                grant.policy_revision,
                grant.grants_revision,
                grant.authority_observation_id.as_str(),
                current_timestamp_string(),
            ],
        )?;
        Ok(())
    }

    pub fn toy_grant_snapshots(&self) -> Result<Vec<ToyGrant>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT grant_id, toy_id, subject_json, scope_json, constraints_json,
                   grant_state, issuer_id, policy_revision, grants_revision, authority_observation_id
            FROM toy_grant_snapshots
            ORDER BY grant_id
            "#,
        )?;
        stmt.query_map([], toy_grant_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("read toy grant snapshots")
    }

    fn count(&self, table: &str, where_clause: Option<&str>) -> Result<u64> {
        if !is_known_count_table(table) {
            bail!("unknown count table '{table}'");
        }
        let sql = if let Some(where_clause) = where_clause {
            format!("SELECT COUNT(*) FROM {table} WHERE {where_clause}")
        } else {
            format!("SELECT COUNT(*) FROM {table}")
        };
        let count: i64 = self.conn.query_row(&sql, [], |row| row.get(0))?;
        Ok(count as u64)
    }
}

fn is_known_count_table(table: &str) -> bool {
    matches!(
        table,
        "component_artifacts"
            | "child_approvals"
            | "child_assignments"
            | "child_instances"
            | "peers"
            | "runtime_runs"
            | "runtime_tasks"
            | "child_state"
            | "child_subscriptions"
            | "metric_points"
            | "toy_catalog_contracts"
            | "toy_grant_snapshots"
            | "call_trigger_authorities"
            | "call_trigger_pending_occurrences"
            | "call_trigger_firings"
            | "call_trigger_occurrences"
            | "call_trigger_projection_meta"
            | "watch_observation_scopes"
            | "watch_scope_sequence_counters"
            | "watch_event_batches"
            | "watch_scope_observed_subjects"
            | "watch_events"
            | "watch_event_delivery_dispositions"
            | "watch_event_deliveries"
            | "watch_projection_meta"
            | "child_keyvalue_entries"
    )
}

fn trigger_occurrence_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MctTriggerOccurrenceRecord> {
    Ok(MctTriggerOccurrenceRecord {
        occurrence_id: CallTriggerOccurrenceId::new(row.get::<_, String>(0)?)
            .expect("stored trigger occurrence id must be non-empty"),
        trigger_authority_id: CallTriggerAuthorityId::new(row.get::<_, String>(1)?)
            .expect("stored trigger authority id must be non-empty"),
        record_revision: row.get::<_, i64>(2)?.max(0) as u64,
        nominal_at: row
            .get::<_, Option<String>>(3)?
            .map(|value| Timestamp::new(value).expect("stored nominal_at must be RFC3339")),
        represented_set: from_json_cell(&row.get::<_, String>(4)?).map_err(to_sql_error)?,
        missed_fire_result: row.get(5)?,
        overlap_result: row.get(6)?,
        final_disposition: from_json_atom(&row.get::<_, String>(7)?).map_err(to_sql_error)?,
        disposition_observation_id: ObservationId::new(row.get::<_, String>(8)?)
            .expect("stored trigger disposition observation id must be non-empty"),
        created_at: Timestamp::new(row.get::<_, String>(9)?)
            .expect("stored trigger occurrence created_at must be RFC3339"),
    })
}

fn trigger_firing_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MctTriggerFiringRecord> {
    Ok(MctTriggerFiringRecord {
        firing_id: CallTriggerFiringId::new(row.get::<_, String>(0)?)
            .expect("stored trigger firing id must be non-empty"),
        occurrence_id: CallTriggerOccurrenceId::new(row.get::<_, String>(1)?)
            .expect("stored trigger occurrence id must be non-empty"),
        trigger_authority_id: CallTriggerAuthorityId::new(row.get::<_, String>(2)?)
            .expect("stored trigger authority id must be non-empty"),
        record_revision: row.get::<_, i64>(3)?.max(0) as u64,
        policy_revision: row.get::<_, i64>(4)?.max(0) as u64,
        call_id: CallId::new(row.get::<_, String>(5)?)
            .expect("stored trigger call id must be non-empty"),
        idempotency_key_ref: row.get(6)?,
        firing_observation_id: ObservationId::new(row.get::<_, String>(7)?)
            .expect("stored trigger firing observation id must be non-empty"),
        target_result_ref: row
            .get::<_, Option<String>>(8)?
            .map(|value| ResultRef::new(value).expect("stored result ref must be non-empty")),
        state: row.get(9)?,
        fired_at: Timestamp::new(row.get::<_, String>(10)?)
            .expect("stored fired_at must be RFC3339"),
        completed_at: row
            .get::<_, Option<String>>(11)?
            .map(|value| Timestamp::new(value).expect("stored completed_at must be RFC3339")),
    })
}

fn trigger_pending_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MctTriggerPendingOccurrenceRecord> {
    let pending_reason = from_json_atom(&row.get::<_, String>(6)?).map_err(to_sql_error)?;
    let represented_set = from_json_cell(&row.get::<_, String>(7)?).map_err(to_sql_error)?;
    Ok(MctTriggerPendingOccurrenceRecord {
        pending_occurrence_id: CallTriggerPendingOccurrenceId::new(row.get::<_, String>(0)?)
            .expect("stored trigger pending id must be non-empty"),
        occurrence_id: CallTriggerOccurrenceId::new(row.get::<_, String>(1)?)
            .expect("stored trigger occurrence id must be non-empty"),
        trigger_authority_id: CallTriggerAuthorityId::new(row.get::<_, String>(2)?)
            .expect("stored trigger authority id must be non-empty"),
        record_revision: row.get::<_, i64>(3)?.max(0) as u64,
        policy_revision: row.get::<_, i64>(4)?.max(0) as u64,
        admission_sequence: row.get::<_, i64>(5)?.max(0) as u64,
        pending_reason,
        represented_set,
        admission_observation_id: ObservationId::new(row.get::<_, String>(8)?)
            .expect("stored trigger pending observation id must be non-empty"),
        state: row.get(9)?,
        admitted_at: Timestamp::new(row.get::<_, String>(10)?)
            .expect("stored admitted_at must be RFC3339"),
        consumed_at: row
            .get::<_, Option<String>>(11)?
            .map(|value| Timestamp::new(value).expect("stored consumed_at must be RFC3339")),
    })
}

fn validate_remote_capability_view(
    peer_node_id: &MctNodeId,
    view: &MctHelloCapabilityView,
) -> Result<()> {
    if &view.node_id != peer_node_id {
        bail!("remote capability view node does not match admitted peer");
    }
    for surface in &view.callable_surfaces {
        if surface.child_name.trim().is_empty() {
            bail!("remote callable surface child_name must not be empty");
        }
        if surface.operation_id.trim().is_empty() {
            bail!("remote callable surface operation_id must not be empty");
        }
        if surface.vision_id != view.vision_id {
            bail!("remote callable surface vision does not match view vision");
        }
        if surface.visibility != "vision_scoped" {
            bail!("remote callable surface visibility must be vision_scoped");
        }
    }
    Ok(())
}

fn remote_callable_surface_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MctRemoteCallableSurfaceRecord> {
    let runtime_kind: String = row.get(7)?;
    Ok(MctRemoteCallableSurfaceRecord {
        peer_node_id: MctNodeId::new(row.get::<_, String>(0)?)
            .expect("string ID literal/generated value must be non-empty"),
        binding_id: PeerBindingId::new(row.get::<_, String>(1)?)
            .expect("string ID literal/generated value must be non-empty"),
        endpoint_id: EndpointIdText::new(row.get::<_, String>(2)?)
            .expect("string ID literal/generated value must be non-empty"),
        vision_id: VisionId::new(row.get::<_, String>(3)?)
            .expect("string ID literal/generated value must be non-empty"),
        publisher_policy_revision: row.get::<_, i64>(4)?.max(0) as u64,
        child_name: row.get(5)?,
        operation_id: row.get(6)?,
        runtime_kind: from_json_atom(&runtime_kind).map_err(to_sql_error)?,
        surface_policy_revision: row.get::<_, i64>(8)?.max(0) as u64,
        visibility: row.get(9)?,
        received_at: Timestamp::new(row.get::<_, String>(10)?)
            .expect("stored received_at timestamp is RFC3339"),
        stale_at: Timestamp::new(row.get::<_, String>(11)?)
            .expect("stored stale_at timestamp is RFC3339"),
    })
}

fn validate_source_authority(source: &ArtifactSourceAuthority) -> Result<()> {
    let source_path = source
        .source_ref
        .strip_prefix("file://")
        .map(Path::new)
        .filter(|path| path.is_absolute());
    let scopes = [
        &source.scope.artifact_scope,
        &source.scope.publisher_scope,
        &source.scope.namespace_scope,
        &source.scope.allowed_actions,
    ];
    let scope_values_valid = scopes
        .iter()
        .all(|values| !values.is_empty() && values.iter().all(|value| !value.trim().is_empty()));
    let broadness_valid = source.scope.scope_mode == ArtifactSourceScopeMode::ExplicitBroad
        || source
            .scope
            .artifact_scope
            .iter()
            .chain(&source.scope.publisher_scope)
            .chain(&source.scope.namespace_scope)
            .all(|value| value != "*");
    let issuer_valid = source
        .issuer_principal_ref
        .strip_prefix("os-uid:")
        .is_some_and(|uid| uid.parse::<u32>().is_ok());
    if source_path.is_none()
        || source.source_ref.contains('?')
        || source.source_ref.contains('#')
        || !scope_values_valid
        || !broadness_valid
        || source
            .scope
            .allowed_actions
            .iter()
            .any(|action| action != "acquire")
        || source.expires_at <= source.issued_at
        || source.integrity_policy_ref != "sha256-sidecars-v1"
        || source.policy_revision == 0
        || !issuer_valid
    {
        bail!("artifact source authority is not explicit, bounded, credential-free, and supported");
    }
    Ok(())
}

fn validate_acquisition_shape(acquisition: &ArtifactAcquisition) -> Result<()> {
    let authority_shape = match acquisition.authority_path {
        ArtifactAcquisitionAuthorityPath::StandingSource => {
            acquisition.standing_source_authority_id.is_some()
                && acquisition.operator_pointed_decision_id.is_none()
        }
        ArtifactAcquisitionAuthorityPath::OperatorPointed => {
            acquisition.standing_source_authority_id.is_none()
                && acquisition.operator_pointed_decision_id.is_some()
        }
    };
    if !authority_shape {
        bail!("artifact acquisition requires exactly one matching authority path");
    }
    if acquisition.component_artifact_id.is_some()
        && (acquisition.acquisition_outcome != ArtifactAcquisitionOutcome::Acquired
            || acquisition.verification_outcome != ArtifactVerificationOutcome::Verified)
    {
        bail!("failed or rejected acquisition cannot reference a component artifact");
    }
    Ok(())
}

fn insert_acquisition_on(
    tx: &rusqlite::Transaction<'_>,
    acquisition: &ArtifactAcquisition,
) -> Result<()> {
    tx.execute(
        r#"
        INSERT INTO artifact_acquisitions(
            acquisition_id, authority_path, standing_source_authority_id,
            operator_pointed_decision_id, adapter_effect_authority_ref, source_ref,
            claimed_child_name, claimed_artifact_version, observed_size_bytes,
            observed_digest, acquisition_outcome, verification_outcome,
            verification_observation_id, acquisition_observation_id,
            component_artifact_id, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        "#,
        params![
            acquisition.acquisition_id.as_str(),
            json_atom(&acquisition.authority_path)?,
            acquisition
                .standing_source_authority_id
                .as_ref()
                .map(ArtifactSourceAuthorityId::as_str),
            acquisition
                .operator_pointed_decision_id
                .as_ref()
                .map(ArtifactAcquisitionDecisionId::as_str),
            acquisition.adapter_effect_authority_ref,
            acquisition.source_ref,
            acquisition.claimed_child_name,
            acquisition.claimed_artifact_version,
            acquisition.observed_size_bytes,
            acquisition.observed_digest,
            json_atom(&acquisition.acquisition_outcome)?,
            json_atom(&acquisition.verification_outcome)?,
            acquisition
                .verification_observation_id
                .as_ref()
                .map(ObservationId::as_str),
            acquisition.acquisition_observation_id.as_str(),
            acquisition
                .component_artifact_id
                .as_ref()
                .map(ComponentArtifactId::as_str),
            current_timestamp_string(),
        ],
    )?;
    Ok(())
}

fn insert_artifact_on(tx: &rusqlite::Transaction<'_>, artifact: &ComponentArtifact) -> Result<()> {
    tx.execute(
        r#"
        INSERT INTO component_artifacts(
            artifact_id, child_name, artifact_version, content_hash, manifest_hash,
            primary_export_json, runtime_shape, ingress_mode, lifecycle_exports,
            verification_status, provenance_status, acquisition_ids_json,
            primary_acquisition_id, created_by_observation_id, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        ON CONFLICT(artifact_id) DO NOTHING
        "#,
        params![
            artifact.artifact_id.as_str(),
            artifact.child_name,
            artifact.artifact_version,
            artifact.content_hash,
            artifact.manifest_hash,
            json_string(&artifact.primary_export)?,
            json_atom(&artifact.runtime_shape)?,
            json_atom(&artifact.ingress_mode)?,
            json_atom(&artifact.lifecycle_exports)?,
            json_atom(&artifact.verification_status)?,
            json_atom(&artifact.provenance_status)?,
            json_string(&artifact.acquisition_ids)?,
            artifact
                .acquisition_ids
                .first()
                .map(ArtifactAcquisitionId::as_str),
            artifact.created_by_observation_id.as_str(),
            current_timestamp_string(),
        ],
    )?;
    Ok(())
}

fn acquisition_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactAcquisition> {
    let authority_path: String = row.get(1)?;
    let acquisition_outcome: String = row.get(10)?;
    let verification_outcome: String = row.get(11)?;
    Ok(ArtifactAcquisition {
        acquisition_id: ArtifactAcquisitionId::new(row.get::<_, String>(0)?)
            .expect("stored acquisition id is non-empty"),
        authority_path: from_json_atom(&authority_path).map_err(to_sql_error)?,
        standing_source_authority_id: row.get::<_, Option<String>>(2)?.map(|value| {
            ArtifactSourceAuthorityId::new(value).expect("stored source authority id is non-empty")
        }),
        operator_pointed_decision_id: row.get::<_, Option<String>>(3)?.map(|value| {
            ArtifactAcquisitionDecisionId::new(value).expect("stored decision id is non-empty")
        }),
        adapter_effect_authority_ref: row.get(4)?,
        source_ref: row.get(5)?,
        claimed_child_name: row.get(6)?,
        claimed_artifact_version: row.get(7)?,
        observed_size_bytes: row.get(8)?,
        observed_digest: row.get(9)?,
        acquisition_outcome: from_json_atom(&acquisition_outcome).map_err(to_sql_error)?,
        verification_outcome: from_json_atom(&verification_outcome).map_err(to_sql_error)?,
        verification_observation_id: row.get::<_, Option<String>>(12)?.map(|value| {
            ObservationId::new(value).expect("stored verification observation id is non-empty")
        }),
        acquisition_observation_id: ObservationId::new(row.get::<_, String>(13)?)
            .expect("stored acquisition observation id is non-empty"),
        component_artifact_id: row.get::<_, Option<String>>(14)?.map(|value| {
            ComponentArtifactId::new(value).expect("stored component artifact id is non-empty")
        }),
    })
}

fn source_authority_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(ArtifactSourceAuthority, String)> {
    let scope_json: String = row.get(2)?;
    let authority_state: String = row.get(7)?;
    Ok((
        ArtifactSourceAuthority {
            source_authority_id: ArtifactSourceAuthorityId::new(row.get::<_, String>(0)?)
                .expect("stored source authority id is non-empty"),
            source_ref: row.get(1)?,
            scope: from_json_cell(&scope_json).map_err(to_sql_error)?,
            integrity_policy_ref: row.get(3)?,
            provenance_policy_ref: row.get(4)?,
            issuer_principal_ref: row.get(5)?,
            policy_revision: row.get(6)?,
            authority_state: from_json_atom(&authority_state).map_err(to_sql_error)?,
            issued_at: Timestamp::new(row.get::<_, String>(8)?)
                .expect("stored issuance timestamp is RFC3339"),
            expires_at: Timestamp::new(row.get::<_, String>(9)?)
                .expect("stored expiry timestamp is RFC3339"),
            authority_observation_id: ObservationId::new(row.get::<_, String>(10)?)
                .expect("stored source observation id is non-empty"),
        },
        row.get(11)?,
    ))
}

fn same_immutable_artifact_facts(left: &ComponentArtifact, right: &ComponentArtifact) -> bool {
    left.artifact_id == right.artifact_id
        && left.child_name == right.child_name
        && left.artifact_version == right.artifact_version
        && left.content_hash == right.content_hash
        && left.manifest_hash == right.manifest_hash
        && left.primary_export == right.primary_export
        && left.runtime_shape == right.runtime_shape
        && left.ingress_mode == right.ingress_mode
        && left.lifecycle_exports == right.lifecycle_exports
        && left.verification_status == right.verification_status
        && left.provenance_status == right.provenance_status
}

fn validate_artifact_provenance_shape(artifact: &ComponentArtifact) -> Result<()> {
    match artifact.provenance_status {
        ArtifactProvenanceStatus::AcquisitionBacked if artifact.acquisition_ids.is_empty() => {
            bail!("acquisition-backed artifact requires acquisition evidence")
        }
        ArtifactProvenanceStatus::HistoricalUnknown if !artifact.acquisition_ids.is_empty() => {
            bail!("historical-unknown artifact cannot claim acquisition evidence")
        }
        _ => Ok(()),
    }
}

fn artifact_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ComponentArtifact> {
    let primary_export_json: String = row.get(5)?;
    let runtime_shape: String = row.get(6)?;
    let ingress_mode: String = row.get(7)?;
    let lifecycle_exports: String = row.get(8)?;
    let verification_status: String = row.get(9)?;
    let provenance_status: String = row.get(10)?;
    let acquisition_ids_json: String = row.get(11)?;
    Ok(ComponentArtifact {
        artifact_id: ComponentArtifactId::new(row.get::<_, String>(0)?)
            .expect("string ID literal/generated value must be non-empty"),
        child_name: row.get(1)?,
        artifact_version: row.get(2)?,
        content_hash: row.get(3)?,
        manifest_hash: row.get(4)?,
        primary_export: from_json_cell(&primary_export_json).map_err(to_sql_error)?,
        runtime_shape: from_json_atom(&runtime_shape).map_err(to_sql_error)?,
        ingress_mode: from_json_atom(&ingress_mode).map_err(to_sql_error)?,
        lifecycle_exports: from_json_atom(&lifecycle_exports).map_err(to_sql_error)?,
        verification_status: from_json_atom(&verification_status).map_err(to_sql_error)?,
        provenance_status: from_json_atom(&provenance_status).map_err(to_sql_error)?,
        acquisition_ids: from_json_cell(&acquisition_ids_json).map_err(to_sql_error)?,
        created_by_observation_id: ObservationId::new(row.get::<_, String>(12)?)
            .expect("string ID literal/generated value must be non-empty"),
    })
}

fn run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MctRuntimeRunRecord> {
    let runtime_kind: String = row.get(2)?;
    let child_instance_id: Option<String> = row.get(4)?;
    let authority_decision_id: Option<String> = row.get(5)?;
    let state: String = row.get(7)?;
    let call_json: String = row.get(10)?;
    let provenance_json: Option<String> = row.get(11)?;
    let result_json: Option<String> = row.get(12)?;
    Ok(MctRuntimeRunRecord {
        run_id: row.get(0)?,
        call_id: CallId::new(row.get::<_, String>(1)?)
            .expect("string ID literal/generated value must be non-empty"),
        runtime_kind: from_json_atom(&runtime_kind).map_err(to_sql_error)?,
        child_name: row.get(3)?,
        child_instance_id: child_instance_id
            .map(|value| ChildInstanceId::new(value).context("decode child_instance_id"))
            .transpose()
            .map_err(to_sql_error)?,
        authority_decision_id: authority_decision_id
            .map(|value| DecisionId::new(value).context("decode authority_decision_id"))
            .transpose()
            .map_err(to_sql_error)?,
        trace_id: TraceId::new(row.get::<_, String>(6)?)
            .expect("string ID literal/generated value must be non-empty"),
        state: from_json_atom(&state).map_err(to_sql_error)?,
        started_at: row.get(8)?,
        completed_at: row.get(9)?,
        call: from_json_cell(&call_json).map_err(to_sql_error)?,
        child_invocation_provenance: provenance_json
            .as_deref()
            .map(from_json_cell)
            .transpose()
            .map_err(to_sql_error)?,
        result: result_json
            .as_deref()
            .map(from_json_cell)
            .transpose()
            .map_err(to_sql_error)?,
    })
}

fn task_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MctQueuedTaskRecord> {
    let status: String = row.get(5)?;
    let attempts: i64 = row.get(8)?;
    Ok(MctQueuedTaskRecord {
        task_id: row.get(0)?,
        child_name: row.get(1)?,
        kind: row.get(2)?,
        payload_json: row.get(3)?,
        dedupe_key: row.get(4)?,
        status: from_json_atom(&status).map_err(to_sql_error)?,
        lease_owner: row.get(6)?,
        lease_until: row.get(7)?,
        attempts: attempts.max(0) as u64,
        last_error: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn toy_contract_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CanonicalToyContract> {
    let contract_json: String = row.get(1)?;
    let authority_bearing: i64 = row.get(2)?;
    Ok(CanonicalToyContract {
        toy_id: ToyId::new(row.get::<_, String>(0)?)
            .expect("string ID literal/generated value must be non-empty"),
        contract: from_json_cell(&contract_json).map_err(to_sql_error)?,
        authority_bearing: authority_bearing != 0,
        catalog_revision: row.get::<_, i64>(3)?.max(0) as u64,
        admitted_by_observation_id: ObservationId::new(row.get::<_, String>(4)?)
            .expect("string ID literal/generated value must be non-empty"),
    })
}

fn toy_grant_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToyGrant> {
    let subject_json: String = row.get(2)?;
    let scope_json: String = row.get(3)?;
    let constraints_json: String = row.get(4)?;
    let grant_state: String = row.get(5)?;
    Ok(ToyGrant {
        grant_id: ToyGrantId::new(row.get::<_, String>(0)?)
            .expect("string ID literal/generated value must be non-empty"),
        toy_id: ToyId::new(row.get::<_, String>(1)?)
            .expect("string ID literal/generated value must be non-empty"),
        subject: from_json_cell(&subject_json).map_err(to_sql_error)?,
        scope: from_json_cell(&scope_json).map_err(to_sql_error)?,
        constraints: from_json_cell(&constraints_json).map_err(to_sql_error)?,
        grant_state: from_json_atom(&grant_state).map_err(to_sql_error)?,
        issuer_id: row.get(6)?,
        policy_revision: row.get::<_, i64>(7)?.max(0) as u64,
        grants_revision: row.get::<_, i64>(8)?.max(0) as u64,
        authority_observation_id: ObservationId::new(row.get::<_, String>(9)?)
            .expect("string ID literal/generated value must be non-empty"),
    })
}

fn upsert_child_instance_on(connection: &Connection, instance: &ChildInstance) -> Result<()> {
    connection.execute(
        r#"
        INSERT INTO child_instances(
            instance_id, assignment_id, artifact_id, child_name, generation, node_id,
            instance_state, readiness_observation_id, last_lifecycle_observation_id, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(instance_id) DO UPDATE SET
            assignment_id = excluded.assignment_id,
            artifact_id = excluded.artifact_id,
            child_name = excluded.child_name,
            generation = excluded.generation,
            node_id = excluded.node_id,
            instance_state = excluded.instance_state,
            readiness_observation_id = excluded.readiness_observation_id,
            last_lifecycle_observation_id = excluded.last_lifecycle_observation_id,
            updated_at = excluded.updated_at
        "#,
        params![
            instance.instance_id.as_str(),
            instance.assignment_id.as_str(),
            instance.artifact_id.as_str(),
            instance.child_name,
            instance.generation,
            instance.node_id.as_str(),
            json_atom(&instance.instance_state)?,
            instance
                .readiness_observation_id
                .as_ref()
                .map(ObservationId::as_str),
            instance.last_lifecycle_observation_id.as_str(),
            current_timestamp_string(),
        ],
    )?;
    Ok(())
}

fn json_string<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).context("encode json cell")
}

fn from_json_cell<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).context("decode json cell")
}

fn json_atom<T: Serialize>(value: &T) -> Result<String> {
    let value = serde_json::to_value(value).context("encode json atom")?;
    match value {
        serde_json::Value::String(text) => Ok(text),
        other => Ok(other.to_string()),
    }
}

fn from_json_atom<T: DeserializeOwned>(value: &str) -> Result<T> {
    let quoted = serde_json::to_string(value).context("quote json atom")?;
    serde_json::from_str(&quoted).context("decode json atom")
}

fn to_sql_error(error: anyhow::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, error.into())
}

fn component_export_from_allowed_operations(allowed_operations: &[String]) -> ComponentWitExport {
    let Some(first) = allowed_operations.first() else {
        return ComponentWitExport {
            namespace: String::new(),
            interface_name: String::new(),
            version: "0.0.0".into(),
            function_names: Vec::new(),
        };
    };

    let Some((namespace, interface_and_function)) = first.split_once('/') else {
        return ComponentWitExport {
            namespace: String::new(),
            interface_name: String::new(),
            version: "0.0.0".into(),
            function_names: allowed_operations.to_vec(),
        };
    };
    let Some((interface_with_version, _function_name)) = interface_and_function.rsplit_once('.')
    else {
        return ComponentWitExport {
            namespace: String::new(),
            interface_name: String::new(),
            version: "0.0.0".into(),
            function_names: allowed_operations.to_vec(),
        };
    };
    let (interface_name, version) = interface_with_version
        .split_once('@')
        .map_or((interface_with_version, "0.0.0"), |(name, version)| {
            (name, version)
        });
    let prefix = format!("{namespace}/{interface_with_version}.");
    let function_names = allowed_operations
        .iter()
        .filter_map(|operation| operation.strip_prefix(&prefix).map(str::to_string))
        .collect();

    ComponentWitExport {
        namespace: namespace.into(),
        interface_name: interface_name.into(),
        version: version.into(),
        function_names,
    }
}

pub fn default_state_path() -> PathBuf {
    PathBuf::from(".mct").join("state.sqlite")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trigger_authority(revision: u64, state: CallTriggerAuthorityState) -> CallTriggerAuthority {
        CallTriggerAuthority {
            trigger_authority_id: CallTriggerAuthorityId::new("trigger-a").unwrap(),
            mother_node_id: MctNodeId::new("node-a").unwrap(),
            vision_id: VisionId::new("vision-a").unwrap(),
            canonical_caller: CallerIdentity {
                node_id: MctNodeId::new("node-a").unwrap(),
                user_id: Some(UserId::new("uid:501").unwrap()),
                vision_id: VisionId::new("vision-a").unwrap(),
                project_id: None,
            },
            target: OperationTarget::new("patina:watch", "control@0.1.0", "scan-now").unwrap(),
            payload_constraint: MctCallPayloadHandle::Empty,
            trigger_source: CallTriggerSource::Temporal {
                anchor_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
                interval_ms: 1_000,
            },
            trigger_source_ref: "blake3:source".into(),
            missed_fire_policy: MissedFirePolicy::Skip,
            overlap_policy: OverlapPolicy::Refuse,
            issuer_principal_ref: "os-uid:501".into(),
            record_revision: revision,
            policy_revision: revision,
            starts_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
            expires_at: Timestamp::new("2026-07-21T13:00:00Z").unwrap(),
            authority_state: state,
            authority_observation_id: ObservationId::new(format!("obs-trigger-a-{revision}"))
                .unwrap(),
            canonical_record_digest: String::new(),
        }
        .seal()
    }

    fn artifact() -> ComponentArtifact {
        ComponentArtifact {
            artifact_id: ComponentArtifactId::new("artifact-a")
                .expect("string ID literal/generated value must be non-empty"),
            child_name: "child-a".into(),
            artifact_version: "0.1.0".into(),
            content_hash: "sha256:wasm".into(),
            manifest_hash: "sha256:manifest".into(),
            primary_export: ComponentWitExport {
                namespace: "patina".into(),
                interface_name: "echo".into(),
                version: "0.1.0".into(),
                function_names: vec!["echo".into()],
            },
            runtime_shape: ComponentRuntimeShape::WasmComponent,
            ingress_mode: ChildIngressMode::WitOnly,
            lifecycle_exports: LifecycleExports::AbsentAllowed,
            verification_status: VerificationStatus::Verified,
            provenance_status: ArtifactProvenanceStatus::HistoricalUnknown,
            acquisition_ids: Vec::new(),
            created_by_observation_id: ObservationId::new("obs-artifact")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn approval(state: ChildApprovalState) -> ChildApproval {
        ChildApproval {
            approval_id: ChildApprovalId::new("approval-a")
                .expect("string ID literal/generated value must be non-empty"),
            artifact_id: ComponentArtifactId::new("artifact-a")
                .expect("string ID literal/generated value must be non-empty"),
            child_name: "child-a".into(),
            artifact_version: "0.1.0".into(),
            scope_vision_id: Some(
                VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            scope_node_id: Some(
                MctNodeId::new("node-a")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            scope_project_id: None,
            approval_state: state,
            policy_revision: 1,
            authority_observation_id: ObservationId::new("obs-approval")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn assignment(state: ChildAssignmentState) -> ChildAssignment {
        ChildAssignment {
            assignment_id: ChildAssignmentId::new("assignment-a")
                .expect("string ID literal/generated value must be non-empty"),
            approval_id: ChildApprovalId::new("approval-a")
                .expect("string ID literal/generated value must be non-empty"),
            artifact_id: ComponentArtifactId::new("artifact-a")
                .expect("string ID literal/generated value must be non-empty"),
            child_name: "child-a".into(),
            vision_id: VisionId::new("vision-a")
                .expect("string ID literal/generated value must be non-empty"),
            node_id: Some(
                MctNodeId::new("node-a")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            project_id: None,
            assignment_state: state,
            pinned_artifact_version: "0.1.0".into(),
            assignment_observation_id: ObservationId::new("obs-assignment")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn instance(state: ChildInstanceState) -> ChildInstance {
        ChildInstance {
            instance_id: ChildInstanceId::new("instance-a")
                .expect("string ID literal/generated value must be non-empty"),
            assignment_id: ChildAssignmentId::new("assignment-a")
                .expect("string ID literal/generated value must be non-empty"),
            artifact_id: ComponentArtifactId::new("artifact-a")
                .expect("string ID literal/generated value must be non-empty"),
            child_name: "child-a".into(),
            generation: 1,
            node_id: MctNodeId::new("node-a")
                .expect("string ID literal/generated value must be non-empty"),
            instance_state: state,
            readiness_observation_id: Some(
                ObservationId::new("obs-ready")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            last_lifecycle_observation_id: ObservationId::new("obs-ready")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn call() -> MctCall {
        MctCall {
            call_id: CallId::new("call-a")
                .expect("string ID literal/generated value must be non-empty"),
            caller: CallerIdentity {
                node_id: MctNodeId::new("node-a")
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
                size_bytes: 0,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 1,
                grants_revision: 1,
                vision_policy_revision: 1,
            },
            deadline: Timestamp::new("2026-05-31T00:01:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-a")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: SpanId::new("span-a")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            origin: CallOrigin::Cli,
        }
    }

    fn authorized() -> AuthorizedChildInvocation {
        crate::authority_test_fixture::authorized_child_for_call(
            &call(),
            "child-a",
            MctNodeId::new("node-a").expect("string ID literal/generated value must be non-empty"),
            "a",
        )
    }

    fn provenance() -> ChildInvocationProvenance {
        ChildInvocationProvenance::from_authorized(
            &authorized(),
            ObservationId::new("obs-child-authority")
                .expect("string ID literal/generated value must be non-empty"),
        )
    }

    fn toy_contract(authority_bearing: bool) -> CanonicalToyContract {
        CanonicalToyContract {
            toy_id: ToyId::new("toy-state")
                .expect("string ID literal/generated value must be non-empty"),
            contract: ToyContractIdentity {
                namespace: "patina".into(),
                interface_name: "state".into(),
                version: "0.1.0".into(),
                function_name: Some("put".into()),
                resource_name: None,
            },
            authority_bearing,
            catalog_revision: 3,
            admitted_by_observation_id: ObservationId::new("obs-toy-catalog")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn toy_grant(state: ToyGrantState) -> ToyGrant {
        ToyGrant {
            grant_id: ToyGrantId::new("grant-state")
                .expect("string ID literal/generated value must be non-empty"),
            toy_id: ToyId::new("toy-state")
                .expect("string ID literal/generated value must be non-empty"),
            subject: ToyGrantSubject {
                child_name: "child-a".into(),
                artifact_id: "artifact-a".into(),
                artifact_version: "0.1.0".into(),
                assignment_id: Some(
                    ChildAssignmentId::new("assignment-a")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                caller_node_id: Some(
                    MctNodeId::new("node-a")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
            },
            scope: ToyGrantScope {
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                node_id: Some(
                    MctNodeId::new("node-a")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                project_id: None,
                data_classification: Some("public".into()),
                resource_id: Some("bucket-a".into()),
                allowed_actions: vec!["put".into()],
            },
            constraints: ToyGrantConstraints {
                starts_at: None,
                expires_at: Some(Timestamp::new("2026-05-31T00:10:00Z").unwrap()),
                max_uses: None,
                max_duration_ms: Some(1000),
                locality_required: true,
            },
            grant_state: state,
            issuer_id: "issuer-a".into(),
            policy_revision: 1,
            grants_revision: 2,
            authority_observation_id: ObservationId::new("obs-toy-grant")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    #[test]
    fn component_artifacts_require_real_acquisition_or_explicit_legacy_migration() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let mut invalid = artifact();
        invalid.provenance_status = ArtifactProvenanceStatus::AcquisitionBacked;
        assert!(store.upsert_artifact(&invalid).is_err());

        let historical = artifact();
        store.upsert_artifact(&historical).unwrap();
        let reopened = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let persisted = reopened
            .get_artifact(&historical.artifact_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            persisted.provenance_status,
            ArtifactProvenanceStatus::HistoricalUnknown
        );
        assert!(persisted.acquisition_ids.is_empty());
    }

    #[test]
    fn pre_v7_artifact_migration_marks_historical_unknown_without_fabricating_acquisition() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite");
        let legacy = artifact();
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE component_artifacts (
                    artifact_id TEXT PRIMARY KEY,
                    child_name TEXT NOT NULL,
                    artifact_version TEXT NOT NULL,
                    content_hash TEXT NOT NULL,
                    manifest_hash TEXT NOT NULL,
                    primary_export_json TEXT NOT NULL,
                    runtime_shape TEXT NOT NULL,
                    ingress_mode TEXT NOT NULL,
                    lifecycle_exports TEXT NOT NULL,
                    verification_status TEXT NOT NULL,
                    created_by_observation_id TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                "#,
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO component_artifacts(
                    artifact_id, child_name, artifact_version, content_hash, manifest_hash,
                    primary_export_json, runtime_shape, ingress_mode, lifecycle_exports,
                    verification_status, created_by_observation_id, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                "#,
                params![
                    legacy.artifact_id.as_str(),
                    legacy.child_name,
                    legacy.artifact_version,
                    legacy.content_hash,
                    legacy.manifest_hash,
                    json_string(&legacy.primary_export).unwrap(),
                    json_atom(&legacy.runtime_shape).unwrap(),
                    json_atom(&legacy.ingress_mode).unwrap(),
                    json_atom(&legacy.lifecycle_exports).unwrap(),
                    json_atom(&legacy.verification_status).unwrap(),
                    legacy.created_by_observation_id.as_str(),
                    "2026-07-15T00:00:00Z",
                ],
            )
            .unwrap();
        drop(connection);

        let store = MctRuntimeStateStore::open(&path).unwrap();
        let migrated = store.get_artifact(&legacy.artifact_id).unwrap().unwrap();
        assert_eq!(
            migrated.provenance_status,
            ArtifactProvenanceStatus::HistoricalUnknown
        );
        assert!(migrated.acquisition_ids.is_empty());
        assert!(store.artifact_acquisitions().unwrap().is_empty());
    }

    #[test]
    fn standing_source_creation_rejects_unbounded_credentialed_and_unsupported_records() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let valid = ArtifactSourceAuthority {
            source_authority_id: ArtifactSourceAuthorityId::new("source-valid").unwrap(),
            source_ref: "file:///tmp/artifacts".into(),
            scope: ArtifactSourceScope {
                scope_mode: ArtifactSourceScopeMode::Constrained,
                artifact_scope: vec!["slate-manager@0.2.0".into()],
                publisher_scope: vec!["patina".into()],
                namespace_scope: vec!["patina:slate".into()],
                allowed_actions: vec!["acquire".into()],
            },
            integrity_policy_ref: "sha256-sidecars-v1".into(),
            provenance_policy_ref: None,
            issuer_principal_ref: "os-uid:501".into(),
            policy_revision: 1,
            authority_state: ArtifactSourceAuthorityState::Active,
            issued_at: Timestamp::new("2026-07-16T00:00:00Z").unwrap(),
            expires_at: Timestamp::new("2099-01-01T00:00:00Z").unwrap(),
            authority_observation_id: ObservationId::new("obs-source-valid").unwrap(),
        };
        let persist = |source: &ArtifactSourceAuthority| {
            let digest = blake3::hash(&serde_json::to_vec(source).unwrap())
                .to_hex()
                .to_string();
            store.upsert_source_authority(source, &digest)
        };
        persist(&valid).unwrap();
        assert!(
            store
                .upsert_source_authority(&valid, &"0".repeat(64))
                .is_err()
        );

        for case in 0..12 {
            let mut invalid = valid.clone();
            invalid.source_authority_id =
                ArtifactSourceAuthorityId::new(format!("source-invalid-{case}")).unwrap();
            invalid.authority_observation_id =
                ObservationId::new(format!("obs-source-invalid-{case}")).unwrap();
            match case {
                0 => invalid.scope.artifact_scope.clear(),
                1 => invalid.scope.publisher_scope.clear(),
                2 => invalid.scope.namespace_scope.clear(),
                3 => invalid.scope.allowed_actions.clear(),
                4 => invalid.scope.artifact_scope = vec!["*".into()],
                5 => invalid.expires_at = invalid.issued_at.clone(),
                6 => invalid.source_ref = "file://relative/path".into(),
                7 => invalid.source_ref = "https://user:secret@example.test/artifact".into(),
                8 => invalid.scope.allowed_actions = vec!["sync".into()],
                9 => invalid.integrity_policy_ref = "trust-source".into(),
                10 => invalid.policy_revision = 0,
                _ => invalid.issuer_principal_ref = "token:secret".into(),
            }
            assert!(
                persist(&invalid).is_err(),
                "invalid source case {case} passed"
            );
        }
    }

    #[test]
    fn state_store_enforces_active_assignment_requires_approved_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        store.upsert_artifact(&artifact()).unwrap();
        store
            .upsert_child_approval(&approval(ChildApprovalState::Candidate))
            .unwrap();

        let result = store.upsert_child_assignment(&assignment(ChildAssignmentState::Active));

        assert!(result.is_err());
        store
            .upsert_child_approval(&approval(ChildApprovalState::Approved))
            .unwrap();
        store
            .upsert_child_assignment(&assignment(ChildAssignmentState::Active))
            .unwrap();
    }

    #[test]
    fn trigger_authority_projection_is_revisioned_current_and_non_resurrecting() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let first = trigger_authority(1, CallTriggerAuthorityState::Active);
        store.insert_call_trigger_authority(&first).unwrap();
        assert_eq!(
            store
                .current_call_trigger_authority(&first.trigger_authority_id)
                .unwrap(),
            Some(first.clone())
        );
        assert!(store.insert_call_trigger_authority(&first).is_err());

        let second = trigger_authority(2, CallTriggerAuthorityState::Active);
        store.insert_call_trigger_authority(&second).unwrap();
        assert_eq!(
            store
                .current_call_trigger_authority(&first.trigger_authority_id)
                .unwrap(),
            Some(second)
        );
        assert_eq!(store.call_trigger_authorities().unwrap().len(), 2);

        let revoked = trigger_authority(3, CallTriggerAuthorityState::Revoked);
        store.insert_call_trigger_authority(&revoked).unwrap();
        assert_eq!(
            store
                .current_call_trigger_authority(&first.trigger_authority_id)
                .unwrap(),
            Some(revoked)
        );
        assert!(
            store
                .insert_call_trigger_authority(&trigger_authority(
                    4,
                    CallTriggerAuthorityState::Active
                ))
                .is_err()
        );
    }

    #[test]
    fn schema_v8_adds_closed_trigger_authority_and_scheduler_projections() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        for table in [
            "call_trigger_authorities",
            "call_trigger_occurrences",
            "call_trigger_pending_occurrences",
            "call_trigger_firings",
            "call_trigger_projection_meta",
        ] {
            assert_eq!(store.count(table, None).unwrap(), 0, "{table}");
        }
    }

    #[test]
    fn schema_v10_adds_watch_authority_delivery_and_bounded_keyvalue_projections() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        for table in [
            "watch_observation_scopes",
            "watch_scope_sequence_counters",
            "watch_event_batches",
            "watch_scope_observed_subjects",
            "watch_events",
            "watch_event_delivery_dispositions",
            "watch_event_deliveries",
            "watch_projection_meta",
            "child_keyvalue_entries",
        ] {
            assert_eq!(store.count(table, None).unwrap(), 0, "{table}");
        }
    }

    #[test]
    fn child_keyvalue_is_bucket_scoped_bounded_and_refuses_without_eviction() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let bucket = "child:artifact:assignment:bucket:default";
        store.child_keyvalue_set(bucket, "config", b"one").unwrap();
        store.child_keyvalue_set(bucket, "config", b"two").unwrap();
        assert_eq!(
            store.child_keyvalue_get(bucket, "config").unwrap(),
            Some(b"two".to_vec())
        );
        assert_eq!(store.child_keyvalue_get("other", "config").unwrap(), None);
        assert!(
            store
                .child_keyvalue_set(
                    bucket,
                    "oversize",
                    &vec![0; MCT_KEYVALUE_VALUE_MAX_BYTES + 1]
                )
                .is_err()
        );
        for index in 1..MCT_KEYVALUE_MAX_KEYS_PER_BUCKET {
            store
                .child_keyvalue_set(bucket, &format!("key-{index:03}"), b"value")
                .unwrap();
        }
        assert!(
            store
                .child_keyvalue_set(bucket, "overflow", b"value")
                .is_err()
        );
        assert_eq!(
            store.child_keyvalue_get(bucket, "config").unwrap(),
            Some(b"two".to_vec())
        );
    }

    #[test]
    fn watch_scope_projection_is_revisioned_current_and_sequences_are_monotonic() {
        fn scope(revision: u64, state: WatchObservationScopeState) -> WatchObservationScope {
            WatchObservationScope {
                watch_scope_id: WatchObservationScopeId::new("watch-state").unwrap(),
                observer_shape: WatchObserverShape::ChildToy,
                observer_ref: WatchObserverRef {
                    child_name: "folder-watch-actor".into(),
                    artifact_id: ComponentArtifactId::new(format!("sha256:{}", "a".repeat(64)))
                        .unwrap(),
                    artifact_version: "0.1.0".into(),
                    assignment_id: ChildAssignmentId::new("assignment-watch").unwrap(),
                },
                scope_mode: WatchScopeMode::Constrained,
                canonical_root_ref: "file:///tmp/watch-state".into(),
                traversal_scope: WatchTraversalScope::Recursive,
                event_classes: vec![WatchEventClass::Created, WatchEventClass::Deleted],
                max_events_per_batch: 16,
                coalescing_policy: WatchCoalescingPolicy::LastPerPath,
                starts_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
                expires_at: Timestamp::new("2026-07-21T13:00:00Z").unwrap(),
                scope_revision: revision,
                policy_revision: 1,
                authority_state: state,
                authority_observation_id: ObservationId::new(format!("obs-watch-state-{revision}"))
                    .unwrap(),
                canonical_record_digest: String::new(),
            }
            .seal()
        }

        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let first = scope(1, WatchObservationScopeState::Active);
        store.insert_watch_observation_scope(&first).unwrap();
        assert_eq!(
            store
                .current_watch_observation_scope(&first.watch_scope_id)
                .unwrap(),
            Some(first.clone())
        );
        assert_eq!(
            store
                .reserve_watch_batch_sequence(&first.watch_scope_id, 1)
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .reserve_watch_batch_sequence(&first.watch_scope_id, 1)
                .unwrap(),
            2
        );
        assert!(store.insert_watch_observation_scope(&first).is_err());

        let revoked = scope(2, WatchObservationScopeState::Revoked);
        store.insert_watch_observation_scope(&revoked).unwrap();
        assert_eq!(
            store
                .current_watch_observation_scope(&first.watch_scope_id)
                .unwrap(),
            Some(revoked)
        );
        assert_eq!(store.watch_observation_scopes().unwrap().len(), 2);
        assert!(
            store
                .insert_watch_observation_scope(&scope(3, WatchObservationScopeState::Active))
                .is_err()
        );
    }

    #[test]
    fn state_store_persists_approved_assignment_and_ready_instance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite");
        let store = MctRuntimeStateStore::open(&path).unwrap();
        store.upsert_artifact(&artifact()).unwrap();
        store
            .upsert_child_approval(&approval(ChildApprovalState::Approved))
            .unwrap();
        store
            .upsert_child_assignment(&assignment(ChildAssignmentState::Active))
            .unwrap();
        store
            .upsert_child_instance(&instance(ChildInstanceState::Ready))
            .unwrap();
        drop(store);

        let reopened = MctRuntimeStateStore::open(path).unwrap();
        let summary = reopened.summary().unwrap();
        assert_eq!(summary.schema_version, SCHEMA_VERSION);
        assert_eq!(summary.artifacts, 1);
        assert_eq!(summary.approved_children, 1);
        assert_eq!(summary.active_assignments, 1);
        assert_eq!(summary.ready_instances, 1);
    }

    #[test]
    fn child_reload_swap_is_atomic_and_failed_swap_keeps_persisted_predecessor_ready() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite");
        let store = MctRuntimeStateStore::open(&path).unwrap();
        store.upsert_artifact(&artifact()).unwrap();
        store
            .upsert_child_approval(&approval(ChildApprovalState::Approved))
            .unwrap();
        store
            .upsert_child_assignment(&assignment(ChildAssignmentState::Active))
            .unwrap();
        let predecessor = instance(ChildInstanceState::Ready);
        store.upsert_child_instance(&predecessor).unwrap();

        let mut stopped = predecessor.clone();
        stopped.instance_state = ChildInstanceState::Stopped;
        stopped.last_lifecycle_observation_id = ObservationId::new("obs-stopped").unwrap();
        let mut invalid_replacement = predecessor.clone();
        invalid_replacement.instance_id = ChildInstanceId::new("instance-b").unwrap();
        invalid_replacement.generation = 2;
        invalid_replacement.instance_state = ChildInstanceState::Loading;
        invalid_replacement.readiness_observation_id = None;
        invalid_replacement.last_lifecycle_observation_id =
            ObservationId::new("obs-loading-b").unwrap();

        let error = store
            .swap_ready_child_generation(&stopped, &invalid_replacement)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("replacement generation must be ready")
        );
        assert_eq!(store.summary().unwrap().ready_instances, 1);
        let persisted: Vec<(String, String)> = store
            .conn
            .prepare("SELECT instance_id, instance_state FROM child_instances ORDER BY generation")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(persisted, vec![("instance-a".into(), "ready".into())]);

        let mut replacement = invalid_replacement;
        replacement.instance_state = ChildInstanceState::Ready;
        replacement.readiness_observation_id = Some(ObservationId::new("obs-ready-b").unwrap());
        replacement.last_lifecycle_observation_id = ObservationId::new("obs-ready-b").unwrap();
        store
            .swap_ready_child_generation(&stopped, &replacement)
            .unwrap();

        let persisted: Vec<(u64, String)> = store
            .conn
            .prepare("SELECT generation, instance_state FROM child_instances ORDER BY generation")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(persisted, vec![(1, "stopped".into()), (2, "ready".into())]);
        assert_eq!(store.summary().unwrap().ready_instances, 1);
    }

    #[test]
    fn state_store_persists_toy_grant_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite");
        let store = MctRuntimeStateStore::open(&path).unwrap();
        store.upsert_toy_contract(&toy_contract(true)).unwrap();
        store
            .upsert_toy_grant_snapshot(&toy_grant(ToyGrantState::Active))
            .unwrap();
        drop(store);

        let reopened = MctRuntimeStateStore::open(path).unwrap();
        assert_eq!(reopened.schema_version().unwrap(), SCHEMA_VERSION);
        let contracts = reopened.toy_contracts().unwrap();
        assert_eq!(contracts, vec![toy_contract(true)]);
        let grants = reopened.toy_grant_snapshots().unwrap();
        assert_eq!(grants, vec![toy_grant(ToyGrantState::Active)]);
        let summary = reopened.summary().unwrap();
        assert_eq!(summary.toy_catalog_contracts, 1);
        assert_eq!(summary.toy_grant_snapshots, 1);
    }

    #[test]
    fn state_store_rejects_active_grant_for_non_authority_toy() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        store.upsert_toy_contract(&toy_contract(false)).unwrap();

        let result = store.upsert_toy_grant_snapshot(&toy_grant(ToyGrantState::Active));

        assert!(result.is_err());
        store
            .upsert_toy_grant_snapshot(&toy_grant(ToyGrantState::Requested))
            .unwrap();
    }

    #[test]
    fn state_store_schema_inventory_stays_private_to_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();

        assert_eq!(store.count("toy_grant_snapshots", None).unwrap(), 0);
        assert!(store.count("beliefs", None).is_err());
        assert!(store.count("sessions", None).is_err());
        assert!(store.count("view_buffers", None).is_err());
    }

    #[test]
    fn state_store_persists_runs_observations_and_metrics() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let call = call();
        let provenance = provenance();
        store
            .insert_run_started(
                "run-a",
                &call,
                RuntimeKind::Process,
                Some(&provenance),
                "2026-05-31T00:00:00Z",
            )
            .unwrap();
        let observation = MctObservation::informational(
            ObservationId::new("obs-run")
                .expect("string ID literal/generated value must be non-empty"),
            Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            ObservationKind::RuntimeExecutionStarted,
            TraceId::new("trace-a").expect("string ID literal/generated value must be non-empty"),
            "started",
        );
        store
            .append_run_observations("run-a", std::slice::from_ref(&observation))
            .unwrap();
        let result = MctResult {
            call_id: CallId::new("call-a")
                .expect("string ID literal/generated value must be non-empty"),
            outcome: ResultOutcome::Success,
            route_taken: Some(RouteTaken {
                node_id: MctNodeId::new("node-a")
                    .expect("string ID literal/generated value must be non-empty"),
                child_id: Some(
                    ChildId::new("child-a")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                runtime_kind: RuntimeKind::Process,
            }),
            authority_decision_ref: DecisionId::new("decision-a")
                .expect("string ID literal/generated value must be non-empty"),
            execution_summary: ExecutionSummary {
                wall_time_ms: 1,
                execution_time_ms: Some(1),
                queue_wait_ms: Some(0),
                input_size_bytes: 0,
                output_size_bytes: Some(2),
            },
            result_payload: MctCallPayloadHandle::Empty,
            requester_message: "ok".into(),
            audit_ref: AuditRef::new("audit-a")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let run = store
            .complete_run("run-a", &result, "2026-05-31T00:00:01Z")
            .unwrap();
        assert_eq!(run.state, MctRuntimeRunState::Completed);
        assert_eq!(run.result, Some(result));

        store
            .append_metric_point(MctMetricPoint {
                metric_name: "runtime.run.completed".into(),
                metric_value: 1,
                labels: serde_json::json!({"runtime": "process"}),
                observed_at: "2026-05-31T00:00:01Z".into(),
            })
            .unwrap();
        let listed = store.list_runs(10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].child_invocation_provenance,
            Some(provenance.clone())
        );
        assert_eq!(store.metric_points().unwrap().len(), 1);
        assert_eq!(store.summary().unwrap().completed_runs, 1);
    }

    #[test]
    fn state_store_migrates_legacy_child_invocation_records_to_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE mct_state_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO mct_state_meta(key, value) VALUES('schema_version', '3');
            CREATE TABLE runtime_runs (
                run_id TEXT PRIMARY KEY,
                call_id TEXT NOT NULL,
                runtime_kind TEXT NOT NULL,
                child_name TEXT,
                child_instance_id TEXT,
                authority_decision_id TEXT,
                trace_id TEXT NOT NULL,
                state TEXT NOT NULL,
                started_at TEXT NOT NULL,
                completed_at TEXT,
                call_json TEXT NOT NULL,
                authorized_child_invocation_json TEXT,
                result_json TEXT
            );
            "#,
        )
        .unwrap();
        let call = call();
        let authorized = authorized();
        let legacy_authorized = LegacyAuthorizedChildInvocation {
            authorized_child_invocation_id: authorized.authorized_child_invocation_id().clone(),
            call_id: authorized.call_id().clone(),
            evaluation_id: authorized.evaluation_id().clone(),
            assignment_id: authorized.assignment_id().clone(),
            approval_id: authorized.approval_id().clone(),
            artifact_id: authorized.artifact_id().clone(),
            child_instance_id: authorized.child_instance_id().clone(),
            child_name: authorized.child_name().to_owned(),
            authority_decision_id: authorized.authority_decision_id().clone(),
        };
        conn.execute(
            r#"
            INSERT INTO runtime_runs(
                run_id, call_id, runtime_kind, child_name, child_instance_id,
                authority_decision_id, trace_id, state, started_at, completed_at,
                call_json, authorized_child_invocation_json, result_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11, NULL)
            "#,
            params![
                "run-legacy",
                call.call_id.as_str(),
                json_atom(&RuntimeKind::Process).unwrap(),
                authorized.child_name(),
                authorized.child_instance_id().as_str(),
                authorized.authority_decision_id().as_str(),
                call.trace_context.trace_id.as_str(),
                json_atom(&MctRuntimeRunState::Running).unwrap(),
                "2026-05-31T00:00:00Z",
                json_string(&call).unwrap(),
                json_string(&legacy_authorized).unwrap(),
            ],
        )
        .unwrap();
        drop(conn);

        let store = MctRuntimeStateStore::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        let run = store.get_run("run-legacy").unwrap().unwrap();
        let provenance = run.child_invocation_provenance.unwrap();
        assert_eq!(
            provenance.authorized_child_invocation_id,
            authorized.authorized_child_invocation_id().clone()
        );
        assert_eq!(
            provenance.authority_observation_id,
            ObservationId::new("obs:migrated-child-authority:run-legacy")
                .expect("string ID literal/generated value must be non-empty")
        );
    }

    #[test]
    fn state_store_persists_child_state_checkpoints_subscriptions_and_offsets() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();

        store
            .put_child_state("child-a", "bucket:key", r#"{"ok":true}"#)
            .unwrap();
        store
            .put_child_state("child-a", "bucket:other", "1")
            .unwrap();
        assert_eq!(
            store.get_child_state("child-a", "bucket:key").unwrap(),
            Some(r#"{"ok":true}"#.into())
        );
        assert_eq!(
            store.list_child_state_prefix("child-a", "bucket:").unwrap(),
            vec!["bucket:key".to_string(), "bucket:other".to_string()]
        );
        store.delete_child_state("child-a", "bucket:other").unwrap();
        assert_eq!(
            store
                .list_child_state_prefix("child-a", "bucket:")
                .unwrap()
                .len(),
            1
        );

        store
            .put_child_checkpoint("child-a", "belief.changed", r#"{"offset":7}"#)
            .unwrap();
        assert_eq!(
            store
                .get_child_checkpoint("child-a", "belief.changed")
                .unwrap(),
            Some(r#"{"offset":7}"#.into())
        );
        store
            .ensure_child_subscription("child-a", "belief.changed")
            .unwrap();
        assert_eq!(
            store.list_child_subscriptions("child-a").unwrap(),
            vec!["belief.changed".to_string()]
        );
        store
            .ack_child_offset("child-a", "belief.changed", 7)
            .unwrap();
        store
            .ack_child_offset("child-a", "belief.changed", 3)
            .unwrap();
        assert_eq!(
            store.get_child_offset("child-a", "belief.changed").unwrap(),
            Some(7)
        );

        let summary = store.summary().unwrap();
        assert_eq!(summary.child_state_keys, 1);
        assert_eq!(summary.child_subscriptions, 1);
    }

    #[test]
    fn trigger_in_flight_recovery_never_expires_into_hidden_reexecution() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite");
        let store = MctRuntimeStateStore::open(&path).unwrap();
        let fingerprint = MctIdempotencyFingerprint {
            target: "patina:demo/control@0.1.0.run".into(),
            call_id: CallId::new("call-trigger-in-flight").unwrap(),
            payload_digest: "blake3:payload".into(),
        };
        store
            .reserve_call_idempotency(
                "trigger:trigger-a",
                "trigger-v1:key-a",
                &fingerprint,
                &Timestamp::new("2026-07-10T00:00:00Z").unwrap(),
                &Timestamp::new("2026-07-10T00:12:00Z").unwrap(),
                1,
            )
            .unwrap();
        drop(store);
        let reopened = MctRuntimeStateStore::open(path).unwrap();
        assert_eq!(
            reopened
                .call_idempotency_entry_state("trigger:trigger-a", "trigger-v1:key-a")
                .unwrap(),
            Some(MctIdempotencyEntryState::InFlight)
        );
        assert_eq!(
            reopened
                .call_idempotency_entry_state("trigger:trigger-a", "trigger-v1:key-a")
                .unwrap(),
            Some(MctIdempotencyEntryState::InFlight),
            "the trigger recovery reader does not apply ordinary TTL eviction"
        );
    }

    #[test]
    fn idempotency_store_scopes_reserves_replays_expires_and_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite");
        let store = MctRuntimeStateStore::open(&path).unwrap();
        let fingerprint = MctIdempotencyFingerprint {
            target: "patina/echo.echo".into(),
            call_id: CallId::new("call-idem-store").unwrap(),
            payload_digest: "digest-store".into(),
        };
        let now = Timestamp::new("2026-07-10T00:00:00Z").unwrap();
        let expires = Timestamp::new("2026-07-10T00:12:00Z").unwrap();

        assert!(matches!(
            store
                .reserve_call_idempotency("binding-a", "key-a", &fingerprint, &now, &expires, 1)
                .unwrap(),
            MctIdempotencyReservation::ExecuteFresh
        ));
        assert!(matches!(
            store
                .reserve_call_idempotency("binding-a", "key-a", &fingerprint, &now, &expires, 1)
                .unwrap(),
            MctIdempotencyReservation::Refused(MctIdempotencyReason::IdempotencyInProgress)
        ));
        assert!(matches!(
            store
                .reserve_call_idempotency("binding-b", "key-a", &fingerprint, &now, &expires, 1)
                .unwrap(),
            MctIdempotencyReservation::ExecuteFresh
        ));

        let recorded = MctRecordedCallReply {
            result_ref: Some(ResultRef::new("result-idem").unwrap()),
            result_payload: MctCallPayloadHandle::InlinePayload {
                inline_payload_ref: "result-payload-idem".into(),
                content_type: "application/json".into(),
                size_bytes: 2,
                blake3_digest_hex: blake3::hash(b"{}").to_hex().to_string(),
            },
            inline_result_payload: Some(b"{}".to_vec()),
            route_decision_id: None,
            route_taken: None,
            outcome: CallProtocolOutcome::Completed,
            protocol_reason: None,
            safe_message: "completed".into(),
        };
        store
            .complete_call_idempotency(
                "binding-a",
                "key-a",
                &fingerprint,
                &recorded,
                &Timestamp::new("2026-07-10T00:01:00Z").unwrap(),
            )
            .unwrap();
        drop(store);

        let reopened = MctRuntimeStateStore::open(&path).unwrap();
        assert_eq!(
            reopened
                .reserve_call_idempotency("binding-a", "key-a", &fingerprint, &now, &expires, 1)
                .unwrap(),
            MctIdempotencyReservation::Replay(Box::new(recorded.clone()))
        );
        let mut mismatch = fingerprint.clone();
        mismatch.call_id = CallId::new("different-call").unwrap();
        assert!(matches!(
            reopened
                .reserve_call_idempotency("binding-a", "key-a", &mismatch, &now, &expires, 1)
                .unwrap(),
            MctIdempotencyReservation::Refused(MctIdempotencyReason::IdempotencyKeyReuseMismatch)
        ));
        assert!(matches!(
            reopened
                .reserve_call_idempotency("binding-a", "new-key", &fingerprint, &now, &expires, 1)
                .unwrap(),
            MctIdempotencyReservation::Refused(MctIdempotencyReason::IdempotencyBudgetFull)
        ));
        assert_eq!(
            reopened
                .reserve_call_idempotency("binding-a", "key-a", &fingerprint, &now, &expires, 1)
                .unwrap(),
            MctIdempotencyReservation::Replay(Box::new(recorded))
        );
        assert!(matches!(
            reopened
                .reserve_call_idempotency(
                    "binding-a",
                    "key-a",
                    &fingerprint,
                    &Timestamp::new("2026-07-10T00:13:00Z").unwrap(),
                    &Timestamp::new("2026-07-10T00:25:00Z").unwrap(),
                    1,
                )
                .unwrap(),
            MctIdempotencyReservation::ExecuteFresh
        ));

        let raw = std::fs::read(&path).unwrap();
        assert!(
            !raw.windows(b"request-secret".len())
                .any(|window| window == b"request-secret")
        );
    }

    #[test]
    fn state_store_leases_and_completes_tasks_with_dedupe() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let intent = MctTaskIntentRecord {
            kind: "native-job".into(),
            payload_json: r#"{"job":"sync"}"#.into(),
            dedupe_key: Some("sync-once".into()),
        };
        let first = store.enqueue_task("child-a", &intent).unwrap();
        let duplicate = store.enqueue_task("child-a", &intent).unwrap();
        assert_eq!(first.task_id, duplicate.task_id);
        assert_eq!(store.summary().unwrap().queued_tasks, 1);

        let leased = store
            .lease_next_task("child-a", "worker-a", "2026-05-31T00:01:00Z")
            .unwrap()
            .unwrap();
        assert_eq!(leased.status, MctTaskStatus::Leased);
        assert_eq!(leased.attempts, 1);
        store.mark_task_running(&leased.task_id).unwrap();
        assert_eq!(
            store.get_task(&leased.task_id).unwrap().unwrap().status,
            MctTaskStatus::Running
        );
        store.mark_task_succeeded(&leased.task_id).unwrap();
        assert_eq!(
            store.get_task(&leased.task_id).unwrap().unwrap().status,
            MctTaskStatus::Succeeded
        );
        assert_eq!(store.summary().unwrap().queued_tasks, 0);
    }
}

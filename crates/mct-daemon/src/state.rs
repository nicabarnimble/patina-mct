use crate::{
    MctDaemonConfig, MctLoadedChild, MctOperatorChildScope, MctPeerAddressBookEntry,
    unix_timestamp_string,
};
use anyhow::{Context, Result, bail};
use mct_kernel::*;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: i64 = 1;

/// Project-local durable runtime state for one standalone MCT node.
///
/// This is an adapter: it enforces storage invariants and persists facts, but it
/// does not create authority. Callers still need kernel authorization records
/// such as `ChildApproval`, `ChildAssignment`, `ChildInstance`,
/// `AuthorizedChildInvocation`, and `AuthorizedToyCall` before effects run.
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
    pub authorized_child_invocation: Option<AuthorizedChildInvocation>,
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
                authorized_child_invocation_json TEXT,
                result_json TEXT
            );

            CREATE TABLE IF NOT EXISTS runtime_run_observations (
                run_id TEXT NOT NULL REFERENCES runtime_runs(run_id) ON DELETE CASCADE,
                observation_id TEXT NOT NULL,
                observation_kind TEXT NOT NULL,
                observation_json TEXT NOT NULL,
                PRIMARY KEY (run_id, observation_id)
            );

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

            CREATE TABLE IF NOT EXISTS composition_runs (
                composition_id TEXT PRIMARY KEY,
                state TEXT NOT NULL,
                steps_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

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
        self.conn.execute(
            "INSERT OR REPLACE INTO mct_state_meta(key, value) VALUES('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
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

    pub fn upsert_artifact(&self, artifact: &ComponentArtifact) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO component_artifacts(
                artifact_id, child_name, artifact_version, content_hash, manifest_hash,
                primary_export_json, runtime_shape, ingress_mode, lifecycle_exports,
                verification_status, created_by_observation_id, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(artifact_id) DO UPDATE SET
                child_name = excluded.child_name,
                artifact_version = excluded.artifact_version,
                content_hash = excluded.content_hash,
                manifest_hash = excluded.manifest_hash,
                primary_export_json = excluded.primary_export_json,
                runtime_shape = excluded.runtime_shape,
                ingress_mode = excluded.ingress_mode,
                lifecycle_exports = excluded.lifecycle_exports,
                verification_status = excluded.verification_status,
                created_by_observation_id = excluded.created_by_observation_id,
                updated_at = excluded.updated_at
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
                artifact.created_by_observation_id.as_str(),
                unix_timestamp_string(),
            ],
        )?;
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
                       verification_status, created_by_observation_id
                FROM component_artifacts WHERE artifact_id = ?1
                "#,
                params![artifact_id.as_str()],
                artifact_from_row,
            )
            .optional()
            .context("read component artifact")
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
                unix_timestamp_string(),
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
                unix_timestamp_string(),
            ],
        )?;
        Ok(())
    }

    pub fn upsert_child_instance(&self, instance: &ChildInstance) -> Result<()> {
        self.conn.execute(
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
                unix_timestamp_string(),
            ],
        )?;
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

    pub fn record_loaded_child_candidate(
        &self,
        child: &MctLoadedChild,
        scope: MctOperatorChildScope,
    ) -> Result<ComponentArtifact> {
        let artifact = ComponentArtifact {
            artifact_id: ComponentArtifactId::from(child.artifact_id.clone()),
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
            created_by_observation_id: ObservationId::from(format!("obs:artifact:{}", child.name)),
        };
        self.upsert_artifact(&artifact)?;
        let candidate = ChildApproval {
            approval_id: ChildApprovalId::from(format!("candidate:{}", child.name)),
            artifact_id: artifact.artifact_id.clone(),
            child_name: child.name.clone(),
            artifact_version: child.version.clone(),
            scope_vision_id: Some(scope.vision_id),
            scope_node_id: Some(scope.node_id),
            scope_project_id: scope.project_id,
            approval_state: ChildApprovalState::Candidate,
            policy_revision: scope.policy_revision,
            authority_observation_id: ObservationId::from(format!("obs:candidate:{}", child.name)),
        };
        self.upsert_child_approval(&candidate)?;
        Ok(artifact)
    }

    pub fn insert_run_started(
        &self,
        run_id: impl Into<String>,
        call: &MctCall,
        runtime_kind: RuntimeKind,
        authorized: Option<&AuthorizedChildInvocation>,
        started_at: impl Into<String>,
    ) -> Result<MctRuntimeRunRecord> {
        let run_id = run_id.into();
        let started_at = started_at.into();
        let child_name = authorized.map(|auth| auth.child_name.clone());
        let child_instance_id = authorized.map(|auth| auth.child_instance_id.clone());
        let authority_decision_id = authorized.map(|auth| auth.authority_decision_id.clone());
        self.conn.execute(
            r#"
            INSERT INTO runtime_runs(
                run_id, call_id, runtime_kind, child_name, child_instance_id,
                authority_decision_id, trace_id, state, started_at, completed_at,
                call_json, authorized_child_invocation_json, result_json
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
                authorized.map(json_string).transpose()?,
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
                       call_json, authorized_child_invocation_json, result_json
                FROM runtime_runs WHERE run_id = ?1
                "#,
                params![run_id],
                run_from_row,
            )
            .optional()
            .context("read runtime run")
    }

    pub fn list_runs(&self, limit: u32) -> Result<Vec<MctRuntimeRunRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT run_id, call_id, runtime_kind, child_name, child_instance_id,
                   authority_decision_id, trace_id, state, started_at, completed_at,
                   call_json, authorized_child_invocation_json, result_json
            FROM runtime_runs ORDER BY started_at DESC, run_id DESC LIMIT ?1
            "#,
        )?;
        stmt.query_map(params![limit], run_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("list runtime runs")
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
            | "metric_points"
    )
}

fn artifact_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ComponentArtifact> {
    let primary_export_json: String = row.get(5)?;
    let runtime_shape: String = row.get(6)?;
    let ingress_mode: String = row.get(7)?;
    let lifecycle_exports: String = row.get(8)?;
    let verification_status: String = row.get(9)?;
    Ok(ComponentArtifact {
        artifact_id: ComponentArtifactId::from(row.get::<_, String>(0)?),
        child_name: row.get(1)?,
        artifact_version: row.get(2)?,
        content_hash: row.get(3)?,
        manifest_hash: row.get(4)?,
        primary_export: from_json_cell(&primary_export_json).map_err(to_sql_error)?,
        runtime_shape: from_json_atom(&runtime_shape).map_err(to_sql_error)?,
        ingress_mode: from_json_atom(&ingress_mode).map_err(to_sql_error)?,
        lifecycle_exports: from_json_atom(&lifecycle_exports).map_err(to_sql_error)?,
        verification_status: from_json_atom(&verification_status).map_err(to_sql_error)?,
        created_by_observation_id: ObservationId::from(row.get::<_, String>(10)?),
    })
}

fn run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MctRuntimeRunRecord> {
    let runtime_kind: String = row.get(2)?;
    let child_instance_id: Option<String> = row.get(4)?;
    let authority_decision_id: Option<String> = row.get(5)?;
    let state: String = row.get(7)?;
    let call_json: String = row.get(10)?;
    let authorized_json: Option<String> = row.get(11)?;
    let result_json: Option<String> = row.get(12)?;
    Ok(MctRuntimeRunRecord {
        run_id: row.get(0)?,
        call_id: CallId::from(row.get::<_, String>(1)?),
        runtime_kind: from_json_atom(&runtime_kind).map_err(to_sql_error)?,
        child_name: row.get(3)?,
        child_instance_id: child_instance_id.map(ChildInstanceId::from),
        authority_decision_id: authority_decision_id.map(DecisionId::from),
        trace_id: TraceId::from(row.get::<_, String>(6)?),
        state: from_json_atom(&state).map_err(to_sql_error)?,
        started_at: row.get(8)?,
        completed_at: row.get(9)?,
        call: from_json_cell(&call_json).map_err(to_sql_error)?,
        authorized_child_invocation: authorized_json
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
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::other(error.to_string())),
    )
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

    fn artifact() -> ComponentArtifact {
        ComponentArtifact {
            artifact_id: ComponentArtifactId::from("artifact-a"),
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
            created_by_observation_id: ObservationId::from("obs-artifact"),
        }
    }

    fn approval(state: ChildApprovalState) -> ChildApproval {
        ChildApproval {
            approval_id: ChildApprovalId::from("approval-a"),
            artifact_id: ComponentArtifactId::from("artifact-a"),
            child_name: "child-a".into(),
            artifact_version: "0.1.0".into(),
            scope_vision_id: Some(VisionId::from("vision-a")),
            scope_node_id: Some(MctNodeId::from("node-a")),
            scope_project_id: None,
            approval_state: state,
            policy_revision: 1,
            authority_observation_id: ObservationId::from("obs-approval"),
        }
    }

    fn assignment(state: ChildAssignmentState) -> ChildAssignment {
        ChildAssignment {
            assignment_id: ChildAssignmentId::from("assignment-a"),
            approval_id: ChildApprovalId::from("approval-a"),
            artifact_id: ComponentArtifactId::from("artifact-a"),
            child_name: "child-a".into(),
            vision_id: VisionId::from("vision-a"),
            node_id: Some(MctNodeId::from("node-a")),
            project_id: None,
            assignment_state: state,
            pinned_artifact_version: "0.1.0".into(),
            assignment_observation_id: ObservationId::from("obs-assignment"),
        }
    }

    fn instance(state: ChildInstanceState) -> ChildInstance {
        ChildInstance {
            instance_id: ChildInstanceId::from("instance-a"),
            assignment_id: ChildAssignmentId::from("assignment-a"),
            artifact_id: ComponentArtifactId::from("artifact-a"),
            child_name: "child-a".into(),
            generation: 1,
            node_id: MctNodeId::from("node-a"),
            instance_state: state,
            readiness_observation_id: Some(ObservationId::from("obs-ready")),
            last_lifecycle_observation_id: ObservationId::from("obs-ready"),
        }
    }

    fn call() -> MctCall {
        MctCall {
            call_id: CallId::from("call-a"),
            caller: CallerIdentity {
                node_id: MctNodeId::from("node-a"),
                user_id: None,
                vision_id: VisionId::from("vision-a"),
                project_id: None,
            },
            target: OperationTarget {
                namespace: "patina".into(),
                interface_name: "echo".into(),
                function_name: "echo".into(),
            },
            payload_metadata: PayloadMetadata {
                data_classification: "public".into(),
                approximate_size_bytes: 0,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 1,
                grants_revision: 1,
                vision_policy_revision: 1,
            },
            deadline: Timestamp::from("2026-05-31T00:01:00Z"),
            trace_context: TraceContext {
                trace_id: TraceId::from("trace-a"),
                span_id: SpanId::from("span-a"),
            },
            origin: CallOrigin::Cli,
        }
    }

    fn authorized() -> AuthorizedChildInvocation {
        AuthorizedChildInvocation {
            authorized_child_invocation_id: AuthorizedChildInvocationId::from("auth-a"),
            call_id: CallId::from("call-a"),
            evaluation_id: ChildCallEvaluationId::from("eval-a"),
            assignment_id: ChildAssignmentId::from("assignment-a"),
            approval_id: ChildApprovalId::from("approval-a"),
            artifact_id: ComponentArtifactId::from("artifact-a"),
            child_instance_id: ChildInstanceId::from("instance-a"),
            child_name: "child-a".into(),
            authority_decision_id: DecisionId::from("decision-a"),
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
    fn state_store_persists_runs_observations_and_metrics() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let call = call();
        let auth = authorized();
        store
            .insert_run_started(
                "run-a",
                &call,
                RuntimeKind::Process,
                Some(&auth),
                "2026-05-31T00:00:00Z",
            )
            .unwrap();
        let observation = MctObservation::informational(
            ObservationId::from("obs-run"),
            Timestamp::from("2026-05-31T00:00:00Z"),
            ObservationKind::RuntimeExecutionStarted,
            TraceId::from("trace-a"),
            "started",
        );
        store
            .append_run_observations("run-a", std::slice::from_ref(&observation))
            .unwrap();
        let result = MctResult {
            call_id: CallId::from("call-a"),
            outcome: ResultOutcome::Success,
            route_taken: Some(RouteTaken {
                node_id: MctNodeId::from("node-a"),
                child_id: Some(ChildId::from("child-a")),
                runtime_kind: RuntimeKind::Process,
            }),
            authority_decision_ref: DecisionId::from("decision-a"),
            execution_summary: ExecutionSummary {
                wall_time_ms: 1,
                execution_time_ms: Some(1),
                queue_wait_ms: Some(0),
                input_size_bytes: 0,
                output_size_bytes: Some(2),
            },
            requester_message: "ok".into(),
            audit_ref: AuditRef::from("audit-a"),
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
        assert_eq!(store.list_runs(10).unwrap().len(), 1);
        assert_eq!(store.metric_points().unwrap().len(), 1);
        assert_eq!(store.summary().unwrap().completed_runs, 1);
    }
}

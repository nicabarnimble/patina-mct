use crate::children::{MctLoadedChild, component_artifact_from_loaded_child};
use anyhow::{Context, Result};
use mct_iroh::MotherIrohEndpointTicket;
use mct_kernel::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctDaemonConfig {
    #[serde(default)]
    pub child_approvals: BTreeMap<String, MctStoredChildApproval>,
    #[serde(default)]
    pub child_assignments: BTreeMap<String, MctStoredChildAssignment>,
    #[serde(default)]
    pub peers: BTreeMap<String, MctPeerAddressBookEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctStoredChildApproval {
    pub child_name: String,
    pub artifact_id: ComponentArtifactId,
    pub artifact_version: String,
    pub approval_state: ChildApprovalState,
    pub vision_id: VisionId,
    pub node_id: MctNodeId,
    pub project_id: Option<ProjectId>,
    pub policy_revision: u64,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctStoredChildAssignment {
    pub child_name: String,
    pub artifact_id: ComponentArtifactId,
    pub artifact_version: String,
    pub assignment_state: ChildAssignmentState,
    pub vision_id: VisionId,
    pub node_id: MctNodeId,
    pub project_id: Option<ProjectId>,
    pub policy_revision: u64,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPeerAddressBookEntry {
    pub peer_node_id: MctNodeId,
    pub binding_id: PeerBindingId,
    pub endpoint_id: EndpointIdText,
    pub vision_id: VisionId,
    pub ticket: Option<MotherIrohEndpointTicket>,
    pub binding_state: BindingState,
    pub policy_revision: u64,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctDaemonConfigStore {
    path: PathBuf,
}

impl MctDaemonConfig {
    pub fn authority_projection_for_loaded_children<'a>(
        &self,
        children: impl IntoIterator<Item = &'a MctLoadedChild>,
        scope: MctOperatorChildScope,
    ) -> MctConfigChildAuthorityProjection {
        let mut artifacts = Vec::new();
        let mut approvals = Vec::new();
        let mut assignments = Vec::new();
        let mut instances = Vec::new();

        for child in children {
            let artifact = component_artifact_from_loaded_child(child);
            artifacts.push(artifact.clone());

            let Some(stored_approval) = self.child_approvals.get(&child.name) else {
                continue;
            };
            let approval_id = ChildApprovalId::from(format!("approval:{}", child.name));
            approvals.push(ChildApproval {
                approval_id: approval_id.clone(),
                artifact_id: stored_approval.artifact_id.clone(),
                child_name: stored_approval.child_name.clone(),
                artifact_version: stored_approval.artifact_version.clone(),
                scope_vision_id: Some(stored_approval.vision_id.clone()),
                scope_node_id: Some(stored_approval.node_id.clone()),
                scope_project_id: stored_approval.project_id.clone(),
                approval_state: stored_approval.approval_state,
                policy_revision: stored_approval.policy_revision,
                authority_observation_id: ObservationId::from(format!(
                    "obs:approval:{}",
                    child.name
                )),
            });

            let Some(stored_assignment) = self.child_assignments.get(&child.name) else {
                continue;
            };
            let assignment_id = ChildAssignmentId::from(format!("assignment:{}", child.name));
            assignments.push(ChildAssignment {
                assignment_id: assignment_id.clone(),
                approval_id,
                artifact_id: stored_assignment.artifact_id.clone(),
                child_name: stored_assignment.child_name.clone(),
                vision_id: stored_assignment.vision_id.clone(),
                node_id: Some(stored_assignment.node_id.clone()),
                project_id: stored_assignment.project_id.clone(),
                assignment_state: stored_assignment.assignment_state,
                pinned_artifact_version: stored_assignment.artifact_version.clone(),
                assignment_observation_id: ObservationId::from(format!(
                    "obs:assignment:{}",
                    child.name
                )),
            });

            instances.push(ChildInstance {
                instance_id: ChildInstanceId::from(format!("instance:{}:1", child.name)),
                assignment_id,
                artifact_id: stored_assignment.artifact_id.clone(),
                child_name: child.name.clone(),
                generation: 1,
                node_id: scope.node_id.clone(),
                instance_state: match child.instance_state {
                    crate::MctChildInstanceState::Ready
                        if stored_approval.approval_state == ChildApprovalState::Approved
                            && stored_assignment.assignment_state
                                == ChildAssignmentState::Active =>
                    {
                        ChildInstanceState::Ready
                    }
                    crate::MctChildInstanceState::Failed => ChildInstanceState::Failed,
                    _ => ChildInstanceState::Loading,
                },
                readiness_observation_id: Some(ObservationId::from(format!(
                    "obs:ready:{}",
                    child.name
                ))),
                last_lifecycle_observation_id: ObservationId::from(format!(
                    "obs:instance:{}:1",
                    child.name
                )),
            });
        }

        MctConfigChildAuthorityProjection {
            local_node_id: scope.node_id,
            vision_id: scope.vision_id,
            project_id: scope.project_id,
            policy_revision: scope.policy_revision,
            artifacts,
            approvals,
            assignments,
            instances,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctConfigChildAuthorityProjection {
    pub local_node_id: MctNodeId,
    pub vision_id: VisionId,
    pub project_id: Option<ProjectId>,
    pub policy_revision: u64,
    pub artifacts: Vec<ComponentArtifact>,
    pub approvals: Vec<ChildApproval>,
    pub assignments: Vec<ChildAssignment>,
    pub instances: Vec<ChildInstance>,
}

impl MctConfigChildAuthorityProjection {
    pub fn authorize_child_for_call(
        &self,
        child_name: &str,
        call: &MctCall,
    ) -> ChildCallAuthorityResult {
        let Some(instance) = self
            .instances
            .iter()
            .find(|instance| instance.child_name == child_name)
        else {
            let request = ChildCallAuthorityRequest {
                instance_id: ChildInstanceId::from(format!("instance:{child_name}:missing")),
                node_id: self.local_node_id.clone(),
                ids: child_authority_ids(child_name, call),
            };
            return evaluate_child_call_authority(
                call,
                &request,
                &self.artifacts,
                &self.approvals,
                &self.assignments,
                &self.instances,
            );
        };
        let request = ChildCallAuthorityRequest {
            instance_id: instance.instance_id.clone(),
            node_id: self.local_node_id.clone(),
            ids: child_authority_ids(child_name, call),
        };
        evaluate_child_call_authority(
            call,
            &request,
            &self.artifacts,
            &self.approvals,
            &self.assignments,
            &self.instances,
        )
    }
}

fn child_authority_ids(child_name: &str, call: &MctCall) -> ChildCallAuthorityIds {
    ChildCallAuthorityIds {
        evaluation_id: ChildCallEvaluationId::from(format!("eval:{}:{}", call.call_id, child_name)),
        decision_id: DecisionId::from(format!("decision:{}:{}", call.call_id, child_name)),
        observation_id: ObservationId::from(format!(
            "obs:authorize:{}:{}",
            call.call_id, child_name
        )),
        authorized_child_invocation_id: AuthorizedChildInvocationId::from(format!(
            "authorized:{}:{}",
            call.call_id, child_name
        )),
    }
}

impl MctDaemonConfigStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<MctDaemonConfig> {
        if !self.path.exists() {
            return Ok(MctDaemonConfig::default());
        }
        let text = fs::read_to_string(&self.path)
            .with_context(|| format!("read config {}", self.path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parse config {}", self.path.display()))
    }

    pub fn save(&self, config: &MctDaemonConfig) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create config dir {}", parent.display()))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(config).context("encode config")?;
        fs::write(&tmp, text).with_context(|| format!("write config temp {}", tmp.display()))?;
        fs::rename(&tmp, &self.path).with_context(|| {
            format!(
                "replace config {} with {}",
                self.path.display(),
                tmp.display()
            )
        })?;
        Ok(())
    }

    pub fn approve_and_assign_loaded_child(
        &self,
        child: &MctLoadedChild,
        scope: MctOperatorChildScope,
    ) -> Result<MctDaemonConfig> {
        let mut config = self.load()?;
        let now = unix_timestamp_string();
        config.child_approvals.insert(
            child.name.clone(),
            MctStoredChildApproval {
                child_name: child.name.clone(),
                artifact_id: ComponentArtifactId::from(child.artifact_id.clone()),
                artifact_version: child.version.clone(),
                approval_state: ChildApprovalState::Approved,
                vision_id: scope.vision_id.clone(),
                node_id: scope.node_id.clone(),
                project_id: scope.project_id.clone(),
                policy_revision: scope.policy_revision,
                updated_at: now.clone(),
            },
        );
        config.child_assignments.insert(
            child.name.clone(),
            MctStoredChildAssignment {
                child_name: child.name.clone(),
                artifact_id: ComponentArtifactId::from(child.artifact_id.clone()),
                artifact_version: child.version.clone(),
                assignment_state: ChildAssignmentState::Active,
                vision_id: scope.vision_id,
                node_id: scope.node_id,
                project_id: scope.project_id,
                policy_revision: scope.policy_revision,
                updated_at: now,
            },
        );
        self.save(&config)?;
        Ok(config)
    }

    pub fn revoke_child(&self, child_name: &str) -> Result<MctDaemonConfig> {
        let mut config = self.load()?;
        let now = unix_timestamp_string();
        if let Some(approval) = config.child_approvals.get_mut(child_name) {
            approval.approval_state = ChildApprovalState::Revoked;
            approval.updated_at = now.clone();
        }
        if let Some(assignment) = config.child_assignments.get_mut(child_name) {
            assignment.assignment_state = ChildAssignmentState::Revoked;
            assignment.updated_at = now;
        }
        self.save(&config)?;
        Ok(config)
    }

    pub fn upsert_peer(&self, entry: MctPeerAddressBookEntry) -> Result<MctDaemonConfig> {
        let mut config = self.load()?;
        config.peers.insert(entry.peer_node_id.to_string(), entry);
        self.save(&config)?;
        Ok(config)
    }

    pub fn remove_peer(&self, peer_node_id: &MctNodeId) -> Result<MctDaemonConfig> {
        let mut config = self.load()?;
        config.peers.remove(peer_node_id.as_str());
        self.save(&config)?;
        Ok(config)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctOperatorChildScope {
    pub vision_id: VisionId,
    pub node_id: MctNodeId,
    pub project_id: Option<ProjectId>,
    pub policy_revision: u64,
}

impl Default for MctOperatorChildScope {
    fn default() -> Self {
        Self {
            vision_id: VisionId::from("vision-local"),
            node_id: MctNodeId::from("local-mct"),
            project_id: None,
            policy_revision: 1,
        }
    }
}

pub fn default_config_path() -> PathBuf {
    PathBuf::from(".mct").join("config.json")
}

pub fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::children::{MctChildFileDigest, MctChildIngressMode, MctChildInstanceState};

    fn call() -> MctCall {
        MctCall {
            call_id: CallId::from("call-a"),
            caller: CallerIdentity {
                node_id: MctNodeId::from("local-mct"),
                user_id: None,
                vision_id: VisionId::from("vision-local"),
                project_id: None,
            },
            target: OperationTarget {
                namespace: "patina:echo".into(),
                interface_name: "control@0.1.0".into(),
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

    fn loaded_child() -> MctLoadedChild {
        MctLoadedChild {
            child_id: ChildId::from("child-a"),
            name: "child-a".into(),
            version: "0.1.0".into(),
            description: None,
            kind: "child".into(),
            role: None,
            wasm_path: PathBuf::from("child-a.wasm"),
            manifest_path: PathBuf::from("child-a.toml"),
            wasm_digest: MctChildFileDigest {
                sha256: "wasm".into(),
                sidecar_present: true,
                verified: true,
            },
            manifest_digest: MctChildFileDigest {
                sha256: "manifest".into(),
                sidecar_present: true,
                verified: true,
            },
            artifact_id: "sha256:wasm".into(),
            ingress_mode: MctChildIngressMode::WitOnly,
            allowed_operations: vec!["patina:echo/control@0.1.0.echo".into()],
            requested_toys: Vec::new(),
            subscribed_streams: Vec::new(),
            relationship_listens: Vec::new(),
            wasm_size_bytes: 4,
            instance_state: MctChildInstanceState::Ready,
        }
    }

    #[test]
    fn config_store_persists_child_approval_assignment_and_revocation() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctDaemonConfigStore::new(dir.path().join("config.json"));

        let config = store
            .approve_and_assign_loaded_child(&loaded_child(), MctOperatorChildScope::default())
            .unwrap();
        assert_eq!(
            config.child_approvals["child-a"].approval_state,
            ChildApprovalState::Approved
        );
        assert_eq!(
            config.child_assignments["child-a"].assignment_state,
            ChildAssignmentState::Active
        );

        let reloaded = store.load().unwrap();
        assert_eq!(reloaded, config);

        let revoked = store.revoke_child("child-a").unwrap();
        assert_eq!(
            revoked.child_approvals["child-a"].approval_state,
            ChildApprovalState::Revoked
        );
        assert_eq!(
            revoked.child_assignments["child-a"].assignment_state,
            ChildAssignmentState::Revoked
        );
    }

    #[test]
    fn config_projection_authorizes_only_stored_approved_assignments() {
        let mut config = MctDaemonConfig::default();
        let child = loaded_child();
        config.child_approvals.insert(
            child.name.clone(),
            MctStoredChildApproval {
                child_name: child.name.clone(),
                artifact_id: ComponentArtifactId::from(child.artifact_id.clone()),
                artifact_version: child.version.clone(),
                approval_state: ChildApprovalState::Approved,
                vision_id: VisionId::from("vision-local"),
                node_id: MctNodeId::from("local-mct"),
                project_id: None,
                policy_revision: 1,
                updated_at: "1".into(),
            },
        );
        config.child_assignments.insert(
            child.name.clone(),
            MctStoredChildAssignment {
                child_name: child.name.clone(),
                artifact_id: ComponentArtifactId::from(child.artifact_id.clone()),
                artifact_version: child.version.clone(),
                assignment_state: ChildAssignmentState::Active,
                vision_id: VisionId::from("vision-local"),
                node_id: MctNodeId::from("local-mct"),
                project_id: None,
                policy_revision: 1,
                updated_at: "1".into(),
            },
        );

        let projection = config
            .authority_projection_for_loaded_children([&child], MctOperatorChildScope::default());
        let evaluation = projection.authorize_child_for_call("child-a", &call());
        assert!(evaluation.is_allowed());

        let missing = MctDaemonConfig::default()
            .authority_projection_for_loaded_children([&child], MctOperatorChildScope::default());
        let denied = missing.authorize_child_for_call("child-a", &call());
        assert!(!denied.is_allowed());
    }

    #[test]
    fn config_store_persists_peer_address_book_entry() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctDaemonConfigStore::new(dir.path().join("config.json"));
        let entry = MctPeerAddressBookEntry {
            peer_node_id: MctNodeId::from("peer-a"),
            binding_id: PeerBindingId::from("binding-peer-a"),
            endpoint_id: EndpointIdText::from("endpoint-a"),
            vision_id: VisionId::from("vision-a"),
            ticket: Some(MotherIrohEndpointTicket {
                endpoint_id: EndpointIdText::from("endpoint-a"),
                direct_addresses: vec!["127.0.0.1:12345".into()],
                relay_urls: Vec::new(),
            }),
            binding_state: BindingState::Admitted,
            policy_revision: 1,
            updated_at: "1".into(),
        };

        store.upsert_peer(entry.clone()).unwrap();
        let reloaded = store.load().unwrap();
        assert_eq!(reloaded.peers["peer-a"], entry);

        store.remove_peer(&MctNodeId::from("peer-a")).unwrap();
        assert!(store.load().unwrap().peers.is_empty());
    }
}

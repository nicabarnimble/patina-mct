use crate::children::{MctLoadedChild, component_artifact_from_loaded_child};
use anyhow::{Context, Result, bail};
use mct_iroh::{
    MotherIrohEndpointTicket, endpoint_id_for_secret_key_hex, load_or_create_node_secret_key_hex,
    sign_peer_binding_signature_ref,
};
use mct_kernel::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctDaemonConfig {
    #[serde(default)]
    pub local_identity: Option<MctLocalNodeIdentity>,
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
pub struct MctOutboundPeerBindingPresentation {
    pub binding_id: PeerBindingId,
    pub policy_revision: u64,
    pub signature_ref: String,
    pub expires_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPeerAddressBookEntry {
    pub peer_node_id: MctNodeId,
    pub binding_id: PeerBindingId,
    pub endpoint_id: EndpointIdText,
    pub vision_id: VisionId,
    pub ticket: Option<MotherIrohEndpointTicket>,
    #[serde(default)]
    pub binding_signature_ref: Option<String>,
    #[serde(default)]
    pub outbound_binding: Option<MctOutboundPeerBindingPresentation>,
    pub binding_state: BindingState,
    pub policy_revision: u64,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctLocalNodeIdentity {
    pub node_id: MctNodeId,
    pub vision_id: VisionId,
    pub endpoint_id: EndpointIdText,
    pub identity_path: PathBuf,
    pub policy_revision: u64,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctPeerAuthorityProjection {
    pub issuer_node_id: MctNodeId,
    pub local_endpoint_id: EndpointIdText,
    pub policy_revision: u64,
    pub bindings: Vec<MctPeerBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctDaemonConfigStore {
    path: PathBuf,
}

impl MctDaemonConfig {
    pub fn peer_authority_projection(&self) -> Result<MctPeerAuthorityProjection> {
        let Some(identity) = self.local_identity.as_ref() else {
            bail!("local MCT node identity is not configured");
        };
        Ok(MctPeerAuthorityProjection {
            issuer_node_id: identity.node_id.clone(),
            local_endpoint_id: identity.endpoint_id.clone(),
            policy_revision: identity.policy_revision,
            bindings: self
                .peers
                .values()
                .map(|peer| peer.to_peer_binding(identity))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

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
            let approval_id = ChildApprovalId::new(format!("approval:{}", child.name))
                .expect("string ID literal/generated value must be non-empty");
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
                authority_observation_id: ObservationId::new(format!(
                    "obs:approval:{}",
                    child.name
                ))
                .expect("string ID literal/generated value must be non-empty"),
            });

            let Some(stored_assignment) = self.child_assignments.get(&child.name) else {
                continue;
            };
            let assignment_id = ChildAssignmentId::new(format!("assignment:{}", child.name))
                .expect("string ID literal/generated value must be non-empty");
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
                assignment_observation_id: ObservationId::new(format!(
                    "obs:assignment:{}",
                    child.name
                ))
                .expect("string ID literal/generated value must be non-empty"),
            });

            instances.push(ChildInstance {
                instance_id: ChildInstanceId::new(format!("instance:{}:1", child.name))
                    .expect("string ID literal/generated value must be non-empty"),
                assignment_id,
                artifact_id: stored_assignment.artifact_id.clone(),
                child_name: child.name.clone(),
                generation: 1,
                node_id: scope.node_id.clone(),
                instance_state: match child.instance_state {
                    crate::MctChildInstanceState::Ready => ChildInstanceState::Ready,
                    crate::MctChildInstanceState::Failed => ChildInstanceState::Failed,
                    _ => ChildInstanceState::Loading,
                },
                readiness_observation_id: Some(
                    ObservationId::new(format!("obs:ready:{}", child.name))
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                last_lifecycle_observation_id: ObservationId::new(format!(
                    "obs:instance:{}:1",
                    child.name
                ))
                .expect("string ID literal/generated value must be non-empty"),
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

pub fn outbound_peer_binding_for_local(
    local_identity: &MctLocalNodeIdentity,
    peer: &MctPeerAddressBookEntry,
    outbound: &MctOutboundPeerBindingPresentation,
) -> Result<MctPeerBinding> {
    Ok(MctPeerBinding {
        binding_id: outbound.binding_id.clone(),
        iroh_endpoint_id: local_identity.endpoint_id.clone(),
        scope: MctPeerBindingScope {
            mct_node_id: local_identity.node_id.clone(),
            vision_id: peer.vision_id.clone(),
            allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            data_scope: None,
            observation_scope: None,
        },
        issuer_node_id: peer.peer_node_id.clone(),
        policy_revision: outbound.policy_revision,
        binding_state: BindingState::Admitted,
        issued_at: Timestamp::new(peer.updated_at.clone())?,
        expires_at: outbound.expires_at.clone(),
        created_by_observation_id: ObservationId::new(format!(
            "obs:peer-outbound-binding:{}",
            outbound.binding_id
        ))
        .expect("string ID literal/generated value must be non-empty"),
        superseded_by_observation_id: None,
    })
}

impl MctPeerAddressBookEntry {
    pub fn to_peer_binding(&self, local_identity: &MctLocalNodeIdentity) -> Result<MctPeerBinding> {
        Ok(MctPeerBinding {
            binding_id: self.binding_id.clone(),
            iroh_endpoint_id: self.endpoint_id.clone(),
            scope: MctPeerBindingScope {
                mct_node_id: self.peer_node_id.clone(),
                vision_id: self.vision_id.clone(),
                allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
                data_scope: None,
                observation_scope: None,
            },
            issuer_node_id: local_identity.node_id.clone(),
            policy_revision: self.policy_revision,
            binding_state: self.binding_state,
            issued_at: Timestamp::new(self.updated_at.clone())?,
            expires_at: None,
            created_by_observation_id: ObservationId::new(format!(
                "obs:peer-binding:{}",
                self.binding_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            superseded_by_observation_id: match self.binding_state {
                BindingState::Revoked | BindingState::Expired => Some(
                    ObservationId::new(format!(
                        "obs:peer-binding:{}:{:?}",
                        self.binding_id, self.binding_state
                    ))
                    .expect("string ID literal/generated value must be non-empty"),
                ),
                BindingState::Pending | BindingState::Admitted | BindingState::Denied => None,
            },
        })
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
                instance_id: ChildInstanceId::new(format!("instance:{child_name}:missing"))
                    .expect("string ID literal/generated value must be non-empty"),
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
        evaluation_id: ChildCallEvaluationId::new(format!("eval:{}:{}", call.call_id, child_name))
            .expect("string ID literal/generated value must be non-empty"),
        decision_id: DecisionId::new(format!("decision:{}:{}", call.call_id, child_name))
            .expect("string ID literal/generated value must be non-empty"),
        observation_id: ObservationId::new(format!(
            "obs:authorize:{}:{}",
            call.call_id, child_name
        ))
        .expect("string ID literal/generated value must be non-empty"),
        authorized_child_invocation_id: AuthorizedChildInvocationId::new(format!(
            "authorized:{}:{}",
            call.call_id, child_name
        ))
        .expect("string ID literal/generated value must be non-empty"),
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

    pub fn ensure_local_identity(
        &self,
        scope: MctOperatorNodeScope,
        identity_path: impl Into<PathBuf>,
    ) -> Result<MctLocalNodeIdentity> {
        let identity_path = identity_path.into();
        let secret_key_hex = load_or_create_node_secret_key_hex(&identity_path)
            .with_context(|| format!("load local Iroh identity {}", identity_path.display()))?;
        let endpoint_id = endpoint_id_for_secret_key_hex(&secret_key_hex)
            .context("derive local Iroh endpoint id")?;
        let identity = MctLocalNodeIdentity {
            node_id: scope.node_id,
            vision_id: scope.vision_id,
            endpoint_id,
            identity_path,
            policy_revision: scope.policy_revision,
            updated_at: current_timestamp_string(),
        };
        let mut config = self.load()?;
        config.local_identity = Some(identity.clone());
        self.save(&config)?;
        Ok(identity)
    }

    pub fn prepare_approved_and_assigned_child(
        &self,
        child: &MctLoadedChild,
        scope: MctOperatorChildScope,
    ) -> Result<MctDaemonConfig> {
        if !child.integrity_verified() {
            bail!(
                "child '{}' cannot be approved because its wasm and manifest hashes are not verified",
                child.name
            );
        }
        let mut config = self.load()?;
        let now = current_timestamp_string();
        config.child_approvals.insert(
            child.name.clone(),
            MctStoredChildApproval {
                child_name: child.name.clone(),
                artifact_id: ComponentArtifactId::new(child.artifact_id.clone())
                    .expect("string ID literal/generated value must be non-empty"),
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
                artifact_id: ComponentArtifactId::new(child.artifact_id.clone())
                    .expect("string ID literal/generated value must be non-empty"),
                artifact_version: child.version.clone(),
                assignment_state: ChildAssignmentState::Active,
                vision_id: scope.vision_id,
                node_id: scope.node_id,
                project_id: scope.project_id,
                policy_revision: scope.policy_revision,
                updated_at: now,
            },
        );
        Ok(config)
    }

    pub fn approve_and_assign_loaded_child(
        &self,
        child: &MctLoadedChild,
        scope: MctOperatorChildScope,
    ) -> Result<MctDaemonConfig> {
        let config = self.prepare_approved_and_assigned_child(child, scope)?;
        self.save(&config)?;
        Ok(config)
    }

    pub fn prepare_revoked_child(&self, child_name: &str) -> Result<MctDaemonConfig> {
        let mut config = self.load()?;
        if !config.child_approvals.contains_key(child_name)
            || !config.child_assignments.contains_key(child_name)
        {
            bail!("child '{child_name}' does not have approval and assignment authority");
        }
        let now = current_timestamp_string();
        if let Some(approval) = config.child_approvals.get_mut(child_name) {
            approval.approval_state = ChildApprovalState::Revoked;
            approval.updated_at = now.clone();
        }
        if let Some(assignment) = config.child_assignments.get_mut(child_name) {
            assignment.assignment_state = ChildAssignmentState::Revoked;
            assignment.updated_at = now;
        }
        Ok(config)
    }

    pub fn revoke_child(&self, child_name: &str) -> Result<MctDaemonConfig> {
        let config = self.prepare_revoked_child(child_name)?;
        self.save(&config)?;
        Ok(config)
    }

    pub fn upsert_peer(&self, mut entry: MctPeerAddressBookEntry) -> Result<MctDaemonConfig> {
        let mut config = self.load()?;
        if entry.binding_signature_ref.is_none()
            && let Some(identity) = config.local_identity.as_ref()
        {
            let binding = entry.to_peer_binding(identity)?;
            let secret_key_hex = load_or_create_node_secret_key_hex(&identity.identity_path)?;
            entry.binding_signature_ref = Some(sign_peer_binding_signature_ref(
                &secret_key_hex,
                &binding,
                &identity.endpoint_id,
            )?);
        }
        config.peers.insert(entry.peer_node_id.to_string(), entry);
        self.save(&config)?;
        Ok(config)
    }

    pub fn set_peer_outbound_proof(
        &self,
        peer_node_id: &MctNodeId,
        outbound_binding: MctOutboundPeerBindingPresentation,
    ) -> Result<MctDaemonConfig> {
        if outbound_binding.signature_ref.trim().is_empty() {
            bail!("outbound binding signature_ref must not be empty");
        }
        let mut config = self.load()?;
        let Some(peer) = config.peers.get_mut(peer_node_id.as_str()) else {
            bail!("peer '{peer_node_id}' not found in config");
        };
        peer.outbound_binding = Some(outbound_binding);
        peer.updated_at = current_timestamp_string();
        self.save(&config)?;
        Ok(config)
    }

    pub fn set_peer_state(
        &self,
        peer_node_id: &MctNodeId,
        binding_state: BindingState,
    ) -> Result<MctDaemonConfig> {
        let mut config = self.load()?;
        let Some(peer) = config.peers.get_mut(peer_node_id.as_str()) else {
            bail!("peer '{peer_node_id}' not found in config");
        };
        peer.binding_state = binding_state;
        peer.updated_at = current_timestamp_string();
        self.save(&config)?;
        Ok(config)
    }

    pub fn revoke_peer(&self, peer_node_id: &MctNodeId) -> Result<MctDaemonConfig> {
        self.set_peer_state(peer_node_id, BindingState::Revoked)
    }

    pub fn remove_peer(&self, peer_node_id: &MctNodeId) -> Result<MctDaemonConfig> {
        let mut config = self.load()?;
        config.peers.remove(peer_node_id.as_str());
        self.save(&config)?;
        Ok(config)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctOperatorNodeScope {
    pub node_id: MctNodeId,
    pub vision_id: VisionId,
    pub policy_revision: u64,
}

impl Default for MctOperatorNodeScope {
    fn default() -> Self {
        Self {
            node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            policy_revision: 1,
        }
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
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            project_id: None,
            policy_revision: 1,
        }
    }
}

pub fn default_config_path() -> PathBuf {
    PathBuf::from(".mct").join("config.json")
}

pub fn current_timestamp_string() -> String {
    jiff::Timestamp::now().to_string()
}

pub fn current_timestamp() -> Timestamp {
    Timestamp::new(current_timestamp_string()).expect("jiff produced RFC3339 timestamp")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::children::{MctChildFileDigest, MctChildIngressMode, MctChildInstanceState};

    fn call() -> MctCall {
        MctCall {
            call_id: CallId::new("call-a")
                .expect("string ID literal/generated value must be non-empty"),
            caller: CallerIdentity {
                node_id: MctNodeId::new("local-mct")
                    .expect("string ID literal/generated value must be non-empty"),
                user_id: None,
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                project_id: None,
            },
            target: OperationTarget {
                namespace: "patina:echo".into(),
                interface_name: "control@0.1.0".into(),
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

    fn loaded_child() -> MctLoadedChild {
        MctLoadedChild {
            child_id: ChildId::new("child-a")
                .expect("string ID literal/generated value must be non-empty"),
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
    fn config_store_rejects_unverified_child_approval() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctDaemonConfigStore::new(dir.path().join("config.json"));
        let mut child = loaded_child();
        child.wasm_digest.verified = false;

        let result =
            store.approve_and_assign_loaded_child(&child, MctOperatorChildScope::default());

        assert!(result.is_err());
        assert!(store.load().unwrap().child_approvals.is_empty());
    }

    #[test]
    fn config_projection_authorizes_only_stored_approved_assignments() {
        let mut config = MctDaemonConfig::default();
        let child = loaded_child();
        config.child_approvals.insert(
            child.name.clone(),
            MctStoredChildApproval {
                child_name: child.name.clone(),
                artifact_id: ComponentArtifactId::new(child.artifact_id.clone())
                    .expect("string ID literal/generated value must be non-empty"),
                artifact_version: child.version.clone(),
                approval_state: ChildApprovalState::Approved,
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                node_id: MctNodeId::new("local-mct")
                    .expect("string ID literal/generated value must be non-empty"),
                project_id: None,
                policy_revision: 1,
                updated_at: "1".into(),
            },
        );
        config.child_assignments.insert(
            child.name.clone(),
            MctStoredChildAssignment {
                child_name: child.name.clone(),
                artifact_id: ComponentArtifactId::new(child.artifact_id.clone())
                    .expect("string ID literal/generated value must be non-empty"),
                artifact_version: child.version.clone(),
                assignment_state: ChildAssignmentState::Active,
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                node_id: MctNodeId::new("local-mct")
                    .expect("string ID literal/generated value must be non-empty"),
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

    fn peer_entry(state: BindingState) -> MctPeerAddressBookEntry {
        MctPeerAddressBookEntry {
            peer_node_id: MctNodeId::new("peer-a")
                .expect("string ID literal/generated value must be non-empty"),
            binding_id: PeerBindingId::new("binding-peer-a")
                .expect("string ID literal/generated value must be non-empty"),
            endpoint_id: EndpointIdText::new("endpoint-a")
                .expect("string ID literal/generated value must be non-empty"),
            vision_id: VisionId::new("vision-a")
                .expect("string ID literal/generated value must be non-empty"),
            ticket: Some(MotherIrohEndpointTicket {
                endpoint_id: EndpointIdText::new("endpoint-a")
                    .expect("string ID literal/generated value must be non-empty"),
                direct_addresses: vec!["127.0.0.1:12345".into()],
                relay_urls: Vec::new(),
            }),
            binding_signature_ref: None,
            outbound_binding: None,
            binding_state: state,
            policy_revision: 1,
            updated_at: "2026-07-09T00:00:00Z".into(),
        }
    }

    #[test]
    fn config_store_persists_local_identity_without_storing_secret() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let identity_path = dir.path().join("identity").join("iroh-secret.hex");
        let store = MctDaemonConfigStore::new(&config_path);

        let identity = store
            .ensure_local_identity(MctOperatorNodeScope::default(), &identity_path)
            .unwrap();

        assert_eq!(
            identity.node_id,
            MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(
            identity.vision_id,
            VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(identity.identity_path, identity_path);
        assert!(identity_path.exists());
        assert_eq!(store.load().unwrap().local_identity, Some(identity));
        let secret_key_hex = fs::read_to_string(&identity_path).unwrap();
        let config_json = fs::read_to_string(&config_path).unwrap();
        assert!(!config_json.contains(secret_key_hex.trim()));
    }

    #[test]
    fn peer_authority_projection_requires_local_identity() {
        let mut config = MctDaemonConfig::default();
        config
            .peers
            .insert("peer-a".into(), peer_entry(BindingState::Admitted));

        let result = config.peer_authority_projection();

        assert!(result.is_err());
    }

    #[test]
    fn config_projects_peer_bindings_from_local_identity() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctDaemonConfigStore::new(dir.path().join("config.json"));
        let identity = store
            .ensure_local_identity(
                MctOperatorNodeScope {
                    node_id: MctNodeId::new("node-local")
                        .expect("string ID literal/generated value must be non-empty"),
                    vision_id: VisionId::new("vision-local")
                        .expect("string ID literal/generated value must be non-empty"),
                    policy_revision: 7,
                },
                dir.path().join("iroh-secret.hex"),
            )
            .unwrap();
        store
            .upsert_peer(peer_entry(BindingState::Admitted))
            .unwrap();
        store
            .revoke_peer(
                &MctNodeId::new("peer-a")
                    .expect("string ID literal/generated value must be non-empty"),
            )
            .unwrap();

        let projection = store.load().unwrap().peer_authority_projection().unwrap();

        assert_eq!(projection.issuer_node_id, identity.node_id);
        assert_eq!(projection.local_endpoint_id, identity.endpoint_id);
        assert_eq!(projection.policy_revision, 7);
        assert_eq!(projection.bindings.len(), 1);
        let binding = &projection.bindings[0];
        assert_eq!(
            binding.binding_id,
            PeerBindingId::new("binding-peer-a")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(
            binding.iroh_endpoint_id,
            EndpointIdText::new("endpoint-a")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(
            binding.scope.mct_node_id,
            MctNodeId::new("peer-a").expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(
            binding.scope.vision_id,
            VisionId::new("vision-a").expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(
            binding.issuer_node_id,
            MctNodeId::new("node-local")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(binding.binding_state, BindingState::Revoked);
        assert_eq!(
            binding.scope.allowed_alpns,
            vec![MCT_HELLO_ALPN.to_string(), MCT_CALL_ALPN.to_string()]
        );
        assert!(binding.superseded_by_observation_id.is_some());
    }

    #[test]
    fn config_store_persists_peer_address_book_entry() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctDaemonConfigStore::new(dir.path().join("config.json"));
        let entry = peer_entry(BindingState::Admitted);

        store.upsert_peer(entry.clone()).unwrap();
        let reloaded = store.load().unwrap();
        assert_eq!(reloaded.peers["peer-a"], entry);

        store
            .remove_peer(
                &MctNodeId::new("peer-a")
                    .expect("string ID literal/generated value must be non-empty"),
            )
            .unwrap();
        assert!(store.load().unwrap().peers.is_empty());
    }
}

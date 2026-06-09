use crate::children::MctLoadedChild;
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

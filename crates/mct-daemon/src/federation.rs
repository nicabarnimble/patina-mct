use crate::{MctDaemonConfig, MctRuntimeStateSummary, unix_timestamp_string};
use mct_kernel::{MctNodeId, VisionId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctFederationPeerView {
    pub peer_node_id: MctNodeId,
    pub vision_id: VisionId,
    pub binding_state: String,
    pub has_ticket: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctFederationCapabilityView {
    pub node_id: MctNodeId,
    pub vision_id: VisionId,
    pub published_at: String,
    pub artifacts: u64,
    pub approved_children: u64,
    pub ready_instances: u64,
    pub peers: Vec<MctFederationPeerView>,
    pub visibility: String,
}

pub fn build_federation_capability_view(
    config: &MctDaemonConfig,
    summary: &MctRuntimeStateSummary,
    node_id: MctNodeId,
    vision_id: VisionId,
) -> MctFederationCapabilityView {
    let peers = config
        .peers
        .values()
        .filter(|peer| peer.vision_id == vision_id)
        .map(|peer| MctFederationPeerView {
            peer_node_id: peer.peer_node_id.clone(),
            vision_id: peer.vision_id.clone(),
            binding_state: serde_json::to_value(peer.binding_state)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| "unknown".into()),
            has_ticket: peer.ticket.is_some(),
        })
        .collect();

    MctFederationCapabilityView {
        node_id,
        vision_id,
        published_at: unix_timestamp_string(),
        artifacts: summary.artifacts,
        approved_children: summary.approved_children,
        ready_instances: summary.ready_instances,
        peers,
        visibility: "vision_scoped".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MctPeerAddressBookEntry;
    use mct_kernel::{BindingState, EndpointIdText, PeerBindingId};

    #[test]
    fn federation_view_is_vision_scoped() {
        let mut config = MctDaemonConfig::default();
        config.peers.insert(
            "peer-a".into(),
            MctPeerAddressBookEntry {
                peer_node_id: MctNodeId::from("peer-a"),
                binding_id: PeerBindingId::from("binding-a"),
                endpoint_id: EndpointIdText::from("endpoint-a"),
                vision_id: VisionId::from("vision-a"),
                ticket: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                updated_at: "1".into(),
            },
        );
        config.peers.insert(
            "peer-b".into(),
            MctPeerAddressBookEntry {
                peer_node_id: MctNodeId::from("peer-b"),
                binding_id: PeerBindingId::from("binding-b"),
                endpoint_id: EndpointIdText::from("endpoint-b"),
                vision_id: VisionId::from("vision-b"),
                ticket: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                updated_at: "1".into(),
            },
        );
        let view = build_federation_capability_view(
            &config,
            &MctRuntimeStateSummary {
                schema_version: 1,
                artifacts: 2,
                approved_children: 1,
                active_assignments: 1,
                ready_instances: 1,
                peers: 2,
                runs: 0,
                completed_runs: 0,
                failed_runs: 0,
                metric_points: 0,
            },
            MctNodeId::from("node-a"),
            VisionId::from("vision-a"),
        );
        assert_eq!(view.peers.len(), 1);
        assert_eq!(view.peers[0].peer_node_id, MctNodeId::from("peer-a"));
    }
}

use crate::{
    MctChildIngressMode, MctChildInstanceState, MctDaemonConfig, MctLoadedChild,
    MctRuntimeStateSummary, current_timestamp_string,
};
use mct_kernel::{
    ChildApprovalState, ChildAssignmentState, MctHelloCallableSurface, MctHelloCapabilityView,
    MctNodeId, RuntimeKind, Timestamp, VisionId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctFederationPeerView {
    pub peer_node_id: MctNodeId,
    pub vision_id: VisionId,
    pub binding_state: String,
    pub has_ticket: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctFederationCallableSurfaceView {
    pub child_name: String,
    pub operation_id: String,
    pub runtime_kind: RuntimeKind,
    pub vision_id: VisionId,
    pub policy_revision: u64,
    pub visibility: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctFederationCapabilityView {
    pub node_id: MctNodeId,
    pub vision_id: VisionId,
    pub published_at: String,
    pub artifacts: u64,
    pub approved_children: u64,
    pub ready_instances: u64,
    pub policy_revision: u64,
    pub callable_surfaces: Vec<MctFederationCallableSurfaceView>,
    pub peers: Vec<MctFederationPeerView>,
    pub visibility: String,
}

pub fn build_federation_capability_view(
    config: &MctDaemonConfig,
    summary: &MctRuntimeStateSummary,
    node_id: MctNodeId,
    vision_id: VisionId,
) -> MctFederationCapabilityView {
    build_federation_capability_view_with_children(
        config,
        summary,
        node_id,
        vision_id,
        std::iter::empty::<&MctLoadedChild>(),
    )
}

pub fn build_federation_capability_view_with_children<'a>(
    config: &MctDaemonConfig,
    summary: &MctRuntimeStateSummary,
    node_id: MctNodeId,
    vision_id: VisionId,
    children: impl IntoIterator<Item = &'a MctLoadedChild>,
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

    let callable_surfaces = federation_callable_surfaces(config, &node_id, &vision_id, children);

    let policy_revision = config
        .local_identity
        .as_ref()
        .filter(|identity| identity.node_id == node_id && identity.vision_id == vision_id)
        .map(|identity| identity.policy_revision)
        .unwrap_or_else(|| {
            callable_surfaces
                .iter()
                .map(|surface| surface.policy_revision)
                .max()
                .unwrap_or(1)
        });

    MctFederationCapabilityView {
        node_id,
        vision_id,
        published_at: current_timestamp_string(),
        artifacts: summary.artifacts,
        approved_children: summary.approved_children,
        ready_instances: summary.ready_instances,
        policy_revision,
        callable_surfaces,
        peers,
        visibility: "vision_scoped".into(),
    }
}

pub fn hello_capability_view_from_federation_view(
    view: &MctFederationCapabilityView,
) -> MctHelloCapabilityView {
    let supported_wit_worlds = view
        .callable_surfaces
        .iter()
        .filter_map(|surface| {
            surface
                .operation_id
                .rsplit_once('.')
                .map(|(world, _)| world)
        })
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    MctHelloCapabilityView {
        node_id: view.node_id.clone(),
        vision_id: view.vision_id.clone(),
        published_at: Timestamp::new(view.published_at.clone())
            .expect("federation published_at is generated as RFC3339"),
        policy_revision: view.policy_revision,
        supported_alpns: vec![
            mct_kernel::MCT_HELLO_ALPN.into(),
            mct_kernel::MCT_CALL_ALPN.into(),
        ],
        supported_wit_worlds,
        supported_observation_modes: vec!["local-ledger".into()],
        callable_surfaces: view
            .callable_surfaces
            .iter()
            .map(|surface| MctHelloCallableSurface {
                child_name: surface.child_name.clone(),
                operation_id: surface.operation_id.clone(),
                runtime_kind: surface.runtime_kind,
                vision_id: surface.vision_id.clone(),
                policy_revision: surface.policy_revision,
                visibility: surface.visibility.clone(),
            })
            .collect(),
        capability_view_ref: None,
    }
}

fn federation_callable_surfaces<'a>(
    config: &MctDaemonConfig,
    node_id: &MctNodeId,
    vision_id: &VisionId,
    children: impl IntoIterator<Item = &'a MctLoadedChild>,
) -> Vec<MctFederationCallableSurfaceView> {
    let mut surfaces = Vec::new();
    for child in children {
        if child.instance_state != MctChildInstanceState::Ready {
            continue;
        }
        let Some(approval) = config.child_approvals.get(&child.name) else {
            continue;
        };
        let Some(assignment) = config.child_assignments.get(&child.name) else {
            continue;
        };
        if approval.approval_state != ChildApprovalState::Approved
            || assignment.assignment_state != ChildAssignmentState::Active
            || &approval.vision_id != vision_id
            || &assignment.vision_id != vision_id
            || &assignment.node_id != node_id
        {
            continue;
        }
        let runtime_kind = match child.ingress_mode {
            MctChildIngressMode::Handle => RuntimeKind::Process,
            MctChildIngressMode::Hybrid | MctChildIngressMode::WitOnly => {
                RuntimeKind::WasmComponent
            }
        };
        for operation_id in &child.allowed_operations {
            surfaces.push(MctFederationCallableSurfaceView {
                child_name: child.name.clone(),
                operation_id: operation_id.clone(),
                runtime_kind,
                vision_id: vision_id.clone(),
                policy_revision: approval.policy_revision.max(assignment.policy_revision),
                visibility: "vision_scoped".into(),
            });
        }
    }
    surfaces
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MctChildFileDigest, MctPeerAddressBookEntry, MctStoredChildApproval,
        MctStoredChildAssignment,
    };
    use mct_kernel::{BindingState, ChildId, ComponentArtifactId, EndpointIdText, PeerBindingId};
    use std::path::PathBuf;

    #[test]
    fn federation_view_is_vision_scoped() {
        let mut config = MctDaemonConfig::default();
        config.peers.insert(
            "peer-a".into(),
            MctPeerAddressBookEntry {
                peer_node_id: MctNodeId::new("peer-a")
                    .expect("string ID literal/generated value must be non-empty"),
                binding_id: PeerBindingId::new("binding-a")
                    .expect("string ID literal/generated value must be non-empty"),
                endpoint_id: EndpointIdText::new("endpoint-a")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                ticket: None,
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                expires_at: Timestamp::new("2026-07-09T00:05:00Z").unwrap(),
                updated_at: "1".into(),
            },
        );
        config.peers.insert(
            "peer-b".into(),
            MctPeerAddressBookEntry {
                peer_node_id: MctNodeId::new("peer-b")
                    .expect("string ID literal/generated value must be non-empty"),
                binding_id: PeerBindingId::new("binding-b")
                    .expect("string ID literal/generated value must be non-empty"),
                endpoint_id: EndpointIdText::new("endpoint-b")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-b")
                    .expect("string ID literal/generated value must be non-empty"),
                ticket: None,
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                expires_at: Timestamp::new("2026-07-09T00:05:00Z").unwrap(),
                updated_at: "1".into(),
            },
        );
        let view = build_federation_capability_view(
            &config,
            &MctRuntimeStateSummary {
                schema_version: 1,
                artifacts: 2,
                daemon_release_artifacts: 0,
                approved_children: 1,
                active_assignments: 1,
                ready_instances: 1,
                peers: 2,
                runs: 0,
                completed_runs: 0,
                failed_runs: 0,
                metric_points: 0,
                queued_tasks: 0,
                child_state_keys: 0,
                child_subscriptions: 0,
                toy_catalog_contracts: 0,
                toy_grant_snapshots: 0,
                trigger_records: 0,
                current_trigger_records: 0,
                pending_trigger_occurrences: 0,
                active_trigger_firings: 0,
                watch_scope_records: 0,
                current_watch_scopes: 0,
                watch_event_batches: 0,
                watch_events: 0,
                watch_event_deliveries: 0,
            },
            MctNodeId::new("node-a").expect("string ID literal/generated value must be non-empty"),
            VisionId::new("vision-a").expect("string ID literal/generated value must be non-empty"),
        );
        assert_eq!(view.peers.len(), 1);
        assert_eq!(
            view.peers[0].peer_node_id,
            MctNodeId::new("peer-a").expect("string ID literal/generated value must be non-empty")
        );
        assert!(view.callable_surfaces.is_empty());
    }

    /// Covers `CapabilityPublicationRelationship.HonestLocalExecutionOffer`.
    #[test]
    fn honest_local_execution_offer_excludes_approved_assigned_non_ready_child() {
        let mut config = MctDaemonConfig::default();
        config.child_approvals.insert(
            "resident-wit".into(),
            MctStoredChildApproval {
                child_name: "resident-wit".into(),
                artifact_id: ComponentArtifactId::new("artifact-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                artifact_version: "0.1.0".into(),
                approval_state: ChildApprovalState::Approved,
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                node_id: MctNodeId::new("node-a")
                    .expect("string ID literal/generated value must be non-empty"),
                project_id: None,
                policy_revision: 3,
                updated_at: "2026-07-09T00:00:00Z".into(),
            },
        );
        config.child_assignments.insert(
            "resident-wit".into(),
            MctStoredChildAssignment {
                child_name: "resident-wit".into(),
                artifact_id: ComponentArtifactId::new("artifact-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                artifact_version: "0.1.0".into(),
                assignment_state: ChildAssignmentState::Active,
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                node_id: MctNodeId::new("node-a")
                    .expect("string ID literal/generated value must be non-empty"),
                project_id: None,
                policy_revision: 4,
                updated_at: "2026-07-09T00:00:00Z".into(),
            },
        );
        let mut loading_approval = config.child_approvals["resident-wit"].clone();
        loading_approval.child_name = "resident-loading".into();
        loading_approval.artifact_id = ComponentArtifactId::new("artifact-resident-loading")
            .expect("string ID literal/generated value must be non-empty");
        config
            .child_approvals
            .insert("resident-loading".into(), loading_approval);
        let mut loading_assignment = config.child_assignments["resident-wit"].clone();
        loading_assignment.child_name = "resident-loading".into();
        loading_assignment.artifact_id = ComponentArtifactId::new("artifact-resident-loading")
            .expect("string ID literal/generated value must be non-empty");
        config
            .child_assignments
            .insert("resident-loading".into(), loading_assignment);

        let child = loaded_child(
            "resident-wit",
            MctChildInstanceState::Ready,
            vec!["patina:demo/control@0.1.0.run".into()],
        );
        let loading_child = loaded_child(
            "resident-loading",
            MctChildInstanceState::Loading,
            vec!["patina:demo/control@0.1.0.loading".into()],
        );

        let view = build_federation_capability_view_with_children(
            &config,
            &summary(),
            MctNodeId::new("node-a").expect("string ID literal/generated value must be non-empty"),
            VisionId::new("vision-a").expect("string ID literal/generated value must be non-empty"),
            [&child, &loading_child],
        );

        assert_eq!(view.callable_surfaces.len(), 1);
        assert_eq!(
            view.callable_surfaces[0].operation_id,
            "patina:demo/control@0.1.0.run"
        );
        assert_eq!(
            view.callable_surfaces[0].runtime_kind,
            RuntimeKind::WasmComponent
        );
        assert_eq!(view.callable_surfaces[0].policy_revision, 4);
    }

    fn summary() -> MctRuntimeStateSummary {
        MctRuntimeStateSummary {
            schema_version: 1,
            artifacts: 2,
            daemon_release_artifacts: 0,
            approved_children: 1,
            active_assignments: 1,
            ready_instances: 1,
            peers: 0,
            runs: 0,
            completed_runs: 0,
            failed_runs: 0,
            metric_points: 0,
            queued_tasks: 0,
            child_state_keys: 0,
            child_subscriptions: 0,
            toy_catalog_contracts: 0,
            toy_grant_snapshots: 0,
            trigger_records: 0,
            current_trigger_records: 0,
            pending_trigger_occurrences: 0,
            active_trigger_firings: 0,
            watch_scope_records: 0,
            current_watch_scopes: 0,
            watch_event_batches: 0,
            watch_events: 0,
            watch_event_deliveries: 0,
        }
    }

    fn loaded_child(
        name: &str,
        instance_state: MctChildInstanceState,
        allowed_operations: Vec<String>,
    ) -> MctLoadedChild {
        MctLoadedChild {
            child_id: ChildId::new(name)
                .expect("string ID literal/generated value must be non-empty"),
            name: name.into(),
            version: "0.1.0".into(),
            description: None,
            kind: "child".into(),
            role: Some("app".into()),
            wasm_path: PathBuf::from(format!("{name}.wasm")),
            manifest_path: PathBuf::from(format!("{name}/child.toml")),
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
            artifact_id: format!("artifact-{name}"),
            ingress_mode: MctChildIngressMode::WitOnly,
            allowed_operations,
            requested_toys: Vec::new(),
            subscribed_streams: Vec::new(),
            relationship_listens: Vec::new(),
            wasm_size_bytes: 42,
            instance_state,
        }
    }
}

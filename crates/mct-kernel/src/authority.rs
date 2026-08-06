//! Provenance-separated authority values assembled only after daemon-side projection proof.

#![allow(missing_docs)]

use crate::{
    BindingState, CanonicalToyContract, ChildApproval, ChildAssignment, ChildInstance,
    ComponentArtifact, EndpointIdText, MctNodeId, MctPeerBinding, NetworkPathClass, PeerBindingId,
    RuntimeKind, Timestamp, ToyGrant, VisionId, WatchObservationScope,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalChildPolicyProvenanceV1 {
    LegacyConfigAndLoadedChildProjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalPeerPolicyProvenanceV1 {
    LegacyConfigAndRuntimePeerProjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalMotherClockProvenanceV1 {
    ExecutingMotherClock,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalGrantsAuthorityIdentityV1 {
    mother_node_id: String,
    authority_epoch: String,
    generation: u64,
    source_authority_observation_id: String,
}

impl LocalGrantsAuthorityIdentityV1 {
    pub fn mother_node_id(&self) -> &str {
        &self.mother_node_id
    }

    pub fn authority_epoch(&self) -> &str {
        &self.authority_epoch
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn source_authority_observation_id(&self) -> &str {
        &self.source_authority_observation_id
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalExecutionAuthorityTokenV1 {
    grants_authority: LocalGrantsAuthorityIdentityV1,
    policy_revision: u64,
    vision_policy_revision: u64,
    effective_deadline: Timestamp,
}

impl LocalExecutionAuthorityTokenV1 {
    pub fn grants_authority(&self) -> &LocalGrantsAuthorityIdentityV1 {
        &self.grants_authority
    }

    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }

    pub fn vision_policy_revision(&self) -> u64 {
        self.vision_policy_revision
    }

    pub fn effective_deadline(&self) -> &Timestamp {
        &self.effective_deadline
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalCanonicalGrantsSnapshotV1 {
    grants_authority: LocalGrantsAuthorityIdentityV1,
    toy_catalog: Vec<CanonicalToyContract>,
    toy_grants: Vec<ToyGrant>,
    watch_scopes: Vec<WatchObservationScope>,
}

impl LocalCanonicalGrantsSnapshotV1 {
    pub fn grants_authority(&self) -> &LocalGrantsAuthorityIdentityV1 {
        &self.grants_authority
    }

    pub fn toy_catalog(&self) -> &[CanonicalToyContract] {
        &self.toy_catalog
    }

    pub fn toy_grants(&self) -> &[ToyGrant] {
        &self.toy_grants
    }

    pub fn watch_scopes(&self) -> &[WatchObservationScope] {
        &self.watch_scopes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalChildPolicySnapshotV1 {
    provenance: LocalChildPolicyProvenanceV1,
    local_node_id: MctNodeId,
    vision_id: VisionId,
    policy_revision: u64,
    artifacts: Vec<ComponentArtifact>,
    approvals: Vec<ChildApproval>,
    assignments: Vec<ChildAssignment>,
    instances: Vec<ChildInstance>,
}

impl LocalChildPolicySnapshotV1 {
    pub fn provenance(&self) -> LocalChildPolicyProvenanceV1 {
        self.provenance
    }

    pub fn local_node_id(&self) -> &MctNodeId {
        &self.local_node_id
    }

    pub fn vision_id(&self) -> &VisionId {
        &self.vision_id
    }

    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }

    pub fn artifacts(&self) -> &[ComponentArtifact] {
        &self.artifacts
    }

    pub fn approvals(&self) -> &[ChildApproval] {
        &self.approvals
    }

    pub fn assignments(&self) -> &[ChildAssignment] {
        &self.assignments
    }

    pub fn instances(&self) -> &[ChildInstance] {
        &self.instances
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalPeerAuthorityRecordV1 {
    peer_node_id: MctNodeId,
    binding_id: PeerBindingId,
    endpoint_id: EndpointIdText,
    vision_id: VisionId,
    binding_state: BindingState,
    policy_revision: u64,
    expires_at: Timestamp,
    local_binding: MctPeerBinding,
    binding_signature_ref: Option<String>,
    outbound_binding: Option<MctPeerBinding>,
    outbound_signature_ref: Option<String>,
    ticket_available: bool,
    network_path: NetworkPathClass,
}

impl LocalPeerAuthorityRecordV1 {
    pub fn peer_node_id(&self) -> &MctNodeId {
        &self.peer_node_id
    }

    pub fn binding_id(&self) -> &PeerBindingId {
        &self.binding_id
    }

    pub fn endpoint_id(&self) -> &EndpointIdText {
        &self.endpoint_id
    }

    pub fn vision_id(&self) -> &VisionId {
        &self.vision_id
    }

    pub fn binding_state(&self) -> BindingState {
        self.binding_state
    }

    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }

    pub fn expires_at(&self) -> &Timestamp {
        &self.expires_at
    }

    pub fn local_binding(&self) -> &MctPeerBinding {
        &self.local_binding
    }

    pub fn binding_signature_ref(&self) -> Option<&str> {
        self.binding_signature_ref.as_deref()
    }

    pub fn outbound_binding(&self) -> Option<&MctPeerBinding> {
        self.outbound_binding.as_ref()
    }

    pub fn outbound_signature_ref(&self) -> Option<&str> {
        self.outbound_signature_ref.as_deref()
    }

    pub fn ticket_available(&self) -> bool {
        self.ticket_available
    }

    pub fn network_path(&self) -> NetworkPathClass {
        self.network_path
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalRemoteCallableSurfaceV1 {
    peer_node_id: MctNodeId,
    binding_id: PeerBindingId,
    endpoint_id: EndpointIdText,
    vision_id: VisionId,
    publisher_policy_revision: u64,
    child_name: String,
    operation_id: String,
    runtime_kind: RuntimeKind,
    surface_policy_revision: u64,
    visibility: String,
    received_at: Timestamp,
    stale_at: Timestamp,
}

impl LocalRemoteCallableSurfaceV1 {
    pub fn peer_node_id(&self) -> &MctNodeId {
        &self.peer_node_id
    }

    pub fn binding_id(&self) -> &PeerBindingId {
        &self.binding_id
    }

    pub fn endpoint_id(&self) -> &EndpointIdText {
        &self.endpoint_id
    }

    pub fn vision_id(&self) -> &VisionId {
        &self.vision_id
    }

    pub fn publisher_policy_revision(&self) -> u64 {
        self.publisher_policy_revision
    }

    pub fn child_name(&self) -> &str {
        &self.child_name
    }

    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub fn runtime_kind(&self) -> RuntimeKind {
        self.runtime_kind
    }

    pub fn surface_policy_revision(&self) -> u64 {
        self.surface_policy_revision
    }

    pub fn visibility(&self) -> &str {
        &self.visibility
    }

    pub fn received_at(&self) -> &Timestamp {
        &self.received_at
    }

    pub fn stale_at(&self) -> &Timestamp {
        &self.stale_at
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalPeerPolicySnapshotV1 {
    provenance: LocalPeerPolicyProvenanceV1,
    local_node_id: MctNodeId,
    local_vision_id: VisionId,
    local_endpoint_id: EndpointIdText,
    policy_revision: u64,
    peers: Vec<LocalPeerAuthorityRecordV1>,
    callable_surfaces: Vec<LocalRemoteCallableSurfaceV1>,
}

impl LocalPeerPolicySnapshotV1 {
    pub fn provenance(&self) -> LocalPeerPolicyProvenanceV1 {
        self.provenance
    }

    pub fn local_node_id(&self) -> &MctNodeId {
        &self.local_node_id
    }

    pub fn local_vision_id(&self) -> &VisionId {
        &self.local_vision_id
    }

    pub fn local_endpoint_id(&self) -> &EndpointIdText {
        &self.local_endpoint_id
    }

    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }

    pub fn peers(&self) -> &[LocalPeerAuthorityRecordV1] {
        &self.peers
    }

    pub fn callable_surfaces(&self) -> &[LocalRemoteCallableSurfaceV1] {
        &self.callable_surfaces
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalMotherClockSnapshotV1 {
    evaluated_at: Timestamp,
    provenance: LocalMotherClockProvenanceV1,
}

impl LocalMotherClockSnapshotV1 {
    pub fn evaluated_at(&self) -> &Timestamp {
        &self.evaluated_at
    }

    pub fn provenance(&self) -> LocalMotherClockProvenanceV1 {
        self.provenance
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalAuthorityProjectionProvenanceV1 {
    projection_id: String,
    source_mother_node_id: String,
    source_ledger_id: String,
    through_sequence: u64,
    through_observation_id: String,
    through_entry_hash: String,
    authority_state_hash: String,
    projection_hash: String,
}

impl LocalAuthorityProjectionProvenanceV1 {
    pub fn projection_id(&self) -> &str {
        &self.projection_id
    }

    pub fn source_mother_node_id(&self) -> &str {
        &self.source_mother_node_id
    }

    pub fn source_ledger_id(&self) -> &str {
        &self.source_ledger_id
    }

    pub fn through_sequence(&self) -> u64 {
        self.through_sequence
    }

    pub fn through_observation_id(&self) -> &str {
        &self.through_observation_id
    }

    pub fn through_entry_hash(&self) -> &str {
        &self.through_entry_hash
    }

    pub fn authority_state_hash(&self) -> &str {
        &self.authority_state_hash
    }

    pub fn projection_hash(&self) -> &str {
        &self.projection_hash
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalExecutionAuthoritySnapshot {
    executing_mother_node_id: String,
    canonical_grants: LocalCanonicalGrantsSnapshotV1,
    policy_revision: u64,
    vision_policy_revision: u64,
    child_policy: LocalChildPolicySnapshotV1,
    peer_policy: LocalPeerPolicySnapshotV1,
    mother_clock: LocalMotherClockSnapshotV1,
    projection: LocalAuthorityProjectionProvenanceV1,
}

impl LocalExecutionAuthoritySnapshot {
    pub fn executing_mother_node_id(&self) -> &str {
        &self.executing_mother_node_id
    }

    pub fn canonical_grants(&self) -> &LocalCanonicalGrantsSnapshotV1 {
        &self.canonical_grants
    }

    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }

    pub fn vision_policy_revision(&self) -> u64 {
        self.vision_policy_revision
    }

    pub fn child_policy(&self) -> &LocalChildPolicySnapshotV1 {
        &self.child_policy
    }

    pub fn peer_policy(&self) -> &LocalPeerPolicySnapshotV1 {
        &self.peer_policy
    }

    pub fn mother_clock(&self) -> &LocalMotherClockSnapshotV1 {
        &self.mother_clock
    }

    pub fn projection(&self) -> &LocalAuthorityProjectionProvenanceV1 {
        &self.projection
    }

    pub(crate) fn execution_authority(
        &self,
        effective_deadline: Timestamp,
    ) -> LocalExecutionAuthorityTokenV1 {
        LocalExecutionAuthorityTokenV1 {
            grants_authority: self.canonical_grants.grants_authority.clone(),
            policy_revision: self.policy_revision,
            vision_policy_revision: self.vision_policy_revision,
            effective_deadline,
        }
    }
}

#[doc(hidden)]
pub struct LocalExecutionAuthoritySnapshotPartsV1 {
    pub executing_mother_node_id: String,
    pub grants_authority_mother_node_id: String,
    pub grants_authority_epoch: String,
    pub grants_authority_generation: u64,
    pub grants_authority_observation_id: String,
    pub toy_catalog: Vec<CanonicalToyContract>,
    pub toy_grants: Vec<ToyGrant>,
    pub watch_scopes: Vec<WatchObservationScope>,
    pub policy_revision: u64,
    pub vision_policy_revision: u64,
    pub child_local_node_id: MctNodeId,
    pub child_vision_id: VisionId,
    pub child_artifacts: Vec<ComponentArtifact>,
    pub child_approvals: Vec<ChildApproval>,
    pub child_assignments: Vec<ChildAssignment>,
    pub child_instances: Vec<ChildInstance>,
    pub peer_local_node_id: MctNodeId,
    pub peer_local_vision_id: VisionId,
    pub peer_local_endpoint_id: EndpointIdText,
    pub peer_records: Vec<LocalPeerAuthorityRecordPartsV1>,
    pub callable_surfaces: Vec<LocalRemoteCallableSurfacePartsV1>,
    pub evaluated_at: Timestamp,
    pub projection_id: String,
    pub projection_source_mother_node_id: String,
    pub projection_source_ledger_id: String,
    pub through_sequence: u64,
    pub through_observation_id: String,
    pub through_entry_hash: String,
    pub authority_state_hash: String,
    pub projection_hash: String,
}

#[doc(hidden)]
pub struct LocalPeerAuthorityRecordPartsV1 {
    pub peer_node_id: MctNodeId,
    pub binding_id: PeerBindingId,
    pub endpoint_id: EndpointIdText,
    pub vision_id: VisionId,
    pub binding_state: BindingState,
    pub policy_revision: u64,
    pub expires_at: Timestamp,
    pub local_binding: MctPeerBinding,
    pub binding_signature_ref: Option<String>,
    pub outbound_binding: Option<MctPeerBinding>,
    pub outbound_signature_ref: Option<String>,
    pub ticket_available: bool,
    pub network_path: NetworkPathClass,
}

#[doc(hidden)]
pub struct LocalRemoteCallableSurfacePartsV1 {
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

#[doc(hidden)]
pub fn assemble_local_execution_authority_snapshot(
    parts: LocalExecutionAuthoritySnapshotPartsV1,
) -> LocalExecutionAuthoritySnapshot {
    LocalExecutionAuthoritySnapshot {
        executing_mother_node_id: parts.executing_mother_node_id,
        canonical_grants: LocalCanonicalGrantsSnapshotV1 {
            grants_authority: LocalGrantsAuthorityIdentityV1 {
                mother_node_id: parts.grants_authority_mother_node_id,
                authority_epoch: parts.grants_authority_epoch,
                generation: parts.grants_authority_generation,
                source_authority_observation_id: parts.grants_authority_observation_id,
            },
            toy_catalog: parts.toy_catalog,
            toy_grants: parts.toy_grants,
            watch_scopes: parts.watch_scopes,
        },
        policy_revision: parts.policy_revision,
        vision_policy_revision: parts.vision_policy_revision,
        child_policy: LocalChildPolicySnapshotV1 {
            provenance: LocalChildPolicyProvenanceV1::LegacyConfigAndLoadedChildProjection,
            local_node_id: parts.child_local_node_id,
            vision_id: parts.child_vision_id,
            policy_revision: parts.policy_revision,
            artifacts: parts.child_artifacts,
            approvals: parts.child_approvals,
            assignments: parts.child_assignments,
            instances: parts.child_instances,
        },
        peer_policy: LocalPeerPolicySnapshotV1 {
            provenance: LocalPeerPolicyProvenanceV1::LegacyConfigAndRuntimePeerProjection,
            local_node_id: parts.peer_local_node_id,
            local_vision_id: parts.peer_local_vision_id,
            local_endpoint_id: parts.peer_local_endpoint_id,
            policy_revision: parts.policy_revision,
            peers: parts
                .peer_records
                .into_iter()
                .map(|record| LocalPeerAuthorityRecordV1 {
                    peer_node_id: record.peer_node_id,
                    binding_id: record.binding_id,
                    endpoint_id: record.endpoint_id,
                    vision_id: record.vision_id,
                    binding_state: record.binding_state,
                    policy_revision: record.policy_revision,
                    expires_at: record.expires_at,
                    local_binding: record.local_binding,
                    binding_signature_ref: record.binding_signature_ref,
                    outbound_binding: record.outbound_binding,
                    outbound_signature_ref: record.outbound_signature_ref,
                    ticket_available: record.ticket_available,
                    network_path: record.network_path,
                })
                .collect(),
            callable_surfaces: parts
                .callable_surfaces
                .into_iter()
                .map(|surface| LocalRemoteCallableSurfaceV1 {
                    peer_node_id: surface.peer_node_id,
                    binding_id: surface.binding_id,
                    endpoint_id: surface.endpoint_id,
                    vision_id: surface.vision_id,
                    publisher_policy_revision: surface.publisher_policy_revision,
                    child_name: surface.child_name,
                    operation_id: surface.operation_id,
                    runtime_kind: surface.runtime_kind,
                    surface_policy_revision: surface.surface_policy_revision,
                    visibility: surface.visibility,
                    received_at: surface.received_at,
                    stale_at: surface.stale_at,
                })
                .collect(),
        },
        mother_clock: LocalMotherClockSnapshotV1 {
            evaluated_at: parts.evaluated_at,
            provenance: LocalMotherClockProvenanceV1::ExecutingMotherClock,
        },
        projection: LocalAuthorityProjectionProvenanceV1 {
            projection_id: parts.projection_id,
            source_mother_node_id: parts.projection_source_mother_node_id,
            source_ledger_id: parts.projection_source_ledger_id,
            through_sequence: parts.through_sequence,
            through_observation_id: parts.through_observation_id,
            through_entry_hash: parts.through_entry_hash,
            authority_state_hash: parts.authority_state_hash,
            projection_hash: parts.projection_hash,
        },
    }
}

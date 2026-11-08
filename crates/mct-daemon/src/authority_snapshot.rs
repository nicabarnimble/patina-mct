use crate::state::AuthorityProjectionSnapshotV1;
use crate::{
    MctChildLoadOptions, MctDaemonConfigStore, MctRuntimeStateStore, current_timestamp,
    load_children_from_dir, outbound_peer_binding_for_local,
};
use mct_kernel::{
    LocalExecutionAuthoritySnapshot, LocalExecutionAuthoritySnapshotPartsV1,
    LocalPeerAuthorityRecordPartsV1, LocalRemoteCallableSurfacePartsV1, NetworkPathClass,
    Timestamp, assemble_local_execution_authority_snapshot,
};
use mct_observation::{
    AuthorityProjectionCursorV1, AuthorityProjectionDenyReasonV1, AuthorityProjectionExpectationV1,
    AuthorityProjectionHashInputV1, AuthorityProjectionLedgerEvidenceV1,
    AuthorityProjectionStatusV1, GovernedChildStateV1, GovernedRuntimeV1, ObservationLedgerError,
    UsableAuthorityProjectionProofV1, authority_projection_hash, authority_state_hash,
    directory_read_contract,
};
use std::path::Path;

/// Typed fail-closed reason for Mother-local snapshot construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalExecutionAuthoritySnapshotDenyV1 {
    LedgerQuarantined,
    ForeignLineage,
    LedgerUnavailable,
    AuthorityReplayBlocked,
    AuthorityProjection(AuthorityProjectionDenyReasonV1),
    LocalPolicyUnavailable,
    MotherClockUnavailable,
}

/// Constructs a snapshot from disk-derived canonical evidence and the executing Mother's clock.
pub fn local_execution_authority_snapshot(
    ledger_path: &Path,
    config_path: &Path,
    children_dir: &Path,
    state_path: &Path,
) -> Result<LocalExecutionAuthoritySnapshot, LocalExecutionAuthoritySnapshotDenyV1> {
    local_execution_authority_snapshot_at(
        ledger_path,
        config_path,
        children_dir,
        state_path,
        Ok(current_timestamp()),
    )
}

/// Clock-injected form used to prove Mother-time provenance and unavailable-clock denial.
pub fn local_execution_authority_snapshot_at(
    ledger_path: &Path,
    config_path: &Path,
    children_dir: &Path,
    state_path: &Path,
    mother_time: Result<Timestamp, LocalExecutionAuthoritySnapshotDenyV1>,
) -> Result<LocalExecutionAuthoritySnapshot, LocalExecutionAuthoritySnapshotDenyV1> {
    let evaluated_at =
        mother_time.map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::MotherClockUnavailable)?;
    let config = MctDaemonConfigStore::new(config_path)
        .load()
        .map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable)?;
    let identity = config
        .local_identity
        .as_ref()
        .ok_or(LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable)?;
    let verified = mct_observation::load_verified_ledger_replay(
        ledger_path,
        "ledger-local",
        identity.node_id.as_str(),
    )
    .map_err(|error| match error {
        ObservationLedgerError::Quarantined { .. } => {
            LocalExecutionAuthoritySnapshotDenyV1::LedgerQuarantined
        }
        ObservationLedgerError::ForeignLineage { .. } => {
            LocalExecutionAuthoritySnapshotDenyV1::ForeignLineage
        }
        _ => LocalExecutionAuthoritySnapshotDenyV1::LedgerUnavailable,
    })?;
    local_execution_authority_snapshot_from_verified(
        verified
            .head
            .ok_or(LocalExecutionAuthoritySnapshotDenyV1::AuthorityReplayBlocked)?,
        verified.replay,
        config_path,
        children_dir,
        state_path,
        Ok(evaluated_at),
    )
}

/// Authority projection proven usable through one verified ledger head.
///
/// The resident ledger writer captures this in one serialized step: it
/// publishes the projection through its verified head and proves that same
/// projection before any other append or publication can move either. A
/// snapshot built from it therefore never pairs one head with another head's
/// projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvenAuthorityProjectionV1 {
    pub cursor: AuthorityProjectionCursorV1,
    pub projection: AuthorityProjectionSnapshotV1,
}

/// The projection expectation a verified writer head and replay establish.
pub fn authority_projection_expectation_from_verified(
    head: &mct_observation::LedgerVerifiedHead,
    replay: &mct_observation::AuthorityReplayV1,
) -> Result<AuthorityProjectionExpectationV1, LocalExecutionAuthoritySnapshotDenyV1> {
    let authority = replay
        .current_authority
        .clone()
        .ok_or(LocalExecutionAuthoritySnapshotDenyV1::AuthorityReplayBlocked)?;
    let expected_state_hash = authority_state_hash(&replay.state)
        .map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::AuthorityReplayBlocked)?;
    Ok(AuthorityProjectionExpectationV1 {
        source_mother_node_id: head.mother_node_id.clone(),
        source_ledger_id: head.ledger_id.clone(),
        through_sequence: head.local_sequence,
        through_entry_hash: head.entry_hash.clone(),
        grants_authority: authority,
        authority_state_hash: expected_state_hash,
    })
}

/// On a passkey-rooted (governed) ledger the verified replay is the projection:
/// there is no second store that could disagree with the ledger.
pub fn governed_proven_projection(
    head: &mct_observation::LedgerVerifiedHead,
    replay: &mct_observation::AuthorityReplayV1,
) -> Result<ProvenAuthorityProjectionV1, LocalExecutionAuthoritySnapshotDenyV1> {
    let blocked = |_| LocalExecutionAuthoritySnapshotDenyV1::AuthorityReplayBlocked;
    if replay.state.passkey_admin.is_none() {
        return Err(LocalExecutionAuthoritySnapshotDenyV1::AuthorityReplayBlocked);
    }
    let expectation = authority_projection_expectation_from_verified(head, replay)?;
    let projection_hash = authority_projection_hash(&AuthorityProjectionHashInputV1 {
        source_mother_node_id: head.mother_node_id.clone(),
        source_ledger_id: head.ledger_id.clone(),
        through_sequence: head.local_sequence,
        through_observation_id: head.observation_id.clone(),
        through_entry_hash: head.entry_hash.clone(),
        grants_authority: expectation.grants_authority.clone(),
        authority_state_hash: expectation.authority_state_hash.clone(),
        projection_status: AuthorityProjectionStatusV1::Current,
    })
    .map_err(blocked)?;
    let cursor = AuthorityProjectionCursorV1 {
        schema_version: 1,
        projection_id: "authority-state-v1".into(),
        projection_kind: "governed_ledger_replay".into(),
        source_mother_node_id: head.mother_node_id.clone(),
        source_ledger_id: head.ledger_id.clone(),
        through_sequence: head.local_sequence,
        through_observation_id: head.observation_id.clone(),
        through_entry_hash: head.entry_hash.clone(),
        grants_authority: expectation.grants_authority,
        authority_state_hash: expectation.authority_state_hash,
        projection_hash,
        projection_status: AuthorityProjectionStatusV1::Current,
        updated_at: crate::current_timestamp_string(),
    };
    Ok(ProvenAuthorityProjectionV1 {
        projection: AuthorityProjectionSnapshotV1 {
            cursor: cursor.clone(),
            state: replay.state.clone(),
            facts: replay.facts.clone(),
        },
        cursor,
    })
}

type GovernedChildRecords = (
    std::collections::BTreeMap<String, crate::MctStoredChildApproval>,
    std::collections::BTreeMap<String, crate::MctStoredChildAssignment>,
);

fn governed_child_records(runtime: &GovernedRuntimeV1) -> GovernedChildRecords {
    let mut approvals = std::collections::BTreeMap::new();
    let mut assignments = std::collections::BTreeMap::new();
    for child in runtime.children.values() {
        let Ok(artifact_id) = mct_kernel::ComponentArtifactId::new(child.artifact_id.clone())
        else {
            continue;
        };
        let active = child.state == GovernedChildStateV1::Approved;
        approvals.insert(
            child.child_name.clone(),
            crate::MctStoredChildApproval {
                manifest_hash: Some(child.manifest_hash.clone()),
                child_name: child.child_name.clone(),
                artifact_id: artifact_id.clone(),
                artifact_version: child.artifact_version.clone(),
                approval_state: if active {
                    mct_kernel::ChildApprovalState::Approved
                } else {
                    mct_kernel::ChildApprovalState::Revoked
                },
                vision_id: child.vision_id.clone(),
                node_id: child.node_id.clone(),
                project_id: None,
                policy_revision: child.policy_revision,
                updated_at: child.decided_at.to_string(),
            },
        );
        assignments.insert(
            child.child_name.clone(),
            crate::MctStoredChildAssignment {
                child_name: child.child_name.clone(),
                artifact_id,
                artifact_version: child.artifact_version.clone(),
                assignment_state: if active {
                    mct_kernel::ChildAssignmentState::Active
                } else {
                    mct_kernel::ChildAssignmentState::Revoked
                },
                vision_id: child.vision_id.clone(),
                node_id: child.node_id.clone(),
                project_id: None,
                policy_revision: child.policy_revision,
                updated_at: child.decided_at.to_string(),
            },
        );
    }
    (approvals, assignments)
}

fn governed_binding_state(peer: &mct_observation::GovernedPeerV1) -> mct_kernel::BindingState {
    if peer.state == GovernedChildStateV1::Approved {
        mct_kernel::BindingState::Admitted
    } else {
        mct_kernel::BindingState::Revoked
    }
}

/// The ledger decides who is a peer. A `config.json` entry survives only as the
/// host-side signature and ticket for a peer the ledger names identically.
fn governed_peer_matches(
    entry: &crate::MctPeerAddressBookEntry,
    peer: &mct_observation::GovernedPeerV1,
) -> bool {
    entry.peer_node_id == peer.peer_node_id
        && entry.binding_id == peer.binding_id
        && entry.endpoint_id == peer.endpoint_id
        && entry.vision_id == peer.vision_id
        && entry.policy_revision == peer.policy_revision
        && entry.expires_at == peer.expires_at
        && entry.binding_state == governed_binding_state(peer)
}

fn governed_outbound_binding(
    peer: &mct_observation::GovernedPeerV1,
) -> Option<crate::MctOutboundPeerBindingPresentation> {
    peer.outbound_proof
        .as_ref()
        .map(|proof| crate::MctOutboundPeerBindingPresentation {
            binding_id: proof.binding_id.clone(),
            policy_revision: proof.policy_revision,
            signature_ref: proof.signature_ref.clone(),
            expires_at: proof.expires_at.clone(),
        })
}

/// Keeps only the `config.json` peers the governed ledger names identically,
/// and takes each kept peer's outbound proof from the ledger. Every governed
/// reader that decides who may connect or be called uses this, so a stale or
/// failed publication of `config.json` cannot admit a peer the ledger revoked.
pub fn retain_peers_named_by_ledger(
    config: &mut crate::MctDaemonConfig,
    peers: &std::collections::BTreeMap<String, mct_observation::GovernedPeerV1>,
) {
    config.peers.retain(|node, entry| {
        peers
            .get(node)
            .is_some_and(|peer| governed_peer_matches(entry, peer))
    });
    for (node, entry) in &mut config.peers {
        entry.outbound_binding = peers.get(node).and_then(governed_outbound_binding);
    }
}

fn apply_governed_runtime(config: &mut crate::MctDaemonConfig, runtime: &GovernedRuntimeV1) {
    let (approvals, assignments) = governed_child_records(runtime);
    config.child_approvals = approvals;
    config.child_assignments = assignments;
    retain_peers_named_by_ledger(config, &runtime.peers);
}

/// Rewrites `config.json` approvals and peers from the governed ledger. Other
/// resident readers use that file; the ledger stays the only authority.
pub fn project_governed_config(
    config_path: &Path,
    runtime: &GovernedRuntimeV1,
) -> anyhow::Result<()> {
    let store = MctDaemonConfigStore::new(config_path);
    let mut config = store.load()?;
    let (approvals, assignments) = governed_child_records(runtime);
    config.child_approvals = approvals;
    config.child_assignments = assignments;
    config.peers.clear();
    store.save(&config)?;
    for peer in runtime.peers.values() {
        store.upsert_peer(crate::MctPeerAddressBookEntry {
            peer_node_id: peer.peer_node_id.clone(),
            binding_id: peer.binding_id.clone(),
            endpoint_id: peer.endpoint_id.clone(),
            vision_id: peer.vision_id.clone(),
            ticket: peer
                .ticket
                .clone()
                .map(serde_json::from_value)
                .transpose()?,
            binding_signature_ref: None,
            outbound_binding: governed_outbound_binding(peer),
            binding_state: governed_binding_state(peer),
            policy_revision: peer.policy_revision,
            expires_at: peer.expires_at.clone(),
            updated_at: peer.decided_at.to_string(),
        })?;
    }
    Ok(())
}

/// Proves the stored projection usable for exactly `expectation`.
pub fn prove_authority_projection(
    state: &MctRuntimeStateStore,
    expectation: AuthorityProjectionExpectationV1,
) -> Result<ProvenAuthorityProjectionV1, LocalExecutionAuthoritySnapshotDenyV1> {
    let cursor = match state
        .usable_authority_projection_proof(&AuthorityProjectionLedgerEvidenceV1::Validated(
            expectation,
        ))
        .map_err(|_| {
            LocalExecutionAuthoritySnapshotDenyV1::AuthorityProjection(
                AuthorityProjectionDenyReasonV1::ProjectionNotCurrent,
            )
        })? {
        UsableAuthorityProjectionProofV1::Usable { cursor } => *cursor,
        UsableAuthorityProjectionProofV1::Denied { reason } => {
            return Err(LocalExecutionAuthoritySnapshotDenyV1::AuthorityProjection(
                reason,
            ));
        }
    };
    let projection = state
        .authority_projection_snapshot()
        .map_err(|_| {
            LocalExecutionAuthoritySnapshotDenyV1::AuthorityProjection(
                AuthorityProjectionDenyReasonV1::ProjectionNotCurrent,
            )
        })?
        .ok_or(LocalExecutionAuthoritySnapshotDenyV1::AuthorityProjection(
            AuthorityProjectionDenyReasonV1::ProjectionMissing,
        ))?;
    if projection.cursor != cursor
        || authority_state_hash(&projection.state).ok().as_deref()
            != Some(cursor.authority_state_hash.as_str())
    {
        return Err(LocalExecutionAuthoritySnapshotDenyV1::AuthorityProjection(
            AuthorityProjectionDenyReasonV1::ProjectionNotCurrent,
        ));
    }
    Ok(ProvenAuthorityProjectionV1 { cursor, projection })
}

/// Builds a snapshot from an already verified writer head and replay.
///
/// The resident call path uses this so it does not re-read or re-hash the ledger
/// prefix. External readers still use [`local_execution_authority_snapshot`], which
/// full-verifies on checkpoint mismatch and fails closed on a suffix break.
pub fn local_execution_authority_snapshot_from_verified(
    head: mct_observation::LedgerVerifiedHead,
    replay: mct_observation::AuthorityReplayV1,
    config_path: &Path,
    children_dir: &Path,
    state_path: &Path,
    mother_time: Result<Timestamp, LocalExecutionAuthoritySnapshotDenyV1>,
) -> Result<LocalExecutionAuthoritySnapshot, LocalExecutionAuthoritySnapshotDenyV1> {
    let evaluated_at =
        mother_time.map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::MotherClockUnavailable)?;
    // Policy availability is checked before the projection, as before.
    MctDaemonConfigStore::new(config_path)
        .load()
        .map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable)?
        .local_identity
        .ok_or(LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable)?;
    if replay.state.passkey_admin.is_some() {
        return local_execution_authority_snapshot_from_proven(
            governed_proven_projection(&head, &replay)?,
            config_path,
            children_dir,
            state_path,
            Ok(evaluated_at),
        );
    }
    let expectation = authority_projection_expectation_from_verified(&head, &replay)?;
    let state = MctRuntimeStateStore::open(state_path).map_err(|_| {
        LocalExecutionAuthoritySnapshotDenyV1::AuthorityProjection(
            AuthorityProjectionDenyReasonV1::ProjectionMissing,
        )
    })?;
    let proven = prove_authority_projection(&state, expectation)?;
    drop(state);
    local_execution_authority_snapshot_from_proven(
        proven,
        config_path,
        children_dir,
        state_path,
        Ok(evaluated_at),
    )
}

/// Builds a snapshot from a projection the resident writer already proved
/// for its head. Nothing here re-reads the authority projection, so a later
/// append or publication cannot pair this snapshot with a different head.
pub fn local_execution_authority_snapshot_from_proven(
    proven: ProvenAuthorityProjectionV1,
    config_path: &Path,
    children_dir: &Path,
    state_path: &Path,
    mother_time: Result<Timestamp, LocalExecutionAuthoritySnapshotDenyV1>,
) -> Result<LocalExecutionAuthoritySnapshot, LocalExecutionAuthoritySnapshotDenyV1> {
    let evaluated_at =
        mother_time.map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::MotherClockUnavailable)?;
    let mut config = MctDaemonConfigStore::new(config_path)
        .load()
        .map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable)?;
    let ProvenAuthorityProjectionV1 {
        cursor: proven_cursor,
        projection,
    } = proven;
    let governed = projection
        .state
        .passkey_admin
        .as_ref()
        .map(|passkey| passkey.runtime.clone());
    if let Some(runtime) = &governed {
        apply_governed_runtime(&mut config, runtime);
    }
    let identity = config
        .local_identity
        .as_ref()
        .ok_or(LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable)?;
    let (toy_catalog, toy_grants, watch_scopes) = match governed {
        Some(runtime) => (
            vec![directory_read_contract()],
            runtime.toy_grants.into_values().collect(),
            Vec::new(),
        ),
        None => (
            projection.state.toy_catalog.into_values().collect(),
            projection.state.toy_grants.into_values().collect(),
            projection.state.watch_scopes.into_values().collect(),
        ),
    };
    let state = MctRuntimeStateStore::open(state_path)
        .map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable)?;

    if identity.node_id.as_str() != proven_cursor.source_mother_node_id {
        return Err(LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable);
    }
    let load_report = load_children_from_dir(MctChildLoadOptions::new(children_dir.to_path_buf()));
    if !load_report.failures.is_empty() {
        return Err(LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable);
    }
    let child_scope = crate::MctOperatorChildScope {
        vision_id: identity.vision_id.clone(),
        node_id: identity.node_id.clone(),
        project_id: None,
        policy_revision: identity.policy_revision,
    };
    let child_projection =
        config.authority_projection_for_loaded_children(load_report.children.iter(), child_scope);

    let mut peer_records = Vec::new();
    let mut callable_surfaces = Vec::new();
    for peer in config.peers.values() {
        let local_binding = peer
            .to_peer_binding(identity)
            .map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable)?;
        let outbound_binding = peer
            .outbound_binding
            .as_ref()
            .map(|outbound| outbound_peer_binding_for_local(identity, peer, outbound))
            .transpose()
            .map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable)?;
        let network_path = match peer.ticket.as_ref() {
            Some(ticket) if !ticket.direct_addresses.is_empty() => NetworkPathClass::Direct,
            Some(ticket) if !ticket.relay_urls.is_empty() => NetworkPathClass::Relayed,
            _ => NetworkPathClass::Unknown,
        };
        peer_records.push(LocalPeerAuthorityRecordPartsV1 {
            peer_node_id: peer.peer_node_id.clone(),
            binding_id: peer.binding_id.clone(),
            endpoint_id: peer.endpoint_id.clone(),
            vision_id: peer.vision_id.clone(),
            binding_state: peer.binding_state,
            policy_revision: peer.policy_revision,
            expires_at: peer.expires_at.clone(),
            local_binding,
            binding_signature_ref: peer.binding_signature_ref.clone(),
            outbound_binding,
            outbound_signature_ref: peer
                .outbound_binding
                .as_ref()
                .map(|outbound| outbound.signature_ref.clone()),
            ticket_available: peer.ticket.is_some(),
            network_path,
        });
        let surfaces = state
            .remote_callable_surfaces(&peer.peer_node_id, &peer.vision_id)
            .map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable)?;
        callable_surfaces.extend(surfaces.into_iter().map(|surface| {
            LocalRemoteCallableSurfacePartsV1 {
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
            }
        }));
    }
    peer_records.sort_by(|left, right| left.peer_node_id.cmp(&right.peer_node_id));
    callable_surfaces.sort_by(|left, right| {
        (&left.peer_node_id, &left.operation_id, &left.child_name).cmp(&(
            &right.peer_node_id,
            &right.operation_id,
            &right.child_name,
        ))
    });

    let grants_authority = proven_cursor.grants_authority.clone();
    Ok(assemble_local_execution_authority_snapshot(
        LocalExecutionAuthoritySnapshotPartsV1 {
            executing_mother_node_id: identity.node_id.to_string(),
            grants_authority_mother_node_id: grants_authority.mother_node_id,
            grants_authority_epoch: grants_authority.authority_epoch,
            grants_authority_generation: grants_authority.generation,
            grants_authority_observation_id: grants_authority.source_authority_observation_id,
            toy_catalog,
            toy_grants,
            watch_scopes,
            policy_revision: identity.policy_revision,
            vision_policy_revision: identity.policy_revision,
            child_local_node_id: child_projection.local_node_id,
            child_vision_id: child_projection.vision_id,
            child_artifacts: child_projection.artifacts,
            child_approvals: child_projection.approvals,
            child_assignments: child_projection.assignments,
            child_instances: child_projection.instances,
            peer_local_node_id: identity.node_id.clone(),
            peer_local_vision_id: identity.vision_id.clone(),
            peer_local_endpoint_id: identity.endpoint_id.clone(),
            peer_records,
            callable_surfaces,
            evaluated_at,
            projection_id: proven_cursor.projection_id,
            projection_source_mother_node_id: proven_cursor.source_mother_node_id,
            projection_source_ledger_id: proven_cursor.source_ledger_id,
            through_sequence: proven_cursor.through_sequence,
            through_observation_id: proven_cursor.through_observation_id,
            through_entry_hash: proven_cursor.through_entry_hash,
            authority_state_hash: proven_cursor.authority_state_hash,
            projection_hash: proven_cursor.projection_hash,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MctOperatorNodeScope, MctRuntimeStateStore};
    use mct_kernel::{
        CanonicalToyContract, MctNodeId, ObservationId, ToyContractIdentity, ToyGrant,
        ToyGrantConstraints, ToyGrantId, ToyGrantScope, ToyGrantState, ToyGrantSubject, ToyId,
        VisionId, WatchCoalescingPolicy, WatchEventClass, WatchObservationScope,
        WatchObservationScopeId, WatchObservationScopeState, WatchObserverRef, WatchObserverShape,
        WatchScopeMode, WatchTraversalScope,
    };
    use mct_observation::{
        AuthorityChangeV1, AuthorityMutationRequestV1, AuthorityMutationResultV1, AuthorityStateV1,
        GrantShapingCommandKindV1, GrantShapingSourceV1, JsonlObservationLedger,
        LEGACY_AUTHORITY_IMPORT_CONFIRMATION_V1, LegacyAuthorityImportRequestV1,
    };
    use std::path::PathBuf;

    struct SnapshotFixture {
        _dir: tempfile::TempDir,
        ledger_path: PathBuf,
        config_path: PathBuf,
        children_dir: PathBuf,
        state_path: PathBuf,
        ledger: JsonlObservationLedger,
        canonical_state: AuthorityStateV1,
    }

    fn complete_scope() -> WatchObservationScope {
        WatchObservationScope {
            watch_scope_id: WatchObservationScopeId::new("scope-snapshot").unwrap(),
            observer_shape: WatchObserverShape::ChildToy,
            observer_ref: WatchObserverRef {
                child_name: "child-snapshot".into(),
                artifact_id: mct_kernel::ComponentArtifactId::new("artifact-snapshot").unwrap(),
                artifact_version: "1.0.0".into(),
                assignment_id: mct_kernel::ChildAssignmentId::new("assignment-snapshot").unwrap(),
            },
            scope_mode: WatchScopeMode::Constrained,
            canonical_root_ref: "file:///tmp/snapshot-watch".into(),
            traversal_scope: WatchTraversalScope::Recursive,
            event_classes: mct_kernel::watch::WatchEventClasses::new(vec![
                WatchEventClass::Created,
                WatchEventClass::Modified,
            ])
            .unwrap(),
            max_events_per_batch: 11,
            coalescing_policy: WatchCoalescingPolicy::LastPerPath,
            starts_at: Timestamp::new("2026-08-05T12:00:00Z").unwrap(),
            expires_at: Timestamp::new("2026-08-05T13:00:00Z").unwrap(),
            scope_revision: std::num::NonZeroU64::MIN,
            policy_revision: 1,
            authority_state: WatchObservationScopeState::Active,
            authority_observation_id: ObservationId::new("obs-snapshot-watch").unwrap(),
            canonical_record_digest: String::new(),
        }
        .seal()
    }

    fn canonical_state() -> AuthorityStateV1 {
        let contract = CanonicalToyContract {
            toy_id: ToyId::new("toy-snapshot").unwrap(),
            contract: ToyContractIdentity {
                namespace: "mct:test".into(),
                interface_name: "snapshot".into(),
                version: "1.0.0".into(),
                function_name: Some("run".into()),
                resource_name: None,
            },
            authority_bearing: true,
            catalog_revision: 1,
            admitted_by_observation_id: ObservationId::new("obs-snapshot-catalog").unwrap(),
        };
        let grant = ToyGrant {
            grant_id: ToyGrantId::new("grant-snapshot").unwrap(),
            toy_id: contract.toy_id.clone(),
            subject: ToyGrantSubject {
                child_name: "child-snapshot".into(),
                artifact_id: "artifact-snapshot".into(),
                artifact_version: "1.0.0".into(),
                assignment_id: Some(
                    mct_kernel::ChildAssignmentId::new("assignment-snapshot").unwrap(),
                ),
                caller_node_id: Some(MctNodeId::new("local-mct").unwrap()),
            },
            scope: ToyGrantScope {
                vision_id: VisionId::new("vision-local").unwrap(),
                node_id: Some(MctNodeId::new("local-mct").unwrap()),
                project_id: None,
                data_classification: None,
                resource_id: Some("resource-snapshot".into()),
                allowed_actions: vec!["run".into()],
            },
            constraints: ToyGrantConstraints {
                starts_at: None,
                expires_at: None,
                max_uses: None,
                max_duration_ms: None,
                locality_required: true,
            },
            grant_state: ToyGrantState::Active,
            issuer_id: "local-mct".into(),
            policy_revision: 1,
            grants_revision: 1,
            authority_observation_id: ObservationId::new("obs-snapshot-grant").unwrap(),
        };
        let scope = complete_scope();
        AuthorityStateV1 {
            toy_catalog: [(contract.toy_id.to_string(), contract)].into(),
            toy_grants: [(grant.grant_id.to_string(), grant)].into(),
            watch_scopes: [(scope.watch_scope_id.to_string(), scope)].into(),
            admin_key_grants: std::collections::BTreeMap::new(),
            pairing_tickets: std::collections::BTreeMap::new(),
            guardian_recovery: None,
            admin_issuance: Default::default(),
            admin_genesis: None,
            passkey_admin: None,
        }
    }

    fn fixture() -> SnapshotFixture {
        let dir = tempfile::tempdir().unwrap();
        let ledger_path = dir.path().join("observations.jsonl");
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        std::fs::create_dir(&children_dir).unwrap();
        MctDaemonConfigStore::new(&config_path)
            .ensure_local_identity(
                MctOperatorNodeScope::default(),
                dir.path().join("identity.hex"),
            )
            .unwrap();
        let mut ledger =
            JsonlObservationLedger::open_authority(&ledger_path, "ledger-local", "local-mct")
                .unwrap();
        let canonical_state = canonical_state();
        let state_hash = authority_state_hash(&canonical_state).unwrap();
        let result = ledger.execute_legacy_authority_import(
            LegacyAuthorityImportRequestV1 {
                schema: "mct-legacy-authority-import-request/v1".into(),
                import_id: "snapshot-import".into(),
                expected_mother_node_id: "local-mct".into(),
                expected_ledger_id: "ledger-local".into(),
                expected_config_authority_hash: authority_state_hash(&AuthorityStateV1::default())
                    .unwrap(),
                expected_sqlite_authority_hash: state_hash,
                confirmation: LEGACY_AUTHORITY_IMPORT_CONFIRMATION_V1.into(),
            },
            "os-uid:501".into(),
            canonical_state.clone(),
            "2026-08-05T11:59:00Z".into(),
        );
        assert!(matches!(
            result,
            AuthorityMutationResultV1::CommittedProjectionPending { .. }
        ));
        MctRuntimeStateStore::open(&state_path)
            .unwrap()
            .publish_authority_projection(&ledger.entries().unwrap())
            .unwrap();
        SnapshotFixture {
            _dir: dir,
            ledger_path,
            config_path,
            children_dir,
            state_path,
            ledger,
            canonical_state,
        }
    }

    fn snapshot(fixture: &SnapshotFixture) -> LocalExecutionAuthoritySnapshot {
        local_execution_authority_snapshot_at(
            &fixture.ledger_path,
            &fixture.config_path,
            &fixture.children_dir,
            &fixture.state_path,
            Ok(Timestamp::new("2026-08-05T12:30:00Z").unwrap()),
        )
        .unwrap()
    }

    #[test]
    fn usable_dg8_proof_constructs_snapshot_with_exact_sources() {
        let fixture = fixture();
        let snapshot = snapshot(&fixture);
        let projection = MctRuntimeStateStore::open(&fixture.state_path)
            .unwrap()
            .authority_projection_snapshot()
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.executing_mother_node_id(), "local-mct");
        assert_eq!(
            snapshot.canonical_grants().toy_catalog(),
            fixture
                .canonical_state
                .toy_catalog
                .values()
                .cloned()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            snapshot.canonical_grants().toy_grants(),
            fixture
                .canonical_state
                .toy_grants
                .values()
                .cloned()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            snapshot.canonical_grants().watch_scopes(),
            fixture
                .canonical_state
                .watch_scopes
                .values()
                .cloned()
                .collect::<Vec<_>>()
        );
        assert_eq!(snapshot.child_policy().policy_revision(), 1);
        assert_eq!(snapshot.peer_policy().policy_revision(), 1);
        assert_eq!(
            snapshot.mother_clock().evaluated_at().as_str(),
            "2026-08-05T12:30:00Z"
        );
        assert_eq!(
            snapshot.projection().through_entry_hash(),
            projection.cursor.through_entry_hash
        );
        assert_eq!(
            snapshot.projection().projection_hash(),
            projection.cursor.projection_hash
        );
    }

    #[test]
    fn arbitrary_call_authority_echoes_cannot_change_snapshot_construction() {
        let fixture = fixture();
        let before = snapshot(&fixture);
        for (policy_revision, grants_revision) in [(0, 0), (1, 1), (u64::MAX, u64::MAX)] {
            let _hostile_echo = mct_kernel::AuthorityContextSnapshot {
                policy_revision,
                expected_receiver_grants_authority: crate::test_grants_authority_identity(
                    grants_revision,
                ),
                vision_policy_revision: u64::MAX - policy_revision,
            };
            assert_eq!(snapshot(&fixture), before);
        }
    }

    #[test]
    fn behind_projection_denies_then_explicit_catchup_restores_snapshot() {
        let mut fixture = fixture();
        let before = snapshot(&fixture);
        let result = fixture.ledger.execute_authority_mutation(
            AuthorityMutationRequestV1 {
                mutation_id: "snapshot-behind".into(),
                changes: vec![AuthorityChangeV1::ToyCatalogPut {
                    toy_id: "toy-after".into(),
                    contract: ToyContractIdentity {
                        namespace: "mct:test".into(),
                        interface_name: "after".into(),
                        version: "1.0.0".into(),
                        function_name: Some("run".into()),
                        resource_name: None,
                    },
                    authority_bearing: true,
                    catalog_revision: 1,
                    admitted_by_observation_id: "obs-after".into(),
                }],
                grant_shaping_sources: vec![GrantShapingSourceV1::OperatorDecision {
                    decision_id: "decision-snapshot-behind".into(),
                    authenticated_principal_ref: "os-uid:501".into(),
                    command_kind: GrantShapingCommandKindV1::CatalogChange,
                }],
                decided_at: "2026-08-05T12:31:00Z".into(),
            },
            |_| Ok(None),
        );
        assert!(matches!(
            result,
            AuthorityMutationResultV1::CommittedProjectionPending { .. }
        ));
        assert!(matches!(
            local_execution_authority_snapshot_at(
                &fixture.ledger_path,
                &fixture.config_path,
                &fixture.children_dir,
                &fixture.state_path,
                Ok(Timestamp::new("2026-08-05T12:32:00Z").unwrap()),
            ),
            Err(LocalExecutionAuthoritySnapshotDenyV1::AuthorityProjection(
                AuthorityProjectionDenyReasonV1::HeadSequenceMismatch
            ))
        ));
        MctRuntimeStateStore::open(&fixture.state_path)
            .unwrap()
            .rebuild_authority_projection(&fixture.ledger.entries().unwrap())
            .unwrap();
        let after = snapshot(&fixture);
        assert_eq!(
            after.canonical_grants().grants_authority().generation(),
            before.canonical_grants().grants_authority().generation() + 1
        );
        assert_eq!(after.canonical_grants().toy_catalog().len(), 2);
    }

    #[test]
    fn old_epoch_projection_denies_then_rebuild_restores_grant_meaning() {
        let fixture = fixture();
        let before = snapshot(&fixture);
        MctRuntimeStateStore::open(&fixture.state_path)
            .unwrap()
            .replace_authority_projection_epoch_for_test("mct-authority-epoch-v1:stale")
            .unwrap();
        assert!(matches!(
            local_execution_authority_snapshot_at(
                &fixture.ledger_path,
                &fixture.config_path,
                &fixture.children_dir,
                &fixture.state_path,
                Ok(Timestamp::new("2026-08-05T12:33:00Z").unwrap()),
            ),
            Err(LocalExecutionAuthoritySnapshotDenyV1::AuthorityProjection(
                AuthorityProjectionDenyReasonV1::EpochMismatch
            ))
        ));
        MctRuntimeStateStore::open(&fixture.state_path)
            .unwrap()
            .rebuild_authority_projection(&fixture.ledger.entries().unwrap())
            .unwrap();
        let rebuilt = snapshot(&fixture);
        assert_eq!(
            rebuilt.canonical_grants().toy_grants(),
            before.canonical_grants().toy_grants()
        );
        assert_eq!(
            rebuilt
                .canonical_grants()
                .grants_authority()
                .authority_epoch(),
            before
                .canonical_grants()
                .grants_authority()
                .authority_epoch()
        );
    }

    #[test]
    fn concurrent_mutation_never_returns_mixed_identity_grants_or_cursor() {
        let mut fixture = fixture();
        let pre = snapshot(&fixture);
        let pre_generation = pre.canonical_grants().grants_authority().generation();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let concurrent = std::thread::scope(|scope| {
            let reader_barrier = std::sync::Arc::clone(&barrier);
            let ledger_path = fixture.ledger_path.clone();
            let config_path = fixture.config_path.clone();
            let children_dir = fixture.children_dir.clone();
            let state_path = fixture.state_path.clone();
            let reader = scope.spawn(move || {
                reader_barrier.wait();
                local_execution_authority_snapshot_at(
                    &ledger_path,
                    &config_path,
                    &children_dir,
                    &state_path,
                    Ok(Timestamp::new("2026-08-05T12:34:00Z").unwrap()),
                )
            });
            barrier.wait();
            let result = fixture.ledger.execute_authority_mutation(
                AuthorityMutationRequestV1 {
                    mutation_id: "snapshot-concurrent".into(),
                    changes: vec![AuthorityChangeV1::ToyCatalogPut {
                        toy_id: "toy-concurrent".into(),
                        contract: ToyContractIdentity {
                            namespace: "mct:test".into(),
                            interface_name: "concurrent".into(),
                            version: "1.0.0".into(),
                            function_name: Some("run".into()),
                            resource_name: None,
                        },
                        authority_bearing: true,
                        catalog_revision: 1,
                        admitted_by_observation_id: "obs-concurrent".into(),
                    }],
                    grant_shaping_sources: vec![GrantShapingSourceV1::OperatorDecision {
                        decision_id: "decision-snapshot-concurrent".into(),
                        authenticated_principal_ref: "os-uid:501".into(),
                        command_kind: GrantShapingCommandKindV1::CatalogChange,
                    }],
                    decided_at: "2026-08-05T12:34:00Z".into(),
                },
                |_| Ok(None),
            );
            assert!(matches!(
                result,
                AuthorityMutationResultV1::CommittedProjectionPending { .. }
            ));
            MctRuntimeStateStore::open(&fixture.state_path)
                .unwrap()
                .rebuild_authority_projection(&fixture.ledger.entries().unwrap())
                .unwrap();
            reader.join().unwrap()
        });
        if let Ok(snapshot) = concurrent {
            let generation = snapshot.canonical_grants().grants_authority().generation();
            assert!(generation == pre_generation || generation == pre_generation + 1);
            let has_new = snapshot
                .canonical_grants()
                .toy_catalog()
                .iter()
                .any(|contract| contract.toy_id.as_str() == "toy-concurrent");
            assert_eq!(has_new, generation == pre_generation + 1);
            assert_eq!(
                snapshot.projection().source_mother_node_id(),
                snapshot
                    .canonical_grants()
                    .grants_authority()
                    .mother_node_id()
            );
        } else {
            assert!(matches!(
                concurrent,
                Err(LocalExecutionAuthoritySnapshotDenyV1::AuthorityProjection(
                    _
                ))
            ));
        }
        let post = snapshot(&fixture);
        assert_eq!(
            post.canonical_grants().grants_authority().generation(),
            pre_generation + 1
        );
        assert!(
            post.canonical_grants()
                .toy_catalog()
                .iter()
                .any(|contract| contract.toy_id.as_str() == "toy-concurrent")
        );
    }

    #[test]
    #[allow(clippy::type_complexity)]
    fn snapshot_provider_api_accepts_only_mother_owned_sources() {
        let _provider: fn(
            &Path,
            &Path,
            &Path,
            &Path,
            Result<Timestamp, LocalExecutionAuthoritySnapshotDenyV1>,
        ) -> Result<
            LocalExecutionAuthoritySnapshot,
            LocalExecutionAuthoritySnapshotDenyV1,
        > = local_execution_authority_snapshot_at;
        let kernel_source = include_str!("../../mct-kernel/src/authority.rs");
        let caller_context = ["Authority", "Context", "Snapshot"].concat();
        let caller_authority = ["Caller", "Authority", "Context"].concat();
        let copying_conversion = ["impl", " From", "<"].concat();
        assert!(!kernel_source.contains(&caller_context));
        assert!(!kernel_source.contains(&caller_authority));
        assert!(!kernel_source.contains(&copying_conversion));
    }
}

#[cfg(test)]
mod governed_peer_tests {
    use super::*;

    #[test]
    fn governed_snapshot_keeps_only_peers_the_ledger_names_identically() {
        let ts = |value: &str| Timestamp::new(value).unwrap();
        let peer = mct_observation::GovernedPeerV1 {
            peer_node_id: mct_kernel::MctNodeId::new("mother-b").unwrap(),
            binding_id: mct_kernel::PeerBindingId::new("binding-b").unwrap(),
            endpoint_id: mct_kernel::EndpointIdText::new("endpoint-b").unwrap(),
            vision_id: mct_kernel::VisionId::new("vision-local").unwrap(),
            ticket: None,
            policy_revision: 1,
            expires_at: ts("2099-01-01T00:00:00Z"),
            state: GovernedChildStateV1::Approved,
            decided_by: "ed25519:ops".into(),
            decided_at: ts("2026-11-07T00:00:00Z"),
            outbound_proof: None,
        };
        let entry = |node: &str, expires: &str| crate::MctPeerAddressBookEntry {
            peer_node_id: mct_kernel::MctNodeId::new(node).unwrap(),
            binding_id: mct_kernel::PeerBindingId::new("binding-b").unwrap(),
            endpoint_id: mct_kernel::EndpointIdText::new("endpoint-b").unwrap(),
            vision_id: mct_kernel::VisionId::new("vision-local").unwrap(),
            ticket: None,
            binding_signature_ref: Some("sig".into()),
            outbound_binding: None,
            binding_state: mct_kernel::BindingState::Admitted,
            policy_revision: 1,
            expires_at: ts(expires),
            updated_at: "2026-11-07T00:00:00Z".into(),
        };
        let mut runtime = GovernedRuntimeV1::default();
        runtime.peers.insert("mother-b".into(), peer);
        let mut config = crate::MctDaemonConfig::default();
        config
            .peers
            .insert("mother-b".into(), entry("mother-b", "2099-01-01T00:00:00Z"));
        config
            .peers
            .insert("mother-x".into(), entry("mother-x", "2099-01-01T00:00:00Z"));
        apply_governed_runtime(&mut config, &runtime);
        assert_eq!(config.peers.keys().collect::<Vec<_>>(), ["mother-b"]);

        let mut stretched = crate::MctDaemonConfig::default();
        stretched
            .peers
            .insert("mother-b".into(), entry("mother-b", "2100-01-01T00:00:00Z"));
        apply_governed_runtime(&mut stretched, &runtime);
        assert!(
            stretched.peers.is_empty(),
            "config cannot extend a signed peer's expiry"
        );
    }
}

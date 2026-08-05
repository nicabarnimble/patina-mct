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
    AuthorityProjectionDenyReasonV1, AuthorityProjectionExpectationV1,
    AuthorityProjectionLedgerEvidenceV1, JsonlObservationLedger, ObservationLedgerError,
    UsableAuthorityProjectionProofV1, authority_state_hash, replay_authority_entries,
};
use std::path::Path;

/// Typed fail-closed reason for Mother-local snapshot construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalExecutionAuthoritySnapshotDenyV1 {
    /// The structurally scanned ledger is quarantined.
    LedgerQuarantined,
    /// The ledger belongs to another lineage.
    ForeignLineage,
    /// Current canonical ledger evidence cannot be read.
    LedgerUnavailable,
    /// Authority facts cannot be replayed at the validated head.
    AuthorityReplayBlocked,
    /// Exact D-G8 projection proof denied with its original reason.
    AuthorityProjection(AuthorityProjectionDenyReasonV1),
    /// Complete local Child or peer policy inputs cannot be captured.
    LocalPolicyUnavailable,
    /// The executing Mother's clock cannot be sampled.
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
    let entries = JsonlObservationLedger::open_read_only(ledger_path, "ledger-local", "local-mct")
        .and_then(|reader| reader.entries())
        .map_err(|error| match error {
            ObservationLedgerError::Quarantined { .. } => {
                LocalExecutionAuthoritySnapshotDenyV1::LedgerQuarantined
            }
            ObservationLedgerError::ForeignLineage { .. } => {
                LocalExecutionAuthoritySnapshotDenyV1::ForeignLineage
            }
            _ => LocalExecutionAuthoritySnapshotDenyV1::LedgerUnavailable,
        })?;
    let replay = replay_authority_entries(&entries)
        .map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::AuthorityReplayBlocked)?;
    let head = entries
        .last()
        .ok_or(LocalExecutionAuthoritySnapshotDenyV1::AuthorityReplayBlocked)?;
    let authority = replay
        .current_authority
        .ok_or(LocalExecutionAuthoritySnapshotDenyV1::AuthorityReplayBlocked)?;
    let expected_state_hash = authority_state_hash(&replay.state)
        .map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::AuthorityReplayBlocked)?;
    let expectation = AuthorityProjectionExpectationV1 {
        source_mother_node_id: head.mother_node_id.clone(),
        source_ledger_id: head.ledger_id.clone(),
        through_sequence: head.local_sequence,
        through_entry_hash: head.entry_hash.clone(),
        grants_authority: authority,
        authority_state_hash: expected_state_hash,
    };
    let state = MctRuntimeStateStore::open(state_path).map_err(|_| {
        LocalExecutionAuthoritySnapshotDenyV1::AuthorityProjection(
            AuthorityProjectionDenyReasonV1::ProjectionMissing,
        )
    })?;
    let proven_cursor = match state
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
    if projection.cursor != proven_cursor
        || authority_state_hash(&projection.state).ok().as_deref()
            != Some(proven_cursor.authority_state_hash.as_str())
    {
        return Err(LocalExecutionAuthoritySnapshotDenyV1::AuthorityProjection(
            AuthorityProjectionDenyReasonV1::ProjectionNotCurrent,
        ));
    }

    let config = MctDaemonConfigStore::new(config_path)
        .load()
        .map_err(|_| LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable)?;
    let identity = config
        .local_identity
        .as_ref()
        .ok_or(LocalExecutionAuthoritySnapshotDenyV1::LocalPolicyUnavailable)?;
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
            toy_catalog: projection.state.toy_catalog.into_values().collect(),
            toy_grants: projection.state.toy_grants.into_values().collect(),
            watch_scopes: projection.state.watch_scopes.into_values().collect(),
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
            event_classes: vec![WatchEventClass::Created, WatchEventClass::Modified],
            max_events_per_batch: 11,
            coalescing_policy: WatchCoalescingPolicy::LastPerPath,
            starts_at: Timestamp::new("2026-08-05T12:00:00Z").unwrap(),
            expires_at: Timestamp::new("2026-08-05T13:00:00Z").unwrap(),
            scope_revision: 1,
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

    /// Phase I proof 3: every local source and full cursor survives exact D-G8 proof.
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

    /// Phase I proof 4: hostile caller echoes are outside the provider's input type.
    #[test]
    fn arbitrary_call_authority_echoes_cannot_change_snapshot_construction() {
        let fixture = fixture();
        let before = snapshot(&fixture);
        for (policy_revision, grants_revision) in [(0, 0), (1, 1), (u64::MAX, u64::MAX)] {
            let _hostile_echo = mct_kernel::AuthorityContextSnapshot {
                policy_revision,
                grants_revision,
                vision_policy_revision: u64::MAX - policy_revision,
            };
            assert_eq!(snapshot(&fixture), before);
        }
    }

    /// Phase I proof 6: behind projection denies exactly until explicit catch-up.
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

    /// Phase I proof 7: an old epoch projection denies until canonical rebuild.
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

    /// Phase I proof 11: every returned concurrent snapshot is entirely pre or post mutation.
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

    /// Phase I proof 13: provider API has no call-derived parameter or conversion.
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

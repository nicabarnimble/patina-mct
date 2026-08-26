//! Resident local-child and eligible remote-peer candidate sourcing and authority facts.

use super::*;

#[derive(Debug)]
pub(super) struct LocalCandidatePlan {
    pub(super) child: mct_daemon::MctLoadedChild,
    pub(super) local_runtime: LocalChildRuntime,
    pub(super) candidate: CandidateRoute,
    pub(super) authority: CandidateAuthorityEvaluation,
    pub(super) child_authority: ChildCallAuthorityResult,
    pub(super) toy_authority: Vec<ToyGrantEvaluation>,
}

#[derive(Debug)]
pub(super) struct RemoteCandidatePlan {
    pub(super) candidate: CandidateRoute,
    pub(super) authority: CandidateAuthorityEvaluation,
}

#[derive(Clone, Copy, Debug)]
struct ResidentRemoteCandidateSource<'a> {
    call: &'a MctCall,
}

impl<'a> ResidentRemoteCandidateSource<'a> {
    fn for_call(call: &'a MctCall) -> Option<Self> {
        call.origin
            .allows_remote_candidate_sourcing()
            .then_some(Self { call })
    }
}

pub(super) fn resident_candidate_for_child(
    projection: &MctConfigChildAuthorityProjection,
    child: &mct_daemon::MctLoadedChild,
) -> (LocalChildRuntime, CandidateRoute) {
    let local_runtime = LocalChildRuntime::WasmComponent;
    let child_id = ChildId::new(child.name.clone())
        .expect("string ID literal/generated value must be non-empty");
    let candidate = CandidateRoute {
        candidate_id: format!("child:{}", child.name),
        node_id: projection.local_node_id.clone(),
        child_id: Some(child_id),
        runtime_kind: local_runtime.into(),
        network_path: NetworkPathClass::Local,
    };
    (local_runtime, candidate)
}

fn resident_remote_candidate_plans_from_source(
    snapshot: &LocalExecutionAuthoritySnapshot,
    source: ResidentRemoteCandidateSource<'_>,
) -> Result<Vec<RemoteCandidatePlan>> {
    let call = source.call;
    let now = snapshot.mother_clock().evaluated_at();
    let operation_id = mct_daemon::operation_id_from_target(&call.target);
    let mut plans = Vec::new();
    for surface in snapshot
        .peer_policy()
        .callable_surfaces()
        .iter()
        .filter(|surface| {
            surface.operation_id() == operation_id
                && surface.vision_id() == &call.caller.vision_id
                && surface.stale_at() > now
        })
    {
        let Some(peer) = snapshot
            .peer_policy()
            .peers()
            .iter()
            .find(|peer| peer.peer_node_id() == surface.peer_node_id())
        else {
            continue;
        };
        let candidate = resident_candidate_for_snapshot_remote_surface(peer, surface);
        let authority =
            resident_remote_candidate_authority(snapshot, peer, surface, candidate.clone(), call)?;
        plans.push(RemoteCandidatePlan {
            candidate,
            authority,
        });
    }
    Ok(plans)
}

pub(super) fn resident_remote_candidate_plans_for_call(
    snapshot: &LocalExecutionAuthoritySnapshot,
    call: &MctCall,
) -> Result<Vec<RemoteCandidatePlan>> {
    ResidentRemoteCandidateSource::for_call(call)
        .map(|source| resident_remote_candidate_plans_from_source(snapshot, source))
        .transpose()
        .map(Option::unwrap_or_default)
}

#[cfg(test)]
fn resident_remote_candidate_plans(
    config: &mct_daemon::MctDaemonConfig,
    state: Option<&MctRuntimeStateStore>,
    call: &MctCall,
    now: Timestamp,
) -> Result<Vec<RemoteCandidatePlan>> {
    let snapshot = resident_test_authority_snapshot(
        config,
        state,
        &[],
        now,
        call.authority_context
            .expected_receiver_grants_authority
            .generation,
    )?;
    let source = ResidentRemoteCandidateSource::for_call(call)
        .context("test call must have a local origin to source remote candidates")?;
    resident_remote_candidate_plans_from_source(&snapshot, source)
}

pub(super) fn resident_candidate_for_snapshot_remote_surface(
    peer: &LocalPeerAuthorityRecordV1,
    surface: &LocalRemoteCallableSurfaceV1,
) -> CandidateRoute {
    CandidateRoute {
        candidate_id: format!(
            "peer:{}:{}:{}:{}",
            surface.peer_node_id(),
            surface.binding_id(),
            surface.operation_id(),
            surface.child_name()
        ),
        node_id: peer.peer_node_id().clone(),
        child_id: Some(
            ChildId::new(surface.child_name().to_owned())
                .expect("string ID literal/generated value must be non-empty"),
        ),
        runtime_kind: RuntimeKind::RemotePeer,
        network_path: peer.network_path(),
    }
}

#[cfg(test)]
pub(super) fn resident_peer_network_path(
    peer: &mct_daemon::MctPeerAddressBookEntry,
) -> NetworkPathClass {
    let Some(ticket) = peer.ticket.as_ref() else {
        return NetworkPathClass::Unknown;
    };
    if !ticket.direct_addresses.is_empty() {
        NetworkPathClass::Direct
    } else if !ticket.relay_urls.is_empty() {
        NetworkPathClass::Relayed
    } else {
        NetworkPathClass::Unknown
    }
}

pub(super) fn resident_remote_candidate_authority(
    snapshot: &LocalExecutionAuthoritySnapshot,
    peer: &LocalPeerAuthorityRecordV1,
    surface: &LocalRemoteCallableSurfaceV1,
    candidate: CandidateRoute,
    call: &MctCall,
) -> Result<CandidateAuthorityEvaluation> {
    let now = snapshot.mother_clock().evaluated_at();
    let operation_id = mct_daemon::operation_id_from_target(&call.target);
    let reason = match verify_peer_binding_signature_ref(
        peer.binding_signature_ref(),
        peer.local_binding(),
        snapshot.peer_policy().local_endpoint_id(),
    ) {
        MctPeerBindingSignatureVerification::Valid => None,
        MctPeerBindingSignatureVerification::Missing
        | MctPeerBindingSignatureVerification::Malformed
        | MctPeerBindingSignatureVerification::Invalid => {
            Some(CandidateEliminationReason::PeerNotAdmitted)
        }
    }
    .or_else(|| {
        let Some(outbound_binding) = peer.outbound_binding() else {
            return Some(CandidateEliminationReason::PeerNotAdmitted);
        };
        match verify_peer_binding_signature_ref(
            peer.outbound_signature_ref(),
            outbound_binding,
            peer.endpoint_id(),
        ) {
            MctPeerBindingSignatureVerification::Valid => None,
            MctPeerBindingSignatureVerification::Missing
            | MctPeerBindingSignatureVerification::Malformed
            | MctPeerBindingSignatureVerification::Invalid => {
                Some(CandidateEliminationReason::PeerNotAdmitted)
            }
        }
    })
    .or_else(|| {
        (timestamp_not_after(peer.expires_at(), now).unwrap_or(true))
            .then_some(CandidateEliminationReason::PeerNotAdmitted)
    })
    .or_else(|| {
        peer.outbound_binding().and_then(|binding| {
            timestamp_not_after(&binding.expires_at, now)
                .unwrap_or(true)
                .then_some(CandidateEliminationReason::PeerNotAdmitted)
        })
    })
    .or_else(|| {
        (peer.binding_state() != BindingState::Admitted)
            .then_some(CandidateEliminationReason::PeerNotAdmitted)
    })
    .or_else(|| {
        (surface.binding_id() != peer.binding_id() || surface.endpoint_id() != peer.endpoint_id())
            .then_some(CandidateEliminationReason::PeerNotAdmitted)
    })
    .or_else(|| {
        (!peer
            .local_binding()
            .scope
            .allowed_alpns
            .iter()
            .any(|alpn| alpn == MCT_CALL_ALPN)
            || !peer.outbound_binding().is_some_and(|binding| {
                binding
                    .scope
                    .allowed_alpns
                    .iter()
                    .any(|alpn| alpn == MCT_CALL_ALPN)
            }))
        .then_some(CandidateEliminationReason::PeerNotAdmitted)
    })
    .or_else(|| {
        (peer.vision_id() != &call.caller.vision_id
            || surface.vision_id() != &call.caller.vision_id)
            .then_some(CandidateEliminationReason::VisionPolicyDenied)
    })
    .or_else(|| {
        (peer.policy_revision() != snapshot.policy_revision()
            || peer.local_binding().policy_revision != snapshot.policy_revision()
            || peer
                .outbound_binding()
                .is_none_or(|binding| binding.policy_revision != snapshot.policy_revision())
            || surface.publisher_policy_revision() != snapshot.policy_revision()
            || surface.surface_policy_revision() != snapshot.policy_revision())
        .then_some(CandidateEliminationReason::PolicyRevisionStale)
    })
    .or_else(|| {
        call.payload_metadata
            .contains_secret_scoped_material
            .then_some(CandidateEliminationReason::SecretScopeForbidden)
    })
    .or_else(|| {
        (surface.operation_id() != operation_id || surface.visibility() != "vision_scoped")
            .then_some(CandidateEliminationReason::CapabilityUnavailable)
    })
    .or_else(|| {
        (!peer.ticket_available()).then_some(CandidateEliminationReason::CapabilityUnavailable)
    });

    let generation = snapshot.canonical_grants().grants_authority().generation();
    Ok(match reason {
        Some(reason) => CandidateAuthorityEvaluation::eliminated(
            candidate,
            reason,
            snapshot.policy_revision(),
            generation,
        ),
        None => CandidateAuthorityEvaluation::admissible(
            candidate,
            snapshot.policy_revision(),
            generation,
        ),
    })
}

pub(super) fn timestamp_not_after(timestamp: &Timestamp, now: &Timestamp) -> Result<bool> {
    let timestamp = timestamp
        .as_str()
        .parse::<jiff::Timestamp>()
        .context("parse timestamp")?;
    let now = now
        .as_str()
        .parse::<jiff::Timestamp>()
        .context("parse current timestamp")?;
    Ok(timestamp <= now)
}

pub(super) fn child_elimination_reason(reason: ChildCallReasonCode) -> CandidateEliminationReason {
    match reason {
        ChildCallReasonCode::ReadyAuthorizedInstance => CandidateEliminationReason::RouteMismatch,
        ChildCallReasonCode::InstanceNotReady => CandidateEliminationReason::CapabilityUnavailable,
        ChildCallReasonCode::StalePolicy => CandidateEliminationReason::PolicyRevisionStale,
        ChildCallReasonCode::OperationNotExported
        | ChildCallReasonCode::UnknownInstance
        | ChildCallReasonCode::MissingAssignment
        | ChildCallReasonCode::AssignmentRevoked
        | ChildCallReasonCode::MissingApproval
        | ChildCallReasonCode::ApprovalNotApproved
        | ChildCallReasonCode::ApprovalScopeMismatch
        | ChildCallReasonCode::ArtifactMissing
        | ChildCallReasonCode::ArtifactRejected
        | ChildCallReasonCode::UnsupportedLocalRuntime
        | ChildCallReasonCode::WrongNode
        | ChildCallReasonCode::WrongProject
        | ChildCallReasonCode::VersionMismatch => CandidateEliminationReason::ChildNotApproved,
    }
}

pub(super) fn resident_candidate_observations(
    call: &MctCall,
    plans: &[LocalCandidatePlan],
) -> Vec<MctObservation> {
    let mut observations = Vec::new();
    for plan in plans {
        observations.push(candidate_considered_observation(
            call.trace_context.trace_id.clone(),
            current_timestamp(),
            call,
            &plan.candidate,
            ObservationId::new(format!(
                "obs-route-candidate-considered:{}:{}",
                call.call_id, plan.candidate.candidate_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            plan.authority.policy_revision,
            plan.authority.grants_revision,
        ));
        observations.extend(plan.toy_authority.iter().map(|evaluation| {
            toy_grant_evaluation_observation(
                call.trace_context.trace_id.clone(),
                current_timestamp(),
                evaluation,
            )
        }));
        if plan.authority.outcome == CandidateAuthorityOutcome::Eliminated {
            observations.push(candidate_eliminated_observation(
                call.trace_context.trace_id.clone(),
                current_timestamp(),
                call,
                &plan.authority,
                ObservationId::new(format!(
                    "obs-route-candidate-eliminated:{}:{}",
                    call.call_id, plan.candidate.candidate_id
                ))
                .expect("string ID literal/generated value must not be empty"),
            ));
            observations.push(child_call_authority_observation(
                call.trace_context.trace_id.clone(),
                current_timestamp(),
                &plan.child_authority.evaluation,
            ));
        }
    }
    observations
}

pub(super) fn resident_remote_candidate_observations(
    call: &MctCall,
    plans: &[RemoteCandidatePlan],
) -> Vec<MctObservation> {
    let mut observations = Vec::new();
    for plan in plans {
        observations.push(candidate_considered_observation(
            call.trace_context.trace_id.clone(),
            current_timestamp(),
            call,
            &plan.candidate,
            ObservationId::new(format!(
                "obs-route-candidate-considered:{}:{}",
                call.call_id, plan.candidate.candidate_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            plan.authority.policy_revision,
            plan.authority.grants_revision,
        ));
        if plan.authority.outcome == CandidateAuthorityOutcome::Eliminated {
            observations.push(candidate_eliminated_observation(
                call.trace_context.trace_id.clone(),
                current_timestamp(),
                call,
                &plan.authority,
                ObservationId::new(format!(
                    "obs-route-candidate-eliminated:{}:{}",
                    call.call_id, plan.candidate.candidate_id
                ))
                .expect("string ID literal/generated value must be non-empty"),
            ));
        }
    }
    observations
}

pub(super) fn resident_child_accepts_call(
    child: &mct_daemon::MctLoadedChild,
    call: &MctCall,
) -> bool {
    let operation_id = mct_daemon::operation_id_from_target(&call.target);
    match child.ingress_mode {
        mct_daemon::MctChildIngressMode::Handle => {
            child.allowed_operations.is_empty()
                || child
                    .allowed_operations
                    .iter()
                    .any(|allowed| allowed == &operation_id)
        }
        mct_daemon::MctChildIngressMode::Hybrid | mct_daemon::MctChildIngressMode::WitOnly => {
            child.allows_operation_target(&call.target)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mct_iroh::{endpoint_id_for_secret_key_hex, sign_peer_binding_signature_ref};

    struct CandidateFixture {
        _dir: tempfile::TempDir,
        config: mct_daemon::MctDaemonConfig,
        state: MctRuntimeStateStore,
        call: MctCall,
    }

    fn contract_peer_expiry() -> Timestamp {
        Timestamp::new("2099-01-01T00:00:00Z").unwrap()
    }
    fn test_child() -> mct_daemon::MctLoadedChild {
        mct_daemon::MctLoadedChild {
            child_id: ChildId::new("child-demo")
                .expect("string ID literal/generated value must be non-empty"),
            name: "child-demo".into(),
            version: "0.1.0".into(),
            description: None,
            kind: "wasm".into(),
            role: None,
            wasm_path: PathBuf::from("child-demo.wasm"),
            manifest_path: PathBuf::from("child.toml"),
            wasm_digest: mct_daemon::MctChildFileDigest {
                sha256: "wasm".into(),
                sidecar_present: true,
                verified: true,
            },
            manifest_digest: mct_daemon::MctChildFileDigest {
                sha256: "manifest".into(),
                sidecar_present: true,
                verified: true,
            },
            artifact_id: "artifact-demo".into(),
            ingress_mode: mct_daemon::MctChildIngressMode::WitOnly,
            allowed_operations: vec!["patina:demo/control@0.1.0.run".into()],
            requested_toys: Vec::new(),
            subscribed_streams: Vec::new(),
            relationship_listens: Vec::new(),
            wasm_size_bytes: 1,
            instance_state: mct_daemon::MctChildInstanceState::Ready,
        }
    }
    fn resident_test_call(trace_id: TraceId) -> MctCall {
        let mut call = local_wasm_call(
            OperationTarget {
                namespace: "patina:demo".into(),
                interface_name: "control@0.1.0".into(),
                function_name: "run".into(),
            },
            test_grants_authority_identity(1),
        );
        call.call_id = CallId::new("call-resident-wit")
            .expect("string ID literal/generated value must be non-empty");
        call.trace_context.trace_id = trace_id;
        call.origin = CallOrigin::Iroh;
        call
    }
    fn candidate_fixture() -> CandidateFixture {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let state_path = dir.path().join("state.sqlite");
        let local_identity_path = dir.path().join("identity").join("iroh-secret.hex");
        let remote_identity_path = dir.path().join("remote").join("iroh-secret.hex");
        let store = MctDaemonConfigStore::new(&config_path);
        let local_identity = store
            .ensure_local_identity(MctOperatorNodeScope::default(), &local_identity_path)
            .unwrap();
        let remote_secret = load_or_create_node_secret_key_hex(&remote_identity_path).unwrap();
        let remote_endpoint_id = endpoint_id_for_secret_key_hex(&remote_secret).unwrap();
        store
            .upsert_peer(resident_remote_peer_entry(
                "remote-mct",
                "binding-remote",
                remote_endpoint_id.as_str(),
                "vision-local",
                BindingState::Admitted,
                None,
            ))
            .unwrap();
        store
            .approve_and_assign_loaded_child(&test_child(), MctOperatorChildScope::default())
            .unwrap();
        let mut config = store.load().unwrap();
        let peer = config.peers.get("remote-mct").unwrap().clone();
        let outbound_binding = MctOutboundPeerBindingPresentation {
            binding_id: PeerBindingId::new("binding-outbound-local")
                .expect("string ID literal/generated value must be non-empty"),
            policy_revision: 1,
            signature_ref: String::new(),
            expires_at: contract_peer_expiry(),
        };
        let outbound_binding_to_sign =
            outbound_peer_binding_for_local(&local_identity, &peer, &outbound_binding).unwrap();
        let outbound_signature = sign_peer_binding_signature_ref(
            &remote_secret,
            &outbound_binding_to_sign,
            &remote_endpoint_id,
        )
        .unwrap();
        store
            .set_peer_outbound_proof(
                &peer.peer_node_id,
                MctOutboundPeerBindingPresentation {
                    signature_ref: outbound_signature,
                    ..outbound_binding
                },
            )
            .unwrap();
        config = store.load().unwrap();
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        let view = hello_capability_view(
            &peer.peer_node_id,
            &peer.vision_id,
            1,
            &["patina:demo/control@0.1.0.run"],
        );
        state
            .refresh_remote_callable_surfaces(MctRemoteSurfaceRefresh {
                peer_node_id: &peer.peer_node_id,
                binding_id: &peer.binding_id,
                endpoint_id: &peer.endpoint_id,
                view: &view,
                received_at: &Timestamp::new("2026-07-09T00:00:00Z").unwrap(),
                stale_at: &Timestamp::new("2026-07-09T00:05:00Z").unwrap(),
                view_observation_id: &ObservationId::new("obs-remote-surface-view")
                    .expect("string ID literal/generated value must be non-empty"),
            })
            .unwrap();
        let mut call = resident_test_call(
            TraceId::new("trace-remote-route-candidate")
                .expect("string ID literal/generated value must be non-empty"),
        );
        call.origin = CallOrigin::Cli;
        CandidateFixture {
            _dir: dir,
            config,
            state,
            call,
        }
    }
    fn resident_remote_peer_entry(
        peer_node_id: &str,
        binding_id: &str,
        endpoint_id: &str,
        vision_id: &str,
        binding_state: BindingState,
        binding_signature_ref: Option<String>,
    ) -> MctPeerAddressBookEntry {
        MctPeerAddressBookEntry {
            peer_node_id: MctNodeId::new(peer_node_id)
                .expect("string ID literal/generated value must be non-empty"),
            binding_id: PeerBindingId::new(binding_id)
                .expect("string ID literal/generated value must be non-empty"),
            endpoint_id: EndpointIdText::new(endpoint_id)
                .expect("string ID literal/generated value must be non-empty"),
            vision_id: VisionId::new(vision_id)
                .expect("string ID literal/generated value must be non-empty"),
            ticket: Some(MotherIrohEndpointTicket {
                endpoint_id: EndpointIdText::new(endpoint_id)
                    .expect("string ID literal/generated value must be non-empty"),
                direct_addresses: vec!["127.0.0.1:12345".into()],
                relay_urls: Vec::new(),
            }),
            binding_signature_ref,
            outbound_binding: None,
            binding_state,
            policy_revision: 1,
            expires_at: contract_peer_expiry(),
            updated_at: "2026-07-09T00:00:00Z".into(),
        }
    }
    fn hello_capability_view(
        node_id: &MctNodeId,
        vision_id: &VisionId,
        policy_revision: u64,
        operations: &[&str],
    ) -> MctHelloCapabilityView {
        MctHelloCapabilityView {
            node_id: node_id.clone(),
            vision_id: vision_id.clone(),
            published_at: Timestamp::new("2026-07-09T00:00:00Z").unwrap(),
            policy_revision,
            supported_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            supported_wit_worlds: vec!["patina:demo/control@0.1.0".into()],
            supported_observation_modes: vec!["local-ledger".into()],
            callable_surfaces: operations
                .iter()
                .map(|operation| MctHelloCallableSurface {
                    child_name: "remote-child".into(),
                    operation_id: (*operation).into(),
                    runtime_kind: RuntimeKind::WasmComponent,
                    vision_id: vision_id.clone(),
                    policy_revision,
                    visibility: "vision_scoped".into(),
                })
                .collect(),
            capability_view_ref: None,
        }
    }
    fn write_resident_wasm_child(children_dir: &Path) {
        write_test_wasm_child(children_dir, "resident-echo");
    }
    #[test]
    fn resident_authorized_unavailable_is_temporal_no_route() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        write_resident_wasm_child(&children_dir);
        let mut loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir));
        let config_store = MctDaemonConfigStore::new(&config_path);
        config_store
            .ensure_local_identity(
                MctOperatorNodeScope::default(),
                dir.path().join("identity.hex"),
            )
            .unwrap();
        config_store
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        loaded.children[0].instance_state = mct_daemon::MctChildInstanceState::Loading;
        let config = MctDaemonConfigStore::new(&config_path).load().unwrap();
        let call = resident_test_call(
            TraceId::new("trace-route-unavailable")
                .expect("string ID literal/generated value must be non-empty"),
        );

        let outcome =
            authorize_resident_child_from_loaded(&config, loaded.children, &call).unwrap();
        let RouteDisposition::Denied { observations, .. } = outcome else {
            panic!("loading child should produce temporal no-route")
        };
        let text = serde_json::to_string(&observations).unwrap();
        assert!(text.contains("CapabilityUnavailable"));
        assert!(text.contains("denial_class:temporal"));
    }

    #[test]
    fn resident_remote_surface_candidate_becomes_admissible_when_all_checks_pass() {
        let fixture = candidate_fixture();

        let plans = resident_remote_candidate_plans(
            &fixture.config,
            Some(&fixture.state),
            &fixture.call,
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
        )
        .unwrap();

        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].candidate.candidate_id,
            "peer:remote-mct:binding-remote:patina:demo/control@0.1.0.run:remote-child"
        );
        assert_eq!(plans[0].candidate.runtime_kind, RuntimeKind::RemotePeer);
        assert_eq!(plans[0].candidate.network_path, NetworkPathClass::Direct);
        assert_eq!(
            plans[0].authority.outcome,
            CandidateAuthorityOutcome::Admissible
        );
        assert_eq!(plans[0].authority.reason, None);
    }

    /// Phase I proof 10: remote policy compares peer facts to local snapshot policy.
    #[test]
    fn remote_policy_mismatch_denies_even_when_caller_echo_matches_stale_peer() {
        let mut fixture = candidate_fixture();
        fixture
            .config
            .local_identity
            .as_mut()
            .unwrap()
            .policy_revision = 2;
        fixture.call.authority_context.policy_revision = 1;

        let plans = resident_remote_candidate_plans(
            &fixture.config,
            Some(&fixture.state),
            &fixture.call,
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
        )
        .unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].authority.reason,
            Some(CandidateEliminationReason::PolicyRevisionStale),
            "call echo 1 cannot make stale peer policy 1 equal local snapshot policy 2"
        );
    }

    /// Covers `PeerOperationalRoleDerivation.EligibleRouteCandidateDerivation`,
    /// `PeerRelationshipTaxonomy.RolesAreCurrentProjections`, and
    /// `BilateralExecutableRouting` by removing each current conjunct independently.
    #[test]
    fn eligible_route_candidate_requires_every_current_conjunct() {
        let fixture = candidate_fixture();
        let now = Timestamp::new("2026-07-09T00:01:00Z").unwrap();
        let is_admissible =
            |config: &mct_daemon::MctDaemonConfig, state: &MctRuntimeStateStore, call: &MctCall| {
                resident_remote_candidate_plans(config, Some(state), call, now.clone())
                    .unwrap()
                    .iter()
                    .any(|plan| plan.authority.outcome == CandidateAuthorityOutcome::Admissible)
            };

        assert!(is_admissible(
            &fixture.config,
            &fixture.state,
            &fixture.call
        ));

        let mut without_local_admission = fixture.config.clone();
        without_local_admission
            .peers
            .get_mut("remote-mct")
            .unwrap()
            .binding_state = BindingState::Pending;
        assert!(!is_admissible(
            &without_local_admission,
            &fixture.state,
            &fixture.call
        ));

        let mut without_reverse_admission = fixture.config.clone();
        without_reverse_admission
            .peers
            .get_mut("remote-mct")
            .unwrap()
            .outbound_binding = None;
        assert!(!is_admissible(
            &without_reverse_admission,
            &fixture.state,
            &fixture.call
        ));

        let empty_state =
            MctRuntimeStateStore::open(fixture._dir.path().join("empty.sqlite")).unwrap();
        assert!(!is_admissible(&fixture.config, &empty_state, &fixture.call));

        let mut wrong_vision = fixture.call.clone();
        wrong_vision.caller.vision_id = VisionId::new("vision-other").unwrap();
        assert!(!is_admissible(
            &fixture.config,
            &fixture.state,
            &wrong_vision
        ));

        let mut outside_call_scope = fixture.call.clone();
        outside_call_scope.target.function_name = "not-published".into();
        assert!(!is_admissible(
            &fixture.config,
            &fixture.state,
            &outside_call_scope
        ));

        let mut without_reachability = fixture.config.clone();
        without_reachability
            .peers
            .get_mut("remote-mct")
            .unwrap()
            .ticket = None;
        assert!(!is_admissible(
            &without_reachability,
            &fixture.state,
            &fixture.call
        ));
    }

    /// Covers `CapabilityPublicationRelationship.OfferLapsesAtFreshnessBoundary`.
    #[test]
    fn capability_offer_lapses_at_freshness_boundary() {
        let fixture = candidate_fixture();

        for now in ["2026-07-09T00:05:00Z", "2026-07-09T00:05:01Z"] {
            let plans = resident_remote_candidate_plans(
                &fixture.config,
                Some(&fixture.state),
                &fixture.call,
                Timestamp::new(now).unwrap(),
            )
            .unwrap();
            assert!(
                !plans.iter().any(|plan| {
                    plan.authority.outcome == CandidateAuthorityOutcome::Admissible
                }),
                "publication must not remain admissible at {now}"
            );
        }
    }

    #[test]
    fn resident_remote_surface_candidate_forbids_secret_scope() {
        let mut fixture = candidate_fixture();
        fixture
            .call
            .payload_metadata
            .contains_secret_scoped_material = true;

        let plans = resident_remote_candidate_plans(
            &fixture.config,
            Some(&fixture.state),
            &fixture.call,
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
        )
        .unwrap();

        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].authority.reason,
            Some(CandidateEliminationReason::SecretScopeForbidden)
        );
    }

    #[test]
    fn resident_remote_route_candidates_reject_unsigned_peer_binding() {
        let mut fixture = candidate_fixture();
        fixture
            .config
            .peers
            .get_mut("remote-mct")
            .unwrap()
            .binding_signature_ref = None;

        let plans = resident_remote_candidate_plans(
            &fixture.config,
            Some(&fixture.state),
            &fixture.call,
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
        )
        .unwrap();

        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].authority.reason,
            Some(CandidateEliminationReason::PeerNotAdmitted)
        );
    }

    #[test]
    fn two_mother_wrong_vision_fails_closed() {
        let mut fixture = candidate_fixture();
        fixture.call.caller.vision_id = VisionId::new("vision-other")
            .expect("string ID literal/generated value must be non-empty");

        let plans = resident_remote_candidate_plans(
            &fixture.config,
            Some(&fixture.state),
            &fixture.call,
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
        )
        .unwrap();

        assert!(plans.is_empty());
    }

    #[test]
    fn two_mother_revoked_or_expired_binding_fails_closed() {
        let mut fixture = candidate_fixture();
        fixture
            .config
            .peers
            .get_mut("remote-mct")
            .unwrap()
            .binding_state = BindingState::Revoked;

        let revoked = resident_remote_candidate_plans(
            &fixture.config,
            Some(&fixture.state),
            &fixture.call,
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
        )
        .unwrap();
        assert_eq!(
            revoked[0].authority.reason,
            Some(CandidateEliminationReason::PeerNotAdmitted)
        );

        let peer = fixture.config.peers.get_mut("remote-mct").unwrap();
        peer.binding_state = BindingState::Admitted;
        peer.outbound_binding.as_mut().unwrap().expires_at =
            Timestamp::new("2026-07-08T00:00:00Z").unwrap();
        let expired = resident_remote_candidate_plans(
            &fixture.config,
            Some(&fixture.state),
            &fixture.call,
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
        )
        .unwrap();
        assert_eq!(
            expired[0].authority.reason,
            Some(CandidateEliminationReason::PeerNotAdmitted)
        );
    }

    #[test]
    fn two_mother_unauthorized_operation_fails_closed() {
        let mut fixture = candidate_fixture();
        fixture.call.target.function_name = "missing".into();

        let plans = resident_remote_candidate_plans(
            &fixture.config,
            Some(&fixture.state),
            &fixture.call,
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
        )
        .unwrap();

        assert!(plans.is_empty());
    }
}

//! Resident admissible-only ranking, route decisions, and local kernel revalidation.

use super::*;

#[derive(Debug)]
pub(super) struct LocalExecutionPlan {
    child: mct_daemon::MctLoadedChild,
    authorized_route: AuthorizedRouteExecution,
    child_authority_observation_id: ObservationId,
}

impl LocalExecutionPlan {
    pub(super) fn into_parts(
        self,
    ) -> (
        mct_daemon::MctLoadedChild,
        AuthorizedRouteExecution,
        ObservationId,
    ) {
        (
            self.child,
            self.authorized_route,
            self.child_authority_observation_id,
        )
    }
}

#[derive(Debug)]
pub(super) struct RemoteExecutionPlan {
    candidate: CandidateRoute,
    initial_decision: RouteDecision,
}

impl RemoteExecutionPlan {
    fn new(candidate: CandidateRoute, initial_decision: RouteDecision) -> Self {
        assert_eq!(initial_decision.selected_route.as_ref(), Some(&candidate));
        Self {
            candidate,
            initial_decision,
        }
    }

    pub(super) fn candidate(&self) -> &CandidateRoute {
        &self.candidate
    }
    pub(super) fn initial_decision(&self) -> &RouteDecision {
        &self.initial_decision
    }
}

#[derive(Debug)]
enum SelectedCandidate {
    Local(Box<LocalCandidatePlan>),
    Remote(RemoteCandidatePlan),
}

impl SelectedCandidate {
    fn candidate(&self) -> &CandidateRoute {
        match self {
            Self::Local(plan) => &plan.candidate,
            Self::Remote(plan) => &plan.candidate,
        }
    }
}

#[derive(Debug)]
pub(super) enum RouteDisposition {
    Denied {
        decision: Box<RouteDecision>,
        observations: Vec<MctObservation>,
    },
    Local {
        plan: Box<LocalExecutionPlan>,
        observations: Vec<MctObservation>,
    },
    Remote {
        plan: Box<RemoteExecutionPlan>,
        observations: Vec<MctObservation>,
    },
}

pub(super) async fn authorize_resident_child(
    paths: ResidentRuntimePaths,
    call: MctCall,
) -> Result<RouteDisposition> {
    tokio::task::spawn_blocking(move || authorize_resident_child_blocking(&paths, &call))
        .await
        .context("join resident child authorization")?
}

pub(super) fn authorize_resident_child_blocking(
    paths: &ResidentRuntimePaths,
    call: &MctCall,
) -> Result<RouteDisposition> {
    let config = MctDaemonConfigStore::new(paths.config_path()).load()?;
    let state = MctRuntimeStateStore::open(paths.state_path())?;
    let load_report =
        load_children_from_dir(MctChildLoadOptions::new(paths.children_dir().to_path_buf()));
    authorize_resident_child_from_loaded_with_state(
        &config,
        Some(&state),
        load_report.children,
        call,
        current_timestamp(),
    )
}

#[cfg(test)]
pub(super) fn authorize_resident_child_from_loaded(
    config: &mct_daemon::MctDaemonConfig,
    children: Vec<mct_daemon::MctLoadedChild>,
    call: &MctCall,
) -> Result<RouteDisposition> {
    authorize_resident_child_from_loaded_with_state(
        config,
        None,
        children,
        call,
        current_timestamp(),
    )
}

pub(super) fn authorize_resident_child_from_loaded_with_state(
    config: &mct_daemon::MctDaemonConfig,
    state: Option<&MctRuntimeStateStore>,
    children: Vec<mct_daemon::MctLoadedChild>,
    call: &MctCall,
    now: Timestamp,
) -> Result<RouteDisposition> {
    let scope = resident_child_scope(config);
    let projection = config.authority_projection_for_loaded_children(children.iter(), scope);
    let mut plans = Vec::new();

    for child in children
        .into_iter()
        .filter(|child| resident_child_accepts_call(child, call))
    {
        let child_authority = projection.authorize_child_for_call(&child.name, call);
        let candidate = resident_candidate_for_child(&projection, &child);
        let authority = if child_authority.is_allowed() {
            CandidateAuthorityEvaluation::admissible(
                candidate.clone(),
                child_authority.evaluation.policy_revision,
                call.authority_context.grants_revision,
            )
        } else {
            CandidateAuthorityEvaluation::eliminated(
                candidate.clone(),
                child_elimination_reason(child_authority.evaluation.reason_code),
                child_authority.evaluation.policy_revision,
                call.authority_context.grants_revision,
            )
        };
        plans.push(LocalCandidatePlan {
            child,
            candidate,
            authority,
            child_authority,
        });
    }

    let remote_plans = resident_remote_candidate_plans_for_call(config, state, call, now.clone())?;
    let mut observations = resident_candidate_observations(call, &plans);
    observations.extend(resident_remote_candidate_observations(call, &remote_plans));
    let mut authority_evaluations = plans
        .iter()
        .map(|plan| plan.authority.clone())
        .collect::<Vec<_>>();
    authority_evaluations.extend(remote_plans.iter().map(|plan| plan.authority.clone()));
    let mut admissible = plans
        .into_iter()
        .filter(|plan| plan.authority.outcome == CandidateAuthorityOutcome::Admissible)
        .map(|plan| SelectedCandidate::Local(Box::new(plan)))
        .collect::<Vec<_>>();
    admissible.extend(
        remote_plans
            .into_iter()
            .filter(|plan| plan.authority.outcome == CandidateAuthorityOutcome::Admissible)
            .map(SelectedCandidate::Remote),
    );

    if admissible.is_empty() {
        let no_route_reason = authority_evaluations
            .iter()
            .find_map(|evaluation| evaluation.reason)
            .unwrap_or(CandidateEliminationReason::ChildNotApproved);
        let decision = RouteDecision::no_route(
            call,
            authority_evaluations,
            no_route_reason,
            resident_route_decision_ids("initial", call),
        );
        observations.push(route_decision_observation(
            call.trace_context.trace_id.clone(),
            current_timestamp(),
            &decision,
        ));
        return Ok(RouteDisposition::Denied {
            decision: Box::new(decision),
            observations,
        });
    }

    admissible.sort_by_key(|plan| resident_route_rank_key(plan.candidate()));
    match admissible.remove(0) {
        SelectedCandidate::Local(selected) => {
            let initial = RouteDecision::selected(
                call,
                selected.candidate.clone(),
                authority_evaluations,
                resident_route_decision_ids("initial", call),
            );
            observations.push(child_call_authority_observation(
                call.trace_context.trace_id.clone(),
                current_timestamp(),
                &selected.child_authority.evaluation,
            ));
            observations.push(route_decision_observation(
                call.trace_context.trace_id.clone(),
                current_timestamp(),
                &initial,
            ));

            let revalidated_child = projection.authorize_child_for_call(&selected.child.name, call);
            let child_authority_observation_id =
                revalidated_child.evaluation.observation_id.clone();
            observations.push(child_call_authority_observation(
                call.trace_context.trace_id.clone(),
                current_timestamp(),
                &revalidated_child.evaluation,
            ));
            let revalidation = revalidate_route_for_execution(
                call,
                &initial,
                revalidated_child,
                Vec::new(),
                resident_route_revalidation_ids(call),
            );
            observations.push(route_decision_observation(
                call.trace_context.trace_id.clone(),
                current_timestamp(),
                &revalidation.decision,
            ));

            let Some(authorized_route) = revalidation.authorized else {
                return Ok(RouteDisposition::Denied {
                    decision: Box::new(revalidation.decision),
                    observations,
                });
            };
            Ok(RouteDisposition::Local {
                plan: Box::new(LocalExecutionPlan {
                    child: selected.child,
                    authorized_route,
                    child_authority_observation_id,
                }),
                observations,
            })
        }
        SelectedCandidate::Remote(selected) => {
            let initial = RouteDecision::selected(
                call,
                selected.candidate.clone(),
                authority_evaluations,
                resident_route_decision_ids("initial", call),
            );
            observations.push(route_decision_observation(
                call.trace_context.trace_id.clone(),
                current_timestamp(),
                &initial,
            ));
            Ok(RouteDisposition::Remote {
                plan: Box::new(RemoteExecutionPlan::new(selected.candidate, initial)),
                observations,
            })
        }
    }
}

pub(super) fn resident_route_rank_key(candidate: &CandidateRoute) -> (u8, u8, String, String) {
    let network = match candidate.network_path {
        NetworkPathClass::Local => 0,
        NetworkPathClass::Direct => 1,
        NetworkPathClass::Relayed => 2,
        NetworkPathClass::Unknown => 3,
    };
    let runtime = match candidate.runtime_kind {
        RuntimeKind::WasmComponent => 0,
        RuntimeKind::Process => 1,
        RuntimeKind::JvmChild => 2,
        RuntimeKind::RemotePeer => 3,
        RuntimeKind::Internal => 4,
    };
    let child_id = candidate
        .child_id
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    (network, runtime, child_id, candidate.candidate_id.clone())
}

pub(super) fn resident_route_decision_ids(kind: &str, call: &MctCall) -> RouteDecisionIds {
    RouteDecisionIds {
        decision_id: DecisionId::new(format!("route-{kind}:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
        observation_id: ObservationId::new(format!("obs-route-{kind}:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
    }
}

pub(super) fn resident_route_revalidation_ids(call: &MctCall) -> RouteRevalidationIds {
    RouteRevalidationIds {
        decision_id: DecisionId::new(format!("route-revalidation:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
        observation_id: ObservationId::new(format!("obs-route-revalidation:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
        authorized_route_execution_id: AuthorizedRouteExecutionId::new(format!(
            "authorized-route:{}",
            call.call_id
        ))
        .expect("string ID literal/generated value must be non-empty"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mct_iroh::{endpoint_id_for_secret_key_hex, sign_peer_binding_signature_ref};

    struct DecisionFixture {
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
        let mut call = local_wasm_call(OperationTarget {
            namespace: "patina:demo".into(),
            interface_name: "control@0.1.0".into(),
            function_name: "run".into(),
        });
        call.call_id = CallId::new("call-resident-wit")
            .expect("string ID literal/generated value must be non-empty");
        call.trace_context.trace_id = trace_id;
        call.origin = CallOrigin::Iroh;
        call
    }
    fn resident_test_protocol_request(call: MctCall) -> MctCallProtocolRequest {
        MctCallProtocolRequest {
            protocol_request_id: ProtocolRequestId::new("proto-resident-wit")
                .expect("string ID literal/generated value must be non-empty"),
            authority: MctCallProtocolAuthority {
                hello_decision_id: DecisionId::new("decision-resident-wit-hello")
                    .expect("string ID literal/generated value must be non-empty"),
                peer_binding_id: PeerBindingId::new("binding-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                accepted_alpn: MCT_CALL_ALPN.into(),
                endpoint_id: EndpointIdText::new("endpoint-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                policy_revision: 1,
                grants_revision: 1,
            },
            received_over: IrohConnectionPresentation {
                endpoint_id: EndpointIdText::new("endpoint-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                alpn: MCT_CALL_ALPN.into(),
                connection_side: ConnectionSide::Incoming,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            call,
            payload: MctCallPayloadHandle::Empty,
            idempotency_key: None,
            received_observation_id: ObservationId::new("obs-resident-wit-received")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }
    fn decision_fixture() -> DecisionFixture {
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
        DecisionFixture {
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
    fn write_resident_process_child(children_dir: &Path) {
        write_resident_process_child_script(
            children_dir,
            "resident-echo",
            b"#!/bin/sh\ncat >/dev/null\nprintf '{\\\"ok\\\":true}'\n",
        );
    }
    fn write_resident_process_child_script(children_dir: &Path, name: &str, script: &[u8]) {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        let child_dir = children_dir.join(name);
        std::fs::create_dir_all(&child_dir).unwrap();
        let artifact_path = child_dir.join(format!("{name}.wasm"));
        let manifest_path = child_dir.join("child.toml");
        std::fs::write(&artifact_path, script).unwrap();
        #[cfg(unix)]
        {
            let mut permissions = std::fs::metadata(&artifact_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&artifact_path, permissions).unwrap();
        }
        write_resident_child_manifest(&manifest_path, name, "handle");
        write_sha256_sidecar(&artifact_path, script);
        let manifest_bytes = std::fs::read(&manifest_path).unwrap();
        write_sha256_sidecar(&manifest_path, &manifest_bytes);
    }
    fn write_resident_wit_child(children_dir: &Path) {
        let child_dir = children_dir.join("resident-wit");
        std::fs::create_dir_all(&child_dir).unwrap();
        let artifact_path = child_dir.join("resident-wit.wasm");
        let manifest_path = child_dir.join("child.toml");
        let component_wat = r#"
(component
  (core module $m
    (func $run (export "run") (result i32)
      i32.const 7))
  (core instance $i (instantiate $m))
  (func $run (result s32) (canon lift (core func $i "run")))
  (instance $control (export "run" (func $run)))
  (export "patina:demo/control@0.1.0" (instance $control)))
"#;
        let component = wat::parse_str(component_wat).unwrap();
        std::fs::write(&artifact_path, &component).unwrap();
        write_resident_child_manifest(&manifest_path, "resident-wit", "wit-only");
        write_sha256_sidecar(&artifact_path, &component);
        let manifest_bytes = std::fs::read(&manifest_path).unwrap();
        write_sha256_sidecar(&manifest_path, &manifest_bytes);
    }
    fn write_resident_child_manifest(manifest_path: &Path, name: &str, mode: &str) {
        std::fs::write(
            manifest_path,
            format!(
                r#"[child]
name = "{name}"
version = "0.1.0"
description = "resident test child"
kind = "child"
role = "app"

[child.ingress]
mode = "{mode}"

[child.artifact]
wasm = "{name}.wasm"

[child.contract]
allow = ["patina:demo/control@0.1.0.run"]

[needs]
toys = []

[relationships]
listens = []
"#
            ),
        )
        .unwrap();
    }
    fn write_sha256_sidecar(path: &Path, bytes: &[u8]) {
        use sha2::{Digest, Sha256};

        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(".sha256");
        std::fs::write(
            PathBuf::from(sidecar),
            format!("{:x}", Sha256::digest(bytes)),
        )
        .unwrap();
    }
    #[tokio::test]
    async fn resident_route_optimization_cannot_grant_authority() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_wit_child(&children_dir);
        write_resident_process_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        let process_child = loaded
            .children
            .iter()
            .find(|child| child.name == "resident-echo")
            .unwrap();
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(process_child, MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let trace_id = TraceId::new("trace-route-optimization-cannot-grant")
            .expect("string ID literal/generated value must be non-empty");
        let request = resident_test_protocol_request(resident_test_call(trace_id));

        let result = execute_resident_call(
            ResidentRuntimePaths::new(config_path, children_dir, state_path),
            ledger.clone(),
            request,
            ResidentPayloadIngress::remote(None),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Completed);
        assert!(matches!(
            result.route_taken,
            Some(RouteTaken {
                runtime_kind: RuntimeKind::Process,
                ..
            })
        ));
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("child:resident-wit"));
        assert!(ledger_text.contains("candidate_eliminated"));
        assert!(ledger_text.contains("ChildNotApproved"));
        assert!(ledger_text.contains("child:resident-echo"));
        assert!(ledger_text.contains("route_selected"));
    }

    #[tokio::test]
    async fn resident_no_route_records_specific_elimination() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_process_child(&children_dir);
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let trace_id = TraceId::new("trace-route-no-route-specific")
            .expect("string ID literal/generated value must be non-empty");
        let request = resident_test_protocol_request(resident_test_call(trace_id));

        let result = execute_resident_call(
            ResidentRuntimePaths::new(config_path, children_dir, state_path),
            ledger.clone(),
            request,
            ResidentPayloadIngress::remote(None),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Denied);
        assert_eq!(result.safe_message, "not authorized");
        assert!(result.route_taken.is_none());
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("candidate_eliminated"));
        assert!(ledger_text.contains("ChildNotApproved"));
        assert!(ledger_text.contains("no_route_recorded"));
    }

    #[test]
    fn forwarded_arrival_with_unavailable_local_candidate_is_terminal() {
        let fixture = decision_fixture();
        let mut unavailable_child = test_child();
        unavailable_child.instance_state = mct_daemon::MctChildInstanceState::Loading;
        let mut forwarded_call = fixture.call.clone();
        forwarded_call.origin = CallOrigin::Iroh;

        let outcome = authorize_resident_child_from_loaded_with_state(
            &fixture.config,
            Some(&fixture.state),
            vec![unavailable_child],
            &forwarded_call,
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
        )
        .unwrap();

        let RouteDisposition::Denied { observations, .. } = outcome else {
            panic!("forwarded arrival must be terminal when local execution is unavailable")
        };
        let text = serde_json::to_string(&observations).unwrap();
        assert!(text.contains("CapabilityUnavailable"));
        assert!(text.contains("denial_class:temporal"));
        assert!(!text.contains("peer:remote-mct"));
        assert!(!text.contains("peer_call_sent"));
    }
}

//! Resident admissible-only ranking, route decisions, and local kernel revalidation.

use super::*;

#[derive(Debug)]
pub(super) struct LocalExecutionPlan {
    child: mct_daemon::MctLoadedChild,
    local_runtime: LocalChildRuntime,
    authorized_route: AuthorizedRouteExecution,
    child_authority_observation_id: ObservationId,
}

impl LocalExecutionPlan {
    pub(super) fn into_parts(
        self,
    ) -> (
        mct_daemon::MctLoadedChild,
        LocalChildRuntime,
        AuthorizedRouteExecution,
        ObservationId,
    ) {
        (
            self.child,
            self.local_runtime,
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
    ledger_path: PathBuf,
    call: MctCall,
) -> Result<RouteDisposition> {
    tokio::task::spawn_blocking(move || {
        authorize_resident_child_blocking(&paths, &ledger_path, &call)
    })
    .await
    .context("join resident child authorization")?
}

pub(super) fn authorize_resident_child_blocking(
    paths: &ResidentRuntimePaths,
    ledger_path: &Path,
    call: &MctCall,
) -> Result<RouteDisposition> {
    let snapshot = mct_daemon::local_execution_authority_snapshot(
        ledger_path,
        paths.config_path(),
        paths.children_dir(),
        paths.state_path(),
    )
    .map_err(|deny| anyhow::anyhow!("local execution authority unavailable: {deny:?}"))?;
    let load_report =
        load_children_from_dir(MctChildLoadOptions::new(paths.children_dir().to_path_buf()));
    authorize_resident_child_from_snapshot(&snapshot, load_report.children, call)
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

#[cfg(test)]
pub(super) fn authorize_resident_child_from_loaded_with_state(
    config: &mct_daemon::MctDaemonConfig,
    state: Option<&MctRuntimeStateStore>,
    children: Vec<mct_daemon::MctLoadedChild>,
    call: &MctCall,
    now: Timestamp,
) -> Result<RouteDisposition> {
    let snapshot = resident_test_authority_snapshot(
        config,
        state,
        &children,
        now,
        call.authority_context
            .expected_receiver_grants_authority
            .generation,
    )?;
    authorize_resident_child_from_snapshot(&snapshot, children, call)
}

#[cfg(test)]
pub(super) fn resident_test_authority_snapshot(
    config: &mct_daemon::MctDaemonConfig,
    state: Option<&MctRuntimeStateStore>,
    children: &[mct_daemon::MctLoadedChild],
    evaluated_at: Timestamp,
    grants_generation: u64,
) -> Result<LocalExecutionAuthoritySnapshot> {
    let authority_state = state
        .and_then(|state| state.authority_projection_snapshot().ok().flatten())
        .map(|snapshot| snapshot.state)
        .unwrap_or_default();
    resident_test_authority_snapshot_with_state(
        config,
        state,
        children,
        evaluated_at,
        grants_generation,
        authority_state,
    )
}

#[cfg(test)]
fn resident_test_authority_snapshot_with_state(
    config: &mct_daemon::MctDaemonConfig,
    runtime_state: Option<&MctRuntimeStateStore>,
    children: &[mct_daemon::MctLoadedChild],
    evaluated_at: Timestamp,
    grants_generation: u64,
    authority_state: AuthorityStateV1,
) -> Result<LocalExecutionAuthoritySnapshot> {
    let identity = config
        .local_identity
        .as_ref()
        .context("test snapshot requires local identity")?;
    let child_projection = config.authority_projection_for_loaded_children(
        children.iter(),
        mct_daemon::MctOperatorChildScope {
            vision_id: identity.vision_id.clone(),
            node_id: identity.node_id.clone(),
            project_id: None,
            policy_revision: identity.policy_revision,
        },
    );
    let mut peer_records = Vec::new();
    let mut callable_surfaces = Vec::new();
    for peer in config.peers.values() {
        let outbound_binding = peer
            .outbound_binding
            .as_ref()
            .map(|outbound| mct_daemon::outbound_peer_binding_for_local(identity, peer, outbound))
            .transpose()?;
        peer_records.push(LocalPeerAuthorityRecordPartsV1 {
            peer_node_id: peer.peer_node_id.clone(),
            binding_id: peer.binding_id.clone(),
            endpoint_id: peer.endpoint_id.clone(),
            vision_id: peer.vision_id.clone(),
            binding_state: peer.binding_state,
            policy_revision: peer.policy_revision,
            expires_at: peer.expires_at.clone(),
            local_binding: peer.to_peer_binding(identity)?,
            binding_signature_ref: peer.binding_signature_ref.clone(),
            outbound_binding,
            outbound_signature_ref: peer
                .outbound_binding
                .as_ref()
                .map(|outbound| outbound.signature_ref.clone()),
            ticket_available: peer.ticket.is_some(),
            network_path: resident_peer_network_path(peer),
        });
        if let Some(state) = runtime_state {
            callable_surfaces.extend(
                state
                    .remote_callable_surfaces(&peer.peer_node_id, &peer.vision_id)?
                    .into_iter()
                    .map(|surface| LocalRemoteCallableSurfacePartsV1 {
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
                    }),
            );
        }
    }
    Ok(assemble_local_execution_authority_snapshot(
        LocalExecutionAuthoritySnapshotPartsV1 {
            executing_mother_node_id: identity.node_id.to_string(),
            grants_authority_mother_node_id: identity.node_id.to_string(),
            grants_authority_epoch: "mct-authority-epoch-v1:test".into(),
            grants_authority_generation: grants_generation,
            grants_authority_observation_id: "obs:test-authority".into(),
            toy_catalog: authority_state.toy_catalog.into_values().collect(),
            toy_grants: authority_state.toy_grants.into_values().collect(),
            watch_scopes: authority_state.watch_scopes.into_values().collect(),
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
            projection_id: "authority-state-v1".into(),
            projection_source_mother_node_id: identity.node_id.to_string(),
            projection_source_ledger_id: "ledger-local".into(),
            through_sequence: 0,
            through_observation_id: "obs:test-authority".into(),
            through_entry_hash: "test-entry-hash".into(),
            authority_state_hash: "test-state-hash".into(),
            projection_hash: "test-projection-hash".into(),
        },
    ))
}

fn resident_route_decision_observation(
    call: &MctCall,
    snapshot: &LocalExecutionAuthoritySnapshot,
    decision: &RouteDecision,
) -> MctObservation {
    let mut observation = route_decision_observation(
        call.trace_context.trace_id.clone(),
        current_timestamp(),
        decision,
    );
    let grants = snapshot.canonical_grants().grants_authority();
    let projection = snapshot.projection();
    observation.detail_ref = Some(format!(
        "route-authority-correlation-v1:{}",
        serde_json::json!({
            "route_detail_ref": observation.detail_ref,
            "caller_policy_revision_echo": call.authority_context.policy_revision,
            "caller_grants_revision_echo": call.authority_context.expected_receiver_grants_authority.generation,
            "caller_vision_policy_revision_echo": call.authority_context.vision_policy_revision,
            "local_policy_revision": snapshot.policy_revision(),
            "local_vision_policy_revision": snapshot.vision_policy_revision(),
            "grants_authority_mother_node_id": grants.mother_node_id(),
            "grants_authority_epoch": grants.authority_epoch(),
            "grants_authority_generation": grants.generation(),
            "projection_source_ledger_id": projection.source_ledger_id(),
            "projection_through_sequence": projection.through_sequence(),
            "projection_through_entry_hash": projection.through_entry_hash(),
        })
    ));
    observation
}

pub(super) fn authorize_resident_child_from_snapshot(
    snapshot: &LocalExecutionAuthoritySnapshot,
    children: Vec<mct_daemon::MctLoadedChild>,
    call: &MctCall,
) -> Result<RouteDisposition> {
    let child_policy = snapshot.child_policy();
    let projection = MctConfigChildAuthorityProjection {
        local_node_id: child_policy.local_node_id().clone(),
        vision_id: child_policy.vision_id().clone(),
        project_id: None,
        policy_revision: child_policy.policy_revision(),
        artifacts: child_policy.artifacts().to_vec(),
        approvals: child_policy.approvals().to_vec(),
        assignments: child_policy.assignments().to_vec(),
        instances: child_policy.instances().to_vec(),
    };
    let generation = snapshot.canonical_grants().grants_authority().generation();
    let mut plans = Vec::new();

    for child in children.into_iter().filter(|child| {
        resident_child_accepts_call(child, call)
            && child_policy
                .artifacts()
                .iter()
                .any(|artifact| artifact.artifact_id.as_str() == child.artifact_id)
    }) {
        let child_authority = projection.authorize_child_for_call_with_policy(
            &child.name,
            call,
            snapshot.policy_revision(),
        );
        let toy_authority = if child_authority.is_allowed() {
            resident_required_toy_authority(snapshot, &child, &child_authority, call)
        } else {
            Vec::new()
        };
        let (local_runtime, candidate) = resident_candidate_for_child(&projection, &child);
        let reason = if !child_authority.is_allowed() {
            Some(child_elimination_reason(
                child_authority.evaluation.reason_code,
            ))
        } else if toy_authority
            .iter()
            .any(|evaluation| evaluation.verdict != ToyGrantVerdict::Allowed)
        {
            Some(CandidateEliminationReason::ToyGrantMissing)
        } else {
            None
        };
        let authority = match reason {
            Some(reason) => CandidateAuthorityEvaluation::eliminated(
                candidate.clone(),
                reason,
                snapshot.policy_revision(),
                generation,
            ),
            None => CandidateAuthorityEvaluation::admissible(
                candidate.clone(),
                snapshot.policy_revision(),
                generation,
            ),
        };
        plans.push(LocalCandidatePlan {
            child,
            local_runtime,
            candidate,
            authority,
            child_authority,
            toy_authority,
        });
    }

    let remote_plans = resident_remote_candidate_plans_for_call(snapshot, call)?;
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
        observations.push(resident_route_decision_observation(
            call, snapshot, &decision,
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
            observations.push(resident_route_decision_observation(
                call, snapshot, &initial,
            ));

            let revalidated_child = projection.authorize_child_for_call_with_policy(
                &selected.child.name,
                call,
                snapshot.policy_revision(),
            );
            let child_authority_observation_id =
                revalidated_child.evaluation.observation_id.clone();
            observations.push(child_call_authority_observation(
                call.trace_context.trace_id.clone(),
                current_timestamp(),
                &revalidated_child.evaluation,
            ));
            let revalidated_toys = resident_required_toy_authority(
                snapshot,
                &selected.child,
                &revalidated_child,
                call,
            );
            observations.extend(revalidated_toys.iter().map(|evaluation| {
                toy_grant_evaluation_observation(
                    call.trace_context.trace_id.clone(),
                    current_timestamp(),
                    evaluation,
                )
            }));
            let revalidation = revalidate_route_for_execution_with_snapshot(
                call,
                &initial,
                revalidated_child,
                revalidated_toys,
                snapshot,
                resident_route_revalidation_ids(call),
            );
            observations.push(resident_route_decision_observation(
                call,
                snapshot,
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
                    local_runtime: selected.local_runtime,
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
            observations.push(resident_route_decision_observation(
                call, snapshot, &initial,
            ));
            Ok(RouteDisposition::Remote {
                plan: Box::new(RemoteExecutionPlan::new(selected.candidate, initial)),
                observations,
            })
        }
    }
}

fn resident_required_toy_authority(
    snapshot: &LocalExecutionAuthoritySnapshot,
    child: &mct_daemon::MctLoadedChild,
    child_authority: &ChildCallAuthorityResult,
    call: &MctCall,
) -> Vec<ToyGrantEvaluation> {
    let Some(authorized_child) = child_authority.authorized.as_ref() else {
        return Vec::new();
    };
    child
        .requested_toys
        .iter()
        .map(|label| {
            let matching_contracts = snapshot
                .canonical_grants()
                .toy_catalog()
                .iter()
                .filter(|contract| requested_toy_contract_matches(label, contract))
                .collect::<Vec<_>>();
            let selected_contract = matching_contracts.iter().copied().find(|contract| {
                snapshot
                    .canonical_grants()
                    .toy_grants()
                    .iter()
                    .any(|grant| {
                        grant.toy_id == contract.toy_id
                            && grant.subject.child_name == child.name
                            && grant.subject.artifact_id == child.artifact_id
                    })
            });
            let toy_id = selected_contract
                .or_else(|| matching_contracts.first().copied())
                .map(|contract| contract.toy_id.clone())
                .unwrap_or_else(|| {
                    ToyId::new(format!("toy:required:{label}"))
                        .expect("required Toy label is non-empty")
                });
            let matching_grant = snapshot
                .canonical_grants()
                .toy_grants()
                .iter()
                .find(|grant| {
                    grant.toy_id == toy_id
                        && grant.subject.child_name == child.name
                        && grant.subject.artifact_id == child.artifact_id
                });
            let action = matching_grant
                .and_then(|grant| grant.scope.allowed_actions.first())
                .cloned()
                .unwrap_or_else(|| "invoke".into());
            let resource_id = matching_grant.and_then(|grant| grant.scope.resource_id.clone());
            evaluate_toy_grant_for_route_snapshot(
                call,
                &ToyGrantEvaluationRequest {
                    toy_id,
                    subject: ToyGrantSubject {
                        child_name: child.name.clone(),
                        artifact_id: child.artifact_id.clone(),
                        artifact_version: child.version.clone(),
                        assignment_id: Some(authorized_child.assignment_id().clone()),
                        caller_node_id: Some(call.caller.node_id.clone()),
                    },
                    child_instance_id: authorized_child.child_instance_id().clone(),
                    action,
                    resource_id,
                    node_id: snapshot.child_policy().local_node_id().clone(),
                    now: snapshot.mother_clock().evaluated_at().clone(),
                    ids: ToyGrantEvaluationIds {
                        evaluation_id: ToyGrantEvaluationId::new(format!(
                            "toy-eval-route:{}:{label}",
                            call.call_id
                        ))
                        .expect("route Toy evaluation id is non-empty"),
                        decision_id: DecisionId::new(format!(
                            "decision-toy-route:{}:{label}",
                            call.call_id
                        ))
                        .expect("route Toy decision id is non-empty"),
                        observation_id: ObservationId::new(format!(
                            "obs-toy-route:{}:{label}",
                            call.call_id
                        ))
                        .expect("route Toy observation id is non-empty"),
                        authorized_toy_call_id: AuthorizedToyCallId::new(format!(
                            "unused-route-toy:{}:{label}",
                            call.call_id
                        ))
                        .expect("route-only Toy id is non-empty"),
                    },
                },
                snapshot.canonical_grants().toy_catalog(),
                snapshot.canonical_grants().toy_grants(),
                snapshot.policy_revision(),
                snapshot.canonical_grants().grants_authority().generation(),
            )
        })
        .collect()
}

fn requested_toy_contract_matches(label: &str, contract: &CanonicalToyContract) -> bool {
    match label {
        "logging" => contract.contract.interface_name.contains("logging"),
        "measure" => contract.contract.interface_name.contains("measure"),
        "git" => contract.contract.interface_name.contains("git"),
        "filesystem" => contract.contract.interface_name.contains("filesystem"),
        "keyvalue" => contract.contract.interface_name.contains("keyvalue"),
        // The bounded Watch observation grant is the authority behind the
        // watch actor's messaging host import; no ambient messaging grant exists.
        "messaging" => {
            contract.contract.interface_name.contains("messaging")
                || contract.toy_id.as_str() == MCT_WATCH_TOY_ID
        }
        other => contract.toy_id.as_str() == other,
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
    fn logging_authority_state(
        child: &mct_daemon::MctLoadedChild,
        grant_state: ToyGrantState,
        starts_at: Option<Timestamp>,
        expires_at: Option<Timestamp>,
    ) -> AuthorityStateV1 {
        let contract = CanonicalToyContract {
            toy_id: ToyId::new("toy:test:logging").unwrap(),
            contract: ToyContractIdentity {
                namespace: "wasi".into(),
                interface_name: "logging/logging".into(),
                version: "0.1.0".into(),
                function_name: Some("log".into()),
                resource_name: None,
            },
            authority_bearing: true,
            catalog_revision: 1,
            admitted_by_observation_id: ObservationId::new("obs:test:logging-catalog").unwrap(),
        };
        let grant = ToyGrant {
            grant_id: ToyGrantId::new("grant:test:logging").unwrap(),
            toy_id: contract.toy_id.clone(),
            subject: ToyGrantSubject {
                child_name: child.name.clone(),
                artifact_id: child.artifact_id.clone(),
                artifact_version: child.version.clone(),
                assignment_id: Some(
                    ChildAssignmentId::new(format!("assignment:{}", child.name)).unwrap(),
                ),
                caller_node_id: Some(MctNodeId::new("local-mct").unwrap()),
            },
            scope: ToyGrantScope {
                vision_id: VisionId::new("vision-local").unwrap(),
                node_id: Some(MctNodeId::new("local-mct").unwrap()),
                project_id: None,
                data_classification: None,
                resource_id: None,
                allowed_actions: vec!["log".into()],
            },
            constraints: ToyGrantConstraints {
                starts_at,
                expires_at,
                max_uses: None,
                max_duration_ms: None,
                locality_required: true,
            },
            grant_state,
            issuer_id: "local-mct".into(),
            policy_revision: 1,
            grants_revision: 1,
            authority_observation_id: ObservationId::new("obs:test:logging-grant").unwrap(),
        };
        AuthorityStateV1 {
            toy_catalog: [(contract.toy_id.to_string(), contract)].into(),
            toy_grants: [(grant.grant_id.to_string(), grant)].into(),
            watch_scopes: Default::default(),
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
                expected_receiver_grants_authority: test_grants_authority_identity(1),
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
    fn write_resident_wasm_child(children_dir: &Path) {
        write_test_wasm_child(children_dir, "resident-echo");
    }
    fn write_resident_wit_child(children_dir: &Path) {
        write_test_wasm_child(children_dir, "resident-wit");
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
        write_resident_wasm_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        let approved_child = loaded
            .children
            .iter()
            .find(|child| child.name == "resident-echo")
            .unwrap();
        let config_store = MctDaemonConfigStore::new(&config_path);
        config_store
            .ensure_local_identity(
                MctOperatorNodeScope::default(),
                dir.path().join("identity.hex"),
            )
            .unwrap();
        config_store
            .approve_and_assign_loaded_child(approved_child, MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn_authority_for_test(ledger_path.clone()).unwrap();
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
                runtime_kind: RuntimeKind::WasmComponent,
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
        write_resident_wasm_child(&children_dir);
        MctDaemonConfigStore::new(&config_path)
            .ensure_local_identity(
                MctOperatorNodeScope::default(),
                dir.path().join("identity.hex"),
            )
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn_authority_for_test(ledger_path.clone()).unwrap();
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

    /// Phase I proof 8: caller echoes are correlation evidence, never route authority.
    #[tokio::test]
    async fn caller_echo_matrix_keeps_route_decision_and_durably_records_each_echo() {
        let fixture = decision_fixture();
        let mut child = test_child();
        child.requested_toys = vec!["logging".into()];
        let snapshot = resident_test_authority_snapshot_with_state(
            &fixture.config,
            Some(&fixture.state),
            std::slice::from_ref(&child),
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
            41,
            logging_authority_state(&child, ToyGrantState::Active, None, None),
        )
        .unwrap();
        let echoes = [0, 41, 42, u64::MAX];
        let ledger_path = fixture._dir.path().join("echo-correlation.jsonl");
        let ledger = ResidentLedgerWriter::spawn_authority_for_test(ledger_path.clone()).unwrap();
        let mut dispositions = Vec::new();
        for echo in echoes {
            let mut call = fixture.call.clone();
            call.authority_context = AuthorityContextSnapshot {
                policy_revision: echo,
                expected_receiver_grants_authority: test_grants_authority_identity(echo),
                vision_policy_revision: echo,
            };
            let outcome =
                authorize_resident_child_from_snapshot(&snapshot, vec![child.clone()], &call)
                    .unwrap();
            let (disposition, observations) = match outcome {
                RouteDisposition::Local { observations, .. } => ("local", observations),
                RouteDisposition::Remote { observations, .. } => ("remote", observations),
                RouteDisposition::Denied { observations, .. } => ("denied", observations),
            };
            dispositions.push(disposition);
            ledger.append(observations).await.unwrap();
        }
        ledger.close().await;

        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        let recorded_echoes = entries
            .iter()
            .filter(|entry| entry.observation.kind == ObservationKind::RouteSelected)
            .map(|entry| {
                let detail = entry.observation.detail_ref.as_deref().unwrap();
                serde_json::from_str::<serde_json::Value>(
                    detail
                        .strip_prefix("route-authority-correlation-v1:")
                        .unwrap(),
                )
                .unwrap()["caller_grants_revision_echo"]
                    .as_u64()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(dispositions, vec!["local"; echoes.len()]);
        assert_eq!(recorded_echoes, echoes);
    }

    /// Phase I proof 9: a legacy echo need not equal canonical generation to admit.
    #[test]
    fn current_snapshot_admits_correct_call_when_legacy_echo_differs_from_generation() {
        let fixture = decision_fixture();
        let mut child = test_child();
        child.requested_toys = vec!["logging".into()];
        let snapshot = resident_test_authority_snapshot_with_state(
            &fixture.config,
            Some(&fixture.state),
            std::slice::from_ref(&child),
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
            73,
            logging_authority_state(&child, ToyGrantState::Active, None, None),
        )
        .unwrap();
        let mut call = fixture.call.clone();
        call.authority_context
            .expected_receiver_grants_authority
            .generation = 1;

        let outcome =
            authorize_resident_child_from_snapshot(&snapshot, vec![child], &call).unwrap();
        assert!(
            matches!(outcome, RouteDisposition::Local { .. }),
            "canonical generation 73 must admit independently of caller echo 1"
        );
    }

    /// Phase I proof 12: only the captured Mother clock controls grant windows.
    #[test]
    fn route_grant_window_uses_snapshot_mother_clock_not_caller_deadline_or_echo() {
        let fixture = decision_fixture();
        let mut child = test_child();
        child.requested_toys = vec!["logging".into()];
        let starts_at = Timestamp::new("2026-07-09T12:00:00Z").unwrap();
        let expires_at = Timestamp::new("2026-07-09T13:00:00Z").unwrap();
        let authority_state = logging_authority_state(
            &child,
            ToyGrantState::Active,
            Some(starts_at),
            Some(expires_at),
        );
        let mut call = fixture.call.clone();
        call.authority_context = AuthorityContextSnapshot {
            policy_revision: u64::MAX,
            expected_receiver_grants_authority: test_grants_authority_identity(u64::MAX),
            vision_policy_revision: u64::MAX,
        };
        call.deadline = Timestamp::new("2099-01-01T00:00:00Z").unwrap();
        let mut outcomes = Vec::new();
        for evaluated_at in [
            "2026-07-09T11:59:59Z",
            "2026-07-09T12:00:00Z",
            "2026-07-09T13:00:00Z",
        ] {
            let snapshot = resident_test_authority_snapshot_with_state(
                &fixture.config,
                Some(&fixture.state),
                std::slice::from_ref(&child),
                Timestamp::new(evaluated_at).unwrap(),
                9,
                authority_state.clone(),
            )
            .unwrap();
            outcomes.push(
                match authorize_resident_child_from_snapshot(&snapshot, vec![child.clone()], &call)
                    .unwrap()
                {
                    RouteDisposition::Local { .. } => "allowed",
                    RouteDisposition::Denied { .. } => "denied",
                    RouteDisposition::Remote { .. } => "remote",
                },
            );
        }
        assert_eq!(outcomes, ["denied", "allowed", "denied"]);
    }

    /// Phase I proof 5: canonical revocation plus catch-up denies the next route.
    #[test]
    fn enveloped_required_toy_revocation_denies_next_route_regardless_of_echo() {
        let dir = tempfile::tempdir().unwrap();
        let ledger_path = dir.path().join("observations.jsonl");
        let config_path = dir.path().join("config.json");
        let state_path = dir.path().join("state.sqlite");
        let children_dir = dir.path().join("children");
        write_resident_wasm_child(&children_dir);
        let manifest_path = children_dir.join("resident-echo").join("child.toml");
        let manifest = std::fs::read_to_string(&manifest_path)
            .unwrap()
            .replace("toys = []", "toys = [\"logging\"]");
        std::fs::write(&manifest_path, manifest.as_bytes()).unwrap();
        write_sha256_sidecar(&manifest_path, manifest.as_bytes());
        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        let child = loaded.children[0].clone();
        let store = MctDaemonConfigStore::new(&config_path);
        store
            .ensure_local_identity(
                MctOperatorNodeScope::default(),
                dir.path().join("identity.hex"),
            )
            .unwrap();
        store
            .approve_and_assign_loaded_child(&child, MctOperatorChildScope::default())
            .unwrap();
        let canonical_state = logging_authority_state(&child, ToyGrantState::Active, None, None);
        let mut ledger =
            JsonlObservationLedger::open_authority(&ledger_path, "ledger-local", "local-mct")
                .unwrap();
        let import = ledger.execute_legacy_authority_import(
            LegacyAuthorityImportRequestV1 {
                schema: "mct-legacy-authority-import-request/v1".into(),
                import_id: "route-revocation-import".into(),
                expected_mother_node_id: "local-mct".into(),
                expected_ledger_id: "ledger-local".into(),
                expected_config_authority_hash: authority_state_hash(&AuthorityStateV1::default())
                    .unwrap(),
                expected_sqlite_authority_hash: authority_state_hash(&canonical_state).unwrap(),
                confirmation: mct_observation::LEGACY_AUTHORITY_IMPORT_CONFIRMATION_V1.into(),
            },
            "os-uid:501".into(),
            canonical_state.clone(),
            "2026-07-09T00:00:00Z".into(),
        );
        assert!(matches!(
            import,
            AuthorityMutationResultV1::CommittedProjectionPending { .. }
        ));
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        state
            .publish_authority_projection(&ledger.entries().unwrap())
            .unwrap();
        let snapshot_before = mct_daemon::local_execution_authority_snapshot_at(
            &ledger_path,
            &config_path,
            &children_dir,
            &state_path,
            Ok(Timestamp::new("2026-07-09T00:01:00Z").unwrap()),
        )
        .unwrap();
        let mut call = resident_test_call(TraceId::new("trace-envelope-revocation").unwrap());
        call.authority_context
            .expected_receiver_grants_authority
            .generation = u64::MAX;
        assert!(matches!(
            authorize_resident_child_from_snapshot(&snapshot_before, vec![child.clone()], &call,)
                .unwrap(),
            RouteDisposition::Local { .. }
        ));

        let active_grant = canonical_state.toy_grants.values().next().unwrap();
        let mutation = ledger.execute_authority_mutation(
            AuthorityMutationRequestV1 {
                mutation_id: "route-required-toy-revoke".into(),
                changes: vec![AuthorityChangeV1::ToyGrantPut {
                    grant_id: active_grant.grant_id.to_string(),
                    toy_id: active_grant.toy_id.to_string(),
                    subject: Box::new(active_grant.subject.clone()),
                    scope: Box::new(active_grant.scope.clone()),
                    constraints: Box::new(active_grant.constraints.clone()),
                    grant_state: ToyGrantState::Revoked,
                    issuer_id: active_grant.issuer_id.clone(),
                    policy_revision: active_grant.policy_revision,
                    source_grants_revision: active_grant.grants_revision + 1,
                    authority_observation_id: "obs:test:logging-revoked".into(),
                }],
                grant_shaping_sources: vec![GrantShapingSourceV1::OperatorDecision {
                    decision_id: "decision-route-required-toy-revoke".into(),
                    authenticated_principal_ref: "os-uid:501".into(),
                    command_kind: GrantShapingCommandKindV1::GrantChange,
                }],
                decided_at: "2026-07-09T00:02:00Z".into(),
            },
            |_| Ok(None),
        );
        assert!(matches!(
            mutation,
            AuthorityMutationResultV1::CommittedProjectionPending { .. }
        ));
        state
            .rebuild_authority_projection(&ledger.entries().unwrap())
            .unwrap();
        let snapshot_after = mct_daemon::local_execution_authority_snapshot_at(
            &ledger_path,
            &config_path,
            &children_dir,
            &state_path,
            Ok(Timestamp::new("2026-07-09T00:03:00Z").unwrap()),
        )
        .unwrap();
        assert!(matches!(
            authorize_resident_child_from_snapshot(&snapshot_after, vec![child], &call).unwrap(),
            RouteDisposition::Denied { .. }
        ));
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

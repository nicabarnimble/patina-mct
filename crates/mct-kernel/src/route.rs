use crate::{call::*, child::*, id::*, toy::*};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Coarse path class used when comparing candidate routes.
pub enum NetworkPathClass {
    /// Peer is reachable directly.
    Direct,
    /// Peer route traverses a relay.
    Relayed,
    /// Work stays on the local Mother.
    Local,
    /// Path class is not known to the adapter.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Possible execution path before authority filtering.
///
/// A candidate is not executable authority; it must appear as admissible in a
/// route decision and pass revalidation immediately before execution.
pub struct CandidateRoute {
    /// Planner-local identifier for this candidate.
    pub candidate_id: String,
    /// Node that would execute or receive the call.
    pub node_id: MctNodeId,
    /// Child selected by the route, when child execution is required.
    pub child_id: Option<ChildId>,
    /// Runtime class for the candidate execution path.
    pub runtime_kind: RuntimeKind,
    /// Network locality class for route planning and audit.
    pub network_path: NetworkPathClass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Authority filtering outcome for one candidate route.
pub enum CandidateAuthorityOutcome {
    /// Candidate survived authority filtering.
    Admissible,
    /// Candidate was removed from the feasible set.
    Eliminated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Typed reason a candidate route was removed from consideration.
pub enum CandidateEliminationReason {
    /// Data classification or placement policy denied the route.
    DataPolicyDenied,
    /// Vision policy denied the route.
    VisionPolicyDenied,
    /// Remote peer was not admitted for this call.
    PeerNotAdmitted,
    /// Child approval or assignment authority was absent.
    ChildNotApproved,
    /// Required toy grant was absent or denied.
    ToyGrantMissing,
    /// Secret-scoped payload was forbidden for this route.
    SecretScopeForbidden,
    /// Policy revision did not match the call authority snapshot.
    PolicyRevisionStale,
    /// Grants revision did not match the call authority snapshot.
    GrantsRevisionStale,
    /// Revalidation facts did not match the selected route.
    RouteMismatch,
    /// Required runtime or route capability was unavailable.
    CapabilityUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Whether a route decision is initial planning or execution-time revalidation.
pub enum RouteDecisionKind {
    /// Initial two-phase route selection decision.
    Initial,
    /// Decision made immediately before execution.
    Revalidation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Result reason for execution-time route revalidation.
pub enum RouteRevalidationReason {
    /// Selected route and all execution authorities still match.
    Revalidated,
    /// Initial decision had no selected route.
    InitialDecisionNotSelected,
    /// Initial decision was for a different call.
    CallIdMismatch,
    /// Selected route was not recorded as admissible in the initial decision.
    SelectedRouteNotAdmissible,
    /// Authorized child invocation did not match the selected child.
    SelectedChildMismatch,
    /// Child authority token was absent or denied.
    ChildAuthorityDenied,
    /// At least one required toy grant was absent or denied.
    ToyGrantDenied,
    /// Policy revision did not match the call authority snapshot.
    PolicyRevisionStale,
    /// Grants revision did not match the call authority snapshot.
    GrantsRevisionStale,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Authority result for one route candidate at one revision pair.
pub struct CandidateAuthorityEvaluation {
    /// Candidate route being judged.
    pub candidate: CandidateRoute,
    /// Whether the candidate remains feasible.
    pub outcome: CandidateAuthorityOutcome,
    /// Elimination reason, present only for eliminated candidates.
    pub reason: Option<CandidateEliminationReason>,
    /// Caller-safe/operator-safe summary for projections.
    pub safe_message: String,
    /// Policy revision used for the candidate judgment.
    pub policy_revision: u64,
    /// Grants revision used for toy-related candidate judgment.
    pub grants_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Final outcome of route selection or revalidation.
pub enum RouteDecisionOutcome {
    /// A route was selected and remains eligible.
    RouteSelected,
    /// No route may be executed.
    NoRoute,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Two-phase routing authority decision for an MCT call.
///
/// Optimization may select only among candidates represented by admissible
/// authority evaluations. `NoRoute` is the fail-closed default when no
/// candidate remains or revalidation fails.
pub struct RouteDecision {
    /// Unique decision identifier.
    pub decision_id: DecisionId,
    /// Call whose route is being selected or revalidated.
    pub call_id: CallId,
    /// Initial planning or revalidation phase.
    pub decision_kind: RouteDecisionKind,
    /// Initial decision referenced by revalidation decisions.
    pub initial_decision_id: Option<DecisionId>,
    /// Per-candidate authority evidence used by this decision.
    pub authority_evaluations: Vec<CandidateAuthorityEvaluation>,
    /// Selected route, present only when outcome is `RouteSelected`.
    pub selected_route: Option<CandidateRoute>,
    /// Route decision outcome.
    pub outcome: RouteDecisionOutcome,
    /// Reason no route exists, present only for no-route decisions.
    pub no_route_reason: Option<CandidateEliminationReason>,
    /// Caller-safe message for result projection.
    pub safe_message: String,
    /// Observation recording this route decision.
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Identifiers assigned to an initial route decision.
pub struct RouteDecisionIds {
    /// Decision identifier to stamp on the route decision.
    pub decision_id: DecisionId,
    /// Observation identifier to stamp on route evidence.
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Identifiers assigned during execution-time route revalidation.
pub struct RouteRevalidationIds {
    /// Decision identifier to stamp on the route decision.
    pub decision_id: DecisionId,
    /// Observation identifier to stamp on route evidence.
    pub observation_id: ObservationId,
    /// Token identifier minted only when revalidation succeeds.
    pub authorized_route_execution_id: AuthorizedRouteExecutionId,
}

#[derive(Debug, PartialEq, Eq)]
/// Capability token proving a selected route passed execution-time revalidation.
///
/// Adapters should execute only when this record is present in a successful
/// revalidation result.
pub struct AuthorizedRouteExecution {
    /// Unique identifier for this execution authorization.
    pub authorized_route_execution_id: AuthorizedRouteExecutionId,
    /// Call authorized for execution.
    pub call_id: CallId,
    /// Initial route decision being revalidated.
    pub initial_decision_id: DecisionId,
    /// Revalidation decision that minted this token.
    pub revalidation_decision_id: DecisionId,
    /// Route authorized for execution.
    pub route: CandidateRoute,
    /// Child invocation token for the selected child.
    pub child_invocation: AuthorizedChildInvocation,
    /// Toy call tokens that survived revalidation.
    pub toy_calls: Vec<AuthorizedToyCall>,
}

#[derive(Debug, PartialEq, Eq)]
/// Result of checking selected route authority immediately before execution.
pub struct RouteRevalidationResult {
    /// Revalidation route decision, selected or no-route.
    pub decision: RouteDecision,
    /// Typed reason for authorization or denial.
    pub reason: RouteRevalidationReason,
    /// Execution authorization token, present only when revalidated.
    pub authorized: Option<AuthorizedRouteExecution>,
}

impl RouteRevalidationResult {
    /// Returns true only when revalidation minted an execution token.
    pub fn is_authorized(&self) -> bool {
        self.reason == RouteRevalidationReason::Revalidated && self.authorized.is_some()
    }
}

impl CandidateAuthorityEvaluation {
    /// Builds an admissible candidate evaluation at the supplied revisions.
    pub fn admissible(
        candidate: CandidateRoute,
        policy_revision: u64,
        grants_revision: u64,
    ) -> Self {
        Self {
            candidate,
            outcome: CandidateAuthorityOutcome::Admissible,
            reason: None,
            safe_message: "admissible".into(),
            policy_revision,
            grants_revision,
        }
    }

    /// Builds an eliminated candidate evaluation with a non-secret safe message.
    pub fn eliminated(
        candidate: CandidateRoute,
        reason: CandidateEliminationReason,
        policy_revision: u64,
        grants_revision: u64,
    ) -> Self {
        Self {
            candidate,
            outcome: CandidateAuthorityOutcome::Eliminated,
            reason: Some(reason),
            safe_message: "not authorized".into(),
            policy_revision,
            grants_revision,
        }
    }
}

impl RouteDecision {
    /// Builds an initial decision selecting one route from authority evaluations.
    pub fn selected(
        call: &MctCall,
        selected_route: CandidateRoute,
        authority_evaluations: Vec<CandidateAuthorityEvaluation>,
        ids: RouteDecisionIds,
    ) -> Self {
        Self {
            decision_id: ids.decision_id,
            call_id: call.call_id.clone(),
            decision_kind: RouteDecisionKind::Initial,
            initial_decision_id: None,
            authority_evaluations,
            selected_route: Some(selected_route),
            outcome: RouteDecisionOutcome::RouteSelected,
            no_route_reason: None,
            safe_message: "route selected".into(),
            observation_id: ids.observation_id,
        }
    }

    /// Builds an initial fail-closed no-route decision.
    pub fn no_route(
        call: &MctCall,
        authority_evaluations: Vec<CandidateAuthorityEvaluation>,
        no_route_reason: CandidateEliminationReason,
        ids: RouteDecisionIds,
    ) -> Self {
        Self {
            decision_id: ids.decision_id,
            call_id: call.call_id.clone(),
            decision_kind: RouteDecisionKind::Initial,
            initial_decision_id: None,
            authority_evaluations,
            selected_route: None,
            outcome: RouteDecisionOutcome::NoRoute,
            no_route_reason: Some(no_route_reason),
            safe_message: "not authorized".into(),
            observation_id: ids.observation_id,
        }
    }

    /// Returns true when this decision denies execution because no route remains.
    pub fn is_no_route(&self) -> bool {
        self.outcome == RouteDecisionOutcome::NoRoute
    }
}

/// Rechecks route, child, and toy authority immediately before execution.
///
/// Authority facts are the original call, initial route decision, child
/// authority result, toy grant results, and caller-supplied IDs. It returns an
/// execution token only when the initial decision selected an admissible route,
/// the child invocation matches the selected child and call, all revisions match
/// the call authority snapshot, and every toy grant is allowed. Any mismatch is
/// a no-route decision with no token.
pub fn revalidate_route_for_execution(
    call: &MctCall,
    initial: &RouteDecision,
    child: ChildCallAuthorityResult,
    toys: &[ToyGrantEvaluationResult],
    ids: RouteRevalidationIds,
) -> RouteRevalidationResult {
    if initial.call_id != call.call_id {
        return revalidation_denied(
            call,
            initial,
            None,
            ids,
            RouteRevalidationReason::CallIdMismatch,
            CandidateEliminationReason::RouteMismatch,
        );
    }

    let Some(selected_route) = initial.selected_route.as_ref() else {
        return revalidation_denied(
            call,
            initial,
            None,
            ids,
            RouteRevalidationReason::InitialDecisionNotSelected,
            CandidateEliminationReason::CapabilityUnavailable,
        );
    };

    if initial.outcome != RouteDecisionOutcome::RouteSelected
        || !initial_route_admitted_candidate(initial, selected_route)
    {
        return revalidation_denied(
            call,
            initial,
            Some(selected_route.clone()),
            ids,
            RouteRevalidationReason::SelectedRouteNotAdmissible,
            CandidateEliminationReason::CapabilityUnavailable,
        );
    }

    if child.evaluation.policy_revision != call.authority_context.policy_revision {
        return revalidation_denied(
            call,
            initial,
            Some(selected_route.clone()),
            ids,
            RouteRevalidationReason::PolicyRevisionStale,
            CandidateEliminationReason::PolicyRevisionStale,
        );
    }

    if child.evaluation.call_id != call.call_id || !child.is_allowed() {
        return revalidation_denied(
            call,
            initial,
            Some(selected_route.clone()),
            ids,
            RouteRevalidationReason::ChildAuthorityDenied,
            CandidateEliminationReason::ChildNotApproved,
        );
    }

    let child_invocation = child
        .authorized
        .expect("allowed child authority has a token");

    if let Some(child_id) = selected_route.child_id.as_ref()
        && child_id.as_str() != child_invocation.child_name()
    {
        return revalidation_denied(
            call,
            initial,
            Some(selected_route.clone()),
            ids,
            RouteRevalidationReason::SelectedChildMismatch,
            CandidateEliminationReason::RouteMismatch,
        );
    }

    let mut authorized_toys = Vec::with_capacity(toys.len());
    for toy in toys {
        if toy.evaluation.policy_revision != call.authority_context.policy_revision {
            return revalidation_denied(
                call,
                initial,
                Some(selected_route.clone()),
                ids,
                RouteRevalidationReason::PolicyRevisionStale,
                CandidateEliminationReason::PolicyRevisionStale,
            );
        }
        if toy.evaluation.grants_revision != call.authority_context.grants_revision {
            return revalidation_denied(
                call,
                initial,
                Some(selected_route.clone()),
                ids,
                RouteRevalidationReason::GrantsRevisionStale,
                CandidateEliminationReason::GrantsRevisionStale,
            );
        }
        let Some(authorized_toy) = toy.authorized.as_ref() else {
            return revalidation_denied(
                call,
                initial,
                Some(selected_route.clone()),
                ids,
                RouteRevalidationReason::ToyGrantDenied,
                CandidateEliminationReason::ToyGrantMissing,
            );
        };
        if toy.evaluation.call_id != call.call_id || !toy.is_allowed() {
            return revalidation_denied(
                call,
                initial,
                Some(selected_route.clone()),
                ids,
                RouteRevalidationReason::ToyGrantDenied,
                CandidateEliminationReason::ToyGrantMissing,
            );
        }
        authorized_toys.push(authorized_toy.clone());
    }

    let decision_id = ids.decision_id.clone();
    let decision = RouteDecision {
        decision_id: ids.decision_id,
        call_id: call.call_id.clone(),
        decision_kind: RouteDecisionKind::Revalidation,
        initial_decision_id: Some(initial.decision_id.clone()),
        authority_evaluations: vec![CandidateAuthorityEvaluation::admissible(
            selected_route.clone(),
            call.authority_context.policy_revision,
            call.authority_context.grants_revision,
        )],
        selected_route: Some(selected_route.clone()),
        outcome: RouteDecisionOutcome::RouteSelected,
        no_route_reason: None,
        safe_message: "route revalidated".into(),
        observation_id: ids.observation_id,
    };
    let authorized = AuthorizedRouteExecution {
        authorized_route_execution_id: ids.authorized_route_execution_id,
        call_id: call.call_id.clone(),
        initial_decision_id: initial.decision_id.clone(),
        revalidation_decision_id: decision_id,
        route: selected_route.clone(),
        child_invocation,
        toy_calls: authorized_toys,
    };

    RouteRevalidationResult {
        decision,
        reason: RouteRevalidationReason::Revalidated,
        authorized: Some(authorized),
    }
}

fn initial_route_admitted_candidate(
    initial: &RouteDecision,
    selected_route: &CandidateRoute,
) -> bool {
    initial.authority_evaluations.iter().any(|evaluation| {
        evaluation.candidate == *selected_route
            && evaluation.outcome == CandidateAuthorityOutcome::Admissible
    })
}

fn revalidation_denied(
    call: &MctCall,
    initial: &RouteDecision,
    selected_route: Option<CandidateRoute>,
    ids: RouteRevalidationIds,
    reason: RouteRevalidationReason,
    elimination_reason: CandidateEliminationReason,
) -> RouteRevalidationResult {
    let authority_evaluations = selected_route
        .clone()
        .map(|candidate| {
            vec![CandidateAuthorityEvaluation::eliminated(
                candidate,
                elimination_reason,
                call.authority_context.policy_revision,
                call.authority_context.grants_revision,
            )]
        })
        .unwrap_or_default();

    RouteRevalidationResult {
        decision: RouteDecision {
            decision_id: ids.decision_id,
            call_id: call.call_id.clone(),
            decision_kind: RouteDecisionKind::Revalidation,
            initial_decision_id: Some(initial.decision_id.clone()),
            authority_evaluations,
            selected_route: None,
            outcome: RouteDecisionOutcome::NoRoute,
            no_route_reason: Some(elimination_reason),
            safe_message: "not authorized".into(),
            observation_id: ids.observation_id,
        },
        reason,
        authorized: None,
    }
}

/// Projects a no-route decision into a caller-safe denied result.
///
/// The result contains no route and preserves the decision ID for audit lookup.
pub fn no_route_denied_result(
    call: &MctCall,
    decision: &RouteDecision,
    audit_ref: AuditRef,
) -> MctResult {
    MctResult {
        call_id: call.call_id.clone(),
        outcome: ResultOutcome::Denied,
        route_taken: None,
        authority_decision_ref: decision.decision_id.clone(),
        execution_summary: ExecutionSummary {
            wall_time_ms: 0,
            execution_time_ms: None,
            queue_wait_ms: None,
            input_size_bytes: call.payload_metadata.approximate_size_bytes,
            output_size_bytes: None,
        },
        requester_message: decision.safe_message.clone(),
        audit_ref,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call() -> MctCall {
        MctCall {
            call_id: CallId::new("call-route-1")
                .expect("string ID literal/generated value must be non-empty"),
            caller: CallerIdentity {
                node_id: MctNodeId::new("node-a")
                    .expect("string ID literal/generated value must be non-empty"),
                user_id: None,
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                project_id: None,
            },
            target: OperationTarget {
                namespace: "patina".into(),
                interface_name: "echo".into(),
                function_name: "echo".into(),
            },
            payload_metadata: PayloadMetadata {
                data_classification: "public".into(),
                approximate_size_bytes: 5,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 1,
                grants_revision: 1,
                vision_policy_revision: 1,
            },
            deadline: Timestamp::new("2026-05-31T00:01:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-route-1")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: SpanId::new("span-route-1")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            origin: CallOrigin::Cli,
        }
    }

    fn candidate(id: &str, runtime_kind: RuntimeKind) -> CandidateRoute {
        CandidateRoute {
            candidate_id: id.into(),
            node_id: MctNodeId::new("node-b")
                .expect("string ID literal/generated value must be non-empty"),
            child_id: Some(
                ChildId::new("child-echo")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            runtime_kind,
            network_path: NetworkPathClass::Local,
        }
    }

    fn route_ids(decision: &str, observation: &str) -> RouteDecisionIds {
        RouteDecisionIds {
            decision_id: DecisionId::new(decision)
                .expect("string ID literal/generated value must be non-empty"),
            observation_id: ObservationId::new(observation)
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn revalidation_ids() -> RouteRevalidationIds {
        RouteRevalidationIds {
            decision_id: DecisionId::new("route-revalidation-1")
                .expect("string ID literal/generated value must be non-empty"),
            observation_id: ObservationId::new("obs-route-revalidation-1")
                .expect("string ID literal/generated value must be non-empty"),
            authorized_route_execution_id: AuthorizedRouteExecutionId::new("authorized-route-1")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn initial_selected_route(selected: CandidateRoute) -> RouteDecision {
        RouteDecision::selected(
            &call(),
            selected.clone(),
            vec![CandidateAuthorityEvaluation::admissible(selected, 1, 1)],
            route_ids("route-initial-1", "obs-route-initial-1"),
        )
    }

    fn child_result(
        policy_revision: u64,
        allowed: bool,
        child_name: &str,
    ) -> ChildCallAuthorityResult {
        let mut authority_call = call();
        authority_call.authority_context.policy_revision = policy_revision;
        let artifact_id = ComponentArtifactId::new("artifact-route-1")
            .expect("string ID literal/generated value must be non-empty");
        let approval_id = ChildApprovalId::new("approval-route-1")
            .expect("string ID literal/generated value must be non-empty");
        let assignment_id = ChildAssignmentId::new("assignment-route-1")
            .expect("string ID literal/generated value must be non-empty");
        let instance_id = ChildInstanceId::new("child-instance-route-1")
            .expect("string ID literal/generated value must be non-empty");
        let request = ChildCallAuthorityRequest {
            instance_id: instance_id.clone(),
            node_id: MctNodeId::new("node-b")
                .expect("string ID literal/generated value must be non-empty"),
            ids: ChildCallAuthorityIds {
                evaluation_id: ChildCallEvaluationId::new("child-eval-route-1")
                    .expect("string ID literal/generated value must be non-empty"),
                decision_id: DecisionId::new("child-decision-route-1")
                    .expect("string ID literal/generated value must be non-empty"),
                observation_id: ObservationId::new("obs-child-route-1")
                    .expect("string ID literal/generated value must be non-empty"),
                authorized_child_invocation_id: AuthorizedChildInvocationId::new(
                    "authorized-child-route-1",
                )
                .expect("string ID literal/generated value must be non-empty"),
            },
        };
        let artifact = ComponentArtifact {
            artifact_id: artifact_id.clone(),
            child_name: child_name.into(),
            artifact_version: "0.1.0".into(),
            content_hash: "sha256:route".into(),
            manifest_hash: "sha256:route-manifest".into(),
            primary_export: ComponentWitExport {
                namespace: authority_call.target.namespace.clone(),
                interface_name: authority_call.target.interface_name.clone(),
                version: "0.1.0".into(),
                function_names: vec![authority_call.target.function_name.clone()],
            },
            runtime_shape: ComponentRuntimeShape::WasmComponent,
            ingress_mode: ChildIngressMode::WitOnly,
            lifecycle_exports: LifecycleExports::AbsentAllowed,
            verification_status: VerificationStatus::Verified,
            created_by_observation_id: ObservationId::new("obs-artifact-route-1")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let approval = ChildApproval {
            approval_id: approval_id.clone(),
            artifact_id: artifact_id.clone(),
            child_name: child_name.into(),
            artifact_version: "0.1.0".into(),
            scope_vision_id: Some(authority_call.caller.vision_id.clone()),
            scope_node_id: Some(request.node_id.clone()),
            scope_project_id: authority_call.caller.project_id.clone(),
            approval_state: ChildApprovalState::Approved,
            policy_revision,
            authority_observation_id: ObservationId::new("obs-approval-route-1")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let assignment = ChildAssignment {
            assignment_id: assignment_id.clone(),
            approval_id,
            artifact_id: artifact_id.clone(),
            child_name: child_name.into(),
            vision_id: authority_call.caller.vision_id.clone(),
            node_id: Some(request.node_id.clone()),
            project_id: authority_call.caller.project_id.clone(),
            assignment_state: if allowed {
                ChildAssignmentState::Active
            } else {
                ChildAssignmentState::Revoked
            },
            pinned_artifact_version: "0.1.0".into(),
            assignment_observation_id: ObservationId::new("obs-assignment-route-1")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let instance = ChildInstance {
            instance_id,
            assignment_id,
            artifact_id,
            child_name: child_name.into(),
            generation: 1,
            node_id: request.node_id.clone(),
            instance_state: ChildInstanceState::Ready,
            readiness_observation_id: Some(
                ObservationId::new("obs-ready-route-1")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            last_lifecycle_observation_id: ObservationId::new("obs-lifecycle-route-1")
                .expect("string ID literal/generated value must be non-empty"),
        };

        let result = evaluate_child_call_authority(
            &authority_call,
            &request,
            &[artifact],
            &[approval],
            &[assignment],
            &[instance],
        );
        assert_eq!(result.is_allowed(), allowed);
        result
    }

    fn toy_result(
        policy_revision: u64,
        grants_revision: u64,
        allowed: bool,
    ) -> ToyGrantEvaluationResult {
        let evaluation = ToyGrantEvaluation {
            evaluation_id: ToyGrantEvaluationId::new("toy-eval-route-1")
                .expect("string ID literal/generated value must be non-empty"),
            call_id: CallId::new("call-route-1")
                .expect("string ID literal/generated value must be non-empty"),
            decision_id: DecisionId::new("toy-decision-route-1")
                .expect("string ID literal/generated value must be non-empty"),
            grant_id: allowed.then(|| {
                ToyGrantId::new("toy-grant-route-1")
                    .expect("string ID literal/generated value must be non-empty")
            }),
            toy_id: ToyId::new("toy-echo")
                .expect("string ID literal/generated value must be non-empty"),
            subject_child_name: "child-echo".into(),
            verdict: if allowed {
                ToyGrantVerdict::Allowed
            } else {
                ToyGrantVerdict::Denied
            },
            reason_code: if allowed {
                ToyGrantReasonCode::ActiveGrant
            } else {
                ToyGrantReasonCode::RevokedGrant
            },
            policy_revision,
            grants_revision,
            observation_id: ObservationId::new("obs-toy-route-1")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let authorized = allowed.then(|| AuthorizedToyCall {
            authorized_toy_call_id: AuthorizedToyCallId::new("authorized-toy-route-1")
                .expect("string ID literal/generated value must be non-empty"),
            call_id: CallId::new("call-route-1")
                .expect("string ID literal/generated value must be non-empty"),
            evaluation_id: evaluation.evaluation_id.clone(),
            grant_id: ToyGrantId::new("toy-grant-route-1")
                .expect("string ID literal/generated value must be non-empty"),
            toy_id: ToyId::new("toy-echo")
                .expect("string ID literal/generated value must be non-empty"),
            child_instance_id: ChildInstanceId::new("child-instance-route-1")
                .expect("string ID literal/generated value must be non-empty"),
            authority_decision_id: evaluation.decision_id.clone(),
            expires_at: Timestamp::new("2026-05-31T00:02:00Z").unwrap(),
        });

        ToyGrantEvaluationResult {
            evaluation,
            authorized,
        }
    }

    #[test]
    fn route_decision_records_selected_candidate_and_authority_evidence() {
        let selected = candidate("candidate-1", RuntimeKind::Process);
        let eliminated = candidate("candidate-2", RuntimeKind::RemotePeer);
        let decision = RouteDecision::selected(
            &call(),
            selected.clone(),
            vec![
                CandidateAuthorityEvaluation::admissible(selected.clone(), 1, 1),
                CandidateAuthorityEvaluation::eliminated(
                    eliminated,
                    CandidateEliminationReason::PeerNotAdmitted,
                    1,
                    1,
                ),
            ],
            RouteDecisionIds {
                decision_id: DecisionId::new("route-decision-1")
                    .expect("string ID literal/generated value must be non-empty"),
                observation_id: ObservationId::new("obs-route-decision-1")
                    .expect("string ID literal/generated value must be non-empty"),
            },
        );

        assert_eq!(
            decision.call_id,
            CallId::new("call-route-1")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(decision.outcome, RouteDecisionOutcome::RouteSelected);
        assert_eq!(decision.selected_route, Some(selected));
        assert_eq!(decision.authority_evaluations.len(), 2);
        assert_eq!(
            decision.authority_evaluations[1].reason,
            Some(CandidateEliminationReason::PeerNotAdmitted)
        );
    }

    #[test]
    fn route_revalidation_allows_matching_execution_authority() {
        let call = call();
        let selected = candidate("candidate-1", RuntimeKind::Process);
        let initial = initial_selected_route(selected.clone());
        let child = child_result(1, true, "child-echo");
        let toy = toy_result(1, 1, true);

        let revalidation =
            revalidate_route_for_execution(&call, &initial, child, &[toy], revalidation_ids());

        assert!(revalidation.is_authorized());
        assert_eq!(revalidation.reason, RouteRevalidationReason::Revalidated);
        assert_eq!(
            revalidation.decision.decision_kind,
            RouteDecisionKind::Revalidation
        );
        assert_eq!(
            revalidation.decision.initial_decision_id,
            Some(initial.decision_id)
        );
        assert_eq!(revalidation.decision.selected_route, Some(selected.clone()));
        let authorized = revalidation.authorized.expect("authorized route execution");
        assert_eq!(authorized.route, selected);
        assert_eq!(authorized.child_invocation.child_name(), "child-echo");
        assert_eq!(authorized.toy_calls.len(), 1);
    }

    #[test]
    fn route_revalidation_denies_stale_policy_before_execution() {
        let call = call();
        let selected = candidate("candidate-1", RuntimeKind::Process);
        let initial = initial_selected_route(selected);
        let child = child_result(0, true, "child-echo");

        let revalidation =
            revalidate_route_for_execution(&call, &initial, child, &[], revalidation_ids());

        assert!(!revalidation.is_authorized());
        assert_eq!(
            revalidation.reason,
            RouteRevalidationReason::PolicyRevisionStale
        );
        assert_eq!(revalidation.decision.outcome, RouteDecisionOutcome::NoRoute);
        assert_eq!(
            revalidation.decision.no_route_reason,
            Some(CandidateEliminationReason::PolicyRevisionStale)
        );
        assert_eq!(revalidation.decision.safe_message, "not authorized");
    }

    #[test]
    fn route_revalidation_denies_route_child_mismatch() {
        let call = call();
        let selected = candidate("candidate-1", RuntimeKind::Process);
        let initial = initial_selected_route(selected);
        let child = child_result(1, true, "other-child");

        let revalidation =
            revalidate_route_for_execution(&call, &initial, child, &[], revalidation_ids());

        assert_eq!(
            revalidation.reason,
            RouteRevalidationReason::SelectedChildMismatch
        );
        assert_eq!(
            revalidation.decision.no_route_reason,
            Some(CandidateEliminationReason::RouteMismatch)
        );
        assert!(revalidation.authorized.is_none());
    }

    #[test]
    fn route_revalidation_denies_failed_toy_evidence() {
        let call = call();
        let selected = candidate("candidate-1", RuntimeKind::Process);
        let initial = initial_selected_route(selected);
        let child = child_result(1, true, "child-echo");
        let toy = toy_result(1, 1, false);

        let revalidation =
            revalidate_route_for_execution(&call, &initial, child, &[toy], revalidation_ids());

        assert_eq!(revalidation.reason, RouteRevalidationReason::ToyGrantDenied);
        assert_eq!(
            revalidation.decision.no_route_reason,
            Some(CandidateEliminationReason::ToyGrantMissing)
        );
        assert!(revalidation.authorized.is_none());
    }

    #[test]
    fn no_route_decision_denies_by_default_without_route_taken() {
        let call = call();
        let eliminated = candidate("candidate-1", RuntimeKind::RemotePeer);
        let decision = RouteDecision::no_route(
            &call,
            vec![CandidateAuthorityEvaluation::eliminated(
                eliminated,
                CandidateEliminationReason::PeerNotAdmitted,
                1,
                1,
            )],
            CandidateEliminationReason::PeerNotAdmitted,
            RouteDecisionIds {
                decision_id: DecisionId::new("route-decision-denied")
                    .expect("string ID literal/generated value must be non-empty"),
                observation_id: ObservationId::new("obs-route-denied")
                    .expect("string ID literal/generated value must be non-empty"),
            },
        );
        let result = no_route_denied_result(
            &call,
            &decision,
            AuditRef::new("audit-route-denied")
                .expect("string ID literal/generated value must be non-empty"),
        );

        assert!(decision.is_no_route());
        assert_eq!(decision.selected_route, None);
        assert_eq!(decision.safe_message, "not authorized");
        assert_eq!(result.outcome, ResultOutcome::Denied);
        assert_eq!(result.route_taken, None);
        assert_eq!(result.authority_decision_ref, decision.decision_id);
        assert_eq!(result.requester_message, "not authorized");
    }

    #[test]
    fn route_decision_uses_snake_case_wire_names() {
        let encoded = serde_json::to_string(&NetworkPathClass::Relayed).unwrap();
        assert_eq!(encoded, "\"relayed\"");
    }
}

use crate::{call::*, child::*, id::*, toy::*};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPathClass {
    Direct,
    Relayed,
    Local,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateRoute {
    pub candidate_id: String,
    pub node_id: MctNodeId,
    pub child_id: Option<ChildId>,
    pub runtime_kind: RuntimeKind,
    pub network_path: NetworkPathClass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateAuthorityOutcome {
    Admissible,
    Eliminated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateEliminationReason {
    DataPolicyDenied,
    VisionPolicyDenied,
    PeerNotAdmitted,
    ChildNotApproved,
    ToyGrantMissing,
    SecretScopeForbidden,
    PolicyRevisionStale,
    GrantsRevisionStale,
    RouteMismatch,
    CapabilityUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteDecisionKind {
    Initial,
    Revalidation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteRevalidationReason {
    Revalidated,
    InitialDecisionNotSelected,
    CallIdMismatch,
    SelectedRouteNotAdmissible,
    SelectedChildMismatch,
    ChildAuthorityDenied,
    ToyGrantDenied,
    PolicyRevisionStale,
    GrantsRevisionStale,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateAuthorityEvaluation {
    pub candidate: CandidateRoute,
    pub outcome: CandidateAuthorityOutcome,
    pub reason: Option<CandidateEliminationReason>,
    pub safe_message: String,
    pub policy_revision: u64,
    pub grants_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteDecisionOutcome {
    RouteSelected,
    NoRoute,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteDecision {
    pub decision_id: DecisionId,
    pub call_id: CallId,
    pub decision_kind: RouteDecisionKind,
    pub initial_decision_id: Option<DecisionId>,
    pub authority_evaluations: Vec<CandidateAuthorityEvaluation>,
    pub selected_route: Option<CandidateRoute>,
    pub outcome: RouteDecisionOutcome,
    pub no_route_reason: Option<CandidateEliminationReason>,
    pub safe_message: String,
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteDecisionIds {
    pub decision_id: DecisionId,
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteRevalidationIds {
    pub decision_id: DecisionId,
    pub observation_id: ObservationId,
    pub authorized_route_execution_id: AuthorizedRouteExecutionId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizedRouteExecution {
    pub authorized_route_execution_id: AuthorizedRouteExecutionId,
    pub call_id: CallId,
    pub initial_decision_id: DecisionId,
    pub revalidation_decision_id: DecisionId,
    pub route: CandidateRoute,
    pub child_invocation: AuthorizedChildInvocation,
    pub toy_calls: Vec<AuthorizedToyCall>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteRevalidationResult {
    pub decision: RouteDecision,
    pub reason: RouteRevalidationReason,
    pub authorized: Option<AuthorizedRouteExecution>,
}

impl RouteRevalidationResult {
    pub fn is_authorized(&self) -> bool {
        self.reason == RouteRevalidationReason::Revalidated && self.authorized.is_some()
    }
}

impl CandidateAuthorityEvaluation {
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

    pub fn is_no_route(&self) -> bool {
        self.outcome == RouteDecisionOutcome::NoRoute
    }
}

pub fn revalidate_route_for_execution(
    call: &MctCall,
    initial: &RouteDecision,
    child: &ChildCallAuthorityResult,
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

    let Some(child_invocation) = child.authorized.as_ref() else {
        return revalidation_denied(
            call,
            initial,
            Some(selected_route.clone()),
            ids,
            RouteRevalidationReason::ChildAuthorityDenied,
            CandidateEliminationReason::ChildNotApproved,
        );
    };

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

    if let Some(child_id) = selected_route.child_id.as_ref()
        && child_id.as_str() != child_invocation.child_name
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
        child_invocation: child_invocation.clone(),
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
            call_id: CallId::from("call-route-1"),
            caller: CallerIdentity {
                node_id: MctNodeId::from("node-a"),
                user_id: None,
                vision_id: VisionId::from("vision-a"),
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
            deadline: Timestamp::from("2026-05-31T00:01:00Z"),
            trace_context: TraceContext {
                trace_id: TraceId::from("trace-route-1"),
                span_id: SpanId::from("span-route-1"),
            },
            origin: CallOrigin::Cli,
        }
    }

    fn candidate(id: &str, runtime_kind: RuntimeKind) -> CandidateRoute {
        CandidateRoute {
            candidate_id: id.into(),
            node_id: MctNodeId::from("node-b"),
            child_id: Some(ChildId::from("child-echo")),
            runtime_kind,
            network_path: NetworkPathClass::Local,
        }
    }

    fn route_ids(decision: &str, observation: &str) -> RouteDecisionIds {
        RouteDecisionIds {
            decision_id: DecisionId::from(decision),
            observation_id: ObservationId::from(observation),
        }
    }

    fn revalidation_ids() -> RouteRevalidationIds {
        RouteRevalidationIds {
            decision_id: DecisionId::from("route-revalidation-1"),
            observation_id: ObservationId::from("obs-route-revalidation-1"),
            authorized_route_execution_id: AuthorizedRouteExecutionId::from("authorized-route-1"),
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
        let evaluation = ChildCallAuthorityEvaluation {
            evaluation_id: ChildCallEvaluationId::from("child-eval-route-1"),
            call_id: CallId::from("call-route-1"),
            decision_id: DecisionId::from("child-decision-route-1"),
            instance_id: Some(ChildInstanceId::from("child-instance-route-1")),
            assignment_id: Some(ChildAssignmentId::from("assignment-route-1")),
            approval_id: Some(ChildApprovalId::from("approval-route-1")),
            artifact_id: Some(ComponentArtifactId::from("artifact-route-1")),
            child_name: Some(child_name.into()),
            verdict: if allowed {
                ChildCallVerdict::Allowed
            } else {
                ChildCallVerdict::Denied
            },
            reason_code: if allowed {
                ChildCallReasonCode::ReadyAuthorizedInstance
            } else {
                ChildCallReasonCode::AssignmentRevoked
            },
            policy_revision,
            observation_id: ObservationId::from("obs-child-route-1"),
        };
        let authorized = allowed.then(|| AuthorizedChildInvocation {
            authorized_child_invocation_id: AuthorizedChildInvocationId::from(
                "authorized-child-route-1",
            ),
            call_id: CallId::from("call-route-1"),
            evaluation_id: evaluation.evaluation_id.clone(),
            assignment_id: ChildAssignmentId::from("assignment-route-1"),
            approval_id: ChildApprovalId::from("approval-route-1"),
            artifact_id: ComponentArtifactId::from("artifact-route-1"),
            child_instance_id: ChildInstanceId::from("child-instance-route-1"),
            child_name: child_name.into(),
            authority_decision_id: evaluation.decision_id.clone(),
        });

        ChildCallAuthorityResult {
            evaluation,
            authorized,
        }
    }

    fn toy_result(
        policy_revision: u64,
        grants_revision: u64,
        allowed: bool,
    ) -> ToyGrantEvaluationResult {
        let evaluation = ToyGrantEvaluation {
            evaluation_id: ToyGrantEvaluationId::from("toy-eval-route-1"),
            call_id: CallId::from("call-route-1"),
            decision_id: DecisionId::from("toy-decision-route-1"),
            grant_id: allowed.then(|| ToyGrantId::from("toy-grant-route-1")),
            toy_id: ToyId::from("toy-echo"),
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
            observation_id: ObservationId::from("obs-toy-route-1"),
        };
        let authorized = allowed.then(|| AuthorizedToyCall {
            authorized_toy_call_id: AuthorizedToyCallId::from("authorized-toy-route-1"),
            call_id: CallId::from("call-route-1"),
            evaluation_id: evaluation.evaluation_id.clone(),
            grant_id: ToyGrantId::from("toy-grant-route-1"),
            toy_id: ToyId::from("toy-echo"),
            child_instance_id: ChildInstanceId::from("child-instance-route-1"),
            authority_decision_id: evaluation.decision_id.clone(),
            expires_at: Timestamp::from("2026-05-31T00:02:00Z"),
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
                decision_id: DecisionId::from("route-decision-1"),
                observation_id: ObservationId::from("obs-route-decision-1"),
            },
        );

        assert_eq!(decision.call_id, CallId::from("call-route-1"));
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
            revalidate_route_for_execution(&call, &initial, &child, &[toy], revalidation_ids());

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
        assert_eq!(authorized.child_invocation.child_name, "child-echo");
        assert_eq!(authorized.toy_calls.len(), 1);
    }

    #[test]
    fn route_revalidation_denies_stale_policy_before_execution() {
        let call = call();
        let selected = candidate("candidate-1", RuntimeKind::Process);
        let initial = initial_selected_route(selected);
        let child = child_result(0, true, "child-echo");

        let revalidation =
            revalidate_route_for_execution(&call, &initial, &child, &[], revalidation_ids());

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
            revalidate_route_for_execution(&call, &initial, &child, &[], revalidation_ids());

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
            revalidate_route_for_execution(&call, &initial, &child, &[toy], revalidation_ids());

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
                decision_id: DecisionId::from("route-decision-denied"),
                observation_id: ObservationId::from("obs-route-denied"),
            },
        );
        let result = no_route_denied_result(&call, &decision, AuditRef::from("audit-route-denied"));

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

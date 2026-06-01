use crate::{call::*, id::*};
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
    CapabilityUnavailable,
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

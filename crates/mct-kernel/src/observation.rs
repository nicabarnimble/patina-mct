use crate::{call::*, child::*, id::*, peer::*, route::*, toy::*};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationTraceRef {
    pub trace_id: TraceId,
    pub span_id: Option<SpanId>,
    pub parent_span_id: Option<SpanId>,
    pub external_trace_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    CallReceived,
    CallRejected,
    CallConstructed,
    CallAuthorized,
    CallDenied,
    CandidateConsidered,
    CandidateEliminated,
    RouteSelected,
    NoRouteRecorded,
    RouteRevalidated,
    ResultRecorded,
    ArtifactVerified,
    ArtifactRejected,
    ChildApproved,
    ChildRevoked,
    ChildAssigned,
    ChildAssignmentRevoked,
    ChildInstanceLoading,
    ChildInstanceReady,
    ChildInstanceDegraded,
    ChildInstanceDraining,
    ChildInstanceStopped,
    ChildInstanceFailed,
    ChildInvoked,
    ToyGrantAllowed,
    ToyGrantDenied,
    ToyGrantExpired,
    ToyGrantRevoked,
    ToyCallStarted,
    ToyCallCompleted,
    ToyCallFailed,
    DataMovementAllowed,
    DataMovementDenied,
    SecretAccessAllowed,
    SecretAccessDenied,
    PeerConnected,
    PeerHelloReceived,
    PeerProtocolNegotiated,
    PeerHelloResponded,
    PeerBindingRecorded,
    PeerBindingRevoked,
    PeerBindingExpired,
    PeerAdmitted,
    PeerRejected,
    PeerCallSent,
    PeerCallReceived,
    PeerCallMalformed,
    PeerCallReplied,
    PeerStreamOpened,
    PeerStreamReset,
    IrohPathObserved,
    RuntimeExecutionStarted,
    RuntimeExecutionCompleted,
    RuntimeExecutionFailed,
    RuntimeExecutionTrapped,
    RuntimeExecutionTimedOut,
    AdapterEffectStarted,
    AdapterEffectCompleted,
    AdapterEffectFailed,
    StorageAppendSucceeded,
    StorageAppendFailed,
    ObservationBackpressureApplied,
    LifecycleTransitionRecorded,
    NodeHealthReported,
    OperatorActionRecorded,
    TelemetryExported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourcePlane {
    Kernel,
    Adapter,
    Peer,
    Child,
    Toy,
    Storage,
    Operator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationOutcome {
    Allowed,
    Denied,
    Started,
    Completed,
    Failed,
    TimedOut,
    Cancelled,
    Informational,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationVisibility {
    CallerSafe,
    VisionOperator,
    NodeOperator,
    SystemOperator,
    InternalOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctObservation {
    pub observation_id: ObservationId,
    pub observed_at: Timestamp,
    pub kind: ObservationKind,
    pub source_plane: SourcePlane,
    pub trace: ObservationTraceRef,
    pub call_id: Option<CallId>,
    pub decision_id: Option<DecisionId>,
    pub subject_id: Option<String>,
    pub resource_id: Option<String>,
    pub policy_revision: Option<u64>,
    pub grants_revision: Option<u64>,
    pub outcome: ObservationOutcome,
    pub visibility: ObservationVisibility,
    pub safe_message: String,
    pub detail_ref: Option<String>,
}

impl MctObservation {
    pub fn informational(
        observation_id: ObservationId,
        observed_at: Timestamp,
        kind: ObservationKind,
        trace_id: TraceId,
        safe_message: impl Into<String>,
    ) -> Self {
        Self {
            observation_id,
            observed_at,
            kind,
            source_plane: SourcePlane::Kernel,
            trace: ObservationTraceRef {
                trace_id,
                span_id: None,
                parent_span_id: None,
                external_trace_id: None,
            },
            call_id: None,
            decision_id: None,
            subject_id: None,
            resource_id: None,
            policy_revision: None,
            grants_revision: None,
            outcome: ObservationOutcome::Informational,
            visibility: ObservationVisibility::InternalOnly,
            safe_message: safe_message.into(),
            detail_ref: None,
        }
    }
}

pub fn hello_evaluation_observation(
    trace_id: TraceId,
    evaluation: &MctHelloAdmissionEvaluation,
) -> MctObservation {
    let admitted = evaluation.is_admitted();
    MctObservation {
        observation_id: evaluation.observation_id.clone(),
        observed_at: Timestamp::from("2026-05-31T00:00:00Z"),
        kind: if admitted {
            ObservationKind::PeerAdmitted
        } else {
            ObservationKind::PeerRejected
        },
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: Some(evaluation.decision_id.clone()),
        subject_id: evaluation
            .selected_binding_id
            .as_ref()
            .map(ToString::to_string),
        resource_id: None,
        policy_revision: None,
        grants_revision: None,
        outcome: if admitted {
            ObservationOutcome::Allowed
        } else {
            ObservationOutcome::Denied
        },
        visibility: ObservationVisibility::InternalOnly,
        safe_message: match evaluation.safe_reason {
            SafeHelloReason::Admitted => "admitted",
            SafeHelloReason::NotAuthorized => "not authorized",
            SafeHelloReason::UnsupportedVersion => "unsupported version",
            SafeHelloReason::RetryLater => "retry later",
        }
        .into(),
        detail_ref: Some(format!("hello_reason:{:?}", evaluation.reason)),
    }
}

pub fn peer_binding_state_observation(
    trace_id: TraceId,
    binding: &MctPeerBinding,
) -> MctObservation {
    let (kind, outcome, safe_message) = match binding.binding_state {
        BindingState::Admitted => (
            ObservationKind::PeerBindingRecorded,
            ObservationOutcome::Allowed,
            "peer binding admitted",
        ),
        BindingState::Pending => (
            ObservationKind::PeerBindingRecorded,
            ObservationOutcome::Informational,
            "peer binding pending",
        ),
        BindingState::Denied => (
            ObservationKind::PeerRejected,
            ObservationOutcome::Denied,
            "not authorized",
        ),
        BindingState::Expired => (
            ObservationKind::PeerBindingExpired,
            ObservationOutcome::Denied,
            "not authorized",
        ),
        BindingState::Revoked => (
            ObservationKind::PeerBindingRevoked,
            ObservationOutcome::Denied,
            "not authorized",
        ),
    };

    MctObservation {
        observation_id: binding
            .superseded_by_observation_id
            .clone()
            .unwrap_or_else(|| binding.created_by_observation_id.clone()),
        observed_at: Timestamp::from("2026-05-31T00:00:00Z"),
        kind,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(binding.binding_id.to_string()),
        resource_id: Some(binding.iroh_endpoint_id.to_string()),
        policy_revision: Some(binding.policy_revision),
        grants_revision: None,
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: Some(format!("binding_state:{:?}", binding.binding_state)),
    }
}

pub fn route_decision_observation(trace_id: TraceId, decision: &RouteDecision) -> MctObservation {
    let (kind, outcome) = match (decision.decision_kind, decision.outcome) {
        (RouteDecisionKind::Initial, RouteDecisionOutcome::RouteSelected) => {
            (ObservationKind::RouteSelected, ObservationOutcome::Allowed)
        }
        (RouteDecisionKind::Revalidation, RouteDecisionOutcome::RouteSelected) => (
            ObservationKind::RouteRevalidated,
            ObservationOutcome::Allowed,
        ),
        (_, RouteDecisionOutcome::NoRoute) => {
            (ObservationKind::NoRouteRecorded, ObservationOutcome::Denied)
        }
    };

    MctObservation {
        observation_id: decision.observation_id.clone(),
        observed_at: Timestamp::from("2026-05-31T00:00:00Z"),
        kind,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(decision.call_id.clone()),
        decision_id: Some(decision.decision_id.clone()),
        subject_id: None,
        resource_id: decision
            .selected_route
            .as_ref()
            .map(|route| route.candidate_id.clone()),
        policy_revision: decision
            .authority_evaluations
            .iter()
            .map(|evaluation| evaluation.policy_revision)
            .max(),
        grants_revision: decision
            .authority_evaluations
            .iter()
            .map(|evaluation| evaluation.grants_revision)
            .max(),
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: decision.safe_message.clone(),
        detail_ref: route_decision_detail_ref(decision),
    }
}

fn route_decision_detail_ref(decision: &RouteDecision) -> Option<String> {
    match (decision.decision_kind, decision.no_route_reason) {
        (RouteDecisionKind::Initial, Some(reason)) => Some(format!("no_route_reason:{reason:?}")),
        (RouteDecisionKind::Initial, None) => None,
        (RouteDecisionKind::Revalidation, Some(reason)) => Some(format!(
            "initial_decision:{};revalidation_no_route_reason:{reason:?}",
            decision
                .initial_decision_id
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "missing".into())
        )),
        (RouteDecisionKind::Revalidation, None) => Some(format!(
            "initial_decision:{};revalidated",
            decision
                .initial_decision_id
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "missing".into())
        )),
    }
}

pub fn call_protocol_evaluation_observation(
    trace_id: TraceId,
    evaluation: &MctCallProtocolEvaluation,
) -> MctObservation {
    let (kind, outcome) = match evaluation.outcome {
        CallProtocolOutcome::AcceptedForRouting | CallProtocolOutcome::Completed => {
            (ObservationKind::CallAuthorized, ObservationOutcome::Allowed)
        }
        CallProtocolOutcome::Malformed => (
            ObservationKind::PeerCallMalformed,
            ObservationOutcome::Denied,
        ),
        CallProtocolOutcome::Denied => (ObservationKind::CallDenied, ObservationOutcome::Denied),
        CallProtocolOutcome::Failed => (
            ObservationKind::AdapterEffectFailed,
            ObservationOutcome::Failed,
        ),
        CallProtocolOutcome::TimedOut => (
            ObservationKind::AdapterEffectFailed,
            ObservationOutcome::TimedOut,
        ),
    };

    MctObservation {
        observation_id: evaluation.observation_id.clone(),
        observed_at: Timestamp::from("2026-05-31T00:00:00Z"),
        kind,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: evaluation.call_id.clone(),
        decision_id: Some(evaluation.decision_id.clone()),
        subject_id: None,
        resource_id: Some(evaluation.protocol_request_id.to_string()),
        policy_revision: None,
        grants_revision: None,
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: evaluation.safe_message.clone(),
        detail_ref: Some(format!("call_reason:{:?}", evaluation.reason)),
    }
}

pub fn child_approval_observation(trace_id: TraceId, approval: &ChildApproval) -> MctObservation {
    let (kind, outcome, safe_message) = match approval.approval_state {
        ChildApprovalState::Approved => (
            ObservationKind::ChildApproved,
            ObservationOutcome::Allowed,
            "child approved",
        ),
        ChildApprovalState::Blocked | ChildApprovalState::Revoked => (
            ObservationKind::ChildRevoked,
            ObservationOutcome::Denied,
            "not authorized",
        ),
        ChildApprovalState::Candidate | ChildApprovalState::Deprecated => (
            ObservationKind::LifecycleTransitionRecorded,
            ObservationOutcome::Informational,
            "child approval state recorded",
        ),
    };

    MctObservation {
        observation_id: approval.authority_observation_id.clone(),
        observed_at: Timestamp::from("2026-05-31T00:00:00Z"),
        kind,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(approval.child_name.clone()),
        resource_id: Some(approval.artifact_id.to_string()),
        policy_revision: Some(approval.policy_revision),
        grants_revision: None,
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: Some(format!(
            "child_approval_state:{:?}",
            approval.approval_state
        )),
    }
}

pub fn child_assignment_observation(
    trace_id: TraceId,
    assignment: &ChildAssignment,
) -> MctObservation {
    let (kind, outcome, safe_message) = match assignment.assignment_state {
        ChildAssignmentState::Active => (
            ObservationKind::ChildAssigned,
            ObservationOutcome::Allowed,
            "child assigned",
        ),
        ChildAssignmentState::Revoked => (
            ObservationKind::ChildAssignmentRevoked,
            ObservationOutcome::Denied,
            "not authorized",
        ),
    };

    MctObservation {
        observation_id: assignment.assignment_observation_id.clone(),
        observed_at: Timestamp::from("2026-05-31T00:00:00Z"),
        kind,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(assignment.child_name.clone()),
        resource_id: Some(assignment.assignment_id.to_string()),
        policy_revision: None,
        grants_revision: None,
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: Some(format!(
            "child_assignment_state:{:?}",
            assignment.assignment_state
        )),
    }
}

pub fn child_instance_observation(trace_id: TraceId, instance: &ChildInstance) -> MctObservation {
    let (kind, outcome, safe_message) = match instance.instance_state {
        ChildInstanceState::Loading => (
            ObservationKind::ChildInstanceLoading,
            ObservationOutcome::Started,
            "child instance loading",
        ),
        ChildInstanceState::Ready => (
            ObservationKind::ChildInstanceReady,
            ObservationOutcome::Allowed,
            "child instance ready",
        ),
        ChildInstanceState::Degraded => (
            ObservationKind::ChildInstanceDegraded,
            ObservationOutcome::Failed,
            "child instance degraded",
        ),
        ChildInstanceState::Draining => (
            ObservationKind::ChildInstanceDraining,
            ObservationOutcome::Started,
            "child instance draining",
        ),
        ChildInstanceState::Stopped => (
            ObservationKind::ChildInstanceStopped,
            ObservationOutcome::Completed,
            "child instance stopped",
        ),
        ChildInstanceState::Failed => (
            ObservationKind::ChildInstanceFailed,
            ObservationOutcome::Failed,
            "child instance failed",
        ),
    };

    MctObservation {
        observation_id: instance.last_lifecycle_observation_id.clone(),
        observed_at: Timestamp::from("2026-05-31T00:00:00Z"),
        kind,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(instance.child_name.clone()),
        resource_id: Some(instance.instance_id.to_string()),
        policy_revision: None,
        grants_revision: None,
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: Some(format!(
            "child_instance_state:{:?}",
            instance.instance_state
        )),
    }
}

pub fn child_call_authority_observation(
    trace_id: TraceId,
    evaluation: &ChildCallAuthorityEvaluation,
) -> MctObservation {
    let allowed = evaluation.verdict == ChildCallVerdict::Allowed;
    MctObservation {
        observation_id: evaluation.observation_id.clone(),
        observed_at: Timestamp::from("2026-05-31T00:00:00Z"),
        kind: if allowed {
            ObservationKind::RouteRevalidated
        } else {
            ObservationKind::CallDenied
        },
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(evaluation.call_id.clone()),
        decision_id: Some(evaluation.decision_id.clone()),
        subject_id: evaluation.child_name.clone(),
        resource_id: evaluation
            .instance_id
            .as_ref()
            .map(ToString::to_string)
            .or_else(|| evaluation.artifact_id.as_ref().map(ToString::to_string)),
        policy_revision: Some(evaluation.policy_revision),
        grants_revision: None,
        outcome: if allowed {
            ObservationOutcome::Allowed
        } else {
            ObservationOutcome::Denied
        },
        visibility: ObservationVisibility::InternalOnly,
        safe_message: if allowed {
            "child call authorized"
        } else {
            "not authorized"
        }
        .into(),
        detail_ref: Some(format!("child_call_reason:{:?}", evaluation.reason_code)),
    }
}

pub fn toy_grant_evaluation_observation(
    trace_id: TraceId,
    evaluation: &ToyGrantEvaluation,
) -> MctObservation {
    let kind = match (evaluation.verdict, evaluation.reason_code) {
        (ToyGrantVerdict::Allowed, _) => ObservationKind::ToyGrantAllowed,
        (ToyGrantVerdict::Denied, ToyGrantReasonCode::ExpiredGrant) => {
            ObservationKind::ToyGrantExpired
        }
        (ToyGrantVerdict::Denied, ToyGrantReasonCode::RevokedGrant) => {
            ObservationKind::ToyGrantRevoked
        }
        (ToyGrantVerdict::Denied, _) => ObservationKind::ToyGrantDenied,
    };

    MctObservation {
        observation_id: evaluation.observation_id.clone(),
        observed_at: Timestamp::from("2026-05-31T00:00:00Z"),
        kind,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(evaluation.call_id.clone()),
        decision_id: Some(evaluation.decision_id.clone()),
        subject_id: Some(evaluation.subject_child_name.clone()),
        resource_id: Some(evaluation.toy_id.to_string()),
        policy_revision: Some(evaluation.policy_revision),
        grants_revision: Some(evaluation.grants_revision),
        outcome: match evaluation.verdict {
            ToyGrantVerdict::Allowed => ObservationOutcome::Allowed,
            ToyGrantVerdict::Denied => ObservationOutcome::Denied,
        },
        visibility: ObservationVisibility::InternalOnly,
        safe_message: match evaluation.verdict {
            ToyGrantVerdict::Allowed => "toy grant allowed",
            ToyGrantVerdict::Denied => "not authorized",
        }
        .into(),
        detail_ref: Some(format!("toy_grant_reason:{:?}", evaluation.reason_code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_denial_evaluations_become_observations() {
        let hello = MctHelloAdmissionEvaluation {
            decision_id: DecisionId::from("decision-hello"),
            request_id: "hello-1".into(),
            peer_admission_decision_id: None,
            selected_binding_id: None,
            selected_node_id: None,
            selected_vision_id: None,
            negotiated_protocol: None,
            accepted_alpns: Vec::new(),
            hello_outcome: HelloOutcome::Denied,
            reason: HelloReason::MissingBinding,
            safe_reason: SafeHelloReason::NotAuthorized,
            observation_id: ObservationId::from("obs-hello-denied"),
        };
        let hello_observation = hello_evaluation_observation(TraceId::from("trace-1"), &hello);
        assert_eq!(hello_observation.kind, ObservationKind::PeerRejected);
        assert_eq!(hello_observation.source_plane, SourcePlane::Kernel);
        assert_eq!(hello_observation.outcome, ObservationOutcome::Denied);
        assert_eq!(
            hello_observation.decision_id,
            Some(DecisionId::from("decision-hello"))
        );
        assert_eq!(hello_observation.safe_message, "not authorized");

        let call = MctCallProtocolEvaluation {
            decision_id: DecisionId::from("decision-call"),
            protocol_request_id: ProtocolRequestId::from("proto-call"),
            call_id: Some(CallId::from("call-1")),
            route_decision_id: None,
            result_ref: None,
            outcome: CallProtocolOutcome::Denied,
            reason: CallProtocolReason::HelloNotAdmitted,
            safe_message: "not authorized".into(),
            observation_id: ObservationId::from("obs-call-denied"),
        };
        let call_observation =
            call_protocol_evaluation_observation(TraceId::from("trace-1"), &call);
        assert_eq!(call_observation.kind, ObservationKind::CallDenied);
        assert_eq!(call_observation.source_plane, SourcePlane::Kernel);
        assert_eq!(call_observation.outcome, ObservationOutcome::Denied);
        assert_eq!(call_observation.call_id, Some(CallId::from("call-1")));
        assert_eq!(
            call_observation.decision_id,
            Some(DecisionId::from("decision-call"))
        );
    }

    fn binding(state: BindingState) -> MctPeerBinding {
        MctPeerBinding {
            binding_id: PeerBindingId::from("binding-1"),
            iroh_endpoint_id: EndpointIdText::from("endpoint-a"),
            scope: MctPeerBindingScope {
                mct_node_id: MctNodeId::from("node-b"),
                vision_id: VisionId::from("vision-a"),
                allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
                data_scope: None,
                observation_scope: None,
            },
            issuer_node_id: MctNodeId::from("node-a"),
            policy_revision: 7,
            binding_state: state,
            issued_at: Timestamp::from("2026-05-31T00:00:00Z"),
            expires_at: None,
            created_by_observation_id: ObservationId::from("obs-binding-created"),
            superseded_by_observation_id: Some(ObservationId::from("obs-binding-superseded")),
        }
    }

    #[test]
    fn no_route_decision_becomes_observation() {
        let call = MctCall {
            call_id: CallId::from("call-no-route"),
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
                policy_revision: 3,
                grants_revision: 4,
                vision_policy_revision: 5,
            },
            deadline: Timestamp::from("2026-05-31T00:01:00Z"),
            trace_context: TraceContext {
                trace_id: TraceId::from("trace-no-route"),
                span_id: SpanId::from("span-no-route"),
            },
            origin: CallOrigin::Cli,
        };
        let candidate = CandidateRoute {
            candidate_id: "candidate-denied".into(),
            node_id: MctNodeId::from("node-b"),
            child_id: None,
            runtime_kind: RuntimeKind::RemotePeer,
            network_path: NetworkPathClass::Relayed,
        };
        let decision = RouteDecision::no_route(
            &call,
            vec![CandidateAuthorityEvaluation::eliminated(
                candidate,
                CandidateEliminationReason::PeerNotAdmitted,
                3,
                4,
            )],
            CandidateEliminationReason::PeerNotAdmitted,
            RouteDecisionIds {
                decision_id: DecisionId::from("route-decision-no-route"),
                observation_id: ObservationId::from("obs-route-no-route"),
            },
        );
        let observation = route_decision_observation(TraceId::from("trace-no-route"), &decision);

        assert_eq!(observation.kind, ObservationKind::NoRouteRecorded);
        assert_eq!(observation.outcome, ObservationOutcome::Denied);
        assert_eq!(observation.call_id, Some(CallId::from("call-no-route")));
        assert_eq!(
            observation.decision_id,
            Some(DecisionId::from("route-decision-no-route"))
        );
        assert_eq!(observation.safe_message, "not authorized");
        assert_eq!(observation.policy_revision, Some(3));
        assert_eq!(observation.grants_revision, Some(4));
    }

    #[test]
    fn route_revalidation_observation_records_allowed_and_denied_outcomes() {
        let trace_id = TraceId::from("trace-revalidation");
        let candidate = CandidateRoute {
            candidate_id: "candidate-revalidated".into(),
            node_id: MctNodeId::from("node-b"),
            child_id: Some(ChildId::from("child-echo")),
            runtime_kind: RuntimeKind::Process,
            network_path: NetworkPathClass::Local,
        };
        let allowed = RouteDecision {
            decision_id: DecisionId::from("route-revalidation-allowed"),
            call_id: CallId::from("call-revalidation"),
            decision_kind: RouteDecisionKind::Revalidation,
            initial_decision_id: Some(DecisionId::from("route-initial")),
            authority_evaluations: vec![CandidateAuthorityEvaluation::admissible(
                candidate.clone(),
                3,
                4,
            )],
            selected_route: Some(candidate.clone()),
            outcome: RouteDecisionOutcome::RouteSelected,
            no_route_reason: None,
            safe_message: "route revalidated".into(),
            observation_id: ObservationId::from("obs-route-revalidated"),
        };
        let allowed_observation = route_decision_observation(trace_id.clone(), &allowed);
        assert_eq!(allowed_observation.kind, ObservationKind::RouteRevalidated);
        assert_eq!(allowed_observation.outcome, ObservationOutcome::Allowed);
        assert_eq!(allowed_observation.policy_revision, Some(3));
        assert_eq!(allowed_observation.grants_revision, Some(4));
        assert_eq!(
            allowed_observation.detail_ref,
            Some("initial_decision:route-initial;revalidated".into())
        );

        let denied = RouteDecision {
            decision_id: DecisionId::from("route-revalidation-denied"),
            call_id: CallId::from("call-revalidation"),
            decision_kind: RouteDecisionKind::Revalidation,
            initial_decision_id: Some(DecisionId::from("route-initial")),
            authority_evaluations: vec![CandidateAuthorityEvaluation::eliminated(
                candidate,
                CandidateEliminationReason::PolicyRevisionStale,
                3,
                4,
            )],
            selected_route: None,
            outcome: RouteDecisionOutcome::NoRoute,
            no_route_reason: Some(CandidateEliminationReason::PolicyRevisionStale),
            safe_message: "not authorized".into(),
            observation_id: ObservationId::from("obs-route-revalidation-denied"),
        };
        let denied_observation = route_decision_observation(trace_id, &denied);
        assert_eq!(denied_observation.kind, ObservationKind::NoRouteRecorded);
        assert_eq!(denied_observation.outcome, ObservationOutcome::Denied);
        assert_eq!(denied_observation.safe_message, "not authorized");
        assert_eq!(
            denied_observation.detail_ref,
            Some(
                "initial_decision:route-initial;revalidation_no_route_reason:PolicyRevisionStale"
                    .into()
            )
        );
    }

    #[test]
    fn revoked_and_expired_bindings_become_observations() {
        let revoked = peer_binding_state_observation(
            TraceId::from("trace-1"),
            &binding(BindingState::Revoked),
        );
        assert_eq!(revoked.kind, ObservationKind::PeerBindingRevoked);
        assert_eq!(revoked.outcome, ObservationOutcome::Denied);
        assert_eq!(revoked.safe_message, "not authorized");
        assert_eq!(revoked.policy_revision, Some(7));

        let expired = peer_binding_state_observation(
            TraceId::from("trace-1"),
            &binding(BindingState::Expired),
        );
        assert_eq!(expired.kind, ObservationKind::PeerBindingExpired);
        assert_eq!(expired.outcome, ObservationOutcome::Denied);
        assert_eq!(expired.resource_id, Some("endpoint-a".into()));
    }

    #[test]
    fn toy_grant_evaluations_become_observations() {
        let allowed = ToyGrantEvaluation {
            evaluation_id: ToyGrantEvaluationId::from("toy-eval-1"),
            call_id: CallId::from("call-toy"),
            decision_id: DecisionId::from("decision-toy"),
            grant_id: Some(ToyGrantId::from("grant-toy")),
            toy_id: ToyId::from("toy-logging"),
            subject_child_name: "slate-manager".into(),
            verdict: ToyGrantVerdict::Allowed,
            reason_code: ToyGrantReasonCode::ActiveGrant,
            policy_revision: 3,
            grants_revision: 7,
            observation_id: ObservationId::from("obs-toy-allowed"),
        };
        let allowed_observation =
            toy_grant_evaluation_observation(TraceId::from("trace-toy"), &allowed);
        assert_eq!(allowed_observation.kind, ObservationKind::ToyGrantAllowed);
        assert_eq!(allowed_observation.outcome, ObservationOutcome::Allowed);
        assert_eq!(allowed_observation.call_id, Some(CallId::from("call-toy")));
        assert_eq!(allowed_observation.resource_id, Some("toy-logging".into()));

        let mut denied = allowed;
        denied.verdict = ToyGrantVerdict::Denied;
        denied.reason_code = ToyGrantReasonCode::MissingGrant;
        denied.observation_id = ObservationId::from("obs-toy-denied");
        let denied_observation =
            toy_grant_evaluation_observation(TraceId::from("trace-toy"), &denied);
        assert_eq!(denied_observation.kind, ObservationKind::ToyGrantDenied);
        assert_eq!(denied_observation.outcome, ObservationOutcome::Denied);
        assert_eq!(denied_observation.safe_message, "not authorized");
    }

    #[test]
    fn child_lifecycle_and_call_authority_become_observations() {
        let approval = ChildApproval {
            approval_id: ChildApprovalId::from("approval-child"),
            artifact_id: ComponentArtifactId::from("artifact-child"),
            child_name: "slate-manager".into(),
            artifact_version: "0.2.0".into(),
            scope_vision_id: Some(VisionId::from("vision-a")),
            scope_node_id: Some(MctNodeId::from("node-a")),
            scope_project_id: None,
            approval_state: ChildApprovalState::Approved,
            policy_revision: 5,
            authority_observation_id: ObservationId::from("obs-child-approved"),
        };
        let approval_observation =
            child_approval_observation(TraceId::from("trace-child"), &approval);
        assert_eq!(approval_observation.kind, ObservationKind::ChildApproved);
        assert_eq!(approval_observation.outcome, ObservationOutcome::Allowed);
        assert_eq!(
            approval_observation.resource_id,
            Some("artifact-child".into())
        );

        let assignment = ChildAssignment {
            assignment_id: ChildAssignmentId::from("assignment-child"),
            approval_id: ChildApprovalId::from("approval-child"),
            artifact_id: ComponentArtifactId::from("artifact-child"),
            child_name: "slate-manager".into(),
            vision_id: VisionId::from("vision-a"),
            node_id: Some(MctNodeId::from("node-a")),
            project_id: None,
            assignment_state: ChildAssignmentState::Active,
            pinned_artifact_version: "0.2.0".into(),
            assignment_observation_id: ObservationId::from("obs-child-assigned"),
        };
        let assignment_observation =
            child_assignment_observation(TraceId::from("trace-child"), &assignment);
        assert_eq!(assignment_observation.kind, ObservationKind::ChildAssigned);
        assert_eq!(assignment_observation.outcome, ObservationOutcome::Allowed);

        let instance = ChildInstance {
            instance_id: ChildInstanceId::from("instance-child"),
            assignment_id: ChildAssignmentId::from("assignment-child"),
            artifact_id: ComponentArtifactId::from("artifact-child"),
            child_name: "slate-manager".into(),
            generation: 1,
            node_id: MctNodeId::from("node-a"),
            instance_state: ChildInstanceState::Ready,
            readiness_observation_id: Some(ObservationId::from("obs-child-ready")),
            last_lifecycle_observation_id: ObservationId::from("obs-child-ready"),
        };
        let instance_observation =
            child_instance_observation(TraceId::from("trace-child"), &instance);
        assert_eq!(
            instance_observation.kind,
            ObservationKind::ChildInstanceReady
        );
        assert_eq!(instance_observation.outcome, ObservationOutcome::Allowed);

        let evaluation = ChildCallAuthorityEvaluation {
            evaluation_id: ChildCallEvaluationId::from("child-eval"),
            call_id: CallId::from("call-child"),
            decision_id: DecisionId::from("decision-child"),
            instance_id: Some(ChildInstanceId::from("instance-child")),
            assignment_id: Some(ChildAssignmentId::from("assignment-child")),
            approval_id: Some(ChildApprovalId::from("approval-child")),
            artifact_id: Some(ComponentArtifactId::from("artifact-child")),
            child_name: Some("slate-manager".into()),
            verdict: ChildCallVerdict::Denied,
            reason_code: ChildCallReasonCode::InstanceNotReady,
            policy_revision: 5,
            observation_id: ObservationId::from("obs-child-denied"),
        };
        let denial_observation =
            child_call_authority_observation(TraceId::from("trace-child"), &evaluation);
        assert_eq!(denial_observation.kind, ObservationKind::CallDenied);
        assert_eq!(denial_observation.outcome, ObservationOutcome::Denied);
        assert_eq!(denial_observation.safe_message, "not authorized");
    }

    #[test]
    fn observation_kind_uses_snake_case_wire_names() {
        let encoded = serde_json::to_string(&ObservationKind::PeerHelloReceived).unwrap();
        assert_eq!(encoded, "\"peer_hello_received\"");
    }
}

use crate::{call::*, id::*, peer::*, route::*};
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

pub fn route_decision_observation(
    trace_id: TraceId,
    decision: &RouteDecision,
) -> MctObservation {
    let (kind, outcome) = match decision.outcome {
        RouteDecisionOutcome::RouteSelected => {
            (ObservationKind::RouteSelected, ObservationOutcome::Allowed)
        }
        RouteDecisionOutcome::NoRoute => {
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
        detail_ref: decision
            .no_route_reason
            .map(|reason| format!("no_route_reason:{reason:?}")),
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
    fn observation_kind_uses_snake_case_wire_names() {
        let encoded = serde_json::to_string(&ObservationKind::PeerHelloReceived).unwrap();
        assert_eq!(encoded, "\"peer_hello_received\"");
    }
}

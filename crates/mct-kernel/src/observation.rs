use crate::{call::*, id::*, peer::*};
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

pub fn call_protocol_evaluation_observation(
    trace_id: TraceId,
    evaluation: &MctCallProtocolEvaluation,
) -> MctObservation {
    let (kind, outcome) = match evaluation.outcome {
        CallProtocolOutcome::AcceptedForRouting | CallProtocolOutcome::Completed => {
            (ObservationKind::CallAuthorized, ObservationOutcome::Allowed)
        }
        CallProtocolOutcome::Malformed => (ObservationKind::PeerCallMalformed, ObservationOutcome::Denied),
        CallProtocolOutcome::Denied => (ObservationKind::CallDenied, ObservationOutcome::Denied),
        CallProtocolOutcome::Failed => (ObservationKind::AdapterEffectFailed, ObservationOutcome::Failed),
        CallProtocolOutcome::TimedOut => (ObservationKind::AdapterEffectFailed, ObservationOutcome::TimedOut),
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
        assert_eq!(hello_observation.decision_id, Some(DecisionId::from("decision-hello")));
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
        let call_observation = call_protocol_evaluation_observation(TraceId::from("trace-1"), &call);
        assert_eq!(call_observation.kind, ObservationKind::CallDenied);
        assert_eq!(call_observation.source_plane, SourcePlane::Kernel);
        assert_eq!(call_observation.outcome, ObservationOutcome::Denied);
        assert_eq!(call_observation.call_id, Some(CallId::from("call-1")));
        assert_eq!(call_observation.decision_id, Some(DecisionId::from("decision-call")));
    }

    #[test]
    fn observation_kind_uses_snake_case_wire_names() {
        let encoded = serde_json::to_string(&ObservationKind::PeerHelloReceived).unwrap();
        assert_eq!(encoded, "\"peer_hello_received\"");
    }
}

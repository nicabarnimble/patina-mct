use crate::id::*;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_kind_uses_snake_case_wire_names() {
        let encoded = serde_json::to_string(&ObservationKind::PeerHelloReceived).unwrap();
        assert_eq!(encoded, "\"peer_hello_received\"");
    }
}

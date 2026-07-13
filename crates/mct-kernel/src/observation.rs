use crate::{call::*, child::*, id::*, peer::*, route::*, toy::*};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Trace linkage copied into durable observations.
pub struct ObservationTraceRef {
    /// Trace shared by related observations.
    pub trace_id: TraceId,
    /// Optional span that produced this observation.
    pub span_id: Option<SpanId>,
    /// Optional parent span for trace reconstruction.
    pub parent_span_id: Option<SpanId>,
    /// Optional foreign trace identifier from another system.
    pub external_trace_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Stable event taxonomy for observation ledger entries.
pub enum ObservationKind {
    /// Records that call received occurred.
    CallReceived,
    /// Records that call rejected occurred.
    CallRejected,
    /// Records that call constructed occurred.
    CallConstructed,
    /// Records that call authorized occurred.
    CallAuthorized,
    /// Records that call denied occurred.
    CallDenied,
    /// Records that candidate considered occurred.
    CandidateConsidered,
    /// Records that candidate eliminated occurred.
    CandidateEliminated,
    /// Records that route selected occurred.
    RouteSelected,
    /// Records that no route recorded occurred.
    NoRouteRecorded,
    /// Records that route revalidated occurred.
    RouteRevalidated,
    /// Records that result recorded occurred.
    ResultRecorded,
    /// Records that artifact verified occurred.
    ArtifactVerified,
    /// Records that artifact rejected occurred.
    ArtifactRejected,
    /// Records that child approved occurred.
    ChildApproved,
    /// Records that child revoked occurred.
    ChildRevoked,
    /// Records that child assigned occurred.
    ChildAssigned,
    /// Records that child assignment revoked occurred.
    ChildAssignmentRevoked,
    /// Records that child instance loading occurred.
    ChildInstanceLoading,
    /// Records that child instance ready occurred.
    ChildInstanceReady,
    /// Records that child instance degraded occurred.
    ChildInstanceDegraded,
    /// Records that child instance draining occurred.
    ChildInstanceDraining,
    /// Records that child instance stopped occurred.
    ChildInstanceStopped,
    /// Records that child instance failed occurred.
    ChildInstanceFailed,
    /// Records that child invoked occurred.
    ChildInvoked,
    /// Records that toy grant allowed occurred.
    ToyGrantAllowed,
    /// Records that toy grant denied occurred.
    ToyGrantDenied,
    /// Records that toy grant expired occurred.
    ToyGrantExpired,
    /// Records that toy grant revoked occurred.
    ToyGrantRevoked,
    /// Records that toy call started occurred.
    ToyCallStarted,
    /// Records that toy call completed occurred.
    ToyCallCompleted,
    /// Records that toy call failed occurred.
    ToyCallFailed,
    /// Records that data movement allowed occurred.
    DataMovementAllowed,
    /// Records that data movement denied occurred.
    DataMovementDenied,
    /// Records that secret access allowed occurred.
    SecretAccessAllowed,
    /// Records that secret access denied occurred.
    SecretAccessDenied,
    /// Records that peer connected occurred.
    PeerConnected,
    /// Records that peer hello received occurred.
    PeerHelloReceived,
    /// Records that peer protocol negotiated occurred.
    PeerProtocolNegotiated,
    /// Records that peer hello responded occurred.
    PeerHelloResponded,
    /// Records that peer binding recorded occurred.
    PeerBindingRecorded,
    /// Records that peer binding revoked occurred.
    PeerBindingRevoked,
    /// Records that peer binding expired occurred.
    PeerBindingExpired,
    /// Records that peer admitted occurred.
    PeerAdmitted,
    /// Records that peer rejected occurred.
    PeerRejected,
    /// Records that peer call sent occurred.
    PeerCallSent,
    /// Records that peer call received occurred.
    PeerCallReceived,
    /// Records that peer call malformed occurred.
    PeerCallMalformed,
    /// Records that peer call replied occurred.
    PeerCallReplied,
    /// Records that peer stream opened occurred.
    PeerStreamOpened,
    /// Records that peer stream reset occurred.
    PeerStreamReset,
    /// Records that iroh path observed occurred.
    IrohPathObserved,
    /// Records that runtime execution started occurred.
    RuntimeExecutionStarted,
    /// Records that runtime execution completed occurred.
    RuntimeExecutionCompleted,
    /// Records that runtime execution failed occurred.
    RuntimeExecutionFailed,
    /// Records that runtime execution trapped occurred.
    RuntimeExecutionTrapped,
    /// Records that runtime execution timed out occurred.
    RuntimeExecutionTimedOut,
    /// Records that adapter effect started occurred.
    AdapterEffectStarted,
    /// Records that adapter effect completed occurred.
    AdapterEffectCompleted,
    /// Records that adapter effect failed occurred.
    AdapterEffectFailed,
    /// Records that storage append succeeded occurred.
    StorageAppendSucceeded,
    /// Records that storage append failed occurred.
    StorageAppendFailed,
    /// Records that observation backpressure applied occurred.
    ObservationBackpressureApplied,
    /// Records that lifecycle transition recorded occurred.
    LifecycleTransitionRecorded,
    /// Records that node health reported occurred.
    NodeHealthReported,
    /// Records that operator action recorded occurred.
    OperatorActionRecorded,
    /// Records that telemetry exported occurred.
    TelemetryExported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Layer that produced the observation fact.
pub enum SourcePlane {
    /// Pure kernel decision or projection.
    Kernel,
    /// Adapter boundary or runtime effect.
    Adapter,
    /// Peer protocol event.
    Peer,
    /// Child lifecycle or invocation event.
    Child,
    /// Toy grant or effect event.
    Toy,
    /// Storage or ledger event.
    Storage,
    /// Operator action or health projection.
    Operator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Outcome class recorded for an observation.
pub enum ObservationOutcome {
    /// Authority allowed the action.
    Allowed,
    /// Authority denied or failed closed.
    Denied,
    /// Effect or execution started.
    Started,
    /// Effect or execution completed.
    Completed,
    /// Effect or execution failed.
    Failed,
    /// Effect or execution timed out.
    TimedOut,
    /// Effect or execution was cancelled.
    Cancelled,
    /// Fact is informational rather than an authority outcome.
    Informational,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Audience boundary for projecting observation details.
pub enum ObservationVisibility {
    /// May be shown to the caller.
    CallerSafe,
    /// Visible to operators for the Vision.
    VisionOperator,
    /// Visible to operators of the local node.
    NodeOperator,
    /// Visible to system operators.
    SystemOperator,
    /// Internal diagnostic or audit detail only.
    InternalOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Durable typed fact written to the observation ledger.
///
/// Observations are the runtime truth for authority and adapter events. Safe
/// messages and visibility define disclosure; privileged details remain behind
/// `detail_ref` or typed records elsewhere.
pub struct MctObservation {
    /// Unique observation identifier for ledger correlation.
    pub observation_id: ObservationId,
    /// Adapter-supplied time when the fact was observed.
    pub observed_at: Timestamp,
    /// Event taxonomy value for this fact.
    pub kind: ObservationKind,
    /// Layer that produced the observation.
    pub source_plane: SourcePlane,
    /// Trace linkage for reconstructing call flow.
    pub trace: ObservationTraceRef,
    /// Call associated with the fact, when any.
    pub call_id: Option<CallId>,
    /// Authority decision associated with the fact, when any.
    pub decision_id: Option<DecisionId>,
    /// Subject of the fact, such as child, peer, or grant.
    pub subject_id: Option<String>,
    /// Resource affected or considered by the fact.
    pub resource_id: Option<String>,
    /// Policy revision relevant to the fact, when any.
    pub policy_revision: Option<u64>,
    /// Grants revision relevant to the fact, when any.
    pub grants_revision: Option<u64>,
    /// Outcome class for projection and filtering.
    pub outcome: ObservationOutcome,
    /// Maximum intended audience for this observation.
    pub visibility: ObservationVisibility,
    /// Audience-safe summary for projections.
    pub safe_message: String,
    /// Opaque reference to privileged detail outside this record.
    pub detail_ref: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Adapter failure class projected into an observation.
pub enum AdapterDiagnosticKind {
    /// Iroh stream reset before protocol completion.
    IrohStreamReset,
    /// WASM runtime trapped during execution.
    WasmTrap,
    /// Configured WASM export was absent.
    WasmMissingExport,
    /// Component requested a missing host import.
    WasmMissingHostImport,
    /// WIT value conversion failed.
    WasmValueConversionFailure,
    /// Process child exited unsuccessfully.
    ProcessExitFailure,
    /// JVM-backed child timed out.
    JvmTimeout,
    /// Storage append failed.
    StorageAppendFailure,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Input facts for projecting an adapter diagnostic into an observation.
pub struct AdapterDiagnosticObservationInput {
    /// Unique observation identifier for ledger correlation.
    pub observation_id: ObservationId,
    /// Adapter-supplied time when the fact was observed.
    pub observed_at: Timestamp,
    /// Adapter diagnostic class to project.
    pub diagnostic_kind: AdapterDiagnosticKind,
    /// Trace linkage for reconstructing call flow.
    pub trace: ObservationTraceRef,
    /// Call associated with the fact, when any.
    pub call_id: Option<CallId>,
    /// Authority decision associated with the fact, when any.
    pub decision_id: Option<DecisionId>,
    /// Subject of the fact, such as child, peer, or grant.
    pub subject_id: Option<String>,
    /// Resource affected or considered by the fact.
    pub resource_id: Option<String>,
    /// Policy revision relevant to the fact, when any.
    pub policy_revision: Option<u64>,
    /// Grants revision relevant to the fact, when any.
    pub grants_revision: Option<u64>,
    /// Opaque reference to privileged detail outside this record.
    pub detail_ref: Option<String>,
}

impl MctObservation {
    /// Builds an internal informational observation with no call or decision context.
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

/// Projects an adapter diagnostic into the stable observation taxonomy.
pub fn adapter_diagnostic_observation(input: AdapterDiagnosticObservationInput) -> MctObservation {
    let (kind, source_plane, outcome, safe_message) = match input.diagnostic_kind {
        AdapterDiagnosticKind::IrohStreamReset => (
            ObservationKind::PeerStreamReset,
            SourcePlane::Adapter,
            ObservationOutcome::Failed,
            "peer stream reset",
        ),
        AdapterDiagnosticKind::WasmTrap => (
            ObservationKind::RuntimeExecutionTrapped,
            SourcePlane::Adapter,
            ObservationOutcome::Failed,
            "wasm execution trapped",
        ),
        AdapterDiagnosticKind::WasmMissingExport => (
            ObservationKind::RuntimeExecutionFailed,
            SourcePlane::Adapter,
            ObservationOutcome::Failed,
            "wasm export missing",
        ),
        AdapterDiagnosticKind::WasmMissingHostImport => (
            ObservationKind::RuntimeExecutionFailed,
            SourcePlane::Adapter,
            ObservationOutcome::Failed,
            "wasm host import missing",
        ),
        AdapterDiagnosticKind::WasmValueConversionFailure => (
            ObservationKind::RuntimeExecutionFailed,
            SourcePlane::Adapter,
            ObservationOutcome::Failed,
            "wasm value conversion failed",
        ),
        AdapterDiagnosticKind::ProcessExitFailure => (
            ObservationKind::RuntimeExecutionFailed,
            SourcePlane::Adapter,
            ObservationOutcome::Failed,
            "process execution failed",
        ),
        AdapterDiagnosticKind::JvmTimeout => (
            ObservationKind::RuntimeExecutionTimedOut,
            SourcePlane::Adapter,
            ObservationOutcome::TimedOut,
            "jvm execution timed out",
        ),
        AdapterDiagnosticKind::StorageAppendFailure => (
            ObservationKind::StorageAppendFailed,
            SourcePlane::Storage,
            ObservationOutcome::Failed,
            "storage append failed",
        ),
    };

    MctObservation {
        observation_id: input.observation_id,
        observed_at: input.observed_at,
        kind,
        source_plane,
        trace: input.trace,
        call_id: input.call_id,
        decision_id: input.decision_id,
        subject_id: input.subject_id,
        resource_id: input.resource_id,
        policy_revision: input.policy_revision,
        grants_revision: input.grants_revision,
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: input.detail_ref,
    }
}

/// Projects a hello admission evaluation into an observation fact.
pub fn hello_evaluation_observation(
    trace_id: TraceId,
    observed_at: Timestamp,
    evaluation: &MctHelloAdmissionEvaluation,
) -> MctObservation {
    let admitted = evaluation.is_admitted();
    MctObservation {
        observation_id: evaluation.observation_id.clone(),
        observed_at,
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

/// Projects a peer binding lifecycle state into an observation fact.
pub fn peer_binding_state_observation(
    trace_id: TraceId,
    observed_at: Timestamp,
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

/// Projects candidate consideration into an observation fact.
pub fn candidate_considered_observation(
    trace_id: TraceId,
    observed_at: Timestamp,
    call: &MctCall,
    candidate: &CandidateRoute,
    observation_id: ObservationId,
    policy_revision: u64,
    grants_revision: u64,
) -> MctObservation {
    MctObservation {
        observation_id,
        observed_at,
        kind: ObservationKind::CandidateConsidered,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id,
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: None,
        subject_id: candidate.child_id.as_ref().map(ToString::to_string),
        resource_id: Some(candidate.candidate_id.clone()),
        policy_revision: Some(policy_revision),
        grants_revision: Some(grants_revision),
        outcome: ObservationOutcome::Informational,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: "candidate considered".into(),
        detail_ref: Some(format!(
            "candidate:{};node:{};runtime:{:?};network:{:?}",
            candidate.candidate_id,
            candidate.node_id,
            candidate.runtime_kind,
            candidate.network_path
        )),
    }
}

/// Projects candidate elimination into an observation fact with the specific rule class.
pub fn candidate_eliminated_observation(
    trace_id: TraceId,
    observed_at: Timestamp,
    call: &MctCall,
    evaluation: &CandidateAuthorityEvaluation,
    observation_id: ObservationId,
) -> MctObservation {
    let reason = evaluation
        .reason
        .unwrap_or(CandidateEliminationReason::RouteMismatch);
    MctObservation {
        observation_id,
        observed_at,
        kind: ObservationKind::CandidateEliminated,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id,
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: None,
        subject_id: evaluation
            .candidate
            .child_id
            .as_ref()
            .map(ToString::to_string),
        resource_id: Some(evaluation.candidate.candidate_id.clone()),
        policy_revision: Some(evaluation.policy_revision),
        grants_revision: Some(evaluation.grants_revision),
        outcome: ObservationOutcome::Denied,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: evaluation.safe_message.clone(),
        detail_ref: Some(format!(
            "elimination_reason:{reason:?};denial_class:{}",
            reason.denial_class().as_str()
        )),
    }
}

/// Projects a route decision into an observation fact, preserving safe no-route detail.
pub fn route_decision_observation(
    trace_id: TraceId,
    observed_at: Timestamp,
    decision: &RouteDecision,
) -> MctObservation {
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
        observed_at,
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

/// Projects a call protocol evaluation into an observation fact.
pub fn call_protocol_evaluation_observation(
    trace_id: TraceId,
    observed_at: Timestamp,
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
        CallProtocolOutcome::Cancelled => (
            ObservationKind::ResultRecorded,
            ObservationOutcome::Cancelled,
        ),
    };

    MctObservation {
        observation_id: evaluation.observation_id.clone(),
        observed_at,
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

/// Projects a child approval authority record into an observation fact.
pub fn child_approval_observation(
    trace_id: TraceId,
    observed_at: Timestamp,
    approval: &ChildApproval,
) -> MctObservation {
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

/// Projects a child assignment authority record into an observation fact.
pub fn child_assignment_observation(
    trace_id: TraceId,
    observed_at: Timestamp,
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

/// Projects a child instance lifecycle state into an observation fact.
pub fn child_instance_observation(
    trace_id: TraceId,
    observed_at: Timestamp,
    instance: &ChildInstance,
) -> MctObservation {
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

/// Projects a child call authority evaluation into an observation fact.
pub fn child_call_authority_observation(
    trace_id: TraceId,
    observed_at: Timestamp,
    evaluation: &ChildCallAuthorityEvaluation,
) -> MctObservation {
    let allowed = evaluation.verdict == ChildCallVerdict::Allowed;
    MctObservation {
        observation_id: evaluation.observation_id.clone(),
        observed_at,
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

/// Projects a toy grant evaluation into an observation fact.
pub fn toy_grant_evaluation_observation(
    trace_id: TraceId,
    observed_at: Timestamp,
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
        observed_at,
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

    fn supplied_observed_at() -> Timestamp {
        Timestamp::new("2026-06-01T00:00:00Z").unwrap()
    }

    fn diagnostic_input(
        kind: AdapterDiagnosticKind,
        id: &str,
    ) -> AdapterDiagnosticObservationInput {
        AdapterDiagnosticObservationInput {
            observation_id: ObservationId::new(id)
                .expect("string ID literal/generated value must be non-empty"),
            observed_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            diagnostic_kind: kind,
            trace: ObservationTraceRef {
                trace_id: TraceId::new("trace-diagnostic")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: Some(
                    SpanId::new("span-diagnostic")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                parent_span_id: None,
                external_trace_id: None,
            },
            call_id: Some(
                CallId::new("call-diagnostic")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            decision_id: Some(
                DecisionId::new("decision-diagnostic")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            subject_id: Some("adapter-subject".into()),
            resource_id: Some("adapter-resource".into()),
            policy_revision: Some(3),
            grants_revision: Some(4),
            detail_ref: Some("detail:opaque".into()),
        }
    }

    #[test]
    fn adapter_diagnostic_observation_covers_failure_kinds() {
        let cases = [
            (
                AdapterDiagnosticKind::IrohStreamReset,
                ObservationKind::PeerStreamReset,
                SourcePlane::Adapter,
                ObservationOutcome::Failed,
                "peer stream reset",
            ),
            (
                AdapterDiagnosticKind::WasmTrap,
                ObservationKind::RuntimeExecutionTrapped,
                SourcePlane::Adapter,
                ObservationOutcome::Failed,
                "wasm execution trapped",
            ),
            (
                AdapterDiagnosticKind::WasmMissingExport,
                ObservationKind::RuntimeExecutionFailed,
                SourcePlane::Adapter,
                ObservationOutcome::Failed,
                "wasm export missing",
            ),
            (
                AdapterDiagnosticKind::WasmMissingHostImport,
                ObservationKind::RuntimeExecutionFailed,
                SourcePlane::Adapter,
                ObservationOutcome::Failed,
                "wasm host import missing",
            ),
            (
                AdapterDiagnosticKind::WasmValueConversionFailure,
                ObservationKind::RuntimeExecutionFailed,
                SourcePlane::Adapter,
                ObservationOutcome::Failed,
                "wasm value conversion failed",
            ),
            (
                AdapterDiagnosticKind::ProcessExitFailure,
                ObservationKind::RuntimeExecutionFailed,
                SourcePlane::Adapter,
                ObservationOutcome::Failed,
                "process execution failed",
            ),
            (
                AdapterDiagnosticKind::JvmTimeout,
                ObservationKind::RuntimeExecutionTimedOut,
                SourcePlane::Adapter,
                ObservationOutcome::TimedOut,
                "jvm execution timed out",
            ),
            (
                AdapterDiagnosticKind::StorageAppendFailure,
                ObservationKind::StorageAppendFailed,
                SourcePlane::Storage,
                ObservationOutcome::Failed,
                "storage append failed",
            ),
        ];

        for (index, (diagnostic, kind, source_plane, outcome, safe_message)) in
            cases.into_iter().enumerate()
        {
            let observation = adapter_diagnostic_observation(diagnostic_input(
                diagnostic,
                &format!("obs-diagnostic-{index}"),
            ));
            assert_eq!(observation.kind, kind);
            assert_eq!(observation.source_plane, source_plane);
            assert_eq!(observation.outcome, outcome);
            assert_eq!(observation.safe_message, safe_message);
            assert_eq!(observation.visibility, ObservationVisibility::InternalOnly);
            assert_eq!(observation.detail_ref, Some("detail:opaque".into()));
            assert_eq!(
                observation.call_id,
                Some(
                    CallId::new("call-diagnostic")
                        .expect("string ID literal/generated value must be non-empty")
                )
            );
            assert_eq!(observation.policy_revision, Some(3));
            assert_eq!(observation.grants_revision, Some(4));
        }
    }

    #[test]
    fn kernel_denial_evaluations_become_observations() {
        let hello = MctHelloAdmissionEvaluation {
            decision_id: DecisionId::new("decision-hello")
                .expect("string ID literal/generated value must be non-empty"),
            request_id: "hello-1".into(),
            peer_admission_decision_id: None,
            selected_binding_id: None,
            selected_node_id: None,
            selected_vision_id: None,
            selected_policy_revision: None,
            negotiated_protocol: None,
            accepted_alpns: Vec::new(),
            hello_outcome: HelloOutcome::Denied,
            reason: HelloReason::MissingBinding,
            safe_reason: SafeHelloReason::NotAuthorized,
            observation_id: ObservationId::new("obs-hello-denied")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let hello_observation = hello_evaluation_observation(
            TraceId::new("trace-1").expect("string ID literal/generated value must be non-empty"),
            supplied_observed_at(),
            &hello,
        );
        assert_eq!(hello_observation.kind, ObservationKind::PeerRejected);
        assert_eq!(hello_observation.source_plane, SourcePlane::Kernel);
        assert_eq!(hello_observation.outcome, ObservationOutcome::Denied);
        assert_eq!(
            hello_observation.decision_id,
            Some(
                DecisionId::new("decision-hello")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
        assert_eq!(hello_observation.safe_message, "not authorized");
        assert_eq!(hello_observation.observed_at, supplied_observed_at());

        let call = MctCallProtocolEvaluation {
            decision_id: DecisionId::new("decision-call")
                .expect("string ID literal/generated value must be non-empty"),
            protocol_request_id: ProtocolRequestId::new("proto-call")
                .expect("string ID literal/generated value must be non-empty"),
            call_id: Some(
                CallId::new("call-1").expect("string ID literal/generated value must be non-empty"),
            ),
            route_decision_id: None,
            result_ref: None,
            outcome: CallProtocolOutcome::Denied,
            reason: CallProtocolReason::HelloNotAdmitted,
            safe_message: "not authorized".into(),
            observation_id: ObservationId::new("obs-call-denied")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let call_observation = call_protocol_evaluation_observation(
            TraceId::new("trace-1").expect("string ID literal/generated value must be non-empty"),
            supplied_observed_at(),
            &call,
        );
        assert_eq!(call_observation.kind, ObservationKind::CallDenied);
        assert_eq!(call_observation.source_plane, SourcePlane::Kernel);
        assert_eq!(call_observation.outcome, ObservationOutcome::Denied);
        assert_eq!(
            call_observation.call_id,
            Some(
                CallId::new("call-1").expect("string ID literal/generated value must be non-empty")
            )
        );
        assert_eq!(
            call_observation.decision_id,
            Some(
                DecisionId::new("decision-call")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
        assert_eq!(call_observation.observed_at, supplied_observed_at());
    }

    fn binding(state: BindingState) -> MctPeerBinding {
        MctPeerBinding {
            binding_id: PeerBindingId::new("binding-1")
                .expect("string ID literal/generated value must be non-empty"),
            iroh_endpoint_id: EndpointIdText::new("endpoint-a")
                .expect("string ID literal/generated value must be non-empty"),
            scope: MctPeerBindingScope {
                mct_node_id: MctNodeId::new("node-b")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
                data_scope: None,
                observation_scope: None,
            },
            issuer_node_id: MctNodeId::new("node-a")
                .expect("string ID literal/generated value must be non-empty"),
            policy_revision: 7,
            binding_state: state,
            issued_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            expires_at: Timestamp::new("2026-05-31T00:05:00Z").unwrap(),
            created_by_observation_id: ObservationId::new("obs-binding-created")
                .expect("string ID literal/generated value must be non-empty"),
            superseded_by_observation_id: Some(
                ObservationId::new("obs-binding-superseded")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
        }
    }

    #[test]
    fn no_route_decision_becomes_observation() {
        let call = MctCall {
            call_id: CallId::new("call-no-route")
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
                size_bytes: 5,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 3,
                grants_revision: 4,
                vision_policy_revision: 5,
            },
            deadline: Timestamp::new("2026-05-31T00:01:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-no-route")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: SpanId::new("span-no-route")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            origin: CallOrigin::Cli,
        };
        let candidate = CandidateRoute {
            candidate_id: "candidate-denied".into(),
            node_id: MctNodeId::new("node-b")
                .expect("string ID literal/generated value must be non-empty"),
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
                decision_id: DecisionId::new("route-decision-no-route")
                    .expect("string ID literal/generated value must be non-empty"),
                observation_id: ObservationId::new("obs-route-no-route")
                    .expect("string ID literal/generated value must be non-empty"),
            },
        );
        let observation = route_decision_observation(
            TraceId::new("trace-no-route")
                .expect("string ID literal/generated value must be non-empty"),
            supplied_observed_at(),
            &decision,
        );

        assert_eq!(observation.kind, ObservationKind::NoRouteRecorded);
        assert_eq!(observation.outcome, ObservationOutcome::Denied);
        assert_eq!(
            observation.call_id,
            Some(
                CallId::new("call-no-route")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
        assert_eq!(
            observation.decision_id,
            Some(
                DecisionId::new("route-decision-no-route")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
        assert_eq!(observation.safe_message, "not authorized");
        assert_eq!(observation.policy_revision, Some(3));
        assert_eq!(observation.grants_revision, Some(4));
        assert_eq!(observation.observed_at, supplied_observed_at());
    }

    #[test]
    fn candidate_observations_record_specific_elimination_class() {
        let call = MctCall {
            call_id: CallId::new("call-candidate-eliminated")
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
                size_bytes: 5,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 3,
                grants_revision: 4,
                vision_policy_revision: 5,
            },
            deadline: Timestamp::new("2026-05-31T00:01:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-candidate-eliminated")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: SpanId::new("span-candidate-eliminated")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            origin: CallOrigin::Cli,
        };
        let candidate = CandidateRoute {
            candidate_id: "candidate-unavailable".into(),
            node_id: MctNodeId::new("node-b")
                .expect("string ID literal/generated value must be non-empty"),
            child_id: Some(
                ChildId::new("child-echo")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            runtime_kind: RuntimeKind::Process,
            network_path: NetworkPathClass::Local,
        };
        let considered = candidate_considered_observation(
            call.trace_context.trace_id.clone(),
            supplied_observed_at(),
            &call,
            &candidate,
            ObservationId::new("obs-candidate-considered")
                .expect("string ID literal/generated value must be non-empty"),
            3,
            4,
        );
        assert_eq!(considered.kind, ObservationKind::CandidateConsidered);
        assert_eq!(considered.outcome, ObservationOutcome::Informational);
        assert_eq!(considered.resource_id, Some("candidate-unavailable".into()));

        let eliminated = CandidateAuthorityEvaluation::eliminated(
            candidate,
            CandidateEliminationReason::CapabilityUnavailable,
            3,
            4,
        );
        let observation = candidate_eliminated_observation(
            call.trace_context.trace_id.clone(),
            supplied_observed_at(),
            &call,
            &eliminated,
            ObservationId::new("obs-candidate-eliminated")
                .expect("string ID literal/generated value must be non-empty"),
        );

        assert_eq!(observation.kind, ObservationKind::CandidateEliminated);
        assert_eq!(observation.outcome, ObservationOutcome::Denied);
        assert_eq!(observation.safe_message, "not authorized");
        assert_eq!(observation.policy_revision, Some(3));
        assert_eq!(observation.grants_revision, Some(4));
        assert_eq!(
            observation.detail_ref,
            Some("elimination_reason:CapabilityUnavailable;denial_class:temporal".into())
        );
    }

    #[test]
    fn route_revalidation_observation_records_allowed_and_denied_outcomes() {
        let trace_id = TraceId::new("trace-revalidation")
            .expect("string ID literal/generated value must be non-empty");
        let candidate = CandidateRoute {
            candidate_id: "candidate-revalidated".into(),
            node_id: MctNodeId::new("node-b")
                .expect("string ID literal/generated value must be non-empty"),
            child_id: Some(
                ChildId::new("child-echo")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            runtime_kind: RuntimeKind::Process,
            network_path: NetworkPathClass::Local,
        };
        let allowed = RouteDecision {
            decision_id: DecisionId::new("route-revalidation-allowed")
                .expect("string ID literal/generated value must be non-empty"),
            call_id: CallId::new("call-revalidation")
                .expect("string ID literal/generated value must be non-empty"),
            decision_kind: RouteDecisionKind::Revalidation,
            initial_decision_id: Some(
                DecisionId::new("route-initial")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            authority_evaluations: vec![CandidateAuthorityEvaluation::admissible(
                candidate.clone(),
                3,
                4,
            )],
            selected_route: Some(candidate.clone()),
            outcome: RouteDecisionOutcome::RouteSelected,
            no_route_reason: None,
            safe_message: "route revalidated".into(),
            observation_id: ObservationId::new("obs-route-revalidated")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let allowed_observation =
            route_decision_observation(trace_id.clone(), supplied_observed_at(), &allowed);
        assert_eq!(allowed_observation.kind, ObservationKind::RouteRevalidated);
        assert_eq!(allowed_observation.outcome, ObservationOutcome::Allowed);
        assert_eq!(allowed_observation.policy_revision, Some(3));
        assert_eq!(allowed_observation.grants_revision, Some(4));
        assert_eq!(
            allowed_observation.detail_ref,
            Some("initial_decision:route-initial;revalidated".into())
        );

        let denied = RouteDecision {
            decision_id: DecisionId::new("route-revalidation-denied")
                .expect("string ID literal/generated value must be non-empty"),
            call_id: CallId::new("call-revalidation")
                .expect("string ID literal/generated value must be non-empty"),
            decision_kind: RouteDecisionKind::Revalidation,
            initial_decision_id: Some(
                DecisionId::new("route-initial")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
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
            observation_id: ObservationId::new("obs-route-revalidation-denied")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let denied_observation =
            route_decision_observation(trace_id, supplied_observed_at(), &denied);
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
            TraceId::new("trace-1").expect("string ID literal/generated value must be non-empty"),
            supplied_observed_at(),
            &binding(BindingState::Revoked),
        );
        assert_eq!(revoked.kind, ObservationKind::PeerBindingRevoked);
        assert_eq!(revoked.outcome, ObservationOutcome::Denied);
        assert_eq!(revoked.safe_message, "not authorized");
        assert_eq!(revoked.policy_revision, Some(7));
        assert_eq!(revoked.observed_at, supplied_observed_at());

        let expired = peer_binding_state_observation(
            TraceId::new("trace-1").expect("string ID literal/generated value must be non-empty"),
            supplied_observed_at(),
            &binding(BindingState::Expired),
        );
        assert_eq!(expired.kind, ObservationKind::PeerBindingExpired);
        assert_eq!(expired.outcome, ObservationOutcome::Denied);
        assert_eq!(expired.resource_id, Some("endpoint-a".into()));
        assert_eq!(expired.observed_at, supplied_observed_at());
    }

    #[test]
    fn toy_grant_evaluations_become_observations() {
        let allowed = ToyGrantEvaluation {
            evaluation_id: ToyGrantEvaluationId::new("toy-eval-1")
                .expect("string ID literal/generated value must be non-empty"),
            call_id: CallId::new("call-toy")
                .expect("string ID literal/generated value must be non-empty"),
            decision_id: DecisionId::new("decision-toy")
                .expect("string ID literal/generated value must be non-empty"),
            grant_id: Some(
                ToyGrantId::new("grant-toy")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            toy_id: ToyId::new("toy-logging")
                .expect("string ID literal/generated value must be non-empty"),
            subject_child_name: "slate-manager".into(),
            verdict: ToyGrantVerdict::Allowed,
            reason_code: ToyGrantReasonCode::ActiveGrant,
            policy_revision: 3,
            grants_revision: 7,
            observation_id: ObservationId::new("obs-toy-allowed")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let allowed_observation = toy_grant_evaluation_observation(
            TraceId::new("trace-toy").expect("string ID literal/generated value must be non-empty"),
            supplied_observed_at(),
            &allowed,
        );
        assert_eq!(allowed_observation.kind, ObservationKind::ToyGrantAllowed);
        assert_eq!(allowed_observation.outcome, ObservationOutcome::Allowed);
        assert_eq!(
            allowed_observation.call_id,
            Some(
                CallId::new("call-toy")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
        assert_eq!(allowed_observation.resource_id, Some("toy-logging".into()));
        assert_eq!(allowed_observation.observed_at, supplied_observed_at());

        let mut denied = allowed;
        denied.verdict = ToyGrantVerdict::Denied;
        denied.reason_code = ToyGrantReasonCode::MissingGrant;
        denied.observation_id = ObservationId::new("obs-toy-denied")
            .expect("string ID literal/generated value must be non-empty");
        let denied_observation = toy_grant_evaluation_observation(
            TraceId::new("trace-toy").expect("string ID literal/generated value must be non-empty"),
            supplied_observed_at(),
            &denied,
        );
        assert_eq!(denied_observation.kind, ObservationKind::ToyGrantDenied);
        assert_eq!(denied_observation.outcome, ObservationOutcome::Denied);
        assert_eq!(denied_observation.safe_message, "not authorized");
        assert_eq!(denied_observation.observed_at, supplied_observed_at());
    }

    /// Covers `MctToyGrantAuthority.ToyGrantDecisionsAreObserved`.
    #[test]
    fn toy_grant_observation_matrix_distinguishes_expiry_and_revocation() {
        let mut evaluation = ToyGrantEvaluation {
            evaluation_id: ToyGrantEvaluationId::new("toy-eval-matrix").unwrap(),
            call_id: CallId::new("call-toy-matrix").unwrap(),
            decision_id: DecisionId::new("decision-toy-matrix").unwrap(),
            grant_id: Some(ToyGrantId::new("grant-toy-matrix").unwrap()),
            toy_id: ToyId::new("toy-matrix").unwrap(),
            subject_child_name: "matrix-child".into(),
            verdict: ToyGrantVerdict::Allowed,
            reason_code: ToyGrantReasonCode::ActiveGrant,
            policy_revision: 3,
            grants_revision: 7,
            observation_id: ObservationId::new("obs-toy-matrix").unwrap(),
        };

        for (verdict, reason, kind, outcome) in [
            (
                ToyGrantVerdict::Allowed,
                ToyGrantReasonCode::ActiveGrant,
                ObservationKind::ToyGrantAllowed,
                ObservationOutcome::Allowed,
            ),
            (
                ToyGrantVerdict::Denied,
                ToyGrantReasonCode::MissingGrant,
                ObservationKind::ToyGrantDenied,
                ObservationOutcome::Denied,
            ),
            (
                ToyGrantVerdict::Denied,
                ToyGrantReasonCode::ExpiredGrant,
                ObservationKind::ToyGrantExpired,
                ObservationOutcome::Denied,
            ),
            (
                ToyGrantVerdict::Denied,
                ToyGrantReasonCode::RevokedGrant,
                ObservationKind::ToyGrantRevoked,
                ObservationOutcome::Denied,
            ),
        ] {
            evaluation.verdict = verdict;
            evaluation.reason_code = reason;
            let observation = toy_grant_evaluation_observation(
                TraceId::new("trace-toy-matrix").unwrap(),
                supplied_observed_at(),
                &evaluation,
            );
            assert_eq!((observation.kind, observation.outcome), (kind, outcome));
            assert_eq!(observation.policy_revision, Some(3));
            assert_eq!(observation.grants_revision, Some(7));
        }
    }

    #[test]
    fn child_lifecycle_and_call_authority_become_observations() {
        let approval = ChildApproval {
            approval_id: ChildApprovalId::new("approval-child")
                .expect("string ID literal/generated value must be non-empty"),
            artifact_id: ComponentArtifactId::new("artifact-child")
                .expect("string ID literal/generated value must be non-empty"),
            child_name: "slate-manager".into(),
            artifact_version: "0.2.0".into(),
            scope_vision_id: Some(
                VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            scope_node_id: Some(
                MctNodeId::new("node-a")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            scope_project_id: None,
            approval_state: ChildApprovalState::Approved,
            policy_revision: 5,
            authority_observation_id: ObservationId::new("obs-child-approved")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let approval_observation = child_approval_observation(
            TraceId::new("trace-child")
                .expect("string ID literal/generated value must be non-empty"),
            supplied_observed_at(),
            &approval,
        );
        assert_eq!(approval_observation.kind, ObservationKind::ChildApproved);
        assert_eq!(approval_observation.outcome, ObservationOutcome::Allowed);
        assert_eq!(
            approval_observation.resource_id,
            Some("artifact-child".into())
        );
        assert_eq!(approval_observation.observed_at, supplied_observed_at());

        let assignment = ChildAssignment {
            assignment_id: ChildAssignmentId::new("assignment-child")
                .expect("string ID literal/generated value must be non-empty"),
            approval_id: ChildApprovalId::new("approval-child")
                .expect("string ID literal/generated value must be non-empty"),
            artifact_id: ComponentArtifactId::new("artifact-child")
                .expect("string ID literal/generated value must be non-empty"),
            child_name: "slate-manager".into(),
            vision_id: VisionId::new("vision-a")
                .expect("string ID literal/generated value must be non-empty"),
            node_id: Some(
                MctNodeId::new("node-a")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            project_id: None,
            assignment_state: ChildAssignmentState::Active,
            pinned_artifact_version: "0.2.0".into(),
            assignment_observation_id: ObservationId::new("obs-child-assigned")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let assignment_observation = child_assignment_observation(
            TraceId::new("trace-child")
                .expect("string ID literal/generated value must be non-empty"),
            supplied_observed_at(),
            &assignment,
        );
        assert_eq!(assignment_observation.kind, ObservationKind::ChildAssigned);
        assert_eq!(assignment_observation.outcome, ObservationOutcome::Allowed);
        assert_eq!(assignment_observation.observed_at, supplied_observed_at());

        let instance = ChildInstance {
            instance_id: ChildInstanceId::new("instance-child")
                .expect("string ID literal/generated value must be non-empty"),
            assignment_id: ChildAssignmentId::new("assignment-child")
                .expect("string ID literal/generated value must be non-empty"),
            artifact_id: ComponentArtifactId::new("artifact-child")
                .expect("string ID literal/generated value must be non-empty"),
            child_name: "slate-manager".into(),
            generation: 1,
            node_id: MctNodeId::new("node-a")
                .expect("string ID literal/generated value must be non-empty"),
            instance_state: ChildInstanceState::Ready,
            readiness_observation_id: Some(
                ObservationId::new("obs-child-ready")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            last_lifecycle_observation_id: ObservationId::new("obs-child-ready")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let instance_observation = child_instance_observation(
            TraceId::new("trace-child")
                .expect("string ID literal/generated value must be non-empty"),
            supplied_observed_at(),
            &instance,
        );
        assert_eq!(
            instance_observation.kind,
            ObservationKind::ChildInstanceReady
        );
        assert_eq!(instance_observation.outcome, ObservationOutcome::Allowed);
        assert_eq!(instance_observation.observed_at, supplied_observed_at());

        let evaluation = ChildCallAuthorityEvaluation {
            evaluation_id: ChildCallEvaluationId::new("child-eval")
                .expect("string ID literal/generated value must be non-empty"),
            call_id: CallId::new("call-child")
                .expect("string ID literal/generated value must be non-empty"),
            decision_id: DecisionId::new("decision-child")
                .expect("string ID literal/generated value must be non-empty"),
            instance_id: Some(
                ChildInstanceId::new("instance-child")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            assignment_id: Some(
                ChildAssignmentId::new("assignment-child")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            approval_id: Some(
                ChildApprovalId::new("approval-child")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            artifact_id: Some(
                ComponentArtifactId::new("artifact-child")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            child_name: Some("slate-manager".into()),
            verdict: ChildCallVerdict::Denied,
            reason_code: ChildCallReasonCode::InstanceNotReady,
            policy_revision: 5,
            observation_id: ObservationId::new("obs-child-denied")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let denial_observation = child_call_authority_observation(
            TraceId::new("trace-child")
                .expect("string ID literal/generated value must be non-empty"),
            supplied_observed_at(),
            &evaluation,
        );
        assert_eq!(denial_observation.kind, ObservationKind::CallDenied);
        assert_eq!(denial_observation.outcome, ObservationOutcome::Denied);
        assert_eq!(denial_observation.safe_message, "not authorized");
        assert_eq!(denial_observation.observed_at, supplied_observed_at());
    }

    /// Covers `MctChildComponentLifecycle.LifecycleTransitionsAreObserved` and
    /// `MctObservationSubsystemCoverage.ChildLifecycleCoverage`.
    #[test]
    fn child_authority_and_instance_observation_matrix_is_typed() {
        let mut approval = ChildApproval {
            approval_id: ChildApprovalId::new("approval-matrix").unwrap(),
            artifact_id: ComponentArtifactId::new("artifact-matrix").unwrap(),
            child_name: "matrix-child".into(),
            artifact_version: "1.0.0".into(),
            scope_vision_id: None,
            scope_node_id: None,
            scope_project_id: None,
            approval_state: ChildApprovalState::Candidate,
            policy_revision: 1,
            authority_observation_id: ObservationId::new("obs-approval-matrix").unwrap(),
        };
        for (state, kind, outcome) in [
            (
                ChildApprovalState::Candidate,
                ObservationKind::LifecycleTransitionRecorded,
                ObservationOutcome::Informational,
            ),
            (
                ChildApprovalState::Approved,
                ObservationKind::ChildApproved,
                ObservationOutcome::Allowed,
            ),
            (
                ChildApprovalState::Blocked,
                ObservationKind::ChildRevoked,
                ObservationOutcome::Denied,
            ),
            (
                ChildApprovalState::Revoked,
                ObservationKind::ChildRevoked,
                ObservationOutcome::Denied,
            ),
            (
                ChildApprovalState::Deprecated,
                ObservationKind::LifecycleTransitionRecorded,
                ObservationOutcome::Informational,
            ),
        ] {
            approval.approval_state = state;
            let observation = child_approval_observation(
                TraceId::new("trace-child-matrix").unwrap(),
                supplied_observed_at(),
                &approval,
            );
            assert_eq!((observation.kind, observation.outcome), (kind, outcome));
        }

        let mut assignment = ChildAssignment {
            assignment_id: ChildAssignmentId::new("assignment-matrix").unwrap(),
            approval_id: approval.approval_id.clone(),
            artifact_id: approval.artifact_id.clone(),
            child_name: approval.child_name.clone(),
            vision_id: VisionId::new("vision-matrix").unwrap(),
            node_id: None,
            project_id: None,
            assignment_state: ChildAssignmentState::Active,
            pinned_artifact_version: approval.artifact_version.clone(),
            assignment_observation_id: ObservationId::new("obs-assignment-matrix").unwrap(),
        };
        for (state, kind, outcome) in [
            (
                ChildAssignmentState::Active,
                ObservationKind::ChildAssigned,
                ObservationOutcome::Allowed,
            ),
            (
                ChildAssignmentState::Revoked,
                ObservationKind::ChildAssignmentRevoked,
                ObservationOutcome::Denied,
            ),
        ] {
            assignment.assignment_state = state;
            let observation = child_assignment_observation(
                TraceId::new("trace-child-matrix").unwrap(),
                supplied_observed_at(),
                &assignment,
            );
            assert_eq!((observation.kind, observation.outcome), (kind, outcome));
        }

        let mut instance = ChildInstance {
            instance_id: ChildInstanceId::new("instance-matrix").unwrap(),
            assignment_id: assignment.assignment_id,
            artifact_id: approval.artifact_id,
            child_name: approval.child_name,
            generation: 1,
            node_id: MctNodeId::new("node-matrix").unwrap(),
            instance_state: ChildInstanceState::Loading,
            readiness_observation_id: None,
            last_lifecycle_observation_id: ObservationId::new("obs-instance-matrix").unwrap(),
        };
        for (state, kind, outcome) in [
            (
                ChildInstanceState::Loading,
                ObservationKind::ChildInstanceLoading,
                ObservationOutcome::Started,
            ),
            (
                ChildInstanceState::Ready,
                ObservationKind::ChildInstanceReady,
                ObservationOutcome::Allowed,
            ),
            (
                ChildInstanceState::Degraded,
                ObservationKind::ChildInstanceDegraded,
                ObservationOutcome::Failed,
            ),
            (
                ChildInstanceState::Draining,
                ObservationKind::ChildInstanceDraining,
                ObservationOutcome::Started,
            ),
            (
                ChildInstanceState::Stopped,
                ObservationKind::ChildInstanceStopped,
                ObservationOutcome::Completed,
            ),
            (
                ChildInstanceState::Failed,
                ObservationKind::ChildInstanceFailed,
                ObservationOutcome::Failed,
            ),
        ] {
            instance.instance_state = state;
            let observation = child_instance_observation(
                TraceId::new("trace-child-matrix").unwrap(),
                supplied_observed_at(),
                &instance,
            );
            assert_eq!((observation.kind, observation.outcome), (kind, outcome));
        }
    }

    #[test]
    fn observation_kind_uses_snake_case_wire_names() {
        let encoded = serde_json::to_string(&ObservationKind::PeerHelloReceived).unwrap();
        assert_eq!(encoded, "\"peer_hello_received\"");
    }
}

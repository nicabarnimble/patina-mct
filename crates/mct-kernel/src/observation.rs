use crate::{call::*, child::*, id::*, peer::*, route::*, toy::*};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `ObservationTraceRef` used by the MCT kernel.
pub struct ObservationTraceRef {
    /// Field `trace_id` of this domain record.
    pub trace_id: TraceId,
    /// Field `span_id` of this domain record.
    pub span_id: Option<SpanId>,
    /// Field `parent_span_id` of this domain record.
    pub parent_span_id: Option<SpanId>,
    /// Field `external_trace_id` of this domain record.
    pub external_trace_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `ObservationKind` used by the MCT kernel.
pub enum ObservationKind {
    /// Public `CallReceived` item.
    CallReceived,
    /// Public `CallRejected` item.
    CallRejected,
    /// Public `CallConstructed` item.
    CallConstructed,
    /// Public `CallAuthorized` item.
    CallAuthorized,
    /// Public `CallDenied` item.
    CallDenied,
    /// Public `CandidateConsidered` item.
    CandidateConsidered,
    /// Public `CandidateEliminated` item.
    CandidateEliminated,
    /// Public `RouteSelected` item.
    RouteSelected,
    /// Public `NoRouteRecorded` item.
    NoRouteRecorded,
    /// Public `RouteRevalidated` item.
    RouteRevalidated,
    /// Public `ResultRecorded` item.
    ResultRecorded,
    /// Public `ArtifactVerified` item.
    ArtifactVerified,
    /// Public `ArtifactRejected` item.
    ArtifactRejected,
    /// Public `ChildApproved` item.
    ChildApproved,
    /// Public `ChildRevoked` item.
    ChildRevoked,
    /// Public `ChildAssigned` item.
    ChildAssigned,
    /// Public `ChildAssignmentRevoked` item.
    ChildAssignmentRevoked,
    /// Public `ChildInstanceLoading` item.
    ChildInstanceLoading,
    /// Public `ChildInstanceReady` item.
    ChildInstanceReady,
    /// Public `ChildInstanceDegraded` item.
    ChildInstanceDegraded,
    /// Public `ChildInstanceDraining` item.
    ChildInstanceDraining,
    /// Public `ChildInstanceStopped` item.
    ChildInstanceStopped,
    /// Public `ChildInstanceFailed` item.
    ChildInstanceFailed,
    /// Public `ChildInvoked` item.
    ChildInvoked,
    /// Public `ToyGrantAllowed` item.
    ToyGrantAllowed,
    /// Public `ToyGrantDenied` item.
    ToyGrantDenied,
    /// Public `ToyGrantExpired` item.
    ToyGrantExpired,
    /// Public `ToyGrantRevoked` item.
    ToyGrantRevoked,
    /// Public `ToyCallStarted` item.
    ToyCallStarted,
    /// Public `ToyCallCompleted` item.
    ToyCallCompleted,
    /// Public `ToyCallFailed` item.
    ToyCallFailed,
    /// Public `DataMovementAllowed` item.
    DataMovementAllowed,
    /// Public `DataMovementDenied` item.
    DataMovementDenied,
    /// Public `SecretAccessAllowed` item.
    SecretAccessAllowed,
    /// Public `SecretAccessDenied` item.
    SecretAccessDenied,
    /// Public `PeerConnected` item.
    PeerConnected,
    /// Public `PeerHelloReceived` item.
    PeerHelloReceived,
    /// Public `PeerProtocolNegotiated` item.
    PeerProtocolNegotiated,
    /// Public `PeerHelloResponded` item.
    PeerHelloResponded,
    /// Public `PeerBindingRecorded` item.
    PeerBindingRecorded,
    /// Public `PeerBindingRevoked` item.
    PeerBindingRevoked,
    /// Public `PeerBindingExpired` item.
    PeerBindingExpired,
    /// Public `PeerAdmitted` item.
    PeerAdmitted,
    /// Public `PeerRejected` item.
    PeerRejected,
    /// Public `PeerCallSent` item.
    PeerCallSent,
    /// Public `PeerCallReceived` item.
    PeerCallReceived,
    /// Public `PeerCallMalformed` item.
    PeerCallMalformed,
    /// Public `PeerCallReplied` item.
    PeerCallReplied,
    /// Public `PeerStreamOpened` item.
    PeerStreamOpened,
    /// Public `PeerStreamReset` item.
    PeerStreamReset,
    /// Public `IrohPathObserved` item.
    IrohPathObserved,
    /// Public `RuntimeExecutionStarted` item.
    RuntimeExecutionStarted,
    /// Public `RuntimeExecutionCompleted` item.
    RuntimeExecutionCompleted,
    /// Public `RuntimeExecutionFailed` item.
    RuntimeExecutionFailed,
    /// Public `RuntimeExecutionTrapped` item.
    RuntimeExecutionTrapped,
    /// Public `RuntimeExecutionTimedOut` item.
    RuntimeExecutionTimedOut,
    /// Public `AdapterEffectStarted` item.
    AdapterEffectStarted,
    /// Public `AdapterEffectCompleted` item.
    AdapterEffectCompleted,
    /// Public `AdapterEffectFailed` item.
    AdapterEffectFailed,
    /// Public `StorageAppendSucceeded` item.
    StorageAppendSucceeded,
    /// Public `StorageAppendFailed` item.
    StorageAppendFailed,
    /// Public `ObservationBackpressureApplied` item.
    ObservationBackpressureApplied,
    /// Public `LifecycleTransitionRecorded` item.
    LifecycleTransitionRecorded,
    /// Public `NodeHealthReported` item.
    NodeHealthReported,
    /// Public `OperatorActionRecorded` item.
    OperatorActionRecorded,
    /// Public `TelemetryExported` item.
    TelemetryExported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `SourcePlane` used by the MCT kernel.
pub enum SourcePlane {
    /// Public `Kernel` item.
    Kernel,
    /// Public `Adapter` item.
    Adapter,
    /// Public `Peer` item.
    Peer,
    /// Public `Child` item.
    Child,
    /// Public `Toy` item.
    Toy,
    /// Public `Storage` item.
    Storage,
    /// Public `Operator` item.
    Operator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `ObservationOutcome` used by the MCT kernel.
pub enum ObservationOutcome {
    /// Public `Allowed` item.
    Allowed,
    /// Public `Denied` item.
    Denied,
    /// Public `Started` item.
    Started,
    /// Public `Completed` item.
    Completed,
    /// Public `Failed` item.
    Failed,
    /// Public `TimedOut` item.
    TimedOut,
    /// Public `Cancelled` item.
    Cancelled,
    /// Public `Informational` item.
    Informational,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `ObservationVisibility` used by the MCT kernel.
pub enum ObservationVisibility {
    /// Public `CallerSafe` item.
    CallerSafe,
    /// Public `VisionOperator` item.
    VisionOperator,
    /// Public `NodeOperator` item.
    NodeOperator,
    /// Public `SystemOperator` item.
    SystemOperator,
    /// Public `InternalOnly` item.
    InternalOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctObservation` used by the MCT kernel.
pub struct MctObservation {
    /// Field `observation_id` of this domain record.
    pub observation_id: ObservationId,
    /// Field `observed_at` of this domain record.
    pub observed_at: Timestamp,
    /// Field `kind` of this domain record.
    pub kind: ObservationKind,
    /// Field `source_plane` of this domain record.
    pub source_plane: SourcePlane,
    /// Field `trace` of this domain record.
    pub trace: ObservationTraceRef,
    /// Field `call_id` of this domain record.
    pub call_id: Option<CallId>,
    /// Field `decision_id` of this domain record.
    pub decision_id: Option<DecisionId>,
    /// Field `subject_id` of this domain record.
    pub subject_id: Option<String>,
    /// Field `resource_id` of this domain record.
    pub resource_id: Option<String>,
    /// Field `policy_revision` of this domain record.
    pub policy_revision: Option<u64>,
    /// Field `grants_revision` of this domain record.
    pub grants_revision: Option<u64>,
    /// Field `outcome` of this domain record.
    pub outcome: ObservationOutcome,
    /// Field `visibility` of this domain record.
    pub visibility: ObservationVisibility,
    /// Field `safe_message` of this domain record.
    pub safe_message: String,
    /// Field `detail_ref` of this domain record.
    pub detail_ref: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `AdapterDiagnosticKind` used by the MCT kernel.
pub enum AdapterDiagnosticKind {
    /// Public `IrohStreamReset` item.
    IrohStreamReset,
    /// Public `WasmTrap` item.
    WasmTrap,
    /// Public `WasmMissingExport` item.
    WasmMissingExport,
    /// Public `WasmMissingHostImport` item.
    WasmMissingHostImport,
    /// Public `WasmValueConversionFailure` item.
    WasmValueConversionFailure,
    /// Public `ProcessExitFailure` item.
    ProcessExitFailure,
    /// Public `JvmTimeout` item.
    JvmTimeout,
    /// Public `StorageAppendFailure` item.
    StorageAppendFailure,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `AdapterDiagnosticObservationInput` used by the MCT kernel.
pub struct AdapterDiagnosticObservationInput {
    /// Field `observation_id` of this domain record.
    pub observation_id: ObservationId,
    /// Field `observed_at` of this domain record.
    pub observed_at: Timestamp,
    /// Field `diagnostic_kind` of this domain record.
    pub diagnostic_kind: AdapterDiagnosticKind,
    /// Field `trace` of this domain record.
    pub trace: ObservationTraceRef,
    /// Field `call_id` of this domain record.
    pub call_id: Option<CallId>,
    /// Field `decision_id` of this domain record.
    pub decision_id: Option<DecisionId>,
    /// Field `subject_id` of this domain record.
    pub subject_id: Option<String>,
    /// Field `resource_id` of this domain record.
    pub resource_id: Option<String>,
    /// Field `policy_revision` of this domain record.
    pub policy_revision: Option<u64>,
    /// Field `grants_revision` of this domain record.
    pub grants_revision: Option<u64>,
    /// Field `detail_ref` of this domain record.
    pub detail_ref: Option<String>,
}

impl MctObservation {
    /// Executes `informational` for this domain type.
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

/// Executes `adapter_diagnostic_observation` for this domain type.
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

/// Executes `hello_evaluation_observation` for this domain type.
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

/// Executes `peer_binding_state_observation` for this domain type.
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

/// Executes `route_decision_observation` for this domain type.
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

/// Executes `call_protocol_evaluation_observation` for this domain type.
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

/// Executes `child_approval_observation` for this domain type.
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

/// Executes `child_assignment_observation` for this domain type.
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

/// Executes `child_instance_observation` for this domain type.
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

/// Executes `child_call_authority_observation` for this domain type.
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

/// Executes `toy_grant_evaluation_observation` for this domain type.
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
            expires_at: None,
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
                approximate_size_bytes: 5,
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

    #[test]
    fn observation_kind_uses_snake_case_wire_names() {
        let encoded = serde_json::to_string(&ObservationKind::PeerHelloReceived).unwrap();
        assert_eq!(encoded, "\"peer_hello_received\"");
    }
}

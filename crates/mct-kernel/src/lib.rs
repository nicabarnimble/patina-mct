//! MCT authority kernel domain records and decisions.
//!
//! This crate owns Mother/Child/Toy domain types. It must not expose Iroh,
//! Wasmtime, storage, telemetry, or daemon implementation types in its public API.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Call records, JSON edge validation, and call protocol admission decisions.
pub mod call;
/// Child artifact, approval, assignment, lifecycle, and invocation authority.
pub mod child;
/// Typed validation and JSON edge errors for malformed kernel inputs.
pub mod error;
/// Opaque string identifiers and RFC3339 timestamps used by domain records.
pub mod id;
/// Observation records and projections from decisions into ledger facts.
pub mod observation;
/// Peer binding, hello negotiation, and ALPN admission decisions.
pub mod peer;
/// Route candidate filtering and execution-time revalidation authority.
pub mod route;
/// Canonical toy contracts, grants, and toy-call authorization tokens.
pub mod toy;

pub use call::{
    AuthorityContextSnapshot, CallEvaluationIds, CallOrigin, CallProtocolOutcome,
    CallProtocolReason, CallProtocolReplyOutcome, CallerIdentity, ExecutionSummary, MctCall,
    MctCallPayloadHandle, MctCallProtocolAuthority, MctCallProtocolEvaluation,
    MctCallProtocolReply, MctCallProtocolRequest, MctResult, OperationTarget, PayloadMetadata,
    ResultOutcome, RouteTaken, RuntimeKind, TraceContext, call_reply_from_evaluation,
    decode_call_protocol_reply_json, decode_call_protocol_request_json,
    encode_call_protocol_reply_json, encode_call_protocol_request_json, evaluate_call_protocol,
};
pub use child::{
    AuthorizedChildInvocation, ChildApproval, ChildApprovalState, ChildAssignment,
    ChildAssignmentState, ChildCallAuthorityEvaluation, ChildCallAuthorityIds,
    ChildCallAuthorityRequest, ChildCallAuthorityResult, ChildCallReasonCode, ChildCallVerdict,
    ChildIngressMode, ChildInstance, ChildInstanceState, ChildLifecycleTransition,
    ChildLifecycleTransitionReason, ComponentArtifact, ComponentRuntimeShape, ComponentWitExport,
    LifecycleExports, VerificationStatus, evaluate_child_call_authority,
    is_allowed_instance_transition, transition_child_instance,
};
pub use error::{InvalidFieldReason, MctKernelError, MctKernelResult};
pub use id::{
    AuditRef, AuthorizedChildInvocationId, AuthorizedRouteExecutionId, AuthorizedToyCallId, CallId,
    ChildApprovalId, ChildAssignmentId, ChildCallEvaluationId, ChildId, ChildInstanceId,
    ComponentArtifactId, DecisionId, EndpointIdText, MctNodeId, ObservationId, PeerBindingId,
    ProjectId, ProtocolRequestId, ReplyId, ResultRef, SpanId, Timestamp, ToyGrantEvaluationId,
    ToyGrantId, ToyId, TraceId, UserId, VisionId,
};
pub use observation::{
    AdapterDiagnosticKind, AdapterDiagnosticObservationInput, MctObservation, ObservationKind,
    ObservationOutcome, ObservationTraceRef, ObservationVisibility, SourcePlane,
    adapter_diagnostic_observation, call_protocol_evaluation_observation,
    child_approval_observation, child_assignment_observation, child_call_authority_observation,
    child_instance_observation, hello_evaluation_observation, peer_binding_state_observation,
    route_decision_observation, toy_grant_evaluation_observation,
};
pub use peer::{
    BindingState, ConnectionSide, EvaluationIds, HelloEvaluationContext, HelloOutcome, HelloPolicy,
    HelloReason, IrohConnectionPresentation, MCT_CALL_ALPN, MCT_HELLO_ALPN,
    MctHelloAdmissionEvaluation, MctHelloCapabilityView, MctHelloRequest, MctHelloResponse,
    MctPeerAdmissionDecision, MctPeerBinding, MctPeerBindingPresentation, MctPeerBindingScope,
    MctProtocolVersion, PathClass, PeerAdmissionOutcome, PeerAdmissionReason, SafeHelloReason,
    evaluate_hello, hello_response,
};
pub use route::{
    AuthorizedRouteExecution, CandidateAuthorityEvaluation, CandidateAuthorityOutcome,
    CandidateEliminationReason, CandidateRoute, NetworkPathClass, RouteDecision, RouteDecisionIds,
    RouteDecisionKind, RouteDecisionOutcome, RouteRevalidationIds, RouteRevalidationReason,
    RouteRevalidationResult, no_route_denied_result, revalidate_route_for_execution,
};
pub use toy::{
    AuthorizedToyCall, CanonicalToyContract, ToyContractIdentity, ToyGrant, ToyGrantConstraints,
    ToyGrantEvaluation, ToyGrantEvaluationIds, ToyGrantEvaluationRequest, ToyGrantEvaluationResult,
    ToyGrantReasonCode, ToyGrantScope, ToyGrantState, ToyGrantSubject, ToyGrantVerdict,
    evaluate_toy_grant_for_call,
};

/// Returns the crate version for health and smoke tests.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn exposes_version() {
        assert_eq!(super::version(), "0.1.0");
    }
}

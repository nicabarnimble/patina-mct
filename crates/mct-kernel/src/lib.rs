//! MCT authority kernel domain records and decisions.
//!
//! This crate owns Mother/Child/Toy domain types. It must not expose Iroh,
//! Wasmtime, storage, telemetry, or daemon implementation types in its public API.

#![forbid(unsafe_code)]

/// Admin-key grant facts and the route-scope check.
pub mod admin_key;
pub mod artifact;
/// Mother-owned, proof-gated local execution authority snapshots.
pub mod authority;
pub mod call;
pub mod child;
pub mod error;
/// Shared Child and admin-key grant check.
pub mod grant_check;
pub mod id;
pub mod observation;
pub mod peer;
pub mod route;
pub mod toy;
pub mod trigger;
pub mod watch;

pub use admin_key::{
    AdminKeyCheckAction, AdminKeyGrantCheckRequest, AdminKeyGrantCheckResult, AdminKeyGrantClassV1,
    AdminKeyGrantReason, AdminKeyGrantStateV1, AdminKeyGrantV1, AdminKeyGrantVerdict,
    AdminKeyPublicHalf, AdminKeySchemeV1, AdminKeyScopeV1, Ed25519VerifyingKey,
    admin_key_grant_shape_error, admin_key_public_half_error, evaluate_admin_key_grant,
    recovery_grant_scopes,
};
pub use artifact::{
    ArtifactAcquisition, ArtifactAcquisitionAuthorityPath, ArtifactAcquisitionAuthorityReason,
    ArtifactAcquisitionAuthorityRequest, ArtifactAcquisitionAuthorityResult,
    ArtifactAcquisitionOutcome, ArtifactSourceAuthority, ArtifactSourceAuthorityState,
    ArtifactSourceScope, ArtifactSourceScopeMode, ArtifactVerificationOutcome,
    AuthorizedFilesystemArtifactAcquisition, FilesystemAcquisitionEffectAuthority,
    OperatorPointedAcquisitionState, OperatorPointedArtifactAcquisitionDecision,
    evaluate_artifact_acquisition_authority,
};
pub use authority::{
    LocalAuthorityProjectionProvenanceV1, LocalCanonicalGrantsSnapshotV1,
    LocalChildPolicyProvenanceV1, LocalChildPolicySnapshotV1, LocalExecutionAuthoritySnapshot,
    LocalExecutionAuthoritySnapshotPartsV1, LocalExecutionAuthorityTokenV1,
    LocalGrantsAuthorityIdentityV1, LocalMotherClockProvenanceV1, LocalMotherClockSnapshotV1,
    LocalPeerAuthorityRecordPartsV1, LocalPeerAuthorityRecordV1, LocalPeerPolicyProvenanceV1,
    LocalPeerPolicySnapshotV1, LocalRemoteCallableSurfacePartsV1, LocalRemoteCallableSurfaceV1,
    assemble_local_execution_authority_snapshot,
};
pub use call::{
    AuthorityContextSnapshot, CallEvaluationContext, CallEvaluationIds, CallOrigin,
    CallProtocolOutcome, CallProtocolReason, CallProtocolReplyOutcome, CallProtocolRetryDirective,
    CallerIdentity, ExecutionSummary, GrantsAuthorityIdentity, MctCall, MctCallPayloadHandle,
    MctCallProtocolAuthority, MctCallProtocolEvaluation, MctCallProtocolReply,
    MctCallProtocolRequest, MctIdempotencyDecision, MctIdempotencyEntryState,
    MctIdempotencyFingerprint, MctIdempotencyReason, MctIdempotencyStoredEntry,
    MctPayloadIntegrityDecision, MctPayloadIntegrityObservation, MctResult, OperationTarget,
    PayloadIntegrityOutcome, PayloadIntegrityReason, PayloadIntegritySubject, PayloadMetadata,
    ResultOutcome, RouteTaken, RuntimeKind, TraceContext, call_reply_from_evaluation,
    call_reply_from_evaluation_with_result_payload,
    call_reply_from_evaluation_with_result_payload_and_route, decode_call_protocol_reply_json,
    decode_call_protocol_request_json, encode_call_protocol_reply_json,
    encode_call_protocol_request_json, evaluate_call_protocol, evaluate_idempotency_request,
    evaluate_payload_integrity,
};
pub use child::{
    ArtifactProvenanceStatus, AuthorizedChildInvocation, ChildApproval, ChildApprovalState,
    ChildAssignment, ChildAssignmentState, ChildCallAuthorityEvaluation, ChildCallAuthorityIds,
    ChildCallAuthorityRequest, ChildCallAuthorityResult, ChildCallReasonCode, ChildCallVerdict,
    ChildEffectAdmissionDenyV1, ChildIngressMode, ChildInstance, ChildInstanceState,
    ChildLifecycleTransition, ChildLifecycleTransitionReason, ComponentArtifact,
    ComponentRuntimeShape, ComponentWitExport, LifecycleExports, VerificationStatus,
    evaluate_child_call_authority, evaluate_child_call_authority_with_policy,
    is_allowed_instance_transition, transition_child_instance,
};
pub use error::{InvalidFieldReason, MctKernelError, MctKernelResult};
pub use grant_check::{GrantCheckOutcome, GrantCheckSubject, evaluate_grant_subject};
pub use id::{
    ArtifactAcquisitionDecisionId, ArtifactAcquisitionId, ArtifactSourceAuthorityId, AuditRef,
    AuthorizedArtifactAcquisitionId, AuthorizedChildInvocationId, AuthorizedRouteExecutionId,
    AuthorizedToyCallId, CallId, CallTriggerAuthorityId, CallTriggerFiringId,
    CallTriggerOccurrenceId, CallTriggerPendingOccurrenceId, ChildApprovalId, ChildAssignmentId,
    ChildCallEvaluationId, ChildId, ChildInstanceId, ComponentArtifactId, DecisionId,
    EndpointIdText, MctNodeId, ObservationId, PeerBindingId, ProjectId, ProtocolRequestId, ReplyId,
    ResultRef, SpanId, Timestamp, ToyGrantEvaluationId, ToyGrantId, ToyId, TraceId, UserId,
    VisionId, WatchEventBatchId, WatchEventDeliveryDispositionId, WatchEventDeliveryId,
    WatchEventId, WatchObservationScopeId,
};
pub use observation::{
    AdapterDiagnosticKind, AdapterDiagnosticObservationInput, MctObservation, ObservationKind,
    ObservationOutcome, ObservationTraceRef, ObservationVisibility, SourcePlane,
    adapter_diagnostic_observation, call_protocol_evaluation_observation,
    candidate_considered_observation, candidate_eliminated_observation, child_approval_observation,
    child_assignment_observation, child_call_authority_observation, child_instance_observation,
    hello_evaluation_observation, peer_binding_state_observation, route_decision_observation,
    toy_grant_evaluation_observation,
};
pub use peer::{
    BindingState, ConnectionSide, EvaluationIds, HelloEvaluationContext, HelloOutcome, HelloPolicy,
    HelloReason, IrohConnectionPresentation, MCT_CALL_ALPN, MCT_HELLO_ALPN,
    MctHelloAdmissionEvaluation, MctHelloCallableSurface, MctHelloCapabilityView, MctHelloRequest,
    MctHelloResponse, MctPeerAdmissionDecision, MctPeerAuthoritySnapshot, MctPeerBinding,
    MctPeerBindingPresentation, MctPeerBindingScope, MctProtocolVersion, PathClass,
    PeerAdmissionOutcome, PeerAdmissionReason, SafeHelloReason, evaluate_hello, hello_response,
};
pub use route::{
    AuthorizedRouteExecution, CandidateAuthorityEvaluation, CandidateAuthorityOutcome,
    CandidateEliminationClass, CandidateEliminationReason, CandidateRoute, NetworkPathClass,
    RouteDecision, RouteDecisionIds, RouteDecisionKind, RouteDecisionOutcome, RouteRevalidationIds,
    RouteRevalidationReason, RouteRevalidationResult, no_route_denied_result,
    revalidate_route_for_execution, revalidate_route_for_execution_with_snapshot,
};
pub use toy::{
    AuthorizedToyCall, CanonicalToyContract, ToyContractIdentity, ToyEffectAdmissionDenyV1,
    ToyGrant, ToyGrantConstraints, ToyGrantEvaluation, ToyGrantEvaluationIds,
    ToyGrantEvaluationRequest, ToyGrantEvaluationResult, ToyGrantReasonCode, ToyGrantScope,
    ToyGrantState, ToyGrantSubject, ToyGrantVerdict, evaluate_toy_grant_for_call,
    evaluate_toy_grant_for_route_snapshot, evaluate_toy_grant_for_snapshot,
};
pub use trigger::{
    CallTriggerAuthority, CallTriggerAuthorityState, CallTriggerClass,
    CallTriggerMissedFireDecision, CallTriggerOccurrenceCandidate, CallTriggerOverlapDecision,
    CallTriggerPendingReason, CallTriggerRepresentedSet, CallTriggerSource,
    CallTriggerTerminalDisposition, CallTriggerTerminalDispositionKind, KnownCallTriggerOccurrence,
    MCT_TRIGGER_MIN_INTERVAL_MS, MissedFirePolicy, OverlapPolicy,
    derive_coalesced_occurrence_identity, derive_represented_set_ref,
    derive_temporal_occurrence_identity, derive_trigger_call_identity,
    derive_trigger_firing_identity, derive_trigger_idempotency_key,
    derive_trigger_pending_identity, evaluate_missed_fire_policy, evaluate_overlap_policy,
    trigger_represented_set_from_bounds,
};
pub use watch::{
    AuthorizedWatchObservationSession, LegacyWatchCompatibilityValidation,
    MCT_CHILD_CALLOUT_MAX_DEPTH, MCT_KEYVALUE_KEY_MAX_BYTES, MCT_KEYVALUE_LIST_PAGE_MAX,
    MCT_KEYVALUE_MAX_KEYS_PER_BUCKET, MCT_KEYVALUE_VALUE_MAX_BYTES, MCT_WATCH_MAX_EVENTS_PER_BATCH,
    MCT_WATCH_MESSAGE_MAX_BYTES, MCT_WATCH_METADATA_PAIRS_MAX, MCT_WATCH_TOY_ACTION,
    MCT_WATCH_TOY_ID, WatchCoalescingPolicy, WatchEventBatchEvidence, WatchEventClass,
    WatchEventDeliveryDisposition, WatchEventDeliveryEvidence, WatchEventDisposition,
    WatchEventEvidence, WatchObservationScope, WatchObservationScopeState,
    WatchObservationSessionRequest, WatchObserverRef, WatchObserverShape, WatchScopeMode,
    WatchTraversalScope, authorize_watch_observation_session, derive_watch_batch_id,
    derive_watch_callout_call_id, derive_watch_callout_event_id,
    derive_watch_callout_idempotency_key, validate_legacy_watch_paths,
    validate_safe_watch_relative_path,
};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn exposes_version() {
        assert_eq!(super::version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn watch_scope_contract_is_closed_before_adapter_work() {
        assert_eq!(
            serde_json::to_string(&super::WatchTraversalScope::Recursive).unwrap(),
            "\"recursive\""
        );
        assert_eq!(super::MCT_WATCH_MAX_EVENTS_PER_BATCH, 128);
    }
}

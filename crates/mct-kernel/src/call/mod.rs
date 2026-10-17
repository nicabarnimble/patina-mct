use crate::{
    error::{MctKernelError, MctKernelResult, ensure_non_blank},
    id::*,
};
use serde::{Deserialize, Serialize};

mod internal;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Identity asserted for the caller of an MCT call.
///
/// The caller node and vision are authority-bearing; optional user and
/// project fields narrow audit and policy scope without changing node identity.
pub struct CallerIdentity {
    pub node_id: MctNodeId,
    pub user_id: Option<UserId>,
    pub vision_id: VisionId,
    pub project_id: Option<ProjectId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationTarget {
    pub namespace: String,
    pub interface_name: String,
    pub function_name: String,
}

impl OperationTarget {
    pub fn new(
        namespace: impl Into<String>,
        interface_name: impl Into<String>,
        function_name: impl Into<String>,
    ) -> MctKernelResult<Self> {
        let target = Self {
            namespace: namespace.into(),
            interface_name: interface_name.into(),
            function_name: function_name.into(),
        };
        target.validate()?;
        Ok(target)
    }

    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank("OperationTarget", "namespace", &self.namespace)?;
        ensure_non_blank("OperationTarget", "interface_name", &self.interface_name)?;
        ensure_non_blank("OperationTarget", "function_name", &self.function_name)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Routing-visible metadata about call payload bytes.
///
/// The kernel uses this summary for authority and routing; it does not inspect
/// business payload contents.
pub struct PayloadMetadata {
    /// Policy label used for data-placement and toy-grant matching.
    pub data_classification: String,
    pub size_bytes: u64,
    /// Whether the adapter says the payload contains secret-scoped material.
    pub contains_secret_scoped_material: bool,
}

impl PayloadMetadata {
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank(
            "PayloadMetadata",
            "data_classification",
            &self.data_classification,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Complete namespaced grants-authority identity advertised by a receiving Mother.
///
/// This wire value is caller-carried change-detection evidence only. It never
/// establishes local execution or effect authority.
pub struct GrantsAuthorityIdentity {
    pub mother_node_id: String,
    pub authority_epoch: String,
    pub generation: u64,
    pub source_authority_observation_id: String,
}

impl GrantsAuthorityIdentity {
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank(
            "GrantsAuthorityIdentity",
            "mother_node_id",
            &self.mother_node_id,
        )?;
        ensure_non_blank(
            "GrantsAuthorityIdentity",
            "authority_epoch",
            &self.authority_epoch,
        )?;
        ensure_non_blank(
            "GrantsAuthorityIdentity",
            "source_authority_observation_id",
            &self.source_authority_observation_id,
        )?;
        Ok(())
    }
}

impl From<&crate::authority::LocalGrantsAuthorityIdentityV1> for GrantsAuthorityIdentity {
    fn from(value: &crate::authority::LocalGrantsAuthorityIdentityV1) -> Self {
        Self {
            mother_node_id: value.mother_node_id().into(),
            authority_epoch: value.authority_epoch().into(),
            generation: value.generation(),
            source_authority_observation_id: value.source_authority_observation_id().into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Caller-carried authority expectations observed when the call was formed.
pub struct AuthorityContextSnapshot {
    pub policy_revision: u64,
    pub expected_receiver_grants_authority: GrantsAuthorityIdentity,
    pub vision_policy_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallOrigin {
    Iroh,
    JvmAdapter,
    WasmHost,
    ProcessHarness,
    Cli,
    TriggerFiring,
}

impl CallOrigin {
    /// Returns whether this Mother may source remote route candidates.
    ///
    /// Calls arriving over `mct/call/0` are terminal at the receiving Mother.
    /// Local origins may select one remote executor, but an Iroh arrival may
    /// only use local candidates and therefore cannot be forwarded again.
    pub const fn allows_remote_candidate_sourcing(self) -> bool {
        match self {
            Self::Iroh => false,
            Self::JvmAdapter
            | Self::WasmHost
            | Self::ProcessHarness
            | Self::Cli
            | Self::TriggerFiring => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Immutable semantic unit of requested work.
///
/// An adapter constructs exactly one call from protocol facts. Authority checks
/// compare caller, target, metadata, revisions, deadline, and origin without
/// reading payload bytes.
pub struct MctCall {
    pub call_id: CallId,
    pub caller: CallerIdentity,
    pub target: OperationTarget,
    pub payload_metadata: PayloadMetadata,
    pub authority_context: AuthorityContextSnapshot,
    pub deadline: Timestamp,
    pub trace_context: TraceContext,
    pub origin: CallOrigin,
}

impl CallerIdentity {
    pub fn validate(&self) -> MctKernelResult<()> {
        Ok(())
    }
}

impl TraceContext {
    pub fn validate(&self) -> MctKernelResult<()> {
        Ok(())
    }
}

impl MctCall {
    pub fn validate(&self) -> MctKernelResult<()> {
        self.caller.validate()?;
        self.target.validate()?;
        self.payload_metadata.validate()?;
        self.authority_context
            .expected_receiver_grants_authority
            .validate()?;
        ensure_non_blank("MctCall", "deadline", self.deadline.as_str())?;
        self.trace_context.validate()?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Process,
    JvmChild,
    WasmComponent,
    RemotePeer,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteTaken {
    pub node_id: MctNodeId,
    pub child_id: Option<ChildId>,
    pub runtime_kind: RuntimeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub wall_time_ms: u64,
    pub execution_time_ms: Option<u64>,
    pub queue_wait_ms: Option<u64>,
    pub input_size_bytes: u64,
    pub output_size_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultOutcome {
    Success,
    Denied,
    /// Execution failed without granting the caller internal details.
    Failed,
    TimedOut,
    Cancelled,
}

impl ResultOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Denied => "denied",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Caller-safe terminal answer for an MCT call.
///
/// The result references the authority decision and audit evidence instead of
/// embedding privileged details in `requester_message`.
pub struct MctResult {
    pub call_id: CallId,
    pub outcome: ResultOutcome,
    pub route_taken: Option<RouteTaken>,
    pub authority_decision_ref: DecisionId,
    pub execution_summary: ExecutionSummary,
    pub result_payload: MctCallPayloadHandle,
    /// Caller-safe message; privileged denial reasons live in observations.
    pub requester_message: String,
    pub audit_ref: AuditRef,
}

impl MctResult {
    pub fn validate(&self) -> MctKernelResult<()> {
        if self.route_taken.is_some()
            && matches!(
                self.outcome,
                ResultOutcome::Denied | ResultOutcome::Cancelled
            )
        {
            return Err(MctKernelError::InvalidRouteTakenPresence {
                record: "MctResult",
                outcome: self.outcome.as_str(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Authority facts carried from a successful hello into `mct/call/0`.
///
/// Call evaluation requires these facts to match the admitted hello, the
/// connection presentation, and the call authority snapshot.
pub struct MctCallProtocolAuthority {
    pub hello_decision_id: DecisionId,
    pub peer_binding_id: PeerBindingId,
    pub vision_id: VisionId,
    /// ALPN admitted for the call phase; must be `mct/call/0`.
    pub accepted_alpn: String,
    pub endpoint_id: EndpointIdText,
    /// Minimum policy revision the call snapshot must cover.
    pub policy_revision: u64,
    pub expected_receiver_grants_authority: GrantsAuthorityIdentity,
}

impl MctCallProtocolAuthority {
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank(
            "MctCallProtocolAuthority",
            "accepted_alpn",
            &self.accepted_alpn,
        )?;
        self.expected_receiver_grants_authority.validate()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "payload_kind", rename_all = "snake_case")]
/// Adapter-neutral reference to call payload bytes.
///
/// Each non-empty variant carries a size that must equal
/// [`MctCall::payload_metadata`].`size_bytes`; the kernel validates the handle
/// shape but never dereferences payload storage.
pub enum MctCallPayloadHandle {
    InlinePayload {
        inline_payload_ref: String,
        content_type: String,
        size_bytes: u64,
        blake3_digest_hex: String,
    },
    ContentAddressedBlob {
        digest: String,
        blob_ref: String,
        content_type: String,
        size_bytes: u64,
    },
    ExternalReference {
        /// Non-blank reference whose dereference is an adapter responsibility.
        external_ref: String,
        content_type: Option<String>,
        /// Approximate byte size for payloads MCT does not dereference.
        approximate_size_bytes: u64,
    },
    Empty,
}

impl MctCallPayloadHandle {
    pub fn declared_size_bytes(&self) -> u64 {
        match self {
            Self::InlinePayload { size_bytes, .. }
            | Self::ContentAddressedBlob { size_bytes, .. } => *size_bytes,
            Self::ExternalReference {
                approximate_size_bytes,
                ..
            } => *approximate_size_bytes,
            Self::Empty => 0,
        }
    }

    pub fn validate(&self) -> MctKernelResult<()> {
        match self {
            Self::InlinePayload {
                inline_payload_ref,
                content_type,
                blake3_digest_hex,
                ..
            } => {
                ensure_non_blank(
                    "MctCallPayloadHandle",
                    "inline_payload_ref",
                    inline_payload_ref,
                )?;
                ensure_non_blank("MctCallPayloadHandle", "content_type", content_type)?;
                ensure_non_blank(
                    "MctCallPayloadHandle",
                    "blake3_digest_hex",
                    blake3_digest_hex,
                )?;
            }
            Self::ContentAddressedBlob {
                digest,
                blob_ref,
                content_type,
                ..
            } => {
                ensure_non_blank("MctCallPayloadHandle", "digest", digest)?;
                ensure_non_blank("MctCallPayloadHandle", "blob_ref", blob_ref)?;
                ensure_non_blank("MctCallPayloadHandle", "content_type", content_type)?;
            }
            Self::ExternalReference {
                external_ref,
                content_type,
                ..
            } => {
                ensure_non_blank("MctCallPayloadHandle", "external_ref", external_ref)?;
                validate_optional_string_field(
                    "MctCallPayloadHandle",
                    "content_type",
                    content_type,
                )?;
            }
            Self::Empty => {}
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadIntegritySubject {
    Request,
    ReplyResult,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadIntegrityOutcome {
    Matched,
    Mismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadIntegrityReason {
    IntegrityMatched,
    PayloadDeclaredTooLarge,
    PayloadActualTooLarge,
    PayloadSizeMismatch,
    PayloadDigestMismatch,
    PayloadMissingInlineBytes,
    PayloadUnexpectedInlineBytes,
    PayloadBlobUnavailable,
    InvalidPayloadDigest,
    ResultPayloadTooLarge,
    ResultPayloadIntegrityMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPayloadIntegrityObservation {
    pub inline_bytes_present: bool,
    #[serde(default)]
    pub content_addressed_blob_fetch_attempted: bool,
    pub observed_size_bytes: Option<u64>,
    pub observed_blake3_digest_hex: Option<String>,
}

impl MctPayloadIntegrityObservation {
    pub fn missing_inline_bytes() -> Self {
        Self {
            inline_bytes_present: false,
            content_addressed_blob_fetch_attempted: false,
            observed_size_bytes: None,
            observed_blake3_digest_hex: None,
        }
    }

    pub fn missing_content_addressed_blob() -> Self {
        Self {
            inline_bytes_present: false,
            content_addressed_blob_fetch_attempted: true,
            observed_size_bytes: None,
            observed_blake3_digest_hex: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPayloadIntegrityDecision {
    pub subject: PayloadIntegritySubject,
    pub outcome: PayloadIntegrityOutcome,
    pub reason: PayloadIntegrityReason,
    pub safe_message: String,
}

/// Compares declared payload-handle facts against adapter-observed size/digest facts.
///
/// The kernel does not decode bytes, hash bytes, or dereference handles. Adapters
/// perform those effects first and pass only the observed facts here.
pub fn evaluate_payload_integrity(
    subject: PayloadIntegritySubject,
    handle: &MctCallPayloadHandle,
    observed: &MctPayloadIntegrityObservation,
    max_inline_size_bytes: u64,
) -> MctPayloadIntegrityDecision {
    internal::evaluate_payload_integrity_internal(subject, handle, observed, max_inline_size_bytes)
}

impl PayloadIntegrityReason {
    pub fn to_call_protocol_reason(self) -> CallProtocolReason {
        match self {
            Self::IntegrityMatched => CallProtocolReason::ResultRecorded,
            Self::PayloadDeclaredTooLarge => CallProtocolReason::PayloadDeclaredTooLarge,
            Self::PayloadActualTooLarge => CallProtocolReason::PayloadActualTooLarge,
            Self::PayloadSizeMismatch => CallProtocolReason::PayloadSizeMismatch,
            Self::PayloadDigestMismatch => CallProtocolReason::PayloadDigestMismatch,
            Self::PayloadMissingInlineBytes => CallProtocolReason::PayloadMissingInlineBytes,
            Self::PayloadUnexpectedInlineBytes => CallProtocolReason::PayloadUnexpectedInlineBytes,
            Self::PayloadBlobUnavailable => CallProtocolReason::PayloadBlobUnavailable,
            Self::InvalidPayloadDigest => CallProtocolReason::InvalidPayloadDigest,
            Self::ResultPayloadTooLarge => CallProtocolReason::ResultPayloadTooLarge,
            Self::ResultPayloadIntegrityMismatch => {
                CallProtocolReason::ResultPayloadIntegrityMismatch
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Wire-edge `mct/call/0` request after JSON decoding.
///
/// Validation requires authority, connection, call, and payload facts to be
/// internally consistent before evaluation can authorize routing.
pub struct MctCallProtocolRequest {
    pub protocol_request_id: ProtocolRequestId,
    pub authority: MctCallProtocolAuthority,
    /// Connection facts supplied by the receiving adapter, not by the peer.
    pub received_over: crate::peer::IrohConnectionPresentation,
    pub call: MctCall,
    pub payload: MctCallPayloadHandle,
    pub idempotency_key: Option<String>,
    pub received_observation_id: ObservationId,
}

impl MctCallProtocolRequest {
    pub fn validate(&self) -> MctKernelResult<()> {
        self.authority.validate()?;
        self.received_over.validate()?;
        self.call.validate()?;
        self.payload.validate()?;
        validate_optional_string_field(
            "MctCallProtocolRequest",
            "idempotency_key",
            &self.idempotency_key,
        )?;
        if self.payload.declared_size_bytes() != self.call.payload_metadata.size_bytes {
            return Err(MctKernelError::PayloadSizeMismatch {
                call_size_bytes: self.call.payload_metadata.size_bytes,
                handle_size_bytes: self.payload.declared_size_bytes(),
            });
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctIdempotencyFingerprint {
    pub target: String,
    pub call_id: CallId,
    /// Declared payload digest or canonical empty/external marker.
    pub payload_digest: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctIdempotencyEntryState {
    InFlight,
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctIdempotencyStoredEntry {
    pub fingerprint: MctIdempotencyFingerprint,
    pub state: MctIdempotencyEntryState,
    /// End of the bounded replay window.
    pub expires_at: Timestamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctIdempotencyReason {
    ExecuteFresh,
    ReplayCompleted,
    IdempotencyKeyReuseMismatch,
    IdempotencyBudgetFull,
    IdempotencyInProgress,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctIdempotencyDecision {
    pub reason: MctIdempotencyReason,
    pub safe_message: String,
}

pub fn evaluate_idempotency_request(
    requested: &MctIdempotencyFingerprint,
    stored: Option<&MctIdempotencyStoredEntry>,
    caller_entry_count: usize,
    caller_entry_budget: usize,
    now: &Timestamp,
) -> MctIdempotencyDecision {
    let live = stored.filter(|entry| entry.expires_at > *now);
    let reason = match live {
        Some(entry) if entry.fingerprint != *requested => {
            MctIdempotencyReason::IdempotencyKeyReuseMismatch
        }
        Some(entry) if entry.state == MctIdempotencyEntryState::InFlight => {
            MctIdempotencyReason::IdempotencyInProgress
        }
        Some(_) => MctIdempotencyReason::ReplayCompleted,
        None if caller_entry_count >= caller_entry_budget => {
            MctIdempotencyReason::IdempotencyBudgetFull
        }
        None => MctIdempotencyReason::ExecuteFresh,
    };
    let safe_message = match reason {
        MctIdempotencyReason::ExecuteFresh => "idempotency key reserved",
        MctIdempotencyReason::ReplayCompleted => "recorded result replayed",
        MctIdempotencyReason::IdempotencyKeyReuseMismatch => {
            "idempotency key does not match request"
        }
        MctIdempotencyReason::IdempotencyBudgetFull => {
            "idempotency capacity unavailable; retry later"
        }
        MctIdempotencyReason::IdempotencyInProgress => "request already in progress; retry later",
    };
    MctIdempotencyDecision {
        reason,
        safe_message: safe_message.into(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallProtocolOutcome {
    AcceptedForRouting,
    Malformed,
    Denied,
    Failed,
    TimedOut,
    Cancelled,
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Specific non-secret reason recorded for a call protocol evaluation.
pub enum CallProtocolReason {
    HelloNotAdmitted,
    AlpnNotAdmitted,
    EndpointMismatch,
    BindingMismatch,
    CallerMismatch,
    VisionMismatch,
    BindingRevoked,
    BindingExpired,
    /// Revisions disagree, a cited revision is behind current peer authority, or the call snapshot is behind the request.
    PolicyRevisionStale,
    ExpectedReceiverAuthorityStale,
    /// Current receiver authority cannot be proved coherently.
    ReceiverAuthorityUnavailable,
    /// Caller deadline was at or behind the executing Mother's acceptance time.
    CallDeadlineExpired,
    MalformedCall,
    PayloadMetadataMismatch,
    PayloadDeclaredTooLarge,
    PayloadActualTooLarge,
    PayloadSizeMismatch,
    PayloadDigestMismatch,
    PayloadMissingInlineBytes,
    PayloadUnexpectedInlineBytes,
    PayloadBlobUnavailable,
    InvalidPayloadDigest,
    ChildPayloadContentTypeUnsupported,
    ResultPayloadTooLarge,
    ResultPayloadIntegrityMismatch,
    IdempotencyKeyReuseMismatch,
    IdempotencyBudgetFull,
    IdempotencyInProgress,
    IdempotencyReplayCompleted,
    AuthorityDenied,
    NoRoute,
    ExecutionFailed,
    ExecutionTimedOut,
    /// Execution was cut off by a Mother restart, so its outcome is unknown.
    /// Restart recovery closes such a call once and never re-executes it; a
    /// same-key retry replays this reason rather than running the call again.
    ExecutionInterrupted,
    AcceptedForRouting,
    ResultRecorded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallProtocolRetryDirective {
    None,
    RefreshHello,
    RetryLater,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Decision produced by `mct/call/0` authority evaluation.
///
/// Denied and malformed evaluations carry no route or result reference; safe
/// messages are caller-facing projections of the typed reason.
pub struct MctCallProtocolEvaluation {
    pub decision_id: DecisionId,
    pub protocol_request_id: ProtocolRequestId,
    pub call_id: Option<CallId>,
    pub route_decision_id: Option<DecisionId>,
    pub result_ref: Option<ResultRef>,
    pub outcome: CallProtocolOutcome,
    pub reason: CallProtocolReason,
    pub retry_directive: CallProtocolRetryDirective,
    /// Caller-safe message that must not disclose privileged policy detail.
    pub safe_message: String,
    pub observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallProtocolReplyOutcome {
    Success,
    Denied,
    /// Execution failed without granting the caller internal details.
    Failed,
    TimedOut,
    Cancelled,
    Malformed,
}

impl CallProtocolReplyOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Denied => "denied",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
            Self::Malformed => "malformed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Wire-edge response for `mct/call/0`.
///
/// The reply carries only caller-safe outcome and an optional opaque result
/// reference; detailed authority reasons remain in observations.
pub struct MctCallProtocolReply {
    pub reply_id: ReplyId,
    pub protocol_request_id: ProtocolRequestId,
    pub decision_id: DecisionId,
    /// Opaque result lookup reference, present only when one is safe to return.
    pub result_ref: Option<ResultRef>,
    pub result_payload: MctCallPayloadHandle,
    pub route_taken: Option<RouteTaken>,
    pub reply_outcome: CallProtocolReplyOutcome,
    pub retry_directive: CallProtocolRetryDirective,
    pub safe_message: String,
    pub reply_observation_id: ObservationId,
}

impl MctCallProtocolReply {
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank("MctCallProtocolReply", "safe_message", &self.safe_message)?;
        self.result_payload.validate()?;
        if self.route_taken.is_some()
            && matches!(
                self.reply_outcome,
                CallProtocolReplyOutcome::Denied
                    | CallProtocolReplyOutcome::Cancelled
                    | CallProtocolReplyOutcome::Malformed
            )
        {
            return Err(MctKernelError::InvalidRouteTakenPresence {
                record: "MctCallProtocolReply",
                outcome: self.reply_outcome.as_str(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Caller-supplied IDs used when minting a call protocol evaluation.
pub struct CallEvaluationIds {
    pub decision_id: DecisionId,
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Adapter-supplied current authority facts for one call evaluation.
pub struct CallEvaluationContext {
    pub ids: CallEvaluationIds,
    pub current_peer_authority: crate::peer::MctPeerAuthoritySnapshot,
    /// Fresh proof-gated receiver identity, absent only when local authority is unavailable.
    pub current_receiver_grants_authority: Option<GrantsAuthorityIdentity>,
    pub now: Timestamp,
}

/// Decides whether an admitted peer may submit this `mct/call/0` request.
///
/// The authority facts are the validated request, the prior hello admission,
/// and adapter-supplied current peer bindings, policy revision, time, and IDs.
/// Returns `AcceptedForRouting` only when hello was admitted and current binding,
/// caller node, Vision, ALPN, endpoint, revisions, and payload size all match.
/// Absence or mismatch of authority is a denied or malformed decision, not an error.
pub fn evaluate_call_protocol(
    request: &MctCallProtocolRequest,
    hello: &crate::peer::MctHelloAdmissionEvaluation,
    context: CallEvaluationContext,
) -> MctCallProtocolEvaluation {
    internal::evaluate_call_protocol_internal(request, hello, context)
}

impl MctCallProtocolEvaluation {
    pub fn is_accepted_for_routing(&self) -> bool {
        self.outcome == CallProtocolOutcome::AcceptedForRouting
    }
}

pub fn encode_call_protocol_request_json(
    request: &MctCallProtocolRequest,
) -> MctKernelResult<Vec<u8>> {
    request.validate()?;
    serde_json::to_vec(request).map_err(|source| MctKernelError::EncodeCallProtocolJson { source })
}

pub fn decode_call_protocol_request_json(bytes: &[u8]) -> MctKernelResult<MctCallProtocolRequest> {
    let request: MctCallProtocolRequest = serde_json::from_slice(bytes)
        .map_err(|source| MctKernelError::DecodeCallProtocolJson { source })?;
    request.validate()?;
    Ok(request)
}

pub fn encode_call_protocol_reply_json(reply: &MctCallProtocolReply) -> MctKernelResult<Vec<u8>> {
    reply.validate()?;
    serde_json::to_vec(reply).map_err(|source| MctKernelError::EncodeCallProtocolJson { source })
}

pub fn decode_call_protocol_reply_json(bytes: &[u8]) -> MctKernelResult<MctCallProtocolReply> {
    let reply: MctCallProtocolReply = serde_json::from_slice(bytes)
        .map_err(|source| MctKernelError::DecodeCallProtocolJson { source })?;
    reply.validate()?;
    Ok(reply)
}

fn validate_optional_string_field(
    record: &'static str,
    field: &'static str,
    value: &Option<String>,
) -> MctKernelResult<()> {
    if let Some(value) = value {
        ensure_non_blank(record, field, value)?;
    }
    Ok(())
}

/// Projects a protocol evaluation into the caller-safe reply shape.
///
/// Accepted and completed evaluations map to success; malformed, denied,
/// failed, timed-out, and cancelled evaluations retain their caller-safe outcome class.
pub fn call_reply_from_evaluation(
    reply_id: ReplyId,
    evaluation: &MctCallProtocolEvaluation,
    result_ref: Option<ResultRef>,
    reply_observation_id: ObservationId,
) -> MctCallProtocolReply {
    call_reply_from_evaluation_with_result_payload(
        reply_id,
        evaluation,
        result_ref,
        MctCallPayloadHandle::Empty,
        reply_observation_id,
    )
}

pub fn call_reply_from_evaluation_with_result_payload(
    reply_id: ReplyId,
    evaluation: &MctCallProtocolEvaluation,
    result_ref: Option<ResultRef>,
    result_payload: MctCallPayloadHandle,
    reply_observation_id: ObservationId,
) -> MctCallProtocolReply {
    call_reply_from_evaluation_with_result_payload_and_route(
        reply_id,
        evaluation,
        result_ref,
        result_payload,
        None,
        reply_observation_id,
    )
}

pub fn call_reply_from_evaluation_with_result_payload_and_route(
    reply_id: ReplyId,
    evaluation: &MctCallProtocolEvaluation,
    result_ref: Option<ResultRef>,
    result_payload: MctCallPayloadHandle,
    route_taken: Option<RouteTaken>,
    reply_observation_id: ObservationId,
) -> MctCallProtocolReply {
    let reply_outcome = match evaluation.outcome {
        CallProtocolOutcome::AcceptedForRouting | CallProtocolOutcome::Completed => {
            CallProtocolReplyOutcome::Success
        }
        CallProtocolOutcome::Malformed => CallProtocolReplyOutcome::Malformed,
        CallProtocolOutcome::Denied => CallProtocolReplyOutcome::Denied,
        CallProtocolOutcome::Failed => CallProtocolReplyOutcome::Failed,
        CallProtocolOutcome::TimedOut => CallProtocolReplyOutcome::TimedOut,
        CallProtocolOutcome::Cancelled => CallProtocolReplyOutcome::Cancelled,
    };
    let route_taken = match reply_outcome {
        CallProtocolReplyOutcome::Denied
        | CallProtocolReplyOutcome::Cancelled
        | CallProtocolReplyOutcome::Malformed => None,
        CallProtocolReplyOutcome::Success
        | CallProtocolReplyOutcome::Failed
        | CallProtocolReplyOutcome::TimedOut => route_taken,
    };

    MctCallProtocolReply {
        reply_id,
        protocol_request_id: evaluation.protocol_request_id.clone(),
        decision_id: evaluation.decision_id.clone(),
        result_ref,
        result_payload,
        route_taken,
        reply_outcome,
        retry_directive: evaluation.retry_directive,
        safe_message: evaluation.safe_message.clone(),
        reply_observation_id,
    }
}

#[cfg(test)]
pub(crate) fn test_grants_authority_identity(generation: u64) -> GrantsAuthorityIdentity {
    GrantsAuthorityIdentity {
        mother_node_id: "mother-test".into(),
        authority_epoch: "epoch-test".into(),
        generation,
        source_authority_observation_id: format!("obs-authority-test-{generation}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::{
        BindingState, ConnectionSide, HelloOutcome, HelloReason, IrohConnectionPresentation,
        MCT_CALL_ALPN, MCT_HELLO_ALPN, MctHelloAdmissionEvaluation, MctPeerAuthoritySnapshot,
        MctPeerBinding, MctPeerBindingScope, MctProtocolVersion, PathClass, SafeHelloReason,
    };

    fn example_call() -> MctCall {
        MctCall {
            call_id: CallId::new("call-1")
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
                policy_revision: 1,
                expected_receiver_grants_authority: test_grants_authority_identity(1),
                vision_policy_revision: 1,
            },
            deadline: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-1")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: SpanId::new("span-1")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            origin: CallOrigin::Iroh,
        }
    }

    fn admitted_hello() -> MctHelloAdmissionEvaluation {
        MctHelloAdmissionEvaluation {
            decision_id: DecisionId::new("hello-decision-1")
                .expect("string ID literal/generated value must be non-empty"),
            request_id: "hello-1".into(),
            peer_admission_decision_id: None,
            selected_binding_id: Some(
                PeerBindingId::new("binding-1")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            selected_node_id: Some(
                MctNodeId::new("node-a")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            selected_vision_id: Some(
                VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            selected_policy_revision: Some(1),
            negotiated_protocol: Some(MctProtocolVersion {
                protocol_name: MCT_HELLO_ALPN.into(),
                major: 0,
                minor: 1,
                compatibility_floor: Some(0),
            }),
            accepted_alpns: vec![MCT_CALL_ALPN.into()],
            hello_outcome: HelloOutcome::Admitted,
            reason: HelloReason::ActiveBinding,
            safe_reason: SafeHelloReason::Admitted,
            observation_id: ObservationId::new("obs-hello-decision")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn protocol_request() -> MctCallProtocolRequest {
        MctCallProtocolRequest {
            protocol_request_id: ProtocolRequestId::new("proto-request-1")
                .expect("string ID literal/generated value must be non-empty"),
            authority: MctCallProtocolAuthority {
                hello_decision_id: DecisionId::new("hello-decision-1")
                    .expect("string ID literal/generated value must be non-empty"),
                peer_binding_id: PeerBindingId::new("binding-1")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                accepted_alpn: MCT_CALL_ALPN.into(),
                endpoint_id: EndpointIdText::new("endpoint-a")
                    .expect("string ID literal/generated value must be non-empty"),
                policy_revision: 1,
                expected_receiver_grants_authority: test_grants_authority_identity(1),
            },
            received_over: IrohConnectionPresentation {
                endpoint_id: EndpointIdText::new("endpoint-a")
                    .expect("string ID literal/generated value must be non-empty"),
                alpn: MCT_CALL_ALPN.into(),
                connection_side: ConnectionSide::Incoming,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            call: example_call(),
            payload: MctCallPayloadHandle::InlinePayload {
                inline_payload_ref: "payload-1".into(),
                content_type: "text/plain".into(),
                size_bytes: 5,
                blake3_digest_hex:
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            },
            idempotency_key: Some("idem-1".into()),
            received_observation_id: ObservationId::new("obs-call-received")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn eval_ids() -> CallEvaluationContext {
        CallEvaluationContext {
            ids: CallEvaluationIds {
                decision_id: DecisionId::new("call-decision-1")
                    .expect("string ID literal/generated value must be non-empty"),
                observation_id: ObservationId::new("obs-call-decision")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            current_peer_authority: MctPeerAuthoritySnapshot {
                bindings: vec![MctPeerBinding {
                    binding_id: PeerBindingId::new("binding-1")
                        .expect("string ID literal/generated value must be non-empty"),
                    iroh_endpoint_id: EndpointIdText::new("endpoint-a")
                        .expect("string ID literal/generated value must be non-empty"),
                    scope: MctPeerBindingScope {
                        mct_node_id: MctNodeId::new("node-a")
                            .expect("string ID literal/generated value must be non-empty"),
                        vision_id: VisionId::new("vision-a")
                            .expect("string ID literal/generated value must be non-empty"),
                        allowed_alpns: vec![MCT_CALL_ALPN.into()],
                        data_scope: None,
                        observation_scope: None,
                    },
                    issuer_node_id: MctNodeId::new("node-local")
                        .expect("string ID literal/generated value must be non-empty"),
                    policy_revision: 1,
                    binding_state: BindingState::Admitted,
                    issued_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
                    expires_at: Timestamp::new("2026-05-31T00:05:00Z").unwrap(),
                    created_by_observation_id: ObservationId::new("obs-binding-current")
                        .expect("string ID literal/generated value must be non-empty"),
                    superseded_by_observation_id: None,
                }],
                policy_revision: 1,
            },
            current_receiver_grants_authority: Some(test_grants_authority_identity(1)),
            now: Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
        }
    }

    #[test]
    fn disagreeing_receiver_identity_copies_are_malformed_before_current_comparison() {
        let mut request = protocol_request();
        request
            .authority
            .expected_receiver_grants_authority
            .authority_epoch = "epoch-protocol-copy".into();
        let semantic_copy = request
            .call
            .authority_context
            .expected_receiver_grants_authority
            .clone();
        let protocol_copy = request.authority.expected_receiver_grants_authority.clone();

        for current in [semantic_copy, protocol_copy] {
            let mut context = eval_ids();
            context.current_receiver_grants_authority = Some(current);
            let evaluation = evaluate_call_protocol(&request, &admitted_hello(), context);
            assert_eq!(evaluation.outcome, CallProtocolOutcome::Malformed);
            assert_eq!(evaluation.reason, CallProtocolReason::MalformedCall);
            assert_eq!(evaluation.retry_directive, CallProtocolRetryDirective::None);
            assert!(evaluation.route_decision_id.is_none());
        }
    }

    #[test]
    fn complete_receiver_identity_mismatch_matrix_refreshes_hello() {
        let current = test_grants_authority_identity(1);
        let mut wrong_mother = current.clone();
        wrong_mother.mother_node_id = "mother-foreign".into();
        let mut wrong_epoch = current.clone();
        wrong_epoch.authority_epoch = "epoch-prior".into();
        let mut wrong_source = current.clone();
        wrong_source.source_authority_observation_id = "obs-authority-other".into();
        let mut future_generation = current.clone();
        future_generation.generation = u64::MAX;

        for expected in [wrong_mother, wrong_epoch, wrong_source, future_generation] {
            let mut request = protocol_request();
            request
                .call
                .authority_context
                .expected_receiver_grants_authority = expected.clone();
            request.authority.expected_receiver_grants_authority = expected;
            let evaluation = evaluate_call_protocol(&request, &admitted_hello(), eval_ids());
            assert_eq!(evaluation.outcome, CallProtocolOutcome::Denied);
            assert_eq!(
                evaluation.reason,
                CallProtocolReason::ExpectedReceiverAuthorityStale
            );
            assert_eq!(
                evaluation.retry_directive,
                CallProtocolRetryDirective::RefreshHello
            );
            assert!(evaluation.route_decision_id.is_none());
        }
    }

    #[test]
    fn execution_interrupted_reason_round_trips_on_the_wire() {
        let reason = CallProtocolReason::ExecutionInterrupted;
        let encoded = serde_json::to_string(&reason).unwrap();
        assert_eq!(encoded, "\"execution_interrupted\"");
        assert_eq!(
            serde_json::from_str::<CallProtocolReason>(&encoded).unwrap(),
            reason
        );
        assert_ne!(
            encoded,
            serde_json::to_string(&CallProtocolReason::ExecutionFailed).unwrap(),
            "interrupted is distinct from failed"
        );

        // The served evaluation carries the handler's reason, as `mct/call/0`
        // serving sets it, and keeps it across an encode/decode.
        let mut evaluation =
            evaluate_call_protocol(&protocol_request(), &admitted_hello(), eval_ids());
        evaluation.outcome = CallProtocolOutcome::Failed;
        evaluation.reason = reason;
        evaluation.retry_directive = CallProtocolRetryDirective::None;
        evaluation.safe_message = "call interrupted by Mother restart; outcome unknown".into();
        let wire = serde_json::to_value(&evaluation).unwrap();
        assert_eq!(wire["reason"], "execution_interrupted");
        assert_eq!(wire["outcome"], "failed");
        let decoded: MctCallProtocolEvaluation = serde_json::from_value(wire).unwrap();
        assert_eq!(decoded, evaluation);

        // The peer reply projects it as a failed, non-retryable reply.
        let reply = call_reply_from_evaluation_with_result_payload_and_route(
            ReplyId::new("reply-interrupted").unwrap(),
            &decoded,
            None,
            MctCallPayloadHandle::Empty,
            None,
            ObservationId::new("obs-reply-interrupted").unwrap(),
        );
        reply.validate().unwrap();
        let decoded_reply: MctCallProtocolReply =
            serde_json::from_str(&serde_json::to_string(&reply).unwrap()).unwrap();
        assert_eq!(decoded_reply, reply);
        assert_eq!(
            decoded_reply.reply_outcome,
            CallProtocolReplyOutcome::Failed
        );
        assert_eq!(
            decoded_reply.retry_directive,
            CallProtocolRetryDirective::None
        );
    }

    #[test]
    fn unavailable_receiver_identity_retries_and_matching_echo_grants_nothing() {
        let mut unavailable = eval_ids();
        unavailable.current_receiver_grants_authority = None;
        let evaluation =
            evaluate_call_protocol(&protocol_request(), &admitted_hello(), unavailable);
        assert_eq!(
            evaluation.reason,
            CallProtocolReason::ReceiverAuthorityUnavailable
        );
        assert_eq!(
            evaluation.retry_directive,
            CallProtocolRetryDirective::RetryLater
        );
        assert!(evaluation.route_decision_id.is_none());

        let mut revoked = eval_ids();
        revoked.current_peer_authority.bindings[0].binding_state = BindingState::Revoked;
        let evaluation = evaluate_call_protocol(&protocol_request(), &admitted_hello(), revoked);
        assert_eq!(evaluation.reason, CallProtocolReason::BindingRevoked);
        assert_eq!(evaluation.retry_directive, CallProtocolRetryDirective::None);
        assert!(evaluation.route_decision_id.is_none());
    }

    #[test]
    fn receiver_identity_wire_has_no_legacy_absent_integer_or_malformed_form() {
        let request = protocol_request();

        let mut absent = serde_json::to_value(&request).unwrap();
        absent["authority"]
            .as_object_mut()
            .unwrap()
            .remove("expected_receiver_grants_authority");
        absent["call"]["authority_context"]
            .as_object_mut()
            .unwrap()
            .remove("expected_receiver_grants_authority");
        assert_eq!(
            decode_call_protocol_request_json(&serde_json::to_vec(&absent).unwrap())
                .unwrap_err()
                .to_string(),
            "failed to decode MCT call protocol JSON edge value: missing field `expected_receiver_grants_authority` at line 1 column 182"
        );

        let mut integer = serde_json::to_value(&request).unwrap();
        integer["authority"]["expected_receiver_grants_authority"] = serde_json::json!(1);
        integer["call"]["authority_context"]["expected_receiver_grants_authority"] =
            serde_json::json!(1);
        assert_eq!(
            decode_call_protocol_request_json(&serde_json::to_vec(&integer).unwrap())
                .unwrap_err()
                .to_string(),
            "failed to decode MCT call protocol JSON edge value: invalid type: integer `1`, expected struct GrantsAuthorityIdentity at line 1 column 108"
        );

        let mut malformed = serde_json::to_value(&request).unwrap();
        malformed["authority"]["expected_receiver_grants_authority"]["authority_epoch"] =
            serde_json::json!(" ");
        malformed["call"]["authority_context"]["expected_receiver_grants_authority"]["authority_epoch"] =
            serde_json::json!(" ");
        assert!(matches!(
            decode_call_protocol_request_json(&serde_json::to_vec(&malformed).unwrap()),
            Err(MctKernelError::InvalidField {
                record: "GrantsAuthorityIdentity",
                field: "authority_epoch",
                ..
            })
        ));
    }

    #[test]
    fn only_local_call_origins_allow_remote_candidate_sourcing() {
        assert!(!CallOrigin::Iroh.allows_remote_candidate_sourcing());
        for origin in [
            CallOrigin::Cli,
            CallOrigin::JvmAdapter,
            CallOrigin::WasmHost,
            CallOrigin::ProcessHarness,
        ] {
            assert!(origin.allows_remote_candidate_sourcing(), "{origin:?}");
        }
    }

    #[test]
    fn trigger_firing_origin_is_additive_local_and_single_hop() {
        let historical = [
            (CallOrigin::Iroh, "\"iroh\""),
            (CallOrigin::JvmAdapter, "\"jvm_adapter\""),
            (CallOrigin::WasmHost, "\"wasm_host\""),
            (CallOrigin::ProcessHarness, "\"process_harness\""),
            (CallOrigin::Cli, "\"cli\""),
        ];
        for (origin, expected) in historical {
            assert_eq!(serde_json::to_string(&origin).unwrap(), expected);
            assert_eq!(
                serde_json::from_str::<CallOrigin>(expected).unwrap(),
                origin
            );
        }
        assert_eq!(
            serde_json::to_string(&CallOrigin::TriggerFiring).unwrap(),
            "\"trigger_firing\""
        );
        assert!(CallOrigin::TriggerFiring.allows_remote_candidate_sourcing());
        assert!(!CallOrigin::Iroh.allows_remote_candidate_sourcing());
        assert_eq!(
            serde_json::from_str::<CallOrigin>("\"trigger\"")
                .unwrap_err()
                .to_string(),
            "unknown variant `trigger`, expected one of `iroh`, `jvm_adapter`, `wasm_host`, `process_harness`, `cli`, `trigger_firing` at line 1 column 9"
        );
    }

    #[test]
    fn mct_call_roundtrips_as_json() {
        let call = example_call();
        let json = serde_json::to_string(&call).unwrap();
        assert!(json.contains("iroh"));
        let decoded: MctCall = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, call);
    }

    #[test]
    fn call_protocol_json_edge_roundtrips_and_rejects_malformed() {
        let request = protocol_request();
        let encoded_request = encode_call_protocol_request_json(&request).unwrap();
        let decoded_request = decode_call_protocol_request_json(&encoded_request).unwrap();
        assert_eq!(decoded_request, request);

        let mut hello = admitted_hello();
        hello.hello_outcome = HelloOutcome::Denied;
        let evaluation = evaluate_call_protocol(&request, &hello, eval_ids());
        let reply = call_reply_from_evaluation(
            ReplyId::new("reply-denied")
                .expect("string ID literal/generated value must be non-empty"),
            &evaluation,
            None,
            ObservationId::new("obs-reply-denied")
                .expect("string ID literal/generated value must be non-empty"),
        );
        let encoded_reply = encode_call_protocol_reply_json(&reply).unwrap();
        let decoded_reply = decode_call_protocol_reply_json(&encoded_reply).unwrap();
        assert_eq!(decoded_reply, reply);
        assert_eq!(
            decoded_reply.reply_outcome,
            CallProtocolReplyOutcome::Denied
        );

        assert!(matches!(
            decode_call_protocol_request_json(b"not json"),
            Err(MctKernelError::DecodeCallProtocolJson { .. })
        ));
    }

    #[test]
    fn call_protocol_json_edge_rejects_invalid_domain_values_with_typed_kernel_error() {
        let mut request = protocol_request();
        request.call.target.namespace.clear();

        assert!(matches!(
            encode_call_protocol_request_json(&request),
            Err(MctKernelError::InvalidField {
                record: "OperationTarget",
                field: "namespace",
                reason: crate::InvalidFieldReason::Empty,
            })
        ));

        let invalid_json = serde_json::to_vec(&request).unwrap();
        assert!(matches!(
            decode_call_protocol_request_json(&invalid_json),
            Err(MctKernelError::InvalidField {
                record: "OperationTarget",
                field: "namespace",
                reason: crate::InvalidFieldReason::Empty,
            })
        ));

        let mut request = protocol_request();
        request.payload = MctCallPayloadHandle::Empty;
        assert!(matches!(
            encode_call_protocol_request_json(&request),
            Err(MctKernelError::PayloadSizeMismatch {
                call_size_bytes: 5,
                handle_size_bytes: 0,
            })
        ));

        let mut invalid_timestamp = serde_json::to_value(protocol_request()).unwrap();
        invalid_timestamp["call"]["deadline"] = serde_json::json!("1772323200");
        let invalid_timestamp_json = serde_json::to_vec(&invalid_timestamp).unwrap();
        assert!(matches!(
            decode_call_protocol_request_json(&invalid_timestamp_json),
            Err(MctKernelError::DecodeCallProtocolJson { .. })
        ));
    }

    #[test]
    fn call_envelope_roundtrip_preserves_semantic_call_across_edges() {
        let request = protocol_request();
        let typed_call = request.call.clone();

        let encoded_request = encode_call_protocol_request_json(&request).unwrap();
        let decoded_request = decode_call_protocol_request_json(&encoded_request).unwrap();
        assert_eq!(decoded_request.call, typed_call);
        assert_eq!(decoded_request.call.target.namespace, "patina");
        assert_eq!(decoded_request.call.target.interface_name, "echo");
        assert_eq!(decoded_request.call.target.function_name, "echo");
        assert_eq!(
            decoded_request.payload.declared_size_bytes(),
            decoded_request.call.payload_metadata.size_bytes
        );
        let encoded_json = serde_json::to_value(&request.payload).unwrap();
        assert_eq!(encoded_json["payload_kind"], "inline_payload");
        assert_eq!(encoded_json["inline_payload_ref"], "payload-1");
        assert_eq!(encoded_json["content_type"], "text/plain");
        assert_eq!(encoded_json["size_bytes"], 5);
        assert!(encoded_json.get("approximate_size_bytes").is_none());
        assert!(encoded_json.get("blob_ref").is_none());

        let evaluation = evaluate_call_protocol(&decoded_request, &admitted_hello(), eval_ids());
        assert_eq!(evaluation.call_id, Some(typed_call.call_id.clone()));
        assert_eq!(evaluation.outcome, CallProtocolOutcome::AcceptedForRouting);

        let reply = call_reply_from_evaluation(
            ReplyId::new("reply-success")
                .expect("string ID literal/generated value must be non-empty"),
            &evaluation,
            Some(
                ResultRef::new("result-call-1")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            ObservationId::new("obs-reply-success")
                .expect("string ID literal/generated value must be non-empty"),
        );
        let decoded_reply =
            decode_call_protocol_reply_json(&encode_call_protocol_reply_json(&reply).unwrap())
                .unwrap();
        assert_eq!(
            decoded_reply.protocol_request_id,
            request.protocol_request_id
        );
        assert_eq!(decoded_reply.decision_id, evaluation.decision_id);
        assert_eq!(
            decoded_reply.reply_outcome,
            CallProtocolReplyOutcome::Success
        );
        assert_eq!(
            decoded_reply.result_ref,
            Some(
                ResultRef::new("result-call-1")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
    }

    #[test]
    fn admitted_hello_allows_call_for_routing() {
        let evaluation = evaluate_call_protocol(&protocol_request(), &admitted_hello(), eval_ids());
        assert!(evaluation.is_accepted_for_routing());
        assert_eq!(evaluation.reason, CallProtocolReason::AcceptedForRouting);
        assert_eq!(
            evaluation.call_id,
            Some(
                CallId::new("call-1").expect("string ID literal/generated value must be non-empty")
            )
        );
    }

    #[test]
    fn call_without_admitted_hello_is_denied() {
        let mut hello = admitted_hello();
        hello.hello_outcome = HelloOutcome::Denied;
        let evaluation = evaluate_call_protocol(&protocol_request(), &hello, eval_ids());
        assert_eq!(evaluation.outcome, CallProtocolOutcome::Denied);
        assert_eq!(evaluation.reason, CallProtocolReason::HelloNotAdmitted);
    }

    #[test]
    fn hello_without_call_alpn_does_not_authorize_call() {
        let mut hello = admitted_hello();
        hello.accepted_alpns.clear();
        let evaluation = evaluate_call_protocol(&protocol_request(), &hello, eval_ids());
        assert_eq!(evaluation.reason, CallProtocolReason::AlpnNotAdmitted);
    }

    #[test]
    fn endpoint_mismatch_is_denied() {
        let mut request = protocol_request();
        request.received_over.endpoint_id = EndpointIdText::new("endpoint-b")
            .expect("string ID literal/generated value must be non-empty");
        let evaluation = evaluate_call_protocol(&request, &admitted_hello(), eval_ids());
        assert_eq!(evaluation.reason, CallProtocolReason::EndpointMismatch);
        assert_eq!(evaluation.safe_message, "not authorized");
    }

    #[test]
    fn newer_call_claim_cannot_reuse_hello_admitted_at_an_older_policy_revision() {
        let mut request = protocol_request();
        request.authority.policy_revision = 2;
        request.call.authority_context.policy_revision = 2;
        let mut context = eval_ids();
        context.current_peer_authority.policy_revision = 2;
        context.current_peer_authority.bindings[0].policy_revision = 2;

        let evaluation = evaluate_call_protocol(&request, &admitted_hello(), context);

        assert_eq!(evaluation.outcome, CallProtocolOutcome::Denied);
        assert_eq!(evaluation.reason, CallProtocolReason::PolicyRevisionStale);
        assert_eq!(evaluation.safe_message, "not authorized");
    }

    #[test]
    fn call_authority_binding_must_match_admitted_hello() {
        let mut request = protocol_request();
        request.authority.peer_binding_id = PeerBindingId::new("binding-other")
            .expect("string ID literal/generated value must be non-empty");

        let evaluation = evaluate_call_protocol(&request, &admitted_hello(), eval_ids());

        assert_eq!(evaluation.outcome, CallProtocolOutcome::Denied);
        assert_eq!(evaluation.reason, CallProtocolReason::BindingMismatch);
        assert_eq!(evaluation.safe_message, "not authorized");
    }

    #[test]
    fn call_caller_must_match_admitted_hello_node() {
        let mut request = protocol_request();
        request.call.caller.node_id = MctNodeId::new("node-other")
            .expect("string ID literal/generated value must be non-empty");

        let evaluation = evaluate_call_protocol(&request, &admitted_hello(), eval_ids());

        assert_eq!(evaluation.outcome, CallProtocolOutcome::Denied);
        assert_eq!(evaluation.reason, CallProtocolReason::CallerMismatch);
        assert_eq!(evaluation.safe_message, "not authorized");
    }

    #[test]
    fn call_authority_vision_must_match_admitted_hello_and_call() {
        let mut request = protocol_request();
        request.authority.vision_id = VisionId::new("vision-other")
            .expect("string ID literal/generated value must be non-empty");

        let evaluation = evaluate_call_protocol(&request, &admitted_hello(), eval_ids());

        assert_eq!(evaluation.outcome, CallProtocolOutcome::Denied);
        assert_eq!(evaluation.reason, CallProtocolReason::VisionMismatch);
        assert_eq!(evaluation.safe_message, "not authorized");
    }

    #[test]
    fn payload_metadata_mismatch_is_malformed() {
        let mut request = protocol_request();
        request.payload = MctCallPayloadHandle::Empty;
        let evaluation = evaluate_call_protocol(&request, &admitted_hello(), eval_ids());
        assert_eq!(evaluation.outcome, CallProtocolOutcome::Malformed);
        assert_eq!(
            evaluation.reason,
            CallProtocolReason::PayloadMetadataMismatch
        );
        let reply = call_reply_from_evaluation(
            ReplyId::new("reply-1").expect("string ID literal/generated value must be non-empty"),
            &evaluation,
            None,
            ObservationId::new("obs-reply")
                .expect("string ID literal/generated value must be non-empty"),
        );
        assert_eq!(reply.reply_outcome, CallProtocolReplyOutcome::Malformed);
    }

    fn digest_hex(ch: char) -> String {
        std::iter::repeat_n(ch, 64).collect()
    }

    fn observed_payload(size_bytes: u64, digest: String) -> MctPayloadIntegrityObservation {
        MctPayloadIntegrityObservation {
            inline_bytes_present: true,
            content_addressed_blob_fetch_attempted: false,
            observed_size_bytes: Some(size_bytes),
            observed_blake3_digest_hex: Some(digest),
        }
    }

    #[test]
    fn payload_integrity_decisions_cover_request_mismatch_classes() {
        let handle = MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: "payload-1".into(),
            content_type: "application/json".into(),
            size_bytes: 5,
            blake3_digest_hex: digest_hex('a'),
        };

        let matched = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &handle,
            &observed_payload(5, digest_hex('a')),
            32,
        );
        assert_eq!(matched.outcome, PayloadIntegrityOutcome::Matched);
        assert_eq!(matched.reason, PayloadIntegrityReason::IntegrityMatched);

        let size_mismatch = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &handle,
            &observed_payload(4, digest_hex('a')),
            32,
        );
        assert_eq!(size_mismatch.outcome, PayloadIntegrityOutcome::Mismatch);
        assert_eq!(
            size_mismatch.reason,
            PayloadIntegrityReason::PayloadSizeMismatch
        );

        let digest_mismatch = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &handle,
            &observed_payload(5, digest_hex('b')),
            32,
        );
        assert_eq!(
            digest_mismatch.reason,
            PayloadIntegrityReason::PayloadDigestMismatch
        );

        let missing = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &handle,
            &MctPayloadIntegrityObservation::missing_inline_bytes(),
            32,
        );
        assert_eq!(
            missing.reason,
            PayloadIntegrityReason::PayloadMissingInlineBytes
        );

        let unexpected = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &MctCallPayloadHandle::Empty,
            &observed_payload(1, digest_hex('c')),
            32,
        );
        assert_eq!(
            unexpected.reason,
            PayloadIntegrityReason::PayloadUnexpectedInlineBytes
        );

        let invalid_digest = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &MctCallPayloadHandle::InlinePayload {
                inline_payload_ref: "payload-invalid-digest".into(),
                content_type: "application/json".into(),
                size_bytes: 5,
                blake3_digest_hex: "not-a-blake3-hex".into(),
            },
            &observed_payload(5, digest_hex('a')),
            32,
        );
        assert_eq!(
            invalid_digest.reason,
            PayloadIntegrityReason::InvalidPayloadDigest
        );

        let uppercase_declared = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &MctCallPayloadHandle::InlinePayload {
                inline_payload_ref: "payload-upper-digest".into(),
                content_type: "application/json".into(),
                size_bytes: 5,
                blake3_digest_hex: digest_hex('A'),
            },
            &observed_payload(5, digest_hex('a')),
            32,
        );
        assert_eq!(
            uppercase_declared.reason,
            PayloadIntegrityReason::InvalidPayloadDigest
        );

        let uppercase_observed = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &handle,
            &observed_payload(5, digest_hex('A')),
            32,
        );
        assert_eq!(
            uppercase_observed.reason,
            PayloadIntegrityReason::InvalidPayloadDigest
        );

        let declared_too_large = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &MctCallPayloadHandle::InlinePayload {
                inline_payload_ref: "payload-large".into(),
                content_type: "application/json".into(),
                size_bytes: 33,
                blake3_digest_hex: digest_hex('a'),
            },
            &observed_payload(33, digest_hex('a')),
            32,
        );
        assert_eq!(
            declared_too_large.reason,
            PayloadIntegrityReason::PayloadDeclaredTooLarge
        );

        let actual_too_large = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &handle,
            &observed_payload(33, digest_hex('a')),
            32,
        );
        assert_eq!(
            actual_too_large.reason,
            PayloadIntegrityReason::PayloadActualTooLarge
        );
    }

    #[test]
    fn payload_integrity_decisions_cover_local_content_addressed_blob() {
        let handle = MctCallPayloadHandle::ContentAddressedBlob {
            digest: digest_hex('a'),
            blob_ref: "blake3:aaaaaaaa".into(),
            content_type: "application/octet-stream".into(),
            size_bytes: 5,
        };

        let remote_declaration_only = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &handle,
            &MctPayloadIntegrityObservation::missing_inline_bytes(),
            32,
        );
        assert_eq!(
            remote_declaration_only.reason,
            PayloadIntegrityReason::IntegrityMatched
        );

        let missing = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &handle,
            &MctPayloadIntegrityObservation::missing_content_addressed_blob(),
            32,
        );
        assert_eq!(
            missing.reason,
            PayloadIntegrityReason::PayloadBlobUnavailable
        );
        assert_eq!(missing.safe_message, "payload blob unavailable");

        let matched = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &handle,
            &MctPayloadIntegrityObservation {
                inline_bytes_present: true,
                content_addressed_blob_fetch_attempted: true,
                observed_size_bytes: Some(5),
                observed_blake3_digest_hex: Some(digest_hex('a')),
            },
            32,
        );
        assert_eq!(matched.reason, PayloadIntegrityReason::IntegrityMatched);

        let tampered = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &handle,
            &MctPayloadIntegrityObservation {
                inline_bytes_present: true,
                content_addressed_blob_fetch_attempted: true,
                observed_size_bytes: Some(5),
                observed_blake3_digest_hex: Some(digest_hex('b')),
            },
            32,
        );
        assert_eq!(
            tampered.reason,
            PayloadIntegrityReason::PayloadDigestMismatch
        );
    }

    #[test]
    fn payload_integrity_decisions_cover_reply_result_mismatch_classes() {
        let handle = MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: "result-1".into(),
            content_type: "application/json".into(),
            size_bytes: 5,
            blake3_digest_hex: digest_hex('a'),
        };

        let digest_mismatch = evaluate_payload_integrity(
            PayloadIntegritySubject::ReplyResult,
            &handle,
            &observed_payload(5, digest_hex('b')),
            32,
        );
        assert_eq!(
            digest_mismatch.reason,
            PayloadIntegrityReason::ResultPayloadIntegrityMismatch
        );
        assert_eq!(
            digest_mismatch.safe_message,
            "result payload integrity mismatch"
        );

        let oversized = evaluate_payload_integrity(
            PayloadIntegritySubject::ReplyResult,
            &handle,
            &observed_payload(33, digest_hex('a')),
            32,
        );
        assert_eq!(
            oversized.reason,
            PayloadIntegrityReason::ResultPayloadTooLarge
        );
        assert_eq!(oversized.safe_message, "result payload too large");
    }

    #[test]
    fn call_protocol_reply_roundtrips_result_payload_handle() {
        let evaluation = evaluate_call_protocol(&protocol_request(), &admitted_hello(), eval_ids());
        let reply = call_reply_from_evaluation_with_result_payload(
            ReplyId::new("reply-result-payload")
                .expect("string ID literal/generated value must be non-empty"),
            &evaluation,
            Some(
                ResultRef::new("result-call-1")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            MctCallPayloadHandle::InlinePayload {
                inline_payload_ref: "result-1".into(),
                content_type: "application/json".into(),
                size_bytes: 5,
                blake3_digest_hex: digest_hex('a'),
            },
            ObservationId::new("obs-reply-result-payload")
                .expect("string ID literal/generated value must be non-empty"),
        );

        let decoded =
            decode_call_protocol_reply_json(&encode_call_protocol_reply_json(&reply).unwrap())
                .unwrap();
        assert_eq!(decoded.result_payload, reply.result_payload);
        assert_eq!(decoded.result_payload.declared_size_bytes(), 5);
    }

    #[test]
    fn call_protocol_reply_roundtrips_route_taken_wire_field() {
        let evaluation = evaluate_call_protocol(&protocol_request(), &admitted_hello(), eval_ids());
        let route_taken = RouteTaken {
            node_id: MctNodeId::new("node-route")
                .expect("string ID literal/generated value must be non-empty"),
            child_id: Some(
                ChildId::new("child-echo")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            runtime_kind: RuntimeKind::Process,
        };
        let reply = call_reply_from_evaluation_with_result_payload_and_route(
            ReplyId::new("reply-route-taken")
                .expect("string ID literal/generated value must be non-empty"),
            &evaluation,
            Some(
                ResultRef::new("result-call-1")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            MctCallPayloadHandle::Empty,
            Some(route_taken.clone()),
            ObservationId::new("obs-reply-route-taken")
                .expect("string ID literal/generated value must be non-empty"),
        );

        let decoded =
            decode_call_protocol_reply_json(&encode_call_protocol_reply_json(&reply).unwrap())
                .unwrap();
        assert_eq!(decoded.route_taken, Some(route_taken));
    }

    #[test]
    fn reply_validation_enforces_route_taken_presence_rule() {
        let evaluation = evaluate_call_protocol(&protocol_request(), &admitted_hello(), eval_ids());
        let route_taken = RouteTaken {
            node_id: MctNodeId::new("node-route")
                .expect("string ID literal/generated value must be non-empty"),
            child_id: None,
            runtime_kind: RuntimeKind::Process,
        };
        let cancelled_with_route = MctCallProtocolReply {
            reply_id: ReplyId::new("reply-cancelled-route")
                .expect("string ID literal/generated value must be non-empty"),
            protocol_request_id: evaluation.protocol_request_id.clone(),
            decision_id: evaluation.decision_id.clone(),
            result_ref: None,
            result_payload: MctCallPayloadHandle::Empty,
            route_taken: Some(route_taken.clone()),
            reply_outcome: CallProtocolReplyOutcome::Cancelled,
            retry_directive: CallProtocolRetryDirective::None,
            safe_message: "cancelled".into(),
            reply_observation_id: ObservationId::new("obs-reply-cancelled-route")
                .expect("string ID literal/generated value must be non-empty"),
        };
        assert_eq!(
            cancelled_with_route.validate().unwrap_err().to_string(),
            "invalid MctCallProtocolReply.route_taken for cancelled reply outcome"
        );

        let cancelled_without_route = MctCallProtocolReply {
            route_taken: None,
            reply_outcome: CallProtocolReplyOutcome::Cancelled,
            ..cancelled_with_route
        };
        cancelled_without_route
            .validate()
            .expect("cancelled reply without a route");
    }

    #[test]
    fn idempotency_decision_replays_matches_and_refuses_other_cases() {
        let fingerprint = MctIdempotencyFingerprint {
            target: "patina/echo.echo".into(),
            call_id: CallId::new("call-idem").unwrap(),
            payload_digest: "digest-a".into(),
        };
        let completed = MctIdempotencyStoredEntry {
            fingerprint: fingerprint.clone(),
            state: MctIdempotencyEntryState::Completed,
            expires_at: Timestamp::new("2026-07-10T00:12:00Z").unwrap(),
        };
        assert_eq!(
            evaluate_idempotency_request(
                &fingerprint,
                Some(&completed),
                1,
                256,
                &Timestamp::new("2026-07-10T00:01:00Z").unwrap(),
            )
            .reason,
            MctIdempotencyReason::ReplayCompleted
        );

        let mut mismatch = fingerprint.clone();
        mismatch.payload_digest = "digest-b".into();
        assert_eq!(
            evaluate_idempotency_request(
                &mismatch,
                Some(&completed),
                1,
                256,
                &Timestamp::new("2026-07-10T00:01:00Z").unwrap(),
            )
            .reason,
            MctIdempotencyReason::IdempotencyKeyReuseMismatch
        );

        let in_flight = MctIdempotencyStoredEntry {
            fingerprint: fingerprint.clone(),
            state: MctIdempotencyEntryState::InFlight,
            expires_at: Timestamp::new("2026-07-10T00:12:00Z").unwrap(),
        };
        assert_eq!(
            evaluate_idempotency_request(
                &fingerprint,
                Some(&in_flight),
                1,
                256,
                &Timestamp::new("2026-07-10T00:01:00Z").unwrap(),
            )
            .reason,
            MctIdempotencyReason::IdempotencyInProgress
        );
        assert_eq!(
            evaluate_idempotency_request(
                &fingerprint,
                None,
                256,
                256,
                &Timestamp::new("2026-07-10T00:01:00Z").unwrap(),
            )
            .reason,
            MctIdempotencyReason::IdempotencyBudgetFull
        );
        assert_eq!(
            evaluate_idempotency_request(
                &fingerprint,
                Some(&completed),
                1,
                256,
                &Timestamp::new("2026-07-10T00:13:00Z").unwrap(),
            )
            .reason,
            MctIdempotencyReason::ExecuteFresh
        );
    }

    #[test]
    fn result_route_taken_follows_reply_presence_rule() {
        let route = RouteTaken {
            node_id: MctNodeId::new("node-route")
                .expect("string ID literal/generated value must be non-empty"),
            child_id: None,
            runtime_kind: RuntimeKind::Process,
        };
        let mut result = MctResult {
            call_id: CallId::new("call-1")
                .expect("string ID literal/generated value must be non-empty"),
            outcome: ResultOutcome::Denied,
            route_taken: None,
            authority_decision_ref: DecisionId::new("decision-1")
                .expect("string ID literal/generated value must be non-empty"),
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: 0,
                output_size_bytes: None,
            },
            result_payload: MctCallPayloadHandle::Empty,
            requester_message: "not authorized".into(),
            audit_ref: AuditRef::new("audit-1")
                .expect("string ID literal/generated value must be non-empty"),
        };
        assert!(result.validate().is_ok());

        for outcome in [ResultOutcome::Denied, ResultOutcome::Cancelled] {
            result.outcome = outcome;
            result.route_taken = Some(route.clone());
            assert!(matches!(
                result.validate(),
                Err(MctKernelError::InvalidRouteTakenPresence {
                    record: "MctResult",
                    ..
                })
            ));
            result.route_taken = None;
            assert!(result.validate().is_ok());
        }

        for outcome in [
            ResultOutcome::Success,
            ResultOutcome::Failed,
            ResultOutcome::TimedOut,
        ] {
            result.outcome = outcome;
            result.route_taken = Some(route.clone());
            assert!(result.validate().is_ok());
        }
    }
}

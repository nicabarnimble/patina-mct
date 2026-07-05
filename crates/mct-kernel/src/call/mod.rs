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
    /// MCT node on whose behalf the call is made.
    pub node_id: MctNodeId,
    /// Optional human or service principal associated with the call.
    pub user_id: Option<UserId>,
    /// Vision boundary in which the call claims authority.
    pub vision_id: VisionId,
    /// Optional project boundary for routing and audit projections.
    pub project_id: Option<ProjectId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// WIT-shaped function identity targeted by a call.
///
/// All fields must be non-empty; substrate-specific export names are normalized
/// to this namespace/interface/function triple before authority checks.
pub struct OperationTarget {
    /// WIT package namespace containing the interface.
    pub namespace: String,
    /// WIT interface name exported by the child.
    pub interface_name: String,
    /// Function name requested within the interface.
    pub function_name: String,
}

impl OperationTarget {
    /// Builds an operation target after validating that no WIT identity part is blank.
    ///
    /// # Errors
    ///
    /// Returns [`MctKernelError::InvalidField`] when namespace, interface, or
    /// function is empty or whitespace.
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

    /// Validates that every WIT identity segment is present.
    ///
    /// # Errors
    ///
    /// Returns [`MctKernelError::InvalidField`] for an empty or whitespace-only
    /// namespace, interface, or function.
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
    /// Size claim that must match the protocol payload handle size.
    pub approximate_size_bytes: u64,
    /// Whether the adapter says the payload contains secret-scoped material.
    pub contains_secret_scoped_material: bool,
}

impl PayloadMetadata {
    /// Validates that every WIT identity segment is present.
    ///
    /// # Errors
    ///
    /// Returns [`MctKernelError::InvalidField`] for an empty or whitespace-only
    /// namespace, interface, or function.
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank(
            "PayloadMetadata",
            "data_classification",
            &self.data_classification,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Revision numbers of authority inputs observed when the call was formed.
///
/// Protocol evaluation rejects calls whose call-side policy or grants revision
/// is older than the authority asserted by the admitted hello.
pub struct AuthorityContextSnapshot {
    /// Node-wide policy revision included in the call authority snapshot.
    pub policy_revision: u64,
    /// Toy-grant catalog revision included in the call authority snapshot.
    pub grants_revision: u64,
    /// Vision-specific policy revision visible to routing decisions.
    pub vision_policy_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Trace identifiers carried through call, route, result, and observations.
pub struct TraceContext {
    /// End-to-end trace that joins protocol, routing, and execution facts.
    pub trace_id: TraceId,
    /// Span for this call within the trace.
    pub span_id: SpanId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Adapter surface that constructed the semantic call.
pub enum CallOrigin {
    /// Call arrived through the MCT peer protocol over Iroh.
    Iroh,
    /// Call was projected from a JVM adapter.
    JvmAdapter,
    /// Call originated inside the WASM host boundary.
    WasmHost,
    /// Call came from a local process harness.
    ProcessHarness,
    /// Call was submitted by a local CLI command.
    Cli,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Immutable semantic unit of requested work.
///
/// An adapter constructs exactly one call from protocol facts. Authority checks
/// compare caller, target, metadata, revisions, deadline, and origin without
/// reading payload bytes.
pub struct MctCall {
    /// Stable identifier used by results, routes, and observations for this work.
    pub call_id: CallId,
    /// Authority-bearing caller asserted by the adapter.
    pub caller: CallerIdentity,
    /// WIT function the caller wants invoked.
    pub target: OperationTarget,
    /// Payload summary used by policy and route selection.
    pub payload_metadata: PayloadMetadata,
    /// Policy and grants revisions that accompanied call construction.
    pub authority_context: AuthorityContextSnapshot,
    /// Adapter-supplied deadline for completing the call.
    pub deadline: Timestamp,
    /// Trace identifiers copied into derived observations.
    pub trace_context: TraceContext,
    /// Adapter boundary that produced the call.
    pub origin: CallOrigin,
}

impl CallerIdentity {
    /// Validates that every WIT identity segment is present.
    ///
    /// # Errors
    ///
    /// Returns [`MctKernelError::InvalidField`] for an empty or whitespace-only
    /// namespace, interface, or function.
    pub fn validate(&self) -> MctKernelResult<()> {
        Ok(())
    }
}

impl TraceContext {
    /// Validates that every WIT identity segment is present.
    ///
    /// # Errors
    ///
    /// Returns [`MctKernelError::InvalidField`] for an empty or whitespace-only
    /// namespace, interface, or function.
    pub fn validate(&self) -> MctKernelResult<()> {
        Ok(())
    }
}

impl MctCall {
    /// Validates that every WIT identity segment is present.
    ///
    /// # Errors
    ///
    /// Returns [`MctKernelError::InvalidField`] for an empty or whitespace-only
    /// namespace, interface, or function.
    pub fn validate(&self) -> MctKernelResult<()> {
        self.caller.validate()?;
        self.target.validate()?;
        self.payload_metadata.validate()?;
        ensure_non_blank("MctCall", "deadline", self.deadline.as_str())?;
        self.trace_context.validate()?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Execution substrate selected for a route or result projection.
pub enum RuntimeKind {
    /// Local process-backed child.
    Process,
    /// JVM-hosted child adapter.
    JvmChild,
    /// WASM component child.
    WasmComponent,
    /// Remote Mother reached through peer routing.
    RemotePeer,
    /// Mother-internal implementation path.
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Route actually used to execute or attempt a call.
pub struct RouteTaken {
    /// Node that handled the call.
    pub node_id: MctNodeId,
    /// Child selected on that node, when execution went through a child.
    pub child_id: Option<ChildId>,
    /// Runtime class used for the selected execution path.
    pub runtime_kind: RuntimeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Caller-safe execution timing and byte counts for a terminal result.
pub struct ExecutionSummary {
    /// End-to-end elapsed time observed by the adapter.
    pub wall_time_ms: u64,
    /// Time spent running child code, if measured separately.
    pub execution_time_ms: Option<u64>,
    /// Time spent waiting before execution began, if measured.
    pub queue_wait_ms: Option<u64>,
    /// Input byte count supplied to the execution path.
    pub input_size_bytes: u64,
    /// Output byte count returned by the execution path, if any.
    pub output_size_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Terminal outcome class exposed to the caller.
pub enum ResultOutcome {
    /// Work completed successfully.
    Success,
    /// Authority denied the work before execution completed.
    Denied,
    /// Execution failed without granting the caller internal details.
    Failed,
    /// Work exceeded its deadline or runtime limit.
    TimedOut,
    /// Work was cancelled by the runtime or operator.
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Caller-safe terminal answer for an MCT call.
///
/// The result references the authority decision and audit evidence instead of
/// embedding privileged details in `requester_message`.
pub struct MctResult {
    /// Identifier of the call this result answers.
    pub call_id: CallId,
    /// Closed outcome category safe to disclose to the requester.
    pub outcome: ResultOutcome,
    /// Execution path used, absent when the call never reached execution.
    pub route_taken: Option<RouteTaken>,
    /// Decision that authorized or denied the terminal path.
    pub authority_decision_ref: DecisionId,
    /// Timing and size facts safe for result consumers.
    pub execution_summary: ExecutionSummary,
    /// Declared result payload handle; empty for denied/no-payload results.
    pub result_payload: MctCallPayloadHandle,
    /// Caller-safe message; privileged denial reasons live in observations.
    pub requester_message: String,
    /// Opaque reference for audit lookup outside the caller response.
    pub audit_ref: AuditRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Authority facts carried from a successful hello into `mct/call/0`.
///
/// Call evaluation requires these facts to match the admitted hello, the
/// connection presentation, and the call authority snapshot.
pub struct MctCallProtocolAuthority {
    /// Hello admission decision the call claims to extend.
    pub hello_decision_id: DecisionId,
    /// Peer binding selected during hello admission.
    pub peer_binding_id: PeerBindingId,
    /// Vision admitted by hello and required to match the call caller.
    pub vision_id: VisionId,
    /// ALPN admitted for the call phase; must be `mct/call/0`.
    pub accepted_alpn: String,
    /// Transport endpoint that must match the received connection.
    pub endpoint_id: EndpointIdText,
    /// Minimum policy revision the call snapshot must cover.
    pub policy_revision: u64,
    /// Minimum grants revision the call snapshot must cover.
    pub grants_revision: u64,
}

impl MctCallProtocolAuthority {
    /// Validates that every WIT identity segment is present.
    ///
    /// # Errors
    ///
    /// Returns [`MctKernelError::InvalidField`] for an empty or whitespace-only
    /// namespace, interface, or function.
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank(
            "MctCallProtocolAuthority",
            "accepted_alpn",
            &self.accepted_alpn,
        )?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "payload_kind", rename_all = "snake_case")]
/// Adapter-neutral reference to call payload bytes.
///
/// Each non-empty variant carries a size that must equal
/// [`MctCall::payload_metadata`].`approximate_size_bytes`; the kernel validates
/// the handle shape but never dereferences payload storage.
pub enum MctCallPayloadHandle {
    /// Payload stored inline or in an adapter-local inline buffer.
    InlinePayload {
        /// Non-blank adapter-local reference to the inline bytes.
        inline_payload_ref: String,
        /// Non-blank media type or schema label for the bytes.
        content_type: String,
        /// Exact byte size declared for inline integrity validation.
        size_bytes: u64,
        /// Declared BLAKE3 digest of the inline bytes, encoded as lowercase hex.
        blake3_digest_hex: String,
    },
    /// Payload stored in content-addressed storage.
    ContentAddressedBlob {
        /// Non-blank digest identifying the blob contents.
        digest: String,
        /// Non-blank adapter reference used to retrieve the blob.
        blob_ref: String,
        /// Non-blank media type or schema label for the blob.
        content_type: String,
        /// Exact byte size verified when the content-addressed blob is ingested.
        size_bytes: u64,
    },
    /// Payload held outside MCT-managed storage.
    ExternalReference {
        /// Non-blank reference whose dereference is an adapter responsibility.
        external_ref: String,
        /// Optional media type; if present it must not be blank.
        content_type: Option<String>,
        /// Claimed byte size used for validation against call metadata.
        approximate_size_bytes: u64,
    },
    /// No payload bytes are associated with the call.
    Empty,
}

impl MctCallPayloadHandle {
    /// Returns the byte-size claim carried by this handle, or zero for empty payloads.
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

    /// Returns the byte-size claim carried by this handle, or zero for empty payloads.
    pub fn approximate_size_bytes(&self) -> u64 {
        self.declared_size_bytes()
    }

    /// Validates that every WIT identity segment is present.
    ///
    /// # Errors
    ///
    /// Returns [`MctKernelError::InvalidField`] for an empty or whitespace-only
    /// namespace, interface, or function.
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
/// Protocol side whose payload integrity is being evaluated.
pub enum PayloadIntegritySubject {
    /// Request payload bytes received before authority evaluation.
    Request,
    /// Reply result payload bytes received by the caller.
    ReplyResult,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Pure kernel outcome for declared-versus-observed payload integrity facts.
pub enum PayloadIntegrityOutcome {
    /// Declared facts match observed adapter facts.
    Matched,
    /// Declared or observed facts failed closed.
    Mismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Typed reason for a payload integrity decision.
pub enum PayloadIntegrityReason {
    /// Declared facts match observed facts.
    IntegrityMatched,
    /// Declared request payload size exceeds the fixed inline cap.
    PayloadDeclaredTooLarge,
    /// Observed request payload size exceeds the fixed inline cap.
    PayloadActualTooLarge,
    /// Declared and observed request payload sizes differ.
    PayloadSizeMismatch,
    /// Declared and observed request payload digests differ.
    PayloadDigestMismatch,
    /// Inline bytes were required by the handle but absent at the adapter edge.
    PayloadMissingInlineBytes,
    /// Inline bytes were supplied where the handle did not declare inline bytes.
    PayloadUnexpectedInlineBytes,
    /// A declared or observed BLAKE3 digest was not valid hex syntax.
    InvalidPayloadDigest,
    /// Result payload size exceeds the fixed inline result cap.
    ResultPayloadTooLarge,
    /// Declared and observed result payload integrity facts differ.
    ResultPayloadIntegrityMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Adapter-observed payload facts supplied to the pure kernel integrity check.
pub struct MctPayloadIntegrityObservation {
    /// Whether inline bytes were actually present at the adapter edge.
    pub inline_bytes_present: bool,
    /// Byte count observed by the adapter after decoding/fetching bytes.
    pub observed_size_bytes: Option<u64>,
    /// BLAKE3 digest observed by the adapter, encoded as lowercase hex.
    pub observed_blake3_digest_hex: Option<String>,
}

impl MctPayloadIntegrityObservation {
    /// Builds an observation for a handle that did not have required inline bytes.
    pub fn missing_inline_bytes() -> Self {
        Self {
            inline_bytes_present: false,
            observed_size_bytes: None,
            observed_blake3_digest_hex: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Pure decision comparing declared payload-handle facts with adapter-observed facts.
pub struct MctPayloadIntegrityDecision {
    /// Request or reply side evaluated.
    pub subject: PayloadIntegritySubject,
    /// Closed decision outcome.
    pub outcome: PayloadIntegrityOutcome,
    /// Typed reason for audit and caller-safe projection.
    pub reason: PayloadIntegrityReason,
    /// Caller-safe projection of the decision.
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
    internal::evaluate_payload_integrity_internal(
        subject,
        handle,
        observed,
        max_inline_size_bytes,
    )
}

impl PayloadIntegrityReason {
    /// Projects a payload integrity reason into the call-protocol reason vocabulary.
    pub fn to_call_protocol_reason(self) -> CallProtocolReason {
        match self {
            Self::IntegrityMatched => CallProtocolReason::ResultRecorded,
            Self::PayloadDeclaredTooLarge => CallProtocolReason::PayloadDeclaredTooLarge,
            Self::PayloadActualTooLarge => CallProtocolReason::PayloadActualTooLarge,
            Self::PayloadSizeMismatch => CallProtocolReason::PayloadSizeMismatch,
            Self::PayloadDigestMismatch => CallProtocolReason::PayloadDigestMismatch,
            Self::PayloadMissingInlineBytes => CallProtocolReason::PayloadMissingInlineBytes,
            Self::PayloadUnexpectedInlineBytes => CallProtocolReason::PayloadUnexpectedInlineBytes,
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
    /// Request identifier for correlating the protocol reply.
    pub protocol_request_id: ProtocolRequestId,
    /// Hello-derived authority facts asserted by the caller.
    pub authority: MctCallProtocolAuthority,
    /// Connection facts supplied by the receiving adapter, not by the peer.
    pub received_over: crate::peer::IrohConnectionPresentation,
    /// Immutable semantic call constructed from the request.
    pub call: MctCall,
    /// Adapter-neutral payload reference whose size must match call metadata.
    pub payload: MctCallPayloadHandle,
    /// Optional retry key; when present it must be non-blank.
    pub idempotency_key: Option<String>,
    /// Observation recording receipt of this protocol request.
    pub received_observation_id: ObservationId,
}

impl MctCallProtocolRequest {
    /// Validates that every WIT identity segment is present.
    ///
    /// # Errors
    ///
    /// Returns [`MctKernelError::InvalidField`] for an empty or whitespace-only
    /// namespace, interface, or function.
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
        if self.payload.approximate_size_bytes()
            != self.call.payload_metadata.approximate_size_bytes
        {
            return Err(MctKernelError::PayloadSizeMismatch {
                call_size_bytes: self.call.payload_metadata.approximate_size_bytes,
                handle_size_bytes: self.payload.approximate_size_bytes(),
            });
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Kernel outcome for the `mct/call/0` protocol decision.
pub enum CallProtocolOutcome {
    /// Authority checks passed and an adapter may route the call.
    AcceptedForRouting,
    /// Request shape or metadata consistency failed validation.
    Malformed,
    /// Authority facts did not justify routing.
    Denied,
    /// Downstream execution failed after protocol admission.
    Failed,
    /// Downstream execution timed out after protocol admission.
    TimedOut,
    /// Downstream execution completed and a result reference may be present.
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Specific non-secret reason recorded for a call protocol evaluation.
pub enum CallProtocolReason {
    /// Prior hello decision was missing, denied, or did not match this request.
    HelloNotAdmitted,
    /// Hello admission did not include the call ALPN.
    AlpnNotAdmitted,
    /// Received transport endpoint differs from the admitted endpoint.
    EndpointMismatch,
    /// Request cites a peer binding not selected during hello.
    BindingMismatch,
    /// Call caller node differs from the node admitted by hello.
    CallerMismatch,
    /// Request, caller, and hello do not agree on one Vision.
    VisionMismatch,
    /// Peer binding was revoked before call routing.
    BindingRevoked,
    /// Peer binding expired before call routing.
    BindingExpired,
    /// Call authority snapshot is older than the admitted authority facts.
    PolicyRevisionStale,
    /// Request shape could not become a valid semantic call.
    MalformedCall,
    /// Payload handle size disagrees with call payload metadata.
    PayloadMetadataMismatch,
    /// Declared request payload size exceeds the fixed inline cap.
    PayloadDeclaredTooLarge,
    /// Observed request payload size exceeds the fixed inline cap.
    PayloadActualTooLarge,
    /// Declared and observed request payload sizes differ.
    PayloadSizeMismatch,
    /// Declared and observed request payload digests differ.
    PayloadDigestMismatch,
    /// Inline bytes required by the handle were absent.
    PayloadMissingInlineBytes,
    /// Inline bytes were supplied where the handle did not declare them.
    PayloadUnexpectedInlineBytes,
    /// A declared or observed payload digest had invalid syntax.
    InvalidPayloadDigest,
    /// The authorized child runtime cannot accept the verified payload content type.
    ChildPayloadContentTypeUnsupported,
    /// Result payload exceeded the fixed inline result cap.
    ResultPayloadTooLarge,
    /// Caller-side result payload size or digest verification failed.
    ResultPayloadIntegrityMismatch,
    /// Later authority checks denied the call after protocol admission.
    AuthorityDenied,
    /// No authorized route remained for the admitted call.
    NoRoute,
    /// Execution failed after routing.
    ExecutionFailed,
    /// Execution timed out after routing.
    ExecutionTimedOut,
    /// Evaluation accepted or recorded a terminal result.
    ResultRecorded,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Decision produced by `mct/call/0` authority evaluation.
///
/// Denied and malformed evaluations carry no route or result reference; safe
/// messages are caller-facing projections of the typed reason.
pub struct MctCallProtocolEvaluation {
    /// Unique decision identifier for this protocol evaluation.
    pub decision_id: DecisionId,
    /// Request this evaluation answers.
    pub protocol_request_id: ProtocolRequestId,
    /// Semantic call evaluated; present because protocol decoding succeeded.
    pub call_id: Option<CallId>,
    /// Route decision produced after admission, when one exists.
    pub route_decision_id: Option<DecisionId>,
    /// Result reference supplied after execution, when one exists.
    pub result_ref: Option<ResultRef>,
    /// Closed outcome class for the protocol decision.
    pub outcome: CallProtocolOutcome,
    /// Typed reason retained for audit and observation projection.
    pub reason: CallProtocolReason,
    /// Caller-safe message that must not disclose privileged policy detail.
    pub safe_message: String,
    /// Observation that records this evaluation.
    pub observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Wire-safe outcome class returned in a `mct/call/0` reply.
pub enum CallProtocolReplyOutcome {
    /// Work completed successfully.
    Success,
    /// Authority denied the work before execution completed.
    Denied,
    /// Execution failed without granting the caller internal details.
    Failed,
    /// Work exceeded its deadline or runtime limit.
    TimedOut,
    /// Work was cancelled by the runtime or operator.
    Cancelled,
    /// Request or reply shape was malformed before execution.
    Malformed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Wire-edge response for `mct/call/0`.
///
/// The reply carries only caller-safe outcome and an optional opaque result
/// reference; detailed authority reasons remain in observations.
pub struct MctCallProtocolReply {
    /// Unique identifier for this protocol reply.
    pub reply_id: ReplyId,
    /// Request identifier being answered.
    pub protocol_request_id: ProtocolRequestId,
    /// Evaluation decision that determined the reply.
    pub decision_id: DecisionId,
    /// Opaque result lookup reference, present only when one is safe to return.
    pub result_ref: Option<ResultRef>,
    /// Declared result payload handle returned with this reply.
    pub result_payload: MctCallPayloadHandle,
    /// Caller-facing outcome class.
    pub reply_outcome: CallProtocolReplyOutcome,
    /// Caller-safe message derived from the evaluation or execution path.
    pub safe_message: String,
    /// Observation recording emission of this reply.
    pub reply_observation_id: ObservationId,
}

impl MctCallProtocolReply {
    /// Validates that every WIT identity segment is present.
    ///
    /// # Errors
    ///
    /// Returns [`MctKernelError::InvalidField`] for an empty or whitespace-only
    /// namespace, interface, or function.
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank("MctCallProtocolReply", "safe_message", &self.safe_message)?;
        self.result_payload.validate()?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Caller-supplied IDs used when minting a call protocol evaluation.
pub struct CallEvaluationIds {
    /// Decision identifier assigned to the evaluation.
    pub decision_id: DecisionId,
    /// Observation identifier assigned to the evaluation evidence.
    pub observation_id: ObservationId,
}

/// Decides whether an admitted peer may submit this `mct/call/0` request.
///
/// The authority facts are the validated request, the prior hello admission,
/// and caller-supplied IDs. Returns `AcceptedForRouting` only when hello was
/// admitted and binding, caller node, vision, ALPN, endpoint, revisions, and
/// payload size all match. Absence or mismatch of authority is a denied or
/// malformed decision, not an error.
pub fn evaluate_call_protocol(
    request: &MctCallProtocolRequest,
    hello: &crate::peer::MctHelloAdmissionEvaluation,
    ids: CallEvaluationIds,
) -> MctCallProtocolEvaluation {
    internal::evaluate_call_protocol_internal(request, hello, ids)
}

impl MctCallProtocolEvaluation {
    /// Returns true only for evaluations that an adapter may route onward.
    pub fn is_accepted_for_routing(&self) -> bool {
        self.outcome == CallProtocolOutcome::AcceptedForRouting
    }
}

/// Validates and serializes a call protocol request for the JSON wire edge.
///
/// # Errors
///
/// Returns a kernel validation error before serialization if the request is
/// internally inconsistent, or [`MctKernelError::EncodeCallProtocolJson`] if
/// JSON encoding fails.
pub fn encode_call_protocol_request_json(
    request: &MctCallProtocolRequest,
) -> MctKernelResult<Vec<u8>> {
    request.validate()?;
    serde_json::to_vec(request).map_err(|source| MctKernelError::EncodeCallProtocolJson { source })
}

/// Decodes and validates a call protocol request from the JSON wire edge.
///
/// # Errors
///
/// Returns [`MctKernelError::DecodeCallProtocolJson`] for invalid JSON and a
/// kernel validation error for malformed authority, call, or payload facts.
pub fn decode_call_protocol_request_json(bytes: &[u8]) -> MctKernelResult<MctCallProtocolRequest> {
    let request: MctCallProtocolRequest = serde_json::from_slice(bytes)
        .map_err(|source| MctKernelError::DecodeCallProtocolJson { source })?;
    request.validate()?;
    Ok(request)
}

/// Validates and serializes a call protocol reply for the JSON wire edge.
///
/// # Errors
///
/// Returns a kernel validation error when the reply message is blank, or
/// [`MctKernelError::EncodeCallProtocolJson`] when JSON encoding fails.
pub fn encode_call_protocol_reply_json(reply: &MctCallProtocolReply) -> MctKernelResult<Vec<u8>> {
    reply.validate()?;
    serde_json::to_vec(reply).map_err(|source| MctKernelError::EncodeCallProtocolJson { source })
}

/// Decodes and validates a call protocol reply from the JSON wire edge.
///
/// # Errors
///
/// Returns [`MctKernelError::DecodeCallProtocolJson`] for invalid JSON and a
/// kernel validation error for an invalid reply shape.
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
/// failed, and timed-out evaluations retain their caller-safe outcome class.
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

/// Projects a protocol evaluation and result payload handle into a caller-safe reply shape.
pub fn call_reply_from_evaluation_with_result_payload(
    reply_id: ReplyId,
    evaluation: &MctCallProtocolEvaluation,
    result_ref: Option<ResultRef>,
    result_payload: MctCallPayloadHandle,
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
    };

    MctCallProtocolReply {
        reply_id,
        protocol_request_id: evaluation.protocol_request_id.clone(),
        decision_id: evaluation.decision_id.clone(),
        result_ref,
        result_payload,
        reply_outcome,
        safe_message: evaluation.safe_message.clone(),
        reply_observation_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::{
        ConnectionSide, HelloOutcome, HelloReason, IrohConnectionPresentation, MCT_CALL_ALPN,
        MCT_HELLO_ALPN, MctHelloAdmissionEvaluation, MctProtocolVersion, PathClass,
        SafeHelloReason,
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
                approximate_size_bytes: 5,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 1,
                grants_revision: 1,
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
                grants_revision: 1,
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
                blake3_digest_hex: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            },
            idempotency_key: Some("idem-1".into()),
            received_observation_id: ObservationId::new("obs-call-received")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn eval_ids() -> CallEvaluationIds {
        CallEvaluationIds {
            decision_id: DecisionId::new("call-decision-1")
                .expect("string ID literal/generated value must be non-empty"),
            observation_id: ObservationId::new("obs-call-decision")
                .expect("string ID literal/generated value must be non-empty"),
        }
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
            decoded_request.payload.approximate_size_bytes(),
            decoded_request.call.payload_metadata.approximate_size_bytes
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
        assert_eq!(size_mismatch.reason, PayloadIntegrityReason::PayloadSizeMismatch);

        let digest_mismatch = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &handle,
            &observed_payload(5, digest_hex('b')),
            32,
        );
        assert_eq!(digest_mismatch.reason, PayloadIntegrityReason::PayloadDigestMismatch);

        let missing = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &handle,
            &MctPayloadIntegrityObservation::missing_inline_bytes(),
            32,
        );
        assert_eq!(missing.reason, PayloadIntegrityReason::PayloadMissingInlineBytes);

        let unexpected = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &MctCallPayloadHandle::Empty,
            &observed_payload(1, digest_hex('c')),
            32,
        );
        assert_eq!(unexpected.reason, PayloadIntegrityReason::PayloadUnexpectedInlineBytes);

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
        assert_eq!(invalid_digest.reason, PayloadIntegrityReason::InvalidPayloadDigest);

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
        assert_eq!(digest_mismatch.safe_message, "result payload integrity mismatch");

        let oversized = evaluate_payload_integrity(
            PayloadIntegritySubject::ReplyResult,
            &handle,
            &observed_payload(33, digest_hex('a')),
            32,
        );
        assert_eq!(oversized.reason, PayloadIntegrityReason::ResultPayloadTooLarge);
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

        let decoded = decode_call_protocol_reply_json(
            &encode_call_protocol_reply_json(&reply).unwrap(),
        )
        .unwrap();
        assert_eq!(decoded.result_payload, reply.result_payload);
        assert_eq!(decoded.result_payload.declared_size_bytes(), 5);
    }

    #[test]
    fn denied_result_has_no_route_taken() {
        let result = MctResult {
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
        assert_eq!(result.outcome, ResultOutcome::Denied);
        assert!(result.route_taken.is_none());
    }
}

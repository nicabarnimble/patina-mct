use crate::{
    error::{MctKernelError, MctKernelResult, ensure_non_blank},
    id::*,
};
use serde::{Deserialize, Serialize};

mod internal;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `CallerIdentity` used by the MCT kernel.
pub struct CallerIdentity {
    /// Field `node_id` of this domain record.
    pub node_id: MctNodeId,
    /// Field `user_id` of this domain record.
    pub user_id: Option<UserId>,
    /// Field `vision_id` of this domain record.
    pub vision_id: VisionId,
    /// Field `project_id` of this domain record.
    pub project_id: Option<ProjectId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `OperationTarget` used by the MCT kernel.
pub struct OperationTarget {
    /// Field `namespace` of this domain record.
    pub namespace: String,
    /// Field `interface_name` of this domain record.
    pub interface_name: String,
    /// Field `function_name` of this domain record.
    pub function_name: String,
}

impl OperationTarget {
    /// Constructs this domain record from validated inputs.
    ///
    /// # Errors
    ///
    /// Returns a typed error when any supplied field is invalid.
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

    /// Validates this domain record and returns typed kernel errors.
    ///
    /// # Errors
    ///
    /// Returns a typed error when required domain fields are invalid.
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank("OperationTarget", "namespace", &self.namespace)?;
        ensure_non_blank("OperationTarget", "interface_name", &self.interface_name)?;
        ensure_non_blank("OperationTarget", "function_name", &self.function_name)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `PayloadMetadata` used by the MCT kernel.
pub struct PayloadMetadata {
    /// Field `data_classification` of this domain record.
    pub data_classification: String,
    /// Field `approximate_size_bytes` of this domain record.
    pub approximate_size_bytes: u64,
    /// Field `contains_secret_scoped_material` of this domain record.
    pub contains_secret_scoped_material: bool,
}

impl PayloadMetadata {
    /// Validates this domain record and returns typed kernel errors.
    ///
    /// # Errors
    ///
    /// Returns a typed error when required domain fields are invalid.
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank(
            "PayloadMetadata",
            "data_classification",
            &self.data_classification,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `AuthorityContextSnapshot` used by the MCT kernel.
pub struct AuthorityContextSnapshot {
    /// Field `policy_revision` of this domain record.
    pub policy_revision: u64,
    /// Field `grants_revision` of this domain record.
    pub grants_revision: u64,
    /// Field `vision_policy_revision` of this domain record.
    pub vision_policy_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `TraceContext` used by the MCT kernel.
pub struct TraceContext {
    /// Field `trace_id` of this domain record.
    pub trace_id: TraceId,
    /// Field `span_id` of this domain record.
    pub span_id: SpanId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `CallOrigin` used by the MCT kernel.
pub enum CallOrigin {
    /// Public `Iroh` item.
    Iroh,
    /// Public `JvmAdapter` item.
    JvmAdapter,
    /// Public `WasmHost` item.
    WasmHost,
    /// Public `ProcessHarness` item.
    ProcessHarness,
    /// Public `Cli` item.
    Cli,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctCall` used by the MCT kernel.
pub struct MctCall {
    /// Field `call_id` of this domain record.
    pub call_id: CallId,
    /// Field `caller` of this domain record.
    pub caller: CallerIdentity,
    /// Field `target` of this domain record.
    pub target: OperationTarget,
    /// Field `payload_metadata` of this domain record.
    pub payload_metadata: PayloadMetadata,
    /// Field `authority_context` of this domain record.
    pub authority_context: AuthorityContextSnapshot,
    /// Field `deadline` of this domain record.
    pub deadline: Timestamp,
    /// Field `trace_context` of this domain record.
    pub trace_context: TraceContext,
    /// Field `origin` of this domain record.
    pub origin: CallOrigin,
}

impl CallerIdentity {
    /// Validates this domain record and returns typed kernel errors.
    ///
    /// # Errors
    ///
    /// Returns a typed error when required domain fields are invalid.
    pub fn validate(&self) -> MctKernelResult<()> {
        Ok(())
    }
}

impl TraceContext {
    /// Validates this domain record and returns typed kernel errors.
    ///
    /// # Errors
    ///
    /// Returns a typed error when required domain fields are invalid.
    pub fn validate(&self) -> MctKernelResult<()> {
        Ok(())
    }
}

impl MctCall {
    /// Validates this domain record and returns typed kernel errors.
    ///
    /// # Errors
    ///
    /// Returns a typed error when required domain fields are invalid.
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
/// Closed domain enum `RuntimeKind` used by the MCT kernel.
pub enum RuntimeKind {
    /// Public `Process` item.
    Process,
    /// Public `JvmChild` item.
    JvmChild,
    /// Public `WasmComponent` item.
    WasmComponent,
    /// Public `RemotePeer` item.
    RemotePeer,
    /// Public `Internal` item.
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `RouteTaken` used by the MCT kernel.
pub struct RouteTaken {
    /// Field `node_id` of this domain record.
    pub node_id: MctNodeId,
    /// Field `child_id` of this domain record.
    pub child_id: Option<ChildId>,
    /// Field `runtime_kind` of this domain record.
    pub runtime_kind: RuntimeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `ExecutionSummary` used by the MCT kernel.
pub struct ExecutionSummary {
    /// Field `wall_time_ms` of this domain record.
    pub wall_time_ms: u64,
    /// Field `execution_time_ms` of this domain record.
    pub execution_time_ms: Option<u64>,
    /// Field `queue_wait_ms` of this domain record.
    pub queue_wait_ms: Option<u64>,
    /// Field `input_size_bytes` of this domain record.
    pub input_size_bytes: u64,
    /// Field `output_size_bytes` of this domain record.
    pub output_size_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `ResultOutcome` used by the MCT kernel.
pub enum ResultOutcome {
    /// Public `Success` item.
    Success,
    /// Public `Denied` item.
    Denied,
    /// Public `Failed` item.
    Failed,
    /// Public `TimedOut` item.
    TimedOut,
    /// Public `Cancelled` item.
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctResult` used by the MCT kernel.
pub struct MctResult {
    /// Field `call_id` of this domain record.
    pub call_id: CallId,
    /// Field `outcome` of this domain record.
    pub outcome: ResultOutcome,
    /// Field `route_taken` of this domain record.
    pub route_taken: Option<RouteTaken>,
    /// Field `authority_decision_ref` of this domain record.
    pub authority_decision_ref: DecisionId,
    /// Field `execution_summary` of this domain record.
    pub execution_summary: ExecutionSummary,
    /// Field `requester_message` of this domain record.
    pub requester_message: String,
    /// Field `audit_ref` of this domain record.
    pub audit_ref: AuditRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctCallProtocolAuthority` used by the MCT kernel.
pub struct MctCallProtocolAuthority {
    /// Field `hello_decision_id` of this domain record.
    pub hello_decision_id: DecisionId,
    /// Field `peer_binding_id` of this domain record.
    pub peer_binding_id: PeerBindingId,
    /// Field `vision_id` of this domain record.
    pub vision_id: VisionId,
    /// Field `accepted_alpn` of this domain record.
    pub accepted_alpn: String,
    /// Field `endpoint_id` of this domain record.
    pub endpoint_id: EndpointIdText,
    /// Field `policy_revision` of this domain record.
    pub policy_revision: u64,
    /// Field `grants_revision` of this domain record.
    pub grants_revision: u64,
}

impl MctCallProtocolAuthority {
    /// Validates this domain record and returns typed kernel errors.
    ///
    /// # Errors
    ///
    /// Returns a typed error when required domain fields are invalid.
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
/// Closed domain enum `MctCallPayloadHandle` used by the MCT kernel.
pub enum MctCallPayloadHandle {
    /// Public `InlinePayload` item.
    InlinePayload {
        /// Field `String` of this domain record.
        inline_payload_ref: String,
        /// Field `String` of this domain record.
        content_type: String,
        /// Field `u64` of this domain record.
        approximate_size_bytes: u64,
    },
    /// Public `ContentAddressedBlob` item.
    ContentAddressedBlob {
        /// Field `String` of this domain record.
        digest: String,
        /// Field `String` of this domain record.
        blob_ref: String,
        /// Field `String` of this domain record.
        content_type: String,
        /// Field `u64` of this domain record.
        approximate_size_bytes: u64,
    },
    /// Public `ExternalReference` item.
    ExternalReference {
        /// Field `String` of this domain record.
        external_ref: String,
        /// Field `item` of this domain record.
        content_type: Option<String>,
        /// Field `u64` of this domain record.
        approximate_size_bytes: u64,
    },
    /// Public `Empty` item.
    Empty,
}

impl MctCallPayloadHandle {
    /// Executes `approximate_size_bytes` for this domain type.
    pub fn approximate_size_bytes(&self) -> u64 {
        match self {
            Self::InlinePayload {
                approximate_size_bytes,
                ..
            }
            | Self::ContentAddressedBlob {
                approximate_size_bytes,
                ..
            }
            | Self::ExternalReference {
                approximate_size_bytes,
                ..
            } => *approximate_size_bytes,
            Self::Empty => 0,
        }
    }

    /// Validates this domain record and returns typed kernel errors.
    ///
    /// # Errors
    ///
    /// Returns a typed error when required domain fields are invalid.
    pub fn validate(&self) -> MctKernelResult<()> {
        match self {
            Self::InlinePayload {
                inline_payload_ref,
                content_type,
                ..
            } => {
                ensure_non_blank(
                    "MctCallPayloadHandle",
                    "inline_payload_ref",
                    inline_payload_ref,
                )?;
                ensure_non_blank("MctCallPayloadHandle", "content_type", content_type)?;
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctCallProtocolRequest` used by the MCT kernel.
pub struct MctCallProtocolRequest {
    /// Field `protocol_request_id` of this domain record.
    pub protocol_request_id: ProtocolRequestId,
    /// Field `authority` of this domain record.
    pub authority: MctCallProtocolAuthority,
    /// Field `received_over` of this domain record.
    pub received_over: crate::peer::IrohConnectionPresentation,
    /// Field `call` of this domain record.
    pub call: MctCall,
    /// Field `payload` of this domain record.
    pub payload: MctCallPayloadHandle,
    /// Field `idempotency_key` of this domain record.
    pub idempotency_key: Option<String>,
    /// Field `received_observation_id` of this domain record.
    pub received_observation_id: ObservationId,
}

impl MctCallProtocolRequest {
    /// Validates this domain record and returns typed kernel errors.
    ///
    /// # Errors
    ///
    /// Returns a typed error when required domain fields are invalid.
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
/// Closed domain enum `CallProtocolOutcome` used by the MCT kernel.
pub enum CallProtocolOutcome {
    /// Public `AcceptedForRouting` item.
    AcceptedForRouting,
    /// Public `Malformed` item.
    Malformed,
    /// Public `Denied` item.
    Denied,
    /// Public `Failed` item.
    Failed,
    /// Public `TimedOut` item.
    TimedOut,
    /// Public `Completed` item.
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `CallProtocolReason` used by the MCT kernel.
pub enum CallProtocolReason {
    /// Public `HelloNotAdmitted` item.
    HelloNotAdmitted,
    /// Public `AlpnNotAdmitted` item.
    AlpnNotAdmitted,
    /// Public `EndpointMismatch` item.
    EndpointMismatch,
    /// Public `BindingMismatch` item.
    BindingMismatch,
    /// Public `CallerMismatch` item.
    CallerMismatch,
    /// Public `VisionMismatch` item.
    VisionMismatch,
    /// Public `BindingRevoked` item.
    BindingRevoked,
    /// Public `BindingExpired` item.
    BindingExpired,
    /// Public `PolicyRevisionStale` item.
    PolicyRevisionStale,
    /// Public `MalformedCall` item.
    MalformedCall,
    /// Public `PayloadMetadataMismatch` item.
    PayloadMetadataMismatch,
    /// Public `AuthorityDenied` item.
    AuthorityDenied,
    /// Public `NoRoute` item.
    NoRoute,
    /// Public `ExecutionFailed` item.
    ExecutionFailed,
    /// Public `ExecutionTimedOut` item.
    ExecutionTimedOut,
    /// Public `ResultRecorded` item.
    ResultRecorded,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctCallProtocolEvaluation` used by the MCT kernel.
pub struct MctCallProtocolEvaluation {
    /// Field `decision_id` of this domain record.
    pub decision_id: DecisionId,
    /// Field `protocol_request_id` of this domain record.
    pub protocol_request_id: ProtocolRequestId,
    /// Field `call_id` of this domain record.
    pub call_id: Option<CallId>,
    /// Field `route_decision_id` of this domain record.
    pub route_decision_id: Option<DecisionId>,
    /// Field `result_ref` of this domain record.
    pub result_ref: Option<ResultRef>,
    /// Field `outcome` of this domain record.
    pub outcome: CallProtocolOutcome,
    /// Field `reason` of this domain record.
    pub reason: CallProtocolReason,
    /// Field `safe_message` of this domain record.
    pub safe_message: String,
    /// Field `observation_id` of this domain record.
    pub observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `CallProtocolReplyOutcome` used by the MCT kernel.
pub enum CallProtocolReplyOutcome {
    /// Public `Success` item.
    Success,
    /// Public `Denied` item.
    Denied,
    /// Public `Failed` item.
    Failed,
    /// Public `TimedOut` item.
    TimedOut,
    /// Public `Cancelled` item.
    Cancelled,
    /// Public `Malformed` item.
    Malformed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctCallProtocolReply` used by the MCT kernel.
pub struct MctCallProtocolReply {
    /// Field `reply_id` of this domain record.
    pub reply_id: ReplyId,
    /// Field `protocol_request_id` of this domain record.
    pub protocol_request_id: ProtocolRequestId,
    /// Field `decision_id` of this domain record.
    pub decision_id: DecisionId,
    /// Field `result_ref` of this domain record.
    pub result_ref: Option<ResultRef>,
    /// Field `reply_outcome` of this domain record.
    pub reply_outcome: CallProtocolReplyOutcome,
    /// Field `safe_message` of this domain record.
    pub safe_message: String,
    /// Field `reply_observation_id` of this domain record.
    pub reply_observation_id: ObservationId,
}

impl MctCallProtocolReply {
    /// Validates this domain record and returns typed kernel errors.
    ///
    /// # Errors
    ///
    /// Returns a typed error when required domain fields are invalid.
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank("MctCallProtocolReply", "safe_message", &self.safe_message)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Domain record `CallEvaluationIds` used by the MCT kernel.
pub struct CallEvaluationIds {
    /// Field `decision_id` of this domain record.
    pub decision_id: DecisionId,
    /// Field `observation_id` of this domain record.
    pub observation_id: ObservationId,
}

/// Evaluates `evaluate_call_protocol` fail-closed from explicit authority inputs.
pub fn evaluate_call_protocol(
    request: &MctCallProtocolRequest,
    hello: &crate::peer::MctHelloAdmissionEvaluation,
    ids: CallEvaluationIds,
) -> MctCallProtocolEvaluation {
    internal::evaluate_call_protocol_internal(request, hello, ids)
}

impl MctCallProtocolEvaluation {
    /// Executes `is_accepted_for_routing` for this domain type.
    pub fn is_accepted_for_routing(&self) -> bool {
        self.outcome == CallProtocolOutcome::AcceptedForRouting
    }
}

/// Executes `encode_call_protocol_request_json` for this domain type.
///
/// # Errors
///
/// Returns a typed error when JSON encoding fails.
pub fn encode_call_protocol_request_json(
    request: &MctCallProtocolRequest,
) -> MctKernelResult<Vec<u8>> {
    request.validate()?;
    serde_json::to_vec(request).map_err(|source| MctKernelError::EncodeCallProtocolJson { source })
}

/// Executes `decode_call_protocol_request_json` for this domain type.
///
/// # Errors
///
/// Returns a typed error when JSON decoding or validation fails.
pub fn decode_call_protocol_request_json(bytes: &[u8]) -> MctKernelResult<MctCallProtocolRequest> {
    let request: MctCallProtocolRequest = serde_json::from_slice(bytes)
        .map_err(|source| MctKernelError::DecodeCallProtocolJson { source })?;
    request.validate()?;
    Ok(request)
}

/// Executes `encode_call_protocol_reply_json` for this domain type.
///
/// # Errors
///
/// Returns a typed error when JSON encoding fails.
pub fn encode_call_protocol_reply_json(reply: &MctCallProtocolReply) -> MctKernelResult<Vec<u8>> {
    reply.validate()?;
    serde_json::to_vec(reply).map_err(|source| MctKernelError::EncodeCallProtocolJson { source })
}

/// Executes `decode_call_protocol_reply_json` for this domain type.
///
/// # Errors
///
/// Returns a typed error when JSON decoding or validation fails.
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

/// Executes `call_reply_from_evaluation` for this domain type.
pub fn call_reply_from_evaluation(
    reply_id: ReplyId,
    evaluation: &MctCallProtocolEvaluation,
    result_ref: Option<ResultRef>,
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
                approximate_size_bytes: 5,
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
            requester_message: "not authorized".into(),
            audit_ref: AuditRef::new("audit-1")
                .expect("string ID literal/generated value must be non-empty"),
        };
        assert_eq!(result.outcome, ResultOutcome::Denied);
        assert!(result.route_taken.is_none());
    }
}

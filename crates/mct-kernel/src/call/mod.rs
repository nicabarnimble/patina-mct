use crate::{
    error::{MctKernelError, MctKernelResult, ensure_non_blank},
    id::*,
};
use serde::{Deserialize, Serialize};

mod internal;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct PayloadMetadata {
    pub data_classification: String,
    pub approximate_size_bytes: u64,
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
pub struct AuthorityContextSnapshot {
    pub policy_revision: u64,
    pub grants_revision: u64,
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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
        ensure_non_blank("CallerIdentity", "node_id", self.node_id.as_str())?;
        if let Some(user_id) = &self.user_id {
            ensure_non_blank("CallerIdentity", "user_id", user_id.as_str())?;
        }
        ensure_non_blank("CallerIdentity", "vision_id", self.vision_id.as_str())?;
        if let Some(project_id) = &self.project_id {
            ensure_non_blank("CallerIdentity", "project_id", project_id.as_str())?;
        }
        Ok(())
    }
}

impl TraceContext {
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank("TraceContext", "trace_id", self.trace_id.as_str())?;
        ensure_non_blank("TraceContext", "span_id", self.span_id.as_str())?;
        Ok(())
    }
}

impl MctCall {
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank("MctCall", "call_id", self.call_id.as_str())?;
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
    Failed,
    TimedOut,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctResult {
    pub call_id: CallId,
    pub outcome: ResultOutcome,
    pub route_taken: Option<RouteTaken>,
    pub authority_decision_ref: DecisionId,
    pub execution_summary: ExecutionSummary,
    pub requester_message: String,
    pub audit_ref: AuditRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctCallProtocolAuthority {
    pub hello_decision_id: DecisionId,
    pub peer_binding_id: PeerBindingId,
    pub vision_id: VisionId,
    pub accepted_alpn: String,
    pub endpoint_id: EndpointIdText,
    pub policy_revision: u64,
    pub grants_revision: u64,
}

impl MctCallProtocolAuthority {
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank(
            "MctCallProtocolAuthority",
            "hello_decision_id",
            self.hello_decision_id.as_str(),
        )?;
        ensure_non_blank(
            "MctCallProtocolAuthority",
            "peer_binding_id",
            self.peer_binding_id.as_str(),
        )?;
        ensure_non_blank(
            "MctCallProtocolAuthority",
            "vision_id",
            self.vision_id.as_str(),
        )?;
        ensure_non_blank(
            "MctCallProtocolAuthority",
            "accepted_alpn",
            &self.accepted_alpn,
        )?;
        ensure_non_blank(
            "MctCallProtocolAuthority",
            "endpoint_id",
            self.endpoint_id.as_str(),
        )?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadKind {
    InlinePayload,
    ContentAddressedBlob,
    ExternalReference,
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctCallPayloadHandle {
    pub payload_kind: PayloadKind,
    pub content_type: Option<String>,
    pub approximate_size_bytes: u64,
    pub digest: Option<String>,
    pub blob_ref: Option<String>,
    pub external_ref: Option<String>,
    pub inline_payload_ref: Option<String>,
}

impl MctCallPayloadHandle {
    pub fn validate(&self) -> MctKernelResult<()> {
        validate_optional_string_field("MctCallPayloadHandle", "content_type", &self.content_type)?;
        validate_optional_string_field("MctCallPayloadHandle", "digest", &self.digest)?;
        validate_optional_string_field("MctCallPayloadHandle", "blob_ref", &self.blob_ref)?;
        validate_optional_string_field("MctCallPayloadHandle", "external_ref", &self.external_ref)?;
        validate_optional_string_field(
            "MctCallPayloadHandle",
            "inline_payload_ref",
            &self.inline_payload_ref,
        )?;

        match self.payload_kind {
            PayloadKind::InlinePayload => {
                require_payload_field(
                    "inline_payload",
                    "inline_payload_ref",
                    &self.inline_payload_ref,
                )?;
                reject_payload_field("inline_payload", "digest", &self.digest)?;
                reject_payload_field("inline_payload", "blob_ref", &self.blob_ref)?;
                reject_payload_field("inline_payload", "external_ref", &self.external_ref)?;
            }
            PayloadKind::ContentAddressedBlob => {
                require_payload_field("content_addressed_blob", "digest", &self.digest)?;
                require_payload_field("content_addressed_blob", "blob_ref", &self.blob_ref)?;
                reject_payload_field(
                    "content_addressed_blob",
                    "inline_payload_ref",
                    &self.inline_payload_ref,
                )?;
                reject_payload_field("content_addressed_blob", "external_ref", &self.external_ref)?;
            }
            PayloadKind::ExternalReference => {
                require_payload_field("external_reference", "external_ref", &self.external_ref)?;
                reject_payload_field(
                    "external_reference",
                    "inline_payload_ref",
                    &self.inline_payload_ref,
                )?;
                reject_payload_field("external_reference", "blob_ref", &self.blob_ref)?;
            }
            PayloadKind::Empty => {
                if self.approximate_size_bytes != 0 {
                    return Err(MctKernelError::EmptyPayloadHasNonZeroSize {
                        size_bytes: self.approximate_size_bytes,
                    });
                }
                reject_payload_field("empty", "content_type", &self.content_type)?;
                reject_payload_field("empty", "digest", &self.digest)?;
                reject_payload_field("empty", "blob_ref", &self.blob_ref)?;
                reject_payload_field("empty", "external_ref", &self.external_ref)?;
                reject_payload_field("empty", "inline_payload_ref", &self.inline_payload_ref)?;
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctCallProtocolRequest {
    pub protocol_request_id: ProtocolRequestId,
    pub authority: MctCallProtocolAuthority,
    pub received_over: crate::peer::IrohConnectionPresentation,
    pub call: MctCall,
    pub payload: MctCallPayloadHandle,
    pub idempotency_key: Option<String>,
    pub received_observation_id: ObservationId,
}

impl MctCallProtocolRequest {
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank(
            "MctCallProtocolRequest",
            "protocol_request_id",
            self.protocol_request_id.as_str(),
        )?;
        self.authority.validate()?;
        self.received_over.validate()?;
        self.call.validate()?;
        self.payload.validate()?;
        validate_optional_string_field(
            "MctCallProtocolRequest",
            "idempotency_key",
            &self.idempotency_key,
        )?;
        ensure_non_blank(
            "MctCallProtocolRequest",
            "received_observation_id",
            self.received_observation_id.as_str(),
        )?;

        if self.payload.approximate_size_bytes != self.call.payload_metadata.approximate_size_bytes
        {
            return Err(MctKernelError::PayloadSizeMismatch {
                call_size_bytes: self.call.payload_metadata.approximate_size_bytes,
                handle_size_bytes: self.payload.approximate_size_bytes,
            });
        }

        Ok(())
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
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallProtocolReason {
    HelloNotAdmitted,
    AlpnNotAdmitted,
    EndpointMismatch,
    BindingMismatch,
    CallerMismatch,
    VisionMismatch,
    BindingRevoked,
    BindingExpired,
    PolicyRevisionStale,
    MalformedCall,
    PayloadMetadataMismatch,
    AuthorityDenied,
    NoRoute,
    ExecutionFailed,
    ExecutionTimedOut,
    ResultRecorded,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctCallProtocolEvaluation {
    pub decision_id: DecisionId,
    pub protocol_request_id: ProtocolRequestId,
    pub call_id: Option<CallId>,
    pub route_decision_id: Option<DecisionId>,
    pub result_ref: Option<ResultRef>,
    pub outcome: CallProtocolOutcome,
    pub reason: CallProtocolReason,
    pub safe_message: String,
    pub observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallProtocolReplyOutcome {
    Success,
    Denied,
    Failed,
    TimedOut,
    Cancelled,
    Malformed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctCallProtocolReply {
    pub reply_id: ReplyId,
    pub protocol_request_id: ProtocolRequestId,
    pub decision_id: DecisionId,
    pub result_ref: Option<ResultRef>,
    pub reply_outcome: CallProtocolReplyOutcome,
    pub safe_message: String,
    pub reply_observation_id: ObservationId,
}

impl MctCallProtocolReply {
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank("MctCallProtocolReply", "reply_id", self.reply_id.as_str())?;
        ensure_non_blank(
            "MctCallProtocolReply",
            "protocol_request_id",
            self.protocol_request_id.as_str(),
        )?;
        ensure_non_blank(
            "MctCallProtocolReply",
            "decision_id",
            self.decision_id.as_str(),
        )?;
        if let Some(result_ref) = &self.result_ref {
            ensure_non_blank("MctCallProtocolReply", "result_ref", result_ref.as_str())?;
        }
        ensure_non_blank("MctCallProtocolReply", "safe_message", &self.safe_message)?;
        ensure_non_blank(
            "MctCallProtocolReply",
            "reply_observation_id",
            self.reply_observation_id.as_str(),
        )?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallEvaluationIds {
    pub decision_id: DecisionId,
    pub observation_id: ObservationId,
}

pub fn evaluate_call_protocol(
    request: &MctCallProtocolRequest,
    hello: &crate::peer::MctHelloAdmissionEvaluation,
    ids: CallEvaluationIds,
) -> MctCallProtocolEvaluation {
    internal::evaluate_call_protocol_internal(request, hello, ids)
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

fn require_payload_field(
    payload_kind: &'static str,
    field: &'static str,
    value: &Option<String>,
) -> MctKernelResult<()> {
    if value.is_some() {
        Ok(())
    } else {
        Err(MctKernelError::PayloadHandleMissingField {
            payload_kind,
            field,
        })
    }
}

fn reject_payload_field(
    payload_kind: &'static str,
    field: &'static str,
    value: &Option<String>,
) -> MctKernelResult<()> {
    if value.is_none() {
        Ok(())
    } else {
        Err(MctKernelError::PayloadHandleUnexpectedField {
            payload_kind,
            field,
        })
    }
}

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
            call_id: CallId::from("call-1"),
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
                policy_revision: 1,
                grants_revision: 1,
                vision_policy_revision: 1,
            },
            deadline: Timestamp::from("2026-05-31T00:00:00Z"),
            trace_context: TraceContext {
                trace_id: TraceId::from("trace-1"),
                span_id: SpanId::from("span-1"),
            },
            origin: CallOrigin::Iroh,
        }
    }

    fn admitted_hello() -> MctHelloAdmissionEvaluation {
        MctHelloAdmissionEvaluation {
            decision_id: DecisionId::from("hello-decision-1"),
            request_id: "hello-1".into(),
            peer_admission_decision_id: None,
            selected_binding_id: Some(PeerBindingId::from("binding-1")),
            selected_node_id: Some(MctNodeId::from("node-a")),
            selected_vision_id: Some(VisionId::from("vision-a")),
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
            observation_id: ObservationId::from("obs-hello-decision"),
        }
    }

    fn protocol_request() -> MctCallProtocolRequest {
        MctCallProtocolRequest {
            protocol_request_id: ProtocolRequestId::from("proto-request-1"),
            authority: MctCallProtocolAuthority {
                hello_decision_id: DecisionId::from("hello-decision-1"),
                peer_binding_id: PeerBindingId::from("binding-1"),
                vision_id: VisionId::from("vision-a"),
                accepted_alpn: MCT_CALL_ALPN.into(),
                endpoint_id: EndpointIdText::from("endpoint-a"),
                policy_revision: 1,
                grants_revision: 1,
            },
            received_over: IrohConnectionPresentation {
                endpoint_id: EndpointIdText::from("endpoint-a"),
                alpn: MCT_CALL_ALPN.into(),
                connection_side: ConnectionSide::Incoming,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            call: example_call(),
            payload: MctCallPayloadHandle {
                payload_kind: PayloadKind::InlinePayload,
                content_type: Some("text/plain".into()),
                approximate_size_bytes: 5,
                digest: None,
                blob_ref: None,
                external_ref: None,
                inline_payload_ref: Some("payload-1".into()),
            },
            idempotency_key: Some("idem-1".into()),
            received_observation_id: ObservationId::from("obs-call-received"),
        }
    }

    fn eval_ids() -> CallEvaluationIds {
        CallEvaluationIds {
            decision_id: DecisionId::from("call-decision-1"),
            observation_id: ObservationId::from("obs-call-decision"),
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
            ReplyId::from("reply-denied"),
            &evaluation,
            None,
            ObservationId::from("obs-reply-denied"),
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
        request.payload.payload_kind = PayloadKind::Empty;
        request.payload.inline_payload_ref = None;
        assert!(matches!(
            encode_call_protocol_request_json(&request),
            Err(MctKernelError::EmptyPayloadHasNonZeroSize { size_bytes: 5 })
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
            decoded_request.payload.approximate_size_bytes,
            decoded_request.call.payload_metadata.approximate_size_bytes
        );

        let evaluation = evaluate_call_protocol(&decoded_request, &admitted_hello(), eval_ids());
        assert_eq!(evaluation.call_id, Some(typed_call.call_id.clone()));
        assert_eq!(evaluation.outcome, CallProtocolOutcome::AcceptedForRouting);

        let reply = call_reply_from_evaluation(
            ReplyId::from("reply-success"),
            &evaluation,
            Some(ResultRef::from("result-call-1")),
            ObservationId::from("obs-reply-success"),
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
            Some(ResultRef::from("result-call-1"))
        );
    }

    #[test]
    fn admitted_hello_allows_call_for_routing() {
        let evaluation = evaluate_call_protocol(&protocol_request(), &admitted_hello(), eval_ids());
        assert!(evaluation.is_accepted_for_routing());
        assert_eq!(evaluation.call_id, Some(CallId::from("call-1")));
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
        request.received_over.endpoint_id = EndpointIdText::from("endpoint-b");
        let evaluation = evaluate_call_protocol(&request, &admitted_hello(), eval_ids());
        assert_eq!(evaluation.reason, CallProtocolReason::EndpointMismatch);
        assert_eq!(evaluation.safe_message, "not authorized");
    }

    #[test]
    fn call_authority_binding_must_match_admitted_hello() {
        let mut request = protocol_request();
        request.authority.peer_binding_id = PeerBindingId::from("binding-other");

        let evaluation = evaluate_call_protocol(&request, &admitted_hello(), eval_ids());

        assert_eq!(evaluation.outcome, CallProtocolOutcome::Denied);
        assert_eq!(evaluation.reason, CallProtocolReason::BindingMismatch);
        assert_eq!(evaluation.safe_message, "not authorized");
    }

    #[test]
    fn call_caller_must_match_admitted_hello_node() {
        let mut request = protocol_request();
        request.call.caller.node_id = MctNodeId::from("node-other");

        let evaluation = evaluate_call_protocol(&request, &admitted_hello(), eval_ids());

        assert_eq!(evaluation.outcome, CallProtocolOutcome::Denied);
        assert_eq!(evaluation.reason, CallProtocolReason::CallerMismatch);
        assert_eq!(evaluation.safe_message, "not authorized");
    }

    #[test]
    fn call_authority_vision_must_match_admitted_hello_and_call() {
        let mut request = protocol_request();
        request.authority.vision_id = VisionId::from("vision-other");

        let evaluation = evaluate_call_protocol(&request, &admitted_hello(), eval_ids());

        assert_eq!(evaluation.outcome, CallProtocolOutcome::Denied);
        assert_eq!(evaluation.reason, CallProtocolReason::VisionMismatch);
        assert_eq!(evaluation.safe_message, "not authorized");
    }

    #[test]
    fn payload_metadata_mismatch_is_malformed() {
        let mut request = protocol_request();
        request.payload.approximate_size_bytes = 99;
        let evaluation = evaluate_call_protocol(&request, &admitted_hello(), eval_ids());
        assert_eq!(evaluation.outcome, CallProtocolOutcome::Malformed);
        assert_eq!(
            evaluation.reason,
            CallProtocolReason::PayloadMetadataMismatch
        );
        let reply = call_reply_from_evaluation(
            ReplyId::from("reply-1"),
            &evaluation,
            None,
            ObservationId::from("obs-reply"),
        );
        assert_eq!(reply.reply_outcome, CallProtocolReplyOutcome::Malformed);
    }

    #[test]
    fn denied_result_has_no_route_taken() {
        let result = MctResult {
            call_id: CallId::from("call-1"),
            outcome: ResultOutcome::Denied,
            route_taken: None,
            authority_decision_ref: DecisionId::from("decision-1"),
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: 0,
                output_size_bytes: None,
            },
            requester_message: "not authorized".into(),
            audit_ref: AuditRef::from("audit-1"),
        };
        assert_eq!(result.outcome, ResultOutcome::Denied);
        assert!(result.route_taken.is_none());
    }
}

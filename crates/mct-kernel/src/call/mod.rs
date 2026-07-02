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
            "accepted_alpn",
            &self.accepted_alpn,
        )?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "payload_kind", rename_all = "snake_case")]
pub enum MctCallPayloadHandle {
    InlinePayload {
        inline_payload_ref: String,
        content_type: String,
        approximate_size_bytes: u64,
    },
    ContentAddressedBlob {
        digest: String,
        blob_ref: String,
        content_type: String,
        approximate_size_bytes: u64,
    },
    ExternalReference {
        external_ref: String,
        content_type: Option<String>,
        approximate_size_bytes: u64,
    },
    Empty,
}

impl MctCallPayloadHandle {
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
        ensure_non_blank("MctCallProtocolReply", "safe_message", &self.safe_message)?;
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

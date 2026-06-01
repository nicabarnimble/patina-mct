use crate::id::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayloadMetadata {
    pub data_classification: String,
    pub approximate_size_bytes: u64,
    pub contains_secret_scoped_material: bool,
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

#[derive(Debug, Error)]
pub enum MctCallJsonEdgeError {
    #[error("failed to encode MCT call protocol JSON edge value: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("failed to decode MCT call protocol JSON edge value: {0}")]
    Decode(#[source] serde_json::Error),
}

pub fn encode_call_protocol_request_json(
    request: &MctCallProtocolRequest,
) -> Result<Vec<u8>, MctCallJsonEdgeError> {
    serde_json::to_vec(request).map_err(MctCallJsonEdgeError::Encode)
}

pub fn decode_call_protocol_request_json(
    bytes: &[u8],
) -> Result<MctCallProtocolRequest, MctCallJsonEdgeError> {
    serde_json::from_slice(bytes).map_err(MctCallJsonEdgeError::Decode)
}

pub fn encode_call_protocol_reply_json(
    reply: &MctCallProtocolReply,
) -> Result<Vec<u8>, MctCallJsonEdgeError> {
    serde_json::to_vec(reply).map_err(MctCallJsonEdgeError::Encode)
}

pub fn decode_call_protocol_reply_json(
    bytes: &[u8],
) -> Result<MctCallProtocolReply, MctCallJsonEdgeError> {
    serde_json::from_slice(bytes).map_err(MctCallJsonEdgeError::Decode)
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
            Err(MctCallJsonEdgeError::Decode(_))
        ));
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

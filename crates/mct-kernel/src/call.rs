use crate::id::*;
use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn mct_call_roundtrips_as_json() {
        let call = example_call();
        let json = serde_json::to_string(&call).unwrap();
        assert!(json.contains("iroh"));
        let decoded: MctCall = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, call);
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

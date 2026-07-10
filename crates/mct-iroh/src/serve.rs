mod internal;

pub(crate) use internal::endpoint_addr_from_ticket;

use crate::{
    endpoint::{
        MotherIrohEndpoint, MotherIrohEndpointError, MotherIrohEndpointResult,
        MotherIrohEndpointTicket, boxed_source,
    },
    identity::{
        MctPeerBindingSignatureVerification, encode_hex, verify_peer_binding_signature_ref,
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use internal::{ROUNDTRIP_CONNECTION_TIMEOUT, SERVE_CONNECTION_TIMEOUT};
use iroh::SecretKey;
use mct_kernel::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, VecDeque},
    error::Error as StdError,
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, Semaphore, mpsc};

pub const MCT_INLINE_PAYLOAD_MAX_BYTES: usize = 32 * 1024;
pub const MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES: usize = 32 * 1024;
pub const MCT_CALL_FRAME_READ_BUDGET_BYTES: usize = 96 * 1024;

const MAX_REMEMBERED_HELLOS: usize = 1024;

/// Mutable state for serving MCT protocols over one Mother-owned endpoint.
///
/// Decision and observation IDs minted from this state include a random prefix
/// generated once in `new`, plus a state-local monotonic counter, so a daemon
/// restart does not reuse the same IDs after the counter resets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctIrohServeState {
    pub last_hello: Option<MctHelloAdmissionEvaluation>,
    hello_by_endpoint: BTreeMap<EndpointIdText, MctHelloAdmissionEvaluation>,
    hello_insertion_order: VecDeque<EndpointIdText>,
    id_prefix: String,
    next_sequence: u64,
}

impl Default for MctIrohServeState {
    fn default() -> Self {
        Self::new()
    }
}

impl MctIrohServeState {
    pub fn new() -> Self {
        Self {
            last_hello: None,
            hello_by_endpoint: BTreeMap::new(),
            hello_insertion_order: VecDeque::new(),
            id_prefix: random_id_prefix(),
            next_sequence: 0,
        }
    }

    fn next_suffix(&mut self) -> String {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        format!("{}-{sequence}", self.id_prefix)
    }

    fn next_decision_id(&mut self, kind: &str) -> DecisionId {
        DecisionId::new(format!("decision-iroh-{kind}-{}", self.next_suffix()))
            .expect("string ID literal/generated value must be non-empty")
    }

    fn next_observation_id(&mut self, kind: &str) -> ObservationId {
        ObservationId::new(format!("obs-iroh-{kind}-{}", self.next_suffix()))
            .expect("string ID literal/generated value must be non-empty")
    }

    fn forget_hello(&mut self, endpoint_id: &EndpointIdText) {
        self.hello_by_endpoint.remove(endpoint_id);
        self.hello_insertion_order
            .retain(|remembered| remembered != endpoint_id);
    }

    fn remember_hello(
        &mut self,
        endpoint_id: EndpointIdText,
        evaluation: MctHelloAdmissionEvaluation,
    ) {
        self.last_hello = Some(evaluation.clone());
        if !evaluation.is_admitted() {
            self.hello_by_endpoint.remove(&endpoint_id);
            self.hello_insertion_order
                .retain(|remembered| remembered != &endpoint_id);
            return;
        }

        if !self.hello_by_endpoint.contains_key(&endpoint_id) {
            self.hello_insertion_order.push_back(endpoint_id.clone());
        }
        self.hello_by_endpoint.insert(endpoint_id, evaluation);
        while self.hello_by_endpoint.len() > MAX_REMEMBERED_HELLOS {
            if let Some(oldest) = self.hello_insertion_order.pop_front() {
                self.hello_by_endpoint.remove(&oldest);
            } else {
                break;
            }
        }
    }

    fn hello_for_endpoint(
        &self,
        endpoint_id: &EndpointIdText,
    ) -> Option<MctHelloAdmissionEvaluation> {
        self.hello_by_endpoint.get(endpoint_id).cloned()
    }

    #[cfg(test)]
    fn remembered_hello_count(&self) -> usize {
        self.hello_by_endpoint.len()
    }
}

fn random_id_prefix() -> String {
    let random_bytes = SecretKey::generate().to_bytes();
    encode_hex(&random_bytes[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_state_ids_do_not_collide_across_instances() {
        let mut first = MctIrohServeState::new();
        let mut second = MctIrohServeState::new();

        assert_ne!(
            first.next_decision_id("hello"),
            second.next_decision_id("hello")
        );
        assert_ne!(
            first.next_observation_id("hello"),
            second.next_observation_id("hello")
        );
    }

    #[test]
    fn denied_hellos_leave_no_per_peer_state() {
        let endpoint_id = endpoint_id("endpoint-denied");
        let mut state = MctIrohServeState::new();

        state.remember_hello(endpoint_id.clone(), hello_evaluation(HelloOutcome::Denied));

        assert_eq!(state.remembered_hello_count(), 0);
        assert_eq!(state.hello_for_endpoint(&endpoint_id), None);
    }

    #[test]
    fn admitted_hello_state_is_capped_oldest_first() {
        let mut state = MctIrohServeState::new();
        for index in 0..=MAX_REMEMBERED_HELLOS {
            state.remember_hello(
                endpoint_id(format!("endpoint-{index}")),
                hello_evaluation(HelloOutcome::Admitted),
            );
        }

        assert_eq!(state.remembered_hello_count(), MAX_REMEMBERED_HELLOS);
        assert_eq!(state.hello_for_endpoint(&endpoint_id("endpoint-0")), None);
        assert!(
            state
                .hello_for_endpoint(&endpoint_id(format!("endpoint-{MAX_REMEMBERED_HELLOS}")))
                .is_some()
        );
    }

    fn endpoint_id(value: impl Into<String>) -> EndpointIdText {
        EndpointIdText::new(value.into())
            .expect("string ID literal/generated value must be non-empty")
    }

    fn hello_evaluation(outcome: HelloOutcome) -> MctHelloAdmissionEvaluation {
        let admitted = outcome == HelloOutcome::Admitted;
        MctHelloAdmissionEvaluation {
            decision_id: DecisionId::new("decision-test-hello")
                .expect("string ID literal/generated value must be non-empty"),
            request_id: "hello-test".into(),
            peer_admission_decision_id: None,
            selected_binding_id: admitted.then(|| {
                PeerBindingId::new("binding-test")
                    .expect("string ID literal/generated value must be non-empty")
            }),
            selected_node_id: admitted.then(|| {
                MctNodeId::new("node-test")
                    .expect("string ID literal/generated value must be non-empty")
            }),
            selected_vision_id: admitted.then(|| {
                VisionId::new("vision-test")
                    .expect("string ID literal/generated value must be non-empty")
            }),
            selected_policy_revision: admitted.then_some(1),
            negotiated_protocol: admitted.then_some(HelloPolicy::default().protocol),
            accepted_alpns: if admitted {
                vec![MCT_CALL_ALPN.into()]
            } else {
                Vec::new()
            },
            hello_outcome: outcome,
            reason: if admitted {
                HelloReason::ActiveBinding
            } else {
                HelloReason::MissingBinding
            },
            safe_reason: if admitted {
                SafeHelloReason::Admitted
            } else {
                SafeHelloReason::NotAuthorized
            },
            observation_id: ObservationId::new("obs-test-hello")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MctIrohServedProtocol {
    Hello {
        request: MctHelloRequest,
        evaluation: MctHelloAdmissionEvaluation,
        response: MctHelloResponse,
    },
    Call {
        request: MctCallProtocolRequest,
        evaluation: MctCallProtocolEvaluation,
        reply: MctCallProtocolReply,
    },
    MalformedCall {
        trace_id: TraceId,
        evaluation: MctCallProtocolEvaluation,
        reply: MctCallProtocolReply,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctIrohPeerCallReport {
    pub hello_response: MctHelloResponse,
    pub call_reply: MctCallProtocolReply,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctIrohCallPayloadReply {
    pub reply: MctCallProtocolReply,
    pub inline_result_payload: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctIrohCallHandlerResult {
    pub result_ref: Option<ResultRef>,
    pub result_payload: MctCallPayloadHandle,
    pub inline_result_payload: Option<Vec<u8>>,
    pub route_decision_id: Option<DecisionId>,
    pub route_taken: Option<RouteTaken>,
    pub outcome: CallProtocolOutcome,
    pub safe_message: String,
}

#[derive(Serialize, Deserialize)]
struct MctCallProtocolRequestEnvelope {
    #[serde(flatten)]
    request: MctCallProtocolRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    inline_payload_base64: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct MctCallProtocolReplyEnvelope {
    #[serde(flatten)]
    reply: MctCallProtocolReply,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    inline_result_payload_base64: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MctIrohObservationDurability {
    BeforeEffect,
    Buffered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MctIrohCallLifecycleStage {
    Received,
    Malformed,
    Constructed,
    Authorized,
    Denied,
    ResultRecorded,
    ReplyEmitted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctIrohCallLifecycleFact {
    pub stage: MctIrohCallLifecycleStage,
    pub trace_id: TraceId,
    pub call_id: Option<CallId>,
    pub protocol_request_id: ProtocolRequestId,
    pub decision_id: Option<DecisionId>,
    pub observation_id: ObservationId,
    pub policy_revision: Option<u64>,
    pub grants_revision: Option<u64>,
    pub outcome: ObservationOutcome,
    pub safe_message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MctIrohObservationFact {
    HelloEvaluation {
        trace_id: TraceId,
        evaluation: MctHelloAdmissionEvaluation,
    },
    CallLifecycle(MctIrohCallLifecycleFact),
}

impl MctIrohObservationFact {
    pub fn call_stage(&self) -> Option<MctIrohCallLifecycleStage> {
        match self {
            Self::HelloEvaluation { .. } => None,
            Self::CallLifecycle(fact) => Some(fact.stage),
        }
    }

    pub fn to_observation(&self, observed_at: Timestamp) -> MctObservation {
        match self {
            Self::HelloEvaluation {
                trace_id,
                evaluation,
            } => hello_evaluation_observation(trace_id.clone(), observed_at, evaluation),
            Self::CallLifecycle(fact) => {
                let (kind, source_plane) = match fact.stage {
                    MctIrohCallLifecycleStage::Received => {
                        (ObservationKind::PeerCallReceived, SourcePlane::Adapter)
                    }
                    MctIrohCallLifecycleStage::Malformed => {
                        (ObservationKind::PeerCallMalformed, SourcePlane::Adapter)
                    }
                    MctIrohCallLifecycleStage::Constructed => {
                        (ObservationKind::CallConstructed, SourcePlane::Adapter)
                    }
                    MctIrohCallLifecycleStage::Authorized => {
                        (ObservationKind::CallAuthorized, SourcePlane::Kernel)
                    }
                    MctIrohCallLifecycleStage::Denied => {
                        (ObservationKind::CallDenied, SourcePlane::Kernel)
                    }
                    MctIrohCallLifecycleStage::ResultRecorded => {
                        (ObservationKind::ResultRecorded, SourcePlane::Kernel)
                    }
                    MctIrohCallLifecycleStage::ReplyEmitted => {
                        (ObservationKind::PeerCallReplied, SourcePlane::Peer)
                    }
                };
                MctObservation {
                    observation_id: fact.observation_id.clone(),
                    observed_at,
                    kind,
                    source_plane,
                    trace: ObservationTraceRef {
                        trace_id: fact.trace_id.clone(),
                        span_id: None,
                        parent_span_id: None,
                        external_trace_id: None,
                    },
                    call_id: fact.call_id.clone(),
                    decision_id: fact.decision_id.clone(),
                    subject_id: None,
                    resource_id: Some(fact.protocol_request_id.to_string()),
                    policy_revision: fact.policy_revision,
                    grants_revision: fact.grants_revision,
                    outcome: fact.outcome,
                    visibility: ObservationVisibility::InternalOnly,
                    safe_message: fact.safe_message.clone(),
                    detail_ref: None,
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctIrohObservationBatch {
    pub durability: MctIrohObservationDurability,
    pub facts: Vec<MctIrohObservationFact>,
}

type ObservationFuture = Pin<
    Box<
        dyn Future<Output = Result<(), Box<dyn StdError + Send + Sync + 'static>>> + Send + 'static,
    >,
>;

type ObservationCallback =
    dyn Fn(MctIrohObservationBatch) -> ObservationFuture + Send + Sync + 'static;

/// Awaited adapter callback for making serving lifecycle facts durable.
#[derive(Clone)]
pub struct MctIrohObservationSink {
    callback: Arc<ObservationCallback>,
}

impl MctIrohObservationSink {
    pub fn new<F, Fut, E>(callback: F) -> Self
    where
        F: Fn(MctIrohObservationBatch) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), E>> + Send + 'static,
        E: StdError + Send + Sync + 'static,
    {
        Self {
            callback: Arc::new(move |batch| {
                let future = callback(batch);
                Box::pin(async move {
                    future.await.map_err(|source| {
                        Box::new(source) as Box<dyn StdError + Send + Sync + 'static>
                    })
                })
            }),
        }
    }

    async fn record(
        &self,
        batch: MctIrohObservationBatch,
    ) -> Result<(), Box<dyn StdError + Send + Sync + 'static>> {
        (self.callback)(batch).await
    }
}

impl fmt::Debug for MctIrohObservationSink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MctIrohObservationSink")
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub struct MctIrohConcurrentServeConfig {
    pub max_concurrent_connections: usize,
    pub connection_timeout: Duration,
    pub events: Option<mpsc::Sender<MctIrohServeEvent>>,
    pub require_binding_signature: bool,
    pub capability_view: Option<MctHelloCapabilityView>,
    pub observation_sink: MctIrohObservationSink,
}

impl MctIrohConcurrentServeConfig {
    pub fn new(observation_sink: MctIrohObservationSink) -> Self {
        Self {
            max_concurrent_connections: 64,
            connection_timeout: SERVE_CONNECTION_TIMEOUT,
            events: None,
            require_binding_signature: false,
            capability_view: None,
            observation_sink,
        }
    }
}

fn call_lifecycle_fact(
    stage: MctIrohCallLifecycleStage,
    request: &MctCallProtocolRequest,
    decision_id: Option<DecisionId>,
    observation_id: ObservationId,
    outcome: ObservationOutcome,
    safe_message: impl Into<String>,
) -> MctIrohObservationFact {
    MctIrohObservationFact::CallLifecycle(MctIrohCallLifecycleFact {
        stage,
        trace_id: request.call.trace_context.trace_id.clone(),
        call_id: Some(request.call.call_id.clone()),
        protocol_request_id: request.protocol_request_id.clone(),
        decision_id,
        observation_id,
        policy_revision: Some(request.call.authority_context.policy_revision),
        grants_revision: Some(request.call.authority_context.grants_revision),
        outcome,
        safe_message: safe_message.into(),
    })
}

fn malformed_call_lifecycle_fact(
    stage: MctIrohCallLifecycleStage,
    trace_id: TraceId,
    evaluation: &MctCallProtocolEvaluation,
    observation_id: ObservationId,
) -> MctIrohObservationFact {
    MctIrohObservationFact::CallLifecycle(MctIrohCallLifecycleFact {
        stage,
        trace_id,
        call_id: None,
        protocol_request_id: evaluation.protocol_request_id.clone(),
        decision_id: Some(evaluation.decision_id.clone()),
        observation_id,
        policy_revision: None,
        grants_revision: None,
        outcome: ObservationOutcome::Denied,
        safe_message: if stage == MctIrohCallLifecycleStage::Received {
            "peer call received".into()
        } else {
            "malformed request".into()
        },
    })
}

fn reply_observation_outcome(outcome: CallProtocolReplyOutcome) -> ObservationOutcome {
    match outcome {
        CallProtocolReplyOutcome::Success => ObservationOutcome::Completed,
        CallProtocolReplyOutcome::Denied | CallProtocolReplyOutcome::Malformed => {
            ObservationOutcome::Denied
        }
        CallProtocolReplyOutcome::Failed => ObservationOutcome::Failed,
        CallProtocolReplyOutcome::TimedOut => ObservationOutcome::TimedOut,
        CallProtocolReplyOutcome::Cancelled => ObservationOutcome::Cancelled,
    }
}

fn malformed_request_evaluation(
    request: &MctCallProtocolRequest,
    state: &mut MctIrohServeState,
) -> MctCallProtocolEvaluation {
    MctCallProtocolEvaluation {
        decision_id: state.next_decision_id("call-malformed"),
        protocol_request_id: request.protocol_request_id.clone(),
        call_id: Some(request.call.call_id.clone()),
        route_decision_id: None,
        result_ref: None,
        outcome: CallProtocolOutcome::Malformed,
        reason: CallProtocolReason::MalformedCall,
        safe_message: "malformed request".into(),
        observation_id: state.next_observation_id("call-malformed"),
    }
}

fn malformed_call_evaluation_and_reply(
    state: &mut MctIrohServeState,
) -> (TraceId, MctCallProtocolEvaluation, MctCallProtocolReply) {
    let suffix = state.next_suffix();
    let protocol_request_id = ProtocolRequestId::new(format!("malformed-call-{suffix}"))
        .expect("generated protocol request ID must be non-empty");
    let evaluation = MctCallProtocolEvaluation {
        decision_id: state.next_decision_id("call-malformed"),
        protocol_request_id,
        call_id: None,
        route_decision_id: None,
        result_ref: None,
        outcome: CallProtocolOutcome::Malformed,
        reason: CallProtocolReason::MalformedCall,
        safe_message: "malformed request".into(),
        observation_id: state.next_observation_id("call-malformed"),
    };
    let reply = call_reply_from_evaluation(
        ReplyId::new(format!("reply-iroh-call-malformed-{suffix}"))
            .expect("generated reply ID must be non-empty"),
        &evaluation,
        None,
        state.next_observation_id("call-reply"),
    );
    let trace_id = TraceId::new(format!("trace-iroh-call-malformed-{suffix}"))
        .expect("generated trace ID must be non-empty");
    (trace_id, evaluation, reply)
}

fn call_received_fact(request: &MctCallProtocolRequest) -> MctIrohObservationFact {
    call_lifecycle_fact(
        MctIrohCallLifecycleStage::Received,
        request,
        None,
        request.received_observation_id.clone(),
        ObservationOutcome::Started,
        "peer call received",
    )
}

fn call_malformed_fact(
    request: &MctCallProtocolRequest,
    evaluation: &MctCallProtocolEvaluation,
) -> MctIrohObservationFact {
    call_lifecycle_fact(
        MctIrohCallLifecycleStage::Malformed,
        request,
        Some(evaluation.decision_id.clone()),
        evaluation.observation_id.clone(),
        ObservationOutcome::Denied,
        "malformed request",
    )
}

fn call_constructed_fact(
    request: &MctCallProtocolRequest,
    observation_id: ObservationId,
) -> MctIrohObservationFact {
    call_lifecycle_fact(
        MctIrohCallLifecycleStage::Constructed,
        request,
        None,
        observation_id,
        ObservationOutcome::Informational,
        "call constructed",
    )
}

fn call_authority_fact(
    request: &MctCallProtocolRequest,
    evaluation: &MctCallProtocolEvaluation,
) -> MctIrohObservationFact {
    let (stage, outcome) = if evaluation.is_accepted_for_routing() {
        (
            MctIrohCallLifecycleStage::Authorized,
            ObservationOutcome::Allowed,
        )
    } else {
        (
            MctIrohCallLifecycleStage::Denied,
            ObservationOutcome::Denied,
        )
    };
    call_lifecycle_fact(
        stage,
        request,
        Some(evaluation.decision_id.clone()),
        evaluation.observation_id.clone(),
        outcome,
        evaluation.safe_message.clone(),
    )
}

fn call_result_fact(
    request: &MctCallProtocolRequest,
    evaluation: &MctCallProtocolEvaluation,
    observation_id: ObservationId,
) -> MctIrohObservationFact {
    let outcome = match evaluation.outcome {
        CallProtocolOutcome::AcceptedForRouting | CallProtocolOutcome::Completed => {
            ObservationOutcome::Completed
        }
        CallProtocolOutcome::Malformed | CallProtocolOutcome::Denied => ObservationOutcome::Denied,
        CallProtocolOutcome::Failed => ObservationOutcome::Failed,
        CallProtocolOutcome::TimedOut => ObservationOutcome::TimedOut,
    };
    call_lifecycle_fact(
        MctIrohCallLifecycleStage::ResultRecorded,
        request,
        Some(evaluation.decision_id.clone()),
        observation_id,
        outcome,
        evaluation.safe_message.clone(),
    )
}

fn call_reply_emitted_fact(served: &MctIrohServedProtocol) -> Option<MctIrohObservationFact> {
    match served {
        MctIrohServedProtocol::Hello { .. } => None,
        MctIrohServedProtocol::Call {
            request,
            evaluation,
            reply,
        } => Some(call_lifecycle_fact(
            MctIrohCallLifecycleStage::ReplyEmitted,
            request,
            Some(evaluation.decision_id.clone()),
            reply.reply_observation_id.clone(),
            reply_observation_outcome(reply.reply_outcome),
            reply.safe_message.clone(),
        )),
        MctIrohServedProtocol::MalformedCall {
            trace_id,
            evaluation,
            reply,
        } => Some(malformed_call_lifecycle_fact(
            MctIrohCallLifecycleStage::ReplyEmitted,
            trace_id.clone(),
            evaluation,
            reply.reply_observation_id.clone(),
        )),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MctIrohServeEvent {
    AcceptedConnection,
    Served(Box<MctIrohServedProtocol>),
    RefusedConnection,
}

impl MctIrohCallHandlerResult {
    pub fn accepted_for_routing(result_ref: Option<ResultRef>) -> Self {
        Self {
            result_ref,
            result_payload: MctCallPayloadHandle::Empty,
            inline_result_payload: None,
            route_decision_id: None,
            route_taken: None,
            outcome: CallProtocolOutcome::AcceptedForRouting,
            safe_message: "accepted for routing".into(),
        }
    }

    pub fn completed(result_ref: ResultRef) -> Self {
        Self {
            result_ref: Some(result_ref),
            result_payload: MctCallPayloadHandle::Empty,
            inline_result_payload: None,
            route_decision_id: None,
            route_taken: None,
            outcome: CallProtocolOutcome::Completed,
            safe_message: "call completed".into(),
        }
    }

    pub fn completed_with_inline_payload(
        result_ref: ResultRef,
        result_payload: MctCallPayloadHandle,
        inline_result_payload: Vec<u8>,
    ) -> Self {
        Self {
            result_ref: Some(result_ref),
            result_payload,
            inline_result_payload: Some(inline_result_payload),
            route_decision_id: None,
            route_taken: None,
            outcome: CallProtocolOutcome::Completed,
            safe_message: "call completed".into(),
        }
    }

    pub fn failed(safe_message: impl Into<String>) -> Self {
        Self {
            result_ref: None,
            result_payload: MctCallPayloadHandle::Empty,
            inline_result_payload: None,
            route_decision_id: None,
            route_taken: None,
            outcome: CallProtocolOutcome::Failed,
            safe_message: safe_message.into(),
        }
    }

    pub fn denied() -> Self {
        Self {
            result_ref: None,
            result_payload: MctCallPayloadHandle::Empty,
            inline_result_payload: None,
            route_decision_id: None,
            route_taken: None,
            outcome: CallProtocolOutcome::Denied,
            safe_message: "not authorized".into(),
        }
    }

    pub fn timed_out() -> Self {
        Self {
            result_ref: None,
            result_payload: MctCallPayloadHandle::Empty,
            inline_result_payload: None,
            route_decision_id: None,
            route_taken: None,
            outcome: CallProtocolOutcome::TimedOut,
            safe_message: "call timed out".into(),
        }
    }

    pub fn with_route(
        mut self,
        route_decision_id: Option<DecisionId>,
        route_taken: Option<RouteTaken>,
    ) -> Self {
        self.route_decision_id = route_decision_id;
        self.route_taken = route_taken;
        self
    }
}

fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn observed_inline_payload(bytes: Option<&[u8]>) -> MctPayloadIntegrityObservation {
    match bytes {
        Some(bytes) => MctPayloadIntegrityObservation {
            inline_bytes_present: true,
            content_addressed_blob_fetch_attempted: false,
            observed_size_bytes: Some(bytes.len() as u64),
            observed_blake3_digest_hex: Some(blake3_hex(bytes)),
        },
        None => MctPayloadIntegrityObservation::missing_inline_bytes(),
    }
}

fn verify_inline_payload_for_request(
    request: &MctCallProtocolRequest,
    inline_payload: Option<&[u8]>,
) -> MotherIrohEndpointResult<()> {
    let decision = evaluate_payload_integrity(
        PayloadIntegritySubject::Request,
        &request.payload,
        &observed_inline_payload(inline_payload),
        MCT_INLINE_PAYLOAD_MAX_BYTES as u64,
    );
    if decision.outcome == PayloadIntegrityOutcome::Matched {
        Ok(())
    } else {
        Err(MotherIrohEndpointError::ProtocolPayload {
            action: "verify outbound mct/call/0 request payload",
            reason: decision.reason,
            safe_message: decision.safe_message,
        })
    }
}

fn verify_inline_result_payload(
    reply: &MctCallProtocolReply,
    inline_payload: Option<&[u8]>,
) -> MotherIrohEndpointResult<()> {
    let decision = evaluate_payload_integrity(
        PayloadIntegritySubject::ReplyResult,
        &reply.result_payload,
        &observed_inline_payload(inline_payload),
        MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES as u64,
    );
    if decision.outcome == PayloadIntegrityOutcome::Matched {
        Ok(())
    } else {
        Err(MotherIrohEndpointError::ProtocolPayload {
            action: "verify inbound mct/call/0 result payload",
            reason: decision.reason,
            safe_message: decision.safe_message,
        })
    }
}

fn enforce_hello_binding_signature(
    request: &MctHelloRequest,
    bindings: &[MctPeerBinding],
    issuer_endpoint_id: &EndpointIdText,
    evaluation: MctHelloAdmissionEvaluation,
) -> MctHelloAdmissionEvaluation {
    if !evaluation.is_admitted() {
        return evaluation;
    }
    let Some(selected_binding_id) = evaluation.selected_binding_id.as_ref() else {
        return deny_hello_for_invalid_signature(request, evaluation);
    };
    let Some(binding) = bindings
        .iter()
        .find(|binding| &binding.binding_id == selected_binding_id)
    else {
        return deny_hello_for_invalid_signature(request, evaluation);
    };
    match verify_peer_binding_signature_ref(
        request.presented_binding.signature_ref.as_deref(),
        binding,
        issuer_endpoint_id,
    ) {
        MctPeerBindingSignatureVerification::Valid => evaluation,
        MctPeerBindingSignatureVerification::Missing
        | MctPeerBindingSignatureVerification::Malformed
        | MctPeerBindingSignatureVerification::Invalid => {
            deny_hello_for_invalid_signature(request, evaluation)
        }
    }
}

fn deny_hello_for_invalid_signature(
    request: &MctHelloRequest,
    evaluation: MctHelloAdmissionEvaluation,
) -> MctHelloAdmissionEvaluation {
    MctHelloAdmissionEvaluation {
        decision_id: evaluation.decision_id,
        request_id: request.hello_id.clone(),
        peer_admission_decision_id: evaluation.peer_admission_decision_id,
        selected_binding_id: None,
        selected_node_id: None,
        selected_vision_id: None,
        selected_policy_revision: None,
        negotiated_protocol: None,
        accepted_alpns: Vec::new(),
        hello_outcome: HelloOutcome::Denied,
        reason: HelloReason::CapabilityInvalid,
        safe_reason: SafeHelloReason::NotAuthorized,
        observation_id: evaluation.observation_id,
    }
}

fn encode_call_request_envelope(
    request: &MctCallProtocolRequest,
    inline_payload: Option<&[u8]>,
) -> MotherIrohEndpointResult<Vec<u8>> {
    verify_inline_payload_for_request(request, inline_payload)?;
    encode_call_request_envelope_unchecked(request, inline_payload)
}

fn encode_call_request_envelope_unchecked(
    request: &MctCallProtocolRequest,
    inline_payload: Option<&[u8]>,
) -> MotherIrohEndpointResult<Vec<u8>> {
    let envelope = MctCallProtocolRequestEnvelope {
        request: request.clone(),
        inline_payload_base64: inline_payload.map(|bytes| BASE64_STANDARD.encode(bytes)),
    };
    serde_json::to_vec(&envelope).map_err(|source| MotherIrohEndpointError::ProtocolJson {
        action: "encode mct/call/0 request",
        source,
    })
}

fn decode_call_request_envelope(
    bytes: &[u8],
) -> MotherIrohEndpointResult<(MctCallProtocolRequest, Option<Vec<u8>>)> {
    let envelope: MctCallProtocolRequestEnvelope =
        serde_json::from_slice(bytes).map_err(|source| MotherIrohEndpointError::ProtocolJson {
            action: "decode mct/call/0 request",
            source,
        })?;
    let inline_payload = match envelope.inline_payload_base64 {
        Some(encoded) => Some(BASE64_STANDARD.decode(encoded).map_err(|_| {
            MotherIrohEndpointError::ProtocolPayload {
                action: "decode mct/call/0 request inline payload",
                reason: PayloadIntegrityReason::PayloadDigestMismatch,
                safe_message: "malformed call payload".into(),
            }
        })?),
        None => None,
    };
    Ok((envelope.request, inline_payload))
}

fn encode_call_reply_envelope(
    reply: &MctCallProtocolReply,
    inline_result_payload: Option<&[u8]>,
) -> MotherIrohEndpointResult<Vec<u8>> {
    verify_inline_result_payload(reply, inline_result_payload)?;
    let envelope = MctCallProtocolReplyEnvelope {
        reply: reply.clone(),
        inline_result_payload_base64: inline_result_payload
            .map(|bytes| BASE64_STANDARD.encode(bytes)),
    };
    serde_json::to_vec(&envelope).map_err(|source| MotherIrohEndpointError::ProtocolJson {
        action: "encode mct/call/0 response",
        source,
    })
}

pub(crate) fn decode_call_reply_envelope(
    bytes: &[u8],
) -> MotherIrohEndpointResult<MctIrohCallPayloadReply> {
    let envelope: MctCallProtocolReplyEnvelope =
        serde_json::from_slice(bytes).map_err(|source| MotherIrohEndpointError::ProtocolJson {
            action: "decode mct/call/0 response",
            source,
        })?;
    let inline_result_payload = match envelope.inline_result_payload_base64 {
        Some(encoded) => Some(BASE64_STANDARD.decode(encoded).map_err(|_| {
            MotherIrohEndpointError::ProtocolPayload {
                action: "decode mct/call/0 result payload",
                reason: PayloadIntegrityReason::ResultPayloadIntegrityMismatch,
                safe_message: "result payload integrity mismatch".into(),
            }
        })?),
        None => None,
    };
    envelope
        .reply
        .validate()
        .map_err(|source| MotherIrohEndpointError::ProtocolKernel {
            action: "validate inbound mct/call/0 reply",
            source,
        })?;
    verify_inline_result_payload(&envelope.reply, inline_result_payload.as_deref())?;
    Ok(MctIrohCallPayloadReply {
        reply: envelope.reply,
        inline_result_payload,
    })
}

fn payload_malformed_evaluation(
    request: &MctCallProtocolRequest,
    state: &mut MctIrohServeState,
    reason: CallProtocolReason,
    safe_message: impl Into<String>,
) -> MctCallProtocolEvaluation {
    MctCallProtocolEvaluation {
        decision_id: state.next_decision_id("call-payload"),
        protocol_request_id: request.protocol_request_id.clone(),
        call_id: Some(request.call.call_id.clone()),
        route_decision_id: None,
        result_ref: None,
        outcome: CallProtocolOutcome::Malformed,
        reason,
        safe_message: safe_message.into(),
        observation_id: state.next_observation_id("call-payload"),
    }
}

impl MotherIrohEndpoint {
    pub async fn send_hello(
        &self,
        peer: &MotherIrohEndpointTicket,
        request: &MctHelloRequest,
    ) -> MotherIrohEndpointResult<MctHelloResponse> {
        self.roundtrip_json(peer, MCT_HELLO_ALPN, request).await
    }

    pub async fn send_call(
        &self,
        peer: &MotherIrohEndpointTicket,
        request: &MctCallProtocolRequest,
    ) -> MotherIrohEndpointResult<MctCallProtocolReply> {
        Ok(self
            .send_call_with_optional_inline_payload(peer, request, None)
            .await?
            .reply)
    }

    pub async fn send_call_with_inline_payload(
        &self,
        peer: &MotherIrohEndpointTicket,
        request: &MctCallProtocolRequest,
        inline_payload: Vec<u8>,
    ) -> MotherIrohEndpointResult<MctIrohCallPayloadReply> {
        self.send_call_with_optional_inline_payload(peer, request, Some(inline_payload))
            .await
    }

    async fn send_call_with_optional_inline_payload(
        &self,
        peer: &MotherIrohEndpointTicket,
        request: &MctCallProtocolRequest,
        inline_payload: Option<Vec<u8>>,
    ) -> MotherIrohEndpointResult<MctIrohCallPayloadReply> {
        request
            .validate()
            .map_err(|source| MotherIrohEndpointError::ProtocolKernel {
                action: "validate outbound mct/call/0 request",
                source,
            })?;
        verify_inline_payload_for_request(request, inline_payload.as_deref())?;
        self.roundtrip_call_payload(peer, request, inline_payload.as_deref(), true)
            .await
    }

    #[cfg(test)]
    pub(crate) async fn send_call_with_unchecked_inline_payload(
        &self,
        peer: &MotherIrohEndpointTicket,
        request: &MctCallProtocolRequest,
        inline_payload: Vec<u8>,
    ) -> MotherIrohEndpointResult<MctIrohCallPayloadReply> {
        request
            .validate()
            .map_err(|source| MotherIrohEndpointError::ProtocolKernel {
                action: "validate outbound unchecked mct/call/0 request",
                source,
            })?;
        self.roundtrip_call_payload(peer, request, Some(&inline_payload), false)
            .await
    }

    pub async fn send_hello_then_call(
        &self,
        peer: &MotherIrohEndpointTicket,
        hello: &MctHelloRequest,
        call: &MctCallProtocolRequest,
    ) -> MotherIrohEndpointResult<MctIrohPeerCallReport> {
        let hello_response = self.send_hello(peer, hello).await?;
        let call_reply = self.send_call(peer, call).await?;
        Ok(MctIrohPeerCallReport {
            hello_response,
            call_reply,
        })
    }

    pub async fn serve_concurrent_with_call_handler<H, Fut, N>(
        &self,
        state: MctIrohServeState,
        bindings: Vec<MctPeerBinding>,
        config: MctIrohConcurrentServeConfig,
        now: N,
        call_handler: H,
    ) -> MotherIrohEndpointResult<()>
    where
        H: Fn(MctCallProtocolRequest, MctCallProtocolEvaluation, Option<Vec<u8>>) -> Fut
            + Clone
            + Send
            + Sync
            + 'static,
        Fut: Future<Output = MctIrohCallHandlerResult> + Send + 'static,
        N: Fn() -> Timestamp + Clone + Send + Sync + 'static,
    {
        let bindings = Arc::new(bindings);
        self.serve_concurrent_with_binding_provider(
            state,
            config,
            now,
            move || {
                let bindings = Arc::clone(&bindings);
                async move {
                    Ok(MctPeerAuthoritySnapshot {
                        bindings: (*bindings).clone(),
                        policy_revision: HelloPolicy::default().current_policy_revision,
                    })
                }
            },
            call_handler,
        )
        .await
    }

    pub async fn serve_concurrent_with_binding_provider<H, Fut, N, B, BindingsFut>(
        &self,
        state: MctIrohServeState,
        config: MctIrohConcurrentServeConfig,
        now: N,
        bindings_provider: B,
        call_handler: H,
    ) -> MotherIrohEndpointResult<()>
    where
        H: Fn(MctCallProtocolRequest, MctCallProtocolEvaluation, Option<Vec<u8>>) -> Fut
            + Clone
            + Send
            + Sync
            + 'static,
        Fut: Future<Output = MctIrohCallHandlerResult> + Send + 'static,
        N: Fn() -> Timestamp + Clone + Send + Sync + 'static,
        B: Fn() -> BindingsFut + Clone + Send + Sync + 'static,
        BindingsFut:
            Future<Output = MotherIrohEndpointResult<MctPeerAuthoritySnapshot>> + Send + 'static,
    {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(MotherIrohEndpointError::EndpointClosed)?
            .clone();
        let issuer_endpoint_id = self.snapshot().endpoint_id;
        let require_binding_signature = config.require_binding_signature;
        let capability_view = config.capability_view.clone();
        let observation_sink = config.observation_sink.clone();
        let state = Arc::new(Mutex::new(state));
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_connections));
        let active_tasks = Arc::new(AtomicU64::new(0));
        let (task_error_tx, mut task_error_rx) = mpsc::unbounded_channel();

        loop {
            let incoming = tokio::select! {
                error = task_error_rx.recv() => {
                    return Err(error.expect("serving task error channel remains open"));
                }
                incoming = endpoint.accept() => incoming,
            };
            let Some(incoming) = incoming else {
                break;
            };
            let Ok(permit) = semaphore.clone().try_acquire_owned() else {
                if let Some(events) = &config.events {
                    let _ = events.send(MctIrohServeEvent::RefusedConnection).await;
                }
                drop(incoming);
                continue;
            };

            if let Some(events) = &config.events {
                let _ = events.send(MctIrohServeEvent::AcceptedConnection).await;
            }

            let state = Arc::clone(&state);
            let bindings_provider = bindings_provider.clone();
            let call_handler = call_handler.clone();
            let now = now.clone();
            let events = config.events.clone();
            let connection_timeout = config.connection_timeout;
            let issuer_endpoint_id = issuer_endpoint_id.clone();
            let active_tasks = Arc::clone(&active_tasks);
            let capability_view = capability_view.clone();
            let observation_sink = observation_sink.clone();
            let task_error_tx = task_error_tx.clone();
            active_tasks.fetch_add(1, Ordering::SeqCst);

            tokio::spawn(async move {
                let served = match tokio::time::timeout(connection_timeout, async {
                    let mut accepting = incoming.accept().map_err(|source| {
                        MotherIrohEndpointError::ProtocolIo {
                            action: "accept incoming connection",
                            source: boxed_source(source),
                        }
                    })?;
                    let alpn = accepting.alpn().await.map_err(|source| {
                        MotherIrohEndpointError::ProtocolIo {
                            action: "read incoming ALPN",
                            source: boxed_source(source),
                        }
                    })?;
                    let connection =
                        accepting
                            .await
                            .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                                action: "finish incoming connection",
                                source: boxed_source(source),
                            })?;
                    let remote_endpoint_id =
                        EndpointIdText::new(connection.remote_id().to_string())
                            .expect("string ID literal/generated value must be non-empty");
                    let (mut send, mut recv) = connection.accept_bi().await.map_err(|source| {
                        MotherIrohEndpointError::ProtocolIo {
                            action: "accept bidirectional stream",
                            source: boxed_source(source),
                        }
                    })?;
                    let request_bytes = match recv
                        .read_to_end(MCT_CALL_FRAME_READ_BUDGET_BYTES)
                        .await
                    {
                        Ok(request_bytes) => request_bytes,
                        Err(_source) if alpn.as_slice() == MCT_CALL_ALPN.as_bytes() => {
                            let mut state_guard = state.lock().await;
                            let (trace_id, evaluation, reply) =
                                malformed_call_evaluation_and_reply(&mut state_guard);
                            let receipt_observation_id =
                                state_guard.next_observation_id("call-received");
                            drop(state_guard);
                            observation_sink
                                .record(MctIrohObservationBatch {
                                    durability: MctIrohObservationDurability::BeforeEffect,
                                    facts: vec![
                                        malformed_call_lifecycle_fact(
                                            MctIrohCallLifecycleStage::Received,
                                            trace_id.clone(),
                                            &evaluation,
                                            receipt_observation_id,
                                        ),
                                        malformed_call_lifecycle_fact(
                                            MctIrohCallLifecycleStage::Malformed,
                                            trace_id.clone(),
                                            &evaluation,
                                            evaluation.observation_id.clone(),
                                        ),
                                    ],
                                })
                                .await
                                .map_err(|source| MotherIrohEndpointError::ProtocolProvider {
                                    action: "durably record malformed mct/call/0 frame",
                                    source,
                                })?;
                            let response_bytes = encode_call_reply_envelope(&reply, None)?;
                            send.write_all(&response_bytes).await.map_err(|source| {
                                MotherIrohEndpointError::ProtocolIo {
                                    action: "write malformed call response stream",
                                    source: boxed_source(source),
                                }
                            })?;
                            send.finish().map_err(|source| {
                                MotherIrohEndpointError::ProtocolIo {
                                    action: "finish malformed call response stream",
                                    source: boxed_source(source),
                                }
                            })?;
                            let served = MctIrohServedProtocol::MalformedCall {
                                trace_id,
                                evaluation,
                                reply,
                            };
                            observation_sink
                                .record(MctIrohObservationBatch {
                                    durability: MctIrohObservationDurability::Buffered,
                                    facts: vec![call_reply_emitted_fact(&served)
                                        .expect("malformed call has reply fact")],
                                })
                                .await
                                .map_err(|sink_source| {
                                    MotherIrohEndpointError::ProtocolProvider {
                                        action: "record malformed mct/call/0 reply",
                                        source: sink_source,
                                    }
                                })?;
                            connection.closed().await;
                            return Ok(served);
                        }
                        Err(source) => {
                            return Err(MotherIrohEndpointError::ProtocolIo {
                                action: "read request stream",
                                source: boxed_source(source),
                            });
                        }
                    };

                    let current_peer_authority = bindings_provider().await?;

                    let (response_bytes, served) = match alpn.as_slice() {
                        bytes if bytes == MCT_HELLO_ALPN.as_bytes() => {
                            let mut request: MctHelloRequest =
                                serde_json::from_slice(&request_bytes).map_err(|source| {
                                    MotherIrohEndpointError::ProtocolJson {
                                        action: "decode mct/hello/0 request",
                                        source,
                                    }
                                })?;
                            request.received_over.endpoint_id = remote_endpoint_id.clone();
                            request.received_over.alpn = MCT_HELLO_ALPN.into();
                            request.received_over.connection_side = ConnectionSide::Incoming;

                            let mut state = state.lock().await;
                            state.forget_hello(&remote_endpoint_id);
                            let hello_policy = HelloPolicy {
                                current_policy_revision: current_peer_authority.policy_revision,
                                ..HelloPolicy::default()
                            };
                            let mut evaluation = evaluate_hello(
                                &request,
                                &current_peer_authority.bindings,
                                &hello_policy,
                                HelloEvaluationContext {
                                    ids: EvaluationIds {
                                        decision_id: state.next_decision_id("hello"),
                                        observation_id: state.next_observation_id("hello"),
                                    },
                                    now: now(),
                                },
                            );
                            if require_binding_signature {
                                evaluation = enforce_hello_binding_signature(
                                    &request,
                                    &current_peer_authority.bindings,
                                    &issuer_endpoint_id,
                                    evaluation,
                                );
                            }
                            let mut response = hello_response(
                                format!("reply-iroh-hello-{}", state.next_suffix()),
                                &evaluation,
                                state.next_observation_id("hello-reply"),
                            );
                            if evaluation.is_admitted() {
                                response.capability_view = capability_view.clone();
                            }
                            let response_bytes =
                                serde_json::to_vec(&response).map_err(|source| {
                                    MotherIrohEndpointError::ProtocolJson {
                                        action: "encode mct/hello/0 response",
                                        source,
                                    }
                                })?;
                            observation_sink
                                .record(MctIrohObservationBatch {
                                    durability: MctIrohObservationDurability::BeforeEffect,
                                    facts: vec![MctIrohObservationFact::HelloEvaluation {
                                        trace_id: request.trace_id.clone(),
                                        evaluation: evaluation.clone(),
                                    }],
                                })
                                .await
                                .map_err(|source| MotherIrohEndpointError::ProtocolProvider {
                                    action: "durably record mct/hello/0 evaluation",
                                    source,
                                })?;
                            state.remember_hello(remote_endpoint_id, evaluation.clone());
                            drop(state);
                            (
                                response_bytes,
                                MctIrohServedProtocol::Hello {
                                    request,
                                    evaluation,
                                    response,
                                },
                            )
                        }
                        bytes if bytes == MCT_CALL_ALPN.as_bytes() => {
                            match decode_call_request_envelope(&request_bytes) {
                                Err(_) => {
                                    let mut state_guard = state.lock().await;
                                    let (trace_id, evaluation, reply) =
                                        malformed_call_evaluation_and_reply(&mut state_guard);
                                    let receipt_observation_id =
                                        state_guard.next_observation_id("call-received");
                                    drop(state_guard);
                                    observation_sink
                                        .record(MctIrohObservationBatch {
                                            durability:
                                                MctIrohObservationDurability::BeforeEffect,
                                            facts: vec![
                                                malformed_call_lifecycle_fact(
                                                    MctIrohCallLifecycleStage::Received,
                                                    trace_id.clone(),
                                                    &evaluation,
                                                    receipt_observation_id,
                                                ),
                                                malformed_call_lifecycle_fact(
                                                    MctIrohCallLifecycleStage::Malformed,
                                                    trace_id.clone(),
                                                    &evaluation,
                                                    evaluation.observation_id.clone(),
                                                ),
                                            ],
                                        })
                                        .await
                                        .map_err(|source| {
                                            MotherIrohEndpointError::ProtocolProvider {
                                                action:
                                                    "durably record malformed mct/call/0 envelope",
                                                source,
                                            }
                                        })?;
                                    let response_bytes =
                                        encode_call_reply_envelope(&reply, None)?;
                                    (
                                        response_bytes,
                                        MctIrohServedProtocol::MalformedCall {
                                            trace_id,
                                            evaluation,
                                            reply,
                                        },
                                    )
                                }
                                Ok((mut request, inline_payload_bytes)) => {
                                    request.received_over.endpoint_id =
                                        remote_endpoint_id.clone();
                                    request.received_over.alpn = MCT_CALL_ALPN.into();
                                    request.received_over.connection_side =
                                        ConnectionSide::Incoming;

                                    let validation_failed = request.validate().is_err();
                                    let payload_decision = (!validation_failed).then(|| {
                                        evaluate_payload_integrity(
                                            PayloadIntegritySubject::Request,
                                            &request.payload,
                                            &observed_inline_payload(
                                                inline_payload_bytes.as_deref(),
                                            ),
                                            MCT_INLINE_PAYLOAD_MAX_BYTES as u64,
                                        )
                                    });
                                    let mut state_guard = state.lock().await;
                                    let constructed_observation_id =
                                        state_guard.next_observation_id("call-constructed");
                                    let mut evaluation = if validation_failed {
                                        malformed_request_evaluation(&request, &mut state_guard)
                                    } else if let Some(payload_decision) = payload_decision.as_ref()
                                        && payload_decision.outcome
                                            != PayloadIntegrityOutcome::Matched
                                    {
                                        payload_malformed_evaluation(
                                            &request,
                                            &mut state_guard,
                                            payload_decision.reason.to_call_protocol_reason(),
                                            payload_decision.safe_message.clone(),
                                        )
                                    } else {
                                        let hello = state_guard
                                            .hello_for_endpoint(&remote_endpoint_id)
                                            .unwrap_or_else(|| {
                                                denied_missing_hello(
                                                    request.protocol_request_id.as_str(),
                                                    &mut state_guard,
                                                )
                                            });
                                        evaluate_call_protocol(
                                            &request,
                                            &hello,
                                            CallEvaluationContext {
                                                ids: CallEvaluationIds {
                                                    decision_id: state_guard
                                                        .next_decision_id("call"),
                                                    observation_id: state_guard
                                                        .next_observation_id("call"),
                                                },
                                                current_peer_authority,
                                                now: now(),
                                            },
                                        )
                                    };
                                    drop(state_guard);

                                    let prefix_facts = if evaluation.outcome
                                        == CallProtocolOutcome::Malformed
                                    {
                                        vec![
                                            call_received_fact(&request),
                                            call_malformed_fact(&request, &evaluation),
                                        ]
                                    } else {
                                        vec![
                                            call_received_fact(&request),
                                            call_constructed_fact(
                                                &request,
                                                constructed_observation_id,
                                            ),
                                            call_authority_fact(&request, &evaluation),
                                        ]
                                    };
                                    observation_sink
                                        .record(MctIrohObservationBatch {
                                            durability:
                                                MctIrohObservationDurability::BeforeEffect,
                                            facts: prefix_facts,
                                        })
                                        .await
                                        .map_err(|source| {
                                            MotherIrohEndpointError::ProtocolProvider {
                                                action:
                                                    "durably record mct/call/0 authority prefix",
                                                source,
                                            }
                                        })?;

                                    let handled = if evaluation.is_accepted_for_routing() {
                                        Some(
                                            call_handler(
                                                request.clone(),
                                                evaluation.clone(),
                                                inline_payload_bytes.clone(),
                                            )
                                            .await,
                                        )
                                    } else {
                                        None
                                    };
                                    if let Some(handled) = handled.as_ref() {
                                        evaluation.outcome = handled.outcome;
                                        evaluation.safe_message = handled.safe_message.clone();
                                        evaluation.route_decision_id =
                                            handled.route_decision_id.clone();
                                    }
                                    let mut state_guard = state.lock().await;
                                    let reply =
                                        call_reply_from_evaluation_with_result_payload_and_route(
                                            ReplyId::new(format!(
                                                "reply-iroh-call-{}",
                                                state_guard.next_suffix()
                                            ))
                                            .expect(
                                                "string ID literal/generated value must be non-empty",
                                            ),
                                            &evaluation,
                                            handled
                                                .as_ref()
                                                .and_then(|handled| handled.result_ref.clone()),
                                            handled
                                                .as_ref()
                                                .map(|handled| handled.result_payload.clone())
                                                .unwrap_or(MctCallPayloadHandle::Empty),
                                            handled
                                                .as_ref()
                                                .and_then(|handled| handled.route_taken.clone()),
                                            state_guard.next_observation_id("call-reply"),
                                        );
                                    let result_observation_id =
                                        state_guard.next_observation_id("call-result");
                                    drop(state_guard);
                                    if evaluation.outcome != CallProtocolOutcome::Malformed {
                                        observation_sink
                                            .record(MctIrohObservationBatch {
                                                durability:
                                                    MctIrohObservationDurability::Buffered,
                                                facts: vec![call_result_fact(
                                                    &request,
                                                    &evaluation,
                                                    result_observation_id,
                                                )],
                                            })
                                            .await
                                            .map_err(|source| {
                                                MotherIrohEndpointError::ProtocolProvider {
                                                    action: "record mct/call/0 result",
                                                    source,
                                                }
                                            })?;
                                    }
                                    let response_bytes = encode_call_reply_envelope(
                                        &reply,
                                        handled.as_ref().and_then(|handled| {
                                            handled.inline_result_payload.as_deref()
                                        }),
                                    )?;
                                    (
                                        response_bytes,
                                        MctIrohServedProtocol::Call {
                                            request,
                                            evaluation,
                                            reply,
                                        },
                                    )
                                }
                            }
                        }
                        other => {
                            let alpn = String::from_utf8_lossy(other).to_string();
                            return Err(MotherIrohEndpointError::UnsupportedAlpn { alpn });
                        }
                    };

                    send.write_all(&response_bytes).await.map_err(|source| {
                        MotherIrohEndpointError::ProtocolIo {
                            action: "write response stream",
                            source: boxed_source(source),
                        }
                    })?;
                    send.finish()
                        .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                            action: "finish response stream",
                            source: boxed_source(source),
                        })?;
                    if let Some(reply_fact) = call_reply_emitted_fact(&served) {
                        observation_sink
                            .record(MctIrohObservationBatch {
                                durability: MctIrohObservationDurability::Buffered,
                                facts: vec![reply_fact],
                            })
                            .await
                            .map_err(|source| MotherIrohEndpointError::ProtocolProvider {
                                action: "record mct/call/0 reply emission",
                                source,
                            })?;
                    }
                    connection.closed().await;
                    Ok(served)
                })
                .await
                {
                    Ok(Ok(served)) => Ok(served),
                    Ok(Err(error)) => Err(error),
                    Err(_) => Err(MotherIrohEndpointError::ProtocolTimeout {
                        action: "serve incoming MCT connection",
                    }),
                };

                if let (Ok(served), Some(events)) = (&served, events) {
                    let _ = events
                        .send(MctIrohServeEvent::Served(Box::new(served.clone())))
                        .await;
                }
                active_tasks.fetch_sub(1, Ordering::SeqCst);
                drop(permit);
                if let Err(error) = served {
                    let reply_observation_failed = matches!(
                        &error,
                        MotherIrohEndpointError::ProtocolProvider { action, .. }
                            if *action == "record mct/call/0 reply emission"
                                || *action == "record malformed mct/call/0 reply"
                    );
                    if reply_observation_failed {
                        let _ = task_error_tx.send(error);
                    }
                }
            });
        }

        while active_tasks.load(Ordering::SeqCst) > 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    }

    pub async fn serve_next(
        &self,
        state: &mut MctIrohServeState,
        bindings: &[MctPeerBinding],
        observation_sink: &MctIrohObservationSink,
        now: Timestamp,
        result_ref: Option<ResultRef>,
    ) -> MotherIrohEndpointResult<MctIrohServedProtocol> {
        self.serve_next_with_call_handler(state, bindings, observation_sink, now, move |_, _, _| {
            MctIrohCallHandlerResult::accepted_for_routing(result_ref.clone())
        })
        .await
    }

    pub async fn serve_next_with_call_handler<F>(
        &self,
        state: &mut MctIrohServeState,
        bindings: &[MctPeerBinding],
        observation_sink: &MctIrohObservationSink,
        now: Timestamp,
        call_handler: F,
    ) -> MotherIrohEndpointResult<MctIrohServedProtocol>
    where
        F: FnMut(
            &MctCallProtocolRequest,
            &MctCallProtocolEvaluation,
            Option<&[u8]>,
        ) -> MctIrohCallHandlerResult,
    {
        self.serve_next_with_call_handler_timeout(
            state,
            bindings,
            observation_sink,
            now,
            SERVE_CONNECTION_TIMEOUT,
            call_handler,
        )
        .await
    }

    pub(crate) async fn serve_next_with_call_handler_timeout<F>(
        &self,
        state: &mut MctIrohServeState,
        bindings: &[MctPeerBinding],
        observation_sink: &MctIrohObservationSink,
        now: Timestamp,
        connection_timeout: Duration,
        mut call_handler: F,
    ) -> MotherIrohEndpointResult<MctIrohServedProtocol>
    where
        F: FnMut(
            &MctCallProtocolRequest,
            &MctCallProtocolEvaluation,
            Option<&[u8]>,
        ) -> MctIrohCallHandlerResult,
    {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(MotherIrohEndpointError::EndpointClosed)?;
        let incoming = endpoint
            .accept()
            .await
            .ok_or(MotherIrohEndpointError::EndpointClosed)?;
        tokio::time::timeout(connection_timeout, async {
            let mut accepting =
                incoming
                    .accept()
                    .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                        action: "accept incoming connection",
                        source: boxed_source(source),
                    })?;
            let alpn =
                accepting
                    .alpn()
                    .await
                    .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                        action: "read incoming ALPN",
                        source: boxed_source(source),
                    })?;
            let connection =
                accepting
                    .await
                    .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                        action: "finish incoming connection",
                        source: boxed_source(source),
                    })?;
            let remote_endpoint_id = EndpointIdText::new(connection.remote_id().to_string())
                .expect("string ID literal/generated value must be non-empty");
            let (mut send, mut recv) = connection.accept_bi().await.map_err(|source| {
                MotherIrohEndpointError::ProtocolIo {
                    action: "accept bidirectional stream",
                    source: boxed_source(source),
                }
            })?;
            let request_bytes = match recv.read_to_end(MCT_CALL_FRAME_READ_BUDGET_BYTES).await {
                Ok(request_bytes) => request_bytes,
                Err(_source) if alpn.as_slice() == MCT_CALL_ALPN.as_bytes() => {
                    let (trace_id, evaluation, reply) = malformed_call_evaluation_and_reply(state);
                    let receipt_observation_id = state.next_observation_id("call-received");
                    observation_sink
                        .record(MctIrohObservationBatch {
                            durability: MctIrohObservationDurability::BeforeEffect,
                            facts: vec![
                                malformed_call_lifecycle_fact(
                                    MctIrohCallLifecycleStage::Received,
                                    trace_id.clone(),
                                    &evaluation,
                                    receipt_observation_id,
                                ),
                                malformed_call_lifecycle_fact(
                                    MctIrohCallLifecycleStage::Malformed,
                                    trace_id.clone(),
                                    &evaluation,
                                    evaluation.observation_id.clone(),
                                ),
                            ],
                        })
                        .await
                        .map_err(|source| MotherIrohEndpointError::ProtocolProvider {
                            action: "durably record malformed mct/call/0 frame",
                            source,
                        })?;
                    let response_bytes = encode_call_reply_envelope(&reply, None)?;
                    send.write_all(&response_bytes).await.map_err(|source| {
                        MotherIrohEndpointError::ProtocolIo {
                            action: "write malformed call response stream",
                            source: boxed_source(source),
                        }
                    })?;
                    send.finish()
                        .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                            action: "finish malformed call response stream",
                            source: boxed_source(source),
                        })?;
                    let served = MctIrohServedProtocol::MalformedCall {
                        trace_id,
                        evaluation,
                        reply,
                    };
                    observation_sink
                        .record(MctIrohObservationBatch {
                            durability: MctIrohObservationDurability::Buffered,
                            facts: vec![
                                call_reply_emitted_fact(&served)
                                    .expect("malformed call has reply fact"),
                            ],
                        })
                        .await
                        .map_err(|source| MotherIrohEndpointError::ProtocolProvider {
                            action: "record malformed mct/call/0 reply",
                            source,
                        })?;
                    connection.closed().await;
                    return Ok(served);
                }
                Err(source) => {
                    return Err(MotherIrohEndpointError::ProtocolIo {
                        action: "read request stream",
                        source: boxed_source(source),
                    });
                }
            };

            let (response_bytes, served) = match alpn.as_slice() {
                bytes if bytes == MCT_HELLO_ALPN.as_bytes() => {
                    let mut request: MctHelloRequest = serde_json::from_slice(&request_bytes)
                        .map_err(|source| MotherIrohEndpointError::ProtocolJson {
                            action: "decode mct/hello/0 request",
                            source,
                        })?;
                    request.received_over.endpoint_id = remote_endpoint_id.clone();
                    request.received_over.alpn = MCT_HELLO_ALPN.into();
                    request.received_over.connection_side = ConnectionSide::Incoming;

                    let evaluation = evaluate_hello(
                        &request,
                        bindings,
                        &HelloPolicy::default(),
                        HelloEvaluationContext {
                            ids: EvaluationIds {
                                decision_id: state.next_decision_id("hello"),
                                observation_id: state.next_observation_id("hello"),
                            },
                            now,
                        },
                    );
                    observation_sink
                        .record(MctIrohObservationBatch {
                            durability: MctIrohObservationDurability::BeforeEffect,
                            facts: vec![MctIrohObservationFact::HelloEvaluation {
                                trace_id: request.trace_id.clone(),
                                evaluation: evaluation.clone(),
                            }],
                        })
                        .await
                        .map_err(|source| MotherIrohEndpointError::ProtocolProvider {
                            action: "durably record mct/hello/0 evaluation",
                            source,
                        })?;
                    state.remember_hello(remote_endpoint_id.clone(), evaluation.clone());
                    let response = hello_response(
                        format!("reply-iroh-hello-{}", state.next_suffix()),
                        &evaluation,
                        state.next_observation_id("hello-reply"),
                    );
                    let response_bytes = serde_json::to_vec(&response).map_err(|source| {
                        MotherIrohEndpointError::ProtocolJson {
                            action: "encode mct/hello/0 response",
                            source,
                        }
                    })?;
                    (
                        response_bytes,
                        MctIrohServedProtocol::Hello {
                            request,
                            evaluation,
                            response,
                        },
                    )
                }
                bytes if bytes == MCT_CALL_ALPN.as_bytes() => {
                    let (mut request, inline_payload_bytes) =
                        match decode_call_request_envelope(&request_bytes) {
                            Ok(decoded) => decoded,
                            Err(_) => {
                                let (trace_id, evaluation, reply) =
                                    malformed_call_evaluation_and_reply(state);
                                let receipt_observation_id =
                                    state.next_observation_id("call-received");
                                observation_sink
                                    .record(MctIrohObservationBatch {
                                        durability: MctIrohObservationDurability::BeforeEffect,
                                        facts: vec![
                                            malformed_call_lifecycle_fact(
                                                MctIrohCallLifecycleStage::Received,
                                                trace_id.clone(),
                                                &evaluation,
                                                receipt_observation_id,
                                            ),
                                            malformed_call_lifecycle_fact(
                                                MctIrohCallLifecycleStage::Malformed,
                                                trace_id.clone(),
                                                &evaluation,
                                                evaluation.observation_id.clone(),
                                            ),
                                        ],
                                    })
                                    .await
                                    .map_err(|source| {
                                        MotherIrohEndpointError::ProtocolProvider {
                                            action: "durably record malformed mct/call/0 envelope",
                                            source,
                                        }
                                    })?;
                                let response_bytes = encode_call_reply_envelope(&reply, None)?;
                                send.write_all(&response_bytes).await.map_err(|source| {
                                    MotherIrohEndpointError::ProtocolIo {
                                        action: "write malformed call response stream",
                                        source: boxed_source(source),
                                    }
                                })?;
                                send.finish().map_err(|source| {
                                    MotherIrohEndpointError::ProtocolIo {
                                        action: "finish malformed call response stream",
                                        source: boxed_source(source),
                                    }
                                })?;
                                let served = MctIrohServedProtocol::MalformedCall {
                                    trace_id,
                                    evaluation,
                                    reply,
                                };
                                observation_sink
                                    .record(MctIrohObservationBatch {
                                        durability: MctIrohObservationDurability::Buffered,
                                        facts: vec![
                                            call_reply_emitted_fact(&served)
                                                .expect("malformed call has reply fact"),
                                        ],
                                    })
                                    .await
                                    .map_err(|source| {
                                        MotherIrohEndpointError::ProtocolProvider {
                                            action: "record malformed mct/call/0 reply",
                                            source,
                                        }
                                    })?;
                                connection.closed().await;
                                return Ok(served);
                            }
                        };
                    request.received_over.endpoint_id = remote_endpoint_id.clone();
                    request.received_over.alpn = MCT_CALL_ALPN.into();
                    request.received_over.connection_side = ConnectionSide::Incoming;
                    let validation_failed = request.validate().is_err();
                    let payload_decision = (!validation_failed).then(|| {
                        evaluate_payload_integrity(
                            PayloadIntegritySubject::Request,
                            &request.payload,
                            &observed_inline_payload(inline_payload_bytes.as_deref()),
                            MCT_INLINE_PAYLOAD_MAX_BYTES as u64,
                        )
                    });
                    let constructed_observation_id = state.next_observation_id("call-constructed");
                    let mut evaluation = if validation_failed {
                        malformed_request_evaluation(&request, state)
                    } else if payload_decision.as_ref().is_some_and(|decision| {
                        decision.outcome == PayloadIntegrityOutcome::Matched
                    }) {
                        let hello = state
                            .hello_for_endpoint(&remote_endpoint_id)
                            .unwrap_or_else(|| {
                                denied_missing_hello(request.protocol_request_id.as_str(), state)
                            });
                        evaluate_call_protocol(
                            &request,
                            &hello,
                            CallEvaluationContext {
                                ids: CallEvaluationIds {
                                    decision_id: state.next_decision_id("call"),
                                    observation_id: state.next_observation_id("call"),
                                },
                                current_peer_authority: MctPeerAuthoritySnapshot {
                                    bindings: bindings.to_vec(),
                                    policy_revision: HelloPolicy::default().current_policy_revision,
                                },
                                now: now.clone(),
                            },
                        )
                    } else {
                        let payload_decision = payload_decision
                            .as_ref()
                            .expect("validated request has payload integrity decision");
                        payload_malformed_evaluation(
                            &request,
                            state,
                            payload_decision.reason.to_call_protocol_reason(),
                            payload_decision.safe_message.clone(),
                        )
                    };
                    let prefix_facts = if evaluation.outcome == CallProtocolOutcome::Malformed {
                        vec![
                            call_received_fact(&request),
                            call_malformed_fact(&request, &evaluation),
                        ]
                    } else {
                        vec![
                            call_received_fact(&request),
                            call_constructed_fact(&request, constructed_observation_id),
                            call_authority_fact(&request, &evaluation),
                        ]
                    };
                    observation_sink
                        .record(MctIrohObservationBatch {
                            durability: MctIrohObservationDurability::BeforeEffect,
                            facts: prefix_facts,
                        })
                        .await
                        .map_err(|source| MotherIrohEndpointError::ProtocolProvider {
                            action: "durably record mct/call/0 authority prefix",
                            source,
                        })?;
                    let handled = if evaluation.is_accepted_for_routing() {
                        Some(call_handler(
                            &request,
                            &evaluation,
                            inline_payload_bytes.as_deref(),
                        ))
                    } else {
                        None
                    };
                    if let Some(handled) = handled.as_ref() {
                        evaluation.outcome = handled.outcome;
                        evaluation.safe_message = handled.safe_message.clone();
                        evaluation.route_decision_id = handled.route_decision_id.clone();
                    }
                    let reply = call_reply_from_evaluation_with_result_payload_and_route(
                        ReplyId::new(format!("reply-iroh-call-{}", state.next_suffix()))
                            .expect("string ID literal/generated value must be non-empty"),
                        &evaluation,
                        handled
                            .as_ref()
                            .and_then(|handled| handled.result_ref.clone()),
                        handled
                            .as_ref()
                            .map(|handled| handled.result_payload.clone())
                            .unwrap_or(MctCallPayloadHandle::Empty),
                        handled
                            .as_ref()
                            .and_then(|handled| handled.route_taken.clone()),
                        state.next_observation_id("call-reply"),
                    );
                    if evaluation.outcome != CallProtocolOutcome::Malformed {
                        observation_sink
                            .record(MctIrohObservationBatch {
                                durability: MctIrohObservationDurability::Buffered,
                                facts: vec![call_result_fact(
                                    &request,
                                    &evaluation,
                                    state.next_observation_id("call-result"),
                                )],
                            })
                            .await
                            .map_err(|source| MotherIrohEndpointError::ProtocolProvider {
                                action: "record mct/call/0 result",
                                source,
                            })?;
                    }
                    let response_bytes = encode_call_reply_envelope(
                        &reply,
                        handled
                            .as_ref()
                            .and_then(|handled| handled.inline_result_payload.as_deref()),
                    )?;
                    (
                        response_bytes,
                        MctIrohServedProtocol::Call {
                            request,
                            evaluation,
                            reply,
                        },
                    )
                }
                other => {
                    let alpn = String::from_utf8_lossy(other).to_string();
                    return Err(MotherIrohEndpointError::UnsupportedAlpn { alpn });
                }
            };

            send.write_all(&response_bytes).await.map_err(|source| {
                MotherIrohEndpointError::ProtocolIo {
                    action: "write response stream",
                    source: boxed_source(source),
                }
            })?;
            send.finish()
                .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                    action: "finish response stream",
                    source: boxed_source(source),
                })?;
            if let Some(reply_fact) = call_reply_emitted_fact(&served) {
                observation_sink
                    .record(MctIrohObservationBatch {
                        durability: MctIrohObservationDurability::Buffered,
                        facts: vec![reply_fact],
                    })
                    .await
                    .map_err(|source| MotherIrohEndpointError::ProtocolProvider {
                        action: "record mct/call/0 reply emission",
                        source,
                    })?;
            }
            connection.closed().await;
            Ok(served)
        })
        .await
        .map_err(|_| MotherIrohEndpointError::ProtocolTimeout {
            action: "serve incoming MCT connection",
        })?
    }

    async fn roundtrip_json<Request, Response>(
        &self,
        peer: &MotherIrohEndpointTicket,
        alpn: &'static str,
        request: &Request,
    ) -> MotherIrohEndpointResult<Response>
    where
        Request: Serialize,
        Response: DeserializeOwned,
    {
        self.roundtrip_json_with_timeout(peer, alpn, request, ROUNDTRIP_CONNECTION_TIMEOUT)
            .await
    }

    #[cfg(test)]
    pub(crate) async fn send_hello_with_timeout(
        &self,
        peer: &MotherIrohEndpointTicket,
        request: &MctHelloRequest,
        connection_timeout: Duration,
    ) -> MotherIrohEndpointResult<MctHelloResponse> {
        self.roundtrip_json_with_timeout(peer, MCT_HELLO_ALPN, request, connection_timeout)
            .await
    }

    async fn roundtrip_call_payload(
        &self,
        peer: &MotherIrohEndpointTicket,
        request: &MctCallProtocolRequest,
        inline_payload: Option<&[u8]>,
        verify_outbound_payload: bool,
    ) -> MotherIrohEndpointResult<MctIrohCallPayloadReply> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(MotherIrohEndpointError::EndpointClosed)?;
        tokio::time::timeout(ROUNDTRIP_CONNECTION_TIMEOUT, async {
            let connection = endpoint
                .connect(endpoint_addr_from_ticket(peer)?, MCT_CALL_ALPN.as_bytes())
                .await
                .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                    action: "connect to peer",
                    source: boxed_source(source),
                })?;
            let (mut send, mut recv) = connection.open_bi().await.map_err(|source| {
                MotherIrohEndpointError::ProtocolIo {
                    action: "open bidirectional stream",
                    source: boxed_source(source),
                }
            })?;
            let bytes = if verify_outbound_payload {
                encode_call_request_envelope(request, inline_payload)?
            } else {
                encode_call_request_envelope_unchecked(request, inline_payload)?
            };
            send.write_all(&bytes)
                .await
                .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                    action: "write request stream",
                    source: boxed_source(source),
                })?;
            send.finish()
                .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                    action: "finish request stream",
                    source: boxed_source(source),
                })?;
            let response = recv
                .read_to_end(MCT_CALL_FRAME_READ_BUDGET_BYTES)
                .await
                .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                    action: "read response stream",
                    source: boxed_source(source),
                })?;
            connection.close(0u32.into(), b"mct client complete");
            decode_call_reply_envelope(&response)
        })
        .await
        .map_err(|_| MotherIrohEndpointError::ProtocolTimeout {
            action: "complete outbound MCT roundtrip",
        })?
    }

    async fn roundtrip_json_with_timeout<Request, Response>(
        &self,
        peer: &MotherIrohEndpointTicket,
        alpn: &'static str,
        request: &Request,
        connection_timeout: Duration,
    ) -> MotherIrohEndpointResult<Response>
    where
        Request: Serialize,
        Response: DeserializeOwned,
    {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(MotherIrohEndpointError::EndpointClosed)?;
        tokio::time::timeout(connection_timeout, async {
            let connection = endpoint
                .connect(endpoint_addr_from_ticket(peer)?, alpn.as_bytes())
                .await
                .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                    action: "connect to peer",
                    source: boxed_source(source),
                })?;
            let (mut send, mut recv) = connection.open_bi().await.map_err(|source| {
                MotherIrohEndpointError::ProtocolIo {
                    action: "open bidirectional stream",
                    source: boxed_source(source),
                }
            })?;
            let bytes = serde_json::to_vec(request).map_err(|source| {
                MotherIrohEndpointError::ProtocolJson {
                    action: "encode request",
                    source,
                }
            })?;
            send.write_all(&bytes)
                .await
                .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                    action: "write request stream",
                    source: boxed_source(source),
                })?;
            send.finish()
                .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                    action: "finish request stream",
                    source: boxed_source(source),
                })?;
            let response = recv
                .read_to_end(MCT_CALL_FRAME_READ_BUDGET_BYTES)
                .await
                .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                    action: "read response stream",
                    source: boxed_source(source),
                })?;
            connection.close(0u32.into(), b"mct client complete");
            serde_json::from_slice(&response).map_err(|source| {
                MotherIrohEndpointError::ProtocolJson {
                    action: "decode response",
                    source,
                }
            })
        })
        .await
        .map_err(|_| MotherIrohEndpointError::ProtocolTimeout {
            action: "complete outbound MCT roundtrip",
        })?
    }
}

fn denied_missing_hello(
    request_id: &str,
    state: &mut MctIrohServeState,
) -> MctHelloAdmissionEvaluation {
    MctHelloAdmissionEvaluation {
        decision_id: state.next_decision_id("missing-hello"),
        request_id: request_id.to_string(),
        peer_admission_decision_id: None,
        selected_binding_id: None,
        selected_node_id: None,
        selected_vision_id: None,
        selected_policy_revision: None,
        negotiated_protocol: None,
        accepted_alpns: Vec::new(),
        hello_outcome: HelloOutcome::Denied,
        reason: HelloReason::MissingBinding,
        safe_reason: SafeHelloReason::NotAuthorized,
        observation_id: state.next_observation_id("missing-hello"),
    }
}

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
    future::Future,
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

#[derive(Clone, Debug)]
pub struct MctIrohConcurrentServeConfig {
    pub max_concurrent_connections: usize,
    pub connection_timeout: Duration,
    pub events: Option<mpsc::Sender<MctIrohServeEvent>>,
    pub require_binding_signature: bool,
    pub capability_view: Option<MctHelloCapabilityView>,
}

impl Default for MctIrohConcurrentServeConfig {
    fn default() -> Self {
        Self {
            max_concurrent_connections: 64,
            connection_timeout: SERVE_CONNECTION_TIMEOUT,
            events: None,
            require_binding_signature: false,
            capability_view: None,
        }
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
                async move { Ok((*bindings).clone()) }
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
            Future<Output = MotherIrohEndpointResult<Vec<MctPeerBinding>>> + Send + 'static,
    {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(MotherIrohEndpointError::EndpointClosed)?
            .clone();
        let issuer_endpoint_id = self.snapshot().endpoint_id;
        let require_binding_signature = config.require_binding_signature;
        let capability_view = config.capability_view.clone();
        let state = Arc::new(Mutex::new(state));
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_connections));
        let active_tasks = Arc::new(AtomicU64::new(0));

        loop {
            let Some(incoming) = endpoint.accept().await else {
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
                    let request_bytes = recv
                        .read_to_end(MCT_CALL_FRAME_READ_BUDGET_BYTES)
                        .await
                        .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                            action: "read request stream",
                            source: boxed_source(source),
                        })?;

                    let bindings = bindings_provider().await?;

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
                            let mut evaluation = evaluate_hello(
                                &request,
                                &bindings,
                                &HelloPolicy::default(),
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
                                    &bindings,
                                    &issuer_endpoint_id,
                                    evaluation,
                                );
                            }
                            state.remember_hello(remote_endpoint_id, evaluation.clone());
                            let mut response = hello_response(
                                format!("reply-iroh-hello-{}", state.next_suffix()),
                                &evaluation,
                                state.next_observation_id("hello-reply"),
                            );
                            if evaluation.is_admitted() {
                                response.capability_view = capability_view.clone();
                            }
                            drop(state);
                            let response_bytes =
                                serde_json::to_vec(&response).map_err(|source| {
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
                                decode_call_request_envelope(&request_bytes)?;
                            request.received_over.endpoint_id = remote_endpoint_id.clone();
                            request.received_over.alpn = MCT_CALL_ALPN.into();
                            request.received_over.connection_side = ConnectionSide::Incoming;
                            request.validate().map_err(|source| {
                                MotherIrohEndpointError::ProtocolKernel {
                                    action: "validate inbound mct/call/0 request",
                                    source,
                                }
                            })?;

                            let payload_decision = evaluate_payload_integrity(
                                PayloadIntegritySubject::Request,
                                &request.payload,
                                &observed_inline_payload(inline_payload_bytes.as_deref()),
                                MCT_INLINE_PAYLOAD_MAX_BYTES as u64,
                            );
                            let mut state_guard = state.lock().await;
                            let mut evaluation =
                                if payload_decision.outcome == PayloadIntegrityOutcome::Matched {
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
                                        CallEvaluationIds {
                                            decision_id: state_guard.next_decision_id("call"),
                                            observation_id: state_guard.next_observation_id("call"),
                                        },
                                    )
                                } else {
                                    payload_malformed_evaluation(
                                        &request,
                                        &mut state_guard,
                                        payload_decision.reason.to_call_protocol_reason(),
                                        payload_decision.safe_message.clone(),
                                    )
                                };
                            drop(state_guard);

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
                                evaluation.route_decision_id = handled.route_decision_id.clone();
                            }
                            let mut state_guard = state.lock().await;
                            let reply = call_reply_from_evaluation_with_result_payload_and_route(
                                ReplyId::new(format!(
                                    "reply-iroh-call-{}",
                                    state_guard.next_suffix()
                                ))
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
                                state_guard.next_observation_id("call-reply"),
                            );
                            drop(state_guard);
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
                served.map(|_| ())
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
        now: Timestamp,
        result_ref: Option<ResultRef>,
    ) -> MotherIrohEndpointResult<MctIrohServedProtocol> {
        self.serve_next_with_call_handler(state, bindings, now, move |_, _, _| {
            MctIrohCallHandlerResult::accepted_for_routing(result_ref.clone())
        })
        .await
    }

    pub async fn serve_next_with_call_handler<F>(
        &self,
        state: &mut MctIrohServeState,
        bindings: &[MctPeerBinding],
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
            let request_bytes = recv
                .read_to_end(MCT_CALL_FRAME_READ_BUDGET_BYTES)
                .await
                .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                    action: "read request stream",
                    source: boxed_source(source),
                })?;

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
                        decode_call_request_envelope(&request_bytes)?;
                    request.received_over.endpoint_id = remote_endpoint_id.clone();
                    request.received_over.alpn = MCT_CALL_ALPN.into();
                    request.received_over.connection_side = ConnectionSide::Incoming;
                    request.validate().map_err(|source| {
                        MotherIrohEndpointError::ProtocolKernel {
                            action: "validate inbound mct/call/0 request",
                            source,
                        }
                    })?;

                    let payload_decision = evaluate_payload_integrity(
                        PayloadIntegritySubject::Request,
                        &request.payload,
                        &observed_inline_payload(inline_payload_bytes.as_deref()),
                        MCT_INLINE_PAYLOAD_MAX_BYTES as u64,
                    );
                    let mut evaluation = if payload_decision.outcome
                        == PayloadIntegrityOutcome::Matched
                    {
                        let hello = state
                            .hello_for_endpoint(&remote_endpoint_id)
                            .unwrap_or_else(|| {
                                denied_missing_hello(request.protocol_request_id.as_str(), state)
                            });
                        evaluate_call_protocol(
                            &request,
                            &hello,
                            CallEvaluationIds {
                                decision_id: state.next_decision_id("call"),
                                observation_id: state.next_observation_id("call"),
                            },
                        )
                    } else {
                        payload_malformed_evaluation(
                            &request,
                            state,
                            payload_decision.reason.to_call_protocol_reason(),
                            payload_decision.safe_message.clone(),
                        )
                    };
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
        negotiated_protocol: None,
        accepted_alpns: Vec::new(),
        hello_outcome: HelloOutcome::Denied,
        reason: HelloReason::MissingBinding,
        safe_reason: SafeHelloReason::NotAuthorized,
        observation_id: state.next_observation_id("missing-hello"),
    }
}

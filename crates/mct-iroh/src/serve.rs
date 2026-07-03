mod internal;

pub(crate) use internal::endpoint_addr_from_ticket;

use crate::{
    endpoint::{
        MotherIrohEndpoint, MotherIrohEndpointError, MotherIrohEndpointResult,
        MotherIrohEndpointTicket, boxed_source,
    },
    identity::encode_hex,
};
use internal::{ROUNDTRIP_CONNECTION_TIMEOUT, SERVE_CONNECTION_TIMEOUT};
use iroh::SecretKey;
use mct_kernel::*;
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

/// Mutable state for serving MCT protocols over one Mother-owned endpoint.
///
/// Decision and observation IDs minted from this state include a random prefix
/// generated once in `new`, plus a state-local monotonic counter, so a daemon
/// restart does not reuse the same IDs after the counter resets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctIrohServeState {
    pub last_hello: Option<MctHelloAdmissionEvaluation>,
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
pub struct MctIrohCallHandlerResult {
    pub result_ref: Option<ResultRef>,
    pub outcome: CallProtocolOutcome,
    pub safe_message: String,
}

impl MctIrohCallHandlerResult {
    pub fn accepted_for_routing(result_ref: Option<ResultRef>) -> Self {
        Self {
            result_ref,
            outcome: CallProtocolOutcome::AcceptedForRouting,
            safe_message: "accepted for routing".into(),
        }
    }

    pub fn completed(result_ref: ResultRef) -> Self {
        Self {
            result_ref: Some(result_ref),
            outcome: CallProtocolOutcome::Completed,
            safe_message: "call completed".into(),
        }
    }

    pub fn failed(safe_message: impl Into<String>) -> Self {
        Self {
            result_ref: None,
            outcome: CallProtocolOutcome::Failed,
            safe_message: safe_message.into(),
        }
    }

    pub fn timed_out() -> Self {
        Self {
            result_ref: None,
            outcome: CallProtocolOutcome::TimedOut,
            safe_message: "call timed out".into(),
        }
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
        request
            .validate()
            .map_err(|source| MotherIrohEndpointError::ProtocolKernel {
                action: "validate outbound mct/call/0 request",
                source,
            })?;
        let reply: MctCallProtocolReply = self.roundtrip_json(peer, MCT_CALL_ALPN, request).await?;
        reply
            .validate()
            .map_err(|source| MotherIrohEndpointError::ProtocolKernel {
                action: "validate inbound mct/call/0 reply",
                source,
            })?;
        Ok(reply)
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

    pub async fn serve_next(
        &self,
        state: &mut MctIrohServeState,
        bindings: &[MctPeerBinding],
        now: Timestamp,
        result_ref: Option<ResultRef>,
    ) -> MotherIrohEndpointResult<MctIrohServedProtocol> {
        self.serve_next_with_call_handler(state, bindings, now, move |_, _| {
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
        F: FnMut(&MctCallProtocolRequest, &MctCallProtocolEvaluation) -> MctIrohCallHandlerResult,
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
        F: FnMut(&MctCallProtocolRequest, &MctCallProtocolEvaluation) -> MctIrohCallHandlerResult,
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
            let request_bytes = recv.read_to_end(64 * 1024).await.map_err(|source| {
                MotherIrohEndpointError::ProtocolIo {
                    action: "read request stream",
                    source: boxed_source(source),
                }
            })?;

            let (response_bytes, served) = match alpn.as_slice() {
                bytes if bytes == MCT_HELLO_ALPN.as_bytes() => {
                    let mut request: MctHelloRequest = serde_json::from_slice(&request_bytes)
                        .map_err(|source| MotherIrohEndpointError::ProtocolJson {
                            action: "decode mct/hello/0 request",
                            source,
                        })?;
                    request.received_over.endpoint_id = remote_endpoint_id;
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
                    state.last_hello = Some(evaluation.clone());
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
                    let mut request: MctCallProtocolRequest =
                        serde_json::from_slice(&request_bytes).map_err(|source| {
                            MotherIrohEndpointError::ProtocolJson {
                                action: "decode mct/call/0 request",
                                source,
                            }
                        })?;
                    request.received_over.endpoint_id = remote_endpoint_id;
                    request.received_over.alpn = MCT_CALL_ALPN.into();
                    request.received_over.connection_side = ConnectionSide::Incoming;
                    request.validate().map_err(|source| {
                        MotherIrohEndpointError::ProtocolKernel {
                            action: "validate inbound mct/call/0 request",
                            source,
                        }
                    })?;

                    let hello = state.last_hello.clone().unwrap_or_else(|| {
                        denied_missing_hello(request.protocol_request_id.as_str(), state)
                    });
                    let mut evaluation = evaluate_call_protocol(
                        &request,
                        &hello,
                        CallEvaluationIds {
                            decision_id: state.next_decision_id("call"),
                            observation_id: state.next_observation_id("call"),
                        },
                    );
                    let reply_result_ref = if evaluation.is_accepted_for_routing() {
                        let handled = call_handler(&request, &evaluation);
                        evaluation.outcome = handled.outcome;
                        evaluation.safe_message = handled.safe_message;
                        handled.result_ref
                    } else {
                        None
                    };
                    let reply = call_reply_from_evaluation(
                        ReplyId::new(format!("reply-iroh-call-{}", state.next_suffix()))
                            .expect("string ID literal/generated value must be non-empty"),
                        &evaluation,
                        reply_result_ref,
                        state.next_observation_id("call-reply"),
                    );
                    let response_bytes =
                        encode_call_protocol_reply_json(&reply).map_err(|source| {
                            MotherIrohEndpointError::ProtocolKernel {
                                action: "encode mct/call/0 response",
                                source,
                            }
                        })?;
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
            let response = recv.read_to_end(64 * 1024).await.map_err(|source| {
                MotherIrohEndpointError::ProtocolIo {
                    action: "read response stream",
                    source: boxed_source(source),
                }
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

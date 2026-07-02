use iroh::{
    Endpoint, EndpointAddr, RelayMode, RelayUrl, SecretKey, TransportAddr,
    endpoint::{BindError, presets},
};
use mct_kernel::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    error::Error as StdError,
    fs::OpenOptions,
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};
use thiserror::Error;

const SERVE_CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);
const ROUNDTRIP_CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

pub type MotherIrohEndpointResult<T> = std::result::Result<T, MotherIrohEndpointError>;

fn boxed_source(
    source: impl StdError + Send + Sync + 'static,
) -> Box<dyn StdError + Send + Sync + 'static> {
    Box::new(source)
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MotherIrohEndpointError {
    #[error("Mother Iroh endpoint config must accept at least one ALPN")]
    EmptyAcceptedAlpns,

    #[error("invalid Mother Iroh secret key hex: {reason}")]
    InvalidSecretKey { reason: String },

    #[error("Mother Iroh identity file error at {path}: {source}")]
    IdentityFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid Mother Iroh endpoint id '{value}': {source}")]
    InvalidEndpointId {
        value: String,
        #[source]
        source: Box<dyn StdError + Send + Sync + 'static>,
    },

    #[error("invalid Mother Iroh direct address '{value}': {source}")]
    InvalidDirectAddress {
        value: String,
        #[source]
        source: Box<dyn StdError + Send + Sync + 'static>,
    },

    #[error("invalid Mother Iroh relay URL '{value}': {source}")]
    InvalidRelayUrl {
        value: String,
        #[source]
        source: Box<dyn StdError + Send + Sync + 'static>,
    },

    #[error("bind Mother-owned Iroh endpoint")]
    Bind {
        #[source]
        source: BindError,
    },

    #[error("Mother-owned Iroh endpoint is closed")]
    EndpointClosed,

    #[error("Mother Iroh protocol {action} failed: {source}")]
    ProtocolIo {
        action: &'static str,
        #[source]
        source: Box<dyn StdError + Send + Sync + 'static>,
    },

    #[error("Mother Iroh protocol {action} JSON failed: {source}")]
    ProtocolJson {
        action: &'static str,
        #[source]
        source: serde_json::Error,
    },

    #[error("Mother Iroh protocol {action} kernel validation failed: {source}")]
    ProtocolKernel {
        action: &'static str,
        #[source]
        source: MctKernelError,
    },

    #[error("Mother Iroh protocol {action} timed out")]
    ProtocolTimeout { action: &'static str },

    #[error("unsupported MCT ALPN '{alpn}'")]
    UnsupportedAlpn { alpn: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotherIrohEndpointLifecycle {
    Bound,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotherIrohRelayMode {
    Disabled,
    Default,
}

impl MotherIrohRelayMode {
    fn into_iroh(self) -> RelayMode {
        match self {
            Self::Disabled => RelayMode::Disabled,
            Self::Default => RelayMode::Default,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MotherIrohEndpointConfig {
    pub accepted_alpns: Vec<String>,
    pub relay_mode: MotherIrohRelayMode,
    pub secret_key_hex: Option<String>,
}

impl MotherIrohEndpointConfig {
    pub fn local_mct() -> Self {
        Self {
            accepted_alpns: mct_alpns(),
            relay_mode: MotherIrohRelayMode::Disabled,
            secret_key_hex: None,
        }
    }

    pub fn with_relay_mode(mut self, relay_mode: MotherIrohRelayMode) -> Self {
        self.relay_mode = relay_mode;
        self
    }

    pub fn with_secret_key_hex(mut self, secret_key_hex: impl Into<String>) -> Self {
        self.secret_key_hex = Some(secret_key_hex.into());
        self
    }
}

impl Default for MotherIrohEndpointConfig {
    fn default() -> Self {
        Self::local_mct()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotherIrohEndpointSnapshot {
    pub endpoint_id: EndpointIdText,
    pub lifecycle: MotherIrohEndpointLifecycle,
    pub accepted_alpns: Vec<String>,
    pub direct_addresses: Vec<String>,
    pub relay_urls: Vec<String>,
    pub relay_mode: MotherIrohRelayMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotherIrohEndpointTicket {
    pub endpoint_id: EndpointIdText,
    pub direct_addresses: Vec<String>,
    pub relay_urls: Vec<String>,
}

impl MotherIrohEndpointTicket {
    pub fn from_snapshot(snapshot: &MotherIrohEndpointSnapshot) -> Self {
        Self {
            endpoint_id: snapshot.endpoint_id.clone(),
            direct_addresses: snapshot.direct_addresses.clone(),
            relay_urls: snapshot.relay_urls.clone(),
        }
    }

    pub fn to_json(&self) -> MotherIrohEndpointResult<String> {
        serde_json::to_string_pretty(self).map_err(|source| MotherIrohEndpointError::ProtocolJson {
            action: "encode endpoint ticket",
            source,
        })
    }

    pub fn from_json(json: &str) -> MotherIrohEndpointResult<Self> {
        serde_json::from_str(json).map_err(|source| MotherIrohEndpointError::ProtocolJson {
            action: "decode endpoint ticket",
            source,
        })
    }
}

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

/// Mother-owned Iroh endpoint lifecycle wrapper.
///
/// The raw Iroh endpoint remains private to the adapter. Public callers receive
/// transport facts only, not authority and not child-usable handles.
pub struct MotherIrohEndpoint {
    endpoint: Option<Endpoint>,
    snapshot: MotherIrohEndpointSnapshot,
}

impl MotherIrohEndpoint {
    /// Bind a local relay-disabled endpoint that accepts MCT peer ALPNs.
    pub async fn bind_local_mct() -> MotherIrohEndpointResult<Self> {
        Self::bind(MotherIrohEndpointConfig::local_mct()).await
    }

    /// Bind a Mother-owned endpoint from explicit adapter configuration.
    pub async fn bind(config: MotherIrohEndpointConfig) -> MotherIrohEndpointResult<Self> {
        if config.accepted_alpns.is_empty() {
            return Err(MotherIrohEndpointError::EmptyAcceptedAlpns);
        }

        let accepted_alpns = config.accepted_alpns;
        let relay_mode = config.relay_mode;
        let mut builder = Endpoint::builder(presets::N0)
            .relay_mode(relay_mode.into_iroh())
            .alpns(alpn_bytes(&accepted_alpns));
        if let Some(secret_key_hex) = config.secret_key_hex {
            builder = builder.secret_key(secret_key_from_hex(&secret_key_hex)?);
        }
        let endpoint = builder
            .bind()
            .await
            .map_err(|source| MotherIrohEndpointError::Bind { source })?;
        let endpoint_addr = endpoint.addr();
        let snapshot = MotherIrohEndpointSnapshot {
            endpoint_id: EndpointIdText::new(endpoint.id().to_string())
                .expect("string ID literal/generated value must be non-empty"),
            lifecycle: MotherIrohEndpointLifecycle::Bound,
            accepted_alpns,
            direct_addresses: endpoint_addr
                .ip_addrs()
                .map(|addr| addr.to_string())
                .collect(),
            relay_urls: endpoint_addr
                .relay_urls()
                .map(|url| url.to_string())
                .collect(),
            relay_mode,
        };

        Ok(Self {
            endpoint: Some(endpoint),
            snapshot,
        })
    }

    pub fn snapshot(&self) -> MotherIrohEndpointSnapshot {
        self.snapshot.clone()
    }

    pub fn ticket(&self) -> MotherIrohEndpointTicket {
        MotherIrohEndpointTicket::from_snapshot(&self.snapshot)
    }

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

    pub async fn close(&mut self) {
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close().await;
        }
        self.snapshot.lifecycle = MotherIrohEndpointLifecycle::Closed;
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

pub fn load_or_create_node_secret_key_hex(
    path: impl AsRef<Path>,
) -> MotherIrohEndpointResult<String> {
    let path = path.as_ref();
    if path.exists() {
        let content = std::fs::read_to_string(path).map_err(|source| {
            MotherIrohEndpointError::IdentityFile {
                path: path.to_path_buf(),
                source,
            }
        })?;
        let secret_key_hex = content.trim().to_string();
        secret_key_from_hex(&secret_key_hex)?;
        return Ok(secret_key_hex);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| {
            MotherIrohEndpointError::IdentityFile {
                path: parent.to_path_buf(),
                source,
            }
        })?;
    }
    let secret_key_hex = secret_key_to_hex(&SecretKey::generate());
    write_new_node_secret_key_file(path, &secret_key_hex)?;
    Ok(secret_key_hex)
}

fn write_new_node_secret_key_file(
    path: &Path,
    secret_key_hex: &str,
) -> MotherIrohEndpointResult<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options
        .open(path)
        .map_err(|source| MotherIrohEndpointError::IdentityFile {
            path: path.to_path_buf(),
            source,
        })?;
    writeln!(file, "{secret_key_hex}").map_err(|source| MotherIrohEndpointError::IdentityFile {
        path: path.to_path_buf(),
        source,
    })
}

pub fn endpoint_id_for_secret_key_hex(
    secret_key_hex: &str,
) -> MotherIrohEndpointResult<EndpointIdText> {
    Ok(
        EndpointIdText::new(secret_key_from_hex(secret_key_hex)?.public().to_string())
            .expect("string ID literal/generated value must be non-empty"),
    )
}

pub(crate) fn mct_alpns() -> Vec<String> {
    vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()]
}

#[cfg(test)]
pub(crate) fn mct_alpn_bytes() -> Vec<Vec<u8>> {
    alpn_bytes(&mct_alpns())
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

pub(crate) fn endpoint_addr_from_ticket(
    ticket: &MotherIrohEndpointTicket,
) -> MotherIrohEndpointResult<EndpointAddr> {
    let endpoint_id =
        iroh::EndpointId::from_str(ticket.endpoint_id.as_str()).map_err(|source| {
            MotherIrohEndpointError::InvalidEndpointId {
                value: ticket.endpoint_id.as_str().to_string(),
                source: boxed_source(source),
            }
        })?;
    let mut addrs = Vec::new();
    for value in &ticket.direct_addresses {
        let addr = value.parse::<SocketAddr>().map_err(|source| {
            MotherIrohEndpointError::InvalidDirectAddress {
                value: value.clone(),
                source: boxed_source(source),
            }
        })?;
        addrs.push(TransportAddr::Ip(addr));
    }
    for value in &ticket.relay_urls {
        let relay = RelayUrl::from_str(value).map_err(|source| {
            MotherIrohEndpointError::InvalidRelayUrl {
                value: value.clone(),
                source: boxed_source(source),
            }
        })?;
        addrs.push(TransportAddr::Relay(relay));
    }
    Ok(EndpointAddr::from_parts(endpoint_id, addrs))
}

fn alpn_bytes(alpns: &[String]) -> Vec<Vec<u8>> {
    alpns.iter().map(|alpn| alpn.as_bytes().to_vec()).collect()
}

fn secret_key_from_hex(secret_key_hex: &str) -> MotherIrohEndpointResult<SecretKey> {
    let bytes = decode_32_hex(secret_key_hex.trim())?;
    Ok(SecretKey::from_bytes(&bytes))
}

fn secret_key_to_hex(secret_key: &SecretKey) -> String {
    encode_hex(&secret_key.to_bytes())
}

fn decode_32_hex(value: &str) -> MotherIrohEndpointResult<[u8; 32]> {
    if value.len() != 64 {
        return Err(MotherIrohEndpointError::InvalidSecretKey {
            reason: format!("expected 64 lowercase hex characters, got {}", value.len()),
        });
    }
    let mut bytes = [0_u8; 32];
    for (index, chunk) in value.as_bytes().chunks(2).enumerate() {
        let high = decode_hex_nibble(chunk[0])?;
        let low = decode_hex_nibble(chunk[1])?;
        bytes[index] = (high << 4) | low;
    }
    Ok(bytes)
}

fn decode_hex_nibble(byte: u8) -> MotherIrohEndpointResult<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(MotherIrohEndpointError::InvalidSecretKey {
            reason: "secret key must be lowercase hex".into(),
        }),
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

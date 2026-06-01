//! Iroh adapter boundary for MCT peer protocols.
//!
//! This crate owns Mother-owned Iroh endpoint lifecycle and MCT ALPN protocol
//! effects. It translates Iroh facts into `mct-kernel` domain records rather
//! than making Iroh transport identity into MCT authority.

#![forbid(unsafe_code)]

use anyhow::{Context, Result, anyhow};
use iroh::{Endpoint, RelayMode, endpoint::presets};
use mct_kernel::*;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotherIrohEndpointLifecycle {
    Bound,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotherIrohRelayMode {
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MotherIrohEndpointSnapshot {
    pub endpoint_id: EndpointIdText,
    pub lifecycle: MotherIrohEndpointLifecycle,
    pub accepted_alpns: Vec<String>,
    pub direct_addresses: Vec<String>,
    pub relay_urls: Vec<String>,
    pub relay_mode: MotherIrohRelayMode,
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
    pub async fn bind_local_mct() -> Result<Self> {
        let endpoint = Endpoint::builder(presets::N0)
            .relay_mode(RelayMode::Disabled)
            .alpns(mct_alpn_bytes())
            .bind()
            .await
            .context("bind Mother-owned local Iroh endpoint")?;
        let endpoint_addr = endpoint.addr();
        let snapshot = MotherIrohEndpointSnapshot {
            endpoint_id: EndpointIdText::from(endpoint.id().to_string()),
            lifecycle: MotherIrohEndpointLifecycle::Bound,
            accepted_alpns: mct_alpns(),
            direct_addresses: endpoint_addr
                .ip_addrs()
                .map(|addr| addr.to_string())
                .collect(),
            relay_urls: endpoint_addr
                .relay_urls()
                .map(|url| url.to_string())
                .collect(),
            relay_mode: MotherIrohRelayMode::Disabled,
        };

        Ok(Self {
            endpoint: Some(endpoint),
            snapshot,
        })
    }

    pub fn snapshot(&self) -> MotherIrohEndpointSnapshot {
        self.snapshot.clone()
    }

    pub async fn close(&mut self) {
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close().await;
        }
        self.snapshot.lifecycle = MotherIrohEndpointLifecycle::Closed;
    }
}

pub struct LocalIrohEchoReport {
    pub hello_response: MctHelloResponse,
    pub call_reply: MctCallProtocolReply,
}

pub struct LocalIrohDeniedPeerReport {
    pub hello_response: MctHelloResponse,
    pub hello_evaluation: MctHelloAdmissionEvaluation,
    pub call_reply: MctCallProtocolReply,
    pub call_evaluation: MctCallProtocolEvaluation,
}

/// Build canonical MCT observations for a local denied-peer adapter proof.
///
/// This is a projection from safe adapter facts into `MctObservation`; logs,
/// metrics, qlog, and OTel can later project from the same facts.
pub fn local_denied_peer_adapter_observations(
    bound_endpoint: &MotherIrohEndpointSnapshot,
    closed_endpoint: &MotherIrohEndpointSnapshot,
    report: &LocalIrohDeniedPeerReport,
    trace_id: TraceId,
) -> Vec<MctObservation> {
    vec![
        adapter_observation(
            "obs-iroh-endpoint-bound",
            ObservationKind::AdapterEffectStarted,
            trace_id.clone(),
            None,
            None,
            ObservationOutcome::Started,
            "iroh endpoint bound",
            Some(bound_endpoint.endpoint_id.as_str().to_string()),
            Some("mct-iroh-endpoint".into()),
        ),
        adapter_observation(
            "obs-iroh-hello-received",
            ObservationKind::PeerHelloReceived,
            trace_id.clone(),
            None,
            None,
            ObservationOutcome::Started,
            "peer hello received",
            Some(bound_endpoint.endpoint_id.as_str().to_string()),
            Some(MCT_HELLO_ALPN.into()),
        ),
        adapter_observation(
            "obs-iroh-peer-rejected",
            ObservationKind::PeerRejected,
            trace_id.clone(),
            None,
            Some(report.hello_evaluation.decision_id.clone()),
            ObservationOutcome::Denied,
            report.hello_response.safe_message.clone(),
            report
                .hello_evaluation
                .selected_binding_id
                .as_ref()
                .map(ToString::to_string),
            Some(MCT_HELLO_ALPN.into()),
        ),
        adapter_observation(
            "obs-iroh-peer-call-received",
            ObservationKind::PeerCallReceived,
            trace_id.clone(),
            report.call_evaluation.call_id.clone(),
            None,
            ObservationOutcome::Started,
            "peer call received",
            None,
            Some(MCT_CALL_ALPN.into()),
        ),
        adapter_observation(
            "obs-iroh-peer-call-replied",
            ObservationKind::PeerCallReplied,
            trace_id.clone(),
            report.call_evaluation.call_id.clone(),
            Some(report.call_evaluation.decision_id.clone()),
            ObservationOutcome::Denied,
            report.call_reply.safe_message.clone(),
            None,
            Some(MCT_CALL_ALPN.into()),
        ),
        adapter_observation(
            "obs-iroh-endpoint-closed",
            ObservationKind::AdapterEffectCompleted,
            trace_id,
            None,
            None,
            ObservationOutcome::Completed,
            "iroh endpoint closed",
            Some(closed_endpoint.endpoint_id.as_str().to_string()),
            Some("mct-iroh-endpoint".into()),
        ),
    ]
}

fn adapter_observation(
    observation_id: &str,
    kind: ObservationKind,
    trace_id: TraceId,
    call_id: Option<CallId>,
    decision_id: Option<DecisionId>,
    outcome: ObservationOutcome,
    safe_message: impl Into<String>,
    subject_id: Option<String>,
    resource_id: Option<String>,
) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::from(observation_id),
        observed_at: Timestamp::from("2026-05-31T00:00:00Z"),
        kind,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id,
        decision_id,
        subject_id,
        resource_id,
        policy_revision: Some(1),
        grants_revision: Some(1),
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: None,
    }
}

#[derive(Clone, Debug, Default)]
struct LocalProtocolState {
    last_hello: Option<MctHelloAdmissionEvaluation>,
    last_call: Option<MctCallProtocolEvaluation>,
}

/// Run a local, relay-disabled Iroh roundtrip for `mct/hello/0` then `mct/call/0`.
///
/// This is intentionally a tiny adapter proof, not the production daemon protocol loop.
pub async fn run_local_iroh_echo_roundtrip() -> Result<LocalIrohEchoReport> {
    let server = Endpoint::builder(presets::N0)
        .relay_mode(RelayMode::Disabled)
        .alpns(mct_alpn_bytes())
        .bind()
        .await
        .context("bind server Iroh endpoint")?;
    let server_addr = server.addr();

    let client = Endpoint::builder(presets::N0)
        .relay_mode(RelayMode::Disabled)
        .alpns(mct_alpn_bytes())
        .bind()
        .await
        .context("bind client Iroh endpoint")?;
    let client_endpoint_id = EndpointIdText::from(client.id().to_string());
    let binding = local_binding_for(&client_endpoint_id);
    let state = Arc::new(Mutex::new(LocalProtocolState::default()));

    let server_task = tokio::spawn(serve_two_local_connections(
        server.clone(),
        vec![binding],
        state.clone(),
    ));

    let trace_id = TraceId::from("trace-local-iroh-echo");
    let hello_request = local_hello_request(&client_endpoint_id, &trace_id);
    let hello_response: MctHelloResponse =
        roundtrip_json(&client, server_addr.clone(), MCT_HELLO_ALPN, &hello_request)
            .await
            .context("complete mct/hello/0 roundtrip")?;
    if hello_response.hello_outcome != HelloOutcome::Admitted {
        anyhow::bail!(
            "local Iroh hello was denied: {}",
            hello_response.safe_message
        );
    }

    let call_request = local_call_request(&client_endpoint_id, &trace_id, &hello_response);
    let call_reply: MctCallProtocolReply =
        roundtrip_json(&client, server_addr, MCT_CALL_ALPN, &call_request)
            .await
            .context("complete mct/call/0 roundtrip")?;

    server.close().await;
    client.close().await;
    server_task.await.context("join local Iroh server task")??;

    Ok(LocalIrohEchoReport {
        hello_response,
        call_reply,
    })
}

/// Run a local Iroh roundtrip where transport connectivity succeeds but MCT
/// authority denies the peer because no active `MctPeerBinding` exists.
pub async fn run_unknown_peer_denial_roundtrip() -> Result<LocalIrohDeniedPeerReport> {
    let server = Endpoint::builder(presets::N0)
        .relay_mode(RelayMode::Disabled)
        .alpns(mct_alpn_bytes())
        .bind()
        .await
        .context("bind server Iroh endpoint")?;
    let server_addr = server.addr();

    let client = Endpoint::builder(presets::N0)
        .relay_mode(RelayMode::Disabled)
        .alpns(mct_alpn_bytes())
        .bind()
        .await
        .context("bind client Iroh endpoint")?;
    let client_endpoint_id = EndpointIdText::from(client.id().to_string());
    let state = Arc::new(Mutex::new(LocalProtocolState::default()));

    let server_task = tokio::spawn(serve_two_local_connections(
        server.clone(),
        Vec::new(),
        state.clone(),
    ));

    let trace_id = TraceId::from("trace-local-iroh-unknown-peer");
    let hello_request = local_hello_request(&client_endpoint_id, &trace_id);
    let hello_response: MctHelloResponse =
        roundtrip_json(&client, server_addr.clone(), MCT_HELLO_ALPN, &hello_request)
            .await
            .context("complete denied mct/hello/0 roundtrip")?;

    let call_request = local_call_request(&client_endpoint_id, &trace_id, &hello_response);
    let call_reply: MctCallProtocolReply =
        roundtrip_json(&client, server_addr, MCT_CALL_ALPN, &call_request)
            .await
            .context("complete denied mct/call/0 roundtrip")?;

    server.close().await;
    client.close().await;
    server_task.await.context("join local Iroh server task")??;

    let state = state.lock().await;
    Ok(LocalIrohDeniedPeerReport {
        hello_response,
        hello_evaluation: state
            .last_hello
            .clone()
            .ok_or_else(|| anyhow!("missing server-side hello evaluation"))?,
        call_reply,
        call_evaluation: state
            .last_call
            .clone()
            .ok_or_else(|| anyhow!("missing server-side call evaluation"))?,
    })
}

async fn serve_two_local_connections(
    endpoint: Endpoint,
    bindings: Vec<MctPeerBinding>,
    state: Arc<Mutex<LocalProtocolState>>,
) -> Result<()> {
    for _ in 0..2 {
        let Some(incoming) = endpoint.accept().await else {
            return Ok(());
        };
        let mut accepting = incoming
            .accept()
            .context("accept incoming Iroh connection")?;
        let alpn = accepting.alpn().await.context("read incoming ALPN")?;
        let connection = accepting
            .await
            .context("finish Iroh connection acceptance")?;
        let (mut send, mut recv) = connection
            .accept_bi()
            .await
            .context("accept bidirectional stream")?;
        let request_bytes = recv
            .read_to_end(64 * 1024)
            .await
            .context("read request stream")?;

        let response_bytes = match alpn.as_slice() {
            bytes if bytes == MCT_HELLO_ALPN.as_bytes() => {
                let request: MctHelloRequest =
                    serde_json::from_slice(&request_bytes).context("decode mct/hello/0 request")?;
                let evaluation = evaluate_hello(
                    &request,
                    &bindings,
                    &HelloPolicy::default(),
                    EvaluationIds {
                        decision_id: DecisionId::from("decision-iroh-hello"),
                        observation_id: ObservationId::from("obs-iroh-hello-decision"),
                    },
                );
                state.lock().await.last_hello = Some(evaluation.clone());
                serde_json::to_vec(&hello_response(
                    "reply-iroh-hello",
                    &evaluation,
                    ObservationId::from("obs-iroh-hello-reply"),
                ))
                .context("encode mct/hello/0 response")?
            }
            bytes if bytes == MCT_CALL_ALPN.as_bytes() => {
                let request: MctCallProtocolRequest =
                    serde_json::from_slice(&request_bytes).context("decode mct/call/0 request")?;
                let hello = state
                    .lock()
                    .await
                    .last_hello
                    .clone()
                    .ok_or_else(|| anyhow!("mct/call/0 received before admitted hello"))?;
                let evaluation = evaluate_call_protocol(
                    &request,
                    &hello,
                    CallEvaluationIds {
                        decision_id: DecisionId::from("decision-iroh-call"),
                        observation_id: ObservationId::from("obs-iroh-call-decision"),
                    },
                );
                state.lock().await.last_call = Some(evaluation.clone());
                let result_ref = evaluation
                    .is_accepted_for_routing()
                    .then(|| ResultRef::from("result-iroh-echo"));
                serde_json::to_vec(&call_reply_from_evaluation(
                    ReplyId::from("reply-iroh-call"),
                    &evaluation,
                    result_ref,
                    ObservationId::from("obs-iroh-call-reply"),
                ))
                .context("encode mct/call/0 response")?
            }
            other => anyhow::bail!(
                "unsupported local Iroh ALPN: {}",
                String::from_utf8_lossy(other)
            ),
        };

        send.write_all(&response_bytes)
            .await
            .context("write response stream")?;
        send.finish().context("finish response stream")?;
        connection.closed().await;
    }
    Ok(())
}

fn mct_alpns() -> Vec<String> {
    vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()]
}

fn mct_alpn_bytes() -> Vec<Vec<u8>> {
    vec![
        MCT_HELLO_ALPN.as_bytes().to_vec(),
        MCT_CALL_ALPN.as_bytes().to_vec(),
    ]
}

async fn roundtrip_json<Request, Response>(
    endpoint: &Endpoint,
    server_addr: iroh::EndpointAddr,
    alpn: &str,
    request: &Request,
) -> Result<Response>
where
    Request: serde::Serialize,
    Response: serde::de::DeserializeOwned,
{
    let connection = endpoint
        .connect(server_addr, alpn.as_bytes())
        .await
        .with_context(|| format!("connect over {alpn}"))?;
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .context("open bidirectional stream")?;
    let bytes = serde_json::to_vec(request).context("encode request")?;
    send.write_all(&bytes)
        .await
        .context("write request stream")?;
    send.finish().context("finish request stream")?;
    let response = recv
        .read_to_end(64 * 1024)
        .await
        .context("read response stream")?;
    connection.close(0u32.into(), b"mct client complete");
    serde_json::from_slice(&response).context("decode response")
}

fn local_binding_for(endpoint_id: &EndpointIdText) -> MctPeerBinding {
    MctPeerBinding {
        binding_id: PeerBindingId::from("binding-local-iroh"),
        iroh_endpoint_id: endpoint_id.clone(),
        scope: MctPeerBindingScope {
            mct_node_id: MctNodeId::from("mother-client"),
            vision_id: VisionId::from("vision-local"),
            allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            data_scope: None,
            observation_scope: None,
        },
        issuer_node_id: MctNodeId::from("mother-server"),
        policy_revision: 1,
        binding_state: BindingState::Admitted,
        issued_at: Timestamp::from("2026-05-31T00:00:00Z"),
        expires_at: None,
        created_by_observation_id: ObservationId::from("obs-binding-local-iroh"),
        superseded_by_observation_id: None,
    }
}

fn local_hello_request(endpoint_id: &EndpointIdText, trace_id: &TraceId) -> MctHelloRequest {
    MctHelloRequest {
        hello_id: "hello-local-iroh".into(),
        received_over: IrohConnectionPresentation {
            endpoint_id: endpoint_id.clone(),
            alpn: MCT_HELLO_ALPN.into(),
            connection_side: ConnectionSide::Incoming,
            path_class: PathClass::Direct,
            relay_url: None,
            presented_capability_ref: None,
        },
        requested_protocol: HelloPolicy::default().protocol,
        requested_vision_id: Some(VisionId::from("vision-local")),
        requested_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
        presented_binding: MctPeerBindingPresentation {
            binding_id: Some(PeerBindingId::from("binding-local-iroh")),
            endpoint_id: endpoint_id.clone(),
            mct_node_id: Some(MctNodeId::from("mother-client")),
            vision_id: Some(VisionId::from("vision-local")),
            policy_revision: Some(1),
            allowed_alpns_claim: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            signature_ref: None,
            expires_at: None,
        },
        capability_view: None,
        local_policy_revision_seen: Some(1),
        trace_id: trace_id.clone(),
        received_observation_id: ObservationId::from("obs-local-hello-received"),
    }
}

fn local_call_request(
    endpoint_id: &EndpointIdText,
    trace_id: &TraceId,
    hello: &MctHelloResponse,
) -> MctCallProtocolRequest {
    let call = MctCall {
        call_id: CallId::from("call-local-iroh-echo"),
        caller: CallerIdentity {
            node_id: MctNodeId::from("mother-client"),
            user_id: None,
            vision_id: VisionId::from("vision-local"),
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
        deadline: Timestamp::from("2026-05-31T00:01:00Z"),
        trace_context: TraceContext {
            trace_id: trace_id.clone(),
            span_id: SpanId::from("span-local-call"),
        },
        origin: CallOrigin::Iroh,
    };

    MctCallProtocolRequest {
        protocol_request_id: ProtocolRequestId::from("proto-local-call"),
        authority: MctCallProtocolAuthority {
            hello_decision_id: hello.decision_id.clone(),
            peer_binding_id: PeerBindingId::from("binding-local-iroh"),
            vision_id: VisionId::from("vision-local"),
            accepted_alpn: MCT_CALL_ALPN.into(),
            endpoint_id: endpoint_id.clone(),
            policy_revision: 1,
            grants_revision: 1,
        },
        received_over: IrohConnectionPresentation {
            endpoint_id: endpoint_id.clone(),
            alpn: MCT_CALL_ALPN.into(),
            connection_side: ConnectionSide::Incoming,
            path_class: PathClass::Direct,
            relay_url: None,
            presented_capability_ref: None,
        },
        call,
        payload: MctCallPayloadHandle {
            payload_kind: PayloadKind::InlinePayload,
            content_type: Some("text/plain".into()),
            approximate_size_bytes: 5,
            digest: None,
            blob_ref: None,
            external_ref: None,
            inline_payload_ref: Some("payload-local-echo".into()),
        },
        idempotency_key: Some("idem-local-call".into()),
        received_observation_id: ObservationId::from("obs-local-call-received"),
    }
}

/// Returns the crate version for health and smoke tests.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_version() {
        assert_eq!(super::version(), "0.1.0");
    }

    #[tokio::test]
    async fn mother_owned_endpoint_starts_and_closes() {
        let mut endpoint = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let bound = endpoint.snapshot();

        assert_eq!(bound.lifecycle, MotherIrohEndpointLifecycle::Bound);
        assert_eq!(bound.relay_mode, MotherIrohRelayMode::Disabled);
        assert!(!bound.endpoint_id.as_str().is_empty());
        assert_eq!(bound.accepted_alpns, mct_alpns());
        assert!(bound.relay_urls.is_empty());

        endpoint.close().await;
        let closed = endpoint.snapshot();
        assert_eq!(closed.lifecycle, MotherIrohEndpointLifecycle::Closed);
        assert_eq!(closed.endpoint_id, bound.endpoint_id);
        assert_eq!(closed.accepted_alpns, bound.accepted_alpns);

        endpoint.close().await;
        assert_eq!(
            endpoint.snapshot().lifecycle,
            MotherIrohEndpointLifecycle::Closed
        );
    }

    #[tokio::test]
    async fn local_iroh_completes_mct_hello_then_call() {
        let report = run_local_iroh_echo_roundtrip().await.unwrap();
        assert_eq!(report.hello_response.hello_outcome, HelloOutcome::Admitted);
        assert_eq!(
            report.call_reply.reply_outcome,
            CallProtocolReplyOutcome::Success
        );
        assert_eq!(
            report.call_reply.result_ref,
            Some(ResultRef::from("result-iroh-echo"))
        );
    }

    #[tokio::test]
    async fn iroh_adapter_observations_cover_endpoint_and_protocol_events() {
        let mut endpoint = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let bound = endpoint.snapshot();
        endpoint.close().await;
        let closed = endpoint.snapshot();
        let report = run_unknown_peer_denial_roundtrip().await.unwrap();

        let observations = local_denied_peer_adapter_observations(
            &bound,
            &closed,
            &report,
            TraceId::from("trace-obs"),
        );

        let kinds = observations
            .iter()
            .map(|observation| observation.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                ObservationKind::AdapterEffectStarted,
                ObservationKind::PeerHelloReceived,
                ObservationKind::PeerRejected,
                ObservationKind::PeerCallReceived,
                ObservationKind::PeerCallReplied,
                ObservationKind::AdapterEffectCompleted,
            ]
        );
        assert!(
            observations
                .iter()
                .all(|observation| observation.source_plane == SourcePlane::Adapter)
        );
        assert_eq!(
            observations[2].decision_id,
            Some(report.hello_evaluation.decision_id)
        );
        assert_eq!(observations[3].call_id, report.call_evaluation.call_id);
        assert_eq!(
            observations[4].decision_id,
            Some(report.call_evaluation.decision_id)
        );
        assert_eq!(observations[4].outcome, ObservationOutcome::Denied);
        assert_eq!(
            observations[0].subject_id,
            Some(bound.endpoint_id.to_string())
        );
        assert_eq!(
            observations[5].subject_id,
            Some(closed.endpoint_id.to_string())
        );
    }

    #[tokio::test]
    async fn unknown_peer_is_denied_before_call() {
        let report = run_unknown_peer_denial_roundtrip().await.unwrap();

        assert_eq!(report.hello_response.hello_outcome, HelloOutcome::Denied);
        assert_eq!(report.hello_response.safe_message, "not authorized");
        assert_eq!(report.hello_evaluation.reason, HelloReason::MissingBinding);
        assert_eq!(report.hello_evaluation.selected_binding_id, None);
        assert_eq!(
            report.hello_evaluation.observation_id,
            ObservationId::from("obs-iroh-hello-decision")
        );

        assert_eq!(
            report.call_reply.reply_outcome,
            CallProtocolReplyOutcome::Denied
        );
        assert_eq!(report.call_reply.result_ref, None);
        assert_eq!(report.call_evaluation.outcome, CallProtocolOutcome::Denied);
        assert_eq!(
            report.call_evaluation.reason,
            CallProtocolReason::HelloNotAdmitted
        );
        assert_eq!(report.call_evaluation.route_decision_id, None);
        assert_eq!(
            report.call_evaluation.observation_id,
            ObservationId::from("obs-iroh-call-decision")
        );
    }
}

use crate::endpoint::mct_alpn_bytes;
use anyhow::{Context, Result, anyhow};
use iroh::{Endpoint, RelayMode, endpoint::presets};
use mct_kernel::*;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(crate) struct LocalIrohEchoReport {
    pub(crate) hello_response: MctHelloResponse,
    pub(crate) call_reply: MctCallProtocolReply,
}

pub(crate) struct LocalIrohDeniedPeerReport {
    pub(crate) hello_response: MctHelloResponse,
    pub(crate) hello_evaluation: MctHelloAdmissionEvaluation,
    pub(crate) call_reply: MctCallProtocolReply,
    pub(crate) call_evaluation: MctCallProtocolEvaluation,
}

#[derive(Clone, Debug, Default)]
struct LocalProtocolState {
    last_hello: Option<MctHelloAdmissionEvaluation>,
    last_call: Option<MctCallProtocolEvaluation>,
}

/// Run a local, relay-disabled Iroh roundtrip for `mct/hello/0` then `mct/call/0`.
///
/// This is intentionally a tiny adapter proof, not the production daemon protocol loop.
pub(crate) async fn run_local_iroh_echo_roundtrip() -> Result<LocalIrohEchoReport> {
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
    let client_endpoint_id = EndpointIdText::new(client.id().to_string())
        .expect("string ID literal/generated value must be non-empty");
    let binding = local_binding_for(&client_endpoint_id);
    let state = Arc::new(Mutex::new(LocalProtocolState::default()));

    let server_task = tokio::spawn(serve_two_local_connections(
        server.clone(),
        vec![binding],
        state.clone(),
    ));

    let trace_id = TraceId::new("trace-local-iroh-echo")
        .expect("string ID literal/generated value must be non-empty");
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
pub(crate) async fn run_unknown_peer_denial_roundtrip() -> Result<LocalIrohDeniedPeerReport> {
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
    let client_endpoint_id = EndpointIdText::new(client.id().to_string())
        .expect("string ID literal/generated value must be non-empty");
    let state = Arc::new(Mutex::new(LocalProtocolState::default()));

    let server_task = tokio::spawn(serve_two_local_connections(
        server.clone(),
        Vec::new(),
        state.clone(),
    ));

    let trace_id = TraceId::new("trace-local-iroh-unknown-peer")
        .expect("string ID literal/generated value must be non-empty");
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
                    HelloEvaluationContext {
                        ids: EvaluationIds {
                            decision_id: DecisionId::new("decision-iroh-hello")
                                .expect("string ID literal/generated value must be non-empty"),
                            observation_id: ObservationId::new("obs-iroh-hello-decision")
                                .expect("string ID literal/generated value must be non-empty"),
                        },
                        now: Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
                    },
                );
                state.lock().await.last_hello = Some(evaluation.clone());
                serde_json::to_vec(&hello_response(
                    "reply-iroh-hello",
                    &evaluation,
                    ObservationId::new("obs-iroh-hello-reply")
                        .expect("string ID literal/generated value must be non-empty"),
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
                        decision_id: DecisionId::new("decision-iroh-call")
                            .expect("string ID literal/generated value must be non-empty"),
                        observation_id: ObservationId::new("obs-iroh-call-decision")
                            .expect("string ID literal/generated value must be non-empty"),
                    },
                );
                state.lock().await.last_call = Some(evaluation.clone());
                let result_ref = evaluation.is_accepted_for_routing().then(|| {
                    ResultRef::new("result-iroh-echo")
                        .expect("string ID literal/generated value must be non-empty")
                });
                serde_json::to_vec(&call_reply_from_evaluation(
                    ReplyId::new("reply-iroh-call")
                        .expect("string ID literal/generated value must be non-empty"),
                    &evaluation,
                    result_ref,
                    ObservationId::new("obs-iroh-call-reply")
                        .expect("string ID literal/generated value must be non-empty"),
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
        binding_id: PeerBindingId::new("binding-local-iroh")
            .expect("string ID literal/generated value must be non-empty"),
        iroh_endpoint_id: endpoint_id.clone(),
        scope: MctPeerBindingScope {
            mct_node_id: MctNodeId::new("mother-client")
                .expect("string ID literal/generated value must be non-empty"),
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            data_scope: None,
            observation_scope: None,
        },
        issuer_node_id: MctNodeId::new("mother-server")
            .expect("string ID literal/generated value must be non-empty"),
        policy_revision: 1,
        binding_state: BindingState::Admitted,
        issued_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
        expires_at: None,
        created_by_observation_id: ObservationId::new("obs-binding-local-iroh")
            .expect("string ID literal/generated value must be non-empty"),
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
        requested_vision_id: Some(
            VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
        ),
        requested_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
        presented_binding: MctPeerBindingPresentation {
            binding_id: Some(
                PeerBindingId::new("binding-local-iroh")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            endpoint_id: endpoint_id.clone(),
            mct_node_id: Some(
                MctNodeId::new("mother-client")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            vision_id: Some(
                VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            policy_revision: Some(1),
            allowed_alpns_claim: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            signature_ref: None,
            expires_at: None,
        },
        capability_view: None,
        local_policy_revision_seen: Some(1),
        trace_id: trace_id.clone(),
        received_observation_id: ObservationId::new("obs-local-hello-received")
            .expect("string ID literal/generated value must be non-empty"),
    }
}

fn local_call_request(
    endpoint_id: &EndpointIdText,
    trace_id: &TraceId,
    hello: &MctHelloResponse,
) -> MctCallProtocolRequest {
    let call = MctCall {
        call_id: CallId::new("call-local-iroh-echo")
            .expect("string ID literal/generated value must be non-empty"),
        caller: CallerIdentity {
            node_id: MctNodeId::new("mother-client")
                .expect("string ID literal/generated value must be non-empty"),
            user_id: None,
            vision_id: VisionId::new("vision-local")
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
            approximate_size_bytes: 0,
            contains_secret_scoped_material: false,
        },
        authority_context: AuthorityContextSnapshot {
            policy_revision: 1,
            grants_revision: 1,
            vision_policy_revision: 1,
        },
        deadline: Timestamp::new("2026-05-31T00:01:00Z").unwrap(),
        trace_context: TraceContext {
            trace_id: trace_id.clone(),
            span_id: SpanId::new("span-local-call")
                .expect("string ID literal/generated value must be non-empty"),
        },
        origin: CallOrigin::Iroh,
    };

    MctCallProtocolRequest {
        protocol_request_id: ProtocolRequestId::new("proto-local-call")
            .expect("string ID literal/generated value must be non-empty"),
        authority: MctCallProtocolAuthority {
            hello_decision_id: hello.decision_id.clone(),
            peer_binding_id: PeerBindingId::new("binding-local-iroh")
                .expect("string ID literal/generated value must be non-empty"),
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
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
        payload: MctCallPayloadHandle::Empty,
        idempotency_key: Some("idem-local-call".into()),
        received_observation_id: ObservationId::new("obs-local-call-received")
            .expect("string ID literal/generated value must be non-empty"),
    }
}

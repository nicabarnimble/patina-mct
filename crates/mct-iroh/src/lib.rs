//! Iroh adapter boundary for MCT peer protocols.
//!
//! This crate owns Mother-owned Iroh endpoint lifecycle and MCT ALPN protocol
//! effects. It translates Iroh facts into `mct-kernel` domain records rather
//! than making Iroh transport identity into MCT authority.

#![forbid(unsafe_code)]

use anyhow::{anyhow, Context, Result};
use iroh::{endpoint::presets, Endpoint, RelayMode};
use mct_kernel::*;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct LocalIrohEchoReport {
    pub hello_response: MctHelloResponse,
    pub call_reply: MctCallProtocolReply,
}

/// Run a local, relay-disabled Iroh roundtrip for `mct/hello/0` then `mct/call/0`.
///
/// This is intentionally a tiny adapter proof, not the production daemon protocol loop.
pub async fn run_local_iroh_echo_roundtrip() -> Result<LocalIrohEchoReport> {
    let server = Endpoint::builder(presets::N0)
        .relay_mode(RelayMode::Disabled)
        .alpns(vec![
            MCT_HELLO_ALPN.as_bytes().to_vec(),
            MCT_CALL_ALPN.as_bytes().to_vec(),
        ])
        .bind()
        .await
        .context("bind server Iroh endpoint")?;
    let server_addr = server.addr();

    let client = Endpoint::builder(presets::N0)
        .relay_mode(RelayMode::Disabled)
        .alpns(vec![
            MCT_HELLO_ALPN.as_bytes().to_vec(),
            MCT_CALL_ALPN.as_bytes().to_vec(),
        ])
        .bind()
        .await
        .context("bind client Iroh endpoint")?;
    let client_endpoint_id = EndpointIdText::from(client.id().to_string());
    let binding = local_binding_for(&client_endpoint_id);
    let last_hello = Arc::new(Mutex::new(None::<MctHelloAdmissionEvaluation>));

    let server_task = tokio::spawn(serve_two_local_connections(
        server.clone(),
        binding,
        last_hello.clone(),
    ));

    let trace_id = TraceId::from("trace-local-iroh-echo");
    let hello_request = local_hello_request(&client_endpoint_id, &trace_id);
    let hello_response: MctHelloResponse = roundtrip_json(&client, server_addr.clone(), MCT_HELLO_ALPN, &hello_request)
        .await
        .context("complete mct/hello/0 roundtrip")?;
    if hello_response.hello_outcome != HelloOutcome::Admitted {
        anyhow::bail!("local Iroh hello was denied: {}", hello_response.safe_message);
    }

    let call_request = local_call_request(&client_endpoint_id, &trace_id, &hello_response);
    let call_reply: MctCallProtocolReply = roundtrip_json(&client, server_addr, MCT_CALL_ALPN, &call_request)
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

async fn serve_two_local_connections(
    endpoint: Endpoint,
    binding: MctPeerBinding,
    last_hello: Arc<Mutex<Option<MctHelloAdmissionEvaluation>>>,
) -> Result<()> {
    for _ in 0..2 {
        let Some(incoming) = endpoint.accept().await else {
            return Ok(());
        };
        let mut accepting = incoming.accept().context("accept incoming Iroh connection")?;
        let alpn = accepting.alpn().await.context("read incoming ALPN")?;
        let connection = accepting.await.context("finish Iroh connection acceptance")?;
        let (mut send, mut recv) = connection.accept_bi().await.context("accept bidirectional stream")?;
        let request_bytes = recv.read_to_end(64 * 1024).await.context("read request stream")?;

        let response_bytes = match alpn.as_slice() {
            bytes if bytes == MCT_HELLO_ALPN.as_bytes() => {
                let request: MctHelloRequest = serde_json::from_slice(&request_bytes)
                    .context("decode mct/hello/0 request")?;
                let evaluation = evaluate_hello(
                    &request,
                    std::slice::from_ref(&binding),
                    &HelloPolicy::default(),
                    EvaluationIds {
                        decision_id: DecisionId::from("decision-iroh-hello"),
                        observation_id: ObservationId::from("obs-iroh-hello-decision"),
                    },
                );
                *last_hello.lock().await = Some(evaluation.clone());
                serde_json::to_vec(&hello_response(
                    "reply-iroh-hello",
                    &evaluation,
                    ObservationId::from("obs-iroh-hello-reply"),
                ))
                .context("encode mct/hello/0 response")?
            }
            bytes if bytes == MCT_CALL_ALPN.as_bytes() => {
                let request: MctCallProtocolRequest = serde_json::from_slice(&request_bytes)
                    .context("decode mct/call/0 request")?;
                let hello = last_hello
                    .lock()
                    .await
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
            other => anyhow::bail!("unsupported local Iroh ALPN: {}", String::from_utf8_lossy(other)),
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
    let (mut send, mut recv) = connection.open_bi().await.context("open bidirectional stream")?;
    let bytes = serde_json::to_vec(request).context("encode request")?;
    send.write_all(&bytes).await.context("write request stream")?;
    send.finish().context("finish request stream")?;
    let response = recv.read_to_end(64 * 1024).await.context("read response stream")?;
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
    async fn local_iroh_completes_mct_hello_then_call() {
        let report = run_local_iroh_echo_roundtrip().await.unwrap();
        assert_eq!(report.hello_response.hello_outcome, HelloOutcome::Admitted);
        assert_eq!(report.call_reply.reply_outcome, CallProtocolReplyOutcome::Success);
        assert_eq!(report.call_reply.result_ref, Some(ResultRef::from("result-iroh-echo")));
    }
}

//! Iroh adapter boundary for MCT peer protocols.
//!
//! This crate owns Mother-owned Iroh endpoint lifecycle and MCT ALPN protocol
//! effects. It translates Iroh facts into `mct-kernel` domain records rather
//! than making Iroh transport identity into MCT authority.

#![forbid(unsafe_code)]

mod endpoint;
mod identity;
#[cfg(test)]
mod observation;
mod serve;
#[cfg(test)]
mod test_support;

pub use endpoint::{
    MotherIrohEndpoint, MotherIrohEndpointConfig, MotherIrohEndpointError,
    MotherIrohEndpointLifecycle, MotherIrohEndpointResult, MotherIrohEndpointSnapshot,
    MotherIrohEndpointTicket, MotherIrohRelayMode,
};
pub use identity::{
    MCT_PEER_BINDING_SIGNATURE_PREFIX, MctPeerBindingSignatureVerification,
    endpoint_id_for_secret_key_hex, load_or_create_node_secret_key_hex,
    sign_peer_binding_signature_ref, verify_peer_binding_signature_ref,
};
pub use serve::{
    MCT_CALL_FRAME_READ_BUDGET_BYTES, MCT_INLINE_PAYLOAD_MAX_BYTES,
    MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES, MctIrohCallHandlerResult, MctIrohCallPayloadReply,
    MctIrohConcurrentServeConfig, MctIrohPeerCallReport, MctIrohServeEvent, MctIrohServeState,
    MctIrohServedProtocol,
};

/// Returns the crate version for health and smoke tests.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoint::{mct_alpn_bytes, mct_alpns};
    use crate::observation::local_denied_peer_adapter_observations;
    use crate::serve::endpoint_addr_from_ticket;
    use crate::test_support::{run_local_iroh_echo_roundtrip, run_unknown_peer_denial_roundtrip};
    use iroh::{Endpoint, RelayMode, endpoint::presets};
    use mct_kernel::*;
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicU64, Ordering},
        },
        time::Duration,
    };

    #[test]
    fn exposes_version() {
        assert_eq!(super::version(), "0.1.0");
    }

    #[test]
    fn endpoint_config_defaults_to_local_mct_alpns() {
        let config = MotherIrohEndpointConfig::local_mct();
        assert_eq!(config.accepted_alpns, mct_alpns());
        assert_eq!(config.relay_mode, MotherIrohRelayMode::Disabled);
    }

    #[test]
    fn endpoint_config_can_select_default_relay_mode() {
        let config =
            MotherIrohEndpointConfig::local_mct().with_relay_mode(MotherIrohRelayMode::Default);
        assert_eq!(config.relay_mode, MotherIrohRelayMode::Default);
    }

    #[tokio::test]
    async fn endpoint_config_rejects_empty_alpns() {
        let result = MotherIrohEndpoint::bind(MotherIrohEndpointConfig {
            accepted_alpns: Vec::new(),
            ..MotherIrohEndpointConfig::local_mct()
        })
        .await;

        assert!(matches!(
            result,
            Err(MotherIrohEndpointError::EmptyAcceptedAlpns)
        ));
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

    #[cfg(unix)]
    #[test]
    fn node_secret_key_file_is_created_owner_read_write_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mother.secret");

        let _secret = load_or_create_node_secret_key_hex(&path).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[tokio::test]
    async fn mother_endpoint_ticket_connects_hello_then_call() {
        let mut server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let client_endpoint_id = client.snapshot().endpoint_id;
        let server_ticket = server.ticket();
        let binding = test_peer_binding(&client_endpoint_id);
        let mut state = MctIrohServeState::new();
        let trace_id = TraceId::new("trace-public-iroh-connect")
            .expect("string ID literal/generated value must be non-empty");
        let hello_request = test_hello_request(&client_endpoint_id, &trace_id);

        let (served_hello, hello_response) = tokio::join!(
            server.serve_next(
                &mut state,
                std::slice::from_ref(&binding),
                Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
                None,
            ),
            client.send_hello(&server_ticket, &hello_request),
        );
        let served_hello = served_hello.unwrap();
        let hello_response = hello_response.unwrap();
        assert_eq!(hello_response.hello_outcome, HelloOutcome::Admitted);
        assert!(matches!(
            served_hello,
            MctIrohServedProtocol::Hello { evaluation, .. } if evaluation.is_admitted()
        ));

        let call_request = test_call_request(&client_endpoint_id, &trace_id, &hello_response);
        let (served_call, call_reply) = tokio::join!(
            server.serve_next(
                &mut state,
                std::slice::from_ref(&binding),
                Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                Some(
                    ResultRef::new("result-public-iroh")
                        .expect("string ID literal/generated value must be non-empty")
                ),
            ),
            client.send_call(&server_ticket, &call_request),
        );
        let served_call = served_call.unwrap();
        let call_reply = call_reply.unwrap();
        assert_eq!(call_reply.reply_outcome, CallProtocolReplyOutcome::Success);
        assert_eq!(
            call_reply.result_ref,
            Some(
                ResultRef::new("result-public-iroh")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
        assert!(matches!(
            served_call,
            MctIrohServedProtocol::Call { evaluation, .. }
                if evaluation.is_accepted_for_routing()
        ));

        server.close().await;
        client.close().await;
    }

    #[tokio::test]
    async fn serve_next_denies_binding_expired_against_current_accept_time() {
        let mut server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let client_endpoint_id = client.snapshot().endpoint_id;
        let server_ticket = server.ticket();
        let mut binding = test_peer_binding(&client_endpoint_id);
        binding.expires_at = Some(Timestamp::new("2026-06-01T00:00:00Z").unwrap());
        let mut state = MctIrohServeState::new();
        let trace_id = TraceId::new("trace-expired-binding-iroh")
            .expect("string ID literal/generated value must be non-empty");
        let hello_request = test_hello_request(&client_endpoint_id, &trace_id);

        let (served_hello, hello_response) = tokio::join!(
            server.serve_next(
                &mut state,
                std::slice::from_ref(&binding),
                Timestamp::new("2026-07-02T00:00:00Z").unwrap(),
                None,
            ),
            client.send_hello(&server_ticket, &hello_request),
        );

        let served_hello = served_hello.unwrap();
        let hello_response = hello_response.unwrap();
        assert_eq!(hello_response.hello_outcome, HelloOutcome::Denied);
        assert_eq!(hello_response.safe_message, "not authorized");
        assert!(matches!(
            served_hello,
            MctIrohServedProtocol::Hello { evaluation, .. }
                if evaluation.reason == HelloReason::BindingExpired
        ));

        server.close().await;
        client.close().await;
    }

    #[tokio::test]
    async fn serve_next_times_out_when_peer_never_sends_data() {
        let mut server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let server_ticket = server.ticket();
        let raw_client = Endpoint::builder(presets::N0)
            .relay_mode(RelayMode::Disabled)
            .alpns(mct_alpn_bytes())
            .bind()
            .await
            .unwrap();
        let server_addr = endpoint_addr_from_ticket(&server_ticket).unwrap();
        let mut state = MctIrohServeState::new();

        let raw_client_task = tokio::spawn(async move {
            let connection = raw_client
                .connect(server_addr, MCT_HELLO_ALPN.as_bytes())
                .await
                .unwrap();
            let (_send, _recv) = connection.open_bi().await.unwrap();
            std::future::pending::<()>().await;
        });
        let served = server
            .serve_next_with_call_handler_timeout(
                &mut state,
                &[],
                Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
                Duration::from_secs(2),
                |_, _, _| MctIrohCallHandlerResult::accepted_for_routing(None),
            )
            .await;

        raw_client_task.abort();
        assert!(matches!(
            served,
            Err(MotherIrohEndpointError::ProtocolTimeout {
                action: "serve incoming MCT connection"
            })
        ));
        server.close().await;
    }

    #[tokio::test]
    async fn call_frame_budget_refuses_oversized_request() {
        let mut server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let server_ticket = server.ticket();
        let raw_client = Endpoint::builder(presets::N0)
            .relay_mode(RelayMode::Disabled)
            .alpns(mct_alpn_bytes())
            .bind()
            .await
            .unwrap();
        let server_addr = endpoint_addr_from_ticket(&server_ticket).unwrap();
        let raw_client_task = tokio::spawn(async move {
            let connection = raw_client
                .connect(server_addr, MCT_CALL_ALPN.as_bytes())
                .await
                .unwrap();
            let (mut send, _recv) = connection.open_bi().await.unwrap();
            send.write_all(&vec![b'x'; MCT_CALL_FRAME_READ_BUDGET_BYTES + 1])
                .await
                .unwrap();
            send.finish().unwrap();
            drop(send);
            connection.close(0u32.into(), b"oversized request sent");
        });
        let mut state = MctIrohServeState::new();
        let served = server
            .serve_next_with_call_handler_timeout(
                &mut state,
                &[],
                Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
                Duration::from_secs(2),
                |_, _, _| MctIrohCallHandlerResult::accepted_for_routing(None),
            )
            .await;

        let _ = raw_client_task.await;
        assert!(matches!(
            served,
            Err(MotherIrohEndpointError::ProtocolIo {
                action: "read request stream",
                ..
            }) | Err(MotherIrohEndpointError::ProtocolTimeout {
                action: "serve incoming MCT connection"
            })
        ));
        server.close().await;
    }

    #[tokio::test]
    async fn send_hello_times_out_when_peer_never_replies() {
        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let raw_server = Endpoint::builder(presets::N0)
            .relay_mode(RelayMode::Disabled)
            .alpns(mct_alpn_bytes())
            .bind()
            .await
            .unwrap();
        let raw_server_addr = raw_server.addr();
        let server_ticket = MotherIrohEndpointTicket {
            endpoint_id: EndpointIdText::new(raw_server.id().to_string())
                .expect("string ID literal/generated value must be non-empty"),
            direct_addresses: raw_server_addr
                .ip_addrs()
                .map(|addr| addr.to_string())
                .collect(),
            relay_urls: raw_server_addr
                .relay_urls()
                .map(|url| url.to_string())
                .collect(),
        };
        let client_endpoint_id = client.snapshot().endpoint_id;
        let trace_id = TraceId::new("trace-client-timeout")
            .expect("string ID literal/generated value must be non-empty");
        let hello_request = test_hello_request(&client_endpoint_id, &trace_id);

        let raw_server_task = tokio::spawn(async move {
            let incoming = raw_server.accept().await.unwrap();
            let mut accepting = incoming.accept().unwrap();
            let _alpn = accepting.alpn().await.unwrap();
            let connection = accepting.await.unwrap();
            let (_send, mut recv) = connection.accept_bi().await.unwrap();
            let _request = recv.read_to_end(64 * 1024).await.unwrap();
            std::future::pending::<()>().await;
        });
        let sent = client
            .send_hello_with_timeout(&server_ticket, &hello_request, Duration::from_secs(2))
            .await;

        raw_server_task.abort();
        assert!(matches!(
            sent,
            Err(MotherIrohEndpointError::ProtocolTimeout {
                action: "complete outbound MCT roundtrip"
            })
        ));
        client.close().await;
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
            Some(
                ResultRef::new("result-iroh-echo")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
    }

    #[tokio::test]
    async fn iroh_call_handler_can_complete_reply_after_runtime_execution() {
        let mut server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let client_endpoint_id = client.snapshot().endpoint_id;
        let server_ticket = server.ticket();
        let binding = test_peer_binding(&client_endpoint_id);
        let mut state = MctIrohServeState::new();
        let trace_id = TraceId::new("trace-handler-iroh")
            .expect("string ID literal/generated value must be non-empty");
        let hello_request = test_hello_request(&client_endpoint_id, &trace_id);

        let (_served_hello, hello_response) = tokio::join!(
            server.serve_next(
                &mut state,
                std::slice::from_ref(&binding),
                Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
                None,
            ),
            client.send_hello(&server_ticket, &hello_request),
        );
        let hello_response = hello_response.unwrap();
        let call_request = test_call_request(&client_endpoint_id, &trace_id, &hello_response);
        let (served_call, call_reply) = tokio::join!(
            server.serve_next_with_call_handler(
                &mut state,
                std::slice::from_ref(&binding),
                Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                |_, evaluation, _| {
                    assert!(evaluation.is_accepted_for_routing());
                    MctIrohCallHandlerResult::completed(
                        ResultRef::new("result-runtime-child")
                            .expect("string ID literal/generated value must be non-empty"),
                    )
                }
            ),
            client.send_call(&server_ticket, &call_request),
        );

        let served_call = served_call.unwrap();
        let call_reply = call_reply.unwrap();
        assert_eq!(call_reply.reply_outcome, CallProtocolReplyOutcome::Success);
        assert_eq!(
            call_reply.result_ref,
            Some(
                ResultRef::new("result-runtime-child")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
        assert!(matches!(
            served_call,
            MctIrohServedProtocol::Call { evaluation, .. }
                if evaluation.outcome == CallProtocolOutcome::Completed
        ));

        server.close().await;
        client.close().await;
    }

    #[tokio::test]
    async fn concurrent_serve_keeps_peer_hello_state_separate() {
        let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut first_client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut second_client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let server_ticket = server.ticket();
        let first_endpoint_id = first_client.snapshot().endpoint_id;
        let second_endpoint_id = second_client.snapshot().endpoint_id;
        let first_binding = test_peer_binding(&first_endpoint_id);
        let second_binding = test_peer_binding(&second_endpoint_id);
        let serve_task = tokio::spawn(async move {
            server
                .serve_concurrent_with_call_handler(
                    MctIrohServeState::new(),
                    vec![first_binding, second_binding],
                    MctIrohConcurrentServeConfig::default(),
                    || Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                    |_, _, _| async {
                        MctIrohCallHandlerResult::completed(
                            ResultRef::new("result-concurrent-iroh")
                                .expect("string ID literal/generated value must be non-empty"),
                        )
                    },
                )
                .await
        });

        let first_trace = TraceId::new("trace-concurrent-first")
            .expect("string ID literal/generated value must be non-empty");
        let second_trace = TraceId::new("trace-concurrent-second")
            .expect("string ID literal/generated value must be non-empty");
        let first_hello = test_hello_request(&first_endpoint_id, &first_trace);
        let second_hello = test_hello_request(&second_endpoint_id, &second_trace);
        let (first_hello_response, second_hello_response) = tokio::join!(
            first_client.send_hello(&server_ticket, &first_hello),
            second_client.send_hello(&server_ticket, &second_hello),
        );
        let first_hello_response = first_hello_response.unwrap();
        let second_hello_response = second_hello_response.unwrap();
        assert_eq!(first_hello_response.hello_outcome, HelloOutcome::Admitted);
        assert_eq!(second_hello_response.hello_outcome, HelloOutcome::Admitted);

        let first_call = test_call_request(&first_endpoint_id, &first_trace, &first_hello_response);
        let second_call =
            test_call_request(&second_endpoint_id, &second_trace, &second_hello_response);
        let (first_reply, second_reply) = tokio::join!(
            first_client.send_call(&server_ticket, &first_call),
            second_client.send_call(&server_ticket, &second_call),
        );
        let first_reply = first_reply.unwrap();
        let second_reply = second_reply.unwrap();
        assert_eq!(first_reply.reply_outcome, CallProtocolReplyOutcome::Success);
        assert_eq!(
            second_reply.reply_outcome,
            CallProtocolReplyOutcome::Success
        );

        first_client.close().await;
        second_client.close().await;
        serve_task.abort();
    }

    #[tokio::test]
    async fn concurrent_serve_requires_signed_peer_binding_when_configured() {
        let server_secret = iroh::SecretKey::generate();
        let server_secret_hex = crate::identity::encode_hex(&server_secret.to_bytes());
        let server = MotherIrohEndpoint::bind(
            MotherIrohEndpointConfig::local_mct().with_secret_key_hex(server_secret_hex.clone()),
        )
        .await
        .unwrap();
        let server_endpoint_id = server.snapshot().endpoint_id;
        let mut signed_client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut unsigned_client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let server_ticket = server.ticket();
        let signed_endpoint_id = signed_client.snapshot().endpoint_id;
        let unsigned_endpoint_id = unsigned_client.snapshot().endpoint_id;
        let signed_binding = test_peer_binding(&signed_endpoint_id);
        let unsigned_binding = test_peer_binding(&unsigned_endpoint_id);
        let signature_ref = sign_peer_binding_signature_ref(
            &server_secret_hex,
            &signed_binding,
            &server_endpoint_id,
        )
        .unwrap();
        let serve_task = tokio::spawn(async move {
            server
                .serve_concurrent_with_call_handler(
                    MctIrohServeState::new(),
                    vec![signed_binding, unsigned_binding],
                    MctIrohConcurrentServeConfig {
                        require_binding_signature: true,
                        ..MctIrohConcurrentServeConfig::default()
                    },
                    || Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                    |_, _, _| async { MctIrohCallHandlerResult::accepted_for_routing(None) },
                )
                .await
        });

        let signed_trace = TraceId::new("trace-signed-binding")
            .expect("string ID literal/generated value must be non-empty");
        let unsigned_trace = TraceId::new("trace-unsigned-binding")
            .expect("string ID literal/generated value must be non-empty");
        let mut signed_hello = test_hello_request(&signed_endpoint_id, &signed_trace);
        signed_hello.presented_binding.signature_ref = Some(signature_ref);
        let unsigned_hello = test_hello_request(&unsigned_endpoint_id, &unsigned_trace);
        let (signed_response, unsigned_response) = tokio::join!(
            signed_client.send_hello(&server_ticket, &signed_hello),
            unsigned_client.send_hello(&server_ticket, &unsigned_hello),
        );

        assert_eq!(
            signed_response.unwrap().hello_outcome,
            HelloOutcome::Admitted
        );
        let unsigned_response = unsigned_response.unwrap();
        assert_eq!(unsigned_response.hello_outcome, HelloOutcome::Denied);
        assert_eq!(unsigned_response.safe_message, "not authorized");

        signed_client.close().await;
        unsigned_client.close().await;
        serve_task.abort();
    }

    #[tokio::test]
    async fn concurrent_serve_refuses_connections_beyond_bound() {
        let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let server_ticket = server.ticket();
        let server_addr = endpoint_addr_from_ticket(&server_ticket).unwrap();
        let (events, mut received_events) = tokio::sync::mpsc::channel(8);
        let serve_task = tokio::spawn(async move {
            server
                .serve_concurrent_with_call_handler(
                    MctIrohServeState::new(),
                    Vec::new(),
                    MctIrohConcurrentServeConfig {
                        max_concurrent_connections: 1,
                        events: Some(events),
                        ..MctIrohConcurrentServeConfig::default()
                    },
                    || Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                    |_, _, _| async { MctIrohCallHandlerResult::accepted_for_routing(None) },
                )
                .await
        });
        let raw_client = Endpoint::builder(presets::N0)
            .relay_mode(RelayMode::Disabled)
            .alpns(mct_alpn_bytes())
            .bind()
            .await
            .unwrap();
        let raw_task = tokio::spawn(async move {
            let connection = raw_client
                .connect(server_addr, MCT_HELLO_ALPN.as_bytes())
                .await
                .unwrap();
            let (_send, _recv) = connection.open_bi().await.unwrap();
            std::future::pending::<()>().await;
        });
        assert!(matches!(
            received_events.recv().await,
            Some(MctIrohServeEvent::AcceptedConnection)
        ));

        let refused_client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let refused_endpoint_id = refused_client.snapshot().endpoint_id;
        let refused_trace = TraceId::new("trace-refused-concurrent")
            .expect("string ID literal/generated value must be non-empty");
        let refused_hello = test_hello_request(&refused_endpoint_id, &refused_trace);
        let refused_call = tokio::spawn(async move {
            refused_client
                .send_hello(&server_ticket, &refused_hello)
                .await
        });
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(5), received_events.recv()).await,
            Ok(Some(MctIrohServeEvent::RefusedConnection))
        ));

        raw_task.abort();
        refused_call.abort();
        serve_task.abort();
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
            TraceId::new("trace-obs").expect("string ID literal/generated value must be non-empty"),
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
            ObservationId::new("obs-iroh-hello-decision")
                .expect("string ID literal/generated value must be non-empty")
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
            ObservationId::new("obs-iroh-call-decision")
                .expect("string ID literal/generated value must be non-empty")
        );
    }

    #[tokio::test]
    async fn call_rechecks_binding_revocation_after_hello() {
        assert_current_binding_denial(
            |binding| binding.binding_state = BindingState::Revoked,
            Timestamp::new("2026-05-31T00:00:03Z").unwrap(),
            CallProtocolReason::BindingRevoked,
        )
        .await;
    }

    #[tokio::test]
    async fn call_rechecks_binding_expiry_after_hello() {
        assert_current_binding_denial(
            |binding| {
                binding.expires_at = Some(Timestamp::new("2026-05-31T00:00:03Z").unwrap());
            },
            Timestamp::new("2026-05-31T00:00:04Z").unwrap(),
            CallProtocolReason::BindingExpired,
        )
        .await;
    }

    #[tokio::test]
    async fn call_rechecks_binding_policy_revision_after_hello() {
        assert_current_binding_denial(
            |binding| binding.policy_revision = 2,
            Timestamp::new("2026-05-31T00:00:03Z").unwrap(),
            CallProtocolReason::PolicyRevisionStale,
        )
        .await;
    }

    #[tokio::test]
    async fn call_rechecks_narrowed_alpn_scope_after_hello() {
        assert_current_binding_denial(
            |binding| binding.scope.allowed_alpns = vec![MCT_HELLO_ALPN.into()],
            Timestamp::new("2026-05-31T00:00:03Z").unwrap(),
            CallProtocolReason::AlpnNotAdmitted,
        )
        .await;
    }

    #[tokio::test]
    async fn call_rechecks_narrowed_vision_scope_after_hello() {
        assert_current_binding_denial(
            |binding| {
                binding.scope.vision_id = VisionId::new("vision-narrowed")
                    .expect("string ID literal/generated value must be non-empty");
            },
            Timestamp::new("2026-05-31T00:00:03Z").unwrap(),
            CallProtocolReason::VisionMismatch,
        )
        .await;
    }

    async fn assert_current_binding_denial(
        mutate: impl FnOnce(&mut MctPeerBinding),
        call_time: Timestamp,
        expected_reason: CallProtocolReason,
    ) {
        let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let server_ticket = server.ticket();
        let client_endpoint_id = client.snapshot().endpoint_id;
        let binding = Arc::new(Mutex::new(test_peer_binding(&client_endpoint_id)));
        let now = Arc::new(Mutex::new(Timestamp::new("2026-05-31T00:00:02Z").unwrap()));
        let execution_count = Arc::new(AtomicU64::new(0));
        let (events, mut received_events) = tokio::sync::mpsc::channel(8);

        let provider_binding = Arc::clone(&binding);
        let server_now = Arc::clone(&now);
        let handler_execution_count = Arc::clone(&execution_count);
        let serve_task = tokio::spawn(async move {
            server
                .serve_concurrent_with_binding_provider(
                    MctIrohServeState::new(),
                    MctIrohConcurrentServeConfig {
                        events: Some(events),
                        ..MctIrohConcurrentServeConfig::default()
                    },
                    move || server_now.lock().unwrap().clone(),
                    move || {
                        let current = provider_binding.lock().unwrap().clone();
                        async move {
                            Ok(MctPeerAuthoritySnapshot {
                                policy_revision: current.policy_revision,
                                bindings: vec![current],
                            })
                        }
                    },
                    move |_, _, _| {
                        let execution_count = Arc::clone(&handler_execution_count);
                        async move {
                            execution_count.fetch_add(1, Ordering::SeqCst);
                            MctIrohCallHandlerResult::completed(
                                ResultRef::new("result-current-binding")
                                    .expect("string ID literal/generated value must be non-empty"),
                            )
                        }
                    },
                )
                .await
        });

        let trace_id = TraceId::new("trace-current-binding")
            .expect("string ID literal/generated value must be non-empty");
        let hello = test_hello_request(&client_endpoint_id, &trace_id);
        let hello_response = client.send_hello(&server_ticket, &hello).await.unwrap();
        assert_eq!(hello_response.hello_outcome, HelloOutcome::Admitted);

        mutate(&mut binding.lock().unwrap());
        *now.lock().unwrap() = call_time;

        let call = test_call_request(&client_endpoint_id, &trace_id, &hello_response);
        let reply = client.send_call(&server_ticket, &call).await.unwrap();
        assert_eq!(reply.reply_outcome, CallProtocolReplyOutcome::Denied);
        assert_eq!(reply.safe_message, "not authorized");
        assert_eq!(execution_count.load(Ordering::SeqCst), 0);

        let evaluation = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(MctIrohServeEvent::Served(served)) = received_events.recv().await
                    && let MctIrohServedProtocol::Call { evaluation, .. } = *served
                {
                    break evaluation;
                }
            }
        })
        .await
        .expect("call evaluation event");
        assert_eq!(evaluation.outcome, CallProtocolOutcome::Denied);
        assert_eq!(evaluation.reason, expected_reason);

        client.close().await;
        serve_task.abort();
    }

    fn test_peer_binding(endpoint_id: &EndpointIdText) -> MctPeerBinding {
        MctPeerBinding {
            binding_id: PeerBindingId::new("binding-public-iroh")
                .expect("string ID literal/generated value must be non-empty"),
            iroh_endpoint_id: endpoint_id.clone(),
            scope: MctPeerBindingScope {
                mct_node_id: MctNodeId::new("mother-client")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-public")
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
            created_by_observation_id: ObservationId::new("obs-binding-public-iroh")
                .expect("string ID literal/generated value must be non-empty"),
            superseded_by_observation_id: None,
        }
    }

    fn test_hello_request(endpoint_id: &EndpointIdText, trace_id: &TraceId) -> MctHelloRequest {
        MctHelloRequest {
            hello_id: "hello-public-iroh".into(),
            received_over: IrohConnectionPresentation {
                endpoint_id: endpoint_id.clone(),
                alpn: MCT_HELLO_ALPN.into(),
                connection_side: ConnectionSide::Outgoing,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            requested_protocol: HelloPolicy::default().protocol,
            requested_vision_id: Some(
                VisionId::new("vision-public")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            requested_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            presented_binding: MctPeerBindingPresentation {
                binding_id: Some(
                    PeerBindingId::new("binding-public-iroh")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                endpoint_id: endpoint_id.clone(),
                mct_node_id: Some(
                    MctNodeId::new("mother-client")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                vision_id: Some(
                    VisionId::new("vision-public")
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
            received_observation_id: ObservationId::new("obs-public-hello-received")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn test_call_request(
        endpoint_id: &EndpointIdText,
        trace_id: &TraceId,
        hello: &MctHelloResponse,
    ) -> MctCallProtocolRequest {
        let call = MctCall {
            call_id: CallId::new("call-public-iroh")
                .expect("string ID literal/generated value must be non-empty"),
            caller: CallerIdentity {
                node_id: MctNodeId::new("mother-client")
                    .expect("string ID literal/generated value must be non-empty"),
                user_id: None,
                vision_id: VisionId::new("vision-public")
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
                size_bytes: 0,
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
                span_id: SpanId::new("span-public-call")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            origin: CallOrigin::Iroh,
        };

        MctCallProtocolRequest {
            protocol_request_id: ProtocolRequestId::new("proto-public-call")
                .expect("string ID literal/generated value must be non-empty"),
            authority: MctCallProtocolAuthority {
                hello_decision_id: hello.decision_id.clone(),
                peer_binding_id: PeerBindingId::new("binding-public-iroh")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-public")
                    .expect("string ID literal/generated value must be non-empty"),
                accepted_alpn: MCT_CALL_ALPN.into(),
                endpoint_id: endpoint_id.clone(),
                policy_revision: 1,
                grants_revision: 1,
            },
            received_over: IrohConnectionPresentation {
                endpoint_id: endpoint_id.clone(),
                alpn: MCT_CALL_ALPN.into(),
                connection_side: ConnectionSide::Outgoing,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            call,
            payload: MctCallPayloadHandle::Empty,
            idempotency_key: Some("idem-public-call".into()),
            received_observation_id: ObservationId::new("obs-public-call-received")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn fake_hello_response() -> MctHelloResponse {
        MctHelloResponse {
            response_id: "reply-fake-hello".into(),
            request_id: "hello-fake".into(),
            decision_id: DecisionId::new("decision-fake-hello")
                .expect("string ID literal/generated value must be non-empty"),
            hello_outcome: HelloOutcome::Admitted,
            negotiated_protocol: None,
            accepted_alpns: vec![MCT_CALL_ALPN.into()],
            safe_message: "admitted".into(),
            retry_after: None,
            capability_view: None,
            response_observation_id: ObservationId::new("obs-fake-hello-reply")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn blake3_hex(bytes: &[u8]) -> String {
        blake3::hash(bytes).to_hex().to_string()
    }

    fn inline_payload_handle(reference: &str, bytes: &[u8]) -> MctCallPayloadHandle {
        MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: reference.into(),
            content_type: "application/json".into(),
            size_bytes: bytes.len() as u64,
            blake3_digest_hex: blake3_hex(bytes),
        }
    }

    fn inline_call_request(
        endpoint_id: &EndpointIdText,
        trace_id: &TraceId,
        hello: &MctHelloResponse,
        bytes: &[u8],
    ) -> MctCallProtocolRequest {
        let mut request = test_call_request(endpoint_id, trace_id, hello);
        request.call.payload_metadata.size_bytes = bytes.len() as u64;
        request.payload = inline_payload_handle("payload-public-inline", bytes);
        request
    }

    #[tokio::test]
    async fn call_payload_roundtrip_carries_request_and_result_bytes() {
        let mut server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let client_endpoint_id = client.snapshot().endpoint_id;
        let server_ticket = server.ticket();
        let binding = test_peer_binding(&client_endpoint_id);
        let mut state = MctIrohServeState::new();
        let trace_id = TraceId::new("trace-payload-roundtrip")
            .expect("string ID literal/generated value must be non-empty");
        let hello_request = test_hello_request(&client_endpoint_id, &trace_id);
        let (_served_hello, hello_response) = tokio::join!(
            server.serve_next(
                &mut state,
                std::slice::from_ref(&binding),
                Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
                None,
            ),
            client.send_hello(&server_ticket, &hello_request),
        );
        let hello_response = hello_response.unwrap();
        let request_payload = br#"["hello"]"#.to_vec();
        let expected_request_payload = request_payload.clone();
        let sent_request_payload = request_payload.clone();
        let result_payload = br#"["hello-result"]"#.to_vec();
        let returned_result_payload = result_payload.clone();
        let call_request = inline_call_request(
            &client_endpoint_id,
            &trace_id,
            &hello_response,
            &request_payload,
        );
        let (served_call, call_reply) = tokio::join!(
            server.serve_next_with_call_handler(
                &mut state,
                std::slice::from_ref(&binding),
                Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                |_, evaluation, payload| {
                    assert!(evaluation.is_accepted_for_routing());
                    assert_eq!(payload, Some(expected_request_payload.as_slice()));
                    MctIrohCallHandlerResult::completed_with_inline_payload(
                        ResultRef::new("result-payload-roundtrip")
                            .expect("string ID literal/generated value must be non-empty"),
                        inline_payload_handle("result-payload-roundtrip", &returned_result_payload),
                        returned_result_payload.clone(),
                    )
                }
            ),
            client.send_call_with_inline_payload(
                &server_ticket,
                &call_request,
                sent_request_payload
            ),
        );
        let served_call = served_call.unwrap();
        let call_reply = call_reply.unwrap();
        assert_eq!(
            call_reply.reply.reply_outcome,
            CallProtocolReplyOutcome::Success
        );
        assert_eq!(
            call_reply.inline_result_payload,
            Some(br#"["hello-result"]"#.to_vec())
        );
        assert!(matches!(
            served_call,
            MctIrohServedProtocol::Call { evaluation, .. }
                if evaluation.outcome == CallProtocolOutcome::Completed
        ));
        server.close().await;
        client.close().await;
    }

    #[tokio::test]
    async fn call_payload_integrity_failures_are_malformed_before_authority() {
        let mut server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let server_ticket = server.ticket();
        let endpoint_id = EndpointIdText::new("endpoint-malicious")
            .expect("string ID literal/generated value must be non-empty");
        let trace_id = TraceId::new("trace-payload-malformed")
            .expect("string ID literal/generated value must be non-empty");
        let declared = b"abc";
        let actual = b"xyz";
        let call_request =
            inline_call_request(&endpoint_id, &trace_id, &fake_hello_response(), declared);
        let mut state = MctIrohServeState::new();
        let (served, reply) = tokio::join!(
            server.serve_next(
                &mut state,
                &[],
                Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                None,
            ),
            client.send_call_with_unchecked_inline_payload(
                &server_ticket,
                &call_request,
                actual.to_vec(),
            ),
        );
        let served = served.unwrap();
        assert_eq!(
            reply.unwrap().reply.reply_outcome,
            CallProtocolReplyOutcome::Malformed
        );
        assert!(matches!(
            served,
            MctIrohServedProtocol::Call { evaluation, .. }
                if evaluation.reason == CallProtocolReason::PayloadDigestMismatch
                    && evaluation.outcome == CallProtocolOutcome::Malformed
        ));
        server.close().await;
        client.close().await;
    }

    #[tokio::test]
    async fn call_payload_caps_fail_closed() {
        let mut server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let server_ticket = server.ticket();
        let endpoint_id = EndpointIdText::new("endpoint-oversize")
            .expect("string ID literal/generated value must be non-empty");
        let trace_id = TraceId::new("trace-payload-oversize")
            .expect("string ID literal/generated value must be non-empty");
        let actual = b"x";
        let mut declared_request =
            inline_call_request(&endpoint_id, &trace_id, &fake_hello_response(), actual);
        declared_request.call.payload_metadata.size_bytes =
            (MCT_INLINE_PAYLOAD_MAX_BYTES + 1) as u64;
        declared_request.payload = MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: "payload-declared-too-large".into(),
            content_type: "application/json".into(),
            size_bytes: (MCT_INLINE_PAYLOAD_MAX_BYTES + 1) as u64,
            blake3_digest_hex: blake3_hex(actual),
        };
        let mut state = MctIrohServeState::new();
        let (served_declared, reply_declared) = tokio::join!(
            server.serve_next(
                &mut state,
                &[],
                Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                None,
            ),
            client.send_call_with_unchecked_inline_payload(
                &server_ticket,
                &declared_request,
                actual.to_vec(),
            ),
        );
        assert!(matches!(
            served_declared.unwrap(),
            MctIrohServedProtocol::Call { evaluation, .. }
                if evaluation.reason == CallProtocolReason::PayloadDeclaredTooLarge
        ));
        assert_eq!(
            reply_declared.unwrap().reply.reply_outcome,
            CallProtocolReplyOutcome::Malformed
        );

        let actual_too_large = vec![b'x'; MCT_INLINE_PAYLOAD_MAX_BYTES + 1];
        let mut actual_request =
            inline_call_request(&endpoint_id, &trace_id, &fake_hello_response(), b"x");
        actual_request.payload = MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: "payload-actual-too-large".into(),
            content_type: "application/json".into(),
            size_bytes: 1,
            blake3_digest_hex: blake3_hex(&actual_too_large),
        };
        let mut state = MctIrohServeState::new();
        let (served_actual, reply_actual) = tokio::join!(
            server.serve_next(
                &mut state,
                &[],
                Timestamp::new("2026-05-31T00:00:03Z").unwrap(),
                None,
            ),
            client.send_call_with_unchecked_inline_payload(
                &server_ticket,
                &actual_request,
                actual_too_large.clone(),
            ),
        );
        assert!(matches!(
            served_actual.unwrap(),
            MctIrohServedProtocol::Call { evaluation, .. }
                if evaluation.reason == CallProtocolReason::PayloadActualTooLarge
        ));
        assert_eq!(
            reply_actual.unwrap().reply.reply_outcome,
            CallProtocolReplyOutcome::Malformed
        );
        server.close().await;
        client.close().await;
    }

    #[test]
    fn caller_rejects_reply_digest_mismatch_and_oversized_result() {
        fn reply_envelope(reply: MctCallProtocolReply, inline_result_payload: &[u8]) -> Vec<u8> {
            let mut envelope = serde_json::to_value(reply).unwrap();
            envelope["inline_result_payload_base64"] = serde_json::json!(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                inline_result_payload,
            ));
            serde_json::to_vec(&envelope).unwrap()
        }

        let protocol_request_id = ProtocolRequestId::new("proto-reply-integrity")
            .expect("string ID literal/generated value must be non-empty");
        let reply = MctCallProtocolReply {
            reply_id: ReplyId::new("reply-digest-mismatch")
                .expect("string ID literal/generated value must be non-empty"),
            protocol_request_id: protocol_request_id.clone(),
            decision_id: DecisionId::new("decision-reply-digest")
                .expect("string ID literal/generated value must be non-empty"),
            result_ref: Some(
                ResultRef::new("result-digest-mismatch")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            result_payload: MctCallPayloadHandle::InlinePayload {
                inline_payload_ref: "result-digest-mismatch".into(),
                content_type: "application/json".into(),
                size_bytes: 3,
                blake3_digest_hex: blake3_hex(b"abc"),
            },
            route_taken: None,
            reply_outcome: CallProtocolReplyOutcome::Success,
            safe_message: "call completed".into(),
            reply_observation_id: ObservationId::new("obs-reply-digest")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let error =
            crate::serve::decode_call_reply_envelope(&reply_envelope(reply, b"xyz")).unwrap_err();
        assert!(matches!(
            error,
            MotherIrohEndpointError::ProtocolPayload {
                reason: PayloadIntegrityReason::ResultPayloadIntegrityMismatch,
                ..
            }
        ));

        let oversized = vec![b'x'; MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES + 1];
        let reply = MctCallProtocolReply {
            reply_id: ReplyId::new("reply-oversized-result")
                .expect("string ID literal/generated value must be non-empty"),
            protocol_request_id,
            decision_id: DecisionId::new("decision-reply-oversized")
                .expect("string ID literal/generated value must be non-empty"),
            result_ref: Some(
                ResultRef::new("result-oversized")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            result_payload: inline_payload_handle("result-oversized", &oversized),
            route_taken: None,
            reply_outcome: CallProtocolReplyOutcome::Success,
            safe_message: "call completed".into(),
            reply_observation_id: ObservationId::new("obs-reply-oversized")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let error = crate::serve::decode_call_reply_envelope(&reply_envelope(reply, &oversized))
            .unwrap_err();
        assert!(matches!(
            error,
            MotherIrohEndpointError::ProtocolPayload {
                reason: PayloadIntegrityReason::ResultPayloadTooLarge,
                ..
            }
        ));
    }
}

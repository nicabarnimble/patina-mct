//! Iroh adapter boundary for MCT peer protocols.
//!
//! This crate owns Mother-owned Iroh endpoint lifecycle and MCT ALPN protocol
//! effects. It translates Iroh facts into `mct-kernel` domain records rather
//! than making Iroh transport identity into MCT authority.

#![forbid(unsafe_code)]

mod endpoint;
#[cfg(test)]
mod observation;
#[cfg(test)]
mod test_support;

pub use endpoint::{
    MctIrohCallHandlerResult, MctIrohPeerCallReport, MctIrohServeState, MctIrohServedProtocol,
    MotherIrohEndpoint, MotherIrohEndpointConfig, MotherIrohEndpointError,
    MotherIrohEndpointLifecycle, MotherIrohEndpointResult, MotherIrohEndpointSnapshot,
    MotherIrohEndpointTicket, MotherIrohRelayMode, endpoint_id_for_secret_key_hex,
    load_or_create_node_secret_key_hex,
};

/// Returns the crate version for health and smoke tests.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoint::{endpoint_addr_from_ticket, mct_alpn_bytes, mct_alpns};
    use crate::observation::local_denied_peer_adapter_observations;
    use crate::test_support::{run_local_iroh_echo_roundtrip, run_unknown_peer_denial_roundtrip};
    use iroh::{Endpoint, RelayMode, endpoint::presets};
    use mct_kernel::*;
    use std::time::Duration;

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
                |_, _| MctIrohCallHandlerResult::accepted_for_routing(None),
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
                |_, evaluation| {
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
                approximate_size_bytes: 5,
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
            payload: MctCallPayloadHandle::InlinePayload {
                inline_payload_ref: "payload-public-echo".into(),
                content_type: "text/plain".into(),
                approximate_size_bytes: 5,
            },
            idempotency_key: Some("idem-public-call".into()),
            received_observation_id: ObservationId::new("obs-public-call-received")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }
}

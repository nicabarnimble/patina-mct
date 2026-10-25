//! H2: wire fields cannot promote an authenticated peer to a local principal.
use super::*;

#[tokio::test]
async fn wire_origins_and_upstream_identities_are_denied_in_both_serve_paths() {
    for concurrent in [false, true] {
        let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let ticket = server.ticket();
        let endpoint = client.snapshot().endpoint_id;
        let binding = test_peer_binding(&endpoint);
        let executions = Arc::new(AtomicU64::new(0));
        let count = executions.clone();
        let handler = move |request: &MctCallProtocolRequest| {
            assert_eq!(
                request.received_over.connection_side,
                ConnectionSide::Incoming
            );
            count.fetch_add(1, Ordering::SeqCst);
            MctIrohCallHandlerResult::completed(ResultRef::new("result-origin-control").unwrap())
        };
        let task = tokio::spawn(async move {
            if concurrent {
                server
                    .serve_concurrent_with_call_handler(
                        MctIrohServeState::new(),
                        vec![binding],
                        MctIrohConcurrentServeConfig::new(test_observation_sink().clone()),
                        || Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                        move |request, _, _| {
                            let result = handler(&request);
                            async move { result }
                        },
                    )
                    .await
                    .unwrap();
            } else {
                let mut state = MctIrohServeState::new();
                loop {
                    server
                        .serve_next_with_call_handler(
                            &mut state,
                            std::slice::from_ref(&binding),
                            test_observation_sink(),
                            Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                            |request, _, _| handler(request),
                        )
                        .await
                        .unwrap();
                }
            }
        });
        let trace = TraceId::new("trace-origin-boundary").unwrap();
        let hello = client
            .send_hello(&ticket, &test_hello_request(&endpoint, &trace))
            .await
            .unwrap();
        assert_eq!(hello.hello_outcome, HelloOutcome::Admitted);
        let valid = test_call_request(&endpoint, &trace, &hello);
        assert_eq!(
            client
                .send_call(&ticket, &valid)
                .await
                .unwrap()
                .reply_outcome,
            CallProtocolReplyOutcome::Success
        );
        let mut attacks = Vec::new();
        for origin in [
            CallOrigin::Cli,
            CallOrigin::JvmAdapter,
            CallOrigin::WasmHost,
            CallOrigin::ProcessHarness,
            CallOrigin::TriggerFiring,
        ] {
            let mut request = valid.clone();
            request.call.origin = origin;
            attacks.push(request);
        }
        let mut upstream = valid.clone();
        upstream.call.caller.node_id = MctNodeId::new("spoofed-upstream-mother").unwrap();
        attacks.push(upstream);
        let mut user = valid.clone();
        user.call.caller.user_id = Some(UserId::new("local-owner").unwrap());
        attacks.push(user);
        let mut vision = valid.clone();
        vision.call.caller.vision_id = VisionId::new("other-vision").unwrap();
        attacks.push(vision);
        let mut binding = valid.clone();
        binding.authority.peer_binding_id = PeerBindingId::new("upstream-binding").unwrap();
        attacks.push(binding);
        let mut transport = valid.clone();
        transport.authority.endpoint_id = EndpointIdText::new("upstream-endpoint").unwrap();
        transport.received_over.endpoint_id = transport.authority.endpoint_id.clone();
        attacks.push(transport);
        for request in attacks {
            let reply = client.send_call(&ticket, &request).await.unwrap();
            assert_eq!(
                reply.reply_outcome,
                CallProtocolReplyOutcome::Denied,
                "concurrent={concurrent} origin={:?}: {reply:?}",
                request.call.origin
            );
            assert_eq!(reply.safe_message, "not authorized");
            assert_eq!(reply.retry_directive, CallProtocolRetryDirective::None);
            assert!(reply.route_taken.is_none());
            assert!(reply.result_ref.is_none());
        }
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        // Denial neither poisons the admitted peer nor requires a fresh receipt.
        assert_eq!(
            client
                .send_call(&ticket, &valid)
                .await
                .unwrap()
                .reply_outcome,
            CallProtocolReplyOutcome::Success
        );
        assert_eq!(executions.load(Ordering::SeqCst), 2);
        client.close().await;
        task.abort();
    }
}

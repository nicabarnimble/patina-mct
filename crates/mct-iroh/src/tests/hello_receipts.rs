//! R2: independent hellos from one transport identity must not replace receipts.
use super::*;

#[tokio::test]
async fn interleaved_same_endpoint_hellos_preserve_each_calls_exact_receipt() {
    let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let mut other = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let ticket = server.ticket();
    let endpoint = client.snapshot().endpoint_id;
    let other_endpoint = other.snapshot().endpoint_id;
    let bindings = vec![
        test_peer_binding(&endpoint),
        test_peer_binding(&other_endpoint),
    ];
    let executions = Arc::new(AtomicU64::new(0));
    let handler_executions = Arc::clone(&executions);
    let server_task = tokio::spawn(async move {
        server
            .serve_concurrent_with_call_handler(
                MctIrohServeState::new(),
                bindings,
                MctIrohConcurrentServeConfig::new(test_observation_sink().clone()),
                || Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                move |_, _, _| {
                    let executions = Arc::clone(&handler_executions);
                    async move {
                        executions.fetch_add(1, Ordering::SeqCst);
                        MctIrohCallHandlerResult::completed(
                            ResultRef::new("result-receipt").unwrap(),
                        )
                    }
                },
            )
            .await
    });
    let trace = TraceId::new("trace-forwarding-hello").unwrap();
    let first = client
        .send_hello(&ticket, &test_hello_request(&endpoint, &trace))
        .await
        .unwrap();
    let refresh_trace = TraceId::new("trace-background-refresh").unwrap();
    // Await the second admission before sending the first call. This forces
    // the fleet's failing order without depending on timing or concurrency.
    let refresh = client
        .send_hello(&ticket, &test_hello_request(&endpoint, &refresh_trace))
        .await
        .unwrap();
    assert_eq!(first.hello_outcome, HelloOutcome::Admitted);
    assert_eq!(refresh.hello_outcome, HelloOutcome::Admitted);
    assert_ne!(first.decision_id, refresh.decision_id);
    for receipt in [&first, &refresh] {
        let reply = client
            .send_call(&ticket, &test_call_request(&endpoint, &trace, receipt))
            .await
            .unwrap();
        assert_eq!(
            reply.reply_outcome,
            CallProtocolReplyOutcome::Success,
            "{reply:?}"
        );
    }
    assert_eq!(executions.load(Ordering::SeqCst), 2);

    // Unknown decisions and receipts issued to another authenticated endpoint
    // remain unusable, even when that endpoint has its own admitted binding.
    let mut unknown = test_call_request(&endpoint, &trace, &first);
    unknown.authority.hello_decision_id = DecisionId::new("unknown-receipt").unwrap();
    assert_eq!(
        client
            .send_call(&ticket, &unknown)
            .await
            .unwrap()
            .reply_outcome,
        CallProtocolReplyOutcome::Denied
    );
    let other_hello = other
        .send_hello(&ticket, &test_hello_request(&other_endpoint, &trace))
        .await
        .unwrap();
    assert_eq!(other_hello.hello_outcome, HelloOutcome::Admitted);
    let stolen = test_call_request(&other_endpoint, &trace, &first);
    assert_eq!(
        other
            .send_call(&ticket, &stolen)
            .await
            .unwrap()
            .reply_outcome,
        CallProtocolReplyOutcome::Denied
    );
    assert_eq!(executions.load(Ordering::SeqCst), 2);

    client.close().await;
    other.close().await;
    server_task.abort();
}

#[tokio::test]
async fn denied_or_unrecorded_hello_invalidates_all_prior_endpoint_receipts() {
    for fail_durability in [false, true] {
        let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let ticket = server.ticket();
        let endpoint = client.snapshot().endpoint_id;
        let binding = test_peer_binding(&endpoint);
        let executions = Arc::new(AtomicU64::new(0));
        let handler_executions = Arc::clone(&executions);
        let hellos = Arc::new(AtomicU64::new(0));
        let sink_hellos = Arc::clone(&hellos);
        let sink = MctIrohObservationSink::new(move |batch| {
            let hellos = Arc::clone(&sink_hellos);
            async move {
                for fact in batch.facts {
                    if matches!(fact, MctIrohObservationFact::HelloEvaluation { .. })
                        && hellos.fetch_add(1, Ordering::SeqCst) == 2
                        && fail_durability
                    {
                        return Err(std::io::Error::other("injected hello admission failure"));
                    }
                }
                Ok(())
            }
        });
        let task = tokio::spawn(async move {
            server
                .serve_concurrent_with_call_handler(
                    MctIrohServeState::new(),
                    vec![binding],
                    MctIrohConcurrentServeConfig::new(sink),
                    || Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                    move |_, _, _| {
                        let executions = Arc::clone(&handler_executions);
                        async move {
                            executions.fetch_add(1, Ordering::SeqCst);
                            MctIrohCallHandlerResult::completed(
                                ResultRef::new("result-receipt").unwrap(),
                            )
                        }
                    },
                )
                .await
        });
        let trace = TraceId::new("trace-receipt-invalidation").unwrap();
        let hello = test_hello_request(&endpoint, &trace);
        let first = client.send_hello(&ticket, &hello).await.unwrap();
        let second = client.send_hello(&ticket, &hello).await.unwrap();
        assert_eq!(first.hello_outcome, HelloOutcome::Admitted);
        assert_eq!(second.hello_outcome, HelloOutcome::Admitted);
        let control = client
            .send_call(&ticket, &test_call_request(&endpoint, &trace, &first))
            .await
            .unwrap();
        assert_eq!(control.reply_outcome, CallProtocolReplyOutcome::Success);
        let mut invalid = hello.clone();
        if !fail_durability {
            invalid.requested_vision_id = Some(VisionId::new("wrong-vision").unwrap());
        }
        let response = client.send_hello(&ticket, &invalid).await;
        if fail_durability {
            assert!(response.is_err());
        } else {
            assert_eq!(response.unwrap().hello_outcome, HelloOutcome::Denied);
        }
        assert_eq!(hellos.load(Ordering::SeqCst), 3);
        for receipt in [&first, &second] {
            let reply = client
                .send_call(&ticket, &test_call_request(&endpoint, &trace, receipt))
                .await
                .unwrap();
            assert_eq!(reply.reply_outcome, CallProtocolReplyOutcome::Denied);
        }
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        client.close().await;
        task.abort();
    }
}

//! H3/M1: bounded ingress without weakening mandatory rejection durability.
use super::*;

async fn raw_request(
    client: &MotherIrohEndpoint,
    ticket: &MotherIrohEndpointTicket,
    alpn: &str,
    bytes: &[u8],
) -> (iroh::endpoint::Connection, Vec<u8>) {
    let connection = client
        .endpoint
        .as_ref()
        .unwrap()
        .connect(endpoint_addr_from_ticket(ticket).unwrap(), alpn.as_bytes())
        .await
        .unwrap();
    let (mut send, mut recv) = connection.open_bi().await.unwrap();
    send.write_all(bytes).await.unwrap();
    send.finish().unwrap();
    let bytes = tokio::time::timeout(
        Duration::from_secs(5),
        recv.read_to_end(MCT_CALL_FRAME_READ_BUDGET_BYTES),
    )
    .await
    .unwrap()
    .unwrap();
    (connection, bytes)
}

#[tokio::test]
async fn malformed_rejections_require_durability_and_failure_does_not_kill_listener() {
    let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let ticket = server.ticket();
    let endpoint = client.snapshot().endpoint_id;
    let failures = Arc::new(AtomicU64::new(0));
    let fail = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let should_fail = fail.clone();
    let rejected = failures.clone();
    let sink = MctIrohObservationSink::new(move |batch| {
        let fail = should_fail.clone();
        let rejected = rejected.clone();
        async move {
            if batch.durability == MctIrohObservationDurability::BeforeEffect
                && fail.load(Ordering::SeqCst)
            {
                rejected.fetch_add(1, Ordering::SeqCst);
                return Err(std::io::Error::other("injected durable rejection failure"));
            }
            Ok(())
        }
    });
    let config = MctIrohConcurrentServeConfig::new(sink);
    let health = config.ingress_health.clone();
    let executions = Arc::new(AtomicU64::new(0));
    let count = executions.clone();
    let binding = test_peer_binding(&endpoint);
    let task = tokio::spawn(async move {
        server
            .serve_concurrent_with_call_handler(
                MctIrohServeState::new(),
                vec![binding],
                config,
                || Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                move |_, _, _| {
                    count.fetch_add(1, Ordering::SeqCst);
                    async { MctIrohCallHandlerResult::accepted_for_routing(None) }
                },
            )
            .await
    });
    let trace = TraceId::new("trace-durable-ingress").unwrap();
    let hello_request = test_hello_request(&endpoint, &trace);
    let hello = client.send_hello(&ticket, &hello_request).await.unwrap();
    let mut call = test_call_request(&endpoint, &trace, &hello);
    call.protocol_request_id = ProtocolRequestId::new("x".repeat(4096)).unwrap();
    fail.store(true, Ordering::SeqCst);
    assert!(client.send_call(&ticket, &call).await.is_err());
    let mut malformed = hello_request.clone();
    malformed.hello_id = "x".repeat(4096);
    assert!(client.send_hello(&ticket, &malformed).await.is_err());
    wait_count(&health.failed_connections, 2).await;
    assert_eq!(failures.load(Ordering::SeqCst), 2);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    assert!(!task.is_finished());
    fail.store(false, Ordering::SeqCst);
    // The failed malformed hello also invalidated the prior receipt.
    let old = test_call_request(&endpoint, &trace, &hello);
    assert_eq!(
        client.send_call(&ticket, &old).await.unwrap().reply_outcome,
        CallProtocolReplyOutcome::Denied
    );
    let hello = client.send_hello(&ticket, &hello_request).await.unwrap();
    assert_eq!(
        client
            .send_call(&ticket, &test_call_request(&endpoint, &trace, &hello))
            .await
            .unwrap()
            .reply_outcome,
        CallProtocolReplyOutcome::Success
    );
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    client.close().await;
    task.abort();
}

#[tokio::test]
async fn protocol_errors_and_a_full_projection_channel_do_not_abort_or_stall_listener() {
    let mut endpoint_config = MotherIrohEndpointConfig::local_mct();
    endpoint_config
        .accepted_alpns
        .push("unsupported-test".into());
    let server = MotherIrohEndpoint::bind(endpoint_config).await.unwrap();
    let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let ticket = server.ticket();
    let endpoint = client.snapshot().endpoint_id;
    let binding = test_peer_binding(&endpoint);
    let (events, _never_drained) = tokio::sync::mpsc::channel(1);
    let config = MctIrohConcurrentServeConfig {
        events: Some(events),
        ..MctIrohConcurrentServeConfig::new(test_observation_sink().clone())
    };
    let health = config.ingress_health.clone();
    let task = tokio::spawn(async move {
        server
            .serve_concurrent_with_call_handler(
                MctIrohServeState::new(),
                vec![binding],
                config,
                || Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                |_, _, _| async { MctIrohCallHandlerResult::accepted_for_routing(None) },
            )
            .await
    });
    for i in 1..=3 {
        let connection = client
            .endpoint
            .as_ref()
            .unwrap()
            .connect(
                endpoint_addr_from_ticket(&ticket).unwrap(),
                b"unsupported-test",
            )
            .await
            .unwrap();
        let (mut send, mut recv) = connection.open_bi().await.unwrap();
        send.write_all(b"{}").await.unwrap();
        send.finish().unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(5), recv.read_to_end(1024))
                .await
                .unwrap()
                .is_err()
        );
        wait_count(&health.failed_connections, i).await;
        assert!(!task.is_finished());
    }
    let trace = TraceId::new("trace-protocol-error-control").unwrap();
    let hello = tokio::time::timeout(
        Duration::from_secs(5),
        client.send_hello(&ticket, &test_hello_request(&endpoint, &trace)),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(hello.hello_outcome, HelloOutcome::Admitted);
    assert_eq!(
        client
            .send_call(&ticket, &test_call_request(&endpoint, &trace, &hello))
            .await
            .unwrap()
            .reply_outcome,
        CallProtocolReplyOutcome::Success
    );
    assert!(health.dropped_projection_events.load(Ordering::SeqCst) > 0);
    client.close().await;
    task.abort();
}

async fn wait_count(count: &AtomicU64, minimum: u64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while count.load(Ordering::SeqCst) < minimum {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

async fn next_served(events: &mut tokio::sync::mpsc::Receiver<MctIrohServeEvent>) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(events.recv().await, Some(MctIrohServeEvent::Served(_))) {
                break;
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn idle_stream_partial_frame_and_post_response_wait_cannot_pin_the_only_slot() {
    let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let ticket = server.ticket();
    let endpoint = client.snapshot().endpoint_id;
    let (events, mut receiver) = tokio::sync::mpsc::channel(16);
    let batches = Arc::new(Mutex::new(Vec::new()));
    let config = MctIrohConcurrentServeConfig {
        max_concurrent_connections: 1,
        connection_timeout: Duration::from_secs(60),
        frame_timeout: Duration::from_millis(100),
        close_timeout: Duration::from_millis(50),
        events: Some(events),
        ..MctIrohConcurrentServeConfig::new(collecting_observation_sink(batches.clone()))
    };
    let health = config.ingress_health.clone();
    let binding = test_peer_binding(&endpoint);
    let task = tokio::spawn(async move {
        server
            .serve_concurrent_with_call_handler(
                MctIrohServeState::new(),
                vec![binding],
                config,
                || Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                |_, _, _| async { MctIrohCallHandlerResult::accepted_for_routing(None) },
            )
            .await
    });
    let raw = client.endpoint.as_ref().unwrap();
    // No first stream: bounded separately from the 60-second execution budget.
    let idle = raw
        .connect(
            endpoint_addr_from_ticket(&ticket).unwrap(),
            MCT_CALL_ALPN.as_bytes(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), idle.closed())
        .await
        .unwrap();
    wait_count(&health.failed_connections, 1).await;

    // One incomplete frame. Extra stream credit is unavailable while it is held.
    let partial = raw
        .connect(
            endpoint_addr_from_ticket(&ticket).unwrap(),
            MCT_CALL_ALPN.as_bytes(),
        )
        .await
        .unwrap();
    let (mut send, mut recv) = partial.open_bi().await.unwrap();
    send.write_all(b"{").await.unwrap();
    let mut extra_bi = Box::pin(partial.open_bi());
    let mut extra_uni = Box::pin(partial.open_uni());
    std::future::poll_fn(|cx| {
        use std::future::Future;
        assert!(extra_bi.as_mut().poll(cx).is_pending());
        assert!(extra_uni.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        recv.read_to_end(MCT_CALL_FRAME_READ_BUDGET_BYTES),
    )
    .await
    .unwrap()
    .unwrap();
    let reply: MctCallProtocolReply = serde_json::from_slice(&response).unwrap();
    assert_eq!(reply.reply_outcome, CallProtocolReplyOutcome::Malformed);
    assert_bounded_rejection(&batches, "{");
    // Deliberately never close from this side; Served means the permit was freed.
    next_served(&mut receiver).await;
    let trace = TraceId::new("trace-slot-control").unwrap();
    let request = test_hello_request(&endpoint, &trace);
    let (held_open, response) = raw_request(
        &client,
        &ticket,
        MCT_HELLO_ALPN,
        &serde_json::to_vec(&request).unwrap(),
    )
    .await;
    assert_eq!(
        serde_json::from_slice::<MctHelloResponse>(&response)
            .unwrap()
            .hello_outcome,
        HelloOutcome::Admitted
    );
    next_served(&mut receiver).await;
    let hello = client.send_hello(&ticket, &request).await.unwrap();
    assert_eq!(hello.hello_outcome, HelloOutcome::Admitted);
    next_served(&mut receiver).await;
    assert!(!task.is_finished());
    drop((extra_bi, extra_uni));
    drop((held_open, partial));
    client.close().await;
    task.abort();
}

#[tokio::test]
async fn admitted_execution_outlives_an_independent_pre_admission_deadline() {
    let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let client = Arc::new(MotherIrohEndpoint::bind_local_mct().await.unwrap());
    let ticket = server.ticket();
    let endpoint = client.snapshot().endpoint_id;
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let config = MctIrohConcurrentServeConfig {
        max_concurrent_connections: 2,
        connection_timeout: Duration::from_secs(10),
        frame_timeout: Duration::from_millis(100),
        ..MctIrohConcurrentServeConfig::new(test_observation_sink().clone())
    };
    let health = config.ingress_health.clone();
    let started = entered.clone();
    let finish = release.clone();
    let binding = test_peer_binding(&endpoint);
    let task = tokio::spawn(async move {
        server
            .serve_concurrent_with_call_handler(
                MctIrohServeState::new(),
                vec![binding],
                config,
                || Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                move |_, _, _| {
                    let started = started.clone();
                    let finish = finish.clone();
                    async move {
                        started.notify_one();
                        finish.notified().await;
                        MctIrohCallHandlerResult::completed(
                            ResultRef::new("result-long-call").unwrap(),
                        )
                    }
                },
            )
            .await
    });
    let trace = TraceId::new("trace-execution-budget").unwrap();
    let hello = client
        .send_hello(&ticket, &test_hello_request(&endpoint, &trace))
        .await
        .unwrap();
    let request = test_call_request(&endpoint, &trace, &hello);
    let call_client = client.clone();
    let call_ticket = ticket.clone();
    let call = tokio::spawn(async move { call_client.send_call(&call_ticket, &request).await });
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    let idle = client
        .endpoint
        .as_ref()
        .unwrap()
        .connect(
            endpoint_addr_from_ticket(&ticket).unwrap(),
            MCT_CALL_ALPN.as_bytes(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), idle.closed())
        .await
        .unwrap();
    wait_count(&health.failed_connections, 1).await;
    assert!(
        !call.is_finished(),
        "admitted work inherited the ingress deadline"
    );
    release.notify_one();
    assert_eq!(
        call.await.unwrap().unwrap().reply_outcome,
        CallProtocolReplyOutcome::Success
    );
    task.abort();
}

fn identifier_paths(value: &serde_json::Value, prefix: &str, paths: &mut Vec<String>) {
    if let Some(fields) = value.as_object() {
        for (key, value) in fields {
            let path = format!("{prefix}/{key}");
            if value.is_string()
                && (key.ends_with("_id")
                    || matches!(key.as_str(), "authority_epoch" | "idempotency_key"))
            {
                paths.push(path.clone());
            }
            identifier_paths(value, &path, paths);
        }
    }
}

fn assert_bounded_rejection(batches: &Arc<Mutex<Vec<MctIrohObservationBatch>>>, hostile: &str) {
    let batches = batches.lock().unwrap();
    let batch = batches
        .iter()
        .rev()
        .find(|batch| batch.durability == MctIrohObservationDurability::BeforeEffect)
        .unwrap();
    let observations: Vec<_> = batch
        .facts
        .iter()
        .map(|fact| fact.to_observation(Timestamp::new("2026-05-31T00:00:02Z").unwrap()))
        .collect();
    let bytes = serde_json::to_vec(&observations).unwrap();
    assert!(
        bytes.len() < 8192,
        "rejection amplified to {} bytes",
        bytes.len()
    );
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains(&serde_json::to_string(hostile).unwrap()));
    assert!(
        observations
            .iter()
            .all(|obs| obs.outcome == ObservationOutcome::Denied)
    );
}

#[tokio::test]
async fn all_wire_identifier_fields_are_bounded_before_facts_in_both_serve_paths() {
    for concurrent in [false, true] {
        let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let ticket = server.ticket();
        let endpoint = client.snapshot().endpoint_id;
        let binding = test_peer_binding(&endpoint);
        let batches = Arc::new(Mutex::new(Vec::new()));
        let sink = collecting_observation_sink(batches.clone());
        let task = tokio::spawn(async move {
            if concurrent {
                server
                    .serve_concurrent_with_call_handler(
                        MctIrohServeState::new(),
                        vec![binding],
                        MctIrohConcurrentServeConfig::new(sink),
                        || Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                        |_, _, _| async { MctIrohCallHandlerResult::accepted_for_routing(None) },
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
                            &sink,
                            Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                            |_, _, _| MctIrohCallHandlerResult::accepted_for_routing(None),
                        )
                        .await
                        .unwrap();
                }
            }
        });
        let trace = TraceId::new("trace-identifier-matrix").unwrap();
        let hello_request = test_hello_request(&endpoint, &trace);
        let hello = client.send_hello(&ticket, &hello_request).await.unwrap();
        let call = test_call_request(&endpoint, &trace, &hello);
        let mut exact_limit = call.clone();
        exact_limit.protocol_request_id = ProtocolRequestId::new("p".repeat(256)).unwrap();
        exact_limit.call.trace_context.trace_id = TraceId::new("t".repeat(256)).unwrap();
        assert_eq!(
            client
                .send_call(&ticket, &exact_limit)
                .await
                .unwrap()
                .reply_outcome,
            CallProtocolReplyOutcome::Success
        );
        for (alpn, value) in [
            (MCT_CALL_ALPN, serde_json::to_value(call).unwrap()),
            (
                MCT_HELLO_ALPN,
                serde_json::to_value(&hello_request).unwrap(),
            ),
        ] {
            let mut paths = Vec::new();
            identifier_paths(&value, "", &mut paths);
            assert!(paths.len() >= 7);
            for path in paths {
                for hostile in [
                    "x".repeat(257),
                    "bad\nidentifier".into(),
                    "nonascii-é".into(),
                ] {
                    let mut invalid = value.clone();
                    *invalid.pointer_mut(&path).unwrap() = hostile.clone().into();
                    let (connection, response) = raw_request(
                        &client,
                        &ticket,
                        alpn,
                        &serde_json::to_vec(&invalid).unwrap(),
                    )
                    .await;
                    if alpn == MCT_CALL_ALPN {
                        let reply: MctCallProtocolReply =
                            serde_json::from_slice(&response).unwrap();
                        assert_eq!(
                            reply.reply_outcome,
                            CallProtocolReplyOutcome::Malformed,
                            "{path}"
                        );
                        assert!(
                            reply
                                .protocol_request_id
                                .as_str()
                                .starts_with("malformed-call-")
                        );
                    } else {
                        let reply: MctHelloResponse = serde_json::from_slice(&response).unwrap();
                        assert_eq!(reply.hello_outcome, HelloOutcome::Denied, "{path}");
                        assert!(reply.request_id.starts_with("malformed-hello-"));
                    }
                    assert_bounded_rejection(&batches, &hostile);
                    connection.close(0u32.into(), b"test complete");
                }
            }
        }
        for bytes in [
            b"{not-json".to_vec(),
            vec![b'x'; MCT_CALL_FRAME_READ_BUDGET_BYTES + 1],
        ] {
            let (connection, response) =
                raw_request(&client, &ticket, MCT_HELLO_ALPN, &bytes).await;
            assert_eq!(
                serde_json::from_slice::<MctHelloResponse>(&response)
                    .unwrap()
                    .hello_outcome,
                HelloOutcome::Denied
            );
            assert_bounded_rejection(&batches, "{not-json");
            connection.close(0u32.into(), b"malformed hello control");
        }
        // Rejection invalidates receipts, but does not poison a fresh admission.
        let hello = client.send_hello(&ticket, &hello_request).await.unwrap();
        assert_eq!(hello.hello_outcome, HelloOutcome::Admitted);
        assert_eq!(
            client
                .send_call(&ticket, &test_call_request(&endpoint, &trace, &hello))
                .await
                .unwrap()
                .reply_outcome,
            CallProtocolReplyOutcome::Success
        );
        client.close().await;
        task.abort();
    }
}

#[tokio::test]
async fn unadmitted_and_malformed_calls_do_not_fetch_authority() {
    let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let ticket = server.ticket();
    let endpoint = client.snapshot().endpoint_id;
    let binding = test_peer_binding(&endpoint);
    let binding_reads = Arc::new(AtomicU64::new(0));
    let receiver_reads = Arc::new(AtomicU64::new(0));
    let executions = Arc::new(AtomicU64::new(0));
    let batches = Arc::new(Mutex::new(Vec::new()));
    let mut config =
        MctIrohConcurrentServeConfig::new(collecting_observation_sink(batches.clone()));
    let reads = receiver_reads.clone();
    config.receiver_authority_provider = MctIrohReceiverAuthorityProvider::new(move || {
        reads.fetch_add(1, Ordering::SeqCst);
        async { Some(test_grants_authority_identity()) }
    });
    let reads = binding_reads.clone();
    let count = executions.clone();
    let task = tokio::spawn(async move {
        server
            .serve_concurrent_with_binding_provider(
                MctIrohServeState::new(),
                config,
                || Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                move || {
                    reads.fetch_add(1, Ordering::SeqCst);
                    let bindings = vec![binding.clone()];
                    async move {
                        Ok(MctPeerAuthoritySnapshot {
                            bindings,
                            policy_revision: 1,
                        })
                    }
                },
                move |_, _, _| {
                    count.fetch_add(1, Ordering::SeqCst);
                    async {
                        MctIrohCallHandlerResult::completed(
                            ResultRef::new("result-ingress").unwrap(),
                        )
                    }
                },
            )
            .await
    });
    let trace = TraceId::new("trace-ingress").unwrap();
    let hello = client
        .send_hello(&ticket, &test_hello_request(&endpoint, &trace))
        .await
        .unwrap();
    let valid = test_call_request(&endpoint, &trace, &hello);
    binding_reads.store(0, Ordering::SeqCst);
    receiver_reads.store(0, Ordering::SeqCst);
    let mut missing = valid.clone();
    missing.authority.hello_decision_id = DecisionId::new("unknown-receipt").unwrap();
    let reply = client.send_call(&ticket, &missing).await.unwrap();
    assert_eq!(reply.reply_outcome, CallProtocolReplyOutcome::Denied);
    assert_eq!(
        binding_reads.load(Ordering::SeqCst),
        0,
        "unknown hello must not fetch bindings"
    );
    assert_eq!(
        receiver_reads.load(Ordering::SeqCst),
        0,
        "unknown hello must not fetch receiver authority"
    );

    let mut malformed = serde_json::to_value(&valid).unwrap();
    malformed["protocol_request_id"] = "x".repeat(4096).into();
    let reply = raw_call_frame(&ticket, &serde_json::to_vec(&malformed).unwrap())
        .await
        .unwrap();
    let reply: MctCallProtocolReply = serde_json::from_slice(&reply).unwrap();
    assert_eq!(reply.reply_outcome, CallProtocolReplyOutcome::Malformed);
    assert_eq!(binding_reads.load(Ordering::SeqCst), 0);
    assert_eq!(receiver_reads.load(Ordering::SeqCst), 0);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    for pointer in [
        "/call/target/namespace",
        "/call/payload_metadata/size_bytes",
        "/authority/expected_receiver_grants_authority/generation",
        "/inline_payload_base64",
    ] {
        let mut broken = serde_json::to_value(&valid).unwrap();
        match pointer {
            "/call/target/namespace" => *broken.pointer_mut(pointer).unwrap() = "".into(),
            "/inline_payload_base64" => broken["inline_payload_base64"] = "eA==".into(),
            _ => *broken.pointer_mut(pointer).unwrap() = 2.into(),
        }
        let (connection, bytes) = raw_request(
            &client,
            &ticket,
            MCT_CALL_ALPN,
            &serde_json::to_vec(&broken).unwrap(),
        )
        .await;
        assert_eq!(
            serde_json::from_slice::<MctCallProtocolReply>(&bytes)
                .unwrap()
                .reply_outcome,
            CallProtocolReplyOutcome::Malformed,
            "{pointer}"
        );
        connection.close(0u32.into(), b"malformed control");
        assert_eq!(binding_reads.load(Ordering::SeqCst), 0);
        assert_eq!(receiver_reads.load(Ordering::SeqCst), 0);
    }
    assert!(batches.lock().unwrap().iter().any(|batch| {
        batch.durability == MctIrohObservationDurability::BeforeEffect && batch.facts.iter().any(|fact| {
            matches!(fact, MctIrohObservationFact::CallLifecycle(f) if f.stage == MctIrohCallLifecycleStage::Denied)
        })
    }));
    assert_eq!(
        client
            .send_call(&ticket, &valid)
            .await
            .unwrap()
            .reply_outcome,
        CallProtocolReplyOutcome::Success
    );
    assert_eq!(binding_reads.load(Ordering::SeqCst), 1);
    assert_eq!(receiver_reads.load(Ordering::SeqCst), 1);
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    client.close().await;
    task.abort();
}

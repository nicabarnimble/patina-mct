use super::*;

#[tokio::test]
async fn l7_hello_publication_uses_admitted_vision_and_rebuilds_derived_metadata() {
    use mct_kernel::{
        MctHelloCallableSurface, MctHelloCapabilityView, RuntimeKind, peer::HelloSurfaceVisibility,
    };
    for dynamic in [false, true] {
        let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut a = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut b = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let a_id = a.snapshot().endpoint_id;
        let b_id = b.snapshot().endpoint_id;
        let ticket = server.ticket();
        let a_binding = test_peer_binding(&a_id);
        let mut b_binding = test_peer_binding(&b_id);
        b_binding.scope.vision_id = VisionId::new("other-vision").unwrap();
        let view = MctHelloCapabilityView {
            node_id: MctNodeId::new("publisher").unwrap(),
            vision_id: VisionId::new("vision-public").unwrap(),
            published_at: Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
            policy_revision: 1,
            supported_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            supported_wit_worlds: vec!["foreign-secret".into()],
            supported_observation_modes: vec!["local-ledger".into()],
            capability_view_ref: Some("foreign-secret-inventory".into()),
            callable_surfaces: [
                ("visible", "vision-public"),
                ("foreign-secret", "other-vision"),
            ]
            .into_iter()
            .map(|(name, vision)| MctHelloCallableSurface {
                child_name: name.into(),
                operation_id: format!("{name}/control@0.1.0.run"),
                runtime_kind: RuntimeKind::WasmComponent,
                vision_id: VisionId::new(vision).unwrap(),
                policy_revision: 1,
                visibility: HelloSurfaceVisibility::VisionScoped,
            })
            .collect(),
        };
        let mut config = MctIrohConcurrentServeConfig::new(test_observation_sink().clone());
        if dynamic {
            config.capability_view_provider =
                Some(MctHelloCapabilityViewProvider::new(move || {
                    Some(view.clone())
                }));
        } else {
            config.capability_view = Some(view);
        }
        let task = tokio::spawn(async move {
            server
                .serve_concurrent_with_call_handler(
                    MctIrohServeState::new(),
                    vec![a_binding, b_binding],
                    config,
                    || Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
                    |_, _, _| async { MctIrohCallHandlerResult::denied() },
                )
                .await
        });
        let trace = TraceId::new("trace-l7").unwrap();
        let response = a
            .send_hello(&ticket, &test_hello_request(&a_id, &trace))
            .await
            .unwrap();
        assert_eq!(response.hello_outcome, HelloOutcome::Admitted);
        let view = response.capability_view.as_ref().unwrap();
        assert_eq!(view.callable_surfaces.len(), 1);
        assert_eq!(view.callable_surfaces[0].child_name, "visible");
        assert_eq!(view.supported_wit_worlds, ["visible/control@0.1.0"]);
        assert!(
            !serde_json::to_string(&response)
                .unwrap()
                .contains("foreign-secret")
        );
        let mut hello = test_hello_request(&b_id, &trace);
        hello.requested_vision_id = Some(VisionId::new("other-vision").unwrap());
        hello.presented_binding.vision_id = hello.requested_vision_id.clone();
        let other = b.send_hello(&ticket, &hello).await.unwrap();
        assert_eq!(other.hello_outcome, HelloOutcome::Admitted);
        assert!(
            other.capability_view.is_none(),
            "another Vision cannot see or relabel the publisher's view"
        );
        let mut call = test_call_request(&a_id, &trace, &response);
        call.call.caller.vision_id = VisionId::new("other-vision").unwrap();
        call.call.target.namespace = "foreign-secret".into();
        let denied = a.send_call(&ticket, &call).await.unwrap();
        assert_eq!(denied.reply_outcome, CallProtocolReplyOutcome::Denied);
        assert!(
            !serde_json::to_string(&denied)
                .unwrap()
                .contains("foreign-secret")
        );
        a.close().await;
        b.close().await;
        task.abort();
        let _ = task.await;
    }
}

#[tokio::test]
async fn l9_shared_send_handles_preserve_inbound_serving_and_owner_shutdown() {
    let mut a = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let mut b = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let a_id = a.snapshot().endpoint_id;
    let b_id = b.snapshot().endpoint_id;
    let a_ticket = a.ticket();
    let b_ticket = b.ticket();
    let a_client = a.client().unwrap();
    let extra = a_client.clone();
    let b_client = b.client().unwrap();
    assert_eq!(a_client.endpoint_id(), a_id);
    assert_eq!(
        extra.inner.endpoint.as_ref().unwrap().id(),
        a_client.inner.endpoint.as_ref().unwrap().id()
    );
    let (a_stop, a_stopped) = tokio::sync::oneshot::channel();
    let (b_stop, b_stopped) = tokio::sync::oneshot::channel();
    let a_binding = test_peer_binding(&b_id);
    let b_binding = test_peer_binding(&a_id);
    let task_a = tokio::spawn(async move {
        tokio::select! { _ = a_stopped => {}, _ = a.serve_concurrent_with_call_handler(MctIrohServeState::new(), vec![a_binding], MctIrohConcurrentServeConfig::new(test_observation_sink().clone()), || Timestamp::new("2026-05-31T00:00:02Z").unwrap(), |_,_,_| async { MctIrohCallHandlerResult::denied() }) => {} }
        a.close().await;
    });
    let task_b = tokio::spawn(async move {
        tokio::select! { _ = b_stopped => {}, _ = b.serve_concurrent_with_call_handler(MctIrohServeState::new(), vec![b_binding], MctIrohConcurrentServeConfig::new(test_observation_sink().clone()), || Timestamp::new("2026-05-31T00:00:02Z").unwrap(), |_,_,_| async { MctIrohCallHandlerResult::denied() }) => {} }
        b.close().await;
    });
    let trace = TraceId::new("trace-l9").unwrap();
    let hello_a = test_hello_request(&a_id, &trace);
    let hello_b = test_hello_request(&b_id, &trace);
    let (outbound, inbound) = tokio::join!(
        a_client.send_hello(&b_ticket, &hello_a),
        b_client.send_hello(&a_ticket, &hello_b)
    );
    assert_eq!(outbound.unwrap().hello_outcome, HelloOutcome::Admitted);
    assert_eq!(inbound.unwrap().hello_outcome, HelloOutcome::Admitted);
    drop(extra);
    assert_eq!(
        a_client
            .send_hello(&b_ticket, &hello_a)
            .await
            .unwrap()
            .hello_outcome,
        HelloOutcome::Admitted
    );
    a_stop.send(()).unwrap();
    task_a.await.unwrap();
    assert!(a_client.inner.endpoint.as_ref().unwrap().is_closed());
    assert!(a_client.send_hello(&b_ticket, &hello_a).await.is_err());
    b_stop.send(()).unwrap();
    task_b.await.unwrap();
}

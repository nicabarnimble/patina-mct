//! H2: real A -> B -> C topology; only a trusted local B call may take B -> C.
use super::*;

#[tokio::test]
async fn remote_payload_provenance_cannot_mint_or_accept_a_local_context() {
    let dir = tempfile::tempdir().unwrap();
    let paths = ResidentRuntimePaths::new(
        dir.path().join("config.json"),
        dir.path().join("children"),
        dir.path().join("state.sqlite"),
    );
    let (request, _) = jvm_bridge_protocol_request(
        "patina:demo/control@0.1.0.run",
        "[]",
        test_grants_authority_identity(1),
    )
    .unwrap();
    for origin in [
        CallOrigin::Cli,
        CallOrigin::JvmAdapter,
        CallOrigin::WasmHost,
        CallOrigin::ProcessHarness,
        CallOrigin::TriggerFiring,
    ] {
        let mut request = request.clone();
        request.call.origin = origin;
        let remote =
            ResidentPayloadIngress::remote(IrohVerifiedInlineBytes::from_serve_callback(None));
        let result = execute_resident_call(
            paths.clone(),
            ResidentLedgerWriter::failed_for_test(),
            request.clone(),
            remote.clone(),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Denied);
        // Even an explicitly supplied local context cannot accompany a
        // transport-proven Remote payload, including with no idempotency key.
        let context = ResidentCallIngressContext::LocalPrincipal {
            origin,
            caller: request.call.caller.clone(),
        };
        let result = execute_resident_call_with_context(
            paths.clone(),
            ResidentLedgerWriter::failed_for_test(),
            request,
            remote,
            context,
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Denied);
        assert_eq!(result.retry_directive, CallProtocolRetryDirective::None);
    }
    assert!(
        !paths.state_path().exists(),
        "rejected ingress must not consume a retry budget"
    );
}

struct OriginMother {
    root: PathBuf,
    identity: MctLocalNodeIdentity,
}

impl OriginMother {
    fn new(root: PathBuf, name: &str, echo: bool) -> Self {
        let store = MctDaemonConfigStore::new(root.join("config.json"));
        let identity = store
            .ensure_local_identity(
                MctOperatorNodeScope {
                    node_id: MctNodeId::new(name).unwrap(),
                    vision_id: VisionId::new("vision-local").unwrap(),
                    policy_revision: 1,
                },
                root.join("identity/iroh-secret.hex"),
            )
            .unwrap();
        let children = root.join("children");
        if echo {
            write_resident_payload_wasm_child(&children);
        } else {
            write_resident_wasm_child(&children);
        }
        let loaded = load_children_from_dir(MctChildLoadOptions::new(&children));
        assert_eq!(loaded.loaded, 1);
        store
            .approve_and_assign_loaded_child(
                &loaded.children[0],
                MctOperatorChildScope {
                    node_id: identity.node_id.clone(),
                    vision_id: identity.vision_id.clone(),
                    project_id: None,
                    policy_revision: 1,
                },
            )
            .unwrap();
        Self { root, identity }
    }

    fn store(&self) -> MctDaemonConfigStore {
        MctDaemonConfigStore::new(self.root.join("config.json"))
    }

    fn admit(
        &self,
        name: &str,
        endpoint: EndpointIdText,
        ticket: Option<MotherIrohEndpointTicket>,
    ) -> MctPeerAddressBookEntry {
        self.store()
            .upsert_peer(MctPeerAddressBookEntry {
                peer_node_id: MctNodeId::new(name).unwrap(),
                binding_id: PeerBindingId::new(format!("{}-admits-{name}", self.identity.node_id))
                    .unwrap(),
                endpoint_id: endpoint,
                vision_id: self.identity.vision_id.clone(),
                ticket,
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                expires_at: contract_peer_expiry(),
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();
        self.store().load().unwrap().peers[name].clone()
    }

    async fn start(
        &self,
    ) -> (
        MotherIrohEndpointTicket,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<Result<()>>,
    ) {
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(run_test_resident_mother(
            ResidentRuntimePaths::new(
                self.root.join("config.json"),
                self.root.join("children"),
                self.root.join("state.sqlite"),
            ),
            self.root.join("identity/iroh-secret.hex"),
            self.root.join("observations.jsonl"),
            self.root.join("control.sock"),
            async move {
                let _ = stop_rx.await;
            },
            Some(ready_tx),
        ));
        let ticket = tokio::time::timeout(Duration::from_secs(10), ready_rx)
            .await
            .unwrap()
            .unwrap();
        (ticket, stop_tx, task)
    }
}

#[tokio::test]
async fn three_mother_wire_origin_cannot_create_local_principal_or_extra_hop() {
    // A is the adversarial Mother endpoint, B and C run full resident loops,
    // durable admission/authorization, real WASM children, and owner-only UDS.
    let dir = tempfile::Builder::new()
        .prefix("mct-h2-")
        .tempdir_in("/tmp")
        .unwrap();
    let root = dir.path().to_path_buf();
    if std::env::var_os("MCT_ORIGIN_KEEP").is_some() {
        eprintln!("H2 fixtures retained: {}", root.display());
        let _ = dir.keep();
    }
    let mut a = MotherIrohEndpoint::bind_local_mct().await.unwrap();
    let a_endpoint = a.snapshot().endpoint_id;
    let b = OriginMother::new(root.join("b"), "mother-b", false);
    let c = OriginMother::new(root.join("c"), "mother-c", true);
    let b_admits_a = b.admit("mother-a", a_endpoint.clone(), None);
    let c_admits_b = c.admit("mother-b", b.identity.endpoint_id.clone(), None);
    let (c_ticket, c_stop, c_task) = c.start().await;
    b.admit("mother-c", c.identity.endpoint_id.clone(), Some(c_ticket));
    b.store()
        .set_peer_outbound_proof(
            &c.identity.node_id,
            MctOutboundPeerBindingPresentation {
                binding_id: c_admits_b.binding_id,
                policy_revision: 1,
                signature_ref: c_admits_b.binding_signature_ref.unwrap(),
                expires_at: contract_peer_expiry(),
            },
        )
        .unwrap();
    let (b_ticket, b_stop, b_task) = b.start().await;
    let target = OperationTarget {
        namespace: "patina:mct-test".into(),
        interface_name: "echo@0.1.0".into(),
        function_name: "echo".into(),
    };
    // Publication is a state barrier, not a delay: require B's real refresh to
    // discover C before testing terminal peer arrivals. No injected route cache.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let state = MctRuntimeStateStore::open(b.root.join("state.sqlite")).unwrap();
            if !state
                .fresh_remote_callable_surfaces_for_operation(
                    &b.identity.vision_id,
                    "patina:mct-test/echo@0.1.0.echo",
                    &current_timestamp(),
                )
                .unwrap()
                .is_empty()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("B must discover C's real publication");
    let trace = TraceId::new("trace-h2-hello").unwrap();
    let hello = a
        .send_hello(
            &b_ticket,
            &cli_hello_request(
                &a_endpoint,
                &b_admits_a.binding_id,
                &b_admits_a.peer_node_id,
                &b_admits_a.vision_id,
                &trace,
                b_admits_a.binding_signature_ref.clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(hello.hello_outcome, HelloOutcome::Admitted);
    let template = cli_call_request(
        &a_endpoint,
        &b_admits_a.binding_id,
        &b_admits_a.peer_node_id,
        &b_admits_a.vision_id,
        &trace,
        target.clone(),
        &hello,
    )
    .unwrap();
    let request_for = |label: &str| {
        let mut request = template.clone();
        request.call.call_id = CallId::new(format!("call-h2-{label}")).unwrap();
        request.protocol_request_id = ProtocolRequestId::new(format!("proto-h2-{label}")).unwrap();
        request.call.trace_context.trace_id = TraceId::new(format!("trace-h2-{label}")).unwrap();
        request.received_observation_id = ObservationId::new(format!("obs-h2-{label}")).unwrap();
        request.idempotency_key = Some(format!("key-h2-{label}"));
        request
    };
    // Direct peer A -> B succeeds on B's actual local child.
    let mut direct = request_for("direct-control");
    direct.call.target = OperationTarget {
        namespace: "patina:demo".into(),
        interface_name: "control@0.1.0".into(),
        function_name: "run".into(),
    };
    assert_eq!(
        a.send_call(&b_ticket, &direct).await.unwrap().reply_outcome,
        CallProtocolReplyOutcome::Success
    );
    let payload = b"[41]";
    // A legitimate local B principal may reach C over the exact route which
    // must remain unavailable to a peer arrival. Exercise actual UDS ingress.
    let local = request_for("local-control");
    let response = post_resident_peer_mutation(&b.root.join("control.sock"), "/calls", serde_json::json!({
        "protocol_request_id": local.protocol_request_id, "call_id": local.call.call_id,
        "target": local.call.target, "payload_metadata": { "data_classification": "public", "size_bytes": payload.len(), "contains_secret_scoped_material": false },
        "authority_context": local.call.authority_context, "deadline": local.call.deadline,
        "trace_context": local.call.trace_context,
        "payload": inline_result_payload_handle("payload-local", "application/json", payload),
        "inline_payload_base64": BASE64_STANDARD.encode(payload), "idempotency_key": local.idempotency_key,
    })).await;
    assert_eq!(response["outcome"], "completed", "{response}");
    assert!(
        response["route_taken"].to_string().contains("mother-c"),
        "{response}"
    );

    let mut attacks = Vec::new();
    for origin in [
        CallOrigin::Iroh,
        CallOrigin::Cli,
        CallOrigin::JvmAdapter,
        CallOrigin::WasmHost,
        CallOrigin::ProcessHarness,
        CallOrigin::TriggerFiring,
    ] {
        let mut request = request_for(&format!("attack-{origin:?}"));
        request.call.origin = origin;
        attacks.push(request);
    }
    let mut upstream = request_for("attack-upstream-node");
    upstream.call.caller.node_id = b.identity.node_id.clone();
    attacks.push(upstream);
    let mut user = request_for("attack-local-user");
    user.call.caller.user_id = Some(UserId::new("local-owner").unwrap());
    attacks.push(user);
    for attack in attacks {
        for local_target in [false, true] {
            if local_target
                && attack.call.origin == CallOrigin::Iroh
                && attack.call.caller.node_id == b_admits_a.peer_node_id
                && attack.call.caller.user_id.is_none()
            {
                continue; // legitimate A -> B already covered by the control
            }
            let mut request = attack.clone();
            request.call.call_id =
                CallId::new(format!("{}-{local_target}", request.call.call_id)).unwrap();
            request.idempotency_key = Some(request.call.call_id.to_string());
            let reply = if local_target {
                request.call.target = direct.call.target.clone();
                a.send_call(&b_ticket, &request).await.unwrap()
            } else {
                request.payload =
                    inline_result_payload_handle("payload-attack", "application/json", payload);
                request.call.payload_metadata.size_bytes = payload.len() as u64;
                a.send_call_with_inline_payload(&b_ticket, &request, payload.to_vec())
                    .await
                    .unwrap()
                    .reply
            };
            assert_eq!(
                reply.reply_outcome,
                CallProtocolReplyOutcome::Denied,
                "local_target={local_target} origin={:?}: {reply:?}",
                request.call.origin
            );
            assert!(reply.route_taken.is_none());
            assert_eq!(reply.retry_directive, CallProtocolRetryDirective::None);
        }
    }
    // Neither attacks nor the downstream positive control poisoned A's admission.
    direct.call.call_id = CallId::new("call-h2-direct-after").unwrap();
    direct.idempotency_key = Some("key-h2-direct-after".into());
    assert_eq!(
        a.send_call(&b_ticket, &direct).await.unwrap().reply_outcome,
        CallProtocolReplyOutcome::Success
    );
    a.close().await;
    b_stop.send(()).unwrap();
    c_stop.send(()).unwrap();
    for task in [b_task, c_task] {
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    let b_text = std::fs::read_to_string(b.root.join("observations.jsonl")).unwrap();
    let c_text = std::fs::read_to_string(c.root.join("observations.jsonl")).unwrap();
    assert!(
        b_text
            .lines()
            .any(|line| line.contains("call-h2-local-control") && line.contains("peer_call_sent"))
    );
    assert!(
        c_text
            .lines()
            .any(|line| line.contains("call-h2-local-control")
                && line.contains("runtime_execution_completed"))
    );
    assert!(
        !c_text.contains("call-h2-attack"),
        "no adversarial call may reach C"
    );
    assert!(
        !b_text
            .lines()
            .any(|line| line.contains("call-h2-attack") && line.contains("peer_call_sent"))
    );
    assert!(!b_text.lines().any(
        |line| line.contains("call-h2-attack") && line.contains("runtime_execution_completed")
    ));
}

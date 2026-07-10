use super::*;

pub(super) fn local_wasm_call(target: OperationTarget) -> MctCall {
    MctCall {
        call_id: CallId::new("call-cli-wasm")
            .expect("string ID literal/generated value must be non-empty"),
        caller: CallerIdentity {
            node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            user_id: None,
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            project_id: None,
        },
        target,
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
        deadline: current_timestamp_after(DEFAULT_CLI_CALL_DEADLINE),
        trace_context: TraceContext {
            trace_id: TraceId::new("trace-cli-wasm")
                .expect("string ID literal/generated value must be non-empty"),
            span_id: SpanId::new("span-cli-wasm")
                .expect("string ID literal/generated value must be non-empty"),
        },
        origin: CallOrigin::WasmHost,
    }
}

pub(super) fn local_process_call(target: OperationTarget, payload_size_bytes: u64) -> MctCall {
    MctCall {
        call_id: CallId::new("call-cli-process")
            .expect("string ID literal/generated value must be non-empty"),
        caller: CallerIdentity {
            node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            user_id: None,
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            project_id: None,
        },
        target,
        payload_metadata: PayloadMetadata {
            data_classification: "public".into(),
            size_bytes: payload_size_bytes,
            contains_secret_scoped_material: false,
        },
        authority_context: AuthorityContextSnapshot {
            policy_revision: 1,
            grants_revision: 1,
            vision_policy_revision: 1,
        },
        deadline: current_timestamp_after(DEFAULT_CLI_CALL_DEADLINE),
        trace_context: TraceContext {
            trace_id: TraceId::new("trace-cli-process")
                .expect("string ID literal/generated value must be non-empty"),
            span_id: SpanId::new("span-cli-process")
                .expect("string ID literal/generated value must be non-empty"),
        },
        origin: CallOrigin::ProcessHarness,
    }
}

pub(super) async fn run_jvm(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected jvm subcommand: call-json");
    }
    match args.remove(0).as_str() {
        "call-json" => run_jvm_call_json(args).await,
        other => bail!("unknown jvm subcommand '{other}'"),
    }
}

pub(super) async fn run_jvm_call_json(mut args: Vec<String>) -> Result<()> {
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    if args.len() < 2 {
        bail!(
            "expected: mct-daemon jvm call-json <operation-id> <args-json> [--children-dir path] [--config path] [--state path] [--ledger path]"
        );
    }
    let operation_id = args.remove(0);
    let args_json = args.remove(0);
    let (request, payload) = jvm_bridge_protocol_request(&operation_id, &args_json)?;
    let ledger = ResidentLedgerWriter::spawn(ledger_path.clone())?;
    let result = execute_resident_call(
        ResidentExecutionPaths {
            config_path,
            children_dir,
            state_path,
        },
        ledger.clone(),
        request,
        ResidentRequestPayload::remote(Some(payload)),
    )
    .await;
    ledger.close().await;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "outcome": result.outcome,
            "safe_message": result.safe_message,
            "result_ref": result.result_ref,
            "route_decision_id": result.route_decision_id,
            "route_taken": result.route_taken,
            "result_payload": result.result_payload,
            "inline_result_payload_base64": result.inline_result_payload.map(|bytes| BASE64_STANDARD.encode(bytes)),
        }))?
    );
    Ok(())
}

pub(super) fn jvm_bridge_protocol_request(
    operation_id: &str,
    args_json: &str,
) -> Result<(MctCallProtocolRequest, Vec<u8>)> {
    let payload_value: serde_json::Value = serde_json::from_str(args_json)
        .context("parse JVM bridge args JSON; expected a JSON array or object")?;
    let payload = serde_json::to_vec(&payload_value)?;
    let target = operation_target_from_wit_operation_id(operation_id)?;
    let suffix = mct_daemon::current_timestamp_string()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    let call_id = CallId::new(format!("call-jvm-bridge-{suffix}"))
        .expect("string ID literal/generated value must be non-empty");
    let trace_id = TraceId::new(format!("trace-jvm-bridge-{suffix}"))
        .expect("string ID literal/generated value must be non-empty");
    let span_id = SpanId::new(format!("span-jvm-bridge-{suffix}"))
        .expect("string ID literal/generated value must be non-empty");
    let protocol_request_id = ProtocolRequestId::new(format!("proto-jvm-bridge-{suffix}"))
        .expect("string ID literal/generated value must be non-empty");
    let call = MctCall {
        call_id: call_id.clone(),
        caller: CallerIdentity {
            node_id: MctNodeId::new("local-jvm-bridge")
                .expect("string ID literal/generated value must be non-empty"),
            user_id: None,
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            project_id: None,
        },
        target,
        payload_metadata: PayloadMetadata {
            data_classification: "public".into(),
            size_bytes: payload.len() as u64,
            contains_secret_scoped_material: false,
        },
        authority_context: AuthorityContextSnapshot {
            policy_revision: 1,
            grants_revision: 1,
            vision_policy_revision: 1,
        },
        deadline: current_timestamp_after(DEFAULT_CLI_CALL_DEADLINE),
        trace_context: TraceContext { trace_id, span_id },
        origin: CallOrigin::JvmAdapter,
    };
    Ok((
        MctCallProtocolRequest {
            protocol_request_id,
            authority: MctCallProtocolAuthority {
                hello_decision_id: DecisionId::new("decision-jvm-bridge-local")
                    .expect("string ID literal/generated value must be non-empty"),
                peer_binding_id: PeerBindingId::new("binding-jvm-bridge-local")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: call.caller.vision_id.clone(),
                accepted_alpn: MCT_CALL_ALPN.into(),
                endpoint_id: EndpointIdText::new("local-jvm-bridge")
                    .expect("string ID literal/generated value must be non-empty"),
                policy_revision: call.authority_context.policy_revision,
                grants_revision: call.authority_context.grants_revision,
            },
            received_over: IrohConnectionPresentation {
                endpoint_id: EndpointIdText::new("local-jvm-bridge")
                    .expect("string ID literal/generated value must be non-empty"),
                alpn: "jvm/bridge/0".into(),
                connection_side: ConnectionSide::Incoming,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            call,
            payload: MctCallPayloadHandle::InlinePayload {
                inline_payload_ref: format!("payload-jvm-bridge-{suffix}"),
                content_type: "application/json".into(),
                size_bytes: payload.len() as u64,
                blake3_digest_hex: blake3_hex(&payload),
            },
            idempotency_key: Some(format!("idem-jvm-bridge-{suffix}")),
            received_observation_id: ObservationId::new(format!(
                "obs-jvm-bridge-received-{suffix}"
            ))
            .expect("string ID literal/generated value must be non-empty"),
        },
        payload,
    ))
}

pub(super) async fn run_iroh(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected iroh subcommand: identity | serve | call");
    }
    match args.remove(0).as_str() {
        "identity" => {
            let config_path = take_option(&mut args, "--config")
                .map(PathBuf::from)
                .unwrap_or_else(default_config_path);
            let identity_path = args
                .first()
                .map(PathBuf::from)
                .unwrap_or_else(default_identity_path);
            let identity = MctDaemonConfigStore::new(&config_path)
                .ensure_local_identity(MctOperatorNodeScope::default(), &identity_path)?;
            println!("node_id={}", identity.node_id);
            println!("vision_id={}", identity.vision_id);
            println!("endpoint_id={}", identity.endpoint_id);
            println!("identity={}", identity.identity_path.display());
            println!("config={}", config_path.display());
        }
        "serve" => serve_iroh(args).await?,
        "serve-process" => serve_iroh_process(args).await?,
        "call" => call_iroh(args).await?,
        "call-peer" => call_iroh_peer(args).await?,
        other => bail!("unknown iroh subcommand '{other}'"),
    }
    Ok(())
}

pub(super) const DEFAULT_CLI_CALL_DEADLINE: jiff::SignedDuration =
    jiff::SignedDuration::from_secs(60);

pub(super) fn current_timestamp_after(budget: jiff::SignedDuration) -> Timestamp {
    let deadline = jiff::Timestamp::now()
        .checked_add(budget)
        .expect("CLI deadline budget is within jiff timestamp range");
    Timestamp::new(deadline.to_string()).expect("jiff produced RFC3339 timestamp")
}

pub(super) fn default_wasm_host_config() -> MctWasmHostConfig {
    MctWasmHostConfig {
        memory_limit_bytes: DEFAULT_WASM_MEMORY_LIMIT_BYTES,
    }
}

pub(super) async fn serve_iroh(mut args: Vec<String>) -> Result<()> {
    let relay_default = take_flag(&mut args, "--relay-default");
    if args.len() < 5 {
        bail!(
            "expected: mct-daemon iroh serve [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> [children-dir]"
        );
    }
    let identity_path = PathBuf::from(&args[0]);
    let binding_id = PeerBindingId::new(args[1].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let peer_endpoint_id = EndpointIdText::new(args[2].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let peer_node_id = MctNodeId::new(args[3].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let vision_id = VisionId::new(args[4].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let children_dir = args
        .get(5)
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);

    let secret_key_hex = load_or_create_node_secret_key_hex(&identity_path)?;
    let mut endpoint = MotherIrohEndpoint::bind(iroh_config(secret_key_hex, relay_default)).await?;
    let local_endpoint_id = endpoint.snapshot().endpoint_id;
    let ticket = endpoint.ticket();
    let load_report = load_children_from_dir(MctChildLoadOptions::new(children_dir));

    println!("mct iroh serving endpoint_id={local_endpoint_id}");
    println!("ticket={}", ticket.to_json()?.replace('\n', ""));
    println!(
        "children loaded={} failed={}",
        load_report.loaded, load_report.failed
    );

    let binding = cli_peer_binding(
        binding_id,
        peer_endpoint_id,
        peer_node_id,
        vision_id,
        identity_path,
        local_endpoint_id.clone(),
    );
    let result = endpoint
        .serve_concurrent_with_call_handler(
            MctIrohServeState::new(),
            vec![binding],
            MctIrohConcurrentServeConfig::default(),
            current_timestamp,
            |_, _, _| async {
                MctIrohCallHandlerResult::accepted_for_routing(Some(
                    ResultRef::new("result-mct-peer-call")
                        .expect("string ID literal/generated value must be non-empty"),
                ))
            },
        )
        .await;
    if let Err(error) = result {
        eprintln!("iroh serve error: {error}");
        endpoint.close().await;
        return Err(error.into());
    }
    Ok(())
}

pub(super) async fn serve_iroh_process(mut args: Vec<String>) -> Result<()> {
    let relay_default = take_flag(&mut args, "--relay-default");
    let child_name = take_option(&mut args, "--child").ok_or_else(|| {
        anyhow::anyhow!("iroh serve-process requires --child <approved-child-name>")
    })?;
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    if args.len() < 6 {
        bail!(
            "expected: mct-daemon iroh serve-process [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> <executable> --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]"
        );
    }
    let identity_path = PathBuf::from(&args[0]);
    let binding_id = PeerBindingId::new(args[1].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let peer_endpoint_id = EndpointIdText::new(args[2].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let peer_node_id = MctNodeId::new(args[3].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let vision_id = VisionId::new(args[4].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let executable = PathBuf::from(&args[5]);

    let secret_key_hex = load_or_create_node_secret_key_hex(&identity_path)?;
    let mut endpoint = MotherIrohEndpoint::bind(iroh_config(secret_key_hex, relay_default)).await?;
    let local_endpoint_id = endpoint.snapshot().endpoint_id;
    let ticket = endpoint.ticket();
    println!("mct iroh process serving endpoint_id={local_endpoint_id}");
    println!("ticket={}", ticket.to_json()?.replace('\n', ""));

    let binding = cli_peer_binding(
        binding_id,
        peer_endpoint_id,
        peer_node_id,
        vision_id,
        identity_path,
        local_endpoint_id.clone(),
    );
    let harness = MctProcessChildHarness {
        executable,
        args: Vec::new(),
        timeout: Duration::from_secs(5),
        local_node_id: MctNodeId::new("local-mct")
            .expect("string ID literal/generated value must be non-empty"),
    };
    let projection = load_configured_child_projection(&config_path, &children_dir)?;
    let result = endpoint
        .serve_concurrent_with_call_handler(
            MctIrohServeState::new(),
            vec![binding],
            MctIrohConcurrentServeConfig::default(),
            current_timestamp,
            move |request, _evaluation, _inline_payload| {
                let harness = harness.clone();
                let projection = projection.clone();
                let child_name = child_name.clone();
                let ledger_path = ledger_path.clone();
                let state_path = state_path.clone();
                async move {
                    let (authorized, authority_observation) =
                        match authorize_configured_child_from_projection(
                            &projection,
                            &child_name,
                            &request.call,
                        ) {
                            Ok(authorized) => authorized,
                            Err(error) => {
                                return MctIrohCallHandlerResult::failed(format!(
                                    "process child authority denied: {error}"
                                ));
                            }
                        };
                    let _ = append_ledger_observations(
                        &ledger_path,
                        std::slice::from_ref(&authority_observation),
                    );
                    let runtime_state = match MctRuntimeStateStore::open(&state_path) {
                        Ok(runtime_state) => runtime_state,
                        Err(error) => {
                            return MctIrohCallHandlerResult::failed(format!(
                                "runtime state unavailable: {error}"
                            ));
                        }
                    };
                    let run_id = run_id_for_call("iroh-process", &request.call);
                    let child_invocation_provenance = ChildInvocationProvenance::from_authorized(
                        &authorized,
                        authority_observation.observation_id.clone(),
                    );
                    if let Err(error) = runtime_state.insert_run_started(
                        &run_id,
                        &request.call,
                        RuntimeKind::Process,
                        Some(&child_invocation_provenance),
                        mct_daemon::current_timestamp_string(),
                    ) {
                        return MctIrohCallHandlerResult::failed(format!(
                            "runtime run could not start: {error}"
                        ));
                    }
                    let _ = runtime_state.append_run_observations(
                        &run_id,
                        std::slice::from_ref(&authority_observation),
                    );
                    let report = match harness.invoke_authorized_child(
                        authorized,
                        &request.call,
                        "{}",
                        MctProcessChildInvocationIds {
                            started_observation_id: ObservationId::new(format!(
                                "obs-iroh-process-started:{}",
                                request.call.call_id
                            ))
                            .expect("string ID literal/generated value must be non-empty"),
                            completed_observation_id: ObservationId::new(format!(
                                "obs-iroh-process-completed:{}",
                                request.call.call_id
                            ))
                            .expect("string ID literal/generated value must be non-empty"),
                            result_ref: ResultRef::new(format!(
                                "result-iroh-process:{}",
                                request.call.call_id
                            ))
                            .expect("string ID literal/generated value must be non-empty"),
                            audit_ref: AuditRef::new(format!(
                                "audit-iroh-process:{}",
                                request.call.call_id
                            ))
                            .expect("string ID literal/generated value must be non-empty"),
                            started_at: current_timestamp(),
                            completed_at: current_timestamp(),
                        },
                    ) {
                        Ok(report) => report,
                        Err(error) => {
                            return MctIrohCallHandlerResult::failed(format!(
                                "process child failed: {error}"
                            ));
                        }
                    };
                    let _ = append_ledger_observations(&ledger_path, &report.observations);
                    let _ = runtime_state.append_run_observations(&run_id, &report.observations);
                    let _ = runtime_state.complete_run(
                        &run_id,
                        &report.result,
                        mct_daemon::current_timestamp_string(),
                    );
                    match report.result.outcome {
                        ResultOutcome::Success => MctIrohCallHandlerResult::completed(
                            ResultRef::new(format!("result-iroh-process:{}", request.call.call_id))
                                .expect("string ID literal/generated value must be non-empty"),
                        ),
                        ResultOutcome::TimedOut => MctIrohCallHandlerResult::timed_out(),
                        ResultOutcome::Failed
                        | ResultOutcome::Denied
                        | ResultOutcome::Cancelled => {
                            MctIrohCallHandlerResult::failed(report.result.requester_message)
                        }
                    }
                }
            },
        )
        .await;
    if let Err(error) = result {
        eprintln!("iroh process serve error: {error}");
        endpoint.close().await;
        return Err(error.into());
    }
    Ok(())
}

pub(super) async fn call_iroh(mut args: Vec<String>) -> Result<()> {
    let relay_default = take_flag(&mut args, "--relay-default");
    let binding_signature_ref = take_option(&mut args, "--signature-ref");
    if args.len() < 5 {
        bail!(
            "expected: mct-daemon iroh call [--relay-default] <identity-file> <peer-ticket-file> <binding-id> <local-node-id> <vision-id> [namespace interface function] [--signature-ref proof]"
        );
    }
    let identity_path = PathBuf::from(&args[0]);
    let peer_ticket_path = PathBuf::from(&args[1]);
    let binding_id = PeerBindingId::new(args[2].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let local_node_id = MctNodeId::new(args[3].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let vision_id = VisionId::new(args[4].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let target = OperationTarget {
        namespace: args.get(5).cloned().unwrap_or_else(|| "patina".into()),
        interface_name: args.get(6).cloned().unwrap_or_else(|| "echo".into()),
        function_name: args.get(7).cloned().unwrap_or_else(|| "echo".into()),
    };

    let secret_key_hex = load_or_create_node_secret_key_hex(&identity_path)?;
    let mut endpoint = MotherIrohEndpoint::bind(iroh_config(secret_key_hex, relay_default)).await?;
    let local_endpoint_id = endpoint.snapshot().endpoint_id;
    let peer_ticket = read_ticket(&peer_ticket_path)?;
    let trace_id = TraceId::new("trace-cli-iroh-call")
        .expect("string ID literal/generated value must be non-empty");
    let hello_request = cli_hello_request(
        &local_endpoint_id,
        &binding_id,
        &local_node_id,
        &vision_id,
        &trace_id,
        binding_signature_ref,
    );
    let hello_response = endpoint.send_hello(&peer_ticket, &hello_request).await?;
    println!("{}", serde_json::to_string_pretty(&hello_response)?);

    let call_request = cli_call_request(
        &local_endpoint_id,
        &binding_id,
        &local_node_id,
        &vision_id,
        &trace_id,
        target,
        &hello_response,
    );
    let call_reply = endpoint.send_call(&peer_ticket, &call_request).await?;
    println!("{}", serde_json::to_string_pretty(&call_reply)?);
    endpoint.close().await;
    Ok(())
}

pub(super) async fn call_iroh_peer(mut args: Vec<String>) -> Result<()> {
    let relay_default = take_flag(&mut args, "--relay-default");
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    if args.len() < 2 {
        bail!(
            "expected: mct-daemon iroh call-peer [--relay-default] <identity-file> <peer-node-id> [namespace interface function] [--config path] [--children-dir path] [--state path]"
        );
    }
    let identity_path = PathBuf::from(args.remove(0));
    let peer_node_id = MctNodeId::new(args.remove(0))
        .expect("string ID literal/generated value must be non-empty");
    let target = OperationTarget {
        namespace: args.first().cloned().unwrap_or_else(|| "patina".into()),
        interface_name: args.get(1).cloned().unwrap_or_else(|| "echo".into()),
        function_name: args.get(2).cloned().unwrap_or_else(|| "echo".into()),
    };
    let config = MctDaemonConfigStore::new(&config_path).load()?;
    let capability_view =
        local_hello_capability_view_from_config(&config, &state_path, &children_dir)?;
    let peer = config.peers.get(peer_node_id.as_str()).ok_or_else(|| {
        anyhow::anyhow!(
            "peer '{peer_node_id}' not found in {}",
            config_path.display()
        )
    })?;
    let peer_ticket = peer
        .ticket
        .clone()
        .ok_or_else(|| anyhow::anyhow!("peer '{peer_node_id}' has no endpoint ticket"))?;

    let secret_key_hex = load_or_create_node_secret_key_hex(&identity_path)?;
    let mut endpoint = MotherIrohEndpoint::bind(iroh_config(secret_key_hex, relay_default)).await?;
    let local_endpoint_id = endpoint.snapshot().endpoint_id;
    let trace_id = TraceId::new("trace-cli-iroh-call-peer")
        .expect("string ID literal/generated value must be non-empty");
    let hello_request = cli_hello_request_with_capability_view(
        &local_endpoint_id,
        &peer.binding_id,
        &MctNodeId::new("local-mct").expect("string ID literal/generated value must be non-empty"),
        &peer.vision_id,
        &trace_id,
        peer.binding_signature_ref.clone(),
        capability_view,
    );
    let hello_response = endpoint.send_hello(&peer_ticket, &hello_request).await?;
    refresh_remote_surfaces_from_admitted_hello_response(
        &state_path,
        peer,
        &hello_response,
        current_timestamp(),
    )?;
    println!("{}", serde_json::to_string_pretty(&hello_response)?);

    let call_request = cli_call_request(
        &local_endpoint_id,
        &peer.binding_id,
        &MctNodeId::new("local-mct").expect("string ID literal/generated value must be non-empty"),
        &peer.vision_id,
        &trace_id,
        target,
        &hello_response,
    );
    let call_reply = endpoint.send_call(&peer_ticket, &call_request).await?;
    println!("{}", serde_json::to_string_pretty(&call_reply)?);
    endpoint.close().await;
    Ok(())
}

pub(super) fn cli_hello_request(
    endpoint_id: &EndpointIdText,
    binding_id: &PeerBindingId,
    node_id: &MctNodeId,
    vision_id: &VisionId,
    trace_id: &TraceId,
    signature_ref: Option<String>,
) -> MctHelloRequest {
    cli_hello_request_with_capability_view(
        endpoint_id,
        binding_id,
        node_id,
        vision_id,
        trace_id,
        signature_ref,
        None,
    )
}

pub(super) fn cli_hello_request_with_capability_view(
    endpoint_id: &EndpointIdText,
    binding_id: &PeerBindingId,
    node_id: &MctNodeId,
    vision_id: &VisionId,
    trace_id: &TraceId,
    signature_ref: Option<String>,
    capability_view: Option<MctHelloCapabilityView>,
) -> MctHelloRequest {
    MctHelloRequest {
        hello_id: "hello-cli".into(),
        received_over: IrohConnectionPresentation {
            endpoint_id: endpoint_id.clone(),
            alpn: MCT_HELLO_ALPN.into(),
            connection_side: ConnectionSide::Outgoing,
            path_class: PathClass::Direct,
            relay_url: None,
            presented_capability_ref: None,
        },
        requested_protocol: HelloPolicy::default().protocol,
        requested_vision_id: Some(vision_id.clone()),
        requested_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
        presented_binding: MctPeerBindingPresentation {
            binding_id: Some(binding_id.clone()),
            endpoint_id: endpoint_id.clone(),
            mct_node_id: Some(node_id.clone()),
            vision_id: Some(vision_id.clone()),
            policy_revision: Some(1),
            allowed_alpns_claim: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            signature_ref,
            expires_at: None,
        },
        capability_view,
        local_policy_revision_seen: Some(1),
        trace_id: trace_id.clone(),
        received_observation_id: ObservationId::new("obs-cli-hello-received")
            .expect("string ID literal/generated value must be non-empty"),
    }
}

pub(super) fn cli_call_request(
    endpoint_id: &EndpointIdText,
    binding_id: &PeerBindingId,
    node_id: &MctNodeId,
    vision_id: &VisionId,
    trace_id: &TraceId,
    target: OperationTarget,
    hello: &MctHelloResponse,
) -> MctCallProtocolRequest {
    let call = MctCall {
        call_id: CallId::new("call-cli-iroh")
            .expect("string ID literal/generated value must be non-empty"),
        caller: CallerIdentity {
            node_id: node_id.clone(),
            user_id: None,
            vision_id: vision_id.clone(),
            project_id: None,
        },
        target,
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
        deadline: current_timestamp_after(DEFAULT_CLI_CALL_DEADLINE),
        trace_context: TraceContext {
            trace_id: trace_id.clone(),
            span_id: SpanId::new("span-cli-call")
                .expect("string ID literal/generated value must be non-empty"),
        },
        origin: CallOrigin::Iroh,
    };

    MctCallProtocolRequest {
        protocol_request_id: ProtocolRequestId::new("proto-cli-call")
            .expect("string ID literal/generated value must be non-empty"),
        authority: MctCallProtocolAuthority {
            hello_decision_id: hello.decision_id.clone(),
            peer_binding_id: binding_id.clone(),
            vision_id: vision_id.clone(),
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
        idempotency_key: Some("idem-cli-call".into()),
        received_observation_id: ObservationId::new("obs-cli-call-received")
            .expect("string ID literal/generated value must be non-empty"),
    }
}

pub(super) fn load_configured_child_projection(
    config_path: &Path,
    children_dir: &Path,
) -> Result<MctConfigChildAuthorityProjection> {
    let config = MctDaemonConfigStore::new(config_path).load()?;
    let load_report = load_children_from_dir(MctChildLoadOptions::new(children_dir));
    Ok(config.authority_projection_for_loaded_children(
        load_report.children.iter(),
        MctOperatorChildScope::default(),
    ))
}

pub(super) fn authorize_configured_child_for_call(
    config_path: &Path,
    children_dir: &Path,
    child_name: &str,
    call: &MctCall,
) -> Result<(AuthorizedChildInvocation, MctObservation)> {
    let projection = load_configured_child_projection(config_path, children_dir)?;
    authorize_configured_child_from_projection(&projection, child_name, call)
}

pub(super) fn authorize_configured_child_from_projection(
    projection: &MctConfigChildAuthorityProjection,
    child_name: &str,
    call: &MctCall,
) -> Result<(AuthorizedChildInvocation, MctObservation)> {
    let result = projection.authorize_child_for_call(child_name, call);
    let observation = child_call_authority_observation(
        call.trace_context.trace_id.clone(),
        current_timestamp(),
        &result.evaluation,
    );
    let authorized = result.authorized.ok_or_else(|| {
        anyhow::anyhow!(
            "child '{child_name}' not authorized for {}.{}.{}: {:?}",
            call.target.namespace,
            call.target.interface_name,
            call.target.function_name,
            result.evaluation.reason_code
        )
    })?;
    Ok((authorized, observation))
}

pub(super) fn ensure_wasm_component_matches_loaded_child(
    children_dir: &Path,
    child_name: &str,
    component_path: &Path,
) -> Result<()> {
    let load_report = load_children_from_dir(MctChildLoadOptions::new(children_dir));
    let child = load_report
        .children
        .iter()
        .find(|child| child.name == child_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "child '{child_name}' not found in {}",
                children_dir.display()
            )
        })?;
    let expected = child
        .wasm_path
        .canonicalize()
        .unwrap_or_else(|_| child.wasm_path.clone());
    let actual = component_path
        .canonicalize()
        .unwrap_or_else(|_| component_path.to_path_buf());
    if expected != actual {
        bail!(
            "wasm component {} does not match approved child '{}' artifact {}",
            component_path.display(),
            child_name,
            child.wasm_path.display()
        );
    }
    Ok(())
}

pub(super) fn append_ledger_observations(
    ledger_path: &Path,
    observations: &[MctObservation],
) -> Result<()> {
    if observations.is_empty() {
        return Ok(());
    }
    let mut ledger = JsonlObservationLedger::open(ledger_path, "ledger-local", "local-mct")?;
    ledger.append_batch_before_effect(
        observations.iter().cloned(),
        mct_daemon::current_timestamp_string(),
    )?;
    Ok(())
}

pub(super) fn run_id_for_call(prefix: &str, call: &MctCall) -> String {
    format!(
        "run:{}:{}:{}",
        prefix,
        call.call_id,
        mct_daemon::current_timestamp_string()
    )
}

pub(super) fn default_observation_ledger_path() -> PathBuf {
    PathBuf::from(".mct").join("observations.jsonl")
}

pub(super) fn iroh_config(secret_key_hex: String, relay_default: bool) -> MotherIrohEndpointConfig {
    let mut config = MotherIrohEndpointConfig::local_mct().with_secret_key_hex(secret_key_hex);
    if relay_default {
        config = config.with_relay_mode(MotherIrohRelayMode::Default);
    }
    config
}

pub(super) fn cli_peer_binding(
    binding_id: PeerBindingId,
    endpoint_id: EndpointIdText,
    peer_node_id: MctNodeId,
    vision_id: VisionId,
    identity_path: PathBuf,
    local_endpoint_id: EndpointIdText,
) -> MctPeerBinding {
    let local_identity = MctLocalNodeIdentity {
        node_id: MctNodeId::new("local-mct")
            .expect("string ID literal/generated value must be non-empty"),
        vision_id: VisionId::new("vision-local")
            .expect("string ID literal/generated value must be non-empty"),
        endpoint_id: local_endpoint_id,
        identity_path,
        policy_revision: 1,
        updated_at: mct_daemon::current_timestamp_string(),
    };
    MctPeerAddressBookEntry {
        peer_node_id,
        binding_id,
        endpoint_id,
        vision_id,
        ticket: None,
        binding_signature_ref: None,
        outbound_binding: None,
        binding_state: BindingState::Admitted,
        policy_revision: 1,
        updated_at: local_identity.updated_at.clone(),
    }
    .to_peer_binding(&local_identity)
    .expect("CLI peer binding timestamp is generated as RFC3339")
}

pub(super) fn read_ticket(path: &Path) -> Result<MotherIrohEndpointTicket> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("reading peer ticket {}", path.display()))?;
    MotherIrohEndpointTicket::from_json(&json).map_err(Into::into)
}

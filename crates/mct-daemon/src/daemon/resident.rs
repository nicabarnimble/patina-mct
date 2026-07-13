use super::*;

#[path = "resident/observation.rs"]
mod observation;
pub(super) use observation::*;

#[path = "resident/payload.rs"]
mod payload;
pub(super) use payload::*;

#[path = "resident/publication.rs"]
mod publication;
pub(super) use publication::*;

#[path = "resident/idempotency.rs"]
mod idempotency;
pub(super) use idempotency::*;

#[path = "resident/candidates.rs"]
mod candidates;
use candidates::*;

#[path = "resident/decision.rs"]
mod decision;
use decision::*;

#[path = "resident/execution.rs"]
mod execution;
use execution::*;

pub(super) async fn run_serve(mut args: Vec<String>) -> Result<()> {
    let relay_default = take_flag(&mut args, "--relay-default");
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let identity_path = take_option(&mut args, "--identity")
        .map(PathBuf::from)
        .unwrap_or_else(default_identity_path);
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let max_concurrent_connections = take_option(&mut args, "--max-connections")
        .map(|value| value.parse::<usize>())
        .transpose()
        .context("parse --max-connections")?
        .unwrap_or(64);
    let http_addr = take_option(&mut args, "--http");
    let uds_path = take_option(&mut args, "--uds").map(PathBuf::from);
    if !args.is_empty() {
        bail!("unexpected serve arguments: {}", args.join(" "));
    }
    let control = match (http_addr, uds_path) {
        (Some(addr), None) => ResidentControlTransport::Http(addr),
        (None, Some(path)) => ResidentControlTransport::Uds(path),
        (None, None) => ResidentControlTransport::Http("127.0.0.1:9173".into()),
        (Some(_), Some(_)) => bail!("serve accepts only one control transport: --http or --uds"),
    };
    run_resident_mother(
        ResidentMotherConfig {
            config_path,
            identity_path,
            children_dir,
            state_path,
            ledger_path,
            control,
            relay_default,
            max_concurrent_connections,
        },
        resident_shutdown_signal(),
        None,
    )
    .await
}

#[derive(Clone, Debug)]
pub(super) enum ResidentControlTransport {
    Http(String),
    Uds(PathBuf),
}

#[derive(Clone, Debug)]
pub(super) struct ResidentMotherConfig {
    pub(super) config_path: PathBuf,
    pub(super) identity_path: PathBuf,
    pub(super) children_dir: PathBuf,
    pub(super) state_path: PathBuf,
    pub(super) ledger_path: PathBuf,
    pub(super) control: ResidentControlTransport,
    pub(super) relay_default: bool,
    pub(super) max_concurrent_connections: usize,
}

#[derive(Clone, Debug)]
pub(super) struct ResidentStatusSource {
    pub(super) endpoint: Arc<Mutex<MotherIrohEndpointSnapshot>>,
    pub(super) accepted_connection_count: Arc<AtomicU64>,
    pub(super) loaded_child_count: usize,
    pub(super) approved_child_count: usize,
    pub(super) binding_count: usize,
    pub(super) ledger_path: PathBuf,
}

impl ResidentStatusSource {
    pub(super) fn status(&self) -> MctDaemonStatus {
        daemon_status_with_resident(
            Some(
                self.endpoint
                    .lock()
                    .expect("resident endpoint status lock must not be poisoned")
                    .clone(),
            ),
            Some(MctResidentStatus {
                accepted_connection_count: self.accepted_connection_count.load(Ordering::SeqCst),
                loaded_child_count: self.loaded_child_count,
                approved_child_count: self.approved_child_count,
                binding_count: self.binding_count,
                ledger_sequence_tip: ledger_sequence_tip(&self.ledger_path),
            }),
        )
    }
}

pub(super) fn ledger_sequence_tip(path: &Path) -> u64 {
    JsonlObservationLedger::open_read_only(path, "ledger-local", "local-mct")
        .and_then(|reader| reader.entries())
        .ok()
        .and_then(|entries| entries.last().map(|entry| entry.local_sequence))
        .unwrap_or(0)
}

pub(super) async fn run_resident_mother<S>(
    config: ResidentMotherConfig,
    shutdown: S,
    ready: Option<tokio::sync::oneshot::Sender<MotherIrohEndpointTicket>>,
) -> Result<()>
where
    S: std::future::Future<Output = ()> + Send,
{
    if config.max_concurrent_connections == 0 {
        bail!("--max-connections must be greater than zero");
    }

    let ledger = ResidentLedgerWriter::spawn(config.ledger_path.clone())?;
    let config_store = MctDaemonConfigStore::new(&config.config_path);
    let existing_config = config_store.load()?;
    let identity_scope = existing_config
        .local_identity
        .as_ref()
        .map(|identity| MctOperatorNodeScope {
            node_id: identity.node_id.clone(),
            vision_id: identity.vision_id.clone(),
            policy_revision: identity.policy_revision,
        })
        .unwrap_or_default();
    let identity = ensure_observed_local_identity(
        &config_store,
        identity_scope,
        &config.identity_path,
        &ledger,
    )
    .await?;
    let secret_key_hex = load_or_create_node_secret_key_hex(&config.identity_path)?;
    let mut endpoint = MotherIrohEndpoint::bind(iroh_config(secret_key_hex, config.relay_default))
        .await
        .context("bind resident Mother Iroh endpoint")?;
    let snapshot = endpoint.snapshot();
    if snapshot.endpoint_id != identity.endpoint_id {
        bail!(
            "identity endpoint mismatch: config has {}, bound endpoint is {}",
            identity.endpoint_id,
            snapshot.endpoint_id
        );
    }
    let ticket = endpoint.ticket();
    let load_report = load_children_from_dir(MctChildLoadOptions::new(config.children_dir.clone()));
    let state = MctRuntimeStateStore::open(&config.state_path)
        .with_context(|| format!("open runtime state {}", config.state_path.display()))?;
    let runtime_summary = state.summary()?;
    drop(state);

    let loaded_child_count = load_report.loaded;
    let resident_config = config_store.load()?;
    let hello_capability_view = resident_hello_capability_view(
        &resident_config,
        &runtime_summary,
        &identity,
        &load_report.children,
    );
    let approved_child_count = resident_config
        .child_approvals
        .values()
        .filter(|approval| approval.approval_state == ChildApprovalState::Approved)
        .count();
    let binding_count = resident_config.peers.len();
    let accepted_connection_count = Arc::new(AtomicU64::new(0));
    let endpoint_status = Arc::new(Mutex::new(snapshot.clone()));
    let status_source = Arc::new(ResidentStatusSource {
        endpoint: Arc::clone(&endpoint_status),
        accepted_connection_count: Arc::clone(&accepted_connection_count),
        loaded_child_count,
        approved_child_count,
        binding_count,
        ledger_path: config.ledger_path.clone(),
    });

    let (events, event_rx) = tokio::sync::mpsc::channel(256);
    let event_ledger = ledger.clone();
    let event_accepted_count = Arc::clone(&accepted_connection_count);
    let event_state_path = config.state_path.clone();
    let event_task = tokio::spawn(async move {
        record_iroh_serve_events(
            event_rx,
            event_ledger,
            event_accepted_count,
            event_state_path,
        )
        .await
    });

    let (shutdown_tx, _) = broadcast::channel(4);
    let control_task = spawn_resident_control_task(
        config.control.clone(),
        config.state_path.clone(),
        config.config_path.clone(),
        config.children_dir.clone(),
        ledger.clone(),
        shutdown_tx.subscribe(),
        Some(status_source),
    )?;

    println!("mct resident mother endpoint_id={}", snapshot.endpoint_id);
    println!("ticket={}", ticket.to_json()?.replace('\n', ""));
    eprintln!(
        "mct resident mother children loaded={} failed={} bindings={} max_connections={}",
        loaded_child_count, load_report.failed, binding_count, config.max_concurrent_connections
    );
    if let Some(ready) = ready {
        let _ = ready.send(ticket.clone());
    }

    let config_path = config.config_path.clone();
    let execution_paths = ResidentExecutionPaths {
        config_path: config.config_path.clone(),
        children_dir: config.children_dir.clone(),
        state_path: config.state_path.clone(),
    };
    let execution_ledger = ledger.clone();
    let observation_sink = resident_iroh_observation_sink(ledger.clone());
    let serve_result = tokio::select! {
        result = endpoint.serve_concurrent_with_binding_provider(
            MctIrohServeState::new(),
            MctIrohConcurrentServeConfig {
                max_concurrent_connections: config.max_concurrent_connections,
                events: Some(events),
                require_binding_signature: true,
                capability_view: Some(hello_capability_view),
                ..MctIrohConcurrentServeConfig::new(observation_sink)
            },
            current_timestamp,
            move || {
                let config_path = config_path.clone();
                async move { load_peer_bindings_for_iroh(config_path).await }
            },
            move |request, _evaluation, inline_payload| {
                let execution_paths = execution_paths.clone();
                let execution_ledger = execution_ledger.clone();
                async move {
                    execute_resident_call(
                        execution_paths,
                        execution_ledger,
                        request,
                        ResidentPayloadIngress::remote(inline_payload),
                    )
                    .await
                }
            },
        ) => result.map_err(anyhow::Error::from),
        _ = shutdown => Ok(()),
    };

    let _ = shutdown_tx.send(());
    endpoint.close().await;
    if let Ok(mut endpoint_status) = endpoint_status.lock() {
        *endpoint_status = endpoint.snapshot();
    }
    if let Err(error) = ledger
        .append(vec![resident_endpoint_observation(
            "obs-resident-mother-endpoint-closed",
            snapshot.endpoint_id.clone(),
            ObservationOutcome::Completed,
            "resident Mother endpoint closed",
        )])
        .await
    {
        eprintln!("ledger shutdown observation failed: {error}");
    }
    let _ = tokio::time::timeout(Duration::from_secs(2), event_task).await;
    control_task.abort();
    ledger.close().await;
    if let ResidentControlTransport::Uds(path) = &config.control {
        let _ = std::fs::remove_file(path);
    }
    serve_result
}

pub(super) fn spawn_resident_control_task(
    control: ResidentControlTransport,
    state_path: PathBuf,
    config_path: PathBuf,
    children_dir: PathBuf,
    ledger: ResidentLedgerWriter,
    shutdown: broadcast::Receiver<()>,
    status_source: Option<Arc<ResidentStatusSource>>,
) -> Result<tokio::task::JoinHandle<Result<()>>> {
    match control {
        ResidentControlTransport::Http(addr) => Ok(tokio::spawn(async move {
            serve_http_control_loop_until(state_path, addr, shutdown, status_source).await
        })),
        ResidentControlTransport::Uds(path) => Ok(tokio::spawn(async move {
            run_control_serve_uds_with_state_until(
                state_path,
                path,
                config_path,
                children_dir,
                ledger,
                shutdown,
                status_source,
            )
            .await
        })),
    }
}

pub(super) async fn load_peer_bindings_for_iroh(
    path: PathBuf,
) -> mct_iroh::MotherIrohEndpointResult<MctPeerAuthoritySnapshot> {
    tokio::task::spawn_blocking(move || {
        MctDaemonConfigStore::new(path)
            .load()
            .and_then(|config| config.peer_authority_projection())
    })
    .await
    .map_err(|source| MotherIrohEndpointError::ProtocolProvider {
        action: "join peer binding load",
        source: Box::new(source),
    })?
    .map(|projection| MctPeerAuthoritySnapshot {
        bindings: projection.bindings,
        policy_revision: projection.policy_revision,
    })
    .map_err(|source| MotherIrohEndpointError::ProtocolProvider {
        action: "load peer bindings",
        source: Box::new(std::io::Error::other(source.to_string())),
    })
}

pub(super) async fn resident_shutdown_signal() {
    #[cfg(unix)]
    {
        let mut interrupt =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .expect("install SIGINT handler");
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

pub(super) async fn serve_http_control_loop(state_path: &Path, addr: &str) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let snapshot_source = ControlSnapshotSource::open(state_path);
    println!("mct daemon serving control http on {addr}");
    loop {
        serve_http_control_once_with_snapshot_result(
            &listener,
            control_snapshot(&snapshot_source).await,
        )
        .await?;
    }
}

pub(super) async fn serve_http_control_loop_until(
    state_path: PathBuf,
    addr: String,
    mut shutdown: broadcast::Receiver<()>,
    status_source: Option<Arc<ResidentStatusSource>>,
) -> Result<()> {
    let listener = TcpListener::bind(&addr).await?;
    let snapshot_source = ControlSnapshotSource::open_with_status(&state_path, status_source);
    println!("mct daemon serving control http on {addr}");
    loop {
        tokio::select! {
            _ = shutdown.recv() => break,
            result = serve_http_control_once_with_snapshot_result(
                &listener,
                control_snapshot(&snapshot_source).await,
            ) => result?,
        }
    }
    Ok(())
}

pub(super) async fn record_iroh_serve_events(
    mut events: tokio::sync::mpsc::Receiver<MctIrohServeEvent>,
    ledger: ResidentLedgerWriter,
    accepted_connection_count: Arc<AtomicU64>,
    state_path: PathBuf,
) {
    while let Some(event) = events.recv().await {
        let observations = match event {
            MctIrohServeEvent::AcceptedConnection => {
                accepted_connection_count.fetch_add(1, Ordering::SeqCst);
                Vec::new()
            }
            MctIrohServeEvent::Served(served) => {
                let served = *served;
                if let MctIrohServedProtocol::Hello {
                    request,
                    evaluation,
                    ..
                } = &served
                    && let Err(error) = refresh_remote_surfaces_from_admitted_hello_request(
                        &state_path,
                        request,
                        evaluation,
                        current_timestamp(),
                    )
                {
                    eprintln!("resident remote surface refresh failed: {error}");
                }
                resident_observations_for_served_protocol(served)
            }
            MctIrohServeEvent::RefusedConnection => Vec::new(),
        };
        if let Err(error) = ledger.append(observations).await {
            eprintln!("resident ledger event write failed: {error}");
        }
    }
}

pub(super) fn resident_observations_for_served_protocol(
    _served: MctIrohServedProtocol,
) -> Vec<MctObservation> {
    // Hello and call lifecycle facts are written by the awaited mandatory sink.
    Vec::new()
}

#[derive(Clone, Debug)]
pub(super) struct ResidentExecutionPaths {
    pub(super) config_path: PathBuf,
    pub(super) children_dir: PathBuf,
    pub(super) state_path: PathBuf,
}

pub(super) fn resident_forwarded_call_sent_observation(
    call: &MctCall,
    candidate: &CandidateRoute,
    forwarded_from: &MctNodeId,
    forwarded_to: &MctNodeId,
) -> MctObservation {
    let operation_id = mct_daemon::operation_id_from_target(&call.target);
    MctObservation {
        observation_id: ObservationId::new(format!("obs-peer-call-sent:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::PeerCallSent,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: None,
        subject_id: Some(forwarded_from.to_string()),
        resource_id: Some(candidate.candidate_id.clone()),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome: ObservationOutcome::Started,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: "forwarding call to remote Mother".into(),
        detail_ref: Some(format!(
            "forwarded_from:{forwarded_from};forwarded_to:{forwarded_to};candidate:{};operation:{operation_id}",
            candidate.candidate_id
        )),
    }
}

pub(super) fn resident_remote_reply_observation(
    call: &MctCall,
    candidate: &CandidateRoute,
    forwarded_from: &MctNodeId,
    forwarded_to: &MctNodeId,
    reply: &MctCallProtocolReply,
) -> MctObservation {
    let outcome = match reply.reply_outcome {
        CallProtocolReplyOutcome::Success => ObservationOutcome::Completed,
        CallProtocolReplyOutcome::Denied | CallProtocolReplyOutcome::Malformed => {
            ObservationOutcome::Denied
        }
        CallProtocolReplyOutcome::Failed => ObservationOutcome::Failed,
        CallProtocolReplyOutcome::TimedOut => ObservationOutcome::TimedOut,
        CallProtocolReplyOutcome::Cancelled => ObservationOutcome::Cancelled,
    };
    MctObservation {
        observation_id: ObservationId::new(format!("obs-peer-call-replied:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::PeerCallReplied,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: Some(reply.decision_id.clone()),
        subject_id: Some(forwarded_to.to_string()),
        resource_id: Some(candidate.candidate_id.clone()),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: reply.safe_message.clone(),
        detail_ref: Some(format!(
            "forwarded_from:{forwarded_from};forwarded_to:{forwarded_to};candidate:{};remote_reply:{:?};remote_decision:{};remote_reply_id:{}",
            candidate.candidate_id, reply.reply_outcome, reply.decision_id, reply.reply_id
        )),
    }
}

pub(super) async fn execute_resident_call(
    paths: ResidentExecutionPaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    payload: ResidentPayloadIngress,
) -> MctIrohCallHandlerResult {
    execute_resident_call_at(paths, ledger, request, payload, current_timestamp()).await
}

pub(super) async fn execute_resident_call_at(
    paths: ResidentExecutionPaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    payload: ResidentPayloadIngress,
    now: Timestamp,
) -> MctIrohCallHandlerResult {
    let inline_payload = match resolve_resident_request_payload(&paths, &request, payload).await {
        Ok(payload) => payload.into_inner(),
        Err(report) => {
            let (safe_message, observations) = report.into_parts();
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident payload failure ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            return MctIrohCallHandlerResult::failed(safe_message);
        }
    };

    let state_path = paths.state_path.clone();
    let idempotency_request = request.clone();
    let idempotency_ledger = ledger.clone();
    execute_idempotent_call(
        state_path,
        idempotency_ledger,
        idempotency_request,
        now,
        move || execute_resident_call_after_payload(paths, ledger, request, inline_payload),
    )
    .await
}

async fn execute_resident_call_after_payload(
    paths: ResidentExecutionPaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    inline_payload: Option<Vec<u8>>,
) -> MctIrohCallHandlerResult {
    let authorization = match authorize_resident_child(paths.clone(), request.call.clone()).await {
        Ok(authorization) => authorization,
        Err(error) => {
            eprintln!("resident child authorization unavailable: {error}");
            return MctIrohCallHandlerResult::failed("runtime unavailable");
        }
    };

    match authorization {
        RouteDisposition::Denied {
            decision,
            observations,
        } => {
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident route denial ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            MctIrohCallHandlerResult::denied().with_route(Some(decision.decision_id), None)
        }
        RouteDisposition::Local { plan, observations } => {
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident route ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }

            let current_revisions = match current_resident_route_revisions(&paths, &request.call) {
                Ok(revisions) => revisions,
                Err(error) => {
                    eprintln!("resident route revision read failed: {error}");
                    return MctIrohCallHandlerResult::failed("runtime unavailable");
                }
            };
            let execution = match tokio::task::spawn_blocking(move || {
                execute_authorized_resident_child(
                    paths,
                    *plan,
                    request,
                    inline_payload,
                    current_revisions,
                )
            })
            .await
            {
                Ok(Ok(report)) => report,
                Ok(Err(error)) => {
                    eprintln!("resident child execution failed: {error}");
                    return MctIrohCallHandlerResult::failed("runtime execution failed");
                }
                Err(error) => {
                    eprintln!("resident child execution task failed: {error}");
                    return MctIrohCallHandlerResult::failed("runtime execution failed");
                }
            };

            let (result, observations, inline_result_payload) = execution.into_parts();
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident execution ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }

            result_to_call_handler_result("result-resident", &result, inline_result_payload)
        }
        RouteDisposition::Remote { plan, observations } => {
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident remote route ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            execute_authorized_resident_remote_call(paths, *plan, request, inline_payload, ledger)
                .await
        }
    }
}

pub(super) struct ResidentRemoteRevalidationAuthorized {
    pub(super) decision: RouteDecision,
    pub(super) peer: MctPeerAddressBookEntry,
    pub(super) local_identity: MctLocalNodeIdentity,
    pub(super) capability_view: Option<MctHelloCapabilityView>,
    pub(super) route_taken: RouteTaken,
}

pub(super) enum ResidentRemoteRevalidation {
    Authorized(Box<ResidentRemoteRevalidationAuthorized>),
    Denied(Box<RouteDecision>),
}

async fn execute_authorized_resident_remote_call(
    paths: ResidentExecutionPaths,
    execution: RemoteExecutionPlan,
    request: MctCallProtocolRequest,
    inline_payload: Option<Vec<u8>>,
    ledger: ResidentLedgerWriter,
) -> MctIrohCallHandlerResult {
    let revalidation = match revalidate_resident_remote_route(&paths, &execution, &request.call) {
        Ok(revalidation) => revalidation,
        Err(error) => {
            eprintln!("resident remote route revalidation failed: {error}");
            return MctIrohCallHandlerResult::failed("runtime unavailable");
        }
    };
    let revalidation_decision = match &revalidation {
        ResidentRemoteRevalidation::Authorized(authorized) => &authorized.decision,
        ResidentRemoteRevalidation::Denied(decision) => decision.as_ref(),
    };
    if let Err(error) = ledger
        .append(vec![route_decision_observation(
            request.call.trace_context.trace_id.clone(),
            current_timestamp(),
            revalidation_decision,
        )])
        .await
    {
        eprintln!("resident remote revalidation ledger write failed: {error}");
        return MctIrohCallHandlerResult::failed("observation ledger unavailable");
    }

    let ResidentRemoteRevalidation::Authorized(authorized) = revalidation else {
        return MctIrohCallHandlerResult::denied()
            .with_route(Some(revalidation_decision.decision_id.clone()), None);
    };

    let Some(peer_ticket) = authorized.peer.ticket.clone() else {
        return MctIrohCallHandlerResult::failed("remote peer unavailable");
    };
    let Some(outbound_binding) = authorized.peer.outbound_binding.clone() else {
        return MctIrohCallHandlerResult::denied()
            .with_route(Some(authorized.decision.decision_id.clone()), None);
    };

    let secret_key_hex =
        match load_or_create_node_secret_key_hex(&authorized.local_identity.identity_path) {
            Ok(secret_key_hex) => secret_key_hex,
            Err(error) => {
                eprintln!("resident remote identity load failed: {error}");
                return MctIrohCallHandlerResult::failed("runtime unavailable");
            }
        };
    let mut endpoint = match MotherIrohEndpoint::bind(iroh_config(secret_key_hex, false)).await {
        Ok(endpoint) => endpoint,
        Err(error) => {
            eprintln!("resident remote endpoint bind failed: {error}");
            return MctIrohCallHandlerResult::failed("remote peer unavailable");
        }
    };
    let local_endpoint_id = endpoint.snapshot().endpoint_id;
    if local_endpoint_id != authorized.local_identity.endpoint_id {
        endpoint.close().await;
        return MctIrohCallHandlerResult::failed("runtime unavailable");
    }
    if let Err(error) = ledger
        .append(vec![resident_forwarded_call_sent_observation(
            &request.call,
            execution.candidate(),
            &authorized.local_identity.node_id,
            &authorized.peer.peer_node_id,
        )])
        .await
    {
        endpoint.close().await;
        eprintln!("resident remote sent-observation ledger write failed: {error}");
        return MctIrohCallHandlerResult::failed("observation ledger unavailable");
    }

    let hello_request = resident_forwarding_hello_request(
        &local_endpoint_id,
        &authorized.local_identity,
        &authorized.peer,
        &outbound_binding,
        &request.call.trace_context.trace_id,
        authorized.capability_view,
    );
    let hello_response = match endpoint.send_hello(&peer_ticket, &hello_request).await {
        Ok(response) => response,
        Err(error) => {
            eprintln!("resident remote hello failed: {error}");
            endpoint.close().await;
            return MctIrohCallHandlerResult::failed("remote peer unavailable");
        }
    };
    if let Err(error) = refresh_remote_surfaces_from_admitted_hello_response(
        &paths.state_path,
        &authorized.peer,
        &hello_response,
        current_timestamp(),
    ) {
        eprintln!("resident remote hello response surface refresh failed: {error}");
    }
    if hello_response.hello_outcome != HelloOutcome::Admitted
        || !hello_response
            .accepted_alpns
            .iter()
            .any(|alpn| alpn == MCT_CALL_ALPN)
    {
        endpoint.close().await;
        return MctIrohCallHandlerResult::denied()
            .with_route(Some(authorized.decision.decision_id.clone()), None);
    }

    let forwarded_request = resident_forwarded_call_request(
        &request,
        &authorized.local_identity,
        &authorized.peer,
        &outbound_binding,
        &local_endpoint_id,
        &hello_response,
        inline_payload.as_deref(),
    );
    let call_reply = match inline_payload {
        Some(bytes) => {
            endpoint
                .send_call_with_inline_payload(&peer_ticket, &forwarded_request, bytes)
                .await
        }
        None => endpoint
            .send_call(&peer_ticket, &forwarded_request)
            .await
            .map(|reply| MctIrohCallPayloadReply {
                reply,
                inline_result_payload: None,
            }),
    };
    endpoint.close().await;
    match call_reply {
        Ok(reply) => {
            if let Err(error) = ledger
                .append(vec![resident_remote_reply_observation(
                    &request.call,
                    execution.candidate(),
                    &authorized.local_identity.node_id,
                    &authorized.peer.peer_node_id,
                    &reply.reply,
                )])
                .await
            {
                eprintln!("resident remote reply-observation ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            remote_reply_to_call_handler_result(
                reply,
                authorized.decision.decision_id,
                authorized.route_taken,
            )
        }
        Err(MotherIrohEndpointError::ProtocolPayload { safe_message, .. }) => {
            MctIrohCallHandlerResult::failed(safe_message).with_route(
                Some(authorized.decision.decision_id),
                Some(authorized.route_taken),
            )
        }
        Err(error) => {
            eprintln!("resident remote call failed: {error}");
            MctIrohCallHandlerResult::failed("remote peer unavailable").with_route(
                Some(authorized.decision.decision_id),
                Some(authorized.route_taken),
            )
        }
    }
}

fn revalidate_resident_remote_route(
    paths: &ResidentExecutionPaths,
    execution: &RemoteExecutionPlan,
    call: &MctCall,
) -> Result<ResidentRemoteRevalidation> {
    let config = MctDaemonConfigStore::new(&paths.config_path).load()?;
    let Some(local_identity) = config.local_identity.clone() else {
        return Ok(ResidentRemoteRevalidation::Denied(Box::new(
            remote_revalidation_denied_decision(
                call,
                execution.initial_decision(),
                execution.candidate().clone(),
                CandidateEliminationReason::PeerNotAdmitted,
            ),
        )));
    };
    let Some(peer) = config
        .peers
        .get(execution.candidate().node_id.as_str())
        .cloned()
    else {
        return Ok(ResidentRemoteRevalidation::Denied(Box::new(
            remote_revalidation_denied_decision(
                call,
                execution.initial_decision(),
                execution.candidate().clone(),
                CandidateEliminationReason::PeerNotAdmitted,
            ),
        )));
    };
    let state = MctRuntimeStateStore::open(&paths.state_path)?;
    let now = current_timestamp();
    let operation_id = mct_daemon::operation_id_from_target(&call.target);
    let surfaces = state.fresh_remote_callable_surfaces_for_operation(
        &call.caller.vision_id,
        &operation_id,
        &now,
    )?;
    let Some(surface) = surfaces.into_iter().find(|surface| {
        surface.peer_node_id == peer.peer_node_id
            && resident_candidate_for_remote_surface(&peer, surface)
                == execution.candidate().clone()
    }) else {
        return Ok(ResidentRemoteRevalidation::Denied(Box::new(
            remote_revalidation_denied_decision(
                call,
                execution.initial_decision(),
                execution.candidate().clone(),
                CandidateEliminationReason::CapabilityUnavailable,
            ),
        )));
    };
    let candidate = resident_candidate_for_remote_surface(&peer, &surface);
    let authority = resident_remote_candidate_authority(
        &local_identity,
        &peer,
        &surface,
        candidate.clone(),
        call,
        &now,
    )?;
    if authority.outcome != CandidateAuthorityOutcome::Admissible {
        return Ok(ResidentRemoteRevalidation::Denied(Box::new(
            remote_revalidation_decision(
                call,
                execution.initial_decision(),
                None,
                authority
                    .reason
                    .unwrap_or(CandidateEliminationReason::CapabilityUnavailable),
                authority,
            ),
        )));
    }
    let decision = remote_revalidation_decision(
        call,
        execution.initial_decision(),
        Some(candidate.clone()),
        CandidateEliminationReason::CapabilityUnavailable,
        authority,
    );
    let route_taken = RouteTaken {
        node_id: candidate.node_id,
        child_id: candidate.child_id,
        runtime_kind: candidate.runtime_kind,
    };
    let capability_view =
        local_hello_capability_view_from_config(&config, &paths.state_path, &paths.children_dir)?;
    Ok(ResidentRemoteRevalidation::Authorized(Box::new(
        ResidentRemoteRevalidationAuthorized {
            decision,
            peer,
            local_identity,
            capability_view,
            route_taken,
        },
    )))
}

pub(super) fn remote_revalidation_denied_decision(
    call: &MctCall,
    initial: &RouteDecision,
    candidate: CandidateRoute,
    reason: CandidateEliminationReason,
) -> RouteDecision {
    let authority = CandidateAuthorityEvaluation::eliminated(
        candidate,
        reason,
        call.authority_context.policy_revision,
        call.authority_context.grants_revision,
    );
    remote_revalidation_decision(call, initial, None, reason, authority)
}

pub(super) fn remote_revalidation_decision(
    call: &MctCall,
    initial: &RouteDecision,
    selected_route: Option<CandidateRoute>,
    no_route_reason: CandidateEliminationReason,
    authority: CandidateAuthorityEvaluation,
) -> RouteDecision {
    let ids = resident_route_revalidation_ids(call);
    let outcome = if selected_route.is_some() {
        RouteDecisionOutcome::RouteSelected
    } else {
        RouteDecisionOutcome::NoRoute
    };
    RouteDecision {
        decision_id: ids.decision_id,
        call_id: call.call_id.clone(),
        decision_kind: RouteDecisionKind::Revalidation,
        initial_decision_id: Some(initial.decision_id.clone()),
        authority_evaluations: vec![authority],
        selected_route,
        outcome,
        no_route_reason: (outcome == RouteDecisionOutcome::NoRoute).then_some(no_route_reason),
        safe_message: if outcome == RouteDecisionOutcome::RouteSelected {
            "route revalidated".into()
        } else {
            "no route available".into()
        },
        observation_id: ids.observation_id,
    }
}

pub(super) fn resident_forwarding_hello_request(
    endpoint_id: &EndpointIdText,
    local_identity: &MctLocalNodeIdentity,
    peer: &MctPeerAddressBookEntry,
    outbound_binding: &MctOutboundPeerBindingPresentation,
    trace_id: &TraceId,
    capability_view: Option<MctHelloCapabilityView>,
) -> MctHelloRequest {
    MctHelloRequest {
        hello_id: format!("hello-forward:{}", trace_id),
        received_over: IrohConnectionPresentation {
            endpoint_id: endpoint_id.clone(),
            alpn: MCT_HELLO_ALPN.into(),
            connection_side: ConnectionSide::Outgoing,
            path_class: PathClass::Direct,
            relay_url: None,
            presented_capability_ref: None,
        },
        requested_protocol: HelloPolicy::default().protocol,
        requested_vision_id: Some(peer.vision_id.clone()),
        requested_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
        presented_binding: MctPeerBindingPresentation {
            binding_id: Some(outbound_binding.binding_id.clone()),
            endpoint_id: endpoint_id.clone(),
            mct_node_id: Some(local_identity.node_id.clone()),
            vision_id: Some(peer.vision_id.clone()),
            policy_revision: Some(outbound_binding.policy_revision),
            allowed_alpns_claim: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            signature_ref: Some(outbound_binding.signature_ref.clone()),
            expires_at: Some(outbound_binding.expires_at.clone()),
        },
        capability_view,
        local_policy_revision_seen: Some(local_identity.policy_revision),
        trace_id: trace_id.clone(),
        received_observation_id: ObservationId::new(format!("obs-hello-forward:{trace_id}"))
            .expect("string ID literal/generated value must be non-empty"),
    }
}

pub(super) fn resident_forwarded_call_request(
    original: &MctCallProtocolRequest,
    local_identity: &MctLocalNodeIdentity,
    peer: &MctPeerAddressBookEntry,
    outbound_binding: &MctOutboundPeerBindingPresentation,
    endpoint_id: &EndpointIdText,
    hello: &MctHelloResponse,
    inline_payload: Option<&[u8]>,
) -> MctCallProtocolRequest {
    let mut call = original.call.clone();
    call.caller = CallerIdentity {
        node_id: local_identity.node_id.clone(),
        user_id: None,
        vision_id: peer.vision_id.clone(),
        project_id: original.call.caller.project_id.clone(),
    };
    call.origin = CallOrigin::Iroh;
    let payload =
        forwarded_request_payload_handle(&original.payload, &call.call_id, inline_payload);
    MctCallProtocolRequest {
        protocol_request_id: ProtocolRequestId::new(format!("proto-forwarded:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
        authority: MctCallProtocolAuthority {
            hello_decision_id: hello.decision_id.clone(),
            peer_binding_id: outbound_binding.binding_id.clone(),
            vision_id: peer.vision_id.clone(),
            accepted_alpn: MCT_CALL_ALPN.into(),
            endpoint_id: endpoint_id.clone(),
            policy_revision: outbound_binding.policy_revision,
            grants_revision: original.call.authority_context.grants_revision,
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
        payload,
        idempotency_key: original.idempotency_key.clone(),
        received_observation_id: ObservationId::new(format!(
            "obs-call-forwarded:{}",
            original.call.call_id
        ))
        .expect("string ID literal/generated value must be non-empty"),
    }
}

pub(super) fn forwarded_request_payload_handle(
    original: &MctCallPayloadHandle,
    call_id: &CallId,
    inline_payload: Option<&[u8]>,
) -> MctCallPayloadHandle {
    if let Some(bytes) = inline_payload {
        return inline_result_payload_handle(
            format!("payload-forwarded:{call_id}"),
            inline_payload_content_type(original).unwrap_or("application/octet-stream"),
            bytes,
        );
    }
    original.clone()
}

pub(super) fn remote_reply_to_call_handler_result(
    reply: MctIrohCallPayloadReply,
    route_decision_id: DecisionId,
    route_taken: RouteTaken,
) -> MctIrohCallHandlerResult {
    match reply.reply.reply_outcome {
        CallProtocolReplyOutcome::Success => {
            let result_ref = reply.reply.result_ref.unwrap_or_else(|| {
                ResultRef::new(format!("result-forwarded:{}", reply.reply.reply_id))
                    .expect("string ID literal/generated value must be non-empty")
            });
            let mut result = match reply.inline_result_payload {
                Some(bytes) => MctIrohCallHandlerResult::completed_with_inline_payload(
                    result_ref,
                    reply.reply.result_payload,
                    bytes,
                ),
                None => {
                    let mut result = MctIrohCallHandlerResult::completed(result_ref);
                    result.result_payload = reply.reply.result_payload;
                    result
                }
            };
            result.safe_message = reply.reply.safe_message;
            result.with_route(Some(route_decision_id), Some(route_taken))
        }
        CallProtocolReplyOutcome::Denied => {
            MctIrohCallHandlerResult::denied().with_route(Some(route_decision_id), None)
        }
        CallProtocolReplyOutcome::Failed => {
            MctIrohCallHandlerResult::failed(reply.reply.safe_message)
                .with_route(Some(route_decision_id), Some(route_taken))
        }
        CallProtocolReplyOutcome::TimedOut => MctIrohCallHandlerResult::timed_out()
            .with_route(Some(route_decision_id), Some(route_taken)),
        CallProtocolReplyOutcome::Cancelled => MctIrohCallHandlerResult::failed("call cancelled")
            .with_route(Some(route_decision_id), None),
        CallProtocolReplyOutcome::Malformed => {
            MctIrohCallHandlerResult::failed(reply.reply.safe_message)
                .with_route(Some(route_decision_id), None)
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use mct_iroh::{endpoint_id_for_secret_key_hex, sign_peer_binding_signature_ref};

    fn test_iroh_observation_sink() -> MctIrohObservationSink {
        MctIrohObservationSink::new(|_| async { Ok::<_, std::io::Error>(()) })
    }

    #[tokio::test]
    async fn first_boot_identity_is_durable_and_secret_free() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let identity_path = dir.path().join("node.key");
        let ledger_path = dir.path().join("observations.jsonl");
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();

        let identity = ensure_observed_local_identity(
            &MctDaemonConfigStore::new(&config_path),
            MctOperatorNodeScope::default(),
            &identity_path,
            &ledger,
        )
        .await
        .unwrap();
        ledger.close().await;

        assert!(config_path.exists());
        assert!(identity_path.exists());
        let secret = std::fs::read_to_string(identity_path).unwrap();
        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].durability_class, DurabilityClass::BeforeEffect);
        assert_eq!(
            entries[0].observation.kind,
            ObservationKind::OperatorActionRecorded
        );
        assert_eq!(
            entries[0].observation.resource_id.as_deref(),
            Some(identity.endpoint_id.as_str())
        );
        assert!(
            !std::fs::read_to_string(ledger_path)
                .unwrap()
                .contains(secret.trim())
        );
    }

    #[tokio::test]
    async fn bootstrap_identity_append_failure_leaves_no_identity_effect() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let identity_path = dir.path().join("node.key");
        let failed_ledger = ResidentLedgerWriter::failed_for_test();

        let error = ensure_observed_local_identity(
            &MctDaemonConfigStore::new(&config_path),
            MctOperatorNodeScope::default(),
            &identity_path,
            &failed_ledger,
        )
        .await
        .unwrap_err();

        assert!(format!("{error:#}").contains("ledger"));
        assert!(!config_path.exists());
        assert!(!identity_path.exists());
    }

    fn contract_peer_expiry() -> Timestamp {
        Timestamp::new("2099-01-01T00:00:00Z").unwrap()
    }

    fn test_call() -> MctCall {
        MctCall {
            call_id: CallId::new("call-cli-toy-expiry")
                .expect("string ID literal/generated value must be non-empty"),
            caller: CallerIdentity {
                node_id: MctNodeId::new("local-mct")
                    .expect("string ID literal/generated value must be non-empty"),
                user_id: None,
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                project_id: None,
            },
            target: OperationTarget {
                namespace: "patina:demo".into(),
                interface_name: "control@0.1.0".into(),
                function_name: "run".into(),
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
            deadline: Timestamp::new("2026-07-02T00:01:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-cli-toy-expiry")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: SpanId::new("span-cli-toy-expiry")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            origin: CallOrigin::Cli,
        }
    }

    fn test_child() -> mct_daemon::MctLoadedChild {
        mct_daemon::MctLoadedChild {
            child_id: ChildId::new("child-demo")
                .expect("string ID literal/generated value must be non-empty"),
            name: "child-demo".into(),
            version: "0.1.0".into(),
            description: None,
            kind: "wasm".into(),
            role: None,
            wasm_path: PathBuf::from("child-demo.wasm"),
            manifest_path: PathBuf::from("child.toml"),
            wasm_digest: mct_daemon::MctChildFileDigest {
                sha256: "wasm".into(),
                sidecar_present: true,
                verified: true,
            },
            manifest_digest: mct_daemon::MctChildFileDigest {
                sha256: "manifest".into(),
                sidecar_present: true,
                verified: true,
            },
            artifact_id: "artifact-demo".into(),
            ingress_mode: mct_daemon::MctChildIngressMode::WitOnly,
            allowed_operations: vec!["patina:demo/control@0.1.0.run".into()],
            requested_toys: Vec::new(),
            subscribed_streams: Vec::new(),
            relationship_listens: Vec::new(),
            wasm_size_bytes: 1,
            instance_state: mct_daemon::MctChildInstanceState::Ready,
        }
    }

    fn test_authorized_child() -> AuthorizedChildInvocation {
        authority_test_fixture::authorized_child_for_call(
            &test_call(),
            "child-demo",
            MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            "child",
        )
    }

    fn test_contract(toy_id: &ToyId) -> CanonicalToyContract {
        CanonicalToyContract {
            toy_id: toy_id.clone(),
            contract: ToyContractIdentity {
                namespace: "patina".into(),
                interface_name: "demo-toy".into(),
                version: "0.1.0".into(),
                function_name: Some("read".into()),
                resource_name: None,
            },
            authority_bearing: true,
            catalog_revision: 1,
            admitted_by_observation_id: ObservationId::new("obs-contract")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn expired_grant(toy_id: &ToyId) -> ToyGrant {
        ToyGrant {
            grant_id: ToyGrantId::new("grant-expired")
                .expect("string ID literal/generated value must be non-empty"),
            toy_id: toy_id.clone(),
            subject: ToyGrantSubject {
                child_name: "child-demo".into(),
                artifact_id: "artifact-demo".into(),
                artifact_version: "0.1.0".into(),
                assignment_id: Some(
                    ChildAssignmentId::new("assignment-child")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                caller_node_id: Some(
                    MctNodeId::new("local-mct")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
            },
            scope: ToyGrantScope {
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                node_id: Some(
                    MctNodeId::new("local-mct")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                project_id: None,
                data_classification: Some("public".into()),
                resource_id: Some("resource-a".into()),
                allowed_actions: vec!["read".into()],
            },
            constraints: ToyGrantConstraints {
                starts_at: None,
                expires_at: Some(Timestamp::new("2026-06-01T00:00:00Z").unwrap()),
                max_uses: None,
                max_duration_ms: None,
                locality_required: false,
            },
            grant_state: ToyGrantState::Active,
            issuer_id: "issuer".into(),
            policy_revision: 1,
            grants_revision: 1,
            authority_observation_id: ObservationId::new("obs-grant")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    #[tokio::test]
    async fn resident_mother_serves_peer_control_and_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let identity_path = dir.path().join("identity").join("iroh-secret.hex");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        let children_dir = dir.path().join("children");
        write_resident_process_child(&children_dir);

        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let client_endpoint_id = client.snapshot().endpoint_id;
        let store = MctDaemonConfigStore::new(&config_path);
        store
            .ensure_local_identity(MctOperatorNodeScope::default(), &identity_path)
            .unwrap();
        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        store
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        store
            .upsert_peer(MctPeerAddressBookEntry {
                peer_node_id: MctNodeId::new("mother-client")
                    .expect("string ID literal/generated value must be non-empty"),
                binding_id: PeerBindingId::new("binding-resident-client")
                    .expect("string ID literal/generated value must be non-empty"),
                endpoint_id: client_endpoint_id.clone(),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                ticket: None,
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                expires_at: contract_peer_expiry(),
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();
        let client_signature_ref = store.load().unwrap().peers["mother-client"]
            .binding_signature_ref
            .clone();

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let resident = tokio::spawn(run_resident_mother(
            ResidentMotherConfig {
                config_path: config_path.clone(),
                identity_path,
                children_dir,
                state_path,
                ledger_path: ledger_path.clone(),
                control: ResidentControlTransport::Uds(socket_path.clone()),
                relay_default: false,
                max_concurrent_connections: 8,
            },
            async move {
                let _ = shutdown_rx.await;
            },
            Some(ready_tx),
        ));
        let ticket = tokio::time::timeout(Duration::from_secs(10), ready_rx)
            .await
            .unwrap()
            .unwrap();

        let trace_id = TraceId::new("trace-resident-mother-test")
            .expect("string ID literal/generated value must be non-empty");
        let binding_id = PeerBindingId::new("binding-resident-client")
            .expect("string ID literal/generated value must be non-empty");
        let client_node_id = MctNodeId::new("mother-client")
            .expect("string ID literal/generated value must be non-empty");
        let vision_id = VisionId::new("vision-local")
            .expect("string ID literal/generated value must be non-empty");
        let hello = cli_hello_request(
            &client_endpoint_id,
            &binding_id,
            &client_node_id,
            &vision_id,
            &trace_id,
            client_signature_ref,
        );
        let hello_response = client.send_hello(&ticket, &hello).await.unwrap();
        assert_eq!(hello_response.hello_outcome, HelloOutcome::Admitted);
        let call = cli_call_request(
            &client_endpoint_id,
            &binding_id,
            &client_node_id,
            &vision_id,
            &trace_id,
            OperationTarget {
                namespace: "patina:demo".into(),
                interface_name: "control@0.1.0".into(),
                function_name: "run".into(),
            },
            &hello_response,
        );
        let reply = client.send_call(&ticket, &call).await.unwrap();
        assert_eq!(reply.reply_outcome, CallProtocolReplyOutcome::Success);
        assert!(reply.route_taken.is_some());

        let revoke_response = post_resident_peer_mutation(
            &socket_path,
            "/peers/revoke",
            serde_json::json!({
                "expected_config_path": config_path,
                "peer_node_id": client_node_id
            }),
        )
        .await;
        assert_eq!(revoke_response["action"], "revoke");
        assert_eq!(
            store.load().unwrap().peers["mother-client"].binding_state,
            BindingState::Revoked
        );
        let denied_reply = client.send_call(&ticket, &call).await.unwrap();
        assert_eq!(denied_reply.reply_outcome, CallProtocolReplyOutcome::Denied);
        assert_eq!(denied_reply.safe_message, "not authorized");
        assert!(denied_reply.route_taken.is_none());

        let status = poll_resident_status(&socket_path, |status| {
            status
                .resident
                .as_ref()
                .is_some_and(|resident| resident.accepted_connection_count >= 3)
        })
        .await;
        assert_eq!(
            status.iroh_endpoint.as_ref().unwrap().endpoint_id,
            ticket.endpoint_id
        );
        let resident_status = status.resident.expect("resident status is present");
        assert!(
            resident_status.accepted_connection_count >= 2,
            "{resident_status:?}"
        );
        assert_eq!(resident_status.loaded_child_count, 1);
        assert_eq!(resident_status.approved_child_count, 1);
        assert_eq!(resident_status.binding_count, 1);
        assert!(
            resident_status.ledger_sequence_tip >= 2,
            "{resident_status:?}"
        );

        let _ = shutdown_tx.send(());
        tokio::time::timeout(Duration::from_secs(10), resident)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(!socket_path.exists());
        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        let trace_entries = entries
            .iter()
            .filter(|entry| entry.observation.trace.trace_id == trace_id)
            .collect::<Vec<_>>();
        assert!(
            trace_entries
                .iter()
                .any(|entry| entry.observation.kind == ObservationKind::RouteRevalidated),
            "{trace_entries:?}"
        );
        assert!(
            trace_entries.iter().any(|entry| {
                entry.observation.kind == ObservationKind::RuntimeExecutionCompleted
            }),
            "{trace_entries:?}"
        );
        let revocation_sequence = entries
            .iter()
            .find(|entry| entry.observation.kind == ObservationKind::PeerBindingRevoked)
            .expect("durable UDS revocation observation")
            .local_sequence;
        let denial_sequence = trace_entries
            .iter()
            .find(|entry| entry.observation.kind == ObservationKind::CallDenied)
            .expect("call denied after live revocation")
            .local_sequence;
        assert!(revocation_sequence < denial_sequence);
        client.close().await;
    }

    #[tokio::test]
    async fn resident_hello_publishes_federation_callable_surface() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let identity_path = dir.path().join("identity").join("iroh-secret.hex");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        let children_dir = dir.path().join("children");
        write_resident_process_child(&children_dir);

        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let client_endpoint_id = client.snapshot().endpoint_id;
        let store = MctDaemonConfigStore::new(&config_path);
        store
            .ensure_local_identity(MctOperatorNodeScope::default(), &identity_path)
            .unwrap();
        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        store
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        store
            .upsert_peer(MctPeerAddressBookEntry {
                peer_node_id: MctNodeId::new("mother-client")
                    .expect("string ID literal/generated value must be non-empty"),
                binding_id: PeerBindingId::new("binding-resident-client")
                    .expect("string ID literal/generated value must be non-empty"),
                endpoint_id: client_endpoint_id.clone(),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                ticket: None,
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                expires_at: contract_peer_expiry(),
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();
        let client_signature_ref = store.load().unwrap().peers["mother-client"]
            .binding_signature_ref
            .clone();

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let resident = tokio::spawn(run_resident_mother(
            ResidentMotherConfig {
                config_path,
                identity_path,
                children_dir,
                state_path,
                ledger_path,
                control: ResidentControlTransport::Uds(socket_path),
                relay_default: false,
                max_concurrent_connections: 8,
            },
            async move {
                let _ = shutdown_rx.await;
            },
            Some(ready_tx),
        ));
        let ticket = tokio::time::timeout(Duration::from_secs(10), ready_rx)
            .await
            .unwrap()
            .unwrap();

        let trace_id = TraceId::new("trace-resident-hello-surface")
            .expect("string ID literal/generated value must be non-empty");
        let binding_id = PeerBindingId::new("binding-resident-client")
            .expect("string ID literal/generated value must be non-empty");
        let client_node_id = MctNodeId::new("mother-client")
            .expect("string ID literal/generated value must be non-empty");
        let vision_id = VisionId::new("vision-local")
            .expect("string ID literal/generated value must be non-empty");
        let hello = cli_hello_request(
            &client_endpoint_id,
            &binding_id,
            &client_node_id,
            &vision_id,
            &trace_id,
            client_signature_ref,
        );
        let hello_response = client.send_hello(&ticket, &hello).await.unwrap();
        assert_eq!(hello_response.hello_outcome, HelloOutcome::Admitted);
        let capability_view = hello_response
            .capability_view
            .expect("resident hello response publishes capability view");
        assert_eq!(
            capability_view.node_id,
            MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(capability_view.vision_id, vision_id);
        assert!(capability_view.callable_surfaces.iter().any(|surface| {
            surface.child_name == "resident-echo"
                && surface.operation_id == "patina:demo/control@0.1.0.run"
                && surface.visibility == "vision_scoped"
        }));

        let _ = shutdown_tx.send(());
        tokio::time::timeout(Duration::from_secs(10), resident)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        client.close().await;
    }

    #[tokio::test]
    async fn resident_mother_rejects_unsigned_peer_binding() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let identity_path = dir.path().join("identity").join("iroh-secret.hex");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        let children_dir = dir.path().join("children");

        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let client_endpoint_id = client.snapshot().endpoint_id;
        let store = MctDaemonConfigStore::new(&config_path);
        store
            .ensure_local_identity(MctOperatorNodeScope::default(), &identity_path)
            .unwrap();
        store
            .upsert_peer(MctPeerAddressBookEntry {
                peer_node_id: MctNodeId::new("mother-unsigned-client")
                    .expect("string ID literal/generated value must be non-empty"),
                binding_id: PeerBindingId::new("binding-resident-unsigned-client")
                    .expect("string ID literal/generated value must be non-empty"),
                endpoint_id: client_endpoint_id.clone(),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                ticket: None,
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                expires_at: contract_peer_expiry(),
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();
        assert!(
            store.load().unwrap().peers["mother-unsigned-client"]
                .binding_signature_ref
                .is_some(),
            "server persists an issued proof, but the peer must present it"
        );

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let resident = tokio::spawn(run_resident_mother(
            ResidentMotherConfig {
                config_path,
                identity_path,
                children_dir,
                state_path,
                ledger_path,
                control: ResidentControlTransport::Uds(socket_path),
                relay_default: false,
                max_concurrent_connections: 8,
            },
            async move {
                let _ = shutdown_rx.await;
            },
            Some(ready_tx),
        ));
        let ticket = tokio::time::timeout(Duration::from_secs(10), ready_rx)
            .await
            .unwrap()
            .unwrap();

        let trace_id = TraceId::new("trace-resident-unsigned-peer")
            .expect("string ID literal/generated value must be non-empty");
        let binding_id = PeerBindingId::new("binding-resident-unsigned-client")
            .expect("string ID literal/generated value must be non-empty");
        let client_node_id = MctNodeId::new("mother-unsigned-client")
            .expect("string ID literal/generated value must be non-empty");
        let vision_id = VisionId::new("vision-local")
            .expect("string ID literal/generated value must be non-empty");
        let hello = cli_hello_request(
            &client_endpoint_id,
            &binding_id,
            &client_node_id,
            &vision_id,
            &trace_id,
            None,
        );
        let hello_response = client.send_hello(&ticket, &hello).await.unwrap();
        assert_eq!(hello_response.hello_outcome, HelloOutcome::Denied);
        assert_eq!(hello_response.safe_message, "not authorized");

        let _ = shutdown_tx.send(());
        tokio::time::timeout(Duration::from_secs(10), resident)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        client.close().await;
    }

    #[tokio::test]
    async fn two_mother_forwards_selected_call_over_iroh_and_maps_reply() {
        let dir = tempfile::tempdir().unwrap();
        let mother_a_config_path = dir.path().join("mother-a").join("config.json");
        let mother_a_identity_path = dir
            .path()
            .join("mother-a")
            .join("identity")
            .join("iroh-secret.hex");
        let mother_a_state_path = dir.path().join("mother-a").join("state.sqlite");
        let mother_a_ledger_path = dir.path().join("mother-a").join("observations.jsonl");
        let mother_a_children_dir = dir.path().join("mother-a").join("children");
        let mother_b_config_path = dir.path().join("mother-b").join("config.json");
        let mother_b_identity_path = dir
            .path()
            .join("mother-b")
            .join("identity")
            .join("iroh-secret.hex");
        let mother_b_state_path = dir.path().join("mother-b").join("state.sqlite");
        let mother_b_ledger_path = dir.path().join("mother-b").join("observations.jsonl");
        let mother_b_socket_path = dir.path().join("mother-b").join("control.sock");
        let mother_b_children_dir = dir.path().join("mother-b").join("children");
        write_resident_payload_process_child(&mother_b_children_dir);

        let mother_a_node_id = MctNodeId::new("mother-a")
            .expect("string ID literal/generated value must be non-empty");
        let mother_b_node_id = MctNodeId::new("mother-b")
            .expect("string ID literal/generated value must be non-empty");
        let vision_id = VisionId::new("vision-local")
            .expect("string ID literal/generated value must be non-empty");
        let mother_a_store = MctDaemonConfigStore::new(&mother_a_config_path);
        let mother_b_store = MctDaemonConfigStore::new(&mother_b_config_path);
        let mother_a_identity = mother_a_store
            .ensure_local_identity(
                MctOperatorNodeScope {
                    node_id: mother_a_node_id.clone(),
                    vision_id: vision_id.clone(),
                    policy_revision: 1,
                },
                &mother_a_identity_path,
            )
            .unwrap();
        let mother_b_identity = mother_b_store
            .ensure_local_identity(
                MctOperatorNodeScope {
                    node_id: mother_b_node_id.clone(),
                    vision_id: vision_id.clone(),
                    policy_revision: 1,
                },
                &mother_b_identity_path,
            )
            .unwrap();
        let loaded_b =
            load_children_from_dir(MctChildLoadOptions::new(mother_b_children_dir.clone()));
        mother_b_store
            .approve_and_assign_loaded_child(
                &loaded_b.children[0],
                MctOperatorChildScope {
                    vision_id: vision_id.clone(),
                    node_id: mother_b_node_id.clone(),
                    project_id: None,
                    policy_revision: 1,
                },
            )
            .unwrap();
        mother_b_store
            .upsert_peer(MctPeerAddressBookEntry {
                peer_node_id: mother_a_node_id.clone(),
                binding_id: PeerBindingId::new("binding-b-admits-a")
                    .expect("string ID literal/generated value must be non-empty"),
                endpoint_id: mother_a_identity.endpoint_id.clone(),
                vision_id: vision_id.clone(),
                ticket: None,
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                expires_at: contract_peer_expiry(),
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();
        let mother_b_proof_for_a = mother_b_store.load().unwrap().peers["mother-a"]
            .binding_signature_ref
            .clone()
            .unwrap();

        let (mother_b_ready_tx, mother_b_ready_rx) = tokio::sync::oneshot::channel();
        let (mother_b_shutdown_tx, mother_b_shutdown_rx) = tokio::sync::oneshot::channel();
        let mother_b = tokio::spawn(run_resident_mother(
            ResidentMotherConfig {
                config_path: mother_b_config_path.clone(),
                identity_path: mother_b_identity_path.clone(),
                children_dir: mother_b_children_dir.clone(),
                state_path: mother_b_state_path.clone(),
                ledger_path: mother_b_ledger_path.clone(),
                control: ResidentControlTransport::Uds(mother_b_socket_path),
                relay_default: false,
                max_concurrent_connections: 8,
            },
            async move {
                let _ = mother_b_shutdown_rx.await;
            },
            Some(mother_b_ready_tx),
        ));
        let mother_b_ticket = tokio::time::timeout(Duration::from_secs(10), mother_b_ready_rx)
            .await
            .unwrap()
            .unwrap();

        mother_a_store
            .upsert_peer(MctPeerAddressBookEntry {
                peer_node_id: mother_b_node_id.clone(),
                binding_id: PeerBindingId::new("binding-a-admits-b")
                    .expect("string ID literal/generated value must be non-empty"),
                endpoint_id: mother_b_identity.endpoint_id.clone(),
                vision_id: vision_id.clone(),
                ticket: Some(mother_b_ticket.clone()),
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                expires_at: contract_peer_expiry(),
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();
        mother_a_store
            .set_peer_outbound_proof(
                &mother_b_node_id,
                MctOutboundPeerBindingPresentation {
                    binding_id: PeerBindingId::new("binding-b-admits-a")
                        .expect("string ID literal/generated value must be non-empty"),
                    policy_revision: 1,
                    signature_ref: mother_b_proof_for_a,
                    expires_at: contract_peer_expiry(),
                },
            )
            .unwrap();
        let mother_a_peer_b = mother_a_store.load().unwrap().peers["mother-b"].clone();
        let received_at = current_timestamp();
        let stale_at = remote_surface_stale_at(&received_at).unwrap();
        let surface_view = MctHelloCapabilityView {
            node_id: mother_b_node_id.clone(),
            vision_id: vision_id.clone(),
            published_at: received_at.clone(),
            policy_revision: 1,
            supported_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            supported_wit_worlds: vec!["patina:demo/control@0.1.0".into()],
            supported_observation_modes: vec!["local-ledger".into()],
            callable_surfaces: vec![MctHelloCallableSurface {
                child_name: "resident-payload-echo".into(),
                operation_id: "patina:demo/control@0.1.0.run".into(),
                runtime_kind: RuntimeKind::Process,
                vision_id: vision_id.clone(),
                policy_revision: 1,
                visibility: "vision_scoped".into(),
            }],
            capability_view_ref: None,
        };
        MctRuntimeStateStore::open(&mother_a_state_path)
            .unwrap()
            .refresh_remote_callable_surfaces(MctRemoteSurfaceRefresh {
                peer_node_id: &mother_b_node_id,
                binding_id: &mother_a_peer_b.binding_id,
                endpoint_id: &mother_a_peer_b.endpoint_id,
                view: &surface_view,
                received_at: &received_at,
                stale_at: &stale_at,
                view_observation_id: &ObservationId::new("obs-test-mother-b-surface")
                    .expect("string ID literal/generated value must be non-empty"),
            })
            .unwrap();

        let trace_id = TraceId::new("trace-two-mother-forward")
            .expect("string ID literal/generated value must be non-empty");
        let payload = br#"{"hello":"remote"}"#.to_vec();
        let mut call = resident_test_protocol_request(resident_test_call(trace_id));
        call.call.call_id = CallId::new("call-two-mother-forward")
            .expect("string ID literal/generated value must be non-empty");
        call.call.caller.node_id = mother_a_node_id;
        call.call.caller.vision_id = vision_id;
        call.call.origin = CallOrigin::Cli;
        call.call.payload_metadata.size_bytes = payload.len() as u64;
        call.payload = MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: "payload-two-mother-forward".into(),
            content_type: "application/json".into(),
            size_bytes: payload.len() as u64,
            blake3_digest_hex: blake3_hex(&payload),
        };
        let mother_a_ledger = ResidentLedgerWriter::spawn(mother_a_ledger_path.clone()).unwrap();
        let call_reply = execute_resident_call(
            ResidentExecutionPaths {
                config_path: mother_a_config_path,
                children_dir: mother_a_children_dir,
                state_path: mother_a_state_path,
            },
            mother_a_ledger.clone(),
            call,
            ResidentPayloadIngress::remote(Some(payload)),
        )
        .await;
        mother_a_ledger.close().await;

        let _ = mother_b_shutdown_tx.send(());
        tokio::time::timeout(Duration::from_secs(10), mother_b)
            .await
            .unwrap()
            .unwrap()
            .unwrap();

        assert_eq!(call_reply.outcome, CallProtocolOutcome::Completed);
        assert_eq!(
            call_reply
                .inline_result_payload
                .expect("forwarded result payload"),
            br#"processed:{"hello":"remote"}"#.to_vec()
        );
        assert!(matches!(
            call_reply.route_taken,
            Some(RouteTaken {
                runtime_kind: RuntimeKind::RemotePeer,
                ..
            })
        ));
        let mother_a_ledger = std::fs::read_to_string(&mother_a_ledger_path).unwrap();
        let mother_b_ledger = std::fs::read_to_string(&mother_b_ledger_path).unwrap();
        assert!(mother_a_ledger.contains("forwarded_from:mother-a;forwarded_to:mother-b"));
        assert!(mother_b_ledger.contains("executed_on:mother-b;forwarded_from:mother-a"));
        assert!(!mother_a_ledger.contains("{\"hello\":\"remote\"}"));
        assert!(!mother_b_ledger.contains("{\"hello\":\"remote\"}"));
        assert!(!mother_a_ledger.contains("processed:"));
        assert!(!mother_b_ledger.contains("processed:"));
    }

    #[tokio::test]
    async fn two_mother_forwarding_denies_when_executor_revokes_binding_after_hello() {
        let mut fixture = remote_surface_candidate_fixture();
        let executor = MotherIrohEndpoint::bind(iroh_config(fixture.remote_secret.clone(), false))
            .await
            .unwrap();
        let executor_ticket = executor.ticket();
        let config_store = MctDaemonConfigStore::new(&fixture.config_path);
        let mut peer = fixture.config.peers["remote-mct"].clone();
        peer.ticket = Some(executor_ticket);
        fixture.config = config_store.upsert_peer(peer).unwrap();

        let received_at = current_timestamp();
        let stale_at = remote_surface_stale_at(&received_at).unwrap();
        let peer = fixture.config.peers["remote-mct"].clone();
        let surface_view = hello_capability_view(
            &peer.peer_node_id,
            &peer.vision_id,
            1,
            &["patina:demo/control@0.1.0.run"],
        );
        fixture
            .state
            .refresh_remote_callable_surfaces(MctRemoteSurfaceRefresh {
                peer_node_id: &peer.peer_node_id,
                binding_id: &peer.binding_id,
                endpoint_id: &peer.endpoint_id,
                view: &surface_view,
                received_at: &received_at,
                stale_at: &stale_at,
                view_observation_id: &ObservationId::new("obs-forward-revocation-surface")
                    .expect("string ID literal/generated value must be non-empty"),
            })
            .unwrap();

        let local_identity = fixture.config.local_identity.as_ref().unwrap();
        let outbound = peer.outbound_binding.as_ref().unwrap();
        let executor_binding =
            outbound_peer_binding_for_local(local_identity, &peer, outbound).unwrap();
        let provider_calls = Arc::new(AtomicU64::new(0));
        let provider_counter = Arc::clone(&provider_calls);
        let handler_calls = Arc::new(AtomicU64::new(0));
        let handler_counter = Arc::clone(&handler_calls);
        let (events, mut received_events) = tokio::sync::mpsc::channel(8);
        let executor_task = tokio::spawn(async move {
            executor
                .serve_concurrent_with_binding_provider(
                    MctIrohServeState::new(),
                    MctIrohConcurrentServeConfig {
                        events: Some(events),
                        require_binding_signature: true,
                        ..MctIrohConcurrentServeConfig::new(test_iroh_observation_sink())
                    },
                    current_timestamp,
                    move || {
                        let mut binding = executor_binding.clone();
                        if provider_counter.fetch_add(1, Ordering::SeqCst) > 0 {
                            binding.binding_state = BindingState::Revoked;
                        }
                        async move {
                            Ok(MctPeerAuthoritySnapshot {
                                bindings: vec![binding],
                                policy_revision: 1,
                            })
                        }
                    },
                    move |_, _, _| {
                        let handler_counter = Arc::clone(&handler_counter);
                        async move {
                            handler_counter.fetch_add(1, Ordering::SeqCst);
                            MctIrohCallHandlerResult::completed(
                                ResultRef::new("unexpected-forwarded-result")
                                    .expect("string ID literal/generated value must be non-empty"),
                            )
                        }
                    },
                )
                .await
        });

        let outcome = authorize_resident_child_from_loaded_with_state(
            &fixture.config,
            Some(&fixture.state),
            Vec::new(),
            &fixture.call,
            current_timestamp(),
        )
        .unwrap();
        let RouteDisposition::Remote {
            plan: execution,
            observations,
        } = outcome
        else {
            panic!("fresh published executor should be selected before revocation");
        };
        let ledger_path = fixture._dir.path().join("forwarding-observations.jsonl");
        let ledger = ResidentLedgerWriter::spawn(ledger_path).unwrap();
        ledger.append(observations).await.unwrap();
        let result = execute_authorized_resident_remote_call(
            ResidentExecutionPaths {
                config_path: fixture.config_path.clone(),
                children_dir: fixture._dir.path().join("children"),
                state_path: fixture.state_path.clone(),
            },
            *execution,
            resident_test_protocol_request(fixture.call.clone()),
            None,
            ledger.clone(),
        )
        .await;
        ledger.close().await;

        assert_eq!(result.outcome, CallProtocolOutcome::Denied);
        assert_eq!(result.safe_message, "not authorized");
        assert_eq!(provider_calls.load(Ordering::SeqCst), 2);
        assert_eq!(handler_calls.load(Ordering::SeqCst), 0);
        let call_evaluation = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(MctIrohServeEvent::Served(served)) = received_events.recv().await
                    && let MctIrohServedProtocol::Call { evaluation, .. } = *served
                {
                    break evaluation;
                }
            }
        })
        .await
        .expect("forwarded call evaluation event");
        assert_eq!(call_evaluation.outcome, CallProtocolOutcome::Denied);
        assert_eq!(call_evaluation.reason, CallProtocolReason::BindingRevoked);

        executor_task.abort();
    }

    #[tokio::test]
    async fn two_mother_mutual_publication_with_unready_children_terminates_single_hop() {
        let dir = tempfile::tempdir().unwrap();
        let a_root = dir.path().join("mother-a");
        let b_root = dir.path().join("mother-b");
        let a_config_path = a_root.join("config.json");
        let a_identity_path = a_root.join("identity").join("iroh-secret.hex");
        let a_state_path = a_root.join("state.sqlite");
        let a_ledger_path = a_root.join("observations.jsonl");
        let a_children_dir = a_root.join("children");
        let b_config_path = b_root.join("config.json");
        let b_identity_path = b_root.join("identity").join("iroh-secret.hex");
        let b_state_path = b_root.join("state.sqlite");
        let b_ledger_path = b_root.join("observations.jsonl");
        let b_children_dir = b_root.join("children");
        write_resident_process_child(&a_children_dir);
        write_resident_process_child(&b_children_dir);

        let a_node = MctNodeId::new("mother-a").unwrap();
        let b_node = MctNodeId::new("mother-b").unwrap();
        let vision = VisionId::new("vision-local").unwrap();
        let a_store = MctDaemonConfigStore::new(&a_config_path);
        let b_store = MctDaemonConfigStore::new(&b_config_path);
        let a_identity = a_store
            .ensure_local_identity(
                MctOperatorNodeScope {
                    node_id: a_node.clone(),
                    vision_id: vision.clone(),
                    policy_revision: 1,
                },
                &a_identity_path,
            )
            .unwrap();
        let b_identity = b_store
            .ensure_local_identity(
                MctOperatorNodeScope {
                    node_id: b_node.clone(),
                    vision_id: vision.clone(),
                    policy_revision: 1,
                },
                &b_identity_path,
            )
            .unwrap();
        let mut loaded_a = load_children_from_dir(MctChildLoadOptions::new(&a_children_dir));
        let mut loaded_b = load_children_from_dir(MctChildLoadOptions::new(&b_children_dir));
        a_store
            .approve_and_assign_loaded_child(
                &loaded_a.children[0],
                MctOperatorChildScope {
                    vision_id: vision.clone(),
                    node_id: a_node.clone(),
                    project_id: None,
                    policy_revision: 1,
                },
            )
            .unwrap();
        b_store
            .approve_and_assign_loaded_child(
                &loaded_b.children[0],
                MctOperatorChildScope {
                    vision_id: vision.clone(),
                    node_id: b_node.clone(),
                    project_id: None,
                    policy_revision: 1,
                },
            )
            .unwrap();
        loaded_a.children[0].instance_state = mct_daemon::MctChildInstanceState::Loading;
        loaded_b.children[0].instance_state = mct_daemon::MctChildInstanceState::Loading;

        let a_secret = load_or_create_node_secret_key_hex(&a_identity_path).unwrap();
        let mut a_ticket_endpoint = MotherIrohEndpoint::bind(iroh_config(a_secret, false))
            .await
            .unwrap();
        let a_ticket = a_ticket_endpoint.ticket();
        a_ticket_endpoint.close().await;
        let b_secret = load_or_create_node_secret_key_hex(&b_identity_path).unwrap();
        let b_endpoint = MotherIrohEndpoint::bind(iroh_config(b_secret, false))
            .await
            .unwrap();
        let b_ticket = b_endpoint.ticket();

        b_store
            .upsert_peer(MctPeerAddressBookEntry {
                peer_node_id: a_node.clone(),
                binding_id: PeerBindingId::new("binding-b-admits-a").unwrap(),
                endpoint_id: a_identity.endpoint_id.clone(),
                vision_id: vision.clone(),
                ticket: Some(a_ticket),
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                expires_at: contract_peer_expiry(),
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();
        let b_proof_for_a = b_store.load().unwrap().peers["mother-a"]
            .binding_signature_ref
            .clone()
            .unwrap();
        a_store
            .upsert_peer(MctPeerAddressBookEntry {
                peer_node_id: b_node.clone(),
                binding_id: PeerBindingId::new("binding-a-admits-b").unwrap(),
                endpoint_id: b_identity.endpoint_id.clone(),
                vision_id: vision.clone(),
                ticket: Some(b_ticket.clone()),
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                expires_at: contract_peer_expiry(),
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();
        let a_proof_for_b = a_store.load().unwrap().peers["mother-b"]
            .binding_signature_ref
            .clone()
            .unwrap();
        a_store
            .set_peer_outbound_proof(
                &b_node,
                MctOutboundPeerBindingPresentation {
                    binding_id: PeerBindingId::new("binding-b-admits-a").unwrap(),
                    policy_revision: 1,
                    signature_ref: b_proof_for_a,
                    expires_at: contract_peer_expiry(),
                },
            )
            .unwrap();
        b_store
            .set_peer_outbound_proof(
                &a_node,
                MctOutboundPeerBindingPresentation {
                    binding_id: PeerBindingId::new("binding-a-admits-b").unwrap(),
                    policy_revision: 1,
                    signature_ref: a_proof_for_b,
                    expires_at: contract_peer_expiry(),
                },
            )
            .unwrap();

        let a_config = a_store.load().unwrap();
        let b_config = b_store.load().unwrap();
        let a_peer_b = a_config.peers["mother-b"].clone();
        let b_peer_a = b_config.peers["mother-a"].clone();
        let received_at = current_timestamp();
        let stale_at = remote_surface_stale_at(&received_at).unwrap();
        let a_view = hello_capability_view(&a_node, &vision, 1, &["patina:demo/control@0.1.0.run"]);
        let b_view = hello_capability_view(&b_node, &vision, 1, &["patina:demo/control@0.1.0.run"]);
        MctRuntimeStateStore::open(&a_state_path)
            .unwrap()
            .refresh_remote_callable_surfaces(MctRemoteSurfaceRefresh {
                peer_node_id: &b_node,
                binding_id: &a_peer_b.binding_id,
                endpoint_id: &a_peer_b.endpoint_id,
                view: &b_view,
                received_at: &received_at,
                stale_at: &stale_at,
                view_observation_id: &ObservationId::new("obs-a-saw-b-surface").unwrap(),
            })
            .unwrap();
        MctRuntimeStateStore::open(&b_state_path)
            .unwrap()
            .refresh_remote_callable_surfaces(MctRemoteSurfaceRefresh {
                peer_node_id: &a_node,
                binding_id: &b_peer_a.binding_id,
                endpoint_id: &b_peer_a.endpoint_id,
                view: &a_view,
                received_at: &received_at,
                stale_at: &stale_at,
                view_observation_id: &ObservationId::new("obs-b-saw-a-surface").unwrap(),
            })
            .unwrap();

        let b_ledger = ResidentLedgerWriter::spawn(b_ledger_path.clone()).unwrap();
        let b_handler_ledger = b_ledger.clone();
        let b_handler_config = b_config.clone();
        let b_handler_state_path = b_state_path.clone();
        let b_handler_children = loaded_b.children.clone();
        let b_bindings = b_config.peer_authority_projection().unwrap().bindings;
        let b_serve = tokio::spawn(async move {
            b_endpoint
                .serve_concurrent_with_call_handler(
                    MctIrohServeState::new(),
                    b_bindings,
                    MctIrohConcurrentServeConfig {
                        require_binding_signature: true,
                        capability_view: Some(b_view),
                        ..MctIrohConcurrentServeConfig::new(test_iroh_observation_sink())
                    },
                    current_timestamp,
                    move |request, _, _| {
                        let ledger = b_handler_ledger.clone();
                        let config = b_handler_config.clone();
                        let state_path = b_handler_state_path.clone();
                        let children = b_handler_children.clone();
                        async move {
                            let state = MctRuntimeStateStore::open(state_path).unwrap();
                            match authorize_resident_child_from_loaded_with_state(
                                &config,
                                Some(&state),
                                children,
                                &request.call,
                                current_timestamp(),
                            )
                            .unwrap()
                            {
                                RouteDisposition::Denied {
                                    decision,
                                    observations,
                                } => {
                                    ledger.append(observations).await.unwrap();
                                    MctIrohCallHandlerResult::denied()
                                        .with_route(Some(decision.decision_id), None)
                                }
                                RouteDisposition::Local { .. } => MctIrohCallHandlerResult::failed(
                                    "unready local child unexpectedly authorized",
                                ),
                                RouteDisposition::Remote { observations, .. } => {
                                    ledger.append(observations).await.unwrap();
                                    MctIrohCallHandlerResult::failed(
                                        "forwarded arrival unexpectedly selected a remote route",
                                    )
                                }
                            }
                        }
                    },
                )
                .await
        });

        let call_id = CallId::new("call-mutual-unready-single-hop").unwrap();
        let mut call = resident_test_call(TraceId::new("trace-mutual-unready-single-hop").unwrap());
        call.call_id = call_id.clone();
        call.caller.node_id = a_node;
        call.caller.vision_id = vision;
        call.origin = CallOrigin::Cli;
        let request = resident_test_protocol_request(call.clone());
        let a_state = MctRuntimeStateStore::open(&a_state_path).unwrap();
        let RouteDisposition::Remote {
            plan: authorized,
            observations,
        } = authorize_resident_child_from_loaded_with_state(
            &a_config,
            Some(&a_state),
            loaded_a.children,
            &call,
            current_timestamp(),
        )
        .unwrap()
        else {
            panic!("originating Mother should select its published remote executor")
        };
        let a_ledger = ResidentLedgerWriter::spawn(a_ledger_path.clone()).unwrap();
        a_ledger.append(observations).await.unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            execute_authorized_resident_remote_call(
                ResidentExecutionPaths {
                    config_path: a_config_path,
                    children_dir: a_children_dir,
                    state_path: a_state_path,
                },
                *authorized,
                request,
                None,
                a_ledger.clone(),
            ),
        )
        .await
        .expect("single-hop denial must complete before the deadline");
        assert_eq!(result.outcome, CallProtocolOutcome::Denied);

        a_ledger.close().await;
        b_ledger.close().await;
        b_serve.abort();
        let a_text = std::fs::read_to_string(&a_ledger_path).unwrap();
        let b_text = std::fs::read_to_string(&b_ledger_path).unwrap();
        assert!(a_text.contains("CapabilityUnavailable"));
        assert!(a_text.contains("denial_class:temporal"));
        assert!(a_text.contains("remote_reply:Denied"));
        assert!(b_text.contains("CapabilityUnavailable"));
        assert!(b_text.contains("denial_class:temporal"));
        assert!(b_text.contains("no_route_recorded"));
        assert!(!b_text.contains("peer_call_sent"));

        let forward_count = [&a_text, &b_text]
            .into_iter()
            .flat_map(|text| text.lines())
            .filter(|line| {
                line.contains(call_id.as_str()) && line.contains("\"kind\":\"peer_call_sent\"")
            })
            .count();
        assert_eq!(forward_count, 1, "the same call_id must be forwarded once");
    }

    /// Covers `MctCallProtocol.CurrentAuthorityPrecedesReplay` for revocation,
    /// binding expiry, and Vision narrowing on the real resident peer path.
    #[tokio::test]
    async fn resident_mother_payload_roundtrip_verifies_result_digest() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let identity_path = dir.path().join("identity").join("iroh-secret.hex");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        let children_dir = dir.path().join("children");
        write_resident_payload_process_child(&children_dir);

        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let client_endpoint_id = client.snapshot().endpoint_id;
        let store = MctDaemonConfigStore::new(&config_path);
        store
            .ensure_local_identity(MctOperatorNodeScope::default(), &identity_path)
            .unwrap();
        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        store
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        store
            .upsert_peer(MctPeerAddressBookEntry {
                peer_node_id: MctNodeId::new("mother-payload-client")
                    .expect("string ID literal/generated value must be non-empty"),
                binding_id: PeerBindingId::new("binding-resident-payload-client")
                    .expect("string ID literal/generated value must be non-empty"),
                endpoint_id: client_endpoint_id.clone(),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                ticket: None,
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                expires_at: contract_peer_expiry(),
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();
        let client_signature_ref = store.load().unwrap().peers["mother-payload-client"]
            .binding_signature_ref
            .clone();
        let signature_marker = client_signature_ref.clone().unwrap();

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let resident = tokio::spawn(run_resident_mother(
            ResidentMotherConfig {
                config_path,
                identity_path,
                children_dir,
                state_path,
                ledger_path: ledger_path.clone(),
                control: ResidentControlTransport::Uds(socket_path),
                relay_default: false,
                max_concurrent_connections: 8,
            },
            async move {
                let _ = shutdown_rx.await;
            },
            Some(ready_tx),
        ));
        let ticket = tokio::time::timeout(Duration::from_secs(10), ready_rx)
            .await
            .unwrap()
            .unwrap();

        let trace_id = TraceId::new("trace-resident-payload-e2e")
            .expect("string ID literal/generated value must be non-empty");
        let binding_id = PeerBindingId::new("binding-resident-payload-client")
            .expect("string ID literal/generated value must be non-empty");
        let client_node_id = MctNodeId::new("mother-payload-client")
            .expect("string ID literal/generated value must be non-empty");
        let vision_id = VisionId::new("vision-local")
            .expect("string ID literal/generated value must be non-empty");
        let hello = cli_hello_request(
            &client_endpoint_id,
            &binding_id,
            &client_node_id,
            &vision_id,
            &trace_id,
            client_signature_ref,
        );
        let hello_response = client.send_hello(&ticket, &hello).await.unwrap();
        assert_eq!(hello_response.hello_outcome, HelloOutcome::Admitted);

        let payload = br#"{"secret":"payload-marker"}"#.to_vec();
        let payload_base64 = BASE64_STANDARD.encode(&payload);
        let mut call = cli_call_request(
            &client_endpoint_id,
            &binding_id,
            &client_node_id,
            &vision_id,
            &trace_id,
            OperationTarget {
                namespace: "patina:demo".into(),
                interface_name: "control@0.1.0".into(),
                function_name: "run".into(),
            },
            &hello_response,
        );
        call.call.call_id = CallId::new("call-resident-payload-e2e")
            .expect("string ID literal/generated value must be non-empty");
        call.call.payload_metadata.size_bytes = payload.len() as u64;
        call.payload = MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: "payload-resident-e2e".into(),
            content_type: "application/json".into(),
            size_bytes: payload.len() as u64,
            blake3_digest_hex: blake3_hex(&payload),
        };
        call.idempotency_key = Some("resident-payload-replay".into());

        let call_reply = client
            .send_call_with_inline_payload(&ticket, &call, payload.clone())
            .await
            .unwrap();
        let result_payload = call_reply
            .inline_result_payload
            .expect("verified result payload bytes returned");
        let expected_result = br#"processed:{"secret":"payload-marker"}"#.to_vec();
        let expected_result_base64 = BASE64_STANDARD.encode(&expected_result);
        assert_eq!(result_payload, expected_result);
        assert_eq!(
            call_reply.reply.reply_outcome,
            CallProtocolReplyOutcome::Success
        );
        assert_eq!(
            call_reply.reply.result_payload.declared_size_bytes(),
            expected_result.len() as u64
        );
        assert!(matches!(
            call_reply.reply.result_payload,
            MctCallPayloadHandle::InlinePayload { ref blake3_digest_hex, .. }
                if blake3_digest_hex == &blake3_hex(&expected_result)
        ));

        let admitted_peer = store.load().unwrap().peers[client_node_id.as_str()].clone();

        let mut expired_peer = admitted_peer.clone();
        expired_peer.expires_at = Timestamp::new("2026-07-08T00:00:00Z").unwrap();
        expired_peer.binding_signature_ref = None;
        store.upsert_peer(expired_peer).unwrap();
        let expired_retry = client
            .send_call_with_inline_payload(&ticket, &call, payload.clone())
            .await
            .unwrap();
        assert_eq!(
            expired_retry.reply.reply_outcome,
            CallProtocolReplyOutcome::Denied
        );
        assert!(expired_retry.inline_result_payload.is_none());

        let mut wrong_vision_peer = admitted_peer.clone();
        wrong_vision_peer.vision_id = VisionId::new("vision-other").unwrap();
        wrong_vision_peer.binding_signature_ref = None;
        store.upsert_peer(wrong_vision_peer).unwrap();
        let narrowed_vision_retry = client
            .send_call_with_inline_payload(&ticket, &call, payload.clone())
            .await
            .unwrap();
        assert_eq!(
            narrowed_vision_retry.reply.reply_outcome,
            CallProtocolReplyOutcome::Denied
        );
        assert!(narrowed_vision_retry.inline_result_payload.is_none());

        store.upsert_peer(admitted_peer).unwrap();
        store.revoke_peer(&client_node_id).unwrap();
        let revoked_retry = client
            .send_call_with_inline_payload(&ticket, &call, payload)
            .await
            .unwrap();
        assert_eq!(
            revoked_retry.reply.reply_outcome,
            CallProtocolReplyOutcome::Denied
        );
        assert_eq!(revoked_retry.reply.safe_message, "not authorized");
        assert!(revoked_retry.inline_result_payload.is_none());

        let _ = shutdown_tx.send(());
        tokio::time::timeout(Duration::from_secs(10), resident)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        client.close().await;

        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        let lifecycle_kinds = entries
            .iter()
            .filter(|entry| {
                entry
                    .observation
                    .call_id
                    .as_ref()
                    .is_some_and(|call_id| call_id.as_str() == "call-resident-payload-e2e")
            })
            .map(|entry| entry.observation.kind)
            .filter(|kind| {
                matches!(
                    kind,
                    ObservationKind::PeerCallReceived
                        | ObservationKind::CallConstructed
                        | ObservationKind::CallAuthorized
                        | ObservationKind::CallDenied
                        | ObservationKind::RouteSelected
                        | ObservationKind::RouteRevalidated
                        | ObservationKind::NoRouteRecorded
                        | ObservationKind::ResultRecorded
                        | ObservationKind::PeerCallReplied
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            lifecycle_kinds,
            vec![
                ObservationKind::PeerCallReceived,
                ObservationKind::CallConstructed,
                ObservationKind::CallAuthorized,
                ObservationKind::RouteRevalidated,
                ObservationKind::RouteSelected,
                ObservationKind::RouteRevalidated,
                ObservationKind::RouteRevalidated,
                ObservationKind::ResultRecorded,
                ObservationKind::PeerCallReplied,
                ObservationKind::PeerCallReceived,
                ObservationKind::CallConstructed,
                ObservationKind::CallDenied,
                ObservationKind::ResultRecorded,
                ObservationKind::PeerCallReplied,
                ObservationKind::PeerCallReceived,
                ObservationKind::CallConstructed,
                ObservationKind::CallDenied,
                ObservationKind::ResultRecorded,
                ObservationKind::PeerCallReplied,
                ObservationKind::PeerCallReceived,
                ObservationKind::CallConstructed,
                ObservationKind::CallDenied,
                ObservationKind::ResultRecorded,
                ObservationKind::PeerCallReplied,
            ]
        );
        let result_entry = entries
            .iter()
            .find(|entry| entry.observation.kind == ObservationKind::ResultRecorded)
            .unwrap();
        let reply_entry = entries
            .iter()
            .find(|entry| entry.observation.kind == ObservationKind::PeerCallReplied)
            .unwrap();
        assert_eq!(result_entry.durability_class, DurabilityClass::Buffered);
        assert_eq!(reply_entry.durability_class, DurabilityClass::Buffered);

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("call-resident-payload-e2e"));
        assert!(ledger_text.contains("payload:request:size="));
        assert!(ledger_text.contains("payload:result:size="));
        assert!(!ledger_text.contains("payload-marker"));
        assert!(!ledger_text.contains("processed:"));
        assert!(!ledger_text.contains(&payload_base64));
        assert!(!ledger_text.contains(&expected_result_base64));
        assert!(!ledger_text.contains(&signature_marker));
    }

    #[tokio::test]
    async fn jvm_bridge_json_call_enters_resident_route_path() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_payload_process_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let (mut request, payload) =
            jvm_bridge_protocol_request("patina:demo/control@0.1.0.run", r#"[{"from":"jvm"}]"#)
                .unwrap();
        request.call.call_id = CallId::new("call-jvm-bridge-test")
            .expect("string ID literal/generated value must be non-empty");
        assert_eq!(request.call.origin, CallOrigin::JvmAdapter);

        let result = execute_resident_call(
            ResidentExecutionPaths {
                config_path,
                children_dir,
                state_path,
            },
            ledger.clone(),
            request,
            ResidentPayloadIngress::local(Some(payload)),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Completed);
        let result_payload = result
            .inline_result_payload
            .expect("result payload returned");
        assert_eq!(
            String::from_utf8(result_payload).unwrap(),
            r#"processed:[{"from":"jvm"}]"#
        );
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("call-jvm-bridge-test"));
        assert!(
            ledger_text.contains("RouteRevalidated") || ledger_text.contains("route_revalidated")
        );
    }

    /// Covers `PerHopPeerAccountability.UpstreamIdentityRemainsAtItsVerifier`.
    #[test]
    fn forwarded_envelope_clears_upstream_user_identity() {
        let fixture = remote_surface_candidate_fixture();
        let local_identity = fixture.config.local_identity.as_ref().unwrap();
        let peer = &fixture.config.peers["remote-mct"];
        let outbound = peer.outbound_binding.as_ref().unwrap();
        let mut original = resident_test_protocol_request(fixture.call.clone());
        original.call.caller.user_id = Some(UserId::new("upstream-user").unwrap());
        let hello = MctHelloResponse {
            response_id: "response-forwarded-identity".into(),
            request_id: "hello-forwarded-identity".into(),
            decision_id: DecisionId::new("decision-forwarded-identity").unwrap(),
            hello_outcome: HelloOutcome::Admitted,
            negotiated_protocol: Some(HelloPolicy::default().protocol),
            accepted_alpns: vec![MCT_CALL_ALPN.into()],
            safe_message: "admitted".into(),
            retry_after: None,
            capability_view: None,
            response_observation_id: ObservationId::new("obs-forwarded-identity").unwrap(),
        };

        let forwarded = resident_forwarded_call_request(
            &original,
            local_identity,
            peer,
            outbound,
            &local_identity.endpoint_id,
            &hello,
            None,
        );

        assert_eq!(
            original.call.caller.user_id.as_ref().unwrap().as_str(),
            "upstream-user"
        );
        assert!(forwarded.call.caller.user_id.is_none());
        assert_eq!(forwarded.call.caller.node_id, local_identity.node_id);
        assert_eq!(forwarded.call.call_id, original.call.call_id);
        assert_eq!(
            forwarded.call.trace_context.trace_id,
            original.call.trace_context.trace_id
        );
    }

    #[test]
    fn two_mother_bad_payload_fails_closed() {
        let reply = remote_reply_fixture(
            CallProtocolReplyOutcome::Malformed,
            "malformed call payload",
        );
        let handler = remote_reply_to_call_handler_result(
            reply,
            DecisionId::new("route-revalidation:bad-payload")
                .expect("string ID literal/generated value must be non-empty"),
            remote_route_taken_fixture(),
        );

        assert_eq!(handler.outcome, CallProtocolOutcome::Failed);
        assert_eq!(handler.safe_message, "malformed call payload");
        assert!(handler.route_taken.is_none());
    }

    #[test]
    fn two_mother_remote_denial_fails_closed() {
        let reply = remote_reply_fixture(CallProtocolReplyOutcome::Denied, "not authorized");
        let handler = remote_reply_to_call_handler_result(
            reply,
            DecisionId::new("route-revalidation:remote-denial")
                .expect("string ID literal/generated value must be non-empty"),
            remote_route_taken_fixture(),
        );

        assert_eq!(handler.outcome, CallProtocolOutcome::Denied);
        assert!(handler.route_taken.is_none());
    }

    async fn post_resident_peer_mutation(
        socket_path: &Path,
        path: &str,
        body: serde_json::Value,
    ) -> serde_json::Value {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let body = serde_json::to_vec(&body).unwrap();
        let mut control = tokio::net::UnixStream::connect(socket_path).await.unwrap();
        control
            .write_all(
                format!(
                    "POST {path} HTTP/1.1\r\nHost: local\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        control.write_all(&body).await.unwrap();
        let mut response = Vec::new();
        control.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
    }

    async fn poll_resident_status(
        socket_path: &Path,
        ready: impl Fn(&MctDaemonStatus) -> bool,
    ) -> MctDaemonStatus {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut last = None;
        for _ in 0..40 {
            let mut control = tokio::net::UnixStream::connect(socket_path).await.unwrap();
            control
                .write_all(b"GET /status HTTP/1.1\r\nHost: local\r\n\r\n")
                .await
                .unwrap();
            let mut response = vec![0; 4096];
            let read = control.read(&mut response).await.unwrap();
            let response = String::from_utf8_lossy(&response[..read]);
            assert!(response.starts_with("HTTP/1.1 200"), "{response}");
            let (_, body) = response
                .split_once("\r\n\r\n")
                .expect("HTTP response separates headers from body");
            let status: MctDaemonStatus = serde_json::from_str(body).unwrap();
            if ready(&status) {
                return status;
            }
            last = Some(status);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("resident status did not become ready: {last:?}");
    }

    pub(crate) fn resident_test_call(trace_id: TraceId) -> MctCall {
        let mut call = local_wasm_call(OperationTarget {
            namespace: "patina:demo".into(),
            interface_name: "control@0.1.0".into(),
            function_name: "run".into(),
        });
        call.call_id = CallId::new("call-resident-wit")
            .expect("string ID literal/generated value must be non-empty");
        call.trace_context.trace_id = trace_id;
        call.origin = CallOrigin::Iroh;
        call
    }

    struct RemoteSurfaceCandidateFixture {
        _dir: tempfile::TempDir,
        config_path: PathBuf,
        state_path: PathBuf,
        remote_secret: String,
        config: mct_daemon::MctDaemonConfig,
        state: MctRuntimeStateStore,
        call: MctCall,
    }

    fn remote_route_taken_fixture() -> RouteTaken {
        RouteTaken {
            node_id: MctNodeId::new("remote-mct")
                .expect("string ID literal/generated value must be non-empty"),
            child_id: Some(
                ChildId::new("remote-child")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            runtime_kind: RuntimeKind::RemotePeer,
        }
    }

    fn remote_reply_fixture(
        reply_outcome: CallProtocolReplyOutcome,
        safe_message: &str,
    ) -> MctIrohCallPayloadReply {
        MctIrohCallPayloadReply {
            reply: MctCallProtocolReply {
                reply_id: ReplyId::new("reply-remote-fixture")
                    .expect("string ID literal/generated value must be non-empty"),
                protocol_request_id: ProtocolRequestId::new("proto-remote-fixture")
                    .expect("string ID literal/generated value must be non-empty"),
                decision_id: DecisionId::new("decision-remote-fixture")
                    .expect("string ID literal/generated value must be non-empty"),
                result_ref: None,
                result_payload: MctCallPayloadHandle::Empty,
                route_taken: None,
                reply_outcome,
                safe_message: safe_message.into(),
                reply_observation_id: ObservationId::new("obs-reply-remote-fixture")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            inline_result_payload: None,
        }
    }

    fn remote_surface_candidate_fixture() -> RemoteSurfaceCandidateFixture {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let state_path = dir.path().join("state.sqlite");
        let local_identity_path = dir.path().join("identity").join("iroh-secret.hex");
        let remote_identity_path = dir.path().join("remote").join("iroh-secret.hex");
        let store = MctDaemonConfigStore::new(&config_path);
        let local_identity = store
            .ensure_local_identity(MctOperatorNodeScope::default(), &local_identity_path)
            .unwrap();
        let remote_secret = load_or_create_node_secret_key_hex(&remote_identity_path).unwrap();
        let remote_endpoint_id = endpoint_id_for_secret_key_hex(&remote_secret).unwrap();
        store
            .upsert_peer(resident_remote_peer_entry(
                "remote-mct",
                "binding-remote",
                remote_endpoint_id.as_str(),
                "vision-local",
                BindingState::Admitted,
                None,
            ))
            .unwrap();
        store
            .approve_and_assign_loaded_child(&test_child(), MctOperatorChildScope::default())
            .unwrap();
        let mut config = store.load().unwrap();
        let peer = config.peers.get("remote-mct").unwrap().clone();
        let outbound_binding = MctOutboundPeerBindingPresentation {
            binding_id: PeerBindingId::new("binding-outbound-local")
                .expect("string ID literal/generated value must be non-empty"),
            policy_revision: 1,
            signature_ref: String::new(),
            expires_at: contract_peer_expiry(),
        };
        let outbound_binding_to_sign =
            outbound_peer_binding_for_local(&local_identity, &peer, &outbound_binding).unwrap();
        let outbound_signature = sign_peer_binding_signature_ref(
            &remote_secret,
            &outbound_binding_to_sign,
            &remote_endpoint_id,
        )
        .unwrap();
        store
            .set_peer_outbound_proof(
                &peer.peer_node_id,
                MctOutboundPeerBindingPresentation {
                    signature_ref: outbound_signature,
                    ..outbound_binding
                },
            )
            .unwrap();
        config = store.load().unwrap();
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        let view = hello_capability_view(
            &peer.peer_node_id,
            &peer.vision_id,
            1,
            &["patina:demo/control@0.1.0.run"],
        );
        state
            .refresh_remote_callable_surfaces(MctRemoteSurfaceRefresh {
                peer_node_id: &peer.peer_node_id,
                binding_id: &peer.binding_id,
                endpoint_id: &peer.endpoint_id,
                view: &view,
                received_at: &Timestamp::new("2026-07-09T00:00:00Z").unwrap(),
                stale_at: &Timestamp::new("2026-07-09T00:05:00Z").unwrap(),
                view_observation_id: &ObservationId::new("obs-remote-surface-view")
                    .expect("string ID literal/generated value must be non-empty"),
            })
            .unwrap();
        let mut call = resident_test_call(
            TraceId::new("trace-remote-route-candidate")
                .expect("string ID literal/generated value must be non-empty"),
        );
        call.origin = CallOrigin::Cli;
        RemoteSurfaceCandidateFixture {
            _dir: dir,
            config_path,
            state_path,
            remote_secret,
            config,
            state,
            call,
        }
    }

    fn resident_remote_peer_entry(
        peer_node_id: &str,
        binding_id: &str,
        endpoint_id: &str,
        vision_id: &str,
        binding_state: BindingState,
        binding_signature_ref: Option<String>,
    ) -> MctPeerAddressBookEntry {
        MctPeerAddressBookEntry {
            peer_node_id: MctNodeId::new(peer_node_id)
                .expect("string ID literal/generated value must be non-empty"),
            binding_id: PeerBindingId::new(binding_id)
                .expect("string ID literal/generated value must be non-empty"),
            endpoint_id: EndpointIdText::new(endpoint_id)
                .expect("string ID literal/generated value must be non-empty"),
            vision_id: VisionId::new(vision_id)
                .expect("string ID literal/generated value must be non-empty"),
            ticket: Some(MotherIrohEndpointTicket {
                endpoint_id: EndpointIdText::new(endpoint_id)
                    .expect("string ID literal/generated value must be non-empty"),
                direct_addresses: vec!["127.0.0.1:12345".into()],
                relay_urls: Vec::new(),
            }),
            binding_signature_ref,
            outbound_binding: None,
            binding_state,
            policy_revision: 1,
            expires_at: contract_peer_expiry(),
            updated_at: "2026-07-09T00:00:00Z".into(),
        }
    }

    fn hello_capability_view(
        node_id: &MctNodeId,
        vision_id: &VisionId,
        policy_revision: u64,
        operations: &[&str],
    ) -> MctHelloCapabilityView {
        MctHelloCapabilityView {
            node_id: node_id.clone(),
            vision_id: vision_id.clone(),
            published_at: Timestamp::new("2026-07-09T00:00:00Z").unwrap(),
            policy_revision,
            supported_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            supported_wit_worlds: vec!["patina:demo/control@0.1.0".into()],
            supported_observation_modes: vec!["local-ledger".into()],
            callable_surfaces: operations
                .iter()
                .map(|operation| MctHelloCallableSurface {
                    child_name: "remote-child".into(),
                    operation_id: (*operation).into(),
                    runtime_kind: RuntimeKind::WasmComponent,
                    vision_id: vision_id.clone(),
                    policy_revision,
                    visibility: "vision_scoped".into(),
                })
                .collect(),
            capability_view_ref: None,
        }
    }

    pub(crate) fn resident_test_protocol_request(call: MctCall) -> MctCallProtocolRequest {
        MctCallProtocolRequest {
            protocol_request_id: ProtocolRequestId::new("proto-resident-wit")
                .expect("string ID literal/generated value must be non-empty"),
            authority: MctCallProtocolAuthority {
                hello_decision_id: DecisionId::new("decision-resident-wit-hello")
                    .expect("string ID literal/generated value must be non-empty"),
                peer_binding_id: PeerBindingId::new("binding-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                accepted_alpn: MCT_CALL_ALPN.into(),
                endpoint_id: EndpointIdText::new("endpoint-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                policy_revision: 1,
                grants_revision: 1,
            },
            received_over: IrohConnectionPresentation {
                endpoint_id: EndpointIdText::new("endpoint-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                alpn: MCT_CALL_ALPN.into(),
                connection_side: ConnectionSide::Incoming,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            call,
            payload: MctCallPayloadHandle::Empty,
            idempotency_key: None,
            received_observation_id: ObservationId::new("obs-resident-wit-received")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    pub(crate) fn write_resident_process_child(children_dir: &Path) {
        write_resident_process_child_script(
            children_dir,
            "resident-echo",
            b"#!/bin/sh\ncat >/dev/null\nprintf '{\\\"ok\\\":true}'\n",
        );
    }

    fn write_resident_payload_process_child(children_dir: &Path) {
        write_resident_process_child_script(
            children_dir,
            "resident-payload-echo",
            b"#!/bin/sh\npayload=$(cat)\nprintf 'processed:%s' \"$payload\"\n",
        );
    }

    fn write_resident_process_child_script(children_dir: &Path, name: &str, script: &[u8]) {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        let child_dir = children_dir.join(name);
        std::fs::create_dir_all(&child_dir).unwrap();
        let artifact_path = child_dir.join(format!("{name}.wasm"));
        let manifest_path = child_dir.join("child.toml");
        std::fs::write(&artifact_path, script).unwrap();
        #[cfg(unix)]
        {
            let mut permissions = std::fs::metadata(&artifact_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&artifact_path, permissions).unwrap();
        }
        write_resident_child_manifest(&manifest_path, name, "handle");
        write_sha256_sidecar(&artifact_path, script);
        let manifest_bytes = std::fs::read(&manifest_path).unwrap();
        write_sha256_sidecar(&manifest_path, &manifest_bytes);
    }

    fn write_resident_child_manifest(manifest_path: &Path, name: &str, mode: &str) {
        std::fs::write(
            manifest_path,
            format!(
                r#"[child]
name = "{name}"
version = "0.1.0"
description = "resident test child"
kind = "child"
role = "app"

[child.ingress]
mode = "{mode}"

[child.artifact]
wasm = "{name}.wasm"

[child.contract]
allow = ["patina:demo/control@0.1.0.run"]

[needs]
toys = []

[relationships]
listens = []
"#
            ),
        )
        .unwrap();
    }

    fn write_sha256_sidecar(path: &Path, bytes: &[u8]) {
        use sha2::{Digest, Sha256};

        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(".sha256");
        std::fs::write(
            PathBuf::from(sidecar),
            format!("{:x}", Sha256::digest(bytes)),
        )
        .unwrap();
    }

    #[test]
    fn resident_status_source_reflects_closed_endpoint() {
        let endpoint = Arc::new(Mutex::new(MotherIrohEndpointSnapshot {
            endpoint_id: EndpointIdText::new("endpoint-resident-status")
                .expect("string ID literal/generated value must be non-empty"),
            lifecycle: mct_iroh::MotherIrohEndpointLifecycle::Bound,
            accepted_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            direct_addresses: Vec::new(),
            relay_urls: Vec::new(),
            relay_mode: MotherIrohRelayMode::Disabled,
        }));
        let source = ResidentStatusSource {
            endpoint: Arc::clone(&endpoint),
            accepted_connection_count: Arc::new(AtomicU64::new(3)),
            loaded_child_count: 2,
            approved_child_count: 1,
            binding_count: 4,
            ledger_path: PathBuf::from("/path/that/does/not/exist.jsonl"),
        };

        let live = source.status();
        assert_eq!(live.readiness, mct_daemon::MctDaemonReadiness::Ready);
        assert_eq!(live.resident.unwrap().accepted_connection_count, 3);

        endpoint.lock().unwrap().lifecycle = mct_iroh::MotherIrohEndpointLifecycle::Closed;
        let closed = source.status();
        assert_eq!(closed.readiness, mct_daemon::MctDaemonReadiness::NotReady);
        assert_eq!(closed.safe_message, "iroh endpoint not ready");
    }

    #[tokio::test]
    async fn control_snapshot_unopenable_state_projects_error_response() {
        let dir = tempfile::tempdir().unwrap();
        let source = ControlSnapshotSource::open(dir.path());

        let snapshot = control_snapshot(&source).await;
        let response = mct_daemon::handle_control_plane_path_result_with_auth(
            "GET",
            "/snapshot",
            snapshot.as_ref(),
            &mct_daemon::MctControlPlaneAuthPolicy::open_local(),
            None,
        );

        assert!(matches!(
            snapshot,
            Err(MctControlPlaneSnapshotError::RuntimeStateUnavailable { .. })
        ));
        assert_eq!(response.status_code, 503);
        assert!(response.body.contains("runtime state unavailable"));
        assert!(response.body.contains("not_ready"));
        assert!(!response.body.contains("\"ready\""));
    }

    #[test]
    fn authorize_secret_cli_persists_scoped_grant_without_value() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let state_path = dir.path().join("state.sqlite");
        let children_dir = dir.path().join("children");
        write_resident_process_child(&children_dir);
        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        let child = loaded
            .children
            .iter()
            .find(|child| child.name == "resident-echo")
            .unwrap();
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(child, MctOperatorChildScope::default())
            .unwrap();

        run_toys_authorize_secret(vec![
            "resident-echo".into(),
            "api-token".into(),
            "--children-dir".into(),
            children_dir.display().to_string(),
            "--config".into(),
            config_path.display().to_string(),
            "--state".into(),
            state_path.display().to_string(),
            "--ledger".into(),
            dir.path().join("observations.jsonl").display().to_string(),
            "--uds".into(),
            dir.path().join("missing.sock").display().to_string(),
        ])
        .unwrap();

        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        let contracts = state.toy_contracts().unwrap();
        let grants = state.toy_grant_snapshots().unwrap();
        assert_eq!(contracts, vec![mct_secrets_toy_contract()]);
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].toy_id.as_str(), MCT_SECRETS_TOY_ID);
        assert_eq!(grants[0].scope.resource_id.as_deref(), Some("api-token"));
        let grant_json = serde_json::to_string(&grants).unwrap();
        assert!(!grant_json.contains("super-secret"));
    }

    #[test]
    fn authorize_cli_toy_denies_expired_grant_against_current_time() {
        let child = test_child();
        let authorized_child = test_authorized_child();
        let call = test_call();
        let toy_id =
            ToyId::new("toy-demo").expect("string ID literal/generated value must be non-empty");
        let contracts = vec![test_contract(&toy_id)];
        let grants = vec![expired_grant(&toy_id)];

        let result = authorize_cli_toy(CliToyAuthorizationRequest {
            child: &child,
            authorized_child: &authorized_child,
            call: &call,
            contracts: &contracts,
            grants: &grants,
            toy_id,
            action: "read",
            resource_id: Some("resource-a".into()),
            label: "expired",
        });

        let Err(error) = result else {
            panic!("expired grant must deny");
        };
        assert!(error.safe_message.contains("ExpiredGrant"));
        assert_eq!(error.observations[0].outcome, ObservationOutcome::Denied);
    }
}

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

#[path = "resident/forwarding.rs"]
mod forwarding;
use forwarding::*;

#[path = "resident/pipeline.rs"]
mod pipeline;
pub(super) use pipeline::*;

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
    let execution_paths = ResidentRuntimePaths::new(
        config.config_path.clone(),
        config.children_dir.clone(),
        config.state_path.clone(),
    );
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

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

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

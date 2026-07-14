//! Resident Mother process bootstrap, endpoint/control/event tasks, shutdown, and status.

use super::*;

pub(crate) async fn run_serve(mut args: Vec<String>) -> Result<()> {
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
        (None, None) => ResidentControlTransport::Uds(default_control_uds_path()),
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
enum ResidentControlTransport {
    Http(String),
    Uds(PathBuf),
}

#[derive(Clone, Debug)]
struct ResidentMotherConfig {
    pub(super) config_path: PathBuf,
    pub(super) identity_path: PathBuf,
    pub(super) children_dir: PathBuf,
    pub(super) state_path: PathBuf,
    ledger_path: PathBuf,
    pub(super) control: ResidentControlTransport,
    pub(super) relay_default: bool,
    pub(super) max_concurrent_connections: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct ResidentStatusSource {
    endpoint: Arc<Mutex<MotherIrohEndpointSnapshot>>,
    node_id: MctNodeId,
    vision_id: VisionId,
    accepted_connection_count: Arc<AtomicU64>,
    loaded_child_count: usize,
    approved_child_count: usize,
    binding_count: usize,
    ledger_path: PathBuf,
}

impl ResidentStatusSource {
    fn new(
        endpoint: Arc<Mutex<MotherIrohEndpointSnapshot>>,
        identity: (MctNodeId, VisionId),
        accepted_connection_count: Arc<AtomicU64>,
        loaded_child_count: usize,
        approved_child_count: usize,
        binding_count: usize,
        ledger_path: PathBuf,
    ) -> Self {
        Self {
            endpoint,
            node_id: identity.0,
            vision_id: identity.1,
            accepted_connection_count,
            loaded_child_count,
            approved_child_count,
            binding_count,
            ledger_path,
        }
    }

    pub(crate) fn status(&self) -> MctDaemonStatus {
        daemon_status_with_resident(
            Some(
                self.endpoint
                    .lock()
                    .expect("resident endpoint status lock must not be poisoned")
                    .clone(),
            ),
            Some(MctResidentStatus {
                node_id: self.node_id.clone(),
                vision_id: self.vision_id.clone(),
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

#[cfg(test)]
pub(super) async fn run_test_resident_mother<S>(
    paths: ResidentRuntimePaths,
    identity_path: PathBuf,
    ledger_path: PathBuf,
    socket_path: PathBuf,
    shutdown: S,
    ready: Option<tokio::sync::oneshot::Sender<MotherIrohEndpointTicket>>,
) -> Result<()>
where
    S: std::future::Future<Output = ()> + Send,
{
    run_resident_mother(
        ResidentMotherConfig {
            config_path: paths.config_path().to_path_buf(),
            identity_path,
            children_dir: paths.children_dir().to_path_buf(),
            state_path: paths.state_path().to_path_buf(),
            ledger_path,
            control: ResidentControlTransport::Uds(socket_path),
            relay_default: false,
            max_concurrent_connections: 8,
        },
        shutdown,
        ready,
    )
    .await
}

async fn run_resident_mother<S>(
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
    let status_source = Arc::new(ResidentStatusSource::new(
        Arc::clone(&endpoint_status),
        (identity.node_id.clone(), identity.vision_id.clone()),
        Arc::clone(&accepted_connection_count),
        loaded_child_count,
        approved_child_count,
        binding_count,
        config.ledger_path.clone(),
    ));

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
        ResidentRuntimePaths::new(
            config.config_path.clone(),
            config.children_dir.clone(),
            config.state_path.clone(),
        ),
        ledger.clone(),
        config.max_concurrent_connections,
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

fn spawn_resident_control_task(
    control: ResidentControlTransport,
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    max_concurrent_connections: usize,
    shutdown: broadcast::Receiver<()>,
    status_source: Option<Arc<ResidentStatusSource>>,
) -> Result<tokio::task::JoinHandle<Result<()>>> {
    match control {
        ResidentControlTransport::Http(addr) => Ok(tokio::spawn(async move {
            serve_http_control_loop_until(
                paths.state_path().to_path_buf(),
                addr,
                shutdown,
                status_source,
            )
            .await
        })),
        ResidentControlTransport::Uds(path) => Ok(tokio::spawn(async move {
            run_control_serve_uds_with_state_until(
                paths,
                path,
                ledger,
                max_concurrent_connections,
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
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

    fn contract_peer_expiry() -> Timestamp {
        Timestamp::new("2099-01-01T00:00:00Z").unwrap()
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
    fn write_resident_process_child(children_dir: &Path) {
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

    async fn resident_uds_request(
        socket_path: &Path,
        request: Vec<u8>,
    ) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut stream = tokio::net::UnixStream::connect(socket_path).await.unwrap();
        stream.write_all(&request).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8(response).unwrap();
        let status = response
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let body = response.split_once("\r\n\r\n").unwrap().1;
        (status, serde_json::from_str(body).unwrap())
    }

    #[tokio::test]
    async fn resident_call_uds_executes_approved_child_and_projects_control_state() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let identity_path = dir.path().join("identity").join("iroh-secret.hex");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        write_resident_payload_process_child(&children_dir);

        let store = MctDaemonConfigStore::new(&config_path);
        store
            .ensure_local_identity(MctOperatorNodeScope::default(), &identity_path)
            .unwrap();
        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        store
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();

        let paths =
            ResidentRuntimePaths::new(config_path.clone(), children_dir, state_path.clone());
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let resident = tokio::spawn(run_test_resident_mother(
            paths,
            identity_path,
            ledger_path.clone(),
            socket_path.clone(),
            async move {
                let _ = shutdown_rx.await;
            },
            Some(ready_tx),
        ));
        tokio::time::timeout(Duration::from_secs(10), ready_rx)
            .await
            .unwrap()
            .unwrap();
        for _ in 0..40 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let status = poll_resident_status(&socket_path, |status| {
            status.readiness == mct_daemon::MctDaemonReadiness::Ready
        })
        .await;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                std::fs::metadata(&socket_path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let sequence_before = status.resident.unwrap().ledger_sequence_tip;

        let payload = br#"[{"from":"uds"}]"#;
        let body = serde_json::json!({
            "protocol_request_id": "proto-resident-uds",
            "call_id": "call-resident-uds",
            "target": {
                "namespace": "patina:demo",
                "interface_name": "control@0.1.0",
                "function_name": "run"
            },
            "payload_metadata": {
                "data_classification": "public",
                "size_bytes": payload.len(),
                "contains_secret_scoped_material": false
            },
            "authority_context": {
                "policy_revision": 1,
                "grants_revision": 1,
                "vision_policy_revision": 1
            },
            "deadline": "2099-01-01T00:00:00Z",
            "trace_context": {
                "trace_id": "trace-resident-uds",
                "span_id": "span-resident-uds"
            },
            "payload": {
                "payload_kind": "inline_payload",
                "inline_payload_ref": "payload-resident-uds",
                "content_type": "application/json",
                "size_bytes": payload.len(),
                "blake3_digest_hex": blake3::hash(payload).to_hex().to_string()
            },
            "inline_payload_base64": BASE64_STANDARD.encode(payload),
            "idempotency_key": "resident-uds-test-key"
        });
        let body = serde_json::to_vec(&body).unwrap();
        let request = [
            format!(
                "POST /calls HTTP/1.1\r\nHost: local\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                body.len()
            )
            .into_bytes(),
            body,
        ]
        .concat();
        let (call_status, call_reply) = resident_uds_request(&socket_path, request).await;
        assert_eq!(call_status, 200, "{call_reply:#}");
        assert_eq!(call_reply["outcome"], "completed");
        let result_payload = BASE64_STANDARD
            .decode(call_reply["inline_result_payload_base64"].as_str().unwrap())
            .unwrap();
        assert_eq!(result_payload, br#"processed:[{"from":"uds"}]"#);
        assert_eq!(
            call_reply["result_payload"]["blake3_digest_hex"],
            blake3::hash(&result_payload).to_hex().to_string()
        );

        let (runs_status, runs) = resident_uds_request(
            &socket_path,
            b"GET /runs HTTP/1.1\r\nHost: local\r\n\r\n".to_vec(),
        )
        .await;
        assert_eq!(runs_status, 200);
        let run = runs
            .as_array()
            .unwrap()
            .iter()
            .find(|run| run["call_id"] == "call-resident-uds")
            .expect("resident call is visible through /runs");
        assert_eq!(run["state"], "completed");
        assert_eq!(run["result"]["outcome"], "success");
        assert!(run["authority_decision_id"].is_string());

        let status = poll_resident_status(&socket_path, |status| {
            status
                .resident
                .as_ref()
                .is_some_and(|resident| resident.ledger_sequence_tip > sequence_before)
        })
        .await;
        assert_eq!(status.readiness, mct_daemon::MctDaemonReadiness::Ready);
        let cli_socket_path = socket_path.clone();
        let cli_status =
            tokio::task::spawn_blocking(move || query_resident_status(&cli_socket_path))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(cli_status.readiness, MctDaemonReadiness::Ready);
        assert_eq!(cli_status.node_id.as_str(), "local-mct");
        assert_eq!(cli_status.vision_id.as_str(), "vision-local");
        assert_eq!(cli_status.loaded_child_count, 1);
        assert_eq!(cli_status.approved_child_count, 1);
        assert!(cli_status.last_observation_sequence > sequence_before);

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("call-resident-uds"));
        assert!(ledger_text.contains("call_constructed"));
        assert!(ledger_text.contains("result_recorded"));
        assert!(!ledger_text.contains(&BASE64_STANDARD.encode(payload)));
        assert!(!ledger_text.contains(std::str::from_utf8(payload).unwrap()));

        let _ = shutdown_tx.send(());
        tokio::time::timeout(Duration::from_secs(10), resident)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
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
        let source = ResidentStatusSource::new(
            Arc::clone(&endpoint),
            (
                MctNodeId::new("node-status-test").unwrap(),
                VisionId::new("vision-status-test").unwrap(),
            ),
            Arc::new(AtomicU64::new(3)),
            2,
            1,
            4,
            PathBuf::from("/path/that/does/not/exist.jsonl"),
        );

        let live = source.status();
        assert_eq!(live.readiness, mct_daemon::MctDaemonReadiness::Ready);
        assert_eq!(live.resident.unwrap().accepted_connection_count, 3);

        endpoint.lock().unwrap().lifecycle = mct_iroh::MotherIrohEndpointLifecycle::Closed;
        let closed = source.status();
        assert_eq!(closed.readiness, mct_daemon::MctDaemonReadiness::NotReady);
        assert_eq!(closed.safe_message, "iroh endpoint not ready");
    }
}

use super::*;

const PEER_MUTATION_BODY_MAX_BYTES: usize = 64 * 1024;
static NEXT_PEER_MUTATION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PeerAddRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) peer_node_id: MctNodeId,
    pub(super) binding_id: PeerBindingId,
    pub(super) endpoint_id: EndpointIdText,
    pub(super) vision_id: VisionId,
    pub(super) ticket: Option<MotherIrohEndpointTicket>,
    pub(super) binding_signature_ref: Option<String>,
    pub(super) policy_revision: u64,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PeerProofRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) peer_node_id: MctNodeId,
    pub(super) binding_id: PeerBindingId,
    pub(super) policy_revision: u64,
    pub(super) signature_ref: String,
    pub(super) expires_at: Option<Timestamp>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PeerNodeRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) peer_node_id: MctNodeId,
}

#[derive(Clone, Debug)]
enum PreparedPeerMutationEffect {
    Add(Box<MctPeerAddressBookEntry>),
    Proof {
        peer_node_id: MctNodeId,
        outbound: MctOutboundPeerBindingPresentation,
    },
    Revoke(MctNodeId),
    Remove(MctNodeId),
}

#[derive(Clone, Debug)]
struct PreparedPeerMutation {
    config_path: PathBuf,
    action: &'static str,
    peer_node_id: MctNodeId,
    binding_id: PeerBindingId,
    endpoint_id: EndpointIdText,
    vision_id: VisionId,
    policy_revision: u64,
    binding_state: BindingState,
    expires_at: Option<Timestamp>,
    effect: PreparedPeerMutationEffect,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub(super) struct PeerMutationSuccess {
    pub(super) action: String,
    pub(super) peer_node_id: MctNodeId,
    pub(super) binding_id: PeerBindingId,
    pub(super) endpoint_id: EndpointIdText,
    pub(super) vision_id: VisionId,
    pub(super) policy_revision: u64,
    pub(super) binding_state: BindingState,
    pub(super) expires_at: Option<Timestamp>,
    pub(super) peer_count: usize,
}

fn normalize_config_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}

fn require_expected_config_path(expected: &Path, configured: &Path) -> Result<()> {
    if normalize_config_path(expected)? != normalize_config_path(configured)? {
        bail!("peer mutation expected config path does not match resident config");
    }
    Ok(())
}

fn peer_mutation_observation(
    prepared: &PreparedPeerMutation,
    kind: ObservationKind,
    outcome: ObservationOutcome,
) -> MctObservation {
    let sequence = NEXT_PEER_MUTATION_ID.fetch_add(1, Ordering::Relaxed);
    let observed_at = current_timestamp();
    let id = format!("peer-mutation-{observed_at}-{sequence}");
    MctObservation {
        observation_id: ObservationId::new(format!("obs:{id}"))
            .expect("generated observation ID must be non-empty"),
        observed_at,
        kind,
        source_plane: if outcome == ObservationOutcome::Failed {
            SourcePlane::Operator
        } else {
            SourcePlane::Kernel
        },
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace:{id}"))
                .expect("generated trace ID must be non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: Some(
            DecisionId::new(format!("decision:{id}"))
                .expect("generated decision ID must be non-empty"),
        ),
        subject_id: Some(prepared.peer_node_id.to_string()),
        resource_id: Some(prepared.binding_id.to_string()),
        policy_revision: Some(prepared.policy_revision),
        grants_revision: None,
        outcome,
        visibility: ObservationVisibility::NodeOperator,
        safe_message: format!(
            "peer authority action={} endpoint={} vision={} state={:?} expires_at={}",
            prepared.action,
            prepared.endpoint_id,
            prepared.vision_id,
            prepared.binding_state,
            prepared
                .expires_at
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "-".into())
        ),
        detail_ref: None,
    }
}

fn prepare_peer_mutation(
    configured_path: &Path,
    path: &str,
    body: &[u8],
) -> Result<PreparedPeerMutation> {
    if body.len() > PEER_MUTATION_BODY_MAX_BYTES {
        bail!("peer mutation body exceeds 64 KiB limit");
    }
    let store = MctDaemonConfigStore::new(configured_path);
    let (
        action,
        peer_node_id,
        binding_id,
        endpoint_id,
        vision_id,
        policy_revision,
        state,
        expires_at,
        effect,
    ) = match path {
        "/peers/add" => {
            let request: PeerAddRequest =
                serde_json::from_slice(body).context("decode peer add request")?;
            require_expected_config_path(&request.expected_config_path, configured_path)?;
            if request.policy_revision == 0 {
                bail!("peer policy revision must be greater than zero");
            }
            if request
                .binding_signature_ref
                .as_ref()
                .is_some_and(|proof| proof.trim().is_empty())
            {
                bail!("peer binding signature reference must not be empty");
            }
            let _current_config = store.load()?;
            let entry = MctPeerAddressBookEntry {
                peer_node_id: request.peer_node_id.clone(),
                binding_id: request.binding_id.clone(),
                endpoint_id: request.endpoint_id.clone(),
                vision_id: request.vision_id.clone(),
                ticket: request.ticket,
                binding_signature_ref: request.binding_signature_ref,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: request.policy_revision,
                updated_at: mct_daemon::current_timestamp_string(),
            };
            (
                "add",
                request.peer_node_id,
                request.binding_id,
                request.endpoint_id,
                request.vision_id,
                request.policy_revision,
                BindingState::Admitted,
                None,
                PreparedPeerMutationEffect::Add(Box::new(entry)),
            )
        }
        "/peers/proof" => {
            let request: PeerProofRequest =
                serde_json::from_slice(body).context("decode peer proof request")?;
            require_expected_config_path(&request.expected_config_path, configured_path)?;
            if request.policy_revision == 0 {
                bail!("peer policy revision must be greater than zero");
            }
            if request.signature_ref.trim().is_empty() {
                bail!("outbound binding signature reference must not be empty");
            }
            let config = store.load()?;
            let peer = config
                .peers
                .get(request.peer_node_id.as_str())
                .ok_or_else(|| anyhow::anyhow!("peer not found in config"))?;
            let outbound = MctOutboundPeerBindingPresentation {
                binding_id: request.binding_id.clone(),
                policy_revision: request.policy_revision,
                signature_ref: request.signature_ref,
                expires_at: request.expires_at.clone(),
            };
            (
                "proof",
                request.peer_node_id.clone(),
                request.binding_id,
                peer.endpoint_id.clone(),
                peer.vision_id.clone(),
                request.policy_revision,
                peer.binding_state,
                request.expires_at,
                PreparedPeerMutationEffect::Proof {
                    peer_node_id: request.peer_node_id,
                    outbound,
                },
            )
        }
        "/peers/revoke" | "/peers/remove" => {
            let request: PeerNodeRequest =
                serde_json::from_slice(body).context("decode peer mutation request")?;
            require_expected_config_path(&request.expected_config_path, configured_path)?;
            let config = store.load()?;
            let peer = config
                .peers
                .get(request.peer_node_id.as_str())
                .ok_or_else(|| anyhow::anyhow!("peer not found in config"))?;
            let remove = path == "/peers/remove";
            (
                if remove { "remove" } else { "revoke" },
                request.peer_node_id.clone(),
                peer.binding_id.clone(),
                peer.endpoint_id.clone(),
                peer.vision_id.clone(),
                peer.policy_revision,
                BindingState::Revoked,
                None,
                if remove {
                    PreparedPeerMutationEffect::Remove(request.peer_node_id)
                } else {
                    PreparedPeerMutationEffect::Revoke(request.peer_node_id)
                },
            )
        }
        _ => bail!("unknown peer mutation route"),
    };
    Ok(PreparedPeerMutation {
        config_path: configured_path.to_path_buf(),
        action,
        peer_node_id,
        binding_id,
        endpoint_id,
        vision_id,
        policy_revision,
        binding_state: state,
        expires_at,
        effect,
    })
}

impl PreparedPeerMutation {
    fn apply(&self) -> Result<PeerMutationSuccess> {
        let store = MctDaemonConfigStore::new(&self.config_path);
        let config = match &self.effect {
            PreparedPeerMutationEffect::Add(entry) => store.upsert_peer(entry.as_ref().clone())?,
            PreparedPeerMutationEffect::Proof {
                peer_node_id,
                outbound,
            } => store.set_peer_outbound_proof(peer_node_id, outbound.clone())?,
            PreparedPeerMutationEffect::Revoke(peer_node_id) => store.revoke_peer(peer_node_id)?,
            PreparedPeerMutationEffect::Remove(peer_node_id) => store.remove_peer(peer_node_id)?,
        };
        Ok(PeerMutationSuccess {
            action: self.action.into(),
            peer_node_id: self.peer_node_id.clone(),
            binding_id: self.binding_id.clone(),
            endpoint_id: self.endpoint_id.clone(),
            vision_id: self.vision_id.clone(),
            policy_revision: self.policy_revision,
            binding_state: self.binding_state,
            expires_at: self.expires_at.clone(),
            peer_count: config.peers.len(),
        })
    }

    fn decision_observation(&self) -> MctObservation {
        let kind = match self.effect {
            PreparedPeerMutationEffect::Add(_) | PreparedPeerMutationEffect::Proof { .. } => {
                ObservationKind::PeerBindingRecorded
            }
            PreparedPeerMutationEffect::Revoke(_) | PreparedPeerMutationEffect::Remove(_) => {
                ObservationKind::PeerBindingRevoked
            }
        };
        peer_mutation_observation(self, kind, ObservationOutcome::Allowed)
    }

    fn failure_observation(&self) -> MctObservation {
        peer_mutation_observation(
            self,
            ObservationKind::OperatorActionRecorded,
            ObservationOutcome::Failed,
        )
    }
}

fn peer_mutation_response(status_code: u16, body: serde_json::Value) -> MctControlPlaneResponse {
    MctControlPlaneResponse {
        status_code,
        content_type: "application/json".into(),
        body: body.to_string(),
    }
}

async fn execute_resident_peer_mutation(
    configured_path: &Path,
    ledger: &ResidentLedgerWriter,
    path: &str,
    body: &[u8],
) -> MctControlPlaneResponse {
    let prepared = match prepare_peer_mutation(configured_path, path, body) {
        Ok(prepared) => prepared,
        Err(_) => {
            return peer_mutation_response(
                400,
                serde_json::json!({"error": "peer mutation rejected"}),
            );
        }
    };
    if ledger
        .append(vec![prepared.decision_observation()])
        .await
        .is_err()
    {
        return peer_mutation_response(
            500,
            serde_json::json!({"error": "peer mutation decision was not durable"}),
        );
    }
    match prepared.apply() {
        Ok(response) => peer_mutation_response(
            200,
            serde_json::to_value(response)
                .expect("peer mutation response serialization must succeed"),
        ),
        Err(_) => {
            let failure_is_durable = ledger
                .append(vec![prepared.failure_observation()])
                .await
                .is_ok();
            let message = if failure_is_durable {
                "peer mutation config apply failed"
            } else {
                "peer mutation config apply failed and failure observation was not durable"
            };
            peer_mutation_response(500, serde_json::json!({"error": message}))
        }
    }
}

pub(super) fn resident_peer_mutation_handler(
    configured_path: PathBuf,
    ledger: ResidentLedgerWriter,
) -> mct_daemon::MctUdsControlMutationHandler {
    mct_daemon::MctUdsControlMutationHandler::new(move |path, body| {
        let configured_path = configured_path.clone();
        let ledger = ledger.clone();
        async move { execute_resident_peer_mutation(&configured_path, &ledger, &path, &body).await }
    })
}

pub(super) fn execute_offline_peer_mutation(
    configured_path: &Path,
    ledger_path: &Path,
    path: &str,
    body: &[u8],
) -> Result<PeerMutationSuccess> {
    let mut ledger = JsonlObservationLedger::open(ledger_path, "ledger-local", "local-mct")
        .with_context(|| {
            format!(
                "acquire exclusive observation ledger writer lock at {}",
                ledger_path.display()
            )
        })?;
    let prepared = prepare_peer_mutation(configured_path, path, body)?;
    ledger.append_batch_before_effect(
        [prepared.decision_observation()],
        mct_daemon::current_timestamp_string(),
    )?;
    match prepared.apply() {
        Ok(response) => Ok(response),
        Err(error) => {
            ledger.append_batch_before_effect(
                [prepared.failure_observation()],
                mct_daemon::current_timestamp_string(),
            )?;
            Err(error)
        }
    }
}

pub(super) async fn run_control(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected control subcommand: serve-http | serve-uds");
    }
    match args.remove(0).as_str() {
        "serve-http" => run_control_serve_http(args).await,
        "serve-uds" => run_control_serve_uds(args).await,
        other => bail!("unknown control subcommand '{other}'"),
    }
}

pub(super) async fn run_control_serve_http(mut args: Vec<String>) -> Result<()> {
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let addr = args
        .first()
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:9173".into());
    serve_http_control_loop(&state_path, &addr).await
}

#[cfg(unix)]
pub(super) async fn run_control_serve_uds(mut args: Vec<String>) -> Result<()> {
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let socket_path = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".mct/control.sock"));
    run_control_serve_uds_with_state(state_path, socket_path).await
}

#[cfg(unix)]
pub(super) async fn run_control_serve_uds_with_state(
    state_path: PathBuf,
    socket_path: PathBuf,
) -> Result<()> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)?;
    println!(
        "mct daemon serving control uds on {}",
        socket_path.display()
    );
    let snapshot_source = ControlSnapshotSource::open(&state_path);
    loop {
        mct_daemon::serve_uds_control_once_with_snapshot_result_and_blob_store(
            &listener,
            control_snapshot(&snapshot_source).await,
            Some(&state_path),
        )
        .await?;
    }
}

#[cfg(unix)]
pub(super) async fn run_control_serve_uds_with_state_until(
    state_path: PathBuf,
    socket_path: PathBuf,
    config_path: PathBuf,
    ledger: ResidentLedgerWriter,
    mut shutdown: broadcast::Receiver<()>,
    status_source: Option<Arc<ResidentStatusSource>>,
) -> Result<()> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)?;
    println!(
        "mct daemon serving control uds on {}",
        socket_path.display()
    );
    let snapshot_source = ControlSnapshotSource::open_with_status(&state_path, status_source);
    let mutation_handler = resident_peer_mutation_handler(config_path, ledger);
    loop {
        tokio::select! {
            _ = shutdown.recv() => break,
            result = mct_daemon::serve_uds_control_once_with_snapshot_result_blob_store_and_mutations(
                &listener,
                control_snapshot(&snapshot_source).await,
                Some(&state_path),
                Some(&mutation_handler),
            ) => result?,
        }
    }
    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}

#[cfg(not(unix))]
pub(super) async fn run_control_serve_uds(_args: Vec<String>) -> Result<()> {
    bail!("UDS control plane is only available on Unix platforms")
}

#[cfg(not(unix))]
pub(super) async fn run_control_serve_uds_with_state(
    _state_path: PathBuf,
    _socket_path: PathBuf,
) -> Result<()> {
    bail!("UDS control plane is only available on Unix platforms")
}

#[cfg(not(unix))]
pub(super) async fn run_control_serve_uds_with_state_until(
    _state_path: PathBuf,
    _socket_path: PathBuf,
    _config_path: PathBuf,
    _ledger: ResidentLedgerWriter,
    _shutdown: broadcast::Receiver<()>,
    _status_source: Option<Arc<ResidentStatusSource>>,
) -> Result<()> {
    bail!("UDS control plane is only available on Unix platforms")
}

#[derive(Clone)]
pub(super) enum ControlSnapshotSource {
    Store {
        state: Arc<Mutex<MctRuntimeStateStore>>,
        status_source: Option<Arc<ResidentStatusSource>>,
    },
    Unavailable,
}

impl ControlSnapshotSource {
    pub(super) fn open(state_path: &Path) -> Self {
        Self::open_with_status(state_path, None)
    }

    pub(super) fn open_with_status(
        state_path: &Path,
        status_source: Option<Arc<ResidentStatusSource>>,
    ) -> Self {
        match MctRuntimeStateStore::open(state_path)
            .with_context(|| format!("open control runtime state at {}", state_path.display()))
        {
            Ok(state) => Self::Store {
                state: Arc::new(Mutex::new(state)),
                status_source,
            },
            Err(_error) => Self::Unavailable,
        }
    }
}

pub(super) async fn control_snapshot(
    source: &ControlSnapshotSource,
) -> MctControlPlaneSnapshotResult {
    match source {
        ControlSnapshotSource::Unavailable => {
            Err(MctControlPlaneSnapshotError::runtime_state_unavailable())
        }
        ControlSnapshotSource::Store {
            state,
            status_source,
        } => {
            let state = Arc::clone(state);
            let status = resident_or_default_status(status_source.as_ref());
            tokio::task::spawn_blocking(move || {
                let state = state
                    .lock()
                    .map_err(|_| MctControlPlaneSnapshotError::runtime_state_unavailable())?;
                control_snapshot_from_state(&state, status)
                    .map_err(|_source| MctControlPlaneSnapshotError::runtime_state_unavailable())
            })
            .await
            .map_err(|_source| MctControlPlaneSnapshotError::runtime_state_unavailable())?
        }
    }
}

pub(super) fn resident_or_default_status(
    status_source: Option<&Arc<ResidentStatusSource>>,
) -> MctDaemonStatus {
    status_source.map_or_else(|| daemon_status(None), |source| source.status())
}

pub(super) fn control_snapshot_from_state(
    state: &MctRuntimeStateStore,
    status: MctDaemonStatus,
) -> Result<MctControlPlaneSnapshot> {
    let summary = state.summary()?;
    let runs = state.list_runs(20)?;
    Ok(MctControlPlaneSnapshot::new(status, Some(summary), runs))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use mct_observation::DurabilityClass;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn post_mutation(
        listener: Arc<UnixListener>,
        handler: mct_daemon::MctUdsControlMutationHandler,
        socket_path: &Path,
        path: &str,
        body: serde_json::Value,
    ) -> (u16, String) {
        let server = tokio::spawn(async move {
            mct_daemon::serve_uds_control_once_with_snapshot_result_blob_store_and_mutations(
                &listener,
                Err(MctControlPlaneSnapshotError::runtime_state_unavailable()),
                None,
                Some(&handler),
            )
            .await
            .unwrap();
        });
        let body = serde_json::to_vec(&body).unwrap();
        let mut client = tokio::net::UnixStream::connect(socket_path).await.unwrap();
        client
            .write_all(
                format!(
                    "POST {path} HTTP/1.1\r\nHost: local\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        client.write_all(&body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();
        let response = String::from_utf8(response).unwrap();
        let status = response.split_whitespace().nth(1).unwrap().parse().unwrap();
        let body = response.split_once("\r\n\r\n").unwrap().1.to_owned();
        (status, body)
    }

    fn add_request(config_path: &Path, proof: &str) -> serde_json::Value {
        serde_json::json!({
            "expected_config_path": config_path,
            "peer_node_id": "mother-b",
            "binding_id": "binding-a-admits-b",
            "endpoint_id": "endpoint-b",
            "vision_id": "vision-local",
            "ticket": null,
            "binding_signature_ref": proof,
            "policy_revision": 1
        })
    }

    #[tokio::test]
    async fn live_uds_peer_mutations_are_durable_and_secret_free() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let handler = resident_peer_mutation_handler(config_path.clone(), ledger.clone());

        let requests = [
            (
                "/peers/add",
                add_request(&config_path, "presented-secret-proof"),
            ),
            (
                "/peers/proof",
                serde_json::json!({
                    "expected_config_path": config_path,
                    "peer_node_id": "mother-b",
                    "binding_id": "binding-b-admits-a",
                    "policy_revision": 2,
                    "signature_ref": "outbound-secret-proof",
                    "expires_at": "2099-01-01T00:00:00Z"
                }),
            ),
            (
                "/peers/revoke",
                serde_json::json!({
                    "expected_config_path": config_path,
                    "peer_node_id": "mother-b"
                }),
            ),
            (
                "/peers/remove",
                serde_json::json!({
                    "expected_config_path": config_path,
                    "peer_node_id": "mother-b"
                }),
            ),
        ];
        for (path, body) in requests {
            let (status, response) = post_mutation(
                Arc::clone(&listener),
                handler.clone(),
                &socket_path,
                path,
                body,
            )
            .await;
            assert_eq!(status, 200, "{response}");
            assert!(!response.contains("secret-proof"));
        }
        let (oversized_status, _) = post_mutation(
            Arc::clone(&listener),
            handler.clone(),
            &socket_path,
            "/peers/add",
            serde_json::json!({"padding": "x".repeat(PEER_MUTATION_BODY_MAX_BYTES)}),
        )
        .await;
        assert_eq!(oversized_status, 400);
        drop(handler);
        ledger.close().await;

        assert!(
            MctDaemonConfigStore::new(&config_path)
                .load()
                .unwrap()
                .peers
                .is_empty()
        );
        let reader =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 4);
        assert!(entries.iter().all(|entry| {
            entry.durability_class == DurabilityClass::BeforeEffect
                && entry.observation.outcome == ObservationOutcome::Allowed
        }));
        assert_eq!(
            entries[0].observation.kind,
            ObservationKind::PeerBindingRecorded
        );
        assert_eq!(
            entries[1].observation.kind,
            ObservationKind::PeerBindingRecorded
        );
        assert_eq!(
            entries[2].observation.kind,
            ObservationKind::PeerBindingRevoked
        );
        assert_eq!(
            entries[3].observation.kind,
            ObservationKind::PeerBindingRevoked
        );
        let ledger_text = std::fs::read_to_string(ledger_path).unwrap();
        assert!(!ledger_text.contains("presented-secret-proof"));
        assert!(!ledger_text.contains("outbound-secret-proof"));
    }

    #[tokio::test]
    async fn resident_append_failure_prevents_peer_config_effect() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let socket_path = dir.path().join("control.sock");
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        drop(receiver);
        let handler =
            resident_peer_mutation_handler(config_path.clone(), ResidentLedgerWriter { sender });

        let (status, _) = post_mutation(
            listener,
            handler,
            &socket_path,
            "/peers/add",
            add_request(&config_path, "secret-proof"),
        )
        .await;

        assert_eq!(status, 500);
        assert!(!config_path.exists());
    }

    #[tokio::test]
    async fn resident_apply_failure_records_typed_failure_after_decision() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::create_dir(config_path.with_extension("json.tmp")).unwrap();
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let handler = resident_peer_mutation_handler(config_path.clone(), ledger.clone());

        let (status, response) = post_mutation(
            listener,
            handler.clone(),
            &socket_path,
            "/peers/add",
            add_request(&config_path, "secret-proof"),
        )
        .await;
        assert_eq!(status, 500, "{response}");
        drop(handler);
        ledger.close().await;

        let entries =
            JsonlObservationLedger::open_read_only(ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].observation.kind,
            ObservationKind::PeerBindingRecorded
        );
        assert_eq!(entries[0].observation.outcome, ObservationOutcome::Allowed);
        assert_eq!(
            entries[1].observation.kind,
            ObservationKind::OperatorActionRecorded
        );
        assert_eq!(entries[1].observation.outcome, ObservationOutcome::Failed);
        assert!(!config_path.exists());
    }

    #[test]
    fn offline_peer_mutation_observes_before_effect_and_fails_on_lock_contention() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let ledger_path = dir.path().join("observations.jsonl");
        let missing_socket = dir.path().join("missing.sock");

        run_peers_add(vec![
            "mother-b".into(),
            "binding-a-admits-b".into(),
            "endpoint-b".into(),
            "vision-local".into(),
            "--signature-ref".into(),
            "offline-secret-proof".into(),
            "--config".into(),
            config_path.display().to_string(),
            "--ledger".into(),
            ledger_path.display().to_string(),
            "--uds".into(),
            missing_socket.display().to_string(),
        ])
        .unwrap();
        assert_eq!(
            MctDaemonConfigStore::new(&config_path)
                .load()
                .unwrap()
                .peers
                .len(),
            1
        );
        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("peer_binding_recorded"));
        assert!(!ledger_text.contains("offline-secret-proof"));

        let locked_config = dir.path().join("locked-config.json");
        let _lock =
            JsonlObservationLedger::open(&ledger_path, "ledger-local", "local-mct").unwrap();
        let error = run_peers_add(vec![
            "mother-c".into(),
            "binding-a-admits-c".into(),
            "endpoint-c".into(),
            "vision-local".into(),
            "--signature-ref".into(),
            "secret-proof".into(),
            "--config".into(),
            locked_config.display().to_string(),
            "--ledger".into(),
            ledger_path.display().to_string(),
            "--uds".into(),
            missing_socket.display().to_string(),
        ])
        .unwrap_err();
        assert!(format!("{error:#}").contains("writer lock"));
        assert!(!locked_config.exists());
    }
}

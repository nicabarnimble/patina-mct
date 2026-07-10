use super::*;

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
    loop {
        tokio::select! {
            _ = shutdown.recv() => break,
            result = mct_daemon::serve_uds_control_once_with_snapshot_result_and_blob_store(
                &listener,
                control_snapshot(&snapshot_source).await,
                Some(&state_path),
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

use crate::{
    MCT_BLOB_MAX_BYTES, MctRuntimeRunRecord, MctRuntimeStateSummary,
    status::{MctDaemonHealth, MctDaemonReadiness, MctDaemonStatus, daemon_status},
};
use anyhow::{Context, Result, bail};
use mct_iroh::MotherIrohEndpointSnapshot;
use serde::{Deserialize, Serialize};
use std::{future::Future, path::Path, pin::Pin, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctDaemonLocalControlFacts {
    pub iroh_endpoint: Option<MotherIrohEndpointSnapshot>,
}

impl MctDaemonLocalControlFacts {
    pub fn new(iroh_endpoint: Option<MotherIrohEndpointSnapshot>) -> Self {
        Self { iroh_endpoint }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MctDaemonLocalControlRequest {
    Status,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MctDaemonLocalControlResponse {
    Status(MctDaemonStatus),
}

pub fn handle_local_control_request(
    request: MctDaemonLocalControlRequest,
    facts: MctDaemonLocalControlFacts,
) -> MctDaemonLocalControlResponse {
    match request {
        MctDaemonLocalControlRequest::Status => {
            MctDaemonLocalControlResponse::Status(daemon_status(facts.iroh_endpoint))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctControlPlaneSnapshot {
    pub status: MctDaemonStatus,
    pub state: Option<MctRuntimeStateSummary>,
    pub runs: Vec<MctRuntimeRunRecord>,
}

pub type MctControlPlaneSnapshotResult =
    std::result::Result<MctControlPlaneSnapshot, MctControlPlaneSnapshotError>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MctControlPlaneSnapshotError {
    RuntimeStateUnavailable { safe_message: String },
}

impl MctControlPlaneSnapshotError {
    pub fn runtime_state_unavailable() -> Self {
        Self::RuntimeStateUnavailable {
            safe_message: "runtime state unavailable".into(),
        }
    }

    fn safe_message(&self) -> &str {
        match self {
            Self::RuntimeStateUnavailable { safe_message } => safe_message,
        }
    }

    fn status(&self) -> MctDaemonStatus {
        MctDaemonStatus {
            version: crate::version().into(),
            health: MctDaemonHealth::Unhealthy,
            readiness: MctDaemonReadiness::NotReady,
            iroh_endpoint: None,
            resident: None,
            safe_message: self.safe_message().into(),
        }
    }
}

impl MctControlPlaneSnapshot {
    pub fn new(
        status: MctDaemonStatus,
        state: Option<MctRuntimeStateSummary>,
        runs: Vec<MctRuntimeRunRecord>,
    ) -> Self {
        Self {
            status,
            state,
            runs,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctControlPlaneResponse {
    pub status_code: u16,
    pub content_type: String,
    pub body: String,
}

const MCT_UDS_CONTROL_HEADER_READ_BUDGET_BYTES: usize = 4096;
const MCT_UDS_CONTROL_READ_BUDGET_BYTES: usize =
    MCT_BLOB_MAX_BYTES.div_ceil(3) * 4 + MCT_UDS_CONTROL_HEADER_READ_BUDGET_BYTES;

#[cfg(unix)]
type MctUdsControlMutationFuture =
    Pin<Box<dyn Future<Output = MctControlPlaneResponse> + Send + 'static>>;

#[cfg(unix)]
type MctUdsControlMutationCallback = dyn Fn(Option<MctUdsPeerCredentials>, String, Vec<u8>) -> MctUdsControlMutationFuture
    + Send
    + Sync
    + 'static;

/// Local-only extension point for binary-owned UDS mutation handlers.
#[cfg(unix)]
#[derive(Clone)]
pub struct MctUdsControlMutationHandler {
    callback: Arc<MctUdsControlMutationCallback>,
}

#[cfg(unix)]
impl MctUdsControlMutationHandler {
    pub fn new<F, Fut>(callback: F) -> Self
    where
        F: Fn(String, Vec<u8>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = MctControlPlaneResponse> + Send + 'static,
    {
        Self {
            callback: Arc::new(move |_peer, path, body| Box::pin(callback(path, body))),
        }
    }

    pub fn new_authenticated<F, Fut>(callback: F) -> Self
    where
        F: Fn(Option<MctUdsPeerCredentials>, String, Vec<u8>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = MctControlPlaneResponse> + Send + 'static,
    {
        Self {
            callback: Arc::new(move |peer, path, body| Box::pin(callback(peer, path, body))),
        }
    }

    async fn handle(
        &self,
        peer: Option<MctUdsPeerCredentials>,
        path: String,
        body: Vec<u8>,
    ) -> MctControlPlaneResponse {
        (self.callback)(peer, path, body).await
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MctUdsPeerCredentials {
    pub uid: u32,
    pub gid: u32,
    pub pid: Option<i32>,
}

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MctUdsControlCallPreflight {
    Authenticated(MctUdsPeerCredentials),
    Refused(Option<MctControlPlaneResponse>),
}

#[cfg(unix)]
type MctUdsControlCallPreflightFuture =
    Pin<Box<dyn Future<Output = MctUdsControlCallPreflight> + Send + 'static>>;

#[cfg(unix)]
type MctUdsControlCallPreflightCallback = dyn Fn(Option<MctUdsPeerCredentials>, usize) -> MctUdsControlCallPreflightFuture
    + Send
    + Sync
    + 'static;

#[cfg(unix)]
type MctUdsControlCallFuture =
    Pin<Box<dyn Future<Output = Option<MctControlPlaneResponse>> + Send + 'static>>;

#[cfg(unix)]
type MctUdsControlCallCallback =
    dyn Fn(MctUdsPeerCredentials, Vec<u8>) -> MctUdsControlCallFuture + Send + Sync + 'static;

/// Authenticated local application-call handler owned by the resident binary.
#[cfg(unix)]
#[derive(Clone)]
pub struct MctUdsControlCallHandler {
    preflight_callback: Arc<MctUdsControlCallPreflightCallback>,
    callback: Arc<MctUdsControlCallCallback>,
}

#[cfg(unix)]
impl MctUdsControlCallHandler {
    pub fn new<P, PFut, F, Fut>(preflight: P, callback: F) -> Self
    where
        P: Fn(Option<MctUdsPeerCredentials>, usize) -> PFut + Send + Sync + 'static,
        PFut: Future<Output = MctUdsControlCallPreflight> + Send + 'static,
        F: Fn(MctUdsPeerCredentials, Vec<u8>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<MctControlPlaneResponse>> + Send + 'static,
    {
        Self {
            preflight_callback: Arc::new(move |peer, declared_body_len| {
                Box::pin(preflight(peer, declared_body_len))
            }),
            callback: Arc::new(move |peer, body| Box::pin(callback(peer, body))),
        }
    }

    async fn preflight(
        &self,
        peer: Option<MctUdsPeerCredentials>,
        declared_body_len: usize,
    ) -> MctUdsControlCallPreflight {
        (self.preflight_callback)(peer, declared_body_len).await
    }

    async fn handle(
        &self,
        peer: MctUdsPeerCredentials,
        body: Vec<u8>,
    ) -> Option<MctControlPlaneResponse> {
        (self.callback)(peer, body).await
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MctControlPlaneAuthPolicy {
    required_bearer_token: Option<String>,
}

impl MctControlPlaneAuthPolicy {
    pub fn open_local() -> Self {
        Self::default()
    }

    pub fn require_bearer_token(token: impl Into<String>) -> Result<Self> {
        let token = token.into();
        if token.trim().is_empty() {
            bail!("control-plane bearer token must not be blank");
        }
        Ok(Self {
            required_bearer_token: Some(token),
        })
    }

    fn authorize(&self, authorization_header: Option<&str>) -> MctControlPlaneAuthDecision {
        let Some(required) = self.required_bearer_token.as_ref() else {
            return MctControlPlaneAuthDecision::Allowed;
        };
        let Some(header) = authorization_header else {
            return MctControlPlaneAuthDecision::MissingCredential;
        };
        let Some(token) = header.trim().strip_prefix("Bearer ") else {
            return MctControlPlaneAuthDecision::InvalidCredential;
        };
        if token == required {
            MctControlPlaneAuthDecision::Allowed
        } else {
            MctControlPlaneAuthDecision::InvalidCredential
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MctControlPlaneAuthDecision {
    Allowed,
    MissingCredential,
    InvalidCredential,
}

pub fn handle_control_plane_path(
    method: &str,
    path: &str,
    snapshot: &MctControlPlaneSnapshot,
) -> MctControlPlaneResponse {
    handle_control_plane_path_with_auth(
        method,
        path,
        snapshot,
        &MctControlPlaneAuthPolicy::open_local(),
        None,
    )
}

pub fn handle_control_plane_path_with_auth(
    method: &str,
    path: &str,
    snapshot: &MctControlPlaneSnapshot,
    policy: &MctControlPlaneAuthPolicy,
    authorization_header: Option<&str>,
) -> MctControlPlaneResponse {
    handle_control_plane_path_result_with_auth(
        method,
        path,
        Ok(snapshot),
        policy,
        authorization_header,
    )
}

pub fn handle_control_plane_path_result_with_auth(
    method: &str,
    path: &str,
    snapshot: std::result::Result<&MctControlPlaneSnapshot, &MctControlPlaneSnapshotError>,
    policy: &MctControlPlaneAuthPolicy,
    authorization_header: Option<&str>,
) -> MctControlPlaneResponse {
    match policy.authorize(authorization_header) {
        MctControlPlaneAuthDecision::Allowed => {}
        MctControlPlaneAuthDecision::MissingCredential => {
            return json_response(401, serde_json::json!({"error": "missing credential"}));
        }
        MctControlPlaneAuthDecision::InvalidCredential => {
            return json_response(403, serde_json::json!({"error": "invalid credential"}));
        }
    }
    if method != "GET" {
        return json_response(405, serde_json::json!({"error": "method not allowed"}));
    }
    let snapshot = match snapshot {
        Ok(snapshot) => snapshot,
        Err(error) => return snapshot_error_response(error),
    };
    match path {
        "/" | "/status" => json_response(200, &snapshot.status),
        "/state" => json_response(200, &snapshot.state),
        "/runs" => json_response(200, &snapshot.runs),
        "/snapshot" => json_response(200, snapshot),
        _ => json_response(404, serde_json::json!({"error": "not found"})),
    }
}

pub async fn serve_http_control_once(
    listener: &TcpListener,
    snapshot: MctControlPlaneSnapshot,
) -> Result<()> {
    serve_http_control_once_with_auth(listener, snapshot, MctControlPlaneAuthPolicy::open_local())
        .await
}

pub async fn serve_http_control_once_with_snapshot_result(
    listener: &TcpListener,
    snapshot: MctControlPlaneSnapshotResult,
) -> Result<()> {
    let (mut stream, _) = listener.accept().await.context("accept http control")?;
    let mut buffer = [0_u8; 4096];
    let read = stream
        .read(&mut buffer)
        .await
        .context("read http control request")?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let (method, path) = parse_http_request_line(&request)?;
    let authorization_header = parse_authorization_header(&request);
    let response = handle_control_plane_path_result_with_auth(
        method,
        path,
        snapshot.as_ref(),
        &MctControlPlaneAuthPolicy::open_local(),
        authorization_header,
    );
    stream
        .write_all(http_response_bytes(&response).as_bytes())
        .await
        .context("write http control response")?;
    Ok(())
}

pub async fn serve_http_control_once_with_auth(
    listener: &TcpListener,
    snapshot: MctControlPlaneSnapshot,
    policy: MctControlPlaneAuthPolicy,
) -> Result<()> {
    let (mut stream, _) = listener.accept().await.context("accept http control")?;
    let mut buffer = [0_u8; 4096];
    let read = stream
        .read(&mut buffer)
        .await
        .context("read http control request")?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let (method, path) = parse_http_request_line(&request)?;
    let authorization_header = parse_authorization_header(&request);
    let response =
        handle_control_plane_path_with_auth(method, path, &snapshot, &policy, authorization_header);
    stream
        .write_all(http_response_bytes(&response).as_bytes())
        .await
        .context("write http control response")?;
    Ok(())
}

#[cfg(unix)]
pub async fn serve_uds_control_once(
    listener: &UnixListener,
    snapshot: MctControlPlaneSnapshot,
) -> Result<()> {
    serve_uds_control_once_with_auth(listener, snapshot, MctControlPlaneAuthPolicy::open_local())
        .await
}

#[cfg(unix)]
pub async fn serve_uds_control_once_with_snapshot_result(
    listener: &UnixListener,
    snapshot: MctControlPlaneSnapshotResult,
) -> Result<()> {
    serve_uds_control_once_with_snapshot_result_and_blob_store(listener, snapshot, None).await
}

#[cfg(unix)]
pub async fn serve_uds_control_once_with_snapshot_result_and_blob_store(
    listener: &UnixListener,
    snapshot: MctControlPlaneSnapshotResult,
    blob_state_path: Option<&Path>,
) -> Result<()> {
    serve_uds_control_once_with_snapshot_result_blob_store_and_mutations(
        listener,
        snapshot,
        blob_state_path,
        None,
    )
    .await
}

#[cfg(unix)]
pub async fn serve_uds_control_once_with_snapshot_result_blob_store_and_mutations(
    listener: &UnixListener,
    snapshot: MctControlPlaneSnapshotResult,
    blob_state_path: Option<&Path>,
    mutation_handler: Option<&MctUdsControlMutationHandler>,
) -> Result<()> {
    serve_uds_control_once_with_handlers(
        listener,
        snapshot,
        blob_state_path,
        mutation_handler,
        None,
    )
    .await
}

#[cfg(unix)]
pub async fn serve_uds_control_once_with_handlers(
    listener: &UnixListener,
    snapshot: MctControlPlaneSnapshotResult,
    _blob_state_path: Option<&Path>,
    mutation_handler: Option<&MctUdsControlMutationHandler>,
    call_handler: Option<&MctUdsControlCallHandler>,
) -> Result<()> {
    let (stream, _) = listener.accept().await.context("accept uds control")?;
    serve_uds_control_stream_with_handlers(stream, snapshot, mutation_handler, call_handler).await
}

#[cfg(unix)]
pub async fn serve_uds_control_stream_with_handlers(
    mut stream: UnixStream,
    snapshot: MctControlPlaneSnapshotResult,
    mutation_handler: Option<&MctUdsControlMutationHandler>,
    call_handler: Option<&MctUdsControlCallHandler>,
) -> Result<()> {
    let peer = stream
        .peer_cred()
        .ok()
        .map(|credentials| MctUdsPeerCredentials {
            uid: credentials.uid(),
            gid: credentials.gid(),
            pid: credentials.pid(),
        });
    let headers = read_http_headers_bounded(&mut stream, MCT_UDS_CONTROL_HEADER_READ_BUDGET_BYTES)
        .await
        .context("read uds control request headers")?;
    let request = String::from_utf8_lossy(&headers);
    let (method, path) = parse_http_request_line(&request)?;
    let authorization_header = parse_authorization_header(&request);
    let content_length = parse_content_length(&request)?.unwrap_or(0);
    let response = if method == "POST" && path == "/calls" {
        match call_handler {
            Some(handler) => match handler.preflight(peer, content_length).await {
                MctUdsControlCallPreflight::Authenticated(peer) => {
                    let body = read_http_body_bounded(
                        &mut stream,
                        content_length,
                        mct_iroh::MCT_CALL_FRAME_READ_BUDGET_BYTES,
                    )
                    .await
                    .context("read bounded uds call body")?;
                    handler.handle(peer, body).await
                }
                MctUdsControlCallPreflight::Refused(response) => response,
            },
            None => Some(json_response(
                405,
                serde_json::json!({"error": "method not allowed"}),
            )),
        }
    } else {
        let body_budget = MCT_UDS_CONTROL_READ_BUDGET_BYTES
            .checked_sub(headers.len())
            .context("control request headers exceed bounded read budget")?;
        let body = read_http_body_bounded(&mut stream, content_length, body_budget)
            .await
            .context("read uds control request body")?;
        if method == "POST"
            && let Some(handler) = mutation_handler
        {
            Some(handler.handle(peer, path.to_owned(), body).await)
        } else {
            Some(handle_control_plane_path_result_with_auth(
                method,
                path,
                snapshot.as_ref(),
                &MctControlPlaneAuthPolicy::open_local(),
                authorization_header,
            ))
        }
    };
    if let Some(response) = response {
        stream
            .write_all(http_response_bytes(&response).as_bytes())
            .await
            .context("write uds control response")?;
    }
    Ok(())
}

pub async fn serve_uds_control_once_with_auth(
    listener: &UnixListener,
    snapshot: MctControlPlaneSnapshot,
    policy: MctControlPlaneAuthPolicy,
) -> Result<()> {
    let (mut stream, _) = listener.accept().await.context("accept uds control")?;
    let mut buffer = [0_u8; 4096];
    let read = stream
        .read(&mut buffer)
        .await
        .context("read uds control request")?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let (method, path) = parse_http_request_line(&request)?;
    let authorization_header = parse_authorization_header(&request);
    let response =
        handle_control_plane_path_with_auth(method, path, &snapshot, &policy, authorization_header);
    stream
        .write_all(http_response_bytes(&response).as_bytes())
        .await
        .context("write uds control response")?;
    Ok(())
}

fn parse_http_request_line(request: &str) -> Result<(&str, &str)> {
    let Some(line) = request.lines().next() else {
        bail!("empty control request");
    };
    let mut parts = line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing method"))?;
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing path"))?;
    Ok((method, path))
}

fn parse_authorization_header(request: &str) -> Option<&str> {
    request.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("authorization")
            .then(|| value.trim())
    })
}

fn parse_content_length(request: &str) -> Result<Option<usize>> {
    request
        .lines()
        .skip(1)
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>())
        })
        .transpose()
        .context("parse content-length")
}

async fn read_http_headers_bounded<S>(stream: &mut S, budget: usize) -> Result<Vec<u8>>
where
    S: tokio::io::AsyncRead + Unpin,
{
    let mut headers = Vec::new();
    let mut byte = [0_u8; 1];
    while !headers.ends_with(b"\r\n\r\n") {
        if headers.len() == budget {
            bail!("control request headers exceed bounded read budget");
        }
        let read = stream
            .read(&mut byte)
            .await
            .context("read control request header byte")?;
        if read == 0 {
            bail!("control request ended before header terminator");
        }
        headers.push(byte[0]);
    }
    Ok(headers)
}

async fn read_http_body_bounded<S>(
    stream: &mut S,
    content_length: usize,
    budget: usize,
) -> Result<Vec<u8>>
where
    S: tokio::io::AsyncRead + Unpin,
{
    if content_length > budget {
        bail!("control request body exceeds bounded read budget");
    }
    let mut body = vec![0_u8; content_length];
    stream
        .read_exact(&mut body)
        .await
        .context("read complete control request body")?;
    Ok(body)
}

fn http_response_bytes(response: &MctControlPlaneResponse) -> String {
    let reason = match response.status_code {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        503 => "Service Unavailable",
        _ => "OK",
    };
    format!(
        "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        response.status_code,
        reason,
        response.content_type,
        response.body.len(),
        response.body
    )
}

fn snapshot_error_response(error: &MctControlPlaneSnapshotError) -> MctControlPlaneResponse {
    json_response(
        503,
        serde_json::json!({
            "error": error.safe_message(),
            "status": error.status(),
        }),
    )
}

fn json_response<T: Serialize>(status_code: u16, value: T) -> MctControlPlaneResponse {
    MctControlPlaneResponse {
        status_code,
        content_type: "application/json".into(),
        body: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "null".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MctDaemonHealth, MctDaemonReadiness, local_blob_store_for_state_path};
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};

    fn snapshot() -> MctControlPlaneSnapshot {
        MctControlPlaneSnapshot::new(
            MctDaemonStatus {
                version: "0.1.0".into(),
                health: MctDaemonHealth::Healthy,
                readiness: MctDaemonReadiness::Ready,
                iroh_endpoint: None,
                resident: None,
                safe_message: "ready".into(),
            },
            Some(MctRuntimeStateSummary {
                schema_version: 1,
                artifacts: 1,
                approved_children: 1,
                active_assignments: 1,
                ready_instances: 1,
                peers: 1,
                runs: 1,
                completed_runs: 1,
                failed_runs: 0,
                metric_points: 1,
                queued_tasks: 0,
                child_state_keys: 0,
                child_subscriptions: 0,
                toy_catalog_contracts: 0,
                toy_grant_snapshots: 0,
                trigger_records: 0,
                current_trigger_records: 0,
                pending_trigger_occurrences: 0,
                active_trigger_firings: 0,
                watch_scope_records: 0,
                current_watch_scopes: 0,
                watch_event_batches: 0,
                watch_event_deliveries: 0,
            }),
            Vec::new(),
        )
    }

    #[test]
    fn control_plane_routes_status_state_runs_and_not_found() {
        assert_eq!(
            handle_control_plane_path("GET", "/status", &snapshot()).status_code,
            200
        );
        assert!(
            handle_control_plane_path("GET", "/state", &snapshot())
                .body
                .contains("artifacts")
        );
        assert_eq!(
            handle_control_plane_path("GET", "/runs", &snapshot()).status_code,
            200
        );
        assert_eq!(
            handle_control_plane_path("GET", "/missing", &snapshot()).status_code,
            404
        );
        assert_eq!(
            handle_control_plane_path("POST", "/status", &snapshot()).status_code,
            405
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn uds_blob_ingest_requires_an_observing_mutation_owner() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let socket_path = dir.path().join("control.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let bytes = b"control blob bytes";
        let digest = blake3::hash(bytes).to_hex().to_string();
        let body = serde_json::json!({
            "digest": digest,
            "size_bytes": bytes.len(),
            "content_type": "application/octet-stream",
            "bytes_base64": BASE64_STANDARD.encode(bytes),
        })
        .to_string();
        let server = tokio::spawn(async move {
            serve_uds_control_once_with_snapshot_result_blob_store_and_mutations(
                &listener,
                Ok(snapshot()),
                Some(&state_path),
                None,
            )
            .await
            .unwrap();
        });
        let mut client = tokio::net::UnixStream::connect(&socket_path).await.unwrap();
        client
            .write_all(
                format!(
                    "POST /blobs HTTP/1.1\r\nHost: local\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        client.shutdown().await.unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).await.unwrap();
        server.await.unwrap();

        assert!(response.starts_with("HTTP/1.1 405"), "{response}");
        assert!(
            !local_blob_store_for_state_path(dir.path().join("state.sqlite"))
                .visible_path(&digest)
                .unwrap()
                .exists()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn uds_local_mutation_handler_receives_post_body() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("control.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let called = Arc::new(AtomicBool::new(false));
        let callback_called = Arc::clone(&called);
        let handler = MctUdsControlMutationHandler::new(move |path, body| {
            let callback_called = Arc::clone(&callback_called);
            async move {
                callback_called.store(true, Ordering::SeqCst);
                assert_eq!(path, "/peers/revoke");
                assert_eq!(body, br#"{"peer":"mother-b"}"#);
                MctControlPlaneResponse {
                    status_code: 200,
                    content_type: "application/json".into(),
                    body: r#"{"status":"ok"}"#.into(),
                }
            }
        });
        let server = tokio::spawn(async move {
            serve_uds_control_once_with_snapshot_result_blob_store_and_mutations(
                &listener,
                Ok(snapshot()),
                None,
                Some(&handler),
            )
            .await
            .unwrap();
        });
        let mut client = tokio::net::UnixStream::connect(socket_path).await.unwrap();
        let body = r#"{"peer":"mother-b"}"#;
        client
            .write_all(
                format!(
                    "POST /peers/revoke HTTP/1.1\r\nHost: local\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        assert!(called.load(Ordering::SeqCst));
        assert!(
            String::from_utf8(response)
                .unwrap()
                .starts_with("HTTP/1.1 200")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn uds_authenticated_mutation_handler_receives_peer_credentials() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("control.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let expected_uid = String::from_utf8(
            std::process::Command::new("/usr/bin/id")
                .arg("-u")
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
        let handler =
            MctUdsControlMutationHandler::new_authenticated(move |peer, path, body| async move {
                let peer = peer.expect("Unix peer credentials must be available");
                assert_eq!(path, "/lifecycle/fact");
                assert_eq!(body, br#"{"action":"stop_prepare"}"#);
                assert_eq!(peer.uid, expected_uid);
                MctControlPlaneResponse {
                    status_code: 200,
                    content_type: "application/json".into(),
                    body: r#"{"status":"ok"}"#.into(),
                }
            });
        let server = tokio::spawn(async move {
            serve_uds_control_once_with_snapshot_result_blob_store_and_mutations(
                &listener,
                Ok(snapshot()),
                None,
                Some(&handler),
            )
            .await
            .unwrap();
        });
        let mut client = tokio::net::UnixStream::connect(socket_path).await.unwrap();
        let body = r#"{"action":"stop_prepare"}"#;
        client
            .write_all(
                format!(
                    "POST /lifecycle/fact HTTP/1.1\r\nHost: local\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();
        assert!(
            String::from_utf8(response)
                .unwrap()
                .starts_with("HTTP/1.1 200")
        );
    }

    #[test]
    fn control_plane_auth_policy_fails_closed_when_token_required() {
        let policy = MctControlPlaneAuthPolicy::require_bearer_token("secret").unwrap();

        let missing =
            handle_control_plane_path_with_auth("GET", "/status", &snapshot(), &policy, None);
        assert_eq!(missing.status_code, 401);
        assert!(!missing.body.contains("ready"));

        let wrong = handle_control_plane_path_with_auth(
            "GET",
            "/status",
            &snapshot(),
            &policy,
            Some("Bearer wrong"),
        );
        assert_eq!(wrong.status_code, 403);
        assert!(!wrong.body.contains("ready"));

        let allowed = handle_control_plane_path_with_auth(
            "GET",
            "/status",
            &snapshot(),
            &policy,
            Some("Bearer secret"),
        );
        assert_eq!(allowed.status_code, 200);
        assert!(allowed.body.contains("ready"));
    }

    #[test]
    fn control_plane_open_policy_preserves_existing_routes() {
        let policy = MctControlPlaneAuthPolicy::open_local();

        let status =
            handle_control_plane_path_with_auth("GET", "/status", &snapshot(), &policy, None);
        let missing =
            handle_control_plane_path_with_auth("GET", "/missing", &snapshot(), &policy, None);
        let method =
            handle_control_plane_path_with_auth("POST", "/status", &snapshot(), &policy, None);

        assert_eq!(status.status_code, 200);
        assert_eq!(missing.status_code, 404);
        assert_eq!(method.status_code, 405);
    }

    #[test]
    fn control_plane_authorization_header_is_case_insensitive() {
        let request =
            "GET /status HTTP/1.1\r\nhost: localhost\r\nAUTHORIZATION: Bearer secret\r\n\r\n";
        assert_eq!(parse_authorization_header(request), Some("Bearer secret"));
    }
}

use crate::{
    MCT_BLOB_MAX_BYTES, MctRuntimeRunRecord, MctRuntimeStateSummary,
    local_blob_store_for_state_path,
    status::{MctDaemonHealth, MctDaemonReadiness, MctDaemonStatus, daemon_status},
};
use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use mct_iroh::MotherIrohEndpointSnapshot;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[cfg(unix)]
use tokio::net::UnixListener;

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

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
struct MctBlobIngestRequest {
    digest: String,
    size_bytes: u64,
    content_type: String,
    bytes_base64: String,
}

const MCT_UDS_CONTROL_READ_BUDGET_BYTES: usize = MCT_BLOB_MAX_BYTES.div_ceil(3) * 4 + 4096;

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
    let (mut stream, _) = listener.accept().await.context("accept uds control")?;
    let request_bytes = read_http_request_bounded(&mut stream, MCT_UDS_CONTROL_READ_BUDGET_BYTES)
        .await
        .context("read uds control request")?;
    let request = String::from_utf8_lossy(&request_bytes);
    let (method, path) = parse_http_request_line(&request)?;
    let authorization_header = parse_authorization_header(&request);
    let response = if method == "POST" && path == "/blobs" {
        match blob_state_path {
            Some(state_path) => handle_blob_ingest_request(&request_bytes, state_path),
            None => json_response(404, serde_json::json!({"error": "not found"})),
        }
    } else {
        handle_control_plane_path_result_with_auth(
            method,
            path,
            snapshot.as_ref(),
            &MctControlPlaneAuthPolicy::open_local(),
            authorization_header,
        )
    };
    stream
        .write_all(http_response_bytes(&response).as_bytes())
        .await
        .context("write uds control response")?;
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

async fn read_http_request_bounded<S>(stream: &mut S, budget: usize) -> Result<Vec<u8>>
where
    S: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = stream
            .read(&mut buffer)
            .await
            .context("read control request chunk")?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > budget {
            bail!("control request exceeds bounded read budget");
        }
        if let Some((headers_len, content_length)) = request_frame_shape(&bytes)?
            && bytes.len() >= headers_len + content_length
        {
            break;
        }
    }
    if bytes.is_empty() {
        bail!("empty control request");
    }
    Ok(bytes)
}

fn request_frame_shape(bytes: &[u8]) -> Result<Option<(usize, usize)>> {
    let Some(headers_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
        return Ok(None);
    };
    let headers_len = headers_end + 4;
    let headers = String::from_utf8_lossy(&bytes[..headers_len]);
    let content_length = parse_content_length(&headers)?.unwrap_or(0);
    Ok(Some((headers_len, content_length)))
}

fn request_body(bytes: &[u8]) -> Result<&[u8]> {
    let Some(headers_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
        bail!("missing HTTP request body separator");
    };
    Ok(&bytes[headers_end + 4..])
}

fn handle_blob_ingest_request(request_bytes: &[u8], state_path: &Path) -> MctControlPlaneResponse {
    let body = match request_body(request_bytes) {
        Ok(body) => body,
        Err(error) => return json_response(400, serde_json::json!({"error": error.to_string()})),
    };
    let request = match serde_json::from_slice::<MctBlobIngestRequest>(body) {
        Ok(request) => request,
        Err(error) => return json_response(400, serde_json::json!({"error": error.to_string()})),
    };
    if request.size_bytes > MCT_BLOB_MAX_BYTES as u64 {
        return json_response(413, serde_json::json!({"error": "blob too large"}));
    }
    let bytes = match BASE64_STANDARD.decode(request.bytes_base64.as_bytes()) {
        Ok(bytes) => bytes,
        Err(error) => return json_response(400, serde_json::json!({"error": error.to_string()})),
    };
    let store = local_blob_store_for_state_path(state_path);
    match store.ingest_reader(
        &request.digest,
        request.size_bytes,
        &request.content_type,
        std::io::Cursor::new(bytes),
    ) {
        Ok(handle) => json_response(201, serde_json::json!({"payload": handle})),
        Err(error) => json_response(400, serde_json::json!({"error": error.safe_message()})),
    }
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
    use crate::{MctDaemonHealth, MctDaemonReadiness};

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

    #[test]
    fn uds_blob_ingest_request_writes_visible_blob() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let bytes = b"control blob bytes";
        let digest = blake3::hash(bytes).to_hex().to_string();
        let body = serde_json::json!({
            "digest": digest,
            "size_bytes": bytes.len(),
            "content_type": "application/octet-stream",
            "bytes_base64": BASE64_STANDARD.encode(bytes),
        })
        .to_string();
        let request = format!(
            "POST /blobs HTTP/1.1\r\nHost: local\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        let response = handle_blob_ingest_request(request.as_bytes(), &state_path);
        assert_eq!(response.status_code, 201, "{}", response.body);
        assert!(
            local_blob_store_for_state_path(&state_path)
                .visible_path(&digest)
                .unwrap()
                .exists()
        );
        assert!(!response.body.contains("control blob bytes"));
        assert!(!response.body.contains(&BASE64_STANDARD.encode(bytes)));
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

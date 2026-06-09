use crate::{
    MctRuntimeRunRecord, MctRuntimeStateSummary,
    status::{MctDaemonStatus, daemon_status},
};
use anyhow::{Context, Result, bail};
use mct_iroh::MotherIrohEndpointSnapshot;
use serde::{Deserialize, Serialize};
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

pub fn handle_control_plane_path(
    method: &str,
    path: &str,
    snapshot: &MctControlPlaneSnapshot,
) -> MctControlPlaneResponse {
    if method != "GET" {
        return json_response(405, serde_json::json!({"error": "method not allowed"}));
    }
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
    let (mut stream, _) = listener.accept().await.context("accept http control")?;
    let mut buffer = [0_u8; 4096];
    let read = stream
        .read(&mut buffer)
        .await
        .context("read http control request")?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let (method, path) = parse_http_request_line(&request)?;
    let response = handle_control_plane_path(method, path, &snapshot);
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
    let (mut stream, _) = listener.accept().await.context("accept uds control")?;
    let mut buffer = [0_u8; 4096];
    let read = stream
        .read(&mut buffer)
        .await
        .context("read uds control request")?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let (method, path) = parse_http_request_line(&request)?;
    let response = handle_control_plane_path(method, path, &snapshot);
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

fn http_response_bytes(response: &MctControlPlaneResponse) -> String {
    let reason = match response.status_code {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
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
}

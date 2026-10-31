//! Transport limits are not authority, execution deadlines, or durability policy.
use anyhow::{Context, Result, bail};
use std::{future::Future, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const CONTROL_HEADER_BYTES: usize = 4096;
pub const CONTROL_BODY_BYTES: usize = 1024 * 1024;
pub const CONTROL_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub const CONTROL_IO_TIMEOUT: Duration = Duration::from_secs(3);
pub const CONTROL_CONNECTION_LIMIT: usize = 64;

#[cfg(unix)]
mod client;
#[cfg(unix)]
pub use client::control_socket_request;

#[derive(Debug)]
pub struct ControlRequestHead {
    pub method: String,
    pub path: String,
    pub authorization: Option<String>,
    pub content_length: usize,
}

pub async fn read_headers<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut byte = [0; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() == CONTROL_HEADER_BYTES {
            bail!("control headers exceed byte cap");
        }
        if stream.read(&mut byte).await? == 0 {
            bail!("incomplete control headers");
        }
        bytes.push(byte[0]);
    }
    Ok(bytes)
}

/// Strict framing: no duplicate lengths/credentials, chunking, or folded headers.
pub fn header_fields(bytes: &[u8]) -> Result<(&str, Option<usize>, Option<&str>)> {
    if bytes.len() > CONTROL_HEADER_BYTES || !bytes.ends_with(b"\r\n\r\n") {
        bail!("invalid control header envelope");
    }
    let text = std::str::from_utf8(bytes).context("control headers are not UTF-8")?;
    let mut lines = text[..text.len() - 4].split("\r\n");
    let first = lines.next().context("missing control start line")?;
    if first.bytes().any(|b| b.is_ascii_control()) {
        bail!("invalid control start line");
    }
    let mut length = None;
    let mut authorization = None;
    for line in lines {
        let (name, value) = line.split_once(':').context("malformed control header")?;
        if name.is_empty()
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || value.bytes().any(|b| b.is_ascii_control() && b != b'\t')
        {
            bail!("invalid control header");
        }
        let value = value.trim();
        if name.eq_ignore_ascii_case("transfer-encoding") {
            bail!("control transfer encoding unsupported");
        }
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                bail!("ambiguous control content length");
            }
            length = Some(value.parse().context("invalid control content length")?);
        }
        if name.eq_ignore_ascii_case("authorization") && authorization.replace(value).is_some() {
            bail!("duplicate control authorization");
        }
    }
    Ok((first, length, authorization))
}

pub fn parse_request_head(bytes: &[u8]) -> Result<ControlRequestHead> {
    let (first, length, authorization) = header_fields(bytes)?;
    let parts: Vec<_> = first.split_ascii_whitespace().collect();
    if parts.len() != 3
        || !matches!(parts[2], "HTTP/1.0" | "HTTP/1.1")
        || !parts[1].starts_with('/')
        || !parts[0].bytes().all(|b| b.is_ascii_uppercase())
    {
        bail!("invalid control request line");
    }
    Ok(ControlRequestHead {
        method: parts[0].into(),
        path: parts[1].into(),
        authorization: authorization.map(str::to_owned),
        content_length: length.unwrap_or(0),
    })
}

pub async fn read_request_head<S: AsyncRead + Unpin>(stream: &mut S) -> Result<ControlRequestHead> {
    let bytes = tokio::time::timeout(CONTROL_IO_TIMEOUT, read_headers(stream))
        .await
        .context("control header deadline elapsed")??;
    parse_request_head(&bytes)
}

pub async fn read_body<S: AsyncRead + Unpin>(
    stream: &mut S,
    length: usize,
    cap: usize,
) -> Result<Vec<u8>> {
    if length > cap {
        bail!("control body exceeds byte cap");
    }
    let mut body = vec![0; length];
    tokio::time::timeout(CONTROL_IO_TIMEOUT, stream.read_exact(&mut body))
        .await
        .context("control body deadline elapsed")??;
    Ok(body)
}

pub async fn write_response<S: AsyncWrite + Unpin>(stream: &mut S, bytes: &[u8]) -> Result<()> {
    tokio::time::timeout(CONTROL_IO_TIMEOUT, stream.write_all(bytes))
        .await
        .context("control write deadline elapsed")??;
    Ok(())
}

/// Resolve once, check every candidate, then bind those exact addresses.
/// The opt-in exposes unauthenticated diagnostics, never mutation routes.
pub async fn bind_http_control(
    addr: &str,
    allow_unauthenticated_non_loopback: bool,
) -> Result<tokio::net::TcpListener> {
    let addresses: Vec<_> = tokio::time::timeout(CONTROL_IO_TIMEOUT, tokio::net::lookup_host(addr))
        .await
        .context("control address resolution deadline elapsed")??
        .collect();
    if addresses.is_empty()
        || (!allow_unauthenticated_non_loopback && addresses.iter().any(|a| !a.ip().is_loopback()))
    {
        bail!(
            "HTTP control is loopback-only; non-loopback diagnostics require --allow-unauthenticated-non-loopback-http"
        );
    }
    tokio::net::TcpListener::bind(addresses.as_slice())
        .await
        .context("bind HTTP control")
}

/// A reset/aborted accept belongs to that peer, not to listener health.
pub fn accept_or_retry<T>(accepted: std::io::Result<T>) -> Result<Option<T>> {
    match accepted {
        Ok(value) => Ok(Some(value)),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::WouldBlock
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(error).context("control listener accept failed"),
    }
}

/// Bounded accepted tasks; connection failures never become listener failures.
/// Failure diagnostics are logarithmically sampled, not canonical observations.
pub struct ControlConnections<T: Send + 'static> {
    tasks: tokio::task::JoinSet<Result<T>>,
    pub failures: u64,
}
impl<T: Send + 'static> Default for ControlConnections<T> {
    fn default() -> Self {
        Self {
            tasks: tokio::task::JoinSet::new(),
            failures: 0,
        }
    }
}
impl<T: Send + 'static> ControlConnections<T> {
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
    pub fn spawn(&mut self, task: impl Future<Output = Result<T>> + Send + 'static) {
        if self.tasks.len() >= CONTROL_CONNECTION_LIMIT {
            self.failed(); // Dropping the unpolled future closes the excess stream.
        } else {
            self.tasks.spawn(task);
        }
    }
    fn failed(&mut self) {
        self.failures = self.failures.saturating_add(1);
        if self.failures.is_power_of_two() {
            eprintln!("control connection failures/refusals: {}", self.failures);
        }
    }
    pub async fn join_next(&mut self) -> Option<T> {
        match self.tasks.join_next().await {
            Some(Ok(Ok(value))) => Some(value),
            Some(_) => {
                self.failed();
                None
            }
            None => None,
        }
    }
}

#[cfg(test)]
mod tests;

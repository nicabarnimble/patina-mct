//! Isolated startup is the same bounded transport, not an alternate authority path.
use super::*;
use mct_daemon::control_transport::{self as transport, ControlConnections};

pub(super) async fn run<S: std::future::Future<Output = ()>>(
    plane: mct_daemon::MctIsolatedStartupPlaneV1,
    socket_path: PathBuf,
    shutdown: S,
) -> Result<()> {
    use std::{
        fs,
        os::unix::fs::{FileTypeExt as _, PermissionsExt as _},
    };
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Ok(metadata) = fs::symlink_metadata(&socket_path) {
        if !metadata.file_type().is_socket() {
            bail!("startup control pathname is not a socket");
        }
        match tokio::time::timeout(
            transport::CONTROL_IO_TIMEOUT,
            tokio::net::UnixStream::connect(&socket_path),
        )
        .await
        {
            Ok(Err(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                ) => {}
            _ => bail!("isolated startup UDS is active or unavailable"),
        }
        fs::remove_file(&socket_path)?;
    }
    let listener = UnixListener::bind(&socket_path).context("bind isolated startup UDS")?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))?;
    let result = serve(listener, plane, shutdown).await;
    let _ = fs::remove_file(&socket_path);
    result
}

pub(super) async fn serve<S: std::future::Future<Output = ()>>(
    listener: UnixListener,
    plane: mct_daemon::MctIsolatedStartupPlaneV1,
    shutdown: S,
) -> Result<()> {
    // Serialize authority operations, not socket IO. Once a gate commits, later
    // queued requests cannot act on the old startup posture, even if its reply fails.
    let plane = Arc::new(std::sync::Mutex::new((plane, false)));
    let closing = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut connections = ControlConnections::default();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => {
                closing.store(true, std::sync::atomic::Ordering::Release);
                break;
            }
            completed = connections.join_next(), if !connections.is_empty() => {
                if completed == Some(true) { break; }
            }
            accepted = listener.accept() => {
                let Some((mut stream, _)) = transport::accept_or_retry(accepted)? else { continue; };
                let plane = Arc::clone(&plane);
                let closing = Arc::clone(&closing);
                connections.spawn(async move {
                    let uid = stream.peer_cred().context("authenticate startup UDS peer")?.uid();
                    let head = transport::read_request_head(&mut stream).await?;
                    let body = transport::read_body(&mut stream, head.content_length, 64 * 1024).await?;
                    // No transport timeout wraps or cancels the authority/durable operation.
                    let (response, gate_accepted) = tokio::task::spawn_blocking(move || -> Result<_> {
                        let mut guard = plane.lock().map_err(|_| anyhow::anyhow!("startup plane unavailable"))?;
                        if guard.1 || closing.load(std::sync::atomic::Ordering::Acquire) { bail!("startup plane closing or gate already accepted"); }
                        let response = guard.0.handle(uid, &head.method, &head.path, &body);
                        let accepted = head.method == "POST" && head.path.split('?').next() == Some("/startup/operator-gate")
                            && response.status_code() == 200;
                        guard.1 = accepted;
                        Ok((response, accepted))
                    }).await.context("startup operation task failed")??;
                    let extra_headers = response.headers().iter()
                        .map(|(name, value)| format!("{name}: {value}\r\n")).collect::<String>();
                    let head = format!("HTTP/1.1 {} Response\r\ncontent-type: {}\r\ncontent-length: {}\r\n{}connection: close\r\n\r\n",
                        response.status_code(), response.content_type(), response.body_bytes().len(), extra_headers);
                    let written = tokio::time::timeout(transport::CONTROL_IO_TIMEOUT, async {
                        use tokio::io::AsyncWriteExt as _;
                        stream.write_all(head.as_bytes()).await?;
                        stream.write_all(response.body_bytes()).await
                    }).await.context("startup response write deadline elapsed").and_then(|v| v.context("write startup response"));
                    if gate_accepted {
                        if written.is_err() { eprintln!("startup gate committed but response delivery failed"); }
                        Ok(true)
                    } else { written?; Ok(false) }
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "startup_control/tests.rs"]
mod tests;

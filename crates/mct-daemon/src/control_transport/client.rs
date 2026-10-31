use super::*;
use std::{
    fs,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::Path,
};
use tokio::net::UnixStream;

fn validate_socket(path: &Path, uid: u32) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).context("inspect control socket")?;
    if !metadata.file_type().is_socket() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
        bail!("control endpoint must be a non-symlink, owner-only socket owned by the current UID");
    }
    let parent = path.parent().context("control socket has no parent")?;
    let parent_metadata = fs::metadata(parent)?;
    if parent_metadata.uid() != uid || parent_metadata.mode() & 0o022 != 0 {
        bail!(
            "control socket parent must be owned by the current UID and not group/world writable"
        );
    }
    for ancestor in parent.ancestors().skip(1) {
        let metadata = fs::metadata(ancestor)?;
        // Root-owned sticky temporary roots are safe above an owner-controlled directory.
        let sticky_root = metadata.uid() == 0 && metadata.mode() & 0o1000 != 0;
        if (metadata.uid() != 0 && metadata.uid() != uid)
            || (metadata.mode() & 0o022 != 0 && !sticky_root)
        {
            bail!("untrusted control socket ancestor");
        }
    }
    Ok(metadata)
}

async fn request(
    socket: &Path,
    method: &str,
    path: &str,
    body: &[u8],
    deadline: Duration,
) -> Result<Option<(u16, Vec<u8>)>> {
    // Kernel credentials on our own socket pair identify the effective UID without unsafe code.
    let (local, other) = UnixStream::pair()?;
    let uid = local.peer_cred()?.uid();
    drop((local, other));
    match fs::symlink_metadata(socket) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).context("inspect control socket before connecting"),
        Ok(m) if m.file_type().is_symlink() => bail!("control socket symlinks are refused"),
        Ok(_) => {}
    }
    let parent = socket
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let canonical = fs::canonicalize(parent)?.join(
        socket
            .file_name()
            .context("control socket filename missing")?,
    );
    let before = validate_socket(&canonical, uid)?;
    if body.len() > crate::MCT_BLOB_MAX_BYTES.div_ceil(3) * 4 + CONTROL_HEADER_BYTES {
        bail!("control client request exceeds byte cap");
    }
    let headers = format!(
        "{method} {path} HTTP/1.1\r\nHost: local\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    parse_request_head(headers.as_bytes())?;
    tokio::time::timeout(deadline, async {
        let mut stream = match UnixStream::connect(&canonical).await {
            Ok(stream) => stream,
            // Only verified stale/absent endpoints permit the existing offline fallback.
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                ) =>
            {
                return Ok(None);
            }
            Err(e) => return Err(e).context("connect verified control socket"),
        };
        let after = validate_socket(&canonical, uid)?;
        if before.dev() != after.dev()
            || before.ino() != after.ino()
            || stream.peer_cred()?.uid() != uid
        {
            bail!("control socket identity changed or peer UID mismatch");
        }
        // Never send privileged payloads until both pathname and live peer checks pass.
        stream.write_all(headers.as_bytes()).await?;
        stream.write_all(body).await?;
        stream.shutdown().await?;
        let headers = read_headers(&mut stream).await?;
        let (first, length, _) = header_fields(&headers)?;
        let mut parts = first.split_ascii_whitespace();
        if !matches!(parts.next(), Some("HTTP/1.1" | "HTTP/1.0")) {
            bail!("invalid control response version");
        }
        let status_text = parts.next().context("missing control response status")?;
        if status_text.len() != 3 || !status_text.bytes().all(|b| b.is_ascii_digit()) {
            bail!("invalid control response status");
        }
        let status: u16 = status_text.parse()?;
        if !(100..600).contains(&status) {
            bail!("invalid control response status");
        }
        let length = length.context("control response missing content length")?;
        if length > CONTROL_RESPONSE_BYTES {
            bail!("control response exceeds byte cap");
        }
        let mut body = vec![0; length];
        stream
            .read_exact(&mut body)
            .await
            .context("read complete control response")?;
        Ok(Some((status, body)))
    })
    .await
    .context("control client exchange deadline elapsed")?
}

/// Synchronous CLI boundary usable even when called from a Tokio CLI runtime.
/// One absolute deadline covers connect, request writes and the complete response;
/// metadata inspection assumes the local filesystem is available. Same UID is not
/// publisher/process authentication and never creates administrative authority.
pub fn control_socket_request(
    socket: &Path,
    method: &str,
    path: &str,
    body: &[u8],
    deadline: Duration,
) -> Result<Option<(u16, Vec<u8>)>> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("control-client".into())
            .spawn_scoped(scope, || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?
                    .block_on(request(socket, method, path, body, deadline))
            })
            .context("start bounded control client")?
            .join()
            .map_err(|_| anyhow::anyhow!("control client worker failed"))?
    })
}

#[cfg(test)]
mod tests;

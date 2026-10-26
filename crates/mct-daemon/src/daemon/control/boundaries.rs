use super::*;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

async fn good<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S) {
    stream
        .write_all(b"GET /status HTTP/1.1\r\nHost: local\r\n\r\n")
        .await
        .unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(1), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert!(
        response.starts_with(b"HTTP/1.1 200"),
        "{}",
        String::from_utf8_lossy(&response)
    );
}

pub(super) async fn exercise<C, F, S>(connect: C)
where
    C: Fn() -> F,
    F: std::future::Future<Output = S>,
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut stalled = connect().await;
    stalled
        .write_all(b"GET /status HTTP/1.1\r\nHost:")
        .await
        .unwrap();
    // Must succeed while the incomplete predecessor is still connected.
    good(connect().await).await;
    let mut denied = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(4), stalled.read_to_end(&mut denied))
        .await
        .expect("partial request must expire");
    good(connect().await).await;
    for request in [
        vec![b'x'; mct_daemon::control_transport::CONTROL_HEADER_BYTES + 1],
        b"GET / HTTP/1.1\r\nContent-Length: 999999999\r\n\r\n".to_vec(),
        b"POST /unknown HTTP/1.1\r\nContent-Length: 999999999\r\n\r\n".to_vec(),
        b"GET / HTTP/1.1\r\nContent-Length: 0\r\nContent-Length: 1\r\n\r\n".to_vec(),
        b"broken\r\n\r\n".to_vec(),
    ] {
        let mut bad = connect().await;
        let _ = bad.write_all(&request).await;
        let mut response = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(1), bad.read_to_end(&mut response))
            .await
            .expect("bad request must be rejected promptly");
        assert!(!response.starts_with(b"HTTP/1.1 200"));
        good(connect().await).await;
    }
    let mut partial = connect().await;
    partial.write_all(b"GET /").await.unwrap();
    drop(partial);
    good(connect().await).await;
}

#[tokio::test]
async fn http_listener_isolates_stalled_oversized_malformed_and_disconnected_clients() {
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let source = ControlSnapshotSource::open(&dir.path().join("state.sqlite"));
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(serve_http_connections(listener, source, async {
        let _ = stopped.await;
    }));
    exercise(|| async { tokio::net::TcpStream::connect(addr).await.unwrap() }).await;
    let _idle = tokio::net::TcpStream::connect(addr).await.unwrap();
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn standalone_uds_listener_isolates_bad_clients() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let source = ControlSnapshotSource::open(&dir.path().join("state.sqlite"));
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(serve_readonly_uds_connections(listener, source, async {
        let _ = stopped.await;
    }));
    exercise(|| async { tokio::net::UnixStream::connect(&socket).await.unwrap() }).await;
    let _idle = tokio::net::UnixStream::connect(&socket).await.unwrap();
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn resident_uds_listener_preserves_readiness_after_bad_connections() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("control.sock");
    let config = dir.path().join("config.json");
    std::fs::write(&config, "{}").unwrap();
    let paths = ResidentRuntimePaths::new(
        config,
        dir.path().join("children"),
        dir.path().join("state.sqlite"),
    );
    let ledger = ResidentLedgerWriter::spawn(dir.path().join("ledger.jsonl")).unwrap();
    let (stop, stopped) = broadcast::channel(1);
    let (bound, ready) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(run_control_serve_uds_with_state_until(
        paths,
        socket.clone(),
        ledger.clone(),
        1,
        stopped,
        None,
        bound,
    ));
    ready.await.unwrap();
    exercise(|| async { tokio::net::UnixStream::connect(&socket).await.unwrap() }).await;
    let mut partial = tokio::net::UnixStream::connect(&socket).await.unwrap();
    partial
        .write_all(b"POST /peers/add HTTP/1.1\r\nContent-Length: 2\r\n\r\n{")
        .await
        .unwrap();
    good(tokio::net::UnixStream::connect(&socket).await.unwrap()).await;
    let mut response = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(4), partial.read_to_end(&mut response))
        .await
        .unwrap();
    good(tokio::net::UnixStream::connect(&socket).await.unwrap()).await;
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(4), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    ledger.close().await;
}

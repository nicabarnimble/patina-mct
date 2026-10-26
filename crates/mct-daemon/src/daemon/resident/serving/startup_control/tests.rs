use super::*;
use std::os::unix::fs::MetadataExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn startup_control_survives_partial_body_bad_headers_and_disconnects() {
    let dir = tempfile::tempdir().unwrap();
    let paths =
        mct_daemon::MctStartupPaths::supervised(dir.path(), dir.path().join("supervisor.plist"));
    std::fs::write(&paths.ledger, b"not-json\n").unwrap();
    let uid = std::fs::metadata(dir.path()).unwrap().uid();
    let plane =
        mct_daemon::MctIsolatedStartupPlaneV1::inspect(paths.clone(), "ledger-a", "mother-a", uid)
            .unwrap();
    let original = std::fs::read(&paths.ledger).unwrap();
    let socket = paths.control_socket.clone();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    // Exercise the production bind/permissions path too; wait only for a finite bind.
    let server = tokio::spawn(run(plane, socket.clone(), async {
        let _ = stopped.await;
    }));
    let mut first = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(stream) = tokio::net::UnixStream::connect(&socket).await {
                break stream;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    first
        .write_all(b"POST /startup/operator-gate HTTP/1.1\r\nContent-Length: 2\r\n\r\n{")
        .await
        .unwrap();
    async fn good(socket: &Path) {
        let mut stream = tokio::net::UnixStream::connect(socket).await.unwrap();
        stream
            .write_all(b"GET /status HTTP/1.1\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(1), stream.read_to_end(&mut response))
            .await
            .unwrap()
            .unwrap();
        assert!(response.starts_with(b"HTTP/1.1 200"));
    }
    good(&socket).await;
    let mut response = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(4), first.read_to_end(&mut response))
        .await
        .unwrap();
    for bytes in [
        vec![b'x'; 4097],
        b"POST /startup/operator-gate HTTP/1.1\r\nContent-Length: 65537\r\n\r\n".to_vec(),
        b"bad\r\n\r\n".to_vec(),
    ] {
        let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        let _ = stream.write_all(&bytes).await;
        let mut response = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(1), stream.read_to_end(&mut response))
            .await
            .unwrap();
        good(&socket).await;
    }
    drop(tokio::net::UnixStream::connect(&socket).await.unwrap());
    good(&socket).await;
    let _idle = tokio::net::UnixStream::connect(&socket).await.unwrap();
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        std::fs::read(&paths.ledger).unwrap(),
        original,
        "malformed requests must not mutate canonical history"
    );
}

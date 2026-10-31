use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

fn listener(dir: &tempfile::TempDir) -> (std::path::PathBuf, tokio::net::UnixListener) {
    let path = dir.path().join("control.sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    (path, listener)
}

#[tokio::test]
async fn client_response_caps_and_exact_boundary_controls() {
    for size in [16, CONTROL_RESPONSE_BYTES, CONTROL_RESPONSE_BYTES + 1] {
        let dir = tempfile::tempdir().unwrap();
        let (path, listener) = listener(&dir);
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_headers(&mut stream).await.unwrap();
            stream
                .write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {size}\r\n\r\n").as_bytes())
                .await
                .unwrap();
            if size <= CONTROL_RESPONSE_BYTES {
                stream.write_all(&vec![b'x'; size]).await.unwrap();
            }
        });
        let result = request(&path, "GET", "/snapshot", &[], Duration::from_secs(2)).await;
        if size <= CONTROL_RESPONSE_BYTES {
            assert_eq!(result.unwrap().unwrap().1.len(), size);
        } else {
            assert!(format!("{:#}", result.unwrap_err()).contains("byte cap"));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn partial_response_has_one_total_deadline_and_never_becomes_offline_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let (path, listener) = listener(&dir);
    let (release, hold) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_headers(&mut stream).await.unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nx")
            .await
            .unwrap();
        let _ = hold.await;
        drop(stream);
    });
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        request(&path, "GET", "/snapshot", &[], Duration::from_millis(100)),
    )
    .await
    .unwrap();
    assert!(result.unwrap_err().to_string().contains("deadline"));
    release.send(()).unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn unverified_socket_paths_fail_before_connect_or_payload_delivery() {
    let dir = tempfile::tempdir().unwrap();
    let (path, listener) = listener(&dir);
    let uid = fs::metadata(&path).unwrap().uid();
    assert!(validate_socket(&path, uid.wrapping_add(1)).is_err());
    let alias = dir.path().join("alias.sock");
    symlink(&path, &alias).unwrap();
    assert!(
        request(
            &alias,
            "POST",
            "/sensitive",
            b"secret",
            Duration::from_secs(1)
        )
        .await
        .is_err()
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(
        request(
            &path,
            "POST",
            "/sensitive",
            b"secret",
            Duration::from_secs(1)
        )
        .await
        .is_err()
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o777)).unwrap();
    assert!(
        request(
            &path,
            "POST",
            "/sensitive",
            b"secret",
            Duration::from_secs(1)
        )
        .await
        .is_err()
    );
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err()
    );
    let regular = dir.path().join("not-a-socket");
    fs::write(&regular, "{}").unwrap();
    assert!(
        request(&regular, "GET", "/snapshot", &[], Duration::from_secs(1))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn client_rejects_ambiguous_or_truncated_responses() {
    for response in [
        b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nx".as_slice(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nx",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (path, listener) = listener(&dir);
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_headers(&mut stream).await.unwrap();
            stream.write_all(response).await.unwrap();
        });
        assert!(
            request(&path, "GET", "/", &[], Duration::from_secs(1))
                .await
                .is_err()
        );
        server.await.unwrap();
    }
}

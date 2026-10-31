use super::*;

#[test]
fn peer_accept_failures_are_retryable_but_listener_failures_surface() {
    use std::io::{Error, ErrorKind::*};
    for kind in [Interrupted, ConnectionAborted, ConnectionReset, WouldBlock] {
        assert!(
            accept_or_retry::<()>(Err(Error::from(kind)))
                .unwrap()
                .is_none()
        );
    }
    assert!(accept_or_retry::<()>(Err(Error::from(PermissionDenied))).is_err());
    assert_eq!(accept_or_retry(Ok(7)).unwrap(), Some(7));
}

#[test]
fn strict_framing_rejects_ambiguous_and_malformed_headers() {
    for headers in [
        "GET /status\r\n\r\n",
        "GET /status HTTP/1.1 extra\r\n\r\n",
        "POST /calls HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\n",
        "POST /calls HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
        "POST /calls HTTP/1.1\r\nContent-Length: +1\r\n\r\n",
        "POST /calls HTTP/1.1\r\nContent-Length: 999999999999999999999999\r\n\r\n",
        "GET / HTTP/1.1\r\nAuthorization: one\r\nAuthorization: two\r\n\r\n",
        "GET / HTTP/1.1\r\n broken\r\n\r\n",
    ] {
        assert!(
            parse_request_head(headers.as_bytes()).is_err(),
            "{headers:?}"
        );
    }
    assert_eq!(
        parse_request_head(b"GET /status HTTP/1.1\r\nContent-Length: 0\r\n\r\n")
            .unwrap()
            .content_length,
        0
    );
}

#[tokio::test(start_paused = true)]
async fn absolute_header_body_and_write_deadlines_and_byte_caps() {
    let (mut stream, mut client) = tokio::io::duplex(16);
    client.write_all(b"GET /").await.unwrap();
    let started = tokio::time::Instant::now();
    assert!(
        read_request_head(&mut stream)
            .await
            .unwrap_err()
            .to_string()
            .contains("deadline")
    );
    assert_eq!(started.elapsed(), CONTROL_IO_TIMEOUT);
    assert!(
        read_body(&mut stream, CONTROL_BODY_BYTES + 1, CONTROL_BODY_BYTES)
            .await
            .is_err()
    );
    let started = tokio::time::Instant::now();
    assert!(
        read_body(&mut stream, 1, 1)
            .await
            .unwrap_err()
            .to_string()
            .contains("deadline")
    );
    assert_eq!(started.elapsed(), CONTROL_IO_TIMEOUT);
    let started = tokio::time::Instant::now();
    assert!(
        write_response(&mut stream, &[0; 32])
            .await
            .unwrap_err()
            .to_string()
            .contains("deadline")
    );
    assert_eq!(started.elapsed(), CONTROL_IO_TIMEOUT);
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn transport_deadlines_do_not_cancel_required_mutation_work() {
    use std::sync::Arc;
    let (mut client, server) = tokio::net::UnixStream::pair().unwrap();
    let uid = server.peer_cred().unwrap().uid();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let handler = crate::MctUdsControlMutationHandler::new(uid, {
        let entered = Arc::clone(&entered);
        let release = Arc::clone(&release);
        move |_, _, _| {
            let entered = Arc::clone(&entered);
            let release = Arc::clone(&release);
            async move {
                entered.notify_one();
                let _permit = release.acquire().await.unwrap();
                crate::MctControlPlaneResponse {
                    status_code: 200,
                    content_type: "application/json".into(),
                    body: "{}".into(),
                }
            }
        }
    });
    let task = tokio::spawn(async move {
        crate::serve_uds_control_stream_with_handlers(
            server,
            Err(crate::MctControlPlaneSnapshotError::runtime_state_unavailable()),
            Some(&handler),
            None,
        )
        .await
    });
    client
        .write_all(b"POST /mutation HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}")
        .await
        .unwrap();
    entered.notified().await;
    tokio::time::advance(CONTROL_IO_TIMEOUT * 3).await;
    assert!(
        !task.is_finished(),
        "transport timer cancelled required work"
    );
    release.add_permits(1);
    let mut response = Vec::new();
    client.read_to_end(&mut response).await.unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200"));
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn non_loopback_http_bind_is_rejected_before_listening() {
    for addr in ["0.0.0.0:0", "[::]:0", "192.0.2.1:0"] {
        let error = bind_http_control(addr, false).await.unwrap_err();
        assert!(error.to_string().contains("loopback-only"), "{error:#}");
    }
    let listener = bind_http_control("127.0.0.1:0", false).await.unwrap();
    assert!(listener.local_addr().unwrap().ip().is_loopback());
    // Exercise explicit policy acceptance without opening an external interface.
    assert!(bind_http_control("127.0.0.1:0", true).await.is_ok());
}

#[tokio::test]
async fn completed_and_failed_connection_tasks_are_reaped_and_capacity_is_bounded() {
    let mut connections = ControlConnections::<()>::default();
    for _ in 0..CONTROL_CONNECTION_LIMIT {
        connections.spawn(std::future::pending());
    }
    connections.spawn(async {
        panic!("excess task must never run");
    });
    assert_eq!(connections.tasks.len(), CONTROL_CONNECTION_LIMIT);
    assert_eq!(connections.failures, 1);
    drop(connections);
    let mut connections = ControlConnections::default();
    connections.spawn(async { anyhow::bail!("bad connection") });
    assert_eq!(connections.join_next().await, None);
    assert_eq!(connections.failures, 1);
    connections.spawn(async { Ok(7) });
    assert_eq!(connections.join_next().await, Some(7));
}

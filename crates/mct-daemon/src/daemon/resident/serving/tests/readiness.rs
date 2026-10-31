//! Deterministic startup ordering: an old socket is never a new listener receipt.
use super::*;
use std::{future::Future, pin::Pin, task::Poll};

async fn assert_pending<F: Future>(mut future: Pin<&mut F>) {
    std::future::poll_fn(|cx| {
        assert!(
            future.as_mut().poll(cx).is_pending(),
            "readiness preceded the new listener bind"
        );
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn restart_readiness_waits_for_new_listener_not_stale_socket_or_unlink() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("control.sock");
    let config = dir.path().join("config.json");
    std::fs::write(&config, "{}").unwrap();
    let paths = ResidentRuntimePaths::new(
        config,
        dir.path().join("children"),
        dir.path().join("state.sqlite"),
    );
    let ledger_path = dir.path().join("observations.jsonl");
    let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
    // Dropping a bound Unix listener leaves the same pathname residue as SIGKILL.
    drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
    assert!(socket.exists());
    assert!(tokio::net::UnixStream::connect(&socket).await.is_err());

    let (start, started) = tokio::sync::oneshot::channel();
    let (unlinked, removed) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let (bound, received) = tokio::sync::oneshot::channel();
    let (shutdown, shutdown_rx) = broadcast::channel(1);
    let task_socket = socket.clone();
    let task_ledger = ledger.clone();
    let task = tokio::spawn(async move {
        // Force both restart interleavings without sleeps or scheduler luck.
        started.await.unwrap();
        std::fs::remove_file(&task_socket).unwrap();
        unlinked.send(()).unwrap();
        released.await.unwrap();
        run_control_serve_uds_with_state_until(
            paths,
            task_socket,
            task_ledger,
            1,
            shutdown_rx,
            None,
            bound,
        )
        .await
    });
    let mut ready = Box::pin(wait_for_resident_control_bound(task, received));
    assert_pending(ready.as_mut()).await; // stale pathname still exists
    start.send(()).unwrap();
    removed.await.unwrap();
    assert!(!socket.exists());
    assert_pending(ready.as_mut()).await; // old socket removed, new bind held
    release.send(()).unwrap();
    let task = tokio::time::timeout(Duration::from_secs(5), ready)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        std::fs::read_to_string(&ledger_path)
            .unwrap()
            .contains("authenticated local call endpoint ready")
    );
    // Immediately usable after the receipt; no pathname/status polling retry.
    let (status, _) = resident_uds_request(
        &socket,
        b"GET /status HTTP/1.1\r\nHost: local\r\n\r\n".to_vec(),
    )
    .await;
    assert_eq!(status, 200);
    shutdown.send(()).unwrap();
    task.await.unwrap().unwrap();
    assert!(!socket.exists());
    ledger.close().await;
}

#[tokio::test]
async fn control_readiness_propagates_uds_startup_failures_without_false_ready() {
    for failure in ["bind", "snapshot", "ledger"] {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("control.sock");
        let config = dir.path().join("config.json");
        let state = dir.path().join("state.sqlite");
        std::fs::write(&config, "{}").unwrap();
        if failure == "bind" {
            std::fs::create_dir(&socket).unwrap();
        } else if failure == "snapshot" {
            std::fs::create_dir(&state).unwrap();
        }
        let paths = ResidentRuntimePaths::new(config, dir.path().join("children"), state);
        let ledger = ResidentLedgerWriter::spawn(dir.path().join("observations.jsonl")).unwrap();
        if failure == "ledger" {
            ledger.clone().close().await;
        }
        let (_shutdown, shutdown_rx) = broadcast::channel(1);
        let control = ResidentControlTransport::Uds(socket.clone());
        let (task, received) = spawn_resident_control_task(
            control.clone(),
            paths,
            ledger.clone(),
            1,
            shutdown_rx,
            None,
        );
        let error = tokio::time::timeout(
            Duration::from_secs(5),
            wait_for_resident_control_bound(task, received),
        )
        .await
        .expect("startup failure must propagate, not wait for a pathname timeout")
        .unwrap_err();
        match failure {
            "bind" => assert!(
                error
                    .chain()
                    .any(|cause| cause.downcast_ref::<std::io::Error>().is_some()),
                "{error:#}"
            ),
            "snapshot" => assert!(format!("{error:#}").contains("status was not ready")),
            "ledger" => assert!(
                format!("{error:#}").contains("record authenticated local call endpoint readiness")
            ),
            _ => unreachable!(),
        }
        assert!(tokio::net::UnixStream::connect(&socket).await.is_err());
        ledger.close().await;
    }
}

#[tokio::test]
async fn control_readiness_propagates_http_bind_failure() {
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let paths = ResidentRuntimePaths::new(
        dir.path().join("config.json"),
        dir.path().join("children"),
        dir.path().join("state.sqlite"),
    );
    let ledger = ResidentLedgerWriter::spawn(dir.path().join("observations.jsonl")).unwrap();
    let (_shutdown, shutdown_rx) = broadcast::channel(1);
    let control = ResidentControlTransport::Http(occupied.local_addr().unwrap().to_string());
    let (task, received) =
        spawn_resident_control_task(control.clone(), paths, ledger.clone(), 1, shutdown_rx, None);
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        wait_for_resident_control_bound(task, received),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(
        error.chain().any(|cause| cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::AddrInUse)),
        "{error:#}"
    );
    ledger.close().await;
}

#[tokio::test]
async fn control_readiness_rejects_task_exit_or_panic_before_bind() {
    for panic in [false, true] {
        let (bound, received) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _bound = bound;
            assert!(!panic, "injected control startup panic");
            Ok(())
        });
        let error = tokio::time::timeout(
            Duration::from_secs(5),
            wait_for_resident_control_bound(task, received),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(
            format!("{error:#}").contains(if panic {
                "injected control startup panic"
            } else {
                "stopped before readiness"
            }),
            "{error:#}"
        );
    }
}

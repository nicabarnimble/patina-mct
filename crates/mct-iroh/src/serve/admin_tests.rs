use super::*;
use crate::MotherIrohEndpointConfig;
use mct_kernel::admin_key::{AdminKeyScopeV1, PasskeyAdminActionV1, PasskeyAdminIntentV1};
use mct_kernel::admin_sig::{DayKeyAdminProofV1, PasskeyAdminRequestV1};
use std::sync::atomic::{AtomicUsize, Ordering};

fn request() -> PasskeyAdminRequestV1 {
    PasskeyAdminRequestV1 {
        intent: PasskeyAdminIntentV1 {
            mother_node_id: "mother".into(),
            ledger_id: "ledger".into(),
            epoch: "epoch".into(),
            generation: 1,
            authority_observation_id: "obs:authority".into(),
            nonce: "ab".repeat(32),
            issued_at: Timestamp::new("2026-10-28T00:00:00Z").unwrap(),
            expires_at: Timestamp::new("2026-10-28T00:05:00Z").unwrap(),
            action: PasskeyAdminActionV1::Command {
                scope: AdminKeyScopeV1::Status,
                route: "/status".into(),
                body: String::new(),
            },
        },
        proof: DayKeyAdminProofV1::Ed25519DayKeyV1 {
            grant_id: "day".into(),
            signature: "00".repeat(64),
        },
    }
}
fn time() -> Timestamp {
    Timestamp::new("2026-10-28T00:00:01Z").unwrap()
}

#[tokio::test]
async fn admin_transport_bounds_schema_and_expiry_precede_the_authority_owner() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let owner = MctIrohAdminHandler::new(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
        async { MctIrohAdminReply::Rejected }
    });
    for bytes in [
        vec![b'x'; MCT_ADMIN_FRAME_MAX_BYTES + 1],
        b"{}".to_vec(),
        b"{\"uid\":0}".to_vec(),
    ] {
        assert_eq!(
            admin::dispatch(Some(&owner), &bytes, time()).await,
            MctIrohAdminReply::Rejected
        );
    }
    let mut expired = request();
    expired.intent.expires_at = time();
    assert_eq!(
        admin::dispatch(Some(&owner), &serde_json::to_vec(&expired).unwrap(), time()).await,
        MctIrohAdminReply::Rejected
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let bytes = serde_json::to_vec(&request()).unwrap();
    assert_eq!(
        admin::dispatch(None, &bytes, time()).await,
        MctIrohAdminReply::Unavailable
    );
    assert_eq!(
        admin::dispatch(Some(&owner), &bytes, time()).await,
        MctIrohAdminReply::Rejected
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "transport syntax is not signature authority"
    );
}

#[tokio::test]
async fn admin_transport_cancellation_cannot_cancel_owned_authority_or_durability_work() {
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let done = Arc::new(AtomicUsize::new(0));
    let (s, r, d) = (started.clone(), release.clone(), done.clone());
    let owner = MctIrohAdminHandler::new(move |_| {
        let (s, r, d) = (s.clone(), r.clone(), d.clone());
        async move {
            s.notify_one();
            r.notified().await;
            d.fetch_add(1, Ordering::SeqCst);
            MctIrohAdminReply::Completed {
                result: serde_json::json!({}),
            }
        }
    });
    let task = tokio::spawn(async move {
        admin::dispatch(
            Some(&owner),
            &serde_json::to_vec(&request()).unwrap(),
            time(),
        )
        .await
    });
    started.notified().await;
    task.abort();
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(2), async {
        while done.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn admin_iroh_uses_explicit_alpn_and_never_requests_peer_or_call_authority() {
    let server =
        MotherIrohEndpoint::bind(MotherIrohEndpointConfig::local_mct().with_administration())
            .await
            .unwrap();
    let ticket = server.ticket();
    let mut client = MotherIrohEndpoint::bind(MotherIrohEndpointConfig::local_mct())
        .await
        .unwrap();
    let mut config = MctIrohConcurrentServeConfig::new(MctIrohObservationSink::new(|_| async {
        Ok::<_, std::io::Error>(())
    }));
    config.admin_handler = Some(MctIrohAdminHandler::new(|_| async {
        MctIrohAdminReply::Rejected
    }));
    let closer = server.endpoint.as_ref().unwrap().clone();
    let serving = server;
    let task = tokio::spawn(async move {
        serving
            .serve_concurrent_with_binding_provider(
                MctIrohServeState::new(),
                config,
                time,
                || async { panic!("admin must not consult peer authority") },
                |_, _, _| async { panic!("admin must not enter call routing") },
            )
            .await
    });
    let reply = client.send_admin(&ticket, &request()).await.unwrap();
    assert_eq!(reply, MctIrohAdminReply::Rejected);
    client.close().await;
    closer.close().await;
    task.await.unwrap().unwrap();
}

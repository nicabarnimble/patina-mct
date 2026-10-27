//! Hybrid admin ledger sizes and primitive timings.
//!
//! Ledger lines are real `JsonlObservationLedger` appends. Sizes are file-length
//! deltas, so each figure includes the trailing newline. Primitives use the
//! workspace release profile (opt-level 3, thin LTO, one codegen unit).

use mct_kernel::{
    AdminHybridSigningKey, AdminKeyGrantClassV1, AdminKeyGrantStateV1, AdminKeyGrantV1,
    AdminKeyScopeV1, AdminSignatureSchemeV1, AdminSignatureV1, Timestamp,
    verify_admin_hybrid_signature,
};
use mct_observation::{
    AuthorityChangeV1, AuthorityMutationRequestV1, AuthorityMutationResultV1, GrantShapingSourceV1,
    JsonlObservationLedger, admin_change_signing_message,
};
use std::time::{Duration, Instant};

fn file_len(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn percentile(samples: &mut [Duration], fraction: f64) -> f64 {
    samples.sort_unstable();
    let rank = (fraction * (samples.len() - 1) as f64).round() as usize;
    samples[rank].as_secs_f64() * 1e6
}

fn kb(bytes: u64) -> f64 {
    bytes as f64 / 1000.0
}

fn grant_from(
    key: &AdminHybridSigningKey,
    grant_id: &str,
    class: AdminKeyGrantClassV1,
    scopes: Vec<AdminKeyScopeV1>,
    expires_at: Option<Timestamp>,
) -> AdminKeyGrantV1 {
    AdminKeyGrantV1 {
        grant_id: grant_id.into(),
        issuance_ceiling: None,
        public_half: key.public_half(),
        ml_dsa65_public_key: Some(key.ml_dsa65_public_key()),
        class,
        scopes,
        expires_at,
        grant_state: AdminKeyGrantStateV1::Active,
    }
}

fn sign_change(
    key: &AdminHybridSigningKey,
    signer: &AdminKeyGrantV1,
    nonce: u64,
    change: AuthorityChangeV1,
) -> AuthorityChangeV1 {
    let message = admin_change_signing_message("mother-a", "ledger-a", nonce, signer, &change)
        .expect("canonical signing message");
    let signature = Some(AdminSignatureV1 {
        scheme: AdminSignatureSchemeV1::Ed25519MlDsa65V1,
        signer_grant_id: signer.grant_id.clone(),
        nonce,
        signature: key.sign_hex(&message),
    });
    match change {
        AuthorityChangeV1::AdminKeyGrantPut { grant, .. } => {
            AuthorityChangeV1::AdminKeyGrantPut { grant, signature }
        }
        AuthorityChangeV1::AdminKeyGrantRevoke { grant_id, .. } => {
            AuthorityChangeV1::AdminKeyGrantRevoke {
                grant_id,
                signature,
            }
        }
        AuthorityChangeV1::PairingTicketIssued {
            ticket_hash,
            peer_endpoint_id,
            window_starts_at,
            window_expires_at,
            ..
        } => AuthorityChangeV1::PairingTicketIssued {
            ticket_hash,
            peer_endpoint_id,
            window_starts_at,
            window_expires_at,
            signature,
        },
        AuthorityChangeV1::PairingTicketConsumed {
            ticket_hash,
            admission,
            ..
        } => AuthorityChangeV1::PairingTicketConsumed {
            ticket_hash,
            admission,
            signature,
        },
        other => panic!("not an admin fact: {other:?}"),
    }
}

fn committed(result: &AuthorityMutationResultV1) -> bool {
    matches!(
        result,
        AuthorityMutationResultV1::Committed { .. }
            | AuthorityMutationResultV1::CommittedProjectionPending { .. }
    )
}

fn measure_ledger() -> u64 {
    let path = std::env::temp_dir().join("phase3-hybrid-session.jsonl");
    let _ = std::fs::remove_file(&path);
    let mut ledger =
        JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").expect("ledger");
    let recovery_key = AdminHybridSigningKey::generate();
    let day_key = AdminHybridSigningKey::generate();
    let recovery = grant_from(
        &recovery_key,
        "recovery",
        AdminKeyGrantClassV1::Recovery,
        vec![AdminKeyScopeV1::KeyManage, AdminKeyScopeV1::StartupGate],
        None,
    );
    let day = grant_from(
        &day_key,
        "day-to-day",
        AdminKeyGrantClassV1::DayToDay,
        vec![AdminKeyScopeV1::KeyManage, AdminKeyScopeV1::Status],
        Some(Timestamp::new("2026-10-14T00:00:00Z").expect("expiry")),
    );
    let ticket_hash = "a".repeat(64);
    let endpoint = day.public_half.to_hex();
    let starts = Timestamp::new("2026-10-15T12:00:00Z").expect("window start");
    let expires = Timestamp::new("2026-10-16T12:00:00Z").expect("window end");
    let steps = [
        (
            "bootstrap-recovery",
            sign_change(
                &recovery_key,
                &recovery,
                1,
                AuthorityChangeV1::AdminKeyGrantPut {
                    grant: Box::new(recovery.clone()),
                    signature: None,
                },
            ),
        ),
        (
            "ticket-issue",
            sign_change(
                &recovery_key,
                &recovery,
                2,
                AuthorityChangeV1::PairingTicketIssued {
                    ticket_hash: ticket_hash.clone(),
                    peer_endpoint_id: Some(endpoint.clone()),
                    window_starts_at: Some(starts.clone()),
                    window_expires_at: Some(expires),
                    signature: None,
                },
            ),
        ),
        (
            "ticket-consume",
            sign_change(
                &recovery_key,
                &recovery,
                3,
                AuthorityChangeV1::PairingTicketConsumed {
                    ticket_hash,
                    admission: Some(mct_observation::PairingTicketAdmissionV1 {
                        authenticated_peer_endpoint_id: endpoint.clone(),
                        admitted_at: starts.clone(),
                    }),
                    signature: None,
                },
            ),
        ),
        (
            "grant-put",
            sign_change(
                &recovery_key,
                &recovery,
                4,
                AuthorityChangeV1::AdminKeyGrantPut {
                    grant: Box::new(day),
                    signature: None,
                },
            ),
        ),
        (
            "grant-revoke",
            sign_change(
                &recovery_key,
                &recovery,
                5,
                AuthorityChangeV1::AdminKeyGrantRevoke {
                    grant_id: "day-to-day".into(),
                    signature: None,
                },
            ),
        ),
    ];
    let tenure = file_len(&path);
    println!("ledger_tenure_bytes\t{tenure}");
    let source = GrantShapingSourceV1::AdminKey {
        public_half: recovery.public_half,
    };
    let mut session = 0u64;
    for (id, change) in steps {
        let before = file_len(&path);
        let request = AuthorityMutationRequestV1 {
            mutation_id: id.into(),
            changes: vec![change],
            grant_shaping_sources: vec![source.clone()],
            decided_at: "2026-10-15T12:00:00Z".into(),
        };
        // Measurement-owned fixture facts, not a production transport authenticator.
        let result = if id == "ticket-consume" {
            ledger.execute_pairing_ticket_admission(
                request,
                &mct_kernel::EndpointIdText::new(endpoint.clone()).unwrap(),
                &starts,
                |_| Ok(None),
            )
        } else {
            ledger.execute_authority_mutation(request, |_| Ok(None))
        };
        let delta = file_len(&path) - before;
        session += delta;
        println!(
            "ledger_fact\t{id}\t{delta}\t{:.2}\tcommitted={}",
            kb(delta),
            committed(&result)
        );
        if !committed(&result) {
            panic!("{id} was not committed: {result:?}");
        }
    }
    println!("ledger_session_bytes\t{session}");
    println!("ledger_session_kb\t{:.2}", kb(session));
    session
}

fn measure_primitives() {
    let msg = vec![0x5au8; 160];
    let mut keygen = Vec::with_capacity(50);
    for _ in 0..10 {
        let generated = AdminHybridSigningKey::generate();
        std::hint::black_box((generated.public_half(), generated.ml_dsa65_public_key()));
    }
    for _ in 0..50 {
        let started = Instant::now();
        let generated = AdminHybridSigningKey::generate();
        std::hint::black_box((generated.public_half(), generated.ml_dsa65_public_key()));
        keygen.push(started.elapsed());
    }
    let key = AdminHybridSigningKey::generate();
    let ed = key.public_half();
    let pq = key.ml_dsa65_public_key();
    let sig = key.sign_hex(&msg);
    assert_eq!(pq.as_bytes().len(), 1952);
    assert_eq!(key.sign(&msg).len(), 3374);
    for _ in 0..20 {
        std::hint::black_box(key.sign(&msg));
    }
    let mut one_key = Vec::with_capacity(200);
    for _ in 0..200 {
        let started = Instant::now();
        std::hint::black_box(key.sign(&msg));
        one_key.push(started.elapsed());
    }
    let mut across = Vec::with_capacity(64);
    for _ in 0..64 {
        let sample_key = AdminHybridSigningKey::generate();
        for _ in 0..3 {
            std::hint::black_box(sample_key.sign(&msg));
        }
        let started = Instant::now();
        std::hint::black_box(sample_key.sign(&msg));
        across.push(started.elapsed());
    }
    verify_admin_hybrid_signature(&ed, &pq, &msg, &sig).expect("verify");
    for _ in 0..20 {
        verify_admin_hybrid_signature(&ed, &pq, &msg, &sig).expect("verify");
    }
    let mut verify = Vec::with_capacity(200);
    for _ in 0..200 {
        let started = Instant::now();
        verify_admin_hybrid_signature(&ed, &pq, &msg, &sig).expect("verify");
        std::hint::black_box(&sig);
        verify.push(started.elapsed());
    }
    println!("primitive_msg_bytes\t160");
    println!("primitive_pk_bytes\t{}", pq.as_bytes().len());
    println!("primitive_sig_bytes\t3374");
    println!(
        "keygen_us\tp10={:.1}\tp50={:.1}\tp90={:.1}\tn=50\twarmup=10",
        percentile(&mut keygen, 0.10),
        percentile(&mut keygen, 0.50),
        percentile(&mut keygen, 0.90)
    );
    println!(
        "sign_one_key_us\tp10={:.1}\tp50={:.1}\tp90={:.1}\tn=200\twarmup=20",
        percentile(&mut one_key, 0.10),
        percentile(&mut one_key, 0.50),
        percentile(&mut one_key, 0.90)
    );
    println!(
        "sign_across_keys_us\tp10={:.1}\tp50={:.1}\tp90={:.1}\tkeys=64\twarmup_per_key=3",
        percentile(&mut across, 0.10),
        percentile(&mut across, 0.50),
        percentile(&mut across, 0.90)
    );
    println!(
        "verify_us\tp10={:.1}\tp50={:.1}\tp90={:.1}\tn=200\twarmup=20",
        percentile(&mut verify, 0.10),
        percentile(&mut verify, 0.50),
        percentile(&mut verify, 0.90)
    );
}

fn main() {
    println!("arch\t{}", std::env::consts::ARCH);
    let session = measure_ledger();
    measure_primitives();
    println!("session_bytes_for_gate\t{session}");
}

use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use mct_kernel::admin_key::*;
use mct_kernel::admin_sig::*;
use p256::ecdsa::signature::Signer as _;
use sha2::{Digest as _, Sha256};

fn clear_ledger_replay_cache(path: &Path, ledger: &str, mother: &str) {
    replay_cache()
        .lock()
        .unwrap()
        .remove(&replay_cache_key(path, ledger, mother));
}
fn time(s: u64) -> Timestamp {
    Timestamp::new(
        "2026-10-28T00:00:00Z"
            .parse::<jiff::Timestamp>()
            .unwrap()
            .checked_add(std::time::Duration::from_secs(s))
            .unwrap()
            .to_string(),
    )
    .unwrap()
}
fn root_key() -> p256::ecdsa::SigningKey {
    p256::ecdsa::SigningKey::from_slice(&[7; 32]).unwrap()
}
fn root() -> WebAuthnP256DayKeyV1 {
    WebAuthnP256DayKeyV1 {
        credential_id: URL_SAFE_NO_PAD.encode([8; 32]),
        public_key_sec1: const_hex::encode(root_key().verifying_key().to_sec1_bytes()),
        rp_id: "admin.example.test".into(),
        origin: "https://admin.example.test".into(),
    }
}
fn ed_key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[9; 32])
}
fn startup() -> AuthorityTenureStartupEvidenceV1 {
    AuthorityTenureStartupEvidenceV1 {
        startup_class: AuthorityStartupClassV1::Virgin,
        expected_predecessor: AuthorityEpochPredecessorV1::NoneForVirgin,
        expected_prior_authority: None,
        expected_authority_state_hash: authority_state_hash(&AuthorityStateV1::default()).unwrap(),
        inventory_hash: "ab".repeat(32),
        operator_gate_decision_id: None,
        authenticated_principal_ref: None,
    }
}
fn setup() -> PasskeyGenesisSetupV1 {
    PasskeyGenesisSetupV1::from_registered_passkey(
        "mother-a".into(),
        "ledger-a".into(),
        root(),
        &startup(),
    )
    .unwrap()
}
fn base(action: PasskeyAdminActionV1, nonce: u64) -> PasskeyAdminIntentV1 {
    PasskeyAdminIntentV1 {
        mother_node_id: "mother-a".into(),
        ledger_id: "ledger-a".into(),
        epoch: format!("{AUTHORITY_EPOCH_PREFIX_V1}{}", "ac".repeat(32)),
        generation: 0,
        authority_observation_id: String::new(),
        nonce: format!("{nonce:064x}"),
        issued_at: time(0),
        expires_at: time(300),
        action,
    }
}
fn intent(
    w: &JsonlObservationLedger,
    action: PasskeyAdminActionV1,
    nonce: u64,
) -> PasskeyAdminIntentV1 {
    let mut i = base(action, nonce);
    let a = w
        .cached_replay()
        .unwrap()
        .current_authority
        .as_ref()
        .unwrap();
    i.epoch = a.authority_epoch.clone();
    i.generation = a.generation;
    i.authority_observation_id = a.source_authority_observation_id.clone();
    i
}
fn root_signed_with(
    i: PasskeyAdminIntentV1,
    change: impl FnOnce(&mut Vec<u8>, &mut serde_json::Value),
) -> PasskeyAdminRequestV1 {
    let r = root();
    let mut auth = Sha256::digest(r.rp_id.as_bytes()).to_vec();
    auth.push(0x1d);
    auth.extend([0; 4]);
    let mut client = serde_json::json!({"type":"webauthn.get","challenge":webauthn_admin_challenge(&i, &r.credential_id).unwrap(),"origin":r.origin,"crossOrigin":false});
    change(&mut auth, &mut client);
    let client = serde_json::to_vec(&client).unwrap();
    let mut signed = auth.clone();
    signed.extend(Sha256::digest(&client));
    let signature: p256::ecdsa::Signature = root_key().sign(&signed);
    PasskeyAdminRequestV1 {
        intent: i,
        proof: DayKeyAdminProofV1::WebAuthnP256DayKeyV1 {
            credential_id: r.credential_id,
            authenticator_data: URL_SAFE_NO_PAD.encode(auth),
            client_data_json: URL_SAFE_NO_PAD.encode(client),
            signature: URL_SAFE_NO_PAD.encode(signature.to_der().as_bytes()),
        },
    }
}
fn root_signed(i: PasskeyAdminIntentV1) -> PasskeyAdminRequestV1 {
    root_signed_with(i, |_, _| {})
}
fn ed_signed(i: PasskeyAdminIntentV1, grant_id: &str) -> PasskeyAdminRequestV1 {
    let signature =
        ed_key().sign(&day_key_signing_message(&i, &format!("ed25519:{grant_id}")).unwrap());
    PasskeyAdminRequestV1 {
        intent: i,
        proof: DayKeyAdminProofV1::Ed25519DayKeyV1 {
            grant_id: grant_id.into(),
            signature: const_hex::encode(signature.to_bytes()),
        },
    }
}
fn birth() -> PasskeyAdminRequestV1 {
    root_signed(base(
        PasskeyAdminActionV1::Bootstrap {
            root: root(),
            inventory_hash: startup().inventory_hash,
        },
        0,
    ))
}
fn writer(path: &Path) -> JsonlObservationLedger {
    let mut w = JsonlObservationLedger::open(path, "ledger-a", "mother-a").unwrap();
    w.execute_passkey_genesis_at(&setup(), birth(), time(0))
        .unwrap();
    w
}
fn issue_action(id: &str, scopes: Vec<AdminKeyScopeV1>, end: u64) -> PasskeyAdminActionV1 {
    PasskeyAdminActionV1::IssueDayKey {
        grant_id: id.into(),
        public_half: AdminKeyPublicHalf::ed25519_from_bytes(ed_key().verifying_key().as_bytes())
            .unwrap(),
        scopes,
        expires_at: time(end),
    }
}
fn issue(w: &mut JsonlObservationLedger, id: &str, scope: AdminKeyScopeV1, nonce: u64) {
    let r = root_signed(intent(w, issue_action(id, vec![scope], 86400), nonce));
    w.execute_passkey_admin_at(r, time(0)).unwrap();
}
fn command() -> PasskeyAdminActionV1 {
    PasskeyAdminActionV1::Command {
        scope: AdminKeyScopeV1::Status,
        route: "/status".into(),
        body: "exact bytes".into(),
    }
}
fn run(w: &mut JsonlObservationLedger, r: PasskeyAdminRequestV1, at: u64) -> Result<String> {
    w.execute_passkey_command_with_clock(
        r,
        "/status",
        AdminKeyScopeV1::Status,
        || time(at),
        |b| Ok(b.to_owned()),
    )
}

#[test]
fn passkey_day_key_ed25519_only_scoped_commands_and_exact_expiry() {
    let d = tempfile::tempdir().unwrap();
    let mut w = writer(&d.path().join("ledger"));
    issue(&mut w, "day", AdminKeyScopeV1::Status, 1);
    let lease = &w
        .cached_replay()
        .unwrap()
        .state
        .passkey_admin
        .as_ref()
        .unwrap()
        .state
        .day_keys["day"];
    assert!(lease.grant.ml_dsa65_public_key.is_none());
    assert_eq!(lease.grant.expires_at, Some(time(86400)));
    let r = ed_signed(intent(&w, command(), 2), "day");
    assert_eq!(run(&mut w, r, 0).unwrap(), "exact bytes");
    let mut i = intent(&w, command(), 3);
    i.issued_at = time(86390);
    i.expires_at = time(86690);
    assert!(run(&mut w, ed_signed(i, "day"), 86399).is_ok());
    let mut i = intent(&w, command(), 4);
    i.issued_at = time(86400);
    i.expires_at = time(86700);
    assert!(run(&mut w, ed_signed(i, "day"), 86400).is_err());
}

#[test]
fn passkey_day_key_lease_ceiling_scope_and_delegation_are_enforced() {
    let d = tempfile::tempdir().unwrap();
    let mut w = writer(&d.path().join("ledger"));
    for action in [
        issue_action("long", vec![AdminKeyScopeV1::Status], 86401),
        issue_action("empty", vec![], 10),
        issue_action("expired", vec![AdminKeyScopeV1::Status], 0),
        issue_action("startup", vec![AdminKeyScopeV1::StartupGate], 10),
    ] {
        let r = root_signed(intent(&w, action, 1));
        assert!(w.execute_passkey_admin_at(r, time(0)).is_err());
    }
    issue(&mut w, "manager", AdminKeyScopeV1::KeyManage, 1);
    let r = ed_signed(
        intent(
            &w,
            issue_action("delegated", vec![AdminKeyScopeV1::Status], 10),
            2,
        ),
        "manager",
    );
    assert!(w.execute_passkey_admin_at(r, time(0)).is_err());
    let r = ed_signed(intent(&w, command(), 3), "manager");
    assert!(run(&mut w, r, 0).is_err());
    issue(&mut w, "status", AdminKeyScopeV1::Status, 4);
    let r = ed_signed(
        intent(
            &w,
            PasskeyAdminActionV1::RevokeDayKey {
                grant_id: "manager".into(),
            },
            5,
        ),
        "status",
    );
    assert!(w.execute_passkey_admin_at(r, time(0)).is_err());
}

#[test]
fn passkey_day_key_targeted_revocation_root_manager_self_and_restart() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("ledger");
    let mut w = writer(&p);
    for (id, scope, nonce) in [
        ("a", AdminKeyScopeV1::Status, 1),
        ("b", AdminKeyScopeV1::Status, 2),
        ("unrelated", AdminKeyScopeV1::Status, 3),
        ("manager", AdminKeyScopeV1::KeyManage, 4),
    ] {
        issue(&mut w, id, scope, nonce);
    }
    let r = root_signed(intent(
        &w,
        PasskeyAdminActionV1::RevokeDayKey {
            grant_id: "a".into(),
        },
        5,
    ));
    w.execute_passkey_admin_at(r, time(0)).unwrap();
    let r = ed_signed(
        intent(
            &w,
            PasskeyAdminActionV1::RevokeDayKey {
                grant_id: "b".into(),
            },
            6,
        ),
        "manager",
    );
    w.execute_passkey_admin_at(r, time(0)).unwrap();
    let r = ed_signed(
        intent(
            &w,
            PasskeyAdminActionV1::RevokeDayKey {
                grant_id: "manager".into(),
            },
            7,
        ),
        "manager",
    );
    w.execute_passkey_admin_at(r, time(0)).unwrap();
    drop(w);
    clear_ledger_replay_cache(&p, "ledger-a", "mother-a");
    let mut w = JsonlObservationLedger::open_authority(&p, "ledger-a", "mother-a").unwrap();
    for id in ["a", "b", "manager"] {
        assert_eq!(
            w.cached_replay()
                .unwrap()
                .state
                .passkey_admin
                .as_ref()
                .unwrap()
                .state
                .day_keys[id]
                .grant
                .grant_state,
            AdminKeyGrantStateV1::Revoked
        );
    }
    for id in ["a", "b"] {
        let r = ed_signed(intent(&w, command(), 8), id);
        assert!(run(&mut w, r, 0).is_err());
    }
    let r = ed_signed(intent(&w, command(), 9), "unrelated");
    assert!(run(&mut w, r, 0).is_ok());
    let r = root_signed(intent(
        &w,
        issue_action("a", vec![AdminKeyScopeV1::Status], 86400),
        10,
    ));
    assert!(w.execute_passkey_admin_at(r, time(0)).is_err());
    let r = root_signed(intent(
        &w,
        PasskeyAdminActionV1::RevokeDayKey {
            grant_id: root().credential_id,
        },
        11,
    ));
    assert!(w.execute_passkey_admin_at(r, time(0)).is_err());
}

#[test]
fn passkey_day_key_web_authn_binding_checks_use_actual_valid_es256_signatures() {
    let d = tempfile::tempdir().unwrap();
    let mut w = writer(&d.path().join("ledger"));
    let i = intent(
        &w,
        issue_action("day", vec![AdminKeyScopeV1::Status], 86400),
        1,
    );
    for case in 0..10 {
        let r = root_signed_with(i.clone(), |auth, c| match case {
            0 => auth[0] ^= 1,
            1 => auth[32] &= !1,
            2 => auth[32] &= !4,
            3 => c["origin"] = "https://evil.example.test".into(),
            4 => c["challenge"] = "wrong".into(),
            5 => c["type"] = "webauthn.create".into(),
            6 => c["crossOrigin"] = true.into(),
            7 => c["topOrigin"] = "https://other.test".into(),
            8 => auth.push(0),
            9 => auth[32] |= 0x40,
            _ => unreachable!(),
        });
        assert!(
            w.execute_passkey_admin_at(r, time(0)).is_err(),
            "case {case}"
        );
    }
    assert_eq!(w.entries().unwrap().len(), 1);
    w.execute_passkey_admin_at(root_signed(i), time(0)).unwrap();
}

#[test]
fn passkey_day_key_target_snapshot_nonce_body_and_route_cannot_be_substituted() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("ledger");
    let mut w = writer(&p);
    issue(&mut w, "day", AdminKeyScopeV1::Status, 1);
    for case in 0..5 {
        let mut i = intent(&w, command(), 2);
        match case {
            0 => i.mother_node_id = "other".into(),
            1 => i.ledger_id = "other".into(),
            2 => i.epoch = "other".into(),
            3 => i.generation += 1,
            4 => i.authority_observation_id = "other".into(),
            _ => unreachable!(),
        }
        assert!(run(&mut w, ed_signed(i, "day"), 0).is_err());
    }
    let mut r = ed_signed(intent(&w, command(), 2), "day");
    if let PasskeyAdminActionV1::Command { body, .. } = &mut r.intent.action {
        body.push('!');
    }
    assert!(run(&mut w, r, 0).is_err());
    let r = ed_signed(intent(&w, command(), 2), "day");
    assert!(
        w.execute_passkey_command_with_clock(
            r.clone(),
            "/peers/add",
            AdminKeyScopeV1::PeerAdd,
            || time(0),
            |_| -> Result<()> { panic!("wrong route executed") }
        )
        .is_err()
    );
    run(&mut w, r, 0).unwrap();
    drop(w);
    clear_ledger_replay_cache(&p, "ledger-a", "mother-a");
    let mut w = JsonlObservationLedger::open_authority(&p, "ledger-a", "mother-a").unwrap();
    let r = ed_signed(intent(&w, command(), 2), "day");
    assert!(run(&mut w, r, 0).is_err());
    let r = ed_signed(intent(&w, command(), 1), "day");
    assert!(
        run(&mut w, r, 0).is_ok(),
        "nonces are unique per signer, not monotonic"
    );
}

#[test]
fn passkey_genesis_independent_pin_and_virgin_provenance_block_legacy_promotion() {
    let d = tempfile::tempdir().unwrap();
    let mut wrong = root();
    wrong.credential_id = URL_SAFE_NO_PAD.encode([99; 32]);
    let pin = PasskeyGenesisSetupV1::from_registered_passkey(
        "mother-a".into(),
        "ledger-a".into(),
        wrong,
        &startup(),
    )
    .unwrap();
    let mut w =
        JsonlObservationLedger::open(d.path().join("fresh"), "ledger-a", "mother-a").unwrap();
    assert!(
        w.execute_passkey_genesis_at(&pin, birth(), time(0))
            .is_err()
    );
    assert!(w.entries().unwrap().is_empty());
    drop(w);
    let p = d.path().join("legacy");
    let mut w = JsonlObservationLedger::open_authority(&p, "ledger-a", "mother-a").unwrap();
    let before = std::fs::read(&p).unwrap();
    assert!(
        w.execute_passkey_genesis_at(&setup(), birth(), time(0))
            .is_err()
    );
    assert_eq!(std::fs::read(&p).unwrap(), before);
    let i = intent(
        &w,
        issue_action("forged", vec![AdminKeyScopeV1::Status], 10),
        1,
    );
    assert!(w.execute_passkey_admin_at(root_signed(i), time(0)).is_err());
    let mut empty =
        JsonlObservationLedger::open(d.path().join("empty"), "ledger-a", "mother-a").unwrap();
    drop(empty);
    empty = JsonlObservationLedger::open(d.path().join("empty"), "ledger-a", "mother-a").unwrap();
    assert!(
        empty
            .execute_passkey_genesis_at(&setup(), birth(), time(0))
            .is_err()
    );
}

#[test]
fn passkey_day_key_before_effect_durability_and_expiry_are_rechecked() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("ledger");
    let mut w = writer(&p);
    issue(&mut w, "day", AdminKeyScopeV1::Status, 1);
    let r = ed_signed(intent(&w, command(), 2), "day");
    let mut times = [time(0), time(300)].into_iter();
    assert!(
        w.execute_passkey_command_with_clock(
            r,
            "/status",
            AdminKeyScopeV1::Status,
            || times.next().unwrap(),
            |_| -> Result<()> { panic!("expired command executed") }
        )
        .is_err()
    );
    let r = ed_signed(intent(&w, command(), 3), "day");
    w.inject_append_fault_after_for_test(0, TestAppendFault::CompleteFrameBeforeDurabilityAck);
    assert!(
        w.execute_passkey_command_with_clock(
            r,
            "/status",
            AdminKeyScopeV1::Status,
            || time(0),
            |_| -> Result<()> { panic!("unacknowledged admission executed") }
        )
        .is_err()
    );
    drop(w);
    clear_ledger_replay_cache(&p, "ledger-a", "mother-a");
    let mut w = JsonlObservationLedger::open_authority(&p, "ledger-a", "mother-a").unwrap();
    let r = ed_signed(intent(&w, command(), 3), "day");
    assert!(
        run(&mut w, r, 0).is_err(),
        "complete unacknowledged frame consumed nonce"
    );
}

#[test]
fn passkey_browser_challenge_matches_rust_transcript() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../scripts/fixtures/passkey-admin-challenge.json"
    ))
    .unwrap();
    let intent: PasskeyAdminIntentV1 = serde_json::from_value(fixture["intent"].clone()).unwrap();
    assert_eq!(
        webauthn_admin_challenge(&intent, fixture["root"]["credential_id"].as_str().unwrap())
            .unwrap(),
        fixture["challenge"].as_str().unwrap()
    );
}

#[test]
fn passkey_cose_parser_rejects_duplicate_wrong_curve_trailing_and_truncated_keys() {
    let sec1 = root_key().verifying_key().to_sec1_bytes();
    let mut cose = vec![0xa5, 1, 2, 3, 0x26, 0x20, 1, 0x21, 0x58, 0x20];
    cose.extend_from_slice(&sec1[1..33]);
    cose.extend([0x22, 0x58, 0x20]);
    cose.extend_from_slice(&sec1[33..]);
    assert_eq!(
        WebAuthnP256DayKeyV1::sec1_from_cose(&cose).unwrap(),
        root().public_key_sec1
    );
    for case in 0..4 {
        let mut bad = cose.clone();
        match case {
            0 => bad[6] = 2,
            1 => bad[42] = 0x21,
            2 => bad.push(0),
            3 => {
                bad.pop();
            }
            _ => unreachable!(),
        }
        assert!(WebAuthnP256DayKeyV1::sec1_from_cose(&bad).is_err());
    }
}

#[test]
fn passkey_genesis_creation_rejects_untrusted_setup_before_creating_storage() {
    let d = tempfile::tempdir().unwrap();
    for field in [
        "mother",
        "ledger",
        "inventory",
        "root",
        "epoch",
        "generation",
    ] {
        let p = d.path().join(field);
        let mut i = birth().intent;
        match field {
            "mother" => i.mother_node_id = "wrong-mother".into(),
            "ledger" => i.ledger_id = "wrong-ledger".into(),
            "inventory" => {
                let PasskeyAdminActionV1::Bootstrap { inventory_hash, .. } = &mut i.action else {
                    unreachable!()
                };
                *inventory_hash = "cd".repeat(32);
            }
            "root" => {
                let PasskeyAdminActionV1::Bootstrap { root, .. } = &mut i.action else {
                    unreachable!()
                };
                root.credential_id = URL_SAFE_NO_PAD.encode([99; 32]);
            }
            "epoch" => i.epoch = "not-a-birth-epoch".into(),
            "generation" => i.generation = 1,
            _ => unreachable!(),
        }
        assert!(
            JsonlObservationLedger::create_passkey_genesis_at(
                &p,
                &setup(),
                root_signed(i),
                time(0)
            )
            .is_err(),
            "{field} substitution was admitted"
        );
        assert!(!p.exists(), "{field} rejection created storage");
        JsonlObservationLedger::create_passkey_genesis_at(&p, &setup(), birth(), time(0)).unwrap();
    }
}

#[test]
fn passkey_genesis_creation_preserves_existing_empty_partial_and_legacy_ledgers() {
    let d = tempfile::tempdir().unwrap();
    for (name, bytes) in [
        ("empty", b"".as_slice()),
        ("partial", b"partial genesis".as_slice()),
        ("malformed", b"{}\n".as_slice()),
        ("legacy", b"".as_slice()),
    ] {
        let p = d.path().join(name);
        if name == "legacy" {
            drop(JsonlObservationLedger::open_authority(&p, "ledger-a", "mother-a").unwrap());
        } else {
            private_fs::write_atomic(&p, bytes).unwrap();
        }
        let before = std::fs::read(&p).unwrap();
        let error =
            JsonlObservationLedger::create_passkey_genesis_at(&p, &setup(), birth(), time(0))
                .unwrap_err();
        assert!(matches!(error, ObservationLedgerError::Io { source, .. }
            if source.kind() == std::io::ErrorKind::AlreadyExists));
        assert_eq!(std::fs::read(&p).unwrap(), before, "{name} was changed");
    }
}

#[test]
fn passkey_genesis_complete_ack_loss_commits_but_partial_birth_never_reinitializes() {
    for fault in [
        TestAppendFault::PartialFrame,
        TestAppendFault::CompleteFrameBeforeDurabilityAck,
        TestAppendFault::CompleteFrameAfterDurabilityAck,
    ] {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("ledger");
        let mut w = JsonlObservationLedger::open(&p, "ledger-a", "mother-a").unwrap();
        w.inject_append_fault_after_for_test(0, fault);
        assert!(
            w.execute_passkey_genesis_at(&setup(), birth(), time(0))
                .is_err()
        );
        drop(w);
        clear_ledger_replay_cache(&p, "ledger-a", "mother-a");
        let mut w = JsonlObservationLedger::open(&p, "ledger-a", "mother-a").unwrap();
        if fault == TestAppendFault::PartialFrame {
            assert!(w.cached_replay().unwrap().state.passkey_admin.is_none());
            assert!(
                w.execute_passkey_genesis_at(&setup(), birth(), time(0))
                    .is_err()
            );
        } else {
            assert!(w.cached_replay().unwrap().state.passkey_admin.is_some());
            let before = std::fs::read(&p).unwrap();
            w.execute_passkey_genesis_at(&setup(), birth(), time(86400))
                .unwrap();
            assert_eq!(std::fs::read(&p).unwrap(), before);
        }
    }
}

#[test]
fn passkey_profile_rejects_ordinary_and_hybrid_mutation_bypasses() {
    let d = tempfile::tempdir().unwrap();
    let mut w = writer(&d.path().join("ledger"));
    let mut state = w.cached_replay().unwrap().state.clone();
    let before = state.clone();
    assert!(apply_authority_changes(&mut state, &[]).is_err());
    assert_eq!(state, before);
    let fact = PasskeyAdminFactV1 {
        request: birth(),
        decided_at: time(0),
    };
    let mut obs = w.entries().unwrap()[0].observation.clone();
    obs.detail_ref = Some(encode_authority_fact("passkey_admin", "forged", &fact).unwrap());
    assert!(w.append_before_effect(obs, time(0).to_string()).is_err());
}

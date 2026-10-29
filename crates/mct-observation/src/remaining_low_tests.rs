use super::*;

fn legacy_request(id: &str, changes: Vec<AuthorityChangeV1>) -> AuthorityMutationRequestV1 {
    AuthorityMutationRequestV1 {
        mutation_id: id.into(),
        changes,
        decided_at: "2026-10-25T00:00:00Z".into(),
        grant_shaping_sources: vec![GrantShapingSourceV1::AdminKey {
            public_half: AdminHybridSigningKey::from_seeds(&[1; 32], &[2; 32]).public_half(),
        }],
    }
}
fn committed(result: AuthorityMutationResultV1) {
    assert!(
        matches!(
            result,
            AuthorityMutationResultV1::Committed { .. }
                | AuthorityMutationResultV1::CommittedProjectionPending { .. }
        ),
        "{result:?}"
    );
}
fn signed_ticket(grant: &AdminKeyGrantV1, ticket: &str, nonce: u64) -> AuthorityChangeV1 {
    let mut change = AuthorityChangeV1::PairingTicketIssued {
        ticket_hash: blake3::hash(ticket.as_bytes()).to_hex().to_string(),
        peer_endpoint_id: Some("fixture-peer".into()),
        window_starts_at: Some(Timestamp::new("2026-10-25T00:00:00Z").unwrap()),
        window_expires_at: Some(Timestamp::new("2026-10-25T01:00:00Z").unwrap()),
        signature: None,
    };
    let message =
        admin_change_signing_message("mother-low", "ledger-low", nonce, grant, &change).unwrap();
    let AuthorityChangeV1::PairingTicketIssued { signature, .. } = &mut change else {
        unreachable!()
    };
    *signature = Some(AdminSignatureV1 {
        scheme: AdminSignatureSchemeV1::Ed25519MlDsa65V1,
        signer_grant_id: grant.grant_id.clone(),
        nonce,
        signature: AdminHybridSigningKey::from_seeds(&[1; 32], &[2; 32]).sign_hex(&message),
    });
    change
}

#[test]
fn l10_legacy_committed_nonces_survive_epoch_changes_without_monotonic_ordering() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger");
    let key = AdminHybridSigningKey::from_seeds(&[1; 32], &[2; 32]);
    let grant = AdminKeyGrantV1 {
        grant_id: "legacy-signer".into(),
        public_half: key.public_half(),
        ml_dsa65_public_key: Some(key.ml_dsa65_public_key()),
        class: AdminKeyGrantClassV1::DayToDay,
        scopes: vec![AdminKeyScopeV1::KeyManage],
        expires_at: Some(Timestamp::new("2026-10-26T00:00:00Z").unwrap()),
        grant_state: AdminKeyGrantStateV1::Active,
        issuance_ceiling: None,
    };
    let mut writer =
        JsonlObservationLedger::open_authority(&path, "ledger-low", "mother-low").unwrap();
    // Historical legacy fixture, not a D-57 bootstrap or a live admission path.
    committed(writer.execute_authority_mutation(
        legacy_request(
            "seed",
            vec![AuthorityChangeV1::AdminKeyGrantPut {
                grant: Box::new(grant.clone()),
                signature: None,
            }],
        ),
        |_| Ok(None),
    ));
    let original = legacy_request("original", vec![signed_ticket(&grant, "ticket-a", 40)]);
    committed(writer.execute_authority_mutation(original.clone(), |_| Ok(None)));
    let prior = writer.verified_replay().unwrap().current_authority.clone();
    drop(writer);
    let mut writer =
        JsonlObservationLedger::open_authority(&path, "ledger-low", "mother-low").unwrap();
    assert_ne!(writer.verified_replay().unwrap().current_authority, prior);
    let before = std::fs::read(&path).unwrap();
    // Exact mutation retry resolves the old fact, never makes another commit.
    committed(writer.execute_authority_mutation(original, |_| Ok(None)));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let reused = legacy_request("new-id", vec![signed_ticket(&grant, "ticket-b", 40)]);
    assert!(matches!(
        writer.execute_authority_mutation(reused, |_| panic!("replay cannot apply")),
        AuthorityMutationResultV1::RejectedBeforeCommit { .. }
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    // Unused lower values are valid. Changing this to a high-water counter would
    // change the contract, not fix a replay of a committed command.
    committed(writer.execute_authority_mutation(
        legacy_request("lower-unused", vec![signed_ticket(&grant, "ticket-b", 3)]),
        |_| Ok(None),
    ));
    let replay = replay_authority_entries(&writer.entries().unwrap()).unwrap();
    assert!(
        replay
            .admin_signature_nonces
            .contains(&("legacy-signer".into(), 40))
    );
    assert!(
        replay
            .admin_signature_nonces
            .contains(&("legacy-signer".into(), 3))
    );
    let mut changed_grant = grant.clone();
    changed_grant.scopes = vec![AdminKeyScopeV1::Status];
    let change = signed_ticket(&grant, "held-command", 41);
    assert!(
        verify_admin_change_signatures(
            &replay.state,
            &replay.admin_signature_nonces,
            "mother-low",
            "ledger-low",
            std::slice::from_ref(&change)
        )
        .is_ok()
    );
    let mut changed_state = replay.state.clone();
    changed_state
        .admin_key_grants
        .insert(grant.grant_id.clone(), changed_grant);
    assert!(
        verify_admin_change_signatures(
            &changed_state,
            &replay.admin_signature_nonces,
            "mother-low",
            "ledger-low",
            std::slice::from_ref(&change)
        )
        .is_err()
    );
    assert!(
        verify_admin_change_signatures(
            &replay.state,
            &BTreeSet::new(),
            "other-mother",
            "ledger-low",
            std::slice::from_ref(&change)
        )
        .is_err()
    );
}

fn ordinary(id: &str) -> MctObservation {
    MctObservation::informational(
        ObservationId::new(id).unwrap(),
        Timestamp::new("2026-10-25T00:00:00Z").unwrap(),
        ObservationKind::LifecycleTransitionRecorded,
        TraceId::new("trace-low").unwrap(),
        "representation café",
    )
}

fn chain(path: &Path) -> Vec<MctObservationLedgerEntry> {
    let mut writer = JsonlObservationLedger::open(path, "ledger-low", "mother-low").unwrap();
    for id in ["one", "two", "three"] {
        writer
            .append_before_effect(ordinary(id), "2026-10-25T00:00:00Z")
            .unwrap();
    }
    writer.entries().unwrap()
}

#[test]
fn l13_cached_checkpoint_rejects_valid_truncation_and_repeated_attempts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger");
    let entries = chain(&path);
    load_verified_ledger_replay(&path, "ledger-low", "mother-low").unwrap();
    let mut shorter = serde_json::to_vec(&entries[0]).unwrap();
    shorter.push(b'\n');
    std::fs::write(&path, &shorter).unwrap();
    for _ in 0..2 {
        assert!(
            load_verified_ledger_replay(&path, "ledger-low", "mother-low").is_err(),
            "a valid shorter prefix is rollback, not permission to discard a known checkpoint"
        );
        assert_eq!(std::fs::read(&path).unwrap(), shorter);
    }
    assert!(JsonlObservationLedger::open(&path, "ledger-low", "mother-low").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), shorter);
    std::fs::write(&path, []).unwrap();
    assert!(load_verified_ledger_replay(&path, "ledger-low", "mother-low").is_err());
    assert!(std::fs::read(&path).unwrap().is_empty());
}

#[test]
fn l13_valid_rechained_fork_and_sequence_gap_are_not_continuity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger");
    let mut entries = chain(&path);
    load_verified_ledger_replay(&path, "ledger-low", "mother-low").unwrap();
    entries[0].observation.safe_message = "replaced history".into();
    let mut previous = None;
    let mut bytes = Vec::new();
    for entry in &mut entries {
        entry.previous_entry_hash = previous;
        entry.entry_hash = entry_hash(entry).unwrap();
        previous = Some(entry.entry_hash.clone());
        bytes.extend(serde_json::to_vec(entry).unwrap());
        bytes.push(b'\n');
    }
    // A hash chain alone authenticates no external continuity anchor.
    let cold = dir.path().join("cold-copy");
    private_fs::write_atomic(&cold, &bytes).unwrap();
    verify_ledger_streaming(&cold, "ledger-low", "mother-low").unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert!(load_verified_ledger_replay(&path, "ledger-low", "mother-low").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    entries[1].local_sequence += 1;
    let gap = dir.path().join("gap");
    let bytes = entries
        .iter()
        .flat_map(|e| {
            let mut b = serde_json::to_vec(e).unwrap();
            b.push(b'\n');
            b
        })
        .collect::<Vec<_>>();
    private_fs::write_atomic(&gap, &bytes).unwrap();
    assert!(verify_ledger_streaming(&gap, "ledger-low", "mother-low").is_err());
    assert_eq!(std::fs::read(&gap).unwrap(), bytes);
}

#[test]
fn l13_checkpoint_survives_partial_committed_tail_and_stale_cache_publication() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger");
    let mut writer = JsonlObservationLedger::open(&path, "ledger-low", "mother-low").unwrap();
    writer
        .append_before_effect(ordinary("one"), "2026-10-25T00:00:00Z")
        .unwrap();
    let old = writer.replay_checkpoint().unwrap();
    writer
        .append_before_effect(ordinary("two"), "2026-10-25T00:00:00Z")
        .unwrap();
    let known = writer.replay_checkpoint().unwrap();
    drop(writer);
    let key = replay_cache_key(&path, "ledger-low", "mother-low");
    replay_cache_store(key.clone(), known.clone()).unwrap();
    assert!(replay_cache_store(key.clone(), old).is_err());
    let mut fork = known.clone();
    fork.through_entry_hash = "0".repeat(64);
    assert!(replay_cache_store(key.clone(), fork).is_err());
    assert_eq!(
        replay_cache_head(&key),
        Some((known.through_sequence, known.through_entry_hash))
    );
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.truncate(bytes.len() - 20);
    std::fs::write(&path, &bytes).unwrap();
    for _ in 0..2 {
        assert!(matches!(
            JsonlObservationLedger::open(&path, "ledger-low", "mother-low"),
            Err(ObservationLedgerError::LedgerDiscontinuity { .. })
        ));
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "a previously committed frame is not disposable crash residue"
        );
    }
}

#[test]
fn l13_historical_entry_hash_is_typed_representation_not_raw_json_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger");
    let entries = chain(&path);
    assert_eq!(
        entries[0].entry_hash,
        "8bcddc16cc8514e497c45c1e97aa67d12cafc2878d3fbe4978a48e2137d941e1"
    );
    let original = std::fs::read(&path).unwrap();
    load_verified_ledger_replay(&path, "ledger-low", "mother-low").unwrap();
    let mut represented = Vec::new();
    for entry in &entries {
        // Object key ordering, whitespace and Unicode escapes do not change typed fields.
        let mut value = serde_json::to_value(entry).unwrap();
        // Historical serde permits unknown fields; they are not authenticated
        // claims and no consumer may interpret them as new authority.
        value.as_object_mut().unwrap().insert(
            "ignored_extension".into(),
            serde_json::json!("not authority"),
        );
        let line = serde_json::to_string(&value)
            .unwrap()
            .replace("café", "caf\\u00e9")
            .replace(',', ", ");
        represented.extend(line.as_bytes());
        represented.push(b'\n');
    }
    assert_ne!(represented, original);
    std::fs::write(&path, &represented).unwrap();
    let verified = load_verified_ledger_replay(&path, "ledger-low", "mother-low").unwrap();
    assert_eq!(verified.head.unwrap().entry_hash, entries[2].entry_hash);
    assert_ne!(blake3::hash(&original), blake3::hash(&represented));
    let tampered = String::from_utf8(represented)
        .unwrap()
        .replace("representation", "tampered");
    std::fs::write(&path, &tampered).unwrap();
    assert!(load_verified_ledger_replay(&path, "ledger-low", "mother-low").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), tampered.as_bytes());
}

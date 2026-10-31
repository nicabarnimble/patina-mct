use super::*;
use mct_kernel::recovery::RecoveryPublicKeyV1;

fn key(seed: u8) -> AdminHybridSigningKey {
    AdminHybridSigningKey::from_seeds(&[seed; 32], &[seed + 64; 32])
}
fn public(seed: u8) -> RecoveryPublicKeyV1 {
    RecoveryPublicKeyV1 {
        ed25519: key(seed).public_half(),
        ml_dsa65: key(seed).ml_dsa65_public_key(),
    }
}
fn time(seconds: u64) -> Timestamp {
    let base: jiff::Timestamp = "2026-10-19T00:00:00Z".parse().unwrap();
    Timestamp::new(
        base.checked_add(std::time::Duration::from_secs(seconds))
            .unwrap()
            .to_string(),
    )
    .unwrap()
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
pub(super) fn setup() -> AdminGenesisSetupV1 {
    AdminGenesisSetupV1::from_verified_virgin_startup(
        "mother-a".into(),
        "ledger-a".into(),
        public(2),
        &startup(),
    )
    .unwrap()
}
fn signed(mut request: AdminGenesisRequestV1) -> AdminGenesisRequestV1 {
    request.signature.signature = key(2).sign_hex(&request.signing_message().unwrap());
    request
}
fn request() -> AdminGenesisRequestV1 {
    signed(AdminGenesisRequestV1 {
        intent: AdminGenesisIntentV1 {
            bootstrap_id: "birth-a".into(),
            mother_node_id: "mother-a".into(),
            ledger_id: "ledger-a".into(),
            birth_epoch: format!("{AUTHORITY_EPOCH_PREFIX_V1}{}", "ac".repeat(32)),
            inventory_hash: startup().inventory_hash,
            root: AdminKeyGrantV1 {
                grant_id: "root".into(),
                public_half: public(2).ed25519,
                ml_dsa65_public_key: Some(public(2).ml_dsa65),
                class: AdminKeyGrantClassV1::Recovery,
                scopes: mct_kernel::recovery_grant_scopes().to_vec(),
                expires_at: None,
                grant_state: AdminKeyGrantStateV1::Active,
                issuance_ceiling: None,
            },
        },
        signature: AdminSignatureV1 {
            scheme: AdminSignatureSchemeV1::Ed25519MlDsa65V1,
            signer_grant_id: "root".into(),
            nonce: 0,
            signature: String::new(),
        },
    })
}
pub(super) fn bootstrap_writer(
    path: &Path,
) -> (
    JsonlObservationLedger,
    AdminGenesisSetupV1,
    AdminGenesisRequestV1,
) {
    let mut writer = JsonlObservationLedger::open(path, "ledger-a", "mother-a").unwrap();
    let setup = setup();
    let request = request();
    writer
        .execute_admin_genesis_at(&setup, request.clone(), time(0))
        .unwrap();
    writer.begin_authority_tenure().unwrap();
    (writer, setup, request)
}
pub(super) fn leaf_request(writer: &JsonlObservationLedger, nonce: u64) -> AdminKeyIssueRequestV1 {
    let replay = writer.cached_replay().unwrap();
    let mut request = AdminKeyIssueRequestV1 {
        intent: AdminKeyIssueIntentV1 {
            issuance_id: "issue-leaf".into(),
            ledger_id: "ledger-a".into(),
            expected_authority: AuthorityStateReferenceV1 {
                grants_authority: replay.current_authority.clone().unwrap(),
                authority_state_hash: authority_state_hash(&replay.state).unwrap(),
            },
            issuer_grant_id: "root".into(),
            grant: AdminKeyGrantV1 {
                grant_id: "leaf".into(),
                public_half: public(3).ed25519,
                ml_dsa65_public_key: Some(public(3).ml_dsa65),
                class: AdminKeyGrantClassV1::DayToDay,
                scopes: vec![AdminKeyScopeV1::Status],
                expires_at: Some(time(100)),
                grant_state: AdminKeyGrantStateV1::Active,
                issuance_ceiling: None,
            },
        },
        signature: AdminSignatureV1 {
            scheme: AdminSignatureSchemeV1::Ed25519MlDsa65V1,
            signer_grant_id: "root".into(),
            nonce,
            signature: String::new(),
        },
    };
    let change = AuthorityChangeV1::AdminKeyIssue {
        request: Box::new(request.clone()),
        decided_at: time(0),
    };
    let message = admin_change_signing_message(
        "mother-a",
        "ledger-a",
        nonce,
        &replay.state.admin_key_grants["root"],
        &change,
    )
    .unwrap();
    request.signature.signature = key(2).sign_hex(&message);
    request
}
fn committed(result: AuthorityMutationResultV1) {
    assert!(
        matches!(
            result,
            AuthorityMutationResultV1::CommittedProjectionPending { .. }
                | AuthorityMutationResultV1::Committed { .. }
        ),
        "{result:?}"
    );
}
fn denied(result: AuthorityMutationResultV1) {
    assert!(
        matches!(
            result,
            AuthorityMutationResultV1::RejectedBeforeCommit { .. }
        ),
        "{result:?}"
    );
}

#[test]
fn admin_genesis_atomic_birth_sets_root_epoch_nonce_and_distinct_hash_domain() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger");
    let mut writer = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
    let receipt = writer
        .execute_admin_genesis_at(&setup(), request(), time(0))
        .unwrap();
    assert_eq!(receipt.local_sequence, 0);
    assert_eq!(writer.entries().unwrap().len(), 1);
    let replay = writer.cached_replay().unwrap();
    assert_eq!(replay.current_authority.as_ref().unwrap().generation, 1);
    assert!(replay.admin_signature_nonces.contains(&("root".into(), 0)));
    assert!(replay.admin_grant_ids.contains("root"));
    let mut legacy = replay.state.clone();
    legacy.admin_genesis = None;
    assert_ne!(
        authority_state_hash(&replay.state).unwrap(),
        authority_state_hash(&legacy).unwrap()
    );
    assert_eq!(
        replay_authority_entries(&writer.entries().unwrap()).unwrap(),
        *replay
    );
    let state = replay.state.clone();
    drop(writer);
    let writer = JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
    assert_eq!(writer.cached_replay().unwrap().state, state);
    assert_ne!(
        writer
            .cached_replay()
            .unwrap()
            .current_authority
            .as_ref()
            .unwrap()
            .authority_epoch,
        request().intent.birth_epoch
    );
}

#[test]
fn admin_genesis_pin_is_separate_from_self_signed_request_including_each_key_half() {
    for which in 0..4 {
        let dir = tempfile::tempdir().unwrap();
        let mut writer =
            JsonlObservationLedger::open(dir.path().join("ledger"), "ledger-a", "mother-a")
                .unwrap();
        let mut pin = public(2);
        if which == 0 {
            pin.ed25519 = public(3).ed25519;
        }
        if which == 1 {
            pin.ml_dsa65 = public(3).ml_dsa65;
        }
        let setup = AdminGenesisSetupV1::from_verified_virgin_startup(
            if which == 2 { "other" } else { "mother-a" }.into(),
            if which == 3 { "other" } else { "ledger-a" }.into(),
            pin,
            &startup(),
        )
        .unwrap();
        assert!(
            writer
                .execute_admin_genesis_at(&setup, request(), time(0))
                .is_err()
        );
        assert!(writer.entries().unwrap().is_empty());
    }
}

#[test]
fn admin_genesis_requires_exact_target_inventory_and_strict_root_shape() {
    for which in 0..10 {
        let dir = tempfile::tempdir().unwrap();
        let mut writer =
            JsonlObservationLedger::open(dir.path().join("ledger"), "ledger-a", "mother-a")
                .unwrap();
        let mut bad = request();
        match which {
            0 => bad.intent.mother_node_id = "other".into(),
            1 => bad.intent.ledger_id = "other".into(),
            2 => bad.intent.inventory_hash = "cd".repeat(32),
            3 => bad.intent.root.ml_dsa65_public_key = None,
            4 => bad.intent.root.scopes = vec![AdminKeyScopeV1::Status],
            5 => bad.intent.root.grant_state = AdminKeyGrantStateV1::Revoked,
            6 => bad.intent.root.grant_id = "recovered:owner-a:2".into(),
            7 => bad.intent.birth_epoch = "not-an-epoch".into(),
            8 => bad.intent.root.expires_at = Some(time(10)),
            _ => bad.intent.bootstrap_id.clear(),
        }
        assert!(
            writer
                .execute_admin_genesis_at(&setup(), signed(bad), time(0))
                .is_err()
        );
        assert!(writer.entries().unwrap().is_empty());
    }
}

#[test]
fn admin_genesis_both_halves_signer_nonce_and_domain_are_required() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer =
        JsonlObservationLedger::open(dir.path().join("ledger"), "ledger-a", "mother-a").unwrap();
    for which in 0..6 {
        let mut bad = request();
        match which {
            0 | 1 => {
                let start = if which == 0 { 4 } else { 200 };
                let flip = if &bad.signature.signature[start..start + 2] == "ff" {
                    "00"
                } else {
                    "ff"
                };
                bad.signature
                    .signature
                    .replace_range(start..start + 2, flip);
            }
            2 => bad.signature.signature.truncate(130),
            3 => bad.signature.nonce = 1,
            4 => bad.signature.signer_grant_id = "other".into(),
            _ => bad.signature.signature = key(2).sign_hex(b"different-purpose"),
        }
        assert!(
            writer
                .execute_admin_genesis_at(&setup(), bad, time(0))
                .is_err()
        );
    }
    assert!(writer.entries().unwrap().is_empty());
}

#[test]
fn admin_genesis_setup_refuses_inferred_virgin_operator_gate_and_stale_inventory_state() {
    for which in 0..7 {
        let mut evidence = startup();
        match which {
            0 => evidence.inventory_hash = "legacy-inferred-startup-evidence".into(),
            1 => evidence.startup_class = AuthorityStartupClassV1::OperatorGatedNonvirgin,
            2 => evidence.startup_class = AuthorityStartupClassV1::OrdinaryReopen,
            3 => {
                evidence.expected_predecessor =
                    AuthorityEpochPredecessorV1::NoneAfterOperatorReinitialization
            }
            4 => evidence.expected_authority_state_hash = "00".repeat(32),
            5 => evidence.authenticated_principal_ref = Some("os-uid:501".into()),
            _ => evidence.operator_gate_decision_id = Some("reset".into()),
        }
        assert!(
            AdminGenesisSetupV1::from_verified_virgin_startup(
                "mother-a".into(),
                "ledger-a".into(),
                public(2),
                &evidence
            )
            .is_err()
        );
    }
}

#[test]
fn admin_genesis_unsigned_epoch_cannot_be_promoted_even_with_empty_grants() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer =
        JsonlObservationLedger::open_authority(dir.path().join("ledger"), "ledger-a", "mother-a")
            .unwrap();
    let before = writer.entries().unwrap();
    assert!(
        writer
            .execute_admin_genesis_at(&setup(), request(), time(0))
            .is_err()
    );
    assert_eq!(writer.entries().unwrap(), before);
    assert!(
        replay_authority_entries(&before)
            .unwrap()
            .state
            .admin_genesis
            .is_none()
    );
}

#[test]
fn admin_genesis_replay_rejects_grafting_after_legacy_or_noncanonical_prefix() {
    for epoch in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger");
        let mut writer = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        if epoch {
            writer.begin_authority_tenure().unwrap();
        } else {
            let mut observation = (AdminGenesisFactV1 {
                request: request(),
                decided_at: time(0),
            })
            .observation()
            .unwrap();
            observation.detail_ref = None;
            writer
                .append_before_effect(observation, time(0).to_string())
                .unwrap();
        }
        assert!(
            writer
                .execute_admin_genesis_at(&setup(), request(), time(0))
                .is_err()
        );
        let fact = AdminGenesisFactV1 {
            request: request(),
            decided_at: time(0),
        };
        // Bypass the typed writer deliberately to test independent semantic replay.
        writer
            .append_canonical_before_effect(fact.observation().unwrap(), time(0).to_string())
            .unwrap();
        assert!(replay_authority_entries(&writer.entries().unwrap()).is_err());
    }
}

#[test]
fn admin_genesis_legacy_mutations_and_imports_cannot_mutate_or_install_profile() {
    let dir = tempfile::tempdir().unwrap();
    let (mut writer, _, _) = bootstrap_writer(&dir.path().join("ledger"));
    let state = writer.cached_replay().unwrap().state.clone();
    let change = AuthorityChangeV1::AdminKeyGrantRevoke {
        grant_id: "root".into(),
        signature: None,
    };
    assert!(apply_authority_changes(&mut state.clone(), std::slice::from_ref(&change)).is_err());
    denied(writer.execute_authority_mutation(
        AuthorityMutationRequestV1 {
            mutation_id: "legacy".into(),
            changes: vec![change],
            grant_shaping_sources: vec![],
            decided_at: time(0).to_string(),
        },
        |_| panic!("must not project"),
    ));
    for imported in [AuthorityStateV1::default(), state.clone()] {
        let hash = authority_state_hash(&imported).unwrap();
        let import = LegacyAuthorityImportRequestV1 {
            schema: "mct-legacy-authority-import-request/v1".into(),
            import_id: "import".into(),
            expected_mother_node_id: "mother-a".into(),
            expected_ledger_id: "ledger-a".into(),
            expected_config_authority_hash: hash.clone(),
            expected_sqlite_authority_hash: hash,
            confirmation: LEGACY_AUTHORITY_IMPORT_CONFIRMATION_V1.into(),
        };
        denied(writer.execute_legacy_authority_import(
            import.clone(),
            "os-uid:501".into(),
            imported.clone(),
            time(0).to_string(),
        ));
        if imported.admin_genesis.is_some() {
            let legacy_dir = tempfile::tempdir().unwrap();
            let mut legacy = JsonlObservationLedger::open_authority(
                legacy_dir.path().join("ledger"),
                "ledger-a",
                "mother-a",
            )
            .unwrap();
            denied(legacy.execute_legacy_authority_import(
                import,
                "os-uid:501".into(),
                imported,
                time(0).to_string(),
            ));
        }
    }
    assert_eq!(writer.cached_replay().unwrap().state, state);
}

#[test]
fn admin_genesis_current_root_issues_under_d56_and_birth_nonce_stays_consumed() {
    let dir = tempfile::tempdir().unwrap();
    let (mut writer, _, _) = bootstrap_writer(&dir.path().join("ledger"));
    let reused = leaf_request(&writer, 0);
    denied(writer.execute_admin_key_issuance_at(reused, time(0)));
    let allowed = leaf_request(&writer, 1);
    committed(writer.execute_admin_key_issuance_at(allowed, time(0)));
    let replay = replay_authority_entries(&writer.entries().unwrap()).unwrap();
    assert!(replay.state.admin_genesis.is_some());
    assert_eq!(replay.state.admin_issuance.len(), 1);
}

#[test]
fn admin_genesis_exact_retry_is_historical_and_changes_conflict_after_tenure_and_issuance() {
    let dir = tempfile::tempdir().unwrap();
    let (mut writer, setup, request) = bootstrap_writer(&dir.path().join("ledger"));
    let issue = leaf_request(&writer, 1);
    committed(writer.execute_admin_key_issuance_at(issue, time(0)));
    let before = writer.entries().unwrap();
    assert_eq!(
        writer
            .execute_admin_genesis_at(&setup, request.clone(), time(1000))
            .unwrap(),
        before[0]
    );
    let mut changed = request;
    changed.intent.bootstrap_id = "other".into();
    assert!(
        writer
            .execute_admin_genesis_at(&setup, signed(changed), time(1000))
            .is_err()
    );
    assert_eq!(writer.entries().unwrap(), before);
}

#[test]
fn admin_genesis_partial_and_complete_frame_crashes_never_create_a_second_origin() {
    for fault in [
        TestAppendFault::PartialFrame,
        TestAppendFault::CompleteFrameBeforeDurabilityAck,
        TestAppendFault::CompleteFrameAfterDurabilityAck,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger");
        let mut writer = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        writer.inject_append_fault_after_for_test(0, fault);
        assert!(matches!(
            writer.execute_admin_genesis_at(&setup(), request(), time(0)),
            Err(ObservationLedgerError::AppendCommitUnknown { .. })
        ));
        assert!(writer.is_poisoned());
        assert!(
            writer
                .execute_admin_genesis_at(&setup(), request(), time(0))
                .is_err()
        );
        drop(writer);
        let mut writer = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        if fault == TestAppendFault::PartialFrame {
            assert!(writer.recovery_status().is_some());
            assert!(
                writer
                    .cached_replay()
                    .unwrap()
                    .state
                    .admin_genesis
                    .is_none()
            );
            assert!(
                writer
                    .execute_admin_genesis_at(&setup(), request(), time(0))
                    .is_err()
            );
        } else {
            let receipt = writer
                .execute_admin_genesis_at(&setup(), request(), time(1000))
                .unwrap();
            assert_eq!(receipt.local_sequence, 0);
            assert_eq!(writer.entries().unwrap().len(), 1);
        }
    }
}

#[test]
fn admin_genesis_replay_rejects_carrier_tampering_and_birth_epoch_reuse() {
    let dir = tempfile::tempdir().unwrap();
    let (writer, _, _) = bootstrap_writer(&dir.path().join("ledger"));
    for which in 0..4 {
        let mut entries = writer.entries().unwrap();
        entries.truncate(1);
        match which {
            0 => entries[0].observation.subject_id = Some("other".into()),
            1 => entries[0].durability_class = DurabilityClass::Buffered,
            2 => entries[0].appended_at = time(1).to_string(),
            _ => entries[0].observation.outcome = ObservationOutcome::Completed,
        }
        entries[0].entry_hash = entry_hash(&entries[0]).unwrap();
        assert!(replay_authority_entries(&entries).is_err());
    }
    let mut entries = writer.entries().unwrap();
    let envelope: CanonicalAuthorityEnvelopeV1 = serde_json::from_str(
        entries[1]
            .observation
            .detail_ref
            .as_ref()
            .unwrap()
            .strip_prefix(AUTHORITY_FACT_DETAIL_PREFIX)
            .unwrap(),
    )
    .unwrap();
    let mut epoch: EpochEstablishedFactV1 = serde_json::from_value(envelope.body).unwrap();
    epoch.authority_epoch = request().intent.birth_epoch;
    epoch.resulting_authority.authority_epoch = epoch.authority_epoch.clone();
    entries[1].observation.detail_ref = Some(encode_epoch_fact(&envelope.fact_id, &epoch).unwrap());
    entries[1].entry_hash = entry_hash(&entries[1]).unwrap();
    assert!(replay_authority_entries(&entries).is_err());
}

#[test]
fn admin_genesis_hash_denies_missing_or_replaced_root_without_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let (writer, _, _) = bootstrap_writer(&dir.path().join("ledger"));
    let mut state = writer.cached_replay().unwrap().state.clone();
    state.admin_key_grants.clear();
    assert!(authority_state_hash(&state).is_err());
    let mut state = writer.cached_replay().unwrap().state.clone();
    state.admin_key_grants.get_mut("root").unwrap().public_half = public(3).ed25519;
    assert!(authority_state_hash(&state).is_err());
}

#[test]
fn admin_genesis_birth_time_fences_backdated_issuance() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer =
        JsonlObservationLedger::open(dir.path().join("ledger"), "ledger-a", "mother-a").unwrap();
    writer
        .execute_admin_genesis_at(&setup(), request(), time(10))
        .unwrap();
    writer.begin_authority_tenure().unwrap();
    let issue = leaf_request(&writer, 1);
    denied(writer.execute_admin_key_issuance_at(issue.clone(), time(9)));
    committed(writer.execute_admin_key_issuance_at(issue, time(10)));
}

#[test]
fn admin_genesis_existing_zero_byte_file_is_not_virgin_even_with_asserted_setup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger");
    let writer = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
    assert!(writer.created_new);
    drop(writer);
    let mut reopened = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
    assert!(!reopened.created_new);
    assert!(
        reopened
            .execute_admin_genesis_at(&setup(), request(), time(0))
            .is_err()
    );
    assert!(std::fs::read(&path).unwrap().is_empty());
}

#[test]
fn admin_genesis_wire_is_strict_and_legacy_state_omits_profile() {
    let mut value = serde_json::to_value(request()).unwrap();
    value["intent"]["trust_me"] = serde_json::json!(true);
    assert!(serde_json::from_value::<AdminGenesisRequestV1>(value).is_err());
    assert!(
        !serde_json::to_string(&AuthorityStateV1::default())
            .unwrap()
            .contains("admin_genesis")
    );
}

#[test]
fn admin_genesis_corpus_roundtrip_and_explicit_export() {
    let dir = tempfile::tempdir().unwrap();
    let (mut writer, _, request) = bootstrap_writer(&dir.path().join("ledger"));
    let genesis = writer.entries().unwrap()[..1].to_vec();
    let issue = leaf_request(&writer, 1);
    committed(writer.execute_admin_key_issuance_at(issue, time(0)));
    let entries = writer.entries().unwrap();
    assert_eq!(
        replay_authority_entries(&entries).unwrap().state,
        writer.cached_replay().unwrap().state
    );
    if let Ok(output) = std::env::var("MCT_GENESIS_CORPUS_OUT") {
        std::fs::create_dir_all(&output).unwrap();
        for (name, bytes) in [
            (
                "canonical-prefix.json",
                serde_json::to_vec(&entries).unwrap(),
            ),
            ("genesis-only.json", serde_json::to_vec(&genesis).unwrap()),
            ("request.json", serde_json::to_vec(&request).unwrap()),
        ] {
            std::fs::write(Path::new(&output).join(name), bytes).unwrap();
        }
    }
}

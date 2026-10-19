use super::*;
use AdminKeyScopeV1 as Scope;
use mct_kernel::AdminIssuanceCeilingV1;

fn grant(
    id: &str,
    seed: u8,
    scopes: Vec<Scope>,
    ceiling: Option<Vec<Scope>>,
    expiry: u64,
) -> AdminKeyGrantV1 {
    AdminKeyGrantV1 {
        grant_id: id.into(),
        public_half: public(seed).ed25519,
        ml_dsa65_public_key: Some(public(seed).ml_dsa65),
        class: AdminKeyGrantClassV1::DayToDay,
        scopes,
        expires_at: Some(time(expiry)),
        grant_state: AdminKeyGrantStateV1::Active,
        issuance_ceiling: ceiling.map(|scopes| AdminIssuanceCeilingV1 { scopes }),
    }
}
fn sign_request(request: &mut AdminKeyIssueRequestV1, issuer: &AdminKeyGrantV1, seed: u8) {
    let change = AuthorityChangeV1::AdminKeyIssue {
        request: Box::new(request.clone()),
        decided_at: time(0),
    };
    let bytes = admin_change_signing_message(
        &request
            .intent
            .expected_authority
            .grants_authority
            .mother_node_id,
        &request.intent.ledger_id,
        request.signature.nonce,
        issuer,
        &change,
    )
    .unwrap();
    request.signature.signature = key(seed).sign_hex(&bytes);
}
fn request(
    f: &Fixture,
    id: &str,
    issuer: &str,
    seed: u8,
    recipient: AdminKeyGrantV1,
    nonce: u64,
) -> AdminKeyIssueRequestV1 {
    let replay = f.writer.cached_replay().unwrap();
    let mut request = AdminKeyIssueRequestV1 {
        intent: AdminKeyIssueIntentV1 {
            issuance_id: id.into(),
            ledger_id: "ledger-a".into(),
            expected_authority: AuthorityStateReferenceV1 {
                grants_authority: replay.current_authority.clone().unwrap(),
                authority_state_hash: authority_state_hash(&replay.state).unwrap(),
            },
            issuer_grant_id: issuer.into(),
            grant: recipient,
        },
        signature: AdminSignatureV1 {
            scheme: AdminSignatureSchemeV1::Ed25519MlDsa65V1,
            signer_grant_id: issuer.into(),
            nonce,
            signature: String::new(),
        },
    };
    sign_request(&mut request, &replay.state.admin_key_grants[issuer], seed);
    request
}
fn issue_manager(f: &mut Fixture) -> AdminKeyIssueRequestV1 {
    let request = request(
        f,
        "issue-manager",
        "root",
        2,
        grant(
            "manager",
            50,
            vec![Scope::KeyManage],
            Some(vec![Scope::KeyManage, Scope::Status]),
            2 * RECOVERY_DELAY_MS,
        ),
        1,
    );
    committed(
        f.writer
            .execute_admin_key_issuance_at(request.clone(), time(0)),
    );
    request
}

#[test]
fn admin_issuance_canonical_root_and_bounded_delegation_preserve_distinct_execution_rights() {
    let mut f = Fixture::new();
    issue_manager(&mut f);
    let request = request(
        &f,
        "issue-leaf",
        "manager",
        50,
        grant("leaf", 51, vec![Scope::Status], None, RECOVERY_DELAY_MS),
        1,
    );
    committed(f.writer.execute_admin_key_issuance_at(request, time(0)));
    let state = f.state();
    assert_eq!(state.admin_issuance.len(), 2);
    assert_eq!(
        state.admin_issuance["leaf"].issuer_at_acceptance.grant_id,
        "manager"
    );
    assert_eq!(
        state.admin_key_grants["manager"].scopes,
        vec![Scope::KeyManage]
    );
    assert_eq!(
        state.admin_key_grants["root"].scopes,
        mct_kernel::recovery_grant_scopes()
    );
    assert_eq!(
        replay_authority_entries(&f.writer.entries().unwrap())
            .unwrap()
            .state,
        state
    );
    assert_eq!(f.reopen().state(), state);
}

#[test]
fn admin_issuance_canonical_denies_execution_delegation_expiry_and_root_escalation() {
    let mut f = Fixture::new();
    issue_manager(&mut f);
    let before = f.state();
    for target in [
        grant("bad", 51, vec![Scope::PeerAdd], None, RECOVERY_DELAY_MS),
        grant(
            "bad",
            51,
            vec![Scope::KeyManage],
            Some(vec![Scope::PeerAdd]),
            RECOVERY_DELAY_MS,
        ),
        grant("bad", 51, vec![Scope::KeyManage], None, RECOVERY_DELAY_MS),
        grant("bad", 51, vec![Scope::Status], None, 3 * RECOVERY_DELAY_MS),
        grant("bad", 51, vec![Scope::Status], None, 0),
        before.admin_key_grants["root"].clone(),
    ] {
        let request = request(&f, "bad-command", "manager", 50, target, 1);
        denied(f.writer.execute_admin_key_issuance_at(request, time(0)));
        assert_eq!(f.state(), before);
    }
    // Failed commands consume neither ID nor nonce.
    let valid = request(
        &f,
        "bad-command",
        "manager",
        50,
        grant("leaf", 51, vec![Scope::Status], None, RECOVERY_DELAY_MS),
        1,
    );
    committed(f.writer.execute_admin_key_issuance_at(valid, time(0)));
}

#[test]
fn admin_issuance_never_issues_current_root_credentials_as_day_keys() {
    let mut f = Fixture::new();
    issue_manager(&mut f);
    for classical in [true, false] {
        let mut recipient = grant("alias", 51, vec![Scope::Status], None, RECOVERY_DELAY_MS);
        if classical {
            recipient.public_half = public(2).ed25519;
        } else {
            recipient.ml_dsa65_public_key = Some(public(2).ml_dsa65);
        }
        let request = request(&f, "alias-root", "manager", 50, recipient, 1);
        denied(f.writer.execute_admin_key_issuance_at(request, time(0)));
    }
}

#[test]
fn admin_issuance_requires_actual_issuer_not_recipient_self_signature_or_partial_hybrid() {
    let mut f = Fixture::new();
    let valid = request(
        &f,
        "issue",
        "root",
        2,
        grant("leaf", 51, vec![Scope::Status], None, RECOVERY_DELAY_MS),
        1,
    );
    for which in 0..6 {
        let mut bad = valid.clone();
        match which {
            0 => sign_request(&mut bad, &f.state().admin_key_grants["root"], 51),
            1 => {
                bad.signature.signer_grant_id = "leaf".into();
                bad.intent.issuer_grant_id = "leaf".into();
            }
            2 | 3 => {
                let offset = if which == 2 { 4 } else { 200 };
                let flip = if &bad.signature.signature[offset..offset + 2] == "ff" {
                    "00"
                } else {
                    "ff"
                };
                bad.signature
                    .signature
                    .replace_range(offset..offset + 2, flip);
            }
            4 => bad.signature.signature.truncate(130),
            _ => {
                bad.intent.grant.ml_dsa65_public_key = None;
                sign_request(&mut bad, &f.state().admin_key_grants["root"], 2);
            }
        }
        denied(f.writer.execute_admin_key_issuance_at(bad, time(0)));
    }
    let mut second_root = f.state().admin_key_grants["root"].clone();
    second_root.grant_id = "second-root".into();
    second_root.public_half = public(51).ed25519;
    second_root.ml_dsa65_public_key = Some(public(51).ml_dsa65);
    let second_root = request(&f, "second-root", "root", 2, second_root, 1);
    denied(f.writer.execute_admin_key_issuance_at(second_root, time(0)));
    assert!(f.state().admin_issuance.is_empty());
    committed(f.writer.execute_admin_key_issuance_at(valid, time(0)));
}

#[test]
fn admin_issuance_expected_target_epoch_generation_snapshot_and_signed_fields_are_exact() {
    let mut f = Fixture::new();
    let valid = request(
        &f,
        "issue",
        "root",
        2,
        grant("leaf", 51, vec![Scope::Status], None, RECOVERY_DELAY_MS),
        1,
    );
    for which in 0..6 {
        let mut bad = valid.clone();
        match which {
            0 => bad.intent.ledger_id = "other-ledger".into(),
            1 => {
                bad.intent
                    .expected_authority
                    .grants_authority
                    .mother_node_id = "other-mother".into()
            }
            2 => {
                bad.intent
                    .expected_authority
                    .grants_authority
                    .authority_epoch = "other-epoch".into()
            }
            3 => bad.intent.expected_authority.grants_authority.generation += 1,
            4 => bad.intent.expected_authority.authority_state_hash = "0".repeat(64),
            _ => {
                bad.intent.grant.scopes = vec![Scope::PeerAdd];
                denied(f.writer.execute_admin_key_issuance_at(bad, time(0)));
                continue;
            }
        }
        sign_request(&mut bad, &f.state().admin_key_grants["root"], 2);
        denied(f.writer.execute_admin_key_issuance_at(bad, time(0)));
    }
    committed(f.writer.execute_admin_key_issuance_at(valid, time(0)));
}

#[test]
fn admin_issuance_nonce_replay_duplicate_grant_and_changed_id_payload_conflict() {
    let mut f = Fixture::new();
    let original = issue_manager(&mut f);
    let mut conflict = original.clone();
    conflict.intent.grant.expires_at = Some(time(RECOVERY_DELAY_MS));
    denied(f.writer.execute_admin_key_issuance_at(conflict, time(0)));
    let repeated_nonce = request(
        &f,
        "second",
        "root",
        2,
        grant("leaf", 51, vec![Scope::Status], None, RECOVERY_DELAY_MS),
        1,
    );
    denied(
        f.writer
            .execute_admin_key_issuance_at(repeated_nonce, time(0)),
    );
    let duplicate_grant = request(
        &f,
        "another",
        "root",
        2,
        f.state().admin_key_grants["manager"].clone(),
        2,
    );
    denied(
        f.writer
            .execute_admin_key_issuance_at(duplicate_grant, time(0)),
    );
    let valid = request(
        &f,
        "second",
        "root",
        2,
        grant("leaf", 51, vec![Scope::Status], None, RECOVERY_DELAY_MS),
        2,
    );
    committed(f.writer.execute_admin_key_issuance_at(valid, time(0)));
    assert_eq!(f.state().admin_issuance.len(), 2);
}

#[test]
fn admin_issuance_two_requests_against_one_snapshot_have_one_winner() {
    let mut f = Fixture::new();
    let first = request(
        &f,
        "first",
        "root",
        2,
        grant("first", 50, vec![Scope::Status], None, RECOVERY_DELAY_MS),
        1,
    );
    let second = request(
        &f,
        "second",
        "root",
        2,
        grant("second", 51, vec![Scope::Status], None, RECOVERY_DELAY_MS),
        2,
    );
    committed(f.writer.execute_admin_key_issuance_at(first, time(0)));
    denied(f.writer.execute_admin_key_issuance_at(second, time(0)));
    assert_eq!(f.state().admin_issuance.len(), 1);
}

#[test]
fn admin_issuance_legacy_paths_cannot_smuggle_ceilings_or_mutate_enrolled_state() {
    let mut f = Fixture::new();
    let recipient = grant(
        "manager",
        50,
        vec![Scope::KeyManage],
        Some(vec![Scope::Status]),
        RECOVERY_DELAY_MS,
    );
    assert!(
        apply_authority_changes(
            &mut f.state(),
            &[AuthorityChangeV1::AdminKeyGrantPut {
                grant: Box::new(recipient.clone()),
                signature: None
            }]
        )
        .is_err()
    );
    let mut forged = f.state();
    forged
        .admin_key_grants
        .insert("manager".into(), recipient.clone());
    assert!(authority_state_hash(&forged).is_err());
    let request = request(&f, "issue", "root", 2, recipient, 1);
    let change = AuthorityChangeV1::AdminKeyIssue {
        request: Box::new(request.clone()),
        decided_at: time(0),
    };
    assert!(
        apply_authority_changes(
            &mut f.state(),
            &[
                change.clone(),
                AuthorityChangeV1::AdminKeyGrantRevoke {
                    grant_id: "day-a".into(),
                    signature: None
                }
            ]
        )
        .is_err()
    );
    denied(f.writer.execute_authority_mutation(
        AuthorityMutationRequestV1 {
            mutation_id: "issue".into(),
            changes: vec![change],
            decided_at: time(0).to_string(),
            grant_shaping_sources: vec![GrantShapingSourceV1::AdminKey {
                public_half: public(2).ed25519,
            }],
        },
        |_| panic!("no legacy projection"),
    ));
    committed(f.writer.execute_admin_key_issuance_at(request, time(0)));
    assert!(
        apply_authority_changes(
            &mut f.state(),
            &[AuthorityChangeV1::AdminKeyGrantPut {
                grant: Box::new(grant(
                    "bypass",
                    51,
                    vec![Scope::PeerAdd],
                    None,
                    RECOVERY_DELAY_MS
                )),
                signature: None
            }]
        )
        .is_err()
    );
}

#[test]
fn admin_issuance_expired_issuer_denies_new_grants_but_committed_retry_resolves_history() {
    let mut f = Fixture::new();
    let original = issue_manager(&mut f);
    let request = request(
        &f,
        "expired-issuer",
        "manager",
        50,
        grant("leaf", 51, vec![Scope::Status], None, 3 * RECOVERY_DELAY_MS),
        1,
    );
    denied(
        f.writer
            .execute_admin_key_issuance_at(request, time(2 * RECOVERY_DELAY_MS)),
    );
    assert!(matches!(
        f.writer
            .execute_admin_key_issuance_at(original, time(3 * RECOVERY_DELAY_MS)),
        AuthorityMutationResultV1::CommittedProjectionPending {
            resolution: AuthorityMutationResolutionV1::ResolvedExistingFact,
            ..
        }
    ));
    assert_eq!(f.state().admin_issuance.len(), 1);
}

#[test]
fn admin_issuance_recovery_maintenance_denies_and_replacement_invalidates_issued_grants() {
    let mut f = Fixture::new();
    f.enroll();
    let original = issue_manager(&mut f);
    f.begin();
    let blocked = request(
        &f,
        "blocked",
        "root",
        2,
        grant(
            "blocked",
            51,
            vec![Scope::Status],
            None,
            3 * RECOVERY_DELAY_MS,
        ),
        2,
    );
    denied(f.writer.execute_admin_key_issuance_at(blocked, time(0)));
    f.challenge();
    committed(f.run(
        "complete",
        f.completion(),
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    assert_eq!(
        f.state().admin_key_grants["manager"].grant_state,
        AdminKeyGrantStateV1::Revoked
    );
    assert_eq!(
        f.state().admin_issuance["manager"]
            .issuer_at_acceptance
            .public_half,
        public(2).ed25519
    );
    assert!(matches!(
        f.writer
            .execute_admin_key_issuance_at(original, time(RECOVERY_DELAY_MS)),
        AuthorityMutationResultV1::CommittedProjectionPending {
            resolution: AuthorityMutationResolutionV1::ResolvedExistingFact,
            ..
        }
    ));
    let snapshot = authority_state_hash(&f.state()).unwrap();
    let signature = sign(6, &f.recovery().resume_signing_message(&snapshot).unwrap());
    committed(f.run(
        "resume",
        Action::Resume {
            authority_snapshot: snapshot,
            signature,
        },
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    let revoked = request(
        &f,
        "revoked-issuer",
        "manager",
        50,
        grant("bad", 51, vec![Scope::Status], None, 2 * RECOVERY_DELAY_MS),
        1,
    );
    denied(
        f.writer
            .execute_admin_key_issuance_at(revoked, time(RECOVERY_DELAY_MS)),
    );
    let alias = grant(
        "alias-retired-root",
        2,
        vec![Scope::Status],
        None,
        2 * RECOVERY_DELAY_MS,
    );
    let alias = request(&f, "alias", "recovered:owner-a:2", 6, alias, 1);
    denied(
        f.writer
            .execute_admin_key_issuance_at(alias, time(RECOVERY_DELAY_MS)),
    );
    let reused_root_id = request(
        &f,
        "reuse-root-id",
        "recovered:owner-a:2",
        6,
        grant("root", 51, vec![Scope::Status], None, 2 * RECOVERY_DELAY_MS),
        1,
    );
    denied(
        f.writer
            .execute_admin_key_issuance_at(reused_root_id, time(RECOVERY_DELAY_MS)),
    );
    assert!(
        f.writer
            .cached_replay()
            .unwrap()
            .admin_grant_ids
            .contains("root")
    );
    let valid = request(
        &f,
        "new-root-issue",
        "recovered:owner-a:2",
        6,
        grant(
            "fresh",
            51,
            vec![Scope::Status],
            None,
            2 * RECOVERY_DELAY_MS,
        ),
        1,
    );
    committed(
        f.writer
            .execute_admin_key_issuance_at(valid, time(RECOVERY_DELAY_MS)),
    );
}

#[test]
fn admin_issuance_during_policy_wait_is_not_overwritten_by_policy_activation() {
    let mut f = Fixture::new();
    f.enroll();
    let state = f.recovery();
    let mut policy = state.policy.clone();
    policy.generation += 1;
    let edit = GuardianPolicyEditV1 {
        edit_id: "disable".into(),
        policy,
        guardian_bindings: state.guardian_bindings().clone(),
        enabled: false,
        issued_at: time(0),
        expires_at: time(RECOVERY_APPROVAL_MAX_MS),
    };
    let approvals = (0..2)
        .map(|i| {
            let guardian_node_id = state.policy.guardians[i].mother_node_id.clone();
            let bytes = guardian_policy_approval_message(
                &state,
                &edit,
                GuardianPolicyApprovalPurposeV1::Propose,
                &guardian_node_id,
                &time(0),
                &time(RECOVERY_APPROVAL_MAX_MS),
            )
            .unwrap();
            GuardianRecoveryApprovalV1 {
                guardian_node_id,
                issued_at: time(0),
                expires_at: time(RECOVERY_APPROVAL_MAX_MS),
                signature: sign(i as u8 + 3, &bytes),
            }
        })
        .collect();
    let proposal = GuardianPolicyProposalV1 {
        owner_signature: sign(2, &edit.owner_signing_message(&state).unwrap()),
        edit,
        acceptances: Vec::new(),
        approvals,
    };
    committed(f.run(
        "propose-disable",
        Action::ProposePolicy {
            proposal: Box::new(proposal),
        },
        0,
        0,
    ));
    issue_manager(&mut f);
    let grants = f.state().admin_key_grants;
    committed(f.run(
        "disable",
        Action::ActivatePolicy {},
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    assert_eq!(f.state().admin_key_grants, grants);
    assert_eq!(f.state().admin_issuance.len(), 1);
}

#[test]
fn admin_issuance_partial_and_complete_frame_faults_replay_without_double_grants() {
    for fault in [
        TestAppendFault::PartialFrame,
        TestAppendFault::CompleteFrameBeforeDurabilityAck,
        TestAppendFault::CompleteFrameAfterDurabilityAck,
    ] {
        let mut f = Fixture::new();
        let recipient = grant("leaf", 51, vec![Scope::Status], None, RECOVERY_DELAY_MS);
        let original = request(&f, "issue", "root", 2, recipient.clone(), 1);
        f.writer.inject_append_fault_after_for_test(0, fault);
        assert!(matches!(
            f.writer
                .execute_admin_key_issuance_at(original.clone(), time(0)),
            AuthorityMutationResultV1::CommitUnknown { .. }
        ));
        assert!(f.writer.is_poisoned());
        let mut f = f.reopen();
        if fault == TestAppendFault::PartialFrame {
            assert!(f.writer.recovery_status.is_some());
            assert!(f.state().admin_issuance.is_empty());
            denied(f.writer.execute_admin_key_issuance_at(original, time(1)));
            let fresh = request(&f, "issue", "root", 2, recipient, 1);
            committed(f.writer.execute_admin_key_issuance_at(fresh, time(1)));
        } else {
            assert!(matches!(
                f.writer
                    .execute_admin_key_issuance_at(original, time(2 * RECOVERY_DELAY_MS)),
                AuthorityMutationResultV1::CommittedProjectionPending {
                    resolution: AuthorityMutationResolutionV1::ResolvedExistingFact,
                    ..
                }
            ));
        }
        assert_eq!(f.state().admin_issuance.len(), 1);
    }
}

#[test]
fn admin_issuance_provenance_and_parser_are_strict() {
    let mut f = Fixture::new();
    let original = issue_manager(&mut f);
    for which in 0..3 {
        let mut forged = f.state();
        match which {
            0 => forged
                .admin_issuance
                .get_mut("manager")
                .unwrap()
                .intent
                .grant
                .scopes
                .push(Scope::Status),
            1 => {
                forged
                    .admin_issuance
                    .get_mut("manager")
                    .unwrap()
                    .issuer_at_acceptance
                    .grant_id = "not-root".into()
            }
            _ => {
                forged.admin_key_grants.remove("manager");
            }
        }
        assert!(authority_state_hash(&forged).is_err());
    }
    let mut value = serde_json::to_value(original).unwrap();
    value["intent"]["grant"]["issuance_ceiling"]["unlimited"] = serde_json::json!(true);
    assert!(serde_json::from_value::<AdminKeyIssueRequestV1>(value).is_err());
}

#[test]
fn admin_issuance_legacy_key_manage_does_not_gain_an_implicit_ceiling() {
    let mut f = Fixture::new();
    let legacy = grant(
        "legacy-manager",
        50,
        vec![Scope::KeyManage],
        None,
        RECOVERY_DELAY_MS,
    );
    committed(f.writer.execute_authority_mutation(
        AuthorityMutationRequestV1 {
            mutation_id: "legacy-manager".into(),
            changes: vec![AuthorityChangeV1::AdminKeyGrantPut {
                grant: Box::new(legacy),
                signature: None,
            }],
            grant_shaping_sources: vec![GrantShapingSourceV1::AdminKey {
                public_half: public(2).ed25519,
            }],
            decided_at: time(0).to_string(),
        },
        |_| Ok(None),
    ));
    let requested = request(
        &f,
        "escalate-legacy",
        "legacy-manager",
        50,
        grant("leaf", 51, vec![Scope::Status], None, RECOVERY_DELAY_MS),
        1,
    );
    denied(f.writer.execute_admin_key_issuance_at(requested, time(0)));
    assert!(f.state().admin_issuance.is_empty());
}

#[test]
fn admin_issuance_same_key_grants_cannot_union_their_ceilings() {
    let mut f = Fixture::new();
    for (id, scope, nonce) in [
        ("manager-a", Scope::PeerAdd, 1),
        ("manager-b", Scope::Status, 2),
    ] {
        let manager = request(
            &f,
            id,
            "root",
            2,
            grant(
                id,
                50,
                vec![Scope::KeyManage],
                Some(vec![scope]),
                RECOVERY_DELAY_MS,
            ),
            nonce,
        );
        committed(f.writer.execute_admin_key_issuance_at(manager, time(0)));
    }
    for id in ["manager-a", "manager-b"] {
        let combined = request(
            &f,
            "combined",
            id,
            50,
            grant(
                "leaf",
                51,
                vec![Scope::PeerAdd, Scope::Status],
                None,
                RECOVERY_DELAY_MS,
            ),
            1,
        );
        denied(f.writer.execute_admin_key_issuance_at(combined, time(0)));
    }
    assert_eq!(f.state().admin_issuance.len(), 2);
}

#[test]
fn admin_issuance_clock_rollback_denies_new_authority_but_not_historical_receipts() {
    let mut f = Fixture::new();
    let original = request(
        &f,
        "first",
        "root",
        2,
        grant("first", 50, vec![Scope::Status], None, RECOVERY_DELAY_MS),
        1,
    );
    committed(
        f.writer
            .execute_admin_key_issuance_at(original.clone(), time(10)),
    );
    let second = request(
        &f,
        "second",
        "root",
        2,
        grant("second", 51, vec![Scope::Status], None, RECOVERY_DELAY_MS),
        2,
    );
    denied(
        f.writer
            .execute_admin_key_issuance_at(second.clone(), time(9)),
    );
    assert!(matches!(
        f.writer.execute_admin_key_issuance_at(original, time(0)),
        AuthorityMutationResultV1::CommittedProjectionPending {
            resolution: AuthorityMutationResolutionV1::ResolvedExistingFact,
            ..
        }
    ));
    committed(f.writer.execute_admin_key_issuance_at(second, time(10)));
}

#[test]
fn admin_issuance_cannot_squat_future_recovery_root_ids() {
    let mut f = Fixture::new();
    f.enroll();
    issue_manager(&mut f);
    for issuer in ["root", "manager"] {
        let attempted = request(
            &f,
            "squat-root-id",
            issuer,
            if issuer == "root" { 2 } else { 50 },
            grant(
                "recovered:owner-a:2",
                51,
                vec![Scope::Status],
                None,
                RECOVERY_DELAY_MS,
            ),
            2,
        );
        denied(f.writer.execute_admin_key_issuance_at(attempted, time(0)));
    }
    f.begin();
    f.challenge();
    committed(f.run(
        "complete",
        f.completion(),
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    assert_eq!(
        f.state().admin_key_grants["recovered:owner-a:2"].class,
        AdminKeyGrantClassV1::Recovery
    );
}

#[test]
fn admin_issuance_generated_root_id_is_not_truncated_to_the_binding_id_bound() {
    let mut f = Fixture::new();
    f.enrollment.policy.target.binding_id = "o".repeat(128);
    let bytes = GuardianEnrollmentV1::signing_message(
        &f.enrollment.genesis,
        &f.enrollment.policy,
        &f.enrollment.guardian_bindings,
    )
    .unwrap();
    f.enrollment.genesis_authorization = sign(1, &bytes);
    f.enrollment.owner_authorization = sign(2, &bytes);
    f.enrollment.guardian_owner_acceptances = std::array::from_fn(|i| sign(i as u8 + 10, &bytes));
    f.enrollment.guardian_acceptances = std::array::from_fn(|i| sign(i as u8 + 3, &bytes));
    f.enroll();
    f.begin();
    f.challenge();
    committed(f.run(
        "complete",
        f.completion(),
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    let snapshot = authority_state_hash(&f.state()).unwrap();
    let signature = sign(6, &f.recovery().resume_signing_message(&snapshot).unwrap());
    committed(f.run(
        "resume",
        Action::Resume {
            authority_snapshot: snapshot,
            signature,
        },
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    let root_id = format!("recovered:{}:2", "o".repeat(128));
    assert!(root_id.len() > 128);
    let issued = request(
        &f,
        "long-root-issue",
        &root_id,
        6,
        grant(
            "fresh",
            51,
            vec![Scope::Status],
            None,
            2 * RECOVERY_DELAY_MS,
        ),
        1,
    );
    committed(
        f.writer
            .execute_admin_key_issuance_at(issued, time(RECOVERY_DELAY_MS)),
    );
    assert_eq!(
        f.state().admin_issuance["fresh"]
            .issuer_at_acceptance
            .grant_id,
        root_id
    );
}

#[test]
fn admin_issuance_corpus_roundtrip_and_explicit_export() {
    let mut f = Fixture::new();
    let prior = f.state();
    issue_manager(&mut f);
    let change = f.writer.cached_replay().unwrap().mutations["issue-manager"]
        .fact
        .changes[0]
        .clone();
    let mut corpus = vec![(
        "root-issue.json",
        serde_json::to_vec(&(prior, change)).unwrap(),
    )];
    let prior = f.state();
    let leaf = request(
        &f,
        "issue-leaf",
        "manager",
        50,
        grant("leaf", 51, vec![Scope::Status], None, RECOVERY_DELAY_MS),
        1,
    );
    committed(f.writer.execute_admin_key_issuance_at(leaf, time(0)));
    let change = f.writer.cached_replay().unwrap().mutations["issue-leaf"]
        .fact
        .changes[0]
        .clone();
    corpus.push((
        "delegated-issue.json",
        serde_json::to_vec(&(prior, change)).unwrap(),
    ));
    for (_, bytes) in &corpus {
        let (mut state, change): (AuthorityStateV1, AuthorityChangeV1) =
            serde_json::from_slice(bytes).unwrap();
        apply_authority_changes(&mut state, &[change]).unwrap();
    }
    let entries = f.writer.entries().unwrap();
    assert_eq!(replay_authority_entries(&entries).unwrap().state, f.state());
    corpus.push((
        "canonical-prefix.json",
        serde_json::to_vec(&entries).unwrap(),
    ));
    if let Ok(dir) = std::env::var("MCT_ISSUANCE_CORPUS_OUT") {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, bytes) in corpus {
            std::fs::write(Path::new(&dir).join(name), bytes).unwrap();
        }
    }
}

use super::*;
use GuardianPolicyApprovalPurposeV1 as Purpose;

fn edit(f: &Fixture, id: &str, enabled: bool, at: u64) -> GuardianPolicyEditV1 {
    let current = f.recovery();
    let mut policy = current.policy.clone();
    policy.generation += 1;
    GuardianPolicyEditV1 {
        edit_id: id.into(),
        policy,
        guardian_bindings: current.guardian_bindings().clone(),
        enabled,
        issued_at: time(at),
        expires_at: time(at + RECOVERY_APPROVAL_MAX_MS),
    }
}
fn new_members(edit: &mut GuardianPolicyEditV1, all: bool) {
    for i in 0..3 {
        if !all && i != 2 {
            continue;
        }
        let node = MctNodeId::new(format!("guardian-new-{i}")).unwrap();
        let binding = &mut edit.guardian_bindings[i];
        binding.mother_node_id = node.clone();
        binding.binding_id = format!("owner-new-{i}");
        binding.ledger_id = format!("ledger-new-{i}");
        binding.epoch_id = format!("epoch-new-{i}");
        binding.root_generation = 1;
        binding.root_key = public(40 + i as u8);
        let guardian = &mut edit.policy.guardians[i];
        guardian.mother_node_id = node;
        guardian.owner_binding_id = binding.binding_id.clone();
        guardian.owner_binding_generation = 1;
        guardian.signing_key = public(30 + i as u8);
    }
}
fn votes(
    f: &Fixture,
    edit: &GuardianPolicyEditV1,
    purpose: Purpose,
    at: u64,
) -> Vec<GuardianRecoveryApprovalV1> {
    let state = f.recovery();
    (0..2)
        .map(|i| {
            let guardian = &state.policy.guardians[i];
            let seed = if guardian.mother_node_id.as_str().contains("new") {
                30
            } else {
                3
            } + i as u8;
            let issued_at = time(at);
            let expires_at = time(at + RECOVERY_APPROVAL_MAX_MS);
            let message = guardian_policy_approval_message(
                &state,
                edit,
                purpose,
                &guardian.mother_node_id,
                &issued_at,
                &expires_at,
            )
            .unwrap();
            GuardianRecoveryApprovalV1 {
                guardian_node_id: guardian.mother_node_id.clone(),
                issued_at,
                expires_at,
                signature: sign(seed, &message),
            }
        })
        .collect()
}
fn proposal(f: &Fixture, edit: GuardianPolicyEditV1, at: u64) -> GuardianPolicyProposalV1 {
    let state = f.recovery();
    let owner_seed = if state.binding.root_key == public(6) {
        6
    } else {
        2
    };
    let owner_signature = sign(owner_seed, &edit.owner_signing_message(&state).unwrap());
    let acceptances = if edit.enabled {
        (0..3)
            .map(|i| {
                let guardian = &edit.policy.guardians[i];
                let new = guardian.mother_node_id.as_str().contains("new");
                let message = edit
                    .acceptance_signing_message(&state, &guardian.mother_node_id)
                    .unwrap();
                GuardianPolicyAcceptanceV1 {
                    guardian_node_id: guardian.mother_node_id.clone(),
                    owner_signature: sign(if new { 40 } else { 10 } + i as u8, &message),
                    guardian_signature: sign(if new { 30 } else { 3 } + i as u8, &message),
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let approvals = if state.recovery_enabled() {
        votes(f, &edit, Purpose::Propose, at)
    } else {
        Vec::new()
    };
    GuardianPolicyProposalV1 {
        edit,
        owner_signature,
        acceptances,
        approvals,
    }
}
fn propose_action(proposal: GuardianPolicyProposalV1) -> Action {
    Action::ProposePolicy {
        proposal: Box::new(proposal),
    }
}
fn disable(f: &mut Fixture) {
    let proposal = proposal(f, edit(f, "disable-1", false, 0), 0);
    committed(f.run("propose-disable", propose_action(proposal), 0, 0));
    committed(f.run(
        "disable",
        Action::ActivatePolicy {},
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
}

#[test]
fn policy_edit_active_replacement_needs_owner_old_quorum_and_newcomer_acceptance() {
    let mut f = Fixture::new();
    f.enroll();
    let mut next = edit(&f, "edit-1", true, 0);
    new_members(&mut next, false);
    let valid = proposal(&f, next, 0);
    for which in 0..7 {
        let mut bad = valid.clone();
        match which {
            0 => bad.approvals.clear(),
            1 => bad.approvals.truncate(1),
            2 => bad.owner_signature = sign(30, b"not-owner"),
            3 => {
                bad.acceptances.pop();
            }
            4 => bad.acceptances[2].owner_signature = sign(2, b"not-new-owner"),
            5 => {
                bad.approvals[1].guardian_node_id =
                    bad.edit.policy.guardians[2].mother_node_id.clone();
                bad.approvals[1].signature = bad.acceptances[2].guardian_signature.clone();
            }
            _ => bad.approvals[0].signature = bad.acceptances[0].guardian_signature.clone(),
        }
        denied(f.run("bad-proposal", propose_action(bad), 0, 0));
        assert!(f.recovery().pending_policy_edit().is_none());
    }
    // Unchanged members need not re-accept an active edit. New member must.
    let mut valid = valid;
    valid.acceptances.drain(..2);
    let grants_before = f.state().admin_key_grants;
    committed(f.run("propose", propose_action(valid), 0, 0));
    assert_eq!(f.recovery().policy.generation, 1);
    for (wall, mono) in [
        (RECOVERY_DELAY_MS, 1),
        (1, RECOVERY_DELAY_MS),
        (RECOVERY_DELAY_MS - 1, RECOVERY_DELAY_MS - 1),
    ] {
        denied(f.run("early", Action::ActivatePolicy {}, wall, mono));
    }
    committed(f.run(
        "activate",
        Action::ActivatePolicy {},
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    assert_eq!(f.recovery().policy.generation, 2);
    assert_eq!(f.recovery().binding.root_generation, 1);
    assert_eq!(f.recovery().policy.guardians[2].signing_key, public(32));
    assert_eq!(f.state().admin_key_grants, grants_before);
    assert!(
        f.recovery()
            .policy_edits
            .unwrap()
            .consumed_edit_ids
            .contains("edit-1")
    );
}

#[test]
fn policy_edit_disablement_releases_old_guardians_but_preserves_history() {
    let mut f = Fixture::new();
    f.enroll();
    disable(&mut f);
    assert!(!f.recovery().recovery_enabled());
    assert!(!f.recovery().maintenance);
    assert_eq!(f.recovery().policy.guardians, f.enrollment.policy.guardians);
    assert_eq!(f.recovery().policy.generation, 2);
    let request = f.request();
    let approvals = f.approvals(
        &request,
        RecoveryApprovalPurposeV1::Initiate,
        RECOVERY_DELAY_MS,
    );
    denied(f.run(
        "former-guardian-recovery",
        Action::Initiate {
            request: Box::new(request),
            approvals,
        },
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    let next = edit(&f, "reenable", true, RECOVERY_DELAY_MS);
    assert!(
        guardian_policy_approval_message(
            &f.recovery(),
            &next,
            Purpose::Propose,
            &f.recovery().policy.guardians[0].mother_node_id,
            &time(RECOVERY_DELAY_MS),
            &time(RECOVERY_DELAY_MS + RECOVERY_APPROVAL_MAX_MS)
        )
        .is_err()
    );
    let f = f.reopen();
    assert!(!f.recovery().recovery_enabled());
    assert_eq!(f.recovery().binding.root_key, public(2));
}

#[test]
fn policy_edit_reenable_requires_fresh_all_three_acceptances_even_for_same_roster() {
    let mut f = Fixture::new();
    f.enroll();
    let old_votes = votes(&f, &edit(&f, "disable-1", false, 0), Purpose::Propose, 0);
    disable(&mut f);
    let at = RECOVERY_DELAY_MS;
    let valid = proposal(&f, edit(&f, "reenable", true, at), at);
    assert!(valid.approvals.is_empty());
    for which in 0..5 {
        let mut bad = valid.clone();
        match which {
            0 => {
                bad.acceptances.pop();
            }
            1 => bad.approvals = old_votes.clone(),
            2 => bad.owner_signature = sign(3, b"guardian-not-owner"),
            3 => {
                bad.acceptances[0].guardian_signature = f.enrollment.guardian_acceptances[0].clone()
            }
            _ => {
                bad.acceptances[0].owner_signature =
                    f.enrollment.guardian_owner_acceptances[0].clone()
            }
        }
        denied(f.run("bad-reenable", propose_action(bad), at, at));
    }
    committed(f.run("reenable-proposal", propose_action(valid), at, at));
    assert!(!f.recovery().recovery_enabled());
    denied(f.run(
        "too-soon",
        Action::ActivatePolicy {},
        2 * at - 1,
        2 * at - 1,
    ));
    committed(f.run("reenable", Action::ActivatePolicy {}, 2 * at, 2 * at));
    assert!(f.recovery().recovery_enabled());
    assert_eq!(f.recovery().policy.generation, 3);
}

#[test]
fn policy_edit_reenable_with_new_roster_does_not_inherit_former_votes() {
    let mut f = Fixture::new();
    f.enroll();
    disable(&mut f);
    let at = RECOVERY_DELAY_MS;
    let mut next = edit(&f, "fresh-roster", true, at);
    new_members(&mut next, true);
    let proposal = proposal(&f, next, at);
    committed(f.run("propose", propose_action(proposal), at, at));
    committed(f.run("activate", Action::ActivatePolicy {}, 2 * at, 2 * at));
    let request = f.request();
    // Same IDs in the envelope, but signatures under the former guardian keys.
    let mut approvals = f.approvals(&request, RecoveryApprovalPurposeV1::Initiate, 2 * at);
    denied(f.run(
        "old-key-recovery",
        Action::Initiate {
            request: Box::new(request.clone()),
            approvals: approvals.clone(),
        },
        2 * at,
        2 * at,
    ));
    let state = f.recovery();
    for (i, approval) in approvals.iter_mut().enumerate() {
        let bytes = GuardianApprovalMessageV1 {
            policy: &state.policy,
            intent: &request.intent,
            purpose: &RecoveryApprovalPurposeV1::Initiate,
            guardian_node_id: &approval.guardian_node_id,
            issued_at: &approval.issued_at,
            expires_at: &approval.expires_at,
        }
        .signing_message()
        .unwrap();
        approval.signature = sign(30 + i as u8, &bytes);
    }
    committed(f.run(
        "new-key-recovery",
        Action::Initiate {
            request: Box::new(request),
            approvals,
        },
        2 * at,
        2 * at,
    ));
    assert!(f.recovery().maintenance);
}

#[test]
fn policy_edit_active_cancellation_uses_current_quorum_not_owner_or_proposed_roster() {
    let mut f = Fixture::new();
    f.enroll();
    let next = edit(&f, "disable-1", false, 0);
    let proposal = proposal(&f, next.clone(), 0);
    let old_votes = proposal.approvals.clone();
    committed(f.run("propose", propose_action(proposal), 0, 0));
    denied(f.run(
        "owner-cancel",
        Action::CancelPolicy {
            owner_signature: Some(sign(2, b"cancel")),
            approvals: Vec::new(),
        },
        1,
        1,
    ));
    denied(f.run(
        "wrong-purpose",
        Action::CancelPolicy {
            owner_signature: None,
            approvals: old_votes,
        },
        1,
        1,
    ));
    let approvals = votes(&f, &next, Purpose::Cancel, 1);
    denied(f.run(
        "one-vote",
        Action::CancelPolicy {
            owner_signature: None,
            approvals: approvals[..1].to_vec(),
        },
        1,
        1,
    ));
    committed(f.run(
        "cancel",
        Action::CancelPolicy {
            owner_signature: None,
            approvals,
        },
        1,
        1,
    ));
    assert!(f.recovery().recovery_enabled());
    assert!(f.recovery().pending_policy_edit().is_none());
    let retry = proposal_for_reused_id(&f);
    denied(f.run("reuse-id", propose_action(retry), 2, 2));
}
fn proposal_for_reused_id(f: &Fixture) -> GuardianPolicyProposalV1 {
    proposal(f, edit(f, "disable-1", false, 2), 2)
}

#[test]
fn policy_edit_disabled_owner_can_withdraw_reenable_without_former_guardian_veto() {
    let mut f = Fixture::new();
    f.enroll();
    let old_votes = votes(&f, &edit(&f, "disable-1", false, 0), Purpose::Propose, 0);
    disable(&mut f);
    let at = RECOVERY_DELAY_MS;
    let next = proposal(&f, edit(&f, "reenable", true, at), at);
    committed(f.run("propose", propose_action(next), at, at));
    denied(f.run(
        "former-cancel",
        Action::CancelPolicy {
            owner_signature: None,
            approvals: old_votes,
        },
        at,
        at,
    ));
    let signature = sign(2, &f.recovery().policy_cancel_signing_message().unwrap());
    committed(f.run(
        "owner-withdraw",
        Action::CancelPolicy {
            owner_signature: Some(signature),
            approvals: Vec::new(),
        },
        at,
        at,
    ));
    assert!(!f.recovery().recovery_enabled());
    assert!(f.recovery().pending_policy_edit().is_none());
}

#[test]
fn policy_edit_pending_edit_and_recovery_exclude_each_other() {
    for recovery_first in [false, true] {
        let mut f = Fixture::new();
        f.enroll();
        if recovery_first {
            f.begin();
        }
        let next = proposal(&f, edit(&f, "disable-1", false, 0), 0);
        let result = f.run("propose", propose_action(next), 0, 0);
        if recovery_first {
            denied(result);
        } else {
            committed(result);
            let request = f.request();
            let approvals = f.approvals(&request, RecoveryApprovalPurposeV1::Initiate, 0);
            denied(f.run(
                "begin",
                Action::Initiate {
                    request: Box::new(request),
                    approvals,
                },
                0,
                0,
            ));
            let duplicate = proposal(&f, edit(&f, "another", false, 0), 0);
            denied(f.run("second-edit", propose_action(duplicate), 0, 0));
        }
    }
}

#[test]
fn policy_edit_restart_rebases_full_delay_and_invalidates_uncommitted_cancel_votes() {
    let mut f = Fixture::new();
    f.enroll();
    let next = edit(&f, "disable-1", false, 0);
    let proposal = proposal(&f, next.clone(), 0);
    committed(f.run("propose", propose_action(proposal), 0, 0));
    let cancel = Action::CancelPolicy {
        owner_signature: None,
        approvals: votes(&f, &next, Purpose::Cancel, 0),
    };
    let mut f = f.reopen();
    denied(f.run(
        "no-rebase",
        Action::ActivatePolicy {},
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    committed(f.run("rebase", Action::RebaseWait {}, 1, 0));
    denied(f.run("stale-cancel", cancel, 2, 1));
    denied(f.run(
        "shortened",
        Action::ActivatePolicy {},
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS - 1,
    ));
    committed(f.run(
        "activate",
        Action::ActivatePolicy {},
        RECOVERY_DELAY_MS + 1,
        RECOVERY_DELAY_MS,
    ));
    assert!(!f.recovery().recovery_enabled());
}

#[test]
fn policy_edit_cancel_activation_race_has_exactly_one_winner() {
    for cancel_first in [true, false] {
        let mut f = Fixture::new();
        f.enroll();
        let next = edit(&f, "disable-1", false, 0);
        let proposal = proposal(&f, next.clone(), 0);
        committed(f.run("propose", propose_action(proposal), 0, 0));
        let cancel = f.command(Action::CancelPolicy {
            owner_signature: None,
            approvals: votes(&f, &next, Purpose::Cancel, RECOVERY_DELAY_MS),
        });
        let activate = f.command(Action::ActivatePolicy {});
        let (first, second) = if cancel_first {
            (cancel, activate)
        } else {
            (activate, cancel)
        };
        committed(f.writer.execute_guardian_recovery_with_clock(
            "first".into(),
            first,
            Some((time(RECOVERY_DELAY_MS), RECOVERY_DELAY_MS)),
        ));
        denied(f.writer.execute_guardian_recovery_with_clock(
            "second".into(),
            second,
            Some((time(RECOVERY_DELAY_MS), RECOVERY_DELAY_MS)),
        ));
        assert_eq!(f.recovery().recovery_enabled(), cancel_first);
    }
}

#[test]
fn policy_edit_activation_uncertainty_replays_once_on_both_sides_of_ack() {
    for fault in [
        TestAppendFault::CompleteFrameBeforeDurabilityAck,
        TestAppendFault::CompleteFrameAfterDurabilityAck,
    ] {
        let mut f = Fixture::new();
        f.enroll();
        let proposal = proposal(&f, edit(&f, "disable-1", false, 0), 0);
        committed(f.run("propose", propose_action(proposal), 0, 0));
        let command = f.command(Action::ActivatePolicy {});
        f.writer.inject_append_fault_after_for_test(0, fault);
        assert!(matches!(
            f.writer.execute_guardian_recovery_with_clock(
                "activate".into(),
                command.clone(),
                Some((time(RECOVERY_DELAY_MS), RECOVERY_DELAY_MS))
            ),
            AuthorityMutationResultV1::CommitUnknown { .. }
        ));
        assert!(f.writer.is_poisoned());
        let mut f = f.reopen();
        assert!(!f.recovery().recovery_enabled());
        assert!(matches!(
            f.writer.execute_guardian_recovery_with_clock(
                "activate".into(),
                command,
                Some((time(RECOVERY_DELAY_MS + 1), 0))
            ),
            AuthorityMutationResultV1::CommittedProjectionPending {
                resolution: AuthorityMutationResolutionV1::ResolvedExistingFact,
                ..
            }
        ));
        assert_eq!(f.recovery().policy.generation, 2);
    }
}

#[test]
fn policy_edit_torn_activation_preserves_active_policy_and_requires_new_wait() {
    let mut f = Fixture::new();
    f.enroll();
    let proposal = proposal(&f, edit(&f, "disable-1", false, 0), 0);
    committed(f.run("propose", propose_action(proposal), 0, 0));
    f.writer
        .inject_append_fault_after_for_test(0, TestAppendFault::PartialFrame);
    assert!(matches!(
        f.run(
            "activate",
            Action::ActivatePolicy {},
            RECOVERY_DELAY_MS,
            RECOVERY_DELAY_MS
        ),
        AuthorityMutationResultV1::CommitUnknown { .. }
    ));
    let mut f = f.reopen();
    assert!(f.writer.recovery_status.is_some());
    assert!(f.recovery().recovery_enabled());
    assert!(f.recovery().pending_policy_edit().is_some());
    committed(f.run("rebase", Action::RebaseWait {}, RECOVERY_DELAY_MS, 0));
    denied(f.run("early", Action::ActivatePolicy {}, RECOVERY_DELAY_MS + 1, 1));
    committed(f.run(
        "activate",
        Action::ActivatePolicy {},
        2 * RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    assert!(!f.recovery().recovery_enabled());
}

#[test]
fn policy_edit_parser_and_retained_slice_two_corpus_remain_strict() {
    for json in [
        r#"{"expected_state":null,"action":{"action":"activate_policy","skip_delay":true}}"#,
        r#"{"expected_state":null,"action":{"action":"cancel_policy","owner_signature":null,"approvals":[],"legacy_override":true}}"#,
    ] {
        assert!(serde_json::from_str::<GuardianRecoveryCommandV1>(json).is_err());
    }
    let entries: Vec<MctObservationLedgerEntry> = serde_json::from_slice(include_bytes!(
        "../../../fuzz/corpus/guardian_recovery/canonical-prefix.json"
    ))
    .unwrap();
    let replay = replay_authority_entries(&entries).unwrap();
    let state = replay.state.guardian_recovery.unwrap();
    assert!(state.policy_edits.is_none());
    assert!(state.recovery_enabled());
    assert_eq!(state.binding.root_generation, 2);
}

#[test]
fn policy_edit_rejects_stale_expired_duplicate_and_cross_binding_proofs() {
    let mut no_op = Fixture::new();
    no_op.enroll();
    assert!(
        edit(&no_op, "no-op", true, 0)
            .owner_signing_message(&no_op.recovery())
            .is_err()
    );
    let mut overflowing = no_op.recovery();
    overflowing.policy.generation = u64::MAX;
    assert!(
        edit(&no_op, "overflow", false, 0)
            .owner_signing_message(&overflowing)
            .is_err()
    );
    let mut f = Fixture::new();
    f.enroll();
    let valid = proposal(&f, edit(&f, "disable-1", false, 0), 0);
    for which in 0..7 {
        let mut bad = valid.clone();
        match which {
            0 => bad.edit.policy.generation += 1,
            1 => bad.edit.policy.target.root_generation += 1,
            2 => {
                bad.edit.guardian_bindings[0].voice_id = VoiceGenesisV1 {
                    public_key: public(55),
                    possession: sign(55, &VoiceGenesisV1::signing_message(&public(55)).unwrap()),
                }
                .verify()
                .unwrap()
            }
            3 => bad.approvals[1] = bad.approvals[0].clone(),
            4 => bad.edit.expires_at = time(RECOVERY_APPROVAL_MAX_MS + 1),
            5 => bad.edit.guardian_bindings[0].root_generation += 1,
            _ => {
                bad.edit.policy.guardians[1].signing_key =
                    bad.edit.policy.guardians[0].signing_key.clone()
            }
        }
        denied(f.run("bad", propose_action(bad), 0, 0));
    }
    denied(f.run(
        "expired",
        propose_action(valid.clone()),
        RECOVERY_APPROVAL_MAX_MS,
        RECOVERY_APPROVAL_MAX_MS,
    ));
    let mut f = f.reopen();
    committed(f.run("rebase", Action::RebaseWait {}, 1, 0));
    // Rewrapping old proofs with a fresh command expectation is insufficient.
    denied(f.run("stale-proof", propose_action(valid), 1, 0));
}

#[test]
fn policy_edit_proposal_fault_matrix_preserves_only_complete_canonical_authorization() {
    for fault in [
        TestAppendFault::PartialFrame,
        TestAppendFault::CompleteFrameBeforeDurabilityAck,
        TestAppendFault::CompleteFrameAfterDurabilityAck,
    ] {
        let mut f = Fixture::new();
        f.enroll();
        let proposal = proposal(&f, edit(&f, "disable-1", false, 0), 0);
        let command = f.command(propose_action(proposal));
        f.writer.inject_append_fault_after_for_test(0, fault);
        assert!(matches!(
            f.writer.execute_guardian_recovery_with_clock(
                "propose".into(),
                command.clone(),
                Some((time(0), 0))
            ),
            AuthorityMutationResultV1::CommitUnknown { .. }
        ));
        let mut f = f.reopen();
        assert!(f.recovery().recovery_enabled());
        if fault == TestAppendFault::PartialFrame {
            assert!(f.recovery().policy_edits.is_none());
            assert!(f.writer.recovery_status.is_some());
            committed(f.run("rebase", Action::RebaseWait {}, 1, 0));
            let fresh = proposal_for_reused_id(&f);
            committed(f.run("propose", propose_action(fresh), 2, 1));
        } else {
            assert!(f.recovery().pending_policy_edit().is_some());
            assert!(matches!(
                f.writer.execute_guardian_recovery_with_clock(
                    "propose".into(),
                    command,
                    Some((time(1), 0))
                ),
                AuthorityMutationResultV1::CommittedProjectionPending {
                    resolution: AuthorityMutationResolutionV1::ResolvedExistingFact,
                    ..
                }
            ));
            assert_eq!(
                f.recovery().policy_edits.unwrap().consumed_edit_ids.len(),
                1
            );
        }
    }
}

#[test]
fn policy_edit_after_root_recovery_uses_new_root_not_genesis_or_retired_root() {
    let mut f = Fixture::new();
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
    let next = proposal(
        &f,
        edit(&f, "disable-after-recovery", false, RECOVERY_DELAY_MS),
        RECOVERY_DELAY_MS,
    );
    for seed in [1, 2] {
        let mut bad = next.clone();
        bad.owner_signature = sign(
            seed,
            &bad.edit.owner_signing_message(&f.recovery()).unwrap(),
        );
        denied(f.run(
            "retired-owner",
            propose_action(bad),
            RECOVERY_DELAY_MS,
            RECOVERY_DELAY_MS,
        ));
    }
    committed(f.run(
        "propose",
        propose_action(next),
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    committed(f.run(
        "activate",
        Action::ActivatePolicy {},
        RECOVERY_DELAY_MS * 2,
        RECOVERY_DELAY_MS * 2,
    ));
    assert_eq!(f.recovery().binding.root_generation, 2);
    assert_eq!(f.recovery().policy.generation, 3);
    assert_eq!(f.recovery().binding.root_key, public(6));
}

#[test]
fn policy_edit_and_recovery_do_not_credit_time_spent_waiting_for_durability_ack() {
    for policy_edit in [true, false] {
        let mut f = Fixture::new();
        f.enroll();
        f.writer.recovery_test_ack_delay_ms = RECOVERY_APPROVAL_MAX_MS;
        if policy_edit {
            let next = proposal(&f, edit(&f, "disable-1", false, 0), 0);
            committed(f.run("propose", propose_action(next), 0, 0));
        } else {
            f.begin();
        }
        f.writer.recovery_test_ack_delay_ms = 0;
        let action = if policy_edit {
            Action::ActivatePolicy {}
        } else {
            Action::IssueChallenge {
                challenge: "post-ack".into(),
            }
        };
        denied(f.run(
            "pre-ack-credit",
            action.clone(),
            RECOVERY_DELAY_MS,
            RECOVERY_DELAY_MS,
        ));
        let boundary = RECOVERY_DELAY_MS + RECOVERY_APPROVAL_MAX_MS;
        denied(f.run("one-ms-early", action.clone(), boundary - 1, boundary - 1));
        committed(f.run("post-ack-delay", action, boundary, boundary));
    }
}

#[test]
fn policy_edit_cancel_withdraw_and_rebase_durability_matrix() {
    for operation in ["cancel", "withdraw", "rebase"] {
        for fault in [
            TestAppendFault::PartialFrame,
            TestAppendFault::CompleteFrameBeforeDurabilityAck,
            TestAppendFault::CompleteFrameAfterDurabilityAck,
        ] {
            let mut f = Fixture::new();
            f.enroll();
            let at = if operation == "withdraw" {
                disable(&mut f);
                RECOVERY_DELAY_MS
            } else {
                0
            };
            let next = edit(&f, "pending-edit", operation == "withdraw", at);
            let proposal = proposal(&f, next.clone(), at);
            committed(f.run("propose", propose_action(proposal), at, at));
            if operation == "rebase" {
                f = f.reopen();
            }
            let action = match operation {
                "cancel" => Action::CancelPolicy {
                    owner_signature: None,
                    approvals: votes(&f, &next, Purpose::Cancel, at + 1),
                },
                "withdraw" => Action::CancelPolicy {
                    owner_signature: Some(sign(
                        2,
                        &f.recovery().policy_cancel_signing_message().unwrap(),
                    )),
                    approvals: Vec::new(),
                },
                _ => Action::RebaseWait {},
            };
            let command = f.command(action);
            let before = f.recovery();
            let clock = RecoveryClockV1 {
                writer_epoch: f
                    .writer
                    .cached_replay()
                    .unwrap()
                    .current_authority
                    .as_ref()
                    .unwrap()
                    .authority_epoch
                    .clone(),
                wall_time: time(at + 1),
                monotonic_ms: at + 1,
            };
            let expected = evaluate_guardian_recovery(
                Some(&before),
                &command,
                &clock,
                &authority_state_hash(&f.state()).unwrap(),
            )
            .unwrap();
            f.writer.inject_append_fault_after_for_test(0, fault);
            assert!(matches!(
                f.writer.execute_guardian_recovery_with_clock(
                    "terminal".into(),
                    command.clone(),
                    Some((time(at + 1), at + 1))
                ),
                AuthorityMutationResultV1::CommitUnknown { .. }
            ));
            let mut f = f.reopen();
            if fault == TestAppendFault::PartialFrame {
                assert_eq!(f.recovery(), before);
                assert!(f.writer.recovery_status.is_some());
            } else {
                assert_eq!(f.recovery(), expected);
                assert!(matches!(
                    f.writer.execute_guardian_recovery_with_clock(
                        "terminal".into(),
                        command,
                        Some((time(at + 2), 0))
                    ),
                    AuthorityMutationResultV1::CommittedProjectionPending {
                        resolution: AuthorityMutationResolutionV1::ResolvedExistingFact,
                        ..
                    }
                ));
                assert_eq!(f.recovery(), expected);
            }
        }
    }
}

#[test]
fn policy_edit_corpus_records_replay_and_can_be_exported_explicitly() {
    let mut f = Fixture::new();
    f.enroll();
    let prior = f.state();
    let next = proposal(&f, edit(&f, "disable-1", false, 0), 0);
    committed(f.run("propose-disable", propose_action(next), 0, 0));
    let change = f.writer.cached_replay().unwrap().mutations["propose-disable"]
        .fact
        .changes[0]
        .clone();
    let mut corpus = vec![(
        "policy-disable-proposal.json",
        serde_json::to_vec(&(prior, change)).unwrap(),
    )];
    committed(f.run(
        "disable",
        Action::ActivatePolicy {},
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    let prior = f.state();
    let mut next = edit(&f, "reenable", true, RECOVERY_DELAY_MS);
    new_members(&mut next, true);
    let next = proposal(&f, next, RECOVERY_DELAY_MS);
    committed(f.run(
        "propose-reenable",
        propose_action(next),
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    let change = f.writer.cached_replay().unwrap().mutations["propose-reenable"]
        .fact
        .changes[0]
        .clone();
    corpus.push((
        "policy-reenable-proposal.json",
        serde_json::to_vec(&(prior, change)).unwrap(),
    ));
    for (_, bytes) in &corpus {
        let (mut state, change): (AuthorityStateV1, AuthorityChangeV1) =
            serde_json::from_slice(bytes).unwrap();
        apply_authority_changes(&mut state, &[change]).unwrap();
    }
    committed(f.run(
        "reenable",
        Action::ActivatePolicy {},
        RECOVERY_DELAY_MS * 2,
        RECOVERY_DELAY_MS * 2,
    ));
    let entries = f.writer.entries().unwrap();
    assert_eq!(replay_authority_entries(&entries).unwrap().state, f.state());
    corpus.push(("policy-prefix.json", serde_json::to_vec(&entries).unwrap()));
    if let Ok(dir) = std::env::var("MCT_RECOVERY_CORPUS_OUT") {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, bytes) in corpus {
            std::fs::write(Path::new(&dir).join(name), bytes).unwrap();
        }
    }
}

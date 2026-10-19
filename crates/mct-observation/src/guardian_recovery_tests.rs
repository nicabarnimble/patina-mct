use super::*;
#[path = "admin_issuance_tests.rs"]
mod admin_issuance_tests;
#[path = "guardian_policy_tests.rs"]
mod policy_tests;
use mct_kernel::recovery::GuardianRecoveryActionV1 as Action;
use mct_kernel::{MctNodeId, recovery::*};

fn key(seed: u8) -> AdminHybridSigningKey {
    AdminHybridSigningKey::from_seeds(&[seed; 32], &[seed + 64; 32])
}
fn public(seed: u8) -> RecoveryPublicKeyV1 {
    RecoveryPublicKeyV1 {
        ed25519: key(seed).public_half(),
        ml_dsa65: key(seed).ml_dsa65_public_key(),
    }
}
fn sign(seed: u8, message: &[u8]) -> RecoverySignatureV1 {
    key(seed).sign_hex(message).try_into().unwrap()
}
fn time(ms: u64) -> Timestamp {
    let base: jiff::Timestamp = "2026-10-19T00:00:00Z".parse().unwrap();
    Timestamp::new(
        base.checked_add(std::time::Duration::from_millis(ms))
            .unwrap()
            .to_string(),
    )
    .unwrap()
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

struct Fixture {
    _dir: tempfile::TempDir,
    path: PathBuf,
    writer: JsonlObservationLedger,
    enrollment: GuardianEnrollmentV1,
}
impl Fixture {
    fn new() -> Self {
        Self::with_strict_origin(false)
    }
    fn with_strict_origin(strict: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.jsonl");
        let mut writer = if strict {
            crate::admin_genesis_tests::bootstrap_writer(&path).0
        } else {
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap()
        };
        if !strict {
            let mut changes = Vec::new();
            for (id, seed, class) in [
                ("day-a", 20, AdminKeyGrantClassV1::DayToDay),
                ("day-b", 21, AdminKeyGrantClassV1::DayToDay),
                ("root", 2, AdminKeyGrantClassV1::Recovery),
            ] {
                changes.push(AuthorityChangeV1::AdminKeyGrantPut {
                    grant: Box::new(AdminKeyGrantV1 {
                        grant_id: id.into(),
                        issuance_ceiling: None,
                        public_half: public(seed).ed25519,
                        ml_dsa65_public_key: Some(public(seed).ml_dsa65),
                        class,
                        scopes: if class == AdminKeyGrantClassV1::Recovery {
                            mct_kernel::recovery_grant_scopes().to_vec()
                        } else {
                            vec![AdminKeyScopeV1::Status]
                        },
                        expires_at: (class == AdminKeyGrantClassV1::DayToDay)
                            .then(|| time(RECOVERY_DELAY_MS * 30)),
                        grant_state: AdminKeyGrantStateV1::Active,
                    }),
                    signature: None,
                });
            }
            changes.push(AuthorityChangeV1::PairingTicketIssued {
                ticket_hash: "ab".into(),
                peer_endpoint_id: None,
                window_starts_at: None,
                window_expires_at: None,
                signature: None,
            });
            committed(writer.execute_authority_mutation(
                AuthorityMutationRequestV1 {
                    mutation_id: "seed-existing-admin".into(),
                    changes,
                    decided_at: time(0).to_string(),
                    grant_shaping_sources: vec![GrantShapingSourceV1::AdminKey {
                        public_half: public(2).ed25519,
                    }],
                },
                |_| Ok(None),
            ));
        }
        let genesis = VoiceGenesisV1 {
            public_key: public(1),
            possession: sign(1, &VoiceGenesisV1::signing_message(&public(1)).unwrap()),
        };
        let voice_id = genesis.verify().unwrap();
        let binding = OwnerBindingV1 {
            mother_node_id: MctNodeId::new("mother-a").unwrap(),
            ledger_id: "ledger-a".into(),
            epoch_id: writer
                .cached_replay()
                .unwrap()
                .current_authority
                .as_ref()
                .unwrap()
                .authority_epoch
                .clone(),
            binding_id: "owner-a".into(),
            voice_id,
            root_generation: 1,
            root_key: public(2),
        };
        let guardian_bindings = std::array::from_fn(|index| OwnerBindingV1 {
            mother_node_id: MctNodeId::new(format!("guardian-{index}")).unwrap(),
            ledger_id: format!("ledger-g-{index}"),
            epoch_id: format!("epoch-g-{index}"),
            binding_id: format!("owner-g-{index}"),
            voice_id,
            root_generation: 1,
            root_key: public(index as u8 + 10),
        });
        let policy = GuardianRecoveryPolicyV1 {
            generation: 1,
            target: binding,
            guardians: std::array::from_fn(|index| RecoveryGuardianV1 {
                mother_node_id: guardian_bindings[index].mother_node_id.clone(),
                owner_binding_id: guardian_bindings[index].binding_id.clone(),
                owner_binding_generation: 1,
                voice_id,
                signing_key: public(index as u8 + 3),
            }),
        };
        let bytes =
            GuardianEnrollmentV1::signing_message(&genesis, &policy, &guardian_bindings).unwrap();
        let enrollment = GuardianEnrollmentV1 {
            genesis,
            policy,
            guardian_bindings,
            genesis_authorization: sign(1, &bytes),
            owner_authorization: sign(2, &bytes),
            guardian_owner_acceptances: std::array::from_fn(|i| sign(i as u8 + 10, &bytes)),
            guardian_acceptances: std::array::from_fn(|i| sign(i as u8 + 3, &bytes)),
        };
        Self {
            _dir: dir,
            path,
            writer,
            enrollment,
        }
    }
    fn state(&self) -> AuthorityStateV1 {
        self.writer.cached_replay().unwrap().state.clone()
    }
    fn recovery(&self) -> GuardianRecoveryStateV1 {
        *self.state().guardian_recovery.unwrap()
    }
    fn command(&self, action: Action) -> GuardianRecoveryCommandV1 {
        GuardianRecoveryCommandV1 {
            expected_state: self
                .state()
                .guardian_recovery
                .as_ref()
                .map(|s| s.digest().unwrap()),
            action,
        }
    }
    fn run(&mut self, id: &str, action: Action, wall: u64, mono: u64) -> AuthorityMutationResultV1 {
        self.writer.execute_guardian_recovery_with_clock(
            id.into(),
            self.command(action),
            Some((time(wall), mono)),
        )
    }
    fn enroll(&mut self) {
        committed(self.run(
            "enroll",
            Action::Enroll {
                enrollment: Box::new(self.enrollment.clone()),
            },
            0,
            0,
        ));
    }
    fn request(&self) -> RecoveryRequestV1 {
        let state = self.recovery();
        let intent = RecoveryIntentV1 {
            request_id: "request-1".into(),
            target: state.binding,
            policy_generation: state.policy.generation,
            policy_digest: state.policy.digest().unwrap(),
            replacement_key: public(6),
            not_before: time(0),
            expires_at: time(RECOVERY_DELAY_MS + RECOVERY_COMPLETION_WINDOW_MS),
        };
        RecoveryRequestV1 {
            replacement_possession: sign(6, &intent.signing_message().unwrap()),
            intent,
        }
    }
    fn approvals(
        &self,
        request: &RecoveryRequestV1,
        purpose: RecoveryApprovalPurposeV1,
        at: u64,
    ) -> Vec<GuardianRecoveryApprovalV1> {
        let state = self.recovery();
        (0..2)
            .map(|i| {
                let guardian_node_id = state.policy.guardians[i].mother_node_id.clone();
                let issued_at = time(at);
                let expires_at = time(at + RECOVERY_APPROVAL_MAX_MS);
                let bytes = GuardianApprovalMessageV1 {
                    policy: &state.policy,
                    intent: &request.intent,
                    purpose: &purpose,
                    guardian_node_id: &guardian_node_id,
                    issued_at: &issued_at,
                    expires_at: &expires_at,
                }
                .signing_message()
                .unwrap();
                GuardianRecoveryApprovalV1 {
                    guardian_node_id,
                    issued_at,
                    expires_at,
                    signature: sign(i as u8 + 3, &bytes),
                }
            })
            .collect()
    }
    fn begin(&mut self) {
        let request = self.request();
        let approvals = self.approvals(&request, RecoveryApprovalPurposeV1::Initiate, 0);
        committed(self.run(
            "begin",
            Action::Initiate {
                request: Box::new(request),
                approvals,
            },
            0,
            0,
        ));
    }
    fn challenge(&mut self) {
        committed(self.run(
            "challenge",
            Action::IssueChallenge {
                challenge: "challenge-1".into(),
            },
            RECOVERY_DELAY_MS,
            RECOVERY_DELAY_MS,
        ));
    }
    fn completion(&self) -> Action {
        let state = self.recovery();
        let request = &state.pending.as_ref().unwrap().request;
        Action::Complete {
            approvals: self.approvals(
                request,
                RecoveryApprovalPurposeV1::Complete {
                    challenge: "challenge-1".into(),
                },
                RECOVERY_DELAY_MS,
            ),
        }
    }
    fn reopen(self) -> Self {
        let Self {
            _dir,
            path,
            writer,
            enrollment,
        } = self;
        drop(writer);
        let writer = JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        Self {
            _dir,
            path,
            writer,
            enrollment,
        }
    }
}

#[test]
fn canonical_recovery_replaces_root_and_all_day_grants_atomically_then_requires_new_root_snapshot()
{
    let mut f = Fixture::new();
    f.enroll();
    f.begin();
    f.challenge();
    committed(f.run(
        "complete",
        f.completion(),
        RECOVERY_DELAY_MS + 1,
        RECOVERY_DELAY_MS + 1,
    ));
    let state = f.state();
    let recovery = state.guardian_recovery.as_ref().unwrap();
    assert_eq!(recovery.binding.root_generation, 2);
    assert_eq!(recovery.policy.generation, 2);
    assert_eq!(recovery.binding.root_key, public(6));
    assert!(recovery.maintenance);
    assert!(recovery.pending.is_none());
    assert!(recovery.consumed_request_ids.contains("request-1"));
    assert_eq!(recovery.retired_root_keys, vec![public(2)]);
    assert!(!state.admin_key_grants.contains_key("root"));
    assert!(
        state
            .admin_key_grants
            .values()
            .filter(|g| g.class == AdminKeyGrantClassV1::DayToDay)
            .all(|g| g.grant_state == AdminKeyGrantStateV1::Revoked)
    );
    assert_eq!(
        state.pairing_tickets["ab"].state,
        PairingTicketStateV1::Consumed
    );
    let snapshot = authority_state_hash(&state).unwrap();
    let message = recovery.resume_signing_message(&snapshot).unwrap();
    denied(f.run(
        "old-root-resume",
        Action::Resume {
            authority_snapshot: snapshot.clone(),
            signature: sign(2, &message),
        },
        RECOVERY_DELAY_MS + 2,
        RECOVERY_DELAY_MS + 2,
    ));
    denied(f.run(
        "wrong-snapshot",
        Action::Resume {
            authority_snapshot: "f".repeat(64),
            signature: sign(6, &message),
        },
        RECOVERY_DELAY_MS + 2,
        RECOVERY_DELAY_MS + 2,
    ));
    committed(f.run(
        "resume",
        Action::Resume {
            authority_snapshot: snapshot,
            signature: sign(6, &message),
        },
        RECOVERY_DELAY_MS + 2,
        RECOVERY_DELAY_MS + 2,
    ));
    assert!(!f.recovery().maintenance);
    assert_eq!(
        replay_authority_entries(&f.writer.entries().unwrap())
            .unwrap()
            .state,
        f.state()
    );
    let f = f.reopen();
    assert_eq!(f.recovery().binding.root_key, public(6));
}

#[test]
fn canonical_recovery_strict_origin_is_history_not_an_old_root_veto_or_grant() {
    let mut f = Fixture::with_strict_origin(true);
    let origin = f.state().admin_genesis.unwrap();
    let too_early = f.command(Action::Enroll {
        enrollment: Box::new(f.enrollment.clone()),
    });
    denied(f.writer.execute_guardian_recovery_with_clock(
        "before-birth".into(),
        too_early,
        Some((Timestamp::new("2026-10-18T23:59:59Z").unwrap(), 0)),
    ));
    f.enroll();
    let stale = crate::admin_genesis_tests::leaf_request(&f.writer, 1);
    f.begin();
    f.challenge();
    committed(f.run(
        "complete",
        f.completion(),
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    assert_eq!(f.state().admin_genesis.as_ref(), Some(&origin));
    assert!(!f.state().admin_key_grants.contains_key("root"));
    denied(
        f.writer
            .execute_admin_key_issuance_at(stale.clone(), time(RECOVERY_DELAY_MS)),
    );
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
    let mut new_issue = stale;
    let replay = f.writer.cached_replay().unwrap();
    new_issue.intent.expected_authority = AuthorityStateReferenceV1 {
        grants_authority: replay.current_authority.clone().unwrap(),
        authority_state_hash: authority_state_hash(&replay.state).unwrap(),
    };
    new_issue.intent.issuer_grant_id = "recovered:owner-a:2".into();
    new_issue.intent.grant.expires_at = Some(time(2 * RECOVERY_DELAY_MS));
    new_issue.signature.signer_grant_id = new_issue.intent.issuer_grant_id.clone();
    let message = admin_change_signing_message(
        "mother-a",
        "ledger-a",
        1,
        &replay.state.admin_key_grants["recovered:owner-a:2"],
        &AuthorityChangeV1::AdminKeyIssue {
            request: Box::new(new_issue.clone()),
            decided_at: time(RECOVERY_DELAY_MS),
        },
    )
    .unwrap();
    new_issue.signature.signature = key(6).sign_hex(&message);
    committed(
        f.writer
            .execute_admin_key_issuance_at(new_issue, time(RECOVERY_DELAY_MS)),
    );
    let entries = f.writer.entries().unwrap();
    assert_eq!(
        f.writer
            .execute_admin_genesis_at(
                &crate::admin_genesis_tests::setup(),
                origin.request.clone(),
                time(RECOVERY_DELAY_MS)
            )
            .unwrap(),
        entries[0]
    );
    assert_eq!(f.writer.entries().unwrap(), entries);
    assert_eq!(f.reopen().state().admin_genesis.as_ref(), Some(&origin));
}

#[test]
fn canonical_recovery_requires_all_enrollment_acceptances_and_current_canonical_root() {
    for which in 0..8 {
        let mut f = Fixture::new();
        let mut enrollment = f.enrollment.clone();
        let bad = sign(30, b"not-enrollment");
        match which {
            0 => enrollment.genesis_authorization = bad,
            1 => enrollment.owner_authorization = bad,
            2..=4 => enrollment.guardian_acceptances[which - 2] = bad,
            _ => enrollment.guardian_owner_acceptances[which - 5] = bad,
        }
        let before = f.state();
        denied(f.run(
            "bad-enroll",
            Action::Enroll {
                enrollment: Box::new(enrollment),
            },
            0,
            0,
        ));
        assert_eq!(f.state(), before);
    }
}

#[test]
fn canonical_recovery_stale_state_and_duplicate_enrollment_deny_without_writes() {
    let mut f = Fixture::new();
    let old = f.command(Action::Enroll {
        enrollment: Box::new(f.enrollment.clone()),
    });
    f.enroll();
    let count = f.writer.entries().unwrap().len();
    denied(f.writer.execute_guardian_recovery_with_clock(
        "other-enroll".into(),
        old,
        Some((time(0), 0)),
    ));
    denied(f.run(
        "reenroll",
        Action::Enroll {
            enrollment: Box::new(f.enrollment.clone()),
        },
        0,
        0,
    ));
    assert_eq!(f.writer.entries().unwrap().len(), count);
}

#[test]
fn canonical_recovery_wall_jump_or_monotonic_jump_alone_cannot_shorten_delay() {
    let mut f = Fixture::new();
    f.enroll();
    f.begin();
    for (wall, mono) in [
        (RECOVERY_DELAY_MS, 1),
        (1, RECOVERY_DELAY_MS),
        (RECOVERY_DELAY_MS - 1, RECOVERY_DELAY_MS - 1),
    ] {
        denied(f.run(
            "early",
            Action::IssueChallenge {
                challenge: "challenge-1".into(),
            },
            wall,
            mono,
        ));
    }
    f.challenge();
    denied(f.run(
        "rollback",
        f.completion(),
        RECOVERY_DELAY_MS - 1,
        RECOVERY_DELAY_MS + 1,
    ));
    committed(f.run(
        "boundary",
        f.completion(),
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
}

#[test]
fn canonical_recovery_restart_discards_challenge_and_requires_full_new_wait_without_extending_expiry()
 {
    let mut f = Fixture::new();
    f.enroll();
    f.begin();
    f.challenge();
    let completion = f.completion();
    let deadline = f.recovery().pending.unwrap().deadline;
    let mut f = f.reopen();
    denied(f.run(
        "old-complete",
        completion.clone(),
        RECOVERY_DELAY_MS + 1,
        RECOVERY_DELAY_MS + 1,
    ));
    committed(f.run("rebase", Action::RebaseWait {}, RECOVERY_DELAY_MS + 1, 0));
    let pending = f.recovery().pending.unwrap();
    assert!(pending.challenge.is_none());
    assert_eq!(pending.deadline, deadline);
    denied(f.run(
        "short-wait",
        Action::IssueChallenge {
            challenge: "challenge-2".into(),
        },
        RECOVERY_DELAY_MS * 2,
        RECOVERY_DELAY_MS - 1,
    ));
    denied(f.run(
        "reuse-challenge",
        Action::IssueChallenge {
            challenge: "challenge-1".into(),
        },
        RECOVERY_DELAY_MS * 2 + 1,
        RECOVERY_DELAY_MS,
    ));
    committed(f.run(
        "new-challenge",
        Action::IssueChallenge {
            challenge: "challenge-2".into(),
        },
        RECOVERY_DELAY_MS * 2 + 1,
        RECOVERY_DELAY_MS,
    ));
    denied(f.run(
        "stale-approvals",
        completion,
        RECOVERY_DELAY_MS * 2 + 1,
        RECOVERY_DELAY_MS,
    ));
}

#[test]
fn canonical_recovery_cancel_requires_quorum_and_keeps_consumed_id_and_maintenance() {
    let mut f = Fixture::new();
    f.enroll();
    f.begin();
    let request = f.recovery().pending.unwrap().request;
    let approvals = f.approvals(&request, RecoveryApprovalPurposeV1::Cancel, 0);
    denied(f.run(
        "one-cancel",
        Action::Cancel {
            approvals: approvals[..1].to_vec(),
        },
        1,
        1,
    ));
    denied(f.run("early-expire", Action::Expire {}, 1, 1));
    committed(f.run("cancel", Action::Cancel { approvals }, 1, 1));
    assert!(f.recovery().maintenance);
    assert!(f.recovery().pending.is_none());
    let approvals = f.approvals(&request, RecoveryApprovalPurposeV1::Initiate, 0);
    denied(f.run(
        "reuse-id",
        Action::Initiate {
            request: Box::new(request),
            approvals,
        },
        2,
        2,
    ));
    let snapshot = authority_state_hash(&f.state()).unwrap();
    let signature = sign(2, &f.recovery().resume_signing_message(&snapshot).unwrap());
    committed(f.run(
        "cancel-resume",
        Action::Resume {
            authority_snapshot: snapshot,
            signature,
        },
        2,
        2,
    ));
    assert!(!f.recovery().maintenance);
}

#[test]
fn canonical_recovery_expiry_is_terminal_and_does_not_resume_work() {
    let mut f = Fixture::new();
    f.enroll();
    f.begin();
    let request = f.recovery().pending.unwrap().request;
    let deadline = RECOVERY_DELAY_MS + RECOVERY_COMPLETION_WINDOW_MS;
    let approvals = f.approvals(&request, RecoveryApprovalPurposeV1::Cancel, 0);
    denied(f.run(
        "expired-cancel",
        Action::Cancel { approvals },
        deadline,
        deadline,
    ));
    denied(f.run(
        "expired-challenge",
        Action::IssueChallenge {
            challenge: "late".into(),
        },
        deadline,
        deadline,
    ));
    committed(f.run("expire", Action::Expire {}, deadline, deadline));
    assert!(f.recovery().maintenance);
    assert!(f.recovery().pending.is_none());
    assert!(f.recovery().consumed_request_ids.contains("request-1"));
    assert_eq!(f.recovery().binding.root_key, public(2));
}

#[test]
fn canonical_recovery_reissued_challenge_invalidates_old_quorum_and_old_nonce() {
    let mut f = Fixture::new();
    f.enroll();
    f.begin();
    f.challenge();
    let old = f.completion();
    denied(f.run(
        "challenge-churn",
        Action::IssueChallenge {
            challenge: "challenge-2".into(),
        },
        RECOVERY_DELAY_MS + 1,
        RECOVERY_DELAY_MS + 1,
    ));
    let after_expiry = RECOVERY_DELAY_MS + RECOVERY_APPROVAL_MAX_MS;
    committed(f.run(
        "challenge2",
        Action::IssueChallenge {
            challenge: "challenge-2".into(),
        },
        after_expiry,
        after_expiry,
    ));
    denied(f.run("old-quorum", old, after_expiry + 1, after_expiry + 1));
    denied(f.run(
        "challenge3",
        Action::IssueChallenge {
            challenge: "challenge-1".into(),
        },
        after_expiry + RECOVERY_APPROVAL_MAX_MS,
        after_expiry + RECOVERY_APPROVAL_MAX_MS,
    ));
}

#[test]
fn canonical_recovery_completion_acks_lost_on_either_side_of_sync_replay_exactly_once() {
    for fault in [
        TestAppendFault::CompleteFrameBeforeDurabilityAck,
        TestAppendFault::CompleteFrameAfterDurabilityAck,
    ] {
        let mut f = Fixture::new();
        f.enroll();
        f.begin();
        f.challenge();
        let command = f.command(f.completion());
        f.writer.inject_append_fault_after_for_test(0, fault);
        let result = f.writer.execute_guardian_recovery_with_clock(
            "complete".into(),
            command.clone(),
            Some((time(RECOVERY_DELAY_MS), RECOVERY_DELAY_MS)),
        );
        assert!(
            matches!(result, AuthorityMutationResultV1::CommitUnknown { .. }),
            "{result:?}"
        );
        assert!(f.writer.is_poisoned());
        let mut f = f.reopen();
        assert_eq!(f.recovery().binding.root_generation, 2);
        let result = f.writer.execute_guardian_recovery_with_clock(
            "complete".into(),
            command,
            Some((time(RECOVERY_DELAY_MS + 1), 0)),
        );
        assert!(
            matches!(
                result,
                AuthorityMutationResultV1::CommittedProjectionPending {
                    resolution: AuthorityMutationResolutionV1::ResolvedExistingFact,
                    ..
                }
            ),
            "{result:?}"
        );
        assert_eq!(f.recovery().retired_root_keys.len(), 1);
        assert!(f.recovery().maintenance);
    }
}

#[test]
fn canonical_recovery_partial_frame_preserves_residue_without_consuming_request() {
    let mut f = Fixture::new();
    f.enroll();
    let request = f.request();
    let approvals = f.approvals(&request, RecoveryApprovalPurposeV1::Initiate, 0);
    f.writer
        .inject_append_fault_after_for_test(0, TestAppendFault::PartialFrame);
    let result = f.run(
        "begin",
        Action::Initiate {
            request: Box::new(request.clone()),
            approvals: approvals.clone(),
        },
        0,
        0,
    );
    assert!(matches!(
        result,
        AuthorityMutationResultV1::CommitUnknown { .. }
    ));
    let mut f = f.reopen();
    assert!(f.recovery().pending.is_none());
    assert!(f.recovery().consumed_request_ids.is_empty());
    assert!(f.writer.recovery_status.is_some());
    committed(f.run("rebase", Action::RebaseWait {}, 1, 0));
    committed(f.run(
        "begin",
        Action::Initiate {
            request: Box::new(request),
            approvals,
        },
        1,
        0,
    ));
    assert_eq!(f.recovery().consumed_request_ids.len(), 1);
}

#[test]
fn canonical_recovery_cannot_mix_with_ordinary_mutations_or_use_legacy_writer_ingress() {
    let mut f = Fixture::new();
    let recovery_change = AuthorityChangeV1::GuardianRecovery {
        command: Box::new(f.command(Action::Enroll {
            enrollment: Box::new(f.enrollment.clone()),
        })),
        clock: RecoveryClockV1 {
            writer_epoch: f.enrollment.policy.target.epoch_id.clone(),
            wall_time: time(0),
            monotonic_ms: 0,
        },
    };
    let mut state = f.state();
    let original = state.clone();
    assert!(
        apply_authority_changes(
            &mut state,
            &[
                recovery_change.clone(),
                AuthorityChangeV1::ToyCatalogRemove { toy_id: "x".into() }
            ]
        )
        .is_err()
    );
    assert_eq!(state, original);
    denied(f.writer.execute_authority_mutation(
        AuthorityMutationRequestV1 {
            mutation_id: "bypass".into(),
            changes: vec![recovery_change],
            grant_shaping_sources: vec![GrantShapingSourceV1::AdminKey {
                public_half: public(2).ed25519,
            }],
            decided_at: time(0).to_string(),
        },
        |_| panic!("no projection on denial"),
    ));
    f.enroll();
    assert!(
        apply_authority_changes(
            &mut f.state(),
            &[AuthorityChangeV1::AdminKeyGrantRevoke {
                grant_id: "root".into(),
                signature: None
            }]
        )
        .is_err()
    );
    assert!(
        apply_authority_changes(
            &mut f.state(),
            &[AuthorityChangeV1::AdminKeyGrantRevoke {
                grant_id: "day-a".into(),
                signature: None
            }]
        )
        .is_err()
    );
}

#[test]
fn canonical_recovery_absent_hash_is_identical_and_new_hash_covers_replay_protection() {
    // AuthorityState travels through resident async commands. A large optional
    // recovery projection must not inflate every legacy future/stack frame.
    assert!(std::mem::size_of::<AuthorityStateV1>() <= 256);
    assert_eq!(
        authority_state_hash(&AuthorityStateV1::default()).unwrap(),
        blake3::hash(br#"{"toy_catalog":[],"toy_grants":[]}"#)
            .to_hex()
            .to_string()
    );
    let mut f = Fixture::new();
    let old = authority_state_hash(&f.state()).unwrap();
    f.enroll();
    let enrolled = authority_state_hash(&f.state()).unwrap();
    assert_ne!(old, enrolled);
    let mut altered = f.state();
    altered
        .guardian_recovery
        .as_mut()
        .unwrap()
        .consumed_request_ids
        .insert("extra-consumed-id".into());
    assert_ne!(authority_state_hash(&altered).unwrap(), enrolled);
    let wire = serde_json::to_string(&AuthorityStateV1::default()).unwrap();
    assert!(!wire.contains("guardian_recovery"));
}

#[test]
fn canonical_recovery_parser_rejects_unknown_and_duplicate_fields_in_commands() {
    for json in [
        r#"{"expected_state":null,"action":{"action":"expire","extra":1}}"#,
        r#"{"expected_state":null,"expected_state":null,"action":{"action":"expire"}}"#,
        r#"{"expected_state":null,"action":{"action":"rebase_wait","monotonic_ms":86400000}}"#,
        r#"{"expected_state":null,"action":{"action":"future_action"}}"#,
    ] {
        assert!(
            serde_json::from_str::<GuardianRecoveryCommandV1>(json).is_err(),
            "{json}"
        );
    }
}

#[test]
fn canonical_recovery_owner_cannot_resume_around_pending_quorum() {
    let mut f = Fixture::new();
    f.enroll();
    f.begin();
    let snapshot = authority_state_hash(&f.state()).unwrap();
    let signature = sign(2, &f.recovery().resume_signing_message(&snapshot).unwrap());
    denied(f.run(
        "owner-veto",
        Action::Resume {
            authority_snapshot: snapshot,
            signature,
        },
        1,
        1,
    ));
    assert!(f.recovery().pending.is_some());
    assert!(f.recovery().maintenance);
}

#[test]
fn canonical_recovery_snapshot_approval_does_not_survive_a_concurrent_rebase() {
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
    let mut f = f.reopen();
    committed(f.run("rebase", Action::RebaseWait {}, RECOVERY_DELAY_MS + 1, 0));
    denied(f.run(
        "stale-review",
        Action::Resume {
            authority_snapshot: snapshot,
            signature,
        },
        RECOVERY_DELAY_MS + 2,
        1,
    ));
    assert!(f.recovery().maintenance);
}

#[test]
fn canonical_recovery_enrollment_targets_actual_root_and_actual_ledger_tenure() {
    let f = Fixture::new();
    f.enrollment.verify().unwrap();
    let command = f.command(Action::Enroll {
        enrollment: Box::new(f.enrollment.clone()),
    });
    let clock = RecoveryClockV1 {
        writer_epoch: f.enrollment.policy.target.epoch_id.clone(),
        wall_time: time(0),
        monotonic_ms: 0,
    };
    let change = AuthorityChangeV1::GuardianRecovery {
        command: Box::new(command),
        clock,
    };
    let authority = f
        .writer
        .cached_replay()
        .unwrap()
        .current_authority
        .clone()
        .unwrap();
    assert!(
        guardian_recovery::validate_context(
            &f.state(),
            std::slice::from_ref(&change),
            &authority,
            "wrong-ledger",
            time(0).as_str()
        )
        .is_err()
    );
    let mut wrong_epoch = authority;
    wrong_epoch.authority_epoch = "other-writer".into();
    assert!(
        guardian_recovery::validate_context(
            &f.state(),
            std::slice::from_ref(&change),
            &wrong_epoch,
            "ledger-a",
            time(0).as_str()
        )
        .is_err()
    );
    let mut wrong_root = f.state();
    wrong_root
        .admin_key_grants
        .get_mut("root")
        .unwrap()
        .public_half = public(30).ed25519;
    assert!(apply_authority_changes(&mut wrong_root, &[change]).is_err());
}

#[test]
fn canonical_recovery_corpus_records_replay() {
    let mut f = Fixture::new();
    let mut corpus = Vec::new();
    let prior = f.state();
    f.enroll();
    let change = f.writer.cached_replay().unwrap().mutations["enroll"]
        .fact
        .changes[0]
        .clone();
    corpus.push(("enroll.json", serde_json::to_vec(&(prior, change)).unwrap()));
    let prior = f.state();
    f.begin();
    let change = f.writer.cached_replay().unwrap().mutations["begin"]
        .fact
        .changes[0]
        .clone();
    corpus.push((
        "initiate.json",
        serde_json::to_vec(&(prior, change)).unwrap(),
    ));
    f.challenge();
    let prior = f.state();
    committed(f.run(
        "complete",
        f.completion(),
        RECOVERY_DELAY_MS,
        RECOVERY_DELAY_MS,
    ));
    let change = f.writer.cached_replay().unwrap().mutations["complete"]
        .fact
        .changes[0]
        .clone();
    corpus.push((
        "complete.json",
        serde_json::to_vec(&(prior, change)).unwrap(),
    ));
    for (_, bytes) in &corpus {
        let (mut prior, change): (AuthorityStateV1, AuthorityChangeV1) =
            serde_json::from_slice(bytes).unwrap();
        apply_authority_changes(&mut prior, &[change]).unwrap();
    }
    let entries = f.writer.entries().unwrap();
    assert_eq!(replay_authority_entries(&entries).unwrap().state, f.state());
    corpus.push((
        "canonical-prefix.json",
        serde_json::to_vec(&entries).unwrap(),
    ));
    // Explicit test-only fixture export; normal tests never write repository files.
    if let Ok(dir) = std::env::var("MCT_RECOVERY_CORPUS_OUT") {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, bytes) in corpus {
            std::fs::write(Path::new(&dir).join(name), bytes).unwrap();
        }
    }
}

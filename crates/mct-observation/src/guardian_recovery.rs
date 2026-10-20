//! Exclusive canonical recovery transitions. No ingress or live activation API.
use super::*;
use mct_kernel::recovery::{
    GuardianRecoveryActionV1 as Action, RecoveryDigestV1, evaluate_guardian_recovery,
};

/// Reserved for writer-generated replacement roots, never caller-issued day grants.
pub(super) const RECOVERED_ROOT_GRANT_PREFIX: &str = "recovered:";

pub(super) fn validate_state(state: &AuthorityStateV1) -> std::result::Result<(), String> {
    let Some(recovery) = &state.guardian_recovery else {
        return Ok(());
    };
    recovery.policy.validate().map_err(|e| e.to_string())?;
    mct_kernel::recovery::guardian_policy_state_is_consistent(recovery)
        .map_err(|e| e.to_string())?;
    if recovery.binding != recovery.policy.target
        || recovery.binding.root_generation != recovery.retired_root_keys.len() as u64 + 1
        || recovery.pending.is_some() && !recovery.maintenance
        || !state.admin_key_grants.values().any(|grant| {
            grant.class == AdminKeyGrantClassV1::Recovery
                && grant.grant_state == AdminKeyGrantStateV1::Active
                && grant.public_half == recovery.binding.root_key.ed25519
                && grant.ml_dsa65_public_key.as_ref() == Some(&recovery.binding.root_key.ml_dsa65)
        })
    {
        return Err("recovery projection disagrees with canonical root/policy".into());
    }
    Ok(())
}

pub(super) fn validate_context(
    state: &AuthorityStateV1,
    changes: &[AuthorityChangeV1],
    authority: &GrantsAuthorityIdentityV1,
    ledger_id: &str,
    decided_at: &str,
) -> std::result::Result<(), String> {
    for change in changes {
        if let AuthorityChangeV1::GuardianRecovery { command, clock } = change {
            if clock.writer_epoch != authority.authority_epoch
                || clock.wall_time.as_str() != decided_at
            {
                return Err("recovery clock is not this canonical writer tenure/time".into());
            }
            let binding = match &command.action {
                Action::Enroll { enrollment } => &enrollment.policy.target,
                _ => {
                    &state
                        .guardian_recovery
                        .as_ref()
                        .ok_or("recovery is not enrolled")?
                        .binding
                }
            };
            if binding.mother_node_id.as_str() != authority.mother_node_id
                || binding.ledger_id != ledger_id
                || matches!(command.action, Action::Enroll { .. })
                    && binding.epoch_id != authority.authority_epoch
            {
                return Err(
                    "recovery targets a different Mother, ledger or enrollment epoch".into(),
                );
            }
        }
    }
    Ok(())
}

pub(super) fn apply(
    state: &mut AuthorityStateV1,
    command: &GuardianRecoveryCommandV1,
    clock: &RecoveryClockV1,
) -> std::result::Result<(), AuthorityReplayError> {
    let denied = |e: &str| authority_incoherent(e);
    let before_hash = authority_state_hash(state).map_err(|e| denied(&e.to_string()))?;
    if let Some(birth) = &state.admin_genesis {
        let floor = state
            .admin_issuance
            .values()
            .map(|record| &record.decided_at)
            .chain(std::iter::once(&birth.decided_at))
            .max()
            .expect("birth supplies floor");
        if &clock.wall_time < floor {
            return Err(denied(
                "strict recovery clock regressed behind canonical authority",
            ));
        }
    }
    let root = state
        .admin_key_grants
        .values()
        .find(|grant| grant.class == AdminKeyGrantClassV1::Recovery)
        .ok_or_else(|| denied("recovery enrollment requires an existing canonical root"))?
        .clone();
    if let Action::Enroll { enrollment } = &command.action
        && (root.public_half != enrollment.policy.target.root_key.ed25519
            || root.ml_dsa65_public_key.as_ref()
                != Some(&enrollment.policy.target.root_key.ml_dsa65))
    {
        return Err(denied(
            "enrollment is not authorized by the canonical hybrid root",
        ));
    }
    if let Action::Initiate { request, .. } = &command.action
        && state.admin_key_grants.values().any(|grant| {
            grant.public_half == request.intent.replacement_key.ed25519
                || grant.ml_dsa65_public_key.as_ref()
                    == Some(&request.intent.replacement_key.ml_dsa65)
        })
    {
        return Err(denied(
            "replacement reuses an existing administrative credential",
        ));
    }
    let next = evaluate_guardian_recovery(
        state.guardian_recovery.as_deref(),
        command,
        clock,
        &before_hash,
    )
    .map_err(|e| denied(&e.to_string()))?;
    let mut proposed = state.clone();
    if matches!(command.action, Action::Complete { .. }) {
        // One canonical fact replaces the root and revokes ALL old day grants.
        // The old root's complete record remains in the append-only history.
        let grant_id = format!(
            "{RECOVERED_ROOT_GRANT_PREFIX}{}:{}",
            next.binding.binding_id, next.binding.root_generation
        );
        if proposed.admin_key_grants.contains_key(&grant_id) {
            return Err(denied("replacement root grant identity already exists"));
        }
        proposed.admin_key_grants.remove(&root.grant_id);
        for grant in proposed.admin_key_grants.values_mut() {
            grant.grant_state = AdminKeyGrantStateV1::Revoked;
        }
        proposed.admin_key_grants.insert(
            grant_id.clone(),
            AdminKeyGrantV1 {
                grant_id,
                issuance_ceiling: None,
                public_half: next.binding.root_key.ed25519,
                ml_dsa65_public_key: Some(next.binding.root_key.ml_dsa65.clone()),
                class: AdminKeyGrantClassV1::Recovery,
                scopes: mct_kernel::recovery_grant_scopes().to_vec(),
                expires_at: None,
                grant_state: AdminKeyGrantStateV1::Active,
            },
        );
        for ticket in proposed.pairing_tickets.values_mut() {
            ticket.state = PairingTicketStateV1::Consumed;
        }
    }
    proposed.guardian_recovery = Some(Box::new(next));
    authority_state_hash(&proposed).map_err(|e| denied(&e.to_string()))?;
    *state = proposed;
    Ok(())
}

impl JsonlObservationLedger {
    /// Commit a signed recovery operation using writer-owned time. This storage
    /// API is NOT an ingress authorization boundary; daemon activation is blocked.
    /// Host-only transitions use the dedicated methods below.
    pub fn execute_guardian_recovery(
        &mut self,
        mutation_id: String,
        command: GuardianRecoveryCommandV1,
    ) -> AuthorityMutationResultV1 {
        if matches!(
            command.action,
            Action::RebaseWait {}
                | Action::IssueChallenge { .. }
                | Action::Expire {}
                | Action::ActivatePolicy {}
        ) {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::InvalidRequest,
            );
        }
        self.execute_guardian_recovery_with_clock(mutation_id, command, None)
    }

    /// Activate a previously authorized policy edit after the writer-owned delay.
    pub fn activate_guardian_policy(
        &mut self,
        mutation_id: String,
        expected_state: RecoveryDigestV1,
    ) -> AuthorityMutationResultV1 {
        self.execute_guardian_recovery_with_clock(
            mutation_id,
            GuardianRecoveryCommandV1 {
                expected_state: Some(expected_state),
                action: Action::ActivatePolicy {},
            },
            None,
        )
    }

    /// Record a new writer tenure conservatively, with zero inherited wait credit.
    pub fn rebase_guardian_recovery_clock(
        &mut self,
        mutation_id: String,
        expected_state: RecoveryDigestV1,
    ) -> AuthorityMutationResultV1 {
        self.execute_guardian_recovery_with_clock(
            mutation_id,
            GuardianRecoveryCommandV1 {
                expected_state: Some(expected_state),
                action: Action::RebaseWait {},
            },
            None,
        )
    }

    /// Expiry never restores work or erases the consumed request ID.
    pub fn expire_guardian_recovery(
        &mut self,
        mutation_id: String,
        expected_state: RecoveryDigestV1,
    ) -> AuthorityMutationResultV1 {
        self.execute_guardian_recovery_with_clock(
            mutation_id,
            GuardianRecoveryCommandV1 {
                expected_state: Some(expected_state),
                action: Action::Expire {},
            },
            None,
        )
    }

    /// Fresh randomness plus exact canonical state/tenure. The challenge cannot
    /// be supplied by a remote caller or transplanted between authority snapshots.
    pub fn issue_guardian_recovery_challenge(
        &mut self,
        mutation_id: String,
        expected_state: RecoveryDigestV1,
    ) -> AuthorityMutationResultV1 {
        let mut random = [0u8; 32];
        let context = self.cached_replay().ok().and_then(|replay| {
            Some((
                authority_state_hash(&replay.state).ok()?,
                replay.current_authority.as_ref()?.authority_epoch.clone(),
            ))
        });
        let Some(context) = context else {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::PriorStateMismatch,
            );
        };
        if getrandom::fill(&mut random).is_err() {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::InvalidRequest,
            );
        }
        let Ok(bytes) =
            canonical_json_bytes(&("mct-recovery-writer-challenge/v1", context, random))
        else {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::InvalidRequest,
            );
        };
        let challenge = blake3::hash(&bytes).to_hex().to_string();
        self.execute_guardian_recovery_with_clock(
            mutation_id,
            GuardianRecoveryCommandV1 {
                expected_state: Some(expected_state),
                action: Action::IssueChallenge { challenge },
            },
            None,
        )
    }

    // The optional deterministic clock seam is private to the writer and tests.
    // Production callers always use the system/monotonic sample above.
    pub(super) fn execute_guardian_recovery_with_clock(
        &mut self,
        mutation_id: String,
        command: GuardianRecoveryCommandV1,
        injected: Option<(Timestamp, u64)>,
    ) -> AuthorityMutationResultV1 {
        let Some(tenure) = &self.authority_tenure else {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::AuthorityEpochUnavailable,
            );
        };
        let injected_clock = injected.is_some();
        let starts_wait = matches!(
            command.action,
            Action::Initiate { .. } | Action::ProposePolicy { .. } | Action::RebaseWait {}
        );
        let (wall_time, monotonic_ms) = injected.unwrap_or_else(|| {
            (
                Timestamp::new(jiff::Timestamp::now().to_string()).expect("system timestamp"),
                u64::try_from(self.recovery_clock_origin.elapsed().as_millis()).unwrap_or(u64::MAX),
            )
        });
        let mut clock = RecoveryClockV1 {
            writer_epoch: tenure.fact.authority_epoch.clone(),
            wall_time,
            monotonic_ms: monotonic_ms.saturating_sub(self.recovery_wait_offset_ms),
        };
        // A lost acknowledgement resolves against the exact committed command,
        // not a newly sampled clock. A different payload under the ID conflicts.
        if let Ok(replay) = self.cached_replay()
            && let Some(existing) = replay.mutations.get(&mutation_id)
        {
            match existing.fact.changes.as_slice() {
                [
                    AuthorityChangeV1::GuardianRecovery {
                        command: old,
                        clock: old_clock,
                    },
                ] if old.as_ref() == &command => clock = old_clock.clone(),
                _ => {
                    return rejected_mutation(
                        mutation_id,
                        AuthorityMutationRejectionReasonV1::MutationIdConflict,
                    );
                }
            }
        }
        let request = AuthorityMutationRequestV1 {
            mutation_id: mutation_id.clone(),
            decided_at: clock.wall_time.to_string(),
            changes: vec![AuthorityChangeV1::GuardianRecovery {
                command: Box::new(command),
                clock,
            }],
            // Correlation only. Cryptographic authorization is checked by the
            // recovery evaluator, never inferred from this descriptive source.
            grant_shaping_sources: vec![GrantShapingSourceV1::OperatorDecision {
                decision_id: mutation_id,
                authenticated_principal_ref: "verified-recovery-transcript".into(),
                command_kind: GrantShapingCommandKindV1::GrantChange,
            }],
        };
        let result = self.execute_authority_mutation_inner(request, |_| Ok(None), true);
        let newly_committed = matches!(
            result,
            AuthorityMutationResultV1::Committed {
                resolution: AuthorityMutationResolutionV1::NewlyCommitted,
                ..
            } | AuthorityMutationResultV1::CommittedProjectionPending {
                resolution: AuthorityMutationResolutionV1::NewlyCommitted,
                ..
            }
        );
        if newly_committed {
            let waiting = self
                .cached_replay()
                .ok()
                .and_then(|r| r.state.guardian_recovery.as_ref())
                .is_some_and(|r| r.pending.is_some() || r.pending_policy_edit().is_some());
            if !waiting {
                self.recovery_wait_offset_ms = 0;
            } else if starts_wait {
                let ack_delay = if injected_clock {
                    #[cfg(test)]
                    {
                        self.recovery_test_ack_delay_ms
                    }
                    #[cfg(not(test))]
                    {
                        0
                    }
                } else {
                    u64::try_from(
                        self.recovery_clock_origin
                            .elapsed()
                            .as_millis()
                            .saturating_add(1),
                    )
                    .unwrap_or(u64::MAX)
                    .saturating_sub(monotonic_ms)
                };
                // This conservative in-tenure floor is deliberately not portable.
                // Lost acknowledgement/reopen requires a new full wait instead.
                // Exact retries never reset it or credit the original disk stall.
                self.recovery_wait_offset_ms =
                    self.recovery_wait_offset_ms.saturating_add(ack_delay);
            }
        }
        result
    }
}

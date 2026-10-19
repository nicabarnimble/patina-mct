//! Deterministic recovery transitions. Only canonical replay may supply state.
//!
//! The caller owns serialization with other authority changes and BeforeEffect
//! durability. Clock readings are Mother evidence, never request authority.
use super::{internal::*, *};
use std::collections::BTreeSet;

/// Target appointment, Voice anchor consent, and all three guardian acceptances.
/// Each guardian owner signs its exact local binding as well as the target policy.
/// Live enrollment must additionally authenticate that binding at the guardian;
/// this record is not a substitute for same-Voice admission at either Mother.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardianEnrollmentV1 {
    pub genesis: VoiceGenesisV1,
    pub policy: GuardianRecoveryPolicyV1,
    pub guardian_bindings: [OwnerBindingV1; 3],
    pub genesis_authorization: RecoverySignatureV1,
    pub owner_authorization: RecoverySignatureV1,
    pub guardian_owner_acceptances: [RecoverySignatureV1; 3],
    pub guardian_acceptances: [RecoverySignatureV1; 3],
}

impl GuardianEnrollmentV1 {
    pub fn signing_message(
        genesis: &VoiceGenesisV1,
        policy: &GuardianRecoveryPolicyV1,
        guardian_bindings: &[OwnerBindingV1; 3],
    ) -> Result<Vec<u8>, RecoveryError> {
        transcript(
            "mct-guardian-enrollment/v1",
            &(genesis, policy, guardian_bindings),
        )
    }

    pub fn verify(&self) -> Result<(), RecoveryError> {
        self.policy.validate()?;
        if self.genesis.verify()? != self.policy.target.voice_id {
            return Err(RecoveryError::VoiceMismatch);
        }
        if self.policy.generation != 1 || self.policy.target.root_generation != 1 {
            return Err(RecoveryError::StaleAuthority);
        }
        let message = Self::signing_message(&self.genesis, &self.policy, &self.guardian_bindings)?;
        verify_signature(
            &self.genesis.public_key,
            &message,
            &self.genesis_authorization,
        )?;
        verify_signature(
            &self.policy.target.root_key,
            &message,
            &self.owner_authorization,
        )?;
        let mut keys = vec![&self.genesis.public_key, &self.policy.target.root_key];
        for (index, guardian) in self.policy.guardians.iter().enumerate() {
            let binding = &self.guardian_bindings[index];
            validate_binding(binding)?;
            if binding.mother_node_id != guardian.mother_node_id
                || binding.binding_id != guardian.owner_binding_id
                || binding.root_generation != guardian.owner_binding_generation
                || binding.voice_id != self.policy.target.voice_id
            {
                return Err(RecoveryError::StaleAuthority);
            }
            keys.push(&binding.root_key);
            keys.push(&guardian.signing_key);
            verify_signature(
                &binding.root_key,
                &message,
                &self.guardian_owner_acceptances[index],
            )?;
            verify_signature(
                &guardian.signing_key,
                &message,
                &self.guardian_acceptances[index],
            )?;
        }
        for (index, key) in keys.iter().enumerate() {
            if keys[..index]
                .iter()
                .any(|other| shares_key_half(key, other))
            {
                return Err(RecoveryError::DuplicatePrincipal);
            }
        }
        Ok(())
    }
}

/// Persisted Mother clock sample. The epoch is a writer tenure, NOT the stable
/// enrollment epoch in OwnerBinding. A new writer never inherits elapsed time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryClockV1 {
    pub writer_epoch: String,
    pub wall_time: Timestamp,
    pub monotonic_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryChallengeV1 {
    pub value: String,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingGuardianRecoveryV1 {
    pub request: RecoveryRequestV1,
    pub wait_origin: RecoveryClockV1,
    pub deadline: Timestamp,
    pub challenge: Option<RecoveryChallengeV1>,
}

/// A projection of validated transitions, never an enrollment or execution token.
/// Construction/deserialization alone is not canonical provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardianRecoveryStateV1 {
    pub enrollment: GuardianEnrollmentV1,
    pub binding: OwnerBindingV1,
    pub policy: GuardianRecoveryPolicyV1,
    pub revision: u64,
    pub clock: RecoveryClockV1,
    pub pending: Option<PendingGuardianRecoveryV1>,
    pub consumed_request_ids: BTreeSet<String>,
    pub consumed_challenges: BTreeSet<String>,
    pub retired_root_keys: Vec<RecoveryPublicKeyV1>,
    pub maintenance: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_edits: Option<Box<GuardianPolicyStateV1>>,
}

/// Strict wire commands: empty STRUCT variants deliberately reject extra fields.
/// RebaseWait, IssueChallenge and Expire are writer-generated, not remote actions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum GuardianRecoveryActionV1 {
    ProposePolicy {
        proposal: Box<GuardianPolicyProposalV1>,
    },
    CancelPolicy {
        owner_signature: Option<RecoverySignatureV1>,
        approvals: Vec<GuardianRecoveryApprovalV1>,
    },
    ActivatePolicy {},
    Enroll {
        enrollment: Box<GuardianEnrollmentV1>,
    },
    Initiate {
        request: Box<RecoveryRequestV1>,
        approvals: Vec<GuardianRecoveryApprovalV1>,
    },
    RebaseWait {},
    IssueChallenge {
        challenge: String,
    },
    Cancel {
        approvals: Vec<GuardianRecoveryApprovalV1>,
    },
    Expire {},
    Complete {
        approvals: Vec<GuardianRecoveryApprovalV1>,
    },
    Resume {
        authority_snapshot: String,
        signature: RecoverySignatureV1,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardianRecoveryCommandV1 {
    pub expected_state: Option<RecoveryDigestV1>,
    pub action: GuardianRecoveryActionV1,
}

impl GuardianRecoveryStateV1 {
    pub fn digest(&self) -> Result<RecoveryDigestV1, RecoveryError> {
        digest("mct-guardian-recovery-state/v1", self)
    }

    pub fn resume_signing_message(&self, snapshot: &str) -> Result<Vec<u8>, RecoveryError> {
        if snapshot.len() != 64
            || !snapshot
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(RecoveryError::Malformed);
        }
        transcript(
            "mct-guardian-recovery-resume/v1",
            &(self.digest()?, snapshot),
        )
    }
}

fn checked_deadline(time: &Timestamp, ms: u64) -> Result<Timestamp, RecoveryError> {
    time.checked_add_milliseconds(ms)
        .ok_or(RecoveryError::TimeInvalid)
}

fn wait_finished(
    pending: &PendingGuardianRecoveryV1,
    clock: &RecoveryClockV1,
) -> Result<(), RecoveryError> {
    if pending.wait_origin.writer_epoch != clock.writer_epoch
        || clock
            .monotonic_ms
            .checked_sub(pending.wait_origin.monotonic_ms)
            .is_none_or(|elapsed| elapsed < RECOVERY_DELAY_MS)
        || clock.wall_time < checked_deadline(&pending.wait_origin.wall_time, RECOVERY_DELAY_MS)?
        || clock.wall_time >= pending.deadline
    {
        return Err(RecoveryError::TimeInvalid);
    }
    Ok(())
}

/// Pure transition evaluator. Inputs must come from current canonical state and
/// Mother clock/snapshot. Denial leaves the input unchanged. Success is a proposed
/// state only; commit it atomically with root/day-grant changes before any effect.
pub fn evaluate_guardian_recovery(
    current: Option<&GuardianRecoveryStateV1>,
    command: &GuardianRecoveryCommandV1,
    clock: &RecoveryClockV1,
    authority_snapshot: &str,
) -> Result<GuardianRecoveryStateV1, RecoveryError> {
    validate_id(&clock.writer_epoch)?;
    if command.expected_state != current.map(GuardianRecoveryStateV1::digest).transpose()? {
        return Err(RecoveryError::StaleAuthority);
    }
    let Some(current) = current else {
        let GuardianRecoveryActionV1::Enroll { enrollment } = &command.action else {
            return Err(RecoveryError::StaleAuthority);
        };
        enrollment.verify()?;
        return Ok(GuardianRecoveryStateV1 {
            enrollment: *enrollment.clone(),
            binding: enrollment.policy.target.clone(),
            policy: enrollment.policy.clone(),
            revision: 1,
            clock: clock.clone(),
            pending: None,
            consumed_request_ids: BTreeSet::new(),
            consumed_challenges: BTreeSet::new(),
            retired_root_keys: Vec::new(),
            maintenance: false,
            policy_edits: None,
        });
    };
    let new_tenure = clock.writer_epoch != current.clock.writer_epoch;
    if clock.wall_time < current.clock.wall_time
        || (!new_tenure && clock.monotonic_ms < current.clock.monotonic_ms)
        || (new_tenure && !matches!(command.action, GuardianRecoveryActionV1::RebaseWait {}))
    {
        return Err(RecoveryError::TimeInvalid);
    }
    let mut next = current.clone();
    next.revision = next
        .revision
        .checked_add(1)
        .ok_or(RecoveryError::Malformed)?;
    match &command.action {
        GuardianRecoveryActionV1::ProposePolicy { proposal } => {
            super::policy::propose(current, &mut next, proposal, clock)?
        }
        GuardianRecoveryActionV1::CancelPolicy {
            owner_signature,
            approvals,
        } => super::policy::cancel(
            current,
            &mut next,
            owner_signature.as_ref(),
            approvals,
            clock,
        )?,
        GuardianRecoveryActionV1::ActivatePolicy {} => super::policy::activate(&mut next, clock)?,
        GuardianRecoveryActionV1::Enroll { .. } => return Err(RecoveryError::StaleAuthority),
        GuardianRecoveryActionV1::Initiate { request, approvals } => {
            if !next.recovery_enabled()
                || next.pending_policy_edit().is_some()
                || next.pending.is_some()
                || next
                    .consumed_request_ids
                    .contains(&request.intent.request_id)
            {
                return Err(RecoveryError::StaleAuthority);
            }
            // Never restore any retired credential, including one half of a pair.
            if next
                .retired_root_keys
                .iter()
                .any(|key| shares_key_half(key, &request.intent.replacement_key))
                || shares_key_half(
                    &next.enrollment.genesis.public_key,
                    &request.intent.replacement_key,
                )
                || next
                    .guardian_bindings()
                    .iter()
                    .any(|b| shares_key_half(&b.root_key, &request.intent.replacement_key))
                || next.policy_edits.as_ref().is_some_and(|edits| {
                    edits
                        .retired_guardian_keys
                        .iter()
                        .any(|key| shares_key_half(key, &request.intent.replacement_key))
                })
            {
                return Err(RecoveryError::ReplacementNotFresh);
            }
            verify_recovery_quorum(
                &next.binding,
                &next.policy,
                request,
                &RecoveryApprovalPurposeV1::Initiate,
                approvals,
                &clock.wall_time,
            )?;
            let earliest = checked_deadline(&clock.wall_time, RECOVERY_DELAY_MS)?;
            let deadline = checked_deadline(&earliest, RECOVERY_COMPLETION_WINDOW_MS)?
                .min(request.intent.expires_at.clone());
            if deadline <= earliest {
                return Err(RecoveryError::TimeInvalid);
            }
            next.consumed_request_ids
                .insert(request.intent.request_id.clone());
            next.pending = Some(PendingGuardianRecoveryV1 {
                request: *request.clone(),
                wait_origin: clock.clone(),
                deadline,
                challenge: None,
            });
            next.maintenance = true;
        }
        GuardianRecoveryActionV1::RebaseWait {} => {
            if !new_tenure {
                return Err(RecoveryError::TimeInvalid);
            }
            if let Some(pending) = &mut next.pending {
                // No credit for offline time or elapsed time from another boot.
                // The hard deadline never moves; frequent restarts may expire it.
                pending.wait_origin = clock.clone();
                pending.challenge = None;
            }
            if let Some(pending) = next.policy_edits.as_mut().and_then(|s| s.pending.as_mut()) {
                pending.wait_origin = clock.clone();
            }
        }
        GuardianRecoveryActionV1::IssueChallenge { challenge } => {
            validate_id(challenge)?;
            let pending = next.pending.as_mut().ok_or(RecoveryError::StaleAuthority)?;
            wait_finished(pending, clock)?;
            if pending
                .challenge
                .as_ref()
                .is_some_and(|old| clock.wall_time < old.expires_at)
                || !next.consumed_challenges.insert(challenge.clone())
            {
                return Err(RecoveryError::StaleAuthority);
            }
            pending.challenge = Some(RecoveryChallengeV1 {
                value: challenge.clone(),
                issued_at: clock.wall_time.clone(),
                expires_at: checked_deadline(&clock.wall_time, RECOVERY_APPROVAL_MAX_MS)?
                    .min(pending.deadline.clone()),
            });
        }
        GuardianRecoveryActionV1::Cancel { approvals } => {
            let pending = next.pending.as_ref().ok_or(RecoveryError::StaleAuthority)?;
            verify_recovery_quorum(
                &next.binding,
                &next.policy,
                &pending.request,
                &RecoveryApprovalPurposeV1::Cancel,
                approvals,
                &clock.wall_time,
            )?;
            next.pending = None;
            // Cancellation is not implicit approval of the authority snapshot.
        }
        GuardianRecoveryActionV1::Expire {} => {
            let pending = next.pending.as_ref().ok_or(RecoveryError::StaleAuthority)?;
            if clock.wall_time < pending.deadline {
                return Err(RecoveryError::TimeInvalid);
            }
            next.pending = None;
        }
        GuardianRecoveryActionV1::Complete { approvals } => {
            let pending = next.pending.as_ref().ok_or(RecoveryError::StaleAuthority)?;
            wait_finished(pending, clock)?;
            let challenge = pending
                .challenge
                .as_ref()
                .ok_or(RecoveryError::StaleAuthority)?;
            if clock.wall_time >= challenge.expires_at
                || approvals.iter().any(|approval| {
                    approval.issued_at < challenge.issued_at
                        || approval.expires_at > challenge.expires_at
                })
            {
                return Err(RecoveryError::TimeInvalid);
            }
            verify_recovery_quorum(
                &next.binding,
                &next.policy,
                &pending.request,
                &RecoveryApprovalPurposeV1::Complete {
                    challenge: challenge.value.clone(),
                },
                approvals,
                &clock.wall_time,
            )?;
            next.retired_root_keys.push(next.binding.root_key.clone());
            next.binding.root_generation = next
                .binding
                .root_generation
                .checked_add(1)
                .ok_or(RecoveryError::Malformed)?;
            next.binding.root_key = pending.request.intent.replacement_key.clone();
            next.policy.target = next.binding.clone();
            next.policy.generation = next
                .policy
                .generation
                .checked_add(1)
                .ok_or(RecoveryError::Malformed)?;
            next.pending = None;
            // maintenance remains set until exact-snapshot approval by NEW root.
        }
        GuardianRecoveryActionV1::Resume {
            authority_snapshot: expected,
            signature,
        } => {
            if next.pending.is_some() || !next.maintenance || expected != authority_snapshot {
                return Err(RecoveryError::StaleAuthority);
            }
            verify_signature(
                &current.binding.root_key,
                &current.resume_signing_message(expected)?,
                signature,
            )?;
            next.maintenance = false;
        }
    }
    next.clock = clock.clone();
    Ok(next)
}

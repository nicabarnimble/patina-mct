//! Protected roster edits and explicit release of guardian authority (D-54/55).
use super::{internal::*, *};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardianPolicyEditV1 {
    pub edit_id: String,
    pub policy: GuardianRecoveryPolicyV1,
    pub guardian_bindings: [OwnerBindingV1; 3],
    /// False explicitly terminates the former roster's authority at activation.
    pub enabled: bool,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardianPolicyAcceptanceV1 {
    pub guardian_node_id: MctNodeId,
    pub owner_signature: RecoverySignatureV1,
    pub guardian_signature: RecoverySignatureV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardianPolicyProposalV1 {
    pub edit: GuardianPolicyEditV1,
    pub owner_signature: RecoverySignatureV1,
    pub acceptances: Vec<GuardianPolicyAcceptanceV1>,
    pub approvals: Vec<GuardianRecoveryApprovalV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardianPolicyApprovalPurposeV1 {
    Propose,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingGuardianPolicyV1 {
    pub edit: GuardianPolicyEditV1,
    pub wait_origin: RecoveryClockV1,
}

/// Present from the first policy edit onward; never discarded on disablement.
/// `state.policy` is historical, NOT authoritative, when `enabled` is false.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardianPolicyStateV1 {
    pub enabled: bool,
    pub guardian_bindings: [OwnerBindingV1; 3],
    pub pending: Option<PendingGuardianPolicyV1>,
    pub consumed_edit_ids: BTreeSet<String>,
    pub retired_guardian_keys: Vec<RecoveryPublicKeyV1>,
}

impl GuardianRecoveryStateV1 {
    pub fn recovery_enabled(&self) -> bool {
        self.policy_edits.as_ref().is_none_or(|edits| edits.enabled)
    }
    pub fn guardian_bindings(&self) -> &[OwnerBindingV1; 3] {
        self.policy_edits
            .as_ref()
            .map_or(&self.enrollment.guardian_bindings, |edits| {
                &edits.guardian_bindings
            })
    }
    pub fn pending_policy_edit(&self) -> Option<&PendingGuardianPolicyV1> {
        self.policy_edits
            .as_ref()
            .and_then(|edits| edits.pending.as_ref())
    }
    pub fn policy_cancel_signing_message(&self) -> Result<Vec<u8>, RecoveryError> {
        if self.recovery_enabled() {
            return Err(RecoveryError::PolicyInvalid);
        }
        let pending = self
            .pending_policy_edit()
            .ok_or(RecoveryError::StaleAuthority)?;
        transcript(
            "mct-guardian-policy-owner-cancel/v1",
            &(self.digest()?, &pending.edit),
        )
    }
}

fn validate_bindings(
    state: &GuardianRecoveryStateV1,
    policy: &GuardianRecoveryPolicyV1,
    bindings: &[OwnerBindingV1; 3],
) -> Result<(), RecoveryError> {
    policy.validate()?;
    let mut keys = vec![
        &state.enrollment.genesis.public_key,
        &state.binding.root_key,
    ];
    for (guardian, binding) in policy.guardians.iter().zip(bindings) {
        validate_binding(binding)?;
        if guardian.mother_node_id != binding.mother_node_id
            || guardian.owner_binding_id != binding.binding_id
            || guardian.owner_binding_generation != binding.root_generation
            || binding.voice_id != state.binding.voice_id
        {
            return Err(RecoveryError::StaleAuthority);
        }
        keys.push(&guardian.signing_key);
        keys.push(&binding.root_key);
    }
    for (i, key) in keys.iter().enumerate() {
        if keys[..i].iter().any(|prior| shares_key_half(prior, key)) {
            return Err(RecoveryError::DuplicatePrincipal);
        }
    }
    Ok(())
}

impl GuardianPolicyEditV1 {
    fn validate(&self, current: &GuardianRecoveryStateV1) -> Result<(), RecoveryError> {
        validate_id(&self.edit_id)?;
        validate_interval(&self.issued_at, &self.expires_at, RECOVERY_APPROVAL_MAX_MS)?;
        if self.policy.target != current.binding
            || current.policy.generation.checked_add(1) != Some(self.policy.generation)
        {
            return Err(RecoveryError::StaleAuthority);
        }
        validate_bindings(current, &self.policy, &self.guardian_bindings)?;
        if self.enabled == current.recovery_enabled()
            && self.policy.guardians == current.policy.guardians
            && &self.guardian_bindings == current.guardian_bindings()
        {
            return Err(RecoveryError::PolicyInvalid);
        }
        if !self.enabled
            && (!current.recovery_enabled()
                || self.policy.guardians != current.policy.guardians
                || &self.guardian_bindings != current.guardian_bindings())
        {
            return Err(RecoveryError::PolicyInvalid);
        }
        Ok(())
    }

    pub fn owner_signing_message(
        &self,
        current: &GuardianRecoveryStateV1,
    ) -> Result<Vec<u8>, RecoveryError> {
        self.validate(current)?;
        transcript(
            "mct-guardian-policy-owner-proposal/v1",
            &(current.digest()?, self),
        )
    }

    pub fn acceptance_signing_message(
        &self,
        current: &GuardianRecoveryStateV1,
        guardian: &MctNodeId,
    ) -> Result<Vec<u8>, RecoveryError> {
        self.validate(current)?;
        if !self
            .policy
            .guardians
            .iter()
            .any(|g| &g.mother_node_id == guardian)
        {
            return Err(RecoveryError::UnappointedGuardian);
        }
        transcript(
            "mct-guardian-policy-acceptance/v1",
            &(current.digest()?, self, guardian),
        )
    }
}

/// Old/current roster vote over an exact policy edit, purpose and current state.
/// This cannot be used as a recovery approval or a newcomer acceptance.
pub fn guardian_policy_approval_message(
    current: &GuardianRecoveryStateV1,
    edit: &GuardianPolicyEditV1,
    purpose: GuardianPolicyApprovalPurposeV1,
    guardian: &MctNodeId,
    issued_at: &Timestamp,
    expires_at: &Timestamp,
) -> Result<Vec<u8>, RecoveryError> {
    if !current.recovery_enabled() {
        return Err(RecoveryError::PolicyInvalid);
    }
    edit.validate(current)?;
    validate_interval(issued_at, expires_at, RECOVERY_APPROVAL_MAX_MS)?;
    if purpose == GuardianPolicyApprovalPurposeV1::Propose
        && (issued_at < &edit.issued_at || expires_at > &edit.expires_at)
    {
        return Err(RecoveryError::TimeInvalid);
    }
    let member = current
        .policy
        .guardians
        .iter()
        .find(|g| &g.mother_node_id == guardian)
        .ok_or(RecoveryError::UnappointedGuardian)?;
    transcript(
        "mct-guardian-policy-quorum/v1",
        &(
            current.digest()?,
            edit,
            purpose,
            member,
            issued_at,
            expires_at,
        ),
    )
}

fn verify_votes(
    current: &GuardianRecoveryStateV1,
    edit: &GuardianPolicyEditV1,
    purpose: GuardianPolicyApprovalPurposeV1,
    approvals: &[GuardianRecoveryApprovalV1],
    now: &Timestamp,
) -> Result<(), RecoveryError> {
    if !(2..=3).contains(&approvals.len()) {
        return Err(RecoveryError::QuorumInvalid);
    }
    let mut seen = BTreeSet::new();
    for approval in approvals {
        if !seen.insert(&approval.guardian_node_id) {
            return Err(RecoveryError::DuplicatePrincipal);
        }
        if now < &approval.issued_at || now >= &approval.expires_at {
            return Err(RecoveryError::TimeInvalid);
        }
        let message = guardian_policy_approval_message(
            current,
            edit,
            purpose,
            &approval.guardian_node_id,
            &approval.issued_at,
            &approval.expires_at,
        )?;
        let guardian = current
            .policy
            .guardians
            .iter()
            .find(|g| g.mother_node_id == approval.guardian_node_id)
            .ok_or(RecoveryError::UnappointedGuardian)?;
        verify_signature(&guardian.signing_key, &message, &approval.signature)?;
    }
    Ok(())
}

pub(super) fn propose(
    current: &GuardianRecoveryStateV1,
    next: &mut GuardianRecoveryStateV1,
    proposal: &GuardianPolicyProposalV1,
    clock: &RecoveryClockV1,
) -> Result<(), RecoveryError> {
    if current.maintenance
        || current.pending.is_some()
        || current.pending_policy_edit().is_some()
        || current
            .policy_edits
            .as_ref()
            .is_some_and(|s| s.consumed_edit_ids.contains(&proposal.edit.edit_id))
    {
        return Err(RecoveryError::StaleAuthority);
    }
    let edit = &proposal.edit;
    edit.validate(current)?;
    if clock.wall_time < edit.issued_at || clock.wall_time >= edit.expires_at {
        return Err(RecoveryError::TimeInvalid);
    }
    verify_signature(
        &current.binding.root_key,
        &edit.owner_signing_message(current)?,
        &proposal.owner_signature,
    )?;
    if current.recovery_enabled() {
        verify_votes(
            current,
            edit,
            GuardianPolicyApprovalPurposeV1::Propose,
            &proposal.approvals,
            &clock.wall_time,
        )?;
    } else if !proposal.approvals.is_empty() {
        // Disabled guardians have no vote, not even a redundant one.
        return Err(RecoveryError::QuorumInvalid);
    }
    if proposal.acceptances.len() > 3 {
        return Err(RecoveryError::QuorumInvalid);
    }
    let mut accepted = BTreeSet::new();
    for acceptance in &proposal.acceptances {
        if !accepted.insert(&acceptance.guardian_node_id) {
            return Err(RecoveryError::DuplicatePrincipal);
        }
        let index = edit
            .policy
            .guardians
            .iter()
            .position(|g| g.mother_node_id == acceptance.guardian_node_id)
            .ok_or(RecoveryError::UnappointedGuardian)?;
        let bytes = edit.acceptance_signing_message(current, &acceptance.guardian_node_id)?;
        verify_signature(
            &edit.guardian_bindings[index].root_key,
            &bytes,
            &acceptance.owner_signature,
        )?;
        verify_signature(
            &edit.policy.guardians[index].signing_key,
            &bytes,
            &acceptance.guardian_signature,
        )?;
    }
    if edit.enabled {
        for (index, guardian) in edit.policy.guardians.iter().enumerate() {
            let unchanged = current.recovery_enabled()
                && current
                    .policy
                    .guardians
                    .iter()
                    .enumerate()
                    .any(|(old_index, old)| {
                        old == guardian
                            && current.guardian_bindings()[old_index]
                                == edit.guardian_bindings[index]
                    });
            if !unchanged && !accepted.contains(&guardian.mother_node_id) {
                return Err(RecoveryError::QuorumInvalid);
            }
        }
    }
    let edits = next.policy_edits.get_or_insert_with(|| {
        Box::new(GuardianPolicyStateV1 {
            enabled: true,
            guardian_bindings: current.guardian_bindings().clone(),
            pending: None,
            consumed_edit_ids: BTreeSet::new(),
            retired_guardian_keys: Vec::new(),
        })
    });
    edits.consumed_edit_ids.insert(edit.edit_id.clone());
    edits.pending = Some(PendingGuardianPolicyV1 {
        edit: edit.clone(),
        wait_origin: clock.clone(),
    });
    Ok(())
}

pub(super) fn cancel(
    current: &GuardianRecoveryStateV1,
    next: &mut GuardianRecoveryStateV1,
    owner_signature: Option<&RecoverySignatureV1>,
    approvals: &[GuardianRecoveryApprovalV1],
    clock: &RecoveryClockV1,
) -> Result<(), RecoveryError> {
    let pending = current
        .pending_policy_edit()
        .ok_or(RecoveryError::StaleAuthority)?;
    if current.recovery_enabled() {
        if owner_signature.is_some() {
            return Err(RecoveryError::PolicyInvalid);
        }
        verify_votes(
            current,
            &pending.edit,
            GuardianPolicyApprovalPurposeV1::Cancel,
            approvals,
            &clock.wall_time,
        )?;
    } else {
        if !approvals.is_empty() {
            return Err(RecoveryError::QuorumInvalid);
        }
        verify_signature(
            &current.binding.root_key,
            &current.policy_cancel_signing_message()?,
            owner_signature.ok_or(RecoveryError::SignatureInvalid)?,
        )?;
    }
    next.policy_edits
        .as_mut()
        .ok_or(RecoveryError::StaleAuthority)?
        .pending = None;
    Ok(())
}

pub(super) fn activate(
    next: &mut GuardianRecoveryStateV1,
    clock: &RecoveryClockV1,
) -> Result<(), RecoveryError> {
    let pending = next
        .pending_policy_edit()
        .ok_or(RecoveryError::StaleAuthority)?;
    if clock.writer_epoch != pending.wait_origin.writer_epoch
        || clock
            .monotonic_ms
            .checked_sub(pending.wait_origin.monotonic_ms)
            .is_none_or(|elapsed| elapsed < RECOVERY_DELAY_MS)
        || clock.wall_time
            < pending
                .wait_origin
                .wall_time
                .checked_add_milliseconds(RECOVERY_DELAY_MS)
                .ok_or(RecoveryError::TimeInvalid)?
    {
        return Err(RecoveryError::TimeInvalid);
    }
    let edit = pending.edit.clone();
    let old_bindings = next.guardian_bindings().clone();
    let extension = next
        .policy_edits
        .as_mut()
        .ok_or(RecoveryError::StaleAuthority)?;
    for key in next
        .policy
        .guardians
        .iter()
        .map(|g| &g.signing_key)
        .chain(old_bindings.iter().map(|b| &b.root_key))
    {
        if !extension.retired_guardian_keys.contains(key) {
            extension.retired_guardian_keys.push(key.clone());
        }
    }
    extension.enabled = edit.enabled;
    extension.guardian_bindings = edit.guardian_bindings;
    extension.pending = None;
    next.policy = edit.policy;
    Ok(())
}

/// Shape consistency only. Canonical provenance still requires replay.
pub fn guardian_policy_state_is_consistent(
    state: &GuardianRecoveryStateV1,
) -> Result<(), RecoveryError> {
    if state.policy.generation < state.binding.root_generation {
        return Err(RecoveryError::StaleAuthority);
    }
    validate_bindings(state, &state.policy, state.guardian_bindings())?;
    if state
        .policy_edits
        .as_ref()
        .is_some_and(|s| s.consumed_edit_ids.is_empty())
    {
        return Err(RecoveryError::Malformed);
    }
    if !state.recovery_enabled() && (state.pending.is_some() || state.maintenance) {
        return Err(RecoveryError::PolicyInvalid);
    }
    if let Some(pending) = state.pending_policy_edit() {
        if state.maintenance
            || state.pending.is_some()
            || !state
                .policy_edits
                .as_ref()
                .is_some_and(|s| s.consumed_edit_ids.contains(&pending.edit.edit_id))
        {
            return Err(RecoveryError::PolicyInvalid);
        }
        pending.edit.validate(state)?;
    }
    Ok(())
}

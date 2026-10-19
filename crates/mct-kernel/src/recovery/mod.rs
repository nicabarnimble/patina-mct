//! Voice-bound guardian recovery evidence and proposed transitions (D-54).
//!
//! Cryptographic checks and a deterministic lifecycle evaluator remain distinct
//! from runtime authority. The canonical writer must supply current authority,
//! trusted clock readings and the exact snapshot, then commit the complete
//! transition before effect. Kernel values alone mint no execution capability.

mod internal;
mod lifecycle;
mod policy;
pub use lifecycle::{
    GuardianEnrollmentV1, GuardianRecoveryActionV1, GuardianRecoveryCommandV1,
    GuardianRecoveryStateV1, PendingGuardianRecoveryV1, RecoveryChallengeV1, RecoveryClockV1,
    evaluate_guardian_recovery,
};
pub use policy::{
    GuardianPolicyAcceptanceV1, GuardianPolicyApprovalPurposeV1, GuardianPolicyEditV1,
    GuardianPolicyProposalV1, GuardianPolicyStateV1, PendingGuardianPolicyV1,
    guardian_policy_approval_message, guardian_policy_state_is_consistent,
};
#[cfg(test)]
mod tests;

use crate::{AdminKeyPublicHalf, MctNodeId, MlDsa65PublicKey, Timestamp};
use serde::{Deserialize, Serialize};

pub use internal::verify_recovery_quorum;

/// Maximum lifetime of an individual guardian approval.
pub const RECOVERY_APPROVAL_MAX_MS: u64 = 600_000;
/// Minimum production recovery delay, checked by the canonical lifecycle.
pub const RECOVERY_DELAY_MS: u64 = 86_400_000;
/// Completion window after the minimum recovery delay.
pub const RECOVERY_COMPLETION_WINDOW_MS: u64 = 604_800_000;

/// Typed internal denial. Adapters must not disclose detailed reasons to peers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RecoveryError {
    #[error("malformed recovery record")]
    Malformed,
    #[error("invalid hybrid recovery signature")]
    SignatureInvalid,
    #[error("invalid recovery policy")]
    PolicyInvalid,
    #[error("recovery Voice does not match")]
    VoiceMismatch,
    #[error("duplicate recovery principal or key")]
    DuplicatePrincipal,
    #[error("recovery binding or policy is stale")]
    StaleAuthority,
    #[error("replacement key is not fresh")]
    ReplacementNotFresh,
    #[error("recovery evidence is outside its validity interval")]
    TimeInvalid,
    #[error("recovery quorum is insufficient or oversized")]
    QuorumInvalid,
    #[error("recovery signer is not appointed")]
    UnappointedGuardian,
}

/// Both public halves, always present. Private key material has no representation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPublicKeyV1 {
    pub ed25519: AdminKeyPublicHalf,
    pub ml_dsa65: MlDsa65PublicKey,
}

/// Length/tag/hex-checked hybrid signature; lowercase canonical wire form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RecoverySignatureV1(String);

impl RecoverySignatureV1 {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<RecoverySignatureV1> for String {
    fn from(value: RecoverySignatureV1) -> Self {
        value.0
    }
}

/// Stable Voice identity, derived from a versioned genesis public-key record.
/// Merely parsing this identifier never proves ownership or authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VoiceIdV1([u8; 32]);

/// Digest of a versioned recovery transcript, not a bearer capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryDigestV1([u8; 32]);

/// Public genesis identity and a self-proof of possession, not an admission grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceGenesisV1 {
    pub public_key: RecoveryPublicKeyV1,
    pub possession: RecoverySignatureV1,
}

impl VoiceGenesisV1 {
    /// Bytes the genesis owner must sign using the existing hybrid primitive.
    pub fn signing_message(key: &RecoveryPublicKeyV1) -> Result<Vec<u8>, RecoveryError> {
        internal::genesis_message(key)
    }

    /// Verify both halves and derive VoiceId. This is not permission to enroll.
    pub fn verify(&self) -> Result<VoiceIdV1, RecoveryError> {
        let message = Self::signing_message(&self.public_key)?;
        internal::verify_signature(&self.public_key, &message, &self.possession)?;
        Ok(VoiceIdV1(*blake3::hash(&message).as_bytes()))
    }
}

/// One Mother-wide administrative owner binding. A wire copy is only a claim.
/// The evaluator's current binding must come from canonical replay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerBindingV1 {
    pub mother_node_id: MctNodeId,
    pub ledger_id: String,
    pub epoch_id: String,
    pub binding_id: String,
    pub voice_id: VoiceIdV1,
    pub root_generation: u64,
    pub root_key: RecoveryPublicKeyV1,
}

/// Guardian identity/key already appointed by canonical target policy.
/// This record does not itself prove guardian acceptance or local issuance rights.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryGuardianV1 {
    pub mother_node_id: MctNodeId,
    pub owner_binding_id: String,
    pub owner_binding_generation: u64,
    pub voice_id: VoiceIdV1,
    pub signing_key: RecoveryPublicKeyV1,
}

/// Fixed 2-of-3 release profile, in strictly increasing guardian node order.
/// Shape validation does not authorize policy activation or roster mutation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardianRecoveryPolicyV1 {
    pub generation: u64,
    pub target: OwnerBindingV1,
    pub guardians: [RecoveryGuardianV1; 3],
}

impl GuardianRecoveryPolicyV1 {
    pub fn validate(&self) -> Result<(), RecoveryError> {
        internal::validate_policy(self)
    }

    pub fn digest(&self) -> Result<RecoveryDigestV1, RecoveryError> {
        self.validate()?;
        internal::digest("mct-guardian-policy/v1", self)
    }
}

/// Exact requested replacement. Validity bounds can only shorten the horizon;
/// they do not start the writer-owned delay or authorize offline elapsed time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryIntentV1 {
    pub request_id: String,
    pub target: OwnerBindingV1,
    pub policy_generation: u64,
    pub policy_digest: RecoveryDigestV1,
    pub replacement_key: RecoveryPublicKeyV1,
    pub not_before: Timestamp,
    pub expires_at: Timestamp,
}

impl RecoveryIntentV1 {
    /// Domain-separated replacement-key proof, bound to the entire intent.
    pub fn signing_message(&self) -> Result<Vec<u8>, RecoveryError> {
        internal::validate_intent_shape(self)?;
        internal::transcript("mct-recovery-replacement-possession/v1", self)
    }

    pub fn digest(&self) -> Result<RecoveryDigestV1, RecoveryError> {
        internal::validate_intent_shape(self)?;
        internal::digest("mct-recovery-intent/v1", self)
    }
}

/// Incoming replacement request, including proof under the new keys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryRequestV1 {
    pub intent: RecoveryIntentV1,
    pub replacement_possession: RecoverySignatureV1,
}

/// One operation; completion must use the canonical writer's fresh challenge.
/// Callers must not choose the expected purpose/challenge on the writer's behalf.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "purpose", rename_all = "snake_case")]
pub enum RecoveryApprovalPurposeV1 {
    Initiate,
    Cancel,
    Complete { challenge: String },
}

/// Guardian approval with bounded freshness. Signature binds purpose externally
/// so an initiation proof cannot be transplanted into cancellation/completion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardianRecoveryApprovalV1 {
    pub guardian_node_id: MctNodeId,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
    pub signature: RecoverySignatureV1,
}

/// Inputs to the guardian's signed transcript; no untyped command byte bag.
pub struct GuardianApprovalMessageV1<'a> {
    pub policy: &'a GuardianRecoveryPolicyV1,
    pub intent: &'a RecoveryIntentV1,
    pub purpose: &'a RecoveryApprovalPurposeV1,
    pub guardian_node_id: &'a MctNodeId,
    pub issued_at: &'a Timestamp,
    pub expires_at: &'a Timestamp,
}

impl GuardianApprovalMessageV1<'_> {
    pub fn signing_message(&self) -> Result<Vec<u8>, RecoveryError> {
        internal::approval_message(self)
    }
}

/// Ephemeral evidence only; intentionally neither serializable nor constructible
/// by callers. Not an execution token: pending state, delay, consumed request IDs,
/// canonical freshness and writer ordering must still be checked at commit.
#[derive(Clone, Debug)]
pub struct RecoveryQuorumEvidenceV1 {
    intent_digest: RecoveryDigestV1,
    policy_digest: RecoveryDigestV1,
    purpose: RecoveryApprovalPurposeV1,
    guardians: Vec<MctNodeId>,
    valid_until: Timestamp,
}

impl RecoveryQuorumEvidenceV1 {
    pub fn intent_digest(&self) -> RecoveryDigestV1 {
        self.intent_digest
    }
    pub fn policy_digest(&self) -> RecoveryDigestV1 {
        self.policy_digest
    }
    pub fn purpose(&self) -> &RecoveryApprovalPurposeV1 {
        &self.purpose
    }
    pub fn guardians(&self) -> &[MctNodeId] {
        &self.guardians
    }
    pub fn valid_until(&self) -> &Timestamp {
        &self.valid_until
    }
}

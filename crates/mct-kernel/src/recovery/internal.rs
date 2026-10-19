use super::*;
use crate::admin_sig::{ADMIN_HYBRID_SIGNATURE_LEN, ADMIN_SIG_V1_TAG};
use crate::verify_admin_hybrid_signature;
use serde::{Deserializer, Serializer, de::Error as _};

// Serde internally tagged unit variants accept extra fields even with
// deny_unknown_fields. Empty struct variants enforce the wire contract.
impl<'de> Deserialize<'de> for RecoveryApprovalPurposeV1 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "purpose", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Initiate {},
            Cancel {},
            Complete { challenge: String },
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::Initiate {} => Self::Initiate,
            Wire::Cancel {} => Self::Cancel,
            Wire::Complete { challenge } => Self::Complete { challenge },
        })
    }
}

impl TryFrom<String> for RecoverySignatureV1 {
    type Error = RecoveryError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() != ADMIN_HYBRID_SIGNATURE_LEN * 2 {
            return Err(RecoveryError::Malformed);
        }
        let bytes = const_hex::decode(&value).map_err(|_| RecoveryError::Malformed)?;
        if bytes.len() != ADMIN_HYBRID_SIGNATURE_LEN || bytes[0] != ADMIN_SIG_V1_TAG {
            return Err(RecoveryError::Malformed);
        }
        Ok(Self(const_hex::encode(bytes)))
    }
}

macro_rules! digest_codec {
    ($name:ident, $prefix:literal) => {
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&format!("{}{}", $prefix, const_hex::encode(self.0)))
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                let hex = text
                    .strip_prefix($prefix)
                    .ok_or_else(|| D::Error::custom("wrong digest domain"))?;
                if hex.len() != 64
                    || !hex
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(D::Error::custom("invalid canonical digest"));
                }
                let mut bytes = [0; 32];
                const_hex::decode_to_slice(hex, &mut bytes).map_err(D::Error::custom)?;
                Ok(Self(bytes))
            }
        }
    };
}

digest_codec!(VoiceIdV1, "mct-voice-v1:");
digest_codec!(RecoveryDigestV1, "mct-recovery-digest-v1:");

pub(super) fn transcript<T: Serialize>(domain: &str, record: &T) -> Result<Vec<u8>, RecoveryError> {
    let json = serde_json::to_vec(record).map_err(|_| RecoveryError::Malformed)?;
    let mut bytes = Vec::with_capacity(16 + domain.len() + json.len());
    for part in [domain.as_bytes(), json.as_slice()] {
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part);
    }
    Ok(bytes)
}

pub(super) fn digest<T: Serialize>(
    domain: &str,
    record: &T,
) -> Result<RecoveryDigestV1, RecoveryError> {
    Ok(RecoveryDigestV1(
        *blake3::hash(&transcript(domain, record)?).as_bytes(),
    ))
}

pub(super) fn validate_key(key: &RecoveryPublicKeyV1) -> Result<(), RecoveryError> {
    let crate::AdminKeyPublicHalf::Ed25519(ed) = key.ed25519;
    if ed.verifying_key().is_weak() {
        return Err(RecoveryError::Malformed);
    }
    Ok(())
}

pub(super) fn verify_signature(
    key: &RecoveryPublicKeyV1,
    message: &[u8],
    signature: &RecoverySignatureV1,
) -> Result<(), RecoveryError> {
    validate_key(key)?;
    verify_admin_hybrid_signature(&key.ed25519, &key.ml_dsa65, message, signature.as_str())
        .map_err(|_| RecoveryError::SignatureInvalid)
}

pub(super) fn genesis_message(key: &RecoveryPublicKeyV1) -> Result<Vec<u8>, RecoveryError> {
    validate_key(key)?;
    transcript("mct-voice-genesis/v1", key)
}

pub(super) fn validate_id(id: &str) -> Result<(), RecoveryError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
    {
        return Err(RecoveryError::Malformed);
    }
    Ok(())
}

pub(super) fn validate_binding(binding: &OwnerBindingV1) -> Result<(), RecoveryError> {
    for id in [
        binding.mother_node_id.as_str(),
        &binding.ledger_id,
        &binding.epoch_id,
        &binding.binding_id,
    ] {
        validate_id(id)?;
    }
    if binding.root_generation == 0 {
        return Err(RecoveryError::Malformed);
    }
    validate_key(&binding.root_key)
}

pub(super) fn shares_key_half(a: &RecoveryPublicKeyV1, b: &RecoveryPublicKeyV1) -> bool {
    a.ed25519 == b.ed25519 || a.ml_dsa65 == b.ml_dsa65
}

pub(super) fn validate_policy(policy: &GuardianRecoveryPolicyV1) -> Result<(), RecoveryError> {
    validate_binding(&policy.target)?;
    if policy.generation == 0 {
        return Err(RecoveryError::PolicyInvalid);
    }
    for (index, guardian) in policy.guardians.iter().enumerate() {
        validate_id(guardian.mother_node_id.as_str())?;
        validate_id(&guardian.owner_binding_id)?;
        validate_key(&guardian.signing_key)?;
        if guardian.owner_binding_generation == 0 {
            return Err(RecoveryError::PolicyInvalid);
        }
        if guardian.voice_id != policy.target.voice_id {
            return Err(RecoveryError::VoiceMismatch);
        }
        if guardian.mother_node_id == policy.target.mother_node_id
            || shares_key_half(&guardian.signing_key, &policy.target.root_key)
            || policy.guardians[..index].iter().any(|prior| {
                prior.mother_node_id == guardian.mother_node_id
                    || shares_key_half(&prior.signing_key, &guardian.signing_key)
            })
        {
            return Err(RecoveryError::DuplicatePrincipal);
        }
        if index > 0 && policy.guardians[index - 1].mother_node_id >= guardian.mother_node_id {
            return Err(RecoveryError::PolicyInvalid);
        }
    }
    Ok(())
}

pub(super) fn validate_interval(
    start: &Timestamp,
    end: &Timestamp,
    max_ms: u64,
) -> Result<(), RecoveryError> {
    let upper = start
        .checked_add_milliseconds(max_ms)
        .ok_or(RecoveryError::TimeInvalid)?;
    if end <= start || end > &upper {
        return Err(RecoveryError::TimeInvalid);
    }
    Ok(())
}

pub(super) fn validate_intent_shape(intent: &RecoveryIntentV1) -> Result<(), RecoveryError> {
    validate_id(&intent.request_id)?;
    validate_binding(&intent.target)?;
    validate_key(&intent.replacement_key)?;
    if intent.policy_generation == 0 {
        return Err(RecoveryError::PolicyInvalid);
    }
    validate_interval(
        &intent.not_before,
        &intent.expires_at,
        RECOVERY_DELAY_MS + RECOVERY_COMPLETION_WINDOW_MS,
    )
}

fn validate_intent_policy(
    intent: &RecoveryIntentV1,
    policy: &GuardianRecoveryPolicyV1,
) -> Result<(), RecoveryError> {
    validate_intent_shape(intent)?;
    if intent.target != policy.target
        || intent.policy_generation != policy.generation
        || intent.policy_digest != policy.digest()?
    {
        return Err(RecoveryError::StaleAuthority);
    }
    if shares_key_half(&intent.replacement_key, &policy.target.root_key)
        || policy
            .guardians
            .iter()
            .any(|guardian| shares_key_half(&intent.replacement_key, &guardian.signing_key))
    {
        return Err(RecoveryError::ReplacementNotFresh);
    }
    Ok(())
}

fn validate_purpose(purpose: &RecoveryApprovalPurposeV1) -> Result<(), RecoveryError> {
    if let RecoveryApprovalPurposeV1::Complete { challenge } = purpose {
        validate_id(challenge)?;
    }
    Ok(())
}

pub(super) fn approval_message(
    parts: &GuardianApprovalMessageV1<'_>,
) -> Result<Vec<u8>, RecoveryError> {
    validate_intent_policy(parts.intent, parts.policy)?;
    validate_purpose(parts.purpose)?;
    validate_interval(parts.issued_at, parts.expires_at, RECOVERY_APPROVAL_MAX_MS)?;
    if parts.issued_at < &parts.intent.not_before || parts.expires_at > &parts.intent.expires_at {
        return Err(RecoveryError::TimeInvalid);
    }
    let guardian = parts
        .policy
        .guardians
        .iter()
        .find(|g| &g.mother_node_id == parts.guardian_node_id)
        .ok_or(RecoveryError::UnappointedGuardian)?;
    #[derive(Serialize)]
    struct Statement<'a> {
        policy_digest: RecoveryDigestV1,
        intent_digest: RecoveryDigestV1,
        guardian: &'a RecoveryGuardianV1,
        purpose: &'a RecoveryApprovalPurposeV1,
        issued_at: &'a Timestamp,
        expires_at: &'a Timestamp,
    }
    transcript(
        "mct-guardian-recovery-approval/v1",
        &Statement {
            policy_digest: parts.policy.digest()?,
            intent_digest: parts.intent.digest()?,
            guardian,
            purpose: parts.purpose,
            issued_at: parts.issued_at,
            expires_at: parts.expires_at,
        },
    )
}

/// Verify an exact-intent quorum against current canonical inputs and Mother time.
///
/// `current_binding`, `policy`, expected `purpose`/challenge and `now` must be
/// obtained by Mother, not trusted from the incoming request. Success is evidence
/// only. It does not check pending/consumed IDs, enforce 24h delay, or commit state.
pub fn verify_recovery_quorum(
    current_binding: &OwnerBindingV1,
    policy: &GuardianRecoveryPolicyV1,
    request: &RecoveryRequestV1,
    purpose: &RecoveryApprovalPurposeV1,
    approvals: &[GuardianRecoveryApprovalV1],
    now: &Timestamp,
) -> Result<RecoveryQuorumEvidenceV1, RecoveryError> {
    policy.validate()?;
    if current_binding != &policy.target {
        return Err(RecoveryError::StaleAuthority);
    }
    validate_intent_policy(&request.intent, policy)?;
    validate_purpose(purpose)?;
    if now < &request.intent.not_before || now >= &request.intent.expires_at {
        return Err(RecoveryError::TimeInvalid);
    }
    if !(2..=3).contains(&approvals.len()) {
        return Err(RecoveryError::QuorumInvalid);
    }
    verify_signature(
        &request.intent.replacement_key,
        &request.intent.signing_message()?,
        &request.replacement_possession,
    )?;
    let mut guardians = Vec::with_capacity(approvals.len());
    let mut valid_until = request.intent.expires_at.clone();
    for approval in approvals {
        if guardians.contains(&approval.guardian_node_id) {
            return Err(RecoveryError::DuplicatePrincipal);
        }
        if now < &approval.issued_at || now >= &approval.expires_at {
            return Err(RecoveryError::TimeInvalid);
        }
        let message = approval_message(&GuardianApprovalMessageV1 {
            policy,
            intent: &request.intent,
            purpose,
            guardian_node_id: &approval.guardian_node_id,
            issued_at: &approval.issued_at,
            expires_at: &approval.expires_at,
        })?;
        let guardian = policy
            .guardians
            .iter()
            .find(|g| g.mother_node_id == approval.guardian_node_id)
            .ok_or(RecoveryError::UnappointedGuardian)?;
        verify_signature(&guardian.signing_key, &message, &approval.signature)?;
        guardians.push(approval.guardian_node_id.clone());
        valid_until = valid_until.min(approval.expires_at.clone());
    }
    guardians.sort();
    Ok(RecoveryQuorumEvidenceV1 {
        intent_digest: request.intent.digest()?,
        policy_digest: policy.digest()?,
        purpose: purpose.clone(),
        guardians,
        valid_until,
    })
}

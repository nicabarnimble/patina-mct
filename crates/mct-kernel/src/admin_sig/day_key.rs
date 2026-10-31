//! WebAuthn ES256 and standalone Ed25519 verification. No hybrid fallback.
use crate::Timestamp;
use crate::admin_key::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::signature::Verifier as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scheme", deny_unknown_fields)]
pub enum DayKeyAdminProofV1 {
    #[serde(rename = "webauthn-p256-day-key/v1")]
    WebAuthnP256DayKeyV1 {
        credential_id: String,
        authenticator_data: String,
        client_data_json: String,
        signature: String,
    },
    #[serde(rename = "ed25519-day-key/v1")]
    Ed25519DayKeyV1 { grant_id: String, signature: String },
}
impl DayKeyAdminProofV1 {
    pub fn signer_ref(&self) -> String {
        match self {
            Self::WebAuthnP256DayKeyV1 { credential_id, .. } => format!("webauthn:{credential_id}"),
            Self::Ed25519DayKeyV1 { grant_id, .. } => format!("ed25519:{grant_id}"),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasskeyAdminRequestV1 {
    pub intent: PasskeyAdminIntentV1,
    pub proof: DayKeyAdminProofV1,
}

/// Fixed transcript. Includes scheme/signer, exact command, nonce, target,
/// authority snapshot and validity window. Never signs a caller's digest label.
pub fn day_key_signing_message(
    intent: &PasskeyAdminIntentV1,
    signer_ref: &str,
) -> Result<Vec<u8>, DayKeyAdminError> {
    let value = serde_json::to_value(("mct-passkey-administration/v1", signer_ref, intent))
        .map_err(|_| DayKeyAdminError("cannot encode intent"))?;
    // Value maps sort keys recursively. The browser helper uses the same fixed
    // string/integer transcript (not arbitrary floating-point JSON).
    serde_json::to_vec(&value).map_err(|_| DayKeyAdminError("cannot encode intent"))
}
pub fn webauthn_admin_challenge(
    intent: &PasskeyAdminIntentV1,
    credential_id: &str,
) -> Result<String, DayKeyAdminError> {
    Ok(
        URL_SAFE_NO_PAD.encode(Sha256::digest(day_key_signing_message(
            intent,
            &format!("webauthn:{credential_id}"),
        )?)),
    )
}
fn decode(value: &str, max: usize) -> Result<Vec<u8>, DayKeyAdminError> {
    if value.len() > max * 2 {
        return Err(DayKeyAdminError("assertion exceeds bounds"));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| DayKeyAdminError("invalid assertion encoding"))?;
    if bytes.len() > max || URL_SAFE_NO_PAD.encode(&bytes) != value {
        return Err(DayKeyAdminError("noncanonical assertion encoding"));
    }
    Ok(bytes)
}

/// Actual WebAuthn assertion verification: signed authenticator data and the hash
/// of original clientDataJSON bytes; RP ID, origin, challenge, UP and UV required.
/// Synced 1Password credentials may have a zero/non-monotonic signCount. Freshness
/// comes from the signed intent and canonical consumed nonce, not that counter.
pub fn verify_webauthn_admin_signature(
    root: &WebAuthnP256DayKeyV1,
    request: &PasskeyAdminRequestV1,
) -> Result<(), DayKeyAdminError> {
    root.validate()?;
    let DayKeyAdminProofV1::WebAuthnP256DayKeyV1 {
        credential_id,
        authenticator_data,
        client_data_json,
        signature,
    } = &request.proof
    else {
        return Err(DayKeyAdminError("passkey proof required"));
    };
    if credential_id != &root.credential_id {
        return Err(DayKeyAdminError("unregistered credential"));
    }
    let auth = decode(authenticator_data, 37)?;
    let client = decode(client_data_json, 4096)?;
    let signature = decode(signature, 72)?;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ClientData {
        #[serde(rename = "type")]
        kind: String,
        challenge: String,
        origin: String,
        #[serde(default)]
        cross_origin: bool,
        top_origin: Option<String>,
    }
    let data: ClientData =
        serde_json::from_slice(&client).map_err(|_| DayKeyAdminError("invalid client data"))?;
    if auth.len() != 37
        || auth[..32] != Sha256::digest(root.rp_id.as_bytes())[..]
        || auth[32] & 0x05 != 0x05
        || auth[32] & !0x1d != 0
        || (auth[32] & 0x10 != 0 && auth[32] & 0x08 == 0)
        || data.kind != "webauthn.get"
        || data.origin != root.origin
        || data.cross_origin
        || data.top_origin.is_some()
        || data.challenge != webauthn_admin_challenge(&request.intent, credential_id)?
    {
        return Err(DayKeyAdminError(
            "WebAuthn relying-party or user-verification binding denied",
        ));
    }
    let mut signed = auth;
    signed.extend_from_slice(&Sha256::digest(&client));
    let public = const_hex::decode(&root.public_key_sec1)
        .map_err(|_| DayKeyAdminError("invalid P-256 key"))?;
    let public = p256::ecdsa::VerifyingKey::from_sec1_bytes(&public)
        .map_err(|_| DayKeyAdminError("invalid P-256 key"))?;
    let signature = p256::ecdsa::Signature::from_der(&signature)
        .map_err(|_| DayKeyAdminError("invalid ES256 signature"))?;
    public
        .verify(&signed, &signature)
        .map_err(|_| DayKeyAdminError("ES256 signature denied"))
}

pub fn verify_ed25519_day_key_signature(
    key: &Ed25519DayKeyV1,
    request: &PasskeyAdminRequestV1,
) -> Result<(), DayKeyAdminError> {
    let DayKeyAdminProofV1::Ed25519DayKeyV1 {
        grant_id,
        signature,
    } = &request.proof
    else {
        return Err(DayKeyAdminError("day-key proof required"));
    };
    if grant_id != &key.grant.grant_id || signature.len() != 128 {
        return Err(DayKeyAdminError("wrong signer or signature size"));
    }
    let bytes =
        const_hex::decode(signature).map_err(|_| DayKeyAdminError("invalid Ed25519 signature"))?;
    if const_hex::encode(&bytes) != *signature {
        return Err(DayKeyAdminError("noncanonical Ed25519 signature"));
    }
    let signature = ed25519_dalek::Signature::from_slice(&bytes)
        .map_err(|_| DayKeyAdminError("invalid Ed25519 signature"))?;
    let AdminKeyPublicHalf::Ed25519(public) = key.grant.public_half;
    public
        .verifying_key()
        .verify_strict(
            &day_key_signing_message(&request.intent, &request.proof.signer_ref())?,
            &signature,
        )
        .map_err(|_| DayKeyAdminError("Ed25519 signature denied"))
}

/// Pure transition evaluation. The adapter must obtain `state` and snapshot
/// identity from canonical replay, never from the request. `None` accepts only a
/// cryptographically valid birth candidate; storage additionally requires trusted
/// registration/pin, independent virgin inventory and first-frame publication.
pub fn evaluate_passkey_admin_transition(
    state: Option<&PasskeyAdminStateV1>,
    request: &PasskeyAdminRequestV1,
    now: &Timestamp,
) -> Result<PasskeyAdminStateV1, DayKeyAdminError> {
    request.intent.validate(now)?;
    let intent = &request.intent;
    let nonce = (request.proof.signer_ref(), intent.nonce.clone());
    let Some(state) = state else {
        let PasskeyAdminActionV1::Bootstrap { root, .. } = &intent.action else {
            return Err(DayKeyAdminError("missing passkey genesis"));
        };
        verify_webauthn_admin_signature(root, request)?;
        return Ok(PasskeyAdminStateV1 {
            mother_node_id: intent.mother_node_id.clone(),
            ledger_id: intent.ledger_id.clone(),
            root: root.clone(),
            day_keys: Default::default(),
            used_nonces: [nonce].into_iter().collect(),
            last_decided_at: now.clone(),
        });
    };
    if state.mother_node_id != intent.mother_node_id
        || state.ledger_id != intent.ledger_id
        || state.used_nonces.contains(&nonce)
        || instant(now)? < instant(&state.last_decided_at)?
    {
        return Err(DayKeyAdminError(
            "wrong target, consumed nonce or clock rollback",
        ));
    }
    let required_scope = match &intent.action {
        PasskeyAdminActionV1::Bootstrap { .. } => {
            return Err(DayKeyAdminError("genesis is immutable"));
        }
        PasskeyAdminActionV1::IssueDayKey { .. } | PasskeyAdminActionV1::RevokeDayKey { .. } => {
            AdminKeyScopeV1::KeyManage
        }
        PasskeyAdminActionV1::Command { scope, .. } => *scope,
    };
    match &request.proof {
        DayKeyAdminProofV1::WebAuthnP256DayKeyV1 { .. } => {
            verify_webauthn_admin_signature(&state.root, request)?
        }
        DayKeyAdminProofV1::Ed25519DayKeyV1 { grant_id, .. } => {
            if matches!(intent.action, PasskeyAdminActionV1::IssueDayKey { .. }) {
                return Err(DayKeyAdminError("only the passkey root may issue leases"));
            }
            let key = state
                .day_keys
                .get(grant_id)
                .ok_or(DayKeyAdminError("unknown day key"))?;
            if key.mother_node_id != intent.mother_node_id
                || key.ledger_id != intent.ledger_id
                || !key.authorizes(required_scope, now)
            {
                return Err(DayKeyAdminError("expired, revoked or wrong-scope day key"));
            }
            verify_ed25519_day_key_signature(key, request)?;
        }
    }
    let mut next = state.clone();
    match &intent.action {
        PasskeyAdminActionV1::IssueDayKey {
            grant_id,
            public_half,
            scopes,
            expires_at,
        } => {
            if next.day_keys.contains_key(grant_id) {
                return Err(DayKeyAdminError(
                    "grant ids cannot be replaced or resurrected",
                ));
            }
            let lease = Ed25519DayKeyV1 {
                mother_node_id: intent.mother_node_id.clone(),
                ledger_id: intent.ledger_id.clone(),
                issued_at: now.clone(),
                grant: AdminKeyGrantV1 {
                    grant_id: grant_id.clone(),
                    public_half: *public_half,
                    class: AdminKeyGrantClassV1::DayToDay,
                    scopes: scopes.clone(),
                    expires_at: Some(expires_at.clone()),
                    grant_state: AdminKeyGrantStateV1::Active,
                    ml_dsa65_public_key: None,
                    issuance_ceiling: None,
                },
            };
            lease.validate()?;
            next.day_keys.insert(grant_id.clone(), lease);
        }
        PasskeyAdminActionV1::RevokeDayKey { grant_id } => {
            next.day_keys
                .get_mut(grant_id)
                .ok_or(DayKeyAdminError("revocation target is not a day key"))?
                .grant
                .grant_state = AdminKeyGrantStateV1::Revoked;
        }
        PasskeyAdminActionV1::Command { .. } => {}
        PasskeyAdminActionV1::Bootstrap { .. } => unreachable!("bootstrap rejected above"),
    }
    next.used_nonces.insert(nonce);
    next.last_decided_at = now.clone();
    Ok(next)
}

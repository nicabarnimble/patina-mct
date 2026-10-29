//! Passkey-rooted administration. Legacy grant records are never provenance here.
use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::collections::{BTreeMap, BTreeSet};

pub const DAY_KEY_LIFETIME_SECONDS: u64 = 24 * 60 * 60;
pub const ADMIN_INTENT_LIFETIME_SECONDS: u64 = 5 * 60;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("passkey administration denied: {0}")]
pub struct DayKeyAdminError(pub &'static str);

pub(crate) fn valid_admin_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
}
pub(crate) fn instant(time: &Timestamp) -> Result<jiff::Timestamp, DayKeyAdminError> {
    time.as_str()
        .parse()
        .map_err(|_| DayKeyAdminError("invalid time"))
}

/// A credential registered independently of the command claiming root authority.
/// Public SEC1 bytes are canonical uncompressed P-256, lowercase hex. Credential
/// IDs are canonical base64url, not UTF-8 interpretations of authenticator bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebAuthnP256DayKeyV1 {
    pub credential_id: String,
    pub public_key_sec1: String,
    pub rp_id: String,
    pub origin: String,
}

impl WebAuthnP256DayKeyV1 {
    pub fn validate(&self) -> Result<(), DayKeyAdminError> {
        let credential = URL_SAFE_NO_PAD
            .decode(&self.credential_id)
            .map_err(|_| DayKeyAdminError("invalid credential id"))?;
        let key = const_hex::decode(&self.public_key_sec1)
            .map_err(|_| DayKeyAdminError("invalid P-256 public key"))?;
        if credential.is_empty()
            || credential.len() > 1024
            || URL_SAFE_NO_PAD.encode(&credential) != self.credential_id
            || key.len() != 65
            || key[0] != 4
            || const_hex::encode(&key) != self.public_key_sec1
            || p256::ecdsa::VerifyingKey::from_sec1_bytes(&key).is_err()
        {
            return Err(DayKeyAdminError("invalid registered credential"));
        }
        let origin =
            url::Url::parse(&self.origin).map_err(|_| DayKeyAdminError("invalid origin"))?;
        let host = origin
            .host_str()
            .ok_or(DayKeyAdminError("missing origin host"))?;
        if self.rp_id.len() > 253
            || !self.rp_id.contains('.')
            || self.rp_id.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            })
            || self.origin.len() > 512
            || origin.scheme() != "https"
            || origin.origin().ascii_serialization() != self.origin
            || !(host == self.rp_id || host.ends_with(&format!(".{}", self.rp_id)))
        {
            return Err(DayKeyAdminError("origin and RP ID disagree"));
        }
        Ok(())
    }

    /// Registration adapters may normalize a strict ES256 COSE_Key to SEC1.
    /// This parses public bytes only; it does not authenticate registration.
    pub fn sec1_from_cose(cose: &[u8]) -> Result<String, DayKeyAdminError> {
        use ciborium::value::Value;
        let bad = || DayKeyAdminError("invalid ES256 COSE key");
        if cose.len() > 512 {
            return Err(bad());
        }
        let mut reader = std::io::Cursor::new(cose);
        let Value::Map(map) = ciborium::from_reader::<Value, _>(&mut reader).map_err(|_| bad())?
        else {
            return Err(bad());
        };
        if reader.position() != cose.len() as u64 || map.len() != 5 {
            return Err(bad());
        }
        let mut fields = BTreeMap::new();
        for (k, v) in map {
            let Value::Integer(k) = k else {
                return Err(bad());
            };
            if fields.insert(i128::from(k), v).is_some() {
                return Err(bad());
            }
        }
        for (key, expected) in [(1, 2), (3, -7), (-1, 1)] {
            if fields.get(&key) != Some(&Value::Integer(expected.into())) {
                return Err(bad());
            }
        }
        let mut sec1 = vec![4];
        for key in [-2, -3] {
            let Some(Value::Bytes(bytes)) = fields.get(&key) else {
                return Err(bad());
            };
            if bytes.len() != 32 {
                return Err(bad());
            }
            sec1.extend(bytes);
        }
        p256::ecdsa::VerifyingKey::from_sec1_bytes(&sec1).map_err(|_| bad())?;
        Ok(const_hex::encode(sec1))
    }
}

/// The lease is a separate record, so old grant serialization/hashes do not change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ed25519DayKeyV1 {
    pub mother_node_id: String,
    pub ledger_id: String,
    pub issued_at: Timestamp,
    pub grant: AdminKeyGrantV1,
}
impl Ed25519DayKeyV1 {
    pub fn validate(&self) -> Result<(), DayKeyAdminError> {
        let issued = instant(&self.issued_at)?;
        let expires = instant(
            self.grant
                .expires_at
                .as_ref()
                .ok_or(DayKeyAdminError("missing lease expiry"))?,
        )?;
        let maximum = issued
            .checked_add(std::time::Duration::from_secs(DAY_KEY_LIFETIME_SECONDS))
            .map_err(|_| DayKeyAdminError("lease time overflow"))?;
        let AdminKeyPublicHalf::Ed25519(key) = self.grant.public_half;
        if !valid_admin_id(&self.grant.grant_id)
            || !valid_admin_id(&self.mother_node_id)
            || !valid_admin_id(&self.ledger_id)
            || self.grant.class != AdminKeyGrantClassV1::DayToDay
            || self.grant.ml_dsa65_public_key.is_some()
            || self.grant.issuance_ceiling.is_some()
            || admin_key_grant_shape_error(&self.grant).is_some()
            || key.verifying_key().is_weak()
            || expires <= issued
            || expires > maximum
        {
            return Err(DayKeyAdminError("invalid 24-hour day-key lease"));
        }
        Ok(())
    }
    pub fn authorizes(&self, scope: AdminKeyScopeV1, now: &Timestamp) -> bool {
        self.validate().is_ok()
            && self.grant.grant_state == AdminKeyGrantStateV1::Active
            && self.grant.scopes.contains(&scope)
            && instant(now).is_ok_and(|now| {
                instant(&self.issued_at).is_ok_and(|start| start <= now)
                    && self
                        .grant
                        .expires_at
                        .as_ref()
                        .and_then(|end| instant(end).ok())
                        .is_some_and(|end| now < end)
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum PasskeyAdminActionV1 {
    Bootstrap {
        root: WebAuthnP256DayKeyV1,
        inventory_hash: String,
    },
    IssueDayKey {
        grant_id: String,
        public_half: AdminKeyPublicHalf,
        scopes: Vec<AdminKeyScopeV1>,
        expires_at: Timestamp,
    },
    RevokeDayKey {
        grant_id: String,
    },
    /// Adapters must match the signed route to their own required scope before
    /// executing. Body bytes are signed verbatim, not reinterpreted as authority.
    Command {
        scope: AdminKeyScopeV1,
        route: String,
        body: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasskeyAdminIntentV1 {
    pub mother_node_id: String,
    pub ledger_id: String,
    pub epoch: String,
    pub generation: u64,
    pub authority_observation_id: String,
    pub nonce: String,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
    pub action: PasskeyAdminActionV1,
}
impl PasskeyAdminIntentV1 {
    pub fn validate(&self, now: &Timestamp) -> Result<(), DayKeyAdminError> {
        let start = instant(&self.issued_at)?;
        let end = instant(&self.expires_at)?;
        let now = instant(now)?;
        if !valid_admin_id(&self.mother_node_id)
            || !valid_admin_id(&self.ledger_id)
            || self.epoch.len() > 128
            || self.authority_observation_id.len() > 256
            || self.nonce.len() != 64
            || !self
                .nonce
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || start > now
            || now >= end
            || end
                > start
                    .checked_add(std::time::Duration::from_secs(
                        ADMIN_INTENT_LIFETIME_SECONDS,
                    ))
                    .map_err(|_| DayKeyAdminError("intent time overflow"))?
        {
            return Err(DayKeyAdminError("invalid target, nonce or intent window"));
        }
        if let PasskeyAdminActionV1::Command { route, body, .. } = &self.action
            && (route.len() > 256 || !route.starts_with('/') || body.len() > 64 * 1024)
        {
            return Err(DayKeyAdminError("admin command exceeds bounds"));
        }
        Ok(())
    }
}

/// Only canonical passkey birth populates this state. Never derive it from the
/// legacy admin_key_grants map or from a presented credential.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasskeyAdminStateV1 {
    pub mother_node_id: String,
    pub ledger_id: String,
    pub root: WebAuthnP256DayKeyV1,
    pub day_keys: BTreeMap<String, Ed25519DayKeyV1>,
    pub used_nonces: BTreeSet<(String, String)>,
    pub last_decided_at: Timestamp,
}

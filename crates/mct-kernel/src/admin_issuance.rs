//! D-56 issuance policy, separate from execution scopes and authentication.
use crate::{
    AdminKeyGrantClassV1 as Class, AdminKeyGrantStateV1 as State, AdminKeyGrantV1,
    AdminKeyPublicHalf, AdminKeyScopeV1 as Scope, Timestamp, admin_key_grant_shape_error,
};
use serde::{Deserialize, Serialize};

/// Explicit permission to request issuance, not permission to execute these scopes.
/// Empty is an explicit revoke-only manager; absence on legacy keys grants nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminIssuanceCeilingV1 {
    pub scopes: Vec<Scope>,
}

impl AdminIssuanceCeilingV1 {
    pub fn is_canonical(&self) -> bool {
        !self.scopes.contains(&Scope::StartupGate) && self.scopes.windows(2).all(|p| p[0] < p[1])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AdminIssuanceError {
    #[error("malformed issuance grant or weak key")]
    Malformed,
    #[error("issuer is not active")]
    InactiveIssuer,
    #[error("issuer is expired")]
    ExpiredIssuer,
    #[error("issuer lacks key-manage")]
    NotKeyManager,
    #[error("day key issuance ceiling is absent")]
    MissingCeiling,
    #[error("recipient execution or delegation exceeds issuer ceiling")]
    CeilingExceeded,
    #[error("recipient expiry is invalid or exceeds issuer expiry")]
    ExpiryExceeded,
    #[error("ordinary issuance cannot create a recovery grant")]
    RecoveryGrantLocked,
    #[error("recovery credentials cannot be reused as a day key")]
    RecoveryCredentialReuse,
}

/// Checks one exact canonical issuer. No union across grants/keys, authentication,
/// nonce consumption, canonical freshness, or effects are supplied by this API.
pub fn evaluate_admin_key_issuance(
    issuer: &AdminKeyGrantV1,
    recipient: &AdminKeyGrantV1,
    now: &Timestamp,
) -> Result<(), AdminIssuanceError> {
    if admin_key_grant_shape_error(issuer).is_some()
        || admin_key_grant_shape_error(recipient).is_some()
        || [issuer, recipient].iter().any(|g| {
            let AdminKeyPublicHalf::Ed25519(key) = g.public_half;
            key.verifying_key().is_weak()
        })
        || recipient.grant_state != State::Active
    {
        return Err(AdminIssuanceError::Malformed);
    }
    if issuer.grant_state != State::Active {
        return Err(AdminIssuanceError::InactiveIssuer);
    }
    if issuer
        .expires_at
        .as_ref()
        .is_some_and(|expires| now >= expires)
    {
        return Err(AdminIssuanceError::ExpiredIssuer);
    }
    if !issuer.scopes.contains(&Scope::KeyManage) {
        return Err(AdminIssuanceError::NotKeyManager);
    }
    if recipient.class != Class::DayToDay {
        return Err(AdminIssuanceError::RecoveryGrantLocked);
    }
    let expires = recipient
        .expires_at
        .as_ref()
        .ok_or(AdminIssuanceError::ExpiryExceeded)?;
    if expires <= now
        || issuer
            .expires_at
            .as_ref()
            .is_some_and(|limit| expires > limit)
    {
        return Err(AdminIssuanceError::ExpiryExceeded);
    }
    if recipient.scopes.contains(&Scope::KeyManage) && recipient.issuance_ceiling.is_none() {
        return Err(AdminIssuanceError::MissingCeiling);
    }
    if issuer.class == Class::Recovery {
        if issuer.public_half == recipient.public_half
            || issuer
                .ml_dsa65_public_key
                .as_ref()
                .is_some_and(|pq| recipient.ml_dsa65_public_key.as_ref() == Some(pq))
        {
            return Err(AdminIssuanceError::RecoveryCredentialReuse);
        }
        return Ok(());
    }
    let ceiling = issuer
        .issuance_ceiling
        .as_ref()
        .ok_or(AdminIssuanceError::MissingCeiling)?;
    if recipient
        .scopes
        .iter()
        .any(|scope| !ceiling.scopes.contains(scope))
        || recipient
            .issuance_ceiling
            .as_ref()
            .is_some_and(|delegated| {
                delegated
                    .scopes
                    .iter()
                    .any(|scope| !ceiling.scopes.contains(scope))
            })
    {
        return Err(AdminIssuanceError::CeilingExceeded);
    }
    Ok(())
}

#[cfg(test)]
mod tests;

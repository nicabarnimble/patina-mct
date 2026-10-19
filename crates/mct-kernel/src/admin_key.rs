//! Admin-key grant check.
//!
//! Mother is the only grantor. This module decides whether a presented public
//! half may exercise one route scope. It does not mint a grant, a Child
//! handle, or a session token. Proof of possession is a later check; a grant
//! fact alone is not a proof.

use crate::grant_check::{ExpiryDecision, ExpiryRequirement, action_is_allowed, expiry_decision};
use crate::id::Timestamp;
use ed25519_dalek::VerifyingKey;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Route scope named on an admin-key grant.
///
/// The recovery grant carries only [`AdminKeyScopeV1::StartupGate`] and
/// [`AdminKeyScopeV1::KeyManage`]. A day-to-day grant cannot carry the startup gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminKeyScopeV1 {
    /// `POST /children/approve`.
    ApproveChild,
    /// Source create/revoke, artifact stage, and release acquire.
    Artifact,
    /// `POST /authority/import-toy-state`.
    AuthorityImport,
    /// `POST /toys/authorize-secret`.
    AuthorizeSecret,
    /// `POST /toys/authorize-slate`.
    AuthorizeSlate,
    /// `POST /blobs`.
    Blob,
    /// Issue, narrow, and revoke admin keys. Not implied by any other scope.
    KeyManage,
    /// `POST /lifecycle/fact`.
    Lifecycle,
    /// `POST /pando/record`. Stays on the admin surface.
    PandoRecord,
    /// `POST /peers/add`.
    PeerAdd,
    /// `POST /peers/proof`.
    PeerProof,
    /// `POST /peers/remove`.
    PeerRemove,
    /// `POST /peers/revoke`.
    PeerRevoke,
    /// Registry install and sync.
    Registry,
    /// `POST /children/revoke`.
    RevokeChild,
    /// Operator startup gate. Recovery grant only.
    StartupGate,
    /// Read-only admin status routes.
    Status,
    /// Trigger create, revise, and revoke.
    Trigger,
    /// Watch grant routes.
    WatchGrant,
    /// Watch revoke routes.
    WatchRevoke,
}

impl AdminKeyScopeV1 {
    /// Action-list spelling for the shared grant matcher.
    ///
    /// These are the serde names of the scope. The matcher compares them the
    /// same way a toy grant compares `allowed_actions`.
    pub fn action_name(self) -> &'static str {
        match self {
            Self::ApproveChild => "approve_child",
            Self::Artifact => "artifact",
            Self::AuthorityImport => "authority_import",
            Self::AuthorizeSecret => "authorize_secret",
            Self::AuthorizeSlate => "authorize_slate",
            Self::Blob => "blob",
            Self::KeyManage => "key_manage",
            Self::Lifecycle => "lifecycle",
            Self::PandoRecord => "pando_record",
            Self::PeerAdd => "peer_add",
            Self::PeerProof => "peer_proof",
            Self::PeerRemove => "peer_remove",
            Self::PeerRevoke => "peer_revoke",
            Self::Registry => "registry",
            Self::RevokeChild => "revoke_child",
            Self::StartupGate => "startup_gate",
            Self::Status => "status",
            Self::Trigger => "trigger",
            Self::WatchGrant => "watch_grant",
            Self::WatchRevoke => "watch_revoke",
        }
    }
}

/// The recovery grant is the startup gate plus key-manage, in canonical order.
pub fn recovery_grant_scopes() -> &'static [AdminKeyScopeV1] {
    &[AdminKeyScopeV1::KeyManage, AdminKeyScopeV1::StartupGate]
}

/// Day-to-day keys expire. The recovery grant is the startup gate plus key-manage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminKeyGrantClassV1 {
    /// Route scopes, mandatory expiry, and no startup gate.
    DayToDay,
    /// Startup gate plus key-manage. Nothing revokes this grant.
    Recovery,
}

/// Active grants authorize. Revoked is terminal for a day-to-day key.
/// The recovery grant stays active: nothing can revoke it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminKeyGrantStateV1 {
    /// Grant may authorize a route scope it names.
    Active,
    /// Terminal. A later put cannot resurrect this grant id.
    Revoked,
}

/// Scheme of an admin-key public half.
///
/// The public half stays Ed25519 so iroh transport never sees ML-DSA. The
/// ML-DSA-65 verifying key sits beside it on the grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminKeySchemeV1 {
    /// One Ed25519 verifying key.
    Ed25519,
}

/// Ed25519 verifying key checked by [`VerifyingKey`].
///
/// The bytes are the canonical compressed Edwards-Y encoding.
#[derive(Clone, Copy, Eq)]
pub struct Ed25519VerifyingKey(VerifyingKey);

impl PartialEq for Ed25519VerifyingKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_bytes() == other.0.as_bytes()
    }
}

impl std::fmt::Debug for Ed25519VerifyingKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Ed25519VerifyingKey(")?;
        formatter.write_str(&hex_encode(self.0.as_bytes()))?;
        formatter.write_str(")")
    }
}

impl Ed25519VerifyingKey {
    /// Returns the dalek verifying key.
    pub fn verifying_key(self) -> VerifyingKey {
        self.0
    }

    /// Returns the canonical 32-byte compressed public key.
    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_bytes()
    }
}

/// Versioned public half of an admin key.
///
/// The only variant is an Ed25519 verifying key. ML-DSA-65 is not folded into
/// this encoding: it is a sibling field on the grant. Construction rejects
/// text that is not a canonical Ed25519 point, including armored secrets,
/// empty text, and hex that does not decompress.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AdminKeyPublicHalf {
    /// Canonical Ed25519 public key.
    Ed25519(Ed25519VerifyingKey),
}

impl std::fmt::Debug for AdminKeyPublicHalf {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ed25519(key) => formatter.debug_tuple("Ed25519").field(key).finish(),
        }
    }
}

impl AdminKeyPublicHalf {
    /// Parses a canonical 32-byte Ed25519 public key.
    ///
    /// # Errors
    ///
    /// Returns an error when `bytes` is not a dalek [`VerifyingKey`] in
    /// canonical compressed form.
    pub fn ed25519_from_bytes(bytes: &[u8; 32]) -> Result<Self, &'static str> {
        let key = VerifyingKey::from_bytes(bytes).map_err(|_| NOT_ED25519_PUBLIC_KEY)?;
        let canonical = key.to_edwards().compress().to_bytes();
        if canonical != *bytes {
            return Err(NOT_ED25519_PUBLIC_KEY);
        }
        Ok(Self::Ed25519(Ed25519VerifyingKey(key)))
    }

    /// Parses 64 hex characters of a canonical Ed25519 public key.
    ///
    /// # Errors
    ///
    /// Returns an error when `hex` is not a canonical Ed25519 point. Empty
    /// text, armored secrets, and other non-points fail.
    pub fn ed25519_from_hex(hex: &str) -> Result<Self, &'static str> {
        let bytes = hex_decode_32(hex)?;
        Self::ed25519_from_bytes(&bytes)
    }

    /// Scheme stored in this public half.
    pub fn scheme(self) -> AdminKeySchemeV1 {
        match self {
            Self::Ed25519(_) => AdminKeySchemeV1::Ed25519,
        }
    }

    /// Canonical compressed public-key bytes for the Ed25519 scheme.
    pub fn to_ed25519_bytes(self) -> [u8; 32] {
        match self {
            Self::Ed25519(key) => key.to_bytes(),
        }
    }

    /// Lowercase hex of [`Self::to_ed25519_bytes`].
    pub fn to_hex(self) -> String {
        hex_encode(&self.to_ed25519_bytes())
    }
}

impl Serialize for AdminKeyPublicHalf {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct as _;
        let mut state = serializer.serialize_struct("AdminKeyPublicHalf", 2)?;
        state.serialize_field("scheme", "ed25519")?;
        let hex = self.to_hex();
        state.serialize_field("public_key", &hex)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for AdminKeyPublicHalf {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            scheme: String,
            public_key: String,
        }

        let wire = Wire::deserialize(deserializer)?;
        if wire.scheme != "ed25519" {
            return Err(D::Error::custom(
                "admin key public half scheme is not ed25519",
            ));
        }
        Self::ed25519_from_hex(&wire.public_key).map_err(D::Error::custom)
    }
}

const NOT_ED25519_PUBLIC_KEY: &str = "admin key public half must be an Ed25519 verifying key";

/// Why a hex public-key string is not an Ed25519 verifying key.
///
/// `None` means `hex` is 64 hex characters of a canonical Ed25519 point.
pub fn admin_key_public_half_error(hex: &str) -> Option<&'static str> {
    AdminKeyPublicHalf::ed25519_from_hex(hex).err()
}

fn hex_encode(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode_32(value: &str) -> Result<[u8; 32], &'static str> {
    let bytes = value.as_bytes();
    if bytes.len() != 64 {
        return Err(NOT_ED25519_PUBLIC_KEY);
    }
    let mut out = [0u8; 32];
    for index in 0..32 {
        let high = hex_nibble(bytes[index * 2])?;
        let low = hex_nibble(bytes[index * 2 + 1])?;
        out[index] = (high << 4) | low;
    }
    Ok(out)
}

fn hex_nibble(byte: u8) -> Result<u8, &'static str> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(NOT_ED25519_PUBLIC_KEY),
    }
}

/// Ledger fact for one admin key. Mother stores the public half only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminKeyGrantV1 {
    /// Stable grant identifier.
    pub grant_id: String,
    /// Ed25519 verifying key. Raw key material cannot be stored in this field.
    pub public_half: AdminKeyPublicHalf,
    /// ML-DSA-65 verifying key. Absent on a pre-scheme grant. A rotation writes
    /// this key and the Ed25519 key together; one cannot be replaced alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ml_dsa65_public_key: Option<crate::admin_sig::MlDsa65PublicKey>,
    /// Day-to-day or recovery.
    pub class: AdminKeyGrantClassV1,
    /// Route scopes, strictly ordered and non-empty.
    pub scopes: Vec<AdminKeyScopeV1>,
    /// Mandatory for a day-to-day key. Absent on the recovery grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Timestamp>,
    /// Active or revoked.
    pub grant_state: AdminKeyGrantStateV1,
    /// D-56 explicit day-key issuance limit; absence never means unlimited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuance_ceiling: Option<crate::AdminIssuanceCeilingV1>,
}

/// Why an admin-key grant fact is not admissible.
///
/// `None` means the shape is the one the ledger and the check both accept.
pub fn admin_key_grant_shape_error(grant: &AdminKeyGrantV1) -> Option<&'static str> {
    if !non_empty_token(&grant.grant_id) {
        return Some("admin key grant id is empty");
    }
    if grant.public_half.scheme() != AdminKeySchemeV1::Ed25519 {
        return Some("admin key public half scheme is not ed25519");
    }
    if !admin_key_scopes_are_canonical(&grant.scopes) {
        return Some("admin key scopes are empty or not strictly ordered");
    }
    if let Some(ceiling) = &grant.issuance_ceiling
        && (grant.class != AdminKeyGrantClassV1::DayToDay
            || !grant.scopes.contains(&AdminKeyScopeV1::KeyManage)
            || !ceiling.is_canonical())
    {
        return Some("issuance ceiling requires day key-manage, ordered unique non-startup scopes");
    }
    match grant.class {
        AdminKeyGrantClassV1::DayToDay => {
            if grant.expires_at.is_none() {
                return Some("day-to-day admin key grant requires expiry");
            }
            if grant.scopes.contains(&AdminKeyScopeV1::StartupGate) {
                return Some("day-to-day admin key grant cannot carry the startup gate");
            }
            None
        }
        AdminKeyGrantClassV1::Recovery => {
            if grant.expires_at.is_some() {
                return Some("recovery grant is not a day-to-day key and does not carry an expiry");
            }
            if grant.scopes.as_slice() != recovery_grant_scopes() {
                return Some("recovery grant is the startup gate plus key-manage");
            }
            None
        }
    }
}

/// What the presented key is asking to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminKeyCheckAction {
    /// One route scope. Membership uses the shared action list.
    Route(AdminKeyScopeV1),
    /// `key-manage` used to revoke one grant.
    ///
    /// The recovery grant is the exception: the check denies every presenter,
    /// including a day-to-day key that itself holds `key-manage`.
    Revoke {
        /// Grant id the presenter wants to revoke.
        target_grant_id: String,
    },
}

/// Facts for one admin-key grant check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminKeyGrantCheckRequest {
    /// Presented Ed25519 verifying key. A secret cannot be represented here.
    pub public_half: AdminKeyPublicHalf,
    /// Route scope, or a revoke of one grant.
    pub action: AdminKeyCheckAction,
    /// Mother clock used for mandatory day-to-day expiry.
    pub now: Timestamp,
    /// Generation the caller claims to be checking against.
    pub presented_generation: u64,
    /// Current authority generation. Any other presented generation is stale.
    pub current_generation: u64,
}

/// Verdict of one admin-key grant check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminKeyGrantVerdict {
    /// The presented key may perform this action.
    Allowed,
    /// The presented key may not. No token is minted.
    Denied,
}

/// Why an admin-key check allowed or denied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminKeyGrantReason {
    /// An active, in-window grant named the route scope.
    ActiveGrant,
    /// The revoke target is not in the grant set.
    MissingGrant,
    /// No grant carries the presented public half.
    WrongKey,
    /// A day-to-day grant's expiry is at or before `now`.
    ExpiredGrant,
    /// The matching grant is revoked.
    RevokedGrant,
    /// The grant does not name the requested route scope.
    WrongScope,
    /// `presented_generation` is not the current authority generation.
    StaleGeneration,
    /// The recovery grant cannot be revoked. A day-to-day key is included.
    RecoveryGrantLocked,
    /// The grant shape or the revoke subject is not admissible.
    PolicyDenied,
}

/// Decision for one admin-key check.
///
/// An allow names the grant and the scope. It is not a session token, not a
/// Child handle, and not reusable authority: the next call checks again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminKeyGrantCheckResult {
    /// Allowed or denied.
    pub verdict: AdminKeyGrantVerdict,
    /// Typed reason.
    pub reason: AdminKeyGrantReason,
    /// Grant considered for this decision, when one was.
    pub grant_id: Option<String>,
    /// Scope admitted by an allow. Absent on every deny.
    pub admitted_scope: Option<AdminKeyScopeV1>,
}

impl AdminKeyGrantCheckResult {
    /// True only when the check allowed and named both the grant and the scope.
    pub fn is_allowed(&self) -> bool {
        self.verdict == AdminKeyGrantVerdict::Allowed
            && self.grant_id.is_some()
            && self.admitted_scope.is_some()
    }
}

/// Decides whether the presented public half may exercise one route scope.
///
/// Default-deny: no grant, wrong key, wrong scope, revoked, expired, missing
/// day-to-day expiry, stale generation, or a revoke of the recovery grant
/// yields a denied result and no token. The recovery grant authorizes only
/// the startup gate and `key-manage`.
pub fn evaluate_admin_key_grant(
    request: &AdminKeyGrantCheckRequest,
    grants: &[AdminKeyGrantV1],
) -> AdminKeyGrantCheckResult {
    if request.presented_generation != request.current_generation {
        return deny(AdminKeyGrantReason::StaleGeneration, None);
    }

    let route_scope = match &request.action {
        AdminKeyCheckAction::Route(scope) => *scope,
        AdminKeyCheckAction::Revoke { .. } => AdminKeyScopeV1::KeyManage,
    };
    let presenter = decide_route(request, grants, route_scope);
    let AdminKeyCheckAction::Revoke { target_grant_id } = &request.action else {
        return presenter;
    };
    if !presenter.is_allowed() {
        return presenter;
    }
    decide_revoke(&presenter, grants, &request.public_half, target_grant_id)
}

fn decide_route(
    request: &AdminKeyGrantCheckRequest,
    grants: &[AdminKeyGrantV1],
    scope: AdminKeyScopeV1,
) -> AdminKeyGrantCheckResult {
    let candidates: Vec<&AdminKeyGrantV1> = grants
        .iter()
        .filter(|grant| grant.public_half == request.public_half)
        .collect();
    if candidates.is_empty() {
        return deny(AdminKeyGrantReason::WrongKey, None);
    }
    if let Some(grant) = candidates
        .iter()
        .copied()
        .find(|grant| grant_authorizes(grant, scope, &request.now))
    {
        return AdminKeyGrantCheckResult {
            verdict: AdminKeyGrantVerdict::Allowed,
            reason: AdminKeyGrantReason::ActiveGrant,
            grant_id: Some(grant.grant_id.clone()),
            admitted_scope: Some(scope),
        };
    }
    let (reason, grant_id) = classify_route_deny(&candidates, scope, &request.now);
    deny(reason, grant_id)
}

fn decide_revoke(
    presenter: &AdminKeyGrantCheckResult,
    grants: &[AdminKeyGrantV1],
    presenter_public_half: &AdminKeyPublicHalf,
    target_grant_id: &str,
) -> AdminKeyGrantCheckResult {
    let Some(presenter_grant_id) = presenter.grant_id.clone() else {
        return deny(AdminKeyGrantReason::PolicyDenied, None);
    };
    let Some(target) = grants
        .iter()
        .find(|grant| grant.grant_id == target_grant_id)
    else {
        return deny(AdminKeyGrantReason::MissingGrant, None);
    };
    if target.class == AdminKeyGrantClassV1::Recovery {
        return deny(
            AdminKeyGrantReason::RecoveryGrantLocked,
            Some(target.grant_id.clone()),
        );
    }
    if &target.public_half == presenter_public_half {
        return deny(
            AdminKeyGrantReason::PolicyDenied,
            Some(target.grant_id.clone()),
        );
    }
    if target.grant_state != AdminKeyGrantStateV1::Active {
        return deny(
            AdminKeyGrantReason::RevokedGrant,
            Some(target.grant_id.clone()),
        );
    }
    if admin_key_grant_shape_error(target).is_some() {
        return deny(
            AdminKeyGrantReason::PolicyDenied,
            Some(target.grant_id.clone()),
        );
    }
    AdminKeyGrantCheckResult {
        verdict: AdminKeyGrantVerdict::Allowed,
        reason: AdminKeyGrantReason::ActiveGrant,
        grant_id: Some(presenter_grant_id),
        admitted_scope: Some(AdminKeyScopeV1::KeyManage),
    }
}

fn grant_authorizes(grant: &AdminKeyGrantV1, scope: AdminKeyScopeV1, now: &Timestamp) -> bool {
    admin_key_grant_shape_error(grant).is_none()
        && grant.grant_state == AdminKeyGrantStateV1::Active
        && expiry_decision(
            now,
            None,
            grant.expires_at.as_ref(),
            expiry_requirement(grant),
        ) == ExpiryDecision::Open
        && scope_allowed(&grant.scopes, scope)
}

fn classify_route_deny(
    candidates: &[&AdminKeyGrantV1],
    scope: AdminKeyScopeV1,
    now: &Timestamp,
) -> (AdminKeyGrantReason, Option<String>) {
    let mut revoked_id = None;
    let mut malformed_id = None;
    for grant in candidates {
        if admin_key_grant_shape_error(grant).is_some() {
            malformed_id = Some(grant.grant_id.clone());
            continue;
        }
        if grant.grant_state != AdminKeyGrantStateV1::Active {
            revoked_id = Some(grant.grant_id.clone());
            continue;
        }
        match expiry_decision(
            now,
            None,
            grant.expires_at.as_ref(),
            expiry_requirement(grant),
        ) {
            ExpiryDecision::Expired => {
                return (
                    AdminKeyGrantReason::ExpiredGrant,
                    Some(grant.grant_id.clone()),
                );
            }
            ExpiryDecision::Missing
            | ExpiryDecision::PresentButForbidden
            | ExpiryDecision::NotYet => {
                return (
                    AdminKeyGrantReason::PolicyDenied,
                    Some(grant.grant_id.clone()),
                );
            }
            ExpiryDecision::Open => {
                if !scope_allowed(&grant.scopes, scope) {
                    return (
                        AdminKeyGrantReason::WrongScope,
                        Some(grant.grant_id.clone()),
                    );
                }
            }
        }
    }
    if let Some(grant_id) = malformed_id {
        return (AdminKeyGrantReason::PolicyDenied, Some(grant_id));
    }
    if let Some(grant_id) = revoked_id {
        return (AdminKeyGrantReason::RevokedGrant, Some(grant_id));
    }
    (AdminKeyGrantReason::MissingGrant, None)
}

fn expiry_requirement(grant: &AdminKeyGrantV1) -> ExpiryRequirement {
    match grant.class {
        AdminKeyGrantClassV1::DayToDay => ExpiryRequirement::Mandatory,
        AdminKeyGrantClassV1::Recovery => ExpiryRequirement::Forbidden,
    }
}

fn scope_allowed(scopes: &[AdminKeyScopeV1], scope: AdminKeyScopeV1) -> bool {
    action_is_allowed(
        scopes.iter().copied().map(AdminKeyScopeV1::action_name),
        scope.action_name(),
    )
}

fn deny(reason: AdminKeyGrantReason, grant_id: Option<String>) -> AdminKeyGrantCheckResult {
    AdminKeyGrantCheckResult {
        verdict: AdminKeyGrantVerdict::Denied,
        reason,
        grant_id,
        admitted_scope: None,
    }
}

fn non_empty_token(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_whitespace)
}

fn admin_key_scopes_are_canonical(scopes: &[AdminKeyScopeV1]) -> bool {
    !scopes.is_empty() && scopes.windows(2).all(|pair| pair[0] < pair[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        Timestamp::new("2026-06-01T00:00:00Z").unwrap()
    }

    fn later() -> Timestamp {
        Timestamp::new("2026-10-14T00:00:00Z").unwrap()
    }

    /// RFC 8032 test 1 public key.
    const DAY_PK: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
    /// RFC 8032 test 2 public key.
    const RECOVERY_PK: &str = "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c";
    /// RFC 8032 test 3 public key.
    const OTHER_PK: &str = "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025";
    /// A different valid Ed25519 point, absent from the grant set.
    const ABSENT_PK: &str = "278117fc144c72340f67d0f2316e8386ceffbf2b2428c9c51fef7c597f1d426e";

    fn half(value: &str) -> AdminKeyPublicHalf {
        AdminKeyPublicHalf::ed25519_from_hex(value).expect("ed25519 public key")
    }

    fn day_to_day(scopes: Vec<AdminKeyScopeV1>) -> AdminKeyGrantV1 {
        AdminKeyGrantV1 {
            grant_id: "day-to-day".into(),
            issuance_ceiling: None,
            public_half: half(DAY_PK),
            ml_dsa65_public_key: None,
            class: AdminKeyGrantClassV1::DayToDay,
            scopes,
            expires_at: Some(later()),
            grant_state: AdminKeyGrantStateV1::Active,
        }
    }

    fn recovery() -> AdminKeyGrantV1 {
        AdminKeyGrantV1 {
            grant_id: "recovery".into(),
            issuance_ceiling: None,
            public_half: half(RECOVERY_PK),
            ml_dsa65_public_key: None,
            class: AdminKeyGrantClassV1::Recovery,
            scopes: recovery_grant_scopes().to_vec(),
            expires_at: None,
            grant_state: AdminKeyGrantStateV1::Active,
        }
    }

    fn request(public_half: &str, action: AdminKeyCheckAction) -> AdminKeyGrantCheckRequest {
        AdminKeyGrantCheckRequest {
            public_half: half(public_half),
            action,
            now: now(),
            presented_generation: 4,
            current_generation: 4,
        }
    }

    fn allow(grant_id: &str, scope: AdminKeyScopeV1) -> AdminKeyGrantCheckResult {
        AdminKeyGrantCheckResult {
            verdict: AdminKeyGrantVerdict::Allowed,
            reason: AdminKeyGrantReason::ActiveGrant,
            grant_id: Some(grant_id.to_owned()),
            admitted_scope: Some(scope),
        }
    }

    fn deny(reason: AdminKeyGrantReason, grant_id: Option<&str>) -> AdminKeyGrantCheckResult {
        AdminKeyGrantCheckResult {
            verdict: AdminKeyGrantVerdict::Denied,
            reason,
            grant_id: grant_id.map(str::to_owned),
            admitted_scope: None,
        }
    }

    #[test]
    fn day_to_day_key_allows_its_route_scope_and_denies_another() {
        let grant = day_to_day(vec![AdminKeyScopeV1::Status]);
        let allowed = evaluate_admin_key_grant(
            &request(DAY_PK, AdminKeyCheckAction::Route(AdminKeyScopeV1::Status)),
            std::slice::from_ref(&grant),
        );
        assert_eq!(allowed, allow("day-to-day", AdminKeyScopeV1::Status));

        let denied = evaluate_admin_key_grant(
            &request(DAY_PK, AdminKeyCheckAction::Route(AdminKeyScopeV1::PeerAdd)),
            &[grant],
        );
        assert_eq!(
            denied,
            deny(AdminKeyGrantReason::WrongScope, Some("day-to-day"))
        );
    }

    #[test]
    fn wrong_key_missing_scope_revoked_and_stale_generation_deny() {
        let grant = day_to_day(vec![AdminKeyScopeV1::Status]);
        let wrong_key = evaluate_admin_key_grant(
            &request(
                ABSENT_PK,
                AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),
            ),
            std::slice::from_ref(&grant),
        );
        assert_eq!(wrong_key, deny(AdminKeyGrantReason::WrongKey, None));

        let mut revoked = grant.clone();
        revoked.grant_state = AdminKeyGrantStateV1::Revoked;
        let revoked_result = evaluate_admin_key_grant(
            &request(DAY_PK, AdminKeyCheckAction::Route(AdminKeyScopeV1::Status)),
            &[revoked],
        );
        assert_eq!(
            revoked_result,
            deny(AdminKeyGrantReason::RevokedGrant, Some("day-to-day"))
        );

        let mut stale = request(DAY_PK, AdminKeyCheckAction::Route(AdminKeyScopeV1::Status));
        stale.presented_generation = 3;
        let stale_result = evaluate_admin_key_grant(&stale, &[grant]);
        assert_eq!(
            stale_result,
            deny(AdminKeyGrantReason::StaleGeneration, None)
        );
    }

    #[test]
    fn day_to_day_expiry_is_mandatory() {
        let mut permanent = day_to_day(vec![AdminKeyScopeV1::Status]);
        permanent.expires_at = None;
        let missing = evaluate_admin_key_grant(
            &request(DAY_PK, AdminKeyCheckAction::Route(AdminKeyScopeV1::Status)),
            &[permanent],
        );
        assert_eq!(
            missing,
            deny(AdminKeyGrantReason::PolicyDenied, Some("day-to-day"))
        );

        let mut expired = day_to_day(vec![AdminKeyScopeV1::Status]);
        expired.expires_at = Some(now());
        let at_expiry = evaluate_admin_key_grant(
            &request(DAY_PK, AdminKeyCheckAction::Route(AdminKeyScopeV1::Status)),
            &[expired],
        );
        assert_eq!(
            at_expiry,
            deny(AdminKeyGrantReason::ExpiredGrant, Some("day-to-day"))
        );
    }

    #[test]
    fn recovery_grant_is_gate_plus_key_manage_only() {
        let mut grant = recovery();
        let gate = evaluate_admin_key_grant(
            &request(
                RECOVERY_PK,
                AdminKeyCheckAction::Route(AdminKeyScopeV1::StartupGate),
            ),
            std::slice::from_ref(&grant),
        );
        assert_eq!(gate, allow("recovery", AdminKeyScopeV1::StartupGate));

        let manage = evaluate_admin_key_grant(
            &request(
                RECOVERY_PK,
                AdminKeyCheckAction::Route(AdminKeyScopeV1::KeyManage),
            ),
            std::slice::from_ref(&grant),
        );
        assert_eq!(manage, allow("recovery", AdminKeyScopeV1::KeyManage));

        let status = evaluate_admin_key_grant(
            &request(
                RECOVERY_PK,
                AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),
            ),
            std::slice::from_ref(&grant),
        );
        assert_eq!(
            status,
            deny(AdminKeyGrantReason::WrongScope, Some("recovery"))
        );

        let mut far_future = request(
            RECOVERY_PK,
            AdminKeyCheckAction::Route(AdminKeyScopeV1::StartupGate),
        );
        far_future.now = Timestamp::new("2099-01-01T00:00:00Z").unwrap();
        assert_eq!(
            evaluate_admin_key_grant(&far_future, std::slice::from_ref(&grant)),
            allow("recovery", AdminKeyScopeV1::StartupGate)
        );

        grant.expires_at = Some(later());
        let dated = evaluate_admin_key_grant(
            &request(
                RECOVERY_PK,
                AdminKeyCheckAction::Route(AdminKeyScopeV1::StartupGate),
            ),
            &[grant],
        );
        assert_eq!(
            dated,
            deny(AdminKeyGrantReason::PolicyDenied, Some("recovery"))
        );
    }

    #[test]
    fn day_to_day_key_cannot_pass_the_startup_gate() {
        let grant = day_to_day(vec![AdminKeyScopeV1::KeyManage, AdminKeyScopeV1::Status]);
        let gate = evaluate_admin_key_grant(
            &request(
                DAY_PK,
                AdminKeyCheckAction::Route(AdminKeyScopeV1::StartupGate),
            ),
            std::slice::from_ref(&grant),
        );
        assert_eq!(
            gate,
            deny(AdminKeyGrantReason::WrongScope, Some("day-to-day"))
        );

        let mut carries_gate = grant;
        carries_gate.scopes = vec![AdminKeyScopeV1::StartupGate, AdminKeyScopeV1::Status];
        let malformed = evaluate_admin_key_grant(
            &request(DAY_PK, AdminKeyCheckAction::Route(AdminKeyScopeV1::Status)),
            &[carries_gate],
        );
        assert_eq!(
            malformed,
            deny(AdminKeyGrantReason::PolicyDenied, Some("day-to-day"))
        );
    }

    #[test]
    fn day_to_day_key_cannot_revoke_the_recovery_grant() {
        let day = day_to_day(vec![AdminKeyScopeV1::KeyManage, AdminKeyScopeV1::Status]);
        let recovery = recovery();
        let revoked = evaluate_admin_key_grant(
            &request(
                DAY_PK,
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "recovery".into(),
                },
            ),
            &[day.clone(), recovery.clone()],
        );
        assert_eq!(
            revoked,
            deny(AdminKeyGrantReason::RecoveryGrantLocked, Some("recovery"))
        );

        let outcome = crate::evaluate_grant_subject(crate::GrantCheckSubject::AdminKey {
            request: &request(
                DAY_PK,
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "recovery".into(),
                },
            ),
            grants: &[day, recovery],
        });
        assert_eq!(
            outcome,
            crate::GrantCheckOutcome::AdminKey(deny(
                AdminKeyGrantReason::RecoveryGrantLocked,
                Some("recovery"),
            ))
        );
    }

    #[test]
    fn recovery_key_cannot_revoke_itself_and_can_revoke_a_day_to_day_key() {
        let day = day_to_day(vec![AdminKeyScopeV1::Status]);
        let recovery = recovery();
        let self_revoke = evaluate_admin_key_grant(
            &request(
                RECOVERY_PK,
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "recovery".into(),
                },
            ),
            &[day.clone(), recovery.clone()],
        );
        assert_eq!(
            self_revoke,
            deny(AdminKeyGrantReason::RecoveryGrantLocked, Some("recovery"))
        );

        let revoke_day = evaluate_admin_key_grant(
            &request(
                RECOVERY_PK,
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "day-to-day".into(),
                },
            ),
            &[day, recovery],
        );
        assert_eq!(revoke_day, allow("recovery", AdminKeyScopeV1::KeyManage));
    }

    #[test]
    fn day_to_day_key_cannot_revoke_itself_and_can_revoke_another_day_to_day_key() {
        let mut other = day_to_day(vec![AdminKeyScopeV1::Status]);
        other.grant_id = "other-day".into();
        other.public_half = half(OTHER_PK);
        let holder = day_to_day(vec![AdminKeyScopeV1::KeyManage]);
        let self_revoke = evaluate_admin_key_grant(
            &request(
                DAY_PK,
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "day-to-day".into(),
                },
            ),
            &[holder.clone(), other.clone()],
        );
        assert_eq!(
            self_revoke,
            deny(AdminKeyGrantReason::PolicyDenied, Some("day-to-day"))
        );

        let revoke_other = evaluate_admin_key_grant(
            &request(
                DAY_PK,
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "other-day".into(),
                },
            ),
            &[holder, other],
        );
        assert_eq!(
            revoke_other,
            allow("day-to-day", AdminKeyScopeV1::KeyManage)
        );
    }

    #[test]
    fn key_manage_is_not_implied_by_another_scope() {
        let grant = day_to_day(vec![AdminKeyScopeV1::PeerAdd, AdminKeyScopeV1::Status]);
        let revoke = evaluate_admin_key_grant(
            &request(
                DAY_PK,
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "missing".into(),
                },
            ),
            &[grant],
        );
        assert_eq!(
            revoke,
            deny(AdminKeyGrantReason::WrongScope, Some("day-to-day"))
        );
    }

    #[test]
    fn empty_grant_set_denies() {
        let denied = evaluate_admin_key_grant(
            &request(DAY_PK, AdminKeyCheckAction::Route(AdminKeyScopeV1::Status)),
            &[],
        );
        assert_eq!(denied, deny(AdminKeyGrantReason::WrongKey, None));
    }

    #[test]
    fn admin_grant_json_is_the_public_half_and_rejects_raw_key_material() {
        let grant = day_to_day(vec![AdminKeyScopeV1::Status]);
        assert_eq!(
            serde_json::to_value(&grant).unwrap(),
            serde_json::json!({
                "grant_id": "day-to-day",
                "public_half": {
                    "scheme": "ed25519",
                    "public_key": DAY_PK
                },
                "class": "day_to_day",
                "scopes": ["status"],
                "expires_at": "2026-10-14T00:00:00Z",
                "grant_state": "active"
            })
        );
        assert_eq!(grant.public_half.to_hex(), DAY_PK);
        assert_eq!(
            grant.public_half.to_ed25519_bytes(),
            hex_decode_32(DAY_PK).unwrap()
        );
        assert_eq!(grant.public_half.scheme(), AdminKeySchemeV1::Ed25519);

        let uppercase = DAY_PK.to_ascii_uppercase();
        assert_eq!(
            AdminKeyPublicHalf::ed25519_from_hex(&uppercase)
                .expect("uppercase hex of a point")
                .to_hex(),
            DAY_PK
        );

        for rejected in [
            "",
            "aabbccddeeff",
            "-----BEGINPRIVATEKEY-----abc",
            "-----BEGIN PRIVATE KEY-----\nabc",
            "-----BEGINPUBLICKEY-----abc",
            &"ff".repeat(64),
            // y = 2^255 - 19 is the field prime, not a reduced Edwards coordinate.
            "edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
        ] {
            assert_eq!(
                AdminKeyPublicHalf::ed25519_from_hex(rejected),
                Err("admin key public half must be an Ed25519 verifying key"),
                "{rejected}"
            );
        }

        let armored = r#"{
            "grant_id": "day-to-day",
            "public_half": "-----BEGINPRIVATEKEY-----abc",
            "class": "day_to_day",
            "scopes": ["status"],
            "expires_at": "2026-10-14T00:00:00Z",
            "grant_state": "active"
        }"#;
        let armored_error = serde_json::from_str::<AdminKeyGrantV1>(armored).unwrap_err();
        assert_eq!(
            armored_error.to_string(),
            "invalid type: string \"-----BEGINPRIVATEKEY-----abc\", expected struct Wire at line 3 column 57"
        );

        let off_curve = r#"{
            "grant_id": "day-to-day",
            "public_half": {
                "scheme": "ed25519",
                "public_key": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
            },
            "class": "day_to_day",
            "scopes": ["status"],
            "expires_at": "2026-10-14T00:00:00Z",
            "grant_state": "active"
        }"#;
        assert_eq!(
            serde_json::from_str::<AdminKeyGrantV1>(off_curve)
                .unwrap_err()
                .to_string(),
            "admin key public half must be an Ed25519 verifying key at line 6 column 13"
        );

        let ml_dsa = r#"{"scheme":"ed25519_ml_dsa65","public_key":"d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"}"#;
        assert_eq!(
            serde_json::from_str::<AdminKeyPublicHalf>(ml_dsa)
                .unwrap_err()
                .to_string(),
            "admin key public half scheme is not ed25519"
        );
    }

    /// RFC 8032 test 1 seed, compared with the public key derived from it.
    #[test]
    fn ed25519_seed_bytes_are_checked_as_a_point_not_as_a_secret() {
        const SEED: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
        let seed = hex_decode_32(SEED).unwrap();
        let derived = ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key();
        assert_eq!(hex_encode(derived.as_bytes()), DAY_PK);
        assert_eq!(
            AdminKeyPublicHalf::ed25519_from_bytes(derived.as_bytes())
                .expect("derived public key")
                .to_hex(),
            DAY_PK
        );
        assert_eq!(
            AdminKeyPublicHalf::ed25519_from_bytes(&seed).map(|key| key.to_hex()),
            Err("admin key public half must be an Ed25519 verifying key")
        );

        // These 32 bytes are a valid point, so they are also a legal seed.
        let point_as_seed = hex_decode_32(DAY_PK).unwrap();
        assert_eq!(
            AdminKeyPublicHalf::ed25519_from_bytes(&point_as_seed)
                .expect("a valid point is accepted")
                .to_hex(),
            DAY_PK
        );
        let derived = hex_encode(
            ed25519_dalek::SigningKey::from_bytes(&point_as_seed)
                .verifying_key()
                .as_bytes(),
        );
        assert_ne!(derived, DAY_PK);
        assert_eq!(
            derived,
            "134c745ace0685684eff306853268b6eea82cfa84376ad7516dd4381b7bdcbd9"
        );
    }
}

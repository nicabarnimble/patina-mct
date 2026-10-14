//! Admin-key grant check.
//!
//! Mother is the only grantor. This module decides whether a presented public
//! half may exercise one route scope. It does not mint a grant, a Child
//! handle, or a session token. Proof of possession is a later check; a grant
//! fact alone is not a proof.

use crate::grant_check::{ExpiryDecision, ExpiryRequirement, action_is_allowed, expiry_decision};
use crate::id::Timestamp;
use serde::{Deserialize, Serialize};

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

/// Ledger fact for one admin key. Mother stores the public half only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminKeyGrantV1 {
    /// Stable grant identifier.
    pub grant_id: String,
    /// Public half. The secret is not a field of this fact.
    pub public_half: String,
    /// Day-to-day or recovery.
    pub class: AdminKeyGrantClassV1,
    /// Route scopes, strictly ordered and non-empty.
    pub scopes: Vec<AdminKeyScopeV1>,
    /// Mandatory for a day-to-day key. Absent on the recovery grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Timestamp>,
    /// Active or revoked.
    pub grant_state: AdminKeyGrantStateV1,
}

/// Why an admin-key grant fact is not admissible.
///
/// `None` means the shape is the one the ledger and the check both accept.
pub fn admin_key_grant_shape_error(grant: &AdminKeyGrantV1) -> Option<&'static str> {
    if !non_empty_token(&grant.grant_id) {
        return Some("admin key grant id is empty");
    }
    if !non_empty_token(&grant.public_half) {
        return Some("admin key grant stores only a non-empty public half");
    }
    if !admin_key_scopes_are_canonical(&grant.scopes) {
        return Some("admin key scopes are empty or not strictly ordered");
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
    /// Presented public half. A secret is not accepted here.
    pub public_half: String,
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
    if !non_empty_token(&request.public_half) {
        return deny(AdminKeyGrantReason::WrongKey, None);
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
    presenter_public_half: &str,
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
    if target.public_half == presenter_public_half {
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

    fn day_to_day(scopes: Vec<AdminKeyScopeV1>) -> AdminKeyGrantV1 {
        AdminKeyGrantV1 {
            grant_id: "day-to-day".into(),
            public_half: "public-half-day".into(),
            class: AdminKeyGrantClassV1::DayToDay,
            scopes,
            expires_at: Some(later()),
            grant_state: AdminKeyGrantStateV1::Active,
        }
    }

    fn recovery() -> AdminKeyGrantV1 {
        AdminKeyGrantV1 {
            grant_id: "recovery".into(),
            public_half: "public-half-recovery".into(),
            class: AdminKeyGrantClassV1::Recovery,
            scopes: recovery_grant_scopes().to_vec(),
            expires_at: None,
            grant_state: AdminKeyGrantStateV1::Active,
        }
    }

    fn request(public_half: &str, action: AdminKeyCheckAction) -> AdminKeyGrantCheckRequest {
        AdminKeyGrantCheckRequest {
            public_half: public_half.into(),
            action,
            now: now(),
            presented_generation: 4,
            current_generation: 4,
        }
    }

    #[test]
    fn day_to_day_key_allows_its_route_scope_and_denies_another() {
        let grant = day_to_day(vec![AdminKeyScopeV1::Status]);
        let allowed = evaluate_admin_key_grant(
            &request(
                "public-half-day",
                AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),
            ),
            std::slice::from_ref(&grant),
        );
        assert!(allowed.is_allowed());
        assert_eq!(allowed.reason, AdminKeyGrantReason::ActiveGrant);
        assert_eq!(allowed.admitted_scope, Some(AdminKeyScopeV1::Status));
        assert_eq!(allowed.grant_id.as_deref(), Some("day-to-day"));

        let denied = evaluate_admin_key_grant(
            &request(
                "public-half-day",
                AdminKeyCheckAction::Route(AdminKeyScopeV1::PeerAdd),
            ),
            &[grant],
        );
        assert!(!denied.is_allowed());
        assert_eq!(denied.reason, AdminKeyGrantReason::WrongScope);
        assert!(denied.admitted_scope.is_none());
    }

    #[test]
    fn wrong_key_missing_scope_revoked_and_stale_generation_deny() {
        let grant = day_to_day(vec![AdminKeyScopeV1::Status]);
        let wrong_key = evaluate_admin_key_grant(
            &request(
                "other-half",
                AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),
            ),
            std::slice::from_ref(&grant),
        );
        assert_eq!(wrong_key.reason, AdminKeyGrantReason::WrongKey);
        assert!(!wrong_key.is_allowed());

        let mut revoked = grant.clone();
        revoked.grant_state = AdminKeyGrantStateV1::Revoked;
        let revoked_result = evaluate_admin_key_grant(
            &request(
                "public-half-day",
                AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),
            ),
            &[revoked],
        );
        assert_eq!(revoked_result.reason, AdminKeyGrantReason::RevokedGrant);

        let mut stale = request(
            "public-half-day",
            AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),
        );
        stale.presented_generation = 3;
        let stale_result = evaluate_admin_key_grant(&stale, &[grant]);
        assert_eq!(stale_result.reason, AdminKeyGrantReason::StaleGeneration);
        assert!(stale_result.grant_id.is_none());
    }

    #[test]
    fn day_to_day_expiry_is_mandatory() {
        let mut permanent = day_to_day(vec![AdminKeyScopeV1::Status]);
        permanent.expires_at = None;
        let missing = evaluate_admin_key_grant(
            &request(
                "public-half-day",
                AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),
            ),
            &[permanent],
        );
        assert_eq!(missing.reason, AdminKeyGrantReason::PolicyDenied);
        assert!(!missing.is_allowed());

        let mut expired = day_to_day(vec![AdminKeyScopeV1::Status]);
        expired.expires_at = Some(now());
        let at_expiry = evaluate_admin_key_grant(
            &request(
                "public-half-day",
                AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),
            ),
            &[expired],
        );
        assert_eq!(at_expiry.reason, AdminKeyGrantReason::ExpiredGrant);
    }

    #[test]
    fn recovery_grant_is_gate_plus_key_manage_only() {
        let mut grant = recovery();
        let gate = evaluate_admin_key_grant(
            &request(
                "public-half-recovery",
                AdminKeyCheckAction::Route(AdminKeyScopeV1::StartupGate),
            ),
            std::slice::from_ref(&grant),
        );
        assert!(gate.is_allowed());
        assert_eq!(gate.admitted_scope, Some(AdminKeyScopeV1::StartupGate));

        let manage = evaluate_admin_key_grant(
            &request(
                "public-half-recovery",
                AdminKeyCheckAction::Route(AdminKeyScopeV1::KeyManage),
            ),
            std::slice::from_ref(&grant),
        );
        assert!(manage.is_allowed());

        let status = evaluate_admin_key_grant(
            &request(
                "public-half-recovery",
                AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),
            ),
            std::slice::from_ref(&grant),
        );
        assert_eq!(status.reason, AdminKeyGrantReason::WrongScope);

        let mut far_future = request(
            "public-half-recovery",
            AdminKeyCheckAction::Route(AdminKeyScopeV1::StartupGate),
        );
        far_future.now = Timestamp::new("2099-01-01T00:00:00Z").unwrap();
        assert!(evaluate_admin_key_grant(&far_future, std::slice::from_ref(&grant)).is_allowed());

        grant.expires_at = Some(later());
        let dated = evaluate_admin_key_grant(
            &request(
                "public-half-recovery",
                AdminKeyCheckAction::Route(AdminKeyScopeV1::StartupGate),
            ),
            &[grant],
        );
        assert_eq!(dated.reason, AdminKeyGrantReason::PolicyDenied);
        assert!(!dated.is_allowed());
    }

    #[test]
    fn day_to_day_key_cannot_pass_the_startup_gate() {
        let grant = day_to_day(vec![AdminKeyScopeV1::KeyManage, AdminKeyScopeV1::Status]);
        let gate = evaluate_admin_key_grant(
            &request(
                "public-half-day",
                AdminKeyCheckAction::Route(AdminKeyScopeV1::StartupGate),
            ),
            std::slice::from_ref(&grant),
        );
        assert_eq!(gate.reason, AdminKeyGrantReason::WrongScope);

        let mut carries_gate = grant;
        carries_gate.scopes = vec![AdminKeyScopeV1::StartupGate, AdminKeyScopeV1::Status];
        let malformed = evaluate_admin_key_grant(
            &request(
                "public-half-day",
                AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),
            ),
            &[carries_gate],
        );
        assert_eq!(malformed.reason, AdminKeyGrantReason::PolicyDenied);
    }

    #[test]
    fn day_to_day_key_cannot_revoke_the_recovery_grant() {
        let day = day_to_day(vec![AdminKeyScopeV1::KeyManage, AdminKeyScopeV1::Status]);
        let recovery = recovery();
        let revoked = evaluate_admin_key_grant(
            &request(
                "public-half-day",
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "recovery".into(),
                },
            ),
            &[day.clone(), recovery.clone()],
        );
        assert!(!revoked.is_allowed());
        assert_eq!(revoked.reason, AdminKeyGrantReason::RecoveryGrantLocked);
        assert_eq!(revoked.grant_id.as_deref(), Some("recovery"));
        assert!(revoked.admitted_scope.is_none());

        let outcome = crate::evaluate_grant_subject(crate::GrantCheckSubject::AdminKey {
            request: &request(
                "public-half-day",
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "recovery".into(),
                },
            ),
            grants: &[day, recovery],
        });
        let crate::GrantCheckOutcome::AdminKey(through_subject) = outcome else {
            panic!("an admin-key subject is not a Child toy call");
        };
        assert_eq!(through_subject, revoked);
    }

    #[test]
    fn recovery_key_cannot_revoke_itself_and_can_revoke_a_day_to_day_key() {
        let day = day_to_day(vec![AdminKeyScopeV1::Status]);
        let recovery = recovery();
        let self_revoke = evaluate_admin_key_grant(
            &request(
                "public-half-recovery",
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "recovery".into(),
                },
            ),
            &[day.clone(), recovery.clone()],
        );
        assert_eq!(self_revoke.reason, AdminKeyGrantReason::RecoveryGrantLocked);

        let revoke_day = evaluate_admin_key_grant(
            &request(
                "public-half-recovery",
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "day-to-day".into(),
                },
            ),
            &[day, recovery],
        );
        assert!(revoke_day.is_allowed());
        assert_eq!(revoke_day.admitted_scope, Some(AdminKeyScopeV1::KeyManage));
        assert_eq!(revoke_day.grant_id.as_deref(), Some("recovery"));
    }

    #[test]
    fn day_to_day_key_cannot_revoke_itself_and_can_revoke_another_day_to_day_key() {
        let mut other = day_to_day(vec![AdminKeyScopeV1::Status]);
        other.grant_id = "other-day".into();
        other.public_half = "public-half-other".into();
        let holder = day_to_day(vec![AdminKeyScopeV1::KeyManage]);
        let self_revoke = evaluate_admin_key_grant(
            &request(
                "public-half-day",
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "day-to-day".into(),
                },
            ),
            &[holder.clone(), other.clone()],
        );
        assert_eq!(self_revoke.reason, AdminKeyGrantReason::PolicyDenied);

        let revoke_other = evaluate_admin_key_grant(
            &request(
                "public-half-day",
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "other-day".into(),
                },
            ),
            &[holder, other],
        );
        assert!(revoke_other.is_allowed());
    }

    #[test]
    fn key_manage_is_not_implied_by_another_scope() {
        let grant = day_to_day(vec![AdminKeyScopeV1::PeerAdd, AdminKeyScopeV1::Status]);
        let revoke = evaluate_admin_key_grant(
            &request(
                "public-half-day",
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "missing".into(),
                },
            ),
            &[grant],
        );
        assert_eq!(revoke.reason, AdminKeyGrantReason::WrongScope);
        assert!(!revoke.is_allowed());
    }

    #[test]
    fn empty_grant_set_denies() {
        let denied = evaluate_admin_key_grant(
            &request(
                "public-half-day",
                AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),
            ),
            &[],
        );
        assert_eq!(denied.reason, AdminKeyGrantReason::WrongKey);
        assert!(!denied.is_allowed());
    }
}

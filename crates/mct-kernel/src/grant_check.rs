//! Shared grant-check rules for a Child toy call and an admin-key route.
//!
//! The action list and the expiry window are one decision. Toy calls keep
//! optional expiry. Day-to-day admin keys require expiry. The recovery grant
//! is not a day-to-day key and does not carry one.

use crate::MctCall;
use crate::admin_key::{
    AdminKeyGrantCheckRequest, AdminKeyGrantCheckResult, AdminKeyGrantV1, evaluate_admin_key_grant,
};
use crate::id::Timestamp;
use crate::toy::{
    CanonicalToyContract, ToyGrant, ToyGrantEvaluationRequest, ToyGrantEvaluationResult,
    evaluate_toy_grant_for_call,
};

/// How a grant's `expires_at` participates in the check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExpiryRequirement {
    /// Absent means the window is open. Toy grants use this.
    Optional,
    /// Absent denies. Day-to-day admin keys use this.
    Mandatory,
    /// Present denies. The recovery grant uses this.
    Forbidden,
}

/// Result of applying [`ExpiryRequirement`] at `now`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExpiryDecision {
    /// The window permits use at `now`.
    Open,
    /// `starts_at` is still in the future.
    NotYet,
    /// `expires_at` is at or before `now`.
    Expired,
    /// A mandatory expiry was omitted.
    Missing,
    /// An expiry was set on a grant that must not carry one.
    PresentButForbidden,
}

/// Subject of the generalized grant check.
///
/// A Child authorizes one Toy call. An admin key authorizes one route scope.
/// The admin arm does not invent a Child or a call.
pub enum GrantCheckSubject<'a> {
    /// Existing toy-grant check. Behavior is unchanged.
    Child {
        /// Call whose toy request is evaluated.
        call: &'a MctCall,
        /// Child, action, and time facts.
        request: &'a ToyGrantEvaluationRequest,
        /// Canonical toy catalog.
        catalog: &'a [CanonicalToyContract],
        /// Current toy grants.
        grants: &'a [ToyGrant],
    },
    /// Admin-key check. Route scopes are the action list.
    AdminKey {
        /// Presented public half, route scope or revoke, and generation.
        request: &'a AdminKeyGrantCheckRequest,
        /// Current admin-key grants. Public halves only.
        grants: &'a [AdminKeyGrantV1],
    },
}

/// Decision from [`evaluate_grant_subject`].
#[derive(Debug, PartialEq, Eq)]
pub enum GrantCheckOutcome {
    /// Toy-call decision, including a token only when the Child grant allows.
    Child(Box<ToyGrantEvaluationResult>),
    /// Admin-key decision. An allow is not a session token and not a Child handle.
    AdminKey(AdminKeyGrantCheckResult),
}

/// Evaluates one grant subject.
///
/// Child and admin-key arms both decide expiry and action membership through
/// the helpers in this module. Denied admin checks carry no token.
pub fn evaluate_grant_subject(subject: GrantCheckSubject<'_>) -> GrantCheckOutcome {
    match subject {
        GrantCheckSubject::Child {
            call,
            request,
            catalog,
            grants,
        } => GrantCheckOutcome::Child(Box::new(evaluate_toy_grant_for_call(
            call, request, catalog, grants,
        ))),
        GrantCheckSubject::AdminKey { request, grants } => {
            GrantCheckOutcome::AdminKey(evaluate_admin_key_grant(request, grants))
        }
    }
}

/// True when `action` is one of `allowed_actions`.
///
/// Toy grants pass their `allowed_actions`. Admin route scopes pass the same
/// list after each scope is named with [`crate::AdminKeyScopeV1::action_name`].
pub(crate) fn action_is_allowed<'a>(
    allowed_actions: impl IntoIterator<Item = &'a str>,
    action: &str,
) -> bool {
    allowed_actions.into_iter().any(|allowed| allowed == action)
}

/// Decides the time window. `now >= expires_at` is expired, matching peer
/// bindings (`expires_at <= now`) and toy grants.
pub(crate) fn expiry_decision(
    now: &Timestamp,
    starts_at: Option<&Timestamp>,
    expires_at: Option<&Timestamp>,
    requirement: ExpiryRequirement,
) -> ExpiryDecision {
    if starts_at.is_some_and(|starts_at| now < starts_at) {
        return ExpiryDecision::NotYet;
    }
    match requirement {
        ExpiryRequirement::Forbidden => {
            if expires_at.is_some() {
                ExpiryDecision::PresentButForbidden
            } else {
                ExpiryDecision::Open
            }
        }
        ExpiryRequirement::Mandatory => match expires_at {
            None => ExpiryDecision::Missing,
            Some(expires_at) if now >= expires_at => ExpiryDecision::Expired,
            Some(_) => ExpiryDecision::Open,
        },
        ExpiryRequirement::Optional => match expires_at {
            Some(expires_at) if now >= expires_at => ExpiryDecision::Expired,
            _ => ExpiryDecision::Open,
        },
    }
}

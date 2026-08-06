use crate::{
    authority::{LocalExecutionAuthoritySnapshot, LocalExecutionAuthorityTokenV1},
    call::*,
    id::*,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// WIT identity of a canonical toy capability.
///
/// Function and resource narrow the interface when a toy exposes multiple
/// authority-bearing operations.
pub struct ToyContractIdentity {
    /// WIT package namespace containing the toy contract.
    pub namespace: String,
    /// WIT interface name for the toy.
    pub interface_name: String,
    /// Contract version used by the canonical catalog.
    pub version: String,
    /// Optional function name when authority is operation-specific.
    pub function_name: Option<String>,
    /// Optional resource name when authority is resource-specific.
    pub resource_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Catalog entry defining whether a toy can carry authority.
///
/// Grant evaluation denies non-catalog toys and catalog entries that are not
/// authority-bearing.
pub struct CanonicalToyContract {
    /// Stable toy identifier used by grants and requests.
    pub toy_id: ToyId,
    /// WIT contract identity for this catalog entry.
    pub contract: ToyContractIdentity,
    /// Whether this toy may be authorized through ToyGrant evaluation.
    pub authority_bearing: bool,
    /// Catalog revision that admitted this entry.
    pub catalog_revision: u64,
    /// Observation that admitted the toy to the canonical catalog.
    pub admitted_by_observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Child identity a toy grant applies to.
///
/// Grant subject matching is exact for child name/artifact/version; optional
/// assignment and caller fields narrow the grant when present.
pub struct ToyGrantSubject {
    /// Child name requesting toy access.
    pub child_name: String,
    /// Artifact identity of the requesting child.
    pub artifact_id: String,
    /// Artifact version of the requesting child.
    pub artifact_version: String,
    /// Optional assignment that must match the request when grant-scoped.
    pub assignment_id: Option<ChildAssignmentId>,
    /// Optional caller node that must match when grant-scoped.
    pub caller_node_id: Option<MctNodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Vision, node, data, resource, and action scope of a toy grant.
///
/// Optional scope fields are wildcards when absent and exact requirements when
/// present; `allowed_actions` must contain the requested action.
pub struct ToyGrantScope {
    /// Vision in which the grant is valid.
    pub vision_id: VisionId,
    /// Optional node restriction for the effect.
    pub node_id: Option<MctNodeId>,
    /// Optional project restriction matched against the call caller.
    pub project_id: Option<ProjectId>,
    /// Optional data classification restriction matched against payload metadata.
    pub data_classification: Option<String>,
    /// Optional resource identifier restriction matched against the request.
    pub resource_id: Option<String>,
    /// Actions permitted by this grant.
    pub allowed_actions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Time and usage constraints attached to a toy grant.
///
/// Evaluation checks `starts_at <= now < expires_at` when bounds are present;
/// max-use and duration fields are authority facts for adapters to enforce.
pub struct ToyGrantConstraints {
    /// Earliest time the grant may be used.
    pub starts_at: Option<Timestamp>,
    /// Exclusive expiry time for grant evaluation.
    pub expires_at: Option<Timestamp>,
    /// Optional maximum uses tracked by adapters or storage.
    pub max_uses: Option<u64>,
    /// Optional maximum duration for a toy effect.
    pub max_duration_ms: Option<u64>,
    /// Whether the effect must remain local to the node.
    pub locality_required: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Lifecycle state of a toy grant authority record.
pub enum ToyGrantState {
    /// Grant was requested but does not authorize effects.
    Requested,
    /// Grant may authorize effects if all other facts match.
    Active,
    /// Grant is expired by lifecycle state.
    Expired,
    /// Grant was explicitly revoked.
    Revoked,
    /// Grant was replaced by a newer authority record.
    Superseded,
    /// Grant request was denied.
    Denied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Authority record that may permit a child to use one canonical toy.
///
/// Evaluation requires an active grant matching subject, scope, action, policy
/// revision, grants revision, and time window before minting a toy-call token.
pub struct ToyGrant {
    /// Stable grant identifier.
    pub grant_id: ToyGrantId,
    /// Canonical toy this grant covers.
    pub toy_id: ToyId,
    /// Child identity eligible to use the grant.
    pub subject: ToyGrantSubject,
    /// Vision/action/resource scope of the grant.
    pub scope: ToyGrantScope,
    /// Time and usage constraints for the grant.
    pub constraints: ToyGrantConstraints,
    /// Lifecycle state used during evaluation.
    pub grant_state: ToyGrantState,
    /// Authority issuer for audit.
    pub issuer_id: String,
    /// Policy revision under which the grant was issued.
    pub policy_revision: u64,
    /// Grants revision under which the grant was issued.
    pub grants_revision: u64,
    /// Observation that created or updated this authority record.
    pub authority_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Verdict of evaluating one toy request against catalog and grants.
pub enum ToyGrantVerdict {
    /// A grant authorized the requested toy action.
    Allowed,
    /// No grant authorized the requested toy action.
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Audit reason for a toy grant evaluation.
pub enum ToyGrantReasonCode {
    /// Active grant matched every authority fact.
    ActiveGrant,
    /// No grant matched the toy and subject.
    MissingGrant,
    /// Matching grant was expired by state or time window.
    ExpiredGrant,
    /// Matching grant was revoked, superseded, or denied.
    RevokedGrant,
    /// Matching grant did not cover the requested scope or action.
    WrongScope,
    /// Requested toy was absent from the canonical catalog.
    UnknownToy,
    /// Policy or lifecycle state denied use of the toy.
    PolicyDenied,
    /// Grant revisions did not match the call authority snapshot.
    StaleSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Decision produced by toy grant evaluation.
///
/// Allowed evaluations cite the matching grant and can mint an
/// [`AuthorizedToyCall`]; denied evaluations carry no executable token.
pub struct ToyGrantEvaluation {
    /// Evaluation identifier for this toy decision.
    pub evaluation_id: ToyGrantEvaluationId,
    /// Call whose toy request was evaluated.
    pub call_id: CallId,
    /// Decision identifier for authority/audit linkage.
    pub decision_id: DecisionId,
    /// Matching grant, present when a specific grant was considered.
    pub grant_id: Option<ToyGrantId>,
    /// Requested canonical toy.
    pub toy_id: ToyId,
    /// Child name from the evaluated subject.
    pub subject_child_name: String,
    /// Allowed or denied verdict.
    pub verdict: ToyGrantVerdict,
    /// Typed reason for the verdict.
    pub reason_code: ToyGrantReasonCode,
    /// Policy revision used by the evaluation.
    pub policy_revision: u64,
    /// Grants revision used by the evaluation.
    pub grants_revision: u64,
    /// Observation recording this evaluation.
    pub observation_id: ObservationId,
}

#[derive(Debug, PartialEq, Eq)]
/// Session-scoped capability token allowing a child to use one toy during one
/// authorized component invocation.
///
/// This token is minted only by [`evaluate_toy_grant_for_call`]. It is
/// intentionally borrowed for each toy host call made during the invocation:
/// `next_toy_call_index`/`MctToyCallIds` provide per-use receipts, while this
/// non-`Clone` token remains the session authority and cannot be copied into a
/// later session.
pub struct AuthorizedToyCall {
    /// Unique token identifier for the authorized toy effect.
    authorized_toy_call_id: AuthorizedToyCallId,
    /// Call during which the toy may be used.
    call_id: CallId,
    /// Evaluation that minted this token.
    evaluation_id: ToyGrantEvaluationId,
    /// Grant that authorized the toy effect.
    grant_id: ToyGrantId,
    /// Toy the token authorizes.
    toy_id: ToyId,
    /// Child instance allowed to exercise the toy.
    child_instance_id: ChildInstanceId,
    /// Exact resource authorized by the evaluated grant/request, when scoped.
    resource_id: Option<String>,
    /// Authority decision tied to this token.
    authority_decision_id: DecisionId,
    /// Token expiry, using grant expiry or call deadline when the grant has none.
    expires_at: Timestamp,
    /// Policy revision under which this capability was minted.
    policy_revision: u64,
    /// Grants revision under which this capability was minted.
    grants_revision: u64,
    /// Complete executing-Mother authority for snapshot-evaluated Toy effects.
    local_execution_authority: Option<LocalExecutionAuthorityTokenV1>,
    /// Exact canonical Toy catalog record used to mint this token.
    canonical_toy: CanonicalToyContract,
    /// Exact canonical grant record used to mint this token.
    canonical_grant: ToyGrant,
    /// Exact subject admitted by evaluation.
    subject: ToyGrantSubject,
    /// Exact action admitted by evaluation.
    action: String,
    /// Exact node admitted by evaluation.
    node_id: MctNodeId,
    /// Exact Vision admitted by evaluation.
    vision_id: VisionId,
    /// Exact project admitted by evaluation.
    project_id: Option<ProjectId>,
    /// Exact data classification admitted by evaluation.
    data_classification: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Typed reason current Toy effect authority was denied.
pub enum ToyEffectAdmissionDenyV1 {
    /// Token was not minted from local snapshot authority.
    LocalAuthorityUnavailable,
    /// Supplied call differs from the authorized call.
    CallIdMismatch,
    /// Effective deadline differs from the minted deadline.
    EffectiveDeadlineMismatch,
    /// Current local policy identity differs from minted authority.
    PolicyAuthorityMismatch,
    /// Current namespaced grants identity differs from minted authority.
    GrantsAuthorityMismatch,
    /// Token or effective deadline has expired on the Mother clock.
    TokenExpired,
    /// Canonical current Toy catalog does not authorize this Toy.
    UnknownToy,
    /// Exact grant named by the token is absent.
    MissingExactGrant,
    /// Exact grant is not active.
    InactiveGrant,
    /// Exact current subject or effect scope differs from the token.
    ExactGrantMismatch,
    /// Exact grant has not reached its start time.
    GrantNotStarted,
    /// Exact grant reached its exclusive expiry.
    GrantExpired,
    /// A usage limit exists but no current live consumption fact is available.
    ConsumptionStateUnavailable,
}

#[derive(Debug)]
/// Proof that a Toy token is bound to the supplied call and its authority
/// revisions at an effect-admission boundary.
///
/// The constructor is private to [`AuthorizedToyCall::admit_effect_for_call_at`]
/// so daemon Toy paths cannot obtain this proof without performing every
/// binding and expiry check.
pub struct AdmittedToyEffect<'a> {
    authorized: &'a AuthorizedToyCall,
}

impl AdmittedToyEffect<'_> {
    /// Returns the capability whose exact-call binding was admitted.
    pub fn authorized(&self) -> &AuthorizedToyCall {
        self.authorized
    }
}

impl AuthorizedToyCall {
    /// Admits this token against one fresh proof-gated executing-Mother snapshot.
    pub fn admit_effect_with_snapshot<'a>(
        &'a self,
        call: &MctCall,
        snapshot: &LocalExecutionAuthoritySnapshot,
    ) -> Result<AdmittedToyEffect<'a>, ToyEffectAdmissionDenyV1> {
        let authority = self
            .local_execution_authority
            .as_ref()
            .ok_or(ToyEffectAdmissionDenyV1::LocalAuthorityUnavailable)?;
        if self.call_id != call.call_id {
            return Err(ToyEffectAdmissionDenyV1::CallIdMismatch);
        }
        if authority.effective_deadline() != &call.deadline {
            return Err(ToyEffectAdmissionDenyV1::EffectiveDeadlineMismatch);
        }
        if authority.policy_revision() != snapshot.policy_revision()
            || authority.vision_policy_revision() != snapshot.vision_policy_revision()
        {
            return Err(ToyEffectAdmissionDenyV1::PolicyAuthorityMismatch);
        }
        if authority.grants_authority() != snapshot.canonical_grants().grants_authority()
            || authority.grants_authority().mother_node_id() != snapshot.executing_mother_node_id()
        {
            return Err(ToyEffectAdmissionDenyV1::GrantsAuthorityMismatch);
        }
        let now = snapshot.mother_clock().evaluated_at();
        if now >= &self.expires_at || now >= authority.effective_deadline() {
            return Err(ToyEffectAdmissionDenyV1::TokenExpired);
        }
        let current_toy = snapshot
            .canonical_grants()
            .toy_catalog()
            .iter()
            .find(|toy| toy.toy_id == self.toy_id && toy.authority_bearing)
            .ok_or(ToyEffectAdmissionDenyV1::UnknownToy)?;
        if current_toy != &self.canonical_toy {
            return Err(ToyEffectAdmissionDenyV1::ExactGrantMismatch);
        }
        let grant = snapshot
            .canonical_grants()
            .toy_grants()
            .iter()
            .find(|grant| grant.grant_id == self.grant_id)
            .ok_or(ToyEffectAdmissionDenyV1::MissingExactGrant)?;
        if grant.grant_state != ToyGrantState::Active {
            return Err(ToyEffectAdmissionDenyV1::InactiveGrant);
        }
        if grant != &self.canonical_grant
            || grant.toy_id != self.toy_id
            || grant.policy_revision != snapshot.policy_revision()
            || !subject_matches(&grant.subject, &self.subject)
            || grant.scope.vision_id != self.vision_id
            || !option_matches(&grant.scope.node_id, &Some(self.node_id.clone()))
            || !option_matches(&grant.scope.project_id, &self.project_id)
            || !option_matches(
                &grant.scope.data_classification,
                &Some(self.data_classification.clone()),
            )
            || !option_matches(&grant.scope.resource_id, &self.resource_id)
            || !grant
                .scope
                .allowed_actions
                .iter()
                .any(|action| action == &self.action)
        {
            return Err(ToyEffectAdmissionDenyV1::ExactGrantMismatch);
        }
        if grant
            .constraints
            .starts_at
            .as_ref()
            .is_some_and(|starts_at| now < starts_at)
        {
            return Err(ToyEffectAdmissionDenyV1::GrantNotStarted);
        }
        if grant
            .constraints
            .expires_at
            .as_ref()
            .is_some_and(|expires_at| now >= expires_at)
        {
            return Err(ToyEffectAdmissionDenyV1::GrantExpired);
        }
        if grant.constraints.max_uses.is_some() {
            return Err(ToyEffectAdmissionDenyV1::ConsumptionStateUnavailable);
        }
        Ok(AdmittedToyEffect { authorized: self })
    }

    /// Legacy exact-call/time guard retained for compatibility surfaces.
    /// Resident Toy effects must use [`Self::admit_effect_with_snapshot`].
    /// Admits this token for an effect only when its exact call and authority
    /// revisions match the supplied call and the executing Mother's current
    /// time is strictly before token expiry.
    pub fn admit_effect_for_call_at<'a>(
        &'a self,
        call: &MctCall,
        executing_mother_now: &Timestamp,
    ) -> Option<AdmittedToyEffect<'a>> {
        (self.call_id == call.call_id && executing_mother_now < &self.expires_at)
            .then_some(AdmittedToyEffect { authorized: self })
    }

    /// Unique token identifier for the authorized toy effect.
    pub fn authorized_toy_call_id(&self) -> &AuthorizedToyCallId {
        &self.authorized_toy_call_id
    }

    /// Call during which the toy may be used.
    pub fn call_id(&self) -> &CallId {
        &self.call_id
    }

    /// Evaluation that minted this token.
    pub fn evaluation_id(&self) -> &ToyGrantEvaluationId {
        &self.evaluation_id
    }

    /// Grant that authorized the toy effect.
    pub fn grant_id(&self) -> &ToyGrantId {
        &self.grant_id
    }

    /// Toy the token authorizes.
    pub fn toy_id(&self) -> &ToyId {
        &self.toy_id
    }

    /// Child instance allowed to exercise the toy.
    pub fn child_instance_id(&self) -> &ChildInstanceId {
        &self.child_instance_id
    }

    /// Exact resource authorized for this token, when one was requested.
    pub fn resource_id(&self) -> Option<&str> {
        self.resource_id.as_deref()
    }

    /// Authority decision tied to this token.
    pub fn authority_decision_id(&self) -> &DecisionId {
        &self.authority_decision_id
    }

    /// Token expiry, using grant expiry or call deadline when the grant has none.
    pub fn expires_at(&self) -> &Timestamp {
        &self.expires_at
    }

    /// Returns the policy revision under which this capability was minted.
    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }

    /// Returns the grants revision under which this capability was minted.
    pub fn grants_revision(&self) -> u64 {
        self.grants_revision
    }

    /// Returns complete local authority when snapshot minting produced this token.
    pub fn local_execution_authority(&self) -> Option<&LocalExecutionAuthorityTokenV1> {
        self.local_execution_authority.as_ref()
    }

    /// Returns the exact action admitted by evaluation.
    pub fn action(&self) -> &str {
        &self.action
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Identifiers supplied for toy grant evaluation and token minting.
pub struct ToyGrantEvaluationIds {
    /// Identifier for the produced evaluation.
    pub evaluation_id: ToyGrantEvaluationId,
    /// Decision identifier for authority linkage.
    pub decision_id: DecisionId,
    /// Observation identifier for evaluation evidence.
    pub observation_id: ObservationId,
    /// Token identifier used only when authorization succeeds.
    pub authorized_toy_call_id: AuthorizedToyCallId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Facts supplied by an adapter when a child requests a toy effect.
pub struct ToyGrantEvaluationRequest {
    /// Toy the child wants to use.
    pub toy_id: ToyId,
    /// Child identity requesting the effect.
    pub subject: ToyGrantSubject,
    /// Live child instance requesting the effect.
    pub child_instance_id: ChildInstanceId,
    /// Requested action; must be present in the grant scope.
    pub action: String,
    /// Optional resource requested by the child.
    pub resource_id: Option<String>,
    /// Node where the effect would occur.
    pub node_id: MctNodeId,
    /// Adapter-supplied current time for grant window checks.
    pub now: Timestamp,
    /// Identifiers to stamp on the evaluation and token.
    pub ids: ToyGrantEvaluationIds,
}

#[derive(Debug, PartialEq, Eq)]
/// Result of toy grant evaluation, including token only on allow.
pub struct ToyGrantEvaluationResult {
    /// Typed evaluation recording verdict and reason.
    pub evaluation: ToyGrantEvaluation,
    /// Executable toy-call token, present only for allowed evaluations.
    pub authorized: Option<AuthorizedToyCall>,
}

impl ToyGrantEvaluationResult {
    /// Returns true only when evaluation allowed and minted a toy-call token.
    pub fn is_allowed(&self) -> bool {
        self.evaluation.verdict == ToyGrantVerdict::Allowed && self.authorized.is_some()
    }
}

/// Decides whether a child may exercise a canonical toy for one call.
///
/// Authority facts are the call snapshot, toy request, canonical catalog, and
/// current grants. It allows only cataloged authority-bearing toys with an
/// active grant whose subject, scope, action, policy revision, and time window match.
/// Every missing, stale, revoked, expired, or mismatched fact returns a denied
/// evaluation and no [`AuthorizedToyCall`].
pub fn evaluate_toy_grant_for_call(
    call: &MctCall,
    request: &ToyGrantEvaluationRequest,
    catalog: &[CanonicalToyContract],
    grants: &[ToyGrant],
) -> ToyGrantEvaluationResult {
    let Some(toy) = catalog.iter().find(|toy| toy.toy_id == request.toy_id) else {
        return denied(
            call,
            request,
            None,
            ToyGrantReasonCode::UnknownToy,
            call.authority_context.policy_revision,
            call.authority_context
                .expected_receiver_grants_authority
                .generation,
        );
    };

    if !toy.authority_bearing {
        return denied(
            call,
            request,
            None,
            ToyGrantReasonCode::PolicyDenied,
            call.authority_context.policy_revision,
            call.authority_context
                .expected_receiver_grants_authority
                .generation,
        );
    }

    let mut matching_wrong_state = None;
    for grant in grants.iter().filter(|grant| grant.toy_id == request.toy_id) {
        if !subject_matches(&grant.subject, &request.subject) {
            continue;
        }

        if grant.policy_revision != call.authority_context.policy_revision {
            return denied(
                call,
                request,
                Some(grant),
                ToyGrantReasonCode::StaleSnapshot,
                grant.policy_revision,
                grant.grants_revision,
            );
        }

        match grant.grant_state {
            ToyGrantState::Active => {}
            ToyGrantState::Expired => {
                matching_wrong_state = Some((grant, ToyGrantReasonCode::ExpiredGrant));
                continue;
            }
            ToyGrantState::Revoked | ToyGrantState::Superseded | ToyGrantState::Denied => {
                matching_wrong_state = Some((grant, ToyGrantReasonCode::RevokedGrant));
                continue;
            }
            ToyGrantState::Requested => {
                matching_wrong_state = Some((grant, ToyGrantReasonCode::PolicyDenied));
                continue;
            }
        }

        if grant
            .constraints
            .starts_at
            .as_ref()
            .is_some_and(|starts_at| request.now < *starts_at)
        {
            return denied(
                call,
                request,
                Some(grant),
                ToyGrantReasonCode::PolicyDenied,
                grant.policy_revision,
                grant.grants_revision,
            );
        }

        if grant
            .constraints
            .expires_at
            .as_ref()
            .is_some_and(|expires_at| request.now >= *expires_at)
        {
            return denied(
                call,
                request,
                Some(grant),
                ToyGrantReasonCode::ExpiredGrant,
                grant.policy_revision,
                grant.grants_revision,
            );
        }

        if !scope_matches(&grant.scope, call, request) {
            return denied(
                call,
                request,
                Some(grant),
                ToyGrantReasonCode::WrongScope,
                grant.policy_revision,
                grant.grants_revision,
            );
        }

        let evaluation = ToyGrantEvaluation {
            evaluation_id: request.ids.evaluation_id.clone(),
            call_id: call.call_id.clone(),
            decision_id: request.ids.decision_id.clone(),
            grant_id: Some(grant.grant_id.clone()),
            toy_id: request.toy_id.clone(),
            subject_child_name: request.subject.child_name.clone(),
            verdict: ToyGrantVerdict::Allowed,
            reason_code: ToyGrantReasonCode::ActiveGrant,
            policy_revision: grant.policy_revision,
            grants_revision: grant.grants_revision,
            observation_id: request.ids.observation_id.clone(),
        };
        let Some(expires_at) = toy_token_expiry(call, request, grant) else {
            return denied(
                call,
                request,
                Some(grant),
                ToyGrantReasonCode::PolicyDenied,
                grant.policy_revision,
                grant.grants_revision,
            );
        };
        let authorized = AuthorizedToyCall {
            authorized_toy_call_id: request.ids.authorized_toy_call_id.clone(),
            call_id: call.call_id.clone(),
            evaluation_id: evaluation.evaluation_id.clone(),
            grant_id: grant.grant_id.clone(),
            toy_id: request.toy_id.clone(),
            child_instance_id: request.child_instance_id.clone(),
            resource_id: request.resource_id.clone(),
            authority_decision_id: evaluation.decision_id.clone(),
            expires_at,
            policy_revision: grant.policy_revision,
            grants_revision: grant.grants_revision,
            local_execution_authority: None,
            canonical_toy: toy.clone(),
            canonical_grant: grant.clone(),
            subject: request.subject.clone(),
            action: request.action.clone(),
            node_id: request.node_id.clone(),
            vision_id: call.caller.vision_id.clone(),
            project_id: call.caller.project_id.clone(),
            data_classification: call.payload_metadata.data_classification.clone(),
        };

        return ToyGrantEvaluationResult {
            evaluation,
            authorized: Some(authorized),
        };
    }

    if let Some((grant, reason)) = matching_wrong_state {
        return denied(
            call,
            request,
            Some(grant),
            reason,
            grant.policy_revision,
            grant.grants_revision,
        );
    }

    denied(
        call,
        request,
        None,
        ToyGrantReasonCode::MissingGrant,
        call.authority_context.policy_revision,
        call.authority_context
            .expected_receiver_grants_authority
            .generation,
    )
}

/// Mints exact Toy effect authority from one proof-gated local snapshot.
pub fn evaluate_toy_grant_for_snapshot(
    call: &MctCall,
    request: &ToyGrantEvaluationRequest,
    snapshot: &LocalExecutionAuthoritySnapshot,
) -> ToyGrantEvaluationResult {
    let policy_revision = snapshot.policy_revision();
    let grants_generation = snapshot.canonical_grants().grants_authority().generation();
    let evaluation = evaluate_toy_grant_for_route_snapshot(
        call,
        request,
        snapshot.canonical_grants().toy_catalog(),
        snapshot.canonical_grants().toy_grants(),
        policy_revision,
        grants_generation,
    );
    if evaluation.verdict != ToyGrantVerdict::Allowed {
        return ToyGrantEvaluationResult {
            evaluation,
            authorized: None,
        };
    }
    let Some(grant_id) = evaluation.grant_id.as_ref() else {
        return ToyGrantEvaluationResult {
            evaluation,
            authorized: None,
        };
    };
    let Some(grant) = snapshot
        .canonical_grants()
        .toy_grants()
        .iter()
        .find(|grant| &grant.grant_id == grant_id)
    else {
        return ToyGrantEvaluationResult {
            evaluation,
            authorized: None,
        };
    };
    let Some(toy) = snapshot
        .canonical_grants()
        .toy_catalog()
        .iter()
        .find(|toy| toy.toy_id == request.toy_id)
    else {
        return ToyGrantEvaluationResult {
            evaluation,
            authorized: None,
        };
    };
    let Some(expires_at) = toy_token_expiry(call, request, grant) else {
        return ToyGrantEvaluationResult {
            evaluation: route_denied(
                call,
                request,
                Some(grant),
                ToyGrantReasonCode::PolicyDenied,
                policy_revision,
                grants_generation,
            ),
            authorized: None,
        };
    };
    let authorized = AuthorizedToyCall {
        authorized_toy_call_id: request.ids.authorized_toy_call_id.clone(),
        call_id: call.call_id.clone(),
        evaluation_id: evaluation.evaluation_id.clone(),
        grant_id: grant.grant_id.clone(),
        toy_id: request.toy_id.clone(),
        child_instance_id: request.child_instance_id.clone(),
        resource_id: request.resource_id.clone(),
        authority_decision_id: evaluation.decision_id.clone(),
        expires_at,
        policy_revision,
        grants_revision: grants_generation,
        local_execution_authority: Some(snapshot.execution_authority(call.deadline.clone())),
        canonical_toy: toy.clone(),
        canonical_grant: grant.clone(),
        subject: request.subject.clone(),
        action: request.action.clone(),
        node_id: request.node_id.clone(),
        vision_id: call.caller.vision_id.clone(),
        project_id: call.caller.project_id.clone(),
        data_classification: call.payload_metadata.data_classification.clone(),
    };
    ToyGrantEvaluationResult {
        evaluation,
        authorized: Some(authorized),
    }
}

/// Evaluates a required Toy for routing from one proof-gated local snapshot.
///
/// The canonical projection cursor proves grants generation; replayed legacy
/// `ToyGrant.grants_revision` content is therefore not treated as generation.
pub fn evaluate_toy_grant_for_route_snapshot(
    call: &MctCall,
    request: &ToyGrantEvaluationRequest,
    catalog: &[CanonicalToyContract],
    grants: &[ToyGrant],
    local_policy_revision: u64,
    local_grants_generation: u64,
) -> ToyGrantEvaluation {
    let Some(toy) = catalog.iter().find(|toy| toy.toy_id == request.toy_id) else {
        return route_denied(
            call,
            request,
            None,
            ToyGrantReasonCode::UnknownToy,
            local_policy_revision,
            local_grants_generation,
        );
    };
    if !toy.authority_bearing {
        return route_denied(
            call,
            request,
            None,
            ToyGrantReasonCode::PolicyDenied,
            local_policy_revision,
            local_grants_generation,
        );
    }
    let mut matching_wrong_state = None;
    for grant in grants.iter().filter(|grant| grant.toy_id == request.toy_id) {
        if !subject_matches(&grant.subject, &request.subject) {
            continue;
        }
        if grant.policy_revision != local_policy_revision {
            return route_denied(
                call,
                request,
                Some(grant),
                ToyGrantReasonCode::StaleSnapshot,
                local_policy_revision,
                local_grants_generation,
            );
        }
        match grant.grant_state {
            ToyGrantState::Active => {}
            ToyGrantState::Expired => {
                matching_wrong_state = Some((grant, ToyGrantReasonCode::ExpiredGrant));
                continue;
            }
            ToyGrantState::Revoked | ToyGrantState::Superseded | ToyGrantState::Denied => {
                matching_wrong_state = Some((grant, ToyGrantReasonCode::RevokedGrant));
                continue;
            }
            ToyGrantState::Requested => {
                matching_wrong_state = Some((grant, ToyGrantReasonCode::PolicyDenied));
                continue;
            }
        }
        if grant
            .constraints
            .starts_at
            .as_ref()
            .is_some_and(|starts_at| request.now < *starts_at)
        {
            return route_denied(
                call,
                request,
                Some(grant),
                ToyGrantReasonCode::PolicyDenied,
                local_policy_revision,
                local_grants_generation,
            );
        }
        if grant
            .constraints
            .expires_at
            .as_ref()
            .is_some_and(|expires_at| request.now >= *expires_at)
        {
            return route_denied(
                call,
                request,
                Some(grant),
                ToyGrantReasonCode::ExpiredGrant,
                local_policy_revision,
                local_grants_generation,
            );
        }
        if !scope_matches(&grant.scope, call, request) {
            return route_denied(
                call,
                request,
                Some(grant),
                ToyGrantReasonCode::WrongScope,
                local_policy_revision,
                local_grants_generation,
            );
        }
        return ToyGrantEvaluation {
            evaluation_id: request.ids.evaluation_id.clone(),
            call_id: call.call_id.clone(),
            decision_id: request.ids.decision_id.clone(),
            grant_id: Some(grant.grant_id.clone()),
            toy_id: request.toy_id.clone(),
            subject_child_name: request.subject.child_name.clone(),
            verdict: ToyGrantVerdict::Allowed,
            reason_code: ToyGrantReasonCode::ActiveGrant,
            policy_revision: local_policy_revision,
            grants_revision: local_grants_generation,
            observation_id: request.ids.observation_id.clone(),
        };
    }
    let (grant, reason) = matching_wrong_state.map_or(
        (None, ToyGrantReasonCode::MissingGrant),
        |(grant, reason)| (Some(grant), reason),
    );
    route_denied(
        call,
        request,
        grant,
        reason,
        local_policy_revision,
        local_grants_generation,
    )
}

fn route_denied(
    call: &MctCall,
    request: &ToyGrantEvaluationRequest,
    grant: Option<&ToyGrant>,
    reason_code: ToyGrantReasonCode,
    policy_revision: u64,
    grants_revision: u64,
) -> ToyGrantEvaluation {
    ToyGrantEvaluation {
        evaluation_id: request.ids.evaluation_id.clone(),
        call_id: call.call_id.clone(),
        decision_id: request.ids.decision_id.clone(),
        grant_id: grant.map(|grant| grant.grant_id.clone()),
        toy_id: request.toy_id.clone(),
        subject_child_name: request.subject.child_name.clone(),
        verdict: ToyGrantVerdict::Denied,
        reason_code,
        policy_revision,
        grants_revision,
        observation_id: request.ids.observation_id.clone(),
    }
}

fn denied(
    call: &MctCall,
    request: &ToyGrantEvaluationRequest,
    grant: Option<&ToyGrant>,
    reason_code: ToyGrantReasonCode,
    policy_revision: u64,
    grants_revision: u64,
) -> ToyGrantEvaluationResult {
    ToyGrantEvaluationResult {
        evaluation: ToyGrantEvaluation {
            evaluation_id: request.ids.evaluation_id.clone(),
            call_id: call.call_id.clone(),
            decision_id: request.ids.decision_id.clone(),
            grant_id: grant.map(|grant| grant.grant_id.clone()),
            toy_id: request.toy_id.clone(),
            subject_child_name: request.subject.child_name.clone(),
            verdict: ToyGrantVerdict::Denied,
            reason_code,
            policy_revision,
            grants_revision,
            observation_id: request.ids.observation_id.clone(),
        },
        authorized: None,
    }
}

fn subject_matches(grant: &ToyGrantSubject, request: &ToyGrantSubject) -> bool {
    grant.child_name == request.child_name
        && grant.artifact_id == request.artifact_id
        && grant.artifact_version == request.artifact_version
        && option_matches(&grant.assignment_id, &request.assignment_id)
        && option_matches(&grant.caller_node_id, &request.caller_node_id)
}

fn scope_matches(
    scope: &ToyGrantScope,
    call: &MctCall,
    request: &ToyGrantEvaluationRequest,
) -> bool {
    scope.vision_id == call.caller.vision_id
        && option_matches(&scope.node_id, &Some(request.node_id.clone()))
        && option_matches(&scope.project_id, &call.caller.project_id)
        && option_matches(
            &scope.data_classification,
            &Some(call.payload_metadata.data_classification.clone()),
        )
        && option_matches(&scope.resource_id, &request.resource_id)
        && scope
            .allowed_actions
            .iter()
            .any(|action| action == &request.action)
}

fn toy_token_expiry(
    call: &MctCall,
    request: &ToyGrantEvaluationRequest,
    grant: &ToyGrant,
) -> Option<Timestamp> {
    let mut expiry = grant
        .constraints
        .expires_at
        .clone()
        .unwrap_or_else(|| call.deadline.clone())
        .min(call.deadline.clone());
    if let Some(max_duration_ms) = grant.constraints.max_duration_ms {
        expiry = expiry.min(request.now.checked_add_milliseconds(max_duration_ms)?);
    }
    Some(expiry)
}

fn option_matches<T: Eq>(grant_value: &Option<T>, request_value: &Option<T>) -> bool {
    grant_value
        .as_ref()
        .is_none_or(|grant_value| request_value.as_ref() == Some(grant_value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call() -> MctCall {
        MctCall {
            call_id: CallId::new("call-toy-1")
                .expect("string ID literal/generated value must be non-empty"),
            caller: CallerIdentity {
                node_id: MctNodeId::new("caller-node")
                    .expect("string ID literal/generated value must be non-empty"),
                user_id: None,
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                project_id: Some(
                    ProjectId::new("project-a")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
            },
            target: OperationTarget {
                namespace: "patina".into(),
                interface_name: "slate".into(),
                function_name: "list-work".into(),
            },
            payload_metadata: PayloadMetadata {
                data_classification: "project".into(),
                size_bytes: 12,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 3,
                expected_receiver_grants_authority: crate::call::test_grants_authority_identity(7),
                vision_policy_revision: 11,
            },
            deadline: Timestamp::new("2026-05-31T00:10:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-toy-1")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: SpanId::new("span-toy-1")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            origin: CallOrigin::Cli,
        }
    }

    fn toy() -> CanonicalToyContract {
        CanonicalToyContract {
            toy_id: ToyId::new("toy-logging")
                .expect("string ID literal/generated value must be non-empty"),
            contract: ToyContractIdentity {
                namespace: "mct".into(),
                interface_name: "logging".into(),
                version: "0.1.0".into(),
                function_name: Some("write".into()),
                resource_name: None,
            },
            authority_bearing: true,
            catalog_revision: 1,
            admitted_by_observation_id: ObservationId::new("obs-toy-catalog")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn subject() -> ToyGrantSubject {
        ToyGrantSubject {
            child_name: "slate-manager".into(),
            artifact_id: "sha256:artifact".into(),
            artifact_version: "0.2.0".into(),
            assignment_id: Some(
                ChildAssignmentId::new("assignment-a")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            caller_node_id: Some(
                MctNodeId::new("caller-node")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
        }
    }

    fn request() -> ToyGrantEvaluationRequest {
        ToyGrantEvaluationRequest {
            toy_id: ToyId::new("toy-logging")
                .expect("string ID literal/generated value must be non-empty"),
            subject: subject(),
            child_instance_id: ChildInstanceId::new("instance-a")
                .expect("string ID literal/generated value must be non-empty"),
            action: "write".into(),
            resource_id: Some("log:project".into()),
            node_id: MctNodeId::new("node-a")
                .expect("string ID literal/generated value must be non-empty"),
            now: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            ids: ToyGrantEvaluationIds {
                evaluation_id: ToyGrantEvaluationId::new("toy-eval-1")
                    .expect("string ID literal/generated value must be non-empty"),
                decision_id: DecisionId::new("toy-decision-1")
                    .expect("string ID literal/generated value must be non-empty"),
                observation_id: ObservationId::new("obs-toy-eval-1")
                    .expect("string ID literal/generated value must be non-empty"),
                authorized_toy_call_id: AuthorizedToyCallId::new("authorized-toy-call-1")
                    .expect("string ID literal/generated value must be non-empty"),
            },
        }
    }

    fn grant(state: ToyGrantState) -> ToyGrant {
        ToyGrant {
            grant_id: ToyGrantId::new("grant-logging")
                .expect("string ID literal/generated value must be non-empty"),
            toy_id: ToyId::new("toy-logging")
                .expect("string ID literal/generated value must be non-empty"),
            subject: subject(),
            scope: ToyGrantScope {
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                node_id: Some(
                    MctNodeId::new("node-a")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                project_id: Some(
                    ProjectId::new("project-a")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                data_classification: Some("project".into()),
                resource_id: Some("log:project".into()),
                allowed_actions: vec!["write".into()],
            },
            constraints: ToyGrantConstraints {
                starts_at: None,
                expires_at: Some(Timestamp::new("2026-05-31T00:05:00Z").unwrap()),
                max_uses: None,
                max_duration_ms: None,
                locality_required: true,
            },
            grant_state: state,
            issuer_id: "issuer-a".into(),
            policy_revision: 3,
            grants_revision: 7,
            authority_observation_id: ObservationId::new("obs-grant")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    #[test]
    fn active_grant_produces_authorized_toy_call() {
        let result = evaluate_toy_grant_for_call(
            &call(),
            &request(),
            &[toy()],
            &[grant(ToyGrantState::Active)],
        );

        assert!(result.is_allowed());
        assert_eq!(result.evaluation.verdict, ToyGrantVerdict::Allowed);
        assert_eq!(
            result.evaluation.reason_code,
            ToyGrantReasonCode::ActiveGrant
        );
        let authorized = result.authorized.expect("authorized toy call");
        assert_eq!(
            authorized.grant_id(),
            &ToyGrantId::new("grant-logging")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(
            authorized.child_instance_id(),
            &ChildInstanceId::new("instance-a")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(
            authorized.expires_at(),
            &Timestamp::new("2026-05-31T00:05:00Z").unwrap()
        );
    }

    #[test]
    fn toy_token_expiry_is_capped_by_effective_call_deadline() {
        let mut effective_call = call();
        effective_call.deadline = Timestamp::new("2026-05-31T00:01:00Z").unwrap();

        let result = evaluate_toy_grant_for_call(
            &effective_call,
            &request(),
            &[toy()],
            &[grant(ToyGrantState::Active)],
        );

        let authorized = result.authorized.expect("authorized toy call");
        assert_eq!(authorized.expires_at(), &effective_call.deadline);
    }

    #[test]
    fn toy_max_duration_narrows_but_never_extends_effective_deadline() {
        let mut bounded = grant(ToyGrantState::Active);
        bounded.constraints.max_duration_ms = Some(1_000);
        let result = evaluate_toy_grant_for_call(&call(), &request(), &[toy()], &[bounded]);
        assert_eq!(
            result.authorized.unwrap().expires_at(),
            &Timestamp::new("2026-05-31T00:00:01Z").unwrap()
        );

        let mut long = grant(ToyGrantState::Active);
        long.constraints.max_duration_ms = Some(3_600_000);
        let mut short_call = call();
        short_call.deadline = Timestamp::new("2026-05-31T00:00:30Z").unwrap();
        let result = evaluate_toy_grant_for_call(&short_call, &request(), &[toy()], &[long]);
        assert_eq!(
            result.authorized.unwrap().expires_at(),
            &short_call.deadline
        );
    }

    #[test]
    fn unknown_toy_denies_by_default() {
        let mut request = request();
        request.toy_id = ToyId::new("legacy-host-filesystem")
            .expect("string ID literal/generated value must be non-empty");
        let result = evaluate_toy_grant_for_call(
            &call(),
            &request,
            &[toy()],
            &[grant(ToyGrantState::Active)],
        );

        assert!(!result.is_allowed());
        assert_eq!(result.evaluation.verdict, ToyGrantVerdict::Denied);
        assert_eq!(
            result.evaluation.reason_code,
            ToyGrantReasonCode::UnknownToy
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn manifest_need_without_grant_denies_as_missing_grant() {
        let result = evaluate_toy_grant_for_call(&call(), &request(), &[toy()], &[]);

        assert_eq!(
            result.evaluation.reason_code,
            ToyGrantReasonCode::MissingGrant
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn revoked_grant_denies_without_authorization() {
        let result = evaluate_toy_grant_for_call(
            &call(),
            &request(),
            &[toy()],
            &[grant(ToyGrantState::Revoked)],
        );

        assert_eq!(result.evaluation.verdict, ToyGrantVerdict::Denied);
        assert_eq!(
            result.evaluation.reason_code,
            ToyGrantReasonCode::RevokedGrant
        );
        assert_eq!(
            result.evaluation.grant_id,
            Some(
                ToyGrantId::new("grant-logging")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn expired_time_window_denies_without_authorization() {
        let mut request = request();
        request.now = Timestamp::new("2026-05-31T00:05:00Z").unwrap();
        let result = evaluate_toy_grant_for_call(
            &call(),
            &request,
            &[toy()],
            &[grant(ToyGrantState::Active)],
        );

        assert_eq!(
            result.evaluation.reason_code,
            ToyGrantReasonCode::ExpiredGrant
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn wrong_scope_denies_without_authorization() {
        let mut request = request();
        request.action = "delete".into();
        let result = evaluate_toy_grant_for_call(
            &call(),
            &request,
            &[toy()],
            &[grant(ToyGrantState::Active)],
        );

        assert_eq!(
            result.evaluation.reason_code,
            ToyGrantReasonCode::WrongScope
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn call_echo_generation_cannot_create_a_legacy_toy_denial() {
        let mut current = grant(ToyGrantState::Active);
        current.grants_revision = 6;
        let result = evaluate_toy_grant_for_call(&call(), &request(), &[toy()], &[current]);

        assert_eq!(
            result.evaluation.reason_code,
            ToyGrantReasonCode::ActiveGrant
        );
        assert!(result.authorized.is_some());
    }

    fn snapshot_with(
        grants: Vec<ToyGrant>,
        epoch: &str,
        generation: u64,
        evaluated_at: &str,
    ) -> LocalExecutionAuthoritySnapshot {
        crate::assemble_local_execution_authority_snapshot(
            crate::LocalExecutionAuthoritySnapshotPartsV1 {
                executing_mother_node_id: "node-a".into(),
                grants_authority_mother_node_id: "node-a".into(),
                grants_authority_epoch: epoch.into(),
                grants_authority_generation: generation,
                grants_authority_observation_id: "obs-toy-authority".into(),
                toy_catalog: vec![toy()],
                toy_grants: grants,
                watch_scopes: Vec::new(),
                policy_revision: 3,
                vision_policy_revision: 11,
                child_local_node_id: MctNodeId::new("node-a").unwrap(),
                child_vision_id: VisionId::new("vision-a").unwrap(),
                child_artifacts: Vec::new(),
                child_approvals: Vec::new(),
                child_assignments: Vec::new(),
                child_instances: Vec::new(),
                peer_local_node_id: MctNodeId::new("node-a").unwrap(),
                peer_local_vision_id: VisionId::new("vision-a").unwrap(),
                peer_local_endpoint_id: EndpointIdText::new("endpoint-toy-test").unwrap(),
                peer_records: Vec::new(),
                callable_surfaces: Vec::new(),
                evaluated_at: Timestamp::new(evaluated_at).unwrap(),
                projection_id: "authority-state-v1".into(),
                projection_source_mother_node_id: "node-a".into(),
                projection_source_ledger_id: "ledger-toy-test".into(),
                through_sequence: 7,
                through_observation_id: "obs-toy-authority".into(),
                through_entry_hash: "entry-toy-authority".into(),
                authority_state_hash: "state-toy-authority".into(),
                projection_hash: "projection-toy-authority".into(),
            },
        )
    }

    fn snapshot_with_grant(grant: ToyGrant) -> LocalExecutionAuthoritySnapshot {
        snapshot_with(
            vec![grant],
            "mct-authority-epoch-v1:toy-test",
            7,
            "2026-05-31T00:00:30Z",
        )
    }

    /// Phase J proof 7/8: exact grant state is checked independently of generation.
    #[test]
    fn toy_revocation_and_missing_exact_grant_deny_at_effect_time() {
        let minted_snapshot = snapshot_with_grant(grant(ToyGrantState::Active));
        let token = evaluate_toy_grant_for_snapshot(&call(), &request(), &minted_snapshot)
            .authorized
            .expect("current grant mints Toy token");

        let revoked_snapshot = snapshot_with_grant(grant(ToyGrantState::Revoked));
        assert_eq!(
            token
                .admit_effect_with_snapshot(&call(), &revoked_snapshot)
                .unwrap_err(),
            ToyEffectAdmissionDenyV1::InactiveGrant
        );
        let missing_snapshot = snapshot_with(
            Vec::new(),
            "mct-authority-epoch-v1:toy-test",
            7,
            "2026-05-31T00:00:30Z",
        );
        assert_eq!(
            token
                .admit_effect_with_snapshot(&call(), &missing_snapshot)
                .unwrap_err(),
            ToyEffectAdmissionDenyV1::MissingExactGrant
        );

        let mut same_generation_changed_scope = grant(ToyGrantState::Active);
        same_generation_changed_scope
            .scope
            .allowed_actions
            .push("widened".into());
        let changed_scope_snapshot = snapshot_with_grant(same_generation_changed_scope);
        assert_eq!(
            token
                .admit_effect_with_snapshot(&call(), &changed_scope_snapshot)
                .unwrap_err(),
            ToyEffectAdmissionDenyV1::ExactGrantMismatch,
            "matching generation cannot hide a changed exact canonical grant"
        );
    }

    /// Phase J proofs 9 and 12: Mother time and authority epoch remain independent gates.
    #[test]
    fn toy_effect_uses_mother_time_and_rejects_prior_epoch() {
        let minted_snapshot = snapshot_with_grant(grant(ToyGrantState::Active));
        let token = evaluate_toy_grant_for_snapshot(&call(), &request(), &minted_snapshot)
            .authorized
            .expect("current grant mints Toy token");
        let expired = snapshot_with(
            vec![grant(ToyGrantState::Active)],
            "mct-authority-epoch-v1:toy-test",
            7,
            "2026-05-31T00:05:00Z",
        );
        assert_eq!(
            token
                .admit_effect_with_snapshot(&call(), &expired)
                .unwrap_err(),
            ToyEffectAdmissionDenyV1::TokenExpired
        );
        let restarted = snapshot_with(
            vec![grant(ToyGrantState::Active)],
            "mct-authority-epoch-v1:after-restart",
            7,
            "2026-05-31T00:00:30Z",
        );
        assert_eq!(
            token
                .admit_effect_with_snapshot(&call(), &restarted)
                .unwrap_err(),
            ToyEffectAdmissionDenyV1::GrantsAuthorityMismatch
        );
    }

    /// Phase J proof 13: caller authority echoes affect neither Toy mint nor revalidation.
    #[test]
    fn toy_snapshot_mint_and_effect_ignore_hostile_caller_echoes() {
        let snapshot = snapshot_with_grant(grant(ToyGrantState::Active));
        for (policy, grants, vision) in [(0, 0, 0), (u64::MAX, u64::MAX - 1, 42)] {
            let mut hostile = call();
            hostile.authority_context.policy_revision = policy;
            hostile
                .authority_context
                .expected_receiver_grants_authority
                .generation = grants;
            hostile.authority_context.vision_policy_revision = vision;
            let token = evaluate_toy_grant_for_snapshot(&hostile, &request(), &snapshot)
                .authorized
                .expect("local snapshot, not caller echoes, mints Toy token");
            assert_eq!(token.policy_revision(), snapshot.policy_revision());
            assert_eq!(
                token.grants_revision(),
                snapshot.canonical_grants().grants_authority().generation()
            );
            assert!(
                token
                    .admit_effect_with_snapshot(&hostile, &snapshot)
                    .is_ok()
            );
        }
    }

    /// Phase J proof 16: unsupported live consumption state denies rather than becoming unmetered.
    #[test]
    fn consumption_bearing_grant_denies_while_unbounded_grant_admits() {
        let unbounded_grant = grant(ToyGrantState::Active);
        let unbounded_snapshot = snapshot_with_grant(unbounded_grant);
        let admitted = evaluate_toy_grant_for_snapshot(&call(), &request(), &unbounded_snapshot)
            .authorized
            .expect("max_uses None mints current Toy authority");
        assert!(
            admitted
                .admit_effect_with_snapshot(&call(), &unbounded_snapshot)
                .is_ok()
        );

        let mut metered_grant = grant(ToyGrantState::Active);
        metered_grant.constraints.max_uses = Some(1);
        let metered_snapshot = snapshot_with_grant(metered_grant);
        let metered = evaluate_toy_grant_for_snapshot(&call(), &request(), &metered_snapshot)
            .authorized
            .expect("consumption constraint remains represented in the minted exact grant");
        let denied = metered.admit_effect_with_snapshot(&call(), &metered_snapshot);
        assert_eq!(
            denied.unwrap_err(),
            ToyEffectAdmissionDenyV1::ConsumptionStateUnavailable
        );
    }

    #[test]
    fn toy_grant_reason_uses_snake_case_wire_names() {
        let encoded = serde_json::to_string(&ToyGrantReasonCode::MissingGrant).unwrap();
        assert_eq!(encoded, "\"missing_grant\"");
    }
}

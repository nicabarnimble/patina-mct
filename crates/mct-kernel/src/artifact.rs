use crate::{
    ArtifactAcquisitionDecisionId, ArtifactAcquisitionId, ArtifactSourceAuthorityId,
    AuthorizedArtifactAcquisitionId, ObservationId, Timestamp,
};
use serde::{Deserialize, Serialize};

/// Scope mode for a standing artifact source authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactSourceScopeMode {
    /// Every scope value is an exact match.
    Constrained,
    /// A literal `*` deliberately broadens a named dimension.
    ExplicitBroad,
}

/// Explicit artifact, publisher, namespace, and action source scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactSourceScope {
    /// Whether values are exact-only or may contain an explicit `*`.
    pub scope_mode: ArtifactSourceScopeMode,
    /// Exact `name@version` values or explicit `*` values.
    pub artifact_scope: Vec<String>,
    /// Exact publisher claims or explicit `*` values.
    pub publisher_scope: Vec<String>,
    /// Exact WIT namespaces or explicit `*` values.
    pub namespace_scope: Vec<String>,
    /// Actions admitted by this source record.
    pub allowed_actions: Vec<String>,
}

/// Current lifecycle state of a standing source authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactSourceAuthorityState {
    /// Record has not become active.
    Pending,
    /// Record may be evaluated while current and in scope.
    Active,
    /// A later operator fact revoked the record.
    Revoked,
    /// The record's time bound elapsed.
    Expired,
    /// A later record superseded this one.
    Superseded,
}

/// Standing, scoped authority to trust claims from one artifact source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactSourceAuthority {
    /// Immutable source authority identifier.
    pub source_authority_id: ArtifactSourceAuthorityId,
    /// Credential-free canonical source reference.
    pub source_ref: String,
    /// Explicit non-empty source scope.
    pub scope: ArtifactSourceScope,
    /// Additive integrity policy reference.
    pub integrity_policy_ref: String,
    /// Optional additive provenance policy reference.
    pub provenance_policy_ref: Option<String>,
    /// Authenticated issuer principal reference.
    pub issuer_principal_ref: String,
    /// Policy revision under which the record was issued.
    pub policy_revision: u64,
    /// Current projected record state.
    pub authority_state: ArtifactSourceAuthorityState,
    /// Issuance instant.
    pub issued_at: Timestamp,
    /// Mandatory expiry instant.
    pub expires_at: Timestamp,
    /// Durable authority observation.
    pub authority_observation_id: ObservationId,
}

/// Current lifecycle state of an operator-pointed acquisition decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorPointedAcquisitionState {
    /// The decision may authorize its one named attempt.
    Active,
    /// The decision was consumed by an attempt, including a failed attempt.
    Consumed,
    /// A later operator fact revoked the decision.
    Revoked,
    /// The decision elapsed before use.
    Expired,
}

/// One artifact-specific operator-pointed source-trust decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorPointedArtifactAcquisitionDecision {
    /// Immutable decision identifier.
    pub decision_id: ArtifactAcquisitionDecisionId,
    /// Credential-free canonical source reference.
    pub source_ref: String,
    /// Child name claimed before source access.
    pub claimed_child_name: String,
    /// Artifact version claimed before source access.
    pub claimed_artifact_version: String,
    /// Optional expected algorithm-tagged BLAKE3 digest.
    pub expected_digest: Option<String>,
    /// Authenticated issuer principal reference.
    pub issuer_principal_ref: String,
    /// Policy revision under which the decision was made.
    pub policy_revision: u64,
    /// Current projected one-shot state.
    pub decision_state: OperatorPointedAcquisitionState,
    /// Durable authority observation.
    pub authority_observation_id: ObservationId,
}

/// The source-trust path used by one acquisition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactAcquisitionAuthorityPath {
    /// A current standing source record supplied source trust.
    StandingSource,
    /// An artifact-specific operator decision supplied source trust.
    OperatorPointed,
}

/// Whether a source adapter acquired complete bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactAcquisitionOutcome {
    /// Complete bounded artifact bytes were staged.
    Acquired,
    /// Source access did not acquire complete bytes.
    Failed,
}

/// Independent verification result for acquired bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactVerificationOutcome {
    /// Every required verification check passed.
    Verified,
    /// At least one reached verification check rejected the bytes.
    Rejected,
    /// Acquisition failed before verification could run.
    NotReached,
}

/// Immutable evidence for one attempted artifact acquisition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactAcquisition {
    /// Immutable acquisition identifier.
    pub acquisition_id: ArtifactAcquisitionId,
    /// Source-trust path used by the attempt.
    pub authority_path: ArtifactAcquisitionAuthorityPath,
    /// Standing source id when that path was used.
    pub standing_source_authority_id: Option<ArtifactSourceAuthorityId>,
    /// Operator decision id when that path was used.
    pub operator_pointed_decision_id: Option<ArtifactAcquisitionDecisionId>,
    /// Current effect authority consumed by the adapter.
    pub adapter_effect_authority_ref: String,
    /// Credential-free canonical source reference.
    pub source_ref: String,
    /// Claimed child name.
    pub claimed_child_name: String,
    /// Claimed artifact version.
    pub claimed_artifact_version: String,
    /// Complete primary component size when observed.
    pub observed_size_bytes: Option<u64>,
    /// Algorithm-tagged BLAKE3 digest when observed.
    pub observed_digest: Option<String>,
    /// Source adapter outcome.
    pub acquisition_outcome: ArtifactAcquisitionOutcome,
    /// Independent verification outcome.
    pub verification_outcome: ArtifactVerificationOutcome,
    /// Verification observation when reached.
    pub verification_observation_id: Option<ObservationId>,
    /// Terminal acquisition observation.
    pub acquisition_observation_id: ObservationId,
    /// Verified component artifact created or linked by this evidence.
    pub component_artifact_id: Option<crate::ComponentArtifactId>,
}

/// Current direct-operator authority for one filesystem adapter effect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilesystemAcquisitionEffectAuthority {
    /// Durable operator action that admitted the effect.
    pub authority_ref: ObservationId,
    /// Exact filesystem acquisition adapter identity.
    pub adapter_ref: String,
    /// Authenticated local UID.
    pub authenticated_uid: u32,
    /// Source reference bound to the capability.
    pub source_ref: String,
    /// Exact adapter action.
    pub allowed_action: String,
    /// Current policy revision.
    pub policy_revision: u64,
    /// Attempt identity bound to this effect.
    pub attempt_id: ArtifactAcquisitionId,
    /// Deadline after which the effect is stale.
    pub expires_at: Timestamp,
}

/// Claims evaluated before one filesystem acquisition effect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactAcquisitionAuthorityRequest {
    /// Source reference requested from the adapter.
    pub source_ref: String,
    /// Claimed `name@version` identity.
    pub artifact: String,
    /// Claimed publisher identity, when standing scope requires it.
    pub publisher: Option<String>,
    /// WIT namespaces claimed by the package.
    pub namespaces: Vec<String>,
    /// Requested source action.
    pub action: String,
    /// Current policy revision.
    pub policy_revision: u64,
    /// Evaluation instant.
    pub now: Timestamp,
    /// Acquisition attempt bound to the effect.
    pub attempt_id: ArtifactAcquisitionId,
    /// Identifier for the executable one-shot capability.
    pub authorized_id: AuthorizedArtifactAcquisitionId,
}

/// Typed denial class for acquisition authority evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactAcquisitionAuthorityReason {
    /// Source trust and effect authority both matched.
    Allowed,
    /// No valid source-trust path was supplied.
    MissingSourceAuthority,
    /// More than one source-trust path was supplied.
    AmbiguousSourceAuthority,
    /// Standing source state, time, revision, source, or scope did not match.
    StandingSourceNotCurrentOrInScope,
    /// Operator-pointed decision did not match this exact current attempt.
    OperatorDecisionNotCurrentOrMatching,
    /// Adapter effect authority was absent, stale, or mismatched.
    AdapterEffectAuthorityNotCurrentOrMatching,
}

/// Result of pure acquisition authority evaluation.
#[derive(Debug, PartialEq, Eq)]
pub struct ArtifactAcquisitionAuthorityResult {
    /// Typed allow or denial reason.
    pub reason: ArtifactAcquisitionAuthorityReason,
    /// One-shot executable authority, present only on allow.
    pub authorized: Option<AuthorizedFilesystemArtifactAcquisition>,
}

/// Private one-shot capability consumed by the filesystem acquisition adapter.
#[derive(Debug, PartialEq, Eq)]
pub struct AuthorizedFilesystemArtifactAcquisition {
    authorized_id: AuthorizedArtifactAcquisitionId,
    acquisition_id: ArtifactAcquisitionId,
    source_ref: String,
    authority_ref: ObservationId,
    policy_revision: u64,
}

impl AuthorizedFilesystemArtifactAcquisition {
    /// Returns the acquisition attempt bound to this capability.
    pub fn acquisition_id(&self) -> &ArtifactAcquisitionId {
        &self.acquisition_id
    }

    /// Returns the exact canonical source bound to this capability.
    pub fn source_ref(&self) -> &str {
        &self.source_ref
    }

    /// Returns the durable direct-operator effect authority reference.
    pub fn authority_ref(&self) -> &ObservationId {
        &self.authority_ref
    }

    /// Returns the current policy revision at minting.
    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }

    /// Returns this one-shot capability identifier.
    pub fn authorized_id(&self) -> &AuthorizedArtifactAcquisitionId {
        &self.authorized_id
    }
}

/// Evaluates source trust and current direct-operator filesystem effect authority.
///
/// Exactly one source path is required. The returned capability cannot be serialized or
/// reconstructed from persisted acquisition evidence.
pub fn evaluate_artifact_acquisition_authority(
    request: &ArtifactAcquisitionAuthorityRequest,
    standing: Option<&ArtifactSourceAuthority>,
    operator: Option<&OperatorPointedArtifactAcquisitionDecision>,
    effect: Option<&FilesystemAcquisitionEffectAuthority>,
) -> ArtifactAcquisitionAuthorityResult {
    let source_paths = usize::from(standing.is_some()) + usize::from(operator.is_some());
    if source_paths == 0 {
        return denied(ArtifactAcquisitionAuthorityReason::MissingSourceAuthority);
    }
    if source_paths != 1 {
        return denied(ArtifactAcquisitionAuthorityReason::AmbiguousSourceAuthority);
    }

    let source_allowed = if let Some(source) = standing {
        standing_source_allows(source, request)
    } else if let Some(decision) = operator {
        decision.decision_state == OperatorPointedAcquisitionState::Active
            && decision.source_ref == request.source_ref
            && format!(
                "{}@{}",
                decision.claimed_child_name, decision.claimed_artifact_version
            ) == request.artifact
            && decision.policy_revision == request.policy_revision
    } else {
        false
    };
    if !source_allowed {
        return denied(if standing.is_some() {
            ArtifactAcquisitionAuthorityReason::StandingSourceNotCurrentOrInScope
        } else {
            ArtifactAcquisitionAuthorityReason::OperatorDecisionNotCurrentOrMatching
        });
    }

    let Some(effect) = effect else {
        return denied(
            ArtifactAcquisitionAuthorityReason::AdapterEffectAuthorityNotCurrentOrMatching,
        );
    };
    let effect_allowed = effect.adapter_ref == "mct:artifact-acquisition/filesystem@1"
        && effect.source_ref == request.source_ref
        && effect.allowed_action == "read_and_stage"
        && effect.policy_revision == request.policy_revision
        && effect.attempt_id == request.attempt_id
        && effect.expires_at > request.now;
    if !effect_allowed {
        return denied(
            ArtifactAcquisitionAuthorityReason::AdapterEffectAuthorityNotCurrentOrMatching,
        );
    }

    ArtifactAcquisitionAuthorityResult {
        reason: ArtifactAcquisitionAuthorityReason::Allowed,
        authorized: Some(AuthorizedFilesystemArtifactAcquisition {
            authorized_id: request.authorized_id.clone(),
            acquisition_id: effect.attempt_id.clone(),
            source_ref: request.source_ref.clone(),
            authority_ref: effect.authority_ref.clone(),
            policy_revision: request.policy_revision,
        }),
    }
}

fn standing_source_allows(
    source: &ArtifactSourceAuthority,
    request: &ArtifactAcquisitionAuthorityRequest,
) -> bool {
    source.authority_state == ArtifactSourceAuthorityState::Active
        && source.source_ref == request.source_ref
        && source.policy_revision == request.policy_revision
        && source.issued_at <= request.now
        && source.expires_at > request.now
        && source.integrity_policy_ref == "sha256-sidecars-v1"
        && scope_matches(&source.scope, request)
}

fn scope_matches(
    scope: &ArtifactSourceScope,
    request: &ArtifactAcquisitionAuthorityRequest,
) -> bool {
    if scope.artifact_scope.is_empty()
        || scope.publisher_scope.is_empty()
        || scope.namespace_scope.is_empty()
        || scope.allowed_actions.is_empty()
    {
        return false;
    }
    let permit = |values: &[String], value: &str| {
        values.iter().any(|candidate| candidate == value)
            || (scope.scope_mode == ArtifactSourceScopeMode::ExplicitBroad
                && values.iter().any(|candidate| candidate == "*"))
    };
    let Some(publisher) = request.publisher.as_deref() else {
        return false;
    };
    permit(&scope.artifact_scope, &request.artifact)
        && permit(&scope.publisher_scope, publisher)
        && !request.namespaces.is_empty()
        && request
            .namespaces
            .iter()
            .all(|namespace| permit(&scope.namespace_scope, namespace))
        && scope
            .allowed_actions
            .iter()
            .any(|action| action == &request.action)
}

fn denied(reason: ArtifactAcquisitionAuthorityReason) -> ArtifactAcquisitionAuthorityResult {
    ArtifactAcquisitionAuthorityResult {
        reason,
        authorized: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(value: &str) -> Timestamp {
        Timestamp::new(value).unwrap()
    }

    fn request() -> ArtifactAcquisitionAuthorityRequest {
        ArtifactAcquisitionAuthorityRequest {
            source_ref: "file:///tmp/source".into(),
            artifact: "slate-manager@0.2.0".into(),
            publisher: Some("patina".into()),
            namespaces: vec!["patina:slate".into()],
            action: "acquire".into(),
            policy_revision: 1,
            now: timestamp("2026-07-16T12:00:00Z"),
            attempt_id: ArtifactAcquisitionId::new("acquisition-1").unwrap(),
            authorized_id: AuthorizedArtifactAcquisitionId::new("authorized-acquisition").unwrap(),
        }
    }

    fn operator() -> OperatorPointedArtifactAcquisitionDecision {
        OperatorPointedArtifactAcquisitionDecision {
            decision_id: ArtifactAcquisitionDecisionId::new("decision-acquisition").unwrap(),
            source_ref: "file:///tmp/source".into(),
            claimed_child_name: "slate-manager".into(),
            claimed_artifact_version: "0.2.0".into(),
            expected_digest: None,
            issuer_principal_ref: "os-uid:501".into(),
            policy_revision: 1,
            decision_state: OperatorPointedAcquisitionState::Active,
            authority_observation_id: ObservationId::new("obs-acquisition-authority").unwrap(),
        }
    }

    fn effect() -> FilesystemAcquisitionEffectAuthority {
        FilesystemAcquisitionEffectAuthority {
            authority_ref: ObservationId::new("obs-acquisition-authority").unwrap(),
            adapter_ref: "mct:artifact-acquisition/filesystem@1".into(),
            authenticated_uid: 501,
            source_ref: "file:///tmp/source".into(),
            allowed_action: "read_and_stage".into(),
            policy_revision: 1,
            attempt_id: ArtifactAcquisitionId::new("acquisition-1").unwrap(),
            expires_at: timestamp("2026-07-16T12:01:00Z"),
        }
    }

    #[test]
    fn artifact_acquisition_requires_source_path_and_current_adapter_authority() {
        let request = request();
        let operator = operator();
        let effect = effect();
        assert!(
            evaluate_artifact_acquisition_authority(&request, None, Some(&operator), Some(&effect))
                .authorized
                .is_some()
        );
        assert_eq!(
            evaluate_artifact_acquisition_authority(&request, None, Some(&operator), None).reason,
            ArtifactAcquisitionAuthorityReason::AdapterEffectAuthorityNotCurrentOrMatching
        );
        assert_eq!(
            evaluate_artifact_acquisition_authority(&request, None, None, Some(&effect)).reason,
            ArtifactAcquisitionAuthorityReason::MissingSourceAuthority
        );
    }

    #[test]
    fn standing_scope_is_explicit_and_missing_scope_grants_nothing() {
        let request = request();
        let mut source = ArtifactSourceAuthority {
            source_authority_id: ArtifactSourceAuthorityId::new("source-1").unwrap(),
            source_ref: request.source_ref.clone(),
            scope: ArtifactSourceScope {
                scope_mode: ArtifactSourceScopeMode::Constrained,
                artifact_scope: vec![request.artifact.clone()],
                publisher_scope: vec!["patina".into()],
                namespace_scope: vec!["patina:slate".into()],
                allowed_actions: vec!["acquire".into()],
            },
            integrity_policy_ref: "sha256-sidecars-v1".into(),
            provenance_policy_ref: None,
            issuer_principal_ref: "os-uid:501".into(),
            policy_revision: 1,
            authority_state: ArtifactSourceAuthorityState::Active,
            issued_at: timestamp("2026-07-16T11:00:00Z"),
            expires_at: timestamp("2026-07-16T13:00:00Z"),
            authority_observation_id: ObservationId::new("obs-source-1").unwrap(),
        };
        assert!(
            evaluate_artifact_acquisition_authority(&request, Some(&source), None, Some(&effect()))
                .authorized
                .is_some()
        );
        source.scope.publisher_scope.clear();
        assert!(
            evaluate_artifact_acquisition_authority(&request, Some(&source), None, Some(&effect()))
                .authorized
                .is_none()
        );
    }
}

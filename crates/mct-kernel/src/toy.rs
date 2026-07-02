use crate::{call::*, id::*};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `ToyContractIdentity` used by the MCT kernel.
pub struct ToyContractIdentity {
    /// Field `namespace` of this domain record.
    pub namespace: String,
    /// Field `interface_name` of this domain record.
    pub interface_name: String,
    /// Field `version` of this domain record.
    pub version: String,
    /// Field `function_name` of this domain record.
    pub function_name: Option<String>,
    /// Field `resource_name` of this domain record.
    pub resource_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `CanonicalToyContract` used by the MCT kernel.
pub struct CanonicalToyContract {
    /// Field `toy_id` of this domain record.
    pub toy_id: ToyId,
    /// Field `contract` of this domain record.
    pub contract: ToyContractIdentity,
    /// Field `authority_bearing` of this domain record.
    pub authority_bearing: bool,
    /// Field `catalog_revision` of this domain record.
    pub catalog_revision: u64,
    /// Field `admitted_by_observation_id` of this domain record.
    pub admitted_by_observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `ToyGrantSubject` used by the MCT kernel.
pub struct ToyGrantSubject {
    /// Field `child_name` of this domain record.
    pub child_name: String,
    /// Field `artifact_id` of this domain record.
    pub artifact_id: String,
    /// Field `artifact_version` of this domain record.
    pub artifact_version: String,
    /// Field `assignment_id` of this domain record.
    pub assignment_id: Option<ChildAssignmentId>,
    /// Field `caller_node_id` of this domain record.
    pub caller_node_id: Option<MctNodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `ToyGrantScope` used by the MCT kernel.
pub struct ToyGrantScope {
    /// Field `vision_id` of this domain record.
    pub vision_id: VisionId,
    /// Field `node_id` of this domain record.
    pub node_id: Option<MctNodeId>,
    /// Field `project_id` of this domain record.
    pub project_id: Option<ProjectId>,
    /// Field `data_classification` of this domain record.
    pub data_classification: Option<String>,
    /// Field `resource_id` of this domain record.
    pub resource_id: Option<String>,
    /// Field `allowed_actions` of this domain record.
    pub allowed_actions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `ToyGrantConstraints` used by the MCT kernel.
pub struct ToyGrantConstraints {
    /// Field `starts_at` of this domain record.
    pub starts_at: Option<Timestamp>,
    /// Field `expires_at` of this domain record.
    pub expires_at: Option<Timestamp>,
    /// Field `max_uses` of this domain record.
    pub max_uses: Option<u64>,
    /// Field `max_duration_ms` of this domain record.
    pub max_duration_ms: Option<u64>,
    /// Field `locality_required` of this domain record.
    pub locality_required: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `ToyGrantState` used by the MCT kernel.
pub enum ToyGrantState {
    /// Public `Requested` item.
    Requested,
    /// Public `Active` item.
    Active,
    /// Public `Expired` item.
    Expired,
    /// Public `Revoked` item.
    Revoked,
    /// Public `Superseded` item.
    Superseded,
    /// Public `Denied` item.
    Denied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `ToyGrant` used by the MCT kernel.
pub struct ToyGrant {
    /// Field `grant_id` of this domain record.
    pub grant_id: ToyGrantId,
    /// Field `toy_id` of this domain record.
    pub toy_id: ToyId,
    /// Field `subject` of this domain record.
    pub subject: ToyGrantSubject,
    /// Field `scope` of this domain record.
    pub scope: ToyGrantScope,
    /// Field `constraints` of this domain record.
    pub constraints: ToyGrantConstraints,
    /// Field `grant_state` of this domain record.
    pub grant_state: ToyGrantState,
    /// Field `issuer_id` of this domain record.
    pub issuer_id: String,
    /// Field `policy_revision` of this domain record.
    pub policy_revision: u64,
    /// Field `grants_revision` of this domain record.
    pub grants_revision: u64,
    /// Field `authority_observation_id` of this domain record.
    pub authority_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `ToyGrantVerdict` used by the MCT kernel.
pub enum ToyGrantVerdict {
    /// Public `Allowed` item.
    Allowed,
    /// Public `Denied` item.
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `ToyGrantReasonCode` used by the MCT kernel.
pub enum ToyGrantReasonCode {
    /// Public `ActiveGrant` item.
    ActiveGrant,
    /// Public `MissingGrant` item.
    MissingGrant,
    /// Public `ExpiredGrant` item.
    ExpiredGrant,
    /// Public `RevokedGrant` item.
    RevokedGrant,
    /// Public `WrongScope` item.
    WrongScope,
    /// Public `UnknownToy` item.
    UnknownToy,
    /// Public `PolicyDenied` item.
    PolicyDenied,
    /// Public `StaleSnapshot` item.
    StaleSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `ToyGrantEvaluation` used by the MCT kernel.
pub struct ToyGrantEvaluation {
    /// Field `evaluation_id` of this domain record.
    pub evaluation_id: ToyGrantEvaluationId,
    /// Field `call_id` of this domain record.
    pub call_id: CallId,
    /// Field `decision_id` of this domain record.
    pub decision_id: DecisionId,
    /// Field `grant_id` of this domain record.
    pub grant_id: Option<ToyGrantId>,
    /// Field `toy_id` of this domain record.
    pub toy_id: ToyId,
    /// Field `subject_child_name` of this domain record.
    pub subject_child_name: String,
    /// Field `verdict` of this domain record.
    pub verdict: ToyGrantVerdict,
    /// Field `reason_code` of this domain record.
    pub reason_code: ToyGrantReasonCode,
    /// Field `policy_revision` of this domain record.
    pub policy_revision: u64,
    /// Field `grants_revision` of this domain record.
    pub grants_revision: u64,
    /// Field `observation_id` of this domain record.
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `AuthorizedToyCall` used by the MCT kernel.
pub struct AuthorizedToyCall {
    /// Field `authorized_toy_call_id` of this domain record.
    pub authorized_toy_call_id: AuthorizedToyCallId,
    /// Field `call_id` of this domain record.
    pub call_id: CallId,
    /// Field `evaluation_id` of this domain record.
    pub evaluation_id: ToyGrantEvaluationId,
    /// Field `grant_id` of this domain record.
    pub grant_id: ToyGrantId,
    /// Field `toy_id` of this domain record.
    pub toy_id: ToyId,
    /// Field `child_instance_id` of this domain record.
    pub child_instance_id: ChildInstanceId,
    /// Field `authority_decision_id` of this domain record.
    pub authority_decision_id: DecisionId,
    /// Field `expires_at` of this domain record.
    pub expires_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Domain record `ToyGrantEvaluationIds` used by the MCT kernel.
pub struct ToyGrantEvaluationIds {
    /// Field `evaluation_id` of this domain record.
    pub evaluation_id: ToyGrantEvaluationId,
    /// Field `decision_id` of this domain record.
    pub decision_id: DecisionId,
    /// Field `observation_id` of this domain record.
    pub observation_id: ObservationId,
    /// Field `authorized_toy_call_id` of this domain record.
    pub authorized_toy_call_id: AuthorizedToyCallId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Domain record `ToyGrantEvaluationRequest` used by the MCT kernel.
pub struct ToyGrantEvaluationRequest {
    /// Field `toy_id` of this domain record.
    pub toy_id: ToyId,
    /// Field `subject` of this domain record.
    pub subject: ToyGrantSubject,
    /// Field `child_instance_id` of this domain record.
    pub child_instance_id: ChildInstanceId,
    /// Field `action` of this domain record.
    pub action: String,
    /// Field `resource_id` of this domain record.
    pub resource_id: Option<String>,
    /// Field `node_id` of this domain record.
    pub node_id: MctNodeId,
    /// Field `now` of this domain record.
    pub now: Timestamp,
    /// Field `ids` of this domain record.
    pub ids: ToyGrantEvaluationIds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Domain record `ToyGrantEvaluationResult` used by the MCT kernel.
pub struct ToyGrantEvaluationResult {
    /// Field `evaluation` of this domain record.
    pub evaluation: ToyGrantEvaluation,
    /// Field `authorized` of this domain record.
    pub authorized: Option<AuthorizedToyCall>,
}

impl ToyGrantEvaluationResult {
    /// Executes `is_allowed` for this domain type.
    pub fn is_allowed(&self) -> bool {
        self.evaluation.verdict == ToyGrantVerdict::Allowed && self.authorized.is_some()
    }
}

/// Evaluates `evaluate_toy_grant_for_call` fail-closed from explicit authority inputs.
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
            call.authority_context.grants_revision,
        );
    };

    if !toy.authority_bearing {
        return denied(
            call,
            request,
            None,
            ToyGrantReasonCode::PolicyDenied,
            call.authority_context.policy_revision,
            call.authority_context.grants_revision,
        );
    }

    let mut matching_wrong_state = None;
    for grant in grants.iter().filter(|grant| grant.toy_id == request.toy_id) {
        if !subject_matches(&grant.subject, &request.subject) {
            continue;
        }

        if grant.policy_revision != call.authority_context.policy_revision
            || grant.grants_revision != call.authority_context.grants_revision
        {
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
        let authorized = AuthorizedToyCall {
            authorized_toy_call_id: request.ids.authorized_toy_call_id.clone(),
            call_id: call.call_id.clone(),
            evaluation_id: evaluation.evaluation_id.clone(),
            grant_id: grant.grant_id.clone(),
            toy_id: request.toy_id.clone(),
            child_instance_id: request.child_instance_id.clone(),
            authority_decision_id: evaluation.decision_id.clone(),
            expires_at: grant
                .constraints
                .expires_at
                .clone()
                .unwrap_or_else(|| call.deadline.clone()),
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
        call.authority_context.grants_revision,
    )
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
                approximate_size_bytes: 12,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 3,
                grants_revision: 7,
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
                max_duration_ms: Some(1000),
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
            authorized.grant_id,
            ToyGrantId::new("grant-logging")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(
            authorized.child_instance_id,
            ChildInstanceId::new("instance-a")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(
            authorized.expires_at,
            Timestamp::new("2026-05-31T00:05:00Z").unwrap()
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
    fn stale_grant_revision_denies_without_authorization() {
        let mut stale = grant(ToyGrantState::Active);
        stale.grants_revision = 6;
        let result = evaluate_toy_grant_for_call(&call(), &request(), &[toy()], &[stale]);

        assert_eq!(
            result.evaluation.reason_code,
            ToyGrantReasonCode::StaleSnapshot
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn toy_grant_reason_uses_snake_case_wire_names() {
        let encoded = serde_json::to_string(&ToyGrantReasonCode::MissingGrant).unwrap();
        assert_eq!(encoded, "\"missing_grant\"");
    }
}

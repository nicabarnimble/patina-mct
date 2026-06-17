use crate::{call::*, id::*};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToyContractIdentity {
    pub namespace: String,
    pub interface_name: String,
    pub version: String,
    pub function_name: Option<String>,
    pub resource_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalToyContract {
    pub toy_id: ToyId,
    pub contract: ToyContractIdentity,
    pub authority_bearing: bool,
    pub catalog_revision: u64,
    pub admitted_by_observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToyGrantSubject {
    pub child_name: String,
    pub artifact_id: String,
    pub artifact_version: String,
    pub assignment_id: Option<ChildAssignmentId>,
    pub caller_node_id: Option<MctNodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToyGrantScope {
    pub vision_id: VisionId,
    pub node_id: Option<MctNodeId>,
    pub project_id: Option<ProjectId>,
    pub data_classification: Option<String>,
    pub resource_id: Option<String>,
    pub allowed_actions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToyGrantConstraints {
    pub starts_at: Option<Timestamp>,
    pub expires_at: Option<Timestamp>,
    pub max_uses: Option<u64>,
    pub max_duration_ms: Option<u64>,
    pub locality_required: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToyGrantState {
    Requested,
    Active,
    Expired,
    Revoked,
    Superseded,
    Denied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToyGrant {
    pub grant_id: ToyGrantId,
    pub toy_id: ToyId,
    pub subject: ToyGrantSubject,
    pub scope: ToyGrantScope,
    pub constraints: ToyGrantConstraints,
    pub grant_state: ToyGrantState,
    pub issuer_id: String,
    pub policy_revision: u64,
    pub grants_revision: u64,
    pub authority_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToyGrantVerdict {
    Allowed,
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToyGrantReasonCode {
    ActiveGrant,
    MissingGrant,
    ExpiredGrant,
    RevokedGrant,
    WrongScope,
    UnknownToy,
    PolicyDenied,
    StaleSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToyGrantEvaluation {
    pub evaluation_id: ToyGrantEvaluationId,
    pub call_id: CallId,
    pub decision_id: DecisionId,
    pub grant_id: Option<ToyGrantId>,
    pub toy_id: ToyId,
    pub subject_child_name: String,
    pub verdict: ToyGrantVerdict,
    pub reason_code: ToyGrantReasonCode,
    pub policy_revision: u64,
    pub grants_revision: u64,
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizedToyCall {
    pub authorized_toy_call_id: AuthorizedToyCallId,
    pub call_id: CallId,
    pub evaluation_id: ToyGrantEvaluationId,
    pub grant_id: ToyGrantId,
    pub toy_id: ToyId,
    pub child_instance_id: ChildInstanceId,
    pub authority_decision_id: DecisionId,
    pub expires_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToyGrantEvaluationIds {
    pub evaluation_id: ToyGrantEvaluationId,
    pub decision_id: DecisionId,
    pub observation_id: ObservationId,
    pub authorized_toy_call_id: AuthorizedToyCallId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToyGrantEvaluationRequest {
    pub toy_id: ToyId,
    pub subject: ToyGrantSubject,
    pub child_instance_id: ChildInstanceId,
    pub action: String,
    pub resource_id: Option<String>,
    pub node_id: MctNodeId,
    pub now: Timestamp,
    pub ids: ToyGrantEvaluationIds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToyGrantEvaluationResult {
    pub evaluation: ToyGrantEvaluation,
    pub authorized: Option<AuthorizedToyCall>,
}

impl ToyGrantEvaluationResult {
    pub fn is_allowed(&self) -> bool {
        self.evaluation.verdict == ToyGrantVerdict::Allowed && self.authorized.is_some()
    }
}

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
            call_id: CallId::from("call-toy-1"),
            caller: CallerIdentity {
                node_id: MctNodeId::from("caller-node"),
                user_id: None,
                vision_id: VisionId::from("vision-a"),
                project_id: Some(ProjectId::from("project-a")),
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
            deadline: Timestamp::from("2026-05-31T00:10:00Z"),
            trace_context: TraceContext {
                trace_id: TraceId::from("trace-toy-1"),
                span_id: SpanId::from("span-toy-1"),
            },
            origin: CallOrigin::Cli,
        }
    }

    fn toy() -> CanonicalToyContract {
        CanonicalToyContract {
            toy_id: ToyId::from("toy-logging"),
            contract: ToyContractIdentity {
                namespace: "mct".into(),
                interface_name: "logging".into(),
                version: "0.1.0".into(),
                function_name: Some("write".into()),
                resource_name: None,
            },
            authority_bearing: true,
            catalog_revision: 1,
            admitted_by_observation_id: ObservationId::from("obs-toy-catalog"),
        }
    }

    fn subject() -> ToyGrantSubject {
        ToyGrantSubject {
            child_name: "slate-manager".into(),
            artifact_id: "sha256:artifact".into(),
            artifact_version: "0.2.0".into(),
            assignment_id: Some(ChildAssignmentId::from("assignment-a")),
            caller_node_id: Some(MctNodeId::from("caller-node")),
        }
    }

    fn request() -> ToyGrantEvaluationRequest {
        ToyGrantEvaluationRequest {
            toy_id: ToyId::from("toy-logging"),
            subject: subject(),
            child_instance_id: ChildInstanceId::from("instance-a"),
            action: "write".into(),
            resource_id: Some("log:project".into()),
            node_id: MctNodeId::from("node-a"),
            now: Timestamp::from("2026-05-31T00:00:00Z"),
            ids: ToyGrantEvaluationIds {
                evaluation_id: ToyGrantEvaluationId::from("toy-eval-1"),
                decision_id: DecisionId::from("toy-decision-1"),
                observation_id: ObservationId::from("obs-toy-eval-1"),
                authorized_toy_call_id: AuthorizedToyCallId::from("authorized-toy-call-1"),
            },
        }
    }

    fn grant(state: ToyGrantState) -> ToyGrant {
        ToyGrant {
            grant_id: ToyGrantId::from("grant-logging"),
            toy_id: ToyId::from("toy-logging"),
            subject: subject(),
            scope: ToyGrantScope {
                vision_id: VisionId::from("vision-a"),
                node_id: Some(MctNodeId::from("node-a")),
                project_id: Some(ProjectId::from("project-a")),
                data_classification: Some("project".into()),
                resource_id: Some("log:project".into()),
                allowed_actions: vec!["write".into()],
            },
            constraints: ToyGrantConstraints {
                starts_at: None,
                expires_at: Some(Timestamp::from("2026-05-31T00:05:00Z")),
                max_uses: None,
                max_duration_ms: Some(1000),
                locality_required: true,
            },
            grant_state: state,
            issuer_id: "issuer-a".into(),
            policy_revision: 3,
            grants_revision: 7,
            authority_observation_id: ObservationId::from("obs-grant"),
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
        assert_eq!(authorized.grant_id, ToyGrantId::from("grant-logging"));
        assert_eq!(
            authorized.child_instance_id,
            ChildInstanceId::from("instance-a")
        );
        assert_eq!(
            authorized.expires_at,
            Timestamp::from("2026-05-31T00:05:00Z")
        );
    }

    #[test]
    fn unknown_toy_denies_by_default() {
        let mut request = request();
        request.toy_id = ToyId::from("legacy-host-filesystem");
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
            Some(ToyGrantId::from("grant-logging"))
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn expired_time_window_denies_without_authorization() {
        let mut request = request();
        request.now = Timestamp::from("2026-05-31T00:05:00Z");
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

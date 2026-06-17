use mct_kernel::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MctToyBackend {
    EchoJson,
    StaticFailure { safe_message: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctToyAdapterRegistry {
    backends: BTreeMap<ToyId, MctToyBackend>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctToyAdapterOutcome {
    Success,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctToyCallIds {
    pub started_observation_id: ObservationId,
    pub completed_observation_id: ObservationId,
    pub started_at: Timestamp,
    pub completed_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctToyCallReport {
    pub outcome: MctToyAdapterOutcome,
    pub output_json: Option<String>,
    pub safe_message: String,
    pub observations: Vec<MctObservation>,
}

impl MctToyAdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, toy_id: ToyId, backend: MctToyBackend) {
        self.backends.insert(toy_id, backend);
    }

    pub fn call_authorized_toy(
        &self,
        authorized: &AuthorizedToyCall,
        call: &MctCall,
        input_json: &str,
        ids: MctToyCallIds,
    ) -> MctToyCallReport {
        let started = toy_observation(
            ids.started_observation_id,
            ids.started_at,
            ObservationKind::ToyCallStarted,
            ObservationOutcome::Started,
            call,
            authorized,
            "toy call started",
        );

        let (outcome, output_json, safe_message, kind, observation_outcome) =
            match self.backends.get(&authorized.toy_id) {
                Some(MctToyBackend::EchoJson) => (
                    MctToyAdapterOutcome::Success,
                    Some(input_json.to_owned()),
                    "toy call completed".to_owned(),
                    ObservationKind::ToyCallCompleted,
                    ObservationOutcome::Completed,
                ),
                Some(MctToyBackend::StaticFailure { safe_message }) => (
                    MctToyAdapterOutcome::Failed,
                    None,
                    safe_message.clone(),
                    ObservationKind::ToyCallFailed,
                    ObservationOutcome::Failed,
                ),
                None => (
                    MctToyAdapterOutcome::Failed,
                    None,
                    "toy backend unavailable".to_owned(),
                    ObservationKind::ToyCallFailed,
                    ObservationOutcome::Failed,
                ),
            };
        let completed = toy_observation(
            ids.completed_observation_id,
            ids.completed_at,
            kind,
            observation_outcome,
            call,
            authorized,
            &safe_message,
        );

        MctToyCallReport {
            outcome,
            output_json,
            safe_message,
            observations: vec![started, completed],
        }
    }
}

fn toy_observation(
    observation_id: ObservationId,
    observed_at: Timestamp,
    kind: ObservationKind,
    outcome: ObservationOutcome,
    call: &MctCall,
    authorized: &AuthorizedToyCall,
    safe_message: &str,
) -> MctObservation {
    MctObservation {
        observation_id,
        observed_at,
        kind,
        source_plane: SourcePlane::Toy,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: Some(authorized.authority_decision_id.clone()),
        subject_id: Some(authorized.child_instance_id.to_string()),
        resource_id: Some(authorized.toy_id.to_string()),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: Some(format!(
            "authorized_toy_call:{}",
            authorized.authorized_toy_call_id
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call() -> MctCall {
        MctCall {
            call_id: CallId::from("call-toy-adapter"),
            caller: CallerIdentity {
                node_id: MctNodeId::from("mother-a"),
                user_id: None,
                vision_id: VisionId::from("vision-a"),
                project_id: None,
            },
            target: OperationTarget {
                namespace: "patina".into(),
                interface_name: "echo".into(),
                function_name: "echo".into(),
            },
            payload_metadata: PayloadMetadata {
                data_classification: "public".into(),
                approximate_size_bytes: 11,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 1,
                grants_revision: 2,
                vision_policy_revision: 1,
            },
            deadline: Timestamp::from("2026-05-31T00:01:00Z"),
            trace_context: TraceContext {
                trace_id: TraceId::from("trace-toy-adapter"),
                span_id: SpanId::from("span-toy-adapter"),
            },
            origin: CallOrigin::Cli,
        }
    }

    fn authorized(toy_id: &str) -> AuthorizedToyCall {
        AuthorizedToyCall {
            authorized_toy_call_id: AuthorizedToyCallId::from("authorized-toy-adapter"),
            call_id: CallId::from("call-toy-adapter"),
            evaluation_id: ToyGrantEvaluationId::from("toy-eval-adapter"),
            grant_id: ToyGrantId::from("grant-toy-adapter"),
            toy_id: ToyId::from(toy_id),
            child_instance_id: ChildInstanceId::from("instance-toy-adapter"),
            authority_decision_id: DecisionId::from("decision-toy-adapter"),
            expires_at: Timestamp::from("2026-05-31T00:10:00Z"),
        }
    }

    fn ids(stem: &str) -> MctToyCallIds {
        MctToyCallIds {
            started_observation_id: ObservationId::from(format!("obs-{stem}-started")),
            completed_observation_id: ObservationId::from(format!("obs-{stem}-completed")),
            started_at: Timestamp::from("2026-05-31T00:00:00Z"),
            completed_at: Timestamp::from("2026-05-31T00:00:01Z"),
        }
    }

    #[test]
    fn toy_adapter_requires_authorized_toy_call_and_records_success() {
        let mut registry = MctToyAdapterRegistry::new();
        registry.register(ToyId::from("toy-echo"), MctToyBackend::EchoJson);

        let report = registry.call_authorized_toy(
            &authorized("toy-echo"),
            &call(),
            "{\"ok\":true}",
            ids("toy-success"),
        );

        assert_eq!(report.outcome, MctToyAdapterOutcome::Success);
        assert_eq!(report.output_json, Some("{\"ok\":true}".into()));
        assert_eq!(report.observations[0].kind, ObservationKind::ToyCallStarted);
        assert_eq!(
            report.observations[1].kind,
            ObservationKind::ToyCallCompleted
        );
        assert_eq!(report.observations[1].source_plane, SourcePlane::Toy);
        assert_eq!(
            report.observations[1].decision_id,
            Some(DecisionId::from("decision-toy-adapter"))
        );
    }

    #[test]
    fn toy_backend_failure_is_adapter_observation_not_kernel_denial() {
        let registry = MctToyAdapterRegistry::new();

        let report = registry.call_authorized_toy(
            &authorized("missing-toy"),
            &call(),
            "{}",
            ids("toy-failed"),
        );

        assert_eq!(report.outcome, MctToyAdapterOutcome::Failed);
        assert_eq!(report.safe_message, "toy backend unavailable");
        assert_eq!(report.observations[1].kind, ObservationKind::ToyCallFailed);
        assert_eq!(report.observations[1].outcome, ObservationOutcome::Failed);
        assert_eq!(
            report.observations[1].call_id,
            Some(CallId::from("call-toy-adapter"))
        );
    }
}

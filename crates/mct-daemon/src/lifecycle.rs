use crate::MctConfigChildAuthorityProjection;
use anyhow::{Result, anyhow, bail};
use mct_kernel::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctChildWarmupReport {
    pub child_name: String,
    pub instance: ChildInstance,
    pub observations: Vec<MctObservation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctChildReloadReport {
    pub child_name: String,
    pub previous_instance: ChildInstance,
    pub next_instance: ChildInstance,
    pub observations: Vec<MctObservation>,
}

#[derive(Debug, Error)]
pub enum MctChildReloadError {
    #[error("child '{child_name}' replacement construction failed: {safe_message}")]
    ReplacementConstruction {
        child_name: String,
        safe_message: String,
    },
    #[error("child '{child_name}' replacement verification failed: {safe_message}")]
    ReplacementVerification {
        child_name: String,
        safe_message: String,
    },
}

pub fn warmup_configured_child(
    projection: &MctConfigChildAuthorityProjection,
    child_name: &str,
    trace_id: TraceId,
) -> Result<MctChildWarmupReport> {
    let instance = projection
        .instances
        .iter()
        .find(|instance| instance.child_name == child_name)
        .ok_or_else(|| anyhow!("child '{child_name}' has no configured instance"))?;
    ensure_instance_has_active_authority(projection, instance)?;

    let (ready, transition) = transition_child_instance(
        instance,
        ChildInstanceState::Ready,
        ObservationId::new(format!(
            "obs:warmup-ready:{child_name}:{}",
            instance.generation
        ))
        .expect("string ID literal/generated value must be non-empty"),
    );
    if !transition.allowed {
        bail!(
            "child '{child_name}' warmup denied: {}",
            transition.safe_message
        );
    }

    Ok(MctChildWarmupReport {
        child_name: child_name.into(),
        observations: vec![child_instance_observation(
            trace_id,
            crate::current_timestamp(),
            &ready,
        )],
        instance: ready,
    })
}

pub fn reload_configured_child(
    projection: &MctConfigChildAuthorityProjection,
    child_name: &str,
    trace_id: TraceId,
) -> Result<MctChildReloadReport> {
    reload_configured_child_with_verifier(projection, child_name, trace_id, |replacement| {
        ensure_instance_has_active_authority(projection, replacement).map_err(|error| {
            MctChildReloadError::ReplacementVerification {
                child_name: child_name.into(),
                safe_message: error.to_string(),
            }
        })
    })
}

fn reload_configured_child_with_verifier(
    projection: &MctConfigChildAuthorityProjection,
    child_name: &str,
    trace_id: TraceId,
    verify_replacement: impl FnOnce(&ChildInstance) -> std::result::Result<(), MctChildReloadError>,
) -> Result<MctChildReloadReport> {
    let instance = projection
        .instances
        .iter()
        .find(|instance| instance.child_name == child_name)
        .ok_or_else(|| anyhow!("child '{child_name}' has no configured instance"))?;
    ensure_instance_has_active_authority(projection, instance)?;

    let next_generation = instance
        .generation
        .checked_add(1)
        .filter(|generation| *generation <= i64::MAX as u64)
        .ok_or_else(|| MctChildReloadError::ReplacementConstruction {
            child_name: child_name.into(),
            safe_message: "generation counter exhausted".into(),
        })?;
    let mut next = instance.clone();
    next.instance_id = ChildInstanceId::new(format!("instance:{child_name}:{next_generation}"))
        .expect("string ID literal/generated value must be non-empty");
    next.generation = next_generation;
    next.instance_state = ChildInstanceState::Loading;
    next.readiness_observation_id = None;
    next.last_lifecycle_observation_id =
        ObservationId::new(format!("obs:reload-loading:{child_name}:{next_generation}"))
            .expect("string ID literal/generated value must be non-empty");

    verify_replacement(&next)?;
    let (ready_next, ready_transition) = transition_child_instance(
        &next,
        ChildInstanceState::Ready,
        ObservationId::new(format!("obs:reload-ready:{child_name}:{next_generation}"))
            .expect("string ID literal/generated value must be non-empty"),
    );
    if !ready_transition.allowed {
        return Err(MctChildReloadError::ReplacementVerification {
            child_name: child_name.into(),
            safe_message: ready_transition.safe_message,
        }
        .into());
    }

    let (draining, drain_transition) = transition_child_instance(
        instance,
        ChildInstanceState::Draining,
        ObservationId::new(format!(
            "obs:reload-draining:{child_name}:{}",
            instance.generation
        ))
        .expect("string ID literal/generated value must be non-empty"),
    );
    if !drain_transition.allowed {
        bail!(
            "child '{child_name}' reload drain denied: {}",
            drain_transition.safe_message
        );
    }
    let (stopped, stop_transition) = transition_child_instance(
        &draining,
        ChildInstanceState::Stopped,
        ObservationId::new(format!(
            "obs:reload-stopped:{child_name}:{}",
            instance.generation
        ))
        .expect("string ID literal/generated value must be non-empty"),
    );
    if !stop_transition.allowed {
        bail!(
            "child '{child_name}' reload stop denied: {}",
            stop_transition.safe_message
        );
    }

    Ok(MctChildReloadReport {
        child_name: child_name.into(),
        observations: vec![
            child_instance_observation(trace_id.clone(), crate::current_timestamp(), &ready_next),
            child_instance_observation(trace_id.clone(), crate::current_timestamp(), &draining),
            child_instance_observation(trace_id, crate::current_timestamp(), &stopped),
        ],
        previous_instance: stopped,
        next_instance: ready_next,
    })
}

fn ensure_instance_has_active_authority(
    projection: &MctConfigChildAuthorityProjection,
    instance: &ChildInstance,
) -> Result<()> {
    let assignment = projection
        .assignments
        .iter()
        .find(|assignment| assignment.assignment_id == instance.assignment_id)
        .ok_or_else(|| anyhow!("child '{}' has no assignment", instance.child_name))?;
    if assignment.assignment_state != ChildAssignmentState::Active {
        bail!("child '{}' assignment is not active", instance.child_name);
    }
    let approval = projection
        .approvals
        .iter()
        .find(|approval| approval.approval_id == assignment.approval_id)
        .ok_or_else(|| anyhow!("child '{}' has no approval", instance.child_name))?;
    if approval.approval_state != ChildApprovalState::Approved {
        bail!("child '{}' approval is not approved", instance.child_name);
    }
    let artifact = projection
        .artifacts
        .iter()
        .find(|artifact| artifact.artifact_id == instance.artifact_id)
        .ok_or_else(|| anyhow!("child '{}' artifact is missing", instance.child_name))?;
    if artifact.verification_status != VerificationStatus::Verified {
        bail!("child '{}' artifact is not verified", instance.child_name);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projection(approved: bool) -> MctConfigChildAuthorityProjection {
        let artifact = ComponentArtifact {
            artifact_id: ComponentArtifactId::new("artifact-a")
                .expect("string ID literal/generated value must be non-empty"),
            child_name: "child-a".into(),
            artifact_version: "0.1.0".into(),
            content_hash: "sha256:wasm".into(),
            manifest_hash: "sha256:manifest".into(),
            primary_export: ComponentWitExport {
                namespace: "patina".into(),
                interface_name: "echo".into(),
                version: "0.1.0".into(),
                function_names: vec!["echo".into()],
            },
            runtime_shape: ComponentRuntimeShape::WasmComponent,
            ingress_mode: ChildIngressMode::WitOnly,
            lifecycle_exports: LifecycleExports::AbsentAllowed,
            verification_status: VerificationStatus::Verified,
            provenance_status: mct_kernel::ArtifactProvenanceStatus::HistoricalUnknown,
            acquisition_ids: Vec::new(),
            created_by_observation_id: ObservationId::new("obs-artifact")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let approval = ChildApproval {
            approval_id: ChildApprovalId::new("approval:child-a")
                .expect("string ID literal/generated value must be non-empty"),
            artifact_id: artifact.artifact_id.clone(),
            child_name: "child-a".into(),
            artifact_version: "0.1.0".into(),
            scope_vision_id: Some(
                VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            scope_node_id: Some(
                MctNodeId::new("local-mct")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            scope_project_id: None,
            approval_state: if approved {
                ChildApprovalState::Approved
            } else {
                ChildApprovalState::Candidate
            },
            policy_revision: 1,
            authority_observation_id: ObservationId::new("obs-approval")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let assignment = ChildAssignment {
            assignment_id: ChildAssignmentId::new("assignment:child-a")
                .expect("string ID literal/generated value must be non-empty"),
            approval_id: approval.approval_id.clone(),
            artifact_id: artifact.artifact_id.clone(),
            child_name: "child-a".into(),
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            node_id: Some(
                MctNodeId::new("local-mct")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            project_id: None,
            assignment_state: ChildAssignmentState::Active,
            pinned_artifact_version: "0.1.0".into(),
            assignment_observation_id: ObservationId::new("obs-assignment")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let instance = ChildInstance {
            instance_id: ChildInstanceId::new("instance:child-a:1")
                .expect("string ID literal/generated value must be non-empty"),
            assignment_id: assignment.assignment_id.clone(),
            artifact_id: artifact.artifact_id.clone(),
            child_name: "child-a".into(),
            generation: 1,
            node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            instance_state: ChildInstanceState::Ready,
            readiness_observation_id: Some(
                ObservationId::new("obs-ready")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            last_lifecycle_observation_id: ObservationId::new("obs-ready")
                .expect("string ID literal/generated value must be non-empty"),
        };
        MctConfigChildAuthorityProjection {
            local_node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            project_id: None,
            policy_revision: 1,
            artifacts: vec![artifact],
            approvals: vec![approval],
            assignments: vec![assignment],
            instances: vec![instance],
        }
    }

    #[test]
    fn warmup_requires_approved_authority() {
        let denied = warmup_configured_child(
            &projection(false),
            "child-a",
            TraceId::new("trace-warmup")
                .expect("string ID literal/generated value must be non-empty"),
        );
        assert!(denied.is_err());

        let report = warmup_configured_child(
            &projection(true),
            "child-a",
            TraceId::new("trace-warmup")
                .expect("string ID literal/generated value must be non-empty"),
        )
        .unwrap();
        assert_eq!(report.instance.instance_state, ChildInstanceState::Ready);
        assert_eq!(
            report.observations[0].kind,
            ObservationKind::ChildInstanceReady
        );
    }

    #[test]
    fn reload_records_replacement_ready_before_predecessor_drain() {
        let projection = projection(true);
        let report = reload_configured_child(
            &projection,
            "child-a",
            TraceId::new("trace-reload")
                .expect("string ID literal/generated value must be non-empty"),
        )
        .unwrap();

        assert_eq!(
            projection.instances[0].instance_state,
            ChildInstanceState::Ready
        );
        assert_eq!(report.next_instance.generation, 2);
        assert_eq!(
            report.next_instance.instance_state,
            ChildInstanceState::Ready
        );
        assert_eq!(
            report.previous_instance.instance_state,
            ChildInstanceState::Stopped
        );
        assert_eq!(report.observations.len(), 3);
        assert_eq!(
            report.observations[0].kind,
            ObservationKind::ChildInstanceReady
        );
        assert_eq!(
            report.observations[0].resource_id.as_deref(),
            Some(report.next_instance.instance_id.as_str())
        );
        assert_eq!(
            report.observations[1].kind,
            ObservationKind::ChildInstanceDraining
        );
        assert_eq!(
            report.observations[2].kind,
            ObservationKind::ChildInstanceStopped
        );
    }

    #[test]
    fn failed_replacement_keeps_current_generation_ready_and_callable() {
        let projection = projection(true);
        let error = reload_configured_child_with_verifier(
            &projection,
            "child-a",
            TraceId::new("trace-reload-failed")
                .expect("string ID literal/generated value must be non-empty"),
            |_| {
                Err(MctChildReloadError::ReplacementVerification {
                    child_name: "child-a".into(),
                    safe_message: "injected verification failure".into(),
                })
            },
        )
        .unwrap_err();

        assert!(matches!(
            error.downcast_ref::<MctChildReloadError>(),
            Some(MctChildReloadError::ReplacementVerification { .. })
        ));
        assert_eq!(
            projection.instances[0].instance_state,
            ChildInstanceState::Ready
        );

        let call = MctCall {
            call_id: CallId::new("call-after-failed-reload").unwrap(),
            caller: CallerIdentity {
                node_id: projection.local_node_id.clone(),
                user_id: None,
                vision_id: projection.vision_id.clone(),
                project_id: None,
            },
            target: OperationTarget {
                namespace: "patina".into(),
                interface_name: "echo".into(),
                function_name: "echo".into(),
            },
            payload_metadata: PayloadMetadata {
                data_classification: "public".into(),
                size_bytes: 0,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: projection.policy_revision,
                grants_revision: 1,
                vision_policy_revision: 1,
            },
            deadline: Timestamp::new("2026-07-10T22:00:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-call-after-failed-reload").unwrap(),
                span_id: SpanId::new("span-call-after-failed-reload").unwrap(),
            },
            origin: CallOrigin::Cli,
        };
        let result = evaluate_child_call_authority(
            &call,
            &ChildCallAuthorityRequest {
                instance_id: projection.instances[0].instance_id.clone(),
                node_id: projection.local_node_id.clone(),
                ids: ChildCallAuthorityIds {
                    evaluation_id: ChildCallEvaluationId::new("eval-after-failed-reload").unwrap(),
                    decision_id: DecisionId::new("decision-after-failed-reload").unwrap(),
                    observation_id: ObservationId::new("obs-after-failed-reload").unwrap(),
                    authorized_child_invocation_id: AuthorizedChildInvocationId::new(
                        "authorized-after-failed-reload",
                    )
                    .unwrap(),
                },
            },
            &projection.artifacts,
            &projection.approvals,
            &projection.assignments,
            &projection.instances,
        );
        assert!(result.is_allowed(), "{:#?}", result.evaluation);
    }
}

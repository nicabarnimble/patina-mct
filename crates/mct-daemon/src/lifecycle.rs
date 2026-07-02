use crate::MctConfigChildAuthorityProjection;
use anyhow::{Result, anyhow, bail};
use mct_kernel::*;
use serde::{Deserialize, Serialize};

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
        ObservationId::from(format!(
            "obs:warmup-ready:{child_name}:{}",
            instance.generation
        )),
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
    let instance = projection
        .instances
        .iter()
        .find(|instance| instance.child_name == child_name)
        .ok_or_else(|| anyhow!("child '{child_name}' has no configured instance"))?;
    ensure_instance_has_active_authority(projection, instance)?;

    let (draining, drain_transition) = transition_child_instance(
        instance,
        ChildInstanceState::Draining,
        ObservationId::from(format!(
            "obs:reload-draining:{child_name}:{}",
            instance.generation
        )),
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
        ObservationId::from(format!(
            "obs:reload-stopped:{child_name}:{}",
            instance.generation
        )),
    );
    if !stop_transition.allowed {
        bail!(
            "child '{child_name}' reload stop denied: {}",
            stop_transition.safe_message
        );
    }

    let mut next = stopped.clone();
    next.instance_id = ChildInstanceId::from(format!(
        "instance:{child_name}:{}",
        instance.generation.saturating_add(1)
    ));
    next.generation = instance.generation.saturating_add(1);
    next.instance_state = ChildInstanceState::Loading;
    next.readiness_observation_id = None;
    next.last_lifecycle_observation_id = ObservationId::from(format!(
        "obs:reload-loading:{child_name}:{}",
        next.generation
    ));

    let (ready_next, ready_transition) = transition_child_instance(
        &next,
        ChildInstanceState::Ready,
        ObservationId::from(format!("obs:reload-ready:{child_name}:{}", next.generation)),
    );
    if !ready_transition.allowed {
        bail!(
            "child '{child_name}' reload ready denied: {}",
            ready_transition.safe_message
        );
    }

    Ok(MctChildReloadReport {
        child_name: child_name.into(),
        observations: vec![
            child_instance_observation(trace_id.clone(), crate::current_timestamp(), &draining),
            child_instance_observation(trace_id.clone(), crate::current_timestamp(), &stopped),
            child_instance_observation(trace_id, crate::current_timestamp(), &ready_next),
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
            artifact_id: ComponentArtifactId::from("artifact-a"),
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
            created_by_observation_id: ObservationId::from("obs-artifact"),
        };
        let approval = ChildApproval {
            approval_id: ChildApprovalId::from("approval:child-a"),
            artifact_id: artifact.artifact_id.clone(),
            child_name: "child-a".into(),
            artifact_version: "0.1.0".into(),
            scope_vision_id: Some(VisionId::from("vision-local")),
            scope_node_id: Some(MctNodeId::from("local-mct")),
            scope_project_id: None,
            approval_state: if approved {
                ChildApprovalState::Approved
            } else {
                ChildApprovalState::Candidate
            },
            policy_revision: 1,
            authority_observation_id: ObservationId::from("obs-approval"),
        };
        let assignment = ChildAssignment {
            assignment_id: ChildAssignmentId::from("assignment:child-a"),
            approval_id: approval.approval_id.clone(),
            artifact_id: artifact.artifact_id.clone(),
            child_name: "child-a".into(),
            vision_id: VisionId::from("vision-local"),
            node_id: Some(MctNodeId::from("local-mct")),
            project_id: None,
            assignment_state: ChildAssignmentState::Active,
            pinned_artifact_version: "0.1.0".into(),
            assignment_observation_id: ObservationId::from("obs-assignment"),
        };
        let instance = ChildInstance {
            instance_id: ChildInstanceId::from("instance:child-a:1"),
            assignment_id: assignment.assignment_id.clone(),
            artifact_id: artifact.artifact_id.clone(),
            child_name: "child-a".into(),
            generation: 1,
            node_id: MctNodeId::from("local-mct"),
            instance_state: ChildInstanceState::Ready,
            readiness_observation_id: Some(ObservationId::from("obs-ready")),
            last_lifecycle_observation_id: ObservationId::from("obs-ready"),
        };
        MctConfigChildAuthorityProjection {
            local_node_id: MctNodeId::from("local-mct"),
            vision_id: VisionId::from("vision-local"),
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
        let denied =
            warmup_configured_child(&projection(false), "child-a", TraceId::from("trace-warmup"));
        assert!(denied.is_err());

        let report =
            warmup_configured_child(&projection(true), "child-a", TraceId::from("trace-warmup"))
                .unwrap();
        assert_eq!(report.instance.instance_state, ChildInstanceState::Ready);
        assert_eq!(
            report.observations[0].kind,
            ObservationKind::ChildInstanceReady
        );
    }

    #[test]
    fn reload_drains_stops_and_replaces_generation() {
        let report =
            reload_configured_child(&projection(true), "child-a", TraceId::from("trace-reload"))
                .unwrap();
        assert_eq!(
            report.previous_instance.instance_state,
            ChildInstanceState::Stopped
        );
        assert_eq!(report.next_instance.generation, 2);
        assert_eq!(
            report.next_instance.instance_state,
            ChildInstanceState::Ready
        );
        assert_eq!(report.observations.len(), 3);
        assert_eq!(
            report.observations[0].kind,
            ObservationKind::ChildInstanceDraining
        );
        assert_eq!(
            report.observations[1].kind,
            ObservationKind::ChildInstanceStopped
        );
        assert_eq!(
            report.observations[2].kind,
            ObservationKind::ChildInstanceReady
        );
    }
}

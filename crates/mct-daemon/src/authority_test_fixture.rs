use mct_kernel::*;

pub(crate) fn authorized_child_for_call(
    call: &MctCall,
    child_name: &str,
    node_id: MctNodeId,
    stem: &str,
) -> AuthorizedChildInvocation {
    let artifact_id = ComponentArtifactId::new(format!("artifact-{stem}"))
        .expect("generated artifact id must be non-empty");
    let approval_id = ChildApprovalId::new(format!("approval-{stem}"))
        .expect("generated approval id must be non-empty");
    let assignment_id = ChildAssignmentId::new(format!("assignment-{stem}"))
        .expect("generated assignment id must be non-empty");
    let instance_id = ChildInstanceId::new(format!("instance-{stem}"))
        .expect("generated instance id must be non-empty");
    let artifact = ComponentArtifact {
        artifact_id: artifact_id.clone(),
        child_name: child_name.into(),
        artifact_version: "0.1.0".into(),
        content_hash: format!("sha256:{stem}"),
        manifest_hash: format!("sha256:manifest-{stem}"),
        primary_export: ComponentWitExport {
            namespace: call.target.namespace.clone(),
            interface_name: call.target.interface_name.clone(),
            version: "0.1.0".into(),
            function_names: vec![call.target.function_name.clone()],
        },
        runtime_shape: ComponentRuntimeShape::WasmComponent,
        ingress_mode: ChildIngressMode::WitOnly,
        lifecycle_exports: LifecycleExports::AbsentAllowed,
        verification_status: VerificationStatus::Verified,
        created_by_observation_id: ObservationId::new(format!("obs-artifact-{stem}"))
            .expect("generated observation id must be non-empty"),
    };
    let approval = ChildApproval {
        approval_id: approval_id.clone(),
        artifact_id: artifact_id.clone(),
        child_name: child_name.into(),
        artifact_version: "0.1.0".into(),
        scope_vision_id: Some(call.caller.vision_id.clone()),
        scope_node_id: Some(node_id.clone()),
        scope_project_id: call.caller.project_id.clone(),
        approval_state: ChildApprovalState::Approved,
        policy_revision: call.authority_context.policy_revision,
        authority_observation_id: ObservationId::new(format!("obs-approval-{stem}"))
            .expect("generated observation id must be non-empty"),
    };
    let assignment = ChildAssignment {
        assignment_id: assignment_id.clone(),
        approval_id: approval_id.clone(),
        artifact_id: artifact_id.clone(),
        child_name: child_name.into(),
        vision_id: call.caller.vision_id.clone(),
        node_id: Some(node_id.clone()),
        project_id: call.caller.project_id.clone(),
        assignment_state: ChildAssignmentState::Active,
        pinned_artifact_version: "0.1.0".into(),
        assignment_observation_id: ObservationId::new(format!("obs-assignment-{stem}"))
            .expect("generated observation id must be non-empty"),
    };
    let instance = ChildInstance {
        instance_id: instance_id.clone(),
        assignment_id,
        artifact_id,
        child_name: child_name.into(),
        generation: 1,
        node_id: node_id.clone(),
        instance_state: ChildInstanceState::Ready,
        readiness_observation_id: Some(
            ObservationId::new(format!("obs-ready-{stem}"))
                .expect("generated observation id must be non-empty"),
        ),
        last_lifecycle_observation_id: ObservationId::new(format!("obs-lifecycle-{stem}"))
            .expect("generated observation id must be non-empty"),
    };
    let request = ChildCallAuthorityRequest {
        instance_id,
        node_id,
        ids: ChildCallAuthorityIds {
            evaluation_id: ChildCallEvaluationId::new(format!("eval-child-{stem}"))
                .expect("generated evaluation id must be non-empty"),
            decision_id: DecisionId::new(format!("decision-child-{stem}"))
                .expect("generated decision id must be non-empty"),
            observation_id: ObservationId::new(format!("obs-child-authority-{stem}"))
                .expect("generated observation id must be non-empty"),
            authorized_child_invocation_id: AuthorizedChildInvocationId::new(format!(
                "auth-child-{stem}"
            ))
            .expect("generated authorization id must be non-empty"),
        },
    };

    let result = evaluate_child_call_authority(
        call,
        &request,
        &[artifact],
        &[approval],
        &[assignment],
        &[instance],
    );
    assert!(
        result.is_allowed(),
        "fixture must mint authorized child invocation through evaluator: {:?}",
        result.evaluation
    );
    result
        .authorized
        .expect("allowed child authority must include a capability")
}

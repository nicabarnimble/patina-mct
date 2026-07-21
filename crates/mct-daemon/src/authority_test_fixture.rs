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
        provenance_status: mct_kernel::ArtifactProvenanceStatus::HistoricalUnknown,
        acquisition_ids: Vec::new(),
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

#[allow(dead_code)]
pub(crate) fn authorized_toy_for_call(
    call: &MctCall,
    toy_id: &str,
    child_instance_id: ChildInstanceId,
    action: &str,
    stem: &str,
) -> AuthorizedToyCall {
    let toy_id = ToyId::new(toy_id).expect("generated toy id must be non-empty");
    let grant_id =
        ToyGrantId::new(format!("grant-toy-{stem}")).expect("generated grant id must be non-empty");
    let subject = ToyGrantSubject {
        child_name: format!("child-{stem}"),
        artifact_id: format!("artifact-{stem}"),
        artifact_version: "0.1.0".into(),
        assignment_id: None,
        caller_node_id: Some(call.caller.node_id.clone()),
    };
    let catalog = CanonicalToyContract {
        toy_id: toy_id.clone(),
        contract: ToyContractIdentity {
            namespace: "mct".into(),
            interface_name: format!("toy-{stem}"),
            version: "0.1.0".into(),
            function_name: Some(action.into()),
            resource_name: None,
        },
        authority_bearing: true,
        catalog_revision: 1,
        admitted_by_observation_id: ObservationId::new(format!("obs-toy-catalog-{stem}"))
            .expect("generated observation id must be non-empty"),
    };
    let grant = ToyGrant {
        grant_id,
        toy_id: toy_id.clone(),
        subject: subject.clone(),
        scope: ToyGrantScope {
            vision_id: call.caller.vision_id.clone(),
            node_id: Some(call.caller.node_id.clone()),
            project_id: call.caller.project_id.clone(),
            data_classification: Some(call.payload_metadata.data_classification.clone()),
            resource_id: None,
            allowed_actions: vec![action.into()],
        },
        constraints: ToyGrantConstraints {
            starts_at: None,
            expires_at: Some(call.deadline.clone()),
            max_uses: None,
            max_duration_ms: None,
            locality_required: true,
        },
        grant_state: ToyGrantState::Active,
        issuer_id: format!("issuer-{stem}"),
        policy_revision: call.authority_context.policy_revision,
        grants_revision: call.authority_context.grants_revision,
        authority_observation_id: ObservationId::new(format!("obs-toy-grant-{stem}"))
            .expect("generated observation id must be non-empty"),
    };
    let request = ToyGrantEvaluationRequest {
        toy_id,
        subject,
        child_instance_id,
        action: action.into(),
        resource_id: None,
        node_id: call.caller.node_id.clone(),
        now: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
        ids: ToyGrantEvaluationIds {
            evaluation_id: ToyGrantEvaluationId::new(format!("eval-toy-{stem}"))
                .expect("generated evaluation id must be non-empty"),
            decision_id: DecisionId::new(format!("decision-toy-{stem}"))
                .expect("generated decision id must be non-empty"),
            observation_id: ObservationId::new(format!("obs-toy-authority-{stem}"))
                .expect("generated observation id must be non-empty"),
            authorized_toy_call_id: AuthorizedToyCallId::new(format!("auth-toy-{stem}"))
                .expect("generated authorization id must be non-empty"),
        },
    };

    let result = evaluate_toy_grant_for_call(call, &request, &[catalog], &[grant]);
    assert!(
        result.is_allowed(),
        "fixture must mint authorized toy call through evaluator: {:?}",
        result.evaluation
    );
    result
        .authorized
        .expect("allowed toy grant must include a capability")
}

use crate::{call::*, id::*};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentWitExport {
    pub namespace: String,
    pub interface_name: String,
    pub version: String,
    pub function_names: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentRuntimeShape {
    WasmComponent,
    JvmChild,
    ProcessChild,
    RemoteChild,
}

impl From<RuntimeKind> for ComponentRuntimeShape {
    fn from(value: RuntimeKind) -> Self {
        match value {
            RuntimeKind::Process => Self::ProcessChild,
            RuntimeKind::JvmChild => Self::JvmChild,
            RuntimeKind::WasmComponent => Self::WasmComponent,
            RuntimeKind::RemotePeer => Self::RemoteChild,
            RuntimeKind::Internal => Self::ProcessChild,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildIngressMode {
    WitOnly,
    Hybrid,
    Handle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleExports {
    Required,
    Optional,
    AbsentAllowed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Verified,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentArtifact {
    pub artifact_id: ComponentArtifactId,
    pub child_name: String,
    pub artifact_version: String,
    pub content_hash: String,
    pub manifest_hash: String,
    pub primary_export: ComponentWitExport,
    pub runtime_shape: ComponentRuntimeShape,
    pub ingress_mode: ChildIngressMode,
    pub lifecycle_exports: LifecycleExports,
    pub verification_status: VerificationStatus,
    pub created_by_observation_id: ObservationId,
}

impl ComponentArtifact {
    pub fn exports_operation(&self, target: &OperationTarget) -> bool {
        let interface_with_version = format!(
            "{}@{}",
            self.primary_export.interface_name, self.primary_export.version
        );
        self.primary_export.namespace == target.namespace
            && (self.primary_export.interface_name == target.interface_name
                || interface_with_version == target.interface_name)
            && self
                .primary_export
                .function_names
                .iter()
                .any(|function_name| function_name == &target.function_name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildApprovalState {
    Candidate,
    Approved,
    Blocked,
    Revoked,
    Deprecated,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildApproval {
    pub approval_id: ChildApprovalId,
    pub artifact_id: ComponentArtifactId,
    pub child_name: String,
    pub artifact_version: String,
    pub scope_vision_id: Option<VisionId>,
    pub scope_node_id: Option<MctNodeId>,
    pub scope_project_id: Option<ProjectId>,
    pub approval_state: ChildApprovalState,
    pub policy_revision: u64,
    pub authority_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildAssignmentState {
    Active,
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildAssignment {
    pub assignment_id: ChildAssignmentId,
    pub approval_id: ChildApprovalId,
    pub artifact_id: ComponentArtifactId,
    pub child_name: String,
    pub vision_id: VisionId,
    pub node_id: Option<MctNodeId>,
    pub project_id: Option<ProjectId>,
    pub assignment_state: ChildAssignmentState,
    pub pinned_artifact_version: String,
    pub assignment_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildInstanceState {
    Loading,
    Ready,
    Degraded,
    Draining,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildInstance {
    pub instance_id: ChildInstanceId,
    pub assignment_id: ChildAssignmentId,
    pub artifact_id: ComponentArtifactId,
    pub child_name: String,
    pub generation: u64,
    pub node_id: MctNodeId,
    pub instance_state: ChildInstanceState,
    pub readiness_observation_id: Option<ObservationId>,
    pub last_lifecycle_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildLifecycleTransitionReason {
    Allowed,
    IllegalTransition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildLifecycleTransition {
    pub instance_id: ChildInstanceId,
    pub from_state: ChildInstanceState,
    pub to_state: ChildInstanceState,
    pub reason: ChildLifecycleTransitionReason,
    pub allowed: bool,
    pub safe_message: String,
    pub observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildCallVerdict {
    Allowed,
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildCallReasonCode {
    ReadyAuthorizedInstance,
    UnknownInstance,
    MissingAssignment,
    AssignmentRevoked,
    MissingApproval,
    ApprovalNotApproved,
    ApprovalScopeMismatch,
    ArtifactMissing,
    ArtifactRejected,
    OperationNotExported,
    InstanceNotReady,
    WrongNode,
    WrongProject,
    StalePolicy,
    VersionMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildCallAuthorityEvaluation {
    pub evaluation_id: ChildCallEvaluationId,
    pub call_id: CallId,
    pub decision_id: DecisionId,
    pub instance_id: Option<ChildInstanceId>,
    pub assignment_id: Option<ChildAssignmentId>,
    pub approval_id: Option<ChildApprovalId>,
    pub artifact_id: Option<ComponentArtifactId>,
    pub child_name: Option<String>,
    pub verdict: ChildCallVerdict,
    pub reason_code: ChildCallReasonCode,
    pub policy_revision: u64,
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizedChildInvocation {
    pub authorized_child_invocation_id: AuthorizedChildInvocationId,
    pub call_id: CallId,
    pub evaluation_id: ChildCallEvaluationId,
    pub assignment_id: ChildAssignmentId,
    pub approval_id: ChildApprovalId,
    pub artifact_id: ComponentArtifactId,
    pub child_instance_id: ChildInstanceId,
    pub child_name: String,
    pub authority_decision_id: DecisionId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildCallAuthorityIds {
    pub evaluation_id: ChildCallEvaluationId,
    pub decision_id: DecisionId,
    pub observation_id: ObservationId,
    pub authorized_child_invocation_id: AuthorizedChildInvocationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildCallAuthorityRequest {
    pub instance_id: ChildInstanceId,
    pub node_id: MctNodeId,
    pub ids: ChildCallAuthorityIds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildCallAuthorityResult {
    pub evaluation: ChildCallAuthorityEvaluation,
    pub authorized: Option<AuthorizedChildInvocation>,
}

impl ChildCallAuthorityResult {
    pub fn is_allowed(&self) -> bool {
        self.evaluation.verdict == ChildCallVerdict::Allowed && self.authorized.is_some()
    }
}

pub fn transition_child_instance(
    instance: &ChildInstance,
    to_state: ChildInstanceState,
    observation_id: ObservationId,
) -> (ChildInstance, ChildLifecycleTransition) {
    let allowed = is_allowed_instance_transition(instance.instance_state, to_state);
    let transition = ChildLifecycleTransition {
        instance_id: instance.instance_id.clone(),
        from_state: instance.instance_state,
        to_state,
        reason: if allowed {
            ChildLifecycleTransitionReason::Allowed
        } else {
            ChildLifecycleTransitionReason::IllegalTransition
        },
        allowed,
        safe_message: if allowed {
            "lifecycle transition recorded"
        } else {
            "illegal lifecycle transition"
        }
        .into(),
        observation_id: observation_id.clone(),
    };

    let mut next = instance.clone();
    if allowed {
        next.instance_state = to_state;
        next.last_lifecycle_observation_id = observation_id.clone();
        if to_state == ChildInstanceState::Ready {
            next.readiness_observation_id = Some(observation_id);
        }
    }

    (next, transition)
}

pub fn is_allowed_instance_transition(
    from_state: ChildInstanceState,
    to_state: ChildInstanceState,
) -> bool {
    use ChildInstanceState::{Degraded, Draining, Failed, Loading, Ready, Stopped};

    match (from_state, to_state) {
        (from, to) if from == to => true,
        (Loading, Ready | Degraded | Failed | Stopped) => true,
        (Ready, Degraded | Draining | Failed | Stopped) => true,
        (Degraded, Ready | Draining | Failed | Stopped) => true,
        (Draining, Stopped | Failed) => true,
        (Stopped, Loading) => true,
        (Failed, Loading) => true,
        _ => false,
    }
}

pub fn evaluate_child_call_authority(
    call: &MctCall,
    request: &ChildCallAuthorityRequest,
    artifacts: &[ComponentArtifact],
    approvals: &[ChildApproval],
    assignments: &[ChildAssignment],
    instances: &[ChildInstance],
) -> ChildCallAuthorityResult {
    let Some(instance) = instances
        .iter()
        .find(|instance| instance.instance_id == request.instance_id)
    else {
        return denied(
            call,
            request,
            ChildCallReasonCode::UnknownInstance,
            None,
            None,
            None,
            None,
            None,
            call.authority_context.policy_revision,
        );
    };

    if instance.node_id != request.node_id {
        return denied_for_instance(
            call,
            request,
            instance,
            ChildCallReasonCode::WrongNode,
            call.authority_context.policy_revision,
        );
    }

    if instance.instance_state != ChildInstanceState::Ready {
        return denied_for_instance(
            call,
            request,
            instance,
            ChildCallReasonCode::InstanceNotReady,
            call.authority_context.policy_revision,
        );
    }

    let Some(assignment) = assignments
        .iter()
        .find(|assignment| assignment.assignment_id == instance.assignment_id)
    else {
        return denied_for_instance(
            call,
            request,
            instance,
            ChildCallReasonCode::MissingAssignment,
            call.authority_context.policy_revision,
        );
    };

    if assignment.assignment_state != ChildAssignmentState::Active {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::AssignmentRevoked,
            Some(instance),
            Some(assignment),
            None,
            None,
            call.authority_context.policy_revision,
        );
    }

    if assignment.vision_id != call.caller.vision_id {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::ApprovalScopeMismatch,
            Some(instance),
            Some(assignment),
            None,
            None,
            call.authority_context.policy_revision,
        );
    }

    if !option_matches(&assignment.node_id, &Some(request.node_id.clone())) {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::WrongNode,
            Some(instance),
            Some(assignment),
            None,
            None,
            call.authority_context.policy_revision,
        );
    }

    if !option_matches(&assignment.project_id, &call.caller.project_id) {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::WrongProject,
            Some(instance),
            Some(assignment),
            None,
            None,
            call.authority_context.policy_revision,
        );
    }

    let Some(approval) = approvals
        .iter()
        .find(|approval| approval.approval_id == assignment.approval_id)
    else {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::MissingApproval,
            Some(instance),
            Some(assignment),
            None,
            None,
            call.authority_context.policy_revision,
        );
    };

    if approval.policy_revision != call.authority_context.policy_revision {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::StalePolicy,
            Some(instance),
            Some(assignment),
            Some(approval),
            None,
            approval.policy_revision,
        );
    }

    if approval.approval_state != ChildApprovalState::Approved {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::ApprovalNotApproved,
            Some(instance),
            Some(assignment),
            Some(approval),
            None,
            approval.policy_revision,
        );
    }

    if approval.artifact_id != assignment.artifact_id
        || approval.artifact_id != instance.artifact_id
        || approval.child_name != assignment.child_name
        || approval.child_name != instance.child_name
        || approval.artifact_version != assignment.pinned_artifact_version
    {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::VersionMismatch,
            Some(instance),
            Some(assignment),
            Some(approval),
            None,
            approval.policy_revision,
        );
    }

    if !approval_scope_matches(approval, call, &request.node_id) {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::ApprovalScopeMismatch,
            Some(instance),
            Some(assignment),
            Some(approval),
            None,
            approval.policy_revision,
        );
    }

    let Some(artifact) = artifacts
        .iter()
        .find(|artifact| artifact.artifact_id == assignment.artifact_id)
    else {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::ArtifactMissing,
            Some(instance),
            Some(assignment),
            Some(approval),
            None,
            approval.policy_revision,
        );
    };

    if artifact.verification_status != VerificationStatus::Verified {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::ArtifactRejected,
            Some(instance),
            Some(assignment),
            Some(approval),
            Some(artifact),
            approval.policy_revision,
        );
    }

    if artifact.artifact_version != assignment.pinned_artifact_version
        || artifact.artifact_version != approval.artifact_version
        || artifact.artifact_id != instance.artifact_id
        || artifact.child_name != instance.child_name
    {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::VersionMismatch,
            Some(instance),
            Some(assignment),
            Some(approval),
            Some(artifact),
            approval.policy_revision,
        );
    }

    if !artifact.exports_operation(&call.target) {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::OperationNotExported,
            Some(instance),
            Some(assignment),
            Some(approval),
            Some(artifact),
            approval.policy_revision,
        );
    }

    let evaluation = ChildCallAuthorityEvaluation {
        evaluation_id: request.ids.evaluation_id.clone(),
        call_id: call.call_id.clone(),
        decision_id: request.ids.decision_id.clone(),
        instance_id: Some(instance.instance_id.clone()),
        assignment_id: Some(assignment.assignment_id.clone()),
        approval_id: Some(approval.approval_id.clone()),
        artifact_id: Some(artifact.artifact_id.clone()),
        child_name: Some(instance.child_name.clone()),
        verdict: ChildCallVerdict::Allowed,
        reason_code: ChildCallReasonCode::ReadyAuthorizedInstance,
        policy_revision: approval.policy_revision,
        observation_id: request.ids.observation_id.clone(),
    };
    let authorized = AuthorizedChildInvocation {
        authorized_child_invocation_id: request.ids.authorized_child_invocation_id.clone(),
        call_id: call.call_id.clone(),
        evaluation_id: evaluation.evaluation_id.clone(),
        assignment_id: assignment.assignment_id.clone(),
        approval_id: approval.approval_id.clone(),
        artifact_id: artifact.artifact_id.clone(),
        child_instance_id: instance.instance_id.clone(),
        child_name: instance.child_name.clone(),
        authority_decision_id: evaluation.decision_id.clone(),
    };

    ChildCallAuthorityResult {
        evaluation,
        authorized: Some(authorized),
    }
}

fn denied_for_instance(
    call: &MctCall,
    request: &ChildCallAuthorityRequest,
    instance: &ChildInstance,
    reason_code: ChildCallReasonCode,
    policy_revision: u64,
) -> ChildCallAuthorityResult {
    denied(
        call,
        request,
        reason_code,
        Some(instance.instance_id.clone()),
        Some(instance.assignment_id.clone()),
        None,
        Some(instance.artifact_id.clone()),
        Some(instance.child_name.clone()),
        policy_revision,
    )
}

#[allow(clippy::too_many_arguments)]
fn denied_with_context(
    call: &MctCall,
    request: &ChildCallAuthorityRequest,
    reason_code: ChildCallReasonCode,
    instance: Option<&ChildInstance>,
    assignment: Option<&ChildAssignment>,
    approval: Option<&ChildApproval>,
    artifact: Option<&ComponentArtifact>,
    policy_revision: u64,
) -> ChildCallAuthorityResult {
    denied(
        call,
        request,
        reason_code,
        instance.map(|instance| instance.instance_id.clone()),
        assignment
            .map(|assignment| assignment.assignment_id.clone())
            .or_else(|| instance.map(|instance| instance.assignment_id.clone())),
        approval.map(|approval| approval.approval_id.clone()),
        artifact
            .map(|artifact| artifact.artifact_id.clone())
            .or_else(|| assignment.map(|assignment| assignment.artifact_id.clone()))
            .or_else(|| instance.map(|instance| instance.artifact_id.clone())),
        artifact
            .map(|artifact| artifact.child_name.clone())
            .or_else(|| assignment.map(|assignment| assignment.child_name.clone()))
            .or_else(|| approval.map(|approval| approval.child_name.clone()))
            .or_else(|| instance.map(|instance| instance.child_name.clone())),
        policy_revision,
    )
}

#[allow(clippy::too_many_arguments)]
fn denied(
    call: &MctCall,
    request: &ChildCallAuthorityRequest,
    reason_code: ChildCallReasonCode,
    instance_id: Option<ChildInstanceId>,
    assignment_id: Option<ChildAssignmentId>,
    approval_id: Option<ChildApprovalId>,
    artifact_id: Option<ComponentArtifactId>,
    child_name: Option<String>,
    policy_revision: u64,
) -> ChildCallAuthorityResult {
    ChildCallAuthorityResult {
        evaluation: ChildCallAuthorityEvaluation {
            evaluation_id: request.ids.evaluation_id.clone(),
            call_id: call.call_id.clone(),
            decision_id: request.ids.decision_id.clone(),
            instance_id,
            assignment_id,
            approval_id,
            artifact_id,
            child_name,
            verdict: ChildCallVerdict::Denied,
            reason_code,
            policy_revision,
            observation_id: request.ids.observation_id.clone(),
        },
        authorized: None,
    }
}

fn approval_scope_matches(approval: &ChildApproval, call: &MctCall, node_id: &MctNodeId) -> bool {
    option_matches(
        &approval.scope_vision_id,
        &Some(call.caller.vision_id.clone()),
    ) && option_matches(&approval.scope_node_id, &Some(node_id.clone()))
        && option_matches(&approval.scope_project_id, &call.caller.project_id)
}

fn option_matches<T: Eq>(authority_value: &Option<T>, request_value: &Option<T>) -> bool {
    authority_value
        .as_ref()
        .is_none_or(|authority_value| request_value.as_ref() == Some(authority_value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call() -> MctCall {
        MctCall {
            call_id: CallId::from("call-child-1"),
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
                policy_revision: 5,
                grants_revision: 7,
                vision_policy_revision: 11,
            },
            deadline: Timestamp::from("2026-05-31T00:10:00Z"),
            trace_context: TraceContext {
                trace_id: TraceId::from("trace-child-1"),
                span_id: SpanId::from("span-child-1"),
            },
            origin: CallOrigin::Cli,
        }
    }

    fn artifact() -> ComponentArtifact {
        ComponentArtifact {
            artifact_id: ComponentArtifactId::from("artifact:slate-manager:0.2.0"),
            child_name: "slate-manager".into(),
            artifact_version: "0.2.0".into(),
            content_hash: "sha256:wasm".into(),
            manifest_hash: "sha256:manifest".into(),
            primary_export: ComponentWitExport {
                namespace: "patina".into(),
                interface_name: "slate".into(),
                version: "0.1.0".into(),
                function_names: vec!["list-work".into(), "complete-work".into()],
            },
            runtime_shape: ComponentRuntimeShape::WasmComponent,
            ingress_mode: ChildIngressMode::WitOnly,
            lifecycle_exports: LifecycleExports::AbsentAllowed,
            verification_status: VerificationStatus::Verified,
            created_by_observation_id: ObservationId::from("obs-artifact"),
        }
    }

    fn approval(state: ChildApprovalState) -> ChildApproval {
        ChildApproval {
            approval_id: ChildApprovalId::from("approval-slate-manager"),
            artifact_id: ComponentArtifactId::from("artifact:slate-manager:0.2.0"),
            child_name: "slate-manager".into(),
            artifact_version: "0.2.0".into(),
            scope_vision_id: Some(VisionId::from("vision-a")),
            scope_node_id: Some(MctNodeId::from("node-a")),
            scope_project_id: Some(ProjectId::from("project-a")),
            approval_state: state,
            policy_revision: 5,
            authority_observation_id: ObservationId::from("obs-approval"),
        }
    }

    fn assignment(state: ChildAssignmentState) -> ChildAssignment {
        ChildAssignment {
            assignment_id: ChildAssignmentId::from("assignment-slate-manager"),
            approval_id: ChildApprovalId::from("approval-slate-manager"),
            artifact_id: ComponentArtifactId::from("artifact:slate-manager:0.2.0"),
            child_name: "slate-manager".into(),
            vision_id: VisionId::from("vision-a"),
            node_id: Some(MctNodeId::from("node-a")),
            project_id: Some(ProjectId::from("project-a")),
            assignment_state: state,
            pinned_artifact_version: "0.2.0".into(),
            assignment_observation_id: ObservationId::from("obs-assignment"),
        }
    }

    fn instance(state: ChildInstanceState) -> ChildInstance {
        ChildInstance {
            instance_id: ChildInstanceId::from("instance-slate-manager-1"),
            assignment_id: ChildAssignmentId::from("assignment-slate-manager"),
            artifact_id: ComponentArtifactId::from("artifact:slate-manager:0.2.0"),
            child_name: "slate-manager".into(),
            generation: 1,
            node_id: MctNodeId::from("node-a"),
            instance_state: state,
            readiness_observation_id: if state == ChildInstanceState::Ready {
                Some(ObservationId::from("obs-instance-ready"))
            } else {
                None
            },
            last_lifecycle_observation_id: ObservationId::from("obs-instance-last"),
        }
    }

    fn request() -> ChildCallAuthorityRequest {
        ChildCallAuthorityRequest {
            instance_id: ChildInstanceId::from("instance-slate-manager-1"),
            node_id: MctNodeId::from("node-a"),
            ids: ChildCallAuthorityIds {
                evaluation_id: ChildCallEvaluationId::from("child-eval-1"),
                decision_id: DecisionId::from("child-decision-1"),
                observation_id: ObservationId::from("obs-child-eval-1"),
                authorized_child_invocation_id: AuthorizedChildInvocationId::from(
                    "authorized-child-invocation-1",
                ),
            },
        }
    }

    #[test]
    fn ready_approved_assigned_instance_produces_authorized_child_invocation() {
        let result = evaluate_child_call_authority(
            &call(),
            &request(),
            &[artifact()],
            &[approval(ChildApprovalState::Approved)],
            &[assignment(ChildAssignmentState::Active)],
            &[instance(ChildInstanceState::Ready)],
        );

        assert!(result.is_allowed());
        assert_eq!(
            result.evaluation.reason_code,
            ChildCallReasonCode::ReadyAuthorizedInstance
        );
        let authorized = result.authorized.expect("authorized child invocation");
        assert_eq!(
            authorized.child_instance_id,
            ChildInstanceId::from("instance-slate-manager-1")
        );
        assert_eq!(
            authorized.assignment_id,
            ChildAssignmentId::from("assignment-slate-manager")
        );
        assert_eq!(authorized.child_name, "slate-manager");
    }

    #[test]
    fn unknown_instance_denies_by_default() {
        let mut request = request();
        request.instance_id = ChildInstanceId::from("unknown-instance");
        let result = evaluate_child_call_authority(
            &call(),
            &request,
            &[artifact()],
            &[approval(ChildApprovalState::Approved)],
            &[assignment(ChildAssignmentState::Active)],
            &[instance(ChildInstanceState::Ready)],
        );

        assert!(!result.is_allowed());
        assert_eq!(
            result.evaluation.reason_code,
            ChildCallReasonCode::UnknownInstance
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn ready_instance_without_assignment_denies_fail_closed() {
        let result = evaluate_child_call_authority(
            &call(),
            &request(),
            &[artifact()],
            &[approval(ChildApprovalState::Approved)],
            &[],
            &[instance(ChildInstanceState::Ready)],
        );

        assert_eq!(
            result.evaluation.reason_code,
            ChildCallReasonCode::MissingAssignment
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn not_ready_instance_denies_without_authorization() {
        let result = evaluate_child_call_authority(
            &call(),
            &request(),
            &[artifact()],
            &[approval(ChildApprovalState::Approved)],
            &[assignment(ChildAssignmentState::Active)],
            &[instance(ChildInstanceState::Loading)],
        );

        assert_eq!(
            result.evaluation.reason_code,
            ChildCallReasonCode::InstanceNotReady
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn revoked_assignment_denies_without_authorization() {
        let result = evaluate_child_call_authority(
            &call(),
            &request(),
            &[artifact()],
            &[approval(ChildApprovalState::Approved)],
            &[assignment(ChildAssignmentState::Revoked)],
            &[instance(ChildInstanceState::Ready)],
        );

        assert_eq!(
            result.evaluation.reason_code,
            ChildCallReasonCode::AssignmentRevoked
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn approval_must_be_approved() {
        let result = evaluate_child_call_authority(
            &call(),
            &request(),
            &[artifact()],
            &[approval(ChildApprovalState::Candidate)],
            &[assignment(ChildAssignmentState::Active)],
            &[instance(ChildInstanceState::Ready)],
        );

        assert_eq!(
            result.evaluation.reason_code,
            ChildCallReasonCode::ApprovalNotApproved
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn approval_scope_must_match_call() {
        let mut approval = approval(ChildApprovalState::Approved);
        approval.scope_project_id = Some(ProjectId::from("other-project"));
        let result = evaluate_child_call_authority(
            &call(),
            &request(),
            &[artifact()],
            &[approval],
            &[assignment(ChildAssignmentState::Active)],
            &[instance(ChildInstanceState::Ready)],
        );

        assert_eq!(
            result.evaluation.reason_code,
            ChildCallReasonCode::ApprovalScopeMismatch
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn artifact_must_export_operation() {
        let mut artifact = artifact();
        artifact.primary_export.function_names = vec!["complete-work".into()];
        let result = evaluate_child_call_authority(
            &call(),
            &request(),
            &[artifact],
            &[approval(ChildApprovalState::Approved)],
            &[assignment(ChildAssignmentState::Active)],
            &[instance(ChildInstanceState::Ready)],
        );

        assert_eq!(
            result.evaluation.reason_code,
            ChildCallReasonCode::OperationNotExported
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn stale_policy_revision_denies_child_call() {
        let mut approval = approval(ChildApprovalState::Approved);
        approval.policy_revision = 4;
        let result = evaluate_child_call_authority(
            &call(),
            &request(),
            &[artifact()],
            &[approval],
            &[assignment(ChildAssignmentState::Active)],
            &[instance(ChildInstanceState::Ready)],
        );

        assert_eq!(
            result.evaluation.reason_code,
            ChildCallReasonCode::StalePolicy
        );
        assert!(result.authorized.is_none());
    }

    #[test]
    fn lifecycle_transitions_allow_ready_path_and_reject_illegal_restart() {
        let loading = instance(ChildInstanceState::Loading);
        let (ready, ready_transition) = transition_child_instance(
            &loading,
            ChildInstanceState::Ready,
            ObservationId::from("obs-ready"),
        );
        assert!(ready_transition.allowed);
        assert_eq!(ready.instance_state, ChildInstanceState::Ready);
        assert_eq!(
            ready.readiness_observation_id,
            Some(ObservationId::from("obs-ready"))
        );

        let (draining, draining_transition) = transition_child_instance(
            &ready,
            ChildInstanceState::Draining,
            ObservationId::from("obs-draining"),
        );
        assert!(draining_transition.allowed);
        assert_eq!(draining.instance_state, ChildInstanceState::Draining);

        let (stopped, stopped_transition) = transition_child_instance(
            &draining,
            ChildInstanceState::Stopped,
            ObservationId::from("obs-stopped"),
        );
        assert!(stopped_transition.allowed);
        assert_eq!(stopped.instance_state, ChildInstanceState::Stopped);

        let (still_stopped, illegal_transition) = transition_child_instance(
            &stopped,
            ChildInstanceState::Ready,
            ObservationId::from("obs-illegal"),
        );
        assert!(!illegal_transition.allowed);
        assert_eq!(
            illegal_transition.reason,
            ChildLifecycleTransitionReason::IllegalTransition
        );
        assert_eq!(still_stopped.instance_state, ChildInstanceState::Stopped);
        assert_eq!(
            still_stopped.last_lifecycle_observation_id,
            ObservationId::from("obs-stopped")
        );
    }

    #[test]
    fn child_call_reason_uses_snake_case_wire_names() {
        let encoded = serde_json::to_string(&ChildCallReasonCode::MissingAssignment).unwrap();
        assert_eq!(encoded, "\"missing_assignment\"");
    }
}

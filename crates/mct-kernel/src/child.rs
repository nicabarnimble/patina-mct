use crate::{call::*, id::*};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// WIT export surface declared by a component artifact.
///
/// Child authority requires the artifact primary export to contain the requested operation.
pub struct ComponentWitExport {
    /// WIT namespace for the exported interface.
    pub namespace: String,
    /// WIT interface name exported by the component.
    pub interface_name: String,
    /// Version of the exported WIT interface.
    pub version: String,
    /// Functions exported by this WIT interface.
    pub function_names: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Execution substrate class declared for a child artifact.
pub enum ComponentRuntimeShape {
    /// WASM component runtime.
    WasmComponent,
    /// JVM-backed child runtime.
    JvmChild,
    /// Process-backed child runtime.
    ProcessChild,
    /// Remote peer child route.
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
/// How a child accepts calls at its component boundary.
pub enum ChildIngressMode {
    /// Only WIT-shaped exports are used for ingress.
    WitOnly,
    /// WIT ingress plus compatibility lifecycle/handle exports.
    Hybrid,
    /// Legacy handle-style ingress.
    Handle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Whether legacy lifecycle exports are expected from the child.
pub enum LifecycleExports {
    /// Lifecycle exports must be present.
    Required,
    /// Lifecycle exports may be present.
    Optional,
    /// Lifecycle exports are not required.
    AbsentAllowed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Integrity and manifest verification state for an artifact.
pub enum VerificationStatus {
    /// Artifact integrity and manifest checks passed.
    Verified,
    /// Artifact verification failed.
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Immutable artifact identity and verified WIT export facts.
///
/// Child call authority requires a verified artifact matching the selected assignment, approval, and instance, and exporting the requested operation.
pub struct ComponentArtifact {
    /// Artifact identifier that must match approvals, assignments, and instances.
    pub artifact_id: ComponentArtifactId,
    /// Stable child name used in authority matching.
    pub child_name: String,
    /// Artifact version pinned by approval and assignment.
    pub artifact_version: String,
    /// Digest of the artifact contents.
    pub content_hash: String,
    /// Digest of the child manifest used for verification.
    pub manifest_hash: String,
    /// Primary WIT export used to match call targets.
    pub primary_export: ComponentWitExport,
    /// Runtime substrate declared for this artifact.
    pub runtime_shape: ComponentRuntimeShape,
    /// Call ingress shape supported by this artifact.
    pub ingress_mode: ChildIngressMode,
    /// Lifecycle export policy for this artifact.
    pub lifecycle_exports: LifecycleExports,
    /// Verification result used by authority evaluation.
    pub verification_status: VerificationStatus,
    /// Observation that recorded creation of this artifact fact.
    pub created_by_observation_id: ObservationId,
}

impl ComponentArtifact {
    /// Returns true when the artifact primary export exposes the requested call target.
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
/// Lifecycle state of approval authority for an artifact.
pub enum ChildApprovalState {
    /// Approval exists but grants no execution authority yet.
    Candidate,
    /// Artifact is approved for matching scoped calls.
    Approved,
    /// Approval blocks use.
    Blocked,
    /// Authority was revoked.
    Revoked,
    /// Approval remains visible but should not authorize new use.
    Deprecated,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Authority record saying whether an artifact may be used in a scope.
///
/// Evaluation accepts only `Approved` records whose artifact, child, version, scope, and policy revision match the call and assignment.
pub struct ChildApproval {
    /// Approval considered by the evaluation.
    pub approval_id: ChildApprovalId,
    /// Artifact identifier that must match approvals, assignments, and instances.
    pub artifact_id: ComponentArtifactId,
    /// Stable child name used in authority matching.
    pub child_name: String,
    /// Artifact version pinned by approval and assignment.
    pub artifact_version: String,
    /// Vision scope in which approval is valid.
    pub scope_vision_id: Option<VisionId>,
    /// Optional node scope; absent means any node in the Vision.
    pub scope_node_id: Option<MctNodeId>,
    /// Optional project scope; absent means any project in the Vision.
    pub scope_project_id: Option<ProjectId>,
    /// Approval lifecycle state used during child authority checks.
    pub approval_state: ChildApprovalState,
    /// Policy revision under which this authority fact was issued.
    pub policy_revision: u64,
    /// Observation that recorded this authority fact.
    pub authority_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Lifecycle state of a child placement assignment.
pub enum ChildAssignmentState {
    /// Assignment or grant may authorize if other facts match.
    Active,
    /// Authority was revoked.
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Placement record binding an approved artifact to a node, Vision, and optional project.
///
/// Assignment is not approval by itself; child call authority also requires a matching approval, artifact, and ready instance.
pub struct ChildAssignment {
    /// Assignment identifier referenced by instances and evaluations.
    pub assignment_id: ChildAssignmentId,
    /// Approval considered by the evaluation.
    pub approval_id: ChildApprovalId,
    /// Artifact identifier that must match approvals, assignments, and instances.
    pub artifact_id: ComponentArtifactId,
    /// Stable child name used in authority matching.
    pub child_name: String,
    /// Vision scope for placement or authority matching.
    pub vision_id: VisionId,
    /// Node constraint or requested execution node.
    pub node_id: Option<MctNodeId>,
    /// Optional project scope for placement or caller matching.
    pub project_id: Option<ProjectId>,
    /// Assignment lifecycle state used during authority checks.
    pub assignment_state: ChildAssignmentState,
    /// Artifact version this assignment is pinned to.
    pub pinned_artifact_version: String,
    /// Observation that recorded this assignment.
    pub assignment_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Runtime readiness state of one child generation.
pub enum ChildInstanceState {
    /// Instance is starting and not ready for calls.
    Loading,
    /// Instance may serve calls if authority matches.
    Ready,
    /// Instance is unhealthy but may transition.
    Degraded,
    /// Instance is draining before stop.
    Draining,
    /// Instance is stopped.
    Stopped,
    /// Instance failed.
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Live runtime instance of an assigned child generation.
///
/// Only `Ready` instances on the requested node can produce an authorized child invocation.
pub struct ChildInstance {
    /// Child instance considered for execution.
    pub instance_id: ChildInstanceId,
    /// Assignment identifier referenced by instances and evaluations.
    pub assignment_id: ChildAssignmentId,
    /// Artifact identifier that must match approvals, assignments, and instances.
    pub artifact_id: ComponentArtifactId,
    /// Stable child name used in authority matching.
    pub child_name: String,
    /// Runtime generation number for replacement and drain flows.
    pub generation: u64,
    /// Node constraint or requested execution node.
    pub node_id: MctNodeId,
    /// Readiness state used during child authority checks.
    pub instance_state: ChildInstanceState,
    /// Observation that marked the instance ready, if any.
    pub readiness_observation_id: Option<ObservationId>,
    /// Most recent lifecycle transition observation for this instance.
    pub last_lifecycle_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Reason a child instance state transition was accepted or rejected.
pub enum ChildLifecycleTransitionReason {
    /// Authority or transition succeeded.
    Allowed,
    /// Requested lifecycle transition is not permitted.
    IllegalTransition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Observation-ready record of an attempted child instance state transition.
pub struct ChildLifecycleTransition {
    /// Child instance considered for execution.
    pub instance_id: ChildInstanceId,
    /// State before the attempted transition.
    pub from_state: ChildInstanceState,
    /// State requested by the transition.
    pub to_state: ChildInstanceState,
    /// Typed reason for the transition decision.
    pub reason: ChildLifecycleTransitionReason,
    /// Whether the transition changed the instance state.
    pub allowed: bool,
    /// Caller-safe or operator-safe message for projections.
    pub safe_message: String,
    /// Observation recording this fact.
    pub observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Verdict of child call authority evaluation.
pub enum ChildCallVerdict {
    /// Authority or transition succeeded.
    Allowed,
    /// Authority check failed closed.
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Typed reason produced by child call authority evaluation.
pub enum ChildCallReasonCode {
    /// Ready instance matched assignment, approval, artifact, scope, export, and policy.
    ReadyAuthorizedInstance,
    /// Requested instance was not found.
    UnknownInstance,
    /// Instance assignment was not found.
    MissingAssignment,
    /// Assignment was not active.
    AssignmentRevoked,
    /// Assignment approval was not found.
    MissingApproval,
    /// Approval state was not approved.
    ApprovalNotApproved,
    /// Approval or assignment scope did not match the call.
    ApprovalScopeMismatch,
    /// Assigned artifact was not found.
    ArtifactMissing,
    /// Artifact verification was not successful.
    ArtifactRejected,
    /// Artifact does not export the requested operation.
    OperationNotExported,
    /// Instance state was not ready.
    InstanceNotReady,
    /// Instance or assignment node did not match the request.
    WrongNode,
    /// Assignment project did not match the call project.
    WrongProject,
    /// Approval policy revision did not match the call snapshot.
    StalePolicy,
    /// Artifact, approval, assignment, or instance versions did not agree.
    VersionMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Decision produced by checking whether a ready child instance may handle a call.
///
/// Allowed evaluations cite instance, assignment, approval, and artifact evidence; denied evaluations preserve the first missing or mismatched authority fact.
pub struct ChildCallAuthorityEvaluation {
    /// Evaluation identifier for this authority decision.
    pub evaluation_id: ChildCallEvaluationId,
    /// Call being evaluated or authorized.
    pub call_id: CallId,
    /// Decision identifier for authority and observation linkage.
    pub decision_id: DecisionId,
    /// Child instance considered for execution.
    pub instance_id: Option<ChildInstanceId>,
    /// Assignment identifier referenced by instances and evaluations.
    pub assignment_id: Option<ChildAssignmentId>,
    /// Approval considered by the evaluation.
    pub approval_id: Option<ChildApprovalId>,
    /// Artifact identifier that must match approvals, assignments, and instances.
    pub artifact_id: Option<ComponentArtifactId>,
    /// Stable child name used in authority matching.
    pub child_name: Option<String>,
    /// Allowed or denied outcome.
    pub verdict: ChildCallVerdict,
    /// Typed reason for the verdict.
    pub reason_code: ChildCallReasonCode,
    /// Policy revision under which this authority fact was issued.
    pub policy_revision: u64,
    /// Observation recording this fact.
    pub observation_id: ObservationId,
}

#[derive(Debug, PartialEq, Eq)]
/// Capability token allowing one child instance to execute one call.
///
/// Only [`evaluate_child_call_authority`] mints this single-effect capability.
/// Adapters consume it when invoking the child; persisted state stores
/// provenance facts instead of rehydrating this executable authority.
pub struct AuthorizedChildInvocation {
    /// Token identifier minted only when child authority succeeds.
    authorized_child_invocation_id: AuthorizedChildInvocationId,
    /// Call being evaluated or authorized.
    call_id: CallId,
    /// Evaluation identifier for this authority decision.
    evaluation_id: ChildCallEvaluationId,
    /// Assignment identifier referenced by instances and evaluations.
    assignment_id: ChildAssignmentId,
    /// Approval considered by the evaluation.
    approval_id: ChildApprovalId,
    /// Artifact identifier that must match approvals, assignments, and instances.
    artifact_id: ComponentArtifactId,
    /// Child instance authorized to execute the call.
    child_instance_id: ChildInstanceId,
    /// Stable child name used in authority matching.
    child_name: String,
    /// Decision that minted this authorization token.
    authority_decision_id: DecisionId,
    /// Policy revision under which this capability was minted.
    policy_revision: u64,
}

impl AuthorizedChildInvocation {
    /// Returns the token identifier minted for this single child invocation.
    pub fn authorized_child_invocation_id(&self) -> &AuthorizedChildInvocationId {
        &self.authorized_child_invocation_id
    }

    /// Returns the call authorized for execution.
    pub fn call_id(&self) -> &CallId {
        &self.call_id
    }

    /// Returns the evaluation that produced this capability.
    pub fn evaluation_id(&self) -> &ChildCallEvaluationId {
        &self.evaluation_id
    }

    /// Returns the assignment whose active binding authorized the instance.
    pub fn assignment_id(&self) -> &ChildAssignmentId {
        &self.assignment_id
    }

    /// Returns the approval that authorized the artifact.
    pub fn approval_id(&self) -> &ChildApprovalId {
        &self.approval_id
    }

    /// Returns the verified artifact authorized for execution.
    pub fn artifact_id(&self) -> &ComponentArtifactId {
        &self.artifact_id
    }

    /// Returns the child instance authorized to execute.
    pub fn child_instance_id(&self) -> &ChildInstanceId {
        &self.child_instance_id
    }

    /// Returns the stable child name authorized for execution.
    pub fn child_name(&self) -> &str {
        &self.child_name
    }

    /// Returns the decision that minted this capability.
    pub fn authority_decision_id(&self) -> &DecisionId {
        &self.authority_decision_id
    }

    /// Returns the policy revision under which this capability was minted.
    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Identifiers supplied for child call authority evaluation and token minting.
pub struct ChildCallAuthorityIds {
    /// Evaluation identifier for this authority decision.
    pub evaluation_id: ChildCallEvaluationId,
    /// Decision identifier for authority and observation linkage.
    pub decision_id: DecisionId,
    /// Observation recording this fact.
    pub observation_id: ObservationId,
    /// Token identifier minted only when child authority succeeds.
    pub authorized_child_invocation_id: AuthorizedChildInvocationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Adapter-supplied facts naming the child instance and node requested for execution.
pub struct ChildCallAuthorityRequest {
    /// Child instance considered for execution.
    pub instance_id: ChildInstanceId,
    /// Node constraint or requested execution node.
    pub node_id: MctNodeId,
    /// Identifiers to stamp on the produced evaluation and token.
    pub ids: ChildCallAuthorityIds,
}

#[derive(Debug, PartialEq, Eq)]
/// Result of child call authority evaluation, including a token only on allow.
pub struct ChildCallAuthorityResult {
    /// Typed authority evaluation result.
    pub evaluation: ChildCallAuthorityEvaluation,
    /// Executable child invocation token, present only on allow.
    pub authorized: Option<AuthorizedChildInvocation>,
}

impl ChildCallAuthorityResult {
    /// Returns true only when the evaluation allowed and minted an invocation token.
    pub fn is_allowed(&self) -> bool {
        self.evaluation.verdict == ChildCallVerdict::Allowed && self.authorized.is_some()
    }
}

/// Attempts a child lifecycle transition and records whether it was allowed.
///
/// The instance state changes only for transitions accepted by [`is_allowed_instance_transition`]; illegal transitions return the original instance with a denial transition record.
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

/// Returns whether a lifecycle state change is valid for child instances.
///
/// The transition graph permits restart from stopped/failed through loading, readiness changes from loading/degraded, and drain/stop/fail paths; all other changes fail closed.
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

/// Decides whether a child instance may execute one MCT call.
///
/// Authority facts are the call, requested instance/node, artifact catalog, approvals, assignments, and live instances. It allows only a ready instance on the requested node with an active assignment, approved matching approval, verified matching artifact, fresh policy revision, matching scope, and an exported target operation. Any absent, stale, revoked, mismatched, unready, or unexported fact returns a denied evaluation and no [`AuthorizedChildInvocation`].
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
        policy_revision: evaluation.policy_revision,
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
            call_id: CallId::new("call-child-1")
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
                policy_revision: 5,
                grants_revision: 7,
                vision_policy_revision: 11,
            },
            deadline: Timestamp::new("2026-05-31T00:10:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-child-1")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: SpanId::new("span-child-1")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            origin: CallOrigin::Cli,
        }
    }

    fn artifact() -> ComponentArtifact {
        ComponentArtifact {
            artifact_id: ComponentArtifactId::new("artifact:slate-manager:0.2.0")
                .expect("string ID literal/generated value must be non-empty"),
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
            created_by_observation_id: ObservationId::new("obs-artifact")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn approval(state: ChildApprovalState) -> ChildApproval {
        ChildApproval {
            approval_id: ChildApprovalId::new("approval-slate-manager")
                .expect("string ID literal/generated value must be non-empty"),
            artifact_id: ComponentArtifactId::new("artifact:slate-manager:0.2.0")
                .expect("string ID literal/generated value must be non-empty"),
            child_name: "slate-manager".into(),
            artifact_version: "0.2.0".into(),
            scope_vision_id: Some(
                VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            scope_node_id: Some(
                MctNodeId::new("node-a")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            scope_project_id: Some(
                ProjectId::new("project-a")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            approval_state: state,
            policy_revision: 5,
            authority_observation_id: ObservationId::new("obs-approval")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn assignment(state: ChildAssignmentState) -> ChildAssignment {
        ChildAssignment {
            assignment_id: ChildAssignmentId::new("assignment-slate-manager")
                .expect("string ID literal/generated value must be non-empty"),
            approval_id: ChildApprovalId::new("approval-slate-manager")
                .expect("string ID literal/generated value must be non-empty"),
            artifact_id: ComponentArtifactId::new("artifact:slate-manager:0.2.0")
                .expect("string ID literal/generated value must be non-empty"),
            child_name: "slate-manager".into(),
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
            assignment_state: state,
            pinned_artifact_version: "0.2.0".into(),
            assignment_observation_id: ObservationId::new("obs-assignment")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn instance(state: ChildInstanceState) -> ChildInstance {
        ChildInstance {
            instance_id: ChildInstanceId::new("instance-slate-manager-1")
                .expect("string ID literal/generated value must be non-empty"),
            assignment_id: ChildAssignmentId::new("assignment-slate-manager")
                .expect("string ID literal/generated value must be non-empty"),
            artifact_id: ComponentArtifactId::new("artifact:slate-manager:0.2.0")
                .expect("string ID literal/generated value must be non-empty"),
            child_name: "slate-manager".into(),
            generation: 1,
            node_id: MctNodeId::new("node-a")
                .expect("string ID literal/generated value must be non-empty"),
            instance_state: state,
            readiness_observation_id: if state == ChildInstanceState::Ready {
                Some(
                    ObservationId::new("obs-instance-ready")
                        .expect("string ID literal/generated value must be non-empty"),
                )
            } else {
                None
            },
            last_lifecycle_observation_id: ObservationId::new("obs-instance-last")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn request() -> ChildCallAuthorityRequest {
        ChildCallAuthorityRequest {
            instance_id: ChildInstanceId::new("instance-slate-manager-1")
                .expect("string ID literal/generated value must be non-empty"),
            node_id: MctNodeId::new("node-a")
                .expect("string ID literal/generated value must be non-empty"),
            ids: ChildCallAuthorityIds {
                evaluation_id: ChildCallEvaluationId::new("child-eval-1")
                    .expect("string ID literal/generated value must be non-empty"),
                decision_id: DecisionId::new("child-decision-1")
                    .expect("string ID literal/generated value must be non-empty"),
                observation_id: ObservationId::new("obs-child-eval-1")
                    .expect("string ID literal/generated value must be non-empty"),
                authorized_child_invocation_id: AuthorizedChildInvocationId::new(
                    "authorized-child-invocation-1",
                )
                .expect("string ID literal/generated value must be non-empty"),
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
            authorized.child_instance_id(),
            &ChildInstanceId::new("instance-slate-manager-1")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(
            authorized.assignment_id(),
            &ChildAssignmentId::new("assignment-slate-manager")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(authorized.child_name(), "slate-manager");
    }

    #[test]
    fn unknown_instance_denies_by_default() {
        let mut request = request();
        request.instance_id = ChildInstanceId::new("unknown-instance")
            .expect("string ID literal/generated value must be non-empty");
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
        approval.scope_project_id = Some(
            ProjectId::new("other-project")
                .expect("string ID literal/generated value must be non-empty"),
        );
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
            ObservationId::new("obs-ready")
                .expect("string ID literal/generated value must be non-empty"),
        );
        assert!(ready_transition.allowed);
        assert_eq!(ready.instance_state, ChildInstanceState::Ready);
        assert_eq!(
            ready.readiness_observation_id,
            Some(
                ObservationId::new("obs-ready")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );

        let (draining, draining_transition) = transition_child_instance(
            &ready,
            ChildInstanceState::Draining,
            ObservationId::new("obs-draining")
                .expect("string ID literal/generated value must be non-empty"),
        );
        assert!(draining_transition.allowed);
        assert_eq!(draining.instance_state, ChildInstanceState::Draining);

        let (stopped, stopped_transition) = transition_child_instance(
            &draining,
            ChildInstanceState::Stopped,
            ObservationId::new("obs-stopped")
                .expect("string ID literal/generated value must be non-empty"),
        );
        assert!(stopped_transition.allowed);
        assert_eq!(stopped.instance_state, ChildInstanceState::Stopped);

        let (still_stopped, illegal_transition) = transition_child_instance(
            &stopped,
            ChildInstanceState::Ready,
            ObservationId::new("obs-illegal")
                .expect("string ID literal/generated value must be non-empty"),
        );
        assert!(!illegal_transition.allowed);
        assert_eq!(
            illegal_transition.reason,
            ChildLifecycleTransitionReason::IllegalTransition
        );
        assert_eq!(still_stopped.instance_state, ChildInstanceState::Stopped);
        assert_eq!(
            still_stopped.last_lifecycle_observation_id,
            ObservationId::new("obs-stopped")
                .expect("string ID literal/generated value must be non-empty")
        );
    }

    #[test]
    fn child_call_reason_uses_snake_case_wire_names() {
        let encoded = serde_json::to_string(&ChildCallReasonCode::MissingAssignment).unwrap();
        assert_eq!(encoded, "\"missing_assignment\"");
    }
}

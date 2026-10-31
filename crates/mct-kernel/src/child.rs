use crate::{
    authority::{LocalExecutionAuthoritySnapshot, LocalExecutionAuthorityTokenV1},
    call::*,
    id::*,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// WIT export surface declared by a component artifact.
///
/// Child authority requires the artifact primary export to contain the requested operation.
pub struct ComponentWitExport {
    pub namespace: String,
    pub interface_name: String,
    pub version: String,
    pub function_names: Vec<String>,
}

/// Groups allowed operation ids into one WIT export per interface, in first-seen order.
///
/// Operation ids use `namespace/interface@version.function`. The first interface stays
/// the artifact primary export; later interfaces are additional exports the same artifact
/// may still be called through.
pub fn component_wit_exports_from_operation_ids(
    allowed_operations: &[String],
) -> Vec<ComponentWitExport> {
    let mut exports: Vec<ComponentWitExport> = Vec::new();
    for operation in allowed_operations {
        let Some((namespace, interface_and_function)) = operation.split_once('/') else {
            return vec![ComponentWitExport {
                namespace: String::new(),
                interface_name: String::new(),
                version: "0.0.0".into(),
                function_names: allowed_operations.to_vec(),
            }];
        };
        let Some((interface_with_version, function_name)) = interface_and_function.rsplit_once('.')
        else {
            return vec![ComponentWitExport {
                namespace: String::new(),
                interface_name: String::new(),
                version: "0.0.0".into(),
                function_names: allowed_operations.to_vec(),
            }];
        };
        if function_name.is_empty() {
            return vec![ComponentWitExport {
                namespace: String::new(),
                interface_name: String::new(),
                version: "0.0.0".into(),
                function_names: allowed_operations.to_vec(),
            }];
        }
        let (interface_name, version) = interface_with_version
            .split_once('@')
            .map_or((interface_with_version, "0.0.0"), |(name, version)| {
                (name, version)
            });
        if let Some(existing) = exports.iter_mut().find(|export| {
            export.namespace == namespace
                && export.interface_name == interface_name
                && export.version == version
        }) {
            if !existing
                .function_names
                .iter()
                .any(|name| name == function_name)
            {
                existing.function_names.push(function_name.to_string());
            }
        } else {
            exports.push(ComponentWitExport {
                namespace: namespace.to_string(),
                interface_name: interface_name.to_string(),
                version: version.to_string(),
                function_names: vec![function_name.to_string()],
            });
        }
    }
    exports
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentRuntimeShape {
    WasmComponent,
    JvmChild,
    ProcessChild,
    RemoteChild,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Runtime substrate accepted for current local Child execution.
///
/// Historical component records may contain other [`ComponentRuntimeShape`]
/// values, but only this type may enter a current local execution plan.
pub enum LocalChildRuntime {
    /// A capability-mediated WASM component.
    WasmComponent,
}

impl From<LocalChildRuntime> for RuntimeKind {
    fn from(value: LocalChildRuntime) -> Self {
        match value {
            LocalChildRuntime::WasmComponent => Self::WasmComponent,
        }
    }
}

impl TryFrom<RuntimeKind> for LocalChildRuntime {
    type Error = RuntimeKind;

    fn try_from(value: RuntimeKind) -> Result<Self, Self::Error> {
        match value {
            RuntimeKind::WasmComponent => Ok(Self::WasmComponent),
            RuntimeKind::Process
            | RuntimeKind::JvmChild
            | RuntimeKind::RemotePeer
            | RuntimeKind::Internal => Err(value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
/// Failure to admit a historical runtime shape as current local execution.
pub enum LocalChildRuntimeError {
    /// The recorded shape is retained for history but is not a local Child runtime.
    #[error("recorded runtime shape {0:?} is not executable as a local Child")]
    RetiredOrNonLocal(ComponentRuntimeShape),
}

impl TryFrom<ComponentRuntimeShape> for LocalChildRuntime {
    type Error = LocalChildRuntimeError;

    fn try_from(value: ComponentRuntimeShape) -> Result<Self, Self::Error> {
        match value {
            ComponentRuntimeShape::WasmComponent => Ok(Self::WasmComponent),
            ComponentRuntimeShape::JvmChild
            | ComponentRuntimeShape::ProcessChild
            | ComponentRuntimeShape::RemoteChild => {
                Err(LocalChildRuntimeError::RetiredOrNonLocal(value))
            }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactProvenanceStatus {
    AcquisitionBacked,
    /// The artifact predates acquisition law and has no fabricated equivalent evidence.
    HistoricalUnknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Immutable artifact identity and verified WIT export facts.
///
/// Child call authority requires a verified artifact matching the selected assignment, approval, and instance, and exporting the requested operation.
pub struct ComponentArtifact {
    pub artifact_id: ComponentArtifactId,
    pub child_name: String,
    pub artifact_version: String,
    pub content_hash: String,
    pub manifest_hash: String,
    pub primary_export: ComponentWitExport,
    #[serde(default)]
    pub additional_exports: Vec<ComponentWitExport>,
    pub runtime_shape: ComponentRuntimeShape,
    pub ingress_mode: ChildIngressMode,
    pub lifecycle_exports: LifecycleExports,
    pub verification_status: VerificationStatus,
    pub provenance_status: ArtifactProvenanceStatus,
    pub acquisition_ids: Vec<crate::ArtifactAcquisitionId>,
    pub created_by_observation_id: ObservationId,
}

impl ComponentArtifact {
    pub fn exports_operation(&self, target: &OperationTarget) -> bool {
        std::iter::once(&self.primary_export)
            .chain(self.additional_exports.iter())
            .any(|export| wit_export_matches(export, target))
    }
}

fn wit_export_matches(export: &ComponentWitExport, target: &OperationTarget) -> bool {
    let interface_with_version = format!("{}@{}", export.interface_name, export.version);
    export.namespace == target.namespace
        && (export.interface_name == target.interface_name
            || interface_with_version == target.interface_name)
        && export
            .function_names
            .iter()
            .any(|function_name| function_name == &target.function_name)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildApprovalState {
    /// Approval exists but grants no execution authority yet.
    Candidate,
    Approved,
    Blocked,
    Revoked,
    /// Approval remains visible but should not authorize new use.
    Deprecated,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Authority record saying whether an artifact may be used in a scope.
///
/// Evaluation accepts only `Approved` records whose artifact, child, version, scope, and policy revision match the call and assignment.
pub struct ChildApproval {
    pub approval_id: ChildApprovalId,
    pub artifact_id: ComponentArtifactId,
    pub child_name: String,
    pub artifact_version: String,
    pub scope_vision_id: Option<VisionId>,
    /// Optional node scope; absent means any node in the Vision.
    pub scope_node_id: Option<MctNodeId>,
    /// Optional project scope; absent means any project in the Vision.
    pub scope_project_id: Option<ProjectId>,
    pub approval_state: ChildApprovalState,
    pub policy_revision: u64,
    pub authority_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildAssignmentState {
    /// Assignment or grant may authorize if other facts match.
    Active,
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Placement record binding an approved artifact to a node, Vision, and optional project.
///
/// Assignment is not approval by itself; child call authority also requires a matching approval, artifact, and ready instance.
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
/// Live runtime instance of an assigned child generation.
///
/// Only `Ready` instances on the requested node can produce an authorized child invocation.
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
    /// Approval or assignment scope did not match the call.
    ApprovalScopeMismatch,
    ArtifactMissing,
    ArtifactRejected,
    UnsupportedLocalRuntime,
    OperationNotExported,
    InstanceNotReady,
    WrongNode,
    WrongProject,
    StalePolicy,
    VersionMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Decision produced by checking whether a ready child instance may handle a call.
///
/// Allowed evaluations cite instance, assignment, approval, and artifact evidence; denied evaluations preserve the first missing or mismatched authority fact.
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

#[derive(Debug, PartialEq, Eq)]
/// Capability token allowing one child instance to execute one call.
///
/// Only [`evaluate_child_call_authority`] mints this single-effect capability.
/// Adapters consume it when invoking the child; persisted state stores
/// provenance facts instead of rehydrating this executable authority.
pub struct AuthorizedChildInvocation {
    call_binding: crate::invocation_data::InvocationCallBinding,
    data_authority: Option<crate::LocalInvocationData>,
    manifest_hash: String,
    authorized_child_invocation_id: AuthorizedChildInvocationId,
    call_id: CallId,
    evaluation_id: ChildCallEvaluationId,
    assignment_id: ChildAssignmentId,
    approval_id: ChildApprovalId,
    artifact_id: ComponentArtifactId,
    child_instance_id: ChildInstanceId,
    child_name: String,
    authority_decision_id: DecisionId,
    policy_revision: u64,
    local_execution_authority: Option<LocalExecutionAuthorityTokenV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildEffectAdmissionDenyV1 {
    /// The capability did not pass snapshot-sourced route minting.
    LocalAuthorityUnavailable,
    CallIdMismatch,
    CallDataMismatch,
    EffectiveDeadlineMismatch,
    PolicyAuthorityMismatch,
    GrantsAuthorityMismatch,
    /// The executing Mother reached the exclusive deadline.
    Expired,
    ChildAuthorityMismatch,
}

#[derive(Debug)]
pub struct AdmittedChildEffect<'a> {
    authorized: &'a AuthorizedChildInvocation,
}

impl AdmittedChildEffect<'_> {
    pub fn authorized(&self) -> &AuthorizedChildInvocation {
        self.authorized
    }
}

impl AuthorizedChildInvocation {
    pub(crate) fn bind_local_execution_authority(
        mut self,
        authority: LocalExecutionAuthorityTokenV1,
    ) -> Self {
        self.local_execution_authority = Some(authority);
        self
    }

    /// Admits this token against one fresh proof-gated executing-Mother snapshot.
    pub fn admit_effect_with_snapshot<'a>(
        &'a self,
        call: &MctCall,
        snapshot: &LocalExecutionAuthoritySnapshot,
    ) -> Result<AdmittedChildEffect<'a>, ChildEffectAdmissionDenyV1> {
        let authority = self
            .local_execution_authority
            .as_ref()
            .ok_or(ChildEffectAdmissionDenyV1::LocalAuthorityUnavailable)?;
        if self.call_id != call.call_id {
            return Err(ChildEffectAdmissionDenyV1::CallIdMismatch);
        }
        if !self.call_binding.matches_call(call) {
            return Err(ChildEffectAdmissionDenyV1::CallDataMismatch);
        }
        if authority.effective_deadline() != &call.deadline {
            return Err(ChildEffectAdmissionDenyV1::EffectiveDeadlineMismatch);
        }
        if authority.policy_revision() != snapshot.policy_revision()
            || authority.vision_policy_revision() != snapshot.vision_policy_revision()
        {
            return Err(ChildEffectAdmissionDenyV1::PolicyAuthorityMismatch);
        }
        if authority.grants_authority() != snapshot.canonical_grants().grants_authority()
            || authority.grants_authority().mother_node_id() != snapshot.executing_mother_node_id()
        {
            return Err(ChildEffectAdmissionDenyV1::GrantsAuthorityMismatch);
        }
        if snapshot.mother_clock().evaluated_at() >= authority.effective_deadline() {
            return Err(ChildEffectAdmissionDenyV1::Expired);
        }
        let policy = snapshot.child_policy();
        let Some(instance) = policy.instances().iter().find(|instance| {
            instance.instance_id == self.child_instance_id
                && instance.assignment_id == self.assignment_id
                && instance.artifact_id == self.artifact_id
                && instance.child_name == self.child_name
                && instance.instance_state == ChildInstanceState::Ready
        }) else {
            return Err(ChildEffectAdmissionDenyV1::ChildAuthorityMismatch);
        };
        let Some(assignment) = policy.assignments().iter().find(|assignment| {
            assignment.assignment_id == instance.assignment_id
                && assignment.approval_id == self.approval_id
                && assignment.artifact_id == self.artifact_id
                && assignment.child_name == self.child_name
                && assignment.assignment_state == ChildAssignmentState::Active
        }) else {
            return Err(ChildEffectAdmissionDenyV1::ChildAuthorityMismatch);
        };
        let Some(approval) = policy.approvals().iter().find(|approval| {
            approval.approval_id == assignment.approval_id
                && approval.artifact_id == self.artifact_id
                && approval.child_name == self.child_name
                && approval.approval_state == ChildApprovalState::Approved
                && approval.policy_revision == snapshot.policy_revision()
        }) else {
            return Err(ChildEffectAdmissionDenyV1::ChildAuthorityMismatch);
        };
        if !policy.artifacts().iter().any(|artifact| {
            artifact.artifact_id == approval.artifact_id
                && artifact.child_name == self.child_name
                && artifact.artifact_version == approval.artifact_version
                && artifact.manifest_hash == self.manifest_hash
                && artifact.verification_status == VerificationStatus::Verified
                && LocalChildRuntime::try_from(artifact.runtime_shape).is_ok()
        }) {
            return Err(ChildEffectAdmissionDenyV1::ChildAuthorityMismatch);
        }
        Ok(AdmittedChildEffect { authorized: self })
    }

    /// Legacy exact-call guard retained for non-resident compatibility surfaces.
    /// It carries no local authority and must not be used by resident effects.
    pub fn admit_effect_for_call<'a>(&'a self, call: &MctCall) -> Option<AdmittedChildEffect<'a>> {
        (self.call_id == call.call_id && self.call_binding.matches_call(call))
            .then_some(AdmittedChildEffect { authorized: self })
    }

    /// Semantic binding only; this does not replace current authority admission.
    pub fn matches_invocation_data(&self, call: &MctCall) -> bool {
        self.call_id == call.call_id && self.call_binding.matches_call(call)
    }

    pub fn data_authority(&self) -> Option<&crate::LocalInvocationData> {
        self.data_authority.as_ref()
    }

    pub fn manifest_hash(&self) -> &str {
        &self.manifest_hash
    }

    pub fn authorized_child_invocation_id(&self) -> &AuthorizedChildInvocationId {
        &self.authorized_child_invocation_id
    }

    pub fn call_id(&self) -> &CallId {
        &self.call_id
    }

    pub fn evaluation_id(&self) -> &ChildCallEvaluationId {
        &self.evaluation_id
    }

    pub fn assignment_id(&self) -> &ChildAssignmentId {
        &self.assignment_id
    }

    pub fn approval_id(&self) -> &ChildApprovalId {
        &self.approval_id
    }

    pub fn artifact_id(&self) -> &ComponentArtifactId {
        &self.artifact_id
    }

    pub fn child_instance_id(&self) -> &ChildInstanceId {
        &self.child_instance_id
    }

    pub fn child_name(&self) -> &str {
        &self.child_name
    }

    pub fn authority_decision_id(&self) -> &DecisionId {
        &self.authority_decision_id
    }

    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }

    pub fn local_execution_authority(&self) -> Option<&LocalExecutionAuthorityTokenV1> {
        self.local_execution_authority.as_ref()
    }
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
    /// Independent Mother-owned data evidence; never copied from wire claims.
    pub data_authority: Option<crate::LocalInvocationData>,
    pub instance_id: ChildInstanceId,
    pub node_id: MctNodeId,
    pub ids: ChildCallAuthorityIds,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ChildCallAuthorityResult {
    pub evaluation: ChildCallAuthorityEvaluation,
    pub authorized: Option<AuthorizedChildInvocation>,
}

impl ChildCallAuthorityResult {
    /// Returns true only when the evaluation allowed and minted an invocation token.
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
    evaluate_child_call_authority_with_policy(
        call,
        request,
        artifacts,
        approvals,
        assignments,
        instances,
        call.authority_context.policy_revision,
    )
}

/// Route-evaluation variant that receives the executing Mother's local policy revision.
pub fn evaluate_child_call_authority_with_policy(
    call: &MctCall,
    request: &ChildCallAuthorityRequest,
    artifacts: &[ComponentArtifact],
    approvals: &[ChildApproval],
    assignments: &[ChildAssignment],
    instances: &[ChildInstance],
    expected_policy_revision: u64,
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
            expected_policy_revision,
        );
    };

    if instance.node_id != request.node_id {
        return denied_for_instance(
            call,
            request,
            instance,
            ChildCallReasonCode::WrongNode,
            expected_policy_revision,
        );
    }

    if instance.instance_state != ChildInstanceState::Ready {
        return denied_for_instance(
            call,
            request,
            instance,
            ChildCallReasonCode::InstanceNotReady,
            expected_policy_revision,
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
            expected_policy_revision,
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
            expected_policy_revision,
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
            expected_policy_revision,
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
            expected_policy_revision,
        );
    }

    if !crate::invocation_data::data_scope_matches(
        call,
        request.data_authority.as_ref(),
        &assignment.project_id,
        &None,
    ) {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::WrongProject,
            Some(instance),
            Some(assignment),
            None,
            None,
            expected_policy_revision,
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
            expected_policy_revision,
        );
    };

    if approval.policy_revision != expected_policy_revision {
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

    if !approval_scope_matches(approval, call, &request.node_id)
        || !crate::invocation_data::data_scope_matches(
            call,
            request.data_authority.as_ref(),
            &approval.scope_project_id,
            &None,
        )
    {
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

    if LocalChildRuntime::try_from(artifact.runtime_shape).is_err() {
        return denied_with_context(
            call,
            request,
            ChildCallReasonCode::UnsupportedLocalRuntime,
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
        call_binding: crate::invocation_data::InvocationCallBinding::capture(call),
        data_authority: request.data_authority.clone(),
        manifest_hash: artifact.manifest_hash.clone(),
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
        local_execution_authority: None,
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
                size_bytes: 12,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 5,
                expected_receiver_grants_authority: crate::call::test_grants_authority_identity(7),
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
            additional_exports: Vec::new(),
            runtime_shape: ComponentRuntimeShape::WasmComponent,
            ingress_mode: ChildIngressMode::WitOnly,
            lifecycle_exports: LifecycleExports::AbsentAllowed,
            verification_status: VerificationStatus::Verified,
            provenance_status: ArtifactProvenanceStatus::HistoricalUnknown,
            acquisition_ids: Vec::new(),
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
            data_authority: Some(crate::LocalInvocationData::from_authoritative_source(
                CallId::new("call-child-1").unwrap(),
                Some(ProjectId::new("project-a").unwrap()),
                "project".into(),
                false,
                ObservationId::new("obs:fixture-local-data").unwrap(),
            )),
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
    fn l6_child_project_claims_need_independent_facts_and_tokens_freeze_metadata() {
        let evaluate = |call: &MctCall, request: &ChildCallAuthorityRequest| {
            evaluate_child_call_authority(
                call,
                request,
                &[artifact()],
                &[approval(ChildApprovalState::Approved)],
                &[assignment(ChildAssignmentState::Active)],
                &[instance(ChildInstanceState::Ready)],
            )
        };
        let trusted = request();
        let mut unverified = trusted.clone();
        unverified.data_authority = None;
        assert_eq!(
            evaluate(&call(), &unverified).evaluation.reason_code,
            ChildCallReasonCode::WrongProject
        );
        let mut peer_claim = call();
        peer_claim.origin = CallOrigin::Iroh;
        assert!(
            !evaluate(&peer_claim, &unverified).is_allowed(),
            "authenticated peer hop is not project attestation"
        );
        let token = evaluate(&call(), &trusted)
            .authorized
            .expect("independent local facts admit");
        assert!(token.admit_effect_for_call(&call()).is_some());
        for variant in 0..4 {
            let mut forged = call();
            match variant {
                0 => forged.caller.project_id = None,
                1 => forged.caller.project_id = Some(ProjectId::new("project-b").unwrap()),
                2 => forged.payload_metadata.data_classification = "public".into(),
                _ => forged.payload_metadata.contains_secret_scoped_material = true,
            }
            assert!(!evaluate(&forged, &trusted).is_allowed());
            assert!(token.admit_effect_for_call(&forged).is_none());
        }
        let mut secret_call = call();
        secret_call.payload_metadata.contains_secret_scoped_material = true;
        let mut secret_request = trusted;
        secret_request.data_authority =
            Some(crate::LocalInvocationData::from_authoritative_source(
                secret_call.call_id.clone(),
                Some(ProjectId::new("project-a").unwrap()),
                "project".into(),
                true,
                ObservationId::new("obs:owned-secret-data").unwrap(),
            ));
        let token = evaluate(&secret_call, &secret_request).authorized.unwrap();
        assert!(
            token.admit_effect_for_call(&call()).is_none(),
            "false cannot declassify an admitted secret context"
        );
        assert!(!evaluate(&call(), &secret_request).is_allowed());
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
    fn historical_non_wasm_runtime_shapes_are_readable_but_not_locally_executable() {
        for (wire, shape) in [
            ("\"process_child\"", ComponentRuntimeShape::ProcessChild),
            ("\"jvm_child\"", ComponentRuntimeShape::JvmChild),
            ("\"remote_child\"", ComponentRuntimeShape::RemoteChild),
        ] {
            let decoded: ComponentRuntimeShape = serde_json::from_str(wire).unwrap();
            assert_eq!(decoded, shape);
            assert_eq!(
                LocalChildRuntime::try_from(decoded),
                Err(LocalChildRuntimeError::RetiredOrNonLocal(shape))
            );

            let mut retired_artifact = artifact();
            retired_artifact.runtime_shape = decoded;
            let result = evaluate_child_call_authority(
                &call(),
                &request(),
                &[retired_artifact],
                &[approval(ChildApprovalState::Approved)],
                &[assignment(ChildAssignmentState::Active)],
                &[instance(ChildInstanceState::Ready)],
            );
            assert_eq!(
                result.evaluation.reason_code,
                ChildCallReasonCode::UnsupportedLocalRuntime
            );
            assert!(result.authorized.is_none());
        }

        for (wire, kind) in [
            ("\"process\"", RuntimeKind::Process),
            ("\"jvm_child\"", RuntimeKind::JvmChild),
            ("\"remote_peer\"", RuntimeKind::RemotePeer),
            ("\"internal\"", RuntimeKind::Internal),
        ] {
            let decoded: RuntimeKind = serde_json::from_str(wire).unwrap();
            assert_eq!(decoded, kind);
            assert_eq!(LocalChildRuntime::try_from(decoded), Err(kind));
        }
        assert_eq!(
            LocalChildRuntime::try_from(ComponentRuntimeShape::WasmComponent),
            Ok(LocalChildRuntime::WasmComponent)
        );
        assert_eq!(
            LocalChildRuntime::try_from(RuntimeKind::WasmComponent),
            Ok(LocalChildRuntime::WasmComponent)
        );
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
    fn later_interface_stays_callable_beside_the_primary_export() {
        let exports = component_wit_exports_from_operation_ids(&[
            "patina:mct-test/echo@0.1.0.echo".into(),
            "patina:demo/control@0.1.0.run".into(),
        ]);
        assert_eq!(exports.len(), 2);
        assert_eq!(exports[1].namespace, "patina:demo");
        assert_eq!(exports[1].interface_name, "control");
        assert_eq!(exports[1].function_names, vec!["run".to_string()]);

        let mut artifact = artifact();
        artifact.primary_export = exports[0].clone();
        artifact.additional_exports = vec![exports[1].clone()];
        let mut demo_call = call();
        demo_call.target = OperationTarget {
            namespace: "patina:demo".into(),
            interface_name: "control@0.1.0".into(),
            function_name: "run".into(),
        };
        let result = evaluate_child_call_authority(
            &demo_call,
            &request(),
            &[artifact],
            &[approval(ChildApprovalState::Approved)],
            &[assignment(ChildAssignmentState::Active)],
            &[instance(ChildInstanceState::Ready)],
        );
        assert!(result.is_allowed(), "{:?}", result.evaluation.reason_code);
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

use mct_kernel::{
    AuthorizedChildInvocationId, CandidateRoute, ChildApproval, ChildApprovalId,
    ChildApprovalState, ChildAssignment, ChildAssignmentId, ChildAssignmentState,
    ChildCallAuthorityIds, ChildCallAuthorityRequest, ChildCallEvaluationId, ChildId,
    ChildIngressMode as KernelChildIngressMode, ChildInstance, ChildInstanceId,
    ChildInstanceState as KernelChildInstanceState, ComponentArtifact, ComponentArtifactId,
    ComponentRuntimeShape, ComponentWitExport, LifecycleExports, MctCall, MctNodeId,
    NetworkPathClass, ObservationId, OperationTarget, ProjectId, RuntimeKind, VerificationStatus,
    VisionId, evaluate_child_call_authority,
};
use patina_sdk::manifest::{
    CHILD_MANIFEST_FILE, ChildManifest as SdkChildManifest, ChildManifestError, ChildPackage,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctChildIntegrityMode {
    /// Load standalone MCT child artifacts while computing digests and reporting whether sidecars match.
    AuditOnly,
    /// Require any present `.sha256` sidecar to match, but still load legacy children that lack sidecars.
    VerifyWhenPresent,
    /// Require `.sha256` sidecars beside both the `.wasm` and `.toml` files.
    RequireSidecars,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctChildLoadOptions {
    pub children_dir: PathBuf,
    pub integrity_mode: MctChildIntegrityMode,
}

impl MctChildLoadOptions {
    pub fn new(children_dir: impl Into<PathBuf>) -> Self {
        Self {
            children_dir: children_dir.into(),
            integrity_mode: MctChildIntegrityMode::AuditOnly,
        }
    }

    pub fn verify_when_present(mut self) -> Self {
        self.integrity_mode = MctChildIntegrityMode::VerifyWhenPresent;
        self
    }

    pub fn strict_integrity(mut self) -> Self {
        self.integrity_mode = MctChildIntegrityMode::RequireSidecars;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctChildIngressMode {
    #[default]
    Handle,
    Hybrid,
    WitOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctChildInstanceState {
    Loading,
    Ready,
    Degraded,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctChildFileDigest {
    pub sha256: String,
    pub sidecar_present: bool,
    pub verified: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctLoadedChild {
    pub child_id: ChildId,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub kind: String,
    pub role: Option<String>,
    pub wasm_path: PathBuf,
    pub manifest_path: PathBuf,
    pub wasm_digest: MctChildFileDigest,
    pub manifest_digest: MctChildFileDigest,
    pub artifact_id: String,
    pub ingress_mode: MctChildIngressMode,
    pub allowed_operations: Vec<String>,
    pub requested_toys: Vec<String>,
    pub subscribed_streams: Vec<String>,
    pub relationship_listens: Vec<String>,
    pub wasm_size_bytes: u64,
    pub instance_state: MctChildInstanceState,
}

impl MctLoadedChild {
    pub fn allows_operation_target(&self, target: &OperationTarget) -> bool {
        let operation_id = operation_id_from_target(target);
        match self.ingress_mode {
            MctChildIngressMode::Handle => false,
            MctChildIngressMode::Hybrid => {
                self.allowed_operations.is_empty()
                    || self
                        .allowed_operations
                        .iter()
                        .any(|allowed| allowed == &operation_id)
            }
            MctChildIngressMode::WitOnly => {
                !self.allowed_operations.is_empty()
                    && self
                        .allowed_operations
                        .iter()
                        .any(|allowed| allowed == &operation_id)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctChildLoadFailure {
    pub wasm_path: Option<PathBuf>,
    pub manifest_path: Option<PathBuf>,
    pub safe_message: String,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctChildLoadReport {
    pub children_dir: PathBuf,
    pub discovered: usize,
    pub loaded: usize,
    pub failed: usize,
    pub children: Vec<MctLoadedChild>,
    pub failures: Vec<MctChildLoadFailure>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MctChildRegistry {
    children_by_name: BTreeMap<String, MctLoadedChild>,
}

impl MctChildRegistry {
    pub fn from_loaded(children: Vec<MctLoadedChild>) -> Self {
        Self {
            children_by_name: children
                .into_iter()
                .map(|child| (child.name.clone(), child))
                .collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.children_by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.children_by_name.is_empty()
    }

    pub fn children(&self) -> impl Iterator<Item = &MctLoadedChild> {
        self.children_by_name.values()
    }

    pub fn get(&self, name: &str) -> Option<&MctLoadedChild> {
        self.children_by_name.get(name)
    }

    pub fn local_candidates_for_call(
        &self,
        call: &MctCall,
        local_node_id: MctNodeId,
    ) -> Vec<CandidateRoute> {
        self.children_by_name
            .values()
            .filter(|child| child.instance_state == MctChildInstanceState::Ready)
            .filter(|child| child.allows_operation_target(&call.target))
            .map(|child| CandidateRoute {
                candidate_id: format!("child:{}", child.name),
                node_id: local_node_id.clone(),
                child_id: Some(child.child_id.clone()),
                runtime_kind: RuntimeKind::WasmComponent,
                network_path: NetworkPathClass::Local,
            })
            .collect()
    }

    pub fn authority_projection(
        &self,
        options: MctChildAuthorityProjectionOptions,
    ) -> MctChildAuthorityProjection {
        MctChildAuthorityProjection::from_loaded(self.children(), options)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctChildActivationMode {
    CandidateOnly,
    ApproveAndAssignLocal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctChildAuthorityProjectionOptions {
    pub local_node_id: MctNodeId,
    pub vision_id: VisionId,
    pub project_id: Option<ProjectId>,
    pub policy_revision: u64,
    pub activation_mode: MctChildActivationMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctChildAuthorityProjection {
    pub local_node_id: MctNodeId,
    pub vision_id: VisionId,
    pub project_id: Option<ProjectId>,
    pub policy_revision: u64,
    pub artifacts: Vec<ComponentArtifact>,
    pub approvals: Vec<ChildApproval>,
    pub assignments: Vec<ChildAssignment>,
    pub instances: Vec<ChildInstance>,
}

impl MctChildAuthorityProjection {
    pub fn from_loaded<'a>(
        children: impl IntoIterator<Item = &'a MctLoadedChild>,
        options: MctChildAuthorityProjectionOptions,
    ) -> Self {
        let mut artifacts = Vec::new();
        let mut approvals = Vec::new();
        let mut assignments = Vec::new();
        let mut instances = Vec::new();

        for child in children {
            let artifact = component_artifact_from_loaded_child(child);
            let artifact_verified = artifact.verification_status == VerificationStatus::Verified;
            let approval_id = ChildApprovalId::from(format!("approval:{}", child.name));
            let assignment_id = ChildAssignmentId::from(format!("assignment:{}", child.name));
            let instance_id = ChildInstanceId::from(format!("instance:{}:1", child.name));
            let activated = options.activation_mode
                == MctChildActivationMode::ApproveAndAssignLocal
                && artifact_verified;

            artifacts.push(artifact.clone());
            approvals.push(ChildApproval {
                approval_id: approval_id.clone(),
                artifact_id: artifact.artifact_id.clone(),
                child_name: child.name.clone(),
                artifact_version: child.version.clone(),
                scope_vision_id: Some(options.vision_id.clone()),
                scope_node_id: Some(options.local_node_id.clone()),
                scope_project_id: options.project_id.clone(),
                approval_state: if activated {
                    ChildApprovalState::Approved
                } else {
                    ChildApprovalState::Candidate
                },
                policy_revision: options.policy_revision,
                authority_observation_id: ObservationId::from(format!(
                    "obs:child-approval:{}",
                    child.name
                )),
            });

            if activated {
                assignments.push(ChildAssignment {
                    assignment_id: assignment_id.clone(),
                    approval_id,
                    artifact_id: artifact.artifact_id.clone(),
                    child_name: child.name.clone(),
                    vision_id: options.vision_id.clone(),
                    node_id: Some(options.local_node_id.clone()),
                    project_id: options.project_id.clone(),
                    assignment_state: ChildAssignmentState::Active,
                    pinned_artifact_version: child.version.clone(),
                    assignment_observation_id: ObservationId::from(format!(
                        "obs:child-assignment:{}",
                        child.name
                    )),
                });
            }

            instances.push(ChildInstance {
                instance_id,
                assignment_id,
                artifact_id: artifact.artifact_id.clone(),
                child_name: child.name.clone(),
                generation: 1,
                node_id: options.local_node_id.clone(),
                instance_state: if activated && child.instance_state == MctChildInstanceState::Ready
                {
                    KernelChildInstanceState::Ready
                } else if child.instance_state == MctChildInstanceState::Failed {
                    KernelChildInstanceState::Failed
                } else {
                    KernelChildInstanceState::Loading
                },
                readiness_observation_id: if activated
                    && child.instance_state == MctChildInstanceState::Ready
                {
                    Some(ObservationId::from(format!(
                        "obs:child-ready:{}",
                        child.name
                    )))
                } else {
                    None
                },
                last_lifecycle_observation_id: ObservationId::from(format!(
                    "obs:child-instance:{}:1",
                    child.name
                )),
            });
        }

        Self {
            local_node_id: options.local_node_id,
            vision_id: options.vision_id,
            project_id: options.project_id,
            policy_revision: options.policy_revision,
            artifacts,
            approvals,
            assignments,
            instances,
        }
    }

    pub fn authorized_local_candidates_for_call(&self, call: &MctCall) -> Vec<CandidateRoute> {
        self.instances
            .iter()
            .filter_map(|instance| {
                let ids = ChildCallAuthorityIds {
                    evaluation_id: ChildCallEvaluationId::from(format!(
                        "child-eval:{}:{}",
                        call.call_id, instance.instance_id
                    )),
                    decision_id: mct_kernel::DecisionId::from(format!(
                        "child-decision:{}:{}",
                        call.call_id, instance.instance_id
                    )),
                    observation_id: ObservationId::from(format!(
                        "obs:child-call:{}:{}",
                        call.call_id, instance.instance_id
                    )),
                    authorized_child_invocation_id: AuthorizedChildInvocationId::from(format!(
                        "authorized-child:{}:{}",
                        call.call_id, instance.instance_id
                    )),
                };
                let request = ChildCallAuthorityRequest {
                    instance_id: instance.instance_id.clone(),
                    node_id: self.local_node_id.clone(),
                    ids,
                };
                let evaluation = evaluate_child_call_authority(
                    call,
                    &request,
                    &self.artifacts,
                    &self.approvals,
                    &self.assignments,
                    &self.instances,
                );
                evaluation.is_allowed().then(|| CandidateRoute {
                    candidate_id: format!("child:{}", instance.child_name),
                    node_id: self.local_node_id.clone(),
                    child_id: Some(ChildId::from(instance.child_name.clone())),
                    runtime_kind: runtime_kind_for_instance(instance, &self.artifacts),
                    network_path: NetworkPathClass::Local,
                })
            })
            .collect()
    }
}

pub fn load_children_from_dir(options: MctChildLoadOptions) -> MctChildLoadReport {
    let children_dir = options.children_dir;
    let mut report = MctChildLoadReport {
        children_dir: children_dir.clone(),
        ..MctChildLoadReport::default()
    };

    if !children_dir.exists() {
        return report;
    }

    if children_dir.join(CHILD_MANIFEST_FILE).exists() {
        report.discovered = 1;
        match load_child_package(&children_dir, options.integrity_mode) {
            Ok(child) => {
                report.children.push(child);
                report.loaded = 1;
            }
            Err(failure) => {
                report.failures.push(failure);
                report.failed = 1;
            }
        }
        return report;
    }

    let entries = match fs::read_dir(&children_dir) {
        Ok(entries) => entries,
        Err(error) => {
            report.failed = 1;
            report.failures.push(MctChildLoadFailure {
                wasm_path: None,
                manifest_path: None,
                safe_message: "children directory could not be read".into(),
                detail: error.to_string(),
            });
            return report;
        }
    };

    let mut loaded_by_name = BTreeMap::<String, MctLoadedChild>::new();
    for entry in entries.flatten() {
        let wasm_path = entry.path();
        if wasm_path.extension().and_then(|ext| ext.to_str()) != Some("wasm") {
            continue;
        }
        report.discovered += 1;
        let manifest_path = wasm_path.with_extension("toml");
        match load_child_pair(&wasm_path, &manifest_path, options.integrity_mode) {
            Ok(child) => {
                if loaded_by_name.contains_key(&child.name) {
                    report.failures.push(MctChildLoadFailure {
                        wasm_path: Some(wasm_path),
                        manifest_path: Some(manifest_path),
                        safe_message: "duplicate child name".into(),
                        detail: format!("duplicate child name '{}'", child.name),
                    });
                    continue;
                }
                loaded_by_name.insert(child.name.clone(), child);
            }
            Err(error) => report.failures.push(MctChildLoadFailure {
                wasm_path: Some(wasm_path),
                manifest_path: Some(manifest_path),
                safe_message: error.safe_message(),
                detail: error.to_string(),
            }),
        }
    }

    report.children = loaded_by_name.into_values().collect();
    report.loaded = report.children.len();
    report.failed = report.failures.len();
    report
}

fn load_child_package(
    children_dir: &Path,
    integrity_mode: MctChildIntegrityMode,
) -> Result<MctLoadedChild, MctChildLoadFailure> {
    let package =
        ChildPackage::from_package_dir(children_dir).map_err(|source| MctChildLoadFailure {
            wasm_path: None,
            manifest_path: Some(children_dir.join(CHILD_MANIFEST_FILE)),
            safe_message: "invalid child package".into(),
            detail: source.to_string(),
        })?;
    load_child_pair(
        &package.artifact_path,
        &package.manifest_path,
        integrity_mode,
    )
    .map_err(|error| MctChildLoadFailure {
        wasm_path: Some(package.artifact_path),
        manifest_path: Some(package.manifest_path),
        safe_message: error.safe_message(),
        detail: error.to_string(),
    })
}

pub fn operation_id_from_target(target: &OperationTarget) -> String {
    format!(
        "{}/{}.{}",
        target.namespace, target.interface_name, target.function_name
    )
}

pub fn component_artifact_from_loaded_child(child: &MctLoadedChild) -> ComponentArtifact {
    ComponentArtifact {
        artifact_id: ComponentArtifactId::from(child.artifact_id.clone()),
        child_name: child.name.clone(),
        artifact_version: child.version.clone(),
        content_hash: format!("sha256:{}", child.wasm_digest.sha256),
        manifest_hash: format!("sha256:{}", child.manifest_digest.sha256),
        primary_export: component_export_from_allowed_operations(&child.allowed_operations),
        runtime_shape: ComponentRuntimeShape::WasmComponent,
        ingress_mode: match child.ingress_mode {
            MctChildIngressMode::Handle => KernelChildIngressMode::Handle,
            MctChildIngressMode::Hybrid => KernelChildIngressMode::Hybrid,
            MctChildIngressMode::WitOnly => KernelChildIngressMode::WitOnly,
        },
        lifecycle_exports: LifecycleExports::AbsentAllowed,
        verification_status: if child.wasm_digest.verified && child.manifest_digest.verified {
            VerificationStatus::Verified
        } else {
            VerificationStatus::Rejected
        },
        created_by_observation_id: ObservationId::from(format!("obs:artifact:{}", child.name)),
    }
}

fn component_export_from_allowed_operations(allowed_operations: &[String]) -> ComponentWitExport {
    let Some(first) = allowed_operations.first() else {
        return ComponentWitExport {
            namespace: String::new(),
            interface_name: String::new(),
            version: "0.0.0".into(),
            function_names: Vec::new(),
        };
    };

    let Some((namespace, interface_and_function)) = first.split_once('/') else {
        return fallback_component_export(allowed_operations);
    };
    let Some((interface_with_version, _function_name)) = interface_and_function.rsplit_once('.')
    else {
        return fallback_component_export(allowed_operations);
    };
    let (interface_name, version) = interface_with_version
        .split_once('@')
        .map_or((interface_with_version, "0.0.0"), |(name, version)| {
            (name, version)
        });

    let prefix = format!("{namespace}/{interface_with_version}.");
    let function_names = allowed_operations
        .iter()
        .filter_map(|operation| operation.strip_prefix(&prefix).map(str::to_string))
        .collect();

    ComponentWitExport {
        namespace: namespace.into(),
        interface_name: interface_name.into(),
        version: version.into(),
        function_names,
    }
}

fn fallback_component_export(allowed_operations: &[String]) -> ComponentWitExport {
    ComponentWitExport {
        namespace: String::new(),
        interface_name: String::new(),
        version: "0.0.0".into(),
        function_names: allowed_operations.to_vec(),
    }
}

fn runtime_kind_for_instance(
    instance: &ChildInstance,
    artifacts: &[ComponentArtifact],
) -> RuntimeKind {
    artifacts
        .iter()
        .find(|artifact| artifact.artifact_id == instance.artifact_id)
        .map(|artifact| match artifact.runtime_shape {
            ComponentRuntimeShape::WasmComponent => RuntimeKind::WasmComponent,
            ComponentRuntimeShape::JvmChild => RuntimeKind::JvmChild,
            ComponentRuntimeShape::ProcessChild => RuntimeKind::Process,
            ComponentRuntimeShape::RemoteChild => RuntimeKind::RemotePeer,
        })
        .unwrap_or(RuntimeKind::WasmComponent)
}

#[derive(Debug, Error)]
enum MctChildLoadError {
    #[error("missing child manifest at {path}")]
    MissingManifest { path: PathBuf },
    #[error("read child file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("child file {path} is missing required hash sidecar {sidecar}")]
    MissingHashSidecar { path: PathBuf, sidecar: PathBuf },
    #[error("hash mismatch for child file {path}")]
    HashMismatch { path: PathBuf },
    #[error("parse child manifest {path}: {source}")]
    ParseManifest {
        path: PathBuf,
        #[source]
        source: ChildManifestError,
    },
}

impl MctChildLoadError {
    fn safe_message(&self) -> String {
        match self {
            Self::MissingManifest { .. } => "missing child manifest".into(),
            Self::MissingHashSidecar { .. } => "missing child integrity sidecar".into(),
            Self::HashMismatch { .. } => "child integrity check failed".into(),
            Self::ParseManifest { .. } => "invalid child manifest".into(),
            Self::ReadFile { .. } => "child file could not be read".into(),
        }
    }
}

fn load_child_pair(
    wasm_path: &Path,
    manifest_path: &Path,
    integrity_mode: MctChildIntegrityMode,
) -> Result<MctLoadedChild, MctChildLoadError> {
    if !manifest_path.exists() {
        return Err(MctChildLoadError::MissingManifest {
            path: manifest_path.to_path_buf(),
        });
    }

    let wasm_bytes = fs::read(wasm_path).map_err(|source| MctChildLoadError::ReadFile {
        path: wasm_path.to_path_buf(),
        source,
    })?;
    let manifest_bytes = fs::read(manifest_path).map_err(|source| MctChildLoadError::ReadFile {
        path: manifest_path.to_path_buf(),
        source,
    })?;
    let wasm_digest = digest_bytes_with_sidecar(wasm_path, &wasm_bytes, integrity_mode)?;
    let manifest_digest =
        digest_bytes_with_sidecar(manifest_path, &manifest_bytes, integrity_mode)?;
    let manifest_text = String::from_utf8_lossy(&manifest_bytes);
    let manifest = SdkChildManifest::from_toml_str(&manifest_text).map_err(|source| {
        MctChildLoadError::ParseManifest {
            path: manifest_path.to_path_buf(),
            source,
        }
    })?;
    let artifact_id = format!("sha256:{}", wasm_digest.sha256);

    Ok(MctLoadedChild {
        child_id: ChildId::from(manifest.name.clone()),
        name: manifest.name,
        version: manifest.version,
        description: manifest.description,
        kind: manifest.kind,
        role: manifest.role,
        wasm_path: wasm_path.to_path_buf(),
        manifest_path: manifest_path.to_path_buf(),
        wasm_digest,
        manifest_digest,
        artifact_id,
        ingress_mode: mct_ingress_mode_from_sdk(manifest.ingress_mode),
        allowed_operations: manifest.contract.allow_operations,
        requested_toys: manifest.needs.toys,
        subscribed_streams: Vec::new(),
        relationship_listens: manifest.relationships.listens,
        wasm_size_bytes: wasm_bytes.len() as u64,
        instance_state: MctChildInstanceState::Ready,
    })
}

fn mct_ingress_mode_from_sdk(mode: patina_sdk::manifest::ChildIngressMode) -> MctChildIngressMode {
    match mode {
        patina_sdk::manifest::ChildIngressMode::Handle => MctChildIngressMode::Handle,
        patina_sdk::manifest::ChildIngressMode::Hybrid => MctChildIngressMode::Hybrid,
        patina_sdk::manifest::ChildIngressMode::WitOnly => MctChildIngressMode::WitOnly,
    }
}

fn digest_bytes_with_sidecar(
    path: &Path,
    bytes: &[u8],
    integrity_mode: MctChildIntegrityMode,
) -> Result<MctChildFileDigest, MctChildLoadError> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    let sidecar = hash_sidecar_path(path);
    if !sidecar.exists() {
        if integrity_mode == MctChildIntegrityMode::RequireSidecars {
            return Err(MctChildLoadError::MissingHashSidecar {
                path: path.to_path_buf(),
                sidecar,
            });
        }
        return Ok(MctChildFileDigest {
            sha256: actual,
            sidecar_present: false,
            verified: false,
        });
    }

    let expected = fs::read_to_string(&sidecar).map_err(|source| MctChildLoadError::ReadFile {
        path: sidecar.clone(),
        source,
    })?;
    let expected = expected.trim();
    if expected != actual {
        if integrity_mode == MctChildIntegrityMode::AuditOnly {
            return Ok(MctChildFileDigest {
                sha256: actual,
                sidecar_present: true,
                verified: false,
            });
        }
        return Err(MctChildLoadError::HashMismatch {
            path: path.to_path_buf(),
        });
    }
    Ok(MctChildFileDigest {
        sha256: actual,
        sidecar_present: true,
        verified: true,
    })
}

fn hash_sidecar_path(path: &Path) -> PathBuf {
    let mut sidecar: OsString = path.as_os_str().to_os_string();
    sidecar.push(".sha256");
    PathBuf::from(sidecar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mct_kernel::{
        AuthorityContextSnapshot, CallOrigin, CallerIdentity, PayloadMetadata, ProjectId, SpanId,
        Timestamp, TraceContext, TraceId, UserId, VisionId,
    };

    fn write_child(dir: &Path, stem: &str, name: &str, ingress: &str, allow: &[&str]) {
        let wasm_path = dir.join(format!("{stem}.wasm"));
        let manifest_path = dir.join(format!("{stem}.toml"));
        fs::write(&wasm_path, format!("wasm-{name}")).unwrap();
        let allow = allow
            .iter()
            .map(|operation| format!("\"{operation}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            manifest_path,
            format!(
                r#"[child]
name = "{name}"
version = "0.1.0"
description = "test child"
kind = "child"
role = "app"

[child.ingress]
mode = "{ingress}"

[child.contract]
allow = [{allow}]

[needs]
toys = ["logging"]

[relationships]
listens = ["events.changed"]
"#
            ),
        )
        .unwrap();
    }

    fn write_sidecars(dir: &Path, stem: &str) {
        for ext in ["wasm", "toml"] {
            let path = dir.join(format!("{stem}.{ext}"));
            let bytes = fs::read(&path).unwrap();
            fs::write(
                hash_sidecar_path(&path),
                format!("{:x}", Sha256::digest(bytes)),
            )
            .unwrap();
        }
    }

    fn write_child_package(dir: &Path, artifact_name: &str, name: &str) {
        let artifact_path = dir.join(artifact_name);
        if let Some(parent) = artifact_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&artifact_path, format!("wasm-{name}")).unwrap();
        fs::write(
            dir.join(CHILD_MANIFEST_FILE),
            format!(
                r#"[child]
name = "{name}"
version = "0.4.0"
description = "test package child"
kind = "child"
role = "app"

[child.ingress]
mode = "wit-only"

[child.artifact]
wasm = "{artifact_name}"

[child.contract]
allow = ["patina:slate/control@0.1.0.list-work"]

[needs]
toys = ["logging", "measure"]

[relationships]
listens = ["events.changed"]
"#
            ),
        )
        .unwrap();
    }

    fn call_for(operation: OperationTarget) -> MctCall {
        MctCall {
            call_id: mct_kernel::CallId::from("call-child-route"),
            caller: CallerIdentity {
                node_id: MctNodeId::from("mother-a"),
                user_id: Option::<UserId>::None,
                vision_id: VisionId::from("vision-a"),
                project_id: Option::<ProjectId>::None,
            },
            target: operation,
            payload_metadata: PayloadMetadata {
                data_classification: "public".into(),
                approximate_size_bytes: 2,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 1,
                grants_revision: 1,
                vision_policy_revision: 1,
            },
            deadline: Timestamp::from("2026-05-31T00:01:00Z"),
            trace_context: TraceContext {
                trace_id: TraceId::from("trace-child-route"),
                span_id: SpanId::from("span-child-route"),
            },
            origin: CallOrigin::Cli,
        }
    }

    #[test]
    fn loads_standalone_wasm_children_from_directory() {
        let dir = tempfile::tempdir().unwrap();
        write_child(
            dir.path(),
            "slate-manager",
            "slate-manager",
            "wit-only",
            &["patina:slate/control@0.1.0.list-work"],
        );
        write_sidecars(dir.path(), "slate-manager");
        write_child(
            dir.path(),
            "watch-null-sink",
            "watch-null-sink",
            "handle",
            &[],
        );

        let report = load_children_from_dir(MctChildLoadOptions::new(dir.path()));

        assert_eq!(report.discovered, 2);
        assert_eq!(report.loaded, 2);
        assert_eq!(report.failed, 0);
        let slate = report
            .children
            .iter()
            .find(|child| child.name == "slate-manager")
            .unwrap();
        assert_eq!(slate.ingress_mode, MctChildIngressMode::WitOnly);
        assert!(slate.wasm_digest.verified);
        assert_eq!(slate.requested_toys, vec!["logging"]);
        assert_eq!(slate.relationship_listens, vec!["events.changed"]);
        let watch = report
            .children
            .iter()
            .find(|child| child.name == "watch-null-sink")
            .unwrap();
        assert!(!watch.wasm_digest.sidecar_present);
        assert_eq!(watch.instance_state, MctChildInstanceState::Ready);
    }

    #[test]
    fn loads_sdk_child_package_from_directory_manifest() {
        let dir = tempfile::tempdir().unwrap();
        write_child_package(
            dir.path(),
            "target/wasm32-wasip1/release/patina_ai_child_slate_manager.wasm",
            "slate-manager",
        );

        let report = load_children_from_dir(MctChildLoadOptions::new(dir.path()));

        assert_eq!(report.discovered, 1);
        assert_eq!(report.loaded, 1);
        assert_eq!(report.failed, 0);
        let child = report.children.first().unwrap();
        assert_eq!(child.name, "slate-manager");
        assert_eq!(child.version, "0.4.0");
        assert_eq!(child.ingress_mode, MctChildIngressMode::WitOnly);
        assert_eq!(
            child.wasm_path.file_name().unwrap(),
            "patina_ai_child_slate_manager.wasm"
        );
        assert_eq!(
            child.allowed_operations,
            vec!["patina:slate/control@0.1.0.list-work"]
        );
        assert_eq!(child.requested_toys, vec!["logging", "measure"]);
        assert_eq!(child.relationship_listens, vec!["events.changed"]);
    }

    #[test]
    fn sdk_child_package_uses_manifest_declared_artifact() {
        let dir = tempfile::tempdir().unwrap();
        write_child_package(dir.path(), "one.wasm", "slate-manager");
        fs::write(dir.path().join("two.wasm"), "wasm-two").unwrap();

        let report = load_children_from_dir(MctChildLoadOptions::new(dir.path()));

        assert_eq!(report.discovered, 1);
        assert_eq!(report.loaded, 1);
        assert_eq!(report.failed, 0);
        assert_eq!(report.children[0].wasm_path, dir.path().join("one.wasm"));
    }

    #[test]
    fn sdk_child_package_rejects_missing_artifact_declaration() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("one.wasm"), "wasm-one").unwrap();
        fs::write(
            dir.path().join(CHILD_MANIFEST_FILE),
            r#"[child]
name = "slate-manager"
version = "0.4.0"
kind = "child"
"#,
        )
        .unwrap();

        let report = load_children_from_dir(MctChildLoadOptions::new(dir.path()));

        assert_eq!(report.discovered, 1);
        assert_eq!(report.loaded, 0);
        assert_eq!(report.failed, 1);
        assert_eq!(report.failures[0].safe_message, "invalid child package");
    }

    #[test]
    fn strict_integrity_requires_hash_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        write_child(dir.path(), "no-sidecar", "no-sidecar", "handle", &[]);

        let report =
            load_children_from_dir(MctChildLoadOptions::new(dir.path()).strict_integrity());

        assert_eq!(report.discovered, 1);
        assert_eq!(report.loaded, 0);
        assert_eq!(report.failed, 1);
        assert_eq!(
            report.failures[0].safe_message,
            "missing child integrity sidecar"
        );
    }

    #[test]
    fn duplicate_child_names_are_failed_closed() {
        let dir = tempfile::tempdir().unwrap();
        write_child(dir.path(), "alpha-a", "alpha", "handle", &[]);
        write_child(dir.path(), "alpha-b", "alpha", "handle", &[]);

        let report = load_children_from_dir(MctChildLoadOptions::new(dir.path()));

        assert_eq!(report.discovered, 2);
        assert_eq!(report.loaded, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.failures[0].safe_message, "duplicate child name");
    }

    #[test]
    fn registry_routes_only_allowlisted_ready_children() {
        let dir = tempfile::tempdir().unwrap();
        write_child(
            dir.path(),
            "slate-manager",
            "slate-manager",
            "wit-only",
            &["patina:slate/control@0.1.0.list-work"],
        );
        write_child(dir.path(), "handle-child", "handle-child", "handle", &[]);
        let report = load_children_from_dir(MctChildLoadOptions::new(dir.path()));
        let registry = MctChildRegistry::from_loaded(report.children);
        let call = call_for(OperationTarget {
            namespace: "patina:slate".into(),
            interface_name: "control@0.1.0".into(),
            function_name: "list-work".into(),
        });

        let candidates = registry.local_candidates_for_call(&call, MctNodeId::from("mother-a"));

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].child_id, Some(ChildId::from("slate-manager")));
        assert_eq!(candidates[0].runtime_kind, RuntimeKind::WasmComponent);
    }

    #[test]
    fn authority_projection_authorizes_only_explicit_local_approvals() {
        let dir = tempfile::tempdir().unwrap();
        write_child(
            dir.path(),
            "slate-manager",
            "slate-manager",
            "wit-only",
            &[
                "patina:slate/control@0.1.0.list-work",
                "patina:slate/control@0.1.0.complete-work",
            ],
        );
        write_sidecars(dir.path(), "slate-manager");
        let report = load_children_from_dir(MctChildLoadOptions::new(dir.path()));
        let registry = MctChildRegistry::from_loaded(report.children);
        let call = call_for(OperationTarget {
            namespace: "patina:slate".into(),
            interface_name: "control@0.1.0".into(),
            function_name: "list-work".into(),
        });

        let candidate_only = registry.authority_projection(MctChildAuthorityProjectionOptions {
            local_node_id: MctNodeId::from("mother-a"),
            vision_id: VisionId::from("vision-a"),
            project_id: None,
            policy_revision: 1,
            activation_mode: MctChildActivationMode::CandidateOnly,
        });
        assert!(
            candidate_only
                .authorized_local_candidates_for_call(&call)
                .is_empty()
        );
        assert_eq!(
            candidate_only.approvals[0].approval_state,
            ChildApprovalState::Candidate
        );
        assert!(candidate_only.assignments.is_empty());

        let approved = registry.authority_projection(MctChildAuthorityProjectionOptions {
            local_node_id: MctNodeId::from("mother-a"),
            vision_id: VisionId::from("vision-a"),
            project_id: None,
            policy_revision: 1,
            activation_mode: MctChildActivationMode::ApproveAndAssignLocal,
        });
        let candidates = approved.authorized_local_candidates_for_call(&call);

        assert_eq!(
            approved.artifacts[0].verification_status,
            VerificationStatus::Verified
        );
        assert_eq!(
            approved.approvals[0].approval_state,
            ChildApprovalState::Approved
        );
        assert_eq!(
            approved.assignments[0].assignment_state,
            ChildAssignmentState::Active
        );
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].child_id, Some(ChildId::from("slate-manager")));
    }

    #[test]
    fn authority_projection_rejects_unverified_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        write_child(
            dir.path(),
            "no-sidecar",
            "no-sidecar",
            "wit-only",
            &["patina:slate/control@0.1.0.list-work"],
        );
        let report = load_children_from_dir(MctChildLoadOptions::new(dir.path()));
        let registry = MctChildRegistry::from_loaded(report.children);
        let projection = registry.authority_projection(MctChildAuthorityProjectionOptions {
            local_node_id: MctNodeId::from("mother-a"),
            vision_id: VisionId::from("vision-a"),
            project_id: None,
            policy_revision: 1,
            activation_mode: MctChildActivationMode::ApproveAndAssignLocal,
        });
        let call = call_for(OperationTarget {
            namespace: "patina:slate".into(),
            interface_name: "control@0.1.0".into(),
            function_name: "list-work".into(),
        });

        assert_eq!(
            projection.artifacts[0].verification_status,
            VerificationStatus::Rejected
        );
        assert_eq!(
            projection.approvals[0].approval_state,
            ChildApprovalState::Candidate
        );
        assert!(projection.assignments.is_empty());
        assert!(
            projection
                .authorized_local_candidates_for_call(&call)
                .is_empty()
        );
    }
}

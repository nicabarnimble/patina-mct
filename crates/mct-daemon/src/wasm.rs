use crate::{
    children::{MctLoadedChild, operation_id_from_target},
    state::MctRuntimeStateStore,
    toy::{MctToyAdapterOutcome, MctToyAdapterRegistry, MctToyCallIds},
    wit_values::{lift_component_results_to_json, lower_typed_args_for_component},
};
use mct_kernel::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};
use thiserror::Error;
use wasmtime::{
    AsContext, AsContextMut, Config, Engine, Store, StoreContextMut, StoreLimits,
    StoreLimitsBuilder, component, component::ResourceTable,
};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctWasmComponentInvocationIds {
    pub started_observation_id: ObservationId,
    pub completed_observation_id: ObservationId,
    pub audit_ref: AuditRef,
    pub started_at: Timestamp,
    pub completed_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctWasmComponentInvocationReport {
    pub result: MctResult,
    pub returned_s32: i32,
    pub observations: Vec<MctObservation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctWitComponentInvocationReport {
    pub result: MctResult,
    pub output_json: Value,
    pub observations: Vec<MctObservation>,
    pub produced_messages: Vec<MctWitProducedMessage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctWitProducedMessage {
    pub target_operation: String,
    pub topic: String,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
    pub metadata: Vec<(String, String)>,
    pub offset: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MctWitWatchCallOutWireEvent {
    pub watcher: String,
    #[serde(alias = "stream-name")]
    pub stream: String,
    pub change_kind: String,
    pub absolute_path: String,
    pub relative_path: String,
    pub size_bytes: Option<u64>,
    pub modified_unix_ms: Option<u64>,
    pub sha256: Option<String>,
    pub detected_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctWitWatchMessageAdmission {
    pub event_classes: BTreeSet<WatchEventClass>,
    pub max_events_per_batch: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum MctWitWatchMessageRefusal {
    #[error("watch_target_refused")]
    Target,
    #[error("watch_topic_refused")]
    Topic,
    #[error("watch_content_type_refused")]
    ContentType,
    #[error("watch_message_bound_refused")]
    MessageBound,
    #[error("watch_batch_capacity_refused")]
    BatchCapacity,
    #[error("watch_event_shape_refused")]
    EventShape,
    #[error("watch_event_class_refused")]
    EventClass,
    #[error("watch_safe_path_refused")]
    SafePath,
    #[error("watch_legacy_path_equality_refused")]
    LegacyPathEquality,
}

pub fn validate_wit_watch_message_admission(
    admission: &MctWitWatchMessageAdmission,
    target_operation: &str,
    topic: &str,
    content_type: Option<&str>,
    data: &[u8],
    metadata_pair_count: usize,
    admitted_event_count: usize,
) -> Result<MctWitWatchCallOutWireEvent, MctWitWatchMessageRefusal> {
    let operation_interface = target_operation
        .rsplit_once('.')
        .map(|(interface, _)| interface)
        .ok_or(MctWitWatchMessageRefusal::Target)?;
    if content_type != Some("application/json") {
        return Err(MctWitWatchMessageRefusal::ContentType);
    }
    if data.len() > MCT_WATCH_MESSAGE_MAX_BYTES
        || metadata_pair_count > MCT_WATCH_METADATA_PAIRS_MAX
    {
        return Err(MctWitWatchMessageRefusal::MessageBound);
    }
    if admitted_event_count >= admission.max_events_per_batch as usize
        || admitted_event_count >= MCT_WATCH_MAX_EVENTS_PER_BATCH as usize
    {
        return Err(MctWitWatchMessageRefusal::BatchCapacity);
    }
    let wire: MctWitWatchCallOutWireEvent =
        serde_json::from_slice(data).map_err(|_| MctWitWatchMessageRefusal::EventShape)?;
    let event_class = match (topic, wire.change_kind.as_str()) {
        ("file-created", "created") => WatchEventClass::Created,
        ("file-modified", "modified") => WatchEventClass::Modified,
        ("file-deleted", "deleted") => WatchEventClass::Deleted,
        ("file-created" | "file-modified" | "file-deleted", _) => {
            return Err(MctWitWatchMessageRefusal::EventClass);
        }
        _ => return Err(MctWitWatchMessageRefusal::Topic),
    };
    if !admission.event_classes.contains(&event_class) {
        return Err(MctWitWatchMessageRefusal::EventClass);
    }
    validate_safe_watch_relative_path(&wire.relative_path)
        .map_err(|_| MctWitWatchMessageRefusal::SafePath)?;
    let compatibility = validate_legacy_watch_paths(
        operation_interface,
        &wire.absolute_path,
        &wire.relative_path,
    )
    .map_err(|_| MctWitWatchMessageRefusal::Target)?;
    if compatibility != LegacyWatchCompatibilityValidation::Matched {
        return Err(MctWitWatchMessageRefusal::LegacyPathEquality);
    }
    Ok(wire)
}

#[derive(Debug)]
pub struct MctWasmToyHostImport {
    pub import_name: String,
    pub authorized_toy_call: AuthorizedToyCall,
    pub ids: MctToyCallIds,
}

#[derive(Debug)]
pub struct MctWasmComponentToyInvocation {
    pub component_path: PathBuf,
    pub export_name: String,
    pub toy_registry: MctToyAdapterRegistry,
    pub toy_imports: Vec<MctWasmToyHostImport>,
    pub ids: MctWasmComponentInvocationIds,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctWasmComponentDiagnosticIds {
    pub observation_id: ObservationId,
    pub observed_at: Timestamp,
}

#[derive(Debug)]
pub struct MctWitToyHostAdapter {
    pub authorized_toy_call: AuthorizedToyCall,
    pub observation_id_prefix: String,
    pub observed_at: Timestamp,
}

#[derive(Debug)]
pub struct MctWitHostImportAdapters {
    pub toy_registry: MctToyAdapterRegistry,
    pub logging: Option<MctWitToyHostAdapter>,
    pub measure: Option<MctWitToyHostAdapter>,
    pub git: Option<MctWitToyHostAdapter>,
    pub keyvalue: Option<MctWitKeyvalueHostAdapter>,
    pub messaging: Option<MctWitMessagingHostAdapter>,
    pub wasi: Option<MctWasiHostConfig>,
}

#[derive(Debug)]
pub struct MctWitKeyvalueHostAdapter {
    pub get: MctWitToyHostAdapter,
    pub set: MctWitToyHostAdapter,
    pub state_path: PathBuf,
    pub bucket_identifier: String,
    pub bucket_resource_id: String,
}

#[derive(Debug)]
pub struct MctWitMessagingHostAdapter {
    pub toy: MctWitToyHostAdapter,
    pub watch_admission: MctWitWatchMessageAdmission,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctWasiHostConfig {
    pub preopens: Vec<MctWasiPreopen>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctWasiPreopen {
    pub host_path: PathBuf,
    pub guest_path: String,
    pub access: MctWasiPreopenAccess,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MctWasiPreopenAccess {
    ReadOnly,
    ReadWrite,
}

pub const DEFAULT_WASM_MEMORY_LIMIT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctWasmHostConfig {
    pub memory_limit_bytes: usize,
}

impl MctWasmHostConfig {
    pub fn default_local() -> Self {
        Self {
            memory_limit_bytes: DEFAULT_WASM_MEMORY_LIMIT_BYTES,
        }
    }
}

impl MctWitHostImportAdapters {
    pub fn none() -> Self {
        Self {
            toy_registry: MctToyAdapterRegistry::new(),
            logging: None,
            measure: None,
            git: None,
            keyvalue: None,
            messaging: None,
            wasi: None,
        }
    }
}

struct MctWasmEmptyHostState {
    limits: StoreLimits,
}

struct MctWasmHostState {
    toy_registry: MctToyAdapterRegistry,
    call: MctCall,
    toy_observations: Vec<MctObservation>,
    limits: StoreLimits,
}

struct MctWitHostState {
    toy_registry: MctToyAdapterRegistry,
    call: MctCall,
    toy_observations: Vec<MctObservation>,
    logging: Option<MctWitToyHostAdapter>,
    measure: Option<MctWitToyHostAdapter>,
    git: Option<MctWitToyHostAdapter>,
    keyvalue: Option<MctWitKeyvalueHostAdapter>,
    messaging: Option<MctWitMessagingHostAdapter>,
    messaging_clients: BTreeMap<u32, String>,
    next_resource_rep: u32,
    produced_messages: Vec<MctWitProducedMessage>,
    wasi_ctx: WasiCtx,
    wasi_table: ResourceTable,
    next_toy_call_index: u64,
    limits: StoreLimits,
}

impl WasiView for MctWitHostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.wasi_table,
        }
    }
}

impl MctWitHostState {
    fn call_toy(
        &mut self,
        adapter: &MctWitToyHostAdapter,
        input_json: &Value,
    ) -> crate::toy::MctToyCallReport {
        self.next_toy_call_index += 1;
        let ids = MctToyCallIds {
            started_observation_id: ObservationId::new(format!(
                "{}:{}:started",
                adapter.observation_id_prefix, self.next_toy_call_index
            ))
            .expect("string ID literal/generated value must be non-empty"),
            completed_observation_id: ObservationId::new(format!(
                "{}:{}:completed",
                adapter.observation_id_prefix, self.next_toy_call_index
            ))
            .expect("string ID literal/generated value must be non-empty"),
            started_at: adapter.observed_at.clone(),
            completed_at: adapter.observed_at.clone(),
        };
        let mut report = self.toy_registry.call_authorized_toy(
            &adapter.authorized_toy_call,
            &self.call,
            &input_json.to_string(),
            ids,
        );
        self.toy_observations.append(&mut report.observations);
        report
    }
}

fn build_wasi_ctx(
    config: Option<&MctWasiHostConfig>,
) -> Result<WasiCtx, MctWasmComponentRuntimeError> {
    let mut builder = WasiCtxBuilder::new();
    builder.allow_blocking_current_thread(true);
    if let Some(config) = config {
        let mut guest_paths = BTreeSet::new();
        for preopen in &config.preopens {
            validate_wasi_preopen(preopen, &mut guest_paths)?;
            let (dir_perms, file_perms) = match preopen.access {
                MctWasiPreopenAccess::ReadOnly => (DirPerms::READ, FilePerms::READ),
                MctWasiPreopenAccess::ReadWrite => (DirPerms::all(), FilePerms::all()),
            };
            builder
                .preopened_dir(
                    &preopen.host_path,
                    &preopen.guest_path,
                    dir_perms,
                    file_perms,
                )
                .map_err(|error| {
                    MctWasmComponentRuntimeError::Configure(format!(
                        "configure WASI preopen '{}'=>'{}': {error}",
                        preopen.host_path.display(),
                        preopen.guest_path
                    ))
                })?;
        }
    }
    Ok(builder.build())
}

fn validate_wasi_preopen(
    preopen: &MctWasiPreopen,
    guest_paths: &mut BTreeSet<String>,
) -> Result<(), MctWasmComponentRuntimeError> {
    if !preopen.host_path.is_absolute() {
        return Err(MctWasmComponentRuntimeError::Configure(format!(
            "WASI preopen host path '{}' must be absolute",
            preopen.host_path.display()
        )));
    }
    if !preopen.host_path.is_dir() {
        return Err(MctWasmComponentRuntimeError::Configure(format!(
            "WASI preopen host path '{}' must be an existing directory",
            preopen.host_path.display()
        )));
    }
    if !preopen.guest_path.starts_with('/') {
        return Err(MctWasmComponentRuntimeError::Configure(format!(
            "WASI preopen guest path '{}' must be absolute",
            preopen.guest_path
        )));
    }
    let guest_path = Path::new(&preopen.guest_path);
    for component in guest_path.components() {
        match component {
            std::path::Component::RootDir | std::path::Component::Normal(_) => {}
            std::path::Component::CurDir
            | std::path::Component::ParentDir
            | std::path::Component::Prefix(_) => {
                return Err(MctWasmComponentRuntimeError::Configure(format!(
                    "WASI preopen guest path '{}' must not contain relative components",
                    preopen.guest_path
                )));
            }
        }
    }
    if !guest_paths.insert(preopen.guest_path.clone()) {
        return Err(MctWasmComponentRuntimeError::Configure(format!(
            "duplicate WASI preopen guest path '{}'",
            preopen.guest_path
        )));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum MctWasmComponentRuntimeError {
    #[error("configure wasm component runtime: {0}")]
    Configure(String),
    #[error("load wasm component {path}: {message}")]
    Load { path: PathBuf, message: String },
    #[error("instantiate wasm component {path}: {message}")]
    Instantiate { path: PathBuf, message: String },
    #[error("wasm component {path} exceeded resource limit {memory_limit_bytes} bytes: {message}")]
    ResourceLimit {
        path: PathBuf,
        memory_limit_bytes: usize,
        message: String,
    },
    #[error("missing wasm component export '{export_name}' in {path}")]
    MissingExport { path: PathBuf, export_name: String },
    #[error("invalid WIT operation '{operation_id}': {message}")]
    InvalidWitOperation {
        operation_id: String,
        message: String,
    },
    #[error(
        "authorized child '{authorized_child_name}' does not match loaded child '{loaded_child_name}'"
    )]
    AuthorizedChildMismatch {
        authorized_child_name: String,
        loaded_child_name: String,
    },
    #[error("child '{child_name}' contract does not allow WIT operation '{operation_id}'")]
    WitOperationNotAllowed {
        child_name: String,
        operation_id: String,
    },
    #[error("unsupported WIT host import '{import_name}.{item_name}' for {path}: {message}")]
    UnsupportedWitHostImport {
        path: PathBuf,
        import_name: String,
        item_name: String,
        message: String,
    },
    #[error("missing WIT component operation '{operation_id}' in {path}")]
    MissingWitOperation { path: PathBuf, operation_id: String },
    #[error("convert WIT component values for '{operation_id}' in {path}: {message}")]
    WitValueConversion {
        path: PathBuf,
        operation_id: String,
        message: String,
    },
    #[error("call wasm component export '{export_name}' in {path}: {message}")]
    Call {
        path: PathBuf,
        export_name: String,
        message: String,
    },
    #[error("wasm component export '{export_name}' returned unexpected value")]
    UnexpectedResult { export_name: String },
}

pub fn wasm_component_runtime_error_observation(
    error: &MctWasmComponentRuntimeError,
    call: &MctCall,
    authorized: &AuthorizedChildInvocation,
    ids: MctWasmComponentDiagnosticIds,
) -> Option<MctObservation> {
    let (diagnostic_kind, resource_id, detail_ref) = match error {
        MctWasmComponentRuntimeError::Call {
            path, export_name, ..
        } => (
            AdapterDiagnosticKind::WasmTrap,
            path.display().to_string(),
            format!(
                "authorized_child_invocation:{}:export:{export_name}",
                authorized.authorized_child_invocation_id()
            ),
        ),
        MctWasmComponentRuntimeError::MissingExport { path, export_name } => (
            AdapterDiagnosticKind::WasmMissingExport,
            path.display().to_string(),
            format!(
                "authorized_child_invocation:{}:missing_export:{export_name}",
                authorized.authorized_child_invocation_id()
            ),
        ),
        MctWasmComponentRuntimeError::MissingWitOperation { path, operation_id } => (
            AdapterDiagnosticKind::WasmMissingExport,
            path.display().to_string(),
            format!(
                "authorized_child_invocation:{}:missing_wit_operation:{operation_id}",
                authorized.authorized_child_invocation_id()
            ),
        ),
        MctWasmComponentRuntimeError::UnsupportedWitHostImport {
            path,
            import_name,
            item_name,
            ..
        } => (
            AdapterDiagnosticKind::WasmMissingHostImport,
            path.display().to_string(),
            format!(
                "authorized_child_invocation:{}:unsupported_wit_host_import:{import_name}.{item_name}",
                authorized.authorized_child_invocation_id()
            ),
        ),
        MctWasmComponentRuntimeError::WitValueConversion {
            path, operation_id, ..
        } => (
            AdapterDiagnosticKind::WasmValueConversionFailure,
            path.display().to_string(),
            format!(
                "authorized_child_invocation:{}:wit_value_conversion:{operation_id}",
                authorized.authorized_child_invocation_id()
            ),
        ),
        _ => return None,
    };
    Some(adapter_diagnostic_observation(
        AdapterDiagnosticObservationInput {
            observation_id: ids.observation_id,
            observed_at: ids.observed_at,
            diagnostic_kind,
            trace: ObservationTraceRef {
                trace_id: call.trace_context.trace_id.clone(),
                span_id: Some(call.trace_context.span_id.clone()),
                parent_span_id: None,
                external_trace_id: None,
            },
            call_id: Some(call.call_id.clone()),
            decision_id: Some(authorized.authority_decision_id().clone()),
            subject_id: Some(authorized.child_name().to_owned()),
            resource_id: Some(resource_id),
            policy_revision: Some(call.authority_context.policy_revision),
            grants_revision: Some(call.authority_context.grants_revision),
            detail_ref: Some(detail_ref),
        },
    ))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctWitResolvedOperation {
    pub operation_id: String,
    pub interface: String,
    pub function: String,
}

pub fn wit_operation_id_from_target(target: &OperationTarget) -> String {
    operation_id_from_target(target)
}

pub fn resolve_wit_operation_target(
    target: &OperationTarget,
) -> Result<MctWitResolvedOperation, MctWasmComponentRuntimeError> {
    resolve_wit_operation_id(&wit_operation_id_from_target(target))
}

const WIT_OPERATION_ID_SHAPE: &str = "<package>:<interface-path>.<function>";

fn is_valid_wit_symbol_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn is_valid_wit_interface_version(version: &str) -> bool {
    let mut parts = version.split('.');
    let (Some(major), Some(minor), Some(patch)) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    parts.next().is_none()
        && [major, minor, patch]
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

fn invalid_wit_operation(
    operation_id: impl Into<String>,
    message: impl Into<String>,
) -> MctWasmComponentRuntimeError {
    MctWasmComponentRuntimeError::InvalidWitOperation {
        operation_id: operation_id.into(),
        message: format!("{}; expected {WIT_OPERATION_ID_SHAPE}", message.into()),
    }
}

fn validate_wit_interface_identity(
    interface: &str,
    operation_id: &str,
) -> Result<(), MctWasmComponentRuntimeError> {
    let Some((package, interface_path)) = interface.split_once(':') else {
        return Err(invalid_wit_operation(
            operation_id,
            "operation id must include '<package>:<interface-path>' before function token",
        ));
    };
    if package.is_empty()
        || interface_path.is_empty()
        || package.contains(':')
        || interface_path.contains(':')
    {
        return Err(invalid_wit_operation(
            operation_id,
            "operation id has malformed package/interface section",
        ));
    }
    if !is_valid_wit_symbol_token(package) {
        return Err(invalid_wit_operation(
            operation_id,
            "operation package token contains unsupported characters",
        ));
    }

    let segments = interface_path.split('/').collect::<Vec<_>>();
    if segments.is_empty() || segments.iter().any(|segment| segment.is_empty()) {
        return Err(invalid_wit_operation(
            operation_id,
            "operation interface path must contain non-empty '/' segments",
        ));
    }
    for (index, segment) in segments.iter().enumerate() {
        let is_last = index + 1 == segments.len();
        if segment.contains('@') {
            if !is_last || segment.matches('@').count() != 1 {
                return Err(invalid_wit_operation(
                    operation_id,
                    "versioned interface token is only allowed at the final path segment",
                ));
            }
            let Some((name, version)) = segment.split_once('@') else {
                return Err(invalid_wit_operation(
                    operation_id,
                    "versioned interface token is malformed",
                ));
            };
            if !is_valid_wit_symbol_token(name) || !is_valid_wit_interface_version(version) {
                return Err(invalid_wit_operation(
                    operation_id,
                    "interface version token must be '<name>@<semver-major.minor.patch>'",
                ));
            }
        } else if !is_valid_wit_symbol_token(segment) {
            return Err(invalid_wit_operation(
                operation_id,
                "operation interface segment contains unsupported characters",
            ));
        }
    }
    Ok(())
}

fn is_valid_wit_function_token(function: &str) -> bool {
    let mut chars = function.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic() && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn resolve_wit_operation_id(
    operation_id: &str,
) -> Result<MctWitResolvedOperation, MctWasmComponentRuntimeError> {
    let operation_id = operation_id.trim();
    let Some((interface, function)) = operation_id.rsplit_once('.') else {
        return Err(invalid_wit_operation(
            operation_id,
            "operation id is malformed",
        ));
    };
    validate_wit_interface_identity(interface, operation_id)?;
    if !is_valid_wit_function_token(function) {
        return Err(invalid_wit_operation(
            operation_id,
            "operation function token must start with a letter and only contain [A-Za-z0-9_-]",
        ));
    }
    Ok(MctWitResolvedOperation {
        operation_id: operation_id.into(),
        interface: interface.into(),
        function: function.into(),
    })
}

struct WasmDeadlineGuard {
    completed: Arc<AtomicBool>,
    timed_out: Arc<AtomicBool>,
    interrupter: JoinHandle<()>,
}

impl WasmDeadlineGuard {
    fn timed_out(&self) -> bool {
        self.timed_out.load(Ordering::SeqCst)
    }
}

impl Drop for WasmDeadlineGuard {
    fn drop(&mut self) {
        self.completed.store(true, Ordering::SeqCst);
        self.interrupter.thread().unpark();
    }
}

enum WasmDeadlinePermit {
    Expired,
    Running(WasmDeadlineGuard),
}

fn is_wasm_resource_limit_message(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("resource limit")
        || (message.contains("memory")
            && (message.contains("limit")
                || message.contains("minimum")
                || message.contains("maximum")))
}

fn wasm_invocation_result(
    call: &MctCall,
    authorized: &AuthorizedChildInvocation,
    audit_ref: AuditRef,
    outcome: ResultOutcome,
    requester_message: &str,
    output_size_bytes: Option<u64>,
) -> MctResult {
    MctResult {
        call_id: call.call_id.clone(),
        outcome,
        route_taken: (!matches!(outcome, ResultOutcome::Denied)).then(|| RouteTaken {
            node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            child_id: Some(
                ChildId::new(authorized.child_name().to_owned())
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            runtime_kind: RuntimeKind::WasmComponent,
        }),
        authority_decision_ref: authorized.authority_decision_id().clone(),
        execution_summary: ExecutionSummary {
            wall_time_ms: 0,
            execution_time_ms: None,
            queue_wait_ms: None,
            input_size_bytes: call.payload_metadata.size_bytes,
            output_size_bytes,
        },
        result_payload: MctCallPayloadHandle::Empty,
        requester_message: requester_message.into(),
        audit_ref,
    }
}

fn wasm_timeout_observation(
    ids: &MctWasmComponentInvocationIds,
    call: &MctCall,
    authorized: &AuthorizedChildInvocation,
) -> MctObservation {
    wasm_observation(
        ids.completed_observation_id.clone(),
        ids.completed_at.clone(),
        ObservationKind::RuntimeExecutionTimedOut,
        ObservationOutcome::TimedOut,
        call,
        authorized,
        "wasm component execution timed out",
    )
}

fn wasm_stale_authority_observation(
    ids: &MctWasmComponentInvocationIds,
    call: &MctCall,
    authorized: &AuthorizedChildInvocation,
) -> MctObservation {
    wasm_observation(
        ids.started_observation_id.clone(),
        ids.started_at.clone(),
        ObservationKind::RuntimeExecutionFailed,
        ObservationOutcome::Denied,
        call,
        authorized,
        "wasm component authority stale",
    )
}

fn s32_stale_authority_report(
    ids: MctWasmComponentInvocationIds,
    call: &MctCall,
    authorized: &AuthorizedChildInvocation,
) -> MctWasmComponentInvocationReport {
    let observation = wasm_stale_authority_observation(&ids, call, authorized);
    let result = wasm_invocation_result(
        call,
        authorized,
        ids.audit_ref,
        ResultOutcome::Denied,
        "not authorized",
        None,
    );
    MctWasmComponentInvocationReport {
        result,
        returned_s32: 0,
        observations: vec![observation],
    }
}

fn wit_stale_authority_report(
    ids: MctWasmComponentInvocationIds,
    call: &MctCall,
    authorized: &AuthorizedChildInvocation,
) -> MctWitComponentInvocationReport {
    let observation = wasm_stale_authority_observation(&ids, call, authorized);
    let result = wasm_invocation_result(
        call,
        authorized,
        ids.audit_ref,
        ResultOutcome::Denied,
        "not authorized",
        None,
    );
    MctWitComponentInvocationReport {
        result,
        output_json: Value::Null,
        observations: vec![observation],
        produced_messages: Vec::new(),
    }
}

fn s32_timeout_report(
    started: MctObservation,
    ids: MctWasmComponentInvocationIds,
    call: &MctCall,
    authorized: &AuthorizedChildInvocation,
) -> MctWasmComponentInvocationReport {
    let completed = wasm_timeout_observation(&ids, call, authorized);
    let result = wasm_invocation_result(
        call,
        authorized,
        ids.audit_ref,
        ResultOutcome::TimedOut,
        "wasm component timed out",
        None,
    );
    MctWasmComponentInvocationReport {
        result,
        returned_s32: 0,
        observations: vec![started, completed],
    }
}

fn wit_timeout_report(
    started: MctObservation,
    ids: MctWasmComponentInvocationIds,
    call: &MctCall,
    authorized: &AuthorizedChildInvocation,
) -> MctWitComponentInvocationReport {
    let completed = wasm_timeout_observation(&ids, call, authorized);
    let result = wasm_invocation_result(
        call,
        authorized,
        ids.audit_ref,
        ResultOutcome::TimedOut,
        "wasm component timed out",
        None,
    );
    MctWitComponentInvocationReport {
        result,
        output_json: Value::Null,
        observations: vec![started, completed],
        produced_messages: Vec::new(),
    }
}

#[derive(Debug)]
pub struct MctWasmComponentRuntime {
    engine: Engine,
    host_config: MctWasmHostConfig,
}

impl MctWasmComponentRuntime {
    pub fn new(host_config: MctWasmHostConfig) -> Result<Self, MctWasmComponentRuntimeError> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config)
            .map_err(|error| MctWasmComponentRuntimeError::Configure(error.to_string()))?;
        Ok(Self {
            engine,
            host_config,
        })
    }

    fn store_limits(&self) -> StoreLimits {
        StoreLimitsBuilder::new()
            .memory_size(self.host_config.memory_limit_bytes)
            .build()
    }

    fn instantiate_error(
        &self,
        path: PathBuf,
        source: wasmtime::Error,
    ) -> MctWasmComponentRuntimeError {
        let message = source.to_string();
        if is_wasm_resource_limit_message(&message) {
            MctWasmComponentRuntimeError::ResourceLimit {
                path,
                memory_limit_bytes: self.host_config.memory_limit_bytes,
                message,
            }
        } else {
            MctWasmComponentRuntimeError::Instantiate { path, message }
        }
    }

    fn call_error(
        &self,
        path: PathBuf,
        export_name: String,
        source: wasmtime::Error,
    ) -> MctWasmComponentRuntimeError {
        let message = source.to_string();
        if is_wasm_resource_limit_message(&message) {
            MctWasmComponentRuntimeError::ResourceLimit {
                path,
                memory_limit_bytes: self.host_config.memory_limit_bytes,
                message,
            }
        } else {
            MctWasmComponentRuntimeError::Call {
                path,
                export_name,
                message,
            }
        }
    }

    fn configure_deadline<T>(
        &self,
        store: &mut Store<T>,
        call: &MctCall,
    ) -> Result<WasmDeadlinePermit, MctWasmComponentRuntimeError> {
        let deadline = call
            .deadline
            .as_str()
            .parse::<jiff::Timestamp>()
            .map_err(|error| MctWasmComponentRuntimeError::Configure(error.to_string()))?;
        let now = jiff::Timestamp::now();
        if now >= deadline {
            return Ok(WasmDeadlinePermit::Expired);
        }

        store.set_epoch_deadline(1);
        store.epoch_deadline_trap();

        let wait = deadline.duration_since(now).unsigned_abs();
        let engine = self.engine.clone();
        let completed = Arc::new(AtomicBool::new(false));
        let timed_out = Arc::new(AtomicBool::new(false));
        let completed_for_thread = completed.clone();
        let timed_out_for_thread = timed_out.clone();
        let interrupter = thread::spawn(move || {
            thread::park_timeout(wait);
            if !completed_for_thread.load(Ordering::SeqCst) {
                timed_out_for_thread.store(true, Ordering::SeqCst);
                engine.increment_epoch();
            }
        });

        Ok(WasmDeadlinePermit::Running(WasmDeadlineGuard {
            completed,
            timed_out,
            interrupter,
        }))
    }

    pub fn discover_wit_operations(
        &self,
        component_path: impl AsRef<Path>,
    ) -> Result<BTreeSet<String>, MctWasmComponentRuntimeError> {
        let component_path = component_path.as_ref().to_path_buf();
        let component =
            component::Component::from_file(&self.engine, &component_path).map_err(|error| {
                MctWasmComponentRuntimeError::Load {
                    path: component_path.clone(),
                    message: error.to_string(),
                }
            })?;
        Ok(discover_wit_component_operations(&self.engine, &component))
    }

    pub fn discover_wit_imports(
        &self,
        component_path: impl AsRef<Path>,
    ) -> Result<BTreeSet<String>, MctWasmComponentRuntimeError> {
        let component_path = component_path.as_ref().to_path_buf();
        let component =
            component::Component::from_file(&self.engine, &component_path).map_err(|error| {
                MctWasmComponentRuntimeError::Load {
                    path: component_path.clone(),
                    message: error.to_string(),
                }
            })?;
        Ok(discover_wit_component_imports(&self.engine, &component))
    }

    pub fn invoke_authorized_child_wit_export(
        &self,
        authorized: AuthorizedChildInvocation,
        child: &MctLoadedChild,
        call: &MctCall,
        args_json: &Value,
        ids: MctWasmComponentInvocationIds,
    ) -> Result<MctWitComponentInvocationReport, MctWasmComponentRuntimeError> {
        if authorized.child_name() != child.name {
            return Err(MctWasmComponentRuntimeError::AuthorizedChildMismatch {
                authorized_child_name: authorized.child_name().to_owned(),
                loaded_child_name: child.name.clone(),
            });
        }
        let operation_id = operation_id_from_target(&call.target);
        if !child.allows_operation_target(&call.target) {
            return Err(MctWasmComponentRuntimeError::WitOperationNotAllowed {
                child_name: child.name.clone(),
                operation_id,
            });
        }
        self.invoke_wit_export_after_contract_check(
            authorized,
            call,
            &child.wasm_path,
            args_json,
            MctWitHostImportAdapters::none(),
            ids,
        )
    }

    pub fn invoke_authorized_child_wit_export_with_host_adapters(
        &self,
        authorized: AuthorizedChildInvocation,
        child: &MctLoadedChild,
        call: &MctCall,
        args_json: &Value,
        host_adapters: MctWitHostImportAdapters,
        ids: MctWasmComponentInvocationIds,
    ) -> Result<MctWitComponentInvocationReport, MctWasmComponentRuntimeError> {
        if authorized.child_name() != child.name {
            return Err(MctWasmComponentRuntimeError::AuthorizedChildMismatch {
                authorized_child_name: authorized.child_name().to_owned(),
                loaded_child_name: child.name.clone(),
            });
        }
        let operation_id = operation_id_from_target(&call.target);
        if !child.allows_operation_target(&call.target) {
            return Err(MctWasmComponentRuntimeError::WitOperationNotAllowed {
                child_name: child.name.clone(),
                operation_id,
            });
        }
        self.invoke_wit_export_after_contract_check(
            authorized,
            call,
            &child.wasm_path,
            args_json,
            host_adapters,
            ids,
        )
    }

    fn invoke_wit_export_after_contract_check(
        &self,
        authorized: AuthorizedChildInvocation,
        call: &MctCall,
        component_path: impl AsRef<Path>,
        args_json: &Value,
        host_adapters: MctWitHostImportAdapters,
        ids: MctWasmComponentInvocationIds,
    ) -> Result<MctWitComponentInvocationReport, MctWasmComponentRuntimeError> {
        let component_path = component_path.as_ref().to_path_buf();
        let operation = resolve_wit_operation_target(&call.target)?;
        if authorized.policy_revision() != call.authority_context.policy_revision {
            return Ok(wit_stale_authority_report(ids, call, &authorized));
        }
        let started = wasm_observation(
            ids.started_observation_id.clone(),
            ids.started_at.clone(),
            ObservationKind::RuntimeExecutionStarted,
            ObservationOutcome::Started,
            call,
            &authorized,
            "wasm component execution started",
        );
        let component =
            component::Component::from_file(&self.engine, &component_path).map_err(|error| {
                MctWasmComponentRuntimeError::Load {
                    path: component_path.clone(),
                    message: error.to_string(),
                }
            })?;
        validate_wit_host_imports_for_adapters(
            &self.engine,
            &component,
            &component_path,
            &host_adapters,
        )?;
        let mut linker = component::Linker::<MctWitHostState>::new(&self.engine);
        link_wit_host_import_adapters(&mut linker, &host_adapters)?;
        let wasi_ctx = build_wasi_ctx(host_adapters.wasi.as_ref())?;
        let mut store = Store::new(
            &self.engine,
            MctWitHostState {
                toy_registry: host_adapters.toy_registry,
                call: call.clone(),
                toy_observations: Vec::new(),
                logging: host_adapters.logging,
                measure: host_adapters.measure,
                git: host_adapters.git,
                keyvalue: host_adapters.keyvalue,
                messaging: host_adapters.messaging,
                messaging_clients: BTreeMap::new(),
                next_resource_rep: 1,
                produced_messages: Vec::new(),
                wasi_ctx,
                wasi_table: ResourceTable::new(),
                next_toy_call_index: 0,
                limits: self.store_limits(),
            },
        );
        store.limiter(|state| &mut state.limits);
        let deadline = self.configure_deadline(&mut store, call)?;
        if matches!(deadline, WasmDeadlinePermit::Expired) {
            return Ok(wit_timeout_report(started, ids, call, &authorized));
        }
        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(|error| self.instantiate_error(component_path.clone(), error))?;
        let func =
            lookup_wit_component_func(&mut store, &instance, &operation).ok_or_else(|| {
                MctWasmComponentRuntimeError::MissingWitOperation {
                    path: component_path.clone(),
                    operation_id: operation.operation_id.clone(),
                }
            })?;
        let func_ty = func.ty(store.as_context());
        let lowered_args =
            lower_typed_args_for_component(args_json, &func_ty).map_err(|error| {
                MctWasmComponentRuntimeError::WitValueConversion {
                    path: component_path.clone(),
                    operation_id: operation.operation_id.clone(),
                    message: error.to_string(),
                }
            })?;
        let mut results = vec![component::Val::Bool(false); func_ty.results().len()];
        let call_result = func.call(store.as_context_mut(), &lowered_args, &mut results);
        let timed_out =
            matches!(&deadline, WasmDeadlinePermit::Running(guard) if guard.timed_out());
        if let Err(error) = call_result {
            if timed_out {
                return Ok(wit_timeout_report(started, ids, call, &authorized));
            }
            return Err(self.call_error(
                component_path.clone(),
                operation.operation_id.clone(),
                error,
            ));
        }
        if timed_out {
            return Ok(wit_timeout_report(started, ids, call, &authorized));
        }
        let output_json = lift_component_results_to_json(&results, &func_ty).map_err(|error| {
            MctWasmComponentRuntimeError::WitValueConversion {
                path: component_path.clone(),
                operation_id: operation.operation_id.clone(),
                message: error.to_string(),
            }
        })?;
        let completed = wasm_observation(
            ids.completed_observation_id,
            ids.completed_at,
            ObservationKind::RuntimeExecutionCompleted,
            ObservationOutcome::Completed,
            call,
            &authorized,
            "wasm component execution completed",
        );
        let output_size_bytes = serde_json::to_vec(&output_json)
            .map(|bytes| bytes.len() as u64)
            .ok();
        let result = MctResult {
            call_id: call.call_id.clone(),
            outcome: ResultOutcome::Success,
            route_taken: Some(RouteTaken {
                node_id: MctNodeId::new("local-mct")
                    .expect("string ID literal/generated value must be non-empty"),
                child_id: Some(
                    ChildId::new(authorized.child_name().to_owned())
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                runtime_kind: RuntimeKind::WasmComponent,
            }),
            authority_decision_ref: authorized.authority_decision_id().clone(),
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: call.payload_metadata.size_bytes,
                output_size_bytes,
            },
            result_payload: MctCallPayloadHandle::Empty,
            requester_message: "wasm component completed".into(),
            audit_ref: ids.audit_ref,
        };
        let mut observations = vec![started];
        observations.extend(store.data().toy_observations.clone());
        observations.push(completed);
        Ok(MctWitComponentInvocationReport {
            result,
            output_json,
            observations,
            produced_messages: store.data().produced_messages.clone(),
        })
    }

    pub fn invoke_authorized_s32_export(
        &self,
        authorized: AuthorizedChildInvocation,
        call: &MctCall,
        component_path: impl AsRef<Path>,
        export_name: &str,
        ids: MctWasmComponentInvocationIds,
    ) -> Result<MctWasmComponentInvocationReport, MctWasmComponentRuntimeError> {
        let component_path = component_path.as_ref().to_path_buf();
        if authorized.policy_revision() != call.authority_context.policy_revision {
            return Ok(s32_stale_authority_report(ids, call, &authorized));
        }
        let started = wasm_observation(
            ids.started_observation_id.clone(),
            ids.started_at.clone(),
            ObservationKind::RuntimeExecutionStarted,
            ObservationOutcome::Started,
            call,
            &authorized,
            "wasm component execution started",
        );
        let component =
            component::Component::from_file(&self.engine, &component_path).map_err(|error| {
                MctWasmComponentRuntimeError::Load {
                    path: component_path.clone(),
                    message: error.to_string(),
                }
            })?;
        let linker = component::Linker::<MctWasmEmptyHostState>::new(&self.engine);
        let mut store = Store::new(
            &self.engine,
            MctWasmEmptyHostState {
                limits: self.store_limits(),
            },
        );
        store.limiter(|state| &mut state.limits);
        let deadline = self.configure_deadline(&mut store, call)?;
        if matches!(deadline, WasmDeadlinePermit::Expired) {
            return Ok(s32_timeout_report(started, ids, call, &authorized));
        }
        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(|error| self.instantiate_error(component_path.clone(), error))?;
        let func = instance.get_func(&mut store, export_name).ok_or_else(|| {
            MctWasmComponentRuntimeError::MissingExport {
                path: component_path.clone(),
                export_name: export_name.into(),
            }
        })?;
        let mut results = [component::Val::S32(0)];
        let call_result = func.call(&mut store, &[], &mut results);
        let timed_out =
            matches!(&deadline, WasmDeadlinePermit::Running(guard) if guard.timed_out());
        if let Err(error) = call_result {
            if timed_out {
                return Ok(s32_timeout_report(started, ids, call, &authorized));
            }
            return Err(self.call_error(component_path.clone(), export_name.into(), error));
        }
        if timed_out {
            return Ok(s32_timeout_report(started, ids, call, &authorized));
        }
        let component::Val::S32(returned_s32) = results[0] else {
            return Err(MctWasmComponentRuntimeError::UnexpectedResult {
                export_name: export_name.into(),
            });
        };
        let completed = wasm_observation(
            ids.completed_observation_id,
            ids.completed_at,
            ObservationKind::RuntimeExecutionCompleted,
            ObservationOutcome::Completed,
            call,
            &authorized,
            "wasm component execution completed",
        );
        let result = wasm_invocation_result(
            call,
            &authorized,
            ids.audit_ref,
            ResultOutcome::Success,
            "wasm component completed",
            Some(std::mem::size_of::<i32>() as u64),
        );
        Ok(MctWasmComponentInvocationReport {
            result,
            returned_s32,
            observations: vec![started, completed],
        })
    }

    pub fn invoke_authorized_s32_export_with_toy_imports(
        &self,
        authorized: AuthorizedChildInvocation,
        call: &MctCall,
        invocation: MctWasmComponentToyInvocation,
    ) -> Result<MctWasmComponentInvocationReport, MctWasmComponentRuntimeError> {
        let component_path = invocation.component_path;
        let export_name = invocation.export_name;
        let ids = invocation.ids;
        if authorized.policy_revision() != call.authority_context.policy_revision {
            return Ok(s32_stale_authority_report(ids, call, &authorized));
        }
        let started = wasm_observation(
            ids.started_observation_id.clone(),
            ids.started_at.clone(),
            ObservationKind::RuntimeExecutionStarted,
            ObservationOutcome::Started,
            call,
            &authorized,
            "wasm component execution started",
        );
        let component =
            component::Component::from_file(&self.engine, &component_path).map_err(|error| {
                MctWasmComponentRuntimeError::Load {
                    path: component_path.clone(),
                    message: error.to_string(),
                }
            })?;
        let mut linker = component::Linker::<MctWasmHostState>::new(&self.engine);
        for toy_import in invocation.toy_imports {
            let import_name = toy_import.import_name.clone();
            linker
                .root()
                .func_wrap(
                    &import_name,
                    move |mut store: StoreContextMut<'_, MctWasmHostState>, _params: ()| {
                        let registry = store.data().toy_registry.clone();
                        let call = store.data().call.clone();
                        let report = registry.call_authorized_toy(
                            &toy_import.authorized_toy_call,
                            &call,
                            "{}",
                            toy_import.ids.clone(),
                        );
                        let return_code = match report.outcome {
                            MctToyAdapterOutcome::Success => 1_i32,
                            MctToyAdapterOutcome::Failed => -1_i32,
                        };
                        store
                            .data_mut()
                            .toy_observations
                            .extend(report.observations);
                        Ok((return_code,))
                    },
                )
                .map_err(|error| MctWasmComponentRuntimeError::Configure(error.to_string()))?;
        }
        let mut store = Store::new(
            &self.engine,
            MctWasmHostState {
                toy_registry: invocation.toy_registry,
                call: call.clone(),
                toy_observations: Vec::new(),
                limits: self.store_limits(),
            },
        );
        store.limiter(|state| &mut state.limits);
        let deadline = self.configure_deadline(&mut store, call)?;
        if matches!(deadline, WasmDeadlinePermit::Expired) {
            return Ok(s32_timeout_report(started, ids, call, &authorized));
        }
        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(|error| self.instantiate_error(component_path.clone(), error))?;
        let func = instance.get_func(&mut store, &export_name).ok_or_else(|| {
            MctWasmComponentRuntimeError::MissingExport {
                path: component_path.clone(),
                export_name: export_name.clone(),
            }
        })?;
        let mut results = [component::Val::S32(0)];
        let call_result = func.call(&mut store, &[], &mut results);
        let timed_out =
            matches!(&deadline, WasmDeadlinePermit::Running(guard) if guard.timed_out());
        if let Err(error) = call_result {
            if timed_out {
                return Ok(s32_timeout_report(started, ids, call, &authorized));
            }
            return Err(self.call_error(component_path.clone(), export_name.clone(), error));
        }
        if timed_out {
            return Ok(s32_timeout_report(started, ids, call, &authorized));
        }
        let component::Val::S32(returned_s32) = results[0] else {
            return Err(MctWasmComponentRuntimeError::UnexpectedResult {
                export_name: export_name.clone(),
            });
        };
        let completed = wasm_observation(
            ids.completed_observation_id,
            ids.completed_at,
            ObservationKind::RuntimeExecutionCompleted,
            ObservationOutcome::Completed,
            call,
            &authorized,
            "wasm component execution completed",
        );
        let mut observations = vec![started];
        observations.extend(store.data().toy_observations.clone());
        observations.push(completed);
        let result = wasm_invocation_result(
            call,
            &authorized,
            ids.audit_ref,
            ResultOutcome::Success,
            "wasm component completed",
            Some(std::mem::size_of::<i32>() as u64),
        );
        Ok(MctWasmComponentInvocationReport {
            result,
            returned_s32,
            observations,
        })
    }
}

fn discover_wit_component_operations(
    engine: &Engine,
    component: &component::Component,
) -> BTreeSet<String> {
    let mut operations = BTreeSet::new();
    let component_type = component.component_type();
    for (interface_name, item) in component_type.exports(engine) {
        let component::types::ComponentItem::ComponentInstance(instance) = item else {
            continue;
        };
        for (function_name, function_item) in instance.exports(engine) {
            if matches!(
                function_item,
                component::types::ComponentItem::ComponentFunc(_)
            ) {
                operations.insert(format!("{interface_name}.{function_name}"));
            }
        }
    }
    operations
}

fn discover_wit_component_imports(
    engine: &Engine,
    component: &component::Component,
) -> BTreeSet<String> {
    component
        .component_type()
        .imports(engine)
        .map(|(import_name, _item)| import_name.to_string())
        .collect()
}

fn validate_wit_host_imports_for_adapters(
    engine: &Engine,
    component: &component::Component,
    component_path: &Path,
    host_adapters: &MctWitHostImportAdapters,
) -> Result<(), MctWasmComponentRuntimeError> {
    for import_name in discover_wit_component_imports(engine, component) {
        let configured = match import_name.as_str() {
            "wasi:logging/logging@0.1.0" => host_adapters.logging.is_some(),
            "patina:measure/measure@0.1.0" => host_adapters.measure.is_some(),
            "patina:git/git@0.1.0" => host_adapters.git.is_some(),
            "wasi:keyvalue/store@0.2.0" => host_adapters.keyvalue.is_some(),
            "wasi:messaging/producer@0.2.0" | "wasi:messaging/types@0.2.0" => {
                host_adapters.messaging.is_some()
            }
            "patina:child/runtime-types@0.1.0" | "patina:watch/types@0.1.0" => true,
            name if is_supported_wasi_p2_import(name) => host_adapters.wasi.is_some(),
            _ => false,
        };
        if !configured {
            return Err(MctWasmComponentRuntimeError::UnsupportedWitHostImport {
                path: component_path.to_path_buf(),
                import_name: import_name.clone(),
                item_name: import_name,
                message: "WIT host imports require a concrete MCT adapter; generic stubs are not permitted"
                    .into(),
            });
        }
    }
    Ok(())
}

fn is_supported_wasi_p2_import(name: &str) -> bool {
    matches!(
        name,
        "wasi:cli/environment@0.2.3"
            | "wasi:cli/exit@0.2.3"
            | "wasi:cli/stdin@0.2.3"
            | "wasi:cli/stdout@0.2.3"
            | "wasi:cli/stderr@0.2.3"
            | "wasi:cli/terminal-input@0.2.3"
            | "wasi:cli/terminal-output@0.2.3"
            | "wasi:cli/terminal-stdin@0.2.3"
            | "wasi:cli/terminal-stdout@0.2.3"
            | "wasi:cli/terminal-stderr@0.2.3"
            | "wasi:clocks/monotonic-clock@0.2.3"
            | "wasi:clocks/wall-clock@0.2.3"
            | "wasi:filesystem/types@0.2.3"
            | "wasi:filesystem/preopens@0.2.3"
            | "wasi:io/error@0.2.3"
            | "wasi:io/streams@0.2.3"
            | "wasi:random/random@0.2.3"
    )
}

fn link_wit_host_import_adapters(
    linker: &mut component::Linker<MctWitHostState>,
    host_adapters: &MctWitHostImportAdapters,
) -> Result<(), MctWasmComponentRuntimeError> {
    if host_adapters.wasi.is_some() {
        wasmtime_wasi::p2::add_to_linker_sync(linker).map_err(|error| {
            MctWasmComponentRuntimeError::Instantiate {
                path: PathBuf::from("wasi:p2"),
                message: error.to_string(),
            }
        })?;
    }

    for type_only_import in [
        "patina:child/runtime-types@0.1.0",
        "patina:watch/types@0.1.0",
        "wasi:messaging/types@0.2.0",
    ] {
        linker.instance(type_only_import).map_err(|error| {
            MctWasmComponentRuntimeError::Instantiate {
                path: PathBuf::from(type_only_import),
                message: error.to_string(),
            }
        })?;
    }

    if host_adapters.logging.is_some() {
        let mut logging = linker
            .instance("wasi:logging/logging@0.1.0")
            .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
                path: PathBuf::from("wasi:logging/logging@0.1.0"),
                message: error.to_string(),
            })?;
        logging
            .func_new("log", |mut store, _ty, params, results| {
                if !results.is_empty() {
                    return Err(wasmtime::Error::msg(
                        "invalid wasi logging host result shape",
                    ));
                }
                let [
                    level,
                    component::Val::String(context),
                    component::Val::String(message),
                ] = params
                else {
                    return Err(wasmtime::Error::msg("invalid wasi logging host call shape"));
                };
                let level = match level {
                    component::Val::Enum(level) => level.as_str(),
                    component::Val::Variant(level, None) => level.as_str(),
                    _ => return Err(wasmtime::Error::msg("invalid wasi logging level shape")),
                };
                let input_json = serde_json::json!({
                    "interface": "wasi:logging/logging@0.1.0",
                    "function": "log",
                    "level": level,
                    "context": context,
                    "message": message,
                });
                let adapter =
                    store.data_mut().logging.take().ok_or_else(|| {
                        wasmtime::Error::msg("wasi logging adapter not configured")
                    })?;
                let report = store.data_mut().call_toy(&adapter, &input_json);
                store.data_mut().logging = Some(adapter);
                match report.outcome {
                    MctToyAdapterOutcome::Success => Ok(()),
                    MctToyAdapterOutcome::Failed => Err(wasmtime::Error::msg(report.safe_message)),
                }
            })
            .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
                path: PathBuf::from("wasi:logging/logging@0.1.0.log"),
                message: error.to_string(),
            })?;
    }

    if host_adapters.measure.is_some() {
        let mut measure = linker
            .instance("patina:measure/measure@0.1.0")
            .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
                path: PathBuf::from("patina:measure/measure@0.1.0"),
                message: error.to_string(),
            })?;
        measure
            .func_new("gauge", |mut store, _ty, params, results| {
                let [component::Val::String(name), component::Val::Float64(value)] = params else {
                    return Err(wasmtime::Error::msg(
                        "invalid patina measure gauge host call shape",
                    ));
                };
                call_measure_toy(&mut store, results, "gauge", name, *value)
            })
            .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
                path: PathBuf::from("patina:measure/measure@0.1.0.gauge"),
                message: error.to_string(),
            })?;
        measure
            .func_new("counter", |mut store, _ty, params, results| {
                let [component::Val::String(name), component::Val::Float64(delta)] = params else {
                    return Err(wasmtime::Error::msg(
                        "invalid patina measure counter host call shape",
                    ));
                };
                call_measure_toy(&mut store, results, "counter", name, *delta)
            })
            .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
                path: PathBuf::from("patina:measure/measure@0.1.0.counter"),
                message: error.to_string(),
            })?;
    }

    if host_adapters.git.is_some() {
        link_git_host_import(linker)?;
    }
    if host_adapters.keyvalue.is_some() {
        link_keyvalue_host_import(linker)?;
    }
    if host_adapters.messaging.is_some() {
        link_messaging_host_import(linker)?;
    }

    Ok(())
}

#[derive(Debug)]
struct MctKeyvalueBucketResource;

#[derive(Debug)]
struct MctMessagingClientResource;

fn set_wit_result_ok(
    results: &mut [component::Val],
    value: Option<component::Val>,
) -> wasmtime::Result<()> {
    let [result] = results else {
        return Err(wasmtime::Error::msg("invalid WIT result shape"));
    };
    *result = component::Val::Result(Ok(value.map(Box::new)));
    Ok(())
}

fn set_keyvalue_error(results: &mut [component::Val], name: &str) -> wasmtime::Result<()> {
    let [result] = results else {
        return Err(wasmtime::Error::msg("invalid keyvalue result shape"));
    };
    *result = component::Val::Result(Err(Some(Box::new(component::Val::Variant(
        name.into(),
        None,
    )))));
    Ok(())
}

fn link_keyvalue_host_import(
    linker: &mut component::Linker<MctWitHostState>,
) -> Result<(), MctWasmComponentRuntimeError> {
    let mut store = linker
        .instance("wasi:keyvalue/store@0.2.0")
        .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
            path: PathBuf::from("wasi:keyvalue/store@0.2.0"),
            message: error.to_string(),
        })?;
    store
        .resource(
            "bucket",
            component::ResourceType::host::<MctKeyvalueBucketResource>(),
            |_store, _rep| Ok(()),
        )
        .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
            path: PathBuf::from("wasi:keyvalue/store@0.2.0.bucket"),
            message: error.to_string(),
        })?;
    store
        .func_new("open", |mut store, _ty, params, results| {
            let [component::Val::String(identifier)] = params else {
                return Err(wasmtime::Error::msg("invalid keyvalue open call shape"));
            };
            let adapter = store
                .data_mut()
                .keyvalue
                .take()
                .ok_or_else(|| wasmtime::Error::msg("keyvalue adapter not configured"))?;
            let expected = adapter.bucket_identifier == *identifier;
            let report = store.data_mut().call_toy(
                &adapter.get,
                &serde_json::json!({"function": "open", "identifier": identifier}),
            );
            store.data_mut().keyvalue = Some(adapter);
            if !expected || report.outcome != MctToyAdapterOutcome::Success {
                return set_keyvalue_error(results, "access-denied");
            }
            let resource = component::Resource::<MctKeyvalueBucketResource>::new_own(1);
            let resource =
                component::ResourceAny::try_from_resource(resource, store.as_context_mut())?;
            set_wit_result_ok(results, Some(component::Val::Resource(resource)))
        })
        .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
            path: PathBuf::from("wasi:keyvalue/store@0.2.0.open"),
            message: error.to_string(),
        })?;
    store
        .func_new("[method]bucket.get", |mut store, _ty, params, results| {
            let [
                component::Val::Resource(bucket),
                component::Val::String(key),
            ] = params
            else {
                return Err(wasmtime::Error::msg("invalid keyvalue get call shape"));
            };
            let bucket =
                bucket.try_into_resource::<MctKeyvalueBucketResource>(store.as_context_mut())?;
            if bucket.rep() != 1 {
                return set_keyvalue_error(results, "access-denied");
            }
            let adapter = store
                .data_mut()
                .keyvalue
                .take()
                .ok_or_else(|| wasmtime::Error::msg("keyvalue adapter not configured"))?;
            let report = store.data_mut().call_toy(
                &adapter.get,
                &serde_json::json!({"function": "get", "key": key}),
            );
            let value = if report.outcome == MctToyAdapterOutcome::Success {
                MctRuntimeStateStore::open(&adapter.state_path)
                    .and_then(|state| state.child_keyvalue_get(&adapter.bucket_resource_id, key))
                    .ok()
                    .flatten()
            } else {
                None
            };
            let allowed = report.outcome == MctToyAdapterOutcome::Success;
            store.data_mut().keyvalue = Some(adapter);
            if !allowed {
                return set_keyvalue_error(results, "access-denied");
            }
            let value = value.map(|bytes| {
                component::Val::List(bytes.into_iter().map(component::Val::U8).collect())
            });
            set_wit_result_ok(results, Some(component::Val::Option(value.map(Box::new))))
        })
        .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
            path: PathBuf::from("wasi:keyvalue/store@0.2.0.[method]bucket.get"),
            message: error.to_string(),
        })?;
    store
        .func_new("[method]bucket.set", |mut store, _ty, params, results| {
            let [
                component::Val::Resource(bucket),
                component::Val::String(key),
                component::Val::List(value),
            ] = params
            else {
                return Err(wasmtime::Error::msg("invalid keyvalue set call shape"));
            };
            let bucket =
                bucket.try_into_resource::<MctKeyvalueBucketResource>(store.as_context_mut())?;
            if bucket.rep() != 1 {
                return set_keyvalue_error(results, "access-denied");
            }
            let bytes = value
                .iter()
                .map(|value| match value {
                    component::Val::U8(value) => Ok(*value),
                    _ => Err(wasmtime::Error::msg("invalid keyvalue byte shape")),
                })
                .collect::<wasmtime::Result<Vec<_>>>()?;
            let adapter = store
                .data_mut()
                .keyvalue
                .take()
                .ok_or_else(|| wasmtime::Error::msg("keyvalue adapter not configured"))?;
            let report = store.data_mut().call_toy(
                &adapter.set,
                &serde_json::json!({"function": "set", "key": key, "size_bytes": bytes.len()}),
            );
            let persisted = report.outcome == MctToyAdapterOutcome::Success
                && MctRuntimeStateStore::open(&adapter.state_path)
                    .and_then(|state| {
                        state.child_keyvalue_set(&adapter.bucket_resource_id, key, &bytes)
                    })
                    .is_ok();
            store.data_mut().keyvalue = Some(adapter);
            if persisted {
                set_wit_result_ok(results, None)
            } else {
                set_keyvalue_error(results, "access-denied")
            }
        })
        .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
            path: PathBuf::from("wasi:keyvalue/store@0.2.0.[method]bucket.set"),
            message: error.to_string(),
        })?;
    Ok(())
}

fn link_messaging_host_import(
    linker: &mut component::Linker<MctWitHostState>,
) -> Result<(), MctWasmComponentRuntimeError> {
    let mut producer = linker
        .instance("wasi:messaging/producer@0.2.0")
        .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
            path: PathBuf::from("wasi:messaging/producer@0.2.0"),
            message: error.to_string(),
        })?;
    producer
        .resource(
            "client",
            component::ResourceType::host::<MctMessagingClientResource>(),
            |mut store, rep| {
                store.data_mut().messaging_clients.remove(&rep);
                Ok(())
            },
        )
        .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
            path: PathBuf::from("wasi:messaging/producer@0.2.0.client"),
            message: error.to_string(),
        })?;
    producer
        .func_new("connect", |mut store, _ty, params, results| {
            let [component::Val::String(name)] = params else {
                return Err(wasmtime::Error::msg("invalid messaging connect call shape"));
            };
            if resolve_wit_operation_id(name).is_err() {
                let [result] = results else {
                    return Err(wasmtime::Error::msg("invalid messaging result shape"));
                };
                *result = component::Val::Result(Err(Some(Box::new(component::Val::String(
                    "invalid operation target".into(),
                )))));
                return Ok(());
            }
            let adapter = store
                .data_mut()
                .messaging
                .take()
                .ok_or_else(|| wasmtime::Error::msg("messaging adapter not configured"))?;
            let report = store.data_mut().call_toy(
                &adapter.toy,
                &serde_json::json!({"function": "connect", "name": name}),
            );
            store.data_mut().messaging = Some(adapter);
            if report.outcome != MctToyAdapterOutcome::Success {
                let [result] = results else {
                    return Err(wasmtime::Error::msg("invalid messaging result shape"));
                };
                *result = component::Val::Result(Err(Some(Box::new(component::Val::String(
                    "not authorized".into(),
                )))));
                return Ok(());
            }
            let rep = store.data().next_resource_rep;
            store.data_mut().next_resource_rep += 1;
            store.data_mut().messaging_clients.insert(rep, name.clone());
            let resource = component::Resource::<MctMessagingClientResource>::new_own(rep);
            let resource =
                component::ResourceAny::try_from_resource(resource, store.as_context_mut())?;
            set_wit_result_ok(results, Some(component::Val::Resource(resource)))
        })
        .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
            path: PathBuf::from("wasi:messaging/producer@0.2.0.connect"),
            message: error.to_string(),
        })?;
    producer
        .func_new("send", |mut store, _ty, params, results| {
            let [
                component::Val::Resource(client),
                component::Val::Record(fields),
            ] = params
            else {
                return Err(wasmtime::Error::msg("invalid messaging send call shape"));
            };
            let client =
                client.try_into_resource::<MctMessagingClientResource>(store.as_context_mut())?;
            let target_operation = store
                .data()
                .messaging_clients
                .get(&client.rep())
                .cloned()
                .ok_or_else(|| wasmtime::Error::msg("unknown messaging client"))?;
            let field = |name: &str| {
                fields
                    .iter()
                    .find(|(field, _)| field == name)
                    .map(|(_, value)| value)
            };
            let topic = match field("topic") {
                Some(component::Val::String(topic)) => topic.clone(),
                _ => return Err(wasmtime::Error::msg("missing message topic")),
            };
            let content_type = match field("content-type") {
                Some(component::Val::Option(Some(value))) => match value.as_ref() {
                    component::Val::String(value) => Some(value.clone()),
                    _ => return Err(wasmtime::Error::msg("invalid message content type")),
                },
                Some(component::Val::Option(None)) => None,
                _ => return Err(wasmtime::Error::msg("missing message content type")),
            };
            let data = match field("data") {
                Some(component::Val::List(values)) => values
                    .iter()
                    .map(|value| match value {
                        component::Val::U8(value) => Ok(*value),
                        _ => Err(wasmtime::Error::msg("invalid message byte shape")),
                    })
                    .collect::<wasmtime::Result<Vec<_>>>()?,
                _ => return Err(wasmtime::Error::msg("missing message data")),
            };
            let metadata = match field("metadata") {
                Some(component::Val::List(values)) => values
                    .iter()
                    .map(|value| match value {
                        component::Val::Tuple(values) => match values.as_slice() {
                            [component::Val::String(key), component::Val::String(value)] => {
                                Ok((key.clone(), value.clone()))
                            }
                            _ => Err(wasmtime::Error::msg("invalid message metadata tuple")),
                        },
                        _ => Err(wasmtime::Error::msg("invalid message metadata shape")),
                    })
                    .collect::<wasmtime::Result<Vec<_>>>()?,
                _ => return Err(wasmtime::Error::msg("missing message metadata")),
            };
            let adapter = store
                .data_mut()
                .messaging
                .take()
                .ok_or_else(|| wasmtime::Error::msg("messaging adapter not configured"))?;
            if let Err(refusal) = validate_wit_watch_message_admission(
                &adapter.watch_admission,
                &target_operation,
                &topic,
                content_type.as_deref(),
                &data,
                metadata.len(),
                store.data().produced_messages.len(),
            ) {
                store.data_mut().messaging = Some(adapter);
                let [result] = results else {
                    return Err(wasmtime::Error::msg("invalid messaging result shape"));
                };
                *result = component::Val::Result(Err(Some(Box::new(component::Val::String(
                    refusal.to_string(),
                )))));
                return Ok(());
            }
            let report = store.data_mut().call_toy(
                &adapter.toy,
                &serde_json::json!({"function": "send", "target": target_operation, "topic": topic, "size_bytes": data.len()}),
            );
            store.data_mut().messaging = Some(adapter);
            if report.outcome != MctToyAdapterOutcome::Success {
                let [result] = results else {
                    return Err(wasmtime::Error::msg("invalid messaging result shape"));
                };
                *result = component::Val::Result(Err(Some(Box::new(component::Val::String(
                    "not authorized".into(),
                )))));
                return Ok(());
            }
            let offset = store.data().produced_messages.len() as u64 + 1;
            store
                .data_mut()
                .produced_messages
                .push(MctWitProducedMessage {
                    target_operation,
                    topic,
                    content_type,
                    data,
                    metadata,
                    offset,
                });
            set_wit_result_ok(results, Some(component::Val::U64(offset)))
        })
        .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
            path: PathBuf::from("wasi:messaging/producer@0.2.0.send"),
            message: error.to_string(),
        })?;
    Ok(())
}

fn link_git_host_import(
    linker: &mut component::Linker<MctWitHostState>,
) -> Result<(), MctWasmComponentRuntimeError> {
    let mut git = linker.instance("patina:git/git@0.1.0").map_err(|error| {
        MctWasmComponentRuntimeError::Instantiate {
            path: PathBuf::from("patina:git/git@0.1.0"),
            message: error.to_string(),
        }
    })?;

    git.func_new("create-tag", |mut store, _ty, params, results| {
        let [component::Val::String(name)] = params else {
            return Err(wasmtime::Error::msg(
                "invalid patina git create-tag host call shape",
            ));
        };
        call_git_toy(
            &mut store,
            results,
            serde_json::json!({"function": "create-tag", "name": name}),
            MctWitGitResultShape::Unit,
        )
    })
    .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
        path: PathBuf::from("patina:git/git@0.1.0.create-tag"),
        message: error.to_string(),
    })?;
    git.func_new("create-tag-at", |mut store, _ty, params, results| {
        let [
            component::Val::String(name),
            component::Val::String(git_ref),
        ] = params
        else {
            return Err(wasmtime::Error::msg(
                "invalid patina git create-tag-at host call shape",
            ));
        };
        call_git_toy(
            &mut store,
            results,
            serde_json::json!({"function": "create-tag-at", "name": name, "git_ref": git_ref}),
            MctWitGitResultShape::Unit,
        )
    })
    .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
        path: PathBuf::from("patina:git/git@0.1.0.create-tag-at"),
        message: error.to_string(),
    })?;
    git.func_new("delete-tag", |mut store, _ty, params, results| {
        let [component::Val::String(name)] = params else {
            return Err(wasmtime::Error::msg(
                "invalid patina git delete-tag host call shape",
            ));
        };
        call_git_toy(
            &mut store,
            results,
            serde_json::json!({"function": "delete-tag", "name": name}),
            MctWitGitResultShape::Unit,
        )
    })
    .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
        path: PathBuf::from("patina:git/git@0.1.0.delete-tag"),
        message: error.to_string(),
    })?;
    git.func_new("tag-exists", |mut store, _ty, params, results| {
        let [component::Val::String(name)] = params else {
            return Err(wasmtime::Error::msg(
                "invalid patina git tag-exists host call shape",
            ));
        };
        call_git_toy(
            &mut store,
            results,
            serde_json::json!({"function": "tag-exists", "name": name}),
            MctWitGitResultShape::Bool,
        )
    })
    .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
        path: PathBuf::from("patina:git/git@0.1.0.tag-exists"),
        message: error.to_string(),
    })?;
    git.func_new("commit", |mut store, _ty, params, results| {
        let [component::Val::String(message)] = params else {
            return Err(wasmtime::Error::msg(
                "invalid patina git commit host call shape",
            ));
        };
        call_git_toy(
            &mut store,
            results,
            serde_json::json!({"function": "commit", "message": message}),
            MctWitGitResultShape::String,
        )
    })
    .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
        path: PathBuf::from("patina:git/git@0.1.0.commit"),
        message: error.to_string(),
    })?;
    git.func_new("log-oneline", |mut store, _ty, params, results| {
        let [component::Val::U32(limit)] = params else {
            return Err(wasmtime::Error::msg(
                "invalid patina git log-oneline host call shape",
            ));
        };
        call_git_toy(
            &mut store,
            results,
            serde_json::json!({"function": "log-oneline", "limit": limit}),
            MctWitGitResultShape::StringList,
        )
    })
    .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
        path: PathBuf::from("patina:git/git@0.1.0.log-oneline"),
        message: error.to_string(),
    })?;
    git.func_new("diff-stat", |mut store, _ty, params, results| {
        if !params.is_empty() {
            return Err(wasmtime::Error::msg(
                "invalid patina git diff-stat host call shape",
            ));
        }
        call_git_toy(
            &mut store,
            results,
            serde_json::json!({"function": "diff-stat"}),
            MctWitGitResultShape::String,
        )
    })
    .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
        path: PathBuf::from("patina:git/git@0.1.0.diff-stat"),
        message: error.to_string(),
    })?;
    git.func_new("status-porcelain", |mut store, _ty, params, results| {
        if !params.is_empty() {
            return Err(wasmtime::Error::msg(
                "invalid patina git status-porcelain host call shape",
            ));
        }
        call_git_toy(
            &mut store,
            results,
            serde_json::json!({"function": "status-porcelain"}),
            MctWitGitResultShape::String,
        )
    })
    .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
        path: PathBuf::from("patina:git/git@0.1.0.status-porcelain"),
        message: error.to_string(),
    })?;
    git.func_new("add-paths", |mut store, _ty, params, results| {
        let [component::Val::List(paths)] = params else {
            return Err(wasmtime::Error::msg(
                "invalid patina git add-paths host call shape",
            ));
        };
        let paths = git_path_list(paths)?;
        call_git_toy(
            &mut store,
            results,
            serde_json::json!({"function": "add-paths", "paths": paths}),
            MctWitGitResultShape::Unit,
        )
    })
    .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
        path: PathBuf::from("patina:git/git@0.1.0.add-paths"),
        message: error.to_string(),
    })?;
    git.func_new("remove-paths", |mut store, _ty, params, results| {
        let [component::Val::List(paths)] = params else {
            return Err(wasmtime::Error::msg(
                "invalid patina git remove-paths host call shape",
            ));
        };
        let paths = git_path_list(paths)?;
        call_git_toy(
            &mut store,
            results,
            serde_json::json!({"function": "remove-paths", "paths": paths}),
            MctWitGitResultShape::Unit,
        )
    })
    .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
        path: PathBuf::from("patina:git/git@0.1.0.remove-paths"),
        message: error.to_string(),
    })?;
    git.func_new("is-clean-tracked", |mut store, _ty, params, results| {
        if !params.is_empty() {
            return Err(wasmtime::Error::msg(
                "invalid patina git is-clean-tracked host call shape",
            ));
        }
        call_git_toy(
            &mut store,
            results,
            serde_json::json!({"function": "is-clean-tracked"}),
            MctWitGitResultShape::Bool,
        )
    })
    .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
        path: PathBuf::from("patina:git/git@0.1.0.is-clean-tracked"),
        message: error.to_string(),
    })?;
    git.func_new(
        "commits-behind-upstream",
        |mut store, _ty, params, results| {
            if !params.is_empty() {
                return Err(wasmtime::Error::msg(
                    "invalid patina git commits-behind-upstream host call shape",
                ));
            }
            call_git_toy(
                &mut store,
                results,
                serde_json::json!({"function": "commits-behind-upstream"}),
                MctWitGitResultShape::U32,
            )
        },
    )
    .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
        path: PathBuf::from("patina:git/git@0.1.0.commits-behind-upstream"),
        message: error.to_string(),
    })?;
    git.func_new("is-diverged", |mut store, _ty, params, results| {
        if !params.is_empty() {
            return Err(wasmtime::Error::msg(
                "invalid patina git is-diverged host call shape",
            ));
        }
        call_git_toy(
            &mut store,
            results,
            serde_json::json!({"function": "is-diverged"}),
            MctWitGitResultShape::Bool,
        )
    })
    .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
        path: PathBuf::from("patina:git/git@0.1.0.is-diverged"),
        message: error.to_string(),
    })?;

    Ok(())
}

fn call_measure_toy(
    store: &mut StoreContextMut<'_, MctWitHostState>,
    results: &mut [component::Val],
    function: &str,
    name: &str,
    value: f64,
) -> wasmtime::Result<()> {
    if results.len() != 1 {
        return Err(wasmtime::Error::msg(
            "invalid patina measure host result shape",
        ));
    }
    let input_json = serde_json::json!({
        "interface": "patina:measure/measure@0.1.0",
        "function": function,
        "name": name,
        "value": value,
    });
    let adapter = store
        .data_mut()
        .measure
        .take()
        .ok_or_else(|| wasmtime::Error::msg("patina measure adapter not configured"))?;
    let report = store.data_mut().call_toy(&adapter, &input_json);
    store.data_mut().measure = Some(adapter);
    results[0] = match report.outcome {
        MctToyAdapterOutcome::Success => component::Val::Result(Ok(None)),
        MctToyAdapterOutcome::Failed => component::Val::Result(Err(Some(Box::new(
            component::Val::String(report.safe_message),
        )))),
    };
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MctWitGitResultShape {
    Unit,
    String,
    Bool,
    U32,
    StringList,
}

fn call_git_toy(
    store: &mut StoreContextMut<'_, MctWitHostState>,
    results: &mut [component::Val],
    mut input_json: Value,
    shape: MctWitGitResultShape,
) -> wasmtime::Result<()> {
    if results.len() != 1 {
        return Err(wasmtime::Error::msg("invalid patina git host result shape"));
    }
    let object = input_json
        .as_object_mut()
        .ok_or_else(|| wasmtime::Error::msg("patina git input must be a JSON object"))?;
    object.insert(
        "interface".into(),
        Value::String("patina:git/git@0.1.0".into()),
    );
    let adapter = store
        .data_mut()
        .git
        .take()
        .ok_or_else(|| wasmtime::Error::msg("patina git adapter not configured"))?;
    let report = store.data_mut().call_toy(&adapter, &input_json);
    store.data_mut().git = Some(adapter);
    results[0] = match report.outcome {
        MctToyAdapterOutcome::Success => {
            component::Val::Result(Ok(git_ok_payload(&report, shape)?))
        }
        MctToyAdapterOutcome::Failed => component::Val::Result(Err(Some(Box::new(
            component::Val::String(report.safe_message),
        )))),
    };
    Ok(())
}

fn git_ok_payload(
    report: &crate::toy::MctToyCallReport,
    shape: MctWitGitResultShape,
) -> wasmtime::Result<Option<Box<component::Val>>> {
    if shape == MctWitGitResultShape::Unit {
        return Ok(None);
    }
    let output_json = report
        .output_json
        .as_ref()
        .ok_or_else(|| wasmtime::Error::msg("patina git toy returned no output JSON"))?;
    let output: Value = serde_json::from_str(output_json).map_err(|error| {
        wasmtime::Error::msg(format!("patina git toy returned invalid JSON: {error}"))
    })?;
    let ok = output
        .get("ok")
        .ok_or_else(|| wasmtime::Error::msg("patina git toy output missing 'ok' field"))?;
    let value = match shape {
        MctWitGitResultShape::Unit => unreachable!("unit handled before parsing output"),
        MctWitGitResultShape::String => component::Val::String(
            ok.as_str()
                .ok_or_else(|| wasmtime::Error::msg("patina git toy 'ok' field must be a string"))?
                .to_owned(),
        ),
        MctWitGitResultShape::Bool => component::Val::Bool(
            ok.as_bool()
                .ok_or_else(|| wasmtime::Error::msg("patina git toy 'ok' field must be a bool"))?,
        ),
        MctWitGitResultShape::U32 => {
            let value = ok
                .as_u64()
                .ok_or_else(|| wasmtime::Error::msg("patina git toy 'ok' field must be a u32"))?;
            component::Val::U32(
                u32::try_from(value)
                    .map_err(|_| wasmtime::Error::msg("patina git toy 'ok' field exceeds u32"))?,
            )
        }
        MctWitGitResultShape::StringList => {
            let values = ok
                .as_array()
                .ok_or_else(|| wasmtime::Error::msg("patina git toy 'ok' field must be a list"))?;
            let values = values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(|value| component::Val::String(value.to_owned()))
                        .ok_or_else(|| {
                            wasmtime::Error::msg("patina git toy string list contains a non-string")
                        })
                })
                .collect::<wasmtime::Result<Vec<_>>>()?;
            component::Val::List(values)
        }
    };
    Ok(Some(Box::new(value)))
}

fn git_path_list(values: &[component::Val]) -> wasmtime::Result<Vec<String>> {
    values
        .iter()
        .map(|value| match value {
            component::Val::String(path) => Ok(path.clone()),
            _ => Err(wasmtime::Error::msg(
                "patina git path list contains a non-string",
            )),
        })
        .collect()
}

fn lookup_wit_component_func<T>(
    store: &mut Store<T>,
    instance: &component::Instance,
    operation: &MctWitResolvedOperation,
) -> Option<component::Func> {
    let interface_idx =
        instance.get_export_index(store.as_context_mut(), None, &operation.interface)?;
    let function_idx = instance.get_export_index(
        store.as_context_mut(),
        Some(&interface_idx),
        &operation.function,
    )?;
    instance.get_func(store.as_context_mut(), function_idx)
}

fn wasm_observation(
    observation_id: ObservationId,
    observed_at: Timestamp,
    kind: ObservationKind,
    outcome: ObservationOutcome,
    call: &MctCall,
    authorized: &AuthorizedChildInvocation,
    safe_message: &str,
) -> MctObservation {
    MctObservation {
        observation_id,
        observed_at,
        kind,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: Some(authorized.authority_decision_id().clone()),
        subject_id: Some(authorized.child_name().to_owned()),
        resource_id: Some(authorized.child_instance_id().to_string()),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: Some(format!(
            "authorized_child_invocation:{}",
            authorized.authorized_child_invocation_id()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn runtime() -> MctWasmComponentRuntime {
        MctWasmComponentRuntime::new(MctWasmHostConfig {
            memory_limit_bytes: DEFAULT_WASM_MEMORY_LIMIT_BYTES,
        })
        .unwrap()
    }

    fn test_deadline() -> Timestamp {
        let deadline = jiff::Timestamp::now()
            .checked_add(jiff::SignedDuration::from_secs(60))
            .unwrap();
        Timestamp::new(deadline.to_string()).unwrap()
    }

    fn call() -> MctCall {
        MctCall {
            call_id: CallId::new("call-wasm-component")
                .expect("string ID literal/generated value must be non-empty"),
            caller: CallerIdentity {
                node_id: MctNodeId::new("mother-a")
                    .expect("string ID literal/generated value must be non-empty"),
                user_id: None,
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                project_id: None,
            },
            target: OperationTarget {
                namespace: "patina".into(),
                interface_name: "answer".into(),
                function_name: "answer".into(),
            },
            payload_metadata: PayloadMetadata {
                data_classification: "public".into(),
                size_bytes: 0,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 1,
                grants_revision: 1,
                vision_policy_revision: 1,
            },
            deadline: test_deadline(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-wasm-component")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: SpanId::new("span-wasm-component")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            origin: CallOrigin::WasmHost,
        }
    }

    fn authorized() -> AuthorizedChildInvocation {
        crate::authority_test_fixture::authorized_child_for_call(
            &call(),
            "wasm-answer",
            MctNodeId::new("mother-a")
                .expect("string ID literal/generated value must be non-empty"),
            "wasm",
        )
    }

    fn toy_authorized(stem: &str) -> AuthorizedToyCall {
        crate::authority_test_fixture::authorized_toy_for_call(
            &call(),
            "toy-echo",
            ChildInstanceId::new("instance-wasm")
                .expect("string ID literal/generated value must be non-empty"),
            "use",
            stem,
        )
    }

    fn toy_ids() -> MctToyCallIds {
        MctToyCallIds {
            started_observation_id: ObservationId::new("obs-wasm-toy-started")
                .expect("string ID literal/generated value must be non-empty"),
            completed_observation_id: ObservationId::new("obs-wasm-toy-completed")
                .expect("string ID literal/generated value must be non-empty"),
            started_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            completed_at: Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
        }
    }

    fn ids() -> MctWasmComponentInvocationIds {
        MctWasmComponentInvocationIds {
            started_observation_id: ObservationId::new("obs-wasm-started")
                .expect("string ID literal/generated value must be non-empty"),
            completed_observation_id: ObservationId::new("obs-wasm-completed")
                .expect("string ID literal/generated value must be non-empty"),
            audit_ref: AuditRef::new("audit-wasm")
                .expect("string ID literal/generated value must be non-empty"),
            started_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            completed_at: Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
        }
    }

    fn diagnostic_ids() -> MctWasmComponentDiagnosticIds {
        MctWasmComponentDiagnosticIds {
            observation_id: ObservationId::new("obs-wasm-trap")
                .expect("string ID literal/generated value must be non-empty"),
            observed_at: Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
        }
    }

    fn typed_call(function_name: &str) -> MctCall {
        let mut call = call();
        call.target = OperationTarget {
            namespace: "patina:demo".into(),
            interface_name: "control@0.1.0".into(),
            function_name: function_name.into(),
        };
        call
    }

    fn slate_call(function_name: &str) -> MctCall {
        let mut call = call();
        call.target = OperationTarget {
            namespace: "patina:slate".into(),
            interface_name: "control@0.1.0".into(),
            function_name: function_name.into(),
        };
        call
    }

    fn typed_component_path(dir: &tempfile::TempDir) -> PathBuf {
        let component_wat = r#"
(component
  (core module $m
    (func $double (export "double") (param i32) (result i32)
      local.get 0
      i32.const 2
      i32.mul))
  (core instance $i (instantiate $m))
  (func $double (param "value" s32) (result s32) (canon lift (core func $i "double")))
  (instance $control (export "double" (func $double)))
  (export "patina:demo/control@0.1.0" (instance $control)))
"#;
        let component_path = dir.path().join("typed.component.wasm");
        fs::write(&component_path, wat::parse_str(component_wat).unwrap()).unwrap();
        component_path
    }

    fn typed_importing_component_path(dir: &tempfile::TempDir) -> PathBuf {
        let component_wat = r#"
(component
  (import "patina:measure/measure@0.1.0" (instance $measure
    (export "counter" (func (param "name" string) (param "delta" float64)))))
  (core module $m
    (func $double (export "double") (param i32) (result i32)
      local.get 0
      i32.const 2
      i32.mul))
  (core instance $i (instantiate $m))
  (func $double (param "value" s32) (result s32) (canon lift (core func $i "double")))
  (instance $control (export "double" (func $double)))
  (export "patina:demo/control@0.1.0" (instance $control)))
"#;
        let component_path = dir.path().join("typed-importing.component.wasm");
        fs::write(&component_path, wat::parse_str(component_wat).unwrap()).unwrap();
        component_path
    }

    fn typed_record_component_path(dir: &tempfile::TempDir) -> PathBuf {
        let component_wat = r#"
(component
  (core module $m
    (func $summarize (export "summarize") (param i32 i32) (result i32)
      local.get 0
      local.get 1
      i32.add))
  (core instance $i (instantiate $m))
  (type $pair (record (field "left" s32) (field "right" s32)))
  (type $summary (record (field "total" s32)))
  (func $summarize (param "pair" $pair) (result $summary) (canon lift (core func $i "summarize")))
  (instance $control
    (export "pair" (type $pair))
    (export "summary" (type $summary))
    (export "summarize" (func $summarize) (func (param "pair" $pair) (result $summary))))
  (export "patina:demo/control@0.1.0" (instance $control)))
"#;
        let component_path = dir.path().join("typed-record.component.wasm");
        fs::write(&component_path, wat::parse_str(component_wat).unwrap()).unwrap();
        component_path
    }

    fn slate_list_work_component_path(dir: &tempfile::TempDir) -> PathBuf {
        let component_wat = r#"
(component
  (core module $m
    (memory (export "memory") 1)
    (global $heap (mut i32) (i32.const 1024))
    (func $realloc (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
      global.get $heap
      global.get $heap
      local.get 3
      i32.add
      global.set $heap)
    (func $list-work (export "list-work")
      (param i32 i32 i32 i32 i32 i32 i32 i32 i32)
      (result i32)
      i32.const 0))
  (core instance $i (instantiate $m))
  (alias core export $i "memory" (core memory $memory))
  (alias core export $i "cabi_realloc" (core func $realloc))
  (type $work-list-request (record
    (field "project" (option string))
    (field "status" (option string))
    (field "kind" (option string))))
  (type $work-summary (record
    (field "id" string)
    (field "title" string)
    (field "kind" string)
    (field "status" string)
    (field "path" string)))
  (type $list-work-result (result (list $work-summary) (error string)))
  (func $list-work (param "req" $work-list-request) (result $list-work-result)
    (canon lift (core func $i "list-work") (memory $memory) (realloc $realloc) string-encoding=utf8))
  (instance $control
    (export "work-list-request" (type $work-list-request))
    (export "work-summary" (type $work-summary))
    (export "list-work-result" (type $list-work-result))
    (export "list-work" (func $list-work) (func (param "req" $work-list-request) (result $list-work-result))))
  (export "patina:slate/control@0.1.0" (instance $control)))
"#;
        let component_path = dir.path().join("slate-list-work.component.wasm");
        fs::write(&component_path, wat::parse_str(component_wat).unwrap()).unwrap();
        component_path
    }

    fn typed_logging_component_path(dir: &tempfile::TempDir) -> PathBuf {
        let component_wat = r#"
(component
  (import (interface "wasi:logging/logging@0.1.0") (instance $logging
    (type $level' (variant (case "trace") (case "debug") (case "info") (case "warn") (case "error") (case "critical")))
    (export "level" (type $level (eq $level')))
    (export "log" (func (param "level" $level) (param "context" string) (param "message" string)))))
  (alias export $logging "log" (func $log))
  (core module $memory-module
    (memory (export "memory") 1))
  (core instance $memory-instance (instantiate $memory-module))
  (alias core export $memory-instance "memory" (core memory $memory))
  (core func $log-core (canon lower (func $log) (memory $memory) string-encoding=utf8))
  (core module $m
    (import "" "memory" (memory 1))
    (import "" "log" (func $log-import (param i32 i32 i32 i32 i32)))
    (data (i32.const 0) "mct-testhello")
    (func $run (export "run") (result i32)
      i32.const 2
      i32.const 0
      i32.const 8
      i32.const 8
      i32.const 5
      call $log-import
      i32.const 1))
  (core instance $imports (export "memory" (memory $memory)) (export "log" (func $log-core)))
  (core instance $i (instantiate $m (with "" (instance $imports))))
  (func $run (result s32) (canon lift (core func $i "run")))
  (instance $control (export "run" (func $run)))
  (export "patina:demo/control@0.1.0" (instance $control)))
"#;
        let component_path = dir.path().join("typed-logging.component.wasm");
        fs::write(&component_path, wat::parse_str(component_wat).unwrap()).unwrap();
        component_path
    }

    fn typed_measure_component_path(dir: &tempfile::TempDir) -> PathBuf {
        let component_wat = r#"
(component
  (import (interface "patina:measure/measure@0.1.0") (instance $measure
    (export "counter" (func (param "name" string) (param "delta" float64) (result (result))))))
  (alias export $measure "counter" (func $counter))
  (core module $memory-module
    (memory (export "memory") 1))
  (core instance $memory-instance (instantiate $memory-module))
  (alias core export $memory-instance "memory" (core memory $memory))
  (core func $counter-core (canon lower (func $counter) (memory $memory) string-encoding=utf8))
  (core module $m
    (import "" "memory" (memory 1))
    (import "" "counter" (func $counter-import (param i32 i32 f64) (result i32)))
    (data (i32.const 0) "slate_dispatch_calls")
    (func $run (export "run") (result i32)
      i32.const 0
      i32.const 20
      f64.const 1
      call $counter-import
      drop
      i32.const 1))
  (core instance $imports (export "memory" (memory $memory)) (export "counter" (func $counter-core)))
  (core instance $i (instantiate $m (with "" (instance $imports))))
  (func $run (result s32) (canon lift (core func $i "run")))
  (instance $control (export "run" (func $run)))
  (export "patina:demo/control@0.1.0" (instance $control)))
"#;
        let component_path = dir.path().join("typed-measure.component.wasm");
        fs::write(&component_path, wat::parse_str(component_wat).unwrap()).unwrap();
        component_path
    }

    fn wit_host_adapters() -> MctWitHostImportAdapters {
        let mut toy_registry = MctToyAdapterRegistry::new();
        toy_registry.register(
            ToyId::new("toy-echo").expect("string ID literal/generated value must be non-empty"),
            crate::MctToyBackend::EchoJson,
        );
        let adapter = MctWitToyHostAdapter {
            authorized_toy_call: toy_authorized("wasm"),
            observation_id_prefix: "obs-wit-host-toy".into(),
            observed_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
        };
        MctWitHostImportAdapters {
            toy_registry,
            logging: Some(adapter),
            measure: Some(MctWitToyHostAdapter {
                authorized_toy_call: toy_authorized("wasm-measure"),
                observation_id_prefix: "obs-wit-host-toy".into(),
                observed_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            }),
            git: None,
            keyvalue: None,
            messaging: None,
            wasi: None,
        }
    }

    fn typed_wasi_random_import_component_path(dir: &tempfile::TempDir) -> PathBuf {
        let component_wat = r#"
(component
  (import "wasi:random/random@0.2.3" (instance $random
    (export "get-random-bytes" (func (param "len" u64) (result (list u8))))))
  (core module $m
    (func $double (export "double") (param i32) (result i32)
      local.get 0
      i32.const 2
      i32.mul))
  (core instance $i (instantiate $m))
  (func $double (param "value" s32) (result s32) (canon lift (core func $i "double")))
  (instance $control (export "double" (func $double)))
  (export "patina:demo/control@0.1.0" (instance $control)))
"#;
        let component_path = dir.path().join("typed-wasi-random-import.component.wasm");
        fs::write(&component_path, wat::parse_str(component_wat).unwrap()).unwrap();
        component_path
    }

    fn typed_wasi_filesystem_import_component_path(dir: &tempfile::TempDir) -> PathBuf {
        let component_wat = r#"
(component
  (import "wasi:filesystem/preopens@0.2.3" (instance $preopens))
  (core module $m
    (func $double (export "double") (param i32) (result i32)
      local.get 0
      i32.const 2
      i32.mul))
  (core instance $i (instantiate $m))
  (func $double (param "value" s32) (result s32) (canon lift (core func $i "double")))
  (instance $control (export "double" (func $double)))
  (export "patina:demo/control@0.1.0" (instance $control)))
"#;
        let component_path = dir
            .path()
            .join("typed-wasi-filesystem-import.component.wasm");
        fs::write(&component_path, wat::parse_str(component_wat).unwrap()).unwrap();
        component_path
    }

    fn wasi_host_adapters() -> MctWitHostImportAdapters {
        let mut adapters = MctWitHostImportAdapters::none();
        adapters.wasi = Some(MctWasiHostConfig { preopens: vec![] });
        adapters
    }

    fn typed_git_create_tag_component_path(dir: &tempfile::TempDir) -> PathBuf {
        let component_wat = r#"
(component
  (import (interface "patina:git/git@0.1.0") (instance $git
    (export "create-tag" (func (param "name" string) (result (result (error string)))))
    (export "create-tag-at" (func (param "name" string) (param "git-ref" string) (result (result (error string)))))
    (export "delete-tag" (func (param "name" string) (result (result (error string)))))
    (export "tag-exists" (func (param "name" string) (result (result bool (error string)))))
    (export "commit" (func (param "message" string) (result (result string (error string)))))
    (export "log-oneline" (func (param "limit" u32) (result (result (list string) (error string)))))
    (export "diff-stat" (func (result (result string (error string)))))
    (export "status-porcelain" (func (result (result string (error string)))))
    (export "add-paths" (func (param "paths" (list string)) (result (result (error string)))))
    (export "remove-paths" (func (param "paths" (list string)) (result (result (error string)))))
    (export "is-clean-tracked" (func (result (result bool (error string)))))
    (export "commits-behind-upstream" (func (result (result u32 (error string)))))
    (export "is-diverged" (func (result (result bool (error string)))))))
  (alias export $git "create-tag" (func $create-tag))
  (core module $memory-module
    (memory (export "memory") 1)
    (global $heap (mut i32) (i32.const 1024))
    (func $realloc (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
      global.get $heap
      global.get $heap
      local.get 3
      i32.add
      global.set $heap))
  (core instance $memory-instance (instantiate $memory-module))
  (alias core export $memory-instance "memory" (core memory $memory))
  (alias core export $memory-instance "cabi_realloc" (core func $realloc))
  (core func $create-tag-core (canon lower (func $create-tag) (memory $memory) (realloc $realloc) string-encoding=utf8))
  (core module $m
    (import "" "memory" (memory 1))
    (import "" "create-tag" (func $create-tag-import (param i32 i32 i32)))
    (data (i32.const 0) "mct-test-tag")
    (func $run (export "run") (result i32)
      i32.const 0
      i32.const 12
      i32.const 100
      call $create-tag-import
      i32.const 1))
  (core instance $imports (export "memory" (memory $memory)) (export "create-tag" (func $create-tag-core)))
  (core instance $i (instantiate $m (with "" (instance $imports))))
  (func $run (result s32) (canon lift (core func $i "run")))
  (instance $control (export "run" (func $run)))
  (export "patina:demo/control@0.1.0" (instance $control)))
"#;
        let component_path = dir.path().join("typed-git-create-tag.component.wasm");
        fs::write(&component_path, wat::parse_str(component_wat).unwrap()).unwrap();
        component_path
    }

    fn git_authorized() -> AuthorizedToyCall {
        crate::authority_test_fixture::authorized_toy_for_call(
            &call(),
            "toy-git",
            ChildInstanceId::new("instance-wasm")
                .expect("string ID literal/generated value must be non-empty"),
            "use",
            "wasm-git",
        )
    }

    fn git_host_adapters(repo_root: PathBuf) -> MctWitHostImportAdapters {
        let mut toy_registry = MctToyAdapterRegistry::new();
        toy_registry.register(
            ToyId::new("toy-git").expect("string ID literal/generated value must be non-empty"),
            crate::MctToyBackend::GitCommand { repo_root },
        );
        MctWitHostImportAdapters {
            toy_registry,
            logging: None,
            measure: None,
            git: Some(MctWitToyHostAdapter {
                authorized_toy_call: git_authorized(),
                observation_id_prefix: "obs-wit-git-toy".into(),
                observed_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            }),
            keyvalue: None,
            messaging: None,
            wasi: None,
        }
    }

    fn init_git_repo() -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        run_git_for_test(repo.path(), &["init"]);
        run_git_for_test(repo.path(), &["config", "user.name", "MCT Test"]);
        run_git_for_test(repo.path(), &["config", "user.email", "mct@example.com"]);
        fs::write(repo.path().join("README.md"), "mct\n").unwrap();
        run_git_for_test(repo.path(), &["add", "README.md"]);
        run_git_for_test(repo.path(), &["commit", "-m", "init"]);
        repo
    }

    fn run_git_for_test(repo_root: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .current_dir(repo_root)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_PREFIX")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn loaded_typed_child(
        component_path: PathBuf,
        allowed_operations: Vec<String>,
    ) -> MctLoadedChild {
        MctLoadedChild {
            child_id: ChildId::new("child:wasm-answer")
                .expect("string ID literal/generated value must be non-empty"),
            name: "wasm-answer".into(),
            version: "0.1.0".into(),
            description: None,
            kind: "test".into(),
            role: None,
            wasm_path: component_path.clone(),
            manifest_path: component_path.with_extension("toml"),
            wasm_digest: crate::children::MctChildFileDigest {
                sha256: "wasm-digest".into(),
                sidecar_present: true,
                verified: true,
            },
            manifest_digest: crate::children::MctChildFileDigest {
                sha256: "manifest-digest".into(),
                sidecar_present: true,
                verified: true,
            },
            artifact_id: "artifact-wasm".into(),
            ingress_mode: crate::children::MctChildIngressMode::WitOnly,
            allowed_operations,
            requested_toys: Vec::new(),
            subscribed_streams: Vec::new(),
            relationship_listens: Vec::new(),
            wasm_size_bytes: 0,
            instance_state: crate::children::MctChildInstanceState::Ready,
        }
    }

    #[test]
    fn watch_send_admission_refuses_paths_shape_and_capacity_synchronously() {
        let admission = MctWitWatchMessageAdmission {
            event_classes: BTreeSet::from([WatchEventClass::Created]),
            max_events_per_batch: 1,
        };
        let event = serde_json::json!({
            "watcher": "folder-watch-actor",
            "stream": "source",
            "change_kind": "created",
            "absolute_path": "safe.txt",
            "relative_path": "safe.txt",
            "size_bytes": 3,
            "modified_unix_ms": 1,
            "sha256": "abc",
            "detected_at": "2026-07-21T00:00:00Z"
        });
        let bytes = serde_json::to_vec(&event).unwrap();
        assert!(
            validate_wit_watch_message_admission(
                &admission,
                "patina:watch/events@0.1.0.emit",
                "file-created",
                Some("application/json"),
                &bytes,
                0,
                0,
            )
            .is_ok()
        );

        let mut unsafe_path = event.clone();
        unsafe_path["absolute_path"] = serde_json::json!("../escape");
        unsafe_path["relative_path"] = serde_json::json!("../escape");
        assert_eq!(
            validate_wit_watch_message_admission(
                &admission,
                "patina:watch/events@0.1.0.emit",
                "file-created",
                Some("application/json"),
                &serde_json::to_vec(&unsafe_path).unwrap(),
                0,
                0,
            ),
            Err(MctWitWatchMessageRefusal::SafePath)
        );

        let mut mismatched = event.clone();
        mismatched["absolute_path"] = serde_json::json!("other.txt");
        assert_eq!(
            validate_wit_watch_message_admission(
                &admission,
                "patina:watch/events@0.1.0.emit",
                "file-created",
                Some("application/json"),
                &serde_json::to_vec(&mismatched).unwrap(),
                0,
                0,
            ),
            Err(MctWitWatchMessageRefusal::LegacyPathEquality)
        );

        let mut unknown_field = event.clone();
        unknown_field["ambient_path"] = serde_json::json!("/private/source/safe.txt");
        assert_eq!(
            validate_wit_watch_message_admission(
                &admission,
                "patina:watch/events@0.1.0.emit",
                "file-created",
                Some("application/json"),
                &serde_json::to_vec(&unknown_field).unwrap(),
                0,
                0,
            ),
            Err(MctWitWatchMessageRefusal::EventShape)
        );
        assert_eq!(
            validate_wit_watch_message_admission(
                &admission,
                "patina:watch/events@0.1.0.emit",
                "file-created",
                Some("application/json"),
                &bytes,
                0,
                1,
            ),
            Err(MctWitWatchMessageRefusal::BatchCapacity)
        );
        assert_eq!(
            validate_wit_watch_message_admission(
                &admission,
                "patina:watch/events@0.1.0.emit",
                "file-modified",
                Some("application/json"),
                &bytes,
                0,
                0,
            ),
            Err(MctWitWatchMessageRefusal::EventClass)
        );
    }

    #[test]
    fn source_derived_watcher_executes_with_explicit_bounded_host_adapters() {
        let root = tempfile::tempdir().unwrap();
        let service = tempfile::tempdir().unwrap();
        let state_path = service.path().join("watch-state.sqlite3");
        MctRuntimeStateStore::open(&state_path).unwrap();
        let component_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/folder-watch-actor-0.1.0/folder-watch-actor.wasm");
        let mut watcher_call = typed_call("status");
        watcher_call.target =
            OperationTarget::new("patina:watch", "control@0.1.0", "status").unwrap();
        let child = MctLoadedChild {
            child_id: ChildId::new("child:folder-watch-actor").unwrap(),
            name: "folder-watch-actor".into(),
            version: "0.1.0".into(),
            description: None,
            kind: "child".into(),
            role: Some("app".into()),
            wasm_path: component_path.clone(),
            manifest_path: component_path.with_file_name("child.toml"),
            wasm_digest: crate::children::MctChildFileDigest {
                sha256: "fixture".into(),
                sidecar_present: false,
                verified: true,
            },
            manifest_digest: crate::children::MctChildFileDigest {
                sha256: "fixture".into(),
                sidecar_present: false,
                verified: true,
            },
            artifact_id: "artifact-watch-fixture".into(),
            ingress_mode: crate::children::MctChildIngressMode::WitOnly,
            allowed_operations: vec![
                "patina:watch/control@0.1.0.status".into(),
                "patina:watch/control@0.1.0.configure".into(),
                "patina:watch/control@0.1.0.scan-now".into(),
            ],
            requested_toys: Vec::new(),
            subscribed_streams: Vec::new(),
            relationship_listens: Vec::new(),
            wasm_size_bytes: std::fs::metadata(&component_path).unwrap().len(),
            instance_state: crate::children::MctChildInstanceState::Ready,
        };
        let make_adapters = |call: &MctCall| {
            let mut toy_registry = MctToyAdapterRegistry::new();
            toy_registry.register(
                ToyId::new("toy-echo").unwrap(),
                crate::MctToyBackend::EchoJson,
            );
            let adapter = |stem: &str| MctWitToyHostAdapter {
                authorized_toy_call: crate::authority_test_fixture::authorized_toy_for_call(
                    call,
                    "toy-echo",
                    ChildInstanceId::new("instance-watch-fixture").unwrap(),
                    "use",
                    stem,
                ),
                observation_id_prefix: format!("obs-{stem}"),
                observed_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            };
            MctWitHostImportAdapters {
                toy_registry,
                logging: Some(adapter("watch-logging")),
                measure: Some(adapter("watch-measure")),
                git: None,
                keyvalue: Some(MctWitKeyvalueHostAdapter {
                    get: adapter("watch-keyvalue-get"),
                    set: adapter("watch-keyvalue-set"),
                    state_path: state_path.clone(),
                    bucket_identifier: "default".into(),
                    bucket_resource_id: "fixture:bucket:default".into(),
                }),
                messaging: Some(MctWitMessagingHostAdapter {
                    toy: adapter("watch-messaging"),
                    watch_admission: MctWitWatchMessageAdmission {
                        event_classes: BTreeSet::from([
                            WatchEventClass::Created,
                            WatchEventClass::Modified,
                            WatchEventClass::Deleted,
                        ]),
                        max_events_per_batch: MCT_WATCH_MAX_EVENTS_PER_BATCH,
                    },
                }),
                wasi: Some(MctWasiHostConfig {
                    preopens: vec![MctWasiPreopen {
                        host_path: root.path().to_path_buf(),
                        guest_path: "/input".into(),
                        access: MctWasiPreopenAccess::ReadOnly,
                    }],
                }),
            }
        };
        let invoke = |call: &MctCall, args: serde_json::Value, stem: &str| {
            runtime()
                .invoke_authorized_child_wit_export_with_host_adapters(
                    crate::authority_test_fixture::authorized_child_for_call(
                        call,
                        "folder-watch-actor",
                        call.caller.node_id.clone(),
                        stem,
                    ),
                    &child,
                    call,
                    &args,
                    make_adapters(call),
                    ids(),
                )
                .unwrap()
        };

        let status = invoke(&watcher_call, serde_json::json!([]), "watch-status");
        assert_eq!(status.result.outcome, ResultOutcome::Success);
        assert_eq!(status.output_json["results"][0]["ticks"], 0);

        let mut configure_call = watcher_call.clone();
        configure_call.call_id = CallId::new("call-watch-configure").unwrap();
        configure_call.target.function_name = "configure".into();
        let configured = invoke(
            &configure_call,
            serde_json::json!([{
                "watch-path": "/input",
                "stream-name": "patina:watch/events@0.1.0.emit",
                "recursive": true,
                "include-hidden": false,
                "emit-existing-on-start": true,
                "extensions": []
            }, true]),
            "watch-configure",
        );
        assert_eq!(configured.result.outcome, ResultOutcome::Success);
        std::fs::write(root.path().join("created.txt"), b"fixture content").unwrap();

        let mut scan_call = watcher_call.clone();
        scan_call.call_id = CallId::new("call-watch-scan").unwrap();
        scan_call.target.function_name = "scan-now".into();
        let scanned = invoke(&scan_call, serde_json::json!([]), "watch-scan");
        assert_eq!(scanned.result.outcome, ResultOutcome::Success);
        assert_eq!(scanned.produced_messages.len(), 1);
        let event: serde_json::Value =
            serde_json::from_slice(&scanned.produced_messages[0].data).unwrap();
        assert_eq!(event["relative_path"], "created.txt");
        assert_eq!(event["absolute_path"], "created.txt");
    }

    #[test]
    fn exact_watch_null_sink_executes_without_watch_or_filesystem_authority() {
        let component_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/watch-null-sink-0.1.0/watch-null-sink.wasm");
        let mut sink_call = typed_call("emit");
        sink_call.call_id = CallId::new("call-watch-sink").unwrap();
        sink_call.target = OperationTarget::new("patina:watch", "events@0.1.0", "emit").unwrap();
        let child = MctLoadedChild {
            child_id: ChildId::new("child:watch-null-sink").unwrap(),
            name: "watch-null-sink".into(),
            version: "0.1.0".into(),
            description: None,
            kind: "child".into(),
            role: Some("app".into()),
            wasm_path: component_path.clone(),
            manifest_path: component_path.with_file_name("child.toml"),
            wasm_digest: crate::children::MctChildFileDigest {
                sha256: "fixture".into(),
                sidecar_present: false,
                verified: true,
            },
            manifest_digest: crate::children::MctChildFileDigest {
                sha256: "fixture".into(),
                sidecar_present: false,
                verified: true,
            },
            artifact_id: "artifact-watch-sink-fixture".into(),
            ingress_mode: crate::children::MctChildIngressMode::WitOnly,
            allowed_operations: vec!["patina:watch/events@0.1.0.emit".into()],
            requested_toys: Vec::new(),
            subscribed_streams: Vec::new(),
            relationship_listens: Vec::new(),
            wasm_size_bytes: std::fs::metadata(&component_path).unwrap().len(),
            instance_state: crate::children::MctChildInstanceState::Ready,
        };
        let adapter = |stem: &str| MctWitToyHostAdapter {
            authorized_toy_call: crate::authority_test_fixture::authorized_toy_for_call(
                &sink_call,
                "toy-echo",
                ChildInstanceId::new("instance-watch-sink").unwrap(),
                "use",
                stem,
            ),
            observation_id_prefix: format!("obs-{stem}"),
            observed_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
        };
        let mut toy_registry = MctToyAdapterRegistry::new();
        toy_registry.register(
            ToyId::new("toy-echo").unwrap(),
            crate::MctToyBackend::EchoJson,
        );
        let report = runtime()
            .invoke_authorized_child_wit_export_with_host_adapters(
                crate::authority_test_fixture::authorized_child_for_call(
                    &sink_call,
                    "watch-null-sink",
                    sink_call.caller.node_id.clone(),
                    "watch-sink",
                ),
                &child,
                &sink_call,
                &serde_json::json!([{
                    "watcher": "folder-watch-actor",
                    "stream-name": "patina:watch/events@0.1.0.emit",
                    "change-kind": "created",
                    "absolute-path": "created.txt",
                    "relative-path": "created.txt",
                    "size-bytes": 15,
                    "modified-unix-ms": 1,
                    "sha256": format!("sha256:{}", "a".repeat(64)),
                    "detected-at": "2026-05-31T00:00:00Z"
                }]),
                MctWitHostImportAdapters {
                    toy_registry,
                    logging: Some(adapter("sink-logging")),
                    measure: Some(adapter("sink-measure")),
                    git: None,
                    keyvalue: None,
                    messaging: None,
                    wasi: Some(MctWasiHostConfig {
                        preopens: Vec::new(),
                    }),
                },
                ids(),
            )
            .unwrap();

        assert_eq!(report.result.outcome, ResultOutcome::Success);
        assert!(report.output_json["results"][0].get("ok").is_some());
        assert!(report.produced_messages.is_empty());
    }

    #[test]
    fn mct_wit_runtime_resolves_versioned_component_export() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_component_path(&dir);

        let operation = resolve_wit_operation_target(&typed_call("double").target).unwrap();
        let exported = runtime.discover_wit_operations(&component_path).unwrap();

        assert_eq!(operation.operation_id, "patina:demo/control@0.1.0.double");
        assert_eq!(operation.interface, "patina:demo/control@0.1.0");
        assert_eq!(operation.function, "double");
        assert!(exported.contains(&operation.operation_id));
    }

    #[test]
    fn mct_wit_runtime_rejects_unimplemented_host_import() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_importing_component_path(&dir);
        let imports = runtime.discover_wit_imports(&component_path).unwrap();
        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.double".into()],
        );

        assert!(imports.contains("patina:measure/measure@0.1.0"));
        let error = runtime
            .invoke_authorized_child_wit_export(
                authorized(),
                &child,
                &typed_call("double"),
                &serde_json::json!([7]),
                ids(),
            )
            .unwrap_err();

        assert!(matches!(
            error,
            MctWasmComponentRuntimeError::UnsupportedWitHostImport { .. }
        ));
    }

    #[test]
    fn mct_wit_runtime_invokes_authorized_logging_import() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_logging_component_path(&dir);
        let child =
            loaded_typed_child(component_path, vec!["patina:demo/control@0.1.0.run".into()]);

        let report = runtime
            .invoke_authorized_child_wit_export_with_host_adapters(
                authorized(),
                &child,
                &typed_call("run"),
                &serde_json::json!([]),
                wit_host_adapters(),
                ids(),
            )
            .unwrap();

        assert_eq!(report.output_json, serde_json::json!({"results": [1]}));
        assert_eq!(report.result.outcome, ResultOutcome::Success);
        assert_eq!(report.observations.len(), 4);
        assert_eq!(report.observations[1].kind, ObservationKind::ToyCallStarted);
        assert_eq!(
            report.observations[2].kind,
            ObservationKind::ToyCallCompleted
        );
        assert_eq!(
            report.observations[3].kind,
            ObservationKind::RuntimeExecutionCompleted
        );
    }

    #[test]
    fn mct_wit_runtime_invokes_authorized_measure_import() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_measure_component_path(&dir);
        let child =
            loaded_typed_child(component_path, vec!["patina:demo/control@0.1.0.run".into()]);

        let report = runtime
            .invoke_authorized_child_wit_export_with_host_adapters(
                authorized(),
                &child,
                &typed_call("run"),
                &serde_json::json!([]),
                wit_host_adapters(),
                ids(),
            )
            .unwrap();

        assert_eq!(report.output_json, serde_json::json!({"results": [1]}));
        assert_eq!(report.result.outcome, ResultOutcome::Success);
        assert_eq!(report.observations[1].kind, ObservationKind::ToyCallStarted);
        assert_eq!(
            report.observations[2].kind,
            ObservationKind::ToyCallCompleted
        );
    }

    #[test]
    fn mct_wit_runtime_invokes_configured_wasi_import() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_wasi_random_import_component_path(&dir);
        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.double".into()],
        );

        let report = runtime
            .invoke_authorized_child_wit_export_with_host_adapters(
                authorized(),
                &child,
                &typed_call("double"),
                &serde_json::json!([7]),
                wasi_host_adapters(),
                ids(),
            )
            .unwrap();

        assert_eq!(report.output_json, serde_json::json!({"results": [14]}));
        assert_eq!(report.result.outcome, ResultOutcome::Success);
    }

    #[test]
    fn mct_wit_runtime_exposes_no_ambient_directory_without_preopen() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_wasi_filesystem_import_component_path(&dir);
        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.double".into()],
        );

        let report = runtime
            .invoke_authorized_child_wit_export_with_host_adapters(
                authorized(),
                &child,
                &typed_call("double"),
                &serde_json::json!([7]),
                wasi_host_adapters(),
                ids(),
            )
            .unwrap();

        assert_eq!(report.output_json, serde_json::json!({"results": [14]}));
    }

    #[test]
    fn mct_wit_runtime_invokes_authorized_git_import() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let repo = init_git_repo();
        let component_path = typed_git_create_tag_component_path(&dir);
        let child =
            loaded_typed_child(component_path, vec!["patina:demo/control@0.1.0.run".into()]);

        let report = runtime
            .invoke_authorized_child_wit_export_with_host_adapters(
                authorized(),
                &child,
                &typed_call("run"),
                &serde_json::json!([]),
                git_host_adapters(repo.path().to_path_buf()),
                ids(),
            )
            .unwrap();

        assert_eq!(report.output_json, serde_json::json!({"results": [1]}));
        assert_eq!(report.result.outcome, ResultOutcome::Success);
        assert_eq!(report.observations[1].kind, ObservationKind::ToyCallStarted);
        assert_eq!(
            report.observations[2].kind,
            ObservationKind::ToyCallCompleted
        );
        let tags = std::process::Command::new("git")
            .current_dir(repo.path())
            .args(["tag", "--list", "mct-test-tag"])
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&tags.stdout).contains("mct-test-tag"));
    }

    #[test]
    fn mct_wit_runtime_rejects_configured_unknown_host_import() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_importing_component_path(&dir);
        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.double".into()],
        );
        let mut host_adapters = wit_host_adapters();
        host_adapters.measure = None;

        let error = runtime
            .invoke_authorized_child_wit_export_with_host_adapters(
                authorized(),
                &child,
                &typed_call("double"),
                &serde_json::json!([7]),
                host_adapters,
                ids(),
            )
            .unwrap_err();

        assert!(matches!(
            error,
            MctWasmComponentRuntimeError::UnsupportedWitHostImport { .. }
        ));
    }

    #[test]
    fn mct_wit_runtime_invokes_typed_component_export() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_component_path(&dir);

        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.double".into()],
        );

        let report = runtime
            .invoke_authorized_child_wit_export(
                authorized(),
                &child,
                &typed_call("double"),
                &serde_json::json!([7]),
                ids(),
            )
            .unwrap();

        assert_eq!(report.output_json, serde_json::json!({"results": [14]}));
        assert_eq!(report.result.outcome, ResultOutcome::Success);
        assert_eq!(
            report.observations[0].kind,
            ObservationKind::RuntimeExecutionStarted
        );
        assert_eq!(
            report.observations[1].kind,
            ObservationKind::RuntimeExecutionCompleted
        );
    }

    #[test]
    fn mct_wit_runtime_lowers_record_args_and_lifts_record_result() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_record_component_path(&dir);
        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.summarize".into()],
        );

        let report = runtime
            .invoke_authorized_child_wit_export(
                authorized(),
                &child,
                &typed_call("summarize"),
                &serde_json::json!([{ "left": 4, "right": 5 }]),
                ids(),
            )
            .unwrap();

        assert_eq!(
            report.output_json,
            serde_json::json!({"results": [{"total": 9}]})
        );
    }

    #[test]
    fn slate_manager_list_work_runs_through_mct_wit_runtime() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let component_path = slate_list_work_component_path(&dir);
        let child = loaded_typed_child(
            component_path,
            vec!["patina:slate/control@0.1.0.list-work".into()],
        );

        let exported = runtime.discover_wit_operations(&child.wasm_path).unwrap();
        assert!(exported.contains("patina:slate/control@0.1.0.list-work"));

        let report = runtime
            .invoke_authorized_child_wit_export(
                authorized(),
                &child,
                &slate_call("list-work"),
                &serde_json::json!([{ "project": null, "status": "active", "kind": null }]),
                ids(),
            )
            .unwrap();

        assert_eq!(report.result.outcome, ResultOutcome::Success);
        assert_eq!(
            report.output_json,
            serde_json::json!({"results": [{"ok": []}]})
        );
        assert_eq!(
            report.observations[0].kind,
            ObservationKind::RuntimeExecutionStarted
        );
        assert_eq!(
            report.observations[1].kind,
            ObservationKind::RuntimeExecutionCompleted
        );
    }

    #[test]
    fn mct_wit_runtime_rejects_unexported_operation() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_component_path(&dir);

        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.missing".into()],
        );

        let error = runtime
            .invoke_authorized_child_wit_export(
                authorized(),
                &child,
                &typed_call("missing"),
                &serde_json::json!([]),
                ids(),
            )
            .unwrap_err();

        assert!(matches!(
            error,
            MctWasmComponentRuntimeError::MissingWitOperation { .. }
        ));
    }

    #[test]
    fn mct_wit_runtime_maps_missing_export_to_adapter_observation() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_component_path(&dir);
        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.missing".into()],
        );
        let call = typed_call("missing");
        let diagnostic_authorized = authorized();

        let error = runtime
            .invoke_authorized_child_wit_export(
                authorized(),
                &child,
                &call,
                &serde_json::json!([]),
                ids(),
            )
            .unwrap_err();
        let observation = wasm_component_runtime_error_observation(
            &error,
            &call,
            &diagnostic_authorized,
            diagnostic_ids(),
        )
        .unwrap();

        assert_eq!(observation.kind, ObservationKind::RuntimeExecutionFailed);
        assert_eq!(observation.safe_message, "wasm export missing");
        assert!(
            observation
                .detail_ref
                .as_deref()
                .unwrap()
                .contains("missing_wit_operation:patina:demo/control@0.1.0.missing")
        );
    }

    #[test]
    fn mct_wit_runtime_rejects_non_allowlisted_operation() {
        let runtime = runtime();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_component_path(&dir);
        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.other".into()],
        );

        let error = runtime
            .invoke_authorized_child_wit_export(
                authorized(),
                &child,
                &typed_call("double"),
                &serde_json::json!([7]),
                ids(),
            )
            .unwrap_err();

        assert!(matches!(
            error,
            MctWasmComponentRuntimeError::WitOperationNotAllowed { .. }
        ));
    }

    #[test]
    fn wasm_component_runtime_trap_maps_to_adapter_observation() {
        let component_wat = r#"
(component
  (core module $m
    (func $trap (export "trap") (result i32)
      unreachable))
  (core instance $i (instantiate $m))
  (func $trap (result s32) (canon lift (core func $i "trap")))
  (export "trap" (func $trap)))
"#;
        let dir = tempfile::tempdir().unwrap();
        let component_path = dir.path().join("trap.component.wasm");
        fs::write(&component_path, wat::parse_str(component_wat).unwrap()).unwrap();
        let runtime = runtime();

        let error = runtime
            .invoke_authorized_s32_export(authorized(), &call(), &component_path, "trap", ids())
            .unwrap_err();
        let observation = wasm_component_runtime_error_observation(
            &error,
            &call(),
            &authorized(),
            diagnostic_ids(),
        )
        .unwrap();

        assert!(matches!(error, MctWasmComponentRuntimeError::Call { .. }));
        assert_eq!(observation.kind, ObservationKind::RuntimeExecutionTrapped);
        assert_eq!(observation.source_plane, SourcePlane::Adapter);
        assert_eq!(observation.outcome, ObservationOutcome::Failed);
        assert_eq!(
            observation.call_id,
            Some(
                CallId::new("call-wasm-component")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
        assert_eq!(
            observation.decision_id,
            Some(
                DecisionId::new("decision-child-wasm")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
    }

    #[test]
    fn wasm_component_runtime_records_failed_toy_host_import() {
        let component_wat = r#"
(component
  (import "mct-toy-call" (func $toy (result s32)))
  (core func $toy-core (canon lower (func $toy)))
  (core module $m
    (import "" "toy" (func $toy-import (result i32)))
    (func $run (export "run") (result i32)
      call $toy-import))
  (core instance $imports (export "toy" (func $toy-core)))
  (core instance $i (instantiate $m (with "" (instance $imports))))
  (func $run (result s32) (canon lift (core func $i "run")))
  (export "run" (func $run)))
"#;
        let dir = tempfile::tempdir().unwrap();
        let component_path = dir.path().join("failed-toy.component.wasm");
        fs::write(&component_path, wat::parse_str(component_wat).unwrap()).unwrap();
        let runtime = runtime();
        let mut toy_registry = MctToyAdapterRegistry::new();
        toy_registry.register(
            ToyId::new("toy-echo").expect("string ID literal/generated value must be non-empty"),
            crate::MctToyBackend::StaticFailure {
                safe_message: "toy unavailable".into(),
            },
        );

        let report = runtime
            .invoke_authorized_s32_export_with_toy_imports(
                authorized(),
                &call(),
                MctWasmComponentToyInvocation {
                    component_path: component_path.clone(),
                    export_name: "run".into(),
                    toy_registry,
                    toy_imports: vec![MctWasmToyHostImport {
                        import_name: "mct-toy-call".into(),
                        authorized_toy_call: toy_authorized("wasm-import-fail"),
                        ids: toy_ids(),
                    }],
                    ids: ids(),
                },
            )
            .unwrap();

        assert_eq!(report.returned_s32, -1);
        assert_eq!(report.observations[2].kind, ObservationKind::ToyCallFailed);
        assert_eq!(
            report.observations[2].call_id,
            Some(
                CallId::new("call-wasm-component")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
        assert_eq!(report.observations[2].outcome, ObservationOutcome::Failed);
    }

    #[test]
    fn wasm_component_runtime_invokes_authorized_toy_host_import() {
        let component_wat = r#"
(component
  (import "mct-toy-call" (func $toy (result s32)))
  (core func $toy-core (canon lower (func $toy)))
  (core module $m
    (import "" "toy" (func $toy-import (result i32)))
    (func $run (export "run") (result i32)
      call $toy-import))
  (core instance $imports (export "toy" (func $toy-core)))
  (core instance $i (instantiate $m (with "" (instance $imports))))
  (func $run (result s32) (canon lift (core func $i "run")))
  (export "run" (func $run)))
"#;
        let dir = tempfile::tempdir().unwrap();
        let component_path = dir.path().join("toy.component.wasm");
        fs::write(&component_path, wat::parse_str(component_wat).unwrap()).unwrap();
        let runtime = runtime();
        let mut toy_registry = MctToyAdapterRegistry::new();
        toy_registry.register(
            ToyId::new("toy-echo").expect("string ID literal/generated value must be non-empty"),
            crate::MctToyBackend::EchoJson,
        );

        let report = runtime
            .invoke_authorized_s32_export_with_toy_imports(
                authorized(),
                &call(),
                MctWasmComponentToyInvocation {
                    component_path: component_path.clone(),
                    export_name: "run".into(),
                    toy_registry,
                    toy_imports: vec![MctWasmToyHostImport {
                        import_name: "mct-toy-call".into(),
                        authorized_toy_call: toy_authorized("wasm-import"),
                        ids: toy_ids(),
                    }],
                    ids: ids(),
                },
            )
            .unwrap();

        assert_eq!(report.returned_s32, 1);
        assert_eq!(report.observations.len(), 4);
        assert_eq!(report.observations[1].kind, ObservationKind::ToyCallStarted);
        assert_eq!(
            report.observations[2].kind,
            ObservationKind::ToyCallCompleted
        );
    }

    #[test]
    fn wasm_component_runtime_denies_stale_child_capability_before_load() {
        let runtime = runtime();
        let mut stale_call = call();
        stale_call.authority_context.policy_revision += 1;

        let report = runtime
            .invoke_authorized_s32_export(
                authorized(),
                &stale_call,
                PathBuf::from("/definitely/not/a/component.wasm"),
                "answer",
                ids(),
            )
            .unwrap();

        assert_eq!(report.result.outcome, ResultOutcome::Denied);
        assert_eq!(report.result.route_taken, None);
        assert_eq!(report.observations.len(), 1);
        assert_eq!(report.observations[0].outcome, ObservationOutcome::Denied);
        assert_eq!(
            report.observations[0].kind,
            ObservationKind::RuntimeExecutionFailed
        );
    }

    #[test]
    fn wasm_component_runtime_invokes_authorized_s32_export() {
        let component_wat = r#"
(component
  (core module $m
    (func $answer (export "answer") (result i32)
      i32.const 42))
  (core instance $i (instantiate $m))
  (func $answer (result s32) (canon lift (core func $i "answer")))
  (export "answer" (func $answer)))
"#;
        let dir = tempfile::tempdir().unwrap();
        let component_path = dir.path().join("answer.component.wasm");
        fs::write(&component_path, wat::parse_str(component_wat).unwrap()).unwrap();
        let runtime = runtime();

        let report = runtime
            .invoke_authorized_s32_export(authorized(), &call(), &component_path, "answer", ids())
            .unwrap();

        assert_eq!(report.returned_s32, 42);
        assert_eq!(report.result.outcome, ResultOutcome::Success);
        assert_eq!(
            report.result.route_taken.unwrap().runtime_kind,
            RuntimeKind::WasmComponent
        );
        assert_eq!(
            report.observations[0].kind,
            ObservationKind::RuntimeExecutionStarted
        );
        assert_eq!(
            report.observations[1].kind,
            ObservationKind::RuntimeExecutionCompleted
        );
        assert_eq!(
            report.observations[1].call_id,
            Some(
                CallId::new("call-wasm-component")
                    .expect("string ID literal/generated value must be non-empty")
            )
        );
    }
}

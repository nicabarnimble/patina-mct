use crate::{
    children::{MctLoadedChild, operation_id_from_target},
    toy::{MctToyAdapterOutcome, MctToyAdapterRegistry, MctToyCallIds},
    wit_values::{lift_component_results_to_json, lower_typed_args_for_component},
};
use mct_kernel::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use thiserror::Error;
use wasmtime::{AsContext, AsContextMut, Config, Engine, Store, StoreContextMut, component};

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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctWasmToyHostImport {
    pub import_name: String,
    pub authorized_toy_call: AuthorizedToyCall,
    pub ids: MctToyCallIds,
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
struct MctWasmHostState {
    toy_registry: MctToyAdapterRegistry,
    call: MctCall,
    toy_observations: Vec<MctObservation>,
}

#[derive(Debug, Error)]
pub enum MctWasmComponentRuntimeError {
    #[error("configure wasm component runtime: {0}")]
    Configure(String),
    #[error("load wasm component {path}: {message}")]
    Load { path: PathBuf, message: String },
    #[error("instantiate wasm component {path}: {message}")]
    Instantiate { path: PathBuf, message: String },
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
                authorized.authorized_child_invocation_id
            ),
        ),
        MctWasmComponentRuntimeError::MissingExport { path, export_name } => (
            AdapterDiagnosticKind::WasmMissingExport,
            path.display().to_string(),
            format!(
                "authorized_child_invocation:{}:missing_export:{export_name}",
                authorized.authorized_child_invocation_id
            ),
        ),
        MctWasmComponentRuntimeError::MissingWitOperation { path, operation_id } => (
            AdapterDiagnosticKind::WasmMissingExport,
            path.display().to_string(),
            format!(
                "authorized_child_invocation:{}:missing_wit_operation:{operation_id}",
                authorized.authorized_child_invocation_id
            ),
        ),
        MctWasmComponentRuntimeError::WitValueConversion {
            path, operation_id, ..
        } => (
            AdapterDiagnosticKind::WasmValueConversionFailure,
            path.display().to_string(),
            format!(
                "authorized_child_invocation:{}:wit_value_conversion:{operation_id}",
                authorized.authorized_child_invocation_id
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
            decision_id: Some(authorized.authority_decision_id.clone()),
            subject_id: Some(authorized.child_name.clone()),
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

#[derive(Debug)]
pub struct MctWasmComponentRuntime {
    engine: Engine,
}

impl MctWasmComponentRuntime {
    pub fn new() -> Result<Self, MctWasmComponentRuntimeError> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        let engine = Engine::new(&config)
            .map_err(|error| MctWasmComponentRuntimeError::Configure(error.to_string()))?;
        Ok(Self { engine })
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

    pub fn invoke_authorized_child_wit_export(
        &self,
        authorized: &AuthorizedChildInvocation,
        child: &MctLoadedChild,
        call: &MctCall,
        args_json: &Value,
        ids: MctWasmComponentInvocationIds,
    ) -> Result<MctWitComponentInvocationReport, MctWasmComponentRuntimeError> {
        if authorized.child_name != child.name {
            return Err(MctWasmComponentRuntimeError::AuthorizedChildMismatch {
                authorized_child_name: authorized.child_name.clone(),
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
            ids,
        )
    }

    fn invoke_wit_export_after_contract_check(
        &self,
        authorized: &AuthorizedChildInvocation,
        call: &MctCall,
        component_path: impl AsRef<Path>,
        args_json: &Value,
        ids: MctWasmComponentInvocationIds,
    ) -> Result<MctWitComponentInvocationReport, MctWasmComponentRuntimeError> {
        let component_path = component_path.as_ref().to_path_buf();
        let operation = resolve_wit_operation_target(&call.target)?;
        let started = wasm_observation(
            ids.started_observation_id.clone(),
            ids.started_at.clone(),
            ObservationKind::RuntimeExecutionStarted,
            ObservationOutcome::Started,
            call,
            authorized,
            "wasm component execution started",
        );
        let component =
            component::Component::from_file(&self.engine, &component_path).map_err(|error| {
                MctWasmComponentRuntimeError::Load {
                    path: component_path.clone(),
                    message: error.to_string(),
                }
            })?;
        let linker = component::Linker::<()>::new(&self.engine);
        let mut store = Store::new(&self.engine, ());
        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
                path: component_path.clone(),
                message: error.to_string(),
            })?;
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
        func.call(store.as_context_mut(), &lowered_args, &mut results)
            .map_err(|error| MctWasmComponentRuntimeError::Call {
                path: component_path.clone(),
                export_name: operation.operation_id.clone(),
                message: error.to_string(),
            })?;
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
            authorized,
            "wasm component execution completed",
        );
        let output_size_bytes = serde_json::to_vec(&output_json)
            .map(|bytes| bytes.len() as u64)
            .ok();
        let result = MctResult {
            call_id: call.call_id.clone(),
            outcome: ResultOutcome::Success,
            route_taken: Some(RouteTaken {
                node_id: MctNodeId::from("local-mct"),
                child_id: Some(ChildId::from(authorized.child_name.clone())),
                runtime_kind: RuntimeKind::WasmComponent,
            }),
            authority_decision_ref: authorized.authority_decision_id.clone(),
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: call.payload_metadata.approximate_size_bytes,
                output_size_bytes,
            },
            requester_message: "wasm component completed".into(),
            audit_ref: ids.audit_ref,
        };
        Ok(MctWitComponentInvocationReport {
            result,
            output_json,
            observations: vec![started, completed],
        })
    }

    pub fn invoke_authorized_s32_export(
        &self,
        authorized: &AuthorizedChildInvocation,
        call: &MctCall,
        component_path: impl AsRef<Path>,
        export_name: &str,
        ids: MctWasmComponentInvocationIds,
    ) -> Result<MctWasmComponentInvocationReport, MctWasmComponentRuntimeError> {
        let component_path = component_path.as_ref().to_path_buf();
        let started = wasm_observation(
            ids.started_observation_id.clone(),
            ids.started_at.clone(),
            ObservationKind::RuntimeExecutionStarted,
            ObservationOutcome::Started,
            call,
            authorized,
            "wasm component execution started",
        );
        let component =
            component::Component::from_file(&self.engine, &component_path).map_err(|error| {
                MctWasmComponentRuntimeError::Load {
                    path: component_path.clone(),
                    message: error.to_string(),
                }
            })?;
        let linker = component::Linker::<()>::new(&self.engine);
        let mut store = Store::new(&self.engine, ());
        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
                path: component_path.clone(),
                message: error.to_string(),
            })?;
        let func = instance.get_func(&mut store, export_name).ok_or_else(|| {
            MctWasmComponentRuntimeError::MissingExport {
                path: component_path.clone(),
                export_name: export_name.into(),
            }
        })?;
        let mut results = [component::Val::S32(0)];
        func.call(&mut store, &[], &mut results).map_err(|error| {
            MctWasmComponentRuntimeError::Call {
                path: component_path.clone(),
                export_name: export_name.into(),
                message: error.to_string(),
            }
        })?;
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
            authorized,
            "wasm component execution completed",
        );
        let result = MctResult {
            call_id: call.call_id.clone(),
            outcome: ResultOutcome::Success,
            route_taken: Some(RouteTaken {
                node_id: MctNodeId::from("local-mct"),
                child_id: Some(ChildId::from(authorized.child_name.clone())),
                runtime_kind: RuntimeKind::WasmComponent,
            }),
            authority_decision_ref: authorized.authority_decision_id.clone(),
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: call.payload_metadata.approximate_size_bytes,
                output_size_bytes: Some(std::mem::size_of::<i32>() as u64),
            },
            requester_message: "wasm component completed".into(),
            audit_ref: ids.audit_ref,
        };
        Ok(MctWasmComponentInvocationReport {
            result,
            returned_s32,
            observations: vec![started, completed],
        })
    }

    pub fn invoke_authorized_s32_export_with_toy_imports(
        &self,
        authorized: &AuthorizedChildInvocation,
        call: &MctCall,
        invocation: MctWasmComponentToyInvocation,
    ) -> Result<MctWasmComponentInvocationReport, MctWasmComponentRuntimeError> {
        let component_path = invocation.component_path;
        let export_name = invocation.export_name;
        let ids = invocation.ids;
        let started = wasm_observation(
            ids.started_observation_id.clone(),
            ids.started_at.clone(),
            ObservationKind::RuntimeExecutionStarted,
            ObservationOutcome::Started,
            call,
            authorized,
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
            },
        );
        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(|error| MctWasmComponentRuntimeError::Instantiate {
                path: component_path.clone(),
                message: error.to_string(),
            })?;
        let func = instance.get_func(&mut store, &export_name).ok_or_else(|| {
            MctWasmComponentRuntimeError::MissingExport {
                path: component_path.clone(),
                export_name: export_name.clone(),
            }
        })?;
        let mut results = [component::Val::S32(0)];
        func.call(&mut store, &[], &mut results).map_err(|error| {
            MctWasmComponentRuntimeError::Call {
                path: component_path.clone(),
                export_name: export_name.clone(),
                message: error.to_string(),
            }
        })?;
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
            authorized,
            "wasm component execution completed",
        );
        let mut observations = vec![started];
        observations.extend(store.data().toy_observations.clone());
        observations.push(completed);
        let result = MctResult {
            call_id: call.call_id.clone(),
            outcome: ResultOutcome::Success,
            route_taken: Some(RouteTaken {
                node_id: MctNodeId::from("local-mct"),
                child_id: Some(ChildId::from(authorized.child_name.clone())),
                runtime_kind: RuntimeKind::WasmComponent,
            }),
            authority_decision_ref: authorized.authority_decision_id.clone(),
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: call.payload_metadata.approximate_size_bytes,
                output_size_bytes: Some(std::mem::size_of::<i32>() as u64),
            },
            requester_message: "wasm component completed".into(),
            audit_ref: ids.audit_ref,
        };
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

fn lookup_wit_component_func(
    store: &mut Store<()>,
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
        decision_id: Some(authorized.authority_decision_id.clone()),
        subject_id: Some(authorized.child_name.clone()),
        resource_id: Some(authorized.child_instance_id.to_string()),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: Some(format!(
            "authorized_child_invocation:{}",
            authorized.authorized_child_invocation_id
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn call() -> MctCall {
        MctCall {
            call_id: CallId::from("call-wasm-component"),
            caller: CallerIdentity {
                node_id: MctNodeId::from("mother-a"),
                user_id: None,
                vision_id: VisionId::from("vision-a"),
                project_id: None,
            },
            target: OperationTarget {
                namespace: "patina".into(),
                interface_name: "answer".into(),
                function_name: "answer".into(),
            },
            payload_metadata: PayloadMetadata {
                data_classification: "public".into(),
                approximate_size_bytes: 0,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 1,
                grants_revision: 1,
                vision_policy_revision: 1,
            },
            deadline: Timestamp::from("2026-05-31T00:01:00Z"),
            trace_context: TraceContext {
                trace_id: TraceId::from("trace-wasm-component"),
                span_id: SpanId::from("span-wasm-component"),
            },
            origin: CallOrigin::WasmHost,
        }
    }

    fn authorized() -> AuthorizedChildInvocation {
        AuthorizedChildInvocation {
            authorized_child_invocation_id: AuthorizedChildInvocationId::from("auth-wasm"),
            call_id: CallId::from("call-wasm-component"),
            evaluation_id: ChildCallEvaluationId::from("eval-wasm"),
            assignment_id: ChildAssignmentId::from("assignment-wasm"),
            approval_id: ChildApprovalId::from("approval-wasm"),
            artifact_id: ComponentArtifactId::from("artifact-wasm"),
            child_instance_id: ChildInstanceId::from("instance-wasm"),
            child_name: "wasm-answer".into(),
            authority_decision_id: DecisionId::from("decision-wasm"),
        }
    }

    fn toy_authorized() -> AuthorizedToyCall {
        AuthorizedToyCall {
            authorized_toy_call_id: AuthorizedToyCallId::from("auth-toy-wasm"),
            call_id: CallId::from("call-wasm-component"),
            evaluation_id: ToyGrantEvaluationId::from("eval-toy-wasm"),
            grant_id: ToyGrantId::from("grant-toy-wasm"),
            toy_id: ToyId::from("toy-echo"),
            child_instance_id: ChildInstanceId::from("instance-wasm"),
            authority_decision_id: DecisionId::from("decision-toy-wasm"),
            expires_at: Timestamp::from("2026-05-31T00:10:00Z"),
        }
    }

    fn toy_ids() -> MctToyCallIds {
        MctToyCallIds {
            started_observation_id: ObservationId::from("obs-wasm-toy-started"),
            completed_observation_id: ObservationId::from("obs-wasm-toy-completed"),
            started_at: Timestamp::from("2026-05-31T00:00:00Z"),
            completed_at: Timestamp::from("2026-05-31T00:00:01Z"),
        }
    }

    fn ids() -> MctWasmComponentInvocationIds {
        MctWasmComponentInvocationIds {
            started_observation_id: ObservationId::from("obs-wasm-started"),
            completed_observation_id: ObservationId::from("obs-wasm-completed"),
            audit_ref: AuditRef::from("audit-wasm"),
            started_at: Timestamp::from("2026-05-31T00:00:00Z"),
            completed_at: Timestamp::from("2026-05-31T00:00:01Z"),
        }
    }

    fn diagnostic_ids() -> MctWasmComponentDiagnosticIds {
        MctWasmComponentDiagnosticIds {
            observation_id: ObservationId::from("obs-wasm-trap"),
            observed_at: Timestamp::from("2026-05-31T00:00:02Z"),
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

    fn loaded_typed_child(
        component_path: PathBuf,
        allowed_operations: Vec<String>,
    ) -> MctLoadedChild {
        MctLoadedChild {
            child_id: ChildId::from("child:wasm-answer"),
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
    fn mct_wit_runtime_resolves_versioned_component_export() {
        let runtime = MctWasmComponentRuntime::new().unwrap();
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
    fn mct_wit_runtime_invokes_typed_component_export() {
        let runtime = MctWasmComponentRuntime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_component_path(&dir);

        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.double".into()],
        );

        let report = runtime
            .invoke_authorized_child_wit_export(
                &authorized(),
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
        let runtime = MctWasmComponentRuntime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_record_component_path(&dir);
        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.summarize".into()],
        );

        let report = runtime
            .invoke_authorized_child_wit_export(
                &authorized(),
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
    fn mct_wit_runtime_rejects_unexported_operation() {
        let runtime = MctWasmComponentRuntime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_component_path(&dir);

        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.missing".into()],
        );

        let error = runtime
            .invoke_authorized_child_wit_export(
                &authorized(),
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
        let runtime = MctWasmComponentRuntime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_component_path(&dir);
        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.missing".into()],
        );
        let call = typed_call("missing");
        let authorized = authorized();

        let error = runtime
            .invoke_authorized_child_wit_export(
                &authorized,
                &child,
                &call,
                &serde_json::json!([]),
                ids(),
            )
            .unwrap_err();
        let observation =
            wasm_component_runtime_error_observation(&error, &call, &authorized, diagnostic_ids())
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
        let runtime = MctWasmComponentRuntime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let component_path = typed_component_path(&dir);
        let child = loaded_typed_child(
            component_path,
            vec!["patina:demo/control@0.1.0.other".into()],
        );

        let error = runtime
            .invoke_authorized_child_wit_export(
                &authorized(),
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
        let runtime = MctWasmComponentRuntime::new().unwrap();

        let error = runtime
            .invoke_authorized_s32_export(&authorized(), &call(), &component_path, "trap", ids())
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
            Some(CallId::from("call-wasm-component"))
        );
        assert_eq!(
            observation.decision_id,
            Some(DecisionId::from("decision-wasm"))
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
        let runtime = MctWasmComponentRuntime::new().unwrap();
        let mut toy_registry = MctToyAdapterRegistry::new();
        toy_registry.register(
            ToyId::from("toy-echo"),
            crate::MctToyBackend::StaticFailure {
                safe_message: "toy unavailable".into(),
            },
        );

        let report = runtime
            .invoke_authorized_s32_export_with_toy_imports(
                &authorized(),
                &call(),
                MctWasmComponentToyInvocation {
                    component_path: component_path.clone(),
                    export_name: "run".into(),
                    toy_registry,
                    toy_imports: vec![MctWasmToyHostImport {
                        import_name: "mct-toy-call".into(),
                        authorized_toy_call: toy_authorized(),
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
            Some(CallId::from("call-wasm-component"))
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
        let runtime = MctWasmComponentRuntime::new().unwrap();
        let mut toy_registry = MctToyAdapterRegistry::new();
        toy_registry.register(ToyId::from("toy-echo"), crate::MctToyBackend::EchoJson);

        let report = runtime
            .invoke_authorized_s32_export_with_toy_imports(
                &authorized(),
                &call(),
                MctWasmComponentToyInvocation {
                    component_path: component_path.clone(),
                    export_name: "run".into(),
                    toy_registry,
                    toy_imports: vec![MctWasmToyHostImport {
                        import_name: "mct-toy-call".into(),
                        authorized_toy_call: toy_authorized(),
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
        let runtime = MctWasmComponentRuntime::new().unwrap();

        let report = runtime
            .invoke_authorized_s32_export(&authorized(), &call(), &component_path, "answer", ids())
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
            Some(CallId::from("call-wasm-component"))
        );
    }
}

use crate::toy::{MctToyAdapterOutcome, MctToyAdapterRegistry, MctToyCallIds};
use mct_kernel::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use wasmtime::{Config, Engine, Store, StoreContextMut, component};

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
    #[error("call wasm component export '{export_name}' in {path}: {message}")]
    Call {
        path: PathBuf,
        export_name: String,
        message: String,
    },
    #[error("wasm component export '{export_name}' returned unexpected value")]
    UnexpectedResult { export_name: String },
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

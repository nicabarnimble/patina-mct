//! M4/L15 counterexamples and positive controls at the actual Wasmtime boundary.
use super::*;
use sha2::{Digest, Sha256};

fn noisy_component(
    dir: &tempfile::TempDir,
    measure: bool,
    count: usize,
    message_bytes: usize,
) -> PathBuf {
    noisy_component_with_start(dir, measure, count, message_bytes, false)
}

fn noisy_component_with_start(
    dir: &tempfile::TempDir,
    measure: bool,
    count: usize,
    message_bytes: usize,
    initialize: bool,
) -> PathBuf {
    let initializer = if initialize {
        "(func $init call $run drop) (start $init)"
    } else {
        ""
    };
    let (imports, signature, invoke) = if measure {
        (
            r#"(import (interface "patina:measure/measure@0.1.0") (instance $host
            (export "counter" (func (param "name" string) (param "delta" float64) (result (result))))))
            (alias export $host "counter" (func $host-func))"#,
            "(param i32 i32 f64) (result i32)",
            format!("i32.const 0 i32.const {message_bytes} f64.const 1 call $host-import drop"),
        )
    } else {
        (
            r#"(import (interface "wasi:logging/logging@0.1.0") (instance $host
            (type $level' (variant (case "trace") (case "debug") (case "info") (case "warn") (case "error") (case "critical")))
            (export "level" (type $level (eq $level')))
            (export "log" (func (param "level" $level) (param "context" string) (param "message" string)))))
            (alias export $host "log" (func $host-func))"#,
            "(param i32 i32 i32 i32 i32)",
            format!(
                "i32.const 2 i32.const 0 i32.const 0 i32.const 0 i32.const {message_bytes} call $host-import"
            ),
        )
    };
    let body = format!("{invoke}\n").repeat(count);
    let wat = format!(
        r#"(component {imports}
        (core module $mem (memory (export "memory") 2))
        (core instance $mi (instantiate $mem))
        (alias core export $mi "memory" (core memory $memory))
        (core func $host-core (canon lower (func $host-func) (memory $memory) string-encoding=utf8))
        (core module $m
            (import "" "memory" (memory 2))
            (import "" "host" (func $host-import {signature}))
            (func $run (export "run") (result i32) {body} i32.const 1) {initializer})
        (core instance $imports (export "memory" (memory $memory)) (export "host" (func $host-core)))
        (core instance $i (instantiate $m (with "" (instance $imports))))
        (func $run (result s32) (canon lift (core func $i "run")))
        (instance $c (export "run" (func $run)))
        (export "patina:demo/control@0.1.0" (instance $c)))"#
    );
    let path = dir.path().join("noisy.wasm");
    fs::write(&path, wat::parse_str(wat).unwrap()).unwrap();
    path
}

fn invoke_noisy(path: PathBuf, limits: MctWasmInvocationLimits) -> MctWitComponentInvocationReport {
    let child = loaded_typed_child(path, vec!["patina:demo/control@0.1.0.run".into()]);
    let call = typed_call("run");
    MctWasmComponentRuntime::new(MctWasmHostConfig {
        invocation_limits: limits,
        ..MctWasmHostConfig::default_local()
    })
    .unwrap()
    .invoke_authorized_child_wit_export_with_host_adapters(
        authorize_artifact(&call, &child),
        &child,
        &call,
        &serde_json::json!([]),
        wit_host_adapters(),
        ids(),
    )
    .unwrap()
}

#[test]
fn logging_measure_host_and_toy_caps_trap_before_the_excess_effect() {
    let dir = tempfile::tempdir().unwrap();
    for (measure, kind) in [
        (false, "logging"),
        (true, "measure"),
        (false, "host"),
        (true, "toy"),
    ] {
        let mut limits = MctWasmInvocationLimits::default();
        match kind {
            "logging" => limits.logging_calls = 3,
            "measure" => limits.measure_calls = 3,
            "host" => limits.host_calls = 3,
            _ => limits.toy_calls = 3,
        }
        let path = noisy_component(&dir, measure, 4, 8);
        let report = invoke_noisy(path.clone(), limits.clone());
        assert_eq!(report.result.outcome, ResultOutcome::Failed, "{kind}");
        assert!(report.result.requester_message.contains("resource limit"));
        assert!(report.produced_messages.is_empty());
        assert_eq!(report.output_json, Value::Null);
        assert_eq!(
            report.observations.len(),
            8,
            "exactly three Toy starts/completions plus runtime start/failure: {kind}"
        );
        assert_eq!(
            report.observations.last().unwrap().kind,
            ObservationKind::RuntimeExecutionFailed
        );
        assert!(
            report
                .observations
                .last()
                .unwrap()
                .detail_ref
                .as_ref()
                .unwrap()
                .contains(kind)
        );
        let path = noisy_component(&dir, measure, 3, 8);
        let control = invoke_noisy(path, limits);
        assert_eq!(
            control.result.outcome,
            ResultOutcome::Success,
            "exact cap must succeed"
        );
    }
}

#[test]
fn observation_count_and_accumulated_bytes_are_reserved_before_effects() {
    let dir = tempfile::tempdir().unwrap();
    let path = noisy_component(&dir, false, 4, 8);
    let limits = MctWasmInvocationLimits {
        observations: 6,
        ..Default::default()
    };
    let report = invoke_noisy(path.clone(), limits);
    assert_eq!(report.result.outcome, ResultOutcome::Failed);
    assert_eq!(report.observations.len(), 6); // two allowed Toy calls, no third effect
    assert!(
        report
            .observations
            .last()
            .unwrap()
            .detail_ref
            .as_ref()
            .unwrap()
            .contains("observations")
    );
    let limits = MctWasmInvocationLimits {
        accumulated_bytes: 8192,
        ..Default::default()
    };
    let report = invoke_noisy(path, limits);
    assert_eq!(report.result.outcome, ResultOutcome::Failed);
    assert!(serde_json::to_vec(&report.observations).unwrap().len() <= 8192);
    assert!(report.observations.len() < 10);
}

#[test]
fn oversized_log_arguments_and_default_logging_limit_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let report = invoke_noisy(noisy_component(&dir, false, 1, 65536), Default::default());
    assert_eq!(report.result.outcome, ResultOutcome::Failed);
    assert_eq!(
        report.observations.len(),
        2,
        "oversized argument must not enter Toy backend"
    );
    let report = invoke_noisy(noisy_component(&dir, false, 257, 1), Default::default());
    assert_eq!(report.result.outcome, ResultOutcome::Failed);
    assert_eq!(report.observations.len(), 514);
}

#[test]
fn staged_verified_bytes_survive_replacement_and_removal_before_compilation() {
    let dir = tempfile::tempdir().unwrap();
    let path = typed_component_path(&dir);
    let child = loaded_typed_child(
        path.clone(),
        vec!["patina:demo/control@0.1.0.double".into()],
    );
    let call = typed_call("double");
    let authorized = authorize_artifact(&call, &child);
    let runtime = runtime();
    let snapshot = runtime
        .verified_child_snapshot(&child, &authorized)
        .unwrap();
    fs::write(&path, b"not wasm anymore").unwrap();
    fs::remove_file(&path).unwrap();
    let component = runtime.compile_snapshot(&path, &snapshot).unwrap();
    let mut store = Store::new(&runtime.engine, ());
    store.set_epoch_deadline(1);
    let instance = component::Linker::new(&runtime.engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let operation = resolve_wit_operation_target(&call.target).unwrap();
    let function = lookup_wit_component_func(&mut store, &instance, &operation).unwrap();
    let mut results = [component::Val::S32(0)];
    function
        .call(&mut store, &[component::Val::S32(7)], &mut results)
        .unwrap();
    assert!(matches!(results[0], component::Val::S32(14)));
}

#[test]
fn audit_only_or_wrong_artifact_identity_cannot_bypass_runtime_digest_binding() {
    let dir = tempfile::tempdir().unwrap();
    let path = typed_component_path(&dir);
    let mut child = loaded_typed_child(path, vec!["patina:demo/control@0.1.0.double".into()]);
    let call = typed_call("double");
    let runtime = runtime();
    let authorized = authorize_artifact(&call, &child);
    child.wasm_digest.verified = false;
    assert!(
        runtime
            .verified_child_snapshot(&child, &authorized)
            .is_err()
    );
    child.wasm_digest.verified = true;
    child.artifact_id = format!("sha256:{}", "0".repeat(64));
    assert!(
        runtime
            .verified_child_snapshot(&child, &authorized)
            .is_err()
    );
}

#[test]
fn initialization_and_post_effect_output_exhaustion_preserve_observations() {
    let dir = tempfile::tempdir().unwrap();
    let path = noisy_component_with_start(&dir, false, 4, 8, true);
    let report = invoke_noisy(
        path,
        MctWasmInvocationLimits {
            logging_calls: 3,
            ..Default::default()
        },
    );
    assert_eq!(report.result.outcome, ResultOutcome::Failed);
    assert_eq!(
        report.observations.len(),
        8,
        "initializer effects must not disappear into instantiate error"
    );
    let report = invoke_noisy(
        noisy_component(&dir, false, 1, 8),
        MctWasmInvocationLimits {
            output_bytes: 32,
            ..Default::default()
        },
    );
    assert_eq!(report.result.outcome, ResultOutcome::Failed);
    assert_eq!(
        report.observations.len(),
        4,
        "completed effect facts survive oversized Toy output"
    );
    assert_eq!(
        report.observations[2].kind,
        ObservationKind::ToyCallCompleted
    );
    assert_eq!(report.output_json, Value::Null);
}

#[test]
fn scalar_toy_import_budget_retains_facts_and_unique_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scalar-noisy.wasm");
    fs::write(
        &path,
        wat::parse_str(
            r#"(component
        (import "mct-toy-call" (func $toy (result s32)))
        (core func $host (canon lower (func $toy)))
        (core module $m (import "" "toy" (func $toy (result i32)))
            (func (export "run") (result i32)
                call $toy drop call $toy drop call $toy drop i32.const 1)
            (func $init call $toy drop) (start $init))
        (core instance $imports (export "toy" (func $host)))
        (core instance $i (instantiate $m (with "" (instance $imports))))
        (func $run (result s32) (canon lift (core func $i "run")))
        (export "run" (func $run)))"#,
        )
        .unwrap(),
    )
    .unwrap();
    let mut registry = MctToyAdapterRegistry::new();
    registry.register(
        ToyId::new("toy-echo").unwrap(),
        crate::MctToyBackend::EchoJson,
    );
    let runtime = MctWasmComponentRuntime::new(MctWasmHostConfig {
        invocation_limits: MctWasmInvocationLimits {
            toy_calls: 2,
            ..Default::default()
        },
        ..MctWasmHostConfig::default_local()
    })
    .unwrap();
    for (export_name, expected_facts) in [("run", 6), ("missing", 4)] {
        let report = runtime
            .invoke_authorized_s32_export_with_toy_imports(
                authorized(&path),
                &call(),
                MctWasmComponentToyInvocation {
                    component_path: path.clone(),
                    export_name: export_name.into(),
                    toy_registry: registry.clone(),
                    toy_imports: vec![MctWasmToyHostImport {
                        import_name: "mct-toy-call".into(),
                        authorized_toy_call: toy_authorized("bounded-scalar"),
                        ids: toy_ids(),
                    }],
                    ids: ids(),
                },
            )
            .unwrap();
        assert_eq!(report.result.outcome, ResultOutcome::Failed);
        assert_eq!(report.observations.len(), expected_facts);
        let unique: BTreeSet<_> = report
            .observations
            .iter()
            .map(|o| o.observation_id.as_str())
            .collect();
        assert_eq!(unique.len(), report.observations.len());
    }
}

#[test]
fn component_result_byte_limit_returns_failed_report_not_successful_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large-result.wasm");
    fs::write(
        &path,
        wat::parse_str(
            r#"(component
        (core module $m (memory (export "memory") 1)
            (data (i32.const 0) "\08\00\00\00\00\01\00\00")
            (func (export "run") (result i32) i32.const 0))
        (core instance $i (instantiate $m))
        (func $run (result string) (canon lift (core func $i "run") (memory (core memory $i "memory"))))
        (instance $c (export "run" (func $run)))
        (export "patina:demo/control@0.1.0" (instance $c)))"#,
        )
        .unwrap(),
    )
    .unwrap();
    let child = loaded_typed_child(path, vec!["patina:demo/control@0.1.0.run".into()]);
    let call = typed_call("run");
    for (limit, outcome) in [(128, ResultOutcome::Failed), (2048, ResultOutcome::Success)] {
        let runtime = MctWasmComponentRuntime::new(MctWasmHostConfig {
            invocation_limits: MctWasmInvocationLimits {
                output_bytes: limit,
                ..Default::default()
            },
            ..MctWasmHostConfig::default_local()
        })
        .unwrap();
        let report = runtime
            .invoke_authorized_child_wit_export(
                authorize_artifact(&call, &child),
                &child,
                &call,
                &serde_json::json!([]),
                ids(),
            )
            .unwrap();
        assert_eq!(report.result.outcome, outcome);
        assert_eq!(report.observations.len(), 2);
        if outcome == ResultOutcome::Failed {
            assert_eq!(report.output_json, Value::Null);
        }
    }
}

fn component_digest(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}

fn authorize_artifact(call: &MctCall, child: &MctLoadedChild) -> AuthorizedChildInvocation {
    crate::authority_test_fixture::authorized_child_for_artifact(
        call,
        &child.name,
        MctNodeId::new("mother-a").unwrap(),
        "wasm-boundary",
        &child.artifact_id,
        &format!("sha256:{}", child.manifest_digest.sha256),
    )
}

#[test]
fn l16_manifest_replacement_after_admission_is_rejected_before_execution() {
    let dir = tempfile::tempdir().unwrap();
    let child = loaded_typed_child(
        typed_component_path(&dir),
        vec!["patina:demo/control@0.1.0.double".into()],
    );
    let call = typed_call("double");
    let mut tokens = (0..5)
        .map(|_| authorize_artifact(&call, &child))
        .collect::<Vec<_>>()
        .into_iter();
    let original = fs::read(&child.manifest_path).unwrap();
    let runtime = runtime();
    let mut forged_metadata = child.clone();
    forged_metadata.requested_toys.push("secrets".into());
    assert!(
        runtime
            .invoke_authorized_child_wit_export(
                tokens.next().unwrap(),
                &forged_metadata,
                &call,
                &serde_json::json!([7]),
                ids(),
            )
            .is_err()
    );
    assert_eq!(
        runtime
            .invoke_authorized_child_wit_export(
                tokens.next().unwrap(),
                &child,
                &call,
                &serde_json::json!([7]),
                ids()
            )
            .unwrap()
            .output_json,
        serde_json::json!({"results":[14]})
    );
    fs::write(&child.manifest_path, b"changed authority, same component").unwrap();
    assert!(
        runtime
            .invoke_authorized_child_wit_export(
                tokens.next().unwrap(),
                &child,
                &call,
                &serde_json::json!([7]),
                ids()
            )
            .is_err()
    );
    fs::write(&child.manifest_path, original).unwrap();
    let mut changed_inventory = child.clone();
    changed_inventory.manifest_digest.sha256 = "0".repeat(64);
    assert!(
        runtime
            .invoke_authorized_child_wit_export(
                tokens.next().unwrap(),
                &changed_inventory,
                &call,
                &serde_json::json!([7]),
                ids()
            )
            .is_err()
    );
    assert_eq!(
        runtime
            .invoke_authorized_child_wit_export(
                tokens.next().unwrap(),
                &child,
                &call,
                &serde_json::json!([7]),
                ids()
            )
            .unwrap()
            .result
            .outcome,
        ResultOutcome::Success
    );
}

#[test]
fn changed_component_after_inventory_verification_cannot_execute_substituted_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = typed_component_path(&dir);
    let mut child = loaded_typed_child(
        path.clone(),
        vec!["patina:demo/control@0.1.0.double".into()],
    );
    child.wasm_digest.sha256 = component_digest(&path);
    child.artifact_id = format!("sha256:{}", child.wasm_digest.sha256);
    let call = typed_call("double");
    let original = fs::read(&path).unwrap();
    // A different valid component exports the same function but returns 99.
    let replacement = wat::parse_str(
        r#"(component
      (core module $m (func (export "double") (param i32) (result i32) i32.const 99))
      (core instance $i (instantiate $m))
      (func $f (param "value" s32) (result s32) (canon lift (core func $i "double")))
      (instance $c (export "double" (func $f)))
      (export "patina:demo/control@0.1.0" (instance $c)))"#,
    )
    .unwrap();
    for rename in [false, true] {
        fs::write(&path, &original).unwrap();
        let authorized = authorize_artifact(&call, &child);
        if rename {
            let other = dir.path().join("replacement.wasm");
            fs::write(&other, &replacement).unwrap();
            fs::rename(other, &path).unwrap();
        } else {
            fs::write(&path, &replacement).unwrap();
        }
        // Replacing the untrusted sidecar cannot change the admitted identity.
        fs::write(path.with_extension("wasm.sha256"), component_digest(&path)).unwrap();
        let result = runtime().invoke_authorized_child_wit_export(
            authorized,
            &child,
            &call,
            &serde_json::json!([7]),
            ids(),
        );
        assert!(
            result.is_err(),
            "substituted component executed: {result:?}"
        );
    }
}

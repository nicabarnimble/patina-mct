use mct_daemon::{
    DEFAULT_WASM_MEMORY_LIMIT_BYTES, MctWasmComponentInvocationIds, MctWasmComponentRuntime,
    MctWasmComponentRuntimeError, MctWasmHostConfig,
};
use mct_kernel::*;
use std::{fs, path::PathBuf, sync::mpsc, time::Duration};

fn timestamp_after_millis(millis: i64) -> Timestamp {
    let instant = jiff::Timestamp::now()
        .checked_add(jiff::SignedDuration::from_millis(millis))
        .unwrap();
    Timestamp::new(instant.to_string()).unwrap()
}

fn call(deadline: Timestamp) -> MctCall {
    MctCall {
        call_id: CallId::from("call-wasm-limit"),
        caller: CallerIdentity {
            node_id: MctNodeId::from("mother-a"),
            user_id: None,
            vision_id: VisionId::from("vision-a"),
            project_id: None,
        },
        target: OperationTarget {
            namespace: "patina".into(),
            interface_name: "limit".into(),
            function_name: "run".into(),
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
        deadline,
        trace_context: TraceContext {
            trace_id: TraceId::from("trace-wasm-limit"),
            span_id: SpanId::from("span-wasm-limit"),
        },
        origin: CallOrigin::WasmHost,
    }
}

fn authorized() -> AuthorizedChildInvocation {
    AuthorizedChildInvocation {
        authorized_child_invocation_id: AuthorizedChildInvocationId::from("auth-wasm-limit"),
        call_id: CallId::from("call-wasm-limit"),
        evaluation_id: ChildCallEvaluationId::from("eval-wasm-limit"),
        assignment_id: ChildAssignmentId::from("assignment-wasm-limit"),
        approval_id: ChildApprovalId::from("approval-wasm-limit"),
        artifact_id: ComponentArtifactId::from("artifact-wasm-limit"),
        child_instance_id: ChildInstanceId::from("instance-wasm-limit"),
        child_name: "wasm-limit".into(),
        authority_decision_id: DecisionId::from("decision-wasm-limit"),
    }
}

fn ids() -> MctWasmComponentInvocationIds {
    MctWasmComponentInvocationIds {
        started_observation_id: ObservationId::from("obs-wasm-limit-started"),
        completed_observation_id: ObservationId::from("obs-wasm-limit-completed"),
        audit_ref: AuditRef::from("audit-wasm-limit"),
        started_at: Timestamp::new("2026-07-02T00:00:00Z").unwrap(),
        completed_at: Timestamp::new("2026-07-02T00:00:01Z").unwrap(),
    }
}

fn write_component(dir: &tempfile::TempDir, name: &str, wat: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, wat::parse_str(wat).unwrap()).unwrap();
    path
}

#[test]
fn looping_component_times_out_instead_of_hanging() {
    let component_wat = r#"
(component
  (core module $m
    (func $spin (export "spin") (result i32)
      (loop $forever
        br $forever)
      i32.const 0))
  (core instance $i (instantiate $m))
  (func $spin (result s32) (canon lift (core func $i "spin")))
  (export "spin" (func $spin)))
"#;
    let dir = tempfile::tempdir().unwrap();
    let component_path = write_component(&dir, "spin.component.wasm", component_wat);
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let runtime = MctWasmComponentRuntime::new(MctWasmHostConfig {
            memory_limit_bytes: DEFAULT_WASM_MEMORY_LIMIT_BYTES,
        })
        .unwrap();
        let report = runtime.invoke_authorized_s32_export(
            &authorized(),
            &call(timestamp_after_millis(100)),
            component_path,
            "spin",
            ids(),
        );
        let _ = tx.send(report);
    });

    let report = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("looping component should return a timed-out report")
        .unwrap();
    assert_eq!(report.result.outcome, ResultOutcome::TimedOut);
    assert_eq!(report.result.requester_message, "wasm component timed out");
    assert_eq!(report.observations.len(), 2);
    assert_eq!(
        report.observations[0].kind,
        ObservationKind::RuntimeExecutionStarted
    );
    assert_eq!(
        report.observations[1].kind,
        ObservationKind::RuntimeExecutionTimedOut
    );
}

#[test]
fn component_allocation_over_memory_cap_fails() {
    let component_wat = r#"
(component
  (core module $m
    (memory (export "memory") 2048)
    (func $run (export "run") (result i32)
      i32.const 1))
  (core instance $i (instantiate $m))
  (func $run (result s32) (canon lift (core func $i "run")))
  (export "run" (func $run)))
"#;
    let dir = tempfile::tempdir().unwrap();
    let component_path = write_component(&dir, "large-memory.component.wasm", component_wat);
    let runtime = MctWasmComponentRuntime::new(MctWasmHostConfig {
        memory_limit_bytes: DEFAULT_WASM_MEMORY_LIMIT_BYTES,
    })
    .unwrap();

    let result = runtime.invoke_authorized_s32_export(
        &authorized(),
        &call(timestamp_after_millis(5_000)),
        component_path,
        "run",
        ids(),
    );

    assert!(
        matches!(
            result,
            Err(MctWasmComponentRuntimeError::ResourceLimit { .. })
        ),
        "memory over cap must fail closed with a typed resource-limit error"
    );
}

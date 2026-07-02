use mct_kernel::*;
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctProcessChildHarness {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub timeout: Duration,
    pub local_node_id: MctNodeId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctProcessChildInvocationIds {
    pub started_observation_id: ObservationId,
    pub completed_observation_id: ObservationId,
    pub result_ref: ResultRef,
    pub audit_ref: AuditRef,
    pub started_at: Timestamp,
    pub completed_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctProcessChildInvocationReport {
    pub result: MctResult,
    pub stdout: String,
    pub stderr: String,
    pub observations: Vec<MctObservation>,
}

#[derive(Debug, Error)]
pub enum MctProcessChildError {
    #[error("spawn process child {executable}: {source}")]
    Spawn {
        executable: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("write process child stdin: {0}")]
    WriteStdin(#[source] std::io::Error),
    #[error("wait for process child: {0}")]
    Wait(#[source] std::io::Error),
}

impl MctProcessChildHarness {
    pub fn invoke_authorized_child(
        &self,
        authorized: &AuthorizedChildInvocation,
        call: &MctCall,
        stdin_json: &str,
        ids: MctProcessChildInvocationIds,
    ) -> Result<MctProcessChildInvocationReport, MctProcessChildError> {
        let started = process_observation(
            ids.started_observation_id.clone(),
            ids.started_at.clone(),
            ObservationKind::RuntimeExecutionStarted,
            ObservationOutcome::Started,
            call,
            authorized,
            "process child execution started",
        );

        let mut command = Command::new(&self.executable);
        command
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|source| MctProcessChildError::Spawn {
                executable: self.executable.clone(),
                source,
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(stdin_json.as_bytes())
                .map_err(MctProcessChildError::WriteStdin)?;
        }

        let deadline = Instant::now() + self.timeout;
        loop {
            if child
                .try_wait()
                .map_err(MctProcessChildError::Wait)?
                .is_some()
            {
                let output = child
                    .wait_with_output()
                    .map_err(MctProcessChildError::Wait)?;
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                let outcome = if output.status.success() {
                    ResultOutcome::Success
                } else {
                    ResultOutcome::Failed
                };
                return Ok(self.report(
                    authorized,
                    call,
                    ids,
                    started,
                    stdout,
                    stderr,
                    outcome,
                    if output.status.success() {
                        ObservationKind::RuntimeExecutionCompleted
                    } else {
                        ObservationKind::RuntimeExecutionFailed
                    },
                    if output.status.success() {
                        ObservationOutcome::Completed
                    } else {
                        ObservationOutcome::Failed
                    },
                    if output.status.success() {
                        "process child execution completed"
                    } else {
                        "process child execution failed"
                    },
                ));
            }

            if Instant::now() >= deadline {
                let _ = child.kill();
                let output = child
                    .wait_with_output()
                    .map_err(MctProcessChildError::Wait)?;
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                return Ok(self.report(
                    authorized,
                    call,
                    ids,
                    started,
                    stdout,
                    stderr,
                    ResultOutcome::TimedOut,
                    ObservationKind::RuntimeExecutionTimedOut,
                    ObservationOutcome::TimedOut,
                    "process child execution timed out",
                ));
            }

            thread::sleep(Duration::from_millis(5));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn report(
        &self,
        authorized: &AuthorizedChildInvocation,
        call: &MctCall,
        ids: MctProcessChildInvocationIds,
        started: MctObservation,
        stdout: String,
        stderr: String,
        outcome: ResultOutcome,
        completed_kind: ObservationKind,
        completed_outcome: ObservationOutcome,
        safe_message: &str,
    ) -> MctProcessChildInvocationReport {
        let completed = process_observation(
            ids.completed_observation_id,
            ids.completed_at,
            completed_kind,
            completed_outcome,
            call,
            authorized,
            safe_message,
        );
        let result = MctResult {
            call_id: call.call_id.clone(),
            outcome,
            route_taken: Some(RouteTaken {
                node_id: self.local_node_id.clone(),
                child_id: Some(ChildId::from(authorized.child_name.clone())),
                runtime_kind: RuntimeKind::Process,
            }),
            authority_decision_ref: authorized.authority_decision_id.clone(),
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: call.payload_metadata.approximate_size_bytes,
                output_size_bytes: Some(stdout.len() as u64),
            },
            requester_message: match outcome {
                ResultOutcome::Success => "process child completed",
                ResultOutcome::TimedOut => "process child timed out",
                ResultOutcome::Failed => "process child failed",
                ResultOutcome::Denied => "not authorized",
                ResultOutcome::Cancelled => "process child cancelled",
            }
            .into(),
            audit_ref: ids.audit_ref,
        };

        MctProcessChildInvocationReport {
            result,
            stdout,
            stderr,
            observations: vec![started, completed],
        }
    }
}

fn process_observation(
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

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn call() -> MctCall {
        MctCall {
            call_id: CallId::from("call-process-echo"),
            caller: CallerIdentity {
                node_id: MctNodeId::from("mother-a"),
                user_id: None,
                vision_id: VisionId::from("vision-a"),
                project_id: None,
            },
            target: OperationTarget {
                namespace: "patina".into(),
                interface_name: "echo".into(),
                function_name: "echo".into(),
            },
            payload_metadata: PayloadMetadata {
                data_classification: "public".into(),
                approximate_size_bytes: 17,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 1,
                grants_revision: 1,
                vision_policy_revision: 1,
            },
            deadline: Timestamp::new("2026-05-31T00:01:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::from("trace-process-echo"),
                span_id: SpanId::from("span-process-echo"),
            },
            origin: CallOrigin::Cli,
        }
    }

    fn authorized() -> AuthorizedChildInvocation {
        AuthorizedChildInvocation {
            authorized_child_invocation_id: AuthorizedChildInvocationId::from("auth-process-1"),
            call_id: CallId::from("call-process-echo"),
            evaluation_id: ChildCallEvaluationId::from("child-eval-process"),
            assignment_id: ChildAssignmentId::from("assignment-process"),
            approval_id: ChildApprovalId::from("approval-process"),
            artifact_id: ComponentArtifactId::from("artifact-process"),
            child_instance_id: ChildInstanceId::from("instance-process"),
            child_name: "process-echo".into(),
            authority_decision_id: DecisionId::from("decision-child-process"),
        }
    }

    fn ids(stem: &str) -> MctProcessChildInvocationIds {
        MctProcessChildInvocationIds {
            started_observation_id: ObservationId::from(format!("obs-{stem}-started")),
            completed_observation_id: ObservationId::from(format!("obs-{stem}-completed")),
            result_ref: ResultRef::from(format!("result-{stem}")),
            audit_ref: AuditRef::from(format!("audit-{stem}")),
            started_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            completed_at: Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
        }
    }

    fn write_script(name: &str, body: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        fs::write(&path, body).unwrap();
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).unwrap();
        }
        (dir, path)
    }

    #[test]
    fn process_harness_invokes_only_with_authorized_child_invocation() {
        let (_dir, script) = write_script(
            "echo-child.sh",
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"reply\":\"ok\"}'\n",
        );
        let harness = MctProcessChildHarness {
            executable: script,
            args: Vec::new(),
            timeout: Duration::from_secs(2),
            local_node_id: MctNodeId::from("mother-a"),
        };

        let report = harness
            .invoke_authorized_child(&authorized(), &call(), "{\"input\":\"hi\"}", ids("echo"))
            .unwrap();

        assert_eq!(report.result.outcome, ResultOutcome::Success);
        assert_eq!(report.stdout, "{\"reply\":\"ok\"}");
        assert_eq!(report.observations.len(), 2);
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
            Some(CallId::from("call-process-echo"))
        );
        assert_eq!(
            report.result.authority_decision_ref,
            DecisionId::from("decision-child-process")
        );
    }

    #[test]
    fn process_harness_timeout_returns_typed_result_and_observation() {
        let (_dir, script) = write_script("slow-child.sh", "#!/bin/sh\nsleep 2\nprintf slow\n");
        let harness = MctProcessChildHarness {
            executable: script,
            args: Vec::new(),
            timeout: Duration::from_millis(20),
            local_node_id: MctNodeId::from("mother-a"),
        };

        let report = harness
            .invoke_authorized_child(&authorized(), &call(), "{}", ids("timeout"))
            .unwrap();

        assert_eq!(report.result.outcome, ResultOutcome::TimedOut);
        assert_eq!(
            report.observations[1].kind,
            ObservationKind::RuntimeExecutionTimedOut
        );
        assert_eq!(report.observations[1].outcome, ObservationOutcome::TimedOut);
        assert_eq!(report.result.requester_message, "process child timed out");
    }
}

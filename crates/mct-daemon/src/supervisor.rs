use mct_kernel::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::{Child, Command, Stdio},
};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctProcessSpawnConfig {
    pub executable: PathBuf,
    pub args: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctSupervisedProcessState {
    Running,
    Exited,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctSupervisedProcessStatus {
    pub instance_id: ChildInstanceId,
    pub child_name: String,
    pub pid: u32,
    pub state: MctSupervisedProcessState,
    pub exit_code: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctProcessSupervisorEvent {
    pub status: MctSupervisedProcessStatus,
    pub observation: MctObservation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctProcessSupervisorRecoveryReport {
    pub recovered: Vec<MctProcessSupervisorEvent>,
    pub safe_message: String,
}

#[derive(Debug, Error)]
pub enum MctProcessSupervisorError {
    #[error("process instance already running: {0}")]
    AlreadyRunning(ChildInstanceId),
    #[error("unknown process instance: {0}")]
    UnknownInstance(ChildInstanceId),
    #[error("spawn supervised process {executable}: {source}")]
    Spawn {
        executable: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("inspect supervised process {instance_id}: {source}")]
    Inspect {
        instance_id: ChildInstanceId,
        #[source]
        source: std::io::Error,
    },
    #[error("stop supervised process {instance_id}: {source}")]
    Stop {
        instance_id: ChildInstanceId,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug)]
pub struct MctProcessSupervisor {
    local_node_id: MctNodeId,
    processes: BTreeMap<ChildInstanceId, SupervisedProcess>,
}

#[derive(Debug)]
struct SupervisedProcess {
    child: Child,
    authorized: AuthorizedChildInvocation,
    call: MctCall,
    last_status: MctSupervisedProcessStatus,
}

impl MctProcessSupervisor {
    pub fn new(local_node_id: MctNodeId) -> Self {
        Self {
            local_node_id,
            processes: BTreeMap::new(),
        }
    }

    pub fn spawn_authorized(
        &mut self,
        authorized: AuthorizedChildInvocation,
        call: MctCall,
        config: MctProcessSpawnConfig,
        observation_id: ObservationId,
        observed_at: Timestamp,
    ) -> Result<MctProcessSupervisorEvent, MctProcessSupervisorError> {
        if self.processes.contains_key(&authorized.child_instance_id) {
            return Err(MctProcessSupervisorError::AlreadyRunning(
                authorized.child_instance_id,
            ));
        }
        let mut command = Command::new(&config.executable);
        command
            .args(&config.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command
            .spawn()
            .map_err(|source| MctProcessSupervisorError::Spawn {
                executable: config.executable,
                source,
            })?;
        let status = MctSupervisedProcessStatus {
            instance_id: authorized.child_instance_id.clone(),
            child_name: authorized.child_name.clone(),
            pid: child.id(),
            state: MctSupervisedProcessState::Running,
            exit_code: None,
        };
        let observation = supervisor_observation(
            observation_id,
            observed_at,
            ObservationKind::RuntimeExecutionStarted,
            ObservationOutcome::Started,
            &call,
            &authorized,
            &self.local_node_id,
            "supervised process started",
        );
        self.processes.insert(
            authorized.child_instance_id.clone(),
            SupervisedProcess {
                child,
                authorized,
                call,
                last_status: status.clone(),
            },
        );
        Ok(MctProcessSupervisorEvent {
            status,
            observation,
        })
    }

    pub fn status(
        &mut self,
        instance_id: &ChildInstanceId,
    ) -> Result<Option<MctSupervisedProcessStatus>, MctProcessSupervisorError> {
        let Some(process) = self.processes.get_mut(instance_id) else {
            return Ok(None);
        };
        if process.last_status.state == MctSupervisedProcessState::Running
            && let Some(exit_status) =
                process
                    .child
                    .try_wait()
                    .map_err(|source| MctProcessSupervisorError::Inspect {
                        instance_id: instance_id.clone(),
                        source,
                    })?
        {
            process.last_status.state = MctSupervisedProcessState::Exited;
            process.last_status.exit_code = exit_status.code();
        }
        Ok(Some(process.last_status.clone()))
    }

    pub fn recover_from_persisted_statuses(
        &self,
        statuses: &[MctSupervisedProcessStatus],
        observed_at: Timestamp,
    ) -> MctProcessSupervisorRecoveryReport {
        let recovered = statuses
            .iter()
            .map(|status| {
                let mut recovered_status = status.clone();
                if recovered_status.state == MctSupervisedProcessState::Running {
                    recovered_status.state = MctSupervisedProcessState::Stopped;
                }
                let observation = MctObservation {
                    observation_id: ObservationId::new(format!(
                        "obs:supervisor-recovery:{}",
                        recovered_status.instance_id
                    ))
                    .expect("string ID literal/generated value must be non-empty"),
                    observed_at: observed_at.clone(),
                    kind: ObservationKind::ChildInstanceDegraded,
                    source_plane: SourcePlane::Adapter,
                    trace: ObservationTraceRef {
                        trace_id: TraceId::new(format!(
                            "trace:supervisor-recovery:{}",
                            recovered_status.instance_id
                        ))
                        .expect("string ID literal/generated value must be non-empty"),
                        span_id: None,
                        parent_span_id: None,
                        external_trace_id: None,
                    },
                    call_id: None,
                    decision_id: None,
                    subject_id: Some(recovered_status.child_name.clone()),
                    resource_id: Some(recovered_status.instance_id.to_string()),
                    policy_revision: None,
                    grants_revision: None,
                    outcome: ObservationOutcome::Informational,
                    visibility: ObservationVisibility::NodeOperator,
                    safe_message: "supervisor recovered persisted process status".into(),
                    detail_ref: Some(format!(
                        "previous_pid:{};node:{};previous_state:{:?};recovered_state:{:?}",
                        status.pid, self.local_node_id, status.state, recovered_status.state
                    )),
                };
                MctProcessSupervisorEvent {
                    status: recovered_status,
                    observation,
                }
            })
            .collect();

        MctProcessSupervisorRecoveryReport {
            recovered,
            safe_message: "supervisor recovery reconciled persisted statuses".into(),
        }
    }

    pub fn stop(
        &mut self,
        instance_id: &ChildInstanceId,
        observation_id: ObservationId,
        observed_at: Timestamp,
    ) -> Result<MctProcessSupervisorEvent, MctProcessSupervisorError> {
        let mut process = self
            .processes
            .remove(instance_id)
            .ok_or_else(|| MctProcessSupervisorError::UnknownInstance(instance_id.clone()))?;
        if process.last_status.state == MctSupervisedProcessState::Running {
            if process
                .child
                .try_wait()
                .map_err(|source| MctProcessSupervisorError::Inspect {
                    instance_id: instance_id.clone(),
                    source,
                })?
                .is_none()
            {
                process
                    .child
                    .kill()
                    .map_err(|source| MctProcessSupervisorError::Stop {
                        instance_id: instance_id.clone(),
                        source,
                    })?;
            }
            let exit_status =
                process
                    .child
                    .wait()
                    .map_err(|source| MctProcessSupervisorError::Stop {
                        instance_id: instance_id.clone(),
                        source,
                    })?;
            process.last_status.exit_code = exit_status.code();
        }
        process.last_status.state = MctSupervisedProcessState::Stopped;
        let observation = supervisor_observation(
            observation_id,
            observed_at,
            ObservationKind::RuntimeExecutionCompleted,
            ObservationOutcome::Completed,
            &process.call,
            &process.authorized,
            &self.local_node_id,
            "supervised process stopped",
        );
        Ok(MctProcessSupervisorEvent {
            status: process.last_status,
            observation,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn supervisor_observation(
    observation_id: ObservationId,
    observed_at: Timestamp,
    kind: ObservationKind,
    outcome: ObservationOutcome,
    call: &MctCall,
    authorized: &AuthorizedChildInvocation,
    local_node_id: &MctNodeId,
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
            "authorized_child_invocation:{};node:{}",
            authorized.authorized_child_invocation_id, local_node_id
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, thread, time::Duration};

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn call() -> MctCall {
        MctCall {
            call_id: CallId::new("call-supervisor")
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
                interface_name: "worker".into(),
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
            deadline: Timestamp::new("2026-05-31T00:01:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-supervisor")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: SpanId::new("span-supervisor")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            origin: CallOrigin::ProcessHarness,
        }
    }

    fn authorized() -> AuthorizedChildInvocation {
        AuthorizedChildInvocation {
            authorized_child_invocation_id: AuthorizedChildInvocationId::new("auth-supervisor")
                .expect("string ID literal/generated value must be non-empty"),
            call_id: CallId::new("call-supervisor")
                .expect("string ID literal/generated value must be non-empty"),
            evaluation_id: ChildCallEvaluationId::new("eval-supervisor")
                .expect("string ID literal/generated value must be non-empty"),
            assignment_id: ChildAssignmentId::new("assignment-supervisor")
                .expect("string ID literal/generated value must be non-empty"),
            approval_id: ChildApprovalId::new("approval-supervisor")
                .expect("string ID literal/generated value must be non-empty"),
            artifact_id: ComponentArtifactId::new("artifact-supervisor")
                .expect("string ID literal/generated value must be non-empty"),
            child_instance_id: ChildInstanceId::new("instance-supervisor")
                .expect("string ID literal/generated value must be non-empty"),
            child_name: "supervised-process".into(),
            authority_decision_id: DecisionId::new("decision-supervisor")
                .expect("string ID literal/generated value must be non-empty"),
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
    fn process_supervisor_spawns_statuses_and_stops_long_lived_child() {
        let (_dir, script) =
            write_script("long-lived.sh", "#!/bin/sh\nwhile true; do sleep 1; done\n");
        let mut supervisor = MctProcessSupervisor::new(
            MctNodeId::new("mother-a")
                .expect("string ID literal/generated value must be non-empty"),
        );

        let spawned = supervisor
            .spawn_authorized(
                authorized(),
                call(),
                MctProcessSpawnConfig {
                    executable: script,
                    args: Vec::new(),
                },
                ObservationId::new("obs-supervisor-start")
                    .expect("string ID literal/generated value must be non-empty"),
                Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            )
            .unwrap();
        assert_eq!(spawned.status.state, MctSupervisedProcessState::Running);
        assert_eq!(
            spawned.observation.kind,
            ObservationKind::RuntimeExecutionStarted
        );

        let status = supervisor
            .status(
                &ChildInstanceId::new("instance-supervisor")
                    .expect("string ID literal/generated value must be non-empty"),
            )
            .unwrap()
            .unwrap();
        assert_eq!(status.state, MctSupervisedProcessState::Running);

        let stopped = supervisor
            .stop(
                &ChildInstanceId::new("instance-supervisor")
                    .expect("string ID literal/generated value must be non-empty"),
                ObservationId::new("obs-supervisor-stop")
                    .expect("string ID literal/generated value must be non-empty"),
                Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
            )
            .unwrap();
        assert_eq!(stopped.status.state, MctSupervisedProcessState::Stopped);
        assert_eq!(
            stopped.observation.kind,
            ObservationKind::RuntimeExecutionCompleted
        );
        assert!(
            supervisor
                .status(
                    &ChildInstanceId::new("instance-supervisor")
                        .expect("string ID literal/generated value must be non-empty")
                )
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn process_supervisor_recovers_persisted_running_status_as_stopped() {
        let supervisor = MctProcessSupervisor::new(
            MctNodeId::new("mother-a")
                .expect("string ID literal/generated value must be non-empty"),
        );
        let report = supervisor.recover_from_persisted_statuses(
            &[MctSupervisedProcessStatus {
                instance_id: ChildInstanceId::new("instance-recover")
                    .expect("string ID literal/generated value must be non-empty"),
                child_name: "recover-child".into(),
                pid: 123,
                state: MctSupervisedProcessState::Running,
                exit_code: None,
            }],
            Timestamp::new("2026-05-31T00:00:02Z").unwrap(),
        );

        assert_eq!(report.recovered.len(), 1);
        assert_eq!(
            report.recovered[0].status.state,
            MctSupervisedProcessState::Stopped
        );
        assert_eq!(
            report.recovered[0].observation.kind,
            ObservationKind::ChildInstanceDegraded
        );
    }

    #[test]
    fn process_supervisor_observes_exited_child_status() {
        let (_dir, script) = write_script("crash.sh", "#!/bin/sh\nexit 7\n");
        let mut supervisor = MctProcessSupervisor::new(
            MctNodeId::new("mother-a")
                .expect("string ID literal/generated value must be non-empty"),
        );
        supervisor
            .spawn_authorized(
                authorized(),
                call(),
                MctProcessSpawnConfig {
                    executable: script,
                    args: Vec::new(),
                },
                ObservationId::new("obs-supervisor-crash-start")
                    .expect("string ID literal/generated value must be non-empty"),
                Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            )
            .unwrap();

        let mut status = supervisor
            .status(
                &ChildInstanceId::new("instance-supervisor")
                    .expect("string ID literal/generated value must be non-empty"),
            )
            .unwrap()
            .unwrap();
        for _ in 0..100 {
            if status.state == MctSupervisedProcessState::Exited {
                break;
            }
            thread::sleep(Duration::from_millis(10));
            status = supervisor
                .status(
                    &ChildInstanceId::new("instance-supervisor")
                        .expect("string ID literal/generated value must be non-empty"),
                )
                .unwrap()
                .unwrap();
        }
        assert_eq!(status.state, MctSupervisedProcessState::Exited);
        assert_eq!(status.exit_code, Some(7));

        let stopped = supervisor
            .stop(
                &ChildInstanceId::new("instance-supervisor")
                    .expect("string ID literal/generated value must be non-empty"),
                ObservationId::new("obs-supervisor-crash-stop")
                    .expect("string ID literal/generated value must be non-empty"),
                Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
            )
            .unwrap();
        assert_eq!(stopped.status.state, MctSupervisedProcessState::Stopped);
        assert_eq!(stopped.status.exit_code, Some(7));
    }
}

//! Exact resident call-stage sequencing, before-effect durability barriers, and handler mapping.
//!
//! Stage logic belongs to payload, idempotency, decision, execution, and forwarding; this module
//! only orders those stages and maps their completed outputs into the transport handler result.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResidentCallIngressContext {
    Peer {
        binding_id: PeerBindingId,
    },
    LocalPrincipal {
        origin: CallOrigin,
        caller: CallerIdentity,
    },
    Trigger {
        trigger_authority_id: CallTriggerAuthorityId,
        record_revision: u64,
        firing_id: CallTriggerFiringId,
        occurrence_id: CallTriggerOccurrenceId,
    },
    #[allow(dead_code)] // Ratified D1B interface; implemented only after Part A lands.
    ChildCallOut {
        parent_call_id: CallId,
        parent_firing_id: Option<CallTriggerFiringId>,
        depth: u8,
    },
}

impl ResidentCallIngressContext {
    pub(super) fn ordinary(request: &MctCallProtocolRequest) -> Option<Self> {
        match request.call.origin {
            CallOrigin::Iroh => Some(Self::Peer {
                binding_id: request.authority.peer_binding_id.clone(),
            }),
            CallOrigin::TriggerFiring => None,
            origin => Some(Self::LocalPrincipal {
                origin,
                caller: request.call.caller.clone(),
            }),
        }
    }

    fn matches_request(&self, request: &MctCallProtocolRequest) -> bool {
        match self {
            Self::Peer { binding_id } => {
                request.call.origin == CallOrigin::Iroh
                    && binding_id == &request.authority.peer_binding_id
            }
            Self::LocalPrincipal { origin, caller } => {
                *origin == request.call.origin
                    && *origin != CallOrigin::Iroh
                    && *origin != CallOrigin::TriggerFiring
                    && caller == &request.call.caller
            }
            Self::Trigger { .. } => request.call.origin == CallOrigin::TriggerFiring,
            Self::ChildCallOut { depth, .. } => {
                request.call.origin == CallOrigin::WasmHost && *depth > 0
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ResidentRuntimePaths {
    config_path: PathBuf,
    children_dir: PathBuf,
    state_path: PathBuf,
}

impl ResidentRuntimePaths {
    pub(crate) fn new(config_path: PathBuf, children_dir: PathBuf, state_path: PathBuf) -> Self {
        Self {
            config_path,
            children_dir,
            state_path,
        }
    }

    pub(crate) fn config_path(&self) -> &Path {
        &self.config_path
    }
    pub(crate) fn children_dir(&self) -> &Path {
        &self.children_dir
    }
    pub(crate) fn state_path(&self) -> &Path {
        &self.state_path
    }
}

pub(crate) async fn execute_resident_call(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    payload: ResidentPayloadIngress,
) -> MctIrohCallHandlerResult {
    let Some(context) = ResidentCallIngressContext::ordinary(&request) else {
        return MctIrohCallHandlerResult::denied();
    };
    execute_resident_call_with_context(paths, ledger, request, payload, context).await
}

pub(crate) async fn execute_resident_call_with_context(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    payload: ResidentPayloadIngress,
    context: ResidentCallIngressContext,
) -> MctIrohCallHandlerResult {
    execute_resident_call_at_with_context(
        paths,
        ledger,
        request,
        payload,
        current_timestamp(),
        context,
    )
    .await
}

#[cfg(test)]
pub(super) async fn execute_resident_call_at(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    payload: ResidentPayloadIngress,
    now: Timestamp,
) -> MctIrohCallHandlerResult {
    let Some(context) = ResidentCallIngressContext::ordinary(&request) else {
        return MctIrohCallHandlerResult::denied();
    };
    execute_resident_call_at_with_context(paths, ledger, request, payload, now, context).await
}

async fn execute_resident_call_at_with_context(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    payload: ResidentPayloadIngress,
    now: Timestamp,
    context: ResidentCallIngressContext,
) -> MctIrohCallHandlerResult {
    if !context.matches_request(&request) {
        return MctIrohCallHandlerResult::denied();
    }
    let inline_payload = match resolve_resident_request_payload(&paths, &request, payload).await {
        Ok(payload) => payload.into_inner(),
        Err(report) => {
            let (safe_message, observations) = report.into_parts();
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident payload failure ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            return MctIrohCallHandlerResult::failed(safe_message);
        }
    };

    let state_path = paths.state_path().to_path_buf();
    let idempotency_request = request.clone();
    let idempotency_ledger = ledger.clone();
    execute_idempotent_call_with_context(
        state_path,
        idempotency_ledger,
        idempotency_request,
        now,
        context,
        move || execute_resident_call_after_payload(paths, ledger, request, inline_payload),
    )
    .await
}

async fn execute_resident_call_after_payload(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    inline_payload: Option<Vec<u8>>,
) -> MctIrohCallHandlerResult {
    let authorization = match authorize_resident_child(paths.clone(), request.call.clone()).await {
        Ok(authorization) => authorization,
        Err(error) => {
            eprintln!("resident child authorization unavailable: {error}");
            return MctIrohCallHandlerResult::failed("runtime unavailable");
        }
    };

    match authorization {
        RouteDisposition::Denied {
            decision,
            mut observations,
        } => {
            let result = no_route_denied_result(
                &request.call,
                &decision,
                AuditRef::new(format!("audit:denied:{}", request.call.call_id))
                    .expect("generated denied audit ref must be non-empty"),
            );
            let result_ref = ResultRef::new(format!("result-resident:{}", request.call.call_id))
                .expect("generated denied result ref must be non-empty");
            observations.push(MctObservation {
                observation_id: ObservationId::new(format!(
                    "obs:result-resident:{}",
                    request.call.call_id
                ))
                .expect("generated denied result observation id must be non-empty"),
                observed_at: current_timestamp(),
                kind: ObservationKind::ResultRecorded,
                source_plane: SourcePlane::Kernel,
                trace: ObservationTraceRef {
                    trace_id: request.call.trace_context.trace_id.clone(),
                    span_id: Some(request.call.trace_context.span_id.clone()),
                    parent_span_id: None,
                    external_trace_id: None,
                },
                call_id: Some(request.call.call_id.clone()),
                decision_id: Some(decision.decision_id.clone()),
                subject_id: Some(request.call.caller.node_id.to_string()),
                resource_id: Some(result_ref.to_string()),
                policy_revision: Some(request.call.authority_context.policy_revision),
                grants_revision: Some(request.call.authority_context.grants_revision),
                outcome: ObservationOutcome::Denied,
                visibility: ObservationVisibility::InternalOnly,
                safe_message: "resident denied result recorded".into(),
                detail_ref: Some("result_outcome:denied".into()),
            });
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident route denial ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            let run_id = format!("run-denied:{}", request.call.call_id);
            let projection = MctRuntimeStateStore::open(paths.state_path()).and_then(|state| {
                if state.get_run(&run_id)?.is_none() {
                    state.insert_run_started(
                        &run_id,
                        &request.call,
                        RuntimeKind::Internal,
                        None,
                        current_timestamp_string(),
                    )?;
                }
                state.complete_run(&run_id, &result, current_timestamp_string())?;
                Ok(())
            });
            if let Err(error) = projection {
                eprintln!("resident denied result projection failed: {error}");
                return MctIrohCallHandlerResult::failed("runtime state unavailable");
            }
            MctIrohCallHandlerResult::denied().with_route(Some(decision.decision_id), None)
        }
        RouteDisposition::Local { plan, observations } => {
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident route ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }

            let current_revisions = match current_resident_route_revisions(&paths, &request.call) {
                Ok(revisions) => revisions,
                Err(error) => {
                    eprintln!("resident route revision read failed: {error}");
                    return MctIrohCallHandlerResult::failed("runtime unavailable");
                }
            };
            let result_state_path = paths.state_path().to_path_buf();
            let result_observation_call = request.call.clone();
            let execution = match tokio::task::spawn_blocking(move || {
                execute_authorized_resident_child(
                    paths,
                    *plan,
                    request,
                    inline_payload,
                    current_revisions,
                )
            })
            .await
            {
                Ok(Ok(report)) => report,
                Ok(Err(error)) => {
                    eprintln!("resident child execution failed: {error}");
                    return MctIrohCallHandlerResult::failed("runtime execution failed");
                }
                Err(error) => {
                    eprintln!("resident child execution task failed: {error}");
                    return MctIrohCallHandlerResult::failed("runtime execution failed");
                }
            };

            let (result, mut observations, inline_result_payload, run_id) = execution.into_parts();
            if result_observation_call.origin == CallOrigin::TriggerFiring {
                observations.push(MctObservation {
                    observation_id: ObservationId::new(format!(
                        "obs:result-resident:{}",
                        result.call_id
                    ))
                    .expect("generated resident result observation id must be non-empty"),
                    observed_at: current_timestamp(),
                    kind: ObservationKind::ResultRecorded,
                    source_plane: SourcePlane::Kernel,
                    trace: ObservationTraceRef {
                        trace_id: result_observation_call.trace_context.trace_id.clone(),
                        span_id: Some(result_observation_call.trace_context.span_id.clone()),
                        parent_span_id: None,
                        external_trace_id: None,
                    },
                    call_id: Some(result.call_id.clone()),
                    decision_id: Some(result.authority_decision_ref.clone()),
                    subject_id: result_observation_call
                        .caller
                        .user_id
                        .as_ref()
                        .map(ToString::to_string)
                        .or_else(|| Some(result_observation_call.caller.node_id.to_string())),
                    resource_id: Some(format!("result-resident:{}", result.call_id)),
                    policy_revision: Some(
                        result_observation_call.authority_context.policy_revision,
                    ),
                    grants_revision: Some(
                        result_observation_call.authority_context.grants_revision,
                    ),
                    outcome: match result.outcome {
                        ResultOutcome::Success => ObservationOutcome::Completed,
                        ResultOutcome::Denied => ObservationOutcome::Denied,
                        ResultOutcome::Failed => ObservationOutcome::Failed,
                        ResultOutcome::TimedOut => ObservationOutcome::TimedOut,
                        ResultOutcome::Cancelled => ObservationOutcome::Cancelled,
                    },
                    visibility: ObservationVisibility::InternalOnly,
                    safe_message: "resident target result recorded".into(),
                    detail_ref: Some(format!("result_outcome:{:?}", result.outcome)),
                });
            }
            let state_observations = observations.clone();
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident execution ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            if let Some(run_id) = run_id {
                let persist_result =
                    MctRuntimeStateStore::open(&result_state_path).and_then(|state| {
                        state.append_run_observations(&run_id, &state_observations)?;
                        state.complete_run(&run_id, &result, mct_daemon::current_timestamp_string())
                    });
                if let Err(error) = persist_result {
                    eprintln!("resident durable result projection failed: {error}");
                    return MctIrohCallHandlerResult::failed("runtime state unavailable");
                }
            }

            result_to_call_handler_result("result-resident", &result, inline_result_payload)
        }
        RouteDisposition::Remote { plan, observations } => {
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident remote route ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            execute_authorized_resident_remote_call(paths, *plan, request, inline_payload, ledger)
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_resident_payload_process_child(children_dir: &Path) {
        write_resident_process_child_script(
            children_dir,
            "resident-payload-echo",
            b"#!/bin/sh\npayload=$(cat)\nprintf 'processed:%s' \"$payload\"\n",
        );
    }
    fn write_resident_process_child_script(children_dir: &Path, name: &str, script: &[u8]) {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        let child_dir = children_dir.join(name);
        std::fs::create_dir_all(&child_dir).unwrap();
        let artifact_path = child_dir.join(format!("{name}.wasm"));
        let manifest_path = child_dir.join("child.toml");
        std::fs::write(&artifact_path, script).unwrap();
        #[cfg(unix)]
        {
            let mut permissions = std::fs::metadata(&artifact_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&artifact_path, permissions).unwrap();
        }
        write_resident_child_manifest(&manifest_path, name, "handle");
        write_sha256_sidecar(&artifact_path, script);
        let manifest_bytes = std::fs::read(&manifest_path).unwrap();
        write_sha256_sidecar(&manifest_path, &manifest_bytes);
    }
    fn write_resident_child_manifest(manifest_path: &Path, name: &str, mode: &str) {
        std::fs::write(
            manifest_path,
            format!(
                r#"[child]
name = "{name}"
version = "0.1.0"
description = "resident test child"
kind = "child"
role = "app"

[child.ingress]
mode = "{mode}"

[child.artifact]
wasm = "{name}.wasm"

[child.contract]
allow = ["patina:demo/control@0.1.0.run"]

[needs]
toys = []

[relationships]
listens = []
"#
            ),
        )
        .unwrap();
    }
    fn write_sha256_sidecar(path: &Path, bytes: &[u8]) {
        use sha2::{Digest, Sha256};

        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(".sha256");
        std::fs::write(
            PathBuf::from(sidecar),
            format!("{:x}", Sha256::digest(bytes)),
        )
        .unwrap();
    }
    #[tokio::test]
    async fn jvm_bridge_json_call_enters_resident_route_path() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_payload_process_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let (mut request, payload) =
            jvm_bridge_protocol_request("patina:demo/control@0.1.0.run", r#"[{"from":"jvm"}]"#)
                .unwrap();
        request.call.call_id = CallId::new("call-jvm-bridge-test")
            .expect("string ID literal/generated value must be non-empty");
        assert_eq!(request.call.origin, CallOrigin::JvmAdapter);

        let result = execute_resident_call(
            ResidentRuntimePaths::new(config_path, children_dir, state_path),
            ledger.clone(),
            request,
            ResidentPayloadIngress::local(Some(payload)),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Completed);
        let result_payload = result
            .inline_result_payload
            .expect("result payload returned");
        assert_eq!(
            String::from_utf8(result_payload).unwrap(),
            r#"processed:[{"from":"jvm"}]"#
        );
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("call-jvm-bridge-test"));
        assert!(
            ledger_text.contains("RouteRevalidated") || ledger_text.contains("route_revalidated")
        );
    }
}

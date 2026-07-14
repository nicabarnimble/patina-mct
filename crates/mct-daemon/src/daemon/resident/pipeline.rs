//! Exact resident call-stage sequencing, before-effect durability barriers, and handler mapping.
//!
//! Stage logic belongs to payload, idempotency, decision, execution, and forwarding; this module
//! only orders those stages and maps their completed outputs into the transport handler result.

use super::*;

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
    execute_resident_call_at(paths, ledger, request, payload, current_timestamp()).await
}

pub(super) async fn execute_resident_call_at(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    payload: ResidentPayloadIngress,
    now: Timestamp,
) -> MctIrohCallHandlerResult {
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
    execute_idempotent_call(
        state_path,
        idempotency_ledger,
        idempotency_request,
        now,
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
            observations,
        } => {
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident route denial ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
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

            let (result, observations, inline_result_payload) = execution.into_parts();
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident execution ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
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

//! Caller-scoped durable idempotency around resident and standalone execution.

use super::*;

fn resident_idempotency_caller_scope(request: &MctCallProtocolRequest) -> String {
    if request.call.origin == CallOrigin::Iroh {
        format!("peer-binding:{}", request.authority.peer_binding_id)
    } else {
        format!(
            "local:{}:{}:{}:{}:{}",
            match request.call.origin {
                CallOrigin::JvmAdapter => "jvm",
                CallOrigin::WasmHost => "wasm",
                CallOrigin::ProcessHarness => "process",
                CallOrigin::Cli => "cli",
                CallOrigin::Iroh => "iroh",
            },
            request.call.caller.node_id,
            request
                .call
                .caller
                .user_id
                .as_ref()
                .map_or("-", UserId::as_str),
            request.call.caller.vision_id,
            request
                .call
                .caller
                .project_id
                .as_ref()
                .map_or("-", ProjectId::as_str),
        )
    }
}

fn resident_idempotency_fingerprint(request: &MctCallProtocolRequest) -> MctIdempotencyFingerprint {
    let payload_digest = match &request.payload {
        MctCallPayloadHandle::InlinePayload {
            blake3_digest_hex, ..
        } => format!("blake3:{blake3_digest_hex}"),
        MctCallPayloadHandle::ContentAddressedBlob { digest, .. } => digest.clone(),
        MctCallPayloadHandle::ExternalReference { external_ref, .. } => {
            format!(
                "external-ref:blake3:{}",
                blake3_hex(external_ref.as_bytes())
            )
        }
        MctCallPayloadHandle::Empty => "empty".into(),
    };
    MctIdempotencyFingerprint {
        target: mct_daemon::operation_id_from_target(&request.call.target),
        call_id: request.call.call_id.clone(),
        payload_digest,
    }
}

fn idempotency_expiry(now: &Timestamp) -> Result<Timestamp> {
    let now = now.as_str().parse::<jiff::Timestamp>()?;
    let expires = now.checked_add(jiff::SignedDuration::from_secs(MCT_IDEMPOTENCY_TTL_SECONDS))?;
    Timestamp::new(expires.to_string()).map_err(anyhow::Error::from)
}

fn handler_result_to_recorded_reply(result: &MctIrohCallHandlerResult) -> MctRecordedCallReply {
    MctRecordedCallReply {
        result_ref: result.result_ref.clone(),
        result_payload: result.result_payload.clone(),
        inline_result_payload: result.inline_result_payload.clone(),
        route_decision_id: result.route_decision_id.clone(),
        route_taken: result.route_taken.clone(),
        outcome: result.outcome,
        protocol_reason: result.protocol_reason,
        safe_message: result.safe_message.clone(),
    }
}

fn recorded_reply_to_handler_result(reply: MctRecordedCallReply) -> MctIrohCallHandlerResult {
    MctIrohCallHandlerResult {
        result_ref: reply.result_ref,
        result_payload: reply.result_payload,
        inline_result_payload: reply.inline_result_payload,
        route_decision_id: reply.route_decision_id,
        route_taken: reply.route_taken,
        outcome: reply.outcome,
        protocol_reason: reply.protocol_reason,
        safe_message: reply.safe_message,
    }
}

fn idempotency_refusal_result(reason: MctIdempotencyReason) -> MctIrohCallHandlerResult {
    let (outcome, protocol_reason, safe_message) = match reason {
        MctIdempotencyReason::IdempotencyKeyReuseMismatch => (
            CallProtocolOutcome::Malformed,
            CallProtocolReason::IdempotencyKeyReuseMismatch,
            "idempotency key does not match request",
        ),
        MctIdempotencyReason::IdempotencyBudgetFull => (
            CallProtocolOutcome::Failed,
            CallProtocolReason::IdempotencyBudgetFull,
            "idempotency capacity unavailable; retry later",
        ),
        MctIdempotencyReason::IdempotencyInProgress => (
            CallProtocolOutcome::Failed,
            CallProtocolReason::IdempotencyInProgress,
            "request already in progress; retry later",
        ),
        MctIdempotencyReason::ExecuteFresh | MctIdempotencyReason::ReplayCompleted => (
            CallProtocolOutcome::Failed,
            CallProtocolReason::ExecutionFailed,
            "runtime unavailable",
        ),
    };
    MctIrohCallHandlerResult {
        result_ref: None,
        result_payload: MctCallPayloadHandle::Empty,
        inline_result_payload: None,
        route_decision_id: None,
        route_taken: None,
        outcome,
        protocol_reason: Some(protocol_reason),
        safe_message: safe_message.into(),
    }
}

fn idempotency_reason_code(reason: MctIdempotencyReason) -> &'static str {
    match reason {
        MctIdempotencyReason::ExecuteFresh => "idempotency_execute_fresh",
        MctIdempotencyReason::ReplayCompleted => "idempotency_replay_completed",
        MctIdempotencyReason::IdempotencyKeyReuseMismatch => "idempotency_key_reuse_mismatch",
        MctIdempotencyReason::IdempotencyBudgetFull => "idempotency_budget_full",
        MctIdempotencyReason::IdempotencyInProgress => "idempotency_in_progress",
    }
}

fn idempotency_replay_observation_outcome(outcome: CallProtocolOutcome) -> ObservationOutcome {
    match outcome {
        CallProtocolOutcome::AcceptedForRouting | CallProtocolOutcome::Completed => {
            ObservationOutcome::Completed
        }
        CallProtocolOutcome::Malformed | CallProtocolOutcome::Denied => ObservationOutcome::Denied,
        CallProtocolOutcome::Failed => ObservationOutcome::Failed,
        CallProtocolOutcome::TimedOut => ObservationOutcome::TimedOut,
        CallProtocolOutcome::Cancelled => ObservationOutcome::Cancelled,
    }
}

fn resident_idempotency_observation(
    request: &MctCallProtocolRequest,
    caller_scope: &str,
    fingerprint: &MctIdempotencyFingerprint,
    reason: MctIdempotencyReason,
    outcome: ObservationOutcome,
) -> MctObservation {
    let replay = reason == MctIdempotencyReason::ReplayCompleted;
    let safe_message = match reason {
        MctIdempotencyReason::ExecuteFresh => "idempotency key reserved",
        MctIdempotencyReason::ReplayCompleted => "recorded result replayed",
        MctIdempotencyReason::IdempotencyKeyReuseMismatch => {
            "idempotency key does not match request"
        }
        MctIdempotencyReason::IdempotencyBudgetFull => {
            "idempotency capacity unavailable; retry later"
        }
        MctIdempotencyReason::IdempotencyInProgress => "request already in progress; retry later",
    };
    MctObservation {
        observation_id: ObservationId::new(format!(
            "obs:{}:{}",
            idempotency_reason_code(reason),
            request.protocol_request_id
        ))
        .expect("generated observation ID must be non-empty"),
        observed_at: current_timestamp(),
        kind: if replay {
            ObservationKind::ResultRecorded
        } else {
            ObservationKind::CallDenied
        },
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id: request.call.trace_context.trace_id.clone(),
            span_id: Some(request.call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(request.call.call_id.clone()),
        decision_id: None,
        subject_id: Some(format!(
            "scope-blake3:{}",
            blake3_hex(caller_scope.as_bytes())
        )),
        resource_id: Some(format!(
            "fingerprint-blake3:{}",
            blake3_hex(
                format!(
                    "{}:{}:{}",
                    fingerprint.target, fingerprint.call_id, fingerprint.payload_digest
                )
                .as_bytes()
            )
        )),
        policy_revision: Some(request.call.authority_context.policy_revision),
        grants_revision: Some(request.call.authority_context.grants_revision),
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: Some(format!(
            "idempotency_reason:{}",
            idempotency_reason_code(reason)
        )),
    }
}

pub(crate) async fn execute_idempotent_call<F, Fut>(
    state_path: PathBuf,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    now: Timestamp,
    execute: F,
) -> MctIrohCallHandlerResult
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = MctIrohCallHandlerResult>,
{
    let Some(idempotency_key) = request.idempotency_key.clone() else {
        return execute().await;
    };
    let caller_scope = resident_idempotency_caller_scope(&request);
    let fingerprint = resident_idempotency_fingerprint(&request);
    let expires_at = match idempotency_expiry(&now) {
        Ok(expires_at) => expires_at,
        Err(error) => {
            eprintln!("resident idempotency expiry failed: {error}");
            return MctIrohCallHandlerResult::failed("runtime unavailable");
        }
    };
    let reservation = match MctRuntimeStateStore::open(&state_path).and_then(|state| {
        state.reserve_call_idempotency(
            &caller_scope,
            &idempotency_key,
            &fingerprint,
            &now,
            &expires_at,
            MCT_IDEMPOTENCY_MAX_ENTRIES_PER_CALLER,
        )
    }) {
        Ok(reservation) => reservation,
        Err(error) => {
            eprintln!("resident idempotency reservation failed: {error}");
            return MctIrohCallHandlerResult::failed("runtime unavailable");
        }
    };

    match reservation {
        MctIdempotencyReservation::Replay(reply) => {
            let replay_outcome = idempotency_replay_observation_outcome(reply.outcome);
            if let Err(error) = ledger
                .append(vec![resident_idempotency_observation(
                    &request,
                    &caller_scope,
                    &fingerprint,
                    MctIdempotencyReason::ReplayCompleted,
                    replay_outcome,
                )])
                .await
            {
                eprintln!("resident idempotency replay observation failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            recorded_reply_to_handler_result(*reply)
                .with_protocol_reason(CallProtocolReason::IdempotencyReplayCompleted)
        }
        MctIdempotencyReservation::Refused(reason) => {
            if let Err(error) = ledger
                .append(vec![resident_idempotency_observation(
                    &request,
                    &caller_scope,
                    &fingerprint,
                    reason,
                    ObservationOutcome::Denied,
                )])
                .await
            {
                eprintln!("resident idempotency refusal observation failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            idempotency_refusal_result(reason)
        }
        MctIdempotencyReservation::ExecuteFresh => {
            let result = execute().await;
            let recorded = handler_result_to_recorded_reply(&result);
            if let Err(error) = MctRuntimeStateStore::open(&state_path).and_then(|state| {
                state.complete_call_idempotency(
                    &caller_scope,
                    &idempotency_key,
                    &fingerprint,
                    &recorded,
                    &now,
                )
            }) {
                eprintln!("resident idempotency completion failed: {error}");
                return MctIrohCallHandlerResult::failed("runtime unavailable");
            }
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resident_test_call(trace_id: TraceId) -> MctCall {
        let mut call = local_wasm_call(OperationTarget {
            namespace: "patina:demo".into(),
            interface_name: "control@0.1.0".into(),
            function_name: "run".into(),
        });
        call.call_id = CallId::new("call-resident-wit")
            .expect("string ID literal/generated value must be non-empty");
        call.trace_context.trace_id = trace_id;
        call.origin = CallOrigin::Iroh;
        call
    }
    fn resident_test_protocol_request(call: MctCall) -> MctCallProtocolRequest {
        MctCallProtocolRequest {
            protocol_request_id: ProtocolRequestId::new("proto-resident-wit")
                .expect("string ID literal/generated value must be non-empty"),
            authority: MctCallProtocolAuthority {
                hello_decision_id: DecisionId::new("decision-resident-wit-hello")
                    .expect("string ID literal/generated value must be non-empty"),
                peer_binding_id: PeerBindingId::new("binding-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                accepted_alpn: MCT_CALL_ALPN.into(),
                endpoint_id: EndpointIdText::new("endpoint-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                policy_revision: 1,
                grants_revision: 1,
            },
            received_over: IrohConnectionPresentation {
                endpoint_id: EndpointIdText::new("endpoint-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                alpn: MCT_CALL_ALPN.into(),
                connection_side: ConnectionSide::Incoming,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            call,
            payload: MctCallPayloadHandle::Empty,
            idempotency_key: None,
            received_observation_id: ObservationId::new("obs-resident-wit-received")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }
    fn write_resident_counting_process_child(children_dir: &Path) {
        write_resident_process_child_script(
            children_dir,
            "resident-counting",
            b"#!/bin/sh\ncounter=\"$0.count\"\ncount=$(cat \"$counter\" 2>/dev/null || printf 0)\ncount=$((count + 1))\nprintf '%s' \"$count\" >\"$counter\"\ncat >/dev/null\nprintf 'result-run-%s' \"$count\"\n",
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
    async fn in_flight_idempotency_duplicate_refuses_without_second_execution() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let ledger = ResidentLedgerWriter::spawn(dir.path().join("observations.jsonl")).unwrap();
        let mut request = resident_test_protocol_request(resident_test_call(
            TraceId::new("trace-idempotency-in-flight").unwrap(),
        ));
        request.idempotency_key = Some("in-flight-key".into());
        let execution_count = Arc::new(AtomicU64::new(0));
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());

        let first = tokio::spawn(execute_idempotent_call(
            state_path.clone(),
            ledger.clone(),
            request.clone(),
            Timestamp::new("2026-07-10T00:00:00Z").unwrap(),
            {
                let execution_count = Arc::clone(&execution_count);
                let started = Arc::clone(&started);
                let release = Arc::clone(&release);
                move || async move {
                    execution_count.fetch_add(1, Ordering::SeqCst);
                    started.notify_one();
                    release.notified().await;
                    MctIrohCallHandlerResult::completed(ResultRef::new("result-first").unwrap())
                }
            },
        ));
        started.notified().await;
        let duplicate = execute_idempotent_call(
            state_path,
            ledger.clone(),
            request,
            Timestamp::new("2026-07-10T00:00:01Z").unwrap(),
            {
                let execution_count = Arc::clone(&execution_count);
                move || async move {
                    execution_count.fetch_add(1, Ordering::SeqCst);
                    MctIrohCallHandlerResult::completed(ResultRef::new("result-duplicate").unwrap())
                }
            },
        )
        .await;
        assert_eq!(duplicate.outcome, CallProtocolOutcome::Failed);
        assert_eq!(
            duplicate.protocol_reason,
            Some(CallProtocolReason::IdempotencyInProgress)
        );
        assert_eq!(execution_count.load(Ordering::SeqCst), 1);

        release.notify_one();
        assert_eq!(first.await.unwrap().outcome, CallProtocolOutcome::Completed);
        assert_eq!(execution_count.load(Ordering::SeqCst), 1);
        ledger.close().await;
    }

    #[tokio::test]
    async fn resident_idempotency_replays_scopes_refuses_and_expires_without_payload_leakage() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_counting_process_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let paths =
            ResidentRuntimePaths::new(config_path, children_dir.clone(), state_path.clone());
        let payload = br#"{"request-secret-marker":true}"#.to_vec();
        let mut call = resident_test_call(TraceId::new("trace-idempotency").unwrap());
        call.call_id = CallId::new("call-idempotency").unwrap();
        call.payload_metadata.size_bytes = payload.len() as u64;
        let mut request = resident_test_protocol_request(call);
        request.idempotency_key = Some("same-key".into());
        request.payload = MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: "payload-idempotency".into(),
            content_type: "application/json".into(),
            size_bytes: payload.len() as u64,
            blake3_digest_hex: blake3_hex(&payload),
        };
        let now = Timestamp::new("2026-07-10T00:00:00Z").unwrap();

        let first = execute_resident_call_at(
            paths.clone(),
            ledger.clone(),
            request.clone(),
            ResidentPayloadIngress::remote(Some(payload.clone())),
            now.clone(),
        )
        .await;
        let replay = execute_resident_call_at(
            paths.clone(),
            ledger.clone(),
            request.clone(),
            ResidentPayloadIngress::remote(Some(payload.clone())),
            now.clone(),
        )
        .await;
        assert_eq!(replay.outcome, first.outcome);
        assert_eq!(replay.safe_message, first.safe_message);
        assert_eq!(replay.result_ref, first.result_ref);
        assert_eq!(replay.result_payload, first.result_payload);
        assert_eq!(replay.inline_result_payload, first.inline_result_payload);
        assert_eq!(replay.route_taken, first.route_taken);
        assert_eq!(first.outcome, CallProtocolOutcome::Completed);
        assert_eq!(
            String::from_utf8(first.inline_result_payload.clone().unwrap()).unwrap(),
            "result-run-1"
        );
        let counter_path = children_dir
            .join("resident-counting")
            .join("resident-counting.wasm.count");
        assert_eq!(std::fs::read_to_string(&counter_path).unwrap().trim(), "1");

        let mut other_caller = request.clone();
        other_caller.authority.peer_binding_id = PeerBindingId::new("binding-other").unwrap();
        other_caller.call.call_id = CallId::new("call-idempotency-other").unwrap();
        let isolated = execute_resident_call_at(
            paths.clone(),
            ledger.clone(),
            other_caller,
            ResidentPayloadIngress::remote(Some(payload.clone())),
            now.clone(),
        )
        .await;
        assert_eq!(isolated.outcome, CallProtocolOutcome::Completed);
        assert_eq!(std::fs::read_to_string(&counter_path).unwrap().trim(), "2");

        let mut mismatch = request.clone();
        mismatch.call.call_id = CallId::new("call-idempotency-mismatch").unwrap();
        let mismatch = execute_resident_call_at(
            paths.clone(),
            ledger.clone(),
            mismatch,
            ResidentPayloadIngress::remote(Some(payload.clone())),
            now.clone(),
        )
        .await;
        assert_eq!(mismatch.outcome, CallProtocolOutcome::Malformed);
        assert_eq!(
            mismatch.protocol_reason,
            Some(CallProtocolReason::IdempotencyKeyReuseMismatch)
        );
        assert_eq!(std::fs::read_to_string(&counter_path).unwrap().trim(), "2");

        let expired = execute_resident_call_at(
            paths.clone(),
            ledger.clone(),
            request.clone(),
            ResidentPayloadIngress::remote(Some(payload.clone())),
            Timestamp::new("2026-07-10T00:13:00Z").unwrap(),
        )
        .await;
        assert_eq!(expired.outcome, CallProtocolOutcome::Completed);
        assert_eq!(std::fs::read_to_string(&counter_path).unwrap().trim(), "3");

        let mut unkeyed = request;
        unkeyed.idempotency_key = None;
        unkeyed.call.call_id = CallId::new("call-unkeyed-repeat").unwrap();
        let first_unkeyed = execute_resident_call_at(
            paths.clone(),
            ledger.clone(),
            unkeyed.clone(),
            ResidentPayloadIngress::remote(Some(payload.clone())),
            now.clone(),
        )
        .await;
        let second_unkeyed = execute_resident_call_at(
            paths,
            ledger.clone(),
            unkeyed,
            ResidentPayloadIngress::remote(Some(payload)),
            now,
        )
        .await;
        assert_eq!(first_unkeyed.outcome, CallProtocolOutcome::Completed);
        assert_eq!(second_unkeyed.outcome, CallProtocolOutcome::Completed);
        assert_eq!(std::fs::read_to_string(&counter_path).unwrap().trim(), "5");
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("idempotency_replay_completed"));
        assert!(ledger_text.contains("idempotency_key_reuse_mismatch"));
        assert!(!ledger_text.contains("same-key"));
        assert!(!ledger_text.contains("request-secret-marker"));
        let state_bytes = std::fs::read(&state_path).unwrap();
        assert!(
            !state_bytes
                .windows(b"request-secret-marker".len())
                .any(|window| window == b"request-secret-marker")
        );
    }

    /// Covers `MctCallProtocol.MatchingCompletedRetryReplaysRecordedReply` for cancellation.
    #[tokio::test]
    async fn cancelled_idempotent_reply_replays_cancelled_with_durable_observation() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let mut request = resident_test_protocol_request(resident_test_call(
            TraceId::new("trace-idempotency-cancelled").unwrap(),
        ));
        request.idempotency_key = Some("cancelled-key".into());
        let execution_count = Arc::new(AtomicU64::new(0));
        let now = Timestamp::new("2026-07-10T00:00:00Z").unwrap();

        let first = execute_idempotent_call(
            state_path.clone(),
            ledger.clone(),
            request.clone(),
            now.clone(),
            {
                let execution_count = Arc::clone(&execution_count);
                move || async move {
                    execution_count.fetch_add(1, Ordering::SeqCst);
                    MctIrohCallHandlerResult::cancelled("cancelled")
                }
            },
        )
        .await;
        let replay = execute_idempotent_call(state_path, ledger.clone(), request, now, {
            let execution_count = Arc::clone(&execution_count);
            move || async move {
                execution_count.fetch_add(1, Ordering::SeqCst);
                MctIrohCallHandlerResult::failed("must not execute")
            }
        })
        .await;

        assert_eq!(first.outcome, CallProtocolOutcome::Cancelled);
        assert_eq!(replay.outcome, CallProtocolOutcome::Cancelled);
        assert_eq!(
            replay.protocol_reason,
            Some(CallProtocolReason::IdempotencyReplayCompleted)
        );
        assert_eq!(execution_count.load(Ordering::SeqCst), 1);
        ledger.close().await;

        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        let replay_entry = entries
            .iter()
            .find(|entry| {
                entry.observation.kind == ObservationKind::ResultRecorded
                    && entry.observation.safe_message == "recorded result replayed"
            })
            .unwrap();
        assert_eq!(
            replay_entry.observation.outcome,
            ObservationOutcome::Cancelled
        );
        assert_eq!(replay_entry.durability_class, DurabilityClass::BeforeEffect);
    }
}

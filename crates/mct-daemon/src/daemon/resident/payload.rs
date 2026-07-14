//! Resident payload ingress, local-CAS resolution, integrity facts, and payload handles.

use super::*;

#[derive(Clone, Debug)]
pub(crate) enum ResidentPayloadIngress {
    Local { inline_payload: Option<Vec<u8>> },
    Remote { inline_payload: Option<Vec<u8>> },
}

impl ResidentPayloadIngress {
    pub(crate) fn remote(inline_payload: Option<Vec<u8>>) -> Self {
        Self::Remote { inline_payload }
    }

    pub(crate) fn local(inline_payload: Option<Vec<u8>>) -> Self {
        Self::Local { inline_payload }
    }

    fn into_parts(self) -> (Option<Vec<u8>>, bool) {
        match self {
            Self::Local { inline_payload } => (inline_payload, true),
            Self::Remote { inline_payload } => (inline_payload, false),
        }
    }
}

pub(super) struct VerifiedRequestPayload(Option<Vec<u8>>);

impl VerifiedRequestPayload {
    pub(super) fn into_inner(self) -> Option<Vec<u8>> {
        self.0
    }
}

pub(crate) fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

pub(super) fn inline_payload_content_type(handle: &MctCallPayloadHandle) -> Option<&str> {
    match handle {
        MctCallPayloadHandle::InlinePayload { content_type, .. }
        | MctCallPayloadHandle::ContentAddressedBlob { content_type, .. } => Some(content_type),
        MctCallPayloadHandle::ExternalReference { content_type, .. } => content_type.as_deref(),
        MctCallPayloadHandle::Empty => None,
    }
}

pub(super) fn inline_result_payload_handle(
    reference: impl Into<String>,
    content_type: impl Into<String>,
    bytes: &[u8],
) -> MctCallPayloadHandle {
    MctCallPayloadHandle::InlinePayload {
        inline_payload_ref: reference.into(),
        content_type: content_type.into(),
        size_bytes: bytes.len() as u64,
        blake3_digest_hex: blake3_hex(bytes),
    }
}

pub(super) fn resident_payload_fact_observation(
    call: &MctCall,
    direction: &str,
    bytes: &[u8],
    classification: &str,
) -> MctObservation {
    let digest = blake3_hex(bytes);
    MctObservation {
        observation_id: ObservationId::new(format!(
            "obs-resident-payload-{direction}:{}",
            call.call_id
        ))
        .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::AdapterEffectCompleted,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: None,
        subject_id: Some(direction.into()),
        resource_id: Some(format!(
            "payload:{direction}:size={}:digest={digest}:class={classification}",
            bytes.len()
        )),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome: ObservationOutcome::Completed,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: format!("{direction} payload integrity facts recorded"),
        detail_ref: None,
    }
}

pub(super) fn observed_local_blob_payload(bytes: &[u8]) -> MctPayloadIntegrityObservation {
    MctPayloadIntegrityObservation {
        inline_bytes_present: true,
        content_addressed_blob_fetch_attempted: true,
        observed_size_bytes: Some(bytes.len() as u64),
        observed_blake3_digest_hex: Some(blake3_hex(bytes)),
    }
}

pub(super) fn resident_payload_integrity_failure_observation(
    call: &MctCall,
    direction: &str,
    handle: &MctCallPayloadHandle,
    decision: &MctPayloadIntegrityDecision,
) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::new(format!(
            "obs-resident-payload-{direction}-failed:{}",
            call.call_id
        ))
        .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::AdapterEffectCompleted,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: None,
        subject_id: Some(direction.into()),
        resource_id: Some(format!(
            "payload:{direction}:size={}:digest={}:class={}:reason={:?}",
            handle.declared_size_bytes(),
            declared_payload_digest(handle).unwrap_or("none"),
            call.payload_metadata.data_classification,
            decision.reason
        )),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome: ObservationOutcome::Failed,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: decision.safe_message.clone(),
        detail_ref: None,
    }
}

pub(super) fn declared_payload_digest(handle: &MctCallPayloadHandle) -> Option<&str> {
    match handle {
        MctCallPayloadHandle::InlinePayload {
            blake3_digest_hex, ..
        } => Some(blake3_digest_hex),
        MctCallPayloadHandle::ContentAddressedBlob { digest, .. } => Some(digest),
        MctCallPayloadHandle::ExternalReference { .. } | MctCallPayloadHandle::Empty => None,
    }
}

pub(super) struct PayloadFailure {
    safe_message: String,
    observations: Vec<MctObservation>,
}

impl PayloadFailure {
    pub(super) fn into_parts(self) -> (String, Vec<MctObservation>) {
        (self.safe_message, self.observations)
    }
}

pub(super) async fn resolve_resident_request_payload(
    paths: &ResidentRuntimePaths,
    request: &MctCallProtocolRequest,
    payload: ResidentPayloadIngress,
) -> std::result::Result<VerifiedRequestPayload, PayloadFailure> {
    let (inline_payload, local_ingress) = payload.into_parts();
    if !local_ingress {
        // The Iroh adapter has already verified remote inline bytes before this pipeline.
        return Ok(VerifiedRequestPayload(inline_payload));
    }
    if !matches!(
        request.payload,
        MctCallPayloadHandle::ContentAddressedBlob { .. }
    ) {
        let observed = MctPayloadIntegrityObservation {
            inline_bytes_present: inline_payload.is_some(),
            content_addressed_blob_fetch_attempted: false,
            observed_size_bytes: inline_payload.as_ref().map(|bytes| bytes.len() as u64),
            observed_blake3_digest_hex: inline_payload.as_ref().map(|bytes| blake3_hex(bytes)),
        };
        let decision = evaluate_payload_integrity(
            PayloadIntegritySubject::Request,
            &request.payload,
            &observed,
            mct_iroh::MCT_INLINE_PAYLOAD_MAX_BYTES as u64,
        );
        if decision.outcome != PayloadIntegrityOutcome::Matched {
            return Err(PayloadFailure {
                safe_message: decision.safe_message.clone(),
                observations: vec![resident_payload_integrity_failure_observation(
                    &request.call,
                    "request",
                    &request.payload,
                    &decision,
                )],
            });
        }
        return Ok(VerifiedRequestPayload(inline_payload));
    }

    let state_path = paths.state_path().to_path_buf();
    let handle = request.payload.clone();
    let fetched = tokio::task::spawn_blocking(move || {
        local_blob_store_for_state_path(state_path).fetch(&handle)
    })
    .await
    .map_err(|error| {
        resident_payload_resolution_failure(
            &request.call,
            &request.payload,
            PayloadIntegrityReason::PayloadBlobUnavailable,
            format!("join local blob fetch: {error}"),
        )
    })?;

    let mut fetched_bytes = None;
    let observed = match fetched {
        Ok(bytes) => {
            let observed = observed_local_blob_payload(&bytes);
            fetched_bytes = Some(bytes);
            observed
        }
        Err(MctLocalBlobStoreError::PayloadBlobUnavailable) => {
            MctPayloadIntegrityObservation::missing_content_addressed_blob()
        }
        Err(MctLocalBlobStoreError::BlobTooLarge) => MctPayloadIntegrityObservation {
            inline_bytes_present: true,
            content_addressed_blob_fetch_attempted: true,
            observed_size_bytes: Some(MCT_BLOB_MAX_BYTES as u64 + 1),
            observed_blake3_digest_hex: declared_payload_digest(&request.payload)
                .map(str::to_owned),
        },
        Err(_) => {
            return Err(resident_payload_resolution_failure(
                &request.call,
                &request.payload,
                PayloadIntegrityReason::PayloadBlobUnavailable,
                "blob store unavailable".into(),
            ));
        }
    };

    let decision = evaluate_payload_integrity(
        PayloadIntegritySubject::Request,
        &request.payload,
        &observed,
        MCT_BLOB_MAX_BYTES as u64,
    );
    if decision.outcome != PayloadIntegrityOutcome::Matched {
        return Err(PayloadFailure {
            safe_message: decision.safe_message.clone(),
            observations: vec![resident_payload_integrity_failure_observation(
                &request.call,
                "request",
                &request.payload,
                &decision,
            )],
        });
    }

    Ok(VerifiedRequestPayload(fetched_bytes))
}

pub(super) fn resident_payload_resolution_failure(
    call: &MctCall,
    handle: &MctCallPayloadHandle,
    reason: PayloadIntegrityReason,
    safe_message: String,
) -> PayloadFailure {
    let decision = MctPayloadIntegrityDecision {
        subject: PayloadIntegritySubject::Request,
        outcome: PayloadIntegrityOutcome::Mismatch,
        reason,
        safe_message: safe_message.clone(),
    };
    PayloadFailure {
        safe_message,
        observations: vec![resident_payload_integrity_failure_observation(
            call, "request", handle, &decision,
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

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
    async fn resident_local_blob_payload_delivery_returns_digest_and_keeps_ledger_byte_free() {
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
        let payload = br#"{"secret":"blob-marker"}"#.to_vec();
        let payload_base64 = BASE64_STANDARD.encode(&payload);
        let payload_digest = blake3_hex(&payload);
        let handle = local_blob_store_for_state_path(&state_path)
            .ingest_reader(
                &payload_digest,
                payload.len() as u64,
                "application/json",
                std::io::Cursor::new(&payload),
            )
            .unwrap();
        let trace_id = TraceId::new("trace-resident-blob-payload")
            .expect("string ID literal/generated value must be non-empty");
        let mut call = resident_test_call(trace_id);
        call.call_id = CallId::new("call-resident-blob-payload")
            .expect("string ID literal/generated value must be non-empty");
        call.payload_metadata.size_bytes = payload.len() as u64;
        let mut request = resident_test_protocol_request(call);
        request.payload = handle;

        let result = execute_resident_call(
            ResidentRuntimePaths::new(config_path, children_dir, state_path),
            ledger.clone(),
            request,
            ResidentPayloadIngress::local(None),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Completed);
        let result_payload = result
            .inline_result_payload
            .expect("result payload returned");
        let expected_result = br#"processed:{"secret":"blob-marker"}"#.to_vec();
        let expected_result_base64 = BASE64_STANDARD.encode(&expected_result);
        assert_eq!(result_payload, expected_result);
        assert!(matches!(
            result.result_payload,
            MctCallPayloadHandle::InlinePayload { ref blake3_digest_hex, .. }
                if blake3_digest_hex == &blake3_hex(&expected_result)
        ));
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("call-resident-blob-payload"));
        assert!(ledger_text.contains(&payload_digest));
        assert!(ledger_text.contains("payload:request:size="));
        assert!(ledger_text.contains("payload:result:size="));
        assert!(!ledger_text.contains("blob-marker"));
        assert!(!ledger_text.contains("processed:"));
        assert!(!ledger_text.contains(&payload_base64));
        assert!(!ledger_text.contains(&expected_result_base64));
    }

    #[tokio::test]
    async fn resident_local_blob_absent_fails_closed_before_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_payload_process_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let payload = b"missing blob bytes";
        let payload_digest = blake3_hex(payload);
        let trace_id = TraceId::new("trace-resident-blob-missing")
            .expect("string ID literal/generated value must be non-empty");
        let mut call = resident_test_call(trace_id);
        call.call_id = CallId::new("call-resident-blob-missing")
            .expect("string ID literal/generated value must be non-empty");
        call.payload_metadata.size_bytes = payload.len() as u64;
        let mut request = resident_test_protocol_request(call);
        request.payload = mct_daemon::content_addressed_blob_handle(
            payload_digest,
            "application/json",
            payload.len() as u64,
        );

        let result = execute_resident_call(
            ResidentRuntimePaths::new(config_path, children_dir, state_path),
            ledger.clone(),
            request,
            ResidentPayloadIngress::local(None),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Failed);
        assert_eq!(result.safe_message, "payload blob unavailable");
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("PayloadBlobUnavailable"));
        assert!(!ledger_text.contains("missing blob bytes"));
        assert!(!ledger_text.contains(&BASE64_STANDARD.encode(payload)));
    }

    #[tokio::test]
    async fn resident_local_blob_tamper_fails_closed_via_digest_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_payload_process_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let payload = br#"{"secret":"trusted-blob"}"#.to_vec();
        let payload_digest = blake3_hex(&payload);
        let store = local_blob_store_for_state_path(&state_path);
        let handle = store
            .ingest_reader(
                &payload_digest,
                payload.len() as u64,
                "application/json",
                std::io::Cursor::new(&payload),
            )
            .unwrap();
        let tampered = vec![b'x'; payload.len()];
        std::fs::write(store.visible_path(&payload_digest).unwrap(), &tampered).unwrap();
        let trace_id = TraceId::new("trace-resident-blob-tamper")
            .expect("string ID literal/generated value must be non-empty");
        let mut call = resident_test_call(trace_id);
        call.call_id = CallId::new("call-resident-blob-tamper")
            .expect("string ID literal/generated value must be non-empty");
        call.payload_metadata.size_bytes = payload.len() as u64;
        let mut request = resident_test_protocol_request(call);
        request.payload = handle;

        let result = execute_resident_call(
            ResidentRuntimePaths::new(config_path, children_dir, state_path),
            ledger.clone(),
            request,
            ResidentPayloadIngress::local(None),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Failed);
        assert_eq!(result.safe_message, "malformed call payload");
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("PayloadDigestMismatch"));
        assert!(!ledger_text.contains("trusted-blob"));
        assert!(!ledger_text.contains(&BASE64_STANDARD.encode(&payload)));
        assert!(!ledger_text.contains(&BASE64_STANDARD.encode(&tampered)));
    }
}

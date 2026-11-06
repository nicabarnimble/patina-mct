use super::*;
use anyhow::ensure;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicU64, Ordering},
};

const LOCAL_CALL_ALPN: &str = "mct/local-call/0";
const MIN_MESH_JOB_PIECES: usize = 2;
const MAX_MESH_JOB_PIECES: usize = 16;
static NEXT_LOCAL_CALL_OBSERVATION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MctResidentCallSubmission {
    protocol_request_id: ProtocolRequestId,
    call_id: CallId,
    target: OperationTarget,
    payload_metadata: PayloadMetadata,
    authority_context: AuthorityContextSnapshot,
    deadline: Timestamp,
    trace_context: TraceContext,
    payload: MctCallPayloadHandle,
    inline_payload_base64: Option<String>,
    idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MctResidentJobPieceSubmission {
    piece_id: String,
    target: OperationTarget,
    payload_metadata: PayloadMetadata,
    payload: MctCallPayloadHandle,
    inline_payload_base64: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MctResidentJobSubmission {
    job_id: String,
    authority_context: AuthorityContextSnapshot,
    deadline: Timestamp,
    trace_context: TraceContext,
    pieces: Vec<MctResidentJobPieceSubmission>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MctResidentSubmission {
    Call(Box<MctResidentCallSubmission>),
    Job(Box<MctResidentJobSubmission>),
}

#[derive(Serialize)]
struct MctResidentCallResponse {
    outcome: CallProtocolOutcome,
    protocol_reason: Option<CallProtocolReason>,
    safe_message: String,
    result_ref: Option<ResultRef>,
    result_payload: MctCallPayloadHandle,
    route_decision_id: Option<DecisionId>,
    route_taken: Option<RouteTaken>,
    inline_result_payload_base64: Option<String>,
}

#[derive(Serialize)]
struct MctResidentJobPieceResponse {
    piece_id: String,
    call_id: CallId,
    #[serde(flatten)]
    result: MctResidentCallResponse,
}

#[derive(Serialize)]
struct MctResidentJobResponse {
    job_id: String,
    outcome: CallProtocolOutcome,
    safe_message: String,
    pieces: Vec<MctResidentJobPieceResponse>,
}

struct MctPreparedJobPiece {
    piece_id: String,
    request: MctCallProtocolRequest,
    inline_payload: Option<Vec<u8>>,
}

fn local_call_response(status_code: u16, value: impl Serialize) -> MctControlPlaneResponse {
    MctControlPlaneResponse {
        status_code,
        content_type: "application/json".into(),
        body: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "null".into()),
    }
}

fn generated_local_call_fact_id(prefix: &str) -> String {
    let sequence = NEXT_LOCAL_CALL_OBSERVATION_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}:{}:{sequence}", current_timestamp())
}

struct LocalCallObservationFact {
    kind: ObservationKind,
    source_plane: SourcePlane,
    outcome: ObservationOutcome,
    trace_id: TraceId,
    call_id: Option<CallId>,
    decision_id: Option<DecisionId>,
    subject_id: Option<String>,
    resource_id: Option<String>,
    policy_revision: Option<u64>,
    grants_revision: Option<u64>,
    safe_message: String,
    detail_ref: Option<String>,
}

fn local_call_observation(fact: LocalCallObservationFact) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::new(generated_local_call_fact_id("obs-local-call"))
            .expect("generated observation ID must be non-empty"),
        observed_at: current_timestamp(),
        kind: fact.kind,
        source_plane: fact.source_plane,
        trace: ObservationTraceRef {
            trace_id: fact.trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: fact.call_id,
        decision_id: fact.decision_id,
        subject_id: fact.subject_id,
        resource_id: fact.resource_id,
        policy_revision: fact.policy_revision,
        grants_revision: fact.grants_revision,
        outcome: fact.outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: fact.safe_message,
        detail_ref: fact.detail_ref,
    }
}

fn local_transport_trace() -> TraceId {
    TraceId::new(generated_local_call_fact_id("trace-local-call"))
        .expect("generated trace ID must be non-empty")
}

fn authenticated_uid_subject(uid: u32) -> String {
    format!("uid-blake3:{}", blake3_hex(uid.to_string().as_bytes()))
}

async fn append_local_refusal(
    ledger: &ResidentLedgerWriter,
    peer_uid: Option<u32>,
    status_code: u16,
    safe_message: &'static str,
    detail: &'static str,
) -> Option<MctControlPlaneResponse> {
    let observation = local_call_observation(LocalCallObservationFact {
        kind: ObservationKind::CallDenied,
        source_plane: SourcePlane::Kernel,
        outcome: ObservationOutcome::Denied,
        trace_id: local_transport_trace(),
        call_id: None,
        decision_id: None,
        subject_id: peer_uid.map(authenticated_uid_subject),
        resource_id: Some("mct-local-call-uds".into()),
        policy_revision: None,
        grants_revision: None,
        safe_message: safe_message.into(),
        detail_ref: Some(format!("local_call_reason:{detail}")),
    });
    ledger.append(vec![observation]).await.ok()?;
    Some(local_call_response(
        status_code,
        serde_json::json!({"error": safe_message}),
    ))
}

fn local_protocol_request(
    identity: MctLocalNodeIdentity,
    uid: u32,
    submission: MctResidentCallSubmission,
) -> Result<(MctCallProtocolRequest, Option<Vec<u8>>)> {
    let inline_payload = submission
        .inline_payload_base64
        .as_deref()
        .map(|encoded| {
            BASE64_STANDARD
                .decode(encoded)
                .context("decode inline call payload")
        })
        .transpose()?;
    let caller = CallerIdentity {
        node_id: identity.node_id,
        user_id: Some(
            UserId::new(format!("uid:{uid}"))
                .expect("canonical authenticated UID must be non-empty"),
        ),
        vision_id: identity.vision_id,
        project_id: None,
    };
    let call = MctCall {
        call_id: submission.call_id,
        caller,
        target: submission.target,
        payload_metadata: submission.payload_metadata,
        authority_context: submission.authority_context,
        deadline: submission.deadline,
        trace_context: submission.trace_context,
        origin: CallOrigin::JvmAdapter,
    };
    let endpoint_id = identity.endpoint_id;
    let protocol_request_id = submission.protocol_request_id;
    let request = MctCallProtocolRequest {
        authority: MctCallProtocolAuthority {
            hello_decision_id: DecisionId::new(format!(
                "decision-local-call:{}",
                protocol_request_id
            ))
            .expect("generated decision ID must be non-empty"),
            peer_binding_id: PeerBindingId::new(format!("binding-local-uid:{uid}"))
                .expect("generated binding ID must be non-empty"),
            vision_id: call.caller.vision_id.clone(),
            accepted_alpn: LOCAL_CALL_ALPN.into(),
            endpoint_id: endpoint_id.clone(),
            policy_revision: call.authority_context.policy_revision,
            expected_receiver_grants_authority: call
                .authority_context
                .expected_receiver_grants_authority
                .clone(),
        },
        received_over: IrohConnectionPresentation {
            endpoint_id,
            alpn: LOCAL_CALL_ALPN.into(),
            connection_side: ConnectionSide::Incoming,
            path_class: PathClass::Direct,
            relay_url: None,
            presented_capability_ref: None,
        },
        call,
        payload: submission.payload,
        idempotency_key: submission.idempotency_key,
        received_observation_id: ObservationId::new(format!(
            "obs-local-call-received:{protocol_request_id}"
        ))
        .expect("generated observation ID must be non-empty"),
        protocol_request_id,
    };
    request.validate().map_err(anyhow::Error::from)?;
    Ok((request, inline_payload))
}

fn validate_mesh_job_identifier(label: &str, value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 64
            && value.as_bytes()[0].is_ascii_alphanumeric()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
        "invalid mesh job {label}"
    );
    Ok(())
}

fn local_job_protocol_requests(
    identity: MctLocalNodeIdentity,
    uid: u32,
    submission: MctResidentJobSubmission,
) -> Result<Vec<MctPreparedJobPiece>> {
    validate_mesh_job_identifier("id", &submission.job_id)?;
    ensure!(
        (MIN_MESH_JOB_PIECES..=MAX_MESH_JOB_PIECES).contains(&submission.pieces.len()),
        "mesh job piece count is outside the supported bound"
    );
    let mut piece_ids = BTreeSet::new();
    let mut requests = Vec::with_capacity(submission.pieces.len());
    for piece in submission.pieces {
        validate_mesh_job_identifier("piece id", &piece.piece_id)?;
        ensure!(
            piece_ids.insert(piece.piece_id.clone()),
            "duplicate mesh job piece id"
        );
        let call_id = CallId::new(format!("{}:{}", submission.job_id, piece.piece_id))?;
        let piece_span = SpanId::new(format!(
            "span-mesh-job-{}-{}",
            submission.job_id, piece.piece_id
        ))?;
        let call = MctResidentCallSubmission {
            protocol_request_id: ProtocolRequestId::new(format!(
                "proto-mesh-job-{}-{}",
                submission.job_id, piece.piece_id
            ))?,
            call_id,
            target: piece.target,
            payload_metadata: piece.payload_metadata,
            authority_context: submission.authority_context.clone(),
            deadline: submission.deadline.clone(),
            trace_context: TraceContext {
                trace_id: submission.trace_context.trace_id.clone(),
                span_id: piece_span,
            },
            payload: piece.payload,
            inline_payload_base64: piece.inline_payload_base64,
            idempotency_key: Some(format!(
                "mct-mesh-job:{}:{}",
                submission.job_id, piece.piece_id
            )),
        };
        let (request, inline_payload) = local_protocol_request(identity.clone(), uid, call)?;
        requests.push(MctPreparedJobPiece {
            piece_id: piece.piece_id,
            request,
            inline_payload,
        });
    }
    Ok(requests)
}

fn local_submission_observations(
    request: &MctCallProtocolRequest,
    uid: u32,
) -> Vec<MctObservation> {
    let trace_id = request.call.trace_context.trace_id.clone();
    let subject = Some(authenticated_uid_subject(uid));
    let resource = Some(mct_daemon::operation_id_from_target(&request.call.target));
    let policy_revision = Some(request.call.authority_context.policy_revision);
    let grants_revision = Some(
        request
            .call
            .authority_context
            .expected_receiver_grants_authority
            .generation,
    );
    vec![
        local_call_observation(LocalCallObservationFact {
            kind: ObservationKind::CallReceived,
            source_plane: SourcePlane::Adapter,
            outcome: ObservationOutcome::Informational,
            trace_id: trace_id.clone(),
            call_id: Some(request.call.call_id.clone()),
            decision_id: None,
            subject_id: subject.clone(),
            resource_id: resource.clone(),
            policy_revision,
            grants_revision,
            safe_message: "authenticated local call received".into(),
            detail_ref: Some("call_origin:jvm_adapter;ingress:local_application_bridge".into()),
        }),
        local_call_observation(LocalCallObservationFact {
            kind: ObservationKind::CallConstructed,
            source_plane: SourcePlane::Adapter,
            outcome: ObservationOutcome::Allowed,
            trace_id,
            call_id: Some(request.call.call_id.clone()),
            decision_id: Some(request.authority.hello_decision_id.clone()),
            subject_id: subject,
            resource_id: resource,
            policy_revision,
            grants_revision,
            safe_message: "local call accepted for evaluation".into(),
            detail_ref: Some("call_origin:jvm_adapter;ingress:local_application_bridge".into()),
        }),
    ]
}

fn local_job_observation(
    job_id: &str,
    trace_id: TraceId,
    uid: u32,
    kind: ObservationKind,
    outcome: ObservationOutcome,
    safe_message: &str,
    detail_ref: String,
) -> Result<MctObservation> {
    Ok(local_call_observation(LocalCallObservationFact {
        kind,
        source_plane: if kind == ObservationKind::CallReceived {
            SourcePlane::Adapter
        } else {
            SourcePlane::Kernel
        },
        outcome,
        trace_id,
        call_id: Some(CallId::new(job_id)?),
        decision_id: None,
        subject_id: Some(authenticated_uid_subject(uid)),
        resource_id: Some("mct-local-mesh-job".into()),
        policy_revision: None,
        grants_revision: None,
        safe_message: safe_message.into(),
        detail_ref: Some(detail_ref),
    }))
}

fn terminal_observation(
    request: &MctCallProtocolRequest,
    result: &MctIrohCallHandlerResult,
) -> MctObservation {
    let outcome = match result.outcome {
        CallProtocolOutcome::AcceptedForRouting => ObservationOutcome::Allowed,
        CallProtocolOutcome::Completed => ObservationOutcome::Completed,
        CallProtocolOutcome::Malformed | CallProtocolOutcome::Denied => ObservationOutcome::Denied,
        CallProtocolOutcome::Failed => ObservationOutcome::Failed,
        CallProtocolOutcome::TimedOut => ObservationOutcome::TimedOut,
        CallProtocolOutcome::Cancelled => ObservationOutcome::Cancelled,
    };
    local_call_observation(LocalCallObservationFact {
        kind: ObservationKind::ResultRecorded,
        source_plane: SourcePlane::Kernel,
        outcome,
        trace_id: request.call.trace_context.trace_id.clone(),
        call_id: Some(request.call.call_id.clone()),
        decision_id: result.route_decision_id.clone(),
        subject_id: Some(authenticated_uid_subject(
            request
                .call
                .caller
                .user_id
                .as_ref()
                .and_then(|user| user.as_str().strip_prefix("uid:"))
                .and_then(|uid| uid.parse::<u32>().ok())
                .expect("local caller UID is canonical"),
        )),
        resource_id: result.result_ref.as_ref().map(ToString::to_string),
        policy_revision: Some(request.call.authority_context.policy_revision),
        grants_revision: Some(
            request
                .call
                .authority_context
                .expected_receiver_grants_authority
                .generation,
        ),
        safe_message: "local call result recorded".into(),
        detail_ref: Some(format!("call_outcome:{:?}", result.outcome)),
    })
}

fn project_local_call_result_value(result: MctIrohCallHandlerResult) -> MctResidentCallResponse {
    MctResidentCallResponse {
        outcome: result.outcome,
        protocol_reason: result.protocol_reason,
        safe_message: result.safe_message,
        result_ref: result.result_ref,
        result_payload: result.result_payload,
        route_decision_id: result.route_decision_id,
        route_taken: result.route_taken,
        inline_result_payload_base64: result
            .inline_result_payload
            .map(|bytes| BASE64_STANDARD.encode(bytes)),
    }
}

async fn preflight_local_submission(
    ledger: &ResidentLedgerWriter,
    expected_uid: u32,
    peer: Option<MctUdsPeerCredentials>,
    declared_body_len: usize,
) -> MctUdsControlCallPreflight {
    let Some(peer) = peer else {
        return MctUdsControlCallPreflight::Refused(
            append_local_refusal(
                ledger,
                None,
                401,
                "not authorized",
                "peer_credentials_unavailable",
            )
            .await,
        );
    };
    if peer.uid != expected_uid {
        return MctUdsControlCallPreflight::Refused(
            append_local_refusal(
                ledger,
                Some(peer.uid),
                403,
                "not authorized",
                "peer_uid_mismatch",
            )
            .await,
        );
    }
    if declared_body_len > mct_iroh::MCT_CALL_FRAME_READ_BUDGET_BYTES {
        return MctUdsControlCallPreflight::Refused(
            append_local_refusal(
                ledger,
                Some(peer.uid),
                413,
                "call frame too large",
                "frame_too_large",
            )
            .await,
        );
    }
    MctUdsControlCallPreflight::Authenticated(peer)
}

async fn execute_local_protocol_request(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    peer_uid: u32,
    request: MctCallProtocolRequest,
    inline_payload: Option<Vec<u8>>,
) -> Option<MctResidentCallResponse> {
    if ledger
        .append(local_submission_observations(&request, peer_uid))
        .await
        .is_err()
    {
        return None;
    }
    let result = execute_resident_call(
        paths,
        ledger.clone(),
        request.clone(),
        ResidentPayloadIngress::local(inline_payload),
    )
    .await;
    if ledger
        .append(vec![terminal_observation(&request, &result)])
        .await
        .is_err()
    {
        return None;
    }
    Some(project_local_call_result_value(result))
}

fn mesh_job_outcome(pieces: &[MctResidentJobPieceResponse]) -> CallProtocolOutcome {
    if pieces
        .iter()
        .all(|piece| piece.result.outcome == CallProtocolOutcome::Completed)
    {
        CallProtocolOutcome::Completed
    } else if pieces
        .iter()
        .any(|piece| piece.result.outcome == CallProtocolOutcome::TimedOut)
    {
        CallProtocolOutcome::TimedOut
    } else if pieces.iter().any(|piece| {
        matches!(
            piece.result.outcome,
            CallProtocolOutcome::Failed | CallProtocolOutcome::Malformed
        )
    }) {
        CallProtocolOutcome::Failed
    } else if pieces
        .iter()
        .any(|piece| piece.result.outcome == CallProtocolOutcome::Denied)
    {
        CallProtocolOutcome::Denied
    } else {
        CallProtocolOutcome::Cancelled
    }
}

async fn execute_local_job_submission(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    peer: MctUdsPeerCredentials,
    identity: MctLocalNodeIdentity,
    submission: MctResidentJobSubmission,
) -> Option<MctControlPlaneResponse> {
    let job_id = submission.job_id.clone();
    let trace_id = submission.trace_context.trace_id.clone();
    let requests = match local_job_protocol_requests(identity, peer.uid, submission) {
        Ok(requests) => requests,
        Err(_) => {
            return append_local_refusal(
                &ledger,
                Some(peer.uid),
                400,
                "malformed call",
                "malformed_envelope",
            )
            .await;
        }
    };
    let job_observations = [
        local_job_observation(
            &job_id,
            trace_id.clone(),
            peer.uid,
            ObservationKind::CallReceived,
            ObservationOutcome::Informational,
            "authenticated local mesh job received",
            format!("local_mesh_job_piece_count:{}", requests.len()),
        ),
        local_job_observation(
            &job_id,
            trace_id.clone(),
            peer.uid,
            ObservationKind::CallConstructed,
            ObservationOutcome::Allowed,
            "local mesh job accepted for evaluation",
            format!("local_mesh_job_piece_count:{}", requests.len()),
        ),
    ]
    .into_iter()
    .collect::<Result<Vec<_>>>()
    .ok()?;
    if ledger.append(job_observations).await.is_err() {
        return None;
    }
    let mut pieces = Vec::with_capacity(requests.len());
    for piece in requests {
        let call_id = piece.request.call.call_id.clone();
        let result = execute_local_protocol_request(
            paths.clone(),
            ledger.clone(),
            peer.uid,
            piece.request,
            piece.inline_payload,
        )
        .await?;
        pieces.push(MctResidentJobPieceResponse {
            piece_id: piece.piece_id,
            call_id,
            result,
        });
    }
    let outcome = mesh_job_outcome(&pieces);
    let completed = pieces
        .iter()
        .filter(|piece| piece.result.outcome == CallProtocolOutcome::Completed)
        .count();
    let denied = pieces
        .iter()
        .filter(|piece| piece.result.outcome == CallProtocolOutcome::Denied)
        .count();
    let failed = pieces.len().saturating_sub(completed + denied);
    let terminal = local_job_observation(
        &job_id,
        trace_id,
        peer.uid,
        ObservationKind::ResultRecorded,
        match outcome {
            CallProtocolOutcome::Completed => ObservationOutcome::Completed,
            CallProtocolOutcome::Denied | CallProtocolOutcome::Malformed => {
                ObservationOutcome::Denied
            }
            CallProtocolOutcome::Failed => ObservationOutcome::Failed,
            CallProtocolOutcome::TimedOut => ObservationOutcome::TimedOut,
            CallProtocolOutcome::Cancelled => ObservationOutcome::Cancelled,
            CallProtocolOutcome::AcceptedForRouting => ObservationOutcome::Allowed,
        },
        if outcome == CallProtocolOutcome::Completed {
            "local mesh job completed"
        } else {
            "local mesh job incomplete"
        },
        format!("local_mesh_job_result:completed={completed};denied={denied};failed={failed}"),
    )
    .ok()?;
    if ledger.append(vec![terminal]).await.is_err() {
        return None;
    }
    Some(local_call_response(
        200,
        MctResidentJobResponse {
            job_id,
            outcome,
            safe_message: if outcome == CallProtocolOutcome::Completed {
                "mesh job completed".into()
            } else {
                "mesh job incomplete".into()
            },
            pieces,
        },
    ))
}

async fn execute_local_submission(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    peer: MctUdsPeerCredentials,
    body: Vec<u8>,
) -> Option<MctControlPlaneResponse> {
    let identity = match MctDaemonConfigStore::new(paths.config_path()).load() {
        Ok(config) => match config.local_identity {
            Some(identity) => identity,
            None => {
                return append_local_refusal(
                    &ledger,
                    Some(peer.uid),
                    503,
                    "resident identity unavailable",
                    "identity_unavailable",
                )
                .await;
            }
        },
        Err(_) => {
            return append_local_refusal(
                &ledger,
                Some(peer.uid),
                503,
                "resident identity unavailable",
                "identity_unavailable",
            )
            .await;
        }
    };
    let submission = match serde_json::from_slice::<MctResidentSubmission>(&body) {
        Ok(submission) => submission,
        Err(_) => {
            return append_local_refusal(
                &ledger,
                Some(peer.uid),
                400,
                "malformed call",
                "malformed_envelope",
            )
            .await;
        }
    };
    match submission {
        MctResidentSubmission::Call(submission) => {
            let (request, inline_payload) =
                match local_protocol_request(identity, peer.uid, *submission) {
                    Ok(request) => request,
                    Err(_) => {
                        return append_local_refusal(
                            &ledger,
                            Some(peer.uid),
                            400,
                            "malformed call",
                            "invalid_call",
                        )
                        .await;
                    }
                };
            execute_local_protocol_request(paths, ledger, peer.uid, request, inline_payload)
                .await
                .map(|result| local_call_response(200, result))
        }
        MctResidentSubmission::Job(submission) => {
            execute_local_job_submission(paths, ledger, peer, identity, *submission).await
        }
    }
}

pub(crate) fn resident_local_call_handler(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    expected_uid: u32,
    max_in_flight: usize,
) -> MctUdsControlCallHandler {
    let capacity = Arc::new(tokio::sync::Semaphore::new(max_in_flight));
    let preflight_ledger = ledger.clone();
    MctUdsControlCallHandler::new(
        move |peer, declared_body_len| {
            let ledger = preflight_ledger.clone();
            async move {
                preflight_local_submission(&ledger, expected_uid, peer, declared_body_len).await
            }
        },
        move |peer, body| {
            let paths = paths.clone();
            let ledger = ledger.clone();
            let capacity = Arc::clone(&capacity);
            async move {
                let Ok(_permit) = capacity.try_acquire_owned() else {
                    return append_local_refusal(
                        &ledger,
                        Some(peer.uid),
                        503,
                        "resident call capacity unavailable; retry later",
                        "capacity_unavailable",
                    )
                    .await;
                };
                execute_local_submission(paths, ledger, peer, body).await
            }
        },
    )
}

pub(crate) fn resident_local_call_endpoint_observation(expected_uid: u32) -> MctObservation {
    local_call_observation(LocalCallObservationFact {
        kind: ObservationKind::AdapterEffectCompleted,
        source_plane: SourcePlane::Adapter,
        outcome: ObservationOutcome::Completed,
        trace_id: local_transport_trace(),
        call_id: None,
        decision_id: None,
        subject_id: Some(authenticated_uid_subject(expected_uid)),
        resource_id: Some("mct-local-call-uds".into()),
        policy_revision: None,
        grants_revision: None,
        safe_message: "authenticated local call endpoint ready".into(),
        detail_ref: Some("socket_mode:0600;authentication:unix_peer_uid".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn submission_body(payload: &[u8], digest: &str, call_id: &str) -> Vec<u8> {
        submission_body_with_key(payload, digest, call_id, &format!("key-{call_id}"))
    }

    fn submission_body_with_key(
        payload: &[u8],
        digest: &str,
        call_id: &str,
        idempotency_key: &str,
    ) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "protocol_request_id": format!("proto-{call_id}"),
            "call_id": call_id,
            "target": {
                "namespace": "patina:demo",
                "interface_name": "control@0.1.0",
                "function_name": "run"
            },
            "payload_metadata": {
                "data_classification": "public",
                "size_bytes": payload.len(),
                "contains_secret_scoped_material": false
            },
            "authority_context": {
                "policy_revision": 1,
                "expected_receiver_grants_authority": {
                    "mother_node_id": "local-mct",
                    "authority_epoch": "epoch-test",
                    "generation": 1,
                    "source_authority_observation_id": "obs-authority-test-1"
                },
                "vision_policy_revision": 1
            },
            "deadline": "2099-01-01T00:00:00Z",
            "trace_context": {
                "trace_id": format!("trace-{call_id}"),
                "span_id": format!("span-{call_id}")
            },
            "payload": {
                "payload_kind": "inline_payload",
                "inline_payload_ref": format!("payload-{call_id}"),
                "content_type": "application/json",
                "size_bytes": payload.len(),
                "blake3_digest_hex": digest
            },
            "inline_payload_base64": BASE64_STANDARD.encode(payload),
            "idempotency_key": idempotency_key
        }))
        .unwrap()
    }

    fn local_paths(dir: &tempfile::TempDir) -> (ResidentRuntimePaths, PathBuf, PathBuf) {
        let config_path = dir.path().join("config.json");
        let identity_path = dir.path().join("identity.key");
        let ledger_path = dir.path().join("observations.jsonl");
        MctDaemonConfigStore::new(&config_path)
            .ensure_local_identity(MctOperatorNodeScope::default(), &identity_path)
            .unwrap();
        (
            ResidentRuntimePaths::new(
                config_path,
                dir.path().join("children"),
                dir.path().join("state.sqlite"),
            ),
            identity_path,
            ledger_path,
        )
    }

    #[tokio::test]
    async fn resident_call_uds_authenticates_peer_before_submission() {
        let dir = tempfile::tempdir().unwrap();
        let (_paths, _identity_path, ledger_path) = local_paths(&dir);
        let ledger = ResidentLedgerWriter::spawn_authority_for_test(ledger_path.clone()).unwrap();
        let response = match preflight_local_submission(
            &ledger,
            501,
            Some(MctUdsPeerCredentials {
                uid: 502,
                gid: 20,
                pid: Some(42),
            }),
            b"not even json".len(),
        )
        .await
        {
            MctUdsControlCallPreflight::Refused(Some(response)) => response,
            other => panic!("expected durable authentication denial, got {other:?}"),
        };
        assert_eq!(response.status_code, 403);
        assert!(response.body.contains("not authorized"));
        assert!(!response.body.contains("malformed"));
        ledger.close().await;

        let text = std::fs::read_to_string(ledger_path).unwrap();
        assert!(text.contains("peer_uid_mismatch"));
        assert!(text.contains("call_denied"));
        assert!(!text.contains("not even json"));
    }

    async fn headers_only_call_response(
        socket_path: &Path,
        content_length: usize,
    ) -> Option<String> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut client = tokio::net::UnixStream::connect(socket_path).await.unwrap();
        client
            .write_all(
                format!(
                    "POST /calls HTTP/1.1\r\nHost: local\r\nContent-Length: {content_length}\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut response = String::new();
        tokio::time::timeout(
            Duration::from_millis(500),
            client.read_to_string(&mut response),
        )
        .await
        .ok()?
        .unwrap();
        Some(response)
    }

    #[tokio::test]
    async fn resident_call_uds_dispatch_authenticates_and_bounds_before_body_read() {
        use std::os::unix::fs::MetadataExt as _;

        let dir = tempfile::tempdir().unwrap();
        let (paths, _identity_path, ledger_path) = local_paths(&dir);
        let ledger = ResidentLedgerWriter::spawn_authority_for_test(ledger_path.clone()).unwrap();

        let mismatched_socket = dir.path().join("mismatched.sock");
        let mismatched_listener = tokio::net::UnixListener::bind(&mismatched_socket).unwrap();
        let actual_uid = std::fs::metadata(&mismatched_socket).unwrap().uid();
        let mismatched_handler = resident_local_call_handler(
            paths.clone(),
            ledger.clone(),
            actual_uid.wrapping_add(1),
            1,
        );
        let mismatched_server = tokio::spawn(async move {
            mct_daemon::serve_uds_control_once_with_handlers(
                &mismatched_listener,
                Err(MctControlPlaneSnapshotError::runtime_state_unavailable()),
                None,
                None,
                Some(&mismatched_handler),
            )
            .await
            .unwrap();
        });
        let mismatched_response = headers_only_call_response(&mismatched_socket, 32)
            .await
            .expect("UID denial must precede body consumption");
        assert!(
            mismatched_response.starts_with("HTTP/1.1 403"),
            "{mismatched_response}"
        );
        mismatched_server.await.unwrap();

        let oversized_socket = dir.path().join("oversized.sock");
        let oversized_listener = tokio::net::UnixListener::bind(&oversized_socket).unwrap();
        let oversized_handler = resident_local_call_handler(paths, ledger.clone(), actual_uid, 1);
        let oversized_server = tokio::spawn(async move {
            mct_daemon::serve_uds_control_once_with_handlers(
                &oversized_listener,
                Err(MctControlPlaneSnapshotError::runtime_state_unavailable()),
                None,
                None,
                Some(&oversized_handler),
            )
            .await
            .unwrap();
        });
        let oversized_response = headers_only_call_response(
            &oversized_socket,
            mct_iroh::MCT_CALL_FRAME_READ_BUDGET_BYTES + 1,
        )
        .await
        .expect("declared call frame refusal must precede body consumption");
        assert!(
            oversized_response.starts_with("HTTP/1.1 413"),
            "{oversized_response}"
        );
        oversized_server.await.unwrap();
        ledger.close().await;

        let text = std::fs::read_to_string(ledger_path).unwrap();
        assert!(text.contains("peer_uid_mismatch"));
        assert!(text.contains("frame_too_large"));
    }

    #[tokio::test]
    async fn locally_submitted_body_cannot_claim_trigger_firing_context() {
        let dir = tempfile::tempdir().unwrap();
        let (paths, _identity_path, ledger_path) = local_paths(&dir);
        let ledger = ResidentLedgerWriter::spawn_authority_for_test(ledger_path.clone()).unwrap();
        let payload = b"{}";
        let digest = blake3::hash(payload).to_hex().to_string();
        let mut body: serde_json::Value = serde_json::from_slice(&submission_body(
            payload,
            &digest,
            "call-cannot-claim-trigger",
        ))
        .unwrap();
        body["origin"] = serde_json::Value::String("trigger_firing".into());
        body["trigger_authority_id"] = serde_json::Value::String("trigger-forged".into());
        let response = execute_local_submission(
            paths,
            ledger.clone(),
            MctUdsPeerCredentials {
                uid: 501,
                gid: 20,
                pid: Some(42),
            },
            serde_json::to_vec(&body).unwrap(),
        )
        .await
        .expect("malformed local submission is durably refused");
        assert_eq!(response.status_code, 400);
        assert!(response.body.contains("malformed call"));
        ledger.close().await;
        let text = std::fs::read_to_string(ledger_path).unwrap();
        assert!(text.contains("malformed_envelope"));
        assert!(!text.contains("trigger-forged"));
    }

    #[tokio::test]
    async fn resident_call_uds_rejects_bad_payload_and_keeps_ledger_byte_free() {
        let dir = tempfile::tempdir().unwrap();
        let (paths, _identity_path, ledger_path) = local_paths(&dir);
        let ledger = ResidentLedgerWriter::spawn_authority_for_test(ledger_path.clone()).unwrap();
        let payload = br#"[{"secret-shaped":"do-not-record"}]"#;
        let response = execute_local_submission(
            paths,
            ledger.clone(),
            MctUdsPeerCredentials {
                uid: 501,
                gid: 20,
                pid: Some(42),
            },
            submission_body(payload, &"0".repeat(64), "call-local-bad-payload"),
        )
        .await
        .expect("durable payload denial returns a response");
        assert_eq!(response.status_code, 200);
        assert!(response.body.contains("failed"), "{}", response.body);
        ledger.close().await;

        let text = std::fs::read_to_string(ledger_path).unwrap();
        assert!(text.contains("PayloadDigestMismatch"));
        assert!(text.contains("result_recorded"));
        assert!(!text.contains(std::str::from_utf8(payload).unwrap()));
        assert!(!text.contains(&BASE64_STANDARD.encode(payload)));
    }

    #[tokio::test]
    async fn resident_call_uds_idempotency_is_authenticated_caller_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let (paths, _identity_path, ledger_path) = local_paths(&dir);
        let ledger = ResidentLedgerWriter::spawn_authority_for_test(ledger_path.clone()).unwrap();
        let payload = b"{}";
        let digest = blake3::hash(payload).to_hex().to_string();
        let peer = MctUdsPeerCredentials {
            uid: 501,
            gid: 20,
            pid: Some(42),
        };
        let body = submission_body_with_key(
            payload,
            &digest,
            "call-local-idempotent",
            "shared-os-user-key",
        );
        let first = execute_local_submission(paths.clone(), ledger.clone(), peer, body.clone())
            .await
            .unwrap();
        assert_eq!(first.status_code, 200);
        let replay = execute_local_submission(paths.clone(), ledger.clone(), peer, body)
            .await
            .unwrap();
        assert_eq!(replay.status_code, 200);
        assert!(replay.body.contains("idempotency_replay_completed"));

        let mismatch = execute_local_submission(
            paths,
            ledger.clone(),
            peer,
            submission_body_with_key(
                payload,
                &digest,
                "call-local-other-application",
                "shared-os-user-key",
            ),
        )
        .await
        .unwrap();
        assert!(mismatch.body.contains("idempotency_key_reuse_mismatch"));
        ledger.close().await;

        let text = std::fs::read_to_string(ledger_path).unwrap();
        assert!(text.contains("idempotency_replay_completed"));
        assert!(text.contains("idempotency_key_reuse_mismatch"));
    }

    #[tokio::test]
    async fn resident_call_uds_observes_decision_before_response() {
        let dir = tempfile::tempdir().unwrap();
        let (paths, _identity_path, ledger_path) = local_paths(&dir);
        let ledger = ResidentLedgerWriter::spawn_authority_for_test(ledger_path.clone()).unwrap();
        let payload = b"{}";
        let digest = blake3::hash(payload).to_hex().to_string();
        let response = execute_local_submission(
            paths,
            ledger.clone(),
            MctUdsPeerCredentials {
                uid: 501,
                gid: 20,
                pid: Some(42),
            },
            submission_body(payload, &digest, "call-local-observed-response"),
        )
        .await
        .expect("durable route denial returns a response");
        assert_eq!(response.status_code, 200);

        let text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(text.contains("call_constructed"));
        assert!(text.contains("result_recorded"));

        let failed = ResidentLedgerWriter::failed_for_test();
        let suppressed = execute_local_submission(
            paths_for_failed_writer(&dir),
            failed,
            MctUdsPeerCredentials {
                uid: 501,
                gid: 20,
                pid: Some(43),
            },
            submission_body(payload, &digest, "call-local-unobserved-response"),
        )
        .await;
        assert!(suppressed.is_none());
        ledger.close().await;
    }

    fn private_tempdir(prefix: &str) -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir_in(std::env::var_os("HOME").unwrap())
            .unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    fn job_submission_body(job_id: &str, piece_ids: &[&str]) -> Vec<u8> {
        let payload = b"[]";
        let digest = blake3::hash(payload).to_hex().to_string();
        serde_json::to_vec(&serde_json::json!({
            "job_id": job_id,
            "authority_context": {
                "policy_revision": 1,
                "expected_receiver_grants_authority": {
                    "mother_node_id": "local-mct",
                    "authority_epoch": "epoch-test",
                    "generation": 1,
                    "source_authority_observation_id": "obs-authority-test-1"
                },
                "vision_policy_revision": 1
            },
            "deadline": "2099-01-01T00:00:00Z",
            "trace_context": {
                "trace_id": format!("trace-{job_id}"),
                "span_id": format!("span-{job_id}")
            },
            "pieces": piece_ids.iter().map(|piece_id| serde_json::json!({
                "piece_id": piece_id,
                "target": {
                    "namespace": "patina:demo",
                    "interface_name": "control@0.1.0",
                    "function_name": "run"
                },
                "payload_metadata": {
                    "data_classification": "public",
                    "size_bytes": payload.len(),
                    "contains_secret_scoped_material": false
                },
                "payload": {
                    "payload_kind": "inline_payload",
                    "inline_payload_ref": format!("payload-{job_id}-{piece_id}"),
                    "content_type": "application/json",
                    "size_bytes": payload.len(),
                    "blake3_digest_hex": digest
                },
                "inline_payload_base64": BASE64_STANDARD.encode(payload)
            })).collect::<Vec<_>>()
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn resident_mesh_job_decomposes_one_submission_into_ordinary_piece_calls() {
        let dir = private_tempdir("mct-mesh-job-");
        let (paths, _identity_path, ledger_path) = local_paths(&dir);
        let ledger = ResidentLedgerWriter::spawn_authority_for_test(ledger_path.clone()).unwrap();
        let response = execute_local_submission(
            paths,
            ledger.clone(),
            MctUdsPeerCredentials {
                uid: 501,
                gid: 20,
                pid: Some(42),
            },
            job_submission_body("job-two-pieces", &["local", "delegated"]),
        )
        .await
        .expect("durable mesh-job result returns a response");
        assert_eq!(response.status_code, 200);
        let body: serde_json::Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(body["job_id"], "job-two-pieces");
        assert_eq!(body["pieces"].as_array().unwrap().len(), 2);
        assert_eq!(body["pieces"][0]["piece_id"], "local");
        assert_eq!(body["pieces"][0]["call_id"], "job-two-pieces:local");
        assert_eq!(body["pieces"][1]["piece_id"], "delegated");
        assert_eq!(body["pieces"][1]["call_id"], "job-two-pieces:delegated");
        ledger.close().await;

        let text = std::fs::read_to_string(ledger_path).unwrap();
        assert!(text.contains("local_mesh_job_piece_count:2"));
        assert!(text.contains("job-two-pieces:local"));
        assert!(text.contains("job-two-pieces:delegated"));
    }

    #[tokio::test]
    async fn resident_mesh_job_rejects_duplicate_pieces_before_any_piece_call() {
        let dir = private_tempdir("mct-mesh-job-duplicate-");
        let (paths, _identity_path, ledger_path) = local_paths(&dir);
        let ledger = ResidentLedgerWriter::spawn_authority_for_test(ledger_path.clone()).unwrap();
        let response = execute_local_submission(
            paths,
            ledger.clone(),
            MctUdsPeerCredentials {
                uid: 501,
                gid: 20,
                pid: Some(42),
            },
            job_submission_body("job-duplicate", &["same", "same"]),
        )
        .await
        .expect("malformed mesh job is durably refused");
        assert_eq!(response.status_code, 400);
        ledger.close().await;

        let text = std::fs::read_to_string(ledger_path).unwrap();
        assert!(text.contains("malformed_envelope"));
        assert!(!text.contains("job-duplicate:same"));
    }

    fn paths_for_failed_writer(dir: &tempfile::TempDir) -> ResidentRuntimePaths {
        ResidentRuntimePaths::new(
            dir.path().join("config.json"),
            dir.path().join("children"),
            dir.path().join("state.sqlite"),
        )
    }
}

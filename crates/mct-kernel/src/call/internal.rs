use super::*;
use crate::peer::{
    BindingState, MCT_CALL_ALPN, MctHelloAdmissionEvaluation, MctPeerAuthoritySnapshot,
};

pub(super) fn evaluate_payload_integrity_internal(
    subject: PayloadIntegritySubject,
    handle: &MctCallPayloadHandle,
    observed: &MctPayloadIntegrityObservation,
    max_inline_size_bytes: u64,
) -> MctPayloadIntegrityDecision {
    let reason = payload_integrity_reason(subject, handle, observed, max_inline_size_bytes);
    let outcome = if reason == PayloadIntegrityReason::IntegrityMatched {
        PayloadIntegrityOutcome::Matched
    } else {
        PayloadIntegrityOutcome::Mismatch
    };
    MctPayloadIntegrityDecision {
        subject,
        outcome,
        reason,
        safe_message: payload_integrity_safe_message(reason).into(),
    }
}

fn payload_integrity_reason(
    subject: PayloadIntegritySubject,
    handle: &MctCallPayloadHandle,
    observed: &MctPayloadIntegrityObservation,
    max_inline_size_bytes: u64,
) -> PayloadIntegrityReason {
    if !handle_declares_observed_bytes(handle, observed) {
        return if observed.inline_bytes_present {
            if subject == PayloadIntegritySubject::ReplyResult {
                PayloadIntegrityReason::ResultPayloadIntegrityMismatch
            } else {
                PayloadIntegrityReason::PayloadUnexpectedInlineBytes
            }
        } else {
            PayloadIntegrityReason::IntegrityMatched
        };
    }

    if !observed.inline_bytes_present
        || observed.observed_size_bytes.is_none()
        || observed.observed_blake3_digest_hex.is_none()
    {
        return if subject == PayloadIntegritySubject::ReplyResult {
            PayloadIntegrityReason::ResultPayloadIntegrityMismatch
        } else if observed.content_addressed_blob_fetch_attempted {
            PayloadIntegrityReason::PayloadBlobUnavailable
        } else {
            PayloadIntegrityReason::PayloadMissingInlineBytes
        };
    }

    let declared_digest = declared_digest_hex(handle);
    let observed_digest = observed
        .observed_blake3_digest_hex
        .as_deref()
        .expect("observed digest presence checked above");
    if !is_valid_blake3_hex(declared_digest) || !is_valid_blake3_hex(observed_digest) {
        return PayloadIntegrityReason::InvalidPayloadDigest;
    }

    let declared_size = handle.declared_size_bytes();
    let observed_size = observed
        .observed_size_bytes
        .expect("observed size presence checked above");
    if declared_size > max_inline_size_bytes {
        return if subject == PayloadIntegritySubject::ReplyResult {
            PayloadIntegrityReason::ResultPayloadTooLarge
        } else {
            PayloadIntegrityReason::PayloadDeclaredTooLarge
        };
    }
    if observed_size > max_inline_size_bytes {
        return if subject == PayloadIntegritySubject::ReplyResult {
            PayloadIntegrityReason::ResultPayloadTooLarge
        } else {
            PayloadIntegrityReason::PayloadActualTooLarge
        };
    }
    if declared_size != observed_size {
        return mismatch_reason(subject, true);
    }
    if declared_digest != observed_digest {
        return mismatch_reason(subject, false);
    }

    PayloadIntegrityReason::IntegrityMatched
}

fn handle_declares_observed_bytes(
    handle: &MctCallPayloadHandle,
    observed: &MctPayloadIntegrityObservation,
) -> bool {
    matches!(handle, MctCallPayloadHandle::InlinePayload { .. })
        || (observed.content_addressed_blob_fetch_attempted
            && matches!(handle, MctCallPayloadHandle::ContentAddressedBlob { .. }))
}

fn declared_digest_hex(handle: &MctCallPayloadHandle) -> &str {
    match handle {
        MctCallPayloadHandle::InlinePayload {
            blake3_digest_hex, ..
        } => blake3_digest_hex,
        MctCallPayloadHandle::ContentAddressedBlob { digest, .. } => digest,
        MctCallPayloadHandle::ExternalReference { .. } | MctCallPayloadHandle::Empty => "",
    }
}

fn mismatch_reason(
    subject: PayloadIntegritySubject,
    size_mismatch: bool,
) -> PayloadIntegrityReason {
    match subject {
        PayloadIntegritySubject::Request if size_mismatch => {
            PayloadIntegrityReason::PayloadSizeMismatch
        }
        PayloadIntegritySubject::Request => PayloadIntegrityReason::PayloadDigestMismatch,
        PayloadIntegritySubject::ReplyResult => {
            PayloadIntegrityReason::ResultPayloadIntegrityMismatch
        }
    }
}

fn payload_integrity_safe_message(reason: PayloadIntegrityReason) -> &'static str {
    match reason {
        PayloadIntegrityReason::IntegrityMatched => "payload integrity verified",
        PayloadIntegrityReason::ResultPayloadTooLarge => "result payload too large",
        PayloadIntegrityReason::ResultPayloadIntegrityMismatch => {
            "result payload integrity mismatch"
        }
        PayloadIntegrityReason::PayloadDeclaredTooLarge
        | PayloadIntegrityReason::PayloadActualTooLarge
        | PayloadIntegrityReason::PayloadSizeMismatch
        | PayloadIntegrityReason::PayloadDigestMismatch
        | PayloadIntegrityReason::PayloadMissingInlineBytes
        | PayloadIntegrityReason::PayloadUnexpectedInlineBytes
        | PayloadIntegrityReason::InvalidPayloadDigest => "malformed call payload",
        PayloadIntegrityReason::PayloadBlobUnavailable => "payload blob unavailable",
    }
}

fn is_valid_blake3_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn evaluate_call_protocol_internal(
    request: &MctCallProtocolRequest,
    hello: &MctHelloAdmissionEvaluation,
    context: CallEvaluationContext,
) -> MctCallProtocolEvaluation {
    let CallEvaluationContext {
        ids,
        current_peer_authority,
        now,
    } = context;

    if !hello.is_admitted() || request.authority.hello_decision_id != hello.decision_id {
        return denied(
            request,
            ids,
            CallProtocolReason::HelloNotAdmitted,
            "not authorized",
        );
    }

    if hello.selected_binding_id.as_ref() != Some(&request.authority.peer_binding_id) {
        return denied(
            request,
            ids,
            CallProtocolReason::BindingMismatch,
            "not authorized",
        );
    }

    if hello.selected_node_id.as_ref() != Some(&request.call.caller.node_id) {
        return denied(
            request,
            ids,
            CallProtocolReason::CallerMismatch,
            "not authorized",
        );
    }

    if hello.selected_vision_id.as_ref() != Some(&request.authority.vision_id)
        || request.authority.vision_id != request.call.caller.vision_id
    {
        return denied(
            request,
            ids,
            CallProtocolReason::VisionMismatch,
            "not authorized",
        );
    }

    if request.authority.accepted_alpn != MCT_CALL_ALPN || !hello.admits_alpn(MCT_CALL_ALPN) {
        return denied(
            request,
            ids,
            CallProtocolReason::AlpnNotAdmitted,
            "not authorized",
        );
    }

    if request.authority.endpoint_id != request.received_over.endpoint_id {
        return denied(
            request,
            ids,
            CallProtocolReason::EndpointMismatch,
            "not authorized",
        );
    }

    if let Some(reason) =
        current_binding_denial_reason(request, hello, &current_peer_authority, &now)
    {
        return denied(request, ids, reason, "not authorized");
    }

    if request.call.authority_context.policy_revision < request.authority.policy_revision
        || request.call.authority_context.grants_revision < request.authority.grants_revision
    {
        return denied(
            request,
            ids,
            CallProtocolReason::PolicyRevisionStale,
            "not authorized",
        );
    }

    if request.payload.declared_size_bytes() != request.call.payload_metadata.size_bytes {
        return denied(
            request,
            ids,
            CallProtocolReason::PayloadMetadataMismatch,
            "malformed call",
        );
    }

    MctCallProtocolEvaluation {
        decision_id: ids.decision_id,
        protocol_request_id: request.protocol_request_id.clone(),
        call_id: Some(request.call.call_id.clone()),
        route_decision_id: None,
        result_ref: None,
        outcome: CallProtocolOutcome::AcceptedForRouting,
        reason: CallProtocolReason::ResultRecorded,
        safe_message: "accepted for routing".into(),
        observation_id: ids.observation_id,
    }
}

fn current_binding_denial_reason(
    request: &MctCallProtocolRequest,
    hello: &MctHelloAdmissionEvaluation,
    authority: &MctPeerAuthoritySnapshot,
    now: &Timestamp,
) -> Option<CallProtocolReason> {
    let Some(binding) = authority.bindings.iter().find(|binding| {
        binding.binding_id == request.authority.peer_binding_id
            && binding.iroh_endpoint_id == request.received_over.endpoint_id
    }) else {
        return Some(CallProtocolReason::BindingMismatch);
    };

    match binding.binding_state {
        BindingState::Admitted => {}
        BindingState::Revoked => return Some(CallProtocolReason::BindingRevoked),
        BindingState::Expired => return Some(CallProtocolReason::BindingExpired),
        BindingState::Pending | BindingState::Denied => {
            return Some(CallProtocolReason::BindingMismatch);
        }
    }

    if binding
        .expires_at
        .as_ref()
        .is_some_and(|expires_at| expires_at <= now)
    {
        return Some(CallProtocolReason::BindingExpired);
    }

    if hello.selected_policy_revision != Some(binding.policy_revision)
        || binding.policy_revision != request.authority.policy_revision
        || binding.policy_revision < authority.policy_revision
        || request.authority.policy_revision < authority.policy_revision
    {
        return Some(CallProtocolReason::PolicyRevisionStale);
    }

    if binding.scope.mct_node_id != request.call.caller.node_id {
        return Some(CallProtocolReason::CallerMismatch);
    }

    if binding.scope.vision_id != request.authority.vision_id
        || binding.scope.vision_id != request.call.caller.vision_id
    {
        return Some(CallProtocolReason::VisionMismatch);
    }

    if !binding
        .scope
        .allowed_alpns
        .iter()
        .any(|alpn| alpn == MCT_CALL_ALPN)
    {
        return Some(CallProtocolReason::AlpnNotAdmitted);
    }

    None
}

fn denied(
    request: &MctCallProtocolRequest,
    ids: CallEvaluationIds,
    reason: CallProtocolReason,
    safe_message: &str,
) -> MctCallProtocolEvaluation {
    MctCallProtocolEvaluation {
        decision_id: ids.decision_id,
        protocol_request_id: request.protocol_request_id.clone(),
        call_id: Some(request.call.call_id.clone()),
        route_decision_id: None,
        result_ref: None,
        outcome: match reason {
            CallProtocolReason::MalformedCall
            | CallProtocolReason::PayloadMetadataMismatch
            | CallProtocolReason::PayloadDeclaredTooLarge
            | CallProtocolReason::PayloadActualTooLarge
            | CallProtocolReason::PayloadSizeMismatch
            | CallProtocolReason::PayloadDigestMismatch
            | CallProtocolReason::PayloadMissingInlineBytes
            | CallProtocolReason::PayloadUnexpectedInlineBytes
            | CallProtocolReason::InvalidPayloadDigest => CallProtocolOutcome::Malformed,
            CallProtocolReason::ChildPayloadContentTypeUnsupported
            | CallProtocolReason::ResultPayloadTooLarge
            | CallProtocolReason::ResultPayloadIntegrityMismatch
            | CallProtocolReason::ExecutionFailed => CallProtocolOutcome::Failed,
            CallProtocolReason::ExecutionTimedOut => CallProtocolOutcome::TimedOut,
            _ => CallProtocolOutcome::Denied,
        },
        reason,
        safe_message: safe_message.into(),
        observation_id: ids.observation_id,
    }
}

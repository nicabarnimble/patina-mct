use super::*;
use crate::peer::{MctHelloAdmissionEvaluation, MCT_CALL_ALPN};

pub(super) fn evaluate_call_protocol_internal(
    request: &MctCallProtocolRequest,
    hello: &MctHelloAdmissionEvaluation,
    ids: CallEvaluationIds,
) -> MctCallProtocolEvaluation {
    if !hello.is_admitted() || request.authority.hello_decision_id != hello.decision_id {
        return denied(
            request,
            ids,
            CallProtocolReason::HelloNotAdmitted,
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

    if request.payload.approximate_size_bytes != request.call.payload_metadata.approximate_size_bytes {
        return denied(
            request,
            ids,
            CallProtocolReason::PayloadMetadataMismatch,
            "malformed call",
        );
    }

    if matches!(request.payload.payload_kind, PayloadKind::Empty)
        && request.call.payload_metadata.approximate_size_bytes != 0
    {
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
            CallProtocolReason::MalformedCall | CallProtocolReason::PayloadMetadataMismatch => {
                CallProtocolOutcome::Malformed
            }
            _ => CallProtocolOutcome::Denied,
        },
        reason,
        safe_message: safe_message.into(),
        observation_id: ids.observation_id,
    }
}

use super::*;
use std::collections::BTreeSet;

pub(super) fn evaluate_hello_internal(
    request: &MctHelloRequest,
    bindings: &[MctPeerBinding],
    policy: &HelloPolicy,
    context: HelloEvaluationContext,
) -> MctHelloAdmissionEvaluation {
    let HelloEvaluationContext { ids, now } = context;

    if request.requested_protocol.protocol_name != policy.protocol.protocol_name
        || request.requested_protocol.major != policy.protocol.major
    {
        return denied(
            request,
            ids,
            HelloReason::VersionUnsupported,
            SafeHelloReason::UnsupportedVersion,
        );
    }

    if request.presented_binding.endpoint_id != request.received_over.endpoint_id {
        return denied(
            request,
            ids,
            HelloReason::EndpointMismatch,
            SafeHelloReason::NotAuthorized,
        );
    }

    let Some(binding) = select_binding(request, bindings) else {
        return denied(
            request,
            ids,
            HelloReason::MissingBinding,
            SafeHelloReason::NotAuthorized,
        );
    };

    match binding.binding_state {
        BindingState::Admitted => {}
        BindingState::Pending => {
            return denied(
                request,
                ids,
                HelloReason::BindingPending,
                SafeHelloReason::RetryLater,
            );
        }
        BindingState::Expired => {
            return denied(
                request,
                ids,
                HelloReason::BindingExpired,
                SafeHelloReason::NotAuthorized,
            );
        }
        BindingState::Revoked => {
            return denied(
                request,
                ids,
                HelloReason::BindingRevoked,
                SafeHelloReason::NotAuthorized,
            );
        }
        BindingState::Denied => {
            return denied(
                request,
                ids,
                HelloReason::MissingBinding,
                SafeHelloReason::NotAuthorized,
            );
        }
    }

    if request
        .presented_binding
        .mct_node_id
        .as_ref()
        .is_some_and(|node_id| node_id != &binding.scope.mct_node_id)
    {
        return denied(
            request,
            ids,
            HelloReason::CapabilityInvalid,
            SafeHelloReason::NotAuthorized,
        );
    }

    if request
        .presented_binding
        .vision_id
        .as_ref()
        .is_some_and(|vision_id| vision_id != &binding.scope.vision_id)
    {
        return denied(
            request,
            ids,
            HelloReason::VisionNotAllowed,
            SafeHelloReason::NotAuthorized,
        );
    }

    if binding.expires_at <= now {
        return denied(
            request,
            ids,
            HelloReason::BindingExpired,
            SafeHelloReason::NotAuthorized,
        );
    }

    if binding.policy_revision < policy.current_policy_revision
        || request
            .presented_binding
            .policy_revision
            .is_some_and(|revision| revision < policy.current_policy_revision)
    {
        return denied(
            request,
            ids,
            HelloReason::PolicyRevisionStale,
            SafeHelloReason::NotAuthorized,
        );
    }

    if request
        .requested_vision_id
        .as_ref()
        .is_some_and(|vision| vision != &binding.scope.vision_id)
    {
        return denied(
            request,
            ids,
            HelloReason::VisionNotAllowed,
            SafeHelloReason::NotAuthorized,
        );
    }

    let accepted_alpns = accepted_alpns(request, binding, policy);
    if accepted_alpns.is_empty() {
        return denied(
            request,
            ids,
            HelloReason::AlpnNotAllowed,
            SafeHelloReason::NotAuthorized,
        );
    }

    MctHelloAdmissionEvaluation {
        decision_id: ids.decision_id,
        request_id: request.hello_id.clone(),
        peer_admission_decision_id: None,
        selected_binding_id: Some(binding.binding_id.clone()),
        selected_node_id: Some(binding.scope.mct_node_id.clone()),
        selected_vision_id: Some(binding.scope.vision_id.clone()),
        selected_policy_revision: Some(binding.policy_revision),
        negotiated_protocol: Some(policy.protocol.clone()),
        accepted_alpns,
        hello_outcome: HelloOutcome::Admitted,
        reason: HelloReason::ActiveBinding,
        safe_reason: SafeHelloReason::Admitted,
        observation_id: ids.observation_id,
    }
}

fn select_binding<'a>(
    request: &MctHelloRequest,
    bindings: &'a [MctPeerBinding],
) -> Option<&'a MctPeerBinding> {
    if let Some(binding_id) = request.presented_binding.binding_id.as_ref() {
        return bindings.iter().find(|binding| {
            &binding.binding_id == binding_id
                && binding.iroh_endpoint_id == request.received_over.endpoint_id
        });
    }

    bindings
        .iter()
        .find(|binding| binding.iroh_endpoint_id == request.received_over.endpoint_id)
}

fn accepted_alpns(
    request: &MctHelloRequest,
    binding: &MctPeerBinding,
    policy: &HelloPolicy,
) -> Vec<String> {
    let policy_alpns: BTreeSet<&str> = policy.supported_alpns.iter().map(String::as_str).collect();
    let binding_alpns: BTreeSet<&str> = binding
        .scope
        .allowed_alpns
        .iter()
        .map(String::as_str)
        .collect();

    request
        .requested_alpns
        .iter()
        .filter(|alpn| policy_alpns.contains(alpn.as_str()))
        .filter(|alpn| binding_alpns.contains(alpn.as_str()))
        .cloned()
        .collect()
}

fn denied(
    request: &MctHelloRequest,
    ids: EvaluationIds,
    reason: HelloReason,
    safe_reason: SafeHelloReason,
) -> MctHelloAdmissionEvaluation {
    MctHelloAdmissionEvaluation {
        decision_id: ids.decision_id,
        request_id: request.hello_id.clone(),
        peer_admission_decision_id: None,
        selected_binding_id: None,
        selected_node_id: None,
        selected_vision_id: None,
        selected_policy_revision: None,
        negotiated_protocol: None,
        accepted_alpns: Vec::new(),
        hello_outcome: match safe_reason {
            SafeHelloReason::UnsupportedVersion => HelloOutcome::UpgradeRequired,
            SafeHelloReason::RetryLater => HelloOutcome::RetryLater,
            SafeHelloReason::NotAuthorized | SafeHelloReason::Admitted => HelloOutcome::Denied,
        },
        reason,
        safe_reason,
        observation_id: ids.observation_id,
    }
}

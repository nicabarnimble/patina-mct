use crate::id::*;
use serde::{Deserialize, Serialize};

mod internal;

pub const MCT_HELLO_ALPN: &str = "mct/hello/0";
pub const MCT_CALL_ALPN: &str = "mct/call/0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionSide {
    Incoming,
    Outgoing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathClass {
    Direct,
    Relayed,
    RelayOnly,
    PrivacyTransport,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrohConnectionPresentation {
    pub endpoint_id: EndpointIdText,
    pub alpn: String,
    pub connection_side: ConnectionSide,
    pub path_class: PathClass,
    pub relay_url: Option<String>,
    pub presented_capability_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPeerBindingScope {
    pub mct_node_id: MctNodeId,
    pub vision_id: VisionId,
    pub allowed_alpns: Vec<String>,
    pub data_scope: Option<String>,
    pub observation_scope: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingState {
    Pending,
    Admitted,
    Denied,
    Expired,
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPeerBinding {
    pub binding_id: PeerBindingId,
    pub iroh_endpoint_id: EndpointIdText,
    pub scope: MctPeerBindingScope,
    pub issuer_node_id: MctNodeId,
    pub policy_revision: u64,
    pub binding_state: BindingState,
    pub issued_at: Timestamp,
    pub expires_at: Option<Timestamp>,
    pub created_by_observation_id: ObservationId,
    pub superseded_by_observation_id: Option<ObservationId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctProtocolVersion {
    pub protocol_name: String,
    pub major: u32,
    pub minor: u32,
    pub compatibility_floor: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPeerBindingPresentation {
    pub binding_id: Option<PeerBindingId>,
    pub endpoint_id: EndpointIdText,
    pub mct_node_id: Option<MctNodeId>,
    pub vision_id: Option<VisionId>,
    pub policy_revision: Option<u64>,
    pub allowed_alpns_claim: Vec<String>,
    pub signature_ref: Option<String>,
    pub expires_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctHelloCapabilityView {
    pub supported_alpns: Vec<String>,
    pub supported_wit_worlds: Vec<String>,
    pub supported_observation_modes: Vec<String>,
    pub capability_view_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctHelloRequest {
    pub hello_id: String,
    pub received_over: IrohConnectionPresentation,
    pub requested_protocol: MctProtocolVersion,
    pub requested_vision_id: Option<VisionId>,
    pub requested_alpns: Vec<String>,
    pub presented_binding: MctPeerBindingPresentation,
    pub capability_view: Option<MctHelloCapabilityView>,
    pub local_policy_revision_seen: Option<u64>,
    pub trace_id: TraceId,
    pub received_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelloOutcome {
    Admitted,
    Denied,
    RetryLater,
    UpgradeRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelloReason {
    ActiveBinding,
    EndpointMismatch,
    MissingBinding,
    BindingPending,
    BindingRevoked,
    BindingExpired,
    VisionNotAllowed,
    AlpnNotAllowed,
    VersionUnsupported,
    PolicyRevisionStale,
    CapabilityInvalid,
    RelayAccessDenied,
    TemporaryUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeHelloReason {
    NotAuthorized,
    UnsupportedVersion,
    RetryLater,
    Admitted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctHelloAdmissionEvaluation {
    pub decision_id: DecisionId,
    pub request_id: String,
    pub peer_admission_decision_id: Option<DecisionId>,
    pub selected_binding_id: Option<PeerBindingId>,
    pub negotiated_protocol: Option<MctProtocolVersion>,
    pub accepted_alpns: Vec<String>,
    pub hello_outcome: HelloOutcome,
    pub reason: HelloReason,
    pub safe_reason: SafeHelloReason,
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctHelloResponse {
    pub response_id: String,
    pub request_id: String,
    pub decision_id: DecisionId,
    pub hello_outcome: HelloOutcome,
    pub negotiated_protocol: Option<MctProtocolVersion>,
    pub accepted_alpns: Vec<String>,
    pub safe_message: String,
    pub retry_after: Option<Timestamp>,
    pub response_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerAdmissionOutcome {
    Admitted,
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerAdmissionReason {
    ActiveBinding,
    UnknownEndpoint,
    MissingBinding,
    BindingPending,
    BindingRevoked,
    BindingExpired,
    VisionNotAllowed,
    AlpnNotAllowed,
    PolicyRevisionStale,
    CapabilityInvalid,
    RelayAccessDenied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPeerAdmissionDecision {
    pub decision_id: DecisionId,
    pub presentation: IrohConnectionPresentation,
    pub binding_id: Option<PeerBindingId>,
    pub requested_vision_id: Option<VisionId>,
    pub policy_revision: u64,
    pub outcome: PeerAdmissionOutcome,
    pub reason: PeerAdmissionReason,
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelloPolicy {
    pub protocol: MctProtocolVersion,
    pub current_policy_revision: u64,
    pub supported_alpns: Vec<String>,
}

impl Default for HelloPolicy {
    fn default() -> Self {
        Self {
            protocol: MctProtocolVersion {
                protocol_name: MCT_HELLO_ALPN.into(),
                major: 0,
                minor: 1,
                compatibility_floor: Some(0),
            },
            current_policy_revision: 1,
            supported_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationIds {
    pub decision_id: DecisionId,
    pub observation_id: ObservationId,
}

pub fn evaluate_hello(
    request: &MctHelloRequest,
    bindings: &[MctPeerBinding],
    policy: &HelloPolicy,
    ids: EvaluationIds,
) -> MctHelloAdmissionEvaluation {
    internal::evaluate_hello_internal(request, bindings, policy, ids)
}

impl MctHelloAdmissionEvaluation {
    pub fn is_admitted(&self) -> bool {
        self.hello_outcome == HelloOutcome::Admitted
    }

    pub fn admits_alpn(&self, alpn: &str) -> bool {
        self.accepted_alpns.iter().any(|accepted| accepted == alpn)
    }
}

pub fn hello_response(
    response_id: impl Into<String>,
    evaluation: &MctHelloAdmissionEvaluation,
    response_observation_id: ObservationId,
) -> MctHelloResponse {
    let safe_message = match evaluation.safe_reason {
        SafeHelloReason::Admitted => "admitted",
        SafeHelloReason::NotAuthorized => "not authorized",
        SafeHelloReason::UnsupportedVersion => "unsupported version",
        SafeHelloReason::RetryLater => "retry later",
    };

    MctHelloResponse {
        response_id: response_id.into(),
        request_id: evaluation.request_id.clone(),
        decision_id: evaluation.decision_id.clone(),
        hello_outcome: evaluation.hello_outcome,
        negotiated_protocol: evaluation.negotiated_protocol.clone(),
        accepted_alpns: evaluation.accepted_alpns.clone(),
        safe_message: safe_message.into(),
        retry_after: None,
        response_observation_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn presentation(endpoint: &str) -> IrohConnectionPresentation {
        IrohConnectionPresentation {
            endpoint_id: EndpointIdText::from(endpoint),
            alpn: MCT_HELLO_ALPN.into(),
            connection_side: ConnectionSide::Incoming,
            path_class: PathClass::Direct,
            relay_url: None,
            presented_capability_ref: None,
        }
    }

    fn request(endpoint: &str) -> MctHelloRequest {
        MctHelloRequest {
            hello_id: "hello-1".into(),
            received_over: presentation(endpoint),
            requested_protocol: HelloPolicy::default().protocol,
            requested_vision_id: Some(VisionId::from("vision-a")),
            requested_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            presented_binding: MctPeerBindingPresentation {
                binding_id: Some(PeerBindingId::from("binding-1")),
                endpoint_id: EndpointIdText::from(endpoint),
                mct_node_id: Some(MctNodeId::from("node-b")),
                vision_id: Some(VisionId::from("vision-a")),
                policy_revision: Some(1),
                allowed_alpns_claim: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
                signature_ref: None,
                expires_at: None,
            },
            capability_view: None,
            local_policy_revision_seen: Some(1),
            trace_id: TraceId::from("trace-1"),
            received_observation_id: ObservationId::from("obs-received"),
        }
    }

    fn binding(state: BindingState) -> MctPeerBinding {
        MctPeerBinding {
            binding_id: PeerBindingId::from("binding-1"),
            iroh_endpoint_id: EndpointIdText::from("endpoint-a"),
            scope: MctPeerBindingScope {
                mct_node_id: MctNodeId::from("node-b"),
                vision_id: VisionId::from("vision-a"),
                allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
                data_scope: None,
                observation_scope: None,
            },
            issuer_node_id: MctNodeId::from("node-a"),
            policy_revision: 1,
            binding_state: state,
            issued_at: Timestamp::from("2026-05-31T00:00:00Z"),
            expires_at: None,
            created_by_observation_id: ObservationId::from("obs-binding"),
            superseded_by_observation_id: None,
        }
    }

    fn ids() -> EvaluationIds {
        EvaluationIds {
            decision_id: DecisionId::from("decision-1"),
            observation_id: ObservationId::from("obs-decision"),
        }
    }

    #[test]
    fn endpoint_id_is_serialized_as_transport_text() {
        let json = serde_json::to_string(&presentation("endpoint-a")).unwrap();
        assert!(json.contains("endpoint-a"));
        assert!(json.contains("mct/hello/0"));
    }

    #[test]
    fn unknown_endpoint_is_denied() {
        let evaluation = evaluate_hello(&request("endpoint-a"), &[], &HelloPolicy::default(), ids());
        assert_eq!(evaluation.hello_outcome, HelloOutcome::Denied);
        assert_eq!(evaluation.reason, HelloReason::MissingBinding);
        assert_eq!(evaluation.safe_reason, SafeHelloReason::NotAuthorized);
        assert!(!evaluation.is_admitted());
    }

    #[test]
    fn endpoint_mismatch_is_denied_before_binding_lookup() {
        let mut request = request("endpoint-a");
        request.presented_binding.endpoint_id = EndpointIdText::from("endpoint-b");
        let evaluation = evaluate_hello(&request, &[binding(BindingState::Admitted)], &HelloPolicy::default(), ids());
        assert_eq!(evaluation.reason, HelloReason::EndpointMismatch);
        assert_eq!(evaluation.safe_reason, SafeHelloReason::NotAuthorized);
    }

    #[test]
    fn active_binding_admits_intersection_of_requested_policy_and_binding_alpns() {
        let mut binding = binding(BindingState::Admitted);
        binding.scope.allowed_alpns = vec![MCT_CALL_ALPN.into()];
        let evaluation = evaluate_hello(&request("endpoint-a"), &[binding], &HelloPolicy::default(), ids());
        assert!(evaluation.is_admitted());
        assert_eq!(evaluation.accepted_alpns, vec![MCT_CALL_ALPN.to_string()]);
        assert!(evaluation.admits_alpn(MCT_CALL_ALPN));
        assert!(!evaluation.admits_alpn(MCT_HELLO_ALPN));
    }

    #[test]
    fn revoked_binding_is_denied_with_safe_message() {
        let evaluation = evaluate_hello(&request("endpoint-a"), &[binding(BindingState::Revoked)], &HelloPolicy::default(), ids());
        assert_eq!(evaluation.reason, HelloReason::BindingRevoked);
        let response = hello_response("response-1", &evaluation, ObservationId::from("obs-response"));
        assert_eq!(response.safe_message, "not authorized");
    }

    #[test]
    fn stale_policy_revision_is_denied() {
        let mut policy = HelloPolicy::default();
        policy.current_policy_revision = 2;
        let evaluation = evaluate_hello(&request("endpoint-a"), &[binding(BindingState::Admitted)], &policy, ids());
        assert_eq!(evaluation.reason, HelloReason::PolicyRevisionStale);
        assert_eq!(evaluation.safe_reason, SafeHelloReason::NotAuthorized);
    }

    #[test]
    fn unsupported_major_version_requests_upgrade() {
        let mut request = request("endpoint-a");
        request.requested_protocol.major = 99;
        let evaluation = evaluate_hello(&request, &[binding(BindingState::Admitted)], &HelloPolicy::default(), ids());
        assert_eq!(evaluation.hello_outcome, HelloOutcome::UpgradeRequired);
        assert_eq!(evaluation.safe_reason, SafeHelloReason::UnsupportedVersion);
    }
}

use crate::id::*;
use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_id_is_serialized_as_transport_text() {
        let presentation = IrohConnectionPresentation {
            endpoint_id: EndpointIdText::from("endpoint-a"),
            alpn: MCT_HELLO_ALPN.into(),
            connection_side: ConnectionSide::Incoming,
            path_class: PathClass::Direct,
            relay_url: None,
            presented_capability_ref: None,
        };
        let json = serde_json::to_string(&presentation).unwrap();
        assert!(json.contains("endpoint-a"));
        assert!(json.contains("mct/hello/0"));
    }
}

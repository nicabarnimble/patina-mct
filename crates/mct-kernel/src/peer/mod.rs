use crate::{
    error::{MctKernelResult, ensure_non_blank},
    id::*,
};
use serde::{Deserialize, Serialize};

mod internal;

/// Public constant `MCT_HELLO_ALPN` used by MCT protocol records.
pub const MCT_HELLO_ALPN: &str = "mct/hello/0";
/// Public constant `MCT_CALL_ALPN` used by MCT protocol records.
pub const MCT_CALL_ALPN: &str = "mct/call/0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `ConnectionSide` used by the MCT kernel.
pub enum ConnectionSide {
    /// Public `Incoming` item.
    Incoming,
    /// Public `Outgoing` item.
    Outgoing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `PathClass` used by the MCT kernel.
pub enum PathClass {
    /// Public `Direct` item.
    Direct,
    /// Public `Relayed` item.
    Relayed,
    /// Public `RelayOnly` item.
    RelayOnly,
    /// Public `PrivacyTransport` item.
    PrivacyTransport,
    /// Public `Unknown` item.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `IrohConnectionPresentation` used by the MCT kernel.
pub struct IrohConnectionPresentation {
    /// Field `endpoint_id` of this domain record.
    pub endpoint_id: EndpointIdText,
    /// Field `alpn` of this domain record.
    pub alpn: String,
    /// Field `connection_side` of this domain record.
    pub connection_side: ConnectionSide,
    /// Field `path_class` of this domain record.
    pub path_class: PathClass,
    /// Field `relay_url` of this domain record.
    pub relay_url: Option<String>,
    /// Field `presented_capability_ref` of this domain record.
    pub presented_capability_ref: Option<String>,
}

impl IrohConnectionPresentation {
    /// Validates this domain record and returns typed kernel errors.
    ///
    /// # Errors
    ///
    /// Returns a typed error when required domain fields are invalid.
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank("IrohConnectionPresentation", "alpn", &self.alpn)?;
        if let Some(relay_url) = &self.relay_url {
            ensure_non_blank("IrohConnectionPresentation", "relay_url", relay_url)?;
        }
        if let Some(capability_ref) = &self.presented_capability_ref {
            ensure_non_blank(
                "IrohConnectionPresentation",
                "presented_capability_ref",
                capability_ref,
            )?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctPeerBindingScope` used by the MCT kernel.
pub struct MctPeerBindingScope {
    /// Field `mct_node_id` of this domain record.
    pub mct_node_id: MctNodeId,
    /// Field `vision_id` of this domain record.
    pub vision_id: VisionId,
    /// Field `allowed_alpns` of this domain record.
    pub allowed_alpns: Vec<String>,
    /// Field `data_scope` of this domain record.
    pub data_scope: Option<String>,
    /// Field `observation_scope` of this domain record.
    pub observation_scope: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `BindingState` used by the MCT kernel.
pub enum BindingState {
    /// Public `Pending` item.
    Pending,
    /// Public `Admitted` item.
    Admitted,
    /// Public `Denied` item.
    Denied,
    /// Public `Expired` item.
    Expired,
    /// Public `Revoked` item.
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctPeerBinding` used by the MCT kernel.
pub struct MctPeerBinding {
    /// Field `binding_id` of this domain record.
    pub binding_id: PeerBindingId,
    /// Field `iroh_endpoint_id` of this domain record.
    pub iroh_endpoint_id: EndpointIdText,
    /// Field `scope` of this domain record.
    pub scope: MctPeerBindingScope,
    /// Field `issuer_node_id` of this domain record.
    pub issuer_node_id: MctNodeId,
    /// Field `policy_revision` of this domain record.
    pub policy_revision: u64,
    /// Field `binding_state` of this domain record.
    pub binding_state: BindingState,
    /// Field `issued_at` of this domain record.
    pub issued_at: Timestamp,
    /// Field `expires_at` of this domain record.
    pub expires_at: Option<Timestamp>,
    /// Field `created_by_observation_id` of this domain record.
    pub created_by_observation_id: ObservationId,
    /// Field `superseded_by_observation_id` of this domain record.
    pub superseded_by_observation_id: Option<ObservationId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctProtocolVersion` used by the MCT kernel.
pub struct MctProtocolVersion {
    /// Field `protocol_name` of this domain record.
    pub protocol_name: String,
    /// Field `major` of this domain record.
    pub major: u32,
    /// Field `minor` of this domain record.
    pub minor: u32,
    /// Field `compatibility_floor` of this domain record.
    pub compatibility_floor: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctPeerBindingPresentation` used by the MCT kernel.
pub struct MctPeerBindingPresentation {
    /// Field `binding_id` of this domain record.
    pub binding_id: Option<PeerBindingId>,
    /// Field `endpoint_id` of this domain record.
    pub endpoint_id: EndpointIdText,
    /// Field `mct_node_id` of this domain record.
    pub mct_node_id: Option<MctNodeId>,
    /// Field `vision_id` of this domain record.
    pub vision_id: Option<VisionId>,
    /// Field `policy_revision` of this domain record.
    pub policy_revision: Option<u64>,
    /// Field `allowed_alpns_claim` of this domain record.
    pub allowed_alpns_claim: Vec<String>,
    /// Field `signature_ref` of this domain record.
    pub signature_ref: Option<String>,
    /// Field `expires_at` of this domain record.
    pub expires_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctHelloCapabilityView` used by the MCT kernel.
pub struct MctHelloCapabilityView {
    /// Field `supported_alpns` of this domain record.
    pub supported_alpns: Vec<String>,
    /// Field `supported_wit_worlds` of this domain record.
    pub supported_wit_worlds: Vec<String>,
    /// Field `supported_observation_modes` of this domain record.
    pub supported_observation_modes: Vec<String>,
    /// Field `capability_view_ref` of this domain record.
    pub capability_view_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctHelloRequest` used by the MCT kernel.
pub struct MctHelloRequest {
    /// Field `hello_id` of this domain record.
    pub hello_id: String,
    /// Field `received_over` of this domain record.
    pub received_over: IrohConnectionPresentation,
    /// Field `requested_protocol` of this domain record.
    pub requested_protocol: MctProtocolVersion,
    /// Field `requested_vision_id` of this domain record.
    pub requested_vision_id: Option<VisionId>,
    /// Field `requested_alpns` of this domain record.
    pub requested_alpns: Vec<String>,
    /// Field `presented_binding` of this domain record.
    pub presented_binding: MctPeerBindingPresentation,
    /// Field `capability_view` of this domain record.
    pub capability_view: Option<MctHelloCapabilityView>,
    /// Field `local_policy_revision_seen` of this domain record.
    pub local_policy_revision_seen: Option<u64>,
    /// Field `trace_id` of this domain record.
    pub trace_id: TraceId,
    /// Field `received_observation_id` of this domain record.
    pub received_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `HelloOutcome` used by the MCT kernel.
pub enum HelloOutcome {
    /// Public `Admitted` item.
    Admitted,
    /// Public `Denied` item.
    Denied,
    /// Public `RetryLater` item.
    RetryLater,
    /// Public `UpgradeRequired` item.
    UpgradeRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `HelloReason` used by the MCT kernel.
pub enum HelloReason {
    /// Public `ActiveBinding` item.
    ActiveBinding,
    /// Public `EndpointMismatch` item.
    EndpointMismatch,
    /// Public `MissingBinding` item.
    MissingBinding,
    /// Public `BindingPending` item.
    BindingPending,
    /// Public `BindingRevoked` item.
    BindingRevoked,
    /// Public `BindingExpired` item.
    BindingExpired,
    /// Public `VisionNotAllowed` item.
    VisionNotAllowed,
    /// Public `AlpnNotAllowed` item.
    AlpnNotAllowed,
    /// Public `VersionUnsupported` item.
    VersionUnsupported,
    /// Public `PolicyRevisionStale` item.
    PolicyRevisionStale,
    /// Public `CapabilityInvalid` item.
    CapabilityInvalid,
    /// Public `RelayAccessDenied` item.
    RelayAccessDenied,
    /// Public `TemporaryUnavailable` item.
    TemporaryUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `SafeHelloReason` used by the MCT kernel.
pub enum SafeHelloReason {
    /// Public `NotAuthorized` item.
    NotAuthorized,
    /// Public `UnsupportedVersion` item.
    UnsupportedVersion,
    /// Public `RetryLater` item.
    RetryLater,
    /// Public `Admitted` item.
    Admitted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctHelloAdmissionEvaluation` used by the MCT kernel.
pub struct MctHelloAdmissionEvaluation {
    /// Field `decision_id` of this domain record.
    pub decision_id: DecisionId,
    /// Field `request_id` of this domain record.
    pub request_id: String,
    /// Field `peer_admission_decision_id` of this domain record.
    pub peer_admission_decision_id: Option<DecisionId>,
    /// Field `selected_binding_id` of this domain record.
    pub selected_binding_id: Option<PeerBindingId>,
    /// Field `selected_node_id` of this domain record.
    pub selected_node_id: Option<MctNodeId>,
    /// Field `selected_vision_id` of this domain record.
    pub selected_vision_id: Option<VisionId>,
    /// Field `negotiated_protocol` of this domain record.
    pub negotiated_protocol: Option<MctProtocolVersion>,
    /// Field `accepted_alpns` of this domain record.
    pub accepted_alpns: Vec<String>,
    /// Field `hello_outcome` of this domain record.
    pub hello_outcome: HelloOutcome,
    /// Field `reason` of this domain record.
    pub reason: HelloReason,
    /// Field `safe_reason` of this domain record.
    pub safe_reason: SafeHelloReason,
    /// Field `observation_id` of this domain record.
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctHelloResponse` used by the MCT kernel.
pub struct MctHelloResponse {
    /// Field `response_id` of this domain record.
    pub response_id: String,
    /// Field `request_id` of this domain record.
    pub request_id: String,
    /// Field `decision_id` of this domain record.
    pub decision_id: DecisionId,
    /// Field `hello_outcome` of this domain record.
    pub hello_outcome: HelloOutcome,
    /// Field `negotiated_protocol` of this domain record.
    pub negotiated_protocol: Option<MctProtocolVersion>,
    /// Field `accepted_alpns` of this domain record.
    pub accepted_alpns: Vec<String>,
    /// Field `safe_message` of this domain record.
    pub safe_message: String,
    /// Field `retry_after` of this domain record.
    pub retry_after: Option<Timestamp>,
    /// Field `response_observation_id` of this domain record.
    pub response_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `PeerAdmissionOutcome` used by the MCT kernel.
pub enum PeerAdmissionOutcome {
    /// Public `Admitted` item.
    Admitted,
    /// Public `Denied` item.
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Closed domain enum `PeerAdmissionReason` used by the MCT kernel.
pub enum PeerAdmissionReason {
    /// Public `ActiveBinding` item.
    ActiveBinding,
    /// Public `UnknownEndpoint` item.
    UnknownEndpoint,
    /// Public `MissingBinding` item.
    MissingBinding,
    /// Public `BindingPending` item.
    BindingPending,
    /// Public `BindingRevoked` item.
    BindingRevoked,
    /// Public `BindingExpired` item.
    BindingExpired,
    /// Public `VisionNotAllowed` item.
    VisionNotAllowed,
    /// Public `AlpnNotAllowed` item.
    AlpnNotAllowed,
    /// Public `PolicyRevisionStale` item.
    PolicyRevisionStale,
    /// Public `CapabilityInvalid` item.
    CapabilityInvalid,
    /// Public `RelayAccessDenied` item.
    RelayAccessDenied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Domain record `MctPeerAdmissionDecision` used by the MCT kernel.
pub struct MctPeerAdmissionDecision {
    /// Field `decision_id` of this domain record.
    pub decision_id: DecisionId,
    /// Field `presentation` of this domain record.
    pub presentation: IrohConnectionPresentation,
    /// Field `binding_id` of this domain record.
    pub binding_id: Option<PeerBindingId>,
    /// Field `requested_vision_id` of this domain record.
    pub requested_vision_id: Option<VisionId>,
    /// Field `policy_revision` of this domain record.
    pub policy_revision: u64,
    /// Field `outcome` of this domain record.
    pub outcome: PeerAdmissionOutcome,
    /// Field `reason` of this domain record.
    pub reason: PeerAdmissionReason,
    /// Field `observation_id` of this domain record.
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Domain record `HelloPolicy` used by the MCT kernel.
pub struct HelloPolicy {
    /// Field `protocol` of this domain record.
    pub protocol: MctProtocolVersion,
    /// Field `current_policy_revision` of this domain record.
    pub current_policy_revision: u64,
    /// Field `supported_alpns` of this domain record.
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
/// Domain record `EvaluationIds` used by the MCT kernel.
pub struct EvaluationIds {
    /// Field `decision_id` of this domain record.
    pub decision_id: DecisionId,
    /// Field `observation_id` of this domain record.
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Domain record `HelloEvaluationContext` used by the MCT kernel.
pub struct HelloEvaluationContext {
    /// Field `ids` of this domain record.
    pub ids: EvaluationIds,
    /// Field `now` of this domain record.
    pub now: Timestamp,
}

/// Evaluates `evaluate_hello` fail-closed from explicit authority inputs.
pub fn evaluate_hello(
    request: &MctHelloRequest,
    bindings: &[MctPeerBinding],
    policy: &HelloPolicy,
    context: HelloEvaluationContext,
) -> MctHelloAdmissionEvaluation {
    internal::evaluate_hello_internal(request, bindings, policy, context)
}

impl MctHelloAdmissionEvaluation {
    /// Executes `is_admitted` for this domain type.
    pub fn is_admitted(&self) -> bool {
        self.hello_outcome == HelloOutcome::Admitted
    }

    /// Executes `admits_alpn` for this domain type.
    pub fn admits_alpn(&self, alpn: &str) -> bool {
        self.accepted_alpns.iter().any(|accepted| accepted == alpn)
    }
}

/// Executes `hello_response` for this domain type.
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
            endpoint_id: EndpointIdText::new(endpoint)
                .expect("string ID literal/generated value must be non-empty"),
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
            requested_vision_id: Some(
                VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            requested_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            presented_binding: MctPeerBindingPresentation {
                binding_id: Some(
                    PeerBindingId::new("binding-1")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                endpoint_id: EndpointIdText::new(endpoint)
                    .expect("string ID literal/generated value must be non-empty"),
                mct_node_id: Some(
                    MctNodeId::new("node-b")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                vision_id: Some(
                    VisionId::new("vision-a")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                policy_revision: Some(1),
                allowed_alpns_claim: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
                signature_ref: None,
                expires_at: None,
            },
            capability_view: None,
            local_policy_revision_seen: Some(1),
            trace_id: TraceId::new("trace-1")
                .expect("string ID literal/generated value must be non-empty"),
            received_observation_id: ObservationId::new("obs-received")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn binding(state: BindingState) -> MctPeerBinding {
        MctPeerBinding {
            binding_id: PeerBindingId::new("binding-1")
                .expect("string ID literal/generated value must be non-empty"),
            iroh_endpoint_id: EndpointIdText::new("endpoint-a")
                .expect("string ID literal/generated value must be non-empty"),
            scope: MctPeerBindingScope {
                mct_node_id: MctNodeId::new("node-b")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
                data_scope: None,
                observation_scope: None,
            },
            issuer_node_id: MctNodeId::new("node-a")
                .expect("string ID literal/generated value must be non-empty"),
            policy_revision: 1,
            binding_state: state,
            issued_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            expires_at: None,
            created_by_observation_id: ObservationId::new("obs-binding")
                .expect("string ID literal/generated value must be non-empty"),
            superseded_by_observation_id: None,
        }
    }

    fn context() -> HelloEvaluationContext {
        HelloEvaluationContext {
            ids: EvaluationIds {
                decision_id: DecisionId::new("decision-1")
                    .expect("string ID literal/generated value must be non-empty"),
                observation_id: ObservationId::new("obs-decision")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            now: Timestamp::new("2026-05-31T00:00:30Z").unwrap(),
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
        let evaluation = evaluate_hello(
            &request("endpoint-a"),
            &[],
            &HelloPolicy::default(),
            context(),
        );
        assert_eq!(evaluation.hello_outcome, HelloOutcome::Denied);
        assert_eq!(evaluation.reason, HelloReason::MissingBinding);
        assert_eq!(evaluation.safe_reason, SafeHelloReason::NotAuthorized);
        assert!(!evaluation.is_admitted());
    }

    #[test]
    fn endpoint_mismatch_is_denied_before_binding_lookup() {
        let mut request = request("endpoint-a");
        request.presented_binding.endpoint_id = EndpointIdText::new("endpoint-b")
            .expect("string ID literal/generated value must be non-empty");
        let evaluation = evaluate_hello(
            &request,
            &[binding(BindingState::Admitted)],
            &HelloPolicy::default(),
            context(),
        );
        assert_eq!(evaluation.reason, HelloReason::EndpointMismatch);
        assert_eq!(evaluation.safe_reason, SafeHelloReason::NotAuthorized);
    }

    #[test]
    fn active_binding_admits_intersection_of_requested_policy_and_binding_alpns() {
        let mut binding = binding(BindingState::Admitted);
        binding.scope.allowed_alpns = vec![MCT_CALL_ALPN.into()];
        let evaluation = evaluate_hello(
            &request("endpoint-a"),
            &[binding],
            &HelloPolicy::default(),
            context(),
        );
        assert!(evaluation.is_admitted());
        assert_eq!(evaluation.accepted_alpns, vec![MCT_CALL_ALPN.to_string()]);
        assert!(evaluation.admits_alpn(MCT_CALL_ALPN));
        assert!(!evaluation.admits_alpn(MCT_HELLO_ALPN));
    }

    #[test]
    fn revoked_binding_is_denied_with_safe_message() {
        let evaluation = evaluate_hello(
            &request("endpoint-a"),
            &[binding(BindingState::Revoked)],
            &HelloPolicy::default(),
            context(),
        );
        assert_eq!(evaluation.reason, HelloReason::BindingRevoked);
        let response = hello_response(
            "response-1",
            &evaluation,
            ObservationId::new("obs-response")
                .expect("string ID literal/generated value must be non-empty"),
        );
        assert_eq!(response.safe_message, "not authorized");
    }

    #[test]
    fn expired_binding_is_denied_with_safe_message() {
        let evaluation = evaluate_hello(
            &request("endpoint-a"),
            &[binding(BindingState::Expired)],
            &HelloPolicy::default(),
            context(),
        );
        assert_eq!(evaluation.reason, HelloReason::BindingExpired);
        let response = hello_response(
            "response-1",
            &evaluation,
            ObservationId::new("obs-response")
                .expect("string ID literal/generated value must be non-empty"),
        );
        assert_eq!(response.safe_message, "not authorized");
    }

    #[test]
    fn active_binding_past_expiry_is_denied() {
        let mut binding = binding(BindingState::Admitted);
        binding.expires_at = Some(Timestamp::new("2026-05-31T00:00:29Z").unwrap());
        let evaluation = evaluate_hello(
            &request("endpoint-a"),
            &[binding],
            &HelloPolicy::default(),
            context(),
        );

        assert_eq!(evaluation.reason, HelloReason::BindingExpired);
        assert_eq!(evaluation.safe_reason, SafeHelloReason::NotAuthorized);
        assert!(!evaluation.is_admitted());
    }

    #[test]
    fn presented_node_claim_must_match_binding_scope() {
        let mut request = request("endpoint-a");
        request.presented_binding.mct_node_id = Some(
            MctNodeId::new("node-c").expect("string ID literal/generated value must be non-empty"),
        );
        let evaluation = evaluate_hello(
            &request,
            &[binding(BindingState::Admitted)],
            &HelloPolicy::default(),
            context(),
        );

        assert_eq!(evaluation.reason, HelloReason::CapabilityInvalid);
        assert_eq!(evaluation.safe_reason, SafeHelloReason::NotAuthorized);
    }

    #[test]
    fn presented_vision_claim_must_match_binding_scope() {
        let mut request = request("endpoint-a");
        request.presented_binding.vision_id = Some(
            VisionId::new("vision-b").expect("string ID literal/generated value must be non-empty"),
        );
        let evaluation = evaluate_hello(
            &request,
            &[binding(BindingState::Admitted)],
            &HelloPolicy::default(),
            context(),
        );

        assert_eq!(evaluation.reason, HelloReason::VisionNotAllowed);
        assert_eq!(evaluation.safe_reason, SafeHelloReason::NotAuthorized);
    }

    #[test]
    fn stale_policy_revision_is_denied() {
        let policy = HelloPolicy {
            current_policy_revision: 2,
            ..HelloPolicy::default()
        };
        let evaluation = evaluate_hello(
            &request("endpoint-a"),
            &[binding(BindingState::Admitted)],
            &policy,
            context(),
        );
        assert_eq!(evaluation.reason, HelloReason::PolicyRevisionStale);
        assert_eq!(evaluation.safe_reason, SafeHelloReason::NotAuthorized);
    }

    #[test]
    fn unsupported_major_version_requests_upgrade() {
        let mut request = request("endpoint-a");
        request.requested_protocol.major = 99;
        let evaluation = evaluate_hello(
            &request,
            &[binding(BindingState::Admitted)],
            &HelloPolicy::default(),
            context(),
        );
        assert_eq!(evaluation.hello_outcome, HelloOutcome::UpgradeRequired);
        assert_eq!(evaluation.safe_reason, SafeHelloReason::UnsupportedVersion);
    }
}

use crate::{
    call::{GrantsAuthorityIdentity, RuntimeKind},
    error::{MctKernelResult, ensure_non_blank},
    id::*,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

mod internal;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NonBlank(String);

impl NonBlank {
    pub fn new(value: impl Into<String>) -> MctKernelResult<Self> {
        let value = value.into();
        ensure_non_blank("NonBlank", "value", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for NonBlank {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for NonBlank {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl From<&str> for NonBlank {
    fn from(value: &str) -> Self {
        Self::new(value).expect("ALPN and relay URL must be non-blank")
    }
}

impl From<String> for NonBlank {
    fn from(value: String) -> Self {
        Self::new(value).expect("ALPN and relay URL must be non-blank")
    }
}

impl AsRef<str> for NonBlank {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for NonBlank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelloSurfaceVisibility {
    VisionScoped,
}

impl HelloSurfaceVisibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::VisionScoped => "vision_scoped",
        }
    }
}

impl From<&str> for HelloSurfaceVisibility {
    fn from(value: &str) -> Self {
        if value == "vision_scoped" {
            Self::VisionScoped
        } else {
            panic!("hello callable surface visibility must be vision_scoped")
        }
    }
}

impl From<String> for HelloSurfaceVisibility {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl PartialEq<str> for HelloSurfaceVisibility {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for HelloSurfaceVisibility {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

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
/// Endpoint ID proves only possession of the Iroh key. Authority requires a
/// matching admitted peer binding during hello evaluation.
pub struct IrohConnectionPresentation {
    pub endpoint_id: EndpointIdText,
    pub alpn: NonBlank,
    pub connection_side: ConnectionSide,
    pub path_class: PathClass,
    pub relay_url: Option<NonBlank>,
    pub presented_capability_ref: Option<String>,
}

impl IrohConnectionPresentation {
    pub fn validate(&self) -> MctKernelResult<()> {
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
/// Hello admission intersects requested ALPNs with `allowed_alpns` and binds the
/// endpoint to exactly one MCT node and Vision.
pub struct MctPeerBindingScope {
    pub mct_node_id: MctNodeId,
    pub vision_id: VisionId,
    pub allowed_alpns: Vec<String>,
    /// Optional data scope for higher-level policy projections.
    pub data_scope: Option<String>,
    /// Optional observation sharing scope for audit projections.
    pub observation_scope: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingState {
    /// Binding exists but hello admission must retry later.
    Pending,
    /// Binding may authorize hello if all other facts match.
    Admitted,
    /// Binding was denied and is treated as no usable binding.
    Denied,
    Expired,
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Hello admission requires `Admitted`, matching endpoint/binding claims, fresh
/// policy revision, and the mandatory `expires_at > now` time bound.
pub struct MctPeerBinding {
    pub binding_id: PeerBindingId,
    pub iroh_endpoint_id: EndpointIdText,
    pub scope: MctPeerBindingScope,
    pub issuer_node_id: MctNodeId,
    pub policy_revision: u64,
    pub binding_state: BindingState,
    pub issued_at: Timestamp,
    /// Mandatory expiry compared against adapter-supplied `now`.
    pub expires_at: Timestamp,
    pub created_by_observation_id: ObservationId,
    pub superseded_by_observation_id: Option<ObservationId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// The snapshot is not an admission token. Kernel evaluation still checks the
/// selected binding, its scope and lifecycle, and the current policy revision.
pub struct MctPeerAuthoritySnapshot {
    pub bindings: Vec<MctPeerBinding>,
    pub policy_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Hello accepts only the configured protocol name and major version.
pub struct MctProtocolVersion {
    pub protocol_name: String,
    /// Major version that must match the local policy.
    pub major: u32,
    pub minor: u32,
    pub compatibility_floor: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Claims are checked against local binding records; omitted node, Vision, or
/// revision claims do not grant authority, but mismatched claims deny.
pub struct MctPeerBindingPresentation {
    pub binding_id: Option<PeerBindingId>,
    /// Endpoint the peer claims; must match the observed connection endpoint.
    pub endpoint_id: EndpointIdText,
    /// Optional node claim; if present it must match the selected binding scope.
    pub mct_node_id: Option<MctNodeId>,
    /// Optional Vision claim; if present it must match the selected binding scope.
    pub vision_id: Option<VisionId>,
    /// Optional policy revision claim checked for staleness.
    pub policy_revision: Option<u64>,
    pub allowed_alpns_claim: Vec<String>,
    pub signature_ref: Option<String>,
    /// Peer-presented expiry, recorded but local binding expiry controls admission.
    pub expires_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctHelloCallableSurface {
    pub child_name: String,
    /// Canonical WIT operation ID, for example `patina:demo/control@0.1.0.run`.
    pub operation_id: String,
    pub runtime_kind: RuntimeKind,
    pub vision_id: VisionId,
    pub policy_revision: u64,
    pub visibility: HelloSurfaceVisibility,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctHelloCapabilityView {
    pub node_id: MctNodeId,
    pub vision_id: VisionId,
    pub published_at: Timestamp,
    /// Publisher-local policy revision used for change detection only.
    pub policy_revision: u64,
    pub supported_alpns: Vec<String>,
    pub supported_wit_worlds: Vec<String>,
    pub supported_observation_modes: Vec<String>,
    pub callable_surfaces: Vec<MctHelloCallableSurface>,
    pub capability_view_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// `mct/hello/0` request presented before any call authority exists.
///
/// Evaluation admits only the intersection of requested ALPNs, local policy,
/// and the selected active binding scope.
pub struct MctHelloRequest {
    pub hello_id: String,
    pub received_over: IrohConnectionPresentation,
    pub requested_protocol: MctProtocolVersion,
    /// Optional Vision requested by the peer; if present it must match binding scope.
    pub requested_vision_id: Option<VisionId>,
    pub requested_alpns: Vec<String>,
    pub presented_binding: MctPeerBindingPresentation,
    /// Optional capability summary, recorded but not sufficient for authority.
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
    /// Matching binding is expired by state or time window.
    BindingExpired,
    /// Requested or presented Vision is outside binding scope.
    VisionNotAllowed,
    /// Requested ALPNs did not intersect local policy and binding scope.
    AlpnNotAllowed,
    /// Requested protocol name or major version is unsupported.
    VersionUnsupported,
    /// Binding or presented revision is older than local policy.
    PolicyRevisionStale,
    /// Presented node id does not match the selected binding scope.
    CapabilityInvalid,
    RelayAccessDenied,
    TemporaryUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Caller-safe projection of hello admission reasons.
pub enum SafeHelloReason {
    /// Do not disclose which authority fact failed.
    NotAuthorized,
    /// Safe to disclose that the protocol version is unsupported.
    UnsupportedVersion,
    /// Safe to ask the peer to retry later.
    RetryLater,
    Admitted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Admitted evaluations carry selected node, Vision, binding, negotiated
/// protocol, and accepted ALPNs; denied evaluations leave those authority
/// grants empty.
pub struct MctHelloAdmissionEvaluation {
    pub decision_id: DecisionId,
    pub request_id: String,
    /// Optional lower-level peer decision, reserved for future projections.
    pub peer_admission_decision_id: Option<DecisionId>,
    /// Binding selected for authority; present only on admission.
    pub selected_binding_id: Option<PeerBindingId>,
    /// MCT node admitted for subsequent calls; present only on admission.
    pub selected_node_id: Option<MctNodeId>,
    /// Vision admitted for subsequent calls; present only on admission.
    pub selected_vision_id: Option<VisionId>,
    /// Binding policy revision admitted by hello; present only on admission.
    pub selected_policy_revision: Option<u64>,
    /// Protocol version negotiated; present only on admission.
    pub negotiated_protocol: Option<MctProtocolVersion>,
    pub accepted_alpns: Vec<String>,
    pub hello_outcome: HelloOutcome,
    /// Full typed reason for observations and audit.
    pub reason: HelloReason,
    /// Caller-safe reason used to build responses.
    pub safe_reason: SafeHelloReason,
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctHelloResponse {
    pub response_id: String,
    pub request_id: String,
    pub decision_id: DecisionId,
    pub hello_outcome: HelloOutcome,
    /// Negotiated protocol, present only when admitted.
    pub negotiated_protocol: Option<MctProtocolVersion>,
    pub accepted_alpns: Vec<String>,
    /// Current receiver authority identity, present only for a proof-gated admission.
    pub receiving_grants_authority: Option<GrantsAuthorityIdentity>,
    pub safe_message: String,
    pub retry_after: Option<Timestamp>,
    pub capability_view: Option<MctHelloCapabilityView>,
    pub response_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerAdmissionOutcome {
    Admitted,
    /// Peer admission failed closed.
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
    /// Presented node id does not match the selected binding scope.
    CapabilityInvalid,
    RelayAccessDenied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Lower-level peer binding decision retained for observation compatibility.
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
    /// Minimum policy revision accepted for bindings and presentations.
    pub current_policy_revision: u64,
    pub supported_alpns: Vec<String>,
}

impl Default for HelloPolicy {
    fn default() -> Self {
        Self {
            protocol: MctProtocolVersion {
                protocol_name: MCT_HELLO_ALPN.into(),
                major: 1,
                minor: 0,
                compatibility_floor: Some(1),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelloEvaluationContext {
    pub ids: EvaluationIds,
    /// Adapter-supplied current time used for expiry checks.
    pub now: Timestamp,
}

/// Decides whether a peer connection may proceed beyond `mct/hello/0`.
///
/// Authority facts are the hello request, local peer bindings, local hello
/// policy, and adapter-supplied current time. Returns `Admitted` only for a
/// matching active binding with compatible protocol, Vision, ALPN intersection,
/// fresh policy revision, and unexpired time window. Missing or mismatched
/// authority becomes a denied/retry/upgrade decision, never an error.
pub fn evaluate_hello(
    request: &MctHelloRequest,
    bindings: &[MctPeerBinding],
    policy: &HelloPolicy,
    context: HelloEvaluationContext,
) -> MctHelloAdmissionEvaluation {
    internal::evaluate_hello_internal(request, bindings, policy, context)
}

impl MctHelloAdmissionEvaluation {
    /// Converts an otherwise-admitted hello into the closed temporary posture used when
    /// the receiving Mother cannot prove current grants authority.
    pub fn refuse_receiver_authority_unavailable(&mut self) {
        if !self.is_admitted() {
            return;
        }
        self.peer_admission_decision_id = None;
        self.selected_binding_id = None;
        self.selected_node_id = None;
        self.selected_vision_id = None;
        self.selected_policy_revision = None;
        self.negotiated_protocol = None;
        self.accepted_alpns.clear();
        self.hello_outcome = HelloOutcome::RetryLater;
        self.reason = HelloReason::TemporaryUnavailable;
        self.safe_reason = SafeHelloReason::RetryLater;
    }

    pub fn is_admitted(&self) -> bool {
        self.hello_outcome == HelloOutcome::Admitted
    }

    pub fn admits_alpn(&self, alpn: &str) -> bool {
        self.accepted_alpns.iter().any(|accepted| accepted == alpn)
    }
}

/// Projects a hello evaluation into the caller-safe wire response.
///
/// The response copies negotiated protocol and accepted ALPNs only from the
/// evaluation and maps detailed reasons to safe messages.
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
        receiving_grants_authority: None,
        safe_message: safe_message.into(),
        retry_after: None,
        capability_view: None,
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
            expires_at: Timestamp::new("2026-05-31T00:05:00Z").unwrap(),
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
    fn hello_capability_view_carries_callable_surfaces() {
        let view = MctHelloCapabilityView {
            node_id: MctNodeId::new("node-a")
                .expect("string ID literal/generated value must be non-empty"),
            vision_id: VisionId::new("vision-a")
                .expect("string ID literal/generated value must be non-empty"),
            published_at: Timestamp::new("2026-07-09T00:00:00Z").unwrap(),
            policy_revision: 7,
            supported_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            supported_wit_worlds: vec!["patina:demo/control@0.1.0".into()],
            supported_observation_modes: vec!["local-ledger".into()],
            callable_surfaces: vec![MctHelloCallableSurface {
                child_name: "resident-wit".into(),
                operation_id: "patina:demo/control@0.1.0.run".into(),
                runtime_kind: RuntimeKind::WasmComponent,
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                policy_revision: 9,
                visibility: "vision_scoped".into(),
            }],
            capability_view_ref: None,
        };

        let decoded: MctHelloCapabilityView =
            serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();

        assert_eq!(decoded.node_id, view.node_id);
        assert_eq!(decoded.vision_id, view.vision_id);
        assert_eq!(decoded.policy_revision, 7);
        assert_eq!(decoded.callable_surfaces.len(), 1);
        assert_eq!(
            decoded.callable_surfaces[0].operation_id,
            "patina:demo/control@0.1.0.run"
        );
        assert_eq!(
            decoded.callable_surfaces[0].runtime_kind,
            RuntimeKind::WasmComponent
        );
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

    /// Contract: `MctIrohPeerBindingAuthority.EveryPeerBindingIsTimeBounded`.
    #[test]
    fn binding_without_expiry_fails_closed() {
        let mut value = serde_json::to_value(binding(BindingState::Admitted)).unwrap();
        value.as_object_mut().unwrap().remove("expires_at");

        assert_eq!(
            serde_json::from_value::<MctPeerBinding>(value)
                .unwrap_err()
                .to_string(),
            "missing field `expires_at`"
        );
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
        binding.expires_at = Timestamp::new("2026-05-31T00:00:29Z").unwrap();
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

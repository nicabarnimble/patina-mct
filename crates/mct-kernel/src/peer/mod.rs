use crate::{
    call::RuntimeKind,
    error::{MctKernelResult, ensure_non_blank},
    id::*,
};
use serde::{Deserialize, Serialize};

mod internal;

/// ALPN for the hello/admission protocol between Mothers.
pub const MCT_HELLO_ALPN: &str = "mct/hello/0";
/// ALPN for submitting calls after hello admission.
pub const MCT_CALL_ALPN: &str = "mct/call/0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Direction of a transport connection from the local Mother's perspective.
pub enum ConnectionSide {
    /// Peer connected to the local Mother.
    Incoming,
    /// Local Mother initiated the peer connection.
    Outgoing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Coarse network path class supplied by the adapter for audit and policy.
pub enum PathClass {
    /// Direct endpoint-to-endpoint path.
    Direct,
    /// Connection used a relay path.
    Relayed,
    /// Only relay connectivity is available.
    RelayOnly,
    /// Adapter classified the path as privacy-preserving transport.
    PrivacyTransport,
    /// Adapter could not classify the path.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Transport facts observed by the adapter for one Iroh connection.
///
/// Endpoint ID proves only possession of the Iroh key. Authority requires a
/// matching admitted peer binding during hello evaluation.
pub struct IrohConnectionPresentation {
    /// Iroh endpoint ID observed on the connection.
    pub endpoint_id: EndpointIdText,
    /// Negotiated ALPN; must be non-blank.
    pub alpn: String,
    /// Whether this endpoint accepted or initiated the connection.
    pub connection_side: ConnectionSide,
    /// Adapter-supplied network path class for audit/policy.
    pub path_class: PathClass,
    /// Relay URL used by the connection, if known; if present it must be non-blank.
    pub relay_url: Option<String>,
    /// Optional adapter reference to peer capability evidence.
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
/// Authority scope granted by a peer binding.
///
/// Hello admission intersects requested ALPNs with `allowed_alpns` and binds the
/// endpoint to exactly one MCT node and Vision.
pub struct MctPeerBindingScope {
    /// MCT node identity the endpoint may present.
    pub mct_node_id: MctNodeId,
    /// Vision boundary in which the binding is valid.
    pub vision_id: VisionId,
    /// ALPNs the peer may negotiate under this binding.
    pub allowed_alpns: Vec<String>,
    /// Optional data scope for higher-level policy projections.
    pub data_scope: Option<String>,
    /// Optional observation sharing scope for audit projections.
    pub observation_scope: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Lifecycle state of a peer binding as authority data.
pub enum BindingState {
    /// Binding exists but hello admission must retry later.
    Pending,
    /// Binding may authorize hello if all other facts match.
    Admitted,
    /// Binding was denied and is treated as no usable binding.
    Denied,
    /// Binding is no longer valid by lifecycle state.
    Expired,
    /// Binding was explicitly revoked.
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Durable authority record tying an Iroh endpoint to an MCT node and Vision.
///
/// Hello admission requires `Admitted`, matching endpoint/binding claims, fresh
/// policy revision, and the mandatory `expires_at > now` time bound.
pub struct MctPeerBinding {
    /// Stable identifier for this binding authority record.
    pub binding_id: PeerBindingId,
    /// Iroh endpoint key text this binding admits.
    pub iroh_endpoint_id: EndpointIdText,
    /// Node, Vision, and ALPN scope granted by the binding.
    pub scope: MctPeerBindingScope,
    /// Node that issued this binding.
    pub issuer_node_id: MctNodeId,
    /// Policy revision under which the binding was issued.
    pub policy_revision: u64,
    /// Current lifecycle state used by hello admission.
    pub binding_state: BindingState,
    /// Time the binding was issued, for audit.
    pub issued_at: Timestamp,
    /// Mandatory expiry compared against adapter-supplied `now`.
    pub expires_at: Timestamp,
    /// Observation that created or admitted this binding.
    pub created_by_observation_id: ObservationId,
    /// Observation that superseded this binding, if it has been replaced.
    pub superseded_by_observation_id: Option<ObservationId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Current peer-binding authority loaded by an adapter for one protocol evaluation.
///
/// The snapshot is not an admission token. Kernel evaluation still checks the
/// selected binding, its scope and lifecycle, and the current policy revision.
pub struct MctPeerAuthoritySnapshot {
    /// Peer bindings current at the adapter boundary.
    pub bindings: Vec<MctPeerBinding>,
    /// Local policy revision current at the adapter boundary.
    pub policy_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Protocol version requested or negotiated during hello.
///
/// Hello accepts only the configured protocol name and major version; minor and
/// compatibility floor are recorded for negotiation/audit.
pub struct MctProtocolVersion {
    /// Protocol name, normally `mct/hello/0` for hello negotiation.
    pub protocol_name: String,
    /// Major version that must match the local policy.
    pub major: u32,
    /// Minor version requested or selected.
    pub minor: u32,
    /// Lowest compatible major/minor floor advertised by the peer.
    pub compatibility_floor: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Binding claims presented by a peer during hello.
///
/// Claims are checked against local binding records; omitted node, Vision, or
/// revision claims do not grant authority, but mismatched claims deny.
pub struct MctPeerBindingPresentation {
    /// Optional binding ID used to select a local binding record.
    pub binding_id: Option<PeerBindingId>,
    /// Endpoint the peer claims; must match the observed connection endpoint.
    pub endpoint_id: EndpointIdText,
    /// Optional node claim; if present it must match the selected binding scope.
    pub mct_node_id: Option<MctNodeId>,
    /// Optional Vision claim; if present it must match the selected binding scope.
    pub vision_id: Option<VisionId>,
    /// Optional policy revision claim checked for staleness.
    pub policy_revision: Option<u64>,
    /// ALPNs the peer asks to use under this binding.
    pub allowed_alpns_claim: Vec<String>,
    /// Adapter-managed reference to binding proof material.
    pub signature_ref: Option<String>,
    /// Peer-presented expiry, recorded but local binding expiry controls admission.
    pub expires_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// One Vision-scoped callable child operation advertised during hello.
pub struct MctHelloCallableSurface {
    /// Publisher-local child name that can execute the operation.
    pub child_name: String,
    /// Canonical WIT operation ID, for example `patina:demo/control@0.1.0.run`.
    pub operation_id: String,
    /// Runtime class used locally by the publishing Mother.
    pub runtime_kind: RuntimeKind,
    /// Vision boundary in which this operation is published.
    pub vision_id: VisionId,
    /// Publisher-local policy revision that produced this surface evidence.
    pub policy_revision: u64,
    /// Surface visibility label; this phase expects `vision_scoped`.
    pub visibility: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Peer-advertised capability summary attached to hello for audit/planning.
pub struct MctHelloCapabilityView {
    /// Publishing MCT node.
    pub node_id: MctNodeId,
    /// Vision boundary in which this view is valid.
    pub vision_id: VisionId,
    /// Publisher timestamp for this view.
    pub published_at: Timestamp,
    /// Publisher-local policy revision used for change detection only.
    pub policy_revision: u64,
    /// ALPNs the peer says it can speak.
    pub supported_alpns: Vec<String>,
    /// WIT worlds the peer says it can host or route.
    pub supported_wit_worlds: Vec<String>,
    /// Observation sharing modes advertised by the peer.
    pub supported_observation_modes: Vec<String>,
    /// Vision-scoped callable operations published by the peer.
    pub callable_surfaces: Vec<MctHelloCallableSurface>,
    /// Optional external reference to a fuller capability document.
    pub capability_view_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// `mct/hello/0` request presented before any call authority exists.
///
/// Evaluation admits only the intersection of requested ALPNs, local policy,
/// and the selected active binding scope.
pub struct MctHelloRequest {
    /// Request identifier copied into the evaluation and response.
    pub hello_id: String,
    /// Adapter-observed transport facts for the hello connection.
    pub received_over: IrohConnectionPresentation,
    /// Protocol version the peer wants to negotiate.
    pub requested_protocol: MctProtocolVersion,
    /// Optional Vision requested by the peer; if present it must match binding scope.
    pub requested_vision_id: Option<VisionId>,
    /// ALPNs requested for subsequent protocols.
    pub requested_alpns: Vec<String>,
    /// Binding claims supplied by the peer.
    pub presented_binding: MctPeerBindingPresentation,
    /// Optional capability summary, recorded but not sufficient for authority.
    pub capability_view: Option<MctHelloCapabilityView>,
    /// Policy revision the peer says it has seen.
    pub local_policy_revision_seen: Option<u64>,
    /// Trace joining hello, call, and observations.
    pub trace_id: TraceId,
    /// Observation recording receipt of the hello request.
    pub received_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Public outcome class for hello admission and response.
pub enum HelloOutcome {
    /// Peer was admitted under a selected binding.
    Admitted,
    /// Peer was not authorized.
    Denied,
    /// Peer may retry after a temporary state such as pending binding.
    RetryLater,
    /// Requested protocol major/name is unsupported.
    UpgradeRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Audit reason for a hello admission decision.
pub enum HelloReason {
    /// A matching admitted binding authorized the peer.
    ActiveBinding,
    /// Presented endpoint did not match the observed connection endpoint.
    EndpointMismatch,
    /// No local binding matched the presented endpoint or binding ID.
    MissingBinding,
    /// Matching binding exists but is not yet admitted.
    BindingPending,
    /// Matching binding has been revoked.
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
    /// Presented node or capability claim conflicts with the binding.
    CapabilityInvalid,
    /// Relay/path policy denied this connection class.
    RelayAccessDenied,
    /// Local policy asks the peer to retry later.
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
    /// Safe admission message.
    Admitted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Kernel decision for a hello request.
///
/// Admitted evaluations carry selected node, Vision, binding, negotiated
/// protocol, and accepted ALPNs; denied evaluations leave those authority
/// grants empty.
pub struct MctHelloAdmissionEvaluation {
    /// Decision identifier assigned by the adapter context.
    pub decision_id: DecisionId,
    /// Hello request identifier being answered.
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
    /// ALPNs admitted for later phases.
    pub accepted_alpns: Vec<String>,
    /// Public outcome of hello evaluation.
    pub hello_outcome: HelloOutcome,
    /// Full typed reason for observations and audit.
    pub reason: HelloReason,
    /// Caller-safe reason used to build responses.
    pub safe_reason: SafeHelloReason,
    /// Observation recording this evaluation.
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Caller-facing response derived from a hello evaluation.
pub struct MctHelloResponse {
    /// Response identifier minted by the serving adapter.
    pub response_id: String,
    /// Hello request identifier being answered.
    pub request_id: String,
    /// Hello decision that determined this response.
    pub decision_id: DecisionId,
    /// Caller-facing admission outcome.
    pub hello_outcome: HelloOutcome,
    /// Negotiated protocol, present only when admitted.
    pub negotiated_protocol: Option<MctProtocolVersion>,
    /// ALPNs admitted for later protocols.
    pub accepted_alpns: Vec<String>,
    /// Safe message derived from [`SafeHelloReason`].
    pub safe_message: String,
    /// Optional retry time for retry-later responses.
    pub retry_after: Option<Timestamp>,
    /// Optional capability summary published by the admitting Mother.
    pub capability_view: Option<MctHelloCapabilityView>,
    /// Observation recording response emission.
    pub response_observation_id: ObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Internal peer admission outcome before hello response projection.
pub enum PeerAdmissionOutcome {
    /// Peer admission succeeded.
    Admitted,
    /// Peer admission failed closed.
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Internal reason for peer admission decisions.
pub enum PeerAdmissionReason {
    /// A matching admitted binding authorized the peer.
    ActiveBinding,
    /// Endpoint is unknown to local peer authority.
    UnknownEndpoint,
    /// No local binding matched the presented endpoint or binding ID.
    MissingBinding,
    /// Matching binding exists but is not yet admitted.
    BindingPending,
    /// Matching binding has been revoked.
    BindingRevoked,
    /// Matching binding is expired by state or time window.
    BindingExpired,
    /// Requested or presented Vision is outside binding scope.
    VisionNotAllowed,
    /// Requested ALPNs did not intersect local policy and binding scope.
    AlpnNotAllowed,
    /// Binding or presented revision is older than local policy.
    PolicyRevisionStale,
    /// Presented node or capability claim conflicts with the binding.
    CapabilityInvalid,
    /// Relay/path policy denied this connection class.
    RelayAccessDenied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Lower-level peer binding decision retained for observation compatibility.
pub struct MctPeerAdmissionDecision {
    /// Decision identifier assigned to peer admission.
    pub decision_id: DecisionId,
    /// Connection facts evaluated for the peer.
    pub presentation: IrohConnectionPresentation,
    /// Binding considered by the decision, if any.
    pub binding_id: Option<PeerBindingId>,
    /// Vision requested by the peer, if any.
    pub requested_vision_id: Option<VisionId>,
    /// Policy revision used for the decision.
    pub policy_revision: u64,
    /// Admission outcome before safe response projection.
    pub outcome: PeerAdmissionOutcome,
    /// Typed audit reason for the outcome.
    pub reason: PeerAdmissionReason,
    /// Observation recording this decision.
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Local policy facts used to evaluate hello admission.
pub struct HelloPolicy {
    /// Protocol version the local Mother is willing to negotiate.
    pub protocol: MctProtocolVersion,
    /// Minimum policy revision accepted for bindings and presentations.
    pub current_policy_revision: u64,
    /// ALPNs supported by local policy for hello negotiation.
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
/// Caller-supplied identifiers for hello evaluation evidence.
pub struct EvaluationIds {
    /// Decision identifier assigned to the hello evaluation.
    pub decision_id: DecisionId,
    /// Observation identifier assigned to the hello evaluation.
    pub observation_id: ObservationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Evaluation context supplied by the adapter, including current time.
pub struct HelloEvaluationContext {
    /// Identifiers to stamp on the produced evaluation.
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
    /// Returns true only when the evaluation grants hello admission.
    pub fn is_admitted(&self) -> bool {
        self.hello_outcome == HelloOutcome::Admitted
    }

    /// Returns true when hello admission included the requested ALPN.
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

        assert!(serde_json::from_value::<MctPeerBinding>(value).is_err());
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

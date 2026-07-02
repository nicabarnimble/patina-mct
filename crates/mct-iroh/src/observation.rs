use crate::endpoint::MotherIrohEndpointSnapshot;
use crate::test_support::LocalIrohDeniedPeerReport;
use mct_kernel::*;

/// Build canonical MCT observations for a local denied-peer adapter proof.
///
/// This is a projection from safe adapter facts into `MctObservation`; logs,
/// metrics, qlog, and OTel can later project from the same facts.
pub(crate) fn local_denied_peer_adapter_observations(
    bound_endpoint: &MotherIrohEndpointSnapshot,
    closed_endpoint: &MotherIrohEndpointSnapshot,
    report: &LocalIrohDeniedPeerReport,
    trace_id: TraceId,
) -> Vec<MctObservation> {
    vec![
        adapter_observation(AdapterObservationFacts {
            observation_id: "obs-iroh-endpoint-bound",
            kind: ObservationKind::AdapterEffectStarted,
            trace_id: trace_id.clone(),
            call_id: None,
            decision_id: None,
            outcome: ObservationOutcome::Started,
            safe_message: "iroh endpoint bound".into(),
            subject_id: Some(bound_endpoint.endpoint_id.as_str().to_string()),
            resource_id: Some("mct-iroh-endpoint".into()),
        }),
        adapter_observation(AdapterObservationFacts {
            observation_id: "obs-iroh-hello-received",
            kind: ObservationKind::PeerHelloReceived,
            trace_id: trace_id.clone(),
            call_id: None,
            decision_id: None,
            outcome: ObservationOutcome::Started,
            safe_message: "peer hello received".into(),
            subject_id: Some(bound_endpoint.endpoint_id.as_str().to_string()),
            resource_id: Some(MCT_HELLO_ALPN.into()),
        }),
        adapter_observation(AdapterObservationFacts {
            observation_id: "obs-iroh-peer-rejected",
            kind: ObservationKind::PeerRejected,
            trace_id: trace_id.clone(),
            call_id: None,
            decision_id: Some(report.hello_evaluation.decision_id.clone()),
            outcome: ObservationOutcome::Denied,
            safe_message: report.hello_response.safe_message.clone(),
            subject_id: report
                .hello_evaluation
                .selected_binding_id
                .as_ref()
                .map(ToString::to_string),
            resource_id: Some(MCT_HELLO_ALPN.into()),
        }),
        adapter_observation(AdapterObservationFacts {
            observation_id: "obs-iroh-peer-call-received",
            kind: ObservationKind::PeerCallReceived,
            trace_id: trace_id.clone(),
            call_id: report.call_evaluation.call_id.clone(),
            decision_id: None,
            outcome: ObservationOutcome::Started,
            safe_message: "peer call received".into(),
            subject_id: None,
            resource_id: Some(MCT_CALL_ALPN.into()),
        }),
        adapter_observation(AdapterObservationFacts {
            observation_id: "obs-iroh-peer-call-replied",
            kind: ObservationKind::PeerCallReplied,
            trace_id: trace_id.clone(),
            call_id: report.call_evaluation.call_id.clone(),
            decision_id: Some(report.call_evaluation.decision_id.clone()),
            outcome: ObservationOutcome::Denied,
            safe_message: report.call_reply.safe_message.clone(),
            subject_id: None,
            resource_id: Some(MCT_CALL_ALPN.into()),
        }),
        adapter_observation(AdapterObservationFacts {
            observation_id: "obs-iroh-endpoint-closed",
            kind: ObservationKind::AdapterEffectCompleted,
            trace_id,
            call_id: None,
            decision_id: None,
            outcome: ObservationOutcome::Completed,
            safe_message: "iroh endpoint closed".into(),
            subject_id: Some(closed_endpoint.endpoint_id.as_str().to_string()),
            resource_id: Some("mct-iroh-endpoint".into()),
        }),
    ]
}

struct AdapterObservationFacts {
    observation_id: &'static str,
    kind: ObservationKind,
    trace_id: TraceId,
    call_id: Option<CallId>,
    decision_id: Option<DecisionId>,
    outcome: ObservationOutcome,
    safe_message: String,
    subject_id: Option<String>,
    resource_id: Option<String>,
}

fn adapter_observation(facts: AdapterObservationFacts) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::new(facts.observation_id)
            .expect("string ID literal/generated value must be non-empty"),
        observed_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
        kind: facts.kind,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: facts.trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: facts.call_id,
        decision_id: facts.decision_id,
        subject_id: facts.subject_id,
        resource_id: facts.resource_id,
        policy_revision: Some(1),
        grants_revision: Some(1),
        outcome: facts.outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: facts.safe_message,
        detail_ref: None,
    }
}

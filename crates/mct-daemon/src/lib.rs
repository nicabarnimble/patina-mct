//! MCT daemon composition layer.
//!
//! The daemon composes the kernel, observation ledger, and adapters. Authority
//! remains in `mct-kernel`; external effects remain in adapter crates.

#![forbid(unsafe_code)]

#[cfg(test)]
use anyhow::{Context, Result};
use mct_iroh::{MotherIrohEndpointLifecycle, MotherIrohEndpointSnapshot};
#[cfg(test)]
use mct_kernel::*;
#[cfg(test)]
use mct_observation::JsonlObservationLedger;
#[cfg(test)]
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MctDaemonHealth {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MctDaemonReadiness {
    Ready,
    NotReady,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctDaemonStatus {
    pub version: String,
    pub health: MctDaemonHealth,
    pub readiness: MctDaemonReadiness,
    pub iroh_endpoint: Option<MotherIrohEndpointSnapshot>,
    pub safe_message: String,
}

pub fn daemon_status(iroh_endpoint: Option<MotherIrohEndpointSnapshot>) -> MctDaemonStatus {
    let readiness = match iroh_endpoint.as_ref() {
        Some(snapshot) if snapshot.lifecycle == MotherIrohEndpointLifecycle::Bound => {
            MctDaemonReadiness::Ready
        }
        _ => MctDaemonReadiness::NotReady,
    };

    let safe_message = match readiness {
        MctDaemonReadiness::Ready => "ready".into(),
        MctDaemonReadiness::NotReady => "iroh endpoint not ready".into(),
    };

    MctDaemonStatus {
        version: version().into(),
        health: MctDaemonHealth::Healthy,
        readiness,
        iroh_endpoint,
        safe_message,
    }
}

#[cfg(test)]
struct FakeEchoReport {
    hello: MctHelloAdmissionEvaluation,
    call: MctCallProtocolEvaluation,
    result: MctResult,
    reply: MctCallProtocolReply,
    trace_observation_count: usize,
}

#[cfg(test)]
struct FakeEndToEndStatusReport {
    daemon: MctDaemonStatus,
    echo: FakeEchoReport,
    call_observation_count: usize,
}

#[cfg(test)]
fn run_fake_end_to_end_status_slice(
    ledger_path: impl AsRef<Path>,
    iroh_endpoint: MotherIrohEndpointSnapshot,
) -> Result<FakeEndToEndStatusReport> {
    let ledger_path = ledger_path.as_ref();
    let echo = run_fake_echo_slice(ledger_path)?;
    let ledger = JsonlObservationLedger::open(ledger_path, "ledger-dev", "mother-a")
        .context("open fake end-to-end status ledger")?;
    let call_observation_count = ledger.by_call(&CallId::from("call-fake-echo"))?.len();

    Ok(FakeEndToEndStatusReport {
        daemon: daemon_status(Some(iroh_endpoint)),
        echo,
        call_observation_count,
    })
}

/// Run the first fake local vertical slice without real networking.
///
/// This proves composition before adding the Iroh adapter:
/// peer binding → `mct/hello/0` → `mct/call/0` → fake echo handler → observations.
#[cfg(test)]
fn run_fake_echo_slice(ledger_path: impl AsRef<Path>) -> Result<FakeEchoReport> {
    let mut ledger = JsonlObservationLedger::open(ledger_path, "ledger-dev", "mother-a")
        .context("open fake echo observation ledger")?;

    let trace_id = TraceId::from("trace-fake-echo");
    let binding = fake_binding();
    let hello_request = fake_hello_request(&trace_id);

    ledger
        .append_before_effect(
            observation(
                "obs-hello-received",
                ObservationKind::PeerHelloReceived,
                trace_id.clone(),
                None,
                None,
                ObservationOutcome::Started,
                "hello received",
            ),
            "2026-05-31T00:00:01Z",
        )
        .context("append hello received observation")?;

    let hello = evaluate_hello(
        &hello_request,
        &[binding],
        &HelloPolicy::default(),
        EvaluationIds {
            decision_id: DecisionId::from("decision-hello"),
            observation_id: ObservationId::from("obs-hello-decision"),
        },
    );
    ledger
        .append_before_effect(
            observation(
                "obs-hello-decision",
                ObservationKind::PeerProtocolNegotiated,
                trace_id.clone(),
                None,
                Some(hello.decision_id.clone()),
                if hello.is_admitted() {
                    ObservationOutcome::Allowed
                } else {
                    ObservationOutcome::Denied
                },
                "hello evaluated",
            ),
            "2026-05-31T00:00:02Z",
        )
        .context("append hello decision observation")?;

    if !hello.is_admitted() {
        anyhow::bail!("fake slice expected admitted hello");
    }

    let call_request = fake_call_request(&trace_id, &hello);
    ledger
        .append_before_effect(
            observation(
                "obs-peer-call-received",
                ObservationKind::PeerCallReceived,
                trace_id.clone(),
                Some(call_request.call.call_id.clone()),
                None,
                ObservationOutcome::Started,
                "peer call received",
            ),
            "2026-05-31T00:00:03Z",
        )
        .context("append peer call received observation")?;

    let call = evaluate_call_protocol(
        &call_request,
        &hello,
        CallEvaluationIds {
            decision_id: DecisionId::from("decision-call"),
            observation_id: ObservationId::from("obs-call-decision"),
        },
    );
    ledger
        .append_before_effect(
            observation(
                "obs-call-decision",
                ObservationKind::CallAuthorized,
                trace_id.clone(),
                Some(call_request.call.call_id.clone()),
                Some(call.decision_id.clone()),
                if call.is_accepted_for_routing() {
                    ObservationOutcome::Allowed
                } else {
                    ObservationOutcome::Denied
                },
                "call evaluated",
            ),
            "2026-05-31T00:00:04Z",
        )
        .context("append call decision observation")?;

    if !call.is_accepted_for_routing() {
        anyhow::bail!("fake slice expected call accepted for routing");
    }

    let result = fake_echo_result(&call_request.call);
    ledger
        .append_before_effect(
            observation(
                "obs-result-recorded",
                ObservationKind::ResultRecorded,
                trace_id.clone(),
                Some(result.call_id.clone()),
                Some(DecisionId::from("decision-call")),
                ObservationOutcome::Completed,
                "result recorded",
            ),
            "2026-05-31T00:00:05Z",
        )
        .context("append result observation")?;

    let reply = call_reply_from_evaluation(
        ReplyId::from("reply-call"),
        &call,
        Some(ResultRef::from("result-call-1")),
        ObservationId::from("obs-call-reply"),
    );
    ledger
        .append_before_effect(
            observation(
                "obs-call-reply",
                ObservationKind::PeerCallReplied,
                trace_id.clone(),
                Some(result.call_id.clone()),
                Some(call.decision_id.clone()),
                ObservationOutcome::Completed,
                "peer call replied",
            ),
            "2026-05-31T00:00:06Z",
        )
        .context("append call reply observation")?;

    let trace_observation_count = ledger.by_trace(&trace_id)?.len();
    Ok(FakeEchoReport {
        hello,
        call,
        result,
        reply,
        trace_observation_count,
    })
}

#[cfg(test)]
fn fake_binding() -> MctPeerBinding {
    MctPeerBinding {
        binding_id: PeerBindingId::from("binding-fake"),
        iroh_endpoint_id: EndpointIdText::from("endpoint-b"),
        scope: MctPeerBindingScope {
            mct_node_id: MctNodeId::from("mother-b"),
            vision_id: VisionId::from("vision-a"),
            allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            data_scope: None,
            observation_scope: None,
        },
        issuer_node_id: MctNodeId::from("mother-a"),
        policy_revision: 1,
        binding_state: BindingState::Admitted,
        issued_at: Timestamp::from("2026-05-31T00:00:00Z"),
        expires_at: None,
        created_by_observation_id: ObservationId::from("obs-binding"),
        superseded_by_observation_id: None,
    }
}

#[cfg(test)]
fn fake_hello_request(trace_id: &TraceId) -> MctHelloRequest {
    MctHelloRequest {
        hello_id: "hello-fake".into(),
        received_over: IrohConnectionPresentation {
            endpoint_id: EndpointIdText::from("endpoint-b"),
            alpn: MCT_HELLO_ALPN.into(),
            connection_side: ConnectionSide::Incoming,
            path_class: PathClass::Direct,
            relay_url: None,
            presented_capability_ref: None,
        },
        requested_protocol: HelloPolicy::default().protocol,
        requested_vision_id: Some(VisionId::from("vision-a")),
        requested_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
        presented_binding: MctPeerBindingPresentation {
            binding_id: Some(PeerBindingId::from("binding-fake")),
            endpoint_id: EndpointIdText::from("endpoint-b"),
            mct_node_id: Some(MctNodeId::from("mother-b")),
            vision_id: Some(VisionId::from("vision-a")),
            policy_revision: Some(1),
            allowed_alpns_claim: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            signature_ref: None,
            expires_at: None,
        },
        capability_view: None,
        local_policy_revision_seen: Some(1),
        trace_id: trace_id.clone(),
        received_observation_id: ObservationId::from("obs-hello-received"),
    }
}

#[cfg(test)]
fn fake_call_request(
    trace_id: &TraceId,
    hello: &MctHelloAdmissionEvaluation,
) -> MctCallProtocolRequest {
    let call = MctCall {
        call_id: CallId::from("call-fake-echo"),
        caller: CallerIdentity {
            node_id: MctNodeId::from("mother-b"),
            user_id: None,
            vision_id: VisionId::from("vision-a"),
            project_id: None,
        },
        target: OperationTarget {
            namespace: "patina".into(),
            interface_name: "echo".into(),
            function_name: "echo".into(),
        },
        payload_metadata: PayloadMetadata {
            data_classification: "public".into(),
            approximate_size_bytes: 5,
            contains_secret_scoped_material: false,
        },
        authority_context: AuthorityContextSnapshot {
            policy_revision: 1,
            grants_revision: 1,
            vision_policy_revision: 1,
        },
        deadline: Timestamp::from("2026-05-31T00:01:00Z"),
        trace_context: TraceContext {
            trace_id: trace_id.clone(),
            span_id: SpanId::from("span-call"),
        },
        origin: CallOrigin::Iroh,
    };

    MctCallProtocolRequest {
        protocol_request_id: ProtocolRequestId::from("proto-call-fake"),
        authority: MctCallProtocolAuthority {
            hello_decision_id: hello.decision_id.clone(),
            peer_binding_id: PeerBindingId::from("binding-fake"),
            vision_id: VisionId::from("vision-a"),
            accepted_alpn: MCT_CALL_ALPN.into(),
            endpoint_id: EndpointIdText::from("endpoint-b"),
            policy_revision: 1,
            grants_revision: 1,
        },
        received_over: IrohConnectionPresentation {
            endpoint_id: EndpointIdText::from("endpoint-b"),
            alpn: MCT_CALL_ALPN.into(),
            connection_side: ConnectionSide::Incoming,
            path_class: PathClass::Direct,
            relay_url: None,
            presented_capability_ref: None,
        },
        call,
        payload: MctCallPayloadHandle {
            payload_kind: PayloadKind::InlinePayload,
            content_type: Some("text/plain".into()),
            approximate_size_bytes: 5,
            digest: None,
            blob_ref: None,
            external_ref: None,
            inline_payload_ref: Some("payload-hello".into()),
        },
        idempotency_key: Some("idem-fake".into()),
        received_observation_id: ObservationId::from("obs-peer-call-received"),
    }
}

#[cfg(test)]
fn fake_echo_result(call: &MctCall) -> MctResult {
    MctResult {
        call_id: call.call_id.clone(),
        outcome: ResultOutcome::Success,
        route_taken: Some(RouteTaken {
            node_id: MctNodeId::from("mother-a"),
            child_id: None,
            runtime_kind: RuntimeKind::Internal,
        }),
        authority_decision_ref: DecisionId::from("decision-call"),
        execution_summary: ExecutionSummary {
            wall_time_ms: 1,
            execution_time_ms: Some(1),
            queue_wait_ms: Some(0),
            input_size_bytes: call.payload_metadata.approximate_size_bytes,
            output_size_bytes: Some(call.payload_metadata.approximate_size_bytes),
        },
        requester_message: "echo ok".into(),
        audit_ref: AuditRef::from("audit-call-fake"),
    }
}

#[cfg(test)]
fn observation(
    id: &str,
    kind: ObservationKind,
    trace_id: TraceId,
    call_id: Option<CallId>,
    decision_id: Option<DecisionId>,
    outcome: ObservationOutcome,
    safe_message: &str,
) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::from(id),
        observed_at: Timestamp::from("2026-05-31T00:00:00Z"),
        kind,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id,
        decision_id,
        subject_id: None,
        resource_id: None,
        policy_revision: Some(1),
        grants_revision: Some(1),
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: None,
    }
}

/// Returns the crate version for health and smoke tests.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mct_iroh::{MotherIrohEndpointLifecycle, MotherIrohRelayMode};
    use mct_observation::JsonlObservationLedger;

    fn iroh_snapshot(lifecycle: MotherIrohEndpointLifecycle) -> MotherIrohEndpointSnapshot {
        MotherIrohEndpointSnapshot {
            endpoint_id: EndpointIdText::from("endpoint-daemon"),
            lifecycle,
            accepted_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            direct_addresses: vec!["127.0.0.1:0".into()],
            relay_urls: Vec::new(),
            relay_mode: MotherIrohRelayMode::Disabled,
        }
    }

    #[test]
    fn exposes_version() {
        assert_eq!(super::version(), "0.1.0");
    }

    #[test]
    fn daemon_reports_health_and_readiness() {
        let ready = daemon_status(Some(iroh_snapshot(MotherIrohEndpointLifecycle::Bound)));
        assert_eq!(ready.version, "0.1.0");
        assert_eq!(ready.health, MctDaemonHealth::Healthy);
        assert_eq!(ready.readiness, MctDaemonReadiness::Ready);
        assert_eq!(ready.safe_message, "ready");

        let closed = daemon_status(Some(iroh_snapshot(MotherIrohEndpointLifecycle::Closed)));
        assert_eq!(closed.readiness, MctDaemonReadiness::NotReady);
        assert_eq!(closed.safe_message, "iroh endpoint not ready");

        let missing = daemon_status(None);
        assert_eq!(missing.readiness, MctDaemonReadiness::NotReady);
        assert!(missing.iroh_endpoint.is_none());
    }

    #[test]
    fn fake_end_to_end_status_reports_runtime_spine() {
        let dir = tempfile::tempdir().unwrap();
        let ledger_path = dir.path().join("observations.jsonl");
        let report = run_fake_end_to_end_status_slice(
            &ledger_path,
            iroh_snapshot(MotherIrohEndpointLifecycle::Bound),
        )
        .unwrap();

        assert_eq!(report.daemon.readiness, MctDaemonReadiness::Ready);
        assert_eq!(report.echo.result.outcome, ResultOutcome::Success);
        assert_eq!(report.echo.trace_observation_count, 6);
        assert_eq!(report.call_observation_count, 4);
    }

    #[test]
    fn fake_echo_slice_records_trace_and_result() {
        let dir = tempfile::tempdir().unwrap();
        let ledger_path = dir.path().join("observations.jsonl");
        let report = run_fake_echo_slice(&ledger_path).unwrap();
        assert!(report.hello.is_admitted());
        assert!(report.call.is_accepted_for_routing());
        assert_eq!(report.result.outcome, ResultOutcome::Success);
        assert_eq!(
            report.reply.reply_outcome,
            CallProtocolReplyOutcome::Success
        );
        assert_eq!(report.trace_observation_count, 6);

        let ledger = JsonlObservationLedger::open(&ledger_path, "ledger-dev", "mother-a").unwrap();
        let call_entries = ledger.by_call(&CallId::from("call-fake-echo")).unwrap();
        assert_eq!(call_entries.len(), 4);
    }
}

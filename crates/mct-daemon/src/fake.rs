use crate::status::{MctDaemonStatus, daemon_status};
use anyhow::{Context, Result};
use mct_iroh::MotherIrohEndpointSnapshot;
use mct_kernel::*;
use mct_observation::JsonlObservationLedger;
use std::path::Path;

pub(crate) struct FakeEchoReport {
    pub(crate) hello: MctHelloAdmissionEvaluation,
    pub(crate) call: MctCallProtocolEvaluation,
    pub(crate) result: MctResult,
    pub(crate) reply: MctCallProtocolReply,
    pub(crate) trace_observation_count: usize,
    pub(crate) call_observation_count: usize,
}

pub(crate) struct FakeEndToEndStatusReport {
    pub(crate) daemon: MctDaemonStatus,
    pub(crate) echo: FakeEchoReport,
    pub(crate) call_observation_count: usize,
}

pub(crate) fn run_fake_end_to_end_status_slice(
    ledger_path: impl AsRef<Path>,
    iroh_endpoint: MotherIrohEndpointSnapshot,
) -> Result<FakeEndToEndStatusReport> {
    let ledger_path = ledger_path.as_ref();
    let echo = run_fake_echo_slice(ledger_path)?;
    let call_observation_count = echo.call_observation_count;

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
pub(crate) fn run_fake_echo_slice(ledger_path: impl AsRef<Path>) -> Result<FakeEchoReport> {
    let mut ledger = JsonlObservationLedger::open(ledger_path, "ledger-dev", "mother-a")
        .context("open fake echo observation ledger")?;

    let trace_id = TraceId::new("trace-fake-echo")
        .expect("string ID literal/generated value must be non-empty");
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
        HelloEvaluationContext {
            ids: EvaluationIds {
                decision_id: DecisionId::new("decision-hello")
                    .expect("string ID literal/generated value must be non-empty"),
                observation_id: ObservationId::new("obs-hello-decision")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            now: Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
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
            decision_id: DecisionId::new("decision-call")
                .expect("string ID literal/generated value must be non-empty"),
            observation_id: ObservationId::new("obs-call-decision")
                .expect("string ID literal/generated value must be non-empty"),
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
                Some(
                    DecisionId::new("decision-call")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                ObservationOutcome::Completed,
                "result recorded",
            ),
            "2026-05-31T00:00:05Z",
        )
        .context("append result observation")?;

    let reply = call_reply_from_evaluation(
        ReplyId::new("reply-call").expect("string ID literal/generated value must be non-empty"),
        &call,
        Some(
            ResultRef::new("result-call-1")
                .expect("string ID literal/generated value must be non-empty"),
        ),
        ObservationId::new("obs-call-reply")
            .expect("string ID literal/generated value must be non-empty"),
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
    let call_observation_count = ledger
        .by_call(
            &CallId::new("call-fake-echo")
                .expect("string ID literal/generated value must be non-empty"),
        )?
        .len();
    Ok(FakeEchoReport {
        hello,
        call,
        result,
        reply,
        trace_observation_count,
        call_observation_count,
    })
}

fn fake_binding() -> MctPeerBinding {
    MctPeerBinding {
        binding_id: PeerBindingId::new("binding-fake")
            .expect("string ID literal/generated value must be non-empty"),
        iroh_endpoint_id: EndpointIdText::new("endpoint-b")
            .expect("string ID literal/generated value must be non-empty"),
        scope: MctPeerBindingScope {
            mct_node_id: MctNodeId::new("mother-b")
                .expect("string ID literal/generated value must be non-empty"),
            vision_id: VisionId::new("vision-a")
                .expect("string ID literal/generated value must be non-empty"),
            allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            data_scope: None,
            observation_scope: None,
        },
        issuer_node_id: MctNodeId::new("mother-a")
            .expect("string ID literal/generated value must be non-empty"),
        policy_revision: 1,
        binding_state: BindingState::Admitted,
        issued_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
        expires_at: None,
        created_by_observation_id: ObservationId::new("obs-binding")
            .expect("string ID literal/generated value must be non-empty"),
        superseded_by_observation_id: None,
    }
}

fn fake_hello_request(trace_id: &TraceId) -> MctHelloRequest {
    MctHelloRequest {
        hello_id: "hello-fake".into(),
        received_over: IrohConnectionPresentation {
            endpoint_id: EndpointIdText::new("endpoint-b")
                .expect("string ID literal/generated value must be non-empty"),
            alpn: MCT_HELLO_ALPN.into(),
            connection_side: ConnectionSide::Incoming,
            path_class: PathClass::Direct,
            relay_url: None,
            presented_capability_ref: None,
        },
        requested_protocol: HelloPolicy::default().protocol,
        requested_vision_id: Some(
            VisionId::new("vision-a").expect("string ID literal/generated value must be non-empty"),
        ),
        requested_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
        presented_binding: MctPeerBindingPresentation {
            binding_id: Some(
                PeerBindingId::new("binding-fake")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            endpoint_id: EndpointIdText::new("endpoint-b")
                .expect("string ID literal/generated value must be non-empty"),
            mct_node_id: Some(
                MctNodeId::new("mother-b")
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
        trace_id: trace_id.clone(),
        received_observation_id: ObservationId::new("obs-hello-received")
            .expect("string ID literal/generated value must be non-empty"),
    }
}

fn fake_call_request(
    trace_id: &TraceId,
    hello: &MctHelloAdmissionEvaluation,
) -> MctCallProtocolRequest {
    let call = MctCall {
        call_id: CallId::new("call-fake-echo")
            .expect("string ID literal/generated value must be non-empty"),
        caller: CallerIdentity {
            node_id: MctNodeId::new("mother-b")
                .expect("string ID literal/generated value must be non-empty"),
            user_id: None,
            vision_id: VisionId::new("vision-a")
                .expect("string ID literal/generated value must be non-empty"),
            project_id: None,
        },
        target: OperationTarget {
            namespace: "patina".into(),
            interface_name: "echo".into(),
            function_name: "echo".into(),
        },
        payload_metadata: PayloadMetadata {
            data_classification: "public".into(),
            approximate_size_bytes: 0,
            contains_secret_scoped_material: false,
        },
        authority_context: AuthorityContextSnapshot {
            policy_revision: 1,
            grants_revision: 1,
            vision_policy_revision: 1,
        },
        deadline: Timestamp::new("2026-05-31T00:01:00Z").unwrap(),
        trace_context: TraceContext {
            trace_id: trace_id.clone(),
            span_id: SpanId::new("span-call")
                .expect("string ID literal/generated value must be non-empty"),
        },
        origin: CallOrigin::Iroh,
    };

    MctCallProtocolRequest {
        protocol_request_id: ProtocolRequestId::new("proto-call-fake")
            .expect("string ID literal/generated value must be non-empty"),
        authority: MctCallProtocolAuthority {
            hello_decision_id: hello.decision_id.clone(),
            peer_binding_id: PeerBindingId::new("binding-fake")
                .expect("string ID literal/generated value must be non-empty"),
            vision_id: VisionId::new("vision-a")
                .expect("string ID literal/generated value must be non-empty"),
            accepted_alpn: MCT_CALL_ALPN.into(),
            endpoint_id: EndpointIdText::new("endpoint-b")
                .expect("string ID literal/generated value must be non-empty"),
            policy_revision: 1,
            grants_revision: 1,
        },
        received_over: IrohConnectionPresentation {
            endpoint_id: EndpointIdText::new("endpoint-b")
                .expect("string ID literal/generated value must be non-empty"),
            alpn: MCT_CALL_ALPN.into(),
            connection_side: ConnectionSide::Incoming,
            path_class: PathClass::Direct,
            relay_url: None,
            presented_capability_ref: None,
        },
        call,
        payload: MctCallPayloadHandle::Empty,
        idempotency_key: Some("idem-fake".into()),
        received_observation_id: ObservationId::new("obs-peer-call-received")
            .expect("string ID literal/generated value must be non-empty"),
    }
}

fn fake_echo_result(call: &MctCall) -> MctResult {
    MctResult {
        call_id: call.call_id.clone(),
        outcome: ResultOutcome::Success,
        route_taken: Some(RouteTaken {
            node_id: MctNodeId::new("mother-a")
                .expect("string ID literal/generated value must be non-empty"),
            child_id: None,
            runtime_kind: RuntimeKind::Internal,
        }),
        authority_decision_ref: DecisionId::new("decision-call")
            .expect("string ID literal/generated value must be non-empty"),
        execution_summary: ExecutionSummary {
            wall_time_ms: 1,
            execution_time_ms: Some(1),
            queue_wait_ms: Some(0),
            input_size_bytes: call.payload_metadata.approximate_size_bytes,
            output_size_bytes: Some(call.payload_metadata.approximate_size_bytes),
        },
        result_payload: MctCallPayloadHandle::Empty,
        requester_message: "echo ok".into(),
        audit_ref: AuditRef::new("audit-call-fake")
            .expect("string ID literal/generated value must be non-empty"),
    }
}

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
        observation_id: ObservationId::new(id)
            .expect("string ID literal/generated value must be non-empty"),
        observed_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
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

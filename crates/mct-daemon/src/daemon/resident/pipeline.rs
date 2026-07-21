//! Exact resident call-stage sequencing, before-effect durability barriers, and handler mapping.
//!
//! Stage logic belongs to payload, idempotency, decision, execution, and forwarding; this module
//! only orders those stages and maps their completed outputs into the transport handler result.

use super::*;

fn watch_callout_observation(
    id: impl Into<String>,
    kind: ObservationKind,
    outcome: ObservationOutcome,
    parent: &MctCall,
    refs: (Option<CallId>, String, String),
    safe_message: &str,
) -> MctObservation {
    let (call_id, subject_id, resource_id) = refs;
    MctObservation {
        observation_id: ObservationId::new(id.into()).expect("generated Watch observation id"),
        observed_at: current_timestamp(),
        kind,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: parent.trace_context.trace_id.clone(),
            span_id: Some(parent.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id,
        decision_id: None,
        subject_id: Some(subject_id),
        resource_id: Some(resource_id),
        policy_revision: Some(parent.authority_context.policy_revision),
        grants_revision: Some(parent.authority_context.grants_revision),
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: None,
    }
}

pub(super) fn existing_watch_subject_is_eligible(
    canonical_root: &Path,
    relative_path: &str,
) -> Result<bool> {
    validate_safe_watch_relative_path(relative_path)?;
    let canonical_root = canonical_root.canonicalize()?;
    let mut current = canonical_root.clone();
    for segment in relative_path.split('/') {
        current.push(segment);
        let metadata = match std::fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            return Ok(false);
        }
    }
    let metadata = std::fs::metadata(&current)?;
    if !metadata.is_file() {
        return Ok(false);
    }
    Ok(current.canonicalize()?.starts_with(canonical_root))
}

async fn execute_watch_callouts(
    paths: &ResidentRuntimePaths,
    ledger: &ResidentLedgerWriter,
    parent: &MctCall,
    parent_route: Option<&RouteTaken>,
    context: &ResidentCallIngressContext,
    messages: Vec<MctWitProducedMessage>,
) -> Result<()> {
    if messages.is_empty() {
        return Ok(());
    }
    let depth = match context {
        ResidentCallIngressContext::ChildCallOut { depth, .. } => *depth,
        _ => 0,
    };
    if depth >= MCT_CHILD_CALLOUT_MAX_DEPTH {
        bail!("nested Child call-out depth exhausted");
    }
    if messages.len() > MCT_WATCH_MAX_EVENTS_PER_BATCH as usize {
        bail!("Watch event batch capacity exceeded");
    }
    let child_name = parent_route
        .and_then(|route| route.child_id.as_ref())
        .map(ChildId::as_str)
        .map(|id| id.strip_prefix("child:").unwrap_or(id))
        .unwrap_or("folder-watch-actor");
    let state = MctRuntimeStateStore::open(paths.state_path())?;
    let scope = state
        .watch_observation_scopes()?
        .into_iter()
        .filter(|scope| {
            scope.authority_state == WatchObservationScopeState::Active
                && scope.observer_ref.child_name == child_name
                && scope.is_current_at(&current_timestamp())
        })
        .max_by_key(|scope| scope.scope_revision)
        .context("current Watch scope missing for Child call-out")?;
    let mut parsed = messages
        .into_iter()
        .map(|message| {
            if message.data.len() > MCT_WATCH_MESSAGE_MAX_BYTES
                || message.metadata.len() > MCT_WATCH_METADATA_PAIRS_MAX
                || message.content_type.as_deref() != Some("application/json")
            {
                bail!("Watch call-out message violates a named bound");
            }
            let wire: MctWitWatchCallOutWireEvent =
                serde_json::from_slice(&message.data).context("decode exact watcher event JSON")?;
            let class = match (message.topic.as_str(), wire.change_kind.as_str()) {
                ("file-created", "created") => WatchEventClass::Created,
                ("file-modified", "modified") => WatchEventClass::Modified,
                ("file-deleted", "deleted") => WatchEventClass::Deleted,
                _ => bail!("Watch topic and event class mismatch"),
            };
            if !scope.event_classes.contains(&class) {
                bail!("Watch event class is outside current scope");
            }
            let compatibility = validate_legacy_watch_paths(
                message
                    .target_operation
                    .rsplit_once('.')
                    .map(|(interface, _)| interface)
                    .unwrap_or_default(),
                &wire.absolute_path,
                &wire.relative_path,
            )?;
            if compatibility != LegacyWatchCompatibilityValidation::Matched {
                bail!("legacy Watch path equality refused");
            }
            let canonical_root = Path::new(
                scope
                    .canonical_root_ref
                    .strip_prefix("file://")
                    .context("Watch scope root is not a local file URI")?,
            );
            match class {
                WatchEventClass::Created | WatchEventClass::Modified => {
                    if !existing_watch_subject_is_eligible(canonical_root, &wire.relative_path)? {
                        bail!("Watch subject is absent, escaped, symlinked, or special");
                    }
                }
                WatchEventClass::Deleted => {
                    if !state.watch_subject_was_present(
                        &scope.watch_scope_id,
                        scope.scope_revision,
                        &wire.relative_path,
                    )? {
                        bail!("deleted Watch subject has no prior in-scope identity");
                    }
                }
            }
            let canonical = serde_json::to_string(&wire)?;
            Ok((wire.relative_path.clone(), class, canonical, wire, message))
        })
        .collect::<Result<Vec<_>>>()?;
    parsed.sort_by(|left, right| {
        (&left.0, format!("{:?}", left.1), &left.2).cmp(&(
            &right.0,
            format!("{:?}", right.1),
            &right.2,
        ))
    });
    if parsed.len() > scope.max_events_per_batch as usize {
        bail!("Watch scope batch capacity exceeded");
    }
    let sequence =
        state.reserve_watch_batch_sequence(&scope.watch_scope_id, scope.scope_revision)?;
    let batch_id = derive_watch_batch_id(
        &scope.watch_scope_id,
        scope.scope_revision,
        sequence,
        &parent.call_id,
    );
    let batch = WatchEventBatchEvidence {
        batch_id: batch_id.clone(),
        watch_scope_id: scope.watch_scope_id.clone(),
        scope_revision: scope.scope_revision,
        sequence,
        parent_call_id: parent.call_id.clone(),
        raw_event_count: parsed.len() as u32,
        eligible_event_count: parsed.len() as u32,
        coalesced_event_count: 0,
        excluded_event_count: 0,
        capacity_refused_event_count: 0,
    };
    let parent_firing_id = match context {
        ResidentCallIngressContext::Trigger { firing_id, .. } => Some(firing_id.clone()),
        ResidentCallIngressContext::ChildCallOut {
            parent_firing_id, ..
        } => parent_firing_id.clone(),
        _ => None,
    };
    let mut events = Vec::new();
    let mut dispositions = Vec::new();
    let mut plan_observations = vec![watch_callout_observation(
        format!("obs:watch-batch:{batch_id}"),
        ObservationKind::AdapterEffectStarted,
        ObservationOutcome::Started,
        parent,
        (
            Some(parent.call_id.clone()),
            scope.watch_scope_id.to_string(),
            batch_id.to_string(),
        ),
        "Watch batch opened",
    )];
    for (position, (_, class, canonical, wire, message)) in parsed.iter().enumerate() {
        let event_id =
            derive_watch_callout_event_id(&parent.call_id, &batch_id, position as u32, canonical);
        let call_id = derive_watch_callout_call_id(&event_id);
        let disposition_id =
            WatchEventDeliveryDispositionId::new(format!("watch-disposition:{}", event_id))?;
        let disposition_observation_id =
            ObservationId::new(format!("obs:watch-disposition:{}", event_id))?;
        events.push(WatchEventEvidence {
            event_id: event_id.clone(),
            batch_id: batch_id.clone(),
            batch_position: position as u32,
            event_class: *class,
            relative_path: wire.relative_path.clone(),
            causative_call_id: parent.call_id.clone(),
            causative_trigger_firing_id: parent_firing_id.clone(),
            causative_adapter_observation_id: None,
        });
        dispositions.push(WatchEventDeliveryDisposition {
            disposition_id: disposition_id.clone(),
            event_id: event_id.clone(),
            disposition: WatchEventDisposition::Fired,
            planned_call_id: Some(call_id.clone()),
            compatibility_validation: LegacyWatchCompatibilityValidation::Matched,
            disposition_observation_id: disposition_observation_id.clone(),
        });
        plan_observations.push(watch_callout_observation(
            format!("obs:watch-event:{event_id}"),
            ObservationKind::DataMovementAllowed,
            ObservationOutcome::Allowed,
            parent,
            (
                Some(parent.call_id.clone()),
                event_id.to_string(),
                wire.relative_path.clone(),
            ),
            "Watch event eligible",
        ));
        plan_observations.push(watch_callout_observation(
            disposition_observation_id.to_string(),
            ObservationKind::CallConstructed,
            ObservationOutcome::Allowed,
            parent,
            (
                Some(call_id),
                event_id.to_string(),
                message.target_operation.clone(),
            ),
            "Child call-out constructed",
        ));
    }
    ledger.append(plan_observations).await?;
    state.insert_watch_event_plan(&batch, &events, &dispositions)?;
    for event in &events {
        state.record_watch_subject_presence(
            &scope.watch_scope_id,
            scope.scope_revision,
            &event.event_id,
            &event.relative_path,
            event.event_class != WatchEventClass::Deleted,
        )?;
    }

    for (((_, _, _, wire, message), event), disposition) in
        parsed.into_iter().zip(events).zip(dispositions)
    {
        let target = operation_target_from_wit_operation_id(&message.target_operation)?;
        let call_id = disposition
            .planned_call_id
            .clone()
            .context("fired disposition missing call id")?;
        let target_payload = serde_json::to_vec(&serde_json::json!([{
            "watcher": wire.watcher,
            "stream-name": wire.stream,
            "change-kind": wire.change_kind,
            "absolute-path": wire.absolute_path,
            "relative-path": wire.relative_path,
            "size-bytes": wire.size_bytes,
            "modified-unix-ms": wire.modified_unix_ms,
            "sha256": wire.sha256,
            "detected-at": wire.detected_at,
        }]))?;
        let endpoint_id = parent.caller.node_id.to_string();
        let endpoint_id = EndpointIdText::new(endpoint_id)?;
        let request = MctCallProtocolRequest {
            authority: MctCallProtocolAuthority {
                hello_decision_id: DecisionId::new(format!(
                    "decision:wasm-host:{event_id}",
                    event_id = event.event_id
                ))?,
                peer_binding_id: PeerBindingId::new(format!(
                    "binding:wasm-host:{}",
                    parent.call_id
                ))?,
                vision_id: parent.caller.vision_id.clone(),
                accepted_alpn: "mct/wasm-host-call/0".into(),
                endpoint_id: endpoint_id.clone(),
                policy_revision: parent.authority_context.policy_revision,
                grants_revision: parent.authority_context.grants_revision,
            },
            received_over: IrohConnectionPresentation {
                endpoint_id,
                alpn: "mct/wasm-host-call/0".into(),
                connection_side: ConnectionSide::Incoming,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            call: MctCall {
                call_id: call_id.clone(),
                caller: parent.caller.clone(),
                target,
                payload_metadata: PayloadMetadata {
                    data_classification: parent.payload_metadata.data_classification.clone(),
                    size_bytes: target_payload.len() as u64,
                    contains_secret_scoped_material: false,
                },
                authority_context: parent.authority_context.clone(),
                deadline: parent.deadline.clone(),
                trace_context: TraceContext {
                    trace_id: parent.trace_context.trace_id.clone(),
                    span_id: SpanId::new(format!("span:wasm-host:{}", event.event_id))?,
                },
                origin: CallOrigin::WasmHost,
            },
            payload: MctCallPayloadHandle::InlinePayload {
                content_type: "application/json".into(),
                size_bytes: target_payload.len() as u64,
                blake3_digest_hex: blake3_hex(&target_payload),
                inline_payload_ref: format!("inline:wasm-host:{}", event.event_id),
            },
            idempotency_key: Some(derive_watch_callout_idempotency_key(
                &parent.call_id,
                &event.event_id,
                &message.target_operation,
            )),
            received_observation_id: ObservationId::new(format!(
                "obs:wasm-host-received:{}",
                event.event_id
            ))?,
            protocol_request_id: ProtocolRequestId::new(format!(
                "protocol:wasm-host:{}",
                event.event_id
            ))?,
        };
        let nested = Box::pin(execute_resident_call_at_with_context(
            paths.clone(),
            ledger.clone(),
            request,
            ResidentPayloadIngress::local(Some(target_payload)),
            current_timestamp(),
            ResidentCallIngressContext::ChildCallOut {
                parent_call_id: parent.call_id.clone(),
                parent_firing_id: parent_firing_id.clone(),
                depth: depth + 1,
            },
        ))
        .await;
        let result_ref = nested.result_ref.clone().unwrap_or_else(|| {
            ResultRef::new(format!("result-resident:{call_id}")).expect("generated result ref")
        });
        let delivered = nested.outcome == CallProtocolOutcome::Completed;
        let delivery = WatchEventDeliveryEvidence {
            delivery_id: WatchEventDeliveryId::new(format!("watch-delivery:{}", event.event_id))?,
            disposition_id: disposition.disposition_id,
            target_call_id: call_id.clone(),
            target_result_ref: result_ref,
            target_result_observation_id: ObservationId::new(format!(
                "obs:result-resident:{call_id}"
            ))?,
            delivered,
        };
        ledger
            .append(vec![watch_callout_observation(
                format!("obs:watch-delivery:{}", event.event_id),
                ObservationKind::AdapterEffectCompleted,
                if delivered {
                    ObservationOutcome::Completed
                } else {
                    ObservationOutcome::Failed
                },
                parent,
                (
                    Some(call_id),
                    event.event_id.to_string(),
                    delivery.delivery_id.to_string(),
                ),
                "Watch delivery completed",
            )])
            .await?;
        state.insert_watch_delivery_evidence(&delivery)?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResidentCallIngressContext {
    Peer {
        binding_id: PeerBindingId,
    },
    LocalPrincipal {
        origin: CallOrigin,
        caller: CallerIdentity,
    },
    Trigger {
        trigger_authority_id: CallTriggerAuthorityId,
        record_revision: u64,
        firing_id: CallTriggerFiringId,
        occurrence_id: CallTriggerOccurrenceId,
    },
    #[allow(dead_code)] // Ratified D1B interface; implemented only after Part A lands.
    ChildCallOut {
        parent_call_id: CallId,
        parent_firing_id: Option<CallTriggerFiringId>,
        depth: u8,
    },
}

impl ResidentCallIngressContext {
    pub(super) fn ordinary(request: &MctCallProtocolRequest) -> Option<Self> {
        match request.call.origin {
            CallOrigin::Iroh => Some(Self::Peer {
                binding_id: request.authority.peer_binding_id.clone(),
            }),
            CallOrigin::TriggerFiring => None,
            origin => Some(Self::LocalPrincipal {
                origin,
                caller: request.call.caller.clone(),
            }),
        }
    }

    fn matches_request(&self, request: &MctCallProtocolRequest) -> bool {
        match self {
            Self::Peer { binding_id } => {
                request.call.origin == CallOrigin::Iroh
                    && binding_id == &request.authority.peer_binding_id
            }
            Self::LocalPrincipal { origin, caller } => {
                *origin == request.call.origin
                    && *origin != CallOrigin::Iroh
                    && *origin != CallOrigin::TriggerFiring
                    && caller == &request.call.caller
            }
            Self::Trigger { .. } => request.call.origin == CallOrigin::TriggerFiring,
            Self::ChildCallOut { depth, .. } => {
                request.call.origin == CallOrigin::WasmHost && *depth > 0
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ResidentRuntimePaths {
    config_path: PathBuf,
    children_dir: PathBuf,
    state_path: PathBuf,
}

impl ResidentRuntimePaths {
    pub(crate) fn new(config_path: PathBuf, children_dir: PathBuf, state_path: PathBuf) -> Self {
        Self {
            config_path,
            children_dir,
            state_path,
        }
    }

    pub(crate) fn config_path(&self) -> &Path {
        &self.config_path
    }
    pub(crate) fn children_dir(&self) -> &Path {
        &self.children_dir
    }
    pub(crate) fn state_path(&self) -> &Path {
        &self.state_path
    }
}

pub(crate) async fn execute_resident_call(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    payload: ResidentPayloadIngress,
) -> MctIrohCallHandlerResult {
    let Some(context) = ResidentCallIngressContext::ordinary(&request) else {
        return MctIrohCallHandlerResult::denied();
    };
    execute_resident_call_with_context(paths, ledger, request, payload, context).await
}

pub(crate) async fn execute_resident_call_with_context(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    payload: ResidentPayloadIngress,
    context: ResidentCallIngressContext,
) -> MctIrohCallHandlerResult {
    execute_resident_call_at_with_context(
        paths,
        ledger,
        request,
        payload,
        current_timestamp(),
        context,
    )
    .await
}

#[cfg(test)]
pub(super) async fn execute_resident_call_at(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    payload: ResidentPayloadIngress,
    now: Timestamp,
) -> MctIrohCallHandlerResult {
    let Some(context) = ResidentCallIngressContext::ordinary(&request) else {
        return MctIrohCallHandlerResult::denied();
    };
    execute_resident_call_at_with_context(paths, ledger, request, payload, now, context).await
}

async fn execute_resident_call_at_with_context(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    payload: ResidentPayloadIngress,
    now: Timestamp,
    context: ResidentCallIngressContext,
) -> MctIrohCallHandlerResult {
    if !context.matches_request(&request) {
        return MctIrohCallHandlerResult::denied();
    }
    let inline_payload = match resolve_resident_request_payload(&paths, &request, payload).await {
        Ok(payload) => payload.into_inner(),
        Err(report) => {
            let (safe_message, observations) = report.into_parts();
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident payload failure ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            return MctIrohCallHandlerResult::failed(safe_message);
        }
    };

    let state_path = paths.state_path().to_path_buf();
    let idempotency_request = request.clone();
    let idempotency_ledger = ledger.clone();
    execute_idempotent_call_with_context(
        state_path,
        idempotency_ledger,
        idempotency_request,
        now,
        context.clone(),
        move || {
            execute_resident_call_after_payload(paths, ledger, request, inline_payload, context)
        },
    )
    .await
}

async fn execute_resident_call_after_payload(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    inline_payload: Option<Vec<u8>>,
    context: ResidentCallIngressContext,
) -> MctIrohCallHandlerResult {
    let authorization = match authorize_resident_child(paths.clone(), request.call.clone()).await {
        Ok(authorization) => authorization,
        Err(error) => {
            eprintln!("resident child authorization unavailable: {error}");
            return MctIrohCallHandlerResult::failed("runtime unavailable");
        }
    };

    match authorization {
        RouteDisposition::Denied {
            decision,
            mut observations,
        } => {
            let result = no_route_denied_result(
                &request.call,
                &decision,
                AuditRef::new(format!("audit:denied:{}", request.call.call_id))
                    .expect("generated denied audit ref must be non-empty"),
            );
            let result_ref = ResultRef::new(format!("result-resident:{}", request.call.call_id))
                .expect("generated denied result ref must be non-empty");
            observations.push(MctObservation {
                observation_id: ObservationId::new(format!(
                    "obs:result-resident:{}",
                    request.call.call_id
                ))
                .expect("generated denied result observation id must be non-empty"),
                observed_at: current_timestamp(),
                kind: ObservationKind::ResultRecorded,
                source_plane: SourcePlane::Kernel,
                trace: ObservationTraceRef {
                    trace_id: request.call.trace_context.trace_id.clone(),
                    span_id: Some(request.call.trace_context.span_id.clone()),
                    parent_span_id: None,
                    external_trace_id: None,
                },
                call_id: Some(request.call.call_id.clone()),
                decision_id: Some(decision.decision_id.clone()),
                subject_id: Some(request.call.caller.node_id.to_string()),
                resource_id: Some(result_ref.to_string()),
                policy_revision: Some(request.call.authority_context.policy_revision),
                grants_revision: Some(request.call.authority_context.grants_revision),
                outcome: ObservationOutcome::Denied,
                visibility: ObservationVisibility::InternalOnly,
                safe_message: "resident denied result recorded".into(),
                detail_ref: Some("result_outcome:denied".into()),
            });
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident route denial ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            let run_id = format!("run-denied:{}", request.call.call_id);
            let projection = MctRuntimeStateStore::open(paths.state_path()).and_then(|state| {
                if state.get_run(&run_id)?.is_none() {
                    state.insert_run_started(
                        &run_id,
                        &request.call,
                        RuntimeKind::Internal,
                        None,
                        current_timestamp_string(),
                    )?;
                }
                state.complete_run(&run_id, &result, current_timestamp_string())?;
                Ok(())
            });
            if let Err(error) = projection {
                eprintln!("resident denied result projection failed: {error}");
                return MctIrohCallHandlerResult::failed("runtime state unavailable");
            }
            MctIrohCallHandlerResult::denied().with_route(Some(decision.decision_id), None)
        }
        RouteDisposition::Local { plan, observations } => {
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident route ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }

            let current_revisions = match current_resident_route_revisions(&paths, &request.call) {
                Ok(revisions) => revisions,
                Err(error) => {
                    eprintln!("resident route revision read failed: {error}");
                    return MctIrohCallHandlerResult::failed("runtime unavailable");
                }
            };
            let result_state_path = paths.state_path().to_path_buf();
            let result_observation_call = request.call.clone();
            let before_effect_ledger = ledger.clone();
            let callout_paths = paths.clone();
            let execution = match tokio::task::spawn_blocking(move || {
                execute_authorized_resident_child(
                    paths,
                    *plan,
                    request,
                    inline_payload,
                    current_revisions,
                    Some(before_effect_ledger),
                )
            })
            .await
            {
                Ok(Ok(report)) => report,
                Ok(Err(error)) => {
                    eprintln!("resident child execution failed: {error}");
                    return MctIrohCallHandlerResult::failed("runtime execution failed");
                }
                Err(error) => {
                    eprintln!("resident child execution task failed: {error}");
                    return MctIrohCallHandlerResult::failed("runtime execution failed");
                }
            };

            let (result, mut observations, inline_result_payload, run_id, produced_messages) =
                execution.into_parts();
            if let Err(error) = execute_watch_callouts(
                &callout_paths,
                &ledger,
                &result_observation_call,
                result.route_taken.as_ref(),
                &context,
                produced_messages,
            )
            .await
            {
                eprintln!("resident Child call-out failed: {error}");
                return MctIrohCallHandlerResult::failed("Child call-out failed");
            }
            if matches!(
                result_observation_call.origin,
                CallOrigin::TriggerFiring | CallOrigin::WasmHost
            ) {
                observations.push(MctObservation {
                    observation_id: ObservationId::new(format!(
                        "obs:result-resident:{}",
                        result.call_id
                    ))
                    .expect("generated resident result observation id must be non-empty"),
                    observed_at: current_timestamp(),
                    kind: ObservationKind::ResultRecorded,
                    source_plane: SourcePlane::Kernel,
                    trace: ObservationTraceRef {
                        trace_id: result_observation_call.trace_context.trace_id.clone(),
                        span_id: Some(result_observation_call.trace_context.span_id.clone()),
                        parent_span_id: None,
                        external_trace_id: None,
                    },
                    call_id: Some(result.call_id.clone()),
                    decision_id: Some(result.authority_decision_ref.clone()),
                    subject_id: result_observation_call
                        .caller
                        .user_id
                        .as_ref()
                        .map(ToString::to_string)
                        .or_else(|| Some(result_observation_call.caller.node_id.to_string())),
                    resource_id: Some(format!("result-resident:{}", result.call_id)),
                    policy_revision: Some(
                        result_observation_call.authority_context.policy_revision,
                    ),
                    grants_revision: Some(
                        result_observation_call.authority_context.grants_revision,
                    ),
                    outcome: match result.outcome {
                        ResultOutcome::Success => ObservationOutcome::Completed,
                        ResultOutcome::Denied => ObservationOutcome::Denied,
                        ResultOutcome::Failed => ObservationOutcome::Failed,
                        ResultOutcome::TimedOut => ObservationOutcome::TimedOut,
                        ResultOutcome::Cancelled => ObservationOutcome::Cancelled,
                    },
                    visibility: ObservationVisibility::InternalOnly,
                    safe_message: "resident target result recorded".into(),
                    detail_ref: Some(format!("result_outcome:{:?}", result.outcome)),
                });
            }
            let state_observations = observations.clone();
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident execution ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            if let Some(run_id) = run_id {
                let persist_result =
                    MctRuntimeStateStore::open(&result_state_path).and_then(|state| {
                        state.append_run_observations(&run_id, &state_observations)?;
                        state.complete_run(&run_id, &result, mct_daemon::current_timestamp_string())
                    });
                if let Err(error) = persist_result {
                    eprintln!("resident durable result projection failed: {error}");
                    return MctIrohCallHandlerResult::failed("runtime state unavailable");
                }
            }

            result_to_call_handler_result("result-resident", &result, inline_result_payload)
        }
        RouteDisposition::Remote { plan, observations } => {
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident remote route ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            execute_authorized_resident_remote_call(paths, *plan, request, inline_payload, ledger)
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_resident_payload_process_child(children_dir: &Path) {
        write_resident_process_child_script(
            children_dir,
            "resident-payload-echo",
            b"#!/bin/sh\npayload=$(cat)\nprintf 'processed:%s' \"$payload\"\n",
        );
    }
    fn write_resident_process_child_script(children_dir: &Path, name: &str, script: &[u8]) {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        let child_dir = children_dir.join(name);
        std::fs::create_dir_all(&child_dir).unwrap();
        let artifact_path = child_dir.join(format!("{name}.wasm"));
        let manifest_path = child_dir.join("child.toml");
        std::fs::write(&artifact_path, script).unwrap();
        #[cfg(unix)]
        {
            let mut permissions = std::fs::metadata(&artifact_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&artifact_path, permissions).unwrap();
        }
        write_resident_child_manifest(&manifest_path, name, "handle");
        write_sha256_sidecar(&artifact_path, script);
        let manifest_bytes = std::fs::read(&manifest_path).unwrap();
        write_sha256_sidecar(&manifest_path, &manifest_bytes);
    }
    fn write_resident_child_manifest(manifest_path: &Path, name: &str, mode: &str) {
        std::fs::write(
            manifest_path,
            format!(
                r#"[child]
name = "{name}"
version = "0.1.0"
description = "resident test child"
kind = "child"
role = "app"

[child.ingress]
mode = "{mode}"

[child.artifact]
wasm = "{name}.wasm"

[child.contract]
allow = ["patina:demo/control@0.1.0.run"]

[needs]
toys = []

[relationships]
listens = []
"#
            ),
        )
        .unwrap();
    }
    fn write_sha256_sidecar(path: &Path, bytes: &[u8]) {
        use sha2::{Digest, Sha256};

        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(".sha256");
        std::fs::write(
            PathBuf::from(sidecar),
            format!("{:x}", Sha256::digest(bytes)),
        )
        .unwrap();
    }
    #[test]
    fn watch_adapter_excludes_escaped_symlinks_and_absolute_paths() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("safe.txt"), b"safe").unwrap();
        std::fs::write(outside.path().join("secret.txt"), b"secret").unwrap();
        assert!(existing_watch_subject_is_eligible(root.path(), "safe.txt").unwrap());
        assert!(existing_watch_subject_is_eligible(root.path(), "/absolute.txt").is_err());
        assert!(existing_watch_subject_is_eligible(root.path(), "../secret.txt").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                outside.path().join("secret.txt"),
                root.path().join("escaped.txt"),
            )
            .unwrap();
            assert!(!existing_watch_subject_is_eligible(root.path(), "escaped.txt").unwrap());
        }
    }

    #[tokio::test]
    async fn watch_admission_append_failure_suppresses_every_nested_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("watch-root");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("safe.txt"), b"safe").unwrap();
        let paths = ResidentRuntimePaths::new(
            dir.path().join("config.json"),
            dir.path().join("children"),
            dir.path().join("state.sqlite"),
        );
        let state = MctRuntimeStateStore::open(paths.state_path()).unwrap();
        let scope = WatchObservationScope {
            watch_scope_id: WatchObservationScopeId::new("scope-append-failure").unwrap(),
            observer_shape: WatchObserverShape::ChildToy,
            observer_ref: WatchObserverRef {
                child_name: "folder-watch-actor".into(),
                artifact_id: ComponentArtifactId::new(format!("sha256:{}", "a".repeat(64)))
                    .unwrap(),
                artifact_version: "0.1.0".into(),
                assignment_id: ChildAssignmentId::new("assignment:folder-watch-actor").unwrap(),
            },
            scope_mode: WatchScopeMode::Constrained,
            canonical_root_ref: format!("file://{}", root.canonicalize().unwrap().display()),
            traversal_scope: WatchTraversalScope::Recursive,
            event_classes: vec![WatchEventClass::Created],
            max_events_per_batch: 8,
            coalescing_policy: WatchCoalescingPolicy::None,
            starts_at: Timestamp::new("2026-01-01T00:00:00Z").unwrap(),
            expires_at: Timestamp::new("2099-01-01T00:00:00Z").unwrap(),
            scope_revision: 1,
            policy_revision: 1,
            authority_state: WatchObservationScopeState::Active,
            authority_observation_id: ObservationId::new("obs-scope-append-failure").unwrap(),
            canonical_record_digest: String::new(),
        }
        .seal();
        state.insert_watch_observation_scope(&scope).unwrap();
        let parent = MctCall {
            call_id: CallId::new("call-watch-append-failure").unwrap(),
            caller: CallerIdentity {
                node_id: MctNodeId::new("local-mct").unwrap(),
                vision_id: VisionId::new("vision-local").unwrap(),
                project_id: None,
                user_id: None,
            },
            target: OperationTarget::new("patina:watch", "control@0.1.0", "scan-now").unwrap(),
            payload_metadata: PayloadMetadata {
                data_classification: "public".into(),
                size_bytes: 2,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 1,
                grants_revision: 1,
                vision_policy_revision: 1,
            },
            deadline: Timestamp::new("2099-01-01T00:00:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-watch-append-failure").unwrap(),
                span_id: SpanId::new("span-watch-append-failure").unwrap(),
            },
            origin: CallOrigin::WasmHost,
        };
        let wire = serde_json::json!({
            "watcher": "folder-watch-actor",
            "stream": "source",
            "change_kind": "created",
            "absolute_path": "safe.txt",
            "relative_path": "safe.txt",
            "size_bytes": 4,
            "modified_unix_ms": 1,
            "sha256": "abc",
            "detected_at": "2026-07-21T00:00:00Z"
        });
        let route = RouteTaken {
            node_id: MctNodeId::new("local-mct").unwrap(),
            child_id: Some(ChildId::new("child:folder-watch-actor").unwrap()),
            runtime_kind: RuntimeKind::WasmComponent,
        };
        let result = execute_watch_callouts(
            &paths,
            &ResidentLedgerWriter::failed_for_test(),
            &parent,
            Some(&route),
            &ResidentCallIngressContext::LocalPrincipal {
                origin: CallOrigin::WasmHost,
                caller: parent.caller.clone(),
            },
            vec![MctWitProducedMessage {
                target_operation: "patina:watch/events@0.1.0.emit".into(),
                topic: "file-created".into(),
                content_type: Some("application/json".into()),
                data: serde_json::to_vec(&wire).unwrap(),
                metadata: Vec::new(),
                offset: 1,
            }],
        )
        .await;
        assert!(result.is_err());
        let summary = MctRuntimeStateStore::open(paths.state_path())
            .unwrap()
            .summary()
            .unwrap();
        assert_eq!(summary.watch_event_batches, 0);
        assert_eq!(summary.watch_events, 0);
        assert_eq!(summary.watch_event_deliveries, 0);
        assert!(
            MctRuntimeStateStore::open(paths.state_path())
                .unwrap()
                .list_runs(10)
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn jvm_bridge_json_call_enters_resident_route_path() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_payload_process_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let (mut request, payload) =
            jvm_bridge_protocol_request("patina:demo/control@0.1.0.run", r#"[{"from":"jvm"}]"#)
                .unwrap();
        request.call.call_id = CallId::new("call-jvm-bridge-test")
            .expect("string ID literal/generated value must be non-empty");
        assert_eq!(request.call.origin, CallOrigin::JvmAdapter);

        let result = execute_resident_call(
            ResidentRuntimePaths::new(config_path, children_dir, state_path),
            ledger.clone(),
            request,
            ResidentPayloadIngress::local(Some(payload)),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Completed);
        let result_payload = result
            .inline_result_payload
            .expect("result payload returned");
        assert_eq!(
            String::from_utf8(result_payload).unwrap(),
            r#"processed:[{"from":"jvm"}]"#
        );
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("call-jvm-bridge-test"));
        assert!(
            ledger_text.contains("RouteRevalidated") || ledger_text.contains("route_revalidated")
        );
    }
}

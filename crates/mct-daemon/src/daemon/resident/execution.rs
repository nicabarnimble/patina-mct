//! Resident local effect-boundary revision guard, child delivery, dispatch, and result projection.

use super::*;

#[derive(Debug)]
struct PreparedChildExecution {
    pub(super) child: mct_daemon::MctLoadedChild,
    pub(super) authorized: AuthorizedChildInvocation,
    pub(super) child_authority_observation_id: ObservationId,
    pub(super) route_taken: RouteTaken,
    pub(super) route_decision_id: DecisionId,
}

#[derive(Clone, Debug)]
pub(super) struct LocalExecutionReport {
    result: MctResult,
    observations: Vec<MctObservation>,
    inline_result_payload: Option<Vec<u8>>,
    run_id: Option<String>,
    produced_messages: Vec<MctWitProducedMessage>,
}

type LocalExecutionParts = (
    MctResult,
    Vec<MctObservation>,
    Option<Vec<u8>>,
    Option<String>,
    Vec<MctWitProducedMessage>,
);

impl LocalExecutionReport {
    pub(super) fn into_parts(self) -> LocalExecutionParts {
        (
            self.result,
            self.observations,
            self.inline_result_payload,
            self.run_id,
            self.produced_messages,
        )
    }
}

pub(super) fn resident_executed_on_observation(
    call: &MctCall,
    route: &RouteTaken,
    outcome: ResultOutcome,
) -> MctObservation {
    let operation_id = mct_daemon::operation_id_from_target(&call.target);
    MctObservation {
        observation_id: ObservationId::new(format!("obs-executed-on:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: match outcome {
            ResultOutcome::Success => ObservationKind::RuntimeExecutionCompleted,
            ResultOutcome::TimedOut => ObservationKind::RuntimeExecutionTimedOut,
            ResultOutcome::Failed | ResultOutcome::Denied | ResultOutcome::Cancelled => {
                ObservationKind::RuntimeExecutionFailed
            }
        },
        source_plane: SourcePlane::Child,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: None,
        subject_id: route.child_id.as_ref().map(ToString::to_string),
        resource_id: Some(format!(
            "node:{};runtime:{:?}",
            route.node_id, route.runtime_kind
        )),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome: match outcome {
            ResultOutcome::Success => ObservationOutcome::Completed,
            ResultOutcome::Denied => ObservationOutcome::Denied,
            ResultOutcome::Failed => ObservationOutcome::Failed,
            ResultOutcome::TimedOut => ObservationOutcome::TimedOut,
            ResultOutcome::Cancelled => ObservationOutcome::Cancelled,
        },
        visibility: ObservationVisibility::InternalOnly,
        safe_message: "runtime execution observed".into(),
        detail_ref: Some(format!(
            "executed_on:{};forwarded_from:{};operation:{operation_id}",
            route.node_id, call.caller.node_id
        )),
    }
}

pub(super) fn current_resident_route_revisions(
    paths: &ResidentRuntimePaths,
    call: &MctCall,
) -> Result<AuthorityContextSnapshot> {
    let config = MctDaemonConfigStore::new(paths.config_path()).load()?;
    let scope = resident_child_scope(&config);
    Ok(AuthorityContextSnapshot {
        policy_revision: scope.policy_revision,
        grants_revision: call.authority_context.grants_revision,
        vision_policy_revision: call.authority_context.vision_policy_revision,
    })
}

pub(super) fn execute_authorized_resident_child(
    paths: ResidentRuntimePaths,
    execution: LocalExecutionPlan,
    request: MctCallProtocolRequest,
    inline_payload: Option<Vec<u8>>,
    current_revisions: AuthorityContextSnapshot,
    before_effect_ledger: Option<ResidentLedgerWriter>,
) -> Result<LocalExecutionReport> {
    let call = request.call.clone();
    let state = MctRuntimeStateStore::open(paths.state_path())?;
    let (child, authorized_route, child_authority_observation_id) = execution.into_parts();
    let route_taken = RouteTaken {
        node_id: authorized_route.route().node_id.clone(),
        child_id: authorized_route.route().child_id.clone(),
        runtime_kind: authorized_route.route().runtime_kind,
    };
    let runtime_kind = route_taken.runtime_kind;
    let run_id = run_id_for_call("resident", &call);

    if authorized_route.policy_revision() != current_revisions.policy_revision {
        let report = resident_route_revision_denial_report(
            &call,
            authorized_route.route(),
            authorized_route.revalidation_decision_id().clone(),
            CandidateEliminationReason::PolicyRevisionStale,
            &current_revisions,
            authorized_route.policy_revision(),
            authorized_route.grants_revision(),
        );
        return Ok(report);
    }
    if authorized_route.grants_revision() != current_revisions.grants_revision {
        let report = resident_route_revision_denial_report(
            &call,
            authorized_route.route(),
            authorized_route.revalidation_decision_id().clone(),
            CandidateEliminationReason::GrantsRevisionStale,
            &current_revisions,
            authorized_route.policy_revision(),
            authorized_route.grants_revision(),
        );
        return Ok(report);
    }

    let route_decision_id = authorized_route.revalidation_decision_id().clone();
    let child_invocation = authorized_route.into_child_invocation();
    let child_execution = PreparedChildExecution {
        child,
        authorized: child_invocation,
        child_authority_observation_id,
        route_taken,
        route_decision_id,
    };
    let provenance = ChildInvocationProvenance::from_authorized(
        &child_execution.authorized,
        child_execution.child_authority_observation_id.clone(),
    );
    state.insert_run_started(
        &run_id,
        &call,
        runtime_kind,
        Some(&provenance),
        mct_daemon::current_timestamp_string(),
    )?;

    let mut report = match child_execution.child.ingress_mode {
        mct_daemon::MctChildIngressMode::Handle => {
            execute_resident_process_child(child_execution, &request, inline_payload.as_deref())?
        }
        mct_daemon::MctChildIngressMode::Hybrid | mct_daemon::MctChildIngressMode::WitOnly => {
            execute_resident_wit_child(
                child_execution,
                &request,
                inline_payload.as_deref(),
                paths.state_path(),
                before_effect_ledger,
            )?
        }
    };
    if let Some(route) = report.result.route_taken.as_ref() {
        report.observations.push(resident_executed_on_observation(
            &call,
            route,
            report.result.outcome,
        ));
    }
    if let Some(bytes) = inline_payload.as_deref() {
        report.observations.push(resident_payload_fact_observation(
            &call,
            "request",
            bytes,
            &call.payload_metadata.data_classification,
        ));
    }
    if let Some(bytes) = report.inline_result_payload.as_deref() {
        report.observations.push(resident_payload_fact_observation(
            &call,
            "result",
            bytes,
            &call.payload_metadata.data_classification,
        ));
    }
    report.run_id = Some(run_id);
    Ok(report)
}

fn execute_resident_process_child(
    execution: PreparedChildExecution,
    request: &MctCallProtocolRequest,
    inline_payload: Option<&[u8]>,
) -> Result<LocalExecutionReport> {
    let call = &request.call;
    let harness = MctProcessChildHarness {
        executable: execution.child.wasm_path.clone(),
        args: Vec::new(),
        timeout: Duration::from_secs(5),
        local_node_id: MctNodeId::new("local-mct")
            .expect("string ID literal/generated value must be non-empty"),
    };
    let payload_bytes = inline_payload.unwrap_or_default();
    let report = harness.invoke_authorized_child_bytes(
        execution.authorized,
        call,
        payload_bytes,
        MctProcessChildInvocationIds {
            started_observation_id: ObservationId::new(format!(
                "obs-resident-process-started:{}",
                call.call_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            completed_observation_id: ObservationId::new(format!(
                "obs-resident-process-completed:{}",
                call.call_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            result_ref: ResultRef::new(format!("result-resident-process:{}", call.call_id))
                .expect("string ID literal/generated value must be non-empty"),
            audit_ref: AuditRef::new(format!("audit-resident-process:{}", call.call_id))
                .expect("string ID literal/generated value must be non-empty"),
            started_at: current_timestamp(),
            completed_at: current_timestamp(),
        },
    )?;
    let result_bytes = report.stdout.as_bytes().to_vec();
    let mut result = report.result;
    result.authority_decision_ref = execution.route_decision_id;
    result.route_taken = route_taken_for_outcome(result.outcome, execution.route_taken);
    let inline_result_payload = apply_inline_result_payload(
        &mut result,
        format!("result-resident-process:{}", call.call_id),
        "text/plain",
        result_bytes,
    );
    Ok(LocalExecutionReport {
        result,
        observations: report.observations,
        inline_result_payload,
        run_id: None,
        produced_messages: Vec::new(),
    })
}

fn authorize_resident_watch_toy(
    state: &MctRuntimeStateStore,
    child: &mct_daemon::MctLoadedChild,
    authorized_child: &AuthorizedChildInvocation,
    call: &MctCall,
    toy_id: &str,
    action: &str,
    label: &str,
) -> std::result::Result<CliAuthorizedToy, CliToyAuthorizationError> {
    let contracts = state.toy_contracts().map_err(cli_adapter_error)?;
    let grants = state.toy_grant_snapshots().map_err(cli_adapter_error)?;
    let toy_id = ToyId::new(toy_id).map_err(|error| cli_adapter_error(error.into()))?;
    let resource_id = grants
        .iter()
        .find(|grant| {
            grant.toy_id == toy_id
                && grant.subject.child_name == child.name
                && grant.subject.artifact_id == child.artifact_id
                && grant.subject.artifact_version == child.version
                && grant.subject.assignment_id.as_ref() == Some(authorized_child.assignment_id())
                && grant.grant_state == ToyGrantState::Active
                && grant
                    .scope
                    .allowed_actions
                    .iter()
                    .any(|allowed| allowed == action)
        })
        .and_then(|grant| grant.scope.resource_id.clone())
        .ok_or_else(|| CliToyAuthorizationError {
            safe_message: format!("current exact {label} Toy grant not found"),
            observations: Vec::new(),
        })?;
    authorize_cli_toy(CliToyAuthorizationRequest {
        child,
        authorized_child,
        call,
        contracts: &contracts,
        grants: &grants,
        toy_id,
        action,
        resource_id: Some(resource_id),
        label,
    })
}

fn resident_watch_host_adapters(
    state: &MctRuntimeStateStore,
    state_path: &Path,
    child: &mct_daemon::MctLoadedChild,
    authorized_child: &AuthorizedChildInvocation,
    call: &MctCall,
    imports: &BTreeSet<String>,
) -> std::result::Result<CliWitHostAdapterBuild, CliToyAuthorizationError> {
    let scopes = state
        .watch_observation_scopes()
        .map_err(cli_adapter_error)?;
    let scope = scopes
        .into_iter()
        .filter(|scope| {
            scope.authority_state == WatchObservationScopeState::Active
                && scope.observer_ref.child_name == child.name
                && scope.observer_ref.artifact_id.as_str() == child.artifact_id
                && scope.observer_ref.assignment_id == *authorized_child.assignment_id()
        })
        .max_by_key(|scope| scope.scope_revision)
        .ok_or_else(|| CliToyAuthorizationError {
            safe_message: "current exact Watch observation scope not found".into(),
            observations: Vec::new(),
        })?;
    let now = current_timestamp();
    let observer_request = WatchObservationSessionRequest {
        current_observer: scope.observer_ref.clone(),
        now: now.clone(),
    };
    let watch_preopen = authorize_resident_watch_toy(
        state,
        child,
        authorized_child,
        call,
        MCT_WATCH_TOY_ID,
        MCT_WATCH_TOY_ACTION,
        "watch-preopen",
    )?;
    let mut observations = vec![toy_grant_evaluation_observation(
        call.trace_context.trace_id.clone(),
        now.clone(),
        &watch_preopen.evaluation,
    )];
    let _watch_session =
        authorize_watch_observation_session(watch_preopen.authorized, &scope, &observer_request)
            .map_err(|error| CliToyAuthorizationError {
                safe_message: error.to_string(),
                observations: observations.clone(),
            })?;
    if scope.traversal_scope != WatchTraversalScope::Recursive {
        return Err(CliToyAuthorizationError {
            safe_message: "root-only Watch traversal is unsupported by the v1 WASI adapter".into(),
            observations,
        });
    }
    let root = scope
        .canonical_root_ref
        .strip_prefix("file://")
        .map(PathBuf::from)
        .ok_or_else(|| CliToyAuthorizationError {
            safe_message: "Watch root is not a canonical local directory reference".into(),
            observations: observations.clone(),
        })?;
    let content = authorize_resident_watch_toy(
        state,
        child,
        authorized_child,
        call,
        MCT_DIRECTORY_READ_TOY_ID,
        "read-content",
        "watch-content-read",
    )?;
    let content_resource = content
        .authorized
        .resource_id()
        .map(str::to_owned)
        .unwrap_or_default();
    if content_resource != scope.canonical_root_ref {
        return Err(CliToyAuthorizationError {
            safe_message: "content-read root must equal the v1 Watch root".into(),
            observations,
        });
    }
    observations.push(toy_grant_evaluation_observation(
        call.trace_context.trace_id.clone(),
        now.clone(),
        &content.evaluation,
    ));

    let mut toy_registry = MctToyAdapterRegistry::new();
    let mut logging = None;
    let mut measure = None;
    for (import, toy_id, label, slot) in [
        (
            "wasi:logging/logging@0.1.0",
            MCT_WASI_LOGGING_TOY_ID,
            "watch-logging",
            0u8,
        ),
        (
            "patina:measure/measure@0.1.0",
            MCT_PATINA_MEASURE_TOY_ID,
            "watch-measure",
            1u8,
        ),
    ] {
        if imports.contains(import) {
            let authorized = authorize_resident_watch_toy(
                state,
                child,
                authorized_child,
                call,
                toy_id,
                "invoke",
                label,
            )?;
            observations.push(toy_grant_evaluation_observation(
                call.trace_context.trace_id.clone(),
                now.clone(),
                &authorized.evaluation,
            ));
            let id = ToyId::new(toy_id).map_err(|error| cli_adapter_error(error.into()))?;
            toy_registry.register(id, MctToyBackend::EchoJson);
            let adapter = wit_toy_adapter(authorized.authorized, &format!("obs-{label}"));
            if slot == 0 {
                logging = Some(adapter);
            } else {
                measure = Some(adapter);
            }
        }
    }

    let keyvalue = if imports.contains("wasi:keyvalue/store@0.2.0") {
        let get = authorize_resident_watch_toy(
            state,
            child,
            authorized_child,
            call,
            MCT_CHILD_KEYVALUE_TOY_ID,
            "get",
            "watch-keyvalue-get",
        )?;
        let set = authorize_resident_watch_toy(
            state,
            child,
            authorized_child,
            call,
            MCT_CHILD_KEYVALUE_TOY_ID,
            "set",
            "watch-keyvalue-set",
        )?;
        observations.extend([
            toy_grant_evaluation_observation(
                call.trace_context.trace_id.clone(),
                now.clone(),
                &get.evaluation,
            ),
            toy_grant_evaluation_observation(
                call.trace_context.trace_id.clone(),
                now.clone(),
                &set.evaluation,
            ),
        ]);
        let resource = get.authorized.resource_id().unwrap_or_default().to_owned();
        let bucket_identifier = resource
            .rsplit_once(":bucket:")
            .map(|(_, bucket)| bucket.to_owned())
            .ok_or_else(|| CliToyAuthorizationError {
                safe_message: "keyvalue grant has malformed bucket resource".into(),
                observations: observations.clone(),
            })?;
        toy_registry.register(
            ToyId::new(MCT_CHILD_KEYVALUE_TOY_ID)
                .map_err(|error| cli_adapter_error(error.into()))?,
            MctToyBackend::EchoJson,
        );
        Some(MctWitKeyvalueHostAdapter {
            get: wit_toy_adapter(get.authorized, "obs-watch-keyvalue-get"),
            set: wit_toy_adapter(set.authorized, "obs-watch-keyvalue-set"),
            state_path: state_path.to_path_buf(),
            bucket_identifier,
            bucket_resource_id: resource,
        })
    } else {
        None
    };

    let messaging = if imports.contains("wasi:messaging/producer@0.2.0") {
        let callout = authorize_resident_watch_toy(
            state,
            child,
            authorized_child,
            call,
            MCT_WATCH_TOY_ID,
            MCT_WATCH_TOY_ACTION,
            "watch-callout",
        )?;
        observations.push(toy_grant_evaluation_observation(
            call.trace_context.trace_id.clone(),
            now,
            &callout.evaluation,
        ));
        toy_registry.register(
            ToyId::new(MCT_WATCH_TOY_ID).map_err(|error| cli_adapter_error(error.into()))?,
            MctToyBackend::EchoJson,
        );
        Some(MctWitMessagingHostAdapter {
            toy: wit_toy_adapter(callout.authorized, "obs-watch-callout"),
            watch_admission: MctWitWatchMessageAdmission {
                event_classes: scope.event_classes.iter().copied().collect(),
                max_events_per_batch: scope.max_events_per_batch,
            },
        })
    } else {
        None
    };

    Ok(CliWitHostAdapterBuild {
        adapters: MctWitHostImportAdapters {
            toy_registry,
            logging,
            measure,
            git: None,
            keyvalue,
            messaging,
            wasi: Some(MctWasiHostConfig {
                preopens: vec![MctWasiPreopen {
                    host_path: root,
                    guest_path: "/input".into(),
                    access: MctWasiPreopenAccess::ReadOnly,
                }],
            }),
        },
        observations,
    })
}

fn execute_resident_wit_child(
    execution: PreparedChildExecution,
    request: &MctCallProtocolRequest,
    inline_payload: Option<&[u8]>,
    state_path: &Path,
    before_effect_ledger: Option<ResidentLedgerWriter>,
) -> Result<LocalExecutionReport> {
    let call = &request.call;
    let content_type = inline_payload_content_type(&request.payload).unwrap_or("application/json");
    if content_type != "application/json" {
        return Ok(resident_delivery_failure_report(
            call,
            execution.route_decision_id,
            execution.route_taken,
            CallProtocolReason::ChildPayloadContentTypeUnsupported,
            "unsupported child payload",
        ));
    }
    let args_json = match inline_payload {
        Some(bytes) => serde_json::from_slice::<serde_json::Value>(bytes)?,
        None => serde_json::json!([]),
    };
    let runtime = MctWasmComponentRuntime::new(default_wasm_host_config())?;
    let imports = runtime.discover_wit_imports(&execution.child.wasm_path)?;
    let state = MctRuntimeStateStore::open(state_path)?;
    let project_root = state
        .toy_grant_snapshots()?
        .into_iter()
        .find(|grant| {
            grant.toy_id == slate_filesystem_toy_id()
                && grant.subject.child_name == execution.child.name
                && grant.subject.artifact_id == execution.child.artifact_id
                && grant.grant_state == ToyGrantState::Active
        })
        .and_then(|grant| grant.scope.resource_id)
        .map(PathBuf::from);
    let adapter_build = match if imports.contains("wasi:keyvalue/store@0.2.0")
        || imports.contains("wasi:messaging/producer@0.2.0")
    {
        resident_watch_host_adapters(
            &state,
            state_path,
            &execution.child,
            &execution.authorized,
            call,
            &imports,
        )
    } else {
        build_wit_host_adapters_for_cli_call(CliWitAdapterRequest {
            state: &state,
            child: &execution.child,
            authorized_child: &execution.authorized,
            call,
            imports: &imports,
            project_root: project_root.as_deref(),
            guest_project: "/project",
            git_repo: project_root.as_deref(),
        })
    } {
        Ok(build) => build,
        Err(error) => {
            return Ok(resident_toy_authority_denial_report(
                call,
                execution.route_decision_id,
                error,
            ));
        }
    };
    let mut observations = adapter_build.observations;
    if let Some(ledger) = before_effect_ledger {
        tokio::runtime::Handle::current()
            .block_on(ledger.append(observations.clone()))
            .context("append Toy and Watch authority evaluations before WIT effects")?;
        observations.clear();
    }
    let mut report = runtime.invoke_authorized_child_wit_export_with_host_adapters(
        execution.authorized,
        &execution.child,
        call,
        &args_json,
        adapter_build.adapters,
        MctWasmComponentInvocationIds {
            started_observation_id: ObservationId::new(format!(
                "obs-resident-wasm-wit-started:{}",
                call.call_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            completed_observation_id: ObservationId::new(format!(
                "obs-resident-wasm-wit-completed:{}",
                call.call_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            audit_ref: AuditRef::new(format!("audit-resident-wasm-wit:{}", call.call_id))
                .expect("string ID literal/generated value must be non-empty"),
            started_at: current_timestamp(),
            completed_at: current_timestamp(),
        },
    )?;
    observations.append(&mut report.observations);
    let produced_messages = report.produced_messages;
    let result_bytes = serde_json::to_vec(&report.output_json)?;
    let mut result = report.result;
    result.authority_decision_ref = execution.route_decision_id;
    result.route_taken = route_taken_for_outcome(result.outcome, execution.route_taken);
    let inline_result_payload = apply_inline_result_payload(
        &mut result,
        format!("result-resident-wit:{}", call.call_id),
        "application/json",
        result_bytes,
    );
    Ok(LocalExecutionReport {
        result,
        observations,
        inline_result_payload,
        run_id: None,
        produced_messages,
    })
}

pub(super) fn apply_inline_result_payload(
    result: &mut MctResult,
    reference: impl Into<String>,
    content_type: impl Into<String>,
    bytes: Vec<u8>,
) -> Option<Vec<u8>> {
    result.execution_summary.output_size_bytes = Some(bytes.len() as u64);
    if bytes.len() > MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES {
        result.outcome = ResultOutcome::Failed;
        result.result_payload = MctCallPayloadHandle::Empty;
        result.requester_message = "result payload too large".into();
        return None;
    }
    result.result_payload = inline_result_payload_handle(reference, content_type, &bytes);
    Some(bytes)
}

pub(super) fn resident_route_revision_denial_report(
    call: &MctCall,
    route: &CandidateRoute,
    decision_id: DecisionId,
    reason: CandidateEliminationReason,
    current: &AuthorityContextSnapshot,
    minted_policy_revision: u64,
    minted_grants_revision: u64,
) -> LocalExecutionReport {
    let observation = MctObservation {
        observation_id: ObservationId::new(format!("obs-route-revision-denied:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::NoRouteRecorded,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: Some(decision_id.clone()),
        subject_id: route.child_id.as_ref().map(ToString::to_string),
        resource_id: Some(route.candidate_id.clone()),
        policy_revision: Some(current.policy_revision),
        grants_revision: Some(current.grants_revision),
        outcome: ObservationOutcome::Denied,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: "not authorized".into(),
        detail_ref: Some(format!(
            "elimination_reason:{reason:?};denial_class:{};minted_policy_revision={minted_policy_revision};current_policy_revision={};minted_grants_revision={minted_grants_revision};current_grants_revision={}",
            reason.denial_class().as_str(),
            current.policy_revision,
            current.grants_revision
        )),
    };
    LocalExecutionReport {
        result: MctResult {
            call_id: call.call_id.clone(),
            outcome: ResultOutcome::Denied,
            route_taken: None,
            authority_decision_ref: decision_id,
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: call.payload_metadata.size_bytes,
                output_size_bytes: None,
            },
            result_payload: MctCallPayloadHandle::Empty,
            requester_message: "not authorized".into(),
            audit_ref: AuditRef::new(format!("audit-route-revision-denied:{}", call.call_id))
                .expect("string ID literal/generated value must be non-empty"),
        },
        observations: vec![observation],
        inline_result_payload: None,
        run_id: None,
        produced_messages: Vec::new(),
    }
}

pub(super) fn route_taken_for_outcome(
    outcome: ResultOutcome,
    route_taken: RouteTaken,
) -> Option<RouteTaken> {
    match outcome {
        ResultOutcome::Success | ResultOutcome::Failed | ResultOutcome::TimedOut => {
            Some(route_taken)
        }
        ResultOutcome::Denied | ResultOutcome::Cancelled => None,
    }
}

fn resident_toy_authority_denial_report(
    call: &MctCall,
    authority_decision_ref: DecisionId,
    error: CliToyAuthorizationError,
) -> LocalExecutionReport {
    let mut observations = error.observations;
    observations.push(MctObservation {
        observation_id: ObservationId::new(format!("obs-resident-toy-denied:{}", call.call_id))
            .expect("generated observation id is non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::CallDenied,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: Some(authority_decision_ref.clone()),
        subject_id: None,
        resource_id: Some("required-toy-authority".into()),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome: ObservationOutcome::Denied,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: "not authorized".into(),
        detail_ref: Some(error.safe_message),
    });
    LocalExecutionReport {
        result: MctResult {
            call_id: call.call_id.clone(),
            outcome: ResultOutcome::Denied,
            route_taken: None,
            authority_decision_ref,
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: call.payload_metadata.size_bytes,
                output_size_bytes: None,
            },
            result_payload: MctCallPayloadHandle::Empty,
            requester_message: "not authorized".into(),
            audit_ref: AuditRef::new(format!("audit-resident-toy-denied:{}", call.call_id))
                .expect("generated audit ref is non-empty"),
        },
        observations,
        inline_result_payload: None,
        run_id: None,
        produced_messages: Vec::new(),
    }
}

pub(super) fn resident_delivery_failure_report(
    call: &MctCall,
    authority_decision_ref: DecisionId,
    route_taken: RouteTaken,
    reason: CallProtocolReason,
    safe_message: &str,
) -> LocalExecutionReport {
    let observation = MctObservation {
        observation_id: ObservationId::new(format!(
            "obs-resident-delivery-failed:{}",
            call.call_id
        ))
        .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::RuntimeExecutionFailed,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: Some(authority_decision_ref.clone()),
        subject_id: None,
        resource_id: Some(format!("{:?}", reason)),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome: ObservationOutcome::Failed,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: None,
    };
    LocalExecutionReport {
        result: MctResult {
            call_id: call.call_id.clone(),
            outcome: ResultOutcome::Failed,
            route_taken: Some(route_taken),
            authority_decision_ref,
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: call.payload_metadata.size_bytes,
                output_size_bytes: None,
            },
            result_payload: MctCallPayloadHandle::Empty,
            requester_message: safe_message.into(),
            audit_ref: AuditRef::new(format!("audit-resident-delivery-failed:{}", call.call_id))
                .expect("string ID literal/generated value must be non-empty"),
        },
        observations: vec![observation],
        inline_result_payload: None,
        run_id: None,
        produced_messages: Vec::new(),
    }
}

pub(super) fn result_to_call_handler_result(
    prefix: &str,
    result: &MctResult,
    inline_result_payload: Option<Vec<u8>>,
) -> MctIrohCallHandlerResult {
    let route_decision_id = Some(result.authority_decision_ref.clone());
    let route_taken = result.route_taken.clone();
    match result.outcome {
        ResultOutcome::Success => {
            let result_ref = ResultRef::new(format!("{prefix}:{}", result.call_id))
                .expect("string ID literal/generated value must be non-empty");
            if let Some(bytes) = inline_result_payload {
                MctIrohCallHandlerResult::completed_with_inline_payload(
                    result_ref,
                    result.result_payload.clone(),
                    bytes,
                )
            } else {
                MctIrohCallHandlerResult::completed(result_ref)
            }
            .with_route(route_decision_id, route_taken)
        }
        ResultOutcome::TimedOut => {
            MctIrohCallHandlerResult::timed_out().with_route(route_decision_id, route_taken)
        }
        ResultOutcome::Denied => {
            MctIrohCallHandlerResult::denied().with_route(route_decision_id, None)
        }
        ResultOutcome::Failed => MctIrohCallHandlerResult::failed(result.requester_message.clone())
            .with_route(route_decision_id, route_taken),
        ResultOutcome::Cancelled => {
            MctIrohCallHandlerResult::cancelled(result.requester_message.clone())
                .with_route(route_decision_id, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

    fn resident_test_call(trace_id: TraceId) -> MctCall {
        let mut call = local_wasm_call(OperationTarget {
            namespace: "patina:demo".into(),
            interface_name: "control@0.1.0".into(),
            function_name: "run".into(),
        });
        call.call_id = CallId::new("call-resident-wit")
            .expect("string ID literal/generated value must be non-empty");
        call.trace_context.trace_id = trace_id;
        call.origin = CallOrigin::Iroh;
        call
    }
    fn resident_test_protocol_request(call: MctCall) -> MctCallProtocolRequest {
        MctCallProtocolRequest {
            protocol_request_id: ProtocolRequestId::new("proto-resident-wit")
                .expect("string ID literal/generated value must be non-empty"),
            authority: MctCallProtocolAuthority {
                hello_decision_id: DecisionId::new("decision-resident-wit-hello")
                    .expect("string ID literal/generated value must be non-empty"),
                peer_binding_id: PeerBindingId::new("binding-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                accepted_alpn: MCT_CALL_ALPN.into(),
                endpoint_id: EndpointIdText::new("endpoint-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                policy_revision: 1,
                grants_revision: 1,
            },
            received_over: IrohConnectionPresentation {
                endpoint_id: EndpointIdText::new("endpoint-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                alpn: MCT_CALL_ALPN.into(),
                connection_side: ConnectionSide::Incoming,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            call,
            payload: MctCallPayloadHandle::Empty,
            idempotency_key: None,
            received_observation_id: ObservationId::new("obs-resident-wit-received")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }
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
    fn write_resident_wit_child(children_dir: &Path) {
        let child_dir = children_dir.join("resident-wit");
        std::fs::create_dir_all(&child_dir).unwrap();
        let artifact_path = child_dir.join("resident-wit.wasm");
        let manifest_path = child_dir.join("child.toml");
        let component_wat = r#"
(component
  (core module $m
    (func $run (export "run") (result i32)
      i32.const 7))
  (core instance $i (instantiate $m))
  (func $run (result s32) (canon lift (core func $i "run")))
  (instance $control (export "run" (func $run)))
  (export "patina:demo/control@0.1.0" (instance $control)))
"#;
        let component = wat::parse_str(component_wat).unwrap();
        std::fs::write(&artifact_path, &component).unwrap();
        write_resident_child_manifest(&manifest_path, "resident-wit", "wit-only");
        write_sha256_sidecar(&artifact_path, &component);
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
    #[tokio::test]
    async fn resident_process_payload_delivery_returns_digest_and_keeps_ledger_byte_free() {
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
        let trace_id = TraceId::new("trace-resident-process-payload")
            .expect("string ID literal/generated value must be non-empty");
        let mut call = resident_test_call(trace_id);
        call.call_id = CallId::new("call-resident-process-payload")
            .expect("string ID literal/generated value must be non-empty");
        let payload = br#"{"secret":"payload-marker"}"#.to_vec();
        let payload_base64 = BASE64_STANDARD.encode(&payload);
        call.payload_metadata.size_bytes = payload.len() as u64;
        let mut request = resident_test_protocol_request(call);
        request.payload = MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: "payload-resident-process".into(),
            content_type: "application/json".into(),
            size_bytes: payload.len() as u64,
            blake3_digest_hex: blake3_hex(&payload),
        };

        let result = execute_resident_call(
            ResidentRuntimePaths::new(config_path, children_dir, state_path),
            ledger.clone(),
            request,
            ResidentPayloadIngress::remote(Some(payload)),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Completed);
        let result_payload = result
            .inline_result_payload
            .expect("result payload returned");
        let expected_result = r#"processed:{"secret":"payload-marker"}"#;
        let expected_result_base64 = BASE64_STANDARD.encode(expected_result.as_bytes());
        assert_eq!(String::from_utf8(result_payload).unwrap(), expected_result);
        assert_eq!(
            result.result_payload.declared_size_bytes(),
            expected_result.len() as u64
        );
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("call-resident-process-payload"));
        assert!(ledger_text.contains("payload:request:size="));
        assert!(ledger_text.contains("payload:result:size="));
        assert!(ledger_text.contains("digest="));
        assert!(!ledger_text.contains("payload-marker"));
        assert!(!ledger_text.contains("processed:"));
        assert!(!ledger_text.contains(&payload_base64));
        assert!(!ledger_text.contains(&expected_result_base64));
    }

    #[tokio::test]
    async fn resident_wit_rejects_non_json_payload_before_execution() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_wit_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path).unwrap();
        let trace_id = TraceId::new("trace-resident-wit-content-type")
            .expect("string ID literal/generated value must be non-empty");
        let mut call = resident_test_call(trace_id);
        let payload = b"not-json".to_vec();
        call.payload_metadata.size_bytes = payload.len() as u64;
        let mut request = resident_test_protocol_request(call);
        request.payload = MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: "payload-resident-wit-text".into(),
            content_type: "text/plain".into(),
            size_bytes: payload.len() as u64,
            blake3_digest_hex: blake3_hex(&payload),
        };

        let result = execute_resident_call(
            ResidentRuntimePaths::new(config_path, children_dir, state_path),
            ledger.clone(),
            request,
            ResidentPayloadIngress::remote(Some(payload)),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Failed);
        assert_eq!(result.safe_message, "unsupported child payload");
        ledger.close().await;
    }

    #[tokio::test]
    async fn resident_execution_runs_wit_child_and_records_trace() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_wit_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let trace_id = TraceId::new("trace-resident-wit-test")
            .expect("string ID literal/generated value must be non-empty");
        let call = resident_test_call(trace_id.clone());
        let request = resident_test_protocol_request(call);

        let result = execute_resident_call(
            ResidentRuntimePaths::new(config_path, children_dir, state_path),
            ledger.clone(),
            request,
            ResidentPayloadIngress::remote(None),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Completed);
        assert!(result.route_decision_id.is_some());
        assert!(result.route_taken.is_some());
        ledger.close().await;

        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        let trace_entries = entries
            .iter()
            .filter(|entry| entry.observation.trace.trace_id == trace_id)
            .collect::<Vec<_>>();
        assert!(
            trace_entries
                .iter()
                .any(|entry| entry.observation.kind == ObservationKind::RouteRevalidated),
            "{trace_entries:?}"
        );
        assert!(
            trace_entries.iter().any(|entry| {
                entry.observation.kind == ObservationKind::RuntimeExecutionCompleted
            }),
            "{trace_entries:?}"
        );
    }

    #[test]
    fn resident_route_revision_guard_denies_before_effect() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let marker_path = dir.path().join("executed-marker");
        write_resident_process_child_script(
            &children_dir,
            "resident-echo",
            format!(
                "#!/bin/sh\necho executed > {}\nprintf '{{\"ok\":true}}'\n",
                marker_path.display()
            )
            .as_bytes(),
        );
        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let config = MctDaemonConfigStore::new(&config_path).load().unwrap();
        let call = resident_test_call(
            TraceId::new("trace-route-stale-effect-guard")
                .expect("string ID literal/generated value must be non-empty"),
        );
        let request = resident_test_protocol_request(call.clone());
        let RouteDisposition::Local {
            plan: authorized, ..
        } = authorize_resident_child_from_loaded(&config, loaded.children, &call).unwrap()
        else {
            panic!("approved child should authorize")
        };
        let stale_revisions = AuthorityContextSnapshot {
            policy_revision: call.authority_context.policy_revision + 1,
            grants_revision: call.authority_context.grants_revision,
            vision_policy_revision: call.authority_context.vision_policy_revision,
        };

        let report = execute_authorized_resident_child(
            ResidentRuntimePaths::new(config_path, children_dir, state_path),
            *authorized,
            request,
            None,
            stale_revisions,
            None,
        )
        .unwrap();

        assert_eq!(report.result.outcome, ResultOutcome::Denied);
        assert!(report.result.route_taken.is_none());
        assert!(!marker_path.exists());
        let text = serde_json::to_string(&report.observations).unwrap();
        assert!(text.contains("PolicyRevisionStale"));
        assert!(text.contains("minted_policy_revision"));
    }

    #[test]
    fn route_taken_projection_follows_outcome_matrix() {
        let route = RouteTaken {
            node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            child_id: Some(
                ChildId::new("resident-echo")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            runtime_kind: RuntimeKind::Process,
        };

        for outcome in [
            ResultOutcome::Success,
            ResultOutcome::Failed,
            ResultOutcome::TimedOut,
        ] {
            assert_eq!(
                route_taken_for_outcome(outcome, route.clone()),
                Some(route.clone())
            );
        }
        for outcome in [ResultOutcome::Denied, ResultOutcome::Cancelled] {
            assert_eq!(route_taken_for_outcome(outcome, route.clone()), None);
        }
    }

    #[test]
    fn cancelled_result_and_reply_hide_route_while_ledger_keeps_selection() {
        let call = resident_test_call(
            TraceId::new("trace-route-cancelled-mid-execution")
                .expect("string ID literal/generated value must be non-empty"),
        );
        let route = CandidateRoute {
            candidate_id: "child:resident-echo".into(),
            node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            child_id: Some(
                ChildId::new("resident-echo")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            runtime_kind: RuntimeKind::Process,
            network_path: NetworkPathClass::Local,
        };
        let decision = RouteDecision::selected(
            &call,
            route.clone(),
            vec![CandidateAuthorityEvaluation::admissible(route, 1, 1)],
            resident_route_decision_ids("cancelled", &call),
        );
        let observation = route_decision_observation(
            call.trace_context.trace_id.clone(),
            current_timestamp(),
            &decision,
        );
        let result = MctResult {
            call_id: call.call_id.clone(),
            outcome: ResultOutcome::Cancelled,
            route_taken: None,
            authority_decision_ref: decision.decision_id.clone(),
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: 0,
                output_size_bytes: None,
            },
            result_payload: MctCallPayloadHandle::Empty,
            requester_message: "cancelled".into(),
            audit_ref: AuditRef::new("audit-cancelled-route")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let reply = MctCallProtocolReply {
            reply_id: ReplyId::new("reply-cancelled-route")
                .expect("string ID literal/generated value must be non-empty"),
            protocol_request_id: ProtocolRequestId::new("proto-cancelled-route")
                .expect("string ID literal/generated value must be non-empty"),
            decision_id: decision.decision_id,
            result_ref: None,
            result_payload: MctCallPayloadHandle::Empty,
            route_taken: None,
            reply_outcome: CallProtocolReplyOutcome::Cancelled,
            safe_message: "cancelled".into(),
            reply_observation_id: ObservationId::new("obs-reply-cancelled-route")
                .expect("string ID literal/generated value must be non-empty"),
        };

        assert!(result.route_taken.is_none());
        assert!(reply.validate().is_ok());
        assert!(reply.route_taken.is_none());
        assert_eq!(observation.kind, ObservationKind::RouteSelected);
        assert_eq!(observation.resource_id, Some("child:resident-echo".into()));
    }

    /// Covers `MctResultTerminality.ClosedOutcomeSet`.
    #[test]
    fn cancelled_result_projection_preserves_cancelled_outcome() {
        let result = MctResult {
            call_id: CallId::new("call-cancelled-projection").unwrap(),
            outcome: ResultOutcome::Cancelled,
            route_taken: None,
            authority_decision_ref: DecisionId::new("decision-cancelled-projection").unwrap(),
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: 0,
                output_size_bytes: None,
            },
            result_payload: MctCallPayloadHandle::Empty,
            requester_message: "cancelled".into(),
            audit_ref: AuditRef::new("audit-cancelled-projection").unwrap(),
        };

        let projected = result_to_call_handler_result("result", &result, None);

        assert_eq!(projected.outcome, CallProtocolOutcome::Cancelled);
        assert!(projected.route_taken.is_none());
    }
}

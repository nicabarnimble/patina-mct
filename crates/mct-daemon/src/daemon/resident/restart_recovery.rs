//! Startup disposition of calls a previous writer tenure left unfinished.
//!
//! A resident Mother holds the exclusive ledger writer lock before this runs,
//! so every in-flight idempotency reservation and every unfinished run record
//! belongs to a tenure that is gone. Nothing of that tenure can still be
//! executing, and nothing may silently execute it again: the call ingress
//! contract says no retry executes the Child twice, and Mother never reaches
//! into Child state to resume it. Each leftover is therefore settled once,
//! deterministically, and in this order:
//!
//! * The run already recorded a result: the reservation completes from it.
//! * The run never recorded a result, or the ledger shows the call reached a
//!   Child or a peer: the call is marked interrupted with an unknown outcome,
//!   the run closes as failed, and the reservation completes with that
//!   interrupted reply. A same-key retry replays it rather than re-executing.
//! * The ledger already recorded a denial as the call's latest result, and
//!   nothing about the call progressed after it (no later route selection or
//!   effect start): the denial was the decision, so the call closes as
//!   Denied, never as interrupted.
//! * No run and no effect evidence: the reservation is released, so a
//!   same-key retry executes exactly once.
//!
//! An active trigger firing whose call was marked interrupted, or whose call
//! the ledger shows was denied, is then closed here too, through its terminal
//! ledger fact, so it neither stays active nor is ever re-submitted.
//!
//! Every disposition is a ledger fact with an ID derived from the leftover
//! itself, appended before the runtime-state change. A crash between the two
//! leaves the fact in place and the next startup finishes the state change
//! without appending it again.

use super::*;
use mct_daemon::{MctInFlightCallIdempotencyEntry, MctRuntimeRunRecord};
use std::collections::BTreeMap;

pub(super) const INTERRUPTED_CALL_MESSAGE: &str =
    "call interrupted by Mother restart; outcome unknown";
const RESULT_RECOVERED_MESSAGE: &str = "call result recovered after Mother restart";
pub(super) const RESULT_BYTES_LOST_MESSAGE: &str =
    "call completed before Mother restart, but its result bytes were not retained; outcome unknown";
const RELEASED_MESSAGE: &str =
    "idempotency reservation released after Mother restart; no effect had begun";
const DENIED_MESSAGE: &str =
    "call closed as denied after Mother restart; the ledger already recorded its denial";
const DISPOSITION_DETAIL_PREFIX: &str = "mct-call-restart-disposition-v1:";
const TRANSIENT_RELEASE_DETAIL: &str = "idempotency_reason:released_retry_later";

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct PriorTenureCallReport {
    pub(super) interrupted: usize,
    pub(super) denied: usize,
    pub(super) released: usize,
    pub(super) results_recovered: usize,
    pub(super) trigger_firings_closed: usize,
}

impl PriorTenureCallReport {
    pub(super) fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Disposition {
    Interrupted,
    Denied,
    Released,
    ResultRecovered,
}

impl Disposition {
    fn code(self) -> &'static str {
        match self {
            Self::Interrupted => "interrupted",
            Self::Denied => "denied",
            Self::Released => "released",
            Self::ResultRecovered => "result_recovered",
        }
    }
}

enum Settlement {
    Reservation {
        entry: MctInFlightCallIdempotencyEntry,
        run: Option<MctRuntimeRunRecord>,
        disposition: Disposition,
    },
    RunOnly {
        run: MctRuntimeRunRecord,
        disposition: Disposition,
    },
}

const INTERRUPTED_OBSERVATION_PREFIX: &str = "obs:call-restart-interrupted:";

#[derive(Default)]
struct LedgerEvidence {
    observation_ids: BTreeSet<String>,
    effect_started_calls: BTreeSet<String>,
    interrupted_calls: BTreeSet<String>,
    /// Calls whose latest recorded result is a denial with nothing about the
    /// call progressing after it, with the denying decision when recorded.
    denied_calls: BTreeMap<String, Option<DecisionId>>,
    /// Calls refused transiently with their key released for retry.
    transiently_released_calls: BTreeSet<String>,
}

fn is_effect_start(kind: ObservationKind) -> bool {
    matches!(
        kind,
        ObservationKind::RuntimeExecutionStarted
            | ObservationKind::ChildInvoked
            | ObservationKind::PeerCallSent
            | ObservationKind::AdapterEffectStarted
            | ObservationKind::ToyCallStarted
    )
}

fn ledger_evidence(ledger_path: &Path, mother_node_id: &str) -> Result<LedgerEvidence> {
    let entries =
        JsonlObservationLedger::open_read_only(ledger_path, "ledger-local", mother_node_id)?
            .entries()?;
    let mut evidence = LedgerEvidence::default();
    // Per call: the latest recorded result (position, denial decision when it
    // was a denial) and the latest position where the call progressed.
    let mut latest_result: BTreeMap<String, (usize, Option<Option<DecisionId>>)> = BTreeMap::new();
    let mut latest_progress: BTreeMap<String, usize> = BTreeMap::new();
    for (position, entry) in entries.into_iter().enumerate() {
        let observation = entry.observation;
        // Transport replies correlate to wire IDs, not durable local executions.
        // Also excludes historical pre-L8 transport records without rewriting them.
        if observation.source_plane == SourcePlane::Peer
            && observation.kind == ObservationKind::ResultRecorded
        {
            continue;
        }
        let is_disposition = observation
            .detail_ref
            .as_deref()
            .is_some_and(|detail| detail.starts_with(DISPOSITION_DETAIL_PREFIX));
        if let Some(call_id) = observation.call_id.as_ref().map(|id| id.as_str()) {
            if is_effect_start(observation.kind) {
                evidence.effect_started_calls.insert(call_id.to_owned());
            }
            if is_effect_start(observation.kind)
                || observation.kind == ObservationKind::RouteSelected
            {
                latest_progress.insert(call_id.to_owned(), position);
            }
            if observation.kind == ObservationKind::ResultRecorded && !is_disposition {
                let denial = (observation.outcome == ObservationOutcome::Denied)
                    .then(|| observation.decision_id.clone());
                latest_result.insert(call_id.to_owned(), (position, denial));
            }
            if observation.detail_ref.as_deref() == Some(TRANSIENT_RELEASE_DETAIL) {
                evidence
                    .transiently_released_calls
                    .insert(call_id.to_owned());
            }
        }
        if observation
            .observation_id
            .as_str()
            .starts_with(INTERRUPTED_OBSERVATION_PREFIX)
            && let Some(call_id) = observation.call_id.as_ref()
        {
            evidence
                .interrupted_calls
                .insert(call_id.as_str().to_owned());
        }
        evidence
            .observation_ids
            .insert(observation.observation_id.as_str().to_owned());
    }
    for (call_id, (position, denial)) in latest_result {
        let progressed_after = latest_progress
            .get(&call_id)
            .is_some_and(|progress| *progress > position);
        if let Some(decision) = denial
            && !progressed_after
        {
            evidence.denied_calls.insert(call_id, decision);
        }
    }
    Ok(evidence)
}

impl LedgerEvidence {
    /// A run (or trigger firing) closes as Denied when the ledger holds the
    /// call's denial and the call was never marked interrupted.
    fn denial_for_run(&self, call_id: &str) -> Option<&Option<DecisionId>> {
        if self.interrupted_calls.contains(call_id) {
            return None;
        }
        self.denied_calls.get(call_id)
    }

    /// A reservation with no run closes as Denied only when the denial cannot
    /// be the record of a transient refusal whose key was released for retry.
    fn denial_for_reservation(&self, call_id: &str) -> Option<&Option<DecisionId>> {
        if self.transiently_released_calls.contains(call_id) {
            return None;
        }
        self.denial_for_run(call_id)
    }
}

/// Settles every call the previous writer tenure left in flight. Must run
/// after startup finalization and before any call ingress or trigger
/// scheduling starts.
pub(super) async fn settle_prior_tenure_calls(
    paths: &ResidentRuntimePaths,
    ledger_path: &Path,
    mother_node_id: &str,
    ledger: &ResidentLedgerWriter,
) -> Result<PriorTenureCallReport> {
    let mut report = settle_prior_tenure_reservations_and_runs(
        paths.state_path(),
        ledger_path,
        mother_node_id,
        ledger,
    )
    .await?;
    report.trigger_firings_closed =
        close_unfinished_trigger_firings(paths, ledger_path, mother_node_id, ledger).await?;
    Ok(report)
}

/// Closes every active trigger firing whose call carries an interruption
/// fact. Runs after reservations and runs are settled, and finds the same
/// work again after a crash part way through, so each firing closes once.
async fn close_unfinished_trigger_firings(
    paths: &ResidentRuntimePaths,
    ledger_path: &Path,
    mother_node_id: &str,
    ledger: &ResidentLedgerWriter,
) -> Result<usize> {
    let (firings, authorities) = {
        let state = MctRuntimeStateStore::open(paths.state_path())?;
        let firings = state.active_call_trigger_firings()?;
        if firings.is_empty() {
            return Ok(0);
        }
        (firings, state.call_trigger_authorities()?)
    };
    let evidence = ledger_evidence(ledger_path, mother_node_id)?;
    let mut closed = 0;
    for firing in firings {
        let call_id = firing.call_id.as_str();
        let denial = evidence.denial_for_run(call_id).cloned();
        if !evidence.interrupted_calls.contains(call_id) && denial.is_none() {
            continue;
        }
        let Some(authority) = authorities.iter().find(|authority| {
            authority.trigger_authority_id == firing.trigger_authority_id
                && authority.record_revision == firing.record_revision
        }) else {
            bail!(
                "trigger authority {} revision {} for interrupted firing {} is absent",
                firing.trigger_authority_id,
                firing.record_revision,
                firing.firing_id
            );
        };
        let terminal_already_recorded = evidence
            .observation_ids
            .contains(&trigger_terminal_observation_id(&firing));
        match denial {
            Some(decision) => {
                close_unfinished_trigger_firing(
                    paths,
                    ledger,
                    authority,
                    &firing,
                    "denied",
                    |run| denied_result(run, decision.clone()),
                    terminal_already_recorded,
                )
                .await?
            }
            None => {
                close_unfinished_trigger_firing(
                    paths,
                    ledger,
                    authority,
                    &firing,
                    "interrupted",
                    interrupted_result,
                    terminal_already_recorded,
                )
                .await?
            }
        }
        closed += 1;
    }
    Ok(closed)
}

async fn settle_prior_tenure_reservations_and_runs(
    state_path: &Path,
    ledger_path: &Path,
    mother_node_id: &str,
    ledger: &ResidentLedgerWriter,
) -> Result<PriorTenureCallReport> {
    let (settlements, mut observations, denials) = {
        let state = MctRuntimeStateStore::open(state_path)?;
        let reservations = state.in_flight_call_idempotency_entries()?;
        let unfinished_runs = state.unfinished_runs()?;
        if reservations.is_empty() && unfinished_runs.is_empty() {
            return Ok(PriorTenureCallReport::default());
        }
        let evidence = ledger_evidence(ledger_path, mother_node_id)?;
        let mut settled_runs = BTreeSet::new();
        let mut settlements = Vec::new();
        for entry in reservations {
            let call_id = entry.recovery_call_id.as_str();
            let run = if call_id.starts_with("legacy-peer-reservation:") {
                None
            } else {
                state.get_run_by_call_id(&entry.recovery_call_id)?
            };
            let disposition = match run.as_ref() {
                None if call_id.starts_with("legacy-peer-reservation:") => Disposition::Interrupted,
                Some(run) if run.result.is_some() => Disposition::ResultRecovered,
                Some(_) if evidence.denial_for_run(call_id).is_some() => Disposition::Denied,
                Some(_) => Disposition::Interrupted,
                None if evidence.denial_for_reservation(call_id).is_some() => Disposition::Denied,
                None if evidence
                    .effect_started_calls
                    .contains(entry.recovery_call_id.as_str()) =>
                {
                    Disposition::Interrupted
                }
                None => Disposition::Released,
            };
            if let Some(run) = run.as_ref() {
                settled_runs.insert(run.run_id.clone());
            }
            settlements.push(Settlement::Reservation {
                entry,
                run,
                disposition,
            });
        }
        for run in unfinished_runs {
            if settled_runs.insert(run.run_id.clone()) {
                let disposition = if evidence.denial_for_run(run.call_id.as_str()).is_some() {
                    Disposition::Denied
                } else {
                    Disposition::Interrupted
                };
                settlements.push(Settlement::RunOnly { run, disposition });
            }
        }
        let observations = settlements
            .iter()
            .map(settlement_observation)
            .collect::<Result<Vec<_>>>()?;
        let observations = observations
            .into_iter()
            .filter(|observation| {
                !evidence
                    .observation_ids
                    .contains(observation.observation_id.as_str())
            })
            .collect::<Vec<_>>();
        let denials = settlements
            .iter()
            .filter_map(|settlement| {
                let call_id = match settlement {
                    Settlement::Reservation {
                        entry,
                        disposition: Disposition::Denied,
                        ..
                    } => entry.recovery_call_id.as_str(),
                    Settlement::RunOnly {
                        run,
                        disposition: Disposition::Denied,
                    } => run.call_id.as_str(),
                    _ => return None,
                };
                Some((
                    call_id.to_owned(),
                    evidence.denied_calls.get(call_id).cloned().flatten(),
                ))
            })
            .collect::<BTreeMap<_, _>>();
        (settlements, observations, denials)
    };

    // The ledger fact comes first; runtime state follows it.
    ledger
        .append(std::mem::take(&mut observations))
        .await
        .context("record prior-tenure call dispositions")?;

    let state = MctRuntimeStateStore::open(state_path)?;
    let completed_at = current_timestamp();
    let mut report = PriorTenureCallReport::default();
    for settlement in &settlements {
        match settlement {
            Settlement::Reservation {
                entry,
                run,
                disposition,
            } => match disposition {
                Disposition::Released => {
                    state.release_call_idempotency(
                        &entry.caller_scope,
                        &entry.idempotency_key,
                        &entry.fingerprint,
                    )?;
                    report.released += 1;
                }
                Disposition::Denied => {
                    let decision = denials
                        .get(entry.recovery_call_id.as_str())
                        .cloned()
                        .flatten();
                    let result = match run {
                        Some(run) => {
                            let result = denied_result(run, decision);
                            close_run(&state, run, &result, settlement)?;
                            result
                        }
                        None => denied_result_for_call(
                            &entry.recovery_call_id,
                            decision,
                            "reservation",
                            0,
                        ),
                    };
                    state.complete_call_idempotency(
                        &entry.caller_scope,
                        &entry.idempotency_key,
                        &entry.fingerprint,
                        &recovered_reply(&result),
                        &completed_at,
                    )?;
                    report.denied += 1;
                }
                Disposition::Interrupted => {
                    if let Some(run) = run {
                        close_run(&state, run, &interrupted_result(run), settlement)?;
                    }
                    let reply = interrupted_reply(run.as_ref());
                    state.complete_call_idempotency(
                        &entry.caller_scope,
                        &entry.idempotency_key,
                        &entry.fingerprint,
                        &reply,
                        &completed_at,
                    )?;
                    report.interrupted += 1;
                }
                Disposition::ResultRecovered => {
                    let result = run
                        .as_ref()
                        .and_then(|run| run.result.as_ref())
                        .context("recovered call result disappeared")?;
                    state.complete_call_idempotency(
                        &entry.caller_scope,
                        &entry.idempotency_key,
                        &entry.fingerprint,
                        &recovered_reply(result),
                        &completed_at,
                    )?;
                    report.results_recovered += 1;
                }
            },
            Settlement::RunOnly { run, disposition } => {
                if *disposition == Disposition::Denied {
                    let decision = denials.get(run.call_id.as_str()).cloned().flatten();
                    close_run(&state, run, &denied_result(run, decision), settlement)?;
                    report.denied += 1;
                } else {
                    close_run(&state, run, &interrupted_result(run), settlement)?;
                    report.interrupted += 1;
                }
            }
        }
    }
    Ok(report)
}

fn close_run(
    state: &MctRuntimeStateStore,
    run: &MctRuntimeRunRecord,
    result: &MctResult,
    settlement: &Settlement,
) -> Result<()> {
    state.complete_run(&run.run_id, result, current_timestamp_string())?;
    state.append_run_observations(&run.run_id, &[settlement_observation(settlement)?])
}

fn interrupted_result(run: &MctRuntimeRunRecord) -> MctResult {
    MctResult {
        call_id: run.call_id.clone(),
        outcome: ResultOutcome::Failed,
        route_taken: None,
        authority_decision_ref: run.authority_decision_id.clone().unwrap_or_else(|| {
            DecisionId::new(format!("decision-restart-interrupted:{}", run.call_id))
                .expect("generated decision ID must be non-empty")
        }),
        execution_summary: ExecutionSummary {
            wall_time_ms: 0,
            execution_time_ms: None,
            queue_wait_ms: None,
            input_size_bytes: run.call.payload_metadata.size_bytes,
            output_size_bytes: None,
        },
        result_payload: MctCallPayloadHandle::Empty,
        requester_message: INTERRUPTED_CALL_MESSAGE.into(),
        audit_ref: AuditRef::new(format!("audit-restart-interrupted:{}", run.run_id))
            .expect("generated audit ID must be non-empty"),
    }
}

fn denied_result(run: &MctRuntimeRunRecord, decision: Option<DecisionId>) -> MctResult {
    denied_result_for_call(
        &run.call_id,
        decision.or_else(|| run.authority_decision_id.clone()),
        &run.run_id,
        run.call.payload_metadata.size_bytes,
    )
}

fn denied_result_for_call(
    call_id: &CallId,
    decision: Option<DecisionId>,
    audit_subject: &str,
    input_size_bytes: u64,
) -> MctResult {
    MctResult {
        call_id: call_id.clone(),
        outcome: ResultOutcome::Denied,
        route_taken: None,
        authority_decision_ref: decision.unwrap_or_else(|| {
            DecisionId::new(format!("decision-restart-denied:{call_id}"))
                .expect("generated decision ID must be non-empty")
        }),
        execution_summary: ExecutionSummary {
            wall_time_ms: 0,
            execution_time_ms: None,
            queue_wait_ms: None,
            input_size_bytes,
            output_size_bytes: None,
        },
        result_payload: MctCallPayloadHandle::Empty,
        requester_message: "not authorized".into(),
        audit_ref: AuditRef::new(format!("audit-restart-denied:{audit_subject}:{call_id}"))
            .expect("generated audit ID must be non-empty"),
    }
}

fn interrupted_reply(run: Option<&MctRuntimeRunRecord>) -> MctRecordedCallReply {
    MctRecordedCallReply {
        result_ref: run.map(|run| {
            ResultRef::new(format!("result-resident:{}", run.call_id))
                .expect("generated result ref must be non-empty")
        }),
        result_payload: MctCallPayloadHandle::Empty,
        inline_result_payload: None,
        route_decision_id: run.and_then(|run| run.authority_decision_id.clone()),
        route_taken: None,
        outcome: CallProtocolOutcome::Failed,
        protocol_reason: Some(CallProtocolReason::ExecutionInterrupted),
        safe_message: INTERRUPTED_CALL_MESSAGE.into(),
    }
}

fn recovered_reply(result: &MctResult) -> MctRecordedCallReply {
    let handler = result_to_call_handler_result("result-resident", result, None);
    let lost_payload = result.outcome == ResultOutcome::Success
        && result.result_payload != MctCallPayloadHandle::Empty;
    if !lost_payload {
        // Nothing was carried inline: the recorded outcome replays as is.
        return MctRecordedCallReply {
            result_ref: handler.result_ref,
            result_payload: handler.result_payload,
            inline_result_payload: None,
            route_decision_id: handler.route_decision_id,
            route_taken: handler.route_taken,
            outcome: handler.outcome,
            protocol_reason: handler.protocol_reason,
            safe_message: handler.safe_message,
        };
    }
    // The Child produced a result payload, but its bytes lived only in the
    // dead tenure's reply path. Replaying the handle without its bytes would
    // fail payload integrity, and replaying `Empty` would claim the Child
    // returned nothing. The only honest reply is an unknown outcome that
    // names the original handle so the bytes are reported as missing.
    MctRecordedCallReply {
        result_ref: handler.result_ref,
        result_payload: MctCallPayloadHandle::Empty,
        inline_result_payload: None,
        route_decision_id: handler.route_decision_id,
        route_taken: handler.route_taken,
        outcome: CallProtocolOutcome::Failed,
        protocol_reason: Some(CallProtocolReason::ExecutionInterrupted),
        safe_message: format!(
            "{RESULT_BYTES_LOST_MESSAGE}; missing result payload {}",
            describe_payload_handle(&result.result_payload)
        ),
    }
}

fn describe_payload_handle(handle: &MctCallPayloadHandle) -> String {
    match handle {
        MctCallPayloadHandle::InlinePayload {
            inline_payload_ref,
            size_bytes,
            blake3_digest_hex,
            ..
        } => {
            format!("inline:{inline_payload_ref} ({size_bytes} bytes, blake3 {blake3_digest_hex})")
        }
        MctCallPayloadHandle::ContentAddressedBlob {
            blob_ref,
            digest,
            size_bytes,
            ..
        } => format!("blob:{blob_ref} ({size_bytes} bytes, {digest})"),
        MctCallPayloadHandle::ExternalReference {
            external_ref,
            approximate_size_bytes,
            ..
        } => format!("external:{external_ref} (~{approximate_size_bytes} bytes)"),
        MctCallPayloadHandle::Empty => "empty".into(),
    }
}

fn settlement_observation(settlement: &Settlement) -> Result<MctObservation> {
    let (disposition, call_id, run, identity, scope, fingerprint) = match settlement {
        Settlement::Reservation {
            entry,
            run,
            disposition,
        } => (
            *disposition,
            entry.recovery_call_id.clone(),
            run.as_ref(),
            format!(
                "reservation\n{}\n{}\n{}\n{}\n{}\n{}",
                entry.caller_scope,
                entry.idempotency_key,
                entry.fingerprint.target,
                entry.recovery_call_id,
                entry.fingerprint.payload_digest,
                entry.created_at
            ),
            Some(format!(
                "scope-blake3:{}",
                blake3_hex(entry.caller_scope.as_bytes())
            )),
            Some(format!(
                "fingerprint-blake3:{}",
                blake3_hex(
                    format!(
                        "{}:{}:{}",
                        entry.fingerprint.target,
                        entry.recovery_call_id,
                        entry.fingerprint.payload_digest
                    )
                    .as_bytes()
                )
            )),
        ),
        Settlement::RunOnly { run, disposition } => (
            *disposition,
            run.call_id.clone(),
            Some(run),
            format!("run\n{}", run.run_id),
            None,
            None,
        ),
    };
    let digest = blake3_hex(identity.as_bytes());
    let (kind, outcome, safe_message) = match disposition {
        Disposition::Interrupted => (
            ObservationKind::ResultRecorded,
            ObservationOutcome::Failed,
            INTERRUPTED_CALL_MESSAGE,
        ),
        Disposition::Denied => (
            ObservationKind::LifecycleTransitionRecorded,
            ObservationOutcome::Denied,
            DENIED_MESSAGE,
        ),
        Disposition::ResultRecovered => (
            ObservationKind::ResultRecorded,
            ObservationOutcome::Informational,
            RESULT_RECOVERED_MESSAGE,
        ),
        Disposition::Released => (
            ObservationKind::CallDenied,
            ObservationOutcome::Informational,
            RELEASED_MESSAGE,
        ),
    };
    let detail = serde_json::json!({
        "schema": "mct-call-restart-disposition-v1",
        "disposition": disposition.code(),
        "call_id": call_id.as_str(),
        "run_id": run.map(|run| run.run_id.as_str()),
        "run_state_before": run.map(|run| format!("{:?}", run.state)),
        "idempotency_scope": scope,
        "idempotency_fingerprint": fingerprint,
        "tenure": "previous_writer_tenure",
    });
    let trace_id = run.map(|run| run.trace_id.clone()).unwrap_or_else(|| {
        TraceId::new(format!("trace-restart-disposition:{digest}"))
            .expect("generated trace ID must be non-empty")
    });
    Ok(MctObservation {
        observation_id: ObservationId::new(format!(
            "obs:call-restart-{}:{digest}",
            disposition.code()
        ))?,
        observed_at: current_timestamp(),
        kind,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        // An unassociated historical peer reservation is not an execution fact.
        call_id: (!call_id.as_str().starts_with("legacy-peer-reservation:")).then_some(call_id),
        decision_id: run.and_then(|run| run.authority_decision_id.clone()),
        subject_id: scope.clone(),
        resource_id: fingerprint
            .clone()
            .or_else(|| run.map(|run| format!("run:{}", run.run_id))),
        policy_revision: run.map(|run| run.call.authority_context.policy_revision),
        grants_revision: run.map(|run| {
            run.call
                .authority_context
                .expected_receiver_grants_authority
                .generation
        }),
        outcome,
        visibility: ObservationVisibility::NodeOperator,
        safe_message: safe_message.into(),
        detail_ref: Some(format!("{DISPOSITION_DETAIL_PREFIX}{detail}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mct_daemon::MctRuntimeRunState;
    use mct_observation::MctObservationLedgerEntry;

    fn test_call(call_id: &str) -> MctCall {
        let mut call = local_wasm_call(
            OperationTarget {
                namespace: "patina:mct-test".into(),
                interface_name: "echo@0.1.0".into(),
                function_name: "echo".into(),
            },
            test_grants_authority_identity(1),
        );
        call.call_id = CallId::new(call_id).unwrap();
        call.trace_context.trace_id = TraceId::new(format!("trace-{call_id}")).unwrap();
        call
    }

    fn fingerprint(call_id: &str) -> MctIdempotencyFingerprint {
        MctIdempotencyFingerprint {
            target: "patina:mct-test/echo@0.1.0.echo".into(),
            call_id: CallId::new(call_id).unwrap(),
            payload_digest: "empty".into(),
        }
    }

    fn reserve(state: &MctRuntimeStateStore, key: &str, call_id: &str) {
        let now = Timestamp::new("2026-07-10T00:00:00Z").unwrap();
        let expires = Timestamp::new("2026-07-10T00:12:00Z").unwrap();
        assert_eq!(
            state
                .reserve_call_idempotency(
                    "local:test",
                    key,
                    &fingerprint(call_id),
                    &now,
                    &expires,
                    64
                )
                .unwrap(),
            MctIdempotencyReservation::ExecuteFresh
        );
    }

    fn disposition_facts(ledger_path: &Path) -> Vec<MctObservationLedgerEntry> {
        JsonlObservationLedger::open_read_only(ledger_path, "ledger-local", "local-mct")
            .unwrap()
            .entries()
            .unwrap()
            .into_iter()
            .filter(|entry| {
                entry
                    .observation
                    .detail_ref
                    .as_deref()
                    .is_some_and(|detail| detail.starts_with(DISPOSITION_DETAIL_PREFIX))
            })
            .collect()
    }

    #[tokio::test]
    async fn l8_legacy_peer_reservations_and_transport_results_cannot_adopt_local_runs() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        let call = test_call("colliding-wire-id");
        state
            .insert_run_started(
                "local-run",
                &call,
                RuntimeKind::Internal,
                None,
                current_timestamp_string(),
            )
            .unwrap();
        let now = Timestamp::new("2026-07-10T00:00:00Z").unwrap();
        state
            .reserve_call_idempotency(
                "peer-binding:attacker",
                "key",
                &fingerprint("colliding-wire-id"),
                &now,
                &Timestamp::new("2026-07-10T00:12:00Z").unwrap(),
                64,
            )
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let mut wire_denial = MctObservation::informational(
            ObservationId::new("transport-result").unwrap(),
            now,
            ObservationKind::ResultRecorded,
            call.trace_context.trace_id.clone(),
            "wire denial",
        );
        wire_denial.call_id = Some(call.call_id.clone());
        wire_denial.source_plane = SourcePlane::Peer;
        wire_denial.outcome = ObservationOutcome::Denied;
        ledger.append(vec![wire_denial]).await.unwrap();
        let report = settle_prior_tenure_reservations_and_runs(
            &state_path,
            &ledger_path,
            "local-mct",
            &ledger,
        )
        .await
        .unwrap();
        assert_eq!(
            report.denied, 0,
            "wire denial is not the local run's result"
        );
        assert_eq!(
            report.interrupted, 2,
            "ambiguous historical peer reservation stays unknown, independently of local execution"
        );
        assert_eq!(disposition_facts(&ledger_path).len(), 2);
        assert!(
            settle_prior_tenure_reservations_and_runs(
                &state_path,
                &ledger_path,
                "local-mct",
                &ledger
            )
            .await
            .unwrap()
            .is_empty()
        );
        ledger.close().await;
    }

    #[tokio::test]
    async fn prior_tenure_calls_settle_once_without_reexecution() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        // Started in the Child, never finished.
        reserve(&state, "key-running", "call-running");
        state
            .insert_run_started(
                "run-running",
                &test_call("call-running"),
                RuntimeKind::WasmComponent,
                None,
                "2026-07-10T00:00:01Z",
            )
            .unwrap();
        // Finished, but the reply was cut off before the key completed.
        reserve(&state, "key-finished", "call-finished");
        state
            .insert_run_started(
                "run-finished",
                &test_call("call-finished"),
                RuntimeKind::WasmComponent,
                None,
                "2026-07-10T00:00:01Z",
            )
            .unwrap();
        let finished = MctResult {
            requester_message: "done".into(),
            outcome: ResultOutcome::Success,
            ..interrupted_result(&state.get_run("run-finished").unwrap().unwrap())
        };
        state
            .complete_run("run-finished", &finished, "2026-07-10T00:00:02Z")
            .unwrap();
        // Reserved, but nothing began.
        reserve(&state, "key-unstarted", "call-unstarted");
        // A keyless run left running.
        state
            .insert_run_started(
                "run-keyless",
                &test_call("call-keyless"),
                RuntimeKind::WasmComponent,
                None,
                "2026-07-10T00:00:01Z",
            )
            .unwrap();
        drop(state);

        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let report = settle_prior_tenure_reservations_and_runs(
            &state_path,
            &ledger_path,
            "local-mct",
            &ledger,
        )
        .await
        .unwrap();
        assert_eq!(
            report,
            PriorTenureCallReport {
                interrupted: 2,
                denied: 0,
                released: 1,
                results_recovered: 1,
                trigger_firings_closed: 0,
            }
        );

        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        assert!(
            state
                .in_flight_call_idempotency_entries()
                .unwrap()
                .is_empty()
        );
        assert!(state.unfinished_runs().unwrap().is_empty());
        for run_id in ["run-running", "run-keyless"] {
            let run = state.get_run(run_id).unwrap().unwrap();
            assert_eq!(run.state, MctRuntimeRunState::Failed);
            assert_eq!(
                run.result.unwrap().requester_message,
                INTERRUPTED_CALL_MESSAGE
            );
        }
        assert_eq!(
            state.get_run("run-finished").unwrap().unwrap().result,
            Some(finished)
        );
        let now = Timestamp::new("2026-07-10T00:01:00Z").unwrap();
        let expires = Timestamp::new("2026-07-10T00:13:00Z").unwrap();
        let replay = |key: &str, call_id: &str| {
            state
                .reserve_call_idempotency(
                    "local:test",
                    key,
                    &fingerprint(call_id),
                    &now,
                    &expires,
                    64,
                )
                .unwrap()
        };
        match replay("key-running", "call-running") {
            MctIdempotencyReservation::Replay(reply) => {
                assert_eq!(reply.outcome, CallProtocolOutcome::Failed);
                assert_eq!(
                    reply.protocol_reason,
                    Some(CallProtocolReason::ExecutionInterrupted)
                );
                assert_eq!(reply.safe_message, INTERRUPTED_CALL_MESSAGE);
            }
            other => panic!("interrupted key did not replay: {other:?}"),
        }
        match replay("key-finished", "call-finished") {
            MctIdempotencyReservation::Replay(reply) => {
                assert_eq!(reply.outcome, CallProtocolOutcome::Completed);
            }
            other => panic!("finished key did not replay: {other:?}"),
        }
        assert_eq!(
            replay("key-unstarted", "call-unstarted"),
            MctIdempotencyReservation::ExecuteFresh,
            "a released key executes exactly once on retry"
        );
        // The fresh retry reservation is live work of this tenure.
        state
            .release_call_idempotency(
                "local:test",
                "key-unstarted",
                &fingerprint("call-unstarted"),
            )
            .unwrap();
        drop(state);

        let facts = disposition_facts(&ledger_path);
        assert_eq!(facts.len(), 4);
        // Settling again changes nothing and appends nothing.
        let again = settle_prior_tenure_reservations_and_runs(
            &state_path,
            &ledger_path,
            "local-mct",
            &ledger,
        )
        .await
        .unwrap();
        assert!(again.is_empty());
        ledger.close().await;
        assert_eq!(disposition_facts(&ledger_path).len(), 4);
    }

    #[tokio::test]
    async fn a_recovered_result_never_replays_a_lost_inline_payload_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        reserve(&state, "key-inline", "call-inline");
        state
            .insert_run_started(
                "run-inline",
                &test_call("call-inline"),
                RuntimeKind::WasmComponent,
                None,
                "2026-07-10T00:00:01Z",
            )
            .unwrap();
        let payload = b"{\"echo\":\"lost with the old tenure\"}";
        let handle = MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: "result-inline".into(),
            content_type: "application/json".into(),
            size_bytes: payload.len() as u64,
            blake3_digest_hex: blake3_hex(payload),
        };
        let finished = MctResult {
            requester_message: "done".into(),
            outcome: ResultOutcome::Success,
            result_payload: handle.clone(),
            ..interrupted_result(&state.get_run("run-inline").unwrap().unwrap())
        };
        state
            .complete_run("run-inline", &finished, "2026-07-10T00:00:02Z")
            .unwrap();
        drop(state);

        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let report = settle_prior_tenure_reservations_and_runs(
            &state_path,
            &ledger_path,
            "local-mct",
            &ledger,
        )
        .await
        .unwrap();
        ledger.close().await;
        assert_eq!(report.results_recovered, 1);

        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        // The run keeps the Child's original handle; only the bytes are gone.
        assert_eq!(
            state.get_run("run-inline").unwrap().unwrap().result,
            Some(finished)
        );
        let now = Timestamp::new("2026-07-10T00:01:00Z").unwrap();
        let expires = Timestamp::new("2026-07-10T00:13:00Z").unwrap();
        let reply = match state
            .reserve_call_idempotency(
                "local:test",
                "key-inline",
                &fingerprint("call-inline"),
                &now,
                &expires,
                64,
            )
            .unwrap()
        {
            MctIdempotencyReservation::Replay(reply) => reply,
            other => panic!("recovered key did not replay: {other:?}"),
        };
        assert_ne!(
            reply.outcome,
            CallProtocolOutcome::Completed,
            "a completed reply without the Child's bytes would claim an empty result"
        );
        assert_eq!(reply.outcome, CallProtocolOutcome::Failed);
        assert_eq!(
            reply.protocol_reason,
            Some(CallProtocolReason::ExecutionInterrupted)
        );
        assert!(reply.inline_result_payload.is_none());
        assert!(reply.safe_message.starts_with(RESULT_BYTES_LOST_MESSAGE));
        assert!(reply.safe_message.contains(&blake3_hex(payload)));
        assert!(
            reply
                .safe_message
                .contains(&format!("{} bytes", payload.len()))
        );
        assert_eq!(
            reply.result_ref.as_ref().map(|value| value.as_str()),
            Some("result-resident:call-inline")
        );
        // The replayed reply passes the wire's payload-integrity rule.
        assert_eq!(
            mct_kernel::evaluate_payload_integrity(
                mct_kernel::PayloadIntegritySubject::ReplyResult,
                &reply.result_payload,
                &mct_kernel::MctPayloadIntegrityObservation::missing_inline_bytes(),
                mct_iroh::MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES as u64,
            )
            .outcome,
            mct_kernel::PayloadIntegrityOutcome::Matched
        );
    }

    fn call_fact(
        id: &str,
        call_id: &str,
        kind: ObservationKind,
        outcome: ObservationOutcome,
        decision: Option<&str>,
        detail: Option<&str>,
    ) -> MctObservation {
        MctObservation {
            observation_id: ObservationId::new(id).unwrap(),
            observed_at: current_timestamp(),
            kind,
            source_plane: SourcePlane::Kernel,
            trace: ObservationTraceRef {
                trace_id: TraceId::new(format!("trace-{call_id}")).unwrap(),
                span_id: None,
                parent_span_id: None,
                external_trace_id: None,
            },
            call_id: Some(CallId::new(call_id).unwrap()),
            decision_id: decision.map(|decision| DecisionId::new(decision).unwrap()),
            subject_id: None,
            resource_id: None,
            policy_revision: None,
            grants_revision: None,
            outcome,
            visibility: ObservationVisibility::InternalOnly,
            safe_message: "test fact".into(),
            detail_ref: detail.map(Into::into),
        }
    }

    #[tokio::test]
    async fn a_call_the_ledger_already_denied_closes_as_denied_not_interrupted() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        let start = |run_id: &str, call_id: &str| {
            state
                .insert_run_started(
                    run_id,
                    &test_call(call_id),
                    RuntimeKind::WasmComponent,
                    None,
                    "2026-07-10T00:00:01Z",
                )
                .unwrap();
        };
        // Effect admission denied after the run started; crash before the
        // run completed and before the key completed.
        reserve(&state, "key-denied", "call-denied");
        start("run-denied-effect", "call-denied");
        // A keyless run denied the same way.
        start("run-keyless-denied", "call-keyless-denied");
        // A route denial recorded before its run record existed.
        reserve(&state, "key-route-denied", "call-route-denied");
        // An earlier denial the call then moved past: a later route was
        // selected and the run started, so the outcome is unknown.
        reserve(&state, "key-moved-on", "call-moved-on");
        start("run-moved-on", "call-moved-on");
        // A transient refusal released the key, then the retry crashed
        // before routing: the key is released again, never denied.
        reserve(&state, "key-transient", "call-transient");
        drop(state);

        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        use ObservationKind as K;
        use ObservationOutcome as O;
        ledger
            .append(vec![
                call_fact(
                    "f1",
                    "call-denied",
                    K::RouteSelected,
                    O::Allowed,
                    None,
                    None,
                ),
                call_fact(
                    "f2",
                    "call-denied",
                    K::ResultRecorded,
                    O::Denied,
                    Some("decision-effect-denied"),
                    Some("result_outcome:Denied"),
                ),
                call_fact(
                    "f3",
                    "call-keyless-denied",
                    K::ResultRecorded,
                    O::Denied,
                    Some("decision-keyless-denied"),
                    None,
                ),
                call_fact(
                    "f4",
                    "call-route-denied",
                    K::ResultRecorded,
                    O::Denied,
                    Some("decision-route-denied"),
                    Some("result_outcome:denied"),
                ),
                call_fact(
                    "f5",
                    "call-moved-on",
                    K::ResultRecorded,
                    O::Denied,
                    None,
                    None,
                ),
                call_fact(
                    "f6",
                    "call-moved-on",
                    K::RouteSelected,
                    O::Allowed,
                    None,
                    None,
                ),
                call_fact(
                    "f7",
                    "call-transient",
                    K::CallDenied,
                    O::Denied,
                    None,
                    Some(TRANSIENT_RELEASE_DETAIL),
                ),
                call_fact(
                    "f8",
                    "call-transient",
                    K::ResultRecorded,
                    O::Denied,
                    None,
                    None,
                ),
            ])
            .await
            .unwrap();

        let report = settle_prior_tenure_reservations_and_runs(
            &state_path,
            &ledger_path,
            "local-mct",
            &ledger,
        )
        .await
        .unwrap();
        assert_eq!(
            report,
            PriorTenureCallReport {
                interrupted: 1,
                denied: 3,
                released: 1,
                results_recovered: 0,
                trigger_firings_closed: 0,
            }
        );

        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        for (run_id, decision) in [
            ("run-denied-effect", "decision-effect-denied"),
            ("run-keyless-denied", "decision-keyless-denied"),
        ] {
            let run = state.get_run(run_id).unwrap().unwrap();
            assert_eq!(run.state, MctRuntimeRunState::Denied, "{run_id}");
            let result = run.result.unwrap();
            assert_eq!(result.outcome, ResultOutcome::Denied);
            assert_eq!(result.authority_decision_ref.as_str(), decision);
        }
        let moved_on = state.get_run("run-moved-on").unwrap().unwrap();
        assert_eq!(moved_on.state, MctRuntimeRunState::Failed);
        assert_eq!(
            moved_on.result.unwrap().requester_message,
            INTERRUPTED_CALL_MESSAGE
        );

        let now = Timestamp::new("2026-07-10T00:01:00Z").unwrap();
        let expires = Timestamp::new("2026-07-10T00:13:00Z").unwrap();
        let replay = |key: &str, call_id: &str| {
            state
                .reserve_call_idempotency(
                    "local:test",
                    key,
                    &fingerprint(call_id),
                    &now,
                    &expires,
                    64,
                )
                .unwrap()
        };
        for (key, call_id, decision) in [
            ("key-denied", "call-denied", "decision-effect-denied"),
            (
                "key-route-denied",
                "call-route-denied",
                "decision-route-denied",
            ),
        ] {
            match replay(key, call_id) {
                MctIdempotencyReservation::Replay(reply) => {
                    assert_eq!(reply.outcome, CallProtocolOutcome::Denied, "{key}");
                    assert_ne!(
                        reply.protocol_reason,
                        Some(CallProtocolReason::ExecutionInterrupted),
                        "{key}: a denial is never reported as interrupted"
                    );
                    assert_eq!(
                        reply.route_decision_id.as_ref().map(|id| id.as_str()),
                        Some(decision)
                    );
                }
                other => panic!("denied key {key} did not replay: {other:?}"),
            }
        }
        match replay("key-moved-on", "call-moved-on") {
            MctIdempotencyReservation::Replay(reply) => {
                assert_eq!(reply.outcome, CallProtocolOutcome::Failed);
                assert_eq!(
                    reply.protocol_reason,
                    Some(CallProtocolReason::ExecutionInterrupted)
                );
                assert_eq!(reply.safe_message, INTERRUPTED_CALL_MESSAGE);
            }
            other => panic!("moved-on key did not replay: {other:?}"),
        }
        assert_eq!(
            replay("key-transient", "call-transient"),
            MctIdempotencyReservation::ExecuteFresh
        );
        state
            .release_call_idempotency(
                "local:test",
                "key-transient",
                &fingerprint("call-transient"),
            )
            .unwrap();
        drop(state);

        let facts = disposition_facts(&ledger_path);
        let denied_facts = facts
            .iter()
            .filter(|entry| {
                entry
                    .observation
                    .observation_id
                    .as_str()
                    .starts_with("obs:call-restart-denied:")
            })
            .count();
        assert_eq!(denied_facts, 3);
        assert!(facts.iter().all(|entry| {
            !(entry
                .observation
                .observation_id
                .as_str()
                .starts_with(INTERRUPTED_OBSERVATION_PREFIX)
                && matches!(
                    entry.observation.call_id.as_ref().map(|id| id.as_str()),
                    Some("call-denied" | "call-keyless-denied" | "call-route-denied")
                ))
        }));
        let again = settle_prior_tenure_reservations_and_runs(
            &state_path,
            &ledger_path,
            "local-mct",
            &ledger,
        )
        .await
        .unwrap();
        assert!(again.is_empty());
        ledger.close().await;
        assert_eq!(disposition_facts(&ledger_path).len(), facts.len());
    }

    #[tokio::test]
    async fn a_crash_after_the_fact_finishes_without_a_duplicate_fact() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        reserve(&state, "key-running", "call-running");
        state
            .insert_run_started(
                "run-running",
                &test_call("call-running"),
                RuntimeKind::WasmComponent,
                None,
                "2026-07-10T00:00:01Z",
            )
            .unwrap();
        let entry = state
            .in_flight_call_idempotency_entries()
            .unwrap()
            .remove(0);
        let run = state.get_run("run-running").unwrap();
        drop(state);
        // The fact reached the ledger; the state change did not.
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        ledger
            .append(vec![
                settlement_observation(&Settlement::Reservation {
                    entry,
                    run,
                    disposition: Disposition::Interrupted,
                })
                .unwrap(),
            ])
            .await
            .unwrap();

        let report = settle_prior_tenure_reservations_and_runs(
            &state_path,
            &ledger_path,
            "local-mct",
            &ledger,
        )
        .await
        .unwrap();
        assert_eq!(report.interrupted, 1);
        ledger.close().await;
        assert_eq!(disposition_facts(&ledger_path).len(), 1);
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        assert!(
            state
                .in_flight_call_idempotency_entries()
                .unwrap()
                .is_empty()
        );
        assert!(state.unfinished_runs().unwrap().is_empty());
    }
}

//! Durable bounded temporal trigger scheduling for the resident Mother.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const MCT_TRIGGER_SCHEDULER_POLL_MS: u64 = 50;
pub(crate) const MCT_TRIGGER_MAX_EVALUATIONS_PER_TURN: usize = 32;
pub(crate) const MCT_TRIGGER_MAX_RECOVERY_RANGE_OCCURRENCES: u64 = 4096;
pub(crate) const MCT_TRIGGER_MAX_PENDING_PER_RECORD: usize = 16;
pub(crate) const MCT_TRIGGER_MAX_PENDING_RESIDENT: usize = 256;
pub(crate) const MCT_TRIGGER_MAX_ACTIVE_CALLS: usize = 8;
const MCT_TRIGGER_EXECUTION_RETRY_MS: u64 = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TriggerLimits {
    pub(crate) max_evaluations_per_turn: usize,
    pub(crate) max_recovery_range_occurrences: u64,
    pub(crate) max_pending_per_record: usize,
    pub(crate) max_pending_resident: usize,
    pub(crate) max_active_calls: usize,
}

impl Default for TriggerLimits {
    fn default() -> Self {
        Self {
            max_evaluations_per_turn: MCT_TRIGGER_MAX_EVALUATIONS_PER_TURN,
            max_recovery_range_occurrences: MCT_TRIGGER_MAX_RECOVERY_RANGE_OCCURRENCES,
            max_pending_per_record: MCT_TRIGGER_MAX_PENDING_PER_RECORD,
            max_pending_resident: MCT_TRIGGER_MAX_PENDING_RESIDENT,
            max_active_calls: MCT_TRIGGER_MAX_ACTIVE_CALLS,
        }
    }
}

pub(super) trait TriggerClock: Send + Sync {
    fn now(&self) -> Timestamp;
}

#[derive(Debug)]
pub(super) struct SystemTriggerClock;

impl TriggerClock for SystemTriggerClock {
    fn now(&self) -> Timestamp {
        current_timestamp()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DueOccurrenceRange {
    first_index: u64,
    last_index: u64,
    first_at: Timestamp,
    last_at: Timestamp,
    count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FireLateRecoveryPlan {
    admitted: u64,
    refused: Option<u64>,
}

impl FireLateRecoveryPlan {
    fn evaluations(self) -> usize {
        usize::try_from(self.admitted).unwrap_or(usize::MAX) + usize::from(self.refused.is_some())
    }
}

fn fire_late_recovery_plan(
    missed_count: u64,
    recovery_limit: u64,
    turn_budget: usize,
) -> FireLateRecoveryPlan {
    if missed_count == 0 || turn_budget == 0 {
        return FireLateRecoveryPlan {
            admitted: 0,
            refused: None,
        };
    }
    let unconstrained_admitted = missed_count.min(recovery_limit).min(turn_budget as u64);
    if unconstrained_admitted == missed_count {
        return FireLateRecoveryPlan {
            admitted: unconstrained_admitted,
            refused: None,
        };
    }
    let admitted = unconstrained_admitted.min(turn_budget.saturating_sub(1) as u64);
    FireLateRecoveryPlan {
        admitted,
        refused: Some(missed_count - admitted),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SchedulerAdmissionDecision {
    FireNow,
    Pending(CallTriggerPendingReason),
    CoalescedInto(CallTriggerPendingOccurrenceId),
    Terminal {
        disposition: MctTriggerOccurrenceDisposition,
        stage: &'static str,
    },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct TriggerPendingProjectionEvidence {
    occurrence: MctTriggerOccurrenceRecord,
    pending: MctTriggerPendingOccurrenceRecord,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct TriggerFiringProjectionEvidence {
    occurrence: MctTriggerOccurrenceRecord,
    firing: MctTriggerFiringRecord,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct TriggerCoalescedProjectionEvidence {
    pending_occurrence_id: CallTriggerPendingOccurrenceId,
    represented_set: CallTriggerRepresentedSet,
    nominal_at: Timestamp,
    observation_id: ObservationId,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct TriggerPendingSuppressedProjectionEvidence {
    pending_occurrence_id: CallTriggerPendingOccurrenceId,
    observation_id: ObservationId,
    suppressed_at: Timestamp,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct TriggerFiringCompletedProjectionEvidence {
    firing_id: CallTriggerFiringId,
    target_result_ref: ResultRef,
    completed_at: Timestamp,
}

struct TriggerScheduler {
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    clock: Arc<dyn TriggerClock>,
    limits: TriggerLimits,
    active_capacity: Arc<tokio::sync::Semaphore>,
    running_firings: BTreeSet<CallTriggerFiringId>,
    retry_after: BTreeMap<CallTriggerFiringId, Timestamp>,
    completion_tx: tokio::sync::mpsc::UnboundedSender<CallTriggerFiringId>,
    completion_rx: tokio::sync::mpsc::UnboundedReceiver<CallTriggerFiringId>,
    last_turn: Timestamp,
}

fn timestamp_nanos(timestamp: &Timestamp) -> Result<i128> {
    Ok(timestamp
        .as_str()
        .parse::<jiff::Timestamp>()?
        .as_nanosecond())
}

fn timestamp_from_nanos(nanos: i128) -> Result<Timestamp> {
    Ok(Timestamp::new(
        jiff::Timestamp::from_nanosecond(nanos)?.to_string(),
    )?)
}

fn ceil_div_nonnegative(numerator: i128, denominator: i128) -> i128 {
    if numerator <= 0 {
        0
    } else {
        (numerator + denominator - 1) / denominator
    }
}

fn temporal_due_range(
    authority: &CallTriggerAuthority,
    after: Option<&Timestamp>,
    through: &Timestamp,
) -> Result<Option<DueOccurrenceRange>> {
    let CallTriggerSource::Temporal {
        anchor_at,
        interval_ms,
    } = &authority.trigger_source
    else {
        return Ok(None);
    };
    let interval = i128::from(*interval_ms) * 1_000_000;
    let anchor = timestamp_nanos(anchor_at)?;
    let starts = timestamp_nanos(&authority.starts_at)?;
    let expires = timestamp_nanos(&authority.expires_at)?;
    let through = timestamp_nanos(through)?.min(expires - 1);
    if through < starts || through < anchor {
        return Ok(None);
    }
    let mut first_index = ceil_div_nonnegative(starts - anchor, interval);
    if let Some(after) = after {
        let after = timestamp_nanos(after)?;
        if after >= anchor {
            first_index = first_index.max((after - anchor) / interval + 1);
        }
    }
    let last_index = (through - anchor) / interval;
    if first_index > last_index {
        return Ok(None);
    }
    let first_nanos = anchor
        .checked_add(
            first_index
                .checked_mul(interval)
                .context("trigger first index overflow")?,
        )
        .context("trigger first occurrence overflow")?;
    let last_nanos = anchor
        .checked_add(
            last_index
                .checked_mul(interval)
                .context("trigger last index overflow")?,
        )
        .context("trigger last occurrence overflow")?;
    Ok(Some(DueOccurrenceRange {
        first_index: u64::try_from(first_index).context("trigger first index out of range")?,
        last_index: u64::try_from(last_index).context("trigger last index out of range")?,
        first_at: timestamp_from_nanos(first_nanos)?,
        last_at: timestamp_from_nanos(last_nanos)?,
        count: u64::try_from(last_index - first_index + 1)
            .context("trigger occurrence count out of range")?,
    }))
}

fn occurrence_at(
    authority: &CallTriggerAuthority,
    index: u64,
) -> Result<(CallTriggerOccurrenceId, Timestamp)> {
    let CallTriggerSource::Temporal {
        anchor_at,
        interval_ms,
    } = &authority.trigger_source
    else {
        bail!("event occurrence requires MotherEventSourceAdapterRuntime");
    };
    let nanos = timestamp_nanos(anchor_at)?
        .checked_add(
            i128::from(index)
                .checked_mul(i128::from(*interval_ms) * 1_000_000)
                .context("trigger occurrence multiplication overflow")?,
        )
        .context("trigger occurrence time overflow")?;
    let nominal_at = timestamp_from_nanos(nanos)?;
    let occurrence_id = derive_temporal_occurrence_identity(
        authority.trigger_authority_id.as_str(),
        authority.record_revision,
        nominal_at.as_str(),
    );
    Ok((occurrence_id, nominal_at))
}

fn represented_range(
    authority: &CallTriggerAuthority,
    range: &DueOccurrenceRange,
) -> Result<CallTriggerRepresentedSet> {
    let (first, _) = occurrence_at(authority, range.first_index)?;
    let (last, _) = occurrence_at(authority, range.last_index)?;
    trigger_represented_set_from_bounds(
        &authority.trigger_authority_id,
        authority.record_revision,
        first,
        last,
        range.count,
    )
    .map_err(Into::into)
}

fn singleton_candidate(
    authority: &CallTriggerAuthority,
    index: u64,
) -> Result<(CallTriggerOccurrenceCandidate, Timestamp)> {
    let (occurrence_id, nominal_at) = occurrence_at(authority, index)?;
    let represented_set = trigger_represented_set_from_bounds(
        &authority.trigger_authority_id,
        authority.record_revision,
        occurrence_id.clone(),
        occurrence_id.clone(),
        1,
    )?;
    Ok((
        CallTriggerOccurrenceCandidate {
            occurrence_id,
            represented_set,
        },
        nominal_at,
    ))
}

#[allow(clippy::too_many_arguments)] // Fixed stages remain explicit at every call site.
fn evaluate_scheduler_admission(
    missed_terminal: Option<MctTriggerOccurrenceDisposition>,
    overlap_policy: OverlapPolicy,
    active_firing: Option<&MctTriggerFiringRecord>,
    existing_coalesced: Option<&MctTriggerPendingOccurrenceRecord>,
    pending_per_record: usize,
    pending_resident: usize,
    active_resident: usize,
    limits: TriggerLimits,
) -> SchedulerAdmissionDecision {
    if let Some(disposition) = missed_terminal {
        return SchedulerAdmissionDecision::Terminal {
            disposition,
            stage: "missed_fire",
        };
    }
    let overlap = evaluate_overlap_policy(
        overlap_policy,
        active_firing.is_some(),
        pending_per_record,
        limits.max_pending_per_record,
        existing_coalesced.map(|pending| pending.pending_occurrence_id.clone()),
    );
    match overlap {
        CallTriggerOverlapDecision::Suppressed => SchedulerAdmissionDecision::Terminal {
            disposition: MctTriggerOccurrenceDisposition::Suppressed,
            stage: "overlap",
        },
        CallTriggerOverlapDecision::CapacityRefused => SchedulerAdmissionDecision::Terminal {
            disposition: MctTriggerOccurrenceDisposition::CapacityRefused,
            stage: "per_record_pending_capacity",
        },
        CallTriggerOverlapDecision::CoalescedInto {
            pending_occurrence_id,
        } => SchedulerAdmissionDecision::CoalescedInto(pending_occurrence_id),
        CallTriggerOverlapDecision::Pending { reason } => {
            if pending_per_record >= limits.max_pending_per_record {
                SchedulerAdmissionDecision::Terminal {
                    disposition: MctTriggerOccurrenceDisposition::CapacityRefused,
                    stage: "per_record_pending_capacity",
                }
            } else if pending_resident >= limits.max_pending_resident {
                SchedulerAdmissionDecision::Terminal {
                    disposition: MctTriggerOccurrenceDisposition::CapacityRefused,
                    stage: "resident_pending_capacity",
                }
            } else {
                SchedulerAdmissionDecision::Pending(reason)
            }
        }
        CallTriggerOverlapDecision::FireNow if active_resident >= limits.max_active_calls => {
            SchedulerAdmissionDecision::Terminal {
                disposition: MctTriggerOccurrenceDisposition::CapacityRefused,
                stage: "resident_active_capacity",
            }
        }
        CallTriggerOverlapDecision::FireNow => SchedulerAdmissionDecision::FireNow,
    }
}

#[allow(clippy::too_many_arguments)] // Evidence inputs mirror the closed trigger entity.
fn trigger_observation(
    authority: &CallTriggerAuthority,
    occurrence_id: &CallTriggerOccurrenceId,
    represented_set: &CallTriggerRepresentedSet,
    nominal_at: Option<&Timestamp>,
    kind: ObservationKind,
    outcome: ObservationOutcome,
    safe_message: &str,
    stage: &str,
    call_id: Option<CallId>,
) -> MctObservation {
    let observation_id = ObservationId::new(format!(
        "obs:trigger:{stage}:{}:{}",
        authority.trigger_authority_id, occurrence_id
    ))
    .expect("generated trigger observation id must be non-empty");
    MctObservation {
        observation_id,
        observed_at: current_timestamp(),
        kind,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace:trigger:{occurrence_id}"))
                .expect("generated trigger trace id must be non-empty"),
            span_id: Some(
                SpanId::new(format!("span:trigger:{occurrence_id}"))
                    .expect("generated trigger span id must be non-empty"),
            ),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id,
        decision_id: None,
        subject_id: Some(authority.trigger_authority_id.to_string()),
        resource_id: Some(represented_set.represented_set_ref.clone()),
        policy_revision: Some(authority.policy_revision),
        grants_revision: None,
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: Some(
            serde_json::json!({
                "trigger_authority_id": authority.trigger_authority_id,
                "record_revision": authority.record_revision,
                "policy_revision": authority.policy_revision,
                "occurrence_id": occurrence_id,
                "represented_set": represented_set,
                "nominal_at": nominal_at,
                "stage": stage,
            })
            .to_string(),
        ),
    }
}

#[allow(clippy::too_many_arguments)] // Projection construction keeps every policy fact visible.
fn occurrence_record(
    authority: &CallTriggerAuthority,
    candidate: &CallTriggerOccurrenceCandidate,
    nominal_at: Option<Timestamp>,
    missed_fire_result: &str,
    overlap_result: Option<String>,
    disposition: MctTriggerOccurrenceDisposition,
    observation_id: ObservationId,
    now: Timestamp,
) -> MctTriggerOccurrenceRecord {
    MctTriggerOccurrenceRecord {
        occurrence_id: candidate.occurrence_id.clone(),
        trigger_authority_id: authority.trigger_authority_id.clone(),
        record_revision: authority.record_revision,
        nominal_at,
        represented_set: candidate.represented_set.clone(),
        missed_fire_result: missed_fire_result.into(),
        overlap_result,
        final_disposition: disposition,
        disposition_observation_id: observation_id,
        created_at: now,
    }
}

fn terminal_occurrence_id(
    disposition: MctTriggerOccurrenceDisposition,
    represented_set: &CallTriggerRepresentedSet,
) -> CallTriggerOccurrenceId {
    CallTriggerOccurrenceId::new(format!(
        "occurrence-terminal:{disposition:?}:{}",
        blake3_hex(represented_set.represented_set_ref.as_bytes())
    ))
    .expect("generated terminal occurrence id must be non-empty")
}

impl TriggerScheduler {
    fn new(
        paths: ResidentRuntimePaths,
        ledger: ResidentLedgerWriter,
        clock: Arc<dyn TriggerClock>,
        limits: TriggerLimits,
    ) -> Self {
        let now = clock.now();
        let (completion_tx, completion_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            paths,
            ledger,
            clock,
            limits,
            active_capacity: Arc::new(tokio::sync::Semaphore::new(limits.max_active_calls)),
            running_firings: BTreeSet::new(),
            retry_after: BTreeMap::new(),
            completion_tx,
            completion_rx,
            last_turn: now,
        }
    }

    async fn record_terminal_range(
        &self,
        authority: &CallTriggerAuthority,
        range: &DueOccurrenceRange,
        disposition: MctTriggerOccurrenceDisposition,
        stage: &'static str,
        missed_result: &'static str,
    ) -> Result<()> {
        let represented_set = represented_range(authority, range)?;
        let occurrence_id = terminal_occurrence_id(disposition, &represented_set);
        let mut observation = trigger_observation(
            authority,
            &occurrence_id,
            &represented_set,
            Some(&range.last_at),
            ObservationKind::LifecycleTransitionRecorded,
            match disposition {
                MctTriggerOccurrenceDisposition::Skipped => ObservationOutcome::Informational,
                _ => ObservationOutcome::Denied,
            },
            match disposition {
                MctTriggerOccurrenceDisposition::Skipped => "trigger occurrence skipped",
                MctTriggerOccurrenceDisposition::Suppressed => "trigger occurrence suppressed",
                MctTriggerOccurrenceDisposition::CapacityRefused => {
                    "trigger occurrence capacity refused"
                }
                MctTriggerOccurrenceDisposition::Pending
                | MctTriggerOccurrenceDisposition::Fired => "trigger occurrence terminal",
            },
            stage,
            None,
        );
        let record = MctTriggerOccurrenceRecord {
            occurrence_id,
            trigger_authority_id: authority.trigger_authority_id.clone(),
            record_revision: authority.record_revision,
            nominal_at: Some(range.last_at.clone()),
            represented_set,
            missed_fire_result: missed_result.into(),
            overlap_result: Some(stage.into()),
            final_disposition: disposition,
            disposition_observation_id: observation.observation_id.clone(),
            created_at: self.clock.now(),
        };
        observation.detail_ref = Some(format!(
            "call-trigger-occurrence-v1:{}",
            serde_json::to_string(&record)?
        ));
        self.ledger.append(vec![observation]).await?;
        MctRuntimeStateStore::open(self.paths.state_path())?
            .insert_call_trigger_occurrence(&record)?;
        Ok(())
    }

    async fn process_candidate(
        &mut self,
        authority: &CallTriggerAuthority,
        candidate: CallTriggerOccurrenceCandidate,
        nominal_at: Timestamp,
        missed_fire_result: &'static str,
    ) -> Result<()> {
        let state = MctRuntimeStateStore::open(self.paths.state_path())?;
        if state.call_trigger_occurrence_exists(&candidate.occurrence_id)? {
            return Ok(());
        }
        let active = state.active_call_trigger_firing(&authority.trigger_authority_id)?;
        let existing_coalesced =
            state.coalesced_call_trigger_pending_occurrence(&authority.trigger_authority_id)?;
        let (pending_per_record, pending_resident) =
            state.call_trigger_pending_counts(&authority.trigger_authority_id)?;
        let active_resident = state.summary()?.active_trigger_firings as usize;
        let decision = evaluate_scheduler_admission(
            None,
            authority.overlap_policy,
            active.as_ref(),
            existing_coalesced.as_ref(),
            pending_per_record,
            pending_resident,
            active_resident,
            self.limits,
        );
        drop(state);

        match decision {
            SchedulerAdmissionDecision::FireNow => {
                let Some(permit) = Arc::clone(&self.active_capacity).try_acquire_owned().ok()
                else {
                    return self
                        .record_terminal_candidate(
                            authority,
                            candidate,
                            nominal_at,
                            MctTriggerOccurrenceDisposition::CapacityRefused,
                            "resident_active_capacity",
                            missed_fire_result,
                        )
                        .await;
                };
                self.admit_firing(authority, candidate, nominal_at, missed_fire_result, permit)
                    .await
            }
            SchedulerAdmissionDecision::Pending(reason) => {
                self.admit_pending(authority, candidate, nominal_at, missed_fire_result, reason)
                    .await
            }
            SchedulerAdmissionDecision::CoalescedInto(pending_id) => {
                let state = MctRuntimeStateStore::open(self.paths.state_path())?;
                let existing = state
                    .coalesced_call_trigger_pending_occurrence(&authority.trigger_authority_id)?
                    .context("coalesced pending occurrence disappeared")?;
                let represented_set = trigger_represented_set_from_bounds(
                    &authority.trigger_authority_id,
                    existing.record_revision,
                    existing.represented_set.first_occurrence_id,
                    candidate.represented_set.last_occurrence_id,
                    existing
                        .represented_set
                        .count
                        .checked_add(candidate.represented_set.count)
                        .context("coalesced trigger count overflow")?,
                )?;
                let mut observation = trigger_observation(
                    authority,
                    &candidate.occurrence_id,
                    &represented_set,
                    Some(&nominal_at),
                    ObservationKind::LifecycleTransitionRecorded,
                    ObservationOutcome::Allowed,
                    "trigger overlap coalesced into pending occurrence",
                    "overlap_coalesce",
                    None,
                );
                let evidence = TriggerCoalescedProjectionEvidence {
                    pending_occurrence_id: pending_id.clone(),
                    represented_set: represented_set.clone(),
                    nominal_at: nominal_at.clone(),
                    observation_id: observation.observation_id.clone(),
                };
                observation.detail_ref = Some(format!(
                    "call-trigger-coalesced-v1:{}",
                    serde_json::to_string(&evidence)?
                ));
                self.ledger.append(vec![observation]).await?;
                state.update_coalesced_call_trigger_pending_occurrence(
                    &pending_id,
                    &represented_set,
                    Some(&nominal_at),
                    &evidence.observation_id,
                )
            }
            SchedulerAdmissionDecision::Terminal { disposition, stage } => {
                self.record_terminal_candidate(
                    authority,
                    candidate,
                    nominal_at,
                    disposition,
                    stage,
                    missed_fire_result,
                )
                .await
            }
        }
    }

    async fn record_terminal_candidate(
        &self,
        authority: &CallTriggerAuthority,
        candidate: CallTriggerOccurrenceCandidate,
        nominal_at: Timestamp,
        disposition: MctTriggerOccurrenceDisposition,
        stage: &'static str,
        missed_fire_result: &'static str,
    ) -> Result<()> {
        let mut observation = trigger_observation(
            authority,
            &candidate.occurrence_id,
            &candidate.represented_set,
            Some(&nominal_at),
            ObservationKind::LifecycleTransitionRecorded,
            if disposition == MctTriggerOccurrenceDisposition::Skipped {
                ObservationOutcome::Informational
            } else {
                ObservationOutcome::Denied
            },
            "trigger occurrence terminal before call",
            stage,
            None,
        );
        let record = occurrence_record(
            authority,
            &candidate,
            Some(nominal_at),
            missed_fire_result,
            Some(stage.into()),
            disposition,
            observation.observation_id.clone(),
            self.clock.now(),
        );
        observation.detail_ref = Some(format!(
            "call-trigger-occurrence-v1:{}",
            serde_json::to_string(&record)?
        ));
        self.ledger.append(vec![observation]).await?;
        MctRuntimeStateStore::open(self.paths.state_path())?.insert_call_trigger_occurrence(&record)
    }

    async fn admit_pending(
        &self,
        authority: &CallTriggerAuthority,
        candidate: CallTriggerOccurrenceCandidate,
        nominal_at: Timestamp,
        missed_fire_result: &'static str,
        reason: CallTriggerPendingReason,
    ) -> Result<()> {
        let state = MctRuntimeStateStore::open(self.paths.state_path())?;
        let sequence = state.next_call_trigger_pending_sequence(&authority.trigger_authority_id)?;
        let pending_id = derive_trigger_pending_identity(&candidate.occurrence_id);
        let mut observation = trigger_observation(
            authority,
            &candidate.occurrence_id,
            &candidate.represented_set,
            Some(&nominal_at),
            ObservationKind::LifecycleTransitionRecorded,
            ObservationOutcome::Allowed,
            "trigger occurrence admitted pending",
            "pending_admitted",
            None,
        );
        let occurrence = occurrence_record(
            authority,
            &candidate,
            Some(nominal_at),
            missed_fire_result,
            Some(format!("{reason:?}")),
            MctTriggerOccurrenceDisposition::Pending,
            observation.observation_id.clone(),
            self.clock.now(),
        );
        let pending = MctTriggerPendingOccurrenceRecord {
            pending_occurrence_id: pending_id,
            occurrence_id: candidate.occurrence_id,
            trigger_authority_id: authority.trigger_authority_id.clone(),
            record_revision: authority.record_revision,
            policy_revision: authority.policy_revision,
            admission_sequence: sequence,
            pending_reason: reason,
            represented_set: candidate.represented_set,
            admission_observation_id: observation.observation_id.clone(),
            state: "pending".into(),
            admitted_at: self.clock.now(),
            consumed_at: None,
        };
        observation.detail_ref = Some(format!(
            "call-trigger-pending-v1:{}",
            serde_json::to_string(&TriggerPendingProjectionEvidence {
                occurrence: occurrence.clone(),
                pending: pending.clone(),
            })?
        ));
        self.ledger.append(vec![observation]).await?;
        state.insert_call_trigger_pending_occurrence(&occurrence, &pending)
    }

    async fn admit_firing(
        &mut self,
        authority: &CallTriggerAuthority,
        candidate: CallTriggerOccurrenceCandidate,
        nominal_at: Timestamp,
        missed_fire_result: &'static str,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Result<()> {
        let current = MctRuntimeStateStore::open(self.paths.state_path())?
            .current_call_trigger_authority(&authority.trigger_authority_id)?;
        if current.as_ref() != Some(authority) || !authority.is_current_at(&self.clock.now()) {
            drop(permit);
            return self
                .record_terminal_candidate(
                    authority,
                    candidate,
                    nominal_at,
                    MctTriggerOccurrenceDisposition::Suppressed,
                    "fresh_current_law",
                    missed_fire_result,
                )
                .await;
        }
        local_blob_store_for_state_path(self.paths.state_path())
            .fetch(&authority.payload_constraint)
            .map_err(anyhow::Error::from)
            .context("fresh trigger payload unavailable")?;

        let firing_id = derive_trigger_firing_identity(&candidate.occurrence_id);
        let call_id = derive_trigger_call_identity(&candidate.occurrence_id);
        let idempotency_key = derive_trigger_idempotency_key(
            &authority.trigger_authority_id,
            authority.record_revision,
            &candidate.occurrence_id,
        );
        let mut observation = trigger_observation(
            authority,
            &candidate.occurrence_id,
            &candidate.represented_set,
            Some(&nominal_at),
            ObservationKind::CallConstructed,
            ObservationOutcome::Allowed,
            "trigger firing constructed durable call",
            "firing",
            Some(call_id.clone()),
        );
        let occurrence = occurrence_record(
            authority,
            &candidate,
            Some(nominal_at),
            missed_fire_result,
            Some("fire_now".into()),
            MctTriggerOccurrenceDisposition::Fired,
            observation.observation_id.clone(),
            self.clock.now(),
        );
        let firing = MctTriggerFiringRecord {
            firing_id: firing_id.clone(),
            occurrence_id: candidate.occurrence_id,
            trigger_authority_id: authority.trigger_authority_id.clone(),
            record_revision: authority.record_revision,
            policy_revision: authority.policy_revision,
            call_id,
            idempotency_key_ref: format!("blake3:{}", blake3_hex(idempotency_key.as_bytes())),
            firing_observation_id: observation.observation_id.clone(),
            target_result_ref: None,
            state: "active".into(),
            fired_at: self.clock.now(),
            completed_at: None,
        };
        observation.detail_ref = Some(format!(
            "call-trigger-firing-v1:{}",
            serde_json::to_string(&TriggerFiringProjectionEvidence {
                occurrence: occurrence.clone(),
                firing: firing.clone(),
            })?
        ));
        self.ledger.append(vec![observation]).await?;
        MctRuntimeStateStore::open(self.paths.state_path())?
            .insert_call_trigger_firing(&occurrence, &firing)?;
        self.spawn_firing(authority.clone(), firing, idempotency_key, permit)
    }

    fn spawn_firing(
        &mut self,
        authority: CallTriggerAuthority,
        firing: MctTriggerFiringRecord,
        idempotency_key: String,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Result<()> {
        if !self.running_firings.insert(firing.firing_id.clone()) {
            return Ok(());
        }
        let paths = self.paths.clone();
        let ledger = self.ledger.clone();
        let firing_id = firing.firing_id.clone();
        let completed_firing_id = firing_id.clone();
        let completion_tx = self.completion_tx.clone();
        tokio::spawn(async move {
            let _permit = permit;
            if let Err(error) =
                execute_trigger_firing(paths, ledger, authority, firing, idempotency_key).await
            {
                eprintln!("resident trigger firing execution unavailable: {error}");
            }
            let _ = completion_tx.send(completed_firing_id);
        });
        let retry_at = self
            .clock
            .now()
            .as_str()
            .parse::<jiff::Timestamp>()?
            .checked_add(jiff::SignedDuration::from_millis(
                MCT_TRIGGER_EXECUTION_RETRY_MS as i64,
            ))?;
        self.retry_after
            .insert(firing_id, Timestamp::new(retry_at.to_string())?);
        Ok(())
    }

    async fn recover_active_firings(&mut self) -> Result<()> {
        let now = self.clock.now();
        let state = MctRuntimeStateStore::open(self.paths.state_path())?;
        for firing in state.active_call_trigger_firings()? {
            if self.running_firings.contains(&firing.firing_id)
                || self
                    .retry_after
                    .get(&firing.firing_id)
                    .is_some_and(|retry_at| retry_at > &now)
            {
                continue;
            }
            let Some(authority) = state
                .call_trigger_authorities()?
                .into_iter()
                .find(|authority| {
                    authority.trigger_authority_id == firing.trigger_authority_id
                        && authority.record_revision == firing.record_revision
                })
            else {
                continue;
            };
            let Some(permit) = Arc::clone(&self.active_capacity).try_acquire_owned().ok() else {
                break;
            };
            let key = derive_trigger_idempotency_key(
                &authority.trigger_authority_id,
                authority.record_revision,
                &firing.occurrence_id,
            );
            self.spawn_firing(authority, firing, key, permit)?;
        }
        Ok(())
    }

    async fn drain_pending(&mut self) -> Result<()> {
        let state = MctRuntimeStateStore::open(self.paths.state_path())?;
        for pending in state.call_trigger_pending_occurrences()? {
            if state
                .active_call_trigger_firing(&pending.trigger_authority_id)?
                .is_some()
            {
                continue;
            }
            let current = state.current_call_trigger_authority(&pending.trigger_authority_id)?;
            let Some(authority) = current.filter(|authority| {
                authority.record_revision == pending.record_revision
                    && authority.policy_revision == pending.policy_revision
                    && authority.is_current_at(&self.clock.now())
            }) else {
                let mut observation = trigger_observation(
                    &state
                        .call_trigger_authorities()?
                        .into_iter()
                        .find(|authority| {
                            authority.trigger_authority_id == pending.trigger_authority_id
                                && authority.record_revision == pending.record_revision
                        })
                        .context("pending trigger authority revision missing")?,
                    &pending.occurrence_id,
                    &pending.represented_set,
                    None,
                    ObservationKind::LifecycleTransitionRecorded,
                    ObservationOutcome::Denied,
                    "pending trigger occurrence suppressed by current law",
                    "dequeue_current_law",
                    None,
                );
                let observation_id = observation.observation_id.clone();
                let suppressed_at = self.clock.now();
                observation.detail_ref = Some(format!(
                    "call-trigger-pending-suppressed-v1:{}",
                    serde_json::to_string(&TriggerPendingSuppressedProjectionEvidence {
                        pending_occurrence_id: pending.pending_occurrence_id.clone(),
                        observation_id: observation_id.clone(),
                        suppressed_at: suppressed_at.clone(),
                    })?
                ));
                self.ledger.append(vec![observation]).await?;
                state.suppress_call_trigger_pending_occurrence(
                    &pending.pending_occurrence_id,
                    &observation_id,
                    &suppressed_at,
                )?;
                continue;
            };
            let active_resident = state.summary()?.active_trigger_firings as usize;
            if active_resident >= self.limits.max_active_calls {
                continue;
            }
            let Some(permit) = Arc::clone(&self.active_capacity).try_acquire_owned().ok() else {
                continue;
            };
            let occurrence = state
                .call_trigger_occurrence(&pending.occurrence_id)?
                .context("pending trigger occurrence missing")?;
            drop(state);
            self.admit_firing(
                &authority,
                CallTriggerOccurrenceCandidate {
                    occurrence_id: pending.occurrence_id,
                    represented_set: pending.represented_set,
                },
                occurrence.nominal_at.unwrap_or_else(|| self.clock.now()),
                "pending_dequeue",
                permit,
            )
            .await?;
            break;
        }
        Ok(())
    }

    async fn evaluate_authority(
        &mut self,
        authority: &CallTriggerAuthority,
        now: &Timestamp,
        budget: &mut usize,
    ) -> Result<()> {
        if *budget == 0 {
            return Ok(());
        }
        let state = MctRuntimeStateStore::open(self.paths.state_path())?;
        let after = state.latest_call_trigger_nominal_at(
            &authority.trigger_authority_id,
            authority.record_revision,
        )?;
        let Some(range) = temporal_due_range(authority, after.as_ref(), now)? else {
            return Ok(());
        };

        let last_turn_nanos = timestamp_nanos(&self.last_turn)?;
        let interval_ms = match authority.trigger_source {
            CallTriggerSource::Temporal { interval_ms, .. } => interval_ms,
            CallTriggerSource::Event { .. } => return Ok(()),
        };
        let anchor_nanos = match &authority.trigger_source {
            CallTriggerSource::Temporal { anchor_at, .. } => timestamp_nanos(anchor_at)?,
            CallTriggerSource::Event { .. } => return Ok(()),
        };
        let interval_nanos = i128::from(interval_ms) * 1_000_000;
        let last_missed_index = if last_turn_nanos < anchor_nanos {
            None
        } else {
            u64::try_from((last_turn_nanos - anchor_nanos) / interval_nanos).ok()
        };
        let missed_last = last_missed_index
            .map(|index| index.min(range.last_index))
            .filter(|index| *index >= range.first_index);

        if let Some(missed_last) = missed_last {
            let missed = DueOccurrenceRange {
                first_index: range.first_index,
                last_index: missed_last,
                first_at: range.first_at.clone(),
                last_at: occurrence_at(authority, missed_last)?.1,
                count: missed_last - range.first_index + 1,
            };
            match authority.missed_fire_policy {
                MissedFirePolicy::Skip => {
                    self.record_terminal_range(
                        authority,
                        &missed,
                        MctTriggerOccurrenceDisposition::Skipped,
                        "missed_fire_skip",
                        "skip",
                    )
                    .await?;
                    *budget = budget.saturating_sub(1);
                }
                MissedFirePolicy::CoalesceOne => {
                    let represented_set = represented_range(authority, &missed)?;
                    let occurrence_id = derive_coalesced_occurrence_identity(&represented_set);
                    self.process_candidate(
                        authority,
                        CallTriggerOccurrenceCandidate {
                            occurrence_id,
                            represented_set,
                        },
                        missed.last_at,
                        "coalesce_one",
                    )
                    .await?;
                    *budget = budget.saturating_sub(1);
                }
                MissedFirePolicy::FireLateBounded => {
                    let plan = fire_late_recovery_plan(
                        missed.count,
                        self.limits.max_recovery_range_occurrences,
                        *budget,
                    );
                    debug_assert!(plan.evaluations() <= *budget);
                    for offset in 0..plan.admitted {
                        let index = missed.first_index + offset;
                        let (candidate, nominal_at) = singleton_candidate(authority, index)?;
                        self.process_candidate(
                            authority,
                            candidate,
                            nominal_at,
                            "fire_late_bounded",
                        )
                        .await?;
                        *budget = budget.saturating_sub(1);
                    }
                    if let Some(refused_count) = plan.refused {
                        let first_refused_index = missed.first_index + plan.admitted;
                        let refused = DueOccurrenceRange {
                            first_index: first_refused_index,
                            last_index: missed.last_index,
                            first_at: occurrence_at(authority, first_refused_index)?.1,
                            last_at: missed.last_at,
                            count: refused_count,
                        };
                        self.record_terminal_range(
                            authority,
                            &refused,
                            MctTriggerOccurrenceDisposition::CapacityRefused,
                            "missed_fire_recovery_bound",
                            "fire_late_bounded",
                        )
                        .await?;
                        *budget = budget.saturating_sub(1);
                    }
                }
            }
        }

        if *budget == 0 {
            return Ok(());
        }
        let live_first = missed_last.map_or(range.first_index, |index| index + 1);
        if live_first <= range.last_index {
            for index in live_first..=range.last_index {
                if *budget == 0 {
                    break;
                }
                let (candidate, nominal_at) = singleton_candidate(authority, index)?;
                self.process_candidate(authority, candidate, nominal_at, "live")
                    .await?;
                *budget = budget.saturating_sub(1);
            }
        }
        Ok(())
    }

    async fn evaluate_revoked_authority(
        &self,
        authority: &CallTriggerAuthority,
        now: &Timestamp,
    ) -> Result<()> {
        if authority.authority_state != CallTriggerAuthorityState::Revoked {
            return Ok(());
        }
        let state = MctRuntimeStateStore::open(self.paths.state_path())?;
        if state
            .latest_call_trigger_nominal_at(
                &authority.trigger_authority_id,
                authority.record_revision,
            )?
            .is_some()
        {
            return Ok(());
        }
        let after =
            state.latest_call_trigger_nominal_at_any_revision(&authority.trigger_authority_id)?;
        let Some(range) = temporal_due_range(authority, after.as_ref(), now)? else {
            return Ok(());
        };
        let next = DueOccurrenceRange {
            first_index: range.first_index,
            last_index: range.first_index,
            first_at: range.first_at.clone(),
            last_at: range.first_at,
            count: 1,
        };
        self.record_terminal_range(
            authority,
            &next,
            MctTriggerOccurrenceDisposition::Suppressed,
            "revoked_next_occurrence",
            "current_authority_denied",
        )
        .await
    }

    async fn evaluate_turn(&mut self) -> Result<()> {
        while let Ok(firing_id) = self.completion_rx.try_recv() {
            self.running_firings.remove(&firing_id);
        }
        self.recover_active_firings().await?;
        self.drain_pending().await?;
        let now = self.clock.now();
        let authorities = MctRuntimeStateStore::open(self.paths.state_path())?
            .current_call_trigger_authorities()?;
        let mut budget = self.limits.max_evaluations_per_turn;
        for authority in authorities {
            match authority.authority_state {
                CallTriggerAuthorityState::Active => {
                    self.evaluate_authority(&authority, &now, &mut budget)
                        .await?;
                }
                CallTriggerAuthorityState::Revoked => {
                    self.evaluate_revoked_authority(&authority, &now).await?;
                }
                CallTriggerAuthorityState::Superseded => {}
            }
            if budget == 0 {
                break;
            }
        }
        self.last_turn = now;
        Ok(())
    }
}

fn trigger_protocol_request(
    authority: &CallTriggerAuthority,
    firing: &MctTriggerFiringRecord,
    idempotency_key: String,
    endpoint_id: EndpointIdText,
) -> Result<MctCallProtocolRequest> {
    let size_bytes = match &authority.payload_constraint {
        MctCallPayloadHandle::ContentAddressedBlob { size_bytes, .. } => *size_bytes,
        MctCallPayloadHandle::Empty => 0,
        _ => bail!("trigger payload is not local content-addressed"),
    };
    let now = current_timestamp().as_str().parse::<jiff::Timestamp>()?;
    let deadline = now.checked_add(jiff::SignedDuration::from_secs(30))?;
    let call = MctCall {
        call_id: firing.call_id.clone(),
        caller: authority.canonical_caller.clone(),
        target: authority.target.clone(),
        payload_metadata: PayloadMetadata {
            data_classification: "trigger-static".into(),
            size_bytes,
            contains_secret_scoped_material: false,
        },
        authority_context: AuthorityContextSnapshot {
            policy_revision: authority.policy_revision,
            grants_revision: authority.policy_revision,
            vision_policy_revision: authority.policy_revision,
        },
        deadline: Timestamp::new(deadline.to_string())?,
        trace_context: TraceContext {
            trace_id: TraceId::new(format!("trace:trigger:{}", firing.occurrence_id))?,
            span_id: SpanId::new(format!("span:trigger:{}", firing.occurrence_id))?,
        },
        origin: CallOrigin::TriggerFiring,
    };
    let protocol_request_id =
        ProtocolRequestId::new(format!("protocol:trigger:{}", firing.occurrence_id))?;
    Ok(MctCallProtocolRequest {
        authority: MctCallProtocolAuthority {
            hello_decision_id: DecisionId::new(format!(
                "decision:trigger:{}",
                firing.occurrence_id
            ))?,
            peer_binding_id: PeerBindingId::new(format!(
                "binding:trigger:{}",
                authority.trigger_authority_id
            ))?,
            vision_id: authority.vision_id.clone(),
            accepted_alpn: "mct/trigger-call/0".into(),
            endpoint_id: endpoint_id.clone(),
            policy_revision: authority.policy_revision,
            grants_revision: authority.policy_revision,
        },
        received_over: IrohConnectionPresentation {
            endpoint_id,
            alpn: "mct/trigger-call/0".into(),
            connection_side: ConnectionSide::Incoming,
            path_class: PathClass::Direct,
            relay_url: None,
            presented_capability_ref: None,
        },
        call,
        payload: authority.payload_constraint.clone(),
        idempotency_key: Some(idempotency_key),
        received_observation_id: firing.firing_observation_id.clone(),
        protocol_request_id,
    })
}

async fn complete_trigger_firing_projection(
    state_path: &Path,
    ledger: &ResidentLedgerWriter,
    authority: &CallTriggerAuthority,
    firing: &MctTriggerFiringRecord,
    target_result_ref: ResultRef,
    outcome: CallProtocolOutcome,
    route_decision_id: Option<DecisionId>,
) -> Result<()> {
    let completed_at = current_timestamp();
    let completion_evidence = TriggerFiringCompletedProjectionEvidence {
        firing_id: firing.firing_id.clone(),
        target_result_ref: target_result_ref.clone(),
        completed_at: completed_at.clone(),
    };
    let completion = MctObservation {
        observation_id: ObservationId::new(format!("obs:trigger:terminal:{}", firing.firing_id))?,
        observed_at: current_timestamp(),
        kind: ObservationKind::LifecycleTransitionRecorded,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace:trigger:{}", firing.occurrence_id))?,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(firing.call_id.clone()),
        decision_id: route_decision_id,
        subject_id: Some(authority.trigger_authority_id.to_string()),
        resource_id: Some(target_result_ref.to_string()),
        policy_revision: Some(authority.policy_revision),
        grants_revision: None,
        outcome: match outcome {
            CallProtocolOutcome::Completed | CallProtocolOutcome::AcceptedForRouting => {
                ObservationOutcome::Completed
            }
            CallProtocolOutcome::Denied | CallProtocolOutcome::Malformed => {
                ObservationOutcome::Denied
            }
            CallProtocolOutcome::Failed => ObservationOutcome::Failed,
            CallProtocolOutcome::TimedOut => ObservationOutcome::TimedOut,
            CallProtocolOutcome::Cancelled => ObservationOutcome::Cancelled,
        },
        visibility: ObservationVisibility::InternalOnly,
        safe_message: "trigger firing target result referenced".into(),
        detail_ref: Some(format!(
            "call-trigger-firing-completed-v1:{}",
            serde_json::to_string(&completion_evidence)?
        )),
    };
    ledger.append(vec![completion]).await?;
    MctRuntimeStateStore::open(state_path)?.complete_call_trigger_firing(
        &firing.firing_id,
        Some(&target_result_ref),
        &completed_at,
    )
}

async fn deny_trigger_firing_before_execution(
    paths: &ResidentRuntimePaths,
    ledger: &ResidentLedgerWriter,
    authority: &CallTriggerAuthority,
    firing: &MctTriggerFiringRecord,
    reason: &'static str,
) -> Result<()> {
    let decision_id = DecisionId::new(format!("decision:trigger-denied:{}", firing.firing_id))?;
    let result_ref = ResultRef::new(format!("result-resident:{}", firing.call_id))?;
    let result = MctResult {
        call_id: firing.call_id.clone(),
        outcome: ResultOutcome::Denied,
        route_taken: None,
        authority_decision_ref: decision_id.clone(),
        execution_summary: ExecutionSummary {
            wall_time_ms: 0,
            execution_time_ms: None,
            queue_wait_ms: None,
            input_size_bytes: authority.payload_constraint.declared_size_bytes(),
            output_size_bytes: None,
        },
        result_payload: MctCallPayloadHandle::Empty,
        requester_message: "not authorized".into(),
        audit_ref: AuditRef::new(format!("audit:trigger-denied:{}", firing.firing_id))?,
    };
    let observation = MctObservation {
        observation_id: ObservationId::new(format!(
            "obs:result-trigger-denied:{}",
            firing.firing_id
        ))?,
        observed_at: current_timestamp(),
        kind: ObservationKind::ResultRecorded,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace:trigger:{}", firing.occurrence_id))?,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(firing.call_id.clone()),
        decision_id: Some(decision_id),
        subject_id: Some(authority.trigger_authority_id.to_string()),
        resource_id: Some(result_ref.to_string()),
        policy_revision: Some(authority.policy_revision),
        grants_revision: None,
        outcome: ObservationOutcome::Denied,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: "trigger firing denied by fresh current law".into(),
        detail_ref: Some(format!("trigger_denial_reason:{reason}")),
    };
    ledger.append(vec![observation]).await?;
    let state = MctRuntimeStateStore::open(paths.state_path())?;
    let run_id = format!("run-trigger-denied:{}", firing.call_id);
    if state.get_run(&run_id)?.is_none() {
        let request = trigger_protocol_request(
            authority,
            firing,
            derive_trigger_idempotency_key(
                &authority.trigger_authority_id,
                authority.record_revision,
                &firing.occurrence_id,
            ),
            MctDaemonConfigStore::new(paths.config_path())
                .load()?
                .local_identity
                .context("trigger denial identity unavailable")?
                .endpoint_id,
        )?;
        state.insert_run_started(
            &run_id,
            &request.call,
            RuntimeKind::Internal,
            None,
            current_timestamp_string(),
        )?;
    }
    state.complete_run(&run_id, &result, current_timestamp_string())?;
    drop(state);
    complete_trigger_firing_projection(
        paths.state_path(),
        ledger,
        authority,
        firing,
        result_ref,
        CallProtocolOutcome::Denied,
        Some(result.authority_decision_ref),
    )
    .await
}

fn protocol_outcome_from_result(outcome: ResultOutcome) -> CallProtocolOutcome {
    match outcome {
        ResultOutcome::Success => CallProtocolOutcome::Completed,
        ResultOutcome::Denied => CallProtocolOutcome::Denied,
        ResultOutcome::Failed => CallProtocolOutcome::Failed,
        ResultOutcome::TimedOut => CallProtocolOutcome::TimedOut,
        ResultOutcome::Cancelled => CallProtocolOutcome::Cancelled,
    }
}

async fn execute_trigger_firing(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    authority: CallTriggerAuthority,
    firing: MctTriggerFiringRecord,
    idempotency_key: String,
) -> Result<()> {
    let state = MctRuntimeStateStore::open(paths.state_path())?;
    if let Some(run) = state.get_run_by_call_id(&firing.call_id)? {
        if let Some(result) = run.result {
            let result_ref = ResultRef::new(format!("result-resident:{}", firing.call_id))?;
            let outcome = protocol_outcome_from_result(result.outcome);
            drop(state);
            return complete_trigger_firing_projection(
                paths.state_path(),
                &ledger,
                &authority,
                &firing,
                result_ref,
                outcome,
                Some(result.authority_decision_ref),
            )
            .await;
        }
        return Ok(());
    }
    if state.call_idempotency_entry_state(
        &format!("trigger:{}", authority.trigger_authority_id),
        &idempotency_key,
    )? == Some(MctIdempotencyEntryState::InFlight)
    {
        return Ok(());
    }
    let current = state.current_call_trigger_authority(&authority.trigger_authority_id)?;
    let wall_now = current_timestamp();
    let fresh_at = if wall_now > firing.fired_at {
        wall_now
    } else {
        firing.fired_at.clone()
    };
    if current.as_ref() != Some(&authority) || !authority.is_current_at(&fresh_at) {
        return deny_trigger_firing_before_execution(
            &paths,
            &ledger,
            &authority,
            &firing,
            "stale_or_inactive_trigger_authority",
        )
        .await;
    }
    if local_blob_store_for_state_path(paths.state_path())
        .fetch(&authority.payload_constraint)
        .is_err()
    {
        return deny_trigger_firing_before_execution(
            &paths,
            &ledger,
            &authority,
            &firing,
            "trigger_payload_unavailable",
        )
        .await;
    }
    drop(state);
    let config = MctDaemonConfigStore::new(paths.config_path()).load()?;
    let identity = config
        .local_identity
        .context("trigger execution identity unavailable")?;
    let request =
        trigger_protocol_request(&authority, &firing, idempotency_key, identity.endpoint_id)?;
    request.validate().map_err(anyhow::Error::from)?;
    let result = execute_resident_call_with_context(
        paths.clone(),
        ledger.clone(),
        request,
        ResidentPayloadIngress::local(None),
        ResidentCallIngressContext::Trigger {
            trigger_authority_id: authority.trigger_authority_id.clone(),
            record_revision: authority.record_revision,
            firing_id: firing.firing_id.clone(),
            occurrence_id: firing.occurrence_id.clone(),
        },
    )
    .await;
    let state = MctRuntimeStateStore::open(paths.state_path())?;
    let target_result_ref = match result.result_ref {
        Some(result_ref) => Some(result_ref),
        None => state
            .get_run_by_call_id(&firing.call_id)?
            .filter(|run| run.result.is_some())
            .map(|_| {
                ResultRef::new(format!("result-resident:{}", firing.call_id))
                    .expect("generated resident result ref must be non-empty")
            }),
    };
    let Some(target_result_ref) = target_result_ref else {
        return Ok(());
    };
    drop(state);
    complete_trigger_firing_projection(
        paths.state_path(),
        &ledger,
        &authority,
        &firing,
        target_result_ref,
        result.outcome,
        result.route_decision_id,
    )
    .await
}

fn detail_json<'a>(detail: &'a str, prefix: &str) -> Option<&'a str> {
    detail.strip_prefix(prefix)
}

pub(crate) fn reconcile_trigger_projection(state_path: &Path, ledger_path: &Path) -> Result<()> {
    let entries = JsonlObservationLedger::open_read_only(ledger_path, "ledger-local", "local-mct")?
        .entries()?;
    let state = MctRuntimeStateStore::open(state_path)?;
    let mut seen_authorities = BTreeSet::new();
    let mut seen_occurrences = BTreeSet::new();
    let mut seen_firings = BTreeSet::new();
    let mut last_sequence = 0_u64;

    for entry in entries {
        last_sequence = entry.local_sequence;
        let Some(detail) = entry.observation.detail_ref.as_deref() else {
            continue;
        };
        if let Some(json) = detail_json(detail, "call-trigger-authority-v1:") {
            let authority: CallTriggerAuthority =
                serde_json::from_str(json).context("decode trigger authority ledger projection")?;
            authority.validate().map_err(anyhow::Error::from)?;
            let key = (
                authority.trigger_authority_id.to_string(),
                authority.record_revision,
            );
            let existing = state
                .call_trigger_authorities()?
                .into_iter()
                .find(|current| {
                    current.trigger_authority_id == authority.trigger_authority_id
                        && current.record_revision == authority.record_revision
                });
            match existing {
                Some(existing) if existing == authority => {}
                Some(_) => bail!("trigger authority projection conflicts with durable ledger"),
                None => state.insert_call_trigger_authority(&authority)?,
            }
            seen_authorities.insert(key);
        } else if let Some(json) = detail_json(detail, "call-trigger-occurrence-v1:") {
            let occurrence: MctTriggerOccurrenceRecord = serde_json::from_str(json)
                .context("decode trigger occurrence ledger projection")?;
            if !state.call_trigger_occurrence_exists(&occurrence.occurrence_id)? {
                state.insert_call_trigger_occurrence(&occurrence)?;
            }
            seen_occurrences.insert(occurrence.occurrence_id.to_string());
        } else if let Some(json) = detail_json(detail, "call-trigger-pending-v1:") {
            let evidence: TriggerPendingProjectionEvidence =
                serde_json::from_str(json).context("decode trigger pending ledger projection")?;
            if state
                .call_trigger_pending_occurrence(&evidence.pending.pending_occurrence_id)?
                .is_none()
            {
                state.insert_call_trigger_pending_occurrence(
                    &evidence.occurrence,
                    &evidence.pending,
                )?;
            }
            seen_occurrences.insert(evidence.occurrence.occurrence_id.to_string());
        } else if let Some(json) = detail_json(detail, "call-trigger-firing-v1:") {
            let evidence: TriggerFiringProjectionEvidence =
                serde_json::from_str(json).context("decode trigger firing ledger projection")?;
            if state
                .call_trigger_firing(&evidence.firing.firing_id)?
                .is_none()
            {
                state.insert_call_trigger_firing(&evidence.occurrence, &evidence.firing)?;
            }
            seen_occurrences.insert(evidence.occurrence.occurrence_id.to_string());
            seen_firings.insert(evidence.firing.firing_id.to_string());
        } else if let Some(json) = detail_json(detail, "call-trigger-coalesced-v1:") {
            let evidence: TriggerCoalescedProjectionEvidence = serde_json::from_str(json)
                .context("decode trigger coalescing ledger projection")?;
            if let Some(pending) =
                state.call_trigger_pending_occurrence(&evidence.pending_occurrence_id)?
                && pending.represented_set != evidence.represented_set
            {
                state.update_coalesced_call_trigger_pending_occurrence(
                    &evidence.pending_occurrence_id,
                    &evidence.represented_set,
                    Some(&evidence.nominal_at),
                    &evidence.observation_id,
                )?;
            }
        } else if let Some(json) = detail_json(detail, "call-trigger-pending-suppressed-v1:") {
            let evidence: TriggerPendingSuppressedProjectionEvidence =
                serde_json::from_str(json)
                    .context("decode trigger pending suppression ledger projection")?;
            if let Some(pending) =
                state.call_trigger_pending_occurrence(&evidence.pending_occurrence_id)?
                && pending.state == "pending"
            {
                state.suppress_call_trigger_pending_occurrence(
                    &evidence.pending_occurrence_id,
                    &evidence.observation_id,
                    &evidence.suppressed_at,
                )?;
            }
        } else if let Some(json) = detail_json(detail, "call-trigger-firing-completed-v1:") {
            let evidence: TriggerFiringCompletedProjectionEvidence = serde_json::from_str(json)
                .context("decode trigger firing completion ledger projection")?;
            if let Some(firing) = state.call_trigger_firing(&evidence.firing_id)?
                && firing.state == "active"
            {
                state.complete_call_trigger_firing(
                    &evidence.firing_id,
                    Some(&evidence.target_result_ref),
                    &evidence.completed_at,
                )?;
            }
        }
    }

    for authority in state.call_trigger_authorities()? {
        let key = (
            authority.trigger_authority_id.to_string(),
            authority.record_revision,
        );
        if !seen_authorities.contains(&key) {
            bail!("trigger authority projection lacks durable ledger fact");
        }
    }
    for occurrence in state.call_trigger_occurrences()? {
        if !seen_occurrences.contains(&occurrence.occurrence_id.to_string()) {
            bail!("trigger occurrence projection lacks durable ledger fact");
        }
    }
    for firing in state.call_trigger_firings()? {
        if !seen_firings.contains(&firing.firing_id.to_string()) {
            bail!("trigger firing projection lacks durable ledger fact");
        }
    }
    state.update_call_trigger_projection_checkpoint(
        &format!("ledger-local:{}", ledger_path.display()),
        last_sequence,
    )
}

pub(super) async fn run_trigger_scheduler_with_runtime(
    paths: ResidentRuntimePaths,
    ledger: ResidentLedgerWriter,
    mut shutdown: broadcast::Receiver<()>,
    clock: Arc<dyn TriggerClock>,
    limits: TriggerLimits,
) -> Result<()> {
    let mut scheduler = TriggerScheduler::new(paths, ledger, clock, limits);
    let mut interval = tokio::time::interval(Duration::from_millis(MCT_TRIGGER_SCHEDULER_POLL_MS));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = shutdown.recv() => return Ok(()),
            _ = interval.tick() => {
                if let Err(error) = scheduler.evaluate_turn().await {
                    eprintln!("resident trigger scheduler turn failed: {error}");
                    if scheduler.ledger.is_fenced() {
                        return Err(error);
                    }
                }
                tokio::task::yield_now().await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct ManualClock(std::sync::Mutex<Timestamp>);

    impl ManualClock {
        fn new(now: &str) -> Self {
            Self(std::sync::Mutex::new(Timestamp::new(now).unwrap()))
        }

        fn set(&self, now: &str) {
            *self.0.lock().unwrap() = Timestamp::new(now).unwrap();
        }
    }

    impl TriggerClock for ManualClock {
        fn now(&self) -> Timestamp {
            self.0.lock().unwrap().clone()
        }
    }

    fn firing(id: &str) -> MctTriggerFiringRecord {
        MctTriggerFiringRecord {
            firing_id: CallTriggerFiringId::new(format!("firing-{id}")).unwrap(),
            occurrence_id: CallTriggerOccurrenceId::new(format!("occurrence-{id}")).unwrap(),
            trigger_authority_id: CallTriggerAuthorityId::new("trigger-a").unwrap(),
            record_revision: 1,
            policy_revision: 1,
            call_id: CallId::new(format!("call-{id}")).unwrap(),
            idempotency_key_ref: "blake3:key".into(),
            firing_observation_id: ObservationId::new(format!("obs-{id}")).unwrap(),
            target_result_ref: None,
            state: "active".into(),
            fired_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
            completed_at: None,
        }
    }

    fn pending(id: &str) -> MctTriggerPendingOccurrenceRecord {
        let occurrence_id = CallTriggerOccurrenceId::new(format!("occurrence-{id}")).unwrap();
        MctTriggerPendingOccurrenceRecord {
            pending_occurrence_id: CallTriggerPendingOccurrenceId::new(format!("pending-{id}"))
                .unwrap(),
            occurrence_id: occurrence_id.clone(),
            trigger_authority_id: CallTriggerAuthorityId::new("trigger-a").unwrap(),
            record_revision: 1,
            policy_revision: 1,
            admission_sequence: 1,
            pending_reason: CallTriggerPendingReason::OverlapCoalesced,
            represented_set: trigger_represented_set_from_bounds(
                &CallTriggerAuthorityId::new("trigger-a").unwrap(),
                1,
                occurrence_id.clone(),
                occurrence_id,
                1,
            )
            .unwrap(),
            admission_observation_id: ObservationId::new(format!("obs-pending-{id}")).unwrap(),
            state: "pending".into(),
            admitted_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
            consumed_at: None,
        }
    }

    fn add_millis(timestamp: &Timestamp, millis: i64) -> Timestamp {
        let value = timestamp
            .as_str()
            .parse::<jiff::Timestamp>()
            .unwrap()
            .checked_add(jiff::SignedDuration::from_millis(millis))
            .unwrap();
        Timestamp::new(value.to_string()).unwrap()
    }

    fn write_counting_child(children_dir: &Path) -> PathBuf {
        use sha2::{Digest as _, Sha256};
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt as _;

        let child_dir = children_dir.join("trigger-counting");
        std::fs::create_dir_all(&child_dir).unwrap();
        let artifact = child_dir.join("trigger-counting.wasm");
        let script = b"#!/bin/sh\ncounter=\"$0.count\"\ncount=$(cat \"$counter\" 2>/dev/null || printf 0)\ncount=$((count + 1))\nprintf '%s' \"$count\" >\"$counter\"\ncat >/dev/null\nprintf 'trigger-result-%s' \"$count\"\n";
        std::fs::write(&artifact, script).unwrap();
        #[cfg(unix)]
        {
            let mut permissions = std::fs::metadata(&artifact).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&artifact, permissions).unwrap();
        }
        let manifest = child_dir.join("child.toml");
        std::fs::write(
            &manifest,
            r#"[child]
name = "trigger-counting"
version = "0.1.0"
description = "trigger scheduler test child"
kind = "child"
role = "app"

[child.ingress]
mode = "handle"

[child.artifact]
wasm = "trigger-counting.wasm"

[child.contract]
allow = ["patina:demo/control@0.1.0.run"]

[needs]
toys = []

[relationships]
listens = []
"#,
        )
        .unwrap();
        let manifest_bytes = std::fs::read(&manifest).unwrap();
        for (path, bytes) in [
            (artifact.as_path(), script.as_slice()),
            (manifest.as_path(), manifest_bytes.as_slice()),
        ] {
            let mut sidecar = path.as_os_str().to_os_string();
            sidecar.push(".sha256");
            std::fs::write(
                PathBuf::from(sidecar),
                format!("{:x}", Sha256::digest(bytes)),
            )
            .unwrap();
        }
        artifact
    }

    fn write_wit_child(children_dir: &Path) -> PathBuf {
        use sha2::{Digest as _, Sha256};

        let child_dir = children_dir.join("trigger-wit");
        std::fs::create_dir_all(&child_dir).unwrap();
        let artifact = child_dir.join("trigger-wit.wasm");
        let manifest = child_dir.join("child.toml");
        let component = wat::parse_str(
            r#"
(component
  (core module $m
    (func $run (export "run") (result i32)
      i32.const 7))
  (core instance $i (instantiate $m))
  (func $run (result s32) (canon lift (core func $i "run")))
  (instance $control (export "run" (func $run)))
  (export "patina:demo/control@0.1.0" (instance $control)))
"#,
        )
        .unwrap();
        std::fs::write(&artifact, &component).unwrap();
        std::fs::write(
            &manifest,
            r#"[child]
name = "trigger-wit"
version = "0.1.0"
description = "trigger resident integration child"
kind = "child"
role = "app"

[child.ingress]
mode = "wit-only"

[child.artifact]
wasm = "trigger-wit.wasm"

[child.contract]
allow = ["patina:demo/control@0.1.0.run"]

[needs]
toys = []

[relationships]
listens = []
"#,
        )
        .unwrap();
        let manifest_bytes = std::fs::read(&manifest).unwrap();
        for (path, bytes) in [
            (artifact.as_path(), component.as_slice()),
            (manifest.as_path(), manifest_bytes.as_slice()),
        ] {
            let mut sidecar = path.as_os_str().to_os_string();
            sidecar.push(".sha256");
            std::fs::write(
                PathBuf::from(sidecar),
                format!("{:x}", Sha256::digest(bytes)),
            )
            .unwrap();
        }
        artifact
    }

    async fn post_uds_json(
        socket_path: &Path,
        path: &str,
        value: &impl serde::Serialize,
    ) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let body = serde_json::to_vec(value).unwrap();
        let mut stream = tokio::net::UnixStream::connect(socket_path).await.unwrap();
        stream
            .write_all(
                format!(
                    "POST {path} HTTP/1.1\r\nHost: local\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        stream.write_all(&body).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8(response).unwrap();
        let status = response
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let body = response.split_once("\r\n\r\n").unwrap().1;
        (status, serde_json::from_str(body).unwrap())
    }

    async fn get_uds_json(socket_path: &Path, path: &str) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let mut stream = tokio::net::UnixStream::connect(socket_path).await.unwrap();
        stream
            .write_all(format!("GET {path} HTTP/1.1\r\nHost: local\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8(response).unwrap();
        let status = response
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let body = response.split_once("\r\n\r\n").unwrap().1;
        (status, serde_json::from_str(body).unwrap())
    }

    async fn start_test_resident(
        paths: ResidentRuntimePaths,
        identity_path: PathBuf,
        ledger_path: PathBuf,
        socket_path: PathBuf,
        clock: Arc<dyn TriggerClock>,
        limits: TriggerLimits,
    ) -> (
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<Result<()>>,
    ) {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            run_test_resident_mother_with_trigger_runtime(
                paths,
                identity_path,
                ledger_path,
                socket_path,
                async {
                    let _ = shutdown_rx.await;
                },
                Some(ready_tx),
                (clock, limits),
            )
            .await
        });
        ready_rx.await.unwrap();
        (shutdown_tx, server)
    }

    async fn revoke_after_writer_release(
        config_path: &Path,
        state_path: &Path,
        ledger_path: &Path,
        revoke: &crate::triggers::TriggerRevokeRequest,
    ) -> CallTriggerAuthority {
        for _ in 0..100 {
            match crate::triggers::execute_offline_trigger_mutation(
                config_path,
                state_path,
                ledger_path,
                501,
                "/triggers/revoke",
                &serde_json::to_vec(revoke).unwrap(),
            ) {
                Ok(authority) => return authority,
                Err(error) => {
                    let detail = format!("{error:#}");
                    if detail.contains("already locked by another writer") {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    } else {
                        panic!("offline trigger revocation failed: {detail}");
                    }
                }
            }
        }
        panic!("resident observation writer did not release after shutdown");
    }

    async fn spawn_test_ledger_after_writer_release(path: &Path) -> ResidentLedgerWriter {
        for _ in 0..100 {
            match ResidentLedgerWriter::spawn(path.to_path_buf()) {
                Ok(ledger) => return ledger,
                Err(error) if format!("{error:#}").contains("already locked by another writer") => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(error) => panic!("open test ledger writer failed: {error:#}"),
            }
        }
        panic!("resident observation writer did not release after shutdown")
    }

    async fn wait_for_trigger_disposition(
        state_path: &Path,
        trigger_authority_id: &CallTriggerAuthorityId,
        record_revision: u64,
        disposition: MctTriggerOccurrenceDisposition,
    ) -> MctTriggerOccurrenceRecord {
        for _ in 0..200 {
            if let Some(occurrence) = MctRuntimeStateStore::open(state_path)
                .unwrap()
                .call_trigger_occurrences()
                .unwrap()
                .into_iter()
                .find(|occurrence| {
                    occurrence.trigger_authority_id == *trigger_authority_id
                        && occurrence.record_revision == record_revision
                        && occurrence.final_disposition == disposition
                        && occurrence.final_disposition.is_terminal()
                })
            {
                return occurrence;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!(
            "trigger {trigger_authority_id} revision {record_revision} did not durably reach {disposition:?}"
        )
    }

    async fn admit_crash_firing_and_overlap_pending(
        paths: &ResidentRuntimePaths,
        ledger_path: &Path,
        clock: Arc<ManualClock>,
        authority: &CallTriggerAuthority,
    ) -> (MctTriggerFiringRecord, MctTriggerPendingOccurrenceRecord) {
        let ledger = spawn_test_ledger_after_writer_release(ledger_path).await;
        let scheduler = TriggerScheduler::new(
            paths.clone(),
            ledger.clone(),
            clock.clone(),
            TriggerLimits::default(),
        );
        let (active_candidate, active_at) = singleton_candidate(authority, 1).unwrap();
        clock.set(active_at.as_str());
        let call_id = derive_trigger_call_identity(&active_candidate.occurrence_id);
        let idempotency_key = derive_trigger_idempotency_key(
            &authority.trigger_authority_id,
            authority.record_revision,
            &active_candidate.occurrence_id,
        );
        let mut observation = trigger_observation(
            authority,
            &active_candidate.occurrence_id,
            &active_candidate.represented_set,
            Some(&active_at),
            ObservationKind::CallConstructed,
            ObservationOutcome::Allowed,
            "trigger firing constructed durable call",
            "firing",
            Some(call_id.clone()),
        );
        let occurrence = occurrence_record(
            authority,
            &active_candidate,
            Some(active_at),
            "live",
            Some("fire_now".into()),
            MctTriggerOccurrenceDisposition::Fired,
            observation.observation_id.clone(),
            clock.now(),
        );
        let firing = MctTriggerFiringRecord {
            firing_id: derive_trigger_firing_identity(&active_candidate.occurrence_id),
            occurrence_id: active_candidate.occurrence_id,
            trigger_authority_id: authority.trigger_authority_id.clone(),
            record_revision: authority.record_revision,
            policy_revision: authority.policy_revision,
            call_id,
            idempotency_key_ref: format!("blake3:{}", blake3_hex(idempotency_key.as_bytes())),
            firing_observation_id: observation.observation_id.clone(),
            target_result_ref: None,
            state: "active".into(),
            fired_at: clock.now(),
            completed_at: None,
        };
        observation.detail_ref = Some(format!(
            "call-trigger-firing-v1:{}",
            serde_json::to_string(&TriggerFiringProjectionEvidence {
                occurrence: occurrence.clone(),
                firing: firing.clone(),
            })
            .unwrap()
        ));
        ledger.append(vec![observation]).await.unwrap();
        MctRuntimeStateStore::open(paths.state_path())
            .unwrap()
            .insert_call_trigger_firing(&occurrence, &firing)
            .unwrap();

        let (pending_candidate, pending_at) = singleton_candidate(authority, 2).unwrap();
        clock.set(pending_at.as_str());
        scheduler
            .admit_pending(
                authority,
                pending_candidate,
                pending_at,
                "live",
                CallTriggerPendingReason::OverlapQueued,
            )
            .await
            .unwrap();
        let pending = MctRuntimeStateStore::open(paths.state_path())
            .unwrap()
            .call_trigger_pending_occurrences()
            .unwrap()
            .into_iter()
            .find(|pending| pending.state == "pending")
            .unwrap();
        drop(scheduler);
        ledger.close().await;
        (firing, pending)
    }

    #[tokio::test]
    async fn resident_temporal_trigger_fires_once_and_recovers_without_duplication() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let children_dir = dir.path().join("children");
        let source_dir = dir.path().join("source");
        let identity_path = dir.path().join("identity.key");
        let socket_path = dir.path().join("control.sock");
        let source_component = write_wit_child(&source_dir);
        let source_package = source_component.parent().unwrap().to_path_buf();
        let component_bytes = std::fs::read(&source_component).unwrap();
        let component_blake3 = blake3::hash(&component_bytes).to_hex().to_string();
        MctDaemonConfigStore::new(&config_path)
            .ensure_local_identity(MctOperatorNodeScope::default(), &identity_path)
            .unwrap();
        let paths = ResidentRuntimePaths::new(
            config_path.clone(),
            children_dir.clone(),
            state_path.clone(),
        );
        let initial = current_timestamp();
        let clock = Arc::new(ManualClock::new(initial.as_str()));
        let (shutdown, server) = start_test_resident(
            paths.clone(),
            identity_path.clone(),
            ledger_path.clone(),
            socket_path.clone(),
            clock.clone(),
            TriggerLimits::default(),
        )
        .await;

        let stage = MctArtifactStageRequest {
            source_root: source_package,
            manifest_path: PathBuf::from("child.toml"),
            component_path: PathBuf::from("trigger-wit.wasm"),
            claimed_child_name: "trigger-wit".into(),
            claimed_artifact_version: "0.1.0".into(),
            expected_digest: Some(format!("blake3:{component_blake3}")),
            standing_source_authority_id: None,
            claimed_publisher: None,
            require_source_sidecars: true,
            children_dir: children_dir.clone(),
            state_path: state_path.clone(),
        };
        let (status, stage_body) = post_uds_json(&socket_path, "/artifacts/stage", &stage).await;
        assert_eq!(status, 200, "{stage_body}");
        let acquisition: MctArtifactAcquisitionReport = serde_json::from_value(stage_body).unwrap();
        assert_eq!(acquisition.acquisition_outcome, "acquired");
        assert_eq!(acquisition.verification_outcome, "verified");
        let artifact_id = acquisition.artifact_id.clone().unwrap();
        let package_path = acquisition.package_path.clone().unwrap();

        let approval = crate::control::ChildApproveRequest {
            expected_config_path: config_path.clone(),
            expected_children_dir: children_dir.clone(),
            child_name: "trigger-wit".into(),
            strict_integrity: true,
            expected_state_path: Some(state_path.clone()),
            expected_artifact_id: Some(artifact_id),
        };
        let (status, approval_body) =
            post_uds_json(&socket_path, "/children/approve", &approval).await;
        assert_eq!(status, 200, "{approval_body}");

        let payload = b"[]";
        let payload_digest = blake3::hash(payload).to_hex().to_string();
        let blob = serde_json::json!({
            "digest": payload_digest,
            "size_bytes": payload.len(),
            "content_type": "application/json",
            "classification": "trigger-static",
            "bytes_base64": BASE64_STANDARD.encode(payload),
        });
        let (status, blob_body) = post_uds_json(&socket_path, "/blobs", &blob).await;
        assert_eq!(status, 201, "{blob_body}");
        let payload_constraint: MctCallPayloadHandle =
            serde_json::from_value(blob_body["payload"].clone()).unwrap();

        let first_due = add_millis(&initial, 500);
        let create = crate::triggers::TriggerCreateRequest {
            expected_config_path: config_path.clone(),
            expected_state_path: state_path.clone(),
            scope: crate::triggers::TriggerAuthorityScopeRequest {
                trigger_authority_id: CallTriggerAuthorityId::new("trigger-resident").unwrap(),
                target: OperationTarget::new("patina:demo", "control@0.1.0", "run").unwrap(),
                payload_constraint,
                trigger_source: CallTriggerSource::Temporal {
                    anchor_at: first_due.clone(),
                    interval_ms: 2_000,
                },
                missed_fire_policy: MissedFirePolicy::Skip,
                overlap_policy: OverlapPolicy::Refuse,
                starts_at: initial,
                expires_at: add_millis(&first_due, 30_000),
            },
        };
        let mut create_request = serde_json::to_value(&create).unwrap();
        let scope = create_request["scope"].as_object_mut().unwrap();
        scope.remove("missed_fire_policy");
        scope.remove("overlap_policy");
        let (status, create_body) =
            post_uds_json(&socket_path, "/triggers/create", &create_request).await;
        assert_eq!(status, 200, "{create_body}");
        let created: CallTriggerAuthority = serde_json::from_value(create_body).unwrap();
        assert_eq!(created.missed_fire_policy, MissedFirePolicy::Skip);
        assert_eq!(created.overlap_policy, OverlapPolicy::Refuse);

        assert!(package_path.join("trigger-wit.wasm").is_file());
        clock.set(first_due.as_str());
        for _ in 0..200 {
            let state = MctRuntimeStateStore::open(&state_path).unwrap();
            let completed = state.list_runs(20).unwrap().iter().any(|run| {
                run.call.origin == CallOrigin::TriggerFiring
                    && run.state == mct_daemon::MctRuntimeRunState::Completed
            });
            if completed && state.summary().unwrap().active_trigger_firings == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        let trigger_run = state
            .list_runs(20)
            .unwrap()
            .into_iter()
            .find(|run| run.call.origin == CallOrigin::TriggerFiring)
            .expect("resident trigger call persisted");
        assert_eq!(trigger_run.state, mct_daemon::MctRuntimeRunState::Completed);
        assert_eq!(trigger_run.call.target, created.target);
        assert_eq!(trigger_run.call.caller, created.canonical_caller);
        assert!(trigger_run.result.as_ref().unwrap().route_taken.is_some());
        let first_occurrence = state
            .call_trigger_occurrences()
            .unwrap()
            .into_iter()
            .find(|occurrence| occurrence.record_revision == 1)
            .unwrap();
        let first_firing = state
            .call_trigger_firings()
            .unwrap()
            .into_iter()
            .find(|firing| firing.call_id == trigger_run.call_id)
            .unwrap();
        assert_eq!(
            first_firing.call_id,
            derive_trigger_call_identity(&first_occurrence.occurrence_id)
        );
        assert_eq!(
            first_firing.firing_id,
            derive_trigger_firing_identity(&first_occurrence.occurrence_id)
        );
        assert_eq!(first_firing.record_revision, created.record_revision);
        assert_eq!(first_firing.policy_revision, created.policy_revision);
        assert_eq!(first_occurrence.represented_set.count, 1);
        assert_eq!(
            first_firing.target_result_ref,
            Some(ResultRef::new(format!("result-resident:{}", trigger_run.call_id)).unwrap())
        );
        assert_eq!(state.summary().unwrap().active_trigger_firings, 0);
        drop(state);

        // The first firing's terminal disposition is the deterministic barrier
        // before any later authority mutation; elapsed wall time is irrelevant.
        let durable_first = wait_for_trigger_disposition(
            &state_path,
            &created.trigger_authority_id,
            1,
            MctTriggerOccurrenceDisposition::Fired,
        )
        .await;
        assert_eq!(durable_first.occurrence_id, first_occurrence.occurrence_id);

        // Re-evaluating the same injected nominal instant remains terminal and
        // cannot create a second call or target effect.
        assert_eq!(
            MctRuntimeStateStore::open(&state_path)
                .unwrap()
                .list_runs(20)
                .unwrap()
                .iter()
                .filter(|run| run.call.origin == CallOrigin::TriggerFiring)
                .count(),
            1
        );

        shutdown.send(()).unwrap();
        server.await.unwrap().unwrap();

        // Model the append/project crash seam exactly: one later firing is
        // durable but not spawned, and the following occurrence is admitted
        // under overlap before the process disappears.
        let (crash_firing, expected_pending) =
            admit_crash_firing_and_overlap_pending(&paths, &ledger_path, clock.clone(), &created)
                .await;
        assert_eq!(
            expected_pending.pending_reason,
            CallTriggerPendingReason::OverlapQueued
        );
        assert_eq!(expected_pending.admission_sequence, 1);

        let revoke = crate::triggers::TriggerRevokeRequest {
            expected_config_path: config_path.clone(),
            expected_state_path: state_path.clone(),
            trigger_authority_id: created.trigger_authority_id.clone(),
            expected_revision: 1,
        };
        let revoked =
            revoke_after_writer_release(&config_path, &state_path, &ledger_path, &revoke).await;
        assert_eq!(revoked.authority_state, CallTriggerAuthorityState::Revoked);
        let next_after_last_disposition = occurrence_at(&revoked, 3).unwrap().1;
        clock.set(next_after_last_disposition.as_str());

        // Destroy every trigger projection so resident startup must reconstruct
        // exact authority, firing, and pending identities from the ledger.
        let connection = rusqlite::Connection::open(&state_path).unwrap();
        connection
            .execute_batch(
                r#"
                DELETE FROM call_trigger_firings;
                DELETE FROM call_trigger_pending_occurrences;
                DELETE FROM call_trigger_occurrences;
                DELETE FROM call_trigger_authorities;
                DELETE FROM call_trigger_projection_meta;
                "#,
            )
            .unwrap();
        drop(connection);

        let saturated_limits = TriggerLimits {
            max_active_calls: 0,
            ..TriggerLimits::default()
        };
        let (restart_shutdown, restarted) = start_test_resident(
            paths.clone(),
            identity_path.clone(),
            ledger_path.clone(),
            socket_path.clone(),
            clock.clone(),
            saturated_limits,
        )
        .await;
        let reconstructed = MctRuntimeStateStore::open(&state_path).unwrap();
        assert_eq!(
            reconstructed
                .current_call_trigger_authority(&created.trigger_authority_id)
                .unwrap()
                .unwrap()
                .record_revision,
            2
        );
        assert!(
            reconstructed
                .call_trigger_firings()
                .unwrap()
                .iter()
                .any(|firing| firing.firing_id == crash_firing.firing_id)
        );
        assert!(
            reconstructed
                .call_trigger_pending_occurrences()
                .unwrap()
                .iter()
                .any(|pending| {
                    pending.pending_occurrence_id == expected_pending.pending_occurrence_id
                        && pending.admission_sequence == expected_pending.admission_sequence
                })
        );
        drop(reconstructed);

        // Saturating the trigger-only budget does not consume ordinary local
        // call admission or control/status responsiveness.
        let ordinary_payload = b"[]";
        let ordinary_call = serde_json::json!({
            "protocol_request_id": "proto-trigger-fairness",
            "call_id": "call-trigger-fairness-ordinary",
            "target": {
                "namespace": "patina:demo",
                "interface_name": "control@0.1.0",
                "function_name": "run"
            },
            "payload_metadata": {
                "data_classification": "public",
                "size_bytes": ordinary_payload.len(),
                "contains_secret_scoped_material": false
            },
            "authority_context": {
                "policy_revision": 1,
                "grants_revision": 1,
                "vision_policy_revision": 1
            },
            "deadline": add_millis(&clock.now(), 60_000),
            "trace_context": {
                "trace_id": "trace-trigger-fairness",
                "span_id": "span-trigger-fairness"
            },
            "payload": {
                "payload_kind": "inline_payload",
                "inline_payload_ref": "payload-trigger-fairness",
                "content_type": "application/json",
                "size_bytes": ordinary_payload.len(),
                "blake3_digest_hex": blake3::hash(ordinary_payload).to_hex().to_string()
            },
            "inline_payload_base64": BASE64_STANDARD.encode(ordinary_payload),
            "idempotency_key": "trigger-fairness-ordinary-v1"
        });
        let (call_status, call_body) = post_uds_json(&socket_path, "/calls", &ordinary_call).await;
        assert_eq!(call_status, 200, "{call_body}");
        assert_eq!(call_body["outcome"], "completed");
        let (status_code, status_body) = get_uds_json(&socket_path, "/status").await;
        assert_eq!(status_code, 200, "{status_body}");

        wait_for_trigger_disposition(
            &state_path,
            &created.trigger_authority_id,
            2,
            MctTriggerOccurrenceDisposition::Suppressed,
        )
        .await;
        let recovered = MctRuntimeStateStore::open(&state_path).unwrap();
        assert_eq!(
            recovered
                .list_runs(20)
                .unwrap()
                .iter()
                .filter(|run| run.call.origin == CallOrigin::TriggerFiring)
                .count(),
            1
        );
        assert_eq!(recovered.summary().unwrap().active_trigger_firings, 1);
        assert!(
            recovered
                .call_trigger_pending_occurrences()
                .unwrap()
                .iter()
                .any(|pending| {
                    pending.pending_occurrence_id == expected_pending.pending_occurrence_id
                        && pending.state == "pending"
                })
        );
        assert!(
            recovered
                .call_trigger_occurrences()
                .unwrap()
                .iter()
                .any(|occurrence| {
                    occurrence.record_revision == 2
                        && occurrence.final_disposition
                            == MctTriggerOccurrenceDisposition::Suppressed
                })
        );
        drop(recovered);
        restart_shutdown.send(()).unwrap();
        restarted.await.unwrap().unwrap();

        // A second restart retains all terminal sets and still cannot resurrect
        // the crash-seam firing, pending occurrence, or revoked nominal work.
        let (final_shutdown, final_resident) = start_test_resident(
            paths,
            identity_path,
            ledger_path.clone(),
            socket_path,
            clock,
            TriggerLimits::default(),
        )
        .await;
        for _ in 0..100 {
            let state = MctRuntimeStateStore::open(&state_path).unwrap();
            let pending_suppressed = !state
                .call_trigger_pending_occurrences()
                .unwrap()
                .iter()
                .any(|pending| {
                    pending.pending_occurrence_id == expected_pending.pending_occurrence_id
                })
                && state
                    .call_trigger_occurrences()
                    .unwrap()
                    .iter()
                    .any(|occurrence| {
                        occurrence.occurrence_id == expected_pending.occurrence_id
                            && occurrence.final_disposition
                                == MctTriggerOccurrenceDisposition::Suppressed
                    });
            if pending_suppressed && state.summary().unwrap().active_trigger_firings == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let final_state = MctRuntimeStateStore::open(&state_path).unwrap();
        let final_runs = final_state.list_runs(20).unwrap();
        assert_eq!(
            final_runs
                .iter()
                .filter(|run| {
                    run.call.origin == CallOrigin::TriggerFiring
                        && run.state == mct_daemon::MctRuntimeRunState::Completed
                })
                .count(),
            1
        );
        assert!(final_runs.iter().any(|run| {
            run.call_id == crash_firing.call_id
                && run.state == mct_daemon::MctRuntimeRunState::Denied
        }));
        assert_eq!(final_state.summary().unwrap().active_trigger_firings, 0);
        assert!(
            !final_state
                .call_trigger_pending_occurrences()
                .unwrap()
                .iter()
                .any(|pending| {
                    pending.pending_occurrence_id == expected_pending.pending_occurrence_id
                })
        );
        assert!(
            final_state
                .call_trigger_occurrences()
                .unwrap()
                .iter()
                .any(|occurrence| {
                    occurrence.occurrence_id == expected_pending.occurrence_id
                        && occurrence.final_disposition
                            == MctTriggerOccurrenceDisposition::Suppressed
                })
        );
        drop(final_state);
        final_shutdown.send(()).unwrap();
        final_resident.await.unwrap().unwrap();

        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        let authority_position = entries
            .iter()
            .position(|entry| entry.observation.observation_id == created.authority_observation_id)
            .unwrap();
        let firing_position = entries
            .iter()
            .position(|entry| {
                entry.observation.observation_id == first_firing.firing_observation_id
            })
            .unwrap();
        assert!(authority_position < firing_position);
        assert!(entries.iter().any(|entry| {
            entry.observation.kind == ObservationKind::RouteSelected
                && entry.observation.call_id.as_ref() == Some(&trigger_run.call_id)
        }));
        assert!(entries.iter().any(|entry| {
            entry.observation.kind == ObservationKind::ResultRecorded
                && entry.observation.call_id.as_ref() == Some(&trigger_run.call_id)
        }));
        assert!(entries.iter().any(|entry| {
            entry.observation.observation_id == expected_pending.admission_observation_id
        }));
        assert!(
            entries.iter().any(|entry| {
                entry.observation.observation_id == revoked.authority_observation_id
            })
        );
        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(!ledger_text.contains("trigger-v1:"));
        assert!(!ledger_text.contains(&BASE64_STANDARD.encode(ordinary_payload)));
        assert!(!ledger_text.contains("trigger-result-1"));
    }

    #[tokio::test]
    async fn injected_temporal_trigger_fires_once_and_recovers_without_duplication() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let children_dir = dir.path().join("children");
        let artifact = write_counting_child(&children_dir);
        let config_store = MctDaemonConfigStore::new(&config_path);
        config_store
            .ensure_local_identity(
                MctOperatorNodeScope::default(),
                dir.path().join("identity.key"),
            )
            .unwrap();
        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        config_store
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();

        let payload = b"[]";
        let digest = blake3::hash(payload).to_hex().to_string();
        let payload_constraint = local_blob_store_for_state_path(&state_path)
            .ingest_reader(
                &digest,
                payload.len() as u64,
                "application/json",
                std::io::Cursor::new(payload),
            )
            .unwrap();
        let initial = current_timestamp();
        let first_due = add_millis(&initial, 1_000);
        let expires = add_millis(&initial, 60_000);
        let create = crate::triggers::TriggerCreateRequest {
            expected_config_path: config_path.clone(),
            expected_state_path: state_path.clone(),
            scope: crate::triggers::TriggerAuthorityScopeRequest {
                trigger_authority_id: CallTriggerAuthorityId::new("trigger-integration").unwrap(),
                target: OperationTarget::new("patina:demo", "control@0.1.0", "run").unwrap(),
                payload_constraint,
                trigger_source: CallTriggerSource::Temporal {
                    anchor_at: first_due.clone(),
                    interval_ms: 1_000,
                },
                missed_fire_policy: MissedFirePolicy::Skip,
                overlap_policy: OverlapPolicy::Refuse,
                starts_at: first_due.clone(),
                expires_at: expires,
            },
        };
        let created = crate::triggers::execute_offline_trigger_mutation(
            &config_path,
            &state_path,
            &ledger_path,
            501,
            "/triggers/create",
            &serde_json::to_vec(&create).unwrap(),
        )
        .unwrap();
        assert_eq!(created.record_revision, 1);
        assert_eq!(created.missed_fire_policy, MissedFirePolicy::Skip);
        assert_eq!(created.overlap_policy, OverlapPolicy::Refuse);

        let ledger = spawn_test_ledger_after_writer_release(&ledger_path).await;
        let paths = ResidentRuntimePaths::new(
            config_path.clone(),
            children_dir.clone(),
            state_path.clone(),
        );
        let clock = Arc::new(ManualClock::new(initial.as_str()));
        let mut scheduler = TriggerScheduler::new(
            paths.clone(),
            ledger.clone(),
            clock.clone(),
            TriggerLimits::default(),
        );
        clock.set(first_due.as_str());
        scheduler.evaluate_turn().await.unwrap();
        wait_for_trigger_disposition(
            &state_path,
            &created.trigger_authority_id,
            1,
            MctTriggerOccurrenceDisposition::Fired,
        )
        .await;
        for _ in 0..100 {
            if MctRuntimeStateStore::open(&state_path)
                .unwrap()
                .summary()
                .unwrap()
                .active_trigger_firings
                == 0
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            std::fs::read_to_string(artifact.with_extension("wasm.count"))
                .unwrap()
                .trim(),
            "1"
        );
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        assert_eq!(state.summary().unwrap().active_trigger_firings, 0);
        let runs = state.list_runs(10).unwrap();
        let run = runs
            .iter()
            .find(|run| run.call.origin == CallOrigin::TriggerFiring)
            .expect("trigger call run persisted");
        assert!(run.call.call_id.as_str().starts_with("call-trigger:"));
        assert_eq!(run.state, mct_daemon::MctRuntimeRunState::Completed);
        drop(state);

        scheduler.evaluate_turn().await.unwrap();
        wait_for_trigger_disposition(
            &state_path,
            &created.trigger_authority_id,
            1,
            MctTriggerOccurrenceDisposition::Fired,
        )
        .await;
        assert_eq!(
            std::fs::read_to_string(artifact.with_extension("wasm.count"))
                .unwrap()
                .trim(),
            "1"
        );
        drop(scheduler);
        ledger.close().await;

        let connection = rusqlite::Connection::open(&state_path).unwrap();
        connection
            .execute_batch(
                r#"
                DELETE FROM call_trigger_firings;
                DELETE FROM call_trigger_pending_occurrences;
                DELETE FROM call_trigger_occurrences;
                DELETE FROM call_trigger_authorities;
                DELETE FROM call_trigger_projection_meta;
                "#,
            )
            .unwrap();
        drop(connection);
        reconcile_trigger_projection(&state_path, &ledger_path).unwrap();
        let reconciled = MctRuntimeStateStore::open(&state_path).unwrap();
        assert_eq!(reconciled.call_trigger_authorities().unwrap().len(), 1);
        assert_eq!(reconciled.call_trigger_firings().unwrap().len(), 1);
        assert_eq!(reconciled.summary().unwrap().active_trigger_firings, 0);
        drop(reconciled);

        let recovered_ledger = spawn_test_ledger_after_writer_release(&ledger_path).await;
        let mut recovered = TriggerScheduler::new(
            paths,
            recovered_ledger.clone(),
            clock.clone(),
            TriggerLimits::default(),
        );
        recovered.evaluate_turn().await.unwrap();
        wait_for_trigger_disposition(
            &state_path,
            &created.trigger_authority_id,
            1,
            MctTriggerOccurrenceDisposition::Fired,
        )
        .await;
        assert_eq!(
            std::fs::read_to_string(artifact.with_extension("wasm.count"))
                .unwrap()
                .trim(),
            "1"
        );

        let revoke = crate::triggers::TriggerRevokeRequest {
            expected_config_path: config_path.clone(),
            expected_state_path: state_path.clone(),
            trigger_authority_id: CallTriggerAuthorityId::new("trigger-integration").unwrap(),
            expected_revision: 1,
        };
        let prepared = crate::triggers::prepare_trigger_mutation(
            &config_path,
            &state_path,
            501,
            "/triggers/revoke",
            &serde_json::to_vec(&revoke).unwrap(),
        )
        .unwrap();
        let response = crate::triggers::execute_prepared_resident_trigger_mutation(
            prepared,
            &recovered_ledger,
        )
        .await;
        assert_eq!(response.status_code, 200);
        let revoked: CallTriggerAuthority = serde_json::from_str(&response.body).unwrap();
        assert_eq!(revoked.authority_state, CallTriggerAuthorityState::Revoked);
        clock.set(add_millis(&first_due, 1_000).as_str());
        recovered.evaluate_turn().await.unwrap();
        let revoked_occurrences = MctRuntimeStateStore::open(&state_path)
            .unwrap()
            .call_trigger_occurrences()
            .unwrap();
        assert!(revoked_occurrences.iter().any(|occurrence| {
            occurrence.record_revision == 2
                && occurrence.final_disposition == MctTriggerOccurrenceDisposition::Suppressed
        }));
        assert_eq!(
            std::fs::read_to_string(artifact.with_extension("wasm.count"))
                .unwrap()
                .trim(),
            "1"
        );
        drop(recovered);
        recovered_ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("trigger firing constructed durable call"));
        assert!(ledger_text.contains("trigger firing target result referenced"));
        assert!(ledger_text.contains("trigger-integration"));
        assert!(!ledger_text.contains("trigger-result-1"));
    }

    #[test]
    fn trigger_missed_fire_policies_are_bounded_deterministic_and_countable() {
        let trigger_id = CallTriggerAuthorityId::new("trigger-policy").unwrap();
        let known = (1..=3)
            .map(|second| KnownCallTriggerOccurrence {
                occurrence_id: CallTriggerOccurrenceId::new(format!("occurrence-{second}"))
                    .unwrap(),
                nominal_at: Timestamp::new(format!("2026-07-21T12:00:0{second}Z")).unwrap(),
            })
            .collect::<Vec<_>>();
        let skipped =
            evaluate_missed_fire_policy(&trigger_id, 1, MissedFirePolicy::Skip, &known, 2);
        assert!(skipped.candidates.is_empty());
        assert_eq!(skipped.terminal[0].represented_set.count, 3);
        let coalesced =
            evaluate_missed_fire_policy(&trigger_id, 1, MissedFirePolicy::CoalesceOne, &known, 2);
        assert_eq!(coalesced.candidates.len(), 1);
        assert_eq!(coalesced.candidates[0].represented_set.count, 3);
        let bounded = evaluate_missed_fire_policy(
            &trigger_id,
            1,
            MissedFirePolicy::FireLateBounded,
            &known,
            2,
        );
        assert_eq!(bounded.candidates.len(), 2);
        assert_eq!(bounded.terminal[0].represented_set.count, 1);
    }

    #[test]
    fn production_fire_late_recovery_terminally_refuses_every_excess_occurrence() {
        let plan = fire_late_recovery_plan(
            MCT_TRIGGER_MAX_RECOVERY_RANGE_OCCURRENCES + 1,
            MCT_TRIGGER_MAX_RECOVERY_RANGE_OCCURRENCES,
            MCT_TRIGGER_MAX_EVALUATIONS_PER_TURN,
        );
        assert_eq!(plan.admitted, 31);
        assert_eq!(plan.refused, Some(4_066));
        assert_eq!(plan.evaluations(), MCT_TRIGGER_MAX_EVALUATIONS_PER_TURN);
    }

    #[test]
    fn trigger_overlap_policies_preserve_one_active_call_and_order() {
        let active = firing("active-policy");
        let existing = pending("existing-policy");
        let limits = TriggerLimits::default();
        assert_eq!(
            evaluate_scheduler_admission(
                None,
                OverlapPolicy::Refuse,
                Some(&active),
                None,
                0,
                0,
                0,
                limits,
            ),
            SchedulerAdmissionDecision::Terminal {
                disposition: MctTriggerOccurrenceDisposition::Suppressed,
                stage: "overlap"
            }
        );
        assert_eq!(
            evaluate_scheduler_admission(
                None,
                OverlapPolicy::CoalesceOne,
                Some(&active),
                Some(&existing),
                1,
                1,
                0,
                limits,
            ),
            SchedulerAdmissionDecision::CoalescedInto(existing.pending_occurrence_id.clone())
        );
        assert_eq!(
            evaluate_scheduler_admission(
                None,
                OverlapPolicy::QueueBounded,
                Some(&active),
                None,
                0,
                0,
                0,
                limits,
            ),
            SchedulerAdmissionDecision::Pending(CallTriggerPendingReason::OverlapQueued)
        );
    }

    #[tokio::test]
    async fn trigger_append_failure_suppresses_pending_and_call_effects() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let authority = crate::triggers::authority_for_scheduler_test();
        MctRuntimeStateStore::open(&state_path)
            .unwrap()
            .insert_call_trigger_authority(&authority)
            .unwrap();
        let clock = Arc::new(ManualClock::new("2026-07-21T12:00:00Z"));
        let mut scheduler = TriggerScheduler::new(
            ResidentRuntimePaths::new(
                dir.path().join("config.json"),
                dir.path().join("children"),
                state_path.clone(),
            ),
            ResidentLedgerWriter::failed_for_test(),
            clock,
            TriggerLimits::default(),
        );
        let (pending_candidate, pending_at) = singleton_candidate(&authority, 0).unwrap();
        assert!(
            scheduler
                .admit_pending(
                    &authority,
                    pending_candidate,
                    pending_at,
                    "live",
                    CallTriggerPendingReason::OverlapQueued,
                )
                .await
                .is_err()
        );
        let permit = Arc::clone(&scheduler.active_capacity)
            .try_acquire_owned()
            .unwrap();
        let (firing_candidate, firing_at) = singleton_candidate(&authority, 0).unwrap();
        assert!(
            scheduler
                .admit_firing(&authority, firing_candidate, firing_at, "live", permit,)
                .await
                .is_err()
        );
        let (terminal_candidate, terminal_at) = singleton_candidate(&authority, 0).unwrap();
        assert!(
            scheduler
                .record_terminal_candidate(
                    &authority,
                    terminal_candidate,
                    terminal_at,
                    MctTriggerOccurrenceDisposition::CapacityRefused,
                    "injected_append_failure",
                    "live",
                )
                .await
                .is_err()
        );
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        assert!(state.call_trigger_occurrences().unwrap().is_empty());
        assert!(state.call_trigger_pending_occurrences().unwrap().is_empty());
        assert!(state.call_trigger_firings().unwrap().is_empty());
    }

    #[tokio::test]
    async fn revoked_authority_suppresses_due_gap_after_last_durable_disposition() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let authority = crate::triggers::authority_for_scheduler_test();
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        state.insert_call_trigger_authority(&authority).unwrap();
        drop(state);

        let ledger = ResidentLedgerWriter::spawn(ledger_path).unwrap();
        let paths = ResidentRuntimePaths::new(
            dir.path().join("config.json"),
            dir.path().join("children"),
            state_path.clone(),
        );
        let clock = Arc::new(ManualClock::new("2026-07-21T12:00:00Z"));
        let scheduler = TriggerScheduler::new(
            paths,
            ledger.clone(),
            clock.clone(),
            TriggerLimits::default(),
        );
        let (evaluated, evaluated_at) = singleton_candidate(&authority, 0).unwrap();
        scheduler
            .record_terminal_candidate(
                &authority,
                evaluated,
                evaluated_at,
                MctTriggerOccurrenceDisposition::Skipped,
                "last_evaluation",
                "skip",
            )
            .await
            .unwrap();

        let mut revoked = authority.clone();
        revoked.record_revision = 2;
        revoked.policy_revision = 2;
        revoked.authority_state = CallTriggerAuthorityState::Revoked;
        revoked.authority_observation_id = ObservationId::new("obs-trigger-revoked-gap").unwrap();
        revoked.canonical_record_digest.clear();
        let revoked = revoked.seal();
        MctRuntimeStateStore::open(&state_path)
            .unwrap()
            .insert_call_trigger_authority(&revoked)
            .unwrap();
        rusqlite::Connection::open(&state_path)
            .unwrap()
            .execute(
                "UPDATE call_trigger_authorities SET projected_at = ?1 WHERE record_revision = 2",
                ["2026-07-21T12:00:10Z"],
            )
            .unwrap();

        // Index one became due after the prior turn but before the mutation's
        // wall-clock projection timestamp. Scheduling arithmetic must derive
        // from the durable index-zero disposition and the evaluation clock.
        clock.set("2026-07-21T12:00:01Z");
        scheduler
            .evaluate_revoked_authority(&revoked, &clock.now())
            .await
            .unwrap();
        let suppressed = wait_for_trigger_disposition(
            &state_path,
            &authority.trigger_authority_id,
            2,
            MctTriggerOccurrenceDisposition::Suppressed,
        )
        .await;
        assert_eq!(
            suppressed.nominal_at.unwrap().as_str(),
            "2026-07-21T12:00:01Z"
        );
        drop(scheduler);
        ledger.close().await;
    }

    #[tokio::test]
    async fn trigger_terminal_dispositions_survive_restart_without_resurrection() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let authority = crate::triggers::authority_for_scheduler_test();
        MctRuntimeStateStore::open(&state_path)
            .unwrap()
            .insert_call_trigger_authority(&authority)
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path).unwrap();
        let paths = ResidentRuntimePaths::new(
            dir.path().join("config.json"),
            dir.path().join("children"),
            state_path.clone(),
        );
        let clock = Arc::new(ManualClock::new("2026-07-21T12:00:00Z"));
        let scheduler = TriggerScheduler::new(
            paths.clone(),
            ledger.clone(),
            clock.clone(),
            TriggerLimits::default(),
        );
        let (candidate, nominal_at) = singleton_candidate(&authority, 0).unwrap();
        scheduler
            .record_terminal_candidate(
                &authority,
                candidate,
                nominal_at,
                MctTriggerOccurrenceDisposition::Skipped,
                "missed_fire_skip",
                "skip",
            )
            .await
            .unwrap();
        drop(scheduler);
        ledger.close().await;
        let before = MctRuntimeStateStore::open(&state_path)
            .unwrap()
            .call_trigger_occurrences()
            .unwrap();
        assert_eq!(before.len(), 1);
        assert!(before[0].final_disposition.is_terminal());

        let recovered_ledger =
            ResidentLedgerWriter::spawn(dir.path().join("reopen.jsonl")).unwrap();
        let mut recovered = TriggerScheduler::new(
            paths,
            recovered_ledger.clone(),
            clock,
            TriggerLimits::default(),
        );
        recovered.evaluate_turn().await.unwrap();
        assert_eq!(
            MctRuntimeStateStore::open(&state_path)
                .unwrap()
                .call_trigger_occurrences()
                .unwrap()
                .len(),
            1
        );
        drop(recovered);
        recovered_ledger.close().await;
    }

    #[test]
    fn trigger_evaluate_crash_re_evaluate_cannot_double_fire() {
        let dir = tempfile::tempdir().unwrap();
        let state = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let authority = crate::triggers::authority_for_scheduler_test();
        state.insert_call_trigger_authority(&authority).unwrap();
        let (candidate, nominal_at) = singleton_candidate(&authority, 0).unwrap();
        let occurrence = occurrence_record(
            &authority,
            &candidate,
            Some(nominal_at),
            "live",
            Some("fire_now".into()),
            MctTriggerOccurrenceDisposition::Fired,
            ObservationId::new("obs-fire-once").unwrap(),
            Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
        );
        let firing = MctTriggerFiringRecord {
            firing_id: derive_trigger_firing_identity(&candidate.occurrence_id),
            occurrence_id: candidate.occurrence_id.clone(),
            trigger_authority_id: authority.trigger_authority_id.clone(),
            record_revision: 1,
            policy_revision: 1,
            call_id: derive_trigger_call_identity(&candidate.occurrence_id),
            idempotency_key_ref: "blake3:key".into(),
            firing_observation_id: ObservationId::new("obs-fire-once").unwrap(),
            target_result_ref: None,
            state: "active".into(),
            fired_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
            completed_at: None,
        };
        state
            .insert_call_trigger_firing(&occurrence, &firing)
            .unwrap();
        assert!(
            state
                .insert_call_trigger_firing(&occurrence, &firing)
                .is_err()
        );
        assert_eq!(state.call_trigger_firings().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn trigger_load_does_not_starve_writer_control_status_or_ordinary_calls() {
        let trigger_capacity = Arc::new(tokio::sync::Semaphore::new(MCT_TRIGGER_MAX_ACTIVE_CALLS));
        let ordinary_capacity = Arc::new(tokio::sync::Semaphore::new(64));
        let mut trigger_permits = Vec::new();
        for _ in 0..MCT_TRIGGER_MAX_ACTIVE_CALLS {
            trigger_permits.push(Arc::clone(&trigger_capacity).try_acquire_owned().unwrap());
        }
        assert!(Arc::clone(&trigger_capacity).try_acquire_owned().is_err());
        assert!(Arc::clone(&ordinary_capacity).try_acquire_owned().is_ok());

        let dir = tempfile::tempdir().unwrap();
        let ledger = ResidentLedgerWriter::spawn(dir.path().join("observations.jsonl")).unwrap();
        ledger
            .append(vec![resident_endpoint_observation(
                "obs-trigger-fairness",
                EndpointIdText::new("endpoint-trigger-fairness").unwrap(),
                ObservationOutcome::Completed,
                "ordinary writer progress under trigger saturation",
            )])
            .await
            .unwrap();
        drop(trigger_permits);
        ledger.close().await;
    }

    #[test]
    fn trigger_observation_mapping_uses_existing_kinds() {
        let authority = crate::triggers::authority_for_scheduler_test();
        let (candidate, nominal_at) = singleton_candidate(&authority, 0).unwrap();
        let lifecycle = trigger_observation(
            &authority,
            &candidate.occurrence_id,
            &candidate.represented_set,
            Some(&nominal_at),
            ObservationKind::LifecycleTransitionRecorded,
            ObservationOutcome::Denied,
            "trigger occurrence suppressed",
            "overlap",
            None,
        );
        let firing = trigger_observation(
            &authority,
            &candidate.occurrence_id,
            &candidate.represented_set,
            Some(&nominal_at),
            ObservationKind::CallConstructed,
            ObservationOutcome::Allowed,
            "trigger firing constructed",
            "firing",
            Some(derive_trigger_call_identity(&candidate.occurrence_id)),
        );
        assert_eq!(lifecycle.kind, ObservationKind::LifecycleTransitionRecorded);
        assert_eq!(firing.kind, ObservationKind::CallConstructed);
    }

    #[test]
    fn trigger_admission_order_is_fixed_and_authority_neutral() {
        let limits = TriggerLimits {
            max_pending_per_record: 1,
            max_pending_resident: 1,
            max_active_calls: 1,
            ..TriggerLimits::default()
        };
        let active = firing("active");
        assert_eq!(
            evaluate_scheduler_admission(
                Some(MctTriggerOccurrenceDisposition::Skipped),
                OverlapPolicy::QueueBounded,
                Some(&active),
                None,
                1,
                1,
                1,
                limits,
            ),
            SchedulerAdmissionDecision::Terminal {
                disposition: MctTriggerOccurrenceDisposition::Skipped,
                stage: "missed_fire"
            }
        );
        assert_eq!(
            evaluate_scheduler_admission(
                None,
                OverlapPolicy::Refuse,
                Some(&active),
                None,
                1,
                1,
                1,
                limits,
            ),
            SchedulerAdmissionDecision::Terminal {
                disposition: MctTriggerOccurrenceDisposition::Suppressed,
                stage: "overlap"
            }
        );
    }

    #[test]
    fn trigger_capacity_refuses_at_each_named_bound_without_eviction() {
        let limits = TriggerLimits {
            max_pending_per_record: 1,
            max_pending_resident: 2,
            max_active_calls: 1,
            ..TriggerLimits::default()
        };
        let active = firing("active");
        assert_eq!(
            evaluate_scheduler_admission(
                None,
                OverlapPolicy::QueueBounded,
                Some(&active),
                None,
                1,
                1,
                0,
                limits,
            ),
            SchedulerAdmissionDecision::Terminal {
                disposition: MctTriggerOccurrenceDisposition::CapacityRefused,
                stage: "per_record_pending_capacity"
            }
        );
        assert_eq!(
            evaluate_scheduler_admission(
                None,
                OverlapPolicy::QueueBounded,
                Some(&active),
                None,
                0,
                2,
                0,
                limits,
            ),
            SchedulerAdmissionDecision::Terminal {
                disposition: MctTriggerOccurrenceDisposition::CapacityRefused,
                stage: "resident_pending_capacity"
            }
        );
        assert_eq!(
            evaluate_scheduler_admission(None, OverlapPolicy::Refuse, None, None, 0, 0, 1, limits,),
            SchedulerAdmissionDecision::Terminal {
                disposition: MctTriggerOccurrenceDisposition::CapacityRefused,
                stage: "resident_active_capacity"
            }
        );
    }

    #[test]
    fn trigger_production_limits_are_exactly_named() {
        assert_eq!(TriggerLimits::default().max_pending_per_record, 16);
        assert_eq!(TriggerLimits::default().max_pending_resident, 256);
        assert_eq!(TriggerLimits::default().max_active_calls, 8);
        assert_eq!(TriggerLimits::default().max_evaluations_per_turn, 32);
        assert_eq!(
            TriggerLimits::default().max_recovery_range_occurrences,
            4096
        );
        assert_eq!(MCT_TRIGGER_SCHEDULER_POLL_MS, 50);
    }

    #[test]
    fn temporal_occurrence_range_is_deterministic_and_exclusive_at_expiry() {
        let mut authority = crate::triggers::authority_for_scheduler_test();
        authority.starts_at = Timestamp::new("2026-07-21T12:00:00Z").unwrap();
        authority.expires_at = Timestamp::new("2026-07-21T12:00:03Z").unwrap();
        authority = authority.seal();
        let range = temporal_due_range(
            &authority,
            None,
            &Timestamp::new("2026-07-21T12:00:03Z").unwrap(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(range.first_index, 0);
        assert_eq!(range.last_index, 2);
        assert_eq!(range.count, 3);
    }

    #[test]
    fn manual_trigger_clock_is_injected_not_global() {
        let clock = ManualClock::new("2026-07-21T12:00:00Z");
        assert_eq!(clock.now(), Timestamp::new("2026-07-21T12:00:00Z").unwrap());
        clock.set("2026-07-21T12:00:01Z");
        assert_eq!(clock.now(), Timestamp::new("2026-07-21T12:00:01Z").unwrap());
        let existing = pending("existing");
        assert_eq!(existing.admission_sequence, 1);
    }
}

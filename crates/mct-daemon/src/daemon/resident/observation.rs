use super::*;
use mct_daemon::call_dispatch::{
    CallDispatchCheckV1, CallDispatchClaimV1, CallDispatchIndexV1, CallDispatchReservationV1,
    dispatch_release_observation, dispatch_reservation_observation,
};
use mct_daemon::{
    MotherAuthorityAdmissionDenyV1, MotherAuthorityCommitOutcomeV1, MotherAuthorityOrderV1,
    authority_expectation_from_ledger, authority_expectation_from_snapshot, same_authority_state,
};
use mct_observation::UsableAuthorityProjectionProofV1;

pub(super) enum DispatchOperation {
    Reserve {
        claim: CallDispatchClaimV1,
        observation: MctObservation,
    },
    Release {
        reservation: CallDispatchReservationV1,
        observation: MctObservation,
    },
}

#[derive(Debug)]
pub(super) enum DispatchResponse {
    Reserved(CallDispatchReservationV1),
    Existing,
    FingerprintMismatch,
    Released,
}

fn process_dispatch(
    ledger: &mut JsonlObservationLedger,
    index: &mut CallDispatchIndexV1,
    operation: DispatchOperation,
) -> Result<DispatchResponse> {
    let sequence = ledger
        .verified_head()?
        .local_sequence
        .checked_add(1)
        .context("dispatch sequence overflow")?;
    let (observation, response) = match operation {
        DispatchOperation::Reserve { claim, observation } => {
            match index.check(&claim)? {
                CallDispatchCheckV1::Existing => return Ok(DispatchResponse::Existing),
                CallDispatchCheckV1::FingerprintMismatch => {
                    return Ok(DispatchResponse::FingerprintMismatch);
                }
                CallDispatchCheckV1::Fresh => {}
            }
            let (observation, reservation) =
                dispatch_reservation_observation(observation, claim, sequence)?;
            (observation, DispatchResponse::Reserved(reservation))
        }
        DispatchOperation::Release {
            reservation,
            observation,
        } => {
            // A late release cannot remove a newer attempt after TTL expiry.
            if !index.contains(&reservation) {
                return Ok(DispatchResponse::Released);
            }
            (
                dispatch_release_observation(observation, &reservation, sequence)?,
                DispatchResponse::Released,
            )
        }
    };
    let entries = ledger.append_durable_batch(
        [observation],
        mct_daemon::current_timestamp_string(),
        DurabilityClass::BeforeEffect,
        ExportStatus::NotRequired,
    )?;
    for entry in &entries {
        index.replay_entry(entry)?;
    }
    Ok(response)
}

const RESIDENT_LEDGER_QUEUE_CAPACITY: usize = 256;

#[derive(Clone, Debug, Default)]
struct CachedLedgerTip {
    sequence: u64,
    mother_node_id: String,
    authority: Option<mct_observation::GrantsAuthorityIdentityV1>,
}

fn cached_tip_from_ledger(ledger: &JsonlObservationLedger) -> CachedLedgerTip {
    let Ok(head) = ledger.verified_head() else {
        return CachedLedgerTip::default();
    };
    let authority = ledger
        .verified_replay()
        .ok()
        .and_then(|replay| replay.current_authority.clone());
    CachedLedgerTip {
        sequence: head.local_sequence,
        mother_node_id: head.mother_node_id,
        authority,
    }
}

fn publish_cached_tip(ledger: &JsonlObservationLedger, cached: &std::sync::Mutex<CachedLedgerTip>) {
    let Ok(mut slot) = cached.lock() else {
        return;
    };
    let next = cached_tip_from_ledger(ledger);
    if next.mother_node_id.is_empty() && !slot.mother_node_id.is_empty() {
        return;
    }
    *slot = next;
}

#[derive(Clone, Debug)]
pub(crate) struct ResidentLedgerWriter {
    sender: tokio::sync::mpsc::Sender<ResidentLedgerCommand>,
    fenced: Arc<std::sync::atomic::AtomicBool>,
    path: Option<Arc<PathBuf>>,
    task: Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
    authority_order: Arc<MotherAuthorityOrderV1>,
    cached_tip: Arc<std::sync::Mutex<CachedLedgerTip>>,
}

enum ResidentLedgerCommand {
    Write(ResidentLedgerWrite),
    Dispatch {
        operation: Box<DispatchOperation>,
        ack: tokio::sync::oneshot::Sender<std::result::Result<DispatchResponse, String>>,
    },
    AuthorityMutation {
        request: AuthorityMutationRequestV1,
        state_path: PathBuf,
        ack: tokio::sync::oneshot::Sender<AuthorityMutationResultV1>,
    },
    FinalizeStartup {
        state_path: PathBuf,
        config_path: PathBuf,
        ack: tokio::sync::oneshot::Sender<
            std::result::Result<mct_daemon::MctStartupAuthorityReadinessV1, String>,
        >,
    },
    LegacyAuthorityImport {
        request: Box<LegacyAuthorityImportRequestV1>,
        authenticated_principal_ref: String,
        imported_state: AuthorityStateV1,
        decided_at: String,
        state_path: PathBuf,
        ack: tokio::sync::oneshot::Sender<AuthorityMutationResultV1>,
    },
    /// Publishes the authority projection through the writer's verified head
    /// and proves it for that same head, in one serialized step.
    ProjectedAuthority {
        state_path: PathBuf,
        ack: tokio::sync::oneshot::Sender<std::result::Result<ResidentProjectedAuthority, String>>,
    },
    Shutdown(tokio::sync::oneshot::Sender<()>),
}

/// The writer's verified head and replay, with the authority projection
/// published and proven for exactly that head.
///
/// `proven` is `Err` when the projection cannot be proven usable (a typed,
/// fail-closed reason); the writer itself answered.
#[derive(Clone, Debug)]
pub(crate) struct ResidentProjectedAuthority {
    pub(crate) head: mct_observation::LedgerVerifiedHead,
    pub(crate) replay: mct_observation::AuthorityReplayV1,
    pub(crate) proven: std::result::Result<
        mct_daemon::ProvenAuthorityProjectionV1,
        mct_daemon::LocalExecutionAuthoritySnapshotDenyV1,
    >,
}

impl ResidentProjectedAuthority {
    /// A Mother-local snapshot built from this proof. The projection is not
    /// re-read, so later appends cannot pair it with another head.
    pub(crate) fn snapshot(
        self,
        paths: &ResidentRuntimePaths,
        mother_time: Timestamp,
    ) -> std::result::Result<
        LocalExecutionAuthoritySnapshot,
        mct_daemon::LocalExecutionAuthoritySnapshotDenyV1,
    > {
        mct_daemon::local_execution_authority_snapshot_from_proven(
            self.proven?,
            paths.config_path(),
            paths.children_dir(),
            paths.state_path(),
            Ok(mother_time),
        )
    }
}

fn projected_authority(
    ledger: &JsonlObservationLedger,
    state_path: &Path,
) -> std::result::Result<ResidentProjectedAuthority, String> {
    let head = ledger.verified_head().map_err(|error| error.to_string())?;
    let replay = ledger
        .verified_replay()
        .map_err(|error| error.to_string())?
        .clone();
    let state = MctRuntimeStateStore::open(state_path).map_err(|error| error.to_string())?;
    state
        .publish_authority_projection_from_replay(&head, &replay)
        .map_err(|error| error.to_string())?;
    let proven = mct_daemon::authority_projection_expectation_from_verified(&head, &replay)
        .and_then(|expectation| mct_daemon::prove_authority_projection(&state, expectation));
    Ok(ResidentProjectedAuthority {
        head,
        replay,
        proven,
    })
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) enum TestAuthorityMutationFailure {
    Rejected,
    CommitUnknown,
    WriterPoisoned,
}

struct ResidentLedgerWrite {
    observations: Vec<MctObservation>,
    durability: DurabilityClass,
    ack: tokio::sync::oneshot::Sender<std::result::Result<(), String>>,
}

fn publish_committed_authority_result(
    ledger: &JsonlObservationLedger,
    state_path: &Path,
    result: AuthorityMutationResultV1,
) -> AuthorityMutationResultV1 {
    let AuthorityMutationResultV1::CommittedProjectionPending {
        mutation_id,
        resolution,
        fact_sequence,
        fact_entry_hash,
        grants_authority,
        ..
    } = result
    else {
        return result;
    };
    let publication = ledger
        .verified_head()
        .and_then(|head| {
            ledger
                .verified_replay()
                .map(|replay| (head, replay.clone()))
        })
        .map_err(|error| error.to_string())
        .and_then(|(head, replay)| {
            MctRuntimeStateStore::open(state_path)
                .and_then(|state| state.publish_authority_projection_from_replay(&head, &replay))
                .map_err(|error| error.to_string())
        });
    match publication {
        Ok(cursor) => AuthorityMutationResultV1::Committed {
            mutation_id,
            resolution,
            fact_sequence,
            fact_entry_hash,
            grants_authority,
            projection_hash: cursor.projection_hash,
        },
        Err(_) => AuthorityMutationResultV1::CommittedProjectionPending {
            mutation_id,
            resolution,
            fact_sequence,
            fact_entry_hash,
            grants_authority,
            pending_reason: mct_observation::AuthorityProjectionPendingReasonV1::ProjectionFailed,
        },
    }
}

fn order_outcome_for_result(
    ledger: &JsonlObservationLedger,
    result: &AuthorityMutationResultV1,
) -> MotherAuthorityCommitOutcomeV1 {
    match result {
        AuthorityMutationResultV1::Committed { mutation_id, .. } => {
            match authority_expectation_from_ledger(ledger) {
                Ok(current_expectation) => MotherAuthorityCommitOutcomeV1::Committed {
                    mutation_id: mutation_id.clone(),
                    current_expectation,
                },
                Err(_) => MotherAuthorityCommitOutcomeV1::CommittedProjectionPending {
                    mutation_id: mutation_id.clone(),
                },
            }
        }
        AuthorityMutationResultV1::CommittedProjectionPending { mutation_id, .. } => {
            MotherAuthorityCommitOutcomeV1::CommittedProjectionPending {
                mutation_id: mutation_id.clone(),
            }
        }
        AuthorityMutationResultV1::CommitUnknown { mutation_id, .. } => {
            MotherAuthorityCommitOutcomeV1::CommitUnknown {
                mutation_id: mutation_id.clone(),
            }
        }
        AuthorityMutationResultV1::RejectedBeforeCommit {
            mutation_id,
            reason: AuthorityMutationRejectionReasonV1::WriterPoisoned,
        } => MotherAuthorityCommitOutcomeV1::WriterPoisoned {
            mutation_id: mutation_id.clone(),
        },
        AuthorityMutationResultV1::RejectedBeforeCommit { mutation_id, .. } => {
            MotherAuthorityCommitOutcomeV1::RejectedBeforeCommit {
                mutation_id: mutation_id.clone(),
            }
        }
    }
}

impl ResidentLedgerWriter {
    #[cfg(test)]
    pub(crate) fn fail_after_batches_for_test(path: PathBuf, allowed_batches: usize) -> Self {
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-local", "local-mct").unwrap();
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<ResidentLedgerCommand>(8);
        let fenced = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task_fenced = Arc::clone(&fenced);
        let task = tokio::task::spawn_blocking(move || {
            let mut completed = 0usize;
            while let Some(command) = receiver.blocking_recv() {
                match command {
                    ResidentLedgerCommand::Write(write) => {
                        if completed >= allowed_batches {
                            task_fenced.store(true, Ordering::SeqCst);
                            let _ = write.ack.send(Err("injected resident writer loss".into()));
                            continue;
                        }
                        let appended_at = mct_daemon::current_timestamp_string();
                        let result = write
                            .observations
                            .into_iter()
                            .try_for_each(|observation| {
                                ledger
                                    .append_before_effect(observation, appended_at.clone())
                                    .map(|_| ())
                            })
                            .map_err(|error| error.to_string());
                        completed += 1;
                        let _ = write.ack.send(result);
                    }
                    ResidentLedgerCommand::AuthorityMutation { request, ack, .. } => {
                        task_fenced.store(true, Ordering::SeqCst);
                        let _ = ack.send(AuthorityMutationResultV1::RejectedBeforeCommit {
                            mutation_id: request.mutation_id,
                            reason: AuthorityMutationRejectionReasonV1::WriterPoisoned,
                        });
                    }
                    ResidentLedgerCommand::FinalizeStartup { ack, .. } => {
                        task_fenced.store(true, Ordering::SeqCst);
                        let _ = ack.send(Err("injected resident writer loss".into()));
                    }
                    ResidentLedgerCommand::LegacyAuthorityImport { request, ack, .. } => {
                        task_fenced.store(true, Ordering::SeqCst);
                        let _ = ack.send(AuthorityMutationResultV1::RejectedBeforeCommit {
                            mutation_id: request.import_id,
                            reason: AuthorityMutationRejectionReasonV1::WriterPoisoned,
                        });
                    }
                    ResidentLedgerCommand::Dispatch { ack, .. } => {
                        task_fenced.store(true, Ordering::SeqCst);
                        let _ = ack.send(Err("injected resident writer loss".into()));
                    }
                    ResidentLedgerCommand::ProjectedAuthority { ack, .. } => {
                        let _ = ack.send(Err("injected resident writer loss".into()));
                    }
                    ResidentLedgerCommand::Shutdown(ack) => {
                        let _ = ack.send(());
                        break;
                    }
                }
            }
        });
        Self {
            sender,
            fenced,
            path: Some(Arc::new(path)),
            task: Arc::new(std::sync::Mutex::new(Some(task))),
            authority_order: Arc::new(MotherAuthorityOrderV1::unavailable()),
            cached_tip: Arc::new(std::sync::Mutex::new(CachedLedgerTip::default())),
        }
    }

    #[cfg(test)]
    pub(crate) fn authority_failure_for_test(failure: TestAuthorityMutationFailure) -> Self {
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<ResidentLedgerCommand>(8);
        let fenced = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task_fenced = Arc::clone(&fenced);
        let task = tokio::task::spawn_blocking(move || {
            while let Some(command) = receiver.blocking_recv() {
                match command {
                    ResidentLedgerCommand::AuthorityMutation { request, ack, .. } => {
                        let result = match failure {
                            TestAuthorityMutationFailure::Rejected => {
                                AuthorityMutationResultV1::RejectedBeforeCommit {
                                    mutation_id: request.mutation_id,
                                    reason: AuthorityMutationRejectionReasonV1::InvalidRequest,
                                }
                            }
                            TestAuthorityMutationFailure::CommitUnknown => {
                                task_fenced.store(true, Ordering::SeqCst);
                                AuthorityMutationResultV1::CommitUnknown {
                                    mutation_id: request.mutation_id,
                                    attempted_intent_hash: "injected-unknown".into(),
                                    failure_stage: mct_observation::AppendFailureStage::Durability,
                                }
                            }
                            TestAuthorityMutationFailure::WriterPoisoned => {
                                task_fenced.store(true, Ordering::SeqCst);
                                AuthorityMutationResultV1::RejectedBeforeCommit {
                                    mutation_id: request.mutation_id,
                                    reason: AuthorityMutationRejectionReasonV1::WriterPoisoned,
                                }
                            }
                        };
                        let _ = ack.send(result);
                    }
                    ResidentLedgerCommand::Write(write) => {
                        let _ = write
                            .ack
                            .send(Err("unexpected write after authority refusal".into()));
                    }
                    ResidentLedgerCommand::FinalizeStartup { ack, .. } => {
                        let _ = ack.send(Err("scripted authority writer".into()));
                    }
                    ResidentLedgerCommand::LegacyAuthorityImport { request, ack, .. } => {
                        let _ = ack.send(AuthorityMutationResultV1::RejectedBeforeCommit {
                            mutation_id: request.import_id,
                            reason: AuthorityMutationRejectionReasonV1::InvalidRequest,
                        });
                    }
                    ResidentLedgerCommand::Dispatch { ack, .. } => {
                        let _ = ack.send(Err("scripted authority writer".into()));
                    }
                    ResidentLedgerCommand::ProjectedAuthority { ack, .. } => {
                        let _ = ack.send(Err("scripted authority writer".into()));
                    }
                    ResidentLedgerCommand::Shutdown(ack) => {
                        let _ = ack.send(());
                        break;
                    }
                }
            }
        });
        Self {
            sender,
            fenced,
            path: None,
            task: Arc::new(std::sync::Mutex::new(Some(task))),
            authority_order: Arc::new(MotherAuthorityOrderV1::unavailable()),
            cached_tip: Arc::new(std::sync::Mutex::new(CachedLedgerTip::default())),
        }
    }

    #[cfg(test)]
    pub(crate) fn failed_for_test() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        drop(receiver);
        Self {
            sender,
            fenced: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            path: None,
            task: Arc::new(std::sync::Mutex::new(None)),
            authority_order: Arc::new(MotherAuthorityOrderV1::unavailable()),
            cached_tip: Arc::new(std::sync::Mutex::new(CachedLedgerTip::default())),
        }
    }

    pub(crate) fn spawn(path: PathBuf) -> Result<Self> {
        let ledger = JsonlObservationLedger::open(&path, "ledger-local", "local-mct")
            .with_context(|| format!("open observation ledger {}", path.display()))?;
        Self::spawn_opened(path, ledger)
    }

    #[cfg(test)]
    pub(crate) fn spawn_authority_for_test(path: PathBuf) -> Result<Self> {
        Self::spawn_authority_with_identity_for_test(path, "local-mct")
    }

    #[cfg(test)]
    pub(crate) fn spawn_authority_with_identity_for_test(
        path: PathBuf,
        mother_node_id: &str,
    ) -> Result<Self> {
        let ledger = JsonlObservationLedger::open_authority(&path, "ledger-local", mother_node_id)
            .with_context(|| format!("open test authority ledger {}", path.display()))?;
        Self::spawn_opened(path, ledger)
    }

    pub(crate) fn spawn_authority(
        path: PathBuf,
        mother_node_id: &str,
        startup: mct_observation::AuthorityTenureStartupEvidenceV1,
    ) -> Result<Self> {
        let ledger = JsonlObservationLedger::open_authority_with_startup(
            &path,
            "ledger-local",
            mother_node_id,
            startup,
        )
        .with_context(|| format!("open authority observation ledger {}", path.display()))?;
        Self::spawn_opened(path, ledger)
    }

    fn spawn_opened(path: PathBuf, mut ledger: JsonlObservationLedger) -> Result<Self> {
        let mut dispatch_index = CallDispatchIndexV1::default();
        for entry in ledger.iter_entries() {
            dispatch_index
                .replay_entry(&entry?)
                .context("rebuild keyed dispatch fence")?;
        }
        let fenced = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task_fenced = Arc::clone(&fenced);
        let authority_order = Arc::new(MotherAuthorityOrderV1::from_ledger(&ledger));
        let task_authority_order = Arc::clone(&authority_order);
        let cached_tip = Arc::new(std::sync::Mutex::new(cached_tip_from_ledger(&ledger)));
        let task_cached_tip = Arc::clone(&cached_tip);
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<ResidentLedgerCommand>(RESIDENT_LEDGER_QUEUE_CAPACITY);
        let task = tokio::task::spawn_blocking(move || {
            while let Some(command) = receiver.blocking_recv() {
                match command {
                    ResidentLedgerCommand::Write(write) => {
                        let appended_at = mct_daemon::current_timestamp_string();
                        let result = ledger
                            .append_durable_batch(
                                write.observations,
                                appended_at,
                                write.durability,
                                ExportStatus::NotRequired,
                            )
                            .map(|_| ())
                            .map_err(|error| error.to_string());
                        publish_cached_tip(&ledger, &task_cached_tip);
                        let _ = write.ack.send(result);
                    }
                    ResidentLedgerCommand::AuthorityMutation {
                        request,
                        state_path,
                        ack,
                    } => {
                        let mutation_id = request.mutation_id.clone();
                        let mut committed_result = None;
                        task_authority_order.commit_mutation(
                            &mutation_id,
                            &request,
                            |ordered_request| {
                                let result = if ledger.authority_tenure().is_some() {
                                    let result = ledger.execute_authority_mutation(
                                        ordered_request.clone(),
                                        |_| Ok(None),
                                    );
                                    publish_committed_authority_result(
                                        &ledger,
                                        &state_path,
                                        result,
                                    )
                                } else {
                                    AuthorityMutationResultV1::RejectedBeforeCommit {
                                        mutation_id: ordered_request.mutation_id.clone(),
                                        reason: AuthorityMutationRejectionReasonV1::AuthorityEpochUnavailable,
                                    }
                                };
                                let outcome = order_outcome_for_result(&ledger, &result);
                                committed_result = Some(result);
                                outcome
                            },
                        );
                        let result = committed_result.unwrap_or(
                            AuthorityMutationResultV1::RejectedBeforeCommit {
                                mutation_id,
                                reason: AuthorityMutationRejectionReasonV1::WriterPoisoned,
                            },
                        );
                        publish_cached_tip(&ledger, &task_cached_tip);
                        let _ = ack.send(result);
                    }
                    ResidentLedgerCommand::Dispatch { operation, ack } => {
                        let result = if task_fenced.load(Ordering::SeqCst) || ledger.is_poisoned() {
                            Err(anyhow::anyhow!("resident observation writer is fenced"))
                        } else {
                            process_dispatch(&mut ledger, &mut dispatch_index, *operation)
                        };
                        if result.is_err() {
                            task_fenced.store(true, Ordering::SeqCst);
                        }
                        publish_cached_tip(&ledger, &task_cached_tip);
                        let _ = ack.send(result.map_err(|error| error.to_string()));
                    }
                    ResidentLedgerCommand::ProjectedAuthority { state_path, ack } => {
                        let _ = ack.send(projected_authority(&ledger, &state_path));
                    }
                    ResidentLedgerCommand::FinalizeStartup {
                        state_path,
                        config_path,
                        ack,
                    } => {
                        let result = mct_daemon::finalize_authority_startup(
                            &mut ledger,
                            &state_path,
                            &config_path,
                        )
                        .map_err(|error| error.to_string());
                        publish_cached_tip(&ledger, &task_cached_tip);
                        let _ = ack.send(result);
                    }
                    ResidentLedgerCommand::LegacyAuthorityImport {
                        request,
                        authenticated_principal_ref,
                        imported_state,
                        decided_at,
                        state_path,
                        ack,
                    } => {
                        let mutation_id = request.import_id.clone();
                        let mut committed_result = None;
                        task_authority_order.commit_mutation(&mutation_id, &(), |_| {
                            let result = if ledger.authority_tenure().is_some() {
                                let result = ledger.execute_legacy_authority_import(
                                    *request,
                                    authenticated_principal_ref,
                                    imported_state,
                                    decided_at,
                                );
                                publish_committed_authority_result(&ledger, &state_path, result)
                            } else {
                                AuthorityMutationResultV1::RejectedBeforeCommit {
                                    mutation_id: mutation_id.clone(),
                                    reason: AuthorityMutationRejectionReasonV1::AuthorityEpochUnavailable,
                                }
                            };
                            let outcome = order_outcome_for_result(&ledger, &result);
                            committed_result = Some(result);
                            outcome
                        });
                        let result = committed_result.unwrap_or(
                            AuthorityMutationResultV1::RejectedBeforeCommit {
                                mutation_id,
                                reason: AuthorityMutationRejectionReasonV1::WriterPoisoned,
                            },
                        );
                        publish_cached_tip(&ledger, &task_cached_tip);
                        let _ = ack.send(result);
                    }
                    ResidentLedgerCommand::Shutdown(ack) => {
                        let _ = ack.send(());
                        break;
                    }
                }
            }
        });
        Ok(Self {
            sender,
            fenced,
            path: Some(Arc::new(path)),
            task: Arc::new(std::sync::Mutex::new(Some(task))),
            authority_order,
            cached_tip,
        })
    }

    pub(crate) fn cached_sequence_tip(&self) -> u64 {
        self.cached_tip.lock().map(|tip| tip.sequence).unwrap_or(0)
    }

    pub(crate) fn cached_receiver_authority(&self) -> Option<GrantsAuthorityIdentity> {
        let tip = self.cached_tip.lock().ok()?;
        let authority = tip.authority.clone()?;
        Some(GrantsAuthorityIdentity {
            mother_node_id: authority.mother_node_id,
            authority_epoch: authority.authority_epoch,
            generation: authority.generation,
            source_authority_observation_id: authority.source_authority_observation_id,
        })
    }

    pub(crate) fn is_fenced(&self) -> bool {
        self.fenced.load(Ordering::SeqCst)
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref().map(PathBuf::as_path)
    }

    pub(crate) fn admit_effect(
        &self,
        snapshot: &LocalExecutionAuthoritySnapshot,
        state_path: &Path,
    ) -> std::result::Result<(), MotherAuthorityAdmissionDenyV1> {
        self.admit_effect_start(snapshot, state_path, &mut || {})
    }

    /// Admits one effect start for `snapshot` under the authority order.
    ///
    /// Ordinary observations keep moving the ledger head while a call runs,
    /// so the snapshot's head is usually behind by the time the effect is
    /// about to start. The proof is therefore taken fresh from the writer
    /// (published and proven for its current head in one serialized step),
    /// and admission requires that the snapshot and that fresh proof name
    /// the same authority identity and state. The order lock then rejects
    /// any authority mutation that committed after the proof. The proof is
    /// taken before the lock: the writer locks the order to commit
    /// mutations, so asking it while holding the lock could deadlock.
    pub(crate) fn admit_effect_start(
        &self,
        snapshot: &LocalExecutionAuthoritySnapshot,
        state_path: &Path,
        start: &mut dyn FnMut(),
    ) -> std::result::Result<(), MotherAuthorityAdmissionDenyV1> {
        let snapshot_expectation = authority_expectation_from_snapshot(snapshot);
        let projected = self
            .projected_authority_blocking(state_path.to_path_buf())
            .map_err(|_| MotherAuthorityAdmissionDenyV1::AuthorityStateUnavailable)?;
        let current = match projected.proven {
            Ok(proven) => proven.cursor,
            Err(mct_daemon::LocalExecutionAuthoritySnapshotDenyV1::AuthorityProjection(reason)) => {
                return Err(MotherAuthorityAdmissionDenyV1::Projection(reason));
            }
            Err(_) => return Err(MotherAuthorityAdmissionDenyV1::AuthorityStateUnavailable),
        };
        let current_expectation = mct_daemon::authority_projection_expectation_from_verified(
            &projected.head,
            &projected.replay,
        )
        .map_err(|_| MotherAuthorityAdmissionDenyV1::AuthorityStateUnavailable)?;
        if !same_authority_state(&snapshot_expectation, &current_expectation) {
            return Err(MotherAuthorityAdmissionDenyV1::ExactAuthorityStateMismatch);
        }
        self.authority_order.admit_effect(
            &current_expectation,
            || UsableAuthorityProjectionProofV1::Usable {
                cursor: Box::new(current),
            },
            |_| start(),
        )
    }

    pub(crate) fn projected_authority_blocking(
        &self,
        state_path: PathBuf,
    ) -> Result<ResidentProjectedAuthority> {
        if self.is_fenced() {
            bail!("resident observation writer is fenced");
        }
        let (ack, rx) = tokio::sync::oneshot::channel();
        self.sender
            .blocking_send(ResidentLedgerCommand::ProjectedAuthority { state_path, ack })
            .context("send blocking projected authority read")?;
        rx.blocking_recv()
            .context("receive blocking projected authority read")?
            .map_err(anyhow::Error::msg)
    }

    pub(crate) async fn projected_authority(
        &self,
        state_path: PathBuf,
    ) -> Result<ResidentProjectedAuthority> {
        if self.is_fenced() {
            bail!("resident observation writer is fenced");
        }
        let (ack, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ResidentLedgerCommand::ProjectedAuthority { state_path, ack })
            .await
            .context("send projected authority read to resident ledger writer")?;
        rx.await
            .context("receive projected authority read")?
            .map_err(anyhow::Error::msg)
    }

    pub(crate) async fn finalize_startup(
        &self,
        state_path: PathBuf,
        config_path: PathBuf,
    ) -> Result<mct_daemon::MctStartupAuthorityReadinessV1> {
        if self.is_fenced() {
            bail!("resident observation writer is fenced");
        }
        let (ack, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ResidentLedgerCommand::FinalizeStartup {
                state_path,
                config_path,
                ack,
            })
            .await
            .context("send startup finalization to resident ledger writer")?;
        rx.await
            .context("receive resident startup finalization acknowledgement")?
            .map_err(anyhow::Error::msg)
    }

    pub(crate) async fn commit_legacy_authority_import(
        &self,
        request: LegacyAuthorityImportRequestV1,
        authenticated_principal_ref: String,
        imported_state: AuthorityStateV1,
        decided_at: String,
        state_path: PathBuf,
    ) -> Result<AuthorityMutationResultV1> {
        if self.is_fenced() {
            return Ok(AuthorityMutationResultV1::RejectedBeforeCommit {
                mutation_id: request.import_id,
                reason: AuthorityMutationRejectionReasonV1::WriterPoisoned,
            });
        }
        let (ack, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ResidentLedgerCommand::LegacyAuthorityImport {
                request: Box::new(request),
                authenticated_principal_ref,
                imported_state,
                decided_at,
                state_path,
                ack,
            })
            .await
            .context("send legacy authority import to resident ledger writer")?;
        rx.await
            .context("receive resident legacy authority import acknowledgement")
    }

    pub(crate) async fn commit_authority_mutation(
        &self,
        request: AuthorityMutationRequestV1,
        state_path: PathBuf,
    ) -> Result<AuthorityMutationResultV1> {
        if self.is_fenced() {
            return Ok(AuthorityMutationResultV1::RejectedBeforeCommit {
                mutation_id: request.mutation_id,
                reason: AuthorityMutationRejectionReasonV1::WriterPoisoned,
            });
        }
        let (ack, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ResidentLedgerCommand::AuthorityMutation {
                request,
                state_path,
                ack,
            })
            .await
            .context("send authority mutation to resident ledger writer")?;
        rx.await
            .context("receive resident authority mutation acknowledgement")
    }

    pub(super) async fn dispatch(&self, operation: DispatchOperation) -> Result<DispatchResponse> {
        if self.is_fenced() {
            bail!("resident observation writer is fenced");
        }
        let (ack, rx) = tokio::sync::oneshot::channel();
        let result = async {
            self.sender
                .send(ResidentLedgerCommand::Dispatch {
                    operation: Box::new(operation),
                    ack,
                })
                .await
                .context("send keyed dispatch fact")?;
            rx.await
                .context("receive keyed dispatch acknowledgement")?
                .map_err(anyhow::Error::msg)
        }
        .await;
        if result.is_err() {
            self.fenced.store(true, Ordering::SeqCst);
        }
        result
    }

    pub(crate) async fn append(&self, observations: Vec<MctObservation>) -> Result<()> {
        self.append_with_durability(observations, DurabilityClass::BeforeEffect)
            .await
    }

    pub(crate) async fn append_with_durability(
        &self,
        observations: Vec<MctObservation>,
        durability: DurabilityClass,
    ) -> Result<()> {
        if observations.is_empty() {
            return Ok(());
        }
        if self.is_fenced() {
            bail!("resident observation writer is fenced");
        }
        let (ack, rx) = tokio::sync::oneshot::channel();
        if let Err(error) = self
            .sender
            .send(ResidentLedgerCommand::Write(ResidentLedgerWrite {
                observations,
                durability,
                ack,
            }))
            .await
        {
            self.fenced.store(true, Ordering::SeqCst);
            return Err(error).context("send observations to resident ledger writer");
        }
        let result = match rx.await {
            Ok(result) => result.map_err(anyhow::Error::msg),
            Err(error) => Err(error).context("receive resident ledger writer acknowledgement"),
        };
        if result.is_err() {
            self.fenced.store(true, Ordering::SeqCst);
        }
        result
    }

    pub(crate) async fn close(self) {
        let (ack, acknowledged) = tokio::sync::oneshot::channel();
        let _ = self.sender.send(ResidentLedgerCommand::Shutdown(ack)).await;
        let _ = acknowledged.await;
        let task = self
            .task
            .lock()
            .expect("resident ledger task mutex must not be poisoned")
            .take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

pub(crate) fn resident_iroh_observation_sink(
    ledger: ResidentLedgerWriter,
) -> MctIrohObservationSink {
    MctIrohObservationSink::new(move |batch: MctIrohObservationBatch| {
        let ledger = ledger.clone();
        async move {
            let durability = match batch.durability {
                MctIrohObservationDurability::BeforeEffect => DurabilityClass::BeforeEffect,
                MctIrohObservationDurability::Buffered => DurabilityClass::Buffered,
            };
            let observed_at = current_timestamp();
            let observations = batch
                .facts
                .iter()
                .map(|fact| {
                    let mut observation = fact.to_observation(observed_at.clone());
                    if let Some(caller_call_id) = observation.call_id.take() {
                        observation.detail_ref = Some(format!("mct-peer-transport-correlation-v1:{}", serde_json::json!({
                            "caller_call_id": caller_call_id, "transport_detail": observation.detail_ref
                        })));
                    }
                    observation
                })
                .collect();
            ledger
                .append_with_durability(observations, durability)
                .await
                .map_err(|error| std::io::Error::other(error.to_string()))
        }
    })
}

pub(super) fn resident_endpoint_observation(
    observation_id: &'static str,
    endpoint_id: EndpointIdText,
    outcome: ObservationOutcome,
    safe_message: &'static str,
) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::new(observation_id)
            .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::AdapterEffectCompleted,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: TraceId::new("trace-resident-mother")
                .expect("string ID literal/generated value must be non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(endpoint_id.to_string()),
        resource_id: Some("mct-iroh-endpoint".into()),
        policy_revision: Some(1),
        grants_revision: Some(1),
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn call_dispatch_unknown_schema_blocks_writer_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let ledger = ResidentLedgerWriter::spawn_authority_for_test(path.clone()).unwrap();
        let mut observation = resident_endpoint_observation(
            "unknown-dispatch",
            EndpointIdText::new("endpoint-test").unwrap(),
            ObservationOutcome::Started,
            "test unknown schema",
        );
        observation.detail_ref = Some("mct-call-dispatch-v2:{}".into());
        ledger.append(vec![observation]).await.unwrap();
        ledger.close().await;
        let error = ResidentLedgerWriter::spawn_authority_for_test(path).unwrap_err();
        assert!(error.to_string().contains("rebuild keyed dispatch fence"));
    }

    fn contract_peer_expiry() -> Timestamp {
        Timestamp::new("2099-01-01T00:00:00Z").unwrap()
    }

    #[tokio::test]
    async fn resident_observation_queue_is_bounded_and_acknowledged() {
        let dir = tempfile::tempdir().unwrap();
        let ledger = ResidentLedgerWriter::spawn(dir.path().join("observations.jsonl")).unwrap();

        assert_eq!(ledger.sender.max_capacity(), RESIDENT_LEDGER_QUEUE_CAPACITY);
        assert_eq!(ledger.sender.capacity(), RESIDENT_LEDGER_QUEUE_CAPACITY);
        ledger
            .append_with_durability(
                vec![resident_endpoint_observation(
                    "obs-bounded-resident-writer",
                    EndpointIdText::new("endpoint-bounded-resident-writer").unwrap(),
                    ObservationOutcome::Completed,
                    "bounded resident writer",
                )],
                DurabilityClass::Buffered,
            )
            .await
            .unwrap();
        assert_eq!(ledger.sender.capacity(), RESIDENT_LEDGER_QUEUE_CAPACITY);
        ledger.close().await;

        let entries = JsonlObservationLedger::open_read_only(
            dir.path().join("observations.jsonl"),
            "ledger-local",
            "local-mct",
        )
        .unwrap()
        .entries()
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].durability_class, DurabilityClass::Buffered);
    }

    #[tokio::test]
    async fn resident_hello_observations_are_durable_before_responses() {
        let dir = tempfile::tempdir().unwrap();
        let ledger_path = dir.path().join("observations.jsonl");
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut admitted_client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut denied_client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let ticket = server.ticket();
        let admitted_endpoint_id = admitted_client.snapshot().endpoint_id;
        let denied_endpoint_id = denied_client.snapshot().endpoint_id;
        let binding = MctPeerBinding {
            binding_id: PeerBindingId::new("binding-durable-hello")
                .expect("string ID literal/generated value must be non-empty"),
            iroh_endpoint_id: admitted_endpoint_id.clone(),
            scope: MctPeerBindingScope {
                mct_node_id: MctNodeId::new("mother-durable-client")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
                data_scope: None,
                observation_scope: None,
            },
            issuer_node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            policy_revision: 1,
            binding_state: BindingState::Admitted,
            issued_at: Timestamp::new("2026-07-09T00:00:00Z").unwrap(),
            expires_at: contract_peer_expiry(),
            created_by_observation_id: ObservationId::new("obs-binding-durable-hello")
                .expect("string ID literal/generated value must be non-empty"),
            superseded_by_observation_id: None,
        };
        let observation_sink = resident_iroh_observation_sink(ledger.clone());
        let serve_task = tokio::spawn(async move {
            server
                .serve_concurrent_with_call_handler(
                    MctIrohServeState::new(),
                    vec![binding],
                    MctIrohConcurrentServeConfig {
                        receiver_authority_provider: test_receiver_authority_provider(),
                        ..MctIrohConcurrentServeConfig::new(observation_sink)
                    },
                    || Timestamp::new("2026-07-09T00:00:02Z").unwrap(),
                    |_, _, _| async { MctIrohCallHandlerResult::accepted_for_routing(None) },
                )
                .await
        });

        let admitted_trace = TraceId::new("trace-durable-admitted-hello")
            .expect("string ID literal/generated value must be non-empty");
        let signature_marker = "key-material-must-not-enter-hello-observation";
        let admitted_hello = cli_hello_request(
            &admitted_endpoint_id,
            &PeerBindingId::new("binding-durable-hello")
                .expect("string ID literal/generated value must be non-empty"),
            &MctNodeId::new("mother-durable-client")
                .expect("string ID literal/generated value must be non-empty"),
            &VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            &admitted_trace,
            Some(signature_marker.into()),
        );
        let admitted_response = admitted_client
            .send_hello(&ticket, &admitted_hello)
            .await
            .unwrap();
        assert_eq!(admitted_response.hello_outcome, HelloOutcome::Admitted);
        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        assert!(entries.iter().any(|entry| {
            entry.observation.trace.trace_id == admitted_trace
                && entry.observation.kind == ObservationKind::PeerAdmitted
                && entry.durability_class == mct_observation::DurabilityClass::BeforeEffect
        }));

        let denied_trace = TraceId::new("trace-durable-denied-hello")
            .expect("string ID literal/generated value must be non-empty");
        let denied_hello = cli_hello_request(
            &denied_endpoint_id,
            &PeerBindingId::new("binding-durable-hello")
                .expect("string ID literal/generated value must be non-empty"),
            &MctNodeId::new("mother-unknown-client")
                .expect("string ID literal/generated value must be non-empty"),
            &VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            &denied_trace,
            Some(signature_marker.into()),
        );
        let denied_response = denied_client
            .send_hello(&ticket, &denied_hello)
            .await
            .unwrap();
        assert_eq!(denied_response.hello_outcome, HelloOutcome::Denied);
        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        assert!(entries.iter().any(|entry| {
            entry.observation.trace.trace_id == denied_trace
                && entry.observation.kind == ObservationKind::PeerRejected
                && entry.durability_class == mct_observation::DurabilityClass::BeforeEffect
        }));
        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(!ledger_text.contains(signature_marker));
        assert!(!ledger_text.contains("inline_payload_base64"));

        admitted_client.close().await;
        denied_client.close().await;
        serve_task.abort();
        ledger.close().await;
    }
}

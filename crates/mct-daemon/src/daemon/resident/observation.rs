//! Resident observation persistence and Iroh durability adaptation.

use super::*;
use mct_daemon::{
    MotherAuthorityAdmissionDenyV1, MotherAuthorityCommitOutcomeV1, MotherAuthorityOrderV1,
    authority_expectation_from_ledger, authority_expectation_from_snapshot,
};
use mct_observation::{
    AuthorityProjectionDenyReasonV1, AuthorityProjectionLedgerEvidenceV1,
    UsableAuthorityProjectionProofV1,
};

const RESIDENT_LEDGER_QUEUE_CAPACITY: usize = 256;

#[derive(Clone, Debug)]
pub(crate) struct ResidentLedgerWriter {
    sender: tokio::sync::mpsc::Sender<ResidentLedgerCommand>,
    fenced: Arc<std::sync::atomic::AtomicBool>,
    path: Option<Arc<PathBuf>>,
    task: Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
    authority_order: Arc<MotherAuthorityOrderV1>,
}

enum ResidentLedgerCommand {
    Write(ResidentLedgerWrite),
    AuthorityMutation {
        request: AuthorityMutationRequestV1,
        state_path: PathBuf,
        ack: tokio::sync::oneshot::Sender<AuthorityMutationResultV1>,
    },
    PublishAuthorityProjection {
        state_path: PathBuf,
        ack: tokio::sync::oneshot::Sender<std::result::Result<(), String>>,
    },
    FinalizeStartup {
        state_path: PathBuf,
        config_path: PathBuf,
        ack: tokio::sync::oneshot::Sender<
            std::result::Result<mct_daemon::MctStartupAuthorityReadinessV1, String>,
        >,
    },
    LegacyAuthorityImport {
        request: LegacyAuthorityImportRequestV1,
        authenticated_principal_ref: String,
        imported_state: AuthorityStateV1,
        decided_at: String,
        state_path: PathBuf,
        ack: tokio::sync::oneshot::Sender<AuthorityMutationResultV1>,
    },
    Shutdown(tokio::sync::oneshot::Sender<()>),
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
        .entries()
        .map_err(|error| error.to_string())
        .and_then(|entries| {
            MctRuntimeStateStore::open(state_path)
                .and_then(|state| state.publish_authority_projection(&entries))
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
                    ResidentLedgerCommand::PublishAuthorityProjection { ack, .. } => {
                        task_fenced.store(true, Ordering::SeqCst);
                        let _ = ack.send(Err("injected resident writer loss".into()));
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
                    ResidentLedgerCommand::PublishAuthorityProjection { ack, .. } => {
                        let _ = ack.send(Err("scripted authority writer".into()));
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
        let authority_order = Arc::new(MotherAuthorityOrderV1::from_ledger(&ledger));
        let task_authority_order = Arc::clone(&authority_order);
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<ResidentLedgerCommand>(RESIDENT_LEDGER_QUEUE_CAPACITY);
        let task = tokio::task::spawn_blocking(move || {
            while let Some(command) = receiver.blocking_recv() {
                match command {
                    ResidentLedgerCommand::Write(write) => {
                        let appended_at = mct_daemon::current_timestamp_string();
                        let result = write
                            .observations
                            .into_iter()
                            .try_for_each(|observation| match write.durability {
                                DurabilityClass::BeforeEffect => ledger
                                    .append_before_effect(observation, appended_at.clone())
                                    .map(|_| ()),
                                DurabilityClass::Buffered | DurabilityClass::ProjectionOnly => {
                                    ledger
                                        .append(
                                            observation,
                                            appended_at.clone(),
                                            write.durability,
                                            ExportStatus::NotRequired,
                                        )
                                        .map(|_| ())
                                }
                            })
                            .map_err(|error| error.to_string());
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
                        let _ = ack.send(result);
                    }
                    ResidentLedgerCommand::PublishAuthorityProjection { state_path, ack } => {
                        let result = ledger
                            .entries()
                            .map_err(|error| error.to_string())
                            .and_then(|entries| {
                                MctRuntimeStateStore::open(&state_path)
                                    .and_then(|state| {
                                        state.publish_authority_projection(&entries).map(|_| ())
                                    })
                                    .map_err(|error| error.to_string())
                            });
                        let _ = ack.send(result);
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
                                    request,
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
            fenced: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            path: Some(Arc::new(path)),
            task: Arc::new(std::sync::Mutex::new(Some(task))),
            authority_order,
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

    pub(crate) fn admit_effect_start(
        &self,
        snapshot: &LocalExecutionAuthoritySnapshot,
        state_path: &Path,
        start: &mut dyn FnMut(),
    ) -> std::result::Result<(), MotherAuthorityAdmissionDenyV1> {
        let expectation = authority_expectation_from_snapshot(snapshot);
        self.authority_order.admit_effect(
            &expectation,
            || {
                MctRuntimeStateStore::open(state_path)
                    .ok()
                    .and_then(|state| {
                        state
                            .usable_authority_projection_proof(
                                &AuthorityProjectionLedgerEvidenceV1::Validated(
                                    expectation.clone(),
                                ),
                            )
                            .ok()
                    })
                    .unwrap_or(UsableAuthorityProjectionProofV1::Denied {
                        reason: AuthorityProjectionDenyReasonV1::ProjectionNotCurrent,
                    })
            },
            |_| start(),
        )
    }

    pub(crate) fn publish_authority_projection_blocking(&self, state_path: PathBuf) -> Result<()> {
        if self.is_fenced() {
            bail!("resident observation writer is fenced");
        }
        let (ack, rx) = tokio::sync::oneshot::channel();
        self.sender
            .blocking_send(ResidentLedgerCommand::PublishAuthorityProjection { state_path, ack })
            .context("send blocking authority projection publication")?;
        rx.blocking_recv()
            .context("receive blocking authority projection publication")?
            .map_err(anyhow::Error::msg)
    }

    pub(crate) async fn publish_authority_projection(&self, state_path: PathBuf) -> Result<()> {
        if self.is_fenced() {
            bail!("resident observation writer is fenced");
        }
        let (ack, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ResidentLedgerCommand::PublishAuthorityProjection { state_path, ack })
            .await
            .context("send authority projection publication to resident ledger writer")?;
        rx.await
            .context("receive authority projection publication acknowledgement")?
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
                request,
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
                .map(|fact| fact.to_observation(observed_at.clone()))
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

    fn contract_peer_expiry() -> Timestamp {
        Timestamp::new("2099-01-01T00:00:00Z").unwrap()
    }

    /// Covers `MctLocalFirstObservationLedger.BufferedEffectsAreBounded`.
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
                    MctIrohConcurrentServeConfig::new(observation_sink),
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

//! Append-only observation ledger support for MCT.
//!
//! Runtime truth starts from `MctObservation` facts defined by `mct-kernel`.
//! Storage details stay in this crate and do not leak into the kernel.

#![forbid(unsafe_code)]

use mct_kernel::{
    CallId, CanonicalToyContract, MctObservation, ObservationId, ObservationKind,
    ObservationOutcome, ObservationTraceRef, ObservationVisibility, SourcePlane, Timestamp,
    ToyContractIdentity, ToyGrant, ToyGrantConstraints, ToyGrantId, ToyGrantScope, ToyGrantState,
    ToyGrantSubject, ToyId, TraceId,
};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

/// Reserved `MctObservation.detail_ref` carrier for inline canonical authority facts.
pub const AUTHORITY_FACT_DETAIL_PREFIX: &str = "mct-authority-fact-v1:";
const AUTHORITY_FACT_SCHEMA_V1: &str = "mct-authority-fact/v1";
const AUTHORITY_EPOCH_PREFIX_V1: &str = "mct-authority-epoch-v1:";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantsAuthorityIdentityV1 {
    pub mother_node_id: String,
    pub authority_epoch: String,
    pub generation: u64,
    pub source_authority_observation_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthorityEpochPredecessorV1 {
    NoneForVirgin,
    ValidatedHead { sequence: u64, entry_hash: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityStartupClassV1 {
    Virgin,
    OrdinaryReopen,
    OperatorGatedNonvirgin,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriterTenureEstablishmentV1 {
    pub started_at: String,
    pub startup_class: AuthorityStartupClassV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_gate_decision_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authenticated_principal_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochEstablishedFactV1 {
    pub mother_node_id: String,
    pub ledger_id: String,
    pub authority_epoch: String,
    pub predecessor: AuthorityEpochPredecessorV1,
    pub generation_baseline: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_authority: Option<GrantsAuthorityIdentityV1>,
    pub resulting_authority: GrantsAuthorityIdentityV1,
    pub grant_state_hash: String,
    pub establishment: WriterTenureEstablishmentV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantShapingCommandKindV1 {
    AuthorizeSlate,
    AuthorizeSecret,
    CatalogChange,
    GrantChange,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source_kind", rename_all = "snake_case")]
pub enum GrantShapingSourceV1 {
    OperatorDecision {
        decision_id: String,
        authenticated_principal_ref: String,
        command_kind: GrantShapingCommandKindV1,
    },
    ChildApproval {
        child_name: String,
        artifact_id: String,
        artifact_version: String,
        authority_observation_id: String,
    },
    ChildAssignment {
        assignment_id: String,
        authority_observation_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change_kind", rename_all = "snake_case")]
pub enum AuthorityChangeV1 {
    ToyCatalogPut {
        toy_id: String,
        contract: ToyContractIdentity,
        authority_bearing: bool,
        catalog_revision: u64,
        admitted_by_observation_id: String,
    },
    ToyCatalogRemove {
        toy_id: String,
    },
    ToyGrantPut {
        grant_id: String,
        toy_id: String,
        subject: Box<ToyGrantSubject>,
        scope: Box<ToyGrantScope>,
        constraints: Box<ToyGrantConstraints>,
        grant_state: ToyGrantState,
        issuer_id: String,
        policy_revision: u64,
        source_grants_revision: u64,
        authority_observation_id: String,
    },
    ToyGrantRemove {
        grant_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityStateReferenceV1 {
    pub grants_authority: GrantsAuthorityIdentityV1,
    pub authority_state_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityMutationFactV1 {
    pub mutation_id: String,
    pub mutation_intent_hash: String,
    pub mother_node_id: String,
    pub ledger_id: String,
    pub authority_epoch: String,
    pub prior_state: AuthorityStateReferenceV1,
    pub changes: Vec<AuthorityChangeV1>,
    pub grant_shaping_sources: Vec<GrantShapingSourceV1>,
    pub resulting_state: AuthorityStateReferenceV1,
    pub decided_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedAuthorityMutationV1 {
    pub fact: AuthorityMutationFactV1,
    pub entry_sequence: u64,
    pub entry_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityMutationRequestV1 {
    pub mutation_id: String,
    pub changes: Vec<AuthorityChangeV1>,
    pub grant_shaping_sources: Vec<GrantShapingSourceV1>,
    pub decided_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityMutationResolutionV1 {
    NewlyCommitted,
    ResolvedExistingFact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityProjectionPendingReasonV1 {
    ProjectionNotAttempted,
    ProjectionFailed,
    ProjectionStale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityMutationRejectionReasonV1 {
    InvalidRequest,
    ImportRequired,
    AuthorityEpochUnavailable,
    PriorStateMismatch,
    MutationIdConflict,
    WriterContended,
    WriterPoisoned,
    LegacySnapshotChanged,
    AlreadyImported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AuthorityMutationResultV1 {
    Committed {
        mutation_id: String,
        resolution: AuthorityMutationResolutionV1,
        fact_sequence: u64,
        fact_entry_hash: String,
        grants_authority: GrantsAuthorityIdentityV1,
        projection_hash: String,
    },
    CommittedProjectionPending {
        mutation_id: String,
        resolution: AuthorityMutationResolutionV1,
        fact_sequence: u64,
        fact_entry_hash: String,
        grants_authority: GrantsAuthorityIdentityV1,
        pending_reason: AuthorityProjectionPendingReasonV1,
    },
    CommitUnknown {
        mutation_id: String,
        attempted_intent_hash: String,
        failure_stage: AppendFailureStage,
    },
    RejectedBeforeCommit {
        mutation_id: String,
        reason: AuthorityMutationRejectionReasonV1,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AuthorityStateV1 {
    pub toy_catalog: BTreeMap<String, CanonicalToyContract>,
    pub toy_grants: BTreeMap<String, ToyGrant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityReplayV1 {
    pub state: AuthorityStateV1,
    pub current_authority: Option<GrantsAuthorityIdentityV1>,
    pub imported: bool,
    pub mutations: BTreeMap<String, CommittedAuthorityMutationV1>,
    pub canonical_fact_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityWriterTenureV1 {
    pub fact: EpochEstablishedFactV1,
    pub entry: MctObservationLedgerEntry,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthorityReplayError {
    #[error("canonical authority fact JSON is malformed at sequence {sequence}: {detail}")]
    Malformed { sequence: u64, detail: String },
    #[error("unknown canonical authority fact schema '{schema}' at sequence {sequence}")]
    UnknownSchema { sequence: u64, schema: String },
    #[error("unknown canonical authority fact kind '{fact_kind}' at sequence {sequence}")]
    UnknownFactKind { sequence: u64, fact_kind: String },
    #[error("canonical authority fact is incoherent at sequence {sequence}: {detail}")]
    Incoherent { sequence: u64, detail: String },
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ObservationLedgerError {
    #[error("observation ledger io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("observation ledger json error at {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("observation ledger hash chain is broken at sequence {sequence}")]
    BrokenHashChain { sequence: u64 },
    #[error(
        "observation ledger identity mismatch at sequence {sequence}: expected {expected_ledger_id}/{expected_mother_node_id}, found {actual_ledger_id}/{actual_mother_node_id}"
    )]
    LedgerIdentityMismatch {
        sequence: u64,
        expected_ledger_id: String,
        expected_mother_node_id: String,
        actual_ledger_id: String,
        actual_mother_node_id: String,
    },
    #[error("observation ledger sequence mismatch: expected {expected}, found {actual}")]
    SequenceMismatch { expected: u64, actual: u64 },
    #[error(
        "observation ledger has an unterminated final frame at byte {offset} ({length} bytes) in {path}"
    )]
    UnterminatedTail {
        path: PathBuf,
        offset: u64,
        length: u64,
        digest: String,
    },
    #[error("observation ledger is quarantined: {status:?}")]
    Quarantined { status: Box<LedgerQuarantineStatus> },
    #[error("observation ledger has foreign lineage: {status:?}")]
    ForeignLineage { status: Box<LedgerQuarantineStatus> },
    #[error(
        "observation ledger writer lock contention at {path}: already locked by another writer"
    )]
    WriterContended { path: PathBuf },
    #[error("observation ledger writer lock error at {path}: {source}")]
    WriterLock {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("observation ledger append commitment is unknown at {path} during {stage:?}: {source}")]
    AppendCommitUnknown {
        path: PathBuf,
        stage: AppendFailureStage,
        #[source]
        source: std::io::Error,
    },
    #[error("observation ledger writer is poisoned at {path}; close and reopen before appending")]
    WriterPoisoned { path: PathBuf },
    #[error("canonical authority replay failed: {detail}")]
    AuthorityReplay { detail: String },
    #[error(
        "observation ledger batch stopped after its acknowledged committed prefix: {outcome:?}"
    )]
    BatchPartiallyCommitted {
        outcome: Box<BatchPartialCommitOutcome>,
    },
    #[error(
        "observation ledger changed behind writer at {path}: expected sequence {expected_sequence}, found {actual_sequence}"
    )]
    LedgerChanged {
        path: PathBuf,
        expected_sequence: u64,
        actual_sequence: u64,
    },
}

pub type Result<T> = std::result::Result<T, ObservationLedgerError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppendFailureStage {
    Write,
    Durability,
    Acknowledgement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchPartialCommitOutcome {
    pub acknowledged_committed_prefix: Vec<MctObservationLedgerEntry>,
    pub failed_index: usize,
    pub commit_unknown: bool,
    pub failure: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerFailureClass {
    TerminatedMalformedFrame,
    EntryHashMismatch,
    PreviousHashMismatch,
    SequenceDiscontinuity,
    ForeignLineage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerQuarantineStatus {
    pub ledger_path: PathBuf,
    pub failure_class: LedgerFailureClass,
    pub first_bad_sequence: Option<u64>,
    pub first_bad_offset: u64,
    pub expected: Option<String>,
    pub observed: Option<String>,
    pub preserved_ledger_path: Option<PathBuf>,
    pub diagnostic_record_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerRecoveryStatus {
    pub ledger_path: PathBuf,
    pub source_ledger_id: String,
    pub source_mother_node_id: String,
    pub residue_offset: u64,
    pub residue_length: u64,
    pub residue_digest: String,
    pub last_committed_sequence: Option<u64>,
    pub last_committed_hash: Option<String>,
    pub failure_class: String,
    pub recovery_decision_id: String,
    pub recovery_time: String,
    pub preserved_bytes_path: PathBuf,
    pub diagnostic_record_path: PathBuf,
    pub recovery_complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecoveryStage {
    ForensicDirectoryReady,
    SourceBytesPreserved,
    DiagnosticRecordPreserved,
    LedgerTruncated,
    RecoveryObservationAppended,
    DiagnosticRecordCompleted,
}

#[cfg(test)]
impl RecoveryStage {
    const ALL: [Self; 6] = [
        Self::ForensicDirectoryReady,
        Self::SourceBytesPreserved,
        Self::DiagnosticRecordPreserved,
        Self::LedgerTruncated,
        Self::RecoveryObservationAppended,
        Self::DiagnosticRecordCompleted,
    ];
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctObservationLedgerEntry {
    pub ledger_id: String,
    pub mother_node_id: String,
    pub local_sequence: u64,
    pub observation: MctObservation,
    pub previous_entry_hash: Option<String>,
    pub entry_hash: String,
    pub appended_at: String,
    pub durability_class: DurabilityClass,
    pub export_status: ExportStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurabilityClass {
    BeforeEffect,
    Buffered,
    ProjectionOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportStatus {
    NotRequired,
    Pending,
    Exported,
    Failed,
}

#[derive(Debug)]
pub struct JsonlObservationLedger {
    path: PathBuf,
    file: File,
    ledger_id: String,
    mother_node_id: String,
    next_sequence: u64,
    previous_hash: Option<String>,
    recovery_status: Option<LedgerRecoveryStatus>,
    authority_tenure: Option<AuthorityWriterTenureV1>,
    writer_state: LedgerWriterState,
    #[cfg(test)]
    append_fault: Option<ScheduledAppendFault>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LedgerWriterState {
    Ready,
    Poisoned,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestAppendFault {
    PartialFrame,
    CompleteFrameBeforeDurabilityAck,
    CompleteFrameAfterDurabilityAck,
    TerminatedCorruption,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScheduledAppendFault {
    successful_appends_before_fault: usize,
    fault: TestAppendFault,
}

#[derive(Debug)]
pub struct JsonlObservationLedgerReader {
    path: PathBuf,
    ledger_id: String,
    mother_node_id: String,
}

impl JsonlObservationLedgerReader {
    pub fn open(
        path: impl AsRef<Path>,
        ledger_id: impl Into<String>,
        mother_node_id: impl Into<String>,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let ledger_id = ledger_id.into();
        let mother_node_id = mother_node_id.into();
        match scan_existing(&path, &ledger_id, &mother_node_id)? {
            LedgerScan::Ready(_) => Ok(Self {
                path,
                ledger_id,
                mother_node_id,
            }),
            LedgerScan::Residue(residue) => Err(ObservationLedgerError::UnterminatedTail {
                path,
                offset: residue.offset,
                length: residue.bytes.len() as u64,
                digest: residue.digest,
            }),
            LedgerScan::Quarantine(status) => Err(ObservationLedgerError::Quarantined {
                status: Box::new(status),
            }),
            LedgerScan::ForeignLineage(status) => Err(ObservationLedgerError::ForeignLineage {
                status: Box::new(status),
            }),
        }
    }

    pub fn iter_entries(&self) -> impl Iterator<Item = Result<MctObservationLedgerEntry>> {
        LedgerEntryIter::open(
            self.path.clone(),
            self.ledger_id.clone(),
            self.mother_node_id.clone(),
        )
    }

    pub fn entries(&self) -> Result<Vec<MctObservationLedgerEntry>> {
        self.iter_entries().collect()
    }

    pub fn by_trace(&self, trace_id: &TraceId) -> Result<Vec<MctObservationLedgerEntry>> {
        entries_by_trace(self.iter_entries(), trace_id)
    }

    pub fn by_call(&self, call_id: &CallId) -> Result<Vec<MctObservationLedgerEntry>> {
        entries_by_call(self.iter_entries(), call_id)
    }
}

impl JsonlObservationLedger {
    pub fn open(
        path: impl AsRef<Path>,
        ledger_id: impl Into<String>,
        mother_node_id: impl Into<String>,
    ) -> Result<Self> {
        open_with_recovery_hook(
            path.as_ref(),
            &ledger_id.into(),
            &mother_node_id.into(),
            |_| Ok(()),
        )
    }

    pub fn open_authority(
        path: impl AsRef<Path>,
        ledger_id: impl Into<String>,
        mother_node_id: impl Into<String>,
    ) -> Result<Self> {
        let mut ledger = Self::open(path, ledger_id, mother_node_id)?;
        ledger.begin_authority_tenure()?;
        Ok(ledger)
    }

    pub fn open_read_only(
        path: impl AsRef<Path>,
        ledger_id: impl Into<String>,
        mother_node_id: impl Into<String>,
    ) -> Result<JsonlObservationLedgerReader> {
        JsonlObservationLedgerReader::open(path, ledger_id, mother_node_id)
    }

    pub fn recovery_status(&self) -> Option<&LedgerRecoveryStatus> {
        self.recovery_status.as_ref()
    }

    pub fn is_poisoned(&self) -> bool {
        self.writer_state == LedgerWriterState::Poisoned
    }

    pub fn authority_tenure(&self) -> Option<&AuthorityWriterTenureV1> {
        self.authority_tenure.as_ref()
    }

    pub fn begin_authority_tenure(&mut self) -> Result<()> {
        if self.authority_tenure.is_some() {
            return Ok(());
        }
        let entries = self.entries()?;
        let replay = replay_authority_entries(&entries).map_err(|error| {
            ObservationLedgerError::AuthorityReplay {
                detail: error.to_string(),
            }
        })?;
        let predecessor =
            entries
                .last()
                .map_or(AuthorityEpochPredecessorV1::NoneForVirgin, |entry| {
                    AuthorityEpochPredecessorV1::ValidatedHead {
                        sequence: entry.local_sequence,
                        entry_hash: entry.entry_hash.clone(),
                    }
                });
        let startup_class = if entries.is_empty() {
            AuthorityStartupClassV1::Virgin
        } else if replay.current_authority.is_some() {
            AuthorityStartupClassV1::OrdinaryReopen
        } else {
            AuthorityStartupClassV1::OperatorGatedNonvirgin
        };
        let generation_baseline = replay
            .current_authority
            .as_ref()
            .map_or(0, |identity| identity.generation);
        let authority_epoch = fresh_authority_epoch()?;
        let fact_id = format!("obs:authority-epoch:{authority_epoch}");
        let resulting_authority = GrantsAuthorityIdentityV1 {
            mother_node_id: self.mother_node_id.clone(),
            authority_epoch: authority_epoch.clone(),
            generation: generation_baseline,
            source_authority_observation_id: fact_id.clone(),
        };
        let started_at = jiff::Timestamp::now().to_string();
        let fact = EpochEstablishedFactV1 {
            mother_node_id: self.mother_node_id.clone(),
            ledger_id: self.ledger_id.clone(),
            authority_epoch,
            predecessor,
            generation_baseline,
            prior_authority: replay.current_authority,
            resulting_authority,
            grant_state_hash: authority_state_hash(&replay.state)?,
            establishment: WriterTenureEstablishmentV1 {
                started_at: started_at.clone(),
                startup_class,
                operator_gate_decision_id: None,
                authenticated_principal_ref: None,
            },
        };
        let detail_ref = encode_epoch_fact(&fact_id, &fact)?;
        let observation = MctObservation {
            observation_id: ObservationId::new(fact_id).expect("epoch fact id is non-empty"),
            observed_at: Timestamp::new(started_at.clone()).expect("system time is RFC3339"),
            kind: ObservationKind::LifecycleTransitionRecorded,
            source_plane: SourcePlane::Storage,
            trace: ObservationTraceRef {
                trace_id: TraceId::new(format!("trace:authority-epoch:{}", fact.authority_epoch))
                    .expect("epoch trace id is non-empty"),
                span_id: None,
                parent_span_id: None,
                external_trace_id: None,
            },
            call_id: None,
            decision_id: None,
            subject_id: Some(self.mother_node_id.clone()),
            resource_id: Some(self.ledger_id.clone()),
            policy_revision: None,
            grants_revision: Some(generation_baseline),
            outcome: ObservationOutcome::Completed,
            visibility: ObservationVisibility::NodeOperator,
            safe_message: "authority writer tenure established".into(),
            detail_ref: Some(detail_ref),
        };
        let entry = self.append_before_effect(observation, started_at)?;
        self.authority_tenure = Some(AuthorityWriterTenureV1 {
            fact: fact.clone(),
            entry,
        });
        Ok(())
    }

    pub fn execute_authority_mutation<F>(
        &mut self,
        request: AuthorityMutationRequestV1,
        legacy_projection_write: F,
    ) -> AuthorityMutationResultV1
    where
        F: FnOnce(&AuthorityStateV1) -> std::result::Result<Option<String>, String>,
    {
        let mutation_id = request.mutation_id.clone();
        if self.authority_tenure.is_none() {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::AuthorityEpochUnavailable,
            );
        }
        if self.is_poisoned() {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::WriterPoisoned,
            );
        }
        let intent_hash = match authority_mutation_intent_hash(&request) {
            Ok(hash) => hash,
            Err(_) => {
                return rejected_mutation(
                    mutation_id,
                    AuthorityMutationRejectionReasonV1::InvalidRequest,
                );
            }
        };
        let entries = match self.entries() {
            Ok(entries) => entries,
            Err(_) => {
                return rejected_mutation(
                    mutation_id,
                    AuthorityMutationRejectionReasonV1::PriorStateMismatch,
                );
            }
        };
        let replay = match replay_authority_entries(&entries) {
            Ok(replay) => replay,
            Err(_) => {
                return rejected_mutation(
                    mutation_id,
                    AuthorityMutationRejectionReasonV1::PriorStateMismatch,
                );
            }
        };
        if let Some(existing) = replay.mutations.get(&request.mutation_id) {
            if existing.fact.mutation_intent_hash != intent_hash {
                return rejected_mutation(
                    mutation_id,
                    AuthorityMutationRejectionReasonV1::MutationIdConflict,
                );
            }
            return finish_projected_mutation(
                &existing.fact,
                existing.entry_sequence,
                existing.entry_hash.clone(),
                AuthorityMutationResolutionV1::ResolvedExistingFact,
                legacy_projection_write(&replay.state),
            );
        }
        let Some(prior_authority) = replay.current_authority.clone() else {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::AuthorityEpochUnavailable,
            );
        };
        let mut resulting_state = replay.state.clone();
        if apply_authority_changes(&mut resulting_state, &request.changes).is_err() {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::InvalidRequest,
            );
        }
        let prior_hash = match authority_state_hash(&replay.state) {
            Ok(hash) => hash,
            Err(_) => {
                return rejected_mutation(
                    mutation_id,
                    AuthorityMutationRejectionReasonV1::PriorStateMismatch,
                );
            }
        };
        let resulting_hash = match authority_state_hash(&resulting_state) {
            Ok(hash) => hash,
            Err(_) => {
                return rejected_mutation(
                    mutation_id,
                    AuthorityMutationRejectionReasonV1::InvalidRequest,
                );
            }
        };
        let Some(generation) = prior_authority.generation.checked_add(1) else {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::InvalidRequest,
            );
        };
        let fact_id = format!("obs:authority-mutation:{}", request.mutation_id);
        let resulting_authority = GrantsAuthorityIdentityV1 {
            mother_node_id: self.mother_node_id.clone(),
            authority_epoch: prior_authority.authority_epoch.clone(),
            generation,
            source_authority_observation_id: fact_id.clone(),
        };
        let fact = AuthorityMutationFactV1 {
            mutation_id: request.mutation_id,
            mutation_intent_hash: intent_hash.clone(),
            mother_node_id: self.mother_node_id.clone(),
            ledger_id: self.ledger_id.clone(),
            authority_epoch: prior_authority.authority_epoch.clone(),
            prior_state: AuthorityStateReferenceV1 {
                grants_authority: prior_authority,
                authority_state_hash: prior_hash,
            },
            changes: request.changes,
            grant_shaping_sources: request.grant_shaping_sources,
            resulting_state: AuthorityStateReferenceV1 {
                grants_authority: resulting_authority.clone(),
                authority_state_hash: resulting_hash,
            },
            decided_at: request.decided_at,
        };
        let detail_ref = match encode_authority_fact("authority_mutation", &fact_id, &fact) {
            Ok(detail) => detail,
            Err(_) => {
                return rejected_mutation(
                    mutation_id,
                    AuthorityMutationRejectionReasonV1::InvalidRequest,
                );
            }
        };
        let observation = authority_mutation_observation(&fact_id, &fact, detail_ref);
        let entry = match self.append_before_effect(observation, fact.decided_at.clone()) {
            Ok(entry) => entry,
            Err(ObservationLedgerError::AppendCommitUnknown { stage, .. }) => {
                return AuthorityMutationResultV1::CommitUnknown {
                    mutation_id,
                    attempted_intent_hash: intent_hash,
                    failure_stage: stage,
                };
            }
            Err(ObservationLedgerError::WriterPoisoned { .. }) => {
                return rejected_mutation(
                    mutation_id,
                    AuthorityMutationRejectionReasonV1::WriterPoisoned,
                );
            }
            Err(_) => {
                return rejected_mutation(
                    mutation_id,
                    AuthorityMutationRejectionReasonV1::InvalidRequest,
                );
            }
        };
        finish_projected_mutation(
            &fact,
            entry.local_sequence,
            entry.entry_hash,
            AuthorityMutationResolutionV1::NewlyCommitted,
            legacy_projection_write(&resulting_state),
        )
    }

    #[cfg(test)]
    fn inject_append_fault_after_for_test(
        &mut self,
        successful_appends_before_fault: usize,
        fault: TestAppendFault,
    ) {
        self.append_fault = Some(ScheduledAppendFault {
            successful_appends_before_fault,
            fault,
        });
    }

    pub fn append_before_effect(
        &mut self,
        observation: MctObservation,
        appended_at: impl Into<String>,
    ) -> Result<MctObservationLedgerEntry> {
        self.append(
            observation,
            appended_at,
            DurabilityClass::BeforeEffect,
            ExportStatus::NotRequired,
        )
    }

    pub fn append_batch_before_effect(
        &mut self,
        observations: impl IntoIterator<Item = MctObservation>,
        appended_at: impl Into<String>,
    ) -> Result<Vec<MctObservationLedgerEntry>> {
        let appended_at = appended_at.into();
        let mut acknowledged_committed_prefix = Vec::new();
        for (failed_index, observation) in observations.into_iter().enumerate() {
            match self.append_before_effect(observation, appended_at.clone()) {
                Ok(entry) => acknowledged_committed_prefix.push(entry),
                Err(error) => {
                    let commit_unknown =
                        matches!(error, ObservationLedgerError::AppendCommitUnknown { .. });
                    return Err(ObservationLedgerError::BatchPartiallyCommitted {
                        outcome: Box::new(BatchPartialCommitOutcome {
                            acknowledged_committed_prefix,
                            failed_index,
                            commit_unknown,
                            failure: error.to_string(),
                        }),
                    });
                }
            }
        }
        Ok(acknowledged_committed_prefix)
    }

    pub fn append(
        &mut self,
        observation: MctObservation,
        appended_at: impl Into<String>,
        durability_class: DurabilityClass,
        export_status: ExportStatus,
    ) -> Result<MctObservationLedgerEntry> {
        if self.is_poisoned() {
            return Err(ObservationLedgerError::WriterPoisoned {
                path: self.path.clone(),
            });
        }
        let mut entry = MctObservationLedgerEntry {
            ledger_id: self.ledger_id.clone(),
            mother_node_id: self.mother_node_id.clone(),
            local_sequence: self.next_sequence,
            observation,
            previous_entry_hash: self.previous_hash.clone(),
            entry_hash: String::new(),
            appended_at: appended_at.into(),
            durability_class,
            export_status,
        };
        entry.entry_hash = entry_hash(&entry)?;

        let mut frame =
            serde_json::to_vec(&entry).map_err(|source| ObservationLedgerError::Json {
                path: self.path.clone(),
                source,
            })?;
        frame.push(b'\n');
        self.append_frame(&frame)?;

        self.previous_hash = Some(entry.entry_hash.clone());
        self.next_sequence += 1;
        Ok(entry)
    }

    fn append_frame(&mut self, frame: &[u8]) -> Result<()> {
        #[cfg(test)]
        if let Some(fault) = self.scheduled_fault_for_next_append() {
            return self.execute_test_append_fault(frame, fault);
        }

        if let Err(source) = self.file.write_all(frame) {
            return Err(self.poisoned_append_error(AppendFailureStage::Write, source));
        }
        if let Err(source) = self.file.sync_data() {
            return Err(self.poisoned_append_error(AppendFailureStage::Durability, source));
        }
        Ok(())
    }

    fn poisoned_append_error(
        &mut self,
        stage: AppendFailureStage,
        source: std::io::Error,
    ) -> ObservationLedgerError {
        self.writer_state = LedgerWriterState::Poisoned;
        ObservationLedgerError::AppendCommitUnknown {
            path: self.path.clone(),
            stage,
            source,
        }
    }

    #[cfg(test)]
    fn scheduled_fault_for_next_append(&mut self) -> Option<TestAppendFault> {
        let scheduled = self.append_fault.as_mut()?;
        if scheduled.successful_appends_before_fault > 0 {
            scheduled.successful_appends_before_fault -= 1;
            return None;
        }
        self.append_fault.take().map(|scheduled| scheduled.fault)
    }

    #[cfg(test)]
    fn execute_test_append_fault(&mut self, frame: &[u8], fault: TestAppendFault) -> Result<()> {
        let injected = |message: &'static str| std::io::Error::other(message);
        match fault {
            TestAppendFault::PartialFrame => {
                let partial_len = (frame.len() / 2).max(1);
                if let Err(source) = self.file.write_all(&frame[..partial_len]) {
                    return Err(self.poisoned_append_error(AppendFailureStage::Write, source));
                }
                let _ = self.file.sync_data();
                Err(self.poisoned_append_error(
                    AppendFailureStage::Write,
                    injected("injected partial frame write failure"),
                ))
            }
            TestAppendFault::CompleteFrameBeforeDurabilityAck => {
                if let Err(source) = self.file.write_all(frame) {
                    return Err(self.poisoned_append_error(AppendFailureStage::Write, source));
                }
                Err(self.poisoned_append_error(
                    AppendFailureStage::Durability,
                    injected("injected durability acknowledgement failure"),
                ))
            }
            TestAppendFault::CompleteFrameAfterDurabilityAck => {
                if let Err(source) = self.file.write_all(frame) {
                    return Err(self.poisoned_append_error(AppendFailureStage::Write, source));
                }
                if let Err(source) = self.file.sync_data() {
                    return Err(self.poisoned_append_error(AppendFailureStage::Durability, source));
                }
                Err(self.poisoned_append_error(
                    AppendFailureStage::Acknowledgement,
                    injected("injected lost append acknowledgement"),
                ))
            }
            TestAppendFault::TerminatedCorruption => {
                if let Err(source) = self.file.write_all(b"injected-corrupt-frame\n") {
                    return Err(self.poisoned_append_error(AppendFailureStage::Write, source));
                }
                let _ = self.file.sync_data();
                Err(self.poisoned_append_error(
                    AppendFailureStage::Acknowledgement,
                    injected("injected terminated corruption"),
                ))
            }
        }
    }

    pub fn iter_entries(&self) -> impl Iterator<Item = Result<MctObservationLedgerEntry>> {
        LedgerEntryIter::open(
            self.path.clone(),
            self.ledger_id.clone(),
            self.mother_node_id.clone(),
        )
    }

    pub fn entries(&self) -> Result<Vec<MctObservationLedgerEntry>> {
        self.iter_entries().collect()
    }

    pub fn by_trace(&self, trace_id: &TraceId) -> Result<Vec<MctObservationLedgerEntry>> {
        entries_by_trace(self.iter_entries(), trace_id)
    }

    pub fn by_call(&self, call_id: &CallId) -> Result<Vec<MctObservationLedgerEntry>> {
        entries_by_call(self.iter_entries(), call_id)
    }
}

pub fn read_ledger_entries(
    path: impl AsRef<Path>,
    ledger_id: impl Into<String>,
    mother_node_id: impl Into<String>,
) -> Result<Vec<MctObservationLedgerEntry>> {
    JsonlObservationLedgerReader::open(path, ledger_id, mother_node_id)?.entries()
}

fn entries_by_trace(
    iter: impl Iterator<Item = Result<MctObservationLedgerEntry>>,
    trace_id: &TraceId,
) -> Result<Vec<MctObservationLedgerEntry>> {
    let mut entries = Vec::new();
    for entry in iter {
        let entry = entry?;
        if &entry.observation.trace.trace_id == trace_id {
            entries.push(entry);
        }
    }
    Ok(entries)
}

fn entries_by_call(
    iter: impl Iterator<Item = Result<MctObservationLedgerEntry>>,
    call_id: &CallId,
) -> Result<Vec<MctObservationLedgerEntry>> {
    let mut entries = Vec::new();
    for entry in iter {
        let entry = entry?;
        if entry.observation.call_id.as_ref() == Some(call_id) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

#[derive(Debug)]
struct LedgerScanState {
    next_sequence: u64,
    previous_hash: Option<String>,
    committed_len: u64,
    entries: Vec<MctObservationLedgerEntry>,
}

#[derive(Debug)]
struct LedgerTailResidue {
    offset: u64,
    bytes: Vec<u8>,
    digest: String,
    state: LedgerScanState,
}

#[derive(Debug)]
enum LedgerScan {
    Ready(LedgerScanState),
    Residue(LedgerTailResidue),
    Quarantine(LedgerQuarantineStatus),
    ForeignLineage(LedgerQuarantineStatus),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CanonicalAuthorityEnvelopeV1 {
    schema: String,
    fact_kind: String,
    fact_id: String,
    body: serde_json::Value,
}

fn fresh_authority_epoch() -> Result<String> {
    let mut entropy = [0_u8; 32];
    getrandom::fill(&mut entropy).map_err(|error| ObservationLedgerError::Io {
        path: PathBuf::from("<authority-epoch-entropy>"),
        source: std::io::Error::other(error.to_string()),
    })?;
    let mut encoded = String::with_capacity(AUTHORITY_EPOCH_PREFIX_V1.len() + 64);
    encoded.push_str(AUTHORITY_EPOCH_PREFIX_V1);
    for byte in entropy {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(encoded)
}

fn canonical_json_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    fn canonicalize(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(object) => {
                let sorted = object
                    .into_iter()
                    .map(|(key, value)| (key, canonicalize(value)))
                    .collect();
                serde_json::Value::Object(sorted)
            }
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(canonicalize).collect())
            }
            other => other,
        }
    }
    let value = serde_json::to_value(value).map_err(|source| ObservationLedgerError::Json {
        path: PathBuf::from("<canonical-authority-fact>"),
        source,
    })?;
    serde_json::to_vec(&canonicalize(value)).map_err(|source| ObservationLedgerError::Json {
        path: PathBuf::from("<canonical-authority-fact>"),
        source,
    })
}

fn encode_authority_fact(fact_kind: &str, fact_id: &str, fact: &impl Serialize) -> Result<String> {
    let envelope = serde_json::json!({
        "schema": AUTHORITY_FACT_SCHEMA_V1,
        "fact_kind": fact_kind,
        "fact_id": fact_id,
        "body": fact,
    });
    let bytes = canonical_json_bytes(&envelope)?;
    Ok(format!(
        "{AUTHORITY_FACT_DETAIL_PREFIX}{}",
        String::from_utf8(bytes).expect("JSON is UTF-8")
    ))
}

fn encode_epoch_fact(fact_id: &str, fact: &EpochEstablishedFactV1) -> Result<String> {
    encode_authority_fact("epoch_established", fact_id, fact)
}

fn authority_mutation_intent_hash(request: &AuthorityMutationRequestV1) -> Result<String> {
    if request.mutation_id.trim().is_empty()
        || request.changes.is_empty()
        || request.grant_shaping_sources.is_empty()
        || Timestamp::new(request.decided_at.clone()).is_err()
    {
        return Err(ObservationLedgerError::AuthorityReplay {
            detail: "authority mutation request is incomplete".into(),
        });
    }
    let keys = request
        .changes
        .iter()
        .map(authority_change_sort_key)
        .collect::<Vec<_>>();
    if keys.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(ObservationLedgerError::AuthorityReplay {
            detail: "authority changes are not strictly canonically ordered".into(),
        });
    }
    #[derive(Serialize)]
    struct Intent<'a> {
        changes: &'a [AuthorityChangeV1],
        grant_shaping_sources: &'a [GrantShapingSourceV1],
    }
    Ok(blake3::hash(&canonical_json_bytes(&Intent {
        changes: &request.changes,
        grant_shaping_sources: &request.grant_shaping_sources,
    })?)
    .to_hex()
    .to_string())
}

fn authority_change_sort_key(change: &AuthorityChangeV1) -> (&'static str, &str) {
    match change {
        AuthorityChangeV1::ToyCatalogPut { toy_id, .. } => ("toy_catalog_put", toy_id),
        AuthorityChangeV1::ToyCatalogRemove { toy_id } => ("toy_catalog_remove", toy_id),
        AuthorityChangeV1::ToyGrantPut { grant_id, .. } => ("toy_grant_put", grant_id),
        AuthorityChangeV1::ToyGrantRemove { grant_id } => ("toy_grant_remove", grant_id),
    }
}

pub fn apply_authority_changes(
    state: &mut AuthorityStateV1,
    changes: &[AuthorityChangeV1],
) -> std::result::Result<(), AuthorityReplayError> {
    for change in changes {
        match change {
            AuthorityChangeV1::ToyCatalogPut {
                toy_id,
                contract,
                authority_bearing,
                catalog_revision,
                admitted_by_observation_id,
            } => {
                let toy_id_value = ToyId::new(toy_id.clone()).map_err(|error| {
                    AuthorityReplayError::Incoherent {
                        sequence: 0,
                        detail: error.to_string(),
                    }
                })?;
                let observation_id = ObservationId::new(admitted_by_observation_id.clone())
                    .map_err(|error| AuthorityReplayError::Incoherent {
                        sequence: 0,
                        detail: error.to_string(),
                    })?;
                state.toy_catalog.insert(
                    toy_id.clone(),
                    CanonicalToyContract {
                        toy_id: toy_id_value,
                        contract: contract.clone(),
                        authority_bearing: *authority_bearing,
                        catalog_revision: *catalog_revision,
                        admitted_by_observation_id: observation_id,
                    },
                );
            }
            AuthorityChangeV1::ToyCatalogRemove { toy_id } => {
                if state.toy_catalog.remove(toy_id).is_none() {
                    return Err(AuthorityReplayError::Incoherent {
                        sequence: 0,
                        detail: format!("cannot remove missing Toy catalog value '{toy_id}'"),
                    });
                }
            }
            AuthorityChangeV1::ToyGrantPut {
                grant_id,
                toy_id,
                subject,
                scope,
                constraints,
                grant_state,
                issuer_id,
                policy_revision,
                source_grants_revision,
                authority_observation_id,
            } => {
                if !state.toy_catalog.contains_key(toy_id)
                    || scope.allowed_actions.is_empty()
                    || scope
                        .allowed_actions
                        .windows(2)
                        .any(|pair| pair[0] >= pair[1])
                {
                    return Err(AuthorityReplayError::Incoherent {
                        sequence: 0,
                        detail: "Toy grant references missing catalog or noncanonical actions"
                            .into(),
                    });
                }
                let grant = ToyGrant {
                    grant_id: ToyGrantId::new(grant_id.clone()).map_err(|error| {
                        AuthorityReplayError::Incoherent {
                            sequence: 0,
                            detail: error.to_string(),
                        }
                    })?,
                    toy_id: ToyId::new(toy_id.clone()).map_err(|error| {
                        AuthorityReplayError::Incoherent {
                            sequence: 0,
                            detail: error.to_string(),
                        }
                    })?,
                    subject: subject.as_ref().clone(),
                    scope: scope.as_ref().clone(),
                    constraints: constraints.as_ref().clone(),
                    grant_state: *grant_state,
                    issuer_id: issuer_id.clone(),
                    policy_revision: *policy_revision,
                    grants_revision: *source_grants_revision,
                    authority_observation_id: ObservationId::new(authority_observation_id.clone())
                        .map_err(|error| AuthorityReplayError::Incoherent {
                            sequence: 0,
                            detail: error.to_string(),
                        })?,
                };
                state.toy_grants.insert(grant_id.clone(), grant);
            }
            AuthorityChangeV1::ToyGrantRemove { grant_id } => {
                if state.toy_grants.remove(grant_id).is_none() {
                    return Err(AuthorityReplayError::Incoherent {
                        sequence: 0,
                        detail: format!("cannot remove missing Toy grant '{grant_id}'"),
                    });
                }
            }
        }
    }
    Ok(())
}

fn authority_mutation_observation(
    fact_id: &str,
    fact: &AuthorityMutationFactV1,
    detail_ref: String,
) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::new(fact_id).expect("mutation fact id is non-empty"),
        observed_at: Timestamp::new(fact.decided_at.clone()).expect("validated mutation time"),
        kind: ObservationKind::OperatorActionRecorded,
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace:authority-mutation:{}", fact.mutation_id))
                .expect("mutation trace is non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(fact.mother_node_id.clone()),
        resource_id: Some(fact.ledger_id.clone()),
        policy_revision: None,
        grants_revision: Some(fact.resulting_state.grants_authority.generation),
        outcome: ObservationOutcome::Allowed,
        visibility: ObservationVisibility::NodeOperator,
        safe_message: "canonical authority mutation committed".into(),
        detail_ref: Some(detail_ref),
    }
}

fn rejected_mutation(
    mutation_id: String,
    reason: AuthorityMutationRejectionReasonV1,
) -> AuthorityMutationResultV1 {
    AuthorityMutationResultV1::RejectedBeforeCommit {
        mutation_id,
        reason,
    }
}

fn finish_projected_mutation(
    fact: &AuthorityMutationFactV1,
    fact_sequence: u64,
    fact_entry_hash: String,
    resolution: AuthorityMutationResolutionV1,
    projection: std::result::Result<Option<String>, String>,
) -> AuthorityMutationResultV1 {
    match projection {
        Ok(Some(projection_hash)) => AuthorityMutationResultV1::Committed {
            mutation_id: fact.mutation_id.clone(),
            resolution,
            fact_sequence,
            fact_entry_hash,
            grants_authority: fact.resulting_state.grants_authority.clone(),
            projection_hash,
        },
        Ok(None) => AuthorityMutationResultV1::CommittedProjectionPending {
            mutation_id: fact.mutation_id.clone(),
            resolution,
            fact_sequence,
            fact_entry_hash,
            grants_authority: fact.resulting_state.grants_authority.clone(),
            pending_reason: AuthorityProjectionPendingReasonV1::ProjectionNotAttempted,
        },
        Err(_) => AuthorityMutationResultV1::CommittedProjectionPending {
            mutation_id: fact.mutation_id.clone(),
            resolution,
            fact_sequence,
            fact_entry_hash,
            grants_authority: fact.resulting_state.grants_authority.clone(),
            pending_reason: AuthorityProjectionPendingReasonV1::ProjectionFailed,
        },
    }
}

pub fn authority_state_hash(state: &AuthorityStateV1) -> Result<String> {
    #[derive(Serialize)]
    struct HashableAuthorityState<'a> {
        toy_catalog: Vec<&'a CanonicalToyContract>,
        toy_grants: Vec<&'a ToyGrant>,
    }
    let hashable = HashableAuthorityState {
        toy_catalog: state.toy_catalog.values().collect(),
        toy_grants: state.toy_grants.values().collect(),
    };
    Ok(blake3::hash(&canonical_json_bytes(&hashable)?)
        .to_hex()
        .to_string())
}

pub fn replay_authority_entries(
    entries: &[MctObservationLedgerEntry],
) -> std::result::Result<AuthorityReplayV1, AuthorityReplayError> {
    let mut replay = AuthorityReplayV1 {
        state: AuthorityStateV1::default(),
        current_authority: None,
        imported: false,
        mutations: BTreeMap::new(),
        canonical_fact_count: 0,
    };
    let mut used_epochs = BTreeSet::new();

    for (index, entry) in entries.iter().enumerate() {
        let Some(detail) = entry.observation.detail_ref.as_deref() else {
            continue;
        };
        let Some(payload) = detail.strip_prefix(AUTHORITY_FACT_DETAIL_PREFIX) else {
            continue;
        };
        let envelope: CanonicalAuthorityEnvelopeV1 =
            serde_json::from_str(payload).map_err(|error| AuthorityReplayError::Malformed {
                sequence: entry.local_sequence,
                detail: error.to_string(),
            })?;
        if envelope.schema != AUTHORITY_FACT_SCHEMA_V1 {
            return Err(AuthorityReplayError::UnknownSchema {
                sequence: entry.local_sequence,
                schema: envelope.schema,
            });
        }
        if envelope.fact_id != entry.observation.observation_id.as_str() {
            return Err(AuthorityReplayError::Incoherent {
                sequence: entry.local_sequence,
                detail: "fact_id does not match observation_id".into(),
            });
        }
        match envelope.fact_kind.as_str() {
            "epoch_established" => {
                let fact: EpochEstablishedFactV1 =
                    serde_json::from_value(envelope.body).map_err(|error| {
                        AuthorityReplayError::Malformed {
                            sequence: entry.local_sequence,
                            detail: error.to_string(),
                        }
                    })?;
                validate_epoch_fact(entries, index, entry, &replay, &used_epochs, &fact)?;
                used_epochs.insert(fact.authority_epoch.clone());
                replay.current_authority = Some(fact.resulting_authority);
                replay.canonical_fact_count += 1;
            }
            "authority_mutation" => {
                let fact: AuthorityMutationFactV1 =
                    serde_json::from_value(envelope.body).map_err(|error| {
                        AuthorityReplayError::Malformed {
                            sequence: entry.local_sequence,
                            detail: error.to_string(),
                        }
                    })?;
                let resulting_state = validate_authority_mutation_fact(entry, &replay, &fact)?;
                if replay.mutations.contains_key(&fact.mutation_id) {
                    return Err(AuthorityReplayError::Incoherent {
                        sequence: entry.local_sequence,
                        detail: "duplicate canonical authority mutation id".into(),
                    });
                }
                replay.mutations.insert(
                    fact.mutation_id.clone(),
                    CommittedAuthorityMutationV1 {
                        fact: fact.clone(),
                        entry_sequence: entry.local_sequence,
                        entry_hash: entry.entry_hash.clone(),
                    },
                );
                replay.state = resulting_state;
                replay.current_authority = Some(fact.resulting_state.grants_authority);
                replay.canonical_fact_count += 1;
            }
            other => {
                return Err(AuthorityReplayError::UnknownFactKind {
                    sequence: entry.local_sequence,
                    fact_kind: other.to_owned(),
                });
            }
        }
    }
    Ok(replay)
}

fn validate_epoch_fact(
    entries: &[MctObservationLedgerEntry],
    index: usize,
    entry: &MctObservationLedgerEntry,
    replay: &AuthorityReplayV1,
    used_epochs: &BTreeSet<String>,
    fact: &EpochEstablishedFactV1,
) -> std::result::Result<(), AuthorityReplayError> {
    let incoherent = |detail: &str| AuthorityReplayError::Incoherent {
        sequence: entry.local_sequence,
        detail: detail.to_owned(),
    };
    let expected_predecessor =
        index
            .checked_sub(1)
            .map_or(AuthorityEpochPredecessorV1::NoneForVirgin, |previous| {
                AuthorityEpochPredecessorV1::ValidatedHead {
                    sequence: entries[previous].local_sequence,
                    entry_hash: entries[previous].entry_hash.clone(),
                }
            });
    let expected_generation = replay
        .current_authority
        .as_ref()
        .map_or(0, |identity| identity.generation);
    let expected_hash =
        authority_state_hash(&replay.state).map_err(|error| AuthorityReplayError::Incoherent {
            sequence: entry.local_sequence,
            detail: error.to_string(),
        })?;
    if entry.observation.kind != ObservationKind::LifecycleTransitionRecorded
        || entry.observation.source_plane != SourcePlane::Storage
        || entry.durability_class != DurabilityClass::BeforeEffect
        || entry.observation.visibility != ObservationVisibility::NodeOperator
        || entry.observation.subject_id.as_deref() != Some(entry.mother_node_id.as_str())
        || entry.observation.resource_id.as_deref() != Some(entry.ledger_id.as_str())
        || entry.observation.grants_revision != Some(fact.generation_baseline)
    {
        return Err(incoherent(
            "epoch observation carrier fields do not match schema",
        ));
    }
    if fact.mother_node_id != entry.mother_node_id
        || fact.ledger_id != entry.ledger_id
        || fact.predecessor != expected_predecessor
        || fact.generation_baseline != expected_generation
        || fact.prior_authority != replay.current_authority
        || fact.grant_state_hash != expected_hash
        || used_epochs.contains(&fact.authority_epoch)
        || !fact.authority_epoch.starts_with(AUTHORITY_EPOCH_PREFIX_V1)
        || fact.authority_epoch.len() != AUTHORITY_EPOCH_PREFIX_V1.len() + 64
    {
        return Err(incoherent(
            "epoch body does not match replayed predecessor state",
        ));
    }
    let expected_identity = GrantsAuthorityIdentityV1 {
        mother_node_id: entry.mother_node_id.clone(),
        authority_epoch: fact.authority_epoch.clone(),
        generation: fact.generation_baseline,
        source_authority_observation_id: entry.observation.observation_id.to_string(),
    };
    if fact.resulting_authority != expected_identity {
        return Err(incoherent("epoch resulting authority identity is invalid"));
    }
    Ok(())
}

fn validate_authority_mutation_fact(
    entry: &MctObservationLedgerEntry,
    replay: &AuthorityReplayV1,
    fact: &AuthorityMutationFactV1,
) -> std::result::Result<AuthorityStateV1, AuthorityReplayError> {
    let incoherent = |detail: &str| AuthorityReplayError::Incoherent {
        sequence: entry.local_sequence,
        detail: detail.to_owned(),
    };
    let Some(prior_authority) = replay.current_authority.as_ref() else {
        return Err(incoherent(
            "authority mutation precedes epoch establishment",
        ));
    };
    let request = AuthorityMutationRequestV1 {
        mutation_id: fact.mutation_id.clone(),
        changes: fact.changes.clone(),
        grant_shaping_sources: fact.grant_shaping_sources.clone(),
        decided_at: fact.decided_at.clone(),
    };
    let expected_intent =
        authority_mutation_intent_hash(&request).map_err(|error| incoherent(&error.to_string()))?;
    let prior_hash =
        authority_state_hash(&replay.state).map_err(|error| incoherent(&error.to_string()))?;
    let mut resulting_state = replay.state.clone();
    apply_authority_changes(&mut resulting_state, &fact.changes).map_err(|error| {
        AuthorityReplayError::Incoherent {
            sequence: entry.local_sequence,
            detail: error.to_string(),
        }
    })?;
    let resulting_hash =
        authority_state_hash(&resulting_state).map_err(|error| incoherent(&error.to_string()))?;
    let expected_generation = prior_authority
        .generation
        .checked_add(1)
        .ok_or_else(|| incoherent("authority generation overflow"))?;
    let expected_identity = GrantsAuthorityIdentityV1 {
        mother_node_id: entry.mother_node_id.clone(),
        authority_epoch: prior_authority.authority_epoch.clone(),
        generation: expected_generation,
        source_authority_observation_id: entry.observation.observation_id.to_string(),
    };
    if entry.observation.kind != ObservationKind::OperatorActionRecorded
        || entry.observation.source_plane != SourcePlane::Kernel
        || entry.durability_class != DurabilityClass::BeforeEffect
        || entry.observation.visibility != ObservationVisibility::NodeOperator
        || entry.observation.subject_id.as_deref() != Some(entry.mother_node_id.as_str())
        || entry.observation.resource_id.as_deref() != Some(entry.ledger_id.as_str())
        || entry.observation.grants_revision != Some(expected_generation)
        || fact.mother_node_id != entry.mother_node_id
        || fact.ledger_id != entry.ledger_id
        || fact.authority_epoch != prior_authority.authority_epoch
        || fact.mutation_intent_hash != expected_intent
        || fact.prior_state.grants_authority != *prior_authority
        || fact.prior_state.authority_state_hash != prior_hash
        || fact.resulting_state.grants_authority != expected_identity
        || fact.resulting_state.authority_state_hash != resulting_hash
    {
        return Err(incoherent(
            "authority mutation does not match replayed prior/resulting state",
        ));
    }
    Ok(resulting_state)
}

pub fn forensic_root_path(ledger_path: &Path) -> PathBuf {
    let file_name = ledger_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("observation-ledger");
    ledger_path.with_file_name(format!("{file_name}.forensics"))
}

fn scan_existing(path: &Path, ledger_id: &str, mother_node_id: &str) -> Result<LedgerScan> {
    let bytes = std::fs::read(path).map_err(|source| ObservationLedgerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut state = LedgerScanState {
        next_sequence: 0,
        previous_hash: None,
        committed_len: 0,
        entries: Vec::new(),
    };
    let mut frame_start = 0usize;

    for terminator in bytes
        .iter()
        .enumerate()
        .filter_map(|(index, byte)| (*byte == b'\n').then_some(index))
    {
        let frame = &bytes[frame_start..terminator];
        let offset = frame_start as u64;
        let entry: MctObservationLedgerEntry = match serde_json::from_slice(frame) {
            Ok(entry) => entry,
            Err(source) => {
                return Ok(LedgerScan::Quarantine(quarantine_status(
                    path,
                    LedgerFailureClass::TerminatedMalformedFrame,
                    Some(state.next_sequence),
                    offset,
                    Some("one complete JSON ledger entry".into()),
                    Some(source.to_string()),
                )));
            }
        };

        if entry.local_sequence != state.next_sequence {
            return Ok(LedgerScan::Quarantine(quarantine_status(
                path,
                LedgerFailureClass::SequenceDiscontinuity,
                Some(state.next_sequence),
                offset,
                Some(state.next_sequence.to_string()),
                Some(entry.local_sequence.to_string()),
            )));
        }
        if entry.ledger_id != ledger_id || entry.mother_node_id != mother_node_id {
            return Ok(LedgerScan::ForeignLineage(quarantine_status(
                path,
                LedgerFailureClass::ForeignLineage,
                Some(entry.local_sequence),
                offset,
                Some(format!("{ledger_id}/{mother_node_id}")),
                Some(format!("{}/{}", entry.ledger_id, entry.mother_node_id)),
            )));
        }
        if entry.previous_entry_hash != state.previous_hash {
            return Ok(LedgerScan::Quarantine(quarantine_status(
                path,
                LedgerFailureClass::PreviousHashMismatch,
                Some(entry.local_sequence),
                offset,
                state.previous_hash.clone(),
                entry.previous_entry_hash.clone(),
            )));
        }
        let expected_hash = entry_hash(&entry)?;
        if entry.entry_hash != expected_hash {
            return Ok(LedgerScan::Quarantine(quarantine_status(
                path,
                LedgerFailureClass::EntryHashMismatch,
                Some(entry.local_sequence),
                offset,
                Some(expected_hash),
                Some(entry.entry_hash.clone()),
            )));
        }

        state.next_sequence += 1;
        state.previous_hash = Some(entry.entry_hash.clone());
        state.entries.push(entry);
        frame_start = terminator + 1;
        state.committed_len = frame_start as u64;
    }

    if frame_start < bytes.len() {
        let residue = bytes[frame_start..].to_vec();
        let digest = blake3::hash(&residue).to_hex().to_string();
        return Ok(LedgerScan::Residue(LedgerTailResidue {
            offset: frame_start as u64,
            bytes: residue,
            digest,
            state,
        }));
    }

    Ok(LedgerScan::Ready(state))
}

fn quarantine_status(
    path: &Path,
    failure_class: LedgerFailureClass,
    first_bad_sequence: Option<u64>,
    first_bad_offset: u64,
    expected: Option<String>,
    observed: Option<String>,
) -> LedgerQuarantineStatus {
    LedgerQuarantineStatus {
        ledger_path: path.to_path_buf(),
        failure_class,
        first_bad_sequence,
        first_bad_offset,
        expected,
        observed,
        preserved_ledger_path: None,
        diagnostic_record_path: None,
    }
}

fn open_with_recovery_hook(
    path: &Path,
    ledger_id: &str,
    mother_node_id: &str,
    mut hook: impl FnMut(RecoveryStage) -> std::io::Result<()>,
) -> Result<JsonlObservationLedger> {
    let path = path.to_path_buf();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ObservationLedgerError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(&path)
        .map_err(|source| ObservationLedgerError::Io {
            path: path.clone(),
            source,
        })?;
    let mut file = acquire_writer_lock(&path, file)?;

    let (mut state, recovery_status) = match scan_existing(&path, ledger_id, mother_node_id)? {
        LedgerScan::Ready(mut state) => {
            let status = complete_pending_recovery(
                &path,
                &mut file,
                ledger_id,
                mother_node_id,
                &mut state,
                &mut hook,
            )?;
            (state, status)
        }
        LedgerScan::Residue(residue) => {
            let (state, status) = recover_tail(
                &path,
                &mut file,
                ledger_id,
                mother_node_id,
                residue,
                &mut hook,
            )?;
            (state, Some(status))
        }
        LedgerScan::Quarantine(status) => {
            let status = preserve_quarantine(&path, status)?;
            return Err(ObservationLedgerError::Quarantined {
                status: Box::new(status),
            });
        }
        LedgerScan::ForeignLineage(status) => {
            let status = preserve_quarantine(&path, status)?;
            return Err(ObservationLedgerError::ForeignLineage {
                status: Box::new(status),
            });
        }
    };

    // Pending-recovery completion may append the deterministic recovery fact.
    state.committed_len = std::fs::metadata(&path)
        .map_err(|source| ObservationLedgerError::Io {
            path: path.clone(),
            source,
        })?
        .len();

    Ok(JsonlObservationLedger {
        path,
        file,
        ledger_id: ledger_id.to_owned(),
        mother_node_id: mother_node_id.to_owned(),
        next_sequence: state.next_sequence,
        previous_hash: state.previous_hash,
        recovery_status,
        authority_tenure: None,
        writer_state: LedgerWriterState::Ready,
        #[cfg(test)]
        append_fault: None,
    })
}

fn recover_tail(
    path: &Path,
    file: &mut File,
    ledger_id: &str,
    mother_node_id: &str,
    residue: LedgerTailResidue,
    hook: &mut impl FnMut(RecoveryStage) -> std::io::Result<()>,
) -> Result<(LedgerScanState, LedgerRecoveryStatus)> {
    let mut status = ensure_residue_forensics(path, ledger_id, mother_node_id, &residue, hook)?;

    file.set_len(residue.state.committed_len)
        .map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.sync_data()
        .map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    call_recovery_hook(path, hook, RecoveryStage::LedgerTruncated)?;

    let mut state = residue.state;
    append_recovery_observation(file, path, ledger_id, mother_node_id, &status, &mut state)?;
    call_recovery_hook(path, hook, RecoveryStage::RecoveryObservationAppended)?;

    status.recovery_complete = true;
    write_json_durable(&status.diagnostic_record_path, &status)?;
    call_recovery_hook(path, hook, RecoveryStage::DiagnosticRecordCompleted)?;
    Ok((state, status))
}

fn ensure_residue_forensics(
    path: &Path,
    ledger_id: &str,
    mother_node_id: &str,
    residue: &LedgerTailResidue,
    hook: &mut impl FnMut(RecoveryStage) -> std::io::Result<()>,
) -> Result<LedgerRecoveryStatus> {
    let root = forensic_root_path(path);
    ensure_private_directory(&root)?;
    let case_dir = root.join(format!("tail-{}-{}", residue.offset, residue.digest));
    ensure_private_directory(&case_dir)?;
    call_recovery_hook(path, hook, RecoveryStage::ForensicDirectoryReady)?;

    let preserved_bytes_path = case_dir.join("source.bin");
    write_bytes_once_durable(&preserved_bytes_path, &residue.bytes)?;
    call_recovery_hook(path, hook, RecoveryStage::SourceBytesPreserved)?;

    let diagnostic_record_path = case_dir.join("record.json");
    let status = if diagnostic_record_path.exists() {
        read_recovery_status(&diagnostic_record_path)?
    } else {
        LedgerRecoveryStatus {
            ledger_path: path.to_path_buf(),
            source_ledger_id: ledger_id.to_owned(),
            source_mother_node_id: mother_node_id.to_owned(),
            residue_offset: residue.offset,
            residue_length: residue.bytes.len() as u64,
            residue_digest: residue.digest.clone(),
            last_committed_sequence: residue.state.next_sequence.checked_sub(1),
            last_committed_hash: residue.state.previous_hash.clone(),
            failure_class: "unterminated_final_frame".into(),
            recovery_decision_id: format!(
                "ledger-tail-recovery-{}-{}",
                residue.offset, residue.digest
            ),
            recovery_time: jiff::Timestamp::now().to_string(),
            preserved_bytes_path,
            diagnostic_record_path: diagnostic_record_path.clone(),
            recovery_complete: false,
        }
    };
    write_json_durable(&diagnostic_record_path, &status)?;
    call_recovery_hook(path, hook, RecoveryStage::DiagnosticRecordPreserved)?;
    Ok(status)
}

fn complete_pending_recovery(
    path: &Path,
    file: &mut File,
    ledger_id: &str,
    mother_node_id: &str,
    state: &mut LedgerScanState,
    hook: &mut impl FnMut(RecoveryStage) -> std::io::Result<()>,
) -> Result<Option<LedgerRecoveryStatus>> {
    let root = forensic_root_path(path);
    if !root.exists() {
        return Ok(None);
    }
    let mut records = std::fs::read_dir(&root)
        .map_err(|source| ObservationLedgerError::Io {
            path: root.clone(),
            source,
        })?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path().join("record.json"))
        .filter(|record| record.is_file())
        .collect::<Vec<_>>();
    records.sort();

    for record_path in records {
        let Ok(mut status) = read_recovery_status(&record_path) else {
            continue;
        };
        if status.ledger_path != path
            || status.source_ledger_id != ledger_id
            || status.source_mother_node_id != mother_node_id
            || status.recovery_complete
        {
            continue;
        }
        let already_observed = state
            .entries
            .iter()
            .any(|entry| entry.observation.observation_id.as_str() == status.recovery_decision_id);
        if !already_observed {
            append_recovery_observation(file, path, ledger_id, mother_node_id, &status, state)?;
            call_recovery_hook(path, hook, RecoveryStage::RecoveryObservationAppended)?;
        }
        status.recovery_complete = true;
        write_json_durable(&record_path, &status)?;
        call_recovery_hook(path, hook, RecoveryStage::DiagnosticRecordCompleted)?;
        return Ok(Some(status));
    }
    Ok(None)
}

fn append_recovery_observation(
    file: &mut File,
    path: &Path,
    ledger_id: &str,
    mother_node_id: &str,
    status: &LedgerRecoveryStatus,
    state: &mut LedgerScanState,
) -> Result<()> {
    let observation_id = ObservationId::new(status.recovery_decision_id.clone())
        .expect("deterministic recovery observation identity is non-empty");
    if state
        .entries
        .iter()
        .any(|entry| entry.observation.observation_id == observation_id)
    {
        return Ok(());
    }
    let observation = MctObservation {
        observation_id,
        observed_at: Timestamp::new(status.recovery_time.clone())
            .expect("recovery time is generated as RFC3339"),
        kind: ObservationKind::StorageAppendSucceeded,
        source_plane: SourcePlane::Storage,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace-{}", status.recovery_decision_id))
                .expect("deterministic recovery trace identity is non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(mother_node_id.to_owned()),
        resource_id: Some(ledger_id.to_owned()),
        policy_revision: None,
        grants_revision: None,
        outcome: ObservationOutcome::Completed,
        visibility: ObservationVisibility::NodeOperator,
        safe_message: "observation ledger tail recovered".into(),
        detail_ref: Some(format!("ledger-tail-recovery-v1:{}", status.residue_digest)),
    };
    let mut entry = MctObservationLedgerEntry {
        ledger_id: ledger_id.to_owned(),
        mother_node_id: mother_node_id.to_owned(),
        local_sequence: state.next_sequence,
        observation,
        previous_entry_hash: state.previous_hash.clone(),
        entry_hash: String::new(),
        appended_at: status.recovery_time.clone(),
        durability_class: DurabilityClass::BeforeEffect,
        export_status: ExportStatus::NotRequired,
    };
    entry.entry_hash = entry_hash(&entry)?;
    write_entry_durable(file, path, &entry)?;
    state.next_sequence += 1;
    state.previous_hash = Some(entry.entry_hash.clone());
    state.entries.push(entry);
    Ok(())
}

fn write_entry_durable(
    file: &mut File,
    path: &Path,
    entry: &MctObservationLedgerEntry,
) -> Result<()> {
    let mut bytes = serde_json::to_vec(entry).map_err(|source| ObservationLedgerError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    bytes.push(b'\n');
    file.write_all(&bytes)
        .map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.sync_data()
        .map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })
}

fn preserve_quarantine(
    path: &Path,
    mut status: LedgerQuarantineStatus,
) -> Result<LedgerQuarantineStatus> {
    let bytes = std::fs::read(path).map_err(|source| ObservationLedgerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let digest = blake3::hash(&bytes).to_hex().to_string();
    let root = forensic_root_path(path);
    ensure_private_directory(&root)?;
    let case_dir = root.join(format!(
        "quarantine-{}-{}-{:?}",
        status.first_bad_offset, digest, status.failure_class
    ));
    ensure_private_directory(&case_dir)?;
    let preserved_ledger_path = case_dir.join("source.bin");
    write_bytes_once_durable(&preserved_ledger_path, &bytes)?;
    let diagnostic_record_path = case_dir.join("record.json");
    status.preserved_ledger_path = Some(preserved_ledger_path);
    status.diagnostic_record_path = Some(diagnostic_record_path.clone());
    write_json_durable(&diagnostic_record_path, &status)?;
    Ok(status)
}

fn read_recovery_status(path: &Path) -> Result<LedgerRecoveryStatus> {
    let bytes = std::fs::read(path).map_err(|source| ObservationLedgerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| ObservationLedgerError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn ensure_private_directory(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|source| ObservationLedgerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    #[cfg(unix)]
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(|source| {
        ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        }
    })?;
    sync_directory(path)
}

fn write_bytes_once_durable(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        let existing = std::fs::read(path).map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if existing == bytes {
            return Ok(());
        }
        return Err(ObservationLedgerError::LedgerChanged {
            path: path.to_path_buf(),
            expected_sequence: bytes.len() as u64,
            actual_sequence: existing.len() as u64,
        });
    }
    write_bytes_durable(path, bytes)
}

fn write_json_durable(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|source| ObservationLedgerError::Json {
            path: path.to_path_buf(),
            source,
        })?;
    write_bytes_durable(path, &bytes)
}

fn write_bytes_durable(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temporary = path.with_extension("tmp");
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&temporary)
        .map_err(|source| ObservationLedgerError::Io {
            path: temporary.clone(),
            source,
        })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| ObservationLedgerError::Io {
            path: temporary.clone(),
            source,
        })?;
    std::fs::rename(&temporary, path).map_err(|source| ObservationLedgerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    sync_directory(parent)
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })
}

fn call_recovery_hook(
    path: &Path,
    hook: &mut impl FnMut(RecoveryStage) -> std::io::Result<()>,
    stage: RecoveryStage,
) -> Result<()> {
    hook(stage).map_err(|source| ObservationLedgerError::Io {
        path: path.to_path_buf(),
        source,
    })
}

struct LedgerEntryIter {
    path: PathBuf,
    ledger_id: String,
    mother_node_id: String,
    lines: Option<std::io::Lines<BufReader<File>>>,
    pending_error: Option<ObservationLedgerError>,
    expected_sequence: u64,
    previous_hash: Option<String>,
}

impl LedgerEntryIter {
    fn open(path: PathBuf, ledger_id: String, mother_node_id: String) -> Self {
        match File::open(&path) {
            Ok(file) => Self {
                path,
                ledger_id,
                mother_node_id,
                lines: Some(BufReader::new(file).lines()),
                pending_error: None,
                expected_sequence: 0,
                previous_hash: None,
            },
            Err(source) => Self {
                path: path.clone(),
                ledger_id,
                mother_node_id,
                lines: None,
                pending_error: Some(ObservationLedgerError::Io { path, source }),
                expected_sequence: 0,
                previous_hash: None,
            },
        }
    }

    fn fail(&mut self, error: ObservationLedgerError) -> Option<Result<MctObservationLedgerEntry>> {
        self.lines = None;
        Some(Err(error))
    }

    fn validate_entry(
        &mut self,
        entry: MctObservationLedgerEntry,
    ) -> Result<MctObservationLedgerEntry> {
        if entry.local_sequence != self.expected_sequence {
            return Err(ObservationLedgerError::SequenceMismatch {
                expected: self.expected_sequence,
                actual: entry.local_sequence,
            });
        }

        if entry.ledger_id != self.ledger_id || entry.mother_node_id != self.mother_node_id {
            return Err(ObservationLedgerError::LedgerIdentityMismatch {
                sequence: entry.local_sequence,
                expected_ledger_id: self.ledger_id.clone(),
                expected_mother_node_id: self.mother_node_id.clone(),
                actual_ledger_id: entry.ledger_id.clone(),
                actual_mother_node_id: entry.mother_node_id.clone(),
            });
        }

        if entry.previous_entry_hash != self.previous_hash {
            return Err(ObservationLedgerError::BrokenHashChain {
                sequence: entry.local_sequence,
            });
        }
        let expected = entry_hash(&entry)?;
        if entry.entry_hash != expected {
            return Err(ObservationLedgerError::BrokenHashChain {
                sequence: entry.local_sequence,
            });
        }
        self.previous_hash = Some(entry.entry_hash.clone());
        self.expected_sequence += 1;
        Ok(entry)
    }
}

impl Iterator for LedgerEntryIter {
    type Item = Result<MctObservationLedgerEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(error) = self.pending_error.take() {
            return Some(Err(error));
        }
        loop {
            let line = match self.lines.as_mut()?.next()? {
                Ok(line) => line,
                Err(source) => {
                    return self.fail(ObservationLedgerError::Io {
                        path: self.path.clone(),
                        source,
                    });
                }
            };
            if line.trim().is_empty() {
                continue;
            }
            let entry = match serde_json::from_str(&line) {
                Ok(entry) => entry,
                Err(source) => {
                    return self.fail(ObservationLedgerError::Json {
                        path: self.path.clone(),
                        source,
                    });
                }
            };
            return match self.validate_entry(entry) {
                Ok(entry) => Some(Ok(entry)),
                Err(error) => self.fail(error),
            };
        }
    }
}

fn acquire_writer_lock(path: &Path, file: File) -> Result<File> {
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => Err(ObservationLedgerError::WriterContended {
            path: path.to_path_buf(),
        }),
        Err(std::fs::TryLockError::Error(source)) => Err(ObservationLedgerError::WriterLock {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn entry_hash(entry: &MctObservationLedgerEntry) -> Result<String> {
    let mut hashable = entry.clone();
    hashable.entry_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|source| ObservationLedgerError::Json {
        path: PathBuf::from("<entry>"),
        source,
    })?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

/// Returns the crate version for health and smoke tests.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mct_kernel::{CallId, MctObservation, ObservationId, ObservationKind, Timestamp, TraceId};

    fn observation(id: &str, trace: &str, call: Option<&str>) -> MctObservation {
        let mut obs = MctObservation::informational(
            ObservationId::new(id).expect("string ID literal/generated value must be non-empty"),
            Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            ObservationKind::PeerHelloReceived,
            TraceId::new(trace).expect("string ID literal/generated value must be non-empty"),
            "hello received",
        );
        obs.call_id = call.map(|call| {
            CallId::new(call).expect("string ID literal/generated value must be non-empty")
        });
        obs
    }

    #[test]
    fn exposes_version() {
        assert_eq!(super::version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn append_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        let entry = ledger
            .append_before_effect(
                observation("obs-1", "trace-1", Some("call-1")),
                "2026-05-31T00:00:01Z",
            )
            .unwrap();
        assert_eq!(entry.local_sequence, 0);
        assert!(entry.previous_entry_hash.is_none());

        let entries = ledger.entries().unwrap();
        let streamed = ledger.iter_entries().collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(streamed, entries);
        assert_eq!(entries[0].entry_hash, entry.entry_hash);
    }

    #[test]
    fn reopens_existing_hash_chain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        let first = ledger
            .append_before_effect(
                observation("obs-1", "trace-1", None),
                "2026-05-31T00:00:01Z",
            )
            .unwrap();
        drop(ledger);

        let mut reopened = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        let second = reopened
            .append_before_effect(
                observation("obs-2", "trace-1", Some("call-1")),
                "2026-05-31T00:00:02Z",
            )
            .unwrap();
        assert_eq!(second.local_sequence, 1);
        assert_eq!(
            second.previous_entry_hash.as_deref(),
            Some(first.entry_hash.as_str())
        );
    }

    #[test]
    fn reopening_with_wrong_ledger_identity_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        ledger
            .append_before_effect(
                observation("obs-1", "trace-1", None),
                "2026-05-31T00:00:01Z",
            )
            .unwrap();
        drop(ledger);

        let result = JsonlObservationLedger::open(&path, "ledger-b", "mother-a");

        assert!(matches!(
            result,
            Err(ObservationLedgerError::ForeignLineage { status })
                if status.first_bad_sequence == Some(0)
        ));
    }

    #[test]
    fn second_open_writer_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let _ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();

        let result = JsonlObservationLedger::open(&path, "ledger-a", "mother-a");

        assert!(matches!(
            result,
            Err(ObservationLedgerError::WriterContended { .. })
        ));
    }

    #[test]
    fn read_only_open_does_not_contend_for_writer_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        ledger
            .append_before_effect(
                observation("obs-1", "trace-1", Some("call-1")),
                "2026-05-31T00:00:01Z",
            )
            .unwrap();

        let reader = JsonlObservationLedger::open_read_only(&path, "ledger-a", "mother-a")
            .expect("read-only ledger access must not acquire the writer lock");
        let call_entries = reader
            .by_call(
                &CallId::new("call-1")
                    .expect("string ID literal/generated value must be non-empty"),
            )
            .unwrap();

        assert_eq!(call_entries.len(), 1);
    }

    #[test]
    fn read_only_open_enforces_identity_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        ledger
            .append_before_effect(
                observation("obs-1", "trace-1", None),
                "2026-05-31T00:00:01Z",
            )
            .unwrap();

        let result = JsonlObservationLedger::open_read_only(&path, "ledger-b", "mother-a");

        assert!(matches!(
            result,
            Err(ObservationLedgerError::ForeignLineage { status })
                if status.first_bad_sequence == Some(0)
                    && status.preserved_ledger_path.is_none()
        ));
    }

    #[test]
    fn stale_marker_lock_file_does_not_block_opening_writer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let marker_lock_path = path.with_file_name("observations.jsonl.lock");
        std::fs::write(&marker_lock_path, "stale marker from crashed writer").unwrap();

        let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        let entry = ledger
            .append_before_effect(
                observation("obs-1", "trace-1", None),
                "2026-05-31T00:00:01Z",
            )
            .unwrap();

        assert_eq!(entry.local_sequence, 0);
        assert!(marker_lock_path.exists());
    }

    #[test]
    fn queries_by_trace_and_call() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        ledger
            .append_before_effect(
                observation("obs-1", "trace-1", Some("call-1")),
                "2026-05-31T00:00:01Z",
            )
            .unwrap();
        ledger
            .append_before_effect(
                observation("obs-2", "trace-2", Some("call-2")),
                "2026-05-31T00:00:02Z",
            )
            .unwrap();
        assert_eq!(
            ledger
                .by_trace(
                    &TraceId::new("trace-1")
                        .expect("string ID literal/generated value must be non-empty")
                )
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            ledger
                .by_call(
                    &CallId::new("call-2")
                        .expect("string ID literal/generated value must be non-empty")
                )
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn batch_persists_adapter_and_kernel_observations_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut adapter_observation = observation("obs-adapter", "trace-1", Some("call-1"));
        adapter_observation.source_plane = mct_kernel::SourcePlane::Adapter;
        adapter_observation.kind = ObservationKind::AdapterEffectStarted;
        let mut kernel_observation = observation("obs-kernel", "trace-1", Some("call-1"));
        kernel_observation.source_plane = mct_kernel::SourcePlane::Kernel;
        kernel_observation.kind = ObservationKind::CallAuthorized;

        let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        let entries = ledger
            .append_batch_before_effect(
                vec![adapter_observation, kernel_observation],
                "2026-05-31T00:00:03Z",
            )
            .unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].local_sequence, 0);
        assert_eq!(entries[1].local_sequence, 1);
        assert_eq!(
            entries[1].previous_entry_hash.as_deref(),
            Some(entries[0].entry_hash.as_str())
        );

        let trace_entries = ledger
            .by_trace(
                &TraceId::new("trace-1")
                    .expect("string ID literal/generated value must be non-empty"),
            )
            .unwrap();
        assert_eq!(trace_entries.len(), 2);
        assert_eq!(
            trace_entries[0].observation.kind,
            ObservationKind::AdapterEffectStarted
        );
        assert_eq!(
            trace_entries[1].observation.kind,
            ObservationKind::CallAuthorized
        );
        assert_eq!(
            ledger
                .by_call(
                    &CallId::new("call-1")
                        .expect("string ID literal/generated value must be non-empty")
                )
                .unwrap()
                .len(),
            2
        );
    }

    fn append_raw(path: &Path, bytes: &[u8]) {
        let mut file = OpenOptions::new().append(true).open(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_data().unwrap();
    }

    fn ledger_lines(path: &Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn rewrite_lines(path: &Path, lines: &[serde_json::Value]) {
        let mut encoded = lines
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
            .join("\n");
        encoded.push('\n');
        std::fs::write(path, encoded).unwrap();
    }

    fn forensic_records(path: &Path) -> Vec<PathBuf> {
        let root = forensic_root_path(path);
        if !root.exists() {
            return Vec::new();
        }
        let mut records = std::fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().path().join("record.json"))
            .filter(|record| record.exists())
            .collect::<Vec<_>>();
        records.sort();
        records
    }

    fn append_fixture_entry(path: &Path, id: &str) -> MctObservationLedgerEntry {
        let mut ledger = JsonlObservationLedger::open(path, "ledger-a", "mother-a").unwrap();
        let entry = ledger
            .append_before_effect(
                observation(id, "trace-recovery", None),
                "2026-05-31T00:00:01Z",
            )
            .unwrap();
        drop(ledger);
        entry
    }

    /// Proof 1: crash before any new frame bytes leaves exactly the previous committed head.
    #[test]
    fn crash_before_frame_bytes_reopens_at_previous_head() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let first = append_fixture_entry(&path, "obs-before-frame-crash");
        let before = std::fs::read(&path).unwrap();

        let mut reopened = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(reopened.entries().unwrap(), vec![first.clone()]);
        let next = reopened
            .append_before_effect(
                observation("obs-after-frame-crash", "trace-recovery", None),
                "2026-05-31T00:00:02Z",
            )
            .unwrap();
        assert_eq!(next.local_sequence, 1);
        assert_eq!(
            next.previous_entry_hash.as_deref(),
            Some(first.entry_hash.as_str())
        );
    }

    /// Proof 2: a torn final frame is preserved before recovery and never changes the committed prefix.
    #[test]
    fn torn_unterminated_tail_is_preserved_and_recovered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let first = append_fixture_entry(&path, "obs-before-torn-tail");
        let committed = std::fs::read(&path).unwrap();
        let residue = br#"{"ledger_id":"ledger-a","mother_node_id":"mother-a""#;
        append_raw(&path, residue);

        let reader = JsonlObservationLedger::open_read_only(&path, "ledger-a", "mother-a");
        assert!(matches!(
            reader,
            Err(ObservationLedgerError::UnterminatedTail { offset, length, .. })
                if offset == committed.len() as u64 && length == residue.len() as u64
        ));
        assert!(forensic_records(&path).is_empty());

        let mut recovered = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        let recovery = recovered
            .recovery_status()
            .expect("tail recovery is reported");
        assert_eq!(recovery.residue_offset, committed.len() as u64);
        assert_eq!(recovery.residue_length, residue.len() as u64);
        assert_eq!(
            std::fs::read(&recovery.preserved_bytes_path).unwrap(),
            residue
        );
        assert_eq!(&std::fs::read(&path).unwrap()[..committed.len()], committed);
        assert_eq!(recovered.entries().unwrap()[0], first);
        assert_eq!(
            recovered.entries().unwrap()[1].observation.safe_message,
            "observation ledger tail recovered"
        );
        let next = recovered
            .append_before_effect(
                observation("obs-after-torn-tail", "trace-recovery", None),
                "2026-05-31T00:00:03Z",
            )
            .unwrap();
        assert_eq!(next.local_sequence, 2);
        assert_eq!(
            next.previous_entry_hash.as_deref(),
            Some(recovered.entries().unwrap()[1].entry_hash.as_str())
        );
    }

    /// Proof 3: an undecodable unterminated final frame is residue under the same rule.
    #[test]
    fn unparseable_unterminated_final_frame_is_residue() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let first = append_fixture_entry(&path, "obs-before-binary-tail");
        let residue = b"\xff\0not-json";
        append_raw(&path, residue);

        let recovered = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        let status = recovered.recovery_status().unwrap();
        assert_eq!(
            std::fs::read(&status.preserved_bytes_path).unwrap(),
            residue
        );
        let entries = recovered.entries().unwrap();
        assert_eq!(entries[0], first);
        assert_eq!(entries.len(), 2);
    }

    /// Proof 4: a terminated malformed final frame quarantines without truncation.
    #[test]
    fn terminated_malformed_frame_quarantines_and_preserves_entire_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        append_fixture_entry(&path, "obs-before-malformed-frame");
        append_raw(&path, b"not-json\n");
        let original = std::fs::read(&path).unwrap();

        let error = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap_err();
        let status = match error {
            ObservationLedgerError::Quarantined { status } => *status,
            other => panic!("expected typed quarantine, got {other:?}"),
        };
        assert_eq!(
            status.failure_class,
            LedgerFailureClass::TerminatedMalformedFrame
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(
            std::fs::read(status.preserved_ledger_path.unwrap()).unwrap(),
            original
        );
    }

    /// Proof 5: a mid-file hash break reports first-bad offset and expected/observed evidence.
    #[test]
    fn hash_break_quarantines_with_diagnostic_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        append_fixture_entry(&path, "obs-hash-0");
        {
            let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
            ledger
                .append_before_effect(
                    observation("obs-hash-1", "trace-recovery", None),
                    "2026-05-31T00:00:02Z",
                )
                .unwrap();
        }
        {
            let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
            ledger
                .append_before_effect(
                    observation("obs-hash-2", "trace-recovery", None),
                    "2026-05-31T00:00:03Z",
                )
                .unwrap();
        }
        let mut lines = ledger_lines(&path);
        lines[1]["entry_hash"] = serde_json::Value::String("forged-entry-hash".into());
        rewrite_lines(&path, &lines);
        let first_line_length = std::fs::read(&path)
            .unwrap()
            .iter()
            .position(|byte| *byte == b'\n')
            .unwrap()
            + 1;

        let error = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap_err();
        let status = match error {
            ObservationLedgerError::Quarantined { status } => *status,
            other => panic!("expected hash quarantine, got {other:?}"),
        };
        assert_eq!(status.failure_class, LedgerFailureClass::EntryHashMismatch);
        assert_eq!(status.first_bad_sequence, Some(1));
        assert_eq!(status.first_bad_offset, first_line_length as u64);
        assert!(status.expected.is_some());
        assert_eq!(status.observed.as_deref(), Some("forged-entry-hash"));
    }

    /// Proof 6: gaps, duplicates, and regressions quarantine rather than being skipped or renumbered.
    #[test]
    fn every_sequence_discontinuity_quarantines_without_repair() {
        for (name, line_index, expected, sequence) in [
            ("gap", 1_usize, 1_u64, 2_u64),
            ("duplicate", 1, 1, 0),
            ("regression", 2, 2, 0),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(format!("{name}.jsonl"));
            append_fixture_entry(&path, "obs-sequence-0");
            {
                let mut ledger =
                    JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
                for index in 1..=line_index {
                    ledger
                        .append_before_effect(
                            observation(&format!("obs-sequence-{index}"), "trace-recovery", None),
                            "2026-05-31T00:00:02Z",
                        )
                        .unwrap();
                }
            }
            let mut lines = ledger_lines(&path);
            lines[line_index]["local_sequence"] = serde_json::Value::from(sequence);
            rewrite_lines(&path, &lines);
            let original = std::fs::read(&path).unwrap();

            let error = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap_err();
            let status = match error {
                ObservationLedgerError::Quarantined { status } => *status,
                other => panic!("expected sequence quarantine for {name}, got {other:?}"),
            };
            assert_eq!(
                status.failure_class,
                LedgerFailureClass::SequenceDiscontinuity
            );
            assert_eq!(
                status.expected.as_deref(),
                Some(expected.to_string().as_str())
            );
            assert_eq!(
                status.observed.as_deref(),
                Some(sequence.to_string().as_str())
            );
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
    }

    /// Proof 7: foreign ledger or Mother lineage is typed and is never automatically adopted.
    #[test]
    fn wrong_identity_is_typed_foreign_lineage_without_adoption() {
        for (ledger_id, mother_id) in [("ledger-b", "mother-a"), ("ledger-a", "mother-b")] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("observations.jsonl");
            append_fixture_entry(&path, "obs-foreign-lineage");
            let original = std::fs::read(&path).unwrap();

            let error = JsonlObservationLedger::open(&path, ledger_id, mother_id).unwrap_err();
            let status = match error {
                ObservationLedgerError::ForeignLineage { status } => *status,
                other => panic!("expected foreign lineage, got {other:?}"),
            };
            assert_eq!(status.first_bad_sequence, Some(0));
            assert_eq!(std::fs::read(&path).unwrap(), original);
            assert_eq!(
                std::fs::read(status.preserved_ledger_path.unwrap()).unwrap(),
                original
            );
        }
    }

    /// Proof 8: a complete valid unacknowledged frame is recovered as committed, never duplicated.
    #[test]
    fn complete_unacknowledged_final_frame_is_committed_on_rescan() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let unacknowledged = append_fixture_entry(&path, "obs-unacknowledged");

        let mut reopened = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        assert!(reopened.recovery_status().is_none());
        assert_eq!(reopened.entries().unwrap(), vec![unacknowledged.clone()]);
        let next = reopened
            .append_before_effect(
                observation("obs-after-unacknowledged", "trace-recovery", None),
                "2026-05-31T00:00:02Z",
            )
            .unwrap();
        assert_eq!(next.local_sequence, 1);
        assert_eq!(
            next.previous_entry_hash.as_deref(),
            Some(unacknowledged.entry_hash.as_str())
        );
        assert_eq!(reopened.entries().unwrap().len(), 2);
    }

    /// Proof 13: interruption at each preservation stage leaves original or preserved bytes and reruns idempotently.
    #[test]
    fn interrupted_recovery_is_idempotent_at_every_preservation_stage() {
        for stage in RecoveryStage::ALL {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("observations.jsonl");
            append_fixture_entry(&path, "obs-interrupted-recovery");
            let residue = format!("unterminated-at-{stage:?}").into_bytes();
            append_raw(&path, &residue);
            let original = std::fs::read(&path).unwrap();

            let mut failed_once = false;
            let result = open_with_recovery_hook(&path, "ledger-a", "mother-a", |observed_stage| {
                if observed_stage == stage && !failed_once {
                    failed_once = true;
                    return Err(std::io::Error::other("injected recovery interruption"));
                }
                Ok(())
            });
            assert!(
                result.is_err(),
                "stage {stage:?} did not interrupt recovery"
            );
            let original_available = std::fs::read(&path).unwrap() == original;
            let preserved_available = forensic_records(&path).iter().any(|record| {
                let bytes_path = record.parent().unwrap().join("source.bin");
                bytes_path.exists() && std::fs::read(bytes_path).unwrap() == residue
            });
            assert!(original_available || preserved_available);

            let reopened = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
            let recovery_observations = reopened
                .entries()
                .unwrap()
                .into_iter()
                .filter(|entry| {
                    entry.observation.safe_message == "observation ledger tail recovered"
                })
                .count();
            assert_eq!(
                recovery_observations, 1,
                "stage {stage:?} duplicated recovery"
            );
        }
    }

    /// Proof 14: encoded entry content cannot create an interior unescaped frame terminator.
    #[test]
    fn escapable_entry_content_round_trips_without_forging_frame_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut obs = observation("obs-escaped", "trace-escaped", None);
        obs.safe_message =
            "line one\nline two\r\t\0quote:\" slash:\\ controls:\u{0001}\u{001f}".into();
        let expected = obs.safe_message.clone();
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        ledger
            .append_before_effect(obs, "2026-05-31T00:00:01Z")
            .unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.iter().filter(|byte| **byte == b'\n').count(), 1);
        assert_eq!(
            ledger.entries().unwrap()[0].observation.safe_message,
            expected
        );
    }

    fn forensic_tree(path: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        fn visit(root: &Path, current: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
            if !current.exists() {
                return;
            }
            for entry in std::fs::read_dir(current).unwrap() {
                let entry = entry.unwrap();
                let child = entry.path();
                if child.is_dir() {
                    visit(root, &child, files);
                } else {
                    files.push((
                        child.strip_prefix(root).unwrap().to_path_buf(),
                        std::fs::read(child).unwrap(),
                    ));
                }
            }
        }
        let root = forensic_root_path(path);
        let mut files = Vec::new();
        visit(&root, &root, &mut files);
        files.sort_by(|left, right| left.0.cmp(&right.0));
        files
    }

    /// Proof 9: either write or durability uncertainty poisons all later appends without another byte.
    #[test]
    fn write_and_sync_uncertainty_poison_writer_without_later_file_changes() {
        for fault in [
            TestAppendFault::PartialFrame,
            TestAppendFault::CompleteFrameBeforeDurabilityAck,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("observations.jsonl");
            let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
            ledger.inject_append_fault_after_for_test(0, fault);

            let first = ledger.append_before_effect(
                observation("obs-uncertain", "trace-poison", None),
                "2026-05-31T00:00:01Z",
            );
            assert!(matches!(
                first,
                Err(ObservationLedgerError::AppendCommitUnknown { .. })
            ));
            assert!(ledger.is_poisoned());
            let after_failure = std::fs::read(&path).unwrap();
            for index in 0..3 {
                let later = ledger.append_before_effect(
                    observation(&format!("obs-poisoned-{index}"), "trace-poison", None),
                    "2026-05-31T00:00:02Z",
                );
                assert!(matches!(
                    later,
                    Err(ObservationLedgerError::WriterPoisoned { .. })
                ));
                assert_eq!(std::fs::read(&path).unwrap(), after_failure);
            }
        }
    }

    /// Proof 10: exclusive reopen resolves uncertain bytes as committed, residue, or quarantine.
    #[test]
    fn poisoned_writer_reopen_resolves_all_three_commit_states() {
        for (fault, expected) in [
            (
                TestAppendFault::CompleteFrameAfterDurabilityAck,
                "committed",
            ),
            (TestAppendFault::PartialFrame, "residue"),
            (TestAppendFault::TerminatedCorruption, "quarantine"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(format!("{expected}.jsonl"));
            let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
            ledger.inject_append_fault_after_for_test(0, fault);
            let result = ledger.append_before_effect(
                observation("obs-uncertain-resolution", "trace-poison", None),
                "2026-05-31T00:00:01Z",
            );
            assert!(matches!(
                result,
                Err(ObservationLedgerError::AppendCommitUnknown { .. })
            ));
            drop(ledger);

            match expected {
                "committed" => {
                    let mut reopened =
                        JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
                    assert!(reopened.recovery_status().is_none());
                    assert_eq!(reopened.entries().unwrap().len(), 1);
                    assert_eq!(
                        reopened
                            .append_before_effect(
                                observation("obs-after-committed", "trace-poison", None),
                                "2026-05-31T00:00:02Z",
                            )
                            .unwrap()
                            .local_sequence,
                        1
                    );
                }
                "residue" => {
                    let reopened =
                        JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
                    assert!(reopened.recovery_status().is_some());
                    assert_eq!(reopened.entries().unwrap().len(), 1);
                    assert_eq!(
                        reopened.entries().unwrap()[0].observation.safe_message,
                        "observation ledger tail recovered"
                    );
                }
                "quarantine" => {
                    assert!(matches!(
                        JsonlObservationLedger::open(&path, "ledger-a", "mother-a"),
                        Err(ObservationLedgerError::Quarantined { .. })
                    ));
                }
                _ => unreachable!(),
            }
        }
    }

    /// Proof 11: an acknowledged batch prefix remains committed and is reported without rollback.
    #[test]
    fn batch_failure_reports_and_preserves_acknowledged_committed_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        ledger.inject_append_fault_after_for_test(1, TestAppendFault::PartialFrame);

        let error = ledger
            .append_batch_before_effect(
                [
                    observation("obs-batch-0", "trace-batch", None),
                    observation("obs-batch-1", "trace-batch", None),
                    observation("obs-batch-2", "trace-batch", None),
                ],
                "2026-05-31T00:00:01Z",
            )
            .unwrap_err();
        let outcome = match error {
            ObservationLedgerError::BatchPartiallyCommitted { outcome } => *outcome,
            other => panic!("expected typed partial batch outcome, got {other:?}"),
        };
        assert_eq!(outcome.acknowledged_committed_prefix.len(), 1);
        assert_eq!(outcome.acknowledged_committed_prefix[0].local_sequence, 0);
        assert_eq!(outcome.failed_index, 1);
        assert!(outcome.commit_unknown);
        drop(ledger);

        let reopened = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        let entries = reopened.entries().unwrap();
        assert_eq!(
            entries[0].observation.observation_id.as_str(),
            "obs-batch-0"
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.observation.observation_id.as_str() == "obs-batch-0")
                .count(),
            1
        );
    }

    /// Proof 12: contention is typed and cannot trigger ledger or forensic recovery mutations.
    #[test]
    fn contending_writer_is_typed_and_byte_identical_without_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let _owner = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        append_raw(&path, b"unterminated-contention-residue");
        let ledger_before = std::fs::read(&path).unwrap();
        let forensics_before = forensic_tree(&path);

        let contender = JsonlObservationLedger::open(&path, "ledger-a", "mother-a");
        assert!(matches!(
            contender,
            Err(ObservationLedgerError::WriterContended { .. })
        ));
        assert_eq!(std::fs::read(&path).unwrap(), ledger_before);
        assert_eq!(forensic_tree(&path), forensics_before);
    }

    fn authority_mutation_request(id: &str) -> AuthorityMutationRequestV1 {
        AuthorityMutationRequestV1 {
            mutation_id: id.into(),
            changes: vec![
                AuthorityChangeV1::ToyCatalogPut {
                    toy_id: "toy-a".into(),
                    contract: ToyContractIdentity {
                        namespace: "mct:test".into(),
                        interface_name: "toy".into(),
                        version: "1.0.0".into(),
                        function_name: Some("run".into()),
                        resource_name: None,
                    },
                    authority_bearing: true,
                    catalog_revision: 1,
                    admitted_by_observation_id: "obs:catalog:toy-a".into(),
                },
                AuthorityChangeV1::ToyGrantPut {
                    grant_id: "grant-a".into(),
                    toy_id: "toy-a".into(),
                    subject: Box::new(ToyGrantSubject {
                        child_name: "child-a".into(),
                        artifact_id: "artifact-a".into(),
                        artifact_version: "1.0.0".into(),
                        assignment_id: None,
                        caller_node_id: None,
                    }),
                    scope: Box::new(ToyGrantScope {
                        vision_id: mct_kernel::VisionId::new("vision-a").unwrap(),
                        node_id: None,
                        project_id: None,
                        data_classification: None,
                        resource_id: Some("resource-a".into()),
                        allowed_actions: vec!["run".into()],
                    }),
                    constraints: Box::new(ToyGrantConstraints {
                        starts_at: None,
                        expires_at: None,
                        max_uses: None,
                        max_duration_ms: None,
                        locality_required: true,
                    }),
                    grant_state: ToyGrantState::Active,
                    issuer_id: "mother-a".into(),
                    policy_revision: 1,
                    source_grants_revision: 1,
                    authority_observation_id: "obs:grant:grant-a".into(),
                },
            ],
            grant_shaping_sources: vec![GrantShapingSourceV1::OperatorDecision {
                decision_id: format!("decision:{id}"),
                authenticated_principal_ref: "os-uid:501".into(),
                command_kind: GrantShapingCommandKindV1::GrantChange,
            }],
            decided_at: "2026-08-03T18:00:00Z".into(),
        }
    }

    /// Phase H2 proof 4: canonical structured content commits before the legacy write and replays alone.
    #[test]
    fn authority_mutation_fact_precedes_legacy_write_and_reconstructs_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        let result =
            ledger.execute_authority_mutation(authority_mutation_request("mutation-4"), |state| {
                let entries = read_ledger_entries(&path, "ledger-a", "mother-a").unwrap();
                assert_eq!(replay_authority_entries(&entries).unwrap().state, *state);
                Ok(Some("projection-hash-4".into()))
            });
        let entries = ledger.entries().unwrap();
        let replay = replay_authority_entries(&entries).unwrap();

        assert!(matches!(
            result,
            AuthorityMutationResultV1::Committed { .. }
        ));
        assert_eq!(replay.state.toy_catalog.len(), 1);
        assert_eq!(replay.state.toy_grants.len(), 1);
        assert_eq!(replay.current_authority.unwrap().generation, 1);
    }

    /// Phase H2 proof 5: uncertain commit suppresses legacy state and same-ID reopen resolves once.
    #[test]
    fn authority_commit_unknown_suppresses_legacy_and_same_id_retry_deduplicates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let request = authority_mutation_request("mutation-5");
        let legacy_called = std::cell::Cell::new(false);
        let mut ledger =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        ledger.inject_append_fault_after_for_test(
            0,
            TestAppendFault::CompleteFrameAfterDurabilityAck,
        );
        let uncertain = ledger.execute_authority_mutation(request.clone(), |_| {
            legacy_called.set(true);
            Ok(Some("must-not-run".into()))
        });
        assert!(matches!(
            uncertain,
            AuthorityMutationResultV1::CommitUnknown { .. }
        ));
        assert!(!legacy_called.get());
        drop(ledger);

        let mut reopened =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        let resolved = reopened.execute_authority_mutation(request, |_| {
            legacy_called.set(true);
            Ok(Some("projection-hash-5".into()))
        });
        let replay = replay_authority_entries(&reopened.entries().unwrap()).unwrap();

        assert!(matches!(
            resolved,
            AuthorityMutationResultV1::Committed {
                resolution: AuthorityMutationResolutionV1::ResolvedExistingFact,
                ..
            }
        ));
        assert!(legacy_called.get());
        assert_eq!(replay.mutations.len(), 1);
    }

    /// Phase H2 proof 6: rejection before append leaves canonical and legacy state untouched.
    #[test]
    fn rejected_authority_mutation_changes_neither_ledger_facts_nor_legacy_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        let before = std::fs::read(&path).unwrap();
        let legacy_called = std::cell::Cell::new(false);
        let mut request = authority_mutation_request("mutation-6");
        request.changes.clear();
        let result = ledger.execute_authority_mutation(request, |_| {
            legacy_called.set(true);
            Ok(Some("must-not-run".into()))
        });

        assert!(matches!(
            result,
            AuthorityMutationResultV1::RejectedBeforeCommit {
                reason: AuthorityMutationRejectionReasonV1::InvalidRequest,
                ..
            }
        ));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!legacy_called.get());
    }

    /// Phase H2 proof 7: canonical commitment survives a distinct projection failure result.
    #[test]
    fn committed_authority_fact_reports_projection_pending_distinctly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        let result = ledger
            .execute_authority_mutation(authority_mutation_request("mutation-7"), |_| {
                Err("injected projection failure".into())
            });
        let replay = replay_authority_entries(&ledger.entries().unwrap()).unwrap();

        assert!(matches!(
            result,
            AuthorityMutationResultV1::CommittedProjectionPending {
                pending_reason: AuthorityProjectionPendingReasonV1::ProjectionFailed,
                ..
            }
        ));
        assert_eq!(replay.mutations.len(), 1);
        assert_eq!(replay.current_authority.unwrap().generation, 1);
    }

    /// Phase H2 proof 1: an authority writer exposes only an acknowledged epoch fact.
    #[test]
    fn fresh_authority_tenure_commits_epoch_before_mutation_admission() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");

        let ledger = JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a")
            .expect("fresh authority tenure must commit its epoch");
        let tenure = ledger.authority_tenure().expect("epoch is exposed");
        let entries = ledger.entries().unwrap();

        assert_eq!(tenure.entry.local_sequence, 0);
        assert_eq!(entries, vec![tenure.entry.clone()]);
        assert_eq!(
            tenure.fact.predecessor,
            AuthorityEpochPredecessorV1::NoneForVirgin
        );
        assert_eq!(tenure.fact.generation_baseline, 0);
        assert_eq!(
            tenure.fact.resulting_authority.authority_epoch,
            tenure.fact.authority_epoch
        );
        assert!(
            tenure
                .fact
                .authority_epoch
                .starts_with(AUTHORITY_EPOCH_PREFIX_V1)
        );
    }

    /// Phase H2 proof 2: each tenure, including a byte-copied restore, receives fresh entropy identity.
    #[test]
    fn authority_tenures_and_byte_copied_restore_use_distinct_epochs() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.jsonl");
        let first_epoch = {
            let ledger =
                JsonlObservationLedger::open_authority(&source, "ledger-a", "mother-a").unwrap();
            ledger
                .authority_tenure()
                .unwrap()
                .fact
                .authority_epoch
                .clone()
        };
        let second_epoch = {
            let ledger =
                JsonlObservationLedger::open_authority(&source, "ledger-a", "mother-a").unwrap();
            ledger
                .authority_tenure()
                .unwrap()
                .fact
                .authority_epoch
                .clone()
        };
        let restored = dir.path().join("restored.jsonl");
        std::fs::copy(&source, &restored).unwrap();
        std::fs::write(
            dir.path().join("restored-projection.sqlite"),
            b"byte-copied-projection",
        )
        .unwrap();
        let restored_ledger =
            JsonlObservationLedger::open_authority(&restored, "ledger-a", "mother-a").unwrap();
        let restored_epoch = &restored_ledger
            .authority_tenure()
            .unwrap()
            .fact
            .authority_epoch;

        assert_ne!(first_epoch, second_epoch);
        assert_ne!(&first_epoch, restored_epoch);
        assert_ne!(&second_epoch, restored_epoch);
    }

    /// Phase H2 proof 3: ledger bytes alone reproduce the complete current epoch identity.
    #[test]
    fn epoch_identity_is_replay_complete_from_ledger_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let expected = {
            let ledger =
                JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
            ledger
                .authority_tenure()
                .unwrap()
                .fact
                .resulting_authority
                .clone()
        };
        let entries = read_ledger_entries(&path, "ledger-a", "mother-a").unwrap();
        let replayed = replay_authority_entries(&entries).unwrap();

        assert_eq!(replayed.current_authority, Some(expected));
        assert_eq!(replayed.canonical_fact_count, 1);
    }

    #[test]
    fn opening_directory_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let result = JsonlObservationLedger::open(dir.path(), "ledger-a", "mother-a");
        assert!(matches!(result, Err(ObservationLedgerError::Io { .. })));
    }
}

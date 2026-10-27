//! Append-only observation ledger support for MCT.
//!
//! Runtime truth starts from `MctObservation` facts defined by `mct-kernel`.
//! Storage details stay in this crate and do not leak into the kernel.

#![forbid(unsafe_code)]

mod admin_genesis;
pub use admin_genesis::{
    AdminGenesisFactV1, AdminGenesisIntentV1, AdminGenesisRequestV1, AdminGenesisSetupV1,
};
#[cfg(test)]
mod admin_genesis_tests;
mod admin_issuance;
mod guardian_recovery;
pub use admin_issuance::{AdminKeyIssuanceRecordV1, AdminKeyIssueIntentV1, AdminKeyIssueRequestV1};
#[cfg(test)]
mod guardian_recovery_tests;
/// Owner-private Unix storage helpers shared with Mother host adapters.
pub mod private_fs;
mod segment;

pub use mct_kernel::recovery::{
    GuardianRecoveryCommandV1, GuardianRecoveryStateV1, RecoveryClockV1,
};

pub use segment::{
    SEGMENT_CHECKPOINT_PREFIX, SealedSegment, SegmentCheckpointV1, SegmentedLedgerReport,
    ledger_segments_enabled, seal_segment, verify_sealed_segment, verify_segmented_ledger,
    verify_segmented_ledger_if_enabled,
};

use mct_kernel::admin_key_grant_shape_error;
pub use mct_kernel::{
    AdminHybridSigningKey, AdminKeyGrantClassV1, AdminKeyGrantStateV1, AdminKeyGrantV1,
    AdminKeyPublicHalf, AdminKeyScopeV1, AdminSignatureError, AdminSignatureSchemeV1,
    AdminSignatureV1, AdminSignedMessageParts, MlDsa65PublicKey, admin_signed_message,
    verify_admin_hybrid_signature,
};
use mct_kernel::{
    CallId, CanonicalToyContract, MctObservation, ObservationId, ObservationKind,
    ObservationOutcome, ObservationTraceRef, ObservationVisibility, SourcePlane, Timestamp,
    ToyContractIdentity, ToyGrant, ToyGrantConstraints, ToyGrantId, ToyGrantScope, ToyGrantState,
    ToyGrantSubject, ToyId, TraceId, WatchObservationScope, WatchObservationScopeState,
};
use serde::{Deserialize, Serialize};

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

/// Bytes fed to the prefix BLAKE3 hasher while opening or resuming a ledger, by path.
///
/// The resident call path must not grow a ledger's count after that writer is open. A
/// checkpoint mismatch still full-verifies, and a broken suffix still fails closed.
static LEDGER_PREFIX_BYTES_HASHED: Mutex<BTreeMap<PathBuf, u64>> = Mutex::new(BTreeMap::new());

/// Returns prefix bytes hashed while opening or resuming `path` in this process.
pub fn ledger_prefix_bytes_hashed_for(path: &Path) -> u64 {
    LEDGER_PREFIX_BYTES_HASHED
        .lock()
        .ok()
        .and_then(|counts| counts.get(path).copied())
        .unwrap_or(0)
}

fn note_prefix_bytes_hashed(path: &Path, bytes: u64) {
    if bytes == 0 {
        return;
    }
    if let Ok(mut counts) = LEDGER_PREFIX_BYTES_HASHED.lock() {
        *counts.entry(path.to_path_buf()).or_insert(0) += bytes;
    }
}
use thiserror::Error;

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
    NoneAfterOperatorReinitialization,
    ValidatedHead { sequence: u64, entry_hash: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityStartupClassV1 {
    Virgin,
    OrdinaryReopen,
    LegacyLedgerUpgrade,
    OperatorGatedNonvirgin,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityTenureStartupEvidenceV1 {
    pub startup_class: AuthorityStartupClassV1,
    pub expected_predecessor: AuthorityEpochPredecessorV1,
    pub expected_prior_authority: Option<GrantsAuthorityIdentityV1>,
    pub expected_authority_state_hash: String,
    pub inventory_hash: String,
    pub operator_gate_decision_id: Option<String>,
    pub authenticated_principal_ref: Option<String>,
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
    /// An admin key shaped the mutation. Mother remains the only grantor.
    /// The value is the public half. Raw key material cannot be represented.
    AdminKey { public_half: AdminKeyPublicHalf },
}

/// Single-use pairing ticket. The ledger stores the hash, not the ticket.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingTicketStateV1 {
    Issued,
    Consumed,
}

/// Hash-only pairing-ticket record. A consumed hash cannot be consumed again.
///
/// Scheme v1 also records the peer endpoint and the expiry window. Pre-scheme
/// facts omit those fields. The ticket value itself is never stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingTicketV1 {
    pub ticket_hash: String,
    pub state: PairingTicketStateV1,
    /// Peer endpoint the ticket was issued for. Absent on a pre-scheme fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_endpoint_id: Option<String>,
    /// Start of the redemption window. Absent on a pre-scheme fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_starts_at: Option<Timestamp>,
    /// End of the redemption window. Absent on a pre-scheme fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_expires_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthorityChangeV1 {
    AdminKeyIssue {
        request: Box<AdminKeyIssueRequestV1>,
        decided_at: Timestamp,
    },
    GuardianRecovery {
        command: Box<GuardianRecoveryCommandV1>,
        clock: RecoveryClockV1,
    },
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
    WatchScopePut {
        scope: Box<WatchObservationScope>,
    },
    AdminKeyGrantPut {
        grant: Box<AdminKeyGrantV1>,
        /// Scheme v1 signature of this command plus the signer grant.
        /// Absent on a pre-scheme fact.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<AdminSignatureV1>,
    },
    AdminKeyGrantRevoke {
        grant_id: String,
        /// Scheme v1 signature of this command plus the signer grant.
        /// Absent on a pre-scheme fact.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<AdminSignatureV1>,
    },
    PairingTicketIssued {
        ticket_hash: String,
        /// Peer endpoint id. Required when `signature` is present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        peer_endpoint_id: Option<String>,
        /// Start of the expiry window. Required when `signature` is present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_starts_at: Option<Timestamp>,
        /// End of the expiry window. Required when `signature` is present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_expires_at: Option<Timestamp>,
        /// Scheme v1 signature of this command plus the signer grant.
        /// Absent on a pre-scheme fact.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<AdminSignatureV1>,
    },
    PairingTicketConsumed {
        ticket_hash: String,
        /// Scheme v1 signature of this command plus the signer grant.
        /// Absent on a pre-scheme fact.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<AdminSignatureV1>,
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

pub const LEGACY_AUTHORITY_IMPORT_CONFIRMATION_V1: &str =
    "import-existing-toy-authority-as-canonical-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyAuthorityImportRequestV1 {
    pub schema: String,
    pub import_id: String,
    pub expected_mother_node_id: String,
    pub expected_ledger_id: String,
    pub expected_config_authority_hash: String,
    pub expected_sqlite_authority_hash: String,
    pub confirmation: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyImportOperatorDecisionV1 {
    pub decision_id: String,
    pub authenticated_principal_ref: String,
    pub confirmation: String,
    pub decided_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyImportSourceEvidenceV1 {
    pub config_authority_hash: String,
    pub sqlite_authority_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyAuthorityImportFactV1 {
    pub import_id: String,
    pub mother_node_id: String,
    pub ledger_id: String,
    pub authority_epoch: String,
    pub operator_decision: LegacyImportOperatorDecisionV1,
    pub source_evidence: LegacyImportSourceEvidenceV1,
    pub imported_state: AuthorityStateV1,
    pub prior_state: AuthorityStateReferenceV1,
    pub resulting_state: AuthorityStateReferenceV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedLegacyAuthorityImportV1 {
    pub fact: LegacyAuthorityImportFactV1,
    pub entry_sequence: u64,
    pub entry_hash: String,
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
    #[serde(default)]
    pub watch_scopes: BTreeMap<String, WatchObservationScope>,
    #[serde(default)]
    pub admin_key_grants: BTreeMap<String, AdminKeyGrantV1>,
    #[serde(default)]
    pub pairing_tickets: BTreeMap<String, PairingTicketV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardian_recovery: Option<Box<GuardianRecoveryStateV1>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub admin_issuance: BTreeMap<String, AdminKeyIssuanceRecordV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_genesis: Option<Box<AdminGenesisFactV1>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityCanonicalFactRecordV1 {
    pub fact_id: String,
    pub fact_kind: String,
    pub source_sequence: u64,
    pub source_entry_hash: String,
    pub canonical_payload: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityProjectionStatusV1 {
    Rebuilding,
    Current,
    Stale,
    Quarantined,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityProjectionCursorV1 {
    pub schema_version: u64,
    pub projection_id: String,
    pub projection_kind: String,
    pub source_mother_node_id: String,
    pub source_ledger_id: String,
    pub through_sequence: u64,
    pub through_observation_id: String,
    pub through_entry_hash: String,
    pub grants_authority: GrantsAuthorityIdentityV1,
    pub authority_state_hash: String,
    pub projection_hash: String,
    pub projection_status: AuthorityProjectionStatusV1,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityProjectionDenyReasonV1 {
    ProjectionMissing,
    ProjectionNotCurrent,
    WrongSourceMother,
    WrongSourceLedger,
    HeadSequenceMismatch,
    HeadHashMismatch,
    AuthorityMotherMismatch,
    EpochMismatch,
    GenerationMismatch,
    SourceAuthorityObservationMismatch,
    AuthorityStateHashMismatch,
    ProjectionHashMismatch,
    LedgerQuarantined,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityProjectionExpectationV1 {
    pub source_mother_node_id: String,
    pub source_ledger_id: String,
    pub through_sequence: u64,
    pub through_entry_hash: String,
    pub grants_authority: GrantsAuthorityIdentityV1,
    pub authority_state_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthorityProjectionLedgerEvidenceV1 {
    Validated(AuthorityProjectionExpectationV1),
    Quarantined,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UsableAuthorityProjectionProofV1 {
    Usable {
        cursor: Box<AuthorityProjectionCursorV1>,
    },
    Denied {
        reason: AuthorityProjectionDenyReasonV1,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityReplayV1 {
    pub state: AuthorityStateV1,
    pub current_authority: Option<GrantsAuthorityIdentityV1>,
    pub imported: bool,
    pub import: Option<CommittedLegacyAuthorityImportV1>,
    pub mutations: BTreeMap<String, CommittedAuthorityMutationV1>,
    pub facts: Vec<AuthorityCanonicalFactRecordV1>,
    pub canonical_fact_count: usize,
    /// `(signer_grant_id, nonce)` pairs already accepted. Replay rejects a reuse.
    pub admin_signature_nonces: BTreeSet<(String, u64)>,
    /// Every historical admin grant ID, including roots retired by recovery.
    /// Reconstructed from canonical replay, never from a request/projection.
    pub admin_grant_ids: BTreeSet<String>,
    /// True while every replayed entry is a valid tail-recovery fact
    /// ([`is_valid_tail_recovery_entry`]); vacuously true for an empty ledger.
    /// It is the only non-empty prefix over which an operator-gated epoch is legal.
    pub tail_recovery_only: bool,
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
    #[error("observation ledger segment rejected: {detail}")]
    SegmentRejected { detail: String },
    #[error("segmented ledger verification is disabled")]
    SegmentsDisabled,
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
    /// Atomic file-creation evidence for this handle, not evidence of virgin host inventory.
    created_new: bool,
    ledger_id: String,
    mother_node_id: String,
    next_sequence: u64,
    previous_hash: Option<String>,
    recovery_status: Option<LedgerRecoveryStatus>,
    authority_tenure: Option<AuthorityWriterTenureV1>,
    recovery_clock_origin: std::time::Instant,
    // Durability latency is not credit toward a newly accepted 24-hour wait.
    recovery_wait_offset_ms: u64,
    #[cfg(test)]
    recovery_test_ack_delay_ms: u64,
    writer_state: LedgerWriterState,
    authority_replay: std::result::Result<AuthorityReplayV1, String>,
    used_epochs: BTreeSet<String>,
    prefix_hasher: blake3::Hasher,
    prefix_len: u64,
    last_observation_id: Option<String>,
    last_frame_len: u64,
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

    pub fn open_authority_with_startup(
        path: impl AsRef<Path>,
        ledger_id: impl Into<String>,
        mother_node_id: impl Into<String>,
        startup: AuthorityTenureStartupEvidenceV1,
    ) -> Result<Self> {
        let mut ledger = Self::open(path, ledger_id, mother_node_id)?;
        ledger.begin_authority_tenure_with_startup(startup)?;
        Ok(ledger)
    }

    pub fn open_read_only(
        path: impl AsRef<Path>,
        ledger_id: impl Into<String>,
        mother_node_id: impl Into<String>,
    ) -> Result<JsonlObservationLedgerReader> {
        JsonlObservationLedgerReader::open(path, ledger_id, mother_node_id)
    }

    /// Startup entry to the R2-L1 residue path for a writer that has not opened yet.
    ///
    /// Read-only access classifies but never recovers, so a Mother that classifies its
    /// disk before opening authority would otherwise refuse forever on crash residue.
    /// When a read-only scan finds an unterminated final frame, or a residue case whose
    /// recovery was interrupted before completion, this takes the exclusive writer lock,
    /// rescans under it, and runs the ratified recovery: exact residue bytes and the
    /// diagnostic record are preserved durably, only then are the bytes set aside, and
    /// one recovery observation is appended. The lock is released before returning so
    /// the caller can reclassify the surviving ledger. Clean, missing, non-regular,
    /// quarantined, and foreign ledgers are left untouched and return `Ok(None)`; their
    /// classification stays with the caller.
    pub fn recover_residue_for_startup(
        path: impl AsRef<Path>,
        ledger_id: impl Into<String>,
        mother_node_id: impl Into<String>,
    ) -> Result<Option<LedgerRecoveryStatus>> {
        let path = path.as_ref();
        let ledger_id = ledger_id.into();
        let mother_node_id = mother_node_id.into();
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(ObservationLedgerError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        }
        let needs_recovery = match scan_existing(path, &ledger_id, &mother_node_id)? {
            LedgerScan::Residue(_) => true,
            LedgerScan::Ready(_) => {
                pending_tail_recovery_exists(path, &ledger_id, &mother_node_id)?
            }
            LedgerScan::Quarantine(_) | LedgerScan::ForeignLineage(_) => false,
        };
        if !needs_recovery {
            return Ok(None);
        }
        let ledger = Self::open(path, ledger_id, mother_node_id)?;
        Ok(ledger.recovery_status().cloned())
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
        let replay = self.cached_replay()?.clone();
        let empty = self.next_sequence == 0;
        let predecessor = if empty {
            AuthorityEpochPredecessorV1::NoneForVirgin
        } else {
            self.head_predecessor()?
        };
        let startup_class = if empty {
            AuthorityStartupClassV1::Virgin
        } else if replay.current_authority.is_some() {
            AuthorityStartupClassV1::OrdinaryReopen
        } else {
            AuthorityStartupClassV1::LegacyLedgerUpgrade
        };
        let startup = AuthorityTenureStartupEvidenceV1 {
            startup_class,
            expected_predecessor: predecessor,
            expected_prior_authority: replay.current_authority,
            expected_authority_state_hash: authority_state_hash(&replay.state)?,
            inventory_hash: "legacy-inferred-startup-evidence".into(),
            operator_gate_decision_id: None,
            authenticated_principal_ref: None,
        };
        self.begin_authority_tenure_with_startup(startup)
    }

    pub fn begin_authority_tenure_with_startup(
        &mut self,
        startup: AuthorityTenureStartupEvidenceV1,
    ) -> Result<()> {
        if self.authority_tenure.is_some() {
            return Ok(());
        }
        let replay = self.cached_replay()?.clone();
        let empty = self.next_sequence == 0;
        let actual_predecessor = if empty {
            AuthorityEpochPredecessorV1::NoneForVirgin
        } else {
            self.head_predecessor()?
        };
        let no_operator_evidence = startup.operator_gate_decision_id.is_none()
            && startup.authenticated_principal_ref.is_none();
        let complete_operator_evidence = startup
            .operator_gate_decision_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            && startup
                .authenticated_principal_ref
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty());
        let startup_relation_valid = match startup.startup_class {
            AuthorityStartupClassV1::Virgin => {
                empty
                    && replay.current_authority.is_none()
                    && startup.expected_predecessor == AuthorityEpochPredecessorV1::NoneForVirgin
                    && no_operator_evidence
            }
            AuthorityStartupClassV1::OperatorGatedNonvirgin => {
                // Empty, or holding nothing but valid tail-recovery facts: a
                // crash on the very first write leaves only recovery evidence,
                // which is not continuity evidence for a prior history.
                (empty || replay.tail_recovery_only)
                    && replay.current_authority.is_none()
                    && startup.expected_predecessor
                        == AuthorityEpochPredecessorV1::NoneAfterOperatorReinitialization
                    && complete_operator_evidence
            }
            AuthorityStartupClassV1::LegacyLedgerUpgrade => {
                !empty
                    && replay.current_authority.is_none()
                    && startup.expected_predecessor == actual_predecessor
                    && no_operator_evidence
            }
            AuthorityStartupClassV1::OrdinaryReopen => {
                !empty
                    && replay.current_authority.is_some()
                    && startup.expected_predecessor == actual_predecessor
                    && no_operator_evidence
            }
        };
        let actual_state_hash = authority_state_hash(&replay.state)?;
        if startup.inventory_hash.trim().is_empty()
            || !startup_relation_valid
            || startup.expected_prior_authority != replay.current_authority
            || startup.expected_authority_state_hash != actual_state_hash
        {
            return Err(ObservationLedgerError::AuthorityReplay {
                detail: "explicit startup evidence does not match exclusive ledger rescan".into(),
            });
        }
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
            predecessor: startup.expected_predecessor,
            generation_baseline,
            prior_authority: replay.current_authority,
            resulting_authority,
            grant_state_hash: authority_state_hash(&replay.state)?,
            establishment: WriterTenureEstablishmentV1 {
                started_at: started_at.clone(),
                startup_class: startup.startup_class,
                operator_gate_decision_id: startup.operator_gate_decision_id,
                authenticated_principal_ref: startup.authenticated_principal_ref,
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
        self.execute_authority_mutation_inner(request, legacy_projection_write, false)
    }

    fn execute_authority_mutation_inner<F>(
        &mut self,
        request: AuthorityMutationRequestV1,
        legacy_projection_write: F,
        typed_path: bool,
    ) -> AuthorityMutationResultV1
    where
        F: FnOnce(&AuthorityStateV1) -> std::result::Result<Option<String>, String>,
    {
        let mutation_id = request.mutation_id.clone();
        if !typed_path
            && request.changes.iter().any(|change| {
                matches!(
                    change,
                    AuthorityChangeV1::GuardianRecovery { .. }
                        | AuthorityChangeV1::AdminKeyIssue { .. }
                )
            })
        {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::InvalidRequest,
            );
        }
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
        let replay = match self.cached_replay() {
            Ok(replay) => replay.clone(),
            Err(_) => {
                return rejected_mutation(
                    mutation_id,
                    AuthorityMutationRejectionReasonV1::PriorStateMismatch,
                );
            }
        };
        if !typed_path
            && (replay.state.guardian_recovery.is_some()
                || !replay.state.admin_issuance.is_empty()
                || replay.state.admin_genesis.is_some())
        {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::InvalidRequest,
            );
        }
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
        if admin_issuance::validate_context(
            &replay.admin_grant_ids,
            &request.changes,
            &prior_authority,
            &self.ledger_id,
            &request.mutation_id,
            &request.decided_at,
        )
        .is_err()
        {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::InvalidRequest,
            );
        }
        if guardian_recovery::validate_context(
            &replay.state,
            &request.changes,
            &prior_authority,
            &self.ledger_id,
            &request.decided_at,
        )
        .is_err()
        {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::InvalidRequest,
            );
        }
        if verify_admin_change_signatures(
            &replay.state,
            &replay.admin_signature_nonces,
            &self.mother_node_id,
            &self.ledger_id,
            &request.changes,
        )
        .is_err()
        {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::InvalidRequest,
            );
        }
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

    pub fn execute_legacy_authority_import(
        &mut self,
        request: LegacyAuthorityImportRequestV1,
        authenticated_principal_ref: String,
        imported_state: AuthorityStateV1,
        decided_at: String,
    ) -> AuthorityMutationResultV1 {
        let mutation_id = request.import_id.clone();
        if self.authority_tenure.is_none() {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::AuthorityEpochUnavailable,
            );
        }
        if request.schema != "mct-legacy-authority-import-request/v1"
            || request.import_id.trim().is_empty()
            || request.expected_mother_node_id != self.mother_node_id
            || request.expected_ledger_id != self.ledger_id
            || request.confirmation != LEGACY_AUTHORITY_IMPORT_CONFIRMATION_V1
            || Timestamp::new(decided_at.clone()).is_err()
        {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::InvalidRequest,
            );
        }
        let replay = match self.cached_replay() {
            Ok(replay) => replay.clone(),
            Err(_) => {
                return rejected_mutation(
                    mutation_id,
                    AuthorityMutationRejectionReasonV1::PriorStateMismatch,
                );
            }
        };
        if replay.imported {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::AlreadyImported,
            );
        }
        if !replay.mutations.is_empty() {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::PriorStateMismatch,
            );
        }
        let Some(prior_authority) = replay.current_authority else {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::AuthorityEpochUnavailable,
            );
        };
        if imported_state.admin_genesis.is_some()
            || replay.state.admin_genesis.is_some()
            || imported_state.guardian_recovery.is_some()
            || replay.state.guardian_recovery.is_some()
            || !imported_state.admin_issuance.is_empty()
            || !replay.state.admin_issuance.is_empty()
        {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::InvalidRequest,
            );
        }
        let imported_hash = match authority_state_hash(&imported_state) {
            Ok(hash) => hash,
            Err(_) => {
                return rejected_mutation(
                    mutation_id,
                    AuthorityMutationRejectionReasonV1::InvalidRequest,
                );
            }
        };
        if imported_hash != request.expected_sqlite_authority_hash {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::LegacySnapshotChanged,
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
        let Some(generation) = prior_authority.generation.checked_add(1) else {
            return rejected_mutation(
                mutation_id,
                AuthorityMutationRejectionReasonV1::InvalidRequest,
            );
        };
        let fact_id = format!("obs:legacy-authority-import:{}", request.import_id);
        let resulting_authority = GrantsAuthorityIdentityV1 {
            mother_node_id: self.mother_node_id.clone(),
            authority_epoch: prior_authority.authority_epoch.clone(),
            generation,
            source_authority_observation_id: fact_id.clone(),
        };
        let fact = LegacyAuthorityImportFactV1 {
            import_id: request.import_id,
            mother_node_id: self.mother_node_id.clone(),
            ledger_id: self.ledger_id.clone(),
            authority_epoch: prior_authority.authority_epoch.clone(),
            operator_decision: LegacyImportOperatorDecisionV1 {
                decision_id: format!("decision:{mutation_id}"),
                authenticated_principal_ref,
                confirmation: request.confirmation,
                decided_at: decided_at.clone(),
            },
            source_evidence: LegacyImportSourceEvidenceV1 {
                config_authority_hash: request.expected_config_authority_hash,
                sqlite_authority_hash: request.expected_sqlite_authority_hash,
            },
            imported_state,
            prior_state: AuthorityStateReferenceV1 {
                grants_authority: prior_authority,
                authority_state_hash: prior_hash,
            },
            resulting_state: AuthorityStateReferenceV1 {
                grants_authority: resulting_authority.clone(),
                authority_state_hash: imported_hash,
            },
        };
        let detail_ref = match encode_authority_fact("legacy_authority_import", &fact_id, &fact) {
            Ok(value) => value,
            Err(_) => {
                return rejected_mutation(
                    mutation_id,
                    AuthorityMutationRejectionReasonV1::InvalidRequest,
                );
            }
        };
        let observation = legacy_import_observation(&fact_id, &fact, detail_ref);
        let entry = match self.append_before_effect(observation, decided_at) {
            Ok(entry) => entry,
            Err(ObservationLedgerError::AppendCommitUnknown { stage, .. }) => {
                return AuthorityMutationResultV1::CommitUnknown {
                    mutation_id,
                    attempted_intent_hash: fact.resulting_state.authority_state_hash,
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
        AuthorityMutationResultV1::CommittedProjectionPending {
            mutation_id,
            resolution: AuthorityMutationResolutionV1::NewlyCommitted,
            fact_sequence: entry.local_sequence,
            fact_entry_hash: entry.entry_hash,
            grants_authority: resulting_authority,
            pending_reason: AuthorityProjectionPendingReasonV1::ProjectionNotAttempted,
        }
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

    /// Writes every frame, then acknowledges the batch with one `sync_data`.
    ///
    /// `BeforeEffectRequiresAcknowledgedCommit` is met because none of the batch is
    /// acknowledged until that single sync succeeds. A crash before the sync loses the
    /// whole batch. A failed or uncertain sync poisons the writer
    /// (`UncertainAppendPoisonsWriter`) and does not advance the in-memory chain.
    pub fn append_durable_batch(
        &mut self,
        observations: impl IntoIterator<Item = MctObservation>,
        appended_at: impl Into<String>,
        durability_class: DurabilityClass,
        export_status: ExportStatus,
    ) -> Result<Vec<MctObservationLedgerEntry>> {
        if self.is_poisoned() {
            return Err(ObservationLedgerError::WriterPoisoned {
                path: self.path.clone(),
            });
        }
        let appended_at = appended_at.into();
        let mut next_sequence = self.next_sequence;
        let mut previous_hash = self.previous_hash.clone();
        let mut prepared = Vec::new();
        for observation in observations {
            let mut entry = MctObservationLedgerEntry {
                ledger_id: self.ledger_id.clone(),
                mother_node_id: self.mother_node_id.clone(),
                local_sequence: next_sequence,
                observation,
                previous_entry_hash: previous_hash.clone(),
                entry_hash: String::new(),
                appended_at: appended_at.clone(),
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
            previous_hash = Some(entry.entry_hash.clone());
            next_sequence = next_sequence.saturating_add(1);
            prepared.push((entry, frame));
        }
        if prepared.is_empty() {
            return Ok(Vec::new());
        }
        for (_, frame) in &prepared {
            if let Err(source) = self.file.write_all(frame) {
                return Err(self.poisoned_append_error(AppendFailureStage::Write, source));
            }
        }
        if let Err(source) = self.file.sync_data() {
            return Err(self.poisoned_append_error(AppendFailureStage::Durability, source));
        }
        let mut acknowledged = Vec::with_capacity(prepared.len());
        for (entry, frame) in &prepared {
            let previous = entry
                .previous_entry_hash
                .as_ref()
                .map(|hash| (entry.local_sequence.saturating_sub(1), hash.clone()));
            self.record_committed_entry(previous, entry, frame);
            self.previous_hash = Some(entry.entry_hash.clone());
            self.next_sequence = entry.local_sequence.saturating_add(1);
            acknowledged.push(entry.clone());
        }
        Ok(acknowledged)
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
        let previous = self.previous_link();
        self.append_frame(&frame)?;
        self.record_committed_entry(previous, &entry, &frame);

        self.previous_hash = Some(entry.entry_hash.clone());
        self.next_sequence += 1;
        Ok(entry)
    }

    fn cached_replay(&self) -> Result<&AuthorityReplayV1> {
        self.authority_replay
            .as_ref()
            .map_err(|detail| ObservationLedgerError::AuthorityReplay {
                detail: detail.clone(),
            })
    }

    fn head_predecessor(&self) -> Result<AuthorityEpochPredecessorV1> {
        let sequence = self.next_sequence.checked_sub(1).ok_or_else(|| {
            ObservationLedgerError::AuthorityReplay {
                detail: "ledger head is missing".into(),
            }
        })?;
        let entry_hash =
            self.previous_hash
                .clone()
                .ok_or_else(|| ObservationLedgerError::AuthorityReplay {
                    detail: "ledger head hash is missing".into(),
                })?;
        Ok(AuthorityEpochPredecessorV1::ValidatedHead {
            sequence,
            entry_hash,
        })
    }

    fn previous_link(&self) -> Option<(u64, String)> {
        let sequence = self.next_sequence.checked_sub(1)?;
        self.previous_hash.clone().map(|hash| (sequence, hash))
    }

    fn record_committed_entry(
        &mut self,
        previous: Option<(u64, String)>,
        entry: &MctObservationLedgerEntry,
        frame: &[u8],
    ) {
        self.prefix_hasher.update(frame);
        self.prefix_len += frame.len() as u64;
        self.last_frame_len = frame.len() as u64;
        self.last_observation_id = Some(entry.observation.observation_id.to_string());
        if let Ok(replay) = self.authority_replay.as_mut()
            && let Err(error) = fold_authority_entry(
                replay,
                &mut self.used_epochs,
                previous
                    .as_ref()
                    .map(|(sequence, hash)| (*sequence, hash.as_str())),
                entry,
            )
        {
            self.authority_replay = Err(error.to_string());
        }
    }

    /// Authority replay captured while the chain was verified.
    ///
    /// Later authority mutations update this cache in place. They do not reread
    /// the prefix of the log.
    pub fn verified_replay(&self) -> Result<&AuthorityReplayV1> {
        self.cached_replay()
    }

    /// Validated head of the open writer, without rereading the log.
    pub fn verified_head(&self) -> Result<LedgerVerifiedHead> {
        let AuthorityEpochPredecessorV1::ValidatedHead {
            sequence,
            entry_hash,
        } = self.head_predecessor()?
        else {
            return Err(ObservationLedgerError::AuthorityReplay {
                detail: "ledger head is missing".into(),
            });
        };
        let observation_id = self.last_observation_id.clone().ok_or_else(|| {
            ObservationLedgerError::AuthorityReplay {
                detail: "ledger head observation is missing".into(),
            }
        })?;
        Ok(LedgerVerifiedHead {
            local_sequence: sequence,
            entry_hash,
            observation_id,
            ledger_id: self.ledger_id.clone(),
            mother_node_id: self.mother_node_id.clone(),
        })
    }

    /// Checkpoint a caller can use to verify only the suffix after this head.
    pub fn replay_checkpoint(&self) -> Result<LedgerReplayCheckpoint> {
        let head = self.verified_head()?;
        Ok(LedgerReplayCheckpoint {
            through_sequence: head.local_sequence,
            through_entry_hash: head.entry_hash,
            prefix_len: self.prefix_len,
            prefix_blake3: self.prefix_hasher.clone().finalize().to_hex().to_string(),
            checkpoint_frame_len: self.last_frame_len,
            replay: self.cached_replay()?.clone(),
            used_epochs: self.used_epochs.clone(),
        })
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

struct PrefixHasher(blake3::Hasher);

impl std::fmt::Debug for PrefixHasher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PrefixHasher")
    }
}

impl PrefixHasher {
    fn new() -> Self {
        Self(blake3::Hasher::new())
    }

    fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
}

#[derive(Debug)]
struct LedgerScanState {
    next_sequence: u64,
    previous_hash: Option<String>,
    committed_len: u64,
    recovery_observation_ids: BTreeSet<String>,
    authority_replay: std::result::Result<AuthorityReplayV1, String>,
    used_epochs: BTreeSet<String>,
    prefix_hasher: PrefixHasher,
    last_observation_id: Option<String>,
    last_frame_len: u64,
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
        || request
            .grant_shaping_sources
            .iter()
            .any(|source| !grant_shaping_source_is_complete(source))
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
        AuthorityChangeV1::GuardianRecovery { .. } => ("guardian_recovery", ""),
        AuthorityChangeV1::AdminKeyIssue { request, .. } => {
            ("admin_key_issue", &request.intent.grant.grant_id)
        }
        AuthorityChangeV1::ToyCatalogPut { toy_id, .. } => ("toy_catalog_put", toy_id),
        AuthorityChangeV1::ToyCatalogRemove { toy_id } => ("toy_catalog_remove", toy_id),
        AuthorityChangeV1::ToyGrantPut { grant_id, .. } => ("toy_grant_put", grant_id),
        AuthorityChangeV1::ToyGrantRemove { grant_id } => ("toy_grant_remove", grant_id),
        AuthorityChangeV1::WatchScopePut { scope } => {
            ("watch_scope_put", scope.watch_scope_id.as_str())
        }
        AuthorityChangeV1::AdminKeyGrantPut { grant, .. } => {
            ("admin_key_grant_put", grant.grant_id.as_str())
        }
        AuthorityChangeV1::AdminKeyGrantRevoke { grant_id, .. } => {
            ("admin_key_grant_revoke", grant_id.as_str())
        }
        AuthorityChangeV1::PairingTicketIssued { ticket_hash, .. } => {
            ("pairing_ticket_issued", ticket_hash.as_str())
        }
        AuthorityChangeV1::PairingTicketConsumed { ticket_hash, .. } => {
            ("pairing_ticket_consumed", ticket_hash.as_str())
        }
    }
}

fn grant_shaping_source_is_complete(source: &GrantShapingSourceV1) -> bool {
    match source {
        GrantShapingSourceV1::AdminKey { public_half } => {
            public_half.scheme() == mct_kernel::AdminKeySchemeV1::Ed25519
        }
        GrantShapingSourceV1::OperatorDecision { .. }
        | GrantShapingSourceV1::ChildApproval { .. }
        | GrantShapingSourceV1::ChildAssignment { .. } => true,
    }
}

fn non_empty_token(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_whitespace)
}

fn pairing_ticket_hash_ok(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn validate_admin_key_grant_shape(grant: &AdminKeyGrantV1) -> std::result::Result<(), String> {
    match admin_key_grant_shape_error(grant) {
        Some(detail) => Err(detail.to_owned()),
        None => Ok(()),
    }
}

fn authority_incoherent(detail: impl Into<String>) -> AuthorityReplayError {
    AuthorityReplayError::Incoherent {
        sequence: 0,
        detail: detail.into(),
    }
}

fn apply_admin_key_grant_put(
    state: &mut AuthorityStateV1,
    grant: &AdminKeyGrantV1,
) -> std::result::Result<(), AuthorityReplayError> {
    validate_admin_key_grant_shape(grant).map_err(authority_incoherent)?;
    if grant.grant_state != AdminKeyGrantStateV1::Active {
        return Err(authority_incoherent(
            "admin key grant put must be active; a revoke is not a put",
        ));
    }
    if let Some(existing) = state.admin_key_grants.get(&grant.grant_id) {
        if existing.class == AdminKeyGrantClassV1::Recovery
            || grant.class == AdminKeyGrantClassV1::Recovery
        {
            return Err(authority_incoherent(
                "recovery grant cannot be revoked or replaced",
            ));
        }
        return Err(authority_incoherent(
            "admin key grant id cannot be put again",
        ));
    }
    if grant.class == AdminKeyGrantClassV1::Recovery
        && state
            .admin_key_grants
            .values()
            .any(|existing| existing.class == AdminKeyGrantClassV1::Recovery)
    {
        return Err(authority_incoherent(
            "a second recovery grant is not admitted",
        ));
    }
    state
        .admin_key_grants
        .insert(grant.grant_id.clone(), grant.clone());
    Ok(())
}

fn apply_admin_key_grant_revoke(
    state: &mut AuthorityStateV1,
    grant_id: &str,
) -> std::result::Result<(), AuthorityReplayError> {
    if !non_empty_token(grant_id) {
        return Err(authority_incoherent("admin key grant id is empty"));
    }
    let Some(existing) = state.admin_key_grants.get(grant_id) else {
        return Err(authority_incoherent(format!(
            "cannot revoke missing admin key grant '{grant_id}'"
        )));
    };
    if existing.class == AdminKeyGrantClassV1::Recovery {
        return Err(authority_incoherent("recovery grant cannot be revoked"));
    }
    if existing.grant_state != AdminKeyGrantStateV1::Active {
        return Err(authority_incoherent(
            "revoked admin key grant cannot be revoked again",
        ));
    }
    let mut revoked = existing.clone();
    revoked.grant_state = AdminKeyGrantStateV1::Revoked;
    state.admin_key_grants.insert(grant_id.to_owned(), revoked);
    Ok(())
}

fn apply_pairing_ticket_issued(
    state: &mut AuthorityStateV1,
    ticket_hash: &str,
    peer_endpoint_id: Option<&str>,
    window_starts_at: Option<&Timestamp>,
    window_expires_at: Option<&Timestamp>,
    signed: bool,
) -> std::result::Result<(), AuthorityReplayError> {
    if !pairing_ticket_hash_ok(ticket_hash) {
        return Err(authority_incoherent(
            "pairing ticket fact stores a hash only",
        ));
    }
    if signed {
        let endpoint = peer_endpoint_id.unwrap_or("");
        if !non_empty_token(endpoint) {
            return Err(authority_incoherent(
                "pairing ticket carries the peer endpoint id",
            ));
        }
        match (window_starts_at, window_expires_at) {
            (Some(starts), Some(expires)) if expires > starts => {}
            _ => {
                return Err(authority_incoherent(
                    "pairing ticket carries an expiry window",
                ));
            }
        }
    }
    if state.pairing_tickets.contains_key(ticket_hash) {
        return Err(authority_incoherent(
            "pairing ticket hash is already recorded",
        ));
    }
    state.pairing_tickets.insert(
        ticket_hash.to_owned(),
        PairingTicketV1 {
            ticket_hash: ticket_hash.to_owned(),
            state: PairingTicketStateV1::Issued,
            peer_endpoint_id: peer_endpoint_id.map(str::to_owned),
            window_starts_at: window_starts_at.cloned(),
            window_expires_at: window_expires_at.cloned(),
        },
    );
    Ok(())
}

fn apply_pairing_ticket_consumed(
    state: &mut AuthorityStateV1,
    ticket_hash: &str,
) -> std::result::Result<(), AuthorityReplayError> {
    if !pairing_ticket_hash_ok(ticket_hash) {
        return Err(authority_incoherent(
            "pairing ticket fact stores a hash only",
        ));
    }
    match state
        .pairing_tickets
        .get(ticket_hash)
        .map(|ticket| ticket.state)
    {
        None => Err(authority_incoherent("pairing ticket hash was not issued")),
        Some(PairingTicketStateV1::Consumed) => Err(authority_incoherent(
            "pairing ticket hash was already consumed",
        )),
        Some(PairingTicketStateV1::Issued) => {
            let mut consumed = state
                .pairing_tickets
                .get(ticket_hash)
                .expect("issued ticket")
                .clone();
            consumed.state = PairingTicketStateV1::Consumed;
            state
                .pairing_tickets
                .insert(ticket_hash.to_owned(), consumed);
            Ok(())
        }
    }
}

pub fn apply_authority_changes(
    state: &mut AuthorityStateV1,
    changes: &[AuthorityChangeV1],
) -> std::result::Result<(), AuthorityReplayError> {
    if changes
        .iter()
        .any(|change| matches!(change, AuthorityChangeV1::AdminKeyIssue { .. }))
    {
        let [
            AuthorityChangeV1::AdminKeyIssue {
                request,
                decided_at,
            },
        ] = changes
        else {
            return Err(authority_incoherent(
                "issuance must be an exclusive authority transition",
            ));
        };
        return admin_issuance::apply(state, request, decided_at);
    }
    if changes
        .iter()
        .any(|change| matches!(change, AuthorityChangeV1::GuardianRecovery { .. }))
    {
        let [AuthorityChangeV1::GuardianRecovery { command, clock }] = changes else {
            return Err(authority_incoherent(
                "recovery must be an exclusive authority transition",
            ));
        };
        return guardian_recovery::apply(state, command, clock);
    }
    if state.guardian_recovery.is_some()
        || !state.admin_issuance.is_empty()
        || state.admin_genesis.is_some()
    {
        return Err(authority_incoherent(
            "enrolled recovery authority requires the new runtime path",
        ));
    }
    for change in changes {
        match change {
            AuthorityChangeV1::GuardianRecovery { .. }
            | AuthorityChangeV1::AdminKeyIssue { .. } => {
                unreachable!("exclusive typed transition handled above")
            }
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
            AuthorityChangeV1::WatchScopePut { scope } => {
                scope
                    .validate()
                    .map_err(|error| AuthorityReplayError::Incoherent {
                        sequence: 0,
                        detail: format!("invalid Watch scope: {error}"),
                    })?;
                let scope_id = scope.watch_scope_id.to_string();
                match state.watch_scopes.get(&scope_id) {
                    None => {
                        if scope.scope_revision.get() != 1
                            || scope.authority_state != WatchObservationScopeState::Active
                        {
                            return Err(AuthorityReplayError::Incoherent {
                                sequence: 0,
                                detail: "first Watch scope put must be active revision one".into(),
                            });
                        }
                    }
                    Some(current) => {
                        if current.authority_state != WatchObservationScopeState::Active {
                            return Err(AuthorityReplayError::Incoherent {
                                sequence: 0,
                                detail: "terminal Watch scope cannot be revised or resurrected"
                                    .into(),
                            });
                        }
                        if current.scope_revision.get().checked_add(1)
                            != Some(scope.scope_revision.get())
                        {
                            return Err(AuthorityReplayError::Incoherent {
                                sequence: 0,
                                detail: "Watch scope revision is not exactly prior plus one".into(),
                            });
                        }
                    }
                }
                state.watch_scopes.insert(scope_id, scope.as_ref().clone());
            }
            AuthorityChangeV1::AdminKeyGrantPut { grant, signature } => {
                if grant.issuance_ceiling.is_some() {
                    return Err(authority_incoherent(
                        "legacy put cannot introduce an issuance ceiling",
                    ));
                }
                if signature.is_some() && grant.ml_dsa65_public_key.is_none() {
                    return Err(authority_incoherent(
                        "scheme v1 grant put records the ML-DSA-65 public key beside Ed25519",
                    ));
                }
                apply_admin_key_grant_put(state, grant)?;
            }
            AuthorityChangeV1::AdminKeyGrantRevoke { grant_id, .. } => {
                apply_admin_key_grant_revoke(state, grant_id)?;
            }
            AuthorityChangeV1::PairingTicketIssued {
                ticket_hash,
                peer_endpoint_id,
                window_starts_at,
                window_expires_at,
                signature,
            } => {
                apply_pairing_ticket_issued(
                    state,
                    ticket_hash,
                    peer_endpoint_id.as_deref(),
                    window_starts_at.as_ref(),
                    window_expires_at.as_ref(),
                    signature.is_some(),
                )?;
            }
            AuthorityChangeV1::PairingTicketConsumed { ticket_hash, .. } => {
                apply_pairing_ticket_consumed(state, ticket_hash)?;
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

fn legacy_import_observation(
    fact_id: &str,
    fact: &LegacyAuthorityImportFactV1,
    detail_ref: String,
) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::new(fact_id).expect("import fact id is non-empty"),
        observed_at: Timestamp::new(fact.operator_decision.decided_at.clone())
            .expect("validated import decision time"),
        kind: ObservationKind::OperatorActionRecorded,
        source_plane: SourcePlane::Operator,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace:legacy-authority-import:{}", fact.import_id))
                .expect("import trace is non-empty"),
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
        safe_message: "legacy Toy authority imported as canonical".into(),
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

fn validate_watch_scope_state(state: &AuthorityStateV1) -> std::result::Result<(), String> {
    for (scope_id, scope) in &state.watch_scopes {
        if scope_id != scope.watch_scope_id.as_str() {
            return Err("Watch scope map key does not match complete value identity".into());
        }
        scope
            .validate()
            .map_err(|error| format!("invalid Watch scope authority value: {error}"))?;
    }
    Ok(())
}

fn validate_admin_authority_state(state: &AuthorityStateV1) -> std::result::Result<(), String> {
    let mut recovery_grants = 0_usize;
    for (grant_id, grant) in &state.admin_key_grants {
        if grant_id != &grant.grant_id {
            return Err("admin key grant map key does not match grant id".into());
        }
        validate_admin_key_grant_shape(grant)?;
        if grant.class == AdminKeyGrantClassV1::Recovery {
            recovery_grants += 1;
            if grant.grant_state != AdminKeyGrantStateV1::Active {
                return Err("recovery grant cannot be revoked".into());
            }
        }
    }
    if recovery_grants > 1 {
        return Err("a second recovery grant is not admitted".into());
    }
    for (ticket_hash, ticket) in &state.pairing_tickets {
        if ticket_hash != &ticket.ticket_hash || !pairing_ticket_hash_ok(ticket_hash) {
            return Err("pairing ticket map key does not match its hash".into());
        }
    }
    Ok(())
}

pub fn authority_state_hash(state: &AuthorityStateV1) -> Result<String> {
    validate_watch_scope_state(state)
        .map_err(|detail| ObservationLedgerError::AuthorityReplay { detail })?;
    validate_admin_authority_state(state)
        .map_err(|detail| ObservationLedgerError::AuthorityReplay { detail })?;
    guardian_recovery::validate_state(state)
        .map_err(|detail| ObservationLedgerError::AuthorityReplay { detail })?;
    admin_issuance::validate_state(state)
        .map_err(|detail| ObservationLedgerError::AuthorityReplay { detail })?;
    admin_genesis::validate_state(state)?;
    #[derive(Serialize)]
    struct HashableAuthorityState<'a> {
        toy_catalog: Vec<&'a CanonicalToyContract>,
        toy_grants: Vec<&'a ToyGrant>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        watch_scopes: Vec<&'a WatchObservationScope>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        admin_key_grants: Vec<&'a AdminKeyGrantV1>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pairing_tickets: Vec<&'a PairingTicketV1>,
    }
    let hashable = HashableAuthorityState {
        toy_catalog: state.toy_catalog.values().collect(),
        toy_grants: state.toy_grants.values().collect(),
        watch_scopes: state.watch_scopes.values().collect(),
        admin_key_grants: state.admin_key_grants.values().collect(),
        pairing_tickets: state.pairing_tickets.values().collect(),
    };
    if state.admin_genesis.is_some() {
        let bytes = canonical_json_bytes(&(
            "mct-strict-admin-authority-state/v1",
            &hashable,
            &state.guardian_recovery,
            &state.admin_issuance,
            &state.admin_genesis,
        ))?;
        return Ok(blake3::hash(&bytes).to_hex().to_string());
    }
    if !state.admin_issuance.is_empty() {
        let bytes = canonical_json_bytes(&(
            "mct-admin-issuance-authority-state/v1",
            &hashable,
            &state.guardian_recovery,
            &state.admin_issuance,
        ))?;
        return Ok(blake3::hash(&bytes).to_hex().to_string());
    }
    let bytes = match &state.guardian_recovery {
        None => canonical_json_bytes(&hashable)?,
        Some(recovery) => {
            canonical_json_bytes(&("mct-guardian-authority-state/v1", &hashable, recovery))?
        }
    };
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityProjectionHashInputV1 {
    pub source_mother_node_id: String,
    pub source_ledger_id: String,
    pub through_sequence: u64,
    pub through_observation_id: String,
    pub through_entry_hash: String,
    pub grants_authority: GrantsAuthorityIdentityV1,
    pub authority_state_hash: String,
    pub projection_status: AuthorityProjectionStatusV1,
}

pub fn authority_projection_hash(input: &AuthorityProjectionHashInputV1) -> Result<String> {
    #[derive(Serialize)]
    struct VersionedProjectionHashInput<'a> {
        schema_version: u64,
        input: &'a AuthorityProjectionHashInputV1,
    }
    Ok(
        blake3::hash(&canonical_json_bytes(&VersionedProjectionHashInput {
            schema_version: 1,
            input,
        })?)
        .to_hex()
        .to_string(),
    )
}

/// True when `entry` is a ledger-tail recovery fact: the observation the
/// ratified residue path appends after preserving an unterminated final frame.
/// Such a fact records crash residue handling; it is never continuity
/// evidence for a prior canonical history.
pub fn is_tail_recovery_entry(entry: &MctObservationLedgerEntry) -> bool {
    let observation = &entry.observation;
    observation
        .observation_id
        .as_str()
        .starts_with("ledger-tail-recovery-")
        && observation.kind == ObservationKind::StorageAppendSucceeded
        && observation.source_plane == SourcePlane::Storage
        && observation
            .detail_ref
            .as_deref()
            .is_some_and(|detail| detail.starts_with("ledger-tail-recovery-v1:"))
}

const TAIL_RECOVERY_SAFE_MESSAGE: &str = "observation ledger tail recovered";

/// Residue offset and digest named by a tail-recovery observation id
/// `ledger-tail-recovery-{offset}-{digest}`, in canonical form only.
fn tail_recovery_identity(entry: &MctObservationLedgerEntry) -> Option<(u64, &str)> {
    let rest = entry
        .observation
        .observation_id
        .as_str()
        .strip_prefix("ledger-tail-recovery-")?;
    let (offset_text, digest) = rest.split_once('-')?;
    let offset: u64 = offset_text.parse().ok()?;
    let canonical_digest = digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    (offset.to_string() == offset_text && canonical_digest).then_some((offset, digest))
}

/// True when `entry` is exactly the observation the ratified residue path
/// appends (see `append_recovery_observation`): every carrier field matches,
/// and the id and detail name the same residue digest. A recovery-looking
/// entry that differs in any field is not a recovery fact.
pub fn is_valid_tail_recovery_entry(entry: &MctObservationLedgerEntry) -> bool {
    let Some((_, digest)) = tail_recovery_identity(entry) else {
        return false;
    };
    let observation = &entry.observation;
    let id = observation.observation_id.as_str();
    is_tail_recovery_entry(entry)
        && observation.detail_ref.as_deref()
            == Some(format!("ledger-tail-recovery-v1:{digest}").as_str())
        && observation.trace.trace_id.as_str() == format!("trace-{id}")
        && observation.trace.span_id.is_none()
        && observation.trace.parent_span_id.is_none()
        && observation.trace.external_trace_id.is_none()
        && observation.call_id.is_none()
        && observation.decision_id.is_none()
        && observation.subject_id.as_deref() == Some(entry.mother_node_id.as_str())
        && observation.resource_id.as_deref() == Some(entry.ledger_id.as_str())
        && observation.policy_revision.is_none()
        && observation.grants_revision.is_none()
        && observation.outcome == ObservationOutcome::Completed
        && observation.visibility == ObservationVisibility::NodeOperator
        && observation.safe_message == TAIL_RECOVERY_SAFE_MESSAGE
        && observation.observed_at.as_str() == entry.appended_at
        && entry.durability_class == DurabilityClass::BeforeEffect
        && entry.export_status == ExportStatus::NotRequired
}

/// True when the private forensics for a valid tail-recovery fact are intact:
/// the completed diagnostic record for that residue case names this ledger,
/// and the preserved residue bytes still hash to the recorded digest.
///
/// # Errors
///
/// Returns an error only for an unreadable forensic directory; a missing or
/// mismatched case is `Ok(false)`.
pub fn tail_recovery_forensics_intact(
    ledger_path: &Path,
    ledger_id: &str,
    mother_node_id: &str,
    entry: &MctObservationLedgerEntry,
) -> Result<bool> {
    if !is_valid_tail_recovery_entry(entry)
        || entry.ledger_id != ledger_id
        || entry.mother_node_id != mother_node_id
    {
        return Ok(false);
    }
    let Some((offset, digest)) = tail_recovery_identity(entry) else {
        return Ok(false);
    };
    let case_dir = forensic_root_path(ledger_path).join(format!("tail-{offset}-{digest}"));
    let record_path = case_dir.join("record.json");
    let source_path = case_dir.join("source.bin");
    if !record_path.is_file() || !source_path.is_file() {
        return Ok(false);
    }
    let Ok(status) = read_recovery_status(&record_path) else {
        return Ok(false);
    };
    let bytes = std::fs::read(&source_path).map_err(|source| ObservationLedgerError::Io {
        path: source_path.clone(),
        source,
    })?;
    Ok(status.recovery_complete
        && status.ledger_path == ledger_path
        && status.source_ledger_id == ledger_id
        && status.source_mother_node_id == mother_node_id
        && status.residue_offset == offset
        && status.residue_digest == digest
        && status.recovery_decision_id == entry.observation.observation_id.as_str()
        && status.recovery_time == entry.appended_at
        && status.preserved_bytes_path == source_path
        && status.residue_length == bytes.len() as u64
        && blake3::hash(&bytes).to_hex().as_str() == digest)
}

pub fn replay_authority_entries(
    entries: &[MctObservationLedgerEntry],
) -> std::result::Result<AuthorityReplayV1, AuthorityReplayError> {
    let mut replay = empty_authority_replay();
    let mut used_epochs = BTreeSet::new();
    let mut previous: Option<(u64, String)> = None;
    for entry in entries {
        fold_authority_entry(
            &mut replay,
            &mut used_epochs,
            previous
                .as_ref()
                .map(|(sequence, hash)| (*sequence, hash.as_str())),
            entry,
        )?;
        previous = Some((entry.local_sequence, entry.entry_hash.clone()));
    }
    Ok(replay)
}

fn empty_authority_replay() -> AuthorityReplayV1 {
    AuthorityReplayV1 {
        state: AuthorityStateV1::default(),
        current_authority: None,
        imported: false,
        import: None,
        mutations: BTreeMap::new(),
        facts: Vec::new(),
        canonical_fact_count: 0,
        admin_signature_nonces: BTreeSet::new(),
        admin_grant_ids: BTreeSet::new(),
        tail_recovery_only: true,
    }
}

fn change_signature(change: &AuthorityChangeV1) -> Option<&AdminSignatureV1> {
    match change {
        AuthorityChangeV1::AdminKeyIssue { request, .. } => Some(&request.signature),
        AuthorityChangeV1::AdminKeyGrantPut { signature, .. }
        | AuthorityChangeV1::AdminKeyGrantRevoke { signature, .. }
        | AuthorityChangeV1::PairingTicketIssued { signature, .. }
        | AuthorityChangeV1::PairingTicketConsumed { signature, .. } => signature.as_ref(),
        _ => None,
    }
}

fn unsigned_admin_command_bytes(change: &AuthorityChangeV1) -> Result<Vec<u8>> {
    match change {
        AuthorityChangeV1::AdminKeyIssue { request, .. } => admin_issuance::command_bytes(request),
        _ => canonical_json_bytes(&command_without_signature(change)),
    }
}

fn command_without_signature(change: &AuthorityChangeV1) -> AuthorityChangeV1 {
    let mut stripped = change.clone();
    match &mut stripped {
        AuthorityChangeV1::AdminKeyGrantPut { signature, .. }
        | AuthorityChangeV1::AdminKeyGrantRevoke { signature, .. }
        | AuthorityChangeV1::PairingTicketIssued { signature, .. }
        | AuthorityChangeV1::PairingTicketConsumed { signature, .. } => *signature = None,
        _ => {}
    }
    stripped
}

fn signer_grant<'a>(
    prior: &'a AuthorityStateV1,
    change: &'a AuthorityChangeV1,
    signer_grant_id: &str,
) -> std::result::Result<&'a AdminKeyGrantV1, String> {
    if let AuthorityChangeV1::AdminKeyGrantPut { grant, .. } = change
        && grant.grant_id == signer_grant_id
    {
        return Ok(grant);
    }
    match prior.admin_key_grants.get(signer_grant_id) {
        Some(grant) if grant.grant_state == AdminKeyGrantStateV1::Active => Ok(grant),
        Some(_) => Err(format!(
            "admin signature signer grant '{signer_grant_id}' is not active"
        )),
        None => Err(format!(
            "admin signature signer grant '{signer_grant_id}' is not in the ledger"
        )),
    }
}

/// Canonical bytes a scheme v1 admin signature covers: the command plus the grant.
///
/// # Errors
///
/// Returns an error when the grant or the command cannot be encoded.
pub fn admin_change_signing_message(
    mother_node_id: &str,
    ledger_id: &str,
    nonce: u64,
    signer_grant: &AdminKeyGrantV1,
    change: &AuthorityChangeV1,
) -> Result<Vec<u8>> {
    let grant_bytes = canonical_json_bytes(signer_grant)?;
    let command_bytes = unsigned_admin_command_bytes(change)?;
    Ok(admin_signed_message(AdminSignedMessageParts {
        scheme: AdminSignatureSchemeV1::Ed25519MlDsa65V1,
        mother_node_id,
        ledger_id,
        nonce,
        signer_grant_canonical: &grant_bytes,
        command_canonical: &command_bytes,
    }))
}

fn verify_admin_change_signatures(
    prior: &AuthorityStateV1,
    used_nonces: &BTreeSet<(String, u64)>,
    mother_node_id: &str,
    ledger_id: &str,
    changes: &[AuthorityChangeV1],
) -> std::result::Result<(), String> {
    let mut seen = BTreeSet::new();
    for change in changes {
        let Some(signature) = change_signature(change) else {
            continue;
        };
        if matches!(change, AuthorityChangeV1::AdminKeyIssue { .. })
            && signature.signature.len() != mct_kernel::ADMIN_HYBRID_SIGNATURE_LEN * 2
        {
            return Err("issuance signature has invalid encoded length".into());
        }
        let nonce_key = (signature.signer_grant_id.clone(), signature.nonce);
        if used_nonces.contains(&nonce_key) || !seen.insert(nonce_key) {
            return Err(format!(
                "admin signature nonce {} was already used for grant '{}'",
                signature.nonce, signature.signer_grant_id
            ));
        }
        let grant = signer_grant(prior, change, &signature.signer_grant_id)?;
        let Some(ml_dsa) = grant.ml_dsa65_public_key.as_ref() else {
            return Err(
                "scheme v1 signature requires the ML-DSA-65 public key on the signer grant".into(),
            );
        };
        let grant_bytes = canonical_json_bytes(grant).map_err(|error| error.to_string())?;
        let command_bytes =
            unsigned_admin_command_bytes(change).map_err(|error| error.to_string())?;
        let message = admin_signed_message(AdminSignedMessageParts {
            scheme: signature.scheme,
            mother_node_id,
            ledger_id,
            nonce: signature.nonce,
            signer_grant_canonical: &grant_bytes,
            command_canonical: &command_bytes,
        });
        verify_admin_hybrid_signature(&grant.public_half, ml_dsa, &message, &signature.signature)
            .map_err(|error: AdminSignatureError| error.to_string())?;
    }
    Ok(())
}

fn record_admin_signature_nonces(
    used: &mut BTreeSet<(String, u64)>,
    changes: &[AuthorityChangeV1],
) {
    for change in changes {
        if let Some(signature) = change_signature(change) {
            used.insert((signature.signer_grant_id.clone(), signature.nonce));
        }
    }
}

fn fold_authority_entry(
    replay: &mut AuthorityReplayV1,
    used_epochs: &mut BTreeSet<String>,
    previous: Option<(u64, &str)>,
    entry: &MctObservationLedgerEntry,
) -> std::result::Result<(), AuthorityReplayError> {
    fold_authority_entry_body(replay, used_epochs, previous, entry)?;
    // Updated after the entry is validated, so the entry is judged against
    // the prefix before it.
    replay.tail_recovery_only = replay.tail_recovery_only && is_valid_tail_recovery_entry(entry);
    replay
        .admin_grant_ids
        .extend(replay.state.admin_key_grants.keys().cloned());
    Ok(())
}

fn fold_authority_entry_body(
    replay: &mut AuthorityReplayV1,
    used_epochs: &mut BTreeSet<String>,
    previous: Option<(u64, &str)>,
    entry: &MctObservationLedgerEntry,
) -> std::result::Result<(), AuthorityReplayError> {
    let Some(detail) = entry.observation.detail_ref.as_deref() else {
        return Ok(());
    };
    let Some(payload) = detail.strip_prefix(AUTHORITY_FACT_DETAIL_PREFIX) else {
        return Ok(());
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
    let fact_kind = envelope.fact_kind.clone();
    let fact_id = envelope.fact_id.clone();
    match fact_kind.as_str() {
        "admin_genesis" => {
            let fact: AdminGenesisFactV1 =
                serde_json::from_value(envelope.body).map_err(|error| {
                    AuthorityReplayError::Malformed {
                        sequence: entry.local_sequence,
                        detail: error.to_string(),
                    }
                })?;
            admin_genesis::replay_genesis(replay, used_epochs, previous, entry, fact)?;
        }
        "epoch_established" => {
            let fact: EpochEstablishedFactV1 =
                serde_json::from_value(envelope.body).map_err(|error| {
                    AuthorityReplayError::Malformed {
                        sequence: entry.local_sequence,
                        detail: error.to_string(),
                    }
                })?;
            validate_epoch_fact(previous, entry, replay, used_epochs, &fact)?;
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
            let resulting_state = validate_authority_mutation_fact(entry, replay, &fact)?;
            record_admin_signature_nonces(&mut replay.admin_signature_nonces, &fact.changes);
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
        "legacy_authority_import" => {
            let fact: LegacyAuthorityImportFactV1 =
                serde_json::from_value(envelope.body).map_err(|error| {
                    AuthorityReplayError::Malformed {
                        sequence: entry.local_sequence,
                        detail: error.to_string(),
                    }
                })?;
            validate_legacy_import_fact(entry, replay, &fact)?;
            if replay.imported || !replay.mutations.is_empty() {
                return Err(AuthorityReplayError::Incoherent {
                    sequence: entry.local_sequence,
                    detail: "legacy authority import is not one-time or precedes mutations".into(),
                });
            }
            replay.state = fact.imported_state.clone();
            replay.current_authority = Some(fact.resulting_state.grants_authority.clone());
            replay.imported = true;
            replay.import = Some(CommittedLegacyAuthorityImportV1 {
                fact,
                entry_sequence: entry.local_sequence,
                entry_hash: entry.entry_hash.clone(),
            });
            replay.canonical_fact_count += 1;
        }
        other => {
            return Err(AuthorityReplayError::UnknownFactKind {
                sequence: entry.local_sequence,
                fact_kind: other.to_owned(),
            });
        }
    }
    replay.facts.push(AuthorityCanonicalFactRecordV1 {
        fact_id,
        fact_kind,
        source_sequence: entry.local_sequence,
        source_entry_hash: entry.entry_hash.clone(),
        canonical_payload: payload.to_owned(),
    });
    Ok(())
}

fn validate_epoch_fact(
    previous: Option<(u64, &str)>,
    entry: &MctObservationLedgerEntry,
    replay: &AuthorityReplayV1,
    used_epochs: &BTreeSet<String>,
    fact: &EpochEstablishedFactV1,
) -> std::result::Result<(), AuthorityReplayError> {
    let incoherent = |detail: &str| AuthorityReplayError::Incoherent {
        sequence: entry.local_sequence,
        detail: detail.to_owned(),
    };
    let validated_head =
        previous.map(
            |(sequence, entry_hash)| AuthorityEpochPredecessorV1::ValidatedHead {
                sequence,
                entry_hash: entry_hash.to_owned(),
            },
        );
    let no_operator_evidence = fact.establishment.operator_gate_decision_id.is_none()
        && fact.establishment.authenticated_principal_ref.is_none();
    let complete_operator_evidence = fact
        .establishment
        .operator_gate_decision_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && fact
            .establishment
            .authenticated_principal_ref
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
    let startup_relation_valid = match (
        replay.current_authority.as_ref(),
        validated_head.as_ref(),
        fact.establishment.startup_class,
    ) {
        (None, None, AuthorityStartupClassV1::Virgin) => {
            fact.predecessor == AuthorityEpochPredecessorV1::NoneForVirgin && no_operator_evidence
        }
        (None, None, AuthorityStartupClassV1::OperatorGatedNonvirgin) => {
            fact.predecessor == AuthorityEpochPredecessorV1::NoneAfterOperatorReinitialization
                && complete_operator_evidence
        }
        // The only non-empty prefix an operator-gated epoch may follow is one
        // made entirely of valid tail-recovery facts. Those facts stay in the
        // chain; this epoch's entry hash-chains after the last of them.
        (None, Some(_), AuthorityStartupClassV1::OperatorGatedNonvirgin)
            if replay.tail_recovery_only =>
        {
            fact.predecessor == AuthorityEpochPredecessorV1::NoneAfterOperatorReinitialization
                && complete_operator_evidence
        }
        (None, Some(expected), AuthorityStartupClassV1::LegacyLedgerUpgrade) => {
            &fact.predecessor == expected && no_operator_evidence
        }
        (Some(_), Some(expected), AuthorityStartupClassV1::OrdinaryReopen) => {
            &fact.predecessor == expected && no_operator_evidence
        }
        _ => false,
    };
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
        || !startup_relation_valid
        || Timestamp::new(fact.establishment.started_at.clone()).is_err()
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

fn validate_legacy_import_fact(
    entry: &MctObservationLedgerEntry,
    replay: &AuthorityReplayV1,
    fact: &LegacyAuthorityImportFactV1,
) -> std::result::Result<(), AuthorityReplayError> {
    let incoherent = |detail: &str| AuthorityReplayError::Incoherent {
        sequence: entry.local_sequence,
        detail: detail.to_owned(),
    };
    if replay.state.admin_genesis.is_some()
        || fact.imported_state.admin_genesis.is_some()
        || replay.state.guardian_recovery.is_some()
        || fact.imported_state.guardian_recovery.is_some()
        || !replay.state.admin_issuance.is_empty()
        || !fact.imported_state.admin_issuance.is_empty()
    {
        return Err(incoherent(
            "legacy import cannot introduce or overwrite recovery authority",
        ));
    }
    let Some(prior_authority) = replay.current_authority.as_ref() else {
        return Err(incoherent("legacy import precedes epoch establishment"));
    };
    let prior_hash =
        authority_state_hash(&replay.state).map_err(|error| incoherent(&error.to_string()))?;
    validate_watch_scope_state(&fact.imported_state).map_err(|error| incoherent(&error))?;
    let imported_hash = authority_state_hash(&fact.imported_state)
        .map_err(|error| incoherent(&error.to_string()))?;
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
        || entry.observation.source_plane != SourcePlane::Operator
        || entry.durability_class != DurabilityClass::BeforeEffect
        || entry.observation.visibility != ObservationVisibility::NodeOperator
        || entry.observation.subject_id.as_deref() != Some(entry.mother_node_id.as_str())
        || entry.observation.resource_id.as_deref() != Some(entry.ledger_id.as_str())
        || entry.observation.grants_revision != Some(expected_generation)
        || fact.mother_node_id != entry.mother_node_id
        || fact.ledger_id != entry.ledger_id
        || fact.authority_epoch != prior_authority.authority_epoch
        || fact.operator_decision.confirmation != LEGACY_AUTHORITY_IMPORT_CONFIRMATION_V1
        || fact.prior_state.grants_authority != *prior_authority
        || fact.prior_state.authority_state_hash != prior_hash
        || fact.resulting_state.grants_authority != expected_identity
        || fact.resulting_state.authority_state_hash != imported_hash
        || fact.source_evidence.sqlite_authority_hash != imported_hash
    {
        return Err(incoherent(
            "legacy import does not match replayed prior/imported state",
        ));
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
    verify_admin_change_signatures(
        &replay.state,
        &replay.admin_signature_nonces,
        &fact.mother_node_id,
        &fact.ledger_id,
        &fact.changes,
    )
    .map_err(|detail| incoherent(&detail))?;
    admin_issuance::validate_context(
        &replay.admin_grant_ids,
        &fact.changes,
        prior_authority,
        &entry.ledger_id,
        &fact.mutation_id,
        &fact.decided_at,
    )
    .map_err(|detail| incoherent(&detail))?;
    guardian_recovery::validate_context(
        &replay.state,
        &fact.changes,
        prior_authority,
        &entry.ledger_id,
        &fact.decided_at,
    )
    .map_err(|detail| incoherent(&detail))?;
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

fn empty_scan_state() -> LedgerScanState {
    LedgerScanState {
        next_sequence: 0,
        previous_hash: None,
        committed_len: 0,
        recovery_observation_ids: BTreeSet::new(),
        authority_replay: Ok(empty_authority_replay()),
        used_epochs: BTreeSet::new(),
        prefix_hasher: PrefixHasher::new(),
        last_observation_id: None,
        last_frame_len: 0,
    }
}

fn scan_existing(path: &Path, ledger_id: &str, mother_node_id: &str) -> Result<LedgerScan> {
    let file = private_fs::read_file(path).map_err(|source| ObservationLedgerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let mut state = empty_scan_state();
    let mut frame = Vec::new();

    loop {
        frame.clear();
        let read =
            reader
                .read_until(b'\n', &mut frame)
                .map_err(|source| ObservationLedgerError::Io {
                    path: path.to_path_buf(),
                    source,
                })?;
        if read == 0 {
            break;
        }
        let offset = state.committed_len;
        if !frame.ends_with(b"\n") {
            let digest = blake3::hash(&frame).to_hex().to_string();
            return Ok(LedgerScan::Residue(LedgerTailResidue {
                offset,
                bytes: frame,
                digest,
                state,
            }));
        }
        let entry_bytes = &frame[..frame.len() - 1];
        let entry: MctObservationLedgerEntry = match serde_json::from_slice(entry_bytes) {
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

        remember_scanned_entry(path, &mut state, &entry, &frame);
    }

    Ok(LedgerScan::Ready(state))
}

fn remember_scanned_entry(
    path: &Path,
    state: &mut LedgerScanState,
    entry: &MctObservationLedgerEntry,
    frame: &[u8],
) {
    let previous = state
        .next_sequence
        .checked_sub(1)
        .zip(state.previous_hash.clone());
    if let Ok(replay) = state.authority_replay.as_mut()
        && let Err(error) = fold_authority_entry(
            replay,
            &mut state.used_epochs,
            previous
                .as_ref()
                .map(|(sequence, hash)| (*sequence, hash.as_str())),
            entry,
        )
    {
        state.authority_replay = Err(error.to_string());
    }
    let observation_id = entry.observation.observation_id.as_str();
    if observation_id.starts_with("ledger-tail-recovery-") {
        state
            .recovery_observation_ids
            .insert(observation_id.to_owned());
    }
    state.last_observation_id = Some(observation_id.to_owned());
    state.last_frame_len = frame.len() as u64;
    state.prefix_hasher.update(frame);
    note_prefix_bytes_hashed(path, frame.len() as u64);
    state.next_sequence += 1;
    state.previous_hash = Some(entry.entry_hash.clone());
    state.committed_len += frame.len() as u64;
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
    // Exclusive creation preserves the strict-genesis birth distinction.
    let (file, created_new) =
        private_fs::open_or_create(&path).map_err(|source| ObservationLedgerError::Io {
            path: path.clone(),
            source,
        })?;
    let mut file = acquire_writer_lock(&path, file)?;

    let (state, recovery_status) = match scan_existing(&path, ledger_id, mother_node_id)? {
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

    Ok(JsonlObservationLedger {
        path,
        file,
        created_new,
        ledger_id: ledger_id.to_owned(),
        mother_node_id: mother_node_id.to_owned(),
        next_sequence: state.next_sequence,
        previous_hash: state.previous_hash,
        recovery_status,
        authority_tenure: None,
        recovery_clock_origin: std::time::Instant::now(),
        recovery_wait_offset_ms: 0,
        #[cfg(test)]
        recovery_test_ack_delay_ms: 0,
        writer_state: LedgerWriterState::Ready,
        authority_replay: state.authority_replay,
        used_epochs: state.used_epochs,
        prefix_hasher: state.prefix_hasher.0,
        prefix_len: state.committed_len,
        last_observation_id: state.last_observation_id,
        last_frame_len: state.last_frame_len,
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

    let truncated_len = residue.state.committed_len;
    file.set_len(truncated_len)
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
    state.committed_len = truncated_len;
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
            .recovery_observation_ids
            .contains(&status.recovery_decision_id);
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
        .recovery_observation_ids
        .contains(observation_id.as_str())
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
        safe_message: TAIL_RECOVERY_SAFE_MESSAGE.into(),
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
    let frame = write_entry_durable(file, path, &entry)?;
    remember_scanned_entry(path, state, &entry, &frame);
    Ok(())
}

fn write_entry_durable(
    file: &mut File,
    path: &Path,
    entry: &MctObservationLedgerEntry,
) -> Result<Vec<u8>> {
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
        })?;
    Ok(bytes)
}

fn preserve_quarantine(
    path: &Path,
    mut status: LedgerQuarantineStatus,
) -> Result<LedgerQuarantineStatus> {
    let bytes = private_fs::read(path).map_err(|source| ObservationLedgerError::Io {
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

/// True when this ledger has a residue case whose recovery did not complete.
fn pending_tail_recovery_exists(
    path: &Path,
    ledger_id: &str,
    mother_node_id: &str,
) -> Result<bool> {
    let root = forensic_root_path(path);
    let cases = match std::fs::read_dir(&root) {
        Ok(cases) => cases,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(source) => return Err(ObservationLedgerError::Io { path: root, source }),
    };
    for case in cases.filter_map(std::result::Result::ok) {
        let record_path = case.path().join("record.json");
        if !record_path.is_file() {
            continue;
        }
        let Ok(status) = read_recovery_status(&record_path) else {
            continue;
        };
        if status.ledger_path == path
            && status.source_ledger_id == ledger_id
            && status.source_mother_node_id == mother_node_id
            && !status.recovery_complete
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn read_recovery_status(path: &Path) -> Result<LedgerRecoveryStatus> {
    let bytes = private_fs::read(path).map_err(|source| ObservationLedgerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| ObservationLedgerError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn ensure_private_directory(path: &Path) -> Result<()> {
    private_fs::ensure_directory(path).map_err(|source| ObservationLedgerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    sync_directory(path)
}

fn write_bytes_once_durable(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        let existing = private_fs::read(path).map_err(|source| ObservationLedgerError::Io {
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
    private_fs::write_atomic(path, bytes).map_err(|source| ObservationLedgerError::Io {
        path: path.to_path_buf(),
        source,
    })
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
        match private_fs::read_file(&path) {
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

/// Head of a hash-chain that has been verified without retaining every entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LedgerVerifiedHead {
    pub local_sequence: u64,
    pub entry_hash: String,
    pub observation_id: String,
    pub ledger_id: String,
    pub mother_node_id: String,
}

/// Streaming verification result. `head` is absent only for an empty ledger.
#[derive(Clone, Debug)]
pub struct VerifiedLedgerReplay {
    pub head: Option<LedgerVerifiedHead>,
    pub replay: AuthorityReplayV1,
    pub prefix_len: u64,
    pub prefix_blake3: String,
    pub checkpoint: Option<LedgerReplayCheckpoint>,
}

/// Verified prefix binding. Hot paths hash these raw prefix bytes and parse
/// only the entries that follow. A digest or entry-hash mismatch is not trusted.
#[derive(Clone, Debug)]
pub struct LedgerReplayCheckpoint {
    pub through_sequence: u64,
    pub through_entry_hash: String,
    pub prefix_len: u64,
    pub prefix_blake3: String,
    pub checkpoint_frame_len: u64,
    pub replay: AuthorityReplayV1,
    pub used_epochs: BTreeSet<String>,
}

#[derive(Clone, Debug)]
pub enum CheckpointResume {
    Resumed(Box<VerifiedLedgerReplay>),
    /// The checkpoint does not bind the current log. Callers must full-verify
    /// and fail closed if that verification fails.
    Mismatch {
        detail: String,
    },
}

/// Stream the ledger, verify every link, and fold authority facts.
///
/// Entries are not retained. Quarantine, foreign lineage, and a torn tail fail closed.
pub fn verify_ledger_streaming(
    path: impl AsRef<Path>,
    ledger_id: &str,
    mother_node_id: &str,
) -> Result<VerifiedLedgerReplay> {
    let path = path.as_ref();
    match scan_existing(path, ledger_id, mother_node_id)? {
        LedgerScan::Ready(state) => verified_from_scan(state, ledger_id, mother_node_id),
        LedgerScan::Residue(residue) => Err(ObservationLedgerError::UnterminatedTail {
            path: path.to_path_buf(),
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

/// Verify the checkpoint prefix by its raw BLAKE3 digest and entry hash, then
/// parse and chain-check only the suffix. A prefix mismatch returns
/// [`CheckpointResume::Mismatch`]. A broken suffix fails closed.
pub fn resume_ledger_replay(
    path: impl AsRef<Path>,
    ledger_id: &str,
    mother_node_id: &str,
    checkpoint: &LedgerReplayCheckpoint,
) -> Result<CheckpointResume> {
    let path = path.as_ref();
    if checkpoint.prefix_len == 0
        || checkpoint.checkpoint_frame_len == 0
        || checkpoint.checkpoint_frame_len > checkpoint.prefix_len
    {
        return Ok(CheckpointResume::Mismatch {
            detail: "checkpoint does not name a committed entry".into(),
        });
    }
    let mut file = private_fs::read_file(path).map_err(|source| ObservationLedgerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let file_len = file
        .metadata()
        .map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .len();
    if file_len < checkpoint.prefix_len {
        return Ok(CheckpointResume::Mismatch {
            detail: "ledger is shorter than the checkpoint prefix".into(),
        });
    }

    let mut hasher = blake3::Hasher::new();
    let mut remaining = checkpoint.prefix_len;
    let mut carry = Vec::new();
    let mut last_frame = Vec::new();
    let mut buffer = [0_u8; 64 * 1024];
    while remaining > 0 {
        let want = std::cmp::min(buffer.len() as u64, remaining) as usize;
        file.read_exact(&mut buffer[..want])
            .map_err(|source| ObservationLedgerError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        hasher.update(&buffer[..want]);
        note_prefix_bytes_hashed(path, want as u64);
        for byte in &buffer[..want] {
            carry.push(*byte);
            if *byte == b'\n' {
                last_frame = std::mem::take(&mut carry);
            }
        }
        remaining -= want as u64;
    }
    if !carry.is_empty() || last_frame.len() as u64 != checkpoint.checkpoint_frame_len {
        return Ok(CheckpointResume::Mismatch {
            detail: "checkpoint prefix frame boundary moved".into(),
        });
    }
    let digest = hasher.clone().finalize().to_hex().to_string();
    if digest != checkpoint.prefix_blake3 {
        return Ok(CheckpointResume::Mismatch {
            detail: "checkpoint prefix digest does not match the log".into(),
        });
    }

    let entry: MctObservationLedgerEntry =
        match serde_json::from_slice(&last_frame[..last_frame.len() - 1]) {
            Ok(entry) => entry,
            Err(_) => {
                return Ok(CheckpointResume::Mismatch {
                    detail: "checkpoint entry is not a ledger frame".into(),
                });
            }
        };
    let recomputed = entry_hash(&entry)?;
    if entry.local_sequence != checkpoint.through_sequence
        || entry.entry_hash != checkpoint.through_entry_hash
        || recomputed != checkpoint.through_entry_hash
        || entry.ledger_id != ledger_id
        || entry.mother_node_id != mother_node_id
    {
        return Ok(CheckpointResume::Mismatch {
            detail: "checkpoint entry hash does not match the log".into(),
        });
    }

    let mut replay = checkpoint.replay.clone();
    let mut used_epochs = checkpoint.used_epochs.clone();
    let mut expected_sequence = checkpoint.through_sequence.checked_add(1).ok_or(
        ObservationLedgerError::SequenceMismatch {
            expected: checkpoint.through_sequence,
            actual: u64::MAX,
        },
    )?;
    let mut previous_hash = Some(checkpoint.through_entry_hash.clone());
    let mut head = head_from_entry(&entry);
    let mut prefix_len = checkpoint.prefix_len;
    let mut last_frame_len = checkpoint.checkpoint_frame_len;
    let mut reader = BufReader::new(file);
    let mut frame = Vec::new();
    loop {
        frame.clear();
        let read =
            reader
                .read_until(b'\n', &mut frame)
                .map_err(|source| ObservationLedgerError::Io {
                    path: path.to_path_buf(),
                    source,
                })?;
        if read == 0 {
            break;
        }
        let offset = prefix_len;
        if !frame.ends_with(b"\n") {
            return Err(ObservationLedgerError::UnterminatedTail {
                path: path.to_path_buf(),
                offset,
                length: frame.len() as u64,
                digest: blake3::hash(&frame).to_hex().to_string(),
            });
        }
        let suffix_entry: MctObservationLedgerEntry =
            serde_json::from_slice(&frame[..frame.len() - 1]).map_err(|source| {
                ObservationLedgerError::Json {
                    path: path.to_path_buf(),
                    source,
                }
            })?;
        if suffix_entry.local_sequence != expected_sequence {
            return Err(ObservationLedgerError::SequenceMismatch {
                expected: expected_sequence,
                actual: suffix_entry.local_sequence,
            });
        }
        if suffix_entry.ledger_id != ledger_id || suffix_entry.mother_node_id != mother_node_id {
            return Err(ObservationLedgerError::LedgerIdentityMismatch {
                sequence: suffix_entry.local_sequence,
                expected_ledger_id: ledger_id.to_owned(),
                expected_mother_node_id: mother_node_id.to_owned(),
                actual_ledger_id: suffix_entry.ledger_id.clone(),
                actual_mother_node_id: suffix_entry.mother_node_id.clone(),
            });
        }
        if suffix_entry.previous_entry_hash != previous_hash {
            return Err(ObservationLedgerError::BrokenHashChain {
                sequence: suffix_entry.local_sequence,
            });
        }
        let expected = entry_hash(&suffix_entry)?;
        if suffix_entry.entry_hash != expected {
            return Err(ObservationLedgerError::BrokenHashChain {
                sequence: suffix_entry.local_sequence,
            });
        }
        let previous = Some((
            expected_sequence - 1,
            previous_hash.clone().unwrap_or_default(),
        ));
        fold_authority_entry(
            &mut replay,
            &mut used_epochs,
            previous
                .as_ref()
                .map(|(sequence, hash)| (*sequence, hash.as_str())),
            &suffix_entry,
        )
        .map_err(|error| ObservationLedgerError::AuthorityReplay {
            detail: error.to_string(),
        })?;
        hasher.update(&frame);
        prefix_len += frame.len() as u64;
        last_frame_len = frame.len() as u64;
        previous_hash = Some(suffix_entry.entry_hash.clone());
        expected_sequence += 1;
        head = head_from_entry(&suffix_entry);
    }

    let verified = assemble_verified(
        Some(head),
        replay,
        used_epochs,
        prefix_len,
        last_frame_len,
        hasher.finalize().to_hex().to_string(),
    );
    Ok(CheckpointResume::Resumed(Box::new(verified)))
}

/// Verify the log, reusing a process-local checkpoint when its prefix digest
/// still matches. A mismatch falls back to a streaming full verification.
pub fn load_verified_ledger_replay(
    path: impl AsRef<Path>,
    ledger_id: &str,
    mother_node_id: &str,
) -> Result<VerifiedLedgerReplay> {
    let path = path.as_ref();
    let key = replay_cache_key(path, ledger_id, mother_node_id);
    if let Some(checkpoint) = replay_cache_get(&key) {
        match resume_ledger_replay(path, ledger_id, mother_node_id, &checkpoint)? {
            CheckpointResume::Resumed(verified) => {
                if let Some(checkpoint) = verified.checkpoint.clone() {
                    replay_cache_store(key, checkpoint);
                }
                return Ok(*verified);
            }
            CheckpointResume::Mismatch { .. } => replay_cache_remove(&key),
        }
    }
    let verified = verify_ledger_streaming(path, ledger_id, mother_node_id)?;
    if let Some(checkpoint) = verified.checkpoint.clone() {
        replay_cache_store(key, checkpoint);
    }
    Ok(verified)
}

/// Write `count` chain-valid observations and fsync once.
///
/// This is a benchmark fixture. It is not the before-effect durability path.
pub fn write_benchmark_chain(
    path: impl AsRef<Path>,
    ledger_id: &str,
    mother_node_id: &str,
    count: u64,
) -> Result<u64> {
    let path = path.as_ref();
    let (mut file, _) =
        private_fs::open_or_create(path).map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.set_len(0)
        .map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let mut previous = None;
    for sequence in 0..count {
        let observation_id = ObservationId::new(format!("obs-bench-{sequence}"))
            .expect("benchmark observation id is non-empty");
        let trace_id = TraceId::new("trace-bench").expect("benchmark trace id is non-empty");
        let observation = MctObservation::informational(
            observation_id,
            Timestamp::new("2026-05-31T00:00:00Z").expect("fixed benchmark timestamp"),
            ObservationKind::PeerHelloReceived,
            trace_id,
            "benchmark observation",
        );
        let mut entry = MctObservationLedgerEntry {
            ledger_id: ledger_id.to_owned(),
            mother_node_id: mother_node_id.to_owned(),
            local_sequence: sequence,
            observation,
            previous_entry_hash: previous.clone(),
            entry_hash: String::new(),
            appended_at: "2026-05-31T00:00:00Z".into(),
            durability_class: DurabilityClass::BeforeEffect,
            export_status: ExportStatus::NotRequired,
        };
        entry.entry_hash = entry_hash(&entry)?;
        previous = Some(entry.entry_hash.clone());
        let mut frame =
            serde_json::to_vec(&entry).map_err(|source| ObservationLedgerError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        frame.push(b'\n');
        file.write_all(&frame)
            .map_err(|source| ObservationLedgerError::Io {
                path: path.to_path_buf(),
                source,
            })?;
    }
    file.sync_all()
        .map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(file
        .metadata()
        .map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .len())
}

/// Drop process-local replay checkpoints. Tests use this to isolate tamper cases.
pub fn clear_ledger_replay_cache() {
    replay_cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
}

fn verified_from_scan(
    state: LedgerScanState,
    ledger_id: &str,
    mother_node_id: &str,
) -> Result<VerifiedLedgerReplay> {
    let replay = state
        .authority_replay
        .map_err(|detail| ObservationLedgerError::AuthorityReplay { detail })?;
    let prefix_blake3 = state.prefix_hasher.0.finalize().to_hex().to_string();
    let head = match (
        state.next_sequence.checked_sub(1),
        state.previous_hash,
        state.last_observation_id,
    ) {
        (Some(sequence), Some(entry_hash), Some(observation_id)) => Some(LedgerVerifiedHead {
            local_sequence: sequence,
            entry_hash,
            observation_id,
            ledger_id: ledger_id.to_owned(),
            mother_node_id: mother_node_id.to_owned(),
        }),
        _ => None,
    };
    Ok(assemble_verified(
        head,
        replay,
        state.used_epochs,
        state.committed_len,
        state.last_frame_len,
        prefix_blake3,
    ))
}

fn assemble_verified(
    head: Option<LedgerVerifiedHead>,
    replay: AuthorityReplayV1,
    used_epochs: BTreeSet<String>,
    prefix_len: u64,
    checkpoint_frame_len: u64,
    prefix_blake3: String,
) -> VerifiedLedgerReplay {
    let checkpoint = head.as_ref().map(|head| LedgerReplayCheckpoint {
        through_sequence: head.local_sequence,
        through_entry_hash: head.entry_hash.clone(),
        prefix_len,
        prefix_blake3: prefix_blake3.clone(),
        checkpoint_frame_len,
        replay: replay.clone(),
        used_epochs,
    });
    VerifiedLedgerReplay {
        head,
        replay,
        prefix_len,
        prefix_blake3,
        checkpoint,
    }
}

fn head_from_entry(entry: &MctObservationLedgerEntry) -> LedgerVerifiedHead {
    LedgerVerifiedHead {
        local_sequence: entry.local_sequence,
        entry_hash: entry.entry_hash.clone(),
        observation_id: entry.observation.observation_id.to_string(),
        ledger_id: entry.ledger_id.clone(),
        mother_node_id: entry.mother_node_id.clone(),
    }
}

fn replay_cache() -> &'static Mutex<BTreeMap<String, LedgerReplayCheckpoint>> {
    static CACHE: OnceLock<Mutex<BTreeMap<String, LedgerReplayCheckpoint>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn replay_cache_key(path: &Path, ledger_id: &str, mother_node_id: &str) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    format!("{}|{ledger_id}|{mother_node_id}", canonical.display())
}

fn replay_cache_get(key: &str) -> Option<LedgerReplayCheckpoint> {
    replay_cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(key)
        .cloned()
}

fn replay_cache_store(key: String, checkpoint: LedgerReplayCheckpoint) {
    replay_cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(key, checkpoint);
}

fn replay_cache_remove(key: &str) {
    replay_cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(key);
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mct_kernel::{CallId, MctObservation, ObservationId, ObservationKind, Timestamp, TraceId};
    use std::fs::OpenOptions;

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

    #[test]
    fn startup_residue_recovery_runs_the_ratified_path_and_leaves_other_ledgers_alone() {
        // Missing ledger: nothing is created.
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.jsonl");
        assert!(
            JsonlObservationLedger::recover_residue_for_startup(&missing, "ledger-a", "mother-a")
                .unwrap()
                .is_none()
        );
        assert!(!missing.exists());
        assert!(!forensic_root_path(&missing).exists());

        // Clean ledger: byte-identical, no forensic root.
        let clean = dir.path().join("clean.jsonl");
        append_fixture_entry(&clean, "obs-clean");
        let clean_before = std::fs::read(&clean).unwrap();
        assert!(
            JsonlObservationLedger::recover_residue_for_startup(&clean, "ledger-a", "mother-a")
                .unwrap()
                .is_none()
        );
        assert_eq!(std::fs::read(&clean).unwrap(), clean_before);
        assert!(!forensic_root_path(&clean).exists());

        // Terminated corruption stays the classifier's quarantine case.
        let corrupt = dir.path().join("corrupt.jsonl");
        append_fixture_entry(&corrupt, "obs-before-corruption");
        append_raw(&corrupt, b"not-json\n");
        let corrupt_before = std::fs::read(&corrupt).unwrap();
        assert!(
            JsonlObservationLedger::recover_residue_for_startup(&corrupt, "ledger-a", "mother-a")
                .unwrap()
                .is_none()
        );
        assert_eq!(std::fs::read(&corrupt).unwrap(), corrupt_before);
        assert!(!forensic_root_path(&corrupt).exists());

        // Torn tail: preserved exactly, set aside, one recovery observation, then readable.
        let torn = dir.path().join("torn.jsonl");
        let first = append_fixture_entry(&torn, "obs-before-startup-torn-tail");
        let committed = std::fs::read(&torn).unwrap();
        let residue = br#"{"ledger_id":"ledger-a","mother_node_id":"mo"#;
        append_raw(&torn, residue);
        let status =
            JsonlObservationLedger::recover_residue_for_startup(&torn, "ledger-a", "mother-a")
                .unwrap()
                .expect("startup recovery reports the residue case");
        assert!(status.recovery_complete);
        assert_eq!(status.residue_offset, committed.len() as u64);
        assert_eq!(
            std::fs::read(&status.preserved_bytes_path).unwrap(),
            residue
        );
        assert_eq!(status.last_committed_sequence, Some(0));
        assert_eq!(
            status.last_committed_hash.as_deref(),
            Some(first.entry_hash.as_str())
        );
        let after = std::fs::read(&torn).unwrap();
        assert!(after.starts_with(&committed));
        let entries = JsonlObservationLedger::open_read_only(&torn, "ledger-a", "mother-a")
            .expect("recovered ledger classifies read-only")
            .entries()
            .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], first);
        assert_eq!(
            entries[1].observation.observation_id.as_str(),
            status.recovery_decision_id
        );

        // Idempotent: a second startup changes nothing.
        assert!(
            JsonlObservationLedger::recover_residue_for_startup(&torn, "ledger-a", "mother-a")
                .unwrap()
                .is_none()
        );
        assert_eq!(std::fs::read(&torn).unwrap(), after);
    }

    #[test]
    fn startup_residue_recovery_completes_an_interrupted_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        append_fixture_entry(&path, "obs-before-interrupted-startup-recovery");
        let residue = b"unterminated-startup-residue".to_vec();
        append_raw(&path, &residue);
        // Interrupt after truncation: the scan is clean, but the case is incomplete.
        let result = open_with_recovery_hook(&path, "ledger-a", "mother-a", |stage| {
            if stage == RecoveryStage::LedgerTruncated {
                return Err(std::io::Error::other("injected interruption"));
            }
            Ok(())
        });
        assert!(result.is_err());
        drop(result);
        assert!(JsonlObservationLedger::open_read_only(&path, "ledger-a", "mother-a").is_ok());
        let status =
            JsonlObservationLedger::recover_residue_for_startup(&path, "ledger-a", "mother-a")
                .unwrap()
                .expect("interrupted recovery is completed at startup");
        assert!(status.recovery_complete);
        let entries = JsonlObservationLedger::open_read_only(&path, "ledger-a", "mother-a")
            .unwrap()
            .entries()
            .unwrap();
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.observation.observation_id.as_str()
                    == status.recovery_decision_id)
                .count(),
            1
        );
        assert!(
            JsonlObservationLedger::recover_residue_for_startup(&path, "ledger-a", "mother-a")
                .unwrap()
                .is_none()
        );
    }

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

    fn complete_watch_scope() -> mct_kernel::WatchObservationScope {
        mct_kernel::WatchObservationScope {
            watch_scope_id: mct_kernel::WatchObservationScopeId::new("scope-canonical").unwrap(),
            observer_shape: mct_kernel::WatchObserverShape::ChildToy,
            observer_ref: mct_kernel::WatchObserverRef {
                child_name: "child-a".into(),
                artifact_id: mct_kernel::ComponentArtifactId::new("artifact-a").unwrap(),
                artifact_version: "1.0.0".into(),
                assignment_id: mct_kernel::ChildAssignmentId::new("assignment-a").unwrap(),
            },
            scope_mode: mct_kernel::WatchScopeMode::Constrained,
            canonical_root_ref: "file:///tmp/canonical-watch".into(),
            traversal_scope: mct_kernel::WatchTraversalScope::Recursive,
            event_classes: mct_kernel::watch::WatchEventClasses::new(vec![
                mct_kernel::WatchEventClass::Created,
                mct_kernel::WatchEventClass::Modified,
            ])
            .unwrap(),
            max_events_per_batch: 17,
            coalescing_policy: mct_kernel::WatchCoalescingPolicy::LastPerPath,
            starts_at: Timestamp::new("2026-08-05T12:00:00Z").unwrap(),
            expires_at: Timestamp::new("2026-08-05T13:00:00Z").unwrap(),
            scope_revision: std::num::NonZeroU64::MIN,
            policy_revision: 9,
            authority_state: mct_kernel::WatchObservationScopeState::Active,
            authority_observation_id: ObservationId::new("obs-watch-scope-canonical").unwrap(),
            canonical_record_digest: String::new(),
        }
        .seal()
    }

    #[test]
    fn watch_scope_put_replays_complete_value_without_ordinary_observation() {
        let scope = complete_watch_scope();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        let result = ledger.execute_authority_mutation(
            AuthorityMutationRequestV1 {
                mutation_id: "watch-scope-complete".into(),
                changes: vec![AuthorityChangeV1::WatchScopePut {
                    scope: Box::new(scope.clone()),
                }],
                grant_shaping_sources: vec![GrantShapingSourceV1::OperatorDecision {
                    decision_id: "decision-watch-scope-complete".into(),
                    authenticated_principal_ref: "os-uid:501".into(),
                    command_kind: GrantShapingCommandKindV1::GrantChange,
                }],
                decided_at: "2026-08-05T12:00:00Z".into(),
            },
            |_| Ok(None),
        );
        assert!(matches!(
            result,
            AuthorityMutationResultV1::CommittedProjectionPending { .. }
        ));
        let mut revoked = scope.clone();
        revoked.scope_revision = std::num::NonZeroU64::new(2).unwrap();
        revoked.authority_state = mct_kernel::WatchObservationScopeState::Revoked;
        revoked.authority_observation_id =
            ObservationId::new("obs-watch-scope-canonical-revoked").unwrap();
        revoked = revoked.seal();
        let revoke_result = ledger.execute_authority_mutation(
            AuthorityMutationRequestV1 {
                mutation_id: "watch-scope-revoke-complete".into(),
                changes: vec![AuthorityChangeV1::WatchScopePut {
                    scope: Box::new(revoked.clone()),
                }],
                grant_shaping_sources: vec![GrantShapingSourceV1::OperatorDecision {
                    decision_id: "decision-watch-scope-revoke-complete".into(),
                    authenticated_principal_ref: "os-uid:501".into(),
                    command_kind: GrantShapingCommandKindV1::GrantChange,
                }],
                decided_at: "2026-08-05T12:01:00Z".into(),
            },
            |_| Ok(None),
        );
        assert!(matches!(
            revoke_result,
            AuthorityMutationResultV1::CommittedProjectionPending { .. }
        ));
        let entries = ledger.entries().unwrap();
        assert!(!entries.iter().any(|entry| {
            entry
                .observation
                .detail_ref
                .as_deref()
                .is_some_and(|detail| detail.starts_with("watch-observation-scope-v1:"))
        }));
        let replay = replay_authority_entries(&entries).unwrap();
        let replayed = replay
            .state
            .watch_scopes
            .get(scope.watch_scope_id.as_str())
            .unwrap();
        assert_eq!(
            serde_json::to_vec(replayed).unwrap(),
            serde_json::to_vec(&revoked).unwrap()
        );
        assert_eq!(replayed.observer_shape, revoked.observer_shape);
        assert_eq!(replayed.observer_ref, revoked.observer_ref);
        assert_eq!(replayed.scope_mode, revoked.scope_mode);
        assert_eq!(replayed.canonical_root_ref, revoked.canonical_root_ref);
        assert_eq!(replayed.traversal_scope, revoked.traversal_scope);
        assert_eq!(replayed.event_classes, revoked.event_classes);
        assert_eq!(replayed.max_events_per_batch, revoked.max_events_per_batch);
        assert_eq!(replayed.coalescing_policy, revoked.coalescing_policy);
        assert_eq!(replayed.starts_at, revoked.starts_at);
        assert_eq!(replayed.expires_at, revoked.expires_at);
        assert_eq!(replayed.scope_revision, revoked.scope_revision);
        assert_eq!(replayed.policy_revision, revoked.policy_revision);
        assert_eq!(replayed.authority_state, revoked.authority_state);
        assert_eq!(
            replayed.authority_observation_id,
            revoked.authority_observation_id
        );
        assert_eq!(
            replayed.canonical_record_digest,
            revoked.canonical_record_digest
        );
    }

    fn gated_startup() -> AuthorityTenureStartupEvidenceV1 {
        AuthorityTenureStartupEvidenceV1 {
            startup_class: AuthorityStartupClassV1::OperatorGatedNonvirgin,
            expected_predecessor: AuthorityEpochPredecessorV1::NoneAfterOperatorReinitialization,
            expected_prior_authority: None,
            expected_authority_state_hash: authority_state_hash(&AuthorityStateV1::default())
                .unwrap(),
            inventory_hash: "inventory-1".into(),
            operator_gate_decision_id: Some("decision:reinit".into()),
            authenticated_principal_ref: Some("os-uid:501".into()),
        }
    }

    fn rechain(entries: &mut [MctObservationLedgerEntry]) {
        let mut previous: Option<String> = None;
        for (sequence, entry) in entries.iter_mut().enumerate() {
            entry.local_sequence = sequence as u64;
            entry.previous_entry_hash = previous.clone();
            entry.entry_hash = entry_hash(entry).unwrap();
            previous = Some(entry.entry_hash.clone());
        }
    }

    #[test]
    fn operator_gated_epoch_replays_only_after_a_recovery_only_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        std::fs::write(&path, br#"{"ledger_id":"ledger-a","mother_"#).unwrap();
        JsonlObservationLedger::recover_residue_for_startup(&path, "ledger-a", "mother-a")
            .unwrap()
            .expect("torn first frame recovers");
        let ledger = JsonlObservationLedger::open_authority_with_startup(
            &path,
            "ledger-a",
            "mother-a",
            gated_startup(),
        )
        .unwrap();
        let entries = ledger.entries().unwrap();
        drop(ledger);
        assert_eq!(entries.len(), 2);
        assert!(is_valid_tail_recovery_entry(&entries[0]));
        assert_eq!(
            entries[1].previous_entry_hash.as_deref(),
            Some(entries[0].entry_hash.as_str())
        );
        let replay = replay_authority_entries(&entries).unwrap();
        assert!(replay.current_authority.is_some());

        // The same epoch after any other entry is not legal.
        let mut other = entries[0].clone();
        other.observation.observation_id = ObservationId::new("obs:other").unwrap();
        other.observation.detail_ref = None;
        let mut with_other = vec![entries[0].clone(), other, entries[1].clone()];
        rechain(&mut with_other);
        assert!(replay_authority_entries(&with_other).is_err());

        // Nor after a recovery-looking entry that is not exactly a recovery fact.
        let mut forged = entries[0].clone();
        forged.observation.safe_message = "observation ledger tail recovered!".into();
        assert!(is_tail_recovery_entry(&forged));
        assert!(!is_valid_tail_recovery_entry(&forged));
        let mut with_forged = vec![forged, entries[1].clone()];
        rechain(&mut with_forged);
        assert!(replay_authority_entries(&with_forged).is_err());

        // Two recovery facts (two crashes before any epoch) still admit it.
        let mut second = entries[0].clone();
        let digest = "b".repeat(64);
        let id = format!("ledger-tail-recovery-100-{digest}");
        second.observation.observation_id = ObservationId::new(id.clone()).unwrap();
        second.observation.trace.trace_id = TraceId::new(format!("trace-{id}")).unwrap();
        second.observation.detail_ref = Some(format!("ledger-tail-recovery-v1:{digest}"));
        assert!(is_valid_tail_recovery_entry(&second));
        let mut two = vec![entries[0].clone(), second, entries[1].clone()];
        rechain(&mut two);
        assert!(replay_authority_entries(&two).is_ok());

        // A non-gated class over a recovery-only prefix is unchanged by the amendment.
        let mut virgin_over_recovery = entries.clone();
        let detail = virgin_over_recovery[1]
            .observation
            .detail_ref
            .clone()
            .unwrap()
            .replace("operator_gated_nonvirgin", "virgin")
            .replace("none_after_operator_reinitialization", "none_for_virgin")
            .replace("\"decision:reinit\"", "null")
            .replace("\"os-uid:501\"", "null");
        assert!(!detail.contains("decision:reinit") && !detail.contains("os-uid:501"));
        virgin_over_recovery[1].observation.detail_ref = Some(detail);
        rechain(&mut virgin_over_recovery);
        assert!(replay_authority_entries(&virgin_over_recovery).is_err());
    }

    #[test]
    fn strict_tail_recovery_identity_rejects_noncanonical_ids() {
        let digest = "c".repeat(64);
        let valid = MctObservationLedgerEntry {
            ledger_id: "ledger-a".into(),
            mother_node_id: "mother-a".into(),
            local_sequence: 0,
            observation: MctObservation {
                observation_id: ObservationId::new(format!("ledger-tail-recovery-0-{digest}"))
                    .unwrap(),
                observed_at: Timestamp::new("2026-10-17T00:00:00Z").unwrap(),
                kind: ObservationKind::StorageAppendSucceeded,
                source_plane: SourcePlane::Storage,
                trace: ObservationTraceRef {
                    trace_id: TraceId::new(format!("trace-ledger-tail-recovery-0-{digest}"))
                        .unwrap(),
                    span_id: None,
                    parent_span_id: None,
                    external_trace_id: None,
                },
                call_id: None,
                decision_id: None,
                subject_id: Some("mother-a".into()),
                resource_id: Some("ledger-a".into()),
                policy_revision: None,
                grants_revision: None,
                outcome: ObservationOutcome::Completed,
                visibility: ObservationVisibility::NodeOperator,
                safe_message: TAIL_RECOVERY_SAFE_MESSAGE.into(),
                detail_ref: Some(format!("ledger-tail-recovery-v1:{digest}")),
            },
            previous_entry_hash: None,
            entry_hash: String::new(),
            appended_at: "2026-10-17T00:00:00Z".into(),
            durability_class: DurabilityClass::BeforeEffect,
            export_status: ExportStatus::NotRequired,
        };
        assert!(is_valid_tail_recovery_entry(&valid));
        let with_id = |id: String| {
            let mut entry = valid.clone();
            entry.observation.trace.trace_id = TraceId::new(format!("trace-{id}")).unwrap();
            entry.observation.observation_id = ObservationId::new(id).unwrap();
            entry
        };
        for bad in [
            format!("ledger-tail-recovery-00-{digest}"),
            format!("ledger-tail-recovery-+0-{digest}"),
            format!("ledger-tail-recovery-0-{}", "C".repeat(64)),
            format!("ledger-tail-recovery-0-{}", "c".repeat(63)),
            format!("ledger-tail-recovery-0-{digest}-x"),
        ] {
            assert!(
                !is_valid_tail_recovery_entry(&with_id(bad.clone())),
                "{bad}"
            );
        }
        let mut other_digest = valid.clone();
        other_digest.observation.detail_ref =
            Some(format!("ledger-tail-recovery-v1:{}", "d".repeat(64)));
        assert!(!is_valid_tail_recovery_entry(&other_digest));
        let mut other_subject = valid.clone();
        other_subject.observation.subject_id = Some("mother-b".into());
        assert!(!is_valid_tail_recovery_entry(&other_subject));
        let mut with_call = valid.clone();
        with_call.observation.call_id = Some(CallId::new("call-1").unwrap());
        assert!(!is_valid_tail_recovery_entry(&with_call));
        let mut after_effect = valid;
        after_effect.durability_class = DurabilityClass::Buffered;
        assert!(!is_valid_tail_recovery_entry(&after_effect));
    }

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

    #[test]
    fn accepted_operator_reinitialization_embeds_gate_in_first_epoch_fact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let evidence = AuthorityTenureStartupEvidenceV1 {
            startup_class: AuthorityStartupClassV1::OperatorGatedNonvirgin,
            expected_predecessor: AuthorityEpochPredecessorV1::NoneAfterOperatorReinitialization,
            expected_prior_authority: None,
            expected_authority_state_hash: authority_state_hash(&AuthorityStateV1::default())
                .unwrap(),
            inventory_hash: "inventory-hash".into(),
            operator_gate_decision_id: Some("decision:reinitialize-1".into()),
            authenticated_principal_ref: Some("os-uid:501".into()),
        };
        let ledger = JsonlObservationLedger::open_authority_with_startup(
            &path, "ledger-a", "mother-a", evidence,
        )
        .unwrap();
        let tenure = ledger.authority_tenure().unwrap();

        assert_eq!(
            tenure.entry.local_sequence, 0,
            "the gated epoch fact must be the first append"
        );
        assert_eq!(
            tenure.fact.predecessor,
            AuthorityEpochPredecessorV1::NoneAfterOperatorReinitialization
        );
        assert_eq!(tenure.fact.generation_baseline, 0);
        assert!(tenure.fact.prior_authority.is_none());
        assert_eq!(
            tenure
                .fact
                .establishment
                .operator_gate_decision_id
                .as_deref(),
            Some("decision:reinitialize-1")
        );
        assert_eq!(
            tenure
                .fact
                .establishment
                .authenticated_principal_ref
                .as_deref(),
            Some("os-uid:501")
        );
    }

    #[test]
    fn valid_nonempty_pre_h2_ledger_is_classified_as_legacy_upgrade() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let predecessor = {
            let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
            ledger
                .append_before_effect(
                    observation("obs:pre-h2", "trace:pre-h2", None),
                    "2026-08-04T00:00:00Z",
                )
                .unwrap()
        };

        let mut ledger = JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a")
            .expect("an intact pre-H2 chain is continuity evidence");
        let fact = ledger.authority_tenure().unwrap().fact.clone();
        assert_eq!(
            fact.establishment.startup_class,
            AuthorityStartupClassV1::LegacyLedgerUpgrade,
            "a valid non-empty ledger with no replayed authority auto-permits legacy_ledger_upgrade"
        );
        assert_eq!(
            fact.predecessor,
            AuthorityEpochPredecessorV1::ValidatedHead {
                sequence: predecessor.local_sequence,
                entry_hash: predecessor.entry_hash,
            }
        );
        assert_eq!(fact.generation_baseline, 0);
        assert!(fact.prior_authority.is_none());

        let empty_hash = authority_state_hash(&AuthorityStateV1::default()).unwrap();
        let import = ledger.execute_legacy_authority_import(
            LegacyAuthorityImportRequestV1 {
                schema: "mct-legacy-authority-import-request/v1".into(),
                import_id: "legacy-upgrade-import".into(),
                expected_mother_node_id: "mother-a".into(),
                expected_ledger_id: "ledger-a".into(),
                expected_config_authority_hash: empty_hash.clone(),
                expected_sqlite_authority_hash: empty_hash,
                confirmation: LEGACY_AUTHORITY_IMPORT_CONFIRMATION_V1.into(),
            },
            "os-uid:501".into(),
            AuthorityStateV1::default(),
            "2026-08-04T00:01:00Z".into(),
        );
        assert!(
            matches!(
                import,
                AuthorityMutationResultV1::CommittedProjectionPending { .. }
            ),
            "legacy_ledger_upgrade must permit the standard replay-derived import afterward"
        );
    }

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

    fn three_entry_ledger(path: &Path) -> LedgerReplayCheckpoint {
        clear_ledger_replay_cache();
        let mut ledger = JsonlObservationLedger::open(path, "ledger-a", "mother-a").unwrap();
        for index in 1..=3 {
            ledger
                .append_before_effect(
                    observation(&format!("obs-{index}"), "trace-replay", None),
                    "2026-05-31T00:00:01Z",
                )
                .unwrap();
        }
        let checkpoint = ledger.replay_checkpoint().unwrap();
        let head = ledger.verified_head().unwrap();
        let replay = ledger.verified_replay().unwrap().clone();
        let entries = ledger.entries().unwrap();
        assert_eq!(head.local_sequence, 2);
        assert_eq!(replay, replay_authority_entries(&entries).unwrap());
        checkpoint
    }

    #[test]
    fn resume_matches_streaming_verification() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let checkpoint = three_entry_ledger(&path);
        drop_cache_and_compare(&path, &checkpoint);
    }

    #[test]
    fn resume_after_append_sees_the_new_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let checkpoint = three_entry_ledger(&path);
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        ledger
            .append_before_effect(
                observation("obs-4", "trace-replay", None),
                "2026-05-31T00:00:02Z",
            )
            .unwrap();
        drop(ledger);
        match resume_ledger_replay(&path, "ledger-a", "mother-a", &checkpoint).unwrap() {
            CheckpointResume::Resumed(resumed) => {
                assert_eq!(resumed.head.as_ref().unwrap().local_sequence, 3);
                let full = verify_ledger_streaming(&path, "ledger-a", "mother-a").unwrap();
                assert_eq!(resumed.head, full.head);
                assert_eq!(resumed.prefix_blake3, full.prefix_blake3);
                assert_eq!(resumed.replay, full.replay);
            }
            CheckpointResume::Mismatch { detail } => panic!("resume mismatched: {detail}"),
        }
    }

    #[test]
    fn prefix_byte_change_mismatches_then_full_verify_quarantines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let checkpoint = three_entry_ledger(&path);
        let mut bytes = std::fs::read(&path).unwrap();
        let pos = bytes
            .windows(4)
            .position(|window| window == b"obs-")
            .unwrap();
        bytes[pos] = b'X';
        std::fs::write(&path, bytes).unwrap();

        match resume_ledger_replay(&path, "ledger-a", "mother-a", &checkpoint).unwrap() {
            CheckpointResume::Mismatch { .. } => {}
            CheckpointResume::Resumed(_) => panic!("tampered prefix was trusted"),
        }
        assert!(matches!(
            verify_ledger_streaming(&path, "ledger-a", "mother-a"),
            Err(ObservationLedgerError::Quarantined { .. })
        ));
    }

    #[test]
    fn suffix_tamper_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let checkpoint = three_entry_ledger(&path);
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        ledger
            .append_before_effect(
                observation("obs-4", "trace-replay", None),
                "2026-05-31T00:00:02Z",
            )
            .unwrap();
        drop(ledger);
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.iter().rposition(|byte| *byte == b'\n').unwrap();
        let line_start = bytes[..last]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .unwrap()
            + 1;
        let pos = bytes[line_start..]
            .windows(4)
            .position(|window| window == b"obs-")
            .unwrap()
            + line_start;
        bytes[pos] = b'Y';
        std::fs::write(&path, bytes).unwrap();

        let error = resume_ledger_replay(&path, "ledger-a", "mother-a", &checkpoint).unwrap_err();
        assert!(matches!(
            error,
            ObservationLedgerError::BrokenHashChain { sequence: 3 }
        ));
    }

    #[test]
    fn stale_checkpoint_entry_hash_mismatches_without_hiding_a_valid_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut checkpoint = three_entry_ledger(&path);
        checkpoint.through_entry_hash = "f".repeat(64);
        match resume_ledger_replay(&path, "ledger-a", "mother-a", &checkpoint).unwrap() {
            CheckpointResume::Mismatch { .. } => {}
            CheckpointResume::Resumed(_) => panic!("stale entry hash was trusted"),
        }
        let verified = verify_ledger_streaming(&path, "ledger-a", "mother-a").unwrap();
        assert_eq!(verified.head.unwrap().local_sequence, 2);
    }

    fn drop_cache_and_compare(path: &Path, checkpoint: &LedgerReplayCheckpoint) {
        clear_ledger_replay_cache();
        match resume_ledger_replay(path, "ledger-a", "mother-a", checkpoint).unwrap() {
            CheckpointResume::Resumed(resumed) => {
                let full = verify_ledger_streaming(path, "ledger-a", "mother-a").unwrap();
                assert_eq!(resumed.head, full.head);
                assert_eq!(resumed.prefix_blake3, full.prefix_blake3);
                assert_eq!(resumed.replay, full.replay);
                assert_eq!(resumed.prefix_len, full.prefix_len);
            }
            CheckpointResume::Mismatch { detail } => panic!("resume mismatched: {detail}"),
        }
    }

    fn assert_pending_commit(
        result: &AuthorityMutationResultV1,
        mutation_id: &str,
        fact_sequence: u64,
        ledger: &JsonlObservationLedger,
    ) {
        match result {
            AuthorityMutationResultV1::CommittedProjectionPending {
                mutation_id: committed_id,
                resolution,
                fact_sequence: committed_sequence,
                fact_entry_hash,
                grants_authority,
                pending_reason,
            } => {
                assert_eq!(committed_id, mutation_id);
                assert_eq!(*resolution, AuthorityMutationResolutionV1::NewlyCommitted);
                assert_eq!(*committed_sequence, fact_sequence);
                assert_eq!(
                    *pending_reason,
                    AuthorityProjectionPendingReasonV1::ProjectionNotAttempted
                );
                let entry = ledger
                    .entries()
                    .expect("ledger entries")
                    .into_iter()
                    .find(|entry| entry.local_sequence == fact_sequence)
                    .expect("committed sequence");
                assert_eq!(fact_entry_hash, &entry.entry_hash);
                assert_eq!(grants_authority.mother_node_id, "mother-a");
                assert_eq!(grants_authority.generation, fact_sequence);
                assert_eq!(
                    grants_authority.source_authority_observation_id,
                    format!("obs:authority-mutation:{mutation_id}")
                );
                assert_eq!(
                    grants_authority.authority_epoch,
                    ledger
                        .authority_tenure()
                        .expect("tenure")
                        .fact
                        .resulting_authority
                        .authority_epoch
                );
            }
            other => panic!("expected pending commit of {mutation_id}, got {other:?}"),
        }
    }

    const DAY_PK: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
    const RECOVERY_PK: &str = "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c";

    fn half(value: &str) -> AdminKeyPublicHalf {
        AdminKeyPublicHalf::ed25519_from_hex(value).expect("ed25519 public key")
    }

    fn admin_key_source(public_half: &str) -> GrantShapingSourceV1 {
        GrantShapingSourceV1::AdminKey {
            public_half: half(public_half),
        }
    }

    fn day_to_day_admin_grant() -> AdminKeyGrantV1 {
        AdminKeyGrantV1 {
            grant_id: "day-to-day".into(),
            issuance_ceiling: None,
            public_half: half(DAY_PK),
            ml_dsa65_public_key: None,
            class: AdminKeyGrantClassV1::DayToDay,
            scopes: vec![AdminKeyScopeV1::KeyManage, AdminKeyScopeV1::Status],
            expires_at: Some(Timestamp::new("2026-10-14T00:00:00Z").unwrap()),
            grant_state: AdminKeyGrantStateV1::Active,
        }
    }

    fn recovery_admin_grant() -> AdminKeyGrantV1 {
        AdminKeyGrantV1 {
            grant_id: "recovery".into(),
            issuance_ceiling: None,
            public_half: half(RECOVERY_PK),
            ml_dsa65_public_key: None,
            class: AdminKeyGrantClassV1::Recovery,
            scopes: vec![AdminKeyScopeV1::KeyManage, AdminKeyScopeV1::StartupGate],
            expires_at: None,
            grant_state: AdminKeyGrantStateV1::Active,
        }
    }

    fn commit_authority(
        ledger: &mut JsonlObservationLedger,
        id: &str,
        changes: Vec<AuthorityChangeV1>,
        source: GrantShapingSourceV1,
    ) -> AuthorityMutationResultV1 {
        ledger.execute_authority_mutation(
            AuthorityMutationRequestV1 {
                mutation_id: id.into(),
                changes,
                grant_shaping_sources: vec![source],
                decided_at: "2026-08-06T12:00:00Z".into(),
            },
            |_| Ok(None),
        )
    }

    /// An admin-key grant replays from the canonical fact: public half, scope, and class.
    #[test]
    fn admin_key_grant_replays_public_half_scope_and_recovery_class() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        let day = day_to_day_admin_grant();
        let recovery = recovery_admin_grant();
        let result = commit_authority(
            &mut ledger,
            "admin-grants",
            vec![
                AuthorityChangeV1::AdminKeyGrantPut {
                    grant: Box::new(day.clone()),

                    signature: None,
                },
                AuthorityChangeV1::AdminKeyGrantPut {
                    grant: Box::new(recovery.clone()),

                    signature: None,
                },
            ],
            admin_key_source(RECOVERY_PK),
        );
        assert_pending_commit(&result, "admin-grants", 1, &ledger);

        let replay = replay_authority_entries(&ledger.entries().unwrap()).unwrap();
        let replayed_day = replay.state.admin_key_grants.get("day-to-day").unwrap();
        let replayed_recovery = replay.state.admin_key_grants.get("recovery").unwrap();
        assert_eq!(replayed_day, &day);
        assert_eq!(replayed_recovery, &recovery);
        assert_eq!(
            serde_json::to_value(replayed_day).unwrap(),
            serde_json::json!({
                "grant_id": "day-to-day",
                "public_half": {"scheme": "ed25519", "public_key": DAY_PK},
                "class": "day_to_day",
                "scopes": ["key_manage", "status"],
                "expires_at": "2026-10-14T00:00:00Z",
                "grant_state": "active"
            })
        );
        assert_eq!(
            serde_json::to_value(replayed_recovery).unwrap(),
            serde_json::json!({
                "grant_id": "recovery",
                "public_half": {"scheme": "ed25519", "public_key": RECOVERY_PK},
                "class": "recovery",
                "scopes": ["key_manage", "startup_gate"],
                "grant_state": "active"
            })
        );
        assert_eq!(
            replay
                .mutations
                .get("admin-grants")
                .unwrap()
                .fact
                .grant_shaping_sources
                .as_slice(),
            &[admin_key_source(RECOVERY_PK)]
        );
    }

    /// A pairing ticket is a hash. Replay rejects a second consume of that hash.
    #[test]
    fn second_pairing_ticket_consume_of_the_same_hash_is_rejected() {
        let ticket_hash = "abc123";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        let issued = commit_authority(
            &mut ledger,
            "ticket-issue",
            vec![AuthorityChangeV1::PairingTicketIssued {
                ticket_hash: ticket_hash.into(),

                peer_endpoint_id: None,
                window_starts_at: None,
                window_expires_at: None,
                signature: None,
            }],
            admin_key_source(DAY_PK),
        );
        assert_pending_commit(&issued, "ticket-issue", 1, &ledger);
        let consumed = commit_authority(
            &mut ledger,
            "ticket-consume",
            vec![AuthorityChangeV1::PairingTicketConsumed {
                ticket_hash: ticket_hash.into(),

                signature: None,
            }],
            admin_key_source(DAY_PK),
        );
        assert_pending_commit(&consumed, "ticket-consume", 2, &ledger);

        let entries = ledger.entries().unwrap();
        let replay = replay_authority_entries(&entries).unwrap();
        assert_eq!(
            replay.state.pairing_tickets.get(ticket_hash),
            Some(&PairingTicketV1 {
                ticket_hash: ticket_hash.into(),
                state: PairingTicketStateV1::Consumed,

                peer_endpoint_id: None,
                window_starts_at: None,
                window_expires_at: None,
            })
        );

        let before = std::fs::read(&path).unwrap();
        let second = commit_authority(
            &mut ledger,
            "ticket-consume-again",
            vec![AuthorityChangeV1::PairingTicketConsumed {
                ticket_hash: ticket_hash.into(),

                signature: None,
            }],
            admin_key_source(DAY_PK),
        );
        assert_eq!(
            second,
            AuthorityMutationResultV1::RejectedBeforeCommit {
                mutation_id: "ticket-consume-again".into(),
                reason: AuthorityMutationRejectionReasonV1::InvalidRequest,
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let replay_after_reject = replay_authority_entries(&ledger.entries().unwrap()).unwrap();
        assert_eq!(
            replay_after_reject
                .state
                .pairing_tickets
                .get(ticket_hash)
                .unwrap()
                .state,
            PairingTicketStateV1::Consumed
        );

        let mut duplicated = entries;
        duplicated.push(duplicated.last().unwrap().clone());
        let replay_error = replay_authority_entries(&duplicated).unwrap_err();
        assert_eq!(
            replay_error.to_string(),
            "canonical authority fact is incoherent at sequence 2: canonical authority fact is incoherent at sequence 0: pairing ticket hash was already consumed"
        );
    }

    /// A mutation that revokes the recovery grant does not apply, including from a day-to-day key.
    #[test]
    fn recovery_grant_revoke_does_not_apply() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        let granted = commit_authority(
            &mut ledger,
            "admin-grants",
            vec![
                AuthorityChangeV1::AdminKeyGrantPut {
                    grant: Box::new(day_to_day_admin_grant()),

                    signature: None,
                },
                AuthorityChangeV1::AdminKeyGrantPut {
                    grant: Box::new(recovery_admin_grant()),

                    signature: None,
                },
            ],
            admin_key_source(RECOVERY_PK),
        );
        assert_pending_commit(&granted, "admin-grants", 1, &ledger);

        let before = std::fs::read(&path).unwrap();
        let revoke = commit_authority(
            &mut ledger,
            "revoke-recovery",
            vec![AuthorityChangeV1::AdminKeyGrantRevoke {
                grant_id: "recovery".into(),

                signature: None,
            }],
            admin_key_source(DAY_PK),
        );
        assert_eq!(
            revoke,
            AuthorityMutationResultV1::RejectedBeforeCommit {
                mutation_id: "revoke-recovery".into(),
                reason: AuthorityMutationRejectionReasonV1::InvalidRequest,
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);

        let entries = ledger.entries().unwrap();
        let replay = replay_authority_entries(&entries).unwrap();
        let recovery = replay.state.admin_key_grants.get("recovery").unwrap();
        assert_eq!(recovery.grant_state, AdminKeyGrantStateV1::Active);
        assert_eq!(recovery.class, AdminKeyGrantClassV1::Recovery);
        assert_eq!(recovery, &recovery_admin_grant());

        let fact_id = "obs:authority-mutation:revoke-recovery";
        let fact = AuthorityMutationFactV1 {
            mutation_id: "revoke-recovery".into(),
            mutation_intent_hash: "not-compared-when-apply-refuses".into(),
            mother_node_id: "mother-a".into(),
            ledger_id: "ledger-a".into(),
            authority_epoch: replay
                .current_authority
                .as_ref()
                .unwrap()
                .authority_epoch
                .clone(),
            prior_state: replay
                .mutations
                .get("admin-grants")
                .unwrap()
                .fact
                .resulting_state
                .clone(),
            changes: vec![AuthorityChangeV1::AdminKeyGrantRevoke {
                grant_id: "recovery".into(),

                signature: None,
            }],
            grant_shaping_sources: vec![admin_key_source(DAY_PK)],
            resulting_state: replay
                .mutations
                .get("admin-grants")
                .unwrap()
                .fact
                .resulting_state
                .clone(),
            decided_at: "2026-08-06T12:05:00Z".into(),
        };
        let mut revoked_entry = entries.last().unwrap().clone();
        revoked_entry.observation.observation_id = ObservationId::new(fact_id).unwrap();
        revoked_entry.observation.detail_ref =
            Some(encode_authority_fact("authority_mutation", fact_id, &fact).unwrap());
        let mut with_revoke = entries;
        with_revoke.push(revoked_entry);
        let replay_error = replay_authority_entries(&with_revoke).unwrap_err();
        assert_eq!(
            replay_error.to_string(),
            "canonical authority fact is incoherent at sequence 1: canonical authority fact is incoherent at sequence 0: recovery grant cannot be revoked"
        );
    }

    /// Replayed phase-1 grants are what the route-scope check reads.
    #[test]
    fn replayed_admin_grants_keep_recovery_locked_and_day_to_day_expiry() {
        use mct_kernel::{
            AdminKeyCheckAction, AdminKeyGrantCheckRequest, AdminKeyGrantCheckResult,
            AdminKeyGrantReason, AdminKeyGrantVerdict, AdminKeyScopeV1, evaluate_admin_key_grant,
        };

        fn allow(grant_id: &str, scope: AdminKeyScopeV1) -> AdminKeyGrantCheckResult {
            AdminKeyGrantCheckResult {
                verdict: AdminKeyGrantVerdict::Allowed,
                reason: AdminKeyGrantReason::ActiveGrant,
                grant_id: Some(grant_id.to_owned()),
                admitted_scope: Some(scope),
            }
        }
        fn deny(reason: AdminKeyGrantReason, grant_id: Option<&str>) -> AdminKeyGrantCheckResult {
            AdminKeyGrantCheckResult {
                verdict: AdminKeyGrantVerdict::Denied,
                reason,
                grant_id: grant_id.map(str::to_owned),
                admitted_scope: None,
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        commit_authority(
            &mut ledger,
            "admin-grants",
            vec![
                AuthorityChangeV1::AdminKeyGrantPut {
                    grant: Box::new(day_to_day_admin_grant()),

                    signature: None,
                },
                AuthorityChangeV1::AdminKeyGrantPut {
                    grant: Box::new(recovery_admin_grant()),

                    signature: None,
                },
            ],
            admin_key_source(RECOVERY_PK),
        );
        let replay = replay_authority_entries(&ledger.entries().unwrap()).unwrap();
        let grants: Vec<_> = replay.state.admin_key_grants.values().cloned().collect();
        let generation = replay.current_authority.unwrap().generation;
        assert_eq!(generation, 1);
        let check = |public_half: &str, action: AdminKeyCheckAction| {
            evaluate_admin_key_grant(
                &AdminKeyGrantCheckRequest {
                    public_half: half(public_half),
                    action,
                    now: Timestamp::new("2026-08-06T12:00:00Z").unwrap(),
                    presented_generation: generation,
                    current_generation: generation,
                },
                &grants,
            )
        };

        assert_eq!(
            check(DAY_PK, AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),),
            allow("day-to-day", AdminKeyScopeV1::Status)
        );
        assert_eq!(
            check(
                DAY_PK,
                AdminKeyCheckAction::Route(AdminKeyScopeV1::StartupGate),
            ),
            deny(AdminKeyGrantReason::WrongScope, Some("day-to-day"))
        );
        assert_eq!(
            check(
                RECOVERY_PK,
                AdminKeyCheckAction::Route(AdminKeyScopeV1::StartupGate),
            ),
            allow("recovery", AdminKeyScopeV1::StartupGate)
        );
        assert_eq!(
            check(
                RECOVERY_PK,
                AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),
            ),
            deny(AdminKeyGrantReason::WrongScope, Some("recovery"))
        );
        assert_eq!(
            check(
                DAY_PK,
                AdminKeyCheckAction::Revoke {
                    target_grant_id: "recovery".into(),
                },
            ),
            deny(AdminKeyGrantReason::RecoveryGrantLocked, Some("recovery"))
        );
        assert_eq!(
            evaluate_admin_key_grant(
                &AdminKeyGrantCheckRequest {
                    public_half: half(DAY_PK),
                    action: AdminKeyCheckAction::Route(AdminKeyScopeV1::Status),
                    now: Timestamp::new("2026-10-14T00:00:00Z").unwrap(),
                    presented_generation: generation,
                    current_generation: generation,
                },
                &grants,
            ),
            deny(AdminKeyGrantReason::ExpiredGrant, Some("day-to-day"))
        );
    }

    /// A day-to-day grant without expiry is refused at admission and on replay.
    #[test]
    fn day_to_day_grant_without_expiry_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        let mut permanent = day_to_day_admin_grant();
        permanent.expires_at = None;
        let before = std::fs::read(&path).unwrap();
        let rejected = commit_authority(
            &mut ledger,
            "permanent-day",
            vec![AuthorityChangeV1::AdminKeyGrantPut {
                grant: Box::new(permanent.clone()),

                signature: None,
            }],
            admin_key_source(RECOVERY_PK),
        );
        assert_eq!(
            rejected,
            AuthorityMutationResultV1::RejectedBeforeCommit {
                mutation_id: "permanent-day".into(),
                reason: AuthorityMutationRejectionReasonV1::InvalidRequest,
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);

        let granted = commit_authority(
            &mut ledger,
            "admin-grants",
            vec![AuthorityChangeV1::AdminKeyGrantPut {
                grant: Box::new(recovery_admin_grant()),

                signature: None,
            }],
            admin_key_source(RECOVERY_PK),
        );
        assert_pending_commit(&granted, "admin-grants", 1, &ledger);
        let entries = ledger.entries().unwrap();
        let fact_id = "obs:authority-mutation:permanent-day";
        let prior_replay = replay_authority_entries(&entries).unwrap();
        let fact = AuthorityMutationFactV1 {
            mutation_id: "permanent-day".into(),
            mutation_intent_hash: "not-compared-when-apply-refuses".into(),
            mother_node_id: "mother-a".into(),
            ledger_id: "ledger-a".into(),
            authority_epoch: prior_replay
                .current_authority
                .as_ref()
                .unwrap()
                .authority_epoch
                .clone(),
            prior_state: prior_replay
                .mutations
                .get("admin-grants")
                .unwrap()
                .fact
                .resulting_state
                .clone(),
            changes: vec![AuthorityChangeV1::AdminKeyGrantPut {
                grant: Box::new(permanent),

                signature: None,
            }],
            grant_shaping_sources: vec![admin_key_source(RECOVERY_PK)],
            resulting_state: prior_replay
                .mutations
                .get("admin-grants")
                .unwrap()
                .fact
                .resulting_state
                .clone(),
            decided_at: "2026-08-06T12:05:00Z".into(),
        };
        let mut bad = entries.last().unwrap().clone();
        bad.observation.observation_id = ObservationId::new(fact_id).unwrap();
        bad.observation.detail_ref =
            Some(encode_authority_fact("authority_mutation", fact_id, &fact).unwrap());
        let mut with_bad = entries;
        with_bad.push(bad);
        let replay_error = replay_authority_entries(&with_bad).unwrap_err();
        assert_eq!(
            replay_error.to_string(),
            "canonical authority fact is incoherent at sequence 1: canonical authority fact is incoherent at sequence 0: day-to-day admin key grant requires expiry"
        );
    }

    /// A revoked day-to-day grant stays revoked after replay and cannot be put again.
    #[test]
    fn revoked_day_to_day_grant_stays_revoked_after_replay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        let granted = commit_authority(
            &mut ledger,
            "admin-grants",
            vec![
                AuthorityChangeV1::AdminKeyGrantPut {
                    grant: Box::new(day_to_day_admin_grant()),

                    signature: None,
                },
                AuthorityChangeV1::AdminKeyGrantPut {
                    grant: Box::new(recovery_admin_grant()),

                    signature: None,
                },
            ],
            admin_key_source(RECOVERY_PK),
        );
        assert_pending_commit(&granted, "admin-grants", 1, &ledger);
        let revoked = commit_authority(
            &mut ledger,
            "revoke-day",
            vec![AuthorityChangeV1::AdminKeyGrantRevoke {
                grant_id: "day-to-day".into(),

                signature: None,
            }],
            admin_key_source(RECOVERY_PK),
        );
        assert_pending_commit(&revoked, "revoke-day", 2, &ledger);

        let entries = ledger.entries().unwrap();
        assert!(entries.len() >= 3);
        for window in entries.windows(2) {
            assert_eq!(
                window[1].previous_entry_hash.as_deref(),
                Some(window[0].entry_hash.as_str())
            );
        }

        let replay = replay_authority_entries(&entries).unwrap();
        let day = replay.state.admin_key_grants.get("day-to-day").unwrap();
        assert_eq!(day.grant_state, AdminKeyGrantStateV1::Revoked);
        assert_eq!(day.class, AdminKeyGrantClassV1::DayToDay);
        assert_eq!(day.public_half, half(DAY_PK));
        assert_eq!(
            replay
                .state
                .admin_key_grants
                .get("recovery")
                .unwrap()
                .grant_state,
            AdminKeyGrantStateV1::Active
        );

        let before = std::fs::read(&path).unwrap();
        let resurrect = commit_authority(
            &mut ledger,
            "resurrect-day",
            vec![AuthorityChangeV1::AdminKeyGrantPut {
                grant: Box::new(day_to_day_admin_grant()),

                signature: None,
            }],
            admin_key_source(RECOVERY_PK),
        );
        assert_eq!(
            resurrect,
            AuthorityMutationResultV1::RejectedBeforeCommit {
                mutation_id: "resurrect-day".into(),
                reason: AuthorityMutationRejectionReasonV1::InvalidRequest,
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let replay_after = replay_authority_entries(&ledger.entries().unwrap()).unwrap();
        assert_eq!(
            replay_after
                .state
                .admin_key_grants
                .get("day-to-day")
                .unwrap()
                .grant_state,
            AdminKeyGrantStateV1::Revoked
        );
    }

    /// Admin-key and pairing-ticket mutations keep the blake3 previous-hash chain intact.
    #[test]
    fn admin_key_and_pairing_ticket_mutations_keep_hash_chain_intact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        let ticket_hash = "abcdef01";
        assert_pending_commit(
            &commit_authority(
                &mut ledger,
                "admin-grants",
                vec![
                    AuthorityChangeV1::AdminKeyGrantPut {
                        grant: Box::new(day_to_day_admin_grant()),

                        signature: None,
                    },
                    AuthorityChangeV1::AdminKeyGrantPut {
                        grant: Box::new(recovery_admin_grant()),

                        signature: None,
                    },
                ],
                admin_key_source(RECOVERY_PK),
            ),
            "admin-grants",
            1,
            &ledger,
        );
        assert_pending_commit(
            &commit_authority(
                &mut ledger,
                "ticket-issue",
                vec![AuthorityChangeV1::PairingTicketIssued {
                    ticket_hash: ticket_hash.into(),

                    peer_endpoint_id: None,
                    window_starts_at: None,
                    window_expires_at: None,
                    signature: None,
                }],
                admin_key_source(DAY_PK),
            ),
            "ticket-issue",
            2,
            &ledger,
        );
        assert_pending_commit(
            &commit_authority(
                &mut ledger,
                "ticket-consume",
                vec![AuthorityChangeV1::PairingTicketConsumed {
                    ticket_hash: ticket_hash.into(),

                    signature: None,
                }],
                admin_key_source(DAY_PK),
            ),
            "ticket-consume",
            3,
            &ledger,
        );
        assert_pending_commit(
            &commit_authority(
                &mut ledger,
                "revoke-day",
                vec![AuthorityChangeV1::AdminKeyGrantRevoke {
                    grant_id: "day-to-day".into(),

                    signature: None,
                }],
                admin_key_source(RECOVERY_PK),
            ),
            "revoke-day",
            4,
            &ledger,
        );

        let entries = ledger.entries().unwrap();
        assert!(entries[0].previous_entry_hash.is_none());
        for window in entries.windows(2) {
            assert_eq!(
                window[1].previous_entry_hash.as_deref(),
                Some(window[0].entry_hash.as_str()),
                "broken hash link at sequence {}",
                window[1].local_sequence
            );
        }
        let replay = replay_authority_entries(&entries).unwrap();
        assert_eq!(
            replay
                .state
                .admin_key_grants
                .get("day-to-day")
                .unwrap()
                .grant_state,
            AdminKeyGrantStateV1::Revoked
        );
        assert_eq!(
            replay.state.pairing_tickets.get(ticket_hash).unwrap().state,
            PairingTicketStateV1::Consumed
        );
        assert_eq!(replay.current_authority.unwrap().generation, 4);

        drop(ledger);
        let reader = JsonlObservationLedger::open_read_only(&path, "ledger-a", "mother-a").unwrap();
        let reopened_entries = reader.entries().unwrap();
        assert_eq!(reopened_entries.len(), entries.len());
        for (left, right) in entries.iter().zip(reopened_entries.iter()) {
            assert_eq!(left.entry_hash, right.entry_hash);
            assert_eq!(left.previous_entry_hash, right.previous_entry_hash);
        }
        let reopened_replay = replay_authority_entries(&reopened_entries).unwrap();
        assert_eq!(reopened_replay.state, replay.state);
        assert_eq!(
            reopened_replay
                .current_authority
                .as_ref()
                .unwrap()
                .generation,
            4
        );
    }

    /// Existing toy-grant replay still succeeds beside the admin-key facts.
    #[test]
    fn toy_grant_replay_still_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        let mut ledger =
            JsonlObservationLedger::open_authority(&path, "ledger-a", "mother-a").unwrap();
        let result = ledger
            .execute_authority_mutation(authority_mutation_request("toy-grant-replay"), |_| {
                Ok(None)
            });
        assert_pending_commit(&result, "toy-grant-replay", 1, &ledger);
        let replay = replay_authority_entries(&ledger.entries().unwrap()).unwrap();
        let grant = replay.state.toy_grants.get("grant-a").unwrap();
        assert_eq!(grant.grant_id.as_str(), "grant-a");
        assert_eq!(grant.toy_id.as_str(), "toy-a");
        assert_eq!(grant.subject.child_name, "child-a");
        assert_eq!(grant.scope.allowed_actions, vec!["run".to_owned()]);
        assert_eq!(grant.scope.resource_id.as_deref(), Some("resource-a"));
        assert_eq!(grant.grant_state, ToyGrantState::Active);
        assert_eq!(grant.issuer_id, "mother-a");
        assert_eq!(grant.grants_revision, 1);
        assert_eq!(replay.state.admin_key_grants, BTreeMap::new());
        assert_eq!(replay.state.pairing_tickets, BTreeMap::new());
        assert_eq!(replay.current_authority.unwrap().generation, 1);
    }
}

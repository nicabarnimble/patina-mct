//! Append-only observation ledger support for MCT.
//!
//! Runtime truth starts from `MctObservation` facts defined by `mct-kernel`.
//! Storage details stay in this crate and do not leak into the kernel.

#![forbid(unsafe_code)]

use mct_kernel::{
    CallId, MctObservation, ObservationId, ObservationKind, ObservationOutcome,
    ObservationTraceRef, ObservationVisibility, SourcePlane, Timestamp, TraceId,
};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

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
    #[error("observation ledger writer lock error at {path}: {source}")]
    WriterLock {
        path: PathBuf,
        #[source]
        source: std::io::Error,
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
        observations
            .into_iter()
            .map(|observation| self.append_before_effect(observation, appended_at.clone()))
            .collect()
    }

    pub fn append(
        &mut self,
        observation: MctObservation,
        appended_at: impl Into<String>,
        durability_class: DurabilityClass,
        export_status: ExportStatus,
    ) -> Result<MctObservationLedgerEntry> {
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

        let line =
            serde_json::to_string(&entry).map_err(|source| ObservationLedgerError::Json {
                path: self.path.clone(),
                source,
            })?;
        writeln!(self.file, "{line}").map_err(|source| ObservationLedgerError::Io {
            path: self.path.clone(),
            source,
        })?;
        self.file
            .sync_data()
            .map_err(|source| ObservationLedgerError::Io {
                path: self.path.clone(),
                source,
            })?;

        self.previous_hash = Some(entry.entry_hash.clone());
        self.next_sequence += 1;
        Ok(entry)
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
    file.try_lock()
        .map_err(|source| ObservationLedgerError::WriterLock {
            path: path.to_path_buf(),
            source: lock_error_to_io(source),
        })?;
    Ok(file)
}

fn lock_error_to_io(error: std::fs::TryLockError) -> std::io::Error {
    match error {
        std::fs::TryLockError::WouldBlock => std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "observation ledger is already locked by another writer",
        ),
        std::fs::TryLockError::Error(source) => source,
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
            Err(ObservationLedgerError::WriterLock { .. })
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

    #[test]
    fn opening_directory_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let result = JsonlObservationLedger::open(dir.path(), "ledger-a", "mother-a");
        assert!(matches!(result, Err(ObservationLedgerError::Io { .. })));
    }
}

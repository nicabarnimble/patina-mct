//! Append-only observation ledger support for MCT.
//!
//! Runtime truth starts from `MctObservation` facts defined by `mct-kernel`.
//! Storage details stay in this crate and do not leak into the kernel.

#![forbid(unsafe_code)]

use mct_kernel::{CallId, MctObservation, TraceId};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
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
}

impl JsonlObservationLedger {
    pub fn open(
        path: impl AsRef<Path>,
        ledger_id: impl Into<String>,
        mother_node_id: impl Into<String>,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let ledger_id = ledger_id.into();
        let mother_node_id = mother_node_id.into();

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

        let file = acquire_writer_lock(&path, file)?;
        let scan_result = scan_existing(&path, &ledger_id, &mother_node_id);
        let (next_sequence, previous_hash) = match scan_result {
            Ok(state) => state,
            Err(error) => {
                drop(file);
                return Err(error);
            }
        };

        Ok(Self {
            path,
            file,
            ledger_id,
            mother_node_id,
            next_sequence,
            previous_hash,
        })
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

    pub fn entries(&self) -> Result<Vec<MctObservationLedgerEntry>> {
        let entries = read_entries(&self.path)?;
        validate_entries(&entries, &self.ledger_id, &self.mother_node_id)?;
        Ok(entries)
    }

    pub fn by_trace(&self, trace_id: &TraceId) -> Result<Vec<MctObservationLedgerEntry>> {
        Ok(self
            .entries()?
            .into_iter()
            .filter(|entry| &entry.observation.trace.trace_id == trace_id)
            .collect())
    }

    pub fn by_call(&self, call_id: &CallId) -> Result<Vec<MctObservationLedgerEntry>> {
        Ok(self
            .entries()?
            .into_iter()
            .filter(|entry| entry.observation.call_id.as_ref() == Some(call_id))
            .collect())
    }
}

fn scan_existing(
    path: &Path,
    ledger_id: &str,
    mother_node_id: &str,
) -> Result<(u64, Option<String>)> {
    let entries = read_entries(path)?;
    validate_entries(&entries, ledger_id, mother_node_id)
}

fn validate_entries(
    entries: &[MctObservationLedgerEntry],
    ledger_id: &str,
    mother_node_id: &str,
) -> Result<(u64, Option<String>)> {
    let mut previous_hash = None;
    let mut expected_sequence = 0;
    for entry in entries {
        if entry.local_sequence != expected_sequence {
            return Err(ObservationLedgerError::SequenceMismatch {
                expected: expected_sequence,
                actual: entry.local_sequence,
            });
        }

        if entry.ledger_id != ledger_id || entry.mother_node_id != mother_node_id {
            return Err(ObservationLedgerError::LedgerIdentityMismatch {
                sequence: entry.local_sequence,
                expected_ledger_id: ledger_id.to_owned(),
                expected_mother_node_id: mother_node_id.to_owned(),
                actual_ledger_id: entry.ledger_id.clone(),
                actual_mother_node_id: entry.mother_node_id.clone(),
            });
        }

        if entry.previous_entry_hash != previous_hash {
            return Err(ObservationLedgerError::BrokenHashChain {
                sequence: entry.local_sequence,
            });
        }
        let expected = entry_hash(entry)?;
        if entry.entry_hash != expected {
            return Err(ObservationLedgerError::BrokenHashChain {
                sequence: entry.local_sequence,
            });
        }
        previous_hash = Some(entry.entry_hash.clone());
        expected_sequence += 1;
    }
    Ok((expected_sequence, previous_hash))
}

fn read_entries(path: &Path) -> Result<Vec<MctObservationLedgerEntry>> {
    let file = File::open(path).map_err(|source| ObservationLedgerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }
        entries.push(serde_json::from_str(&line).map_err(|source| {
            ObservationLedgerError::Json {
                path: path.to_path_buf(),
                source,
            }
        })?);
    }
    Ok(entries)
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
        assert_eq!(super::version(), "0.1.0");
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
        assert_eq!(entries.len(), 1);
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
            Err(ObservationLedgerError::LedgerIdentityMismatch { sequence: 0, .. })
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

    #[test]
    fn opening_directory_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let result = JsonlObservationLedger::open(dir.path(), "ledger-a", "mother-a");
        assert!(matches!(result, Err(ObservationLedgerError::Io { .. })));
    }
}

//! Checkpointed ledger segments.
//!
//! A sealed segment is a copy of a chain-valid log plus one trailing
//! `StorageAppendSucceeded` observation. That observation's `detail_ref` binds
//! the prefix length, entry count, and BLAKE3 digest of the bytes that precede
//! it. [`crate::JsonlObservationLedger::open`] does not read this layout.
//! Rotation of the live store stays off unless a caller opts in through
//! [`verify_segmented_ledger_if_enabled`].

use crate::{
    DurabilityClass, ExportStatus, MctObservationLedgerEntry, ObservationLedgerError, Result,
    entry_hash,
};
use mct_kernel::{
    MctObservation, ObservationId, ObservationKind, ObservationOutcome, ObservationTraceRef,
    ObservationVisibility, SourcePlane, Timestamp, TraceId,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

/// Prefix of a segment checkpoint carried in `MctObservation.detail_ref`.
pub const SEGMENT_CHECKPOINT_PREFIX: &str = "mct-ledger-segment-checkpoint-v1:";

const SEGMENT_CHECKPOINT_SCHEMA: &str = "mct-ledger-segment-checkpoint/v1";

/// Commitment to the bytes that precede a segment's checkpoint observation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentCheckpointV1 {
    pub schema: String,
    pub segment_id: String,
    pub entry_count: u64,
    pub last_sequence: u64,
    pub last_entry_hash: String,
    pub segment_blake3: String,
}

/// A sealed segment file and the checkpoint observation that closes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedSegment {
    pub path: PathBuf,
    pub checkpoint: SegmentCheckpointV1,
    pub checkpoint_sequence: u64,
    pub checkpoint_entry_hash: String,
    pub first_sequence: u64,
    pub first_previous_entry_hash: Option<String>,
}

/// Hash-chain verification of `seg-NNNNNN.jsonl` files followed by `open.jsonl`.
#[derive(Clone, Debug)]
pub struct SegmentedLedgerReport {
    pub segments: Vec<SealedSegment>,
    pub open_entries: u64,
    pub open_head_sequence: Option<u64>,
    pub open_head_hash: Option<String>,
}

/// `MCT_LEDGER_SEGMENTS=1` or `true` opts a caller into segmented verification.
///
/// The live writer ignores this variable.
pub fn ledger_segments_enabled() -> bool {
    matches!(
        std::env::var("MCT_LEDGER_SEGMENTS").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE")
    )
}

/// Copy `source` to `destination` and append one checkpoint observation.
///
/// `source` is left unchanged. The copy is the sealed segment. This does not
/// rotate or archive the live ledger.
pub fn seal_segment(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    segment_id: &str,
    ledger_id: &str,
    mother_node_id: &str,
    sealed_at: &str,
) -> Result<SealedSegment> {
    let source = source.as_ref();
    let destination = destination.as_ref();
    if segment_id.trim().is_empty() || segment_id.contains(['/', '\\']) {
        return Err(ObservationLedgerError::SegmentRejected {
            detail: "segment id must be a non-blank path-free token".into(),
        });
    }
    if source == destination {
        return Err(ObservationLedgerError::SegmentRejected {
            detail: "segment destination must be a distinct copy".into(),
        });
    }
    let body = stream_chain(source, ledger_id, mother_node_id, None)?;
    if body.entry_count == 0 {
        return Err(ObservationLedgerError::SegmentRejected {
            detail: "empty ledger cannot be sealed".into(),
        });
    }
    let checkpoint = SegmentCheckpointV1 {
        schema: SEGMENT_CHECKPOINT_SCHEMA.into(),
        segment_id: segment_id.to_owned(),
        entry_count: body.entry_count,
        last_sequence: body.last_sequence,
        last_entry_hash: body.last_entry_hash.clone(),
        segment_blake3: body.file_blake3.clone(),
    };
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|source| ObservationLedgerError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::copy(source, destination).map_err(|io| ObservationLedgerError::Io {
        path: destination.to_path_buf(),
        source: io,
    })?;
    let observation = checkpoint_observation(
        segment_id,
        ledger_id,
        mother_node_id,
        sealed_at,
        &checkpoint,
    )?;
    let next_sequence =
        body.last_sequence
            .checked_add(1)
            .ok_or(ObservationLedgerError::SegmentRejected {
                detail: "segment sequence overflow".into(),
            })?;
    append_checkpoint_frame(
        destination,
        ledger_id,
        mother_node_id,
        next_sequence,
        Some(body.last_entry_hash),
        sealed_at,
        observation,
    )?;
    verify_sealed_segment(destination, ledger_id, mother_node_id)
}

/// Stream-verify one sealed segment, including its checkpoint observation.
pub fn verify_sealed_segment(
    path: impl AsRef<Path>,
    ledger_id: &str,
    mother_node_id: &str,
) -> Result<SealedSegment> {
    let path = path.as_ref();
    let streamed = stream_chain(path, ledger_id, mother_node_id, None)?;
    if streamed.entry_count < 2 {
        return Err(ObservationLedgerError::SegmentRejected {
            detail: "sealed segment is missing its checkpoint observation".into(),
        });
    }
    let checkpoint_entry =
        streamed
            .last_entry
            .as_ref()
            .ok_or(ObservationLedgerError::SegmentRejected {
                detail: "sealed segment has no checkpoint frame".into(),
            })?;
    let checkpoint = decode_checkpoint(checkpoint_entry)?;
    if checkpoint.entry_count != streamed.entry_count - 1 {
        return Err(ObservationLedgerError::SegmentRejected {
            detail: "checkpoint entry count does not match the prefix".into(),
        });
    }
    if checkpoint.last_sequence != streamed.body_last_sequence
        || checkpoint.last_entry_hash != streamed.body_last_hash
    {
        return Err(ObservationLedgerError::SegmentRejected {
            detail: "checkpoint does not bind the prefix head".into(),
        });
    }
    if checkpoint.segment_blake3 != streamed.prefix_blake3 {
        return Err(ObservationLedgerError::SegmentRejected {
            detail: "checkpoint digest does not match the prefix bytes".into(),
        });
    }
    let expected_checkpoint_sequence =
        checkpoint
            .last_sequence
            .checked_add(1)
            .ok_or(ObservationLedgerError::SegmentRejected {
                detail: "segment sequence overflow".into(),
            })?;
    if checkpoint_entry.local_sequence != expected_checkpoint_sequence {
        return Err(ObservationLedgerError::SequenceMismatch {
            expected: expected_checkpoint_sequence,
            actual: checkpoint_entry.local_sequence,
        });
    }
    if checkpoint_entry.previous_entry_hash.as_deref() != Some(checkpoint.last_entry_hash.as_str())
    {
        return Err(ObservationLedgerError::BrokenHashChain {
            sequence: checkpoint_entry.local_sequence,
        });
    }
    Ok(SealedSegment {
        path: path.to_path_buf(),
        checkpoint,
        checkpoint_sequence: checkpoint_entry.local_sequence,
        checkpoint_entry_hash: checkpoint_entry.entry_hash.clone(),
        first_sequence: streamed.first_sequence,
        first_previous_entry_hash: streamed.first_previous_entry_hash,
    })
}

/// Verify `seg-000000.jsonl` … then `open.jsonl` as one hash chain.
///
/// The next file's first entry must name the previous segment's checkpoint
/// entry hash and the following sequence. This function does not consult
/// `MCT_LEDGER_SEGMENTS`.
pub fn verify_segmented_ledger(
    dir: impl AsRef<Path>,
    ledger_id: &str,
    mother_node_id: &str,
) -> Result<SegmentedLedgerReport> {
    let dir = dir.as_ref();
    let paths = segment_paths(dir)?;
    let mut segments = Vec::with_capacity(paths.len());
    let mut expected_sequence = 0_u64;
    let mut expected_previous: Option<String> = None;
    for path in &paths {
        let sealed = verify_sealed_segment(path, ledger_id, mother_node_id)?;
        expect_link(
            sealed.first_sequence,
            sealed.first_previous_entry_hash.as_deref(),
            expected_sequence,
            expected_previous.as_deref(),
        )?;
        expected_sequence = sealed.checkpoint_sequence.checked_add(1).ok_or(
            ObservationLedgerError::SegmentRejected {
                detail: "segment sequence overflow".into(),
            },
        )?;
        expected_previous = Some(sealed.checkpoint_entry_hash.clone());
        segments.push(sealed);
    }
    let open_path = dir.join("open.jsonl");
    let open = stream_chain(
        &open_path,
        ledger_id,
        mother_node_id,
        Some((expected_sequence, expected_previous.clone())),
    )?;
    if open.entry_count > 0 {
        expect_link(
            open.first_sequence,
            open.first_previous_entry_hash.as_deref(),
            expected_sequence,
            expected_previous.as_deref(),
        )?;
    }
    Ok(SegmentedLedgerReport {
        segments,
        open_entries: open.entry_count,
        open_head_sequence: (open.entry_count > 0).then_some(open.last_sequence),
        open_head_hash: (open.entry_count > 0).then_some(open.last_entry_hash),
    })
}

/// Verify a segmented directory only when [`ledger_segments_enabled`] is set.
pub fn verify_segmented_ledger_if_enabled(
    dir: impl AsRef<Path>,
    ledger_id: &str,
    mother_node_id: &str,
) -> Result<SegmentedLedgerReport> {
    verify_segmented_ledger_gated(dir, ledger_id, mother_node_id, ledger_segments_enabled())
}

fn verify_segmented_ledger_gated(
    dir: impl AsRef<Path>,
    ledger_id: &str,
    mother_node_id: &str,
    enabled: bool,
) -> Result<SegmentedLedgerReport> {
    if !enabled {
        return Err(ObservationLedgerError::SegmentsDisabled);
    }
    verify_segmented_ledger(dir, ledger_id, mother_node_id)
}

struct StreamedChain {
    entry_count: u64,
    first_sequence: u64,
    first_previous_entry_hash: Option<String>,
    last_sequence: u64,
    last_entry_hash: String,
    last_entry: Option<MctObservationLedgerEntry>,
    body_last_sequence: u64,
    body_last_hash: String,
    prefix_blake3: String,
    file_blake3: String,
}

fn stream_chain(
    path: &Path,
    ledger_id: &str,
    mother_node_id: &str,
    expected_start: Option<(u64, Option<String>)>,
) -> Result<StreamedChain> {
    let file = File::open(path).map_err(|source| ObservationLedgerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();
    let mut file_hasher = blake3::Hasher::new();
    let mut frame = Vec::new();
    let mut pending: Option<Vec<u8>> = None;
    let mut entry_count = 0_u64;
    let mut adopt_first = expected_start.is_none();
    let mut expected_sequence = expected_start
        .as_ref()
        .map(|(sequence, _)| *sequence)
        .unwrap_or(0);
    let mut previous_hash = expected_start.and_then(|(_, hash)| hash);
    let mut first_sequence = None;
    let mut first_previous = None;
    let mut last_sequence = 0_u64;
    let mut last_entry_hash = String::new();
    let mut body_last_sequence = 0_u64;
    let mut body_last_hash = String::new();
    let mut last_entry = None;

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
        if !frame.ends_with(b"\n") {
            return Err(ObservationLedgerError::UnterminatedTail {
                path: path.to_path_buf(),
                offset: entry_count,
                length: frame.len() as u64,
                digest: blake3::hash(&frame).to_hex().to_string(),
            });
        }
        file_hasher.update(&frame);
        if let Some(previous) = pending.take() {
            hasher.update(&previous);
            let entry = parse_and_check(
                path,
                ledger_id,
                mother_node_id,
                &previous,
                expected_sequence,
                previous_hash.as_deref(),
                &mut adopt_first,
            )?;
            if first_sequence.is_none() {
                first_sequence = Some(entry.local_sequence);
                first_previous = entry.previous_entry_hash.clone();
            }
            body_last_sequence = entry.local_sequence;
            body_last_hash = entry.entry_hash.clone();
            expected_sequence = entry.local_sequence.checked_add(1).ok_or(
                ObservationLedgerError::SegmentRejected {
                    detail: "segment sequence overflow".into(),
                },
            )?;
            previous_hash = Some(entry.entry_hash);
            entry_count += 1;
        }
        pending = Some(frame.clone());
    }

    if let Some(last) = pending {
        let entry = parse_and_check(
            path,
            ledger_id,
            mother_node_id,
            &last,
            expected_sequence,
            previous_hash.as_deref(),
            &mut adopt_first,
        )?;
        if first_sequence.is_none() {
            first_sequence = Some(entry.local_sequence);
            first_previous = entry.previous_entry_hash.clone();
        }
        last_sequence = entry.local_sequence;
        last_entry_hash = entry.entry_hash.clone();
        last_entry = Some(entry);
        entry_count += 1;
    }

    if entry_count == 0 {
        return Ok(StreamedChain {
            entry_count: 0,
            first_sequence: expected_sequence,
            first_previous_entry_hash: previous_hash,
            last_sequence: 0,
            last_entry_hash: String::new(),
            last_entry: None,
            body_last_sequence: 0,
            body_last_hash: String::new(),
            prefix_blake3: blake3::Hasher::new().finalize().to_hex().to_string(),
            file_blake3: file_hasher.finalize().to_hex().to_string(),
        });
    }

    Ok(StreamedChain {
        entry_count,
        first_sequence: first_sequence.unwrap_or(0),
        first_previous_entry_hash: first_previous,
        last_sequence,
        last_entry_hash,
        last_entry,
        body_last_sequence,
        body_last_hash,
        prefix_blake3: hasher.finalize().to_hex().to_string(),
        file_blake3: file_hasher.finalize().to_hex().to_string(),
    })
}

fn parse_and_check(
    path: &Path,
    ledger_id: &str,
    mother_node_id: &str,
    frame: &[u8],
    expected_sequence: u64,
    previous_hash: Option<&str>,
    adopt_first: &mut bool,
) -> Result<MctObservationLedgerEntry> {
    let entry: MctObservationLedgerEntry = serde_json::from_slice(&frame[..frame.len() - 1])
        .map_err(|source| ObservationLedgerError::Json {
            path: path.to_path_buf(),
            source,
        })?;
    let (expected_sequence, previous_owned) = if *adopt_first {
        *adopt_first = false;
        (entry.local_sequence, entry.previous_entry_hash.clone())
    } else {
        (expected_sequence, previous_hash.map(str::to_owned))
    };
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
            actual_ledger_id: entry.ledger_id,
            actual_mother_node_id: entry.mother_node_id,
        });
    }
    let linked = match (&entry.previous_entry_hash, previous_owned.as_deref()) {
        (None, None) => true,
        (Some(actual), Some(expected)) => actual == expected,
        _ => false,
    };
    if !linked {
        return Err(ObservationLedgerError::BrokenHashChain {
            sequence: entry.local_sequence,
        });
    }
    let expected_hash = entry_hash(&entry)?;
    if entry.entry_hash != expected_hash {
        return Err(ObservationLedgerError::BrokenHashChain {
            sequence: entry.local_sequence,
        });
    }
    Ok(entry)
}

fn decode_checkpoint(entry: &MctObservationLedgerEntry) -> Result<SegmentCheckpointV1> {
    if entry.observation.kind != ObservationKind::StorageAppendSucceeded {
        return Err(ObservationLedgerError::SegmentRejected {
            detail: "segment checkpoint observation has the wrong kind".into(),
        });
    }
    let Some(detail) = entry.observation.detail_ref.as_deref() else {
        return Err(ObservationLedgerError::SegmentRejected {
            detail: "segment checkpoint observation has no detail".into(),
        });
    };
    let Some(json) = detail.strip_prefix(SEGMENT_CHECKPOINT_PREFIX) else {
        return Err(ObservationLedgerError::SegmentRejected {
            detail: "segment checkpoint detail prefix is missing".into(),
        });
    };
    let checkpoint: SegmentCheckpointV1 =
        serde_json::from_str(json).map_err(|source| ObservationLedgerError::Json {
            path: PathBuf::from("<segment-checkpoint>"),
            source,
        })?;
    if checkpoint.schema != SEGMENT_CHECKPOINT_SCHEMA {
        return Err(ObservationLedgerError::SegmentRejected {
            detail: "segment checkpoint schema is not v1".into(),
        });
    }
    Ok(checkpoint)
}

fn checkpoint_observation(
    segment_id: &str,
    ledger_id: &str,
    mother_node_id: &str,
    sealed_at: &str,
    checkpoint: &SegmentCheckpointV1,
) -> Result<MctObservation> {
    let json =
        serde_json::to_string(checkpoint).map_err(|source| ObservationLedgerError::Json {
            path: PathBuf::from("<segment-checkpoint>"),
            source,
        })?;
    let observation_id = ObservationId::new(format!("ledger-segment-checkpoint-{segment_id}"))
        .map_err(|error| ObservationLedgerError::SegmentRejected {
            detail: error.to_string(),
        })?;
    let trace_id = TraceId::new(format!("trace-segment-{segment_id}")).map_err(|error| {
        ObservationLedgerError::SegmentRejected {
            detail: error.to_string(),
        }
    })?;
    let observed_at =
        Timestamp::new(sealed_at).map_err(|error| ObservationLedgerError::SegmentRejected {
            detail: error.to_string(),
        })?;
    Ok(MctObservation {
        observation_id,
        observed_at,
        kind: ObservationKind::StorageAppendSucceeded,
        source_plane: SourcePlane::Storage,
        trace: ObservationTraceRef {
            trace_id,
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
        safe_message: "observation ledger segment sealed".into(),
        detail_ref: Some(format!("{SEGMENT_CHECKPOINT_PREFIX}{json}")),
    })
}

fn append_checkpoint_frame(
    path: &Path,
    ledger_id: &str,
    mother_node_id: &str,
    sequence: u64,
    previous_entry_hash: Option<String>,
    appended_at: &str,
    observation: MctObservation,
) -> Result<MctObservationLedgerEntry> {
    let mut entry = MctObservationLedgerEntry {
        ledger_id: ledger_id.to_owned(),
        mother_node_id: mother_node_id.to_owned(),
        local_sequence: sequence,
        observation,
        previous_entry_hash,
        entry_hash: String::new(),
        appended_at: appended_at.to_owned(),
        durability_class: DurabilityClass::BeforeEffect,
        export_status: ExportStatus::NotRequired,
    };
    entry.entry_hash = entry_hash(&entry)?;
    let mut bytes = serde_json::to_vec(&entry).map_err(|source| ObservationLedgerError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    bytes.push(b'\n');
    let file = OpenOptions::new()
        .read(true)
        .append(true)
        .open(path)
        .map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let mut file = crate::acquire_writer_lock(path, file)?;
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
    Ok(entry)
}

fn expect_link(
    actual_sequence: u64,
    actual_previous: Option<&str>,
    expected_sequence: u64,
    expected_previous: Option<&str>,
) -> Result<()> {
    if actual_sequence != expected_sequence {
        return Err(ObservationLedgerError::SequenceMismatch {
            expected: expected_sequence,
            actual: actual_sequence,
        });
    }
    let linked = match (actual_previous, expected_previous) {
        (None, None) => true,
        (Some(actual), Some(expected)) => actual == expected,
        _ => false,
    };
    if !linked {
        return Err(ObservationLedgerError::BrokenHashChain {
            sequence: actual_sequence,
        });
    }
    Ok(())
}

fn segment_paths(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut indexed = Vec::new();
    let entries = fs::read_dir(dir).map_err(|source| ObservationLedgerError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ObservationLedgerError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name == "open.jsonl" || !name.starts_with("seg-") || !name.ends_with(".jsonl") {
            continue;
        }
        let Some(index) = segment_index(name) else {
            return Err(ObservationLedgerError::SegmentRejected {
                detail: format!("segment file `{name}` is not seg-NNNNNN.jsonl"),
            });
        };
        indexed.push((index, entry.path()));
    }
    indexed.sort_by_key(|(index, _)| *index);
    for (position, (index, _)) in indexed.iter().enumerate() {
        if *index != position as u64 {
            return Err(ObservationLedgerError::SegmentRejected {
                detail: format!("segment index {index} is not contiguous from 0"),
            });
        }
    }
    Ok(indexed.into_iter().map(|(_, path)| path).collect())
}

fn segment_index(name: &str) -> Option<u64> {
    let rest = name.strip_prefix("seg-")?.strip_suffix(".jsonl")?;
    if rest.len() != 6 || !rest.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write_benchmark_chain;
    use mct_kernel::{ObservationId, ObservationKind, Timestamp, TraceId};

    fn write_linked(path: &Path, start: u64, previous: Option<String>, count: u64) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = File::create(path).unwrap();
        let mut previous = previous;
        for offset in 0..count {
            let sequence = start + offset;
            let observation = MctObservation::informational(
                ObservationId::new(format!("obs-seg-{sequence}")).unwrap(),
                Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
                ObservationKind::PeerHelloReceived,
                TraceId::new("trace-seg").unwrap(),
                "segment body",
            );
            let mut entry = MctObservationLedgerEntry {
                ledger_id: "ledger-a".into(),
                mother_node_id: "mother-a".into(),
                local_sequence: sequence,
                observation,
                previous_entry_hash: previous.clone(),
                entry_hash: String::new(),
                appended_at: "2026-05-31T00:00:00Z".into(),
                durability_class: DurabilityClass::BeforeEffect,
                export_status: ExportStatus::NotRequired,
            };
            entry.entry_hash = entry_hash(&entry).unwrap();
            previous = Some(entry.entry_hash.clone());
            let mut frame = serde_json::to_vec(&entry).unwrap();
            frame.push(b'\n');
            file.write_all(&frame).unwrap();
        }
    }

    #[test]
    fn seal_copies_source_and_binds_prefix_digest() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.jsonl");
        write_benchmark_chain(&source, "ledger-a", "mother-a", 3).unwrap();
        let original = fs::read(&source).unwrap();
        let sealed_path = dir.path().join("seg-000000.jsonl");
        let sealed = seal_segment(
            &source,
            &sealed_path,
            "000000",
            "ledger-a",
            "mother-a",
            "2026-05-31T00:00:03Z",
        )
        .unwrap();
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(sealed.checkpoint.entry_count, 3);
        assert_eq!(sealed.checkpoint.last_sequence, 2);
        assert_eq!(sealed.first_sequence, 0);
        assert!(sealed.first_previous_entry_hash.is_none());
        let again = verify_sealed_segment(&sealed_path, "ledger-a", "mother-a").unwrap();
        assert_eq!(again.checkpoint, sealed.checkpoint);
    }

    #[test]
    fn segmented_directory_links_checkpoint_to_the_next_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let source0 = root.join("source0.jsonl");
        write_benchmark_chain(&source0, "ledger-a", "mother-a", 2).unwrap();
        let first = seal_segment(
            &source0,
            root.join("seg-000000.jsonl"),
            "000000",
            "ledger-a",
            "mother-a",
            "2026-05-31T00:00:02Z",
        )
        .unwrap();
        let source1 = root.join("source1.jsonl");
        write_linked(
            &source1,
            first.checkpoint_sequence + 1,
            Some(first.checkpoint_entry_hash.clone()),
            2,
        );
        let second = seal_segment(
            &source1,
            root.join("seg-000001.jsonl"),
            "000001",
            "ledger-a",
            "mother-a",
            "2026-05-31T00:00:04Z",
        )
        .unwrap();
        write_linked(
            &root.join("open.jsonl"),
            second.checkpoint_sequence + 1,
            Some(second.checkpoint_entry_hash),
            1,
        );
        let report = verify_segmented_ledger(root, "ledger-a", "mother-a").unwrap();
        assert_eq!(report.segments.len(), 2);
        assert_eq!(report.open_entries, 1);
        assert!(
            verify_segmented_ledger_gated(root, "ledger-a", "mother-a", false)
                .unwrap_err()
                .to_string()
                .contains("disabled")
        );
        let enabled = verify_segmented_ledger_gated(root, "ledger-a", "mother-a", true).unwrap();
        assert_eq!(enabled.segments.len(), 2);
    }

    #[test]
    fn prefix_tamper_rejects_a_sealed_segment() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.jsonl");
        write_benchmark_chain(&source, "ledger-a", "mother-a", 2).unwrap();
        let sealed_path = dir.path().join("seg-000000.jsonl");
        seal_segment(
            &source,
            &sealed_path,
            "000000",
            "ledger-a",
            "mother-a",
            "2026-05-31T00:00:02Z",
        )
        .unwrap();
        let mut bytes = fs::read(&sealed_path).unwrap();
        let pos = bytes
            .windows(4)
            .position(|window| window == b"obs-")
            .unwrap();
        bytes[pos] = b'Z';
        fs::write(&sealed_path, bytes).unwrap();
        assert!(verify_sealed_segment(&sealed_path, "ledger-a", "mother-a").is_err());
    }

    #[test]
    fn live_open_still_reads_a_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observations.jsonl");
        write_benchmark_chain(&path, "ledger-a", "mother-a", 2).unwrap();
        let ledger = crate::JsonlObservationLedger::open(&path, "ledger-a", "mother-a").unwrap();
        assert_eq!(ledger.verified_head().unwrap().local_sequence, 1);
        assert!(matches!(
            verify_segmented_ledger_if_enabled(dir.path(), "ledger-a", "mother-a"),
            Err(ObservationLedgerError::SegmentsDisabled)
        ));
    }
}

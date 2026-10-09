//! Deterministic ingestion, verification, normalization, and static projections for MCT's
//! public trace archive.

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

pub const MANIFEST_SCHEMA: &str = "mct.trace-manifest-entry/v1";
pub const EVENT_SCHEMA: &str = "mct.normalized-trace-event/v1";
pub const NORMALIZER_VERSION: &str = "mct-trace/0.1.0";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectReceipt {
    pub path: String,
    pub digest: String,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_event_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Redaction {
    pub event_id: String,
    pub category: String,
    pub reason: String,
    pub approved_by: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub status: String,
    pub redactions: Vec<Redaction>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestEntry {
    pub record_type: String,
    pub schema: String,
    pub trace_id: String,
    pub runtime: String,
    pub runtime_session_id: String,
    pub source_format: String,
    pub source: ObjectReceipt,
    pub archive: ObjectReceipt,
    pub event_count: u64,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub publication: Publication,
    pub attachments: Vec<ObjectReceipt>,
    pub normalizer_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
}

#[derive(Clone, Debug)]
struct ParsedEvent {
    raw_line: String,
    value: Value,
    id: String,
    parent_id: Option<String>,
    timestamp: Option<String>,
}

#[derive(Clone, Debug)]
struct ParsedTrace {
    session_id: String,
    events: Vec<ParsedEvent>,
    started_at: Option<String>,
    completed_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationReport {
    pub traces: usize,
    pub events: u64,
    pub compressed_bytes: u64,
}

/// Ingest one reviewed Pi session. The source bytes are scanned, parsed, receipted, and
/// compressed without modification. Repeating an identical import is idempotent.
pub fn ingest_pi_session(repo_root: &Path, source_path: &Path) -> Result<ManifestEntry> {
    let source_bytes = fs::read(source_path)
        .with_context(|| format!("failed to read source trace {}", source_path.display()))?;
    scan_for_secrets(&source_bytes)?;
    let parsed = parse_pi_v3(&source_bytes)?;
    reject_unextracted_attachments(&parsed)?;
    validate_session_id(&parsed.session_id)?;

    let trace_id = format!("trace:pi:{}", parsed.session_id);
    let archive_rel = format!("traces/sessions/{}.jsonl.zst", parsed.session_id);
    let archive_bytes = zstd::stream::encode_all(source_bytes.as_slice(), 19)
        .context("failed to compress source trace with Zstandard")?;

    let entry = ManifestEntry {
        record_type: "trace".to_owned(),
        schema: MANIFEST_SCHEMA.to_owned(),
        trace_id,
        runtime: "pi".to_owned(),
        runtime_session_id: parsed.session_id.clone(),
        source_format: "pi-session/v3".to_owned(),
        source: receipt(
            format!("runtime:pi/{}.jsonl", parsed.session_id),
            &source_bytes,
            Some("application/x-ndjson"),
        ),
        archive: receipt(
            archive_rel.clone(),
            &archive_bytes,
            Some("application/zstd"),
        ),
        event_count: parsed.events.len() as u64,
        started_at: parsed.started_at,
        completed_at: parsed.completed_at,
        publication: Publication {
            status: "complete".to_owned(),
            redactions: Vec::new(),
        },
        attachments: Vec::new(),
        normalizer_version: NORMALIZER_VERSION.to_owned(),
        supersedes: None,
    };
    validate_manifest_entry(&entry)?;

    let manifest_path = repo_root.join("traces/manifest.jsonl");
    let existing = read_manifest(&manifest_path)?;
    if let Some(previous) = existing.iter().find(|item| item.trace_id == entry.trace_id) {
        ensure!(
            previous == &entry,
            "trace {} already has a different manifest receipt; append a superseding record explicitly",
            entry.trace_id
        );
        verify_archive(repo_root, previous)?;
        return Ok(entry);
    }

    let archive_path = safe_repo_path(repo_root, &archive_rel)?;
    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if archive_path.exists() {
        let current = fs::read(&archive_path)?;
        ensure!(
            current == archive_bytes,
            "archive path {} exists with different bytes",
            archive_path.display()
        );
    } else {
        fs::write(&archive_path, &archive_bytes)?;
    }

    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut manifest = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&manifest_path)?;
    serde_json::to_writer(&mut manifest, &entry)?;
    manifest.write_all(b"\n")?;
    manifest.sync_all()?;

    Ok(entry)
}

/// Verify every receipt, source event, publication state, and safety scan in the archive.
pub fn verify_repository(repo_root: &Path) -> Result<VerificationReport> {
    let entries = read_manifest(&repo_root.join("traces/manifest.jsonl"))?;
    let mut trace_ids = BTreeSet::new();
    let mut events = 0_u64;
    let mut compressed_bytes = 0_u64;

    for entry in &entries {
        validate_manifest_entry(entry)?;
        ensure!(
            trace_ids.insert(entry.trace_id.clone()),
            "duplicate trace ID without explicit supersession: {}",
            entry.trace_id
        );
        let parsed = verify_archive(repo_root, entry)?;
        ensure!(
            parsed.events.len() as u64 == entry.event_count,
            "event count mismatch for {}",
            entry.trace_id
        );
        ensure!(
            parsed.started_at == entry.started_at,
            "start timestamp mismatch for {}",
            entry.trace_id
        );
        ensure!(
            parsed.completed_at == entry.completed_at,
            "completion timestamp mismatch for {}",
            entry.trace_id
        );
        events += entry.event_count;
        compressed_bytes += entry.archive.bytes;
    }

    Ok(VerificationReport {
        traces: entries.len(),
        events,
        compressed_bytes,
    })
}

/// Generate disposable normalized events and complete static documentation projections.
pub fn project_book(repo_root: &Path, book_source: &Path) -> Result<VerificationReport> {
    let report = verify_repository(repo_root)?;
    let entries = read_manifest(&repo_root.join("traces/manifest.jsonl"))?;

    let provenance = book_source.join("provenance");
    let generated = provenance.join("generated");
    let normalized_dir = provenance.join("normalized");
    let raw_dir = provenance.join("raw");
    for directory in [&generated, &normalized_dir, &raw_dir] {
        if directory.exists() {
            fs::remove_dir_all(directory)?;
        }
        fs::create_dir_all(directory)?;
    }

    let mut session_index = String::from(
        "<!-- Generated by mct-trace; do not edit. -->\n# Published sessions\n\n\
         These pages are deterministic projections of immutable runtime traces. Traces are evidence, not product authority.\n\n",
    );

    for entry in &entries {
        let archive_path = safe_repo_path(repo_root, &entry.archive.path)?;
        let archive = fs::read(&archive_path)?;
        let source = zstd::stream::decode_all(archive.as_slice())?;
        let parsed = parse_pi_v3(&source)?;
        let slug = &entry.runtime_session_id;

        let normalized = normalized_jsonl(entry, &parsed)?;
        fs::write(normalized_dir.join(format!("{slug}.jsonl")), normalized)?;
        fs::copy(&archive_path, raw_dir.join(format!("{slug}.jsonl.zst")))?;

        fs::write(
            generated.join(format!("{slug}.md")),
            render_overview(entry, &parsed),
        )?;
        fs::write(
            generated.join(format!("{slug}-transcript.md")),
            render_transcript(entry, &parsed),
        )?;

        session_index.push_str(&format!(
            "- [`{}`](generated/{}.md) — {} events, `{}`\n",
            entry.trace_id, slug, entry.event_count, entry.publication.status
        ));
    }

    if entries.is_empty() {
        session_index.push_str("No public sessions have been imported.\n");
    }
    fs::write(provenance.join("sessions.md"), session_index)?;
    project_summary(book_source, &entries)?;
    Ok(report)
}

fn project_summary(book_source: &Path, entries: &[ManifestEntry]) -> Result<()> {
    let summary_path = book_source.join("SUMMARY.md");
    let mut summary = fs::read_to_string(&summary_path)?;
    let parent = "  - [Published sessions](provenance/sessions.md)";
    ensure!(
        summary.matches(parent).count() == 1,
        "SUMMARY.md must contain exactly one published sessions entry"
    );
    let mut replacement = parent.to_owned();
    for entry in entries {
        let session = &entry.runtime_session_id;
        replacement.push_str(&format!(
            "\n    - [Pi session `{session}`](provenance/generated/{session}.md)\n      - [Complete transcript](provenance/generated/{session}-transcript.md)"
        ));
    }
    summary = summary.replace(parent, &replacement);
    fs::write(summary_path, summary)?;
    Ok(())
}

pub fn read_manifest(path: &Path) -> Result<Vec<ManifestEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)?;
    let mut entries = Vec::new();
    let mut saw_header = false;
    for (index, line) in text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
    {
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("invalid manifest JSON on line {}", index + 1))?;
        if value.get("record_type").and_then(Value::as_str) == Some("manifest") {
            ensure!(!saw_header, "duplicate trace manifest header");
            ensure!(index == 0, "trace manifest header must be the first record");
            ensure!(
                value.get("schema").and_then(Value::as_str) == Some("mct.trace-manifest/v1"),
                "unsupported trace manifest header schema"
            );
            ensure!(
                value.get("policy").and_then(Value::as_str) == Some("public-by-default"),
                "trace manifest is not public-by-default"
            );
            saw_header = true;
            continue;
        }
        entries.push(
            serde_json::from_value(value)
                .with_context(|| format!("invalid trace receipt on line {}", index + 1))?,
        );
    }
    Ok(entries)
}

fn verify_archive(repo_root: &Path, entry: &ManifestEntry) -> Result<ParsedTrace> {
    let archive_path = safe_repo_path(repo_root, &entry.archive.path)?;
    let archive = fs::read(&archive_path)
        .with_context(|| format!("failed to read archive {}", archive_path.display()))?;
    verify_receipt(&entry.archive, &archive)?;
    let source = zstd::stream::decode_all(archive.as_slice())
        .with_context(|| format!("invalid Zstandard archive for {}", entry.trace_id))?;
    verify_receipt(&entry.source, &source)?;
    scan_for_secrets(&source)?;
    let parsed = parse_pi_v3(&source)?;
    reject_unextracted_attachments(&parsed)?;
    ensure!(
        parsed.session_id == entry.runtime_session_id,
        "runtime session ID mismatch for {}",
        entry.trace_id
    );
    Ok(parsed)
}

fn validate_manifest_entry(entry: &ManifestEntry) -> Result<()> {
    ensure!(entry.record_type == "trace", "invalid manifest record type");
    ensure!(
        entry.schema == MANIFEST_SCHEMA,
        "unsupported manifest schema"
    );
    ensure!(entry.runtime == "pi", "unsupported trace runtime");
    ensure!(
        entry.source_format == "pi-session/v3",
        "unsupported source format"
    );
    ensure!(
        entry.trace_id == format!("trace:pi:{}", entry.runtime_session_id),
        "trace identity does not match runtime session"
    );
    validate_session_id(&entry.runtime_session_id)?;
    ensure!(
        entry.event_count > 0,
        "trace must contain at least one event"
    );
    ensure!(
        entry.publication.status == "complete" && entry.publication.redactions.is_empty(),
        "the v1 importer only publishes complete, unredacted traces"
    );
    ensure!(
        entry.attachments.is_empty(),
        "attachment extraction is not implemented"
    );
    ensure!(
        entry.normalizer_version == NORMALIZER_VERSION,
        "unsupported normalizer version"
    );
    ensure!(
        entry.supersedes.is_none(),
        "superseding receipts are not implemented"
    );
    ensure!(
        entry.source.digest.starts_with("blake3:"),
        "invalid source digest"
    );
    ensure!(
        entry.archive.digest.starts_with("blake3:"),
        "invalid archive digest"
    );
    Ok(())
}

fn validate_session_id(session_id: &str) -> Result<()> {
    ensure!(!session_id.is_empty(), "empty session ID");
    ensure!(
        session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')),
        "session ID contains path-unsafe characters"
    );
    Ok(())
}

fn safe_repo_path(repo_root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    ensure!(
        !path.is_absolute(),
        "archive path must be repository-relative"
    );
    ensure!(
        path.components()
            .all(|part| matches!(part, Component::Normal(_))),
        "archive path contains unsafe components"
    );
    Ok(repo_root.join(path))
}

fn receipt(path: String, bytes: &[u8], media_type: Option<&str>) -> ObjectReceipt {
    ObjectReceipt {
        path,
        digest: format!("blake3:{}", blake3::hash(bytes).to_hex()),
        bytes: bytes.len() as u64,
        media_type: media_type.map(str::to_owned),
        source_event_id: None,
    }
}

fn verify_receipt(receipt: &ObjectReceipt, bytes: &[u8]) -> Result<()> {
    ensure!(
        receipt.bytes == bytes.len() as u64,
        "byte receipt mismatch for {}",
        receipt.path
    );
    let actual = format!("blake3:{}", blake3::hash(bytes).to_hex());
    ensure!(
        receipt.digest == actual,
        "digest receipt mismatch for {}",
        receipt.path
    );
    Ok(())
}

fn parse_pi_v3(bytes: &[u8]) -> Result<ParsedTrace> {
    let text = std::str::from_utf8(bytes).context("Pi trace is not UTF-8 JSONL")?;
    ensure!(!text.is_empty(), "Pi trace is empty");
    let mut events = Vec::new();
    let mut ids = BTreeSet::new();

    for (index, source_line) in text.split_inclusive('\n').enumerate() {
        let raw_line = source_line
            .strip_suffix('\n')
            .unwrap_or(source_line)
            .strip_suffix('\r')
            .unwrap_or(source_line.strip_suffix('\n').unwrap_or(source_line));
        ensure!(
            !raw_line.trim().is_empty(),
            "blank JSONL record on line {}",
            index + 1
        );
        let value: Value = serde_json::from_str(raw_line)
            .with_context(|| format!("invalid Pi JSON on line {}", index + 1))?;
        let object = value
            .as_object()
            .ok_or_else(|| anyhow!("Pi record on line {} is not an object", index + 1))?;
        let id = required_string(object, "id", index + 1)?.to_owned();
        ensure!(ids.insert(id.clone()), "duplicate Pi entry ID {id}");
        let parent_id = optional_string(object, "parentId")?;
        if let Some(parent) = &parent_id {
            ensure!(
                ids.contains(parent),
                "Pi entry {id} references unseen parent {parent}"
            );
        }
        let timestamp = optional_string(object, "timestamp")?;
        events.push(ParsedEvent {
            raw_line: raw_line.to_owned(),
            value,
            id,
            parent_id,
            timestamp,
        });
    }

    let first = events
        .first()
        .ok_or_else(|| anyhow!("Pi trace has no events"))?;
    let header = first.value.as_object().expect("validated object");
    ensure!(
        required_string(header, "type", 1)? == "session",
        "first Pi record is not a session header"
    );
    ensure!(
        header.get("version").and_then(Value::as_u64) == Some(3),
        "unsupported Pi session version"
    );
    let session_id = first.id.clone();
    let started_at = first.timestamp.clone();
    let completed_at = events.last().and_then(|event| event.timestamp.clone());

    Ok(ParsedTrace {
        session_id,
        events,
        started_at,
        completed_at,
    })
}

fn required_string<'a>(object: &'a Map<String, Value>, key: &str, line: usize) -> Result<&'a str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Pi record on line {line} has no string {key}"))
}

fn optional_string(object: &Map<String, Value>, key: &str) -> Result<Option<String>> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("Pi record field {key} is not a string or null"),
    }
}

fn scan_for_secrets(bytes: &[u8]) -> Result<()> {
    let text = std::str::from_utf8(bytes).context("trace safety scan requires UTF-8")?;
    let mut findings = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("-----begin ") && lower.contains("private key-----") {
            findings.push((index + 1, "private-key"));
        }
        let compact = lower
            .chars()
            .filter(|character| !character.is_ascii_whitespace())
            .collect::<String>();
        if lower.contains("authorization: bearer ")
            || compact.contains("\"authorization\":\"bearer")
        {
            findings.push((index + 1, "authorization-header"));
        }
        for key in [
            "password",
            "passwd",
            "api_key",
            "apikey",
            "access_token",
            "refresh_token",
            "client_secret",
        ] {
            if contains_assigned_credential(line, key) {
                findings.push((index + 1, "assigned-credential"));
            }
        }
        for prefix in ["ghp_", "github_pat_", "xoxb-", "xoxa-", "xoxp-", "xoxr-"] {
            if contains_prefixed_credential(line, prefix, 16) {
                findings.push((index + 1, "token-prefix"));
            }
        }
        if contains_aws_access_key(line) {
            findings.push((index + 1, "aws-access-key"));
        }
        if contains_openai_style_key(line) {
            findings.push((index + 1, "api-key-prefix"));
        }
    }
    findings.sort_unstable();
    findings.dedup();
    if findings.is_empty() {
        return Ok(());
    }
    let summary = findings
        .iter()
        .map(|(line, category)| format!("line {line}: {category}"))
        .collect::<Vec<_>>()
        .join(", ");
    bail!("trace failed publication safety scan ({summary}); secret values were not printed")
}

fn contains_assigned_credential(line: &str, key: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let mut offset = 0;
    while let Some(relative) = lower[offset..].find(key) {
        let start = offset + relative;
        let end = start + key.len();
        let before_is_name = start > 0 && lower.as_bytes()[start - 1].is_ascii_alphanumeric();
        let after_is_name = lower
            .as_bytes()
            .get(end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
        if !before_is_name && !after_is_name {
            let suffix = line[end..].trim_start_matches(['"', '\'', ' ', '\t']);
            if let Some(suffix) = suffix.strip_prefix([':', '=']) {
                let value = suffix.trim_start_matches(['"', '\'', ' ', '\t']);
                let candidate = value
                    .split(['"', '\'', ',', ';', ' ', '\t', '\\'])
                    .next()
                    .unwrap_or_default();
                let placeholder = candidate.starts_with('<')
                    || candidate.starts_with("${")
                    || matches!(
                        candidate.to_ascii_lowercase().as_str(),
                        "redacted" | "example" | "placeholder" | "none" | "null"
                    );
                if candidate.len() >= 8 && !placeholder {
                    return true;
                }
            }
        }
        offset = end;
    }
    false
}

fn contains_prefixed_credential(line: &str, prefix: &str, minimum_suffix: usize) -> bool {
    let mut rest = line;
    while let Some(index) = rest.find(prefix) {
        let candidate = &rest[index + prefix.len()..];
        let length = candidate
            .chars()
            .take_while(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
            })
            .count();
        if length >= minimum_suffix {
            return true;
        }
        rest = &candidate[candidate.len().min(1)..];
    }
    false
}

fn contains_aws_access_key(line: &str) -> bool {
    line.as_bytes().windows(20).any(|window| {
        window.starts_with(b"AKIA")
            && window[4..]
                .iter()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    })
}

fn contains_openai_style_key(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.windows(3).enumerate().any(|(index, window)| {
        if window != b"sk-" {
            return false;
        }
        bytes[index + 3..]
            .iter()
            .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'-' || **byte == b'_')
            .count()
            >= 20
    })
}

fn reject_unextracted_attachments(parsed: &ParsedTrace) -> Result<()> {
    for event in &parsed.events {
        let Some(content) = event
            .value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for item in content {
            let content_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
            ensure!(
                !matches!(content_type, "image" | "file" | "attachment"),
                "trace event {} contains an attachment; extraction must be implemented before publication",
                event.id
            );
        }
    }
    Ok(())
}

fn normalized_jsonl(entry: &ManifestEntry, parsed: &ParsedTrace) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    for (sequence, event) in parsed.events.iter().enumerate() {
        let (actor, kind) = actor_and_kind(&event.value);
        let normalized = json!({
            "schema": EVENT_SCHEMA,
            "trace_id": entry.trace_id,
            "event_id": event.id,
            "sequence": sequence,
            "occurred_at": event.timestamp,
            "normalizer_version": entry.normalizer_version,
            "source_digest": entry.source.digest,
            "source": {
                "format": entry.source_format,
                "session_id": entry.runtime_session_id,
                "entry_id": event.id,
                "parent_entry_id": event.parent_id,
            },
            "actor": actor,
            "kind": kind,
            "payload": event.value,
        });
        serde_json::to_writer(&mut output, &normalized)?;
        output.push(b'\n');
    }
    Ok(output)
}

fn actor_and_kind(value: &Value) -> (Value, String) {
    let record_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if record_type != "message" {
        return (
            json!({"kind": "runtime", "id": null, "model": null}),
            record_type.to_owned(),
        );
    }
    let message = value.get("message").and_then(Value::as_object);
    let role = message
        .and_then(|item| item.get("role"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let actor_kind = match role {
        "user" => "user",
        "assistant" => "assistant",
        "toolResult" => "tool",
        "system" => "system",
        _ => "unknown",
    };
    let model = message
        .and_then(|item| item.get("model"))
        .cloned()
        .unwrap_or(Value::Null);
    (
        json!({"kind": actor_kind, "id": null, "model": model}),
        format!("message.{role}"),
    )
}

fn render_overview(entry: &ManifestEntry, parsed: &ParsedTrace) -> String {
    let mut kinds = BTreeMap::<String, usize>::new();
    for event in &parsed.events {
        let kind = actor_and_kind(&event.value).1;
        *kinds.entry(kind).or_default() += 1;
    }
    let mut inventory = String::new();
    for (kind, count) in kinds {
        inventory.push_str(&format!("- `{kind}`: {count}\n"));
    }
    format!(
        "<!-- Generated by {normalizer}; source {source_digest}; do not edit. -->\n\
         # Pi session `{session}`\n\n\
         This is a deterministic overview of [`{trace_id}`](./{session}-transcript.md). The trace is evidence, not product authority.\n\n\
         | Receipt | Value |\n|---|---|\n\
         | Publication | `{status}` |\n\
         | Events | {events} |\n\
         | Started | `{started}` |\n\
         | Completed | `{completed}` |\n\
         | Source | `{source_digest}` ({source_bytes} bytes) |\n\
         | Archive | `{archive_digest}` ({archive_bytes} bytes) |\n\
         | Normalizer | `{normalizer}` |\n\n\
         ## Downloads and views\n\n\
         - [Complete transcript](./{session}-transcript.md)\n\
         - [Exact compressed runtime trace](../raw/{session}.jsonl.zst)\n\
         - [Normalized event projection](../normalized/{session}.jsonl)\n\n\
         ## Event inventory\n\n{inventory}",
        normalizer = entry.normalizer_version,
        source_digest = entry.source.digest,
        session = entry.runtime_session_id,
        trace_id = entry.trace_id,
        status = entry.publication.status,
        events = entry.event_count,
        started = entry.started_at.as_deref().unwrap_or("unknown"),
        completed = entry.completed_at.as_deref().unwrap_or("unknown"),
        source_bytes = entry.source.bytes,
        archive_digest = entry.archive.digest,
        archive_bytes = entry.archive.bytes,
    )
}

fn render_transcript(entry: &ManifestEntry, parsed: &ParsedTrace) -> String {
    let mut output = format!(
        "<!-- Generated by {normalizer}; source {source_digest}; do not edit. -->\n\
         # Complete transcript: `{session}`\n\n\
         **Publication:** `{status}`  \n\
         **Events:** {events}  \n\
         **Raw archive:** [download exact compressed JSONL](../raw/{session}.jsonl.zst)\n\n\
         Every persisted source event appears below in source order. The raw archive is fidelity authority.\n\n",
        normalizer = entry.normalizer_version,
        source_digest = entry.source.digest,
        session = entry.runtime_session_id,
        status = entry.publication.status,
        events = entry.event_count,
    );
    for (sequence, event) in parsed.events.iter().enumerate() {
        let kind = actor_and_kind(&event.value).1;
        output.push_str(&format!(
            "<a id=\"{}\"></a>\n## {:04} · `{}` · `{}`\n\nCitation: `{}`\n\n<pre><code class=\"language-json\">{}</code></pre>\n\n",
            html_escape(&event.id),
            sequence,
            html_escape(&kind),
            html_escape(&event.id),
            html_escape(&format!("{}#{}", entry.trace_id, event.id)),
            html_escape(&event.raw_line),
        ));
    }
    output
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACE: &str = concat!(
        "{\"type\":\"session\",\"version\":3,\"id\":\"session-1\",\"timestamp\":\"2026-01-01T00:00:00Z\",\"cwd\":\"/repo\"}\n",
        "{\"type\":\"message\",\"id\":\"entry-1\",\"parentId\":null,\"timestamp\":\"2026-01-01T00:00:01Z\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"hello\"}]}}\n",
        "{\"type\":\"message\",\"id\":\"entry-2\",\"parentId\":\"entry-1\",\"timestamp\":\"2026-01-01T00:00:02Z\",\"message\":{\"role\":\"assistant\",\"model\":\"test\",\"content\":[{\"type\":\"text\",\"text\":\"world\"}]}}\n",
    );

    #[test]
    fn ingestion_is_idempotent_and_projection_is_complete() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("traces")).unwrap();
        fs::write(temp.path().join("traces/manifest.jsonl"), "").unwrap();
        let source = temp.path().join("source.jsonl");
        fs::write(&source, TRACE).unwrap();

        let first = ingest_pi_session(temp.path(), &source).unwrap();
        let second = ingest_pi_session(temp.path(), &source).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            read_manifest(&temp.path().join("traces/manifest.jsonl"))
                .unwrap()
                .len(),
            1
        );

        let book = temp.path().join("book");
        fs::create_dir_all(book.join("provenance")).unwrap();
        fs::write(
            book.join("SUMMARY.md"),
            "  - [Published sessions](provenance/sessions.md)\n",
        )
        .unwrap();
        let report = project_book(temp.path(), &book).unwrap();
        assert_eq!(report.events, 3);
        let transcript =
            fs::read_to_string(book.join("provenance/generated/session-1-transcript.md")).unwrap();
        assert_eq!(transcript.matches("<a id=").count(), 3);
        assert!(transcript.contains("trace:pi:session-1#entry-2"));
        let normalized =
            fs::read_to_string(book.join("provenance/normalized/session-1.jsonl")).unwrap();
        assert_eq!(normalized.lines().count(), 3);

        project_book(temp.path(), &book).unwrap();
        assert_eq!(
            transcript,
            fs::read_to_string(book.join("provenance/generated/session-1-transcript.md")).unwrap()
        );
        assert_eq!(
            normalized,
            fs::read_to_string(book.join("provenance/normalized/session-1.jsonl")).unwrap()
        );
    }

    #[test]
    fn compression_and_receipts_are_reproducible() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("traces")).unwrap();
        fs::write(temp.path().join("traces/manifest.jsonl"), "").unwrap();
        let source = temp.path().join("source.jsonl");
        fs::write(&source, TRACE).unwrap();
        let entry = ingest_pi_session(temp.path(), &source).unwrap();
        let original = fs::read(temp.path().join(&entry.archive.path)).unwrap();
        fs::remove_file(temp.path().join(&entry.archive.path)).unwrap();
        fs::write(temp.path().join("traces/manifest.jsonl"), "").unwrap();
        let repeated = ingest_pi_session(temp.path(), &source).unwrap();
        assert_eq!(entry, repeated);
        assert_eq!(
            original,
            fs::read(temp.path().join(&repeated.archive.path)).unwrap()
        );
    }

    #[test]
    fn archive_tampering_fails_receipt_verification() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("traces")).unwrap();
        fs::write(temp.path().join("traces/manifest.jsonl"), "").unwrap();
        let source = temp.path().join("source.jsonl");
        fs::write(&source, TRACE).unwrap();
        let entry = ingest_pi_session(temp.path(), &source).unwrap();
        let archive = temp.path().join(entry.archive.path);
        let mut bytes = fs::read(&archive).unwrap();
        bytes[0] ^= 0xff;
        fs::write(archive, bytes).unwrap();
        assert!(verify_repository(temp.path()).is_err());
    }

    #[test]
    fn secret_scanner_fails_without_echoing_the_secret() {
        let secret = "ghp_abcdefghijklmnopqrstuvwxyz123456";
        let error = scan_for_secrets(format!("value={secret}\n").as_bytes())
            .unwrap_err()
            .to_string();
        assert!(error.contains("token-prefix"));
        assert!(!error.contains(secret));
    }

    #[test]
    fn assigned_credentials_and_attachments_fail_closed() {
        let secret = "password=correct-horse-battery-staple";
        let error = scan_for_secrets(secret.as_bytes()).unwrap_err().to_string();
        assert!(error.contains("assigned-credential"));
        assert!(!error.contains("correct-horse"));

        let attachment = concat!(
            "{\"type\":\"session\",\"version\":3,\"id\":\"session-1\",\"timestamp\":\"2026-01-01T00:00:00Z\"}\n",
            "{\"type\":\"message\",\"id\":\"entry-1\",\"parentId\":null,\"timestamp\":\"2026-01-01T00:00:01Z\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"image\",\"data\":\"abc\"}]}}\n",
        );
        let parsed = parse_pi_v3(attachment.as_bytes()).unwrap();
        assert!(reject_unextracted_attachments(&parsed).is_err());
    }

    #[test]
    fn malformed_or_incomplete_parent_chains_fail_closed() {
        let broken = concat!(
            "{\"type\":\"session\",\"version\":3,\"id\":\"session-1\",\"timestamp\":\"2026-01-01T00:00:00Z\"}\n",
            "{\"type\":\"message\",\"id\":\"entry-1\",\"parentId\":\"missing\",\"timestamp\":\"2026-01-01T00:00:01Z\"}\n",
        );
        assert!(parse_pi_v3(broken.as_bytes()).is_err());
    }
}

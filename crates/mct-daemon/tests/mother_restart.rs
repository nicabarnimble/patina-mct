//! Subprocess crash/restart proof for the resident Mother.
//!
//! Each iteration starts the real `mct-daemon serve` binary on a private
//! state root, acquires and approves three Children, starts a temporal
//! trigger on Child 0, drives multi-step idempotency-keyed work into the
//! Children over the owner-only UDS (`POST /calls`), SIGKILLs Mother at a
//! seeded point (mid-run, or right after Child 0 finishes its plan), restarts
//! it on the same state, and checks:
//!
//! * the ledger hash chain verifies through `verify_ledger_streaming` and an
//!   independent re-hash (contiguous sequences, previous-hash links, entry
//!   hashes), every pre-kill committed byte is preserved, and the restart
//!   writes no committed fact again;
//! * Child work acknowledged as completed before the kill keeps exactly one
//!   run record and one `runtime_execution_started`, and a retry with the same
//!   idempotency key replays the recorded reply without re-running the Child;
//!   trigger calls start their Child at most once across the crash;
//! * restart opens a fresh authority epoch, and handles still resolve:
//!   approved and loaded Children, artifact records, the trigger payload blob,
//!   and a fresh call into every Child;
//! * a second SIGKILL/restart with no work in between re-executes nothing,
//!   leaves state projections and forensics unchanged, and still replays.
//!
//! Separate tests pin gaps this harness found (they fail on 8684d5a):
//! `inflight_child_call_is_resumed_or_marked_interrupted_after_sigkill`,
//! `torn_final_ledger_frame_is_recovered_on_restart`, and the no-crash
//! control `concurrent_children_calls_are_not_refused_without_a_crash`.
//! `crash_on_first_ledger_write_recovers_then_operator_gate_starts_mother`
//! pins the operator-gate path over a recovery-only ledger (D-H3.5).
//!
//! Knobs: `MCT_RESTART_SEED` (default 0x6d63_7421), `MCT_RESTART_ITERATIONS`
//! (default 3 for the CI test, 24 for the ignored long test), and
//! `MCT_RESTART_KEEP` to keep each state root for inspection.
#![cfg(unix)]

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use mct_daemon::{MctRuntimeRunState, MctRuntimeStateStore, local_blob_store_for_state_path};
use mct_kernel::{CallId, MctCallPayloadHandle, ObservationKind, ResultOutcome};
use mct_observation::{MctObservationLedgerEntry, forensic_root_path, verify_ledger_streaming};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read as _, Write as _},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const LEDGER_ID: &str = "ledger-local";
const MOTHER_NODE_ID: &str = "local-mct";
const CHILD_COUNT: usize = 3;
const DEFAULT_SEED: u64 = 0x6d63_7421;
const READY_TIMEOUT: Duration = Duration::from_secs(60);
const TRIGGER_ID: &str = "trigger-restart-c0";

// ---------------------------------------------------------------- utilities

/// SplitMix64: tiny, dependency-free, and reproducible under a fixed seed.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn range(&mut self, low: u64, high_inclusive: u64) -> u64 {
        low + self.next_u64() % (high_inclusive - low + 1)
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    match std::env::var(name) {
        Ok(value) => {
            let value = value.trim();
            let parsed = value
                .strip_prefix("0x")
                .map_or_else(|| value.parse(), |hex| u64::from_str_radix(hex, 16));
            parsed.unwrap_or_else(|_| panic!("{name} must be an integer, got {value:?}"))
        }
        Err(_) => default,
    }
}

fn now_plus_ms(millis: i64) -> String {
    jiff::Timestamp::now()
        .checked_add(jiff::SignedDuration::from_millis(millis))
        .unwrap()
        .to_string()
}

/// One HTTP/1.1 request over the Mother's owner-only control socket.
fn uds(
    socket: &Path,
    method: &str,
    path: &str,
    body: Option<&Value>,
    timeout: Duration,
) -> std::io::Result<(u16, Value)> {
    let body = body.map_or_else(Vec::new, |value| serde_json::to_vec(value).unwrap());
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(
        format!(
            "{method} {path} HTTP/1.1\r\nHost: local\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .as_bytes(),
    )?;
    stream.write_all(&body)?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    let response = String::from_utf8_lossy(&response).into_owned();
    let invalid =
        |what: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, what.to_owned());
    let status = response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| invalid("response had no status line"))?;
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .ok_or_else(|| invalid("response had no body separator"))?;
    let value = if body.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(body).map_err(|_| invalid("response body was not JSON"))?
    };
    Ok((status, value))
}

// ------------------------------------------------------------- the Mother

struct Layout {
    _work: Option<tempfile::TempDir>,
    work: PathBuf,
    root: PathBuf,
    sources: PathBuf,
}

impl Layout {
    fn new() -> Self {
        let work = tempfile::tempdir().unwrap();
        let work_path = work.path().to_path_buf();
        if std::env::var_os("MCT_RESTART_KEEP").is_some() {
            eprintln!("mother-restart: keeping {}", work_path.display());
        }
        // The state root must hold nothing but Mother-managed artifacts:
        // anything else makes startup classify the root as non-virgin.
        let root = work_path.join("root");
        let sources = work_path.join("sources");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&sources).unwrap();
        let work = if std::env::var_os("MCT_RESTART_KEEP").is_some() {
            let _ = work.keep();
            None
        } else {
            Some(work)
        };
        Self {
            _work: work,
            work: work_path,
            root,
            sources,
        }
    }
    fn socket(&self) -> PathBuf {
        self.root.join("control.sock")
    }
    fn ledger(&self) -> PathBuf {
        self.root.join("observations.jsonl")
    }
    fn state(&self) -> PathBuf {
        self.root.join("state.sqlite")
    }
    fn config(&self) -> PathBuf {
        self.root.join("config.json")
    }
    fn children(&self) -> PathBuf {
        self.root.join("children")
    }
    fn identity(&self) -> PathBuf {
        self.root.join("identity").join("iroh-secret.hex")
    }
}

struct Mother {
    process: Child,
    boot: usize,
}

impl Mother {
    fn spawn(layout: &Layout, boot: usize) -> Self {
        let log = std::fs::File::create(layout.work.join(format!("mother-{boot}.log"))).unwrap();
        let process = Command::new(env!("CARGO_BIN_EXE_mct-daemon"))
            .arg("serve")
            .arg("--identity")
            .arg(layout.identity())
            .arg("--config")
            .arg(layout.config())
            .arg("--children-dir")
            .arg(layout.children())
            .arg("--state")
            .arg(layout.state())
            .arg("--ledger")
            .arg(layout.ledger())
            .arg("--uds")
            .arg(layout.socket())
            .current_dir(&layout.work)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("spawn mct-daemon serve");
        Self { process, boot }
    }

    /// Start and wait for `/status` to report ready. On timeout or exit the
    /// last status and startup bodies are returned so the caller can report
    /// exactly which posture Mother came up in.
    fn start(layout: &Layout, boot: usize) -> Result<Self, String> {
        let mut mother = Self::spawn(layout, boot);
        let deadline = Instant::now() + READY_TIMEOUT;
        let mut last = Value::Null;
        while Instant::now() < deadline {
            if let Some(exit) = mother.process.try_wait().unwrap() {
                return Err(format!(
                    "boot {boot}: mct-daemon exited {exit} before ready; log:\n{}",
                    mother.log(layout)
                ));
            }
            if let Ok((200, status)) = uds(
                &layout.socket(),
                "GET",
                "/status",
                None,
                Duration::from_secs(2),
            ) {
                if status["readiness"] == "ready" {
                    return Ok(mother);
                }
                last = status;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let startup = uds(
            &layout.socket(),
            "GET",
            "/startup",
            None,
            Duration::from_secs(2),
        )
        .map(|(_, body)| body)
        .unwrap_or(Value::Null);
        mother.sigkill();
        Err(format!(
            "boot {boot}: Mother never became ready.\nlast /status: {last:#}\n/startup: {}\nlog:\n{}",
            summarize_startup(&startup),
            mother.log(layout)
        ))
    }

    fn sigkill(&mut self) {
        // std's Child::kill is SIGKILL on Unix: no handler, no flush, no
        // shutdown fact. That is the crash under test.
        let _ = self.process.kill();
        let _ = self.process.wait();
    }

    fn log(&self, layout: &Layout) -> String {
        std::fs::read_to_string(layout.work.join(format!("mother-{}.log", self.boot)))
            .unwrap_or_default()
    }
}

impl Drop for Mother {
    fn drop(&mut self) {
        self.sigkill();
    }
}

fn summarize_startup(startup: &Value) -> String {
    let mut out = serde_json::Map::new();
    if let Some(object) = startup.as_object() {
        for (key, value) in object {
            if key != "inventory" {
                out.insert(key.clone(), value.clone());
            }
        }
    }
    let present = startup["inventory"]["entries"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry["state"] != "absent")
                .map(|entry| format!("{}={}", entry["artifact_class"], entry["state"]))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    format!("{:#} present={present:?}", Value::Object(out))
}

// -------------------------------------------------------------- Children

fn child_name(index: usize) -> String {
    format!("work-c{index}")
}

fn child_namespace(index: usize) -> String {
    format!("patina:mct-restart-c{index}")
}

/// `step(value) -> value + index + 1` after spinning `value` million loop
/// iterations, so the caller controls how long each step stays in flight.
fn child_component(index: usize) -> Vec<u8> {
    let wat = format!(
        r#"(component
  (core module $m
    (func $step (export "step") (param i32) (result i32)
      (local i64)
      (local.set 1 (i64.mul (i64.extend_i32_u (local.get 0)) (i64.const 1000000)))
      (block $done
        (loop $spin
          (br_if $done (i64.eqz (local.get 1)))
          (local.set 1 (i64.sub (local.get 1) (i64.const 1)))
          (br $spin)))
      (i32.add (local.get 0) (i32.const {increment}))))
  (core instance $i (instantiate $m))
  (func $step (param "value" s32) (result s32)
    (canon lift (core func $i "step")))
  (instance $work (export "step" (func $step)))
  (export "{namespace}/work@0.1.0" (instance $work)))"#,
        increment = index + 1,
        namespace = child_namespace(index),
    );
    wat::parse_str(wat).unwrap()
}

fn write_child_source(layout: &Layout, index: usize) -> (PathBuf, String) {
    let name = child_name(index);
    let dir = layout.sources.join(&name);
    std::fs::create_dir_all(&dir).unwrap();
    let component = child_component(index);
    std::fs::write(dir.join(format!("{name}.wasm")), &component).unwrap();
    std::fs::write(
        dir.join("child.toml"),
        format!(
            r#"[child]
name = "{name}"
version = "0.1.0"
description = "restart-test multi-step worker"
kind = "child"
role = "test"

[child.ingress]
mode = "wit-only"

[child.artifact]
wasm = "{name}.wasm"

[child.contract]
default = "{namespace}/work@0.1.0.step"
allow = ["{namespace}/work@0.1.0.step"]

[needs]
toys = []
"#,
            namespace = child_namespace(index)
        ),
    )
    .unwrap();
    (dir, blake3::hash(&component).to_hex().to_string())
}

#[derive(Clone, Debug)]
struct Acquired {
    artifact_ids: Vec<String>,
    trigger_payload: Option<MctCallPayloadHandle>,
}

/// Acquire and approve every Child through the resident Mother, then
/// optionally create a temporal trigger driving Child 0.
fn provision(layout: &Layout, with_trigger: bool) -> Acquired {
    let socket = layout.socket();
    let timeout = Duration::from_secs(60);
    let mut artifact_ids = Vec::new();
    for index in 0..CHILD_COUNT {
        let (source, digest) = write_child_source(layout, index);
        let name = child_name(index);
        let stage = json!({
            "source_root": source,
            "manifest_path": "child.toml",
            "component_path": format!("{name}.wasm"),
            "claimed_child_name": name,
            "claimed_artifact_version": "0.1.0",
            "expected_digest": format!("blake3:{digest}"),
            "standing_source_authority_id": null,
            "claimed_publisher": null,
            "require_source_sidecars": false,
            "children_dir": layout.children(),
            "state_path": layout.state(),
        });
        let (status, report) =
            uds(&socket, "POST", "/artifacts/stage", Some(&stage), timeout).unwrap();
        assert_eq!(status, 200, "stage {name}: {report:#}");
        assert_eq!(report["verification_outcome"], "verified", "{report:#}");
        let artifact_id = report["artifact_id"].as_str().unwrap().to_owned();
        let approve = json!({
            "expected_config_path": layout.config(),
            "expected_children_dir": layout.children(),
            "expected_state_path": layout.state(),
            "expected_artifact_id": artifact_id,
            "child_name": name,
            "strict_integrity": true,
        });
        let (status, report) = uds(
            &socket,
            "POST",
            "/children/approve",
            Some(&approve),
            timeout,
        )
        .unwrap();
        assert_eq!(status, 200, "approve {name}: {report:#}");
        assert_eq!(report["approval_state"], "approved", "{report:#}");
        artifact_ids.push(artifact_id);
    }

    let trigger_payload = with_trigger.then(|| {
        let payload = b"[2]";
        let blob = json!({
            "digest": blake3::hash(payload).to_hex().to_string(),
            "size_bytes": payload.len(),
            "content_type": "application/json",
            "classification": "trigger-static",
            "bytes_base64": BASE64.encode(payload),
        });
        let (status, body) = uds(&socket, "POST", "/blobs", Some(&blob), timeout).unwrap();
        assert_eq!(status, 201, "blob: {body:#}");
        let handle: MctCallPayloadHandle = serde_json::from_value(body["payload"].clone()).unwrap();
        let create = json!({
            "expected_config_path": layout.config(),
            "expected_state_path": layout.state(),
            "scope": {
                "trigger_authority_id": TRIGGER_ID,
                "target": {
                    "namespace": child_namespace(0),
                    "interface_name": "work@0.1.0",
                    "function_name": "step"
                },
                "payload_constraint": handle,
                "trigger_source": {
                    "source_kind": "temporal",
                    "anchor_at": now_plus_ms(200),
                    "interval_ms": 250
                },
                "missed_fire_policy": "skip",
                "overlap_policy": "refuse",
                "starts_at": now_plus_ms(-1_000),
                "expires_at": now_plus_ms(30 * 60 * 1_000),
            }
        });
        let (status, body) =
            uds(&socket, "POST", "/triggers/create", Some(&create), timeout).unwrap();
        assert_eq!(status, 200, "trigger: {body:#}");
        handle
    });
    Acquired {
        artifact_ids,
        trigger_payload,
    }
}

// --------------------------------------------------------------- workload

#[derive(Clone, Debug, PartialEq, Eq)]
enum StepState {
    /// Request bytes may have reached Mother; no reply arrived.
    InFlight,
    Completed {
        inline_result: String,
    },
    Refused {
        body: String,
    },
}

#[derive(Clone, Debug)]
struct Step {
    child: usize,
    /// Final step of this Child's plan.
    last: bool,
    spin: u32,
    call_id: String,
    key: String,
    state: StepState,
}

fn call_submission(child: usize, call_id: &str, key: &str, spin: u32, authority: &Value) -> Value {
    let payload = format!("[{spin}]").into_bytes();
    json!({
        "protocol_request_id": format!("proto-{call_id}-{}", now_plus_ms(0)),
        "call_id": call_id,
        "target": {
            "namespace": child_namespace(child),
            "interface_name": "work@0.1.0",
            "function_name": "step"
        },
        "payload_metadata": {
            "data_classification": "public",
            "size_bytes": payload.len(),
            "contains_secret_scoped_material": false
        },
        "authority_context": {
            "policy_revision": 1,
            "expected_receiver_grants_authority": authority,
            "vision_policy_revision": 1
        },
        "deadline": "2099-01-01T00:00:00Z",
        "trace_context": {
            "trace_id": format!("trace-{call_id}"),
            "span_id": format!("span-{call_id}")
        },
        "payload": {
            "payload_kind": "inline_payload",
            "inline_payload_ref": format!("payload-{call_id}"),
            "content_type": "application/json",
            "size_bytes": payload.len(),
            "blake3_digest_hex": blake3::hash(&payload).to_hex().to_string()
        },
        "inline_payload_base64": BASE64.encode(&payload),
        "idempotency_key": key
    })
}

fn receiver_authority(socket: &Path) -> std::io::Result<Value> {
    let (status, body) = uds(
        socket,
        "GET",
        "/receiver-authority",
        None,
        Duration::from_secs(10),
    )?;
    if status == 200 {
        Ok(body)
    } else {
        Err(std::io::Error::other(format!(
            "receiver authority {status}: {body}"
        )))
    }
}

/// Refusals seen under concurrent load with no crash involved, per Mother
/// socket (tests run in parallel). They are retried and counted for the report.
static TRANSIENT_REFUSALS: Mutex<BTreeMap<PathBuf, usize>> = Mutex::new(BTreeMap::new());

fn note_transient(socket: &Path) {
    *TRANSIENT_REFUSALS
        .lock()
        .unwrap()
        .entry(socket.to_path_buf())
        .or_default() += 1;
}

fn take_transient(socket: &Path) -> usize {
    TRANSIENT_REFUSALS
        .lock()
        .unwrap()
        .remove(socket)
        .unwrap_or_default()
}

/// Only refusals made before the idempotency reservation are retried with
/// the same key. A `failed`/"runtime unavailable" reply is recorded under the
/// key and replays forever, so it is kept as the step's final answer.
fn is_transient(reply: &Value) -> bool {
    reply["protocol_reason"] == "receiver_authority_unavailable"
}

/// Submit one step, retrying transient refusals. `Err` means the transport
/// broke (Mother died) and the step's fate is unknown to the caller.
fn submit(socket: &Path, step: &Step) -> std::io::Result<StepState> {
    let mut attempts = 0;
    loop {
        let authority = receiver_authority(socket)?;
        let body = call_submission(step.child, &step.call_id, &step.key, step.spin, &authority);
        let (status, reply) = uds(
            socket,
            "POST",
            "/calls",
            Some(&body),
            Duration::from_secs(60),
        )?;
        if status == 200 && reply["outcome"] == "completed" {
            return Ok(StepState::Completed {
                inline_result: reply["inline_result_payload_base64"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            });
        }
        attempts += 1;
        if status == 200 && is_transient(&reply) && attempts < 100 {
            note_transient(socket);
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        return Ok(StepState::Refused {
            body: format!("{status} {reply}"),
        });
    }
}

/// The WIT host returns `{"results": [value]}` for a single-result export.
fn decode_result(inline_result: &str) -> Value {
    let value: Value = serde_json::from_slice(&BASE64.decode(inline_result).unwrap()).unwrap();
    value["results"][0].clone()
}

/// Seeded multi-step plans. Child 0 has a short plan so it can finish while
/// the others are still running.
fn plan_steps(rng: &mut Rng, iteration: usize) -> Vec<Vec<Step>> {
    (0..CHILD_COUNT)
        .map(|child| {
            let count = if child == 0 { 3 } else { 40 };
            (0..count)
                .map(|step| {
                    let call_id = format!("call-i{iteration}-c{child}-s{step}");
                    Step {
                        child,
                        last: step + 1 == count,
                        spin: rng.range(1, 30) as u32,
                        key: format!("key-{call_id}"),
                        call_id,
                        state: StepState::InFlight,
                    }
                })
                .collect()
        })
        .collect()
}

struct Workload {
    stop: Arc<AtomicBool>,
    log: Arc<Mutex<Vec<Step>>>,
    finished: Arc<Mutex<BTreeSet<usize>>>,
    handles: Vec<thread::JoinHandle<()>>,
}

impl Workload {
    fn start(socket: PathBuf, plans: Vec<Vec<Step>>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let log = Arc::new(Mutex::new(Vec::new()));
        let finished = Arc::new(Mutex::new(BTreeSet::new()));
        let handles = plans
            .into_iter()
            .map(|plan| {
                let (socket, stop, log, finished) =
                    (socket.clone(), stop.clone(), log.clone(), finished.clone());
                thread::spawn(move || {
                    for mut step in plan {
                        if stop.load(Ordering::SeqCst) {
                            return;
                        }
                        let index = {
                            let mut log = log.lock().unwrap();
                            log.push(step.clone());
                            log.len() - 1
                        };
                        match submit(&socket, &step) {
                            Ok(state) => {
                                step.state = state;
                                if step.last {
                                    finished.lock().unwrap().insert(step.child);
                                }
                                log.lock().unwrap()[index] = step;
                            }
                            // Transport broke: Mother is gone. The step stays
                            // InFlight in the log and this Child's work stops.
                            Err(_) => return,
                        }
                    }
                })
            })
            .collect();
        Self {
            stop,
            log,
            finished,
            handles,
        }
    }

    fn finished(&self) -> BTreeSet<usize> {
        self.finished.lock().unwrap().clone()
    }

    fn completed_count(&self) -> usize {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|step| matches!(step.state, StepState::Completed { .. }))
            .count()
    }

    fn join(self) -> Vec<Step> {
        self.stop.store(true, Ordering::SeqCst);
        for handle in self.handles {
            handle.join().unwrap();
        }
        Arc::try_unwrap(self.log).unwrap().into_inner().unwrap()
    }
}

// ------------------------------------------------------------- invariants

fn ledger_lines(bytes: &[u8]) -> Vec<MctObservationLedgerEntry> {
    bytes
        .split_inclusive(|byte| *byte == b'\n')
        .filter(|line| line.ends_with(b"\n"))
        .map(|line| serde_json::from_slice(&line[..line.len() - 1]).unwrap())
        .collect()
}

/// Verify the chain through the crate's own verifier and an independent
/// re-hash. Returns the parsed entries.
///
/// The Mother may be live (its trigger keeps appending), so the ledger is read
/// once into a snapshot and both checks run over that same snapshot. A live
/// writer can be caught between `write_all` and the next read; such an
/// in-progress final frame is re-read briefly until it completes. A frame that
/// stays unterminated is reported.
fn verify_chain(layout: &Layout, context: &str) -> Vec<MctObservationLedgerEntry> {
    let mut bytes = std::fs::read(layout.ledger()).unwrap();
    for _ in 0..200 {
        if bytes.is_empty() || bytes.ends_with(b"\n") {
            break;
        }
        thread::sleep(Duration::from_millis(5));
        bytes = std::fs::read(layout.ledger()).unwrap();
    }
    assert!(
        bytes.is_empty() || bytes.ends_with(b"\n"),
        "{context}: ledger ends in an unterminated frame after restart"
    );
    let snapshot = layout.work.join("verify-snapshot.jsonl");
    std::fs::write(&snapshot, &bytes).unwrap();
    let verified = verify_ledger_streaming(&snapshot, LEDGER_ID, MOTHER_NODE_ID)
        .unwrap_or_else(|error| panic!("{context}: ledger does not verify: {error}"));
    let _ = std::fs::remove_file(&snapshot);
    let entries = ledger_lines(&bytes);
    let mut previous: Option<String> = None;
    for (index, entry) in entries.iter().enumerate() {
        assert_eq!(
            entry.local_sequence, index as u64,
            "{context}: sequence gap"
        );
        assert_eq!(entry.ledger_id, LEDGER_ID, "{context}");
        assert_eq!(entry.mother_node_id, MOTHER_NODE_ID, "{context}");
        assert_eq!(
            entry.previous_entry_hash, previous,
            "{context}: previous-hash link broken at {index}"
        );
        let mut hashable = entry.clone();
        hashable.entry_hash.clear();
        let recomputed = blake3::hash(&serde_json::to_vec(&hashable).unwrap())
            .to_hex()
            .to_string();
        assert_eq!(
            recomputed, entry.entry_hash,
            "{context}: entry hash at {index}"
        );
        previous = Some(entry.entry_hash.clone());
    }
    let head = verified.head.expect("non-empty ledger has a verified head");
    assert_eq!(head.local_sequence + 1, entries.len() as u64, "{context}");
    assert_eq!(Some(head.entry_hash), previous, "{context}");
    entries
}

/// Facts appended by a restart must be new events: none may be a committed
/// pre-kill fact written again (same observation ID *and* same observation
/// time). Returns the observation IDs a restart reused for a new event (the
/// resident pipeline derives several IDs deterministically from the call or
/// trigger occurrence), for the report.
fn assert_restart_appended_only_new_facts(
    entries: &[MctObservationLedgerEntry],
    committed_before: usize,
    context: &str,
) -> Vec<String> {
    let committed: BTreeMap<_, BTreeSet<_>> =
        entries[..committed_before]
            .iter()
            .fold(BTreeMap::new(), |mut map, entry| {
                map.entry(entry.observation.observation_id.as_str())
                    .or_insert_with(BTreeSet::new)
                    .insert(entry.observation.observed_at.as_str());
                map
            });
    let mut reused = Vec::new();
    for entry in &entries[committed_before..] {
        let id = entry.observation.observation_id.as_str();
        if let Some(times) = committed.get(id) {
            assert!(
                !times.contains(entry.observation.observed_at.as_str()),
                "{context}: restart re-appended committed fact {id} ({:?}) at sequence {}",
                entry.observation.kind,
                entry.local_sequence
            );
            reused.push(format!("{:?}:{id}", entry.observation.kind));
        }
    }
    reused
}

/// Trigger firings are the resident path that is designed to resume after a
/// crash. Each trigger call may start its Child at most once, ever.
fn assert_trigger_calls_execute_at_most_once(entries: &[MctObservationLedgerEntry], context: &str) {
    let mut starts: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in entries {
        if entry.observation.kind == ObservationKind::RuntimeExecutionStarted
            && let Some(call_id) = &entry.observation.call_id
            && !call_id.as_str().starts_with("call-")
            && !call_id.as_str().starts_with("probe-")
        {
            *starts.entry(call_id.as_str()).or_default() += 1;
        }
    }
    for (call_id, count) in starts {
        assert_eq!(
            count, 1,
            "{context}: trigger call {call_id} started {count} times"
        );
    }
}

/// Observation IDs that occur more than once in normal operation. Reported,
/// not asserted: the resident pipeline records the Child authorization
/// `route_revalidated` fact twice under one deterministic ID per call.
fn duplicate_observation_ids(entries: &[MctObservationLedgerEntry]) -> BTreeMap<String, usize> {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in entries {
        *seen
            .entry(entry.observation.observation_id.as_str())
            .or_default() += 1;
    }
    let mut by_shape: BTreeMap<String, usize> = BTreeMap::new();
    for entry in entries {
        let id = entry.observation.observation_id.as_str();
        if seen[id] > 1 {
            let prefix = id.split(':').take(2).collect::<Vec<_>>().join(":");
            *by_shape
                .entry(format!("{:?}/{prefix}", entry.observation.kind))
                .or_default() += 1;
        }
    }
    by_shape
}

fn invocations_for(entries: &[MctObservationLedgerEntry], call_id: &str) -> usize {
    entries
        .iter()
        .filter(|entry| {
            entry.observation.kind == ObservationKind::RuntimeExecutionStarted
                && entry.observation.call_id.as_ref().map(CallId::as_str) == Some(call_id)
        })
        .count()
}

fn runs_by_call(layout: &Layout) -> BTreeMap<String, Vec<MctRuntimeRunState>> {
    let mut runs: BTreeMap<String, Vec<MctRuntimeRunState>> = BTreeMap::new();
    for run in MctRuntimeStateStore::open(layout.state())
        .unwrap()
        .list_runs(100_000)
        .unwrap()
    {
        runs.entry(run.call_id.as_str().to_owned())
            .or_default()
            .push(run.state);
    }
    runs
}

fn forensic_listing(layout: &Layout) -> Vec<(PathBuf, u64)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, u64)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push((path.clone(), std::fs::metadata(&path).unwrap().len()));
            }
        }
    }
    let mut out = Vec::new();
    walk(&forensic_root_path(&layout.ledger()), &mut out);
    out.sort();
    out
}

#[derive(Clone, Copy, Debug)]
enum KillPoint {
    /// SIGKILL at a seeded delay while every Child has work in flight.
    MidRun { delay_ms: u64 },
    /// SIGKILL right after Child 0 finishes all of its steps while the other
    /// Children are still mid-plan.
    AfterSomeChildrenFinish,
}

fn kill_point(rng: &mut Rng, iteration: usize) -> KillPoint {
    if iteration.is_multiple_of(2) {
        KillPoint::MidRun {
            delay_ms: rng.range(30, 900),
        }
    } else {
        KillPoint::AfterSomeChildrenFinish
    }
}

// Fields are read through the `Debug` iteration log line.
#[allow(dead_code)]
#[derive(Default, Debug)]
struct IterationReport {
    completed_before_kill: usize,
    in_flight_at_kill: usize,
    in_flight_run_states: BTreeMap<String, usize>,
    in_flight_retry_outcomes: BTreeMap<String, usize>,
    duplicate_ids_in_normal_operation: BTreeMap<String, usize>,
    refused_before_kill: Vec<String>,
    transient_refusals_retried: usize,
    ids_reused_by_restart_for_new_events: usize,
}

/// One full kill/restart/verify cycle on a fresh state root.
fn run_iteration(seed: u64, iteration: usize) -> Result<IterationReport, String> {
    let mut rng = Rng(seed ^ (iteration as u64).wrapping_mul(0xa076_1d64_78bd_642f));
    let point = kill_point(&mut rng, iteration);
    let context = format!("seed={seed:#x} iteration={iteration} kill={point:?}");
    let layout = Layout::new();
    let mut mother = Mother::start(&layout, 0).map_err(|e| format!("{context}: {e}"))?;
    let acquired = provision(&layout, true);
    let epoch_before = receiver_authority(&layout.socket()).unwrap();

    let plans = plan_steps(&mut rng, iteration);
    // Alternate concurrent callers (one per Child) with one serial caller
    // interleaving every Child's plan: concurrent load trips the
    // HeadSequenceMismatch refusals reported below, serial load does not.
    let serial = iteration % 4 >= 2;
    let plans = if serial {
        let mut queues: Vec<_> = plans.into_iter().map(Vec::into_iter).collect();
        let mut interleaved = Vec::new();
        while queues.iter().any(|queue| queue.len() != 0) {
            for queue in &mut queues {
                interleaved.extend(queue.next());
            }
        }
        vec![interleaved]
    } else {
        plans
    };
    let workload = Workload::start(layout.socket(), plans);
    match point {
        KillPoint::MidRun { delay_ms } => {
            thread::sleep(Duration::from_millis(delay_ms));
        }
        KillPoint::AfterSomeChildrenFinish => {
            let deadline = Instant::now() + Duration::from_secs(60);
            while !workload.finished().contains(&0) {
                assert!(
                    Instant::now() < deadline,
                    "{context}: Child 0 never finished"
                );
                thread::sleep(Duration::from_millis(2));
            }
        }
    }
    let completed_at_kill_decision = workload.completed_count();
    mother.sigkill();
    let steps = workload.join();
    drop(mother);

    let pre_kill = std::fs::read(layout.ledger()).unwrap();
    let pre_kill_entries = ledger_lines(&pre_kill).len();

    // ---- restart 1
    let mother = Mother::start(&layout, 1).map_err(|e| format!("{context}: {e}"))?;
    let entries = verify_chain(&layout, &format!("{context} restart=1"));
    let post = std::fs::read(layout.ledger()).unwrap();
    assert!(
        post.starts_with(&pre_kill),
        "{context}: committed pre-kill ledger bytes were rewritten"
    );
    assert!(
        entries.len() > pre_kill_entries,
        "{context}: no new tenure fact"
    );
    let reused = assert_restart_appended_only_new_facts(&entries, pre_kill_entries, &context);
    assert_trigger_calls_execute_at_most_once(&entries, &context);

    // Fresh writer tenure (D-R2.1) on restart.
    let epoch_after = receiver_authority(&layout.socket()).unwrap();
    assert_ne!(
        epoch_before["authority_epoch"], epoch_after["authority_epoch"],
        "{context}: restart reused the pre-crash authority epoch"
    );

    // Completed work is kept: one run, one invocation, recorded reply replays.
    let runs = runs_by_call(&layout);
    let mut report = IterationReport {
        ids_reused_by_restart_for_new_events: reused.len(),
        duplicate_ids_in_normal_operation: duplicate_observation_ids(&entries[..pre_kill_entries]),
        completed_before_kill: steps
            .iter()
            .filter(|step| matches!(step.state, StepState::Completed { .. }))
            .count(),
        ..IterationReport::default()
    };
    assert!(
        report.completed_before_kill >= completed_at_kill_decision,
        "{context}"
    );
    for step in &steps {
        match &step.state {
            StepState::Completed { inline_result } => {
                assert_eq!(
                    decode_result(inline_result),
                    json!(step.spin as usize + step.child + 1),
                    "{context}: wrong Child result for {}",
                    step.call_id
                );
                assert_eq!(
                    runs.get(&step.call_id).map(Vec::as_slice),
                    Some(&[MctRuntimeRunState::Completed][..]),
                    "{context}: completed {} lost or duplicated its run record",
                    step.call_id
                );
                assert_eq!(
                    invocations_for(&entries, &step.call_id),
                    1,
                    "{context}: completed {} invoked != once",
                    step.call_id
                );
                let replay = submit(&layout.socket(), step).unwrap();
                assert_eq!(
                    replay, step.state,
                    "{context}: retry of completed {} did not replay its recorded reply",
                    step.call_id
                );
            }
            StepState::Refused { body } => {
                report
                    .refused_before_kill
                    .push(format!("{}: {body}", step.call_id));
            }
            StepState::InFlight => {
                report.in_flight_at_kill += 1;
                let state = runs
                    .get(&step.call_id)
                    .map_or("no_run_record".to_owned(), |states| format!("{states:?}"));
                *report.in_flight_run_states.entry(state).or_default() += 1;
            }
        }
    }
    let entries = verify_chain(&layout, &format!("{context} after replays"));
    for step in &steps {
        if matches!(step.state, StepState::Completed { .. }) {
            assert_eq!(
                invocations_for(&entries, &step.call_id),
                1,
                "{context}: replay of {} re-invoked the Child",
                step.call_id
            );
        }
    }
    let runs_after_replay = runs_by_call(&layout);
    for step in &steps {
        if matches!(step.state, StepState::Completed { .. }) {
            assert_eq!(runs_after_replay[&step.call_id].len(), 1, "{context}");
        }
    }

    report.transient_refusals_retried = take_transient(&layout.socket());
    // Handles still resolve.
    assert_handles_resolve(&layout, &acquired, iteration, "post-restart", &context);

    // Record (not assert) what retrying in-flight work yields; the dedicated
    // in-flight test asserts the disposition.
    for step in steps
        .iter()
        .filter(|step| step.state == StepState::InFlight)
    {
        let outcome = match submit(&layout.socket(), step) {
            Ok(StepState::Completed { .. }) => "completed".to_owned(),
            Ok(StepState::Refused { body }) => {
                let reply: Value =
                    serde_json::from_str(body.split_once(' ').unwrap().1).unwrap_or(Value::Null);
                format!("{}/{}", reply["outcome"], reply["protocol_reason"])
            }
            Ok(StepState::InFlight) => unreachable!(),
            Err(error) => format!("transport:{error}"),
        };
        *report.in_flight_retry_outcomes.entry(outcome).or_default() += 1;
    }

    // ---- restart 2: SIGKILL an idle Mother and restart; nothing but the new
    // tenure may change.
    let summary_before = MctRuntimeStateStore::open(layout.state())
        .unwrap()
        .summary()
        .unwrap();
    let runs_before = runs_by_call(&layout);
    let forensics_before = forensic_listing(&layout);
    drop(mother);
    let idle_kill = std::fs::read(layout.ledger()).unwrap();
    let _mother = Mother::start(&layout, 2).map_err(|e| format!("{context}: {e}"))?;
    let entries_2 = verify_chain(&layout, &format!("{context} restart=2"));
    assert_restart_appended_only_new_facts(&entries_2, ledger_lines(&idle_kill).len(), &context);
    assert_trigger_calls_execute_at_most_once(&entries_2, &context);
    assert!(
        std::fs::read(layout.ledger())
            .unwrap()
            .starts_with(&idle_kill),
        "{context}: second restart rewrote committed bytes"
    );
    let re_executed: Vec<_> = entries_2[ledger_lines(&idle_kill).len()..]
        .iter()
        .filter(|entry| {
            entry
                .observation
                .call_id
                .as_ref()
                .is_some_and(|call_id| call_id.as_str().starts_with("call-i"))
        })
        .map(|entry| {
            format!(
                "{:?}:{}",
                entry.observation.kind, entry.observation.observation_id
            )
        })
        .collect();
    assert!(
        re_executed.is_empty(),
        "{context}: idle restart touched workload calls: {re_executed:?}"
    );
    let mut summary_after = MctRuntimeStateStore::open(layout.state())
        .unwrap()
        .summary()
        .unwrap();
    // Trigger occurrences keep accruing while Mother runs; compare the
    // restart-relevant projections.
    summary_after.trigger_records = summary_before.trigger_records;
    summary_after.current_trigger_records = summary_before.current_trigger_records;
    summary_after.pending_trigger_occurrences = summary_before.pending_trigger_occurrences;
    summary_after.active_trigger_firings = summary_before.active_trigger_firings;
    summary_after.runs = summary_before.runs;
    summary_after.completed_runs = summary_before.completed_runs;
    summary_after.failed_runs = summary_before.failed_runs;
    summary_after.metric_points = summary_before.metric_points;
    assert_eq!(
        summary_after, summary_before,
        "{context}: idle restart changed state"
    );
    let runs_after = runs_by_call(&layout);
    for (call_id, states) in &runs_before {
        if call_id.starts_with("call-i") {
            assert_eq!(
                runs_after.get(call_id),
                Some(states),
                "{context}: idle restart changed run {call_id}"
            );
        }
    }
    assert_eq!(
        forensic_listing(&layout),
        forensics_before,
        "{context}: idle restart touched forensic artifacts"
    );
    for step in steps
        .iter()
        .filter(|step| matches!(step.state, StepState::Completed { .. }))
    {
        assert_eq!(
            submit(&layout.socket(), step).unwrap(),
            step.state,
            "{context}: replay after second restart"
        );
    }
    assert_handles_resolve(&layout, &acquired, iteration, "second-restart", &context);
    Ok(report)
}

fn assert_handles_resolve(
    layout: &Layout,
    acquired: &Acquired,
    iteration: usize,
    phase: &str,
    context: &str,
) {
    let (status, body) = uds(
        &layout.socket(),
        "GET",
        "/status",
        None,
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(status, 200, "{context}");
    assert_eq!(
        body["resident"]["loaded_child_count"], CHILD_COUNT,
        "{context}: {body:#}"
    );
    assert_eq!(
        body["resident"]["approved_child_count"], CHILD_COUNT,
        "{context}: {body:#}"
    );
    let state = MctRuntimeStateStore::open(layout.state()).unwrap();
    for artifact_id in &acquired.artifact_ids {
        assert!(
            state
                .get_artifact(&mct_kernel::ComponentArtifactId::new(artifact_id).unwrap())
                .unwrap()
                .is_some(),
            "{context}: artifact {artifact_id} no longer resolves"
        );
    }
    drop(state);
    if let Some(handle) = &acquired.trigger_payload {
        assert_eq!(
            local_blob_store_for_state_path(layout.state())
                .fetch(handle)
                .unwrap_or_else(|error| panic!("{context}: trigger payload blob: {error:?}")),
            b"[2]",
            "{context}"
        );
    }
    for child in 0..CHILD_COUNT {
        // Fresh call per attempt: a concurrent-load "runtime unavailable"
        // failure is recorded under its key and cannot be retried.
        let mut last = None;
        for attempt in 0..20 {
            let call_id = format!("probe-i{iteration}-c{child}-{phase}-a{attempt}");
            let step = Step {
                child,
                last: false,
                spin: 1,
                key: format!("key-{call_id}"),
                call_id,
                state: StepState::InFlight,
            };
            match submit(&layout.socket(), &step).unwrap() {
                StepState::Completed { inline_result } => {
                    assert_eq!(decode_result(&inline_result), json!(child + 2), "{context}");
                    last = None;
                    break;
                }
                other => {
                    note_transient(&layout.socket());
                    last = Some(other);
                }
            }
        }
        assert!(
            last.is_none(),
            "{context}: Child {child} unreachable {phase}: {last:?}"
        );
    }
}

fn run_loop(default_iterations: u64) {
    let seed = env_u64("MCT_RESTART_SEED", DEFAULT_SEED);
    let iterations = env_u64("MCT_RESTART_ITERATIONS", default_iterations) as usize;
    let mut failures = Vec::new();
    let (mut completed, mut in_flight) = (0, 0);
    for iteration in 0..iterations {
        let started = Instant::now();
        match std::panic::catch_unwind(|| run_iteration(seed, iteration)) {
            Ok(Ok(report)) => {
                completed += report.completed_before_kill;
                in_flight += report.in_flight_at_kill;
                eprintln!(
                    "mother-restart seed={seed:#x} iteration={iteration} ok in {:?}: {report:?}",
                    started.elapsed()
                );
            }
            Ok(Err(message)) => {
                eprintln!("mother-restart iteration={iteration} FAILED: {message}");
                failures.push(iteration);
            }
            Err(_) => {
                eprintln!("mother-restart iteration={iteration} PANICKED (see above)");
                failures.push(iteration);
            }
        }
    }
    eprintln!(
        "mother-restart seed={seed:#x}: {}/{iterations} passed; {completed} steps completed \
         before a kill, {in_flight} in flight at a kill",
        iterations - failures.len()
    );
    assert!(
        failures.is_empty(),
        "mother-restart seed={seed:#x}: {}/{iterations} iterations failed: {failures:?}",
        failures.len()
    );
    assert!(
        completed > 0 && in_flight > 0,
        "the kills never landed with both completed and in-flight work"
    );
}

/// Simulate loss of recent NORMAL-synchronous SQLite commits without rolling
/// back the independently synced canonical ledger. All paths are test-private.
fn sqlite_rollback_case(kill_inflight: bool) {
    let layout = Layout::new();
    let mut mother = Mother::start(&layout, 0).unwrap();
    provision(&layout, false);
    let snapshot = layout.work.join("before-call.sqlite");
    rusqlite::Connection::open(layout.state())
        .unwrap()
        .execute("VACUUM INTO ?1", [snapshot.to_str().unwrap()])
        .unwrap();
    let step = Step {
        child: 1,
        last: true,
        spin: if kill_inflight { 1_500 } else { 1 },
        call_id: "call-sqlite-rollback".into(),
        key: "key-sqlite-rollback".into(),
        state: StepState::InFlight,
    };
    if kill_inflight {
        let workload = Workload::start(layout.socket(), vec![vec![step.clone()]]);
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let started = MctRuntimeStateStore::open(layout.state())
                .unwrap()
                .get_run_by_call_id(&CallId::new(&step.call_id).unwrap())
                .unwrap()
                .is_some();
            if started {
                break;
            }
            assert!(Instant::now() < deadline, "run never started");
            thread::sleep(Duration::from_millis(5));
        }
        thread::sleep(Duration::from_millis(100));
        mother.sigkill();
        assert_eq!(workload.join()[0].state, StepState::InFlight);
    } else {
        assert!(matches!(
            submit(&layout.socket(), &step).unwrap(),
            StepState::Completed { .. }
        ));
        mother.sigkill();
    }
    drop(mother);
    let before = verify_chain(&layout, "before SQLite rollback");
    let routes_before = before
        .iter()
        .filter(|entry| {
            entry.observation.call_id.as_ref().map(CallId::as_str) == Some(step.call_id.as_str())
                && entry.observation.kind == ObservationKind::RouteSelected
        })
        .count();
    assert_eq!(routes_before, 1);
    for suffix in ["-wal", "-shm"] {
        let path = PathBuf::from(format!("{}{suffix}", layout.state().display()));
        if path.exists() {
            std::fs::remove_file(path).unwrap();
        }
    }
    std::fs::copy(&snapshot, layout.state()).unwrap();
    assert!(
        MctRuntimeStateStore::open(layout.state())
            .unwrap()
            .get_run_by_call_id(&CallId::new(&step.call_id).unwrap())
            .unwrap()
            .is_none()
    );

    for boot in 1..=2 {
        let mut mother = Mother::start(&layout, boot).unwrap();
        let retry = submit(&layout.socket(), &step).unwrap();
        assert!(
            matches!(&retry, StepState::Refused { body } if body.contains("execution_interrupted")),
            "lost SQLite history must not authorize another execution: {retry:?}"
        );
        mother.sigkill();
        let entries = verify_chain(&layout, "after SQLite rollback retry");
        assert_eq!(
            entries
                .iter()
                .filter(|entry| {
                    entry.observation.call_id.as_ref().map(CallId::as_str)
                        == Some(step.call_id.as_str())
                        && entry.observation.kind == ObservationKind::RouteSelected
                })
                .count(),
            routes_before,
            "retry re-entered execution routing"
        );
    }
}

#[test]
fn sqlite_rollback_after_completed_call_never_reexecutes() {
    sqlite_rollback_case(false);
}

#[test]
fn sqlite_rollback_after_sigkill_never_reexecutes() {
    sqlite_rollback_case(true);
}

#[test]
fn mother_survives_sigkill_mid_run_with_active_children() {
    run_loop(3);
}

#[test]
#[ignore = "long soak; run with --ignored and MCT_RESTART_ITERATIONS"]
fn mother_survives_sigkill_mid_run_with_active_children_long() {
    run_loop(24);
}

/// In-flight disposition. A call that is inside the Child when Mother is
/// SIGKILLed must, after restart, either resume to completion or be
/// deterministically closed as interrupted, so a retry with the same
/// idempotency key gets a final answer instead of waiting on a run no
/// process owns.
#[test]
fn inflight_child_call_is_resumed_or_marked_interrupted_after_sigkill() {
    let layout = Layout::new();
    let mut mother = Mother::start(&layout, 0).unwrap();
    provision(&layout, false);
    let step = Step {
        child: 1,
        last: true,
        // ~1.5 s of guest work keeps the call inside the Child at the kill.
        spin: 1_500,
        call_id: "call-inflight-c1".into(),
        key: "key-call-inflight-c1".into(),
        state: StepState::InFlight,
    };
    let workload = Workload::start(layout.socket(), vec![vec![step.clone()]]);
    // Wait until the run is durably started, then kill inside the Child.
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let started = MctRuntimeStateStore::open(layout.state())
            .unwrap()
            .get_run_by_call_id(&CallId::new(&step.call_id).unwrap())
            .unwrap()
            .is_some();
        if started {
            break;
        }
        assert!(Instant::now() < deadline, "run never started");
        thread::sleep(Duration::from_millis(5));
    }
    thread::sleep(Duration::from_millis(100));
    mother.sigkill();
    let steps = workload.join();
    drop(mother);
    assert_eq!(
        steps[0].state,
        StepState::InFlight,
        "kill landed after the reply"
    );

    let mut mother = Mother::start(&layout, 1).unwrap();
    let entries = verify_chain(&layout, "in-flight restart");
    let call_id = CallId::new(&step.call_id).unwrap();
    // Design: the Child is never re-executed (no retry executes the Child
    // twice; Mother never reaches into Child state to resume it). The sweep
    // runs before the control socket listens, so it has settled by now.
    let run = MctRuntimeStateStore::open(layout.state())
        .unwrap()
        .get_run_by_call_id(&call_id)
        .unwrap()
        .expect("the started run record survives the crash");
    assert_ne!(
        run.state,
        MctRuntimeRunState::Running,
        "GAP: run {call_id} is still `running` after restart; no live process owns it \
         and nothing resumed it or marked it interrupted"
    );
    let result = run.result.as_ref().expect("interrupted run has a result");
    assert_eq!(result.outcome, ResultOutcome::Failed);
    assert!(
        result
            .requester_message
            .contains("interrupted by Mother restart"),
        "run result does not say interrupted: {result:?}"
    );
    let interruption_facts = |entries: &[MctObservationLedgerEntry]| {
        entries
            .iter()
            .filter(|entry| {
                entry.observation.call_id.as_ref() == Some(&call_id)
                    && entry
                        .observation
                        .observation_id
                        .as_str()
                        .starts_with("obs:call-restart-interrupted:")
            })
            .count()
    };
    // Local execution facts are written with the run's report, which the
    // kill cut off, so route selection is the per-execution ledger marker.
    let executions = |entries: &[MctObservationLedgerEntry]| {
        entries
            .iter()
            .filter(|entry| {
                entry.observation.call_id.as_ref() == Some(&call_id)
                    && matches!(
                        entry.observation.kind,
                        ObservationKind::RouteSelected | ObservationKind::RuntimeExecutionStarted
                    )
            })
            .count()
    };
    assert_eq!(interruption_facts(&entries), 1, "one interruption fact");
    assert_eq!(
        executions(&entries),
        1,
        "the call was routed once before the kill"
    );
    let run_once = |label: &str| {
        let state = MctRuntimeStateStore::open(layout.state()).unwrap();
        let again = state.get_run_by_call_id(&call_id).unwrap().unwrap();
        assert_eq!(again.run_id, run.run_id, "{label}: a new run started");
        assert_eq!(
            again.result, run.result,
            "{label}: the interrupted result changed"
        );
    };

    let retry = submit(&layout.socket(), &step).unwrap();
    assert!(
        !matches!(&retry, StepState::Refused { body } if body.contains("idempotency_in_progress")),
        "GAP: retry with the same idempotency key is refused as in-progress after restart: {retry:?}"
    );
    match &retry {
        StepState::Refused { body } => {
            assert!(
                body.contains("interrupted by Mother restart") && body.contains("\"failed\""),
                "same-key retry did not replay the interrupted outcome: {body}"
            );
            let reply: Value = serde_json::from_str(body.split_once(' ').unwrap().1).unwrap();
            assert_eq!(
                reply["protocol_reason"], "execution_interrupted",
                "the interrupted reply lacks its machine-readable reason: {body}"
            );
        }
        other => panic!("same-key retry re-executed or completed a dead call: {other:?}"),
    }
    let entries = verify_chain(&layout, "in-flight retry");
    assert_eq!(
        executions(&entries),
        1,
        "the same-key retry re-executed the call"
    );
    run_once("retry");

    // A second crash and restart settles nothing twice.
    mother.sigkill();
    drop(mother);
    let _mother = Mother::start(&layout, 2).unwrap();
    let entries = verify_chain(&layout, "in-flight second restart");
    assert_eq!(
        interruption_facts(&entries),
        1,
        "second restart re-interrupted"
    );
    assert_eq!(
        executions(&entries),
        1,
        "second restart re-executed the Child"
    );
    assert_eq!(submit(&layout.socket(), &step).unwrap(), retry);
    run_once("second restart");
}

/// Torn ledger write. SIGKILL mid-run, then leave a torn, unterminated final
/// frame on the ledger: the on-disk state a crash inside a frame write
/// produces. The binary has no hook to stop inside the ledger's `write_all`
/// (the append fault injector is `#[cfg(test)]`, compiled out of the
/// daemon), so the test writes the torn bytes itself. Per the ratified
/// ledger law (ledger-commit-recovery B1), an unterminated final frame is
/// crash residue: it is preserved as forensics, set aside, and appends
/// resume at the committed head.
#[test]
fn torn_final_ledger_frame_is_recovered_on_restart() {
    let seed = env_u64("MCT_RESTART_SEED", DEFAULT_SEED);
    let mut rng = Rng(seed ^ 0x746f_726e);
    let layout = Layout::new();
    let mut mother = Mother::start(&layout, 0).unwrap();
    let acquired = provision(&layout, true);
    let workload = Workload::start(layout.socket(), plan_steps(&mut rng, 0));
    thread::sleep(Duration::from_millis(rng.range(100, 600)));
    mother.sigkill();
    let steps = workload.join();
    drop(mother);

    let committed = std::fs::read(layout.ledger()).unwrap();
    let last = committed[..committed.len() - 1]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    let frame = &committed[last..];
    let keep = (frame.len() as u64 * rng.range(10, 90) / 100).max(1) as usize;
    let torn = frame[..keep].to_vec();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(layout.ledger())
        .unwrap();
    file.write_all(&torn).unwrap();
    file.sync_all().unwrap();
    drop(file);
    let on_disk = std::fs::read(layout.ledger()).unwrap();

    let boot = Mother::start(&layout, 1);
    if let Err(message) = &boot {
        // Whatever the posture, the evidence must not be destroyed.
        assert_eq!(
            std::fs::read(layout.ledger()).unwrap(),
            on_disk,
            "failed boot modified the torn ledger"
        );
        // And the refusal is permanent, not a transient: boot again.
        let again = Mother::start(&layout, 2).err().unwrap_or_default();
        panic!(
            "GAP: Mother does not boot over a torn final ledger frame ({} committed entries, \
             {} torn bytes, {} completed steps before kill). First boot: {message}\n\
             Second boot: {again}",
            ledger_lines(&committed).len(),
            torn.len(),
            steps
                .iter()
                .filter(|step| matches!(step.state, StepState::Completed { .. }))
                .count()
        );
    }
    let mut mother = boot.unwrap();
    let entries = verify_chain(&layout, "torn restart");
    assert!(
        std::fs::read(layout.ledger())
            .unwrap()
            .starts_with(&committed),
        "recovery rewrote committed bytes"
    );
    assert_restart_appended_only_new_facts(&entries, ledger_lines(&committed).len(), "torn");
    // Exactly one recovery fact, immediately after the committed prefix.
    let recovery: Vec<_> = entries
        .iter()
        .filter(|entry| {
            entry
                .observation
                .observation_id
                .as_str()
                .starts_with("ledger-tail-recovery-")
        })
        .collect();
    assert_eq!(recovery.len(), 1, "expected one recovery observation");
    assert_eq!(
        recovery[0].local_sequence,
        ledger_lines(&committed).len() as u64,
        "the recovery fact is the first append after the committed head"
    );
    // The torn bytes are preserved exactly, with the diagnostic record.
    let forensics = forensic_listing(&layout);
    let source = forensics
        .iter()
        .find(|(path, _)| path.ends_with("source.bin"))
        .expect("torn residue was not preserved as forensics");
    assert_eq!(std::fs::read(&source.0).unwrap(), torn);
    assert!(
        forensics
            .iter()
            .any(|(path, _)| path.ends_with("record.json"))
    );
    assert_handles_resolve(&layout, &acquired, 0, "torn-restart", "torn restart");

    // A second crash and restart recovers nothing more and keeps the evidence.
    mother.sigkill();
    drop(mother);
    let before_second = std::fs::read(layout.ledger()).unwrap();
    let _mother = Mother::start(&layout, 2).unwrap();
    let after = verify_chain(&layout, "torn second restart");
    assert!(
        std::fs::read(layout.ledger())
            .unwrap()
            .starts_with(&before_second)
    );
    assert_eq!(
        after
            .iter()
            .filter(|entry| entry
                .observation
                .observation_id
                .as_str()
                .starts_with("ledger-tail-recovery-"))
            .count(),
        1,
        "second restart recovered again"
    );
    assert_eq!(forensic_listing(&layout), forensics);
}

/// Not a crash case: the control run with no kill. Concurrent local callers
/// (one per Child) plus a live trigger must not get calls refused or failed
/// by Mother's own authority-projection bookkeeping. Serially driven, the same
/// plans complete with zero refusals.
#[test]
fn concurrent_children_calls_are_not_refused_without_a_crash() {
    let layout = Layout::new();
    let _mother = Mother::start(&layout, 0).unwrap();
    provision(&layout, true);
    let mut rng = Rng(env_u64("MCT_RESTART_SEED", DEFAULT_SEED));
    let plans: Vec<Vec<Step>> = plan_steps(&mut rng, 900)
        .into_iter()
        .map(|plan| {
            let mut plan: Vec<Step> = plan.into_iter().take(8).collect();
            plan.last_mut().unwrap().last = true;
            plan
        })
        .collect();
    let workload = Workload::start(layout.socket(), plans);
    let deadline = Instant::now() + Duration::from_secs(120);
    while workload.finished().len() < CHILD_COUNT {
        assert!(Instant::now() < deadline, "workload did not finish");
        thread::sleep(Duration::from_millis(10));
    }
    let steps = workload.join();
    let refused: Vec<_> = steps
        .iter()
        .filter_map(|step| match &step.state {
            StepState::Refused { body } => Some(format!("{}: {body}", step.call_id)),
            _ => None,
        })
        .collect();
    let retried = take_transient(&layout.socket());
    assert!(
        refused.is_empty() && retried == 0,
        "GAP: {} of {} calls refused/failed and {retried} more needed a retry under \
         concurrent load with no crash (Mother log: AuthorityProjection(HeadSequenceMismatch)):\n{}",
        refused.len(),
        steps.len(),
        refused.join("\n")
    );
}

/// A crash on the very first ledger write leaves a ledger holding only the
/// tail-recovery fact. Mother must not start on its own (no continuity
/// evidence), the owner's operator gate must be able to reinitialize over that
/// recovery evidence, and the next boot must come up ready with the recovery
/// fact still first in the verified chain.
#[test]
fn crash_on_first_ledger_write_recovers_then_operator_gate_starts_mother() {
    let layout = Layout::new();
    let torn = br#"{"ledger_id":"ledger-local","mother_node_id":"local-mct","local_seq"#;
    std::fs::write(layout.ledger(), torn).unwrap();

    // Boot 1 recovers the torn frame and waits behind the operator gate.
    let mut gated = Mother::spawn(&layout, 1);
    let deadline = Instant::now() + READY_TIMEOUT;
    let startup = loop {
        assert!(
            gated.process.try_wait().unwrap().is_none(),
            "Mother exited before exposing the operator gate; log:\n{}",
            gated.log(&layout)
        );
        if let Ok((200, body)) = uds(
            &layout.socket(),
            "GET",
            "/startup",
            None,
            Duration::from_secs(2),
        ) && body["startup_posture"] == "operator_gate_required"
        {
            break body;
        }
        assert!(
            Instant::now() < deadline,
            "no operator gate; log:\n{}",
            gated.log(&layout)
        );
        thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(startup["startup_class"], "operator_gated_nonvirgin");
    let recovered = std::fs::read(layout.ledger()).unwrap();
    let before_gate = ledger_lines(&recovered);
    assert_eq!(before_gate.len(), 1, "only the recovery fact");
    assert!(mct_observation::is_valid_tail_recovery_entry(
        &before_gate[0]
    ));
    let source = forensic_listing(&layout)
        .into_iter()
        .find(|(path, _)| path.ends_with("source.bin"))
        .expect("torn bytes preserved");
    assert_eq!(std::fs::read(&source.0).unwrap(), torn);

    let request = json!({
        "schema": "mct-operator-startup-gate-request/v1",
        "decision_id": "decision:first-write-crash",
        "expected_mother_node_id": MOTHER_NODE_ID,
        "expected_ledger_id": LEDGER_ID,
        "expected_inventory_hash": startup["inventory"]["inventory_hash"],
        "confirmation": mct_daemon::MCT_OPERATOR_REINITIALIZATION_CONFIRMATION_V1,
    });
    let (status, body) = uds(
        &layout.socket(),
        "POST",
        "/startup/operator-gate",
        Some(&request),
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(status, 200, "gate refused: {body:#}");
    // The isolated plane ends after an accepted gate.
    let exit_deadline = Instant::now() + READY_TIMEOUT;
    while gated.process.try_wait().unwrap().is_none() {
        assert!(Instant::now() < exit_deadline, "gated Mother did not exit");
        thread::sleep(Duration::from_millis(25));
    }
    drop(gated);

    // Boot 2 is an ordinary reopen and becomes ready.
    let _mother = Mother::start(&layout, 2).unwrap_or_else(|error| panic!("{error}"));
    let entries = verify_chain(&layout, "after operator gate");
    assert!(
        std::fs::read(layout.ledger())
            .unwrap()
            .starts_with(&recovered),
        "the recovery fact was rewritten"
    );
    assert_eq!(entries[0], before_gate[0]);
    let epochs: Vec<_> = entries
        .iter()
        .filter(|entry| {
            entry
                .observation
                .observation_id
                .as_str()
                .starts_with("obs:authority-epoch:")
        })
        .collect();
    assert!(epochs.len() >= 2, "gate epoch plus the reopen epoch");
    assert_eq!(
        epochs[0].local_sequence, 1,
        "gate epoch chains after recovery"
    );
    let gate_detail = epochs[0].observation.detail_ref.as_deref().unwrap();
    assert!(gate_detail.contains("\"operator_gated_nonvirgin\""));
    assert!(gate_detail.contains("\"none_after_operator_reinitialization\""));
    assert!(gate_detail.contains("decision:first-write-crash"));
    let reopen_detail = epochs[1].observation.detail_ref.as_deref().unwrap();
    assert!(reopen_detail.contains("\"ordinary_reopen\""));
}

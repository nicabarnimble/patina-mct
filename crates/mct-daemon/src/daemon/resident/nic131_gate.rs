//! NIC-131 witness only. Compiled only with the `nic131-witness` feature.
//!
//! The governed config publisher already runs one synchronous span after its
//! `projected_authority` await returns: it reads the snapshot and writes
//! `config.json` without yielding. This gate records that span and can hold
//! it, exactly once, inside the span. Holding blocks the current thread; it adds
//! no await, so the task yields nowhere it did not already yield. A held span is
//! what a preempted worker thread or slow storage would look like from outside.
//!
//! With `NIC131_GATE_DIR` unset the gate does nothing. Inside that directory:
//! - `trace.jsonl` receives one line per event;
//! - a file named `pause-next` makes the next span hold, and is consumed;
//! - the held span writes `paused-<seq>.json` and waits for `release`, which it
//!   consumes, for at most `NIC131_GATE_MAX_HOLD_MS` (default 30000 ms);
//! - a file named `fail-next` makes the next span report a write failure
//!   without writing, and is consumed. Reads are untouched.
//!
//! Two more boundaries use the same directory:
//! - Inbound binding reads. Each finished read is traced as `binding_read`.
//!   `binding-pause-next` holds the next one after it read and before it
//!   returns to the hello or call evaluator, which reads the receiver
//!   authority next. It writes `binding-paused-<seq>.json` and waits for
//!   `binding-release`.
//! - The resident writer. `writer-fence-next` sets the writer's own fence flag
//!   when it receives its next command; `writer-stop-next` makes the writer
//!   task end there, as a dead writer would. Both are consumed.
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) struct Span {
    dir: Option<PathBuf>,
    seq: u64,
    fail: bool,
}

fn now() -> String {
    jiff::Timestamp::now().to_string()
}

fn thread() -> serde_json::Value {
    let current = std::thread::current();
    serde_json::json!({
        "name": current.name().unwrap_or(""),
        "id": format!("{:?}", current.id()),
    })
}

fn append(dir: &std::path::Path, event: serde_json::Value) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("trace.jsonl"))
    {
        let _ = writeln!(file, "{event}");
        let _ = file.sync_all();
    }
}

/// File and trace-event names for one hold boundary.
struct HoldNames {
    pause: &'static str,
    paused: &'static str,
    release: &'static str,
    held: &'static str,
    released: &'static str,
    timeout: &'static str,
}

const PUBLICATION_HOLD: HoldNames = HoldNames {
    pause: "pause-next",
    paused: "paused",
    release: "release",
    held: "hold",
    released: "released",
    timeout: "hold_timeout",
};

const BINDING_HOLD: HoldNames = HoldNames {
    pause: "binding-pause-next",
    paused: "binding-paused",
    release: "binding-release",
    held: "binding_hold",
    released: "binding_released",
    timeout: "binding_hold_timeout",
};

/// Holds the current thread once, if the pause file exists, until the release
/// file appears or the hold limit passes. Adds no await.
fn hold(dir: &std::path::Path, seq: u64, names: &HoldNames, marker: serde_json::Value) {
    let HoldNames {
        pause,
        paused,
        release,
        held,
        released,
        timeout,
    } = names;
    if std::fs::remove_file(dir.join(pause)).is_err() {
        return;
    }
    let _ = std::fs::write(dir.join(format!("{paused}-{seq}.json")), marker.to_string());
    append(
        dir,
        serde_json::json!({"event": held, "seq": seq, "at": now(), "thread": thread()}),
    );
    let max_hold = std::env::var("NIC131_GATE_MAX_HOLD_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(30_000);
    let deadline = Instant::now() + Duration::from_millis(max_hold);
    let release_file = dir.join(release);
    let was_released = loop {
        if std::fs::remove_file(&release_file).is_ok() {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    append(
        dir,
        serde_json::json!({
            "event": if was_released { released } else { timeout },
            "seq": seq, "at": now(), "thread": thread(),
        }),
    );
}

fn gate_dir() -> Option<PathBuf> {
    std::env::var_os("NIC131_GATE_DIR").map(PathBuf::from)
}

/// Called by the inbound binding provider with its finished read, before it
/// returns to the evaluator. `outcome` is the peer state it read or its error.
pub(super) fn binding_read(outcome: serde_json::Value) {
    let Some(dir) = gate_dir() else {
        return;
    };
    let seq = SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1;
    append(
        &dir,
        serde_json::json!({
            "event": "binding_read", "seq": seq, "at": now(), "thread": thread(),
            "outcome": outcome,
        }),
    );
    hold(&dir, seq, &BINDING_HOLD, serde_json::json!({"seq": seq}));
}

/// A fault for the resident writer to take when it receives its next command.
pub(crate) enum WriterFault {
    Fence,
    Stop,
}

pub(crate) fn writer_fault() -> Option<WriterFault> {
    let dir = gate_dir()?;
    let fault = if std::fs::remove_file(dir.join("writer-fence-next")).is_ok() {
        WriterFault::Fence
    } else if std::fs::remove_file(dir.join("writer-stop-next")).is_ok() {
        WriterFault::Stop
    } else {
        return None;
    };
    append(
        &dir,
        serde_json::json!({
            "event": match fault { WriterFault::Fence => "writer_fenced", WriterFault::Stop => "writer_stopped" },
            "at": now(), "thread": thread(),
        }),
    );
    Some(fault)
}

/// Called after the snapshot is captured and before `config.json` is written.
pub(super) fn enter(generation: Option<u64>, runtime: &mct_observation::GovernedRuntimeV1) -> Span {
    let Some(dir) = std::env::var_os("NIC131_GATE_DIR").map(PathBuf::from) else {
        return Span {
            dir: None,
            seq: 0,
            fail: false,
        };
    };
    let seq = SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1;
    let peers: serde_json::Map<String, serde_json::Value> = runtime
        .peers
        .iter()
        .map(|(name, peer)| {
            (
                name.clone(),
                serde_json::to_value(peer.state).unwrap_or(serde_json::Value::Null),
            )
        })
        .collect();
    append(
        &dir,
        serde_json::json!({
            "event": "enter", "seq": seq, "at": now(), "thread": thread(),
            "snapshot_generation": generation, "snapshot_peers": peers,
        }),
    );
    hold(
        &dir,
        seq,
        &PUBLICATION_HOLD,
        serde_json::json!({"seq": seq, "snapshot_generation": generation}),
    );
    let fail = std::fs::remove_file(dir.join("fail-next")).is_ok();
    if fail {
        append(
            &dir,
            serde_json::json!({"event": "inject_write_failure", "seq": seq, "at": now(), "thread": thread()}),
        );
    }
    Span {
        dir: Some(dir),
        seq,
        fail,
    }
}

impl Span {
    /// True when this span must report a write failure instead of publishing.
    pub(super) fn injected_write_failure(&self) -> bool {
        self.fail
    }

    /// Called with the publisher's own result, before it is returned.
    pub(super) fn exit(self, result: &anyhow::Result<()>) {
        if let Some(dir) = self.dir {
            append(
                &dir,
                serde_json::json!({
                    "event": "exit", "seq": self.seq, "at": now(), "thread": thread(),
                    "published": result.is_ok(),
                    "error": result.as_ref().err().map(|error| format!("{error:#}")),
                }),
            );
        }
    }
}

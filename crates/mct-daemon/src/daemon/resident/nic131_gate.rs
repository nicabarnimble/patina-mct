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
    let pause = dir.join("pause-next");
    if std::fs::remove_file(&pause).is_ok() {
        let _ = std::fs::write(
            dir.join(format!("paused-{seq}.json")),
            serde_json::json!({"seq": seq, "snapshot_generation": generation}).to_string(),
        );
        append(
            &dir,
            serde_json::json!({"event": "hold", "seq": seq, "at": now(), "thread": thread()}),
        );
        let max_hold = std::env::var("NIC131_GATE_MAX_HOLD_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(30_000);
        let deadline = Instant::now() + Duration::from_millis(max_hold);
        let release = dir.join("release");
        let released = loop {
            if std::fs::remove_file(&release).is_ok() {
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        append(
            &dir,
            serde_json::json!({
                "event": if released { "released" } else { "hold_timeout" },
                "seq": seq, "at": now(), "thread": thread(),
            }),
        );
    }
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

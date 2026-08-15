use anyhow::{Context, Result, anyhow, bail};
use mct_daemon::{
    MctChildLoadOptions, MctDaemonConfigStore, MctIdempotencyReservation, MctOperatorNodeScope,
    MctRuntimeStateStore, MctWasmComponentRuntime, MctWasmHostConfig, load_children_from_dir,
    local_execution_authority_snapshot,
};
use mct_kernel::{
    CallId, MctIdempotencyFingerprint, MctObservation, ObservationId, ObservationKind,
    ObservationOutcome, ObservationTraceRef, ObservationVisibility, SourcePlane, Timestamp,
    TraceId,
};
use mct_observation::JsonlObservationLedger;
use serde_json::{Value, json};
use std::{
    env, fs,
    hint::black_box,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{Duration, Instant},
};
use tempfile::TempDir;
use wasmtime::{Config, Engine, component::Component};

const COVERED_REVISION: &str = "ead8796d5143d0f9da623057dadc5c920c47bf2b";
const CAVEAT: &str = "Dev-launched, harness-supervised, unsupervised-by-launchd release-Cargo-profile binary; not a release artifact and not directly comparable to BASELINES-v0.2.0-aarch64-apple-darwin.md.";
const MAX_JSON_BYTES: usize = 5_000_000;

#[derive(Debug)]
struct Args {
    fixtures: PathBuf,
    output: PathBuf,
}

fn parse_args() -> Result<Args> {
    let mut values = env::args().skip(1);
    let mut fixtures = None;
    let mut output = None;
    while let Some(value) = values.next() {
        match value.as_str() {
            "--fixtures" => fixtures = values.next().map(PathBuf::from),
            "--output" => output = values.next().map(PathBuf::from),
            // Cargo appends this libtest-style marker even for harness = false benches.
            "--bench" => {}
            other => bail!("unknown perf_phase_0 argument: {other}"),
        }
    }
    let fixtures = fixtures.context("missing --fixtures")?;
    let fixtures = if fixtures.is_absolute() || fixtures.exists() {
        fixtures
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(fixtures)
    };
    Ok(Args {
        fixtures: fixtures
            .canonicalize()
            .context("resolve --fixtures from Cargo workspace or process directory")?,
        output: output.context("missing --output")?,
    })
}

fn command_output(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        bail!(
            "{} {:?} failed: status={}; stderr={}",
            program,
            args,
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn host_preflight() -> Result<Value> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        bail!("component-cost measurements require aarch64-apple-darwin");
    }
    let rustc_verbose = command_output("rustc", &["-vV"])?;
    if !rustc_verbose
        .lines()
        .any(|line| line == "host: aarch64-apple-darwin")
    {
        bail!("rustc host is not aarch64-apple-darwin");
    }
    let power = command_output("/usr/bin/pmset", &["-g", "batt"])?;
    if !power.contains("Now drawing from 'AC Power'") {
        bail!("component-cost measurements require AC power: {power}");
    }
    Ok(json!({
        "captured_at": now_string(),
        "covered_revision": COVERED_REVISION,
        "bench_revision": command_output("git", &["rev-parse", "HEAD"]).unwrap_or_else(|error| error.to_string()),
        "working_tree_porcelain": command_output("git", &["status", "--porcelain", "--untracked-files=all"]).unwrap_or_else(|error| error.to_string()),
        "cargo_profile": "bench_optimized",
        "rustc_verbose": rustc_verbose,
        "cargo_version": command_output("cargo", &["--version"]).unwrap_or_else(|error| error.to_string()),
        "uname": command_output("/usr/bin/uname", &["-a"]).unwrap_or_else(|error| error.to_string()),
        "hardware_model": command_output("/usr/sbin/sysctl", &["-n", "hw.model"]).unwrap_or_else(|error| error.to_string()),
        "chip_or_cpu": command_output("/usr/sbin/sysctl", &["-n", "machdep.cpu.brand_string"]).unwrap_or_else(|error| error.to_string()),
        "logical_cpus": command_output("/usr/sbin/sysctl", &["-n", "hw.logicalcpu"]).unwrap_or_else(|error| error.to_string()),
        "memory_bytes": command_output("/usr/sbin/sysctl", &["-n", "hw.memsize"]).unwrap_or_else(|error| error.to_string()),
        "power": power,
        "power_custom": command_output("/usr/bin/pmset", &["-g", "custom"]).unwrap_or_else(|error| error.to_string()),
        "load_average": command_output("/usr/bin/uptime", &[]).unwrap_or_else(|error| error.to_string()),
        "load_note": env::var("MCT_PERF_LOAD_NOTE").unwrap_or_else(|_| "not supplied; correlate with host.json from the same official run".into()),
    }))
}

fn now_string() -> String {
    jiff::Timestamp::now().to_string()
}

fn nearest_rank(values: &[u64], percentile: f64) -> u64 {
    assert!(!values.is_empty());
    let mut ordered = values.to_vec();
    ordered.sort_unstable();
    let rank = ((percentile * ordered.len() as f64).ceil() as usize)
        .saturating_sub(1)
        .min(ordered.len() - 1);
    ordered[rank]
}

fn sample_record(warmups: usize, samples: Vec<u64>) -> Value {
    json!({
        "warmups": warmups,
        "sample_count": samples.len(),
        "unit": "nanoseconds",
        "percentile_method": "nearest_rank_sorted_ceil_p_times_n_minus_1",
        "p50_ns": nearest_rank(&samples, 0.50),
        "p95_ns": nearest_rank(&samples, 0.95),
        "max_ns": samples.iter().copied().max().unwrap(),
        "samples_ns": samples,
    })
}

fn run_samples<T, F>(warmups: usize, samples: usize, mut operation: F) -> Result<Vec<u64>>
where
    F: FnMut(usize) -> Result<T>,
{
    for index in 0..warmups {
        black_box(operation(index)?);
    }
    let mut values = Vec::with_capacity(samples);
    for index in 0..samples {
        let started = Instant::now();
        let result = operation(warmups + index)?;
        let elapsed = started.elapsed();
        black_box(result);
        values.push(
            elapsed
                .as_nanos()
                .try_into()
                .context("sample exceeds u64 nanoseconds")?,
        );
    }
    Ok(values)
}

fn runtime_engine_cost() -> Result<Value> {
    let samples = run_samples(20, 200, |_| {
        MctWasmComponentRuntime::new(MctWasmHostConfig::default_local()).map_err(anyhow::Error::msg)
    })?;
    Ok(sample_record(20, samples))
}

fn component_engine() -> Result<Engine> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.epoch_interruption(true);
    Engine::new(&config)
        .map_err(|error| anyhow!("construct matching Wasmtime component engine: {error}"))
}

fn fixture_metadata(path: &Path) -> Result<Value> {
    let bytes = fs::read(path)?;
    Ok(json!({
        "path": path,
        "bytes": bytes.len(),
        "blake3": blake3::hash(&bytes).to_hex().to_string(),
    }))
}

fn purge_failure(output: &Output) -> Value {
    json!({
        "command": ["/usr/sbin/purge"],
        "status": output.status.code(),
        "signal_or_description": output.status.to_string(),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr),
    })
}

fn cold_component_cost(path: &Path) -> Result<Value> {
    let mut samples = Vec::with_capacity(10);
    for _ in 0..10 {
        let purge = Command::new("/usr/sbin/purge").output();
        let output = match purge {
            Ok(output) if output.status.success() => output,
            Ok(output) => {
                return Ok(json!({
                    "status": "cold_unavailable",
                    "method": "Component::from_file after verified /usr/sbin/purge success; fresh matching Engine constructed outside timed interval",
                    "failure": purge_failure(&output),
                    "samples_ns": [],
                }));
            }
            Err(error) => {
                return Ok(json!({
                    "status": "cold_unavailable",
                    "method": "Component::from_file after verified /usr/sbin/purge success; fresh matching Engine constructed outside timed interval",
                    "failure": {
                        "command": ["/usr/sbin/purge"],
                        "io_error": error.to_string(),
                    },
                    "samples_ns": [],
                }));
            }
        };
        black_box(output);
        let engine = component_engine()?;
        let started = Instant::now();
        let component = Component::from_file(&engine, path)
            .map_err(|error| anyhow!("compile cold component {}: {error}", path.display()))?;
        let elapsed = started.elapsed();
        black_box(component);
        samples.push(elapsed.as_nanos().try_into()?);
    }
    Ok(json!({
        "status": "measured",
        "method": "Component::from_file after verified /usr/sbin/purge success; fresh matching Engine constructed outside timed interval",
        "purge_command": ["/usr/sbin/purge"],
        "result": sample_record(0, samples),
    }))
}

fn warm_component_cost(path: &Path) -> Result<Value> {
    black_box(fs::read(path)?);
    let engine = component_engine()?;
    let samples = run_samples(10, 100, |_| {
        Component::from_file(&engine, path)
            .map_err(|error| anyhow!("compile warm component {}: {error}", path.display()))
    })?;
    Ok(json!({
        "status": "measured",
        "method": "fixture pre-read, one matching Engine, 10 discarded compiles, then Component::from_file samples",
        "result": sample_record(10, samples),
    }))
}

fn component_costs(fixtures: &Path) -> Result<Value> {
    let cases = [
        (
            "watch-null-sink",
            fixtures.join("watch-null-sink-0.1.0/watch-null-sink.wasm"),
        ),
        (
            "slate-manager",
            fixtures.join("slate-manager-0.2.0/slate-manager.wasm"),
        ),
    ];
    let mut result = serde_json::Map::new();
    for (name, path) in cases {
        if !path.is_file() {
            bail!("component fixture missing: {}", path.display());
        }
        result.insert(
            name.into(),
            json!({
                "fixture": fixture_metadata(&path)?,
                "cold": cold_component_cost(&path)?,
                "warm": warm_component_cost(&path)?,
            }),
        );
    }
    Ok(Value::Object(result))
}

fn copy_child_fixture(fixtures: &Path, root: &Path) -> Result<(PathBuf, PathBuf)> {
    // Reproduce the staged catalog shape emitted by the resident acquisition path.
    let package = root.join("artifacts/sha256/perf-phase-0-watch-null-sink");
    let artifact_dir = package.join("artifact");
    fs::create_dir_all(&artifact_dir)?;
    let manifest = package.join("child.toml");
    let component = artifact_dir.join("watch-null-sink.wasm");
    let mut manifest_text = fs::read_to_string(fixtures.join("watch-null-sink-0.1.0/child.toml"))?;
    manifest_text.push_str("\n[child.artifact]\nwasm = \"artifact/watch-null-sink.wasm\"\n");
    fs::write(&manifest, manifest_text)?;
    fs::copy(
        fixtures.join("watch-null-sink-0.1.0/watch-null-sink.wasm"),
        &component,
    )?;
    Ok((manifest, component))
}

fn child_load_cost(fixtures: &Path, temp: &TempDir) -> Result<Value> {
    let children = temp.path().join("child-load");
    fs::create_dir(&children)?;
    let (manifest, component) = copy_child_fixture(fixtures, &children)?;
    let control_samples = run_samples(20, 200, |_| -> Result<usize> {
        let manifest_bytes = fs::read(&manifest)?;
        let component_bytes = fs::read(&component)?;
        Ok(black_box(manifest_bytes.len() + component_bytes.len()))
    })?;
    let load_samples = run_samples(20, 200, |_| {
        let report = load_children_from_dir(MctChildLoadOptions::new(&children));
        if report.loaded != 1 || report.failed != 0 {
            bail!("unexpected Child load report: {report:?}");
        }
        Ok(report)
    })?;
    Ok(json!({
        "method": "load_children_from_dir over an isolated exact watch-null-sink package; read-only control reads manifest and component bytes without parsing or hashing",
        "manifest": fixture_metadata(&manifest)?,
        "component": fixture_metadata(&component)?,
        "load_children_from_dir": sample_record(20, load_samples),
        "file_read_only_control": sample_record(20, control_samples),
        "interpretation": "load-minus-read bounds manifest parse, digest/hash, validation, and object construction together; it is not claimed as exact hashing CPU attribution",
    }))
}

fn observation(index: usize, prefix: &str) -> Result<MctObservation> {
    let timestamp = Timestamp::new("2026-08-15T00:00:00Z")?;
    Ok(MctObservation {
        observation_id: ObservationId::new(format!("obs-perf-{prefix}-{index}"))?,
        observed_at: timestamp,
        kind: ObservationKind::NodeHealthReported,
        source_plane: SourcePlane::Operator,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace-perf-{prefix}-{index}"))?,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some("local-mct".into()),
        resource_id: Some("perf-phase-0".into()),
        policy_revision: None,
        grants_revision: None,
        outcome: ObservationOutcome::Informational,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: "perf phase 0 ledger growth fact".into(),
        detail_ref: None,
    })
}

fn append_sync_cost(temp: &TempDir) -> Result<Value> {
    let ledger_path = temp.path().join("append-sync.jsonl");
    let mut ledger = JsonlObservationLedger::open(&ledger_path, "ledger-perf", "local-mct")?;
    let samples = run_samples(20, 200, |index| {
        let appended_at = now_string();
        let value = observation(index, "append")?;
        Ok(ledger.append_before_effect(value, appended_at)?)
    })?;
    let entries = ledger.entries()?.len();
    let bytes = fs::metadata(&ledger_path)?.len();
    Ok(json!({
        "method": "one JsonlObservationLedger::append_before_effect per sample; append_frame includes write_all and sync_data on the target temp volume",
        "target_volume_path": temp.path(),
        "result": sample_record(20, samples),
        "ledger_entries_after": entries,
        "ledger_bytes_after": bytes,
    }))
}

fn idempotency_cost(temp: &TempDir) -> Result<Value> {
    let state_path = temp.path().join("idempotency.sqlite");
    drop(MctRuntimeStateStore::open(&state_path)?);
    let open_samples = run_samples(20, 200, |_| MctRuntimeStateStore::open(&state_path))?;
    let now = Timestamp::new("2026-08-15T00:00:00Z")?;
    let expires_at = Timestamp::new("2099-01-01T00:00:00Z")?;
    let combined_samples = run_samples(20, 200, |index| {
        let store = MctRuntimeStateStore::open(&state_path)?;
        let fingerprint = MctIdempotencyFingerprint {
            target: "patina:watch/events@0.1.0.emit".into(),
            call_id: CallId::new(format!("call-perf-idempotency-{index}"))?,
            payload_digest: format!("blake3:{:064x}", index + 1),
        };
        let reservation = store.reserve_call_idempotency(
            "perf-phase-0-caller",
            &format!("perf-phase-0-key-{index}"),
            &fingerprint,
            &now,
            &expires_at,
            256,
        )?;
        if reservation != MctIdempotencyReservation::ExecuteFresh {
            bail!("fresh idempotency reservation was not ExecuteFresh: {reservation:?}");
        }
        Ok((store, reservation))
    })?;
    Ok(json!({
        "method": "one already-migrated SQLite file; open-only and open plus one unique Immediate-transaction ExecuteFresh reservation measured separately",
        "state_path": state_path,
        "open_only": sample_record(20, open_samples),
        "open_plus_reservation": sample_record(20, combined_samples),
    }))
}

struct SnapshotFixture {
    _temp: TempDir,
    ledger_path: PathBuf,
    config_path: PathBuf,
    children_dir: PathBuf,
    state_path: PathBuf,
    ledger: JsonlObservationLedger,
}

impl SnapshotFixture {
    fn new() -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let ledger_path = temp.path().join("observations.jsonl");
        let config_path = temp.path().join("config.json");
        let children_dir = temp.path().join("children");
        let state_path = temp.path().join("state.sqlite");
        fs::create_dir(&children_dir)?;
        MctDaemonConfigStore::new(&config_path).ensure_local_identity(
            MctOperatorNodeScope::default(),
            temp.path().join("identity.hex"),
        )?;
        let ledger =
            JsonlObservationLedger::open_authority(&ledger_path, "ledger-local", "local-mct")?;
        Ok(Self {
            _temp: temp,
            ledger_path,
            config_path,
            children_dir,
            state_path,
            ledger,
        })
    }

    fn grow_to(&mut self, target: usize) -> Result<(usize, u64, Duration)> {
        let started = Instant::now();
        let mut current = self.ledger.entries()?.len();
        while current < target {
            self.ledger.append_before_effect(
                observation(current, "snapshot-growth")?,
                "2026-08-15T00:00:00Z",
            )?;
            current += 1;
        }
        let elapsed = started.elapsed();
        let entries = self.ledger.entries()?;
        MctRuntimeStateStore::open(&self.state_path)?.publish_authority_projection(&entries)?;
        Ok((
            entries.len(),
            fs::metadata(&self.ledger_path)?.len(),
            elapsed,
        ))
    }

    fn measure(&self, warmups: usize, samples: usize) -> Result<Vec<u64>> {
        run_samples(warmups, samples, |_| {
            local_execution_authority_snapshot(
                &self.ledger_path,
                &self.config_path,
                &self.children_dir,
                &self.state_path,
            )
            .map_err(|error| anyhow!("local authority snapshot denied: {error:?}"))
        })
    }
}

fn snapshot_costs() -> Result<Value> {
    let mut fixture = SnapshotFixture::new()?;
    let cases = [
        (1_000usize, 3usize, 30usize),
        (10_000, 2, 20),
        (100_000, 1, 10),
    ];
    let mut values = Vec::new();
    for (target, warmups, sample_count) in cases {
        let (actual, ledger_bytes, setup_elapsed) = fixture.grow_to(target)?;
        let samples = fixture.measure(warmups, sample_count)?;
        values.push(json!({
            "target_entries": target,
            "actual_entries": actual,
            "ledger_bytes": ledger_bytes,
            "real_writer_growth_seconds": setup_elapsed.as_secs_f64(),
            "result": sample_record(warmups, samples),
        }));
    }
    Ok(json!({
        "method": "one valid JsonlObservationLedger authority tenure grown by the real sync_data writer to each target, published through MctRuntimeStateStore, then local_execution_authority_snapshot measured",
        "cases": values,
    }))
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let host = host_preflight()?;
    if !args.fixtures.is_dir() {
        bail!("fixtures directory missing: {}", args.fixtures.display());
    }
    if let Some(parent) = args.output.parent()
        && !parent.is_dir()
    {
        bail!("output parent does not exist: {}", parent.display());
    }
    let temp = tempfile::tempdir()?;
    let started = Instant::now();
    let result = json!({
        "schema": "mct-perf-phase-0-component-costs/v1",
        "caveat": CAVEAT,
        "covered_revision": COVERED_REVISION,
        "captured_at": now_string(),
        "host": host,
        "sample_clock": "std::time::Instant",
        "target_temp_volume": temp.path(),
        "mct_wasm_component_runtime_new": runtime_engine_cost()?,
        "component_from_file": component_costs(&args.fixtures)?,
        "load_children_from_dir": child_load_cost(&args.fixtures, &temp)?,
        "append_including_sync_data": append_sync_cost(&temp)?,
        "state_open_and_idempotency": idempotency_cost(&temp)?,
        "local_execution_authority_snapshot": snapshot_costs()?,
        "total_bench_seconds": started.elapsed().as_secs_f64(),
    });
    let bytes = serde_json::to_vec_pretty(&result)?;
    if bytes.len() > MAX_JSON_BYTES {
        bail!(
            "component-costs.json is {} bytes, above D-P0.10 limit {}",
            bytes.len(),
            MAX_JSON_BYTES
        );
    }
    fs::write(&args.output, [&bytes[..], b"\n"].concat())?;
    println!(
        "wrote {} bytes to {}",
        bytes.len() + 1,
        args.output.display()
    );
    Ok(())
}

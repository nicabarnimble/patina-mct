//! Isolated installation measurements, not a Child authority ingress.
use anyhow::{Context, Result, ensure};
use mct_kernel::*;
use mct_observation::{JsonlObservationLedger, private_fs};
use serde_json::json;
use std::{path::PathBuf, time::Instant};
use wasmtime::{
    Config, Engine, Store,
    component::{Component, Linker, ResourceTable},
};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

struct Host {
    ctx: WasiCtx,
    table: ResourceTable,
}
impl WasiView for Host {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("component") if args.next().is_none() => component(),
        Some("ledger") => {
            let directory = PathBuf::from(args.next().context("missing fresh ledger directory")?);
            ensure!(args.next().is_none(), "unexpected argument");
            ledger(directory)
        }
        _ => anyhow::bail!("expected component | ledger FRESH_DIRECTORY"),
    }
}

fn component() -> Result<()> {
    let bytes = wat::parse_str(
        r#"(component
      (import "wasi:random/random@0.2.3" (instance $random
        (export "get-random-u64" (func (result u64)))))
      (alias export $random "get-random-u64" (func $random))
      (core func $random-lowered (canon lower (func $random)))
      (core module $module
        (import "host" "random" (func $random (result i64)))
        (func (export "run") (result i32)
          call $random drop i32.const 42))
      (core instance $instance (instantiate $module
        (with "host" (instance (export "random" (func $random-lowered))))))
      (func (export "run") (result u32)
        (canon lift (core func $instance "run"))))"#,
    )?;
    let mut config = Config::new();
    config.wasm_component_model(true).epoch_interruption(true);
    let engine = Engine::new(&config)?;
    let start = Instant::now();
    let component = Component::from_binary(&engine, &bytes)?;
    let compile_ms = start.elapsed().as_secs_f64() * 1000.0;
    // Whole helper-process high-water RSS at the compile boundary, not a heap delta.
    let compile_peak_kib = proc_kib("VmHWM:");
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    let mut store = Store::new(
        &engine,
        Host {
            ctx: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
        },
    );
    store.set_epoch_deadline(1);
    let instance = linker.instantiate(&mut store, &component)?;
    let run = instance.get_typed_func::<(), (u32,)>(&mut store, "run")?;
    let (value,) = run.call(&mut store, ())?;
    ensure!(value == 42, "component result mismatch");
    println!(
        "{}",
        json!({"wasi_probe":true, "compile_ms":compile_ms,
        "compile_peak_kib":compile_peak_kib, "wasm_bytes":bytes.len()})
    );
    Ok(())
}

fn proc_kib(field: &str) -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|line| line.starts_with(field))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn ledger(directory: PathBuf) -> Result<()> {
    ensure!(!directory.try_exists()?, "probe requires a fresh directory");
    private_fs::ensure_directory(&directory)?;
    let path = directory.join("probe.jsonl");
    let mut writer =
        JsonlObservationLedger::open(&path, "ledger-install-probe", "mother-install-probe")?;
    let mut samples = Vec::new();
    for i in 0..40 {
        let observation = MctObservation {
            observation_id: ObservationId::new(format!("probe-{i}"))?,
            observed_at: Timestamp::new("2026-10-31T00:00:00Z")?,
            kind: ObservationKind::RuntimeExecutionCompleted,
            source_plane: SourcePlane::Kernel,
            trace: ObservationTraceRef {
                trace_id: TraceId::new("trace-install-probe")?,
                span_id: None,
                parent_span_id: None,
                external_trace_id: None,
            },
            call_id: None,
            decision_id: None,
            subject_id: None,
            resource_id: None,
            policy_revision: None,
            grants_revision: None,
            outcome: ObservationOutcome::Completed,
            visibility: ObservationVisibility::InternalOnly,
            safe_message: "isolated durability probe".into(),
            detail_ref: None,
        };
        let start = Instant::now();
        writer.append_before_effect(observation, "2026-10-31T00:00:00Z")?;
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    drop(writer);
    let reader = JsonlObservationLedger::open_read_only(
        &path,
        "ledger-install-probe",
        "mother-install-probe",
    )?;
    ensure!(
        reader.entries()?.len() == samples.len(),
        "ledger replay lost samples"
    );
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    println!(
        "{}",
        json!({"samples_ms":samples, "p50_ms":sorted[19],
        "p95_ms":sorted[37], "max_ms":sorted[39], "count":40,
        "method":"single-frame append_before_effect including sync_data; verified reopen"})
    );
    Ok(())
}

//! Measure streaming ledger open time and process memory.
//!
//! ```text
//! cargo run -p mct-observation --release --bin ledger-bench -- --sizes 10000,100000
//! cargo run -p mct-observation --release --bin ledger-bench -- --sizes 1000000
//! ```
//!
//! `--check` enforces ceilings for 10k and 100k. One million entries stays
//! report-only so CI can keep the smaller sizes.

use mct_observation::{resume_ledger_replay, verify_ledger_streaming, write_benchmark_chain};
use std::{
    env, fs,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

struct Sample {
    entries: u64,
    bytes: u64,
    open_ms: u128,
    resume_ms: u128,
    rss_before_kb: u64,
    rss_after_kb: u64,
    hwm_kb: u64,
}

fn main() {
    let mut sizes = vec![10_000_u64, 100_000];
    let mut check = false;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--sizes" => {
                let raw = args.next().unwrap_or_else(|| usage());
                sizes = raw
                    .split(',')
                    .map(|part| {
                        part.trim()
                            .parse::<u64>()
                            .unwrap_or_else(|_| panic!("size `{part}` is not an integer"))
                    })
                    .collect();
            }
            "--check" => check = true,
            "--help" | "-h" => usage(),
            other => panic!("unknown argument `{other}`"),
        }
    }
    if sizes.is_empty() {
        usage();
    }

    let mut failures = Vec::new();
    for entries in sizes {
        let sample = measure(entries);
        println!(
            "entries={entries} bytes={} open_ms={} resume_ms={} rss_before_kb={} rss_after_kb={} hwm_kb={}",
            sample.bytes,
            sample.open_ms,
            sample.resume_ms,
            sample.rss_before_kb,
            sample.rss_after_kb,
            sample.hwm_kb
        );
        if check && let Some(reason) = ceiling_failure(&sample) {
            failures.push(reason);
        }
    }
    if !failures.is_empty() {
        for reason in &failures {
            eprintln!("ledger benchmark ceiling exceeded: {reason}");
        }
        std::process::exit(1);
    }
}

fn measure(entries: u64) -> Sample {
    let dir = env::temp_dir().join(format!(
        "mct-ledger-bench-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&dir).expect("create benchmark directory");
    let path = dir.join("observations.jsonl");
    let bytes = write_benchmark_chain(&path, "ledger-bench", "mother-bench", entries)
        .expect("write benchmark chain");
    let (rss_before_kb, _) = proc_vm();
    let started = Instant::now();
    let verified =
        verify_ledger_streaming(&path, "ledger-bench", "mother-bench").expect("streaming verify");
    let open_ms = started.elapsed().as_millis();
    let checkpoint = verified
        .checkpoint
        .expect("benchmark chain has a checkpoint");
    let started = Instant::now();
    let resumed = resume_ledger_replay(&path, "ledger-bench", "mother-bench", &checkpoint)
        .expect("resume checkpoint");
    let resume_ms = started.elapsed().as_millis();
    match resumed {
        mct_observation::CheckpointResume::Resumed(resumed) => {
            assert_eq!(resumed.head, verified.head);
        }
        mct_observation::CheckpointResume::Mismatch { detail } => {
            panic!("fresh checkpoint mismatched: {detail}");
        }
    }
    let (rss_after_kb, hwm_kb) = proc_vm();
    let _ = fs::remove_dir_all(&dir);
    Sample {
        entries,
        bytes,
        open_ms,
        resume_ms,
        rss_before_kb,
        rss_after_kb,
        hwm_kb,
    }
}

fn ceiling_failure(sample: &Sample) -> Option<String> {
    let (max_open_ms, max_hwm_kb) = match sample.entries {
        10_000 => (5_000_u128, 64 * 1024),
        100_000 => (20_000_u128, 96 * 1024),
        _ => return None,
    };
    if sample.open_ms > max_open_ms {
        return Some(format!(
            "{} entries opened in {} ms (ceiling {max_open_ms} ms)",
            sample.entries, sample.open_ms
        ));
    }
    if sample.hwm_kb > max_hwm_kb {
        return Some(format!(
            "{} entries peaked at {} kB RSS (ceiling {max_hwm_kb} kB)",
            sample.entries, sample.hwm_kb
        ));
    }
    None
}

fn proc_vm() -> (u64, u64) {
    #[cfg(target_os = "linux")]
    {
        let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
        let mut rss = 0;
        let mut hwm = 0;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                rss = parse_kb(rest);
            } else if let Some(rest) = line.strip_prefix("VmHWM:") {
                hwm = parse_kb(rest);
            }
        }
        (rss, hwm)
    }
    #[cfg(not(target_os = "linux"))]
    {
        (0, 0)
    }
}

#[cfg(target_os = "linux")]
fn parse_kb(rest: &str) -> u64 {
    rest.split_whitespace()
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn usage() -> ! {
    eprintln!(
        "usage: ledger-bench [--sizes 10000,100000] [--check]\n\
         1000000 is opt-in and is not ceiling-checked"
    );
    std::process::exit(2);
}

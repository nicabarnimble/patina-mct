# MCT Performance Phase 0 harness

This directory contains measurement tooling only. It does not alter resident behavior.

## Official call-path capture

Run from a clean tree on an AC-powered `aarch64-apple-darwin` host:

```bash
python3 scripts/perf/run.py \
  --official \
  --load-note "no intentional concurrent workload; normal logged-in desktop services" \
  --output /absolute/path/outside/the/repository/mct-perf-phase-0
```

Official mode has no matrix knobs. It:

- verifies the source-derived Watch fixtures with `scripts/build-watch-fixtures.sh`;
- builds the resident and digest helper with Cargo's locked release profile;
- creates fresh owner-private roots under a unique short `/tmp/mctp0.*` run root;
- initializes each isolated identity offline, then directly launches `mct-daemon serve` with explicit identity, config, Child, state, ledger, and unique UDS paths;
- never invokes launchd, a production supervisor, `release-local.sh`, or `release-baselines.sh`;
- stages, approves, and grants the exact `watch-null-sink@0.1.0` fixture separately for each scenario;
- captures the ratified 5-startup, idle-RSS, 100-warmup + 10,000-sequential, and 4 × 500 concurrent matrix; and
- writes machine JSON, rendered Markdown, raw ledgers/client intervals/logs, and raw-file byte-size/BLAKE3 receipts.

The complete output remains outside git. Under D-P0.10, later profile commits copy only `call-path.json`, `attribution.json`, `component-costs.json`, `host.json`, rendered Markdown, and the PROFILE report. Raw `observations.jsonl`, `client-calls.jsonl`, and logs stay in the output directory. `host.json` binds them by relative path, exact byte size, and BLAKE3 digest.

The same one-command surface invokes `attribution.py` after clean resident shutdown. It derives stage timings and durability-class accounting from raw ledger frames and client intervals; no production instrumentation is used.

## Non-official harness check

`--smoke` uses a deliberately tiny matrix and labels all output non-official. Hidden skip flags are accepted only with `--smoke`; they exist for local code-path checks and can never produce official evidence.

```bash
python3 scripts/perf/run.py --smoke --output /tmp/mct-perf-smoke
python3 scripts/perf/run.py --self-test
```

## Cold-cache component bench

The later component bench attempts `/usr/sbin/purge` directly and never invokes `sudo`. An operator who has an environment where that command succeeds reruns:

```bash
cargo bench -p mct-daemon --bench perf_phase_0 -- \
  --fixtures crates/mct-daemon/tests/fixtures \
  --output /absolute/path/component-costs.json
```

If purge is unavailable, the bench emits typed `cold_unavailable` records with the exact failure; warm evidence remains valid under D-P0.9.

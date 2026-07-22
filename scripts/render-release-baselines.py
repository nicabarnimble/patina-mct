#!/usr/bin/env python3
"""Render the committed no-threshold baseline record from complete measurements."""

import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--baseline-json", type=Path, required=True)
parser.add_argument("--fixture-json", type=Path, required=True)
parser.add_argument("--context-json", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
baseline = json.loads(args.baseline_json.read_text())
fixture = json.loads(args.fixture_json.read_text())
context = json.loads(args.context_json.read_text())


def values(items: list[object]) -> str:
    return ", ".join(f"{value:.3f}" if isinstance(value, float) else str(value) for value in items)


text = f"""# MCT 0.2.0 performance baselines — aarch64-apple-darwin

These values are release evidence, not SLOs or admission thresholds.

## Artifact and host

- Source revision: `{context['source_revision']}`
- Archive SHA-256: `sha256:{context['archive_sha256']}`
- Archive BLAKE3: `blake3:{context['archive_blake3']}`
- Executable BLAKE3: `{context['executable_blake3']}`
- Rust: `{context['rust_version']}`
- Cargo: `{context['cargo_version']}`
- macOS: {context['os_version']}; architecture: `arm64`
- Hardware model: `{context['hardware_model']}`; logical CPUs: {context['logical_cpus']}; memory bytes: {context['memory_bytes']}
- Power configuration: `{context['power_mode']}`

## Startup

Method: five real fixed-label launchd `start` requests through the internal D1.18 plist seam, each awaiting owner-authenticated readiness, followed by clean `stop`.

- Samples (ms): {values(baseline['startup_ms'])}
- Min/median/max (ms): {baseline['startup_min_ms']:.3f} / {baseline['startup_median_ms']:.3f} / {baseline['startup_max_ms']:.3f}

## Idle RSS

Method: 60 seconds ready and idle, then seven RSS samples ten seconds apart from the launchd-supervised PID.

- Samples (bytes): {values(baseline['idle_rss_bytes'])}
- Min/median/max (bytes): {baseline['idle_rss_min_bytes']} / {baseline['idle_rss_median_bytes']} / {baseline['idle_rss_max_bytes']}

## Owner-authenticated UDS calls

Payload: exact approved `watch-null-sink@0.1.0` `patina:watch/events@0.1.0.emit` call with public inline legacy file-change data.

- Sequential: {baseline['uds_latency_warmups']} warmups; {baseline['uds_latency_samples']} measured; {baseline['uds_latency_successes']} successes
- p50/p95/p99/max (µs): {baseline['uds_latency_p50_us']:.3f} / {baseline['uds_latency_p95_us']:.3f} / {baseline['uds_latency_p99_us']:.3f} / {baseline['uds_latency_max_us']:.3f}
- Throughput: {baseline['throughput_clients']} clients × {baseline['throughput_calls_per_client']} calls in {baseline['throughput_seconds']:.3f}s = {baseline['throughput_calls_per_second']:.3f} calls/s; failures={baseline['throughput_failures']}
- Throughput resident CPU seconds: {baseline['throughput_cpu_seconds']:.3f}; peak RSS bytes: {baseline['throughput_peak_rss_bytes']}

## Trigger-turn load

Method: one production scheduler recovery range of 4,097 occurrences under `fire_late_bounded`, yielding 31 admitted candidates plus one terminal record representing every excess refusal.

- Turn wall/CPU: {baseline['trigger_turn_ms']:.3f} ms / {baseline['trigger_turn_cpu_seconds']:.3f} s
- Admitted candidates: {baseline['trigger_turn_admitted']}; terminally represented refusals: {baseline['trigger_turn_terminal_refusals']}
- Concurrent ordinary owner-authenticated status latency: {baseline['trigger_turn_status_ms']:.3f} ms
- Ledger growth during turn: {baseline['trigger_turn_ledger_bytes_after']} bytes

## Complete three-fixture resources

Method: `/usr/bin/time -l python3 scripts/release-smoke-proof.py ...` over copied Slate, folder-watch actor, and null-sink fixtures, including exact approval/grants, Watch call-out, temporal trigger, revocation, and clean restart.

- Wall/user/system seconds: {context['fixture_wall']} / {context['fixture_user']} / {context['fixture_sys']}
- Peak RSS bytes: {context['fixture_peak_rss']}
- Catalog bytes delta: {context['catalog_delta']}
- State bytes delta: {context['state_delta']}
- Ledger bytes delta: {context['ledger_delta']}
- Terminal outcomes: fixture acquisitions={fixture['fixture_acquisitions']}; Watch deliveries={fixture['watch_event_deliveries']}; revocation survived restart={str(fixture['revocation_survived_restart']).lower()}

## Reproduction

```text
scripts/release-local.sh baselines --artifact {context['artifact']} --output {context['output']}
```

The harness refuses an occupied fixed MCT launchd label, uses no network acquisition, snapshots and post-compares production supervisor files byte-for-byte, and exposes no alternate plist or label selector in the distributed CLI.
"""
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(text)

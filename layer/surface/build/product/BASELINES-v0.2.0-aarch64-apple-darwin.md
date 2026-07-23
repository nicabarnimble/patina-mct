# MCT 0.2.0 performance baselines — aarch64-apple-darwin

These values are release evidence, not SLOs or admission thresholds.

## Artifact and host

- Source revision: `08a508f76e6cfc3e2b739303c9ea0f22527f4d25`
- Archive SHA-256: `sha256:12b318de74b1da54e4598fd04aac2b6b920b9659a52e0e8fbf2e8b9659e1885e`
- Archive BLAKE3: `blake3:3cd6f02da4e015eaea4115d73ca7a108362413003d5d495c522aaf5eda0909a3`
- Executable BLAKE3: `blake3:a2953fe5c2aa859164977f090c95cf5aee762d7a223ce554ad1948653fdd7929`
- Rust: `rustc 1.96.0 (ac68faa20 2026-05-25);binary: rustc;commit-hash: ac68faa20c58cbccd01ee7208bf3b6e93a7d7f96;commit-date: 2026-05-25;host: aarch64-apple-darwin;release: 1.96.0;LLVM version: 22.1.2`
- Cargo: `cargo 1.96.0 (30a34c682 2026-05-25)`
- macOS: 26.5.2; architecture: `arm64`
- Hardware model: `Mac14,13`; logical CPUs: 12; memory bytes: 103079215104
- Power configuration: `AC Power: Sleep On Power Button 1 autorestartatconnect 0 lowpowermode 0 standby 0 ttyskeepawake 1 powernap 1 displaysleep 90 womp 1 networkoversleep 0 sleep 1 tcpkeepalive 1 autorestart 1 disksleep 10`

## Startup

Method: five real fixed-label launchd `start` requests through the internal D1.18 plist seam, each awaiting owner-authenticated readiness, followed by clean `stop`.

- Samples (ms): 1043.783, 433.834, 329.520, 340.958, 465.260
- Min/median/max (ms): 329.520 / 433.834 / 1043.783

## Idle RSS

Method: 60 seconds ready and idle, then seven RSS samples ten seconds apart from the launchd-supervised PID.

- Samples (bytes): 26443776, 26443776, 26476544, 26476544, 26509312, 26509312, 26509312
- Min/median/max (bytes): 26443776 / 26476544 / 26509312

## Owner-authenticated UDS calls

Payload: exact approved `watch-null-sink@0.1.0` `patina:watch/events@0.1.0.emit` call with public inline legacy file-change data.

- Sequential: 100 warmups; 1000 measured; 1000 successes
- p50/p95/p99/max (µs): 485478.083 / 650263.167 / 672942.125 / 788367.125
- Throughput: 4 clients × 500 calls in 2056.512s = 0.973 calls/s; failures=0
- Throughput resident CPU seconds: 2199.270; peak RSS bytes: 572768256

## Trigger-turn load

Method: one production scheduler recovery range of 4,097 occurrences under `fire_late_bounded`, yielding 31 admitted candidates plus one terminal record representing every excess refusal.

- Turn wall/CPU: 3118.659 ms / 3.550 s
- Admitted candidates: 31; terminally represented refusals: 4066
- Concurrent ordinary owner-authenticated status latency: 1341.622 ms
- Ledger growth during turn: 216375 bytes

## Complete three-fixture resources

Method: `/usr/bin/time -l python3 scripts/release-smoke-proof.py ...` over copied Slate, folder-watch actor, and null-sink fixtures, including exact approval/grants, Watch call-out, temporal trigger, revocation, and clean restart.

- Wall/user/system seconds: 13.07 / 0.31 / 0.29
- Peak RSS bytes: 60030976
- Catalog bytes delta: 1806336
- State bytes delta: 208896
- Ledger bytes delta: 347533
- Terminal outcomes: fixture acquisitions=3; Watch deliveries=2; revocation survived restart=true

## Reproduction

```text
scripts/release-local.sh baselines --artifact target/release-artifacts/mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz --output layer/surface/build/product/BASELINES-v0.2.0-aarch64-apple-darwin.md
```

The harness refuses an occupied fixed MCT launchd label, uses no network acquisition, snapshots and post-compares production supervisor files byte-for-byte, and exposes no alternate plist or label selector in the distributed CLI.

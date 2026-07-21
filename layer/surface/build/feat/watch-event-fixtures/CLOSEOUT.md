# Watch event fixtures Part B close-out

Status: complete — Replacement Slice 4B

## Landed range

- Required predecessor boundary: `bf4e922`, the completed Replacement Slice 4A landing.
- Exact fixture receipts: `4df52cc`.
- Kernel Watch authority and schema-v9 evidence model: `62d4c1e`.
- Resident Watch/supporting-grant controls, schema-v10 keyvalue, bounded host adapters, deterministic delivery runtime, and composed checkpoint: `21da7c3`.
- Truthful buffered-proof checkpoint: `37f2cc2`.
- D1B.7-A synchronous send admission: `acfcaa5`.
- Reissued immutable Part A close-out: `8a727d0`.
- D1B.7-A.1 batch durability barrier: `38bc359`.
- Restart-proof writer-release hardening: `fac727b`.
- Track 3 attribution, product documentation, final composed proof name, this close-out, and SPEC completion: the Part B landing commit containing this file.

The unrelated untracked older session and belief artifacts were not included.

## Failing-test-first record

The first Part B resident Watch control compile was red before its production types were corrected:

```text
error[E0425]: cannot find type `MctDaemonConfig` in this scope
   --> crates/mct-daemon/src/daemon/watch.rs:133:5

error[E0609]: no field `assignment_id` on type `&MctStoredChildAssignment`
   --> crates/mct-daemon/src/daemon/watch.rs:153:35

error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 2 previous errors
```

The correction imported the actual config type and derived the canonical assignment identity as `assignment:{child_name}` instead of inventing a persisted field.

D1B.7-A subsequently exposed a real architecture conflict rather than hiding it: the exact watcher sends one event at a time, while deterministic batch positions require the complete scan. The operator accepted invocation-local synchronous admission plus post-export deterministic delivery. The new red/green boundary is named by `watch_send_admission_refuses_paths_shape_and_capacity_synchronously` and `watch_admission_append_failure_suppresses_every_nested_delivery`.

## Flake and deterministic-failure log

Two full-workspace runs exposed bounded writer-release races in pre-existing Part A restart test helpers:

```text
observation ledger is already locked by another writer
resident_temporal_trigger_fires_once_and_recovers_without_duplication
injected_temporal_trigger_fires_once_and_recovers_without_duplication
```

Neither failure was rerun unchanged and called green. `fac727b` replaced immediate reopen assumptions with bounded retry only for the known canonical-writer lock condition; every other open error remains terminal. Both targeted tests and the following full workspace run passed. No product authority or runtime retry behavior changed.

## D1B.7-A / D1B.7-A.1 closure

- `producer.send` synchronously evaluates exact Watch authorization and validates topic, content type, named byte/metadata bounds, strict event JSON shape, event class, safe relative path, current scope capacity, resident capacity, and exact 0.1.x legacy path equality.
- Refusal is a stable typed WIT error and the event does not join the admitted set.
- An admitted offset is invocation-local only; it is not a durable or delivered acknowledgement.
- After export return, the resident normalizes the complete admitted set and appends batch, event, and pre-call disposition observations through the single canonical writer.
- `watch_admission_append_failure_suppresses_every_nested_delivery` proves failed append creates no event plan, run, nested call, or delivery.
- The composed test orders `Watch batch opened` → `Watch event eligible` → `Child call-out constructed` before the first nested `RuntimeExecutionStarted`.

## Required composed proof — disk reconstruction

All line citations refer to `crates/mct-daemon/src/daemon/supervisor_lifecycle.rs` in the Part B landing tree. The primary proof is `supervised_trigger_watch_delivery_fixtures_execute_end_to_end`, beginning at line 3323.

| Step | Proof |
|---:|---|
| 1 | Creates isolated supervised paths and installs/starts a digest-bound resident through the existing lifecycle adapter. |
| 2 | Acquires real Slate, source-derived watcher, and exact unmodified sink packages, then asserts immutable artifact/provenance records for all three. |
| 3 | Exactly approves and assigns each artifact and proves the watcher/sink WIT exports are loaded from those acquired packages. |
| 4 | Grants Watch, directory-read, keyvalue, logging, and measure independently to the watcher; grants only its own logging/measure effects to the sink. |
| 5 | Configures the exact watcher stream as `patina:watch/events@0.1.0.emit`, creates a real file, and invokes `scan-now` through resident call law. |
| 6 | Proves one deterministic batch/event/delivery and orders durable batch/event/disposition evidence before nested sink execution (lines 3960–3984). |
| 7 | Creates a temporal trigger targeting watcher `scan-now`, observes the second delivery, and preserves truthful trigger-parent versus `WasmHost` sink lineage (around lines 4013–4043). |
| 8 | Revokes trigger and Watch scope, creates another file, and proves denial before another Watch observation or delivery. |
| 9 | Stops and reopens state/ledger, asserting three acquisitions, eleven independent Toy grants, and exactly two persisted deliveries (line 4170). |
| 10 | Restarts the supervised resident and proves revoked authority still denies with no duplicate trigger firing or sink delivery (lines 4214–4222). |

The exact watcher component also executes independently with read-only preopen, bounded durable keyvalue, logging, measure, and messaging adapters. The exact sink executes independently without Watch or filesystem authority.

## Track 3 and closed boundaries

`layer/surface/build/spec-drift-audit/track3/LEDGER.md` names every invariant under:

- `MctEventSourcePlacement`;
- `MctWatchObservationScope`;
- `MctLegacyWatchEventsCompatibility`;
- `MctWatchEventDelivery`; and
- `PatinaWatcherQuarryDisposition`.

There are no implicit waivers. `MotherEventSourceAdapterRuntime`, `RegistrySyncTriggerComposition`, and `NetworkArtifactAcquisitionAdapter` remain exact deferrals with no executable path. Allium and `crates/mct-kernel/src/observation.rs` remain unchanged from `20941a4`.

## Final validation

- `allium check layer/allium` — passed without diagnostics/findings.
- `allium analyse layer/allium` — passed without diagnostics/findings.
- `cargo test --workspace` — passed: 394 tests (121 daemon library, 132 daemon binary, 2 Wasm-limit integration, 36 Iroh, 92 kernel, 11 observation), plus doc tests.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `./scripts/ci-tier0.sh` — passed.
- `bash scripts/check-comparative-vocabulary.sh` — passed.
- `git diff --check` — passed.
- `git diff 20941a4 -- layer/allium` — empty.
- `git diff 20941a4 -- crates/mct-kernel/src/observation.rs` — empty.

## Boundary statement

Replacement Slice 4 now proves acquisition-backed Slate plus durable temporal triggering plus scoped Watch observation plus ordinary exact-sink delivery under a supervised resident. This closes the three-fixture runtime slice. It does not claim full `patinaMother` replacement while `mct-release-hardening` and `mct-interface-launcher-control` remain paused, and it does not implement Mother-side event observation, registry-sync composition, or network artifact acquisition.

---
type: feat
id: perf-phase-0
status: active
created: 2026-08-14
target: mct-perf-phase-0-covered-revision
sessions:
  origin: 20260810-102025-217186000
  work:
    - 20260810-102025-217186000
related:
  - layer/core/reliability-doctrine.md
  - layer/allium/mct-product-map.allium
  - Cargo.lock
  - layer/surface/build/product/BASELINES-v0.2.0-aarch64-apple-darwin.md
  - scripts/perf/run.py
  - scripts/perf/attribution.py
  - crates/mct-daemon/benches/perf_phase_0.rs
  - crates/mct-daemon/Cargo.toml
  - crates/mct-daemon/src/daemon/resident/local_ingress.rs
  - crates/mct-daemon/src/daemon/resident/pipeline.rs
  - crates/mct-daemon/src/daemon/resident/decision.rs
  - crates/mct-daemon/src/daemon/resident/execution.rs
  - crates/mct-daemon/src/daemon/resident/observation.rs
  - crates/mct-daemon/src/daemon/resident/idempotency.rs
  - crates/mct-daemon/src/authority_snapshot.rs
  - crates/mct-daemon/src/children.rs
  - crates/mct-daemon/src/wasm.rs
  - crates/mct-observation/src/lib.rs
beliefs:
  - authority-freshness-requires-mother-state
  - mother-kernel-decides-adapters-perform
  - typed-domain-records-before-algorithms
exit_criteria:
  - id: measurement-only-guard
    text: The phase changes no production code or behavior; the only permitted paths under crates are the G1-ratified perf_phase_0 bench target declaration and bench source, with no production source or dependency change.
    checked: false
    verify: git diff ead8796d5143d0f9da623057dadc5c920c47bf2b..HEAD -- crates/ and git diff --exit-code ead8796d5143d0f9da623057dadc5c920c47bf2b..HEAD -- crates/mct-daemon/src crates/mct-observation/src crates/mct-kernel/src crates/mct-iroh/src
  - id: reproducible-harness
    text: One Python 3.11-or-newer command builds the release Cargo profile, provisions fresh isolated service roots, directly launches unsupervised residents, runs the exact ratified matrix, invokes attribution, and emits complete machine JSON plus rendered Markdown while refusing non-aarch64-apple-darwin hosts, existing sockets, production paths, and output overwrite.
    checked: false
    verify: python3 scripts/perf/run.py --official --load-note "<honest concurrent load>" --output <new-output-directory>
  - id: stage-attribution
    text: The measured run contains a per-stage p50/p95 table derived only from call-correlated ledger observation timestamps and client totals, documents every stage-to-observation and stage-to-code mapping, and reports the explicit unattributed remainder and every unavailable or invalid timestamp boundary.
    checked: false
    verify: python3 scripts/perf/attribution.py --run <output-directory>/call-path.json --ledger <output-directory>/observations.jsonl --clients <output-directory>/client-calls.jsonl --json <output-directory>/attribution.json --markdown <output-directory>/attribution.md
  - id: component-costs
    text: Public-API-only micro-bench evidence reports p50/p95 and full sample metadata for H1 Engine construction, warm-cache null-sink and Slate component loading plus either verified-purge cold evidence or a typed cold_unavailable result for each fixture, H2 snapshot construction, H3 Child loading and hashing, H4 one sync_data ledger append, and H5 SQLite open plus one idempotency reservation cycle.
    checked: false
    verify: cargo bench -p mct-daemon --bench perf_phase_0 -- --fixtures crates/mct-daemon/tests/fixtures --output <output-directory>/component-costs.json; inspect each fixture's cold status and, when cold_unavailable, its exact failure plus the close-out attribution-gaps entry
  - id: scaling-curve
    text: The report gives p50/p95 latency for each consecutive 500-call window across exactly 10000 sequential measured calls, with ledger entry count and byte size at every boundary, and gives snapshot-construction p50/p95 at approximately 1000, 10000, and 100000 real-writer entries.
    checked: false
    verify: Inspect call-path.json scaling.windows and component-costs.json local_execution_authority_snapshot, then compare the rendered profile scaling tables.
  - id: ranked-candidates
    text: Every ranked candidate states only its measured p50 share or measured throughput/RSS effect, cites constraining and enabling law by exact Contract.Invariant plus the relevant reliability-doctrine section, and names the gated slice it feeds.
    checked: false
    verify: Inspect the profile report Ranked optimization candidates table; every row must contain Measurement, Product-map law, Doctrine, and Fed slice cells.
  - id: durability-class-accounting
    text: The measured call set reports per-call ledger entry count and exact framed byte volume by each DurabilityClass, with p50/p95/max and totals, as evidence for—not a decision about—the later durability-classes specification.
    checked: false
    verify: Inspect attribution.json durability_classes and the profile report Durability-class accounting table.
  - id: workspace-validation
    text: Every phase commit passes workspace tests, warnings-denied all-target Clippy, and Tier 0; final close-out also passes Allium, this spec check, and diff checking under the recorded flake protocol.
    checked: false
    verify: cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh && allium check layer/allium && patina spec check perf-phase-0 --json && git diff --check
  - id: map-waiver
    text: Close-out records the map tend-or-waiver disposition; the expected waiver says this phase adds measurement evidence over existing contracts, no invariant or authority surface, and therefore no LEDGER row.
    checked: false
    verify: Inspect the SPEC Phase 0 close-out evidence Map-tend waiver subsection and confirm git diff ead8796d5143d0f9da623057dadc5c920c47bf2b..HEAD -- layer/allium layer/surface/build/spec-drift-audit/track3/LEDGER.md is empty.
---

# feat: MCT Perf Phase 0 — profile the covered revision

> Measure the covered production call path without changing it, then rank later optimization slices beneath the existing reliability net.

## Claim boundary

This is an evidence-only phase governed by **MCT Reliability Doctrine — Optimize Under the Net**. Its covered baseline is `ead8796d5143d0f9da623057dadc5c920c47bf2b` on branch `patina`. It measures the release-profile code built from that revision and this phase's measurement-only commits; it does not optimize, reorder, cache, batch, instrument, or otherwise change production behavior.

The resulting call-path numbers are **dev-launched, directly supervised by the harness, unsupervised by launchd, release-Cargo-profile, and not release-artifact measurements**. They are not directly comparable to `BASELINES-v0.2.0-aarch64-apple-darwin.md`. Every JSON and Markdown output, including the final profile report, carries that caveat.

The requested reading path named `layer/surface/build/feat/post-r3-credibility/SPEC.md`, but that path does not exist at the covered revision. Its historical evidence is in `layer/surface/build/feat/post-r3-credibility/CLOSEOUT.md`; this SPEC follows that document's evidence-only map-waiver wording and the current frontmatter/close-out convention in `grants-authority-v0/SPEC.md`.

## Gate G1 ratification decisions

### D-P0.1 — pre-G1 RUSTSEC-2026-0253 prerequisite repair

The original Step 0 revision was `c3ad3de724fa4c728e78e1ecaedefe994db8338e`. Before the Task 1 SPEC commit, Tier 0 discovered transitive `lru 0.18.0` was denied by the existing audit policy under `RUSTSEC-2026-0253`. The operator ratified the Phase G B0 precedent: run exactly `cargo update -p lru`, require a lockfile-only single-package patch bump to `lru 0.18.2`, weaken no audit policy, validate the complete per-commit suite, and commit it as `chore(deps): patch lru to 0.18.2 (RUSTSEC-2026-0253)`.

That prerequisite landed as `ead8796d5143d0f9da623057dadc5c920c47bf2b` after tests, warnings-denied all-target Clippy, and Tier 0 all passed, including a clean `cargo audit`. It is now the Step 0 expected HEAD, measurement-only diff origin, profiled revision, and `<12-hex-rev>` source (`ead8796d5143`) for the PROFILE filename. The repair changed only `Cargo.lock` (`lru 0.18.0` → `0.18.2`) and does not authorize general dependency triage or work on the two separate Scorecard advisories.

### D-P0.2 — measurement-only fence

No production source under `crates/*/src/` changes. No runtime dependency is added. The only permitted `crates/` changes after G1 are:

1. `crates/mct-daemon/benches/perf_phase_0.rs`, a standalone public-API-only measurement executable; and
2. one `[[bench]]` declaration named `perf_phase_0` with `harness = false` in `crates/mct-daemon/Cargo.toml`.

No new dev-dependency is requested. The bench may use only existing package dependencies and the existing `tempfile` dev-dependency. It is not run by `cargo test --workspace`; warnings-denied all-target Clippy still compiles it. If any accurate measurement requires a production seam, dependency, feature, or visibility change, the phase stops and presents that fork.

### D-P0.3 — harness and evidence locations

The harness is Python **3.11 or newer**, under `scripts/perf/`, in the style of `scripts/release-baselines.py`. The final one-command surface is:

```bash
python3 scripts/perf/run.py \
  --official \
  --load-note "<honest concurrent load>" \
  --output <new-output-directory>
```

`run.py` builds `mct-daemon` with `cargo build --release --locked`, provisions fresh private temp service roots, copies the committed canonical `watch-null-sink@0.1.0` bytes and stages them through the same digest-verified UDS acquisition/approval/supporting-grant flow as `release-baselines.py`, launches `target/release/mct-daemon serve` directly, awaits owner-authenticated readiness, measures the matrix, shuts the child process down, and invokes `scripts/perf/attribution.py`. It does not invoke `scripts/build-watch-fixtures.sh`. Call-matrix evidence records the committed source manifest and component receipts plus the acquisition request's expected BLAKE3 digest, returned observed BLAKE3 digest, canonical artifact SHA-256 identity, and staged package manifest/sidecar receipts; all must agree with the committed fixture bytes and product-generated canonical metadata. The component bench likewise reads only the committed canonical `watch-null-sink@0.1.0` and `slate-manager@0.2.0` fixture bytes and records their receipts.

The output directory must not exist. The durable bundle contains at least:

- `call-path.json` and `call-path.md`;
- `client-calls.jsonl`, one client interval and outcome per measured call;
- `observations.jsonl`, copied byte-for-byte after clean resident shutdown;
- `attribution.json` and `attribution.md`;
- `host.json`, resident stdout/stderr, harness log, and failure metadata; and
- separately, `component-costs.json` from the ratified Rust bench.

JSON carries raw samples as well as summaries. Markdown is rendered from JSON, never maintained as an independent number source. Failed runs preserve their isolated temp evidence and emit no successful result marker. The run directory is the complete on-disk evidence bundle; D-P0.10 below defines the deliberately smaller committed evidence set and required raw-file digests.

### D-P0.4 — isolation and host refusal

The harness accepts no service-root option. It creates owner-private roots under a fresh `tempfile` directory and passes every identity, config, children, state, ledger, and UDS path explicitly to the direct resident. Before launch it requires that its unique socket path does not exist; existence is a hard refusal, not stale-socket cleanup.

The harness resolves and refuses any path equal to or beneath `~/.mct` or `~/Library/LaunchAgents`, any path containing the production `io.patina.mct.mother` label, and any production supervisor path. It never invokes launchctl, `scripts/release-local.sh`, `scripts/release-baselines.sh`, a production supervisor command, or an archival/session-close command. It never starts, stops, probes, or locks the operator's daily-driver Mother.

Execution refuses unless the host is Darwin arm64 and Rust's host is `aarch64-apple-darwin`. `--official` additionally requires AC power and a nonblank `--load-note`. The host record includes source revision, dirty-state declaration, hardware model and chip/CPU, logical CPU count, memory bytes, macOS version, architecture, Cargo and rustc versions, complete `pmset -g custom` output, load averages, a process-load snapshot, and the operator load note. Every output records that it is a dev-launched unsupervised release-profile binary, not a release artifact.

Nothing under `scripts/perf/` is wired into Tier 0, Tier 1, or a workflow. Linux CI does not run the Darwin-only harness or micro-benches.

### D-P0.5 — exact call-path matrix

All successful call samples use the exact approved `watch-null-sink@0.1.0` `patina:watch/events@0.1.0.emit` inline public file-change payload shape from `scripts/release-baselines.py`, unique protocol/call/trace/payload references, owner-authenticated UDS `/calls`, and the current proof-gated receiver identity obtained from owner-authenticated readiness/status data. Calls require `outcome == completed`; every other HTTP/protocol result is retained as a failure.

Scenarios use separate fresh service roots beneath one run directory so startup/idle, sequential scaling, and concurrent throughput cannot contaminate one another's ledger-length or RSS evidence. Every scenario repeats the same acquisition/approval/grant setup.

| Scenario | Exact matrix | Summary |
|---|---|---|
| Startup | 5 fresh direct-resident launches, each timed process-spawn to owner-authenticated ready and then cleanly stopped | min / median / max milliseconds, all samples |
| Idle RSS | 1 fresh ready resident, 60-second settle, then 7 samples 10 seconds apart | min / median / max bytes, all samples |
| Sequential and scaling | 100 warmups excluded, then exactly 10,000 measured calls without intervening workload | headline p50/p95/p99/max over calls 1–1000; p50/p95 for each consecutive 500-call window over calls 1–10000; ledger entry count and byte size immediately after each window |
| Throughput | 1 fresh ready resident, 4 clients × 500 calls, simultaneous start | aggregate calls/s, elapsed seconds, failures, resident CPU seconds, peak RSS bytes, per-client counts |

Resident CPU is sampled immediately before and after throughput from the direct child PID. Peak RSS is monitored for that PID at 20 ms or faster. Client latency uses `time.monotonic_ns`; ledger stages use only persisted observation timestamps. Exact percentile selection and units are recorded in JSON.

### D-P0.6 — ledger-derived attribution, not instrumentation

`attribution.py` parses each raw JSONL frame, retains its exact framed byte length (including newline), and joins only calls listed in `client-calls.jsonl`. It does not infer a boundary from append order when no truthful timestamp exists.

The successful-call mapping is documented against these anchors:

| Boundary | Existing evidence | Code anchor and interpretation |
|---|---|---|
| submission received / constructed | `CallReceived` and `CallConstructed`, safe messages `authenticated local call received` and `local call accepted for evaluation` | `resident/local_ingress.rs` `local_submission_observations`; both enter the first acknowledged append batch |
| receiver/deadline/payload/idempotency through route | first call-correlated route/candidate observation through final `RouteRevalidated`/`RouteSelected` correlation | `pipeline.rs` `execute_resident_call_at_with_context` and `execute_resident_call_after_payload`; successful deadline and fresh idempotency reservation emit no dedicated observation, so this remains one combined pre-route interval |
| route durable to before-effect facts | last route/revalidation observation to first required Toy/Watch authority observation | route batch at `pipeline.rs`; before-effect adapter batch at `execution.rs` around the append preceding WIT invocation |
| WASM start | `obs-resident-wasm-wit-started:<call_id>` / safe message `wasm component execution started` | constructed in `wasm.rs` immediately before invocation-path `Component::from_file` |
| WASM completion proxy | `obs-executed-on:<call_id>` / safe message `runtime execution observed` | constructed in `execution.rs` after the runtime returns; this is the truthful post-invocation boundary |
| terminal durable | local `ResultRecorded`, safe message `local call result recorded` | `local_ingress.rs` terminal append immediately before the UDS response |

The nominal `obs-resident-wasm-wit-completed` timestamp is **not** used as a completion clock: `execution.rs` currently creates both invocation ID timestamps before calling into the runtime. The report records this and the absent successful deadline/idempotency boundary as attribution gaps. The pre-WASM interval cannot split effect-time snapshot construction, Child reload/hashing, SQLite open, Engine construction, import discovery compilation, adapter construction, and invoke-path compilation. Micro-benches cover those suspects without a production seam. If operator review deems one of these unavailable splits essential rather than a reportable gap, work stops for an instrumentation-seam fork.

For each stage, the tool reports sample eligibility, p50/p95, exclusions, and clock/ordering anomalies. `unattributed_remainder_us = max(client_total_us - sum(nonoverlapping_attributed_intervals_us), 0)` is reported per call and as p50/p95; negative raw remainder is retained as a clock anomaly rather than silently clamped away.

For durability accounting, each call-correlated ledger entry contributes one append/frame and its exact source-line bytes to its serialized `durability_class` (`before_effect`, `buffered`, or `projection_only`). Results include per-call p50/p95/max and totals by class. Batch-command count is not guessed from JSONL; because the current writer fsyncs each entry, the report distinguishes persisted entry/frame count from unobservable actor-message batch count.

### D-P0.7 — public-API component micro-benches

The standalone bench emits raw nanosecond samples and p50/p95/max summaries. It uses `std::hint::black_box`, one process, the target temp volume, and public APIs only. Setup is excluded unless explicitly named. Each case records fixture digest/bytes, iteration count, warmups, generated ledger count/bytes, cache preparation result, and failures.

| Suspect | Case and methodology |
|---|---|
| H1 Engine | `MctWasmComponentRuntime::new(MctWasmHostConfig::default_local())`; 20 warmups, 200 measured independent constructions |
| H1 component compile | direct public Wasmtime `Component::from_file` for null-sink and Slate; warm cache: pre-read fixture plus 10 discarded compiles then 100 measured compiles; cold cache: 10 independent samples, each only after `/usr/sbin/purge` succeeds and a fresh matching Engine is constructed outside the timed interval |
| H2 snapshot | `local_execution_authority_snapshot` against captured valid ledgers grown by `JsonlObservationLedger` and projected through `MctRuntimeStateStore`; target entry counts are nearest valid counts at or above 1k/10k/100k; warmups/measures are 3/30, 2/20, and 1/10 respectively |
| H3 Child load/hash | `load_children_from_dir` over the captured harness children directory; 20 warmups, 200 measured; report artifact/manifest bytes and, if public composition permits, an additional file-read-only control so hashing share is stated as a bounded difference rather than claimed exact CPU attribution |
| H4 append+sync | one `JsonlObservationLedger::append_before_effect` per sample on the target temp volume, including `sync_data`; 20 warmups then 200 measured unique valid observations on one real writer |
| H5 SQLite | `MctRuntimeStateStore::open` alone and open plus one unique `reserve_call_idempotency` `ExecuteFresh` cycle; 20 warmups then 200 measured on one migrated state file; report open-only and combined numbers separately |

The bench never invokes `sudo`. Cold-cache samples are valid only when `/usr/sbin/purge` exits successfully; otherwise it emits a typed `cold_unavailable` result naming the exact command failure rather than relabeling a warm first read as cold. The bench documents the same invocation for an operator-controlled environment where purge succeeds.

### D-P0.8 — law-bound candidate ranking

The report may rank only measured candidate families and may not propose merging the two authority evaluations. Every row cites exact law and **Reliability Doctrine — Optimize Under the Net**; physical-write candidates additionally cite **Durability Classes Are Specification Decisions** and **Standing Obligations**.

| Candidate family | Mandatory product-map law | Fed gated slice |
|---|---|---|
| incremental authority cursor/head proof; no merged evaluations | `TwoPhaseRouting.ExecutionRevalidatesAuthority`, `TwoPhaseRouting.EffectBoundaryRevisionGuardIsDistinct`, `MctAuthorityProjectionFreshness.AuthorityCursorReachesCanonicalHead`, `.CursorBindsHeadHashAndAuthorityIdentity`, `.RebuildEqualsReplay`, `.UnprovableFreshnessDenies` | ledger segmentation / incremental authority projection |
| Engine/component cache and resident Child generations keyed by immutable digest | `MctImmutabilityModel.MutationBoundariesAreNamed`, `MctChildComponentLifecycle.ArtifactIsImmutableValue`, `.InstanceIsLiveGeneration`, `.ReplacementLoadsBeforeSwap`, `.CallsRequireReadyAuthorizedInstance`, and `MctToyGrantAuthority.GrantSnapshotsAreCacheNotTruth` | WASM Engine/component caching + resident Child generations |
| single-writer group commit and crash proof | `MctLedgerCommitAndRecovery.OneWriterDefinesLocalOrder`, `.BeforeEffectRequiresAcknowledgedCommit`, `MctLocalFirstObservationLedger.AuthorityFactsAreDurableBeforeEffect`, `.BufferedEffectsAreBounded`, and `MctLocalApplicationIngress.LocalAcknowledgementRequiresDurableFacts` | group commit + crash matrix |
| classify required and bufferable call facts | the same durability invariants plus the 2026-07-14 W1 B7 decision | durability-classes specification, then implementation |
| remove non-durability cross-caller serialization if measured | `MctLocalApplicationIngress.CallsRemainOutsideMutationSequencer` and the 2026-07-14 W1 B10 decision | defect slice scoped by measured lock/CPU/RSS evidence |

SQLite-open and Child-reload candidates may be ranked separately only if micro-benches establish meaningful cost. H6 may state correlation between concurrent peak RSS and per-call Engine/component work, but it must not claim causation without process-memory attribution.

### D-P0.9 — cold-cache availability cannot deadlock close-out

The `component-costs` criterion is satisfied when warm-cache evidence lands for both null-sink and Slate and each fixture's cold-cache result is either (a) measured only after verified `/usr/sbin/purge` success or (b) typed `cold_unavailable` with the exact command failure. Every `cold_unavailable` result is listed in the close-out attribution gaps. The bench supports and documents an operator invocation in an environment where purge succeeds, but the phase does not wait on that environment. The agent never invokes `sudo`.

### D-P0.10 — raw evidence stays out of git; digests go in

The run output directory retains the complete evidence bundle on disk. Git receives only `call-path.json`, `attribution.json`, `component-costs.json`, `host.json`, rendered Markdown files, and `PROFILE-ead8796d5143-aarch64-apple-darwin.md`. Raw `observations.jsonl`, `client-calls.jsonl`, resident stdout/stderr, harness logs, and other raw logs remain uncommitted in that output directory.

Committed `host.json` records each retained raw file's relative path, exact byte size, and BLAKE3 digest after clean shutdown and before rendering. Operator verification is a harness rerun plus digest comparison against those on-disk raw files, not a committed raw ledger. Before staging, every committable JSON is checked independently; if any exceeds 5,000,000 bytes, work stops and reports the oversized artifact rather than committing it.

### D-P0.11 — fixture staging follows the canonical copy-and-digest methodology

The instruction to run `scripts/build-watch-fixtures.sh` was an instruction error. That script is a provenance rebuild verifier, not the baseline measurement staging path. Because the Watch upstream archive has no committed lockfile and the committed provenance explicitly binds historical output bytes rather than promising future dependency-index reproducibility, a current-toolchain rebuild is not an admissible precondition for measuring the canonical fixture.

The harness must not invoke `scripts/build-watch-fixtures.sh`. Following `scripts/release-baselines.py`, each scenario copies the committed `watch-null-sink@0.1.0` source manifest and component into its fresh private source directory, computes the component's expected BLAKE3 digest from those bytes, and supplies that digest to UDS artifact acquisition before exact-artifact approval and the supporting grant. Run evidence must prove that the expected and acquisition-observed BLAKE3 digests match, that the acquired canonical artifact SHA-256 identity matches the committed component receipt, and that the product-generated canonical package manifest and digest sidecar match that identity. The micro-bench reads the committed `watch-null-sink@0.1.0` and `slate-manager@0.2.0` files directly and records their manifest/component receipts. This is the same copy-and-digest artifact floor used by the `0.2.0` baselines and three-fixture replacement proof, not a verification waiver.

The failed provenance rebuild remains evidence rather than becoming work in this measurement-only phase. The close-out records that `scripts/build-watch-fixtures.sh` is not byte-reproducible with Rust/Cargo 1.96.0 and the current unpinned upstream dependency index, cites the preserved failed evidence bundle, and creates a release-discipline board follow-up for an upstream lockfile plus pinned toolchain in MCT-REBUILD provenance. This phase neither repairs rebuild reproducibility nor refreshes committed fixture bytes.

## Implementation tasks after Gate G1

1. **Call-path harness** — land `scripts/perf/` and the isolated direct-resident matrix.
2. **Ledger attribution** — land raw-frame attribution and durability-class accounting.
3. **Component micro-benches** — land only the two ratified bench scaffolding files and public-API cases.
4. **Covered-revision profile** — run on AC power; retain raw evidence only in the output directory; commit only the D-P0.10 allowlist with raw-file sizes/BLAKE3 digests and no JSON over 5 MB; and write `PROFILE-ead8796d5143-aarch64-apple-darwin.md`.
5. **Phase K-style close-out** — reconstruct evidence from disk into this SPEC and prepare, but do not archive, the active session.

Each task uses the commit subject named in the phase instruction. Ratified decisions above are not reopened absent a genuine stop-condition fork.

## Validation and flake protocol

Every commit runs each command separately while teeing complete stdout/stderr to scratch files:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

A failed test is rerun in isolation. If it passes isolated, the verbatim first-run output is retained in the phase flake log with isolated-rerun count; a reproducing failure is fixed before advancement. The known trigger-authority writer-lock collision and supervisor watch-delivery timing failure receive no automatic exemption.

Final close-out additionally runs:

```bash
allium check layer/allium
patina spec check perf-phase-0 --json
git diff --check
git diff ead8796d5143d0f9da623057dadc5c920c47bf2b..HEAD -- crates/
```

## Map tend-or-waiver at G1

**Proposed waiver:** no `mct-product-map.allium` tend and no Track 3 LEDGER attribution row. This phase produces measurement evidence over already-landed routing, authority-freshness, Child lifecycle, immutability, local-ingress, and durability contracts. It adds no structural obligation, entity, surface, authority source, durability choice, or behavior. This mirrors the post-R3 credibility evidence-only waiver. If implementation or findings would change an invariant, authority semantic, observation law, or durability behavior, the phase stops instead of tending the map inside a profiling slice.

## Gate G1

The operator ratified the committed plan at `495e3526879433e2c8c158e479f70b27ca4c27d3`, including D-P0.1 through D-P0.8, then ratified amendments D-P0.9 and D-P0.10 above. After the correctly stopped official run exposed the fixture-rebuild instruction error, the operator ratified D-P0.11. Harness fixture staging resumes only after the amendment commit `spec(perf): record D-P0.11 fixture staging disposition`. Ratified decisions are not reopened absent a genuine stop-condition fork.

## Phase 0 close-out evidence

### Attribution gaps and findings (open)

- **Fixture provenance rebuild reproducibility:** the first official Task 5 attempt correctly stopped before resident launch because `scripts/build-watch-fixtures.sh` rebuilt `folder-watch-actor.wasm` to bytes that differed from the committed fixture. The rebuild ran with Rust/Cargo 1.96.0 against an unpinned upstream dependency index, whereas the committed MCT-REBUILD provenance records Rust/Cargo 1.94.0 and explicitly disclaims future dependency-index byte reproducibility. The complete failed bundle is retained outside git at `/Users/nicabar/Projects/Patina/patina-mct-perf-phase0-ead8796-20260815`; its `failure.json` and `raw/fixture-verification.log` preserve the exact failure. This does not invalidate canonical copy-and-digest measurement under D-P0.11 and is not repaired here.
- **Release-discipline board follow-up:** require an upstream lockfile and pinned Rust/toolchain declaration for future MCT-REBUILD provenance verification. This is a later release-discipline item, not a Phase 0 optimization or fixture refresh.

The remaining close-out evidence will be reconstructed from disk after Tasks 2–5. It will contain the exact commit range, one evidence-table row per frontmatter criterion with central numbers, validation and verbatim flake log per commit, final map waiver, measurement-only diff, attribution gaps carried into the next slice's SPEC, and checked exit-criterion flags. Session archival remains operator-run.

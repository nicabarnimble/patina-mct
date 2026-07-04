# Resident Mother Phase 4 tasks

- [x] Task R0 — Housekeeping
- [x] Task R1 — SPEC first
- [x] Task R2 — Concurrent peer serving in mct-iroh
- [x] Task R3 — The resident `mct-daemon serve`
- [x] Task R4 — Resident call execution
- [x] Task R5 — Operational surface

---

# MCT Phase 4 — Resident Mother daemon

You are starting product Phase 4 in `patina-mct`: ROADMAP item 1. Today
`mct-daemon serve` serves control-plane snapshots only, and Iroh peer
serving is a separate foreground CLI command handling one connection at a
time with bindings passed as CLI args. Phase 4 produces ONE resident
process — bind endpoint, serve peers concurrently, expose the control
plane, supervise state and ledger, shut down cleanly. Scope discipline:
NO payload data plane, NO routing engine, NO binding signatures (ROADMAP
items 2–4). Incoming calls dispatch directly to the locally configured
child, exactly as `iroh serve-process` does today — just resident and
concurrent.

## Working principles (binding)

1. Read `AGENTS.md`, `layer/core/dependable-rust.md`,
   `layer/core/what-is-mct.md`, and `layer/surface/build/product/ROADMAP.md`
   before touching code. Non-negotiable: kernel decides, adapters perform
   (kernel stays pure — time and facts are inputs); fail closed; typed
   decisions; sealed capabilities are minted only by kernel evaluators and
   the stale-revision guards at effect boundaries must remain.
2. Favor strong invariants over defensive fallbacks. Do not add complexity
   to paper over unclear design. Prefer simple data models, explicit
   contracts, and shared logic over local patches, duplicated code, or
   speculative abstractions. Write Rust that Jon Gjengset would agree with.
3. Always read code before writing code. Before designing the serve loop,
   read: `crates/mct-iroh/src/serve.rs` (current single-connection model),
   `crates/mct-daemon/src/main.rs` `run_serve`/`serve_iroh_process`
   (current composition and child dispatch), `crates/mct-daemon/src/config.rs`
   (peer address book, identity), `crates/mct-observation/src/lib.rs`
   (writer exclusivity), `crates/mct-daemon/src/state.rs` (store threading).
4. Scalpel commits; stage files by name; failing test first where a task
   fixes observable behavior. Stop at a task boundary if context runs low.
5. Validation green before starting and after every commit:
   `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh`
6. Flake protocol: capture any test failure verbatim into TASKS.md under
   "## Flake log" BEFORE rerunning.

## Architecture constraints (decide within these, not around them)

- **Ledger is single-writer.** `JsonlObservationLedger` appends take
  `&mut self` and fsync. Concurrent connection tasks must funnel
  observations through one owner — a dedicated writer task fed by an mpsc
  channel is the expected shape (a `Mutex` + `spawn_blocking` is
  acceptable only if it is genuinely simpler; never block the async
  executor on fsync). Authority-critical observations must still be
  durable before their effect proceeds — if a task needs
  observed-before-effect, it awaits the writer's ack.
- **SQLite store is `!Sync`.** Follow the existing Phase-2 pattern
  (`spawn_blocking` snapshot source in main.rs) — extend it, don't invent
  a second pattern.
- **Per-connection authority state.** The single-slot
  `MctIrohServeState.last_hello` model cannot survive concurrency. Hello
  admission state becomes per-connection (or per-peer keyed by endpoint
  id — read how the call evaluation matches `hello_decision_id` and pick
  the simplest correct scope). A second peer must not evict the first's
  admission.
- **Kernel purity holds.** Each connection task supplies its own `now` at
  evaluation time; no clock enters mct-kernel.

## Task R0 — Housekeeping

a) Verify a clean tree on `patina` (except `brew-noncore-report.html` and
   `layer/surface/build/product/ROADMAP.md`, which is new and expected).
   Commit ROADMAP.md as `docs: add product roadmap`.
b) Save this prompt verbatim as
   `layer/surface/build/feat/resident-mother/TASKS.md` with a checklist
   header; commit before any code work. Check tasks off there in the same
   commit as each task's final change.

## Task R1 — SPEC first

Per `layer/core/spec-driven-design.md`, write
`layer/surface/build/feat/resident-mother/SPEC.md` (short — one screen):
the serve composition (what runs in the one process), the config surface
(identity path, bindings source = persisted peer address book NOT CLI
args, control transport, children dir), concurrency model chosen for the
ledger/store/connections, shutdown semantics (SIGINT/SIGTERM → stop
accepting, drain in-flight connections with a bounded deadline, close
endpoint, final observation), and explicit non-goals (data plane, routing,
signatures). Commit it; it is the contract the remaining tasks implement.

## Task R2 — Concurrent peer serving in mct-iroh

Restructure the serve path so accepting and connection-handling are
separate: an accept loop that spawns one task per connection, per-
connection hello/call state, a configurable max-concurrent-connections
bound (fail closed: refuse accept beyond the bound), and the existing
per-connection timeout preserved. Public API may change (0.x). Tests:
two peers complete hello concurrently without evicting each other; the
concurrency bound refuses the N+1th connection; existing protocol tests
stay green.

## Task R3 — The resident `mct-daemon serve`

Rebuild `run_serve` to compose, in one process: identity load (0o600 file
per existing helper), endpoint bind, peer bindings loaded from the
persisted config store (address book) and refreshed on a modest interval
or on connection (pick one, state it in SPEC), the R2 accept loop, the
existing HTTP/UDS control plane, the ledger writer (per the architecture
constraint), and the state store. Graceful shutdown per SPEC. The old
`iroh serve`/`serve-process` commands may delegate to the new composition
or be removed — no duplicated serve logic may remain. Tests: daemon-level
integration test that starts a resident Mother, completes hello+call from
a second endpoint, hits the control plane, and shuts down cleanly.

## Task R4 — Resident call execution

Incoming admitted `mct/call/0` requests dispatch to the locally
configured, approved child (process AND wasm-WIT paths — generalize what
`serve_iroh_process`'s call handler does today): mint capabilities through
the kernel evaluators per call, enforce the existing deadline/memory
limits, funnel all observations to the shared ledger writer, return
caller-safe replies. Denials follow the dual-reason pattern (precise
reason to ledger, "not authorized" to peer). Test: end-to-end two-Mother
test where a remote call executes a real child through the resident
daemon and the full trace is reconstructible from the ledger afterward.

## Task R5 — Operational surface

`daemon_status`/control snapshots report the live composition: endpoint
id + lifecycle, accepted-connection count, loaded/approved children,
binding count, ledger sequence tip. Logs to stderr, meaningful exit codes,
UDS socket file removed on shutdown. Test: status reflects a live serving
daemon and a shut-down one.

## Definition of done (every task)

- Validation green; no deny path weakened; sealed-capability and
  observed-before-effect invariants intact; kernel stays pure.
- Concurrency additions are tokio-idiomatic: no executor-blocking fsync/
  SQLite, no lock held across await on a hot path.
- Each commit states WHAT invariant or capability it adds; TASKS.md
  checked off as you go.
- Final summary: commits, SPEC decisions made, flake log contents (or
  none), validation results, and what a reviewer should look at first.

## Flake log

### 2026-07-04 — R2 compile failure during concurrent serve refactor

Command:

```bash
cargo test -p mct-iroh --lib
```

Failure output:

```text
   Compiling mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
error[E0599]: no method named `lock` found for struct `tokio::sync::MutexGuard<'_, serve::MctIrohServeState>` in the current scope
   --> crates/mct-iroh/src/serve.rs:415:51
    |
415 | ...                   let mut state = state.lock().await;
    |                                             ^^^^ private field, not a method
...
error[E0282]: type annotations needed
   --> crates/mct-iroh/src/serve.rs:415:33
...
error[E0382]: borrow of moved value: `remote_endpoint_id`
   --> crates/mct-iroh/src/serve.rs:595:42
...
error[E0382]: borrow of moved value: `remote_endpoint_id`
   --> crates/mct-iroh/src/serve.rs:635:45
...
warning: variable does not need to be mutable
   --> crates/mct-iroh/src/lib.rs:482:13
...
error: could not compile `mct-iroh` (lib test) due to 4 previous errors; 1 warning emitted
```

Assessment: deterministic compile errors from shadowing the shared serve-state handle with a mutex guard and moving endpoint IDs before storing/looking them up; not an intermittent flake.

### 2026-07-04 — R2 concurrent hello test assertion failure

Command:

```bash
cargo test -p mct-iroh --lib
```

Failure output:

```text
running 17 tests
...
test tests::concurrent_serve_keeps_peer_hello_state_separate ... FAILED
...
---- tests::concurrent_serve_keeps_peer_hello_state_separate stdout ----

thread 'tests::concurrent_serve_keeps_peer_hello_state_separate' (3306037) panicked at crates/mct-iroh/src/lib.rs:423:9:
assertion `left == right` failed
  left: Denied
 right: Admitted
...
test result: FAILED. 16 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.78s
error: test failed, to rerun pass `-p mct-iroh --lib`
```

Assessment: deterministic test fixture mismatch while adding the second peer; the failing path is an authority denial, not an intermittent runtime flake.

### 2026-07-04 — R2 clippy large enum variant

Command:

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
```

Failure output:

```text
    Checking mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
error: large size difference between variants
   --> crates/mct-iroh/src/serve.rs:159:1
    |
159 | / pub enum MctIrohServeEvent {
160 | |     AcceptedConnection,
    | |     ------------------ the second-largest variant carries no data at all
161 | |     Served(MctIrohServedProtocol),
    | |     ----------------------------- the largest variant contains at least 1104 bytes
162 | |     RefusedConnection,
163 | | }
    | |_^ the entire enum is at least 1104 bytes
    |
    = note: `-D clippy::large-enum-variant` implied by `-D warnings`
help: consider boxing the large fields or introducing indirection in some other way to reduce the total size of the enum
    |
161 -     Served(MctIrohServedProtocol),
161 +     Served(Box<MctIrohServedProtocol>),
    |

error: could not compile `mct-iroh` (lib) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `mct-iroh` (lib test) due to 1 previous error
```

Assessment: deterministic clippy issue from adding an event enum with a large served-protocol payload; fixed by boxing the event payload rather than allowing the lint.

### 2026-07-04 — R3 main binary compile failure during resident composition

Command:

```bash
cargo test -p mct-daemon
```

Failure output:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0433]: failed to resolve: could not find `signal` in `tokio`
    --> crates/mct-daemon/src/main.rs:1216:36
     |
1216 |         let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
     |                                    ^^^^^^ could not find `signal` in `tokio`
...
error[E0433]: failed to resolve: could not find `signal` in `tokio`
    --> crates/mct-daemon/src/main.rs:1218:64
     |
1218 |         let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
     |                                                                ^^^^^^ could not find `signal` in `tokio`
...
error[E0277]: the trait bound `anyhow::Error: std::error::Error` is not satisfied
    --> crates/mct-daemon/src/main.rs:1209:17
     |
1209 |         source: Box::new(source),
     |                 ^^^^^^^^^^^^^^^^ the trait `std::error::Error` is not implemented for `anyhow::Error`
     |
     = note: required for the cast from `Box<anyhow::Error>` to `Box<(dyn std::error::Error + Send + Sync + 'static)>`

Some errors have detailed explanations: E0277, E0433.
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 5 previous errors
warning: build failed, waiting for other jobs to finish...
error: could not compile `mct-daemon` (bin "mct-daemon") due to 5 previous errors
```

Assessment: deterministic compile issue from using Tokio signal APIs without enabling the `signal` feature and from boxing `anyhow::Error` directly for an adapter provider source; fixed by enabling the feature and preserving a typed provider error as `std::io::Error::other` at the public adapter boundary.

### 2026-07-04 — R4 resident execution compile failure

Command:

```bash
cargo test -p mct-iroh --lib && cargo test -p mct-daemon --bin mct-daemon
```

Failure output:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0277]: the trait bound `mct_kernel::AuthorizedChildInvocation: Clone` is not satisfied
    --> crates/mct-daemon/src/main.rs:1397:5
     |
1394 | #[derive(Clone, Debug)]
     |          ----- in this derive macro expansion
...
1397 |     authorized: AuthorizedChildInvocation,
     |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the trait `Clone` is not implemented for `mct_kernel::AuthorizedChildInvocation`

error[E0609]: no field `result_id` on type `&mct_kernel::MctResult`
    --> crates/mct-daemon/src/main.rs:1670:58
     |
1670 |             ResultRef::new(format!("{prefix}:{}", result.result_id))
     |                                                          ^^^^^^^^^ unknown field
     |
     = note: available fields are: `call_id`, `outcome`, `route_taken`, `authority_decision_ref`, `execution_summary` ... and 2 others

error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 2 previous errors
```

Assessment: deterministic compile issue from deriving `Clone` over sealed executable authority and guessing a non-existent result id field; fixed by keeping resident authorization non-clone and deriving reply result refs from the call id.

### 2026-07-04 — R4 resident integration expected execution child

Command:

```bash
cargo fmt --all && cargo test -p mct-daemon --bin mct-daemon
```

Failure output:

```text
running 3 tests
test tests::authorize_cli_toy_denies_expired_grant_against_current_time ... ok
test tests::control_snapshot_unopenable_state_projects_error_response ... ok
test tests::resident_mother_serves_peer_control_and_shutdown ... FAILED

failures:

---- tests::resident_mother_serves_peer_control_and_shutdown stdout ----
mct resident mother endpoint_id=570540d6f066c56ee8f4ed2d6e90d91c02c143c8032b70427d1358764a5ea575
ticket={  "endpoint_id": "570540d6f066c56ee8f4ed2d6e90d91c02c143c8032b70427d1358764a5ea575",  "direct_addresses": [    "10.10.10.182:54712",    "10.10.10.209:54712",    "100.114.124.29:54712"  ],  "relay_urls": []}
mct resident mother children loaded=0 failed=0 bindings=1 max_connections=8
mct daemon serving control uds on /var/folders/6h/329275913d1d3k1lfvvvryp40000gn/T/.tmpWzhaBa/control.sock

thread 'tests::resident_mother_serves_peer_control_and_shutdown' (3370763) panicked at crates/mct-daemon/src/main.rs:3433:9:
assertion `left == right` failed
  left: Denied
 right: Success
...
test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.89s
error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

Assessment: deterministic test fixture drift after resident calls started executing real configured children; fixed by making the integration test provision and approve a real handle child rather than expecting success with no child loaded.

### 2026-07-04 — R4 test assertion used nonexistent observation kind

Command:

```bash
cargo fmt --all && cargo test -p mct-daemon --bin mct-daemon resident_mother_serves_peer_control_and_shutdown -- --nocapture
```

Failure output:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0599]: no variant or associated item named `ChildCallAuthorized` found for enum `mct_kernel::ObservationKind` in the current scope
    --> crates/mct-daemon/src/main.rs:3470:73
     |
3470 |                 .any(|entry| entry.observation.kind == ObservationKind::ChildCallAuthorized),
     |                                                                         ^^^^^^^^^^^^^^^^^^^ variant or associated item not found in `mct_kernel::ObservationKind`
     |
help: there is a variant with a similar name
     |
3470 -                 .any(|entry| entry.observation.kind == ObservationKind::ChildCallAuthorized),
3470 +                 .any(|entry| entry.observation.kind == ObservationKind::CallAuthorized),
     |

error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 1 previous error
```

Assessment: deterministic test assertion error; child authority observations currently project allowed child calls as `RouteRevalidated`, so the trace reconstruction assertion was corrected to the implemented taxonomy.

### 2026-07-04 — R4 test moved ledger path before ledger assertion

Command:

```bash
cargo fmt --all && cargo test -p mct-daemon --bin mct-daemon resident_mother_serves_peer_control_and_shutdown -- --nocapture
```

Failure output:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0382]: borrow of moved value: `ledger_path`
    --> crates/mct-daemon/src/main.rs:3459:52
     |
3353 |         let ledger_path = dir.path().join("observations.jsonl");
     |             ----------- move occurs because `ledger_path` has type `std::path::PathBuf`, which does not implement the `Copy` trait
...
3393 |                 ledger_path,
     |                 ----------- value moved here
...
3459 |             JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
     |                                                    ^^^^^^^^^^^^ value borrowed here after move
     |
help: consider cloning the value if the performance cost is acceptable
     |
3393 |                 ledger_path: ledger_path.clone(),
     |                            +++++++++++++++++++++

error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 1 previous error
```

Assessment: deterministic test ownership issue after adding ledger reconstruction assertions; fixed by cloning the path into the resident config.

### 2026-07-04 — R4 clippy obfuscated if/else in hello-state test helper

Command:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Failure output:

```text
    Checking mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error: this method chain can be written more clearly with `if .. else ..`
   --> crates/mct-iroh/src/serve.rs:194:29
    |
194 |               accepted_alpns: admitted
    |  _____________________________^
195 | |                 .then(|| vec![MCT_CALL_ALPN.into()])
196 | |                 .unwrap_or_default(),
    | |____________________________________^ help: try: `if admitted { vec![MCT_CALL_ALPN.into()] } else { Default::default() }`
    |
    = note: `-D clippy::obfuscated-if-else` implied by `-D warnings`

error: could not compile `mct-iroh` (lib test) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
```

Assessment: deterministic clippy style issue in a test helper; fixed by using an explicit `if` expression.

### 2026-07-04 — R4 clippy resident authorization enum and nested denial write

Command:

```bash
cargo fmt --all && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
```

Failure output:

```text
    Checking mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error: large size difference between variants
    --> crates/mct-daemon/src/main.rs:1402:1
     |
1402 | / enum ResidentAuthorizationOutcome {
1403 | |     Authorized(Box<ResidentAuthorizedExecution>),
     | |     -------------------------------------------- the second-largest variant contains at least 8 bytes
1404 | |     Denied { observation: MctObservation },
     | |     -------------------------------------- the largest variant contains at least 352 bytes
1405 | | }
     | |_^ the entire enum is at least 352 bytes
     |
     = note: `-D clippy::large-enum-variant` implied by `-D warnings`
help: consider boxing the large fields or introducing indirection in some other way to reduce the total size of the enum
     |
1404 -     Denied { observation: MctObservation },
1404 +     Denied { observation: Box<MctObservation> },
     |

error: this `if` statement can be collapsed
    --> crates/mct-daemon/src/main.rs:1427:9
     |
1427 | /         if let ResidentAuthorizationOutcome::Denied { observation } = authorization {
1428 | |             if let Err(error) = ledger.append(vec![observation]).await {
1429 | |                 eprintln!("resident authority denial ledger write failed: {error}");
1430 | |                 return MctIrohCallHandlerResult::failed("observation ledger unavailable");
1431 | |             }
1432 | |         }
     | |_________^
     |
     = note: `-D clippy::collapsible-if` implied by `-D warnings`

error: could not compile `mct-daemon` (bin "mct-daemon") due to 2 previous errors
warning: build failed, waiting for other jobs to finish...
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 2 previous errors
```

Assessment: deterministic clippy issues from an internal enum carrying a large denial observation and a nested denial-write branch; fixed with boxed denial observations and a collapsed condition.

### 2026-07-04 — R5 status compile failure during operational surface wiring

Command:

```bash
cargo fmt --all && cargo test -p mct-daemon --bin mct-daemon -- --nocapture
```

Failure output:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0425]: cannot find type `MctDaemonStatus` in this scope
    --> crates/mct-daemon/src/main.rs:1081:25
     |
1081 |     fn status(&self) -> MctDaemonStatus {
     |                         ^^^^^^^^^^^^^^^ not found in this scope
...
error[E0433]: failed to resolve: use of undeclared type `MctDaemonReadiness`
    --> crates/mct-daemon/src/main.rs:3809:36
     |
3809 |         assert_eq!(live.readiness, MctDaemonReadiness::Ready);
     |                                    ^^^^^^^^^^^^^^^^^^ use of undeclared type `MctDaemonReadiness`
...
error[E0282]: type annotations needed
    --> crates/mct-daemon/src/main.rs:3567:9
     |
3567 | /         tokio::time::timeout(Duration::from_secs(10), resident)
3568 | |             .await
     | |__________________^ cannot infer type

error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 7 previous errors
```

Assessment: deterministic compile issue while adding resident status types to the binary; fixed by importing the newly exported status types and by letting the resident join result bind before nested unwraps.

### 2026-07-04 — R5 live status assertion raced event projection

Command:

```bash
cargo fmt --all && cargo test -p mct-daemon --bin mct-daemon -- --nocapture
```

Failure output:

```text
running 5 tests
...
mct resident mother endpoint_id=2b33c52faf64cc067aa0c55408011ca2faed2fa1ff8ca9e7305c7fe79b034447
ticket={  "endpoint_id": "2b33c52faf64cc067aa0c55408011ca2faed2fa1ff8ca9e7305c7fe79b034447",  "direct_addresses": [    "10.10.10.182:65090",    "10.10.10.209:65090",    "100.114.124.29:65090"  ],  "relay_urls": []}
mct resident mother children loaded=1 failed=0 bindings=1 max_connections=8
mct daemon serving control uds on /var/folders/6h/329275913d1d3k1lfvvvryp40000gn/T/.tmp8VzQwW/control.sock

thread 'tests::resident_mother_serves_peer_control_and_shutdown' (3399008) panicked at crates/mct-daemon/src/main.rs:3561:9:
MctResidentStatus { accepted_connection_count: 0, loaded_child_count: 1, approved_child_count: 1, binding_count: 1, ledger_sequence_tip: 0 }
...
test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.48s
error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

Assessment: deterministic test race against the asynchronous resident event projection that updates operational counters and ledger tip; fixed by polling the control plane until the live status reflects processed peer events.

### 2026-07-04 — R5 clippy unused readiness import outside tests

Command:

```bash
cargo fmt --all && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
```

Failure output:

```text
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error: unused import: `MctDaemonReadiness`
 --> crates/mct-daemon/src/main.rs:6:27
  |
6 |     MctDaemonConfigStore, MctDaemonReadiness, MctDaemonStatus, MctLocalNodeIdentity,
  |                           ^^^^^^^^^^^^^^^^^^
  |
  = note: `-D unused-imports` implied by `-D warnings`

error: could not compile `mct-daemon` (bin "mct-daemon") due to 1 previous error
warning: build failed, waiting for other jobs to finish...
```

Assessment: deterministic target-specific unused import; fixed by removing the binary import and qualifying readiness in test-only assertions.

## Phase complete

Date: 2026-07-04.

Final implementation commit before closeout: `6098e00 feat: report resident mother operational status`.

Summary: Phase 4 turns `mct-daemon serve` into the resident Mother process: it owns the persisted identity, binds and serves the Iroh endpoint concurrently, refreshes peer bindings from persisted config, executes configured and approved process or wasm-WIT children through kernel-minted per-call authority, writes observations through a single acknowledged ledger writer, exposes HTTP/UDS control status, tracks live endpoint/connection/child/binding/ledger counters, and shuts down cleanly with UDS socket cleanup.

Scoped out: payload byte/data-plane transfer, routing-engine consumption of `AuthorizedRouteExecution`, binding signature verification, broader toy catalog work, and multi-Vision capability publication remain ROADMAP items 2–6.

# Resident Mother Phase 4 tasks

- [x] Task R0 — Housekeeping
- [x] Task R1 — SPEC first
- [x] Task R2 — Concurrent peer serving in mct-iroh
- [ ] Task R3 — The resident `mct-daemon serve`
- [ ] Task R4 — Resident call execution
- [ ] Task R5 — Operational surface

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

# Phase 2 progress

- [x] Task P0 — Housekeeping before any code changes
  - [x] P0a committed pre-existing Slate WIP test-only diff as `7a4a661 test: add slate list-work WIT runtime coverage`.
  - [x] P0b persisted this task list in this file.
- [x] Task P1 — Timeouts on the Iroh serve path
- [x] Task P2 — Unique decision/observation IDs in the Iroh adapter
- [x] Task P3 — Control plane: stop swallowing errors, reuse the store
- [ ] Task P4 — Ledger read API: stream instead of slurp
- [ ] Task P5 — Kernel rustdoc and missing_docs
- [ ] Task P6 — Module shape: split mct-iroh endpoint.rs

---

# Verbatim task prompt

# MCT quality hardening — Phase 2 (polish and doctrine alignment)

You are starting Phase 2 of audit remediation in `patina-mct`. Phase 1
(correctness, security, invariants — commits through 760432e) is complete.
These tasks are lower-severity robustness and doctrine-alignment items from
the same audit. Work them in order; each is independently shippable, so stop
cleanly at a task boundary if context runs low and report which tasks landed.

## Working principles (binding)

1. Read `AGENTS.md`, then `layer/core/dependable-rust.md` and
   `layer/core/what-is-mct.md` before touching code. Non-negotiable:
   - **Kernel decides; adapters perform.** `mct-kernel` stays pure.
   - **Fail closed.** Never weaken a deny path.
   - **Typed decisions, not strings.** Preserve `#[source]` chains;
     caller-safe messages are a projection at the disclosure edge.
2. **Favor strong invariants over defensive fallbacks.** Make bad states
   impossible where practical. Do not add complexity to paper over unclear
   design. Prefer simple data models, explicit contracts, and shared logic
   over local patches, duplicated code, or speculative abstractions. Write
   Rust that Jon Gjengset would agree with.
3. **Always read code before writing code.** Before editing any file, read
   it and its callers. Before adding a helper, grep for an existing one.
4. **Commit with a scalpel as you work, not a shotgun after.** Stage
   specific files by name — never `git add -A` / `git add .`.
5. **Do not touch pre-existing working-tree changes that are not yours**,
   EXCEPT as explicitly directed in Task P0. Check `git status` first;
   anything dirty/untracked not named in P0 is off-limits: no commit,
   revert, stash, or reformat.
6. Baseline must be green before starting and after every commit:
   `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh`

## Task P0 — Housekeeping before any code changes

a) **Commit the stranded Slate WIP.** `crates/mct-daemon/src/wasm.rs` has a
   pre-existing uncommitted 92-line diff: a test-only addition (a
   `slate_list_work` WIT component test) belonging to the
   `mct-typed-wit-runtime-parity` spec work. It has been dodged for three
   sessions and later tasks rework adjacent code. Verify the diff is still
   entirely inside `#[cfg(test)] mod tests`, run the full validation suite,
   then commit it by itself with a message noting it is pre-existing spec
   work (e.g. `test: add slate list-work WIT runtime coverage`). If the
   diff contains ANY non-test hunk, stop and report instead of committing.
b) **Persist this task list.** Context walls keep eating the plan. Save
   this entire prompt verbatim as
   `layer/surface/build/audit-remediation/PHASE2.md` and commit it. As you
   complete each task, mark it done in that file in the same commit as the
   task's final change, so a future agent can be told simply "continue from
   PHASE2.md" and know exactly where things stand.
c) All other dirty/untracked files (`AGENTS.md`, `.pi/`, `layer/` session
   files, etc.) remain strictly off-limits.

## Task P1 — Timeouts on the Iroh serve path (robustness)

`serve_next_with_call_handler` in `crates/mct-iroh/src/endpoint.rs` has no
timeout on any network await (`accept_bi`, `read_to_end`, `write_all`,
`connection.closed()`): a stalled peer parks the serve loop forever. The
64KiB bounded read is good; keep it.

- Add an explicit per-connection timeout (named constant, e.g.
  `SERVE_CONNECTION_TIMEOUT`) around the connection-handling section using
  `tokio::time::timeout`. On expiry, return a typed
  `MotherIrohEndpointError` variant (new `ProtocolTimeout { action }` or
  similar) — fail closed, no partial reply.
- The same for the client side (`roundtrip_json`).
- Tests: a peer that connects and never sends data causes `serve_next` to
  return the timeout error rather than hang (use a raw Iroh connection from
  the existing test helpers, and a short injected timeout).

## Task P2 — Unique decision/observation IDs in the Iroh adapter (bug, low)

`MctIrohServeState` generates IDs like `decision-iroh-hello-0` from an
in-memory counter (`crates/mct-iroh/src/endpoint.rs`). Every daemon restart
resets the counter, so IDs recur across runs — but observations are
runtime truth in an append-forever ledger, where recurring decision IDs
corrupt traceability.

- Make generated IDs unique across restarts without adding a heavy
  dependency: include a per-state random or time-derived prefix generated
  once in `MctIrohServeState::new()` (e.g. epoch-nanos of construction plus
  the counter). Document the uniqueness contract on the type.
- Test: two `MctIrohServeState` instances never produce colliding IDs.

## Task P3 — Control plane: stop swallowing errors, reuse the store

`crates/mct-daemon/src/main.rs` `control_snapshot`:
- `state.summary().ok()` and `list_runs(20).unwrap_or_default()` silently
  degrade the snapshot on storage errors. Fail closed instead: a storage
  error should surface as a typed error / not-ready status, not an
  empty-but-healthy-looking snapshot.
- The serve loops reopen the SQLite store on every request. Open it once
  before the loop and reuse it. While there, move the blocking
  SQLite/file-I/O call off the async runtime (`tokio::task::spawn_blocking`
  or equivalent) — the current pattern blocks the executor.
- Tests: a corrupted/unopenable state path yields an error response (not a
  silently empty snapshot).

## Task P4 — Ledger read API: stream instead of slurp

`crates/mct-observation/src/lib.rs`: `entries()`, `by_trace`, `by_call`
load the entire ledger into a `Vec` on every call. The file grows forever.

- Add an iterator-based reading API (e.g. `fn iter_entries(&self) ->
  impl Iterator<Item = Result<MctObservationLedgerEntry>>`) that validates
  the chain incrementally as it streams.
- Reimplement `entries`, `by_trace`, `by_call` on top of it (keep them —
  callers exist). No behavior change to their results.
- Hold the doctrine line: no new public types from std::io in signatures.

## Task P5 — Kernel rustdoc and missing_docs (docs)

The kernel is an authority surface with almost no item-level docs.

- Add `#![warn(missing_docs)]` to `crates/mct-kernel/src/lib.rs` and write
  rustdoc for every public item it flags. Style: one-line summary; document
  invariants and `# Errors` on fallible constructors/functions; document
  the fail-closed semantics on `evaluate_*` functions. No doctests unless
  they demonstrate stable usage (per `dependable-rust.md`).
- Keep it factual and terse — no marketing prose. Clippy/warnings stay
  clean, so every flagged item must be documented, not `#[allow]`ed.

## Task P6 — Module shape: split mct-iroh endpoint.rs (doctrine)

`crates/mct-iroh/src/endpoint.rs` is ~800 lines doing several jobs
(endpoint lifecycle, protocol serving, identity/key management, hex
codecs), violating the black-box module shape `dependable-rust.md`
prescribes for this exact crate.

- Split along the doctrine's lines: `endpoint` (lifecycle + snapshot),
  `serve` (protocol dispatch; the internal framing/timeout detail in an
  `internal.rs`), `identity` (secret key load/create + hex). Public API of
  the crate (`lib.rs` re-exports) must not change — this is an internal
  reshuffle, verified by the compiler and unchanged tests.
- Pure code movement plus visibility adjustments; zero behavior change.
  Commit as one mechanical move per module, not one giant commit.

## Definition of done (every task)

- `cargo test --workspace` green; `cargo clippy --workspace --all-targets
  -- -D warnings` clean; `./scripts/ci-tier0.sh` passes.
- No deny path weakened; no substrate type in kernel public signatures; no
  new defensive fallback where an invariant was available; dirty files
  other than those named in Task P0 untouched.
- Each commit is one coherent step; its message states WHAT invariant or
  property the change enforces.
- Final summary: commits landed, tasks completed/remaining (also reflected
  in PHASE2.md), validation results.

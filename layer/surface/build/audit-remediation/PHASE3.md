# Phase 3 progress

- [x] Task T0 — Housekeeping
- [x] Task T1 — Persistence split: run records store provenance, not capabilities
- [x] Task T2 — Seal `AuthorizedChildInvocation` (single-effect capability)
- [x] Task T3 — Seal `AuthorizedToyCall` (session-scoped capability)
- [ ] Task T4 — Seal `AuthorizedRouteExecution` (single-effect capability)
- [ ] Task T5 — Staleness guard at the effect boundary

---

# Verbatim task prompt

# MCT quality hardening — Phase 3 (unforgeable authorization capabilities)

You are starting Phase 3 in `patina-mct`. Phases 1–2 are complete (see
`layer/surface/build/audit-remediation/PHASE2.md` — every task checked).
Phase 3 closes the last conventionally-enforced segment of the authority
loop: today, successful kernel evaluations produce authorization *records*
(`AuthorizedChildInvocation`, `AuthorizedToyCall`,
`AuthorizedRouteExecution`) that are plain structs with public,
deserializable fields — any code can forge one without consulting the
kernel. Phase 3 makes them unforgeable capabilities that only kernel
evaluators can mint, so `token → effect` is enforced by construction, not
convention. The daemon's existing paths already mint correctly through the
evaluators; this phase removes the possibility of any path doing otherwise.
Net behavior change: zero (one disclosed exception in Task T1).

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
   it and its callers. Before each seal task, read the kernel evaluator
   that produces that record and every daemon site that consumes it.
4. **Commit with a scalpel as you work, not a shotgun after.** Stage
   specific files by name — never `git add -A` / `git add .`.
5. **Do not touch pre-existing dirty/untracked files that are not yours**
   (`AGENTS.md`, `.pi/`, `layer/` session files, etc.). Check `git status`
   first.
6. Baseline must be green before starting and after every commit:
   `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh`
7. Order: T0 → T1 → T2 → T3 → T4 → T5. Each task is independently
   shippable; if context runs low, stop at a task boundary and report.

## Sealing mechanics (applies to every seal task)

- **Private fields + read-only accessors.** A struct with any private
  field cannot be constructed outside its defining module, and that holds
  across crates. Do NOT add a `_sealed` marker field — that is the trait-
  sealing pattern and is redundant complexity here.
- **Constructors stay inside the kernel evaluators** that already produce
  these records (`evaluate_child_call_authority`,
  `evaluate_toy_grant_for_call`, `revalidate_route_for_execution`). No
  other public constructor, no `Default`, no public struct literal path.
- **Remove `Clone`, `Serialize`, and `Deserialize`** from sealed types.
  Keep `Debug`; keep `PartialEq/Eq` if tests use it.
- **No forge escape hatches.** No `#[cfg(test)]` constructors, no
  `#[doc(hidden)]` builders. Cross-crate tests must obtain capabilities by
  running real kernel evaluations with test fixtures. This is deliberate:
  it makes the tests exercise the actual authority path. Budget for it —
  the test migration is most of each task's diff. Where many tests need
  the same minting boilerplate, add ONE shared test-fixture helper (in the
  consuming crate's test support) that calls the real evaluator — shared
  logic, not per-test copies.
- **What stays serializable:** decisions, evaluations, observations,
  grants, approvals, assignments — the persisted *facts*. Only the three
  executable `Authorized*` records get sealed. If resuming from persisted
  state, do not rehydrate a capability: re-run kernel revalidation and
  mint a fresh one.

## Task T0 — Housekeeping

a) Verify state: `git log --oneline -3` and confirm PHASE2.md shows all
   tasks checked. If `layer/surface/build/audit-remediation/PHASE3.md`
   already exists, continue from whatever it marks incomplete instead of
   starting over. If the tree state contradicts this prompt, stop and
   report.
b) Save this entire prompt verbatim as
   `layer/surface/build/audit-remediation/PHASE3.md` with a checklist
   header (same format as PHASE2.md) and commit it BEFORE any code work.
   Check each task off in that file in the same commit as the task's final
   change.

## Task T1 — Persistence split: run records store provenance, not capabilities

`MctRuntimeRunRecord.authorized_child_invocation:
Option<AuthorizedChildInvocation>` (`crates/mct-daemon/src/state.rs:116`)
persists an executable authorization record into SQLite and serializes it
into control-plane snapshots. Under the capability model, persisted state
carries *evidence*, not *authority*.

- Replace the field with the provenance references the capability already
  carries (its evaluation/decision/observation IDs and the identifying
  child/assignment fields the run record actually needs for display —
  read the consumers in `state.rs`, `control.rs`, and `main.rs` to
  determine the minimal set; a small serializable
  `ChildInvocationProvenance` record is acceptable if plain IDs are not
  enough).
- Bump `SCHEMA_VERSION` (currently 3) and handle existing rows the way the
  store already handles schema changes — read how the current migration
  path works before writing one. Fail closed on unreadable rows.
- DISCLOSED BEHAVIOR CHANGE: control-plane snapshot JSON for runs changes
  shape (provenance refs instead of the embedded record). Update affected
  tests deliberately and say so in the commit message.
- After this task, nothing in the workspace requires
  `Serialize`/`Deserialize` on `AuthorizedChildInvocation`. Verify with
  grep before proceeding.

## Task T2 — Seal `AuthorizedChildInvocation` (single-effect capability)

Read `evaluate_child_call_authority` in `crates/mct-kernel/src/child.rs`
and every daemon consumption site first.

- Apply the sealing mechanics. This capability authorizes exactly one
  child invocation: additionally remove `Clone` and make the effect
  entrypoints (process harness, wasm component invocation) consume it
  **by value** — ownership encodes "this authority is spent."
- Migrate all construction sites (grep shows literals in `process.rs`,
  `supervisor.rs`, `main.rs`, `wasm.rs`, `state.rs` tests) to mint via the
  real evaluator through a shared test fixture.
- Test: a compile-fail check is not required, but add a unit test proving
  the evaluator is the only mint path you could find (document the grep
  audit in the commit message: zero struct-literal constructions remain
  outside the kernel module).

## Task T3 — Seal `AuthorizedToyCall` (session-scoped capability)

Read `evaluate_toy_grant_for_call` in `crates/mct-kernel/src/toy.rs`, the
WIT host adapter (`crates/mct-daemon/src/wasm.rs` — note
`MctWitToyHostAdapter` and `MctWasmToyHostImport` embed this record, the
latter with serde derives), and `call_authorized_toy` in
`crates/mct-daemon/src/toy.rs` first.

- Apply the sealing mechanics. Scope decision (deliberate, document it in
  rustdoc on the type): this capability is **session-scoped** — minted
  once per authorized component invocation and borrowed (`&`) for the
  multiple toy calls the component makes during that invocation (this is
  the existing, intentional `next_toy_call_index` model — preserve it).
  `!Clone` ensures it cannot outlive its session by copying; the per-call
  `MctToyCallIds` observations remain the per-use receipts.
- `MctWasmToyHostImport` / `MctWitToyHostAdapter` lose their serde derives
  (or the embedded capability moves out of the serialized shape — read
  how they're actually used and pick the simpler; if nothing deserializes
  them today, dropping the derives is the answer).
- Migrate construction sites and tests via the shared fixture pattern.

## Task T4 — Seal `AuthorizedRouteExecution` (single-effect capability)

Read `revalidate_route_for_execution` in `crates/mct-kernel/src/route.rs`
and its consumers first. Same treatment as T2: sealing mechanics, `!Clone`,
by-value consumption at the execution site, tests mint via the real
revalidation evaluator.

## Task T5 — Staleness guard at the effect boundary

Capabilities carry decision provenance; ensure each sealed capability also
exposes (via accessor) the `policy_revision`/`grants_revision` it was
minted under — most already embed an `AuthorityContextSnapshot` or
equivalent; add it only where genuinely absent, do not restructure what
exists.

- At each effect entrypoint that consumes a capability, add a cheap
  equality check of the capability's minted revisions against the current
  revisions the caller already has in hand. Mismatch → typed denial +
  observation, never execution. This composes with (does not replace) the
  existing route revalidation stage.
- This is the only Phase 3 task that adds a check rather than moving
  visibility; keep it small. If a call path has no current-revision fact
  available without new plumbing, note it in PHASE3.md as future work
  rather than building speculative plumbing now.

## Definition of done (every task)

- `cargo test --workspace` green; `cargo clippy --workspace --all-targets
  -- -D warnings` clean; `./scripts/ci-tier0.sh` passes.
- No deny path weakened; kernel public API grows only accessors and
  loses only derives; no forge escape hatches anywhere.
- Zero behavior change except the disclosed T1 snapshot shape change.
- Each commit is one coherent step; message states WHAT invariant the
  change enforces. Check tasks off in PHASE3.md as you go.
- Final summary: commits landed, grep-audit results (zero out-of-kernel
  constructions of the three sealed types), tasks completed/remaining per
  PHASE3.md, full validation results.

## Flake log

### 2026-07-03 — T2 compile failure, not intermittent ledger-writer flake

Command:

```bash
cargo test --workspace
```

Failure output:

```text
   Compiling mct-kernel v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-kernel)
error[E0277]: can't compare `&id::ChildInstanceId` with `id::ChildInstanceId`
    --> crates/mct-kernel/src/child.rs:1090:9
     |
1090 | /         assert_eq!(
1091 | |             authorized.child_instance_id(),
1092 | |             ChildInstanceId::new("instance-slate-manager-1")
1093 | |                 .expect("string ID literal/generated value must be non-empty")
1094 | |         );
     | |_________^ no implementation for `&id::ChildInstanceId == id::ChildInstanceId`
     |
     = help: the trait `PartialEq<id::ChildInstanceId>` is not implemented for `&id::ChildInstanceId`
     = note: this error originates in the macro `assert_eq` (in Nightly builds, run with -Z macro-backtrace for more info)
help: consider dereferencing here
    -->  /Users/nicabar/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/macros/mod.rs:46:22
     |
  46 |                 if !(**left_val == *right_val) {
     |                      +

error[E0277]: can't compare `&id::ChildAssignmentId` with `id::ChildAssignmentId`
    --> crates/mct-kernel/src/child.rs:1095:9
     |
1095 | /         assert_eq!(
1096 | |             authorized.assignment_id(),
1097 | |             ChildAssignmentId::new("assignment-slate-manager")
1098 | |                 .expect("string ID literal/generated value must be non-empty")
1099 | |         );
     | |_________^ no implementation for `&id::ChildAssignmentId == id::ChildAssignmentId`
     |
     = help: the trait `PartialEq<id::ChildAssignmentId>` is not implemented for `&id::ChildAssignmentId`
     = note: this error originates in the macro `assert_eq` (in Nightly builds, run with -Z macro-backtrace for more info)
help: consider dereferencing here
    -->  /Users/nicabar/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/macros/mod.rs:46:22
     |
  46 |                 if !(**left_val == *right_val) {
     |                      +

For more information about this error, try `rustc --explain E0277`.
error: could not compile `mct-kernel` (lib test) due to 2 previous errors
warning: build failed, waiting for other jobs to finish...
```

Assessment: deterministic compile error from accessor migration in T2 tests;
not the ledger-writer flake and no `JsonlObservationLedger` writer-lock path
was involved.

### 2026-07-03 — T2 compile failure, remaining borrowed capability call

Command:

```bash
cargo test --workspace
```

Failure output:

```text
   Compiling mct-kernel v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-kernel)
   Compiling mct-observation v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-observation)
   Compiling mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0308]: mismatched types
    --> crates/mct-daemon/src/wasm.rs:2750:17
     |
2749 |             .invoke_authorized_child_wit_export(
     |              ---------------------------------- arguments to this method are incorrect
2750 |                 &authorized,
     |                 ^^^^^^^^^^^ expected `AuthorizedChildInvocation`, found `&AuthorizedChildInvocation`
     |
note: method defined here
    --> crates/mct-daemon/src/wasm.rs:814:12
     |
 814 |     pub fn invoke_authorized_child_wit_export(
     |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
 815 |         &self,
 816 |         authorized: AuthorizedChildInvocation,
     |         -------------------------------------
help: consider removing the borrow
     |
2750 -                 &authorized,
2750 +                 authorized,
     |

For more information about this error, try `rustc --explain E0308`.
error: could not compile `mct-daemon` (lib test) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
```

Assessment: deterministic compile error from by-value T2 effect-entrypoint
migration; not the ledger-writer flake and no `JsonlObservationLedger`
writer-lock path was involved.

### 2026-07-03 — T2 assertion update after real evaluator fixture IDs

Command:

```bash
cargo test --workspace
```

Failure output:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 6.76s
     Running unittests src/lib.rs (target/debug/deps/mct_daemon-a8edfdb472c9b5be)

running 87 tests
...
test wasm::tests::wasm_component_runtime_trap_maps_to_adapter_observation ... FAILED
...

failures:

---- wasm::tests::wasm_component_runtime_trap_maps_to_adapter_observation stdout ----

thread 'wasm::tests::wasm_component_runtime_trap_maps_to_adapter_observation' (2197679) panicked at crates/mct-daemon/src/wasm.rs:2840:9:
assertion `left == right` failed
  left: Some(DecisionId("decision-child-wasm"))
 right: Some(DecisionId("decision-wasm"))
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    wasm::tests::wasm_component_runtime_trap_maps_to_adapter_observation

test result: FAILED. 86 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s

error: test failed, to rerun pass `-p mct-daemon --lib`
```

Assessment: deterministic assertion drift after replacing a forged test
literal with a real evaluator-minted capability; not the ledger-writer flake
and no `JsonlObservationLedger` writer-lock path was involved.

### 2026-07-03 — T2 post-commit validation ledger-writer flake

Command:

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
```

Failure output:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.21s
     Running unittests src/lib.rs (target/debug/deps/mct_daemon-a8edfdb472c9b5be)

running 87 tests
...
test tests::fake_echo_slice_records_trace_and_result ... FAILED
...

failures:

---- tests::fake_echo_slice_records_trace_and_result stdout ----

thread 'tests::fake_echo_slice_records_trace_and_result' (2204151) panicked at crates/mct-daemon/src/lib.rs:199:91:
called `Result::unwrap()` on an `Err` value: WriterLock { path: "/var/folders/6h/329275913d1d3k1lfvvvryp40000gn/T/.tmp7PEPjJ/observations.jsonl", source: Custom { kind: WouldBlock, error: "observation ledger is already locked by another writer" } }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    tests::fake_echo_slice_records_trace_and_result

test result: FAILED. 86 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s

error: test failed, to rerun pass `-p mct-daemon --lib`
```

Assessment: this matches the rare ledger-writer flake. The failing test is
`tests::fake_echo_slice_records_trace_and_result`; the failure path opens a
`JsonlObservationLedger` writer for a temp `observations.jsonl` path while an
advisory writer lock is still reported as held (`WouldBlock`). Per protocol,
no fix attempted mid-task; rerun will determine intermittence.

### 2026-07-03 — T3 compile failure, route toy capability move ordering

Command:

```bash
cargo test --workspace
```

Failure output:

```text
   Compiling mct-kernel v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-kernel)
error[E0382]: borrow of partially moved value: `toy`
   --> crates/mct-kernel/src/route.rs:427:55
    |
417 |         let Some(authorized_toy) = toy.authorized else {
    |                  -------------- value partially moved here
...
427 |         if toy.evaluation.call_id != call.call_id || !toy.is_allowed() {
    |                                                       ^^^ value borrowed here after partial move
    |
    = note: partial move occurs because value has type `toy::AuthorizedToyCall`, which does not implement the `Copy` trait
help: borrow this binding in the pattern to avoid moving the value
    |
417 |         let Some(ref authorized_toy) = toy.authorized else {
    |                  +++

For more information about this error, try `rustc --explain E0382`.
error: could not compile `mct-kernel` (lib) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `mct-kernel` (lib test) due to 1 previous error
```

Assessment: deterministic compile error from moving the now-non-Clone toy
capability before finishing result validation in route revalidation; not the
ledger-writer flake and no `JsonlObservationLedger` writer-lock path was
involved.

### 2026-07-03 — T3 CI formatting failure after toy sealing edits

Command:

```bash
cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
```

Failure output:

```text
    Checking mct-kernel v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-kernel)
    Checking mct-observation v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-observation)
    Checking mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.85s
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon/src/authority_test_fixture.rs:118:
...
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon/src/wasm.rs:1369:
...
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-kernel/src/route.rs:746:
...
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-kernel/src/route.rs:778:
...
```

Assessment: deterministic formatting failure from T3 edits; clippy had
already passed, and no `JsonlObservationLedger` writer-lock path was involved.

# Resident decomposition phase tasks

## Operator prompt (verbatim)

```text
You are starting the resident decomposition phase in patina-mct: a
BEHAVIOR-OWNING refactor of crates/mct-daemon/src/daemon/resident.rs
(~6,000 lines) into a designed module structure. This is not S2.5's
move-only slice: you are allowed — required — to design internal
module APIs. You are NOT allowed to change behavior: every semantic,
every typed outcome, every observation, every wire byte stays
identical, held by the 290-test suite and the contract-test net from
Track 3. This phase has an OPERATOR GATE: the seam-design SPEC stops
for review before any code moves, because module APIs are new
internal contracts and contracts are operator decisions.

## Step 0 — Re-establish state (STOP and report if anything differs)

a) Branch `patina`, expected HEAD 97e3041, aligned with origin/patina
   and origin/main (post PR #20). Commit pending session artifacts
   via your normal flow; tree otherwise clean.
b) Read ALL of crates/mct-daemon/src/daemon/resident.rs — the seam
   design must come from the code, not from the hints below. Then:
   the S2.5 seam plan and itch list in
   layer/surface/build/spec-drift-audit/track1/TASKS.md (the itch
   list names fixture sprawl and the cross-stage records this phase
   exists to resolve); the 15 Resident* struct definitions and every
   site where one crosses a would-be stage boundary; the kernel types
   that already serve as boundaries (RouteDecision,
   AuthorizedRouteExecution, CandidateRoute, the payload handles) so
   binary-local records don't duplicate kernel contracts;
   layer/surface/build/spec-drift-audit/track3/LEDGER.md — its rows
   cite tests by module::name, and relocations must not strand it.

## Working principles (binding)

Favor strong invariants over defensive fallbacks. Make bad states
impossible where practical. Do not add complexity to paper over
unclear design. Prefer simple data models, explicit contracts, and
shared logic over local patches, duplicated code, or speculative
abstractions — the ENUM/RECORD types are the extension points; do
NOT introduce traits unless a specific boundary demonstrably needs
one, and say so in the SPEC if it does. Write Rust code that Jon
Gjengset would agree with. Always read code before writing code. Git
update with scalpel as you work, not with shotgun after. Kernel
decides, adapters perform. Fail closed. Stop at a task boundary if
context runs low.

Validation green after EVERY commit:
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
Flake protocol: capture failures verbatim in the phase TASKS.md
(layer/surface/build/feat/resident-decomposition/TASKS.md — create it
with this prompt verbatim and a checklist in your first commit).

## Verification protocol (operator-enforced)

- Test count: the operator's independent baseline at 97e3041 is 290
  (sum of passed + ignored across all `test result:` lines of
  `cargo test --workspace`). Before and after counts must equal 290.
  Do not rename tests; move them with their subjects.
- Library surface: the operator diffs the mct-daemon LIBRARY's public
  surface across the phase. Expected delta: zero pub additions EXCEPT
  promotions the approved SPEC names with justification. If the
  compiler pushes you toward an unplanned lib `pub`, that is a design
  smell — restructure binary-locally or STOP.
- Ledger integrity: every LEDGER.md row citing a relocated test is
  updated to its new module::name in the SAME commit that moves it.
  A stale ledger row is a defect.

## Task R1 — Seam-design SPEC (GATE: STOP after committing; operator
reviews before R2)

Write layer/surface/build/feat/resident-decomposition/SPEC.md
deciding:
- The module tree under daemon/resident/ (mod.rs plus submodules).
  Candidate stages to evaluate against the code — refine as it
  dictates: candidates (local + remote sourcing, plans), decision
  (rank, RouteDecision construction, no-route), execution (revision
  guard, delivery preflight, child dispatch, result capture,
  route_taken projection), forwarding (outbound hello/call client,
  reply mapping and verification), serving (endpoint config, sinks,
  ledger wiring, idempotency integration), plus shared observation
  projection if the code says it is genuinely shared.
- The DISPOSITION OF ALL 15 Resident* records, each one: stage
  interface (which modules exchange it, at what visibility), stage
  internal (which module owns it privately), or restructure (split /
  merge / rename — with the reason; renames are permitted in this
  phase ONLY when the SPEC declares them). This table is the heart of
  the gate review.
- Any LIBRARY promotions, each with a one-sentence justification.
  Evaluate specifically: the forwarding client (conceptually a
  peer-client capability, not binary glue) — promote or keep, and
  why. Default remains binary-local.
- The test and fixture plan: which inline test groups move to which
  submodules; how the broad shared resident fixture is split into
  focused per-stage fixtures (the itch-list item) WITHOUT weakening
  any assertion; confirmation that the count stays 290.
- What this phase deliberately does NOT do: no semantic changes, no
  new observation kinds, no wire changes, no itch-list fixes beyond
  the fixture split, no speculative stage APIs for future features
  (brokered submission gets designed when it exists).
Commit the SPEC (`docs: specify resident decomposition`) and STOP.

## Task R2+ (after the gate releases — do not start)

Planned shape, refined by the approved SPEC: execute stage by stage,
one scalpel commit per extracted module in dependency order
(`refactor(daemon): extract resident <stage>`), tests and fixtures
moving with their subjects, LEDGER.md rows updated in the same
commits, validation green per commit. Close with a phase summary:
final line counts for resident/mod.rs and every submodule,
before/after test counts, the record-disposition table as
implemented, any approved promotions as landed, itch list for future
work, ROADMAP note if anything surfaced.

## Boundary

STOP at the R1 gate. Final report for this run: the SPEC as
committed, the 15-record disposition table verbatim, proposed
promotions with justifications, and anything the code taught you that
the operator should know before reviewing. Stay on `patina`; no
pushes, PRs, or merges.
```

## Checklist

- [x] R0: verify `patina` at `97e3041`, aligned with `origin/patina` and `origin/main`, with a clean tree.
- [x] R0: establish the independent workspace baseline: 290 passed + ignored (290 passed, 0 ignored).
- [x] R0: commit this phase task surface as the first phase commit.
- [x] R1: read all of `crates/mct-daemon/src/daemon/resident.rs`.
- [x] R1: read the Track 1 S2.5 seam plan and itch list.
- [x] R1: inventory all 15 `Resident*` records and every would-be stage-boundary crossing.
- [x] R1: inspect existing kernel boundary types and avoid duplicate binary-local contracts.
- [x] R1: map Track 3 ledger test citations to proposed test destinations.
- [x] R1: write `SPEC.md` with the module tree, record dispositions, promotions, test/fixture plan, and explicit non-goals.
- [x] R1: validate 290 tests, Clippy with warnings denied, Tier 0, and diff hygiene.
- [x] R1: commit `docs: specify resident decomposition`.
- [x] GATE: stop for operator review before moving code.
- [x] GATE: operator approved R1 with four binding conditions.
- [x] R2.0: amend the SPEC with the gate conditions and validate.
- [x] R2.1: extract resident observation.
- [x] R2.2: extract resident payload.
- [x] R2.3: extract resident publication.
- [x] R2.4: extract resident idempotency.
- [x] R2.5: extract resident candidates.
- [x] R2.6: extract resident decision.
- [x] R2.7: extract resident execution.
- [x] R2.8: extract resident forwarding.
- [x] R2.9: extract resident pipeline.
- [ ] R2.10: extract resident serving.
- [ ] R2.11: close with line counts, test counts, implemented record table, itch list, and ROADMAP disposition.

## Validation log

- Baseline at `97e3041`: 290 passed, 0 ignored, total 290.
- Baseline after the approved JVM local-CAS behavior fix `2a43b0f`: 291 passed, 0 ignored, total 291. All R2 extraction commits and close-out compare against 291.
- First phase commit: workspace tests 290, Clippy clean with warnings denied, Tier 0 clean, diff check clean.
- R1 pre-commit gate: workspace tests 290, Clippy clean with warnings denied, Tier 0 clean, diff check clean.

## Condition-4 finding

- The production JVM adapter constructed `ResidentRequestPayload::remote` despite being a local ingress origin. The operator adjudicated the production constructor as the bug; `2a43b0f` now uses the local-CAS-permitting path, covered by `ingress::tests::jvm_ingress_dereferences_local_content_addressed_blob`.

## Failure log

Capture validation failures verbatim here before rerunning. None observed through R1.

### R2.1 observation extraction compile failure

```text
$ cargo check --workspace
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0583]: file not found for module `observation`
 --> crates/mct-daemon/src/daemon/resident.rs:3:1
  |
3 | mod observation;
  | ^^^^^^^^^^^^^^^^
  |
  = help: to create the module `observation`, create file "crates/mct-daemon/src/daemon/observation.rs" or "crates/mct-daemon/src/daemon/observation/mod.rs"
  = note: if there is a `mod observation` elsewhere in the crate already, import it with `use crate::...` instead

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:401:13
    |
401 |     ledger: ResidentLedgerWriter,
    |             ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:503:13
    |
503 |     ledger: ResidentLedgerWriter,
    |             ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
    --> crates/mct-daemon/src/daemon/resident.rs:1187:13
     |
1187 |     ledger: ResidentLedgerWriter,
     |             ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
    --> crates/mct-daemon/src/daemon/resident.rs:1196:13
     |
1196 |     ledger: ResidentLedgerWriter,
     |             ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
    --> crates/mct-daemon/src/daemon/resident.rs:1227:13
     |
1227 |     ledger: ResidentLedgerWriter,
     |             ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
    --> crates/mct-daemon/src/daemon/resident.rs:1319:13
     |
1319 |     ledger: ResidentLedgerWriter,
     |             ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
    --> crates/mct-daemon/src/daemon/resident.rs:2065:13
     |
2065 |     ledger: ResidentLedgerWriter,
     |             ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
   --> crates/mct-daemon/src/daemon/control.rs:348:14
    |
348 |     ledger: &ResidentLedgerWriter,
    |              ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
   --> crates/mct-daemon/src/daemon/control.rs:656:14
    |
656 |     ledger: &ResidentLedgerWriter,
    |              ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
   --> crates/mct-daemon/src/daemon/control.rs:748:14
    |
748 |     ledger: &ResidentLedgerWriter,
    |              ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
   --> crates/mct-daemon/src/daemon/control.rs:770:14
    |
770 |     ledger: &ResidentLedgerWriter,
    |              ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
    --> crates/mct-daemon/src/daemon/control.rs:1080:14
     |
1080 |     ledger: &ResidentLedgerWriter,
     |              ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
    --> crates/mct-daemon/src/daemon/control.rs:1376:14
     |
1376 |     ledger: &ResidentLedgerWriter,
     |              ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
    --> crates/mct-daemon/src/daemon/control.rs:1449:13
     |
1449 |     ledger: ResidentLedgerWriter,
     |             ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
    --> crates/mct-daemon/src/daemon/control.rs:1536:13
     |
1536 |     ledger: ResidentLedgerWriter,
     |             ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
    --> crates/mct-daemon/src/daemon/control.rs:1658:14
     |
1658 |     ledger: &ResidentLedgerWriter,
     |              ^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `ResidentLedgerWriter` in this scope
    --> crates/mct-daemon/src/daemon/control.rs:1778:13
     |
1778 |     ledger: ResidentLedgerWriter,
     |             ^^^^^^^^^^^^^^^^^^^^ not found in this scope

warning: unused imports: `MctIrohObservationBatch`, `MctIrohObservationDurability`, and `MctIrohObservationSink`
  --> crates/mct-daemon/src/main.rs:27:35
   |
27 |     MctIrohConcurrentServeConfig, MctIrohObservationBatch, MctIrohObservationDurability,
   |                                   ^^^^^^^^^^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
28 |     MctIrohObservationSink, MctIrohServeEvent, MctIrohServeState, MctIrohServedProtocol,
   |     ^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default

warning: unused imports: `DurabilityClass` and `ExportStatus`
  --> crates/mct-daemon/src/main.rs:36:23
   |
36 | use mct_observation::{DurabilityClass, ExportStatus, JsonlObservationLedger};
   |                       ^^^^^^^^^^^^^^^  ^^^^^^^^^^^^

warning: unused import: `observation::*`
 --> crates/mct-daemon/src/daemon/resident.rs:4:16
  |
4 | pub(super) use observation::*;
  |                ^^^^^^^^^^^^^^

error[E0433]: cannot find type `ResidentLedgerWriter` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:233:18
    |
233 |     let ledger = ResidentLedgerWriter::spawn(config.ledger_path.clone())?;
    |                  ^^^^^^^^^^^^^^^^^^^^ use of undeclared type `ResidentLedgerWriter`

error[E0425]: cannot find function `resident_iroh_observation_sink` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:338:28
    |
338 |     let observation_sink = resident_iroh_observation_sink(ledger.clone());
    |                            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find function `resident_endpoint_observation` in this scope
    --> crates/mct-daemon/src/daemon/resident.rs:377:22
     |
 377 |           .append(vec![resident_endpoint_observation(
     |                        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
1912 | / pub(super) fn resident_candidate_observations(
1913 | |     call: &MctCall,
1914 | |     plans: &[ResidentCandidatePlan],
1915 | | ) -> Vec<MctObservation> {
...    |
1950 | |     observations
1951 | | }
     | |_- similarly named function `resident_candidate_observations` defined here
     |
help: a function with a similar name exists
     |
 377 -         .append(vec![resident_endpoint_observation(
 377 +         .append(vec![resident_candidate_observations(
     |

error[E0433]: cannot find type `ResidentLedgerWriter` in this scope
   --> crates/mct-daemon/src/daemon/ingress.rs:102:18
    |
102 |     let ledger = ResidentLedgerWriter::spawn(ledger_path.clone())?;
    |                  ^^^^^^^^^^^^^^^^^^^^ use of undeclared type `ResidentLedgerWriter`

error[E0433]: cannot find type `ResidentLedgerWriter` in this scope
   --> crates/mct-daemon/src/daemon/ingress.rs:310:18
    |
310 |     let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).with_context(|| {
    |                  ^^^^^^^^^^^^^^^^^^^^ use of undeclared type `ResidentLedgerWriter`

error[E0425]: cannot find function `resident_iroh_observation_sink` in this scope
   --> crates/mct-daemon/src/daemon/ingress.rs:316:28
    |
316 |     let observation_sink = resident_iroh_observation_sink(ledger.clone());
    |                            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0433]: cannot find type `ResidentLedgerWriter` in this scope
   --> crates/mct-daemon/src/daemon/ingress.rs:420:18
    |
420 |     let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).with_context(|| {
    |                  ^^^^^^^^^^^^^^^^^^^^ use of undeclared type `ResidentLedgerWriter`

error[E0425]: cannot find function `resident_iroh_observation_sink` in this scope
   --> crates/mct-daemon/src/daemon/ingress.rs:426:28
    |
426 |     let observation_sink = resident_iroh_observation_sink(ledger.clone());
    |                            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope

Some errors have detailed explanations: E0425, E0433, E0583.
For more information about an error, try `rustc --explain E0425`.
warning: `mct-daemon` (bin "mct-daemon") generated 3 warnings
error: could not compile `mct-daemon` (bin "mct-daemon") due to 26 previous errors; 3 warnings emitted
```

### JVM local-CAS origin mismatch expected red

```text
$ cargo test -p mct-daemon --bin mct-daemon ingress::tests::jvm_ingress_dereferences_local_content_addressed_blob -- --exact
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.38s
     Running unittests src/main.rs (target/debug/deps/mct_daemon-701d058281c133f0)

running 1 test
test ingress::tests::jvm_ingress_dereferences_local_content_addressed_blob ... FAILED

failures:

---- ingress::tests::jvm_ingress_dereferences_local_content_addressed_blob stdout ----

thread 'ingress::tests::jvm_ingress_dereferences_local_content_addressed_blob' (1347929) panicked at crates/mct-daemon/src/daemon/ingress.rs:1163:9:
assertion failed: ledger_text.contains(&format!("payload:request:size={}:digest={digest}",
            payload.len()))
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    ingress::tests::jvm_ingress_dereferences_local_content_addressed_blob

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 64 filtered out; finished in 0.23s

error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

### R2.2 payload extraction compile failure

```text
$ cargo check --workspace
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0425]: cannot find function `blake3_hex` in this scope
   --> crates/mct-daemon/src/daemon/ingress.rs:219:36
    |
219 |                 blake3_digest_hex: blake3_hex(&payload),
    |                                    ^^^^^^^^^^ not found in this scope
    |
note: function `crate::resident::blake3_hex` exists but is inaccessible
   --> crates/mct-daemon/src/daemon/resident/payload.rs:36:1
    |
 36 | pub(super) fn blake3_hex(bytes: &[u8]) -> String {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not accessible

error[E0433]: cannot find type `ResidentRequestPayload` in this scope
   --> crates/mct-daemon/src/daemon/ingress.rs:140:9
    |
140 |         ResidentRequestPayload::local(inline_payload),
    |         ^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `ResidentRequestPayload`

Some errors have detailed explanations: E0425, E0433.
For more information about an error, try `rustc --explain E0425`.
error: could not compile `mct-daemon` (bin "mct-daemon") due to 2 previous errors
```

### R2.2 payload test relocation compile failure

```text
$ cargo test -p mct-daemon --bin mct-daemon resident::payload::tests -- --nocapture
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0433]: cannot find type `ResidentRequestPayload` in this scope
   --> crates/mct-daemon/src/daemon/resident/payload.rs:435:13
    |
435 |             ResidentRequestPayload::local(None),
    |             ^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `ResidentRequestPayload`
    |
help: a struct with a similar name exists
    |
435 -             ResidentRequestPayload::local(None),
435 +             VerifiedRequestPayload::local(None),
    |

error[E0433]: cannot find type `ResidentRequestPayload` in this scope
   --> crates/mct-daemon/src/daemon/resident/payload.rs:500:13
    |
500 |             ResidentRequestPayload::local(None),
    |             ^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `ResidentRequestPayload`
    |
help: a struct with a similar name exists
    |
500 -             ResidentRequestPayload::local(None),
500 +             VerifiedRequestPayload::local(None),
    |

error[E0433]: cannot find type `ResidentRequestPayload` in this scope
   --> crates/mct-daemon/src/daemon/resident/payload.rs:557:13
    |
557 |             ResidentRequestPayload::local(None),
    |             ^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `ResidentRequestPayload`
    |
help: a struct with a similar name exists
    |
557 -             ResidentRequestPayload::local(None),
557 +             VerifiedRequestPayload::local(None),
    |

For more information about this error, try `rustc --explain E0433`.
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 3 previous errors
```

### R2.3 publication extraction compile failure

```text
$ cargo check --workspace
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0425]: cannot find function `refresh_remote_surfaces_from_admitted_hello_response` in this scope
   --> crates/mct-daemon/src/daemon/ingress.rs:808:5
    |
808 |     refresh_remote_surfaces_from_admitted_hello_response(
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope
    |
note: function `crate::resident::refresh_remote_surfaces_from_admitted_hello_response` exists but is inaccessible
   --> crates/mct-daemon/src/daemon/resident/publication.rs:85:1
    |
 85 | / pub(super) fn refresh_remote_surfaces_from_admitted_hello_response(
 86 | |     state_path: &Path,
 87 | |     peer: &MctPeerAddressBookEntry,
 88 | |     response: &MctHelloResponse,
...   |
111 | |     Ok(true)
112 | | }
    | |_^ not accessible

For more information about this error, try `rustc --explain E0425`.
error: could not compile `mct-daemon` (bin "mct-daemon") due to 1 previous error
```

### R2.3 publication test relocation compile failure

```text
$ cargo test -p mct-daemon --bin mct-daemon resident::publication::tests
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0425]: cannot find function `contract_peer_expiry` in this scope
    --> crates/mct-daemon/src/daemon/resident/publication.rs:145:25
     |
 145 |             expires_at: contract_peer_expiry(),
     |                         ^^^^^^^^^^^^^^^^^^^^ not found in this scope
     |
note: these functions exist but are inaccessible
    --> crates/mct-daemon/src/daemon/resident.rs:2676:5
     |
2676 |     fn contract_peer_expiry() -> Timestamp {
     |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `crate::tests::contract_peer_expiry`: not accessible
     |
    ::: crates/mct-daemon/src/daemon/resident/observation.rs:146:5
     |
 146 |     fn contract_peer_expiry() -> Timestamp {
     |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `crate::resident::observation::tests::contract_peer_expiry`: not accessible

For more information about this error, try `rustc --explain E0425`.
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 1 previous error
```

### R2.4 idempotency extraction compile failure

```text
$ cargo check --workspace
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0425]: cannot find function `resident_idempotency_caller_scope` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:697:24
    |
697 |     let caller_scope = resident_idempotency_caller_scope(&request);
    |                        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope
    |
note: function `crate::resident::idempotency::resident_idempotency_caller_scope` exists but is inaccessible
   --> crates/mct-daemon/src/daemon/resident/idempotency.rs:5:1
    |
  5 | fn resident_idempotency_caller_scope(request: &MctCallProtocolRequest) -> String {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not accessible

error[E0425]: cannot find function `resident_idempotency_fingerprint` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:698:23
    |
698 |     let fingerprint = resident_idempotency_fingerprint(&request);
    |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope
    |
note: function `crate::resident::idempotency::resident_idempotency_fingerprint` exists but is inaccessible
   --> crates/mct-daemon/src/daemon/resident/idempotency.rs:36:1
    |
 36 | fn resident_idempotency_fingerprint(request: &MctCallProtocolRequest) -> MctIdempotencyFingerprint {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not accessible

error[E0425]: cannot find function `idempotency_expiry` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:699:28
    |
699 |     let expires_at = match idempotency_expiry(&now) {
    |                            ^^^^^^^^^^^^^^^^^^
    |
note: function `crate::resident::idempotency::idempotency_expiry` exists but is inaccessible
   --> crates/mct-daemon/src/daemon/resident/idempotency.rs:57:1
    |
 57 | fn idempotency_expiry(now: &Timestamp) -> Result<Timestamp> {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not accessible
help: a local variable with a similar name exists
    |
699 -     let expires_at = match idempotency_expiry(&now) {
699 +     let expires_at = match idempotency_key(&now) {
    |

error[E0425]: cannot find function `resident_idempotency_observation` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:726:30
    |
726 |                   .append(vec![resident_idempotency_observation(
    |                                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
   ::: crates/mct-daemon/src/daemon/resident/observation.rs:110:1
    |
110 | / pub(super) fn resident_endpoint_observation(
111 | |     observation_id: &'static str,
112 | |     endpoint_id: EndpointIdText,
113 | |     outcome: ObservationOutcome,
...   |
140 | | }
    | |_- similarly named function `resident_endpoint_observation` defined here
    |
note: function `crate::resident::idempotency::resident_idempotency_observation` exists but is inaccessible
   --> crates/mct-daemon/src/daemon/resident/idempotency.rs:134:1
    |
134 | / fn resident_idempotency_observation(
135 | |     request: &MctCallProtocolRequest,
136 | |     caller_scope: &str,
137 | |     fingerprint: &MctIdempotencyFingerprint,
...   |
202 | | }
    | |_^ not accessible
help: a function with a similar name exists
    |
726 -                 .append(vec![resident_idempotency_observation(
726 +                 .append(vec![resident_endpoint_observation(
    |

error[E0425]: cannot find function `recorded_reply_to_handler_result` in this scope
    --> crates/mct-daemon/src/daemon/resident.rs:737:13
     |
 737 |               recorded_reply_to_handler_result(*reply)
     |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
1939 | / pub(super) fn remote_reply_to_call_handler_result(
1940 | |     reply: MctIrohCallPayloadReply,
1941 | |     route_decision_id: DecisionId,
1942 | |     route_taken: RouteTaken,
...    |
1981 | | }
     | |_- similarly named function `remote_reply_to_call_handler_result` defined here
     |
note: function `crate::resident::idempotency::recorded_reply_to_handler_result` exists but is inaccessible
    --> crates/mct-daemon/src/daemon/resident/idempotency.rs:76:1
     |
  76 | fn recorded_reply_to_handler_result(reply: MctRecordedCallReply) -> MctIrohCallHandlerResult {
     | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not accessible
help: a function with a similar name exists
     |
 737 -             recorded_reply_to_handler_result(*reply)
 737 +             remote_reply_to_call_handler_result(*reply)
     |

error[E0425]: cannot find function `resident_idempotency_observation` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:742:30
    |
742 |                   .append(vec![resident_idempotency_observation(
    |                                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
   ::: crates/mct-daemon/src/daemon/resident/observation.rs:110:1
    |
110 | / pub(super) fn resident_endpoint_observation(
111 | |     observation_id: &'static str,
112 | |     endpoint_id: EndpointIdText,
113 | |     outcome: ObservationOutcome,
...   |
140 | | }
    | |_- similarly named function `resident_endpoint_observation` defined here
    |
note: function `crate::resident::idempotency::resident_idempotency_observation` exists but is inaccessible
   --> crates/mct-daemon/src/daemon/resident/idempotency.rs:134:1
    |
134 | / fn resident_idempotency_observation(
135 | |     request: &MctCallProtocolRequest,
136 | |     caller_scope: &str,
137 | |     fingerprint: &MctIdempotencyFingerprint,
...   |
202 | | }
    | |_^ not accessible
help: a function with a similar name exists
    |
742 -                 .append(vec![resident_idempotency_observation(
742 +                 .append(vec![resident_endpoint_observation(
    |

error[E0425]: cannot find function `idempotency_refusal_result` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:753:13
    |
753 |             idempotency_refusal_result(reason)
    |             ^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope
    |
note: function `crate::resident::idempotency::idempotency_refusal_result` exists but is inaccessible
   --> crates/mct-daemon/src/daemon/resident/idempotency.rs:89:1
    |
 89 | fn idempotency_refusal_result(reason: MctIdempotencyReason) -> MctIrohCallHandlerResult {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not accessible

error[E0425]: cannot find function `handler_result_to_recorded_reply` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:757:28
    |
757 |             let recorded = handler_result_to_recorded_reply(&result);
    |                            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope
    |
note: function `crate::resident::idempotency::handler_result_to_recorded_reply` exists but is inaccessible
   --> crates/mct-daemon/src/daemon/resident/idempotency.rs:63:1
    |
 63 | fn handler_result_to_recorded_reply(result: &MctIrohCallHandlerResult) -> MctRecordedCallReply {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not accessible

warning: unused import: `idempotency::*`
  --> crates/mct-daemon/src/daemon/resident.rs:17:16
   |
17 | pub(super) use idempotency::*;
   |                ^^^^^^^^^^^^^^
   |
   = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default

For more information about this error, try `rustc --explain E0425`.
warning: `mct-daemon` (bin "mct-daemon") generated 1 warning
error: could not compile `mct-daemon` (bin "mct-daemon") due to 8 previous errors; 1 warning emitted
```

### R2.5 candidates extraction compile failure

```text
$ cargo check --workspace
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0433]: cannot find type `ResidentRemoteCandidateSource` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:854:24
    |
854 |     let remote_plans = ResidentRemoteCandidateSource::for_call(call)
    |                        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `ResidentRemoteCandidateSource`
    |
note: struct `crate::resident::candidates::ResidentRemoteCandidateSource` exists but is inaccessible
   --> crates/mct-daemon/src/daemon/resident/candidates.rs:20:1
    |
 20 | struct ResidentRemoteCandidateSource<'a> {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not accessible

error[E0425]: cannot find function `resident_remote_candidate_plans_from_source` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:856:13
    |
856 |               resident_remote_candidate_plans_from_source(config, state, source, now.clone())
    |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
   ::: crates/mct-daemon/src/daemon/resident/candidates.rs:342:1
    |
342 | / pub(super) fn resident_remote_candidate_observations(
343 | |     call: &MctCall,
344 | |     plans: &[RemoteCandidatePlan],
345 | | ) -> Vec<MctObservation> {
...   |
375 | |     observations
376 | | }
    | |_- similarly named function `resident_remote_candidate_observations` defined here
    |
note: function `crate::resident::candidates::resident_remote_candidate_plans_from_source` exists but is inaccessible
   --> crates/mct-daemon/src/daemon/resident/candidates.rs:65:1
    |
 65 | / fn resident_remote_candidate_plans_from_source(
 66 | |     config: &mct_daemon::MctDaemonConfig,
 67 | |     state: Option<&MctRuntimeStateStore>,
 68 | |     source: ResidentRemoteCandidateSource<'_>,
...   |
104 | |     Ok(plans)
105 | | }
    | |_^ not accessible
help: a function with a similar name exists
    |
856 -             resident_remote_candidate_plans_from_source(config, state, source, now.clone())
856 +             resident_remote_candidate_observations(config, state, source, now.clone())
    |

warning: glob import doesn't reexport anything with visibility `pub(crate)` because no imported item is public enough
  --> crates/mct-daemon/src/daemon/resident.rs:21:16
   |
21 | pub(super) use candidates::*;
   |                ^^^^^^^^^^^^^
   |
note: the most public imported item is `pub(self)`
  --> crates/mct-daemon/src/daemon/resident.rs:21:16
   |
21 | pub(super) use candidates::*;
   |                ^^^^^^^^^^^^^
   = help: reduce the glob import's visibility or increase visibility of imported items
   = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default

Some errors have detailed explanations: E0425, E0433.
For more information about an error, try `rustc --explain E0425`.
warning: `mct-daemon` (bin "mct-daemon") generated 1 warning
error: could not compile `mct-daemon` (bin "mct-daemon") due to 2 previous errors; 1 warning emitted
```

### R2.6 decision extraction compile failure

```text
$ cargo check --workspace
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0616]: field `candidate` of struct `decision::RemoteExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident.rs:824:24
    |
824 |             &execution.candidate,
    |                        ^^^^^^^^^ private field
    |
help: a method `candidate` also exists, call it with parentheses
    |
824 |             &execution.candidate(),
    |                                 ++

error[E0616]: field `candidate` of struct `decision::RemoteExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident.rs:899:32
    |
899 |                     &execution.candidate,
    |                                ^^^^^^^^^ private field
    |
help: a method `candidate` also exists, call it with parentheses
    |
899 |                     &execution.candidate(),
    |                                         ++

error[E0616]: field `initial_decision` of struct `decision::RemoteExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident.rs:941:28
    |
941 |                 &execution.initial_decision,
    |                            ^^^^^^^^^^^^^^^^ private field
    |
help: a method `initial_decision` also exists, call it with parentheses
    |
941 |                 &execution.initial_decision(),
    |                                            ++

error[E0616]: field `candidate` of struct `decision::RemoteExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident.rs:942:27
    |
942 |                 execution.candidate.clone(),
    |                           ^^^^^^^^^ private field
    |
help: a method `candidate` also exists, call it with parentheses
    |
942 |                 execution.candidate().clone(),
    |                                    ++

error[E0616]: field `candidate` of struct `decision::RemoteExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident.rs:949:24
    |
949 |         .get(execution.candidate.node_id.as_str())
    |                        ^^^^^^^^^ private field
    |
help: a method `candidate` also exists, call it with parentheses
    |
949 |         .get(execution.candidate().node_id.as_str())
    |                                 ++

error[E0616]: field `initial_decision` of struct `decision::RemoteExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident.rs:955:28
    |
955 |                 &execution.initial_decision,
    |                            ^^^^^^^^^^^^^^^^ private field
    |
help: a method `initial_decision` also exists, call it with parentheses
    |
955 |                 &execution.initial_decision(),
    |                                            ++

error[E0616]: field `candidate` of struct `decision::RemoteExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident.rs:956:27
    |
956 |                 execution.candidate.clone(),
    |                           ^^^^^^^^^ private field
    |
help: a method `candidate` also exists, call it with parentheses
    |
956 |                 execution.candidate().clone(),
    |                                    ++

error[E0616]: field `candidate` of struct `decision::RemoteExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident.rs:971:83
    |
971 |             && resident_candidate_for_remote_surface(&peer, surface) == execution.candidate
    |                                                                                   ^^^^^^^^^ private field
    |
help: a method `candidate` also exists, call it with parentheses
    |
971 |             && resident_candidate_for_remote_surface(&peer, surface) == execution.candidate()
    |                                                                                            ++

error[E0616]: field `initial_decision` of struct `decision::RemoteExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident.rs:976:28
    |
976 |                 &execution.initial_decision,
    |                            ^^^^^^^^^^^^^^^^ private field
    |
help: a method `initial_decision` also exists, call it with parentheses
    |
976 |                 &execution.initial_decision(),
    |                                            ++

error[E0616]: field `candidate` of struct `decision::RemoteExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident.rs:977:27
    |
977 |                 execution.candidate.clone(),
    |                           ^^^^^^^^^ private field
    |
help: a method `candidate` also exists, call it with parentheses
    |
977 |                 execution.candidate().clone(),
    |                                    ++

error[E0616]: field `initial_decision` of struct `decision::RemoteExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident.rs:995:28
    |
995 |                 &execution.initial_decision,
    |                            ^^^^^^^^^^^^^^^^ private field
    |
help: a method `initial_decision` also exists, call it with parentheses
    |
995 |                 &execution.initial_decision(),
    |                                            ++

error[E0616]: field `initial_decision` of struct `decision::RemoteExecutionPlan` is private
    --> crates/mct-daemon/src/daemon/resident.rs:1006:20
     |
1006 |         &execution.initial_decision,
     |                    ^^^^^^^^^^^^^^^^ private field
     |
help: a method `initial_decision` also exists, call it with parentheses
     |
1006 |         &execution.initial_decision(),
     |                                    ++

error[E0609]: no field `route_taken` on type `decision::LocalExecutionPlan`
    --> crates/mct-daemon/src/daemon/resident.rs:1232:34
     |
1232 |     let runtime_kind = execution.route_taken.runtime_kind;
     |                                  ^^^^^^^^^^^ unknown field
     |
     = note: available fields are: `child`, `authorized_route`, `child_authority_observation_id`

error[E0609]: no field `route_taken` on type `decision::LocalExecutionPlan`
    --> crates/mct-daemon/src/daemon/resident.rs:1270:33
     |
1270 |     let route_taken = execution.route_taken.clone();
     |                                 ^^^^^^^^^^^ unknown field
     |
     = note: available fields are: `child`, `authorized_route`, `child_authority_observation_id`

Some errors have detailed explanations: E0609, E0616.
For more information about an error, try `rustc --explain E0609`.
error: could not compile `mct-daemon` (bin "mct-daemon") due to 14 previous errors
```

### R2.6 decision extraction Clippy failure

```text
$ cargo clippy --workspace --all-targets -- -D warnings
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error: large size difference between variants
  --> crates/mct-daemon/src/daemon/resident/decision.rs:51:1
   |
51 | /   pub(super) enum RouteDisposition {
52 | | /     Denied {
53 | | |         decision: RouteDecision,
54 | | |         observations: Vec<MctObservation>,
55 | | |     },
   | | |_____- the largest variant contains at least 256 bytes
56 | | /     Local {
57 | | |         plan: Box<LocalExecutionPlan>,
58 | | |         observations: Vec<MctObservation>,
59 | | |     },
   | | |_____- the second-largest variant contains at least 32 bytes
...  |
63 | |       },
64 | |   }
   | |___^ the entire enum is at least 256 bytes
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#large_enum_variant
   = note: `-D clippy::large-enum-variant` implied by `-D warnings`
   = help: to override `-D warnings` add `#[allow(clippy::large_enum_variant)]`
help: consider boxing the large fields or introducing indirection in some other way to reduce the total size of the enum
   |
53 -         decision: RouteDecision,
53 +         decision: Box<RouteDecision>,
   |

error: could not compile `mct-daemon` (bin "mct-daemon") due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 1 previous error
```

### R2.7 execution extraction compile failure

```text
$ cargo check --workspace
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0425]: cannot find value `child_child_authority_observation_id` in this scope
   --> crates/mct-daemon/src/daemon/resident/execution.rs:152:9
    |
152 |         child_child_authority_observation_id.clone(),
    |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
help: a local variable with a similar name exists
    |
152 -         child_child_authority_observation_id.clone(),
152 +         child_authority_observation_id.clone(),
    |

error[E0423]: expected value, found module `child`
   --> crates/mct-daemon/src/daemon/resident/execution.rs:279:10
    |
279 |         &child,
    |          ^^^^^ not a value

error[E0616]: field `authorized_route` of struct `decision::LocalExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident/execution.rs:112:18
    |
112 |                 .authorized_route
    |                  ^^^^^^^^^^^^^^^^ private field

error[E0616]: field `authorized_route` of struct `decision::LocalExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident/execution.rs:127:18
    |
127 |                 .authorized_route
    |                  ^^^^^^^^^^^^^^^^ private field

error[E0616]: field `authorized_route` of struct `decision::LocalExecutionPlan` is private
   --> crates/mct-daemon/src/daemon/resident/execution.rs:139:10
    |
139 |         .authorized_route
    |          ^^^^^^^^^^^^^^^^ private field

Some errors have detailed explanations: E0423, E0425, E0616.
For more information about an error, try `rustc --explain E0423`.
error: could not compile `mct-daemon` (bin "mct-daemon") due to 5 previous errors
```

### R2.8 forwarding test relocation compile failure

```text
$ cargo test -p mct-daemon --bin mct-daemon resident::forwarding::tests
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0425]: cannot find function `test_child` in this scope
   --> crates/mct-daemon/src/daemon/resident/forwarding.rs:691:47
    |
691 |             .approve_and_assign_loaded_child(&test_child(), MctOperatorChildScope::default())
    |                                               ^^^^^^^^^^ not found in this scope
    |
note: these functions exist but are inaccessible
   --> crates/mct-daemon/src/daemon/resident.rs:709:5
    |
709 |     fn test_child() -> mct_daemon::MctLoadedChild {
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `crate::tests::test_child`: not accessible
    |
   ::: crates/mct-daemon/src/daemon/resident/decision.rs:339:5
    |
339 |     fn test_child() -> mct_daemon::MctLoadedChild {
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `crate::resident::decision::tests::test_child`: not accessible
    |
   ::: crates/mct-daemon/src/daemon/resident/candidates.rs:424:5
    |
424 |     fn test_child() -> mct_daemon::MctLoadedChild {
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `crate::resident::candidates::tests::test_child`: not accessible

For more information about this error, try `rustc --explain E0425`.
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 1 previous error
```

### R2.8 forwarding extraction Clippy failure

```text
$ cargo clippy --workspace --all-targets -- -D warnings
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error: unused imports: `endpoint_id_for_secret_key_hex` and `sign_peer_binding_signature_ref`
   --> crates/mct-daemon/src/daemon/resident.rs:597:20
    |
597 |     use mct_iroh::{endpoint_id_for_secret_key_hex, sign_peer_binding_signature_ref};
    |                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
    = note: `-D unused-imports` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(unused_imports)]`

error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 1 previous error
```

### R2.9 pipeline extraction compile failure

```text
$ cargo check --workspace
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0425]: cannot find function `execute_resident_call` in this scope
   --> crates/mct-daemon/src/daemon/ingress.rs:136:5
    |
136 |     execute_resident_call(
    |     ^^^^^^^^^^^^^^^^^^^^^ not found in this scope
    |
note: function `crate::resident::execute_resident_call` exists but is inaccessible
   --> crates/mct-daemon/src/daemon/resident/pipeline.rs:35:1
    |
 35 | / pub(super) async fn execute_resident_call(
 36 | |     paths: ResidentRuntimePaths,
 37 | |     ledger: ResidentLedgerWriter,
 38 | |     request: MctCallProtocolRequest,
...   |
 41 | |     execute_resident_call_at(paths, ledger, request, payload, current_timestamp()).await
 42 | | }
    | |_^ not accessible

error[E0616]: field `state_path` of struct `pipeline::ResidentRuntimePaths` is private
   --> crates/mct-daemon/src/daemon/resident/payload.rs:185:28
    |
185 |     let state_path = paths.state_path.clone();
    |                            ^^^^^^^^^^ private field
    |
help: a method `state_path` also exists, call it with parentheses
    |
185 |     let state_path = paths.state_path().clone();
    |                                      ++

error[E0616]: field `state_path` of struct `pipeline::ResidentRuntimePaths` is private
   --> crates/mct-daemon/src/daemon/resident/forwarding.rs:196:16
    |
196 |         &paths.state_path,
    |                ^^^^^^^^^^ private field
    |
help: a method `state_path` also exists, call it with parentheses
    |
196 |         &paths.state_path(),
    |                          ++

error[E0616]: field `config_path` of struct `pipeline::ResidentRuntimePaths` is private
  --> crates/mct-daemon/src/daemon/resident/decision.rs:95:51
   |
95 |     let config = MctDaemonConfigStore::new(&paths.config_path).load()?;
   |                                                   ^^^^^^^^^^^ private field
   |
help: a method `config_path` also exists, call it with parentheses
   |
95 |     let config = MctDaemonConfigStore::new(&paths.config_path()).load()?;
   |                                                              ++

error[E0616]: field `state_path` of struct `pipeline::ResidentRuntimePaths` is private
  --> crates/mct-daemon/src/daemon/resident/decision.rs:96:51
   |
96 |     let state = MctRuntimeStateStore::open(&paths.state_path)?;
   |                                                   ^^^^^^^^^^ private field
   |
help: a method `state_path` also exists, call it with parentheses
   |
96 |     let state = MctRuntimeStateStore::open(&paths.state_path())?;
   |                                                             ++

error[E0616]: field `children_dir` of struct `pipeline::ResidentRuntimePaths` is private
  --> crates/mct-daemon/src/daemon/resident/decision.rs:97:77
   |
97 |     let load_report = load_children_from_dir(MctChildLoadOptions::new(paths.children_dir.clone()));
   |                                                                             ^^^^^^^^^^^^ private field
   |
help: a method `children_dir` also exists, call it with parentheses
   |
97 |     let load_report = load_children_from_dir(MctChildLoadOptions::new(paths.children_dir().clone()));
   |                                                                                         ++

error[E0616]: field `config_path` of struct `pipeline::ResidentRuntimePaths` is private
  --> crates/mct-daemon/src/daemon/resident/execution.rs:80:51
   |
80 |     let config = MctDaemonConfigStore::new(&paths.config_path).load()?;
   |                                                   ^^^^^^^^^^^ private field
   |
help: a method `config_path` also exists, call it with parentheses
   |
80 |     let config = MctDaemonConfigStore::new(&paths.config_path()).load()?;
   |                                                              ++

error[E0616]: field `state_path` of struct `pipeline::ResidentRuntimePaths` is private
  --> crates/mct-daemon/src/daemon/resident/execution.rs:97:51
   |
97 |     let state = MctRuntimeStateStore::open(&paths.state_path)?;
   |                                                   ^^^^^^^^^^ private field
   |
help: a method `state_path` also exists, call it with parentheses
   |
97 |     let state = MctRuntimeStateStore::open(&paths.state_path())?;
   |                                                             ++

error[E0616]: field `config_path` of struct `pipeline::ResidentRuntimePaths` is private
   --> crates/mct-daemon/src/daemon/resident/forwarding.rs:280:51
    |
280 |     let config = MctDaemonConfigStore::new(&paths.config_path).load()?;
    |                                                   ^^^^^^^^^^^ private field
    |
help: a method `config_path` also exists, call it with parentheses
    |
280 |     let config = MctDaemonConfigStore::new(&paths.config_path()).load()?;
    |                                                              ++

error[E0616]: field `state_path` of struct `pipeline::ResidentRuntimePaths` is private
   --> crates/mct-daemon/src/daemon/resident/forwarding.rs:305:51
    |
305 |     let state = MctRuntimeStateStore::open(&paths.state_path)?;
    |                                                   ^^^^^^^^^^ private field
    |
help: a method `state_path` also exists, call it with parentheses
    |
305 |     let state = MctRuntimeStateStore::open(&paths.state_path())?;
    |                                                             ++

error[E0616]: field `state_path` of struct `pipeline::ResidentRuntimePaths` is private
   --> crates/mct-daemon/src/daemon/resident/forwarding.rs:362:65
    |
362 |         local_hello_capability_view_from_config(&config, &paths.state_path, &paths.children_dir)?;
    |                                                                 ^^^^^^^^^^ private field
    |
help: a method `state_path` also exists, call it with parentheses
    |
362 |         local_hello_capability_view_from_config(&config, &paths.state_path(), &paths.children_dir)?;
    |                                                                           ++

error[E0616]: field `children_dir` of struct `pipeline::ResidentRuntimePaths` is private
   --> crates/mct-daemon/src/daemon/resident/forwarding.rs:362:84
    |
362 |         local_hello_capability_view_from_config(&config, &paths.state_path, &paths.children_dir)?;
    |                                                                                    ^^^^^^^^^^^^ private field
    |
help: a method `children_dir` also exists, call it with parentheses
    |
362 |         local_hello_capability_view_from_config(&config, &paths.state_path, &paths.children_dir())?;
    |                                                                                                ++

Some errors have detailed explanations: E0425, E0616.
For more information about an error, try `rustc --explain E0425`.
error: could not compile `mct-daemon` (bin "mct-daemon") due to 12 previous errors
```

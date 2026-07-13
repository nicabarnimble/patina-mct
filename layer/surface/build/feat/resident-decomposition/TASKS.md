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

## Validation log

- Baseline at `97e3041`: 290 passed, 0 ignored, total 290.
- First phase commit: workspace tests 290, Clippy clean with warnings denied, Tier 0 clean, diff check clean.
- R1 pre-commit gate: workspace tests 290, Clippy clean with warnings denied, Tier 0 clean, diff check clean.

## Failure log

Capture validation failures verbatim here before rerunning. None observed through R1.

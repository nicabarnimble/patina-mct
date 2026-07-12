# Track 3 — contract-test propagation

## Operator prompt (verbatim)

```text
Track 3 slice 1 for patina-mct: contract-test propagation — turn the
tended semantic law (layer/allium/mct-product-map.allium and
layer/allium/mct-peer-ontology.allium) into named tests so future
spec-code drift becomes a CI failure instead of a manual audit
discovery. This slice establishes the obligation ledger, tests the
two seams where law is KNOWN to lead code, and fills gaps in the
priority contracts. Full-map coverage continues in later slices — do
not attempt it here.

## Task S0 — Push checkpoint (authorized) and state

a) Branch `patina`, expected HEAD 502defd (docs: disposition planner
   evidence and close audit rows) or session-artifact commits on top
   of it. Commit pending session artifacts via your normal flow; tree
   otherwise clean. STOP on any other mismatch.
b) AUTHORIZED, one-time: push the patina branch to origin
   (git push origin patina). Do NOT open, update, or merge any PR —
   that remains the operator's act. Report the pushed ref.
c) Read: both spec files in full (the map's tended sections
   especially: idempotency contract, payload/CAS, revision guard,
   route projection, binding proofs, publication evidence, and the
   companion references); the peer ontology's contracts;
   layer/surface/build/spec-drift-audit/REPORT.md (all dispositions);
   track2/TASKS.md follow-ups (the two law-leads-code seams). Run and
   record: allium plan and allium model on BOTH files — note which
   obligation categories the 3.5.0 CLI emits; contract invariants
   likely still require manual derivation per the propagate
   taxonomy.

## Working principles (binding)

Favor strong invariants over defensive fallbacks. Do not add
complexity to paper over unclear design. Prefer simple data models,
explicit contracts, and shared logic over local patches, duplicated
code, or speculative abstractions. Write Rust code that Jon Gjengset
would agree with. Always read code before writing code. Git update
with scalpel as you work, not with shotgun after. Tests live beside
their subjects per crate convention; do not regrow main.rs and do not
extend the broad shared resident fixture — smaller focused fixtures
beside the new tests. An existing test that covers an obligation is
REFERENCED in the ledger, never duplicated. Failing test first. Stop
at a task boundary if context runs low.

Validation green after EVERY commit:
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
Flake protocol: capture failures verbatim in
layer/surface/build/spec-drift-audit/track3/TASKS.md (create it with
this prompt verbatim and a checklist in your first commit).

## Task S1 — The obligation ledger
(one commit: `docs: open contract obligation ledger`)

Create layer/surface/build/spec-drift-audit/track3/LEDGER.md mapping
obligations to evidence for the PRIORITY contracts only:
- map: IdempotencyIsRequestScoped (the full tended contract),
  payload integrity + CAS store invariant, the effect-boundary
  revision guard, route_taken projection rule, Ed25519 binding proof
  enforcement, TwoPhaseRouting, NoRouteDecision,
  HelloObservationsBeforeEffects / AuthorityFactsAreDurableBeforeEffect,
  AuthorityDecisionsAreObserved / AdapterEffectsAreObserved;
- ontology: EligibleRouteCandidateDerivation (the conjunction),
  TerminalPeerCallSubmission, PerHopPeerAccountability,
  BilateralExecutableRouting, HonestLocalExecutionOffer /
  AdvertisementNeverGrantsAuthority, RolesAreCurrentProjections,
  OperatorPointedSubmissionIsDistinct.
Each ledger row: invariant/obligation → covering test(s) as
module::test_name, or GAP, or LAW-LEADS-CODE (the known two plus any
others you find while mapping — mapping honestly may surface more).
Structural allium-plan obligations for these areas get rows too,
mapped mostly to existing entity/wire tests. The 285 existing tests
are your primary material — this task is mostly attribution, and the
ledger is the artifact that makes coverage auditable.

## Task S2 — The law-leads-code seams
(one commit per seam, failing test first)

Known seam 1 — mandatory binding expiry: the tended law requires
peer-binding time bounds. Write the test against the law (a binding
without expiry, or an expired-unenforced path, must fail closed). If
code fails: the fix is spec-ward under existing law — apply it in
the same commit if small and unambiguous; STOP and report if it is
structural.
Known seam 2 — operator-pointed egress observation:
OperatorPointedSubmissionIsDistinct requires the operator's explicit
individual egress decision to be durably observed. Test the manual
CLI call path for a durable observation before the egress effect.
Same rule: small spec-ward fix in-commit; structural → STOP.
Any additional LAW-LEADS-CODE rows found in S1: same treatment, one
commit each, in ledger order. Update each ledger row and, if a code
fix landed, add a dated line to REPORT.md's summary noting drift
caught by contract propagation (the first CI-shaped catches).

## Task S3 — Gap tests for the priority contracts
(scalpel commits grouped by contract)

For every GAP row in S1: a focused test named for its obligation,
doc-comment citing the invariant by name, asserting the contract
through the real path (existing fixtures where they fit). Expected
gap areas from the arc's history: the candidacy conjunction as a
property (each conjunct removed → candidacy lost — several exist
individually; the ledger says whether the full conjunction is
proven); replay-never-revives-authority edge classes beyond
revocation (expiry, narrowing); publication honesty (a surface for a
non-ready child is never published — federation.rs filter has
partial coverage). Trust the ledger over this list. Update rows as
tests land; final state: every priority row is COVERED,
LAW-LEADS-CODE-resolved, or explicitly deferred with a reason.

## Boundary

STOP after S3. Later Track 3 slices extend the ledger beyond the
priority contracts; the gated resident.rs split follows once this
net exists. Final report: pushed ref; ledger summary (rows by
status); each law-leads-code outcome (test + fix or STOP report);
new tests by module; validation results; flake log; anything for
ROADMAP. No PR, no merge, no further pushes.
```

## Slice checklist

- [x] S0: verify exact clean `patina` baseline at `502defd`.
- [x] S0: push authorized checkpoint `patina` to `origin/patina` (`ab067ee..502defd`).
- [x] S0: read both laws, audit dispositions, and Track 2 follow-ups.
- [x] S0: capture Allium 3.5.0 plan/model categories for both laws.
- [ ] S1: commit the priority obligation ledger.
- [ ] S2.1: mandatory peer-binding expiry contract test and small fix or stop.
- [ ] S2.2: operator-pointed egress observation contract test and small fix or stop.
- [ ] S2.x: resolve any additional LAW-LEADS-CODE rows found by S1.
- [ ] S3: fill every priority GAP or explicitly defer it with reason.
- [ ] Final validation and report; no PR, merge, or further push.

## Allium 3.5.0 propagation baseline

- `allium plan layer/allium/mct-product-map.allium`: 179 structural obligations.
  - `entity_fields`: 56
  - `entity_optional`: 38
  - `surface_actor`: 27
  - `surface_exposure`: 27
  - `value_equality`: 29
  - `when_presence`: 2
- `allium model layer/allium/mct-product-map.allium`: 27 entities and 29 value types.
- `allium plan layer/allium/mct-peer-ontology.allium`: zero obligations.
- `allium model layer/allium/mct-peer-ontology.allium`: no entities or value types.
- Allium 3.5.0 does not emit contract `@invariant` obligations in `plan`; the priority contract rows are therefore manually derived using the propagation taxonomy.

## Failure and flake log

Capture every expected red test and every flake verbatim here before rerunning.

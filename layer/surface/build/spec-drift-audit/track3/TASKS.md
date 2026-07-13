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
- [x] S1: commit the priority obligation ledger (`c988fb3`).
- [x] S2.1: mandatory peer-binding expiry contract test and spec-ward fix.
- [x] S2.2: operator-pointed egress observation contract test and spec-ward fix.
- [x] S2.x: no additional LAW-LEADS-CODE seams found by S1.
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

### S2.1 expected red — mandatory binding expiry

```text
$ cargo test -p mct-kernel peer::tests::binding_without_expiry_fails_closed -- --nocapture
running 1 test

thread 'peer::tests::binding_without_expiry_fails_closed' panicked at crates/mct-kernel/src/peer/mod.rs:723:9:
assertion failed: serde_json::from_value::<MctPeerBinding>(value).is_err()
test peer::tests::binding_without_expiry_fails_closed ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 75 filtered out
error: test failed, to rerun pass `-p mct-kernel --lib`
```

### S2.1 implementation compile — second current-binding evaluator

```text
$ cargo check --workspace
error[E0599]: no method named `as_ref` found for struct `id::Timestamp` in the current scope
   --> crates/mct-kernel/src/call/internal.rs:280:10
    |
278 |       if binding
279 |           .expires_at
280 |           .as_ref()
    |           -^^^^^^ method not found in `id::Timestamp`
error: could not compile `mct-kernel` (lib) due to 1 previous error
```

### S2.1 implementation compile — daemon projections

```text
$ cargo check --workspace
error[E0599]: no method named `as_ref` found for struct `mct_kernel::Timestamp`
    --> crates/mct-daemon/src/daemon/resident.rs:1940:54
error[E0308]: mismatched types
    --> crates/mct-daemon/src/daemon/resident.rs:2534:25
     |
     | expected `Option<Timestamp>`, found `Timestamp`
error[E0063]: missing field `expires_at` in initializer of `mct_daemon::MctPeerAddressBookEntry`
    --> crates/mct-daemon/src/daemon/ingress.rs:1008:5
error: could not compile `mct-daemon` (bin "mct-daemon") due to 3 previous errors
```

### S2.1 test compilation — mandatory field fixture migration

```text
$ cargo test --workspace --no-run
error[E0308]: mismatched types
    --> crates/mct-kernel/src/call/mod.rs:1251:33
     | expected `Timestamp`, found `Option<_>`
error[E0308]: mismatched types
    --> crates/mct-kernel/src/observation.rs:1209:25
     | expected `Timestamp`, found `Option<_>`
error[E0308]: mismatched types
   --> crates/mct-kernel/src/peer/mod.rs:781:30
     | expected `Timestamp`, found `Option<Timestamp>`
error: could not compile `mct-kernel` (lib test) due to 3 previous errors
```

### S2.1 test compilation — adapter/config fixtures

```text
$ cargo test --workspace --no-run
error[E0308]: mismatched types
   --> crates/mct-iroh/src/identity.rs:267:25
error[E0308]: mismatched types
   --> crates/mct-iroh/src/test_support.rs:303:21
error[E0308]: mismatched types
   --> crates/mct-iroh/src/lib.rs:206:30
error[E0308]: mismatched types
    --> crates/mct-iroh/src/lib.rs:1091:38
error[E0308]: mismatched types
    --> crates/mct-iroh/src/lib.rs:1233:25
error: could not compile `mct-iroh` (lib test) due to 5 previous errors
error[E0063]: missing field `expires_at` in initializer of `config::MctPeerAddressBookEntry`
   --> crates/mct-daemon/src/config.rs:779:9
error[E0308]: mismatched types
   --> crates/mct-daemon/src/fake.rs:240:21
error[E0063]: missing field `expires_at` in initializer of `config::MctPeerAddressBookEntry`
   --> crates/mct-daemon/src/federation.rs:215:13
error[E0063]: missing field `expires_at` in initializer of `config::MctPeerAddressBookEntry`
   --> crates/mct-daemon/src/federation.rs:234:13
error: could not compile `mct-daemon` (lib test) due to 4 previous errors
```

### S2.1 test compilation — binary-local peer fixtures

```text
$ cargo test --workspace --no-run
error[E0308]: mismatched types
    --> crates/mct-daemon/src/daemon/resident.rs:3323:25
error[E0063]: missing field `expires_at` in initializer of `mct_daemon::MctPeerAddressBookEntry`
    --> crates/mct-daemon/src/daemon/resident.rs:3432:26
error[E0063]: missing field `expires_at` in initializer of `mct_daemon::MctPeerAddressBookEntry`
    --> crates/mct-daemon/src/daemon/resident.rs:3618:26
error[E0063]: missing field `expires_at` in initializer of `mct_daemon::MctPeerAddressBookEntry`
    --> crates/mct-daemon/src/daemon/resident.rs:3899:26
error[E0063]: missing field `expires_at` in initializer of `mct_daemon::MctPeerAddressBookEntry`
    --> crates/mct-daemon/src/daemon/resident.rs:4040:26
error[E0063]: missing field `expires_at` in initializer of `mct_daemon::MctPeerAddressBookEntry`
    --> crates/mct-daemon/src/daemon/resident.rs:4083:26
error[E0308]: mismatched types
    --> crates/mct-daemon/src/daemon/resident.rs:4105:33
error[E0063]: missing field `expires_at` in initializer of `mct_daemon::MctPeerAddressBookEntry`
    --> crates/mct-daemon/src/daemon/resident.rs:4420:26
error[E0063]: missing field `expires_at` in initializer of `mct_daemon::MctPeerAddressBookEntry`
    --> crates/mct-daemon/src/daemon/resident.rs:4438:26
error[E0308]: mismatched types
    --> crates/mct-daemon/src/daemon/resident.rs:4462:33
error[E0308]: mismatched types
    --> crates/mct-daemon/src/daemon/resident.rs:4473:33
error[E0063]: missing field `expires_at` in initializer of `mct_daemon::MctPeerAddressBookEntry`
    --> crates/mct-daemon/src/daemon/resident.rs:4659:26
error[E0308]: mismatched types
    --> crates/mct-daemon/src/daemon/resident.rs:5720:13
error[E0308]: mismatched types
    --> crates/mct-daemon/src/daemon/resident.rs:6082:25
error[E0063]: missing field `expires_at` in initializer of `mct_daemon::MctPeerAddressBookEntry`
    --> crates/mct-daemon/src/daemon/resident.rs:6145:9
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 15 previous errors
```

### S2.1 full gate — CLI fixture migration

```text
failures:

---- control::tests::offline_peer_mutation_observes_before_effect_and_fails_on_lock_contention stdout ----
called `Result::unwrap()` on an `Err` value: peers add requires --expires-at <timestamp>

---- control::tests::resident_append_failure_prevents_peer_config_effect stdout ----
assertion `left == right` failed
  left: 400
 right: 500

---- control::tests::live_uds_peer_mutations_are_durable_and_secret_free stdout ----
assertion `left == right` failed: {"error":"peer mutation rejected"}
  left: 400
 right: 200

---- control::tests::resident_apply_failure_records_typed_failure_after_decision stdout ----
assertion `left == right` failed: {"error":"peer mutation rejected"}
  left: 400
 right: 500

---- ingress::tests::standalone_serve_refuses_held_ledger_before_endpoint_bind stdout ----
assertion failed: format!("{error:#}").contains("standalone Iroh serve refused: could not acquire the exclusive observation ledger writer; another Mother may already be serving this node")

---- ingress::tests::standalone_serve_process_persists_hello_and_call_lifecycle stdout ----
called `Result::unwrap()` on an `Err` value: RecvError(())

test result: FAILED. 54 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out
error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

### S2.2 expected red — operator-pointed egress observation

```text
$ cargo test -p mct-daemon --bin mct-daemon ingress::tests::operator_pointed_egress_is_durable_before_send -- --nocapture
running 1 test

thread 'ingress::tests::operator_pointed_egress_is_durable_before_send' panicked at crates/mct-daemon/src/daemon/ingress.rs:1146:9:
assertion failed: observed_before_send.load(Ordering::SeqCst)
test ingress::tests::operator_pointed_egress_is_durable_before_send ... FAILED

failures:
    ingress::tests::operator_pointed_egress_is_durable_before_send

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 60 filtered out
error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

## Completed validation

### S2.1 mandatory binding expiry

- `cargo test --workspace`: 286 passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `./scripts/ci-tier0.sh`: clean, including both Allium laws.
- `git diff --check`: clean.

### S2.2 operator-pointed egress observation

- `cargo test --workspace`: 287 passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `./scripts/ci-tier0.sh`: clean, including both Allium laws.
- `git diff --check`: clean.

### S3 candidacy conjunction and publication freshness

- Added `eligible_route_candidate_requires_every_current_conjunct` and `capability_offer_lapses_at_freshness_boundary`.
- `cargo test --workspace`: 289 passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `./scripts/ci-tier0.sh`: clean, including both Allium laws.
- `git diff --check`: clean.

### S3 honest local execution publication

- Strengthened and renamed coverage as `honest_local_execution_offer_excludes_approved_assigned_non_ready_child`.
- `cargo test --workspace`: 289 passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `./scripts/ci-tier0.sh`: clean, including both Allium laws.
- `git diff --check`: clean.

### S3 per-hop upstream identity

- Added `forwarded_envelope_clears_upstream_user_identity`.
- `cargo test --workspace`: 290 passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `./scripts/ci-tier0.sh`: clean, including both Allium laws.
- `git diff --check`: clean.

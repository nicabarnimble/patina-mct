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
- [x] S3: fill every priority GAP or explicitly defer it with reason.
- [x] Final validation and report; no PR, merge, or further push.

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

### S3 replay authority lifecycle expectation

```text
$ cargo test -p mct-daemon --bin mct-daemon resident_mother_payload_roundtrip_verifies_result_digest -- --nocapture
thread 'resident::tests::resident_mother_payload_roundtrip_verifies_result_digest' panicked at crates/mct-daemon/src/daemon/resident.rs:4867:9:
assertion `left == right` failed
  left: [PeerCallReceived, CallConstructed, CallAuthorized, RouteRevalidated, RouteSelected, RouteRevalidated, RouteRevalidated, ResultRecorded, PeerCallReplied, PeerCallReceived, CallConstructed, CallDenied, ResultRecorded, PeerCallReplied, PeerCallReceived, CallConstructed, CallDenied, ResultRecorded, PeerCallReplied, PeerCallReceived, CallConstructed, CallDenied, ResultRecorded, PeerCallReplied]
 right: [PeerCallReceived, CallConstructed, CallAuthorized, RouteRevalidated, RouteSelected, RouteRevalidated, RouteRevalidated, ResultRecorded, PeerCallReplied, PeerCallReceived, CallConstructed, CallDenied, ResultRecorded, PeerCallReplied]
test resident::tests::resident_mother_payload_roundtrip_verifies_result_digest ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 63 filtered out
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

### S3 current authority before replay

- Strengthened `resident_mother_payload_roundtrip_verifies_result_digest` with a keyed success followed by expiry-, Vision-, and revocation-denied retries.
- Narrowed-ALPN replay is explicitly deferred in the ledger because persisted peer bindings have no configurable ALPN scope; call-time narrowing remains covered.
- `cargo test --workspace`: 290 passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `./scripts/ci-tier0.sh`: clean, including both Allium laws.
- `git diff --check`: clean.

## Slice result

- One authorized push completed at S0: `502defd` to `origin/patina`; no later commit was pushed.
- Ledger terminal state: 73 COVERED, 0 GAP, 0 LAW-LEADS-CODE, 3 DEFERRED.
- Mandatory binding expiry and operator-pointed egress both moved from law-leads-code to enforced, named contract tests.
- S3 added or strengthened candidacy-conjunction, freshness-boundary, publication-honesty, per-hop identity, and replay-authority coverage.
- ROADMAP candidate: configurable peer-binding ALPN scope is required before narrowed-ALPN replay can be exercised through the persisted resident authority path.
- No PR was opened or updated; nothing was merged.

# Track 3 slice 2 — full contract ledger extension

## Operator scope

Extend the existing slice-1 ledger without restructuring it so every named invariant in both Allium laws has an honest row, then resolve only bounded gaps through real paths and stage-local helpers. Preserve monotonic tests from 291, add no mct-daemon library public surface, cite every new test in the same commit, and validate every commit with workspace tests, Clippy with warnings denied, Tier 0, and diff hygiene. No push, PR, or merge.

## Slice 2 checklist

- [x] S0: exact clean `patina` baseline at `e73704f`, synchronized with `origin/patina` and `origin/main`.
- [x] S0: operator baseline re-established at 291 passed, 0 ignored.
- [x] S0: read both Allium laws in full, including all 223 `@invariant` declarations and 32 load-bearing Decision clusters.
- [x] S0: read the slice-1 ledger conventions and resident-decomposition close-out/itch list.
- [x] S0: rerun and record Allium plan/model obligation categories for both laws.
- [x] S1: extend the ledger to every named invariant and bulk structural obligation.
- [x] S2.1: resolve required external fixture compatibility disposition.
- [x] S2.2: Option 1 adjudicated — preserve cancelled through protocol evaluation, wire reply, replay, and observations.
- [x] S2.3: resolve complete child-lifecycle observation matrix gaps.
- [ ] S2.4: resolve bounded resident observation buffering gap.
- [ ] S2.5: resolve typed toy-grant expiry/revocation observation gap.
- [ ] S2.x: end every remaining GAP as COVERED, DEFERRED with reason, or STOP for adjudication.
- [ ] S3: close the ledger extension with final counts, running test record, drift disposition, REPORT/ROADMAP updates, and final validation.

## Slice 2 Allium inventory

- `mct-product-map.allium`: 188 named contract invariants; 217 `-- Decision:` statements grouped into 26 adjacent load-bearing clusters.
- `mct-peer-ontology.allium`: 35 named contract invariants; 19 `-- Decision:` statements grouped into 6 adjacent load-bearing clusters.
- `allium plan layer/allium/mct-product-map.allium`: 179 structural obligations.
  - `entity_fields`: 56
  - `entity_optional`: 38
  - `surface_actor`: 27
  - `surface_exposure`: 27
  - `value_equality`: 29
  - `when_presence`: 2
- `allium model layer/allium/mct-product-map.allium`: 27 entities and 29 value types.
- `allium plan layer/allium/mct-peer-ontology.allium`: zero structural obligations.
- `allium model layer/allium/mct-peer-ontology.allium`: zero entities and zero value types.
- The CLI still does not emit prose contract invariants; all 223 were inventoried directly from the law files.

## Slice 2 running test count

| Commit / boundary | Tests added | Running passed + ignored | Result |
|---|---:|---:|---|
| Baseline `e73704f` | — | 291 | 291 passed, 0 ignored |
| S1 `6f574c4` | 0 | 291 | 291 passed, 0 ignored |
| S2.1 `6c687da` | 0 | 291 | 291 passed, 0 ignored |
| S2.2 STOP record `01b470b` | 0 | 291 | 291 passed, 0 ignored |
| S2.2 fix `8565636` | 3 | 294 | 294 passed, 0 ignored |
| S2.2 disposition `342cabc` | 0 | 294 | 294 passed, 0 ignored |
| S2.3 `fix(control): complete child lifecycle observations` | 2 | 296 | pending commit validation |

## Slice 2 S1 inventory result

- Named invariant universe: 223/223 attributed.
- Structural plan universe: 179/179 attributed through 14 existing bulk rows; the companion emits none.
- Citation audit: 125 distinct named test citations resolve; no stale resident paths.
- Initial full-universe disposition: 195 COVERED, 6 GAP, 0 LAW-LEADS-CODE, 22 DEFERRED.
- The absence of a new LAW-LEADS-CODE row was checked against the implementation rather than assumed. The newly exposed unresolved obligations are missing complete named evidence over landed behavior (GAP) or explicitly unbuilt/governance-only surfaces (DEFERRED), not observed contradictory execution.

## Slice 2 S2 triage record

- `ExternalChildCompatibility.RequiredFixturesDoNotRegress`: converted GAP → DEFERRED. The repository contains a real `slate-manager` invocation path but not the versioned `folder-watch-actor` and `watch-null-sink` artifacts; generated lookalikes would prove only the generic loader and would overstate external compatibility.
- `MctObservationSubsystemCoverage.ResultCoverage`: real-path triage exposed a structural LAW-LEADS-CODE mismatch instead of a missing matrix. `result_to_call_handler_result` maps `ResultOutcome::Cancelled` to `MctIrohCallHandlerResult::failed`, so the downstream result observation is failed rather than cancelled.
- `MctResultTerminality.ClosedOutcomeSet`: also moved COVERED → LAW-LEADS-CODE. The existing helper test proves route presence for all five result variants but did not prove actual result-consumer projection.
- Structural stop (resolved): `MctCallProtocolReply` and `ResultOutcome` included cancellation, but `MctCallProtocolEvaluation.outcome` in the law and `CallProtocolOutcome` in Rust did not.
- Operator adjudication selected Option 1. The protocol evaluation model now carries `cancelled`; resident projection, wire reply route suppression, durable idempotent replay, and buffered/before-effect observations preserve it end-to-end. Options 2 and 3 were rejected because projection indirection would hide the model gap, while failure collapse would violate route projection and replay semantics.
- `MctChildComponentLifecycle.LifecycleTransitionsAreObserved` and `MctObservationSubsystemCoverage.ChildLifecycleCoverage`: the full authority/instance matrix was testable with a small kernel-local fixture and is now covered. The real strict registry-sync test caught an additional spec-ward drift: failed artifact loads were not projected because the decision batch iterated only successfully loaded children. The small fix adds typed before-effect `ArtifactRejected` facts without exposing failure detail or adding public surface.

## Slice 2 failure and flake log

Capture every failure verbatim here before rerunning.

### S2.2 expected red — cancelled result projection

```text
$ cargo test -p mct-daemon --bin mct-daemon resident::execution::tests::cancelled_result_projection_preserves_cancelled_outcome -- --nocapture
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.74s
     Running unittests src/main.rs (target/debug/deps/mct_daemon-701d058281c133f0)

running 1 test

thread 'resident::execution::tests::cancelled_result_projection_preserves_cancelled_outcome' (1619908) panicked at crates/mct-daemon/src/daemon/resident/execution.rs:957:9:
assertion `left != right` failed
  left: Failed
 right: Failed
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test resident::execution::tests::cancelled_result_projection_preserves_cancelled_outcome ... FAILED

failures:

failures:
    resident::execution::tests::cancelled_result_projection_preserves_cancelled_outcome

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 65 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

### S2.2 adjudicated expected red — restored cancelled result probe

```text
$ cargo test -p mct-daemon --bin mct-daemon resident::execution::tests::cancelled_result_projection_preserves_cancelled_outcome -- --nocapture
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.17s
     Running unittests src/main.rs (target/debug/deps/mct_daemon-701d058281c133f0)

running 1 test

thread 'resident::execution::tests::cancelled_result_projection_preserves_cancelled_outcome' (1632652) panicked at crates/mct-daemon/src/daemon/resident/execution.rs:957:9:
assertion `left != right` failed
  left: Failed
 right: Failed
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test resident::execution::tests::cancelled_result_projection_preserves_cancelled_outcome ... FAILED

failures:

failures:
    resident::execution::tests::cancelled_result_projection_preserves_cancelled_outcome

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 65 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

### S2.2 cancelled observation test Clippy failure

```text
$ cargo clippy --workspace --all-targets -- -D warnings
    Checking mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error: this `MutexGuard` is held across an await point
   --> crates/mct-iroh/src/lib.rs:642:13
    |
642 |         let batches = batches.lock().unwrap();
    |             ^^^^^^^
    |
    = help: consider using an async-aware `Mutex` type or ensuring the `MutexGuard` is dropped before calling `await`
note: these are all the await points this lock is held through
   --> crates/mct-iroh/src/lib.rs:686:24
    |
686 |         client.close().await;
    |                        ^^^^^
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#await_holding_lock
    = note: `-D clippy::await-holding-lock` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::await_holding_lock)]`

error: could not compile `mct-iroh` (lib test) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
```

### S2.3 artifact rejection observation expectation

```text
$ cargo test -p mct-daemon --bin mct-daemon control::tests::live_registry_sync_observes_artifact_rejection_before_state_effect -- --nocapture
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.23s
     Running unittests src/main.rs (target/debug/deps/mct_daemon-701d058281c133f0)

running 1 test

thread 'control::tests::live_registry_sync_observes_artifact_rejection_before_state_effect' (1653992) panicked at crates/mct-daemon/src/daemon/control.rs:2861:14:
called `Option::unwrap()` on a `None` value
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test control::tests::live_registry_sync_observes_artifact_rejection_before_state_effect ... FAILED

failures:

failures:
    control::tests::live_registry_sync_observes_artifact_rejection_before_state_effect

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 67 filtered out; finished in 0.03s

error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

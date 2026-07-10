# Track 1 — implementation hardening

## Operator prompt (verbatim)

```text
You are starting Track 1 of the spec-drift remediation in patina-mct:
implementation hardening, sliced small. This slice covers tooling
finding T1 and Class A finding A2 from
layer/surface/build/spec-drift-audit/REPORT.md. The adjudication
directions are pinned: spec-ward = preserve the spec, fix code;
code-ward = preserve landed behavior, update the map; elicitation =
decide intent before changing either. A2 is spec-ward: the product
map's contract stands and the code must honor it. There is no operator
gate mid-slice — the map text quoted below IS the contract; if the
mechanism SPEC uncovers a genuine design fork the map does not answer,
STOP and report instead of choosing.

## Step 0 — Re-establish state (STOP and report if anything differs)

a) Branch `patina`, expected HEAD aad2322 (docs: report spec drift
   audit). Tree clean. Commit any pending session artifacts via your
   normal flow first.
b) Read: layer/surface/build/spec-drift-audit/REPORT.md (findings T1
   and A2 with their spec/code citations),
   layer/allium/mct-product-map.allium lines 131 and 146-147
   (execution revalidation) and 692-698 plus 826-840
   (HelloDoesNotPreAuthorizeCall and peer-call authority),
   layer/core/dependable-rust.md.
c) Code: crates/mct-iroh/src/serve.rs:83-114 (per-connection binding
   load), 818-820 and 880-939 (call branch using only the remembered
   hello); crates/mct-kernel/src/call/internal.rs:148-227 (call
   evaluation inputs); how hello admission checks binding
   active/expiry/revision today, for reuse.

## Working principles (binding)

Favor strong invariants over defensive fallbacks. Make bad states
impossible where practical. Do not add complexity to paper over
unclear design. Prefer simple data models, explicit contracts, and
shared logic over local patches, duplicated code, or speculative
abstractions. Write Rust code that Jon Gjengset would agree with.
Always read code before writing code. Git update with scalpel as you
work, not with shotgun after. Kernel decides, adapters perform: the
adapter supplies current binding facts and current time; the kernel
evaluates. Fail closed. Failing test first for behavior changes. Each
slice lands its regression tests with the fix, not later. Stop at a
task boundary if context runs low.

Validation green after EVERY commit:
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
Flake protocol: capture any failure verbatim in the slice TASKS.md
before rerunning.

## Task S1.0 — Housekeeping: T1 pin bump (one commit: `chore: pin allium 3.5.0 in CI`)

Bump ALLIUM_VERSION in scripts/install-allium-ci.sh to 3.5.0 and
update the matching release-asset checksum from the official
juxt/allium-tools v3.5.0 release. Verify ./scripts/ci-tier0.sh passes
with the local 3.5.0. Create
layer/surface/build/spec-drift-audit/track1/TASKS.md with this prompt
verbatim and a slice checklist; commit together.

## Task S1.1 — Mechanism SPEC (short; no gate)

Add a short SPEC section to track1/TASKS.md (or SPEC.md beside it)
deciding the A2 mechanism within the map's fixed contract:
- What "current binding authority" means per call: the adapter loads
  current peer bindings and current validated time for each mct/call/0
  evaluation; the kernel receives them as facts alongside the
  remembered hello and re-checks binding active state, expiry, revoked
  status, Vision/ALPN admission scope, and policy revision currency.
- Where the check lives: extend the existing kernel call evaluation
  inputs (preferred — one decision path) vs a separate pre-check.
  Justify in two sentences; do not build a parallel evaluator.
- The typed outcome for each staleness class (revoked, expired,
  revision-changed, scope-narrowed) and the caller-safe projection
  (existing `not authorized` convention).
- Explicitly out of scope: observation ordering/durability (slice 2,
  finding A5), idempotency (A3), lifecycle (A7).

## Task S1.2 — Implement + regression tests

Failing tests first. Minimum coverage, all through the real serve/call
path:
- binding revoked after admitted hello → subsequent call denied with
  the typed revoked reason, observed, never executed;
- binding expired between hello and call (controllable time source —
  if the evaluation path lacks a time seam, add the injection point
  rather than sleeping) → denied;
- policy revision bumped after hello → denied stale;
- unchanged active binding → calls continue to succeed (no
  regression), including the existing two-Mother suites;
- forwarding path: a forwarding Mother whose outbound relationship is
  revoked mid-flight fails closed when calling the executor.
Update the REPORT.md summary row for A2 with outcome `fixed` and the
commit hash; tick the slice checklist. Suggested commits:
`fix(kernel): evaluate calls against current binding authority` and
`test(iroh): cover stale binding denial paths` — scalpel as the work
dictates.

## Boundary

STOP after S1.2. Slices 2-6 (A5+A6-peer, A8, A6-remainder, A7, A3)
are separate prompts. Final report: commits, validation results,
flake log, which staleness classes are covered by which test, and
anything discovered that belongs in the audit report or ROADMAP.
Branch discipline: stay on `patina`; do not merge or rebase anything
from specification-track branches mid-slice.
```

## Slice checklist

- [x] Step 0 baseline matches `patina` at `aad2322` with a clean tree.
- [x] Read the audit findings, fixed product-map contracts, dependable Rust guidance, and current hello/call evaluation paths.
- [x] S1.0: verify the official v3.5.0 Linux x86_64 release checksum.
- [x] S1.0: pin Allium 3.5.0 and its checksum in CI.
- [x] S1.0: run tier-0, commit, and run the required post-commit validation.
- [x] S1.1: record the A2 mechanism specification.
- [x] S1.2: land failing real-path regression tests before the behavior fix.
- [x] S1.2: implement current-binding call revalidation in the kernel decision path.
- [x] S1.2: cover revoked, expired, stale-revision, narrowed-scope, unchanged, and forwarding paths.
- [x] S1.2: update A2 in the audit report with `fixed` and the implementation commit.
- [x] S1.2: complete per-commit validation and stop.

## S1.1 mechanism specification — current binding authority per call

### Fixed contract

Every `mct/call/0` evaluation rechecks the remembered hello against current peer authority. The adapter supplies the current peer-binding snapshot, the current local policy revision, and a validated current timestamp for that call connection; the remembered hello retains the binding policy revision it admitted and remains necessary evidence, but is not sufficient current authority.

The kernel selects the current binding by the binding ID and authenticated endpoint recorded by the request/hello chain, then requires the binding to remain admitted, unexpired, unchanged from the hello-admitted revision, current under local policy, and scoped to the same node, Vision, and `mct/call/0` ALPN. The call handler is unreachable when any current-binding check denies.

### Decision seam

Extend the existing `evaluate_call_protocol` inputs with an explicit `CallEvaluationContext` carrying IDs, current bindings, current policy revision, and current time. Keep every denial in the existing call evaluator so protocol callers cannot accidentally perform a stale hello-only evaluation; do not add a separate adapter pre-check or parallel evaluator.

This keeps the adapter responsible for loading config/time facts and the kernel responsible for the one typed authority decision. It also makes omission of current binding authority unrepresentable at the public call-evaluation seam.

### Typed outcomes

| Current-binding condition | `CallProtocolReason` | Caller-safe message |
|---|---|---|
| Binding state is `Revoked` | `BindingRevoked` | `not authorized` |
| Binding state is `Expired`, or `expires_at <= now` | `BindingExpired` | `not authorized` |
| Binding is absent/denied/pending or no longer matches the admitted binding and endpoint | `BindingMismatch` | `not authorized` |
| Current binding or call authority revision is older than the current local policy revision | `PolicyRevisionStale` | `not authorized` |
| Current binding no longer grants the admitted node | `CallerMismatch` | `not authorized` |
| Current binding no longer grants the admitted Vision | `VisionMismatch` | `not authorized` |
| Current binding no longer grants `mct/call/0` | `AlpnNotAdmitted` | `not authorized` |

A binding revision greater than the call/remembered authority is also stale for that call: policy changes do not silently widen an old admission. Unchanged active authority continues through the existing payload and routing checks.

### Out of scope

- Observation ordering and durability (slice 2, A5).
- Idempotency and replay semantics (A3).
- Child replacement lifecycle (A7).
- Peer-ontology changes from Track 2.

## Validation record

- `ce42258` (`chore: pin allium 3.5.0 in CI`): required post-commit workspace tests, Clippy `-D warnings`, and tier-0 passed.
- `5f8f1af` (`fix(kernel): evaluate calls against current binding authority`): required post-commit workspace tests, Clippy `-D warnings`, and tier-0 passed.
- `test(iroh): cover forwarding-time binding revocation`: required post-commit workspace tests, Clippy `-D warnings`, and tier-0 passed.

## Staleness coverage

| Class | Regression evidence |
|---|---|
| Revoked after hello | `call_rechecks_binding_revocation_after_hello`; `two_mother_forwarding_denies_when_executor_revokes_binding_after_hello` |
| Expired by current time | `call_rechecks_binding_expiry_after_hello` with injected time |
| Policy revision changed | `call_rechecks_binding_policy_revision_after_hello`; `newer_call_claim_cannot_reuse_hello_admitted_at_an_older_policy_revision` |
| ALPN scope narrowed | `call_rechecks_narrowed_alpn_scope_after_hello` |
| Vision scope narrowed | `call_rechecks_narrowed_vision_scope_after_hello` |
| Unchanged active binding | Existing local Iroh roundtrips, concurrent peer isolation, resident Mother, and two-Mother forwarding suites |

## Flake log

No flakes. Expected red phase captured before the behavior fix:

```text
$ cargo test -p mct-iroh call_rechecks_ -- --nocapture
   Compiling mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 6.45s
     Running unittests src/lib.rs (target/debug/deps/mct_iroh-c905e0248aa84c8b)

running 4 tests

thread 'tests::call_rechecks_binding_expiry_after_hello' (602639) panicked at crates/mct-iroh/src/lib.rs:812:9:
assertion `left == right` failed
  left: Success
 right: Denied
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

thread 'tests::call_rechecks_binding_revocation_after_hello' (602641) panicked at crates/mct-iroh/src/lib.rs:812:9:
assertion `left == right` failed
  left: Success
 right: Denied

thread 'tests::call_rechecks_binding_policy_revision_after_hello' (602640) panicked at crates/mct-iroh/src/lib.rs:812:9:
assertion `left == right` failed
  left: Success
 right: Denied

thread 'tests::call_rechecks_narrowed_alpn_scope_after_hello' (602642) panicked at crates/mct-iroh/src/lib.rs:812:9:
assertion `left == right` failed
  left: Success
 right: Denied
error: test failed, to rerun pass `-p mct-iroh --lib`
test tests::call_rechecks_binding_revocation_after_hello ... FAILED
test tests::call_rechecks_binding_expiry_after_hello ... FAILED
test tests::call_rechecks_binding_policy_revision_after_hello ... FAILED
test tests::call_rechecks_narrowed_alpn_scope_after_hello ... FAILED

failures:

failures:
    tests::call_rechecks_binding_expiry_after_hello
    tests::call_rechecks_binding_policy_revision_after_hello
    tests::call_rechecks_binding_revocation_after_hello
    tests::call_rechecks_narrowed_alpn_scope_after_hello

test result: FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 26 filtered out; finished in 3.33s
```

Implementation compile failure captured before correction:

```text
$ cargo check --workspace
    Checking mct-kernel v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-kernel)
    Checking mct-observation v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-observation)
    Checking mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
error[E0422]: cannot find struct, variant or union type `MctPeerAuthoritySnapshot` in this scope
   --> crates/mct-iroh/src/serve.rs:715:24
    |
715 |                     Ok(MctPeerAuthoritySnapshot {
    |                        ^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find type `MctPeerAuthoritySnapshot` in this scope
   --> crates/mct-iroh/src/serve.rs:743:63
    |
743 |         BindingsFut: Future<Output = MotherIrohEndpointResult<MctPeerAuthoritySnapshot>>
    |                                                               ^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope
    |
help: you might be missing a type parameter
    |
612 | impl<MctPeerAuthoritySnapshot> MotherIrohEndpoint {
    |     ++++++++++++++++++++++++++

error[E0422]: cannot find struct, variant or union type `CallEvaluationContext` in this scope
   --> crates/mct-iroh/src/serve.rs:923:41
    |
923 | ...                   CallEvaluationContext {
    |                       ^^^^^^^^^^^^^^^^^^^^^
    |
   ::: crates/mct-kernel/src/peer/mod.rs:479:1
    |
479 | pub struct HelloEvaluationContext {
    | --------------------------------- similarly named struct `HelloEvaluationContext` defined here
    |
help: a struct with a similar name exists
    |
923 -                                         CallEvaluationContext {
923 +                                         HelloEvaluationContext {
    |

error[E0422]: cannot find struct, variant or union type `CallEvaluationContext` in this scope
    --> crates/mct-iroh/src/serve.rs:1214:29
     |
1214 | ...                   CallEvaluationContext {
     |                       ^^^^^^^^^^^^^^^^^^^^^
     |
    ::: crates/mct-kernel/src/peer/mod.rs:479:1
     |
 479 | pub struct HelloEvaluationContext {
     | --------------------------------- similarly named struct `HelloEvaluationContext` defined here
     |
help: a struct with a similar name exists
     |
1214 -                             CallEvaluationContext {
1214 +                             HelloEvaluationContext {
     |

error[E0422]: cannot find struct, variant or union type `MctPeerAuthoritySnapshot` in this scope
    --> crates/mct-iroh/src/serve.rs:1219:57
     |
1219 | ...                   current_peer_authority: MctPeerAuthoritySnapshot {
     |                                               ^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope

Some errors have detailed explanations: E0422, E0425.
For more information about an error, try `rustc --explain E0422`.
error: could not compile `mct-iroh` (lib) due to 5 previous errors
```

Clippy compile failure captured before correction:

```text
$ cargo clippy --workspace --all-targets -- -D warnings
    Checking mct-kernel v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-kernel)
    Checking mct-observation v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-observation)
    Checking mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0382]: borrow of moved value: `binding`
   --> crates/mct-daemon/src/fake.rs:130:32
    |
 48 |     let binding = fake_binding();
    |         ------- move occurs because `binding` has type `mct_kernel::MctPeerBinding`, which does not implement the `Copy` trait
...
 68 |         &[binding],
    |           ------- value moved here
...
130 |                 bindings: vec![binding.clone()],
    |                                ^^^^^^^ value borrowed here after move
    |
help: consider cloning the value if the performance cost is acceptable
    |
 68 |         &[binding.clone()],
    |                  ++++++++

For more information about this error, try `rustc --explain E0382`.
error: could not compile `mct-daemon` (lib test) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
```

---

# Slice 2 — peer-authority observation durability

## Operator prompt (verbatim)

```text
Track 1 slice 2 of the spec-drift remediation in patina-mct:
observation durability for peer authority. This slice covers finding
A5 and the PEER-AUTHORITY portion of A6 from
layer/surface/build/spec-drift-audit/REPORT.md. Both are spec-ward:
the product map's contract stands and the code must honor it. The
binding contract: hello receipt/admission/denial facts are durable
BEFORE subsequent protected peer effects proceed
(HelloObservationsBeforeEffects, map lines 563-590;
AuthorityFactsAreDurableBeforeEffect, lines 1319 and 1406-1407), and
every peer authority mutation produces a typed observation
(AuthorityDecisionsAreObserved, lines 1135-1142). No mid-slice gate;
if the mechanism SPEC uncovers a design fork the map does not answer —
in particular, if durable-before-effect cannot be achieved without
redesigning the single-writer ledger — STOP and report options
instead of choosing.

## Step 0 — Re-establish state (STOP and report if anything differs)

a) Branch `patina`, expected HEAD 2f540ca (test(iroh): cover
   forwarding-time binding revocation). Commit pending session
   artifacts via your normal flow first; tree otherwise clean.
b) Read: REPORT.md findings A5 and A6 with citations; the map lines
   above plus the observation matrix (1223-1235);
   layer/surface/build/spec-drift-audit/track1/TASKS.md (slice 1
   record); layer/core/dependable-rust.md.
c) Code: crates/mct-iroh/src/serve.rs:833-878 and 987-1010 (hello
   remembered, response finished, Served emitted after close);
   crates/mct-daemon/src/main.rs:1529-1563 (async Served-to-ledger
   projection); the peer mutation paths at
   crates/mct-daemon/src/main.rs:4500-4597 and 4625-4665 (add, proof,
   revoke, remove — config writes with no ledger append); the JSONL
   ledger writer and its single-writer/locking model in
   mct-observation (including open_read_only added in the audit arc);
   how the resident daemon currently owns the ledger handle.

## Working principles (binding)

Favor strong invariants over defensive fallbacks. Make bad states
impossible where practical. Do not add complexity to paper over
unclear design. Prefer simple data models, explicit contracts, and
shared logic over local patches, duplicated code, or speculative
abstractions. Write Rust code that Jon Gjengset would agree with.
Always read code before writing code. Git update with scalpel as you
work, not with shotgun after. Kernel decides, adapters perform:
observation append is an adapter effect; fail closed — if the
authority fact cannot be made durable, the effect it licenses must
not proceed. Failing test first. Each slice lands its regression
tests with the fix. Stop at a task boundary if context runs low.

Validation green after EVERY commit:
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
Flake protocol: capture failures verbatim in track1/TASKS.md before
rerunning.

## Task S2.1 — Mechanism SPEC (short; no gate)

Append a slice-2 SPEC section to track1/TASKS.md deciding:
- How the serve path achieves durable-before-effect for hello
  admission AND denial facts: synchronous append via a shared ledger
  handle/callback passed into serve, vs a write-ahead acknowledgment
  from the daemon, vs another shape the code supports. Weigh against
  the single-writer lock model; justify briefly. The ordering rule to
  satisfy: the hello observation is durable before the hello RESPONSE
  is sent (the response is the first protected effect — it grants the
  peer an admission it can immediately use).
- Failure semantics: ledger append fails → the hello response is not
  sent and the admission is not remembered; state the connection
  outcome and what, if anything, is retried. Never
  observe-after-effect as a fallback.
- Peer authority mutations (add, proof update, revoke, remove): each
  produces a typed observation appended durably BEFORE the command
  reports success; state the ObservationKind mapping (reuse existing
  kinds where they exist) and what facts each records (binding id,
  endpoint, revision, no secrets).
- Explicitly out of scope: full peer-call lifecycle coverage
  including malformed requests (A8, slice 3); child/operator/storage
  observation gaps (A6 remainder, slice 4).

## Task S2.2 — Implement + regression tests

Failing tests first. Minimum coverage:
- ordering: after a completed hello, the admission observation is
  already durable in the JSONL ledger at the moment the client holds
  the response (assert via read-only ledger access in the test);
  same for a denied hello;
- fail-closed: with ledger append failure injected (use or add a
  seam; no sleeping/racing), the hello response is never sent, no
  admission is remembered, and a subsequent call on that connection
  is denied for missing hello;
- peer mutations: add, proof update, revoke, and remove each append
  their typed observation before command success, asserted against
  the ledger file; revoke's observation composes with slice 1 — the
  revocation fact is in the ledger by the time calls start being
  denied;
- no payload bytes and no key material in any new observation
  (extend the existing no-bytes assertions to the new kinds);
- existing suites stay green, including two-Mother forwarding.
Update REPORT.md: A5 row → fixed with commit hash; A6 row → note
"peer-authority portion fixed in <hash>; child/operator/storage
remainder open (slice 4)". Tick the slice checklist.

## Boundary

STOP after S2.2. Slices 3-6 are separate prompts. Final report:
commits, validation, flake log, the mechanism chosen for
durable-before-effect and why, and anything discovered for the audit
report or ROADMAP. Stay on `patina`; no merges from other branches
mid-slice.
```

## Slice 2 checklist

- [x] Step 0 baseline matched `patina` at `2f540ca` with a clean tree.
- [x] Read A5/A6, the fixed product-map contracts, slice-1 record, dependable Rust guidance, serve/event ordering, peer mutation commands, and ledger locking.
- [x] Specify the supported hello write-ahead mechanism and failure semantics.
- [x] Identify the live peer-mutation/single-writer design fork required by the stop condition.
- [x] Operator selects the live mutation path.
- [x] Implement S2.2 and mark A5 fixed; defer A6 peer mutations to slice 2b and the remainder to slice 4.

## S2.1 mechanism specification and stop finding

### Hello ordering — supported without ledger redesign

Use an awaited daemon-supplied write-ahead callback in the concurrent Iroh serve path. The callback projects the typed hello evaluation into `MctObservation`, sends it through the existing cloned `ResidentLedgerWriter`, and awaits the writer acknowledgment; that acknowledgment is issued only after `append_batch_before_effect` writes and `sync_data` completes. This preserves the single JSONL writer and keeps observation storage in the daemon adapter rather than moving it into `mct-iroh`.

For each hello, the server first clears any remembered admission for that endpoint, evaluates the hello, constructs the response facts, and invokes the callback before remembering a newly admitted hello or writing response bytes. Callback failure terminates that hello stream without a response, leaves no admission remembered, and is not retried or downgraded to observe-after-effect. A later `mct/call/0` connection from the endpoint is evaluated without remembered admission and receives the existing missing-hello denial.

### Peer mutation observation mapping

The existing taxonomy can represent the required facts without key material:

| Command | Observation kind | Recorded facts |
|---|---|---|
| `peers add` | `PeerBindingRecorded` | binding ID, peer endpoint ID, policy revision, admitted state, peer node/Vision in safe detail |
| `peers set-outbound-proof` | `PeerBindingRecorded` | outbound binding ID, peer endpoint ID, policy revision, expiry presence/time, action label; never the signature |
| `peers revoke` | `PeerBindingRevoked` | binding ID, peer endpoint ID, policy revision, revoked state |
| `peers remove` | `PeerBindingRevoked` | final binding ID, peer endpoint ID, policy revision, removal action/tombstone |

Authority-expanding add/proof decisions must be durable before config publication. Revocation/removal should first persist a revoked tombstone (fail closed), append the typed fact, and only then report success or remove the tombstone; this avoids restoring authority when observation storage is unavailable.

### Blocking design fork — live commands cannot reach the resident writer

The resident daemon owns `JsonlObservationLedger` through `ResidentLedgerWriter`; the OS file lock intentionally rejects a second writer. The four `mct-daemon peers ...` commands are separate short-lived processes that mutate the shared config directly and currently have no IPC route to the resident-owned writer. Opening the JSONL ledger in those commands would therefore either make all live peer mutations fail while Mother is running or require weakening/redesigning the single-writer invariant.

This matters functionally, not only operationally: slice 1 deliberately reloads peer config for every call so a live `peers revoke` can deny subsequent calls. An offline-only observation implementation would remove that live revocation behavior, while direct multi-writer append would violate the ledger contract.

The map fixes durability but does not choose how external peer commands reach the resident authority/ledger owner. S2.2 therefore stops pending one of these operator choices:

1. **Resident control-plane mutation (recommended architecture, not selected):** add authenticated add/proof/revoke/remove control operations; the resident appends through its writer and mutates config in the ordered command handler. CLI commands become control clients while Mother is active.
2. **Dedicated ledger/authority IPC:** expose a narrow append-and-ack service owned by the resident, while leaving config mutation in the CLI process. This preserves one writer but introduces cross-process ordering/partial-failure coordination.
3. **Offline-only peer mutation:** have peer commands acquire the JSONL writer lock and fail closed when the resident is active. This is smallest but removes live revocation and conflicts with the slice-1 operational shape.
4. **Multi-writer ledger redesign:** coordinate file append/hash-chain state across processes. This is the widest and riskiest option and directly crosses the prompt's stop condition.

### Out of scope

- Full peer-call lifecycle and malformed-call observations (A8, slice 3).
- Child/operator/storage observation gaps (A6 remainder, slice 4).
- Changes to the single-writer ledger before operator selection.

## S2.1 operator decision

Option 1 is selected: peer authority mutations become resident control-plane operations and the CLI becomes a control client. The existing local UDS already accepts mutations while HTTP remains read-only, and the resident Mother is the authority owner for one node. When Mother is not running, the CLI may acquire the free JSONL writer lock and perform the mutation with its durable typed observation directly. The existing lock arbitrates the two paths without weakening the single-writer invariant.

Options 2–4 are rejected: dedicated append IPC leaves cross-process mutation/observation partial-failure windows; offline-only administration removes live revocation; and multi-writer ledger access breaks the chosen single-writer shape. Implementation is deferred to slice 2b. Slice 2 completes A5 only.

## Slice 2 failure log

Expected failing-test compile output before the hello observation seam existed:

```text
   Compiling mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
error[E0560]: struct `serve::MctIrohConcurrentServeConfig` has no field named `hello_observation_sink`
   --> crates/mct-iroh/src/lib.rs:520:25
    |
520 |                         hello_observation_sink: Some(MctIrohHelloObservationSink::new(
    |                         ^^^^^^^^^^^^^^^^^^^^^^ `serve::MctIrohConcurrentServeConfig` does not have this field
    |
    = note: available fields are: `max_concurrent_connections`, `connection_timeout`, `require_binding_signature`, `capability_view`

error[E0433]: cannot find type `MctIrohHelloObservationSink` in this scope
   --> crates/mct-iroh/src/lib.rs:520:54
    |
520 |                         hello_observation_sink: Some(MctIrohHelloObservationSink::new(
    |                                                      ^^^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohHelloObservationSink`

Some errors have detailed explanations: E0433, E0560.
For more information about an error, try `rustc --explain E0433`.
error: could not compile `mct-iroh` (lib test) due to 2 previous errors
```

Intermediate compile failure after adding the generic callback seam:

```text
   Compiling mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0277]: the trait bound `anyhow::Error: std::error::Error` is not satisfied
    --> crates/mct-daemon/src/main.rs:1538:5
     |
1538 | /     MctIrohHelloObservationSink::new(move |trace_id, evaluation| {
1539 | |         let ledger = ledger.clone();
1540 | |         async move {
1541 | |             ledger
...    |
1549 | |     })
     | |______^ the trait `std::error::Error` is not implemented for `anyhow::Error`
     |
note: required by a bound in `MctIrohHelloObservationSink::new`
    --> crates/mct-iroh/src/serve.rs:310:12
     |
 306 |     pub fn new<F, Fut, E>(callback: F) -> Self
     |            --- required by a bound in this associated function
...
 310 |         E: StdError + Send + Sync + 'static,
     |            ^^^^^^^^ required by this bound in `MctIrohHelloObservationSink::new`

For more information about this error, try `rustc --explain E0277`.
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 1 previous error
```

## S2.2 implementation record

- `e16e59d` — `fix(iroh): persist hello authority before response`.
- `MctIrohHelloObservationSink` is awaited while the per-endpoint hello state is cleared and before an admitted evaluation is remembered or response bytes are written.
- `resident_hello_observation_sink` projects the kernel evaluation and awaits `ResidentLedgerWriter::append`, whose acknowledgment follows `append_batch_before_effect` and `sync_data`.
- The old post-response `Served::Hello` projection is suppressed, avoiding a duplicate canonical authority fact; the served event still drives non-ledger follow-up such as admitted remote-surface refresh.
- `resident_hello_observations_are_durable_before_responses` reads the ledger immediately after each admitted and denied client response and finds the matching `BeforeEffect` `PeerAdmitted`/`PeerRejected` entry. It also verifies that a signature marker and inline payload field are absent.
- `failed_hello_observation_prevents_response_and_remembered_admission` injects one deterministic append failure, observes no hello response, proves there is no retry, and verifies a subsequent call is denied with `HelloNotAdmitted` without invoking the handler.
- Full required validation passed after `e16e59d`: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh`.
- Flakes: none. The two expected compile failures above were red-test/intermediate implementation failures, not rerun flakes.

---

# Slice S2.5 — mechanical daemon main decomposition

## Baseline

- Starting branch/HEAD: `patina` at `9707420` (`docs: close hello observation durability`).
- Starting tree: clean.
- Starting `crates/mct-daemon/src/main.rs`: 8,682 lines.
- Before-count: **254** tests (254 passed + 0 ignored across every `test result:` line from `cargo test --workspace`), matching the operator baseline.
- `crates/mct-daemon/src/lib.rs` and every library-owned module remain untouched; no library public-surface promotion is planned.

## S2.5a seam plan

The full binary was read before choosing these seams. The route, forwarding, execution, serving, and observation records form one tightly connected resident pipeline: route outcomes carry execution records, forwarding reuses route revalidation and payload projection, and the 35 resident tests share end-to-end fixtures across those boundaries. To keep this slice mechanical rather than manufacture a new internal API, those candidate seams are merged into one binary-local `resident` module. This still isolates the complete resident runtime from CLI command families and gives later semantic decomposition a single subject-owned file rather than `main.rs`.

All files are binary-local under `crates/mct-daemon/src/daemon/`, declared from `main.rs`; none join the library module set.

| Module | Responsibility | Approximate current ranges moving | Test placement |
|---|---|---|---|
| `daemon/cli_runtime.rs` | Children/process/WASM/WIT/slate command implementations and their CLI-only authority helpers. | `main.rs:83-1009` | Existing CLI authorization tests remain in the resident binary test module because they share its child/authority fixtures; new focused tests belong in inline `mod tests`. |
| `daemon/resident.rs` | Resident Mother serving, ledger writer, observation projection, candidate sourcing/ranking, local and remote route decisions, forwarding client, payload resolution, child delivery, result projection, and revision guards. | `main.rs:1010-3711` plus the existing `main.rs:5628-8682` binary test module | The existing inline `mod tests` moves intact with this module so all resident route/forwarding/execution/serving tests and shared fixtures remain compiled together without logic edits. |
| `daemon/control.rs` | HTTP/UDS control serving, snapshot source, resident status projection, and control command dispatch. | `main.rs:3712-3890` | Existing control/status tests remain in `resident::tests` because they share resident status fixtures; new focused tests belong inline here. |
| `daemon/cli_admin.rs` | Registry, federation, metrics, Pando, toys, state/runs, and peer command families. This is the clean landing zone for slice 2b peer CLI rewiring, without preparing that work now. | `main.rs:3891-4681` | Existing toy CLI tests remain in `resident::tests` with their shared child fixtures; new focused tests belong inline here. |
| `daemon/ingress.rs` | JVM and standalone Iroh ingress/client bridges, protocol request construction, configured-child lookup, and binary adapter helpers. | `main.rs:4682-5591` | Existing JVM/Iroh integration coverage remains in `resident::tests`; new focused tests belong inline here. |

`main.rs` retains only imports, binary-local `mod` declarations/use wiring, `main()`, argument token helpers, default path helpers, help text, and the test-only authority fixture declaration. Expected end state is comfortably within the requested order of 1,000–1,500 lines (likely smaller because the current parser skeleton is compact).

Dependency order for extraction is `cli_runtime` → `control` → `cli_admin` → `ingress` → `resident`: the first four are command/adapter leaves while `resident` composes them and takes the shared integration tests last. Each move may add only `mod`/`use` wiring and the narrow `pub(super)` visibility required for binary-local cross-module calls and tests.

## S2.5 checklist

- [x] Step 0 matched `9707420`, clean tree, completed A5.
- [x] Read all 8,682 lines of `main.rs` and all of `lib.rs`.
- [x] Record the 254-test baseline.
- [x] Commit the seam plan.
- [x] Extract `cli_runtime`.
- [x] Extract `control`.
- [x] Extract `cli_admin`.
- [x] Extract `ingress`.
- [x] Extract `resident` with the existing inline binary tests.
- [x] Confirm after-count is 254 and close the slice.

## Itch list (notes only; no fixes in S2.5)

- CLI option parsing and default-path selection are repeated across most command families.
- Several `serve-process` ledger/state writes intentionally discard errors with `let _ =`; changing those semantics is outside a move-only slice.
- Standalone `iroh serve` and `iroh serve-process` use default concurrent-serve config without the resident hello-observation sink; this should be reconciled separately rather than folded into decomposition.
- Peer add/proof/revoke/remove mutate config directly and remain the explicit slice 2b work.
- Resident route selection, forwarding, and execution use many concrete cross-stage records; a finer module API should be designed only in a behavior-owning refactor, not inferred during moves.
- The resident integration suite has broad shared setup and fixture construction; future focused contract tests should be added beside the extracted subjects instead of extending the shared fixture module indefinitely.
- Several protocol/observation IDs are fixed CLI literals while resident IDs are generated; consistency is outside this slice.
- Payload fact construction and result projection contain repeated content-type/digest handling that should not be deduplicated mechanically.

## S2.5 failure log

Mechanical resident extraction initially carried the `control` module's `#[path]` attribute into `resident.rs`. The first check failed as follows before the attribute was moved back with its declaration:

```text
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0432]: unresolved imports `crate::MctRuntimeRunRecord`, `crate::MctRuntimeStateSummary`, `crate::status`
 --> crates/mct-daemon/src/control.rs:2:25
  |
2 |     MCT_BLOB_MAX_BYTES, MctRuntimeRunRecord, MctRuntimeStateSummary,
  |                         ^^^^^^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^ no `MctRuntimeStateSummary` in the root
  |                         |
  |                         no `MctRuntimeRunRecord` in the root
3 |     local_blob_store_for_state_path,
4 |     status::{MctDaemonHealth, MctDaemonReadiness, MctDaemonStatus, daemon_status},
  |     ^^^^^^ could not find `status` in the crate root
  |
help: a similar name exists in the module
  |
2 -     MCT_BLOB_MAX_BYTES, MctRuntimeRunRecord, MctRuntimeStateSummary,
2 +     MCT_BLOB_MAX_BYTES, MctRuntimeRunRecord, MctRuntimeStateStore,
  |

error[E0432]: unresolved imports `crate::MctDaemonHealth`, `crate::MctDaemonReadiness`
   --> crates/mct-daemon/src/control.rs:514:17
    |
514 |     use crate::{MctDaemonHealth, MctDaemonReadiness};
    |                 ^^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^ no `MctDaemonReadiness` in the root
    |                 |
    |                 no `MctDaemonHealth` in the root
    |
help: a similar name exists in the module
    |
514 -     use crate::{MctDaemonHealth, MctDaemonReadiness};
514 +     use crate::{MctDaemonStatus, MctDaemonReadiness};
    |

warning: unused imports: `MctControlPlaneSnapshotError`, `MctControlPlaneSnapshotResult`, `MctControlPlaneSnapshot`, and `daemon_status`
  --> crates/mct-daemon/src/main.rs:6:60
   |
 6 |     MctCompositionStep, MctConfigChildAuthorityProjection, MctControlPlaneSnapshot,
   |                                                            ^^^^^^^^^^^^^^^^^^^^^^^
 7 |     MctControlPlaneSnapshotError, MctControlPlaneSnapshotResult, MctDaemonConfigStore,
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
15 |     build_metrics_snapshot, current_timestamp, daemon_status, daemon_status_with_resident,
   |                                                ^^^^^^^^^^^^^
   |
   = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default

warning: unused import: `tokio::net::UnixListener`
  --> crates/mct-daemon/src/main.rs:44:5
   |
44 | use tokio::net::UnixListener;
   |     ^^^^^^^^^^^^^^^^^^^^^^^^

warning: unused import: `control::*`
  --> crates/mct-daemon/src/main.rs:92:5
   |
92 | use control::*;
   |     ^^^^^^^^^^

warning: unused imports: `MctControlPlaneSnapshotResult`, `MctControlPlaneSnapshot`, and `daemon_status`
  --> crates/mct-daemon/src/main.rs:6:60
   |
 6 |     MctCompositionStep, MctConfigChildAuthorityProjection, MctControlPlaneSnapshot,
   |                                                            ^^^^^^^^^^^^^^^^^^^^^^^
 7 |     MctControlPlaneSnapshotError, MctControlPlaneSnapshotResult, MctDaemonConfigStore,
   |                                   ^^^^^^^^^^^^^^^^^^^^^^^^
...
15 |     build_metrics_snapshot, current_timestamp, daemon_status, daemon_status_with_resident,
   |                                                ^^^^^^^^^^^^^
   |
   = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default

error[E0433]: cannot find type `ControlSnapshotSource` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:447:27
    |
447 |     let snapshot_source = ControlSnapshotSource::open(state_path);
    |                           ^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `ControlSnapshotSource`

error[E0425]: cannot find function `control_snapshot` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:452:13
    |
452 |             control_snapshot(&snapshot_source).await,
    |             ^^^^^^^^^^^^^^^^ not found in this scope

error[E0433]: cannot find type `ControlSnapshotSource` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:465:27
    |
465 |     let snapshot_source = ControlSnapshotSource::open_with_status(&state_path, status_source);
    |                           ^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `ControlSnapshotSource`

error[E0425]: cannot find function `control_snapshot` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:472:17
    |
472 |                 control_snapshot(&snapshot_source).await,
    |                 ^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find function `run_control` in this scope
  --> crates/mct-daemon/src/main.rs:62:22
   |
62 |         "control" => run_control(args).await?,
   |                      ^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find function `run_control_serve_uds_with_state_until` in this scope
   --> crates/mct-daemon/src/daemon/resident.rs:397:13
    |
397 |             run_control_serve_uds_with_state_until(state_path, path, shutdown, status_source).await
    |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0433]: cannot find type `ControlSnapshotSource` in this scope
    --> crates/mct-daemon/src/daemon/resident.rs:5679:22
     |
5679 |         let source = ControlSnapshotSource::open(dir.path());
     |                      ^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `ControlSnapshotSource`

error[E0425]: cannot find function `control_snapshot` in this scope
    --> crates/mct-daemon/src/daemon/resident.rs:5681:24
     |
5681 |         let snapshot = control_snapshot(&source).await;
     |                        ^^^^^^^^^^^^^^^^ not found in this scope

Some errors have detailed explanations: E0425, E0432, E0433.
For more information about an error, try `rustc --explain E0425`.
warning: `mct-daemon` (bin "mct-daemon") generated 3 warnings
error: could not compile `mct-daemon` (bin "mct-daemon") due to 7 previous errors; 3 warnings emitted
warning: build failed, waiting for other jobs to finish...
warning: `mct-daemon` (bin "mct-daemon" test) generated 3 warnings (2 duplicates)
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 10 previous errors; 3 warnings emitted
```

## S2.5c close record

### Per-module commits and final line counts

| Commit | Extracted module | Final lines |
|---|---|---:|
| `7977989` | `crates/mct-daemon/src/daemon/cli_runtime.rs` | 933 |
| `9ad3be0` | `crates/mct-daemon/src/daemon/control.rs` | 185 |
| `a31f1f0` | `crates/mct-daemon/src/daemon/cli_admin.rs` | 798 |
| `c212b85` | `crates/mct-daemon/src/daemon/ingress.rs` | 915 |
| `741e620` | `crates/mct-daemon/src/daemon/resident.rs` (including the moved inline integration tests) | 5,767 |

Final `crates/mct-daemon/src/main.rs`: **141 lines**, retaining only imports, entrypoint dispatch, binary-local module wiring, argument-token helpers, default paths, help text, and the test authority-fixture declaration. The result is below the approximate 1,000–1,500-line target because this binary's parser skeleton is compact once command implementations and inline tests move.

### Verification

- Before-count: **254** (254 passed, 0 ignored).
- After-count: **254** (254 passed, 0 ignored).
- Every extraction commit passed `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh`.
- The mct-daemon library-owned surface has zero diff from `9707420`; all new files are binary-local beneath `src/daemon/`.
- Flakes: none. The one deterministic intermediate path-attribute compile failure is recorded verbatim above and was not rerun as a flake.
- ROADMAP standing backlog now marks `main.rs` decomposition substantially addressed; dispatch wiring remains in `main.rs` by design.

### Itch list (verbatim from the plan)

- CLI option parsing and default-path selection are repeated across most command families.
- Several `serve-process` ledger/state writes intentionally discard errors with `let _ =`; changing those semantics is outside a move-only slice.
- Standalone `iroh serve` and `iroh serve-process` use default concurrent-serve config without the resident hello-observation sink; this should be reconciled separately rather than folded into decomposition.
- Peer add/proof/revoke/remove mutate config directly and remain the explicit slice 2b work.
- Resident route selection, forwarding, and execution use many concrete cross-stage records; a finer module API should be designed only in a behavior-owning refactor, not inferred during moves.
- The resident integration suite has broad shared setup and fixture construction; future focused contract tests should be added beside the extracted subjects instead of extending the shared fixture module indefinitely.
- Several protocol/observation IDs are fixed CLI literals while resident IDs are generated; consistency is outside this slice.
- Payload fact construction and result projection contain repeated content-type/digest handling that should not be deduplicated mechanically.

---

# Slice 2b — observed peer authority mutations

## S2b.1 mechanism specification

Starting state: `patina` at `b49240e` (`docs: close daemon main decomposition`), clean tree.

### Local UDS commands

HTTP remains read-only. The local UDS accepts these JSON commands, each with a **64 KiB body budget** inside the existing bounded UDS transport read budget:

| Method/path | Request | Success response |
|---|---|---|
| `POST /peers/add` | expected config path; peer node, binding, endpoint and Vision IDs; optional endpoint ticket and presented binding proof | action, peer node, binding, endpoint, Vision, policy revision, resulting peer count |
| `POST /peers/proof` | expected config path; peer node and outbound binding IDs; policy revision; proof; optional expiry | same safe mutation facts and resulting peer count; proof is omitted |
| `POST /peers/revoke` | expected config path and peer node ID | safe facts for the binding now revoked and resulting peer count |
| `POST /peers/remove` | expected config path and peer node ID | safe facts for the removed binding and resulting peer count |

The expected config path prevents a CLI using a custom `--config` from silently mutating a resident attached to another store. Malformed/oversized requests fail before observation or mutation. Responses never echo proof/signature material.

### Handler ordering and observation mapping

The resident UDS handler serially performs: deserialize and validate against the current config → construct the typed decision fact → await `ResidentLedgerWriter::append` (`append_batch_before_effect` plus `sync_data`) → apply the config mutation → return success. If apply fails after the decision is durable, it appends an `OperatorActionRecorded` failure fact and returns failure; it never reports success or hides ledger/config divergence. If the decision append fails, config is untouched.

Decision kinds are `PeerBindingRecorded` for add and proof update, and `PeerBindingRevoked` for revoke and remove. Facts include action, peer node, binding, endpoint, Vision, policy revision, and state/expiry metadata where applicable. Proofs, signatures, ticket secrets, payload bytes, and raw failure details are excluded. Apply failures use `OperatorActionRecorded` with `Failed` outcome and the same safe identity/revision facts.

### Mutation visibility coherence

The resident handler applies through `MctDaemonConfigStore` at the same `config_path` captured by `run_resident_mother`. The Iroh binding provider invokes `load_peer_bindings_for_iroh(config_path)` for every accepted hello/call connection; that function calls `MctDaemonConfigStore::load()` and `peer_authority_projection()` rather than reading a cached snapshot. Therefore a successful UDS reply follows config replacement, and the next call's current-binding evaluation necessarily reloads the mutation. Since the decision append precedes config replacement, the typed fact is durable before any denial or admission caused by the mutation.

### CLI online/offline arbitration

Peer mutation commands accept `--uds` (default `.mct/control.sock`) and `--ledger` (default `.mct/observations.jsonl`). They first attempt the UDS command. If connection fails, they open `JsonlObservationLedger` as the exclusive writer, prepare the same validated mutation, append the same decision fact durably, apply the same config operation, append a typed failure on apply error, and report the result. If the resident owns the writer lock, offline open fails with a clear fail-closed error and config is untouched. The CLI never writes config before either a resident acknowledgment or acquisition of the free writer lock.

### Out of scope

- Child approval, operator, and storage observation gaps (A6 remainder, slice 4).
- Standalone serve sink gaps and discarded ledger writes (slice 3).
- Any HTTP mutation route.
- Operator identity/authentication beyond local UDS filesystem permissions (C1 future).

## Slice 2b checklist

- [x] Re-establish `b49240e` baseline and read the recorded decision and affected surfaces.
- [x] Record the UDS, ordering, visibility, and offline-lock mechanism.
- [x] Land failing regression tests and implementation.
- [x] Mark the peer-authority portion of A6 fixed; leave slice 4 remainder open.

## Slice 2b failure log

Expected red compile before the UDS mutation callback seam existed:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0433]: cannot find type `MctUdsControlMutationHandler` in this scope
   --> crates/mct-daemon/src/control.rs:617:23
    |
617 |         let handler = MctUdsControlMutationHandler::new(move |path, body| {
    |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctUdsControlMutationHandler`

error[E0425]: cannot find function `serve_uds_control_once_with_snapshot_result_blob_store_and_mutations` in this scope
   --> crates/mct-daemon/src/control.rs:631:13
    |
304 | / pub async fn serve_uds_control_once_with_snapshot_result_and_blob_store(
305 | |     listener: &UnixListener,
306 | |     snapshot: MctControlPlaneSnapshotResult,
307 | |     blob_state_path: Option<&Path>,
...   |
334 | |     Ok(())
335 | | }
    | |_- similarly named function `serve_uds_control_once_with_snapshot_result_and_blob_store` defined here
...
631 |               serve_uds_control_once_with_snapshot_result_blob_store_and_mutations(
    |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
help: a function with a similar name exists
    |
631 -             serve_uds_control_once_with_snapshot_result_blob_store_and_mutations(
631 +             serve_uds_control_once_with_snapshot_result_and_blob_store(
    |

Some errors have detailed explanations: E0425, E0433.
For more information about an error, try `rustc --explain E0425`.
error: could not compile `mct-daemon` (lib test) due to 2 previous errors
warning: build failed, waiting for other jobs to finish...
```

Expected red compile after adding the Slice 2b behavior regressions and before implementing the shared resident/offline mutation workflow:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0425]: cannot find function `resident_peer_mutation_handler` in this scope
   --> crates/mct-daemon/src/daemon/control.rs:260:23
    |
260 |         let handler = resident_peer_mutation_handler(config_path.clone(), ledger.clone());
    |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find function `resident_peer_mutation_handler` in this scope
   --> crates/mct-daemon/src/daemon/control.rs:344:23
    |
344 |         let handler = resident_peer_mutation_handler(
    |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find function `resident_peer_mutation_handler` in this scope
   --> crates/mct-daemon/src/daemon/control.rs:371:23
    |
371 |         let handler = resident_peer_mutation_handler(config_path.clone(), ledger.clone());
    |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find function `execute_offline_peer_mutation` in this scope
   --> crates/mct-daemon/src/daemon/control.rs:408:24
    |
408 |         let response = execute_offline_peer_mutation(
    |                        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find function `execute_offline_peer_mutation` in this scope
   --> crates/mct-daemon/src/daemon/control.rs:428:21
    |
428 |         let error = execute_offline_peer_mutation(
    |                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope

For more information about this error, try `rustc --explain E0425`.
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 5 previous errors
```

Deterministic pre-commit Clippy findings before tightening the prepared mutation representation:

```text
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error: large size difference between variants
  --> crates/mct-daemon/src/daemon/control.rs:38:1
   |
38 | /  enum PreparedPeerMutationEffect {
39 | |      Add(MctPeerAddressBookEntry),
   | |      ---------------------------- the largest variant contains at least 352 bytes
40 | |/     Proof {
41 | ||         peer_node_id: MctNodeId,
42 | ||         outbound: MctOutboundPeerBindingPresentation,
43 | ||     },
   | ||_____- the second-largest variant contains at least 136 bytes
44 | |      Revoke(MctNodeId),
45 | |      Remove(MctNodeId),
46 | |  }
   | |__^ the entire enum is at least 352 bytes
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#large_enum_variant
   = note: `-D clippy::large-enum-variant` implied by `-D warnings`
   = help: to override `-D warnings` add `#[allow(clippy::large_enum_variant)]`
help: consider boxing the large fields or introducing indirection in some other way to reduce the total size of the enum
   |
39 -     Add(MctPeerAddressBookEntry),
39 +     Add(Box<MctPeerAddressBookEntry>),
   |

error: this function has too many arguments (10/7)
   --> crates/mct-daemon/src/daemon/control.rs:102:1
    |
102 | / fn peer_mutation_observation(
103 | |     kind: ObservationKind,
104 | |     outcome: ObservationOutcome,
105 | |     action: &str,
...   |
112 | |     expires_at: Option<&Timestamp>,
113 | | ) -> MctObservation {
    | |___________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#too_many_arguments
    = note: `-D clippy::too-many-arguments` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::too_many_arguments)]`

error: could not compile `mct-daemon` (bin "mct-daemon") due to 2 previous errors
warning: build failed, waiting for other jobs to finish...
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 2 previous errors
```

Validation command invocation corrected before rerun (Cargo accepts one test filter):

```text
error: unexpected argument 'resident::tests::resident_mother_serves_peer_control_and_shutdown' found

Usage: cargo test [OPTIONS] [TESTNAME] [-- [ARGS]...]

For more information, try '--help'.
```

## S2b.2 implementation record

### Commits

- `d8be388` — `docs: specify peer authority control mutations`
- `e86a26a` — `feat(control): add local UDS mutation seam`
- `393884f` — `fix(daemon): observe peer authority mutations`

### Implemented command shapes

The resident UDS now owns `POST /peers/add`, `POST /peers/proof`, `POST /peers/revoke`, and `POST /peers/remove`. Requests are bounded to 64 KiB, carry `expected_config_path`, and return only safe peer/binding/endpoint/Vision/revision/state/count facts. HTTP remains read-only.

`mct-daemon peers add`, `set-outbound-proof`, `revoke`, and `remove` accept `--uds` and `--ledger`. Each first attempts the resident UDS; connection failure alone permits the offline path, which must acquire the exclusive `JsonlObservationLedger` writer lock before validation, decision append, and config replacement. A connected resident rejection never falls back to direct config mutation.

### Ordering and visibility evidence

- `live_uds_peer_mutations_are_durable_and_secret_free` exercises all four UDS mutations, their typed `PeerBindingRecorded`/`PeerBindingRevoked` facts, `BeforeEffect` durability, the 64 KiB rejection, and proof/signature exclusion.
- `resident_append_failure_prevents_peer_config_effect` proves append acknowledgment gates config replacement.
- `resident_apply_failure_records_typed_failure_after_decision` proves a post-decision config failure yields `OperatorActionRecorded`/`Failed` without command success.
- `offline_peer_mutation_observes_before_effect_and_fails_on_lock_contention` exercises the actual CLI fallback and proves a held writer lock leaves the target config untouched.
- `resident_mother_serves_peer_control_and_shutdown` now revokes an admitted peer over the live resident UDS, observes the revocation first, and proves the next call reloads current config and is denied before execution.

The resident mutation handler and the Iroh authority provider capture the same `config_path`. The provider still calls `MctDaemonConfigStore::load()` for each connection, so no stale peer cache can survive a successful mutation reply. The decision ledger append is acknowledged before config replacement; therefore authority visibility cannot precede its canonical fact.

### Validation and flakes

Required workspace tests, Clippy with `-D warnings`, and tier-0 passed after each of `d8be388`, `e86a26a`, and `393884f`. The final implementation validation passed **259 tests** (259 passed, 0 ignored), Clippy, and Allium check with empty diagnostics/findings.

Flakes: none. Expected red compile failures, deterministic intermediate Clippy findings, and the corrected Cargo filter invocation are recorded verbatim above.

A6 remains open only for the child/operator/storage paths assigned to slice 4. Stop after S2b.2; no slice 3 or slice 4 implementation begins here.

---

# Slice 3 — complete peer-call lifecycle observations

Starting state: `patina` at `bebd227` (`docs: close observed peer mutation slice`), clean tree. Allium 3.5.0 check returns no diagnostics or findings.

## S3.1 mechanism specification

### Lifecycle facts and durability

| Lifecycle fact | Observation kind | Durability | Ordering obligation |
|---|---|---|---|
| Peer call frame received | `PeerCallReceived` | `BeforeEffect` | Durable before decode success can advance into call construction/authority and before any malformed response. It is call-ingress evidence required by the matrix. |
| Undecodable, oversized, invalid, or payload-mismatched request rejected | `PeerCallMalformed` | `BeforeEffect` | Durable before the caller-safe malformed reply. Append failure closes the stream without a reply. |
| Valid envelope constructs one semantic call | `CallConstructed` | `BeforeEffect` | Durable in the same causal prefix as receipt and before authority or handler execution. |
| Call authority allows routing | `CallAuthorized` | `BeforeEffect` | Durable before invoking the call handler, because the handler may perform the protected route/execution effect. |
| Call authority denies | `CallDenied` | `BeforeEffect` | Durable before writing the denial reply. |
| Route selected/revalidated or no route | existing `RouteSelected`, `RouteRevalidated`, or `NoRouteRecorded` facts | Existing `BeforeEffect` writes | `execute_resident_call` already awaits these through `ResidentLedgerWriter` before local/remote execution or terminal no-route return. The transport sink must not duplicate them. |
| Terminal handler result recorded | `ResultRecorded` | `Buffered` | Appended after the handler/runtime effect produces its terminal result and before reply encoding/writing. It is an adapter/result fact, not authority; sink failure is nevertheless propagated and suppresses the reply rather than silently losing evidence. |
| Peer reply emitted | `PeerCallReplied` | `Buffered` | Appended only after response bytes are written and the send stream is finished, so the fact remains truthful. Failure cannot retract delivered bytes, but it is propagated as a fatal serving error rather than discarded. |

The successful causal sequence is therefore receipt → construction → authorization → existing route/revalidation and runtime facts → result recording → reply emission. A denied valid call has receipt → construction → denial → terminal result recording → reply emission. A malformed call has receipt → malformed rejection → reply emission; no fabricated semantic call or `MctResult` is created.

### Malformed handling

The concurrent serve task intercepts call-frame read-budget failures before the current `?` return and intercepts envelope decode, request validation, and payload-integrity failures inside the call ALPN branch before their current early returns. It creates a typed malformed evaluation/reply containing only generated safe identifiers and `malformed request`, awaits the lifecycle sink for receipt plus malformed rejection, then writes the safe malformed reply. If that append fails, the branch returns before any response bytes are written. Undecodable and oversized inputs never enter the handler and their raw bytes never enter a fact.

### One mandatory sink

Rename/generalize the slice-2 callback to one `MctIrohObservationSink` carrying typed hello and call-lifecycle fact batches plus the requested durability class. `MctIrohConcurrentServeConfig` owns a non-optional sink and no longer implements a sinkless default; construction requires `MctIrohConcurrentServeConfig::new(sink)`. The single-connection serving APIs also take the same sink, so every public serving path represents observation ownership explicitly. Tests use collecting or no-op-success test sinks; production serving paths use ledger-backed sinks. There is no second observation callback or post-response authority path.

### Standalone serving and writer ownership

Both `iroh serve` and `iroh serve-process` accept `--ledger` (default `.mct/observations.jsonl`) and acquire `ResidentLedgerWriter` before binding an endpoint or printing a ticket. This is the same exclusive `JsonlObservationLedger` lock used by the resident and slice-2b offline path. Refusal is:

```text
standalone Iroh serve refused: could not acquire the exclusive observation ledger writer; another Mother may already be serving this node
```

No refusal observation is appended: inability to acquire the canonical writer is exactly why this process has no authority to write that ledger. The command returns before endpoint bind, so there is no partial serving effect.

### Discarded serve-process writes

Every current discarded write is fail-open and none qualifies to remain so:

| Current site | Disposition |
|---|---|
| Authority observation `append_ledger_observations` | Use the already-owned resident writer; await and return a failed handler result before child invocation if append fails. |
| Authority observation `runtime_state.append_run_observations` | Propagate as a failed handler result before child invocation; runtime provenance may not silently diverge. |
| Process report `append_ledger_observations` | Await the owned writer after execution; failure prevents a success reply. |
| Process report `runtime_state.append_run_observations` | Propagate failure; do not report successful completion with an incomplete run projection. |
| `runtime_state.complete_run` | Propagate failure; do not report successful completion while durable runtime state remains running/incomplete. |

The standalone path must not call `append_ledger_observations`, because reopening the ledger would contend with its own lifetime writer lock.

### Out of scope

- Child approval, operator, and storage observation gaps (A6 remainder, slice 4).
- Reload replacement ordering (A7).
- Request idempotency/replay semantics (A3).
- New observation kinds beyond the existing lifecycle taxonomy listed above.

## Slice 3 checklist

- [x] Re-establish `bebd227`, read A8/map contracts, sink paths, standalone paths, discarded writes, and writer-lock semantics.
- [x] Record the S3.1 lifecycle, durability, malformed, mandatory-sink, standalone ownership, and discarded-write mechanism.
- [x] Land failing lifecycle and standalone regressions before implementation.
- [x] Implement the mandatory lifecycle sink and fail-closed standalone paths.
- [x] Mark A8 fixed, annotate A5 standalone coverage, validate every commit, and stop after S3.2.

## Slice 3 failure log

Expected red compile after adding lifecycle-sink regressions and before the generalized mandatory sink existed:

```text
   Compiling mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
error[E0425]: cannot find type `MctIrohObservationBatch` in this scope
   --> crates/mct-iroh/src/lib.rs:442:32
    |
442 |         batches: Arc<Mutex<Vec<MctIrohObservationBatch>>>,
    |                                ^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope
    |
help: you might be missing a type parameter
    |
441 |     fn collecting_observation_sink<MctIrohObservationBatch>(
    |                                   +++++++++++++++++++++++++

error[E0425]: cannot find type `MctIrohObservationSink` in this scope
   --> crates/mct-iroh/src/lib.rs:443:10
    |
443 |     ) -> MctIrohObservationSink {
    |          ^^^^^^^^^^^^^^^^^^^^^^
    |
   ::: crates/mct-iroh/src/serve.rs:297:1
    |
297 | pub struct MctIrohHelloObservationSink {
    | -------------------------------------- similarly named struct `MctIrohHelloObservationSink` defined here
    |
help: a struct with a similar name exists
    |
443 |     ) -> MctIrohHelloObservationSink {
    |                 +++++

error[E0433]: cannot find type `MctIrohObservationSink` in this scope
   --> crates/mct-iroh/src/lib.rs:444:9
    |
444 |         MctIrohObservationSink::new(move |batch| {
    |         ^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohObservationSink`
    |
help: a struct with a similar name exists
    |
444 |         MctIrohHelloObservationSink::new(move |batch| {
    |                +++++

error[E0599]: no associated function or constant named `new` found for struct `serve::MctIrohConcurrentServeConfig` in the current scope
   --> crates/mct-iroh/src/lib.rs:509:51
    |
509 |                     MctIrohConcurrentServeConfig::new(sink),
    |                                                   ^^^ associated function or constant not found in `serve::MctIrohConcurrentServeConfig`
    |
   ::: crates/mct-iroh/src/serve.rs:338:1
    |
338 | pub struct MctIrohConcurrentServeConfig {
    | --------------------------------------- associated function or constant `new` not found for this struct
    |
    = help: items from traits can only be used if the trait is implemented and in scope
    = note: the following traits define an item `new`, perhaps you need to implement one of them:
            candidate #1: `crypto_common::KeyInit`
            candidate #2: `crypto_common::KeyInit`
            candidate #3: `crypto_common::KeyIvInit`
            candidate #4: `crypto_common::KeyIvInit`
            candidate #5: `crypto_common::TryKeyInit`
            candidate #6: `curve25519_dalek::traits::VartimePrecomputedMultiscalarMul`
            candidate #7: `digest::block_api::VariableOutputCore`
            candidate #8: `digest::digest::Digest`
            candidate #9: `parking_lot_core::thread_parker::ThreadParkerT`
            candidate #10: `rand::distr::uniform::UniformSampler`
            candidate #11: `ring::aead::BoundKey`
            candidate #12: `typenum::marker_traits::Bit`

error[E0433]: cannot find type `MctIrohObservationFact` in this scope
   --> crates/mct-iroh/src/lib.rs:536:33
    |
536 |                     .filter_map(MctIrohObservationFact::call_stage)
    |                                 ^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohObservationFact`

error[E0433]: cannot find type `MctIrohCallLifecycleStage` in this scope
   --> crates/mct-iroh/src/lib.rs:538:37
    |
538 |                 if stages.contains(&MctIrohCallLifecycleStage::ReplyEmitted) {
    |                                     ^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohCallLifecycleStage`

error[E0433]: cannot find type `MctIrohObservationFact` in this scope
   --> crates/mct-iroh/src/lib.rs:551:25
    |
551 |             .filter_map(MctIrohObservationFact::call_stage)
    |                         ^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohObservationFact`

error[E0433]: cannot find type `MctIrohCallLifecycleStage` in this scope
   --> crates/mct-iroh/src/lib.rs:556:17
    |
556 |                 MctIrohCallLifecycleStage::Received,
    |                 ^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohCallLifecycleStage`

error[E0433]: cannot find type `MctIrohCallLifecycleStage` in this scope
   --> crates/mct-iroh/src/lib.rs:557:17
    |
557 |                 MctIrohCallLifecycleStage::Constructed,
    |                 ^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohCallLifecycleStage`

error[E0433]: cannot find type `MctIrohCallLifecycleStage` in this scope
   --> crates/mct-iroh/src/lib.rs:558:17
    |
558 |                 MctIrohCallLifecycleStage::Authorized,
    |                 ^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohCallLifecycleStage`

error[E0433]: cannot find type `MctIrohCallLifecycleStage` in this scope
   --> crates/mct-iroh/src/lib.rs:559:17
    |
559 |                 MctIrohCallLifecycleStage::ResultRecorded,
    |                 ^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohCallLifecycleStage`

error[E0433]: cannot find type `MctIrohCallLifecycleStage` in this scope
   --> crates/mct-iroh/src/lib.rs:560:17
    |
560 |                 MctIrohCallLifecycleStage::ReplyEmitted,
    |                 ^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohCallLifecycleStage`

error[E0599]: no associated function or constant named `new` found for struct `serve::MctIrohConcurrentServeConfig` in the current scope
   --> crates/mct-iroh/src/lib.rs:579:51
    |
579 |                     MctIrohConcurrentServeConfig::new(sink),
    |                                                   ^^^ associated function or constant not found in `serve::MctIrohConcurrentServeConfig`
    |
   ::: crates/mct-iroh/src/serve.rs:338:1
    |
338 | pub struct MctIrohConcurrentServeConfig {
    | --------------------------------------- associated function or constant `new` not found for this struct
    |
    = help: items from traits can only be used if the trait is implemented and in scope
    = note: the following traits define an item `new`, perhaps you need to implement one of them:
            candidate #1: `crypto_common::KeyInit`
            candidate #2: `crypto_common::KeyInit`
            candidate #3: `crypto_common::KeyIvInit`
            candidate #4: `crypto_common::KeyIvInit`
            candidate #5: `crypto_common::TryKeyInit`
            candidate #6: `curve25519_dalek::traits::VartimePrecomputedMultiscalarMul`
            candidate #7: `digest::block_api::VariableOutputCore`
            candidate #8: `digest::digest::Digest`
            candidate #9: `parking_lot_core::thread_parker::ThreadParkerT`
            candidate #10: `rand::distr::uniform::UniformSampler`
            candidate #11: `ring::aead::BoundKey`
            candidate #12: `typenum::marker_traits::Bit`

error[E0433]: cannot find type `MctIrohObservationFact` in this scope
   --> crates/mct-iroh/src/lib.rs:598:25
    |
598 |             .filter_map(MctIrohObservationFact::call_stage)
    |                         ^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohObservationFact`

error[E0433]: cannot find type `MctIrohCallLifecycleStage` in this scope
   --> crates/mct-iroh/src/lib.rs:603:17
    |
603 |                 MctIrohCallLifecycleStage::Received,
    |                 ^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohCallLifecycleStage`

error[E0433]: cannot find type `MctIrohCallLifecycleStage` in this scope
   --> crates/mct-iroh/src/lib.rs:604:17
    |
604 |                 MctIrohCallLifecycleStage::Malformed,
    |                 ^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohCallLifecycleStage`

error[E0433]: cannot find type `MctIrohCallLifecycleStage` in this scope
   --> crates/mct-iroh/src/lib.rs:605:17
    |
605 |                 MctIrohCallLifecycleStage::ReplyEmitted,
    |                 ^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohCallLifecycleStage`

error[E0433]: cannot find type `MctIrohObservationSink` in this scope
   --> crates/mct-iroh/src/lib.rs:612:28
    |
612 |         let failing_sink = MctIrohObservationSink::new(|batch| async move {
    |                            ^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohObservationSink`
    |
help: a struct with a similar name exists
    |
612 |         let failing_sink = MctIrohHelloObservationSink::new(|batch| async move {
    |                                   +++++

error[E0433]: cannot find type `MctIrohCallLifecycleStage` in this scope
   --> crates/mct-iroh/src/lib.rs:616:55
    |
616 |                 .any(|fact| fact.call_stage() == Some(MctIrohCallLifecycleStage::Malformed))
    |                                                       ^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared type `MctIrohCallLifecycleStage`

error[E0599]: no associated function or constant named `new` found for struct `serve::MctIrohConcurrentServeConfig` in the current scope
   --> crates/mct-iroh/src/lib.rs:628:51
    |
628 |                     MctIrohConcurrentServeConfig::new(failing_sink),
    |                                                   ^^^ associated function or constant not found in `serve::MctIrohConcurrentServeConfig`
    |
   ::: crates/mct-iroh/src/serve.rs:338:1
    |
338 | pub struct MctIrohConcurrentServeConfig {
    | --------------------------------------- associated function or constant `new` not found for this struct
    |
    = help: items from traits can only be used if the trait is implemented and in scope
    = note: the following traits define an item `new`, perhaps you need to implement one of them:
            candidate #1: `crypto_common::KeyInit`
            candidate #2: `crypto_common::KeyInit`
            candidate #3: `crypto_common::KeyIvInit`
            candidate #4: `crypto_common::KeyIvInit`
            candidate #5: `crypto_common::TryKeyInit`
            candidate #6: `curve25519_dalek::traits::VartimePrecomputedMultiscalarMul`
            candidate #7: `digest::block_api::VariableOutputCore`
            candidate #8: `digest::digest::Digest`
            candidate #9: `parking_lot_core::thread_parker::ThreadParkerT`
            candidate #10: `rand::distr::uniform::UniformSampler`
            candidate #11: `ring::aead::BoundKey`
            candidate #12: `typenum::marker_traits::Bit`

Some errors have detailed explanations: E0425, E0433, E0599.
For more information about an error, try `rustc --explain E0425`.
error: could not compile `mct-iroh` (lib test) due to 20 previous errors
```

Expected integration compile after making the sink mandatory and before rewiring daemon consumers:

```text
    Checking mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
    Checking mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0432]: unresolved import `mct_iroh::MctIrohHelloObservationSink`
  --> crates/mct-daemon/src/main.rs:25:35
   |
25 |     MctIrohConcurrentServeConfig, MctIrohHelloObservationSink, MctIrohServeEvent,
   |                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^ no `MctIrohHelloObservationSink` in the root
   |
help: a similar name exists in the module
   |
25 -     MctIrohConcurrentServeConfig, MctIrohHelloObservationSink, MctIrohServeEvent,
25 +     MctIrohConcurrentServeConfig, MctIrohObservationSink, MctIrohServeEvent,
   |

error[E0560]: struct `mct_iroh::MctIrohConcurrentServeConfig` has no field named `hello_observation_sink`
   --> crates/mct-daemon/src/daemon/resident.rs:338:17
    |
338 |                 hello_observation_sink: Some(hello_observation_sink),
    |                 ^^^^^^^^^^^^^^^^^^^^^^ unknown field
    |
help: a field with a similar name exists
    |
338 -                 hello_observation_sink: Some(hello_observation_sink),
338 +                 observation_sink: Some(hello_observation_sink),
    |

error[E0599]: no associated function or constant named `default` found for struct `mct_iroh::MctIrohConcurrentServeConfig` in the current scope
   --> crates/mct-daemon/src/daemon/resident.rs:339:49
    |
339 |                 ..MctIrohConcurrentServeConfig::default()
    |                                                 ^^^^^^^ associated function or constant not found in `mct_iroh::MctIrohConcurrentServeConfig`
    |
note: if you're trying to build a new `mct_iroh::MctIrohConcurrentServeConfig`, consider using `mct_iroh::MctIrohConcurrentServeConfig::new` which returns `mct_iroh::MctIrohConcurrentServeConfig`
   --> crates/mct-iroh/src/serve.rs:463:5
    |
463 |     pub fn new(observation_sink: MctIrohObservationSink) -> Self {
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

error[E0599]: no associated function or constant named `default` found for struct `mct_iroh::MctIrohConcurrentServeConfig` in the current scope
   --> crates/mct-daemon/src/daemon/ingress.rs:308:43
    |
308 |             MctIrohConcurrentServeConfig::default(),
    |                                           ^^^^^^^ associated function or constant not found in `mct_iroh::MctIrohConcurrentServeConfig`
    |
note: if you're trying to build a new `mct_iroh::MctIrohConcurrentServeConfig`, consider using `mct_iroh::MctIrohConcurrentServeConfig::new` which returns `mct_iroh::MctIrohConcurrentServeConfig`
   --> crates/mct-iroh/src/serve.rs:463:5
    |
463 |     pub fn new(observation_sink: MctIrohObservationSink) -> Self {
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

error[E0599]: no associated function or constant named `default` found for struct `mct_iroh::MctIrohConcurrentServeConfig` in the current scope
   --> crates/mct-daemon/src/daemon/ingress.rs:386:43
    |
386 |             MctIrohConcurrentServeConfig::default(),
    |                                           ^^^^^^^ associated function or constant not found in `mct_iroh::MctIrohConcurrentServeConfig`
    |
note: if you're trying to build a new `mct_iroh::MctIrohConcurrentServeConfig`, consider using `mct_iroh::MctIrohConcurrentServeConfig::new` which returns `mct_iroh::MctIrohConcurrentServeConfig`
   --> crates/mct-iroh/src/serve.rs:463:5
    |
463 |     pub fn new(observation_sink: MctIrohObservationSink) -> Self {
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

error[E0004]: non-exhaustive patterns: `mct_iroh::MctIrohServedProtocol::MalformedCall { .. }` not covered
   --> crates/mct-daemon/src/daemon/resident.rs:599:11
    |
599 |     match served {
    |           ^^^^^^ pattern `mct_iroh::MctIrohServedProtocol::MalformedCall { .. }` not covered
    |
note: `mct_iroh::MctIrohServedProtocol` defined here
   --> crates/mct-iroh/src/serve.rs:234:1
    |
234 | pub enum MctIrohServedProtocol {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
245 |     MalformedCall {
    |     ------------- not covered
    = note: the matched value is of type `mct_iroh::MctIrohServedProtocol`
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern or an explicit pattern as shown
    |
609 ~         )],
610 ~         mct_iroh::MctIrohServedProtocol::MalformedCall { .. } => todo!(),
    |

Some errors have detailed explanations: E0004, E0432, E0560, E0599.
For more information about an error, try `rustc --explain E0004`.
error: could not compile `mct-daemon` (bin "mct-daemon") due to 6 previous errors
```

Existing Slice-2 regression exposed over-broad serving-loop termination on an authority-prefix sink failure:

```text
   Compiling mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.37s
     Running unittests src/lib.rs (target/debug/deps/mct_iroh-c905e0248aa84c8b)

running 34 tests
test serve::tests::denied_hellos_leave_no_per_peer_state ... ok
test serve::tests::serve_state_ids_do_not_collide_across_instances ... ok
test identity::tests::peer_binding_signature_ref_roundtrips_and_fails_on_tamper ... ok
test tests::caller_rejects_reply_digest_mismatch_and_oversized_result ... ok
test serve::tests::admitted_hello_state_is_capped_oldest_first ... ok
test tests::call_payload_integrity_failures_are_malformed_before_authority ... ok
test tests::concurrent_call_sink_covers_success_lifecycle ... ok
test tests::endpoint_config_can_select_default_relay_mode ... ok
test tests::endpoint_config_defaults_to_local_mct_alpns ... ok
test tests::endpoint_config_rejects_empty_alpns ... ok
test tests::call_rechecks_binding_revocation_after_hello ... ok
test tests::exposes_version ... ok
test tests::call_rechecks_binding_policy_revision_after_hello ... ok
test tests::call_rechecks_narrowed_alpn_scope_after_hello ... ok
test tests::call_rechecks_narrowed_vision_scope_after_hello ... ok
test tests::call_payload_roundtrip_carries_request_and_result_bytes ... ok
test tests::concurrent_serve_refuses_connections_beyond_bound ... ok
test tests::call_rechecks_binding_expiry_after_hello ... ok
test tests::node_secret_key_file_is_created_owner_read_write_only ... ok
test tests::call_payload_caps_fail_closed ... ok
test tests::mother_owned_endpoint_starts_and_closes ... ok
test tests::concurrent_serve_keeps_peer_hello_state_separate ... ok
test tests::mother_endpoint_ticket_connects_hello_then_call ... ok
test tests::local_iroh_completes_mct_hello_then_call ... ok
test tests::iroh_call_handler_can_complete_reply_after_runtime_execution ... ok
test tests::serve_next_denies_binding_expired_against_current_accept_time ... ok
test tests::iroh_adapter_observations_cover_endpoint_and_protocol_events ... ok
test tests::concurrent_serve_requires_signed_peer_binding_when_configured ... ok
test tests::unknown_peer_is_denied_before_call ... ok
test tests::malformed_frames_are_observed_before_safe_reply_and_append_failure_suppresses_reply ... ok
test tests::call_frame_budget_refuses_oversized_request ... ok
test tests::send_hello_times_out_when_peer_never_replies ... ok
test tests::serve_next_times_out_when_peer_never_sends_data ... ok
test tests::failed_hello_observation_prevents_response_and_remembered_admission ... FAILED

failures:

---- tests::failed_hello_observation_prevents_response_and_remembered_admission stdout ----

thread 'tests::failed_hello_observation_prevents_response_and_remembered_admission' (1073238) panicked at crates/mct-iroh/src/lib.rs:777:67:
called `Result::unwrap()` on an `Err` value: ProtocolTimeout { action: "complete outbound MCT roundtrip" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    tests::failed_hello_observation_prevents_response_and_remembered_admission

test result: FAILED. 33 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 31.33s

error: test failed, to rerun pass `-p mct-iroh --lib`
```

Expected red full-lifecycle assertion exposed the existing route path's multiple revalidation facts and actual ordering:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 4.26s
     Running unittests src/main.rs (target/debug/deps/mct_daemon-701d058281c133f0)

running 1 test
mct resident mother endpoint_id=4b1dcfb4692e3c76639409c5f12822a22943677d00b2c8029d33c9a6a1be6007
ticket={  "endpoint_id": "4b1dcfb4692e3c76639409c5f12822a22943677d00b2c8029d33c9a6a1be6007",  "direct_addresses": [    "10.10.10.209:58908",    "100.114.124.29:58908"  ],  "relay_urls": []}
mct resident mother children loaded=1 failed=0 bindings=1 max_connections=8
mct daemon serving control uds on /var/folders/6h/329275913d1d3k1lfvvvryp40000gn/T/.tmp9jNduM/control.sock

thread 'resident::tests::resident_mother_payload_roundtrip_verifies_result_digest' (1083056) panicked at crates/mct-daemon/src/daemon/resident.rs:4417:9:
assertion `left == right` failed
  left: [PeerCallReceived, CallConstructed, CallAuthorized, RouteRevalidated, RouteSelected, RouteRevalidated, RouteRevalidated, ResultRecorded, PeerCallReplied]
 right: [PeerCallReceived, CallConstructed, CallAuthorized, RouteSelected, RouteRevalidated, ResultRecorded, PeerCallReplied]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test resident::tests::resident_mother_payload_roundtrip_verifies_result_digest ... FAILED

failures:

failures:
    resident::tests::resident_mother_payload_roundtrip_verifies_result_digest

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 40 filtered out; finished in 2.96s

error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

Standalone integration fixture visibility compile failure before correction:

```text
   Compiling mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0603]: function `write_resident_process_child` is private
    --> crates/mct-daemon/src/daemon/ingress.rs:1016:33
     |
1016 |         crate::resident::tests::write_resident_process_child(&children_dir);
     |                                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ private function
     |
note: the function `write_resident_process_child` is defined here
    --> crates/mct-daemon/src/daemon/resident.rs:5688:5
     |
5688 |     pub(super) fn write_resident_process_child(children_dir: &Path) {
     |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

For more information about this error, try `rustc --explain E0603`.
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 1 previous error
```

## S3.2 implementation record

### Commits

- `fa0e515` — `docs: specify peer call lifecycle observations`
- `fd3cd3d` — `fix(iroh): observe peer call lifecycle`

### Lifecycle outcome

The one serving sink now carries typed hello and call batches. `MctIrohConcurrentServeConfig::new(sink)` requires it, and the single-connection serving APIs also require the same sink explicitly; no sinkless serving default remains.

The durable per-call prefix is `PeerCallReceived` followed by either `PeerCallMalformed`, or `CallConstructed` plus `CallAuthorized`/`CallDenied`. Result-producing valid calls then append `ResultRecorded`; the post-send fact is separately `PeerCallReplied`. Existing handler-owned `RouteSelected`, `RouteRevalidated`, `NoRouteRecorded`, and runtime facts remain untouched and unduplicated. The real resident success test reconstructs this local evidence order:

```text
PeerCallReceived
CallConstructed
CallAuthorized
RouteRevalidated
RouteSelected
RouteRevalidated
RouteRevalidated
ResultRecorded
PeerCallReplied
```

The multiple revalidation entries are existing distinct authority/effect-boundary facts, not lifecycle-sink duplication.

### Regression evidence

- `malformed_frames_are_observed_before_safe_reply_and_append_failure_suppresses_reply`: undecodable and oversized frames produce receipt/malformed/reply stages and caller-safe malformed replies; injected prefix append failure produces no response.
- `concurrent_call_sink_covers_success_lifecycle`: successful transport sequence includes distinct receipt, construction, authorization, result, and reply stages.
- `denied_call_fact_is_recorded_before_reply`: denial is acknowledged by the sink before the caller receives its denial response.
- `resident_mother_payload_roundtrip_verifies_result_digest`: one real resident call reconstructs the complete transport + route lifecycle from the JSONL ledger, verifies result/reply buffered durability, and excludes payload bytes, base64, and binding key material.
- `standalone_serve_process_persists_hello_and_call_lifecycle`: standalone process serving uses the mandatory ledger sink for A5 hello ordering and A8 call facts, excluding node secret material.
- `standalone_serve_refuses_held_ledger_before_endpoint_bind`: lock contention returns the specified refusal before identity creation or endpoint bind.

### Discarded-write enumeration outcome

All five serve-process ledger/state discards were removed. Authority and process-report ledger writes now await the lifetime writer; authority/run-observation projection failures return before child invocation where possible; report projection and completion failures prevent a success reply after execution. `rg` finds no discarded `append_ledger_observations`, `append_run_observations`, or `complete_run` call on the standalone path. None was justified as fail-open.

### Validation and flakes

Both slice commits passed the required workspace tests, Clippy with `-D warnings`, and tier-0. Final implementation validation passed **264 tests** (264 passed, 0 ignored), and Allium check returned empty diagnostics/findings.

Flakes: none. Expected red compile/test failures and deterministic integration corrections are captured verbatim above.

A8 is fixed in `fd3cd3d`; A5 now covers resident and standalone serving. Slices 4–6 remain untouched. Stop after S3.2.

# Slice 4 — observed child authority, identity, and storage mutations

Starting state: `patina` at `86ea6d5` (`docs: close peer call lifecycle observations`), clean tree. Allium 3.5.0 check returned no diagnostics or findings. The operator resolved the identity fork as offline-only: a bound Mother never changes identity; first bootstrap and stopped-daemon administration hold the one ledger writer and durably record public identity before key/config effects.

## S4.1 mutation enumeration and mechanism specification

### Complete daemon/CLI mutation enumeration

| Command or path | Mutation | Disposition |
|---|---|---|
| `children approve` | child approval plus active assignment in config | **observed-in-this-slice** through live resident UDS or lock-protected offline execution |
| `children revoke` | child approval plus assignment revocation in config | **observed-in-this-slice** through live resident UDS or lock-protected offline execution |
| resident first identity bootstrap | local node identity config and, when absent, node secret-key file | **observed-in-this-slice, offline-only bootstrap ordering**: open resident writer, append public identity fact, then write key/config |
| `iroh identity` | local node identity config and, when absent, node secret-key file | **observed-in-this-slice, offline-only**: connected resident refuses; unavailable UDS permits exclusive-lock execution |
| `registry install` | verified child package publication/replacement under the children directory | **observed-in-this-slice** through resident UDS or lock-protected offline execution |
| `registry sync` | verified/rejected artifact candidates and registry-source status in runtime state | **observed-in-this-slice** through resident UDS or lock-protected offline execution |
| `toys authorize-slate` | canonical toy-contract and scoped ToyGrant snapshots in runtime state | **observed-in-this-slice** through resident UDS or lock-protected offline execution |
| `toys authorize-secret` | secrets toy contract and scoped ToyGrant snapshot in runtime state | **observed-in-this-slice** through resident UDS or lock-protected offline execution |
| `pando record` | operator-authored composition plan in runtime state | **observed-in-this-slice** through resident UDS or lock-protected offline execution |
| UDS `POST /blobs` | content-addressed blob publication | **observed-in-this-slice**, resident UDS only; there is no offline blob CLI/path |
| `peers add`, `set-outbound-proof`, `revoke`, `remove` | peer authority config | **already observed** in `393884f` through the same resident/offline seam |
| `children warmup`, `children reload` | projected authority records and child instance generations in runtime state | **already observed** by their child approval/assignment/lifecycle report observations; reload effect ordering remains A7/Slice 5 |
| `process call`, `wasm call`, `wasm call-wit`, `slate list-work`, resident calls, and standalone Iroh serving | run state, invocation state, and adapter effects | **already observed** by before-effect authority observations and typed runtime/toy/serve lifecycle observations (Slices 1–3 included) |
| `iroh call`, `iroh call-peer`, JVM call ingress | call/transport/runtime effects | **already observed** by their call, authority, transport, route, result, and runtime facts |
| `children load` | filesystem discovery only; no config, authority, CAS, or runtime-state write | **justified out-of-scope** pure read/discovery projection |
| `children approvals`, `peers list`, `registry/federation/state/runs/metrics` read surfaces | none | **justified out-of-scope** pure reads/projections |
| control socket/identity-file cleanup and test-fixture filesystem writes | transport lifecycle or tests, not product authority/storage commands | **justified out-of-scope** |

There is no independent child-assignment command, toy-grant revoke command, general policy editor, or blob CLI. Public config/state methods without a daemon or CLI command caller are not additional command paths.

### UDS commands, budgets, and safe facts

All JSON mutation bodies are bounded to 64 KiB except `/blobs`, whose existing bounded HTTP read budget accommodates the 8 MiB decoded blob cap. Every request names the expected resident-owned path(s); a mismatch is rejected rather than mutating a different store.

| UDS route | Request facts | Typed observation mapping and safe facts |
|---|---|---|
| `POST /children/approve` | expected config/children paths, child name, strict-integrity flag | `ChildApproved` and `ChildAssigned`; child, artifact and assignment ids, policy revision |
| `POST /children/revoke` | expected config path, child name | `ChildRevoked` and `ChildAssignmentRevoked`; child, artifact and assignment ids, policy revision |
| `POST /identity/ensure` | none accepted for mutation while resident is bound | connected resident returns conflict: `stop the daemon to create or rotate identity`; no fallback or effect |
| `POST /registry/install` | expected children path, verified package source path, replace flag | `ArtifactVerified` before publication; `StorageAppendSucceeded` after publication or `StorageAppendFailed` on apply failure; child, artifact id/version and destination only |
| `POST /registry/sync` | expected state/children paths, source id, strict-integrity flag | `ArtifactVerified`/`ArtifactRejected` for discovered packages plus `OperatorActionRecorded`; source, child/artifact ids, counts; no package bytes |
| `POST /toys/authorize-slate` | expected config/state/children paths, child, project root | one `ToyGrantAllowed` per grant before snapshots; child, toy/grant/resource ids and policy/grants revisions |
| `POST /toys/authorize-secret` | expected config/state/children paths, child, secret name | `ToyGrantAllowed`; child, toy/grant ids and revisions; the secret name may be a scoped resource id but no secret value or key material is recorded |
| `POST /pando/record` | expected state path, composition id and typed steps | `OperatorActionRecorded`; composition id and step count, without step payloads |
| `POST /blobs` | digest, declared size, content type/classification, base64 transport body | `AdapterEffectStarted` before CAS publication, then `StorageAppendSucceeded`; validation rejection uses `StorageAppendFailed` with typed reason (`digest_mismatch`, `size_mismatch`, `oversize`, invalid encoding/digest/content type) |

Identity uses `OperatorActionRecorded` with node id as subject and public endpoint/public-key identity as resource. It never records secret key bytes, hex/base64 encodings, file contents, or private-key material. Other observations likewise omit payload bytes, base64 bodies, package contents, peer proofs, and secrets.

### Ordering and failures

Each resident handler performs: bound and decode → validate against current stores/filesystem → construct typed decision facts → await the resident writer's `BeforeEffect` acknowledgement → apply the config/state/storage replacement → append a typed success effect where required → return success. A decision append failure leaves config/state/CAS/package visibility untouched. An apply failure after the durable decision appends `StorageAppendFailed` or failed `OperatorActionRecorded` and returns failure; it never reports success.

Offline-capable CLI commands first attempt the resident UDS. A connected response, including rejection, is authoritative and never falls back. Connection failure alone permits opening `JsonlObservationLedger` as exclusive writer; the lock is held across validation, decision append, effect, and failure/success append. Contention refuses without mutation. Blob ingest has no offline path.

Resident bootstrap is the identity exception to UDS dispatch because no endpoint is bound yet: `ResidentLedgerWriter::spawn` does not consume identity facts (it opens fixed local ledger identity `ledger-local`/`local-mct`), so there is no circularity. Bootstrap opens the writer first, prepares or reads key material in memory, appends the public identity decision, then writes a new key if needed and replaces config. Append failure aborts startup before either file exists. `iroh identity` follows the same preparation under the offline writer lock. Live node identity rotation is not implemented.

### Mutation visibility

Resident child candidate sourcing calls `authorize_resident_child_blocking` for each call. That function reloads `MctDaemonConfigStore` from the same `config_path`, reloads children from the same `children_dir`, and derives `authority_projection_for_loaded_children`; there is no approval or assignment cache. Toy authorization likewise opens the same runtime state for current grant snapshots. Therefore a successful UDS response follows store replacement, and the immediately following call sees approval, revocation, package, and grant changes. Since decision appends precede replacement, effects cannot become authoritative before their facts.

### Out of scope

- A7/Slice 5 child generation reload ordering.
- A3/Slice 6 idempotency.
- Operator identity/authentication beyond UDS filesystem permissions (C1 future).
- New observation kinds beyond those required above.
- Live node identity rotation; it requires endpoint rebind and peer re-admission design and is recorded in the ROADMAP standing backlog.

## Slice 4 checklist

- [x] Enumerate every daemon/CLI mutation path and assign a disposition.
- [ ] Land failing child-authority, identity, storage, and additional-subsystem regressions before each implementation seam.
- [ ] Implement child authority plus offline-only identity ordering.
- [ ] Implement observed blob and registry storage mutations.
- [ ] Implement observed toy-grant and composition-state administration.
- [ ] Mark A6 fully fixed, validate each commit, and stop after S4.2.

## Slice 4 failure log

Expected red compile after adding Slice 4 child-authority and identity regressions:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0425]: cannot find function `ensure_observed_local_identity` in this scope
    --> crates/mct-daemon/src/daemon/resident.rs:2767:21
     |
2767 |         let error = ensure_observed_local_identity(
     |                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find function `resident_authority_mutation_handler` in this scope
   --> crates/mct-daemon/src/daemon/control.rs:913:23
    |
390 | / pub(super) fn resident_peer_mutation_handler(
391 | |     configured_path: PathBuf,
392 | |     ledger: ResidentLedgerWriter,
393 | | ) -> mct_daemon::MctUdsControlMutationHandler {
...   |
398 | |     })
399 | | }
    | |_- similarly named function `resident_peer_mutation_handler` defined here
...
913 |           let handler = resident_authority_mutation_handler(
    |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
help: a function with a similar name exists
    |
913 -         let handler = resident_authority_mutation_handler(
913 +         let handler = resident_peer_mutation_handler(
    |

error[E0425]: cannot find function `resident_authority_mutation_handler` in this scope
    --> crates/mct-daemon/src/daemon/control.rs:1002:23
     |
 390 | / pub(super) fn resident_peer_mutation_handler(
 391 | |     configured_path: PathBuf,
 392 | |     ledger: ResidentLedgerWriter,
 393 | | ) -> mct_daemon::MctUdsControlMutationHandler {
...    |
 398 | |     })
 399 | | }
     | |_- similarly named function `resident_peer_mutation_handler` defined here
...
1002 |           let handler = resident_authority_mutation_handler(
     |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
     |
help: a function with a similar name exists
     |
1002 -         let handler = resident_authority_mutation_handler(
1002 +         let handler = resident_peer_mutation_handler(
     |

error[E0425]: cannot find function `execute_offline_child_mutation` in this scope
    --> crates/mct-daemon/src/daemon/control.rs:1034:9
     |
 401 | / pub(super) fn execute_offline_peer_mutation(
 402 | |     configured_path: &Path,
 403 | |     ledger_path: &Path,
 404 | |     path: &str,
...    |
 429 | | }
     | |_- similarly named function `execute_offline_peer_mutation` defined here
...
1034 |           execute_offline_child_mutation(
     |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
     |
help: a function with a similar name exists
     |
1034 -         execute_offline_child_mutation(
1034 +         execute_offline_peer_mutation(
     |

error[E0425]: cannot find function `execute_offline_identity_mutation` in this scope
    --> crates/mct-daemon/src/daemon/control.rs:1048:9
     |
 401 | / pub(super) fn execute_offline_peer_mutation(
 402 | |     configured_path: &Path,
 403 | |     ledger_path: &Path,
 404 | |     path: &str,
...    |
 429 | | }
     | |_- similarly named function `execute_offline_peer_mutation` defined here
...
1048 |           execute_offline_identity_mutation(&config_path, &identity_path, &ledger_path).unwrap();
     |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
     |
help: a function with a similar name exists
     |
1048 -         execute_offline_identity_mutation(&config_path, &identity_path, &ledger_path).unwrap();
1048 +         execute_offline_peer_mutation(&config_path, &identity_path, &ledger_path).unwrap();
     |

error[E0425]: cannot find function `execute_offline_identity_mutation` in this scope
    --> crates/mct-daemon/src/daemon/control.rs:1066:21
     |
 401 | / pub(super) fn execute_offline_peer_mutation(
 402 | |     configured_path: &Path,
 403 | |     ledger_path: &Path,
 404 | |     path: &str,
...    |
 429 | | }
     | |_- similarly named function `execute_offline_peer_mutation` defined here
...
1066 |           let error = execute_offline_identity_mutation(
     |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
     |
help: a function with a similar name exists
     |
1066 -         let error = execute_offline_identity_mutation(
1066 +         let error = execute_offline_peer_mutation(
     |

For more information about this error, try `rustc --explain E0425`.
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 6 previous errors
```

Deterministic child CLI ownership compile failure before correction:

```text
   Compiling mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0382]: borrow of moved value: `children_dir`
   --> crates/mct-daemon/src/daemon/cli_runtime.rs:116:9
    |
100 |     let children_dir = args
    |         ------------ move occurs because `children_dir` has type `std::path::PathBuf`, which does not implement the `Copy` trait
...
104 |     let mut options = MctChildLoadOptions::new(children_dir);
    |                                                ------------ value moved here
...
116 |         &children_dir,
    |         ^^^^^^^^^^^^^ value borrowed here after move
    |
    = note: borrow occurs due to deref coercion to `std::path::Path`
help: consider borrowing `children_dir`
    |
104 |     let mut options = MctChildLoadOptions::new(&children_dir);
    |                                                +

error[E0505]: cannot move out of `children_dir` because it is borrowed
   --> crates/mct-daemon/src/daemon/cli_runtime.rs:122:36
    |
100 |     let children_dir = args
    |         ------------ binding `children_dir` declared here
...
114 |     let response = execute_cli_child_mutation(
    |                    -------------------------- borrow later used by call
115 |         &config_path,
116 |         &children_dir,
    |         ------------- borrow of `children_dir` occurs here
...
122 |             expected_children_dir: children_dir,
    |                                    ^^^^^^^^^^^^ move out of `children_dir` occurs here
    |
help: consider cloning the value if the performance cost is acceptable
    |
116 |         &children_dir.clone(),
    |                      ++++++++

Some errors have detailed explanations: E0382, E0505.
For more information about an error, try `rustc --explain E0382`.
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 2 previous errors
```

Initial composition assertion used the wrong field for the typed elimination class:

```text
running 1 test
test control::tests::live_child_revocation_is_visible_to_the_immediately_following_route ... FAILED

failures:

---- control::tests::live_child_revocation_is_visible_to_the_immediately_following_route stdout ----

thread 'control::tests::live_child_revocation_is_visible_to_the_immediately_following_route' (1251367) panicked at crates/mct-daemon/src/daemon/control.rs:1527:9:
assertion failed: denial_observations.iter().any(|observation|
        {
            observation.kind == ObservationKind::CandidateEliminated &&
                observation.detail_ref.as_deref().is_some_and(|detail|
                        { detail.contains("ChildNotApproved") })
        })
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    control::tests::live_child_revocation_is_visible_to_the_immediately_following_route

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 47 filtered out; finished in 0.05s

error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

Detailed deterministic routing failure showing revocation collapsed into readiness before correction:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.21s
     Running unittests src/main.rs (target/debug/deps/mct_daemon-701d058281c133f0)

running 1 test

thread 'control::tests::live_child_revocation_is_visible_to_the_immediately_following_route' (1253641) panicked at crates/mct-daemon/src/daemon/control.rs:1527:9:
[
    MctObservation {
        observation_id: ObservationId(
            "obs-route-candidate-considered:call-resident-wit:child:resident-echo",
        ),
        observed_at: Timestamp {
            value: "2026-07-10T20:22:34.027041Z",
            epoch_nanoseconds: 1783714954027041000,
        },
        kind: CandidateConsidered,
        source_plane: Kernel,
        trace: ObservationTraceRef {
            trace_id: TraceId(
                "trace-live-child-revoke",
            ),
            span_id: Some(
                SpanId(
                    "span-cli-wasm",
                ),
            ),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(
            CallId(
                "call-resident-wit",
            ),
        ),
        decision_id: None,
        subject_id: Some(
            "resident-echo",
        ),
        resource_id: Some(
            "child:resident-echo",
        ),
        policy_revision: Some(
            1,
        ),
        grants_revision: Some(
            1,
        ),
        outcome: Informational,
        visibility: InternalOnly,
        safe_message: "candidate considered",
        detail_ref: Some(
            "candidate:child:resident-echo;node:local-mct;runtime:Process;network:Local",
        ),
    },
    MctObservation {
        observation_id: ObservationId(
            "obs-route-candidate-eliminated:call-resident-wit:child:resident-echo",
        ),
        observed_at: Timestamp {
            value: "2026-07-10T20:22:34.027047Z",
            epoch_nanoseconds: 1783714954027047000,
        },
        kind: CandidateEliminated,
        source_plane: Kernel,
        trace: ObservationTraceRef {
            trace_id: TraceId(
                "trace-live-child-revoke",
            ),
            span_id: Some(
                SpanId(
                    "span-cli-wasm",
                ),
            ),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(
            CallId(
                "call-resident-wit",
            ),
        ),
        decision_id: None,
        subject_id: Some(
            "resident-echo",
        ),
        resource_id: Some(
            "child:resident-echo",
        ),
        policy_revision: Some(
            1,
        ),
        grants_revision: Some(
            1,
        ),
        outcome: Denied,
        visibility: InternalOnly,
        safe_message: "not authorized",
        detail_ref: Some(
            "elimination_reason:CapabilityUnavailable;denial_class:temporal",
        ),
    },
    MctObservation {
        observation_id: ObservationId(
            "obs:authorize:call-resident-wit:resident-echo",
        ),
        observed_at: Timestamp {
            value: "2026-07-10T20:22:34.027051Z",
            epoch_nanoseconds: 1783714954027051000,
        },
        kind: CallDenied,
        source_plane: Kernel,
        trace: ObservationTraceRef {
            trace_id: TraceId(
                "trace-live-child-revoke",
            ),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(
            CallId(
                "call-resident-wit",
            ),
        ),
        decision_id: Some(
            DecisionId(
                "decision:call-resident-wit:resident-echo",
            ),
        ),
        subject_id: Some(
            "resident-echo",
        ),
        resource_id: Some(
            "instance:resident-echo:1",
        ),
        policy_revision: Some(
            1,
        ),
        grants_revision: None,
        outcome: Denied,
        visibility: InternalOnly,
        safe_message: "not authorized",
        detail_ref: Some(
            "child_call_reason:InstanceNotReady",
        ),
    },
    MctObservation {
        observation_id: ObservationId(
            "obs-route-initial:call-resident-wit",
        ),
        observed_at: Timestamp {
            value: "2026-07-10T20:22:34.027062Z",
            epoch_nanoseconds: 1783714954027062000,
        },
        kind: NoRouteRecorded,
        source_plane: Kernel,
        trace: ObservationTraceRef {
            trace_id: TraceId(
                "trace-live-child-revoke",
            ),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(
            CallId(
                "call-resident-wit",
            ),
        ),
        decision_id: Some(
            DecisionId(
                "route-initial:call-resident-wit",
            ),
        ),
        subject_id: None,
        resource_id: None,
        policy_revision: Some(
            1,
        ),
        grants_revision: Some(
            1,
        ),
        outcome: Denied,
        visibility: InternalOnly,
        safe_message: "not authorized",
        detail_ref: Some(
            "no_route_reason:CapabilityUnavailable",
        ),
    },
]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test control::tests::live_child_revocation_is_visible_to_the_immediately_following_route ... FAILED

failures:

failures:
    control::tests::live_child_revocation_is_visible_to_the_immediately_following_route

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 47 filtered out; finished in 0.03s

error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

Deterministic Clippy finding before grouping typed mutation fact fields:

```text
error: this function has too many arguments (9/7)
   --> crates/mct-daemon/src/daemon/control.rs:468:1
    |
468 | / fn mutation_observation(
469 | |     namespace: &str,
470 | |     kind: ObservationKind,
471 | |     subject_id: String,
...   |
477 | |     safe_message: String,
478 | | ) -> MctObservation {
    | |___________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#too_many_arguments
    = note: `-D clippy::too-many-arguments` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::too_many_arguments)]`

error: could not compile `mct-daemon` (bin "mct-daemon") due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 1 previous error
```

Expected red compile after adding observed blob regressions:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0425]: cannot find function `resident_observed_mutation_handler` in this scope
    --> crates/mct-daemon/src/daemon/control.rs:1585:23
     |
 391 | / pub(super) fn resident_peer_mutation_handler(
 392 | |     configured_path: PathBuf,
 393 | |     ledger: ResidentLedgerWriter,
 394 | | ) -> mct_daemon::MctUdsControlMutationHandler {
...    |
 399 | |     })
 400 | | }
     | |_- similarly named function `resident_peer_mutation_handler` defined here
...
1585 |           let handler = resident_observed_mutation_handler(
     |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
     |
help: a function with a similar name exists
     |
1585 -         let handler = resident_observed_mutation_handler(
1585 +         let handler = resident_peer_mutation_handler(
     |

error[E0425]: cannot find function `resident_observed_mutation_handler` in this scope
    --> crates/mct-daemon/src/daemon/control.rs:1672:23
     |
 391 | / pub(super) fn resident_peer_mutation_handler(
 392 | |     configured_path: PathBuf,
 393 | |     ledger: ResidentLedgerWriter,
 394 | | ) -> mct_daemon::MctUdsControlMutationHandler {
...    |
 399 | |     })
 400 | | }
     | |_- similarly named function `resident_peer_mutation_handler` defined here
...
1672 |           let handler = resident_observed_mutation_handler(
     |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
     |
help: a function with a similar name exists
     |
1672 -         let handler = resident_observed_mutation_handler(
1672 +         let handler = resident_peer_mutation_handler(
     |

For more information about this error, try `rustc --explain E0425`.
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 2 previous errors
```

Expected red registry mutation regression before route implementation:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.27s
     Running unittests src/main.rs (target/debug/deps/mct_daemon-701d058281c133f0)

running 1 test
test control::tests::live_registry_install_and_sync_are_observed_before_storage_effects ... FAILED

failures:

---- control::tests::live_registry_install_and_sync_are_observed_before_storage_effects stdout ----

thread 'control::tests::live_registry_install_and_sync_are_observed_before_storage_effects' (1273523) panicked at crates/mct-daemon/src/daemon/control.rs:1792:9:
assertion `left == right` failed: {"error":"peer mutation rejected"}
  left: 400
 right: 200
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    control::tests::live_registry_install_and_sync_are_observed_before_storage_effects

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 53 filtered out; finished in 0.01s

error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

Registry integration fixture initially requested strict re-verification after the existing installer intentionally copied only installable package files, so sync recorded zero candidates:

```text
running 1 test
test control::tests::live_registry_install_and_sync_are_observed_before_storage_effects ... FAILED

failures:

---- control::tests::live_registry_install_and_sync_are_observed_before_storage_effects stdout ----

thread 'control::tests::live_registry_install_and_sync_are_observed_before_storage_effects' (1275191) panicked at crates/mct-daemon/src/daemon/control.rs:2132:9:
assertion `left == right` failed
  left: 0
 right: 1
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    control::tests::live_registry_install_and_sync_are_observed_before_storage_effects

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 53 filtered out; finished in 0.09s

error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

The corrected assertion exposed that the fixture passed the package parent rather than the package root, producing a nested install that registry discovery correctly ignored:

```text
assertion `left == right` failed: {"failed":0,"load_report":{"children":[],"children_dir":".../children","discovered":0,"failed":0,"failures":[],"loaded":0},"loaded":0,"source_id":"resident-registry","source_path":".../children"}
  left: 0
 right: 1
```

Deterministic Clippy finding after resident wiring moved to the storage-capable handler:

```text
error: function `resident_authority_mutation_handler` is never used
    --> crates/mct-daemon/src/daemon/control.rs:1222:15
     |
1222 | pub(super) fn resident_authority_mutation_handler(
     |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
     |
     = note: `-D dead-code` implied by `-D warnings`
     = help: to override `-D warnings` add `#[expect(dead_code)]` or `#[allow(dead_code)]`

error: could not compile `mct-daemon` (bin "mct-daemon") due to 1 previous error
warning: build failed, waiting for other jobs to finish...
```

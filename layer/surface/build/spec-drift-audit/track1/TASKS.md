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
- [ ] S1.2: cover revoked, expired, stale-revision, narrowed-scope, unchanged, and forwarding paths.
- [ ] S1.2: update A2 in the audit report with `fixed` and the implementation commit.
- [ ] S1.2: complete per-commit validation and stop.

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

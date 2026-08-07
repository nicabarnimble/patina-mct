---
type: feat
id: grants-authority-v0
status: active
created: 2026-08-02
target: mct-grants-authority-v0-phase-g
sessions:
  origin: 20260724-223731-101286000
  work: []
related:
  - layer/allium/mct-product-map.allium
  - layer/allium/mct-peer-ontology.allium
  - layer/surface/build/spec-drift-audit/track3/LEDGER.md
  - layer/core/safety-boundaries.md
  - layer/core/spec-driven-design.md
  - crates/mct-daemon/src/daemon/resident/execution.rs
  - crates/mct-daemon/src/toy.rs
  - crates/mct-daemon/src/wasm.rs
  - crates/mct-kernel/src/toy.rs
beliefs:
  - authority-freshness-requires-mother-state
  - mother-kernel-decides-adapters-perform
exit_criteria:
  - id: exact-token-call-binding
    text: Process, all three WASM invocation paths, and Toy effect admission deny a token whose call_id differs from the supplied call before any backend effect.
    checked: true
    verify: Required proof steps 1-3 below have landed test file and line citations.
  - id: toy-token-expiry
    text: Every Toy effect admission uses the executing Mother's clock; at-or-after token expiry denies without a backend effect and before expiry proceeds.
    checked: true
    verify: Required proof steps 4-5 below have landed test file and line citations.
  - id: bounded-effective-deadline
    text: Ingress computes an effective deadline from the caller deadline, a configurable 600-second default maximum horizon, and any stricter local policy bound; WASM and Toy authorization consume only that bound.
    checked: true
    verify: Required proof steps 6-9 below have landed test file and line citations.
  - id: allium-law
    text: The product map records grants-authority identity, provenance separation, peer echo, effect admission, clock/deadline, delegated-capability, and projection-freshness law, and validates without diagnostics.
    checked: true
    verify: allium check layer/allium
  - id: attribution-ledger
    text: Every new invariant is attributed in the Track 3 ledger as targeted COVERED-by-this-phase or DEFERRED to Review 2 and slices 4-8.
    checked: true
    verify: rg -n "Grants authority v0|GrantGenerationIsMotherOwned|ExecutionTokenBindsExactCall|ToyTokenExpiryIsEnforced|CallerDeadlineCannotExtendLocalHorizon" layer/surface/build/spec-drift-audit/track3/LEDGER.md
  - id: workspace-validation
    text: Every implementation commit and the final phase pass the required workspace validation suite.
    checked: true
    verify: cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
  - id: phase-i-envelope-completeness
    text: Every production mutation of the D-R2.7 Toy catalog/grant/Watch-scope authority commits exactly one canonical mutation/import fact and advances the namespaced generation exactly once.
    checked: true
    verify: Phase I proof steps 1-2 and 15-17 have landed test file and line citations.
  - id: phase-i-local-snapshot
    text: Route evaluation receives one Mother-owned local execution authority snapshot only after exact D-G8 proof, with canonical grants, labeled local policy provenance, Mother clock, and cursor provenance.
    checked: true
    verify: Phase I proof steps 3-4, 6-7, and 11-13 have landed test file and line citations.
  - id: phase-i-route-evaluation
    text: Resident route evaluation uses the local snapshot rather than caller-echoed revisions, records those echoes only as correlation evidence, and fails closed on unprovable freshness.
    checked: true
    verify: Phase I proof steps 5 and 8-10 have landed test file and line citations.
  - id: phase-i-deferral-fence
    text: Token minting revision sourcing, the resident effect guard, Child/Toy/WASM/process effect guards, hello/peer wire, idempotent replay, and MotherAuthorityOrderV1 production adoption remain unchanged.
    checked: true
    verify: Phase I proof step 14 plus the close-out changed-reader and call-site audits.
  - id: phase-i-validation
    text: Every Phase I implementation commit and the final close-out pass workspace tests, warnings-denied clippy, Tier 0/RustSec, Allium, and the grants-authority spec check.
    checked: true
    verify: cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh && allium check layer/allium
  - id: phase-j-local-minting
    text: Route, Child, and Toy execution authority carries the complete grants-authority identity from the evaluating local snapshot, exact call/effect identity, and the effective deadline; caller echoes cannot influence minting.
    checked: true
    verify: Phase J proof steps 1 and 13 have landed test file and line citations.
  - id: phase-j-child-effect-boundary
    text: Resident process and all three WASM Child starts require a fresh proof-gated current read, exact token comparison, and one MotherAuthorityOrderV1 admission; stale, expired, fenced, or unprovable authority starts no adapter effect.
    checked: true
    verify: Phase J proof steps 2-6, 11-12, and 15 have landed test file and line citations.
  - id: phase-j-toy-effect-boundary
    text: Every Toy backend and delegated-capability admission revalidates current generation, exact grant state/scope, grant and token time bounds, and supported live consumption state before ordered effect start.
    checked: true
    verify: Phase J proof steps 7-10, 12-13, and 15-16 have landed test file and line citations.
  - id: phase-j-production-order
    text: Canonical control-plane authority mutations use MotherAuthorityOrderV1::commit_mutation and final Child/Toy adapter starts use its single-use admit_effect handoff without a second order or cross-file transaction claim.
    checked: true
    verify: Phase J proof steps 4-6 and 14-15 have landed test file and line citations.
  - id: phase-j-pin-retirement
    text: Every Phase I proof-14 pin is replaced by named Phase J behavior evidence or retained as an explicit slice-6 or Review-3 residue; no unrelated pin failure is accepted as cleanup.
    checked: true
    verify: Phase J pin-retirement map and proof step 14 are complete.
  - id: phase-j-validation
    text: Every Phase J implementation commit and final close-out pass workspace tests, warnings-denied clippy, Tier 0/RustSec, Allium, grants-authority spec check, and diff check under the recorded flake protocol.
    checked: true
    verify: cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh && allium check layer/allium
  - id: phase-k-hello-advertisement
    text: An admitted hello advertises the receiving Mother's complete current proof-gated grants-authority identity; an unprovable receiver advertises no usable identity and refuses typed.
    checked: true
    verify: Phase K proof steps 1 and 8 have landed test file and line citations.
  - id: phase-k-early-stale-rejection
    text: Peer and local ingress first reject disagreeing expected-identity copies as malformed, then compare one consistent complete expected receiver identity with a fresh local identity before route evaluation; mismatch is a typed temporal rejection with correlation evidence, while agreement grants nothing.
    checked: true
    verify: Phase K proof steps 2-5 and 11 have landed test file and line citations.
  - id: phase-k-wire-and-internal-migration
    text: The legacy call-carried grants_revision is replaced according to the ratified consumer inventory, forwarding echoes the admitted hello identity, and internal Child-originated calls source the local current identity rather than parent-carried values.
    checked: true
    verify: Phase K proof steps 6-7 and 9 have landed test file and line citations.
  - id: phase-k-no-authority-widening
    text: Matching, forged, absent, malformed, future, or stale expected receiver identities never skip or alter Phase I evaluation or Phase J effect admission; the peer echo can only reject earlier.
    checked: true
    verify: Phase K proof steps 3-5 and 10 have landed test file and line citations.
  - id: phase-k-terminal-ledger
    text: All four remaining grants-authority Track 3 rows and audit item M2d are terminal with cited production-path proofs and no grants-authority row remains non-terminal.
    checked: true
    verify: Phase K close-out ledger delta and audit disposition.
  - id: phase-k-validation
    text: Every Phase K implementation commit and final close-out pass workspace tests, warnings-denied clippy, Tier 0/RustSec, Allium, grants-authority spec check, and diff check under the recorded flake protocol.
    checked: true
    verify: cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh && allium check layer/allium
---

# feat: MCT grants-authority v0 — Phase G

> The executing Mother derives effect authority only from coherent local state, while independent hardening binds capability tokens to exact calls and bounds every caller deadline before an effect begins.

## Scope

Phase G contains two ratified slices:

1. **Task A — law tend:** capture the grants-authority v0 model in the product law and attribution ledger.
2. **Task B — independent hardening:** add exact token/call binding, Toy-token expiry enforcement, and locally bounded effective deadlines without crossing into persisted grants generation or projection recovery.

Task B begins only after the operator ratifies Gate G1.

## Ratified design

The following decisions are settled and reproduced verbatim from operator adjudication.

### D-G1

v0 uses one monotonic grants-authority generation per executing
Mother, namespaced (mother_node_id, authority_epoch, generation).
The epoch prevents generation reuse after ledger replacement,
restoration, or reinitialization. Represented as an opaque authority
identity in APIs, not a bare comparable counter.

### D-G2

The generation advances on authority-SHAPE mutations only: catalog
admission/removal, grant creation, activation, revocation,
supersession, denial, scope or constraint correction.
Authority-CONSUMPTION state (usage counters, metered limits) does
NOT advance the generation; it is enforced as a live fact at effect
time, like time bounds. Time passing alone does not advance the
generation; a materialized expiry fact does.

### D-G3

Only the executing Mother's locally verifiable current state
authorizes execution. Call-carried revisions are caller expectations
and correlation evidence; copying them can never establish local
freshness. Caller context and local execution authority stay
provenance-distinct in law; separate Rust types are the recommended
implementation, not an Allium mandate.

### D-G4

hello advertises the receiving Mother's namespaced generation; the
call echoes it for early stale rejection ONLY. Forged or guessed
current generations grant nothing; the receiver always evaluates
from local state. Wire change is required in hello/call immediately
(no compatibility period) — but the WIRE IMPLEMENTATION is deferred
(see fence below); only the law lands now.

### D-G5

effective_deadline = min(caller_deadline,
accepted_at + configured_max_call_horizon, any stricter local policy
bound), on the executing Mother's clock. Proposed default horizon:
600s, as operator-adjustable config. Caller clock skew gets no
positive grace: ahead → clamped; already expired → rejected before
any effect. Toy-token expiry never exceeds the effective deadline.

### D-G6

Every Child effect revalidates current generation; every Toy effect
revalidates generation, exact grant state/scope, and both grant and
token expiry. Stale authority denies with no implicit refresh; retry
is a full new evaluation minting new tokens (deny is the passive
default, per existing law at mct-product-map.allium:188).

### D-G7

Delegated capabilities (e.g. filesystem preopens): for v0,
already-admitted delegation remains valid until its bounded expiry,
which the effective-deadline clamp bounds. Per-operation mediation
and active revocation are explicitly future law.

### D-G8

Projection requirement (law only): authority evaluation requires the
projection to prove it covers the current canonical generation;
unprovable freshness denies. The proof MECHANISM is Review 2's.

## Task B exit criteria

### B1 — Token/call binding

Every effect-admission path verifies `token.call_id == call.call_id` and denies before any backend effect on mismatch:

- process;
- all three WASM invocation paths; and
- Toy effects.

The API shape must enforce the check so future composition cannot skip it. The decision belongs in the kernel/daemon authority path, not a backend adapter.

### B2 — Toy-token expiry

Every Toy effect admission enforces `AuthorizedToyCall.expires_at` using the executing Mother's clock. At-or-after expiry denies without invoking the backend; before expiry proceeds.

### B3 — Deadline clamping

Ingress exposes configurable `max_call_horizon` with a default of 600 seconds and computes the effective deadline required by D-G5. WASM deadline configuration consumes the effective deadline rather than the raw caller deadline. Already-expired deadlines reject before effects. Toy-token expiry is capped by the effective deadline on the production path.

## Required proof steps

Each proof step must land as a cited test. Close-out requires the test file and line plus the verbatim assertion.

1. Process-path token with mismatched `call_id` denies before effect.
2. WASM-path token with mismatched `call_id` denies before effect, on each of the three invocation paths.
3. Toy token with mismatched `call_id` denies before backend invocation.
4. Toy effect admitted at/after `expires_at` denies without backend call.
5. Toy effect before `expires_at` proceeds.
6. Far-future caller deadline: effective deadline equals the configured horizon bound; WASM epoch deadline uses the clamped value.
7. Already-expired caller deadline rejects at ingress; no effect begins.
8. Toy-token expiry never exceeds the effective deadline.
9. Caller clock ahead and behind with an injected test clock: ahead is clamped and behind is rejected, with no positive grace.

## Deferral fence

The following work is gated on Review 2 and must not be implemented, even partially, in Phase G:

- No persisted grants generation and no authority epoch storage.
- No local authority snapshot provider.
- No hello/call wire changes for generation echo.
- No projection freshness proof mechanics.
- No replacement of the vacuous grants guard in `crates/mct-daemon/src/daemon/resident/execution.rs`; its repair requires the snapshot provider in slice 7. The tended law classifies it as a code bug against ratified law.
- No ledger crash recovery.
- No process supervision.
- No permissions, HTTP, leasing, or Git work.

If an independent-hardening item requires crossing this fence, implementation stops and reports the fork rather than improvising a partial generation.

## Validation and close-out

Every implementation commit must pass:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

A reproducible failure must be fixed before proceeding. A non-reproducing failure may be classified as a flake only after up to five isolated reruns, with the verbatim failure retained in the phase flake log.

Gate G1 precedes all Rust changes. Final close-out reconstructs commit purpose, proof citations, validation transcript, flake log, and ledger dispositions from disk. Session closure remains operator-run.

## Phase G close-out

### Commit ledger

| Commit | Purpose |
|---|---|
| `fc7e06a` | Upgraded `wasmtime` and `wasmtime-wasi` to 46.0.2 for RUSTSEC-2026-0222. The only source migration was the mechanical Wasmtime 46 `ComponentExtern.ty` projection; Store and epoch-deadline behavior did not change. |
| `ccb60bb` | Added kernel-issued exact-call admission proofs and enforced them before process spawn, all three WASM component-load paths, and Toy backend selection. |
| `81c4c86` | Required an executing-Mother timestamp at every Toy effect admission and denied at or after `AuthorizedToyCall.expires_at`. |
| `b2b2e20` | Added configurable effective-deadline admission with a 600-second default, clamped resident ingress before payload/effect work, used the effective bound for WASM epoch interruption, and capped Toy tokens at that bound. |

### Required proof citations

| Step | Test citation | Verbatim assertion(s) |
|---:|---|---|
| 1 | `crates/mct-daemon/src/process.rs:435`, assertion at `:456` | `assert_eq!(report.result.outcome, ResultOutcome::Denied);` |
| 2 | WIT `crates/mct-daemon/src/wasm.rs:3942` / `:3963`; s32 `:3969` / `:3985`; toy-enabled s32 `:3991` / `:4011` | Each path: `assert_eq!(report.result.outcome, ResultOutcome::Denied);` |
| 3 | `crates/mct-daemon/src/toy.rs:810`, assertions at `:828-829` | `assert_eq!(report.outcome, MctToyAdapterOutcome::Failed);` and `assert_eq!(report.output_json, None);` |
| 4 | `crates/mct-daemon/src/toy.rs:765`, assertions at `:781-782` | `assert_eq!(report.outcome, MctToyAdapterOutcome::Failed);` and `assert_eq!(report.output_json, None);` |
| 5 | `crates/mct-daemon/src/toy.rs:788`, assertion at `:804` | `assert_eq!(report.outcome, MctToyAdapterOutcome::Success);` |
| 6 | `crates/mct-daemon/src/wasm.rs:3904`, assertions at `:3933-3938` | `assert!(matches!(permit, WasmDeadlinePermit::Running(_)));`, `assert_eq!(caller_call.deadline, Timestamp::new("2026-08-02T12:10:00Z").unwrap());`, and `assert_eq!(configured_wait, Some(std::time::Duration::from_secs(600)));` |
| 7 | `crates/mct-daemon/src/daemon/resident/pipeline.rs:1017`, assertions at `:1053-1061` | `assert_eq!(result.outcome, CallProtocolOutcome::TimedOut);` and `assert!(!effect_marker.exists(), "expired ingress began a Child effect");` |
| 8 | `crates/mct-kernel/src/toy.rs:782`, assertion at `:794` | `assert_eq!(authorized.expires_at(), &effective_call.deadline);` |
| 9 | `crates/mct-daemon/src/config.rs:668`, assertions at `:690-712` | Ahead: `assert_eq!(ahead, MctCallDeadlineAdmission::Admitted(Timestamp::new("2026-08-02T12:10:00Z").unwrap()));`; behind: `assert_eq!(admit_call_deadline(&caller_behind, &accepted_at, DEFAULT_MAX_CALL_HORIZON_SECONDS, None).unwrap(), MctCallDeadlineAdmission::Expired);` |

### Validation transcript

Each implementation commit was validated with:

```text
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

- `fc7e06a`: all three commands passed; Tier-0 scanned 534 dependencies with no RustSec finding.
- `ccb60bb`: all three commands passed.
- `81c4c86`: all three commands passed.
- `b2b2e20`: final per-commit run and final phase rerun passed all three commands.
- `allium check layer/allium`: all three Allium files returned empty diagnostics and findings.
- `patina spec check grants-authority-v0 --json`: six of six exit criteria checked after this close-out update.

### Flake log

One non-reproducing B3 workspace-test failure was retained:

```text
triggers::tests::trigger_authority_is_scoped_observed_revisioned_and_revocable
acquire exclusive observation ledger writer lock .../observations.jsonl
observation ledger is already locked by another writer
```

The required isolated rerun passed (`1 passed; 0 failed`), and two subsequent full validation runs passed. It is classified as a transient test ledger-lock collision; no production or test code was changed to mask it.

### Final ledger disposition

The 31 Phase G invariants are now attributed as **8 COVERED**, **0 Task-B LAW-LEADS-CODE**, and **23 DEFERRED**. `MctToyGrantAuthority.ToyTokenBindsCallAndEffect` remains DEFERRED as a whole: Task B covered only its exact-call edge, while action/resource/local-version scope remains fenced to slice 8. The pre-existing `TwoPhaseRouting.EffectBoundaryRevisionGuardIsDistinct` remains LAW-LEADS-CODE and untouched behind the Review 2/slice 7 fence.

# Phase I — slices 4/5 envelope completeness and Mother-owned evaluation snapshot

> Canonical Toy authority enters through one envelope, and route evaluation receives one proof-gated Mother-owned snapshot; caller echoes remain evidence and never become local authority.

## Scope and Gate G1

Phase I consumes the already-ratified Phase H/H2/H3 machinery and implements only:

1. **slice 4 residual** — close every production mutation of the D-R2.7 Toy catalog/grant scope over the existing canonical envelope; and
2. **slice 5** — construct a Mother-owned local execution authority snapshot and migrate resident route-evaluation authority inputs to it.

The Phase I baseline is `b818550`. This SPEC-only commit is Gate G1 and precedes every Phase I Rust or Cargo change. D-G1 through D-G8, D-R2.1 through D-R2.8, D-H2.1, and D-H3.1 through D-H3.4 are settled and are not reopened. A newly discovered genuine fork stops for an operator-supplied D-I.n amendment.

## Ratified Phase I amendments

### D-I.1 — operator verification from disk

The Watch-bypass repair may use the existing `toy_catalog_put` and `toy_grant_put` authority changes with no new canonical fact kind only if replay of the enveloped Watch grant, supporting-grant, and revoke facts reconstructs the complete projected state, including Watch observation scopes. Task B proof step 2 must assert that complete replay result. If the complete Watch scope genuinely rides inside the existing Toy contract/grant values, the test must identify and assert those exact fields. If the existing value schemas cannot carry the complete scope, this is a C4 fork: stop and report rather than shoehorning scope data into an unrelated field or leaving scope reconstruction to the legacy dual-write.

Disk inspection established the C4 fork: `ToyCatalogPut` and `ToyGrantPut` cannot carry the complete `WatchObservationScope`, while the ordinary `watch-observation-scope-v1:` detail is observability rather than canonical authority state.

### D-I.2 — canonical Watch-scope authority

The existing `authority_mutation` fact kind gains the sanctioned additive `WatchScopePut` change variant carrying one complete validated `WatchObservationScope`. Put semantics preserve every kernel field and revision; revocation is a complete put with `authority_state = revoked`, matching the existing non-deleting projection behavior. There is no Watch-scope remove variant.

`AuthorityStateV1` gains a stable-identity-ordered `watch_scopes` map. State hashing, mutation replay, projection publication/rebuild, and legacy import cover that map. Scope puts require complete record validation and exact prior-state/revision discipline: a first revision starts at one, a later revision is exactly current plus one, and a revoked or superseded current scope cannot be resurrected. A pre-Phase-I legacy import reads the complete SQLite Watch-scope history/current state into canonical authority; once-per-surviving-history import semantics do not change.

Each `/watch/grant`, `/watch/supporting-grant`, or `/watch/revoke` request commits exactly one canonical fact containing all authority changes for that logical request and advances generation once. Watch grant/revoke facts contain their complete scope put plus Toy changes; supporting-grant facts contain all selected Toy changes and preserve the current scope set unchanged. The ordinary `watch-observation-scope-v1:` observation remains observability only and is never an authority-replay input.

The canonical carrier remains `mct-authority-fact/v1`; D-I.2 adds no fact kind. Strict deserialization of an unknown authority change variant blocks authority projection/replay without quarantining the structurally valid ledger, matching the existing unknown authority-schema/fact-kind posture.

As a D-I.2 consequence, D-R2.7's first authority-wide projection scope is now Toy catalog, Toy grants, their grant-shaping sources, and complete Watch observation scopes. This is the sole sanctioned exception to Phase I's no-new-change-variant and no-Watch-scope-canonicalization fences.

## Binding landed law

The implementation treats the following landed law as binding, quoted verbatim rather than reinterpreted.

- **D-G3:** “Only the executing Mother's locally verifiable current state authorizes execution. Call-carried revisions are caller expectations and correlation evidence; copying them can never establish local freshness. Caller context and local execution authority stay provenance-distinct in law; separate Rust types are the recommended implementation, not an Allium mandate.”
- **D-G8:** “Projection requirement (law only): authority evaluation requires the projection to prove it covers the current canonical generation; unprovable freshness denies. The proof MECHANISM is Review 2's.”
- **`CallerAuthorityCannotBecomeLocalAuthorityByCopying`:** “The call authority context is a caller-carried expectation snapshot. Copying any of its values into another record cannot establish the executing Mother's current authority.”
- **`LocalExecutionSnapshotHasMotherProvenance`:** “Execution authority identifies the executing Mother and is produced only from that Mother's locally verifiable policy, grants, time, and canonical authority evidence.”
- **`LocalSnapshotIsCoherent`:** “One local execution snapshot binds policy, grants authority, effective deadline, and source evidence from one coherent authority state rather than independently mixed reads.”
- **`AuthorityProjectionCoversCurrentGeneration`:** “Authority evaluation may use a projection only when an authority-state cursor proves coverage of the current canonical grants-authority generation rather than merely a formerly valid generation.”
- **`UnprovableFreshnessDenies`:** “When current canonical generation coverage cannot be proved, authority evaluation denies instead of treating cached, missing, or ambiguous projection state as current.”
- **`LedgerFactsAreCanonicalAuthority`:** “Current grant and catalog authority derives from committed canonical ledger facts rather than configuration or projection records.”
- **`MutationAndGenerationAdvanceAreOneFact`:** “One logical authority-shape mutation and its resulting generation advance are represented by one committed canonical fact, never by independently committable authority changes.”
- **`ProjectionFailureDoesNotUndoCommit`:** “A committed authority fact remains canonical when projection fails. Mutation results distinguish `committed`, `committed_projection_pending`, `commit_unknown`, and `rejected_before_commit`; authority use denies until coherent projection coverage returns.”

Phase I relies on Phase H3 readiness ordering: import and final startup observations precede current projection proof and serving readiness. It nevertheless proves D-G8 on every snapshot construction; startup readiness is not a reusable mid-run authority permit.

## Local execution authority snapshot

### Observable value

`LocalExecutionAuthoritySnapshot` is a kernel authority input with four provenance-distinct portions:

```text
LocalExecutionAuthoritySnapshot {
  executing_mother_node_id: String,
  canonical_grants: {
    grants_authority: GrantsAuthorityIdentity,
    toy_catalog: [complete CanonicalToyContract, ...],
    toy_grants: [complete ToyGrant, ...],
    watch_scopes: [complete WatchObservationScope, ...]
  },
  policy: {
    policy_revision: u64,
    vision_policy_revision: u64,
    child: {
      provenance: legacy_config_and_loaded_child_projection,
      complete_evaluation_inputs: child artifacts, approvals, assignments, instances
    },
    peer: {
      provenance: legacy_config_and_runtime_peer_projection,
      complete_evaluation_inputs: local identity, bindings, outbound proofs, callable surfaces
    }
  },
  mother_clock: {
    evaluated_at: Timestamp,
    provenance: executing_mother_clock
  },
  projection: {
    projection_id: String,
    source_mother_node_id: String,
    source_ledger_id: String,
    through_sequence: u64,
    through_observation_id: String,
    through_entry_hash: String,
    authority_state_hash: String,
    projection_hash: String
  }
}
```

The canonical arrays have stable-identity order and come from the same authority projection publication named by `projection`; `canonical_grants.grants_authority` exactly equals that cursor's complete namespaced identity. A projected `ToyGrant.grants_revision` is retained only as replayed legacy source content and is never compared with a call echo or substituted for `GrantsAuthorityIdentity`.

The v0 composition decision is explicit: only Toy catalog/grants, complete Watch observation scopes, and their D-R2.7 grant-shaping source history are canonical. Policy, Child approval/assignment/instance, peer binding/proof, and callable-surface inputs remain Mother-local legacy projections with explicit provenance labels. Their inclusion does not canonicalize them, permit them to shape the canonical generation, or claim cross-store ACID. `LocalSnapshotIsCoherent` in Phase I means that the canonical portion and its complete cursor are one atomic projection publication, while each labeled policy portion is captured as one immutable input value and all portions are bound into one non-refreshable evaluation argument.

The snapshot's fields are read-only outside its defining kernel module. There is no public constructor, `From`, or copying conversion from `AuthorityContextSnapshot`/`CallerAuthorityContext`; caller echo and local authority are different types. Token types do not gain a snapshot field in Phase I because their migration is fenced to slices 7-8.

### Provider API

The daemon exposes one concrete provider operation, not a speculative trait:

```text
local_execution_authority_snapshot(
  current_canonical_ledger_evidence,
  mother_local_policy_state,
  mother_clock
) ->
  usable(LocalExecutionAuthoritySnapshot)
  | denied(LocalExecutionAuthoritySnapshotDeny)
```

The API has no `MctCall`, `AuthorityContextSnapshot`, caller revision, caller timestamp, or caller-provided identity parameter. The provider:

1. derives exact canonical expectation from the maximal validated current ledger head and authority replay;
2. reads Toy catalog, Toy grants, and cursor as one SQLite publication snapshot;
3. requires the existing `UsableAuthorityProjectionProofV1::Usable` result against that exact expectation;
4. binds the complete cursor identity and projection hashes into the kernel snapshot;
5. captures the complete existing Mother-local Child/peer policy inputs with the labels above; and
6. samples the executing Mother's clock once for `evaluated_at`.

Every existing `AuthorityProjectionDenyReasonV1` is preserved one-for-one as a typed snapshot denial. Ledger/authority-replay, local-policy, or Mother-clock unavailability is likewise typed; no denial path returns a partial snapshot. The provider neither repairs nor refreshes canonical history. Projection catch-up/rebuild remains an explicit recovery action; a later retry constructs a wholly new snapshot.

A snapshot linearizes at one proof-validated canonical projection publication. Concurrent canonical mutation may yield an entirely pre-mutation snapshot or, after projection catch-up, an entirely post-mutation snapshot. A canonical fact committed before the provider's validated head with a behind projection denies; mixed identity/grants/cursor values are impossible.

## Complete authority-mutation inventory

Disk inspection covered `RESIDENT_MUTATION_ROUTES`, its handler dispatch, CLI command dispatch, every production call to `upsert_toy_contract`/`upsert_toy_grant_snapshot`, and the offline administrative fallback. `MctRuntimeStateStore` upsert methods are legacy projection helpers, not ingress surfaces.

| Production ingress / control route | Authority shape | Canonical fact today | Phase I disposition |
|---|---|---|---|
| `POST /authority/import-toy-state` | D-R2.7 Toy catalog/grant import | exactly one `legacy_authority_import`; one generation advance | Already complete; include in parameterized proof. |
| `toys authorize-slate` → `POST /toys/authorize-slate` or exclusive offline fallback | Toy catalog puts plus Slate Toy grant puts | exactly one `authority_mutation`; one generation advance | Already complete; resident and offline modes share `PreparedAdministrativeMutation`. |
| `toys authorize-secret` → `POST /toys/authorize-secret` or exclusive offline fallback | secret Toy catalog put plus grant put | exactly one `authority_mutation`; one generation advance | Already complete; resident and offline modes share the envelope. |
| `toys grant-watch` → resident-only `POST /watch/grant` | Watch scope plus one D-R2.7 Toy catalog/grant pair | no canonical authority fact; ordinary observations precede direct legacy upserts | **Bypass found.** Add explicit mutation id and commit the complete Toy pair as one existing `authority_mutation` only if D-I.1 proves that replay also reconstructs the complete Watch scope; otherwise stop at C4. |
| `toys grant-directory-read`, `grant-keyvalue`, `grant-observability` → resident-only `POST /watch/supporting-grant` | one or more D-R2.7 supporting Toy catalog/grant pairs | no canonical authority fact; ordinary grant observations precede direct legacy upserts | **Bypass found.** Commit the complete request as one existing `authority_mutation`, regardless of one/two selected observability grants. |
| `toys revoke-watch` → resident-only `POST /watch/revoke` | Watch-scope revocation plus D-R2.7 Toy grant revocation | no canonical authority fact; ordinary observations precede direct legacy upsert | **Bypass found.** Commit the complete Toy grant replacement/revocation as one existing `authority_mutation` only if D-I.1 proves that replay also reconstructs the complete revoked Watch scope; otherwise stop at C4. |
| `POST /artifacts/sources/create|revoke` | standing artifact-source authority | no canonical Toy/grant fact | Outside D-R2.7 by ratified H3; source correlation and D-G8 standing-source admission remain unchanged. |
| `triggers create|revise|revoke` → `POST /triggers/*` | trigger authority | no canonical Toy/grant fact | Outside D-R2.7; remains labeled Mother-local policy state. |
| `children approve|revoke` → `POST /children/*` | Child approval/assignment authority | no canonical Toy/grant fact | Outside D-R2.7; remains labeled legacy Child policy until later canonicalization. |
| `peers add|set-outbound-proof|revoke|remove` → `POST /peers/*` | peer binding/proof authority | no canonical Toy/grant fact | Outside D-R2.7; remains labeled legacy peer policy. |
| offline `iroh identity` / resident-refused `POST /identity/ensure` | Mother identity/policy source | no canonical Toy/grant fact | Outside D-R2.7 and not a Phase I mutation target. |
| `/lifecycle/fact`, `/blobs`, closed `/registry/install|sync`, `/artifacts/stage`, `/releases/acquire`, and `/pando/record` | lifecycle/data/evidence/composition, not grant authority shape | no canonical Toy/grant fact | Not authority-shape mutations for this phase. |

There are no production Toy catalog/grant removals or general grant-correction endpoints beyond the complete `put`/revocation surfaces above. Test-only direct upserts in `startup.rs`, `state.rs`, and Watch unit fixtures are not production ingress. D-I.1 established that the existing Toy values were not replay-complete; D-I.2 therefore sanctions `WatchScopePut` inside `AuthorityMutationRequestV1` while preserving the existing canonical fact kind. Task B repairs the three Watch route families with that complete value and never consumes the ordinary scope observation as authority.

Exactly one logical request above may commit exactly one canonical import/mutation fact and therefore advances generation exactly once, even when it carries multiple catalog/grant changes. Its required noncanonical domain observations do not advance grants authority. `rejected_before_commit` and `commit_unknown` perform no legacy Toy write; acknowledged commitment precedes all legacy Watch/Toy projection writes; projection failure cannot undo the fact.

## Complete call-revision comparison inventory and gate classification

The inventory includes every production expression that compares a `call.authority_context` revision with evaluation/token state, plus the call-derived construction sites that can make a later comparison vacuous.

| Site | Current comparison/source | Gate classification and transitional behavior |
|---|---|---|
| `crates/mct-kernel/src/call/internal.rs:223-224` | call policy/grants echo `<` admitted protocol authority | **Fenced to slice 6.** Existing hello/call early stale rejection is byte-for-byte unchanged. It remains only an early rejection and is not used by the Phase I provider or route evaluation. |
| `crates/mct-kernel/src/child.rs:706` | approval policy revision `!= call.authority_context.policy_revision` | **Migrate in Phase I for route evaluation.** Route Child evaluation compares approval/policy facts with the snapshot's labeled local Child policy, never the call echo. The Child token field continues to source its revision from the allowed local evaluation exactly as today; its effect-admission API remains slice 7. |
| `crates/mct-kernel/src/toy.rs:419-420` | grant policy/grants revisions `!=` call echoes | **Split by consumer.** The Phase I route evaluator uses canonical snapshot grants and complete `GrantsAuthorityIdentity`; existing token-mint/effect callers retain byte-for-byte legacy behavior until slice 8. No route decision compares a bare projected grant revision to canonical generation. |
| `crates/mct-kernel/src/route.rs:450` | Child evaluation policy `!=` call policy echo | **Migrate in Phase I.** Revalidation compares evaluation provenance with the same local snapshot's policy portion. |
| `crates/mct-kernel/src/route.rs:491` | Toy evaluation policy `!=` call policy echo | **Migrate in Phase I.** Compare against snapshot policy provenance. |
| `crates/mct-kernel/src/route.rs:501` | Toy evaluation grants revision `!=` call grants echo | **Migrate in Phase I.** Compare complete snapshot/canonical `GrantsAuthorityIdentity` and cursor provenance, not bare counters. |
| `crates/mct-daemon/src/daemon/resident/candidates.rs:248` | peer policy revision `!=` call policy echo | **Migrate in Phase I.** Remote candidate evaluation uses the snapshot's labeled local peer policy. Peer wire remains unchanged. |
| `crates/mct-kernel/src/child.rs:420-422` | Child token exact call and policy revision `==` call values | **Fenced to slice 7.** Existing process/WASM Child effect admission is unchanged; the token remains call-bound and this phase does not claim current-generation effect safety. |
| `crates/mct-kernel/src/toy.rs:265-269` | Toy token exact call, policy/grants revisions, and expiry `==`/time check | **Fenced to slice 8.** Existing Toy backend admission is unchanged, including exact-call and expiry hardening. |
| `crates/mct-daemon/src/daemon/resident/execution.rs:92-101,124-144` | current policy comes from config, while current grants/vision are copied from the call before token/current comparison | **Fenced to slice 7 and explicitly still vacuous for grants.** This known guard is read and regression-pinned but not touched or credited in Phase I. Slice 7 replaces it with locally minted authority and `MotherAuthorityOrderV1`. |

Related construction sites are also fenced or migrated explicitly:

- `resident/decision.rs:143-151` and `resident/candidates.rs:271-276` currently stamp candidate grants revision from the call; Phase I stamps route-evaluation evidence from the snapshot's complete grants identity/provenance.
- `route.rs:542-560,594-595` currently stamps the authorized route token and denial records from call revisions. Evaluation/observation records migrate to local snapshot provenance, while token revision sourcing stays unchanged for slice 7.
- local/peer call protocol authority construction and forwarding continue to carry/echo the existing wire revisions until slice 6.
- process, WASM, Toy backend, hello, and idempotent replay call sites remain untouched. Test helpers that construct synthetic calls are not production authority sources.

Thus Phase I introduces no new self-comparison: every migrated route-evaluation check compares separately sourced local values or binds an evaluation to the exact snapshot/cursor that produced it. The pre-existing vacuous effect guard remains isolated behind the explicit slice-7 fence and is not used to prove any Phase I invariant.

## Correlation-only caller echo

The call's existing `authority_context` remains immutable request evidence. Existing `CallReceived`/`PeerCallReceived` and `CallConstructed` observations retain the echoed policy/grants values. Phase I route-decision observations additionally correlate the call echo with the local snapshot's namespaced grants identity and cursor reference using existing observation kinds and fields; they do not reinterpret the echo as local generation.

At the route-evaluation layer, stale, equal, future, or absurd echoed revision numbers neither grant nor deny. They produce the same decision under the same local snapshot and differ only in recorded correlation evidence. The pre-route protocol comparison remains the separately fenced slice-6 behavior; Phase I tests the route layer directly and does not claim the wire gate has migrated.

## Phase I implementation tasks

### B1 — slice 4 envelope completeness

Repair the three Watch route families above through the existing authority envelope, then prove all inventoried D-R2.7 mutation families commit one fact/one generation and no production legacy Toy projection write is reachable without acknowledged canonical commitment.

### C-1 — kernel type and daemon provider

Add the read-only kernel `LocalExecutionAuthoritySnapshot` and one concrete daemon provider. Construct it only from exact current ledger expectation, one coherent authority projection publication, labeled Mother-local policy inputs, and the Mother clock. Preserve every typed D-G8 denial and expose no call-derived constructor.

### C-2 — route-evaluation migration

Require resident local and remote route evaluation/revalidation to receive the snapshot. Migrate only the comparison/construction sites classified for Phase I, evaluate required route Toy grants from the canonical portion, and record caller echoes as correlation evidence. Do not change token or effect admission.

### C-3 — adversarial proof closure

Land the coherence, concurrency, epoch, revocation, clock, provenance, negative, and fence proofs below without widening the sanctioned reader change.

## Phase I required proof steps

Each proof lands as a named test. Close-out cites its file/line and quotes the verbatim central assertion.

1. A parameterized matrix over legacy import, authorize-Slate, authorize-secret, `/watch/grant`, every `/watch/supporting-grant` variant, and `/watch/revoke` proves exactly one canonical fact and exactly one generation advance per successful logical request.
2. Every production D-R2.7 route, parameterized over `/watch/grant`, every `/watch/supporting-grant` variant, and `/watch/revoke`, refuses or leaves legacy Toy and Watch-scope rows byte-value unchanged when canonical commitment is rejected, unknown, poisoned, or unavailable; no direct legacy-only production path is reachable.
3. Usable D-G8 proof constructs one snapshot whose executing Mother, complete grants identity/state, labeled Child/peer policy inputs, Mother time, and full cursor provenance match the exact source values.
4. Hostile arbitrary call `authority_context` values cannot affect snapshot construction; the provider API accepts no call and yields the same snapshot as construction with no call in scope.
5. End-to-end required-Toy revocation through the envelope, projection catch-up, and next resident route evaluation denies regardless of the call echo.
6. A projection behind the canonical head at snapshot time returns the exact typed denial and no snapshot; after explicit catch-up a fresh construction restores evaluation.
7. After epoch transition an old-epoch projection returns `epoch_mismatch`; rebuild restores evaluation with unchanged grant meaning and a different authority identity.
8. Stale, current, future, and absurd caller echoes produce identical route decisions under one snapshot, while durable observations retain each exact echo as correlation evidence.
9. A correct call admits under a current snapshot even when its echoed legacy revision differs from canonical generation.
10. A parameterized genuinely stale local-policy, grants-identity, cursor, or projection case denies at every migrated comparison; no test can pass by supplying the same value to both sides.
11. Concurrent canonical mutation yields either an entirely pre-mutation or entirely post-catch-up snapshot, never mixed grants/identity/cursor state.
12. Time-window route grant evaluation uses `snapshot.mother_clock.evaluated_at`; caller-controlled timestamps or echoes cannot move the allow/deny boundary.
13. API-shape/constructor audit proves no public conversion or constructor exists from caller authority context to local execution snapshot and no provider parameter can carry it.
14. Regression pins prove slice-6 protocol echo behavior, slice-7 token minting/vacuous resident guard and Child/process/WASM guards, slice-8 Toy effect guard, hello/peer wire serialization, idempotent replay, and the zero-production-consumer status of `MotherAuthorityOrderV1` are unchanged from `b818550`.
15. Canonical replay of Watch grant and revoke facts, with ordinary `watch-observation-scope-v1:` observations absent, reconstructs a byte-equal `WatchObservationScope` and explicitly asserts every kernel field.
16. Legacy import with existing SQLite Watch scopes includes them completely in `LegacyAuthorityImportFactV1.imported_state`, projection rebuild preserves them, and once-per-canonical-history behavior is unchanged.
17. A structurally valid authority fact carrying an unknown `change_kind` fails closed for authority replay/projection without ledger quarantine or silent skipping.

## Phase I close-out

### Commit ledger

| Commit | Purpose |
|---|---|
| `f6c5029` | Ratified the Phase I provider, reader inventory, consumer split, and 14-step proof plan. |
| `92f62bf` | Added D-I.1's replay-complete Watch-authority requirement and stopped at the discovered schema fork. |
| `b7a4d9a` | Preserved the active-session C4 stop without changing Rust or law. |
| `ab4c87d` | Ratified D-I.2: complete Watch scope in additive `WatchScopePut` changes inside the existing fact kind. |
| `44bec15` | Repaired all Watch bypasses, added schema v13 Watch projection/replay/import, and landed proofs 1-2 and 15-17. |
| `29e9e9e` | Added the sealed proof-gated Mother-owned snapshot/provider and landed proofs 3-4, 6-7, 11, and 13. |
| `d8234f2` | Migrated local/remote resident route evaluation and revalidation to the snapshot, including correlation-only caller echoes. |
| `5d53d13` | Landed adversarial route, clock, revocation, no-vacuous-pass/no-spurious-deny, and deferral-fence proofs 5, 8-10, 12, and 14. |

### Required proof citations

| Step | Test citation | Verbatim central assertion(s) |
|---:|---|---|
| 1 | `crates/mct-daemon/src/daemon/control.rs:4144`, assertions at `:4347-4352` | `assert_eq!(after.current_authority.unwrap().generation, before_generation + 1, "{path}");` and `assert_eq!(after.canonical_fact_count, before_facts + 1, "{path}");` |
| 2 | `crates/mct-daemon/src/daemon/control.rs:4374`, assertions at `:4549,4557` | `assert_ne!(status, 200, "{failure:?} {surface:?}: {response}");` and `assert_eq!(after, before, "{failure:?} {surface:?}");` |
| 3 | `crates/mct-daemon/src/authority_snapshot.rs:434`, representative assertions at `:442,476-483` | `assert_eq!(snapshot.executing_mother_node_id(), "local-mct");` and exact projection entry/projection-hash equality. |
| 4 | `crates/mct-daemon/src/authority_snapshot.rs:488`, assertion at `:497` | `assert_eq!(snapshot(&fixture), before);` for hostile zero/current/`u64::MAX` echoes. |
| 5 | `crates/mct-daemon/src/daemon/resident/decision.rs:1281`, final assertion at `:1389` | `assert!(matches!(authorize_resident_child_from_snapshot(&snapshot_after, vec![child], &call).unwrap(), RouteDisposition::Denied { .. }));` |
| 6 | `crates/mct-daemon/src/authority_snapshot.rs:503`, assertions at `:535-556` | Behind projection is exactly `HeadSequenceMismatch`; after rebuild, generation advances and the new catalog is visible. |
| 7 | `crates/mct-daemon/src/authority_snapshot.rs:561`, assertions at `:568-599` | Old projection is exactly `EpochMismatch`; rebuild preserves grant meaning under the canonical epoch identity. |
| 8 | `crates/mct-daemon/src/daemon/resident/decision.rs:1143`, assertions at `:1200-1201` | `assert_eq!(dispositions, vec!["local"; echoes.len()]);` and `assert_eq!(recorded_echoes, echoes);` |
| 9 | `crates/mct-daemon/src/daemon/resident/decision.rs:1206`, assertion at `:1224` | `matches!(outcome, RouteDisposition::Local { .. })` with canonical generation 73 and caller echo 1. |
| 10 | Local/grants `crates/mct-kernel/src/route.rs:1309`, assertions at `:1322-1355`; remote `crates/mct-daemon/src/daemon/resident/candidates.rs:752`; cursor/projection `crates/mct-daemon/src/state.rs:6152` | Independent policy/grants mismatches deny as `PolicyRevisionStale`/`GrantsRevisionStale`; matching independent values authorize; caller echo cannot erase stale peer policy; every D-G8 mismatch remains typed. |
| 11 | `crates/mct-daemon/src/authority_snapshot.rs:603`, assertions at `:662-672` | Returned generation is pre or post only; catalog membership, grants identity, and cursor sequence agree exactly. |
| 12 | `crates/mct-daemon/src/daemon/resident/decision.rs:1232`, assertion at `:1276` | `assert_eq!(outcomes, ["denied", "allowed", "denied"]);` across before-start, at-start, and at-expiry Mother times despite hostile echoes/deadline. |
| 13 | `crates/mct-daemon/src/authority_snapshot.rs:700`, assertions at `:715-717` | Kernel source exposes neither caller-context nor caller-authority conversion and no copying conversion. |
| 14 | `crates/mct-daemon/src/authority_order.rs:638`, representative assertions at `:645-677,700-703` | Protocol/effect/wire/replay source pins remain present and `production_consumers.is_empty()` for `MotherAuthorityOrderV1`. |
| 15 | `crates/mct-observation/src/lib.rs:3878`, assertions at `:3929-3967` | No ordinary Watch-scope observation exists; replayed scope is byte-equal and every kernel field equals the revoked canonical value. |
| 16 | `crates/mct-daemon/src/state.rs:6546`, assertions at `:6559-6598` | Imported state contains the complete scope, rebuild preserves it, and duplicate import is `AlreadyImported`. |
| 17 | `crates/mct-daemon/src/state.rs:6657`, assertions at `:6721-6734` | Unknown `change_kind` names the error, leaves projection byte-value unchanged, and the structurally valid ledger remains readable. |

### Close-out audits

- **Changed readers:** Phase I changes canonical Watch mutation/projection paths, the new snapshot provider/types, resident local/remote route evaluation/revalidation, and test fixtures needed to open an authority ledger. It does not migrate effect-time consumers.
- **Protected effect range:** `git diff --unified=0 b818550..HEAD` shows `daemon/resident/execution.rs` changes only in test setup; `execution.rs:92-146`, process/WASM/Toy adapters, token admission, and `call/internal.rs` have no production hunk.
- **Ordering boundary:** source audit finds `MotherAuthorityOrderV1` only in `authority_order.rs` and the `lib.rs` re-export; all `commit_mutation()`/`admit_effect()` calls remain harness tests.
- **Canonical carrier:** the only authority fact kinds remain `authority_mutation`, `legacy_authority_import`, and `epoch_established`; D-I.2 adds only `WatchScopePut` inside `authority_mutation`.
- **Framing/schema fence:** `MctObservation`, `MctObservationLedgerEntry`, and newline-delimited framing are unchanged; SQLite advances only to schema v13 for the reconstructable Watch-scope projection.
- **Honest disposition:** route-evaluation clauses are covered. Slice-6 peer-wire semantics and slices 7-8 token/effect ordering, live grant revalidation, and delegated-capability revocation remain `DEFERRED` in Track 3.

### Validation and flake log

Final Phase I validation passed `cargo test --workspace` (494 passed, 1 ignored), warnings-denied workspace clippy, Tier 0/RustSec, `allium check layer/allium`, `patina spec check grants-authority-v0 --json`, and `git diff --check`.

One new proof initially reproduced a fixture error: `remote_policy_mismatch_denies_even_when_caller_echo_matches_stale_peer` returned `PeerNotAdmitted` because mutating the signed peer record invalidated its binding signature before reaching the policy comparison. The fixture was corrected to vary the independently sourced local policy instead; the isolated rerun and full suite passed. This was a deterministic test-construction defect, not a product flake. No Phase I non-reproducing failure was observed.

## Phase I deferral fence

The following remain forbidden, including partial implementation:

- No changes to `execution.rs:92-146`, token minting revision sourcing, `AuthorizedChildInvocation`/`AuthorizedToyCall` effect guards, process/WASM/Toy adapters, or delegated-capability semantics; slices 7-8 own them.
- No hello/call schema or peer-wire migration and no early protocol stale-rejection change; slice 6 owns them.
- No idempotent replay authority migration.
- No production consumer of `MotherAuthorityOrderV1`; slices 7-8 route commits and effect starts through it.
- No new canonical fact kind, `MctObservation` field, `MctObservationLedgerEntry` field, or ledger framing change. D-I.2's additive `WatchScopePut` is the only sanctioned new authority change variant.
- No canonicalization of policy, Child approval/assignment/instance, peer binding/proof/publication, trigger, or artifact-source domains. D-I.2 canonicalizes complete Watch scopes; all other named domains remain labeled local legacy inputs.
- No evaluation fallback to config, legacy Toy tables, caller echoes, stale projection, bare generation, startup readiness cache, or record digest when D-G8 is unusable.
- No effect-time revocation claim. Evaluation-level clauses may advance; global/effect-time clauses remain deferred to slices 7-8.

If implementation requires crossing this fence, Phase I stops for a D-I.n amendment rather than adding a compatibility path or hidden authority source.

## Validation and Gate G1

Every implementation commit and the final close-out must pass:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

The final close-out also runs `allium check layer/allium`, `patina spec check grants-authority-v0 --json`, and `git diff --check`. A failing test is rerun in isolation up to five times; a non-reproducing failure is retained verbatim in the Phase I flake log, while a reproducing failure is fixed before advancement.

**Gate G1:** stop after committing this SPEC amendment. Report the complete diff, mutation inventory, and comparison-site classification for operator ratification. No Phase I Rust change may precede that ratification.

# Phase J — slices 7/8 effect-boundary authority

> Execution authority is minted from the evaluating Mother snapshot, compared with a fresh proof-gated current read, and handed directly into the final Child or Toy effect start under the one existing Mother authority order.

## Scope and gate

Phase J retires the Phase I slice-7/8 pins and implements only:

1. **slice 7** — locally sourced route/Child execution authority, replacement of the vacuous resident grants guard, and production adoption of the Mother-local mutation/effect order at process and all three WASM Child seams; and
2. **slice 8** — current exact-grant Toy revalidation, ordered Toy backend/delegation admission, and the D-G7 bounded-delegation proof.

The Phase J baseline is `6b179bf` on branch `patina`. D-G1 through D-G8, D-R2.1 through D-R2.8, D-H2.1, D-H3.1 through D-H3.4, D-I.1 through D-I.2, and D-J.1 remain settled. D-J.2 records later scope without authorizing Phase J implementation. Gate G1 was operator-ratified after disk verification of `fa87ed3` and `b1779cd`; Phase J implementation may proceed under this design and the active reliability doctrine. A newly discovered genuine behavioral fork stops for an operator-supplied D-J.n amendment.

### Gate G1 decisions

- **D-J.1 — unsupported consumption state denies:** a Toy grant carrying `max_uses = Some(_)` denies at effect admission with typed `consumption_state_unavailable` until a separately specified live consumption fact exists. Proof 16 distinguishes that denial from an otherwise-identical `max_uses = None` admission. This closes a latent silently unenforced constraint; it is not merely scope documentation.
- **D-J.2 — Review 3 authority-response scope:** the retained idempotent-replay pin concerns what a replayed response means after authority changes, not process supervision. Review 3 owns the caller- and Child-observable semantics of authority change during and after execution, including completed replay and mid-execution denial. Phase J records the observed current shape and designs neither response semantic.

## Minted execution-authority contract

The existing route, Child, and Toy authorization tokens remain non-cloneable executable authority values. Phase J completes their authority content rather than introducing a second token family:

- every token names the exact `call_id` evaluated;
- route and Child authority name the selected Child, instance, assignment, approval, artifact, and route decision already present in the successful evaluation;
- Toy authority names the exact grant, canonical Toy, Child instance, action, resource, Vision, Node, project, and data scope admitted by evaluation;
- each token carries the complete namespaced grants-authority identity — Mother node, authority epoch, generation, and source authority observation — copied only from the `LocalExecutionAuthoritySnapshot` used for that evaluation;
- each token carries the locally admitted effective deadline; Toy token expiry is the earliest of exact grant expiry and that effective deadline, while Child authority expires at the effective deadline; and
- policy identity is copied from the same local snapshot portion used by evaluation. `call.authority_context` remains immutable correlation evidence and is not a token authority source.

The additive fields above do not redesign token lifecycle: successful kernel evaluation remains the only mint, adapters still consume the existing token types, and denial still mints no token. There is no constructor or conversion from caller authority context to locally minted execution authority.

A Child effect admission compares exact call and selected-Child binding, local policy identity, complete grants-authority identity, and effective deadline against one fresh proof-gated read. A Toy effect admission composes those checks with exact current grant and effect scope. Any mismatch produces a typed denial before adapter effect; no guard repairs or refreshes the token. Retry re-enters the complete route/evaluation path and mints wholly new authority.

## Effect-boundary integration points

| Adapter family | Existing final seam | Phase J integration |
|---|---|---|
| Process Child | `MctProcessChildHarness::invoke_authorized_child_bytes`, immediately before `Command::spawn` | Fresh local snapshot and kernel Child admission precede `MotherAuthorityOrderV1::admit_effect`; its single-use handoff enters process spawn directly. Spawn is the ordered effect start; waiting/reaping is outside the authority order. |
| WIT component Child | `MctWasmComponentRuntime::invoke_wit_export_after_contract_check`, before component load/runtime entry | The same fresh Child admission and one `admit_effect` handoff enter the WIT runtime adapter before component load. The order is released at adapter entry and is not held across component execution or nested Toy calls. |
| s32 component Child | `MctWasmComponentRuntime::invoke_authorized_s32_export`, before component load/runtime entry | Same Child admission and direct single-use handoff; no policy-only call comparison remains. |
| s32 component with Toy imports | `MctWasmComponentRuntime::invoke_authorized_s32_export_with_toy_imports`, before component load/runtime entry | Same Child admission and direct single-use handoff; nested Toy calls perform their own later ordered admissions. |
| Toy backend | `MctToyAdapterRegistry::call_authorized_toy_at`, immediately before backend selection/invocation | The daemon supplies a fresh proof-gated snapshot to the kernel Toy revalidation. A successful exact-grant decision enters the selected backend only through one `admit_effect` handoff. |
| WASI filesystem delegation | `build_wasi_ctx`, immediately before installing each authorized preopen | Delegation admission performs the same current exact-grant and ordered Toy handoff once. The admitted preopen is bounded by the token/effective deadline; later filesystem operations are not mediated in v0. |

`admit_effect` remains the sole Mother-local order. Its production handoff linearizes effect start and releases the order when control enters the named adapter-start seam; it is never held for the complete Child execution and therefore cannot deadlock a nested Toy admission. It returns no refreshable permit that can be stored for later start.

## Canonical mutation integration points

Every control-plane commit that can change canonical grants authority enters the same `MotherAuthorityOrderV1::commit_mutation` position before bytes are offered:

- resident `ResidentLedgerCommand::AuthorityMutation`, shared by administrative and Watch mutations;
- resident `ResidentLedgerCommand::LegacyAuthorityImport`; and
- the equivalent offline administrative mutation/import path while it owns the exclusive authority writer.

The existing `JsonlObservationLedger::{execute_authority_mutation,execute_legacy_authority_import}` operations remain the canonical commit implementation. Phase J wraps them; it does not add a fact kind, change variant, ledger writer, mutation lock, or cross-file transaction. The ordering position remains owned until the result is classified and, for an acknowledged commit, projection publication is either proved current or classified pending. `commit_unknown`, writer poisoning, and committed projection lag fence later admissions under the existing H3 recovery law.

An ordinary non-authority observation may advance the canonical head without changing authority identity/state. Effect admission still proves projection coverage through the actual current head; head advancement cannot be replaced by a cached readiness value. The ordering boundary compares the token's exact authority identity/state expectation while the D-G8 proof independently covers the current head.

## Child effect-time checklist

Immediately before process or WASM adapter start, the kernel decision requires:

1. token `call_id` equals the supplied call;
2. token Child/instance/artifact/assignment identity equals the selected loaded Child seam;
3. token policy identity equals the fresh snapshot's labeled local Child policy;
4. token complete grants-authority identity equals the fresh snapshot identity, including Mother and epoch;
5. executing-Mother time is strictly before the token's effective deadline;
6. the snapshot carries an exact usable D-G8 proof through the current canonical head; and
7. `MotherAuthorityOrderV1` is unfenced and admits that exact expectation.

Failure is a typed Child authority denial with no adapter invocation and no implicit refresh. The resident maps it through the existing denied result/observation path; Phase J does not invent new Child-visible wire semantics.

## Toy effect-time checklist

Immediately before every Toy backend or delegated-capability admission, one fresh snapshot and kernel decision require:

1. the complete token grants-authority identity equals the fresh current identity;
2. the token's exact `grant_id` is present in canonical current grants;
3. that grant remains `active` and its complete subject matches the token's Child instance/artifact/assignment/caller restrictions;
4. canonical Toy identity and authority-bearing catalog state remain current;
5. action, resource, Vision, Node, project, data classification, and locality scope equal the token's admitted effect scope;
6. the executing Mother's current time satisfies `starts_at <= now < grant.expires_at` where present;
7. the executing Mother's current time is strictly before token expiry and effective deadline;
8. any consumption-bearing limit is checked against a current live consumption fact without advancing authority-shape generation; and
9. the exact D-G8 expectation is admitted by the unfenced Mother order.

There is currently no production surface or live projection that creates/tracks consumption-bearing Toy grants: supported grant constructors set `max_uses = None`, and no usage-counter fact exists. Under D-J.1, `max_uses = None` needs no counter, while a canonical/imported grant with `max_uses = Some(_)` denies at effect admission as typed `consumption_state_unavailable` until separately specified live consumption state exists. It is never treated as unmetered. `max_duration_ms`, when present in future canonical data, may only narrow token/effect duration and cannot extend the effective deadline.

Toy denial is typed and the backend is not invoked. A Toy denial arising during a running Child is exposed through the current host-adapter error/result path and recorded during close-out; Phase J does not define a new Child-facing error contract.

## D-G7 v0 delegated-capability semantics

Delegation admission is a Toy effect admission, not ambient configuration. A filesystem preopen is installed only after current exact-grant, time, D-G8, and ordered-admission checks. Its expiry is exactly bounded by the earlier of the grant/token bound and the effective call deadline.

After admission, the delegated capability remains usable until that bound even if authority changes. The change denies every new delegation and every separately mediated Toy effect, but does not retroactively retract an already-installed preopen. Phase J adds no per-filesystem-operation mediation, active revocation, or background capability recall.

## Phase I proof-14 pin retirement

| Phase I pin | Phase J disposition |
|---|---|
| `call/internal.rs` policy/grants echo early rejection | **Retained — slice 6.** Peer-wire echo remains an early stale hint and is untouched. |
| Resident `current_resident_route_revisions` copies grants/vision from the call | **Retired.** Removed; proofs 2-3, 12-13, and 15 require a fresh local snapshot and complete token identity. |
| Authorized route token stamps policy/grants revisions from the call | **Retired.** Proof 1 requires snapshot identity for hostile, absent, and absurd echoes. |
| `AuthorizedChildInvocation::admit_effect_for_call` compares token policy with the call | **Retired.** Proofs 2, 11, and 13 compare local token authority with a fresh local snapshot at all Child seams. |
| Process harness calls only the legacy token/call guard | **Retired.** Proofs 2, 4-6, 12-13, and 15 cover current-state and ordered process admission. |
| Three WASM paths call only the legacy token/call guard | **Retired.** Proofs 4-6, 11-13, and 15 cover current-state and ordered WIT/s32/s32+Toy admission. |
| `AuthorizedToyCall::admit_effect_for_call_at` compares policy/grants with the call | **Retired.** Proofs 7-9, 12-13, and 15 cover fresh identity, exact grant, scope, and Mother time. |
| `MotherAuthorityOrderV1` has no production consumer | **Retired.** Proofs 4-6 and 14-15 require resident/offline mutation and Child/Toy effect consumers. |
| Hello request/response and peer wire carry no local snapshot | **Retained — slice 6.** No schema or serialization change in Phase J. |
| Idempotent replay carries no local execution snapshot | **Retained — Review 3 under D-J.2.** Replay performs no new external effect. What a completed replayed response means after authority changes is an authority-semantics question within the caller- and Child-observable semantics of authority change during and after execution; it is not process supervision or Phase J effect-start admission. |

Any failure of the old pin test must correspond to one row above. The test is replaced by a positive source/behavior audit; broad deletion or unrelated source drift is a defect.

## Gate G1 Allium tend

The current `EffectBoundaryRevisionGuardIsDistinct`, `EffectAdmissionIsOrderedWithAuthorityMutation`, `EffectPermitCannotRefreshItself`, `EveryToyEffectRevalidatesCurrentAuthority`, `ToyEffectChecksExactGrant`, and delegated-capability wording already states the Phase J observable law. Gate G1 approved one clarification of the legacy-narrow phrase.

Applied edit in `layer/allium/mct-product-map.allium`, contract `TwoPhaseRouting`, invariant `EffectBoundaryGuardCannotRepairStaleAuthority`:

```diff
-        -- A revision mismatch denies before the child effect; the adapter cannot refresh, widen,
-        -- or reinterpret the already-minted authority token as current.
+        -- A policy or namespaced grants-authority mismatch, exact-grant denial, expiry, or
+        -- unprovable currentness denies before the protected effect; no Child or Toy adapter can
+        -- refresh, widen, or reinterpret the already-minted authority token as current.
```

This is clarification of ratified law, not a new behavior. No other Allium edit is authorized. `allium check layer/allium` was clean before and after the tend.

## Phase J required proof steps

Each proof lands as a named failing test before its implementation. Close-out cites the landed file/line and quotes the verbatim central assertion.

1. Minted token authority identity equals the evaluation snapshot's for hostile, absent, and absurd call echoes alike.
2. Production-shaped M2b kill test: authority revoked after route mint causes typed Child denial before adapter execution, with no synthetic current snapshot.
3. An unrelated grant mutation after mint denies the next effect; retry performs a complete new evaluation and mints a new token without implicit refresh.
4. A revocation committed before production Child admission denies; an effect-start handoff before revocation proceeds, and subsequent admissions deny.
5. Poisoned writer and `commit_unknown` fence production Child admission until ratified recovery, then un-fence against the exact recovered state.
6. Committed revocation with projection lag denies Child admission even when the stale projection would allow.
7. Toy revocation after adapter construction denies at effect time and the backend is not invoked.
8. Matching generation with an inactive or missing exact grant denies, proving the exact-grant belt independently of generation.
9. Grant and token time bounds are checked at effect on the Mother clock; a token expiring during a running Child denies its next Toy effect.
10. A delegated preopen admitted before authority change survives until bounded expiry; a new delegation after revocation denies; its expiry equals the effective-deadline clamp.
11. All three WASM invocation paths compare token grants identity with a fresh current snapshot rather than policy-only/call-only authority.
12. A token minted under a prior authority epoch denies at effect after restart.
13. Hostile caller echoes cannot influence minting, revalidation, or admission at any Child, Toy, process, or WASM boundary.
14. Every Phase I pinned site maps to a new proof or the explicit slice-6/Review-3 residue above; no pin retires without replacement evidence.
15. A full resident call injects canonical revocation post-mint, post-adapter-construction, and mid-Child at the applicable boundaries; each path returns the correct typed denial and leaves its external effect marker absent.
16. An otherwise-current Toy grant with `max_uses = Some(1)` denies at effect admission as typed `consumption_state_unavailable`, while the otherwise-identical grant with `max_uses = None` admits.

## Phase J close-out

Phase J is complete on `1fa116f`. The implementation range from baseline `6b179bf` is:

| Commit | Purpose |
|---|---|
| `fa87ed3` | Ratified effect-boundary design, proof plan, and pin-retirement map. |
| `b1779cd` | Added the reliability doctrine later activated by Gate G1. |
| `b46a54a` | Applied the approved Allium tend, activated the doctrine, and recorded D-J.1/D-J.2. |
| `47e2d3a` | Minted route/Child execution authority only from the evaluating local snapshot. |
| `2b5aa2b` | Replaced the resident call-echo guard with fresh snapshot Child admission. |
| `befddeb` | Adopted the one Mother mutation/effect order in resident and offline production paths. |
| `214677f` | Added fresh exact Toy/backend/delegation revalidation and bounded token duration. |
| `1fa116f` | Closed exact-grant, backend-marker, Mother-clock, delegation, ordered-handoff, and full-resident adversarial evidence. |

Named tests were written and exercised against the target seam before each worktree slice was accepted; implementation and its now-green proof landed together. No intentionally red commit was retained. The deterministic idempotency clock defect and the warnings-denied `too_many_arguments` finding reproduced, were repaired, and are not flakes.

### Sixteen proof citations

| # | Landed evidence | Verbatim central assertion |
|---:|---|---|
| 1 | `crates/mct-kernel/src/route.rs:1373` — `snapshot_sourced_execution_tokens_ignore_hostile_caller_echoes` | `assert_eq!(child_authority, route_authority);` |
| 2 | `crates/mct-daemon/src/daemon/resident/execution.rs:1302` — `full_resident_post_mint_mutation_denies_then_retry_remints` | `assert!(!marker_path.exists(), "stale token starts no process effect");` |
| 3 | Same full-resident test, lines 1366-1384 | `assert!(marker_path.exists(), "retry re-evaluates and mints wholly new current authority");` |
| 4 | `crates/mct-daemon/src/authority_order.rs:623` — `revocation_first_denies_while_effect_start_first_runs_exactly_once` | `assert_eq!(starts.load(Ordering::SeqCst), 0, "revocation-first must start no effect");` |
| 5 | `crates/mct-daemon/src/authority_order.rs:493` — `uncertainty_and_projection_lag_fence_until_exclusive_reopen_and_exact_proof` | `assert_eq!(boundary.fence_reason(), Some(reason));` |
| 6 | Same fence/projection test, lines 530-578 | `assert_eq!(starts.load(Ordering::SeqCst), 1);` only after fresh-tenure rescan and exact projection proof. |
| 7 | `crates/mct-daemon/src/toy.rs:1132` — `current_toy_revocation_denies_before_order_and_echo_backend` | `assert_eq!(report.output_json, None, "denied Echo backend emits no marker");` |
| 8 | `crates/mct-kernel/src/toy.rs:1406` — `toy_revocation_and_missing_exact_grant_deny_at_effect_time` | `ToyEffectAdmissionDenyV1::ExactGrantMismatch, "matching generation cannot hide a changed exact canonical grant"` |
| 9 | `crates/mct-daemon/src/toy.rs:1080` — `token_expiring_during_child_denies_the_next_toy_backend_effect` | `assert_eq!(second.authority_denial, Some(ToyEffectAdmissionDenyV1::TokenExpired));` |
| 10 | `crates/mct-daemon/src/wasm.rs:3656` — `delegated_preopen_survives_revocation_while_new_delegation_denies` | `assert_eq!(ordered_starts.load(Ordering::SeqCst), 1, "revoked new delegation never reaches ordered preopen installation");` |
| 11 | `crates/mct-daemon/src/authority_order.rs:701` — `phase_j_pin_retirement_maps_each_old_seam_to_proof_or_named_residue`, composed with the common resident fresh Child guard | `assert!(resident_effect.contains("admit_effect_with_snapshot(&call, &effect_snapshot)"));` |
| 12 | `crates/mct-kernel/src/toy.rs:1449` — `toy_effect_uses_mother_time_and_rejects_prior_epoch` | `ToyEffectAdmissionDenyV1::GrantsAuthorityMismatch` for the restarted epoch. |
| 13 | `crates/mct-kernel/src/toy.rs:1482` and `crates/mct-kernel/src/route.rs:1373` | `assert!(token.admit_effect_with_snapshot(&hostile, &snapshot).is_ok());` |
| 14 | `crates/mct-daemon/src/authority_order.rs:701` — positive source/behavior pin map | `assert!(!resident_effect.contains("current_resident_route_revisions"));` while slice-6 and Review-3 residues remain asserted. |
| 15 | Full-resident proof at `execution.rs:1302`, post-construction Toy proof at `toy.rs:1132`, and running-Child expiry proof at `toy.rs:1080` | Each denial leaves its external marker/output absent; the full resident ledger contains `GrantsAuthorityMismatch`. |
| 16 | `crates/mct-kernel/src/toy.rs:1507` — `consumption_bearing_grant_denies_while_unbounded_grant_admits` | `assert_eq!(denied.unwrap_err(), ToyEffectAdmissionDenyV1::ConsumptionStateUnavailable);` |

### Pin retirement and audit status

The proof-14 test positively maps every Phase I pin. Resident call-derived revision copying is gone; route/Child/Toy tokens carry local snapshot authority; fresh Child/Toy admission and `MotherAuthorityOrderV1` have production consumers; exact Toy catalog/grant/time/consumption checks precede backend/preopen start. The only retained authority pins are the slice-6 hello/call peer-wire echo shape and the D-J.2 Review-3 completed-replay/response-semantics shape.

Audit disposition is therefore: M2a local authority provenance **closed**; M2b stale Child effect guard **closed** by the full-resident kill/retry proof; M2c current exact Toy authority **closed**; M2d peer-wire freshness echo **retained for slice 6**, not over-claimed by Phase J; M5 exact call/effect binding and bounded deadline enforcement **closed**. No Phase J claim adds immediate recall of already-admitted delegation.

### Current response behavior recorded for Review 3

A stale Child token denied before adapter start currently produces the existing denied result/observation shape with no `route_taken`. A Toy denial during a running Child currently follows the existing host-adapter error/result and close-out observation path; Phase J adds no new Child-visible response contract. A completed idempotent replay performs no new protected effect and currently replays the stored completed response without a local execution snapshot. D-J.2 leaves the meaning of those mid-execution and replayed responses to Review 3.

### Validation and flake log

Final Phase J validation passed:

- `cargo test --workspace`: **504 passed, 1 ignored, 0 failed**;
- `cargo clippy --workspace --all-targets -- -D warnings`: clean;
- `./scripts/ci-tier0.sh`: release/version/notes, RustSec, workspace tests, and Allium clean;
- `allium check layer/allium`: all three specifications clean;
- `patina spec check grants-authority-v0 --json`: all criteria pass after this close-out;
- `git diff --check`: clean.

Phase J flake log: **empty**. No non-reproducing failure invoked the five-rerun protocol. The earlier pre-Phase-J trigger ledger-lock collision remains preserved in its originating phase record and is not reclassified here.

Track 3 now records the thirteen slices-7/8 invariants moved from `DEFERRED` to `COVERED`: **27 COVERED / 0 LAW-LEADS-CODE / 4 DEFERRED** within the 31 grants-authority invariants. The four deferrals are slice-6 peer-wire law. Review-3 response semantics remain separately recorded under D-J.2.

## Updated fence

Phase J deliberately retires only the slice-7/8 rows named above. The following remain forbidden, including partial implementation:

- no hello/call schema, peer-wire generation echo, or early protocol stale-rejection change; slice 6 owns them;
- no new caller- or Child-visible semantics for authority change during or after execution: Review 3 owns mid-execution Toy/authority denial and completed-replay response semantics under D-J.2;
- no per-operation delegation mediation, active preopen revocation, or capability recall;
- no performance cache, group commit, profiling instrumentation, or authority-path restructuring beyond the required integration;
- no new canonical fact kind or authority change variant;
- no second mutation/effect order, token family, writer, or cross-file transaction claim; and
- no H1 process cleanup/reaping/capacity semantics.

## Validation and close-out

Every implementation commit and the final close-out must pass:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

The final close-out additionally runs `allium check layer/allium`, `patina spec check grants-authority-v0 --json`, and `git diff --check`. A failing test is rerun in isolation up to five times; a non-reproducing failure is retained verbatim in the Phase J flake log, while a reproducing failure is fixed before advancement.

Close-out is reconstructed from `6b179bf..HEAD`: commit purpose, all sixteen proof citations and verbatim assertions, complete pin retirement, full validation transcript, flake log even when empty, Track 3 terminal dispositions, M2a-M2d/M5 status, current caller- and Child-visible mid-execution and completed-replay behavior, and the active session update. Remaining board order is slice 6, then performance Phase 0 profiling on the covered revision and the ratified optimization sequence; Review 3 begins from the recorded caller- and Child-observable authority-response behavior under D-J.2.

**Phase J Gate G1:** ratified. The Allium clarification and doctrine activation are the first Task B action; Rust work follows only under D-J.1, D-J.2, the sixteen required proofs, and the updated fence.

# Phase K — slice 6 peer generation advertisement and echo

> Hello advertises the receiving Mother's complete current grants-authority identity; a call echoes that identity only so the receiver can reject stale work before route evaluation.

## Scope and gate

Phase K implements the final grants-authority slice and only:

1. advertises the receiver's proof-gated namespaced grants-authority identity in an admitted hello;
2. requires each peer or local call to carry that complete identity as its expected receiver authority;
3. rejects mismatch before route evaluation as typed temporal staleness with durable correlation evidence; and
4. retires the legacy bare `grants_revision` call-echo semantics, including the fenced `call/internal.rs:223-224` comparison and parent-copied internal Child callouts.

The Phase K baseline is `57c7ab0` on branch `patina`. D-G1 through D-G8, D-R2.1 through D-R2.8, D-H2.1, D-H3.1 through D-H3.4, D-I.1 through D-I.2, and D-J.1 through D-J.2 remain settled. Phase K sits strictly before the Phase I route-evaluation and Phase J effect-admission layers. A newly discovered genuine behavioral fork stops for an operator-supplied D-K.n amendment.

- **D-K.1 — provenance-copy disagreement is malformedness:** the semantic-call and peer-protocol expected-receiver identities must agree completely before either is compared with current receiver authority. Disagreement rejects typed as malformed ingress, not temporal staleness; the receiver must not select, merge, or prefer either copy. Proof 11 establishes this ordering and classification.

Task A inventories and proposes the one wire/schema disposition below. Gate G1 was ratified from disk at `d47a359`; the approved Allium tend and D-K.1 clarification are the first Task B action.

## Hello advertisement contract

An admitted `mct/hello/0` response carries one opaque `GrantsAuthorityIdentity` with all four fields:

```text
receiving_grants_authority: {
  mother_node_id,
  authority_epoch,
  generation,
  source_authority_observation_id
}
```

The value comes from a newly constructed `LocalExecutionAuthoritySnapshot` only after the current ledger head, canonical replay, authority projection, and exact D-G8 proof agree. It is not sourced from startup readiness, peer policy, configuration, a prior hello, or the caller. The advertised identity exactly equals `snapshot.canonical_grants.grants_authority` and names the receiving Mother.

Hello peer-binding evaluation still runs first and remains independent authority. For a peer-binding-admissible hello, inability to construct the local execution snapshot because the projection is stale, rebuilding, missing, quarantined, foreign-lineage, replay-blocked, or otherwise unprovable changes the hello to the existing typed `retry_later`/`temporary_unavailable` degraded refusal. Such a response carries no `receiving_grants_authority` and no capability view. A structurally denied hello likewise advertises no identity. No response can contain a usable identity unless its hello outcome is `admitted` and the exact current proof succeeded.

The hello identity is change-detection evidence, not a capability publication or grant. Advertising it grants no peer admission, route, Child, Toy, data, or effect authority.

## Call echo and early-rejection contract

The required echo is the full namespaced identity, never a bare or order-compared counter. Both the semantic call authority context and its peer protocol authority carry `expected_receiver_grants_authority: GrantsAuthorityIdentity`; they must be byte-value equal. There is no absent/legacy form and no default. JSON missing the field, carrying the former integer, or carrying malformed identity content is rejected as a typed malformed call at decode/validation before route evaluation.

For a well-formed call, peer and local ingress construct a fresh proof-gated local snapshot and compare the expected identity with its complete current local identity before route evaluation, idempotency reservation/replay, payload-dependent Child work, or any Phase I/J authority consumer. Exact inequality in Mother, epoch, generation, or source authority observation produces:

- `CallProtocolReason::ExpectedReceiverAuthorityStale`;
- denied protocol evaluation with no route decision and no handler invocation;
- caller-safe `CallProtocolRetryDirective::RefreshHello` on the peer/local response; and
- the existing durable `CallDenied` lifecycle observation as correlation evidence, recording the complete expected and current identities without treating either as a grant.

Failure to prove a current local identity produces the distinct typed `CallProtocolReason::ReceiverAuthorityUnavailable`, no route evaluation, and caller-safe `CallProtocolRetryDirective::RetryLater`. It never compares against stale projection state.

Exact agreement has no positive result. It only permits execution to continue to the unchanged current peer-binding, payload, idempotency, Phase I route-evaluation, and Phase J effect-admission checks. It does not cache the snapshot as an effect permit, skip any evaluation, mint authority, refresh a token, or alter a later denial. The early gate may remove an outcome by rejecting sooner; it can never widen one.

Peer forwarding copies the admitted hello response's complete receiver identity into both expected-identity positions of the newly constructed per-hop call. It never forwards the original call's expectation. Re-hello is the only refresh path after `RefreshHello`.

## C5 legacy `grants_revision` consumer inventory and proposed disposition

Disk inspection found two legacy call-carried bare fields, not one: `AuthorityContextSnapshot.grants_revision` and `MctCallProtocolAuthority.grants_revision`. The landed Allium model already represents both as `expected_receiver_grants_authority: GrantsAuthorityIdentity`. The Phase K proposal is immediate **replacement** of both Rust `u64` fields with one wire-serializable complete identity value in each provenance position. There is no deprecated alias, optional fallback, serde default, integer promotion, or compatibility reader.

The similarly named `ToyGrant.grants_revision`, route/Toy evaluation generations, canonical authority-fact generation, projection cursor generation, and `MctObservation.grants_revision` are different established records; they are not renamed wholesale.

| Consumer class and complete production sites | Current use | Proposed disposition and compatibility consequence |
|---|---|---|
| Semantic call/wire schema: `mct-kernel/src/call/mod.rs` (`AuthorityContextSnapshot`), `MctCall`, JSON call envelopes in `mct-iroh/src/serve.rs`, and every call constructor/fixture | Bare call-carried `u64` | **Replace.** Required `expected_receiver_grants_authority` object. Old/missing/integer JSON is malformed immediately; compile failures force every Rust constructor to choose an explicit complete identity. |
| Peer protocol authority: `mct-kernel/src/call/mod.rs` (`MctCallProtocolAuthority`) and `call/internal.rs:223-224` | A second bare minimum revision and `<` comparison | **Replace.** Required complete expected identity, exact equality with the semantic call copy, then exact comparison with a separately supplied current local identity. Remove numeric ordering; wrong Mother/epoch/source with equal generation rejects. |
| Hello response and peer forwarding: `mct-kernel/src/peer/mod.rs`, `mct-iroh/src/serve.rs`, `daemon/resident/serving.rs`, `daemon/resident/forwarding.rs`, and `daemon/ingress.rs` CLI peer call construction | Hello has no authority identity; forwarded call copies the original caller's grants number or fixture constant | **Replace.** Admitted response requires the current receiver identity; forwarding/CLI peer calls use that response value. Degraded hello carries none and no call is formed. No old peer is supported. |
| Local UDS/JVM and local CLI ingress: `daemon/resident/local_ingress.rs` and `daemon/ingress.rs` | Submission/bridge JSON carries or fabricates a bare number; protocol authority copies it | **Replace.** External local submission shape requires the complete expected identity and verifies it against a fresh local snapshot. Mother-owned CLI/JVM construction reads the local current identity; it cannot fabricate `1`. Existing old JSON breaks deliberately under C3. |
| Trigger and internal Child-originated construction: `daemon/resident/trigger_scheduler.rs` and `daemon/resident/pipeline.rs` | Trigger uses policy revision as grants revision; Child callout copies the parent's complete authority context | **Replace.** Each locally constructed call obtains the executing Mother's fresh current identity. A Child callout preserves call/trace lineage but does not inherit the parent's expected receiver identity. Unprovable local identity suppresses call construction typed. |
| Route/evaluation correlation: `mct-kernel/src/route.rs`, `mct-kernel/src/toy.rs`, `daemon/authority_test_fixture.rs`, `resident/candidates.rs`, and `resident/decision.rs` | Legacy APIs/tests read the call number; Phase I production evaluation already ignores it for authority and records it as an echo | **Replace/reinterpret.** Production authority remains snapshot-only. Correlation reads use the complete expected identity; any legacy helper still comparing the call number is migrated or removed so agreement cannot admit or deny below the early gate. Test matrices mutate complete identity components, not numeric echoes. |
| Process/WASM/Toy/supervisor/payload/execution observations: `process.rs`, `wasm.rs`, `toy.rs`, `supervisor.rs`, `resident/payload.rs`, `resident/execution.rs`, `resident/pipeline.rs`, and `resident/forwarding.rs` | Call-derived observations project the bare echoed number into `MctObservation.grants_revision` | **Reinterpret at this compatibility boundary.** Preserve the established observation schema; call-derived rows store `expected_receiver_grants_authority.generation` only as a lossy correlation projection. The complete expected/current identities for stale rejection are encoded in the existing non-authorizing correlation detail. Canonical fact and local evaluation observations keep their existing locally sourced generation meaning. |
| Iroh lifecycle observations: `mct-iroh/src/serve.rs` (`MctIrohCallLifecycleFact`) | Lifecycle fact copies the call number into `MctObservation.grants_revision` | **Reinterpret** exactly as above. The stale `CallDenied` fact additionally retains complete expected/current identity correlation and the refresh directive through existing detail evidence; no new observation kind or canonical fact kind is added. |
| Idempotency: `daemon/resident/idempotency.rs` | The idempotency observation copies the call number; the actual fingerprint is only canonical target, semantic call id, and payload digest | **Reinterpret observation; fingerprint unchanged.** The disk inventory contradicts the premise that `grants_revision` participates in the fingerprint. Phase K does not add it, does not redesign replay, and preserves D-J.2. Crucially, the fresh expected/current comparison occurs before reserve or replay. |
| Tests and synthetic fixtures across `mct-kernel`, `mct-iroh`, and `mct-daemon` | Construct `AuthorityContextSnapshot { grants_revision: ... }` or assert old source pins | **Replace.** Fixtures use explicit complete identities; hostile tests vary absence/malformed wire bytes and Mother/epoch/generation/source components. The Phase J pin map is updated only for its named slice-6 residue. |

`MctObservation.grants_revision` therefore coexists as an established lossy numeric projection, not as a deprecated authority source. No code may reconstruct a `GrantsAuthorityIdentity` from that field or compare it to establish currentness.

## `call/internal.rs:223-224` and internal-call migration

The old check:

```text
call.policy_revision < protocol.policy_revision
or call.grants_revision < protocol.grants_revision
```

is retired. Peer policy revision remains governed by current binding revalidation. Grants staleness is replaced by one shared kernel decision that receives two provenance-distinct values: the caller's required complete expected identity and the receiver adapter's fresh proof-gated local identity. It decides exact match or one of the typed stale/unavailable refusals; it has no allow token and no access to route/effect evaluation.

The peer protocol copies must agree exactly before comparison with local state. For local and Child-originated calls, the Mother constructs both copies from her own current snapshot at call construction and nevertheless performs the same fresh ingress comparison. Hostile parent-supplied identity cannot enter the child-originated call. If authority changes between construction and ingress, the internal call is early-rejected and reconstructed only by a complete new local call attempt.

## Proposed Allium tend for Gate G1

The existing value shapes and the four governing hello invariants already state the target identity and non-authorizing semantics. Phase K proposes exactly the following additions after ratification.

### Exact hello and internal-construction decision edits

```diff
 -- Decision: `mct/hello/0` may advertise a policy-filtered capability view, but capability advertisement is not a grant. Child access, toy use, thought acceptance, and observation replication still require their own authority checks.
+-- Decision: An admitted hello advertises `receiving_grants_authority` only when the receiving
+-- Mother can prove her complete current namespaced authority identity. Stale, rebuilding,
+-- quarantined, missing, or otherwise unprovable receiver authority produces `retry_later` with
+-- no usable identity or capability view; a denied hello likewise advertises neither.
```

```diff
 -- For Iroh arrivals, local-only evaluation follows from the submitted `mct/call/0` question under
 -- companion contract `TerminalPeerCallSubmission`, not from an origin-specific permission gate.
+-- Decision: A Mother-internal call uses that Mother's proof-gated current grants-authority
+-- identity as its expected receiver identity when constructed. Parent-call or Child-supplied
+-- identity is correlation lineage only and is never copied into the new call as authority.
```

### Exact protocol value/surface edits

```diff
 entity MctCallProtocolEvaluation {
     ...
-    reason: hello_not_admitted | alpn_not_admitted | endpoint_mismatch | binding_revoked | binding_expired | policy_revision_stale | malformed_call | payload_metadata_mismatch | authority_denied | no_route | execution_failed | execution_timed_out | result_recorded | idempotency_key_reuse_mismatch | idempotency_budget_full | idempotency_in_progress | idempotency_replay_completed
+    reason: hello_not_admitted | alpn_not_admitted | endpoint_mismatch | binding_revoked | binding_expired | policy_revision_stale | expected_receiver_authority_stale | receiver_authority_unavailable | malformed_call | payload_metadata_mismatch | authority_denied | no_route | execution_failed | execution_timed_out | result_recorded | idempotency_key_reuse_mismatch | idempotency_budget_full | idempotency_in_progress | idempotency_replay_completed
+    retry_directive: none | refresh_hello | retry_later
     safe_message: String
     observation_id: String
 }

 entity MctCallProtocolReply {
     ...
     reply_outcome: success | denied | failed | timed_out | cancelled | malformed
+    retry_directive: none | refresh_hello | retry_later
     safe_message: String
     reply_observation_id: String
 }
```

```diff
 surface MctCallProtocolEvaluationProjection {
     ...
         evaluation.reason
+        evaluation.retry_directive
         evaluation.safe_message
         evaluation.observation_id
 }

 surface MctCallProtocolReplyProjection {
     ...
         reply.reply_outcome
+        reply.retry_directive
         reply.safe_message
         reply.reply_observation_id
 }
```

The required directive is `none` for structural denial and every pre-existing terminal shape. `refresh_hello` is only the safe response to a well-formed expected/current identity mismatch; `retry_later` is only the safe response when current receiver authority cannot be proved. It does not change D-J.2 replay behavior.

### Exact `MctCallProtocol` invariant edits

```diff
 contract MctCallProtocol {
     ...
     @invariant HelloDoesNotPreAuthorizeCall
         -- Hello admission permits the peer to submit a call envelope, but each call still passes authority, routing, ToyGrant, child assignment, data policy, and revalidation checks.
+
+    @invariant ExpectedReceiverIdentityIsCompleteAndConsistent
+        -- The protocol-authority and semantic-call copies of expected receiver authority contain
+        -- the same complete Mother, epoch, generation, and source-observation identity. Missing
+        -- or malformed copies are refused, and disagreement is malformedness rejected before
+        -- either copy is compared with current receiver authority; neither copy is preferred.
+
+    @invariant ReceiverIdentityComparisonPrecedesRoutingAndReplay
+        -- A well-formed expected identity is compared with freshly proved local receiver authority
+        -- before route evaluation or idempotent replay. Unprovable local authority retries later;
+        -- mismatch directs the caller to refresh hello.
+
+    @invariant EchoAgreementCannotAdmit
+        -- Expected/current identity agreement can only avoid the early stale rejection. It grants
+        -- no authority and skips no peer, route, Child, Toy, data, deadline, or effect evaluation.
+
+    @invariant StaleEchoIsDurablyCorrelated
+        -- Early stale rejection records the complete expected and current identities as correlation
+        -- evidence before returning its safe refresh directive; neither identity becomes a grant.
```

No entity lifecycle, peer ontology relationship, authority source, observation kind, canonical fact kind, or Review 3 replay semantic changes. `allium check layer/allium` must remain clean after the ratified tend.

## Phase K implementation tasks

### B1 — hello advertisement

Add the wire identity value and source admitted hello responses from the current local snapshot. Bind snapshot unavailability to the existing degraded hello refusal and prove no usable identity/capability advertisement escapes.

### B2 — echo and early rejection

Require the complete expected identity at peer and local ingress, compare it with a fresh current identity before route evaluation/idempotency, emit typed retry direction and durable correlation evidence, and prove agreement has no positive authority effect.

### B3 — retire legacy revision semantics

Migrate every inventoried constructor, observation projection, forwarding path, legacy helper, and source pin. Internal Child callouts obtain current local identity rather than parent-carried authority. Preserve Phase I/J evaluation and effect behavior byte-for-behavior except for the new earlier rejection possibility.

## Phase K required proof steps

Each proof lands as a cited test. Close-out cites its file/line and quotes the verbatim central assertion.

1. Hello advertises the current complete identity; after one canonical grant mutation and coherent projection publication, the next hello advertises the same namespace/epoch with generation advanced exactly once and the new source observation.
2. A stale echoed identity is durably rejected as `ExpectedReceiverAuthorityStale` with `RefreshHello` before route evaluation or idempotency, complete expected/current correlation is retained, and re-hello plus retry succeeds.
3. Matching expected identity grants nothing: with current echo but revoked underlying exact grant, unchanged Phase I/J local evaluation denies and no effect starts.
4. Correct generation under the wrong Mother, wrong epoch, or wrong source authority observation is rejected typed before routing.
5. Hostile echo matrix: absent field, former integer shape, malformed identity, absurd/future generation, and well-formed mismatch cannot widen an outcome; absent/malformed are typed malformed ingress and well-formed mismatches are typed temporal staleness.
6. Two-Mother wire flow succeeds after hello; a receiver mutation between hello and call rejects early; re-hello returns the advanced identity and the retry succeeds.
7. A pre-restart identity is early-rejected after a fresh writer-tenure epoch; re-hello recovers with the new epoch.
8. Stale, rebuilding, missing, quarantined, foreign-lineage, or replay-blocked receiver authority produces no usable hello identity/capability view and calls refuse with the exact typed unavailable posture.
9. Child-originated internal calls carry the constructing Mother's fresh identity in both positions; hostile parent values are absent from the constructed call, and post-construction mutation maps to the shared early-stale decision.
10. All Phase I route-evaluation and Phase J effect-boundary proofs pass unchanged; matching echo never suppresses their local snapshot construction, exact-grant checks, or effect admission.
11. A call whose semantic-call and peer-protocol expected-receiver identities disagree is rejected typed as malformed ingress before any comparison with current receiver identity; neither copy is selected, merged, or preferred.

## Expected terminal ledger and audit state

At close-out the four remaining grants-authority Track 3 rows move from `DEFERRED` to `COVERED` with Phase K citations:

- `MctHelloProtocol.PeerEchoOnlyDetectsStaleness`;
- `ForgedCurrentGenerationDoesNotGrantAuthority`;
- `ReceiverAlwaysUsesLocalAuthority`; and
- `GenerationNamespaceMustMatchReceiver`.

The grants-authority section must then read **31 COVERED / 0 LAW-LEADS-CODE / 0 DEFERRED**. Audit item M2d, peer-wire freshness echo, becomes closed. The original Review 1 grants-authority finding set is fully terminal if and only if those counts and M2a-M2d/M5 close-out claims are all backed by the eleven landed proofs. D-J.2 Review 3 response semantics remain a separate board item, not a non-terminal grants-authority row.

## Updated Phase K fence

Phase K changes only hello/call identity advertisement, early stale/unavailable rejection, correlation evidence, and required constructor/schema migration. It must not add or change:

- any Phase I route-evaluation authority input or decision;
- any Phase J token, effect-admission, exact-grant, delegation, or `MotherAuthorityOrderV1` behavior;
- completed idempotent replay or mid-execution caller-/Child-visible semantics under D-J.2;
- performance profiling, caching, batching, group commit, or optimization;
- canonical fact kinds, authority change variants, observation kinds, or ledger framing;
- policy/Child/peer canonicalization, per-operation delegation mediation, or active recall; or
- H1 process cleanup, reaping, capacity, or supervisor semantics.

If implementation needs a new authority source, new observation/fact kind, compatibility mode, replay semantic, or positive echo permit, stop for D-K.n rather than broadening the slice.

## Validation and Gate G1

Every implementation commit and final close-out must pass:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

The final close-out additionally runs `allium check layer/allium`, `patina spec check grants-authority-v0 --json`, and `git diff --check`. A failing test is rerun in isolation up to five times; a non-reproducing failure is retained verbatim in the Phase K flake log, while a reproducing failure is fixed before advancement.

**Phase K Gate G1:** ratified from disk at `d47a359`. D-K.1 adds proof 11 and fixes provenance-copy disagreement as malformed ingress before current-identity comparison. Task B proceeds only under the eleven required proofs and the unchanged Phase K fence.

## Phase K close-out evidence

Implementation landed in `9eecaed` (proof-gated hello advertisement) and `7983479` (complete echo schema, early rejection, internal/per-hop construction, legacy retirement, and proofs). The eleven proof obligations are cited below; line numbers are from `7983479` plus the close-out worktree.

| Proof | Landed evidence and central assertion |
|---:|---|
| 1 | `crates/mct-daemon/src/daemon/resident/serving.rs:1765` — `resident_hello_publishes_federation_callable_surface`: `assert_eq!(next_authority.generation, first_authority.generation + 1)` while Mother/epoch remain equal and source observation changes. |
| 2 | `crates/mct-daemon/src/daemon/resident/pipeline.rs:1429` and `crates/mct-iroh/src/lib.rs:1377` — stale ingress retains expected/current source observations and `refresh_hello` before route/idempotency; re-hello then yields `assert_eq!(execution_count.load(Ordering::SeqCst), 1)`. |
| 3 | `crates/mct-daemon/src/toy.rs:1136` — `current_toy_revocation_denies_before_order_and_echo_backend`: matching call correlation cannot overcome current exact-grant revocation and the backend marker remains absent. |
| 4 | `crates/mct-kernel/src/call/mod.rs:1394` — wrong Mother, epoch, source, and future generation each assert `ExpectedReceiverAuthorityStale`, `RefreshHello`, and no route decision. |
| 5 | `crates/mct-kernel/src/call/mod.rs:1451` plus proof 4 — absent, former-integer, and malformed wire forms fail decode/validation; every complete mismatch remains typed temporal staleness. |
| 6 | `crates/mct-iroh/src/lib.rs:1377` — the two-Mother wire test succeeds after hello, rejects a post-hello receiver mutation without invoking the handler, and succeeds exactly once after re-hello. |
| 7 | `crates/mct-iroh/src/lib.rs:1386` — a fresh writer-tenure epoch rejects the pre-restart identity with `RefreshHello`; re-hello advertises the new epoch and retry succeeds once. |
| 8 | `crates/mct-iroh/src/lib.rs:210`, `crates/mct-daemon/src/authority_snapshot.rs:505`, `:563`, and startup quarantine/foreign-lineage proofs — every inability to construct the proof-gated snapshot collapses to admitted-neither `RetryLater`, with `receiving_grants_authority.is_none()` and `capability_view.is_none()`; stale/epoch conditions recover only after explicit coherent publication/rebuild. |
| 9 | `crates/mct-daemon/src/daemon/resident/pipeline.rs:1398` — the Child callout constructor asserts both copies equal the constructing Mother's current identity and differ from the hostile parent value; proof 4 supplies the shared post-construction stale decision. |
| 10 | `crates/mct-daemon/src/authority_order.rs:701` and the complete workspace suite — the source audit retires bare call-revision comparisons and preserves all Phase I snapshot and Phase J effect-admission seams; the unchanged route/Toy/effect proofs remain green. |
| 11 | `crates/mct-kernel/src/call/mod.rs:1369` — with current identity alternately equal to either disagreeing copy, both evaluations assert `Malformed`, `MalformedCall`, retry `None`, and no route decision, proving neither copy is selected or preferred. |

B3 retirement is complete: both call-carried bare `grants_revision` fields are gone, old/missing/integer wire forms have no compatibility reader, forwarding derives both copies from the admitted per-hop hello, local/trigger/Child construction uses proof-gated local identity, and `MctObservation.grants_revision` remains only the explicitly retained lossy correlation projection. The idempotency fingerprint and D-J.2 replay semantics are unchanged.

Track 3 is terminal for this finding family at **31 COVERED / 0 LAW-LEADS-CODE / 0 DEFERRED**. Audit disposition is M2a **closed**, M2b **closed**, M2c **closed**, M2d **closed**, and M5 **closed**. This completes the original Review 1 grants-authority finding set; Review 3 response behavior remains separately scoped by D-J.2.

### Validation and flake log

Both Phase K implementation commits passed workspace tests, warnings-denied Clippy, Tier 0/RustSec, Allium, and diff checking before advancement. Final implementation validation passed 513 tests with 1 ignored (175/1 daemon library, 155 daemon binary, 3 release archive, 2 WASM limits, 39 Iroh, 104 kernel, and 35 observation), with warnings-denied Clippy and Tier 0/RustSec clean. Final close-out also runs Allium, spec check, and diff check.

Phase K flake log: GitHub Tier 1 run `31177794761` produced one non-reproducing post-close-out failure in `supervisor_lifecycle::tests::supervised_trigger_watch_delivery_fixtures_execute_end_to_end`: `called Result::unwrap() on an Err value: Elapsed(())`, followed by `resident route denial ledger write failed: send observations to resident ledger writer` and `resident Child call-out failed: resident observation writer is fenced`. Five isolated local reruns passed (38.99–39.61 seconds), so the failure is retained verbatim as a timing flake rather than treated as a product reproducer. The legacy Toy fixture expectation, authority-test fixture alignment, source-pin formatting, and Clippy argument-count findings reproduced deterministically and were repaired before advancement.

Phase K is complete. The next grants-adjacent work is only the separately fenced D-J.2 Review 3 behavior; H1 process cleanup and performance profiling remain independent board items.

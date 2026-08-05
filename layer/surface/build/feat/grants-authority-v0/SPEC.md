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
    text: Every production mutation of the D-R2.7 Toy catalog/grant scope commits exactly one canonical mutation/import fact and advances the namespaced generation exactly once.
    checked: false
    verify: Phase I proof steps 1-2 have landed test file and line citations.
  - id: phase-i-local-snapshot
    text: Route evaluation receives one Mother-owned local execution authority snapshot only after exact D-G8 proof, with canonical grants, labeled local policy provenance, Mother clock, and cursor provenance.
    checked: false
    verify: Phase I proof steps 3-4, 6-7, and 11-13 have landed test file and line citations.
  - id: phase-i-route-evaluation
    text: Resident route evaluation uses the local snapshot rather than caller-echoed revisions, records those echoes only as correlation evidence, and fails closed on unprovable freshness.
    checked: false
    verify: Phase I proof steps 5 and 8-10 have landed test file and line citations.
  - id: phase-i-deferral-fence
    text: Token minting revision sourcing, the resident effect guard, Child/Toy/WASM/process effect guards, hello/peer wire, idempotent replay, and MotherAuthorityOrderV1 production adoption remain unchanged.
    checked: false
    verify: Phase I proof step 14 plus the close-out changed-reader and call-site audits.
  - id: phase-i-validation
    text: Every Phase I implementation commit and the final close-out pass workspace tests, warnings-denied clippy, Tier 0/RustSec, Allium, and the grants-authority spec check.
    checked: false
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
    toy_grants: [complete ToyGrant, ...]
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

The v0 composition decision is explicit: only Toy catalog/grants and their D-R2.7 grant-shaping source history are canonical. Policy, Child approval/assignment/instance, peer binding/proof, and callable-surface inputs remain Mother-local legacy projections with explicit provenance labels. Their inclusion does not canonicalize them, permit them to shape the canonical generation, or claim cross-store ACID. `LocalSnapshotIsCoherent` in Phase I means that the canonical portion and its complete cursor are one atomic projection publication, while each labeled policy portion is captured as one immutable input value and all portions are bound into one non-refreshable evaluation argument.

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

There are no production Toy catalog/grant removals or general grant-correction endpoints beyond the complete `put`/revocation surfaces above. Test-only direct upserts in `startup.rs`, `state.rs`, and Watch unit fixtures are not production ingress. Task B may repair the three Watch route families through `AuthorityMutationRequestV1` only after D-I.1 proves that the existing `toy_catalog_put` and `toy_grant_put` values are replay-complete for every projected family, including Watch observation scopes. Inability to reconstruct that scope is the named C4 stop condition, not permission to invent a new fact kind or compatibility write.

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

1. A parameterized matrix over legacy import, authorize-Slate, authorize-secret, Watch grant, each supporting-grant variant, and Watch revoke proves exactly one canonical fact and exactly one generation advance per successful logical request.
2. Every production D-R2.7 route refuses or leaves legacy Toy rows byte-value unchanged when canonical commitment is rejected, unknown, poisoned, or unavailable; no direct legacy-only production path is reachable. Replay assertions over the enveloped Watch grant, each supporting-grant variant, and Watch revoke must reconstruct the complete projected state, including every Watch observation-scope field, solely from the existing authority-change values; inability to do so is the D-I.1 C4 stop condition.
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

## Phase I deferral fence

The following remain forbidden, including partial implementation:

- No changes to `execution.rs:92-146`, token minting revision sourcing, `AuthorizedChildInvocation`/`AuthorizedToyCall` effect guards, process/WASM/Toy adapters, or delegated-capability semantics; slices 7-8 own them.
- No hello/call schema or peer-wire migration and no early protocol stale-rejection change; slice 6 owns them.
- No idempotent replay authority migration.
- No production consumer of `MotherAuthorityOrderV1`; slices 7-8 route commits and effect starts through it.
- No new canonical fact kind, authority change variant, `MctObservation` field, `MctObservationLedgerEntry` field, or ledger framing change.
- No canonicalization of policy, Child approval/assignment/instance, peer binding/proof/publication, trigger, Watch-scope, or artifact-source domains. They remain labeled local legacy inputs.
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

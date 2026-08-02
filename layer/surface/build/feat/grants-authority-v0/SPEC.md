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
    checked: false
    verify: Required proof steps 1-3 below have landed test file and line citations.
  - id: toy-token-expiry
    text: Every Toy effect admission uses the executing Mother's clock; at-or-after token expiry denies without a backend effect and before expiry proceeds.
    checked: false
    verify: Required proof steps 4-5 below have landed test file and line citations.
  - id: bounded-effective-deadline
    text: Ingress computes an effective deadline from the caller deadline, a configurable 600-second default maximum horizon, and any stricter local policy bound; WASM and Toy authorization consume only that bound.
    checked: false
    verify: Required proof steps 6-9 below have landed test file and line citations.
  - id: allium-law
    text: The product map records grants-authority identity, provenance separation, peer echo, effect admission, clock/deadline, delegated-capability, and projection-freshness law, and validates without diagnostics.
    checked: false
    verify: allium check layer/allium
  - id: attribution-ledger
    text: Every new invariant is attributed in the Track 3 ledger as targeted COVERED-by-this-phase or DEFERRED to Review 2 and slices 4-8.
    checked: false
    verify: rg -n "Grants authority v0|GrantGenerationIsMotherOwned|ExecutionTokenBindsExactCall|ToyTokenExpiryIsEnforced|CallerDeadlineCannotExtendLocalHorizon" layer/surface/build/spec-drift-audit/track3/LEDGER.md
  - id: workspace-validation
    text: Every implementation commit and the final phase pass the required workspace validation suite.
    checked: false
    verify: cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
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

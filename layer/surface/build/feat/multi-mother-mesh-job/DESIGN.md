# Design: resident mesh-job decomposition

## Why this seam

The existing resident pipeline already decides whether each call runs locally or forwards once. The missing capability is only a bounded local envelope that groups independent calls under one job identity. Putting decomposition in the authenticated resident ingress preserves the existing authority model:

```text
local application
  -> one authenticated job envelope
Mother A resident ingress
  -> ordinary piece call A -> local route -> Child on A
  -> ordinary piece call B -> remote route -> mct/call/0 -> Child on B
```

The adapter decomposes explicit data; it does not grant authority or choose an executor outside normal route evaluation.

## API shape

Use an untagged, structurally disjoint request enum at `POST /calls`:

```rust
enum MctResidentSubmission {
    Call(MctResidentCallSubmission),
    Job(MctResidentJobSubmission),
}
```

A call has `call_id`; a job has `job_id` and `pieces`, so strict deserialization cannot confuse the variants. The existing call JSON remains byte-for-byte compatible.

Job pieces omit caller, origin, peer, ALPN, route, and authority decisions. They carry only target and existing payload fields. Shared authority expectation, deadline, and job trace live once on the parent envelope. The resident derives all per-piece identities deterministically after validating the full envelope.

## Validation before effects

Before executing piece zero, validate:

- 2..=16 pieces;
- portable job/piece IDs and unique piece IDs;
- every derived ID fits the existing domain newtypes;
- every inline payload base64 decodes;
- every piece request validates under the existing protocol request rules; and
- total input remains inside the existing frame budget enforced before body read.

No piece begins when another piece is malformed.

## Execution and retry

Execute in declaration order. Each piece calls `execute_resident_call` exactly as a single local submission does. Derived idempotency keys are stable, so retry after a lost aggregate response replays completed pieces rather than executing them twice.

Continue after a typed piece denial/failure so the caller receives the disposition of every independent piece. Stop only when the ledger cannot make the next required durable observation, because returning or continuing after writer failure would violate append-before-effect.

## Observation projection

Use existing observation kinds:

- parent `CallReceived`: authenticated mesh job accepted, `call_id = job_id`;
- parent `CallConstructed`: all pieces validated, safe detail contains piece count only;
- ordinary per-piece observations from the existing pipeline;
- parent `ResultRecorded`: aggregate outcome and completed/denied/failed counts.

No new `ObservationKind` is needed. Detail fields contain no payload bytes, paths, addresses, credentials, or policy internals.

## Aggregate response

```text
MctResidentJobResponse {
  job_id
  outcome            # completed only if all pieces completed
  pieces: [
    { piece_id, call_id, <existing resident call response fields> }, ...
  ]
}
```

The response is caller-safe and bounded by the existing control response cap. It makes no atomicity or rollback promise.

## Physical proof reuse

Reuse two landed fixtures rather than adding a Toy:

- PR #105 arithmetic Child for A-local execution;
- PR #107 `mct-repo-report` plus `toy:mct:directory-read` on B.

Package the repo-report component, manifest, and Cargo.lock snapshot into the federation test closure built on nicamaster. The helper copies the immutable fixture into its private run-owned data root, stages the Child through the real artifact path, approves it, and issues the caller-scoped supporting grant through the real canonical authority mutation path.

The grant expiry is chosen only after peer setup and publication readiness. The controller records the exact expiry, completes the covered job before it, waits until Mother time is past it, and sends the uncovered probe. The ledger, not wall-clock assumption alone, must show the typed `ToyGrantExpired` refusal and zero execution.

## Test order

1. Red Rust tests for strict job envelope validation, deterministic piece IDs, aggregate response, and per-piece idempotent replay.
2. Implement the smallest local-ingress decomposition.
3. Red helper/controller tests for repo-report fixture provisioning, two-piece result validation, ledger attribution, and expired-grant refusal.
4. Extend the federation bundle and build script.
5. Run repository gates.
6. Build once on nicamaster, import the closure on nicanode0, run the physical job, and commit sanitized evidence.

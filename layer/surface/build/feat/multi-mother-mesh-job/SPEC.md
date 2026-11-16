---
type: feat
id: multi-mother-mesh-job
status: active
created: 2026-11-05
target: nic-63-first-real-mesh-job
related:
  - docs/DECISIONS.md
  - docs/nic63-multi-host-plan.md
  - docs/nic63-post-review-suite.md
  - docs/nic73-lume-mother.md
  - layer/surface/build/feat/resident-call-ingress/SPEC.md
  - layer/surface/build/feat/multi-mother-route-forwarding/SPEC.md
  - crates/mct-daemon/src/daemon/resident/local_ingress.rs
  - crates/mct-daemon/examples/federation_host.rs
  - scripts/federation/fleet.mjs
exit_criteria:
  - id: one-job-two-pieces
    text: One authenticated local submission to Mother A names a bounded ordered set of pieces; Mother expands each piece into the existing immutable MctCall pipeline and returns one aggregate result without creating a second authority or execution model.
    checked: true
    verify: cargo test -p mct-daemon --bin mct-daemon resident_mesh_job_
  - id: local-and-remote-execution
    text: The NIC-63 physical run completes one piece on Mother A and one single-hop forwarded piece on Mother B, with exact job/piece correlation and no execution attribution to the forwarding Mother for B's piece.
    checked: true
    verify: node --test scripts/federation/mesh-job.test.mjs && physical evidence document
  - id: existing-toy-coverage
    text: Mother B reuses the mct-repo-report Child and scoped directory-read ToyGrant from the first-real-app proof; a covered piece runs before grant expiry and a later piece is refused after expiry with the typed ToyGrantExpired fact and zero runtime starts. No board.post Toy is added.
    checked: true
    verify: physical evidence document plus canonical ledger summaries
  - id: build-once-delivery
    text: The daemon and helper are built once on nicamaster; the exact previously accepted PR #107 mct-repo-report component is reused there with its pinned digest. Native artifacts, manifest, component, and input fixture are packaged in one signed immutable closure and imported on nicanode0 without Cargo or Nix compilation there.
    checked: true
    verify: package provenance and deployment receipts in physical evidence
  - id: repository-gates
    text: The requested repository gates pass on the final diff.
    checked: true
    verify: cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && bash scripts/ci-tier0.sh specs && git diff --check
---

# NIC-63 first real multi-Mother mesh job

## Goal

Run one real two-piece job through Mother A on nicamaster. Mother A evaluates both pieces through the resident pipeline, executes the local piece herself, and delegates the other piece over the already-proven single-hop Iroh federation to Mother B on nicanode0. Then submit a third piece for B after B's exact ToyGrant has expired. B must durably deny the Toy before any Child runtime starts.

## Architecture decision

The smallest truthful job surface is an authenticated local resident submission that contains a bounded ordered list of ordinary call envelopes. The resident adapter owns only decomposition and aggregation:

1. authenticate the same owner UDS principal used by `POST /calls`;
2. validate the job envelope and every piece before beginning the first piece;
3. derive one ordinary `MctCallProtocolRequest` per piece;
4. execute each piece through the existing payload, idempotency, route authority, revalidation, local execution or single-hop forwarding, and result pipeline; and
5. return an aggregate result containing the existing caller-safe result projection for each piece.

This adds no kernel job authority, no transitive forwarding, no shared ledger, no scheduler, and no transactional claim across pieces. Each piece remains independently durable and idempotent. A retry of a partly completed job replays already completed pieces through their derived per-piece idempotency keys and proceeds with the remaining pieces.

## Job request

The UDS `POST /calls` endpoint accepts either the existing single-call envelope or this disjoint job envelope:

```text
MctResidentJobSubmission {
  job_id
  authority_context
  deadline
  trace_context
  pieces: [
    {
      piece_id
      required_node_id?   # optional exact placement constraint, not authority
      target
      payload_metadata
      payload
      inline_payload_base64?
    }, ...
  ]
}
```

Rules:

- `job_id` and every `piece_id` are non-empty portable identifiers.
- Piece IDs are unique within the job.
- The first slice accepts 2 through 16 pieces and remains under the existing call-frame budget.
- The adapter derives `call_id = <job_id>:<piece_id>`, per-piece protocol/span IDs, and `idempotency_key = mct-mesh-job:<job_id>:<piece_id>`.
- Caller identity and origin still come only from the authenticated local adapter.
- An optional `required_node_id` removes nonmatching candidates before ranking; it cannot make an unauthorized candidate admissible and participates in idempotency.
- Pieces execute in request order. All pieces are attempted unless durability becomes unavailable; the aggregate outcome is `completed` only when every piece completed.
- The job envelope is orchestration, not an atomic transaction. Already completed effects are not rolled back when a later piece denies or fails.

The resident writes a job receipt observation before the first piece and a terminal aggregate observation before returning the response. Piece payload bytes remain absent from observations.

## Physical fixture

Mother A keeps the arithmetic Child from PR #105 for the local piece. Mother B additionally stages and approves the existing `mct-repo-report` Child used by PR #107. B receives the existing directory-read ToyGrant for A as caller and a private Cargo.lock snapshot as its exact `file://` resource.

The job pieces are intentionally different operations:

- local: A's `patina:fleet-n0/work@0.1.0.step`;
- delegated: B's `patina:repo-report/report@0.1.0.inspect`.

The distinct operation identities prevent a local fallback from satisfying the delegated piece.

## Covered versus uncovered without board.post

Do not build `board.post`. The existing directory-read ToyGrant already has a coverage dimension suitable for this pass/fail test: its mandatory time bounds. B executes the delegated repo-report piece while the exact caller/Child/resource/action grant is current. After the recorded expiry, A submits the same B operation as a third piece with a new call ID. B's route authority evaluates the expired grant, records the typed `ToyGrantExpired` refusal, eliminates the local candidate, and returns denial without a runtime start.

This proves covered versus uncovered authority using an already implemented Toy and Child. Building `board.post` would add a new WIT contract, host adapter, signed issuance/revocation surface, persistence facts, and effect semantics that are unnecessary for NIC-63's first mesh proof.

## Evidence contract

The physical result passes only when one report proves all of the following from canonical verified ledgers:

- one job ID with exactly two completed piece results;
- A's local piece has exactly one completed execution on A and none on B;
- B's delegated piece has forwarding evidence on A and exactly one completed execution on B, with none on A;
- the uncovered call has one typed `ToyGrantExpired` refusal on B and zero runtime started/completed/failed facts on both Mothers;
- both independent ledgers verify under their own Mother identity; and
- the target imported the signed closure and did not compile.

Hostnames, LAN addresses, SSH identities, ports, and run roots remain in private configuration and are sanitized from committed evidence.

## Non-goals

- No `board.post` implementation.
- No Child-created delegation or borrowed-handle chain.
- No multi-hop forwarding.
- No shared or replicated ledger.
- No job rollback, cancellation, parallel scheduling, restart-resume, or durable queue.
- No compile on Pi 3 targets.
- No re-image, firmware update, or persistent network-policy change.

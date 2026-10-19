---
type: feat
id: federation-finish
status: active
created: 2026-10-19
sessions:
  origin: null
  work: []
related:
  - docs/DECISIONS.md
  - docs/FINISH.md
  - layer/core/reliability-doctrine.md
  - layer/surface/build/feat/multi-mother-route-forwarding/SPEC.md
exit_criteria:
  - id: independent-policy-revisions
    text: Direct federation works with unequal Mother-local policy revisions; stale local or remote authority still denies.
    checked: true
    verify: python3 scripts/check-finish.py proofs
  - id: six-independent-processes
    text: Six actual resident processes with distinct identities and state cooperate in one Vision; surviving nodes work while a peer is stopped, and the peer restarts without losing identity or revocation.
    checked: true
    verify: cargo test -p mct-daemon --test federation_fleet -- --nocapture --test-threads=1
---

# Bounded federation completion

## Authorization and scope

The owner requested a fresh branch/clone from `fix/ledger-first-call-recovery`,
a completion check, and implementation toward a finished single-Mother and
4–6-Mother runtime. D-52 records that release boundary. This first slice fixes
issuer ownership of policy revisions and provides an isolated six-process
acceptance fixture. It does not implement the entire release checklist.

## Contract for this slice

- Every Mother has independent identity, keys, configuration, ledger, runtime
  state, and execution authority. One explicitly agreed Vision is sufficient.
  Existing single-local-Vision and immutable recorded node/Vision constraints
  remain; this slice does not add membership mutation or multi-Vision hosting.
- Admission remains bilateral and signature checked. Local peer admission is
  checked against the local Mother's policy revision. The remote-signed
  outbound binding, remote publication, and published surface must agree on
  the remote Mother's policy revision. Independent counters need not match.
- Forwarding carries the remote-signed binding revision as an expectation for
  the receiving hop, not the originating caller's local policy revision.
  This expectation grants no authority: the receiver still checks its current
  binding, current policy, grants identity, and Child/Toy authority.
- Expiry, publication freshness, ALPN, Vision, payload constraints, signatures,
  revocation, and single-hop termination remain mandatory. A higher remote
  counter does not refresh local authority, nor does a higher local counter
  refresh remote authority. No wire/schema change is needed for this split.
- The receiver may deny after selection (e.g. concurrent revocation). Neither
  cached advertisements nor a local successful revalidation override that.
- Refresh configured admitted peers immediately at boot, then at the existing
  120-second interval. Poll refresh concurrently with serving and shutdown;
  network waits must never suspend those futures. A single refresh loop avoids
  overlapping refresh passes and is cancelled with the resident lifecycle.
  Its period and serving/refresh/shutdown futures are injectable test seams.
  This corrects the startup delay and serving stall found by the fleet test;
  it does not add transitive discovery or a fleet coordinator.
- Six-node proof uses direct loopback Iroh between real subprocesses, private
  temporary roots, and actual WASM execution. Test setup may use today's local
  configuration/admin paths; that is not proof of the future admin cutover.
  Config-only peer/Child authority remains an explicit release blocker.
- Killing one process must not stop independent local work on the others.
  Restart preserves node identity and surviving canonical ledger bytes; peer
  revocation remains effective. No takeover or replay of unfinished effects.

## Non-goals

No clustering, shared ledger, global revision, quorum, standby, failover,
automatic work migration, transitive routing, cross-Vision sharing, deployment
to live services, new admin-key policy, or whole-Pi-in-WASM dependency. Linux
release support and real multi-host network partitions require separate proof.

## Evidence obligations

Reproduce unequal-revision failure before changing runtime code. Retain stale
local-policy denial and add remote-binding/publication mismatch denial. Run
existing two-Mother denial/single-hop tests, the six-process fixture, full Tier
0 and docs. Report the six-node test's setup shortcuts and unsupported failure
classes; do not equate healthy echo cooperation with a finished product.

Local proof results and remaining limits are recorded in [EVIDENCE.md](EVIDENCE.md).

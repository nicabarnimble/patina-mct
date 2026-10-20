---
type: feat
id: ledger-first-call-recovery
status: active
created: 2026-10-19
sessions:
  origin: null
  work: []
related:
  - docs/DECISIONS.md
  - docs/design/child-start-ledger-fact.md
  - layer/core/reliability-doctrine.md
  - layer/surface/build/feat/resident-call-ingress/SPEC.md
  - layer/surface/build/feat/perf-phase-0/SPEC.md
exit_criteria:
  - id: rollback-refuses-reexecution
    text: A keyed call whose SQLite reservation and result are lost cannot execute again within its dispatch window when its canonical dispatch fact survives.
    checked: true
    verify: cargo test -p mct-daemon --test mother_restart sqlite_rollback -- --nocapture --test-threads=1
  - id: replay-contract
    text: Dispatch replay is strict, caller-scoped, fingerprint-sensitive, bounded by the existing TTL, and release is conditional on the exact recorded attempt.
    checked: true
    verify: cargo test -p mct-daemon call_dispatch -- --test-threads=1
  - id: validation
    text: Workspace tests, Clippy, Tier 0, documentation build, and the dispatch fuzz corpus pass.
    checked: true
    verify: ./scripts/ci-tier0.sh && ./scripts/build-docs.sh
---

# Ledger-first keyed dispatch recovery

## Authorization and boundary

The owner requested a branch implementing the architecture review's approach:
accurate claims, then closure of the known duplicate-execution gap. This branch
implements a narrower corrective slice than proposed D-40. It does not mark
D-40 built or silently settle its seven questions. Merge review accepts this
specific change, not the remaining durable-agent architecture.

## Selected implementation contract

1. Preserve authenticated caller + idempotency-key scope and the existing
   fingerprint (operation, semantic call ID, payload identity). Persist only
   domain-separated BLAKE3 digests of key scope and fingerprint; never raw
   idempotency keys, payloads, credentials, or application state.
2. Before the keyed execution closure can run, the resident single writer
   commits one versioned **dispatch reservation** as a BeforeEffect observation.
   This is permission to enter the ordinary authority pipeline, not authority
   to launch a Child, and not evidence that a Child started. All existing
   authority checks remain mandatory. No authority-order mutex is held while
   awaiting this write.
3. The writer reconstructs the reservation index from the validated ledger
   before serving calls and serializes check-and-append. SQLite alone cannot
   authorize dispatch when a surviving reservation exists. A matching SQLite
   completed reply still replays normally; if it is lost, the surviving ledger
   reservation yields `execution_interrupted`, with no re-execution. A different
   fingerprint under the same live key yields `idempotency_key_reuse_mismatch`.
4. Reservation lifetime is the existing 720 seconds, from Mother acceptance
   time. A reservation beyond that window does not prevent a fresh execution.
   Unkeyed calls keep their existing repeatable behaviour. Identity is not a
   global caller-supplied `call_id` lock. This is not an exactly-once guarantee.
5. Only the existing provably-before-effect `retry_later` result may release a
   reservation. The release is another BeforeEffect fact referring to the
   exact attempt; it precedes SQLite release. A crash before release ack may
   conservatively refuse a call which never executed. There is no optimistic
   inference of non-execution from missing SQLite rows.
6. Write/ack uncertainty fences the writer and suppresses new dispatch. A
   complete surviving reservation without acknowledgement remains canonical.
   Torn final frames use the existing recovery law. Unknown/malformed dispatch
   facts block writer startup rather than being skipped.
7. Keep SQLite NORMAL and the existing schema. Do not fabricate a lost run or
   result payload. Ledger-first reconstruction here restores the retry fence,
   not all runtime run projections. Existing startup settlement still handles
   surviving legacy rows. Pre-upgrade calls with no dispatch facts are not
   retroactively protected against total loss of their SQLite evidence.

## Why not implement all of option D in this slice?

D-40's call-ID-only guard would conflate existing caller-scoped keys and change
unkeyed semantics. Its Child-admission ordering, run reconstruction, Watch
scope, and Toy-order-lock changes deserve separate proof. A dedicated dispatch
reservation closes the keyed retry hole for both local and forwarded calls
without reinterpreting an old route fact as an execution-start fact. Its cost
is one extra sync per new keyed dispatch, and conservative interruption when a
crash follows reservation but precedes launch. No performance improvement is
claimed.

## Performance sequencing

Perf Phase 0 remains a historical measurement of its pinned revision. Its
measurement-only fence does not block this separately authorized correctness
slice. Results measured before this reservation write do not characterize the
new path. A current-revision profile is separate work; no old baseline is
relabeled, and no frozen Allium map is edited.

## Required proof

- Restore pre-call SQLite state after a completed real WASM call; retry must not
  enter routing or execute it again, including after a second restart.
- Kill a real Mother during a call, restore pre-call SQLite, and retry: same rule.
- Preserve normal exact-result replay, caller isolation, mismatch, TTL expiry,
  unkeyed repetition, and before-effect transient release.
- Simulate complete reservation with lost ack, torn reservation, malformed
  complete fact, release before/after ack, duplicate/conflicting facts, and
  stale releases. Preserve failing artifacts; fuzz the strict decoder.
- Fail the dispatch write and prove the execution closure was never entered.

## Remaining design work (not implemented here)

Writer-ordered final Child admission (D-40), unkeyed/Watch dedup policy, full
ledger reconstruction of runtime runs/results, Toy admission ordering,
admin transport/cutover, mandatory hybrid signatures, recovery-key compromise,
portable handles, and task checkpoint/resume semantics remain open. In
particular, an interrupted task is not automatically retried under a new key.

## Evidence

Implementation and local validation are complete; owner review remains open.
See [EVIDENCE.md](EVIDENCE.md) for the reproduced pre-fix failure, 614 passing
workspace tests, two rollback regressions, the extended restart run, fuzz
coverage, dependency audit, and the unavailable external spec-manager check.

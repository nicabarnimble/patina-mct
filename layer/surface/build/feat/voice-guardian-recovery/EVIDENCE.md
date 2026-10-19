# Voice-bound guardian recovery — implementation evidence

Owner authorization: D-54, recorded in `6a494d5`. Work is local on
`feat/finish-mct-federation`. This is partial implementation, not release readiness.

## Slice 1: kernel evidence boundary

`crates/mct-kernel/src/recovery/` defines versioned Voice genesis, Mother owner
bindings, fixed same-Voice 2-of-3 guardian policy, exact replacement intents,
replacement possession proofs and purpose-bound short-lived hybrid approvals.
The pure verifier returns bounded evidence, not an execution capability.

The old admin-key evaluator and its root-revocation denial tests are unchanged.
No live ingress, canonical authority hash, persistence format, or deployment
behavior changes in this slice. Current state/time must be supplied by Mother;
request-carried copies never become the source of current authority by this API.

## Slice 1 validation

- `cargo test --locked -p mct-kernel -- --test-threads=1`: **157 passed**, including
  **25 new recovery tests**, no failures/ignores. Existing ordinary-admin
  root-revocation denials still pass.
- `cargo clippy --locked -p mct-kernel --all-targets -- -D warnings`: passed.
- `bash scripts/ci-tier0.sh`: passed formatting, policy/tooling checks,
  warnings-denied workspace Clippy, workspace tests (**647 passed, 0 failed,
  2 existing ignored**), and RustSec audit.
- `python3 scripts/check-finish.py proofs`: **9 groups / 56 tests passed**,
  including the new recovery group and all eight inherited groups.
- `python3 scripts/test-check-finish.py`: **17 passed**. The runner now accepts
  only the allowlisted kernel/observation/daemon packages, retaining daemon as
  the default. Partial proofs under an open requirement cannot green release.
- `bash scripts/build-docs.sh`: passed.
- `git diff --check`: passed.

Local logs: `/tmp/mct-guardian-evidence/{tier0,proofs,docs}.log`. These are
scratch diagnostics, not portable release receipts.

The optional shared build cache was `/tmp/patina-mct-retire-allium/target`.
Tests require no live daemon or installed Patina service.

## Negative tests that initially failed

The first focused run passed 23/25:

1. Serde internally tagged **unit variants** accepted an unexpected field despite
   `deny_unknown_fields`. The decoder now uses strict empty-struct wire variants
   and maps to the public purpose enum. The rejection test remains.
2. The overflow test chose an instant outside Jiff's constructible timestamp
   range. It now starts from a valid late-year-9999 timestamp whose checked
   addition overflows, proving a typed timing denial rather than fixture panic.

## Slice 2: canonical lifecycle

Implemented in `recovery/lifecycle.rs` and
`mct-observation/src/guardian_recovery.rs`, behind a deliberately unsupported
live-runtime boundary:

- Signed initial enrollment against the actual canonical hybrid root, complete
  Voice genesis, exact three-member policy and all guardian owner/key acceptances.
- Exclusive BeforeEffect authority transitions, exact-state expectations,
  permanently consumed request/challenge IDs, and canonical replay.
- Dual-clock 24-hour delay; restart resets the wait without extending expiry.
  A live challenge cannot be churned; an expired challenge can be replaced without
  another same-tenure delay. Restart invalidates it and restarts the full delay.
- Quorum cancellation, expiry, atomic root replacement and revocation of every
  prior day grant/pairing ticket. Historical facts and retired keys remain.
- Separate current-root approval of the exact canonical authority snapshot.
  Cancellation and expiry do not implicitly resume work.
- New hash domain only when recovery state exists; unchanged legacy hash otherwise.
  Ordinary mutation/import cannot install, overwrite or mutate enrolled authority.
  The daemon refuses enrolled ledgers rather than serving legacy UID/admin paths.

### Results

- `scripts/ci-tier0.sh`: **665 passed, 0 failed, 2 existing ignored**; formatting,
  workspace warnings-denied Clippy, policy checks and RustSec audit passed.
- `scripts/check-finish.py proofs`: **11 groups / 74 tests passed**, including
  **17 new canonical recovery tests** and the daemon unsupported-runtime test.
- Completion runner unit tests: **17 passed**. Inventory validation and diff check
  passed. Documentation build passed.
- New `guardian_recovery` fuzz target: **25,000 runs / 12 seconds, no failure**,
  under local `rustc 1.96.0-nightly (3b1b0ef4d 2026-03-11)`. Four committed public
  fixture seeds cover enrollment, initiation, completion and semantic ledger
  replay. This is a bounded smoke run, not exhaustive coverage or ingress limits.
  CI adds the target to its pinned nightly smoke matrix.

Tests cover lost completion acknowledgements on both sides of durability ack,
poisoning, reopen/deduplication, partial initiation frames and preserved residue,
wall/monotonic jumps, rollback, rebase, challenge reuse/churn, root-only veto denial,
all enrollment acceptances, actual root/ledger/tenure, stale snapshot review,
root/day-grant/pairing invalidation, legacy hash compatibility and strict decoding.

### Failure found by the full gate

The initial focused tests passed, but the first workspace run exposed an inline
recovery projection inflating `ResidentLedgerCommand` to 5,808 bytes, triggering
Clippy's large-enum check and a resident test stack overflow. The projection is
now boxed in `AuthorityStateV1` without changing its wire/hash representation.
A state-size regression assertion and the full workspace rerun pass. No warning
suppression or enlarged test stack was used.

Scratch logs are `/tmp/mct-guardian-evidence/slice2-{tier0,proofs,docs}.log`,
`slice2-tier0-first-failure.log`, and `fuzz-final-smoke.log`. They are diagnostics,
not portable release receipts. The source tests and committed corpus reproduce
these boundaries; regenerate corpus with `MCT_RECOVERY_CORPUS_OUT` and the
`canonical_recovery_corpus_records_replay` test.

## Still required

Authenticated cross-Mother same-Voice admission and current-binding exchange,
roster changes/removal/disablement and delayed activation, Iroh-only admin and
restricted recovery serving, final-admission maintenance fencing, canonical
runtime consumers, real-process quorum/outage fixtures, production parser byte
limits, and Devin/operator host/custody/upgrade drills. Storage APIs are not a
remote enrollment protocol. Full recovery and remote revocation immediacy are
not claimed. `key-recovery` remains **open**.

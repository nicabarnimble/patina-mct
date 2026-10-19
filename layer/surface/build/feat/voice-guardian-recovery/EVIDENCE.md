# Voice-bound guardian recovery — slice 1 evidence

Owner authorization: D-54, recorded in `6a494d5`. Work is local on
`feat/finish-mct-federation`. This is partial implementation, not release readiness.

## Implemented boundary

`crates/mct-kernel/src/recovery/` defines versioned Voice genesis, Mother owner
bindings, fixed same-Voice 2-of-3 guardian policy, exact replacement intents,
replacement possession proofs and purpose-bound short-lived hybrid approvals.
The pure verifier returns bounded evidence, not an execution capability.

The old admin-key evaluator and its root-revocation denial tests are unchanged.
No live ingress, canonical authority hash, persistence format, or deployment
behavior changes in this slice. Current state/time must be supplied by Mother;
request-carried copies never become the source of current authority by this API.

## Local validation

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

## Still required

Canonical bootstrap/OwnerBinding enrollment, signed guardian acceptance and roster
changes, delayed persistent recovery/cancellation/completion, consumed requests,
clock/restart behavior, atomic root/day-grant replacement, Iroh-only admin and
restricted recovery serving, exact-authority-snapshot resumption, real-process
quorum/outage fixtures, parser fuzzing at integrated ingress/persistence, and
Devin/operator host/custody/upgrade drills. No replay-prevention, 24-hour timer,
remote-revocation immediacy, or full recovery guarantee is inferred from a
successful cryptographic quorum check. `key-recovery` remains **open**.

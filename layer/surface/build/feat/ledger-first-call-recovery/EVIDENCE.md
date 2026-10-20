# Ledger-first call recovery — branch evidence

Base: `433087658b35ff83ff20459052a9b2f3150455cb`.
Branch: `fix/ledger-first-call-recovery`.
This is implementation/review evidence, not a release or owner-ratification claim.

## Red before green

Added the real-process regression before changing runtime code:

```text
cargo test -p mct-daemon --test mother_restart \
  sqlite_rollback_after_completed_call_never_reexecutes \
  -- --exact --nocapture --test-threads=1

lost SQLite history must not authorize another execution:
Completed { inline_result: "eyJyZXN1bHRzIjpbM119" }
test result: FAILED. 0 passed; 1 failed
```

The test uses a test-private `VACUUM INTO` snapshot made before submission,
kills the real resident, restores only SQLite while retaining the synced
ledger, and retries. It simulates lost database commits; it is not a physical
power-cut experiment. The sibling SIGKILL test interrupts an active guest
before restoring the snapshot. Both pass with the fence, including retries
after a second restart; no second route selection occurs.

## Final checks

- `./scripts/ci-tier0.sh`: PASS. Formatting, release/vocabulary checks,
  warnings-denied all-target Clippy, serialized workspace tests, and RustSec
  audit. **614 tests passed, 2 existing ignored tests**.
- `./scripts/build-docs.sh`: PASS (mdBook, links, trace verification, rustdoc).
- `cargo test -p mct-daemon call_dispatch -- --test-threads=1`: covered by the
  final workspace run: five library tests and four resident tests.
- `cargo test -p mct-daemon --test mother_restart sqlite_rollback -- --nocapture --test-threads=1`:
  both new regressions pass, also covered by Tier 0.
- `MCT_RESTART_SEED=0x102019 MCT_RESTART_ITERATIONS=6 cargo test -p mct-daemon --test mother_restart mother_survives_sigkill_mid_run_with_active_children -- --exact --nocapture --test-threads=1`:
  PASS, six additional iterations, 41 completed steps before kills and 12
  in-flight steps at kills.
- `cargo +nightly fuzz run call_dispatch --fuzz-dir fuzz <fresh-scratch-corpus> fuzz/corpus/call_dispatch -- -seed=114423508 -runs=25000 -max_total_time=120`:
  PASS, 25,000 runs in 22 seconds; final coverage counters `cov: 2179`,
  `ft: 3646`. Both committed seeds are validated by a unit test. Local nightly
  is 1.96.0-nightly (2026-03); CI retains pinned nightly-2026-04-17.
- `cargo audit --file fuzz/Cargo.lock`: PASS, 563 dependencies, existing audit
  exceptions unchanged.
- `bash scripts/check-comparative-vocabulary.sh` and `git diff --check`: PASS.

The fuzz workspace's previous lockfile lacked the already-declared kernel
libcrux dependency graph. Building the new target refreshed that stale graph;
no runtime manifest dependency or root `Cargo.lock` changed. The refreshed
fuzz graph was audited separately. The new target joins the existing bounded
fuzz CI loop; the other fuzz targets were not rerun locally for this slice.

## Failures retained and resolved

- The pre-fix rollback regression above failed by executing again, as intended.
- The first full suite found two observation-sequence assertions that needed
  the new dispatch fact: keyed echo grows from 10 to 11 entries; the peer
  lifecycle includes a second, explicitly labelled `CallConstructed` dispatch
  observation. Counts/order remain asserted; no timing bound was weakened.
- The initial real-ledger unit fixtures mistakenly placed their first record
  at sequence 1 although the ledger starts at 0. The fixtures now include a
  non-dispatch initial record, matching production's preexisting authority
  history. No production sequence rule changed to satisfy the tests.

Complete local logs are retained in `/tmp/mct-hardening-evidence/`, including
`rollback-before.log`, `workspace-first.log`, `dispatch-unit.log`,
`tier0-final.log`, `fuzz-final.log`, and `restart-six-iterations.log`.

## Not performed / limits

- `patina spec check ledger-first-call-recovery --json` could not run because
  the separately installed `patinaMother` spec-manager was unavailable. No
  production daemon was started to satisfy that optional check.
- No public CI run, release smoke, production install/upgrade, current-path
  performance profile, physical power-cut experiment, or whole-workspace
  line-coverage measurement is claimed. Fuzz counters are measurements for
  this target/run, not a line-coverage percentage or a cross-version ratchet.
- New dispatch facts protect new keyed calls. They do not recover the missing
  history of pre-upgrade calls, establish exactly-once external effects, or
  reconstruct lost run/result payload projections.
- An old binary ignores this new fact namespace. Downgrading and discarding
  call projections therefore does not preserve the new retry guarantee. Do
  not advertise such a downgrade as safe for retained in-flight work.
- Recovery scans canonical history to rebuild the fence; no bounded-startup-
  latency or large-ledger performance claim is made. Fresh dispatch costs one
  additional sync; transient release costs another durable record.

## Next review boundary

Review D-51 and this slice's conservative dispatch semantics first. Final
Child admission, all-call/Watch dedup, run reconstruction, and the larger
admin/durable-task decisions remain separate. Proposed next-task semantics
are in `docs/design/durable-task-boundary.md`; security decision boundaries
are in `docs/SECURITY-BOUNDARIES.md`.

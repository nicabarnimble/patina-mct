# R1 — Restart control-listener readiness ordering

Local fix on `experiment/git-toy-confinement`, after H1 checkpoint `7aafdcc`.
No push, main-branch change, H1 closure, or R2 repair is included.

## Retained failure and timing

Original failure: `mother_survives_sigkill_mid_run_with_active_children`, seed
`0x6d637421`, iteration `1`, `AfterSomeChildrenFinish`, boot `1`.

- Fixture: `/var/folders/6h/329275913d1d3k1lfvvvryp40000gn/T/.tmpbSbHKw`.
- Full failing run: `/tmp/mct-h1-promotion-evidence/workspace-tests-final.log`.
- `mother-0.log` records the original control listener serving successfully.
- `mother-1.log` records prior-tenure call settlement, endpoint creation, three
  loaded children, then `resident control socket did not bind before readiness`.
  It never records the new control listener serving.

The retained log's filesystem timestamps and canonical ledger distinguish this
failure from an exhausted two-second cold-start wait. UTC times on 2026-10-25:

| Evidence | Time |
| --- | --- |
| `mother-1.log` birth time (created before process spawn by the test) | 01:37:16.492855 |
| New authority tenure, ledger sequence 163 | 01:37:16.552160 |
| Startup drift evaluated, authority ready, sequence 166 | 01:37:16.596430 |
| Prior-tenure call disposition appended, sequence 167 | 01:37:16.644306 |
| Local-call endpoint admission appended, sequence 168 | 01:37:16.724544 |
| `mother-1.log` final mtime (contains terminal error) | 01:37:16.728907 |

The entire log-creation-to-error interval is about **236 ms**, not two seconds.
The endpoint-admission fact is intentionally recorded **before** binding; it is
not a receipt that the listener exists. The retained socket pathname is absent.
These artifacts were inspected read-only and were not replayed or repaired.

## Cause

`resident/serving.rs` spawned the control task and inferred readiness using:

1. Poll `path.exists()` up to 200 times, breaking on existence.
2. Independently check `path.exists()` again, failing if absent.
3. Publish supervised readiness / send the caller's ready ticket.

Meanwhile, `control.rs` removed any old socket, durably admitted the local
endpoint, initialized the control snapshot, then bound and secured the new UDS
listener. SIGKILL leaves the predecessor's socket pathname behind.

Two legal interleavings were therefore incorrect:

- The first existence check sees the stale socket; the control task unlinks it
  before the second check. Startup fails immediately with the reported message,
  without exhausting the wait loop. The retained timing and pre-bind progress
  fit this interleaving.
- Both checks see the stale socket before unlink. The parent advances readiness
  before the new listener is bound.

Additionally, control-task startup errors were not joined at this boundary;
real bind/snapshot/ledger failures could be hidden by the generic pathname
error. HTTP had no listener startup acknowledgment either.

## Fix

- `spawn_resident_control_task` returns a task handle and a one-shot listener
  acknowledgment receiver.
- UDS sends that acknowledgment only after durable endpoint admission, initial
  snapshot validation, actual bind, mode `0600`, owner verification, and handler
  initialization. HTTP acknowledges its bind and initial snapshot validation.
- `wait_for_resident_control_bound` waits on that receipt and the task itself,
  preferring a known task failure over a queued receipt. Early errors, panic,
  or clean exit without readiness fail startup with their real cause.
- The parent publishes readiness only after this await. On control startup
  failure it aborts/joins startup background tasks, closes the endpoint, and
  closes the ledger writer instead of leaving detached startup effects.

No sleeps, retry counts, process-test deadlines, or authority/durability
assertions were relaxed. The pathname polling loop was removed, not lengthened.
This is synchronization with the task that owns the new listener, not a new
assumption about how quickly a cold restart must finish.

## Deterministic regression

`crates/mct-daemon/src/daemon/resident/serving/tests/readiness.rs` adds four tests:

- Leave a real stale Unix socket, hold the test control task before unlink, and
  explicitly poll the production readiness waiter: it must remain pending.
  Unlink and hold before running real control startup: it must still be pending.
  Release startup, await the actual listener acknowledgment, then make an
  immediate `/status` request with no socket/status retry loop. Verify mode
  `0600`, the preceding ledger fact, and normal shutdown cleanup.
- Real UDS startup failures: blocked bind path, unavailable snapshot store, and
  closed ledger writer. None may produce a successful readiness receipt.
- Real HTTP `AddrInUse` bind failure propagates.
- Control-task clean exit or panic before acknowledgment fails readiness.

The stale-socket regression was first run with the **original pathname wait
extracted unchanged** into the waiter (the channel wiring was present but the
waiter ignored it). It failed in 0.01 s with:

```text
readiness preceded the new listener bind
1 failed; 0 passed
```

Replacing only that algorithm with the listener/task handshake made all four
tests pass. Barriers and explicit future polling, not sleeps or scheduler luck,
control the important interleavings. The five-second test timeouts only bound
hangs; they are not a production startup policy.

## Validation

Environment: macOS arm64, Rust 1.96.0, locked/offline dependency graph,
`CARGO_BUILD_JOBS=8`, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_NET_OFFLINE=true`.

| Check | Result |
| --- | --- |
| Deterministic readiness tests | 4 passed; original-algorithm failure retained |
| SIGKILL restart test, seed `0x6d637421`, 20 iterations | 20/20 passed |
| SIGKILL restart test, seed `0x6d637422`, 10 iterations | 10/10 passed |
| Full workspace tests | 755 passed, 0 failed, 2 pre-existing ignored |
| Workspace/all-target Clippy with `-D warnings` | Passed |
| `cargo fmt --all --check`, `git diff --check` | Passed |

All process runs retained fixtures with `MCT_RESTART_KEEP=1`; the full workspace
also used `MCT_FLEET_KEEP=1`, `--no-fail-fast`, and `--test-threads=1`. It includes
the default three restart iterations and the fleet restart test. Repeated runs
changed only the documented seed/iteration environment variables, not assertions
or timeouts.

```sh
cargo test --locked --offline -p mct-daemon --bin mct-daemon \
  resident::serving::tests::readiness -- --nocapture --test-threads=1
MCT_RESTART_KEEP=1 MCT_RESTART_ITERATIONS=20 MCT_RESTART_SEED=0x6d637421 \
  cargo test --locked --offline -p mct-daemon --test mother_restart \
  mother_survives_sigkill_mid_run_with_active_children -- --exact --nocapture --test-threads=1
MCT_RESTART_KEEP=1 MCT_RESTART_ITERATIONS=10 MCT_RESTART_SEED=0x6d637422 \
  cargo test --locked --offline -p mct-daemon --test mother_restart \
  mother_survives_sigkill_mid_run_with_active_children -- --exact --nocapture --test-threads=1
MCT_RESTART_KEEP=1 MCT_FLEET_KEEP=1 \
  cargo test --workspace --locked --offline --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all --check && git diff --check
```

Logs and retained-fixture paths: `/tmp/mct-r1-readiness-evidence/`. SHA-256
receipts below are local evidence, not portable release artifacts:

```text
3efc2ac69edd73c05cdf967b5d214dc6ea13ce6e4f783d375173886735ab322d  regression-before.log
4b95905b45c48625d1a6d46306f1a255eea3fa106aa09bc3235712fef9b1cba3  regression-after.log
e7005b25a1e62ac5c1afaf11b9dcbdd482d79d650f7c874abe773f6b8cf724f0  restart-20.log
e14227603a0df65b77594fff73e239be2e37255a725ac6c9129bf7a0950171cd  restart-10-alternate-seed.log
4cd131f361f055175b5526fbc00d118a2d268be144230764fed1739cf708ca89  workspace-tests.log
7e8397b3c3d5ef73830e57f61ffa6f2cfd565f338d3d371aefe79dbf3b020764  clippy.log
```

Original retained fixture receipts:

```text
0215e31da29bb3c26c50bba91daf755a253dd5857e53d4f495405e1464776b2c  mother-0.log
fd7d0b4c6b26aba5287f6c06e56ab11288fb8960865238aef701e85043ed73e4  mother-1.log
4a64fb6ef0ac672a78430178a9529aee2ebe9a9d591412b8ea8c13bfcae11ade  root/observations.jsonl
```

The demonstrated stale-socket readiness race is fixed locally with regression
coverage. Review/merge and formal completion-inventory closure are separate;
H1 and R2 are not closed by this work.

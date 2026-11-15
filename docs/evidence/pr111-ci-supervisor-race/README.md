# PR #111 Ubuntu CI: `supervisor_conflicts_refuse_before_launchd_or_endpoint_effects`

## Failure

On CI run 42317368679 (Ubuntu, head `27fbccb`), the test panicked at the
concurrent-install leg: `unwrap_err()` was called on
`Ok(LifecycleReport { action: "install", outcome: "completed", .. })`.

## Root cause

The failure is scheduling-dependent, not OS-dependent. The ledger writer lock is
std `File::try_lock`, which is `flock` on both Linux and macOS: an exclusive lock
on the open file description. So a second open in the same process conflicts on
both systems.

The test held the writer, spawned the losing install, slept a fixed 100 ms, and
then released the writer. Before its first `open`, the loser validates the
executable and builds a discovery summary. When that took longer than 100 ms, the
loser's first `open` came after the release, so it found no contention and
installed successfully.

## Reproduction (macOS, canonical TMPDIR, umask 077)

A temporary 150 ms delay before the loser's first `open` stood in for a slow
runner. This edit was not committed.

```
panicked at crates/mct-daemon/src/daemon/supervisor_lifecycle.rs:3474:48:
called `Result::unwrap_err()` on an `Ok` value: LifecycleReport { action: "install", outcome: "completed", ..
test result: FAILED. 0 passed; 1 failed
```

## Fix

`SupervisorAdapter::writer_contention_observed` is a no-op in production. Install
calls it at the moment it finds the bootstrap writer held. The test releases the
writer only after the fake adapter has seen that call. Every assertion is
unchanged. With the same 150 ms delay re-applied, the test passes:
`test result: ok. 1 passed`.

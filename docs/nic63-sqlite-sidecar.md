# NIC-63: disappearing SQLite sidecar preflight

The first final-head CI test run failed when the scheduler test opened its runtime
state store and the optional `-wal` metadata guard rejected a file. A same-head
retry passed, but that was not a diagnosis or a fix. The original error did not
include the failed metadata predicate, so its precise cause remains unproven.

## Reproduced defect and correction

A Linux/aarch64 filesystem probe—not a SQLite workload—reproduced the old guard's
failure while another thread created and unlinked a private sidecar pathname.
The rejected snapshot was a regular file, owned by the current UID, mode `0600`,
with **zero links**. Pathname lookup and unlink can overlap; the snapshot does not
establish that a current file at that name is unsafe.

Only optional SQLite sidecars now receive a bounded recheck:

- At most three no-follow metadata lookups.
- Only an otherwise-private, zero-link regular-file snapshot is rechecked.
- At most two requested 1 ms backoffs give concurrent cleanup a scheduling
  opportunity. This is not a wall-clock bound or a recovery guarantee.
- Success requires a fresh absent pathname or a strictly valid single-link file.
- Unsafe type, owner, permissions, multiple links, and other I/O errors are not
  retried or treated as absence.
- Continued zero-link observations return `WouldBlock`: transient unavailability,
  **not** permission to use the file and not a definitive unsafe-inode verdict.

Required database identity checks remain strict. Preflight still uses metadata
lookups, never an extra SQLite file descriptor whose close could release
process-associated POSIX locks. No production umask, chmod repair, SQLite storage
policy, or general error-retry policy changed. Rejection diagnostics now include
observed mode, UID, and link count.

## Evidence and limits

On the designated Linux builder:

- Three deterministic tests failed with the old behavior before the correction.
  They cover disappearing/replaced snapshots, unsafe replacements and I/O errors,
  bounded exhaustion, and the unchanged strict database guard.
- Seven final private-filesystem tests passed, including actual public-path churn,
  successful checks after the writer joins, and refusal of an unsafe replacement
  without repairing its permissions.
- Six local-storage tests passed, including preserved POSIX locks and an actual
  SQLite test: two threads each perform 64 runtime-store open/summary/close
  cycles, then verify a previously stored value survives.
- A private 200,000-cycle filesystem stress probe recorded 61,094 successful
  validations, two bounded instability refusals, and successful validation after
  churn stopped. It does **not** prove uninterrupted availability during churn.

Earlier stress runs required every check during continuous churn to succeed.
That overstrong oracle failed with immediate rechecks, yields, and backoffs.
Those failures are retained, not relabeled as passes. The budget was not increased
to force that oracle green. The corrected stress contract permits only successful
validation or explicitly classified instability during churn, then requires
success after quiescence. The deterministic tests remain the regression oracle;
no particular race frequency is required from CI.

Independent read-only source review returned **PASS+NOTES**, including the
synchronous backoff tradeoff and the original CI predicate's uncertainty. The
reviewer did not execute the tests. Temporary stress-probe source and raw logs
remain private; the probe is not part of the shipped test suite. Final combined
CI and physical evidence must be read with their own source revisions, not
inferred from these targeted checks.

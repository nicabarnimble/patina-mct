# Multi-Mother mesh job — evidence

NIC-63 passed on the physical Pi fleet on 2026-11-06. The complete narrative,
artifact provenance, exact job/call IDs, independent ledger heads, failure
history, and sanitized receipts are recorded in
[`docs/nic63-mesh-job.md`](../../../../../docs/nic63-mesh-job.md).

The accepted run proves:

- one authenticated two-piece job received by Mother A;
- one local arithmetic execution on A;
- one Iroh-forwarded repo-report execution on Mother B;
- exact caller/receiver call correlation and independent verified ledgers;
- one later B call refused by `ToyGrantExpired` / `ExpiredGrant` with zero
  runtime facts; and
- one builder-produced signed closure imported on the Pi 3 without compilation.

`board.post` was intentionally not implemented. The reused PR #107
directory-read ToyGrant supplied the covered versus uncovered boundary through
its mandatory expiry.

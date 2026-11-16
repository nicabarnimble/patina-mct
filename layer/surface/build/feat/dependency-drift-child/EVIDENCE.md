# Dependency drift Child — evidence

NIC-105 passed on the physical Pi fleet on 2026-11-06. The complete narrative,
artifact provenance, job/call IDs, drift summary, independent ledger heads,
build failure history, blast-radius review and sanitized receipts are recorded
in [`docs/nic105-dependency-drift.md`](../../../../../docs/nic105-dependency-drift.md).

The accepted run proves:

- the same canonical `mct-repo-report` component executed once on nicamaster and
  once on nicanode0 against distinct immutable `Cargo.lock` snapshots;
- the client computed `mct-dependency-drift/v1` with 52 real differences: 48
  version mismatches, two A-only packages and two B-only packages;
- exact placement remained subordinate to each Mother's independent artifact,
  assignment and directory-read ToyGrant checks;
- a fresh post-expiry B probe recorded `ToyGrantExpired` / `ExpiredGrant` and
  zero runtime facts or Child starts;
- both independent ledgers verified with preserved prefixes; and
- one builder-produced signed closure was imported on nicanode0 without Cargo
  or Nix compilation there.

No new Toy was added. Legacy WAT and sample Children remain test-only guidance.

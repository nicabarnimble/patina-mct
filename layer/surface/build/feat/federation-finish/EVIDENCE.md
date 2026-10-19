# Federation finish — implementation evidence

Base: `ba685af` (`fix/ledger-first-call-recovery`). Isolated clone/branch:
`patina-mct-finish`, `feat/finish-mct-federation`. Local work only.

## Reproduced failures

1. Before changing routing, the unequal-revision candidate test failed:
   `local=1, remote=7: Some(PolicyRevisionStale)`; it was eliminated despite
   valid independent local and remote signatures/publication.
2. The initial six-process fixture learned no publications within 60 seconds.
   Inspection found the first periodic refresh tick explicitly discarded,
   imposing a 120-second startup delay. The timer branch also awaited network
   refresh while no longer polling inbound serving or shutdown.
3. After issuer-scoped candidate checks and concurrent startup refresh, the
   actual `mother-0` (revision 1) to `mother-1` (revision 7) call still denied.
   The forwarded call retained revision 1 in its receiver expectation. Setting
   that expectation from the receiver-issued signed binding fixes the hop;
   receiver current-authority checks are unchanged.

Retained local logs: `/tmp/mct-finish-evidence/pre-fix-revisions.log`,
`pre-fix-forwarded-revision.log` (startup discovery failure), and
`pre-fix-forwarded-context.log`. `MCT_FLEET_KEEP=1` preserves process logs,
ledgers and state roots named in these logs. These paths are local reproduction
artifacts, not portable release evidence.

## Six-process acceptance test

`cargo test -p mct-daemon --test federation_fleet -- --nocapture --test-threads=1`

Initial post-fix run passed in 12.55 seconds (`fleet-first-pass.log`). Six real
`mct-daemon serve` processes use six distinct endpoint keys, node IDs, roots,
ledgers and databases, one Vision, and revisions `[1, 7, 2, 11, 3, 5]`.

- 30 direct remote routes and six local routes perform actual WASM work.
- Each node has a distinct operation, so another node cannot accidentally
  satisfy the remote request locally. Results and executing node are checked.
- Publications are learned through real signed Iroh hello exchanges, not
  fabricated SQLite rows or capability fixtures.
- Receiver-side revocation denies despite a caller's fresh cached offer.
- Two processes are SIGKILLed; the five, then four remaining processes perform
  local work and remote work with surviving peers. One survivor is restarted
  while a configured peer is down; boot refresh does not prevent inbound work.
- Both stopped processes restart with unchanged node and endpoint identity and
  preserved pre-restart ledger bytes. The revocation survives; an admitted
  peer still executes on the restarted node.
- After stopping all processes, all six hash-chained ledgers verify under
  their own node identity and contain correlated forwarded/executed evidence.

Fixture limits: same host and owner UID; loopback networking; full mesh only;
uses today's owner-only UDS; sets revision counters through an explicit offline
config edit after virgin bootstrap. No live service is accessed. No claim of
admin-key cutover, canonical reconstruction of peer/Child config, multi-host
partitions, caller-timeout outcomes, lost remote replies, rolling upgrades,
real artifact approval, Linux release support, or takeover.

## Validation

- `./scripts/ci-tier0.sh`: **622 passed, 0 failed, 2 existing ignored**;
  formatting, Clippy (`-D warnings`), release/vocabulary checks and RustSec audit
  passed. Full output: `/tmp/mct-finish-evidence/tier0.log`.
- `python3 scripts/check-finish.py proofs`: all eight proof groups passed
  (**31 tests**): independent revisions, stale local admission, fleet,
  existing 11 two-Mother denial/forwarding tests, refresh lifecycle, inherited
  dispatch parser/writer and both SQLite rollback regressions.
- `python3 scripts/test-check-finish.py`: **14 passed**, including zero-test
  rejection, missing requirements, unsupported completion claims, ignored
  tests, and cleanup of the check's private subprocess group on timeout or
  interruption. The updated runner tests were re-run after the full Tier 0.
- `python3 scripts/check-finish.py`: implemented proofs pass; **exit 1 is
  intentional**, reporting **eight unfinished obligations**. Retained log:
  `/tmp/mct-finish-evidence/completion-check.log`.
- `./scripts/build-docs.sh`: passed; `/tmp/mct-finish-evidence/docs.log`.
- `git diff --check`: passed. No lockfile or frozen Allium changes. This slice
  changes no fact parser/wire format/physical write path; fuzzing was not rerun.
- `patina spec check federation-finish --json` could not run: external
  spec-manager unavailable at `http://localhost:50051`. No live daemon was
  started to satisfy it; local tests/checks do not depend on that service.

`docs/finish-checks.json` keeps the remaining release obligations open. This
first slice is implementation evidence, not owner release acceptance, a
completed administration cutover, or a claim that MCT is finished.

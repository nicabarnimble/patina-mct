# Handoff: finish MCT and bounded federation

## Repository and publication

- Repository: `git@github.com:nicabarnimble/patina-mct.git`.
- Merged baseline: `main` at `ebc0d2d` (PR #82), following PR #81 at `f315175`.
- Canonical local checkout: `/Users/nicabar/Projects/Patina/patina-mct`.
- `feat/finish-mct-federation` is merged and retired. Checkout consolidation is
  complete; the old clones and preservation bundle are under
  `/Users/nicabar/Projects/Patina/.patina-mct-archive/2026-10-19/`.
- On 2026-10-19 the owner separately authorized publishing, consolidation and
  merging the tested groundwork. Earlier draft-only restrictions were superseded
  for that work, not for release or deployment.
- Current work order: [TODO-security-and-finish.md](TODO-security-and-finish.md).
  Begin with bounded H1 Git/WASI regressions and remediation, then peer ingress
  and resource boundaries. Do not reopen settled design or merge old experiments.

The earlier handoff is preserved in Git at `665985c`. Its unratified recovery,
issuance and signature recommendations are superseded by D-54 through D-57.

## Current contract

Finish a dependable single-Mother runtime and direct cooperation of 4–6
independent Mothers in one explicitly agreed Vision. Each Mother owns its
identity, state, authority and ledger. Unavailable peers must not stop unrelated
local work. Keep single-hop forwarding; no consensus, shared ledger, execution
takeover, cluster scheduler or cross-Vision federation.

D-54/D-55 approve explicitly enrolled, same-Voice 2-of-3 guardian recovery,
protected roster changes and released control after delayed disablement.
D-56 approves explicit issuance ceilings, separate from execution scopes.
D-57 approves mandatory hybrid administration and a fresh signed cutover,
without UID/offline alternatives or automatic promotion of unsigned histories.

Allium remains frozen historical evidence, not current authority or a CI check.
Continue approved implementation without seeking permission at commit boundaries.

## Implemented portions and limits

- Ledger-first dispatch fencing survives lost SQLite call history, within the
  existing 720-second caller/key window. It is not exactly-once execution or
  durable task continuation; unkeyed calls remain repeatable.
- Six real resident processes exercise independent policy revisions, all 30
  direct peer routes, local work, revocation, peer loss and restart. This is
  same-host evidence using the legacy admin fixture, not multi-host validation
  or the final authenticated administration path.
- Kernel and canonical writer/replay paths implement hybrid guardian evidence,
  enrollment, recovery, cancellation, completion, resumption and protected
  policy changes, including generation/nonce consumption and restart clocks.
- D-56 issuance enforces exact signed canonical intent, explicit ceilings,
  historical grant-ID exclusion, expiration and atomic receipts.
- D-57 genesis commits root and birth provenance in the first physical frame.
  Existing empty ledgers cannot acquire genesis. Complete surviving frames are
  committed despite lost acknowledgement; partial birth preserves residue and
  denies automatic rebootstrap. Legacy hashes remain unchanged when absent.
- Genesis setup construction is test-only. Enrolled/issued/genesis authority
  profiles deliberately refuse legacy daemon startup until authenticated
  runtime integration and canonical consumers are ready.

See the existing specifications and evidence under
`layer/surface/build/feat/{ledger-first-call-recovery,federation-finish,voice-guardian-recovery,admin-issuance-ceilings,strict-admin-cutover}/`.

## Remaining work

Read `docs/FINISH.md`, `docs/finish-checks.json`, `docs/DECISIONS.md`,
`docs/SECURITY-BOUNDARIES.md`, and `layer/core/reliability-doctrine.md`.

Eight completion obligations remain open: `administration`,
`canonical-authority`, `final-admission`, `key-recovery`, `fleet-operations`,
`distributed-failures`, `useful-workflow`, and `release-evidence`.

- Finish authenticated Iroh administration/recovery, trusted setup production,
  complete canonical Child/peer records and consumers, and final admission
  ordering. Remove old bypasses in the same cutover. Sparse historical facts
  cannot reconstruct authority that was never recorded.
- Recipient-bound Child handles and trusted supervisor/setup lifecycle remain
  unresolved owner decisions. The proposals in
  `docs/design/finish-owner-gates.md` are not approvals.
- The intermittent `resident control socket did not bind before readiness`
  failure in restart/fleet validation remains unexplained. Successful reruns
  are not a fix. Retain the investigation described in strict-cutover evidence.
- Fleet route revalidation also intermittently denied `mother-0 -> 2` during
  PR #82 validation. Passing reruns did not establish a fix. Retain R1/R2 as
  separate investigations in the work index.
- Multi-host macOS partition/upgrade validation requires approved disposable
  hosts, actions and evidence returned through Devin. No assumed remote access,
  production deployment, PiD/Unraid changes or unsupported Linux release claims.

## Validation and continuation

At `a1cb795`, recorded evidence includes **737 workspace tests passed, 2 existing
ignored**, **17 completion proof groups / 146 tests**, **17 runner self-tests**,
Clippy, audit, docs, and 25,000 strict-genesis fuzz runs. Actual older readers
rejected the new recovery-policy, issuance and genesis facts. Those results
prove implemented portions, not release readiness.

```sh
python3 scripts/check-finish.py status    # intentionally nonzero: eight obligations open
python3 scripts/check-finish.py proofs    # implemented proofs only
bash scripts/ci-tier0.sh                 # ordinary development checks
bash scripts/build-docs.sh
python3 scripts/check-finish.py release  # full acceptance, still blocked
```

An optional existing build cache is
`CARGO_TARGET_DIR=/tmp/patina-mct-retire-allium/target`. Temporary diagnostic logs
are under `/tmp/mct-{finish,guardian,issuance,genesis}-evidence/`; durable evidence
summaries are checked into the feature directories above.

PRs #81/#82 are already merged; do not reopen their publication gates. New work
must keep its own scope, validation and remaining blockers explicit. Do not waive
requirements or mark unfinished work implemented to obtain green. Follow D-44:
merge updates from `main`, never rebase or force-push published history. Local
implementation does not authorize pushing, merging, release, deployment or changes
to production services.

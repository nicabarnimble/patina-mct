---
type: feat
id: strict-admin-cutover
status: active
created: 2026-10-19
related:
  - docs/DECISIONS.md
  - docs/design/finish-owner-gates.md
  - layer/core/reliability-doctrine.md
exit_criteria:
  - id: passkey-day-key-boundaries
    text: D-58 independently pinned passkey genesis, bounded Ed25519 leases, scoped BeforeEffect commands and persistent targeted revocation reject legacy promotion.
    checked: true
    verify: cargo test -p mct-observation passkey -- --test-threads=1
  - id: genesis-boundary
    text: Given separate trusted setup, genesis commits atomically, replays strict origin, rejects legacy promotion and survives complete-frame acknowledgement loss.
    checked: true
    verify: cargo test -p mct-observation admin_genesis -- --test-threads=1
  - id: legacy-runtime-denial
    text: Existing runtime refuses strict genesis without rewriting canonical history.
    checked: true
    verify: cargo test -p mct-daemon --lib strict_admin_genesis_cannot_start_through_legacy_runtime
  - id: approvals-only-browser-ceremony
    text: An authenticated loopback queue delivers only CLI-enqueued root intents to the default Chromium browser with 1Password, returns private assertions without file selections or Sign clicks, and preserves Mother verification and restart revocation.
    checked: true
    verify: node --test scripts/passkey-approval.e2e.test.mjs
  - id: authenticated-cutover
    text: Iroh administrative ingress and canonical consumers replace all UID/offline authority paths, with approved trusted setup and lifecycle.
    checked: false
---

# Passkey-rooted administrative origin and cutover (D-58)

Current administration uses registered WebAuthn P-256 roots and <=24-hour
Ed25519 day keys. The owner superseded the guardian/hybrid ceremony on 2026-10-28.
Canonical issuance, scoped BeforeEffect command admission and targeted revocation
are implemented in `passkey_admin.rs`; `passkey-admin` provides bounded signed
CLI input, explicit pinned bootstrap, issue/revoke and authenticated status.
Day keys cannot delegate; KeyManage allows targeted revocation, not lease renewal.

Automated tests cover WebAuthn RP/origin/challenge/UP/UV binding, exact target and
snapshot, scope, expiry, nonces, restart, lost acknowledgement and legacy denial.
The admin-only resident Iroh mode now uses the existing bounded ledger actor
and shared signed issuance/revocation/status adapters. Real-process tests reject
bad signatures, wrong targets/scopes and replay, and preserve revocation across
restart. Other command adapters, canonical runtime projections, remote snapshot
acquisition and removal of their legacy callers remain open. The full legacy
runtime still refuses passkey ledgers. Doug reported the original manual browser
flow passed on `ab193bb`; the new queued flow still needs a live 1Password rerun.

The optional loopback approval broker is an assertion transport, not an authority
owner. Only its private Unix socket may enqueue operations. The authenticated
foreground page claims work before the CLI prepares a fresh intent; it renders
that intent and calls WebAuthn automatically. The CLI consumes the matching private
assertion once and submits it through existing bootstrap/Iroh paths. Existing
expiry, nonvirgin storage and uncertain-submission refusals remain unchanged.
See `docs/design/passkey-approval-queue.md` and `docs/passkey-approval-RUN.md`.
The Chrome CDP test covers the complete ceremony and both fresh post-revocation
refusals. Virtual authentication is not evidence of actual 1Password UI behavior.

## Archived hybrid slice

The text below and its tests are historical compatibility material. See
`docs/archive/2026-10-28-guardian-hybrid-admin-plan.md`; do not make this ceremony
mandatory for D-58.

### Strict administrative origin and cutover (former D-57)

## First bounded implementation

Add a versioned `admin_genesis` canonical fact as the **first physical ledger
entry**, combining the initial root, strict-profile marker and birth authority
identity in one BeforeEffect frame. Do not bootstrap by upgrading an unsigned
epoch, importing configuration, reinterpreting historical self-signatures, or
calling an empty current grant map virgin. Existing history remains inspectable
with its original hashes and semantics, not eligible for implicit activation.

The request signs an exact Mother, ledger, fresh birth epoch, bootstrap ID,
verified-inventory digest and complete fixed-class recovery root. Use a distinct
transcript domain and mandatory existing hybrid verification. Genesis consumes
root nonce zero. A root grant cannot be in the generated `recovered:` namespace.

The storage writer requires **separate trusted setup input**, not a field in the
request: pinned hybrid root plus explicit virgin startup evidence and target
identity. This input is not deserializable from a network request and is not an
execution capability. Its constructor is test-only until the trusted producer is
implemented; no production code can currently manufacture this setup input. The trusted adapter must supply real disk inventory and
operator-installed pin; that adapter, its artifact publication/race fencing and
supervisor lifecycle remain unimplemented and gate-5-dependent. Neither a signed
genesis nor replay alone proves that external setup was trustworthy.

Under the exclusive writer, only an atomically newly created, empty, unpoisoned
ledger with no prior tenure/history can acquire genesis. Reopening an existing
zero-byte file does not restore birth eligibility. The existing inferred-virgin helper is
not accepted as trusted setup. Exact retries return the committed genesis frame,
not fresh authority, even after tenure advancement or guardian root replacement.
Conflicts never replace it. The birth epoch is consumed in canonical replay;
subsequent actual writer epochs use normal preserved-history establishment.
Genesis is birth authority, not an invented writer-tenure receipt. The first
post-birth writer tenure is a real subsequent epoch fact; clients must use its
current identity rather than treating the historical birth receipt as current.

A complete surviving frame is committed even if acknowledgement was lost.
Partial genesis is preserved as residue; it is **not** automatically virgin or
permission to overwrite/rebootstrap. Missing/nonvirgin setup requires a future
explicitly authorized operational procedure, not an invented fallback here.

Store immutable genesis provenance in authority state, selecting a new complete
hash domain only for the strict profile. Current root authority still comes from
the current canonical grant map: the birth pin does not veto guardian replacement
or remain an active grant. Ordinary legacy mutations/imports cannot introduce,
clear, or mutate strict-profile authority. Only already-authenticated exclusive
issuance and guardian transitions are initially admitted by this storage slice.

Daemon startup refuses this profile until authenticated ingress, all canonical
consumers and final-admission fencing are ready. Existing prototype runtime is
not relabelled as cut over. No new live listener, UID fallback, migration tool,
supervisor policy or Child-handle semantics is introduced by this first slice.

## Evidence and continuation

Tests must cover both signature halves, wrong pin/target/inventory, malformed
root, nonce/epoch replay, nonvirgin and legacy prefixes, exact and conflicting
retries, partial/complete append faults, post-recovery provenance, old-reader
rejection, fuzz parsing and a valid strict ledger refused by old runtime.
Subsequent implementation continues under D-57 without reauthorization, except
for the genuinely pending recipient-bound handle and trusted lifecycle gates.
Administration and the release gate stay open; no push until complete and green.

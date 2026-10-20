---
type: feat
id: admin-issuance-ceilings
status: active
created: 2026-10-19
sessions:
  origin: null
  work: []
related:
  - docs/DECISIONS.md
  - docs/design/finish-owner-gates.md
  - layer/core/reliability-doctrine.md
exit_criteria:
  - id: bounded-policy
    text: Issuance separately checks current issuer, explicit ceiling, child scopes/ceiling, expiry and root restrictions without unioning authority.
    checked: true
    verify: cargo test -p mct-kernel admin_issuance -- --test-threads=1
  - id: canonical-issuance
    text: Exclusive signed issuance replays ceilings/provenance, denies stale or bypassed authority, and survives crash/ack loss without duplicate grants.
    checked: true
    verify: cargo test -p mct-observation admin_issuance_tests -- --test-threads=1
  - id: authenticated-runtime
    text: Approved signature cutover and Iroh ingress enforce issuance and current authority without legacy bypass.
    checked: false
---

# Explicit admin issuance ceilings (D-56)

## Boundary

D-56 ratifies owner gate 2. Implement kernel policy and an exclusive canonical
issuance transition, not the still-unratified global signature/legacy cutover.
Mother remains the grantor; an administrative grant authorizes its holder to
request bounded issuance, not to mint a transferable execution capability.

- Add an optional versioned issuance ceiling to `AdminKeyGrantV1`. Missing means
  **no issuance authority** for a day key, never unlimited authority. Omission
  preserves historical serialization and hashes. An explicit empty ceiling is
  revoke-only key-manage. Ceilings are strictly sorted, unique, node-wide scope
  lists and cannot include startup-gate. Only day keys with key-manage may carry
  them; the root uses its existing fixed class authority, not a ceiling field.
- Every newly issued day key with key-manage must have an explicit ceiling,
  including an explicit empty one when no issuance is intended. Non-managers
  cannot hide a ceiling for later use. Old missing-ceiling records remain
  inspectable and do not gain issuance power.
- Select one exact current canonical issuer grant, never union grants sharing a
  public key. It must be active, shape-valid and in-window with key-manage. The
  recovery root may issue any supported non-startup day scope. A day issuer's
  ceiling bounds both recipient execution scopes and any delegated ceiling;
  those are independent from the issuer's own execution scopes. Recipient expiry
  is mandatory, future, and no later than a day issuer's expiry. D-56 does not
  invent a universal 24-hour lifetime for the day-to-day grant class.
- Issuance cannot create/rewrite a root, re-put a grant ID, reuse a current/retired
  root credential as a day key, or authorize weak classical keys. Existing root
  route restrictions and ordinary root-revocation denials remain intact. New day
  grant IDs cannot occupy the writer's reserved `recovered:` root namespace.
  Caller-created IDs are bounded; inherited canonical issuer/ledger IDs are
  matched exactly, including generated roots with maximal-length binding IDs.

## Canonical command and compatibility

New `AdminKeyIssue` facts bind an issuance ID, exact prior Mother/ledger/epoch,
complete authority reference/hash, exact issuer grant ID, recipient grant and
existing hybrid signature nonce. This new command always authenticates its
issuer using the existing hybrid primitive; that does not decide how other
legacy commands or histories will be cut over. The signature has a distinct
command-domain transcript. No recipient self-signing or unsigned new issuance.
The nonce is a consumed per-issuer request identifier, not a Mother-issued,
time-bounded challenge. Exact canonical state/epoch binding fences stale authority.

The single writer samples decision time, verifies current state/signature/nonce
and ceiling, and commits a single OperatorActionRecorded BeforeEffect fact.
Replay uses that recorded decision time, not the present clock. New issuance
cannot regress behind recorded issuance/recovery decision times; this does not
claim an independent trusted time/rollback anchor. Exact committed
retries resolve history even after expiry/revocation; changed payloads conflict.
An uncertain writer is poisoned until reopened and validated.

Store reconstructable issuance provenance beside the issued grants, including
issuer snapshot, intent and decision time. Its presence selects a new complete
state-hash domain. With no provenance and no ceiling fields, old hash paths and
committed recovery corpus remain byte-identical. Old readers must reject the new
fact/field, not ignore its restrictions. Imports and legacy puts cannot introduce
ceilings or issuance provenance. Once enrolled, legacy mutations fail closed.

Recovery maintenance denies issuance. Policy-edit waits may admit independent
issuance; later policy activation must preserve the latest grants. Guardian root
replacement revokes newly issued day grants too and retains their provenance.
This slice does not infer cascading revocation or live legacy migration from an
issuer snapshot. Those runtime authority details remain subject to the cutover
contract; no configuration or caller record becomes canonical authority.

Until authenticated admin runtime is complete, daemon startup refuses ledgers
containing new issuance provenance, just as it refuses enrolled recovery state.
There is no live UID/offline route to this storage API. No push/deploy/merge.

## Proofs

Kernel negatives for missing/empty ceilings, execution versus delegation, both
scope bounds, expiry boundaries, revoked/malformed issuers, root restrictions,
weak keys, and no implicit union. Canonical tests for exact state/target/issuer,
forgery/partial signature, nonce replay, duplicate IDs, serialized issue/revoke
ordering, exclusive transitions, legacy smuggling, snapshot/provenance integrity,
maintenance, policy-edit preservation, recovery invalidation and clock injection.
Exercise partial/complete-frame loss and exact retries after reopening. Add fuzz
corpus and old-reader denial evidence. Keep administration/release obligations
open until the full runtime and remaining owner gates are completed.

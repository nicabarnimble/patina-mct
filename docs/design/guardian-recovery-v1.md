# Voice-bound guardian recovery v1: ratified owner contract

**Status: ratified by the owner (D-54), 2026-10-19; partially implemented, live recovery disabled.**
The owner approved this complete first-decision contract and requested building
it. Recommendations/defaults below are accepted policy; implementation details
must preserve them. D-53 records the preceding design direction. Supporting research: [[recovery-signal-iroh-research]],
[[guardian-recovery-research]], [[voice-identity-history]]. This revision
consolidates the first owner gate; it does not settle the other admin gates.

## 1. Identity and recovery subject

- **Voice** is a stable ownership/context identity, not a speech biometric,
  personality prompt, device, legal-person verification, or Vision.
- One person may control multiple Voices. The same Voice can bind to multiple
  Mothers. A Mother and its Iroh endpoint remain distinct from a Voice.
- **Release profile: one administrative owner Voice per Mother.** A Mother may
  run applications carrying other Voice/context labels, but this release does
  not promise independently isolated per-Voice administration or multi-tenancy.
  Its administrative route scopes remain node-wide.
- A canonical **OwnerBinding** binds a Mother identity to its owner Voice and
  current administrative credentials. Recovery replaces credentials for that
  binding, not the Voice identity, Mother identity, or all other bindings.
- Recovering A does not automatically rotate credentials or change grants on B.
  The UI/audit names both the affected Mother and owner Voice.

### Stable Voice identity without a shared fleet root secret

Recommend a versioned Voice genesis record containing a hybrid public identity
anchor, with a domain-separated digest as VoiceId. Its initial owner proves
possession during explicit bootstrap. This anchor identifies the Voice; it is
**not a permanently active fleet-wide administrative grant**.

Each Mother records its own authorized binding and distinct local admin keys.
Initial enrollment of another Mother requires an authenticated endorsement from
an existing binding, explicit target bootstrap/admission, and proof of possession
of the new local credentials. Endorsements bind exact target and key bytes;
matching a VoiceId string or presenting an old genesis signature is insufficient.
No automatic adoption of permissions from another Mother occurs.

Exact transcript/schema and legacy/virgin validation belong in the slice spec.
The old Patina `.patina/voice` string and empty `identity.age` placeholder are not
imported as authenticated identities. This differs deliberately from the older
proposal to copy one Voice secret onto multiple machines.

## 2. Guardian eligibility and appointment

A guardian is an explicitly appointed **(guardian Mother, owner Voice binding)**,
not a generic fleet peer. Target A and guardian B must have authenticated owner
bindings for the same Voice. Same Vision alone grants no recovery power.

The target stores the accepted guardian public keys, binding references,
allowed recovery operation, quorum, and policy generation/hash in its canonical
ledger. The guardian separately records acceptance and its authority to approve
recovery of that exact target/binding. It uses a distinct hybrid guardian signing
credential; no target root secret is shared.

An approval requires an explicit authenticated administrative action on that
guardian under a recovery-approval scope. Neither downtime, being in the roster,
nor a same-Voice label automatically causes it to sign. This provides an operator
approval step, not a claim that software can prove a human was physically present.

## 3. Quorum and independence

Default supported guarded profile: **2 of 3 distinct guardian Mothers**.
A four-Mother fleet can appoint the other three for each target. Larger fleets
choose three explicitly; full-mesh guardianship is not automatic.

- One unavailable guardian: recovery remains possible.
- One malicious guardian: cannot recover alone.
- Two unavailable guardians: recovery fails closed; unrelated work continues.
- Two colluding guardians: can authorize replacement. They are collectively
  root-equivalent in that power, despite being limited to a recovery operation.

Reject duplicate signer/key/guardian identities as additional votes. Separate
hosts do not prove independent custody: deployment evidence must disclose
shared administrative accounts, control planes, secrets, and backups.

## 4. Enrollment, roster changes, and removal

- Initial protection activation requires the current owner's authorized action
  plus acceptance from all three proposed guardians. Publish the exact roster
  for owner review; permit only one pending configuration transition.
- Once protection is active, any change/removal/disablement requires the current
  local owner authorization AND the existing guardian quorum. Added guardians
  must accept. Apply a 24-hour delay before activation, with durable notices and
  canonical policy-generation change. If recovery is pending, roster edits deny.
- A lost owner key is recovered first; roster change is not an alternate recovery
  path. A stolen owner key cannot immediately remove its guardians.
- If the existing guardian quorum is permanently lost, a still-valid owner may
  continue ordinary administration but cannot silently disable/change protection.
  Recovery of guardian availability or explicit retirement/re-enrollment is then
  necessary. This availability cost is an intentional proposed tradeoff.
- Cancellation of a pending roster edit uses the existing quorum. The owner
  must not bypass that rule by restarting or reverting config.

Implementation clarification pending: the authority retained by a disabled roster
when protection is later re-enabled is not explicit in this contract. See
[open question 13](../OPEN-QUESTIONS.md#13-guardian-authority-after-an-authorized-disablement).
The active-policy rules above remain ratified; neither post-disablement rule has
been silently selected.

## 5. Recovery request and approval

A recovery request names the exact target Mother/epoch, OwnerBinding, VoiceId,
recovery-policy generation/hash, old-root generation, unique request ID, new
hybrid admin keys, and validity bounds. The replacement proves possession of
both keys. No signature from the lost/compromised root is required.

Each guardian approves the same statement under its current local authority.
Collect and verify separate Ed25519 + ML-DSA signatures from two distinct
appointed guardians; no FROST or threshold-ML-DSA prerequisite.

Use short-lived approvals (proposed maximum 10 minutes). At completion obtain a
fresh quorum over the pending recovery and a target-issued completion challenge;
old initiation signatures cannot finalize recovery later. Remote revocation has
bounded propagation: an issued approval may remain usable until its expiry,
subject to the target's current policy. Do not claim instantaneous revocation
across a partition or treat an authenticated connection as fresh authorization.

## 6. Enforcement point, delay, and cancellation

The target's trusted binary, canonical ledger, and exclusive writer enforce the
transition. Guardians do not need a shared ledger or run the target's work.
The OS supervisor may launch the pinned binary into a restricted Iroh recovery
listener; this supplies process availability, not authority.

The target must retain its validated canonical policy/history and endpoint
identity. With sufficient valid approvals it durably enters recovery maintenance
mode before authority changes. No new Child/effect admissions or ordinary admin
mutations occur there. Already-admitted effects/preopens retain their documented
bounds; the protocol does not undo them. Inspection, alerts, and recovery remain
available. Other Mothers keep doing unrelated local work.

Recommended production timing:

- **24-hour minimum recovery delay**, starting at target-local durable acceptance.
- **7-day completion window after that delay**, then the request expires.
- Restart cannot shorten the delay. If elapsed-time evidence is uncertain, impose
  a fresh full delay or refuse; never credit an unverifiable offline wait.
- Clock jumps must not accelerate completion. Enforce the wait using monotonic
  time during a process lifetime; persist enough state to classify restart and
  fail closed when wall-clock/expiry evidence conflicts. Exact clock transitions
  and injected-clock tests must be specified before implementation.

Durable pending notices are visible on the target and involved guardians; emit
best-effort owner alerts. Alert delivery is not identity proof or a guarantee
that a human saw the notice. The delay is a response window, not cryptographic
protection against a malicious quorum.

The old owner/root may raise an alarm but **cannot unilaterally cancel or block
recovery indefinitely**. Cancellation requires the appointed quorum. This
explicitly differs from owner-priority escape designs: the owner credential may
be the stolen credential. Completion/cancellation races serialize at the target
writer. One pending request per binding; conflicting replacements deny.

## 7. Atomic completion and return to service

Commit root replacement, generation advancement, consumed recovery ID, and
invalidation of all pre-recovery day-to-day admin grants as one canonical
transition. Preserve old keys/grants/facts for inspection; no history rewrite.
Ordinary `key-manage` still cannot perform this operation.

Administrative access under the new root returns first, in maintenance mode.
Ordinary work does not resume automatically. The new administrator reviews and
explicitly approves the exact canonical authority snapshot before resuming;
concurrent authority changes invalidate that approval. Re-issue day grants
intentionally. Review Child/Toy/peer/secret authority for malicious changes made
before recovery; replacement of the root alone is not remediation of those acts.
Rotate affected external credentials through their own supported procedures.

The recovering Mother need not change endpoint identity or be re-admitted by
all peers merely because its local admin credential changed. Peers still enforce
current local admission rules; targeted quarantine/revocation remains available.

## 8. Backups, total loss, and compromise boundaries

- Lost admin device with a safe surviving key backup: restore that credential;
  this is not root replacement. Suspected theft uses recovery instead.
- No surviving admin credentials, but sufficient appointed guardians and intact
  trusted target state: use this protocol.
- No guardians enrolled, or insufficient guardians: no reset bypass. Preserve
  D-16 lockout for that case; retire/re-enroll when necessary.
- Process stopped: restart/replay or restricted recovery mode. Downtime does not
  itself prove compromise or trigger automatic root replacement.
- Destroyed storage: signatures cannot reconstruct identity keys or ledger bytes.
  Backup restoration and prevention of concurrent restored instances require
  their own operational proof. Never turn missing policy into virgin bootstrap.
- Compromised host/endpoint identity, corrupt authority, or uncertain full-history
  continuity: this bounded protocol does not establish trust. Quarantine/retire
  the identity, revoke affected peer admissions, preserve evidence, and bootstrap
  a replacement explicitly. Do not promise to stop hostile software remotely.
- One local binding's recovery does not establish global Voice-key rotation,
  fork resolution, or recovery of every Mother after fleet-wide compromise.

## 9. Release obligations and implementation order

Recommend including this bounded v1 in the current completion effort, rather
than deferring it indefinitely. Ratification must explicitly amend D-16's
absolute no-replacement rule, D-41, and D-52, preserving their prior history.
Create a bounded slice and extend the existing `key-recovery` obligation with
named executable proof groups. Keep the gate red until those proofs exist.

Implementation order:

1. Typed canonical Voice genesis/OwnerBinding, guardian policy and acceptance;
   strict hybrid issuance and authenticated Iroh administration.
2. Writer-ordered enrollment/recovery state machine and canonical consumers.
3. Four-process recovery drill plus six-Mother fleet noninterference coverage.
4. Devin handoff for approved disposable-host fault/upgrade/custody drills.

Required negative/crash evidence includes wrong Voice, copied UID, forged binding,
unappointed peer, duplicate signer/key, one-of-three, missing signature half,
wrong target/replacement, expired/stale policy or root generation, roster-removal
bypass, revoked guardian approval boundaries, replay, clock anomalies, restart,
lost reply, cancellation/completion race, torn writes, projection rollback,
old-root/day-grant denial, insufficient-quorum local-work survival, exact-snapshot
resume approval, and rejection of missing/corrupt canonical authority.

Test clocks may accelerate time; production policy must not acquire a test-only
bypass. Source inspection is not release evidence. No production/PiD/Unraid
changes, push, or merge are authorized by this proposal.

## Implementation boundary

D-54 ratifies this first-gate contract. It answers the product choices; detailed
types, signed transcripts, parser limits, clock state machine, proof layout, and
schema compatibility must be pinned in the implementation slice. Discovering a new authority choice
there requires an explicit amendment, not a permissive default.

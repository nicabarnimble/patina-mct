---
type: feat
id: voice-guardian-recovery
status: active
created: 2026-10-19
sessions:
  origin: null
  work: []
related:
  - docs/DECISIONS.md
  - docs/design/guardian-recovery-v1.md
  - layer/core/reliability-doctrine.md
exit_criteria:
  - id: kernel-contract
    text: Typed Voice genesis and exact-intent hybrid quorum validation deny malformed, cross-Voice, stale, duplicate, altered, expired and wrong-purpose evidence.
    checked: true
    verify: cargo test -p mct-kernel recovery:: -- --test-threads=1
  - id: canonical-lifecycle-core
    text: Exclusive canonical recovery transitions preserve delay, consumed IDs, atomic replacement and snapshot review across lost acknowledgements and restart, with live runtime disabled.
    checked: true
    verify: cargo test -p mct-observation guardian_recovery_tests:: -- --test-threads=1
  - id: protected-policy-edits
    text: D-54 and D-55 roster changes, disablement and fresh re-enablement preserve authority separation, delay, consumed IDs and crash/replay ordering.
    checked: true
    verify: cargo test -p mct-observation guardian_recovery_tests::policy_tests:: -- --test-threads=1
  - id: canonical-state-machine
    text: Canonical owner/guardian enrollment, delayed recovery and cancellation, atomic replacement and exact-snapshot resumption survive crash/race/replay.
    checked: false
  - id: authenticated-runtime
    text: Iroh-only admin and restricted recovery paths enforce current canonical authority without UID/offline bypass.
    checked: false
  - id: fleet-and-operator-proof
    text: Four-process quorum recovery, six-Mother noninterference and Devin-run approved disposable-host drills prove the full contract.
    checked: false
---

# Voice-bound guardian recovery

## Authorization

D-54 ratifies [[guardian-recovery-v1]] in full. Its one-owner-Voice profile,
same-Voice appointment, 2-of-3 quorum, delays, cancellation, and compromise
limits are the product law. No whole-Pi port, shared ledger or takeover.

## Slice 1: bounded kernel evidence contract

This first implementation adds strict, substrate-free domain records and pure
cryptographic checks. It **does not** activate a policy, mint runtime authority,
append facts, change old authority hashes, or override D-16 at live ingress.
It is deliberately not a claim that `key-recovery` is implemented.

- VoiceId is BLAKE3 of a versioned/domain-separated canonical genesis transcript
  with Ed25519 and ML-DSA public keys. Proof of possession requires both halves.
  Genesis self-proof proves key control only, not permission to join a Mother.
- OwnerBinding names target node, ledger, epoch, binding ID, VoiceId, current
  root generation and hybrid root keys. Names are bounded ASCII identifiers.
  The runtime must supply this from canonical state, not a request/config file.
- A candidate recovery policy has one target and exactly three canonically
  node-sorted guardian records. Each names an accepted owner binding, generation,
  same Voice and distinct hybrid signing keys. The target cannot guard itself;
  reusing either signature half across guardians or the target root is denied.
  Shape validation is not enrollment/consent authorization. Later canonical
  enrollment must verify owner authorization and all guardian acceptances.
- Recovery intent binds the complete target binding, policy generation/digest,
  request ID, replacement keys and caller-requested validity interval. That
  interval can shorten but never exceed the policy's 24h + 7d total horizon;
  it does not replace the target's later durable acceptance time or delay check.
  Fresh root keys must differ in both halves from old root and guardian keys.
- Replacement possession proof signs the complete intent under a distinct
  domain. It does not authorize replacement on its own.
- Guardian approval signs the intent digest, policy digest, complete guardian
  record, purpose (`initiate`, `cancel`, or `complete` with target challenge),
  issue time and expiry. Purpose/challenge cannot be transplanted.
- Approvals are valid for at most 600 seconds, issued no later than Mother time,
  and deny at expiry. The evaluator accepts only 2 or 3 distinct current roster
  members and rejects the whole envelope on any malformed/nonmember/duplicate
  or invalid signature; it never skips a bad proof to find a quorum.
- Reuse the existing hybrid crypto primitive/context with separate
  length-prefixed recovery message domains. No old admin transcript verifies
  as a recovery transcript. No new optional-signature path.
- Evidence returned by the evaluator is an ephemeral, non-deserializable result
  with exact intent/policy digests, purpose, counted guardians and earliest
  expiry. It is **not an execution capability**: the writer must still check
  pending state, request consumption, target-issued challenge, timing, current
  authority, and serialize commit. This slice alone does not prevent replay.
- New JSON records reject unknown fields. Hex public keys/signatures normalize
  through typed codecs; digest IDs require exact version prefix and lowercase
  hex. Signature length is checked before hex allocation/crypto. Zero generations,
  empty/control/oversized identifiers, weak Ed25519 keys, unsafe timing and
  overflow deny. Transport byte/frame limits remain adapter work, not a claim
  made by serde struct decoding alone.

Canonical JSON means struct declaration field order, fixed enum tags, typed
key encodings and explicit timestamp spelling. There are no maps or floats.
Signatures bind that exact spelling; chronology uses parsed timestamps. Existing
ledger state hash generations and optional legacy signatures are untouched.

## Slice 2: canonical lifecycle

D-54 authorizes this bounded writer/replay implementation; no remote enrollment
or live recovery surface is enabled by it. A new exclusive authority change
records signed enrollment, pending recovery, clock rebasing, target challenges,
quorum cancellation/completion, expiry and exact-snapshot resumption. It uses
existing OperatorActionRecorded observations and BeforeEffect durability.

- Enrollment binds genesis, the exact local binding/policy and all three guardian
  owner bindings. Genesis and current local root sign; all guardian owners and
  distinct guardian approval keys accept. The local root must already be the
  sole canonical hybrid recovery grant. This is not a fleet join protocol:
  authenticated current-binding exchange and same-Voice admission at guardians
  remain required before a runtime may expose enrollment.
- Commands name the exact recovery state digest. Enrollment cannot overwrite a
  binding. No ordinary mutation or legacy import may smuggle in recovery state.
- Existing empty-extension hashes remain byte-identical; enrolled state has a
  separately domain-tagged hash including the complete recovery projection.
- The enrollment epoch is stable binding identity; writer tenure epochs rotate.
  Monotonic readings are meaningful only within one writer tenure. A new tenure
  must commit a clock rebase before another recovery transition. Pending delay
  restarts at a full 24 hours, challenge is discarded, and the original hard
  expiry never extends. UTC rollback denies until time catches up; forward wall
  jumps alone never satisfy the monotonic wait. Both clocks must pass the delay.
  The writer removes pre-acknowledgement write/sync latency from credited monotonic
  elapsed time for newly committed waits, rounding conservatively. This in-tenure
  floor is not portable or persisted as restart credit; reopen requires rebase.
- Completion challenge is writer-issued after the delay, never reused, valid at
  most ten minutes. A live challenge cannot be replaced; after expiry a fresh
  challenge may be issued without another same-tenure delay. Completion approvals
  must be issued after its issuance and
  expire no later than it. Expired requests are terminated by a writer fact;
  cancellation need not override expired approval rules. IDs stay consumed.
- Cancellation/expiry retain maintenance until authenticated exact-snapshot
  review, or a new quorum starts another recovery. Neither silently resumes work.
- Completion mechanically advances root and policy generations without changing
  guardian membership. It replaces the active root record, revokes every day
  grant, consumes pairing tickets and retains prior facts and retired key history.
  Ordinary key-manage still cannot revoke or replace the root.
- Snapshot review signs the whole canonical authority hash and recovery state
  digest under the current root. Pending recovery cannot be resumed around.
- Until authenticated runtime/canonical consumers are finished, an enrolled
  ledger is deliberately unsupported by daemon startup and ordinary authority
  mutation. This prevents old UID/admin paths from serving through the new state.

Required proofs: deterministic timing and stale-command denials; root/quorum
separation; exact-snapshot signatures; append acknowledgement loss and reopen;
complete-frame replay, partial-tail preservation, poisoning and no replayed
replacement; strict parser fuzz smoke and old hash compatibility. Full roster
revision, runtime admission fencing and real-process tests remain later work.

## Slice 3: protected policy edits (D-54, D-55)

Active-roster changes require current owner plus current quorum, newcomer
acceptance, 24-hour delay, no edits during recovery, and quorum-controlled
cancellation. D-55 ratifies termination of guardian authority after an authorized
and delayed disablement. Re-enablement requires current owner, fresh acceptance
from all three proposed guardians, and a fresh 24-hour delay. Former guardians
are history only; they cannot initiate recovery, authorize edits or cancel the
re-enablement proposal. While disabled, the current owner controls withdrawal of
its own pending re-enablement proposal; no proposed guardian has authority yet.

Typed proposals sign exact current state digest, next policy/bindings/generation,
enablement, consumed edit ID and a maximum ten-minute authorization interval.
Owner proposal, guardian acceptance, quorum proposal and cancellation have
separate transcript domains/purposes. Active edits use the old/current roster
for quorum. Re-enablement accepts no old-quorum votes and requires all three
fresh acceptances even if the same guardians are selected again.

Only one pending edit or recovery is permitted. A committed proposal consumes
its ID immediately. Writer activation follows a full dual-clock 24-hour wait;
restart rebases the wait without credit for offline time. The short signatures
are checked at durable proposal; the pending authorization remains canonical
until activation/cancellation. No test clock enters production ingress.
Disablement is an explicit operation, not a simultaneous roster replacement;
replacing a member while enabled retains exactly three distinct members.

An additive optional boxed policy-edit projection preserves slice-2 hashes when
absent and retains generations, consumed IDs and retired guardian keys thereafter.
The last disabled roster is retained for audit but is gated out of every authority
check. Retained guardian keys cannot become replacement target roots; they may
still be explicitly reappointed as guardians with fresh required acceptance.
Policy generations advance independently of root generations. Ordinary
admin/root grants are not mutated by roster edits. Existing OperatorActionRecorded
BeforeEffect facts serialize activation/cancellation and exact retries.

Required tests: owner-only bypass, new members voting themselves in, missing
acceptances, disable/re-enable authority separation, consumed edits, delay,
restart, cancellation/activation ordering, partial frames and acknowledgement-loss
replay. Old corpus replay and new policy-edit fuzz corpus must pass. Live runtime
activation remains disabled pending authenticated cross-Mother binding exchange
and the other admin gates.

## Remaining integration slices

1. Authenticated cross-Mother same-Voice endorsements, explicit target bootstrap,
   current-binding exchange, and guardian-side acceptance/approval facts. Do not
   promote unsigned legacy histories, copied UID selectors or sparse history.
2. Live recovery serving, durable target/guardian notices and best-effort owner
   alerts, integrated clock handling and canonical runtime consumers. The tested
   storage lifecycle is not itself a remotely callable recovery service.
3. Iroh admin/recovery integration and final-admission maintenance fencing;
   canonical consumers; approved exact-snapshot readiness and external-credential
   remediation instructions. Other owner gates must be explicitly settled.
4. Isolated real-process and distributed failure fixtures; custody and restore
   drills; supported artifact/upgrade evidence. Devin executes remote tests only
   after host/action approval. Release check stays red until all obligations pass.

## Evidence discipline

Retain ordinary root-revocation denial tests. New guardian replacement requires
its own canonical transition; do not weaken the old evaluator to make tests pass.
Run kernel negatives plus workspace fmt/Clippy/tests and completion-runner tests.
Persistence-format and physical-write changes in later slices carry replay,
crash, fuzz and recovery proofs at that boundary. Record actual results and
remaining work in EVIDENCE.md, not future intentions as completed proof.

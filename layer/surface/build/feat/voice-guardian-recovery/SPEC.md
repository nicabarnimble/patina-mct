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

## Subsequent slices (required, not implemented by kernel checks)

1. Versioned canonical genesis/OwnerBinding, guardian acceptance and policy
   activation/revision facts; bootstrap and legacy deny rules; no sparse-history
   inference. Protect active roster with owner + quorum and delayed activation.
2. Writer-owned pending state, monotonic/UTC restart classification, notices,
   quorum cancellation/completion, root/day-grant invalidation and consumed IDs.
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

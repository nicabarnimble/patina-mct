# D-57 strict administrative origin evidence

## Implemented boundary

A signed `admin_genesis` first frame atomically records the initial root, birth
authority identity, nonce consumption and strict-profile provenance. Verification
requires both key/signature halves and a distinct transcript. Existing legacy
history remains readable without new fields or changed hashes, but cannot acquire
a genesis after any existing physical prefix. New genesis selects a complete
separate hash domain and cannot be installed through legacy import/mutation.

The typed storage operation additionally compares an independently supplied root
pin, target and inventory. **Its setup constructor is test-only.** This is
conditional correctness given trusted setup, not proof that the external pin or
inventory was honestly obtained. No production setup adapter or bootstrap/admin
listener is enabled. Artifact publication, race fencing and trusted lifecycle are
pending gate 5; existing prototype runtime is not described as cut over.

The writer now records atomic file creation (`create_new`) separately from
reopening an existing file. An existing zero-byte ledger cannot be called virgin.
Complete surviving genesis frames resolve exact retries. Partial frames retain
forensics and do not authorize re-bootstrap. Birth identity is followed by an
actual writer-tenure epoch before ordinary mutations; a historical birth receipt
is not a current authority snapshot.

Root replacement preserves origin provenance without preserving the old root's
authority. Tests compose strict genesis, enrollment, delayed quorum replacement,
exact-snapshot resumption and new-root issuance; the original bootstrap request
then resolves history only. Clock rollback behind birth is denied.

## Validation

Final uninstrumented-source runs:

- Tier-0: **737 passed, 0 failed, 2 existing ignored**. Formatting, warnings-denied
  workspace Clippy, policy checks and RustSec audit passed.
- Completion proofs: **17 groups / 146 passed**. No completion requirement was
  marked implemented by adding these partial proofs.
- Genesis writer: **17 tests**, plus one strict-origin/guardian/issuance composition
  test in the recovery group and one daemon startup-denial test.
- Fuzz: **25,000 runs / 5 seconds**, no failure; three committed public seeds.
  Harness byte bounds are not production ingress limits.
- Actual previous revision **fe1e0c9**, with an isolated build target, rejects
  `admin_genesis` as an unknown canonical fact kind. Reproduce by copying
  `scripts/fixtures/admin-genesis-downgrade.rs` into that revision's observation
  integration tests and setting `MCT_NEW_GENESIS_PREFIX` to this branch's
  `fuzz/corpus/admin_genesis/canonical-prefix.json`. Disposable worktree removed.
- Documentation build, inventory validation, vocabulary and diff checks passed.
  Completion-runner tests: **17 passed**.

Corpus export is explicit through `MCT_GENESIS_CORPUS_OUT` and
`admin_genesis_corpus_roundtrip_and_explicit_export`. Scratch diagnostics are in
`/tmp/mct-genesis-evidence/`; committed tests and fixtures are reproducible sources.

### Unresolved validation finding — not a fixed bug

Earlier validation observed `resident control socket did not bind before
readiness` in both `mother_survives_sigkill_mid_run_with_active_children` and
`six_mothers_independent_revisions_survive_peer_loss_and_restart`. The restart
failure was seed `0x6d637421`, iteration 0, mid-run kill delay 190 ms, boot 1.

An isolated restart reproduction and two complete restart-suite runs passed.
Sixteen fleet runs with temporary task-exit/stage diagnostics also passed, so
those diagnostics did **not** establish a root cause. They were removed before
final Tier-0/proofs/docs runs, which passed. No timeout was increased, test
ignored, assertion weakened, or readiness behavior changed to obtain a pass.

Keep this intermittent readiness failure in the release/lifecycle investigation;
passing reruns do not prove it fixed. Failure logs are retained as
`tier0-restart-readiness-failure.log` and `proofs-fleet-readiness-failure.log`.
The existing runtime waits on a socket pathname before readiness; the observed
failure does not establish whether the control task exited or was still starting.

## Remaining work

Authenticated Iroh ingress, current canonical consumers and final-admission
fencing, removal of all UID/offline alternatives, and the trusted setup/lifecycle
producer. Recipient-bound Child handles and supervisor trust are still separate
owner gates. No migration, release readiness, remote-host access, deployment or
push is inferred from this storage evidence.

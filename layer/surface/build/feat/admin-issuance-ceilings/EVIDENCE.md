# D-56 administrative issuance evidence

## Implemented boundary

`mct-kernel/src/admin_issuance.rs` evaluates one exact issuer grant against an
explicit issuance ceiling, independently of route execution. `AdminKeyGrantV1`
omits absent ceilings to preserve legacy bytes; legacy day key-manage gains no
implicit ceiling. New managers must explicitly declare their ceiling.

`mct-observation/src/admin_issuance.rs` authenticates and commits exclusive
issuance facts against the current canonical target, epoch, generation and hash.
Grant insertion, issuer provenance, generation advancement and nonce consumption
share one BeforeEffect fact. Historical grant IDs, including retired root IDs,
are reconstructed during replay and cannot be reused by new issuance.

New issuance cannot create another root, turn current/retired root credentials
into day keys, bypass a ceiling, exceed a day issuer's expiry, or union same-key
grants. Recovery maintenance denies issuance. Policy-edit activation preserves
concurrent valid issuance; root replacement revokes issued day grants while
retaining provenance. Exact committed retries resolve historical receipts even
when the original issuer is gone or expired.

Legacy puts/imports cannot install ceilings or provenance. New provenance selects
a complete versioned hash domain and disables legacy mutation/runtime serving.
The global signature/legacy cutover is still a separate owner gate; this is not
permission to promote old unsigned histories or expose this storage API through
UID/offline ingress. No live admin/recovery service is enabled by this slice.

## Validation

- `scripts/ci-tier0.sh`: **718 passed, 0 failed, 2 existing ignored**. Formatting,
  warnings-denied workspace Clippy, policy checks and RustSec audit passed.
- `scripts/check-finish.py proofs`: **15 groups / 127 tests passed**. Recovery
  lifecycle/policy filters are separated so the new issuance group is not counted
  twice. Completion requirements remain open despite these partial proofs.
- Kernel issuance tests: **15 passed**, including every pair of the 20 supported
  route scopes, missing/empty ceilings, independent delegated ceilings, expiry,
  weak keys, root restrictions and no union of grants sharing a key.
- Canonical issuance tests: **19 passed**, including signatures/target/state/nonce
  tampering, grant and issuance ID conflicts, historical root-ID reuse denial,
  missing legacy ceilings, maintenance, policy activation, root replacement,
  same-key grants, preserved receipts and clock regression. Final review added
  namespace-squatting denial and issuance after recovery of a maximal-length
  binding; new issuance must neither block future root allocation nor truncate
  generated canonical issuer identities.
- Daemon test rejects a valid issuance ledger through the old runtime without
  rewriting it. Existing recovery and authority tests also pass.
- Fuzz smoke: **25,000 runs / 4 seconds**, no failure; three committed seeds.
  This bounds a smoke experiment, not production parser limits or full coverage.
- Actual prior revision **7e5bc9d**, built in an isolated target directory,
  rejects the new prefix with `unknown variant admin_key_issue`. Reproduce with
  `scripts/fixtures/admin-issuance-downgrade.rs` copied into that revision's
  `mct-observation/tests/`, setting `MCT_NEW_ISSUANCE_PREFIX` to this branch's
  `fuzz/corpus/admin_issuance/canonical-prefix.json`. Disposable worktree removed.
- Documentation build, inventory validation, comparative vocabulary and diff
  checks passed. Completion-runner tests: **17 passed**.

The crash matrix covers partial frames and complete surviving frames on both
sides of acknowledgement. Partial frames preserve forensics and consume no new
grant/nonce; after restart a newly signed current-epoch command is required.
Complete frames replay exactly once and exact retries resolve their original
receipt, even after expiry. Issuance after guardian replacement must use the new
root; old credentials and historically used root grant IDs cannot be resurrected.

One tooling fixture initially failed because it assumed the first open completion
requirement had no proofs. Administration now correctly has partial evidence.
The test now explicitly removes each required proof/evidence field; no validator
rule or release requirement was relaxed. Full validation then passed.

Logs: `/tmp/mct-issuance-evidence/{kernel,canonical,tier0,proofs,docs,fuzz,downgrade}.log`.
These scratch logs are diagnostics; committed tests/corpora are the reproducible
proof sources. Corpus export is explicit through `MCT_ISSUANCE_CORPUS_OUT` and the
`admin_issuance_corpus_roundtrip_and_explicit_export` test.

## Remaining work

Gate 3 signature/legacy cutover, authenticated Iroh admin ingress, supported virgin
bootstrap, current authority consumers, live revocation/lineage semantics and
production input limits. Other release obligations and Devin access remain open.
`administration` and the release gate are not marked implemented.

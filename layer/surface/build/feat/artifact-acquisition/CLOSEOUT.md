# Artifact acquisition close-out

Status: complete — Daily-Driver Slice 3

## Reconstructed range

- Required starting branch/tree: `patina` at `9d5598e` (the implementation began from the exact tree).
- Full slice range: `9d5598e..HEAD`; `HEAD` is the docs-only Task 3 close-out commit.
- Ratification/specification: `89c506b`.
- Fixture and acquisition substrate: `b7c0620`.
- Product command, UDS, approval, catalog resolution, and resident Slate execution: `7ecdd88`.
- Exact approval hardening: `5a765aa`.
- Standing-source hardening: `64dd4ee`.
- Correlated decision/acquisition/observation evidence and reacquisition: `2da794d`.
- Failure, writer-loss, migration, package-shaped acquisition, and immutable-collision proofs: `eddb7eb`.
- Supervised restart/uninstall preservation: `e0e1eee`.
- Track 3 attribution: `72cfe8a`.
- Complete primary evidence assertions: `a4e703e`.
- Task 3 product docs and this close-out: `HEAD`.

The pre-existing untracked session and belief files were not included.

## Failing-test-first record

The first targeted compile after the supervised test was added and before the acquisition API existed was:

```text
cargo test -p mct-daemon --bin mct-daemon \
  supervised_slate_artifact_acquisition_executes_and_revokes_end_to_end -- --nocapture

error[E0425]: cannot find function `stage_operator_pointed_artifact` in crate `mct_daemon`
error[E0422]: cannot find struct, variant or union type `MctArtifactStageRequest` in crate `mct_daemon`
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 2 previous errors
```

This is the implementation-red transcript: the supervised test referenced the product operation before the operation or request type existed. A prior attempt to rebuild the accepted upstream fixture failed because `wasm32-wasip1` was not installed; D1.17 permits the committed, hash-guarded upstream build output and does not represent that environment failure as a product test.

Flake log: **none**. Every later failed command was deterministic and followed by a code/test correction before rerun; no unchanged transient failure was rerun as a flake.

## Commit gates

Each implementation commit listed above passed:

```text
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

Final standing results:

- `allium check layer/allium` — passed, no diagnostics/findings.
- `allium analyse layer/allium` — passed, no diagnostics/findings.
- `cargo test --workspace` — passed: 344 tests (112 daemon library, 101 daemon binary, 2 Wasm-limit integration, 36 Iroh, 82 kernel, 11 observation), plus doc tests.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `./scripts/ci-tier0.sh` — passed.
- `bash scripts/check-comparative-vocabulary.sh` — passed.
- `git diff --check` — passed.
- Ratified Allium law diff from `9d5598e` — empty.

Final targeted `--nocapture` runs:

- `cargo test -p mct-kernel artifact::tests -- --nocapture` — 4 passed.
- `cargo test -p mct-daemon acquisition::tests -- --nocapture` — 6 passed.
- `cargo test -p mct-daemon pre_v7_artifact_migration -- --nocapture` — 1 passed.
- `cargo test -p mct-daemon --bin mct-daemon artifact -- --nocapture` — 8 passed, including the supervised proof.
- Both closed-registry tests — passed.
- Both child-name-only approval tests — passed.
- Standing-source validation and acquisition-provenance state tests — passed.

## Required Integration Proof — 16-row disk reconstruction

All citations refer to `crates/mct-daemon/src/daemon/supervisor_lifecycle.rs`.

| Step | Landed proof |
|---:|---|
| 1 | Lines 3258–3280 load the committed `slate-manager@0.2.0` fixture, assert exact byte size, SHA-256 and BLAKE3 values, and prove source sidecars are absent. |
| 2 | Lines 3281–3336 create the isolated supervisor/service/config/identity/catalog/state/ledger/UDS/project/fake-launchd paths, install/start, and await real resident readiness. |
| 3 | Lines 3338–3357 submit the exact operator-pointed stage request through resident UDS. Lines 3464–3492 correlate the durable adapter-start, acquisition, and verification sequence; `control::tests::artifact_acquisition_append_failure_suppresses_filesystem_and_catalog_effects` proves failed authority append never enters staging or creates state/catalog paths. |
| 4 | Lines 3358–3406 prove source bytes and modes are unchanged, all paths are isolated, and the canonical package plus both SHA-256 sidecars exists under the digest catalog. |
| 5 | Lines 3408–3492 inspect authority path, consumed decision, adapter authority reference, credential-free source shape, exact size/BLAKE3, acquired/verified outcomes, observation ids, and component-artifact cross-reference. |
| 6 | Lines 3408–3507 inspect exact child/version/hash/export/provenance/acquisition facts and prove staging created no approval, assignment, or ToyGrant. |
| 7 | Lines 3510–3518 submit `list-work` and prove denial before approval. |
| 8 | Lines 3520–3563 approve the exact SHA-256 artifact through resident mutation, assert the structured acquisition-evidence response, and order separate exact `ChildApproved` and `ChildAssigned` facts. |
| 9 | Lines 3564–3583 submit before ToyGrants and prove denial without another runtime start. |
| 10 | Lines 3585–3609 authorize the four existing Slate Toys and prove active artifact/assignment/node/resource-scoped grants. |
| 11 | Lines 3611–3622 execute the real component through resident UDS and assert the real `fixture-work` result. |
| 12 | Lines 3623–3642 inspect `/runs` and `/snapshot` without payload projection; lines 3675–3700 reopen state/ledger and correlate acquisition, verification, approval, assignment, execution, and revocation facts. |
| 13 | Lines 3644–3672 revoke, submit a fresh-key call, prove denial, and prove no additional runtime start. |
| 14 | Lines 3673–3701 stop cleanly, reopen disk state and ledger, and verify acquisition/artifact/package/grants/revocation remain. The fresh store contains one acquisition-backed artifact and no historical artifact. |
| 15 | Lines 3702–3799 restart from the same governing record, prove revocation still denies, stop/uninstall, and verify package, acquisition/decision state, runs, ledger, config, identity, and project survive while record/plist/loaded policy are removed. |
| 16 | Lines 3281–3406 construct and canonicalize every mutable path beneath the temporary root. The only fixture reads occur before the isolated source copy; the test has no path to `~/.patina/plugins`, a sibling Slate checkout, real `~/Library/LaunchAgents`, or machine launchd. |

Difference statement: no required integration step is waived. Step 3's no-read-before-append claim is established by the append-failure regression preventing entry into the staging function and by the primary ordered durable identities, rather than by relying on filesystem access-time metadata.

## Additional failure evidence

- Standing scope/currentness/policy/root/action independence: `mct_kernel::artifact::tests::*`, `state::tests::standing_source_creation_rejects_unbounded_credentialed_and_unsupported_records`.
- Missing/mismatched sidecars, digest/claim/export errors, oversize files, and escaping symlink: `acquisition::tests::malformed_tampered_oversize_and_escaping_sources_leave_attempt_evidence_only`.
- Failed attempt without artifact: `artifact_acquisition_failures_are_observed_without_artifact_publication`.
- Decision consumption and same-byte reacquisition: `identical_reacquisition_adds_evidence_without_replacing_immutable_artifact`.
- Same digest with conflicting manifest fact: `same_digest_different_manifest_fact_cannot_replace_catalog_artifact`.
- Package-shaped read ordering: `package_shaped_acquisition_discovers_declared_component_only_after_authority_start`.
- Append failure before adapter start: `artifact_acquisition_append_failure_suppresses_filesystem_and_catalog_effects`.
- Writer loss after read: `artifact_writer_loss_after_read_leaves_no_artifact_authority_or_catalog_package`.
- Historical migration: `pre_v7_artifact_migration_marks_historical_unknown_without_fabricating_acquisition`.
- Exact approval failures: `child_name_only_approval_is_rejected_before_authority_or_config_effect` and `exact_approval_refuses_wrong_historical_failed_and_tampered_artifact_evidence`.
- Ambient registry closure: `live_registry_install_and_sync_are_closed_without_storage_effects` and `registry_is_closed_and_offline_lock_contention_refuses_legacy_helper`.

Track 3 dispositions are in `layer/surface/build/spec-drift-audit/track3/LEDGER.md`; there are no implicit waivers. Network acquisition, update channels, recurring scheduling, historical reconciliation, and credential attachment remain explicitly deferred future gates.

## Fixture provenance

`crates/mct-daemon/tests/fixtures/slate-manager-0.2.0/PROVENANCE.md` binds the fixture to Slate tag `v0.2.0`, commit `fb85706aad55fdfbf091e28ac8f4c09864996b0c`, and the release build command.

- Manifest SHA-256: `b6d7b4e532df5b787acd37f3ae8c25ed093552097e5cf6dbc5c7eaca360e4919`.
- Component SHA-256: `76b568f40491d7e3bd1dcb55644ec7c42dbc393642a5a7a2ba5b1daa1ea6966a`.
- Component BLAKE3: `e06cab5f7605f3c070ef792f67f7b71a179d8a9c7da0c45e525b39e8a3a88e7d`.
- Component bytes: `1,338,615`.

## Daily-driver statement

Slate's one-node supervised acquisition-to-execution fixture is proven. `folder-watch-actor@0.1.0` and `watch-null-sink@0.1.0` remain fixtures two and three before the full three-fixture `patinaMother` replacement test can close.

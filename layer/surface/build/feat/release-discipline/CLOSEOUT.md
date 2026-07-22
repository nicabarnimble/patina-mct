# Release discipline R3 close-out

Status: complete — MCT 0.2.0 pre-GA release mechanics

## Claim boundary

R3 closes version, package, SBOM/provenance, signing-layout, daemon-release evidence, operator-file acquisition, exact-approved lifecycle upgrade, real packaged smoke, baseline, attribution, and operator-documentation obligations.

It does not close operational `patinaMother` shutoff or into-the-wild GA. Network acquisition/update-channel authority, notarization activation, Linux/systemd/signing, Homebrew, JVM SDK, launcher/interface work, and hard SLOs remain deferred.

## Reconstructed implementation range

- Ratified D1.1–D1.16: `6152a8d`.
- Unified product version: `8a259a5`.
- Closed hostile-archive verification: `064b99d`.
- Deterministic fixture-aware SBOM: `6db16e4`.
- Signed bundle amendment/assembly: `781c863`, `d404e09`.
- Distinct daemon-release evidence and operator-file acquisition: `b6ad158`, `fb3bf6d`.
- Source-closed resident/offline routing: `464f8ba`.
- Exact approval/shared replacement/post-verification: `d1970d3`.
- Failure matrix and writer-loss rollback: `d4ab99b`.
- D1.18 internal smoke plist seam: `aba4440`, `a6b6b5a`.
- Real packaged smoke and corrections: `e19d14e`, `d24ed35`, `10320aa`.
- Baseline harness and record: `72e69a7`, `08a508f`, `50a75a7`.
- Track 3 and operator docs: `67d0c1a`, `dcd5510`.

The two pre-existing untracked session/belief artifacts were never included in a release worktree or commit.

## Failing-test-first and deterministic-failure record

The first version gate failed before workspace inheritance existed. The first archive test failed to compile before the verifier API existed. D1.17 stopped implementation after real `codesign` proved that activation-ready bundle signing necessarily creates `Contents/_CodeSignature/CodeResources`; the operator ratified the exact closed member. D1.18 stopped implementation after the fixed production plist collision was proven; the operator ratified an internal root-local plist seam with fixed-label exclusivity and production-file byte snapshots.

The first real smoke failed because the long macOS temporary path exceeded the Unix-socket path bound. Cleanup could not claim a clean lifecycle, so the fixed-label service was safety-unloaded and its preserved root diagnosed; the next implementation used a bounded `/private/tmp` root. A later deterministic trigger request used the wrong serde tag and was corrected to `source_kind`. The first full baseline used a one-second recovery interval, allowing additional turns; the final exact 60-second interval proves one 4,097-occurrence turn.

No unchanged transient failure failed twice. The recorded R2B targeted trigger failure passed on its exact rerun and all later complete suites.

## Required Integration Proof reconstruction

| Step | Landed evidence |
|---:|---|
| 1 | Tier-0 `check-release-version.sh` joins workspace, lockfile, changelog, and exact-tag state at 0.2.0. |
| 2 | Detached `release-local.sh build` uses `--locked`, records source/toolchain, and emits only the supported target archive. |
| 3 | `verify-release` enforces both sidecars, closed bounded archive layout, internal checksums, and no partial extracted tree. |
| 4 | Target assembly creates the exact app bundle; `codesign --verify --strict` proves the extracted signed payload including only `CodeResources`. |
| 5 | CycloneDX 1.6 and fixture provenance generation verifies all three committed fixture receipts and excluded scope. |
| 6 | Schema v11 stores `DaemonReleaseArtifactV1` separately from Child/Toy authority ontology. |
| 7 | Operator-file acquisition appends decision/effect/terminal evidence before immutable digest publication; writer loss rolls back visibility. |
| 8 | The real smoke acquires the primary archive, inspects daemon-release evidence, and proves no ComponentArtifact was created by that acquisition. |
| 9 | The packaged resident acquires all three copied fixtures and proves denial before exact Child approval and ToyGrants. |
| 10 | Slate completes with real result bytes; folder-watch actor produces scoped Watch evidence and the exact null sink receives the ordinary nested call. |
| 11 | Trigger and Watch revocation deny fresh effects and survive a clean real-launchd restart. |
| 12 | A same-version/different-archive-digest candidate is generated with `release_mode=smoke` and separately verified. |
| 13 | Missing and wrong exact approval create acquisition/denial evidence but leave supervisor record, running resident, and executable binding unchanged. |
| 14 | Digest-exact approval precedes shared clean stop, `install --replace`, start, and the named 30-second health/revision/digest post-verification. |
| 15 | Upgrade success reaches supervisor revision 2; failure tests preserve prior immutable bytes and emit no automatic-rollback claim. |
| 16 | Stop/uninstall removes only current smoke record/plist/loaded label and preserves ledger, state, identity, config, runs, Child artifacts, and both daemon releases. |
| 17 | Smoke preflight refuses an occupied fixed label; default production record/plist presence and bytes compare exactly before/after; the distributed CLI cannot invoke the alternate plist seam. |
| 18 | Baselines record exact startup/RSS/call/throughput/trigger/fixture sample counts with correctness gates and no numeric admission threshold. |

No Required Integration Proof row is waived.

## Evidence locations

- Release contract: `layer/surface/build/feat/release-discipline/SPEC.md`.
- Security review: `layer/surface/build/product/RELEASE-REVIEW-R1.md`.
- Release checklist: `layer/surface/build/product/RELEASE-CHECKLIST-v0.md`.
- Upgrade guide: `layer/surface/build/product/RELEASE-UPGRADE-v0.2.0.md`.
- Baselines: `layer/surface/build/product/BASELINES-v0.2.0-aarch64-apple-darwin.md`.
- Attribution: `layer/surface/build/spec-drift-audit/track3/LEDGER.md`.
- Packaged smoke transcript: generated adjacent to the final local archive as `<archive>.smoke.txt`.

## Validation contract

Final reconstruction runs:

```text
allium check layer/allium
allium analyse layer/allium
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo audit
./scripts/ci-tier0.sh
./scripts/release-local.sh build --target aarch64-apple-darwin --output target/release-artifacts
./scripts/verify-release-artifact.sh target/release-artifacts/mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz
./scripts/release-local.sh smoke --artifact target/release-artifacts/mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz --nocapture
```

Archive identities are recorded by their external sidecars and final smoke transcript rather than embedded here, avoiding a recursive source-commit/package-checksum claim.

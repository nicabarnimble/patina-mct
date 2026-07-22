# MCT 0.2.0 pre-GA release checklist

Scope: evidence-backed `mct-daemon` release mechanics for the supported `aarch64-apple-darwin` target. Version 0.2.0 proves runtime, package, upgrade, and measurement discipline; it does **not** claim operational `patinaMother` shutoff or into-the-wild 1.0.0 GA.

## Product version and source

- [x] All four workspace crates inherit `workspace.package.version = "0.2.0"`.
- [x] `mct-daemon version`, `Cargo.lock`, and the committed `CHANGELOG.md` section agree.
- [x] `scripts/check-release-version.sh` is part of tier-0 and checks any exact annotated `v0.2.0` tag when present.
- [x] Local build/smoke uses a detached clean worktree; publishing remains gated on a fresh clean exact tag.
- [ ] Exact annotated `v0.2.0` publication tag exists. This is a publication-time action, not an R3 implementation claim.

## Locked package and supply-chain evidence

- [x] `cargo build --release --locked --target aarch64-apple-darwin` produces the manifest-selected app-bundle payload.
- [x] The closed archive verifier enforces exact members, order, modes, normalized ownership/timestamps, bounded two-pass extraction, and no escaping members.
- [x] Internal `CHECKSUMS` and external SHA-256/BLAKE3 sidecars bind all distributed bytes.
- [x] CycloneDX 1.6 SBOM generation is pinned to `cargo-sbom 0.10.0`, deterministic after normalization, and includes all workspace/transitive packages.
- [x] `slate-manager@0.2.0`, `folder-watch-actor@0.1.0`, and `watch-null-sink@0.1.0` appear as excluded proof fixtures with exact receipts and rebuild/patch provenance.
- [x] `LICENSE` is the exact repository MIT license selected by the release manifest.
- [x] `cargo audit` runs in tier-0 under the explicit `.cargo/audit.toml` policy.

## Platform and signing

- [x] `aarch64-apple-darwin` is the only supported 0.2.0 release target.
- [x] Packaging creates the exact minimal `.app`, including only the generated `Contents/_CodeSignature/CodeResources` signature resource.
- [x] Ad-hoc `codesign` runs before checksums and extraction verification runs `codesign --verify --strict`.
- [x] Developer ID/notarytool/stapler is a credential-gated insertion slot over the same bundle layout; it is not activated or represented as completed.
- [x] Linux build/supervision/signing adapters refuse as unavailable rather than emitting a simulated supported artifact.

## Daemon release acquisition and upgrade

- [x] `DaemonReleaseArtifactV1` and its attempt/evidence projections are distinct from ComponentArtifact, ChildApproval, ChildAssignment, ChildInstance, and ToyGrant ontology.
- [x] Operator-file acquisition verifies both archive digests, target, manifest, notes, SBOM, provenance, executable, and signature before immutable publication.
- [x] Live acquisition uses the owner-authenticated resident writer; offline acquisition requires exclusive ledger ownership; writer loss rolls publication back.
- [x] Upgrade planning consumes verified daemon-release evidence and admits no network or credential-bearing source.
- [x] Approval must equal the complete `sha256:<archive-digest>` identity. Version, filename, `yes`, EOF, wrong digest, and prior evidence grant no replacement authority.
- [x] Exact approval is durable before the shared clean stop / `install --replace` / start lifecycle.
- [x] Post-verification is bounded by `MCT_UPGRADE_POST_VERIFY_DEADLINE_SECONDS = 30` and checks health, readiness, version, successor revision, and executable digest.
- [x] Failure retains immutable prior releases and prints explicit manual rollback guidance; no automatic rollback is claimed.

## Packaged release smoke

- [x] `scripts/release-local.sh smoke` verifies and executes the distributed bytes through real user launchd.
- [x] The fixed `io.patina.mct.mother` label is refuse-not-skip exclusive: a loaded production resident is never stopped by the smoke.
- [x] The root-local smoke plist seam exists only in a feature-built orchestration harness and is absent from the distributed production CLI.
- [x] Default production record/plist presence and bytes are snapshotted before mutation and compare unchanged after cleanup.
- [x] The packaged resident proves all three copied fixtures, denial before authority, exact approval/grants, Slate output, temporal Watch delivery, revocation, and restart persistence.
- [x] A smoke-only same-version/different-digest archive proves missing/wrong approval has no lifecycle effect and exact approval completes revisioned replacement.
- [x] Stop/uninstall removes only current smoke supervision while preserving identity, config, state, ledger, Child artifacts, runs, and both immutable daemon releases.

## Baseline evidence

- [x] [`BASELINES-v0.2.0-aarch64-apple-darwin.md`](BASELINES-v0.2.0-aarch64-apple-darwin.md) records five startup samples and seven settled idle-RSS samples.
- [x] It records 100 warmups plus 1,000 sequential UDS calls and four clients × 500 throughput calls with complete success counts.
- [x] It records the 32-evaluation trigger turn: 31 candidates plus one terminal record covering 4,066 refusals, while ordinary status remains available.
- [x] It records `/usr/bin/time -l` and storage deltas for the complete three-fixture proof.
- [x] Baseline numbers are evidence only and create no runtime or release SLO.

## Standing validation

- [x] `allium check layer/allium` and `allium analyse layer/allium`.
- [x] `cargo test --workspace`.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` (release close-out also checks all features).
- [x] `cargo audit`.
- [x] `./scripts/ci-tier0.sh`.
- [x] Package verification, platform signature verification, real packaged smoke, and baseline completeness gates.

## Explicitly deferred

- [ ] Operational `patinaMother` shutoff after inventory/adjudication of remaining Patina/interface behavior.
- [ ] Into-the-wild 1.0.0 publication and update-channel authority.
- [ ] `NetworkArtifactAcquisitionAdapter`, scheduled/background discovery, publisher/channel/latest selection, and credential attachment.
- [ ] Notarization execution, Linux/systemd, Linux signing, Homebrew, JVM SDK, launcher/interface, and hard performance SLOs.

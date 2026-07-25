# MCT 0.2.0 assurance

This document maps MCT 0.2.0 claims to landed evidence and commands that a
reader can run. Version 0.2.0 is pre-GA and supports only
`aarch64-apple-darwin`; it does not claim into-the-wild publication or
operational `patinaMother` shutoff
([release checklist](layer/surface/build/product/RELEASE-CHECKLIST-v0.md),
[R3 close-out](layer/surface/build/feat/release-discipline/CLOSEOUT.md)).

## Runtime replacement proof

The supervised three-fixture test proves this bounded runtime claim:

1. acquired `slate-manager@0.2.0` executes after exact approval and scoped Toy
   grants;
2. source-derived `folder-watch-actor@0.1.0` executes from temporal trigger
   authority through scoped Watch observation; and
3. exact, unmodified `watch-null-sink@0.1.0` receives the narrowed ordinary
   Child call-out.

The test also covers revocation, restart, immutable acquisition evidence, and
reopened state. The assertions are in
[`supervisor_lifecycle.rs`](crates/mct-daemon/src/daemon/supervisor_lifecycle.rs)
under `supervised_trigger_watch_delivery_fixtures_execute_end_to_end`; the
line-by-line reconstruction is in the
[Watch fixture close-out](layer/surface/build/feat/watch-event-fixtures/CLOSEOUT.md).
Run it with:

```bash
cargo test -p mct-daemon --bin mct-daemon \
  supervised_trigger_watch_delivery_fixtures_execute_end_to_end \
  -- --exact --nocapture
```

This proof is the runtime-responsibility claim ratified in
[`RELEASE-REVIEW-R1.md`](layer/surface/build/product/RELEASE-REVIEW-R1.md). It
is not evidence that every `patinaMother` application or interface service has
been replaced.

## Release artifact flow

The release flow is an ordered chain over the distributed bytes:

1. `scripts/release-local.sh build` creates a detached clean worktree, builds
   with the committed lockfile, generates release notes and a normalized
   CycloneDX 1.6 SBOM, assembles the target payload, applies ad-hoc macOS
   codesigning, and emits SHA-256 and BLAKE3 evidence
   ([script](scripts/release-local.sh),
   [release contract](layer/surface/build/feat/release-discipline/SPEC.md)).
2. `scripts/verify-release-artifact.sh` invokes the closed Rust archive
   verifier and target signature verifier
   ([script](scripts/verify-release-artifact.sh),
   [verifier](crates/mct-daemon/src/release.rs)).
3. `scripts/release-local.sh smoke` re-verifies and executes the packaged
   binary through real launchd install/start, the three-fixture proof,
   digest-exact same-version upgrade, stop, uninstall, and preservation checks
   ([script](scripts/release-local.sh),
   [R3 close-out](layer/surface/build/feat/release-discipline/CLOSEOUT.md)).

Reproduce the build and verification with:

```bash
./scripts/release-local.sh build \
  --target aarch64-apple-darwin \
  --output target/release-artifacts

archive=target/release-artifacts/mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz
./scripts/verify-release-artifact.sh "$archive"
./scripts/release-local.sh smoke --artifact "$archive" --nocapture
```

The archive contains internal checksums, external SHA-256/BLAKE3 sidecars, a
CycloneDX 1.6 SBOM, fixture provenance, release notes, and an ad-hoc-signed app
bundle. Ad-hoc signing establishes package consistency, not publisher identity;
notarization is not activated
([release checklist](layer/surface/build/product/RELEASE-CHECKLIST-v0.md)).

## Validation tiers

- **Tier 0** is the repository script
  [`scripts/ci-tier0.sh`](scripts/ci-tier0.sh): formatting, release
  consistency/tooling checks, dependency audit, Clippy with warnings denied,
  serialized workspace tests, comparative vocabulary, and Allium validation.
- **Tier 1** is the GitHub Actions job in
  [`.github/workflows/ci.yml`](.github/workflows/ci.yml). It installs Rust,
  Allium, and pinned `cargo-audit 0.22.2`, then runs Tier 0 on Ubuntu.
- **Target release evidence** remains a separate macOS gate: archive/signature
  verification, real launchd smoke, and baseline capture are defined in the
  [R3 contract](layer/surface/build/feat/release-discipline/SPEC.md) and are not
  implied by portable Tier 1 success.

Run the portable local gate with:

```bash
./scripts/ci-tier0.sh
```

## Committed performance evidence

[`BASELINES-v0.2.0-aarch64-apple-darwin.md`](layer/surface/build/product/BASELINES-v0.2.0-aarch64-apple-darwin.md)
records the artifact and host identity, raw samples, methods, and summaries for
startup, idle RSS, sequential UDS latency, concurrent UDS throughput, trigger
recovery load, and the complete three-fixture segment. The document explicitly
treats those values as evidence rather than SLOs. Its reproduction command is
recorded in that file and implemented by
[`scripts/release-local.sh`](scripts/release-local.sh).

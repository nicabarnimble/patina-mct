# Release artifact

Release verification tells an operator whether a distributed `mct-daemon` archive matches the closed R3 checks. On a non-Darwin host the automatable proof is the rejection path.

## Sub-features

- `release-usage` rejects a missing archive path.
- `release-basename` rejects an archive whose name is not the R3 Darwin name.
- `release-bogus` rejects a correctly named file that is not a real archive.
- `release-darwin` verifies a real archive. This sub-feature is manual on any host that is not Darwin arm64.

## How to get to it (user POV)

- Run `scripts/verify-release-artifact.sh` with no argument.
- Run `scripts/verify-release-artifact.sh <file>` when `<file>` does not use the `mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz` basename.
- Run `scripts/verify-release-artifact.sh <archive>` with that basename when you have a real archive.
- On Darwin arm64, the same script continues into `codesign --verify`. That host is the passing entry point.

## Driving it with verify-patina-mct

Preconditions:

- `VERIFY_PATINA_MCT_ROOT` points at this run's directory.
- You do not have to possess a release archive to run the rejection checks.

- **Rejection drive.** Run `.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh release-artifact`. Exit code `0` from the helper means the rejection checks behaved as stated. The script under test still exits nonzero.
- **Usage.** `proof/release-results.tsv` shows `release-usage` with exit `2`, and stderr contains `usage:`.
- **Basename.** `release-basename` shows exit `1`, and stderr contains `unsupported R3 target basename`.
- **Bogus archive.** `release-bogus-archive` shows a nonzero exit. The helper feeds a text file with the legal basename to `scripts/verify-release-artifact.sh`. On this checkout the verifier stops because the `.sha256` sidecar is missing. That failure is a refusal, not a passing release.
- **Real archive.** When you have a real archive on Darwin arm64, run `scripts/verify-release-artifact.sh <archive>` yourself. Record its exit code. Until that host and that file exist, mark `release-darwin` manual. Do not copy a rejection log forward as a pass.

## Gotchas

- The legal basename is `mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz`. A different version or target stops in the shell before the Rust verifier.
- The signature adapter in `scripts/release/targets/aarch64-apple-darwin.sh` requires Darwin arm64. A Linux run cannot finish a real verification even if the bytes are intact.
- `release-artifact` creates the junk files under `scratch/` and cleanup deletes them. The proof is the tsv and the stderr files.
- A zero exit from `scripts/verify-release-artifact.sh` is the only pass for a real archive. The helper's own zero exit only means the rejection checks fired.

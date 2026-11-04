# Getting started

MCT 0.2.0 is pre-GA and supports macOS arm64 and Linux arm64. For Linux
`aarch64-unknown-linux-gnu`, follow the Nix/systemd admin-only installation
procedure in the repository's `docs/linux-admin-install.md`.
The full passkey Child runtime remains closed.

The following archive procedure is for macOS `aarch64-apple-darwin`. Its release
archive, SHA-256 sidecar and provenance sidecar are a single verification set.
Verify them from a trusted checkout before extracting the archive.

```bash
archive=/absolute/path/mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz
./scripts/verify-release-artifact.sh "$archive"
```

Set `MCT` to the executable inside the extracted archive, install the observed
launchd service, and ask it for machine-readable status:

```bash
MCT=/absolute/path/mct-daemon-v0.2.0-aarch64-apple-darwin/payload/mct-daemon.app/Contents/MacOS/mct-daemon
"$MCT" install --executable "$MCT"
"$MCT" start
"$MCT" status --json
```

## What this proves

A successful status response proves that the installed Mother is reachable
through its local control plane. It does not approve a Child, issue a ToyGrant,
or establish peer authority.

## Development checkout

Contributors need the pinned Rust toolchain:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

For the structure behind these commands, continue to
[Core concepts](../concepts/index.md). Operational lifecycle and recovery
belong in [Operations](../operations/index.md).

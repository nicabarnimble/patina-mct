# Linux admin Mother

The Linux admin Mother is an aarch64 systemd service installed from the Nix flake. The smoke proves install, signed status, revocation, restart, a WASI probe, and cleanup on that host.

## Sub-features

- `linux-smoke-command` checks host support, then runs `nix run .#mother-install-smoke` on a supported host and records the real exit code.
- `linux-smoke-pass` is the operator smoke on aarch64 Linux with systemd, passwordless sudo for systemctl, and an ext4, XFS, or Btrfs home filesystem.
- `linux-pi-record` is the physical Raspberry Pi evidence. It is not re-run from a generic VM.

## How to get to it (user POV)

- On the aarch64 build host, run `nix build .#mother` and then `nix run .#mother-install-smoke`, as [docs/linux-admin-install.md](../../../../docs/linux-admin-install.md) describes.
- On nicanode0, the accepted invocation is the offline `nix run .#mother-install-smoke` recorded in [docs/nicanode0-install-evidence.md](../../../../docs/nicanode0-install-evidence.md).
- On nicamaster, the earlier accepted run is recorded in [docs/linux-admin-install-evidence.md](../../../../docs/linux-admin-install-evidence.md).
- For an already provisioned root, the operator runs `nix run .#mother-admin-service` and then `systemctl enable --now`. That path is a separate entry point and needs a real passkey root.

## Driving it with verify-patina-mct

Preconditions:

- `VERIFY_PATINA_MCT_ROOT` points at this run's directory.
- Read `flake.nix`. `outputs` sets `system` to `aarch64-linux`.

- **Attempt the smoke.** Run `.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh nix-smoke`. On a supported host this invokes a real systemd install smoke. Require the operator's approval and the native host preconditions before this command.
- **Record the host.** `proof/nix-host.txt` contains `uname -s`, `uname -m`, whether `nix` is on `PATH`, and the flake system `aarch64-linux`.
- **Classify the result.** A host other than Linux aarch64 returns `2` as `out-of-vm` without invoking Nix. A supported host without Nix returns `127` in that class. An executed Nix process is `auto`, with its exit code unchanged, whether it passes or fails. Read `proof/nix-results.tsv` and `proof/nix-smoke.stderr`. Do not change a skipped or failed result to `0`.
- **Passing smoke.** A pass prints `Software test authority only; not a 1Password ceremony or production identity.` and the `PASS` lines for install, signed status, second writer, revoke, forced restart, revoked key, WASI probe, immutable installer inputs, and isolated cleanup. It also prints the cold-readiness, RSS, compile, and durable-write `METRIC` lines. The private evidence directory stays on that host and is not uploaded.
- **Pi evidence.** Quote [docs/nicanode0-install-evidence.md](../../../../docs/nicanode0-install-evidence.md) as a historical record. Do not replay it by editing the document or by claiming the current host is nicanode0.

## Gotchas

- The smoke uses software test authority. A green smoke is not a 1Password ceremony and not a production identity.
- The smoke needs noninteractive sudo and the host `/usr/bin/systemctl`. A VM without those preconditions cannot be nudged into a pass by skipping the sandbox checks.
- Cleanup on the real host removes only the unit and package link that the smoke created. Canonical state from a provisioned Mother is a different command and is not deleted here.
- `nix-smoke` on an unsupported host records a skipped installer, not an attempted or passing native smoke.
- The native smoke creates its own private evidence directory under the host home and installs a temporary systemd unit. The helper run root holds its captured output, not that native state.
- Pi 3 nodes consume a trusted cache. Do not treat a local Rust compile on a larger VM as the Pi build.

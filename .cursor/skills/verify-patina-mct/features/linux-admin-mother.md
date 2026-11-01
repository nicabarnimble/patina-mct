# Linux admin Mother

The Linux admin Mother is an aarch64 systemd service installed from the Nix flake. The smoke proves install, signed status, revocation, restart, a WASI probe, and cleanup on that host.

## Sub-features

- `linux-smoke-command` runs `nix run .#mother-install-smoke` and records the real exit code.
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

- **Attempt the smoke.** Run `.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh nix-smoke`. The helper returns the `nix` process exit code unchanged.
- **Record the host.** `proof/nix-host.txt` contains `uname -s`, `uname -m`, whether `nix` is on `PATH`, and the flake system `aarch64-linux`.
- **Classify the result.** When `uname -m` is not `aarch64`, or `nix` is absent, write the row as out of VM. Keep the real exit code from `proof/nix-results.tsv`. Do not change it to `0`.
- **Passing smoke.** A pass prints `Software test authority only; not a 1Password ceremony or production identity.` and the `PASS` lines for install, signed status, second writer, revoke, forced restart, revoked key, WASI probe, immutable installer inputs, and isolated cleanup. It also prints the cold-readiness, RSS, compile, and durable-write `METRIC` lines. The private evidence directory stays on that host and is not uploaded.
- **Pi evidence.** Quote [docs/nicanode0-install-evidence.md](../../../../docs/nicanode0-install-evidence.md) as a historical record. Do not replay it by editing the document or by claiming this VM is nicanode0.

## Gotchas

- The smoke uses software test authority. A green smoke is not a 1Password ceremony and not a production identity.
- The smoke needs noninteractive sudo and the host `/usr/bin/systemctl`. A VM without those preconditions cannot be nudged into a pass by skipping the sandbox checks.
- Cleanup on the real host removes only the unit and package link that the smoke created. Canonical state from a provisioned Mother is a different command and is not deleted here.
- `nix-smoke` on the wrong architecture is evidence that the gate was attempted. It is not evidence that the gate passed.
- Pi 3 nodes consume a trusted cache. Do not treat a local Rust compile on a larger VM as the Pi build.

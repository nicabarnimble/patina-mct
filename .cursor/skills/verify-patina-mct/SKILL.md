---
name: verify-patina-mct
description: "Drive patina-mct the way an operator does, beyond unit tests. Use for /verify-patina-mct, to prove Mother and admin CLI behavior, or to run the repository gates and record what each check proves. Hardware ceremonies stay manual."
---

# Verify patina-mct

This skill is for an agent proving patina-mct from a checkout. The operator-facing commands are `mct-daemon` and the repository scripts. Passkey taps in the default browser, Raspberry Pi installs, and macOS launchd smoke stay manual. Do not invent their results.

Read [features/README.md](features/README.md) before driving a feature. The helper is [scripts/verify-patina-mct.sh](scripts/verify-patina-mct.sh).

## Launch

A short-lived CLI has no server to keep up. Launch builds `mct-daemon` once. Each drive then uses that binary with its own run directory.

```sh
export VERIFY_PATINA_MCT_ROOT=/tmp/verify-patina-mct-$RUN_ID
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh launch
```

`RUN_ID` is any new token. The root must be an absolute path outside the home directory. The helper creates `scratch/` at mode 0700 and `proof/` at mode 0700. Launch is finished when `proof/binary-path.txt` exists and names an executable `mct-daemon`.

Two drives may share `target/debug/mct-daemon`. They must not share `VERIFY_PATINA_MCT_ROOT`. The helper rejects a root inside the home directory.

## Doctor

Run doctor before the first drive, and again after a failed drive, before you retry.

```sh
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh doctor
```

Doctor requires the binary from Launch. It runs `mct-daemon version` and requires the line `mct-daemon 0.2.0`. A missing binary, a different version line, or a binary inside the home directory means this instance is not worth driving.

## Drive

Drive the admin CLI with the helper. The recipe for that feature is [features/mother-admin-cli.md](features/mother-admin-cli.md).

```sh
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh drive-admin
```

`drive-admin` rebuilds the binary, runs doctor, then exercises help, day-key generation, genesis preparation, the macOS lifecycle refusal, a missing control socket, and an unknown command. It passes explicit paths under the run root. It does not start `serve-iroh`, open the default browser, or submit a signed bootstrap.

Repository gates, release rejection, and the Nix smoke are separate drives. Run them from the checkout.

```sh
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh gates
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh release-artifact
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh nix-smoke
```

`gates` runs, in order, `cargo fmt --all --check`, `cargo clippy -- -D warnings`, `cargo test -- --test-threads=1`, `bash scripts/ci-tier0.sh specs`, and `git diff --check`. One failure does not skip the later checks. The command exits nonzero when any of those five exit nonzero. `cargo test` here is the workspace test run with one test thread, because resident tests own real sockets. See [features/repository-gates.md](features/repository-gates.md).

`release-artifact` proves the rejection paths of `scripts/verify-release-artifact.sh`. A passing verification of a real archive is manual. See [features/release-artifact.md](features/release-artifact.md).

`nix-smoke` runs `nix run .#mother-install-smoke` and returns that process's exit code. The flake builds only `aarch64-linux`. On any other host, record the exit code as out of VM. Do not treat a missing `nix` binary or a platform error as a passing smoke. See [features/linux-admin-mother.md](features/linux-admin-mother.md).

## Evidence

Proof stays under `$VERIFY_PATINA_MCT_ROOT/proof`. Capture the command, the exit code, and the resulting state.

| File | What it holds |
| --- | --- |
| `drive-results.tsv` | Admin CLI checks, exit codes, and the behavior each one proves |
| `day-keygen.stdout` | Public half only. The secret file is not proof. |
| `prepare-genesis.json` | Challenge and intent. This file is not a committed ledger. |
| `gates-results.tsv` and `gates/*.log` | Gate name, exit code, and the full log |
| `release-results.tsv` | Usage and rejection exits |
| `nix-host.txt` and `nix-results.tsv` | Host arch and the Nix exit code |

A day-key secret, a pin file, and a bogus archive live under `scratch/` and are not proof. Do not copy them into the proof directory or into a pull request. Do not print secret file contents.

The admin drive proves observable CLI behavior. It does not prove a 1Password ceremony, a systemd unit, or a signed release. When a feature file lists another entry point, a run that used one path does not verify the others.

## Cleanup

Cleanup deletes `scratch/` for the run root and leaves `proof/` in place. It refuses to run when `proof/` is already missing, so a cleanup cannot invent an empty proof directory.

```sh
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh cleanup
test -d "$VERIFY_PATINA_MCT_ROOT/proof"
test ! -e "$VERIFY_PATINA_MCT_ROOT/scratch"
```

Run cleanup after a failed drive too. Kill only a process you started. The helper does not start a resident Mother. If you start `passkey-admin serve-iroh` yourself, stop that process by the pid you recorded. Do not kill by process name.

## Helpers

Both scripts are executable.

```sh
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh launch
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh doctor
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh drive-admin
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh gates
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh release-artifact
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh nix-smoke
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh cleanup
.cursor/skills/verify-patina-mct/scripts/check-skill.sh
```

`check-skill.sh` checks frontmatter, the feature headings, the index links, and the forbidden browser and home-directory tokens. It does not drive the CLI.

## Manual steps

These steps need a person or a machine this skill does not substitute.

- The 1Password registration, assertion, and approval taps follow [docs/passkey-admin.md](../../../docs/passkey-admin.md) and [docs/passkey-approval-RUN.md](../../../docs/passkey-approval-RUN.md). The operator uses the default browser. Leave the step marked manual when that tap did not happen. See [features/passkey-browser.md](features/passkey-browser.md).
- The Raspberry Pi admin install is the record in [docs/nicanode0-install-evidence.md](../../../docs/nicanode0-install-evidence.md). The earlier nicamaster record is [docs/linux-admin-install-evidence.md](../../../docs/linux-admin-install-evidence.md). The operator command is in [docs/linux-admin-install.md](../../../docs/linux-admin-install.md).
- macOS `install`, `start`, `stop`, `restart`, `uninstall`, and `scripts/release-local.sh smoke` need Darwin arm64. On any other host, `mct-daemon install` must fail before it writes supervisor state.
- A passing `scripts/verify-release-artifact.sh` run needs a real `mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz` and the Darwin arm64 signature adapter.

## Maintenance

When commands or the feature map change, run `/maintain-verification-skill` and correct this directory from a live drive. Do not edit product code from that pass.

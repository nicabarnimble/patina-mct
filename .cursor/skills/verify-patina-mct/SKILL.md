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

`RUN_ID` is any new token. The root must be a nonexistent absolute path with an existing parent outside the canonical home directory. Node.js 18 or newer is required. The helper canonicalizes the path, creates a private root with an ownership marker, and creates `scratch/` and `proof/` at mode 0700. Launch is finished when `proof/binary-path.txt` names an executable `mct-daemon`.

Two drives may share `target/debug/mct-daemon`. They must not share `VERIFY_PATINA_MCT_ROOT`. The helper refuses home state through symlink and `..` aliases. It refuses unmarked roots and symlinks or hard-linked files below a run root before writes or deletion. This is an operator safety check, not protection from a hostile process with the same UID.

## Doctor

Run doctor before the first drive, and again after a failed drive, before you retry.

```sh
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh doctor
```

Doctor requires the binary from Launch. It runs `mct-daemon version` and requires the line `mct-daemon 0.2.0`. A missing binary or a different version line fails doctor. The checkout and binary may live inside the home directory. Only writable run state must stay outside it.

## Drive

Drive the admin CLI with the helper. The recipe for that feature is [features/mother-admin-cli.md](features/mother-admin-cli.md).

```sh
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh drive-admin
```

`drive-admin` rebuilds the binary, runs doctor, then exercises help, day-key generation, genesis preparation, a missing control socket, and an unknown command. On non-Darwin hosts it also checks the lifecycle refusal. On Darwin it records `install` as `manual` with status `not-run` and never invokes it. State-bearing admin calls use explicit paths under the run root. It does not start `serve-iroh`, open the default browser, or submit a signed bootstrap.

Repository gates, release rejection, and the Nix smoke are separate drives. Run them from the checkout.

```sh
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh gates
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh release-artifact
.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh nix-smoke
```

`gates` runs, in order, `cargo fmt --all --check`, `cargo clippy -- -D warnings`, `bash scripts/ci-tier0.sh test`, `bash scripts/ci-tier0.sh specs`, and `git diff --check`. One failure does not skip the later checks. The command exits nonzero when any of those five exit nonzero. The Tier-0 test wrapper canonicalizes `TMPDIR`, sets a private umask, and runs workspace tests with one test thread because resident tests own real sockets. See [features/repository-gates.md](features/repository-gates.md).

`release-artifact` proves the rejection paths of `scripts/verify-release-artifact.sh`. A passing verification of a real archive is manual. See [features/release-artifact.md](features/release-artifact.md).

`nix-smoke` checks the host before invoking Nix. Unsupported hosts return `2` as `out-of-vm` without running the installer. A supported host without Nix returns `127` in that class. On `aarch64-linux` with Nix, it runs `nix run .#mother-install-smoke`, records `auto`, and returns the process exit code unchanged. Do not count a skipped smoke as a pass. See [features/linux-admin-mother.md](features/linux-admin-mother.md).

## Evidence

Proof stays under `$VERIFY_PATINA_MCT_ROOT/proof`. Capture the command, the exit code, and the resulting state.

| File | What it holds |
| --- | --- |
| `drive-results.tsv` | Four tab-separated fields per row. Check, classification, exit code or `not-run`, and detail. |
| `day-keygen.stdout` | Public half only. The secret file is not proof. |
| `prepare-genesis.json` | Challenge and intent. This file is not a committed ledger. |
| `gates-results.tsv` and `gates/*.log` | Gate name, exit code, and the full log |
| `release-results.tsv` | Usage and rejection exits |
| `nix-host.txt` and `nix-results.tsv` | Host, classification, and helper or Nix exit code |

A day-key secret, a pin file, and a bogus archive live under `scratch/` and are not proof. Do not copy them into the proof directory or into a pull request. Do not print secret file contents.

The admin drive proves observable CLI behavior. It does not prove a 1Password ceremony, a systemd unit, or a signed release. When a feature file lists another entry point, a run that used one path does not verify the others.

## Cleanup

Cleanup validates the ownership marker and directory tree, deletes `scratch/`, and leaves `proof/` and the marker in place. Repeated cleanup is safe. It refuses unmarked roots, unsafe links, and missing proof instead of adopting or repairing an unrelated directory.

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

`check-skill.sh` checks frontmatter, headings, links, and Pi discovery. It runs behavioral regression tests through the helper with disposable roots and fake subprocesses. The tests cover home aliases, cleanup ownership, links, portable permission checks, Darwin install refusal, TSV fields, gate exits, and Nix classifications. They never run a real installer and do not replace a real CLI drive.

## Manual steps

These steps need a person or a machine this skill does not substitute.

- The 1Password registration, assertion, and approval taps follow [docs/passkey-admin.md](../../../docs/passkey-admin.md) and [docs/passkey-approval-RUN.md](../../../docs/passkey-approval-RUN.md). The operator uses the default browser. Leave the step marked manual when that tap did not happen. See [features/passkey-browser.md](features/passkey-browser.md).
- The Raspberry Pi admin install is the record in [docs/nicanode0-install-evidence.md](../../../docs/nicanode0-install-evidence.md). The earlier nicamaster record is [docs/linux-admin-install-evidence.md](../../../docs/linux-admin-install-evidence.md). The operator command is in [docs/linux-admin-install.md](../../../docs/linux-admin-install.md).
- macOS `install`, `start`, `stop`, `restart`, `uninstall`, and `scripts/release-local.sh smoke` need Darwin arm64. On any other host, `mct-daemon install` must fail before it writes supervisor state.
- A passing `scripts/verify-release-artifact.sh` run needs a real `mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz` and the Darwin arm64 signature adapter.

## Maintenance

When commands or the feature map change, run `/maintain-verification-skill` and correct this directory from a live drive. Do not edit product code from that pass.

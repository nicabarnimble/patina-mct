# patina-mct verification map

This directory is the maintained source for verifying operator-facing patina-mct behavior. Read the index before driving the checkout, then use the matching feature file as the recipe.

## Baseline preconditions

- Use a checkout of patina-mct with Rust 1.96.0 from `rust-toolchain.toml` and Node.js 18 or newer.
- Set `VERIFY_PATINA_MCT_ROOT` to a nonexistent absolute path with an existing parent outside the home directory, for example `/tmp/verify-patina-mct-$RUN_ID`. Let the helper create its marker. Do not pre-create or share the root.
- Put `.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh` on the command line by path. Do not rely on `PATH`.
- Run `verify-patina-mct.sh doctor` after launch and require `mct-daemon 0.2.0`.
- Never drive a run directory you did not create for this verification, and never reuse a directory inside the home directory.

## Driving conventions

- Start every recipe from the baseline state unless its preconditions say otherwise.
- Treat every command as literal. Keep quoted flags unchanged.
- Pass explicit `--root`, `--uds`, and `--secret-output` paths under the run directory for the admin CLI drive. The native Nix install smoke owns separate host state as its recipe describes.
- Run admin CLI actions through `verify-patina-mct.sh drive-admin`.
- Run repository gates through `verify-patina-mct.sh gates`.
- Restore nothing in the operator's home directory. Delete only this run's `scratch/`. Do not remove `proof/` during cleanup.

## Proof and skip reporting

- Capture the operator action and the resulting state, not only a final summary line.
- CLI proof includes the command, stdout, stderr, and exit code. Secret file bytes are not proof.
- Mutation proof includes a second look at the filesystem. Genesis preparation must leave no ledger file behind.
- Record the feature id and the entry point with every artifact.
- Report an unreachable path with the attempted command and the unmet precondition.
- Do not report a skipped entry point as verified through a different path.
- A manual or out-of-VM step stays in that class until an operator runs it on the required machine.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the operator-visible behavior. It then uses exactly four H2 sections in this order.

1. `Sub-features` lists short ids with one line for each behavior.
2. `How to get to it (user POV)` lists every operator entry point.
3. `Driving it with verify-patina-mct` starts with `Preconditions:` and uses labeled bullets that pair each action with an exact command and observable result.
4. `Gotchas` lists traps that can waste or invalidate a verification run.

Keep implementation essays out of the map. Name operator paths, flags, required state, commands, and observable proof.

## Features

- [Mother admin CLI](./mother-admin-cli.md) covers help, day-key generation, genesis preparation, the macOS lifecycle refusal, and a missing control socket.
- [Repository gates](./repository-gates.md) covers formatting, Clippy, tests, tier-0 specs, and whitespace checks, and states what each one proves.
- [Release artifact](./release-artifact.md) covers rejection of a bad archive name and the manual Darwin verification.
- [Linux admin Mother](./linux-admin-mother.md) covers the Nix install smoke and the Raspberry Pi records that an unattended run must not fake.
- [Passkey browser ceremony](./passkey-browser.md) covers the default-browser 1Password taps that remain manual.

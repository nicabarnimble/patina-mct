# Mother admin CLI

The admin CLI lets an operator read the command list, mint a day-key file, prepare a passkey genesis challenge, and see a missing Mother fail closed. Signed bootstrap stays with the operator.

## Sub-features

- `admin-help` prints the command list, including `passkey-admin`.
- `admin-day-keygen` writes a new private secret and prints only the public half.
- `admin-day-keygen-refuse` refuses to replace an existing secret file.
- `admin-prepare-genesis` prints a bootstrap challenge and writes no ledger.
- `admin-install-host` refuses the macOS supervisor lifecycle on any other host.
- `admin-status-absent` fails closed when the named control socket is absent.
- `admin-unknown` rejects an unknown command.

## How to get to it (user POV)

- Run `mct-daemon help`.
- Run `mct-daemon version`.
- Run `mct-daemon passkey-admin day-keygen --secret-output <file>`.
- Run `mct-daemon passkey-admin prepare-genesis` with an explicit root, Mother id, ledger id, and registered pin.
- Run `mct-daemon install` on a non-macOS host and expect a refusal.
- Run `mct-daemon status --uds <socket> --json` against a socket that is not there.
- Run `mct-daemon passkey-admin serve-iroh` only after a real bootstrap, on a machine you control. The helper does not take this entry point.

## Driving it with verify-patina-mct

Preconditions:

- `VERIFY_PATINA_MCT_ROOT` is a new absolute directory outside the home directory.
- `verify-patina-mct.sh doctor` reports `mct-daemon 0.2.0`.
- The registered pin is the public `root` object in `scripts/fixtures/passkey-admin-challenge.json`, written as its own mode 0600 file. It is a software test pin, not a 1Password credential.

- **Launch and drive.** Build the binary and run the admin checks. Run `.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh drive-admin`. Exit code `0`. `proof/drive-results.tsv` lists `help`, `day-keygen`, `day-keygen-replace`, `prepare-genesis`, `install`, `status-absent`, and `unknown-command`.
- **Public half.** Confirm the day-key result without printing it. `proof/day-keygen.stdout` is JSON with `scheme` equal to `ed25519` and a 64-character `public_key`. The secret file is gone after the drive. A second `day-keygen` to the same path exits nonzero and says `refusing to replace an existing day-key secret`.
- **Genesis challenge.** Confirm preparation did not commit authority. `proof/prepare-genesis.json` has `intent.action.action` equal to `bootstrap`, `intent.mother_node_id` equal to `mother-verify`, and a `challenge`. `scratch/mother` contains no ledger file.
- **Lifecycle refusal.** Confirm install did not write supervisor state. `install` in `drive-results.tsv` has a nonzero exit, and stderr contains `supervisor lifecycle is supported only on macOS`.
- **Missing socket.** Confirm status did not start a resident. `status-absent` has a nonzero exit, and stderr contains `resident not running at verified control socket`.
- **Proof.** Keep `proof/drive-results.tsv` and the captured stdout and stderr files. Do not copy the secret or the pin into the proof directory.

## Gotchas

- `day-keygen` will not overwrite a secret. Delete the scratch file in cleanup, then mint again. Do not point the flag at a file you care about.
- `prepare-genesis` is not bootstrap. A challenge without a signed request leaves a virgin root. Do not describe that output as an enrolled Mother.
- The fixture pin is public test data. It does not prove 1Password, UV, or a live origin.
- `mct-daemon install` checks the host before it reads paths. A nonzero exit on Linux is the proof. A zero exit on Linux is a failure of the check.
- `status` without `--uds` uses a relative `.mct/control.sock` under the current directory. Always pass the socket path.
- Intents expire after at most five minutes. A later manual sign step must prepare again after any delay or any authority change.
- `serve-iroh` is a resident process. The helper does not start it, and a verification run must not kill every `mct-daemon` on the machine.

# Governed profile (D-58 passkey Mother running Children)

Built in [NIC-126](https://linear.app/nicabar/issue/NIC-126). The C2 probes in
`bash scripts/federation/suite.sh done` prove this path on macOS with a software
ES256 root and Ed25519 day key through the production adapter. Two steps need a
person or hardware and stay pending: Doug's live 1Password tap, and the Pi run.

## What is built

- A passkey-rooted ledger passes the startup gate. Its Child authority comes only
  from four signed commands, each bound to one scope:

  | Route | Scope | Body |
  | --- | --- | --- |
  | `/children/approve` | `approve_child` | `child_name`, `artifact_id` and `manifest_hash` (`sha256:<hex>`), `artifact_version`, `vision_id`, `policy_revision` |
  | `/children/revoke` | `revoke_child` | `child_name` |
  | `/toys/grant-directory-read` | `watch_grant` | `grant_id` (never reused), `child_name`, `canonical_root` (`file:///abs`), `expires_at` (mandatory), optional `caller_node_id` |
  | `/toys/revoke` | `watch_revoke` | `grant_id` |

- The command fact is the change. Approvals and grants record the signer
  (`ed25519:<day key>`), and only the passkey root can issue a day key.
- Every Unix-socket mutation (`governed_unix_socket_mutation_refused`) and every
  offline mutation (`governed_offline_mutation_refused`) is refused before any effect.
- Not built yet: resident startup finalize and serve on a passkey ledger
  (`governed-resident-boot-not-wired`), and governed peer and artifact-stage commands.

## Live 1Password tap (Doug, macOS)

Use a new private root (`umask 077`) and the HTTPS page from
[passkey-admin.md](../passkey-admin.md). The runtime reads `local-mct` / `ledger-local`.

```sh
ROOT=$HOME/.mct-governed; M="--root $ROOT --mother local-mct --ledger-id ledger-local"
# 1. registerRoot(rpId, userName) in the browser; save the pin as PIN.json.
mct-daemon passkey-admin prepare-genesis $M --registered-root PIN.json > genesis.json
# 2. signPrepared(genesis) in the browser (tap 1) -> GENESIS-SIGNED.json
mct-daemon passkey-admin bootstrap $M --registered-root PIN.json --request GENESIS-SIGNED.json
mct-daemon passkey-admin day-keygen --secret-output OPS-SECRET.json      # prints PUBLIC_HALF
# 3. ISSUE.json: {"action":"issue_day_key","grant_id":"ops","public_half":PUBLIC_HALF,
#    "scopes":["approve_child","revoke_child","watch_grant","watch_revoke"],"expires_at":"<within 24 h>"}
mct-daemon passkey-admin prepare $M --action ISSUE.json --signer root > issue.json
# 4. signPrepared(issue) in the browser (tap 2) -> ISSUE-SIGNED.json
mct-daemon passkey-admin apply $M --request ISSUE-SIGNED.json
# 5. For each governed command, CMD.json is {"action":"command","scope":"approve_child",
#    "route":"/children/approve","body":"<the body above, JSON-encoded as a string>"}
mct-daemon passkey-admin prepare $M --action CMD.json --signer ops > cmd.json
jq .intent cmd.json > cmd-intent.json
mct-daemon passkey-admin sign-day --intent cmd-intent.json --grant ops --secret OPS-SECRET.json > CMD-SIGNED.json
mct-daemon passkey-admin apply $M --request CMD-SIGNED.json
```

Pass iff:
- The approve and grant replies report `"children":1` and `"toy_grants":1`.
- A grant body without `expires_at` fails with `MissingExpiry`, and
  `shasum $ROOT/observations.jsonl` is unchanged.
- `mct-daemon toys authorize-secret ... --ledger $ROOT/observations.jsonl` fails
  with `governed_offline_mutation_refused`.

## Pi run (NIC-129, after governed boot)

On nicanode0, use only the installed closure. Run no cargo or nix builds on the Pi.
Run steps 1 to 5 above with `mct-daemon` from the closure and a fresh root on the Pi.
Run `prepare` on the Pi, sign on the Mac (browser for root actions, `sign-day` for
`ops`), copy the signed JSON back, and `apply` on the Pi. Then, once
`governed-resident-boot-not-wired` is closed, start the governed Mother and run:

```sh
bash scripts/federation/suite.sh done /absolute/new/evidence-directory
```

Pass iff C1, C3, C5 and C6 move from tracked declared rows to observed passes in
one governed-profile run, with sanitized evidence checked in.

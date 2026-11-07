# Governed profile (D-58 passkey Mother running Children)

Built in [NIC-126](https://linear.app/nicabar/issue/NIC-126). The C2 probes in
`bash scripts/federation/suite.sh done` prove this path on macOS with a software
ES256 root and Ed25519 day key through the production adapter. Two steps need a
person or hardware and stay pending: Doug's live 1Password tap, and the Pi run.

## What is built

- `mct-daemon serve` boots one resident Mother on a passkey-rooted ledger. On that
  profile the startup readiness proof is the verified replay. The same process
  serves `mct/hello/0`, `mct/call/0` and `mct/admin/1`, and every one of them
  goes through one ledger writer.
- Signed commands are the only authority. Each is bound to one scope, and a day-key
  lease must list its scopes in ascending order.

  | Route | Scope | Body |
  | --- | --- | --- |
  | `/artifacts/stage` | `artifact` | an operator-pointed stage request pinned to `expected_digest` and `expected_manifest_digest` |
  | `/children/approve` | `approve_child` | `child_name`, `artifact_id` and the **installed** `manifest_hash` (`sha256:<hex>` of `children/artifacts/sha256/<id>/child.toml`), `artifact_version`, `vision_id`, `policy_revision` |
  | `/children/revoke` | `revoke_child` | `child_name` |
  | `/toys/grant-directory-read` | `watch_grant` | `grant_id` (never reused), `child_name`, `canonical_root` (`file:///abs`), `expires_at` (mandatory), optional `caller_node_id` |
  | `/toys/revoke` | `watch_revoke` | `grant_id` |
  | `/peers/add` | `peer_add` | `peer_node_id`, `binding_id`, `endpoint_id`, `vision_id`, `policy_revision`, `expires_at` (mandatory), optional `ticket` |
  | `/peers/revoke` | `peer_revoke` | `peer_node_id` |

- At boot, and after each completed governed admin command, approvals and peers
  are projected into `config.json` for the hello, publication and forwarding
  readers. The execution snapshot keeps only peers the ledger names identically.
  Those readers trust the projection, the same as other same-UID host state
  (see `docs/SECURITY-BOUNDARIES.md`).
- Every Unix-socket mutation (`governed_unix_socket_mutation_refused`) and every
  offline mutation (`governed_offline_mutation_refused`) is refused before any
  effect, and the ledger bytes are unchanged.

## Known limitation, not accepted by Doug

An unkeyed call whose caller deadline is more than 12 minutes away is refused
(`UnkeyedDeadlineBeyondFence`). This keeps the D-40 single-launch fence sound.
Unkeyed callers must send a nearer deadline or an idempotency key. The cap is
recorded as a limitation pending Doug's decision. It is not a ratified policy.

## Live 1Password tap (Doug, macOS): pending

Every command below ran end to end in `crates/mct-daemon/tests/governed_boot.rs`,
with a software ES256 key standing in for the passkey. The browser
`signPrepared` step with 1Password has **not** been run. Prerequisites:
- the HTTPS page and registration from [passkey-admin.md](../passkey-admin.md);
- a new owner-private root (`umask 077`);
- the ids `local-mct` / `ledger-local`, which the runtime reads.

```sh
ROOT=$HOME/.mct-governed; M="--root $ROOT --mother local-mct --ledger-id ledger-local"
mct-daemon passkey-admin prepare-genesis $M --registered-root PIN.json > genesis.json
#   browser: signPrepared(genesis) with 1Password -> GENESIS-SIGNED.json
mct-daemon passkey-admin bootstrap $M --registered-root PIN.json --request GENESIS-SIGNED.json
mct-daemon passkey-admin day-keygen --secret-output OPS-SECRET.json > ops-public-half.json
#   ISSUE.json: {"action":"issue_day_key","grant_id":"ops","public_half":<ops-public-half.json>,
#     "scopes":["approve_child","artifact","peer_add","peer_revoke","revoke_child","watch_grant","watch_revoke"],
#     "expires_at":"<RFC 3339, at most 24 h ahead>"}
mct-daemon passkey-admin prepare $M --action ISSUE.json --signer root > issue.json
#   browser: signPrepared(issue) with 1Password -> ISSUE-SIGNED.json
mct-daemon passkey-admin apply $M --request ISSUE-SIGNED.json
#   per command: CMD.json = {"action":"command","scope":"<scope>","route":"<route>","body":"<JSON body as a string>"}
mct-daemon passkey-admin prepare $M --action CMD.json --signer ops > cmd.json
jq .intent cmd.json > cmd-intent.json
mct-daemon passkey-admin sign-day --intent cmd-intent.json --grant ops --secret OPS-SECRET.json > CMD-SIGNED.json
mct-daemon passkey-admin apply $M --request CMD-SIGNED.json
#   order: /artifacts/stage, /children/approve (installed manifest hash), optional /peers/add
mct-daemon serve --identity $ROOT/identity/iroh-secret.hex --config $ROOT/config.json \
  --children-dir $ROOT/children --state $ROOT/state.sqlite --ledger $ROOT/observations.jsonl \
  --uds $ROOT/control.sock
```

Pass iff:
- `GET /status` on `$ROOT/control.sock` reports `"readiness":"ready"`.
- A `POST /calls` to the approved Child completes.
- A `POST /children/approve` over the socket returns 403
  `governed_unix_socket_mutation_refused`.

## Pi run (NIC-129): pending

On nicanode0, use only the installed closure. Run no cargo or nix builds on the
Pi. Run the sequence above with the Pi as `$ROOT`. Run `prepare` on the Pi, sign
on the Mac (browser for root actions, `sign-day` for `ops`), copy the signed JSON
back, and `apply` on the Pi. Then start `mct-daemon serve` there and run:

```sh
bash scripts/federation/suite.sh done /absolute/new/evidence-directory
```

Pass iff C1, C3, C5 and C6 move from tracked declared rows to observed passes in
one governed-profile run, with sanitized evidence checked in.

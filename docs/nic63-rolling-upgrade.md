# NIC-63 rolling binary replacement

**PASS. Completed 2026-11-04 at 18:12:23 UTC in the one-command suite.**

After the [three-host crash/partition pass](nic63-slice3-three-host.md), three
fresh test roots started the older Mother binary. Nicamaster (`a`), nicanode0
(`b`), and nicaNode1 (`c`) were replaced one at a time.

## Distinct artifacts

| Artifact | Source revision | Mother SHA256 |
| --- | --- | --- |
| Baseline | `aa87237008ea2508ea125128b312fc269c67444f` | `fc1a53e5bb3ef798f4955538491f65243b7568c473ea814265f40fb0a3d8bde4` |
| Candidate bundle | `d9340ae7bcde339ff7418b1841211841616401f6` | `66b4dff183c36fccfc5d3d6c37b3a5b8458f898a318bbe243e6dab8e82e3cfd3` |

The cached baseline package was
`/nix/store/43c376cj22mfa100nwfivkcki38iirkh-mct-admin-mother-0.2.0-nic53`.
Its recorded derivation uses `/nix/store/sin0vqh71vg5xf4bsaf1jd161scxqmzl-source`.
Every tracked file in its `Cargo.toml`, `Cargo.lock`, `crates/`, `fuzz/corpus/`,
and `scripts/fixtures/` source set matched that historical revision's SHA256
manifest. It was reused, not rebuilt on targets.
Its Wasmtime dependency is 48.0.8; the candidate uses 49.0.5.

The candidate package and helper match the three-host slice. All candidate Rust
compilation and fixture generation occurred on nicamaster. Each target verified
its imported signed closure offline. The candidate is a test-profile binary,
not a production release installation.

## Observed gates

Before any Mother started, each owned helper service had its actual running
cgroup and directly attached `sd_fw_ingress` and `sd_fw_egress` programs verified.
The service allowed only the three configured LAN `/32` addresses and denied
other IPv4 and IPv6 destinations/sources. No configured peer was a default
gateway. This mattered because the older baseline still contains discovery and
router-mapping code. The fence restricts IP traffic; this is not a claim that the
old binary made no discovery attempts.

| Gate | Result |
| --- | --- |
| Baseline on all hosts | Actual started-binary hashes matched the baseline |
| One replacement at a time | A, then B, then C; each victim drained before replacement |
| Survivors during replacement | Both remaining Mothers completed remote WASM calls while the victim was stopped |
| Mixed-version operation | Full call matrix passed after each replacement, including candidate/older peers |
| Identity continuity | Each restarted Mother retained its original endpoint key |
| Ledger continuity | Each exact prior byte prefix survived replacement; all final ledgers verified |
| Final revocation | B revoked A; A's next call was denied for `BindingRevoked`, without execution |
| Cleanup | Graceful stops, no escalation; owned transient services inactive/absent and scratch executables removed |

There were 42 completed calls and one denial. Completed calls included 30 remote
executions and 12 local controls. This was actual replacement between distinct
binaries, not a same-binary restart.

| Mother | Ledger entries | Head sequence | Ledger SHA256 |
| --- | ---: | ---: | --- |
| A | 366 | 365 | `98311ffe9f5b9e27818d3e302ecc723aea84b9479cb3807f075c18fa0674812a` |
| B | 360 | 359 | `c901c26ef62b5f961e8d20b91efc263b005c6129fc1bb103d9cd054da75f53ba` |
| C | 358 | 357 | `89dfb8fedece5ca35e944e6b98eb84c260b7a2d22f2b7ec215dda1c9dca3b3f0` |

## Evidence and limits

- [Sanitized result, correlations, keys, and verification receipts](evidence/nic63-upgrade/result.json).
- [Timestamped stages, including per-host fence proofs](evidence/nic63-upgrade/stages.jsonl).
- Run `fleet-0bfaae2f-d2f2-44ce-9ff9-1d0c3e7479ac`.

Raw canonical ledgers and logs remain private. Sanitized reductions are not
replacement ledgers and cannot independently reproduce their canonical hashes.
The stricter post-run markerless-caller execution check also accepted this run.

This is harness-managed Linux replacement, not the product's macOS upgrade CLI,
a version rollback, or a claim of universal release compatibility. Physical
Proxmox, host reboot, and power-cut acceptance remain untested. Static LAN
addresses plus persisted keys do not establish changed-address key-only discovery.

## OCI-118 follow-up

The reusable [one-command harness](../scripts/federation/README.md) can be pointed
at another explicit host set. These passes do not remove the need for
authenticated address refresh and expiry, discovery after an address change,
identity rotation/revocation, and lifecycle ownership on future deployment
platforms. Those remain separate key/address-management work.

Refs NIC-63

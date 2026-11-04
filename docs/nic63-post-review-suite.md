# NIC-63 post-review physical acceptance

**PASS. All three slices passed consecutively; the final slice completed on
2026-11-04 at 20:12:10 UTC.** The suite command exited zero.

This run used source `2f3ffe166720446938781eff1d265fc5aca35315`, rebased onto
main `4182ca00dd00a05fb20e10e9a33ac39982afd59b` after dependency PRs #100,
#101, and #102. It includes the [SQLite-sidecar correction](nic63-sqlite-sidecar.md).
The earlier [18:12 suite](nic63-acceptance-suite.md) remains historical evidence;
its old binary was not used to infer acceptance of this source.

```sh
bash scripts/federation/suite.sh \
  /absolute/private/fleet.json /absolute/private/new-suite-evidence
```

The private inventory was unchanged except for the candidate package. The same
three independently identified physical hosts were used. Each slice created fresh
private roots and identities; continuity claims remain within each slice.

## Consecutive results

| Slice | Completed UTC | Remote / local completed calls | Other expected outcomes | Verified ledgers |
| --- | --- | --- | --- | ---: |
| [Two hosts](evidence/nic63-postreview/two/result.json) | 20:04:19 | 2 / 2 | One receiver revocation denial | 2 |
| [Three hosts](evidence/nic63-postreview/three/result.json) | 20:09:47 | 57 / 12 | Three blocked transport probes; one revocation denial | 3 |
| [Rolling replacement](evidence/nic63-postreview/upgrade/result.json) | 20:12:10 | 30 / 12 | One receiver revocation denial | 3 |

The [aggregate receipt](evidence/nic63-postreview/result.json) records success.
Stage receipts are retained for [two hosts](evidence/nic63-postreview/two/stages.jsonl),
[three hosts](evidence/nic63-postreview/three/stages.jsonl), and
[replacement](evidence/nic63-postreview/upgrade/stages.jsonl).

Every three-host victim had a reaped SIGKILL exit signal **9**. Survivor service,
same-key restart, and exact checkpoint-prefix preservation passed. Per-victim UDP
drop counters were **16 / 16 / 23**. Blocked probes had the exact transport failure
and forwarding correlation, with no correlated execution. Survivors served while
the probes waited; policy restoration and healed calls passed.

Replacement used distinct baseline/candidate hashes, mixed-version calls,
survivor service, stable keys, and within-slice ledger continuity. Actual directly
attached kernel ingress/egress filters were verified before each legacy Mother
started. This is not a claim that the legacy binary attempted no discovery.

## Artifact provenance

- Source: `2f3ffe166720446938781eff1d265fc5aca35315`.
- Source tree: `2ef075c91dc44e40a772f5d35feda10148efcec6`.
- Candidate package: `/nix/store/5xcpx03752xzggmcqc95y9mp46iaxssa-mct-federation-test-bundle`.
- Mother SHA256: `17489cd1286d9acf9b89a37df2f374b780c3200d4026850711af5606ed41ddc1`.
- Helper SHA256: `4ac27df777c0fd74c9e6c275fd95355980ccb6c796a15b62cb1bf9273f91b92e`.
- Baseline Mother SHA256: `fc1a53e5bb3ef798f4955538491f65243b7568c473ea814265f40fb0a3d8bde4`;
  unchanged historical source `aa87237008ea2508ea125128b312fc269c67444f`.

All Rust/package builds and fixture generation ran on nicamaster. The builder
started from a preview merge tree; the actual rebased tree and a 213-file manifest
of Rust, lockfile, fixture, and packaging inputs matched exactly before artifact
acceptance. Package provenance records that verified revision and manifest. The
test-only profile has no debug information; this is not a production release
installation.

The first combined build encountered a read-only generated `libsqlite3-sys`
bindings file left in the shared build cache. Only that verified, agent-owned
generated file was removed. The successful rebuild and 16 helper tests supplied
the package; production storage permissions and source files were not repaired.
Targets imported signed closures and verified content/signatures offline.

## Verification and cleanup

- 193 controller tests passed on this source; mocked wrapper tests are not
  physical acceptance.
- 16 native helper tests passed with the new dependencies.
- Fresh combined-source [CI](https://github.com/nicabarnimble/patina-mct/actions/runs/41899235332)
  and [bounded fuzz](https://github.com/nicabarnimble/patina-mct/actions/runs/41899235285)
  passed on their first attempts. The original CI failure and the correction's
  bounded evidence are documented separately; its exact original predicate
  remains unproven.
- All eight raw canonical ledger copies matched the SHA256 values below.
  Saved-observation audits passed for both private reports and published reductions.
- Independent read-only source review of the storage correction returned
  PASS+NOTES. Its four reviewed source hashes survived the rebase unchanged.
- All three final host audits found no test Mother/helper, owned helper/rollback
  unit, owned nftables table, executable scratch directory, or delivery-cache/import
  directory. Secret identity material, inventory, raw ledgers, and boot logs remain
  private. Public ephemeral endpoint IDs are part of the reviewed receipts.

| Slice / node | Entries / head sequence | Raw ledger SHA256 |
| --- | --- | --- |
| Two / A | 70 / 69 | `2810b4215aa55ce9a07557779c952fa910990103d2dbf9fb4e2dfc40a97e7c38` |
| Two / B | 64 / 63 | `707441411c24a87056d26607d1342a7d934e2408fec95cb9b639baa5cb57e1e4` |
| Three / A | 651 / 650 | `d2fdfc1dc4532583fb75b3fd403351aac0c8834df99221b00d1bcf6d9b08a62d` |
| Three / B | 628 / 627 | `13a0cb312c2c83ebcdb8b572f316ac037ccb0f23e80769412aed948a560109db` |
| Three / C | 609 / 608 | `5ba11965fe96f9475cf32565feee3de77e17afcaeec33280f6a0901c26a2dd96` |
| Upgrade / A | 366 / 365 | `6b4befb57a7d82777f18d6c0d9a90bdc6fb8c7f1d1cec4fd2f019b41697e5c46` |
| Upgrade / B | 360 / 359 | `bb27f7eb03cc9a30f26b4a4d0c7938a28ba4d8f204fb09afab691d9a773aa65e` |
| Upgrade / C | 358 / 357 | `8049eb725ed8362913e766440ba717f16907535b7c6054ee1c4506fc35f38edb` |

Published reductions replace the private run-root prefix with `[test-root]` and
omit private inventory and secret keys. They are not replayable canonical ledgers.
Their hashes and heads refer to retained private original bytes.
`canonical_fact_count` counts authority facts, not executions.

## Unchanged limits

This proves Linux Mother process SIGKILL, scoped UDP partitions, and
harness-managed distinct-binary replacement. It does not prove host reboot,
power-loss durability, full-link failure, rollback, the macOS product upgrade
command, or physical Proxmox deployment. Fixed-address key persistence is not
changed-address discovery; OCI-118 still owns that work. Existing routing, DNS,
Tailscale, persistent network/service policy, firmware, host images, and NIC-80
installation/LaunchAgent state were not changed.

Refs NIC-63

# NIC-63 physical acceptance suite

This is the historical 18:12 suite. The [post-review rerun](nic63-post-review-suite.md)
records acceptance after the dependency rebase and SQLite-sidecar correction.

**PASS. The one-command suite completed on 2026-11-04 at 18:12:23 UTC.**

The operator can rerun the complete sequence against another configured Linux
host set with one command:

```sh
bash scripts/federation/suite.sh \
  /absolute/private/fleet.json /absolute/private/new-suite-evidence
```

See the [prerequisites and builder/deployment instructions](../scripts/federation/README.md).
The private inventory selects hosts, expected hostnames, LAN addresses, SSH
identity, builder, candidate package, baseline package, vision, and run root.
The suite stops at the first failed slice. It never reuses an evidence directory.

## Consecutive physical results

| Slice | Completed UTC | Completed calls | Other expected outcomes | Evidence |
| --- | --- | ---: | --- | --- |
| Two hosts | 18:04:47 | 4 | One receiver revocation denial | [Result](evidence/nic63-suite-two/result.json), [stages](evidence/nic63-suite-two/stages.jsonl) |
| Three hosts | 18:10:10 | 69 | Three blocked transport failures; one revocation denial | [Narrative and receipts](nic63-slice3-three-host.md) |
| Rolling replacement | 18:12:23 | 42 | One receiver revocation denial | [Narrative and receipts](nic63-rolling-upgrade.md) |

The earlier [two-host acceptance](nic63-slice2-two-host.md) remains historical
evidence. The full suite repeated it using the final helper. Both directions
executed real remote WASM, both local controls passed, and both independent
ledgers verified. Every slice used fresh private roots; continuity was checked
within each slice, not asserted between independently created test Mothers.

The three-host slice observed SIGKILL exit signal `9` for every victim, survivor
service while that Mother was absent, same-key restart, and preserved checkpoint
prefixes. Real per-victim UDP drop counts were 16, 21, and 22. Each blocked probe
had exact forwarding evidence and no correlated execution. Surviving Mothers
served while the probe waited. The temporary policy was removed before healed
calls passed.

Rolling replacement used distinct baseline/candidate hashes and a full call
matrix after each replacement. Every old-binary service had directly attached
kernel ingress/egress filters verified before Mother startup. Keys and exact
checkpointed ledger prefixes survived each replacement.

## Provenance and checks

- Controller source `38f0bac1989d7a961cce975efffbbbe8e82f1672`.
- Native artifact source `d9340ae7bcde339ff7418b1841211841616401f6`.
- Candidate package `/nix/store/ia51gj5lmgc036j5spxprgv9q1hqkhcy-mct-federation-test-bundle`.
- Mother SHA256 `66b4dff183c36fccfc5d3d6c37b3a5b8458f898a318bbe243e6dab8e82e3cfd3`.
- Helper SHA256 `b08005565217e9d0bc0c33d83c2ebfb4a11b837b22f41f8b2dfac7fc43359318`.
- 193 controller tests passed. The suite-wrapper tests use a mocked Node command;
  they do not supply the physical evidence above.
- 16 native helper tests and targeted clippy passed on nicamaster.
- Independent read-only reviews returned PASS+NOTES after the timeout,
  correlation, markerless execution, and observed-signal corrections.
- All eight final raw ledger copies matched their reported SHA256 hashes.
- The stronger saved-observation audit accepted all three final reports.

All physical-test artifact builds and fixture generation occurred on nicamaster.
Targets imported signed closures in parallel and verified content/signatures
offline. Target runtime WASM compilation is execution, not a Cargo/package build.
No test result substitutes for the physical runs.

## Cleanup and retained evidence

After the suite, all three hosts had no `mct-daemon` or `federation_host` process,
no owned helper/rollback unit, no nftables table, and no executable scratch
directory. Each scenario also verified its owned service cleanup before returning
success. Private identities, inventory, raw canonical ledgers, and boot logs were
retained. No private keys or inventory addresses are published.

Published JSON is a reviewed reduction, not a replayable replacement ledger.
Its canonical heads and source-byte hashes refer to the private original bytes.
Authority `canonical_fact_count` values are not execution counts.

## Bounded claims

These are real Linux Mother process, UDP partition, and harness-managed binary
replacement tests. Host reboot, power loss, full-link failure, version rollback,
the macOS product upgrade command, and physical Proxmox acceptance were not run.

Persisted identity at a fixed LAN address passed. Changed-address discovery,
authenticated address refresh/expiry, key rotation, and future-platform lifecycle
ownership remain OCI-118 work. Existing routing, DNS, Tailscale, persistent
network/service policy, firmware, and host images were not changed.

Refs NIC-63

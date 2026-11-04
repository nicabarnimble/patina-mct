# NIC-63 slice 3: three-host crash and UDP partition

**PASS. Completed 2026-11-04 at 18:10:10 UTC in the one-command suite.**

Nicamaster (`a`), nicanode0 (`b`), and nicaNode1 (`c`) each ran one
independently identified Mother. This slice followed the [two-host pass](nic63-slice2-two-host.md).

## Observed gates

Each Mother was SIGKILLed in turn. The helper reaped and reported exit signal
`9` for A, B, and C; the controller required that signal, not merely a requested
crash mode. Both survivors completed remote WASM calls while that Mother was
absent. The victim restarted with its existing endpoint key, replayed its own
ledger, and rejoined the full call matrix. Every previously checkpointed byte
prefix remained unchanged.

Each victim was then isolated with an owned, temporary nftables table dropping
only test UDP traffic between that victim and its peers. SSH remained available.
The blocked probe overlapped a successful call between survivors.

| Victim | Counted dropped packets | Blocked probe | Survivors served | Policy restored and victim rejoined |
| --- | ---: | --- | --- | --- |
| A | 16 | `failed`, `remote peer unavailable` | Yes | Yes |
| B | 21 | `failed`, `remote peer unavailable` | Yes | Yes |
| C | 22 | `failed`, `remote peer unavailable` | Yes | Yes |

The blocked calls have exact caller/trace-correlated forwarding observations.
No execution occurred under their traces or correlated execution IDs. Aggregate
packet counters alone cannot satisfy this gate. An independent 180-second timer
protected each temporary table's cleanup. After removal, the pre-existing
firewall policy matched its prior digest, excluding volatile counters/handles.

There were 69 completed calls, three blocked transport failures, and one final
receiver-revocation denial. Completed calls included 57 remote executions and
12 local controls. B revoked A; A's next call was denied for `BindingRevoked`
without execution. All three independent ledgers verified.

| Mother | Ledger entries | Head sequence | Ledger SHA256 |
| --- | ---: | ---: | --- |
| A | 651 | 650 | `f3b9348f9aa1e0ae29273ccd4847e8f4acb561ac4e7b64dba964d99941043db2` |
| B | 628 | 627 | `79b2b846c968c5c9bd1640bf4545714d741dca6202a852dbc3d38362cfd2c50f` |
| C | 609 | 608 | `16c39a1c7d379a530e52276c7d531950333af219a1a407ccd9b6e311a421b155` |

## Evidence and artifact

- [Sanitized result, correlations, keys, and canonical verification receipts](evidence/nic63-slice3/result.json).
- [Timestamped stages](evidence/nic63-slice3/stages.jsonl).
- Run `fleet-7831d877-08a1-4ff4-a8c4-b8e80dafe204`.
- Bundle source `d9340ae7bcde339ff7418b1841211841616401f6`.
- Package `/nix/store/ia51gj5lmgc036j5spxprgv9q1hqkhcy-mct-federation-test-bundle`.
- Mother SHA256 `66b4dff183c36fccfc5d3d6c37b3a5b8458f898a318bbe243e6dab8e82e3cfd3`.
- Helper SHA256 `b08005565217e9d0bc0c33d83c2ebfb4a11b837b22f41f8b2dfac7fc43359318`.

Compilation and fixture generation occurred only on nicamaster. Targets imported
signed closures and verified them offline. This is the same Mother binary as
slice 2, with a corrected helper observation window.

The first three-host attempt completed its crash/rejoin cycles but failed when
the helper's 30-second control deadline preempted Iroh's 30-second transport
failure. It did not pass. The corrected helper allows 90 seconds for the hello
and call exchanges plus margin. Protocol failures still require exact forwarding
and transport-failure evidence. All reruns used fresh evidence directories.

A subsequent run passed those gates but omitted the terminating signal from its
crash receipt. Independent review correctly withheld final crash acceptance.
A real failing native regression demonstrated that an already-exited child
could satisfy the old crash request. The helper now rejects missing/dead children
and requires an observed SIGKILL exit; the controller also requires signal `9`.
The corrected standalone rerun and the final whole-suite rerun both passed.

A later test-first checker correction also rejects markerless caller execution
on successful remote calls. Reapplying that stricter checker to the saved physical
observations passed for all three acceptance slices. The published reductions
are not independently replayable canonical ledgers; the raw ledgers, identities,
inventory, and logs remain private. Authority `canonical_fact_count` is not an
execution count.

## Limits

This proves Mother process crash/restart and scoped UDP partition recovery. It
does not prove host reboot, power-loss durability, full-link failure, dynamic
address discovery, or Proxmox support. Keys persisted at fixed configured LAN
addresses. [Rolling replacement](nic63-rolling-upgrade.md) ran next.

Refs NIC-63

# NIC-63 slice 2: two-host LAN federation

**PASS — 2026-11-04, completed 16:53:49 UTC.**

Nicamaster (`a`) and nicanode0 (`b`) ran independently identified Mothers.
The configured LAN sockets and public tickets were exchanged explicitly;
receiver-issued signed binding proofs were admitted through real Iroh hello.
The local-only candidate disables public discovery, relays, router mapping,
and optional internet probes.

## Observed gates

| Gate | Result |
| --- | --- |
| One owned Mother on each physical host | Distinct boot identities; native PID-count check returned one on each host |
| A → B remote WASM | Completed, result `42`, route `remote_peer`, correlated execution in B's ledger only |
| B → A remote WASM | Completed, result `41`, route `remote_peer`, correlated execution in A's ledger only |
| Local controls | A returned `41`; B returned `42` through local WASM routes |
| Independent ledger verification | A: 70 entries, head sequence 69; B: 64 entries, head sequence 63; both verified |
| Revocation | B revoked A; A's very next call was denied, with receiver `BindingRevoked` evidence and no execution under the denied trace |
| Cleanup | Graceful stops without escalation; owned transient helper services confirmed inactive/absent; executable scratch copies removed |

Each ledger records two completed child-execution observations: its own local
control and its execution for the other caller. The verifier's separate
`canonical_fact_count` counts authority facts, not executions. Forwarding
receipts are not attributed as local child execution. Receiver-minted call IDs are joined to the original caller IDs
through actual `mct-peer-call-correlation-v1` constructed observations.

The endpoint keys persisted across the setup restart:

- A: `5f15f453892384c0c121730cf91d2cdf77a5b4f691d71273b67bcdb5ef1ff043`
- B: `de728d89e8ab557449c8aad1a11c6b57862e7cf6569fe2268770adefe3f12145`

## Evidence

- [Sanitized result and canonical verification receipts](evidence/nic63-slice2/result.json)
- [Timestamped stages](evidence/nic63-slice2/stages.jsonl)
- Run: `fleet-9545bc8d-3fc8-46b6-bf66-311a67abae67`
- A ledger SHA256: `c87f2a73f50d8764dcc837d02e7d8123da3f0433cdf98b27e53f882c9d0a8690`
- B ledger SHA256: `f5ca5359bc95d8eb55698d366c453cb08de82a3fd6d74fc2b6c3192012643e23`

Raw canonical ledgers and boot logs remain in private host run directories.
Published reductions are not replacement ledgers and are not claimed independently
replayable after sanitization. They retain IDs, canonical sequence positions,
verification heads, and source-byte hashes. No private identity material is
published.

## Artifact and execution boundary

Source: `f37a14e30bbf6725069466417535a03357af20e0`.

Package: `/nix/store/b02kryi9kwrff9279pzx7cq8hzlwqp4k-mct-federation-test-bundle`.

- Daemon SHA256: `66b4dff183c36fccfc5d3d6c37b3a5b8458f898a318bbe243e6dab8e82e3cfd3`
- Helper SHA256: `233b148ac8c95425be3ec0f13cf8e45df74b96a9c95d9456d3ecab3a174430bd`

All Rust compilation and WASM fixture generation occurred on nicamaster.
Nicanode0 imported the signed closure over LAN SSH and verified signatures and
content offline; it did not build the package. These are no-debuginfo test-profile
binaries, not a release installation. Runtime WASM compilation is part of Mother
execution, not a Cargo build on the target.

The candidate's local-only lookup regression failed before the fix and passed
afterward in an isolated network namespace. The current helper passed 14 native
tests on nicamaster; the controller passed 185 Node tests. Those tests supported,
but did not substitute for, the physical evidence above.

## Failures retained, not waived

Earlier attempts failed on harness integration: sudo's Nix PATH, a public key
file without a newline, missing new signatures on already-imported store paths,
and Nix's sticky store ancestor failing the existing executable trust check.
The harness now imports signatures before re-verification and uses hash-checked
private executable copies. It does not weaken signature or executable validation.

One daemon attempt correctly stayed operator-gated because fixture/log files
had dirtied a supposedly virgin authority root. A failing layout regression
preceded moving auxiliary files outside that root. The product startup gate
remained intact. None of those attempts is counted as a pass.

An early worker's red transport test also unintentionally attempted SSH at the
example inventory before old code supported injected transports. It timed out
with the fixture identity absent; no successful remote operation was evidenced.
Tests now block real subprocess transport before importing the controller.

## Limits and continuation

The [final one-command suite](nic63-acceptance-suite.md) repeated this slice
with the final helper before its three-host and rolling-replacement runs.

This slice is not partition, three-host survival, or rolling-upgrade proof.
See the separate [three-host](nic63-slice3-three-host.md) and
[rolling-replacement](nic63-rolling-upgrade.md) evidence. Each scenario uses fresh
private roots; key continuity is checked within its restart/replacement sequence.

Stable keys at fixed LAN addresses passed. Changed-address key-only discovery,
authenticated address refresh/expiry, and rotation remain OCI-118 follow-up work.
No Tailscale, existing routing/DNS policy, firmware, permanent service, or host
image was changed.

Refs NIC-63

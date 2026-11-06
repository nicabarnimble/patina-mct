# NIC-63 first real multi-Mother mesh job

**PASS — 2026-11-06, completed 01:20:40 UTC.**

Mother A (`nicamaster`) received one bounded two-piece job. It executed the
arithmetic piece locally and delegated the repo-report piece over the existing
Iroh federation to Mother B (`nicanode0`). After B's exact directory-read
ToyGrant expired, A sent the same B operation under a new call ID. B recorded
the typed `toy_grant_expired` refusal and did not start a runtime for it.

The sanitized receipts are [result.json](evidence/nic63-mesh-job/result.json)
and [stages.jsonl](evidence/nic63-mesh-job/stages.jsonl). Private inventory,
addresses, SSH identity, canonical ledgers, and runtime roots remain outside the
repository.

## Job and result

- Job ID: `nic63-4b28f72e-b641-4fb7-9e75-494e520ee672`.
- A local call: `nic63-4b28f72e-b641-4fb7-9e75-494e520ee672:local`.
- B delegated call: `nic63-4b28f72e-b641-4fb7-9e75-494e520ee672:delegated`.
- B receiver execution ID: `mct-peer-execution-v1:.peer-execution-238d9ba662a370489d2abaab33ab2ed5`.
- Uncovered call: `nic63-4b28f72e-b641-4fb7-9e75-494e520ee672-uncovered`.
- Uncovered receiver ID: `mct-peer-execution-v1:.peer-execution-1b9475ae353e8c977c515745810b59fa`.

Both job pieces completed in order. The local result was `41`. The delegated
Child returned `mct-repo-report/v1` for the exact packaged `Cargo.lock`, with
SHA256 `c672b84f1818273f8259a1e6433a1aaf3b568dcaf7d6daa668655708bcd5519b`
and 587 packages.

## Per-Mother ledger facts

| Mother | Canonical ledger | Required facts |
| --- | --- | --- |
| A / nicamaster | 56 entries, head sequence 55, SHA256 `f4e178a72f0fb77113a3f78567b32f9816831f7f6d566de06e54b70e4fe06605` | Parent job result recorded `completed=2;denied=0;failed=0`; local piece had one runtime start and one exact executor attestation on A; delegated piece had one `peer_call_sent` to B and a completed reply; A had no runtime fact for B's piece or the uncovered call. |
| B / nicanode0 | 64 entries, head sequence 63, SHA256 `5c653da1fde7b01138afd37e3f2a872ad87620469c01479dd2ed0ee8a85b565d` | Delegated caller ID correlated to the receiver execution ID; the active directory-read grant was observed; the delegated piece had one runtime start and one exact executor attestation on B; the uncovered caller ID correlated to a distinct receiver ID with one `toy_grant_expired` / `ExpiredGrant` denial and zero runtime facts. |

Both hash-chained ledgers verified under their own Mother identities. Runtime
fact sets contained only A's local call on A and B's correlated delegated call
on B. Post-run audits found no test Mother/helper process, owned transient unit,
or executable staging directory on either host.

## Artifact and delivery boundary

- Native artifact source: `4904968bac54f6c99b9d67c8b9e9b06dc4294411`.
- Final controller source: `ab176b6`.
- Signed closure: `/nix/store/gw0dwpm2psx7zgfwkcawf4hmy6dxgm1z-mct-federation-test-bundle`.
- Mother SHA256: `8bc4d1d00410f3e59fe1587131777953e9bbad1ed414edb7618da296178a1cf5`.
- Helper SHA256: `b2e181853ad693b43525c07ecca5b87b056e8b95aeb4c34541b21354712a400d`.
- Reused PR #107 Child artifact: `sha256:447fe08ba0a105e65ceb6795162015944b67d7c0b639df010efa4989e49e0e23`.

The native daemon/helper build and immutable package were produced on
nicamaster. The package reused the exact previously accepted PR #107
`mct-repo-report` component by pinned digest. Nicanode0 imported the signed
closure, verified it offline, and did not run Cargo or a Nix build. Runtime WASM
compilation by Mother is execution, not package compilation.

No `board.post` Toy was added. The existing directory-read grant supplied the
covered/uncovered boundary: active before `2026-11-06T01:20:35.768Z`, expired at
the later probe.

## Failures retained, not waived

- The first controller attempt selected a key that depended on agent fallback;
  strict `IdentitiesOnly` authentication rejected it before deployment. The
  private inventory was corrected to the dedicated fleet key.
- The first package recipe unnecessarily attempted to rebuild the already
  accepted PR #107 component and stopped offline on an uncached build-only
  crate. The final recipe instead pins and reuses the exact accepted artifact,
  matching the task's reuse requirement.
- Two earlier physical attempts completed the requested workload and refusal,
  but post-run harness assertions expected a truncated mesh detail and one
  completion observation rather than one executor attestation. Neither attempt
  was counted. The reduction and assertions were corrected and the complete
  physical run was repeated to the passing receipt above.

## Boundaries

This proves one ordered two-piece submission, single-hop delegation, independent
ledger attribution, and receiver-side expiry refusal on the named Pi fleet. It
does not claim parallel scheduling, durable job queues, restart-resume, rollback,
shared ledgers, multi-hop routing, host reboot, or a new Toy contract.

Refs NIC-63

# NIC-105 canonical Child dependency drift

**PASS — 2026-11-06, completed 03:41:28 UTC.**

The same canonical `mct-repo-report` component ran against immutable
`Cargo.lock` snapshots on Mother A (`nicamaster`) and Mother B (`nicanode0`).
Mother A submitted one ordered mesh job with exact placement constraints. A ran
its inventory locally, delegated B's inventory over the existing Iroh
federation, and computed a non-empty client-owned drift. After both exact
directory-read grants expired, a fresh B probe was denied by B before any Child
runtime fact.

The sanitized receipts are
[`result.json`](evidence/nic105-dependency-drift/result.json) and
[`stages.jsonl`](evidence/nic105-dependency-drift/stages.jsonl). Private
addresses, SSH identity, source paths, canonical ledger files and runtime roots
remain outside the repository.

## Job and drift

- Job ID: `nic105-4e6ee9b2-a52f-42c9-ad0b-5785ccacb776`.
- A call: `nic105-4e6ee9b2-a52f-42c9-ad0b-5785ccacb776:mother-a`.
- B delegated call: `nic105-4e6ee9b2-a52f-42c9-ad0b-5785ccacb776:mother-b`.
- B receiver execution ID: `mct-peer-execution-v1:.peer-execution-a08039a4e7d6368cc5f483b9d50fc892`.
- Expired B probe: `nic105-4e6ee9b2-a52f-42c9-ad0b-5785ccacb776-expired-b`.
- Expired B receiver ID: `mct-peer-execution-v1:.peer-execution-19a1d0af0adb90ed1f6558818486b68a`.

Both pieces completed. The `mct-dependency-drift/v1` result compared two
537-package inventories and found **52 package differences**:

- 48 `version_mismatch` entries;
- 2 `only_left` entries; and
- 2 `only_right` entries.

Notable differences included Wasmtime and `wasmtime-wasi` `49.0.5` on A versus
`48.0.8` on B, `mct-repo-report` `0.2.0` only on A, and `wasip2`/`wasip3` only
on B. A's snapshot SHA256 was
`dc15ce610aaf8d883848b8eef6304f05273bbc227e454aa03520b263c857ee70`;
B's was `da4c7963a8811ee8fa57d89b38484ac92c7ff61bb301a9a58290f58fdac9e182`.

## Per-Mother ledger facts

| Mother | Canonical ledger | Required facts |
| --- | --- | --- |
| A / nicamaster | 77 entries, head sequence 76, SHA256 `3ffd7d8ddde5944dc8629be4d0b0ddf92852ecb520b60bca2a0ae666919fa043` | Parent result recorded `completed=2;denied=0;failed=0`; A's piece had one runtime start and one exact executor attestation on A; B's piece had one peer send and completed reply; A had no runtime fact for B's piece or the expired probe. |
| B / nicanode0 | 64 entries, head sequence 63, SHA256 `1c8d81c6f2110fa827063e6f9e1ba26d623085a8dbc0e81afc389c78e3998971` | B's caller ID correlated to the receiver execution ID; B's piece had one runtime start and one exact executor attestation on B; the fresh probe correlated to a distinct receiver ID with one denied `toy_grant_expired` / `ExpiredGrant` fact and zero runtime start/completion/failure facts. |

Both independent hash-chained ledgers verified with preserved prefixes. Runtime
fact sets contained only A's inventory on A and B's correlated inventory on B.
For the expired probe, the combined A/B ledger query returned **zero runtime
facts and zero Child starts**.

## Artifact and delivery boundary

- Artifact and controller source: `be6c6f76512d145d1665eaf2dd5a80d0b5acb0dd`.
- Signed closure: `/nix/store/irrgs7gyki4ypid9a85jsmnldvw6xshh-mct-federation-test-bundle`.
- Closure size: 310,737,584 bytes across 37 requisites.
- Mother SHA256: `0fcedb277de53afa61dc6bb6cbf789f7553ee44c4b88cc896e7a118f03cc3fea`.
- Helper SHA256: `e032ad36937e5c99949ae71dae3a2e8fd3265655d53ac0112f3a53cf55dbd4a2`.
- Child component SHA256: `fef072235c904a68de5c3079c8dbaa81f87a2f35244e0a0de54a10c9ee1fcb87`.

Nicamaster produced the native daemon, federation helper, Child component,
manifest and both immutable snapshots, then assembled one signed Nix closure.
Nicanode0 imported that closure through the signed binary cache path and
verified the same daemon digest. It did not run Cargo or a Nix build. Runtime
Wasm compilation by Mother is Child execution, not target package compilation.

Post-run audits found the closure on both hosts and no test Mother/helper
process, owned transient unit or executable staging directory on either host.

## Architecture and blast-radius review

- `mct-repo-report` remains a data-in/data-out Child. Its WIT imports only the
  existing directory-read filesystem surface; no Toy, network capability,
  persistence contract or authority source was added.
- The report schema stays `mct-repo-report/v1`; the complete sorted
  package/version map is additive and bounded by the existing Child result
  limit. Physical payloads were 18,376 and 18,340 bytes.
- `required_node_id` is an optional placement constraint on a local mesh piece.
  It removes nonmatching candidates before ranking, cannot make a route
  authorized, and participates in the idempotency fingerprint. Existing NIC-63
  callers omit it and retain their prior behavior.
- B still revalidates artifact approval, assignment and ToyGrant independently.
  The physical expiry probe proves placement did not bypass receiver authority.
- Production impact is limited to local mesh ingress candidate selection,
  idempotency metadata, the canonical Child's additive output, and the
  federation acceptance package. No public network protocol or Toy contract
  changed.

## Failures retained, not waived

- The first builder attempt stopped before closure production because the
  offline Child build lacked a Child-only build crate. The final builder path
  permits dependency fetch on nicamaster while keeping the target hosts
  compile-free.
- The next attempt stopped before closure production because the minimal Rust
  toolchain did not include `wasm32-wasip1`. The pinned Nix toolchain now
  includes that target explicitly.
- A transferred macOS archive left AppleDouble metadata beside WIT files; WIT
  parsing rejected the non-UTF-8 sidecar. The metadata was removed on the
  builder and was not part of the committed source or final artifacts.
- The first passing physical receipt preceded a release-spec finding that the
  Child crate version had to inherit the workspace version. The manifest was
  corrected without changing resolved version `0.2.0`; the closure was rebuilt
  from `be6c6f7` and the full physical ceremony was repeated. This document and
  its receipts describe that later authoritative run.

All failed build attempts remained on nicamaster and produced no deployable
closure. The single final closure above was imported unchanged on nicanode0.
Both physical workload runs passed; only the later final-source run is retained
as acceptance evidence.

## Repository gates

The final branch passed:

- `cargo fmt --check`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- `cargo test --workspace`;
- `bash scripts/ci-tier0.sh specs`; and
- `git diff --check`.

This proves one canonical directory-read-plus-compute Child, exact two-Mother
placement under existing authority, client-owned dependency comparison,
independent ledger attribution, receiver-side expiry denial, and build-once
closure delivery. It does not claim parallel scheduling, durable queues,
restart-resume, multi-hop routing, rollback or a typed successor WIT.

Refs NIC-105, NIC-104

# NIC-118 second opinion from Sol

**Status.** Independent design review, not a ratified spec or implementation plan.
Reviewed `bc48d1e` on `nicabar-nic118-sol-review`. This complements Astra's design;
it does not replace it. The independent sections were written before reading
Astra's document.

**Recommendation.** Ship one Mother, one filesystem-agnostic Child, and a
read-only `host-fs` Toy first. Keep ZFS privileges outside Mother's container.
Do not make federation, a full durable-agent runtime, or native identity
replacement prerequisites for browsing files.

Here, **piece 1** means authenticated list/stat/bounded read of named roots on
one long-lived Mother. Piece 2 adds move/rename. Piece 3 adds write/delete with
an explicit recovery policy. These are review labels, not new issue scope.
The deployment is a container on nica-unraid. The same Child must work on a Pi
without ZFS, a browser, or another Mother.

## 1. What Mother still needs to be a normal, maintained app

### What is already there

This is not a request to build a runtime from scratch. The repository has
artifact integrity checks, separate approvals and assignments, WIT invocation,
ToyGrant evaluation, call deadlines, memory limits, restart settlement, and
real federation/upgrade evidence. It also has an ordinary, caller-scoped
read-only directory adapter and the `mct-repo-report` Child. That workload is
useful precedent, although it is not a general file service. [E1][E2][E3][E4]

Several overview documents lag the code and later decisions. In particular,
D-58 replaces the guardian/hybrid ceremony for new administration. The current
passkey adapter supports admin-only Iroh serving, but the full Child/Toy
runtime cutover remains unbuilt. Do not turn an old build-plan checklist into
NIC-118's mandatory backlog, or claim that admin-only acceptance proves an
authenticated file application. [E5][E6]

### Gap triage

Every row classifies the missing work, not an assertion that all of its
prerequisites are absent. “Blocks later” means a named later feature must wait,
not that piece 1 can omit basic safe operation.

| Gap and current evidence | Classification | Smallest sufficient closure |
| --- | --- | --- |
| **Real application identity.** Toy subjects currently bind Child artifact/version, assignment and optional caller node. `CallerIdentity` has an optional user ID, but peer calls require it absent; a peer binding does not delegate a human. That is not yet a Pocket ID human or bot grant. [E2][E7][E18] | **blocks piece 1** | Specify and implement one authenticated ingress into Mother's call path. Verify Pocket ID issuer, audience, signature and expiry at the trusted edge. Preserve a stable `(issuer, subject)` principal through admission and Toy authorization. Bind root, action and principal to the invocation, not an untrusted JSON field. Give bots distinct credentials and grants, not a shared human session. |
| **Admin and app authority must connect without a bypass.** Passkey admin-only serving and the legacy resident runtime are deliberately separate; passkey ledgers refuse the legacy startup path. [E6][E8] | **blocks piece 1** | Choose an explicit authenticated Child ingress and authorized provisioning path. Do not bridge with an all-powerful same-UID proxy or silently convert passkey facts. If this is a bounded new profile rather than the complete D-08/D-10 cutover, record that scope decision and its remaining limits before exposure. Pocket ID login is not permission to administer Mother. |
| **Named-root selection and multi-root grants.** The ordinary adapter accepts exactly one matching resource and preopens it as `/project`; multiple matching resources are ambiguous. [E2] | **blocks piece 1** | Add a root registry owned by deployment config, a stable opaque root ID in the Child API, and exact per-invocation root selection authorized by Mother. Never accept a browser-supplied host path. A manifest requests access; it does not grant it. |
| **Host-root confinement appropriate to shared data.** The existing adapter requires a canonical trusted directory through private-storage-style ancestry checks. Guest-writable vault directories may not satisfy them. [E2][E9] | **blocks piece 1** | Keep Mother's authority storage private. Separately define and test shared-data root admission, including UID/GID, mount identity, root replacement and symlink policy. Use held directory capabilities and descriptor-relative operations, not `canonicalize` followed by a later ambient-path open. Do not loosen private ledger checks to accommodate vault permissions. |
| **Container and actual host target.** Documented targets are Darwin/Linux arm64; the Linux install is systemd/admin-only, not this Unraid container. [E1][E10] | **blocks piece 1** | Verify the server architecture, then package and exercise that target. If it is x86-64, add explicit amd64 build/acceptance rather than assuming arm64 evidence transfers. Pin the image digest, run unprivileged, mount only named data roots read-only, and persist endpoint identity, ledger, state, config and artifact catalog outside the disposable image layer. No Docker socket, host `/`, ZFS devices or privileged container. |
| **Long-lived lifecycle and readiness.** Restart settlement exists, but installation on one platform is not container lifecycle proof. [E1][E10][E11] | **blocks piece 1** | Prove stop/drain, bounded restart, same identity/history after container replacement, second-writer refusal and fail-closed startup with missing mounts or unavailable authority. Readiness must include replay and root admission, not just an open port. A missing vault mount must not expose an empty fallback directory as the real root. |
| **Bounded file-service contract.** Current resident WIT ingress consumes JSON and returns JSON; transport and WASM limits are finite. [E2][E12] | **blocks piece 1** | Specify paged listings and ranged reads with conservative byte/time limits, typed errors, unsupported node kinds, filename encoding and cancellation. Reject symlinks initially, and refuse devices/FIFOs/sockets. Do not read an arbitrary file into one JSON value. Define whether a file changing between chunks yields a conflict or a best-effort view. |
| **Updates that preserve authority and compatibility.** Exact-digest upgrade and Child lifecycle primitives exist. The Linux rolling-upgrade proof is harness-managed, not a container migration or rollback proof. [E3][E4][E13] | **blocks piece 1** | Document one manual update path with retained old image, backup, quiescence and compatibility checks. Replace the image without regenerating identity or grants. Stage/approve the exact Child artifact; its new version must not inherit authority accidentally. Reverting a binary must not roll back revocations. For piece 1, no application schema migration is preferable to inventing one. |
| **Recovery and storage budget.** The ledger is canonical for some authority, while Child/peer facts and recovery still have documented projection gaps. [E5][E11] | **blocks piece 1** | Back up all currently required sources together, not “ledger only.” Rehearse restore into an isolated replacement and fence the old writer. Test denial on corrupt history and full disk. Set basic ledger/state space monitoring and a documented response. Never discard old facts merely to make startup fast. |
| **Operator visibility and application exposure.** Existing status/audit machinery is a base, not a file-service runbook. [E1][E6] | **blocks piece 1** | Supply readiness, version/digest, active roots, denied calls and opaque audit references without leaking file contents or tokens. Test wrong identity, wrong root and revoked identity. Browser ingress needs CSRF/session handling, rate limits and a defined logout/revocation delay. Obtain the issue's exposure review before LAN/tailnet/WAN access. |
| **Three authority levels.** Runtime preopens currently have only `ReadOnly` and `ReadWrite`. [E14] | **blocks later** | Before move/rename, enforce allowed operations in the host boundary. A general read-write preopen is not “read-write without delete.” Test the guest directly, bypassing the intended Child API. Define overwrite, truncation and rename-replacement explicitly; they can destroy data without an unlink call. |
| **Mutation retries, conflicts and crash outcomes.** A 12-minute dispatch fence is not a permanent effect journal or a filesystem transaction. [E11][E12] | **blocks later** | Before move/write/delete, use stable effect IDs, no-clobber defaults and source/destination preconditions. Authorize both roots for cross-root work. Same-filesystem rename and copy-then-delete are different operations. Unknown outcome after a crash must not trigger blind replay. Define temp-file publication, fsync ordering and result retention for writes. |
| **Trash/snapshot recovery contract.** No ZFS Toy or `host-fs` backend is implemented in the inspected adapter registry. [E15] | **blocks later** | Before destructive operations, choose trash or demonstrably sufficient snapshot coverage and test restoration. Trash paths are per-root capabilities too. Snapshot availability alone is not an authorization grant or a guarantee that the latest bytes are recoverable. |
| **Small-device capacity.** Per-invocation memory limits exist; neither those limits nor the admin-only Pi gate prove this service fits a small Pi. The resident path creates a runtime and discovers component imports per call. [E2][E10][E14] | **blocks piece 1** | Run the same Child on a physical target Pi with bounded concurrency, a large directory and repeated ranged reads. Record peak RSS, cold/warm latency, ledger growth and restart time. Keep no persistent unbounded index in the first slice. Cache verified compilation only if measurements justify it. |
| **State migration for future Child versions.** Ready-before-drain lifecycle transitions do not themselves migrate application state or outstanding effects. [E13][E16] | **blocks later** | Before persistent jobs/indexes/uploads, specify schema compatibility, task pinning and migration recovery. An incompatible image cannot be “rolled back” by restoring an old authority database. |
| **Fleet inventory, seamless updates, portable delegated handles, watch/indexing and sophisticated dashboards.** These are separable programmes. [E5][E16] | **nice-to-have** | Keep extension points and accurate limitations. Do not make every future handle/delegation feature a prerequisite for one bounded authenticated caller path. Seamless availability is not required; explicit short maintenance downtime is acceptable. |

“Done” for a maintained Mother means operators can install, stop, restart,
update, diagnose and recover it without inventing permissions or losing
identity. It does not mean every future Child capability is finished. The
first acceptance should include a container replacement and an identity
revocation, not merely a successful file listing.

### Confinement is not the whole data policy

A WASI preopen confines guest path reach. It does not establish the human's
identity, grant a particular action, stop an already-issued descriptor
immediately after revocation, or make snapshots a backup. Existing D-G7
semantics permit admitted preopens until their bounded invocation ends. Keep
piece-1 calls short and disclose that revocation latency. [E1][E14]

Use read-only WASI preopens for piece 1. For later differentiated mutation
rights, prefer operation-scoped `host-fs` WIT calls backed by held cap-std
handles. Do not also hand the guest a broader writable descriptor. Alternatively,
a standard WASI host implementation must enforce equivalent rights at every
relevant operation; unsupported operations may deny, but their standard meaning
must not change. A Child-side `if can_delete` is not a security boundary.

Root configuration must also define the trust in hard links, nested mounts and
other host writers. Path confinement cannot distinguish two authorized names
for the same inode, or undo a host administrator exposing another dataset
inside a granted root. Start with regular files/directories, no symlink
following, explicit mount policy and separately tested root-open races.
Existing guest SMB access remains an independent writer and access path; this
review neither changes it nor claims Mother has replaced it.

Stay with versioned WIT and the component model. This checkout pins Wasmtime
49.0.5 and explicitly admits selected WASI 0.2.3 imports. Adopt WASI 0.3 through
a tested runtime/contract update, not as a promise that the current host already
supports its async/streaming interfaces. Paged/ranged Child operations can ship
without waiting for that update. The UI should adapt a Norton Commander/VFS
library to the Child API, never obtain host paths, mount privileges or Toy
handles. Library choice is not a Mother-completion blocker. [E1][E2][E17]

## 2. The ZFS piece and its relationship to Mother

**Recommendation.** A narrow native host helper plus a separately granted ZFS
Toy adapter. Not ZFS logic in Mother, not a shell-capable Child, and not a
second Mother merely because the helper runs outside a container.

| Placement | Judgment | Reason |
| --- | --- | --- |
| ZFS inside the generic filesystem Child | Reject | Couples the portable file API to one storage stack and cannot provide native ZFS privilege within the WASM boundary. |
| ZFS commands in a privileged Mother container | Reject | Enlarges the authority runtime's compromise radius to pool administration. |
| ZFS host helper behind a Mother-owned Toy adapter | Prefer | Keeps host privilege narrow and optional while Mother retains grant decisions. |
| A ZFS-aware Child over that Toy | Later, if useful | Appropriate for snapshot browsing, retention planning or health interpretation as application logic. It does not replace the privileged helper. |

Proposed path for optional functionality:

```text
client -> filesystem Child -> ordinary host-fs Toy -> named data root
client -> storage-aware Child -> zfs Toy adapter -> constrained host helper
                   Mother admits both paths and owns their grants
```

The second Child need not exist in piece 1. If the file API later displays
recovery information, expose filesystem-neutral optional protection metadata.
A separate management client can consume the ZFS-aware application API without
teaching the ordinary Child how to run `zfs`.

The helper accepts fixed operations and configured dataset IDs, never arbitrary
command strings, device names, mountpoints or client-supplied host paths. Give
it the least host privilege/delegation needed. A local socket is a transport,
not sufficient authority. Constrain its authenticated Mother client, operation
allowlist, dataset mapping, request size and replay behavior. A container holding
that helper credential must not obtain pool-wide administration. If Unraid
cannot supply a safe narrow helper, keep snapshots externally operated first.

Mother owns the caller identity, ToyGrant, exact dataset/action admission and
before-effect observations. The helper owns only native mechanism and its
fixed safety envelope. It reports structured results with dataset identity,
operation ID, snapshot ID/GUID where available, observation time, outcome and
bounded errors. It cannot mint grants, infer an allow from a UI selection, or
become a second authority database. A mutating helper needs stable operation IDs
and queryable outcomes so Mother can reconcile a lost reply without repeating
a snapshot destruction or restore. This follows the repository's distinction
between effect admission and effect completion. [E7][E11][E16]

Start with read-only dataset/snapshot/health reporting if it has immediate user
value. Reserve snapshot create, restore, destroy and pool administration as
separate capabilities. Never expose raw pool rollback as “undo file delete.”
Dataset rollback can destroy unrelated newer changes. Prefer an explicit
single-file restore workflow and conflict checks.

Do not enable destructive file operations because a report says “ZFS enabled.”
If snapshots are the recovery mechanism, establish dataset-to-root mapping,
freshness, retention, recoverability and who can delete snapshots. Old snapshots
do not contain newly written files. A pool failure can lose both data and its
snapshots. Snapshot browsing also needs scope checks because it can reveal
historical files the caller can no longer read in the live root.

The host helper is deployment plumbing, not portable Child code. On a Pi
without ZFS it is absent, the optional capability is unavailable, and ordinary
filesystem operations remain unchanged. This preserves the existing rule that
native host adapters are trusted mechanisms, not native local Children. [E5]

## 3. Multi-Mother needed now?

**No.** NIC-118 needs one authoritative Mother near the mounted files. A Mac,
browser or bot can be a client; it need not operate a Mother merely to browse.
A ZFS host helper is likewise not a reason to add another Mother.

This is a NIC-118 dependency judgment, not a reversal of D-52. Independent
Mother federation is already a separate product/release obligation, and NIC-63
has real mixed-binary Linux evidence. Clustering, shared ledgers, standby and
work takeover remain different, excluded work. Do not reopen those programmes
under the name of file-service availability. [E3][E5]

Add a second Mother when there is a second independent authority locality with
its own storage or host effects and a real cross-node use case. Then preserve
bilateral admission and local grants. A remote file call must remain pinned to
the node that owns that root; a planner must not substitute another `/data`
when the first node disappears. An unavailable vault should be unavailable,
not silently executed against a replica or another Mother's ledger.

Single-node crash recovery, explicit update downtime and backup/restore are
required now. Automatic failover is not. Two processes pointed at the same
SQLite database, ledger and mounted files are not two safely federated Mothers.

## 4. Biggest risk to MCT

**My judgment.** The biggest risk is mistaking a rich authority model for an
enforced, usable authority boundary on the actual application path.

NIC-118 makes that risk concrete. A system can have signed admin leases,
ToyGrants and a hash-chained ledger while a web proxy collapses every user into
one service principal, or a writable preopen lets a compromised Child bypass
the advertised delete level. The ledger would then record permission to hand
out excessive power rather than prove the user's actual operation was allowed.
This is a design risk, not a claim that an unimplemented `host-fs` has already
been exploited. [E6][E7][E14]

The inverse failure is spending another release completing abstract machinery
before anyone can safely read a real directory. Both failures come from the
same mistake of measuring framework completeness rather than the user's path.

The countermeasure is one adversarial, operator-facing acceptance per piece:

1. On one long-lived container, two real principals receive different named
   roots. Allowed list/stat/read works; wrong-root and forged-principal calls
   deny. A direct guest attack cannot escape by path or symlink.
2. Revocation blocks new calls within the stated bound. Container replacement
   preserves identity, grants and denial. Missing mounts, stale authority and
   ledger failure fail closed without inventing successful results.
3. Before mutation release, an adversarial guest attempts unlink, truncate,
   overwrite-by-rename and cross-root copy/delete under each level. Host
   enforcement, not UI controls, decides. Crash after the host effect but before
   the reply yields a reconciled result or an honest unknown, never blind replay.
4. Run the same Child and bounded API on a physical Pi. The ZFS helper's absence
   changes optional capabilities, not the application contract.

Ship those proofs in order. Do not promise universal exactly-once effects,
immediate descriptor revocation, or data recovery beyond the mechanism tested.

## Evidence and limits

This review inspected repository documents, Mother/Child/Toy source and test
bodies, plus the requested NIC-118 issue. It did not deploy a container, inspect
nica-unraid's mounts/architecture, test Pocket ID, run a physical Pi workload,
or execute ZFS. Capacity and deployment judgments above are proposed gates,
not measured results. Patina context returned no indexed knowledge, scry no
results, and assay lacked `index_state`; source inspection supplied the evidence.

- [E1] [README](../../README.md), including current runtime and security limits.
- [E2] [Resident execution](../../crates/mct-daemon/src/daemon/resident/execution.rs), especially `resident_directory_read_host_adapters`, `authorize_resident_toy` and `execute_resident_wit_child`.
- [E3] [NIC-63 rolling replacement](../nic63-rolling-upgrade.md), including its evidence limits.
- [E4] [First application plan](../plans/first-real-app.md), [report Child](../../crates/mct-repo-report/src/component.rs) and [WIT world](../../crates/mct-repo-report/wit/world.wit).
- [E5] [Decision log](../DECISIONS.md), especially D-03, D-06, D-08/D-10, D-32, D-38, D-51/D-52 and D-58.
- [E6] [Passkey administration](../passkey-admin.md), especially the current adapter and resident Iroh slice.
- [E7] [Toy authority types and admission](../../crates/mct-kernel/src/toy.rs), especially `ToyGrantSubject`, `ToyGrantScope` and `AuthorizedToyCall`.
- [E8] [Startup](../../crates/mct-daemon/src/startup.rs), including explicit rejection of passkey authority by the legacy runtime.
- [E9] [Private filesystem checks](../../crates/mct-observation/src/private_fs.rs), especially `directory` and `trusted_directory`.
- [E10] [Linux ARM64 installation](../linux-admin-install.md), including admin-only and measurement limits.
- [E11] [Open questions](../OPEN-QUESTIONS.md), [current security boundaries](../SECURITY-BOUNDARIES.md) and [reliability doctrine](../../layer/core/reliability-doctrine.md). Older admin/recovery descriptions must be read with D-58.
- [E12] [Iroh payload limits](../../crates/mct-iroh/src/serve.rs), [blob limits](../../crates/mct-daemon/src/blob_store.rs), [idempotency TTL](../../crates/mct-daemon/src/state.rs) and [resident JSON ingress](../../crates/mct-daemon/src/daemon/resident/local_ingress.rs).
- [E13] [Child reload](../../crates/mct-daemon/src/lifecycle.rs) and [upgrade CLI](../../crates/mct-daemon/src/daemon/upgrade.rs).
- [E14] [WASM host](../../crates/mct-daemon/src/wasm.rs), especially `MctWasiPreopenAccess`, `build_wasi_ctx` and `delegated_preopen_survives_revocation_while_new_delegation_denies`.
- [E15] [Toy adapter registry](../../crates/mct-daemon/src/toy.rs).
- [E16] [Durable task boundary proposal](durable-task-boundary.md), not a ratified continuation contract.
- [E17] [Workspace dependencies](../../Cargo.toml) and [Child package manifest](../../crates/mct-repo-report/child.toml).
- [E18] [Call identity](../../crates/mct-kernel/src/call/mod.rs), especially `CallerIdentity` and its peer delegation restriction.

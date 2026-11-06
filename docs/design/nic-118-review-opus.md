# NIC-118 second-opinion review: host-fs Child on a nica-unraid Mother

Status: independent review, not a decision. Written 2026-11-06 against `main`
at `bc48d1e`. The main analysis is `docs/design/nic-118-host-fs.md` on branch
`nicabar-nic118-fs-design`. This note does not replace it.

Evidence labels used below:

- **Read**: I read the cited source or doc line in this checkout.
- **Inferred**: follows from code I read, not executed.
- **Guess**: not verified in this session.

No code was run against nica-unraid, a Pi, or a container. Nothing here is
release evidence.

## The ask in one paragraph

Doug wants the first real `mctChild`: a filesystem-agnostic Child, behind a
Mother running as a container on nica-unraid, that lists, stats and reads files
first, then moves and renames, then writes and deletes. A generic `host-fs` Toy
grants named host roots through WASI preopens and cap-std at three levels
(read, read-write, read-write+delete), tied to a person's identity. A
Norton Commander style UI talks only to the Child's API. The same Child must
run on a Pi.

## What already exists (and is closer than it looks)

Piece 1 is mostly built. It is not new filesystem code.

- `toy:mct:directory-read` is a real catalog Toy over
  `wasi:filesystem/preopens@0.2.3` (Read: `crates/mct-daemon/src/daemon/watch.rs:6`, `:90-96`).
- The resident executor evaluates the grant, requires a `file://` resource,
  and installs one read-only preopen at guest path `/project`
  (Read: `crates/mct-daemon/src/daemon/resident/execution.rs:371-444`).
- `crates/mct-repo-report` is a working Child that imports exactly
  `wasi:filesystem/{types,preopens}@0.2.3` and exports one function. It ran
  across Mothers in #106 and on a Lume VM in #107. A host-fs Child for piece 1
  is this crate with `list`, `stat` and `read-range` instead of `inspect`.
- wasmtime-wasi does the path confinement with cap-std, so Pi Grok's "don't
  write our own path checks" is right for reads.

## 1. Gaps before a Mother is "done" as a long-lived app

"Done" here means it runs unattended for months on nica-unraid, takes Mother
and Child updates without re-bootstrapping, and a person can be held to what
they did. Each gap is tagged **blocks piece 1**, **blocks later**, or
**nice-to-have**.

### Blocks piece 1

**G1. No x86_64 Linux build or container image.** Unraid runs on x86_64 only
(Guess, general Unraid knowledge, not checked on nica-unraid). The supported
targets are macOS arm64 and Linux arm64 (Read: README "Quick start", D-52
NIC-63 amendment). The x86_64 Linux release slot is deliberately closed:
`scripts/test-release-platform.sh:41-45` fails if it ever emits a release.
There is no Dockerfile or OCI build anywhere in the repo (Read: grep). Piece 1
needs an `x86_64-unknown-linux-gnu` build and a pinned image. Note the irony.
The Pi is the supported platform and Unraid is the unsupported one, so
"small-device-first" is also the fastest route to a first run (see
Sequencing).

**G2. The existing directory-read Toy will refuse an Unraid share root.**
`resident_directory_read_host_adapters` calls
`mct_observation::private_fs::trusted_directory(&root)` (Read:
`execution.rs:415`). That check walks every path component and refuses any
directory not owned by root or by Mother's uid, and any group- or
world-writable directory unless it is root-owned and sticky (Read:
`crates/mct-observation/src/private_fs.rs:35-90`). Unraid user shares are
normally `nobody:users` with `0777` (Guess, Unraid default), and the issue
says `data` and `documents` are guest-writable over SMB today. So the first
grant on `documents` fails closed, whatever uid the container runs as
(Inferred). The ways out are:

- Change the dataset's owner and mode. That is an SMB permission change, which
  NIC-118 puts out of scope without Doug's OK.
- Add an explicit, ledgered grant attribute for a foreign-owned root that is a
  mount point, granted read-only only. The check exists to stop a root swap
  between canonicalize and open. Inside a container, a bind-mounted dataset
  root cannot be renamed by SMB guests, and cap-std anchors every later
  operation on the opened directory handle (Inferred). The exception must be
  visible in the grant and the ledger, never a silent loosening.

Either way, this is a decision for Doug before piece 1, not a deploy-day
surprise.

**G3. There is no ingress a browser or the UI can reach.** Local calls enter
through `POST /calls` on the owner UDS socket, and the caller's `user_id` is
`uid:<peer_cred uid>` (Read: `local_ingress.rs:182-214`). Peer calls enter over
Iroh from other Mothers. Nothing speaks HTTP to a browser. Piece 1 needs one
small ingress adapter. Building it as a separate process on the UDS socket
makes every human the same `uid:N`, and D-08/D-15 already decided to delete
that socket. Build it as a Mother-side adapter instead, with its own
`CallOrigin`, so the caller identity is set by Mother from a verified
credential (see G6).

**G4. Reads are capped at about 250 KB per call.** Default
`output_bytes` is 256 KiB and host values are 64 KiB (Read:
`crates/mct-daemon/src/wasm/budget.rs:17-29`). Results are lifted to JSON, and
`list<u8>` lifts to a JSON array of numbers (Read: `wit_values.rs:687`), so one
call returns roughly 60-90 KB of raw file bytes (Inferred). A Child that
returns base64 in a `string` gets about 190 KB. Piece 1 can live with a
`read-range(path, offset, len)` API and chunked downloads for documents. It
should not pretend that a 2 GB file goes through Child results. Larger files
are G9.

**G5. Exact WASI version pinning.** The ordinary directory-read path admits
only an exact list of `@0.2.3` import names (Read:
`execution.rs:344-368`). A Child built by a current toolchain that emits a
newer 0.2.x patch version will be refused before instantiation (Inferred).
Pin the Child's WIT deps to 0.2.3 for piece 1, as `mct-repo-report` does, and
fix the allowlist to semver-compatible matching later.

**G6. Grants have no person in them.** Revised from "blocks later" after
reading Astra's doc. `ToyGrantSubject` matches child name, artifact id, exact artifact version, assignment, and caller node. It never
looks at `CallerIdentity.user_id` (Read: `crates/mct-kernel/src/toy.rs:36-46`,
`:836-842`). "Grants tied to identity" is not representable today. My first
draft said piece 1 could survive with one human (Doug) and a read-only grant.
That contradicts my own biggest-risk argument below. An identity-gated piece 1
whose identity lives only at the edge is the shortcut I warn against. Moves and
deletes make it worse, because "who moved this" is the first question they
raise, and bots need narrower grants than Doug (issue question 4). The fix is
a principal field in the grant subject, matched against an identity Mother verified itself. For
Pocket ID that means Mother checks the OIDC ID token signature against a
pinned issuer key that is itself a ledger fact. Caddy `forward_auth` headers
are transport, not identity. Mother already verifies WebAuthn P-256 for D-58,
so the crypto is in-tree (Read: `p256` in `mct-daemon/Cargo.toml`).

### Blocks later

**G7. Read-write and read-write+delete collapse to one bit.** wasmtime-wasi
49 has `FsPerms::{ReadOnly, ReadWrite}` and nothing else (Read:
`wasmtime-wasi-49.0.5/src/filesystem.rs:100-105`; MCT notes the collapse at
`wasm.rs:391`). A read-write preopen allows `unlink-file-at`,
`remove-directory-at`, and `rename-at` over an existing file. Overwrite with
truncate is also delete in practice. So the three levels cannot be enforced
by preopens. Any "no delete" promise made with a read-write preopen is
enforced by Child code, which breaks "running code is not authority".

**G8. No per-operation audit or revocation inside a preopen.** The preopen is
authorized once, when the WASI context is built (Read: `wasm.rs:380-430`).
Everything the Child then does through it is invisible to the ledger, and
revocation waits for the call to end (D-G7, `SECURITY-BOUNDARIES.md`). That is
fine for reads. For moves and deletes, Pi Grok's "every operation writes a
signed receipt" is not true on this path (Inferred).

G7 and G8 have one fix. Mutations become typed Toy functions on a
`host-fs` WIT interface, implemented in Mother with cap-std `Dir` handles.
Each call is a Toy call with its own grant evaluation, action string
(`move`, `write`, `remove`), and before-effect ledger fact. This follows D-06
("extend with a Toy, never fork the standard"). Reads can stay on standard
`wasi:filesystem`. Do not interpose wasmtime's `wasi:filesystem` host to
deny unlink. It couples MCT to wasmtime internals that changed in 48, and
turning `unlink` into "move to trash" would change a standard meaning, which
D-06 forbids.

**G9. Large files need a byte path that is not a Child result.** The repo
already has a content-addressed local blob store and `/blobs` route (Read:
`control.rs:2488`, `:5260-5335`). The clean shape is a Mother-minted,
short-lived, read-only content handle. The Child authorizes and names the
file, and Mother streams the bytes from the same cap-std handle. It matches
the courier rule (D-21: Mother mints every handle). WASI 0.3 `stream<u8>` is
the long-term Bytecode Alliance answer. wasmtime 49 is built with
`component-model-async` (Read: `Cargo.toml:51`), but MCT does not wire
0.3 WASI yet.

**G10. Every Child update needs new grants.** Grant subjects match the exact
`artifact_version` (Read: `toy.rs:838-840`). Ongoing Child updates are the
normal life of an app, so each release means re-issuing every person's grants
by hand. A "supersede to digest X" grant transition, explicitly approved and
ledgered, would keep exactness without the toil.

**G11. The Child runtime and the new admin model cannot coexist.** A Mother
is either the legacy runtime with owner-UDS admin, or the D-58 passkey admin
Mother. "The legacy runtime still refuses passkey ledgers" (Read:
`docs/passkey-admin.md` "Current adapter"). The Linux admin install
explicitly "does not enable the full passkey Child runtime" (Read:
`docs/linux-admin-install.md`). D-58 says old ledgers are never auto-promoted.
A piece-1 Mother therefore runs on the admin path scheduled for deletion, and
its ledger will need a fresh bootstrap at cutover (Inferred). Accept that in
writing, or finish the runtime cutover first.

**G12. Supervision and upgrade are launchd-only.** The supervisor context
check bails on non-macOS (Read: `supervisor_lifecycle.rs:887`, `:1349`), and the
guided `upgrade` composes launchd stop/install/start. NIC-63 proved rolling
binary replacement on Linux with test binaries (Read:
`docs/nic63-rolling-upgrade.md`), not a product path. In a container, Docker
restarts the process and an image swap is the upgrade. MCT needs a foreground
mode that admits it is externally supervised, a health probe for the
container, and an upgrade fact that records the image digest. Without it,
updates on nica-unraid happen outside the ledger.

**G13. The ledger only grows.** Segment rotation exists but is off unless a
caller opts in, and `open` does not read segments (Read:
`crates/mct-observation/src/segment.rs:1-8`). A file browser writes facts on
every call. On a Pi, startup replay time will grow with the ledger (Guess,
not measured). Needs a retention and checkpoint policy before the app is
left alone for months.

**G14. Deployment is not authorized yet.** `finish-owner-gates.md:105` and
`HANDOFF-MCT-FINISH.md:93` say no production Mothers or Unraid deployments
are authorized. Doug's NIC-118 ask is the authorization in spirit. It should
land as a DECISIONS.md entry so the repo stops contradicting the work.

### Nice-to-have

- **G15. Named roots, plural.** The Toy installs one preopen at `/project`
  (Read: `execution.rs:437`). Piece 1 needs one root. Two-pane browsing
  across `documents` and `library` needs several named preopens per call.
- **G16. Compile cost on small devices.** Engine config disables the module
  disk cache (Read: `Cargo.toml:44-46`). A Pi 3 recompiles the component on
  every Mother start (Inferred). Precompiled `.cwasm` bound to the approved
  digest would help, but it is a new artifact to verify.
- **G17. Semver-compatible WASI matching.** The follow-up to G5.
- **G18. Child-start ledger fact (D-40).** Matters for mutations that must
  never run twice. Read-only piece 1 does not care. Moves are idempotent if the
  API takes source and destination plus an expected source identity
  (inode+mtime or content hash).

## 2. The ZFS piece

**Answer: a Toy contract with a host-helper backend. Never a Child, and not
Mother core.**

- It cannot be a Child. Children are WASM only (D-03) and have no host power.
- It should not be Mother core. Mother must run unchanged on a Pi with ext4,
  and "the Child is filesystem-agnostic" applies to Mother too.
- It needs host privilege. `/dev/zfs` is not in a container by default
  (Guess). A small helper on the Unraid host, running as a dedicated user with
  `zfs allow` delegation (`snapshot`, `hold`, nothing destructive), exposes a
  narrow typed socket bind-mounted into the container. Mother's Toy adapter
  calls it. The helper is a trusted host adapter, the same category as the
  macOS Git confinement.

The Toy surface should be about recoverability, not ZFS:

```wit
interface versions {
  // previous versions of a path under a granted root; empty on plain ext4
  list-versions: func(root: string, path: string) -> result<list<version>, error>;
  // durable undo point before a destructive batch
  checkpoint: func(root: string, reason: string) -> result<checkpoint-id, error>;
}
```

- **A read grant on a dataset root is also a grant on its history.** OpenZFS
  serves `.zfs/snapshot/<name>/...` by explicit path even when `snapdir` is
  hidden (Guess, OpenZFS behavior, not tested here). cap-std confines `..` and
  symlinks, not mount crossings into the snapshot control directory
  (Inferred). So a person who loses access to a file, or whose file was
  trashed and purged, can still read old copies through any read grant on that
  root. Root enrollment must either refuse `.zfs` in the read path or record
  that the grant includes history. I missed this in my first draft; see
  Response to Astra.
- **Reads are almost free.** A dataset mountpoint exposes
  `.zfs/snapshot/<name>/...` read-only. `list-versions` maps to that
  directory. Restore is a read from the snapshot plus a normal `host-fs`
  write. No rollback (Guess: `.zfs` automount through a container bind mount
  needs a test on nica-unraid).
- **Delete becomes "checkpoint, then unlink".** The ZFS backend runs
  `zfs snapshot dataset@mct-<call_id>` and the ledger records the snapshot
  name next to the remove fact. The ledger then links each destructive
  effect to its undo point. On a Pi the same `checkpoint` uses a per-root
  trash directory. "Trash or snapshots" becomes a backend choice per root,
  chosen at grant time, behind one `remove` contract.
- **Pool health, quotas and dataset facts** are host observations. The helper
  hands them up as structured JSON and Mother records them as facts. The
  Child sees free space and quota, not pools.

**How it relates to Mother.** Two rules, both about Mother's own state:

1. Never roll back Mother's state dataset. The hash chain cannot detect a
   coherent rollback without an independent anchor (Read:
   `SECURITY-BOUNDARIES.md`, "Hash chain"). A ZFS rollback of Mother's
   appdata resurrects revoked grants and breaks D-14 "no resurrection". The
   helper refuses any operation on Mother's own dataset, and no grant may name
   it.
2. Snapshots of Mother's dataset are fine as backups for disaster recovery,
   where restore is treated as a new Mother under the operator gate, not as
   "undo".

No `rollback` Toy action in any piece. Rollback is dataset-wide and destroys
other people's newer work.

## 3. Multi-Mother: needed now?

**No.**

- One Mother on nica-unraid owns the vault grants. A Pi running the same Child
  is a second, independent Mother with its own roots and ledger. They do not
  need to talk for pieces 1-3.
- The UI talks to one Mother's ingress. Forwarding through a Mac Mother adds a
  hop and a second ledger while giving the user nothing.
- Federation (D-37, D-52) is built and proven. It does not need a filesystem
  app to prove it again.
- It becomes relevant when a bot on another Mother needs vault files. Then
  single-hop forwarding with a caller-node-scoped grant (which exists) is the
  path. That is later, and it does not change the Child.

## 4. Biggest risk to MCT

**Authority drifts to the edge, and the first real app proves MCT can be
bypassed.**

The path of least resistance is visible from the gaps. Caddy and Pocket ID
decide who is allowed in (G3, G6). A gateway process calls Mother over UDS as
one uid. The Child gets a read-write preopen (G7). The Child itself decides
whether "delete" means trash (G8). The trust check gets loosened to make the
share mount work (G2). Every step is small and reasonable. The result works,
demos well, and Mother is a rubber stamp. The ledger records "uid 1000 ran
host-fs" and cannot answer "did Doug or a bot delete this?". That is exactly
what MCT says it is not ("same-uid and localhost are not identities", D-08).

It matters more than any single gap because this is the first application.
Whatever shortcut it takes becomes the template for the next one.

The guard is cheap if it is decided now:

- Identity is verified by Mother, not asserted by the edge.
- Every mutation is a Toy call with its own grant check and ledger fact.
- Read-only is the only level that uses a raw preopen.
- Any trust-check exception is an explicit grant attribute in the ledger.

## Sequencing I would propose

1. **Piece 1 on a Pi first.** Linux arm64 is supported today. Run the host-fs
   Child (repo-report shape, `list`/`stat`/`read-range`) against a local
   folder through the existing directory-read Toy. Measure call latency and
   ledger growth per browse session. This proves the Child and the API without
   waiting on G1-G3.
2. **In parallel, unblock nica-unraid.** x86_64 build and image (G1), the
   Doug decision on G2, the D-entry for G14, the ingress adapter (G3), and the
   person in the grant (G6).
3. **Piece 1 on nica-unraid** with the *same approved artifact digest* as the
   Pi. That is the honest proof of "the same Child runs on both".
4. **Before piece 2,** land the typed mutation Toy (G7/G8). Moves and renames
   are the first ops on it.
5. **Piece 3** adds `write` and `remove` behind `checkpoint` (ZFS section).

## Response to Astra

Astra's `docs/design/nic-118-host-fs.md` landed on
`origin/nicabar-nic118-fs-design` as `a1380d8` at 11:22 ET. I read all 894
lines after sections 1-4 above were written and pushed. Since then I changed
only G6's tag and added the `.zfs` history bullet. Both changes are marked in
place.

Astra's doc is the stronger design document. It has the WIT sketch, a mutation
state machine with a checked TLA+ model and two negative controls, and a much
fuller hostile-input list (hard links, nested mounts, special files, reserved
trash paths). This response covers where we differ.

### Where we agree

- **Multi-Mother: no.** Same answer, same reasons. Astra adds a distinction I
  lacked. Multi-Mother and multi-Vision are different questions, and many
  users do not need many Visions.
- **Standard read-only WASI for reads, typed host operations for
  mutations.** We both found the `FsPerms::{ReadOnly, ReadWrite}` collapse
  independently, and both rejected interposing `wasi:filesystem`. Astra's
  candidate A vs B table states it more clearly than my G7/G8.
- **The grant has no person in it, and the edge is not the authority.**
  Astra's G04 and my G6 cite the same `ToyGrantSubject` fields. Astra was
  right to tag it blocks piece 1, and I changed my tag.
- **The D-58 runtime split is real.** Astra's G03 and my G11 cite the same
  line in `passkey-admin.md`.
- **ZFS is a host helper behind a Toy. It is never a Child, and `/dev/zfs`
  never goes into Mother.** Same shape. We agree rollback is never an API.
- **Bounded `read-range` first, WASI 0.3 streams later.** Same sequencing.
  We both note that `component-model-async` in Cargo is not 0.3 support.

### Where I disagree

**1. Piece 1 has too many prerequisites.** Astra tags twelve gaps as blocks
piece 1 and puts three preparatory pieces (0a, 0b, 0c) ahead of it. They
include signed per-call receipts from NIC-110 (G13), a full backup and
restore drill (G09), and log routing and retention (G08). A read-only grant on
a read-only mount needs none of these to be usable or safe. The ledger already
records every admitted call. Losing Mother state means a re-bootstrap, and the
files are untouched. I would hold piece 1 to the x86_64 image, the root trust
decision, an ingress, the person in the grant, and ranged reads. Receipts,
backup drills and log routing belong before mutations (piece 4), when they
protect something. The risk of the long list is that piece 1 slips, and the
pressure to ship produces exactly the edge shortcut in my section 4.

**2. 4 KiB read chunks with a signed receipt per chunk do not work for real
files.** Astra's limits table proposes 4 KiB raw chunks, and its receipt
section gives every chunk its own receipt. A 10 MB PDF is then 2,560 Child
invocations, 2,560 sets of ledger facts and 2,560 signatures (arithmetic).
The whole 2.7 GB `documents` set is about 700,000 (arithmetic). Live ledger
rotation is off by default (my G13), so every download grows the ledger for
good. I have not measured bytes per call. If each call writes even 1 KB of
facts, the ledger grows by a quarter of the bytes served (Guess). The fix is
my G9. The Child authorizes and names the file. Mother streams the bytes from
the same cap-std handle under a short-lived content handle. The ledger gets
one fact per transfer with the final length and digest. That also matches
Astra's own rule that the HTTP adapter never serves the mount.

**3. Mother should verify the identity provider's signature, not trust an
adapter's assertion.** Astra's identity model has a relying-party service
mint a short-lived assertion that Mother accepts "from its configured
authenticated adapter." That makes the adapter an identity authority. Anyone
who controls it can mint any principal. If Mother checks the Pocket ID token
signature against a pinned issuer key recorded in the ledger, the adapter
only carries tokens and cannot invent people. The browser session handling
Astra describes (cookies, CSRF, state, nonce) still belongs in the adapter.

**4. Snapshot creation should come before trash, not after.** Astra defers
snapshot creation and says delete must prove that a snapshot holds the exact
pre-effect object. A periodic snapshot cannot prove that, as Astra notes. A
snapshot taken by the Toy immediately before the effect, named after the
operation ID and recorded in the same intent fact, proves it by construction.
It needs only `zfs allow snapshot` for the helper user, which is not
destructive. The per-root trash stays the Pi backend. On nica-unraid it
removes the trash directory, the trash-in-root filtering problem, and the
trash quota.

**5. Pi first, not third.** Astra puts Pi qualification after the container
and after upgrades, and requires trusted AOT with no compile fallback. Linux
arm64 is the supported target today and Linux x86_64 is not (my G1). A Pi 4
such as nicamaster can JIT-compile the component now. Running piece 1 there
first proves the Child, the API and the call costs while the x86_64 image and
the Unraid decisions are pending. The no-compile AOT rule matters for a Pi 3
and can stay a later gate.

### What Astra missed

- **The existing root trust check refuses Unraid shares (my G2).** This is
  the most concrete piece-1 blocker in the code.
  `resident_directory_read_host_adapters` calls `trusted_directory`, which
  refuses any directory not owned by root or Mother's uid, and any group- or
  world-writable directory unless it is root-owned and sticky. Astra lists
  "which UID/GID" as an open operator question. No choice of UID fixes a
  `0777` dataset root. Doug must decide on SMB permissions, or MCT must add an
  explicit ledgered exception, before the first read works.
- **A read grant exposes the dataset's snapshot history through `.zfs`.**
  Neither of us had this at first; I added it to my ZFS section after reading
  Astra's reserved-path discussion. Astra correctly requires trash and staging
  to be unreachable through the read preopen. The same rule applies to
  `.zfs/snapshot`, which holds every old and purged copy. Root enrollment must
  refuse it or record that the grant includes history.
- **The x86_64 slot is deliberately closed.** Astra says to "add Linux x86-64
  packaging if needed." `scripts/test-release-platform.sh` fails if that
  target ever emits a release, and D-52 names arm64 only. Opening it is a
  decision plus a test change, not only packaging.
- **Every Child update re-issues every grant (my G10).** Grants match the
  exact `artifact_version`. Astra's grant binding keeps that exactness, which
  is right, but the ongoing-update cost is missing from the long-lived-app
  analysis. Without a ledgered "supersede to digest X" step, each Child
  release means re-granting every person by hand.
- **The repo still forbids this deployment.** `finish-owner-gates.md` and
  `HANDOFF-MCT-FINISH.md` say no Unraid deployments are authorized. Astra
  notes that the request does not authorize a live deploy. Neither doc can
  proceed without a DECISIONS.md entry from Doug.
- **Mother's own dataset must be fenced from the ZFS helper.** Astra's G09
  correctly requires that a restore keeps revoked grants denied. The helper
  should also refuse every operation on Mother's state dataset, and no grant
  should be able to name it. A ZFS rollback there is a coherent rollback the
  hash chain cannot detect.
- **The UI library pick needs a maintenance check now.** Astra's first
  choice, Chonky, was last published to npm as 2.3.2, with registry metadata
  last modified 2022-04-12 (Measured: `npm view chonky version time.modified`).
  Astra already lists maintenance as a check. On that evidence it would
  likely fail, so the UI evaluation should start with a second candidate.

### What Astra caught that I missed

These are good, and I would adopt them as written:

- Guest-writable SMB bypasses and races Mother. Mother audits only what it
  mediates, and mutations should stay off on guest-writable roots.
- `preopened_dir` takes a path, so an enrolled root can be swapped between
  validation and open. A handle-preserving seam is needed (Astra G05).
- Standard WASI file operations are not charged to the invocation budget.
  Only custom host calls are (Astra G14).
- Readiness in `status.rs` mostly reports whether Iroh is bound (Astra G07).
- Pocket ID is an OIDC provider, not a Caddy `forward_auth` endpoint, so Pi
  Grok's edge sketch needs a relying party either way.
- Missing-mount failure. Docker must not create an empty directory where the
  dataset should be.

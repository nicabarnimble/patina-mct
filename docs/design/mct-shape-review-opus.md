# MCT shape review (Opus): prior art against the code, and the path to done

Status: review and recommendation. This is not a decision. It changes no code,
no decision entry, and no other branch.

Baseline: `origin/main` at `bc48d1e` (PR #108). The design branches
`nicabar-nic110-design`, `nicabar-nic118-fs-design`,
`nicabar-nic105-drift-report` were read with `git show`, not merged. The
input is Doug's held note of 2026-11-06 (`HELD-NOTE.md`, untracked in this
checkout).

Goal restated. Doug wants a decentralized, OS-style network where anything can
be built and everything is governed. This review asks whether MCT's code
already enforces the reason it exists, which prior-art borrows actually fit,
whether to stand on wasmCloud, and the fastest honest route to a small,
falsifiable "done".

Evidence labels used below. **Read** means I read the cited source line at
`bc48d1e`. **Doc** means a checked-in evidence or design document says so and
I did not rerun it. **Public** means public knowledge of another project that
I did not verify against its source in this session. **Guess** means an
estimate or prediction.

## Summary

1. The "why" is mostly enforced inside a Child and mostly not enforced around
   Mother. A Child really does start with nothing. But every Mother that runs
   Children is governed by whoever holds the Unix UID, and the Mother that is
   governed by a person (the D-58 passkey root) refuses to run Children at all
   (`crates/mct-daemon/src/startup.rs:668-675`). Toy grants are also not
   time-bounded by type. Those two facts are the whole gap between the note
   and the code.
2. Workers bindings and Fuchsia use/expose are already true. Fuchsia offer is
   partly true (no mandatory expiry, no person issuer). Service bindings exist
   only as a Watch-specific call-out. The Durable Object single writer is
   already true per Mother. "Vision as realm" is the wrong borrow. In MCT the
   realm is a Mother and her Children. A Vision is a membership boundary
   between peers.
3. Keep MCT's runtime. Do not build on wasmCloud's host or providers. Stay
   WIT-compatible with wasmCloud instead.
4. Done is seven pass/fail checks, all run on Pi 3 Mothers in one governed
   profile. Today none is fully proven in that profile. Five are partly
   proven, two are not.
5. Fastest honest path is 16 Pi build sessions (Sol high), about 10 to 12
   calendar days with two lanes. The critical path is the governed runtime
   cutover (D-08/D-58 into the Child runtime). It is also the biggest risk.

## 1. Does the "why" match what the code enforces?

Short answer: inside the Child, yes. Around Mother, no.

| Claim in the note | Verdict | Evidence |
| --- | --- | --- |
| A Child starts with nothing (D-01). | **True**, with one open exception. | Read. `validate_wit_host_imports_for_adapters` refuses any import without a concrete adapter, "generic stubs are not permitted" (`crates/mct-daemon/src/wasm.rs:1878-1906`). The WASI allowlist has no sockets and no HTTP (`wasm.rs:1910-1930`). The WASI context starts empty and each preopen is installed only after `AuthorizedToyCall` admission (`wasm.rs:380-436`). Exception: the Git Toy can still reach native execution through Child-writable `.git` metadata (`docs/TODO-security-and-finish.md`, H1, "H1 is not closed"). |
| Every folder, secret or network effect traces to an explicit grant. | **True.** | Read. Each WIT adapter is built from an `authorize_resident_toy` result (`crates/mct-daemon/src/daemon/resident/execution.rs:579-676`). There is no network Toy, so egress is absent rather than governed (D-39 "not built"). |
| ...scoped... | **Partly.** | Read. "Optional scope fields are wildcards when absent" (`crates/mct-kernel/src/toy.rs:49-60`). A grant with no `resource_id` matches every resource of that Toy. |
| ...time-bounded... | **False at the grant level.** | Read. Toy grants use `ExpiryRequirement::Optional`, "Absent means the window is open. Toy grants use this." (`crates/mct-kernel/src/grant_check.rs:19-21`, `toy.rs:868-881`). The secret and Slate grant builders write `expires_at: None` (`crates/mct-daemon/src/daemon/cli_admin.rs:361-397`, `473-519`). Only the per-call token is bounded, by the call deadline (`toy.rs:883-897`). Day-key admin grants do have mandatory expiry (D-58). |
| ...revocable... | **True for new admissions.** | Read. `ToyGrantRevoked` is a ledger kind (`crates/mct-kernel/src/observation.rs:43`) and revoked grants deny (`toy.rs:1231`, `toy.rs:1443`). Already-installed preopens live until the call deadline (D-G7, `docs/SECURITY-BOUNDARIES.md`). |
| ...written to the hash-chained ledger before the effect (D-04, D-05). | **Partly.** | Read. Toy, Watch and admin-key authority is canonical in the ledger. Child approvals, assignments and peer bindings are still fields of `config.json` (`crates/mct-daemon/src/config.rs:21-32`), and D-04 says so. A Child launch writes no BeforeEffect fact, so power loss can double-run an unkeyed call (`docs/OPEN-QUESTIONS.md` item 1, D-40 still proposed). |
| Mother alone admits work (D-09). | **True for admission. False for who governs Mother.** | Read. Children are in-process Wasmtime instances behind the call pipeline. But the Mother that runs Children takes authority mutations over a Unix socket checked by peer UID (`crates/mct-daemon/src/control.rs:201-210`, `control.rs:543`). D-08 ("Iroh is the only admin transport") is "decided, not built". The passkey Mother refuses the Child runtime: "passkey administration cannot run through legacy UID/offline runtime" (`startup.rs:671-674`), and its signed command adapter accepts only `/status` (`crates/mct-daemon/src/passkey_admin.rs:25-29`). The NIC-63 mesh job minted its grant through that UID socket (`crates/mct-daemon/examples/federation_host.rs:957-990`, `POST /watch/supporting-grant`). |
| Several Mothers cooperate over Iroh by Vision, with no cloud or bus. | **True.** | Read. Wrong Vision is a typed denial in the kernel and on the wire (`crates/mct-kernel/src/call/internal.rs:211`, `crates/mct-iroh/src/lib.rs:1404`). Doc. Physical runs on nicamaster, nicanode0 and nicanode1 (`docs/nic63-acceptance-suite.md`, `docs/nic63-mesh-job.md`). |
| Rebuilt away from "a manifest need became a grant" and HTTP fall-through (D-39). | **True.** | Read. `[needs]` in `crates/mct-repo-report/child.toml` grants nothing. Grants arrive by a separate admin mutation. No HTTP import is in the allowlist. |
| Pi 3 is the floor. | **Partly, and the note understates it.** | Doc. The admin-only Mother is proven on nicanode0 (`docs/nicanode0-install-evidence.md`, 18,272 KiB idle RSS). The note says "full Child runtime on Pi 3" is not proven. In fact the NIC-63 mesh job ran the real `mct-repo-report` Child on nicanode0 (a Pi 3B) under a directory-read grant, and refused it after expiry with zero runtime facts (`docs/nic63-mesh-job.md`). That run used a test-harness Mother, compiled the component on the Pi, and measured no memory. |

What this means. The note's sentence "every folder, secret, or network effect
must trace to an explicit, scoped, time-bounded, revocable grant" holds for the
Child's side of the boundary. It fails on two words. "Time-bounded" is not a
type rule for Toy grants. And no grant in a running Mother today traces back to
a person. It traces back to a UID. MCT's distinctive claim, that a person
governs and the ledger proves it, is the part not yet in the code.

## 2. The borrows, one by one

| Borrow | MCT today | Smallest change to make it true | Verdict on the borrow |
| --- | --- | --- | --- |
| **Workers bindings** (a worker reaches only named handles = Toys) | **Already true.** Import allowlist plus grant-gated adapters (`wasm.rs:1878-1930`, `execution.rs:579-676`). No default outbound network. Exception H1 (Git Toy). | None for bindings. Keep the Git Toy out of the governed profile until H1 closes. | Right. Already MCT's shape. |
| Workers "don't borrow config-as-the-only-grant" | **Still present.** Approvals, assignments and peers live in `config.json` (`config.rs:21-32`). | Read those three from canonical ledger facts on the governed profile (FINISH item `canonical-authority`). That lands inside piece 3 below. | Right to refuse it. MCT still has to finish refusing it. |
| **Service bindings** (Child-to-Child with no public address, Mother-mediated) | **Partly true.** A Child's `wasi:messaging` send becomes a new call that re-enters Mother's full pipeline as `ChildCallOut`, gated by a `watch-callout` grant, depth 1 (`execution.rs:635-660`, `crates/mct-daemon/src/daemon/resident/pipeline.rs:112-130`, `crates/mct-kernel/src/watch.rs:14`). But the payload is hard-wired to file-watch events (`pipeline.rs:150-200`). Mother-to-Mother forwarding is the cross-host form and is built (D-37). | Generalize the call-out: grant `resource_id` names the exact target operation, the payload is opaque bounded JSON, the callee runs under its own grants, depth stays 1. Not needed for done. Do it after. | Right, with one MCT rule Workers lacks. Mother mints and ledgers the binding (D-21). A binding in a config file is not enough. |
| **Durable Object single writer plus input/output gates** (model for the NIC-118 mutation journal) | **Already true per Mother.** One writer per ledger, a second writer is refused (proven on nicanode0). `ResidentLedgerWriter` serializes authority work (`crates/mct-daemon/src/daemon/resident/observation.rs:325-336`). Append-before-effect (D-05) is the output gate for Toy effects. Missing for Child launch (D-40). | Do not add a second journal for NIC-118. Write mutation intents as BeforeEffect facts in the existing Mother ledger, take a per-root exclusive lease inside the existing actor, and refuse mutations on any root where Mother is not the only writer. | Right in granularity. Wrong in one assumption. A Durable Object is safe because nothing else touches its storage. NIC-118's root is guest-writable over SMB (`nic-118-host-fs.md` §1). Separate the writers first. A journal cannot serialize a writer it never sees. Also do not borrow object migration. Moving a root's writer to another Mother is failover, which D-09 forbids. |
| **Fuchsia `use`** (Child `needs` grants nothing) | **Already true.** `[needs]` in `child.toml` is a request. Acquisition pins manifest bytes to approval (L16). | None. | Right. |
| **Fuchsia `offer`** (ToyGrant = runtime offer) | **Partly true.** Offers exist and are revocable. Expiry is optional, scope fields default to wildcards, and the issuer is a UID. | Flip Toy grants to `ExpiryRequirement::Mandatory` (`grant_check.rs:21`, `toy.rs:873`). Require `resource_id` for filesystem Toys. Require the issuing day key to chain to the passkey root (piece 3). | Right. MCT's offer must be stronger than Fuchsia's: expiry, person, ledgered revocation. The note says so too. |
| **Fuchsia `expose`** | **Already true.** `[child.contract] allow` names the exported operations. Peer hello publishes only the admitted Vision's view (L7). | None. | Right. |
| **Vision as realm** (realm = parent plus subtree, hard boundary, cross-Vision needs explicit export) | **Hard boundary already true.** Wrong Vision denies (`call/internal.rs:211`). One Vision per Mother for this release (`docs/FINISH.md`). Cross-Vision is excluded, not exported. | No code change. Change the vocabulary. | **Wrong as stated.** A realm has a parent that offers down. A Vision has no parent. It is a sharing boundary among independent peer Mothers (`layer/core/what-is-mct.md` §Vision, D-52 "no implicit transitive trust"). Giving a Vision a parent creates a second grantor (D-31) and a place for central rules (D-52). The realm in MCT is a Mother and her Children: Mother is the parent, offers ToyGrants, and is the hard boundary. Keep "cross-Vision needs an explicit export" as a later rule for NIC-104 multi-Vision work. |
| Fuchsia isolated `storage` (trash and staging outside the read preopen) | **Not applicable yet.** The only filesystem grant is read-only directory-read at `/project` or `/input` (`execution.rs:676-680`). | Belongs to NIC-118 pieces 5 and 6. | Right, when writes exist. |

Rejected in the note and still rejected here: V8 isolates as the Child (D-03
keeps one confinement boundary), default outbound internet (D-39), and
Component Manager (MCT needs expiry, a person and ledgered revocation, which a
static topology cannot carry).

## 3. wasmCloud host and providers, or MCT's own runtime

**Pick: keep MCT's own runtime.** Borrow wasmCloud's interfaces, not its host.

What wasmCloud is (Public, not verified against source this session). A Rust
host on Wasmtime. Hosts join a lattice over NATS. Components call capability
providers over wRPC on NATS. Providers are native executables the host
launches. wadm reconciles declarative application manifests across hosts. Links
in those manifests connect a component to a provider with configuration. An
optional external policy service is consulted over NATS.

| | Adopt wasmCloud host and providers | Keep MCT's runtime |
| --- | --- | --- |
| What you gain | A provider catalogue (HTTP server, key-value, messaging, blob). Declarative multi-host deployment and scaling (wadm). OCI distribution. A community. | Every existing guarantee and every existing proof stays valid: ~127k lines of authority, ledger and federation code (Read, `wc -l` over `crates/`), the `docs/finish-checks.json` inventory, and NIC-63 physical evidence. |
| What breaks | D-03. Providers are native processes with OS authority, the same class of hole as H1, once per provider. D-05. The effect happens inside a provider after a NATS hop, so Mother cannot append before it unless Mother is the provider, at which point MCT is rebuilt inside wasmCloud. D-52 and "no bus holds the rules". wadm and NATS become the shared governance plane. Links are config-as-grant: no expiry, no person, no ledgered revocation. The note rejects exactly that. Iroh identities and signed bindings would have to be rewired onto NATS accounts. Pi 3 floor. A NATS server plus host plus one process per provider on 905 MB is unmeasured (Guess: several times the 18 MiB idle Mother). | No provider catalogue. Each Toy is MCT work. No wadm-style declarative fleet deployment. MCT tracks Wasmtime and WASI upgrades by hand (D-47, PR #104). "Anything can be built" grows at the speed of the Toy catalogue. |
| Who pays | Everything already proven is re-proven. The governance claim that distinguishes MCT is lost or rebuilt. | Toy velocity. |

Why this pick. The product is the governance, not the host. wasmCloud is good
at "anything can be built" and puts the rules in the lattice. MCT's reason to
exist is putting the rules in each node, before each effect, with a person at
the root. Building on wasmCloud means keeping MCT's ledger and admission
anyway and losing the confinement boundary under every provider.

How to keep "anything can be built" without the host:

- Keep Child imports on standard WIT that wasmCloud also uses. MCT already
  links `wasi:keyvalue/store@0.2.0`, `wasi:messaging/producer@0.2.0` and
  `wasi:logging/logging@0.1.0` (`wasm.rs:1884-1891`). A Child written to these
  runs in either world.
- If a wasmCloud lattice ever needs MCT, it is a client of a Mother's
  authenticated ingress, as D-03 already says for native ecosystems. Never the
  other way round.
- Grow the Toy catalogue from the small-device designs (NIC-111 to NIC-116)
  only after done. Each Toy is one grant shape plus one adapter.

## 4. Done and proven: seven checks

All seven run in **one governed profile** on Pi 3B Mothers (nicanode0 and
nicanode1), with nicamaster only as the builder. The governed profile means a
passkey-rooted ledger (D-58), no UID or Unix-socket mutation path, and the Child
runtime enabled. A check that passes only on the legacy UID profile is
"partly". Budgets below are proposals for Doug to change.

| # | Check (pass/fail) | Status today |
| --- | --- | --- |
| C1 | **Real Child, real Pi 3.** An approved Child (`mct-repo-report`) runs as a call on an installed, boot-started Pi 3B Mother. Peak Mother RSS during the call is at most 96 MiB and the first cold call returns within 2 s. Otherwise fail. | **Partly.** Ran on nicanode0 in a test-harness Mother with on-device compile (`docs/nic63-mesh-job.md`). The installed Pi service is admin-only. No memory figure for the full runtime. |
| C2 | **Governed by a person, with expiry.** The Child's approval and its ToyGrant are written by day-key-signed commands that chain to the passkey root, and the ledger fact names that chain. A grant without expiry, or any mutation over a Unix socket, is refused with the ledger unchanged. | **Not.** `startup.rs:671-674` refuses passkey ledgers in the runtime. `passkey_admin.rs:25-29` admits only `/status`. Toy expiry is optional. |
| C3 | **Nothing ambient.** A hostile fixture Child that imports sockets or HTTP, asks for an ungranted path, escapes its root by `..` or symlink, calls without a grant, or sends a forged caller principal over the wire, gets zero effects and either an instantiate refusal or a typed denial with zero runtime facts. | **Partly.** Import refusal and preopen gating are built and unit-tested (`wasm.rs:1878-1930`, `380-436`). Forged-origin refusal is built (H2). Expired-grant refusal is proven on nicanode0. No hostile Child has run on a Pi. |
| C4 | **Receipts.** Every admitted call (success, denied, failed, timed out) yields a Mother-signed receipt that an offline tool verifies against Mother's enrolled key. It binds call ID, outcome, result digest, caller principal and the ledger sequence and hash. A flipped byte fails verification. | **Not.** The hash chain exists. Signed per-call receipts are the NIC-110 draft only (`nic-110-shared-contract.md` §3). |
| C5 | **Revocation.** The person revokes the grant. The next call, under the same or a new call ID, is denied with zero runtime facts. An in-flight preopen ends by its call deadline. After SIGKILL and restart, replay does not resurrect the grant. | **Partly.** Day-key revoke is proven on nicanode0 (admin-only). Receiver-side peer revocation denials pass in the NIC-63 suite. ToyGrant revocation is unit-tested. Never person to Child on one Mother. |
| C6 | **Two Mothers.** Mother A's call runs on B only under B's own grant. B's expiry or revocation refuses A with zero runtime facts on B. With A down, B's local work continues. Both ledgers verify under their own identities. | **Partly.** Proven physically on the legacy profile (`docs/nic63-mesh-job.md`, `docs/nic63-acceptance-suite.md`). Must be rerun once peers are admitted by signed commands. |
| C7 | **Power-cut honesty.** Power-cycle a Pi Mother during an admitted call. On restart the ledger verifies, the call settles as `execution_interrupted`, no call ID (keyed or unkeyed) launches twice, and a revoked grant stays revoked. | **Partly.** SIGKILL recovery is proven on Pis (NIC-63 slice 3). Power loss is not. The known double-run (`OPEN-QUESTIONS.md` item 1) is closed only for keyed calls (D-51). D-40 is proposed. |

Count: 0 proven, 5 partly, 2 not.

## 5. Fastest honest path

A session means one Pi build session on Sol high that ends in one reviewable
PR with tests. Calibration (Read, `git log --first-parent`): between
2026-11-01 and 2026-11-05 `main` took 15 PRs, of which 7 were feature, fix
or physical-evidence PRs, about 1.4 per day. Calendar days assume owner review within a
day. Estimates are Guesses.

Two human gates are fixed. Doug's live 1Password tap for the governed profile
(piece 3) and the physical power cut (piece 8). Everything else uses the
virtual authenticator and the existing fleet harness.

| Order | Piece | Pass/fail check | Sessions | Days |
| --- | --- | --- | --- | --- |
| 1 | **Done lever.** One command, extending `scripts/federation/suite.sh`, that runs C1 to C7 and prints pass/fail per check. Include the hostile fixture Child for C3. | Pass iff it runs against `bc48d1e` on nicamaster plus nicanode0 and reports each check red with a named reason, and C6 green on the legacy profile. | 2 | 1.5 |
| 2 | **Mandatory Toy-grant expiry.** `ExpiryRequirement::Mandatory` for Toy grants. Secret and Slate builders take an explicit expiry. Filesystem grants require `resource_id`. Open-ended legacy grants deny (D-02). | Pass iff a grant with no expiry denies with a typed reason and the NIC-63 mesh job still passes. | 1 | 0.5 |
| 3 | **Governed runtime cutover.** One resident process opens the passkey ledger (`ResidentLedgerWriter::open_passkey_admin` already exists) and serves `mct/admin/1`, `mct/hello/0` and `mct/call/0`. Migrate only the routes done needs: artifact stage and approve, assign, directory-read grant, grant revoke, Child revoke, peer add and revoke. Read approvals, assignments and peers from ledger facts on this profile. Refuse every Unix-socket mutation and offline path on a passkey ledger. Leave every other admin scope unmigrated and unreachable on this profile. | Pass iff C2 is green on macOS and on one Pi, a Unix-socket mutation against the governed Mother is refused with the ledger bytes unchanged, and Doug's live tap issues the day key. | 5 | 4 to 5 |
| 4 | **Person principal.** Calls carry the authenticated principal from ingress (day-key signer for a person, signed binding for a peer), never from the wire. Grants may narrow to one principal. The principal is recorded in ledger facts. | Pass iff two principals with different grants stay isolated, the other principal gets zero runtime facts, and a forged wire principal is denied. | 2 | 1.5 |
| 5 | **Signed receipts** (lane B, parallel with 3). The NIC-110 §3 subset: a purpose-bound Mother key, a receipt after the terminal fact is durable, and an offline verifier. | Pass iff C4 is green on macOS for all four outcomes and the tamper case fails. | 2 | 1.5 |
| 6 | **Child-start fact, D-40 option D** (lane B). The writer admits launch and appends `ChildInvoked` first. A second admission of the same call ID is refused. Recovery reads the ledger first. Needs Doug to accept the seven proposed defaults in `OPEN-QUESTIONS.md`. | Pass iff the existing SQLite-rollback regression passes for unkeyed calls as well as keyed calls. | 2 | 2 |
| 7 | **Pi 3 measurement on the installed governed Mother.** Deploy pieces 2 to 6 through the Nix closure to nicanode0 and nicanode1. Run C1, C3, C5, C6. | Pass iff C1, C3, C5 and C6 are green. If C1 fails only on memory or cold time, do 7b. | 1 | 1 |
| 7b | *Only if C1 fails.* Trusted `.cwasm` loading: built on nicamaster, bound to the approved component digest and engine fingerprint, no compiler on the Pi. | Pass iff C1 is green and a wrong-engine or tampered `.cwasm` is refused with no on-device compile. | (+3) | (+3) |
| 8 | **Power cut and final run.** Doug power-cycles nicanode0 mid-call, then the lever runs all seven. | Pass iff C1 to C7 are all green in one run, with sanitized evidence checked in. | 1 | 1 |

**Total.** 16 sessions, or 19 if 7b is needed. About 10 to 12 calendar days
with lane B (pieces 5 and 6) running beside piece 3. About 15 days fully
serial.

**Critical path.** 1, 2, 3, 4, 7, 8. That is 12 sessions. Piece 3 alone is 5
of them.

**Biggest risk: piece 3, the governed runtime cutover.** D-08 has been
"decided, not built" since 2026-10-14. It touches the control plane where the
unexplained readiness flake R1 lives. And it invites migrating all 20
admin scopes (`AdminKeyScopeV1`, `crates/mct-kernel/src/admin_key.rs:24`)
instead of the seven routes done needs. If
piece 3 grows past 5 sessions, cut scope back to those seven routes. Do not
extend the timeline.

Second risk: the full runtime's memory on a Pi 3 is unmeasured. That is the
3-session contingency in 7b. I recommend measuring before building `.cwasm`,
because a 336-byte component compiled on the Pi in 33.9 ms at 10.9 MiB peak
(`docs/nicanode0-install-evidence.md`) and the real `mct-repo-report` Child
already compiled and ran on nicanode0. The NIC-110 draft treats "no compile on
Pi 3" as a requirement. If Doug wants that regardless of measurement, add 7b
to the critical path (19 sessions, about 13 to 15 days).

Out of scope until done passes: H1 Git Toy closure (keep Git out of the
governed profile), the generalized service binding, multi-Vision, NIC-118
writes, guardian recovery, and the remaining admin scopes.

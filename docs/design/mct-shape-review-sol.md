# MCT shape review — Sol

**Review baseline:** `bc48d1e0d721d1531a58192a69ef65ffd234882f`, branch
`nicabar-mct-shape-sol`. Input: Doug's supplied `HELD-NOTE.md` (untracked;
intentionally not included in this commit). This is an architectural recommendation,
not a ratified scope change or a claim of fresh hardware acceptance. No code changed.
The parallel review checkout and branch were not inspected or modified. Settled
intent is in [DECISIONS.md](../DECISIONS.md); source and named evidence below
separate what is built from what is decided.

**Recommendation:** keep MCT's runtime. Finish the existing authority system by
joining person-backed administration to useful application execution. Borrow named
capabilities, explicit service routes and single-writer effect gates; do not borrow
a cloud control plane, a component hierarchy, or an exactly-once promise.

## 1. Does the note's “why” match today's enforcement?

**Yes as a purpose; only partly as an end-to-end guarantee.** “Running code is not
authority” accurately describes the local WASM boundary. “Every effect has a
person-backed, time-bounded, canonical grant and receipt” does not describe the
whole deployed application path yet.

| Claim in the note | What this checkout actually does |
| --- | --- |
| A Child starts without host power. | **True for the supported confinement boundary.** [wasm.rs](../../crates/mct-daemon/src/wasm.rs), `MctWitHostImportAdapters::none`, `build_wasi_ctx` and `validate_wit_host_imports_for_adapters`, starts with no adapters/preopens and rejects unsupported imports, including HTTP/socket interfaces. A manifest request cannot install an adapter. Standard clocks/randomness are supported WASI services, not a promise that every observable host interaction is a separately issued Toy. |
| Each capability is explicitly scoped and revocable. | **Substantially true.** [kernel toy.rs](../../crates/mct-kernel/src/toy.rs), `ToyGrantSubject`, `ToyGrantScope`, `evaluate_toy_grant_for_snapshot` and `AuthorizedToyCall::admit_effect_with_snapshot`, binds executable power to exact Child/artifact, scope, call and current Mother authority. Missing scope fields can be wildcards: “scoped” does not automatically mean least privilege. Private tokens are not guest bearer credentials. |
| Every grant is time-bounded. | **False as a universal statement.** `ToyGrantConstraints.expires_at` is optional; [grant_check.rs](../../crates/mct-kernel/src/grant_check.rs) explicitly distinguishes optional Toy expiry from mandatory day-key expiry. `toy_token_expiry` caps an executable token by the call deadline, but that does not expire the stored grant. [board-toygrant.md](board-toygrant.md), “Decided policy,” records mandatory Toy expiry, default 30 days, independent of the issuing day key; that policy is not universally enforced by these types/evaluators today. |
| Ledger truth precedes effects. | **True for existing required admission facts, incomplete for all authority and effect outcomes.** [observation lib.rs](../../crates/mct-observation/src/lib.rs), `append_durable_batch`, syncs before acknowledgement and poisons uncertain writers. [resident execution.rs](../../crates/mct-daemon/src/daemon/resident/execution.rs), `execute_resident_wit_child`, appends Toy authority evaluations before invoking WASM. However, `AuthorityStateV1` has no canonical Child approval/assignment or peer-binding collections. [control.rs](../../crates/mct-daemon/src/daemon/control.rs), `PreparedChildMutation`, still writes those Child decisions into config with incomplete observations. D-04's qualification remains real. |
| Mother alone admits work; losing Mother stops authority-dependent work. | **True at mediated admission, not an instantaneous kill of an already admitted effect.** [authority_order.rs](../../crates/mct-daemon/src/authority_order.rs), `MotherAuthorityOrderV1::admit_effect`, orders mutation against effect admission. [wasm.rs](../../crates/mct-daemon/src/wasm.rs), `delegated_preopen_survives_revocation_while_new_delegation_denies`, deliberately preserves the already-delegated WASI directory lifetime. Read-only preopens are invocation-bounded, not per-read revocable. Restart settles interrupted calls; it does not resume arbitrary durable computation (D-32). |
| Several sovereign Mothers cooperate over Iroh, not a central bus. | **True within the bounded federation.** [federation.rs](../../crates/mct-daemon/src/federation.rs) publishes eligible, Vision-scoped operations; [call/internal.rs](../../crates/mct-kernel/src/call/internal.rs) checks the authenticated hop, Vision and current binding. Shared Vision is not permission. [NIC-63 mesh evidence](../nic63-mesh-job.md) demonstrates independent executing nodes and ledgers. This is single-hop federation, not consensus or takeover. |
| Grants are tied to a person. | **Not on the complete application path.** [passkey-admin.md](../passkey-admin.md) and [passkey_admin.rs](../../crates/mct-daemon/src/passkey_admin.rs) provide passkey-rooted day-key issuance/revocation and signature-checked status admission. Application Toy subjects name a Child and optional caller **node**, not an authenticated person. `CallerIdentity.user_id` is not a delegation proof: peer v0 explicitly rejects it. [startup.rs](../../crates/mct-daemon/src/startup.rs), `classify_authority_startup`, refuses passkey ledgers through the legacy UID/offline runtime. That refusal is correct, but exposes the unfinished join. |

The integrated-Patina comparison is a reason to **rebuild** accepted responsibilities,
not copy defaults: D-39 and [migration-vocabulary.md](../../layer/core/migration-vocabulary.md)
record how `patinaToy` manifest needs could become power, whereas `mctToy` needs
explicit authority from `mctMother`. This review did not audit an external
`patinaMother` checkout; the comparison is repository-documented history.

Two corrections to the note's “facts on the ground” matter for the schedule:

- **A real Child has now run on the Pi 3.** [nic63-mesh-job.md](../nic63-mesh-job.md)
  records `mct-repo-report` executing on nicanode0 on November 6, returning the
  exact Cargo.lock digest and 587 packages. After its directory-read grant expired,
  B recorded `toy_grant_expired` with zero runtime facts for the refused call.
  [component.rs](../../crates/mct-repo-report/src/component.rs) actually reads the
  bounded host file through `/project`; this is not the tiny WASI probe.
- **That is not a resource qualification or a signed application receipt.** The
  older [Pi 3 admin smoke](../nicanode0-install-evidence.md) measured approximately
  18 MiB idle RSS, 431 ms ready and 3.645 ms append+sync p50, for an admin-only
  Mother. The mesh run establishes useful execution, not those same numbers for
  the full runtime. Its “executor attestation” is an observation/provenance field,
  not a cryptographic effect receipt. The committed JSON is a sanitized reduction;
  canonical ledgers remain private.

**Architectural consequence:** the missing work is not another sandbox. It is a
continuous provenance chain:

```text
trusted person enrollment -> authorized day key -> exact application grant
  -> locally verified call -> ordered effect admission -> durable outcome
  -> signed, independently verifiable receipt
```

The executing Mother must own each authority decision. The host OS, Mother binary,
Wasmtime and trusted Toy adapters remain the trusted computing base. “Everything
is governed” means every supported Child effect traverses that boundary; it does
not mean MCT governs a malicious host administrator or implements general
information-flow control. [SECURITY-BOUNDARIES.md](../SECURITY-BOUNDARIES.md)
explicitly excludes those stronger claims.

## 2. Which borrows fit, and the smallest honest change?

These are semantic borrows, not requests to import the platforms. The NIC-118
branch design packs mentioned in the input are not present in this checkout and
were not inspected. The filesystem recommendations below are independently
proposed proof obligations, not assertions about those packs' implementation.

| Borrow | Status today | Smallest change / architectural judgment |
| --- | --- | --- |
| **Workers bindings: named handles, not ambient access** | **Already true for implemented Toys.** `MctWitHostImportAdapters`, conditional linking and kernel-minted `AuthorizedToyCall` implement the essential boundary. | Preserve it. For the next capability, bind one host-chosen resource to one WIT import after grant evaluation; never let guest arguments select a broader root or credential. Add its allow/missing/wrong-resource/expired/revoked tests. No new binding framework is needed to make this principle true. A binding alone is **not** a grant, and default outbound Internet/config-only authorization/V8 replacement are wrong borrows. |
| **Service bindings: private Mother-mediated Child-to-Child calls** | **Partly true.** Private WIT routing and peer forwarding exist. [pipeline.rs](../../crates/mct-daemon/src/daemon/resident/pipeline.rs) constructs governed nested calls for the specific Watch messaging path. But `validate_wit_host_imports_for_adapters` offers no general Child service-call import or portable recipient-bound handle. | Add one fixed-target service-call Toy: exact destination service/WIT operation, recipient Child identity, Vision, expiry and payload bounds. Mother constructs the nested call, preserves parent attribution/deadline, reauthorizes the callee, and records both decisions. Start local, deny self/cyclic recursion beyond a fixed depth, then reuse single-hop forwarding. Do not expose a URL, raw Iroh or a general unrestricted dispatcher. D-10's “holding the handle” must mean a Mother-issued, recipient-proved capability, not a copied string. |
| **Durable Object single-writer plus input/output gates** | **Partly true.** The canonical ledger has an exclusive writer; `MotherAuthorityOrderV1` orders admission against authority changes. This is not a per-resource host-filesystem mutation journal. [toy.rs](../../crates/mct-daemon/src/toy.rs), `call_admitted_toy`, returns started/completed observations after executing its backend; buffered reports are not a crash-reconciliation protocol. | For one output root, give mutations one owner and a durable operation identity. Sync intent before touching files; sync outcome before returning success. A retry with the same identity/content returns its original result; changed content conflicts. Recover an interrupted mutation by inspecting recorded staging/content identities, never blind re-execution. Serialize the mutation's resource, not all Mothers or every Child. Keep staging/trash outside **all** guest-readable roots. A file lock on the ledger alone does not establish this property. |
| **Fuchsia `use` / `offer` / `expose`** | **Partly true overall:** `use` is already represented by manifest `needs` plus actual WIT imports; runtime `offer` exists for supported ToyGrants; explicit `expose` policy is incomplete. [federation_callable_surfaces](../../crates/mct-daemon/src/federation.rs) advertises allowed operations of ready, approved, assigned Children, not a separate narrowly approved export route. | Keep `needs` non-authoritative; validate its correspondence to actual imports. Store one canonical route record identifying provider, consumer, resource, operation and Vision. A ToyGrant authorizes that runtime route; a distinct publication/export permission controls disclosure/reachability of a Child service. This can be flat records and negative tests, not CML or Component Manager. Static offers alone are wrong: they lack Mother's person, expiry, revocation and ledger semantics. |
| **Vision as realm** | **Partly true as an isolation analogy; wrong as a literal equality.** Vision scopes grants, assignments, admission and publication. A Vision can span independent Mothers; Fuchsia's parent/subtree ownership is not MCT's sovereignty model. There is no general cross-Vision export mechanism in this release. | Call Vision a **sharing/authority domain**, not a component tree. Keep cross-Vision routing denied and test that denial. An explicit bilateral export/import grant is the smallest future bridge, requiring both domains' local decisions; do not implement that bridge to finish the current one-Vision release. Never infer rights from membership or make one Mother a component-manager parent of the others. |

The useful common model is **request / route / authorize / execute / prove**.
Do not collapse those five facts into a manifest. In particular, isolated storage
is a reachability property: hiding `.staging` in a listing is insufficient if an
existing preopen can still address it.

## 3. wasmCloud hosts/providers or MCT's own runtime?

**Pick: keep MCT's own runtime, using upstream Wasmtime/WASI. Do not adopt
wasmCloud as the execution foundation.**

This compares the host/provider plus NATS/wadm lattice option described in the
input, not a fresh audit or benchmark of upstream wasmCloud. wasmCloud is not
incapable of local policy or decentralized deployment; those possibilities do
not supply MCT's existing contracts for free.

| Choice | Concrete gain | What breaks or remains our responsibility |
| --- | --- | --- |
| Adopt wasmCloud hosts/providers | Reuse a broader component/provider ecosystem, host operations and deployment machinery instead of implementing every host integration. | Existing private Rust authority tokens cannot become authoritative merely by crossing a host/provider RPC. Every protected provider invocation would need a locally verified, single-use admission bound to the exact grant revision, resource, deadline and journal identity. Link configuration is not canonical issuance. Delayed messages, provider retries and lost replies add authority/effect uncertainty boundaries. MCT would need to preserve its ledger actor and rewrite/integrate runtime admission, revocation, receipts, lifecycle and Iroh routing around the new host; unwrapped providers would bypass the guarantee. A normal NATS/wadm deployment also adds operational dependencies and an **unmeasured** Pi 3 footprint. Existing NIC-63/Wasmtime proofs would not qualify that stack. |
| Keep the existing runtime | Preserve the in-process pure kernel → typed admission → Toy boundary, existing local ledgers/Iroh paths, resource limits and physical Child evidence. Avoid a migration before proving the product. | MCT still owns component compatibility, host adapter safety, storage recovery, packaging and operator tooling. Today the import table is deliberately small and partly application-specific; [wasm.rs](../../crates/mct-daemon/src/wasm.rs) admits only named versions/interfaces, and native Git confinement fails closed on Linux ([toy.rs](../../crates/mct-daemon/src/toy.rs)). Arbitrary shell, general HTTP, Xcode and native/JVM local Children do **not** become available by declaring a need. “Anything can be built today” remains false. |

The deciding trade is **ecosystem breadth versus preserving the authority/effect
boundary already built and exercised**. Adopting hosts/providers now trades a
known integration gap for a runtime migration plus the same integration gap.
It does not remove the hard work.

Reshape the retained runtime toward ordinary WIT contracts and small resource-bound
adapters, not another application-specific branch for every product. Reuse
upstream libraries behind those adapters where useful; do not introduce a second
provider authority plane. The closed catalog should mean “no unadmitted effect,”
not “freeze the set of applications forever.” “Anything” is the extension rule;
“everything governed” is the non-negotiable rule for each admitted extension.

## 4. “Done and proven”: seven pass/fail checks

**Proposed bounded finish:** a useful WASM application can read its approved input
and publish one report through a narrowly governed filesystem mutation; a real
person can authorize and revoke it; a second Mother can execute it independently;
the Pi 3 can sustain that exact path. This proves a governed extensible substrate,
not every possible application, Toy, durable-agent feature or hostile-host defense.

**Status legend:** **proven** = checked-in executable/physical evidence meets the
stated baseline check; **partly** = useful constituent proofs, but the whole check
fails today; **not** = the required mechanism has no demonstrated implementation.
Existing proven checks must be rerun on the final integrated binary. Proposed
limits below are acceptance targets, not measured results.

| # | Pass iff | Today and evidence |
| --- | --- | --- |
| **C1 — Real Child, real host** | An exact-digest approved/assigned WASM Child reads a real bounded Cargo.lock via its admitted Toy and returns the expected digest/report through Mother's normal call path. The absent/expired grant control has no runtime execution. SSH may deploy the package, not execute the application in place of Mother. | **Proven.** [NIC-63 mesh job](../nic63-mesh-job.md), [result](../evidence/nic63-mesh-job/result.json), [Child implementation](../../crates/mct-repo-report/src/component.rs): B is the physical Pi 3, and the expired-grant control has zero runtime facts. This check does not include report publication; C3/C5 cover that. |
| **C2 — Person + scoped expiring grants** | Trusted passkey enrollment identifies a local person principal. Its scoped ≤24-hour day key can issue an exact Child/artifact/resource/action grant with mandatory expiry. An application call proves its person/recipient binding; receiver-local policy checks it, not an asserted `user_id`. Wrong person, resource, target, snapshot, nonce, not-yet-active and exact-expiry calls deny. No UID/offline mutation or local-call path bypasses these checks. | **Partly.** D-58, [passkey tests](../../crates/mct-observation/src/passkey_admin_tests.rs) and admin-only Iroh exist. Application subject/proof, mandatory Toy expiry and full runtime cutover do not. |
| **C3 — Verifiable receipts** | A separate verifier, with pinned Mother identity/history, verifies a domain-separated signed receipt binding person proof, Child/artifact, grant/authority epoch, call/effect ID, input/output digests, outcome and canonical ledger position/hash. Mutating success is returned only after durable outcome; changed bytes/identity/outcome fail verification. Denial and interrupted/unknown outcome are never signed as completed effects. | **Not.** Typed observations, payload digests and sanitized harness receipts exist, but not this signed application/effect receipt path (the input's NIC-110). Admin signatures or a field named “attestation” are insufficient. |
| **C4 — Revocation actually wins** | Revoke the application grant and an invocation/recipient capability while calls are paused before admission: subsequent effects and fresh instances deny, including after restart and stale-config replay. A revocation committed first wins; an effect admitted first has an explicit bounded completion rule. Day-key revocation stops new issuance, **not** independently live ToyGrants. Existing read-only preopens may survive only to their invocation deadline; no raw writable preopen bypasses mutation checks. | **Partly.** [authority-order tests](../../crates/mct-daemon/src/authority_order.rs), current Toy revocation tests in [toy.rs](../../crates/mct-daemon/src/toy.rs), the explicit preopen limit in [wasm.rs](../../crates/mct-daemon/src/wasm.rs), passkey revocation tests and physical receiver revocation are evidence. Person/invocation grants and the governed writer still need this integrated proof. |
| **C5 — One writer, crash-safe truth** | For one output resource, a second mutation owner is refused. Required pre-effect ledger/sync failure prevents effect start; post-effect uncertainty fences success replies and further mutations. Kill at each intent, staging, replacement, outcome and reply boundary; recover to a verifiable completed result, provable non-effect, or explicit unresolved/deny—not blind retry or invented success. Duplicate identical effect IDs cannot apply twice. Rebuild/delete/roll back projections/config: canonical Child, peer and Toy revocations still win. Staging/trash are unreachable through any read preopen, including traversal/symlink attempts. | **Partly.** Ledger locking/poisoning, authority ordering and [SQLite-rollback dispatch tests](../../crates/mct-daemon/tests/mother_restart.rs) exist. The keyed dispatch fence is only 720 seconds, not permanent effect deduplication; complete Child/peer canonical replay and the host-fs journal are missing. See [FINISH.md](../FINISH.md). |
| **C6 — Two independent Mothers** | Two physical hosts with distinct keys, state and ledgers complete one local+delegated job by single-hop Iroh. B locally refuses expired/revoked authority with no execution. Partition/kill B: A's unrelated local work continues; reconnect preserves identity/history and never silently takes over B's work. No common database, global policy counter or runtime coordinator supplies permission. | **Proven for current node-scoped federation.** [Mesh job](../nic63-mesh-job.md) plus [post-review physical suite](../nic63-post-review-suite.md) covers execution, receiver refusal, process SIGKILL, scoped UDP partition, survivors and replacement. It does not prove new person delegation, full-link outage, reboot or power loss; those cannot be inferred from this pass. |
| **C7 — Pi 3 is an operating floor** | The final full Child/Toy/person/receipt binary passes C1–C5 on an aarch64 Pi 3B with approximately 909 MiB RAM: 20 process-cold starts ready within 5 seconds; 100 sequential bounded report/publication calls with p95 ≤10 seconds; peak workload cgroup memory ≤512 MiB; no OOM or swap growth; a 30-minute idle/repeated-call soak and a 10,000-fact restart pass. Signed immutable packages are built elsewhere; measure runtime WASM compilation separately. | **Partly.** Physical useful Child execution is now proven. Admin-only RSS/readiness and tiny-component compile measurements are not full-runtime peaks, a soak or final person/receipt integration. No-compile `.cwasm` loading is **missing**, but is not necessary to this floor if the measured ordinary WASM path passes. |

These seven checks are a **proposed product acceptance contract**, not a silent
replacement for [finish-checks.json](../finish-checks.json) or release approval.
That inventory still has eight open requirements and historical guardian/hybrid
wording superseded for new administration by D-58. First reconcile the inventory
with the owner: map administration/canonical/final-admission into C2–C5, fleet and
failure obligations into C6, and packaging/operator evidence into C7 plus release
review. Preserve useful-workflow/exact-artifact approval obligations explicitly:
a report-output demonstration does **not** prove a native build-and-publish Toy.
If that broader workflow remains a release prerequisite, the bounded seven-check
milestone can be complete while the release gate honestly remains red. Do not
close either by changing a status string.

## 5. Fastest honest path, estimate, critical path and risk

**Plan the seven-check milestone at 28 Pi build sessions on Sol high and 21 calendar
days; working range 22–33 sessions / 16–24 calendar days.** A session means roughly
2–4 focused hours, one coherent implementation/proof slice plus review, not one
prompt. Assumptions: one primary builder, warm build infrastructure, roughly two
sessions per working day, available disposable Pi 3/second host, and an available
person for enrollment and acceptance. Hardware scheduling and failed runs count;
this is not a claim that unit tests substitute for them. The estimate is for the
seven checks above, **not** universal OS completeness or unclosed release scope.

| Order | Deliverable and smallest scope | Piece passes iff | Sessions |
| --- | --- | --- | ---: |
| **0** | Freeze the acceptance/data contracts and inventory mapping. Use D-58, not a new guardian programme. Distinguish person authorizing issuance from person on whose behalf work runs. Specify the receipt, invocation and mutation identities before adapters. | Owner accepts C1–C7, expiry/revocation rules and explicit remaining release obligations; every proof has a reproducible command and expected negative control. | **1–2** |
| **1** | Finish canonical Child approval/assignment and peer-binding facts, replay and current-snapshot consumers. Preserve old history formats; legacy input cannot become new authority by reopening config. | Approve/revoke, restart with missing/rolled-back config and SQLite, and show exact canonical permissions/denials; uncertain writes fence effects. Existing Toy/Watch/hash contracts still replay. | **3–4** |
| **2** | Join the D-58 actor to the real runtime. Migrate the bounded Child approve/assign, Toy issue/revoke, peer administration and application submission routes with adapter-selected scopes. Remove UID/UDS/offline authority bypasses in the same cutover; diagnostic/lifecycle plumbing must not become a grant source. | One passkey-issued day key admits a real Child call under the new profile; expiry, stale snapshot, replay and unmigrated routes deny. Startup no longer needs the current passkey-profile refusal **because the authenticated replacement exists**, not because its guard was deleted. | **4–6** |
| **3** | Carry one recipient-bound, person-proved invocation across one hop. Add versioned proof-carrying call data, not a relaxed `user_id` check. Each receiver explicitly enrolls the accepted person/key relationship and independently issues its own resource grant. No global identity service. Require new ToyGrants to expire; keep their lifetime independent of issuing day-key expiry/revocation and prohibit grant-ID reuse, as already decided. | Same Child/resource, wrong person or wrong receiver fails; copying a capability without recipient proof fails. A legitimately proved call succeeds on B. Old v0's unbound-user denial remains; omitted proof cannot reach person-required grants. | **3–4** |
| **4** | Implement **one** host-fs mutation: bounded report replacement in an admitted output root, conditional on expected previous digest/absence. No arbitrary shell, generic filesystem CRUD or guest writable root. Use one resource owner, private out-of-preopen staging, durable effect identity/intent/outcome, directory/file sync and content-aware recovery. This is the smallest proving slice of the NIC-118 lesson, not a claim to finish its whole design pack. | C5's kill/fault matrix passes, two writers cannot publish concurrently, changed-content retries conflict, and the original result is reacquirable after a lost reply. Read-only input grants cannot reach staging/retained old output. Revoke/expiry before replacement denies. | **4–6** |
| **5** | Implement signed effect/result receipts and a standalone verifier. Sync the canonical outcome first, sign its exact ledger position/hash, then durably record the receipt before acknowledging publication; no self-referential receipt hash. Retain enough canonical bytes to reproduce verification after restart. | C3 passes positive/tamper/wrong-identity tests; a completed mutation with lost acknowledgement returns its original verifiable receipt. Unknown outcome stays unknown, and absence of a valid receipt is never a new authorization. | **2–3** |
| **6** | Extend the existing repo-report acceptance through the new ingress and output Toy. Reuse NIC-63 controllers rather than invent a scheduler. Add person/revocation/race/ledger-fault cases to the same path, and repeat the two-host survivor test on the integrated binary. | C1–C6 pass together, not as disjoint legacy-runtime/admin-only demos. Each denial proves zero forbidden effects, not merely an HTTP/protocol error. Ledger and result/receipt attribution join across both Mothers. | **2–3** |
| **7** | Qualify the final Pi 3 package. Build/sign off-device; install, measure actual cgroup peaks and cold compilation, run soak/history/restart/expiry/revoke checks with the person-backed path. | C7 passes with retained measurements, package hashes and canonical evidence; no builder process or hidden swap dependence on the target. A resource miss is a failed check, not permission to cite the 18 MiB admin number. | **2–3** |
| **8** | Independent review and reproducible close-out. Run fmt, workspace tests, Clippy, Tier-0/specs, docs and bounded fault/fuzz proofs for the changed boundaries. Retain failing seeds/runs and rerun final artifacts after corrections. | All seven checks have current-binary evidence and reviewer acceptance. Report the existing release gate separately, including any intentionally still-open workflow/packaging obligations. A merged branch or green development CI is not release approval. | **1–2** |

**Critical path:** canonical authority → authenticated runtime cutover → receiver-
verified person invocation → journaled mutation → receipt/recovery integration →
final physical Pi qualification. Receipt schema and adversarial test design can
start early, but independent implementation on the shared writer/replay/startup
boundary is not a safe way to compress this chain. C1/C6's prior hardware results
save rediscovery, not the final regression run.

**Biggest risk:** the authority cutover is broader than “add a passkey check.”
Child/peer config, the canonical ledger/projection, ingress provenance and effect
ordering currently meet at different boundaries. A shortcut can either resurrect
revoked authority or leave an old bypass alive. Keep each piece fail-closed with
replay/rollback counterexamples before enabling the next. Filesystem crash
reconciliation is the next largest technical risk: rename atomicity is not an
atomic transaction with the ledger, and process SIGKILL is not proof of power-loss
durability. Do not promise exactly-once external effects.

**Deliberately not on this critical path:** wasmCloud migration, Component Manager,
Cross-Vision federation, general service-call composition, a fleet scheduler,
whole-Pi/PiD WASM conversion, guardian recovery resurrected from archived plans,
and AOT `.cwasm`. The latter would require authenticated native-code provenance,
engine/target compatibility and safe loading/rejection proofs; [compile_snapshot](../../crates/mct-daemon/src/wasm.rs)
today compiles verified WASM bytes with `Component::from_binary`. Avoid replacing a
known sandbox input with trusted native code merely to claim “no compile.”

**What this review actually verified:** read-only source/document inspection at the
baseline above; `node --test scripts/federation/mesh-job.test.mjs
scripts/first-real-app/client.test.mjs` passed **22/22**, no skips. Those tests check
controller/client validation, correlations, expiry evidence and durable client
recovery, not physical deployment or cryptographic effect receipts. Full Rust,
hardware, browser ceremonies and fault campaigns were not rerun for this doc-only
review. Historical passes are attributed to their committed evidence, not to this
session.

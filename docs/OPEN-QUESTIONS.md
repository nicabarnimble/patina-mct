# Open questions

Questions the owner has not settled, with the current proposal where there is
one. A proposal here is not a decision. When one is settled, move it to
[DECISIONS.md](DECISIONS.md) and delete it here.

Baseline: `main` at `4330876`; review-branch dispositions are explicitly labelled.

## 1. Child-start ledger fact (double-run on power loss)

**The bug.** A local Child call writes no ledger fact between route
revalidation and the Child returning. Restart recovery finds in-flight work in
SQLite, which runs WAL with `synchronous = NORMAL`. That survives a process kill
but can lose recent commits on power loss or an OS crash. After that, a
same-key retry finds no reservation and runs the Child a second time. This
breaks "no retry executes the child twice". SIGKILL tests do not catch it.

**Review-branch progress.** The [ledger-first-call-recovery slice](../layer/surface/build/feat/ledger-first-call-recovery/SPEC.md)
adds an independent caller-scoped dispatch fence for keyed retries. Its
SQLite-rollback regression reproduces the original bug and passes with the
fence. It does not implement final Child-start ordering, unkeyed dedup, or
reconstruction of lost run/result projections. The seven Option D questions
below remain open; do not mark D-40 built.

**Proposal.** Option D in
[design/child-start-ledger-fact.md](design/child-start-ledger-fact.md): the
ledger writer admits the launch and appends `ChildInvoked` before it; a
writer-level guard refuses a second admission of the same `call_id`; recovery
reads the ledger first.

**The seven questions, with proposed defaults (pending the owner):**

| # | Question | Proposed default |
| --- | --- | --- |
| 1 | Land now, or after perf-phase-0 closes? | Fix now. The note itself recommended waiting for the Phase 0 close-out; the later proposal is to fix now because this is a correctness bug. The Phase 0 spec then needs an amendment. |
| 2 | Does the once-only guard cover only keyed calls? | Cover every call, keyed and unkeyed, including Watch redelivery. |
| 3 | How does recovery tell a run started under the new law? | Record the epoch on each run. Runs from before the law have no admission fact and stay interrupted (fail closed). |
| 4 | Move SQLite to `synchronous = FULL`? | No. Keep `NORMAL`; the ledger is the truth. |
| 5 | Fold Toy effect admission into the same writer path? | Yes, in a follow-up PR, not this one. |
| 6 | Expose the admission fact to peers in `mct/call/0`? | Not yet. |
| 7 | SQLite has a result the ledger lacks. What then? | Mother refuses to start. |

## 2. Admin path: the rest of Phase 3, then Phase 4

Decided, not built. Open items inside it:

- Admin ALPN on iroh and pairing-ticket redeem over iroh.
- Hard cutover in one build: delete the UDS control socket, `peer_cred` and
  `authenticate_owner`, `open_local`, the `os-uid` principals and every
  `execute_offline_*` command; move `POST /calls` to the iroh call path; move the
  operator gate's principal to the recovery public key.
- Re-init must not mint a second admin key over an existing ledger (bind it to
  the inventory hash, as the operator gate already does).
- Key files `0600`, parents `0700`, existing files re-checked.

Code map (stale line numbers): [design/admin-keys-code-map.md](design/admin-keys-code-map.md).

## 3. Make scheme v1 signatures required

Today replay accepts unsigned admin facts and self-signed grant puts (#67).
Open: when signatures become required, what happens to grants made before the
scheme, and whether a self-signed put must also be authorized by an existing
`key-manage` grant.

## 4. The ledger is not yet the only truth

- Child approvals and assignments, and peer bindings, are saved in daemon
  config and skipped by authority replay. Their revocations do not replay from
  the ledger. Build plan phase 5 moves the Child facts the check-in needs into
  the ledger; peer bindings have no plan yet.
- Restart recovery lists in-flight work from SQLite (item 1).
- Until both close, "the database is a rebuildable cache" is a goal, not a
  property.

## 5. Measurements still missing

- **Mac fsync cost.** One extra synced frame is 76 µs p50 on a Linux ext4 VM.
  On macOS, `F_FULLFSYNC` is likely milliseconds. Run
  `cargo bench -p mct-daemon --bench perf_phase_0` on the Mac and read
  `append_including_sync_data`.
- **libcrux NEON on Apple silicon.** The `simd128` path is not measured.
  Run `cargo run --release -p phase3-measure` on the Mac.
- **perf-phase-0 official run and close-out** are still outstanding.

## 6. Pi Durable as a Child: four open conflicts

From [design/pi-durable-child.md](design/pi-durable-child.md#contradictions-to-discuss):

1. Ambient credentials: Pi's harness builds a provider from `OPENAI_API_KEY`
   and `bash` inherits the process environment. MCT requires that Mother holds
   the credential and a gateway forwards.
2. A JS Child through jco or StarlingMonkey versus WASI standards.
3. Subagents start as copies of the parent's tools, which is a Child minting
   power. MCT requires Mother to mint every borrowed handle.
4. Background work that survives `abort` versus Mother's stop.

Also open: what a Child does after the fingerprint rule refuses a resume, and
the four speed levers in [design/pi-durable-handoff.md](design/pi-durable-handoff.md)
(batching wakes, caching session state, pre-authorizing low-risk actions, a
local scratchpad). Each one conflicts with (a)–(f) under at least one reading.

## 7. Does WIT composition fit for handing Toys to Children?

Not adopted, not designed. See
[design/admin-keys.md](design/admin-keys.md#does-wit-composition-fit-for-handing-toys-to-children).

## 8. Status of the decision-model Toy

The admin-keys note calls it proposed. The build plan lists it as decision 5
and phase 8. Confirm which.

## 9. Mother never dying

Decision (c) puts the effort into hot standby, replicated state and fast
restart. None of that is designed. The build plan explicitly leaves hot standby
out of scope. A standby that lets a Child keep working from another Mother's
ledger would be failover of a Child's work, which (c) forbids.

## 10. Multi-Mother

Future scale work, not planned now (D-38). Nothing to decide yet. Known
constraints for when it comes: per-Mother ledgers versus (c), and a FROST
guardian quorum not covering the ML-DSA half.

## 11. Remaining architecture boundaries

The review identified concrete decisions needed before broader implementation:

- [Current security boundaries](SECURITY-BOUNDARIES.md): recovery-key compromise,
  admin issuance ceilings/resource scopes, portable handle semantics, trusted
  supervisor lifecycle, and signature/rollback trust.
- [Durable task boundary proposal](design/durable-task-boundary.md): task versus
  invocation versus effect identity, unknown-outcome recovery, checkpoint and
  crash semantics, deterministic inputs, result retention, and code upgrades.
- Secrets: opaque use-only credentials versus an explicitly authorized reveal
  operation. The current value-returning backend is not a universal no-raw-key
  guarantee.
- Delegation: cascading revocation must state how it interacts with the
  bounded lifetime of already-admitted capabilities (D-G7).

These are proposed clarification/implementation boundaries, not new owner
rulings. Capability checks alone do not provide general information-flow
security.

## 12. Smaller open items

- Linux systemd supervision and macOS headless launchd: separately gated.
- Network and storage Toys: designed in outline (D-39), not built.
- Signature size: a digest cannot replace a verifiable signature. Any detached
  signature design must retain the actual signature bytes with authenticated,
  durable availability; no such design is adopted.
- Live node identity rotation needs endpoint rebind and peer re-admission.
- Kenton Varda argued (June 2026) that hand-configuring each agent's permissions
  means you have already lost. MCT grants per Child and per task by hand today.
  How grants get proposed and approved at scale is unanswered. Only the opening
  of that post was read.
- Ideas from prior art, none decided: [PRIOR-ART.md](PRIOR-ART.md#ideas-worth-borrowing-not-decided).

# MCT build plan

> **Imported 2026-10-18** from working notes (2026-10-14). The ten-phase order is still the plan. Progress on `main` at `49dfacd`: Phase 1 done (#48, #64). Phase 2 done (#51). Phase 3 partly done: the Ed25519 + ML-DSA-65 scheme v1 signature exists and is optional on replay (#67). The admin ALPN and pairing redeem over iroh are not built. Phases 4–10 are not started.
> Superseded inside this note: "Scheme v1 is still open" under (C) was settled on 2026-10-15 (see [admin-keys.md](admin-keys.md#decisions-october-15-2026)). "No new code has been written" was true when the plan was written. Names such as Douglas, Fawn and Wren are kept as written in the original discussion and are not defined here.


This plan implements the decisions already written in [admin-keys.md](admin-keys.md) and mapped in [admin-keys-code-map.md](admin-keys-code-map.md). No new code has been written. The foundation already in patina-mct is the iroh path, the authority ledger, and the Child runtime. wasi-pi bindings are not in this checkout. See [What this plan could not verify](#what-this-plan-could-not-verify).

Line numbers below are the ones read in this checkout of `crates/mct-observation/src/lib.rs`. The code map's other line numbers are from origin/main `784c5ed` and will shift after #45 and #39–#44 merge. This plan does not invent files.

(a)–(f) stay locked. This plan does not rewrite them. The grace window starts at the next checkpoint. The two-clocks flag is closed in the design note.

## Decisions in scope

1. The recovery key is a lockout. Nothing day-to-day revokes it.
2. Hard cutover. The old OS-user path is deleted in the same build that adds the iroh key path.
3. One pause, ordered check, then weigh, then act, plus the freeze rule when Mother is unreachable. The grace window starts at the next checkpoint.
4. Older-child status is a grant. Only an older Child may share. Mother mints every borrowed handle. In a group, Mother reissues Toys upward as separate handles.
5. The decision model is a catalog of Toys (Jev, Nimble, Local) behind a thin gateway. Mother holds each credential. The gateway only forwards. Mother's grants enforce who may call.
6. The replay Toy sits beside `wasi:random` as its own Toy, never a fork. It replaces only wasi-pi's seeded-secure-random fork. A replayed run reuses the recorded model answer.
7. Mother never reaches into a Child's state. "Do this next" is the interrupt carrying the next task. Every fix is a ledgered grant.
8. Freeze-as-wake. The ledger records the freeze. Resuming reuses the wake check-in. A resume counts as a wake under (e).
9. The grace window is a policy Mother sets and records in the ledger, not a fixed number. It starts at the next checkpoint.

## (A) Phases

No new features past that list. Multi-Mother, per-Mother ledgers, FROST guardians, and the delayed reset stay future work. Mother hot standby is not in the nine decisions. The code map already says it needs a separate design. This plan does not build it.

The code map's first order step is to land #45 and then #39–#44 before new edits, because those changes touch the daemon files the later phases edit. That is a prerequisite, not a feature.

| Phase | What | Depends on |
| --- | --- | --- |
| 1 | Ledger facts for an admin-key grant, a pairing-ticket burn, and a recovery-key principal. Replay denies a second ticket consume and does not accept a revoke of the recovery grant. | The existing authority ledger. Later phases all append or read these facts. |
| 2 | Generalize the toy-grant check so the subject can be an admin key, with route scopes and mandatory expiry on day-to-day keys. The recovery grant stays gate plus key-manage, and a day-to-day key cannot revoke it. | Phase 1. |
| 3 | Admin path on the existing iroh connection: an admin ALPN, pairing redeem, and the Ed25519+ML-DSA-65 fingerprint. No new transport. | Phases 1 and 2. The ML-DSA half is not in the code the map located. |
| 4 | Hard cutover, one build. Serve the admin routes over iroh and, in that same build, delete the Unix socket, the `peer_cred` check, `open_local`, the `os-uid` principals, and the `execute_offline_*` commands. The CLI becomes an iroh client. | Phase 3. |
| 5 | Wake check-in, and freeze-as-wake. The ledger records the freeze and the successful check-in. A resume counts as a wake under (e). Child facts the check-in must replay move into the ledger. Douglas-approved acceptance criteria are in [Phase 5](#phase-5). | Phase 1, and the Child runtime that already invokes a Child. |
| 6 | The single pause in the task loop: check, then weigh, then act. No live Mother means no pass. The grace window is the ledgered policy and starts at the next checkpoint. | Phase 5. The checkpoint the map names is the lease loop, not an epoch trap. |
| 7 | Older-child status as its own grant. Only an older Child may share. Mother mints every borrowed handle. A young Child's hand-up is a request. Mother reissues that Toy as the older Child's own handle, one handle per Toy. | Phases 1, 2, and 4. Mother is the only grantor. |
| 8 | Decision-model catalog: Jev, Nimble, and Local, one Toy each, same interface. Mother holds each credential and grants a fresh per-task handle. The gateway forwards and honors that handle. It does not decide who may call. Ship one model first. | Phase 7, because older-child and per-task are grant checks. |
| 9 | Replay Toy beside `wasi:random`. Seed and counter on that Toy. Secure random stays real entropy. A replayed run reuses the recorded model answer instead of calling the model again. | Phase 8 for the recorded answer. The wasi-pi tree is not in this checkout, so the fork being replaced cannot be named as a file here. |
| 10 | "Do this next" is the interrupt carrying the next task, at the checkpoint. A fix is a ledgered grant: a fix Toy, a corrected input, or a restart with new parameters. Mother does not rewrite Child state. | Phase 6 for the checkpoint, and phase 1 for the grant record. |

### Phase 5

Douglas approved these wake-path acceptance criteria. 1–5 are Fawn's. 6–8 are Wren's. (a)–(f) stay locked. The nine decisions in [Decisions in scope](#decisions-in-scope) stay as written.

1. Phase 5 owns the full wake path.
2. A Child checks in with Mother before any access to Child state.
3. Replay can never restore old authority.
4. Define who records a freeze when Mother is down.
5. Every inference path is gateway-only, so the Child never sees a raw key.
6. Admit wakes before `cycle.rs` drains events.
7. Measure grace with a host-side clock.
8. Add a test that a wake W delivered twice after commit R produces exactly one continuation.

Conflict check. None of the eight weaken (a)–(f) or the nine decisions.

- 1 gives Phase 5 the wake path. (e) and decision 8 stay the wake check-in and freeze-as-wake. Decision 6 stays the replay Toy beside `wasi:random`.
- 2 is the (e) check-in before the Child touches its own state.
- 3 keeps replay from putting revoked authority back. Decision 1 stays the recovery-key lockout: nothing day-to-day revokes that grant, and (a) keeps that grant as the exception to revocability.
- 4 asks the phase to name the recorder of a freeze while Mother is down. Decision 8 stays "the ledger records the freeze." (c) stays a hard stop for a Child's work, not failover. The criterion names no recorder.
- 5 is decision 5 on inference: Mother holds the credential, the gateway only forwards, and the Child does not see it. (a) stays Mother as the only grantor. (d) stays the platform-given handle.
- 6 orders wake admission before `run_child_task_cycle` drains. Decision 3 and (f) stay one pause, check then weigh then act, at the next checkpoint. The phase 6 row still places that checkpoint on the lease loop.
- 7 names the clock that measures grace. Decision 9 stays a ledgered policy Mother sets, not a fixed number, starting at the next checkpoint. The two-clocks flag stays closed: the window starts at that checkpoint.
- 8 is one continuation when wake W is delivered twice after commit R. Decision 8 and (e) stay: a resume counts as a wake and reuses the check-in.

## (B) First task

Phase 1. It is the earliest new work. Phases 2 through 10 read or append the facts it adds.

**File.** `crates/mct-observation/src/lib.rs`. Confirmed in this checkout.

Symbols read there:

- `AuthorityChangeV1` (line 121). Variants today: `ToyCatalogPut`, `ToyCatalogRemove`, `ToyGrantPut`, `ToyGrantRemove`, `WatchScopePut`. No admin-key grant and no pairing ticket.
- `GrantShapingSourceV1` (line 101). Sources today: `OperatorDecision`, `ChildApproval`, `ChildAssignment`.
- `execute_authority_mutation` (line 919).
- `replay_authority_entries` (line 1934).
- `OperatorGatedNonvirgin` (used at line 828). Wiping the ledger is a new Mother, not a way back in. This task must not add a bypass around that class.

The code map also points at this file for `AuthorityChangeV1`, `execute_authority_mutation`, `replay_authority_entries`, and `OperatorGatedNonvirgin`. Those names match this checkout. Its other citations were not re-read for this task.

**In.**

- An admin-key grant fact: public half only, scope, expiry for a day-to-day key, and a recovery grant distinguished from a day-to-day grant. The recovery grant is the startup gate plus key-manage.
- A pairing-ticket fact: hash only, single-use. Replay rejects a second consume of the same hash.
- Mutation admission and replay do not apply a revoke of the recovery grant. A day-to-day grant cannot revoke it.
- A grant-shaping source for an admin key, beside the sources already on `GrantShapingSourceV1`. The code map calls this shape `GrantShapingSourceV1`.

**Out.**

- No iroh ALPN, no fingerprint bytes, no CLI, no socket deletion. Those are phases 3 and 4, and phase 4 deletes the old path in the same build as the new iroh admin path, not in this task.
- No pause, no grace window, no decision-model Toy, no replay Toy, no older-child grant. Later phases.
- No ML-DSA implementation. The code map did not locate that code.
- No fixed grace-window number, and no number for key expiry beyond "day-to-day keys have mandatory expiry."
- No change to `OperatorGatedNonvirgin` that would let a lost recovery key become a virgin epoch.

**Tests.** Extend the replay tests already in `crates/mct-observation/src/lib.rs`. They already call `replay_authority_entries`. Add cases for: an admin grant replays; a second consume of the same ticket hash is rejected; a mutation that revokes the recovery grant does not apply; existing toy-grant replay still succeeds. Do not add a new test crate.

## (C) Risks and open questions

- The iroh endpoint is the only admin way in after phase 4. If it is down, there is no local escape hatch. That is decision (b). The recovery key is the way back only while its secret still exists. Losing it is a lockout.
- Re-init must not mint a second admin key over an existing ledger. The design note treats a reprinted token on an existing ledger as a new grant that has to be bound to the inventory hash. This task does not build that bind. Phase 4 must not skip it.
- Line numbers outside `crates/mct-observation/src/lib.rs` are the code map's, from `784c5ed`, and shift after #45 and #39–#44.
- An epoch trap is a kill, not a pause. Phase 6 must not use it as the checkpoint. The code map says a graceful interrupt needs a host-call checkpoint. The grace window starts at that checkpoint, after the current step, and does not cut the step.
- Scheme v1 is still open in the design note: whether the signature covers the grant fact, the request, or both, and where the ML-DSA-65 public key sits. Phase 3 cannot close that by guessing.
- WIT composition for handing Toys to Children is still an open question in the design note. No phase wires it.
- Whether `patina:pi@0.3.0` is present anywhere outside this checkout was not checked. See below.

### Conflicts with (a)–(f)

The two-clocks flag is closed. The window starts at the next checkpoint. (c) and (f) are not rewritten.

What still conflicts in the design note, and is not a task in this plan:

- If the Child's act on a model judgment performs the share, mints a handle, or lends for a young Child, that is the closed Child-as-grantor conflict. The model advises and never decides. Mother mints every borrowed handle. Phase 8 must not let the model's answer mint.
- Future path 2, a FROST guardian quorum, does not cover the ML-DSA half of the Ed25519+ML-DSA-65 fingerprint. This plan defers that path. It does not drop the ML-DSA half.

## What this plan could not verify

patina-wasi-pi is not in this checkout. Searched the working directory for `wasi-pi`, `wasi_pi`, `patina:pi`, and a wasi filename. No matches. The design note already says `main.rs` near `random::wasi_sources` and `patina:pi@0.3.0` are not in patina-mct at `784c5ed` and were not in the public patina tree searched when that note was written. This plan could not verify those files, that package, or the seeded-secure-random fork as source. Phase 9 cannot name the file to delete until that tree is visible.

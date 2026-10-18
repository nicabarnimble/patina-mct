# Item 8: ledger evidence that a Child execution started

> **Imported 2026-10-18.** Status: **proposed**, waiting on the owner's answers to the seven questions in §5. Proposed defaults for those questions are in [../OPEN-QUESTIONS.md](../OPEN-QUESTIONS.md). "Item 8" is this note's number in the post-restart follow-up list (#66 "Known follow-ups": no ledger execution-start fact). The S1 double-run on power loss is a known open correctness bug on `main`.


Status: design note only. No code, commits or PRs.
Base: `main` at `49dfacd`. All `file:line` references are to that revision.
Date: 2026-10-18.

## 0. Short version

- Today the ledger durably records a local call up to route revalidation, and nothing after that until the Child returns. The ledger's only "started" fact, `runtime_execution_started`, is appended **after** the runtime returns, in the same batch as the completion. So a crash while a Child runs leaves no ledger evidence that it started.
- Restart recovery decides from SQLite (`in_flight` reservation, `running` run record). SQLite runs in WAL mode with `synchronous = NORMAL` (`crates/mct-daemon/src/state.rs:404-405`). That setting survives a process kill but can lose the most recent commits on power loss or an OS crash. The ledger fsyncs every batch. After a power loss you can have a ledger that shows the call routed and a database that never heard of it. A same-key retry then runs the Child a second time. That is a real hole, not just a forensics gap.
- The deadlock risk recorded earlier is real, but only for one placement: appending while holding the authority-order mutex. The fix is to stop doing admission on the caller's thread and have **the ledger writer do the admission itself**. The writer proves authority, takes the order lock, appends the start fact, fsyncs and replies, all on its own thread. No caller holds the lock while it waits on the writer, so the wait cycle cannot form.
- Cost: one more synced ledger batch per local Child call. Today a call has four, so this makes five. Measured on a Linux ext4 VM, one extra frame plus `sync_data` is 76 µs p50 / 138 µs p95. On the Mac it is not measured; `F_FULLFSYNC` is likely milliseconds. The v0.2.0 baseline call p50 is 485 ms. The "ledger off the hot path" objection does not match the code: the ledger is already on the hot path four times per call.
- Recommendation: option D below (writer-arbitrated admission fact, using the existing but never-emitted `ObservationKind::ChildInvoked`). It should be followed by a writer-level guard that refuses a second admission of the same `call_id`, and by a recovery pass that uses the ledger alone. Group commit (option B) is a later throughput step that the doctrine forbids until the net exists. Option C (inferring the start from the route fact) costs nothing but cannot prove ordering, and still depends on SQLite.

## 1. The problem

### 1.1 What is written today, in order, for a successful local UDS call

| # | What | Where | Store | Durable? |
|---|---|---|---|---|
| 1 | `CallReceived` + `CallConstructed` | `local_ingress.rs:412` | ledger | one synced batch |
| 2 | idempotency `reserve_call_idempotency` → `in_flight` | `idempotency.rs:345` | SQLite | WAL, `synchronous=NORMAL` |
| 3 | route batch: candidates, `RouteSelected`, `RouteRevalidated`, `CallAuthorized` | `pipeline.rs:928` | ledger | one synced batch |
| 4 | effect snapshot from `projected_authority` | `pipeline.rs:938-972` | writer round trip | n/a |
| 5 | `admit_effect_with_snapshot` (token vs snapshot), can deny | `execution.rs:119` | memory | n/a |
| 6 | `insert_run_started` → `running` | `execution.rs:140` | SQLite | NORMAL |
| 7 | `ledger.admit_effect` (projection proof, then order mutex, empty start closure) | `execution.rs:152`, `observation.rs:610-640`, `authority_order.rs:290-327` | memory | **no fact written** |
| 8 | adapter build; Toy/Watch grant evaluations appended if any | `execution.rs:631-637` | ledger | synced batch (only for Toy/Watch Children) |
| 9 | WIT invocation (guest runs) | `execution.rs:639` | n/a | n/a |
| 10 | `runtime_execution_started` (`obs-resident-wasm-wit-started:<call>`), `runtime_execution_completed`, `obs-executed-on`, payload facts | built in `wasm.rs:1166-1174` and `execution.rs:176-200`, appended at `pipeline.rs:1063` | ledger | one synced batch, **after** the guest returned |
| 11 | `complete_run`, `append_run_observations` | `pipeline.rs:1068-1074` | SQLite | NORMAL |
| 12 | terminal `ResultRecorded`, then UDS reply | `local_ingress.rs:426` | ledger | one synced batch |
| 13 | `complete_call_idempotency` | `idempotency.rs:428` | SQLite | NORMAL |

Two details make the start timestamp worse than it looks. `started_at` and `completed_at` are both sampled before invocation (`execution.rs:658-659`; perf-phase-0 D-P0.6 records this as an attribution gap). And the runtime re-checks the token inside `wasm.rs:1163` before it builds the start observation, so the observation is not even the admission point.

The forwarding path differs. `PeerCallSent` is appended and acknowledged **before** the peer is contacted (`forwarding.rs:205-218`). Watch batches do the same: they append their source-start fact before exposing the preopen (`watch-event-fixtures/SPEC.md:434`). The local Child path is the one effect path without a pre-effect start fact.

### 1.2 What the ledger cannot prove today

1. **That a local Child was launched.** For a call that crashed mid-Child, the ledger holds facts 1 and 3 (plus 8 for Toy/Watch Children) and nothing else. Those same facts are also exactly what a call shows if it crashed before step 6 or 7, or was denied in memory at step 5 or 7 just before the crash. "Routed" and "running" look the same.
2. **That the launch was admitted under current authority, in ledger order.** `EffectAdmissionIsOrderedWithAuthorityMutation` and `MutationCommitAndEffectStartHaveOneOrder` (`mct-product-map.allium:187-194`) are enforced by an in-memory mutex. They leave nothing in the chain. A revocation fact at sequence N+1, after the route batch at N, is consistent with both "admitted before the revocation (legal)" and "admitted after it (a bug)". An auditor cannot tell which happened.
3. **Which calls were in flight at a crash.** Recovery builds that set from SQLite (`restart_recovery.rs:292-330`). It only reads the ledger to refine the disposition (denial, effect-start, transient release) of rows that SQLite already lists. A call that SQLite lost is not settled at all.
4. **That the DB and the ledger agree.** Nothing cross-checks them at startup. If they diverge, nobody notices.

### 1.3 Failure scenarios

**S1: power loss or OS crash during or just after a Child effect (DB rollback). This is reachable today and breaks the spec rule.**
Sequence: steps 2 and 6 commit to the WAL without fsync (NORMAL). Step 3 is fsynced. The guest runs and performs a Toy effect (git, secrets) or a Watch callout. Then the machine loses power before the WAL is synced.
After reboot, SQLite has no reservation and no run. The ledger shows `CallReceived`/`RouteSelected`/`RouteRevalidated` (and Toy grant evaluations) for the call, and nothing after them. `settle_prior_tenure_reservations_and_runs` returns early because both SQLite lists are empty (`restart_recovery.rs:301-303`). The client retries with the same key. `reserve_call_idempotency` returns `ExecuteFresh`, and the Child runs again. That violates `resident-call-ingress/SPEC.md:39` ("no retry executes the child twice") and the 0.x decision at `mct-product-map.allium:2024-2029`.
SIGKILL alone does not trigger this: WAL commits survive a process kill. That is why `tests/mother_restart.rs` (which only SIGKILLs) is green.

**S2: the DB is restored, copied or replaced.** Examples: an operator restores `state.sqlite` from backup, a migration rebuilds it, or a disk tool truncates the WAL. The effect is the same as S1, across arbitrarily many calls.

**S3: peer or offline auditing.** Today no cross-Mother ledger export or audit exists. `Cross-Mother replay` is a federation follow-on (`mct-product-map.allium:2021-2023`). Even so, an auditor holding a verified copy of this Mother's chain (`verify_ledger_streaming`) cannot list the Children it launched, or check that each launch was admitted under the authority current at its position. A forwarding peer has its own `PeerCallSent` fact, but the receiver's ledger has no matching "admitted for launch" fact to pair it with.

**S4: crash forensics.** After any crash, the operator cannot tell from the ledger whether call X died before admission, during guest execution, or between return and fact 10. The SQLite `running` row cannot say either: it is inserted before admission (`execution.rs:140` precedes `:152`), so it over-reports.

**S5: precision loss in recovery (the conservative direction).** A crash between step 6 and step 7 leaves a `running` row for a Child that provably never launched. Recovery marks it `execution_interrupted`, outcome unknown, and pins the key for the TTL. That is safe but wrong. With a durable admission fact, recovery could release such calls.

## 2. Constraints

Spec and doctrine:

- **C1. No retry runs a Child twice.** `resident-call-ingress/SPEC.md:39`. Idempotency decisions at `mct-product-map.allium:2009-2016` ("within its TTL, a keyed retry replays or is typed-refused and never silently re-executes"; in-flight entries refuse until expiry). Restart decision at `:2024-2029`: an interrupted call is closed once, `execution_interrupted`, "never re-executed".
- **C2. Mother never reaches into Child state.** Restart recovery does not resume, inspect or replay a guest (`restart_recovery.rs:1-9`). The only legal post-crash action is to close the call.
- **C3. Before-effect durability.** `MctLedgerCommitAndRecovery.BeforeEffectRequiresAcknowledgedCommit` (`allium:2976`): the effect does not begin unless every required BeforeEffect fact was acknowledged. `MctLocalFirstObservationLedger.AuthorityFactsAreDurableBeforeEffect` (`allium:2932`): "allow/deny/**admission**/grant… observations are durable before the protected effect proceeds". Read literally, the final order admission at step 7 is an admission that writes no fact. Today's code arguably under-satisfies this clause, and the start fact would close that gap.
- **C4. One order for mutation and effect start.** `TwoPhaseRouting.EffectAdmissionIsOrderedWithAuthorityMutation`, `.MutationCommitAndEffectStartHaveOneOrder` (`allium:187-194`). An uncertain mutation admits no effect. An effect that already started is not undone by a later revocation (pinned by `authority_order.rs:623` `revocation_first_denies_while_effect_start_first_runs_exactly_once`).
- **C5. One writer and poisoning.** `OneWriterDefinesLocalOrder` (`allium:2980`), `UncertainAppendPoisonsWriter` (`allium:2971`). Writer law in `ledger-commit-recovery/SPEC.md:99-110`.
- **C6. Calls are not admin mutations.** `MctLocalApplicationIngress.CallsRemainOutsideMutationSequencer` (`allium:1029`), W1 B10 decision (`allium:1005-1008`). Calls must not enter the protected mutation critical section or block control reads.
- **C7. Doctrine.** `layer/core/reliability-doctrine.md:54-58` ("Optimize Under the Net": no batching or group commit until the path is covered and profiled). `:60-64`: durability classes are spec decisions, never ad hoc at a call site. `:66-75`: physical write-path changes carry crash-state generation at every new boundary, fuzzing of changed parsers, preserved seeds, and coverage ratchet.
- **C8. perf-phase-0** (`layer/surface/build/feat/perf-phase-0/SPEC.md`). Read precisely, it does **not** say "keep the ledger off the hot path". It says:
  - D-P0.2 (`:93-100`): *during that phase*, no production source changes. That phase is still `status: active`, with its official run and close-out not yet done (`:258-265`).
  - D-P0.6 (`:151-170`): attribution uses `obs-resident-wasm-wit-started` as the "WASM start" boundary, and counts per-call frames by durability class. Adding a fact changes the measured path and the frame accounting.
  - D-P0.8 (`:187-197`): physical-write candidates (group commit, durability classification) are later gated slices that must cite `BeforeEffectRequiresAcknowledgedCommit`, `AuthorityFactsAreDurableBeforeEffect`, `BufferedEffectsAreBounded`, `LocalAcknowledgementRequiresDurableFacts`, and the two doctrine sections.

  So the real conflict is about sequencing. Landing a new BeforeEffect fact while Phase 0 is still measuring `ead8796` changes what Phase 0 measures. It must be its own spec slice with its own durability decision.
- **C9. No raw keys or payload in observations.** `RELEASE-REVIEW-R1.md:140`: observations hash scope and fingerprint and never store keys. A Child never sees a key; nothing here gives the Child anything new.

Tests that pin current behaviour and would change or conflict:

| Test | Pins | Conflict |
|---|---|---|
| `tests/mother_restart.rs:873-881` `invocations_for` | counts `RuntimeExecutionStarted` per call as "executions" | If the start fact reuses that kind, counts double. If it moves the existing fact earlier, an admitted-but-not-launched call counts as an execution. |
| `resident/idempotency.rs:528-533` `durable_execution_count`, used by `in_flight_idempotency_duplicate_refuses_without_second_execution` | `matches("runtime_execution_started")` | same |
| `resident/serving.rs:1473`, `:1558-1563` | exactly one `runtime_execution_started` across replay and restart | same |
| `resident/execution.rs:1389`, `:1463-1468` | post-mint denial writes no start observation | fine if the start fact is written only on successful admission; breaks under option C if anyone reinterprets the route fact as "start" |
| `tests/mother_restart.rs:1359-1371` `inflight_child_call_is_resumed_or_marked_interrupted_after_sigkill` | waits for the SQLite run row as "durably started" | Should wait for the ledger fact instead. The current wait races step 7 and can kill before admission. |
| `authority_order.rs:623` and the source-pin test `:700-753` (asserts `ledger.admit_effect(&effect_snapshot` and `self.authority_order.admit_effect` strings exist) | admission shape | Options A2 and D move or rename the call site. The pin must be updated with the new seam, not deleted. |
| `pipeline.rs` `before_effect_append_failure_suppresses_child_effect` (`ledger-commit-recovery/SPEC.md:267`) | failed BeforeEffect ack ⇒ no Child effect | must keep passing; the new fact is a new instance of it |
| `scripts/perf/attribution.py:211-215` | `wasm_started` = `runtime_execution_started` | gains a new, truthful `child_admitted` boundary; the old one stays |
| restart dispositions in `restart_recovery.rs` unit tests | "reservation, no run, no effect evidence ⇒ Released" | Option C turns some of those into Interrupted. |

`ObservationKind::ChildInvoked` exists (`mct-kernel/src/observation.rs:39`, `allium:2486` `child_invoked`). It is **never emitted** in production, and `restart_recovery.rs:108-116` already counts it as an effect start. Every option below uses it rather than `runtime_execution_started`. That leaves all `runtime_execution_started` counting tests and the D-P0.6 boundary semantically unchanged.

## 3. Options

Common to every option that writes a fact: the start fact is a `ChildInvoked` observation, `outcome: Started`, `durability_class: before_effect`, with ID `obs-child-admitted:<call_id>`. It carries:
- `call_id`, `trace`;
- `decision_id` = the revalidation decision (`route_decision_id`);
- `subject_id` = Child name, `resource_id` = `child_instance_id` + artifact id;
- `policy_revision`, `grants_revision`;
- `detail_ref: mct-child-admission-v1:through_sequence=…;through_entry_hash=…;authority_epoch=…;generation=…;authority_state_hash=…;run_id=…;fingerprint_digest=…`.

It never carries a key, a payload, or any Child state. Its meaning is **"admitted for launch; the guest may have started"**. It does not mean the guest made progress. Recovery treats it as possibly started (C1, C2). Frame size is about 1.1 KB.

### Option A: synchronous start fact on the caller thread, before launch

**Mechanism.** In `execute_authorized_resident_child`, after admission, append `[ChildInvoked]` and block on the ack before `execute_resident_wit_child`. Two placements:

- **A1, after the order lock is released.** `ledger.admit_effect(...)` returns. Then `block_on(ledger.append([child_invoked]))`, then launch.
  - *Lock ordering:* nothing is held while waiting on the writer. No deadlock.
  - *What it gives up:* the fact is not ordered with admission. An authority mutation can commit between the unlock and the append. The chain then reads `revocation@N, ChildInvoked@N+1`, which is indistinguishable from a real C4 violation. The ledger proves launch, but not that the launch was legal.
- **A2, inside the order lock** (in the `start` closure of `admit_effect_start`).
  - *Lock ordering:* **deadlocks.** The effect thread holds `MotherAuthorityOrderV1.state` and waits for the writer's ack. The writer thread is processing an `AuthorityMutation`, has called `task_authority_order.commit_mutation` (`observation.rs:455-470`), and blocks on the same `std::sync::Mutex`. The bounded mpsc queue (256) behind it fills up. This is the cycle that `observation.rs:600-608` already warns about ("asking it while holding the lock could deadlock"). Rejected.

**Where written.** Resident ledger, through the existing `Write` command, as a 1-frame batch.
**Hot-path cost.** +1 writer round trip and +1 synced batch. Measured on a Linux ext4 VM: 76 µs p50 / 138 µs p95 for one ~1.1 KB frame with `sync_data` (scratch probe against `JsonlObservationLedger::append_durable_batch` at `49dfacd`, 280 samples). macOS: unmeasured here. The repo's H4 bench refuses non-Darwin hosts. Run `cargo bench -p mct-daemon --bench perf_phase_0` on the Mac and read `append_including_sync_data`.
**Recovery.** Calls with `ChildInvoked` and no terminal fact are interrupted. A `running` row without `ChildInvoked` can be released (see D for the upgrade caveat).
**Spec changes.** Durability decision for `ChildInvoked`; D-P0.6 boundary note.
**Gives up.** A1: the ordering proof. A2: liveness.

### Option B: group-committed start fact, launch gated on its own ack

**Mechanism.** The writer drains whatever `Write` commands are already queued (`try_recv` up to N frames or M bytes, no timer) and commits them with one `sync_data` (`append_durable_batch` already does one sync per batch, `mct-observation/src/lib.rs:1431-1499`). Every command's ack waits for that sync. The Child launches only after its own start fact is acked, so C3 holds. The batch never spans an `AuthorityMutation`, `FinalizeStartup` or `ProjectedAuthority` command: the writer flushes before processing one.
**Where written.** Same writer, coalesced with other calls' facts.
**Lock ordering.** Same as whichever of A1 or D it is built on. Group commit adds no lock.
**Hot-path cost.** At concurrency 1: identical to A (no wait timer, so nothing to coalesce). Under load: the per-call sync cost amortizes. On that VM, a 4-frame batch costs 125 µs versus 4 × 76 µs separately; 8 frames cost 167 µs. Latency never goes up, because nothing waits for company.
**Recovery.** Same as A or D. One addition: one failed sync now fails, and fences, every caller in the group. That already happens, since the first failure poisons the writer (`UncertainAppendPoisonsWriter`).
**Spec changes.** D-P0.8's "single-writer group commit and crash proof" slice. Doctrine C7 forbids it before the net exists. It needs the crash-state matrix at the new boundary (mid-group write, after write and before sync, after sync and before acks).
**Gives up.** Nothing semantic. It is a throughput optimization, not an answer to item 8. Do it later, under its own slice, on top of D.

### Option C: intent implied by the existing route/authorization fact (no new write)

**Mechanism.** Redefine the step-3 route batch (`RouteRevalidated` Allowed + `CallAuthorized`, already durable before launch) as launch intent. Recovery treats "local route revalidated, no terminal fact" as possibly started ⇒ interrupted.
**Where written.** Nowhere new.
**Lock ordering.** Unchanged. Nothing new to deadlock.
**Hot-path cost.** Zero.
**Recovery.** `ledger_evidence` adds local `RouteRevalidated` to `effect_started_calls`. A ledger-only pass can then close S1 calls as interrupted. But nothing at ingress stops the same-key retry (see D's guard, which C could also borrow, keyed on the route fact).
**Spec changes.** A decision note: "a local revalidated route is the launch intent". Recovery disposition table changes: "reservation, no run" becomes Interrupted when a route fact exists.
**Gives up.**
- Precision. Every call that crashed between route and launch, or was denied at steps 5 or 7 just before a crash, becomes `execution_interrupted` with its key pinned for 720 s. That is S5, made strictly worse.
- The ordering proof (§1.2 item 2). The route fact precedes the order admission, so revocation races stay ambiguous.
- The ledger never says the Child launched. An auditor still cannot answer S3.
- It changes the meaning of an existing fact after the fact. Old ledgers then read differently under new code, and "facts are immutable values" (`allium:2992`) becomes muddier.

### Option D (recommended): writer-arbitrated admission fact

**Mechanism.** Add a writer command `AdmitChildEffect { snapshot_expectation, observations, ack }`, where `observations` = `[ChildInvoked]`, optionally prefixed by the Toy/Watch evaluations from step 8. The writer thread does, in order, on its own thread:

1. Builds `projected_authority(&ledger, state_path)` exactly as the existing `ProjectedAuthority` command does (`observation.rs:120-140`).
2. Locks `MotherAuthorityOrderV1.state` and runs the existing `admit_effect` checks: fenced, `current_expectation` equals snapshot expectation, usable cursor matches.
3. On success, fills `through_sequence`/`entry_hash`/authority identity into the fact and calls `append_durable_batch` (one `sync_data`).
4. Unlocks and replies `Admitted { sequence }` or `Denied(reason)`.

A denial appends nothing; the caller writes today's denial report, unchanged. Any append error poisons the writer, and the reply is `Denied(Fenced(WriterPoisoned))`, so the Child does not launch (C3, C5). The caller thread launches the Child only after `Admitted`.

This replaces today's step 7, which costs a `ProjectedAuthority` round trip plus the lock on the caller thread. It also replaces the separate step-8 batch for Toy/Watch Children, so those calls keep their current count of synced batches.

**Where written.** Resident ledger, by the single writer, at the admission instant. The fact's chain position *is* the admission's linearization point. Every authority mutation is committed by the same thread (`observation.rs:455-500`; the offline `control.rs` path runs only under the exclusive writer lock, so never at the same time). Every mutation fact is therefore provably before or after every admission fact, and the writer checked authority at exactly that position.

**Lock ordering and why it cannot deadlock.**
- Order mutex holders after the change:
  - (i) the writer thread, for `commit_mutation`, `clear_fence_after_exclusive_rescan` and `AdmitChildEffect`. These are serialized by the writer loop itself.
  - (ii) Toy effects via `admit_effect_start` (`toy.rs:163-165`), which run the Toy backend inside the lock but fetch their proof before taking it (`observation.rs:615-628`).
  - (iii) the offline control process, which never coexists with a resident writer.
- A deadlock needs a lock holder waiting on the writer while the writer waits on the lock. The writer never waits on anything but the mutex and the disk while holding the mutex. Case (ii) never touches the writer inside its closure. So the wait-for graph has no cycle. Invariant **I1: no thread holding the authority-order mutex sends to or awaits the ledger writer.** It becomes a named, tested invariant.
- What remains is a *stall*, not a deadlock: if a Toy effect (for example a slow `git` Toy) holds the mutex, the writer and every ledger write wait for it. That is already true today for `commit_mutation`, and today's Child `admit_effect` also waits on the same mutex. So the coupling is not new, but it becomes visible because all appends queue behind it. A follow-up could move the Toy start onto the same writer-arbitrated path and remove case (ii) entirely (open question 5).
- C6 (calls outside the mutation sequencer): admission was already under the order mutex (C4 demands it). D does not put calls into the *administrative mutation* path. It moves where one existing check executes, and no control read takes the order mutex. The spec note must say this explicitly.

**Hot-path cost.**
- Plain WIT Child: +1 synced 1-frame batch, about 76 µs p50 on that VM, minus one removed `ProjectedAuthority` round trip, so roughly neutral on round trips.
- Toy/Watch Child: zero extra syncs, because the step-8 batch merges into the admission batch.
- Mac cost: run H4 on the covered revision before ratifying. Expectation: low single-digit ms, which is under 1% of the 485 ms v0.2.0 p50 (`BASELINES-v0.2.0-aarch64-apple-darwin.md:36`). That baseline is old and from a different revision; the Phase 0 profile is the number to quote.
- Serialization: admissions now serialize on the writer thread, which already serializes the four other batches of every call.

**Recovery.** Recovery becomes ledger-first.
1. Scan the validated prefix (already done for `ledger_evidence`). `open_admissions` = calls with `ChildInvoked` (local) or `PeerCallSent` (forwarded) and no terminal fact: no non-disposition `ResultRecorded`, no `obs-executed-on`, no restart disposition.
2. For each open admission, close it **interrupted** with today's disposition fact (deterministic ID, fact before state). If SQLite has its run or reservation, settle those as today. If SQLite has neither (S1/S2), insert the closed run from the fact's fields (`run_id`, call, Child provenance) and record the call in the admitted-call guard (below).
3. A `running` run whose call has **no** `ChildInvoked`, with a reservation and no effect evidence, was provably never launched: launch requires an acked fact, and a torn final frame is residue, not a fact. It can be **released** rather than interrupted (fixes S5). *Caveat:* this applies only to runs started under the new law. Runs left by a pre-upgrade binary have no admission facts, so they must stay interrupted. Mark this with a `admission_law` column on the run row, or by comparing the run's `started_at` tenure with the first tenure that carries an admission law marker (open question 3).
4. Cross-check: a SQLite run with a recorded result but no ledger terminal fact cannot happen in correct operation (the ledger append precedes `complete_run`, `pipeline.rs:1063-1074`). It means the ledger lost a committed fact. Refuse to start (quarantine-class), never guess.

**Admitted-call guard (closes S1 at ingress, not just at recovery).** The writer keeps an index of `call_id`s admitted within the idempotency window (720 s plus a margin, bounded by throughput × TTL). It is built from the ledger at open and updated on every admission. `AdmitChildEffect` for a `call_id` already in the index is refused with `ExecutionInterrupted` and no fact is written. The pipeline maps that to the interrupted reply, and the idempotency entry is completed with it. Because the fingerprint includes the semantic call id (`allium:2005-2008`), a same-key retry carries the same `call_id`. So even with SQLite fully lost, no retry within TTL can launch the Child twice. That is C1 enforced by the ledger alone. Unkeyed calls get the same protection within the window as a side effect. Whether that is wanted is open question 2.

**Spec changes.**
- New slice spec (for example `feat/child-admission-fact/SPEC.md`). It records the durability decision ("`ChildInvoked` is BeforeEffect, written only by the writer at admission"), per doctrine C7.
- `mct-product-map.allium`:
  - `TwoPhaseRouting`: add `EffectAdmissionIsALedgerFact`. Final local Child effect admission is a BeforeEffect fact appended by the single writer in the same order as authority mutation facts. The Child launches only after its acknowledgement. The fact means admitted and possibly started.
  - Restart contract: add `LedgerAloneBoundsInFlightWork`. A call with an admission or peer-send fact and no terminal fact is closed interrupted from the ledger, whatever the runtime state holds. A runtime-state run without an admission fact under the admission law never launched.
  - Add `AdmissionIsOncePerCallWithinWindow`.
  - Add I1 as `OrderHolderNeverAwaitsWriter`.
  - Amend the B10 decision text to say the admission rides the writer and not the admin mutation sequencer.
- `perf-phase-0/SPEC.md`: none if item 8 lands after Phase 0 closes. Otherwise, a D-P0 amendment, because the profiled path changes. `attribution.py` gains a `child_admitted` boundary (a truthful timestamp at last).
- `ledger-commit-recovery/SPEC.md`: recovery disposition table rows for "admission fact, no terminal", "run, no admission fact (new law)", and "SQLite result, no ledger terminal ⇒ refuse".

**What it gives up.**
- One fsync per plain local Child call.
- More work on the writer thread. Under heavy concurrency the writer becomes the admission bottleneck, which is what B later fixes.
- A Toy holding the order mutex now stalls all appends, not only admissions (this is visible today for mutations).
- The source-pin test and the `admit_effect` call-site shape change.
- About 150 lines of writer code on the most safety-critical thread, plus the standing crash and fuzz obligations.

### Considered and rejected

- **Make SQLite `synchronous = FULL`.** It closes S1 for the DB (about 3 extra fsyncs per call: reservation, run insert, completion). It does nothing for S3 or S4, and keeps two sources of truth. It is a reasonable belt-and-braces change, but it is not item 8. Open question 4.
- **Write the start fact from inside the guest or a host import.** The Child would be producing authority evidence. That violates "Mother holds all authority / adapters perform, kernel decides". Rejected.
- **Pre-write the start fact in the route batch (step 3) and rely on later denial facts to cancel it.** A crash between them leaves a "started" fact for a Child that never ran, and "cancel" facts amount to rewriting meaning. It has C's precision loss with A's cost. Rejected.

## 4. Recommendation

Adopt **D**, in its own spec slice, after perf-phase-0 closes (or with an explicit Phase 0 amendment). Add the admitted-call guard and the ledger-first recovery pass in the same slice. Defer **B** to the D-P0.8 group-commit slice. Do not adopt **C**.

Reasoning:

1. D is the only option where the ledger alone proves both that a Child was admitted and that the admission was legal at its chain position (C4). A1 proves launch but not legality. C proves neither.
2. D removes the deadlock by construction instead of by care: the lock and the append happen on the same thread, and I1 becomes a tested invariant.
3. D closes the S1 double-execution hole twice: at recovery (ledger-first pass) and at ingress (admitted-call guard). Neither depends on SQLite durability.
4. The cost is one fsync per plain call, and zero for Toy/Watch calls. The ledger is already on this path four times. The "off the hot path" objection is a sequencing constraint (Phase 0 is measuring), not a law.
5. It reuses `ChildInvoked`. Recovery already treats it as an effect start, and every `runtime_execution_started` counting test keeps its meaning.

Tests that would prove it (extend `crates/mct-daemon/tests/mother_restart.rs` unless noted):

1. `admission_fact_precedes_every_guest_start`: in the seeded SIGKILL loop, for every call with `RuntimeExecutionStarted`, assert a `ChildInvoked` for that call exists at a lower sequence. Add `admissions_for(entries, call_id) <= 1` next to `invocations_for <= admissions_for`.
2. `sigkill_after_admission_closes_interrupted_from_ledger`: wait on the **ledger** for `obs-child-admitted:<call>` (replacing the SQLite wait at `:1359-1371`), then SIGKILL. Restart. Assert an interrupted disposition fact, `execution_interrupted` on same-key retry, and one admission.
3. `state_db_rollback_after_admission_never_reexecutes` (S1): snapshot `state.sqlite*` after provisioning and before submission. Run a keyed call that is SIGKILLed mid-Child. Restore the snapshot, simulating NORMAL-sync rollback on power loss. Restart. Assert recovery closes the call from the ledger alone, a same-key retry gets `execution_interrupted`, and `invocations_for <= 1`. **This test should fail on `49dfacd`**: today the retry re-executes.
4. `state_db_loss_with_no_admission_fact_releases_and_runs_once`: same as test 3, but kill before admission, using a test-only env fault at the admission seam (mirroring `TestAppendFault`). Assert the retry executes exactly once.
5. `torn_admission_frame_is_residue_and_child_never_ran`: append fault that tears the admission frame. Assert recovery preserves the residue, nothing launched (no `RuntimeExecutionStarted`, no Child marker), and the reservation is released.
6. `revocation_racing_admission_has_one_ledger_order`: a call storm concurrent with a revocation. Replay the chain and assert every `ChildInvoked`'s recorded `through_sequence`/authority equals the replayed authority at its position, and that no `ChildInvoked` for the revoked Child appears after the revocation fact. This makes `authority_order.rs:623` a ledger-level property.
7. `toy_holding_order_lock_does_not_deadlock_admissions_or_mutations` (unit, `observation.rs`): a Toy closure blocks inside the order mutex while N admissions and one authority mutation are queued. Release it. Assert all complete within a bound. Also add a source-pin update asserting no `append`/`blocking_send` inside `admit_effect*` closures (I1).
8. `pre_upgrade_running_run_without_admission_law_stays_interrupted` (unit, `restart_recovery.rs`): a legacy-shaped run with no admission fact is still interrupted, never released.
9. `sqlite_result_without_ledger_terminal_refuses_start` (unit): the divergence cross-check fails closed.
10. Keep green: `before_effect_append_failure_suppresses_child_effect`, `serving.rs:1473/1558`, `idempotency.rs` `durable_execution_count` tests, and the `crash_on_first_ledger_write…` operator-gate test. Under the standing obligations, add admission-frame crash states to the restart crash matrix and preserve their seeds.

## 5. Open questions for you

1. **Sequencing with perf-phase-0.** Land item 8 after Phase 0's official run and close-out, or amend Phase 0 to profile with the admission fact in place? (Recommendation: after close-out.)
2. **Admitted-call guard scope.** Refuse a second admission of any `call_id` within the 720 s window (keyed and unkeyed), or keyed calls only? Unkeyed calls have no dedup promise today, but a Watch redelivery reuses deterministic call ids. Should those also be at-most-once?
3. **Upgrade marker.** How should recovery know a run was started under the admission law: a new `runtime_runs` column, or the tenure/epoch in which it started? This decides whether S5's "release" is available on the first restart after upgrade.
4. **SQLite `synchronous`.** Also move `state.sqlite` to `FULL` as defence in depth (≈3 fsyncs per call), or keep NORMAL and rely on the ledger-first design? With D, NORMAL becomes acceptable for idempotency. Without D, it is the S1 hole.
5. **Toy effects.** `ToyCallStarted` is also post hoc and Toy backends run inside the order mutex. Fold Toy admission into the same writer-arbitrated path now (removing lock holder (ii) and the stall), or leave it as a follow-up?
6. **What a peer may see.** Should the admission fact (or its sequence and hash) be returned in the `mct/call/0` reply or exposed to a future ledger-export audit, so that a forwarding Mother can pair `PeerCallSent` with the receiver's `ChildInvoked`? Today nothing crosses Mothers.
7. **Divergence policy.** For "SQLite has a result the ledger lacks", should Mother refuse to start (proposed), or start degraded with call ingress denied?

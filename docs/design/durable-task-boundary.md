# Durable task boundary — proposal for review

**Status: proposed, not implemented or ratified.** This fills the missing
questions between D-30 (re-execution with recorded results) and D-32 (restart
settles interrupted calls and never resumes them). It does not authorize
re-execution under a fresh idempotency key.

## Four different identities

| Identity | Lifetime | Meaning |
| --- | --- | --- |
| Task | User's logical work | May outlive a process and several invocations |
| Invocation | One admitted execution attempt | Ends once; a crash gives it a terminal or unknown outcome |
| Effect | One logical external operation within a task | Must retain identity across replay; cannot be inferred from a new invocation ID |
| Checkpoint | A committed task continuation boundary | Names code/input/journal position, not a snapshot of arbitrary WASM memory |

A future continuation is a newly authorized invocation of the same task,
consulting the task's effect journal. It is not restart recovery silently
restarting the old invocation. Mother admits work; the application owns the
meaning of its task state.

## Effect crash matrix

| Durable evidence | Safe default | Additional mechanism needed to do more |
| --- | --- | --- |
| No dispatch/admission | No claim that execution began | A valid new admission |
| Dispatch reserved; exact start unknown | Interrupted/unknown, no automatic repeat | Stronger evidence that no effect began |
| Effect intent committed; no result | Interrupted/unknown | Destination idempotency or reconciliation protocol |
| Effect result committed and available | Replay recorded result without reissuing effect | Current authorization to read it |
| Result digest exists but bytes are unavailable | Refuse exact replay | Durable result storage or an explicit recompute policy |
| Recomputed read has different bytes | Diverged | Application decision: abandon, reconcile, or new task/input |

A single journal engine can implement these outcomes, but cannot erase the
difference between a read, a nondeterministic inference, a state mutation, and
an irreversible external request. “Replay safe” cannot be replaced by an
unqualified promise to repeat everything.

## Pause, failure, and reconstruction

- A cooperative pause retains a live instance only while its host remains
  alive. Define a guest/host checkpoint interface and which state is durable.
- A process crash destroys its in-process WASM guests. There is no finishing
  the current step after host death. Recovery records only what it can prove.
- Authority fencing while the process lives blocks new protected effects;
  already-admitted operations and preopens follow their existing lifetime.
- Resuming a task requires current Mother authority before accessing task
  storage, and a defined code/input compatibility check.
- A maximum step duration and a hard deadline are different controls. A
  deadline trap is termination, not a resumable cooperative checkpoint.
- A freeze during writer loss cannot be synchronously committed by that dead
  writer. Later recovery must name an inferred interruption, not fabricate an
  exact historical freeze timestamp.

## Determinism boundary

Keeping standard secure randomness and clocks honest remains D-06. A task
claiming deterministic replay must either exclude those inputs from replayed
computation or journal their relevant effects through an explicitly designed
interface. A seeded Toy does not account for arbitrary standard imports,
concurrent completion order, changing filesystem bytes, or missing model
responses.

A code hash pins what may resume; it is not an upgrade strategy. Decide whether
a task stays on its immutable artifact until completion, receives an explicit
state migration, or terminates when the artifact becomes unavailable. Do not
silently resume different code against an old journal.

## Smallest feasibility experiment

Before building all of Pi as a Child:

1. Build one minimal component with a two-step task and grant-gated state.
2. Invoke one controlled external-effect fixture with stable logical effect ID.
3. Crash before intent, after intent, after effect, after result commit, and
   after checkpoint commit. Show the matrix above rather than assuming success.
4. Revoke a grant between attempts and prove a continuation cannot restore it.
5. Change the code hash or input, and return a typed compatibility/divergence
   outcome rather than guessing.
6. Demonstrate duplicate wake delivery produces one continuation admission.
7. Measure warm/cold invocation and recovery cost on that actual path.

For the Pi feasibility branch, additionally demonstrate a real TypeScript-to-
component build and one model/state Toy boundary. The existing notes explicitly
did not establish jco/StarlingMonkey feasibility. Application checkpoints are
worth evaluating; lack of transparent Wasmtime snapshots does not by itself
prove full-history re-execution is the only possible application design.

## Owner decisions before implementation

- Who schedules task continuation and how are task/effect IDs scoped?
- Which external effects support reconciliation/idempotency, and which stop at
  unknown outcome?
- Where do result bytes live, with what retention, access, and backup guarantees?
- What maximum stop delay is acceptable, and which operations can be cancelled?
- How do artifact upgrades, revocation, and expired dedup windows affect tasks?

The dispatch fix on this branch intentionally answers none of these by stealth.

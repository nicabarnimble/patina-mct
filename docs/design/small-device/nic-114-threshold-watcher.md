# NIC-114 — threshold watcher: wake on crossing

**Status: design proposal, not implemented or approved for build.**
Target: Raspberry Pi 3 Mothers, approximately 905 MB RAM, aarch64 Debian.
[Work item: NIC-114](https://linear.app/nicabar/issue/NIC-114), under
[NIC-109](https://linear.app/nicabar/issue/NIC-109).

## 1. Decision and scope

Use an opt-in **stateful-event Child**: a small deterministic threshold reducer
inside bounded WIT invocations, with durable subscriptions, checkpoints and an
outbox held by Mother through narrowly granted Toys. Its watch lasts across
invocations and reboots; a Wasmtime instance does not. When no input arrives,
there is no running guest, tick loop, agent poll, or inference call.

Health, sensor and log Children supply structured observations. The watcher
emits one event per qualified threshold transition. Mother delivers that event
to an authorized agent inbox on a capable host; only that delivery can request
an agent wake. Remaining above a threshold does not repeatedly wake the agent.
An agent need not query a Pi to discover changes. Mother may schedule bounded
producer calls when hardware has no interrupt feed; that is explicitly not
agent polling, and producer sampling cost is accounted separately.

The Pi is a hand on local hardware, not a place to think. No model, Git, shell,
heavy build, service mutation or general network client belongs in this Child.
Git Children remain macOS-only on Studio/Lume. No Mother on a microcontroller.
A wake conveys evidence, never permission to restart a service or perform a
NIC-116 action. Those actions require their own admission and grants.

### Baseline and evidence boundary

Source baseline: `bc48d1e0d721d1531a58192a69ef65ffd234882f`.

- [First-app plan](../../plans/first-real-app.md) and
  [repo-report WIT](../../../crates/mct-repo-report/wit/world.wit): bounded
  read-compute is the reference, not the legacy watcher lifecycle world.
- The `docs/mct-vision-child-plan.md` consultation was read in the primary
  checkout; it is not present at this branch's baseline. Its sections B.1–B.3
  recommend bounded operations, opt-in state/event imports and explicit storage
  generalization. [NIC-104](https://linear.app/nicabar/issue/NIC-104) records the
  approved profile/lifecycle decisions. This document does not copy or land that
  parallel plan. Artifact, approval, assignment and instance remain four facts.
- [wasm.rs](../../../crates/mct-daemon/src/wasm.rs):
  `DEFAULT_WASM_MEMORY_LIMIT_BYTES` is 64 MiB; `store_limits` limits linear
  memory, not total Mother RSS. Calls use fresh stores and epoch deadlines.
  `compile_snapshot` currently uses `Component::from_binary`; a safe precompiled
  loader is a dependency, not something this Child already has.
- [Invocation budget](../../../crates/mct-daemon/src/wasm/budget.rs): default
  limits are 4,096 host calls, 256 logging calls, 1,024 measure calls, 1,024 Toy
  calls, 2,050 observations, 4 MiB cumulative accounting, 64 KiB host values and
  256 KiB output per invocation. Dynamic WIT value/container overhead is charged;
  wire size alone does not establish fit.
- [ToyGrant](../../../crates/mct-kernel/src/toy.rs) and
  [effect adapters](../../../crates/mct-daemon/src/toy.rs): exact subject,
  resource/action scope, expiry and current authority checks are reusable.
  `needs` requests authority; it never grants it.
- [Supporting Toy catalog](../../../crates/mct-daemon/src/daemon/watch.rs) and
  `link_keyvalue_host_import` / `link_messaging_host_import` in `wasm.rs` provide
  Watch-oriented keyvalue and messaging. Separate keyvalue writes plus sends
  are **not** an atomic checkpoint/outbox. Messaging validates file-change
  shapes; it is not a general sensor bus or durable agent inbox.
- [Trigger domain](../../../crates/mct-kernel/src/trigger.rs) has an event class;
  this alone does not supply a generic observation source/durable wake adapter.
  [Durable-task proposal](../durable-task-boundary.md) is still a proposal.
  [D-32](../../DECISIONS.md#d-32-restart-recovery-settles-it-never-resumes)
  settles interrupted invocations; this design does not silently resume them.

## 2. Shared-contract alignment and required additions

[NIC-110](https://linear.app/nicabar/issue/NIC-110) owns the shared result
envelope, signed receipt, fan-out result, budget declaration, precompiled
artifact pipeline and handover contract. Its issue and the parallel drafts
`docs/design/small-device/nic-110-shared-contract.md` and `wit/shared.wit` were
read in the NIC-110 checkout (package `patina:small-device@0.1.0-draft`). They
are not yet in this branch; this document does not copy or modify them. Use
its `common`, `receipts`, `fleet` and `delivery` records, with the mappings
below. Watcher-specific names remain provisional; reconcile both drafts at
review before freezing signed bytes or building.

| Shared concept | NIC-114 use / requirement on NIC-110 |
| --- | --- |
| Result envelope | `common.call-context` carries job/task/call IDs, attempt, target, request/artifact digests and effective budget; `result-envelope` carries outcome/reason/effect, `child-answer`, coverage and usage. Watch assignment/config revision is bound in the watcher request/payload. A zero-crossing call still returns a result. |
| Signed receipt per call | `receipts.receipted-result` binds the Mother-owned envelope to a `signed-receipt` with a ledger position. The draft selects `ed25519-v1`; signing preimage, trust binding and rotation follow NIC-110, not a Child-designed key scheme. Receipt covers success, denial and failure outcomes, not just crossings. Child holds no signer. A hash chain alone is not this receipt. |
| Offline handover | Use `delivery.handover-item` / `handover-ack`: delivery ID, destination and result digest; crossing items also use `event-ref` with task/event IDs, source sequence and checkpoint digest. Retain exact bytes until acknowledged; revalidate export authority on retry. |
| Fan-out result | Use `fleet.receipted-fleet-result`: requested members, a `node-answer.received` or `.missing` for every node, counts, revision/supersedes and optional domain merge. Target denial/failure/timeout is a received terminal result; unreachable/no verified reply is missing, possibly `may-have-executed`. Distinguish completeness from whether a metric is normal. |
| Budget declaration | Use `common.operation-declaration` / `budget` for §7's memory/time/input/output/call limits. Host RSS, disk queues, rate and retention are additional admission requirements not expressible in the current budget record; NIC-110 must specify how they are declared. |
| Stateful-event extension **needed** | Durable source identity/order, gap indication, idempotent checkpoint + cursor + outbox commit, ledger/projection reconciliation, and destination durable inbox dedupe. Do not infer these from a generic result envelope. |
| Receipt unavailable | If a peer never receives a call, cannot sign, or dies before terminal evidence, the coordinator records its own attempt receipt and an explicit missing target receipt. Never forge target success or a target signature. Recovery may issue a receipt for an evidenced interrupted outcome. |

Projection: export typed watcher WIT records plus `common.coverage`; Mother
encodes them as a versioned `typed-payload` schema
`patina:threshold-watcher/result@0.1.0` (RFC 8785 canonical JSON; signed 64-bit
values/sequences outside the exact JSON numeric range are decimal strings by
schema). Use NIC-110's deterministic-CBOR receipt preimage and lowercase hex
BLAKE3 digest convention, never hash native WIT ABI layouts. Stable watch
identity maps to `task-id`; each command/step maps to `job-id`. Retransmission
uses the same call ID and request digest; an independently admitted new attempt
retains job/task IDs, gets a new call ID and increments `attempt`. Reacquisition
returns immutable result/receipt bytes without executing a guest.
Map normal/no-crossing/duplicate to `success/none`; bad inputs to
`failed/invalid-input`, revision conflicts to `failed/conflict`, queue pressure to
`failed/budget-exceeded`, incompatible state to `failed/conflict`, missing storage
to `failed/unavailable`, authority refusal to `denied/not-authorized`, and
interruption/uncertain delivery to the shared `execution-interrupted` /
`effect-unknown` reasons with truthful effect state. Per-input details remain in
successful typed bodies, not new shared outcome enums. Failed/denied terminal
envelopes carry no answer per NIC-110; detailed diagnostics remain ledger-side.
Before durable admission, use
`delivery.submission-state.not-admitted`; do not promise a terminal signature.

Use `common.source-ref` and `sample-origin` for gateway/resource/device/channel,
time, freshness, sequence and evidence digest. Add watcher schema fields for
producer assignment/operation, metric/unit, mandatory stream incarnation and
sequence, gap/coverage and receipt lookup. The draft's optional sequence is
insufficient for a watch: sources without an ordered continuity contract cannot
be admitted merely because they fit `sample-origin`.

Receipt anchors must remain verifiable after restart and ledger segment rotation.
Payload retention and verifier availability must be explicit; a digest without
result bytes is not replay. Producer receipts attest the producer's result, not
physical sensor accuracy. Mother verifies provenance before input admission.

## 3. WIT world and inputs/outputs

Proposed versioned identity: `patina:threshold-watcher@0.1.0`, world `child`.
The callable identities are `control.configure`, `control.pause`,
`control.status` and `events.advance`, not legacy `init`/`tick`/`handle` hooks.
Per-operation profiles: configure/pause/advance are `stateful-event`; status is
`read-compute` over committed state. The internal transition reducer is pure.
Each operation declares its own effective budget using the ceilings in §7.

Illustrative WIT shape (design notation; record definitions are specified in
the tables below, not a compilable or checked-in package):

```wit
package patina:threshold-watcher@0.1.0;

interface control {
    // All named request/result records are provisional typed records.
    configure: func(request: configure-request) -> result<watch-result, watch-error>;
    pause: func(request: pause-request) -> result<watch-result, watch-error>;
    status: func(request: status-request) -> result<watch-result, watch-error>;
}
interface events {
    advance: func(request: advance-request) -> result<watch-result, watch-error>;
}
world child {
    // Domain interfaces use common.coverage/source-ref/sample-origin from
    // patina:small-device@0.1.0-draft; type reuse confers no host authority.
    // NEW authority-bearing capability, not existing WASI keyvalue semantics.
    import patina:watch-state/store@0.1.0;
    export control;
    export events;
}
```

The proposed store interface offers bounded `read`, revision-checked
`configure`, `pause`, and `commit-step`. `read` returns the committed watch
state and a host-admitted inbox batch. `commit-step` names the expected state
revision, batch identity/digest, next state and candidate crossing events. It
atomically commits consumption, checkpoint, result payload and outbox intents
as described in §6. A watch ID is a lookup key, **not a portable capability**.
The host independently bounds and authorizes every import. No guest preopen,
SQL connection, raw socket, secret, clock or RNG import is needed.

Mother supplies authenticated inputs, time-quality facts and admitted source
references; Child computes domain decisions. Watcher-owned IDs are bounded
(≤128 UTF-8 bytes); shared IDs retain NIC-110's ≤256-byte ceiling. References
carry digests rather than embedded receipt histories. Read/status page only the
touched rules or requested status slice. Configure refuses an oversized rule set
without partial mutation; declared maxima do not promise that every combination
fits one call. No input, snapshot or commit may exceed the ABI budget. Mother owns receipt wrapping and
signing outside the export. WIT domain errors map to shared terminal outcomes;
`unchanged`, `baseline-only` and `duplicate` are successful domain dispositions,
not invented transport outcomes.

### Input records

| Record | Required fields and semantics |
| --- | --- |
| Common call context | Shared request/invocation IDs and input digest; host-verified caller and scope remain outside guest-provided authority claims. Reusing a logical request ID with different bytes is `identity-conflict`. |
| Configure request | Watch ID, expected revision (absent only on creation), rule set, source selectors, delivery route reference and resource limits. Repeating identical creation is idempotent; conflicting revision is refused. Configuration never creates a grant or arbitrary destination. |
| Rule | Rule ID/revision, exact source selector + metric schema/version, direction (`high` or `low`), signed fixed-point integer threshold and reset boundary, unit/scale, confirmation count 1–8, maximum inter-sample gap, maximum sample age, enabled flag, and whether recovery crossings also wake. Defaults: confirmation 1; recovery wake off. No expressions, scripts, regex or floating-point NaNs. |
| Source selector | Producer Mother + assignment + operation, opaque stream/resource ID, metric ID, schema, classification, and optional leaf ID. These are logical identities, not Pi-local paths, GPIO pin assumptions or peer addresses. Up to 16 streams / 32 rules; at most four rules per observation. |
| Observation | Stable source event ID, stream incarnation, ordered sequence, typed integer value, unit/scale, validity/quality, source observation time with clock-quality flag, host receipt time, source result digest/receipt reference, optional bounded window interval and coverage. A log count includes its window; missing results never mean zero. |
| Advance request | Watch ID, scheduled inbox-batch ID and expected revision. Only a currently authorized Mother event-source adapter supplies the inbox; callers cannot inject unsigned values by calling `advance`. Up to four observations per step. |
| Pause request | Watch ID, expected revision and reason code. Stop further evaluation/delivery admission; preserve checkpoint and queued evidence. No deletion or implicit cancellation of already admitted effects. |
| Status request | Watch ID, view (`summary` or `committed-events`) and bounded page cursor. Return state/config revision, source freshness/gaps, latest committed cursor, queue depth, delivery dispositions and receipt references, or a bounded page of committed crossing records. Separate read/export authorization applies to event pages. This is operator/on-demand inspection or host publication, not required agent polling. |

### Outputs

`watch-result` contains watch/config/state revisions, consumed batch/cursor,
per-input dispositions, rule state summary and at most 16 bounded crossing
records. Each record has a canonical digest; the signed result binds the exact
event bytes. If the complete answer/commit would exceed a bound, shrink the
input batch before processing, not truncate the event list. Each crossing's
handover carries the original receipted result plus its selecting `event-ref`;
multiple crossings from one step may reference the same immutable call receipt.
If the step committed but its invocation later trapped or was interrupted before
terminal success, retain that failed/interrupted receipt with confirmed effect
state. A freshly admitted `status` event-page read may publish the already
committed crossing records with its own successful receipt; it does not rerun
the transition. This separates durable event creation from invocation success. Domain reasons include `invalid-rule`, `invalid-observation`,
`source-mismatch`, `unit-mismatch`, `sequence-gap`, `stale`, `clock-uncertain`,
`identity-conflict`, `revision-conflict`, `queue-full`, `storage-unavailable`,
`state-incompatible` and `authority-denied`. Internal policy detail stays in
Mother's ledger; caller-safe failures use the shared envelope.

Each crossing event carries a stable event ID, watch/rule/config revision,
source identity and triggering sequence, prior/current state, threshold/reset
boundary, value/unit, source evidence digest, occurrence time/quality, commit
anchor and delivery status. Notification contains bounded structured evidence,
not raw logs or an LLM-written summary. A wake can fetch further authorized
context later. `queued`, `delivered-to-inbox` and `agent-handled` are distinct;
queueing is not proof that an agent ran.

## 4. Crossing semantics

For a high rule, assert when `value >= threshold`; rearm when
`value <= reset`, requiring `reset < threshold`. A low rule reverses the
comparisons and requires `reset > threshold`. Integer units/scales must match
exactly; conversions, log aggregation and rate calculations belong upstream.
Values in the hysteresis band leave the established state unchanged.

State per rule: `uninitialized`, `normal` or `asserted`, plus pending direction,
confirmation count, last accepted value/sequence, and a separate freshness /
coverage flag. Confirmations require N consecutive eligible samples on the new
side with no gap larger than the declared inter-sample limit. A sample outside
that candidate condition resets its count. This is sample confirmation, not a
claim of continuous-time dwell. Duplicate delivery does not add a confirmation.

1. **Initial baseline:** the first valid sample establishes state without a
   crossing or agent wake, even if already high. In the hysteresis band, stay
   uninitialized until a boundary is reached. Status explicitly reports
   `baseline-only`; clients needing a startup alarm must separately approve that
   policy, not disguise it as an observed crossing.
2. **Edge:** a confirmed normal → asserted transition emits an `asserted` event
   and requests one logical wake. Further asserted samples do nothing. A
   confirmed asserted → normal transition emits `recovered`; it wakes only if
   that rule explicitly enables recovery wakes. Re-crossing can wake again.
3. **Ordering:** serialize updates per watch. Host persists per-stream sequence
   and incarnation. Same ID + same digest is a duplicate; same ID + different
   digest is refused. Older input cannot roll state backwards. Out-of-order
   delivery is buffered only within the bounded inbox; a missing sequence blocks
   progress until replay or an explicit gap record, not an invented value.
4. **Gaps/invalidity:** stale, invalid, wrong-unit or uncertain-time input does
   not advance a confirmation or fabricate a recovery. Record a visible
   coverage break, clear pending confirmation and require a fresh baseline
   before declaring a new crossing. An explicit gap disposition can consume
   the unusable sequence durably. A producer incarnation change without a
   proved continuity token has the same behavior. Silence alone causes no
   numeric crossing; freshness is visible in status. A missing-data alert
   would need a separately designed clock/event rule.
5. **Reboot:** restore established state, cursor and queued events. If ordered
   fresh observations continue in the same stream, an asserted value remains
   asserted and does not wake again. A discontinuity requires the gap behavior
   above. Never persist an in-memory monotonic clock as a cross-boot timestamp.
6. **Rule edits:** apply compare-and-set at a committed input cursor. A changed
   threshold/source/confirmation policy creates a new rule revision and fresh
   baseline. Unchanged rules retain state. Older pending events retain their
   original revision; editing configuration cannot relabel or silently erase
   them. Upgrade needs explicit state-schema compatibility or migration, not
   execution of new code against an old opaque checkpoint.

Example: high 80, reset 75, confirmation 1. Values
`70, 79, 80, 81, 79, 75, 76, 80` yield assert at the third value, recovery at the
sixth, then assert at the eighth: three events, two default agent wakes.
No debounce timer, model judgement or repeated warning while high is needed.

## 5. Toys and authority

| Capability | Existing versus new | Exact use and scope |
| --- | --- | --- |
| Durable watch-state Toy (`patina:watch-state/store@0.1.0`, provisional) | **NEW contract and adapter**. Existing `wasi:keyvalue/store@0.2.0` is prior art only. | Bounded read/configure/pause/commit-step on one watch resource, namespaced by Vision/node/project/assignment. One serialized writer and revision fence. Commit must cover checkpoint, consumed input, domain result and event intents together. No generic transaction or SQL power. |
| Structured observation ingress / event source | **NEW/generalized Mother adapter**, not an extra guest import and not the filesystem Watch Toy. | Admit exact producer streams and signed results into a durable inbox. Bind `CallTriggerAuthority` to that source and this watch's `events.advance`. A producer's read grant alone does not authorize publishing to a watcher. |
| Crossing delivery / agent wake | **NEW/generalized durable Mother delivery adapter**, not an ambient network Toy in the guest. | Consume committed outbox entries, export only permitted classification to one configured receiver operation, revalidate peer/call/trigger/export authority, obtain durable inbox acknowledgement. Routing references never confer authority. |
| Logging / measure | Existing catalog entries, **not required imports**. | Mother emits bounded call/queue/resource observations. Do not add guest logging just to make the world broader. |
| Sensor/GPIO/serial, journal and health read | Separate producer dependencies; new Toys belong to NIC-111/112/115 as appropriate. | Watcher receives typed values; it never acquires those hardware or file grants itself. |

A ToyGrant must name the exact artifact/version, assignment and admitted caller;
Vision, local node, project when applicable, data classification, watch resource,
allowed action and finite expiry are explicit. Avoid optional wildcard scope
fields for this application. Mother evaluates grants into `AuthorizedToyCall`
and rechecks current revisions immediately before every protected store/delivery
effect. Subscription admission is not perpetual authority. Expiry or revocation
pauses new processing or export, leaves queued evidence intact and records a
safe denial. After renewal, fresh admissions may continue the same watch.

Stay within one admitted Vision for initial producer → watcher → receiver
composition. NIC-104 permits only read-only inspect as the initial cross-Vision
path; this proposal does not silently authorize cross-Vision subscriptions,
state writes or agent wakes. No failover Mother takes over an unavailable watch.
No shared fleet key, guardian recovery or general graph scheduler is introduced.

## 6. Persistence, reboot and offline queueing

### Proposed commit boundary (build prerequisite)

The new Toy is not implemented by calling existing `bucket.set` then `send`.
That order can lose or duplicate an alert on power loss. Use one durable,
canonical step record with a revision predicate, including input identity and
consumption, next rule state, domain result bytes/digest and all outbox intents.
Mother chooses the bounded persistence format; Child supplies no authority facts.

1. The source adapter durably admits a bounded observation batch with evidence
   references before acknowledging it to a producer. Reserve input, output,
   receipt and ledger space before admission; upstream keeps unacknowledged data.
2. Under current authority, `advance` reads the next batch and committed state.
   Child computes a pure transition. `commit-step` validates batch/revision,
   schema, source bounds and output quotas; no input is consumed on refusal.
3. Mother durably appends the step commit to its canonical ledger **before**
   exposing the result or delivering anything. The record must carry enough
   bounded data (or a durably installed, digest-verified payload reference) to
   reconstruct checkpoint and outbox; a hash of lost bytes is insufficient.
   A SQLite WAL projection applies that committed record idempotently. The
   ledger and SQLite are not pretended to be one atomic database transaction.
4. Receipt finalization follows NIC-110's separate terminal-envelope staging,
   terminal observation and signing order. A step commit proves its state
   effect, not necessarily successful export return. Delivery is eligible only
   once canonical commit and a receipted event-bearing result exist (§3's fresh
   status read handles a committed step with a failed/interrupted invocation).
   A signer failure leaves `receipt-pending`, never a falsely signed success.
   If terminal evidence survives, recovery finishes that same receipt; otherwise
   settle interruption honestly, without rerunning the reducer.
5. Mother dispatches a committed crossing under new current admission to a
   durable receiver inbox. Receiver deduplicates by event ID + digest before
   scheduling the logical agent wake and acknowledges durable inbox acceptance.
   Destination-side wake scheduling must also be durable/idempotent; a socket
   write or transport ACK is not enough.

Logical event ID binds originating Mother, Vision/assignment/watch, rule revision,
source incarnation/sequence and transition direction using the shared canonical
hash convention. It is not derived from a transient invocation ID. Delivery
attempts admitted as new calls have distinct call IDs and receipts but retain
this event ID; transport retransmission/reacquisition retains its call ID too. Same ID
with different content is corruption/conflict. State revision and source high-
water marks prevent resurrection after the ordinary call-dedupe TTL expires.

| Crash/drop point | Required recovery behavior |
| --- | --- |
| Before durable inbox admission | No producer ACK; retry same source event. |
| After inbox admission, before step commit | Recover pending input. Settle the interrupted invocation per D-32. A separately admitted later invocation may process the still-unconsumed batch. |
| Step append ACK lost | Recover the maximal valid ledger prefix. If commit survives, apply it once; do not assume failed ACK means no commit. |
| Commit survives, SQLite update/reply missing | Rebuild projection; reacquire a terminal result if one committed, otherwise settle interruption and publish the committed event through a fresh status read. No second crossing; projection errors never undo canonical facts. |
| Outbox sent, receiver ACK lost | Retry/reconcile same event identity; receiver returns original inbox acknowledgement and does not schedule a second logical wake. |
| Receiver cannot prove whether it accepted | Report `delivery-unknown`; do not retry under a new event ID or claim exactly-once agent execution. |
| Corrupt state, ledger discontinuity or missing committed payload | Quarantine/pause, preserve evidence, report unavailable. Never start from an empty healthy baseline by treating unreadability as absence. |

The service is logically long-lived; there is **no** transparent Wasm stack
snapshot, resumed stale call capability or automatic retry of an unknown
external mutation. The guarantee is durable intent + at-least-once delivery +
idempotent inbox acceptance, not exactly-once inference or agent side effects.

### Offline and saturation policy

Loss of WAN/receiver does not stop local crossings while local authority remains
valid and reserved capacity exists. A required live policy check that is
unavailable blocks admission; the Pi cannot pretend to know remote revocations.
Untrusted wall time after reboot blocks time-sensitive authority rather than
extending grant expiry. Queue both events and signed call results
locally, in configured private Mother storage, not `/tmp` or the repository.
On reconnect, deliver in committed order per watch; expose occurrence time and
delivery delay so an old alert is not presented as live. Any latest-state update
is a separate newly receipted status result, never an edit of signed event bytes.
Do not collapse assert/recovery pairs or discard them to improve apparent health.

Use NIC-110's **per-Mother shared** ceiling of 256 pending records / 16 MiB,
not a fresh allowance for each Child. Proposed watcher subdivisions, available
only if unreserved by siblings: inbox ≤128 observations / 4 MiB; event intents
≤64 / 2 MiB; terminal result/receipt packages ≤64 / 8 MiB. Count intents and
packages separately even when related; first byte/count/global cap wins.
Checkpoint/config projection ≤256 KiB plus ≤1.75 MiB index/WAL allowance fit
inside that 16 MiB reservation. The shared 1 MiB emergency settlement reserve
is separate. A smaller effective reservation must backpressure earlier.

NIC-110 retains acknowledged results at least seven days and retains identity
tombstones after payload expiry. Propose a separately admitted ≤64 MiB on-disk
retained-result/tombstone allocation for this watch, paged rather than resident;
exhaustion also blocks new work. Its bounded compaction/stream-retirement policy
needs shared review. The canonical ledger has a separate capacity/retention
budget; these are not total disk usage claims. Every call consumes ledger and
receipt space, even with no crossing. High-frequency monitoring therefore
requires explicitly budgeted evidence storage, not just a small guest.

At 75% capacity expose structured pressure; at capacity refuse new admission
with `queue-full`, retain committed data, and backpressure the producer without
ACKing lost input. Preserve a separately reserved diagnostic/terminal record
slot; if even required evidence cannot persist, fail closed. If a producer
cannot retain data, its next admission must declare a gap and new baseline.
Finite storage cannot promise unlimited offline history or uninterrupted
monitoring. No silent overwrite, drop-oldest or time-based removal of
unacknowledged work;
explicit owner-authorized discard requires NIC-110's tombstone. Unstarted calls
expire at their deadline (at most the shared 24-hour queue-age default); their
inputs remain accounted for and any source continuity loss becomes a gap.
Acknowledged payloads may be compacted only after the shared minimum retention,
preserving ledger anchors, source cursors and dedupe evidence; retired stream
IDs are fenced against replay. These policies require implementation proof,
not reliance on the existing 12-minute call cache.

## 7. Memory, startup and work budget

These are **proposed acceptance limits, not measurements**. Measure on the
approximately 905 MB aarch64 Debian Pi 3, including serialization, receipts,
fsync and host-side bookkeeping. Do not equate the 64 MiB linear-memory cap
with total process consumption.

| Resource | Proposed limit / acceptance target |
| --- | --- |
| Guest linear memory | 4 MiB initial target, 8 MiB hard aggregate maximum across all component memories; no unbounded growth. Current per-memory limiting alone is insufficient. |
| Host memory attributable to watcher | ≤8 MiB live batch/state/queue buffers; queues are disk-backed and paged. |
| Total incremental Mother peak RSS | ≤24 MiB over the same idle Mother baseline, including guest, adapter, compiled-code cache and SQLite working set. Concurrent watches must be globally admitted, not each given an unaccounted allowance. |
| Logical capacity | Initial admission: one watch assignment, ≤32 rules / 16 streams, four observations/step, four matching rules/observation, at most 16 transition events/step (`max-items = 16` for advance, 32 for control rule sets). NIC-110 admits one active invocation per Pi 3 across all siblings, not merely one per watch. |
| Input/output | Request ≤16 KiB, observation ≤1 KiB encoded, crossing ≤1 KiB, domain response ≤16 KiB, shared result + receipt ≤32 KiB. Typed host values ≤16 KiB including dynamic `Val` overhead, matching the shared starting limit; reduce batch/page size before exceeding either bound. Never split one input's checkpoint from its associated events. |
| Per-invocation accounting | ≤32 host calls, ≤8 Toy calls, ≤32 observations, no guest logging/measure calls, ≤512 KiB cumulative charged data. Adapter expands/reserves actual observation costs before commit; overflow fails safely. |
| Startup from compatible `.cwasm` | Cold load to first bounded callable result p95 ≤1 s over 30 trials, excluding full Mother boot; measure separately from receipt/network delay. |
| Event step | Reducer CPU p95 ≤5 ms; admitted batch to durable local result p95 ≤100 ms warm. `startup-ms = 1000`, `wall-ms = 2000` hard ceilings (startup included); use the earlier caller deadline. Bound Toy I/O waits too: epoch traps alone do not bound host I/O. Slow storage/deadline cannot turn an uncommitted step into success. |
| Reboot readiness | After Mother verifies its ledger, watcher checkpoint/outbox ready p95 ≤2 s; report total boot/ledger replay time separately. No scan of unbounded event history on every wake. |
| Load / idle | 24-hour lifecycle test at one observation/minute aggregate with active receipt collection; separate 60-second burst at 10 observations/s using up to four-item batches (≤100 ms wait). Incremental CPU average ≤5% of one core during the burst. No-input guest executions = 0 and no agent wakes; host delivery retries back off. These loads do not imply unlimited retention: measure seven-day evidence growth against disk reservations. |
| Overload | Reject/backpressure above admitted input/storage/CPU capacity. Never convert a benchmark target into an unbounded retry queue. |

A batch can flush earlier to keep latency bounded. CPU and SD-write cost may
force a lower admitted sampling rate at review; if these targets fail, report
that failure rather than increasing the 64 MiB ceiling. Shared receipt sizes
and ABI overhead must be measured before claiming the table fits.

**Artifact pipeline:** nicamaster builds the component and precompiles an
AArch64 Pi-3-compatible `.cwasm` with the pinned Wasmtime version, engine options,
CPU feature floor and WIT/profile metadata. Bind source Wasm digest, compiled
artifact digest, target and producer provenance to Mother's approval. Pi Mothers
verify trusted provenance/integrity and exact compatibility before loading;
untrusted native serialized code is never deserialized as a convenience.
An incompatible or missing `.cwasm` is a typed artifact refusal, **never local
compilation fallback**. A separately compatible nicamaster artifact is allowed
for its fleet entry. The loader/pipeline belongs to NIC-110; no such build or
load is performed in this design task.

## 8. Composition, fleet answer and MCU gateway

Recommended decomposition:

1. **Producers (NIC-111/112/115):** acquire health, bounded log counts/windows,
   sensor samples through their own Toys. Publish typed results, source order
   and receipt evidence. Raw log parsing and sensor normalization stay here.
2. **Threshold reducer:** pure transition function over rules + state + admitted
   observations, packaged inside this stateful-event Child initially. Extract a
   separate pure Child only if another composition needs that WIT operation;
   do not pay extra remote calls for every comparison.
3. **Mother state/event adapters:** persist inputs, state and delivery intents,
   enforce authority and schedule bounded calls. Reuse the shared contract;
   do not create a watcher-owned daemon or general workflow engine.
4. **Remote agent inbox and fleet reducer:** durable inbox acceptance schedules
   agent work elsewhere; a pure fleet reducer merges per-node status/results.
   Keep any guarded service action behind NIC-116's independent dry-run and act
   authorization. A threshold event cannot grant that action.

For six Pi 3 Mothers plus nicamaster, the authorized coordinator snapshots the
seven-node target list and submits one logical fleet request, retaining separate
per-node request identities. Each operation is single-hop to an independently
authorized Mother. Configuration fan-out is not a distributed transaction: a
merged answer names the revision installed on each node and any refusal; do not
roll back good nodes or hide failed nodes. Status fan-out returns each node's
rule state, source freshness, queue health and signed receipt. Use bounded
concurrency (initially two requests) and a declared deadline. An absent node is
`unreachable`/`unknown`, never a normal metric or zero events. All seven entries
remain present in the merged answer, with a completeness flag and retryable
node set. No coordinator receipt substitutes for a missing target receipt.

Later handover updates that fleet request's versioned answer without erasing
its earlier partial-failure evidence. A globally unique event includes its
origin node, so equal threshold values on different nodes are not duplicates.
Do not average away a critical crossing. Fleet queries are operator/on-demand
calls; subscriptions still push crossings without periodic agent fan-out.

For an ESP32/Pi Pico leaf, NIC-115's sensor/serial Toy remains on the Pi Mother.
The leaf runs firmware or tiny core Wasm under WAMR/wasm3, not this component
world and not a Mother. A source reference names gateway + logical leaf +
channel; sequence/incarnation and quality expose leaf reboot or link gaps. The
Pi holds grants, verifies/adapts observations, signs receipts and queues them.
The same threshold rules work whether a sample originated locally or at a leaf.
The Pi receipt authenticates gateway observation, not a fictitious MCU Mother.

## 9. Pi 3 acceptance plan — future work, not run

Doug must approve build order first; subsequent implementation uses Sol
high/xhigh. The design task runs no Rust tests, builds, deploys or fleet commands.
The following are implementation pass/fail gates, not evidence of readiness.
Use deterministic clock, ordering, append/ACK, signing and delivery fault seams,
then physical Pi tests; a simulator alone cannot establish Pi resource budgets.

| Gate | Test and pass condition |
| --- | --- |
| Typed contract | Invoke every export through Mother with allow/deny cases. Typed health/sensor/log observations and structured results only; malformed, oversized, wrong-schema/unit/source input is refused without a threshold effect. No shell scraping. |
| Crossing and chatter | Feed §4's eight values: exactly three transition events and two default wakes. Exercise low rules, equality, hysteresis chatter, confirmation N=3, duplicate samples and initial-high baseline. Constant high for 10,000 observations never repeats its wake. |
| Gaps and provenance | Drop/reorder an input, restart producer incarnation, send invalid/late values, jump wall clock and replay a conflicting digest. State never rolls backward; no fabricated crossing/recovery; explicit gap/quality status and baseline behavior match §4. Include a leaf reboot behind a gateway. |
| Real composition | After approval, connect at least one actual health/sensor/log producer using its Toy plus controlled replay fixtures for deterministic crossings. Agent waits on a non-Pi host; no agent polling occurs. Producer receipt/source identity survives the whole path. |
| Receipts | Verify every configure/status/pause/advance and delivery-attempt terminal receipt, including no-crossing, denied and failed calls. Tamper with output, input digest, node, operation or ledger anchor: verification fails. Test key rotation and explicit unavailable target receipts per NIC-110. |
| Power loss and crash cuts | Inject every boundary in §6, including ledger commit with lost ACK and projection failure; physically interrupt power during a step and during delivery. Ledgers still verify or enter the prescribed evidence-preserving quarantine. No consumed input loses its committed crossing; no baseline reset silently reports healthy. |
| Reboot continuity | Assert once, reboot, feed an ordered same-stream asserted sample: no new wake. Replay the same input after the ordinary dedupe TTL: still no new crossing. Rearm and cross again: one new event. Recovery uses fresh invocation authority, not resurrection of the old call. |
| Offline / lost ACK | Disconnect receiver, generate assert/recovery/reassert, restart Pi, then reconnect. All events and signed results hand over in order with original IDs/bytes. Drop receiver ACK after durable acceptance: retransmission causes no second logical wake. Ambiguous acceptance stays unknown. |
| Authority | Revoke/expire source, store or export authority between read/commit and between commit/delivery. New protected work is denied at its own boundary; old queues are not authority. Wrong Vision, caller, assignment, artifact and route all fail closed. Recovery after revocation does not restore permission. |
| Bounds / exhaustion | Exercise all §7 caps, fill each spool and exhaust disk/receipt reserve. Backpressure and typed failure, never overwrite or unbounded memory. Verify no post-failure source ACK or uncommitted delivery. Record CPU, RSS, startup, durable-step latency and disk growth for the 24-hour rate test. All proposed budget gates must pass or be explicitly revised before approval. |
| Precompiled-only | On actual Debian aarch64 Pi 3, use only nicamaster-produced compatible `.cwasm`. Reject tampered, wrong-engine and unsupported CPU-feature artifacts before load. Instrument/prove zero compilation paths on the Pi, rather than merely observing a fast startup. |
| Seven-node partial failure | Fan out to six Pi 3s + nicamaster with one offline, one revoked and one mid-call drop. One answer retains seven attributed entries, verified available receipts and truthful missing/unknown outcomes. Reconnect and hand over without reissuing completed logical work or inventing target receipts. |
| Upgrade / pause | Pause preserves queues; already admitted work is described honestly. Change a rule revision and reject an incompatible state schema/artifact. Neither edit nor restart duplicates old events or implicitly grants new code access. |

## 10. Review gates and design validation

Before build, reconcile changes to the inspected NIC-110 contract/WIT, receipt
signer/trust and retention rules; agree producer stream/gap schemas with
NIC-111/112/115;
ratify the new state Toy commit boundary and durable inbox/event adapters; and
approve the numerical budgets and baseline-only startup policy. These are
explicit dependencies, not permission to widen existing Watch grants.

Recommended order within NIC-114: shared contract/precompiled loader and one
structured producer first; pure reducer proofs next; durable state + outbox and
receiver dedupe with crash cuts next; physical Pi budget/offline tests last.
Do not parallelize away the atomic commit proof or bundle a service actuator.

Design completion check: WIT world, inputs/outputs, Toys/new capability flags,
memory/startup/work budget, stateful-event profile, composition and Pi 3 tests
are all present. This document changes no settled decision or implementation;
proposed capabilities remain denied until separately approved and built.
Validation for this change is document/source review, local-link inspection and
`git diff --check` only. No runtime or physical acceptance result is claimed.

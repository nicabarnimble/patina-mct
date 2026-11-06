# NIC-116 guarded service action Child

## Status and scope

Proposed design for [NIC-116](https://linear.app/nicabar/issue/NIC-116), under
[NIC-109](https://linear.app/nicabar/issue/NIC-109). This document is not an
implementation, an approved build order, or physical acceptance evidence.
Doug approves the design and build order before implementation on Sol high or
xhigh. No build, deployment, or fleet operation is part of this change.

An agent may ask a Raspberry Pi 3 Mother to restart one operator-enrolled,
noncritical systemd service. The Mother first returns a signed dry-run plan.
A separate, authorized act call may execute that exact plan once. Agents use
Pis as hands on local hardware, not as inference or build machines.

The first target is aarch64 Debian on a Pi 3 with about 905 MB RAM. The same
contract supports seven independent targets, six Pi 3s plus nicamaster.
Instance configuration supplies node IDs and service names. No fleet identity
or production service name is baked into the component.

Excluded capabilities include shell commands, argv, scripts, environment
variables, arbitrary D-Bus calls, unit installation or editing, daemon reload,
log rotation, package management, inference, Git, and builds on a Pi 3. Git
Children remain macOS-only on Studio or Lume. V1 does not restart Mother,
networking, storage, authentication, or an MCU. It does not enable write-capable
cross-Vision calls or change NIC-104's recommended Toy build order.

## Evidence and compatibility

The inspected implementation baseline is `bc48d1e0d721d1531a58192a69ef65ffd234882f`.
The following distinctions keep the proposal separate from shipped behavior.

| Source | What the design uses or must add |
| --- | --- |
| [MCT ontology](../../../layer/core/what-is-mct.md) and [safety boundaries](../../../layer/core/safety-boundaries.md) | Mother decides, Children compute, Toys effect. Manifest needs grant no authority. Required observation failure denies the effect. |
| [First application plan](../../plans/first-real-app.md) and [repo-report WIT](../../../crates/mct-repo-report/wit/world.wit) | A small bounded operation is the starting point. The existing report uses strings and a read-only directory Toy, not a service-control capability. |
| [NIC-104 decisions](https://linear.app/nicabar/issue/NIC-104) | Bounded app operations, separate artifact/approval/assignment/instance facts, read-compute by default, opt-in stateful-event, and fail-closed new admission rather than exactly-once effects. |
| Canonical consultation, `docs/mct-vision-child-plan.md`, and updated `docs/src/child-development/index.md` | Read from the adjacent `patina-mct` checkout. The plan was committed in `2bef593`, after this branch's baseline. It is not copied here or falsely linked as a file present on this branch. Keep fixtures as fixtures and avoid fake Watch imports for storage. |
| [Wasm runtime](../../../crates/mct-daemon/src/wasm.rs), `DEFAULT_WASM_MEMORY_LIMIT_BYTES`, `store_limits`, `configure_deadline_at`, `compile_snapshot` | The default memory limit is 64 MiB. Epoch interruption supplies guest deadlines. `Component::from_binary` compiles component bytes. A trusted precompiled-only path is new work, not an existing Pi guarantee. The memory limit is not a total Mother RSS limit. |
| [Invocation budget](../../../crates/mct-daemon/src/wasm/budget.rs) | Existing counters bound host calls, Toy calls, observations, host values, output, and cumulative bytes. Budget exhaustion is sticky. New adapters must use these counters before effects. |
| [ToyGrant and effect admission](../../../crates/mct-kernel/src/toy.rs) | Exact subject and resource matching, time bounds, and fresh snapshot checks exist. An absent optional scope field is a wildcard. `max_uses: Some(...)` fails with `ConsumptionStateUnavailable` at effect admission. |
| [Toy adapter registry](../../../crates/mct-daemon/src/toy.rs) and [resident execution](../../../crates/mct-daemon/src/daemon/resident/execution.rs) | The registry has no service-action backend. The ordinary directory adapter's import allowlist does not admit the proposed world. New catalog entries, typed host imports, and resident wiring are required. |
| [D-32 through D-37](../../DECISIONS.md#5-restart-recovery-and-the-peer-wire) and [durable task proposal](../durable-task-boundary.md) | Recovery settles interrupted calls, never resumes them. Single-hop federation is the baseline. Durable task/effect identity beyond transport deduplication remains explicit new work. |
| [NIC-110 shared contract](https://linear.app/nicabar/issue/NIC-110) | Owns the result envelope, signed receipt, fan-out result, budget declaration, offline handover, and trusted `.cwasm` pipeline. Its parallel draft appeared during review and was read from `nic-110/docs/design/small-device/wit/shared.wit`. This design adopts `patina:small-device@0.1.0-draft`. The inspected WIT SHA-256 is `14ff8bf1a9543edfaea6841da50ddd183601f53691391b505f5efae990ae7c94`. The accompanying `nic-110-shared-contract.md` was also read when it appeared during review. These are draft dependencies, not files added by NIC-116. |

## Profile and ownership

The overall profile is **stateful-event, opt-in**, with bounded calls rather
than a resident loop. `plan` is read-compute. `act` is an explicitly mutating
operation. `status` reads durable action evidence. The shared profile vocabulary
must not label `act` as read-compute or imply it is safe to replay.

The Child validates domain inputs and formats structured plans and results.
The new service-action Toy reads systemd state and performs the single allowed
mutation. Mother owns approval, fresh ToyGrant evaluation, durable plan and
effect records, receipts, and result delivery. No signing key, database handle,
D-Bus connection, process handle, or filesystem preopen reaches the Child.
The component has no durable in-memory state, polling loop, lifecycle exports,
or authority to approve its own plan.

## WIT world

Proposed package `patina:guarded-service-action@0.1.0`, world `child`.
Callable operations are `control#plan`, `control#act`, and `control#status`.
The following is a contract sketch inside documentation, not a new compiled
WIT package. Shared types use the inspected NIC-110 draft. Final package versions
and schema projections must reconcile with NIC-110 before build.

```wit
package patina:guarded-service-action@0.1.0;

interface types {
    use patina:small-device/common@0.1.0-draft.{call-context, digest, timestamp-ms, coverage};
    use patina:small-device/receipts@0.1.0-draft.{signed-receipt};

    enum service-state { active, inactive, failed, activating, deactivating, unknown }
    enum action-state { planned, not-dispatched, dispatch-possible, completed, failed, outcome-unknown }
    enum error-code { invalid-request, denied, stale-plan, expired-plan, busy,
        conflict, unsupported, budget-exceeded, evidence-unavailable, outcome-unknown }

    record fault {
        code: error-code,
        safe-message: string,
    }
    record service-snapshot {
        resource-id: string,
        boot-id: string,
        definition-digest: digest,
        invocation-id: string,
        state: service-state,
        pending-job: bool,
    }
    record plan-request {
        context: call-context,
        action-id: string,
        resource-id: string,
    }
    record service-plan {
        coverage: coverage,
        plan-id: string,
        action-id: string,
        plan-digest: digest,
        before: service-snapshot,
        expires-at: timestamp-ms,
    }
    record act-request {
        context: call-context,
        plan-id: string,
        plan-digest: digest,
        plan-receipt: signed-receipt,
        approval-id: string,
    }
    record status-request {
        context: call-context,
        action-id: string,
    }
    record action-result {
        coverage: coverage,
        action-id: string,
        plan-digest: digest,
        state: action-state,
        after: option<service-snapshot>,
        effect-receipt-digest: option<digest>,
    }
}

interface control {
    use types.{plan-request, service-plan, act-request, action-result, status-request, fault};
    plan: func(request: plan-request) -> result<service-plan, fault>;
    act: func(request: act-request) -> result<action-result, fault>;
    status: func(request: status-request) -> result<action-result, fault>;
}

world child {
    import patina:service-action/guard@0.1.0;
    export control;
}
```

The **new** `patina:service-action/guard@0.1.0` Toy interface has three typed
functions. `prepare(plan-request)` returns a host-registered `service-plan` or `fault`.
`restart(act-request)` returns an `action-result` or `fault`.
`lookup(status-request)` returns an `action-result` or `fault`.
These records move into a versioned common domain-types package at build time
so Toy and Child do not create circular package dependencies. Only the
`restart` import can mutate the service. The host rejects any alternate method
or unit supplied through unrecognized fields.

NIC-110's Mother-owned result envelope wraps each Child reply. The guest does
not return a signature over its own assertion. Mother binds the registered
plan or effect record to the returned payload before signing. A guest that
substitutes a different plan digest or claims completion without matching host
evidence gets a failed result, not a signed success.

The domain schema IDs are `patina:guarded-service-action/plan@0.1.0` and
`patina:guarded-service-action/action-result@0.1.0`. NIC-110's `child-answer`
projection moves `coverage` into its coverage field and encodes the remaining
record as a `typed-payload` using RFC 8785 `canonical-json`. Input records use
the same field/value projection for their domain data under NIC-110's request
digest rules. Field names remain
WIT kebab-case names, options become null or their value, and enums become their
case names. Unsigned 64-bit integers use decimal strings in the JSON projection.
Digests use NIC-110's BLAKE3 convention. Accepted single-resource replies use
complete coverage with one examined item, zero omitted items, and no
continuation. Effect uncertainty is separate from coverage. The
`effect-receipt-digest` references an earlier retained act receipt, if one exists.
It is absent on the first act reply to avoid a self-referential receipt hash.
A `fault` is not a successful typed payload. Mother maps it to the shared closed
outcome/reason vocabulary, retains detail in the ledger, and returns no answer
on denied, failed, timed-out, or cancelled envelopes, as NIC-110 requires.

## Inputs and outputs

| Field or record | Required meaning and bounds |
| --- | --- |
| Shared `call-context` | NIC-110's job/task/call IDs, target node, operation, artifact and request digests, scope, profile, effective budget, and deadline. Mother derives caller identity and data classification from admission. The host plan additionally pins assignment identity, which the current shared context does not contain. |
| `action-id` | Caller-generated opaque logical identity, at most 128 ASCII bytes. It is the logical effect ID within NIC-110's `task-id`. Its key also includes authenticated caller, Vision, executing Mother, and assignment. It survives reconnect and transport call-ID changes. Reuse with changed input is `conflict`. |
| `resource-id` | At most 256 ASCII bytes. Opaque ID of one enrolled service resource. Mother resolves it to an exact canonical unit name and manager. No glob, path, template expansion, alias selection, or list of units. |
| `plan` | Host-generated ID and canonical digest over the operation `restart`, action key, target, caller, artifact digest, assignment, resource, approved definition/dependency digest, before-state, boot ID, policy/grant revisions, creation time, expiry, and budget version. Returned compact fields refer to the complete host record. |
| Plan validity | At most 120 seconds, no later than the plan grant's expiry. This validity is separate from the 2-second planning invocation deadline. Act has its own deadline, clamped to the plan, approval, and restart grant. Mother uses its clock and a same-boot monotonic age bound. Clock uncertainty denies execution. A reboot invalidates an unexecuted plan. |
| `act-request` | Names the existing plan, its signed plan receipt, and a separately issued approval. It cannot override the operation, target, resource, budget, or preconditions. IDs and receipt references are locators, not bearer authority. |
| `approval-id` | Reference to a new Mother-owned, revocable approval record binding the exact plan digest, caller, target, artifact, one act, and expiry. An authenticated local owner approval is required in v1. A model-generated `approved: true` is never accepted. |
| `service-snapshot` | Bounded typed systemd properties. The host records observation time in the envelope. Unknown or missing properties remain unknown, never a fabricated healthy state. No journal text or shell output. |
| `action-result` | Distinguishes dispatch evidence, completed restart, known failure, and uncertainty. Missing post-state does not prove no effect. `completed` requires a matching successful systemd job, active post-state, and a new service invocation ID. |
| `fault` | Closed code plus caller-safe message of at most 256 UTF-8 bytes. Detailed policy decisions stay in the local ledger. Retry is never implied by a generic failure. |
| Wire size | Whole input at most 4 KiB. Domain output at most 8 KiB. Signed per-call envelope at most 16 KiB. Bounds include encoding overhead. IDs, lists, and strings are checked before allocation or host effects. |

`plan` success means a plan was recorded, not that a restart occurred or is
already authorized. `act` success means `completed` evidence was durably
recorded. Known act failure maps to the existing failed result. Interruption
maps to failed or timed-out with explicit effect uncertainty, not a new
transport success. A successful `status` read may report `outcome-unknown`.
It is success at reading evidence, not success at restarting a service.

Mother maps these results to NIC-110's `result-envelope`. Plans and status
reads have `effect: no-effect` with respect to service mutation. Rejected acts
before admission have `not-started`, proven completed acts have `confirmed`,
and uncertain dispatches have `unknown` with `effect-unknown` or
`execution-interrupted` as appropriate. A known failed job may still have
changed service state, so failure alone never implies `not-started`. A status
reply describes historical uncertainty in its payload, not as a new effect.

The action's semantic digest covers its scoped identity, resource, operation,
and immutable plan. Before a plan exists, its initial binding uses the scoped
resource and operation. It excludes transport call ID, attempt number, and a
shortened retry deadline. NIC-110's separate request digest follows its exact
canonical field set, including the deadline and excluding transport retry
counters. Same-call-ID retransmission preserves that digest. A changed deadline
requires a new call ID, never a new logical effect. Reusing an action under a
new transport call cannot create a second effect or extend plan validity.

## Toys and exact ToyGrant

The service-action Toy is new. It requires three authority-bearing canonical
catalog entries, narrowed to `guard#prepare`, `guard#restart`, and
`guard#lookup`, with separate Toy IDs and matching WIT versions. The proposed
actions are `plan`, `restart`, and `status`. No existing Git or directory Toy
can substitute for these entries.

| Grant field | V1 requirement |
| --- | --- |
| Subject | Exact Child name, artifact ID and version, assignment ID, and authenticated caller node ID. No absent assignment or caller wildcard. |
| Scope | Exact Vision, executing node, configured project, trusted operational data classification, and enrolled service resource ID. Only the matching action is allowed. |
| Constraints | Explicit start and exclusive expiry, `locality_required: true`, and a duration bounded by this design. Restart authority expires within 120 seconds of issuance. `max_duration_ms` bounds admission-token lifetime, not systemd cancellation. |
| Issuer and evidence | Authorized local issuer, current policy and grant revisions, and durable authority observation. Receipt signatures alone do not issue a grant. |
| One-shot protection | `max_uses: None` under the current evaluator. A mandatory new Mother-owned plan-consumption record enforces one dispatch per approved plan. This design must not pretend `max_uses: 1` works today. |

Planning requires only the plan grant. An owner reviews the signed plan and
separately authorizes its approval record and exact restart grant. `status`
requires its own read grant. Revoking restart authority does not automatically
revoke access to prior receipts, but result disclosure always needs current
read authority. A plan may exist when act authority is absent. The new catalog
admission policy and adapter reject wildcard grant shapes even though the
generic evaluator permits them. `lookup` derives its resource from the scoped
stored action before evaluating the status grant, not from a caller's guess.

Planning revisions are audit evidence, not a requirement that no authority
record changes before act. Issuing the approval and restart grant necessarily
changes authority state. Act checks those exact current records, together with
current approval/assignment and resource policy, rather than accepting the old
planning snapshot or rejecting every legitimate grant issuance.

At the effect boundary, Mother obtains a fresh authority snapshot and mints
an `AuthorizedToyCall`. The adapter uses `admit_effect_with_snapshot` and the
Mother authority-order mechanism, not the legacy time-only check. It verifies
the exact plan, approval, scope, deadline, artifact, unit definition, and
same-boot preconditions before dispatch. Deny if the ledger, authority proof,
signer, result reservation, or exclusive resource admission is unavailable.

Revocation linearizes with effect admission. A revocation ordered first blocks
dispatch. Once the systemd request is handed off, revocation cannot undo it.
No claim of instantaneous fleet-wide revocation during a partition is made.
V1 act admission requires a live authenticated caller and fresh local owner
authority. It does not execute an old remotely queued approval while offline.

## Host adapter and service boundary

Mother enrolls a fixed systemd system-manager resource. Enrollment pins the
canonical `.service` name, effective unit configuration including drop-ins,
and reviewed dependencies and restart side effects. The unit must be a simple,
noncritical service with bounded stop/start behavior. Reject templates,
transient units, aliases, masked or missing units, and services whose dependency
or lifecycle hooks could affect protected resources. All IDs for the same
physical manager/unit share one host exclusion key across assignments and
Visions. Alternate resource registration cannot bypass it. The acceptance fixture has
no hardware effects and no dependency propagation.

The host uses typed systemd D-Bus properties and `RestartUnit` with job mode
`fail`. It does not invoke `systemctl`, parse terminal output, or expose the
system bus to the guest. The adapter subscribes to job completion before
submission, bounds all property reads and waits, and correlates the returned
job identity with the exact unit. An OS permission failure is a typed denial
or failure, never a reason to fall back to privileged shell execution.

Dry-run reads state and registers a plan. It does not enqueue a systemd job,
start an inactive unit, cancel another job, or run lifecycle hooks. Act requires
an active unit, no pending job, unchanged invocation ID, unchanged approved
definition, the same boot ID, and an unconsumed plan.

Systemd does not provide an atomic compare-and-restart against this snapshot.
The per-resource Mother lock serializes MCT calls, not unrelated root activity.
V1 therefore admits only resources whose definitions and lifecycle management
are operator-controlled during the plan/act window. It rechecks immediately
before dispatch and rejects observed changes. It does not promise atomic
snapshot isolation against an independent administrator or spontaneous service
failure. If that stronger guarantee is required, this backend is unsupported
until a cooperating service protocol supplies it. The receipt records the
observed pre-state and actual result rather than concealing this race.

## Plan, act, and crash behavior

The new durable record is keyed by action identity and plan digest. Each
accepted plan has at most one approval consumption and one possible dispatch.
A per-resource in-flight exclusion also blocks a different action ID from
restarting the same unit while an earlier dispatch remains unresolved.

| Stage | Durable evidence and next permitted step |
| --- | --- |
| Plan | Mother records the canonical plan and produces a signed plan receipt. No service mutation. A duplicate plan request returns the same plan, even if now expired. Fresh planning uses a new action ID. |
| Approval | Owner authorization records the exact plan approval and expiry. The Child cannot create this record. |
| Pre-dispatch | Under the authority order and resource exclusion, revalidate, reserve bounded result/receipt storage, and durably consume the plan with a `dispatch-possible` fact before touching D-Bus. The durable ledger fact is authoritative. A SQLite projection cannot be the only record preventing repeats. |
| Dispatch | The adapter submits at most one restart request. It records the job identity when available. A lost D-Bus reply does not license resubmission. Adapter and D-Bus client retries of this method are disabled. |
| Terminal | Record the structured result and result digest, then sign its receipt. Reply only after durable evidence is available. Client disconnection does not erase the record. |
| Recovery | Rebuild consumption and resource exclusion from verified evidence. Settle unfinished dispatches as `outcome-unknown`. Do not resume a Child or repeat the restart. |

The interval between durable intent and D-Bus submission is deliberately
conservative. A crash there can leave a plan consumed even when no restart
occurred. This is the cost of avoiding a second restart without destination
idempotency. Neither a service's active state nor a changed invocation ID alone
proves that this particular request caused its restart.

| Failure point | Required outcome |
| --- | --- |
| Before durable plan | No executable plan. Return a signed failure if evidence storage works. Otherwise report evidence unavailable with no effect. |
| After plan, before act admission | No restart. Expiry, reboot, grant revocation, or definition change requires a new plan and owner approval. |
| Before durable dispatch admission | No D-Bus mutation. A later explicit act attempt still needs fresh authority and an unexpired plan. No automatic retry. |
| After dispatch admission, before terminal evidence | Outcome unknown, even if dispatch might not have happened. Keep the consumption record and resource exclusion. |
| After terminal evidence, before reply | Replay the recorded result under current read authority. Do not repeat the effect. |
| Deadline, cancellation, or transport drop after handoff | Stop waiting within the call budget and report uncertainty unless completion evidence is available. The OS job may continue. No automatic cancel, rollback, or restart. |
| Torn ledger tail, full disk, or broken projection | Apply existing ledger verification/recovery rules. No mutation until evidence and consumed-plan state are trustworthy. Never restore an older projection and treat a consumed action as new. |

An explicit owner reconciliation can inspect retained job evidence and service
state, append what is known, and release the resource exclusion. If attribution
is impossible, the historical outcome stays unknown. A new action requires a
new plan and approval that acknowledges the unresolved prior result. A 30-second
execution deadline is not permission to garbage-collect the action identity.

## Signed receipts and offline handover

NIC-110's inspected draft uses 64-character lowercase BLAKE3-256 digests,
Unix-millisecond timestamps, `ed25519-v1` signed receipts, and a terminal
`ledger-position`. Adopt its deterministic-CBOR signing preimage, purpose-bound
Mother receipt key, domain separation, enrollment/rotation policy, and ledger
verification rules without a second scheme. Receipt verification distinguishes
signature verification from independently checked ledger inclusion. An
`audit_ref` or a hash chain alone is not the requested signed receipt.
Child-held signing keys and self-signed guest output are excluded.

Each admitted call, including plan, act, status, denial, and failure, receives a
Mother-signed receipt over its request digest, output digest, terminal outcome,
call/action identity, executor, artifact, assignment, Vision, and ledger anchor.
Act receipts additionally bind the plan receipt/digest, approval reference,
authority revision evidence, and whether dispatch was possible. Every host Toy
call also leaves correlated authority and effect facts. These remain linked to
the enclosing signed receipt, including when the guest traps after a Toy effect.

A signed denial must not expose private unit names, grant inventories, or
internal reasons to an unauthorized caller. It may bind a digest of the denied
request and an opaque audit reference. If durable evidence or signing fails,
Mother cannot truthfully return a signed terminal receipt. Before dispatch that
failure blocks the effect. After dispatch it leaves an unresolved record for
later recovery and receipt delivery. A transport failure is never manufactured
into a receipt from an unreachable Mother.

Offline queueing separates work intent from permission to act:

- A client may queue plan requests while a Mother is unreachable. The target
  evaluates each request and its deadline only when delivered. An offline queue
  entry is not an admitted call.
- Completed plans and action results enter a bounded Mother-owned outbox.
  Reconnect transfers the original signed evidence under current audience and
  read policy. The receiver acknowledges the exact receipt/result digest.
- V1 never queues an executable restart for automatic future delivery. An act
  not handed off needs a fresh connected submission. An uncertain act is
  queried by action ID, not resubmitted as new work.
- A network drop after local admission does not reverse an OS operation.
  Mother records the result locally and hands it over later.

Proposed limits are 256 undelivered result records or 8 MiB per assignment,
whichever comes first. Act admission reserves space for terminal evidence.
These stricter assignment limits also count against NIC-110's per-Mother cap
of 256 records and 16 MiB, plus its separate 1 MiB settlement reserve. Full
queues deny new actions rather than evicting unacknowledged receipts.
Retain result bytes until acknowledged, then for at least seven more days.
Keep compact consumption records and unresolved exclusions for the assignment lifetime,
reconstructible from the ledger. Storage exhaustion requires operator action,
not forgetting replay protection. A missing result body returns
`evidence-unavailable`, never a recomputed restart.

## Fleet fan-out and one merged answer

The coordinator targets an explicit, immutable roster of six Pi 3 Mothers plus
nicamaster. Each node gets its own action ID, plan, approval, exact local grant,
and receipt. This is seven independent single-hop calls, not a shared grant,
shared ledger, remote shell, or distributed transaction.

Plan requests may run with at most two in flight. The first version executes
approved act calls serially, at most one in flight across the roster. Serial
execution limits simultaneous service disruption without changing local
sovereignty. A failed or unknown act stops further acts by default. Remaining
nodes appear as `not-attempted`, not silently omitted or called successful.
Seven worst-case 30-second acts can outlive the 120-second plan validity.
The coordinator does not extend plans to finish the fleet. Expired plans need
an explicit new plan/approval round with new action IDs. This tradeoff limits
stale authority rather than promising fleet-wide atomic success.

Use NIC-110's `fanout-request`, `node-result`, and `receipted-fleet-result`.
The signed envelope digest binds the explicit roster. Preserve its per-node
rows, counts, and `complete`, `partial`, or `failed` status without redefining
them. Each `node-answer` is either a received signed result or a `no-reply`
record. Denial and effect uncertainty belong in the received envelope.
Unreachable nodes use `missing(unreachable)`. The domain's `not-attempted`
means `missing(not-dispatched)` with `may-have-executed: false`. A reply lost
after submission has `may-have-executed: true`, not a fabricated failed act.
Overall restart success requires all seven target act receipts to verify and
report completion.
A successful plan merge is never presented as seven successful restarts.

Unreachable targets have no target receipt. The coordinator may sign its own
observation of non-delivery, clearly attributed to itself. The merged answer
retains each executor's signed evidence. Late results produce a new aggregate
revision linked to the earlier answer, never a silent rewrite. Duplicate,
wrong-node, wrong-plan, and invalid-signature results are rejected. A pure
reducer may compose verified results but cannot authorize missing acts.

## Memory and startup budget

All numbers below are **proposed acceptance ceilings**, not measurements.
They must be declared through NIC-110 and enforced by Mother. A cap violation
fails closed, with effect uncertainty preserved if a dispatch already occurred.

| Resource | NIC-116 declaration |
| --- | --- |
| Guest linear memory | 8 MiB maximum, one memory. Reject extra memories or structures that evade the aggregate budget. This is one eighth of the current 64 MiB default. |
| Incremental host memory | Peak RSS increase at most 24 MiB over an idle enrolled Mother during one call, including component mappings and adapter buffers. No swap or OOM during acceptance. Guest memory and Mother RSS are measured separately. |
| Artifact | Precompiled `.cwasm` at most 4 MiB. No JIT/AOT fallback on Pi 3. |
| Cold startup | At most 500 ms p95 for verified local artifact load plus instantiation to export entry on Pi 3. No compilation or network acquisition in this interval. |
| Warm startup | At most 100 ms p95 from invocation admission to export entry with the artifact already verified/cached. No resident guest required. |
| Guest compute | At most 100 ms CPU per invocation, excluding host waits. Enforcement or a tighter available CPU limit is a NIC-110/runtime build requirement. |
| Plan and status deadline | At most 2 seconds including bounded property reads and local evidence commit. |
| Act deadline | At most 30 seconds, clamped by the call, plan, approval, and grant deadlines. Systemd stop/start configuration must fit this envelope. Host D-Bus waits need their own deadline. Guest epoch interruption alone does not cancel them. |
| Invocation counters | Host calls 32, Toy calls 4, observations 32, cumulative charged bytes 256 KiB, host value bytes 16 KiB, guest output 8 KiB, logging calls 0, measure calls 0. Reserve failure and recovery evidence separately in Mother. |
| Host service work | At most 8 bounded D-Bus interactions and 64 KiB cumulative decoded properties/events per invocation. No unbounded polling or event retention. Overflow yields failure or uncertainty, not truncation into success. |
| Concurrency | One active NIC-116 invocation per Mother. One unresolved act per resource. Fleet limits are two plans or one act in flight. |
| Fleet output | At most 128 KiB for seven compact per-node envelopes and aggregate metadata. Reject oversize results rather than dropping failed nodes. |
| Persistent queue | 256 records or 8 MiB per assignment, as defined above. Ledger growth and consumption-index storage are separate admission budgets, not hidden guest memory. |

Map each operation to NIC-110's `operation-declaration` and `budget`.
Use `linear-memory-bytes: 8388608`, `startup-ms: 500`, `input-bytes: 4096`,
`output-bytes: 8192`, `host-value-bytes: 16384`, `accumulated-bytes: 262144`,
and `max-items: 1`, with the counters above. `wall-ms` is 2000 for plan/status
and 30000 for act. Plan/status declare read-compute, act declares stateful-event.
Startup is included in `wall-ms`, as NIC-110 requires, not added after it.
V1 sets `minimum-budget` equal to `requested-budget` for this declared vector.
A tighter effective budget refuses admission until a smaller vector is qualified.
NIC-110 defines `output-bytes` as application output and reserves terminal
packages separately. NIC-116 lowers its receipted-result limit to 16 KiB.
RSS, CPU, host-wait, artifact, and storage limits need host policy fields or a
versioned budget extension because they are absent from the inspected WIT.
Apply NIC-110's aggregate admission policy, including 256 MiB reserved for the
OS, Mother, and control work. This design does not raise node concurrency.

NIC-110 builds and precompiles on nicamaster for the Pi 3's aarch64 CPU baseline.
The release record binds the source Wasm digest, `.cwasm` digest, WIT and budget
versions, Wasmtime version/configuration, target triple, and CPU features.
Mother verifies trusted build provenance and compatibility before deserializing
native executable artifacts. Unknown, corrupted, untrusted, or incompatible
artifacts are rejected. Renaming a `.wasm` file does not satisfy this contract.
Building on nicamaster must not select CPU features missing on a Pi 3.

## Composition candidates

| Component | Profile and permitted composition |
| --- | --- |
| Service-state collector | Read-compute. Share a typed bounded snapshot with NIC-111 health probing if its schema fits. Keep observe and restart authority separate even if one host backend implements both. |
| Restart preflight | Pure compute over snapshots and declared policy for explanation and tests. NIC-113 config preflight may supply advisory evidence. Mother still checks current authoritative state before dispatch. |
| Guarded executor | Stateful-event, opt-in. One service-action Toy and durable consumption evidence. Keep the plan-to-act safety check inside the host boundary rather than trusting a chain of Child approvals. |
| Fleet reducer | Pure compute over verified per-node envelopes. No service-action grant and no authority to retry. Signature verification and result-release policy remain Mother responsibilities. |
| Threshold watcher | NIC-114 may request a plan when a threshold crosses. A crossing cannot auto-approve or execute a restart in v1. |

Use plain typed data between components. No general graph scheduler is needed.
A future sensor or serial Toy can represent a remote MCU through a Pi gateway,
as NIC-115 proposes. The Pi Mother would still hold grants and sign receipts.
This service-action world makes no assumption about sensor locality because it
has no sensor input. MCU reset or firmware update is a different capability,
not a reinterpretation of `resource-id`. A Mother on an MCU remains out of scope.

## Pi 3 acceptance test

These are future pass/fail tests after approval, not commands run for this
change. The fixture is an operator-created, noncritical service with a unique
invocation marker, no hardware effects, and no dependency propagation.
Exercise the public Child call path and typed host adapter, not SSH workloads.
Retain structured results, signed receipts, ledger verification output, and
separate guest-memory/RSS/timing measurements for each run.

| Test | Pass condition |
| --- | --- |
| Dry-run only | Plan reports the exact active fixture and produces a verifiable receipt. Service invocation marker, process generation, and job count do not change. Plan succeeds with no restart grant. |
| Explicit act | Owner approves the exact plan, then a separately granted act causes one restart. A new invocation marker, matching completed job, active state, and linked plan/act receipts prove completion. |
| No authority shortcuts | Missing/wildcard scope, wrong caller/assignment/artifact/Vision/node/resource/action, tampered plan, missing approval, or unknown catalog entry causes zero systemd mutations. Cross-Vision mutation is denied. |
| Revocation race | Revoke approval, ToyGrant, or assignment after planning and before admission. Act is denied. Race revocation against handoff and verify the ledger order explains which won. Do not expect rollback after handoff. |
| Stale plan | Expiry, uncertain clock, reboot, changed definition/dependency digest, changed invocation ID, inactive unit, or pending job rejects act. No implicit replanning or activation of an inactive service. |
| Duplicate and concurrency | Repeat the same act with the same and different transport call IDs, including beyond the current dedupe window. At most one dispatch occurs. Changed input under the same action ID conflicts. Competing action IDs cannot bypass the resource exclusion. |
| Crash matrix | Kill Mother before/after plan commit, approval, dispatch fact, D-Bus send, job response, result commit, and reply. Repeat with host power interruption on the fixture. Ledger verification passes after permitted recovery. Uncertain windows never auto-repeat. |
| OS race boundary | Change service state or configuration between plan and act and verify observed changes deny. Inject a change after the final read. The result reports actual evidence or uncertainty without claiming atomic snapshot isolation. Unsupported resource-management assumptions fail enrollment. |
| Receipt integrity | Alter result bytes, target, plan digest, ledger anchor, or signature. Verification rejects each alteration. A guest that invents completion cannot obtain a valid success receipt. |
| Evidence failure | Inject full disk, signer failure, outbox full, budget exhaustion, and unavailable authority proof before dispatch. No mutation occurs. Repeat after dispatch and verify unresolved evidence is retained without a fabricated success or retry. |
| Offline handover | Queue a plan while disconnected, reconnect, and evaluate fresh authority. Drop the network after act admission and retrieve the original result later. Duplicate acknowledgements do not lose evidence. No offline queue executes a restart automatically. |
| Fleet partial failure | Plan across all seven targets. Include an unreachable node and a denied node. Verify one merged answer retains all targets and truthful receipt absence. During serial acts, inject an unknown result and verify remaining targets are `not-attempted`. A later result creates a new merge revision. |
| Hardware budgets | Run 30 cold invocations with artifact caches cleared by the approved test procedure and 100 warm invocations on an otherwise idle Pi 3. Report p95 startup, max guest allocation, peak RSS delta, CPU time, output bytes, and host counts. All declared limits pass without swap or OOM. Include real plan/act/status calls, not only an empty component. |
| Precompiled-only | Inspect Pi execution evidence for zero compilation. Reject mismatched Wasmtime/config/CPU artifacts and untrusted `.cwasm` bytes. Missing precompiled bytes must not invoke `Component::from_binary` as fallback. |
| Confinement | Malformed resource IDs, aliases, templates, arbitrary D-Bus methods, extra imports, shell-like strings, and requests for protected services cause no unintended effect. The component has no shell, network socket, filesystem, or signing-key access. |

## Shared-contract review and build gates

NIC-110 remains the single owner of the shared records. Reconcile these explicit
requirements before implementation rather than copying six envelope variants.

1. Ratify the draft WIT names, bounded context/digest/receipt types, canonical
   encoding, and the domain-payload projection above. Preserve transport outcome
   separately from service-effect certainty and from result delivery.
2. Approve and implement NIC-110's receipt trust, ledger anchoring, denial
   redaction, recovery receipts, key rotation verification, and honest absence
   of a receipt when storage or a target is unavailable.
3. Implement stable job/action identity, same-input checks, replay retention,
   result acknowledgement, and linked late-result merge revisions as drafted.
   Do not equate the short transport dedupe window with permanent protection.
4. Provide a durable plan-consumption and result-reservation boundary ordered
   with authority and ledger facts. Generic Child keyvalue or guest-owned flags
   cannot enforce one-shot restart. Standardize this mechanism if another Child
   needs it, without promising exactly-once external effects.
5. Define budget enforcement for aggregate guest memory, host buffers, CPU,
   blocking host waits, and storage. Deliver trusted precompiled-only loading
   and no compile fallback on a Pi 3.
6. Preserve NIC-110's per-operation profiles and distinguish safe offline
   result delivery from unsafe automatic mutation retries. Approve NIC-116's
   stricter rule that an unstarted act is never dispatched from an offline queue.

NIC-116 additionally needs owner approval of the enrolled service policy,
manual reconciliation semantics, and the 120-second plan/approval window.
The new Toy catalog, D-Bus adapter, resident imports, owner approval path,
consumption records, and receipts all require separately reviewed implementation.
NIC-104's owner-admin runtime cutover is not assumed complete at this baseline.
A standalone service-control privilege must not bypass that cutover.

The design deliverable passes when this document contains every required field
and its branch is pushed. Runtime acceptance remains unverified until the
approved implementation passes the tests above on a Pi 3.

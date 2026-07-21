---
type: feat
id: trigger-event-runtime
status: ratified
draft_date: 2026-07-21
ratified_date: 2026-07-21
target: replacement-slice-4a
operator_gate: D1-ratified
sessions:
  origin: 20260721-072059-435043000
  work:
    - 20260721-092955-065681000
related:
  - layer/surface/build/feat/watch-event-fixtures/SPEC.md
  - layer/allium/mct-product-map.allium
  - layer/allium/mct-patina-migration.allium
  - layer/sessions/20260721-072059-435043000.md
  - layer/surface/build/feat/artifact-acquisition/SPEC.md
  - layer/surface/build/feat/resident-call-ingress/SPEC.md
  - layer/surface/build/feat/supervisor-lifecycle/SPEC.md
  - layer/surface/build/spec-drift-audit/track3/LEDGER.md
  - crates/mct-kernel/src/call/mod.rs
  - crates/mct-daemon/src/state.rs
  - crates/mct-daemon/src/daemon/resident/pipeline.rs
  - crates/mct-daemon/src/daemon/resident/idempotency.rs
exit_criteria:
  - id: additive-trigger-origin
    text: CallOrigin gains only trigger_firing; existing wire spellings remain byte-for-byte stable, TriggerFiring is local for candidate sourcing, and a forwarded arrival is still Iroh.
    checked: false
    verify: cargo test -p mct-kernel trigger_firing_origin_is_additive_local_and_single_hop -- --nocapture
  - id: record-occurrence-idempotency
    text: Trigger retries scope replay by exact trigger-authority id and deterministic occurrence identity; distinct records, revisions, and occurrences cannot share replay state.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon trigger_firing_idempotency_is_record_and_occurrence_scoped -- --nocapture
  - id: durable-trigger-authority
    text: Create, revise, and revoke are owner-authenticated, ledger-first mutations with a current SQLite projection; absent, stale, expired, revoked, superseded, unobserved, or digest-mismatched records grant nothing.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon trigger_authority_is_scoped_observed_revisioned_and_revocable -- --nocapture
  - id: fixed-admission-order
    text: Every temporal occurrence passes missed-fire, overlap, per-record pending, resident-wide pending or active, and fresh current-call checks in exactly that order.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon trigger_admission_order_is_fixed_and_authority_neutral -- --nocapture
  - id: missed-fire-matrix
    text: skip, coalesce_one, and fire_late_bounded produce deterministic, countable, bounded evidence and never fabricate unknown event occurrences.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon trigger_missed_fire_policies_are_bounded_deterministic_and_countable -- --nocapture
  - id: overlap-matrix
    text: refuse, coalesce_one, and queue_bounded preserve one active target call per trigger record and deterministic pending identity/order.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon trigger_overlap_policies_preserve_one_active_call_and_order -- --nocapture
  - id: three-capacity-bounds
    text: Per-record pending, resident-wide pending, and resident-wide active-trigger limits refuse without eviction, implicit queueing, or hidden retry.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon trigger_capacity_refuses_at_each_named_bound_without_eviction -- --nocapture
  - id: append-before-visibility
    text: Pending and firing facts are durable before scheduler visibility or call construction; append failure creates neither pending work nor a call.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon trigger_append_failure_suppresses_pending_and_call_effects -- --nocapture
  - id: deterministic-recovery
    text: Restart reconciles trigger projections from the validated ledger and cannot double-admit or double-fire across evaluate, append, project, reserve, execute, and result seams.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon trigger_evaluate_crash_re_evaluate_cannot_double_fire -- --nocapture
  - id: terminal-gating
    text: Skipped, suppressed, capacity-refused, and fired-with-terminal-result occurrences remain terminal after restart and cannot be reconstructed as misses.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon trigger_terminal_dispositions_survive_restart_without_resurrection -- --nocapture
  - id: resident-fairness
    text: Trigger work has a distinct active budget and bounded work turn and cannot consume ordinary call admission or block writer, lifecycle, status, or control reads.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon trigger_load_does_not_starve_writer_control_status_or_ordinary_calls -- --nocapture
  - id: observation-kind-composition
    text: Trigger authority, occurrence, pending, firing, and completion records compose existing observation kinds; no ObservationKind variant is added.
    checked: false
    verify: bash -lc 'test -z "$(git diff 20941a4 -- crates/mct-kernel/src/observation.rs)" && cargo test -p mct-daemon --bin mct-daemon trigger_observation_mapping_uses_existing_kinds -- --nocapture'
  - id: attribution-ledger
    text: Every implemented MctCallTriggerAuthority invariant and structural obligation has a named Track 3 disposition with landed tests or an exact deferred gate.
    checked: false
    verify: bash -lc 'rg -n "MctCallTriggerAuthority|CallTriggerAuthority|CallTriggerFiring|CallTriggerPending" layer/surface/build/spec-drift-audit/track3/LEDGER.md'
  - id: workspace-validation
    text: Part A passes the standing workspace validation suite without Allium edits.
    checked: false
    verify: allium check layer/allium && allium analyse layer/allium && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
---

# Replacement Slice 4A: trigger authority and resident scheduler

> Mother may originate one fresh, fully governed call at a known temporal occurrence only after the exact standing record, occurrence policy, capacity, and current call authority have become durable and current.

## D1 split — ratified 2026-07-21

The operator ratified the D1A/D1B split and directed that Part A implement and land before Part B begins.

This phase is too large for one honest implementation gate. Trigger authority, scheduler recovery, and three policy/capacity matrices form one independently testable subsystem. Watch observation, four host adapters, child call-out, legacy ABI narrowing, two real fixture builds, and the supervised composed proof form another. Combining them would make a red-to-green commit span kernel serialization, schema migration, async scheduling, Wasmtime resources, filesystem safety, fixture provenance, and integration close-out at once.

The proposed gate is therefore:

1. **D1A — this SPEC:** `CallOrigin::TriggerFiring`, trigger authority persistence and management, deterministic temporal scheduler, policy/capacity/recovery law, and the resident execution seam.
2. **D1B — [watch-event-fixtures/SPEC.md](../watch-event-fixtures/SPEC.md):** Watch Toy and supporting fixture effects, WASM child call-out, legacy ABI validation, fixture provenance, and the full supervised composed proof.

Both documents were ratified together. Part A must land first, but it is not Replacement Slice 4 completion and cannot claim an event/fixture proof. Part B begins only after that landing.

## Baseline reconstructed from disk

- Branch: `patina`.
- HEAD: `20941a4fb7ad5275fea578c67715dc335767ace1`, exactly the expected commit; `git diff 20941a4 HEAD` is empty.
- Expected pre-existing untracked artifacts remain outside this SPEC:
  `layer/sessions/20260716-124858-031057000.md` and
  `layer/surface/epistemic/beliefs/evidence-claims-require-landed-proof.md`.
- Baseline `allium check layer/allium`, `cargo test --workspace`, and
  `./scripts/ci-tier0.sh` passed on 2026-07-21 with 344 workspace tests.
- Binding law is [[session-20260721-072059]] as ratified by
  [[commit-20941a4]]. This SPEC does not amend it.

## Problem

The resident can execute authenticated local, process-host, WASM-host, CLI, and terminal Iroh calls, but it has no standing authority record that can originate a future call. There is no scheduler, no durable occurrence identity, and no recovery rule distinguishing a retry from a second firing. Existing request idempotency scopes local calls by adapter and caller; they cannot distinguish two trigger records owned by the same canonical caller.

A timer loop alone would violate the ratified law. It would lose skipped/suppressed/capacity attempts, permit crash duplication, turn configuration into authority, and allow trigger load to compete unboundedly with ordinary resident work. This part implements the smallest temporal runtime that makes those states explicit and leaves the Mother-side event-source adapter unimplemented.

## Goals

1. Add the single truthful `trigger_firing` origin without changing historical serialization.
2. Persist exact, revisioned `CallTriggerAuthority` records as ledger-backed SQLite projections.
3. Expose authenticated create, revise, revoke, show, and list operations.
4. Build deterministic temporal occurrence identities and idempotency keys.
5. Implement all three missed-fire and overlap policies in the ratified order.
6. Enforce named per-record, resident-pending, and resident-active limits without eviction or hidden retry.
7. Make pending admission and firing durable before scheduler visibility and call effects.
8. Reconcile from validated ledger facts after crashes and prevent terminal resurrection.
9. Re-enter the existing payload, idempotency, route, revalidation, execution, result, and observation pipeline.
10. Preserve ordinary call/control/writer capacity under trigger load.

## Non-goals and interdicts

- No Watch Toy, filesystem observation, WASM child call-out, or watcher fixture in Part A.
- No Mother-side event-source adapter. Event-shaped types remain representable, but no event source can activate or fire them in this slice.
- No `RegistrySyncTriggerComposition` or `NetworkArtifactAcquisitionAdapter` work.
- No unattended registry sync, trigger-carried acquisition authority, call origination by standing source authority, or recurring operator-pointed acquisition.
- No network, secret, connection, acquisition, Child, Toy, route, data, or effect authority carried by a trigger record.
- No JVM SDK, paused-epic work, real launchd, `~/.patina`, or legacy-repository mutation.
- No new `ObservationKind`. Discovery of a genuine need for one stops at the operator.
- No ratified Allium edit. A conflict stops implementation and returns to the operator.

## D1A decisions — ratified 2026-07-21

### D1A.1 — `trigger_firing` is additive and local

`CallOrigin` gains exactly:

```rust
#[serde(rename_all = "snake_case")]
pub enum CallOrigin {
    Iroh,
    JvmAdapter,
    WasmHost,
    ProcessHarness,
    Cli,
    TriggerFiring,
}
```

Existing variants retain their current serialized spellings. Golden JSON tests deserialize pre-slice ledgers and requests and prove reserialization does not relabel them. `TriggerFiring` returns `true` from `allows_remote_candidate_sourcing`; an Iroh arrival remains `false`.

Trigger provenance is not added to the wire `MctCall` or accepted from local JSON. The resident introduces a local-only `ResidentCallIngressContext` alongside the semantic request:

```text
ResidentCallIngressContext =
  Peer { binding_id }
| LocalPrincipal { origin, caller }
| Trigger { trigger_authority_id, record_revision, firing_id, occurrence_id }
| ChildCallOut { parent_call_id, parent_firing_id?, depth }
```

The semantic `MctCall.origin` is `TriggerFiring` only for the Trigger case. Forwarding constructs the existing per-hop envelope and the receiver sets origin to `Iroh`; trigger context remains evidence at its verifier and is not forwarded as authority.

**Rationale:** Local context gives idempotency and evidence the exact record identity without enlarging the peer protocol or letting a caller submit trigger provenance.

### D1A.2 — occurrence, firing, call, and idempotency identities are deterministic

All derivations use lowercase BLAKE3 over length-delimited UTF-8 fields and a versioned domain prefix. Implementations expose one shared helper and golden vectors; no caller supplies these identifiers.

```text
occurrence_id = blake3(
  "mct-trigger-occurrence-v1",
  trigger_authority_id,
  record_revision,
  trigger_class,
  canonical nominal time OR retained event identity+sequence
)

represented_set_ref = blake3(
  "mct-trigger-represented-set-v1",
  trigger_authority_id,
  record_revision,
  first_occurrence_id,
  last_occurrence_id,
  count
)

pending_occurrence_id = "pending:" + occurrence_id
firing_id             = "firing:" + occurrence_id
call_id               = "call-trigger:" + occurrence_id
idempotency_key        = "trigger-v1:" + blake3(
  trigger_authority_id,
  record_revision,
  occurrence_id
)
```

A `coalesce_one` catch-up occurrence derives its occurrence id from the exact represented set rather than scheduler time. A retry of one firing recreates the same call id, firing id, idempotency key, target, payload handle, and trace root. Different trigger ids, record revisions, nominal occurrences, or represented sets cannot collide in replay scope.

The resident idempotency caller scope for this origin is exactly `trigger:<trigger_authority_id>`. The record revision and occurrence remain in the key and fingerprint. Existing peer/local scopes are unchanged.

**Rationale:** Stable identity, not timing luck, makes evaluate-crash-re-evaluate a replay rather than a second firing.

### D1A.3 — one immutable record revision binds all authority-bearing inputs

The kernel gains Rust representations and pure validation/evaluation for the ratified `CallTriggerScope`, `CallTriggerAuthority`, policies, state, pending occurrence, and firing evidence. A temporal record stores:

```text
CallTriggerAuthorityRecordV1 {
  trigger_authority_id
  mother_node_id                 # derived current local identity
  vision_id                      # derived current local Vision
  canonical_caller               # derived authenticated local UID + node/Vision
  target                         # one exact WIT operation
  payload_constraint_ref         # immutable local-CAS handle descriptor
  temporal_source {
    anchor_at                    # canonical RFC3339 timestamp
    interval_ms                  # integer >= MCT_TRIGGER_MIN_INTERVAL_MS
  }
  trigger_class = temporal
  trigger_source_ref             # digest of canonical temporal_source
  missed_fire_policy             # default skip
  overlap_policy                 # default refuse
  issuer_principal_ref           # derived os-uid:<uid>
  record_revision
  policy_revision
  starts_at
  expires_at
  authority_state
  authority_observation_id
  canonical_record_digest
}
```

`starts_at < expires_at`; the interval and anchor must admit at least one nominal occurrence within the validity window. The Mother, Vision, canonical caller, issuer, policy revision, and observation id are derived or minted by the resident and are never accepted as client authority claims.

The payload is a bounded immutable existing local-CAS object. `triggers create|revise --payload-json` first uses the current CAS ingest/verification path and records only an algorithm-tagged digest, size, and content type in the trigger record. CAS storage grants no trigger or call authority. Templates are static in this slice: no substitution, environment access, event interpolation, secrets, or path expansion.

**Rationale:** A digest-bound static call shape is enough for the watcher tick and avoids hiding application logic in a scheduler template language.

### D1A.4 — management is an owner-authenticated mutation surface

The production surface is:

```text
mct-daemon triggers create <trigger-id>
  --target <namespace/interface@version.function>
  --payload-json <json>
  --anchor-at <RFC3339>
  --interval-ms <u64>
  --starts-at <RFC3339>
  --expires-at <RFC3339>
  [--missed-fire-policy skip|coalesce-one|fire-late-bounded]
  [--overlap-policy refuse|coalesce-one|queue-bounded]
  [--config path] [--state path] [--ledger path] [--uds path] [--json]

mct-daemon triggers revise <trigger-id>
  --expected-revision <u64>
  <complete replacement scope and policies>
  [paths/json as above]

mct-daemon triggers revoke <trigger-id>
  --expected-revision <u64>
  [paths/json as above]

mct-daemon triggers show <trigger-id> [--state path] [--json]
mct-daemon triggers list [--state path] [--json]
```

Missing policy flags use `skip` and `refuse`. Revision is replace-not-patch: all fields are resubmitted, current revision must match, and the next revision is exactly `current + 1`. Revision supersedes the old revision after the new authority fact is durable. Revocation creates a new revoked revision and cannot be undone under the same id.

Resident UDS mutation authenticates Unix peer UID and uses the administrative mutation sequencer. Offline fallback obtains the exclusive canonical ledger writer and runs the same validator/observation/projection function. A Child can request cadence in business data but no Child, call, manifest, Toy, or host import can invoke these management functions.

`show` and `list` are read-only projections and append no facts.

**Rationale:** Full replacement revisions avoid partially inherited scope and make policy changes incapable of reinterpreting prior occurrences.

### D1A.5 — the ledger is authority; SQLite is current and scheduler projection

Schema v8 adds:

```text
call_trigger_authorities
call_trigger_occurrences
call_trigger_pending_occurrences
call_trigger_firings
call_trigger_projection_meta
```

`call_trigger_authorities` retains every revision under `(trigger_authority_id, record_revision)` and has one current-revision index. Canonical JSON columns use closed enums and deny unknown fields. SQL checks enforce positive revision, ordered validity, temporal source shape, policy vocabulary, and state vocabulary.

`call_trigger_occurrences` is append-only by deterministic occurrence id and stores nominal source, represented-set range/count, missed-fire evaluation, overlap evaluation, final pre-call disposition, exact trigger revisions, and observation references. A terminal disposition cannot update to another disposition.

`call_trigger_pending_occurrences` exists only for durable pending state and stores every field in the ratified entity. It has unique `(trigger_authority_id, admission_sequence)` and deterministic id. Dequeue marks the pending projection consumed only after the next disposition fact is durable; it never deletes evidence.

`call_trigger_firings` is append-only by firing id and uniquely binds occurrence, call, idempotency key reference, trigger revisions, firing observation, and target result reference when terminal.

Every mutation/firing observation carries bounded canonical JSON in `detail_ref` sufficient to validate and replay the projection. `call_trigger_projection_meta` records the last reconciled ledger sequence and ledger identity, not a second journal.

On resident startup, before scheduler readiness, the reconciler opens the validated ledger, replays trigger facts after the projection checkpoint transactionally, validates canonical record digests and legal state transitions, and then compares active/pending/firing rows with current call/idempotency/run facts. A projection row with no matching durable fact grants nothing and is quarantined from scheduler queries. Hash-chain or transition failure keeps the trigger scheduler unavailable and resident readiness honest; it does not start a parallel journal.

**Rationale:** Append-then-project creates a recoverable seam; treating SQLite as canonical would lose a durable firing after append/project crash.

### D1A.6 — temporal occurrence discovery is explicit

Production constants are:

```text
MCT_TRIGGER_MIN_INTERVAL_MS                 = 100
MCT_TRIGGER_SCHEDULER_POLL_MS               = 50
MCT_TRIGGER_MAX_EVALUATIONS_PER_TURN        = 32
MCT_TRIGGER_MAX_RECOVERY_RANGE_OCCURRENCES  = 4096
```

The last constant bounds arithmetic/materialization work, not pending capacity. A larger missed range is represented by exact first/last/count arithmetic and terminal aggregate evidence; it is never iterated without bound.

A nominal temporal occurrence is `anchor_at + n * interval_ms` within `[starts_at, expires_at)`. While a resident remains live, the first scheduler turn that sees a not-yet-dispositioned due occurrence treats it as a live occurrence. At startup/activation, due occurrences strictly after the last durably evaluated nominal boundary and no later than recovery time are the known missed set. A newly activated record with a past anchor uses the same rule.

The scheduler clock is injected behind a `TriggerClock` trait for deterministic tests. Wall-clock reads occur once per turn. Ordering is `(nominal_time, trigger_authority_id, record_revision, occurrence_id)`.

A revoked or superseded revision does not produce unbounded future denials. The first represented due set made ineligible by that transition receives one exact terminal suppression record; the revision is then closed for future occurrence discovery. Pending occurrences from that revision are individually suppressed on dequeue with their existing ids.

**Rationale:** This defines “missed” without pretending a polling wakeup happened at the nominal nanosecond and keeps revoked schedules from becoming denial log generators.

### D1A.7 — missed-fire evaluation is pure, first, and countable

A pure kernel function receives one exact trigger revision, current validity/authority facts, the known missed set, prior terminal evidence, and limits. It returns one of:

- `skip`: no call/pending item; one terminal skipped record names exact first/last/count and observation.
- `coalesce_one`: one deterministic representative occurrence names the complete set and proceeds to overlap evaluation.
- `fire_late_bounded`: ordered per-occurrence representatives up to available bounded admission; excess known occurrences become one terminal capacity-refused represented range/count.

Only temporal occurrences mathematically derived from the durable source are known in Part A. Event gaps never enter this function. Revoked, expired, superseded, narrowed, digest-invalid, or stale-policy records suppress before overlap and cannot catch up regardless of policy.

A policy revision starts a new record revision. The old revision's represented sets and terminal evidence are evaluated only under its old policy.

**Rationale:** Aggregated represented sets retain countability without making long downtime an unbounded row or call storm.

### D1A.8 — overlap evaluation preserves one active call per record

Overlap exists when a firing for the same trigger authority id has a target call without a durable terminal result reference. The check is across revisions of the same trigger id so revision cannot create parallel active work.

- `refuse`: terminal `suppressed`; no pending row or call; evidence names the active firing.
- `coalesce_one`: if no pending representative exists, append one deterministic pending representative. If one exists, append a coalescing transition that expands its represented set deterministically; no second pending item is created.
- `queue_bounded`: append deterministic per-occurrence pending items in occurrence order until capacity; excess is terminally capacity-refused.

Every pending item retains the trigger record revision and both policy evaluations that admitted it. Policy revision, expiry, revocation, supersession, or scope narrowing never rewrites it; dequeue suppresses it under fresh current checks.

**Rationale:** One active call is a database invariant, not a scheduler convention.

### D1A.9 — the fixed five-stage admission pipeline and three limits are named

Production capacity constants are:

```text
MCT_TRIGGER_MAX_PENDING_PER_RECORD = 16
MCT_TRIGGER_MAX_PENDING_RESIDENT   = 256
MCT_TRIGGER_MAX_ACTIVE_CALLS       = 8
```

The active limit is lower than the current ordinary resident default of 64 and is enforced by a trigger-only semaphore. It does not replace or consume the ordinary local-call admission semaphore.

Each representative passes exactly:

1. missed-fire evaluation;
2. overlap evaluation;
3. per-record pending capacity;
4. resident-wide pending capacity or direct active-slot capacity; and
5. fresh trigger, payload, idempotency, child/call, route, revalidation, and effect checks immediately before call construction/execution.

A new pending item that fails stage 3 or 4 is terminally `capacity_refused`; no older item is evicted. A directly fireable occurrence that cannot reserve a trigger active permit is terminally `capacity_refused`; it does not enter pending. Only work already pending because of overlap may remain pending while waiting for a later active permit.

Tests may construct smaller `TriggerLimits` to hit boundaries cheaply, but production construction is fixed to the named values and one test asserts them exactly.

**Rationale:** Separate trigger permits and a bounded scheduler turn make backpressure visible without taking ordinary admission hostage.

### D1A.10 — append-before-visibility governs pending and firing

For a non-fire terminal disposition, the scheduler appends the `LifecycleTransitionRecorded` fact before updating the occurrence projection or watermark.

For pending admission, it:

1. computes deterministic identity and next per-record admission sequence under the scheduler's record lock;
2. appends the pending transition;
3. transactionally inserts/reconciles occurrence and pending projections; and
4. only then exposes the item to the in-memory dequeue set.

For firing, it:

1. reserves a trigger-active permit;
2. rechecks current record, payload CAS, idempotency state, and call inputs;
3. appends `CallConstructed` carrying firing/occurrence/record provenance;
4. projects the firing as active;
5. constructs the immutable request with `origin=TriggerFiring` and the deterministic idempotency key; and
6. enters `execute_resident_call` with `ResidentCallIngressContext::Trigger`.

Append failure releases capacity and creates no projection/call. Projection failure after append suppresses the call in that process; startup or the next reconciliation recovers the durable fact first.

**Rationale:** A projected but unobserved call is forbidden; an observed but not yet projected fact is recoverable.

### D1A.11 — retry and result recovery reuse existing call law

The trigger runtime does not implement a second executor. It supplies local ingress context and calls the shared resident payload/idempotency/routing/execution path.

After a firing fact with no terminal result is recovered:

- no idempotency reservation means retry the same immutable firing;
- a matching completed reservation replays its existing terminal reply/result;
- a matching in-flight reservation remains active and is not executed again; and
- a mismatched reservation is a durable fail-closed scheduler error, never a fresh key.

Local execution references the existing persisted `MctResult` from the run projection. A remote execution may retain the verified opaque `result_ref` to the executor's existing result; it does not synthesize a second outcome enum. Denied-before-route trigger calls receive the shared resident terminal-result projection added by Part A, using the existing `MctResult` and `ResultOutcome::Denied`, so trigger completion can always reference one closed outcome rather than a scheduler-specific status.

A firing is active until the target result reference is durable. Trigger projection completion follows, never precedes, the existing `ResultRecorded` fact. If that final projection is lost, ledger/result reconciliation closes it on restart.

**Rationale:** Scheduler state answers admission; `MctResult` remains the only post-call outcome vocabulary.

### D1A.12 — terminal evidence gates every later evaluation

Before occurrence discovery emits work, it subtracts exact durable represented sets for:

- skipped;
- suppressed;
- capacity-refused; and
- fired occurrences whose call has a durable terminal result.

Subtraction uses occurrence identity/range arithmetic, not timestamps alone. Restart, policy revision, delayed projection, or a later wider capacity limit cannot resurrect those sets. A refused occurrence becomes work only if an explicit later nominal occurrence has a different deterministic identity under ratified policy.

**Rationale:** “Retry later” is not hidden inside a terminal refusal.

### D1A.13 — existing observation kinds compose trigger evidence

No `ObservationKind` variant is added. Typed canonical details and SQLite entities provide trigger vocabulary; kinds retain their current broad event meaning.

| Trigger fact | Existing kind | Plane / outcome |
|---|---|---|
| create/revise/revoke authority attempt | `OperatorActionRecorded` | Operator / allowed or denied |
| skipped, suppressed, capacity-refused, pending admitted/consumed | `LifecycleTransitionRecorded` | Kernel / informational, allowed, or denied |
| firing durable before call | `CallConstructed` | Kernel / allowed |
| current trigger/call denial before construction | `CallDenied` | Kernel / denied |
| target call execution | existing route/runtime/Toy kinds | existing planes/outcomes |
| target terminal result | `ResultRecorded` | Kernel / closed existing outcome |

Every attempt-bearing detail names the exact trigger id/revision, occurrence or represented set, stage, policy result, capacity snapshot, and applicable active firing. Safe messages contain no payload bytes or arbitrary template JSON.

If implementation cannot express a required fact with this table without lying about the kind, it stops for the operator rather than adding a variant.

### D1A.14 — scheduler lifecycle cannot starve resident work

The scheduler is a separate task spawned after ledger reconciliation and before readiness. It owns:

- one bounded timer;
- no unbounded channel;
- a trigger-only active semaphore of eight;
- at most 32 evaluations per turn; and
- deterministic yielding after each turn.

It never holds the administrative mutation sequencer while executing a call. Trigger create/revise/revoke serialize only their short authority mutation. Status and read-only control use existing independent dispatch. Ledger appends use the canonical writer queue and are awaited; writer fencing disables the scheduler and marks readiness unhealthy under existing law.

Shutdown stops new evaluation, lets already executing calls follow the existing bounded shutdown behavior, and records no clean trigger completion without a durable target result. Restart reconciliation determines what remains.

### D1A.15 — Mother-side event observation is deferred under one named slot

Part A represents `trigger_class=event` in kernel types and identity helpers but production create/revise rejects it with:

```text
MotherEventSourceAdapterRuntime is not implemented; use temporal triggers or return to D1.
```

The named slot **MotherEventSourceAdapterRuntime** covers only the already-ratified Mother-observer path: independent event-source effect authority, durable receipt/eligibility, exact event identity/sequence, and matching current trigger authority. It does not reopen event-source placement or permit Child impersonation.

The Child-observed watcher path in Part B does not close this slot because its delivery is an ordinary `WasmHost` call, not a Mother event-triggered firing.

**Rationale:** Implementing a second observer path that no required fixture uses would expand this slice and its failure matrix without increasing the replacement proof.

### D1A.16 — coupled registry/network slots remain untouched

Trigger command targets reject registry acquisition/sync internal operations in this slice. More fundamentally, there is no internal triggerable registry-sync target. The following remain jointly deferred and unchanged:

- `RegistrySyncTriggerComposition`;
- `NetworkArtifactAcquisitionAdapter`.

No trigger payload, source ref, target alias, or scheduler callback may contain source authority, operator-pointed acquisition, credential, connection, secret, or acquisition-adapter capability.

## Persistence transition rules

At minimum, schema constraints and Rust validators enforce:

- one closed trigger class/source shape per revision;
- one current revision per trigger id;
- `record_revision >= 1` and exactly incrementing replacement;
- nonblank caller/target/source/payload references;
- valid ordered timestamps and interval;
- closed missed-fire, overlap, authority, pending-reason, and disposition enums;
- one deterministic occurrence row per identity;
- no pending row for terminal non-pending disposition;
- one pending row per pending id and one active firing per trigger id;
- no firing without a prior durable firing observation;
- no terminal firing reference without an existing target result/ref;
- no projection checkpoint advance beyond facts applied in the same transaction; and
- no trigger table, JSON, observation, or response field for registry/acquisition/network/secret authority.

Migration from schema v7 creates empty trigger tables and checkpoint metadata. It does not infer trigger records from tasks, cron-like configuration, child subscriptions, registry sources, or historical call traffic.

## Response and inspection contract

Create/revise/revoke return only after ledger append and projection success:

```text
CallTriggerAuthorityReport {
  trigger_authority_id
  record_revision
  authority_state
  mother_node_id
  vision_id
  canonical_caller_ref            # bounded safe projection, not submitted identity
  target
  payload_constraint_ref
  trigger_class
  trigger_source_ref
  anchor_at?
  interval_ms?
  missed_fire_policy
  overlap_policy
  starts_at
  expires_at
  policy_revision
  authority_observation_id
  canonical_record_digest
}
```

`show` additionally projects pending count, active firing id/call id, last evaluated nominal boundary, and recent terminal disposition counts. It does not expose payload bytes or idempotency keys; `idempotency_key_ref` is a digest reference.

## Failing-test-first implementation order

1. Add red kernel tests for origin serialization/routing and deterministic identity golden vectors.
2. Add red schema-v7 migration and trigger transition/constraint tests.
3. Add red authenticated create/revise/revoke and append-failure tests.
4. Implement kernel trigger types, pure validation, missed-fire, overlap, and identity helpers.
5. Add state records, ledger reconciliation, projection checkpoint, and management surfaces.
6. Refactor resident execution to accept local-only ingress context and shared terminal `MctResult` projection without changing peer/local JSON.
7. Add the red injected-clock scheduler test for one temporal firing through the existing resident call path.
8. Implement scheduler lifecycle, bounded turn, active semaphore, and append/project barriers.
9. Add each missed-fire policy test separately.
10. Add each overlap policy test separately.
11. Add per-record, resident-pending, and resident-active refusal tests separately.
12. Add append/project/call/idempotency crash-seam tests and restart reconciliation.
13. Add revocation/supersession/expiry/terminal resurrection and fairness tests.
14. Add every D1A Track 3 row before declaring Part A complete.

Every implementation commit is one concern and must pass:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

Any Allium conflict stops; this SPEC authorizes no Allium commit.

## Required Part A integration proof

The primary Part A test
`resident_temporal_trigger_fires_once_and_recovers_without_duplication` must run with an injected clock and isolated config/state/ledger/CAS/children paths:

1. Start a test resident with one acquired, exactly approved, assigned, ready local WIT child and no trigger records.
2. Create one temporal trigger through authenticated mutation ingress while omitting policy flags; prove persisted defaults are `skip` and `refuse` and the authority observation precedes active projection.
3. Advance to one nominal occurrence; prove the firing observation is durable before call construction/effect and carries exact trigger id, record revision, policy revision, source, occurrence, and represented-set identity.
4. Prove the semantic call uses `origin=trigger_firing`, deterministic call/idempotency identity, exact static CAS payload, and ordinary current routing/revalidation/execution law.
5. Prove the target's existing durable successful `MctResult` is referenced by the firing and the trigger active slot is released only after that result.
6. Re-evaluate the same nominal occurrence and prove idempotent replay/terminal gating produces no second child effect.
7. Create a second pending occurrence under overlap, stop after durable pending admission, and restart from the same ledger/state.
8. Reconcile the ledger projection before scheduler readiness; prove deterministic pending identity/order and no duplicate admission.
9. Revoke the trigger before the next nominal occurrence; advance once and prove one terminal suppression record and no call.
10. Restart again; prove fired, pending-consumed, and suppressed occurrence sets remain terminal and no miss reconstruction resurrects them.
11. Submit an ordinary authenticated local call while trigger capacity is saturated; prove it remains independently admitted and status/control reads remain responsive.
12. Reopen ledger/state from disk and correlate authority, occurrence, pending, firing, call, route, result, and revocation references without payload bytes or idempotency key material.

Close-out must cite the exact landed test file:line range for every step. “Matched” without assertion citations is rejected.

## Additional required failure proofs

Named tests must prove:

- pre-slice CallOrigin JSON remains stable and unknown origins still fail closed;
- a locally submitted body cannot claim `trigger_firing` or trigger context;
- two records with equal callers/targets/payloads never share replay state;
- two occurrences and two revisions of one record never share keys;
- create/revise rejects missing scope, caller claims, blank target, malformed CAS ref, missing bytes, invalid interval/window, and unknown enums;
- revise rejects stale expected revision, partial replacement, illegal state revival, and nonincrementing revision;
- append failure suppresses activation, revision, revocation, pending insertion, firing, and acknowledgement;
- projection failure after append is recovered from ledger without duplicate effect;
- `skip`, `coalesce_one`, and `fire_late_bounded` each preserve exact represented range/count and deterministic identities;
- `refuse`, `coalesce_one`, and `queue_bounded` each preserve one active call and deterministic pending order;
- missed-fire always precedes overlap at their crash seam;
- per-record pending, resident pending, and direct active limits each produce terminal capacity refusal without eviction;
- pending overlap work may await active capacity, while directly fireable work never enters an implicit queue;
- revocation, expiry, supersession, stale policy, missing payload, denied child, and failed route defeat live, catch-up, and dequeue paths;
- failed, timed-out, cancelled, denied, and successful target results all reuse `MctResult` and release/retain state correctly;
- a recovered pre-call firing retries the same identity, a completed reservation replays, and an in-flight reservation never executes twice;
- terminal skip/suppression/capacity/result facts survive reopen and cannot be relabeled as misses;
- scheduler work-turn and trigger semaphore limits preserve ordinary call/control/writer progress;
- event trigger creation is refused under `MotherEventSourceAdapterRuntime`;
- registry sync/acquisition targets and authority fields remain absent/refused; and
- the trigger implementation adds no `ObservationKind` variant.

## Track 3 attribution gate

Part A adds explicit rows for all structural obligations for:

- `CallTriggerScope`;
- `CallTriggerAuthority`;
- `CallTriggerFiringEvidence`;
- `CallTriggerPendingOccurrence`;
- their readers and projection surfaces.

It must disposition every invariant in `MctCallTriggerAuthority`:

- `ManagementRequiresCurrentLocalAuthority`;
- `TriggerScopeIsExplicitAndBounded`;
- `ActivationFollowsDurableAuthorityFact`;
- `TriggerAuthorityCannotExpandCallAuthority`;
- `EachFiringCreatesFreshCall`;
- `StaleTriggerCannotPreserveAuthority`;
- `ChildRequestIsNeverTriggerGrant`;
- `FiringEvidenceCarriesTriggerProvenance`;
- `TriggerFiringOriginIsTruthfulAndAdditive`;
- `TriggerFiringIdempotencyIsRecordAndOccurrenceScoped`;
- `TriggerFiringIsLocalAndSingleHop`;
- `MechanismDoesNotOwnCadenceMeaning`;
- `MissedFirePolicyIsExplicitAndDefaultsToSkip`;
- `CatchUpUsesOnlyKnownOccurrences`;
- `CurrentAuthorityDefeatsCatchUp`;
- `CatchUpIsBoundedByNamedConstants`;
- `MissedFireDispositionsRemainEvidence`;
- `PolicyRevisionDoesNotReinterpretMisses`;
- `CatchUpIdentityIsDeterministic`;
- `OverlapPolicyIsExplicitAndDefaultsToRefuse`;
- `OneActiveCallPerTriggerRecord`;
- `OverlapPendingStateIsPerRecordBounded`;
- `QueueAdmissionIsNotDeliveryOutcome`;
- `CoalescingStagesRemainDistinct`;
- `PendingIdentityAndOrderAreDeterministic`;
- `MissedFirePrecedesOverlap`;
- `TriggerQueuesAndActiveCallsUseThreeNamedBounds`;
- `AdmissionOrderIsFixedAndAuthorityNeutral`;
- `PendingAdmissionIsDurableBeforeVisibility`;
- `PendingAdmissionNeverEvicts`;
- `NoImplicitResidentRetryQueue`;
- `TriggerWorkCannotStarveResidentControl`;
- `DequeueAndRestartAreDeterministic`;
- `DequeueRechecksCurrentLaw`;
- `TerminalDispositionPreventsResurrection`; and
- `LaterOccurrencesRequireExplicitPolicy`.

`MotherEventSourceAdapterRuntime`, `RegistrySyncTriggerComposition`, and
`NetworkArtifactAcquisitionAdapter` may be `DEFERRED` only with their exact
interdicts and proof no executable path was introduced.

## Verification and close-out contract

Part A close-out is reconstructed from disk and includes:

- expected starting tree and full Part A commit range;
- the first red-test transcript;
- per-commit standing validation;
- `--nocapture` output for every named policy, capacity, crash, terminal, and fairness test;
- a flake log with the first failure verbatim before one rerun, or exactly `none`;
- the 12-step proof diff with exact test file:line citations;
- every Track 3 row or explicit operator-approved waiver;
- proof `git diff 20941a4 -- layer/allium` is empty;
- proof no observation kind was added; and
- an explicit statement that Part A alone does not complete fixtures two/three or the three-fixture replacement proof.

## Build readiness

**D1A is ratified.** Checkpoint-commit both ratified SPECs, then implement Part A failing-test-first. Part B must not begin until Part A lands. Stop only on a genuine design fork or conflict with ratified law.

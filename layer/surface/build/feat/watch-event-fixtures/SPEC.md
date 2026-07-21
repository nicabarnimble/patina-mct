---
type: feat
id: watch-event-fixtures
status: complete
draft_date: 2026-07-21
ratified_date: 2026-07-21
completed_date: 2026-07-21
target: replacement-slice-4b
operator_gate: D1-ratified
depends_on:
  - trigger-event-runtime
sessions:
  origin: 20260721-072059-435043000
  work:
    - 20260721-092955-065681000
related:
  - layer/surface/build/feat/watch-event-fixtures/CLOSEOUT.md
  - layer/surface/build/feat/trigger-event-runtime/SPEC.md
  - layer/surface/build/feat/artifact-acquisition/SPEC.md
  - layer/allium/mct-product-map.allium
  - layer/allium/mct-patina-migration.allium
  - layer/sessions/20260721-072059-435043000.md
  - layer/surface/build/product/MCT-NEXT-BUILD-TODO.md
  - layer/surface/build/product/MOTHER-REPLACEMENT-RUNBOOK.md
  - layer/surface/build/spec-drift-audit/track3/LEDGER.md
  - crates/mct-kernel/src/toy.rs
  - crates/mct-daemon/src/toy.rs
  - crates/mct-daemon/src/wasm.rs
  - crates/mct-daemon/src/daemon/supervisor_lifecycle.rs
exit_criteria:
  - id: watch-scope-authority
    text: A Child watch requires a current exact WatchObservationScope and Watch ToyGrant; missing, stale, expired, revoked, superseded, wrong-artifact, or wrong-root facts deny before observation.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon watch_scope_and_toy_grant_are_both_current_before_observation -- --nocapture
  - id: observation-only-boundary
    text: Watch authority permits bounded directory observation and safe metadata only; content reads and child state require separate current ToyGrants, and no watch grant can originate a call.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon watch_grant_cannot_read_content_state_or_originate_delivery -- --nocapture
  - id: safe-watch-metadata
    text: Existing subjects are canonical and root-relative, deleted subjects require prior in-scope identity, and symlink escapes are excluded without leaking escaped or absolute paths.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon watch_adapter_excludes_escaped_symlinks_and_absolute_paths -- --nocapture
  - id: bounded-batch-evidence
    text: Every scan batch has exact scope revision, monotonic per-scope sequence, deterministic coalescing/order, and countable raw, eligible, coalesced, excluded, and capacity-refused counts.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon watch_batches_are_bounded_sequenced_deterministic_and_countable -- --nocapture
  - id: synchronous-send-admission
    text: producer.send synchronously validates exact Watch authorization, topic/content/bounds, event shape/class, safe path, batch capacity, and legacy equality; every failure returns a typed refusal without joining the admitted set.
    checked: false
    verify: cargo test -p mct-daemon watch_send_admission_refuses_paths_shape_and_capacity_synchronously -- --nocapture
  - id: batch-admission-barrier
    text: The normalized admitted set and pre-call dispositions append durably as one batch before the first nested delivery; append failure suppresses every nested call and delivered claim.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon watch_admission_append_failure_suppresses_every_nested_delivery -- --nocapture
  - id: wasm-child-callout
    text: The watcher messaging import may propose one fresh child call with WasmHost origin, but the target still passes ordinary payload, idempotency, child, route, revalidation, effect, result, and observation law.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon watcher_child_callout_reenters_ordinary_call_law -- --nocapture
  - id: legacy-abi-narrowing
    text: For patina:watch/events@0.1.x only, absolute-path must byte-equal the safe root-relative path before a call is constructed; mismatch is durably refused and successor contracts cannot carry the slot.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon legacy_watch_abi_mismatch_is_refused_before_sink_call -- --nocapture
  - id: truthful-lineage
    text: Trigger-started watcher output links the exact parent firing/record/revision; manual watcher invocation has only its actual parent call and never fabricated trigger lineage.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon watch_delivery_lineage_is_actual_and_never_fabricated -- --nocapture
  - id: result-vocabulary
    text: Delivery disposition is pre-call only and delivery completion references the target call's existing MctResult; no delivery-specific post-call outcome enum exists.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon watch_delivery_reuses_closed_mct_result_outcomes -- --nocapture
  - id: fixture-provenance
    text: Raw fixture directories contain real source-derived folder-watch-actor@0.1.0 and exact watch-null-sink@0.1.0 builds with upstream/tag/commit/build/patch provenance, sizes, SHA-256, BLAKE3, and no source sidecars.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon watcher_fixture_provenance_is_exact_source_derived_and_sidecar_free -- --nocapture
  - id: supervised-composed-proof
    text: A supervised resident acquires, approves, assigns, and grants both fixtures, then proves temporal TriggerFiring to Watch Toy observation to ordinary WasmHost delivery to validated legacy ABI to watch-null-sink.emit with reopened correlated evidence.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon supervised_trigger_watch_delivery_fixtures_execute_end_to_end -- --nocapture
  - id: revocation-and-restart
    text: Trigger revocation suppresses the next occurrence, Watch scope/grant revocation denies before observation, and restart reconstructs pending/terminal/batch sequence state without double fire or delivery.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon supervised_trigger_watch_delivery_fixtures_execute_end_to_end -- --nocapture
  - id: no-new-observation-kind
    text: Watch, event, disposition, call-out, ABI, and completion evidence compose existing ObservationKind values; no kernel kind is added.
    checked: false
    verify: bash -lc 'test -z "$(git diff 20941a4 -- crates/mct-kernel/src/observation.rs)" && cargo test -p mct-daemon --bin mct-daemon watch_delivery_observation_mapping_uses_existing_kinds -- --nocapture'
  - id: attribution-ledger
    text: Every MctEventSourcePlacement, MctWatchObservationScope, MctLegacyWatchEventsCompatibility, MctWatchEventDelivery, and PatinaWatcherQuarryDisposition invariant has a named Track 3 disposition.
    checked: false
    verify: bash -lc 'rg -n "MctEventSourcePlacement|MctWatchObservationScope|MctLegacyWatchEventsCompatibility|MctWatchEventDelivery|PatinaWatcherQuarryDisposition" layer/surface/build/spec-drift-audit/track3/LEDGER.md'
  - id: workspace-validation
    text: Part B and the full phase pass the standing validation suite without Allium edits.
    checked: false
    verify: allium check layer/allium && allium analyse layer/allium && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
---

# Replacement Slice 4B: watch delivery and fixtures two + three

> A real watcher may observe only through current scoped Toys and may propose delivery only as a fresh ordinary call; compatibility preserves the legacy ABI shape without preserving unsafe path meaning.

## Gate relationship — ratified 2026-07-21

The operator ratified all D1B rulings, including the fixture security rebuild, supporting Toy/call-out surfaces and constants, and the named Mother adapter deferral. Part A must implement and land before Part B begins.

This is Part B of the D1 split. The split rationale and trigger-side decisions are in [trigger-event-runtime/SPEC.md](../trigger-event-runtime/SPEC.md). Both D1A and D1B were ratified together on 2026-07-21.

Part B consumes only this interface from Part A:

```text
- CallOrigin::TriggerFiring and ResidentCallIngressContext::Trigger
- deterministic firing/occurrence/call/idempotency identity
- ledger-backed current CallTriggerAuthority projection
- temporal scheduler and current result/pending reconstruction
- shared resident call executor and terminal MctResult projection
```

Part B does not revise trigger policies, capacities, routing, or recovery. Its fixture proof treats Part A as a governed call source.

## Quarry finding and ratified D1B ruling

The exact read-only quarry is commit/tag
`526dbf123b040198cb4395c1a63cf498a28ff915` in
`patina-child-watcher-system` (`folder-watch-actor-v0.1.0` and
`watch-null-sink-v0.1.0`). It creates a necessary distinction:

- `watch-null-sink@0.1.0` can be rebuilt byte-for-byte from the exact tag and uses only logging and measure imports already expressible under ToyGrant-backed host adapters.
- The exact upstream `folder-watch-actor@0.1.0` source records a rooted guest path in `absolute-path`, a stripped path in `relative-path`, imports broad unused legacy capabilities, and obtains ambient WASI filesystem/keyvalue/messaging behavior. Its unmodified component therefore cannot both produce a successful event and satisfy the ratified equality narrowing or Watch Toy placement.

D1B ratifies a **source-derived MCT rebuild**, not a claim that the upstream release binary is safe unchanged. A committed minimal patch is applied to the exact tag in a temporary build tree. The patch may only:

1. remove statically imported interfaces the source never calls;
2. change legacy event construction so `absolute-path` receives the same safe root-relative value as `relative-path`; and
3. make build-only binding updates required by those two changes.

It may not change scan/diff/filter/cadence behavior, event classes, WIT business exports, sink behavior, package name, or version. Provenance names the upstream commit, patch digest, exact diff, toolchain, build command, output hashes, and the fact that this is an MCT security rebuild rather than the unmodified upstream release binary.

If the operator requires the unmodified folder watcher binary instead, **this SPEC is blocked**: its successful payload necessarily violates the already-ratified ABI narrowing. The byte-exact requirement in law is the `patina:watch/events@0.1.x` ABI shape, not unsafe artifact bytes.

## Problem

The daemon has ToyGrant evaluation and Wasmtime adapters for several Slate imports, but it has no canonical Watch Toy, no `WatchObservationScope`, no durable batch/event/delivery projections, no keyvalue host for the real watcher, and no way for a running Child to originate a fresh ordinary call. Existing host-adapter evaluation is also accumulated with runtime observations; watch source and delivery facts need a synchronous append barrier before observation and nested call effects.

The prior fixture used `patinaToy` manifest needs, WASI filesystem preopens, and messaging as ambient runtime wiring. Those shapes are behavior evidence only. This part rebuilds them as explicit `mctToy` grants and ordinary call law while retaining the exact legacy event WIT shape under the ratified narrowing.

## Goals

1. Persist and evaluate exact `WatchObservationScope` records.
2. Add a canonical Watch Toy whose authority is observation-only.
3. Support the real watcher's separately authorized read-only content and keyvalue effects.
4. Generalize logging/measure Toy contracts so each fixture receives its own exact grants.
5. Add a synchronous WASM child-call-out bridge for the exact messaging import.
6. Validate the legacy absolute/relative path equality before target call construction.
7. Persist complete batch, event, pre-call disposition, and delivery/result evidence.
8. Import both real 0.1.0 build fixtures through acquisition-backed staging.
9. Prove the complete temporal-trigger-to-sink flow under a supervised resident.
10. Prove revocation, crash/restart, capacity, escape, mismatch, and truthful-lineage failures.

## Non-goals and interdicts

- No Mother-side event-source adapter; `MotherEventSourceAdapterRuntime` remains deferred.
- No new trigger policy, queue, capacity, or identity semantics beyond D1A.
- No new `ObservationKind`.
- No network, HTTP, SQL, Git, peer, task, registry-sync, acquisition-trigger, or secret adapter for these fixtures.
- No write/delete filesystem authority. The watcher gets directory observation and optional content read only.
- No general message broker, durable pub/sub service, topic registry, or ambient stream subscription.
- No successor watch-events ABI in this slice; only the rule that a successor cannot carry `absolute-path`.
- No mutation of the quarry repository, real launchd, `~/.patina`, or installed plugins.
- No edits to ratified Allium law. Conflicts stop at the operator.
- No JVM SDK and no paused-epic resumption.

## D1B decisions — ratified 2026-07-21

### D1B.1 — watch scope is a distinct ledger-backed authority projection

The kernel gains Rust types and pure validation for the ratified `WatchObservationScope`, event classes, traversal, coalescing, lifecycle state, and current evaluation. The production record is:

```text
WatchObservationScopeV1 {
  watch_scope_id
  observer_shape = child_toy
  observer_ref {
    child_name
    artifact_id
    artifact_version
    assignment_id
  }
  scope_mode                     # constrained | explicit_broad
  canonical_root_ref             # canonical file:// URI, credential-free
  traversal_scope                # root_only | recursive
  event_classes                  # non-empty ordered subset created/modified/deleted
  max_events_per_batch           # 1..=MCT_WATCH_MAX_EVENTS_PER_BATCH
  coalescing_policy              # none | last_per_path
  starts_at
  expires_at
  scope_revision
  policy_revision
  authority_state
  authority_observation_id
  canonical_record_digest
}
```

`explicit_broad` still names one canonical root; it means deliberately broad beneath that root, never machine-wide ambient filesystem access. Revision follows replace-not-patch and exactly increments. Missing, expired, revoked, superseded, wrong-artifact, wrong-assignment, stale-policy, unobserved, ledger-digest-mismatched, or root-mismatched scope denies before preopen or scan.

Schema adds `watch_observation_scopes` retaining all revisions and a current index. The ledger is canonical and startup reconciliation follows D1A's append/replay checkpoint pattern.

**Rationale:** ToyGrant says which Child may ask for the Toy; the Watch scope says exactly what source observation means. Neither can substitute for the other.

### D1B.2 — one command records scope and Watch ToyGrant as separate facts

The production surface is:

```text
mct-daemon toys grant-watch <child-name> <canonical-root>
  --scope-id <id>
  --traversal root-only|recursive
  --events created[,modified,deleted]
  --max-events-per-batch <n>
  --coalescing none|last-per-path
  --starts-at <RFC3339>
  --expires-at <RFC3339>
  [--scope-mode constrained|explicit-broad]
  [--children-dir path] [--config path] [--state path]
  [--ledger path] [--uds path] [--json]

mct-daemon toys revoke-watch <scope-id>
  --expected-revision <n> [paths/json]

mct-daemon watch scopes show <scope-id> [--state path] [--json]
mct-daemon watch scopes list [--state path] [--json]
```

`grant-watch` resolves the exact current acquired artifact, approval, assignment, and ready Child; creates one Watch scope and one `ToyGrant` for `toy:mct:watch-observation` action `observe`; and appends both authority facts before projecting either active. It is one authenticated convenience mutation over two independently inspectable facts, following the existing approval/assignment precedent. The v1 WASI-preopen adapter accepts only explicit `--traversal recursive`; `root-only` is parsed as a distinct contract value but the command returns typed unsupported before any authority append rather than widening it.

`revoke-watch` writes a new revoked scope revision and revokes only the matching Watch grant. It does not revoke content-read, keyvalue, logging, measure, Child approval, assignment, or trigger authority. Repeated/stale revocation is an observed refusal or no-op according to current mutation law, never silent mutation.

A Child request, manifest `needs`, configured root, or imported WASI interface cannot invoke these commands or create their facts.

### D1B.3 — Watch Toy is observation-only and uses a capability-gated WASI session

The canonical contract is:

```text
toy_id: toy:mct:watch-observation
contract: mct:watch/observation@0.1.0.observe
resource: watch-scope:<scope-id>:<revision>
action: observe
```

Kernel evaluation first obtains the existing non-clone `AuthorizedToyCall`, then composes it with the exact current scope into a non-clone `AuthorizedWatchObservationSession`. The wrapper exposes the scope and original decision/token references but cannot be serialized, cloned, stored, or minted by an adapter.

The real watcher uses Rust `std::fs` through WASI rather than a custom Watch import. The Wasmtime adapter treats creation of its `/input` directory preopen as the watch effect boundary:

- Watch authority alone permits directory enumeration, path resolution, and safe metadata with `DirPerms::READ` and no regular-file read permission.
- The root is selected only from the current scope; guest payload/config cannot select a host root.
- The v1 adapter admits only an explicitly `recursive` scope. An explicit
  `root_only` record remains a valid law shape but is typed unsupported by this
  WASI-preopen adapter and is denied before preopen rather than widened to
  recursive behavior.
- Allowed event classes are revalidated against emitted evidence.
- `ToyCallStarted` is appended before the preopen becomes visible to the component.
- `ToyCallCompleted` or `ToyCallFailed` brackets the scan session and carries no absolute host path.

Possessing this session does not expose messaging/call-out, keyvalue, content bytes, writes, deletes, network, or another root. It cannot originate a call.

**Rationale:** The adapter preserves the exact source's WASI filesystem use while replacing ambient preopen configuration with a kernel-minted Watch capability.

### D1B.4 — content reads require an independent read-only directory ToyGrant

The source-derived watcher hashes file bytes. That is content access, not safe watch metadata. This slice admits the first narrow read-only directory contract:

```text
toy_id: toy:mct:directory-read
contract: wasi:filesystem/preopens@0.2.3.read-file
resource: canonical file://<same-or-narrower-root>
action: read-content
```

Production command:

```text
mct-daemon toys grant-directory-read <child-name> <canonical-root>
  --expires-at <RFC3339> [paths/json]
```

A current exact grant permits `FilePerms::READ` only beneath its canonical root. No write, create, rename, delete, symlink-follow escape, special file, device, FIFO, socket, or second preopen is permitted. The Watch root and content root are evaluated independently; for the fixture the content root must equal or be narrower than the Watch root. Directory-read authority alone cannot enumerate a watch source because the runtime does not configure the watcher preopen without the Watch session.

Revoking or omitting content-read may cause the exact watcher scan to fail when it hashes a file, but cannot widen Watch authority. This contract dispositions only the read-only directory row in TODO item 3; read-write, write-only, blob storage, and network remain future work.

**Rationale:** Treating a Watch grant as permission to hash contents would directly violate the ratified observation-only boundary.

### D1B.5 — watcher state uses a separate bounded keyvalue Toy

The exact watcher stores configuration, snapshot, and counters through `wasi:keyvalue/store@0.2.0`. This slice adds:

```text
toy_id: toy:mct:child-keyvalue
contract: wasi:keyvalue/store@0.2.0
resource: child:<artifact-id>:bucket:<name>
actions: get | set | delete | exists | list-keys
```

Production command:

```text
mct-daemon toys grant-keyvalue <child-name> <bucket-name>
  --expires-at <RFC3339> [paths/json]
```

Named bounds are:

```text
MCT_KEYVALUE_KEY_MAX_BYTES       = 128
MCT_KEYVALUE_VALUE_MAX_BYTES     = 262144
MCT_KEYVALUE_MAX_KEYS_PER_BUCKET = 128
MCT_KEYVALUE_LIST_PAGE_MAX       = 128
```

The adapter reuses the existing `child_state` SQLite substrate with keys namespaced by exact artifact, assignment, and bucket. Values are opaque bytes encoded in the projection, never ledger payload. Every host call receives fresh current Toy evaluation and existing ToyCall start/completion/failure observations. Set refuses at capacity without eviction. Listing is lexical and bounded. A grant is local, exact-child, exact-artifact, exact-assignment, Vision/Node scoped, and grants no filesystem, watch, call, or downstream effect authority.

**Rationale:** Rebuilding keyvalue under ToyGrant is narrower and more faithful than moving the Child's snapshot/diff state into Mother scheduler logic.

### D1B.6 — logging and measure become reusable canonical contracts

The existing Slate-branded toy ids remain readable for historical state, but new grants use canonical ids:

```text
toy:mct:wasi-logging    -> wasi:logging/logging@0.1.0.log
toy:mct:patina-measure  -> patina:measure/measure@0.1.0.(gauge|counter)
```

`toys grant-observability <child-name> [--logging] [--measure]` issues separate exact-artifact grants. Runtime adapter selection resolves current contract identity plus subject rather than hardcoding Slate ids. Existing `toys authorize-slate` becomes a convenience wrapper over the same shared grant writer and remains behavior-compatible.

The watcher and sink receive distinct grants. Sink delivery cannot borrow watcher grants, and successful call delivery grants no logging or metric effect.

### D1B.7-A — watcher admission is synchronous; deterministic delivery is buffered

Operator amendment accepted 2026-07-21 after checkpoint review. The MCT rebuild retains the exact source's used `wasi:messaging/producer@0.2.0` import, but the host implementation is not a general broker.

`producer.connect(name)` accepts only a canonical operation id of the form
`<namespace>/<interface@version>.<function>` and returns an invocation-local resource. It creates no durable subscription, route, target approval, or standing authority. The integration configures the watcher stream to `patina:watch/events@0.1.0.emit` before activating the trigger.

`producer.send(client, message)` is the synchronous admission boundary. Before returning an admitted offset it:

1. requires current exact Watch Toy authorization;
2. accepts only topic `file-created|file-modified|file-deleted`, content type `application/json`, bounded payload/metadata, and the exact watcher event JSON shape;
3. validates safe path, event class against the current scope, per-scope and resident batch capacity, and exact legacy ABI equality; and
4. returns a stable typed refusal synchronously when any check fails, without adding the message to the admitted set or constructing a target call.

An admitted offset means only that the invocation-local host buffer accepted the event. It does not mean that evidence is durable, a target call was admitted, or delivery succeeded. After the watcher export returns, Mother normalizes the complete admitted set into deterministic batch order. Before the parent call completes it:

1. appends the batch/event/pre-call disposition facts under the ratified D1B.7-A.1 timing;
2. constructs one fresh immutable target request per fired disposition with `CallOrigin::WasmHost` and local-only `ResidentCallIngressContext::ChildCallOut`;
3. re-enters the shared resident payload/idempotency/child/routing/revalidation/effect/result pipeline; and
4. appends/result-projects delivery completion before acknowledging parent-call completion.

The exact watcher cannot both reveal future sends to the host and block its first send until scan-wide sorting completes. D1B.7-A therefore makes the parent invocation the atomic evidenced unit while preserving synchronous fail-closed admission and deterministic whole-batch identities.

The target call's canonical caller copies the current local node/Vision/project and authenticated principal from the parent call; it does not claim the Child is an OS principal. Causal evidence names the exact parent call and authorized Child invocation.

Call-out identity is deterministic:

```text
callout_event_id = blake3(parent_call_id, batch_id, batch_position, canonical event)
call_id          = "call-wasm-host:" + callout_event_id
idempotency_key  = "wasm-host-v1:" + blake3(parent_call_id, callout_event_id, target)
```

The local caller idempotency scope remains the truthful WasmHost/canonical-caller scope. A replay of one host send reuses its key; another event cannot share it.

`MCT_CHILD_CALLOUT_MAX_DEPTH = 1`. A Child call-out target may execute or use its own Toys, but a second nested call-out is refused with evidence. This prevents cycles without creating graph/broker authority.

**Rationale:** The host import proposes a call; only the existing target authority and routing pipeline decide whether it runs.

### D1B.8 — exact legacy ABI narrowing occurs before call construction

The broker parses the exact 0.1.x `file-change` fields:

```text
watcher
stream-name / source JSON key stream
change-kind
absolute-path
relative-path
size-bytes?
modified-unix-ms?
sha256?
detected-at
```

Before creating a target payload or `MctCall`, it requires:

```text
absolute-path == relative-path == root-relative canonical safe path
```

Equality is byte equality after validating the value itself as normalized UTF-8 path segments. Neither value may be absolute, empty, contain `.`, `..`, NUL, platform prefix, separator ambiguity, or resolve outside the scope. Mismatch produces `mismatch_refused`, durable `DataMovementDenied`, no target call id, and no sink effect. The adapter never rewrites a mismatched event into compliance.

The source-derived patch makes legitimate watcher output equal before it reaches this check. The check remains mandatory and is separately tested with a mismatching malicious sender.

The rule is dispatched only for exact operation package/interface version `patina:watch/events@0.1.x`. No wildcard version fallback exists. A future successor registration fails schema validation if its field model contains `absolute-path`.

### D1B.9 — safe metadata and deterministic polling are explicit

Production constants are:

```text
MCT_WATCH_MAX_EVENTS_PER_BATCH = 128
MCT_WATCH_MESSAGE_MAX_BYTES    = 65536
MCT_WATCH_METADATA_PAIRS_MAX   = 16
```

The temporal trigger supplies polling cadence by invoking the Child's existing `scan-now`; Mother does not own why the cadence matters. The Child retains scan, snapshot comparison, extension/hidden filtering, diff, and event construction. The Watch adapter supplies only the scoped preopen and validates safe mechanical observation/delivery facts.

For an existing path, the adapter canonicalizes the host subject, proves it remains under canonical root, derives the normalized root-relative path, and compares it with the emitted narrowed slots. Symlinks are inspected without following an escape. An escaped or special subject increments `excluded_event_count` and never exposes its host/escaped path.

For deletion, where the subject no longer canonicalizes, eligibility requires the same normalized relative identity to exist in the prior durable in-scope observation snapshot for that exact scope revision. A deletion not backed by prior scope evidence is excluded, not guessed.

Within one batch, source order is normalized by `(root-relative path, event class, stable event id)`. `none` preserves every eligible item in that order. `last_per_path` deterministically retains the last item per path after source ordering and records every represented input. Items beyond the record maximum become capacity-refused evidence; no earlier item is evicted.

### D1B.10 — batch, event, disposition, and delivery are durable projections

Schema adds:

```text
watch_event_batches
watch_events
watch_event_delivery_dispositions
watch_event_deliveries
watch_scope_sequence_counters
watch_scope_observed_subjects
```

The batch id is deterministic from `(watch_scope_id, revision, sequence, parent_call_id)`. A resident-wide `WatchCoordinator` serializes sequence reservation per scope. It appends a batch-open/source-start fact before exposing the preopen, then projects the unique sequence. Crash after append is reconciled from ledger before another sequence is assigned.

Each synchronously admitted message contributes one event and disposition to the invocation-local set. After deterministic normalization, the complete set crosses the D1B.7-A.1 batch durability barrier before any nested call executes. Batch counts are projections derived from immutable event/disposition rows and sealed at parent call completion. Failed batch-seal append prevents a delivered claim even if an already started external effect cannot be undone.

`WatchEventEvidence.causative_call_id` always names the watcher call. `causative_trigger_firing_id` is present only when the local parent ingress context is Trigger and matches the exact firing/record/revision. Manual, UDS, CLI, or other WasmHost parents leave it null. `causative_adapter_observation_id` remains null for this Child path.

Disposition is exactly `fired`, `coalesced`, `suppressed`, or `capacity_refused` and describes only the state before a target call. A fired disposition names planned call id. Delivery completion names the target call's existing `MctResult`/result reference and `ResultRecorded` observation. There is no delivery success/denied/failed/timed-out/cancelled enum.

“Delivered” is projected true only when the referenced target `MctResult.outcome` is `success` and its result observation is durable.

### D1B.11 — existing observation kinds compose the watch flow

No `ObservationKind` variant is added.

| Watch/delivery fact | Existing kind | Plane / outcome |
|---|---|---|
| scope + grant create/revoke | `OperatorActionRecorded`, `ToyGrantAllowed`, `ToyGrantRevoked` | Operator/Kernel |
| watch preopen/scan bracket | `ToyCallStarted`, `ToyCallCompleted`, `ToyCallFailed` | Toy |
| batch/event eligibility | `DataMovementAllowed` | Kernel / allowed |
| exclusion, ABI mismatch, suppression, capacity | `DataMovementDenied` | Kernel / denied |
| child call-out construction | `CallConstructed` | Adapter / allowed |
| target call law | existing call/route/runtime/Toy kinds | existing |
| target completion | `ResultRecorded` | Kernel / existing result outcome |

Canonical bounded details carry the Allium entity fields and observation references; no payload bytes, content hashes beyond optional event SHA, absolute host paths, keyvalue values, or unbounded adapter errors enter the ledger.

If this table cannot truthfully express a required fact, implementation stops for an operator decision before changing the enum.

### D1B.12 — source admission precedes buffered delivery

Under D1B.7-A, the synchronous Wasmtime send bridge performs every admission check and buffers only typed admitted events. It neither runs a nested call nor claims durable admission while the watcher export is still executing. After export return, the resident uses the canonical `ResidentLedgerWriter`; it creates no side ledger. No nested `execute_resident_call` begins until the admitted set has crossed the operator-ruled D1B.7-A.1 durability barrier. Append failure suppresses every nested call and delivered claim.

Watch grant/scope decisions and the read-only preopen remain independently authorized. The buffered bridge carries no authority token except the non-clone capabilities already minted by kernel evaluation. Adapters still perform effects; the kernel still decides.

**D1B.7-A.1 — ratified 2026-07-21:** event/admission facts are buffered with the admitted set and appended durably as one normalized batch before any nested delivery begins. The watcher is a mid-execution Child and the parent call is the atomic evidenced unit; an invocation-local admitted offset is not an external durability acknowledgement. Append failure suppresses every nested call and delivered claim.

### D1B.13 — Mother-side event-source adapter remains deferred

The already-ratified Mother observer shape is not needed for either fixture and remains the Part A named slot `MotherEventSourceAdapterRuntime`. This implementation contains no native filesystem watcher task, event-source registration, adapter-trigger lookup, or `TriggerFiring` event call.

The Watch Toy path cannot be reused as a Mother adapter because it requires an acting authorized Child invocation. Conversely, a future Mother adapter cannot impersonate this Child or use its grants.

### D1B.14 — fixture artifacts follow the D1.13 acquisition pattern

Implementation adds:

```text
crates/mct-daemon/tests/fixtures/folder-watch-actor-0.1.0/
  child.toml
  folder-watch-actor.wasm
  MCT-REBUILD.patch
  PROVENANCE.md

crates/mct-daemon/tests/fixtures/watch-null-sink-0.1.0/
  child.toml
  watch-null-sink.wasm
  PROVENANCE.md
```

A committed build script:

1. verifies the read-only upstream repository contains exact commit
   `526dbf123b040198cb4395c1a63cf498a28ff915` and both 0.1.0 tags;
2. uses `git archive` into a temporary directory, never a mutable worktree;
3. applies the reviewed folder-watcher patch with exact zero fuzz;
4. runs `cargo component build --release -p patina-ai-child-folder-watch-actor` and
   `cargo component build --release -p patina-ai-child-watch-null-sink`;
5. verifies manifest package/version/export/import expectations; and
6. prints sizes, SHA-256, and BLAKE3 for provenance.

Each `PROVENANCE.md` records repository URL, exact commit/tag, Rust and cargo-component versions, exact command, manifest/component hashes and sizes, WIT import/export inventory, and sidecar absence. Folder provenance additionally records patch SHA-256/BLAKE3 and an exact statement that output is a source-derived MCT security rebuild. Sink provenance states unmodified exact-tag source.

Raw fixture directories contain no `.sha256` sidecars. Tests copy only manifest/component to isolated read-only source roots and call the existing operator-pointed `artifacts stage`; MCT creates canonical package sidecars and acquisition evidence. The quarry checkout, installed plugins, and real user paths are never test dependencies.

### D1B.15 — the full proof uses exact fixture responsibilities

Setup exactly approves and assigns both acquired artifacts. The watcher receives:

- current Watch scope + Watch ToyGrant;
- separate read-only directory ToyGrant;
- separate keyvalue ToyGrant for bucket `default`;
- its own logging and measure grants; and
- no trigger-management, sink, network, write, acquisition, or registry authority.

The sink receives only its own logging and measure grants. It receives no Watch, filesystem, keyvalue, trigger, or acquisition authority.

Before the temporal trigger is active, an ordinary configured call sets the watcher's stream to `patina:watch/events@0.1.0.emit`, recursive scope, and `emit_existing_on_start=true` using the exact typed control ABI. That manual configure call carries no trigger lineage.

The trigger then targets `patina:watch/control@0.1.0.scan-now` with static `[]`. The accepted proof is exactly:

```text
temporal TriggerFiring
  -> approved/assigned folder-watch-actor@0.1.0
  -> current Watch Toy scope/preopen and separate effects
  -> prepared filesystem change becomes safe event
  -> wasi:messaging host call-out
  -> ordinary MctCall origin=WasmHost
  -> patina:watch/events@0.1.x equality validation
  -> approved/assigned watch-null-sink@0.1.0.emit
  -> sink's own logging/measure grants
  -> existing successful MctResult
```

No step transfers authority to the next.

## Persistence constraints

At minimum, schema and Rust validators enforce:

- one current Watch scope revision and exact observer artifact/assignment;
- explicit non-empty event class set and ordered validity;
- `1 <= max_events_per_batch <= 128`;
- closed traversal/coalescing/state/compatibility/disposition enums;
- unique per-scope sequence and batch identity;
- event position unique within batch;
- no event path that is absolute, noncanonical, escaping, or absent from current/prior safe subject evidence;
- compatibility `matched` only when both legacy slots are identical;
- `mismatch_refused` cannot have planned/target call id;
- fired disposition must have planned call id;
- delivery row must reference fired disposition, exact target call, and existing result reference;
- delivered projection only for existing successful result;
- no Child-path event with Mother adapter evidence;
- no trigger firing link unless the actual parent ingress context is Trigger;
- no keyvalue value, message payload, absolute host path, or source content bytes in observation detail; and
- no new tables/fields for registry-sync or network acquisition composition.

## Failing-test-first implementation order

1. Add provenance/build scripts and red fixture-inspection tests; capture the exact upstream mismatch that motivates the patch.
2. Add red kernel Watch scope validation and Watch+ToyGrant composition tests.
3. Add red schema migration/projection/sequence tests.
4. Implement Watch scope types, non-clone session capability, state tables, and authenticated grant/revoke surfaces.
5. Add red read-only-directory, keyvalue, and reusable observability Toy tests before adapters.
6. Implement the three supporting canonical contracts using shared ToyGrant evaluation and bounded adapters.
7. Add red Wasmtime fixture-instantiation tests and implement only the exact used/pruned WIT imports.
8. Add red synchronous append-before-preopen and append-before-call-out tests; refactor host preparation into a before-effect bridge.
9. Add red child-call-out identity/depth/ordinary-call-law tests and implement the messaging producer bridge.
10. Add red safe path, symlink escape, deleted-subject, coalescing, batch-capacity, and sequence tests.
11. Add red legacy match/mismatch tests, then implement exact 0.1.x validation before call construction.
12. Add event/disposition/delivery projections and truthful lineage/result tests.
13. Add the full red supervised integration test by extending/reusing `supervisor_lifecycle.rs` harness helpers.
14. Add restart, trigger revocation, Watch revocation, pending/terminal reconstruction, and no-double-delivery assertions.
15. Add every D1B Track 3 row and only then update TODO/runbook in a docs-only close-out commit.

Every implementation commit is one concern and must pass:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

No implementation commit may modify ratified Allium.

## Required Integration Proof

The landed test
`supervised_trigger_watch_delivery_fixtures_execute_end_to_end` must cite each step by exact file:line in close-out:

1. Read both committed raw fixture directories; assert manifests identify exact names/versions, component imports/exports match provenance, hashes/sizes match `PROVENANCE.md`, the folder patch digest/diff classification matches, sink is unmodified exact-tag source, and neither raw directory has source `.sha256` sidecars.
2. Create isolated service root, config, identity, children/catalog, state, ledger, UDS, logs, executable, watcher root, keyvalue state, and fake-launchd paths; install/start the fake supervised resident and await real UDS readiness.
3. Stage each raw fixture through owner-authenticated `/artifacts/stage`; prove independent operator-pointed decision, adapter, verification, immutable package, and acquisition-backed artifact evidence, with source bytes/modes unchanged.
4. Exactly approve and assign both digest-addressed artifacts; prove no trigger, Watch scope, ToyGrant, call, or event evidence was implied by acquisition/approval.
5. Issue the watcher Watch scope/grant, equal-root read-only-directory grant, `default` keyvalue grant, and watcher logging/measure grants as distinct facts; issue only sink logging/measure grants. Prove exact artifact/assignment/Node/Vision/resource/revision scope.
6. Submit an ordinary typed watcher `configure` call setting the target operation stream and `emit_existing_on_start=true`; prove `origin` is the truthful manual ingress origin and no trigger firing id is attached.
7. Create a temporal trigger record targeting watcher `scan-now` with omitted policies; prove `skip`/`refuse` defaults, static CAS payload, exact caller/target/source/validity, and durable authority before activation.
8. Prepare one in-scope regular file, advance the injected scheduler clock, and prove firing uses `TriggerFiring`, exact record/revision/occurrence identity, and durable firing evidence before watcher call execution.
9. Prove Watch scope and Watch Toy evaluation are durable before `/input` preopen; content read and keyvalue host effects cite their separate grants; scan evidence contains no absolute host path or content bytes.
10. Prove one batch uses the exact scope revision and next monotonic sequence; its event is created, canonical root-relative, nonescaping, countable, and causally linked to the watcher call and exact trigger firing.
11. Prove the source-derived watcher emitted equal `absolute-path` and `relative-path`; the composed boundary recorded `compatibility_validation=matched` before constructing the target call.
12. Prove the target is a fresh ordinary call with `origin=WasmHost`, deterministic call-out id/key, parent watcher causal reference, and no trigger authority carried into target call authority.
13. Prove watch-null-sink receives exact `patina:watch/events@0.1.0.emit`, exercises only its own logging/measure grants, and returns an existing durable successful `MctResult`; only then does delivery project as delivered.
14. Reopen the validated ledger and SQLite while resident remains supervised; correlate trigger authority/occurrence/firing, Watch scope/grants/source effect, batch/event/disposition, parent and target calls, ABI validation, sink Toys, result, acquisition, approval, and assignment references.
15. Revoke the trigger record, advance one nominal occurrence, and prove one terminal suppression with no watcher call, Watch effect, call-out, or sink effect.
16. Create a fresh active trigger revision for the recovery portion, admit one pending/active occurrence, stop at the named append/project or firing/result seam, restart the supervised resident, and prove ledger reconciliation preserves deterministic identity/order and does not double-fire or deliver.
17. Revoke only the Watch scope/grant, advance another occurrence, and prove the trigger call may reach watcher authority but observation denies before preopen, source scan, event, call-out, or sink effect; content/keyvalue/observability grants cannot compensate.
18. Stop/reopen/restart once more; prove batch sequence never regresses, prior terminal dispositions remain terminal, no call/event is duplicated, and revocations remain current.
19. Stop and uninstall fake supervision; prove fixture packages, acquisition/provenance, trigger/watch/toy authority, events/deliveries/results, keyvalue state, identity, and ledger are preserved while only supervisor policy/current record is removed.
20. Prove the test never reads after fixture copy from or mutates the quarry checkout, `~/.patina`, installed plugins, real `~/Library/LaunchAgents`, machine launchd, or a network source.

Uncited “matched” claims are rejected. Close-out must include this 20-row proof diff with exact assertions and any difference stated explicitly.

## Additional required failure proofs

Named tests must prove:

### Watch authority and metadata

- missing, malformed, implicit-broad, empty-event, invalid-batch, invalid-window, stale-revision, revoked, expired, superseded, wrong-child/artifact/assignment/root, unobserved, and ledger-digest-mismatched scopes deny before preopen;
- Watch ToyGrant without scope and scope without Watch ToyGrant each grant nothing;
- directory-read or keyvalue grant cannot substitute for Watch grant;
- Watch grant cannot read content, write, delete, create, rename, access keyvalue, or originate a call;
- content-read grant is required independently and is read-only/root-bounded;
- keyvalue grant is exact-bucket/artifact/assignment bounded, refuses oversize/capacity without eviction, and stores no values in ledger;
- an explicit `root_only` request is refused as unsupported before preopen and is never silently widened to recursive traversal;
- created/modified/deleted classes deny when absent;
- hidden/filter/diff meaning remains Child behavior rather than a Mother schedule rule;
- existing symlink escapes, nested escapes, replacement races, special files, and path normalization ambiguity are excluded without escaped/absolute path disclosure;
- deleted events require prior durable in-scope subject identity;
- `none` and `last_per_path` coalescing are deterministic and countable;
- event batch maximum and global message bound refuse excess without eviction; and
- per-scope sequence remains monotonic across concurrency, crash after append, and restart.

### Child call-out, ABI, and result

- messaging connect rejects non-operation names and creates no subscription/authority;
- unsupported topic/content type, oversize body/metadata, malformed JSON, wrong event class, and path mismatch are durable pre-call refusals;
- ABI absolute/relative mismatch creates no target call and is not sanitized;
- equality narrowing dispatches only for exact 0.1.x and successor schema rejects the deprecated slot;
- target child absent, unapproved, unassigned, not ready, wrong operation, revoked, stale policy, or missing sink grant fails under existing call/Toy law;
- Watch authority and parent Child authority cannot authorize the sink;
- one event replay reuses call-out idempotency while distinct events do not;
- nested call-out depth greater than one refuses without recursion;
- append failure before event/disposition suppresses nested call; append failure after sink effect suppresses acknowledgement/delivered claim;
- success, denied, failed, timed-out, and cancelled calls reference existing `MctResult` outcomes and add no parallel delivery outcome;
- manual watcher invocation has parent call lineage and null trigger firing; trigger invocation names the exact actual firing; and
- no caller/body/message can fabricate trigger, Watch, scope, or sink-grant lineage.

### Fixtures and composed recovery

- fixture metadata fails on wrong upstream commit/tag, patch digest, build command, import/export inventory, byte size, SHA-256, BLAKE3, sidecar presence, or manifest version;
- an unpatched 0.1.0 watcher event is refused by the ABI mismatch test, documenting why source-derived rebuild is required;
- acquisition, verification, approval, assignment, Watch scope, Toy grants, trigger authority, call-out, and sink result remain separate facts;
- trigger missed-fire policies all three, overlap policies all three, and all three trigger capacity bounds remain covered by D1A named tests in the full phase transcript;
- evaluate-crash-re-evaluate cannot double-fire or double-deliver;
- terminal gating survives restart;
- revoking trigger suppresses next occurrence with evidence;
- revoking Watch denies before observation even when every other grant remains active; and
- Mother event adapter, registry-sync composition, network acquisition, JVM, real launchd, and paused epics remain absent.

## Track 3 attribution gate

Part B adds structural obligation rows for:

- `WatchObservationScope` and projection;
- `WatchEventBatchEvidence` and projection;
- `WatchEventEvidence` and projection;
- `WatchEventDeliveryDisposition` and projection; and
- `WatchEventDeliveryEvidence` and projection.

It dispositions every invariant in:

### `MctEventSourcePlacement`

- `OneObservationShapePerPath`;
- `DirectChildObservationRequiresWatchToy`;
- `MotherObservationRequiresIndependentEffectAuthority` — `DEFERRED` only under `MotherEventSourceAdapterRuntime`;
- `MotherAdapterCannotImpersonateChild`;
- `WatchToyGrantsObservationOnly`;
- `ChildEmissionReentersCallLaw`; and
- `TriggerAuthorityIsSoleStandingOrigination`.

### `MctWatchObservationScope`

- `RootAndBreadthAreExplicit`;
- `TraversalIsExplicit`;
- `EventClassesAreExplicit`;
- `BatchIsBoundedByNamedCeiling`;
- `CoalescingIsExplicitDeterministicAndCountable`;
- `ValidityIsCurrentAndBounded`;
- `WatchAuthorityIsObservationOnly`;
- `SafeMetadataIsCanonicalAndRootRelative`;
- `EscapedSubjectsAreExcluded`;
- `DeliveryCarriesExactScopeProvenance`; and
- `BatchSequenceIsMonotonicPerScope`.

### `MctLegacyWatchEventsCompatibility`

- `LegacyAbsolutePathSlotIsNarrowed`;
- `MismatchIsRefusedWithEvidence`;
- `NarrowingIsLegacyAbiOnly`; and
- `SuccessorDropsDeprecatedSlot`.

### `MctWatchEventDelivery`

- `DeliveryPathsAreExclusive`;
- `ChildDeliveryIsOrdinaryCurrentCall`;
- `WasmFixtureUsesTruthfulWasmHostOrigin`;
- `MotherDeliveryRequiresTriggerAuthority` — `DEFERRED` only under `MotherEventSourceAdapterRuntime`;
- `TriggerLineageIsNeverFabricated`;
- `BatchEvidenceIsCompleteAndScoped`;
- `EventEvidenceIsSafeAndCausal`;
- `DurableReceiptAndEligibilityPrecedeDelivery`;
- `PreCallDispositionSetIsClosed`;
- `PostCallOutcomeReusesMctResult`;
- `DeliveredMeansDurableTargetSuccess`; and
- `SinkEffectsRequireSinkGrants`.

### `PatinaWatcherQuarryDisposition`

- `GenericTriggerAndDeliveryBecomeMctProduct`;
- `WatchAndCallAuthorityRemainKernel`;
- `WatchApplicationMeaningRemainsChildMeaning`;
- `LegacyAbsolutePathSemanticsAreRejected`;
- `LegacyAbiShapeRequiresValidatedNarrowing`; and
- `DeprecatedSlotCannotPropagate`.

The new read-only directory, keyvalue, logging, and measure contracts receive explicit ToyGrant/adapter rows and named tests. No invariant receives an implicit waiver.

## Task 3 docs close-out

Only after all proof and attribution gates pass, one docs-only commit updates:

- `layer/surface/build/product/MCT-NEXT-BUILD-TODO.md` — mark fixtures two/three and Replacement Slice 4 complete, mark only the read-only directory Toy row completed, and retain read-write/blob/network/JVM work.
- `layer/surface/build/product/MOTHER-REPLACEMENT-RUNBOOK.md` — add trigger and Watch grant/configuration/inspection commands and the exact three-fixture proof statement.

The statement must be:

> All three external compatibility fixtures are proven under `mctMother`: Slate through supervised acquisition/execution, and the source-derived watcher plus exact sink through temporal trigger, scoped Watch Toy, ordinary child call-out, and validated legacy ABI. Full `patinaMother` replacement is still not claimed until the paused release-hardening and interface-launcher-control epics resume and close under TODO item 7.

Do not mark the paused epics complete or resume them in this phase.

## Verification and phase close-out contract

Close-out is reconstructed from disk and includes:

- expected starting tree and full D1A+D1B commit range;
- first red-test transcripts for both parts;
- every per-commit validation result;
- final standing checks:

```bash
allium check layer/allium
allium analyse layer/allium
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

- `--nocapture` transcripts for the primary integration and every named policy, capacity, crash, terminal, escape, ABI, lineage, and fixture test;
- a flake log containing the first failure verbatim before one rerun, or exactly `none`;
- the 20-row proof diff with exact test file:line citations;
- every Track 3 row or named operator-approved waiver;
- exact fixture repository/commit/tag/build/patch/toolchain/size/SHA-256/BLAKE3 values;
- proof fixture source directories contain no sidecars and tests use acquisition staging;
- proof `git diff 20941a4 -- layer/allium` is empty;
- proof no `ObservationKind` was added;
- docs-only close-out commit; and
- the truthful three-fixture statement with paused final gates still named.

## Build readiness

**D1B is ratified but sequenced after Part A.** The source-derived
`folder-watch-actor@0.1.0` fixture must carry a readable committed patch beside
provenance, name its patch digest, and be plainly labeled an MCT security
rebuild in the replacement statement. `watch-null-sink@0.1.0` remains an
unmodified exact-tag build. Part B implementation, fixture binaries, Track 3
rows, and product close-out edits must not begin until ratified Part A lands.

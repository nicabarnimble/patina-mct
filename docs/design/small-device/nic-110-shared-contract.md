# NIC-110 — shared small-device Child contract

**Status: draft for Doug's review; design only, not implementation authority.**
This is the shared dependency for [NIC-111–116](#8-sibling-design-agreement),
under [NIC-109](https://linear.app/nicabar/issue/NIC-109). Parallel designs may
use this draft now; reconcile at review. Approval gates **build**, not design.
The companion [WIT package](wit/shared.wit) is
`patina:small-device@0.1.0-draft`, outside executable crates and manifests.

Target: Raspberry Pi 3, approximately 905 MB RAM, aarch64 Debian; six Pi 3
Mothers plus nicamaster for fan-out. Fleet membership and machine paths are
configuration, not constants in Children. nicamaster is the designated build
host, not a new authority above other Mothers. Agents use Pis as hands on
hardware, not places to think: no inference, Git or heavy builds on Pi 3s.
Git Children remain macOS-only on Studio/Lume. No MCU Mother is proposed.

Design work uses Astra; after Doug approves, implementation belongs on Sol
high/xhigh. This change contains no Rust, build, deployment, fleet command,
new grant, or change to `main`.

The WIT is a reusable schema package, not a new Mother endpoint, Toy catalog
entry or replacement ALPN. Siblings publish versioned operation interfaces;
any adapter from today's string-returning `inspect` must explicitly validate
and project its data. Do not silently change existing peer wire contracts.

## 1. Fit to the current repository

Baseline inspected: `bc48d1e`. Distinguish built behavior from this proposal:

| Source | Existing contract | NIC-110 extension, not yet built |
| --- | --- | --- |
| [First real application](../../plans/first-real-app.md), [repo-report WIT](../../../crates/mct-repo-report/wit/world.wit) | Bounded immutable-input read/compute through directory-read; `inspect` currently returns strings. Durable application result is separate from Mother's 12-minute dedupe cache. | Shared typed envelope, durable job identity and receipt; not shell/stdout parsing. |
| [NIC-104 locked decisions](https://linear.app/nicabar/issue/NIC-104) | Canonical bounded operation: pure / read-compute default / opt-in stateful-event. Separate artifact, approval, assignment and instance. Membership is not admin or Toy authority; revocation fails closed on new admission, not exactly-once. | These three profiles are the common vocabulary here. The `mct-vision-child` consult/full NIC-104 plan is not present under `docs/` at this baseline; the issue's locked decisions were read directly. Do not invent an on-disk plan or override those decisions. |
| [wasm.rs](../../../crates/mct-daemon/src/wasm.rs) | Default `DEFAULT_WASM_MEMORY_LIMIT_BYTES` is 64 MiB; `StoreLimitsBuilder.memory_size` limits each linear memory, not total host RSS. Epoch deadlines exist. `compile_snapshot` calls `Component::from_binary`. | Lower per-operation limits, aggregate memory/host admission and trusted precompiled loading. Pi 3 no-compile is **not** satisfied today. |
| [wasm/budget.rs](../../../crates/mct-daemon/src/wasm/budget.rs) | Sticky host-call, logging, measure, Toy, observation, cumulative-byte, host-value and output limits. Host value accounting includes containers; defaults are not a device profile. | Declarative effective budgets and measured usage in receipts. |
| [kernel Toy/ToyGrant](../../../crates/mct-kernel/src/toy.rs), [Toy adapters](../../../crates/mct-daemon/src/toy.rs), [supporting Toys](../../../crates/mct-daemon/src/daemon/watch.rs) | Exact artifact/subject, Vision/resource/action and revision checks; non-cloneable `AuthorizedToyCall`; fresh authority at effect admission. Directory read, keyvalue, logging and measure exist; native Git fails closed on Linux. | No sensor, serial, journal or service-action Toy is admitted by this document. A WIT import or manifest `needs` is not a grant. |
| [MctResult](../../../crates/mct-kernel/src/call/mod.rs) | Terminal outcomes: success, denied, failed, timed_out, cancelled; caller-safe payload and audit reference. | Preserve those outcomes; add a versioned payload contract, not new kernel success semantics. |
| [observation ledger](../../../crates/mct-observation/src/lib.rs), [D-32–35](../../DECISIONS.md#d-32-restart-recovery-settles-it-never-resumes) | Hash-chained ledger, append-before-effect law, torn-tail evidence, interrupted-call settlement; no restart re-execution. Hash chain is not a signed receipt per application call. | Signed terminal projection and bounded durable outbox. Do not call hello admission receipts or admin signatures application receipts. |
| [Durable task proposal](../durable-task-boundary.md) | Task/invocation/effect/checkpoint are distinct; proposal, not a deployed journal API. | Bounded event steps and conservative unknown-effect handling, without silently implementing general continuation. |

Mother decides, Children compute, Toys effect. This design changes neither
single-hop forwarding (D-37) nor local sovereignty. No general graph scheduler,
write-capable cross-Vision sharing, guardian recovery, or admin-key cutover is
smuggled into the shared contract.

## 2. Profiles, identity and result envelope

### Profiles describe operations, not permission

| Profile | Contract | Examples |
| --- | --- | --- |
| `pure` | Bounded computation over supplied immutable data; no external I/O, clocks or randomness. Identical pinned inputs/schema/artifact have deterministic semantic output. | Diff, threshold transition, normalization. |
| `read-compute` | Bounded authorized read, then compute; default. A fresh read is a new sample, not replay of an earlier observation. | Health, log summary, config inventory, sensor sample. |
| `stateful-event` | Opt-in durable application state across **bounded invocations**; persisted checkpoint/event cursor, no always-resident guest or infinite loop. Can host an explicitly declared guarded action, but profile alone never grants mutation. | Threshold watcher; guarded service-action workflow. |

Classification is per exported operation. A Child may export a read-compute
`plan` and stateful-event `act`. A pure reducer can be a separate component or
library composed into another Child; composition does not inherit grants. The
host may schedule subscriptions, but every step has new admission, a deadline
and a receipt. No agent polling loop is needed for a watcher.

### Identity and submission

- `job-id`: logical caller request, including a fleet request when applicable.
- `task-id`: application state/workflow identity; for a one-shot call it may
  equal `job-id`. A watcher retains it across samples and boots.
- `call-id`: one admitted invocation. Retransmission uses the same ID and
  request digest. An explicit new attempt gets a new call ID, increments
  `attempt`, and retains task/job identity; retry is not automatic permission.
- `parent-call-id`: optional composition correlation; it grants nothing.
- Effect identity is `(task-id, logical-effect-id)` in the sibling action
  schema. Event identity is `(task-id, event-id)` with a monotonic source cursor.
  Neither is replaced just because a process or invocation changed.

Deduplication is scoped by authenticated caller, Vision, target node and call
ID. Same key/different request digest is `conflict` and never executes. The
request digest commits to the operation, pinned artifact, input schema and
exact canonical input bytes, task/job/parent IDs, caller/data scope, target,
requested budget and deadline. Transport retry counters are excluded. Mother
binds identity from authenticated admission, never trusts guest-supplied IDs.
Effective limits may only shrink; the envelope records what was enforced.

Queue acceptance is **not** a terminal result. WIT `submission-state` separates
`queued`, `running`, `settling`, `terminal`, `result-unavailable` and
`not-admitted`. `settling` means no further guest execution while durable
settlement/signing completes. `result-unavailable` preserves a retained call's
identity (and receipt when available), not a claim it was never admitted.
`queued-call` is an acknowledgment of durable storage, not a success receipt
or authority token.

### Two ownership boundaries

1. A sibling exports named, bounded operations with typed WIT inputs/results.
   It uses shared `coverage` plus its own typed body, or `child-answer` with a
   registered `typed-payload` schema. A schema defines the exact projection
   from typed WIT into canonical JSON or deterministic CBOR. Bytes without a
   known schema, stdout, arbitrary JSON-as-a-message and shell scraping fail
   validation. NIC-110 does not replace all operations with generic `execute`.
2. Mother validates that answer and creates `result-envelope`; the Child
   cannot choose authoritative identity, outcome, usage, audit reference,
   ledger position or signature. Trap, timeout, denial and failed decoding
   still produce Mother-owned terminal results when settlement is durable.

The envelope carries context (including classification, scope, exact artifact,
operation, request digest and budget), executing node/instance/boot identity,
terminal outcome/reason, effect certainty, optional typed answer, measured
usage and settlement time. `instance-id` is absent if no guest ran. Unknown
measurements are absent, never guessed as zero; counters for work not done are
zero. `linear-memory-high-water-bytes` is optional until measured reliably.
Preserve the invocation's `boot-id` across recovery; settlement in a later boot
does not pretend the guest ran there. If no guest ran, use the settling boot.
No secret, grant internals, raw device path or privileged denial detail appears
in caller-visible metadata. Visibility is checked before signing a projection.

Invariants:

- `success` has a schema-valid answer; `reason = none`. It means the operation
  completed, **not** that a sensor is healthy or every scanned item matched.
- A bounded useful answer can have `coverage = partial`, with examined count,
  omitted count if known, and optional opaque continuation. A signed partial
  answer is never silently upgraded to complete. Continuations bind the same
  caller/scope/source snapshot/schema, expire, and confer no authority.
- `denied`, `failed`, `timed-out`, `cancelled` have no answer in this draft.
  Bounded diagnostics remain in the ledger. This avoids treating partial
  writes or trap garbage as usable domain output.
- `effect = no-effect` means no external mutation was requested (reads are
  still Toy effects subject to authority). `not-started` proves a requested
  mutation never began; `confirmed` cites a durably recorded effect result;
  `unknown` means it may have occurred. Cancellation/timeout never means undo.
- Terminal reason codes are the draft WIT's closed safe vocabulary, not raw
  errors. `execution-interrupted` maps to existing interrupted-call behavior;
  `effect-unknown` additionally warns against action replay.
- A terminal envelope is immutable. Reacquisition returns the same bytes and
  receipt after current authorization to read them; it does not invoke a guest.

## 3. Signed ledger receipt per call

**Proposed v1:** the settling Mother signs with a purpose-bound Ed25519 receipt
key, privately held by Mother. Pin its public key to an enrolled Mother node
and ledger identity using existing owner-authorized trust establishment; never
trust a key delivered only inside its own receipt. Key provisioning/rotation
needs review before build. This is not a Child secret Toy, a D-58 admin lease,
a fresh fleet CA, or proof that an Iroh endpoint alone grants authority.

Every durably admitted call has one terminal envelope and signed receipt,
including denied, failed, cancelled, timed-out and recovery-interrupted calls.
A request too malformed to become an `MctCall`, or refused before durable
admission because storage/signing is unavailable, is `not-admitted`; no honest
system can promise a signed ledger receipt when its ledger cannot commit.
An unreachable node owes no fabricated receipt. A coordinator can sign its
own observation that no reply arrived, not assert the remote execution outcome.

### Durable ordering and signature bytes

1. Before admission reserve bounded result/outbox/receipt space and validate
   the signer is usable. Before dispatch or protected effect, durably record
   the required intent/authorization fact; an uncertain append poisons the
   writer and blocks further effects. Guest budgets cannot consume the host's
   reserved terminal-record space.
2. Validate and durably stage the exact canonical envelope bytes, then append
   a terminal observation binding their digest, call ID and request digest.
   The terminal entry uses the existing ledger hash algorithm (BLAKE3 over
   its typed serialization with `entry_hash` cleared); do not rehash it using
   the new receipt encoding or change historical ledger verification.
3. Construct `receipt-statement` naming version, `call-result`, call ID,
   envelope digest, signer node/key/scheme and exact terminal ledger position
   (ledger ID, sequence, observation ID, entry hash). Sign and durably store
   it with the result before publication. The receipt points to the terminal
   entry; the entry does not contain its own signature/hash, avoiding a cycle.
4. Crash after terminal append but before signing/outbox commit: reconstruct
   from the same durable bytes/entry and sign, **never rerun**. If bytes/key
   cannot be recovered, mark delivery blocked; do not invent the old result.
   Orphan staged bytes without a terminal entry are not successful results.

Draft canonical encoding `C(x)` uses RFC 8949 core deterministic CBOR: records
as maps keyed by exact WIT field names; enums as their kebab-case text; variants
as `[case-name, payload]` (null for a payloadless case); options as null/value;
integers as shortest unsigned integers, strings as UTF-8 without normalization,
`list<u8>` as byte strings, other lists as arrays. No floats, tags, duplicate
keys, indefinite lengths or unknown record fields. Lists retain order.
`digest(x) = BLAKE3-256(C(x))`, lowercase hex. Inner payload bytes remain byte
strings: canonical JSON is RFC 8785 (large integers represented by their
schema as strings); deterministic CBOR uses the same restrictions. Cross-tool
canonicalization vectors are a build gate, not an assertion of existing code.

Sign exactly:

```text
UTF8("patina:small-device:receipt:v1\0") || C(receipt-statement)
```

`contract-version` is `0.1.0-draft` for this draft; signature is exactly 64
bytes. Hash/key IDs are identifiers, not raw private keys. The envelope digest
covers all identity, outcome, payload, coverage, budgets and usage, not only
application output. `subject-id` must equal the call ID; signer node must equal
the settling/executor node. For fleet receipts it equals the job ID and the
signer is the coordinator. Receipts are audience-specific; filtering a signed
result later requires a separately signed projection, not byte substitution.

### Verification and limits of the claim

A verifier checks schema/bounds/canonical encoding, request binding, expected
node/key trust, subject digest and Ed25519 signature. It then checks the named
terminal entry against a verified local ledger or an authorized exported chain
segment anchored at a previously trusted head. Chain proof is fetched in
bounded pages, not duplicated into each result. Exports are audience-filtered;
when disclosure is disallowed, report **signature verified, ledger inclusion
not independently checked**, not full ledger verification.

A signature proves that Mother attested these bytes. It does not prove a sensor
was truthful, prevent a compromised Mother from lying, or alone detect ledger
rollback/forks. Remember verified heads, reject conflicting hashes at the same
position, preserve old verification keys and signed key-rotation evidence, and
quarantine inconsistent history. Ledger repair never rewrites issued receipts.
An expired signing key does not erase historical receipts; evaluating historical
validity uses its enrolled validity/compromise policy, not today's Toy grants.

## 4. Fleet fan-out: one answer, honest partial failure

A caller selects an authorized member set (up to seven for the first fleet),
operation/input pins and collection deadline. The coordinator freezes and
hashes `fanout-request` before dispatch. Each node receives its own call ID and
request digest, scoped through normal peer admission, assignment and ToyGrants.
The parent job is durable; per-node calls use existing single-hop forwarding.
No failure on one node rolls back success on another or grants a fallback route.

`receipted-fleet-result` is **one merged answer**: immutable request, revision,
coverage counts, one row per requested node, optional schema-defined domain
merge, and the coordinator's ledger receipt. Rows are sorted by node ID;
duplicate members/rows, omitted members or substituted request digests fail
validation. The default merge is the structured node-indexed union plus counts,
not an English string. It works for all six siblings without inventing a generic
arithmetic reducer. Domain merges must declare units, freshness, coverage and
a deterministic policy; never average missing sensor values as zero.

A row is either:

- `received`: original executor envelope and receipt, verified against the
  expected node, call, input, artifact, schema and scope; or
- `missing`: the coordinator's bounded observation (`not-dispatched`,
  `unreachable`, `reply-deadline`, `invalid-receipt`, `incompatible-result`),
  timestamp and `may-have-executed`. After uncertain send/drop this flag is
  true. Unverified bytes are quarantined, not counted as results.

`successful + unsuccessful + missing = requested members`. A received
`success` counts as successful even if its domain coverage is partial:

| Fleet status | Definition |
| --- | --- |
| `complete` | Every row is received success with complete coverage; domain merge, if requested, also complete. |
| `partial` | At least one usable success exists but the condition above fails. |
| `failed` | No usable success exists. A valid signed denial is evidence, not success. |

Example: seven members; four complete successes, one partial log summary, one
signed denial and one lost reply → successful 5, unsuccessful 1, missing 1,
status `partial`. Preserve the denial's receipt and mark the missing row
`may-have-executed = true`; do not pretend seven calls failed or all passed.

At the collection deadline, settle a snapshot. Late results are retained and
may yield revision N+1 with `supersedes = digest(previous fleet-envelope)` and
a new coordinator terminal observation/receipt; original revisions and node
receipts never change. A node's own execution deadline is distinct from the
collection cutoff. Parent `MctResult.success` means aggregation completed;
consumers must inspect `fleet-status` for workload success. Coordinator failure
reconstructs from durable child-call records, never blindly resends effects.

Fan-out coordination and structural union are Mother/application-client work,
not a new raw-network Toy in each Child. A pure domain reducer is optional,
separately bounded and admitted; on Pi 3 it receives only a bounded projection,
not seven full envelopes stuffed past the guest input cap. Larger reduction
belongs on nicamaster/Studio, still under normal data authority.

## 5. Budgets: declarations are not reservations or grants

Each operation declares minimum and requested WIT `budget` values in
artifact-bound metadata. Mother validates them before instantiation and chooses
the component-wise minimum of requested declaration, caller request and node
policy. If that falls below the operation's
required minimum, refuse admission rather than silently change its semantics.
All byte accounting is checked for overflow; truncation happens at schema item
boundaries with explicit coverage, never by cutting serialized bytes.

Proposed Pi 3 starting policy (unmeasured, to be qualified):

| Resource | Starting limit per invocation | Reason / current integration |
| --- | --- | --- |
| Aggregate guest linear memory | 16 MiB default; reviewed declarations up to 32 MiB | Well under 64 MiB. Sum **all** memories in a component; present per-memory limiter alone is insufficient. |
| Startup / total wall time | 2 s / 5 s default; reviewed operation at most 30 s | Startup is inside total wall time. Includes instantiate, host waits and compute; queue wait is bounded by absolute deadline. Epoch interruption handles guest code, not arbitrary blocked host I/O. |
| Input / application output | 32 KiB / 32 KiB | Validate before copying/lifting when possible; large snapshots are bounded/chunked reads, not giant arguments. |
| One host value / accumulated bytes | 16 KiB / 1 MiB | Match existing `host_value_bytes` / `accumulated_bytes`; charge container and serialization overhead as current code does. |
| Host / Toy / logging / measure calls | 256 / 64 / 16 / 64 | Map to the existing counters; no unbounded loop through a cheap import. |
| Observations / domain items | 130 / 256 | At most two runtime facts plus two per Toy within the count; extra facts reduce work or require a reviewed declaration. Bytes can exhaust first. |
| Signed terminal package | 48 KiB | Application output plus bounded host metadata/receipt; reserve separately from guest counters. Receipt alone at most 8 KiB. |
| Fleet package | 512 KiB, at most 7 rows | Coordinator's separate budget, not the guest's 32 KiB output allowance. Domain merge at most 32 KiB. |

The current runtime defaults (4 MiB accumulated, 64 KiB host value, 256 KiB
output, 4096 host calls, 1024 Toy calls, 2050 observations) are **not** inherited
by small-device operations. Maximum field sizes are also explicit: identifiers,
schema IDs, operation names and audit refs 256 UTF-8 bytes; opaque continuation
at most 1 KiB; digest 64 lowercase hex characters; signature 64 bytes. Reject
unknown schema versions, oversized lists and nesting before unbounded allocation.
Payload schemas limit nesting to 16 and bound every string/list. WIT itself
cannot express these limits; boundary validation is mandatory.

Start with **one active invocation per Pi 3**, including event steps, and
single-flight Toy I/O with bounded waits/cancellation. Stage admission so control
traffic, ledger settlement and delivery still have reserved memory/disk. Proposed
node policy keeps 256 MiB available to OS/Mother/control work and never relies
on swap for admission; measure idle/peak RSS, code mappings, ABI lifts, host
buffers and page-cache pressure before approving higher concurrency. There is
no claim that a 16 MiB linear-memory limit is a 16 MiB process RSS limit. Host
allocation failure must stop new work without losing durable facts.

Time budget exhaustion is `timed-out`; other resource exhaustion is
`failed/budget-exceeded`, with effect certainty independently reported. Every
new Toy must bound native I/O and cleanup; a guest deadline cannot cancel an
already admitted service restart. Fuel is not currently enabled and is not a
portable promise of milliseconds. Use actual monotonic elapsed time in a live
boot and conservative deadline validation after reboot/clock uncertainty.

## 6. Precompiled `.cwasm`: nicamaster builds, Pi 3 never compiles

This is a **new trusted-artifact pipeline**, not a file-extension convention.
Wasmtime precompiled deserialization admits native machine code and therefore
requires stronger trust than validating arbitrary Wasm. The existing
`from_binary` path must not be a fallback on Pi 3, even for discovery or a
cache miss.

Proposed pipeline, to implement only after approval:

1. On nicamaster, build the portable component and extract/validate WIT imports,
   exports, schema and budget declaration. Approve the exact portable artifact;
   keep artifact/approval/assignment/instance as separate facts.
2. Precompile **a component**, not a core module, using the exact deployed
   Wasmtime release/build (baseline pins 49.0.5), engine settings and Linux
   aarch64 target. Use the Pi 3 Cortex-A53/ARMv8-A feature baseline, not
   nicamaster's native CPU flags. Record compiler/backend, target triple,
   required CPU features, component model settings, epoch interruption and
   all engine compatibility settings in a reproducible fingerprint.
3. Publish a bounded signed derivation manifest binding portable `.wasm`
   digest, `.cwasm` digest/size, WIT/import-set digest, budget/schema digest,
   compiler/engine fingerprint and target requirements. The locally enrolled
   builder key is a distinct trust role from receipt keys. A checksum alone
   is integrity, not permission to load executable serialized code.
4. Mother stages immutable bytes privately, verifies builder signature and
   manifest, source approval/assignment, byte digest, size, architecture/CPU
   and exact engine compatibility **before deserialization**. Read/hash/load
   the same verified byte snapshot to avoid pathname replacement races.
   Initial artifact-transfer cap: 16 MiB per `.cwasm`; larger needs review.
5. Atomically activate a verified generation; retain last-known-good for
   explicit authorized rollback. Advertise capability only after precompiled
   inspection/instantiation succeeds. Missing/mismatched/untrusted artifacts
   yield `incompatible-artifact` or unavailable, never local compilation.
6. Runtime/Wasmtime upgrades require new compatible artifacts from nicamaster.
   Rollback cannot resurrect revoked approvals, assignments or ToyGrants.
   Offline nodes continue only with already-approved compatible bytes.

Pi 3 packages must omit/disable compiler paths, with a proof that all load,
discovery, validation and recovery routes obey that policy. The portable source
may be retained off-device for audit; device verification cannot secretly
compile it. Deserialization's unsafe trust boundary needs a separately reviewed
implementation, signature vectors, negative tests and actual Pi 3 CPU evidence.
No `.cwasm` files are generated or deployed by this design change.

## 7. Reboot, offline queue and handover

Mother owns a bounded durable inbox/outbox and replay index in configured
private state, not guest memory, `/tmp`, an agent transcript or the short-lived
call cache. Proposed implementation is a SQLite projection plus staged result
blobs, reconciled from the existing ledger by its single writer. Do not claim
one atomic transaction spans today's JSONL ledger and SQLite: staged data,
ledger facts and idempotent projection replay define the commit protocol.
A queued acknowledgment is sent only after durable payload staging and its
ledger acceptance fact; a terminal reply only after section 3's ordering.

| Durable state at reboot/drop | Required disposition |
| --- | --- |
| No accepted call/dispatch | No execution claim; caller may submit with normal admission. |
| Queued, provably never dispatched | Retain until deadline; any later first dispatch rechecks live authority, assignment, input and budget. Queue admission did not pre-authorize execution. |
| Dispatch/start may have occurred, no terminal | Settle once as interrupted; mutation certainty is unknown unless positive evidence proves otherwise. Do not resume the old invocation. |
| Effect intent, no effect result | Never auto-repeat a mutation, including with a fresh call ID. Reconcile against destination idempotency/status or require operator decision. |
| Result/terminal committed, receipt/outbox incomplete | Finish signing/projecting from exact durable bytes; no guest execution. |
| Receipt/result committed, connection drops | Retain and hand over the exact package later, with at-least-once delivery and durable receiver dedupe. |
| Torn tail / corrupt hash / uncertain append | Existing recovery/quarantine rules; fail closed, preserve evidence, do not repair by guessing successful work. |

An offline Mother may run **locally authorized** read/event work using a valid
local authority snapshot and unexpired grants. It cannot obtain new authority
from an absent coordinator or claim instant knowledge of remote revocations.
Any required live membership/policy check that is unavailable blocks new
admission. Untrusted wall time after reboot blocks time-sensitive authority;
a boot ID/monotonic counter is not a license to extend grant expiry. Queued
remote requests are inert data until full live revalidation. On reconnect,
result access and data egress are checked anew; revocation can block delivery
without erasing evidence of a historically authorized call.

### Bounds, retention and delivery

- Initial per-Mother queue reservation: at most 256 pending records **and**
  16 MiB total payload/metadata, whichever fills first, plus 1 MiB separately
  reserved for bounded settlement/overflow control facts. These are disk
  budgets, not permission to load the whole queue into RAM. Queue age limit
  defaults to 24 hours for unstarted calls; execution deadline wins if earlier.
- Reserve a terminal slot before accepting a call. Full queue, insufficient
  ledger disk reserve or unavailable signer blocks new work. Watchers pause
  at a checkpoint and record a gap/overflow fact; never silently drop threshold
  crossings. Coalescing requires a sibling-defined aggregate retaining the
  covered sequence range, count and loss semantics. Continued overload
  backpressures producers; the emergency reserve is not an infinite log.
- Unacknowledged terminal results do not expire silently. Retain them until
  durable ack or explicit owner-authorized discard recorded as a tombstone;
  fullness stops admission. After ack, proposed minimum result retention is
  seven days. Beyond it, retain call/request/effect identity tombstones so
  replay returns `result unavailable`, never fresh execution. Tombstone space
  is also bounded: capacity exhaustion stops admission, and compaction or
  retirement of old ID namespaces requires explicit reviewed policy.
- `handover-item` has a stable delivery ID, intended destination, exact signed
  result and optional event identity. Authenticate the peer, check Vision/data
  egress, send bounded pages and back off. Receiver verifies and durably stores
  the result plus dedupe key before authenticated `handover-ack` binding the
  delivery ID/destination/result digest. Lost ack means resend, not recompute.
  Acks are transport-authenticated, not trusted unsigned files supplied by a
  Child. Partial batches ack individually. Exactly-once agent wake is not
  promised: the consumer must durably deduplicate event IDs and reconcile
  an uncertain wake rather than blindly repeat it.
- Handover means **delivery/custody of evidence**, not transfer of Mother keys,
  grants, action authority or a running guest. A collector can forward an
  original receipt unchanged, but cannot re-sign as the original executor.
  Moving an event task to another Mother requires separate assignment,
  explicit checkpoint compatibility and old-owner fencing; automatic failover
  of mutable workflows is outside v1.

### Stateful-event checkpoints and guarded actions

A watcher invocation consumes a bounded source sequence range and committed
checkpoint, computes a transition, then requests grant-gated persistence.
The checkpoint includes state schema/artifact/input pins, last processed source
cursor, hysteresis/debounce state and pending event IDs. Checkpoint update and
event creation need one logical commit (ledger fact plus replayable projection),
so crash cannot advance the cursor while losing its event. Existing keyvalue
and messaging imports alone do **not** establish this transaction; an explicit
state/outbox capability or Mother transaction adapter is a build prerequisite.
No event-send or checkpoint-write API is invented as an existing Toy here.

After reboot, settle an interrupted invocation first. A later, independently
authorized step can consume the committed checkpoint with the same event IDs;
it is not D-32 recovery rerunning a call. Unknown source gaps, clock uncertainty
and artifact changes are visible, not silently replayed against new code.

For NIC-116, `plan` and `act` each receive their own signed receipt. Act binds
the exact plan digest, resource, action, preconditions, expiry and stable logical
effect ID. Revalidate authority and preconditions at the Toy boundary; stale
plan/revocation denies before mutation. Dry-run is not an executable grant.
Persist intent before the single action; ambiguous completion stops at unknown.
Even naturally idempotent-looking service restarts are not automatically safe
to repeat. NIC-110 supplies no general shell, rollback or exactly-once promise.

## 8. Sibling design agreement

Each sibling supplies its world/operation signatures, versioned input/output
schema, minimal profile per operation, new-vs-existing Toy inventory, exact
grant scope, memory/startup/IO budgets, bounded composition, restart/offline
semantics and a Pi 3 acceptance scenario. All reuse sections 2–7 rather than
forking receipt, profile, fan-out or AOT vocabulary.

| Issue | Shared needs / proposed composition | Authority or durability work to flag |
| --- | --- | --- |
| [NIC-111 health](https://linear.app/nicabar/issue/NIC-111) | Read-compute; typed health inventory → pure classification. Preserve per-probe unsupported/unavailable/stale, not shell text. | Host health/service-status interfaces need review; directory read alone is not blanket `/proc`, `/sys`, SD or service-manager authority. |
| [NIC-112 logs](https://linear.app/nicabar/issue/NIC-112) | Read-compute bounded journal/file page → pure deterministic summary; no LLM. Coverage, redaction, cursor and source window required. | Journal read is new; file read uses exact directory scope. Snapshot/cursor rotation cannot silently skip records; logs may contain secrets. |
| [NIC-113 config](https://linear.app/nicabar/issue/NIC-113) | Read-compute inventory + pure normalization/diff/preflight against pinned baseline. Natural smaller-component composition. | Read-only scope; changed input is a new digest, not permission to apply changes. No Git on Pi 3. |
| [NIC-114 watcher](https://linear.app/nicabar/issue/NIC-114) | Stateful-event; reuse health/sensor/log outputs + pure threshold reducer + durable state/event step. | Atomic checkpoint/outbox, source-gap policy and deduplicated wake delivery are new obligations, not an eternal guest loop. |
| [NIC-115 sensors](https://linear.app/nicabar/issue/NIC-115) | Read-compute collection + pure calibration/units. `source-ref` names gateway, resource and optional device/channel; `sample-origin` distinguishes sample time from Pi receipt time. | Sensor/GPIO and likely serial Toys are new. No raw device address, serial byte-stream or MCU-held Mother key in the Child. |
| [NIC-116 action](https://linear.app/nicabar/issue/NIC-116) | Read-compute plan/precondition + opt-in stateful-event act workflow; reuse health/config checks where useful. | New exact service-action Toy, bounded native effect, plan and act receipts, stable effect ID and unknown-outcome reconciliation. No ambient shell. |

NIC-115's sensor may be local, attached by serial, or behind an MCU running
WAMR/wasm3 or firmware. The gateway Mother resolves a logical resource under an
exact grant, bounds reads/timeouts, signs what it received and owns the ledger
and outbox. MCU identity/sequence/firmware evidence is data, not Mother authority.
The source schema must record stale/unknown readings and disconnected devices.
A sequence is scoped by `source-epoch` (device boot/session); a reset or missing
epoch is a visible discontinuity, not proof of replay or a new monotonic sample.
A Pi receipt does not certify physical accuracy or MCU authentication unless
separately established. The shared WIT assumes neither local sensor placement
nor Component Model support on the MCU. MCU firmware is not `.cwasm`; this
pipeline applies to the Pi-hosted gateway Child only.

## 9. Review and acceptance gates

### Design pass (this branch)

- [x] Contract and shared WIT cover structured result/receipt, fan-out, budgets,
  `.cwasm`, reboot/offline/handover and three profiles.
- [x] Current implementation versus proposed extension is explicit; six sibling
  needs, composition and MCU gateway boundary are accounted for.
- [x] Shared WIT parses as a package using installed `wasm-tools component wit`
  in text-only mode. This checks syntax/type references, **not** bindings,
  runtime support, cryptography or behavior.
- [x] Documentation links and whitespace checked locally; no Rust build or
  deployment is part of validation.
- Branch delivery: commit and push `nicabar-nic110-design`; no PR, no `main`
  changes. Git history/remote branch, not this checklist, proves delivery.

### Approval decisions before implementation

Recommended defaults above are concrete review proposals, not settled policy.
Doug and sibling reviewers must approve: receipt-key enrollment/rotation and
canonical byte format; signed-AOT trust/compatibility policy; aggregate memory
and host-I/O enforcement; queue/retention/tombstone limits; state/outbox atomic
boundary; new Toys and exact scopes; action reconciliation. Reconcile schema
and budget needs from all six drafts before freezing `0.1.0`. No sibling design
waits on those build approvals. A breaking draft change updates the package
version and all affected design references; never silently alter signed bytes.

### Later Pi 3 build acceptance (specified, not run)

| Scenario | Required evidence |
| --- | --- |
| Success, denial, trap, timeout, resource exhaustion | Typed terminal outcome, correct effect certainty, ledger position and verified signature; modifying any payload/context/budget breaks verification. No unsigned success. |
| Output/input/count limits and hostile nesting | Exact boundary accepted; boundary+1 rejected without unbounded allocation, truncated JSON or lost terminal evidence. |
| Pi 3 memory and startup | Actual idle/peak RSS and startup/step timings under load, aggregate memories bounded, no swap dependence, control/ledger service remains responsive. |
| AOT loading | Approved Pi-3-target `.cwasm` loads; wrong CPU/engine/build/signature/digest/oversize fails closed; missing cache or discovery never compiles, including offline/reboot. |
| Seven-node fan-out with failures | One stable sorted answer; 5/1/1 example gives partial; no-result case fails; bad receipt rejected, late reply creates a linked revision, no duplicate effect dispatch. |
| Power loss at every durable boundary | Crash before/after admission, intent, effect, result, terminal append, signing, projection and ack; ledger verifies or quarantines honestly, same call never silently reruns. |
| Offline/reconnect and full SD/queue | Replay exact signed bytes, deduplicate lost-ack deliveries, preserve event checkpoint/outbox consistency, stop admission on exhaustion, expose gaps, recheck expired/revoked authority. |
| Guarded action ambiguity | Plan receipt cannot authorize act; revoke/stale preconditions deny; crash after native effect yields unknown until reconciliation, never automatic second restart. |
| Gateway fixture | Same WIT handles local and simulated serial/MCU sample; timeout/stale/bad sequence explicit; only Pi Mother signs/holds grants. Real hardware ceremony remains manual. |

No hardware, runtime, cryptographic or performance gate above is claimed passed
by the design-only syntax and document checks.

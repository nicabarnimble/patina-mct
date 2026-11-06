# NIC-111 — Small-device health probe

**Status: proposed design; not implemented, measured, or approved for build.**
[Issue NIC-111](https://linear.app/nicabar/issue/NIC-111), under
[NIC-109](https://linear.app/nicabar/issue/NIC-109). Shared contract owner:
[NIC-110](https://linear.app/nicabar/issue/NIC-110).

## 1. Decision and scope

One bounded **read-compute** Child, `mct-health-probe`, answers: “What health
signals can this Mother observe for these exact resources now?” It reads typed
Toy samples, computes findings, and returns immutable structured data. It does
not run a shell, infer a diagnosis with a model, repair storage, restart a
service, poll forever, or retain state. Pi 3 Mothers (aarch64 Debian, about
905 MB RAM) are the first target. A Pi is a hand on local hardware, not a place
to think: no inference, Git, or heavy builds. Git Children remain macOS-only on
Studio/Lume.

| Required field | Design |
| --- | --- |
| WIT world | Proposed `patina:device-health@0.1.0`, world `health-probe`; exported `probe.inspect` below. |
| Inputs / outputs | Exact resource selectors and threshold policy → typed samples, per-field availability, findings, coverage; NIC-110 wraps the domain result and Mother's signed receipt. |
| Toys | **Four new** read-only interfaces: storage, temperature, throttling, service state. None is shipped by this baseline. |
| Memory / startup | 8 MiB guest ceiling; peak incremental resident-memory target ≤24 MiB; cold start p95 ≤500 ms from verified `.cwasm`; details in §5. All unmeasured targets. |
| Profile | `read-compute`; invocation-local memory; no lifecycle hooks or Child-owned outbox. |
| Composition | Smaller storage, thermal/power, service samplers + pure evaluator; NIC-114 consumes results for threshold events; NIC-116 may use them as evidence, not authorization. |
| Pi 3 acceptance | §9: useful physical samples, explicit unsupported SD telemetry, exact grants, bounded execution, verified receipts, seven-node partial failure, restart/offline delivery. Future tests only. |

**Non-goals:** arbitrary device discovery, raw file/log export, SMART self-tests,
SD write tests, fsck, service mutation, journald scanning (NIC-112), general GPIO
(NIC-115), a graph scheduler, multi-hop routing, or a Mother on an MCU. A normal
microSD card often exposes no wear estimate or reliable failure prediction.
“Unsupported” is a valid observation; absence of evidence is not healthy media.

## 2. Baseline and dependencies

Inspected repository baseline: `bc48d1e` (PR #108). Sources:

- [First application plan](../../plans/first-real-app.md) and
  [`mct-repo-report` WIT](../../../crates/mct-repo-report/wit/world.wit): bounded
  read/compute precedent, not authority to expose `/sys`, `/dev`, or systemd.
- Canonical consultation: `docs/mct-vision-child-plan.md`, read from local
  `nicabar-nic105-drift-report` at `f69b163e4d0d868315449772cd119a3955446504`
  (not present on this baseline branch). Its §B and
  [NIC-104's locked decisions](https://linear.app/nicabar/issue/NIC-104) establish
  WIT-only bounded operations, typed successor edges, separate artifact /
  approval / assignment / instance facts, and `needs` ≠ grant.
- [`wasm.rs`](../../../crates/mct-daemon/src/wasm.rs):
  `DEFAULT_WASM_MEMORY_LIMIT_BYTES` is 64 MiB; `store_limits` applies a linear
  memory limit, not a process RSS cap. `compile_snapshot` uses
  `Component::from_binary`; this is **not** an existing `.cwasm` load path.
  Epoch deadlines exist; they do not by themselves cancel blocking host I/O.
- [`wasm/budget.rs`](../../../crates/mct-daemon/src/wasm/budget.rs): default
  invocation ceilings are 4,096 host calls, 1,024 Toy calls, 2,050 observations,
  4 MiB accumulated bytes, 64 KiB host-value bytes, 256 KiB output. Charges include
  dynamic value/container overhead, not merely wire payload size.
- [`ToyGrant` and evaluation](../../../crates/mct-kernel/src/toy.rs),
  [`grant_check.rs`](../../../crates/mct-kernel/src/grant_check.rs),
  [Toy dispatch](../../../crates/mct-daemon/src/toy.rs), and
  [resident adapters](../../../crates/mct-daemon/src/daemon/resident/execution.rs):
  exact subject/resource/action, current authority revalidation, unknown Toy
  denial. Existing `authorize_resident_toy` selects one unambiguous resource
  from grants; it is **not** a per-request multi-resource selector. Health needs
  explicit per-resource admission, not an expanded wildcard on that helper.
- [D-32/D-33/D-36/D-37](../../DECISIONS.md): settle interrupted calls, never
  resume them; safe peer error vocabulary; single-hop forwarding.
  [Reliability doctrine](../../../layer/core/reliability-doctrine.md): canonical
  surviving ledger prefix, preserve torn tails, quarantine corruption.

NIC-110 is In Progress. During review its sibling working tree supplied
`docs/design/small-device/wit/shared.wit`, package
`patina:small-device@0.1.0-draft` (SHA-256
`14ff8bf1a9543edfaea6841da50ddd183601f53691391b505f5efae990ae7c94`). The companion
`nic-110-shared-contract.md` was also read when it appeared (SHA-256
`73da3218f06fba2b21cb7e4850879cce207c8a1472241cf5591a9096738842be`).
The shared file is not copied into this branch. Final NIC-110 reconciliation
gates build, not design. The domain interfaces below are **proposals**, not a
second shared ABI. Receipt schemas/signing remain entirely shared.

## 3. WIT world, inputs, outputs, and meaning

### Domain WIT draft

This is an inline design sketch, not a compiled WIT artifact. Interfaces in this
package are proposed canonical identities; NIC-110 owns shared transport types.
The export uses NIC-110's permitted typed-body-plus-coverage form. Mother binds
that answer into the shared result envelope; the Child neither signs receipts
nor imports a ledger.

```wit
package patina:device-health@0.1.0;

interface types {
    use patina:small-device/common@0.1.0-draft.{source-ref, sample-origin,
        coverage as shared-coverage};
    // Shared logical identity, never a host path, command, or peer address.
    type resource-ref = source-ref;
    enum source-kind { host-kernel, host-firmware, service-manager, gateway }
    record stamp {
        source: source-kind,
        origin: sample-origin,
        boot-id: string,
        start-monotonic-ms: u64,
        end-monotonic-ms: u64,
    }
    enum unavailable {
        unsupported, not-present, not-authorized, timed-out,
        io-error, malformed, stale, resource-changed,
    }
    enum media-kind { sd, emmc, other }
    enum media-condition { no-fault-reported, warning, failing }
    record volume {
        total-bytes: u64,
        free-bytes: u64,
        available-bytes: u64,
        total-inodes: option<u64>,
        available-inodes: option<u64>,
        read-only: bool,
    }
    record media {
        kind: media-kind,
        condition: result<media-condition, unavailable>,
        // Device-reported range/bucket, not an invented exact wear percentage.
        life-used-lower-bps: result<u16, unavailable>,
        life-used-upper-bps: result<u16, unavailable>,
    }
    record temperature { milli-celsius: s32 }
    record throttling {
        raw-mask: u32,
        under-voltage-now: bool,
        frequency-capped-now: bool,
        throttled-now: bool,
        soft-temperature-limit-now: bool,
        under-voltage-since-boot: bool,
        frequency-capped-since-boot: bool,
        throttled-since-boot: bool,
        soft-temperature-limit-since-boot: bool,
        unknown-mask: u32,
    }
    enum unit-state { active, inactive, failed, activating, deactivating, other }
    enum unit-load { loaded, not-found, masked, error, other }
    record service {
        load: unit-load,
        state: unit-state,
        sub-state: string,
    }
    variant value {
        volume(volume), media(media), temperature(temperature),
        throttling(throttling), service(service),
    }
    record sample {
        target: resource-ref,
        stamp: stamp,
        reading: result<value, unavailable>,
    }
    enum expected-service { active, inactive }
    record service-target { target: resource-ref, expected: expected-service }
    record policy {
        min-available-bytes: u64,
        min-available-bps: u16,
        min-available-inode-bps: u16,
        temperature-warning-mc: s32,
        temperature-critical-mc: s32,
    }
    record request {
        volumes: list<resource-ref>,
        media: list<resource-ref>,
        temperatures: list<resource-ref>,
        throttling: list<resource-ref>,
        services: list<service-target>,
        thresholds: policy,
        max-sample-age-ms: u32,
    }
    enum severity { info, warning, critical }
    enum finding-code {
        capacity-low, inodes-low, volume-read-only, media-warning,
        media-failing, temperature-high, under-voltage, frequency-capped,
        throttled, soft-temperature-limit, historical-power-event,
        service-mismatch, evidence-unavailable, unknown-device-flags,
    }
    // Index into the immutable ordered report.samples list; no repeated IDs.
    record finding { sample-index: u8, severity: severity, code: finding-code }
    enum health { ok, warning, critical, unknown }
    enum coverage { complete, partial, none }
    record report {
        samples: list<sample>,
        findings: list<finding>,
        health: health,
        coverage: coverage,
    }
    record answer { body: report, coverage: shared-coverage }
    enum request-error { invalid-selector, invalid-policy, too-large }
}

// All four imports are NEW Toys, subject to catalog admission and exact grants.
interface storage-read {
    use types.{resource-ref, sample};
    read-volume: func(target: resource-ref) -> sample;
    read-media: func(target: resource-ref) -> sample;
}
interface temperature-read {
    use types.{resource-ref, sample};
    read: func(target: resource-ref) -> sample;
}
interface throttling-read {
    use types.{resource-ref, sample};
    read: func(target: resource-ref) -> sample;
}
interface service-read {
    use types.{resource-ref, sample};
    read: func(target: resource-ref) -> sample;
}
interface probe {
    use types.{request, answer, request-error};
    inspect: func(input: request) -> result<answer, request-error>;
}
world health-probe {
    import storage-read;
    import temperature-read;
    import throttling-read;
    import service-read;
    export probe;
}
```

Callable identity: `patina:device-health/probe@0.1.0#inspect`. There are no
`init`, `tick`, Watch, filesystem, clock, random, process, network, keyvalue,
logging, or model imports. Shared type dependencies confer no host capability.
Host sampling provides time metadata.

### Projection into NIC-110

- Input schema: `patina:device-health/request@0.1.0`; payload schema:
  `patina:device-health/report@0.1.0`. Project `answer.body` into shared
  `typed-payload` with `encoding = canonical-json` (RFC 8785). Records use keys
  matching WIT field names; enums are strings; variants/results are two-element
  arrays `[case-name, payload]`; absent options are null; lists are arrays.
  Encode every `u64` as a canonical unsigned decimal string (no leading zeros,
  except `"0"`), and `u8/u16/u32/s32` as exact JSON integers. This preserves large
  counters and negative temperatures without extending NIC-110's unsigned
  outer-CBOR schema. Reject unknown fields, duplicate keys and schema-invalid
  numbers; bound nesting to 16. Apply the same mapping to the input request.
  NIC-110 owns the outer deterministic-CBOR digest/signature preimages.
- Shared `coverage.examined` counts returned sample slots (including unavailable
  slots); `omitted = some(0)` and `continuation = none`: this operation never
  paginates or silently drops requests. Shared completeness is complete only
  when domain coverage is complete; otherwise partial. Domain `none` expresses
  no usable evidence even if every requested slot was examined.
- `source-ref` supplies `gateway-node-id`, `resource-id`, optional `device-id`
  and `channel-id`. `sample-origin` supplies observed/received time, freshness,
  source epoch/sequence and evidence digest. Domain boot/monotonic metadata supplements these
  shared types. Returned `target` must equal `stamp.origin.source`. The adapter
  supplies provenance; a guest cannot turn a claimed origin into host evidence.
- Shared terminal outcomes stay closed: interrupted maps to `failed` with
  `execution-interrupted`; unavailable individual readings do not become new
  terminal outcomes. This read-only operation has `effect = no-effect` (audit
  writes are not domain mutations). Shared `reason` details must still respect
  the admitted peer disclosure policy, not widen D-36 automatically.

### Bounds and validation

- Maximum 4 volumes, 4 media devices, 4 temperatures, 4 throttling sources,
  16 services: **32 samples / at most 32 Toy calls**. At least one selector is
  required. Reject duplicates within a category before any reads.
- Resource/device/channel IDs and boot IDs are bounded to 96 UTF-8 bytes,
  gateway node IDs follow NIC-110's identity rules, service substate to 48;
  selectors use a provisioned opaque-ID alphabet, not path syntax or globbing.
  Reject mismatched returned identity, wrong value variant, invalid ranges,
  reversed times, or a changed device binding; never silently rebind a selector.
- Input wire encoding ≤8 KiB; output domain payload ≤24 KiB; ≤128 findings.
  Bound collections and values before host allocation/copying. If the accepted
  request's conservative maximum report size cannot fit (including repeated
  provenance and worst-case findings), reject `too-large` before any read.
  Thus the count ceilings are not a promise that 32 maximally long selectors fit.
  An adapter violating the admitted bounds causes shared resource-limit failure,
  never dropped evidence or a truncated “ok.”
- Percentages use basis points (0–10,000). Require warning temperature < critical
  temperature and fresh-sample limit 1–5,000 ms. A concrete example policy is
  512 MiB available, 10% available bytes/inodes, 75°C warning, 80°C critical.
  These are supplied operational thresholds, **not** universal hardware ratings
  or firmware throttle thresholds. Deployments choose their policy.
- Request bytes including thresholds and selector mapping revision are digest
  bound by the shared envelope. Device binding revision is host-attested receipt
  evidence, not a caller's authority assertion.

### Sample and finding semantics

A snapshot is **not atomic across resources**. Each sample carries the gateway
Mother's boot-scoped monotonic acquisition interval; NIC-110 supplies envelope
wall-clock/time-confidence metadata. `source-sequence` optionally identifies a
leaf reading. The host verifies freshness at delivery to the Child, including
cache/serial age. Boot IDs and monotonic times are not compared across Mothers.
A late offline delivery remains historical evidence, never silently “current.”

| Signal | Computation and honest limits |
| --- | --- |
| Volume | `available-bytes` uses filesystem `f_bavail`, not root-reserved free space. Warn when either supplied byte or ratio minimum is breached; handle zero capacity as malformed, and inode support absence as unavailable. A read-only volume is critical for a selector provisioned as expected-writable. Read-only expected volumes must be excluded from this v1 volume policy. |
| SD/media | Report kind and available kernel/device health evidence independently of capacity. Device warning/failing → corresponding finding. Wear bounds are optional evidence represented by typed unavailability, never zeros fabricated for SD. No-fault-reported means only that the reporting device saw no fault, not a remaining-life guarantee. |
| Temperature | Compare signed milli-Celsius to supplied thresholds using inclusive boundaries. Preserve negative values where the sensor permits them; validate against the provisioned sensor's documented range. |
| Throttling | Raspberry Pi firmware bits 0–3 describe current under-voltage, frequency cap, throttling, soft-temperature-limit; bits 16–19 are historical since boot. Current conditions warn; historical-only conditions are informational. Unknown bits remain in `raw-mask` / `unknown-mask`, produce a finding and incomplete interpretation, not “all clear.” |
| Service | Compare explicit desired active/inactive state with structured load/active/substate. `failed`, missing, or unexpected state produces a warning; transitional/unknown state is not “up.” Preserve active substate: active/exited is not evidence of a running process or application readiness. No HTTP health checks are implied. |

`coverage` accounts for requested samples **and** requested subfields: missing
wear/inode evidence, unknown firmware bits, and unavailable samples make it
partial. If nothing is usable it is none. Health reports the highest observed
warning/critical; otherwise it is `unknown` when coverage is incomplete and `ok`
only when all requested evidence is interpretable and within policy. Thus a
critical temperature is not hidden by unsupported SD wear, while unsupported SD
wear alone cannot yield green. Findings and samples have deterministic order
(category, gateway/device/channel/resource IDs, finding code); finding indexes
refer to that ordered sample list. Calculations use checked
integer arithmetic. No free-form model summary is the source of truth.

Valid sampling with an unavailable sensor is a successful domain evaluation
with partial coverage, **not** a forged successful reading. Invalid input,
authority rejection, runtime failure and deadline expiry remain distinct in the
shared result envelope. Mother denial before execution supplies no domain report.
Per-read revocation denies that read before access; a report may retain earlier
admitted samples and mark the denied sample unavailable if the runtime permits
partial reporting. A fatal runtime denial/trap must instead remain a failed call;
partial progress cannot change it to success. Caller-facing errors obey D-36:
detailed policy reasons stay local, and resource absence versus denial is not an
inventory oracle.

## 4. Toys, host adapters, and authority

All IDs below are **proposed new catalog entries**, not available imports today.

| New Toy / WIT interface | Actions and exact resource | Host mechanism; prohibited shortcut |
| --- | --- | --- |
| `toy:mct:storage-read` / `patina:device-health/storage-read@0.1.0` | `read-volume` on one provisioned mount ID; `read-media` on one media ID | `statvfs`/equivalent bound to an opened approved mount; fixed, documented kernel/device attributes for identity/type and supported health evidence. Never `df`, `lsblk`, `smartctl`, raw sectors, arbitrary ioctl, fsck, or write tests. |
| `toy:mct:temperature-read` / `patina:device-health/temperature-read@0.1.0` | `read` on one thermal source ID | Typed host parser of fixed Linux thermal/hwmon ABI scalar attributes. Only configured paths, bounded reads; no `sensors` output parsing or broad `/sys` preopen. |
| `toy:mct:throttling-read` / `patina:device-health/throttling-read@0.1.0` | `read` on one board/firmware source ID | Fixed Raspberry Pi firmware GET_THROTTLED request via a supported driver/mailbox adapter. Whitelist the read tag, bound response and time, verify non-clearing semantics on the target. No `vcgencmd` and no general mailbox or history-reset capability. Unsupported driver/permissions → typed unavailable. |
| `toy:mct:service-read` / `patina:device-health/service-read@0.1.0` | `read` on one mapped systemd unit ID | Read-only systemd D-Bus properties for exact approved units, no ListUnits inventory or activation/load/start/stop/restart methods. No `systemctl` parsing. A unit absent from manager state is not automatically proven absent from disk; report not-present/unknown as appropriate. |

Reading documented scalar kernel ABIs in the **Toy adapter** is structured
collection, unlike scraping presentation output in a Child. The host maps opaque
IDs to exact mount/device/zone/unit records and pins identity across a read.
Mount/device replacement, symlink escape or changed provisioning yields
`resource-changed`; it never falls back to a similarly named device. Serial
numbers, filesystem paths, arbitrary unit properties, environment variables and
raw kernel logs do not leave the adapter.

No generic SD lifespan backend is promised: ordinary SD often lacks standard
wear telemetry; eMMC lifetime buckets must not be relabeled as SD support. Any
future device-specific read backend must separately prove its read-only API,
permission floor and timeout. If the kernel offers no bounded/cancellable path,
refuse that backend; an epoch interrupt cannot stop a hung ioctl or storage read.
Use bounded host workers and no unbounded queue or automatic retry. Production
SD failure is not an excuse to run a destructive “health test.”

### Grant shape

Each Toy requires an authority-bearing `CanonicalToyContract` admitted into the
closed catalog. A manifest need or an installed Linux utility grants nothing.
Every issued `ToyGrant` must bind:

- exact Child name, artifact ID/version, assignment and admitted caller;
- executing node, Vision, project/data classification, exact provisioned
  resource ID (including subject binding), and only the action above;
- issuer, authority observation, current policy/grants revisions, explicit
  start/expiry and a maximum duration no greater than the call budget.

Use `max_uses: None`: the current effect gate refuses metered grants because
usage accounting is not implemented; invocation budgets enforce the 32-call
ceiling instead. No wildcard `resource_id: None`. Mother kernel evaluation
mints `AuthorizedToyCall` and the adapter revalidates current authority **for
each selected resource before reading**. Host binding must match that capability,
not a caller-supplied path. Grant revocation blocks new reads/admission; it cannot
unread bytes already observed. Offline execution requires locally valid unexpired
authority; disconnected is not an extension of a grant.

Existing directory-read, keyvalue, Watch, logging and measurement adapters are
not substitutes for these Toys. Extending `MctWitHostImportAdapters`, catalog and
resident admission is future work; do not fake Watch imports to obtain storage.
Neither receipt signing nor offline queues requires granting this Child secrets,
a Mother database connection, or filesystem writes.

### Nonlocal sensors / MCU stretch

The shared `source-ref.device-id` identifies an observed leaf when present;
otherwise the subject is the host named by `gateway-node-id`. `resource-id` and
optional `channel-id` select a provisioned source, never a network address.
Only the gateway's own mapped resources are readable by its local adapter;
a caller cannot turn another gateway ID into implicit network authority. The same temperature sample can later come through the
NIC-115 sensor/serial Toy. A Pi gateway owns grants, bounds the serial exchange,
attests acquisition/freshness and signs the receipt. Leaf provenance distinguishes
firmware/WAMR/wasm3 readings from local kernel readings. Leaf boot/sequence data
uses NIC-110's `sample-origin.source-epoch` / `source-sequence`, never confused
with the gateway boot clock; missing/reset epochs expose discontinuity. Unknown leaf freshness is unavailable, not a fresh sample.
No MCU Mother, component-model assumption on the leaf, new network authority in
this Child, or claim of independent MCU signature is implied. V1 implements host
resources only; the data model does not encode “sensor must be local.”

## 5. Resource budget and precompiled artifact profile

All numbers are **design ceilings/acceptance targets**, not measurements. Mother
admission must choose the smaller of these and caller/host limits. Values are
expressed in binary KiB/MiB.

| Resource | Proposed NIC-111 declaration | Enforcement / measurement |
| --- | --- | --- |
| Guest linear memory | 2 MiB initial target; **8 MiB maximum** including guest stack/heap | 128 Wasm pages maximum; reject artifacts with multiple memories whose aggregate escapes this bound. Existing per-memory StoreLimits alone does not prove an aggregate cap. |
| Incremental resident memory | ≤24 MiB peak over idle Mother, including Wasmtime instance/code and host buffers | Measure RSS and peak over idle on Pi 3; guest memory alone is insufficient. Total Mother RSS must also be recorded against the ~905 MB machine capacity. |
| Concurrency | 1 active invocation per Pi across profiles, following NIC-110; bounded rejection when occupied | No seven guest instances on one Pi. Fleet coordinator is outside this Child; preserve NIC-110's 256 MiB OS/Mother/control reserve without relying on swap. |
| Cold startup | p95 ≤500 ms, every sample ≤1 s | Verification/load/instantiate of already-local `.cwasm` through ready-to-call; exclude network transfer, include artifact verification. No JIT. |
| Invocation time | p95 ≤1 s on healthy local resources; **5 s hard deadline** | Includes startup, host reads, evaluation, terminal ledger/receipt work; each host operation ≤100 ms or remaining deadline, whichever is smaller. 32 reads leave time for completion. Refuse admission if evidence space cannot be reserved. |
| Guest compute | ≤50 ms target per full request | Measure separately from host waits; epoch deadline is the hard backstop, not a currently configured fuel allowance. |
| Host / Toy calls | ≤64 host calls; ≤32 Toy calls; 0 logging/measure calls | Charge through existing invocation-budget machinery plus per-resource adapter checks. |
| Observations / accumulation | ≤256 invocation observations; ≤512 KiB cumulative charged bytes | Include authority/read/terminal evidence and dynamic overhead. Shared receipt/outbox allocations require separate bounded host accounting. |
| Values and wire bytes | ≤64 KiB charged host values; input ≤8 KiB, domain output ≤24 KiB | Serialized and lifted-value limits are separate. Verify worst-case WIT nesting fits before approval. |
| Returned envelope | ≤32 KiB **including receipt**; seven-node merged result ≤256 KiB | NIC-110 must allocate receipt/proof overhead or change this declaration explicitly. No unbounded embedded ledger history. |
| Artifact | `.cwasm` ≤4 MiB target | Measure size and compatibility before installation; not an existing loader guarantee. |

Shared `operation-declaration` uses `read-compute`, the schema IDs in §3 and
these maxima: linear memory 8,388,608; startup 1,000 ms; wall 5,000 ms; input
8,192 bytes; application output 24,576 bytes; host value 65,536 bytes;
accumulated 524,288 bytes; host/Toy calls 64/32; logging/measure 0/0;
observations 256; max-items 32 sample slots (plus the schema's 128 findings).
**Review exceptions:** the 64 KiB host-value and 256-observation requests exceed
NIC-110's initial 16 KiB/130 defaults. They accommodate dynamic typed ABI overhead
and per-resource authority facts; they are requests, not permission. Admission
must reject if the accepted effective budget cannot fit the preflight bound,
not silently drop resources. All other NIC-111 caps narrow the shared starting
policy. The 32 KiB signed package/256 KiB fleet limits are host-side reservations,
not a second interpretation of shared `budget.output-bytes`.

NIC-110 must supply a trusted **nicamaster-only** precompilation pipeline: build
for Pi 3's aarch64 CPU feature floor, pin Wasmtime version/engine configuration,
bind original component + manifest/WIT hashes to the compiled artifact digest,
and authenticate build provenance before loading. A serialized Wasmtime artifact
is native executable material, not safe merely because the original Wasm was
sandboxed. Verify trusted bytes and compatibility before deserialization, without
a time-of-check/file-replacement gap. Wrong architecture, features, engine, digest
or provenance must reject; **never fall back to compiling on a Pi 3**. Compile,
link and artifact generation belong on nicamaster only after build approval.

## 6. Shared envelope, signed receipt, fleet fan-out

NIC-111 supplies the typed `answer` (report plus coverage). It requires
NIC-110's envelope/receipt to bind:

- schema version, job/attempt/call identity, input digest (including policy),
  output digest, exact artifact/assignment, executing Mother and observed subject;
- terminal outcome, sample provenance/time confidence, requested freshness,
  authority/resource-binding evidence references, actual budget usage;
- durable ledger entry/range or checkpoint commitment, signer/key identity and
  verification rules. A hash chain alone is not a portable signed receipt.

Mother signs **after durable terminal evidence**, not after an in-memory report.
Accepted calls that deny, time out, fail or are settled interrupted also need
receipted terminal outcomes. If disk/signing/ledger durability is unavailable,
return failure without a valid completion receipt; do not fabricate one or claim
receipt success. If the Mother never received a call or never recovers, no
executor receipt is possible. The coordinator receipts its own dispatch/failure
observation and labels executor evidence missing. NIC-110 must make this boundary
explicit rather than promising a signature from an unreachable machine.

Fan-out is a bounded coordinator/client operation, **not a Toy import or recursive
Child call**. Submit one call to each of six Pi 3 Mothers plus nicamaster using
normal single-hop routing and independent local authority. Use an authorized
explicit target set, stable per-node call IDs and at most seven in flight, one
per node; one merged answer is returned by a proposed 10 s collection deadline.
This is a requirement, not a claim that the current sequential split-job path
already implements concurrent health fan-out.

Each requested node has exactly one NIC-110 `node-result` slot: `received`
with the original receipted outcome, or `missing` with `not-dispatched`,
`unreachable`, `reply-deadline`, `invalid-receipt` or `incompatible-result`.
Pending is submission state, not a new settled slot variant. Interrupted is
`received(failed/execution-interrupted)`. After an uncertain send,
`missing.may-have-executed` is true. Preserve all successful siblings. Slots bind
node identity and request digest, reject duplicate/conflicting replies and do
not accept a receipt for another node/call. Verification failure is not healthy.

The merge presents counts (requested/responded/verified/unknown), worst observed
health, coverage, and per-node findings. Overall green requires all seven fresh,
verified, complete and healthy results. An unknown node remains visible even
when another is critical; an offline node cannot vanish from the denominator.
Use NIC-110's successful/unsuccessful/missing counts and complete/partial/failed
fleet status unchanged; domain health is a separate field. A late result creates
a newly receipted revision with `supersedes`, never edits the first answer.
Signature verification and independent ledger inclusion verification remain
separate statuses; if chain disclosure is unavailable, label inclusion unchecked
rather than claiming fully verified health.
Node-specific provisioning can differ (nicamaster may lack Pi throttling); typed
unsupported is evidence, not a transport failure. No topology beyond the
caller's permitted target inventory is disclosed.

## 7. Reboot, dropped replies, offline queue and handover

No persistent state in this Child. NIC-110's coordinator and executing Mother own
bounded durable request/result storage. Keep durable delivery separate from
running the Child again; current Mother's 12-minute dedupe cache is not a durable
job/outbox implementation.

| Interruption point | Required outcome |
| --- | --- |
| Durably queued but provably not dispatched | Persist immutable request identity/digest and expiry. First dispatch after connectivity returns rechecks current grants/deadline. An expired deadline settles as timed-out/deadline-exceeded, never backdated execution. |
| Runtime may have started, no durable terminal result | D-32 settlement records interrupted/unknown; no automatic resume. Same-attempt retry returns that disposition. A policy-authorized fresh sample requires a **new attempt/call ID**, linked to the old job. |
| Terminal result committed, reply/ack lost | Recover exact retained result and receipt by ID/digest and redeliver; do not reread changing temperatures under the old ID. |
| Offline after successful local sampling | Keep original sample times, result and receipt in Mother-owned outbox; deliver later with explicit age. Result delivery is not new execution. |
| Torn ledger tail / corrupt ledger | Preserve/recover a valid prefix per doctrine; corruption quarantines/fails closed. Never skip evidence to manufacture a completed sample. |
| Disk full, read-only, dying SD, or uncertain append | Fail closed before further admitted reads if required observation cannot commit. No “healthy” response and no unreceipted result presented as verified. Surface unavailable evidence; do not delete canonical ledger history to make room. |

Proposed outbox admission bound per Mother/assignment: **256 unacknowledged
results or 8 MiB**, whichever comes first, with 32 KiB reserved per accepted
result. Reserve both canonical evidence and result space before admission; the
canonical ledger has its own host quota/reserve, not this 8 MiB cap. At capacity,
reject/defer new work; never overwrite an undelivered receipt. A separate
coordinator request queue has at most 256 intents / 2 MiB and explicit expiries;
full means backpressure. These are narrower health quotas within NIC-110's
aggregate 256-record/16 MiB per-Mother queue budget, not extra independent
allowances: count each pending record once across both queues and reserve the
shared additional 1 MiB for settlement/control. Other assignments consume the
same aggregate. These are delivery requirements, not new Child keyvalue
authority or existing runtime behavior.

Handover verifies signer, digests, audience and identity before durable receiver
acknowledgement; duplicate deliveries are idempotent. Release sender result
payload only after durable acknowledgement and NIC-110's proposed minimum
seven-day retention. Retention/identity-tombstone exhaustion also backpressures
admission; no unbounded tombstone set. Keep canonical ledger facts and delivery
tombstones so delayed duplicates cannot become new
execution. Revalidate data-release authority at handover; revocation may block
export without erasing the historical local observation. Wall-clock uncertainty
must not extend grant validity. A crashed/offline Mother that cannot validate
current authority does not sample merely because an old request was queued.

## 8. Composition candidates and review decisions

Separate collection from pure evaluation internally first; do not multiply
resident components merely to satisfy a diagram. Future smaller Children may
export `storage-snapshot`, `thermal-power-snapshot`, and `service-snapshot`, with
a no-import pure evaluator consuming their typed, provenance-bound samples.
Static component composition or a client assembling these results must retain
resource and subcall receipts; it cannot inherit another component's grants.
Per-component split is justified when NIC-114/NIC-115 actually reuse the seam.

- **NIC-114 threshold watcher:** consumes health samples/findings plus freshness
  and boot identity; owns crossing/hysteresis state, scheduling and wake policy.
  NIC-111 remains one-shot and never wakes agents itself.
- **NIC-112 log summary:** may explain a reported fault through a separately
  granted journal reader. Log access is not automatically bundled with health.
- **NIC-113 config preflight:** can compare expected services/resource mappings;
  pure comparison needs no health Toy if supplied verified snapshots.
- **NIC-115 sensor/gateway:** align subject IDs, units, availability and provenance;
  reuse temperature sources without assuming a local thermal zone.
- **NIC-116 guarded service action:** uses a fresh service sample as plan evidence;
  rechecks its own action grant and precondition at effect time. A health warning
  never grants permission to restart or bypass a dry-run.

### NIC-110 reconciliation checklist (build blockers)

1. Final shared WIT package and wrapping convention: typed report payload,
   domain partial coverage distinct from transport/authority failure; versioning
   and the exact RFC 8785 projection above without JSON-in-string WIT edges.
2. Portable receipt: signature scheme, canonical encoding, trust/key rotation,
   terminal evidence commitment, input/output/authority binding, withheld evidence
   rules, maximum proof size, and missing-receipt representation.
3. Fan-out slots, deterministic merge and late-result revisions, coordinator
   receipt, age/time-confidence semantics, and no inference of remote success.
4. Budget declaration/admission: per-Child guest and host caps, before-allocation
   limits, host I/O cancellation, receipt capacity and memory accounting.
5. Trusted `.cwasm` pipeline and fail-closed loader; no Pi compilation fallback.
6. Durable queued-intent versus execution-attempt identity, post-dedupe retrieval,
   bounded outbox/ack/retention behavior, reboot settlement, and release authority.
7. Shared source/subject identifiers and gateway provenance; avoid a competing
   NIC-111/NIC-115 sensor ABI.

Doug reviews this design and build order before implementation. Start future
NIC-111 implementation with structured host sampling + pure evaluation after
NIC-110 is agreed; add no unrelated scheduler, recovery ceremony or admin model.
Design session environment reported `PI_MODEL=gpt-6-astra` and
`PI_REASONING_LEVEL=high`; approved building belongs to Sol high/xhigh.

## 9. Pi 3 acceptance plan — not executed

Use a separately approved disposable test setup, not production services/media.
Each row is a future pass/fail gate. Retain artifact/runtime versions, request and
result digests, receipts, per-node outcomes, ledger verification and measured
budgets as evidence. A unit test alone cannot establish hardware acceptance.

| Gate | Procedure and pass condition |
| --- | --- |
| Real useful call | On a ~905 MB aarch64 Debian Pi 3, install only a trusted precompiled artifact. Read a real configured volume, SD identity/available health, CPU thermal source, firmware flags if supported, and disposable systemd service state through Mother. Report typed values matching independent native API reads, a verified receipt and no shell/process execution. |
| Honest SD limits | With ordinary SD lacking lifetime telemetry, report unsupported wear and partial/unknown rather than 0% wear or healthy. With device-health fixtures, warning/failure and eMMC buckets are interpreted correctly without claiming physical wear was induced. No destructive SD test. |
| Boundaries and semantics | Inject exact threshold equality, negative valid temperature, zero capacity, unavailable inode data, near-full/read-only volume, current versus historical throttle masks, unknown firmware bits, service active/exited/failed/transitional/missing state. Expected findings and coverage follow §3; no divide-by-zero or hidden unknowns. Use a disposable volume/service, never fill the Mother ledger volume deliberately. |
| Authority | Missing, wrong-resource/subject/caller/Vision, expired and revoked grants deny access; mutate provisioning between admission/read and require rejection. Instrument adapters to prove zero unauthorized reads. Revocation between two reads stops the latter; neither old grants nor caller-selected IDs widen scope. |
| Resource abuse | Over-limit selectors/strings/input fail before reads. A 32-sample/128-finding fixture admitted by size preflight fits charged-value and output bounds; maximally long selectors whose report cannot fit reject before reads. Excess memory, malformed/huge Toy output and slow backend cannot OOM/hang Mother; deadline failures remain failed with terminal evidence where durability permits. |
| Measured budget | Record 100 cold instantiations and 100 full warm calls, with idle and peak RSS, guest peak, CPU/wall times, host calls, observation bytes and serialized output. Meet every §5 ceiling/p95 target; repeat with resource-limit and I/O-timeout injection. No “under 64 MiB” claim based solely on guest pages. |
| No Pi compiler | Observe no compilation/JIT path on Pi. Supply wrong CPU/runtime/config/provenance/digest artifact variants; all refuse before load with no source-Wasm fallback. Valid artifact compiled on nicamaster runs on the Pi 3 CPU feature floor. |
| Seven-node partial failure | One job addresses six Pi 3s + nicamaster; verify one slot per intended node and one merged answer. Separately deny one node, time out one, drop one connection, and tamper with a receipt. Good siblings survive; missing/invalid evidence is explicit, no overall green, no local fallback masquerading as the target. |
| Reboot/drop | Inject interruption before start, during sampling, before/after terminal append and after receipt before delivery ACK; include an approved Pi reboot and separately a process kill. After recovery verify both Mother ledgers. Calls settle once or replay committed results; no silent resampling under the old call ID. Preserve torn-tail evidence; corruption refuses operation. |
| Offline handover | With locally valid grants, queue results while disconnected; restart, reconnect and redeliver. Receiver verifies original times/digests/signatures, acknowledges durably and dedupes repeated/lost ACK delivery. Revoked/expired authority prevents new sampling/export as appropriate. Late results remain stale historical samples. |
| Queue/storage pressure | Reach the bounded outbox and request-queue limits; observe backpressure, no deletion of undelivered receipts and no unbounded RAM. Inject ledger append failure/uncertainty and signing failure: no verified success is issued. Restore/recover only through existing doctrine. |
| Gateway shape | Feed a test serial-source sample with a distinct subject/boot/sequence into the proposed adapter seam. Verify gateway signer differs from observed subject and stale/malformed leaf evidence is unavailable. This tests ABI extensibility, not a claim of MCU runtime delivery. |

### Design-only validation

- Reviewed the existing runtime/budget/ToyGrant boundaries and the NIC-104 /
  NIC-110 sources identified in §2; filled every required design field.
- Extracted the inline WIT into a temporary directory with NIC-110's shared
  dependency and parsed it with `wasm-tools component wit` in **text-only mode**.
  Syntax/type references passed; no Wasm binary, Rust binding or `.cwasm` was built.
- Checked all ten relative documentation/source links and whitespace. These
  checks do not establish runtime behavior or measured budgets.

**Design-pass acceptance:** every field is specified here, gaps are explicitly
assigned to NIC-110 or new Toy work, and this document is committed/pushed on
`nicabar-nic111-design`. Hardware/runtime gates remain unrun. No Rust, builds,
deployments, fleet commands, PR, or changes to `main` belong to this task.

# NIC-115 — Sensor/GPIO read Child and MCU gateway

**Status: design proposal; not built, measured, deployed, or approved for build.**
Issue: [NIC-115](https://linear.app/nicabar/issue/NIC-115), under
[NIC-109](https://linear.app/nicabar/issue/NIC-109). Shared-contract dependency:
[NIC-110](https://linear.app/nicabar/issue/NIC-110). Target: Raspberry Pi 3,
aarch64 Debian, approximately 905 MB RAM. Design session: `gpt-6-astra`, high.
Doug approves the build order before implementation on `gpt-5.6-sol` high/xhigh.

## 1. Decision and profile

Build one **read-compute** Child, provisionally `mct-sensor-read`. One bounded
`read` invocation observes up to eight named channels on one authorized endpoint,
checks freshness and value semantics, and returns a structured report. An endpoint
can be a Pi-attached sensor/GPIO group or a microcontroller behind that Pi. The
same Child/WIT works for both; neither a device pathname nor a physical pin number
is a routing identity.

```text
agent/client → ingress Mother → executing Pi Mother → sensor-read Child
                                      ↑                    │
                             authority + ledger     sensor Toy import
                                      │                    ↓
                              admitted host adapter
                               /                 \
                      Pi sensor/GPIO       bounded serial protocol
                                                 ↓
                                        ESP32 / Pi Pico leaf
```

Mother decides and signs; the Child computes; the Toy performs hardware I/O.
The leaf is firmware or tiny core Wasm under WAMR/wasm3, **not another Mother or a
local MCT Child**. It receives no ToyGrant, Mother key, peer credential, or ability
to forward MCT calls. Host receipt signing proves what the Pi observed, not that a
physical measurement is true or that unauthenticated firmware is trustworthy.

**In scope:** input-only GPIO level snapshots, bounded sensor readings, explicit
missing/stale/error results, and a location-independent gateway contract.
**Out of scope:** GPIO output/PWM, pinmux/pull changes requested by callers,
actuation, firmware upload/reset, arbitrary I2C/SPI/UART transactions, streaming,
edge capture, hard real-time sampling, autonomous scheduling, MCU Mothers, or
inference/git/build work on Pi 3s. Git Children remain macOS-only on Studio/Lume.
A Pi is a hand on hardware, not a place for an agent to think.

### Canonical fit and source boundary

This follows NIC-104's bounded app operation and NIC-105's reference read-compute
shape, not the legacy watcher lifecycle. Artifact, approval, assignment, and
instance remain separate facts; manifest `needs` is not permission. No `init`,
`tick`, fake Watch scope, persistent guest process, or ambient filesystem is needed.
Existing `mct-repo-report` JSON v1 stays unchanged; the WIT below is a new,
separately versioned domain proposal, not a migration of that interface.

Inspection baseline is `bc48d1e`. Sources:

- [First real app plan](../../plans/first-real-app.md) and
  [reference WIT](../../../crates/mct-repo-report/wit/world.wit).
- The `docs/mct-vision-child-plan.md` consult, read from NIC-105's parallel
  checkout, committed there as `2bef593` (not present in this branch's baseline),
  plus [NIC-104's locked decisions](https://linear.app/nicabar/issue/NIC-104).
- [Runtime](../../../crates/mct-daemon/src/wasm.rs):
  `DEFAULT_WASM_MEMORY_LIMIT_BYTES`, `store_limits`, `compile_snapshot`, and
  `configure_deadline`.
- [Invocation budgets](../../../crates/mct-daemon/src/wasm/budget.rs),
  [Toy grants](../../../crates/mct-kernel/src/toy.rs),
  [Toy adapters](../../../crates/mct-daemon/src/toy.rs), and
  [resident selection](../../../crates/mct-daemon/src/daemon/resident/execution.rs).
- [D-32–D-37](../../DECISIONS.md#5-restart-recovery-and-the-peer-wire) and the
  explicitly unratified [durable-task proposal](../durable-task-boundary.md).

The inspected code has neither hardware Toy nor a `.cwasm` loader. It uses
`Component::from_binary`, including import discovery, so precompilation is a
**shared runtime prerequisite**, not a packaging trick. Existing observations and
signed peer/admin facts are not proof of a signed application receipt per call.
Ordinary directory-read wiring and Watch-specific keyvalue/messaging do not
provide these new imports or a general durable outbox.

## 2. WIT world and inputs/outputs

The proposed package is `mct:sensor@0.1.0`, world `child`. Callable operation:
`mct:sensor/report@0.1.0#read`. Authority-bearing import:
`mct:sensor/observation@0.1.0#snapshot`. `types` is data, not a Toy.
This is design WIT, not a generated binding or implemented runtime contract.
It reuses NIC-110's draft source, digest, sample-origin and coverage types.

```wit
package mct:sensor@0.1.0;

interface types {
    use patina:small-device/common@0.1.0-draft.{source-ref, sample-origin,
        digest, coverage as result-coverage};

    // Host-configured opaque identifiers, never paths, URLs, or grant handles.
    record endpoint-ref {
        source: source-ref,
        descriptor-digest: digest,
    }
    record read-request {
        endpoint: endpoint-ref,
        channels: list<string>,
        max-age-ms: u32,
        allow-cached: bool,
    }
    enum unit { celsius, pascal, percent, volt, ampere, lux, count }
    record quantity {
        coefficient: s64,
        decimal-exponent: s8,
        units: unit,
    }
    record gpio-level {
        electrical-high: bool,
        logical-active: bool,
    }
    variant value { scalar(quantity), gpio(gpio-level) }
    enum reading-error {
        unavailable, unsupported, stale, invalid-value, device-reset,
        protocol-error, deadline, busy,
    }
    record reading {
        channel: string,
        observed-monotonic-ms: u64,
        origin: sample-origin,
        age-upper-bound-ms: option<u32>,
        from-cache: bool,
        outcome: result<value, reading-error>,
    }
    enum transport { local-sensor, local-gpio, serial-leaf }
    enum device-binding { configured-port, authenticated-leaf }
    record provenance {
        endpoint: endpoint-ref,
        transport-kind: transport,
        binding: device-binding,
        gateway-boot-id: string,
        received-unix-ms: option<u64>,
        source-boot-id: option<string>,
        source-sequence: option<u64>,
        firmware-digest: option<digest>,
        source-time-trusted: bool,
        frame-digest: digest,
    }
    record sample-batch {
        origin: provenance,
        readings: list<reading>,
    }
    enum read-error {
        invalid-request, denied, unavailable, unsupported, deadline,
        protocol-error, resource-limit,
    }
    enum coverage { complete, partial, none }
    record read-report {
        observation: sample-batch,
        availability: coverage,
        usable-channels: u8,
        result-coverage: result-coverage,
    }
}

interface observation {
    use types.{read-request, sample-batch, read-error};
    snapshot: func(request: read-request) -> result<sample-batch, read-error>;
}

interface report {
    use types.{read-request, read-report, read-error};
    read: func(request: read-request) -> result<read-report, read-error>;
}

world child {
    import observation;
    export report;
}
```

### Request and response rules

- The shared invocation metadata supplies job/call/idempotency IDs, caller,
  Vision/project/classification, effective deadline, and budget. Business input
  cannot override any of these. Mother authenticates `source.gateway-node-id` against the
  executing node, not the ingress node. A mismatch fails; no adapter forwarding
  to a second Mother. Only the authorized publisher reveals endpoint references.
- One call names one immutable endpoint descriptor and 1–8 unique channels,
  sorted by channel ID. Domain IDs are bounded ASCII tokens (64 bytes each);
  shared `digest` values are 64 lowercase hex BLAKE3-256 characters. Existing
  SHA-256 artifact/manifest approval IDs retain their original meaning; do not
  substitute a shared content digest for an approved runtime artifact identity.
  Unknown channels and duplicate IDs fail validation. There is no discovery wildcard, physical bus scan, arbitrary
  path, IP address, or user-supplied register/baud/driver parameter.
- The Mother-owned descriptor binds the logical resource to the physical device,
  channel set, driver/protocol version, units/scaling/calibration revision,
  input direction and active-low semantics, freshness policy, and rate limits.
  It is immutable and its digest is part of the exact granted resource identity.
  Rewiring, firmware/protocol changes, or widening the channel set requires a new
  descriptor and grant review, not retargeting an old resource silently.
- `max-age-ms` is 0–60,000, capped further by policy; zero requires acquisition
  for this invocation, not physically zero elapsed age. Positive limits bound age
  at report production. `allow-cached=false` also forbids cached observations.
  If a defensible age upper bound is unavailable, the reading is `stale`, never
  fresh by assumption. Use gateway monotonic time within its boot, not a drifting
  MCU wall clock. An MCU boot/tick-to-gateway mapping needs bounded uncertainty;
  a fresh correlated acquisition may instead bound age by the round-trip time.
- Values use bounded fixed-point quantities, not locale-dependent strings or
  NaN. Decimal exponent is limited to -9…9; supported unit, range, calibration,
  and overflow checks follow the descriptor. GPIO reports electrical level and
  configured logical interpretation; never infer pull state from a floating pin.
- Each requested channel has exactly one result in request order. Absent values
  are errors, never zero. The Child validates the typed snapshot and computes
  `coverage`/usable count; it does not average away errors. Complete means every
  requested channel is usable, partial means some, none means zero. A non-atomic
  multi-channel read preserves each channel's acquisition time; it is not a
  simultaneous physical snapshot.
- A domain report, including `coverage=none`, can be a successfully executed
  app operation. Shared execution outcome and measurement availability are
  separate. Admission denial, traps, and deadline termination use the existing
  Mother outcome vocabulary; do not add `partial` to kernel `ResultOutcome`.
  An authorized domain `err` must remain visible in the shared payload even if
  the WIT invocation itself completed successfully. Do not label it a usable read.
  `denied` is reserved for the host authority path: Mother promotes a Toy denial
  to outer denied/no answer, never a success-shaped guest error. A guest cannot
  override or conceal that host denial by returning a different report.
- The adapter, not the guest or leaf, supplies authoritative gateway identity,
  descriptor binding, receipt timestamps, and evidence digests. A claimed
  firmware digest from unauthenticated firmware is merely reported metadata;
  only an independently verified value may be labeled verified. No raw serial
  frames, kernel errors, device paths, keys, or grant internals in caller output.

For each reading, shared `sample-origin.source` repeats the authorized endpoint
and sets `channel-id`; the endpoint-level source has `channel-id=None` and a
configured `device-id` for a leaf (not a raw address). Shared `source-epoch` is
the leaf boot/session ID for serial or gateway boot ID for local reads, matching
the provenance below; sequence comparisons never cross epochs. `observed-at` is
absent unless sample time is defensible; `received-at` is the Pi's receipt time. Unknown
source age maps to shared `freshness=unknown` and a domain stale error. A trusted
Mother wall clock is required for admission/receipts; per-boot monotonic time
carries age calculations. Evidence digests cover bounded canonical adapter sample
material; `frame-digest` covers the frame for serial, or that material for local
hardware. No fictitious serial frame is implied for GPIO.

Example meaning: one MCU endpoint returns temperature `coefficient=23125`,
`decimal-exponent=-3`, `units=celsius` (23.125 °C), a GPIO logical-active value,
and `stale` for a third channel. Coverage is partial, usable count two. A caller
can distinguish that from a denied invocation and from an unreachable gateway.

## 3. Toys, authority, and hardware safety

| Capability | Status and decision |
| --- | --- |
| Sensor/GPIO observation Toy | **NEW, required.** One canonical `mct:sensor/observation@0.1.0#snapshot` contract with separately configured local-sensor, local-GPIO, and serial-leaf backends. Only `read-snapshot` is authorized. |
| Serial Toy | **NEW if separately exposed; not an existing Toy.** The minimal MCU slice uses a bounded serial backend inside the sensor Toy, not a second guest import. A reusable `mct:serial/framed-read` Toy is deferred until another Child needs it; it must get its own catalog admission, exact grants, and protocol review. No generic `read/write bytes` escape hatch. |
| Logging/measure | Existing capabilities, optional diagnostics only; omitted from this minimal world. Receipts never depend on guest logs. |
| Keyvalue, messaging, clock/replay | Not imported here. The client/shared delivery layer owns durable job/result handling; NIC-114 owns watcher state. Existing Watch plumbing is not reused by pretending this is a Watch Child. |
| Filesystem, shell/process, Git, network, secrets | Not needed or granted. The host driver opening an allowlisted device is the sensor Toy effect, not a guest `/dev`, `/sys`, or `/proc` preopen. |

A grant fixes the exact Child name, artifact/version, assignment, caller node,
Vision, executing node, project and data classification, immutable endpoint
resource ID, and action `read-snapshot`. Set explicit start/expiry and a duration
no longer than the call budget. Avoid optional wildcard scope fields. Several
channels fit only because they belong to that one immutable resource; arbitrary
multi-resource batching is out of scope. Resource descriptors and their validation
are **new adapter policy**, not fields that already exist in `ToyGrant`.

Kernel evaluation mints `AuthorizedToyCall`; at effect time use the fresh
`LocalExecutionAuthoritySnapshot`/`admit_effect_with_snapshot` path and Mother's
ordered admission barrier. Persist required before-effect observations and reserve
terminal/result space before opening/reading hardware. If authority, clock,
observation storage, or reservation is unavailable, fail closed. Expiry/revocation
blocks new I/O admission; it cannot undo an already completed measurement.

Current `max_uses` grants fail closed with `ConsumptionStateUnavailable` at
resident effect admission. Do not pretend that field supplies rate limiting.
First slice uses `max_uses=None`, a bounded invocation, one outstanding read per
endpoint, and a Mother-owned token bucket: at most one snapshot/second/endpoint,
burst one, starting empty after reboot. Hardware timing or policy may be stricter.
Rate/range/channel checks occur at the Toy boundary even for a malicious guest.

For local GPIO, use the Linux GPIO character-device interface with exclusive
input-only line requests and reviewed static board configuration. Deny lines
claimed by another driver; do not steal or reconfigure them. No sysfs shell
scraping, `/dev/mem`, pigpio daemon bypass, direction change, output write, or
pin-number guessing. Board pin labels map to chip/line offsets only in the host
configuration. Sensor drivers may use reviewed kernel interfaces or fixed
read/conversion transactions. Bus writes needed to select a sensor register or
start conversion are explicitly allowlisted protocol effects—not arbitrary
write permission. Exclude destructive/read-to-clear registers and devices whose
read sequence can actuate hardware from this profile.

## 4. MCU gateway: same WIT, narrower physical trust

The Pi resolves the endpoint to a statically enrolled UART/USB-serial device and
leaf identity. Stable configured hardware identity and exclusive access matter:
`ttyUSB0` reuse alone is not identity. The Child cannot select a port. Host tty
configuration must not toggle DTR/RTS/reset lines or enter a bootloader; hardware
that resets automatically on open requires a reviewed connection strategy before
admission. Input-only/no-actuation safety must hold when the Pi or leaf reboots.

**Proposed protocol v1:** one outstanding, deadline-bounded `read-snapshot`
request. A length-delimited binary frame contains version, bounded length,
request nonce, configured leaf ID, leaf boot/session ID, sequence, channel IDs,
typed values, sample ticks/age information, and CRC. The reply must echo the
request nonce and match endpoint/channel schema. Frame ceiling: 2 KiB; at most
one request/reply, no automatic retries, bulk transfers, scripts, output writes,
firmware update, or reset commands. Mother caps reads before allocation and bounds
parsing, resynchronization, and device wait; oversized or malformed input closes
that exchange. No indefinite blocking host syscall behind a Wasm deadline.

CRC detects corruption, **not authenticity**. Default `configured-port` is an
explicit physical-trust profile: spoofing on that wire remains possible.
Authenticated-leaf requires a separately enrolled per-leaf MAC/signature protocol
with anti-replay/session rules, host-held verifier/key material, and a security
review. Never derive leaf trust from USB vendor/product IDs or a firmware string.
This stronger profile is optional and unbuilt; neither profile gives the leaf MCT
authority. A leaf's claimed gateway/call/grant identity is ignored.

Late replies, wrong nonces, duplicate sequences within a boot, and old-session
frames cannot satisfy a new call. A boot ID change invalidates cached timing and
sequence state and returns `device-reset` for the outstanding acquisition; a
later explicit call may re-establish the leaf session. A malicious leaf can lie
about boot or readings in the physical-trust profile; receipt metadata must not
conceal that limit. Interruptions never cause an MCU command to be replayed under
a new nonce as if it were the old observation.

Firmware is the first stretch target. WAMR/wasm3 core-Wasm leaves can implement the
same protocol later; component-model WIT bindings and the Pi `.cwasm` artifact do
not run there. A provisional Pico protocol/working-buffer target is ≤32 KiB RAM
(excluding firmware/runtime); WAMR/wasm3 total RAM/flash must be measured on the
chosen board separately. NIC-115 does not claim an MCU runtime footprint or require
an MCU deployment for the primary Pi 3 acceptance gate.

## 5. Budget declaration and precompiled artifacts

These are **proposed acceptance limits, not measurements**. NIC-110 owns their
final shared representation and enforcement. Follow its one active invocation
per Pi 3 policy across all Children, with single-flight endpoint I/O; reject
excess work or queue it durably outside guest memory.

| Resource | NIC-115 target / hard bound |
| --- | --- |
| Guest linear memory | Target ≤4 MiB peak; hard ≤8 MiB total across component memories; admit one bounded memory initially. Well below the 64 MiB default. |
| Host/process incremental resident memory | ≤32 MiB peak over an idle Mother, including guest, mapped artifact, ABI values, driver buffers and outbox staging; target ≤24 MiB. Measure process RSS/PSS, not just Wasm pages. |
| Artifact | `.cwasm` ≤4 MiB; no on-Pi compilation or compiler cache growth. |
| Startup | Precompiled load + link + instantiate p95 ≤500 ms cold, ≤100 ms warm; hard 1 s, included in the total call deadline. No resident guest required. |
| Total call | Hard 2 s from execution admission through result commit; hardware exchange ≤500 ms and bounded by remaining time; release I/O resources within 100 ms of cancellation/deadline. No retries. |
| Domain request / result | ≤2 KiB input, ≤16 KiB report; shared envelope + signed receipt ≤32 KiB per terminal bundle. No truncation that silently loses requested channels. |
| Channels / raw frame | 1–8 channels; ≤2 KiB serial frame; ≤4 KiB total raw exchange; no streaming buffer. |
| Host ABI/accounting | ≤16 KiB per host-value charge (includes dynamic value overhead), ≤512 KiB accumulated bytes; reject before copy/allocation where possible. |
| Invocation counts | ≤16 host calls, 1 Toy call, 0 logging/measure calls, ≤16 charged runtime/Toy observations including reserved terminal facts. Shared delivery observations are separately bounded/accounted. |
| Offline storage | Per assignment: ≤128 pending records AND ≤4 MiB payload/metadata, also within NIC-110's shared per-Mother ≤256 records/16 MiB and 1 MiB control reserve. Node-wide disk staging/WAL allowance ≤8 MiB extra; no unbounded in-memory queue. Ledger storage/retention is a separate Mother quota. |
| Fleet merge | Seven target slots, ≤256 KiB total merged envelope, at most two outstanding calls; 10 s overall collection deadline. Sequential dispatch is acceptable within that deadline. |

The runtime defaults are 4,096 host calls, 1,024 Toy calls, 2,050 observations,
4 MiB accumulated bytes, 64 KiB host values, and 256 KiB output. NIC-115 tightens
these; it does not raise the 64 MiB Wasmtime default. `StoreLimits.memory_size`
limits individual linear memories, **not whole-process RSS or necessarily the
sum of multiple memories**. Aggregate memory admission, per-profile selection,
host I/O cancellation, and total-call enforcement are build gaps to prove.
Epoch interruption alone cannot interrupt a blocked driver or bound ledger I/O.
The 32 MiB resident delta is a measured acceptance gate, not an existing limiter.

NIC-110's artifact pipeline must compile and precompile on **nicamaster**, using
the Pi 3 CPU feature baseline, aarch64 Linux target, pinned Wasmtime version and
engine configuration. Do not use nicamaster-native optional CPU features blindly.
An approved provenance record binds source/component digest, manifest and WIT,
compiled digest, compiler/runtime configuration, target and CPU features. Verify
trusted origin and immutable exact bytes **before** loading native serialized
code. A digest without trusted provenance is insufficient. Incompatible or
corrupt artifacts fail closed, never fall back to compiling `.wasm` on the Pi.
Import discovery/approval inspection must also avoid compilation on Pi 3.

## 6. Shared receipt, fleet answer, and offline handover

Reconciled against NIC-110's parallel draft, `docs/design/small-device/` files
`nic-110-shared-contract.md` and `wit/shared.wit`, package
`patina:small-device@0.1.0-draft`, committed as `5486458`. They are not copied
into this branch. The final WIT snapshot inspected has SHA-256
`4bbb5361b424ea1e80dcc26ccae5863cb0d60996439bbad030ad712989cee24a`.
Use its **result envelope, signed receipt, fan-out result, and budget declaration**;
Mother attaches them outside the guest. Draft changes must be reconciled before
build; no second envelope or receipt scheme is introduced here.

| Shared type | NIC-115 projection |
| --- | --- |
| `common.operation-declaration` / `budget` | Operation `mct:sensor/report@0.1.0#read`, profile `read-compute`; §5 supplies every counter/byte/time field, `max-items=8`. `output-bytes=16384`, `linear-memory-bytes=8388608`, `startup-ms=1000`, `wall-ms=2000`. Initially set `minimum-budget=requested-budget` to these ceilings rather than silently shrinking workload semantics; later measured tuning needs review. Disk/RSS/concurrency caps are host policy, not invented WIT budget fields. |
| `common.child-answer` | Typed `read-report` plus shared `result-coverage`. Register input schema `mct.sensor.read-request.v1` and output schema `mct.sensor.read-result.v1`; project the complete WIT result, including its `err` case, into `typed-payload`. |
| `common.coverage` | `examined` counts attempted channel outcomes, not usable values; no pagination or silent omissions, `omitted=Some(0)`, `continuation=None`. Every requested channel represented means shared complete even if measurement availability is none. A domain call-wide error has examined zero, omitted=requested count, partial. |
| `common.result-envelope` | Mother owns context/outcome/reason/usage. A schema-valid non-authority domain result uses success/none; its payload may still report unavailable sensors. Denied/trapped/timed-out invocations have no answer. `effect=no-effect` means no external mutation was requested, not that sensor Toy I/O needed no grant. |
| `receipts.receipted-result` | Use NIC-110's purpose-bound Ed25519 key, deterministic-CBOR statement, BLAKE3 envelope digest and ledger anchor; do not add a guest signer. |
| `fleet.receipted-fleet-result` | Seven sorted node rows, received/missing union, coordinator receipt, linked late-result revisions. Its successful/unsuccessful/missing counts are execution counts; sensor usability counts belong in the domain merge, not replacements for them. |
| `delivery` types | Shared queued/running/settling/terminal/result-unavailable/not-admitted states and destination-bound handover ACKs; no private NIC-115 queue protocol. |

**Exact payload projection:** choose shared `canonical-json` (RFC 8785). Record
keys are exact WIT field names; lists preserve order; enums are kebab-case strings;
variants/results are two-element arrays `[case,payload]`, including `ok`/`err`;
options are null/value. Encode all domain `s64`/`u64` fields as canonical decimal
strings (no leading zeros or plus sign); smaller integers are JSON integers.
This supports negative temperatures without extending NIC-110's unsigned outer
CBOR schema. Reject unknown fields/variants, duplicate keys and noncanonical
numbers. On an `ok` result copy `result-coverage` into `child-answer.coverage` and
require equality; for `err`, Mother derives the coverage described above from the
validated request. Nesting is ≤16; all strings/lists retain §2 bounds. Schema and
canonicalization vectors remain a build gate.

Shared fleet `complete` describes complete reports, not healthy/fresh sensors.
The sensor domain merge separately reports requested/usable/error channel counts
and `complete`/`partial`/`none` measurement availability. Thus seven fully accounted
reports with missing measurements cannot be presented as seven healthy devices.

### Receipt requirements to carry into NIC-110

Every admitted call has a durable terminal receipt, including denial, failure,
timeout, cancellation, and recovered interruption. Bind at least: schema version,
job/call/attempt and caller scope, requested and executing node, exact artifact
and assignment, operation, request and terminal-result digests, ledger epoch and
terminal observation/hash anchor, host time/clock quality, and signer/key identity.
The result digest covers domain errors and source provenance, including descriptor,
frame digest, freshness/cache flags and device-binding quality. Keep grant/policy
details private unless separately authorized. The shared contract owns signing
algorithm, canonical encoding/domain separation, trust distribution, rotation,
and ledger-anchor verification. No signing Toy or private key goes into the Child.

Commit result bytes and terminal ledger evidence before acknowledging delivery.
If signing is interrupted after commit, recovery may derive the same receipt from
the committed terminal evidence; it must not re-read hardware. A signature alone
does not prove ledger inclusion, and a hash chain alone is not an authenticated
receipt. Both links and the output digest must verify. If storage/signing is
unavailable, report delivery/receipt unavailable—not unsigned success. A dead or
unreachable target cannot produce a receipt on demand; mark its receipt missing,
retain the ingress observation, and acquire a target receipt later if one exists.
Malformed pre-admission transport data is an adapter failure, not an invented
executed Child call.

### Seven-node fan-out

The client/shared coordinator freezes a seven-target plan (six Pi 3 Mothers plus
nicamaster), with an explicit endpoint reference per target and stable piece IDs.
Submit ordinary authorized calls; each executing Mother admits its own artifact,
assignment, and exact Toy grant. Do not share keys/grants, move a read to a different
Pi because it is idle, or issue seven unconstrained calls that all select one Pi.
A missing sensor on nicamaster is an explicit unavailable/unsupported slot, not a
synthetic sensor value. D-37 remains single-hop: an MCU serial link is a local Toy
transport, not another Mother hop. Existing supplied-piece jobs are sequential;
parallel scheduling is a shared enhancement, not a current guarantee.

Merge in stable target/channel order, with one slot for each planned target:
requested/executing node attribution, shared execution status, domain report or
safe error, receipt/verification state, and delivery state. Count usable,
partial, failed/denied, unavailable, and pending slots without collapsing them.
No physical units or channels are summed across unrelated sensors. A target
timeout does not discard six valid results or become a fabricated target denial.
Only a verified receipt/result pair qualifies as verified evidence. The merged
answer references available executor receipts; the shared coordinator's required
fleet receipt cannot replace any missing executor receipt. Late verified
results create a new merge revision, preserving the prior incomplete answer.

### Offline queue and crash behavior

Queue **intent** at the client/shared coordinator and **completed bundles** at the
executing Mother's shared result store, not in Wasm memory. Follow NIC-110's
staged-bytes → terminal-ledger-fact → replayable projection/signature ordering;
there is no claim of one physical transaction spanning JSONL and SQLite.
This shared outbox is new infrastructure, not an assumed durable Child keyvalue
capability. Scope it
by Vision, assignment, caller and job; restrict result export and re-check current
read/export authority at handover. Queueing itself grants no hardware permission.
An offline Pi may execute a locally submitted call only with current locally
provable authority; stale cached peer authority is not a partition bypass.

| Interruption point | Required result / replay behavior |
| --- | --- |
| Intent queued, never admitted | Preserve the original expiry/freshness intent. On reconnect, revalidate authority and deadline; expired work is not silently refreshed. |
| Admission/start uncertain, or hardware read without committed result | Follow D-32/D-33: terminal `execution_interrupted`/unknown, no auto-resume or hardware replay. A new sample requires an explicitly new call/attempt and timestamp. |
| Result and terminal evidence durable, reply lost | Return the same bytes/digest/receipt; never repeat the sample to recreate a lost reply. |
| Receipt or acknowledgement lost | Retry handover by stable bundle identity. Receiver commits before ACK; duplicate ACK/delivery is idempotent. Delivery is at-least-once, not exactly-once hardware acquisition. |
| Pi or MCU reboot | Verify ledger/recover torn tail under existing law; settle in-flight calls, release lines/ports, invalidate caches/session timing. No background restart of the Child. |
| Full queue/disk or expired grant | Refuse new sampling before I/O if terminal capacity cannot be reserved. Preserve existing unacknowledged evidence; never evict it silently or reset quota through reboot. |

Reserve up to 32 KiB per admitted terminal bundle. Stop admission at either outbox
limit; separately reserve failure/settlement evidence capacity in the ledger.
ACKed bundles follow NIC-110's proposed minimum seven-day retention;
unacknowledged bundles remain until ACK or explicit recorded operator disposal. A queue full indefinitely
means sampling stays stopped. No promise of infinite offline history or unbounded
MCU buffering. Results handed over later retain original observation time and may
no longer be fresh enough for a new decision.

Mother's current 12-minute dedupe window is not the durable job/outbox contract.
NIC-110 must retain bundle/job identity (or a bounded terminal tombstone/archive
policy) independently of it; after retention expires return history unavailable,
not an automatic new measurement under the old key. Queued intent cancellation and
result-read revocation are distinct from deleting historical ledger evidence.

## 7. Composition candidates and build dependencies

Keep the first deployment one small read-compute component with pure validation
functions, rather than a graph scheduler or several always-on guests.

- **NIC-114 threshold watcher:** consume these timestamped typed readings;
  own hysteresis/debounce/crossing state and event delivery separately. Missing,
  stale, untrusted, or interrupted readings are not a threshold crossing or zero.
  Scheduling another snapshot needs current Mother authority on every invocation.
- **Pure calibrate/validate or summary component:** extract only if multiple
  Children reuse it. Feed immutable bounded readings and a calibration digest;
  no Toy imports. Keep hardware decoding/driver safety at the host boundary.
- **NIC-111 health probe / NIC-113 config preflight:** share quantity/provenance
  data where useful; a health probe is not permission to access arbitrary pins,
  and a preflight descriptor check is not permission to apply configuration.
- **Pure fleet reducer:** reuse NIC-110 fan-out merging and receipt verification
  support rather than embedding seven-node scheduling or inference in this Child.
- **Separate serial protocol Child:** only if multiple protocols justify it.
  This would require a new narrowly framed serial Toy, independently issued grants,
  and explicit typed composition. It must not inherit or lend the sensor Child's
  token. Firmware upload and GPIO actuation remain separate designs.

Suggested NIC-115 order after Doug's approval: shared contract/runtime/artifact
prerequisites → local input-only GPIO and one sensor vertical slice → crash,
revocation, offline and seven-target proofs → firmware serial-leaf stretch →
optional WAMR/wasm3 and stronger leaf authentication. This is not a change to
NIC-104's approved four-piece sequence or its next-Toy HTTPS priority; Doug decides
how NIC-109 work fits that build order.

**NIC-110 review checklist / build blockers:**

1. Versioned shared envelope/WIT binding and explicit distinction between domain
   coverage, call execution, receipt verification, and delivery state.
2. Signed terminal receipt and ledger proof binding, including recovered failures,
   unavailable target receipts, signer trust/rotation and bounded receipt size.
   The draft lacks explicit assignment and Mother clock-quality fields: bind them
   through an approved host-evidence extension or the independently verified
   ledger anchor; do not imply they are already first-class envelope fields.
   Also define how its BLAKE3 artifact digest binds the existing SHA-256 approval
   identity without weakening exact-artifact checks.
3. Fixed-target fan-out, stable attempts, late-result merge revisions, safe partial
   failure and no implicit routing to the wrong physical hardware.
4. Durable result outbox/intent queue, logically atomic evidence/result publication,
   reservation/backpressure, export authorization, ACK/dedupe and retention rules.
5. Precompiled trusted `.cwasm` path including import discovery, Pi 3-compatible
   compiler profile and fail-closed mismatch behavior; no local compile fallback.
6. Per-profile/aggregate memory and I/O deadlines, host buffer/count accounting,
   queue quotas, and budget enforcement outside guest linear memory.

NIC-115 additionally needs catalog admission, resident import selection and an
exact endpoint-descriptor authority binding for its **new sensor Toy**. No source
code, manifest, approval, grant, or daemon configuration is changed by this design.

## 8. Pi 3 acceptance test (future execution, not run here)

Primary pass requires the actual Pi 3 (~905 MB RAM, aarch64 Debian), not only a
larger Pi or Wasm fixture. Use a reviewed 3.3 V-safe input switch/jig with external
bias/current limiting and a real sensor with documented tolerance; no actuator,
5 V GPIO input, or floating line. Record board/kernel/driver versions, wiring,
sensor calibration/tolerance, runtime/config and artifact digests, and authority
setup in the eventual evidence. Hardware/power-cut ceremonies stay manual.

| Test | Pass evidence |
| --- | --- |
| **A. Artifact and resource gate** | A nicamaster-built, approved Pi 3 `.cwasm` loads without any Wasm compilation on the Pi, including discovery. Wrong runtime/CPU configuration, modified bytes, and untrusted provenance are rejected without compile fallback. Record 30 cold and 100 warm invocations, p95 startup, total latency, peak guest memory, Mother RSS/PSS delta and no OOM/swap pressure; all §5 bounds hold. |
| **B. Useful GPIO + sensor read** | Through ordinary Mother ingress, flip the physical input manually and observe both states with correct active-low interpretation; verify sensor values against the fixture tolerance. Return ordered typed channels, units, timestamp/freshness and descriptor attribution, not parsed CLI output. Verify each result digest, Mother signature and ledger anchor independently. |
| **C. Narrow authority** | Fresh calls with missing, expired, revoked, wrong caller/Vision/artifact/assignment/resource grants cause zero device I/O. Missing pre-admission required grants cause zero Child starts. Revoke/change descriptor after token mint: effect-time revalidation denies the next I/O. Restart does not restore revoked access. Unknown endpoint must not disclose topology. |
| **D. Read-only boundary** | Reject caller-supplied paths, register/write commands, extra channels, out-of-range scales, duplicate IDs, invalid direction, claimed lines and oversized payloads. Hardware observation shows no output, reset, pinmux or pull changes. A deliberately malicious guest cannot exceed one Toy exchange, channel limits, endpoint rate or deadline. |
| **E. Partial measurement** | Disconnect a sensor or force a stale channel while another remains valid. Preserve successful readings and explicit error slots, never zero-fill. Domain `partial`/`none` is distinguishable from execution denial and unverified delivery. |
| **F. Drop/reboot matrix** | Disconnect the caller and interrupt the Pi at each §6 boundary (before I/O, after I/O before commit, after terminal commit before receipt/ACK). Verify surviving ledger chains/torn-tail handling, one terminal disposition per admitted attempt, no automatic resample, and identical committed bundles on retry. No successful reading is fabricated for interrupted evidence. |
| **G. Offline and backpressure** | With client connectivity absent, authorized local calls queue results durably. Reboot, reconnect and hand them over with original timestamps and no duplicate logical results. Fill both quota boundaries, lose ACKs and deny export after revocation: new unreservable reads stop before I/O; retained evidence is intact. Test retry beyond 12 minutes and after configured retention expiry without resampling. |
| **H. Fleet answer** | One plan addresses six Pi 3s plus nicamaster with target-pinned endpoint references. Make one target unreachable, one deny, and one return a channel error. Produce seven stable slots and one bounded merged answer by the deadline; valid peers survive partial failure, missing target receipts remain missing, attribution verifies, and a later handover updates the merge revision. No local fallback or multi-hop call. |
| **I. Serial-leaf stretch** | Repeat B–G through a real ESP32 or Pico firmware leaf using the same Child artifact/WIT. Exercise unplug/replug, leaf reset, stale/wrong nonce, duplicate sequence, CRC failure, oversized/truncated frame and wrong device binding. Deadlines release the port; no replay, reset/actuation or fake trusted timestamp occurs. Verify the Pi—not the leaf—holds grants and signs the MCT receipt. |

A–H are required for the built Pi design; I is a separately reported stretch gate,
not a claim that WAMR/wasm3 or authenticated leaves were tested. Source/unit tests
alone cannot satisfy B, F, H or I. If a bound fails, report the measurement and
revise the design for review—do not quietly raise caps or move work to a larger Pi.

### Design-only validation record

- Read the source boundaries and NIC-104/NIC-109/NIC-110/NIC-115 requirements;
  reconciled the parallel NIC-110 draft without editing sibling files.
- Extracted the WIT fence into a temporary package with that shared dependency;
  `wasm-tools component wit` passed in text-only mode. This proves WIT syntax and
  type references, not bindings, host integration, budgets or hardware behavior.
- Checked all nine repository-relative links, required design fields, code fences,
  whitespace and the repository comparative-vocabulary check.
- No Rust build/tests, artifact compilation, deployment, fleet commands or hardware
  tests ran. A–I above are future evidence obligations, not results of this change.

**This change's pass/fail:** all requested design fields are present in this file,
reviewed against the cited source and NIC-110 assumptions, committed on
`nicabar-nic115-design`, and that branch pushed to origin. No Rust implementation,
build, deploy, fleet operation, PR, or modification of `main` belongs to this task.

# NIC-112 log and journal summary Child

**Status.** Design proposal for Doug's review. No implementation, build,
deployment, fleet probe, or measured performance is claimed here.

An agent needs a small, attributable answer about recent service failures on a
Raspberry Pi, not an unrestricted shell or a model running on the device. This
Child reads a bounded tail, applies explicit filters, and returns counts and
selected evidence. Mother owns admission, receipts, storage, and delivery.

| Required field | Decision |
| --- | --- |
| Target | Raspberry Pi 3, about 905 MB RAM, aarch64 Debian Mother. |
| WIT world | Proposed `patina:log-summary@0.1.0`, world `child`, export `summary.summarize`. |
| Inputs | Registered journal/file source IDs, exact units, fixed time window, severity selection, tail/summary mode, bounded limits. |
| Outputs | Typed per-source coverage, counts, bounded message groups and tail samples, wrapped in the NIC-110 result envelope with a Mother-signed ledger receipt. |
| Toys | **New** bounded `log-read` Toy, with **new** journald and live-log-file backends. Existing directory-read is a snapshot-only alternative, not a journal reader. |
| Memory/startup | Target 8 MiB guest peak; 16 MiB guest ceiling; at most 32 MiB incremental resident memory including host work. Cold start p95 at most 500 ms; warm at most 100 ms. Proposed gates, not measurements. |
| Profile | `read-compute`. One bounded operation, fresh instance, no resident loop or Child-owned persistent state. |
| Composition | Source reader plus pure summary reducer; fleet merge on the coordinator; optional health-probe and threshold-watcher consumers. |
| Acceptance | Section 9 defines future Pi 3 and seven-node pass/fail checks. |

## 1. Fit the existing Child plan

Use NIC-104's canonical bounded app operation. Preserve the separate
`ComponentArtifact`, `ChildApproval`, `ChildAssignment`, and `ChildInstance`
facts. A manifest `needs` entry requests a Toy and never grants it. The
read-compute profile describes responsibility, not a requirement to copy
repo-report's filesystem imports into a journal reader.

The inspected baseline is `bc48d1e`. Sources distinguish implemented behavior
from prerequisites for this design.

| Evidence | Consequence |
| --- | --- |
| [Canonical Child consult](https://github.com/nicabarnimble/patina-mct/blob/f69b163e4d0d868315449772cd119a3955446504/docs/mct-vision-child-plan.md), section B, and [NIC-104](https://linear.app/nicabar/issue/NIC-104) | Use a typed, bounded successor pattern. The consult was read from `origin/nicabar-nic105-drift-report`; it is not in this branch's baseline. Do not copy broad fixture worlds or fake a Watch identity. |
| [First application plan](../../plans/first-real-app.md) and [repo-report WIT](../../../crates/mct-repo-report/wit/world.wit) | Read-compute is proven with an immutable file snapshot. The current string/JSON report ABI is not the proposed typed log ABI. |
| [Wasmtime runtime](../../../crates/mct-daemon/src/wasm.rs), `DEFAULT_WASM_MEMORY_LIMIT_BYTES`, `store_limits`, `compile_snapshot`, `configure_deadline` | Default linear-memory limit is 64 MiB; epoch deadlines exist. `compile_snapshot` calls `Component::from_binary`. A trusted `.cwasm` load path is **not implemented here**. The memory limit is not a cap on Mother RSS or all host allocations. |
| [Invocation budgets](../../../crates/mct-daemon/src/wasm/budget.rs) | Defaults include 4 MiB cumulative accounting, 64 KiB host values, 256 KiB output, 1,024 Toy calls. Accounting includes container/serialization overhead, not just message text. Exhaustion is sticky. |
| [Toy authority](../../../crates/mct-kernel/src/toy.rs) and [effect adapter](../../../crates/mct-daemon/src/toy.rs) | Exact subject/scope evaluation and fresh per-effect admission exist. No journal backend exists. `max_uses` cannot supply metering because current effect admission rejects it with `ConsumptionStateUnavailable`. |
| [Supporting Toys](../../../crates/mct-daemon/src/daemon/watch.rs) and [ordinary execution adapters](../../../crates/mct-daemon/src/daemon/resident/execution.rs) | Directory-read, logging, measure and bounded Watch storage exist. Logging is a write sink, not permission to read host logs. Ordinary directory-read imports have a strict allowlist. New log imports need deliberate runtime wiring. |
| [Security boundaries](../../SECURITY-BOUNDARIES.md), [D-32](../../DECISIONS.md), [durable-task proposal](../durable-task-boundary.md) | Restart settles interrupted calls; it does not resume them. Short-lived dedupe is not permanent job storage. Signed per-call result receipts and durable offline handover remain NIC-110 dependencies, not guarantees inferred from a hash chain. |

[NIC-110](https://linear.app/nicabar/issue/NIC-110) owns the shared contract.
The parallel draft was read from `nicabar-nic110-design` at `5486458`, including
`docs/design/small-device/nic-110-shared-contract.md` and `wit/shared.wit`.
It declares `patina:small-device@0.1.0-draft`. This document follows that draft;
those files are not copied into this branch. Reconcile further draft changes
before build. This design does not wait for NIC-105 or change NIC-104's build
order. NIC-110's sibling table suggests directory scope for files; this proposal
uses a narrower live-file backend and retains directory-read for snapshots.

## 2. Scope and result meaning

### Read a finite tail, not an unbounded window

A request names one to four registered sources. Each source resolves through
Mother configuration to exactly one journal namespace/unit allowlist or one
regular log file with a versioned parser and optional trusted unit mapping.
There is no caller-supplied path, glob, command, arbitrary journal expression,
regex, or auto-discovery of all services.

The caller supplies `[since, until)` in UTC microseconds. A coordinator resolves
"last 15 minutes" once before dispatch, so all nodes receive identical bounds.
Default UI choice is 15 minutes; the maximum is 24 hours. The Child has no clock
import. Journal boot selection is current boot by default or one explicit boot
ID. Cross-boot searches require separate calls and visible coverage.

For each source, read newest first from an opening high-water mark. Apply unit,
time and severity filters; stop at the first exhausted source quota, global
quota, deadline, or window boundary. Return selected records oldest first for
reading. A journal uses its stable cursor ordering; a file uses byte offsets.
Ties in rendered time use source ID and source position. Clock movement does
not justify stopping early unless the backend can prove the window is exhausted.

Counts are exact **over the selected bounded records**, never estimates of all
messages in the requested window. `complete-window` is allowed only when the
backend proves it exhausted the eligible retained window. Otherwise report
`bounded-tail`, with the stopping reason and observed range. Distinguish
retention gaps from a successfully read empty window. No silent pagination or
background continuation turns a bounded call into an unlimited scan.

### Make filters and unknown values explicit

- Journal severity is syslog priority 0 through 7. `max-priority = 4` includes
  emergency through warning. Missing/invalid priorities are `unknown`, included
  only by `include-unknown-priority`. `None` includes every known priority.
- Unit selection uses trusted journal fields such as `_SYSTEMD_UNIT`, not an
  application-written `SYSLOG_IDENTIFIER`. User-unit sources require a separate
  configured mapping. An empty unit list means all units allowed by that source,
  never the entire host. A requested unit outside the allowlist is denied.
- File v1 supports configured JSON Lines with explicit timestamp/priority/unit
  fields and plain UTF-8 lines. The JSON parser schema and trusted source-to-unit
  binding are pinned by configuration. Plain lines have unknown event time and
  priority. File mtime is not an event timestamp. Unknown-time records are omitted
  unless `include-unknown-time` is true, then counted separately from the window.
- Malformed JSON, invalid UTF-8, absent fields and a cut final line are counted.
  They do not become invented severities, inferred timestamps or concatenated
  multiline incidents. Oversized records are skipped and counted; no unbounded
  line allocation or decompression is allowed.

Summary means deterministic reduction, not inference. Count by source, trusted
unit and priority. Group up to 32 exact, sanitized, bounded message values using
an algorithm/version recorded in the report. The first 32 distinct values in
newest-first traversal get slots; later new values increment `other-count`.
Existing groups keep accumulating. This is not a global top-k algorithm and
must not be labelled "top errors". Equality is checked on retained bytes, not
just a hash. Messages with clipped prefixes may merge and are labelled as such.
No cause diagnosis, embeddings, git, builds, network fetches or service actions
belong in the Child. Git Children remain macOS-only on Studio/Lume.

## 3. WIT world, inputs and outputs

The following is a **design sketch**, not a new checked-in WIT package or a
compiled contract. It uses NIC-110's typed-body-plus-coverage option. Mother
wraps the answer rather than asking the guest to invent node identity, terminal
status, ledger positions or signatures.

```wit
package patina:log-summary@0.1.0;

interface summary {
    use patina:small-device/common@0.1.0-draft.{coverage};
    enum mode { tail, summary, both }
    enum source-kind { journal, file }
    variant boot-selection { current, explicit(string) }
    record source {
        id: string,
        kind: source-kind,
        boot: option<boot-selection>,
    }
    record window { since-us: u64, until-us: u64 }
    record limits {
        records-per-source: u32,
        scan-bytes: u32,
        scan-records: u32,
        output-bytes: u32,
        duration-ms: u32,
    }
    record request {
        sources: list<source>,
        units: list<string>,
        time: window,
        max-priority: option<u8>,
        include-unknown-priority: bool,
        include-unknown-time: bool,
        mode: mode,
        limits: limits,
    }
    enum source-coverage { complete-window, bounded-tail, unavailable }
    enum stop-reason {
        window-end, source-end, record-limit, scan-limit, output-limit,
        deadline, retention-gap, source-changed, denied, unsupported, io-error,
    }
    record count {
        unit: option<string>,
        priority: option<u8>,
        value: u32,
    }
    record sample {
        position: string,
        event-time-us: option<u64>,
        unit: option<string>,
        priority: option<u8>,
        message: string,
        clipped: bool,
    }
    record message-group { exemplar: sample, occurrences: u32 }
    record source-report {
        source-id: string,
        source-generation: option<string>,
        parser-version: option<string>,
        source-policy-digest: option<string>,
        coverage: source-coverage,
        reasons: list<stop-reason>,
        observed-range: option<window>,
        selected: u32,
        scanned-records: u32,
        scanned-bytes: u32,
        filtered: u32,
        malformed: u32,
        oversized: u32,
        unknown-time: u32,
        unknown-priority: u32,
        counts: list<count>,
        groups: list<message-group>,
        other-count: u32,
        tail: list<sample>,
        omitted-samples: u32,
        omitted-groups: u32,
        selected-digest: option<string>,
    }
    record report {
        algorithm-version: string,
        requested-window: window,
        effective-limits: limits,
        sources: list<source-report>,
    }
    record answer { body: report, coverage: coverage }
    enum request-error { invalid-request, unsupported-version, budget-too-small }
    summarize: func(input: request) -> result<answer, request-error>;
}

world child {
    import patina:log-read/pages@0.1.0;
    export summary;
}
```

The new imported interface exposes `read-page(query) -> result<page, read-error>`.
Its typed query names the registered source, fixed window, units, priorities,
opening boot/generation and optional opaque continuation token. Its typed page
contains bounded normalized records, scan/filter/error counters, opening
high-water mark, source generation, observed range, stopping reasons and an
optional next token. A record carries source position, optional event time,
trusted unit, optional priority, bounded message bytes and truncation flags.
`read-error` distinguishes denial, unsupported source/parser, stale cursor,
source change, I/O failure, deadline and budget exhaustion. This page schema is
part of the proposed new Toy contract, not JSON parsed from a subprocess.

Tokens are Mother-held or authenticated opaque locators, at most 512 bytes.
They bind source generation, query digest and invocation, expire with the call,
and confer no authority. Reusing a token cannot reset quotas or switch sources.
The final report does not expose a reusable continuation token. Position strings
are sanitized opaque evidence locators, not paths or capabilities.

At admission, validate source IDs (at most 128 bytes), at most eight exact units
(each at most 256 bytes), window ordering, priority range and all list/byte
bounds before allocation or host access. Reject duplicate source IDs and a boot
selector on a file source. WIT integer/list types alone do not enforce these
bounds. Missing metadata is represented by `option`, not empty strings that look
like known values. `observed-range` encloses known selected timestamps only.
Bound generation, parser version, boot and evidence-position strings at 256
UTF-8 bytes; digests use the shared 64-character form. At most 81 nonzero count
bins (eight allowed units plus unknown, eight priorities plus unknown) and one
of each stop reason are returned per source. Configured unit sets also obey the
eight-unit bound when the request list is empty. Counters for malformed,
oversized and unknown fields are diagnostic subsets and may overlap; do not
sum them as disjoint totals. `filtered` counts examined records rejected by the
query. `selected` counts accepted records actually reduced.

### Bind to the four NIC-110 shared records

Register input schema `patina.log-summary.request.v1`, output schema
`patina.log-summary.report.v1` and merge schema `patina.log-summary.fleet.v1`.
Project the WIT body field-for-field into NIC-110 `common.typed-payload` using
its deterministic CBOR encoding. Options become null/value, enums kebab-case
text and ordered lists remain ordered. Use the shared BLAKE3-256 digest and
Ed25519 receipt definitions, not a sibling-specific signing format.

Shared `coverage.examined` is the sum of examined source records. Completeness
is partial if any source is unavailable, bounded, gapped or has omitted output.
`omitted` is absent when the unread remainder is unknown; it is zero only for
proved complete coverage. Shared `continuation` is absent in v1. Full internal
scan counters are bounded-work counters, not claims about unseen records.
`effect = no-effect` means no mutation was requested, not that log reads bypass
Toy authority. Domain request errors map to shared `invalid-input` or
`budget-exceeded`; unsupported backends map to safe `unavailable`. Missing or
incompatible AOT artifacts map to `unavailable` or `incompatible-artifact`.
Additional source-policy/parser/generation claims live in the validated domain
payload or bound request, and are thereby covered by the envelope digest.

| Shared concept | NIC-112 requirement |
| --- | --- |
| Result envelope | Use `common.result-envelope` context, executor/instance/boot, outcome/reason, answer, measured usage, settlement time and audit reference, wrapped by `receipts.receipted-result`. Domain coverage remains separate from `MctResult`'s closed outcome set. Exact assignment identity and absolute start time are requested shared additions, not fields already in the draft. |
| Signed receipt | Mother signs canonical bytes binding the above identities, query/filter/parser/source-policy versions, source generations, coverage, payload digest and committed ledger reference. Include original signature bytes, algorithm/key ID and a verifiable signer identity. A digest or `audit_ref` alone is not a signature. |
| Fan-out result | Use `fleet.receipted-fleet-result` with one `received(receipted-result)` or `missing(no-reply)` slot per requested node, merged bounded report and coordinator receipt. Never fabricate a remote receipt for an unreachable node. |
| Budget declaration | Profile, guest memory, host accounting, time, page/record/scan limits and serialized output limits. Effective values are the minimum of request, admitted profile and Mother policy; a caller cannot raise a ceiling. |

A report may return useful authorized sources while another source is denied or
unavailable. The envelope can be successful with partial domain coverage. If no
source is readable, do not emit a successful empty report; Mother reports a
safe denied or failed outcome as appropriate. A trap or hard timeout is a
terminal failure, not a successful partial result assembled from guest memory.

The selected digest uses the shared BLAKE3-256 encoding and commits to the
canonical sequence actually supplied to the reducer, including flags and
source positions. It is not a digest of an entire
journal or unseen original text. The receipt proves what Mother observed and
returned, not that a compromised host or log writer told the truth.

## 4. Toys and authority

### Add one narrow read interface, with two backends

**New proposed canonical Toy:** `toy:mct:log-read`, contract
`patina:log-read/pages@0.1.0`, function `read-page`, action `read-page`.
NIC-110 review may split its backend catalog identities without changing the
summary's domain types. Both backend implementations and their linker wiring
are new work.

| Backend or existing capability | Decision and boundary |
| --- | --- |
| **NEW journal read** | Mother-side native systemd journal API, for example `sd-journal`, supplies structured fields and cursors. No `journalctl`, shell pipelines, subprocess output parsing, guest journal-directory preopen or arbitrary field enumeration. Restrict fields to timestamp, priority, trusted unit, boot, cursor and message. |
| **NEW file tail read** | Mother-side bounded positioned reads of configured regular files. Capture file identity and size; scan backward in bounded blocks. No `/var/log` directory preopen, guest path resolution, recursive search, device/FIFO/socket access, symlinks or compressed rotation expansion. |
| Existing `toy:mct:directory-read` | Suitable for a separately authorized immutable log fixture/snapshot plus a pure parser/reducer. Not selected for the live-source world because it exposes broader byte access and existing preopens are not per-page revocable. |
| Existing logging/measure | Not required imports. Mother observes execution and budgets; the guest must not copy source messages into a logging sink. |
| Existing keyvalue/messaging | Not required. Offline retention is a Mother/coordinator contract, not fake Watch state or an extra ambient Toy grant. |
| Signing, network, administration | Mother responsibilities. The Child receives no signing keys, raw Iroh endpoint, SQL handle, secret Toy or service-control Toy. |

Use exact `ToyGrantSubject` fields for child name, artifact ID/version,
assignment and caller node. Fill `ToyGrantScope` with the Vision, executing
node, project if applicable, data classification, **one registered source ID**
and `allowed_actions = [read-page]`. Issue separate source grants rather than
leaving `resource_id` or caller fields as wildcards. Require exclusive expiry,
`starts_at`, locality and duration bounds. Set `max_uses = None`; invocation
budgets meter pages, not an unsupported consumption counter.

The existing grant shape has no unit-list, parser, window or byte-limit fields.
Bind those restrictions through a versioned, Mother-owned source policy keyed
by the exact resource ID. Changing that mapping requires authorized policy
revision and invalidates old tokens. It must never silently widen a file alias
or a journal unit set. Bind the policy version/digest into observations and the
result. This policy binding is **new design work**, not a free-form grant field
already interpreted by the kernel.

Mother must validate source grants before linking/admission and mint a fresh
`AuthorizedToyCall` for each page/resource. The new dispatcher must support the
approved resource set, returning typed denial for another requested resource;
it must not install an always-success stub to satisfy an import. With no
admitted source, fail before Child execution. Check current authority at every
page effect using the existing snapshot/ordered-admission model, not a cached
allow. Revocation blocks the next admission; it cannot retract bytes already
returned to the Child. Recheck release authority before handing results out.

OS access is separate from Toy authority. Give the trusted backend only the
configured journal/file access it needs; do not run the guest as root or treat
journal-group membership as an MCT grant. On systems without access or without
journald, return denied/unsupported, not shell fallback.

### Bound mutable and hostile sources

A journal read fixes its boot and high-water cursor. Vacuum or cursor loss
returns a gap, never an implicit restart at the current tail. Appends after the
opening cursor are excluded. A file read pins an open descriptor, device/inode,
opening length and change metadata. Rename rotation keeps reading the pinned
file, while truncation or detected in-place mutation marks `source-changed`.
Do not follow the replacement name mid-call. For strict repeatability, require
an immutable snapshot; metadata checks cannot prove a live file was never
rewritten during the read.

Journal field thresholds limit returned data, not necessarily native mmap or
compressed-entry allocation. The future adapter must bound native working
memory and have a cancellable/isolated read boundary. Epoch interruption of the
guest alone cannot stop a blocked host library call. A backend that cannot
honor memory and stop-delay gates is unavailable for this profile until that
boundary is proven. This is an explicit implementation feasibility gate.

Source policy drops non-allowlisted fields and applies configured sanitization
before returning message bytes. Default release policy allows counts only;
groups with exemplars and tail text require explicit source-policy allowance.
Without it, return no groups/tail and record their omission. Cap each text value
at 256 bytes, escape controls for rendering and label clipping. Logs can still
contain secrets or prompt injection. No sanitizer is claimed to make arbitrary
logs public or safe instructions. Keep report classification at least as
restrictive as every contributing source and authorize its audience separately.
Canonical ledger facts carry identities, digests, counters and safe failure
codes, not raw message bodies. Protected result storage holds any retained text.

## 5. Budget and precompiled artifact profile

These are proposed admission ceilings and acceptance targets. They need
measurement after approval, not larger limits disguised as implementation
convenience. NIC-110 owns the final declaration format.

| Resource | Default / ceiling | Why |
| --- | --- | --- |
| Guest linear memory | Target at most 8 MiB; hard 16 MiB | Streaming pages and bounded counters need no full-journal buffer. Require one guest memory or an aggregate limit; a per-memory Wasmtime limit alone does not bound a multi-memory component. |
| Incremental resident memory | At most 32 MiB over idle Mother, including native journal work, ABI copies and mapped resident pages | Separately measures host cost; the 64 MiB guest cap is not a process RSS guarantee. One active log-summary call per Pi 3. |
| Start latency | Warm p95 at most 100 ms; cold p95 at most 500 ms | Admission-to-export-entry, including artifact verification and instantiation but excluding network/queue time. No JIT compilation hidden in cold start. |
| Execution | Soft stop at 1,500 ms; hard deadline 2,000 ms including startup; host stop delay at most 100 ms | Soft stop reserves time to finish a bounded report. Hard deadline/trap returns a receipt-bearing terminal outcome, not a fabricated report. Earlier caller/grant expiry wins. |
| Sources / units | At most 4 / 8 | Bounds source reports and count cardinality, including unknown unit/priority bins. |
| Selected records | Default and ceiling 64 per source; at most 256 overall | Counts and summary apply to this bounded tail; shared `max-items = 256`. |
| Scan work | At most 1 MiB logical source bytes and 4,096 examined records per invocation | Bounds work even when filters match nothing. Metadata and rejected/oversized records count. Divide quota equally between sources before reading; no starvation by the first source. |
| Toy pages | At most 8 per source and 32 total; at most 16 records and 4 KiB serialized normalized data per page | Incremental processing. Page and scan counters live with Mother and cannot reset through new tokens. |
| Individual input record | At most 4 KiB returned fields/log-line bytes before sanitization; retained text at most 256 bytes | Larger records are skipped with an oversized counter. Include all bounded fields in page sizing. Native allocation is independently gated above. |
| Host calls / Toy calls / observations | 64 / 32 / 80 | Two Toy facts per page plus invocation and terminal headroom. No guest logging/measure imports. |
| Host value accounting | 16 KiB per value; 1 MiB cumulative invocation ceiling | Use NIC-110 limits with existing container/serialization accounting. A 4 KiB wire page may have a larger dynamic representation; reduce page records until both bounds fit. Stop before exhausting reserved terminal capacity. |
| Structured input | At most 32 KiB, nesting at most 16 | Shared input limit; source/unit bounds normally keep requests much smaller. |
| Serialized application output | Default and maximum 32 KiB; this is request `output-bytes` | Match shared `budget.output-bytes`. A lower requested limit must fit a minimal report or return `budget-too-small` before reading. |
| Signed terminal package | At most 48 KiB, including the application output; receipt at most 8 KiB | NIC-110 reserves host metadata/receipt space separately from guest output. |
| Tail and groups | At most 32 tail samples and 32 groups per source, further reduced by the output cap | Counters/coverage take precedence over text. Omission is explicit. |
| Fleet output | At most 512 KiB for seven nodes, including coordinator envelope and receipt; domain merge at most 32 KiB | Follow NIC-110. Seven intact 48 KiB node packages fit with bounded coordinator metadata and merge; validate the full reservation before dispatch. |

Map these limits into shared `common.budget`; set `logging-calls = 0` and
`measure-calls = 0`, `startup-ms = 500` and `wall-ms = 2000`. Request `limits`
additionally narrows log-specific work. The Toy stops admitting pages at the
soft cutoff and returns a deadline stop reason so the clock-free Child can
finish its report; Mother enforces the hard deadline independently.
The minimum useful admission reserves one source report, one read page, its
Toy facts and a terminal package. NIC-110 review must freeze that schema-sized
minimum vector; any smaller caller budget is refused, never partially admitted.

Output limits apply after escaping and canonical serialization. Drop optional
samples/groups deterministically, then set omission counters and `output-limit`.
The tail contains the newest selected samples that fit, rendered oldest first.
`omitted-groups` counts dropped groups; `omitted-samples` counts omitted tail
samples. `other-count` remains the number not assigned to a group during
reduction. Counts and selected-record totals survive output trimming.
Never truncate JSON bytes, a signature, an identity or a counter record. If even
mandatory fields do not fit, return the shared minimal budget failure. Mother
must reserve terminal observation/receipt capacity outside exhausted guest
output so denial and timeout remain recordable. Disk/signing failure cannot be
represented as a valid signed success.

Build and precompile on **nicamaster only**, after approval. NIC-110 must bind
source component digest, compiled artifact digest, manifest/WIT digest, trusted
builder identity, exact Wasmtime version/configuration, target triple and Pi 3
CPU features. Sign/approve that mapping before native artifact deserialization.
Do not use nicamaster's native CPU feature set by default. Verification, import
inspection and WIT discovery on a Pi must also avoid a compile-on-inspection
fallback. A missing, corrupt, untrusted or incompatible `.cwasm` fails closed
with shared `unavailable` or `incompatible-artifact`; it never compiles `.wasm`
on the Pi. Artifact approval and source provenance remain distinct facts.

## 6. Signed receipts, interruption and offline handover

One accepted call attempt has one terminal ledger result and one corresponding
signed receipt, including denied, failed, timed-out and cancelled outcomes.
Malformed transport bytes that never become a call are not invented invocations.
Toy page facts are linked beneath the call; they are not substitute per-call
receipts. Receipt key custody and canonical signing format follow NIC-110's
purpose-bound Ed25519 key, canonical deterministic CBOR statement and BLAKE3
envelope digest.

Proposed durable ordering is admission/dispatch fact before execution, then
protected result bytes committed before their terminal digest reference, then
a durable terminal fact and signed receipt before acknowledging success.
Recovery may finish signing an already committed terminal fact without running
the Child again. It must never sign a terminal success inferred from an allow.
Reconcile a result blob without a terminal fact as uncommitted/orphaned, not as
success. Define the ledger/result-store crash protocol in NIC-110 rather than
assuming an atomic transaction across unrelated files.

| Interruption point | Safe outcome |
| --- | --- |
| Queued, not admitted | Retain the request and fixed time bounds. Revalidate deadline, artifact and all authority before later admission. Expired work is not silently shifted to a newer window. |
| Dispatch/page started, no terminal fact | On restart settle interrupted/unknown using durable evidence. No automatic reread and no claim that the guest resumed. A fresh observation requires an explicit new attempt ID and current authority. |
| Terminal result and bytes durable, acknowledgement lost | Reacquire the same immutable result and receipt by logical job/attempt identity. Do not read the now-changing logs again. |
| Digest retained, result bytes missing or expired | Return result-unavailable with retained evidence. A matching digest alone cannot reconstruct the report. |
| Network drops after local admission | The node may finish within its existing deadline and store its result for later delivery. Disconnect never extends grants, retries reads or authorizes another node to take over. |
| Ledger writer, disk capacity or signer unavailable | Do not start new reads without reserved durable capacity. Do not acknowledge completion until it is durable and verifiable. Recovery exposes an honest unavailable/interrupted state. |

Proposed assignment subquotas are 32 pending requests, 64 retained results and
8 MiB total payload/metadata, within NIC-110's per-Mother 256-record/16 MiB
queue ceiling and separate 1 MiB settlement reserve. Queued unstarted work has
a 24-hour age limit; an earlier absolute deadline wins. Reserve terminal slots
before admission. On overflow, refuse new admission with safe `unavailable`
and a ledger queue-full diagnostic, not an invented terminal reason enum.
Unacknowledged results never expire automatically. Retain until durable ACK or
explicit owner-authorized discard with a tombstone. After ACK, retain results
for at least seven days, then retain bounded identity tombstones so a retry
cannot trigger a fresh read. These are queue quotas, not ledger-size limits;
full result/tombstone storage backpressures admission.

A node can serve a locally authorized caller while disconnected. Undelivered
remote requests remain with the coordinator; an offline node has not received
or executed them. Handover sends the original signed bytes only to the bound,
currently authorized recipient. Revoked export authority holds delivery until
authorized again or an explicit owner-authorized discard; it does not send
because the original read was allowed. Delivery ACKs
and duplicate receipt IDs make handover idempotent beyond the current 12-minute
call dedupe TTL. No exactly-once execution, global revocation during a partition,
or recovery from total storage loss is promised.

## 7. Fleet fan-out and one merged answer

The coordinator creates one logical job with seven fixed target slots, the six
Pi 3 Mothers plus nicamaster. It sends independent single-hop calls under each
node's own authority. Target identity is mandatory for node-local logs; failure
on one node must not route its request to another node's journal. No new graph
scheduler, cross-Vision bypass or shared fleet credential is required.

Use identical fixed windows and algorithm/parser-policy versions where
available. Allow at most two node calls concurrently and one active call on
each Pi 3, with a 15-second interactive fan-out deadline. Offline jobs have a
separate explicit expiry, never an extension of an admitted execution deadline.

The merged result follows NIC-110's node-ID ordering and success/unsuccessful/
missing counts. It preserves each received original envelope and receipt,
per-source coverage, clock/boot provenance and schema version. Verify each
receipt and payload digest before including its counts. Invalid receipts are failures, not usable evidence.
For a node that never replied, the coordinator signs its own `reply-deadline`
or `unreachable` observation; the remote receipt remains absent. Set
`may-have-executed = true` after an uncertain send or lost reply. Late delivery
creates a new version of the merged answer linked to the original, not a rewrite
of a signed answer. Duplicate attempts/results are not double-counted.

Merge only compatible counter/group versions. Represent incompatible results
with the shared `incompatible-result` missing reason; retain quarantined bytes
separately, never count them. Trim optional samples only in the newly produced
domain merge, never mutate a received signed package to meet fleet limits.
Retain node/source provenance on every exemplar. Cross-node totals are labelled "observed selected records
from N of 7 nodes", with partial-source flags. Do not claim a fleet-wide global
tail or total incidents; a forwarded message in two logs is two observations,
not a proved duplicate incident. Unknown timestamps and clock skew remain
visible rather than implying a synchronized causal ordering.

## 8. Composition candidates and alternatives

Keep the first deployed Child as one streaming read-compute operation. Within
it, use a pure `normalized records -> bounded report` reducer. This limits ABI
copies and start costs while preserving a separable data boundary.

| Candidate | Recommendation |
| --- | --- |
| Journal/file reader plus pure reducer Child | Share normalized record/page types. Split the reducer into a separately placed `pure` Child only when reuse or measured placement benefits justify extra serialization. Retained input and policy versions enable deterministic replay of the reducer, not replay of live reads. |
| Pure fleet-report merger | Compose on the coordinator/nicamaster, not once per Pi. Verify receipts in Mother's trusted boundary before passing verified typed reports to a pure merge component. Bound both intermediate and final bytes. |
| NIC-111 health probe plus this Child | Correlate service health with recent messages in the caller's answer. Keep separate grants/results and receipts; a health grant does not grant log access. |
| NIC-114 threshold watcher plus summary | Watcher requests a fresh bounded summary on a crossing, with a separate authorized call. Dedup/scheduling belong to the opt-in stateful-event component, not a hidden follow loop here. |
| NIC-116 guarded action | A human/agent can use the report as evidence for a separately authorized dry run. No automatic restart from a message pattern and no inherited write authority. |
| MCU gateway stretch | No Mother on an MCU. This Child needs OS journal/file facilities. Future serial telemetry can feed normalized records through a separately admitted gateway Toy; do not assume device identity equals the Pi's node identity or add serial authority now. NIC-115 owns that design. |

Rejected alternatives are shell scraping (`journalctl`, `tail`, `grep`), a broad
filesystem preopen, LLM summarization on a Pi, and a resident log collector.
They respectively discard structured/source authority, broaden access, violate
the small-device role, or add persistent state before the bounded operation is
proven. The price of the narrow Toy is new native adapter and policy wiring;
that cost is explicit, not hidden inside an existing logging grant.

## 9. Pi 3 acceptance test plan

**Future gates, not executed in this design task.** After Doug approves and
NIC-110 is reconciled, build with Sol high/xhigh on nicamaster. Use the ordinary
Mother call path and real Pi 3 hardware for acceptance, not SSH workload
execution or a faster host standing in for a Pi. Hardware power interruption is
an operator-approved manual ceremony. Keep synthetic fixture messages free of
personal data and credentials.

Record hardware/RAM, OS, runtime/config/CPU target, component and `.cwasm`
digests, source-policy/parser versions, exact requests, raw structured results,
receipts, ledger verification and measurement method. Every row must pass.

| Gate | Fixture or fault | Pass predicate |
| --- | --- | --- |
| Canonical operation | Fresh approved/assigned Child, one journal source and one JSONL source with exact grants | Typed result and valid Mother receipt through ordinary calls; no fake Watch scope, lifecycle loop, shell, inference or Git. A missing grant set yields no runtime start. |
| Filters and known answer | Journal fixtures in two units with all eight priorities, boundary timestamps and two boots; JSONL and plain-line files | Exact expected counts for unit, `[since, until)`, boot and severity rules; unknowns and parser failures match fixture counts. A readable empty window differs from denied/unavailable. |
| Bounded-tail honesty | At least 10,000 records, rare/no matches, oversized entries, huge unterminated line, invalid UTF-8 and escaped controls | Scan/page/record limits hold even with no matches; counts describe only selected data; truncation/gaps/unknowns are explicit. No unbounded allocation or claim of a complete window after a quota stop. |
| Output pressure | Four sources, maximum count cardinality, 32 groups/source, escaped text, minimum accepted output budget | Application output stays within 32 KiB and signed package within 48 KiB or stricter admission limits; omitted samples/groups are counted; mandatory fields/signatures are intact. Too-small budgets fail before reads. |
| Authority and confinement | Wrong caller/artifact/Vision/resource, requested extra unit, path traversal/symlink/device source, revoked/expired grant between pages | No unauthorized source bytes or inventory leak. Next page admission fails after revocation; held data is not released without export authority. Mother ledger records safe denial evidence. |
| Mutable sources | Rename rotation, copy-truncate, concurrent append, journal vacuum, boot change | No follow to a replacement file, no silent cursor reset, no appended records past high-water mark; changed/lost data produces source-changed/gap coverage. |
| Native read limits | Slow storage and blocked journal backend, large compressed journal entries, deadline and memory pressure | Guest and host limits both hold; host work stops within 100 ms of deadline. Failure to bound native work blocks this profile, even if guest-memory tests pass. |
| Pi 3 memory and latency | At least 30 fresh-load and 100 warm calls on a documented bounded fixture; idle Mother baseline; no swap growth | Guest peak at most 16 MiB with an 8 MiB target; incremental resident peak at most 32 MiB; cold/warm start p95 at most 500/100 ms; report or terminal timeout within 2 seconds plus at most 100 ms host stop delay. Report regressions against idle/baseline separately from network time. |
| AOT-only execution | Valid Pi 3 `.cwasm`, then missing/corrupt/untrusted/wrong-version/wrong-CPU artifact | Valid artifact runs without any compilation, including import discovery. Others fail closed. Observe no compiler fallback and preserve source-to-compiled digest binding. |
| Receipt integrity | Modify one payload byte, coverage field, request identity, source-policy digest or signature; reuse receipt from another node | Independent verifier rejects every substitution and verifies untouched signed receipts against the expected Mother identity/history. No raw source text in canonical Toy facts. |
| Reboot/drop matrix | Interrupt before admission, mid-page, after result bytes, after terminal commit and before ACK; separately drop network mid-call | Ledgers verify after restart; no inferred success or automatic reread. Committed results reacquire with original receipt; interrupted attempts remain explicit. Repeat the approved hardware power-loss case, not just process kill. |
| Offline and retention | Disconnect a node, finish locally, restart it, reconnect after more than 12 minutes; duplicate delivery; revoke export; fill queue and expire unstarted work | Durable authorized handover deduplicates by receipt identity. No authority is extended. Full queues reject safely, pending expiry is visible, unacknowledged results survive the 24-hour queue age, revoked export sends no payload and new reads fail closed without durable capacity. |
| Seven-node fan-out | Six Pi 3s plus nicamaster; one offline, one denied, one with a partial source; then all healthy | One merged answer has exactly seven target slots, accurate per-node attribution, valid receipts for received results and explicit absent receipts for unreachable nodes. No remote fallback, inflated totals or discarded healthy results; fleet package stays within 512 KiB and domain merge within 32 KiB. |

Performance counters must include native mapped resident pages and serialization
copies. Report RSS/PSS method, background services and page-cache conditions;
virtual address reservations are not resident memory. Cold start means a fresh
component load, not an unmeasured assumption about disk-cache eviction. If the
native journal backend cannot meet these gates, revise its isolation or reduce
work before considering a higher profile. Do not quietly raise the budget.

## 10. Review handoff and shared-contract requirements

NIC-112 can be accepted as a design while these build dependencies remain open.
NIC-110 gates implementation, not parallel design work.

1. Freeze the reviewed `patina:small-device@0.1.0-draft` projection and domain
   schema versions. Preserve partial coverage separately from MCT outcomes.
   Confirm the schema-sized minimum budget and seven-slot byte reservations.
   Add exact assignment identity and optional absolute start time to the
   Mother-owned context/envelope, or define their authenticated ledger binding;
   neither is currently a field in the shared draft.
2. Approve NIC-110's proposed signing authority, canonical bytes, signature
   verification, key rotation/history and minimal signed failures. Do not equate
   the existing hash chain, hello receipt or `audit_ref` with a per-call signed
   result.
3. Specify durable result/terminal/receipt ordering, crash reconciliation,
   offline queues, retention, delivery ACKs and retry identities beyond TTL.
4. Supply trusted AOT provenance/loading and no-compile import discovery on the
   Pi, plus per-profile guest/host limits and terminal budget reservation.
5. Approve the new bounded log-read Toy and source-policy authority binding.
   Prove native journal memory/cancellation and per-resource page admission.
   Journal and file reads remain separate source grants even with one interface.
6. Preserve exact target placement, per-node missing receipt semantics and
   audience checks during merge and delayed delivery. No fleet credential,
   multi-hop routing, new recovery ceremony or cross-Vision permission is implied.

Recommended NIC-112 implementation order after approval is shared contract and
AOT path, normalized fixture/reducer proof, one source backend with deny tests,
second backend, then Pi 3 budgets and seven-node failure acceptance. This is a
local dependency order, not an override of Doug's umbrella build order.

**Design completion check.** Review this file for every field in the opening
table, check its links and whitespace, commit only this document on
`nicabar-nic112-design`, and verify the pushed branch SHA. That check proves the
design deliverable, not any future runtime or hardware acceptance gate.

# NIC-113. Config preflight and drift Child

## Decision and scope

**Status. Design proposal, not approved implementation or measured acceptance.**

Give an operator one bounded report comparing declared configuration with
snapshots from six Raspberry Pi 3 Mothers and nicamaster. Before a proposed
change, check its candidate snapshot against the baseline, observed snapshot,
and explicit validation policy. Never install that candidate.

The target is Raspberry Pi 3, approximately 905 MB RAM, aarch64 Debian. The Pi
is a hand on local hardware, not a place for an agent to think. This Child does
no inference, Git, package resolution, shell execution, service restart, or
build. Git Children remain macOS-only on Studio/Lume. Compilation belongs on
nicamaster. Building this design waits for Doug's approval and Sol high/xhigh.

| Required field | Decision |
| --- | --- |
| Child and WIT world | `mct-config-preflight`, proposed `patina:config-preflight@0.1.0`, world `child`, operation `report.inspect` |
| Inputs | Digest-pinned baseline, observed snapshot, optional candidate, and bounded validation policy in one private immutable bundle |
| Outputs | Versioned structured drift/preflight report, shared result envelope, Mother-signed per-call ledger receipt, and one shared fleet result |
| Toys | Existing directory-read Toy only for v1; **new optional live-snapshot Toy** described below, not implied by v1 |
| Memory/startup | 8 MiB guest target, 16 MiB guest ceiling; 32 MiB incremental Mother RSS target; precompiled cold start p95 at most 1 second |
| Profile | `read-compute`; no persistent Child state, lifecycle loop, or event subscription |
| Composition | Snapshot inventory + pure diff + pure policy checks; client-owned fleet reduction first |
| Pi 3 acceptance | Section 9 defines future functional, authority, budget, recovery, and seven-target gates |

### Evidence boundary

This branch starts at `bc48d1e`. The design follows these inspected sources.

- [First real app plan](../../plans/first-real-app.md), especially the
  configuration-validation candidate. Supplied snapshots and pure computation
  precede separately authorized live collection or mutation.
- [NIC-104](https://linear.app/nicabar/issue/NIC-104) and
  [NIC-105](https://linear.app/nicabar/issue/NIC-105). The locked shape is a
  bounded operation, with separate artifact, approval, assignment, and instance
  facts. Preserve repo-report's JSON v1 rather than silently replacing its ABI.
- The canonical consult `docs/mct-vision-child-plan.md`, sections B.1 through
  B.5, and profile guidance `docs/src/child-development/index.md`, inspected in
  the adjacent NIC-105 checkout at `f69b163e4d0d868315449772cd119a3955446504`.
  They are not yet present in this branch at that revision. This document does
  not copy or merge the sibling's work.
- [Repo-report WIT](../../../crates/mct-repo-report/wit/world.wit),
  [component reader](../../../crates/mct-repo-report/src/component.rs), and
  [report logic](../../../crates/mct-repo-report/src/lib.rs). These establish
  bounded digest-checked read/compute, not general config parsing.
- [Wasm runtime](../../../crates/mct-daemon/src/wasm.rs), including
  `DEFAULT_WASM_MEMORY_LIMIT_BYTES`, `store_limits`, `configure_deadline_at`,
  and `compile_snapshot`; [invocation budgets](../../../crates/mct-daemon/src/wasm/budget.rs).
- [ToyGrant types and evaluator](../../../crates/mct-kernel/src/toy.rs),
  [effect adapters](../../../crates/mct-daemon/src/toy.rs),
  [supporting Toy catalog](../../../crates/mct-daemon/src/daemon/watch.rs), and
  [ordinary-call directory adapter](../../../crates/mct-daemon/src/daemon/resident/execution.rs).
- [Reliability doctrine](../../../layer/core/reliability-doctrine.md) and
  [NIC-63 mesh evidence](../../nic63-mesh-job.md). A surviving valid ledger
  frame is committed even if its acknowledgement was lost. A mesh job is not
  proof of a durable general graph scheduler.

[NIC-110](https://linear.app/nicabar/issue/NIC-110) owns the shared envelope,
receipt, fan-out result, budget, precompiled delivery, and offline handover.
The sibling draft subsequently became available and was read in full at
`docs/design/small-device/nic-110-shared-contract.md` and
`docs/design/small-device/wit/shared.wit` in the NIC-110 checkout. This design
aligns with its `patina:small-device@0.1.0-draft` types and starting policy.
The inspected draft was subsequently committed as
`5486458dab6ec760caa89995532eb7aa1a585035` on `nicabar-nic110-design`.
That is this document's shared-contract review pin. NIC-110 gates build, not
design. Do not copy its files into this branch or create a second contract.

## 1. Profile and execution boundary

Choose **read-compute**, using one scoped, read-only `/project` preopen. A call
reads immutable inputs, computes a report, returns, and discards memory.
The application client owns job identity, scheduling, comparison history, and
fleet reduction. Mother owns authority, receipts, durable handover, and
placement. The Child holds no signing key and no fleet credentials.

Start in one admitted Vision. Being in the fleet or sharing an owner does not
grant data access. Cross-Vision use stays read-only and waits for the explicit
NIC-104 sharing contract. No membership union, multi-hop forwarding, recovery
ceremony, or D-58 runtime-authority bypass is introduced here.

**V1 assesses supplied snapshots, not the current contents of `/etc`.** A
signed execution receipt proves which computation Mother admitted and its
inputs. It does not prove that a caller-supplied snapshot represents a live
machine. Reports distinguish supplied data from Mother-attested capture.
Missing capture evidence is not silently promoted to live-state evidence.

## 2. WIT world and structured protocol

Proposed domain WIT, shown for design review only. No WIT or Rust source is
added by NIC-113.

```wit
package patina:config-preflight@0.1.0;

interface report {
    use patina:small-device/common@0.1.0-draft.{coverage, reason};

    enum mode { drift, preflight }
    record request {
        operation-mode: mode,
        baseline-sha256: string,
        observed-sha256: string,
        candidate-sha256: option<string>,
        policy-sha256: string,
        change-id: option<string>,
    }
    record answer {
        report-json: string,
        coverage: coverage,
    }
    inspect: func(input: request) -> result<answer, reason>;
}

world child {
    import wasi:filesystem/types@0.2.3;
    import wasi:filesystem/preopens@0.2.3;
    export report;
}
```

The callable identity is `patina:config-preflight/report@0.1.0#inspect`.
This is a new domain package, not a change to `patina:repo-report`.
This new typed operation follows NIC-110's typed body plus shared `coverage`
option. It does not change repo-report's existing string/JSON v1. `report-json`
is the registered `mct-config-report/v1` schema, encoded as RFC 8785 canonical
JSON, not free-form text. Mother validates it and projects its exact UTF-8
bytes into `typed-payload` with `schema-id = mct-config-report/v1` and
`encoding = canonical-json`, then constructs `child-answer`,
`result-envelope`, and `receipted-result`. The string avoids a large
`list<u8>` expansion in today's dynamic WIT value representation. Generated
imports and type linking still need acceptance against the strict ordinary
adapter; shared type use does not grant a host capability.

The request projects into `mct-config-request/v1` using the snake-case fields
below (`operation-mode` maps to `mode`). Hashes are 64 lowercase hexadecimal
characters; optional fields map to JSON null. Other fields map directly.
Mother uses that canonical projection in the shared request binding. Manifest
and blob SHA-256 values are domain content identities, **not** NIC-110's
BLAKE3 envelope/request/receipt digests. Keep the algorithms explicit.

A domain report returns in `ok`, including a blocked preflight. A Child `err`
uses shared `reason`: invalid input, unavailable input, resource exhaustion,
or internal error map to `failed` with the corresponding shared reason and
no answer. Guest errors cannot assert an authoritative denial. Mother owns
`denied`, traps, timeouts and cancellation. Successful computation is distinct
from a favorable domain verdict. All NIC-113 outcomes use `effect = no-effect`
in NIC-110's mutation-certainty vocabulary; reads still require Toy authority.
There is no stdout/stderr scraping or command exit status in the contract.

### Request and immutable bundle

`mct-config-request/v1` contains the following fields.

| Field | Meaning and validation |
| --- | --- |
| `mode` | `drift` or `preflight` |
| `baseline_sha256` | Required SHA-256 of exact baseline manifest bytes; never a mutable `latest` reference |
| `observed_sha256` | Required SHA-256 of exact observed manifest bytes |
| `candidate_sha256` | Non-null for preflight; null for drift |
| `policy_sha256` | Required SHA-256 of exact validation-policy bytes |
| `change_id` | Non-null bounded correlation ID for preflight, null for drift; not authorization |

Call/job/task IDs, target-node-id, Vision, deadline, classification, and
effective-budget use NIC-110's `call-context`, not a second domain authority
model. Audience filtering remains Mother's responsibility.
A field naming a node is only a claim until Mother binds the actual execution
and, where available, capture provenance. Duplicate JSON keys, unknown fields,
invalid hashes, invalid modes, and oversize requests fail before content reads.

The bundle contains fixed `baseline.json`, `observed.json`, `candidate.json`
when needed, `policy.json`, and `blobs/<sha256>`. Each manifest is UTF-8 JSON
with schema version, configuration-set ID, revision, target class, a sorted
resource list, and capture provenance when applicable. Identifiers and keys
are at most 64 UTF-8 bytes; free-form diagnostic text is forbidden. Capture
provenance projects into shared `source-ref` and `sample-origin`, preserving
unknown time/freshness. Supplied assertions are not trusted attestation.
Hash exact bytes; do
not introduce a second canonicalization algorithm alongside NIC-110.

Each resource has an opaque stable `resource_key`, `format` (`opaque` or
`json`), and one explicit state:

- `present` with blob SHA-256 and byte length;
- `absent`, a declared observation or desired absence, not a failed read; or
- for observed data only, `unavailable` with a bounded reason code.

Baseline defines the closed resource universe. Candidate and observed
manifests must contain exactly those keys, including explicit absent states.
A duplicate, missing, or extra key is an invalid manifest, not an unchanged
file. A present baseline versus observed absence is missing-file drift;
baseline absence versus observed presence is unexpected-file drift **within
the declared universe**. V1 does not enumerate a host directory or discover
undeclared files. There are no glob, recursive include, or path-expansion rules.

Resource keys map to deployment paths outside this Child. The Child constructs
blob paths solely from validated lowercase hexadecimal hashes. It never opens
a caller-provided absolute path. Bundle provisioning must reject symlinks,
hardlink aliases to mutable files, special files, traversal, and writable
shared roots. Missing or hash-mismatched blobs are input errors, not absence.
The existing directory Toy confines a root, not individual blob identities;
a dedicated immutable root is therefore part of the input boundary.

### Baseline and policy ownership

An authorized operator declares the baseline revision and policy before the
job. The job pins both hashes. NIC-110's authenticated job record must retain
that declaration and the exact seven-target set. A caller cannot turn a new
baseline into approved deployment policy by labelling it approved. Approval,
publication, and replication of bundles are external operations, not Child
Toys or side effects of `inspect`.

Different host roles may use different explicitly declared baseline hashes.
The fan-out request records each target's assignment and a common policy-set
revision. Reduction groups by that identity; it never compares unrelated roles
or selects a majority baseline. Unexpected revisions produce an indeterminate
slot rather than a clean comparison.

## 3. Computation and outputs

### Drift

Verify the pinned manifests and policy, then read and hash each present blob.
Compare observed state to baseline by resource key. The primary comparison is
**exact bytes**, so comments, whitespace, and line endings count as drift.
No lossy normalization may hide an operationally relevant change.

A JSON policy can additionally validate declared resources. V1 supports only
strict UTF-8 JSON, duplicate-key rejection, and an explicit list of JSON-pointer
rules: required/forbidden member, scalar type, scalar equality, bounded enum,
and integer range. Limits are depth 16, 1,024 values per file, 64 rules per
bundle, pointer length 128 bytes, and scalar string length 1 KiB. Integers are
signed 64-bit; policy numeric comparisons do not use floating-point coercion.
No regular expressions, remote schemas, scripts, YAML aliases, executable
includes, TOML/INI/service-specific interpretation, or daemon `--check` call.
Unsupported formats still permit byte-drift reporting but cannot pass a
requested semantic check. An empty policy explicitly means byte checks only.

### Preflight before change

Let B be the declared baseline, O the observed snapshot, and C the proposed
complete candidate snapshot. Preflight performs these checks in order.

1. Verify all input identities, closed resource sets, and budget limits.
2. Compute O versus B. V1 blocks on existing drift; it does not silently treat
   a repair or rebaseline as an ordinary change.
3. Compute C versus O and check the policy's explicit allowed-change resource
   list, required/forbidden states, and supported JSON rules. The candidate
   contains the whole set, not a patch whose application could vary.
4. Return `ready_for_review` only if every requested check completed and passed.
   A proved violation returns `blocked`. Any required unavailable data,
   unsupported check, stale required capture, or incomplete coverage returns
   `indeterminate`, with known violations retained in the findings.

The report pins O and C, the policy, baseline revision, and change ID. It is
**not an apply grant or a claim that the running service will accept C**.
Syntax validity and explicit policy compliance do not prove daemon behavior,
free disk space, reachability, or restart health. A supplied-snapshot review
can be ready for review without being a live-state preflight. The report must
label that scope. Any consumer requiring current-state evidence checks trusted
capture provenance and freshness outside the pure comparison; a guest's claim
of fresh capture is not authority and cannot satisfy that requirement.

A later, separately authorized apply workflow must recapture or compare the
live content identity to O and bind the exact C and policy before any write.
Expiry alone cannot close this time-of-check/time-of-use gap. Concurrent live
changes invalidate the check, even one second after preflight. This Child
never writes configuration, repairs drift, changes ownership/mode, reloads a
service, or provides rollback. File permission/owner drift is outside v1;
staged-copy metadata must not be reported as live-host metadata.

### Domain report

`mct-config-report/v1` contains mode, configuration-set ID, input manifest and
policy digests, baseline revision, capture provenance class, complete resource
counts, findings, and a domain verdict. Findings sort by resource key and check
ID for deterministic comparison. A finding contains only resource key, delta
(`unchanged`, `modified`, `missing`, `unexpected`, `unknown`), check ID, check
status, and fixed reason code. No file contents, differing values, unified
patches, private paths, parser excerpts, or environment values are returned.

Drift verdict is `clean`, `drifted`, or `indeterminate`. Unknown coverage takes
precedence over clean/drifted, while known drift counts remain visible.
Preflight verdict is `ready_for_review`, `blocked`, or `indeterminate`.
The report states byte-only versus semantic coverage and supplied versus
attested capture. Host execution time is not substituted for capture time.
Shared `coverage.examined` counts fully assessed resource keys, not passed
checks. Any unassessed resource or omitted requested check makes completeness
`partial`; `omitted` counts unassessed resources when known. Domain check
counts separately expose omitted checks on examined resources. Continuation
is absent in v1. A partial report is indeterminate even when transport succeeds.

Examples of fixed reasons include `content_changed`, `expected_absence`,
`input_digest_mismatch`, `policy_violation`, `unsupported_check`,
`capture_unavailable`, `capture_stale`, and `budget_exceeded`. Receipt-level
policy denials remain caller-safe; detailed authority reasons stay in the
permitted ledger projection.

Do not stage secret-bearing configurations for v1. Redacting the result after
reading a secret does not authorize the read, and exposing a low-entropy hash
can leak the secret. An operator may provide a separately classified,
secret-free projection, but its report must identify that projection and may
not claim completeness for the original file. No secrets Toy is imported.

## 4. Toys and grants

| Capability | Status | Decision |
| --- | --- | --- |
| `toy:mct:directory-read` | **Existing** | Required for the immutable bundle. Catalog identity is `wasi:filesystem/preopens@0.2.3`, function `read-file`; grant action is `read-content`. The names are not interchangeable. |
| Live config snapshot | **NEW, optional follow-on Toy** | Required only to claim automatically collected live-host drift. Proposed identity `mct:config-snapshot/capture@0.1.0`, action `capture-read`, resource an operator-owned allowlist ID. Not imported or implemented by v1. |
| Logging/measurement | Existing capabilities, not requested | Mother measures and records execution. Ordinary directory-read has a strict import allowlist; do not add logging imports casually. |
| Keyvalue/Watch/messaging | Not requested | Job durability belongs to the shared host/client contract. Do not fake a Watch scope to obtain persistence. |
| Git, shell, network fetch, write/apply, secrets | Not requested | No ambient authority or replacement through subprocesses. |

Issue one exact directory grant per executing Mother and intended caller.
`ToyGrantSubject` binds Child name, artifact ID/version, assignment, and caller
node. Scope binds Vision, executing node, applicable trusted project and data
classification, and the `file://` canonical private bundle root. Allow only
`read-content`, require locality, set an explicit expiry and duration bounded
by the call deadline. Manifest filesystem needs at `/project` grant nothing.
No grant for `/`, `/etc`, Mother's state directory, or an arbitrary caller path.

Current Mother authority must admit the Child and Toy before runtime start.
Expired, revoked, wrong-caller, wrong-artifact, or wrong-resource probes must
produce denial with zero starts. Do not mistake caller-supplied project or
classification labels for trusted data authority. Kernel-minted capabilities
and their current-authority checks remain the effect boundary. Revocation
blocks new admission; this design does not promise instant cancellation of an
already-admitted read during a partition.

The optional live-snapshot Toy must pin a no-follow source descriptor under an
operator allowlist, refuse special files, enforce size/read deadlines, capture
provenance, and detect instability during capture. If it cannot establish a
consistent multi-file generation, report non-atomic/unknown rather than claim
a coherent snapshot. It needs separate secret/data-export policy and an
immutable publication boundary. It must not execute validators or install
candidates. Its output would feed the same inventory contract. A supplied
snapshot remains a useful v1 without this Toy, but not a live collector.

Future MCU capture can use a Pi Mother gateway and a separately granted
serial/sensor Toy. Provenance distinguishes executor Mother from source device
and source resource. The comparison API assumes no local sensor, native path,
or Mother on an MCU. WAMR/wasm3 or firmware execution is stretch work, not Pi 3
acceptance and not an automatic port of this component.

## 5. Memory, startup, and workload budget

All numbers in this section are **proposed gates**, not measurements. NIC-110
owns their final declaration syntax and enforcement. Bounds apply before
allocation and after serialization, including escaped JSON and WIT values.

| Resource | NIC-113 budget |
| --- | --- |
| Guest linear memory | Target peak at most 8 MiB; hard ceiling 16 MiB, one memory; no unbounded allocator retention across files |
| Incremental Mother process RSS | At most 32 MiB over warmed idle at peak for one call, measured separately from guest memory |
| Concurrency | One active NIC-113 invocation per Pi 3; coordinator maximum two in-flight target calls initially |
| Artifact | Target `.cwasm` at most 8 MiB; compatibility and trusted provenance required |
| Cold start | p95 at most 1,000 ms for verified precompiled load/link/instantiate; no compiler on target |
| Warm start | p95 at most 250 ms for fresh instance from admitted cached code |
| Work | p95 at most 2 seconds for the maximum accepted bundle; 5-second local execution deadline including load and reads |
| Fan-out wait | 30-second initial collection deadline; incomplete targets remain explicit, later delivery creates a new aggregate revision |
| Request | At most 4 KiB canonical domain request; complete dynamic host argument charge at most 16 KiB under NIC-110 |
| Input | At most 16 resource keys; 32 KiB per blob; 256 KiB cumulative bytes read across B/O/C, counting repeat reads; four manifests/policy at most 16 KiB each, 320 KiB total |
| Read behavior | One blob at a time, chunks at most 4 KiB; no reread retry loop, enumeration, or expanding includes |
| Findings | At most 64 findings and all 16 resource summaries; further findings mean incomplete/indeterminate, never truncated success |
| Domain output | At most 8 KiB JSON report; actual runtime-serialized result including JSON-in-string escaping at most 32 KiB |
| Shared result and receipt | NIC-110's 48 KiB terminal package, receipt at most 8 KiB; ledger-chain proofs fetched separately in bounded authorized pages |
| Fleet answer | NIC-110's 512 KiB coordinator package, seven original signed node results; config domain merge at most 32 KiB; never one giant Wasm return |
| Host budgets | NIC-110 starting limits: 256 host calls, 64 Toy calls, zero logging/measure calls requested, 130 observations, 1 MiB cumulative accounting, 16 KiB host value, 32 KiB application output, 64 finding items |
| Offline storage | Admission reservation up to 1 MiB/job/target for immutable input and results; at most 16 NIC-113 pending jobs within the shared 16 MiB spool, plus NIC-110's separate 1 MiB settlement reserve; refuse when either bound is full |

The current 64 MiB setting in `wasm.rs` is a Wasmtime memory-size limit, not a
Mother RSS guarantee or a complete multi-memory/process budget. V1 must admit
only the reviewed single-memory artifact and enforce the lower declaration.
NIC-110 must settle per-Child overrides and total memory accounting. Count
ABI expansion, parser structures, hashing buffers, retained allocations, host
results, and observations rather than extrapolating from raw input length.
Do not copy repo-report's non-reclaiming allocator without proving the full
multi-file peak fits.

The shared budget declaration uses `linear-memory-bytes = 16777216`,
`startup-ms = 1000`, `wall-ms = 5000`, `input-bytes = 4096`,
`output-bytes = 32768`, `host-value-bytes = 16384`,
`accumulated-bytes = 1048576`, `host-calls = 256`, `toy-calls = 64`,
`logging-calls = 0`, `measure-calls = 0`, `observations = 130`, and
`max-items = 64`. Snapshot bytes are a separate 320 KiB read allowance, not
an enlarged call argument. Mother applies the minimum of declaration, caller
request, and node policy, refusing a budget below required operation minima.
NIC-110's 256 MiB available-memory reserve for OS/Mother/control also applies.

`wasm/budget.rs` accounts dynamic values and host observations. Its existence
does not prove every standard WASI read is charged through the generic Toy
counter. Enforce the workload read bounds in the Child and prove host-side
read/deadline limits in shared acceptance. Host budget exhaustion is sticky
and terminal. No retry with a larger budget, silent truncation, or clean result
from a partial parse. Blocking host I/O must respect the deadline too; epoch
interruption alone is not that proof.

### Precompiled delivery is a prerequisite

At this baseline `compile_snapshot` calls `Component::from_binary`. Therefore
building a `.wasm` on nicamaster and copying it to a Pi does **not** meet the
no-compilation requirement. NIC-110 must supply trusted `.cwasm` production
and loading before this design can pass hardware acceptance.

Nicamaster must compile for the Pi 3's aarch64 CPU feature floor and the exact
Wasmtime version/configuration. Bind source `.wasm` and manifest digests,
compiled artifact digest, target/features, engine configuration, and builder
provenance in an authenticated artifact record. Mother's existing approval,
assignment, and instance identities still apply. Serialized native code is
not trusted merely because it has a `.cwasm` suffix or a matching public hash.
Reject unknown producers, incompatible versions/features, digest mismatches,
and altered manifests **without** falling back to target compilation.

## 6. Signed receipts and fleet answer

Use NIC-110's `receipted-result` and `signed-receipt` for every durably admitted
call's terminal outcome, including denied, failed, timed-out, and cancelled.
Its envelope and request binding cover job/task/call/attempt, actual executor
and Vision, operation, artifact, request/result digests, effective budget and
outcome. Domain input/policy identities are committed through that request
and successful answer. Artifact provenance must bind the manifest too.
Caller/receiver forwarding correlation remains in the ledger.

Use the shared deterministic CBOR/BLAKE3 receipt preimage, purpose-bound
Ed25519 receipt key, `ledger-position`, verifier, and enrollment/rotation rules.
A chain proof is separately paged, not embedded in each result. If policy
prevents independent inclusion verification, label signature-only evidence
as such, not fully ledger-verified. NIC-113 adds no signing Toy or key hierarchy.
A hash-chained ledger, a peer-admission signature, and an output hash are not
substitutes for this portable execution receipt.

Durably retain the exact result bytes before releasing a successful terminal
receipt. If result persistence or required ledger append fails, withhold
success and reconcile from surviving canonical evidence. A disconnected or
powered-off Mother cannot sign an immediate terminal result. The coordinator
records a shared `missing` row with its own dispatch evidence and
`may-have-executed` flag, never a fabricated remote receipt. Once recovery classifies an admitted call, its
executing Mother supplies the recovered terminal receipt. Malformed ingress
that never becomes a call is not invented as a Child invocation.

The client submits one explicit seven-target job, six Pi 3s plus nicamaster,
using exact placement constraints through ordinary Mother routing. Each
receiver independently authorizes its artifact, scope, and read grant. There
is no healthy-node fallback for another node's observation. NIC-105's exact
placement capability is a dependency; it is not assumed present at `bc48d1e`.

The single merged answer is NIC-110's `receipted-fleet-result`. Its sorted
`node-result` rows contain each original `received(receipted-result)` or a
`missing(no-reply)` observation. Missing reasons use `not-dispatched`,
`unreachable`, `reply-deadline`, `invalid-receipt`, or `incompatible-result`.
Set `may-have-executed` after uncertain dispatch. A received denial is not a
missing row. The coordinator signs the aggregate, retaining every original
executor receipt unchanged.

The optional domain merge uses `mct-config-fleet/v1` canonical JSON with
per-target baseline/observed/candidate identities, coverage, domain verdicts,
capture freshness and counts. Full terminal results remain in node rows, not
replaced by references. Only verified successful reports contribute findings;
only complete comparable ones can contribute to a clean fleet verdict.

NIC-110 transport counts remain `successful + unsuccessful + missing = 7`.
Its `complete` requires seven received successes with complete coverage;
`partial` requires some usable success but incomplete coverage; `failed` means
none. A complete transport result may still report domain drift or blocked
preflight. A complete **clean** fleet additionally requires seven comparable
clean domain reports. Otherwise preserve every known finding and the explicit
coverage/domain verdict; never relabel transport counts to mean clean files.
Never treat offline, unsupported, invalid-signature, or wrong-baseline targets
as clean. Stale capture cannot establish a current-state clean verdict.
For a preflight job, only seven verified `ready_for_review` reports produce a
fleet `ready_for_review`, still not deployment permission. A later arrival
adds an immutable revision referencing the previous aggregate; it does not
rewrite the earlier report. Bind the target set and all included per-node
result digests in the aggregate so omission or substitution is detectable.

## 7. Reboot, disconnect, and offline handover

The Child is stateless and read-only. Shared host/client infrastructure, not
Child keyvalue or a background loop, owns the durable queue and result spool.
The existing 12-minute call dedupe window is not a durable offline job store.

| Interruption point | Required behavior |
| --- | --- |
| Before dispatch | Coordinator persists job, target set, digests, budget and deadline. Offline targets stay queued, not completed. |
| Admitted on Pi, before result commit | Preserve admission/start facts. Until settlement, the coordinator may lack an outcome. Recovery settles the read-only attempt as failed with `execution-interrupted` and `effect = no-effect`; it does not resume guest memory or infer success. |
| Result committed, reply lost | Reacquire the exact stored result and receipt under the same attempt identity; no second logical completion. |
| Offline after completion | Keep local result/receipt in the private spool; hand over later through authenticated, audience-authorized transport. |
| Reboot during handover | Idempotent acknowledgement binds job, attempt, executor, result and receipt digests. Duplicate delivery is harmless. |
| Corrupt ledger or uncertain continuity | Quarantine/deny according to reliability doctrine. Do not truncate valid history, forge continuity, or rerun as if nothing happened. |
| Full disk or spool | Reject new admission before reads if result/receipt space cannot be reserved. Preserve committed evidence and surface backpressure. |

Queue entries contain immutable references/authorized bytes, not cached
`AuthorizedToyCall` tokens. Recheck current grants, membership, classification,
deadlines, and artifact policy at dispatch. An offline peer cannot renew its
own authority. After the dedupe horizon, a recomputation is a new attempt
linked to the same logical job, explicitly requested and admitted afresh.
Recovery never initiates that recomputation itself. Immutable reads make it
safe to repeat, not exactly once. A changed snapshot is a new input/job
revision, never a replay of the old result.

Unstarted jobs expire after a proposed 24-hour queue TTL. Captures used as
live preflight evidence have a proposed maximum age of 60 seconds; old
snapshots remain useful for historical drift but cannot qualify as current
preflight evidence. Trusted capture and Mother time establish age; unknown
clock quality or reboot discontinuity makes freshness indeterminate. The
coordinator must recheck freshness on receipt and before any later workflow.
An offline-completed preflight may therefore arrive as valid historical
evidence but already be stale for action.

Keep completed records until authenticated acknowledgement, subject to the
spool admission cap, then retain for NIC-110's proposed seven-day minimum and
retain bounded call/request tombstones afterward. Replaying an old call whose
bytes were retired returns result-unavailable, never fresh execution. The
`handover-item`/`handover-ack` fields and receiver dedupe follow NIC-110.
Age out unstarted work with an explicit expired record; the execution deadline
wins over the 24-hour queue TTL. Do not silently delete undelivered completion
evidence to make room. Current result-release authority must still allow
handover. Revocation may prevent
export without erasing local canonical facts. Queue retention and recovery
journal details are NIC-110 responsibilities and must be reconciled before
build approval.

## 8. Composition candidates and alternatives

The natural decomposition is inventory + diff, with a pure policy evaluator.
Keep these as pure domain functions within the first Child unless independent
reuse earns a separate artifact. One call and one receipt are simpler on a Pi
than a general graph scheduler.

| Component | Profile | Responsibility and boundary |
| --- | --- | --- |
| Config inventory | `read-compute` | Verify bundle and collect bounded resource facts. Only component that needs directory-read. |
| Config diff | `pure` | Compare two compatible inventories by resource key, exact digest and presence; reusable for drift and candidate delta. |
| Policy evaluator | `pure` | Validate approved bounded JSON rules. Do not pass raw config to another node without data-export approval. |
| Fleet reducer | Client-owned first; possible `pure` Child later | Merge already verified per-node reports into shared fan-out output. Host verifies signatures; a pure reducer cannot assert trust from unverified receipt strings. |
| Live capture | New Toy behind a separately admitted read-compute operation | Add attested live observations without granting diff or policy logic host-path authority. |

NIC-105 dependency inventory and config inventory may reuse diff concepts,
receipt handling, and target reduction. Do not reinterpret Cargo.lock parsing
as a universal configuration parser or compare unlike schemas as if identical.
A hardware/sensor Child can supply a projected config snapshot through a Pi
gateway with distinct source identity. No direct Child-to-Child host calls,
borrowed grants, or automatic dataflow scheduler are assumed.

Rejected first versions are a broad `/etc` preopen, a shell wrapper around
`diff` or daemon validators, a stateful watcher, and a deploy-and-rollback
Child. They widen authority or obscure structured failure without being
necessary for a snapshot preflight report.

## 9. Pi 3 acceptance test plan

**Future tests, not run in this design task.** A build may pass only after the
shared-contract prerequisites exist. Run on a real approximately 905 MB
Raspberry Pi 3 with aarch64 Debian, then on all six Pi 3s plus nicamaster.
Use generated secret-free fixture configurations in private approved bundles.
No hardcoded credentials, production paths, or personal configuration belong
in the fixtures. Operator-authorized power interruption remains a manual
hardware ceremony.

| Gate | Setup and pass condition |
| --- | --- |
| Useful drift | Declare three resources: unchanged JSON, byte-modified opaque config, and baseline-present/observed-absent config. One call returns exact clean/modified/missing counts and input identities with a verifiable receipt. A baseline-absent/observed-present case reports unexpected within scope. |
| Before-change preflight | With O=B, a C satisfying allowed-change and JSON rules returns `ready_for_review`. Forbidden key/range, forbidden resource change, and pre-existing O/B drift return blocked. Snapshot hashes and live fixture bytes remain unchanged. No service/process/write effect occurs. |
| Honest scope | Whitespace-only changes count as byte drift. Unsupported requested semantics, missing checks, or absent observed keys never produce ready/clean. Missing/stale capture cannot pass a current-state requirement; supplied-snapshot review stays labelled as such. Undeclared host files are explicitly outside coverage. |
| Input confinement | Absolute/traversal blob references, links, special files, mutable root, hash mismatch, duplicate/extra keys, and include-like payloads are rejected. An unreadable file never becomes declared absence. Inspect authority and artifact import evidence, not a shell exit code. |
| No secret leakage | Marker strings placed in rejected secret-bearing input or malformed JSON never appear in caller results, receipts, logs, or exported observations. Failure diagnostics remain bounded codes. |
| Exact authority | Expire/revoke the receiver's exact read grant and probe with a fresh call ID. Require denial and zero Child runtime starts. Repeat with wrong caller/artifact/root/Vision and no grant. Manifest needs alone must not permit reads. |
| Seven-target partial result | One submission targets six Pi 3s plus nicamaster. Exercise clean, drifted, denied, malformed-input, and disconnected targets. Get exactly seven attributed slots and one merged partial answer by 30 seconds; no substitute execution or success for missing targets. Then recover the offline target and obtain a linked later revision. |
| Receipt integrity | Verify every available terminal receipt and ledger proof independently. Modify result bytes, input digest, executor, artifact, ledger position, or signature and require rejection. Reject omitted target slots and untrusted/rotated keys lacking the shared trust proof. Denials have receipts where the receiver admitted a protocol call. |
| Reboot and drop | Inject interruption before read, during read, after result commit/before reply, and during handover. After recovery, both coordinator and executor ledgers verify their surviving prefixes. Unknown/interrupted attempts never become success. Lost replies reacquire exact bytes; duplicate delivery does not duplicate logical completion. |
| Offline authority | Queue while disconnected, revoke or expire the grant before delivery, then reconnect. New admission is denied. Same-ID replay beyond 12 minutes reacquires the original result or its unavailable tombstone; only explicitly requested recomputation uses a distinct admitted attempt. Expired work and full spools give explicit outcomes, preserving committed records. |
| Freshness and race | Change the live fixture after capture and before the hypothetical apply boundary. Preflight cannot authorize the change; an apply consumer must reject the stale O identity. Delayed preflight handover and untrusted clock state are not current-ready evidence. |
| Hard bounds | Exercise each input/rule/depth/output limit at and beyond its boundary. All allocations/reads and final escaped outputs fit the declaration. Overflow traps or returns a bounded error/indeterminate report, never truncated clean. Test slow host reads, full disk, ledger append failure, and output persistence failure. |
| Memory and startup | Record 30 cold verified precompiled loads and 100 warm calls, including maximum accepted fixtures. Meet the guest, incremental RSS, startup and execution gates in section 5 without swap/OOM. Record Mother idle RSS and system available memory separately. Repeat calls must not show accumulating retained job memory. |
| No target compilation | Artifact evidence shows nicamaster produced the trusted Pi-compatible `.cwasm`. Instrument loader/compiler entry points and require zero target compiler invocations, not merely no Cargo process. Missing/corrupt/incompatible `.cwasm` fails closed with no compile fallback. |

Retain structured requests/results, per-node call correlation, receipt
verification output, ledger verification evidence, budget samples, system and
engine versions, and compiled artifact provenance. Timing separates code
verification/load/instantiate, execution, ledger commit, and network wait.
Repeatability and attribution matter more than a favorable aggregate average.
No existing NIC-63/NIC-105 evidence is relabelled as passing these new gates.

## 10. Shared-contract reconciliation and approval gates

Before building, reconcile these requirements with NIC-110's exact draft.

1. **Envelope.** Freeze this typed request/body-plus-coverage projection into
   the shared envelope. Register the canonical JSON schemas and error mapping,
   confirm dynamic ABI bounds, and keep domain SHA-256 content identities
   distinct from shared BLAKE3 digests.
2. **Receipt.** Define canonical signed bytes, ledger anchoring, key trust and
   rotation, result persistence ordering, and recovered terminal outcomes.
   Distinguish no remote receipt from a signed remote failure.
3. **Fan-out.** Bind the exact target set and per-target baseline assignment,
   use the shared node-answer variants and original received packages, and
   version late-arrival aggregates. The per-target request digest commits its
   baseline assignment. Exact placement must not bypass admission.
4. **Budget.** Enforce the 16 MiB guest declaration and host/RSS/read/output
   bounds within NIC-110's smaller starting policy, including standard WASI
   I/O, aggregate memory, and receipt/spool overhead. No target `.wasm`
   compilation fallback is permitted.
5. **Offline.** Define queue/result ownership, reservations, retention,
   acknowledgement identity, retry attempts, clock/freshness semantics, and
   current-authority rechecks. Do not equate a dedupe cache with durability.
6. **Source identity.** Keep executing Mother distinct from captured device,
   resource, capture generation/time, and supplied-data assertions. This also
   permits the Pi-fronted MCU gateway without an MCU Mother.

Approval choices are the narrow v1 format/policy set, byte-exact comparison,
snapshot-only scope, block-on-existing-drift preflight, and the proposed budget
and freshness values. Automatic live collection requires approving the new
Toy separately. No unresolved choice defaults to broader filesystem, secret,
execution, or deployment authority.

### Design-only validation

The document's eight required field rows, twelve relative source links, code
fences, and whitespace were checked locally. Its embedded WIT was extracted
to a temporary directory and parsed in text-only mode with
`wasm-tools component wit`, using the pinned shared draft and the existing
filesystem WIT dependency. This checks syntax and type references, not runtime
linking, generated bindings, cryptography, or a built Child.

The design deliverable is this document on `nicabar-nic113-design`. Runtime,
hardware, signing, durability, and performance acceptance remain future work.

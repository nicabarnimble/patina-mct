# H3/M1 — Bounded Iroh ingress, preserving rejection durability

Local implementation on `experiment/git-toy-confinement`, atop M2/M3 checkpoint
`d974af5` (parent `983b2a3`). No push, main change, release or deployment.

## Accepted contract and scope

Owner decision, 2026-10-25: **Rule 3 of
[`layer/core/safety-boundaries.md`](../layer/core/safety-boundaries.md) stays in
force.** Required rejection/authorization observations remain `BeforeEffect`.
An unavailable or failed durable write suppresses the response/protected effect;
this change does not replace required facts with sampling, logging, or counters.
The [reliability doctrine](../layer/core/reliability-doctrine.md) is unchanged.

This implements the requested identifier, pre-admission authority-fetch,
connection-error isolation and deadline/occupancy boundaries. It bounds work
and ledger amplification **per request**, not lifetime ledger growth or a fair
share under an unlimited sustained distributed flood. The wider H3/M1 audit
acceptance/load campaign and independent review remain separate.

## Retained counterexample

With the new regression test and otherwise unchanged `d974af5`,
`unadmitted_and_malformed_calls_do_not_fetch_authority` failed deterministically:
an admitted endpoint presented an unknown hello receipt; the call was denied,
but the bindings provider was still invoked **once**, rather than zero times.
The test forces the order through completed requests, not sleeps/races.

The failing log is `/tmp/mct-h3-m1-iroh-ingress-evidence/before.log`.
The test source retains the counterexample and the successful authorized control.
The original audit's sustained disk-exhaustion/64-slot load fixture was not
available or reproduced; this is not a claim of that original demonstration.

## H3 implementation

- Both single-connection and concurrent serving validate the typed decoded wire
  identifiers **before constructing any observation fact**. This is a wire-only
  policy: local/historical kernel IDs and canonical history are not rewritten.
- Every `*_id` field, nested authority identity, `authority_epoch`, idempotency
  key, operation namespace/interface/function, data-classification label,
  child name and protocol name is nonempty, at most **256 ASCII bytes**, and
  restricted to `A–Z a–z 0–9 - _ . : / @`. The sum of these fields is at most
  **8 KiB per request**, including nested capability identities. Null optional
  identities remain permitted. References, signatures, URLs and business bytes
  are not misclassified as IDs; the existing 96 KiB frame cap still bounds them.
- A malformed call uses fresh local trace/protocol/decision/observation IDs.
  Malformed hello JSON, invalid identifiers, oversized frames and partial-frame
  failures also produce a safe, locally correlated durable hello rejection.
  No truncation/aliasing of a hostile identifier; no raw request/error string is
  copied into the rejection. Failed/denied hello invalidates prior peer receipts.
- On `mct/call/0`, shape/identifier/payload validation and exact endpoint/hello
  receipt lookup precede **both** bindings and receiver-authority providers.
  Disagreeing receiver-authority copies are malformed before either fetch.
  Missing receipts use an empty authority context only to evaluate denial;
  admitted requests still fetch fresh authority and perform all kernel checks.
  Receipt lookup is repeated after provider awaits, preserving invalidation.
- Concurrent protocol/read/write failures remain task-local; they do not use
  `?` to exit the listener loop. Adapter failures increment fixed-size shared
  `MctIrohIngressHealth` counters. Existing escalation for a failed required
  reply-emission observation is preserved, not recategorized as a peer error.
- The optional event channel is a bounded, best-effort **projection**, not the
  mandatory sink. Full/closed projection queues increment a health counter
  instead of blocking acceptance, refusal or permit release. A dropped served
  event can delay capability-inventory refresh; it cannot create admission or
  replace durable facts. Consumers can retain `config.ingress_health` to inspect
  both counters. Neither counter introduces per-peer maps or durable writes.

## M1 implementation and default budgets

| Boundary | Default bound |
| --- | --- |
| Concurrent accepted connections | 64; excess incoming connections refused |
| Combined handshake / ALPN / connection completion | 3 seconds |
| Combined first bidi stream + entire request frame | 3 seconds, one absolute deadline |
| Post-frame processing, durable prefix and execution | 30 seconds, independent of the two ingress deadlines |
| Response write/FIN | 3 seconds, also within processing budget |
| Post-response peer-close wait | 1 second maximum, then local connection close |
| Incoming bidirectional streams | 1 concurrent stream per connection |
| Incoming unidirectional streams | 0 |
| QUIC stream / connection receive windows | 128 KiB each |
| Decoded request frame | 96 KiB |
| Remembered admitted hello receipts | Existing bounded 1,024-entry cache |

The endpoint serves one request/response per connection. FIN is not assumed to
mean delivery: there is a bounded opportunity to drain the response, then the
server closes even if the peer refuses to close. The concurrency permit is
released before optional event delivery. A peer cannot consume a full execution
timeout merely by holding the connection after the response.

The single-connection API uses the same ingress/frame/write/close helpers. Its
internal test timeout caps the ingress defaults as well as post-frame work.
Concurrent configuration exposes separate phase durations for controlled tests
and operator policy; defaults do not expand when execution time is increased.

## Regression coverage

Eight new tests in `crates/mct-iroh/src/tests/ingress.rs` and
`crates/mct-iroh/src/serve/ingress_tests.rs` establish:

1. Unknown hello receipt, oversized identifier, invalid semantic shape,
   unexpected inline bytes and inconsistent authority copies perform **zero**
   bindings/receiver reads and zero executions. Denial is still durable; a
   legitimate call performs both fresh reads and executes.
2. Every populated ID field in actual hello/call fixtures is attacked with
   257-byte strings, newline characters and non-ASCII characters in **both**
   serving paths. Rejections contain no hostile value and their serialized
   mandatory batches stay below 8 KiB. Exact 256-byte call/trace IDs succeed.
   Malformed JSON and oversized hello frames also get durable safe rejection.
3. Injected durable rejection failure prevents both malformed call and hello
   replies, prevents execution, invalidates the prior hello receipt, and leaves
   the concurrent listener available after the sink recovers.
4. Repeated unsupported-protocol errors do not abort serving. A deliberately
   undrained one-slot event channel cannot stall subsequent valid hello/call.
5. With **one connection slot**, an idle first-stream peer times out; a partial
   call gets a durable malformed reply; extra bidi/uni stream opens are pending;
   peers that read responses but never close relinquish the slot. A later valid
   hello succeeds despite a 60-second configured execution budget.
6. An admitted handler is held at an explicit notification barrier while a
   second connection's ingress deadline expires. The original handler survives
   that deadline and succeeds when released. No scheduling sleep selects the
   attack order.
7. Unit boundary matrix covers empty/control/non-ASCII/illegal characters,
   exact per-field maximum, nested fields and exact aggregate/overflow limits.
8. Paused-clock tests prove independent handshake, response and execution
   deadline expiration and ready-future success without wall-clock races.

Existing H2 forged-origin, R2 receipt interleaving, stale/revoked binding,
receiver-authority, payload-integrity and mandatory observation tests remain.
The old oversized-frame test raced immediate client close against reading; it
now waits for and asserts the actual malformed rejection instead of accepting
an incidental I/O/timeout error. The idle-stream test now asserts its named
pre-admission timeout rather than the old whole-connection timeout.

## Validation

macOS arm64, locked/offline dependency graph. Logs:
`/tmp/mct-h3-m1-iroh-ingress-evidence/`.

```sh
export CARGO_BUILD_JOBS=8 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_NET_OFFLINE=true
cargo test -p mct-iroh --locked --offline -- --test-threads=1
MCT_ORIGIN_KEEP=1 MCT_FLEET_KEEP=1 MCT_RESTART_KEEP=1 \
  cargo test --workspace --locked --offline --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all --check
git diff --check
bash scripts/ci-tier0.sh specs
```

- Iroh tests: **56 passed**, zero failures/ignored.
- Clippy, fmt, diff checks and tier-0 specs: passed.
- Full workspace: **781 passed, zero failures, 2 pre-existing ignored**.
- Five additional ingress-only reruns: **8 passed each**, zero failures.

Selected SHA-256 receipts:

```text
6f8822cbbab9ee0577c9c2bc89ead405478a9ae528748639c7cfe89f13b931cb  before.log
f7e6e84892ae48733c23ebaacf98f4985ec67dc10870ab03ae7e3252b07843c3  iroh-tests.log
f3a15cfb7915640920e11b1cedf146bfabd751d138b095807b8ad4c272b3d72f  workspace-tests.log
```

H3/M1 source, regression tests and this evidence are checkpointed together
in the local commit directly atop `d974af5`, per owner instruction on 2026-10-25.
M2/M3 remains checkpointed separately as `d974af5`.

## Limits / remaining assurance

- Preserving required rejection facts means storage can still grow with total
  request count. No total ledger quota, admission fairness under Sybil traffic,
  sustained-load service guarantee, or process RSS measurement is claimed.
- Normal hello admission still needs current bindings; syntactically valid
  unknown peers can cause bounded hello authority work. Calls without a receipt
  cannot. Source/provider CPU/storage costs and mandatory sink stalls retain
  their existing host-side boundaries, not a new storage scheduler.
- Optional projection loss is visible in bounded health counters, but persistent
  health/status export is not added here. Canonical write failure remains an
  error; the sink is never replaced by those counters.
- No raw-UDP handshake flood, physical multi-host/relay campaign, or original
  audit fixture has been run. Deadline seams, local QUIC integration tests and
  the workspace controls establish the named local boundaries only.
- Independent boundary review, extended fuzz/fault/load evidence and formal
  finding/completion-inventory closure remain outstanding. No release claim.

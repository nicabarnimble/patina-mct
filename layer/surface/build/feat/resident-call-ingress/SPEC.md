---
type: feat
id: resident-call-ingress
status: approved
created: 2026-07-14
target: resident-operational-loop-slice-1
sessions:
  origin: null
  work: []
related:
  - layer/surface/build/product/MCT-NEXT-BUILD-TODO.md
  - layer/surface/build/feat/resident-mother/SPEC.md
  - layer/surface/build/feat/payload-data-plane/SPEC.md
  - layer/surface/build/feat/multi-mother-route-forwarding/SPEC.md
  - layer/core/what-is-mct.md
  - layer/core/mct-build-boundaries.md
  - layer/core/safety-boundaries.md
  - crates/mct-kernel/src/call/mod.rs
  - crates/mct-daemon/src/control.rs
  - crates/mct-daemon/src/daemon/control.rs
  - crates/mct-daemon/src/daemon/resident/pipeline.rs
beliefs:
  - mother-kernel-decides-adapters-perform
  - protocol-outcomes-survive-projection
exit_criteria:
  - id: authenticated-uds-ingress
    text: The resident exposes POST /calls only on its owner-only UDS control endpoint, verifies the connecting process UID from Unix peer credentials before reading caller claims, and fails closed when credentials, resident identity, or the observation ledger are unavailable.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon resident_call_uds_authenticates_peer_before_submission -- --nocapture
  - id: resident-call-pipeline
    text: An authenticated local submission is translated into one immutable MctCall with canonical resident caller facts and CallOrigin::JvmAdapter, then enters the existing payload, idempotency, route-authority, revalidation, execution, result, and observation pipeline without a parallel call model.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon resident_call_uds_executes_approved_child_and_projects_control_state -- --nocapture
  - id: payload-law-reuse
    text: Local inline/blob payload declarations use the existing named frame and payload limits and exact BLAKE3 validation; request and result bytes never enter observations, status, run records, or error text.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon resident_call_uds_rejects_bad_payload_and_keeps_ledger_byte_free -- --nocapture
  - id: caller-scoped-idempotency
    text: UDS authentication and payload integrity precede caller-scoped idempotency; matching completed retries replay the durable caller-safe result, mismatches and in-flight duplicates refuse, and no retry executes the child twice.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon resident_call_uds_idempotency_is_authenticated_caller_scoped -- --nocapture
  - id: durable-submission-response
    text: Local admission, malformed rejection, authority denial, idempotency replay/refusal, and terminal result decisions are durably observed before a synchronous application response is written; safe responses expose no policy internals or topology.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon resident_call_uds_observes_decision_before_response -- --nocapture
  - id: control-read-visibility
    text: After a successful submission, GET /runs exposes the call, authority decision reference, terminal result, and route projection, while GET /status exposes an advanced observation sequence; neither read exposes payload bytes.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon resident_call_uds_executes_approved_child_and_projects_control_state -- --nocapture
  - id: real-cli-status
    text: mct-daemon status queries the resident UDS and reports actual readiness, resident identity, child instance counts, and last observation sequence; a missing or unusable socket reports not running/not ready and never prints the former static readiness claim.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon status_ -- --nocapture
  - id: kernel-boundary
    text: mct-kernel gains no UDS, HTTP, Unix credential, control-plane, JVM SDK, storage, or Patina application semantics.
    checked: false
    verify: bash -lc '! rg -n "UnixStream|peer_cred|POST /calls|control.sock|java|kotlin|belief|scry|assay" crates/mct-kernel/src'
  - id: workspace-validation
    text: The phase passes the required workspace validation suite.
    checked: false
    verify: cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
---

# feat: Resident call ingress

> The running Mother accepts authenticated local application calls through its UDS control endpoint, feeds them into the existing authority-first resident pipeline, and returns a durable caller-safe terminal reply.

## Problem

The resident Mother owns routing, execution, state, and the observation ledger, but local application calls still use one-shot CLI/JVM paths that open those resources outside the resident process. The daemon therefore cannot yet be the machine's sole operational coordinator. Its CLI `status` command also prints a static readiness claim without contacting a resident.

This slice translates and rebuilds the useful UDS-first application boundary from `patinaMother` under MCT authority. It does not reuse `patinaMother` request routing or trust semantics.

## Goals

1. Add one authenticated local application submission endpoint to the running Mother's existing UDS control plane.
2. Translate each accepted submission into the existing immutable `MctCall` and resident route/execution pipeline.
3. Preserve payload integrity, caller-scoped idempotency, safe denial, and observation durability laws.
4. Return a synchronous, caller-safe terminal result suitable for the future JVM SDK.
5. Replace the CLI's static status line with a real UDS-backed resident projection.

## Non-Goals

- No launchd/systemd installation or lifecycle wrapper.
- No package acquisition, registry trust, update, or rollback design.
- No new mctToy or changed ToyGrant semantics.
- No new remote, multi-hop, or transitive call behavior.
- No observation streaming API or general call queue.
- No asynchronous run-handle API, cancellation API, or restartable work scheduler.
- No child warmup, generation, or reconciliation rebuild.
- No resident module split and no resumption of paused release/launcher epics.
- No application-specific Patina or Slate semantics in Mother or kernel.

## D1 Decisions

### D1.1 — Transport is owner-authenticated UDS `POST /calls`

The v0 local application transport is HTTP/1.1-shaped JSON over the existing resident control Unix-domain socket:

```text
POST /calls HTTP/1.1
Content-Type: application/json
Content-Length: ...

<MctResidentCallSubmission JSON>
```

`POST /calls` is not served by the TCP/HTTP control transport. HTTP remains a read-only diagnostics surface for this slice. The normal resident default becomes the UDS control path (`.mct/control.sock` unless explicitly overridden); an explicitly HTTP-only resident has no local application call ingress.

UDS locality alone is not authentication. Before accepting a submission, Mother must:

1. bind the socket owner-only (`0600`);
2. obtain Unix peer credentials from the accepted stream;
3. require the peer UID to equal the resident process/socket owner UID; and
4. require the configured resident node identity to be available.

Credential lookup failure and UID mismatch deny. There is no bearer-token fallback and no trust in body-supplied caller identity. This first slice authenticates the local OS user, not an independently provisioned application identity. Per-application credentials require a separate authority design.

The authenticated UID is represented only as a canonical local caller fact and a safe/hashable observation subject. Raw process arguments, environment, and executable paths are not authentication evidence.

### D1.2 — Calls share the socket, not the admin mutation sequencer

The call endpoint and control reads share one socket and framing implementation, but a call is not an administrative mutation. `/calls` must not enter the serialized config/state/CAS mutation executor.

Connection dispatch must keep status reads available while a synchronous child call is in flight. Existing administrative mutations remain serialized and preserve their write-ahead decision → effect ordering. Local call work is independently bounded by resident connection capacity; capacity exhaustion fails closed with a caller-safe retry-later response and a durable denial observation rather than creating unbounded tasks.

This is a control-plane transport change, not a second daemon, second ledger writer, or second authority path.

### D1.3 — Reuse `CallOrigin::JvmAdapter`

No new `CallOrigin` variant is added. This endpoint is the production successor to the current JVM JSON bridge and is the transport the future Java/Kotlin SDK will target, so Mother sets `CallOrigin::JvmAdapter` after authentication.

The client cannot choose `origin`. Existing CLI, WASM host, process harness, and Iroh paths keep their existing variants. If a future non-JVM resident adapter needs separately truthful origin telemetry, that adapter gets its own gated mapping; this slice does not turn transport names into kernel authority.

**D1.9 amendment:** `CallOrigin::JvmAdapter` in ledger facts denotes the local application bridge ingress lineage—the transport succeeded by this endpoint—not a claim about the caller's implementation language. Future non-JVM local adapters needing distinct telemetry get their own gated mapping.

`CallOrigin` remains observation/dispatch context and grants no Vision, child, route, data, Toy, or peer authority.

### D1.4 — The local envelope excludes remote admission claims

The public request is a local submission envelope, not a client-authored `MctCallProtocolRequest`. It carries only application-controlled call facts:

```text
MctResidentCallSubmission {
  protocol_request_id
  call_id
  target                 # WIT namespace/interface/function
  payload_metadata
  authority_context
  deadline
  trace_context
  payload                # existing MctCallPayloadHandle
  inline_payload_base64?
  idempotency_key?
}
```

It does not accept `caller`, `origin`, peer binding, hello decision, endpoint, ALPN, connection path, or received-observation claims.

The UDS adapter constructs exactly one internal request:

- `caller.node_id` and `caller.vision_id` come from the resident identity;
- `caller.user_id` is the canonical authenticated UID identity;
- `caller.project_id` is absent in slice 1 because no authenticated UID→project binding exists;
- `origin` is `JvmAdapter`;
- target, payload metadata, authority snapshot, deadline, trace, payload handle, and idempotency intent come from the validated envelope.

The adapter may privately project local transport facts into the existing resident pipeline input, but those compatibility facts are not accepted from the client and do not simulate peer hello authority.

### D1.5 — Existing payload laws apply unchanged

The local endpoint reuses the payload data-plane contract:

- request frame budget: `MCT_CALL_FRAME_READ_BUDGET_BYTES` (96 KiB);
- inline request maximum: `MCT_INLINE_PAYLOAD_MAX_BYTES` (32 KiB);
- inline result maximum: `MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES` (32 KiB);
- local CAS object maximum: `MCT_BLOB_MAX_BYTES` (8 MiB);
- exact declared/observed size and BLAKE3 digest validation;
- existing local CAS resolution for `ContentAddressedBlob` handles.

The call frame is bounded before JSON/base64 decode. Payload integrity is evaluated before authority routing or idempotency replay. Payload bytes and base64 never enter `MctObservation`, run records, status, error text, or logs. Observations retain only safe metadata such as size, digest, classification, IDs, and typed outcomes.

### D1.6 — Idempotency is scoped to the authenticated local caller

The existing 720-second TTL, 256-entry per-caller budget, fingerprint, replay, mismatch, capacity, and in-flight refusal semantics remain unchanged.

For this endpoint, the scope is derived from the canonical `JvmAdapter` caller: resident node, authenticated UID, resident Vision, and no project. A client cannot select another scope by changing JSON fields.

**D1.10 amendment:** The `(node, UID, Vision)` idempotency scope is shared by all applications running as the same OS user. Reusing one key with a different fingerprint refuses as a mismatch, so applications must namespace idempotency keys; the future JVM SDK defaults to UUID keys. Per-application identity is the durable fix and remains explicitly deferred under D1.1.

Ordering is fixed:

1. authenticate the UDS peer and load current resident identity;
2. bound/decode the envelope and validate payload integrity;
3. construct the canonical local caller and record the local submission decision;
4. reserve/replay/refuse within that caller scope;
5. for fresh work, run normal route authority, execution-time revalidation, and effect execution.

Thus cached success never bypasses current transport/caller admission or payload integrity. Completed matching retries may avoid route selection and child execution exactly as the existing request-scoped idempotency law permits. A lost synchronous response is recovered by retrying the same fingerprint and key, not by blind re-execution.

### D1.7 — Submission decisions are durable before response

Local call ingress follows the A5/A8 write-ahead precedent. It reuses existing observation kinds rather than adding transport ontology:

- authenticated receipt: `CallReceived`;
- valid immutable call construction/submission-for-evaluation: `CallConstructed`;
- malformed/authentication/capacity rejection: `CallRejected` or `CallDenied` with a typed safe reason;
- route authorization/denial, revalidation, runtime execution, result, and idempotency facts: existing resident observations.

The admission decision permits evaluation only; it does not authorize a child or route.

An allow or denial is acknowledged to the application only after the corresponding observation batch is durable through the resident's single ledger writer. A terminal synchronous response is written only after the terminal result, denial, malformed outcome, or idempotency replay/refusal fact is durable. If the required append fails, Mother performs no protected child/forwarding effect and closes the request without an application response; it does not downgrade to an unobserved response.

Caller responses expose only typed outcomes, caller-safe messages, opaque result/decision references, safe route projection where already allowed, payload handles, and bounded inline result bytes. Authentication and authority denials do not disclose child inventory, candidate elimination, policy internals, topology, or credentials.

Binding and making the authenticated local call endpoint ready is itself a Mother operational effect. Startup/readiness observations must identify the control adapter without exposing the socket's host path outside operator visibility. Status reads are projections and do not create a new effect zone.

### D1.8 — Result delivery is synchronous

`POST /calls` blocks until the resident pipeline reaches a terminal caller-safe outcome or the call deadline. The JSON response carries the existing result/reply projection plus optional bounded inline result base64.

Synchronous delivery is selected because:

- the resident pipeline already produces a terminal typed result;
- request and inline result sizes are deliberately bounded;
- call deadlines already bound execution;
- durable idempotency recovers a response lost after execution;
- `/runs` already provides post-call operator inspection;
- an asynchronous handle would require a durable submission queue, call-result retrieval authorization, polling lifecycle, cancellation, expiry, and restart recovery that do not exist and are outside this slice.

`/runs` is an inspection projection, not the result-delivery protocol. A client must not submit, receive a run handle, and infer completion by searching the unscoped operator run list.

This synchronous authenticated UDS contract is the chosen ingress transport for `MCT-NEXT-BUILD-TODO.md` item 4. The future JVM SDK will construct `MctResidentCallSubmission`, connect to the owner-authenticated UDS, apply call/client deadlines, use idempotency keys for safe retry, and decode the typed terminal response.

## Response Contract

For an authenticated, decodable submission, transport success and application outcome are separate. The response body always carries the typed call outcome (`completed`, `denied`, `failed`, `timed_out`, `cancelled`, or `malformed`) and caller-safe fields. Authority denial is therefore not converted into an HTTP authorization claim.

HTTP status is reserved for the local transport boundary:

- `200`: authenticated submission reached a durable terminal typed call outcome, including call-level denial/failure;
- `400`: authenticated request framing/envelope is malformed and the rejection is durable;
- `401`/`403`: peer credentials are absent/unacceptable and the rejection is durable;
- `405`: `/calls` attempted on a transport/method that does not serve it;
- `413`: bounded call frame is exceeded and the rejection is durable when enough safe context exists;
- `503`: resident identity, capacity, runtime state, or other pre-execution adapter dependency is unavailable and the refusal is durable.

If durability itself is unavailable, the connection closes without a response.

## Real Status Companion Change

After D1 implementation, `mct-daemon status` becomes a client of `GET /snapshot` (or the minimum equivalent UDS reads) at `.mct/control.sock`, overridable by `--uds`. It reports from the response rather than opening state/ledger files independently.

The human and JSON projections include:

- running/reachable versus not running;
- actual `health` and `readiness`;
- resident node, Vision, and Iroh endpoint identity;
- loaded, approved, and ready child/instance counts available from status/state;
- last durable observation sequence.

Connect refusal, missing socket, malformed response, identity absence, and `not_ready` are reported honestly and produce a non-success status result. The former unconditional “ready for local child loading and Iroh” line is removed. This is a narrow control client, not launcher/supervisor work.

## Failing-Test-First Implementation Order

1. Add one failing resident integration test that starts an isolated UDS Mother, submits an inline call to an approved local process child, and proves response, ledger, `/runs`, and `/status` facts.
2. Add the local envelope and UDS peer-credential authentication at the control adapter boundary.
3. Add `/calls` dispatch outside the serialized admin mutation executor while preserving bounded concurrency and responsive reads.
4. Translate the envelope into the existing resident call pipeline; do not duplicate payload, route, execution, or idempotency logic.
5. Add fail-closed authentication, malformed payload, durability failure, and caller-scoped replay tests, including ledger no-byte assertions.
6. Replace static CLI status with the UDS snapshot client and missing/not-ready tests.
7. Update `MCT-NEXT-BUILD-TODO.md` only at implementation close-out with this chosen JVM transport and the started daily-operation supervisor clock.

## Required Integration Proof

The primary test must reconstruct the whole path from disk-backed state:

1. create isolated config, identity, children, SQLite state, ledger, and UDS paths;
2. install/approve an integrity-verified local process child fixture;
3. start the resident and wait for real UDS readiness;
4. connect as the same OS UID and submit `POST /calls` with a valid digest and idempotency key;
5. assert a durable submission/authority decision exists before reading the terminal response;
6. verify the returned result payload digest and bytes;
7. query `GET /runs` and find the same call, authority reference, route, and terminal result;
8. query `GET /status` and find the resident identity/readiness and an observation sequence at or beyond the call facts;
9. retry the same call/key and prove replay without a second child effect;
10. stop and reopen disk state to prove the observations, run, result, and replay entry survive process closure.

The test uses only isolated temporary state and never contacts or mutates a running `patinaMother`.

## Verification

Implementation commits must each pass:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

Close-out also captures the new integration tests with `--nocapture`, reconstructs the phase commit range from `git log`, and records any one-time flake rerun. A second failure is real and blocks the commit.

## Build Readiness

Approved at operator gate D1 on 2026-07-14. D1.1–D1.8 plus amendments D1.9–D1.10 are the implementation contract.

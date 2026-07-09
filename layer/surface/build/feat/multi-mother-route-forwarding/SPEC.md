---
type: feat
id: multi-mother-route-forwarding
status: active
created: 2026-07-09
target: mct-multi-mother
sessions:
  origin: 20260709-091408-460632000
  work: []
related:
  - layer/surface/build/feat/multi-mother-route-forwarding/DESIGN.md
  - layer/surface/build/product/MCT-NEXT-BUILD-TODO.md
  - layer/core/what-is-mct.md
  - layer/core/adapter-pattern.md
  - layer/core/safety-boundaries.md
  - crates/mct-daemon/src/federation.rs
  - crates/mct-daemon/src/main.rs
  - crates/mct-kernel/src/peer/mod.rs
  - crates/mct-kernel/src/route.rs
  - crates/mct-iroh/src/serve.rs
beliefs:
  - mother-kernel-decides-adapters-perform
  - iroh-provides-connectivity-not-authority
references:
  - layer/core/adapter-pattern.md
  - layer/core/safety-boundaries.md
  - layer/core/what-is-mct.md
exit_criteria:
  - id: capability-view-wire-surface
    text: mct/hello/0 carries a typed Vision-scoped callable-surface view with node, Vision, publication time, policy revision, and per-operation policy revisions; callers populate it from the daemon federation view instead of passing None.
    checked: false
    verify: cargo test -p mct-kernel hello_capability_view_carries_callable_surfaces && cargo test -p mct-daemon resident_hello_publishes_federation_callable_surface
  - id: admitted-hello-stores-surfaces
    text: An admitted signed hello request or response atomically stores the peer's callable-surface view in runtime state, replacing the prior peer+Vision view and recording received_at/stale_at; denied, mismatched, stale, or cross-Vision hellos store nothing executable.
    checked: false
    verify: cargo test -p mct-daemon admitted_hello_refreshes_peer_callable_surfaces && cargo test -p mct-daemon hello_response_capability_view_refreshes_surfaces_on_caller && cargo test -p mct-daemon denied_or_wrong_vision_hello_does_not_refresh_surfaces
  - id: remote-candidate-authority
    text: Resident route planning generates executable RemotePeer candidates only from fresh stored surfaces whose operation, Vision, peer binding state, local binding signature, outbound binding proof, ALPN scope, ticket, policy revision, and secret-scope checks all pass.
    checked: false
    verify: cargo test -p mct-daemon resident_remote_surface_candidate_becomes_admissible_when_all_checks_pass && cargo test -p mct-daemon resident_remote_surface_candidate_forbids_secret_scope
  - id: forwarding-execution
    text: When a RemotePeer route is selected and revalidated, the daemon forwards the call over mct/call/0, verifies request/result payload integrity using existing inline/blob limits, and maps the remote reply into the local typed result/reply outcome.
    checked: false
    verify: cargo test -p mct-daemon two_mother_forwards_selected_call_over_iroh_and_maps_reply
  - id: route-chain-observations
    text: The caller Mother records forwarded-from/forwarded-to peer observations, the executor Mother records executed-on/forwarded-from observations, and remote denial observations are typed; no payload bytes, credentials, or secret values appear in either ledger.
    checked: false
    verify: cargo test -p mct-daemon two_mother_forwarding_records_route_chain_without_payload_bytes
  - id: two-mother-fail-closed
    text: Cross-Mother wrong-Vision, revoked or expired binding, bad-payload, unauthorized-operation, and remote-denial paths fail closed with typed outcomes and observations.
    checked: false
    verify: cargo test -p mct-daemon two_mother_ -- --nocapture
  - id: kernel-boundary
    text: mct-kernel remains free of concrete iroh, wasmtime, WASI, storage, network, and Patina Belief/scry/assay internals.
    checked: false
    verify: bash -lc '! rg -n "iroh::|wasmtime|wasmtime_wasi|wasi_|rusqlite|belief|scry|assay" crates/mct-kernel/src'
  - id: workspace-validation
    text: The phase passes the required workspace validation suite.
    checked: false
    verify: cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
---

# feat: Multi-Mother route forwarding

> One Mother may call another Mother only through explicit authority: signed peer binding, admitted hello state, ALPN scope, fresh Vision-scoped callable-surface publication, policy revision, payload integrity, and route revalidation.

## Problem

MCT already observes remote peer route candidates, but the daemon intentionally eliminates them with `CapabilityUnavailable`. That is correct for the current slice because a peer connection proves only endpoint-key possession. It does not prove that the peer exposes the requested operation, that the operation is in the caller's Vision, or that the route can be executed safely.

The next Multi-Mother slice must close that temporal gap without weakening the existing authority model.

## Goal

Build the first executable Multi-Mother path:

1. A Mother publishes its local callable child operations in `mct/hello/0`.
2. The receiving Mother stores fresh, Vision-scoped remote callable surfaces as runtime evidence.
3. Route authority generates executable `RuntimeKind::RemotePeer` candidates from those surfaces.
4. The selected call is forwarded over `mct/call/0` and the remote reply is mapped into the local typed result.
5. Both ledgers record enough route-chain evidence to reconstruct what happened without exposing payload bytes, credentials, or secret values.

## Non-Goals

- No storage/network toy implementation.
- No JVM SDK work beyond preserving the existing local bridge behavior.
- No supervisor wrapper work.
- No Belief/scry/assay/spec/session internals in MCT.
- No changes to the Ed25519 peer-binding signature scheme.
- No changes to the two-phase `RouteDecision` / revalidation model except using it for remote routes.
- No new payload data plane beyond existing `mct/call/0` inline payload and declared handle rules.

## Authority Invariants

A remote route is executable only when all of these are true at route planning and still true at execution/forwarding time:

- the local config contains a peer binding for the remote node;
- the binding signature that admits that remote peer to this Mother verifies with the local Mother's endpoint identity;
- the peer binding state is `Admitted` and not expired/revoked;
- the binding ALPN scope includes `mct/call/0`;
- the peer has a usable ticket;
- the caller Vision equals the peer Vision and the stored surface Vision;
- the stored surface is fresh and includes the requested operation;
- the local peer config policy revision matches the local call authority snapshot revision for this phase;
- the publisher's surface policy revision is stored as remote evidence for refresh/change detection only and is never compared for equality against local Mother policy revisions;
- an outbound binding presentation issued by the remote Mother for the local Mother exists and verifies before the route is selected for forwarding;
- the payload is not secret-scoped;
- the route selected in phase 1 is revalidated immediately before the forwarding effect;
- the remote Mother admits the forwarding hello and call, then independently applies its own child/runtime authority.

Any missing fact denies. Secret-scoped material always eliminates the remote candidate with `SecretScopeForbidden`.

## Wire Contract

`MctHelloRequest.capability_view` and `MctHelloResponse.capability_view` are the wire carriers for publication. The request publishes the initiating Mother's view; the response from an admitting Mother publishes that Mother's own Vision-scoped view so an admitted hello in either direction can refresh both sides. This phase extends `MctHelloCapabilityView` with explicit callable-surface fields rather than encoding authority in strings:

- publisher `node_id`;
- `vision_id`;
- `published_at`;
- publisher `policy_revision`;
- existing advertised ALPN/WIT/observation summary fields;
- `callable_surfaces: Vec<MctHelloCallableSurface>`.

Each callable surface carries:

- `child_name`;
- canonical `operation_id` such as `patina:slate/control@0.1.0.list-work`;
- `runtime_kind` of the publisher's local child implementation;
- `vision_id`;
- `policy_revision` from the publisher's child approval/assignment surface;
- `visibility`, which must be `vision_scoped` for this phase.

The existing daemon federation view remains the local builder. The Iroh/daemon boundary maps it into `MctHelloCapabilityView`; no Iroh type enters `mct-kernel`.

Wire compatibility for this phase is same-version Mothers only. The `mct/hello/0` protocol version is bumped so a populated capability view from a newer Mother is never silently misread by an older receiver; an unrecognized hello version yields the existing `UpgradeRequired` outcome rather than a whole-hello deserialization failure.

## Surface Storage and Refresh

Remote callable surfaces are dynamic runtime evidence, not operator-granted authority. They are stored in `MctRuntimeStateStore`, not as trusted child approvals in config.

Storage rules:

- Store only from a hello request whose kernel evaluation is `Admitted` after binding-signature enforcement, or from a hello response whose outcome is `Admitted` after the caller has verified the remote-issued outbound binding proof used for that exchange.
- For request-carried views, the hello evaluation's selected node and Vision must match the capability view node and Vision; for response-carried views, the configured remote peer node/Vision and the admitted/requested Vision must match the capability view node and Vision.
- Every stored surface must be `vision_scoped`, match the admitted Vision, have non-empty child/operation identifiers, and carry a policy revision.
- Refresh is atomic per `(peer_node_id, vision_id)`: a newer admitted view replaces the previous view for that peer and Vision, including deletion of operations no longer published.
- `received_at` is the local receive time. `stale_at = received_at + 300 seconds`.
- Route authority treats `now >= stale_at` as stale and eliminates any candidate that depends on that surface with `CapabilityUnavailable`.
- A stale surface becomes executable again only after the publishing Mother presents a fresh admitted hello request or response with a capability view. Because hello exchange is symmetric, a caller's outbound forwarding hello publishes the caller view to the executor and the admitted response refreshes the executor's view at the caller as a natural side effect.
- Publisher policy revisions are per-Mother counters. A newly admitted view whose publisher revision differs from the stored publisher revision replaces the stored view as change-detection evidence, but that remote revision is not compared against local call or binding revisions.

## Binding Proof Direction

The signature scheme stays unchanged, but route forwarding needs two directional binding facts:

- **local admission proof**: the existing peer binding proof issued by this Mother for the remote peer; used by local candidate authority before selecting a remote route;
- **outbound presentation proof**: a binding presentation issued by the remote Mother for this local Mother; used by the forwarding Mother when it opens `mct/hello/0` to the executor.

The outbound proof is stored as explicit peer config data, not inferred from the local admission proof. If it is absent, malformed, expired, revoked, or does not verify against the remote endpoint identity, the remote candidate is not executable.

## Forwarding Contract

For a selected remote route, the caller Mother:

1. records route selection/revalidation before the forwarding effect;
2. sends a fresh hello to the executor using the local federation capability view and the remote-issued outbound binding proof;
3. requires the hello response to be admitted and to include `mct/call/0`, then refreshes the executor's stored callable surfaces from the response capability view when present;
4. constructs a forwarded `MctCallProtocolRequest` using the same target, deadline, trace, payload metadata, payload handle, and idempotency intent, while presenting the forwarding Mother as the call-protocol caller because `mct/call/0` authority is node-bound to hello admission;
5. sends the request over `mct/call/0` with inline bytes only when the declared handle and existing `MCT_INLINE_PAYLOAD_MAX_BYTES` rules verify;
6. verifies the remote result payload with existing `MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES` rules;
7. maps remote `Success`, `Denied`, `Failed`, `TimedOut`, `Cancelled`, and `Malformed` reply classes into the local typed result/reply without exposing remote policy internals.

Content-addressed or external payload handles are not made magically remote-readable in this phase. If a remote route cannot provide verified inline bytes or a handle the executor can safely evaluate under the existing rules, it fails closed before child delivery.

## Observation Contract

Observations are required before protected effects. This phase adds no payload bytes to observations.

Caller Mother observations include:

- remote candidate considered/eliminated/admitted evidence;
- route selected and revalidated;
- `PeerCallSent` / forwarding-start evidence with `forwarded_from=<caller-node>` and `forwarded_to=<executor-node>`;
- remote reply/denial mapping evidence with outcome and remote decision/reply refs only.

Executor Mother observations include:

- hello/call protocol admission or denial;
- `PeerCallReceived` evidence with `forwarded_from=<caller-node>` and `executed_on=<executor-node>`;
- local route/child/runtime observations for the actual execution path;
- typed denial observations for malformed payload, unauthorized operation, stale/revoked binding, and no-route cases.

Observation detail strings may contain node IDs, candidate IDs, operation IDs, policy revision numbers, denial classes, and opaque audit refs. They must not contain payload bytes, base64 payloads, credentials, or secret values.

## Implementation Order

1. Failing tests for hello capability-view wire shape and population from federation view.
2. Populate `MctHelloRequest.capability_view` at all real hello construction sites that have daemon context.
3. Store and refresh admitted peer callable surfaces from hello requests and responses in runtime state.
4. Generate remote candidates from fresh stored surfaces and replace the intentional `CapabilityUnavailable` gap only in `resident_remote_candidate_authority` / remote candidate planning.
5. Forward selected remote calls over `mct/call/0` and map replies.
6. Add route-chain observations on caller and executor.
7. Add end-to-end two-Mother fail-closed tests.

## Verification

Use the `exit_criteria` commands above. Per implementation commit, also run:

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
```

Flakes may be rerun once. A second failure is real and must be fixed before the commit lands.

## Build Readiness

Ready for D1 review only. No implementation code should be written until this SPEC/DESIGN gate is approved.

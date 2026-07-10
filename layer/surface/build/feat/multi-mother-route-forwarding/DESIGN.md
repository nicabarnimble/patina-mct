# Design: Multi-Mother route forwarding

## Why This Design

The existing code already has the right safety spine:

- `MctPeerBinding` + `mct/hello/0` admit a peer only under signed binding authority.
- `mct/call/0` accepts calls only after hello admission and ALPN/endpoint/Vision revision checks.
- `RouteDecision` separates authority filtering from route selection and requires execution-time revalidation.
- The daemon's federation view already publishes the local callable surface using Ready + Approved + Active + Vision checks.

The missing link is not another trust model. The missing link is carrying the callable-surface evidence across hello, storing it as dynamic runtime evidence, and using it as one more required authority fact for remote route candidates.

## Design Summary

```text
Publisher Mother A
  local children/config -> federation capability view
  hello request capability_view -> mct/hello/0
  admitted hello response capability_view <- receiver's local view

Receiver Mother B
  admitted signed hello request/response -> store fresh peer surfaces in runtime state
  locally originated call -> route candidates from local children + fresh A surfaces
  forwarded mct/call/0 arrival -> local candidates only; terminal no-route if none survive
  RouteDecision selects RemotePeer candidate only for the locally originated call
  revalidate -> outbound hello to A -> mct/call/0 -> reply mapped to local result

Both Mothers
  observations reconstruct route/authority chain without payload bytes
```

Mother kernel decisions remain pure MCT records. Iroh remains the adapter that carries hello/call frames and bytes.

## Direct Code Targets

- `crates/mct-kernel/src/peer/mod.rs`
  - Extend `MctHelloCapabilityView` with typed callable-surface publication fields.
  - Add `MctHelloResponse.capability_view` for symmetric response publication.
  - Add a pure `MctHelloCallableSurface` record if needed.
  - Keep the type adapter-neutral: no Iroh, storage, Wasmtime, or daemon concrete types.

- `crates/mct-daemon/src/federation.rs`
  - Keep the existing local federation view builder as the source of callable surfaces.
  - Add a narrow mapper from `MctFederationCapabilityView` to `MctHelloCapabilityView`.

- `crates/mct-daemon/src/state.rs`
  - Add runtime-state storage for remote callable-surface views.
  - Store dynamic publication evidence separately from operator config authority.
  - Provide read APIs for fresh surfaces by peer/Vision/operation.

- `crates/mct-daemon/src/config.rs`
  - Preserve existing local peer binding authority.
  - Add explicit remote-issued outbound binding presentation data if needed for forwarding hellos.
  - Do not change the Ed25519 signature payload or verification scheme.

- `crates/mct-iroh/src/serve.rs`
  - Preserve signature enforcement at hello admission.
  - Keep bounded request/result payload verification on send and receive.
  - Surface admitted hello/call events to the daemon so the daemon can store surfaces and write observations.

- `crates/mct-daemon/src/main.rs`
  - Populate hello capability views at resident/CLI construction sites with daemon context.
  - Replace the remote candidate temporal gap in `resident_remote_candidate_plans` / `resident_remote_candidate_authority` only after stored surface authority exists.
  - Add forwarding execution for selected `RuntimeKind::RemotePeer` routes.
  - Add route-chain observation helpers.

## Data Model

### Hello capability publication

Add explicit fields to `MctHelloCapabilityView`:

```rust
pub struct MctHelloCapabilityView {
    pub node_id: MctNodeId,
    pub vision_id: VisionId,
    pub published_at: Timestamp,
    pub policy_revision: u64,
    pub supported_alpns: Vec<String>,
    pub supported_wit_worlds: Vec<String>,
    pub supported_observation_modes: Vec<String>,
    pub callable_surfaces: Vec<MctHelloCallableSurface>,
    pub capability_view_ref: Option<String>,
}

pub struct MctHelloCallableSurface {
    pub child_name: String,
    pub operation_id: String,
    pub runtime_kind: RuntimeKind,
    pub vision_id: VisionId,
    pub policy_revision: u64,
    pub visibility: String,
}
```

Exact field names may vary during implementation, but the shape must remain explicit and typed. `capability_view_ref` remains optional and non-authoritative. `MctHelloResponse` also carries `capability_view: Option<MctHelloCapabilityView>` so the admitting Mother returns its own current view during the same exchange.

### Stored remote surface

Store in runtime state, not config:

```text
remote_surface_views(
  peer_node_id,
  binding_id,
  endpoint_id,
  vision_id,
  publisher_policy_revision,
  published_at,
  received_at,
  stale_at,
  view_observation_id
)

remote_callable_surfaces(
  peer_node_id,
  vision_id,
  child_name,
  operation_id,
  runtime_kind,
  surface_policy_revision,
  visibility,
  received_at,
  stale_at
)
```

Primary keys should make refresh atomic for `(peer_node_id, vision_id)`. A refresh deletes prior surfaces for that peer/Vision before inserting the new set in one transaction.

### Outbound binding presentation

Forwarding needs a remote-issued proof for this local Mother. Keep it distinct from the existing local-issued binding proof:

```rust
pub struct MctOutboundPeerBindingPresentationConfig {
    pub binding_id: PeerBindingId,
    pub policy_revision: u64,
    pub signature_ref: String,
    pub expires_at: Option<Timestamp>,
}
```

Operator installation extends the existing `peers` command surface, e.g. `peers set-outbound-proof <peer-node-id> <binding-id> --signature-ref proof [--expires-at ts]`.

Candidate authority can locally verify this proof by constructing the same `MctPeerBinding` shape the remote Mother will check, with:

- issuer node/endpoint = remote peer;
- peer node/endpoint = local Mother;
- Vision = peer Vision;
- ALPNs = `mct/hello/0`, `mct/call/0`.

This uses the existing `verify_peer_binding_signature_ref` function and signature payload. It does not change the scheme.

## Candidate Generation

Remote candidates come from stored surfaces, not from every peer, and only for a call that originated on this Mother. The existing kernel `MctCall.origin` is the required fact: `Cli`, `JvmAdapter`, `WasmHost`, and `ProcessHarness` may enter remote candidate sourcing; `Iroh` means the call arrived over `mct/call/0` and is terminal.

At the daemon's local/remote plan merge seam, construct a private originated-call capability from `MctCall` only when its origin permits remote sourcing. Make `resident_remote_candidate_plans` require that capability rather than an unrestricted `&MctCall`. An `Iroh` arrival therefore cannot be represented as input to remote candidate sourcing; its remote-plan set is empty by construction before ranking or forwarding exists. Do not add a later forward-time guard, hop counter, or config switch.

For a call target, compute the canonical operation id. For each stored fresh surface with that operation:

1. find the matching peer config entry;
2. build `CandidateRoute` with:
   - `candidate_id = peer:<peer-node>:<binding-id>:<operation-id>:<child-name>`;
   - `node_id = peer.peer_node_id`;
   - `child_id = Some(child_name)` for audit, even though execution class is remote;
   - `runtime_kind = RuntimeKind::RemotePeer`;
   - `network_path` from ticket direct/relay facts;
3. evaluate authority in one shared function.

Authority checks return `CandidateAuthorityEvaluation::admissible` only when every invariant in the SPEC passes. Otherwise they return `eliminated` with the most specific existing reason:

- missing/invalid local or outbound binding proof -> `PeerNotAdmitted`;
- non-admitted/revoked/expired binding -> `PeerNotAdmitted`;
- ALPN missing -> `PeerNotAdmitted`;
- wrong Vision -> `VisionPolicyDenied`;
- local peer config revision vs local call authority snapshot mismatch -> `PolicyRevisionStale`;
- secret-scoped payload -> `SecretScopeForbidden`;
- stale/missing ticket/surface -> `CapabilityUnavailable`.

Do not add defensive fallback candidates. If the surface is absent, there is no remote candidate for that operation. Publisher surface policy revisions are stored as remote evidence for refresh/change detection only; they are not compared for equality against local Mother revisions.

## Route Selection and Revalidation

The existing ranking keeps local routes ahead of remote routes:

```text
Local < Direct remote < Relayed remote < Unknown
Wasm < Process < JVM < RemotePeer < Internal
```

That is acceptable for this phase. Optimization never grants authority; it only ranks candidates that survived filtering.

For remote routes, execution-time revalidation must happen immediately before the forwarding effect. If the current config/state no longer matches the selected remote candidate, deny before opening the outbound call stream. Revalidation does not implement loop prevention: the stronger invariant is that forwarded arrivals never have a remote route available to select.

## Forwarding Execution

Add a remote execution branch beside local process/WIT execution.

Inputs:

- selected `CandidateRoute` / `AuthorizedRouteExecution`;
- peer config entry and fresh stored surface used by the candidate;
- outbound binding presentation;
- original request payload handle and inline bytes, when present;
- resident endpoint capable of opening outbound Iroh streams.

Flow:

1. Append caller-side route/forwarding-start observations before network effect.
2. Build local capability view from current config/state/loaded children and attach it to outbound hello.
3. Send `mct/hello/0` to the executor, require `HelloOutcome::Admitted` + accepted `mct/call/0`, and refresh the executor's stored surfaces from the response capability view when present.
4. Build the forwarded `MctCallProtocolRequest`:
   - preserve call id, target, trace, deadline, authority snapshots, payload metadata, payload handle, and idempotency key where safe;
   - use the forwarding Mother's local node/Vision as `call.caller` for the remote protocol because the remote hello admitted the forwarding Mother, not the original upstream requester;
   - record original/upstream caller only in local observations for this phase.
5. Send `mct/call/0` with inline bytes only when `MCT_INLINE_PAYLOAD_MAX_BYTES` and digest checks pass.
6. Verify remote reply/result payload using the existing Iroh adapter result integrity code.
7. Map remote reply to local `MctIrohCallHandlerResult` / `MctResult`:
   - success -> completed with remote result handle/inline bytes;
   - denied -> denied, no route in caller-safe projection;
   - malformed -> failed or denied with caller-safe `malformed call payload` / `not authorized` as appropriate;
   - failed/timed out/cancelled -> matching safe local outcome.

No payload bytes go into observations. Inline bytes may traverse `mct/call/0` only as protocol payload, bounded by existing constants.

Publication means “I execute this operation,” not “I broker this operation.” The executor therefore keeps the forwarded request's `CallOrigin::Iroh`, considers only local candidates, and returns its typed no-route denial when local availability disappears. This also limits caller rewriting to one hop: multi-hop would require end-to-end caller identity and explicit transitive policy and is deferred to ROADMAP item 6.

## Observation Details

Use existing observation kinds where possible:

- `CandidateConsidered`
- `CandidateEliminated`
- `RouteSelected`
- `RouteRevalidated`
- `PeerCallSent`
- `PeerCallReceived`
- `PeerCallReplied`
- `CallDenied`
- `RuntimeExecutionCompleted` / `RuntimeExecutionFailed`

If helper functions are useful, add them to `mct-kernel` only as pure observation projections over domain records. Otherwise construct adapter observations in `mct-daemon`.

Suggested safe detail refs:

```text
forwarded_from:<node>;forwarded_to:<node>;candidate:<candidate-id>;operation:<operation-id>
executed_on:<node>;forwarded_from:<node>;operation:<operation-id>
remote_reply:<outcome>;remote_decision:<decision-id>;remote_reply_id:<reply-id>
elimination_reason:<Reason>;denial_class:<structural|temporal>
```

Never include payload bytes, base64 payload strings, credentials, secret names/values, or raw child stdout/stderr.

## Tests First

Each behavior change starts with a failing test:

1. `hello_capability_view_carries_callable_surfaces`
2. `resident_hello_publishes_federation_callable_surface`
3. `admitted_hello_refreshes_peer_callable_surfaces`
4. `denied_or_wrong_vision_hello_does_not_refresh_surfaces`
5. `resident_remote_surface_candidate_becomes_admissible_when_all_checks_pass`
6. `resident_remote_surface_candidate_forbids_secret_scope`
7. `two_mother_forwards_selected_call_over_iroh_and_maps_reply`
8. `two_mother_forwarding_records_route_chain_without_payload_bytes`
9. `two_mother_wrong_vision_fails_closed`
10. `two_mother_revoked_or_expired_binding_fails_closed`
11. `two_mother_bad_payload_fails_closed`
12. `two_mother_unauthorized_operation_fails_closed`
13. `two_mother_remote_denial_fails_closed`
14. `hello_response_capability_view_refreshes_surfaces_on_caller`
15. `forwarded_arrival_with_unavailable_local_candidate_is_terminal`
16. `two_mother_mutual_publication_with_unready_children_terminates_single_hop`

## Commit Plan After D1

1. `feat(iroh): populate hello capability_view from the federation view`
2. `feat(daemon): receive, store, and refresh peer callable surfaces`
3. `feat(daemon): generate executable remote candidates from stored surfaces`
4. `feat(iroh): forward selected calls over mct/call/0 and map the remote reply into a local typed result`
5. `feat: record forwarded-from on the caller, executed-on on the executor, and typed denial observations`
6. `test: end-to-end wrong-Vision, revoked/expired-binding, bad-payload, unauthorized-operation, and remote-denial paths fail closed`

If a step exposes an authority ambiguity not resolved by this design, stop and ask rather than adding fallback behavior.

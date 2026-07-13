# Resident decomposition seam design

Status: **R1 operator gate**  
Behavior baseline: `97e3041`  
Code subject: `crates/mct-daemon/src/daemon/resident.rs` (6,740 lines)  
Verification baseline: **290 passed + ignored** (290 passed, 0 ignored)

No code may move until this design is approved.

## Design reading

The resident file is not one component. It is a call pipeline, a resident process host, and a set of binary-wide adapters that accumulated in one mechanical landing zone during S2.5. Its behavior currently follows this order:

1. serving acquires the single ledger writer, establishes identity, binds the endpoint, loads children/state/config, publishes local capability facts, and starts control and event tasks;
2. transport validates the wire request and current peer authority before invoking resident code;
3. resident resolves and verifies locally dereferenced payload bytes;
4. idempotency reserves, refuses, or replays against the caller-scoped durable entry;
5. candidate sourcing independently constructs local-child and eligible remote-peer plans;
6. decision filters authority, ranks only admissible candidates, records the initial decision, and performs local kernel route revalidation;
7. the pipeline makes route observations durable before either effect;
8. local execution applies the distinct revision guard, performs child delivery, captures the result, and projects `route_taken`; or forwarding re-reads peer/publication facts, records revalidation and send facts, performs hello/call, verifies/maps the reply, and closes the temporary endpoint;
9. idempotency records the terminal caller-safe reply.

That order is the behavior contract. Decomposition must not reorder any read, observation append, effect, reply mapping, endpoint close, or idempotency transition.

The code also showed three boundaries that the candidate-stage hint did not name explicitly:

- payload resolution is a security boundary because local CAS dereference is permitted for local adapters and forbidden for peer arrivals;
- idempotency is a durable stage around routing/execution, not serving miscellany;
- capability publication/refresh is shared by resident serving and outbound forwarding and should not belong to either one.

## Module tree

All modules remain binary-local under `crates/mct-daemon/src/daemon/resident/`. `resident/mod.rs` is a narrow façade, not a second implementation file.

```text
daemon/resident/
├── mod.rs          # declarations and the binary-local façade only
├── observation.rs  # single writer, durability adapter, Iroh observation sink
├── payload.rs      # request-byte resolution/integrity and payload/result helpers
├── publication.rs  # local hello view and admitted hello surface refresh
├── idempotency.rs  # caller scope, fingerprint, reserve/replay/refuse/complete
├── candidates.rs   # local and remote sourcing plus candidate authority facts
├── decision.rs     # admissible-only rank, initial decision, no-route, local revalidation
├── execution.rs    # local revision guard, delivery, child dispatch, result projection
├── forwarding.rs   # remote revalidation, outbound hello/call, reply verification/mapping
├── pipeline.rs     # exact stage ordering and before-effect observation barriers
└── serving.rs      # process bootstrap, endpoint/control/event tasks, shutdown/status
```

Two functions currently in `resident.rs` are control-plane-owned rather than resident-stage behavior: `serve_http_control_loop` and `serve_http_control_loop_until` move to existing `daemon/control.rs`. `spawn_resident_control_task` remains in `serving.rs` because it composes the resident lifetime. No other existing daemon sibling is folded into the new tree.

### Dependency direction

```text
serving ───────────────▶ pipeline ─────▶ payload
   │                        │           idempotency
   ├──▶ publication         │           decision ─────▶ candidates
   └──▶ observation         │           execution ────▶ payload
                            └──────────▶ forwarding ──▶ publication, payload

all effect-owning stages ─────────────────────────────▶ observation::ResidentLedgerWriter
```

There is no dependency from candidates back to decision, from execution/forwarding back to pipeline, or from an adapter stage into serving. No new trait is introduced: each boundary has one concrete record/enum and ordinary functions. Existing closure-based idempotent execution remains a generic function rather than becoming a trait.

## Binary-local façade

`resident/mod.rs` exposes only the items already needed by `main.rs`, `daemon/control.rs`, and `daemon/ingress.rs`:

- `run_serve`;
- `ResidentLedgerWriter` and the Iroh sink constructor;
- `ResidentStatusSource` for the concrete control snapshot source;
- `ResidentRuntimePaths`, `ResidentPayloadIngress`, and `execute_resident_call`;
- the standalone-server idempotency wrapper used by `iroh serve` and `serve-process`;
- the payload digest helper used to construct the JVM request;
- the local hello capability-view helper used by `iroh call-peer`;
- the standalone HTTP control loop after it moves to `daemon/control.rs` is no longer exported by resident.

All other stage functions and records are restricted to the resident tree. Fields crossing the façade are private where constructors can preserve an invariant. Test-only failure constructors replace direct access to `ResidentLedgerWriter.sender` from sibling tests.

The pipeline's outer contract continues to use existing boundary types:

```text
(ResidentRuntimePaths,
 ResidentLedgerWriter,
 MctCallProtocolRequest,
 ResidentPayloadIngress)
    -> MctIrohCallHandlerResult
```

`MctCallProtocolRequest` and `MctIrohCallHandlerResult` remain the semantic/wire adapter boundaries. `CandidateRoute`, `RouteDecision`, `AuthorizedRouteExecution`, `MctCallPayloadHandle`, and `RouteTaken` remain the kernel-owned authority/projection records. Resident records may carry these types but must not duplicate their fields as a second source of truth.

## Stage contracts

### Payload

`ResidentPayloadIngress` is an enum with `Local { inline_payload }` and `Remote { inline_payload }` variants. This replaces the independent `allow_local_content_addressed_blob` boolean, so a peer arrival cannot accidentally be constructed with local-CAS dereference authority. Resolution returns a private `VerifiedRequestPayload` newtype; only payload code can construct it after the existing integrity path. `PayloadFailure` carries the unchanged caller-safe message and observations to the pipeline.

### Candidates and decision

`candidates` returns `LocalCandidatePlan` and `RemoteCandidatePlan` values. They carry kernel `CandidateRoute` and `CandidateAuthorityEvaluation`; the local plan additionally carries the loaded child and kernel child-authority result needed for local revalidation. `RemoteCandidateSource` stays a private constructor-checked newtype around `&MctCall`, preserving the rule that only origins for which `allows_remote_candidate_sourcing()` is true can source peers.

`decision` consumes those plans and returns one `RouteDisposition` enum:

- `Denied { decision, observations }`;
- `Local { plan: LocalExecutionPlan, observations }`;
- `Remote { plan: RemoteExecutionPlan, observations }`.

This makes the observation batch explicit at the pipeline boundary. `LocalExecutionPlan` carries the loaded child, kernel `AuthorizedRouteExecution`, and child-authority observation reference. It does not carry a second `RouteTaken`; execution derives that projection from the authorized kernel route. `RemoteExecutionPlan` keeps the selected `CandidateRoute` beside the initial `RouteDecision`, but construction is private and checks that the decision selected that same route; the kernel has no remote execution capability token, so this is the minimal adapter-owned invariant.

### Execution and forwarding

`execution` accepts only `LocalExecutionPlan` plus the request, verified bytes, runtime paths, and freshly read revision snapshot. It owns the distinct effect-boundary revision guard, process/WIT delivery, run-state writes, payload fact projection, and caller-safe result mapping.

`forwarding` accepts only `RemoteExecutionPlan`. Before egress it reloads config/state/publication facts, creates a private `RemoteRevalidation`, durably records the revalidation decision, and proceeds only with `RevalidatedRemoteRoute`. It then preserves the exact endpoint bind/check, send-observation barrier, hello, surface refresh, terminal forwarded request, reply observation, reply mapping, and close order found at the baseline.

### Observation

Only writer mechanics, durability mapping, the mandatory Iroh sink, and generic endpoint lifecycle projection are shared. Candidate, payload, idempotency, execution, and forwarding observation constructors stay with the stage that owns their typed fact. There is no catch-all projection module and no duplicated observation constructor.

## Disposition of all 15 `Resident*` structs

The “current crossings” column records every production boundary crossing plus out-of-module test use found at the baseline. Test construction sites follow the owning subject when tests move.

| Current record | Current crossings | Disposition | Approved shape and visibility |
|---|---|---|---|
| `ResidentMotherConfig` | `run_serve` constructs it; `run_resident_mother` consumes it; resident integration tests construct it. | **Stage internal** | Keep the name and fields; private to `serving`. It is process bootstrap configuration, not a call-stage contract. |
| `ResidentStatusSource` | `serving` constructs/updates it; `spawn_resident_control_task` and both control transports pass it; `daemon/control.rs` stores and reads it in `ControlSnapshotSource`; status test constructs it. | **Stage interface** | Owned by `serving`, concrete and binary-visible only through the resident façade for `control`. Keep the name; no trait. Fields become private and serving constructors/update methods preserve snapshot ownership. |
| `ResidentLedgerWriter` | Constructed by resident serving, JVM/standalone Iroh ingress, control tests, and resident tests; passed to control mutation handlers, observation sink/event recording, idempotency, pipeline, execution observation barriers, and forwarding. | **Stage interface** | Owned by `observation`, re-exported binary-locally with the same name. Keep `spawn`, `append`, durability append, and `close`; make `sender` private and add only a `cfg(test)` failure constructor. |
| `ResidentLedgerWrite` | Created and consumed only inside `ResidentLedgerWriter::{spawn,append_with_durability}`. | **Stage internal** | Move unchanged to `observation` and make it private. It is queue protocol implementation, not a module API. |
| `ResidentExecutionPaths` | Constructed by serving, JVM ingress, control route test, and resident tests; passed through payload resolution, pipeline, candidate authorization, current-revision read, local execution, remote revalidation, forwarding, and local capability projection. | **Restructure — rename** | Rename to `ResidentRuntimePaths`, keep the three paths as one immutable façade/stage-interface value, and expose read-only accessors. The old name is false: the value locates authority, publication, payload, and execution state. Do not split it into partially initialized path bags. |
| `ResidentRequestPayload` | Constructed by serving, JVM ingress, and tests; consumed by payload resolution before idempotency/routing. | **Restructure — replace with enum** | Replace with `ResidentPayloadIngress::{Local, Remote} { inline_payload }`. This removes the independent CAS-permission boolean and makes local dereference authority construction-specific. Binary façade to `ingress`; payload/pipeline otherwise. |
| `ResidentAuthorizedExecution` | Constructed by `decision` after kernel revalidation; carried by `ResidentAuthorizationOutcome` through pipeline; consumed by local execution. | **Restructure — rename and narrow** | Rename to `LocalExecutionPlan`. Stage interface `decision -> pipeline -> execution`. Remove duplicated `route_taken`; move `route_observations` to `RouteDisposition`; retain loaded child, `AuthorizedRouteExecution`, and child-authority observation ID. Fields private outside decision/execution. |
| `ResidentAuthorizedRemoteExecution` | Constructed by `decision` after initial remote selection; carried through pipeline; read by remote revalidation and consumed by forwarding. | **Restructure — rename and narrow** | Rename to `RemoteExecutionPlan`. Stage interface `decision -> pipeline -> forwarding`. Move `route_observations` to `RouteDisposition`; retain selected `CandidateRoute` and initial `RouteDecision` behind a private invariant-preserving constructor. |
| `ResidentChildExecution` | Constructed inside local execution after consuming `AuthorizedRouteExecution`; passed to process or WIT dispatch. | **Restructure — rename, stage internal** | Rename to `PreparedChildExecution`, private to `execution`. It is a post-guard dispatch record, not authorization. Keep only loaded child, consumed `AuthorizedChildInvocation`, authority observation ID, derived route projection, and decision ID. |
| `ResidentExecutionReport` | Returned by revision denial, delivery failure, process dispatch, and WIT dispatch; enriched/persisted by local execution; mapped by pipeline to `MctIrohCallHandlerResult`. | **Restructure — rename** | Rename to `LocalExecutionReport`. Stage interface `execution -> pipeline`; unchanged semantic fields: `MctResult`, observations, optional inline result bytes. The name distinguishes it from remote protocol replies. |
| `ResidentPayloadResolutionFailure` | Returned by payload fetch/integrity helpers; consumed only by pipeline to append observations and return the safe failure. | **Restructure — rename** | Rename to `PayloadFailure`. Stage interface `payload -> pipeline`, with private fields and accessors/consuming split; message and observations remain byte-for-byte unchanged. |
| `ResidentCandidatePlan` | Constructed during local sourcing; inspected for candidate observations, authority filtering/ranking, local selection, and kernel child revalidation. | **Restructure — rename** | Rename to `LocalCandidatePlan`. Stage interface `candidates -> decision`; retain loaded child, kernel `CandidateRoute`, `CandidateAuthorityEvaluation`, and `ChildCallAuthorityResult`. No duplicate authority fields. |
| `ResidentRemoteCandidatePlan` | Constructed during persisted remote-surface sourcing; inspected for observations, filtering/ranking, and remote selection. | **Restructure — rename** | Rename to `RemoteCandidatePlan`. Stage interface `candidates -> decision`; retain only kernel candidate and authority evaluation. |
| `ResidentRemoteCandidateSource<'a>` | Constructed from `MctCall` only when origin allows remote sourcing; consumed by remote sourcing; test helper uses the same constructor. | **Stage internal** | Keep the name and constructor-check, private to `candidates`. It is the representation of terminal peer-arrival policy and prevents an unchecked remote-source call. |
| `ResidentRemoteRevalidationAuthorized` | Constructed only after current config/surface/authority revalidation; carried by `ResidentRemoteRevalidation::Authorized`; consumed by forwarding for endpoint/hello/call. | **Restructure — rename, stage internal** | Rename to `RevalidatedRemoteRoute`, private to `forwarding`. Retain the revalidation `RouteDecision`, peer, local identity, optional capability view, and derived `RouteTaken`; only the authorized enum variant can expose it to the effect path. |

Related enums are part of the same API cleanup even though they are not in the 15-struct inventory: `ResidentAuthorizationOutcome` becomes `RouteDisposition`, `ResidentSelectedCandidate` becomes private `SelectedCandidate`, and `ResidentRemoteRevalidation` becomes private `RemoteRevalidation`. These renames are approved by this SPEC; no other production rename is implied.

## Library promotions

**Proposed promotions: none. Expected mct-daemon library public-surface delta: zero.**

| Candidate | Decision | Justification |
|---|---|---|
| Forwarding client | Keep binary-local in `resident/forwarding.rs`. | The code is not a general peer-client capability: it reloads daemon config/state, applies resident bilateral/publication policy, writes resident observations, constructs per-hop identity, and maps into resident route outcomes. `mct-iroh::MotherIrohEndpoint` already supplies the reusable transport client. Promoting this orchestration would leak daemon policy into a library adapter. |
| Ledger writer | Keep binary-local. | It hard-codes the resident ledger identity and coordinates daemon control/ingress ownership; the reusable JSONL ledger already lives in `mct-observation`. |
| Candidate/decision records | Keep binary-local. | Kernel `CandidateRoute`, `RouteDecision`, and `AuthorizedRouteExecution` are the reusable authority contracts; resident plans only pair them with loaded-child/config adapter state. |
| Payload ingress/resolution | Keep binary-local. | Kernel owns payload handles and integrity decisions; local CAS permission and byte retrieval are daemon adapter policy. |

Any compiler pressure for a new `pub` item in `crates/mct-daemon/src/lib.rs` or another library crate is a stop condition, not permission to promote it.

## Test and fixture plan

All test function names remain unchanged. Tests move with their subject, so the workspace count remains **290 passed + ignored**. Assertions, literal IDs, timestamps, expected observation order, payload bytes, and wire outcomes are not weakened or normalized.

| Destination | Existing tests moving there |
|---|---|
| `resident::observation::tests` | `resident_hello_observations_are_durable_before_responses` |
| `resident::publication::tests` | `admitted_hello_refreshes_peer_callable_surfaces`; `hello_response_capability_view_refreshes_surfaces_on_caller`; `denied_or_wrong_vision_hello_does_not_refresh_surfaces` |
| `resident::idempotency::tests` | `in_flight_idempotency_duplicate_refuses_without_second_execution`; `resident_idempotency_replays_scopes_refuses_and_expires_without_payload_leakage` |
| `resident::candidates::tests` | `resident_authorized_unavailable_is_temporal_no_route`; `resident_remote_surface_candidate_becomes_admissible_when_all_checks_pass`; `eligible_route_candidate_requires_every_current_conjunct`; `capability_offer_lapses_at_freshness_boundary`; `resident_remote_surface_candidate_forbids_secret_scope`; `resident_remote_route_candidates_reject_unsigned_peer_binding`; `two_mother_wrong_vision_fails_closed`; `two_mother_revoked_or_expired_binding_fails_closed`; `two_mother_unauthorized_operation_fails_closed` |
| `resident::decision::tests` | `resident_route_optimization_cannot_grant_authority`; `resident_no_route_records_specific_elimination`; `forwarded_arrival_with_unavailable_local_candidate_is_terminal` |
| `resident::payload::tests` | `resident_local_blob_payload_delivery_returns_digest_and_keeps_ledger_byte_free`; `resident_local_blob_absent_fails_closed_before_delivery`; `resident_local_blob_tamper_fails_closed_via_digest_mismatch` |
| `resident::execution::tests` | `resident_process_payload_delivery_returns_digest_and_keeps_ledger_byte_free`; `resident_wit_rejects_non_json_payload_before_execution`; `resident_execution_runs_wit_child_and_records_trace`; `resident_route_revision_guard_denies_before_effect`; `route_taken_projection_follows_outcome_matrix`; `cancelled_result_and_reply_hide_route_while_ledger_keeps_selection` |
| `resident::forwarding::tests` | `two_mother_forwards_selected_call_over_iroh_and_maps_reply`; `two_mother_forwarding_denies_when_executor_revokes_binding_after_hello`; `two_mother_mutual_publication_with_unready_children_terminates_single_hop`; `forwarded_envelope_clears_upstream_user_identity`; `two_mother_bad_payload_fails_closed`; `two_mother_remote_denial_fails_closed` |
| `resident::pipeline::tests` | `jvm_bridge_json_call_enters_resident_route_path` |
| `resident::serving::tests` | `first_boot_identity_is_durable_and_secret_free`; `bootstrap_identity_append_failure_leaves_no_identity_effect`; `resident_mother_serves_peer_control_and_shutdown`; `resident_hello_publishes_federation_callable_surface`; `resident_mother_rejects_unsigned_peer_binding`; `resident_mother_payload_roundtrip_verifies_result_digest`; `resident_status_source_reflects_closed_endpoint` |
| existing `daemon/control.rs::tests` | `control_snapshot_unopenable_state_projects_error_response` moves to its actual owner; `live_child_revocation_is_visible_to_the_immediately_following_route` stops reaching into decision records and exercises the unchanged pipeline outcome after the mutation. |
| existing `daemon/cli_admin.rs::tests` | `authorize_secret_cli_persists_scoped_grant_without_value` moves to its command owner. |
| existing `daemon/cli_runtime.rs::tests` | `authorize_cli_toy_denies_expired_grant_against_current_time` moves to its authority-helper owner. |

### Focused fixtures

The present all-purpose test module is not recreated as `resident/test_support.rs`.

- `candidates::tests::CandidateFixture` owns only signed bilateral config, one persisted remote surface, state, and a call. It is the narrowed successor to `RemoteSurfaceCandidateFixture`.
- `forwarding::tests::ForwardingFixture` owns only the two peer identities/bindings/tickets and forwarding ledgers needed for an outbound call. It does not create local child execution fixtures unless that specific test requires the terminal receiver.
- `serving::tests::ResidentHarness` owns endpoint startup/shutdown, UDS, and status polling for full-process tests.
- `payload::tests::PayloadFixture` owns a local CAS plus one approved process child.
- `execution::tests::ExecutionFixture` owns one approved process or WIT child and run-state paths.
- `idempotency::tests::IdempotencyFixture` owns only state, ledger, request, and the counting child needed by the integrated replay test.
- Small call/protocol constructors live in the test module whose contract they express. Child manifest/script writers are duplicated only where ownership differs rather than exposed through one broad fixture API.
- The three non-resident tests build focused local fixtures in their actual owner modules. No production item becomes public for a test.

Fixture refactoring may remove setup duplication within one stage, but every existing assertion remains. It does not add sleeps, loosen ordering checks, replace exact outcomes with broad matches, or turn end-to-end tests into mocks.

## Ledger relocation map

Every cited path below is updated in `layer/surface/build/spec-drift-audit/track3/LEDGER.md` in the same commit that moves the test. Unlisted resident tests have no Track 3 citation.

| New test module | Cited test names |
|---|---|
| `resident::publication::tests` | `hello_response_capability_view_refreshes_surfaces_on_caller` |
| `resident::idempotency::tests` | `resident_idempotency_replays_scopes_refuses_and_expires_without_payload_leakage`; `in_flight_idempotency_duplicate_refuses_without_second_execution` |
| `resident::serving::tests` | `resident_mother_payload_roundtrip_verifies_result_digest`; `resident_mother_rejects_unsigned_peer_binding` |
| `resident::decision::tests` | `resident_route_optimization_cannot_grant_authority`; `resident_no_route_records_specific_elimination`; `forwarded_arrival_with_unavailable_local_candidate_is_terminal` |
| `resident::execution::tests` | `resident_route_revision_guard_denies_before_effect`; `route_taken_projection_follows_outcome_matrix`; `cancelled_result_and_reply_hide_route_while_ledger_keeps_selection` |
| `resident::observation::tests` | `resident_hello_observations_are_durable_before_responses` |
| `resident::candidates::tests` | `resident_remote_surface_candidate_becomes_admissible_when_all_checks_pass`; `eligible_route_candidate_requires_every_current_conjunct`; `resident_remote_surface_candidate_forbids_secret_scope`; `resident_remote_route_candidates_reject_unsigned_peer_binding`; `two_mother_wrong_vision_fails_closed`; `two_mother_revoked_or_expired_binding_fails_closed`; `two_mother_unauthorized_operation_fails_closed`; `capability_offer_lapses_at_freshness_boundary` |
| `resident::forwarding::tests` | `two_mother_forwards_selected_call_over_iroh_and_maps_reply`; `two_mother_forwarding_denies_when_executor_revokes_binding_after_hello`; `two_mother_mutual_publication_with_unready_children_terminates_single_hop`; `forwarded_envelope_clears_upstream_user_identity`; `two_mother_bad_payload_fails_closed`; `two_mother_remote_denial_fails_closed` |

The exact prefix becomes `mct_daemon_bin::resident::<stage>::tests::<name>`. The control, ingress, library, kernel, Iroh, and observation citations do not change.

## Planned extraction order after gate release

Each item is one scalpel commit, validated before the next:

1. `refactor(daemon): extract resident observation`
2. `refactor(daemon): extract resident payload`
3. `refactor(daemon): extract resident publication`
4. `refactor(daemon): extract resident idempotency`
5. `refactor(daemon): extract resident candidates`
6. `refactor(daemon): extract resident decision`
7. `refactor(daemon): extract resident execution`
8. `refactor(daemon): extract resident forwarding`
9. `refactor(daemon): extract resident pipeline`
10. `refactor(daemon): extract resident serving`

A test and every corresponding ledger citation move in the subject's commit. The façade and sibling-owned tests/functions move with the first commit that makes their owner clear; no “cleanup everything” commit is reserved for stale paths.

## Deliberate non-goals

- no semantic, outcome, observation, durability, wire, JSON, ID, timestamp, error-text, or ordering change;
- no new observation kind or changed observation projection;
- no kernel or Iroh contract change;
- no mct-daemon library public-surface addition;
- no trait abstraction;
- no forwarding-client promotion;
- no deduplication of content-type/digest logic beyond assigning each existing operation to `payload`;
- no fixes for discarded standalone-server errors, CLI parsing repetition, fixed CLI IDs, configurable ALPN scope, observation replication, cross-Mother replay, live identity rotation, or any other S2.5/Track 3 itch;
- no speculative API for brokered or transitive submission;
- no new tests and no renamed tests in this phase; only fixture split and relocation with unchanged assertions.

The only S2.5 itch resolved here is broad resident fixture ownership. All other itch-list entries remain follow-up work.

## Gate questions for the operator

Approval authorizes the module tree, dependency direction, all declared record/enum renames and restructures, the zero-promotion decision, test destinations, fixture split, and extraction order. Any requested change to those contracts is made in this SPEC before R2. Until approval, no Rust file or ledger citation moves.

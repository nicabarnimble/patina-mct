# Routing wired end-to-end

## Contract

Every valid call that reaches execution passes the route pipeline: initial two-phase route decision, execution-time route revalidation, then by-value consumption of `AuthorizedRouteExecution` at the child effect boundary. Local dispatch is the single-candidate case of the same path. If no candidate remains, or if route authority becomes stale before the effect, the call fails closed with typed no-route evidence and caller-safe `not authorized` text.

This phase is local-candidate only. It wires the existing kernel route model into daemon and Iroh paths; it does not add remote forwarding.

## Merged call order

The payload data plane validated order is extended as follows:

1. bounded transport/control read;
2. JSON decode;
3. inline base64 decode when present;
4. declared payload handle validation and declared cap check;
5. actual cap check;
6. adapter computes BLAKE3 digest and observed size, or local CAS fetch facts for local `ContentAddressedBlob`;
7. kernel payload-integrity decision compares declared and observed facts;
8. hello/call protocol authority for remote `mct/call/0` arrivals, or equivalent local admission facts for CLI/control initiated calls;
9. daemon sources all local route candidates for the call target and computes per-candidate authority evaluations;
10. initial route decision: authority filter over all candidates, then deterministic ranking of admissible survivors only;
11. selected route revalidation: re-run selected child and toy authority against current facts and mint `AuthorizedRouteExecution` only through `revalidate_route_for_execution`;
12. adapter effect-boundary revision guard compares the minted route token revisions with current revisions read immediately before execution;
13. resident delivery preflight, including child-kind/content-type compatibility;
14. effect execution through the selected runtime;
15. result payload serialization/capture, result cap check, hashing, reply-handle construction, and result recording.

Steps 9-12 subsume the current daemon shortcut in `authorize_resident_child`: authorization of a child remains required, but it is no longer the dispatch decision by itself. Child authorization becomes candidate authority evidence and selected-route revalidation evidence.

## Candidate sourcing and ranking

For this phase the daemon supplies local candidates only. For each candidate it provides:

- `candidate_id`, stable within the call, derived from the child identity/generation;
- local `node_id`;
- `child_id` when the route invokes a child;
- `runtime_kind` from the loaded child/runtime shape;
- `network_path = Local`;
- child authority evidence from the same facts currently used by `authorized_local_candidates_for_call`;
- readiness/availability facts for temporal no-route classification;
- policy and grants revisions from the current authority projection.

The implementation should share the existing `authorized_local_candidates_for_call` authority logic rather than fork it, but it must preserve evidence for eliminated candidates. A helper that returns per-candidate `CandidateAuthorityEvaluation` records plus selected loaded-child handles is acceptable; a helper that returns only survivors is insufficient for this phase.

Ranking is deterministic and non-authoritative. It runs only over admissible survivors and sorts by a stable local key: network path class, runtime-kind order, child id, then candidate id. With all candidates local today, this is mostly a deterministic tie-breaker. It cannot create authority because denied candidates are removed before ranking and never enter the planner input. D5 must prove this with the `OptimizationCannotGrantAuthority` adversarial ordering test: a ranking-preferred candidate eliminated by authority is not ranked, and the worse-ranked admissible candidate is selected.

## Consumption contract and revision guard

`AuthorizedRouteExecution` remains sealed and is consumed by value at the execution site. No new constructors, `Clone`, serde derives, or test-only mint paths are introduced.

The selected route is revalidated immediately before execution by calling `revalidate_route_for_execution` with:

- the original `MctCall`;
- the initial `RouteDecision`;
- fresh selected-child authority evidence;
- fresh toy-grant evidence for the selected child;
- adapter-minted revalidation IDs.

After revalidation succeeds, the adapter reads current policy/grants revisions from the same current authority projection/config snapshot it will execute under. It then compares those current revisions with `AuthorizedRouteExecution::policy_revision()` and `AuthorizedRouteExecution::grants_revision()` at the effect boundary. Any mismatch is terminal and passive: no retry, no fallback candidate, no grant request. The typed denial uses `CandidateEliminationReason::PolicyRevisionStale` or `CandidateEliminationReason::GrantsRevisionStale`, safe text `not authorized`, and no child execution.

This guard composes with revalidation. Revalidation proves the selected route was valid against the facts supplied to the kernel; the effect-boundary guard proves the adapter is still executing under those same revisions.

## Entry paths and result projection

Both entry paths use the same route execution function:

- local CLI/control initiated calls build or receive a valid `MctCallProtocolRequest`, resolve local payload bytes/CAS facts, then enter the route pipeline at local admission plus candidate sourcing;
- remote `mct/call/0` arrivals keep the existing transport/payload/hello/call authority gates, then enter the same route pipeline after `evaluation.is_accepted_for_routing()`.

`crates/mct-iroh/src/serve.rs` must no longer leave successful handled calls with `route_decision_id: None`. The call handler result should return the selected/no-route route decision id; serve stamps it onto the mutable `MctCallProtocolEvaluation` before constructing the reply.

`MctResult.route_taken` follows the product-map rule:

- present for `success`, `failed`, and `timed_out` outcomes because execution was attempted on a route;
- absent for `denied` and `cancelled` before execution;
- malformed adapter input is not an `MctResult`.

The peer reply surface carries the same caller-safe route projection as the result. `MctCallProtocolReply` gains `route_taken: Option<RouteTaken>` populated from the handler result: present for success/failed/timed-out replies, absent for denied/cancelled/malformed replies. Full route reasoning, eliminations, topology, and stale-revision details remain ledger-only.

## Denial classification

The route wire-up uses the existing candidate elimination enum and classifies it as follows:

| Reason | Class | Notes |
| --- | --- | --- |
| `DataPolicyDenied` | structural | data placement/classification authority denied |
| `VisionPolicyDenied` | structural | Vision policy denied |
| `PeerNotAdmitted` | structural | peer/caller admission authority absent |
| `ChildNotApproved` | structural | approval, assignment, export, or scope authority absent |
| `ToyGrantMissing` | structural | required toy authority absent, denied, expired, or revoked |
| `SecretScopeForbidden` | structural | secret scope/data authority denied |
| `PolicyRevisionStale` | structural | current policy changed; new request/revalidation required |
| `GrantsRevisionStale` | structural | current grant snapshot changed; new request/revalidation required |
| `RouteMismatch` | structural | selected route does not match revalidated facts |
| `CapabilityUnavailable` | temporal | authorized route cannot run now, e.g. child not ready, node maintenance, or local runtime unavailable |

Temporal classification does not imply retry in this phase. It only preserves the difference between "not allowed" and "allowed but unavailable" in the ledger. Denial remains terminal and passive unless a future explicit policy adds retry/grant-request/escalation capabilities.

## Observability mapping

The ledger must reconstruct every route outcome without payload bytes.

- Initial candidate consideration: `ObservationKind::CandidateConsidered` per candidate, with candidate id, call id, revisions, and no payload bytes.
- Candidate elimination: `ObservationKind::CandidateEliminated` per eliminated candidate, with the specific `CandidateEliminationReason` and structural/temporal class in detail text.
- Initial selection: existing `ObservationKind::RouteSelected` via `route_decision_observation` for selected initial decisions.
- Initial no-route: existing `ObservationKind::NoRouteRecorded` via `route_decision_observation`, plus per-candidate eliminations so the reason is never only generic no-route text.
- Revalidation success: existing `ObservationKind::RouteRevalidated` via `route_decision_observation`.
- Revalidation denial: existing `ObservationKind::NoRouteRecorded` with the revalidation decision id and specific stale/mismatch reason.
- Effect-boundary revision denial after token mint: `ObservationKind::NoRouteRecorded` with the selected route candidate id, `PolicyRevisionStale` or `GrantsRevisionStale`, current and minted revisions in detail text, and safe text `not authorized`.
- Result recording: existing result/runtime observations; `route_taken` follows the presence rule above.

Caller-safe projections stay concealment-safe. The ledger may contain candidate ids, rule classes, and revision numbers; replies and safe messages do not reveal internal topology or policy internals.

## Non-goals

- No remote candidates.
- No call forwarding between Mothers.
- No Iroh blob routing or blob transfer changes.
- No retry loop, grant-request path, escalation capability, or fallback execution.
- No new ranking policy language.
- No scheduler, load-balancing, telemetry, or health-scoring heuristics beyond local deterministic tie-breaking.
- No changes to sealed capability mechanics.
- No payload-byte observations or ledger payload storage.

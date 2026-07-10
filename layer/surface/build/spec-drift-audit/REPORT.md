# MCT product-map spec-drift audit

Date: 2026-07-09

Mode: audit only (`weed` check mode)

Baseline: branch `patina`, `bcb4778` (`docs: close multi-mother forwarding phase`), clean tree

## Scope and method

This audit compares `layer/allium/mct-product-map.allium` with the landed resident Mother, payload/CAS, route-wiring, typed-WIT, binding-signature, and single-hop forwarding implementation. It makes no code, specification, or test changes.

Direction terminology:

- **spec-ward**: preserve the product-map decision and change implementation later.
- **code-ward**: preserve landed behavior and update the product map later.
- **elicitation**: resolve the ontology or intended behavior at the operator gate before editing either side.

## Tooling baseline

Local Allium 3.5.0 was explicitly accepted by the operator after the original 3.2.3 expectation proved stale.

```text
$ allium --version
allium 3.5.0 (language versions: 1, 2, 3)
```

```json
$ allium check layer/allium/mct-product-map.allium
{
  "command": "check",
  "diagnostics": [],
  "findings": [],
  "spec_file": "layer/allium/mct-product-map.allium"
}
```

```json
$ allium analyse layer/allium/mct-product-map.allium
{
  "command": "analyse",
  "diagnostics": [],
  "findings": [],
  "spec_file": "layer/allium/mct-product-map.allium"
}
```

**Tooling finding T1 — local/CI Allium version skew.** The accepted local baseline is 3.5.0, while `scripts/install-allium-ci.sh:4-5` defaults CI to Allium 3.2.3 and its matching checksum. This can make local and CI syntax/analysis behavior differ. **Direction: code-ward** — bump the CI pin and checksum in a separate gate-time commit; do not combine it with this audit.

## Class A — code violates a product-map decision or invariant

### A1 — `CallOrigin` is permission-bearing

- **Spec:** `MctCallAtomicity.OriginIsForObservationNotPermission`, `layer/allium/mct-product-map.allium:599,682-686`.
- **Code:** `crates/mct-kernel/src/call/mod.rs:127-155`; `crates/mct-daemon/src/main.rs:2184-2194,2267-2272`.
- **Evidence:** The map says, “`origin` records which adapter produced the call for audit and telemetry, but must not create adapter-specific routing authority.” `CallOrigin::allows_remote_candidate_sourcing` grants remote-candidate sourcing to CLI/JVM/WASM/process origins and denies it to Iroh origin; `ResidentRemoteCandidateSource` is constructible only through that predicate at the candidate merge seam. Origin therefore changes the set of routes the call may receive, not merely its audit projection. The single-hop behavior is intentional and should remain Class A until the operator reconciles the contradictory map statement.
- **Direction:** code-ward.

### A2 — an admitted hello is reused without current binding revalidation

- **Spec:** `MctCallProtocol.HelloDoesNotPreAuthorizeCall`, `layer/allium/mct-product-map.allium:692-698,826-840`; execution revalidation, `layer/allium/mct-product-map.allium:131,146-147`.
- **Code:** `crates/mct-iroh/src/serve.rs:83-114,818-820,880-939`; `crates/mct-kernel/src/call/internal.rs:148-227`.
- **Evidence:** The map says every peer call still passes “the normal MCT authority filter” and execution-time revalidation, including peer binding authority. The server loads current bindings for each connection, but the call branch does not use them; it retrieves a remembered hello solely by endpoint. Kernel call evaluation compares the request with that cached hello and never receives current bindings or current time. A binding revoked or expired after hello can therefore continue admitting calls until the remembered hello is evicted or replaced.
- **Direction:** spec-ward.
- **Outcome:** fixed in `5f8f1af` (`fix(kernel): evaluate calls against current binding authority`); current bindings, policy revision, admitted revision, and current time are now mandatory call-evaluation facts.

### A3 — request idempotency is declared but not implemented

- **Spec:** `MctCallProtocol.IdempotencyIsRequestScoped`, `layer/allium/mct-product-map.allium:842-843`.
- **Code:** `crates/mct-kernel/src/call/mod.rs:592-634`; `crates/mct-daemon/src/main.rs:3189-3213`.
- **Evidence:** The invariant says idempotency keys “deduplicate retry of the same protocol request.” Production code only validates that the optional string is nonblank and copies it when forwarding; there is no lookup, reservation, replay, or duplicate-result path. Repeating the same request/key can execute the child effect again.
- **Direction:** spec-ward.

### A4 — `RouteDecision` does not record phase-2 reasoning or snapshots

- **Spec:** route-decision record decision, `layer/allium/mct-product-map.allium:929-979`; `TwoPhaseRouting`, `layer/allium/mct-product-map.allium:133-148`.
- **Code:** `crates/mct-kernel/src/route.rs:144-198`; `crates/mct-daemon/src/main.rs:2273-2315,2744-2763`.
- **Evidence:** The map says `RouteDecision` records authority filtering, feasible routes, planner scoring, selected route, snapshot revisions, and revalidation chain. The Rust record has authority evaluations, one selected route, and one no-route reason, but no phase-1 survivor set, planner evaluations/ranks/reasons, or decision snapshot revisions. The daemon sorts admissible candidates using an unrecorded static tuple. Operators can see what was selected but cannot reconstruct the recorded phase-2 comparison or the capability/telemetry snapshot that justified it.
- **Direction:** spec-ward.

### A5 — hello admission observations are durable only after the response effect

- **Spec:** `MctHelloProtocol.HelloObservationsBeforeEffects`, `layer/allium/mct-product-map.allium:563-590`; `MctLocalFirstObservationLedger.AuthorityFactsAreDurableBeforeEffect`, `layer/allium/mct-product-map.allium:1319,1406-1407`.
- **Code:** `crates/mct-iroh/src/serve.rs:833-878,987-1010`; `crates/mct-daemon/src/main.rs:1529-1563`.
- **Evidence:** The map requires hello receipt, admission/denial, negotiation, and response facts before subsequent protected peer effects. The Iroh server remembers the admitted hello, writes and finishes the network response, waits for connection close, and only then emits `Served`; the daemon asynchronously turns that event into one ledger append. A subsequent call can use the remembered admission before the hello observation is durable, and a crash after sending the response can leave authority granted with no canonical hello fact.
- **Direction:** spec-ward.
- **Outcome:** fixed in `e16e59d` (`fix(iroh): persist hello authority before response`), with standalone coverage completed in `fd3cd3d` (`fix(iroh): observe peer call lifecycle`). The now-mandatory serving observation sink is backed by the owning path's single `ResidentLedgerWriter`; resident, `iroh serve`, and `iroh serve-process` synchronize admitted and denied hello evaluations before admission is remembered or response bytes are written. Append failure closes the hello without a response or remembered admission.

### A6 — authority/operator/storage mutations bypass the observation ledger

- **Spec:** `MctObservabilitySpine.AuthorityDecisionsAreObserved` and `.AdapterEffectsAreObserved`, `layer/allium/mct-product-map.allium:1135-1142,1200-1212`; observation matrix, `layer/allium/mct-product-map.allium:1223-1235`.
- **Code:** child approval/revocation writes at `crates/mct-daemon/src/config.rs:417-476` called from `crates/mct-daemon/src/daemon/cli_runtime.rs`; observed peer mutation orchestration at `crates/mct-daemon/src/daemon/control.rs` and CLI arbitration at `crates/mct-daemon/src/daemon/cli_admin.rs`; blob storage effect at `crates/mct-daemon/src/control.rs:442-467` and `crates/mct-daemon/src/blob_store.rs:126-174`.
- **Evidence:** The map says every grant/revoke/child-approval decision, every operator policy/approval/grant/peer action, and every storage write/failure produces a typed observation. Child authority commands and CAS publication can still return after mutation without the required typed ledger fact, so the ledger is not yet the source of truth for every A6 effect.
- **Direction:** spec-ward.
- **Outcome:** fully fixed across `393884f` (`fix(daemon): observe peer authority mutations`), `3b1fa34` (`fix(daemon): observe child and node authority mutations`), `abe3eb1` (`fix(daemon): observe registry and blob storage effects`), `57c6b21` (`fix(daemon): observe grant and state administration`), and `0fb06c3` (`fix(control): require observed blob mutation owner`). Live peer, child, grant, registry, composition, and blob mutations run through resident-only UDS handlers and await `BeforeEffect` decision appends before config/state/CAS/package effects; offline-capable CLI fallback holds the exclusive writer lock across the same ordering. Identity is intentionally offline-only, while first bootstrap opens the resident writer and records public identity before key/config creation. Append failure leaves protected effects untouched, apply failures add typed failure facts, and proofs, payload/base64 bytes, secret keys, and secret values are absent from observations. Sinkless blob publication is refused.

### A7 — reload stops the current generation before constructing the replacement

- **Spec:** `MctChildComponentLifecycle.ReplacementLoadsBeforeSwap` and `.FailedReplacementDoesNotPoisonCurrent`, `layer/allium/mct-product-map.allium:1514-1515,1687-1694`.
- **Code:** `crates/mct-daemon/src/lifecycle.rs:60-140`; persistence order at `crates/mct-daemon/src/main.rs:289-298`.
- **Evidence:** The map says, “Replacement loads before swap,” and requires a failed replacement not to invalidate the ready generation. `reload_configured_child` first transitions the existing instance to draining and stopped, then clones that stopped value into a new loading generation and marks it ready. The command persists both outputs afterward. This is the inverse order and offers no path that leaves the old ready generation intact if replacement construction/readiness fails.
- **Direction:** spec-ward.

### A8 — peer-call lifecycle observations are incomplete and emitted after reply

- **Spec:** `MctCallProtocol.PeerCallObservationsCoverLifecycle`, `layer/allium/mct-product-map.allium:697,826-850`; observation coverage, `layer/allium/mct-product-map.allium:1223-1233,1282-1308`.
- **Code:** mandatory typed lifecycle sink and serving order in `crates/mct-iroh/src/serve.rs`; resident ledger projection in `crates/mct-daemon/src/daemon/resident.rs`; standalone writer ownership in `crates/mct-daemon/src/daemon/ingress.rs`.
- **Evidence:** The map requires receipt, malformed rejection, call construction, authorization/denial, route/no-route, result recording, and reply observations. Route/no-route and execution observations remain owned by the existing resident handler; the transport now supplies the missing ingress, authority prefix, result, and reply facts without duplicating route evidence.
- **Direction:** spec-ward.
- **Outcome:** fixed in `fd3cd3d` (`fix(iroh): observe peer call lifecycle`). Every serving API requires one typed sink. Receipt/construction/authorization/denial/malformed prefixes are awaited with `BeforeEffect` durability; undecodable and oversized requests receive a safe malformed reply only after durable rejection, while append failure yields no response. Result and truthful post-send reply facts use buffered durability. Standalone servers acquire the exclusive ledger writer before endpoint bind, and serve-process no longer discards ledger/state write failures.

## Class B — the product map under-describes landed behavior

### B1 — payload integrity limits and local BLAKE3 CAS are now concrete

- **Spec:** payload separation and handles, `layer/allium/mct-product-map.allium:598,614-618,695,710-718`.
- **Code:** `crates/mct-iroh/src/serve.rs:30-32,396-447`; `crates/mct-daemon/src/blob_store.rs:11-12,71-188`; control-plane ingest at `crates/mct-daemon/src/control.rs:442-467`.
- **Evidence:** The map says bytes may be inline/content-addressed/external and models only approximate sizes and generic digest/reference fields. Landed behavior uses exact byte counts, BLAKE3 digests, 32 KiB inline request/result caps, a 96 KiB frame budget, and an 8 MiB local CAS with temp-write, size/digest verification, atomic rename, and a control-plane ingestion endpoint. These are externally observable payload semantics absent from the map.
- **Direction:** code-ward.

### B2 — a second revision guard exists at the local effect boundary

- **Spec:** execution revalidation, `layer/allium/mct-product-map.allium:131,146-147,930-931`.
- **Code:** `crates/mct-daemon/src/main.rs:2104-2118,2789-2799,3296-3337`.
- **Evidence:** The map requires authority revalidation but does not describe the landed split. The daemon first mints an `AuthorizedRouteExecution`, then rereads the current policy/grants revisions from the execution-side config snapshot and compares them with the token immediately before child execution. Either mismatch yields a typed terminal denial. That additional effect-boundary revision guard should be captured as part of route/revalidation semantics.
- **Direction:** code-ward.

### B3 — `route_taken` is a caller-safe `mct/call/0` reply projection

- **Spec:** `MctResult` route rule and call reply shape, `layer/allium/mct-product-map.allium:852-879,742-750`.
- **Code:** `crates/mct-kernel/src/call/mod.rs:772-822,955-985`; server projection at `crates/mct-iroh/src/serve.rs:945-971`.
- **Evidence:** The map gives `MctResult` a conditional `route_taken`, but its `MctCallProtocolReply` has no result-payload or route field. The landed wire reply carries both, validates that denied/cancelled/malformed outcomes cannot expose `route_taken`, and includes it for execution-attempt outcomes. This is a new caller-visible protocol projection rule.
- **Direction:** code-ward.

### B4 — binding proof has a concrete Ed25519 canonical-message format

- **Spec:** peer binding and opaque `signature_ref`, `layer/allium/mct-product-map.allium:223-229,248-259,427-435`.
- **Code:** `crates/mct-iroh/src/identity.rs:9-17,78-168`; admission enforcement at `crates/mct-iroh/src/serve.rs:450-499`.
- **Evidence:** The map treats signed tokens as evidence and exposes an opaque signature reference, but does not define the landed proof. Code uses prefix `mct-ed25519-binding-v1:`, verifies against the issuer's Iroh public key, and signs a canonical JSON payload covering binding/issuer/peer endpoint and node IDs, Vision, ALPNs, policy revision, and expiry. Missing, malformed, or invalid proofs become safe `CapabilityInvalid` hello denial when signature enforcement is enabled.
- **Direction:** code-ward.

### B5 — hello capability views carry expiring callable-surface evidence

- **Spec:** Vision-scoped capability publication and hello advertisement, `layer/allium/mct-product-map.allium:186-220,418,438-443`.
- **Code:** callable surface/view records at `crates/mct-kernel/src/peer/mod.rs:185-223`; publication filtering at `crates/mct-daemon/src/federation.rs:155-197`; five-minute freshness and storage at `crates/mct-daemon/src/main.rs:1150-1221` and `crates/mct-daemon/src/state.rs:910-1031`.
- **Evidence:** The map's hello view lists ALPNs, WIT worlds, observation modes, and an optional reference. Code additionally publishes each ready, approved, assigned, Vision-matching child operation with runtime and policy revisions; received views are stored transactionally and eligible for route sourcing for 300 seconds. This runtime-evidence model is substantially more specific than the map.
- **Direction:** code-ward.

## Class C — specified future behavior not yet built

### C1 — the full User/Node/Vision and data/compute authority spine remains future

- **Spec:** authority spine and multi-node additions, `layer/allium/mct-product-map.allium:79-83`; data/compute authority, `layer/allium/mct-product-map.allium:60-66`.
- **Code:** current durable authority config, `crates/mct-daemon/src/config.rs:15-25,27-84`; remote candidate checks, `crates/mct-daemon/src/main.rs:2518-2632`.
- **Evidence:** The map calls for users, memberships, Vision membership/guardrails, project/app identities, data movement policy, compute placement policy, and approval scoped by data class/toys/child version. Current config contains one local identity, child approvals/assignments, and peers; remote authority has same-Vision, binding, ALPN, revision, secret flag, operation, and ticket checks but no full membership or data/compute policy model. No landed work makes the intended authority spine stale.
- **Direction:** elicitation.

### C2 — NodeCapabilityProfile, NodeTelemetry, and environment-aware planning remain future

- **Spec:** `NodeProfileAndTelemetry`, `layer/allium/mct-product-map.allium:186-220`; two-phase planner inputs, `layer/allium/mct-product-map.allium:120-131`.
- **Code:** current capability publication, `crates/mct-daemon/src/federation.rs:30-42,96-107`; current planner key, `crates/mct-daemon/src/main.rs:2744-2763`.
- **Evidence:** There are no stable private `NodeCapabilityProfile` and live `NodeTelemetry` domain records. The current view exposes counts and callable surfaces, while phase 2 uses fixed network/runtime/child/id ordering rather than load, memory, health, RTT, throughput, data size, locality, or deadline estimates. The callable-surface slice is compatible with, but not a replacement for, the intended profile/telemetry split.
- **Direction:** elicitation.

### C3 — retry, grant request, and escalation no-route capabilities remain future

- **Spec:** `NoRouteDecision`, `layer/allium/mct-product-map.allium:150-179`.
- **Code:** current route outcomes, `crates/mct-kernel/src/route.rs:161-198,351-398`; no-route projection, `crates/mct-kernel/src/route.rs:618-641`.
- **Evidence:** The map explicitly makes denial passive by default and reserves deferred retry, scoped/time-bounded grant requests, and visible escalation for separately authorized capabilities. Code implements only route-selected/no-route and projects no-route to a caller-safe denial; it has no retry budget/TTL, `may-request-grant`, grant-response, or escalation state. This matches the map's passive default and leaves the active capabilities intentionally unbuilt.
- **Direction:** elicitation.

### C4 — thought, observation replication, and federation-control ALPNs remain future

- **Spec:** protocol catalog, `layer/allium/mct-product-map.allium:346-412`; hello ordering, `layer/allium/mct-product-map.allium:414-418`.
- **Code:** exported protocol constants, `crates/mct-kernel/src/peer/mod.rs:10-13`; server ALPN dispatch, `crates/mct-iroh/src/serve.rs:820-983`.
- **Evidence:** The map names `mct/thought/0`, `mct/observe/0`, and `mct/federation/0` as candidate protected protocols. Code defines and serves only `mct/hello/0` and `mct/call/0`; all other ALPNs are unsupported. The future protocol list remains consistent with landed work.
- **Direction:** elicitation.

### C5 — a true JVM-backed WIT child execution substrate remains future

- **Spec:** `JvmAsWitChild`, `layer/allium/mct-product-map.allium:1108-1133`.
- **Code:** runtime kinds, `crates/mct-kernel/src/call/mod.rs:223-237`; loaded-child publication mapping, `crates/mct-daemon/src/federation.rs:180-194`; JVM adapter origin path, `crates/mct-daemon/src/main.rs:4817-4848`.
- **Evidence:** The JVM JSON ingress translates requests into the common WIT-shaped call, but loaded children currently publish/route as process or WASM components; no resident loader produces a `JvmChild` execution candidate backed by jars/classpath/JVM lifecycle. The map's JVM-as-WIT-child substrate remains intended future work rather than stale design.
- **Direction:** elicitation.

## Class D — peer semantics that exist only in code

### D1 — publication means the publishing Mother executes locally

- **Spec:** capability publication describes Vision filtering but states no execution commitment, `layer/allium/mct-product-map.allium:194-220,418`.
- **Code:** publication source filter, `crates/mct-daemon/src/federation.rs:155-197`; candidate sourcing, `crates/mct-daemon/src/main.rs:2432-2471`.
- **Evidence:** Only operations of a locally loaded, ready, approved, actively assigned, same-Vision child are published. A consumer converts a fresh published operation directly into a route to that publishing peer. There is no broker/forwardable-capability distinction: publication is executable evidence that the publisher itself can run the operation. The product map does not state that peer-relationship commitment.
- **Direction:** elicitation.

### D2 — `mct/call/0` arrivals are terminal at the receiving Mother

- **Spec:** the map defines remote calls and adapter-neutral origin but has no single-hop or terminal-arrival rule, `layer/allium/mct-product-map.allium:592-599,692-698`.
- **Code:** origin capability, `crates/mct-kernel/src/call/mod.rs:127-155`; unrepresentable candidate-source seam, `crates/mct-daemon/src/main.rs:2184-2194,2267-2272`.
- **Evidence:** Calls constructed with `CallOrigin::Iroh` cannot produce `ResidentRemoteCandidateSource`; therefore remote plans never join local plans for a forwarded arrival. If local candidates are unavailable, the receiving Mother returns its existing no-route result rather than brokering another hop. This exact relationship semantics is absent from the map and also creates the A1 contradiction.
- **Direction:** elicitation.

### D3 — forwarding rewrites caller identity per hop

- **Spec:** the immutable call/caller model does not define forwarding identity projection, `layer/allium/mct-product-map.allium:592-640,672-686`.
- **Code:** forwarded request construction, `crates/mct-daemon/src/main.rs:3171-3218`; executor observation, `crates/mct-daemon/src/main.rs:1814-1860`.
- **Evidence:** Before forwarding, the daemon clones the call, replaces `caller.node_id` with the forwarding Mother's node, clears `user_id`, replaces Vision with the peer Vision, preserves project ID, and sets origin to Iroh while retaining the same call ID. The executor therefore authorizes and records the immediate forwarding Mother, not the end-to-end original caller; upstream caller context remains only on the originator's local ledger. The map has no stated per-hop accountability model.
- **Direction:** elicitation.

### D4 — executable peer routing requires two directional binding proofs

- **Spec:** the map models one explicit `MctPeerBinding` and one presented `signature_ref`, `layer/allium/mct-product-map.allium:223-259,427-435`.
- **Code:** address-book relationship shape, `crates/mct-daemon/src/config.rs:53-75`; remote authority chain, `crates/mct-daemon/src/main.rs:2518-2596`.
- **Evidence:** A configured relationship stores both the local Mother's signed admission of the peer (`binding_signature_ref`) and a distinct peer-issued outbound proof that the local Mother may be admitted by the peer (`outbound_binding`). Remote candidacy requires both signatures, both call ALPN scopes, outbound expiry, admitted local state, matching endpoint/binding evidence, and a ticket. This bilateral, directional relationship ontology is not represented by the product map's single binding concept.
- **Direction:** elicitation.

## Coverage notes

The audit also found implementation alignment, not divergence, in these walked areas:

- Endpoint ID remains transport identity; hello admission intersects endpoint, binding, Vision, ALPN, version, policy revision, and expiry with safe denial.
- Payload metadata/handle integrity is checked before execution, and result payload integrity is checked by the caller.
- Candidate authority filtering precedes the daemon's deterministic sort, and selected local and remote routes are revalidated before their external effect.
- Structural/temporal elimination classes are typed; `CapabilityUnavailable` is temporal and authority mismatches are structural.
- `MctResult` keeps caller-safe terminal outcomes and suppresses `route_taken` for denied/cancelled-before-execution outcomes.
- WIT namespace/interface/function remains the shared operation identity across Iroh, JVM ingress, process, and WASM paths.
- The JSONL ledger is append-only, hash-chained, locally sequenced, synchronized on before-effect appends, and queryable by call or trace.

## Summary

| ID | Class | Area | Resolution direction |
|---|---|---|---|
| A1 | A | Origin creates remote-routing permission | code-ward |
| A2 | A | Cached hello bypasses current binding revalidation | spec-ward — **fixed** in `5f8f1af` |
| A3 | A | Idempotency key does not deduplicate | spec-ward |
| A4 | A | RouteDecision omits planner/snapshot evidence | spec-ward |
| A5 | A | Hello observation follows response effect | spec-ward — **fixed** in `e16e59d` |
| A6 | A | Authority/operator/storage mutations are unobserved | spec-ward — **fixed** in `393884f`, `3b1fa34`, `abe3eb1`, `57c6b21`, and `0fb06c3` |
| A7 | A | Reload drains/stops before replacement readiness | spec-ward |
| A8 | A | Peer-call lifecycle observation coverage is incomplete | spec-ward — **fixed** in `fd3cd3d` |
| B1 | B | Payload caps and local BLAKE3 CAS are under-described | code-ward |
| B2 | B | Effect-boundary revision guard is under-described | code-ward |
| B3 | B | Reply `route_taken` projection is under-described | code-ward |
| B4 | B | Ed25519 binding proof format is under-described | code-ward |
| B5 | B | Expiring callable-surface evidence is under-described | code-ward |
| C1 | C | Full identity/Vision/data/compute authority is future | elicitation |
| C2 | C | Capability profiles, telemetry, environment planner are future | elicitation |
| C3 | C | Retry/grant-request/escalation are future | elicitation |
| C4 | C | Thought/observe/federation ALPNs are future | elicitation |
| C5 | C | JVM-backed WIT child substrate is future | elicitation |
| D1 | D | Publication commits the publisher to local execution | elicitation |
| D2 | D | Forwarded arrivals are terminal/single-hop | elicitation |
| D3 | D | Forwarding uses per-hop caller identity | elicitation |
| D4 | D | Peer routing requires bilateral directional proofs | elicitation |
| T1 | Tooling | Local Allium 3.5.0 vs CI pin 3.2.3 | separate gate-time commit |

Counts: **A = 8, B = 5, C = 5, D = 4; tooling = 1.**

## Unclassified items

None. Every evidence-backed divergence found in scope is classified above; aligned behavior is recorded separately in coverage notes.

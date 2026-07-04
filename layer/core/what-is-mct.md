---
id: what-is-mct
layer: core
status: active
created: 2026-06-09
revised: 2026-06-09
tags: [mct, product, mother, child, toy, authority, iroh, observations]
references: [mct-build-boundaries, spec-driven-design, adapter-pattern, safety-boundaries]
---

# What Is MCT

**Purpose:** The narrative answer to "what is MCT?" for humans reading this
repo. The semantic law is `layer/allium/mct-product-map.allium`; this document
explains it. Where they disagree, the Allium spec wins.

---

## Product statement

MCT is a whole-app, local-first Mother/Child/Toy runtime: a Mother carries
local authority, observation ledger, child runtime, ToyGrant checks, and peer
protocols in the same deployable node instead of treating a hidden SaaS
backend as the source of truth.

Iroh supplies public-key-addressed connectivity between nodes. MCT supplies
the application, runtime, and authority layer above it.

> Iroh connects peers. MCT connects authorized thoughts and effects.

## The name is the ontology

```text
Mother = the authority of one node
Child  = an application component that computes
Toy    = a host capability through which a child affects the world
```

**Mother** is a role, not a server process: the thing that decides. One Mother
per deployable node, owning five things — authority decisions (who may
connect, what may run, which effects are permitted), the observation ledger,
the child runtime, ToyGrant evaluation, and the peer protocols. There is
nothing above her: no hosted registry or cloud control plane holds the "real"
state. The node is sovereign and complete.

**Child** identity is WIT-shaped: namespace, interface, function. The design
center is a WASM component, but a child may be process-backed or JVM-backed —
the substrate is an execution detail recorded in route observations, never a
separate authority model. Authority only ever sees the WIT contract.
WIT-only children are valid; legacy lifecycle exports (`init`, `handle`,
`drain`, `tick`) are compatibility hooks, not identity.

**Toy** capabilities form a closed canonical catalog, and each toy is itself a
WIT contract identity. Toys are the only way children touch the world outside
their own memory.

Mother decides; children compute; toys effect.

## The core inversion: nothing is ambient

Most runtimes start from code having power and sandbox away the dangerous
parts. MCT starts from code having zero power. Every power a child exercises
must trace to an explicit, issued, scoped, time-bounded, revocable record:

- A manifest `needs` entry is a request. It grants nothing.
- Authority exists only as data: a `ToyGrant` names subject, canonical toy
  contract, Vision/Node/project scope, data classification, allowed actions,
  issuer, policy revision, and time bounds.
- Execution never uses a grant directly. Grants are evaluated against the
  current call, and what runs carries an `AuthorizedToyCall` (or
  `AuthorizedChildInvocation`) capability minted by that evaluation. These
  executable capabilities are unforgeable by construction: their fields are
  private, they are minted only by kernel evaluators, they are not cloned or
  deserialized as authority, and effect adapters reject stale policy/grant
  revisions before performing the effect.

Why so strict: the product is multi-node. A Mother runs other people's
components and accepts other Mothers' calls. That is only sane when authority
is inspectable data — listable, auditable, scopeable, expirable, revocable —
rather than booleans buried in runtime state.

## Child lifecycle: four separate facts

```text
ComponentArtifact  what this code IS      (immutable digest/version/WIT exports)
ChildApproval      may it be used?        (authority decision, scoped)
ChildAssignment    where may it run?      (binding to Vision/Node/project)
ChildInstance      what is running now?   (live generation, ready/degraded/...)
```

Existence is not permission; permission is not placement; placement is not
readiness. Each fact answers one question and is revocable independently.
Replacement generations load and verify before the old generation drains.

## Anatomy of a call

A peer Mother invokes `patina:slate/control@0.1.0#complete-work` on your node:

1. **Iroh connects.** QUIC, NAT traversal, relays — Iroh's problem. The
   connection proves exactly one thing: the peer holds that endpoint key.
2. **mct/hello/0.** The peer presents a binding. The kernel checks for an
   active `MctPeerBinding` tying that endpoint to an MCT node, a Vision, the
   requested ALPNs, under the current policy revision. Admission is narrow:
   only the negotiated version, Vision scope, and ALPN set in the decision.
   Denial is safe ("not authorized"); the precise reason stays in the ledger.
3. **mct/call/0.** The adapter constructs exactly one immutable `MctCall`:
   WIT-shaped target, caller identity, payload metadata (classification and
   size — routing never deserializes business data), authority snapshot,
   deadline, trace context. Malformed input gets a native adapter error and
   never becomes a call.
4. **Phase 1 — authority filter.** Pure set reduction, no ranking. Hello
   admission coverage, approved + assigned child exporting this interface,
   active toy grants, data policy. Empty feasible set → hard deny with a
   policy reason class, observed. Deny is the passive default; retry and
   grant-request are privileges requiring their own policy.
5. **Phase 2 — environment planner.** Rank survivors by capability, load,
   locality, data size, deadline; pick one. Optimization can never make an
   inadmissible route admissible. Fastest path wins only among authorized
   paths, and data authority has final say over placement.
6. **Revalidate, then execute.** Authority is rechecked at execution time —
   stale authority is a security bug; stale optimization is a performance
   miss. Every effect the child attempts passes through a toy gate.
7. **MctResult returns.** Closed outcome set — success, denied, failed,
   timed_out, cancelled — with a caller-safe message and opaque audit ref.
8. **Throughout:** every step emits typed `MctObservation` facts into an
   append-only, hash-chained, per-Mother ledger. Authority-critical
   observations are durable before their effect proceeds (fail closed). The
   whole story reconstructs from the ledger alone.

## Four records, four audiences

```text
MctCall        the immutable semantic unit       (adapter-neutral)
RouteDecision  the full two-phase reasoning      (operator-facing, internal)
MctResult      the terminal outcome              (caller-safe)
MctObservation the durable evidence              (audience-filtered projections)
```

Logs, metrics, traces, dashboards, and audit views are projections of the
observation stream — never the truth themselves.

## The layering

```text
Patina epistemic / orchestration / interfaces   (future consumers, above)
MCT: authority + runtime + protocols            (this repo)
Iroh: public-key-addressed connectivity         (substrate, below)
```

Belief internals, SDK authoring, and Clojure orchestration are explicitly out
of MCT scope — they sit above MCT as future consumers, not inside it. The
integrated Patina Mother (`~/Projects/Sandbox/AI/RUST/patina`) grew this
runtime in embryo alongside those layers; it is evidence and prior art for
MCT, never ontology. Existing children `slate-manager@0.2.0`,
`folder-watch-actor@0.1.0`, and `watch-null-sink@0.1.0` are required
compatibility fixtures.

## Multi-Mother and Vision

Multi-Mother exists to share selected data and compute across nodes under
explicit authority. Vision is the sharing boundary — it may represent a
tenant, institution, business unit, environment, compliance zone, data
domain, or application fabric; do not collapse it to "tenant" early. Nodes
are heterogeneous (a Raspberry Pi and a Mac Studio are both Mothers);
capability publication is per-Vision and policy-filtered; cross-Vision
sharing requires explicit grants. JVM ecosystems (banking is the adoption
example) integrate as WIT children, not as a second call model.

## What MCT is not

- not a generic plugin loader;
- not an Iroh fork;
- not a SaaS control plane;
- not a Belief/scry/assay runtime.

## Where the truth lives

- Semantic law: `layer/allium/mct-product-map.allium`
- Build discipline: [MCT Build Boundaries](./mct-build-boundaries.md)
- Design rationale narrative: `layer/sessions/` (esp. the 20260529 Allium
  foundation elicitation)
- Comparison against integrated Mother:
  `layer/surface/mct-vs-patina-mother-deep-dive-report.html`

# Mother Kernel Design — Rust Systems Lens

Slate: `mother-kernel-design`
Status: complete

## Context

This note captures the design foundation for the clean `patina-mct` Mother kernel. It is complete as a design artifact; implementation progress should be tracked by child slice Slates with binary proof gates, not by reopening this broad design Slate.

It is grounded in the feature map:

- `layer/surface/patina-mct.org`

And in the integrated Patina reference implementation:

- `/Users/nicabar/Projects/Sandbox/AI/RUST/patina`

This is not an extraction plan. The integrated repo is evidence and prior art. The new MCT product should rebuild the kernel intentionally.

## Design lens

The MCT design must lock into the project layer/core values and use them as gates, not slogans:

- `dependable-rust`: keep public interfaces small and stable; hide implementation details in private `internal` modules; each module must do one thing that can be stated in one sentence.
- `adapter-pattern`: define trait boundaries at external system edges only when the boundary is proven by real implementations; use honest signatures and domain types at the seam.
- `unix-philosophy`: compose focused tools/components instead of building a monolith; new responsibilities should become new modules/services, not flags on one god object.
- `safety-boundaries`: keep authority explicit, fail closed, and avoid surprising side effects.
- `scalpel-commits`: evolve MCT in small reviewable slices so design can adapt without boiling the ocean.

Under a Jon Gjengset-style Rust systems lens, the kernel should have:

- small public API surface;
- honest signatures that expose real dependencies;
- domain types instead of stringly configuration;
- explicit state machines instead of boolean soup;
- fail-closed error types instead of broad `anyhow` at the library boundary;
- adapter traits only where multiple real implementations exist;
- private implementation details behind a narrow module boundary;
- boring, reviewable failure modes.

This is how MCT stays modular: the kernel owns decisions; external systems sit behind adapters; child/toy/component semantics stay typed; implementation slices stay small enough to replace.

## Answer: what the Mother kernel is

The Mother kernel is the small typed authority core that decides:

1. whether a child package is valid enough to install;
2. whether an installed child is allowed to activate;
3. whether a child instance is allowed to receive a call;
4. whether a child is allowed to use a toy;
5. what lifecycle state a child is in;
6. what observation/audit event records the decision.

In short:

```text
kernel decides
adapters perform
daemon exposes
children execute
toys mediate effects
```

## Proposed shape

```text
mct-daemon
  transport / CLI / HTTP / UDS
    ↓
  API handlers
    ↓
  MotherKernel
    ├── child registry
    ├── lifecycle state machine
    ├── grant/capability evaluator
    ├── call router
    ├── toy broker
    └── observation/audit sink
        ↓
  adapters
    ├── process harness/runtime
    ├── JVM bridge adapter
    ├── WASM component runtime
    ├── SQLite store
    ├── secrets backend
    └── Iroh p2p transport
```

The kernel sits in the middle. It owns decisions. Runtime/store/network implementations sit outside it.

## Proposed Rust module boundary

A first crate/module shape could be:

```text
crates/mct-kernel/src/
  lib.rs
  child.rs
  toy.rs
  grant.rs
  lifecycle.rs
  call.rs
  observation.rs
  identity.rs
  error.rs
  internal/
    mod.rs
    registry.rs
    grants.rs
    routing.rs
    state.rs
```

Public `lib.rs` should curate only the stable surface. `internal/` should not be public.

Module gates:

| Module | One-sentence job | Public surface rule |
|--------|------------------|---------------------|
| `child` | Represent WASM/WASI child identity, manifest claims, and lifecycle-facing handles. | No runtime implementation types. |
| `toy` | Represent WIT/WASI host capability contracts and toy request/reply values. | No host implementation details. |
| `grant` | Decide whether a child may use a toy/capability for an operation. | Produces typed authorization tokens; no side effects. |
| `lifecycle` | Model allowed child/component state transitions. | Enum/state machine, no booleans-as-state. |
| `call` | Define the kernel-level semantic operation envelope. | Transport-independent: no HTTP, Iroh, Wasmtime, or JVM types. |
| `identity` | Define Mother/node/child/caller identity values. | Typed IDs, no raw strings past parsing. |
| `observation` | Define audit/observation events the kernel guarantees. | Capture schema only; storage policy is an adapter. |
| `error` | Expose fail-closed kernel errors. | Typed, non-exhaustive, operationally distinguishable. |

If a module cannot pass the one-sentence test, split it before implementation.

## Proposed core type sketch

```rust
pub struct MotherKernel<R, S, O> {
    runtimes: RuntimeRegistry<R>,
    store: S,
    observations: O,
    grants: GrantEngine,
}
```

The generic names here are illustrative, not final. The important point is that runtime, store, and observation effects are dependencies, not hidden globals.

A more concrete public surface could look like:

```rust
impl MotherKernel {
    pub fn install_child(&mut self, package: ChildPackage) -> Result<ChildId, KernelError>;

    pub fn activate_child(&mut self, child: ChildId) -> Result<InstanceId, KernelError>;

    pub fn call_child(
        &mut self,
        target: ChildTarget,
        call: ChildCall,
    ) -> Result<ChildReply, KernelError>;

    pub fn handle_toy_call(
        &mut self,
        request: ToyRequest,
    ) -> Result<ToyReply, KernelError>;

    pub fn health(&self) -> KernelHealth;
}
```

The kernel API should not expose HTTP requests, CLI structs, Wasmtime types, JVM process handles, or SQLite rows.

## Domain types, not raw strings

The kernel should convert strings at the boundary into typed domain values:

```rust
pub struct ChildId(String);
pub struct InstanceId(String);
pub struct ToyId(String);
pub struct OperationId(String);
pub struct PandoId(String);
pub struct NodeId(String);

pub enum RuntimeKind {
    Process,
    Jvm,
    WasmComponent,
}
```

Reason: stringly code is acceptable at TOML/JSON/CLI boundaries, but the authority core should not confuse child names, runtime IDs, operation IDs, and toy IDs.

## Lifecycle as a state machine

Avoid this style:

```rust
loaded: bool,
active: bool,
healthy: bool,
failed: bool,
```

Prefer a lifecycle enum:

```rust
pub enum ChildLifecycle {
    Installed,
    Validated,
    Starting,
    Ready { instance: InstanceId },
    Degraded { reason: DegradeReason },
    Stopping,
    Stopped,
    Failed { error: ChildFailure },
}
```

Reason: Mother is runtime authority. Ambiguous state creates unsafe routing and confusing operations. Rust should force each state transition to be handled.

## Runtime and substrate adapters stay outside the kernel

The kernel should not know how to spawn JVMs, instantiate WASM components, run process harnesses, serve HTTP, persist SQLite rows, or open Iroh connections.

Adapter boundaries are justified where MCT already has external systems or multiple real substrate paths:

- WASM/WASI child runtime;
- process harness for protocol proof/testing;
- JVM bridge adapter for banking infrastructure;
- Iroh Mother-to-Mother substrate;
- local HTTP/JSON or UDS substrate if selected;
- observation storage backend;
- secrets backend.

The JVM path is not kernel internals. It is a first-class bridge/client path so existing banking JVM infrastructure can connect to the node while domain logic migrates toward WASM/WASI children. Clojure sits above MCT as an interface/orchestration layer, sequenced after the kernel is solid.

A small runtime trait may be justified because multiple child/harness implementations are real product needs:

```rust
pub trait ChildRuntime: Send + Sync {
    fn kind(&self) -> RuntimeKind;
    fn validate(&self, package: &ChildPackage) -> Result<(), RuntimeError>;
    fn spawn(&self, request: SpawnRequest) -> Result<Box<dyn ChildInstance>, RuntimeError>;
}

pub trait ChildInstance: Send {
    fn health(&mut self) -> Result<ChildHealth, RuntimeError>;
    fn call(&mut self, call: ChildCall) -> Result<ChildReply, RuntimeError>;
    fn stop(&mut self) -> Result<(), RuntimeError>;
}
```

Reason: this is not speculative abstraction when used for WASM child runtime plus process harness. For the JVM bridge and Iroh/HTTP substrates, use separate adapter traits only when their seams are clear; do not force everything into `ChildRuntime`.

## Grant engine is the kernel heart

MCT's promise is explicit authority. The grant engine is therefore central, not a plugin detail.

```rust
pub struct GrantEngine;

impl GrantEngine {
    pub fn authorize_toy_call(
        &self,
        child: ChildId,
        toy: ToyId,
        request: &ToyRequest,
    ) -> Result<AuthorizedToyCall, GrantDenied>;
}
```

Toy execution should require an `AuthorizedToyCall` value that only the kernel can produce.

Reason: if toy calls can bypass authorization, Mother is not actually the authority boundary.

## Errors should be typed at the kernel boundary

The daemon and CLI may use `anyhow`.

The kernel should expose a typed error enum:

```rust
#[non_exhaustive]
pub enum KernelError {
    UnknownChild(ChildId),
    InvalidManifest(ManifestError),
    GrantDenied(GrantDenied),
    RuntimeUnavailable(RuntimeKind),
    ChildNotReady(ChildId),
    Runtime(RuntimeError),
    Store(StoreError),
}
```

Reason: callers and tests need to distinguish unknown child, denied grant, runtime failure, and invalid package. These are not the same operationally.

## What is not in the kernel

The kernel should not contain:

- HTTP parsing;
- CLI clap structs;
- Wasmtime details;
- JVM process flags;
- SQLite SQL strings;
- Clojure SDK code;
- `scry`, `assay`, `oxidize`, scrape, or Belief graph logic;
- session markdown formatting;
- view rendering details.

These are adapters, product integrations, or presentation layers.

## Relationship to existing integrated code

The old integrated daemon demonstrates useful pieces but also shows what to avoid.

Reference surfaces:

- Child trait and runtime domain: `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/mother/src/runtime.rs`
- Child registry: `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/mother/src/registry.rs`
- Current all-in-one daemon state: `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/src/commands/mother/daemon.rs`
- Current broad API runtime trait: `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/mother/src/http_api.rs`
- Current WASM child runtime: `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/src/child/internal/`
- Current feature map: `layer/surface/patina-mct.org`

The clean kernel should preserve the good seams and reject the accidental coupling.

## Architecture overview alignment

The architecture overview draft (`/Users/nicabar/Downloads/architecture-overview.docx`) sharpens this Slate with these agreed decisions:

1. The MCT kernel must stay tight, small, and edge-ready.
2. Iroh is not merely future optional networking; it is the v0 Mother-to-Mother building system for node identity and node-to-node transport design.
3. The JVM path is a bridge into the WIT/WASI child/toy design so existing banking JVM infrastructure can connect now and migrate toward a WASM world over time. It is not kernel internals.
4. Clojure becomes the interface/orchestration layer above MCT, after the kernel is solid.
5. WASM/WIT/WASI is the design center: children are WASM/WASI components, and toys are WIT/WASI host capability contracts granted by Mother. The exact runtime/component model should be worked out as we build.
6. Observability is mandatory: capture every event, make the stream append-only/tamper-evident, and separate capture from retention/storage policy.
7. Buffer management, business rules, and domain modules stay outside the kernel in components.
8. Quack's HTTP lesson is important, but HTTP placement is still a design question. The lesson is to use a small semantic protocol over the substrate the world has optimized for a given boundary, while Iroh remains the Mother-to-Mother substrate.

## Quack-inspired protocol lesson

DuckDB Quack is useful inspiration because it makes a remote capability feel local without turning the database into a giant bespoke server. Its lesson for MCT is:

```text
familiar local interface
  -> optimized substrate for that boundary
  -> small typed semantic protocol
  -> remote capability feels local
```

For MCT, the design center is still WIT/WASI children and toys:

- children are WASM/WASI components;
- toys are WIT/WASI host capability contracts granted by Mother;
- the JVM bridge lets banking infrastructure enter this child/toy world without a rewrite;
- Iroh is the Mother-to-Mother building system;
- Clojure later becomes the interface/orchestration layer above MCT.

Quack's important lesson is not “everything should be HTTP.” It is that transport should be boring and optimized where possible, while the product semantics stay above the transport. HTTP may be the right bridge/local-client substrate because the world has optimized it deeply, especially for JVM and tooling boundaries. Iroh remains the v0 substrate for Mother-to-Mother communication.

The open design question is where HTTP belongs exactly, not whether Quack's protocol taste matters.

## Session decisions 2026-05-29: naming, Iroh, envelope, observations, storage

These decisions refine the open questions without closing the whole design.

### Crate name: working answer is `mct-kernel`, with a terminology check

Working answer: use `mct-kernel` for the first authority-core crate, unless the word `kernel` proves misleading before implementation.

Why this is plausible:

- Similar Rust/product naming often uses product prefix plus role: `patina-core`, `deno_core`, `temporal-sdk-core`, `wasmtime-wasi`, `tokio-util`, `opentelemetry-*`.
- `mct-kernel` names the product boundary, not only the Mother role.
- `mother-kernel` is clear but narrower than MCT.
- `mother-core` sounds like daemon plumbing rather than the trusted Child/Toy authority seam.

Terminology caveat: the user questioned `kernel`. In this design, `kernel` means the trusted, performance-sensitive decision core. It does not mean an OS kernel, a monolith, or a place to put every runtime detail. If the word remains too heavy, the fallback is likely `mct-core`, with an internal `kernel` module.

### Iroh v0: real substrate, not a skeleton

Revised answer: Iroh is not a tiny placeholder. MCT should build a production-shaped Iroh v0 early, while still deferring business cluster policy.

Reference repo available for study:

- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/TRANSPORTS.md`

Facts from the cached Iroh docs that matter for MCT:

- Iroh's model is dialing by public key / peer id.
- `Endpoint` is the core connection entrypoint.
- Connections use QUIC and can be direct, hole-punched, or relayed.
- Protocols are separated by ALPN and handled by protocol routers/handlers.
- The workspace includes `iroh`, `iroh-base`, `iroh-relay`, `iroh-dns`, and `iroh-dns-server`.
- Iroh emits structured tracing events under `iroh::_events::*` targets.
- Higher-level protocols such as `iroh-blobs`, `iroh-gossip`, and `iroh-docs` are referenced as composable protocols and should be reviewed separately before deciding artifact/event-sync design.

Near-complete Iroh v0 should include:

- persistent Mother node identity and key material;
- typed `NodeId`/peer identity in the MCT domain model;
- endpoint lifecycle owned by the daemon adapter, not by the kernel;
- relay/discovery configuration and local address reporting;
- peer address book and peer admission/trust decisions;
- MCT ALPN protocol name/version;
- direct request/reply over bidirectional streams;
- connection state, retry/backoff, and graceful close handling;
- structured adapter observations for connect, accept, send, receive, relay fallback, stream reset, and close;
- local multi-node integration tests, ideally covering direct and relay-shaped paths.

Still deferred:

- full private-cloud tenancy policy;
- multi-institution cluster governance;
- custom transport IDs;
- gossip mesh unless a concrete event fanout use case requires it;
- blob transfer until child packages/artifacts/observation chunks require it.

### Stable child call envelope: lock the semantic shape

Agreement: MCT needs one transport-independent semantic call envelope shared by process harnesses, JVM bridge, WASM components, local HTTP/UDS, and Iroh.

Locked v0 shape, at the semantic level:

```text
MctCallEnvelope v1
  call_id              unique operation/correlation id
  caller               user/interface/child/node identity making the request
  target               node + child/component/instance target
  operation            WIT/interface/function operation reference
  payload              codec + bytes; JSON is an edge codec, not the kernel's native truth
  presented_auth       optional remote proof/capability material from the caller boundary
  deadline             optional timeout/deadline
  idempotency_key      optional retry-safe operation key
  trace                trace/span/session/correlation metadata
```

Important authority rule: the caller may present identity or proof, but the caller does not grant itself authority. The kernel derives the required grants from registered child/toy policy, decides allow/deny, and records the decision.

Performance rule: parse strings and JSON at the transport edge. Inside the kernel, use typed IDs, typed operation references, and bytes/borrowed payloads where possible. Avoid making `serde_json::Value`, HTTP structs, Iroh structs, JVM handles, or Wasmtime values part of the kernel API.

### Kernel observations vs adapter observations: Temporal/Terraform alignment

The observation split should align with mature systems:

- Temporal: workflow history records durable state transitions such as workflow started, activity scheduled, activity completed/failed, timer fired. Workers perform effects and report results/failures back. MCT should similarly make authority/lifecycle/routing decisions durable while adapters report effect outcomes.
- Terraform: Terraform Core builds the graph/plan and decides provider RPCs; providers perform cloud-specific CRUD and return diagnostics. MCT should similarly let the kernel decide route/grant/lifecycle, while Iroh/JVM/WASM/process/toy adapters perform substrate-specific work and return diagnostics.

MCT kernel-guaranteed observations:

- child install accepted/denied;
- child activation accepted/denied;
- lifecycle transition requested/accepted/denied;
- child call accepted/denied/routed;
- toy grant allowed/denied;
- peer call accepted/denied at the authority boundary;
- unknown child, not-ready child, missing grant, invalid operation, and invalid caller failures.

Adapter observations:

- Iroh endpoint started/stopped, peer connected, relay fallback used, stream opened/reset/closed;
- process spawned/exited/signaled;
- JVM bridge connected/disconnected/timed out;
- WASM component instantiated/trapped;
- toy backend read/write/network/secret operation succeeded or failed;
- SQLite/file append failed, retried, or checkpointed.

Rule: every authority decision is a kernel observation. Every external effect or substrate failure is an adapter observation. The two are linked by `call_id`/trace ids.

### Storage boundary: concrete, narrow, fast first pass

The current integrated Mother already uses a concrete SQLite store, `MotherRuntimeStore`, but it mixes many concerns: sessions, beliefs, project identity, child registry, project runtime state, tasks, view buffers, and audit-ish data. Current child-call observability also writes metrics into `events.db` and keeps only a small in-memory typed-call history.

Clean MCT should keep the good part — concrete boring storage — and remove the coupling.

Working answer:

- Use concrete storage in the first pass; do not expose a generic `MotherKernel<S>` public API yet.
- Keep SQL and file layout private behind small modules.
- Keep hot routing/grant state in memory after bootstrap.
- Persist durable facts narrowly: node identity, peer address book, child registry/install state, grants, lifecycle snapshots, and append-only observations.
- Do not put Belief, session markdown, scrape/index tables, or view rendering tables in `mct-kernel`.
- Avoid a database write on every hot decision before routing; use an observation sink that can batch, append, or fail closed according to the event class.

The performance target is a small, typed, edge-ready authority core: parse at the edge, decide in memory, observe every decision, and keep persistence boring and explicit.

## Modularity gates before implementation

Before implementing any MCT slice, apply these gates:

1. Can the module's job be stated in one sentence?
2. Is its public API smaller than its private implementation?
3. Are stringly values parsed into typed domain values at the boundary?
4. Does the kernel API avoid HTTP, Iroh, JVM, Wasmtime, SQLite, and filesystem types?
5. Are external systems hidden behind adapters only where the seam is real?
6. Does every authority decision fail closed and emit an observation?
7. Can the slice be committed/reverted as one coherent step?
8. Does the slice avoid solving cluster policy, JVM migration, WASM runtime depth, and Clojure UX all at once?

These gates are how MCT adapts without boiling the ocean.

### Gate examples

These examples make the gates concrete. They are not implementation commitments; they are review tests for future slices.

#### 1. One-sentence module job

Passes:

- `identity`: parse and represent Mother, node, child, toy, and operation identities as typed values.
- `grant`: decide whether a child may use a toy for one specific operation.

Fails:

- `runtime`: manage children, launch JVMs, route Iroh messages, serve HTTP, store logs, and handle config.
- `bridge`: make JVM, Clojure, HTTP, Iroh, WASM, and banking migration all work together.

#### 2. Stable public API

Passes:

- `pub struct ChildId(String);` because child identity is a stable kernel concept.
- `pub fn authorize_toy_call(...) -> Result<AuthorizedToyCall, GrantDenied>` because authorization is a stable authority operation.

Fails:

- `pub struct KernelState { pub children: HashMap<...> }` because it leaks storage internals.
- `pub fn spawn_wasmtime_component(store: wasmtime::Store<...>)` because the kernel API would expose Wasmtime internals.

#### 3. Private internals

Passes:

- `internal::registry::HashMap<ChildId, ChildRecord>` stays private behind registry methods.
- `internal::grants::CompiledGrantRule` stays private behind the grant evaluator.

Fails:

- Making `internal::registry::ChildRecord` public because one CLI command needs one field.
- Returning SQLite row structs from kernel APIs.

#### 4. Real adapter seams

Passes:

- `ChildRuntime` for WASM/WASI runtime plus process harness/test runtime.
- `ObservationStore` once there is an in-memory test store plus append-only/Merkle/file-backed store.

Fails:

- `GrantEngineBackend` when there is only one pure in-memory evaluator.
- `ClojureOrchestrationRuntime` before the Clojure seam is known.

#### 5. Typed domain values

Passes:

- Parse `"watcher"` at CLI/API boundaries into `ChildId`.
- Parse `"fs.read"` at manifest/config boundaries into `ToyId` or `CapabilityId`.

Fails:

- `fn call_child(child: String, toy: String, payload: Vec<u8>)` in the kernel API.
- Lifecycle stored as `"ready"`, `"failed"`, or `"starting"` strings instead of an enum.

#### 6. State machines over boolean soup

Passes:

- `Installed -> Validated -> Starting -> Ready -> Stopped` as an explicit lifecycle.
- `GrantDecision::Allowed(AuthorizedToyCall)` / `GrantDecision::Denied(GrantDenied)` as a typed decision.

Fails:

- `loaded: bool`, `running: bool`, `healthy: bool`, `failed: bool` on one record.
- `authorized: bool`, `needs_auth: bool`, `auth_failed: bool` without one source of truth.

#### 7. Observations for authority decisions

Passes:

- Unknown child call emits `child.call.denied` with `reason = unknown_child`.
- Toy grant success emits `toy.call.authorized` with child, toy, operation, and observation IDs.

Fails:

- Grant denial silently returns `false`.
- Runtime spawn fails with an exception but no structured observation.

#### 8. Independent slices

Passes:

- `identity: add typed MotherId/ChildId/ToyId` as one commit.
- `lifecycle: model child states explicitly` as one commit with tests/examples.

Fails:

- One commit adding Iroh transport, JVM bridge, WASM runtime, Clojure API, and observation storage.
- `misc mct work` touching protocol, runtime, docs, config, and storage without one coherent review boundary.

The point of these examples is not to make MCT small in ambition. It is to build large goals as replaceable, typed, reviewable slices.

## Initial non-goals

The first kernel design should not implement:

- full multi-institution cluster policy beyond a production-shaped Iroh v0 transport/identity path;
- full pando component composition;
- view buffer rendering;
- Patina Belief app integration;
- SDK authoring surface;
- full WASM host toy implementation.

Those can attach once the kernel seam is clear.

## Open questions

1. Working name is `mct-kernel`; final terminology question is whether `kernel` is the right public word or whether `mct-core` should be used with an internal kernel module.
2. Storage first pass is concrete and private; remaining question is the exact narrow schema and replay/snapshot strategy.
3. Is `ChildRuntime` the right trait shape, or should runtime and instance supervision be split further?
4. The semantic call envelope shape is locked at v1; remaining question is exact Rust type names, codec choices, and WIT operation encoding.
5. Kernel vs adapter observation split is aligned with Temporal/Terraform; remaining question is the exact event schema and retention policy.
6. How much of session/launcher state belongs in kernel versus launcher service?
7. Iroh v0 should be production-shaped, not skeletal; remaining question is exact inclusion of blobs/gossip/docs versus direct streams and relay/discovery only.
8. What is the exact kernel security model for node auth, component auth, and WIT-interface exposure?
9. What retention/storage policy follows the rule “capture everything, store selectively”?
10. Which buffer component model should be explored first: CRDT, OT, or custom?

## Current design answer

The Mother kernel should be:

> a small typed Rust authority core for WASM child identity, lifecycle, routing, WIT/WASI toy grants, a production-shaped Iroh Mother-to-Mother substrate, JVM bridge ingress, and observations.

Everything else is outside the kernel until a narrow, typed boundary requires it.

---
id: adapter-pattern
layer: core
status: active
created: 2026-05-31
revised: 2026-05-31
tags: [architecture, mct, mother, adapters, authority, rust]
references: [dependable-rust, unix-philosophy, mct-build-boundaries, mother-kernel-decides-adapters-perform, iroh-provides-connectivity-not-authority]
---

# MCT Adapter Pattern

**Purpose:** Keep Mother authority in the kernel and push external effects to adapters. Adapters perform effects; they do not create authority.

---

## Core Principle

Patina MCT is built around one boundary:

```text
Mother kernel decides.
Adapters perform.
MctObservation proves what happened.
```

An adapter is code that crosses from MCT domain logic into an external system or runtime:

- Iroh/noq networking
- WASM/WASI component runtime
- process or JVM execution
- filesystem/database persistence
- secret storage
- telemetry export
- local CLI/control socket

Adapters translate between external details and MCT domain records. They must not invent permission, rewrite policy, or leak implementation types into the kernel.

## Adapter vs Kernel

### Kernel/domain logic

These are **not** adapters:

- `MctCall`
- `MctResult`
- `MctObservation`
- `MctPeerBinding`
- `MctHelloAdmissionEvaluation`
- `MctCallProtocolEvaluation`
- `ToyGrant`
- route authority filtering
- child assignment approval
- Vision/data policy checks

They are MCT records and decisions. They belong in `mct-kernel` or closely allied domain crates.

### Adapter logic

These are adapters:

| Adapter | External thing | MCT boundary |
|---------|----------------|--------------|
| Iroh adapter | Iroh endpoint, streams, hooks, relay/discovery, noq path facts | `mct/hello/0`, `mct/call/0`, peer observations |
| WASM adapter | Wasmtime/WASI/component runtime | WIT-shaped child invocation + authorized toy host calls |
| Observation sink | JSONL/fsync/backpressure | append and validate `MctObservation` facts |
| Toy backend | filesystem, git, messaging, state, network, secrets | `AuthorizedToyCall`-guarded effect |
| Telemetry exporter | OTel/Prometheus/qlog dashboards | projection from local ledger |
| Control adapter | CLI/UDS/HTTP local control | local request translated to `MctCall` or operator action |

## The MCT Rules

### 1. No adapter grants authority

An adapter can reject early for safety, but allow decisions come from the kernel.

```text
Iroh hook may reject unknown endpoint early.
Kernel decides whether EndpointId + binding + Vision + ALPN is admitted.
```

### 2. No raw substrate types in kernel API

The kernel should not expose Iroh streams, Wasmtime stores, SQLite connections, qlog structs, or OS handles. Use MCT domain records at the boundary.

```rust
// Bad: kernel API depends on Iroh details.
pub fn admit(conn: &iroh::endpoint::ConnectionInfo) -> Result<Admission>;

// Good: adapter extracts facts; kernel evaluates domain records.
pub fn evaluate_hello(
    request: &MctHelloRequest,
    bindings: &[MctPeerBinding],
    policy: &HelloPolicy,
    context: HelloEvaluationContext,
) -> MctHelloAdmissionEvaluation;
```

### 3. Traits wait for the second implementation

Do not create traits for imagined future backends. Start concrete. Introduce traits when a second real implementation or a proven test seam exists.

MCT examples:

- Keep one concrete append-only JSONL observation ledger until another sink is real.
- Keep one concrete Iroh adapter until another transport is real.
- Keep WASM/process runtime composition concrete inside the daemon until a split is earned.
- Add traits only after the seam is proven.

### 4. Adapters emit observations

Every adapter effect that matters must produce or return an observation fact:

- Iroh connect/handshake/stream/reset/path fact
- hello received/responded
- peer call received/malformed/replied
- WASM trap/timeout
- process exit
- storage append/backpressure
- toy backend failure

Adapter errors are not invisible logs; they become `MctObservation` facts or health degradation.

### 5. Children use Toys, not adapter handles

WASM/WASI/WIT children do not receive raw Iroh endpoints, raw database handles, unrestricted filesystem access, or host secrets. They receive scoped WIT/Toy access only through explicit `ToyGrant` evaluation and kernel-minted `AuthorizedToyCall` capabilities.

## First Build Application

For the first vertical slice:

```text
mct-kernel        → concrete domain records and evaluations
mct-observation   → concrete append-only local ledger
mct-iroh          → concrete Mother-owned Iroh endpoint adapter
mct-daemon        → composition and local lifecycle
```

No generalized plugin framework is needed yet. Build the vertical seam honestly first:

```text
Endpoint facts → MctPeerBinding check → mct/hello/0 → mct/call/0 → MctObservation
```

## Common Mistakes

### Treating Iroh EndpointId as MCT authority

Wrong:

```text
EndpointId authenticated → allow call
```

Right:

```text
EndpointId authenticated → find active MctPeerBinding → hello admission → call authority filter
```

### Letting adapter convenience leak upward

Wrong:

```text
Iroh stream handler directly invokes a child.
```

Right:

```text
Iroh stream handler constructs MctCallProtocolRequest.
Kernel evaluates the call protocol request and child authority.
Runtime adapter invokes child only with a kernel-minted `AuthorizedChildInvocation`.
```

### Abstracting before the seam exists

Wrong:

```rust
trait PeerTransport { ... } // only Iroh exists
```

Right:

```rust
mod iroh; // concrete adapter first
```

## References

- [Dependable Rust](./dependable-rust.md)
- [Unix Philosophy](./unix-philosophy.md)
- [MCT Build Boundaries](./mct-build-boundaries.md)
- [mother-kernel-decides-adapters-perform](../surface/epistemic/beliefs/mother-kernel-decides-adapters-perform.md)
- [iroh-provides-connectivity-not-authority](../surface/epistemic/beliefs/iroh-provides-connectivity-not-authority.md)

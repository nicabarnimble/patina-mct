---
id: adapter-pattern
layer: core
status: active
created: 2025-08-02
revised: 2026-04-03
tags: [architecture, patterns, traits, external-systems, mct]
references: [dependable-rust, unix-philosophy, mother-kernel-decides-adapters-perform, iroh-provides-connectivity-not-authority]
---

# Adapter Pattern

**Purpose:** Define trait boundaries at external system edges. Prove the boundary with real implementations — don't abstract speculatively.

---

## Core Principle

When MCT touches an external system (Iroh endpoint, WASM/WASI runtime, storage engine, secret backend, telemetry exporter, CLI/control socket), put a narrow boundary in front of it. The boundary is the contract. Implementations are black boxes. Only introduce a Rust trait when you have 2+ real implementations or a proven test seam — a trait with one implementation is ceremony, not architecture.

This is the Gjengset principle applied to system boundaries: honest signatures, type integrity at the seam, and proof before abstraction.

## Adapter vs Strategy

Not every trait boundary is an adapter. The distinction matters:

- **Adapter**: bridges an external system into MCT's domain. The implementation wraps external or runtime-specific code. Examples: an Iroh endpoint/protocol adapter, a Wasmtime component adapter, a storage sink, a secrets backend, or an OpenTelemetry exporter.
- **Strategy**: selects among internal algorithms. No external system involved. Examples: route ranking among already-authorized candidates, safe projection filtering, retry budget selection, or thought acceptance ordering.

The distinction matters for MCT because Mother kernel authority must remain internal domain logic. Iroh, WASM, storage, and telemetry are adapters; ToyGrant evaluation and peer admission are not.

Use adapter when crossing a system boundary. Use strategy when choosing among internal approaches. Both use traits; only adapters isolate external dependencies.

## When to Use

Apply this pattern when:
- 2+ implementations exist today (not "might exist someday")
- An external system may change independently of MCT (Iroh/noq versions, Wasmtime APIs, storage engines, metrics exporters)
- You need to swap implementations without changing calling code

**MCT adapter boundaries to apply deliberately:**

| Boundary | Why it is external | First implementation |
|----------|--------------------|----------------------|
| Iroh endpoint/protocols | Iroh/noq changes independently of MCT | Mother-owned Iroh adapter for `mct/hello/0` and `mct/call/0` |
| WASM/WASI runtime | Wasmtime/component runtime changes independently | Wasmtime adapter after the peer spine works |
| Observation sink | Storage durability/backpressure can vary | Append-only JSONL or SQLite sink first |
| Secret backend | Secret storage is host-specific | Local development backend first |
| Telemetry export | OTel/Prometheus/qlog are projections | Export adapter after local ledger truth exists |

Do not turn core concepts such as `MctCall`, `MctPeerBinding`, `ToyGrant`, or `MctObservation` into adapter traits. Those are domain records.

## When NOT to Use

- Only one implementation exists and no second is planned — use a module, not a trait
- Internal code talking to internal code — modules and function calls, not trait objects
- The "abstraction" just forwards calls — wrapper tax with no benefit
- You're guessing where the seam is — wait until the second implementation proves it

## How to Apply

### 1. Honest Signatures

The trait should declare exactly what it needs. No smuggling dependencies through config bags.

```rust
// ❌ Bad: hides what the function actually depends on
pub fn admit_peer(config: &AppConfig, endpoint: EndpointId) -> Result<PeerAdmission> {
    let store = config.peer_binding_store();  // hidden dependency
    store.evaluate(endpoint, config.current_policy_revision())
}

// ✅ Good: dependency is visible in the signature
pub fn admit_peer(
    bindings: &PeerBindings,
    endpoint: EndpointId,
    policy_revision: PolicyRevision,
) -> Result<PeerAdmission> {
    bindings.evaluate(endpoint, policy_revision)
}
```

### 2. Prove the Boundary

A trait with one implementation is a hypothesis. Two implementations prove the seam is in the right place.

```rust
// Proven boundary candidate: ObservationSink
// - JSONL sink for the first local ledger
// - SQLite sink only when a real second backend is implemented

// Unproven: don't create a trait
// - If you only have one local JSONL observation sink,
//   use it directly. Add the trait when the second backend arrives.
```

### 3. Keep Traits Minimal

3-7 methods is typical. If a trait grows beyond that, it's doing too much — split it or push methods into the implementation.

```rust
// ✅ Good: focused trait when the second sink exists
pub trait ObservationSink: Send + Sync {
    fn append(&self, observation: &MctObservation) -> Result<AppendAck>;
    fn flush(&self) -> Result<()>;
}
// File formats, batching, fsync, and database details stay in the implementation.
```

### 4. Domain Types at the Boundary

The trait uses Patina's domain types. Implementation-specific types stay behind the boundary.

```rust
// ❌ Bad: leaks implementation type
pub trait Backend {
    fn query(&self) -> rusqlite::Rows;  // caller now depends on rusqlite
}

// ✅ Good: domain type at the boundary
pub trait ObservationSink {
    fn append(&self, observation: &MctObservation) -> Result<AppendAck>;  // MCT domain types
}
```

### 5. Combine with Dependable-Rust

Each adapter implementation is a black-box module:

```
crates/mct-iroh/src/
├── lib.rs          # public adapter contract and curated exports
└── internal/       # Iroh endpoint, stream, relay, and hook details
```

The trait lives in the parent module. Implementations live in their own subdirectories. Nothing in the implementation leaks into the trait.

## Testing

Prefer integration tests with real implementations. Adapter boundaries exist to isolate external systems, not to invite mocks. Test the real thing whenever possible — local Iroh endpoints, local observation files, local WASM fixtures, local relay processes when feasible.

Mocks are a last resort for when the real system is genuinely unavailable in CI (external APIs requiring credentials, third-party services with rate limits). Even then, prefer a lightweight real implementation (in-memory database, local test server) over a mock that fakes behavior.

## Common Mistakes

**1. Abstracting with one implementation**
```rust
// ❌ Bad: trait exists "just in case"
trait CacheBackend { fn get(&self, key: &str) -> Option<String>; }
struct RedisCacheBackend;  // the only implementation
// Just use Redis directly. Add the trait when you need a second backend.
```

**2. Leaking implementation types**
```rust
// ❌ Bad: trait exposes vendor type
trait Storage { fn connection(&self) -> &duckdb::Connection; }
// ✅ Good: trait exposes domain operations
trait Storage { fn store_fact(&self, fact: &Fact) -> Result<()>; }
```

**3. Oversized traits**
```rust
// ❌ Bad: 15 methods — some callers only need 2
trait FullService {
    fn query(&self, ...) -> Result<...>;
    fn index(&self, ...) -> Result<()>;
    fn delete(&self, ...) -> Result<()>;
    fn migrate(&self, ...) -> Result<()>;
    // ... 11 more
}
// ✅ Good: split into focused traits
trait Queryable { fn query(&self, ...) -> Result<...>; }
trait Indexable { fn index(&self, ...) -> Result<()>; }
```

## References

- [Dependable Rust](./dependable-rust.md) — How to structure each implementation as a black box
- [Unix Philosophy](./unix-philosophy.md) — Each implementation does one thing
- [mother-kernel-decides-adapters-perform](../surface/epistemic/beliefs/mother-kernel-decides-adapters-perform.md) — Kernel decides; adapters perform effects
- [iroh-provides-connectivity-not-authority](../surface/epistemic/beliefs/iroh-provides-connectivity-not-authority.md) — Iroh is substrate, MCT owns authority

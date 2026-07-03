---
id: dependable-rust
layer: core
status: active
created: 2026-05-31
revised: 2026-05-31
tags: [architecture, rust, mct, modules, black-box]
references: [unix-philosophy, adapter-pattern, spec-driven-design, mct-build-boundaries]
---

# Dependable Rust for MCT

**Purpose:** Build MCT as small Rust black boxes with stable public domain interfaces and private implementation detail.

---

## Core Principle

MCT Rust code should make the authority boundary obvious in the type system:

```text
public API:  MCT domain records and decisions
private API: implementation helpers, substrate/runtime details
```

A caller should be able to understand what a module does by reading its public `mod.rs`/`lib.rs`. The module internals may change, but the public domain contract remains stable.

For MCT, public signatures should use types such as:

- `MctCall`
- `MctResult`
- `MctObservation`
- `MctPeerBinding`
- `MctHelloRequest`
- `MctCallProtocolRequest`
- `ToyGrant`
- `RouteDecision`

Public kernel APIs should not expose:

- Iroh stream/connection types
- Wasmtime store/linker/component internals
- SQLite/rusqlite handles
- filesystem writer internals
- OTel/qlog exporter structs

## Crate Shape

The first build should be intentionally small:

```text
crates/mct-kernel/       domain records + authority evaluation
crates/mct-observation/  append-only local observation ledger
crates/mct-iroh/         Mother-owned Iroh endpoint + MCT ALPN protocols
crates/mct-daemon/       process lifecycle and composition
```

Later crates may split out WASM, storage, CLI, or Toy backends when the seams are proven.

## Module Shape

Use black-box modules:

```text
module/
├── mod.rs          # public interface: docs, domain types, curated exports
└── internal.rs     # private implementation detail
```

Example:

```text
crates/mct-kernel/src/peer/
├── mod.rs          # MctPeerBinding, MctHelloRequest, evaluate_hello
└── internal.rs     # matching, expiry checks, reason construction
```

Example:

```text
crates/mct-iroh/src/
├── endpoint.rs          # endpoint lifecycle, configuration, and snapshots
├── identity.rs          # node secret key loading/creation and hex codecs
├── serve.rs             # MCT ALPN serving and call-handler dispatch
└── serve/internal.rs    # stream framing, address helpers, and timeouts
```

## Public Interface Rules

1. Public functions should say what authority facts they need.
2. Public functions should return typed decisions or observations, not strings.
3. `internal` must not be public.
4. `internal::` types must not appear in public signatures.
5. Prefer one small `Error` enum per module boundary.
6. Add doctests/examples only when they demonstrate stable usage, not incidental setup.

## MCT Examples

### Good: explicit authority inputs

```rust
pub fn evaluate_call_protocol(
    request: &MctCallProtocolRequest,
    hello: &MctHelloAdmissionEvaluation,
    ids: CallEvaluationIds,
) -> MctCallProtocolEvaluation;
```

### Bad: hidden authority through a context bag

```rust
pub fn evaluate_call_protocol(ctx: &DaemonContext, call: &MctCall) -> bool;
```

The second signature hides the hello admission, call envelope, and minted IDs. That makes review harder and authority easier to bypass.

### Good: adapter extracts facts

```rust
// mct-iroh
let presentation = IrohConnectionPresentation { /* adapter facts */ };
let request = MctHelloRequest { /* peer claims */ };
let decision = evaluate_hello(&request, bindings, policy, context);
```

### Bad: kernel imports substrate

```rust
// mct-kernel
use iroh::endpoint::ConnectionInfo;
```

## Testing Strategy

Test at three levels:

1. **Kernel unit tests** — pure domain decisions:
   - EndpointId alone is insufficient.
   - hello without active binding is denied.
   - hello admission does not pre-authorize calls.
   - mct/call still runs authority checks.

2. **Adapter integration tests** — real local effects:
   - local append-only ledger writes observations;
   - local Iroh endpoints complete `mct/hello/0`;
   - denied peer path records observations.

3. **Daemon smoke tests** — composed vertical slice:
   - two local Mothers;
   - bound peer admitted;
   - remote echo call returns caller-safe result;
   - trace can be reconstructed from observations.

## The "Do X" Test

Every module should do one sentence-worthy job:

Good:

- "Evaluate peer binding admission."
- "Append observations to the local ledger."
- "Run `mct/hello/0` over an Iroh stream."
- "Translate WIT child invocation into a runtime adapter call."

Too vague:

- "Manage peers."
- "Handle networking."
- "Run Mother."
- "Do storage."

If the sentence is vague, split the module or keep it as explicit glue rather than pretending it is a stable abstraction.

## References

- [Unix Philosophy](./unix-philosophy.md)
- [Adapter Pattern](./adapter-pattern.md)
- [Spec-Driven Design](./spec-driven-design.md)
- [MCT Build Boundaries](./mct-build-boundaries.md)
- [MCT product map](../allium/mct-product-map.allium)

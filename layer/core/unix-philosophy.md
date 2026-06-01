---
id: unix-philosophy
layer: core
status: active
created: 2026-05-31
revised: 2026-05-31
tags: [architecture, mct, decomposition, crates, core-principle]
references: [dependable-rust, adapter-pattern, mct-build-boundaries]
---

# Unix Philosophy for MCT

**Purpose:** Build Mother/Child/Toy as composed, single-purpose pieces instead of one magical Mother blob.

---

## Core Principle

MCT follows Unix philosophy:

```text
one piece, one job, explicit composition
```

The system is powerful because the pieces compose:

```text
peer facts → authority decision → observation → adapter effect → observation
```

Do not hide multiple responsibilities behind names like "manager", "runtime", or "service" unless the module is explicit glue.

## MCT Decomposition

### Design-time layer

```text
layer/core/              build laws and project patterns
layer/allium/            product/domain behavior
layer/slate/work/        executable work and proof plans
layer/surface/           evidence, beliefs, product notes
layer/sessions/          discussion and git-range context
```

### Runtime layer

```text
crates/mct-kernel/       authority records and decisions
crates/mct-observation/  local-first append-only observation ledger
crates/mct-iroh/         Iroh endpoint and ALPN protocol adapter
crates/mct-daemon/       process lifecycle and composition
```

Later, if pressure proves the split:

```text
crates/mct-wasm/         WASM/WASI/WIT child runtime adapter
crates/mct-toys/         toy backends and grant enforcement helpers
crates/mct-cli/          user-facing commands
```

## Jobs That Must Stay Separate

### Authority vs effect

```text
mct-kernel decides whether a peer/call/toy/child/data move is allowed.
adapters perform the network/runtime/storage/secret effect.
```

### Observation vs logging

```text
MctObservation is durable truth.
logs/metrics/qlog/OTel are projections or diagnostics.
```

### Connectivity vs authority

```text
Iroh connects endpoints.
MCT admits peers and authorizes protocol effects.
```

### Child runtime vs Toy authority

```text
WASM/WIT child exports define callable shape.
ToyGrant decides which host capabilities are available.
```

## Vertical Slice First

The first build should prove this path:

```text
Mother starts
  → local observation ledger opens
  → peer binding exists
  → Iroh endpoint receives mct/hello/0
  → kernel admits or denies
  → admitted peer sends mct/call/0
  → kernel constructs MctCall
  → fake local handler returns MctResult
  → observations reconstruct the trace
```

That slice is enough to prove the architecture without prematurely building the whole Toy catalog, WASM runtime, thought mesh, or federation layer.

## Good MCT Module Names

Good names say the job:

- `peer_binding`
- `hello_protocol`
- `call_protocol`
- `observation_ledger`
- `toy_grants`
- `route_authority`
- `child_assignment`

Suspicious names hide scope:

- `manager`
- `runtime` with no qualifier
- `service`
- `engine`
- `orchestrator`

Glue may exist, but name it as glue: `daemon`, `app`, `command`, or `composition`.

## Composition Example

Bad:

```rust
pub fn handle_iroh_stream(stream: IrohStream) {
    // parse hello
    // check database
    // update policy
    // invoke child
    // write logs
    // return result
}
```

Good:

```rust
pub fn handle_iroh_stream(stream: IrohStream) -> Result<()> {
    let presentation = adapter.read_presentation(&stream)?;
    let hello = adapter.read_hello(&stream)?;
    let decision = kernel.evaluate_hello(presentation, hello, snapshots)?;
    observations.append(decision.observation())?;
    adapter.write_hello_response(&stream, decision.safe_response())?;
    Ok(())
}
```

Each part has one job. The function composes them.

## References

- [Dependable Rust](./dependable-rust.md)
- [Adapter Pattern](./adapter-pattern.md)
- [MCT Build Boundaries](./mct-build-boundaries.md)
- [MCT product map](../allium/mct-product-map.allium)

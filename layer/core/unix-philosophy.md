---
id: unix-philosophy
layer: core
status: active
created: 2025-08-02
tags: [architecture, philosophy, decomposition, core-principle]
references: [dependable-rust, adapter-pattern]
---

# Unix Philosophy

**Purpose:** Decompose complex systems into simple, single-purpose tools that do one thing well and compose cleanly.

---

## Core Principle

Patina MCT follows Unix philosophy: **one tool, one job, done well**. Each component has a single, clear responsibility. Complex functionality emerges from composition of simple tools, not from monolithic systems.

## When to Use

Apply this principle when:
- Designing new CLI commands
- Extracting functionality from monolithic code
- Planning module boundaries
- Deciding what belongs in a component

## How to Apply

### 1. Single Responsibility Per Component

Each MCT component has one clear job:

```
layer/allium/           → Product/domain specification
layer/slate/work/       → Build work items and proof plans
crates/mct-kernel/      → Domain records and authority decisions
crates/mct-observation/ → Append-only observations and projections
crates/mct-iroh/        → Iroh endpoint and ALPN protocol adapter
crates/mct-daemon/      → Process orchestration and local control
```

### 2. Decomposition Strategy

When facing a complex system:

1. **Identify core responsibilities** - What distinct jobs need doing?
2. **Create focused modules** - One module per responsibility
3. **Apply dependable-rust** - Black-box each module
4. **Compose functionality** - Combine modules to create features

**Example:** MCT vertical-slice decomposition

```
Monolithic Mother →  mct-kernel        (authority/domain decisions)
                     mct-observation   (append-only evidence)
                     mct-iroh          (Iroh endpoint/protocol effects)
                     mct-daemon        (lifecycle/local control)
```

### 3. Tools vs Systems

**Tools (build these):**
- Single primary operation
- Transform input → output predictably
- Don't maintain complex state
- Context-independent behavior

**Systems (decompose into tools):**
- Coordinate multiple operations
- Maintain complex state
- Depend on context/environment
- Require cross-interaction mental model

### 4. Composition Over Monolith

```rust
// ❌ Bad: monolithic command doing everything
pub fn init_project(path: &Path) -> Result<()> {
    // 500 lines: detect env, copy templates, init git,
    // configure adapters, generate docs...
}

// ✅ Good: composed from focused tools
pub fn init_project(path: &Path) -> Result<()> {
    let env = environment::detect()?;
    let templates = templates::load(&env)?;
    git::init(path)?;
    adapters::configure(path, &env)?;
    Ok(())
}
```

Each function is a tool doing one thing. The command coordinates them.

## Manifestation in MCT

This philosophy should appear throughout:

1. **Small kernel** - Authority/domain decisions stay focused and typed.
2. **Adapters at edges** - Iroh, WASM, storage, secrets, and telemetry perform effects outside the kernel.
3. **Typed records** - Calls, results, peer bindings, grants, and observations compose through explicit IDs.
4. **No feature creep** - New peer behaviours become explicit ALPN protocols or toys, not hidden flags in one giant daemon.

## Common Mistakes

**1. Building systems when you need tools**
```rust
// ❌ Bad: "workspace manager" (what does it manage?)
struct WorkspaceManager { /* everything */ }

// ✅ Good: specific tools
fn create_workspace(...) -> Result<Workspace>
fn list_workspaces(...) -> Result<Vec<Workspace>>
fn execute_in_workspace(...) -> Result<Output>
```

**2. Adding flags instead of commands**
```bash
# ❌ Bad: flag soup
patina init --with-git --llm=claude --env=docker --copy-templates

# ✅ Good: separate commands
patina init              # minimal setup
patina git init          # if you want git
patina template apply    # if you want templates
```

**3. Tight coupling between components**
```rust
// ❌ Bad: Iroh adapter knows about ledger internals
impl IrohAdapter {
    fn record_peer_call(&self) {
        self.ledger.internal.file.write_all(...);  // ❌
    }
}

// ✅ Good: use public interface only
impl IrohAdapter {
    fn record_peer_call(&self, observation: MctObservation) {
        self.observations.append(&observation)?;  // ✅
    }
}
```

## Benefits

When you follow Unix philosophy:
- ✅ Easy to test individual components
- ✅ Clear mental model for users
- ✅ Natural composition of functionality
- ✅ Predictable behavior
- ✅ Replace components without breaking others

## References

- [Dependable Rust](./dependable-rust.md) - How to structure each module as a black box
- [Adapter Pattern](./adapter-pattern.md) - Tool pattern for external system bridges
- [MCT product map](../allium/mct-product-map.allium) - Current decomposition and authority anchors

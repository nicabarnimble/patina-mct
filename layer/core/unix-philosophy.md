---
id: unix-philosophy
layer: core
status: active
created: 2026-05-31
revised: 2026-10-19
tags: [architecture, mct, decomposition, crates, core-principle]
references: [dependable-rust, adapter-pattern, mct-build-boundaries]
---

# Unix Philosophy for MCT

Compose pieces with explicit responsibilities:

```text
peer facts → authority decision → observation → adapter effect → observation
```

Keep authority evaluation separate from effects and connectivity. Composition
code may coordinate several pieces, but must not become a second source of
permission.

Split modules or crates when responsibility, testability or measured behavior
justifies it—not merely for a speculative abstraction or to satisfy a naming
convention.

## References

- [Adapter Pattern](./adapter-pattern.md)
- [Dependable Rust](./dependable-rust.md)

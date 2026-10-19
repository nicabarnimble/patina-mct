---
id: dependable-rust
layer: core
status: active
created: 2026-05-31
revised: 2026-10-19
tags: [architecture, rust, mct, modules, black-box]
references: [unix-philosophy, adapter-pattern, spec-driven-design, mct-build-boundaries]
---

# Dependable Rust for MCT

Make authority boundaries obvious in types and public interfaces.

- Public kernel APIs use MCT domain records, not Iroh connections, Wasmtime
  internals, SQLite handles or filesystem writers.
- Functions expose the authority facts they require. Avoid context bags that
  mix caller claims, current policy and unrelated implementation state.
- Return typed decisions and errors rather than ambiguous booleans or strings.
- Keep implementation details private and public exports deliberate. File layout
  should serve that boundary, not dictate it.
- Execution capabilities are minted by evaluators, not deserialized from caller
  input or constructed from copied expectations.

Test pure kernel decisions, real adapter effects, and composed behavior across
Mothers. Include denial, stale-authority, crash and restart paths—not only a
successful call.

## References

- [Reliability Doctrine](./reliability-doctrine.md)
- [Adapter Pattern](./adapter-pattern.md)
- [MCT Build Boundaries](./mct-build-boundaries.md)

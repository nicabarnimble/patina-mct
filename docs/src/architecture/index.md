# Architecture

The workspace separates pure authority decisions from effectful adapters:

| Crate | Responsibility |
| --- | --- |
| `mct-kernel` | Typed authority records and pure decisions |
| `mct-observation` | Append-only, hash-chained observation ledger |
| `mct-iroh` | Mother-owned Iroh endpoint and peer protocols |
| `mct-daemon` | Configuration, persistence, runtimes, adapters, control plane, and CLI |

The kernel decides. Other crates gather facts or perform effects that the
kernel has authorized. Wasmtime, Iroh, SQLite, and filesystem implementation
details do not enter kernel authority APIs.

## Authority before optimization

Routing has two phases:

1. Pure authority filtering removes inadmissible candidates.
2. Environment planning ranks only the remaining candidates.

Optimization can never grant authority. Authority is revalidated at the
effect boundary because stale authority is a security failure while stale
optimization is only a performance miss.

## Local execution boundary

Current local execution plans carry a WASM-only runtime type. Persisted legacy
`process` and `jvm_child` values remain decodeable for historical inspection,
but checked conversion and kernel authority evaluation reject them before
candidate execution.

## Knowledge boundaries

- Allium defines semantic law.
- Rust implements it.
- `layer/` retains internal doctrine, specifications, evidence, and sessions.
- This book explains supported product behavior.
- [Provenance](../provenance/index.md) connects explanations to public evidence.

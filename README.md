# Patina MCT

Patina MCT is a clean Mother/Child/Toy runtime for Patina.

MCT is a local-first authority runtime: a Mother owns local identity, child approval, toy grants, routing decisions, runtime execution, and observations. Children are approved components or processes. Toys are host capabilities such as logging, metrics, git, filesystem preopens, or future secrets/network/storage adapters. The kernel decides authority; adapters perform effects.

This repository is intentionally standalone. Existing Patina Mother is useful prior art, but it is not the ontology for this codebase.

## Core invariants

- **Authority before effects.** Child calls, toy calls, peer admission, and filesystem access are authorized before adapter effects run.
- **Kernel decides; adapters perform.** `mct-kernel` exposes typed domain records and decisions. Wasmtime, WASI, SQLite, git, process, HTTP, and Iroh details stay outside the kernel.
- **Observations are runtime truth.** Important decisions and adapter effects produce `MctObservation` facts; logs and metrics are projections.
- **No ambient child host power.** Children do not get raw filesystem roots, raw Iroh endpoints, raw secrets, raw process handles, or raw database handles by default.
- **No generic WIT host stubs.** WIT imports require concrete configured adapters or fail closed before instantiation.
- **Verified packages before approval.** MCT will not persist approved/active child config unless the child manifest and wasm artifact hashes are verified.
- **Optimization cannot grant authority.** Routing may rank only candidates that already passed authority checks.

## Workspace

| Crate | Role |
| --- | --- |
| `mct-kernel` | Authority domain types and pure decisions for calls, children, peers, routes, toys, and observations. |
| `mct-observation` | Append-only JSONL observation ledger with hash chaining and writer safety. |
| `mct-iroh` | Mother-owned Iroh endpoint and MCT ALPN protocol adapter. |
| `mct-daemon` | Composition layer: config, child loading, process/WASM runtimes, toy adapters, SQLite state, local control, metrics, registry, and CLI. |

## Current runtime capabilities

MCT currently supports:

- child package loading through the SDK-owned `child.toml` contract;
- strict child integrity mode using `.sha256` sidecars;
- durable child approvals and assignments;
- process child invocation after child authority;
- typed WIT component invocation;
- concrete WIT host adapters for:
  - `wasi:logging/logging@0.1.0`,
  - `patina:measure/measure@0.1.0`,
  - `patina:git/git@0.1.0`,
  - selected WASI p2 imports with explicit project-root preopens;
- local git toy execution scoped to a configured repo root;
- JSONL observations and private SQLite runtime state;
- local HTTP/UDS control snapshots;
- local Iroh hello/call protocol slices.

## Quick checks

```bash
cargo test -p mct-daemon mct_wit_runtime
cargo test -p mct-kernel -p mct-daemon
cargo clippy -p mct-daemon -- -D warnings
./scripts/ci-tier0.sh
```

## Basic CLI flow

Load children:

```bash
cargo run -p mct-daemon -- children load .mct/children --strict-integrity
```

Approve a verified child package:

```bash
cargo run -p mct-daemon -- \
  children approve slate-manager /path/to/slate-release-bundle \
  --strict-integrity
```

Persist the narrow local Slate toy authority for a project root:

```bash
cargo run -p mct-daemon -- \
  toys authorize-slate slate-manager /path/to/project \
  --children-dir /path/to/slate-release-bundle
```

Invoke a typed WIT export with `/project` explicitly preopened:

```bash
cargo run -p mct-daemon -- \
  wasm call-wit slate-manager \
  patina:slate/control@0.1.0.list-work \
  '[{"project":"/project","status":null,"kind":null}]' \
  --project-root /path/to/project \
  --children-dir /path/to/slate-release-bundle
```

`--project-root` supplies a concrete path; it does not create authority. The call path still evaluates persisted child approval and persisted toy grants before linking host imports.

## Slate/WIT package contract

A package-backed WIT child is expected to provide:

- `child.toml`;
- `[child.artifact].wasm` pointing to the package-relative wasm path;
- the declared wasm artifact at that path;
- `child.toml.sha256` containing the manifest hash;
- `<artifact>.sha256` containing the wasm hash.

MCT strict approval rejects missing or mismatched sidecars. Release tooling should preserve the package-relative artifact path rather than flattening the wasm file into the release root.

## Design notes

The important boundary is not “can the adapter do it?” but “which authority fact permits it?”

For protected effects, review the path in this order:

1. domain fact or grant that allows the action;
2. observation proving the decision;
3. adapter performing the effect;
4. observation or typed result for completion/failure;
5. caller-safe disclosure.

If any step is unclear, the correct default is deny or fail closed.

## Status

MCT is under active development. It is usable for local authority/runtime slices and Slate-like WIT child execution, but APIs and CLI surfaces are still evolving.

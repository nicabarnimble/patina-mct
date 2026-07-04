# Patina MCT

MCT is a local-first application runtime in which **no code has ambient
power**. A node runs sandboxed components (WASM or processes) that start with
zero access to the filesystem, network, secrets, or each other — every effect
a component performs must trace back to an explicit, inspectable, revocable
authority record. Nodes connect to each other over [Iroh](https://iroh.computer)
public-key networking, and there is no cloud control plane: each node is
sovereign and holds its own complete state.

The name is the model:

```text
Mother = the authority of one node   (decides everything, owns the ledger)
Child  = an application component    (computes; identified by its WIT contract)
Toy    = a host capability           (the only way a child touches the world)
```

Mother decides; children compute; toys effect. When a child wants to write a
file, tag a git repo, or emit a metric, it calls a toy — and the toy runs only
if a persisted grant, evaluated against the current call, says it may.

## Status

Early and under active development. The local slices work — loading and
approving verified child packages, invoking typed WIT exports with
capability-scoped host access, recording the audit trail — and the
peer-to-peer protocol is a functional early slice. APIs and CLI surfaces are
still evolving; nothing here is stable yet.

## What works today

- **Verified child packages** — children ship as packages with a `child.toml`
  manifest and SHA-256 sidecars; approval is refused unless both manifest and
  wasm artifact hashes verify.
- **Durable approvals and assignments** — which children may run, and where,
  is persisted authority data, not runtime state.
- **Typed WIT invocation** — call a component's WIT export with JSON
  arguments; results lift back to JSON. Process-backed children are also
  supported.
- **Capability-scoped host access** — concrete host adapters for
  `wasi:logging`, `patina:measure` (metrics), `patina:git`, and selected WASI
  filesystem imports with explicit directory preopens. A WIT import with no
  configured adapter fails closed before instantiation.
- **Execution limits** — WASM component invocations run under wall-clock
  deadlines and memory caps; process-backed children run under harness
  timeouts.
- **Audit trail** — every decision and effect emits a typed observation into
  an append-only, hash-chained ledger with exclusive writer locking and
  lock-free read-only validation. Logs and metrics are projections of this
  ledger, never the truth themselves.
- **Local control plane** — HTTP or Unix-socket status/state snapshots.
- **Peer protocols (early)** — Iroh-based hello/call slices: a peer Mother is
  admitted only against a persisted binding, and admission never pre-
  authorizes calls.

## Quick start

Build and check the workspace:

```bash
cargo test -p mct-kernel -p mct-daemon
```

Load child packages from a directory (strict integrity requires the
`.sha256` sidecars to verify):

```bash
cargo run -p mct-daemon -- children load .mct/children --strict-integrity
```

Approve a verified child package (`slate-manager`, a work-tracking component,
is the reference child used throughout):

```bash
cargo run -p mct-daemon -- \
  children approve slate-manager /path/to/slate-release-bundle \
  --strict-integrity
```

Grant the child a narrow toy authority scoped to one project directory:

```bash
cargo run -p mct-daemon -- \
  toys authorize-slate slate-manager /path/to/project \
  --children-dir /path/to/slate-release-bundle
```

Invoke a typed WIT export, with `/project` explicitly preopened:

```bash
cargo run -p mct-daemon -- \
  wasm call-wit slate-manager \
  patina:slate/control@0.1.0.list-work \
  '[{"project":"/project","status":null,"kind":null}]' \
  --project-root /path/to/project \
  --children-dir /path/to/slate-release-bundle
```

Note that `--project-root` supplies a path; it does not create authority. The
call still evaluates the persisted approval and toy grants before any host
import is linked.

## Security model

The guarantees MCT is built to give you:

- **Authority before effects.** Child calls, toy calls, peer admission, and
  filesystem access are authorized before any adapter effect runs. Executable
  child, toy, and route authority is carried by private, kernel-minted
  capability tokens and checked for stale revisions at effect boundaries —
  earlier admission is never treated as permanent permission.
- **Nothing is ambient.** Children receive no raw filesystem roots, network
  endpoints, secrets, process handles, or database handles by default. A
  manifest `needs` entry is a request; it grants nothing.
- **Fail closed.** Unknown, expired, revoked, mismatched, or malformed state
  becomes a typed denial — never a permissive default. Unconfigured WIT
  imports refuse to instantiate.
- **Verified before approved.** Approval cannot persist unless the child's
  manifest and artifact hashes verify against their sidecars.
- **Optimization cannot grant authority.** Routing ranks only candidates that
  already passed authority checks.
- **Everything is auditable.** Decisions and effects are recorded in the
  hash-chained observation ledger before effects proceed. Denials carry a
  precise internal reason in the ledger and a deliberately vague external
  message ("not authorized") to the caller.

## Workspace

| Crate | Role |
| --- | --- |
| `mct-kernel` | Pure authority domain: typed records and decisions for calls, children, peers, routes, toys, and observations. No I/O. |
| `mct-observation` | Append-only JSONL observation ledger with hash chaining, single-writer locking, and read-only validated access. |
| `mct-iroh` | Mother-owned Iroh endpoint and the MCT hello/call protocol adapters. |
| `mct-daemon` | Composition: config, child loading, WASM/process runtimes, toy adapters, SQLite state, control plane, and the CLI. |

The kernel decides; the other crates gather facts for it and perform effects
it has authorized. Wasmtime, Iroh, SQLite, and filesystem details never
appear in kernel APIs.

## Building child packages

A package-backed WIT child provides:

- `child.toml` — the manifest (SDK-owned contract);
- `[child.artifact].wasm` in the manifest, pointing to the package-relative
  wasm path;
- the wasm artifact at that path;
- `child.toml.sha256` and `<artifact>.sha256` sidecars containing the
  respective hashes.

Strict approval rejects missing or mismatched sidecars. A common packaging
mistake is flattening the wasm file into the release root — keep the
artifact at its package-relative path so the digests verify.

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

All three must pass before a change lands.

## Learn more

The `layer/` directory is the project's knowledge base:

- [What is MCT](layer/core/what-is-mct.md) — the full narrative: the model,
  the anatomy of a peer call, multi-node Visions, and what MCT is not.
- [Dependable Rust](layer/core/dependable-rust.md) — the code discipline this
  workspace is built under.
- `layer/allium/mct-product-map.allium` — the semantic specification; where
  prose and spec disagree, the spec wins.

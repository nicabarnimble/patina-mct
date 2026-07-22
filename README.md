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

Version 0.2.0 is a pre-GA release for `aarch64-apple-darwin`. Its local
runtime, signed peer admission, launchd supervision, immutable artifact
acquisition, temporal triggers, scoped Watch delivery, closed release package,
and exact-approved upgrade path are proven. APIs and CLI surfaces are still
evolving; operational `patinaMother` shutoff and into-the-wild 1.0.0 GA remain
separate, unclaimed gates.

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
- **Observed service and release lifecycle** — macOS user-launchd install/start/
  stop/restart/uninstall, closed ad-hoc-signed archives with SBOM/provenance,
  immutable daemon-release evidence, and digest-exact guided upgrade.
- **Local control plane** — HTTP or owner-authenticated Unix-socket status,
  calls, and administrative mutations.
- **Peer protocols (early)** — Iroh-based hello/call slices: a peer Mother is
  admitted only against a persisted binding, and admission never pre-
  authorizes calls.

## Quick start

Obtain the target archive and both sidecars, then verify from a trusted matching
checkout before extracting:

```bash
archive=/absolute/path/mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz
./scripts/verify-release-artifact.sh "$archive"
```

Set `MCT` to the extracted distributed executable and install it through the
observed launchd lifecycle:

```bash
MCT=/absolute/path/mct-daemon-v0.2.0-aarch64-apple-darwin/payload/mct-daemon.app/Contents/MacOS/mct-daemon
"$MCT" install --executable "$MCT"
"$MCT" start
"$MCT" status --json
```

Acquire selected local Child output into the immutable artifact catalog. This
records independent digest/evidence facts; it does not approve or assign the
Child:

```bash
"$MCT" artifacts stage /path/to/slate-build-output \
  --manifest slate-manager.toml \
  --component slate-manager.wasm \
  --child slate-manager \
  --version 0.2.0 \
  --children-dir ~/.mct/children \
  --state ~/.mct/state.sqlite \
  --ledger ~/.mct/observations.jsonl \
  --uds ~/.mct/control.sock \
  --json
```

Approve only the exact acquisition-backed digest returned by staging, then
grant the narrow Slate Toy authority:

```bash
"$MCT" children approve slate-manager ~/.mct/children \
  --artifact sha256:<digest> \
  --strict-integrity \
  --state ~/.mct/state.sqlite \
  --uds ~/.mct/control.sock

"$MCT" toys authorize-slate slate-manager /path/to/project \
  --children-dir ~/.mct/children \
  --state ~/.mct/state.sqlite \
  --uds ~/.mct/control.sock
```

Upgrade is evidence-informed and requires approval equal to the complete
candidate archive identity:

```bash
"$MCT" upgrade /absolute/path/to/candidate.tar.gz \
  --expected-digest sha256:<candidate-archive-digest> \
  --approve-artifact sha256:<candidate-archive-digest> \
  --json
```

A version, filename, broad confirmation, or different digest grants no
replacement authority. Upgrade composes the existing clean stop,
`install --replace`, start, and bounded post-verification paths; it never rolls
back automatically. See
[`RELEASE-UPGRADE-v0.2.0.md`](layer/surface/build/product/RELEASE-UPGRADE-v0.2.0.md)
for diagnosis and explicit retained-release rollback.

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

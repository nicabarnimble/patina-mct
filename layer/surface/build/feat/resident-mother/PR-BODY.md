## Summary

Phase 4 adds the resident Mother daemon: `mct-daemon serve` now composes the persisted Mother identity, bound Iroh endpoint, concurrent peer serving, control plane, runtime SQLite state, observation ledger, configured child directory, and graceful shutdown into one long-running process.

## What changed

- `mct-daemon serve` persists/loads the local identity, binds the Mother Iroh endpoint, verifies the bound endpoint matches the persisted identity, refreshes peer bindings from config, serves HTTP or UDS control, owns the runtime state path, and cleans up the UDS socket on shutdown.
- The Iroh serve path now accepts concurrently with a bounded connection limit. Per-peer hello authority state is endpoint-keyed, retains admitted hellos only, removes/no-ops denied hellos, and evicts oldest admissions first when the cap is reached.
- Ledger writes run through one resident ledger-writer task. Authority observations are acknowledged before execution effects proceed; execution observations are written after adapter completion.
- SQLite access stays outside async shared state through bounded resident helpers and `spawn_blocking` boundaries.
- Resident call execution dispatches configured and approved process children and wasm-WIT children. Each call reprojects persisted config/state into kernel authority, receives kernel-minted capabilities per call, and fails closed with both safe peer-facing denial text and typed ledgered denial reasons.
- The control/status surface reports resident operational state: endpoint lifecycle and status, accepted-connection count, loaded child count, approved child count, peer binding count, and ledger sequence tip.

## Behavior changes

- Legacy `iroh serve` and `iroh serve-process` paths now ride the concurrent serve API.
- `/status` may include resident status fields when served by the resident Mother.
- `.gitignore` now root-anchors `/build/` so `layer/surface/build/...` phase records can be staged normally.

## Non-goals

- No payload byte data plane or blob transfer.
- No routing-engine consumption of `AuthorizedRouteExecution`.
- No cryptographic binding signature verification.
- No broader toy catalog or multi-Vision publication work.

## Verification

- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `./scripts/ci-tier0.sh`
- Resident daemon integration coverage: `resident_mother_serves_peer_control_and_shutdown`, `resident_execution_runs_wit_child_and_records_trace`, and bounded per-peer Iroh serve-state tests.

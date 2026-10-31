# SQLite write patterns and pi-durable group commit

> **Observational note, 2026-10-30.** Status: **observation only**. No code changes. Read-only source review on `feat/iroh-admin-daykey` at `263d550`. Prompted by Mario Zechner's pi-durable group-commit change ([thread](https://x.com/badlogicgames/status/2116078693827428383)): commits that pile up while one is in flight go out together, ~439 runs/s vs ~5 at 128 concurrent conversations.

## 1. The observation ledger is not SQLite

`mct-observation` has no SQLite dependency. The ledger is an append-only, hash-chained JSONL file (`seg-NNNNNN.jsonl` + `open.jsonl`) with one writer thread, enforced by a file lock (`WriterContended`). The writer already syncs each batch once (`append_durable_batch`: write every frame, then one `sync_data`). It does not yet merge separate queued write commands into one sync. That is "Option B" in [child-start-ledger-fact.md](child-start-ledger-fact.md), deferred as a later throughput step.

## 2. The only real SQLite file is the daemon's state database

`.mct/state.sqlite`, opened by `MctRuntimeStateStore` in `mct-daemon` (`rusqlite` 0.37 bundled):

- `journal_mode = WAL`, `synchronous = NORMAL`, `foreign_keys = ON`, `busy_timeout` 5 s.
- One short `BEGIN IMMEDIATE` transaction per operation, about 3 per call (reservation, run insert, completion). No autocommit write loops.
- At most 8 concurrent inbound connections (`max_concurrent_connections`). The daemon and CLI commands can open the file at the same time; WAL plus `BEGIN IMMEDIATE` plus the busy timeout serialize writers safely.
- The v0.2.0 Mac baseline is about 1 call/s (4 clients × 500 calls in 2056 s), with call p50 485 ms and p99 673 ms. Estimated under 10 commits/s total. The call is the bottleneck, not the commits.

## 3. Risk: double run after power loss

With `synchronous = NORMAL`, a power loss or OS crash can drop the state DB's latest commits while the fsynced ledger keeps them. A same-key retry could then run a Child twice. This is the known S1 hole in [child-start-ledger-fact.md](child-start-ledger-fact.md).

## 4. Minor waste: migrations on every open

`MctRuntimeStateStore::open` reruns the PRAGMAs and `CREATE TABLE IF NOT EXISTS` migrations on every open, and the call path opens the DB several times per call. Reusing connections fixes this; batching does not.

## 5. Bottom line

pi-durable's group commit would not meaningfully help MCT today; MCT's SQLite use is too sparse. Revisit only if call rates rise by orders of magnitude. If so, build the ledger's own Option B in Rust rather than porting pi-durable's TypeScript.

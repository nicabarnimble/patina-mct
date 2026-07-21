# `watch-null-sink@0.1.0` fixture provenance

This fixture is an **unmodified exact-tag source** build. No patch was applied to the sink source, WIT, manifest, package name, or version.

## Upstream identity

- Repository: `git@github.com:NicabarNimble/patina-child-watcher-system.git`
- Commit: `526dbf123b040198cb4395c1a63cf498a28ff915`
- Tag: `watch-null-sink-v0.1.0` (peeled commit exactly as above)
- Package: `patina-ai-child-watch-null-sink 0.1.0`
- Manifest source: `children/watch-null-sink/child.toml`

The source was obtained with `git archive` from the exact commit into a temporary directory. The upstream worktree was neither read for build inputs nor modified.

## Build

Built on macOS aarch64 with:

- `rustc 1.94.0 (4a4ef493e 2026-03-02)`
- `cargo 1.94.0 (85eff7c80 2026-01-15)`
- `cargo-component 0.21.1`
- `wasm-tools 1.245.0`

Exact command from the unmodified archive root:

```text
cargo component build --release -p patina-ai-child-watch-null-sink
```

Output copied from:

```text
target/wasm32-wasip1/release/patina_ai_child_watch_null_sink.wasm
```

The archive had no committed `Cargo.lock`; this receipt binds the resulting bytes and exact toolchain/build command.

## Fixture receipts

### `child.toml`

- Bytes: `358`
- SHA-256: `6447a156d08b4b438acc1b55f28cf05d1130889479f4428aca98b2e6d327238a`
- BLAKE3: `780dc11735559d58248f69be4ca64659e82a0e4ba36d886d4a7be7608275fedf`

### `watch-null-sink.wasm`

- Bytes: `70,027`
- SHA-256: `37f42ebe17db2c6e44e02bf79cf590ba899d3cb96579bbd4ed735597f53dbfe3`
- BLAKE3: `e40605e80b5a61fa7340abc1207676848b1a1383048bb1c5dc13c20e11d7a0cf`

The raw fixture directory intentionally contains no `.sha256` sidecars. Acquisition staging creates canonical package sidecars after durable authority and verification.

## WIT inventory

Business export:

- `patina:watch/events@0.1.0.emit` with the exact legacy `file-change` record.

Behavior imports:

- `wasi:logging/logging@0.1.0`;
- `patina:measure/measure@0.1.0`; and
- `patina:watch/types@0.1.0` for the exported record shape.

The Rust/WASI component also carries expected Preview 2 CLI, I/O, clock, and filesystem type/preopen imports. The sink source performs no filesystem observation or content access and receives no preopen in the composed proof. It has no Watch, keyvalue, messaging, trigger, network, registry, secret, or acquisition authority.

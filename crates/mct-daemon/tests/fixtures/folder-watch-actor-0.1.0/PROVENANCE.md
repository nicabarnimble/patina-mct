# `folder-watch-actor@0.1.0` fixture provenance

This component is a **source-derived MCT security rebuild**, not the unmodified upstream release binary.

## Upstream identity

- Repository: `git@github.com:NicabarNimble/patina-child-watcher-system.git`
- Commit: `526dbf123b040198cb4395c1a63cf498a28ff915`
- Tag: `folder-watch-actor-v0.1.0` (peeled commit exactly as above)
- Package: `patina-ai-child-folder-watch-actor 0.1.0`
- Manifest source: `children/folder-watch-actor/child.toml`

The source was obtained with `git archive` from the exact commit into a temporary directory. The upstream worktree was neither read for build inputs nor modified.

## Reviewed security patch

`MCT-REBUILD.patch` is applied at the archive root with `git apply --check` followed by `git apply` (zero fuzz).

- Bytes: `2,362`
- SHA-256: `a71f9924149cd96647de542e3eaf6940930fe77e5dcc626b07c38b4bd67d9a2c`
- BLAKE3: `202bb03b3caf9cd258a59ab01f7ac76a7be2359d90e861502c8a72e230c10529`

The patch has only two security effects:

1. it removes unused declared HTTP, connect, SQL, event-stream, task, peer, and Git imports from the component world; and
2. it makes the legacy `absolute-path` slot receive the same normalized root-relative value as `relative-path`, including deletion identities.

It does not change polling cadence, scan/diff/filter behavior, event classes, WIT business exports, package name, or version. Runtime validation still rejects unequal legacy slots rather than sanitizing them.

## Build

Built on macOS aarch64 with:

- `rustc 1.94.0 (4a4ef493e 2026-03-02)`
- `cargo 1.94.0 (85eff7c80 2026-01-15)`
- `cargo-component 0.21.1`
- `wasm-tools 1.245.0`

Exact command from the patched archive root:

```text
cargo component build --release -p patina-ai-child-folder-watch-actor
```

Output copied from:

```text
target/wasm32-wasip1/release/patina_ai_child_folder_watch_actor.wasm
```

The archive had no committed `Cargo.lock`; this receipt binds the resulting bytes and the exact toolchain/build command rather than claiming future dependency-index builds are byte-reproducible without verification.

## Fixture receipts

### `child.toml`

- Bytes: `719`
- SHA-256: `f1f53ce495b3c5c408bb582a3d8a3d100f33102a4a355bdea2ac7848831c790a`
- BLAKE3: `6ca8f8225fe11ae00c94d08fb8e530f53741c3b0dcfde2273a8e355fe52be718`

### `folder-watch-actor.wasm`

- Bytes: `352,529`
- SHA-256: `00910422135e822524cd52e446c157056e755187392a36d441cd1ba406ba9096`
- BLAKE3: `466033617dcd41c532f22f881f7fba347a793c9c52c1ac89e93f4df902ce251e`

The raw fixture directory intentionally contains no `.sha256` sidecars. Acquisition staging creates canonical package sidecars after durable authority and verification.

## WIT inventory

Business exports:

- lifecycle exports `init`, `name`, `on-load`, `on-unload`, `health`, `handle`, `drain`, and `tick`;
- `patina:watch/control@0.1.0` (`configure`, `status`, `scan-now`, `reset`).

Direct behavior imports retained by the rebuilt world:

- `wasi:keyvalue/store@0.2.0`;
- `wasi:messaging/producer@0.2.0` and its types;
- `wasi:logging/logging@0.1.0`;
- `patina:measure/measure@0.1.0`;
- `patina:child/runtime-types@0.1.0`; and
- `patina:watch/types@0.1.0`.

The Rust/WASI component also imports the expected Preview 2 CLI, I/O, clocks, random, filesystem types, and filesystem preopens used by `std::fs`. It does not retain the pruned HTTP, SQL, connect, event-stream, task, peer, or Git business imports. MCT supplies no ambient network, write, registry, or acquisition adapter for this fixture.

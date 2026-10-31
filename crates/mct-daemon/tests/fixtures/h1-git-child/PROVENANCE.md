# `h1-git-child@0.1.0` test fixture provenance

This component is an in-tree MCT test fixture used by `crates/mct-daemon/src/wasm/tests/git_confinement.rs` to exercise composed WASI filesystem writes followed by WIT `patina:git/git@0.1.0` calls.

## Source

- Crate manifest: `crates/mct-daemon/tests/fixtures/h1-git-child/Cargo.toml`
- Lockfile: `crates/mct-daemon/tests/fixtures/h1-git-child/Cargo.lock`
- Source: `crates/mct-daemon/tests/fixtures/h1-git-child/src/lib.rs`
- WIT world: `crates/mct-daemon/tests/fixtures/h1-git-child/wit/world.wit`

## Build

Built offline on macOS aarch64 with:

- `rustc 1.96.0`
- `cargo 1.96.0`
- `wasm-tools 1.245.0`

Exact build commands from the repository root:

```sh
CARGO_TARGET_DIR=target/h1-child \
cargo build --manifest-path crates/mct-daemon/tests/fixtures/h1-git-child/Cargo.toml \
  --target wasm32-wasip1 --release --locked --offline

wasm-tools component new \
  target/h1-child/wasm32-wasip1/release/mct_h1_git_child.wasm \
  -o crates/mct-daemon/tests/fixtures/h1-git-child/child.wasm
```

Building the core module on `wasm32-wasip1` with `#![no_std]` and wrapping it directly with `wasm-tools component new` preserves the exact WASI 0.2.3 imports declared in `wit/world.wit` rather than linking Rust 1.96's `wasm32-wasip2` sysroot adapter (`@0.2.6`).

## Fixture receipt (`child.wasm`)

- Bytes: `25,835`
- SHA-256: `bba25cd1fa6c437a41f98e7f2897c0c25f1077d5c520f3331abdcfa8ce6263ca`

## WIT inventory

Imports:

- `patina:git/git@0.1.0` (`add-paths`, `commit`)
- `wasi:filesystem/types@0.2.3`
- `wasi:filesystem/preopens@0.2.3`

Exports:

- `patina:demo/control@0.1.0` (`run`)

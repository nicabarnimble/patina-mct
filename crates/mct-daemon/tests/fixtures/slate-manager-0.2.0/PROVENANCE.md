# slate-manager@0.2.0 fixture provenance

This is real raw build output for the Slate Child, retained as the first
external compatibility fixture for the MCT replacement proof.

- Upstream repository: `https://github.com/nicabarnimble/patina-child-slate`
- Tag: `v0.2.0`
- Commit: `fb85706aad55fdfbf091e28ac8f4c09864996b0c`
- Build command: `cargo component build --release`
- Rust: `rustc 1.96.0 (ac68faa20 2026-05-25)`
- cargo-component: `0.21.1`
- wasm-tools used to inspect the component: `1.245.0`

| File | Bytes | SHA-256 | BLAKE3 |
|---|---:|---|---|
| `slate-manager.toml` | 1801 | `b6d7b4e532df5b787acd37f3ae8c25ed093552097e5cf6dbc5c7eaca360e4919` | `7dccdaf3bc348c76e53f9b30e0f9e59c2d40cf8b3453d3e80af5710cca4a7161` |
| `slate-manager.wasm` | 1338615 | `76b568f40491d7e3bd1dcb55644ec7c42dbc393642a5a7a2ba5b1daa1ea6966a` | `e06cab5f7605f3c070ef792f67f7b71a179d8a9c7da0c45e525b39e8a3a88e7d` |

The raw fixture intentionally has no `.sha256` sidecars and its historical
flat manifest has no `[child.artifact]` declaration. `artifacts stage` must
create the canonical package and sidecars without mutating these source files.
This metadata is test-fixture provenance, not runtime acquisition authority.

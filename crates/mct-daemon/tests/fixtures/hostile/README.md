# Hostile fixture Children (C3)

Each `.wat` component asks for ambient network power that MCT never links.
`src/wasm/tests/hostile.rs` runs them through the real Wasmtime boundary with
the most permissive configured host (WASI enabled) and requires an instantiate
refusal. That is local, legacy-profile baseline evidence only.

The governed-profile Pi run (NIC-129) reuses these components and adds the
cases tracked in `docs/done/known-failures.json`: ungranted path, `..` escape,
symlink escape, call without a grant, and forged caller principal (NIC-127).

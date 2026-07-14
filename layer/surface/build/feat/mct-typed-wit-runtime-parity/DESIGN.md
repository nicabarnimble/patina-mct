# Design: Restore typed WIT component runtime parity in MCT

## Why This Design

`patinaMother` proved the useful technical direction: `patinaChild` behavior can live behind WIT-defined WASM components while `mctMother` owns runtime linking and authority. The accepted responsibility should be rebuilt under MCT law; the legacy coupling and capability model are not portable.

MCT should keep the model and remove the coupling:

- kernel decides;
- daemon adapter performs;
- WIT defines the child contract;
- toys/WASI imports are explicit capabilities;
- observations record runtime truth.

## Build Target

Add a typed WIT invocation lane to `mct-daemon` that can execute a versioned component export selected by `MctCall.target`.

Canonical operation identity:

```text
<namespace>/<interface_name>.<function_name>
```

Example:

```text
patina:slate/control@0.1.0.list-work
```

This should resolve against Wasmtime component exports and be checked against the child's MCT allowlist before invocation.

## Resolved Decisions

1. **No legacy handle fallback in this spec.** Strict typed WIT invocation is the canonical path.
2. **No Wasmtime in kernel.** Wasmtime/WASI/WIT-bindgen types stay in `mct-daemon` or later adapter crates.
3. **Use `patinaMother` as behavioral evidence, not inherited ownership.** Translate requirements and rebuild them under MCT law; do not port broad containers such as `MotherRuntimeStore`.
4. **Slate is the proof target.** The first real product fixture should be Slate-like `list-work`, not a fake generic demo forever.

## Commits

1. `spec: draft mct-typed-wit-runtime-parity` — create concrete spec and design target.

## Direct Code Targets

- `crates/mct-daemon/src/wasm.rs` — add typed component invocation API.
- `crates/mct-daemon/src/lib.rs` — export typed WIT runtime surface.
- `crates/mct-daemon/src/children.rs` — reuse/strengthen operation allowlist shape if needed.
- `crates/mct-kernel/src/call/mod.rs` — only if operation identity helpers belong in kernel as pure string/domain helpers; no Wasmtime types.
- `crates/mct-kernel/src/observation.rs` — only for new canonical observation kinds if current adapter diagnostics are insufficient.

## Verification Plan

- Start with pure operation-id tests.
- Add component fixture tests before touching Slate.
- Add Slate-like WIT fixture once generic invocation works.
- Run `./scripts/ci-tier0.sh` after each meaningful slice.

## Build Readiness

The first implementation slice is ready:

```text
mct_wit_runtime_resolves_versioned_component_export
mct_wit_runtime_lowers_record_args_and_lifts_record_result
```

## Open Questions

- Should JSON input live as a new runtime adapter parameter, or should MCT add a first-class payload store before full Slate execution?
- Which Slate operation is the smallest useful fixture: `list-work` or `show-work`?
- How much of Mother `typed_conversion.rs` should be translated before we introduce a narrower conversion module in MCT?

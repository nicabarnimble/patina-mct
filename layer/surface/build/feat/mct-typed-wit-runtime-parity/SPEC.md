---
type: feat
id: mct-typed-wit-runtime-parity
status: active
created: 2026-06-29
target: mct-wasm-component-runtime
sessions:
  origin: 20260628-062914-739494000
  work: []
related:
  - layer/surface/build/feat/mct-typed-wit-runtime-parity/DESIGN.md
  - /Users/nicabar/Projects/Sandbox/AI/RUST/patina/src/child/internal/child.rs
  - /Users/nicabar/Projects/Sandbox/AI/RUST/patina/src/child/internal/typed_conversion.rs
  - /Users/nicabar/Projects/Sandbox/AI/RUST/patina/wit/child/child.wit
  - /Users/nicabar/Projects/Patina/patina-child-slate/child.toml
  - /Users/nicabar/Projects/Patina/patina-child-slate/wit-contract/slate.wit
beliefs:
  - mother-kernel-decides-adapters-perform
  - greenfield-products-reference-legacy
references:
  - layer/core/adapter-pattern.md
  - layer/core/safety-boundaries.md
  - layer/core/dependable-rust.md
exit_criteria:
  - id: typed-export-resolution
    text: MCT resolves MctCall OperationTarget values to versioned WIT component exports such as patina:slate/control@0.1.0.list-work.
    checked: true
    verify: cargo test -p mct-daemon mct_wit_runtime_resolves_versioned_component_export
  - id: json-wit-lowering-lifting
    text: MCT lowers JSON call inputs into WIT values and lifts WIT results back into JSON-backed MctResult data for scalar, list, option, record, and result-shaped values needed by Slate.
    checked: true
    verify: cargo test -p mct-daemon mct_wit_runtime_lowers_record_args_and_lifts_record_result
  - id: authority-before-execution
    text: MCT invokes typed WIT exports only after AuthorizedChildInvocation and child contract allowlist validation.
    checked: true
    verify: cargo test -p mct-daemon mct_wit_runtime_rejects_non_allowlisted_operation
  - id: fail-closed-missing-export
    text: Missing or mismatched WIT exports fail closed with typed adapter errors and observations, not fallback handle dispatch.
    checked: true
    verify: cargo test -p mct-daemon mct_wit_runtime_rejects_unexported_operation
  - id: wasi-toy-host-imports
    text: Required WASI/toy host imports for Slate-like children are explicit MCT adapter capabilities, with missing grants denied before ambient access.
    checked: false
    verify: cargo test -p mct-daemon mct_wit_runtime_denies_missing_host_import_grant
  - id: slate-fixture
    text: A Slate-like WIT component fixture executes list-work through the MCT runtime path using the Slate WIT contract shape.
    checked: false
    verify: cargo test -p mct-daemon slate_manager_list_work_runs_through_mct_wit_runtime
  - id: kernel-boundary
    text: mct-kernel remains free of concrete Wasmtime, WASI, WIT-bindgen, filesystem, and HTTP runtime types.
    checked: false
    verify: rg -n "wasmtime|wasmtime_wasi|wit_bindgen|wasi_" crates/mct-kernel/src returns no matches
---

# feat: Restore typed WIT component runtime parity in MCT

> MCT must regain the original Mother ability to run WIT-defined WASM component children like `slate-manager`, using MCT authority records and adapter observations instead of Mother's coupled runtime internals.

## Problem

WASM/WIT is critical to MCT. Original Patina Mother had a real Wasmtime component runtime for WIT children: component export discovery, typed operation resolution, JSON-to-WIT lowering, WIT-result lifting, WASI host imports, and child manifest contract validation.

New MCT has the authority spine and a minimal Wasmtime component proof, but it currently calls only narrow test exports and does not yet run a full WIT child like `slate-manager`.

## Goal

Build the MCT typed WIT runtime adapter until a Slate-shaped component can be invoked through MCT's authority-first path.

The runtime must:

- treat `MctCall.target` as the semantic operation identity;
- require `AuthorizedChildInvocation` before runtime effects;
- validate the operation against the child contract allowlist;
- resolve versioned WIT exports;
- lower JSON call arguments into WIT values;
- call the component export;
- lift WIT results into JSON result data;
- emit adapter observations for traps, missing exports, and host-call failures;
- keep Wasmtime/WASI concrete types out of `mct-kernel`.

## Status

Active. This spec replaces abstract Slate planning for this work. Slate remains useful as a backlog, but this runtime parity work should be driven by executable spec criteria and tests.

## Non-Goals

- No copy-paste resurrection of Mother runtime containers.
- No Belief/scry/assay/oxidize/scrape surfaces in MCT core.
- No raw filesystem, HTTP, Iroh, or secret handles given to children.
- No fallback from strict WIT invocation to legacy `handle(action, payload)` unless a separate compatibility spec explicitly requires it.
- No full Slate product polish before the runtime can execute the Slate WIT shape.

## Target Shape

MCT owns these layers:

1. `mct-kernel`: authority facts and decisions only.
2. `mct-daemon` WASM adapter: Wasmtime/WASI/WIT runtime effects.
3. `mct-observation`: durable facts about decisions and effects.
4. child package: WIT contract and component implementation.

A call such as:

```text
namespace = "patina:slate"
interface_name = "control@0.1.0"
function_name = "list-work"
```

must map to the WIT export:

```text
patina:slate/control@0.1.0.list-work
```

and execute only if the child is approved, assigned, ready, and explicitly allowed to export that operation.

## Solution

Use the original Mother implementation as reference material, especially:

- `src/child/internal/child.rs`
- `src/child/internal/typed_conversion.rs`
- `wit/child/child.wit`
- `patina-child-slate/wit-contract/slate.wit`

Translate the design into MCT as narrower adapter modules:

- typed operation identity parser/resolver;
- component export discovery;
- JSON/WIT value conversion;
- authorized typed component invocation;
- host import linker construction from explicit toy/grant facts;
- adapter diagnostic observations.

## Implementation Order

1. Add typed operation identity resolution from `OperationTarget` to canonical WIT operation id.
2. Add component export discovery and fail-closed missing-export errors.
3. Add JSON-to-WIT lowering and WIT-to-JSON lifting for Slate-needed shapes.
4. Add authorized typed component invocation API in `mct-daemon`.
5. Add focused WIT component fixtures proving scalar, record, option/list, result, and trap paths.
6. Add first Slate-like fixture using `patina:slate/control@0.1.0.list-work`.
7. Add explicit host import grant handling for logging, measure, git, and filesystem-like toys.

## Resolved Decisions

- Spec is the driver for this work; Slate is not the primary planning surface here.
- Strict WIT invocation is canonical. Legacy handle bridges are deferred.
- MCT may reuse Mother behavior as reference, but not Mother runtime coupling.
- Authority remains in kernel records; Wasmtime/WASI behavior remains in daemon adapters.

## Verification

- `cargo test -p mct-daemon mct_wit_runtime_resolves_versioned_component_export`
- `cargo test -p mct-daemon mct_wit_runtime_lowers_record_args_and_lifts_record_result`
- `cargo test -p mct-daemon mct_wit_runtime_rejects_non_allowlisted_operation`
- `cargo test -p mct-daemon mct_wit_runtime_rejects_unexported_operation`
- `cargo test -p mct-daemon slate_manager_list_work_runs_through_mct_wit_runtime`
- `cargo test -p mct-daemon`
- `./scripts/ci-tier0.sh`

## Exit Criteria

See frontmatter `exit_criteria`.

## Build Readiness

Ready to implement in small commits. First build target should be typed export resolution plus JSON/WIT conversion tests, before any Slate-specific integration.

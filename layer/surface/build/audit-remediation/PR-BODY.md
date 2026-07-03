# Harden MCT executable authority capabilities

## Summary

This closes Phase 3 of the audit-remediation arc. The remaining conventionally enforced authority records are now executable capabilities that only kernel evaluators can mint, and observation-ledger verification no longer reopens a second writer just to read.

## Changes

- Persist run provenance instead of executable `AuthorizedChildInvocation` records.
  - Adds `ChildInvocationProvenance` for durable evidence.
  - Migrates legacy run rows from the embedded capability shape.
  - Bumps daemon state schema to version 4.
- Seal executable authority tokens.
  - `AuthorizedChildInvocation`, `AuthorizedToyCall`, and `AuthorizedRouteExecution` have private fields and read-only accessors.
  - They are no longer serde authority payloads and are not cloneable executable tokens.
  - Tests mint capabilities through real kernel evaluators/shared fixtures instead of struct literals.
- Guard stale capabilities at effect boundaries.
  - Process, WASM, and toy adapters compare minted policy/grant revisions against current facts before executing.
  - Revision mismatches return typed denial/observation paths without spawning, loading, or calling backends.
- Add read-only observation ledger access.
  - `JsonlObservationLedger::open_read_only`, `JsonlObservationLedgerReader`, and `read_ledger_entries` share the existing incremental identity/hash-chain validation.
  - Read-only verification takes no exclusive writer lock, closing the `fake_echo_slice_records_trace_and_result` writer-reopen flake class by design.
- Documentation closeout.
  - README and `layer/core` facts now match current capability sealing, ledger access, execution limits, module layout, and typed WIT/process runtime scope.
  - `PHASE3.md` records arc completion, stress proof, and scoped-out future work.

## Behavior changes

- Control-plane run snapshot JSON changes shape for child invocation authority: persisted run records now expose provenance references/facts rather than an embedded executable `AuthorizedChildInvocation` record.
- No intended runtime behavior change beyond that disclosed snapshot shape change; authority allow/deny semantics remain fail-closed.

## Invariant/audit notes

- Kernel remains the only mint point for executable authority.
- Durable state stores facts/provenance, not executable authority.
- Effect adapters reject stale minted revisions before effects.
- Observation read paths validate ledger identity and hash chain without contending for the writer lock.
- Grep audit during Phase 3 found zero out-of-kernel struct-literal constructions of the sealed capability types.

## Validation

- `cargo test --workspace` ✅
- `cargo clippy --workspace --all-targets -- -D warnings` ✅
- `./scripts/ci-tier0.sh` ✅

Additional T6 stress proof:

- `cargo test -p mct-daemon --lib`: 10/10 consecutive green runs.
- `cargo test --workspace`: 3/3 consecutive green runs.

## Scoped out

- No push, PR opening, or release publication in this branch by the assistant.
- No new runtime features, storage backends, transport abstractions, UI/inspector surfaces, or federation/thought-mesh work.
- Future route-execution adapters must apply the same stale-revision guard before consuming `AuthorizedRouteExecution`.

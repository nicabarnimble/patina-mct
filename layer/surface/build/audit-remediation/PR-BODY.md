# Harden the MCT authority runtime (audit remediation, Phases 1–3)

## Summary

This branch is the full remediation arc from a code audit of the workspace
against *Rust for Rustaceans* principles and the project's own
`layer/core/dependable-rust.md` doctrine: 55 commits across three phases,
tracked task-by-task in `layer/surface/build/audit-remediation/PHASE2.md`
and `PHASE3.md`. The arc moves the system's core promise — authority before
effects — from convention to construction: authority facts are validated
types, effects run under enforced limits, and executable authority is
carried by capability tokens only kernel evaluators can mint.

## Phase 1 — correctness and security invariants

- **Real clocks everywhere.** Hello admission, toy-grant evaluation, run
  records, and kernel observation constructors all used hardcoded
  timestamps in production paths; binding/grant expiry could never fire
  correctly. Time is now an explicit adapter-supplied input at every
  kernel boundary.
- **Validated, chronologically ordered `Timestamp`.** Previously a string
  newtype with lexicographic ordering and two incompatible formats
  (RFC3339 and epoch-seconds) in circulation. Construction and serde now
  reject non-RFC3339 input with typed errors; ordering is by instant.
- **WASM execution limits.** Component invocations run under epoch-based
  wall-clock deadlines derived from `MctCall.deadline` and an explicit
  memory cap; a runaway or over-allocating child yields a typed
  `TimedOut`/limit failure with observations instead of hanging the Mother.
- **Trust-boundary guards.** The Iroh node secret key is written `0o600`;
  git toy tag/ref arguments are wrapped in a constructor-validated type
  that rejects leading-dash argument injection.
- **Invalid states made unrepresentable.** `MctCallPayloadHandle` became a
  data-carrying enum (per-variant required fields replace runtime field
  policing); string ID newtypes reject empty/blank input at construction.
- **Observation ledger hardening.** The crash-fragile lock-marker file was
  replaced by an OS advisory lock (self-releasing on process death), which
  also eliminated an O(n²) full-file rescan on every append.
- **API constraint pass.** Curated explicit re-exports replace glob
  re-exports in the kernel; error enums are `#[non_exhaustive]`; error
  sources are typed and chained instead of stringified.

## Phase 2 — robustness and doctrine alignment

- Timeouts on every Iroh serve/roundtrip network wait, with typed timeout
  errors (fail closed, no partial replies).
- Serve-state decision/observation IDs are unique across daemon restarts.
- The control plane fails closed on storage errors instead of serving
  empty-but-healthy snapshots, reuses its state store, and runs blocking
  SQLite work off the async executor.
- Streaming, incrementally validated ledger reads (`iter_entries`).
- `#![warn(missing_docs)]` on the kernel with real invariant documentation
  on the authority surface (what each evaluator decides, from which facts,
  and its fail-closed guarantee).
- `mct-iroh` split into black-box modules per the house doctrine
  (endpoint lifecycle / identity / protocol serving / framing internals),
  with the crate's public API unchanged.

## Phase 3 — unforgeable executable capabilities

- **Persistence carries evidence, not authority.** Run records persist a
  serializable `ChildInvocationProvenance` instead of an embedded
  executable record; daemon state schema bumps to v4 with a migration for
  legacy rows.
- **Sealed capability tokens.** `AuthorizedChildInvocation`,
  `AuthorizedToyCall`, and `AuthorizedRouteExecution` now have private
  fields, read-only accessors, and no `Clone`/serde: the only way one can
  exist is as the result of a successful kernel evaluation. Child
  invocation is consumed by value (single-effect); route execution is
  sealed as single-effect authority and currently has no daemon adapter
  consumer. The toy capability is session-scoped for the WIT host's
  multiple-toy-calls-per-invocation model, documented on the type.
- **Staleness guards at effect boundaries.** Process, WASM, and toy
  adapters compare the capability's minted policy/grant revisions against
  current facts before executing; mismatch is a typed denial plus
  observation, never execution.
- **Read-only ledger access.** `open_read_only`/`read_ledger_entries`
  share the incremental identity/hash-chain validation but take no writer
  lock — verification and inspection no longer contend with the live
  writer, closing an intermittent writer-reopen test flake by design.
- Tests mint capabilities through real kernel evaluations via shared
  fixtures; a grep audit confirms zero out-of-kernel struct-literal
  constructions of the sealed types.

## Disclosed behavior changes

- Control-plane run-snapshot JSON exposes provenance references instead of
  an embedded `AuthorizedChildInvocation` record.
- Daemon state schema v4 (automatic migration from v3 rows).
- JSON edges and constructors now reject malformed timestamps and
  empty/blank IDs with typed errors (previously accepted silently).
- Control-plane snapshots return an error status on storage failure
  instead of a silently empty snapshot.
- No other intended runtime behavior change; authority allow/deny
  semantics remain fail-closed throughout.

## Documentation

- README rewritten for external readers (model, quick start, security
  model as user-facing guarantees, child-package contract).
- `layer/core` docs corrected to match current facts (capability sealing,
  ledger access, execution limits, module layout).
- `PHASE2.md`/`PHASE3.md` record every task, the flake log with captured
  evidence, stress counts, and scoped-out future work: no push/release work,
  no new runtime/storage/transport/UI/federation features, future crate
  splits only after a proven seam, and any future `AuthorizedRouteExecution`
  daemon consumer must apply the same stale-revision guard when it arrives.

## How to verify

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

Final closeout validation is green. Flake-sensitive suites were stress-proven:
`cargo test -p mct-daemon --lib` 10× consecutive green and `cargo test
--workspace` 3× consecutive green after the read-only ledger change; the Iroh
timeout tests were made deterministic (typed-error assertions, no wall-clock
margins).

🤖 Generated with [Claude Code](https://claude.com/claude-code)

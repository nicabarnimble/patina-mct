# TODO: WASM-only local Children

**Status:** Approved direction; autonomous execution authorized after the current documentation foundation is checkpointed.

**Product decision:** Option C. A locally executed Child is a WASM component. Native and JVM processes are external integrations or trusted Mother-side adapters, not local Children.

**Engineering posture:** Hashimoto-style subtraction at the product boundary; Gjengset-style types and tests at the implementation boundary.

This file is the execution ledger. An agent may complete it without further operator design gates as long as it stays within the settled decisions, milestone boundaries, compatibility rules, and stop conditions below.

---

## 1. Outcome

When this work is complete:

```text
Local Child  = WASM component confined by the component runtime
Remote Child = Child governed by another Mother
Native/JVM   = external integration or trusted Mother adapter
```

The repository must make these invalid states unreachable:

- approving an ordinary host executable as a local Child;
- routing a local call to a native or JVM Child runtime;
- dispatching a local Child call through `std::process::Command`;
- presenting `process call` or `iroh serve-process` as product commands;
- treating legacy handle ingress as local Child execution;
- converting historical `process` or `jvm_child` records into current execution authority.

Historical process/JVM observations and persisted rows remain readable and inspectable. They are inert evidence, never executable current authority.

---

## 2. Settled decisions

These decisions are authorized and must not be reopened during implementation.

### D-C.1 — One local Child substrate

Only a WASM component may become a current local Child execution target.

### D-C.2 — Native and JVM placement

Native and JVM software may integrate as:

- an external caller using an authenticated MCT ingress;
- a remote workload governed by another Mother;
- a trusted Mother-side adapter explicitly inside the trusted computing base.

It may not be represented as a confined local Child merely because it uses a WIT-shaped call.

### D-C.3 — Admission is not confinement

`AuthorizedChildInvocation` authorizes a Child execution start. It does not convert ordinary process access into Toy-mediated authority.

### D-C.4 — Preserve history, reject execution

Persisted and wire-visible runtime values from prior 0.x builds remain decodeable where required for ledger/state inspection. Conversion from a historical process/JVM value to a current local execution target fails closed with a typed unsupported/retired disposition.

### D-C.5 — Remove, do not hide

There is no compatibility flag, environment variable, undocumented command, alternate parser spelling, or test-only production branch that restores process-backed local Child execution.

### D-C.6 — Replace test scaffolding

Tests that currently use shell scripts as convenient Children migrate to real WASM component fixtures. Coverage, assertions, observation ordering, payload integrity, idempotency, routing, forwarding, and recovery behavior are preserved or strengthened.

### D-C.7 — Trusted subprocesses remain possible

This phase does not remove legitimate Mother-side subprocess use such as launchd operations, release tooling, platform inspection, or a separately named trusted adapter. Such code must not accept Child authority types or report itself as Child execution.

### D-C.8 — Remote substrate is not local authority

A remote route names the remote Mother boundary. The caller does not gain local process/JVM execution semantics from the remote implementation substrate.

### D-C.9 — Allium is frozen, minimally aligned

Do not expand or redesign the Allium corpus. Because it remains declared authority during this phase, make only the smallest edit required to stop blessing local `process_child` and `jvm_child` execution. Allium retirement is a separate governance change and is not required to complete Option C.

### D-C.10 — Pre-GA product break, durable evidence compatibility

Removing process-backed Child commands and APIs is an intentional pre-GA product break. Durable observation and state history must not be made unreadable merely to simplify current types.

---

## 3. Non-goals

- No native-process sandbox framework.
- No container, Seatbelt, seccomp, App Sandbox, or entitlement abstraction.
- No new JVM runtime.
- No new remote execution protocol.
- No change to Mother lifecycle subprocesses.
- No broad daemon refactor unrelated to process-Child removal.
- No new trait merely to represent one remaining WASM implementation.
- No Allium retirement or wholesale Allium-to-Markdown translation.
- No history rewrite or deletion of historical observations.
- No weakening of authority, durability, payload, idempotency, routing, or caller-safe denial behavior.
- No reopening the paused Perf Phase 0 measurement run.

---

## 4. Known impact surface

Initial inventory found approximately 165 process-runtime references across 18 Rust source files plus documentation and specifications.

Production-reachable areas include:

- `crates/mct-daemon/src/process.rs`;
- public exports from `crates/mct-daemon/src/lib.rs`;
- `process call` in `daemon/cli_runtime.rs` and `main.rs`;
- `iroh serve-process` in `daemon/ingress.rs` and `main.rs`;
- resident local process dispatch in `daemon/resident/execution.rs`;
- handle-to-process candidate mapping in `daemon/resident/candidates.rs`;
- runtime parsing in `daemon/cli_admin.rs`;
- `ComponentRuntimeShape::{ProcessChild,JvmChild}`;
- `RuntimeKind::{Process,JvmChild}`;
- state and observation serialization containing historical runtime values;
- process-backed shell fixtures across resident, control, ingress, forwarding, payload, idempotency, and route tests.

The implementation must repeat this inventory at M1 and reconcile every production occurrence before close-out.

---

## 5. Branch, commit, push, and PR strategy

### Repository state at plan creation

- Working branch: `patina`.
- `patina` is ahead of `origin/patina` by the paused Perf Phase 0 commits.
- The mdBook/open-trace foundation is uncommitted.
- `origin/main` and `origin/patina` currently share the prior integration point.

Do not open a feature branch until M0 has checkpointed and pushed the documentation foundation on `patina`.

### Integration model

1. Checkpoint existing documentation work on `patina`.
2. Push `patina` to `origin/patina` as a backup/integration checkpoint.
3. Create `feat/wasm-only-local-child` from that exact pushed commit.
4. Commit this TODO first on the feature branch.
5. Open a draft PR from `feat/wasm-only-local-child` into `patina`.
6. Push at every milestone boundary and update the PR ledger.
7. Mark ready only after M8 validation and close-out.
8. Merge with a merge commit so the scalpel commit history remains visible.
9. Delete the remote feature branch after merge.

Do not open an Option C PR directly to `main`: that would couple this work to the paused Perf Phase 0 commits already carried by `patina`. A later `patina -> main` integration PR is owned by the integration/release sequence, not this TODO.

### Commit discipline

- Stage named files; never use `git add -A` blindly.
- No generated `target/` output.
- No attribution/branding footer.
- No history rewrite or force push.
- Every commit is green under its milestone validation gate.
- Observe the failing test locally before implementing a behavior change, but do not commit a knowingly red tree.
- Check off the corresponding TODO items in the same commit that completes them.

---

## 6. Validation policy

### Focused loop

During a milestone, run the narrowest relevant checks repeatedly:

```bash
cargo fmt --check
cargo test -p <affected-crate> <targeted-test-filter>
cargo clippy -p <affected-crate> --all-targets -- -D warnings
```

Use one positional `cargo test` filter per invocation.

### Commit gate

Before every planned commit:

```bash
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
git diff --check
```

While Allium remains in Tier 0, it must stay valid. Documentation-changing commits also run:

```bash
./scripts/build-docs.sh
```

### Final gate

Before marking the PR ready:

```bash
cargo fmt --check
cargo test --workspace -- --test-threads=1
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
./scripts/build-docs.sh
allium check layer/allium
allium analyse layer/allium
patina spec check perf-phase-0 --json
git diff --check
```

The Perf command verifies this unrelated phase did not corrupt the paused spec; it does not authorize a profiling rerun or require Perf Phase 0 completion.

### Failure protocol

- Deterministic failures are fixed before committing.
- Record unexpected failure output verbatim under `## Flake log` before rerunning.
- A non-reproducing failure may be rerun up to five times in isolation; retain the original output and disposition.
- Never delete or weaken an assertion merely because migrating from a shell fixture is difficult.
- If a WASM fixture cannot express required test behavior, improve the fixture rather than restoring process-backed Child execution.

---

## 7. Milestones and commit points

## M0 — Checkpoint the documentation foundation

**Branch:** `patina`

- [x] Re-run `./scripts/build-docs.sh` and current documentation validations.
- [x] Confirm generated output remains ignored.
- [x] Update the active session artifact with the documentation work and Option C decision.
- [x] Commit all current mdBook, rustdoc, open-trace, documentation-copy, CI, roadmap, and session changes except this Option C TODO.
- [x] Use commit message:

```text
docs: establish mdBook and open trace foundation
```

- [x] Push without force:

```bash
git push origin patina
```

- [x] Confirm `git rev-parse HEAD` equals `git rev-parse origin/patina`.

**Push point P0:** Durable integration checkpoint. No PR is opened here because `patina` is the integration base.

---

## M1 — Start the autonomous Option C branch

- [x] Create the branch from the pushed P0 commit:

```bash
git switch -c feat/wasm-only-local-child
```

- [x] Re-run the process/JVM impact inventory with `rg`.
- [x] Record the exact production/test file inventory in this TODO under close-out evidence.
- [x] Commit this TODO only:

```text
spec(runtime): plan WASM-only local Children
```

- [x] Push the branch:

```bash
git push -u origin feat/wasm-only-local-child
```

- [x] Create a draft PR into `patina` using a body file:

```bash
gh pr create \
  --draft \
  --base patina \
  --head feat/wasm-only-local-child \
  --title "refactor(runtime): make local Children WASM-only" \
  --body-file /tmp/wasm-only-local-child-pr.md
```

- [x] Verify rendered Markdown:

```bash
gh pr view --json number,title,body,url --jq '.body'
```

**Commit C1:** `spec(runtime): plan WASM-only local Children`

**PR point PR-DRAFT:** Reviewable plan and inventory are public before code changes.

---

## M2 — Add a real reusable WASM test Child

Create a purpose-built test component that replaces shell-script behavior without weakening tests.

Required behavior:

- a stable WIT-shaped echo/transform operation;
- output depends on input bytes or JSON rather than returning a constant;
- no host imports unless a test explicitly supplies an authorized Toy;
- deterministic output;
- bounded input/output compatible with existing payload caps;
- committed source or WAT plus reproducible build instructions;
- committed component bytes, manifest, and required digest sidecars;
- verification test that regenerated bytes/receipts match the committed fixture.

Preferred location:

```text
crates/mct-daemon/tests/fixtures/mct-test-echo-0.1.0/
```

- [x] Add the fixture and reproducibility check.
- [x] Add focused loader/invocation proof through the real WASM runtime.
- [x] Do not add a generic fixture framework or production public API for tests.
- [x] Commit:

```text
test(runtime): add deterministic WASM echo Child fixture
```

**Commit C2:** Fixture and proof only.

**Push point P1:** Push and update the draft PR validation ledger.

---

## M3 — Migrate process-backed behavioral tests to WASM

Migrate tests before removing production code so coverage remains legible.

### M3.1 — Execution, payload, and idempotency

- [x] Replace shell Children in resident execution tests.
- [x] Replace payload-dependent shell Children; retain proof that output depends on request input.
- [x] Replace counting/idempotency shell behavior with deterministic WASM-visible evidence or an adapter-owned test counter that does not become product authority.
- [x] Preserve exact payload digest/size and no-ledger-payload assertions.
- [x] Preserve revision-guard and no-effect-on-denial assertions.
- [x] Commit:

```text
test(resident): move execution proofs to WASM Children
```

**Commit C3.1**

### M3.2 — Serving, control, ingress, and forwarding

- [x] Replace process fixtures in serving and control tests.
- [x] Replace standalone and two-Mother process fixtures.
- [x] Preserve hello, binding, route, forwarding, durability, shutdown, and caller-safe result coverage.
- [x] Ensure remote tests prove a remote Mother boundary, not a remote process substrate.
- [x] Commit:

```text
test(resident): move peer and control proofs to WASM Children
```

**Commit C3.2**

### M3.3 — Test inventory closure

- [x] Search all non-historical tests for process-backed Child helpers.
- [x] Remove dead shell Child writers.
- [x] Retain subprocess tests only for explicitly trusted Mother adapters.
- [x] Record before/after test counts and explain any intentional replacement.
- [x] Commit only if cleanup is substantial and independently reviewable (folded into C3.2):

```text
test(runtime): remove retired process Child fixtures
```

**Optional commit C3.3**

**Push point P2:** All meaningful behavior has WASM coverage before production deletion.

---

## M4 — Encode the current execution boundary in Rust types

The type design must distinguish historical records from currently executable local Child state.

Required properties:

- only WASM can construct a current local execution target;
- historical `process` and `jvm_child` values remain decodeable where compatibility requires;
- conversion from historical runtime shape to current local execution is fallible and typed;
- route/execution plans cannot carry an unconfined local process;
- trusted subprocess adapters cannot accept `AuthorizedChildInvocation` as permission for arbitrary process behavior;
- remote execution is represented by a remote Mother boundary, not a local substrate claim.

Implementation guidance:

```rust
pub enum LocalChildRuntime {
    WasmComponent,
}

pub enum ExecutionTarget {
    Local(LocalWasmChild),
    Remote(RemoteChild),
}
```

Names may differ after reading the code, but the representable states may not. Do not retain a multi-variant abstraction merely for aesthetic symmetry.

Compatibility guidance:

- A serialized/history enum may retain `process` and `jvm_child`.
- It must not be the type consumed by current local execution.
- Add explicit tests for reading old values and refusing executable conversion.
- Preserve old observation display and state inspection.

- [x] Introduce the narrow current type(s).
- [x] Add private or checked constructors.
- [x] Update candidate and execution plans to consume only the narrow type.
- [x] Add negative conversion and construction tests.
- [x] Avoid changing unrelated wire records unless necessary.
- [x] Commit:

```text
refactor(kernel): make local Child execution WASM-only
```

**Commit C4**

**Push point P3:** Type-level boundary is independently reviewable.

---

## M5 — Remove process/JVM Child product surfaces

### CLI and standalone ingress

- [x] Remove `process call` dispatch, implementation, and help text.
- [x] Remove `iroh serve-process` dispatch, implementation, and help text.
- [x] Remove parser acceptance that creates a current local process/JVM Child runtime.
- [x] Keep `jvm call-json` only if inspection proves it is an external authenticated ingress into a WASM/remote Child; rename or document it if its current name implies JVM Child execution.
- [x] Unknown retired commands fail with ordinary unknown/unsupported command behavior and perform no state, ledger, spawn, or network effect.

### Child loading and ingress mode

- [x] Reject legacy handle-only ingress as a current local Child shape.
- [x] Keep WIT-only and hybrid only when both execute through the WASM component runtime.
- [x] Do not silently reinterpret a missing ingress mode as process execution.
- [x] Existing supported fixtures (`slate-manager`, `folder-watch-actor`, `watch-null-sink`) remain loadable and callable.

### Public library surface

- [x] Remove `MctProcessChildHarness` and related Child-named public exports.
- [x] Delete `process.rs` if no trusted non-Child consumer remains.
- [x] If a trusted subprocess helper remains, give it a Mother-adapter name, restrict its visibility, and ensure it does not consume Child authority types.

- [x] Add CLI/API absence tests.
- [x] Add no-spawn marker tests for retired inputs.
- [x] Commit:

```text
refactor(daemon): retire process-backed local Children
```

**Commit C5**

**Push point P4:** Public product removal is visible in the draft PR.

---

## M6 — Remove resident local process dispatch and routing

- [x] Delete `execute_resident_process_child` and all resident selection branches that reach it.
- [x] Remove handle-to-`RuntimeKind::Process` candidate mapping.
- [x] Ensure every local candidate is backed by a verified loaded WASM component.
- [x] Ensure local `RouteTaken` can report only the current WASM local runtime.
- [x] Keep remote route observations substrate-neutral at the local authority boundary.
- [x] Preserve authority filtering before ranking.
- [x] Preserve effect-boundary revision checks and ordered observation durability.
- [x] Add a regression proving no persisted/synthetic process artifact becomes a local candidate.
- [x] Add a regression proving no process/JVM historical value becomes a ready/routable local Child.
- [x] Commit (folded into C5 because removing the public harness required removing its only resident consumer):

```text
refactor(resident): route local calls only to WASM Children
```

**Commit C6**

**Push point P5:** No production local route reaches a host process.

---

## M7 — Historical state, wire, and observation compatibility

- [x] Inventory every persisted SQLite column and JSON/ledger field carrying runtime kind/shape.
- [x] Add fixtures for historical `process` and `jvm_child` values.
- [x] Prove historical rows remain listable/inspectable.
- [x] Prove historical observations remain deserializeable and renderable.
- [x] Prove current approval, assignment, candidate sourcing, and execution refuse them.
- [x] Do not mutate historical facts into WASM.
- [x] Do not delete rows or observations automatically.
- [x] If a schema migration is required, make it idempotent and preserve exact historical identity (no migration was required).
- [x] Ensure current writes cannot create new process/JVM local Child runtime facts.
- [x] Commit:

```text
fix(state): keep retired runtime history readable and inert
```

**Commit C7**

**Push point P6:** Compatibility behavior is independently reviewable.

---

## M8 — Align law, documentation, release notes, and close-out

### Minimal Allium alignment

- [ ] Remove current-law statements that bless local process/JVM Child execution.
- [ ] Narrow current `ComponentArtifact.runtime_shape` law to WASM local execution and remote Mother routing, while preserving historical evidence semantics in prose where needed.
- [ ] State native/JVM integration as external caller, remote Mother, or trusted Mother adapter.
- [ ] Do not add new entities, actors, surfaces, or generalized confinement machinery.
- [ ] Run Allium check/analyse.

### Documentation

- [ ] Update README and mdBook from “current limitation” to the final product statement: local Children are WASM components.
- [ ] Update Core concepts, Child development, Integrations, Architecture, and Operations where relevant.
- [ ] Update `layer/core/what-is-mct.md` and the product roadmap.
- [ ] Update CLI/reference material so retired commands do not appear.
- [ ] Add a CHANGELOG entry naming the intentional pre-GA removal.
- [ ] Explain that native/JVM systems remain valid external integrations, not local Child runtimes.
- [ ] Rebuild `/docs/` and `/api/`.

### Close-out audits

- [ ] `rg` proves no production `Command::spawn` path is named or typed as Child execution.
- [ ] `rg` proves no current local candidate can be `Process` or `JvmChild`.
- [ ] Public exports contain no process Child harness.
- [ ] Help output contains no `process call` or `iroh serve-process`.
- [ ] Historical compatibility tests are green.
- [ ] Record final test counts and changed-surface inventory below.
- [ ] Check every completed TODO item.
- [ ] Commit:

```text
docs(runtime): close WASM-only local Child transition
```

**Commit C8**

**Push point P7:** Push final implementation and update the PR body from actual diff/commits.

---

## 8. PR ledger and merge procedure

At PR-DRAFT, create `/tmp/wasm-only-local-child-pr.md` with:

```markdown
## Summary
- Make WASM components the only locally executable Child substrate.
- Retire process-backed local Child CLI, routing, and resident execution.
- Preserve historical process/JVM state and observations as readable but inert evidence.
- Replace shell-based Child fixtures with deterministic WASM components.

## Why / Design
- Running code is not authority.
- Admission before `Command::spawn` is not OS confinement.
- Native and JVM software remains an external integration or trusted Mother adapter, not a local Child.
- Historical compatibility is separated from current executable types.

## Changes
- Plan/spec: link the autonomous TODO and settled decisions.
- Fixtures/tests: describe WASM fixture and migrated coverage.
- Kernel: describe narrow current runtime types and checked conversion.
- Daemon: describe removed CLI, routing, dispatch, and exports.
- State/history: describe compatibility behavior.
- Documentation/law: describe Allium, mdBook, README, roadmap, and changelog updates.

## Validation
- List exact commands and final results; do not claim checks not run.

## Compatibility / Operations
- Intentional pre-GA removal of process-backed local Child commands/APIs.
- Existing historical process/JVM observations and rows remain inspectable but cannot execute.
- Existing supported WASM fixtures remain supported.
- No database or ledger history is deleted.

## Follow-ups
- Allium retirement remains a separate governance decision.
- Native/JVM external integration guides may be expanded separately.
```

After every push point:

- [ ] Refresh the body from the actual branch, not this template alone.
- [ ] Add commits and exact validation completed so far.
- [ ] Keep known failures/follow-ups explicit.
- [ ] Verify no literal `\n` appears in rendered prose.

Before ready-for-review:

```bash
gh pr view <number> --json title,body,statusCheckRollup,commits,files --jq '.body'
gh pr ready <number>
```

Wait for required CI. Inspect checks:

```bash
gh pr checks <number> --watch
```

If CI fails, fix on the feature branch, run the local gate, push, and update the ledger. Do not merge with failing or pending required checks.

Merge only when:

- all required CI is green;
- final local gate is recorded;
- PR body matches final branch state;
- compatibility and follow-ups are explicit;
- the diff contains no unrelated Perf Phase 0 change beyond the inherited base.

Merge preserving commits:

```bash
gh pr merge <number> --merge --delete-branch
```

Then:

```bash
git switch patina
git pull --ff-only origin patina
```

Record the merged PR URL and merge commit in the active session artifact.

**PR point PR-READY:** M8 complete, final gate green, body repaired.

**PR point PR-MERGED:** Feature merged into `patina`; no direct `main` PR from this task.

---

## 9. Autonomous resolution rules

The agent does not request operator involvement for ordinary implementation choices covered here.

When alternatives arise:

1. Choose fewer product concepts and fewer runtime variants.
2. Prefer deletion over deprecation shims for current product surfaces.
3. Preserve durable historical readability at explicit adapter boundaries.
4. Prefer private newtypes and checked constructors over comments and boolean flags.
5. Prefer one concrete WASM path over a trait with one implementation.
6. Preserve tests by improving WASM fixtures rather than retaining process execution.
7. Keep trusted Mother subprocesses explicitly outside Child authority types.
8. Fail closed rather than infer a replacement runtime.
9. Keep commits concern-specific and green.
10. Defer unrelated cleanup to Follow-ups instead of expanding scope.

### Hard stop conditions

Stop without destructive action only if:

- publishing would disclose a credential or restricted third-party material;
- `origin/patina` has diverged and cannot be reconciled by an ordinary pull/rebase without rewriting published history;
- preserving historical state requires deleting or silently rewriting durable facts;
- GitHub authorization no longer permits the authorized push/PR workflow;
- an unrelated working-tree change appears that cannot be attributed to this session.

For code complexity, test migration difficulty, compile errors, CI failures, or documentation drift, continue autonomously under this TODO.

---

## 10. Definition of done

- [ ] Local executable Child types represent only WASM.
- [ ] Native/JVM processes cannot be approved, assigned, made ready, routed, or executed as local Children.
- [ ] `process call` is removed.
- [ ] `iroh serve-process` is removed.
- [ ] Resident process Child dispatch is removed.
- [ ] Process Child public library exports are removed.
- [ ] Handle-only ingress cannot become local execution.
- [ ] Existing supported WASM fixtures still load and execute.
- [ ] Process-backed behavioral tests are replaced with WASM tests without coverage loss.
- [ ] Historical process/JVM records remain readable and inert.
- [ ] Current code cannot write new process/JVM local execution facts.
- [ ] Allium is minimally aligned and not expanded.
- [ ] README, mdBook, core narrative, roadmap, help, and changelog agree.
- [ ] Every planned commit passed its gate.
- [ ] Final validation passed and is recorded in the PR.
- [ ] Draft PR was updated into a reviewer-ready ledger.
- [ ] Required CI passed.
- [ ] PR merged into `patina` with commit history preserved.
- [ ] Feature branch deleted and local `patina` updated.
- [ ] Session artifact records commits, PR, validation, compatibility, and follow-ups.

---

## 11. Close-out evidence

Populate during execution.

### Baseline inventory

- Commit: `7abc88583eda4752837a648ae5e02cd8297624a3` (`origin/patina` at M1 branch creation).
- Production files (18): `crates/mct-daemon/src/{children.rs,daemon/cli_admin.rs,daemon/cli_runtime.rs,daemon/ingress.rs,daemon/resident/candidates.rs,daemon/resident/decision.rs,daemon/resident/execution.rs,daemon/resident/forwarding.rs,federation.rs,lib.rs,main.rs,process.rs,state.rs}`, `crates/mct-iroh/src/lib.rs`, and `crates/mct-kernel/src/{call/mod.rs,child.rs,observation.rs,route.rs}`.
- Process-fixture/test-bearing files (18): `crates/mct-daemon/src/{daemon/cli_admin.rs,daemon/cli_runtime.rs,daemon/control.rs,daemon/ingress.rs,daemon/resident/candidates.rs,daemon/resident/decision.rs,daemon/resident/execution.rs,daemon/resident/forwarding.rs,daemon/resident/idempotency.rs,daemon/resident/payload.rs,daemon/resident/pipeline.rs,daemon/resident/serving.rs,daemon/resident/trigger_scheduler.rs,main.rs,process.rs,supervisor.rs,toy.rs}` and `crates/mct-daemon/tests/release_archive.rs`.
- Public commands: `process call` and `iroh serve-process` are process-Child execution surfaces; `jvm call-json` is subject to the M5 external-ingress semantic check.
- Public library items: `MctProcessChildHarness`, `MctProcessChildError`, `MctProcessChildInvocationIds`, and `MctProcessChildInvocationReport` are exported from `mct-daemon`.
- Persisted runtime fields: `component_artifacts.runtime_shape`, `remote_callable_surfaces.runtime_kind`, and `runtime_runs.runtime_kind`; runtime kinds also occur in serialized observations and peer policy views.
- Baseline test counts: 514 workspace tests discovered with `cargo test --workspace -- --list`; 102 selected process/runtime symbol occurrences across Rust; 112 shell/process fixture matches in the focused test inventory.

### Final inventory

- Merge commit:
- PR URL:
- Removed commands:
- Removed public items:
- Remaining trusted subprocess surfaces:
- Historical compatibility proofs:
- Final test counts:
- Documentation URL/check:

### Commit ledger

| Commit | Milestone | Purpose | Validation |
| --- | --- | --- | --- |
| `633cbcb` | Gate repair | Patch `h2` for RUSTSEC-2026-0258 | Tier 0, audit, workspace tests, clippy |
| `d049a8a` | C1 | Plan and inventory | Tier 0, docs build, diff check |
| `bdfacc6` | C2 | WASM test fixture | Tier 0, focused fixture test, clippy |
| `579bded` | C3.1 | Execution/payload/idempotency test migration | Tier 0, clippy, focused and full binary tests |
| `6a4a6e9` | C3.2 | Peer/control test migration | Tier 0, clippy, serialized workspace tests |
| `3f58c5c` | C4 | Current execution type boundary | Workspace tests, Tier 0, clippy |
| `d95ddb0` | C5/C6 | Product surface and resident dispatch retirement | Tier 0, CLI no-spawn tests, handle rejection tests, clippy |
| pending | C7 | Historical compatibility | pending |
| pending | C8 | Law/docs/release close-out | pending |

### Flake log

None at plan creation.

### Follow-ups

- Decide whether to reduce or retire Allium based on demonstrated value, separately from Option C.
- Expand native/JVM external-integration guidance only from concrete consumer requirements.
- Open the eventual `patina -> main` integration PR only under the integration/release plan that also disposes the paused Perf Phase 0 state.

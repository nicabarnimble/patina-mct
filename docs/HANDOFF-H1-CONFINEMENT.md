# H1 Git Toy Confinement — Continuation Handoff

**Updated 2026-10-24. Branch: `experiment/git-toy-confinement` (based on `edd947a`). macOS confinement promoted locally; H1 remains open for Linux confinement and independent security review.**

## Architectural Boundaries (Keep Invariant)

1. **Preserve ordinary repositories:** No Mother-managed workspace requirement, enrollment, or `.git` relocation.
2. **Treat `.git` metadata as untrusted:** Confine the native Git invocation, not mutable config selected by a preflight scan.
3. **No unconfined fallback:** Unsupported platforms and confinement failures fail closed.
4. **Keep tests strict:** Preserve positive controls, failure assertions, and durable-output redaction checks.
5. **Keep H1 open:** macOS promotion and local validation are not Linux confinement, independent security review, or release acceptance.

## Current State

- **Production macOS backend:** `crates/mct-daemon/src/toy/confinement.rs` is compiled under `#[cfg(target_os = "macos")]`. `MctToyBackend::GitCommand` selects it directly; no registry toggle or production raw-Git builder remains.
- **Confinement:** Host `/usr/bin/sandbox-exec`, deny-default policy, parameterized `GIT`/`REPO`/`ANCESTOR_i` paths, cleared environment, explicit `--git-dir`/`--work-tree`, helper-suppressing overrides, no process-fork/network/general-exec grants. Failure never retries unconfined.
- **Supported installations:** Homebrew arm64 `/opt/homebrew/bin/git`, otherwise Command Line Tools `/Library/Developer/CommandLineTools/usr/bin/git`; canonicalize, never use ambient PATH. These are installation paths, not a cryptographic version pin. Unsupported installations/policy-runtime combinations fail closed.
- **Non-macOS:** Explicit `git confinement unavailable on this platform` error, no success output, and a redacted `ToyCallFailed` observation before operation dispatch, including no-ops. Linux confinement remains unimplemented.
- **Repository compatibility:** Ordinary self-contained `.git` directories only. Gitfiles, symlinked `.git`, and linked worktrees fail with `git repository layout unsupported`. No current grant enables an exception. Layout checking is not the security boundary; OS enforcement still applies after metadata mutation.
- **Tests:** All **13 `git_confinement` tests** (10 adapter + 3 WASM) and **32 `toy::tests` tests** pass on macOS arm64 through production dispatch. Raw Git exists only in disposable fixture setup/independent positive controls.
- **Unsupported-platform tests:** `crates/mct-daemon/src/toy/tests/git_unsupported.rs` covers every operation, redaction, and no-op failure; the direct WIT Git-import test asserts failure/no tag on non-macOS. These branches are not executed by the macOS run; no native non-macOS target is installed here.
- **WASM fixture:** Unchanged `crates/mct-daemon/tests/fixtures/h1-git-child/child.wasm`, SHA-256 `bba25cd1fa6c437a41f98e7f2897c0c25f1077d5c520f3331abdcfa8ce6263ca`. See `PROVENANCE.md` for the locked/offline build.

See [H1 evidence](H1-git-toy-evidence.md#production-macos-confinement) for the full contract, test coverage, and historical evidence.

## Task Status

1. **Completed: fail-closed upstream queries.**
   - Removed failure-to-zero coercion. Both operations require born, attached HEAD; unborn/detached HEAD fails.
   - Only successfully read effective config with no `branch.<current>.merge` setting establishes no upstream and permits `0`/`false`.
   - Broken configured upstreams, every failed command, policy denial, malformed counts, and overflow propagate as failure. One strict `rev-list --left-right --count HEAD...@{upstream}` computes both counts.
   - Four shared tests plus four confinement regressions include denied history traversal after successful HEAD/upstream-tip verification.

2. **Completed: promote macOS and decide compatibility.**
   - Removed the test-only backend toggle and raw production fallback.
   - Non-macOS platforms fail explicitly until OS-level confinement is implemented.
   - Linked worktrees/external metadata are refused; no implicit external grant or repository migration was introduced.
   - Production tests include a real linked worktree and `.git` redirection after command construction. The composed WASM positive control now runs raw Git independently on the exact Child-written payload, never through an unconfined adapter.

3. **Completed: requested promotion validation runs (R1 failure retained, not resolved).**
   - All 13 confinement / 32 Toy tests passed; the latest full workspace rerun passed 751 tests with 2 pre-existing ignored.
   - Warnings-denied workspace/all-target Clippy, fmt/diff checks, tier-0 specs, and docs build passed.
   - One full revalidation reproduced the recorded R1 readiness failure. The isolated and full unchanged-source reruns passed; this is not an R1 repair or a clean first-pass claim. See evidence below.

## Verification Commands

```sh
export CARGO_BUILD_JOBS=8 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_NET_OFFLINE=true

cargo test --locked --offline -p mct-daemon --lib git_confinement -- --nocapture --test-threads=1
cargo test --locked --offline -p mct-daemon --lib toy::tests -- --nocapture --test-threads=1
MCT_RESTART_KEEP=1 MCT_FLEET_KEEP=1 cargo test --workspace --locked --offline --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all --check && git diff --check
bash scripts/ci-tier0.sh specs
bash scripts/build-docs.sh
```

## Validation History

Before Task 1, the original 9 confinement tests passed. Task 1 then passed 32 Toy tests, all 13 confinement tests, and full workspace tests (**751 passed, 0 failed, 2 pre-existing ignored**), plus Clippy/fmt/diff checks. Its log is `/tmp/mct-h1-task1-evidence/workspace-tests.log`; those results predate promotion.

Promotion validation is recorded in [H1 evidence](H1-git-toy-evidence.md#promotion-validation):

- Initial promoted workspace run: **751 passed, 0 failed, 2 ignored**.
- Revalidation after closing the linked-worktree no-op case: **750 passed, 1 failed, 2 ignored**. The failure was `mother_survives_sigkill_mid_run_with_active_children`, seed `0x6d637421`, iteration `1`, `AfterSomeChildrenFinish`, boot `1`: `resident control socket did not bind before readiness`.
- Failing fixture retained: `/var/folders/6h/329275913d1d3k1lfvvvryp40000gn/T/.tmpbSbHKw`.
- Without source changes, the isolated restart rerun passed all 3 iterations, followed by a full workspace rerun: **751 passed, 0 failed, 2 pre-existing ignored**.
- Clippy (`--workspace --all-targets --locked --offline -- -D warnings`), `cargo fmt --all --check`, `git diff --check`, tier-0 specs (17 self-tests), and docs build all passed.
- Local logs: `/tmp/mct-h1-promotion-evidence/` (`workspace-tests-final.log` preserves the failure; `restart-rerun.log` and `workspace-rerun.log` preserve the passing reruns).

R1/R2 remain open; reruns do not repair or erase the failure. H1 remains open for Linux confinement and independent security review.

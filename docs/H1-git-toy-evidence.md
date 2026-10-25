# H1 Git Toy: hardening and production macOS confinement

**2026-10-24 — macOS confinement promoted locally; H1 remains open for Linux confinement and independent security review. Not release acceptance.**

The first sections preserve the historical bounded-hardening/PR #84 evidence.
The current production backend and its validation are documented under
[Production macOS confinement](#production-macos-confinement).

Scope comes from [H1 in the work index](TODO-security-and-finish.md#h1--git-toy-can-cross-the-wasm-execution-boundary-high)
and the existing [safety boundaries](../layer/core/safety-boundaries.md).
A Git Toy grant does not grant arbitrary native process execution. No new
permission, authority source, ledger format, dependency or deployment is added.
The runtime baseline is `ebc0d2d`; `b84359a` preserves the work index and corrects
stale handoff instructions without changing runtime code.

## Reproduced and hardened

Tests in `crates/mct-daemon/src/toy/tests/git_boundary.rs` exercise the real
`MctToyAdapterRegistry::call_authorized_toy_at` path with a kernel-minted grant.
The adversarial input is repository metadata a filesystem-authorized Child can
write; the tests write those bytes from the fixture rather than through WASM.
They are adapter-level evidence, **not** an end-to-end malicious WASM proof.

Each fixture first runs ordinary Git through a separate command builder to
prove the configured helper actually executes. Its only effect is a sentinel
write to a private sibling temporary directory outside the repository. It does
not read secrets, connect to a network, or mutate a real project. The marker is
removed before invoking the Git Toy. No process-global environment is mutated.

Three regression tests failed on the unchanged runtime:

```text
running 3 tests
h1_git_toy_never_executes_default_or_redirected_hooks ... FAILED
h1_git_toy_never_executes_repository_commit_or_tag_signers ... FAILED
h1_git_toy_never_executes_repository_fsmonitor ... FAILED

Git Toy executed repository-selected native code: h1-hooks
Git Toy executed repository-selected native code: h1-signers
Git Toy executed repository-selected native code: h1-fsmonitor

test result: FAILED. 0 passed; 3 failed; 0 ignored
```

The failure summary above is condensed, not a verbatim complete log. The first
failure in each loop stops that test: the baseline demonstrates default hooks,
commit signing and fsmonitor. On the fixed path, all cases execute, additionally
covering redirected hooks and annotated-tag signing with positive raw-Git controls.

The shared Git command builder now overrides:

- `core.hooksPath=/dev/null`;
- `core.fsmonitor=false`;
- `commit.gpgSign=false`, `tag.gpgSign=false`, `tag.forceSignAnnotated=false`;
- `log.showSignature=false`;
- `gc.auto=0`, `maintenance.auto=false`;
- paging via `--no-pager`.

Hook, fsmonitor and commit/tag signer suppression has behavioral regression
coverage. The remaining switches are defense in depth, not independently
reproduced boundary regressions. Command-line configuration takes precedence over
repository configuration. Existing ref/path rejection and effect-time grant
checks remain unchanged.

Ordinary file edits, Toy staging and Toy commit still succeed, and the resulting
HEAD contains the edited bytes. Tags still succeed. Git signatures selected by
ambient/repository configuration are intentionally not performed by this Toy;
this does not change MCT's separate hybrid authority signatures.

The stderr-redaction test now uses a nonexistent ref containing a marker. It
proves Git emits that marker and the Toy excludes it from caller-safe and durable
output, without requiring execution of a pre-commit hook to pass the test.

## Validation

Host: macOS arm64; Rust 1.96.0, Cargo 1.96.0, Git 2.56.0.
Rust checks used the existing locked dependency graph, offline, with:

```sh
export CARGO_BUILD_JOBS=8 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_NET_OFFLINE=true
cargo test --locked --offline -p mct-daemon --lib toy::tests -- --nocapture --test-threads=1
cargo test --locked --offline --workspace -- --test-threads=1
MCT_RESTART_KEEP=1 cargo test --locked --offline -p mct-daemon --test mother_restart \
  mother_survives_sigkill_mid_run_with_active_children -- --exact --nocapture --test-threads=1
MCT_RESTART_KEEP=1 MCT_FLEET_KEEP=1 cargo test --locked --offline --workspace \
  --no-fail-fast -- --test-threads=1
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
cargo fmt --check
bash scripts/ci-tier0.sh specs
```

| Check | Observed result |
| --- | --- |
| Toy tests | 18 passed, none ignored; includes all three new regressions |
| First workspace run | Failed in the previously recorded R1 restart-readiness failure; later binaries were not all run |
| Isolated restart rerun | 1 test / 3 iterations passed; successful fixtures retained |
| Full no-fail-fast workspace rerun | 734 passed, 2 pre-existing ignored; no source change between the failed run and reruns |
| Warnings-denied workspace/all-target Clippy | Passed |
| Formatting, diff whitespace | Passed after formatting the new code |
| Tier-0 spec/release-tooling/vocabulary checks | Passed, including 17 completion-runner self-tests |
| `bash scripts/build-docs.sh` (offline) | mdBook/link checking and warnings-denied Rust API documentation passed |

The R1 failure was seed `0x6d637421`, iteration `2`,
`MidRun { delay_ms: 718 }`, boot `1`:

```text
mct-daemon exited exit status: 1 before ready
Error: resident control socket did not bind before readiness
mother-restart seed=0x6d637421: 1/3 iterations failed: [2]
```

The **first failing fixture was not retained**: its default temporary cleanup
ran. Its complete stdout/stderr was retained. The isolated rerun used
`MCT_RESTART_KEEP=1` and retained three successful roots named in its log.
The later full rerun retained restart/fleet roots, but successful-test capture
means their paths are not all printed. No first-failure ledger is claimed.
These reruns do not identify or repair R1. R1/R2 remain open; this slice changes
no startup, timing, stale-authority, or fleet assertions. For that initial H1-only slice, a fresh advisory audit,
independent security review, and full release acceptance were not performed;
no dependency or lockfile changed. The subsequent shipping-gate update below
changes dependency versions and adds fresh audit evidence. The completion inventory still has eight
open obligations.

Raw local logs are at `/tmp/mct-h1-evidence/` (not portable release artifacts).
SHA-256 receipts for the initial records:

```text
d4318abc78da70bb8b5896dedafee3ba86f4fa585456ab5fbd436dab855b65e3  before.log
92eda6ad005b5262fc539b20d99ab8a48462aac885f584f42a7d9bcae9c394bb  toy-tests.log
6838148bf170876ee28054ae5d527e64692e5551cf1f10d1d09a78cb81c34725  workspace-tests.log
ebf1daad386f9d05284ff7b47e6a2ff6b8bb04fea89c8c637fcfcb9660b8a8f8  restart-rerun.log
9a35f09173db324b8368ec0ca877ec53eb4ac6d5e7dc68a7e69a771315ef9df1  workspace-rerun.log
```

## Shipping-gate follow-up: dependency security patches

PR #84's first CI run at `e237221` passed code tests, Clippy, formatting, docs,
authors and the ledger-open benchmark, but OSV failed. The same failures were
confirmed on unchanged `main` (`ebc0d2d`), already tracked by issue #83. This
was not waived merely because the H1 code did not introduce them.

OSV reported seven distinct advisories across the two lockfiles (14 findings),
including the critical Wasmtime aarch64 `br_table` issue RUSTSEC-2026-0379.
The narrowly scoped follow-up uses the reported fixed versions:

| Package | Root before | Fuzz before | Updated | Advisories |
| --- | --- | --- | --- | --- |
| Wasmtime/WASI family | 48.0.6 | 48.0.6 | 48.0.7 | RUSTSEC-2026-0378, RUSTSEC-2026-0379 |
| Tokio | 1.52.3 | 1.53.1 | 1.53.3 | RUSTSEC-2026-0386 |
| ctutils | 0.4.2 | 0.4.2 | 0.4.4 | RUSTSEC-2026-0385 |
| rustls-webpki | 0.103.15 | 0.103.15 | 0.103.16 | RUSTSEC-2026-0382 |
| webpki-roots | 1.0.7 | 1.0.9 | 1.0.10 | RUSTSEC-2026-0381 |
| webpki-root-certs | 1.0.7 | 1.0.9 | 1.0.10 | RUSTSEC-2026-0383 |

The workspace manifests raise the minimum Tokio and Wasmtime/WASI versions.
Both lockfiles retain their existing package membership and dependency edges;
38 package version/checksum records change in each, limited to the listed
packages and Wasmtime's coupled Cranelift/Pulley/Wiggle packages. No move to
Wasmtime 49, no Iroh refresh, no new packages, no new audit ignores or waivers.
This is security remediation, not the optional dependency proposal sweep.

Resolution used `cargo update --manifest-path <manifest> --package <package>
--precise <fixed-version>` on both manifests. The initial qualified WASI selector
was stale after Wasmtime updated its coupled family; that command failed and was
reissued by package name. Cargo's transient selection of Tokio 1.54.1 was narrowed
to 1.53.3. Unrelated fuzz-lock tempfile/getrandom/Windows dependency-edge churn
was removed; `cargo check --locked --manifest-path fuzz/Cargo.toml --bins`
validated the final lock. No dependency checksum was hand-invented.

Local revalidation on the patched graphs:

- `cargo audit` and `cargo audit --file fuzz/Cargo.lock`: passed using a freshly
  fetched database with 1,352 advisories and unchanged audit policy. Final
  `--no-fetch` rechecks also passed after locking the exact graphs.
- `MCT_RESTART_KEEP=1 MCT_FLEET_KEEP=1 cargo test --locked --workspace
  --no-fail-fast -- --test-threads=1`: 734 passed, two pre-existing ignored.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo check --locked --manifest-path fuzz/Cargo.toml --bins`: passed; this is
  compilation, not a claim of local sanitizer fuzz execution.
- `bash scripts/ci-tier0.sh specs`, `cargo fmt --check`, `git diff --check`: passed.
- `bash scripts/build-docs.sh`: product/link checks and Rust API docs passed.

The same build-job/debug environment as above was used. Package retrieval and
fresh advisory fetches were online. New logs use the `security-` prefix under
`/tmp/mct-h1-evidence/`. Final GitHub checks and bounded fuzz outcomes belong in
PR #84's ledger, not inferred from the earlier commit's results. The earlier R1
failure remains retained and unresolved. No release/deployment approval follows.

## Shipping-gate follow-up: hosted test runner disk budget

At dependency-patched head `b711321`, GitHub audit and OSV passed, as did
Clippy, formatting, docs, authors and the ledger benchmark. The Linux test job
failed in `supervisor_conflicts_refuse_before_launchd_or_endpoint_effects`
at `supervisor_lifecycle.rs:3403` while copying a fixture executable:

```text
called `Result::unwrap()` on an `Err` value:
Os { code: 28, kind: StorageFull, message: "No space left on device" }
test result: FAILED. 181 passed; 1 failed; 0 ignored
```

This is an observed infrastructure failure, not evidence that R1 recurred or
that the dependency fix failed a runtime assertion. The full job log is retained
at `/tmp/mct-h1-evidence/pr84-test-full.log`; the failed run remains linked in
PR #84. No passing rerun is claimed for that attempt.

Only the CI **test job** now sets `CARGO_PROFILE_DEV_DEBUG=0` and
`CARGO_PROFILE_TEST_DEBUG=0`, matching the successful local 734-test run. Debug
assertions, unoptimized execution, panic unwinding, test selection, serial harness,
timeouts and fuzz/sanitizer settings are unchanged. Disk usage is reported before
and after tests. No arbitrary hosted tools or files are deleted and no assertions
are skipped. The updated job must pass on GitHub before merge; a local pass alone
cannot certify the hosted runner budget.

## What still blocks H1 closure

1. Linux OS-level Git confinement is not implemented. All non-macOS Git Toy
   calls now fail closed rather than using an unconfined native Git backend.
2. Independent security review is still required, including the macOS policy,
   supported Git/runtime distributions, filesystem aliasing and mutation races.
   Local passing tests are not an exhaustive sandbox assurance claim.

The project-root WASI preopen still permits `.git` writes. This is intentional:
ordinary repositories are preserved and metadata remains untrusted. The native
Git interpreter's process/filesystem execution context is the boundary, not a
preflight config scan, repository enrollment, or relocated metadata. No H1 or
other release obligation is closed by this promotion.

## Production macOS confinement

The owner approved promotion on `experiment/git-toy-confinement`, based on
PR #84's merge `edd947a`. `MctToyBackend::GitCommand` now always selects the
confined macOS executor in `crates/mct-daemon/src/toy/confinement.rs`.
It is compiled under `#[cfg(target_os = "macos")]`, including production builds.
The test-only registry toggle and ambient production Git command builder are
removed. Raw Git remains only in disposable test setup and independent positive
controls; it cannot be selected as an adapter fallback.

### Platform and repository contract

- **macOS:** use host `/usr/bin/sandbox-exec` with a deny-default policy,
  parameterized `-D` paths, cleared environment, explicit `--git-dir` /
  `--work-tree`, and helper-suppressing `-c` overrides. No process-fork,
  general exec, network, Mach IPC, or tracing grants. Missing sandbox support,
  failed policy application, and denied Git commands fail without raw retry.
- **Git installation:** select `/opt/homebrew/bin/git` (Homebrew arm64), then
  `/Library/Developer/CommandLineTools/usr/bin/git` if the first is absent,
  canonicalizing the selected executable. Never search ambient PATH. Runtime
  read grants cover the named macOS/Homebrew library trees; selecting a repository
  that overlaps trusted runtime paths is refused. A selected Git installation
  that cannot operate under this policy fails closed, without retrying raw Git.
  This is a supported-installation contract, not a cryptographic version pin.
  Local evidence uses macOS arm64, Rust 1.96.0 and Git 2.56.0.
- **Non-macOS:** fail before operation dispatch, including otherwise successful
  no-ops, with `git confinement unavailable on this platform`, no success
  output, and a redacted `ToyCallFailed` observation. No native Git is started.
  The platform-specific test checks all 13 operations; it is not executed by
  the macOS test run. No native non-macOS target is installed on this host.
- **Repositories:** ordinary self-contained checkouts with an in-root `.git`
  directory only. Gitfiles, symlinked `.git`, and linked worktrees are refused
  with `git repository layout unsupported`. No current grant authorizes an
  exception; future support requires explicit metadata authority, not an
  implicit external-path grant. Parent discovery is disabled. The layout
  preflight is only a compatibility restriction: OS confinement remains active
  against redirection/config mutation after command construction.
- **Upstream queries:** a born, attached HEAD with successfully read config and
  no `branch.<current>.merge` setting returns `0` / `false`. Unborn/detached
  HEAD, unreadable metadata, broken configured upstreams, and failed history
  traversal fail closed. Both counts come from one strict
  `rev-list --left-right --count HEAD...@{upstream}`; invalid/overflowing
  output is an error, never a default count.

No repository migration, new grant, new dependency, deployment, or H1 closure
is introduced.

### Executable evidence

**13 confinement tests pass through the production registry, without a toggle:**

1. **Adapter confinement (`crates/mct-daemon/src/toy/tests/git_confinement.rs`,
   10 passed):**
   - `clean` and `process` helper suppression, with independent raw-Git
     positive controls and inside/outside markers. Optional filters use native
     Git's unfiltered-byte behavior; required filters fail closed.
   - All 13 Git Toy operations on an ordinary checkout.
   - OS enforcement after config mutation and metadata redirection following
     command construction.
   - Parent-discovery refusal, symlink/gitfile redirection refusal without
     external repository mutation, and real linked-worktree refusal.
   - SBPL parameters treat quoted/newline-containing paths as data.
   - Fail-closed queries on non-repositories, unreadable/policy-denied config,
     redirected metadata, and unborn HEAD.
   - A policy-denied common-ancestor object fails history traversal even after
     both HEAD and upstream tip verification succeed.
   - Durable-output redaction assertions remain enforced.

2. **Composed WASM -> Git Toy
   (`crates/mct-daemon/src/wasm/tests/git_confinement.rs`, 3 passed):**
   - The WASM Child writes filter configuration and calls the production adapter:
     neither filter starts a helper, both inside/outside markers stay absent,
     and `ToyCallFailed` is observed with paths/filter names redacted.
   - Independent raw Git then executes the exact WASM-written payload as a
     positive control. Its markers are removed and the WASM production path
     is exercised again. There is no unconfined adapter test toggle.
   - Ordinary authorized WASM edit -> stage -> commit still succeeds without
     metadata relocation or config rewriting.
   - Without filesystem delegation, the Child cannot modify repo config or
     attributes, and no Git Toy call starts.

The unchanged Child fixture is
`crates/mct-daemon/tests/fixtures/h1-git-child/child.wasm` (25,835 bytes,
SHA-256 `bba25cd1fa6c437a41f98e7f2897c0c25f1077d5c520f3331abdcfa8ce6263ca`);
source/build provenance is in its `PROVENANCE.md`.

All **32 Toy unit tests** pass, including the four shared upstream-query tests,
existing hook/signer/fsmonitor controls, and ref/path validation. The direct WIT
Git import test also uses production dispatch and asserts unsupported-platform
failure/no tag on non-macOS builds.

### Promotion validation

Validation used the locked graph offline, with `CARGO_BUILD_JOBS=8`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, and
`CARGO_NET_OFFLINE=true`. Full workspace tests used
`MCT_RESTART_KEEP=1 MCT_FLEET_KEEP=1`, `--no-fail-fast`, and
`--test-threads=1`.

| Check | Observed result |
| --- | --- |
| Production confinement tests | 13 passed (10 adapter + 3 WASM), no toggle |
| Toy unit tests | 32 passed |
| Initial promoted workspace run | 751 passed, 0 failed, 2 pre-existing ignored |
| Revalidation after also refusing linked-worktree no-ops | 750 passed, 1 failed, 2 pre-existing ignored; R1 readiness signature below |
| Isolated restart rerun | 1 test / 3 iterations passed |
| Full unchanged-source workspace rerun | 751 passed, 0 failed, 2 pre-existing ignored |
| Workspace/all-target Clippy, `-D warnings` | Passed |
| `cargo fmt --all --check`, `git diff --check` | Passed |
| `bash scripts/ci-tier0.sh specs` | Passed, including 17 completion-runner self-tests |
| `bash scripts/build-docs.sh` | Passed (mdBook/link checking and warnings-denied Rust API docs) |

The revalidation failure in
`mother_survives_sigkill_mid_run_with_active_children` matches the previously
recorded R1 control-socket readiness signature; these runs do not establish or
repair its root cause. Seed `0x6d637421`, iteration `1`,
`AfterSomeChildrenFinish`, boot `1`:

```text
mct-daemon exited exit status: 1 before ready
Error: resident control socket did not bind before readiness
mother-restart seed=0x6d637421: 1/3 iterations failed: [1]
```

The failing fixture is retained at
`/var/folders/6h/329275913d1d3k1lfvvvryp40000gn/T/.tmpbSbHKw`.
The failed full log, isolated rerun, successful full rerun, Clippy, specs, and
docs logs are under `/tmp/mct-h1-promotion-evidence/`. No source change,
assertion weakening, retry logic, or timing adjustment occurred between the
failed revalidation and the passing reruns. R1/R2 remain open; a rerun does not
erase the failure. H1 remains open for Linux confinement and independent review.

Local log SHA-256 receipts (not portable release artifacts):

```text
746220fb9d2e0e59d6dfb9d30cb27b1bae2ec2225531aa34eea447f09aa29e96  workspace-tests-final.log
06cb9d81810e43a603cf54fbaf18bf7bd6018c6ad898fc6c3c7bd049005b4bf4  restart-rerun.log
c9f1e28aa0cbf70562d106fee8551f62b972450b6572de197014ee3c0430cc36  workspace-rerun.log
```

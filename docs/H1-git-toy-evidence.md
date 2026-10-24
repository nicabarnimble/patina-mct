# H1 Git Toy: first bounded hardening slice

**2026-10-24 — H1 remains open. Not a sandbox fix or release acceptance.**

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
prove the installed payload actually executes. Its only effect is a sentinel
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
reproduced exploit closures. Command-line configuration takes precedence over
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
no startup, timing, stale-authority, or fleet assertions. A fresh advisory audit,
independent security review, and full release acceptance were not performed;
no dependency or lockfile changed. The completion inventory still has eight
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

## What still blocks H1 closure

1. The project-root WASI preopen still permits `.git` writes. This patch does
   not isolate Git metadata from the Child, other Children, or concurrent writers.
2. Arbitrary `filter.<name>.clean` / `filter.<name>.process` configuration and
   worktree attributes can still select native helpers during staging. Other
   repository/ambient helper paths have not been exhaustively reviewed.
3. No immutable/trusted metadata policy or race-safe Git execution context is
   installed. A pre-call scan of mutable config would not solve this: another
   writer can change it after validation. Removing selected environment variables
   or disabling hooks is not a substitute.
4. A composed WASM filesystem-write → Git Toy test and independent security
   review are still required before closing the finding.

Next slice: establish the trusted metadata/execution boundary, retaining normal
edit/stage/commit behavior. Explicitly cover Git discovery (`.git` directories,
gitfiles, linked worktrees, symlinks), nested repositories, worktree attributes,
filters/helpers and mutation races. Do not auto-migrate or rewrite existing
repositories, pretend metadata is immutable, or quietly change grant/durability
semantics. Bring back any actual compatibility/policy decision with the concrete
tradeoff. Keep H1 and the eight release obligations open until the full boundary
has executable evidence.

# Repository gates

The repository gates tell an operator whether this checkout still formats, lints, tests, and matches its release and vocabulary checks.

## Sub-features

- `gate-fmt` proves Rust formatting matches `cargo fmt --all --check`.
- `gate-clippy` proves `cargo clippy -- -D warnings` accepts the workspace.
- `gate-test` proves workspace tests pass with one test thread.
- `gate-specs` proves release consistency, comparative vocabulary, finish checks, and the local admin script tests.
- `gate-diff` proves `git diff --check` finds no whitespace errors in the unstaged diff.

## How to get to it (user POV)

- From the checkout, run `cargo fmt --all --check`.
- From the checkout, run `cargo clippy -- -D warnings`.
- From the checkout, run `cargo test -- --test-threads=1`.
- From the checkout, run `bash scripts/ci-tier0.sh specs`.
- From the checkout, run `git diff --check`.
- `./scripts/ci-tier0.sh` without a subcommand also runs formatting, specs, a stricter Clippy, tests, and `cargo audit`. That full script is a separate entry point from the five commands above.

## Driving it with verify-patina-mct

Preconditions:

- `VERIFY_PATINA_MCT_ROOT` points at this run's directory.
- The checkout is the tree you intend to judge. `git diff --check` sees only unstaged whitespace, so a clean result on a committed tree does not review an unstaged patch you have not saved.

- **Run the five gates.** Run `.cursor/skills/verify-patina-mct/scripts/verify-patina-mct.sh gates`. The helper prints `gates: <name> exit <code>` for `fmt`, `clippy`, `test`, `ci-tier0-specs`, and `git-diff-check`.
- **Read the exits.** A passing gate has exit `0` in `proof/gates-results.tsv`. The matching log is `proof/gates/<name>.log`.
- **Fmt.** Exit `0` means `cargo fmt --all --check` found no formatting drift.
- **Clippy.** Exit `0` means `cargo clippy -- -D warnings` reported no warning. This is the listed gate. `scripts/ci-tier0.sh` Clippy is stricter because it adds `--workspace --all-targets`. A green listed Clippy does not by itself prove the stricter command.
- **Tests.** Exit `0` means `cargo test -- --test-threads=1` passed. The thread flag is required so resident socket tests do not contend with each other.
- **Specs.** Exit `0` means `bash scripts/ci-tier0.sh specs` passed. That subcommand runs release version and tooling checks, comparative vocabulary, finish validation, and the local Python and Node admin script tests. It does not compile the Rust workspace tests.
- **Whitespace.** Exit `0` means `git diff --check` printed nothing and found no whitespace errors in the unstaged diff.
- **Proof.** Keep `proof/gates-results.tsv` and the five logs. Paste the exit line and the log tail into the report. Do not claim a gate passed when its log is missing.

## Gotchas

- `gates` keeps going after a failure, then exits nonzero if any check failed. Read every line, not only the helper's final status.
- `cargo test` without `-- --test-threads=1` can contend on sockets and fail for a reason the serialized run would not.
- `git diff --check` on a clean worktree exits `0` even when the branch diff has a problem. Run it while the patch is unstaged, or also run `git diff --check origin/main` when you need the branch.
- Specs do not prove the Clippy or test gates. Those are separate rows.
- A failed gate is a product or checkout finding. Do not weaken the script to make the row green.

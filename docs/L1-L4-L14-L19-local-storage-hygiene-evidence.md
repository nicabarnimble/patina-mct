# L1 / L4 / L14 / L19 — local storage hygiene evidence

## Checkpoint and scope

Branch: `experiment/git-toy-confinement`. L6/L16 is checkpointed as
`a187344aa2af4cb61e8bbcedb2d7e6a74a441ee2` (`fix(authority): bind manifest digests
and separate local data facts`), directly atop
`0cc4544aaf5b79bfbec1e1056bce97297bfbaf94`. That checkpoint contained the reviewed
32-file L6/L16 slice and left a clean working tree. This subsequent L1/L4/L14/L19
batch is **separately checkpointed** atop that commit; independent review remains open. No push or change to `main` is part
of this work.

This is local implementation/test evidence, not independent audit closure or
severity reassessment. The identifiers remain individually tracked in
[the security TODO](TODO-security-and-finish.md). D-57's authenticated-administration
cutover is unchanged. Filesystem ownership is not an administrative credential.
Required `BeforeEffect` observations remain awaited; no timeout, optional sink,
repair, or broader grant substitutes for required evidence.

## L1 — owner-private storage

`mct-observation::private_fs` is the shared Unix boundary used by config, SQLite,
and observation-ledger creation/reopening, including ledger recovery and segment
files. It uses descriptor-relative directory traversal and no-follow opens:

- New storage directories use `0700`; new regular files use `0600`, independent
  of a permissive umask. New directory entries and durable publications are
  synced. Production does not change process-wide umask.
- Existing storage leaves must have the current effective UID, exact private
  modes, and one hard link. Symlinks, non-regular files, unsafe owners/modes,
  and parent traversal deny rather than being silently chmod-repaired.
- Ancestors must be current-UID/root owned and not group/other writable. Storage
  alone permits root-owned sticky temporary ancestors; executable ancestry does
  not. The immediate storage directory must itself be current-UID owned `0700`.
- Config publication and ledger sidecar writes use random, exclusively created
  private temporary files and same-directory rename, rather than predictable
  `.tmp` files that could be followed or truncated. A post-rename sync failure
  remains an uncertain acknowledgement, not a rollback claim.
- SQLite checks the DB and `-wal`, `-shm`, and `-journal` paths before opening,
  uses `SQLITE_OPEN_NOFOLLOW`, checks pathname inode continuity, and rechecks
  sidecars after migration. Unsafe pre-existing sidecars reject before DB
  creation. Existing unsafe bytes/permissions are left untouched.

### SQLite lock-preservation constraint

SQLite uses process-associated POSIX locks. Opening and closing an auxiliary
file descriptor to a live DB or SHM inode can release those locks, even if the
extra descriptor is read-only. SQLite preflight therefore uses **no-follow
`statat` metadata**, not the ordinary private-file reader. A new DB inode is
created privately under a random name, synced and **closed before** no-replace
publication. Existing DB/SHM inodes are never opened/closed by the preflight.

The two-process regression holds a live write transaction, performs all
preflight checks, and proves an independent writer receives `DatabaseBusy`.
After rollback, the same probe succeeds. This is a compatibility requirement,
not an exemption from path validation.

## L4 — supervisor executable provenance checks

`supervisor_lifecycle.rs` validates an absolute executable path before
canonicalization can erase symlink evidence. The leaf must be a regular,
single-link executable owned by the current UID or root, without group/other
write or set-ID bits. Every ancestor must have trusted ownership and no
group/other write permission; even sticky writable ancestors are rejected.

The digest is read through the validated descriptor. Install performs the cheap
path validation before bootstrap and hashing after acquiring the exclusive
writer, then rechecks the selected digest before recording the installation.
Record verification and supervised-start checks validate executable paths again,
including the current executable before canonicalization. A byte-identical
replacement reached through a symlink or newly writable path does not pass merely
because its digest matches.

This does not make the executable immutable until `exec`, authenticate a same-UID
process, establish publisher/code-signing trust, or resolve the separately tracked
trusted lifecycle decision. The checks do not introduce an administration bypass.

## L14 — corrupt SQLite values are errors, not panics

`state.rs` replaces persisted-value constructor `expect`s with fallible row
conversion. `MctCorruptStateError`, wrapped in SQLite conversion errors, reports
invalid typed IDs, timestamps, encoded values, and boolean values without
printing their contents. Native SQLite type/range errors also propagate.
Negative unsigned fields/attempt counts and invalid metric-label JSON no longer
become clamped or guessed values. Pending-sequence allocation validates the raw
maximum before checked increment, rejecting negative, nonnumeric, or exhausted
sequences instead of SQL coercion/clamping; an actually empty table still starts
at one. Remaining `expect`s concern literal/generated nonempty identifiers, not
persisted row conversions.

The fixtures explicitly inject corruption, catch unwinding, assert typed errors,
and verify the corrupt cell is unchanged. Optional `NULL` timestamps and valid
rows remain readable. There is no destructive repair, blanket validation of all
free-form strings, or newly invented global SQLite quarantine protocol. These
errors propagate through existing startup/inspection error and denial paths;
unread corrupt rows are not claimed to have been proactively diagnosed.

## L19 — retained registry installation boundary

The installer is publicly exported and referenced by control mutation application
code; it was not treated as provably dead. D-57's live registry-mutation refusal
is unchanged; this work does not re-enable that ingress.

- Package names must be bounded ASCII basenames, starting with an alphanumeric
  character, containing only alphanumerics/`-`/`_`/`.`, and not ending in `.`.
  Empty/dot names, separators, traversal, absolute/drive-like forms, and names
  longer than 128 bytes reject. Names are not sanitized into another identity.
- The registry root and existing installation directory must be real private
  directories with trusted ancestry. The source directory is checked before
  canonicalization. Symlink roots/destinations and writable ancestors deny.
- Random staging/backup names replace predictable timestamp names. Selected
  members are read through trusted no-follow descriptors and copied to exclusive
  private outputs with a 64 MiB per-member limit (plus one overflow-detection byte). Relative paths cannot escape
  their verified source root or staging destination.
- The staged package is strictly reverified. Its artifact identity and manifest
  digest must match the source verification before publication. Discovery and
  sidecars do not themselves grant execution approval; the L16 bindings remain.

This is not a universal archive extractor, publisher-authentication mechanism,
or a proof of crash-atomic multi-rename replacement against same-UID interference.

## Deterministic regressions and controls

Twelve new tests cover the four findings:

| Finding | Tests |
| --- | --- |
| L1 | `l1_private_creation_under_permissive_umask`; `l1_symlinks_hardlinks_and_unsafe_modes_are_refused_without_mutation`; `l1_config_sqlite_and_sidecars_are_private_under_permissive_umask`; `l1_config_database_and_sidecar_links_refuse_without_touching_targets`; `l1_sqlite_metadata_preflight_preserves_live_posix_locks` |
| L4 | `l4_executable_leaf_and_ancestor_permissions_are_checked_before_hashing`; `l4_install_and_record_verification_refuse_untrusted_executable_paths` |
| L14 | `l14_corrupt_artifact_ids_and_json_are_errors_not_panics_or_repaired_rows`; `l14_optional_timestamps_booleans_and_negative_counters_fail_closed` |
| L19 | `l19_package_names_are_basenames_not_sanitized_paths`; `l19_installer_refuses_traversal_and_symlink_roots_or_destinations`; `l19_package_members_and_registry_ancestors_do_not_follow_symlinks` |

Umask-000 tests run in separate shell-launched processes, not by changing umask
inside a multithreaded test runner. They check actual directory, config, DB,
WAL/SHM, and ledger modes. UID policy predicates are tested without privileged
`chown`; live foreign-UID ownership changes are not claimed to have been tested.
Positive controls restore safe executable metadata, read valid rows, release
SQLite locks, and install a valid package. Existing supervised lifecycle,
unsupervised fleet/restart, ledger recovery/genesis, and control durability tests
remain in the workspace run.

### Development failures distinguished from final evidence

Early legacy fixtures created `0755` directories and were correctly refused by
the new storage contract. The test harness now provisions private fixtures in a
subshell; it does not relax production checks. Explicit `/tmp` test aliases were
replaced with canonical temporary roots. The former fixed-`.tmp` failure test
now injects a filesystem failure at the real apply boundary, after the awaited
decision, and still checks the durable failure observation.

An initial descriptor-based SQLite preflight caused fleet `disk I/O error`
failures by interfering with POSIX locks. It was replaced with metadata-only
checks and the dedicated subprocess regression above. Expensive early executable
hashing also disturbed the old contention test timing; path validation remains
early, while hashing now occurs under the acquired writer. These were development
regressions, not independently reproduced pristine-baseline exploits for L1/L4/
L14/L19. No pristine-baseline exploit run is claimed.

## Validation and receipts

Environment: macOS/aarch64, Rust 1.96.0. Logs:
`/tmp/mct-l1-l4-l14-l19-local-storage-evidence/`. Only `verified-*` receipts below
represent the final source; earlier development logs are not final results.

The workspace's legacy fixtures use ordinary filesystem constructors. Thus the
Tier-0 test function (`scripts/ci-tier0.sh test`) uses a **subshell** with umask
077 and canonical `TMPDIR`; a raw workspace invocation needs the same fixture
setup. Running those legacy fixtures under an arbitrary permissive umask is not
claimed to pass. The dedicated production-creation tests deliberately use 000.
Existing public/symlinked storage is rejected, not grandfathered in for tests.

```sh
export CARGO_BUILD_JOBS=8 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_NET_OFFLINE=true
export TMPDIR="$(cd "${TMPDIR:-/tmp}" && pwd -P)"
umask 077 # test fixture setup only, before launching test processes
for finding in l1_ l4_ l14_ l19_; do
  cargo test --workspace --locked --offline "$finding" -- --test-threads=1
done
MCT_ORIGIN_KEEP=1 MCT_FLEET_KEEP=1 MCT_RESTART_KEEP=1 \
  cargo test --workspace --locked --offline --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all --check
git diff --check
bash scripts/ci-tier0.sh specs
```

Final result: **834 workspace tests passed, 0 failed, 2 pre-existing ignored**.
Target totals (excluding subprocess re-executions and doc-test zeros):
258 + 197 + 1 + 7 + 3 + 2 + 2 + 56 + 177 + 131. All twelve focused regressions
passed (L1: 5; L4: 2; L14: 2; L19: 3). Clippy with warnings denied, formatting,
diff checks, and tier-0 specs passed; specs ran 17 structural/tooling tests.
All eight corresponding `verified-*.exit` files contain `0`.

| Log | SHA-256 |
| --- | --- |
| `verified-l1_tests.log` | `da268c78e3a73fac9deb26a3cea17801be7a5fb56a2593df816f448808112a53` |
| `verified-l4_tests.log` | `3dae18e04876cee7eaf344dbfe88de9b56a09a25021f872c639d5152b3ccbff3` |
| `verified-l14_tests.log` | `bf123f854c79844bb9de94a76aa8430e00ef5db2821913ed74103a9c20c97b4f` |
| `verified-l19_tests.log` | `700bbfba0573025e78da368246f69f6370a0729f5caf1554d9289b7628fa312e` |
| `verified-workspace-tests.log` | `33fac56f73617f87ef6283ed7ed0e35594ceaa31ab54ec9f266cc7ec31ed2089` |
| `verified-clippy.log` | `a2e4b9bf58ac79c376fe6b374d6109c4c82878750351ee59b689fd42a619e45b` |
| `verified-fmt.log` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `verified-specs.log` | `1c1c4585a4b053198cca0866c8bfb9b19643d20574ced665440124a64247b7dc` |

## Compatibility and remaining assurance

- Existing installations with non-private modes, hard-linked storage, symlinked
  roots, or unsafe ancestry now fail closed. Operator-reviewed migration must
  preserve original evidence and deliberately provision a real owner-private
  destination; startup does not silently chmod, follow aliases, delete corruption,
  or reinterpret it as virgin state. On macOS, use real paths such as
  `/private/tmp/...`, not the `/tmp` or `/var` aliases.
- Supervisor binaries installed through symlinks, set-ID executables, or writable
  ancestor trees require deliberate relocation to a trusted real path. A private
  leaf under a writable parent is not enough.
- The trust model excludes a malicious process already acting as the same UID
  or root. These are Unix DAC owner/mode/type/ancestry checks, not executable
  identity authentication or a complete ACL, network-filesystem, mount-namespace,
  resource-exhaustion, or OS compromise proof.
- Linux/other-platform execution, ACL inheritance, privileged cross-UID tests,
  adversarial same-UID rename races, lifecycle authorization, migration UX,
  severity reassessment, and independent review/formal closure remain open.

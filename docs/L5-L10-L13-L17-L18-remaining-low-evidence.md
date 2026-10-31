# L5 / L10 / L13 / L17 / L18 — remaining Low boundary evidence

## Checkpoint and scope

The authorized 24-file peer/forwarding/ledger batch is committed as
`4098d6843bd0c2e5a3c8724b339a391f8aec41cc`, directly atop
`1a189f52b35df58c98808cc888b6c97b671c9f7c`, with message
`fix(peer): scope hello capabilities, isolate peer execution IDs, and guard ledger authority`.
The tree was clean immediately afterward. This subsequent remaining-Low batch
stays **uncommitted** on `experiment/git-toy-confinement`; no push or main change.

This is local implementation/contract evidence, not independent audit closure,
severity reassessment, deployment approval, or completion of strict administration.
The [security TODO](TODO-security-and-finish.md), D-56 issuance ceilings and
[D-57 cutover](DECISIONS.md) remain authoritative. No grant, freshness rule,
mandatory observation, or durability acknowledgement is relaxed to pass tests.

## L5 — actual effective UID, not fabricated root

The retained offline administrative helper passed literal `0` into
`PreparedAdministrativeMutation::authority_mutation_request`. This method emits
an `OperatorDecision` source with `authenticated_principal_ref = os-uid:<uid>`.
The affected retained CLI paths are Toy authorization (`authorize-secret` and
`authorize-slate`) via `execute_cli_administrative_mutation`; this is not merely
a misleading display string. Other offline helpers do not call this builder
with a fabricated UID. The resident path already uses the authenticated UDS
owner's UID and is unchanged.

The new regression, run against the pre-fix helper on effective UID 501, failed:

```text
left:  "os-uid:0"
right: "os-uid:501"
```

The helper now obtains the effective UID from the shared Unix
`private_fs::current_uid` (`rustix::process::geteuid`), not environment variables,
caller JSON, or the configuration file's owner. POSIX effective-UID retrieval
has no unavailable/unknown fallback; no synthetic root value is substituted.
A genuinely root process may correctly be attributed UID 0. The test verifies
the persisted canonical source and unchanged historical prefix bytes.

This corrects retained-path attribution. It does **not** declare UID/offline
administration to satisfy D-57 or finish removing those paths during cutover.
Existing startup classification, writer acquisition, canonical commitment,
projection ordering and enrolled-profile refusals stay in force.

## L10 — uniqueness, snapshot freshness and epochs are distinct

The implementation's historical contract is a set of committed
`(signer_grant_id, nonce)` pairs, not a high-water counter. The comment claiming
strictly increasing nonces was wrong and is corrected. Ordinary epoch
establishment retains that set; replay reconstructs it from the complete chain.
A fresh mutation ID cannot reuse a committed nonce even after reopen/epoch
change. Exact same-intent mutation retry resolves the old fact without another
append. Lower **unused** nonce values remain legal; no D-56 ordering policy was
invented.

The v1 signature binds domain/version, Mother, ledger, nonce, canonical signer
grant (including keys/state) and command. Its envelope does **not** independently
bind an epoch or expiration. In particular, an unused legacy signature is not
proof of a fresh intent after an epoch change. That historical limitation is
explicit, not silently fixed by changing signed bytes or rewriting old facts.
Legacy verification must not be promoted into a current D-57 administrative path.

Current D-56 issuance signs its exact expected authority reference and state hash
inside the command. Deterministic tests use a strict-genesis/guardian fixture to
prove:

- a held, unused signed intent fails after ordinary reopen changes the epoch;
- two intents against the same snapshot cannot both commit after the first changes
  authority; a newly signed current snapshot can use the still-unused lower nonce;
- accepted nonces survive another reopen, changed IDs cannot reuse them, and exact
  retry appends nothing;
- a cryptographically valid legacy ticket command is refused in the enrolled
  strict profile rather than becoming a compatibility authorization path.

A separate historical legacy fixture proves nonce retention across epochs,
non-monotonic unique values, and signature binding to the signer grant and Mother.
Existing issuance/recovery tests continue covering revoked roots, historical grant
IDs, current ceilings, stale snapshots, clock checks and both signature halves.
No reproducible **committed-nonce** replay gap was found; this portion is contract
clarification and targeted proof, not a signature-format migration or a claim that
all legacy signed intents have current-epoch freshness.

## L13a — historical entry hashes authenticate typed representation

`entry_hash` is unchanged: clone the typed entry, clear its `entry_hash`, serialize
that struct with the historical serde layout, then BLAKE3. It is not a raw-file
byte hash and is not claimed to be a general JSON canonicalization standard.

The fixed fixture hash, measured before the production fix, remains:

```text
8bcddc16cc8514e497c45c1e97aa67d12cafc2878d3fbe4978a48e2137d941e1
```

The representation regression reorders outer object keys, changes whitespace and
Unicode escape spelling, and includes an ignored extension field. Those changes
preserve the interpreted typed entry/hash while changing the raw prefix digest.
A changed authenticated field fails verification without altering the file.
Historically ignored fields are **not authenticated claims** and must not be
interpreted by a new authority consumer without an explicit format contract.
Embedded `detail_ref` strings remain hashed string contents, not freely
reorderable inner JSON objects. No historical hash/signature version is changed.

## L13b — checkpoint mismatch cannot erase known continuity

Two new tests failed against the prior production loader: a valid shorter prefix
and a different, fully rechained valid history were both accepted after the raw
checkpoint mismatch evicted the cache and full verification started without its
anchor. Valid self-consistent hashes alone do not establish continuity.

The loader now retains the known typed head during fallback. Full scans compare
the entry at that sequence with its known hash and require it to exist. The same
check protects read/open/recovery paths while that process-local checkpoint is
known. A shortened file, an empty file, a valid rehashed fork, or a torn frame
that was previously known committed fails with typed `LedgerDiscontinuity`.
Repeated attempts cannot erase the remembered head; stale cache publication
cannot lower its sequence or replace a same-sequence hash. Cache clearing is
now test-only. Rejected files are left intact, not truncated, rehashed, chmodded,
or silently repaired. A genuine uncommitted tail *after* the known head retains
the existing preservation/recovery rules.

Equivalent outer JSON representations still pass full verification if they retain
the known typed hash and ancestry. Sequence gaps and malformed/broken chains
retain their existing rejection paths. Ordinary valid suffix appends remain
covered by existing resume/recovery tests.

**Boundary:** these checkpoints are process-local and path/ledger/Mother-scoped.
They are not a persistent external anti-rollback witness. A cold process with no
independent prior head cannot distinguish an otherwise valid old/forked history
from the only history it has; the regression deliberately demonstrates that
limitation on an unanchored copy. This does not invent durable continuity, solve
coordinated ledger/projection rollback, recover ambiguous historical lineage, or
establish same-UID race immunity. Durable external anchoring and operator-reviewed
migration/recovery remain separate assurance work. The low-level resume API's
raw `Mismatch` is not permission for callers to discard their trusted anchor.

A bounded `ledger_continuity` libFuzzer target exercises arbitrary frames,
truncated prefixes, suffixes and representation variants, checks ancestry on
accepted histories, and asserts verification never changes the supplied bytes.
It uses one private disposable fixture and a fixed anchor; input is capped at
64 KiB. A 512-run seed-17 coverage-guided smoke uses `--sanitizer none`, not a
memory-sanitizer or exhaustive fuzz assurance claim.

## L17 — remove the blanket yank waiver

Registry/upstream discovery was performed on **2026-10-27**. Both original
lockfiles were retained in the local evidence directory and scanned with yank
checks enabled: the root scan failed on three yanked releases and the fuzz scan
failed on `chacha20 0.10.1`. The previous `enabled = false` policy hid all yank
warnings, not just the `spin` series mentioned in its obsolete comment.

| Crate | Original root / fuzz | Selected in both graphs | Upstream evidence / constraint |
| --- | --- | --- | --- |
| `spin` | 0.10.0 / 0.10.1 | **0.10.1** | The publisher's `v0.10` backport at `026d8e95bf03f90ad2067038df4c138ebc1ac4e7` and packaged changelog fix unsound `Once::{force_into_inner,try_into_inner,into_inner_unchecked}`. `futures-buffered 0.2.13` accepts the 0.10 series; its enabled spin use is `spin_mutex`, with defaults off. No Iroh major upgrade is necessary. |
| `chacha20` | 0.10.0 / 0.10.1 | **0.10.2** | [Release notes](https://github.com/RustCrypto/stream-ciphers/blob/master/chacha20/CHANGELOG.md) mark both originals yanked; 0.10.2 fixes use of an SSE4.1 intrinsic in the SSE2 RNG/legacy backend ([#580](https://github.com/RustCrypto/stream-ciphers/pull/580)). The `rand` 0.10 dependency constraints permit this patch. |
| `der` | 0.8.0 / 0.8.1 | **0.8.3** | [Release notes](https://github.com/RustCrypto/formats/blob/master/der/CHANGELOG.md) explicitly state that 0.8.0 was yanked for a `minimal-versions` CI check. 0.8.3 additionally fixes decoding/ordering/panic/recursion issues. `pkcs8` 0.11 and `spki` 0.8 permit the selected 0.8 patch. |

The registry snapshots establish the original yank flags and selected non-yanked
status. They provide no machine-readable yank reason; the reasons/fixes above
are sourced separately from publisher release notes, not inferred solely from a
flag or asserted to be a CVE. `spin`'s packaged VCS revision matches its upstream
backport ref. The selected MSRVs (1.60, 1.85, 1.85) fit Rust 1.96.0.

`[yanked].enabled` is now `true`; all warning denial remains enabled. There is
**no retained yank exception**. Revisit on every lockfile change or newly reported
yank; normal audits must fail rather than rely on a calendar waiver. The two
unrelated, documented unmaintained-advisory ignores are unchanged and their
metadata check passes.

Root lock changes are limited to those three version/checksum pairs. Fuzz updates
`chacha20` and `der`; its `spin` was already fixed. Resolution also records the
previous storage batch's already-declared `mct-observation -> rustix` edge in the
fuzz lockfile, not a new dependency version. Both retained graphs are checked and
audited with `cargo audit --no-fetch` (and `--file fuzz/Cargo.lock`). Advisory DB
snapshot: `21d909e4c8bd4e14c7aea58bf980c881a0053ab1` (2026-10-24, 1352 advisories).
`--no-fetch` does not refresh that advisory database; registry yank index checks
remain enabled. A clean scan is not an application audit or a claim of unknown
future advisory absence.

The first instrumented fuzz build exposed missing sanitizer-coverage symbols in
an unrelated Iroh relay cdylib on macOS. The fuzz manifest now makes daemon targets
an explicit **default-enabled** feature with required-feature guards. The ledger
smoke opts out of those irrelevant dependencies; existing daemon target defaults
and both full lock graphs remain retained. All-target checks still include the
default daemon targets. The empty fuzz library receives its formatter-required
newline. This is not a fix or assurance claim for all daemon-target instrumentation.

## L18 — immutable action code, explicit unchanged compiler

The setup action is pinned to upstream commit
`683b588c487a69d01f6c6ddf40f036ff682cc2c8`. `git ls-remote` resolved that upstream
master head; the GitHub commit API reports a valid verified signature. The fetched
`action.yml` was compared byte-for-byte with GitHub's blob response and its Git
blob SHA-1 recomputed as `27e3e934c6bf2d024bf5ae51229389635a74f3d5`.
Its SHA-256 is `ad8ad6383b6ce8c08579fc6f6a3fabd6a01f90b4007f65e2a5160944ba2988d4`.
This is upstream provenance verification, not independent maintainer-key enrollment.

The pinned action supports the explicit `toolchain` input; the composite now sets
`toolchain: 1.96.0` and keeps `components: rustfmt, clippy`. The existing compiler
check against `rust-toolchain.toml` is unchanged. The upstream commit's message
mentions a 1.99.1 release-table update; that does **not** override the explicit
1.96.0 input. Local install validation requested exactly 1.96.0/rustfmt/clippy,
reported them unchanged/up-to-date, and passed the actual compiler-version check.
No local rustup default change was needed.

Two standard-library Python policy regressions, integrated into tier-0 specs,
check yank enforcement/both lockfiles and immutable action syntax/explicit
compiler/version-check preservation. Local validation does not execute a GitHub
hosted runner or certify Windows/Linux CI; no push/workflow dispatch is claimed.

## Validation and receipts

Environment: macOS/aarch64, Rust 1.96.0; the bounded fuzz smoke additionally uses
the installed nightly toolchain. Logs and public upstream snapshots are in
`/tmp/mct-l5-l10-l13-l17-l18-evidence/`. The pre-fix counterexample logs retain
UID and rollback failures; failed temporary test directories were not retained.
Exact regression inputs/builders and the fixed representation hash are in source.
Dependency metadata fetching and the explicit compiler install check preceded
locked/offline build/test validation.

```sh
export CARGO_BUILD_JOBS=8 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_NET_OFFLINE=true
export TMPDIR="$(cd "${TMPDIR:-/tmp}" && pwd -P)"
umask 077 # private legacy fixtures only; no production process-wide umask change
for finding in l5_ l10_ l13_; do
  cargo test --workspace --locked --offline "$finding" -- --test-threads=1
done
MCT_ORIGIN_KEEP=1 MCT_FLEET_KEEP=1 MCT_RESTART_KEEP=1 MCT_RESTART_SEED=0x6d637421 \
  cargo test --workspace --locked --offline --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all --check
cargo fmt --manifest-path fuzz/Cargo.toml --check
cargo check --manifest-path fuzz/Cargo.toml --all-targets --locked --offline
cargo audit --no-fetch
cargo audit --no-fetch --file fuzz/Cargo.lock
git diff --check
bash scripts/ci-tier0.sh specs
# Four one-byte corpus seeds: 00, 01, 02, 03; retain corpus/artifacts in evidence dir.
cargo +nightly fuzz run ledger_continuity /tmp/mct-l5-l10-l13-l17-l18-evidence/verified-ledger-corpus \
  --dev --sanitizer none --no-default-features -- \
  -runs=512 -seed=17 -max_len=65536 -timeout=10 -rss_limit_mb=2048
```

Final results: **849 workspace tests passed, zero failed, two pre-existing
ignores**. Target totals excluding subprocess re-execution and empty doc-test
targets: `258 + 200 + 1 + 7 + 3 + 2 + 2 + 58 + 177 + 141 = 849`.
All seven new Rust regressions passed their focused filters:

- `l5_offline_mutation_records_effective_uid_without_rewriting_history`
- `l10_legacy_committed_nonces_survive_epoch_changes_without_monotonic_ordering`
- `l10_d56_held_intents_require_current_epoch_and_snapshot_but_nonces_need_not_increase`
- `l13_cached_checkpoint_rejects_valid_truncation_and_repeated_attempts`
- `l13_valid_rechained_fork_and_sequence_gap_are_not_continuity`
- `l13_checkpoint_survives_partial_committed_tail_and_stale_cache_publication`
- `l13_historical_entry_hash_is_typed_representation_not_raw_json_bytes`

Clippy `-D warnings`, both formatting checks, diff/shell checks, default-feature
fuzz all-target checks, both no-fetch audits and tier-0 specs passed. Specs ran
17 existing structural/tooling tests plus the two new policy regressions.
The final fuzz smoke completed 512 runs, seed 17, reporting `cov: 4315`,
`ft: 8548`, 37 MiB RSS; these counters are not line/branch coverage percentages.
The final corpus is retained as `verified-ledger-corpus.tar.gz`.
All **15** `verified-*.exit` receipts are `0`. The constraints receipt separates
manifest declarations (including inactive dev/optional clauses) from resolved
inverse trees. The **16-file** batch remains unstaged and uncommitted.

SHA-256 receipts (paths relative to the evidence directory):

| Log / artifact | SHA-256 |
| --- | --- |
| `verified-l5-tests.log` | `348b9a6091620578dafcd24de32952a08ca9aee7e9d8a50fdcc52c4309767c96` |
| `verified-l10-tests.log` | `4efca40b09c9ad8edef5cd0662c07a9522daec0592e4efdf354f518c59c7429d` |
| `verified-l13-tests.log` | `b1783c8fc4181b5e48e3bef22ed37087080b3baed070ac27ebe1fbbbd4a9db3e` |
| `verified-workspace-tests.log` | `d19280389745cbf324576f425aab0af0544abda9c11a384827505470221a29f4` |
| `verified-clippy.log` | `f1d0e587f4eaf9c5f6803d3e9210c0e1b536942252f03191c763f7b78b97cc4a` |
| `verified-fmt.log` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `verified-specs.log` | `d80365430e7f0233982ef85559e6bf79898dd08c3647ae1d734f108870dcab43` |
| `verified-diff.log` | `b4beffab0348ecfb9eaf29fc6d98a8d4b82d379fc72dde9d50141ef60a61e827` |
| `verified-audit-root.log` | `c471e303d8dd628bc352c63da97681104d87d4716393d942beefb93d139f9946` |
| `verified-audit-fuzz.log` | `b4dd5e19cec193fb8f9755883bf92b10da1588570ecba769d4250b4c764561de` |
| `verified-constraints.log` | `c14f1bc97c86ae706b932fd57508a43de18b2bbc755c6d441afc26f04387b6d9` |
| `verified-fuzz-check.log` | `6c8c081409464da21ae86a3d716442cab25088cb7a2c7d684e843588dc9a1154` |
| `verified-ledger-fuzz.log` | `361706ad364900af5341bd3bfc9731078d68981f90c74304922444643633189e` |
| `verified-toolchain.log` | `fdf0c4cc4f420f8cdee0c4722322c0cf51d4d050615b5ee121c2bfcc92085f0c` |
| `verified-upstream.log` | `c1c5419a154e9f516f8267ef8cb867d3ce5a7fe9882723e3eab005f12be440d5` |
| `verified-ledger-corpus.tar.gz` | `b136d0757a03b8198c3216a3343475d3be2819bfb0a9a171a2572c5c6cce70a6` |
| `before-l5_.log` | `b46d758236c3b82bee45efc1bcb9573784fdd4e4292a7a8606766678ebe04072` |
| `before-l13_.log` | `0830de39f4eb58d49f1c97089a8681b96dfe04cae20819157a86970d5555a2d4` |
| `before-root-audit.log` | `bbce26118ca9d40aaf82516900e14c0344b3a5edadc88e7b903d9dc63356008c` |
| `before-fuzz-audit.log` | `3bb36642393af033bc5af1d6558e9555a81f6f4a3893271d98399beb8b173781` |

The before-test commands exited 101 (L5 and L13), and the original-lock audits
exited 1 under restored yank enforcement. These are retained counterexamples,
not final validation failures. Other development corrections included a non-hex
synthetic ticket hash in the new fixture, a test-only Clippy clone warning, and
the instrumented unrelated-cdylib link failure described above.

Public source snapshot receipts:

| Snapshot | SHA-256 |
| --- | --- |
| `spin-0.10.1-CHANGELOG.md` | `0a36ecfcb5b686c962f28166d88ac7f8751ae3dd9002927bbaa398aabd896a19` |
| `chacha20-CHANGELOG.md` | `c0d7137243cb855ec67ae432734c93ebc553dc0e3d710d46e0ebde3a351e9c99` |
| `der-CHANGELOG.md` | `4daf8eab05a8fe4e90473483f0008676d8e5a5f881423d595af474e00697a969` |
| `spin.json` | `c20cf380c0f6928db9fe4133819c5b8177a916b9d65562f6d0d9b2add5b14c5d` |
| `chacha20.json` | `01824f341438b35f352b5be1ab08f86b006c8ae209453df2342f2c5daa40a185` |
| `der.json` | `8d03230029e7d32b222ce0bd75b80c9c8b901233df3e59f4d83b766eba5cebee` |
| `rust-toolchain-master-commit.json` | `2dc7c04b4063cf80ff608b9c074de611c0259418af64477633581db341dddc73` |
| `rust-toolchain-action-blob.json` | `f1da3a318ae34a0a0a794bfbed02cc3977769b7088d636455c4dff3f79234a8e` |

## Remaining assurance

Independent review and formal closure remain open for all five findings.
In particular: full D-57 administrative integration/removal of UID alternatives;
no reactivation of unbound legacy signatures as fresh authority; durable external
anti-rollback anchoring; rollback/migration/crash/race review; current advisory and
yank monitoring; hosted CI and other-platform verification. No release,
publication, migration, or deployment is authorized by this evidence.

# M2/M3 — Immutable release input and bounded tar metadata

Local work on `experiment/git-toy-confinement`, atop H2 checkpoint `983b2a3`
(parent `39239f4`). No push or main-branch change.

## Format and original boundaries

This checkout uses **gzip**, `flate2::read::GzDecoder`, and `.tar.gz`, not zstd.
The release format and public verification API are unchanged.

Previously, verification hashed the source path, reopened it for scanning,
reopened it for the gzip header, reopened it for extraction, then checked a
final source hash. Per-file extraction hashes were also checked, but these
checks did not make the input immutable across passes. Acquisition additionally
reopened the source sidecars *after* verification to copy their contents into
the published release.

Tar's normal iterator processes GNU long-name/long-link and local PAX extension
bodies before yielding the ordinary entry to our checks. The pinned tar crate
initially caps its extension preallocation at 128 KiB, but then calls
`read_to_end`, growing the buffer without our regular-entry size/count checks.

The audit's reported ~1 MB / 1.1 GB RSS reproducer is not available here. This
work retains newly generated finite counterexamples and does not claim to have
reproduced that original RSS measurement or run a full fuzz campaign.

## M2 implementation

- Open the source once, check the opened handle is a regular file, then stage
  at most **256 MiB** into a private `Box<[u8]>`. No backing pathname or writable
  file handle remains attached to the snapshot.
- SHA-256/BLAKE3 checks, raw-header preflight, archive scan, gzip header check,
  and extraction all consume the same immutable bytes. The final mutable-path
  rehash is removed; immutability replaces that retrospective check.
- Reads remain bounded even if an initially small file grows during staging.
  Snapshot buffer reservations are fallible and capped to the logical limit.
- Sidecars are read with an actual **256-byte** stream limit, not a stat-only
  check. Their exact canonical text must match the snapshot digests/name.
- Published sidecars are generated from those verified digests/name; acquisition
  never reopens the mutable source sidecars after verification.
- Extraction starts in a newly created **0700** destination. Existing files,
  directories and symlinks are not overwritten. Failed extraction removes the
  newly created partial tree; staging bytes themselves need no filesystem
  cleanup.

A write during staging need not produce an atomic filesystem point-in-time
snapshot. The important property is that whatever bytes were captured are
frozen **before** hashing/scanning/extraction. A changed or mixed snapshot must
pass all ordinary archive checks and the requested identity; it cannot be
installed under a previously verified digest. With no externally pinned
identity, sidecars still provide consistency, not publisher authentication;
operator approval and platform verification remain unchanged.

### Destination trust boundary

The destination's parent must be operator-controlled, as documented on the API.
0700 protects the extraction tree from other users traversing it, but does not
defend against an attacker who owns/can replace its parent or against arbitrary
same-UID mutation of installed files. This patch does not claim descriptor-
relative extraction into a hostile shared namespace. Existing acquisition
publication/rollback and signature-verification boundaries are preserved.

## M3 implementation and named budgets

`HardByteBudget` sits **between decompression and `tar::Archive`** on preflight,
scan and extraction. It supplies no bytes beyond its limit. Unlike `Read::take`,
it errors on overflow rather than presenting successful truncated EOF. One
unexposed overflow-probe byte distinguishes exact-limit EOF; after overflow the
error is sticky and subsequent reads consume no more input.

| Resource | Bound |
| --- | --- |
| Compressed immutable snapshot | 256 MiB |
| Regular-file payload total | 512 MiB |
| Entire decompressed stream per pass | 520 MiB |
| Aggregate GNU/PAX extension metadata | 8 MiB |
| Individual captured metadata file | Existing 8 MiB |
| Logical entries | Existing 32 |
| Physical headers, including extensions | 128 |
| Each external checksum sidecar | 256 bytes |

A second protection avoids letting a metadata bomb consume the entire generous
payload budget: a **raw-header preflight** (`entries().raw(true)`) checks the
aggregate advertised GNU/PAX sizes before the normal iterator allocates their
buffers. It also rejects forbidden raw entry types, nonempty directories,
excess payload declarations and excessive physical headers. The reader is then
rewound over the same immutable snapshot for the existing semantic scan.

All passes drain the remaining decompressed stream after tar's end marker:
trailing padding is charged, and missing/corrupt gzip trailers cannot be hidden
by an early tar EOF. Existing exact layout, path/type, canonical metadata,
internal/external checksum, target and epoch checks remain in force. Ordinary
GNU long-name and PAX path extensions within budget still pass.

These are byte/entry bounds, not a measured process-RSS guarantee. There is an
additional bounded streaming preflight pass; no new dependencies or archive
format changes were introduced.

## Deterministic regression coverage

Ten tests in
`crates/mct-daemon/src/release/acquisition_tests/boundaries.rs` cover:

- exact-limit EOF, zero limits, one-byte overflow, sticky errors and growing
  source reads;
- GNU `L`/`K` and PAX `x`/`g` streams: the old unguarded iterator consumes
  **67,072 decoded bytes**, while the guarded iterator consumes **4,097** for
  a 4,096-byte test cap (4,096 delivered plus one overflow probe), then errors;
- 1 GiB metadata declarations rejected from headers alone, before body reads,
  plus aggregate metadata and raw-header-count limits;
- a valid release at its exact decompressed-byte boundary, cap-minus-one
  refusal, padding beyond tar EOF, and a missing gzip trailer;
- valid bounded GNU/PAX metadata and normal release fixtures;
- replacement and in-place modification **after staging**: extraction still
  produces the original executable and original archive digest;
- replacement and in-place modification **during staging**, injected after the
  first byte read: rename preserves the opened original source; an in-place
  substitution of a different valid archive is refused under the original
  expected SHA-256, with no destination created;
- source/sidecar replacement at the actual acquisition `ArtifactVerified`
  observation: published sidecars and executable still describe the original
  verified bytes;
- existing destination/symlink preservation, partial-extraction cleanup and
  owner-only destination permissions.

No sleeps, racing threads or probabilistic scheduling are used in these tests.
The compressed and decompressed boundary tests use small injected limits through
the production budget implementation, rather than allocating hundreds of MiB.
The existing three hostile/positive release archive integration tests also pass.

## Validation and retained evidence

macOS arm64, locked/offline graph:

```sh
export CARGO_BUILD_JOBS=8 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_NET_OFFLINE=true
export MCT_RELEASE_EVIDENCE_DIR=/tmp/mct-m2-m3-release-evidence
cargo test -p mct-daemon --lib --locked --offline release::acquisition_tests \
  -- --nocapture --test-threads=1
MCT_ORIGIN_KEEP=1 MCT_FLEET_KEEP=1 MCT_RESTART_KEEP=1 \
  cargo test --workspace --locked --offline --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo check -p mct-daemon --features fuzzing --locked --offline
cargo fmt --all --check && git diff --check
bash scripts/ci-tier0.sh specs
```

- Release acquisition unit tests: **14 passed, 1 pre-existing ignored**.
- Full workspace: **773 passed, 0 failed, 2 pre-existing ignored**.
- Clippy, fuzz-feature compilation, fmt, diff checks and tier-0 specs: passed.
- Logs and four generated 196–198-byte extension counterexamples are retained
  under `/tmp/mct-m2-m3-release-evidence/`. Test code deterministically regenerates
  them with `MCT_RELEASE_EVIDENCE_DIR` set. They are local evidence, not committed
  release artifacts or the unavailable original audit PoC.

Regenerated after the power interruption on 2026-10-25: both test suites and
all checks above passed again with the same counts. The four extension fixture
digests are unchanged; the log receipts below identify this fresh run.

Selected SHA-256 receipts:

```text
b89b468968234541dcdde74dd515720aceae0f9db106be44fa69dc41f09a45f4  release-tests.log
276dd8beb41b4312f39485c9b6b3ef61fa33603385f3dfb14156907c757da5cf  workspace-tests.log
a884c4185be51f7c8c3901475479d51daa6410806e1bcb55ad78116b1b0ac141  extension-L.tar.gz
20fd3d61e866010258c7cf1d70ae323383cbe69e534c464b79c0a54474db567a  extension-K.tar.gz
ce1e5f9e6a65fd2390c33fa426608a10ec889e637ef5fff7f979b3be4634b6a2  extension-x.tar.gz
5ddfd2633c33c35639b81566d204aab114cd3833bd2a8bb4eac2146c31e4d8de  extension-g.tar.gz
```

Local implementation/testing is separate from independent review, an extended
fuzz/resource-limit campaign and formal completion-inventory closure. H1's
Linux confinement and review requirements remain open.

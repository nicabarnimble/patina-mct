# Post-R3 credibility slice close-out

Status: complete — assurance docs, Scorecard workflow, hostile-input fuzz coverage

## Claim boundary

This slice adds credibility *evidence surfaces* over the already-landed R2
security closure and R3 release discipline. It introduces no new runtime
behavior, no new authority surface, and no new product-map invariant. It does
not touch the operational `patinaMother` shutoff (R4) or into-the-wild
publication (R5) claims, which remain separately gated.

## Reconstructed implementation range

Commits `5ba5eed..4d8713e` (10 commits on `patina`):

- `bec8434` docs(assurance): add ASSURANCE.md and SECURITY.md
- `70930dd` fix(ci): remove or guard tier0 python3>=3.11 dependency
- `b4bd807` ci: add OpenSSF Scorecard workflow
- `79b009f` test(fuzz): scaffold cargo-fuzz workspace
- `a958b84` test(fuzz): uds_control_request target
- `83c0c99` test(fuzz): release_archive target
- `163d6d1` test(fuzz): child_package_manifest target
- `fe74ebf` test(fuzz): pando_manifest target
- `b47be97` ci: add bounded fuzz smoke workflow
- `471f5b5` docs(product): record post-R3 credibility slice close-out
- `4d8713e` fix(ci): scratch fuzz corpora in smoke run and mark seeds binary

Division-of-labor note: `bec8434`, `70930dd`, `b4bd807`, and the `79b009f`
scaffold were landed by the pi build agent. Tasks under `a958b84..b47be97`
plus the `4d8713e` repair were completed directly by Claude after pi's
provider flagged the fuzz work mid-task (operator-ratified exception).

The two pre-existing untracked session/belief artifacts were never included in
any commit.

## Fuzz seams (file:line proof citations)

Each target drives an operator-external parse path through a fuzzing-gated
entry point re-exported from `crates/mct-daemon/src/lib.rs`:

| Target | Entry point | Seam under test |
|---|---|---|
| `uds_control_request` | `control::fuzz_uds_control_request` (`crates/mct-daemon/src/control.rs:645`) | `parse_uds_control_request_head` — bounded header parse, request-line/content-length extraction, owner/preflight/read-only route classification (`crates/mct-daemon/src/control.rs:616`) |
| `release_archive` | `release::fuzz_release_archive` (`crates/mct-daemon/src/release.rs:1446`) | `scan_archive_reader` — gzip/tar walk, layout, manifest decode, internal checksums, display-safe metadata before extraction (`crates/mct-daemon/src/release.rs:922`) |
| `child_package_manifest` | `acquisition::fuzz_child_package_manifest` (`crates/mct-daemon/src/acquisition.rs:913`) | `SdkChildManifest::from_toml_str` → `manifest_namespaces` → `canonical_package_manifest` (`crates/mct-daemon/src/acquisition.rs:750`, `:766`) |
| `pando_manifest` | `parse_pando_manifest_str` (public) (`crates/mct-daemon/src/composition.rs:163`) | `MctPandoManifest` TOML parse plus structural validation (`crates/mct-daemon/src/composition.rs:164`) |

Excluded by operator ruling: daemon `config.json` and supervisor records —
daemon-owned projections, not operator-external input seams. Verified absent
from all fuzz references.

## Corpus seeds

Nine curated seeds, marked `binary` in `.gitattributes` (some are
deliberately CRLF UDS HTTP heads and must not be normalized):

- `uds_control_request`: `call-preflight`, `owner-mutation`, `read-only-auth`
- `release_archive`: `valid-release` (regenerable via the `#[ignore]`
  `write_release_archive_fuzz_seed` test from the existing `release_fixture`)
- `child_package_manifest`: `slate-manager`, `folder-watch-actor`,
  `watch-null-sink` (copied from committed `tests/fixtures`)
- `pando_manifest`: `slate-pando`, `writer-pando` (from the composition tests)

## Per-commit and slice validation evidence

Every commit passed, per the binding per-commit protocol:

```
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
```

Final slice validation (from disk, this close-out HEAD):

- `cargo test --workspace -- --nocapture`: **418 tests passed**, 0 failed, 0
  ignored across 11 suites. Full transcript captured at
  `close-out-test-transcript.txt` (session scratch), SHA-256 prefix
  `619667eedc539eb2`.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean (exit 0).
- `./scripts/ci-tier0.sh`: exit 0 (now python3-version-independent).

## Fuzz evidence run

Toolchain `rustc 1.96.0-nightly (3b1b0ef4d 2026-03-11)`, cargo-fuzz 0.13.2.
Each target run 100,000 executions over its committed corpus into a scratch
output dir (committed corpora left unmodified, confirmed via `git status`):

| Target | Runs | Edge cov | Features | Corpus | Crashes |
|---|---:|---:|---:|---:|---:|
| `uds_control_request` | 100,000 | 279 | 700 | 237 | 0 |
| `release_archive` | 100,000 | 2,483 | 6,403 | 400 | 0 |
| `child_package_manifest` | 100,000 | 3,429 | 10,630 | 589 | 0 |
| `pando_manifest` | 100,000 | 2,905 | 7,053 | 524 | 0 |

`fuzz/artifacts/` holds zero crash files. No sanitizer aborts, no panics, no
timeouts observed.

## Flake log

No flakes. The workspace suite and all four fuzz targets ran clean on first
attempt at this HEAD. One earlier process error, corrected before it reached a
durable commit: an initial `uds_control_request` smoke run without a scratch
output dir let libFuzzer write ~190 machine-generated entries into the
committed corpus; the commit was amended to the 3 curated seeds, and `4d8713e`
moved the CI smoke to a scratch-first invocation so this cannot recur in CI.

## Deferred / pending

- **OpenSSF Scorecard initial score**: the workflow (`b4bd807`) is
  digest-pinned and `publish_results: true` is valid because the repository is
  public, but no score exists until the workflow first runs on GitHub
  (`branch_protection_rule` / weekly schedule / push to `main`). The initial
  observed score must be recorded here after that first run — it is not
  reconstructable from local disk.

## Map-tend waiver

No `mct-product-map.allium` change and no LEDGER attribution rows: the four
targets are test evidence exercising existing landed parse contracts. They
emit no new structural obligations, entities, surfaces, or authority. Recorded
against TODO item 8 in `MCT-NEXT-BUILD-TODO.md`.

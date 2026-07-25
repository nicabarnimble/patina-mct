# Post-R3 credibility slice close-out

Status: complete

## Claim boundary

This slice adds credibility *evidence surfaces* over the already-landed R2
security closure and R3 release discipline. It introduces no new runtime
authority surface and no new product-map invariant. It does not touch the
operational `patinaMother` shutoff (R4) or into-the-wild publication (R5)
claims, which remain separately gated.

## Reconstructed commit range

The original implementation range `5ba5eed..4d8713e` contains **11 commits**.
The complete range through this repaired close-out contains **15 commits**.
The repair endpoint is resolved from the transcript introduced by that commit,
avoiding an impossible self-referential commit hash:

```bash
repair_endpoint=$(git log -1 --format=%H -- \
  layer/surface/build/feat/post-r3-credibility/close-out-test-transcript.txt)
git rev-list --count 5ba5eed.."$repair_endpoint"
git log --reverse --format='%h %s' 5ba5eed.."$repair_endpoint"
```

The resulting ordered list is:

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
- `47b6106` docs(release): durable post-R3 credibility slice close-out evidence
- `76e3eec` chore(fuzz): keep text manifest seeds diffable
- `6f6ae18` ci(fuzz): bound smoke job with timeout-minutes
- repair endpoint: docs(release): repair close-out to disk-verifiable evidence

Division-of-labor note: `bec8434`, `70930dd`, `b4bd807`, and the `79b009f`
scaffold were landed by the pi build agent. Tasks under `a958b84..b47be97`
plus `4d8713e` were completed directly by Claude after pi's provider flagged
the fuzz work mid-task (operator-ratified exception). The repair commits begin
at `76e3eec`.

At the repaired close-out commit, `git status --porcelain` reports only the two
pre-existing untracked session and belief artifacts; the abandoned truncated
product-directory close-out draft was removed with operator approval and was
never committed.

## Fuzz seams (file:line proof citations)

Each target drives an operator-external parse path through a fuzzing-gated
entry point re-exported from `crates/mct-daemon/src/lib.rs`:

| Target | Entry point | Seam under test |
|---|---|---|
| `uds_control_request` | `control::fuzz_uds_control_request` (`crates/mct-daemon/src/control.rs:645`) | `parse_uds_control_request_head` — bounded header parse, request-line/content-length extraction, owner/preflight/read-only route classification (`crates/mct-daemon/src/control.rs:616`) |
| `release_archive` | `release::fuzz_release_archive` (`crates/mct-daemon/src/release.rs:1446`) | `scan_archive_reader` — gzip/tar walk, layout, manifest decode, internal checksums, and display-safe metadata before extraction (`crates/mct-daemon/src/release.rs:922`) |
| `child_package_manifest` | `acquisition::fuzz_child_package_manifest` (`crates/mct-daemon/src/acquisition.rs:913`) | `SdkChildManifest::from_toml_str` → `manifest_namespaces` → `canonical_package_manifest` (`crates/mct-daemon/src/acquisition.rs:750`, `:766`) |
| `pando_manifest` | `parse_pando_manifest_str` (public) (`crates/mct-daemon/src/composition.rs:163`) | `MctPandoManifest` TOML parse plus structural validation (`crates/mct-daemon/src/composition.rs:164`) |

Excluded by operator ruling: daemon `config.json` and supervisor records are
daemon-owned projections, not operator-external input seams. They have no fuzz
targets.

## Corpus and durable fuzz evidence

Nine curated seeds are committed:

- `uds_control_request`: `call-preflight`, `owner-mutation`, `read-only-auth`;
- `release_archive`: `valid-release`, regenerable via the ignored
  `write_release_archive_fuzz_seed` test;
- `child_package_manifest`: `slate-manager`, `folder-watch-actor`,
  `watch-null-sink`; and
- `pando_manifest`: `slate-pando`, `writer-pando`.

`.gitattributes` marks only the raw CRLF UDS heads and gzip release archive as
binary. The five Child/Pando TOML manifests remain reviewable text.

The durable fuzz evidence is the four target declarations and source files,
the nine seeds, and the scratch-first bounded CI smoke in
`.github/workflows/fuzz.yml`. The four declared targets compile with
`cargo +nightly fuzz build --fuzz-dir fuzz`; CI will run 25,000 executions per
target after push. The whole job is bounded at 30 minutes: four 120-second fuzz
windows total 8 minutes, leaving 22 minutes for setup and sanitizer builds.
`find fuzz/artifacts -type f 2>/dev/null` returns no files at this close-out.
No local fuzz-run count, coverage count, or zero-crash result is claimed here.

## Workspace test evidence

The full committed transcript is
[`close-out-test-transcript.txt`](close-out-test-transcript.txt), produced with:

```bash
cargo test --workspace
```

Its SHA-256 is
`00d8f67e2f4175e10828336643c0c2487a4446822eab11f6b5b41f1e9f4aac1c`.
The 11 suite result lines record **418 passed, 0 failed, 1 ignored**. The one
ignored test is `write_release_archive_fuzz_seed`, visible in the transcript
and defined at `crates/mct-daemon/src/release.rs:1624`; it intentionally
regenerates the committed release-archive seed.

The earlier claim that every implementation commit had durable per-commit
validation evidence is withdrawn: no landed logs support that historical
claim. The binding repair gate remains reproducible at any named commit:

```bash
cargo test --workspace && \
  cargo clippy --workspace --all-targets -- -D warnings && \
  ./scripts/ci-tier0.sh
```

`scripts/check-release-version.sh` no longer imports `tomllib` and therefore no
longer requires Python >=3.11. It passes with macOS system Python 3.9; reproduce
that selection by placing a `python3` symlink to `/usr/bin/python3` first on
`PATH` before invoking the script.

## Flake log

The committed workspace transcript contains no failed test. No single-test
failure occurred while producing it, so the flake protocol required no rerun.
Repair-commit gate outcomes and any later flake are reported from disk at the
operator review gate rather than inferred here.

## Resolved post-merge evidence

- **Push and PR:** PR #31 merged to `main` at `39c5e33`.
- **OpenSSF Scorecard initial score:** **5.1**, observed at
  `2026-07-25T02:02:28Z` for evaluated commit `39c5e33` with Scorecard
  `v5.3.0`. The reproducible source is the public API:
  <https://api.securityscorecards.dev/projects/github.com/nicabarnimble/patina-mct>.
  Fuzzing, Security-Policy, Token-Permissions, Dangerous-Workflow, License,
  and CI-Tests scored 10. The zero-scored checks are structural for a young
  single-maintainer repository. Two items merit separate follow-up:
  Vulnerabilities scored 8 with 2 existing advisories detected and should be
  triaged against the R2 RustSec closure in a future slice; Binary-Artifacts
  scored 7, likely reflecting the committed binary corpus seeds and fixtures.

## Map-tend waiver

No `mct-product-map.allium` change and no LEDGER attribution rows: the four
targets are test evidence exercising existing landed parse contracts. They
emit no new structural obligations, entities, surfaces, or authority. The
waiver is recorded against TODO item 8 in `MCT-NEXT-BUILD-TODO.md`.

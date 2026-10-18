# Start here

This page is for a reviewer, human or agent, who has the repository and nothing
else. Read it first. It tells you what MCT is, where the rules are written, what
is built and what is only decided, and how to report what you find.

State described here: `main` at `49dfacd` (2026-10-17, after PR #69).

## What MCT is

MCT is a runtime built on one rule: **code does not gain power because it is
running.**

- **Mother** is the only authority. She admits every call, grants every
  capability, holds every key and credential, and records every authority
  decision in her ledger before the effect happens.
- A **Child** is a WASM component. It computes. It has no ambient filesystem,
  network, secrets, processes or database. Its only reach is the handles Mother
  gives it. A Child never sees a raw key or credential.
- A **Toy** is a capability Mother grants to a Child: one directory, one
  secret, one logging sink, one Git repo. A `ToyGrant` names which Child may do
  which action, in which scope, until when. No grant, no Toy.
- A **handle** is how anything reaches a Child. Holding a platform-given handle
  is the authorization. A Child never mints a handle. When one Child shares with
  another, Mother mints the narrowed handle and records it (the courier rule).
- The **ledger** (`mct-observation`) is an append-only, BLAKE3 hash-chained
  JSONL file with one writer. It is the source of truth for authority. Facts
  that must exist before an effect are fsynced before the effect starts; if the
  append fails, the effect does not happen.
- The **SQLite database** is runtime state and projections. It is not
  authority. Where the two disagree, the ledger wins. One known gap: restart
  recovery still reads in-flight work from SQLite (see
  [OPEN-QUESTIONS.md](OPEN-QUESTIONS.md#1-child-start-ledger-fact-double-run-on-power-loss)).
- **Fail closed.** Unknown, expired, revoked, stale, mismatched or malformed
  state is a typed denial, never a permissive default. Mother unreachable is a
  hard stop for a Child's work, not failover.
- Mothers talk to each other over **iroh** (QUIC with Ed25519 endpoint
  identities). A peer is admitted only against a signed binding, and admission
  never pre-authorizes a call.

**Mother decides. Children compute. Toys effect.**

The longer narrative is [`layer/core/what-is-mct.md`](../layer/core/what-is-mct.md).
The formal specification is
[`layer/allium/mct-product-map.allium`](../layer/allium/mct-product-map.allium).
Where prose and the Allium spec disagree, the spec wins.

## Decided versus built

Do not assume a decision is code. These are the main gaps on `main` today:

| Topic | Decided | Built on `main` |
| --- | --- | --- |
| Admin keys (admin side makes the keypair, Mother stores the public half, scoped, ledgered, expiring) | Yes, 2026-10-14 | Ledger facts and the route-scope check, yes (#48, #51, #64). Admin calls over iroh, no. |
| Iroh is the only admin path; delete UDS admin, `peer_cred`, `open_local`, `execute_offline_*` | Yes, hard cutover | **No.** The UDS control socket, `authenticate_owner`, `open_local` and the `execute_offline_*` commands still exist. README and ROADMAP describe the decided end state. |
| Hybrid Ed25519 + ML-DSA-65 admin signatures (scheme v1) | Yes, 2026-10-15 | Yes, but **optional** on replay (#67). Unsigned admin facts are still accepted. |
| Child call is a platform handle on the iroh path | Yes | Peer calls use iroh. Local `POST /calls` still uses the UDS peer-credential preflight. |
| Wake check-in, checkpoints, freeze (decisions e, f) | Yes | No. Build plan phases 5–10. |
| Restart recovery, interrupted calls, torn-tail recovery | Yes | Yes (#66, #68). |
| Ledger fact proving a Child launch started | Proposed | No. Known double-run hole on power loss. |
| Multi-Mother clustering, standby, quorum, shared ledgers | Future, not planned | No. Only single-hop peer forwarding is real. |

The full list, with dates and reasons, is [DECISIONS.md](DECISIONS.md).

## Repository map

| Path | What it is |
| --- | --- |
| `crates/mct-kernel` | Pure authority domain. Calls, Children, Toys, peers, routes, admin-key grants, hybrid signatures (`admin_sig.rs`, `admin_key.rs`, `grant_check.rs`). No I/O. |
| `crates/mct-observation` | The ledger: hash chain, single-writer lock, authority facts and replay, tail recovery, segments. |
| `crates/mct-iroh` | Mother's iroh endpoint, `mct/hello/0` and `mct/call/0`, signed peer bindings. |
| `crates/mct-daemon` | Composition: config, wasmtime Child runtime, Toy adapters, SQLite state, control plane, resident Mother, restart recovery, CLI. |
| `crates/mct-trace` | Verifies and projects published session traces into the docs book. |
| `crates/phase3-measure` | Benchmark for hybrid signature cost and ledger size. Not shipped. |
| `fuzz/` | cargo-fuzz targets over hostile inputs. |
| `layer/allium/` | Allium specs. `mct-product-map.allium` is the product law. `mct-peer-ontology.allium` is the peer law. |
| `layer/core/` | Doctrine: what MCT is, reliability doctrine, safety boundaries, dependable Rust, vocabulary. |
| `layer/surface/build/feat/<slice>/` | One spec (and sometimes design or closeout) per build slice. Slice specs carry numbered decisions such as `D-H3.5`. |
| `layer/surface/build/product/` | `ROADMAP.md`, `MCT-NEXT-BUILD-TODO.md`, release checklists, the Mother replacement runbook, baselines. |
| `layer/sessions/`, `traces/` | Session records and published traces. Historical evidence; do not edit. |
| `docs/src/` | The mdBook product manual. |
| `docs/design/` | Design notes imported from working discussions. See [design/README.md](design/README.md). |
| `docs/DECISIONS.md`, `docs/OPEN-QUESTIONS.md`, `docs/PRIOR-ART.md` | Decision log, open questions, prior-art research. |
| `ASSURANCE.md`, `SECURITY.md` | Security evidence and reporting. |

Where to look for a rule, in order: the Allium spec, then the slice `SPEC.md`,
then `layer/core/`, then the design notes, then code comments.

## Build and test

Rust toolchain is pinned in `rust-toolchain.toml`.

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -- --test-threads=1
./scripts/ci-tier0.sh            # fmt, specs, clippy, test, audit
./scripts/ci-tier0.sh specs      # release, vocabulary and Allium checks
./scripts/build-docs.sh          # needs ./scripts/install-docs-tools.sh first
```

The resident tests own real Unix sockets and iroh endpoints, so CI runs tests
single-threaded. `scripts/install-hooks.sh` installs a pre-commit hook that runs
`ci-tier0.sh`.

## How changes land

- Every change is a pull request into `main`. Branch protection requires the
  `fmt`, `clippy`, `test` and `authors` checks, requires the branch to be up to
  date with `main`, and applies to admins. Force-push and branch deletion on
  `main` are off.
- Merge with a **merge commit**. Do not squash and do not rebase: session and
  provenance history in `layer/` lives in the individual commits.
- If `main` moves, merge `main` into your branch. Never rebase or force-push a
  pushed branch.
- Only the owner is credited. `scripts/check-authors.sh` fails any commit whose
  author or committer is not on its allowlist, and any `Co-authored-by` trailer.
- Do not delete a branch after merge without the owner's say-so.
- A failing check is fixed in the change, never by weakening the check.
- A behavior change updates the Allium spec or the slice spec in the same PR,
  or says why it does not need to.

## What a review should look for

The owner wants cold, blunt reviews. Useful questions:

1. Can any path reach an effect without Mother admitting it first? Look for
   allow-by-default branches, optional expiry, wildcard scopes, and checks done
   after the effect.
2. Can a Child see a raw key, credential, secret value, or ambient host power?
3. Is anything trusted because it is local (same uid, localhost, a file on
   disk) instead of because it presents a granted key or handle?
4. Does every BeforeEffect fact really reach disk before the effect? What
   happens on a crash at each step, on power loss, and when SQLite and the
   ledger disagree?
5. Can replay restore authority that was revoked, or accept a fact it should
   refuse?
6. Do the docs, the Allium spec and the code say the same thing?
7. Is there a simpler or cheaper design that keeps every guarantee? See
   [PRIOR-ART.md](PRIOR-ART.md) for what we already compared.

## How to report findings

Write one entry per finding:

- **Severity**: `authority` (a guarantee can be bypassed), `correctness`
  (wrong result or lost or duplicated work), `durability`, `spec drift`,
  `docs`, or `design question`.
- **Where**: file and line on a named commit, or the spec rule ID.
- **What**: one or two sentences. What happens, and under what conditions.
- **Evidence**: a failing test, a command and its output, or a quoted line. Say
  plainly what you did not verify.
- **Proposal**: the smallest change that fixes it, if you have one.

Check [DECISIONS.md](DECISIONS.md) before proposing to reverse a decision, and
say which entry you are challenging. Check [OPEN-QUESTIONS.md](OPEN-QUESTIONS.md)
before raising something already open. Do not open PRs that change code
without the owner's approval. A findings file or an issue is the default
output.

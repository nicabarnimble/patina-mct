---
type: feat
id: dependency-drift-child
status: active
created: 2026-11-06
target: NIC-105
related:
  - docs/mct-vision-child-plan.md
  - docs/plans/first-real-app.md
  - docs/nic63-mesh-job.md
  - layer/surface/build/feat/multi-mother-mesh-job/SPEC.md
  - crates/mct-repo-report/
  - scripts/federation/mesh-job.mjs
exit_criteria:
  - id: canonical-read-compute-child
    text: mct-repo-report remains a bounded WIT-only read/compute Child with only the directory-read capability request and returns a complete bounded Cargo.lock package/version inventory.
    checked: false
    verify: cargo test -p mct-repo-report
  - id: client-owned-drift
    text: One authenticated mesh job runs the same Child on Mother A and Mother B and the client computes a versioned dependency drift result with at least one real package/version difference.
    checked: false
    verify: node --test scripts/federation/drift-report.test.mjs plus physical evidence
  - id: exact-execution-attribution
    text: Verified independent ledgers prove A's inventory ran only on A and B's inventory ran only on B.
    checked: false
    verify: physical evidence document plus canonical ledger summaries
  - id: expired-b-grant-denial
    text: After B's exact directory-read grant expires, a fresh B probe is denied by B with a typed expired-grant fact and zero B runtime starts.
    checked: false
    verify: physical evidence document plus canonical ledger summaries
  - id: build-once-delivery
    text: Native artifacts and the Child component are built once on nicamaster, packaged with both immutable snapshots in one signed closure, and imported on nicanode0 without Cargo or Nix compilation there.
    checked: false
    verify: build and deployment receipts in physical evidence
  - id: repository-gates
    text: The requested repository gates pass on the final diff.
    checked: false
    verify: cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && bash scripts/ci-tier0.sh specs && git diff --check
---

# NIC-105 reference Child and two-Mother dependency drift

## Goal

Ship `mct-repo-report` as the first canonical read/compute Child. Mother A reads
its immutable `Cargo.lock` snapshot, delegates the same Child to Mother B for
B's immutable snapshot, and returns both inventories through one mesh job. The
client computes and retains the dependency drift. No new Toy is introduced.

## Authority boundary

Each Mother independently holds the exact artifact approval, assignment and
scoped directory-read ToyGrant for its own snapshot. A local mesh placement
constraint may remove nonmatching routes, but it cannot make an unauthorized
route admissible. B independently revalidates its grant before runtime start.

## Evidence contract

The physical result passes only when verified canonical ledgers prove:

- A's piece has exactly one runtime start and completion on A and none on B;
- B's piece has forwarding evidence on A and exactly one runtime start and
  completion on B, with none on A;
- the client-owned drift contains at least one package/version difference; and
- a fresh post-expiry B probe has `ToyGrantExpired` / `ExpiredGrant` on B and
  zero B runtime start/completion/failure facts.

Host addresses, SSH identity, private roots and source paths remain in private
configuration. The target Pi imports the signed closure and does not compile.

## Non-goals

- No new Toy or network capability.
- No typed successor WIT in this piece.
- No Child-owned persistence or comparison state.
- No graph scheduler, parallel execution, rollback or multi-hop forwarding.
- No compilation on Pi 3 targets.

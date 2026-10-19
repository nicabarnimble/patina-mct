# Handoff: finish MCT and bounded federation

## Start here

Working repository (standalone clone, not the older worktree):

```sh
cd /Users/nicabar/Projects/Patina/patina-mct-finish
```

- Branch: `feat/finish-mct-federation`.
- Runtime/documentation baseline before this handoff: `4ac0bfe`.
- `origin`: `git@github.com:nicabarnimble/patina-mct.git`.
- `seed`: local source `/Users/nicabar/Projects/Patina/patina-mct-design-hardening`.
- Original worktree remains on `fix/ledger-first-call-recovery`, unchanged.
- Latest confirmed remote `main`: `433087658b35ff83ff20459052a9b2f3150455cb`.
- Work was clean before adding this handoff. All completion work is local:
  **nothing pushed, merged, or deployed**.

Owner's instruction: **continue until all green, then push this branch to
origin when complete.** The subsequent instruction was to make this handoff
so a new session can address the outstanding decisions. That is not approval
of the proposed security defaults below.

## Goal and boundaries

Finish a dependable single-Mother runtime and direct cooperation of 4–6
independent Mothers in one explicitly agreed Vision. Each Mother owns its
identity, state, authority and ledger. An unavailable peer must not stop
unrelated local work.

Keep single-hop forwarding and no takeover. No shared ledger, consensus,
cluster scheduler, guardian quorum, cross-Vision federation or whole-Pi WASM
port prerequisite. Keep the existing one-local-Vision boundary for this release.
PiD may be a practical client/reference workflow; it is not Mother's authority
ledger and has not been deployed or changed during this work.

Allium was reversibly retired in merged PR #71. Its maps remain frozen; do not
restore an Allium gate or edit those maps. Follow `AGENTS.md`, core doctrine,
settled decisions, bounded slices and executable evidence.

## Read in this order

1. `AGENTS.md`.
2. `docs/FINISH.md` — full completion scope and sequence.
3. `docs/design/finish-owner-gates.md` — proposed choices requiring confirmation.
4. `docs/finish-checks.json` — executable completion inventory.
5. `docs/DECISIONS.md`, especially D-07–D-17, D-37–D-40, D-51 and D-52.
6. `docs/SECURITY-BOUNDARIES.md` and `docs/OPEN-QUESTIONS.md`.
7. `layer/core/reliability-doctrine.md` and `layer/core/spec-driven-design.md`.
8. `layer/surface/build/feat/federation-finish/{SPEC,EVIDENCE}.md` and
   `layer/surface/build/feat/ledger-first-call-recovery/{SPEC,EVIDENCE}.md`.

## Local commits already present

| Commit | Work |
| --- | --- |
| `ba685af` | Inherited hardening: durable caller/key-scoped dispatch fence against lost SQLite call history. |
| `066b182` | Independent policy counters, nonblocking/startup peer refresh, real six-process federation proof, completion checklist and runner. |
| `4ac0bfe` | Explicit owner/environment gates; corrected stale multi-Mother scope wording. No runtime changes in this commit. |

### Dispatch fence limits

The fence precedes entry into the existing authority pipeline. It preserves
caller-scoped keys and repeatable unkeyed calls. Protection is bounded to the
existing **720 seconds**. Missing result projections yield interruption, not
re-execution or fabricated success. This does not prove final Child launch,
provide task continuation, or implement D-40's proposed global call-ID guard.
D-51 remains a review-branch contract, not a released guarantee.

### Federation changes

- Local peer admission checks use the local policy revision. The remote-signed
  reverse binding and remote publication/surface agree on the remote revision.
  No requirement that independent counters match.
- Forwarded requests carry the receiver-issued policy expectation. The receiver
  still independently checks current binding, policy, grants and execution.
- Peer refresh starts at boot and then uses the existing 120-second interval.
  Refresh is polled alongside serving/shutdown rather than suspending them.
- `crates/mct-daemon/tests/federation_fleet.rs` starts six actual resident
  subprocesses with separate roots, keys and ledgers, revisions
  `[1, 7, 2, 11, 3, 5]`, real Iroh hello publication and actual WASM execution.
  It covers all 30 direct peer routes, local work, receiver revocation,
  four/five surviving nodes, survivor restart with a peer down, restart
  identity preservation, surviving ledger bytes and ledger verification.
- The fixture is same-host loopback, uses today's owner-only UDS, and sets
  policy counters through explicit offline setup. It is **not** admin-cutover,
  canonical configuration recovery, multi-host partition or Linux release proof.

## Validation already recorded

At the runtime milestone, including the final fleet fixture:

- Tier 0: **622 Rust tests passed, 0 failed, 2 existing ignored**.
- Formatting, Clippy (`-D warnings`), policy/vocabulary checks, RustSec audit
  and documentation build passed.
- Completion proof runner: **8 groups / 31 tests passed**.
- Completion-runner self-tests: **14 passed**.
- Full completion remains **red with 8 unfinished obligations**. That is
  intentional, not a failing implemented proof.
- The latest documentation-only changes passed checker self-tests, vocabulary,
  docs build and author checks. No new runtime implementation was attempted
  after the decision-gate review.

Local logs and pre-fix failures: `/tmp/mct-finish-evidence/`. These are temporary
local artifacts, not required inputs or portable release receipts. Durable
summaries are in the slice `EVIDENCE.md` files. `MCT_FLEET_KEEP=1` retains private
fixture roots and logs for diagnosis.

Useful commands:

```sh
python3 scripts/check-finish.py status    # fast inventory; currently exits 1
python3 scripts/check-finish.py proofs    # implemented proofs only; not readiness
python3 scripts/test-check-finish.py
python3 scripts/check-finish.py release   # proofs + full Tier 0 + docs + inventory
```

An optional existing build cache is
`CARGO_TARGET_DIR=/tmp/patina-mct-retire-allium/target`; it is not required for
correctness. Some shell scripts are not executable; invoke them with `bash`.

## Decisions to resolve with the owner first

`docs/design/finish-owner-gates.md` contains the precise proposals. None has
been approved merely by the instruction to finish or by requesting this handoff.

1. **Recovery-key compromise:** retain D-16's no-replacement rule and explicitly
   accept retiring the Mother, revoking peer admissions, rotating affected
   external credentials and bootstrapping a new identity. Do not erase history
   or claim the replacement continues the old authority.
2. **Admin issuance:** node-wide route-class scopes for this release; explicit
   delegation ceilings separate from execution scopes. Recovery can issue
   scoped, expiring day grants. Day-to-day issuers cannot exceed their ceiling
   or expiry, create recovery authority, or delegate a larger ceiling.
3. **Signatures and legacy handling:** mandatory hybrid signatures and authorized
   issuance; self-signing only for explicit virgin bootstrap. Hard cutover,
   not dual UID/key administration. Preserve old unsigned history for inspection;
   do not silently trust it or import configuration-held permissions.
4. **Child handles:** recipient-bound, issuer/epoch/scope/expiry/grant lineage
   checked; Mother issues narrower delegation. Preserve D-G7's preopen limit.
5. **Supervisor boundary:** trusted OS lifecycle starts/stops the pinned binary
   but never mutates authority or bypasses recovery. Signed Iroh authority path
   remains distinct from virgin deployment setup.

Record accepted choices and bounded implementation contracts before implementing
these rules. If the owner changes a recommendation, preserve that distinction
rather than treating the proposal as settled history.

## Environment gate

The supported release target remains **macOS arm64**. Ask which disposable
hosts and test actions are approved for multi-host partition/upgrade proof.
Do not deploy to production, PiD or Unraid to make the gate green. Linux support
requires its own explicit platform/package/supervisor evidence.

`patina spec check federation-finish --json` was blocked by the external
spec-manager at `http://localhost:50051`. Do not start a live daemon solely to
satisfy it. Local checks run without that service. The last attempt to load the
advertised Patina MCT skill file also returned ENOENT; probe actual CLI surfaces
instead of assuming the skill or service exists.

## Next implementation investigation

The intended next slice is canonical Child/peer authority and final-admission
ordering. Important findings from the read-only investigation:

- `crates/mct-daemon/src/daemon/control.rs`:
  `PreparedChildMutation::decision_observations` records names/artifact IDs and
  revisions but not complete approval/assignment scope. Peer mutation
  observations likewise lack full binding/reverse-proof records. Their config
  apply happens after observation append; this is not reconstructable authority.
- `crates/mct-observation/src/lib.rs`: `AuthorityStateV1`/`AuthorityChangeV1`
  currently cover Toys, Watch, admin grants and pairing, not Child/peer authority.
  Canonical mutation generation, state hashes and strict replay must include
  any new records without silently changing old hash contracts.
- `crates/mct-daemon/src/daemon/resident/observation.rs`: the serialized writer
  already supplies canonical mutations, projection publication, startup
  finalization and dispatch fencing. Preserve ordering and failure fencing.
- `crates/mct-daemon/src/authority_snapshot.rs`: execution snapshots still load
  Child/peer policy from config even when Toy authority has a canonical proof.
- `crates/mct-daemon/src/config.rs`, `state.rs`, `startup.rs` and
  `authority_order.rs` are affected integration boundaries.

Do not pretend historical sparse observations contain missing permissions.
A complete solution needs typed canonical records, safe bootstrap/legacy
handling, stale-intent protection, canonical consumers and rollback/crash/race
proofs. Do not fix only startup while live ingress can still trust stale config.

The eight remaining inventory IDs are: `administration`, `canonical-authority`,
`final-admission`, `key-recovery`, `fleet-operations`, `distributed-failures`,
`useful-workflow`, and `release-evidence`.

## Push condition

Only after every obligation has real evidence and
`python3 scripts/check-finish.py release` passes:

```sh
git push -u origin feat/finish-mct-federation
```

Do not waive requirements, remove negative tests, or mark proposals implemented
to obtain green. If remote main moves, follow D-44 (merge, no rebase/force-push).
Push permission is conditional on completion; it does not authorize merging,
deploying, deleting branches or changing production services.

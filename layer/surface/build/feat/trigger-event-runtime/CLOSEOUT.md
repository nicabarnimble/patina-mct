# Trigger event runtime Part A close-out

Status: complete — Replacement Slice 4A

## Reconstructed range

- Required starting branch/tree: `patina` at `20941a4fb7ad5275fea578c67715dc335767ace1`.
- Full immutable Part A range: `20941a4..bf4e922`; `bf4e9225c6e7b83b9974863b39ed47d5dcec31d7` is the Part A integration, attribution, and close-out commit.
- Ratified D1A/D1B checkpoint: `82713eb`.
- Kernel, schema-v8, authority management, scheduler, policy/capacity/recovery, and resident execution substrate: `e3d8a37`.
- Resident acquisition-backed integration proof, Track 3 attribution, product docs, and this close-out: `bf4e922`.

The pre-existing untracked older session and belief artifacts were not included. Part B did not begin before the `bf4e922` close-out boundary.

### 2026-07-21 disk reissuance

Context recovery initially reported this close-out as missing. Git-object inspection proved that no Part A artifact needed to be invented: `bf4e922` already contains this file, has `e3d8a37` as its sole parent, and lands the Track 3 trigger rows and proof integration together. The cited proof-step diff is:

```text
git diff e3d8a37..bf4e922 -- \
  crates/mct-daemon/src/daemon/resident/trigger_scheduler.rs \
  layer/surface/build/spec-drift-audit/track3/LEDGER.md \
  layer/surface/build/feat/trigger-event-runtime/CLOSEOUT.md

3 files changed, 900 insertions(+), 10 deletions(-)
```

The immutable Track 3 addition is the `Replacement Slice 4A — trigger authority and resident scheduler` section in `bf4e922:layer/surface/build/spec-drift-audit/track3/LEDGER.md`: four trigger structural groups, every named `MctCallTriggerAuthority` invariant, all three named deferrals, and the unchanged-observation-kind disposition are explicit.

The SPEC-targeted commands were reissued from the descendant tree after recovery. All thirteen named checks passed, including the acquisition-backed `resident_temporal_trigger_fires_once_and_recovers_without_duplication` proof:

```text
PART_A_REISSUE_TARGETED_TESTS=PASS
13 named tests; 13 passed; 0 failed
```

The original implementation-red and final gate transcripts remain below. This reissuance changes citation durability only; it does not move Part B evidence into Part A.

## Failing-test-first record

The first Part A implementation test was added before `CallOrigin::TriggerFiring` existed:

```text
cargo test -p mct-kernel \
  trigger_firing_origin_is_additive_local_and_single_hop -- --nocapture

error[E0599]: no variant, associated function, or constant named `TriggerFiring` found for enum `call::CallOrigin` in the current scope
    --> crates/mct-kernel/src/call/mod.rs:1300:48
...
error[E0599]: no variant, associated function, or constant named `TriggerFiring` found for enum `call::CallOrigin` in the current scope
    --> crates/mct-kernel/src/call/mod.rs:1303:29
...
error: could not compile `mct-kernel` (lib test) due to 2 previous errors
```

This is the implementation-red transcript: the additive-origin test referenced the new closed variant before production code supplied it.

Flake log: **none**. Later failures were deterministic assertion, compile, or integration failures followed by code/test corrections; no unchanged transient failure was rerun as a flake. The first full-workspace failure after adding canonical trigger result evidence was an exact duplicate `ResultRecorded` on Iroh success; result emission was narrowed to truthful local `TriggerFiring` execution before rerun.

## Commit gates

Every Part A implementation commit passed:

```text
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

Final standing results:

- `allium check layer/allium` — passed without diagnostics/findings.
- `allium analyse layer/allium` — passed without diagnostics/findings.
- `cargo test --workspace` — passed: 372 tests (115 daemon library, 120 daemon binary, 2 Wasm-limit integration, 36 Iroh, 88 kernel, 11 observation), plus doc tests.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `./scripts/ci-tier0.sh` — passed.
- `bash scripts/check-comparative-vocabulary.sh` — passed.
- `git diff --check` — passed.
- `git diff 20941a4 -- layer/allium` — empty.
- `git diff 20941a4 -- crates/mct-kernel/src/observation.rs` — empty.

Final targeted `--nocapture` runs passed for the additive origin, authority lifecycle, idempotency scope, missed-fire matrix, overlap matrix, admission order, three capacity limits, append failure, crash/re-evaluation, terminal restart, fairness, observation mapping, and the primary resident integration proof.

## Required Part A integration proof — 12-step disk reconstruction

All citations refer to the immutable blob `bf4e922:crates/mct-daemon/src/daemon/resident/trigger_scheduler.rs`.

| Step | Landed proof |
|---:|---|
| 1 | Lines 2044–2108 create isolated config/state/ledger/CAS/children/identity/UDS paths, start the resident under an injected clock, stage a real WIT component through `/artifacts/stage`, and exactly approve/assign it through `/children/approve`. |
| 2 | Lines 2124–2151 submit `/triggers/create` after removing both policy fields from JSON and assert persisted `skip`/`refuse` defaults. Lines 2462–2472 correlate the authority observation before firing evidence. |
| 3 | Lines 2153–2203 advance the injected clock to one nominal occurrence and assert exact revision, policy, occurrence, represented-set, firing, and call identities; lines 2462–2472 prove authority/firing ledger order. |
| 4 | Lines 2166–2203 assert truthful `TriggerFiring` execution, exact target/caller, deterministic call/firing identities, ordinary route presence, and the authority-bound static CAS call. |
| 5 | Lines 2173–2204 assert successful resident completion, a selected route, exact existing `result-resident:<call-id>` reference, and release of the active trigger slot only after result projection. |
| 6 | Lines 2207–2219 leave the injected clock on the same nominal instant across repeated scheduler polls and assert exactly one trigger run. |
| 7 | Lines 1949–2041 construct the exact append/project crash seam: a later firing fact is durable without spawning and the next deterministic occurrence is admitted as overlap-pending. Lines 2221–2234 assert its reason and sequence. |
| 8 | Lines 2246–2301 delete all trigger projections, start the resident, and assert startup ledger reconstruction preserves revision two plus exact firing/pending identities and admission order before use. |
| 9 | Lines 2236–2244 revoke revision one before restart/next work; lines 2345–2377 prove no second successful target effect and one revision-two terminal suppression. |
| 10 | Lines 2382–2455 restart again, wait for stale active/pending reconciliation, and assert one successful effect, a denied recovered crash firing, no live pending row, terminal suppression, and zero active slots. |
| 11 | Lines 2262–2343 restart with the trigger-only active budget saturated, submit an ordinary authenticated `/calls` request successfully, and read `/status` while trigger work remains blocked. |
| 12 | Lines 2457–2492 reopen the ledger and correlate authority, firing, route, result, pending-admission, and revocation references while asserting raw idempotency keys, inline payload bytes, and target result bytes are absent. |

No integration step is waived. The helper at lines 1949–2041 intentionally stops after durable firing/pending projection and before child spawn; this is the required deterministic crash seam, not a synthetic claim of target execution.

## Additional failure and policy evidence

- Additive origin and historical wire stability: `mct_kernel::call::tests::trigger_firing_origin_is_additive_local_and_single_hop`.
- Closed trigger validation/defaults and deterministic identities: `mct_kernel::trigger::tests::*`.
- Owner-authenticated ledger-first create/revise/revoke and closed event/registry/network deferrals: `triggers::tests::*`.
- Revisioned/non-resurrecting schema-v8 state and in-flight recovery: `state::tests::*trigger*`.
- Caller-submitted provenance refusal and exact replay scoping: `resident::local_ingress::tests::locally_submitted_body_cannot_claim_trigger_firing_context` and `resident::idempotency::tests::trigger_firing_idempotency_is_record_and_occurrence_scoped`.
- Missed-fire, overlap, capacity, admission-order, append-failure, terminal, and fairness matrices: the named `resident::trigger_scheduler::tests::trigger_*` tests.
- Ledger rebuild and no duplicate child effect: both resident temporal integration tests, with the acquisition-backed WIT/UDS test as primary proof.
- Observation vocabulary: `trigger_observation_mapping_uses_existing_kinds` plus the empty `ObservationKind` diff.

Track 3 dispositions are in `layer/surface/build/spec-drift-audit/track3/LEDGER.md`; there are no implicit waivers. `MotherEventSourceAdapterRuntime`, `RegistrySyncTriggerComposition`, and `NetworkArtifactAcquisitionAdapter` remain exact named deferrals with no executable path.

## Boundary statement

Part A proves durable temporal trigger authority and resident scheduling. It does **not** implement Watch authority, filesystem observation, WASM child call-out, the legacy watch ABI, fixtures two/three, or the full three-fixture `patinaMother` replacement proof. Those remain Replacement Slice 4B.

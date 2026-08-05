---
type: feat
id: ledger-commit-recovery
status: active
created: 2026-08-03
target: mct-ledger-commit-recovery-phase-h
sessions:
  origin: 20260724-223731-101286000
  work:
    - 20260724-223731-101286000
related:
  - layer/allium/mct-product-map.allium
  - layer/surface/build/feat/grants-authority-v0/SPEC.md
  - layer/surface/build/spec-drift-audit/track3/LEDGER.md
  - layer/core/safety-boundaries.md
  - layer/core/spec-driven-design.md
  - crates/mct-observation/src/lib.rs
  - crates/mct-daemon/src/daemon/resident/observation.rs
exit_criteria:
  - id: maximal-valid-prefix
    text: Reopen classifies the maximal surviving validated prefix as canonical, distinguishes empty and operationally unavailable ledgers, and resumes from the exact committed head.
    checked: true
    verify: Required proof steps 1 and 8 have landed test file and line citations.
  - id: forensic-residue-recovery
    text: An unterminated final frame is preserved with the complete ratified forensic record before only its bytes are set aside, recovery is idempotent, and append resumes from the unchanged committed chain.
    checked: true
    verify: Required proof steps 2, 3, and 13 have landed test file and line citations.
  - id: typed-quarantine
    text: Terminated malformed frames, hash breaks, sequence discontinuities, and foreign lineage preserve evidence and produce typed quarantine without truncation, skipping, renumbering, or automatic adoption.
    checked: true
    verify: Required proof steps 4-7 have landed test file and line citations.
  - id: poisoned-writer-and-batch-outcomes
    text: Write or durability uncertainty poisons the writer, later appends do not touch the file, exclusive reopen resolves the uncertain fact, and batch failure reports its committed prefix without rollback.
    checked: true
    verify: Required proof steps 9-11 have landed test file and line citations.
  - id: exclusive-contention-and-before-effect
    text: A second writer fails fast without recovery or mutation, entry content cannot forge framing, and a failed or uncertain BeforeEffect append suppresses the protected Child effect.
    checked: true
    verify: Required proof steps 12, 14, and 15 have landed test file and line citations.
  - id: law-attribution-and-validation
    text: Review 2 law is valid and attributed, every implementation commit and the final phase pass workspace validation, and the phase flake log records the trigger-scheduler collision disposition.
    checked: true
    verify: allium check layer/allium && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
  - id: h2-authority-tenure-and-mutations
    text: Every exclusive authority writer tenure commits a fresh entropy-backed epoch before mutation admission, and replay-complete mutation facts return typed outcomes with same-ID resolution.
    checked: true
    verify: Phase H2 proof steps 1-7 have landed file/line citations and assertions.
  - id: h2-legacy-import
    text: Existing Toy authority enters canonical history only through the one-time owner-gated import, with import-required and already-imported outcomes proved.
    checked: true
    verify: Phase H2 proof step 8 has landed file/line citations and assertions.
  - id: h2-coherent-projection-proof
    text: One authority-wide cursor coherently reaches the full ledger head and exposes an unconsumed typed D-G8 proof over source, identity, state, and projection hashes.
    checked: true
    verify: Phase H2 proof steps 9-11 have landed file/line citations and assertions.
  - id: h2-rebuild-and-quarantine
    text: Shadow rebuild is atomic and replay-equivalent, committed facts survive projection failure, and quarantine advances no prior projected truth.
    checked: true
    verify: Phase H2 proof steps 12-15 have landed file/line citations and assertions.
  - id: h2-validation-and-fence
    text: Every H2 implementation commit and close-out pass workspace tests, warnings-denied clippy, Tier 0/RustSec, and Allium while R2-L5/R2-L6 and grants slices 4-8 remain untouched.
    checked: true
    verify: allium check layer/allium && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
---

# feat: MCT ledger commit and recovery — Phase H

> Canonical ledger truth is the maximal surviving validated prefix; uncertain writes fence the writer, while narrowly proven final-frame residue is preserved before recovery.

## Scope

Phase H contains two gated tasks:

1. **Task A — law tend:** ratify Review 2 decisions, land observable ledger/recovery and later authority law, and attribute every new invariant.
2. **Task B — R2-L1/R2-L2:** implement maximal valid-prefix scanning, forensic tail recovery, typed quarantine, writer poisoning, typed contention, and partial batch outcomes in `mct-observation` with minimal daemon integration.

Task B begins only after the operator ratifies Gate G1. R2-L3 through R2-L6 and grants-authority slices 4 through 8 remain parked.

## Commit definition

A ledger fact is COMMITTED when durability is acknowledged, OR when
recovery finds its complete, framed, identity-valid, sequence-valid,
hash-valid frame in the maximal surviving validated prefix. Canonical
truth is that prefix. A complete surviving frame is committed even if its
caller never received success; an unterminated frame is never a fact even
if its bytes happen to parse.

## Residue versus corruption

Only an UNTERMINATED final frame is automatic crash residue. A TERMINATED
malformed frame — anywhere, including final position — is corruption.
Residue is preserved (exact bytes, source identity, offset/length,
digest, last committed head, failure class, recovery decision id/time)
durably BEFORE being set aside; recovery is idempotent and never rewrites
a committed entry. Corruption is never skipped, truncated, renumbered, or
silently repaired: the entire ledger is preserved with diagnostic proof
and the ledger is quarantined.

## Writer law

Exactly one exclusive, fail-fast writer per ledger. A contending writer
receives a typed contention result and changes no ledger, projection, or
recovery state — recovery runs only under the exclusive lock. Any write
or durability failure after bytes may have been offered POISONS the
writer: every later append fails deterministically without touching the
file, until close, exclusive reopen, and full rescan resolve the
uncertain fact three ways (committed / residue / quarantine). Batch
appends: a committed prefix stands; never roll back committed entries.
BeforeEffect: a protected effect does not begin when any required
BeforeEffect fact has an unsuccessful or uncertain acknowledgement.

## Ratified owner decisions

The following decisions are settled and reproduced verbatim from operator adjudication.

### D-R2.1

Fresh authority epoch on every exclusive writer tenure (law now,
implementation in R2-L3).

### D-R2.2

A quarantined Mother may expose an isolated read-only
health/forensic plane — the existing owner-only UDS with every mutation
and authority surface disabled (law now, implementation in R2-L5).

### D-R2.3

Existing config/SQLite grant state migrates via one-time
operator-gated import committed as canonical facts (R2-L3).

### D-R2.4

Forensic artifacts are retained indefinitely under the Mother's
data directory, owner-only permissions, no automatic export.

### D-R2.5

Authority mutation results are typed: committed,
committed_projection_pending, commit_unknown, rejected_before_commit;
same-mutation-ID retry resolves, a new ID must not guess around
commit_unknown (law now, implementation in R2-L3).

### D-R2.6

Writer contention is fail-fast; ordinary supervisor restart
policy is the retry mechanism — no bespoke handoff protocol.

### D-R2.7

The first authority-wide projection covers Toy catalog/grants
plus grant-shaping source facts (R2-L4).

### D-R2.8

A virgin Mother is the absence of EVERY durable artifact the
daemon writes (ledger, SQLite state, recorded Mother identity,
supervisor lifecycle marker); presence of any one forces an operator
gate (R2-L5).

## Task B exit criteria

### B1 — R2-L1 maximal valid-prefix scanner and forensic recovery

- Clean validated prefix opens ready at its exact sequence and previous hash.
- Empty and operational unavailability are typed distinctly; bootstrap gating remains deferred.
- An unterminated final frame is classified as residue, preserved with the complete D-R2.4 forensic record before set-aside, and followed by a recovery observation.
- Recovery remains idempotent across interruption at every preservation stage and never rewrites a committed entry.
- A terminated malformed frame, hash break, sequence gap/duplicate/regression, or identity mismatch preserves the required evidence and returns typed quarantine or foreign lineage without changing the ledger.
- Recovery runs only under exclusive writer ownership. Read-only access classifies but does not recover.
- Existing on-disk newline-delimited entry schema remains unchanged.

### B2 — R2-L2 writer lifecycle, contention, and commit outcomes

- Any write or durability failure after bytes may have been offered poisons the writer.
- Every append after poisoning fails deterministically without touching the ledger until close, exclusive reopen, and full rescan.
- Reopen resolves an uncertain append as committed, residue, or quarantine according to the commit definition.
- A contending writer receives a typed fail-fast result and performs no recovery, ledger mutation, projection mutation, or forensic-artifact mutation.
- Batch failure returns a typed partial outcome naming its committed prefix; committed entries are never rolled back.
- BeforeEffect callers suppress their protected effect on unsuccessful or uncertain append acknowledgement.
- Parallel tests isolate ledger paths and join writer shutdown rather than retrying lock contention.

## Required proof steps

Each proof step must land as a cited test. Close-out requires the test file and line plus its verbatim assertion.

1. Crash before frame bytes: reopen sees the previous head, no new fact.
2. Torn unterminated tail: recovery preserves exact bytes and forensic record; committed chain unchanged; appends resume at correct sequence and previous-hash.
3. Unparseable unterminated final frame: classified residue, same rule.
4. Terminated malformed frame (including final position): quarantine; no truncation; entire ledger preserved with diagnostics.
5. Hash-chain break mid-file: quarantine with first-bad sequence/offset and expected-vs-observed evidence.
6. Sequence gap, duplicate, and regression: quarantine; never renumber or skip.
7. Wrong ledger or Mother identity: typed foreign-lineage state; no automatic adoption.
8. Complete valid final frame with unacknowledged commit: rescan reports it committed; the writer resumes after it; no duplicate.
9. Write/sync failure poisons: every subsequent append fails without modifying the file (file digest identical before/after attempts).
10. Poisoned writer reopen+rescan resolves all three outcomes (committed / residue / quarantine) correctly.
11. Batch failure after a committed prefix: prefix stands; typed partial outcome; no rollback.
12. Second writer during an active writer: typed contention; ledger file and any recovery artifacts byte-identical.
13. Recovery interrupted at every preservation stage: original or preserved bytes always available; rerun is idempotent.
14. Entry encoding cannot forge a frame end: entries containing every escapable character (raw newlines, control bytes, quotes) round-trip with no interior unescaped terminator.
15. Daemon BeforeEffect path: append failure before a Child effect produces no external effect and returns a typed observation-unavailable outcome to the caller.

## Deferral fence

The following work must not be implemented, even partially, in Phase H:

- No authority epoch facts or epoch establishment (R2-L3).
- No canonical mutation envelope, mutation IDs, or config-as-intent migration (R2-L3).
- No authority-wide projection cursor, rebuild, or D-G8 proof (R2-L4).
- No startup degraded-deny plane, bootstrap gate, or generalized trust-path correlation (R2-L5).
- No mutation/effect ordering boundary (R2-L6).
- Grants-authority slices 4-8 remain parked. The vacuous resident grants guard remains untouched.
- The on-disk entry schema does not change in this phase.

If Task B appears to require crossing this fence, implementation stops and reports the fork rather than improvising.

## Validation and close-out

Every implementation commit and the final phase must pass:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

A failing test is rerun in isolation up to five times. A non-reproducing failure is retained verbatim in the phase flake log; a reproducing failure is fixed before proceeding.

Close-out reconstructs the commit ledger, all fifteen proof citations and verbatim assertions, full validation transcript, flake log, and invariant dispositions from disk. The active session may be updated but must not be archived or ended.

## Gate G1 ratification

The operator ratified Gate G1 without amendment after verifying commits `1c1a2a3..9fa6506`, all 27 invariants, the `MctProjectionCursor` extension, the 10/17+1 Track 3 partition, and the full-ledger-replay seam. Rust work began only after that ratification.

## Commit ledger

| Task | Commit | Disposition |
|---|---|---|
| A1 | `1c1a2a3 spec(ledger): ratify Review 2 commit/recovery design D-R2.1..D-R2.8` | Ratified design, exit criteria, proof matrix, and fence. |
| A2 | `a0e5608 spec(allium): land ledger commit/recovery and authority epoch law (Review 2)` | 27 invariants and projection-cursor law; Allium clean. |
| A3 | `9fa6506 docs(ledger): attribute Review 2 invariants` | Initial 0/10/17 plus one structural deferral. |
| B1 test | `baf6f03 test(ledger): specify maximal-prefix recovery and quarantine` | Failing-test-first proofs 1-8, 13, and 14. |
| B1 implementation | `b1f1df3 fix(ledger): recover torn tails and quarantine corruption` | Maximal-prefix scan, private durable forensics, idempotent recovery observation, typed quarantine/foreign lineage. |
| B2 test | `cf62f15 test(ledger): specify poisoned-writer and contention outcomes` | Failing-test-first proofs 9-12 and 15. |
| B2 implementation | `a292152 fix(ledger): fence uncertain writers and isolate ledger tests` | Poison fencing, typed uncertainty/contention/partial batches, joined resident shutdown, retry-loop removal. |

## Fifteen-step proof table

Line citations name the landed test and quote its central assertion verbatim.

| # | Test citation | Verbatim assertion |
|---:|---|---|
| 1 | `crates/mct-observation/src/lib.rs:1595-1615` — `crash_before_frame_bytes_reopens_at_previous_head` | `assert_eq!(std::fs::read(&path).unwrap(), before);` and `assert_eq!(next.local_sequence, 1);` |
| 2 | `crates/mct-observation/src/lib.rs:1619-1662` — `torn_unterminated_tail_is_preserved_and_recovered` | `assert_eq!(&std::fs::read(&path).unwrap()[..committed.len()], committed);` and `assert_eq!(next.local_sequence, 2);` |
| 3 | `crates/mct-observation/src/lib.rs:1666-1682` — `unparseable_unterminated_final_frame_is_residue` | `assert_eq!(std::fs::read(&status.preserved_bytes_path).unwrap(), residue);` |
| 4 | `crates/mct-observation/src/lib.rs:1686-1707` — `terminated_malformed_frame_quarantines_and_preserves_entire_ledger` | `assert_eq!(status.failure_class, LedgerFailureClass::TerminatedMalformedFrame);` and `assert_eq!(std::fs::read(&path).unwrap(), original);` |
| 5 | `crates/mct-observation/src/lib.rs:1711-1753` — `hash_break_quarantines_with_diagnostic_evidence` | `assert_eq!(status.first_bad_sequence, Some(1));`, `assert_eq!(status.first_bad_offset, first_line_length as u64);`, and `assert_eq!(status.observed.as_deref(), Some("forged-entry-hash"));` |
| 6 | `crates/mct-observation/src/lib.rs:1757-1802` — `every_sequence_discontinuity_quarantines_without_repair` | `assert_eq!(status.failure_class, LedgerFailureClass::SequenceDiscontinuity);` and `assert_eq!(std::fs::read(&path).unwrap(), original);` |
| 7 | `crates/mct-observation/src/lib.rs:1806-1825` — `wrong_identity_is_typed_foreign_lineage_without_adoption` | `assert_eq!(status.first_bad_sequence, Some(0));` and `assert_eq!(std::fs::read(&path).unwrap(), original);` |
| 8 | `crates/mct-observation/src/lib.rs:1829-1849` — `complete_unacknowledged_final_frame_is_committed_on_rescan` | `assert_eq!(reopened.entries().unwrap(), vec![unacknowledged.clone()]);` and `assert_eq!(reopened.entries().unwrap().len(), 2);` |
| 9 | `crates/mct-observation/src/lib.rs:1946-1978` — `write_and_sync_uncertainty_poison_writer_without_later_file_changes` | `assert!(ledger.is_poisoned());` and `assert_eq!(std::fs::read(&path).unwrap(), after_failure);` |
| 10 | `crates/mct-observation/src/lib.rs:1982-2041` — `poisoned_writer_reopen_resolves_all_three_commit_states` | `assert!(reopened.recovery_status().is_none());`, `assert!(reopened.recovery_status().is_some());`, and `Err(ObservationLedgerError::Quarantined { .. })`. |
| 11 | `crates/mct-observation/src/lib.rs:2045-2084` — `batch_failure_reports_and_preserves_acknowledged_committed_prefix` | `assert_eq!(outcome.acknowledged_committed_prefix.len(), 1);`, `assert_eq!(outcome.failed_index, 1);`, and `assert!(outcome.commit_unknown);` |
| 12 | `crates/mct-observation/src/lib.rs:2088-2103` — `contending_writer_is_typed_and_byte_identical_without_recovery` | `Err(ObservationLedgerError::WriterContended { .. })`, `assert_eq!(std::fs::read(&path).unwrap(), ledger_before);`, and `assert_eq!(forensic_tree(&path), forensics_before);` |
| 13 | `crates/mct-observation/src/lib.rs:1853-1895` — `interrupted_recovery_is_idempotent_at_every_preservation_stage` | `assert!(original_available || preserved_available);` and `assert_eq!(recovery_observations, 1, "stage {stage:?} duplicated recovery");` |
| 14 | `crates/mct-observation/src/lib.rs:1899-1917` — `escapable_entry_content_round_trips_without_forging_frame_end` | `assert_eq!(bytes.iter().filter(|byte| **byte == b'\n').count(), 1);` |
| 15 | `crates/mct-daemon/src/daemon/resident/pipeline.rs:1018-1056` — `before_effect_append_failure_suppresses_child_effect` | `assert_eq!(result.outcome, CallProtocolOutcome::Failed);`, `assert_eq!(result.safe_message, "observation ledger unavailable");`, and `assert!(!effect_marker.exists(), "unsuccessful BeforeEffect acknowledgement began a Child effect");` |

## Validation transcript

### B1 implementation (`b1f1df3`)

- `cargo test --workspace` — passed: **439 passed, 1 ignored**.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `./scripts/ci-tier0.sh` — passed; RustSec audit clean and Allium clean.
- Flakes: none.

### B2 implementation (`a292152`)

- Targeted trigger lock-isolation test, five consecutive runs — **5/5 passed**.
- `resident::trigger_scheduler::tests`, serial run — **16/16 passed**.
- First `cargo test --workspace` exposed a deterministic compatibility assertion, not a flake: `supervisor_lifecycle::tests::supervisor_conflicts_refuse_before_launchd_or_endpoint_effects` expected the typed contention message to retain `writer lock`. The message was repaired; isolated rerun passed.
- Final `cargo test --workspace` — passed: **444 passed, 1 ignored**.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `./scripts/ci-tier0.sh` — passed; RustSec audit clean and Allium clean.

### Flake disposition

No non-reproducing Phase H failures were observed. The prior trigger-scheduler/observation-ledger collision class did **not** reproduce after unique temporary ledger paths, explicit resident writer shutdown, task join, and removal of lock-contention retry loops: five targeted runs, the full 16-test scheduler group, final workspace validation, and Tier 0 all passed.

## Final invariant disposition

Track 3 now reports:

- **10 COVERED** R2-L1/R2-L2 invariants;
- **0 LAW-LEADS-CODE** Phase H targets;
- **17 DEFERRED** R2-L3..L6 / slices 4-8 invariants;
- **1 DEFERRED structural** `MctProjectionCursor` row.

The full-ledger replay readiness seam remains accepted. The on-disk `MctObservationLedgerEntry` schema is unchanged. Quarantine still refuses writer startup; the degraded read-only Mother plane remains fenced to R2-L5. No authority epoch, canonical mutation envelope, authority-wide projection, startup bootstrap/degraded plane, mutation/effect order, resident grants-guard repair, or grants-authority slice 4-8 work landed.

# Phase H2 — R2-L3/R2-L4 epoch and authority projection

> Every exclusive writer tenure first names fresh canonical authority; every authority mutation is one replay-complete fact; and one coherent projection cursor proves exact coverage of the canonical head without becoming authority itself.

## Scope and gate

Phase H2, after explicit operator ratification of Gate G1, implements:

1. **R2-L3** — writer-tenure epoch establishment, canonical Toy catalog/grant mutation facts, typed mutation outcomes and same-ID resolution, and the one-time operator-gated import required by D-R2.3.
2. **R2-L4** — full-ledger authority replay, coherent authority-state cursor publication, the D-G8 usable-projection proof, and replay-equivalent shadow rebuild.

This schema was Gate G1. The operator ratified these permanent fact schemas before Phase H2 Rust work began. Any amendment is recorded as D-H2.n; D-R2.1 through D-R2.8 remain settled.

Gate G1 specifically ratifies four permanent choices made concrete below: the reserved inline `detail_ref` carrier, a non-resetting generation baseline across epochs, one replay-complete fact per ordinary mutation/import, and the H2/R2-L5 seam where H2 records startup provenance but R2-L5 remains responsible for enforcing D-R2.8's every-artifact virgin/operator gate at Mother startup.

## Ratified Phase H2 amendment

### D-H2.1

`mct-authority-fact-v1:` is a named constant at the carrier parse site. The `MctObservation.detail_ref` field documentation names this reserved inline canonical-authority payload semantics; all unreserved values remain opaque references. This documentation/constant amendment changes neither the entry field schema nor the Phase H2 fence.

### Gate G1 disposition

Ratified before Task B. D-H2.1 landed in `d257515`; no Rust or Cargo work preceded ratification. The operator accepted the inline carrier, non-resetting generation baseline, one-fact mutation/import shape, H2/R2-L5 startup seam, and SQLite/config legacy-import interpretation.

## Binding landed law

The following already-landed Allium law is binding, not reopened by this proposal.

### `MctAuthorityEpochContinuity`

- **EpochBeginsWithCanonicalFact:** “Authority evaluation and advertisement begin only after one committed canonical epoch fact names the Mother, source ledger, predecessor head when known, and generation baseline. A Mother is virgin only when every daemon-written durable artifact is absent; otherwise initialization requires an operator gate.”
- **WriterTenureUsesFreshEpoch:** “Every successful exclusive writer tenure establishes a fresh non-repeating authority epoch before authority evaluation or advertisement becomes available.”
- **EpochTransitionPreservesCurrentGrantMeaning:** “A writer-tenure epoch transition invalidates prior authority tokens without silently widening, narrowing, creating, or removing the current grant set.”
- **RestoredHistoryCannotReuseAuthorityIdentity:** “Ledger replacement, restoration, or reinitialization cannot make a previously used Mother, authority epoch, and generation identity current again.”
- **ProjectionEpochMustMatchCanonicalEpoch:** “Projection facts under an epoch other than the current canonical epoch grant no authority, irrespective of matching generation or record values.”

### `MctCanonicalAuthorityFacts`

- **LedgerFactsAreCanonicalAuthority:** “Current grant and catalog authority derives from committed canonical ledger facts rather than configuration or projection records.”
- **MutationAndGenerationAdvanceAreOneFact:** “One logical authority-shape mutation and its resulting generation advance are represented by one committed canonical fact, never by independently committable authority changes.”
- **AuthorityFactIsReplayComplete:** “A canonical authority fact carries enough structured meaning to reconstruct its resulting authority state; a digest, safe message, or projection row alone is insufficient.”
- **ConfigurationIsIntentNotAuthority:** “Configuration and existing projected grant state may request only a one-time operator-gated import committed as canonical facts; disagreement with the ledger never grants, retains, or revokes authority by itself.”
- **ProjectionFailureDoesNotUndoCommit:** “A committed authority fact remains canonical when projection fails. Mutation results distinguish `committed`, `committed_projection_pending`, `commit_unknown`, and `rejected_before_commit`; authority use denies until coherent projection coverage returns.”

### `MctAuthorityProjectionFreshness`

- **AuthorityProjectionIdentifiesCanonicalSource:** “A grant-authority projection identifies the executing Mother's canonical source ledger and exact grants-authority identity from which its facts were derived.”
- **AuthorityProjectionCoversCurrentGeneration:** “Authority evaluation may use a projection only when an authority-state cursor proves coverage of the current canonical grants-authority generation rather than merely a formerly valid generation. Domain cursors such as trigger or watch checkpoints are diagnostics for their own replay and cannot serve as this authority proof.”
- **ProjectionVersionAndFactsAreCoherent:** “The projected grant facts and the grants-authority identity exposed with them describe one coherent projected state and cannot be assembled from different generations.”
- **UnprovableFreshnessDenies:** “When current canonical generation coverage cannot be proved, authority evaluation denies instead of treating cached, missing, or ambiguous projection state as current.”
- **AuthorityCursorReachesCanonicalHead:** “A current authority-state projection proves it processed the canonical ledger through its committed head, including entries that do not themselves change authority. The first such projection covers Toy catalog and grants plus their grant-shaping source facts.”
- **CursorBindsHeadHashAndAuthorityIdentity:** “Authority coverage binds the source Mother, source ledger, committed sequence and entry hash, authority epoch, generation, and source authority observation as one proof.”
- **ProjectionFactsAndCursorBecomeVisibleTogether:** “Authority facts, projection hash, projection status, and cursor describe one coherent visible state; a new cursor cannot expose old facts and new facts cannot expose an old cursor.”
- **EpochMismatchDenies:** “Projection facts from another authority epoch grant no authority even when their generation, source sequence, or record digests happen to match.”
- **RebuildEqualsReplay:** “Rebuilding from the same validated committed history produces the same authority facts, authority identity, and projection hash as uninterrupted incremental replay.”

The extended `MctProjectionCursor` is likewise binding: an `authority_state` row names projection id/kind, source Mother and ledger, through-sequence/observation/hash, complete `GrantsAuthorityIdentity`, projection hash/status, and update time.

## Phase-critical constraints

- **C1 — ledger facts are forever.** Every new canonical payload has an explicit versioned kind, excludes incidental implementation fields, and contains structured replay-complete content. Digests and `safe_message` values are checks or summaries, never substitutes for the fact.
- **C2 — write path only.** Phase H2 changes recording, replay, projection, and proof construction. It does not change any authority-evaluation reader or add a consumer of the D-G8 proof.
- **C3 — transitional dual-write order.** A Toy authority-shape mutation commits its canonical fact first. Only acknowledged commitment permits the existing legacy SQLite/config projection write. `rejected_before_commit`, `commit_unknown`, and poisoned/fenced writers suppress that write. A crash may leave legacy state behind canonical history.
- **C4 — unpredictable epoch.** Each epoch is 256 bits obtained from operating-system entropy and encoded as 64 lowercase hexadecimal characters prefixed `mct-authority-epoch-v1:`. Time, sequence, process identity, prior epoch, and deterministic hashes are forbidden epoch inputs.

## Canonical fact carrier and encoding

The newline-delimited `MctObservationLedgerEntry` framing and every existing entry/observation field remain unchanged. Canonical authority payloads use the reserved existing `MctObservation.detail_ref` carrier:

```text
mct-authority-fact-v1:<canonical-json>
```

Only this prefix changes `detail_ref` from an opaque diagnostic reference into an inline canonical authority payload. Other `detail_ref` values retain their existing non-authoritative meaning. The payload is a UTF-8 JSON object with lexicographically ordered object keys, integer numbers only, explicit tagged variants, omitted absent optionals, and arrays ordered as specified below. The ledger entry hash binds the complete encoded payload. Replay rejects an unknown `schema`, unknown `fact_kind`, duplicate identity, malformed payload, or payload/observation-field disagreement; it never infers authority from the observation's safe summary, kind, revision conveniences, or digest alone.

Every canonical authority fact uses:

```text
CanonicalAuthorityFactV1 {
  schema: "mct-authority-fact/v1",
  fact_kind: "epoch_established" | "authority_mutation" | "legacy_authority_import",
  fact_id: String,
  body: EpochEstablishedFactV1 | AuthorityMutationFactV1 | LegacyAuthorityImportFactV1
}
```

`fact_id` equals the containing observation's `observation_id`. Every `GrantsAuthorityIdentity` has exactly `mother_node_id`, `authority_epoch`, `generation`, and `source_authority_observation_id`. The containing observation is `BeforeEffect` and `NodeOperator`, has `subject_id = mother_node_id`, `resource_id = ledger_id`, and carries the resulting generation in `grants_revision`. Epoch establishment uses `LifecycleTransitionRecorded`/`Storage`; ordinary mutation uses `OperatorActionRecorded`/`Kernel`; import uses `OperatorActionRecorded`/`Operator`. Any disagreement makes authority replay fail closed without advancing or publishing a cursor; only the already-ratified R2-L1 ledger corruption classes place the ledger itself in quarantine.

## Ratified epoch-establishment schema

```text
EpochEstablishedFactV1 {
  mother_node_id: String,
  ledger_id: String,
  authority_epoch: String,
  predecessor: none_for_virgin | none_after_operator_reinitialization | validated_head {
    sequence: u64,
    entry_hash: String
  },
  generation_baseline: u64,
  prior_authority: GrantsAuthorityIdentity?,
  resulting_authority: GrantsAuthorityIdentity,
  grant_state_hash: String,
  establishment: writer_tenure {
    started_at: Timestamp,
    startup_class: virgin | ordinary_reopen | legacy_ledger_upgrade | operator_gated_nonvirgin,
    operator_gate_decision_id: String?,
    authenticated_principal_ref: String?
  }
}
```

Rules:

- `authority_epoch` satisfies C4 and is fresh for every successfully acquired exclusive tenure.
- `none_for_virgin` is legal only when the validated ledger has no prior entry and the D-R2.8 virgin predicate has been established by the later R2-L5 gate. H2 records the value but does not implement that startup gate.
- `validated_head` exactly equals the sequence/hash immediately preceding this fact. A copied or restored ledger therefore names its copied head but receives a new entropy-derived epoch.
- The baseline is `0` exactly when canonical replay yields no prior authority. When canonical replay yields prior authority, the baseline equals its latest generation; within that surviving canonical history it never decreases or resets merely because the epoch changes.
- `prior_authority` is absent exactly when canonical replay yields no prior authority, regardless of ledger emptiness. `resulting_authority` names this Mother, the fresh epoch, `generation_baseline`, and this epoch fact's observation id.
- `grant_state_hash` is the deterministic hash of the complete current Toy catalog/grant state before and after the epoch transition. Equality proves that epoch transition itself changed identity but not grant meaning.
- The epoch fact is the first append made by the newly exclusive writer and must be acknowledged before that writer exposes its epoch or accepts an authority mutation. Failure or uncertainty poisons/fences the tenure under R2-L2.

## Ratified canonical authority-mutation schema

```text
AuthorityMutationFactV1 {
  mutation_id: String,
  mutation_intent_hash: String,
  mother_node_id: String,
  ledger_id: String,
  authority_epoch: String,
  prior_state: {
    grants_authority: GrantsAuthorityIdentity,
    authority_state_hash: String
  },
  changes: [AuthorityChangeV1, ...],
  grant_shaping_sources: [GrantShapingSourceV1, ...],
  resulting_state: {
    grants_authority: GrantsAuthorityIdentity,
    authority_state_hash: String
  },
  decided_at: Timestamp
}
```

`changes` is non-empty and ordered canonically by `(change_kind, stable identity)`. One fact may carry the complete atomic set produced by one control request; its resulting generation is exactly prior generation plus one. `mutation_intent_hash` hashes only the canonical `changes` and `grant_shaping_sources`, allowing a retry to distinguish the same intent from mutation-ID reuse without replacing the structured content.

```text
AuthorityChangeV1 =
  toy_catalog_put {
    toy_id: String,
    contract: {
      namespace: String,
      interface_name: String,
      version: String,
      function_name: String?,
      resource_name: String?
    },
    authority_bearing: bool,
    catalog_revision: u64,
    admitted_by_observation_id: String
  }
| toy_catalog_remove {
    toy_id: String
  }
| toy_grant_put {
    grant_id: String,
    toy_id: String,
    subject: {
      child_name: String,
      artifact_id: String,
      artifact_version: String,
      assignment_id: String?,
      caller_node_id: String?
    },
    scope: {
      vision_id: String,
      node_id: String?,
      project_id: String?,
      data_classification: String?,
      resource_id: String?,
      allowed_actions: [String, ...]
    },
    constraints: {
      starts_at: Timestamp?,
      expires_at: Timestamp?,
      max_uses: u64?,
      max_duration_ms: u64?,
      locality_required: bool
    },
    grant_state: requested | active | expired | revoked | superseded | denied,
    issuer_id: String,
    policy_revision: u64,
    source_grants_revision: u64,
    authority_observation_id: String
  }
| toy_grant_remove {
    grant_id: String
  }
```

`allowed_actions` is sorted and duplicate-free. A `put` is the entire resulting immutable value, not a partial patch. A remove names the exact stable identity and is valid only when the prior state contains it. Replay applies every change to the declared prior state, verifies the resulting state hash and generation, and rejects gaps, duplicate stable identities, wrong epochs, prior-state mismatches, and non-monotonic generation.

```text
GrantShapingSourceV1 =
  operator_decision {
    decision_id: String,
    authenticated_principal_ref: String,
    command_kind: authorize_slate | authorize_secret | catalog_change | grant_change
  }
| child_approval {
    child_name: String,
    artifact_id: String,
    artifact_version: String,
    authority_observation_id: String
  }
| child_assignment {
    assignment_id: String,
    authority_observation_id: String
  }
```

These source records preserve why the structured catalog/grant values were shaped as written. They do not independently grant Child or Toy authority and do not expand the D-R2.7 projection scope.

## Ratified typed mutation result and same-ID resolution

The control boundary returns the following tagged value; impossible field combinations are not represented:

```text
AuthorityMutationResultV1 =
  committed {
    mutation_id: String,
    resolution: newly_committed | resolved_existing_fact,
    fact_sequence: u64,
    fact_entry_hash: String,
    grants_authority: GrantsAuthorityIdentity,
    projection_hash: String
  }
| committed_projection_pending {
    mutation_id: String,
    resolution: newly_committed | resolved_existing_fact,
    fact_sequence: u64,
    fact_entry_hash: String,
    grants_authority: GrantsAuthorityIdentity,
    pending_reason: projection_not_attempted | projection_failed | projection_stale
  }
| commit_unknown {
    mutation_id: String,
    attempted_intent_hash: String,
    failure_stage: write | durability | acknowledgement
  }
| rejected_before_commit {
    mutation_id: String,
    reason: invalid_request | import_required | authority_epoch_unavailable |
            prior_state_mismatch | mutation_id_conflict | writer_contended |
            writer_poisoned | legacy_snapshot_changed | already_imported
  }
```

Semantics:

- `committed` means both the fact and a coherent projection through the then-current committed head are published. `committed_projection_pending` means the fact is canonical but projection publication is not proved.
- `commit_unknown` claims no sequence, entry hash, resulting authority, or legacy effect. It poisons/fences the writer and suppresses the legacy write.
- `rejected_before_commit` means no authority fact was offered and no legacy write occurred.
- A retry must carry the same `mutation_id` and intent. After exclusive reopen/full rescan: exactly one matching committed fact resolves as `resolved_existing_fact`; clean absence permits one append of that same intent; a different intent under the ID is `mutation_id_conflict`; residue/quarantine follows R2-L1/R2-L2 and cannot be guessed around with a new ID.
- The legacy write occurs only after an acknowledged fact. Its failure cannot change the fact result back to rejection or unknown; it produces `committed_projection_pending`.

## Ratified one-time operator import schema

### Request

```text
LegacyAuthorityImportRequestV1 {
  schema: "mct-legacy-authority-import-request/v1",
  import_id: String,
  expected_mother_node_id: String,
  expected_ledger_id: String,
  expected_config_authority_hash: String,
  expected_sqlite_authority_hash: String,
  confirmation: "import-existing-toy-authority-as-canonical-v1"
}
```

The authenticated local owner is derived from UDS peer credentials and is never accepted from the body. The two expected hashes cover normalized Toy catalog/grant authority content only, not paths, timestamps, SQLite layout, or unrelated configuration.

### Gate

Import proceeds only when:

1. an exclusive ready writer has an acknowledged current epoch;
2. request Mother/ledger identities match that writer;
3. the authenticated principal is the owner-authorized local operator;
4. the confirmation string is exact;
5. normalized config and SQLite authority hashes equal the request;
6. no canonical `legacy_authority_import` fact exists; and
7. no ordinary canonical authority mutation already claims the legacy state.

If legacy Toy authority rows exist and no import fact covers them, ordinary enveloped mutation refuses `import_required`. An empty legacy state may be imported explicitly; once imported, the marker remains one-time. A second import returns typed `already_imported` with the original fact reference and appends nothing.

### Recorded decision

```text
LegacyAuthorityImportFactV1 {
  import_id: String,
  mother_node_id: String,
  ledger_id: String,
  authority_epoch: String,
  operator_decision: {
    decision_id: String,
    authenticated_principal_ref: String,
    confirmation: "import-existing-toy-authority-as-canonical-v1",
    decided_at: Timestamp
  },
  source_evidence: {
    config_authority_hash: String,
    sqlite_authority_hash: String
  },
  imported_state: {
    toy_catalog: [complete toy_catalog_put values, sorted by toy_id],
    toy_grants: [complete toy_grant_put values, sorted by grant_id]
  },
  prior_state: {
    grants_authority: GrantsAuthorityIdentity,
    authority_state_hash: String
  },
  resulting_state: {
    grants_authority: GrantsAuthorityIdentity,
    authority_state_hash: String
  }
}
```

The import is one fact and one generation advance. Its complete snapshot is replay authority; source hashes merely prove the operator-approved legacy inputs. Oversized or internally inconsistent legacy state rejects before append rather than being split into a partially canonical import. The containing observation is the recorded operator decision.

## Ratified authority projection rows and coherent publication

R2-L4 adds one live authority projection and replay evidence. The observable cursor row is:

```text
AuthorityProjectionCursorRowV1 {
  schema_version: 1,
  projection_id: "authority-state-v1",
  projection_kind: authority_state,
  source_mother_node_id: String,
  source_ledger_id: String,
  through_sequence: u64,
  through_observation_id: String,
  through_entry_hash: String,
  grants_authority: GrantsAuthorityIdentity,
  authority_state_hash: String,
  projection_hash: String,
  projection_status: rebuilding | current | stale | quarantined,
  updated_at: Timestamp
}
```

The live projection contains:

- one row per complete current Toy catalog value;
- one row per complete current Toy grant value;
- one immutable replay row per decoded canonical epoch/mutation/import fact, binding fact id, kind, source sequence, source entry hash, and canonical payload;
- one current cursor row above.

`authority_state_hash` is BLAKE3 over canonical JSON containing sorted complete Toy catalog and grant values. `projection_hash` is BLAKE3 over canonical JSON containing schema version, source Mother/ledger, through-sequence/observation/hash, complete `GrantsAuthorityIdentity`, `authority_state_hash`, and projection status. Thus a non-authority entry advances the cursor and changes `projection_hash` without changing `authority_state_hash` or generation.

Projected facts, current Toy rows, hashes, status, and cursor become visible in one SQLite transaction. Incremental replay consumes every validated ledger entry in sequence; non-authority entries update only the through-head fields and projection hash. A reader sees old facts with the old cursor or new facts with the new cursor, never a mixed generation. Shadow rebuild constructs and validates a complete candidate away from the live rows, then replaces facts and cursor in one publication transaction. A quarantined source publishes only typed `quarantined` status without advancing the prior through-head or replacing prior facts.

## Ratified usable-projection proof

R2-L4 exposes but does not consume:

```text
UsableAuthorityProjectionProofV1 =
  usable {
    cursor: AuthorityProjectionCursorRowV1
  }
| denied {
    reason: projection_missing | projection_not_current | wrong_source_mother |
            wrong_source_ledger | head_sequence_mismatch | head_hash_mismatch |
            authority_mother_mismatch | epoch_mismatch | generation_mismatch |
            source_authority_observation_mismatch | authority_state_hash_mismatch |
            projection_hash_mismatch | ledger_quarantined
  }
```

The check independently recomputes projection hash and compares canonical source Mother, ledger, committed head sequence/hash, epoch, generation, source authority observation, and authority-state hash. It never repairs, refreshes, or authorizes. Slice 5 is the first consumer.

## Phase H2 required proof steps

Each step must land as a named test and close-out must cite its file/line and verbatim central assertion.

1. Fresh tenure commits the epoch fact before any mutation is accepted; fields match the ratified schema.
2. Two tenures yield distinct entropy-derived epochs; a byte-copied ledger+projection restore still gets a fresh epoch next tenure.
3. Epoch identity is replay-complete: rebuild from ledger bytes alone reproduces it.
4. Enveloped mutation commits its canonical fact before the legacy state write; structured content alone reconstructs resulting state.
5. Injected `commit_unknown` suppresses legacy state; same-ID retry after reopen resolves without a duplicate fact.
6. `rejected_before_commit` leaves no fact and no legacy change.
7. `committed_projection_pending` leaves a committed fact, a behind projection, and a result distinct from `committed`.
8. Import converts legacy grant state into the complete canonical import fact and recorded operator decision; second import is typed `already_imported`; pre-import mutation of unimported state refuses.
9. Cursor advances through non-authority entries to the committed head; concurrent readers see old-old or new-new facts/cursor, never mixed state.
10. Usable-projection proof passes on full match; wrong Mother, stale head, wrong epoch, and wrong projection hash each return their typed deny reason.
11. Across restart, projected grant meaning and `authority_state_hash` are identical while epoch and complete authority identity differ.
12. Stale, epoch-mismatched, or hash-incoherent projection rebuilds in shadow and publishes atomically without exposing partial replacement.
13. Clean rebuild and incremental replay of identical ledger bytes produce identical authority facts, identity, state hash, and projection hash.
14. Projection failure after committed mutation leaves the fact canonical, returns pending, and catches up without undoing commitment.
15. Quarantined ledger refuses projection advancement with typed `ledger_quarantined`/`quarantined` status.

## Phase H2 commit plan

1. `spec(ledger): propose Phase H2 epoch and mutation fact schemas`
2. `feat(ledger): establish authority epoch per writer tenure`
3. `feat(ledger): canonical authority mutation envelope with typed commit results`
4. `feat(control): operator-gated import of grant state into canonical facts`
5. `feat(projection): authority-wide cursor with coherent publication`
6. `feat(projection): usable-projection proof for D-G8`
7. `feat(projection): shadow rebuild with replay equivalence`

Every implementation commit and the final phase must pass the workspace test, warnings-denied clippy, Tier 0/RustSec, and Allium checks under the established flake protocol.

## Updated deferral fence

R2-L3 and R2-L4 leave the Phase H fence only after Gate G1 ratification. The following remain forbidden, including partial implementation:

- No authority-evaluation read-path changes. Route evaluation, resident grants guard, Toy grant evaluation, host adapters, and hello retain current sources; the vacuous resident grants guard remains untouched.
- No R2-L5 startup degraded-deny plane, D-R2.8 bootstrap gate, config-drift exposure, or generalized trust-path correlation.
- No R2-L6 mutation/effect ordering boundary; the two `TwoPhaseRouting` ordering invariants remain deferred.
- No hello/call peer wire change.
- No grants-authority slice 4 generation advancement from route or Toy evaluation. Slice 4 later consumes this envelope and cursor; slices 5/7/8 consume the proof and current snapshots.
- No changes to the Phase H newline framing or `MctObservationLedgerEntry`/`MctObservation` field schema.

If implementation requires any fenced reader or behavior, work stops and reports the design fork for a D-H2.n amendment.

## Phase H2 close-out

### Landed commits

1. `6f62404 spec(ledger): propose Phase H2 epoch and mutation fact schemas`
2. `d257515 spec(ledger): record D-H2.1 carrier semantics`
3. `d1d8ce6 feat(ledger): establish authority epoch per writer tenure`
4. `4f57175 feat(ledger): canonical authority mutation envelope with typed commit results`
5. `535c3f9 feat(control): operator-gated import of grant state into canonical facts`
6. `79aad8b feat(projection): authority-wide cursor with coherent publication`
7. `ba047d9 feat(projection): usable-projection proof for D-G8`
8. `09f6b06 feat(projection): shadow rebuild with replay equivalence`

No implementation commit changed an authority-evaluation reader, resident grants guard, Toy effect evaluator, host adapter, hello/call wire value, R2-L5 startup plane, R2-L6 ordering boundary, or grants-authority slice 4-8 consumer.

### Required proof citations and verbatim central assertions

1. **Epoch before mutation admission** — `crates/mct-observation/src/lib.rs:3768`, central assertion at line 3778: `assert_eq!(entries, vec![tenure.entry.clone()]);`
2. **Distinct entropy-backed tenures and restore** — `crates/mct-observation/src/lib.rs:3798`, central assertions beginning at line 3836: `assert_ne!(first_epoch, second_epoch);`, `assert_ne!(&first_epoch, restored_epoch);`, and `assert_ne!(&second_epoch, restored_epoch);`
3. **Epoch replay from ledger bytes** — `crates/mct-observation/src/lib.rs:3843`, central assertion at line 3859: `assert_eq!(replayed.current_authority, Some(expected));`
4. **Canonical fact before legacy write; replay-complete mutation** — `crates/mct-observation/src/lib.rs:3649`, assertion executed inside the legacy-write closure at line 3657: `assert_eq!(replay_authority_entries(&entries).unwrap().state, *state);`
5. **Commit unknown suppresses legacy and same-ID retry deduplicates** — `crates/mct-observation/src/lib.rs:3674`, central assertions: `assert!(!legacy_called.get());` and `assert_eq!(replay.mutations.len(), 1);`
6. **Rejected-before-commit has no canonical or legacy change** — `crates/mct-observation/src/lib.rs:3717`, central assertions at lines 3738-3739: `assert_eq!(std::fs::read(&path).unwrap(), before);` and `assert!(!legacy_called.get());`
7. **Projection-pending remains a committed fact** — `crates/mct-observation/src/lib.rs:3744`, central assertions: `assert!(matches!(result, AuthorityMutationResultV1::CommittedProjectionPending { pending_reason: AuthorityProjectionPendingReasonV1::ProjectionFailed, .. }));` and `assert_eq!(replay.mutations.len(), 1);`
8. **One-time owner-gated legacy import** — `crates/mct-daemon/src/daemon/control.rs:4121`, central assertions at lines 4171 and 4207: `assert_eq!(blocked["reason"], "import_required");` and `assert_eq!(second["reason"], "already_imported");`; replay additionally asserts `assert!(replay.imported);`.
9. **Authority cursor reaches non-authority head atomically** — `crates/mct-daemon/src/state.rs:5993`, reader assertion at line 6024: `assert_eq!(visible.cursor, old.cursor);`; post-publication assertion begins at line 6031: `assert_eq!(new.cursor.through_sequence, new_entries.last().unwrap().local_sequence);`
10. **Typed D-G8 proof** — `crates/mct-daemon/src/state.rs:6045`; the usable assertion is `assert_eq!(proof, UsableAuthorityProjectionProofV1::Usable { cursor: Box::new(cursor.clone()) });`, and mismatch assertions return exactly `Deny::WrongSourceMother`, `Deny::HeadSequenceMismatch`, `Deny::EpochMismatch`, and `Deny::ProjectionHashMismatch` at lines 6080, 6093, 6106, and 6124.
11. **Epoch transition preserves grant meaning** — `crates/mct-daemon/src/state.rs:6131`, central assertions beginning at line 6161: `assert_eq!(after.state, before.state);` and `assert_ne!(after.cursor.grants_authority.authority_epoch, before.cursor.grants_authority.authority_epoch);`
12. **Atomic shadow replacement** — `crates/mct-daemon/src/state.rs:6178`, central concurrent-reader assertion at line 6230: `assert_eq!(reader.authority_projection_snapshot()?.unwrap(), defective);`
13. **Rebuild equals advancing replay** — `crates/mct-daemon/src/state.rs:6250`, central assertion at lines 6301-6304: `assert_eq!(incrementally_replayed.cursor.projection_hash, rebuilt.cursor.projection_hash);`
14. **Projection failure cannot undo commitment** — `crates/mct-daemon/src/state.rs:6309`, central assertions: `assert!(matches!(result, AuthorityMutationResultV1::CommittedProjectionPending { .. }));` and, after catch-up at line 6375, `assert_eq!(caught_up.state, canonical.state);`
15. **Quarantine advances no projection and denies proof** — `crates/mct-daemon/src/state.rs:6380`, central assertions beginning at line 6415: `assert_eq!(after.state, before.state);`, `assert_eq!(after.cursor.through_sequence, before.cursor.through_sequence);`, and the typed proof equals `UsableAuthorityProjectionProofV1::Denied { reason: AuthorityProjectionDenyReasonV1::LedgerQuarantined }`.

The additional regression `crates/mct-daemon/src/state.rs:6438` proves an unknown reserved authority schema blocks publication while the structurally valid ledger remains non-quarantined and the prior projection remains byte-value equal.

### Validation and flake disposition

Each H2 implementation slice passed `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `./scripts/ci-tier0.sh`; the close-out rerun passed **460 tests with 1 ignored**, warnings-denied clippy, RustSec/Tier 0, and `allium check layer/allium`. No Phase H2 failure required the flake protocol and no non-reproducing failure was observed.

### Final invariant disposition

Track 3 reports **19 COVERED**, **0 LAW-LEADS-CODE**, and **8 DEFERRED** Review 2 invariants, plus a now-`COVERED` structural `MctProjectionCursor` row. H2 moved 9 R2-L3/R2-L4 invariants to `COVERED`. The remaining 8 stay deferred because their complete law requires an authority-evaluation proof consumer, D-R2.8 startup classification, or R2-L6 effect ordering; constructing a correct proof is not misreported as consuming it.

The canonical carrier and newline framing preserve the existing `MctObservation` and `MctObservationLedgerEntry` field schemas. Unknown authority schemas fail projection without ledger quarantine. Quarantined ledgers retain the prior through-head/facts and publish only quarantined projection status. The active work session remains active and unarchived.

# Phase H3 — R2-L5/R2-L6 startup posture and ordering boundary

> Startup proves what survived before it creates anything, canonical projection failure degrades to an owner-readable deny plane, and mutation commitment and protected-effect start receive one Mother-local order without prematurely routing any Child or Toy admission through it.

## Scope and Gate G1

Phase H3 is the final Review 2 implementation slice. After this design commit and a separate operator Gate G1 ratification, it may implement:

1. **R2-L5** — disk-first startup classification, D-R2.8 virgin enforcement, authenticated operator-gated reinitialization, legacy-ledger upgrade, degraded-deny/quarantine posture, authority drift reporting, and the first narrowly sanctioned D-G8 proof consumer at standing-source artifact staging; and
2. **R2-L6** — a Mother-local ordering boundary whose semantics are complete for later Child/Toy effect adoption, but which has no production effect consumer in this phase.

The Phase H3 Step-0 baseline is merge-topology commit `be9be82`; close-out commit ranges use `be9be82..HEAD`. Gate G1 follows this SPEC-only commit and precedes every Rust or Cargo change.

## Ratified Phase H3 amendments

These amendments are settled and reproduced verbatim from operator adjudication.

### D-H3.1

prior_authority = None is legal exactly when canonical replay
yields no prior authority — regardless of ledger emptiness. In every
such case generation_baseline = 0 and canonical authority state is
empty. Safety basis: old tokens die by EPOCH mismatch (fresh entropy);
generation continuity is a within-surviving-canonical-history property.
Reword SPEC:420 accordingly ("never decreases or resets merely because
the epoch changes" applies within a surviving canonical history) and
reword SPEC:421 to tie prior_authority absence to replay yielding no
prior authority rather than to virginity. Legacy SQLite/config authority
remains non-authoritative until the standard one-time import. When
startup_class is operator_gated_nonvirgin, the epoch fact MUST record
the authenticated operator decision id and principal — the currently
hard-coded None values at lib.rs:773-774 become illegal for that class.

### D-H3.2

the predecessor field gains an explicit additive variant
none_after_operator_reinitialization for the empty-ledger,
operator-gated reinitialization case. none_for_virgin keeps its strict
meaning: the D-R2.8 virgin conjunction only. Adding a variant is
forward-safe — strict replay already rejects unknown variants
fail-closed, and no shipped ledger carries v1 epoch facts a new variant
could invalidate.

### D-H3.3

startup_class gains legacy_ledger_upgrade for the third case: a
VALID non-empty ledger whose canonical replay yields no authority
history (pre-H2 history receiving its first epoch fact). This class is
AUTO-PERMITTED with predecessor = validated_head, prior_authority =
None, generation_baseline = 0, and no operator gate: the intact hash
chain is itself the continuity evidence — precisely the evidence the
missing-ledger case lacks. operator_gated_nonvirgin is reserved for the
empty/missing-ledger-with-prior-evidence case where continuity is
unprovable. The full startup taxonomy is therefore: virgin |
ordinary_reopen | legacy_ledger_upgrade | operator_gated_nonvirgin.

### D-H3.4 (confirmation, no code gap)

because the import gate is
replay-derived, reinitialization and upgrade legitimately permit a fresh
import under the standard gate and a fresh operator decision. The SPEC
states this explicitly so "one-time" reads as
one-time-per-canonical-history, never one-time-per-Mother-forever.

## Amended epoch-establishment semantics

The Phase H2 `EpochEstablishedFactV1` schema above is amended in place by D-H3.1 through D-H3.3. The complete predecessor/startup relation is:

| Startup class | Predecessor | Replayed prior authority | Baseline | Operator evidence in epoch fact |
|---|---|---|---:|---|
| `virgin` | `none_for_virgin` | absent | 0 | absent |
| `ordinary_reopen` | exact `validated_head` | present | latest replayed generation | absent |
| `legacy_ledger_upgrade` | exact `validated_head` | absent | 0 | absent |
| `operator_gated_nonvirgin` | `none_after_operator_reinitialization` | absent | 0 | authenticated decision id and principal, both present |

`none_for_virgin` proves the full disk-derived virgin conjunction, not merely an empty ledger. `none_after_operator_reinitialization` proves that continuity was unavailable and an authenticated owner deliberately admitted a fresh canonical history. Neither variant may be inferred from configuration, SQLite, a projection, timestamps, or a missing read result.

A tenure's epoch fact remains that tenure's first append. For operator-gated reinitialization the accepted decision is embedded in that first epoch fact; the corresponding ordinary operator/startup observation is appended only after the epoch acknowledgement. A pending, malformed, stale-evidence, unauthenticated, or refused gate creates no ledger and appends nothing, so it cannot manufacture the intact non-empty history needed for `legacy_ledger_upgrade`.

When replay has no prior authority, the pre-epoch canonical state is empty and its deterministic hash is used. Legacy SQLite/config values cannot populate `prior_authority`, select a generation baseline, or shape the pre-epoch state. They become canonical only through the unchanged import schema after epoch establishment.

The import predicate remains replay-derived: import is available exactly when the current canonical history has no import and no ordinary authority mutation claiming the legacy state. Reinitialization and legacy upgrade therefore each permit one fresh standard import decision; an import in an abandoned or non-surviving canonical history does not become a permanent per-Mother marker.

## Disk-first D-R2.8 virgin predicate

Startup classification occurs before opening/creating the ledger, SQLite, config, identity, UDS, logs, projection, or any staging directory. It uses `symlink_metadata`-equivalent presence inspection and never treats unreadable, ambiguous, symlinked, or special entries as absence. An inspection error yields `disk_evidence_unavailable` and degraded deny; it never yields virgin.

Paths come from the selected supervised root and `SupervisorPaths`, or from the explicit standalone runtime paths and their documented derivations. A config or SQLite row is not allowed to redefine the path set being inspected. For a selected path outside the service root, that exact configured path, its known sidecars/temporary names, and its daemon-managed derived root are inspected as well.

The virgin predicate is the conjunction that every durable artifact class below is absent:

| Artifact class | Path/pattern inspected | Production source establishing that the daemon can write it |
|---|---|---|
| Canonical observation ledger | selected `ledger_path`; supervised `<root>/observations.jsonl` | `JsonlObservationLedger::open` / resident and lifecycle writers |
| Ledger recovery forensics | sibling `<ledger filename>.forensics/`, including case directories, `source.bin`, diagnostic JSON, and interrupted temporary records | `mct_observation::forensic_root_path` and R2-L1 recovery |
| Runtime SQLite | selected `state_path`; supervised `<root>/state.sqlite` | `MctRuntimeStateStore::open` |
| SQLite durability sidecars | `<state_path>-wal`, `<state_path>-shm`, `<state_path>-journal`, and SQLite temporary siblings for that database | SQLite WAL/journal operation selected by `MctRuntimeStateStore::migrate` |
| Daemon configuration | selected `config_path`; supervised `<root>/config.json` | `MctDaemonConfigStore::save` |
| Interrupted config publication | `config_path.with_extension("json.tmp")` | `MctDaemonConfigStore::save` |
| Recorded Mother identity | selected `identity_path`; supervised `<root>/identity/iroh-secret.hex`, plus a daemon-created `identity/` directory left before file creation | `load_or_create_node_secret_key_hex` / observed identity mutation |
| Child/package catalog | selected `children_dir`; supervised `<root>/children`, including installed packages, manifests, components, SHA-256 sidecars, `checksums.txt`, and immutable `artifacts/sha256/*` packages | registry install and artifact acquisition |
| Interrupted child acquisition/install | `<children_dir>/.acquiring/*`, `.installing-*`, `.replaced-*`, and any daemon-created partial package directories | acquisition and registry publication paths |
| Content-addressed blobs | `<state parent>/blobs`, including `tmp/ingest-*.tmp` and `blake3/<prefix>/*.blob` | `MctLocalBlobStore::for_state_path` |
| Daemon release store | `<state parent>/releases`, including `.acquiring/*`, copied archive sidecars, extracted staging trees, and immutable `sha256/*` releases | daemon release acquisition and supervised upgrade |
| Supervisor lifecycle record | selected `<root>/supervisor.json` and staged `.supervisor.json.<pid>.tmp` siblings | `SupervisorPaths` and supervisor `atomic_write` |
| Supervisor policy | `~/Library/LaunchAgents/io.patina.mct.mother.plist` for production, plus staged `.io.patina.mct.mother.plist.<pid>.tmp` siblings | `LaunchdSupervisorAdapter::publish_policy` |
| Supervisor logs | `<root>/logs/`, `mother.stdout.log`, and `mother.stderr.log` | generated launchd policy and supervised lifecycle setup |
| Other entries under daemon-managed roots | any otherwise unclassified entry below existing `identity/`, `children/`, `blobs/`, `releases/`, `logs/`, or ledger forensic roots | conservative catch-all for current/future daemon residue; unknown durable residue cannot prove virginity |

A zero-length ledger or SQLite file is present evidence, not absence. A partially created managed directory is evidence even when its final file is missing. Exact forensic and staging residue is never deleted to make the conjunction pass.

The selected service-root directory by itself may be an operator-created container and is not sufficient evidence only when it is empty and no daemon-managed metadata can be attributed to it. Any managed child directory or unclassified entry beneath it is evidence. External source trees, project/watch roots, operator-supplied executable paths, and arbitrary files written by a Child/Toy effect are not Mother-owned startup artifacts and are not traversed. A supervised executable becomes evidence only when it lives in the daemon-managed `releases/` store.

`<root>/control.sock` is classified separately as transient runtime residue. It never proves virginity by itself because the daemon removes it on clean shutdown and a crash may leave its directory entry; an active listener or writer lock yields `writer_contended`/`already_running`, while a stale socket is reported and replaced only after the immutable disk classification snapshot is taken. The launchd in-memory loaded state is likewise operational evidence, not a substitute for disk evidence.

The classifier returns an ordered, canonical `StartupArtifactInventoryV1` containing each inspected path, artifact class, `absent | present | unavailable | transient` result, file type when present, and a hash of the inventory. It does not include file contents or identity secret bytes.

## Complete startup classification and posture

`authority_ready` below means that Phase H3 may expose ordinary resident readiness and the sanctioned standing-source proof consumer. It does not claim that deferred route, resident-grants, Toy, host-adapter, or peer-wire readers have migrated to canonical authority.

| Disk/ledger/projection state | Detection | Transition | Daemon posture | Authority readiness |
|---|---|---|---|---|
| Missing ledger; every durable artifact class absent | complete inventory; ledger `NotFound`; no transient active owner | create one exclusive writer; first append is fresh `virgin` epoch with `none_for_virgin`, empty state, baseline 0 | bootstrap, then ordinary service only after observations/projection complete | pending until current D-G8 proof; then ready |
| Missing or empty ledger; any durable artifact present | complete inventory plus no canonical entry | expose owner-only gate; require exact inventory hash, explicit confirmation, unique decision id, and UDS-authenticated principal; accepted decision establishes `operator_gated_nonvirgin` with `none_after_operator_reinitialization` | degraded deny while pending/refused; no identity/config/state/import mutation before accepted epoch | not ready until accepted epoch, optional standard import/reconciliation, current projection, and no authority-bearing drift |
| Valid non-empty ledger; canonical replay has no authority history | maximal validated non-empty head plus empty authority replay | auto-establish `legacy_ledger_upgrade` with `validated_head`, no operator gate, empty state, baseline 0 | upgrading; then ordinary service after projection/import posture resolves | pending, then ready; standard import is available under D-H3.4 |
| Valid ledger; canonical replay has current authority | maximal validated head and current replay identity | establish `ordinary_reopen` with exact `validated_head` and replayed baseline/state | ordinary reopen | pending until projection reaches the post-startup head and proof is usable |
| Unterminated final residue | R2-L1 scan under exclusive lock and completed forensic preservation | recover only by the already-ratified residue path, then reclassify the surviving ledger using the rows above | recovery, never early readiness | not ready during recovery |
| Terminated corruption, identity/lineage failure, or chain discontinuity | typed R2-L1 quarantine/foreign-lineage result | preserve; do not truncate, adopt, replace, import, or establish epoch | isolated quarantine forensic plane | not ready |
| Structurally valid ledger with unknown/reserved or incoherent authority schema | ledger scan valid but authority replay/projection rejects | retain ledger and prior projection; operator changes software/history only outside this phase | degraded deny, typed `authority_replay_blocked`; not falsely quarantined | not ready |
| Writer contention, permission/I/O failure, or incomplete artifact inspection | typed contention/unavailability or any inventory `unavailable` | no recovery or mutation; supervisor retry remains ordinary policy | degraded deny when a safe owner UDS can be bound, otherwise fail-stop with typed startup result | not ready |
| Epoch committed but projection absent, stale, wrong-epoch/hash, rebuilding, or behind any post-epoch startup observation | D-G8 expectation against current canonical replay/head | incremental catch-up or atomic shadow rebuild from validated history only | degraded deny until proof is usable | not ready until usable |
| Replay-derived import required or authority-bearing config/legacy SQLite drift exists | canonical state plus normalized config/SQLite comparison | permit only the existing authenticated import when its standard predicate holds, or explicit ordinary reconciliation; never auto-import/overwrite | degraded deny with drift/gate report | not ready |
| Epoch current; startup/gate/drift observations included in the projected head; D-G8 proof usable; no blocking drift | exact typed proof and report | publish readiness once, without a later append that would move the proven head | ordinary service | ready for the Phase H3-sanctioned consumer |

Readiness publication never races ahead of the final startup observations: classification, accepted gate (if any), startup posture, and drift observations append first; projection then covers that resulting canonical head; the final usable proof is checked before network/Child/Toy or ordinary mutation surfaces are exposed. A later canonical mutation that returns `committed_projection_pending` immediately returns posture to degraded deny until coverage is restored. No cached prior readiness survives restart, epoch transition, quarantine, writer poisoning, or proof mismatch.

### Authenticated operator gate

The isolated owner UDS exposes `POST /startup/operator-gate` only in `operator_gate_required` posture. Its request names a unique decision id, the expected Mother/ledger ids, the exact `StartupArtifactInventoryV1` hash, and the confirmation `reinitialize-missing-canonical-authority-v1`. The principal comes only from UDS peer credentials. A changed inventory, active writer/listener, wrong identity, reused decision, malformed confirmation, or non-owner is a typed refusal and causes no durable write.

An accepted request acquires the exclusive writer, rescans disk/ledger, and commits the epoch as the first append with both operator fields present. It then appends ordinary `OperatorActionRecorded` and `LifecycleTransitionRecorded` observations correlating the decision, inventory hash, resulting epoch observation id, and startup posture. The gate is not a new canonical authority fact kind and does not import legacy authority.

## Isolated quarantine and degraded forensic plane

The existing UDS path remains mode `0600`; in isolated posture every request, including reads, additionally requires peer credentials matching the service-root owner. The Iroh endpoint, TCP control endpoint, hello/call service, trigger scheduler, Child/Toy execution, and ordinary mutation handlers do not start.

The read-only surface is closed and explicit:

| Endpoint | Exposed content |
|---|---|
| `GET /status` | version, `unhealthy/not_ready`, startup posture, typed reason, and no advertised Iroh authority |
| `GET /startup` | four-class result when classifiable, daemon posture, authority readiness, inventory hash, per-artifact presence classifications, gate state, and projection status |
| `GET /forensics/ledger` | ledger path/id/Mother, scan class, maximal valid head sequence/hash, failure class, first bad offset/sequence, expected/observed diagnostics, and forensic root; never a repaired view |
| `GET /forensics/cases` | retained case ids, decision/time, exact byte lengths/digests, source offsets, prior committed head, recovery/quarantine class, and local owner-only paths |
| `GET /forensics/cases/{case_id}/source` | exact preserved bytes with explicit bounded byte-range requests and digest/total-length headers; no transformation and no automatic export |
| `GET /drift` | the latest typed authority drift report below, without identity secrets, config secrets, blob payloads, or arbitrary SQLite rows |

Every other GET, every call preflight, and every POST except the startup operator gate in its one legal posture returns:

```text
StartupRefusalV1 {
  schema: "mct-startup-refusal/v1",
  kind: startup_degraded_deny | ledger_quarantined | operator_gate_required |
        authority_replay_blocked | projection_unusable | writer_fenced,
  startup_class: virgin | ordinary_reopen | legacy_ledger_upgrade |
                 operator_gated_nonvirgin | unclassified,
  authority_ready: false,
  retryable: bool,
  safe_message: String
}
```

Quarantine is not operator-gated reinitialization. The gate endpoint itself refuses in quarantine; H3 supplies no delete, truncate, adopt, replace, export, or repair operation. The forensics surface reads already-preserved evidence and performs no projection publication or ledger mutation.

## Authority drift report and observations

After epoch establishment and before final projection publication, startup computes:

```text
AuthorityDriftReportV1 {
  schema: "mct-authority-drift-report/v1",
  report_id: String,
  observed_at: Timestamp,
  startup_class: virgin | ordinary_reopen | legacy_ledger_upgrade |
                 operator_gated_nonvirgin,
  canonical: {
    mother_node_id: String,
    ledger_id: String,
    head_sequence: u64,
    head_entry_hash: String,
    grants_authority: GrantsAuthorityIdentity,
    authority_state_hash: String
  },
  projection: {
    status: missing | rebuilding | current | stale | quarantined | blocked,
    through_sequence: u64?,
    through_entry_hash: String?,
    projection_hash: String?,
    proof_denial: AuthorityProjectionDenyReasonV1?
  },
  legacy_inputs: [{
    source: config_authority_intent | sqlite_toy_authority,
    normalized_hash: String?,
    comparison: no_authority_intent | matches_canonical | differs_from_canonical |
                import_required | unavailable
  }],
  blocking_reasons: [String, ...],
  authority_ready: bool
}
```

Hashes use the existing normalized legacy-import inputs and canonical authority-state hash; no `MAX()` aggregation or incidental SQLite/config representation enters the comparison. Config with no authority-shaping intent is `no_authority_intent`, not false drift. A usable proof cannot erase legacy drift, and matching legacy values cannot repair an unusable proof.

Projection mismatch, unavailable authority-bearing input, unimported legacy state, or legacy SQLite/config authority that could still be read as broader than canonical is blocking until an explicit existing mutation/import/reconciliation path resolves it. Drift never creates authority, rewrites canonical history, auto-imports, or silently edits config/SQLite. Non-authority operational differences may be reported without becoming authority.

A writable, non-quarantined startup appends ordinary observations using existing kinds only:

- `LifecycleTransitionRecorded`/`Storage` records the selected startup class and posture;
- `OperatorActionRecorded`/`Operator` records an accepted operator gate and its authenticated correlation; and
- `NodeHealthReported`/`Storage` records `report_id`, `authority_ready`, blocking reason codes, canonical state hash, and normalized input hashes.

These are ordinary observations, not `mct-authority-fact-v1` payloads and not new canonical fact kinds. They append before final projection catch-up. If the ledger is quarantined or unavailable, no observation is appended to it; the isolated status reports that recording was impossible rather than mutating forensic evidence.

## Standing-source D-G8 migration

The sole new production authority-proof consumer in H3 is standing-source artifact acquisition through offline/resident control staging. Operator-pointed acquisition is unchanged and cannot borrow the standing-source proof.

`verify_standing_source_ledger_correlation` becomes one component of a typed standing-source admission, not the complete freshness proof. While the resident control mutation serialization guard is held (or while an offline exclusive writer is held), staging:

1. appends its already-required pre-effect decision observations;
2. catches the authority projection up through that resulting validated ledger head;
3. constructs current `AuthorityProjectionLedgerEvidenceV1` from canonical replay and requires `UsableAuthorityProjectionProofV1::Usable`;
4. performs the existing exact standing-source projection digest/state and unique ledger-observation correlation;
5. rechecks that source projection against the admitted source proof; and only then
6. evaluates standing source scope and permits the filesystem acquisition adapter to begin reading.

The D-G8 check's linearization point is the standing-source trust evaluation under the existing control mutation guard. A canonical authority/source mutation cannot pass that guard between proof consumption and adapter-start handoff. An unrelated later ordinary observation does not retroactively undo an already-completed trust decision; a mutation committed before that decision necessarily makes the proof stale or changes the correlated source and denies.

The typed result is:

```text
StandingSourceAdmissionV1 =
  usable {
    authority_projection: UsableAuthorityProjectionProofV1::usable,
    source_authority_id: String,
    source_record_digest: String,
    source_fact_sequence: u64
  }
| denied {
    reason: authority_projection(AuthorityProjectionDenyReasonV1) |
            ledger_unavailable | ledger_quarantined | authority_replay_blocked |
            source_missing | source_digest_mismatch | source_inactive |
            source_fact_missing | source_fact_duplicate | source_fact_mismatch |
            source_changed_after_proof
  }
```

Every denied result occurs before source bytes are read, before staging/catalog paths are created, and before an adapter-start effect is admitted. Safe control responses preserve the typed reason without exposing source credentials or raw forensic bytes. This migration does not make standing-source records canonical Toy/grant facts and does not broaden D-R2.7 projection content.

## Mother-local mutation/effect ordering boundary

R2-L6 introduces one process-local boundary per executing Mother authority writer. It has two mutually ordered operations:

```text
MotherAuthorityOrderV1::commit_mutation(mutation_id, intent, commit_fn)
MotherAuthorityOrderV1::admit_effect(expectation, proof_fn, start_fn)
```

`commit_mutation` owns the ordering position from before canonical append is offered until the result is classified. An acknowledged/recovered canonical fact linearizes at commitment. `rejected_before_commit` consumes no commitment position. `commit_unknown`, writer poisoning, or loss of exact rescan evidence sets the boundary to `fenced` before it releases its position.

`admit_effect` acquires the same order, requires an unfenced writer, exact current canonical identity/state expectation, and a usable projection through the current canonical head. It hands an unforgeable, single-use admission directly to `start_fn` while still owning the order; only entry into the protected adapter's effect-start seam releases the order. It never returns a refreshable bearer token that can be stored and started later. Denial starts nothing.

Consequences:

- a revocation committed before admission is visible to the required current proof and denies;
- admission handed to effect start before a later revocation has one earlier order position, so the already-started external effect is not retroactively undone;
- `commit_unknown` and a poisoned writer fence all later admissions until close, exclusive reopen, full rescan, mutation-id resolution, and current projection proof;
- acknowledged commitment with projection pending also fences admission: canonical state has advanced but freshness is unprovable;
- projection lag can never be bypassed with the pre-mutation cursor, a bare generation, a caller echo, record digests, or cached permit; and
- the boundary gives no cross-Mother order and invents no JSONL/SQLite transaction. Ledger commitment remains canonical before projection publication.

`fenced` therefore means the boundary cannot prove which canonical authority state must govern a new effect start. It is fail-closed but not permanent: only the already-ratified reopen/rescan plus coherent projection transition clears it. A new mutation id, supervisor retry without rescan, or stale projection cannot clear it.

H3 lands this API and adversarial tests but routes no production Child, Toy, route-evaluation, resident-grants, or host-adapter path through it. Grants slices 7 and 8 later adopt the same `admit_effect` handoff at their final Child and Toy adapter seams, and route all authority-shape commits through `commit_mutation`; they do not redesign token shape, add a second lock/order, or move projection checks into adapters.

## Phase H3 implementation tasks

### B1 — disk classifier and four startup classes

Implement the immutable pre-write inventory, unavailable/ambiguous handling, four-class taxonomy, additive predecessor/startup variants, and replay validation for D-H3.1 through D-H3.3.

### B2 — epoch establishment and operator-gated reinitialization

Require explicit startup evidence at authority open, remove inference from ledger emptiness alone, authenticate the gate on the owner UDS, keep the epoch fact first, record operator fields for gated startup, and preserve replay-derived import semantics from D-H3.4.

### B3 — degraded-deny/quarantine forensic plane

Separate classification from full resident startup. Bind only the owner-authenticated UDS in non-ready postures, expose the closed read surface above, and return typed refusal for every other route without ledger/projection/config mutation.

### B4 — coherent startup projection and drift posture

Record ordinary startup/gate/drift observations before final catch-up, publish/rebuild from validated history, construct the final D-G8 proof, and expose readiness only when no blocking drift or proof denial remains.

### B5 — sanctioned standing-source proof consumption

Require typed D-G8 proof plus existing exact source correlation under the staging mutation guard, and prove every mismatch denies before source read, staging, catalog, or adapter effect.

### C-1 — R2-L6 ordering primitive

Implement the Mother-local commit/admission order, single-use start handoff, and typed fenced state without adding a production consumer.

### C-2 — recovery/projection fence and future adoption seam

Clear fencing only from exclusive reopen/full rescan plus current coherent projection; expose the exact future slices 7-8 integration seam and prove revocation-first, effect-start-first, uncertainty, and lag orders with test-only adapters.

## Phase H3 required proof steps

Each proof lands as a named failing test before its implementation. Close-out cites the landed file/line and quotes the verbatim central assertion.

1. A truly virgin disk snapshot creates no artifact before classification, selects `virgin`, and commits a first epoch with `none_for_virgin`, absent prior authority, empty state, and baseline 0.
2. A table-driven matrix proves each durable artifact class above, including SQLite sidecars, config temp, forensic/staging residue, blobs, releases, logs, record, and plist, independently prevents virgin classification; unavailable inspection also denies.
3. Missing/empty ledger plus prior evidence exposes only the owner gate; pending, wrong-inventory, malformed, and non-owner decisions create no ledger or legacy effect.
4. Accepted reinitialization commits the epoch first as `operator_gated_nonvirgin` with `none_after_operator_reinitialization`, baseline 0, empty state, decision id/principal, then records ordinary gate/startup observations.
5. Ordinary reopen uses exact validated head, replayed prior authority/state/generation, a fresh epoch, and no operator fields.
6. Reinitialized canonical history permits one standard import with a fresh operator decision; its second import is `already_imported`, while abandoned-history import evidence grants nothing.
7. Terminated corruption/foreign lineage starts only the owner-authenticated forensic plane; ledger bytes and prior projection remain unchanged, and every call/mutation/gate route returns typed quarantine refusal.
8. Forensic endpoints expose exact typed diagnostics and ranged preserved bytes to the owner, reject a non-owner, and disclose no identity secret, config secret, blob payload, or arbitrary SQLite row.
9. Unknown authority schema and projection missing/stale/wrong-epoch/hash/quarantined statuses each withhold readiness without misclassifying a structurally valid ledger or advancing prior projection truth.
10. Startup/gate/drift ordinary observations precede final projection publication; the readiness proof covers their exact final sequence/hash and no later readiness append invalidates it.
11. Config/SQLite legacy drift produces the typed report and `NodeHealthReported` observation, performs no canonical/import/config/SQLite rewrite, and blocking authority drift keeps posture degraded.
12. A fully matching canonical replay/projection and non-broader normalized legacy inputs yields a usable final proof and ready posture across restart.
13. Standing-source staging with a usable D-G8 proof and exact source correlation reaches source read; operator-pointed staging neither requires nor can consume that proof.
14. Every D-G8 deny reason and every source-correlation deny reason is returned typed before source bytes, staging paths, catalog rows, or adapter-start effects; source change after proof also denies.
15. The test-only ordering seam proves both total orders: revocation commitment before admission starts no effect, while admission handoff before later revocation starts exactly once and is not retroactively undone.
16. `commit_unknown`, poisoned writer, unresolved rescan, and committed projection lag each fence the ordering boundary; repeated admission attempts start nothing until exclusive rescan resolution and an exact current proof clear the fence.
17. A valid non-empty pre-H2 ledger starts as `legacy_ledger_upgrade` with no operator gate, establishes its first epoch fact with `validated_head` predecessor and baseline 0, records the class in the fact, and permits the standard import afterward.

## Phase-critical constraints C1-C5

- **C1 — one sanctioned reader change.** Only standing-source acquisition/control staging becomes a production D-G8 proof consumer. No other authority reader changes.
- **C2 — no ordering consumer yet.** The R2-L6 boundary is implemented and tested with test-only adapters, but no production effect or mutation path adopts it in H3.
- **C3 — degraded deny over brick or fallback.** When safe local ownership can be established, unavailable authority exposes the isolated owner forensic/gate plane and denies authority; it never falls back to config, SQLite, caller values, stale projection, or guessed recovery.
- **C4 — permanent schemas stay closed.** H3 adds no canonical authority fact kind and changes no `MctObservation`/`MctObservationLedgerEntry` field or newline framing. D-H3.2/D-H3.3 are the only additive v1 enum variants. Startup/drift/gate records are ordinary observations using existing kinds.
- **C5 — no hidden authority transition.** Startup classification, accepted gating, drift, readiness denial, and projection catch-up are observable; quarantine/unavailable cases report inability to append rather than mutating evidence. Ledger commitment still precedes projection and any legacy write.

## Updated deferral fence

The following remain forbidden in Phase H3, including partial implementation:

- No production consumer of `MotherAuthorityOrderV1`; grants-authority slices 7-8 remain separately gated.
- No route-evaluation, resident-grants-guard, Toy grant/effect, process/WASM host-adapter, hello, or remote/local call authority-reader migration.
- No grants-authority slice 4 authority-shape integration, slice 5 general local snapshot provider, slice 6 peer generation wire migration, or slices 7-8 effect consumption.
- No peer wire/schema changes and no caller-carried authority value becoming local authority.
- No new canonical fact kind, ledger entry field, observation field, frame format, compaction, truncation, automatic lineage adoption, or cross-file ACID claim.
- No automatic config/SQLite import, drift repair, quarantine repair, forensic deletion/export, or legacy state precedence over canonical replay.
- No broad artifact-source redesign, unattended registry sync, network acquisition adapter, or operator-pointed acquisition authority change.
- No H1 process cleanup escalation, process-tree reap, capacity restoration, or supervisor restart/fail-stop implementation.

The active session `20260724-223731-101286000` remains active and must not be archived or ended. Any implementation need beyond this fence stops for a D-H3.n amendment rather than improvising.

## Validation and close-out plan

Every implementation commit and final close-out must pass:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
allium check layer/allium
```

A failing test is rerun in isolation up to five times. A non-reproducing failure is retained verbatim in the Phase H3 flake log; a reproducing failure is fixed before proceeding.

Close-out is reconstructed from disk using `be9be82..HEAD`: commit purpose, all seventeen test file/line citations and verbatim central assertions, per-commit and final validation transcripts, flake disposition, changed-reader audit, ordering-consumer audit, canonical-fact-kind/schema audit, Track 3 disposition, and the active session update. Gate G1 ratification is recorded before any Rust implementation commit.

## Phase H3 close-out

Gate G1 ratified commit `cd438bf` before any Phase H3 Rust change. The implementation then landed exactly one named commit per task:

1. B1 — `b25fe93 feat(startup): classify durable state before authority open`
2. B2 — `2b2375b feat(startup): establish epochs from explicit operator-gated evidence`
3. B3 — `bd7a695 feat(startup): isolate degraded authority behind owner forensic plane`
4. B4 — `cc38956 feat(startup): publish coherent authority readiness and drift posture`
5. B5 — `e809aaf feat(acquisition): consume D-G8 proof for standing sources`
6. C-1 — `3e5b499 feat(authority): add Mother-local mutation and effect-start order`
7. C-2 — `5819d28 feat(authority): clear fences only after exclusive rescan and projection proof`

### Required proof citations and verbatim central assertions

1. **Virgin classification and first epoch** — `crates/mct-daemon/src/startup.rs:1699`, `virgin_and_reopen_epoch_establishment_consumes_exact_startup_evidence`; central assertions: `assert!(!paths.ledger.exists());`, `assert_eq!(first_tenure.entry.local_sequence, 0);`, `AuthorityEpochPredecessorV1::NoneForVirgin`, `assert_eq!(first_tenure.fact.generation_baseline, 0);`, and `assert!(first_tenure.fact.prior_authority.is_none());`.
2. **Complete durable-artifact matrix** — `crates/mct-daemon/src/startup.rs:2260`, `durable_artifact_matrix_and_unavailable_inspection_prevent_virgin_classification`; central assertions: `assert!(!inventory.proves_virgin(), "durable class {expected_class:?} must independently prevent virginity");` and `assert!(!inventory.proves_virgin(), "unavailable inspection must deny the virgin conjunction");`.
3. **Gate refusals are pre-write** — `crates/mct-daemon/src/startup.rs:1744`, `operator_gate_refusals_are_prewrite_and_acceptance_embeds_authenticated_decision`; after non-owner, malformed, and stale-inventory requests the repeated central assertion is `assert!(!paths.ledger.exists());`.
4. **Accepted reinitialization embeds authenticated evidence in the first epoch** — same test at `startup.rs:1744`; central assertions: `assert_eq!(entries[0], ledger.authority_tenure().unwrap().entry);`, `AuthorityEpochPredecessorV1::NoneAfterOperatorReinitialization`, `Some("decision:gate-1")`, and `Some("os-uid:501")`; `mct-observation/src/lib.rs:3903` independently asserts `tenure.entry.local_sequence == 0`.
5. **Ordinary reopen names exact surviving authority** — `startup.rs:1699`; central assertions: `assert_eq!(fact.prior_authority.as_ref(), Some(&first_authority));` and `AuthorityEpochPredecessorV1::ValidatedHead { sequence: first_head.local_sequence, entry_hash: first_head.entry_hash }`.
6. **Import is once per surviving canonical history** — `startup.rs:1744` proves the second current-history import is `AlreadyImported`; `startup.rs:1892`, `reinitialization_import_is_scoped_to_the_surviving_canonical_history`, preserves bytes containing `abandoned-import` and centrally asserts `assert_eq!(replay.import.unwrap().fact.import_id, "current-import");` after accepted reinitialization.
7. **Corruption and foreign lineage enter only quarantine** — `startup.rs:1956`, `quarantine_plane_is_owner_only_read_only_and_preserves_exact_forensics`, and `startup.rs:2056`, `foreign_lineage_enters_the_same_nonmutating_quarantine_plane`; central assertions keep both `fs::read(&paths.ledger)` and `fs::read(&paths.state)` byte-equal and return `Some(MctStartupRefusalKindV1::LedgerQuarantined)` for protected routes.
8. **Forensics are exact, ranged, owner-only, and secret-free** — `startup.rs:1956`; central assertions are `status_code() == 403` for a non-owner, `assert_eq!(source.body_bytes(), ledger_before);`, exact digest/length headers, and negative checks for identity, config, blob, and prior-projection material.
9. **Unusable projection withholds authority without rewriting truth** — `crates/mct-daemon/src/state.rs:6045`, `usable_authority_projection_proof_is_typed_for_every_mismatch`, centrally asserts each exact `AuthorityProjectionDenyReasonV1`; `state.rs:6438`, `unknown_authority_schema_blocks_projection_without_ledger_quarantine`, preserves the prior projection while structural replay remains non-quarantined; `startup.rs:2091` centrally asserts `!drift.authority_ready` for blocking legacy drift.
10. **Observations precede final covered readiness** — `startup.rs:2091`, `startup_observations_are_projected_before_readiness_and_drift_never_repairs`; central assertions bind `ready.report.canonical.head_sequence`, `head_entry_hash`, and `ready.cursor().unwrap().through_sequence` to the exact final head after `LifecycleTransitionRecorded` and `NodeHealthReported`.
11. **Drift reports but never repairs** — same test; central assertions are `assert!(!drift.authority_ready, "broader legacy SQLite authority must keep startup degraded");`, `blocking_reasons.contains(&"legacy_import_required".into())`, byte-value preservation of the Toy contracts, and `assert!(!replay_authority_entries(...).imported);`.
12. **Matching restart reconstructs readiness** — same test; central assertion: `assert!(restarted.authority_ready, "ready posture must reconstruct across restart");`.
13. **Only standing-source trust consumes D-G8** — `crates/mct-daemon/src/acquisition.rs:1286`, `standing_source_admission_requires_current_dg8_and_exact_source_before_read`, centrally asserts `matches!(admission, StandingSourceAdmissionV1::Usable { .. })` before `assert_eq!(report.acquisition_outcome, "acquired");`; `acquisition.rs:1685` keeps successful operator-pointed acquisition independent of that admission.
14. **Every typed standing-source denial stays before staging** — `acquisition.rs:1380`, `every_projection_denial_remains_typed_at_standing_source_admission`, maps every D-G8 reason exactly; `acquisition.rs:1409`, `every_source_correlation_denial_remains_typed_before_staging`, asserts exact `SourceMissing`, `SourceDigestMismatch`, `SourceInactive`, `SourceFactMissing`, `SourceFactDuplicate`, and `SourceFactMismatch`; `acquisition.rs:1286` adds exact `SourceChangedAfterProof` and centrally asserts `assert!(!changed_request.children_dir.exists());`.
15. **Both total orders** — `crates/mct-daemon/src/authority_order.rs:560`, `revocation_first_denies_while_effect_start_first_runs_exactly_once`; central assertions are `starts.load(Ordering::SeqCst) == 0` after revocation-first and `== 1` after effect-start-first, with the latter `"not retroactively undone"`.
16. **Uncertainty and lag remain fenced through exact recovery** — `authority_order.rs:430`, `uncertainty_and_projection_lag_fence_until_exclusive_reopen_and_exact_proof`; central assertions deny three repeated starts with `Fenced(ProjectionLag)`, reject the old tenure with `FreshWriterTenureRequired`, clear only after exclusive reopen/rebuild/exact proof, and finish with `"uncertainty and poisoned-writer retries must start nothing"`. The boundary retains offered mutation ids internally; callers cannot omit an uncertain id from recovery.
17. **Valid pre-H2 history auto-upgrades** — `crates/mct-observation/src/lib.rs:3951`, `valid_nonempty_pre_h2_ledger_is_classified_as_legacy_upgrade`; central assertions name `AuthorityStartupClassV1::LegacyLedgerUpgrade`, the exact `ValidatedHead`, baseline `0`, absent prior authority, and a subsequent `CommittedProjectionPending` standard import.

### Scope audits reconstructed from `be9be82..HEAD`

- **Reader audit:** the only new production authority trust/evaluation consumer is standing-source acquisition/control staging in `acquisition.rs` and `daemon/control.rs`. Startup finalization constructs the mandated readiness proof but is not a route/effect authority reader. Route evaluation, resident grants, Child/Toy execution, process/WASM host adapters, hello, and peer call wire retain their prior sources.
- **Ordering audit:** repository references to `MotherAuthorityOrderV1::{commit_mutation,admit_effect}` occur only in `authority_order.rs` tests. The exported type has zero production mutation/effect consumers.
- **Schema audit:** the only authority-schema additions are D-H3.2/D-H3.3 enum variants `none_after_operator_reinitialization` and `legacy_ledger_upgrade`. Canonical fact kinds remain exactly `epoch_established`, `authority_mutation`, and `legacy_authority_import`; `MctObservation`, `MctObservationLedgerEntry`, and newline framing are unchanged. Startup/gate/drift evidence uses existing observation kinds.
- **Disposition audit:** Track 3 now reports **20 COVERED**, **0 LAW-LEADS-CODE**, and **7 DEFERRED** Review 2 invariants. The harness-only ordering laws and globally incomplete reader laws remain deferred; partial consumption is not over-credited.

### Validation and flake disposition

Each B1-B5 and C-1/C-2 slice passed workspace tests, warnings-denied clippy, Tier 0/RustSec, and Allium before advancement. The final close-out rerun passed **475 tests with 1 ignored**, `cargo clippy --workspace --all-targets -- -D warnings`, `./scripts/ci-tier0.sh`, `allium check layer/allium`, and `patina spec check ledger-commit-recovery --json` at **11/11**. `git diff --check` was clean.

No Phase H3 non-reproducing failure occurred and the flake protocol was not invoked. Expected failing-first proof failures and deterministic fixture mismatches were repaired before advancement; the supervisor fixture now honestly expects degraded restart when deferred legacy SQLite authority is broader than canonical authority.

The active session `20260724-223731-101286000` remains `active`; it was not ended or archived.

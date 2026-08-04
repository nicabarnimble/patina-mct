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

Phase H2 proposes and, only after operator ratification of this section, implements:

1. **R2-L3** — writer-tenure epoch establishment, canonical Toy catalog/grant mutation facts, typed mutation outcomes and same-ID resolution, and the one-time operator-gated import required by D-R2.3.
2. **R2-L4** — full-ledger authority replay, coherent authority-state cursor publication, the D-G8 usable-projection proof, and replay-equivalent shadow rebuild.

This schema proposal is Gate G1. No Phase H2 Rust or Cargo change may begin until the operator ratifies these permanent fact schemas. Any amendment is recorded as D-H2.n; D-R2.1 through D-R2.8 remain settled.

Gate G1 specifically ratifies four permanent choices made concrete below: the reserved inline `detail_ref` carrier, a non-resetting generation baseline across epochs, one replay-complete fact per ordinary mutation/import, and the H2/R2-L5 seam where H2 records startup provenance but R2-L5 remains responsible for enforcing D-R2.8's every-artifact virgin/operator gate at Mother startup.

## Ratified Phase H2 amendment

### D-H2.1

`mct-authority-fact-v1:` is a named constant at the carrier parse site. The `MctObservation.detail_ref` field documentation names this reserved inline canonical-authority payload semantics; all unreserved values remain opaque references. This documentation/constant amendment changes neither the entry field schema nor the Phase H2 fence.

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

## Proposed epoch-establishment schema

```text
EpochEstablishedFactV1 {
  mother_node_id: String,
  ledger_id: String,
  authority_epoch: String,
  predecessor: none_for_virgin | validated_head {
    sequence: u64,
    entry_hash: String
  },
  generation_baseline: u64,
  prior_authority: GrantsAuthorityIdentity?,
  resulting_authority: GrantsAuthorityIdentity,
  grant_state_hash: String,
  establishment: writer_tenure {
    started_at: Timestamp,
    startup_class: virgin | ordinary_reopen | operator_gated_nonvirgin,
    operator_gate_decision_id: String?,
    authenticated_principal_ref: String?
  }
}
```

Rules:

- `authority_epoch` satisfies C4 and is fresh for every successfully acquired exclusive tenure.
- `none_for_virgin` is legal only when the validated ledger has no prior entry and the D-R2.8 virgin predicate has been established by the later R2-L5 gate. H2 records the value but does not implement that startup gate.
- `validated_head` exactly equals the sequence/hash immediately preceding this fact. A copied or restored ledger therefore names its copied head but receives a new entropy-derived epoch.
- A virgin baseline is `0`. A non-virgin baseline equals the latest replayed generation; it never decreases or resets merely because the epoch changes.
- `prior_authority` is absent only for a virgin ledger. `resulting_authority` names this Mother, the fresh epoch, `generation_baseline`, and this epoch fact's observation id.
- `grant_state_hash` is the deterministic hash of the complete current Toy catalog/grant state before and after the epoch transition. Equality proves that epoch transition itself changed identity but not grant meaning.
- The epoch fact is the first append made by the newly exclusive writer and must be acknowledged before that writer exposes its epoch or accepts an authority mutation. Failure or uncertainty poisons/fences the tenure under R2-L2.

## Proposed canonical authority-mutation schema

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

## Proposed typed mutation result and same-ID resolution

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

## Proposed one-time operator import schema

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

## Proposed authority projection rows and coherent publication

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

## Proposed usable-projection proof

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

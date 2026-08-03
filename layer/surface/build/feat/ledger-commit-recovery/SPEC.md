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

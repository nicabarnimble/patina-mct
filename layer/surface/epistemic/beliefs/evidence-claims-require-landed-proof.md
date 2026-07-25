---
type: belief
id: evidence-claims-require-landed-proof
persona: architect
facets: [evidence, testing, documentation, review]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-07-16
revised: 2026-07-25
---

# evidence-claims-require-landed-proof

Close-out evidence claims require landed assertions and source citations; when prose overclaims, withdraw and repair the evidence without destabilizing passing code.

## Statement

Close-out evidence claims require landed assertions and source citations; when prose overclaims, withdraw and repair the evidence without destabilizing passing code.

## Evidence

- [[session-20260714-065952]] records the independent verification that distinguished green implementation from an unsupported “all 13 steps matched” close-out claim, and the operator decision to preserve accepted code while reopening the proof.
- [[commit-24dae37]], [[commit-ef285f2]], and [[commit-ae09fba]] add the missing store-reopen, exact-instance, preservation, failure/no-op, and ordered final-chain assertions.
- [[commit-15b3377]] reissues the [supervisor lifecycle SPEC](layer/surface/build/feat/supervisor-lifecycle/SPEC.md) with a source-line citation for every Required Integration Proof step and records the original overclaim as withdrawn.
- [[commit-7f938a4]] merges the corrected evidence ledger only after the required Tier 1 CI passed.

## Supports

- [[authority-docs-state-facts-and-outcomes]] by requiring documentation about governed behavior to name concrete facts, outcomes, and verifiable evidence.
- [[iroh-noq-evidence-before-rules]] by generalizing the discipline that evidence must precede durable technical claims.

## Attacks

- Declaring a required proof complete from a green aggregate suite when the named assertions do not establish every claimed obligation.
- Reworking or reverting passing implementation merely to hide that close-out prose exceeded its evidence.

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- [[commit-24dae37]] reopens config, identity, and SQLite stores and proves exact-instance and populated artifact/blob preservation in the primary integration test.
- [[commit-ef285f2]] adds named failure/no-op proofs and fixes only the two durable-refusal gaps those tests exposed.
- [[commit-ae09fba]] reconstructs the full bootstrap/start/readiness/stop/reconciliation/uninstall chain after final ledger reopen.
- [[commit-15b3377]] replaces the withdrawn close-out with line-cited evidence while leaving Track 3 dispositions unchanged.
- [[commit-f6c8a19]] withdraws unsupported 100k-run and per-commit-evidence claims and commits the reproducible workspace-test transcript instead.

## Revision Log

- 2026-07-16: Created — metrics computed by `patina scrape`
- 2026-07-25: Applied to [[commit-f6c8a19]], which repaired the post-R3 credibility close-out to landed, reproducible evidence.

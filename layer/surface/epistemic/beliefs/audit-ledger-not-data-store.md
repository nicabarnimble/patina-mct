---
type: belief
id: audit-ledger-not-data-store
persona: mct-operator
facets: [mct, observability, payloads, privacy]
entrenchment: high
status: active
endorsed: true
extracted: 2026-07-05
revised: 2026-07-05
---

# audit-ledger-not-data-store

Audit ledgers record integrity facts (digest, size, classification), never payload bytes. The ledger is an audit spine, not a data store.

## Statement

Audit ledgers record integrity facts (digest, size, classification), never payload bytes. The ledger is an audit spine, not a data store.

## Evidence

- Accepted Phase 5 payload data plane gate and [SPEC.md](layer/surface/build/feat/payload-data-plane/SPEC.md) observability invariant require no payload bytes in JSONL ledger entries.

## Supports

<!-- Add beliefs this supports -->

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

<!-- Add concrete applications -->

## Revision Log

- 2026-07-05: Created — metrics computed by `patina scrape`

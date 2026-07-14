---
type: belief
id: protocol-outcomes-survive-projection
persona: architect
facets: [mct, protocols, outcomes, projection, wire]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-07-13
revised: 2026-07-13
---

# protocol-outcomes-survive-projection

Intermediate protocol models must preserve governed semantic outcomes end-to-end; never collapse them into narrower outcomes to simplify projection.

## Statement

Intermediate protocol models must preserve governed semantic outcomes end-to-end; never collapse them into narrower outcomes to simplify projection.

## Evidence

- [[session-20260712-160953]] records the Track 3 expected-red probe that found `ResultOutcome::Cancelled` collapsing into `CallProtocolOutcome::Failed` at the resident-to-Iroh boundary.
- [[commit-8565636]] adds cancellation to the protocol evaluation model and preserves it through caller-safe wire reply projection, idempotent replay, and result observations.
- [mct-product-map.allium](layer/allium/mct-product-map.allium) governs the closed result outcome set and the outcome-conditional `route_taken` rule: cancellation remains cancellation and carries no route projection.
- [LEDGER.md](layer/surface/build/spec-drift-audit/track3/LEDGER.md) ties the closed outcome, replay, route projection, and observation obligations to named tests.

## Supports

- [[mct-call-protocol-wraps-semantic-call]] by requiring the transport protocol to preserve semantic call outcomes instead of inventing a narrower result model.
- [[typed-domain-records-before-algorithms]] by preferring an explicit outcome variant and exhaustive matches over fallback conversion logic.
- [[authority-docs-state-facts-and-outcomes]] by keeping caller-visible and durable outcome facts aligned with the governed domain meaning.

## Attacks

- Outcome projection that maps a governed semantic state to a merely similar state because an intermediate enum omitted the correct variant.
- Wildcard match arms or adapter fallbacks that hide missing protocol states.

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- [[commit-8565636]] makes `CallProtocolOutcome::Cancelled` explicit and updates every exhaustive projection site.
- [[commit-8565636]] proves cancelled replies suppress `route_taken`, idempotent replay remains cancelled, and buffered/before-effect observations retain the cancelled outcome.
- [[commit-d5cbefc]] merges the complete contract-ledger slice with zero remaining GAP or LAW-LEADS-CODE rows.

## Revision Log

- 2026-07-13: Created from the operator-adjudicated Track 3 cancellation drift — metrics computed by `patina scrape`.

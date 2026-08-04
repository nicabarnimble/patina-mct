---
type: belief
id: authority-freshness-requires-mother-state
persona: mct-operator
facets: [mct, authority, security, provenance]
entrenchment: high
status: active
endorsed: true
extracted: 2026-08-02
revised: 2026-08-02
---

# authority-freshness-requires-mother-state

Authority freshness is valid only when compared against current state independently supplied by the executing Mother, never the request or token being checked.

## Statement

Authority freshness is valid only when compared against current state independently supplied by the executing Mother, never the request or token being checked.

## Evidence

- [[session-20260724-223731-101286000]] traced the vacuous grants-revision comparison in `crates/mct-daemon/src/daemon/resident/execution.rs` against the independent-currentness law in [MCT product map](../../../allium/mct-product-map.allium), while distinguishing token/call binding from current-authority freshness.

## Supports

- [[mother-kernel-decides-adapters-perform]] by requiring Mother-owned current facts, rather than adapter-carried claims, to govern execution.
- [[authority-docs-state-facts-and-outcomes]] by requiring freshness claims to name the authoritative state source.
- [[iroh-provides-connectivity-not-authority]] by refusing to treat peer-carried revisions as local execution authority.

## Attacks

- Treating a revision copied from the request, call, or minted token as proof that authority remains current.

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- [[session-20260724-223731-101286000]] applies this distinction to the M2 grants-authority review and separates token/call binding from effect-time freshness.

## Revision Log

- 2026-08-02: Created — metrics computed by `patina scrape`

---
type: belief
id: mct-audits-patina-before-preserve-replace
persona: architect
facets: [architecture, product-boundary, migration, mct]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-07-09
revised: 2026-07-09
---

# mct-audits-patina-before-preserve-replace

MCT should audit Patina for preserve/replace decisions before building replacement runtime features; it should not port Patina internals into MCT.

## Statement

MCT should audit Patina for preserve/replace decisions before building replacement runtime features. Existing Patina Mother behavior is prior art and evidence, but MCT must keep its runtime/orchestration boundary clean: preserve useful operator and domain patterns, replace legacy trust/coupling with explicit MCT authority, and leave Patina knowledge-product internals in Patina.

## Evidence

- [[session-20260709-091408]] records the user clarification that MCT will not absorb Patina Belief/scry/assay internals, and that interface launching should likely become an MCT-managed child/app rather than copied Mother internals.
- `layer/surface/build/product/MCT-NEXT-BUILD-TODO.md` records the 2026-07-09 audit comparing existing Patina Mother routing, storage/network capability handling, and supervisor wrappers against the MCT direction.
- `layer/surface/build/product/MCT-NEXT-BUILD-TODO.md` records preserve/replace decisions for Multi-Mother: preserve UDS-first local control and fail-closed auth ideas, but replace HTTP `/child/{child}/{action}`, graph/federation knowledge routing, and local native-job peer enqueue as cross-Mother runtime trust models.

## Supports

- [[greenfield-products-reference-legacy]]
- [[mother-kernel-decides-adapters-perform]]
- [[iroh-endpointid-is-transport-identity]]

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- `layer/surface/build/product/MCT-NEXT-BUILD-TODO.md` applies this belief to sequence Multi-Mother before JVM SDK hardening and to turn Patina Mother behavior into explicit preserve/replace decisions.

## Revision Log

- 2026-07-09: Created from the post-v0 MCT next-build alignment discussion.

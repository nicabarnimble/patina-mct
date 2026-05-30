---
type: belief
id: greenfield-products-reference-legacy
persona: architect
facets: [architecture, product-boundary, migration]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-05-28
revised: 2026-05-28
---

# greenfield-products-reference-legacy

Greenfield Patina product splits should use the integrated Patina repo as evidence and prior art, not as a source tree to move wholesale.

## Statement

Greenfield Patina product splits should use the integrated Patina repo as evidence and prior art, not as a source tree to move wholesale.

## Evidence

- `layer/surface/patina-mct.org` records the agreed answer to build `patina-mct` greenfield and treat `/Users/nicabar/Projects/Sandbox/AI/RUST/patina` as reference material.
- [[mother-kernel-design]] applies that boundary to the Mother kernel design by using integrated repo paths as evidence while designing a new kernel shape.

## Supports

<!-- Add beliefs this supports -->

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- `layer/surface/patina-mct.org` uses this belief to separate feature mapping from source extraction.
- `layer/slate/work/mother-kernel-design/mother-kernel-design.md` uses this belief to reject copying current `ServerState`, `ApiRuntime`, or WASM runtime internals wholesale.

## Revision Log

- 2026-05-28: Created — metrics computed by `patina scrape`

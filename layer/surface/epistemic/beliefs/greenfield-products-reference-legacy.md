---
type: belief
id: greenfield-products-reference-legacy
persona: architect
facets: [architecture, product-boundary, migration]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-05-28
revised: 2026-07-14
---

# greenfield-products-reference-legacy

Greenfield product work should treat `patinaMother`, `patinaChild`, and `patinaToy` as operational evidence and prior art, translating accepted responsibilities rather than moving the integrated source tree wholesale.

## Statement

Greenfield product work should treat `patinaMother`, `patinaChild`, and `patinaToy` as operational evidence and prior art. Accepted responsibilities are translated or rebuilt under MCT law; intentional code reuse is a port and requires explicit justification.

## Evidence

- `layer/surface/patina-mct.org` records the agreed answer to build `patina-mct` greenfield and treat the [published integrated Patina revision](https://github.com/NicabarNimble/patina/tree/d8f90270a53047b99d12004f834b62dbc629570d) as reference material.
- [[mother-kernel-design]] applies that boundary by using integrated repo paths as evidence while designing the `mctMother` kernel shape.

## Supports

<!-- Add beliefs this supports -->

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- `layer/surface/patina-mct.org` uses this belief to separate feature mapping from source extraction.
- `layer/slate/work/mother-kernel-design/mother-kernel-design.md` uses this belief to reject copying `patinaMother` `ServerState`/`ApiRuntime` and `patinaChild` WASM runtime internals wholesale.

## Revision Log

- 2026-07-14: Adopted comparative namespace vocabulary and migration verb discipline.
- 2026-05-28: Created — metrics computed by `patina scrape`

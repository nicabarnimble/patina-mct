---
type: belief
id: mother-kernel-decides-adapters-perform
persona: architect
facets: [mct, rust, architecture, kernel]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-05-28
revised: 2026-05-28
---

# mother-kernel-decides-adapters-perform

The Mother kernel should decide authority, lifecycle, routing, and toy grants; runtime adapters should perform effects outside the kernel.

## Statement

The Mother kernel should decide authority, lifecycle, routing, and toy grants; runtime adapters should perform effects outside the kernel.

## Evidence

- [[mother-kernel-design]] records the kernel mantra: kernel decides, adapters perform, daemon exposes, children execute, toys mediate effects.
- `layer/surface/patina-mct.org` records MCT as the runtime/control-plane product boundary and classifies runtimes as adapter surfaces outside the kernel.

## Supports

<!-- Add beliefs this supports -->

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- `layer/slate/work/mother-kernel-design/mother-kernel-design.md` uses this belief to keep process, JVM/Clojure, WASM, store, secrets, and p2p implementations outside the kernel.

## Revision Log

- 2026-05-28: Created — metrics computed by `patina scrape`

---
type: belief
id: adapter-storage-facts-kernel-decisions
persona: mct-operator
facets: [mct, adapters, kernel, storage, integrity]
entrenchment: high
status: active
endorsed: true
extracted: 2026-07-06
revised: 2026-07-06
---

# adapter-storage-facts-kernel-decisions

When adding adapter storage, the adapter should gather bounded facts and let the existing kernel evaluator classify integrity failures, rather than creating a parallel decision path.

## Statement

When adding adapter storage, the adapter should gather bounded facts and let the existing kernel evaluator classify integrity failures, rather than creating a parallel decision path.

## Evidence

- [[session-20260702-073627-029329000]]: D6 local CAS work kept file I/O and bounded fetches in `crates/mct-daemon/src/blob_store.rs` while extending `crates/mct-kernel/src/call/mod.rs` and `crates/mct-kernel/src/call/internal.rs` so `[[commit-2770aa7]]` classified absent/tampered blob facts through `evaluate_payload_integrity`; `[[commit-722a2c5]]` applied the pattern in resident local blob consumption, and `[[commit-122424d]]` closed the phase.

## Supports

- [[mother-kernel-decides-adapters-perform]]
- [[payload-integrity-before-authority]]

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

<!-- Add concrete applications -->

## Revision Log

- 2026-07-06: Created — metrics computed by `patina scrape`

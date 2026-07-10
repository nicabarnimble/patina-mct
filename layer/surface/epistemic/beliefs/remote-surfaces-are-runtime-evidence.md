---
type: belief
id: remote-surfaces-are-runtime-evidence
persona: architect
facets: [mct, routing, authority, multi-mother, iroh, observations]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-07-09
revised: 2026-07-09
---

# remote-surfaces-are-runtime-evidence

Remote callable surfaces should be treated as revocable runtime evidence, not durable config authority; every remote network effect should revalidate that evidence immediately before opening the stream.

## Statement

Remote callable surfaces should be treated as revocable runtime evidence, not durable config authority; every remote network effect should revalidate that evidence immediately before opening the stream.

## Evidence

- [[session-20260709-091408]] records the multi-Mother implementation decision that remote callable surfaces remain runtime evidence and are revalidated before forwarding effects.
- [SPEC.md](layer/surface/build/feat/multi-mother-route-forwarding/SPEC.md) and [DESIGN.md](layer/surface/build/feat/multi-mother-route-forwarding/DESIGN.md) define remote surfaces as runtime evidence.
- [[commit-7475788]], [[commit-88625ff]], [[commit-283080a]], and [[commit-c274144]] implement storage, candidate authority, revalidation, forwarding, and observations.

## Supports

- [[mother-kernel-decides-adapters-perform]] by keeping remote surface facts as evidence for route authority rather than granting authority through transport/config alone.
- [[iroh-endpointid-is-transport-identity]] by requiring signed peer binding and fresh surface evidence above the transport endpoint identity.
- [[mct-hello-precedes-protected-peer-effects]] by refreshing remote callable-surface evidence through admitted hello before protected `mct/call/0` effects.

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- [[commit-7475788]] stores remote callable surfaces in runtime state and refreshes them from admitted hello capability views.
- [[commit-88625ff]] generates executable remote candidates only from fresh stored surfaces and preserves `SecretScopeForbidden`.
- [[commit-283080a]] revalidates the selected remote candidate immediately before opening the outbound `mct/call/0` stream.
- [[commit-c274144]] records forwarding and execution observations without payload bytes.

## Revision Log

- 2026-07-09: Created — metrics computed by `patina scrape`

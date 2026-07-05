---
type: belief
id: payload-integrity-before-authority
persona: mct-operator
facets: [mct, security, payloads, authority]
entrenchment: high
status: active
endorsed: true
extracted: 2026-07-05
revised: 2026-07-05
---

# payload-integrity-before-authority

Payload integrity is verified before authority evaluation; integrity failures are typed outcomes that never execute.

## Statement

Payload integrity is verified before authority evaluation; integrity failures are typed outcomes that never execute.

## Evidence

- Accepted Phase 5 payload data plane gate and [SPEC.md](layer/surface/build/feat/payload-data-plane/SPEC.md) validation order require adapter-side integrity before authority.

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

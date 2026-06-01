---
type: belief
id: typed-domain-records-before-algorithms
persona: architect
facets: [mct, rust, architecture, authority, workflow]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-05-31
revised: 2026-05-31
---

# typed-domain-records-before-algorithms

Typed domain records should land before planner, transport, or runtime algorithms so later behavior remains reviewable, authority-safe, and independent of adapter implementation details.

## Statement

Typed domain records should land before planner, transport, or runtime algorithms so later behavior remains reviewable, authority-safe, and independent of adapter implementation details.

## Evidence

- [[session-20260529-070316-510393000]] records that [[mct-slice-kernel-routing-decision-records]] added RouteDecision/CandidateRoute records before any planner algorithm and [[mct-slice-kernel-no-route-denial]] added deny-by-default route facts before route optimization behavior.

## Supports

- [[mother-kernel-decides-adapters-perform]] by keeping authority facts explicit before adapters execute effects.
- [[slate-epics-progress-through-child-slices]] by reinforcing narrow domain-record slices before broad algorithmic epics.


## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- [[mct-slice-kernel-routing-decision-records]] added route/candidate authority records without adding a planner.
- [[mct-slice-kernel-no-route-denial]] added no-route denial facts before optimizer behavior.
- `crates/mct-kernel/src/route.rs` is the current concrete application.


## Revision Log

- 2026-05-31: Created — metrics computed by `patina scrape`

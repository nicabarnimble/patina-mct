---
id: spec-driven-design
layer: core
status: active
created: 2026-05-31
revised: 2026-10-19
tags: [governance, decisions, specifications, mct, build-process]
references: [dependable-rust, unix-philosophy, session-capture, mct-build-boundaries]
---

# Decision- and Evidence-Driven MCT Build

Implementation follows accepted product decisions and reviewable work scope.
Tests establish what works; they cannot silently authorize a different policy.

## Sources

- `docs/DECISIONS.md`: settled decisions and their status.
- Existing work specifications under `layer/surface/build/feat/`: scope and
  expected behavior for substantial changes.
- Rust source and executable tests: implemented behavior.
- Research, beliefs and session records: rationale and continuity.

Allium is retired. `layer/allium/` is frozen historical evidence, not a current
implementation authority or CI requirement.

## Working Rules

Before changing authority, persistence or compatibility, identify the accepted
contract, affected boundary, observable outcome and tests that establish it.
Update existing records when the contract changes; do not create another document
for an ordinary refactor, local experiment or already-covered implementation.

Keep caller expectations distinct from Mother authority. Adapters supply facts
and perform admitted effects; substrate convenience never grants permission.

Record validation accurately, including failures, untested cases and remaining
work. Passing a targeted test does not establish that an unfinished runtime or
release is ready.

## When to Ask the Owner

Ask when a decision would change permissions, trust boundaries, supported product
scope, compatibility commitments or destructive/external operational policy that
has not already been authorized. Surface conflicts with settled decisions rather
than choosing a permissive default.

Module structure, test seams and representing already-approved failure behavior
are implementation decisions. A trait does not require owner approval merely
because it has one implementation.

Continue approved work across commits and implementation portions. Completing a
portion is not itself a reason to stop or request the same permission again.

## References

- [Reliability Doctrine](./reliability-doctrine.md)
- [MCT Build Boundaries](./mct-build-boundaries.md)
- [Safety Boundaries](./safety-boundaries.md)

# Compose kernel route revalidation

## Story
Add an execution-time revalidation decision that links an initial `RouteDecision` to fresh child and toy authority evidence before adapter effects execute.

## Why
`patinaMother` has practical runtime guardrails. `mctMother` should translate that requirement by making the final pre-effect authority check an explicit kernel fact rather than a daemon-side convention. Stale policy, mismatched selected route, revoked child authority, or revoked toy authority must produce a denial record, not a best-effort fallback.

## Direction
- Initial routing remains immutable.
- Revalidation is a separate decision linked to the initial decision.
- Child and toy authority results are inputs, not hidden context.
- No planner, storage, or adapter types in this slice.

## Context
- `layer/allium/mct-product-map.allium` RouteDecision and observation sections.
- `layer/core/safety-boundaries.md`: observations before protected effects.
- `crates/mct-kernel/src/route.rs`
- `crates/mct-kernel/src/child.rs`
- `crates/mct-kernel/src/toy.rs`

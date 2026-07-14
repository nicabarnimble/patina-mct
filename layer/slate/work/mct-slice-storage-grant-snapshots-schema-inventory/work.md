# Persist toy grant snapshots and storage inventory

## Story
Harden the private MCT state store toward accepted `patinaMother` operational requirements by adding `mctToy` catalog/grant snapshot persistence and documenting the intentionally narrow schema.

## Why
`patinaMother` has broad SQL depth. `mctMother` should rebuild useful durability and guardrails while rejecting broad belief/session/view/runtime coupling. Toy grants are authority-bearing facts, so a durable snapshot belongs in MCT state; Belief and UI tables do not.

## Direction
- Concrete SQLite tables inside `mct-daemon`.
- Kernel types remain storage-agnostic.
- Schema inventory records what is included/excluded.

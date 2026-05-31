---
id: safety-boundaries
status: verified
verification_date: 2025-08-02
oxidizer: nicabar
references: [spec-driven-design, oxidized-knowledge, iroh-endpointid-is-transport-identity]
tags: [safety, security, boundaries]
---

# Safety Boundaries

Patina MCT respects system boundaries and operates safely within designated areas.

## The Pattern

Patina operates within clear boundaries:

1. **No unsafe code by default** - Rust's safety guarantees are maintained unless a SPEC explicitly authorizes and justifies an exception.
2. **Project-scoped files** - Modify this repository intentionally; do not touch system or unrelated project files without explicit request.
3. **User consent** - Ask before major operations, network/service changes, destructive file changes, or long-running background work.
4. **Privacy respected** - Personal sessions stay local; project beliefs/evidence are git-tracked only when intentionally promoted.
5. **Authority boundaries respected** - Iroh EndpointId, relay reachability, discovery records, and child manifests are not treated as MCT authority without explicit binding/grants.
6. **Kernel/adapters separated** - Mother kernel decisions stay in domain code; adapters perform external effects and emit observations.

## Implementation

- Prefer paths relative to project root for repo edits.
- Session artifacts live in `layer/sessions/`; active interface traces may remain local.
- No network calls, service starts, relay changes, or pushes without consent.
- Clear separation of user-local data, project evidence, and runtime authority state.
- Record authority-affecting decisions in Allium/Slate/beliefs before implementing them.

## Consequences

- Users trust Patina's operations
- No accidental system changes
- Clear data ownership
- Safe to use anywhere
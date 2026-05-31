---
id: oxidized-knowledge
layer: core
status: active
created: 2026-05-31
revised: 2026-05-31
tags: [knowledge, mct, layer, provenance, beliefs]
references: [session-capture, spec-driven-design, mct-build-boundaries]
---

# MCT Oxidized Knowledge

**Purpose:** Explain how design knowledge hardens into MCT build law without confusing design-time Patina knowledge with runtime MCT authority.

---

## Core Separation

Patina MCT has two different kinds of truth:

| Truth | Location | Used for |
|-------|----------|----------|
| Design-time project knowledge | `layer/`, `.patina/` | guiding agents and humans building MCT |
| Runtime MCT truth | `MctObservation` ledger and runtime stores | deciding/auditing Mother/Child/Toy behavior |

Do not mix them.

Beliefs, sessions, scrape, scry, assay, and oxidize are design/build aids. The MCT runtime must not depend on them for authority.

At runtime:

```text
MctObservation ledger wins.
ToyGrant facts win.
MctPeerBinding facts win.
Policy snapshots win.
```

## Project Layer Map

```text
layer/core/                    hardened build principles
layer/allium/                  MCT product/domain behavior
layer/slate/work/              build queue, dependencies, proof plans
layer/surface/                 active evidence and product notes
layer/surface/epistemic/       beliefs and rationale
layer/sessions/                chronological work context
layer/dust/                    archived or deprecated knowledge
```

## Current MCT Belief Spine

Key beliefs currently shaping the build:

- [[mct-builds-on-iroh-substrate]]
- [[iroh-provides-connectivity-not-authority]]
- [[iroh-endpointid-is-transport-identity]]
- [[mct-hello-precedes-protected-peer-effects]]
- [[mct-call-protocol-wraps-semantic-call]]
- [[mother-kernel-decides-adapters-perform]]

These beliefs guide implementation, but code still traces to Allium/Slate.

## Knowledge Flow

```text
session discussion
  → evidence artifact or belief
  → Allium/Slate update
  → implementation
  → observations/tests/commit
  → session update
```

`patina scrape` makes these relationships discoverable. It does not make a belief true by itself; truth comes from evidence, validation, and successful application.

## Promotion Model

- **Surface → Core**: a pattern is repeatedly used and stable enough to guide future work.
- **Surface → Dust**: a pattern failed, became obsolete, or was superseded.
- **Session → Belief**: a repeated principle or strong decision becomes reusable rationale.
- **Belief → Allium/Slate**: rationale becomes executable design scope.

The docs in `layer/core/` are build laws. They should be fewer, stronger, and more stable than session notes.

## MCT-Specific Warning

MCT is itself an observability/authority system. That makes it easy to accidentally confuse Patina's design-time knowledge graph with MCT's runtime ledger.

Do not do that.

```text
Patina layer helps us build MCT.
MctObservation helps MCT explain itself at runtime.
```

## References

- [Session Capture](./session-capture.md)
- [Spec-Driven Design](./spec-driven-design.md)
- [MCT Build Boundaries](./mct-build-boundaries.md)

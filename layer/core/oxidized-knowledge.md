---
id: oxidized-knowledge
layer: core
status: active
created: 2026-05-31
revised: 2026-10-19
tags: [knowledge, mct, layer, provenance, beliefs]
references: [session-capture, spec-driven-design, mct-build-boundaries]
---

# MCT Oxidized Knowledge

Patina project knowledge and MCT runtime authority are different things.

| Knowledge | Location | Purpose |
| --- | --- | --- |
| Design-time project knowledge | `layer/`, `.patina/` | Guide humans and agents building MCT |
| Runtime MCT truth | Canonical `MctObservation` ledger | Establish and audit Mother/Child/Toy authority and behavior |

Beliefs, sessions, scrape, scry, assay, and oxidize are development aids. They do
not grant runtime permissions. Runtime projections and caches must remain
subordinate to canonical facts.

Recording or indexing a belief does not establish its truth. Implementation
claims require executable evidence; changes to authority require an explicit
accepted decision.

## References

- [Decision- and Evidence-Driven Build](./spec-driven-design.md)
- [Reliability Doctrine](./reliability-doctrine.md)

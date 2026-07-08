---
type: belief
id: course-material-source-material-separated
persona: mct-operator
facets: [allium, training, documentation, repository-boundaries]
entrenchment: high
status: active
endorsed: true
extracted: 2026-07-08
revised: 2026-07-08
---

# course-material-source-material-separated

Course material should live in a dedicated learning repo, while product repos should retain real source artifacts and stable pointers to them for learners.

## Statement

Course material should live in a dedicated learning repo, while product repos should retain real source artifacts and stable pointers to them for learners.

## Evidence

- [[session-20260702-073627-029329000]]: The Allium curriculum was moved to https://github.com/nicabarnimble/learn-allium in [[commit-b757537]], while Patina and MCT kept pointer docs in [[commit-d8f90270]] and [[commit-613061a]] and retained production Allium artifacts as real-world references.

## Supports

- [[authority-docs-state-facts-and-outcomes]]

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- `docs/allium-training.md` in Patina now points learners to https://github.com/nicabarnimble/learn-allium while keeping `layer/allium/mother/*.allium` as source material.
- `docs/allium-training.md` in MCT now points learners to https://github.com/nicabarnimble/learn-allium while linking `layer/allium/mct-product-map.allium`, `scripts/install-allium-ci.sh`, `scripts/ci-tier0.sh`, and `layer/core/spec-driven-design.md` as real-world references.

## Revision Log

- 2026-07-08: Created — metrics computed by `patina scrape`

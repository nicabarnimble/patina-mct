---
type: belief
id: course-material-source-material-separated
persona: mct-operator
facets: [allium, training, documentation, repository-boundaries]
entrenchment: low
status: superseded
endorsed: false
extracted: 2026-07-08
revised: 2026-10-18
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

- Historical: `docs/allium-training.md` once pointed to the standalone
  curriculum. MCT retired that pointer and its Allium development dependency
  under D-50 while preserving the former artifacts in Git.

## Revision Log

- 2026-07-08: Created — metrics computed by `patina scrape`
- 2026-10-18: Superseded by D-50 after Allium was removed from MCT's active
  development and CI path.

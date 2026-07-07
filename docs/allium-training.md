# Allium Training

The team's Allium training curriculum lives in the standalone learning repo:

> https://github.com/nicabarnimble/learn-allium

Use that repo for session guides, slides, exercises, cheat sheets, and facilitator material.

## Why this pointer is here

Several patina-mct artifacts are first-class course material:

- `layer/allium/mct-product-map.allium` — the "large real spec" exhibit
  for Sessions 3, 4, and 6 (patterns: authority records, two-phase
  routing, safe projections, terminal results):
  <https://github.com/nicabarnimble/patina-mct/blob/main/layer/allium/mct-product-map.allium>
- `scripts/install-allium-ci.sh` — the pinned, SHA-verified CLI install
  (Session 5 toolchain-reproducibility example; also the Session 1
  distill demo subject):
  <https://github.com/nicabarnimble/patina-mct/blob/main/scripts/install-allium-ci.sh>
- `scripts/ci-tier0.sh` — `allium check layer/allium` as a tier-0 gate:
  <https://github.com/nicabarnimble/patina-mct/blob/main/scripts/ci-tier0.sh>
- `layer/core/spec-driven-design.md` — the doctrine the whole course
  teaches: *Allium says what MCT is; Slate says what work is ready;
  beliefs/evidence say why; code executes inside that boundary.*
  <https://github.com/nicabarnimble/patina-mct/blob/main/layer/core/spec-driven-design.md>

Capstone projects scoped to MCT slices (`layer/slate/work/`) should nest
under `mct-product-map.allium` rather than contradict it — see the capstone
brief in the learn-allium repo.

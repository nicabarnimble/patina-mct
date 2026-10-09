# Allium training

The team's Allium training curriculum lives in the standalone learning
repository:

> <https://github.com/nicabarnimble/learn-allium>

Use that repository for session guides, slides, exercises, cheat sheets, and
facilitator material.

## MCT source material

Several MCT artifacts are first-class course material:

- [`mct-product-map.allium`](https://github.com/nicabarnimble/patina-mct/blob/main/layer/allium/mct-product-map.allium)
  is the large real-spec exhibit for authority records, two-phase routing,
  safe projections, and terminal results.
- [`install-allium-ci.sh`](https://github.com/nicabarnimble/patina-mct/blob/main/scripts/install-allium-ci.sh)
  demonstrates a pinned, SHA-verified CLI installation.
- [`ci-tier0.sh`](https://github.com/nicabarnimble/patina-mct/blob/main/scripts/ci-tier0.sh)
  shows `allium check layer/allium` as a tier-zero gate.
- [`spec-driven-design.md`](https://github.com/nicabarnimble/patina-mct/blob/main/layer/core/spec-driven-design.md)
  explains how Allium law, build scope, evidence, and code relate.

Capstone projects scoped to MCT slices must nest under
`mct-product-map.allium` rather than contradict it.

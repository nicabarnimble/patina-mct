# Frozen Allium maps

Status: **historical — frozen 2026-10-18**

These files preserve the behavioral maps that guided earlier MCT design and
implementation. They remain in their original location so historical commits,
session records, audit ledgers, and slice close-outs retain valid references.

They are no longer:

- a semantic-law override;
- required input for a feature;
- maintained alongside new behavior;
- validated by Tier 0 or GitHub Actions; or
- a prerequisite for implementation.

Current intended behavior is governed by:

1. `docs/DECISIONS.md` for settled decisions and status;
2. active ratified specifications under `layer/surface/build/feat/` for bounded
   work;
3. `layer/core/` for standing doctrine; and
4. source plus executable tests for what is actually built.

Restoring Allium to the active development path requires a new explicit
project-governance decision. The exact pre-retirement repository state is Git
commit `e3d01b933f59331204537208448eeee741c2de70`.

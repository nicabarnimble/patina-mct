# Design notes

Design notes imported on 2026-10-18 from working discussions that were not in
the repository. Each note keeps its original body apart from link fixes, and
starts with a banner giving its status and what has changed on `main` since it
was written. Where a note and [../DECISIONS.md](../DECISIONS.md) disagree, the
decision log is newer. The frozen Allium maps are historical context and no
longer override either source.

| Note | Written | Status |
| --- | --- | --- |
| [admin-keys.md](admin-keys.md) | 2026-10-14, edited to 2026-10-17 | The main design. Locked decisions (a)–(f), bootstrap, scopes, recovery-key lockout, scheme v1, hard cutover, the family narrative, the courier rule, and the proposed decision-model and replay Toys. |
| [admin-keys-code-map.md](admin-keys-code-map.md) | 2026-10-14 | Code-change map for admin keys. Line numbers are stale. |
| [build-plan.md](build-plan.md) | 2026-10-14 | Ten-phase build order. Phases 1–2 done, 3 partly done. |
| [pi-durable-child.md](pi-durable-child.md) | 2026-10-14 | Pi Durable placed inside MCT as a Child. Three conflicts ruled, four open. |
| [pi-durable-handoff.md](pi-durable-handoff.md) | 2026-10-14 | Notes from a call: older Child versus "uncle/aunt", pending speed levers. |
| [child-start-ledger-fact.md](child-start-ledger-fact.md) | 2026-10-18 | Proposed fix for the double-run-on-power-loss bug. Seven open questions. |

Design material that was already in the repository stays where it is:

- [`layer/allium/`](../../layer/allium/): frozen historical product and peer maps.
- [`layer/core/`](../../layer/core/): doctrine.
- [`layer/surface/build/feat/`](../../layer/surface/build/feat/): one spec per build slice, with numbered decisions.
- [`layer/surface/build/product/`](../../layer/surface/build/product/): roadmap, next-build TODO, runbooks.

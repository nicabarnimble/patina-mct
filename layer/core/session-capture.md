---
id: session-capture
layer: core
status: active
created: 2026-05-31
revised: 2026-10-19
tags: [sessions, mct, provenance, workflow]
references: [spec-driven-design, oxidized-knowledge]
---

# MCT Session Capture

Preserve enough context to continue work without reconstructing the conversation:

- user intent and accepted decisions;
- important reasoning, disagreements and unresolved questions;
- commits and meaningful changes;
- validation actually performed, including failures and limitations;
- remaining work and the next action.

Distinguish proposals, approvals, implementation and test results. Session notes
preserve evidence and continuity; they do not independently authorize a change
to accepted implementation contracts or grant runtime permission.
Use existing records rather than creating parallel bookkeeping for the same work.

## References

- [Decision- and Evidence-Driven Build](./spec-driven-design.md)
- [Design-time and Runtime Knowledge](./oxidized-knowledge.md)

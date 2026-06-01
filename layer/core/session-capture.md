---
id: session-capture
layer: core
status: active
created: 2026-05-31
revised: 2026-05-31
tags: [sessions, mct, provenance, workflow]
references: [spec-driven-design, oxidized-knowledge]
---

# MCT Session Capture

**Purpose:** Preserve why MCT decisions were made, which git range they cover, and which Allium/Slate/belief artifacts changed.

---

## Core Principle

Sessions are the reaction chamber for MCT design and build work.

They capture:

- user intent;
- questions and disagreements;
- evidence reviewed;
- Allium and Slate changes;
- beliefs created or revised;
- git range and commits;
- handoff context for the next run.

Sessions are evidence. They are not implementation authority by themselves. Non-trivial code still needs Allium/Slate scope.

## Runtime-Specific Workflow

This project uses runtime wrappers from `AGENTS.md`. In the current Pi runtime, use:

```bash
.pi/bin/session-update.sh
.pi/bin/session-note.sh "note"
```

Session updates should mention:

- active git range;
- files changed;
- Allium anchors touched;
- Slate work items touched;
- beliefs created/used;
- validation commands run;
- recommended next step.

## Linking Convention

Use wikilinks where possible so Patina scrape can trace the graph:

```text
[[mct-builds-on-iroh-substrate]]
[[iroh-endpointid-is-transport-identity]]
[[mct-hello-precedes-protected-peer-effects]]
[[commit-f059507]]
```

Use code formatting for paths:

```text
`layer/allium/mct-product-map.allium`
`layer/slate/work/mct-iroh-substrate/work.toml`
```

## What Good MCT Notes Capture

Good:

```text
Defined mct/call/0 as a peer transport envelope around semantic MctCall.
Updated Allium anchors MctCallProtocol* and Slate work mct-call-envelope.
Ran allium check and patina scrape. Next: implement mct-kernel records.
```

Weak:

```text
Worked on networking.
```

The first note preserves decisions. The second forces future rediscovery.

## References

- [Spec-Driven Design](./spec-driven-design.md)
- [Oxidized Knowledge](./oxidized-knowledge.md)

---
id: session-capture
status: verified
verification_date: 2026-02-12
oxidizer: nicabar
references: [spec-driven-design, oxidized-knowledge]
tags: [sessions, capture, workflow]
---

# Session Capture

Patina MCT captures development context through friction-free session tracking.


## The Pattern

Capture development context with minimal friction:

1. **Scripts handle mechanics** - Timestamps, git state, file tracking
2. **Markdown for humans** - Readable session files
3. **Progressive detail** - Start simple, enhance later
4. **Time-based organization** - Natural chronological flow

## Implementation

Sessions are YAML-frontmatter markdown files in `layer/sessions/`, managed by the runtime-specific wrappers documented in `AGENTS.md` and by Patina session commands when available. In this Pi runtime, use `.pi/bin/session-update.sh` for durable progress updates:

```yaml
---
type: session
id: 20260212-161126              # Timestamp-based ID
title: "feature name"
status: active                   # active → archived
runtime: pi                       # Which interface/runtime
created: 2026-02-12T21:11:26Z
git:
  branch: patina
  starting_commit: e1a1736c...
  start_tag: session-20260212-161126-claude-start
---

## Previous Session Context
## Goals
## Activity Log
## Beliefs Captured
## Git Range
## Handoff
```

Git tags bracket each session (`session-{id}-start` / `session-{id}-end`). Updates should name the active git range, e.g. `session-20260529-070316-510393000-pi-start..HEAD`, so commits can be traced back to session context.

## Consequences

- Natural documentation emerges
- No friction during development
- Context preserved for future
- Patterns ready for promotion
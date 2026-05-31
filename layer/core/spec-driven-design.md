---
id: spec-driven-design
layer: core
status: active
created: 2026-02-08
tags: [governance, specs, agentic, process, core-principle]
references: [dependable-rust, unix-philosophy, session-capture, mct-builds-on-iroh-substrate]
---

# Spec-Driven Design

**Purpose:** Allium specs and Slate work items are the single source of truth for non-trivial MCT action. Code traces to Allium/Slate, Allium/Slate traces to sessions and beliefs. The AI agent executes within that scope — never outside it.

---

## Core Principle

Every non-trivial MCT change must be authorized by Allium and/or Slate. The spec/work item is a contract: it defines what gets built, why, and what "done" looks like. Sessions capture the discussion that produces specs. Beliefs capture the principles that inform them. Code implements them. Nothing else authorizes action.

**Sessions discuss. Allium/Slate decide. Code executes.**

## The Problem This Solves

Without spec-driven governance, agentic development falls into a loop: rich sessions full of learning and iteration, but little action. Sessions have no contract and no exit criteria — they just end. The AI makes judgment calls that feel reasonable in the moment but diverge from intent. Knowledge accumulates but nothing ships.

SPECs break the loop by converting discussion into contracts with clear scope, exit criteria, and accountability.

## The Pipeline

```
session (discuss) → Allium/Slate (contract) → action (execute) → session (review)
```

Each stage has a different relationship with the AI agent:

- **Session**: Participatory. Discuss, suggest, explore, disagree.
- **Allium/Slate**: Subordinate. Execute what the spec/work item says. Surface gaps. Never fill them unilaterally.
- **Action**: Bounded. Every line of code traces to a SPEC. If the SPEC doesn't authorize it, it doesn't happen.

## Rules

### 1. Allium/Slate Is the Authority

The Allium spec and Slate work item define the scope of work. When the AI encounters an edge case they do not address, the correct action is to stop and ask — not to make a judgment call.

```
SPEC says: "Delete dead code"
AI finds:  dead functions with tests

Bad:  AI decides to preserve them with #[cfg(test)] (unilateral)
Good: AI asks "these functions are dead but have tests — delete both?"
```

### 2. SPEC Amendments Are First-Class

When a SPEC is wrong or incomplete, the fix is a new linked spec — not a silent divergence. This preserves decision provenance.

- **fix**: the original SPEC was wrong (use existing `fix` type)
- **refactor/feat**: the SPEC needs extension (use existing types)
- Link via `related` field in frontmatter to maintain the chain

The amendment chain gives you something most AI workflows lack: you can trace any piece of code back through code → commit → SPEC → amendment → original SPEC → session → beliefs.

### 3. Sessions Are Not Authorization

Sessions capture the messy process: discussion, false starts, questions, decisions-in-progress. They are raw material for SPECs, not substitutes for them.

A session can identify that work needs to happen. Only a SPEC can authorize it.

### 4. The Threshold

Not everything needs a full SPEC. The line:

**Needs a SPEC:**
- Changes to architecture or module boundaries
- New features or capabilities
- Refactors that touch multiple files
- Anything with exit criteria worth defining
- Any work where the AI might make judgment calls about scope

**Lives in commits/sessions:**
- Typo fixes, formatting
- Single-line bug fixes with obvious correctness
- Documentation updates to existing content

When in doubt, the answer is SPEC. The cost of an unnecessary SPEC is low (a small document). The cost of unscoped work is high (divergence, rework, lost intent).

### 5. Decision Provenance

Every piece of code should be traceable:

```
code → commit → SPEC → (optional amendment → original SPEC) → session → beliefs
```

This chain is what makes Patina's knowledge layer trustworthy. Without it, accumulated knowledge is just text. With it, every decision has context, rationale, and history.

### 6. Push Discoveries Outbound

Discoveries made during one work item's execution that affect other specs/work items must be pushed to the destination Allium section, Slate item, evidence artifact, or belief before the originating work can close.

Without this rule, archiving the originating spec severs the knowledge chain. The discovery lives in session logs (archived), beliefs (only if searched for), and commit messages (buried). None of these paths naturally surface when opening the destination spec.

```
Working on SPEC A → discover something that affects SPEC B

Bad:  Note it in session log, archive SPEC A, SPEC B never knows
Good: Push discovery to the affected Allium/Slate/belief artifact
      THEN close or archive the originating work
```

This applies at all three layers:
- **Process**: check for outbound discoveries before closing a spec
- **Tooling**: session notes, belief files, and Slate work updates now; richer discovery commands may come later
- **Structural**: Allium anchors, `belief_refs`, and `implementation_plan` entries carry outbound discoveries today

### 7. Ground Every Assertion

Every testable claim in Allium/Slate must carry evidence inline or nearby. An ungrounded assertion is a hypothesis masquerading as a contract — and review becomes the testing mechanism.

Three forms of grounding:

**Verification commands** — run the command during spec creation, document timing and expected output:
```
Bad:  "Run `rg 'foo'` — should return zero"
Good: "Run `rg 'foo' src/ tests/` post-commit — targets only
       actionable locations. Should return zero."
```

**Invariants** — every "doesn't change" claim needs a one-line justification:
```
Bad:  "build.rs files — do not change"
Good: "build.rs files — paths are crate-relative; cargo runs build.rs
       from crate root, so internal paths survive the directory move"
```

**Prerequisites** — state execution context before the command, not after a reviewer discovers it:
```
Bad:  "cargo package -p my-crate"
Good: "cargo package -p my-crate (post-commit; validates manifest
       and include/exclude globs, no registry credentials needed)"
```

The cost of grounding is one sentence per assertion. The cost of NOT grounding is 2-3 review cycles per assertion — and the cycles compound because each fix can introduce new unstated assumptions.

## Relationship to Other Patterns

**[[dependable-rust]]**: SPECs are the external interface for work. Like a module's public API, the SPEC is small, stable, and authoritative. Implementation details (how the AI gets there) are internal. The contract (what gets built) is the SPEC.

**[[unix-philosophy]]**: One SPEC, one job. A SPEC that tries to authorize everything authorizes nothing. Focused SPECs with clear exit criteria are composable — they can block each other, relate to each other, and build on each other.

**[[adapter-pattern]]**: The spec/work-item system is adapter-agnostic. Whether Pi, Claude, Gemini, OpenCode, or a human reads the contract, the scope is the same. The contract doesn't encode how to build — it encodes what to build and when it's done.

## Existing Infrastructure

This repository already supports this governance:

- **Allium**: `layer/allium/mct-product-map.allium` holds product/domain invariants.
- **Slate**: `layer/slate/work/*/work.toml` holds executable work items, dependencies, anchors, proof plans, and implementation plans.
- **Beliefs**: `layer/surface/epistemic/beliefs/*.md` provides the "why" behind the contract.
- **Evidence**: `layer/surface/*.md` stores durable research and design evidence.
- **Sessions**: `layer/sessions/*.md` provides the discussion and git-range history.
- **Commits**: commit messages and hashes connect implementation to the above.

What this pattern adds is the governance rule: these aren't just organizational tools, they're the authority system.

## Common Mistakes

**1. AI fills gaps instead of surfacing them**
```
SPEC says: "merge match arms"
AI finds:  unused imports after merge

Bad:  AI silently cleans up imports (reasonable but unauthorized)
Good: AI cleans up imports AND notes it as a consequence of the spec'd change
Best: SPEC anticipated this — "merge match arms, clean up dead code"
```

**2. Sessions substitute for SPECs**
```
Bad:  "We discussed adding caching in the session, so I'll add it"
Good: "We discussed caching — should I draft a SPEC for it?"
```

**3. SPECs are too broad**
```
Bad:  SPEC: "Improve the retrieval system"
Good: Slate: "Define mct/hello/0 peer admission gate" (Phase 1)
      Slate: "Define mct/call/0 peer protocol" (Phase 2)
```

**4. Amendments bypass the chain**
```
Bad:  Edit the original SPEC to change scope mid-work
Good: Create a fix/patch SPEC that links to the original, preserving history
```

## References

- [Dependable Rust](./dependable-rust.md) - Black-box module pattern (specs as external interface)
- [Unix Philosophy](./unix-philosophy.md) - One tool, one job (one work item, one scope)
- [Adapter Pattern](./adapter-pattern.md) - Agent-agnostic contracts
- [Session Capture](./session-capture.md) - The discussion layer that feeds Allium/Slate
- [MCT product map](../allium/mct-product-map.allium) - Current authoritative product/domain spec

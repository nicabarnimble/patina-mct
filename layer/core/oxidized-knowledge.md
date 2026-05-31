---
id: oxidized-knowledge
status: active
created: 2025-08-11
updated: 2026-02-12
references: [session-capture, adapter-pattern]
tags: [architecture, metaphor, core]
---

# Patina MCT - Oxidized Knowledge

**Purpose:** Knowledge accumulation through oxidation - how patterns form, evolve, and persist across projects and personas.

---

## Knowledge Separation

Patina distinguishes two types of knowledge:

| Type | Location | Shared? | Contains |
|------|----------|---------|----------|
| **Project** | `layer/` + `.patina/` | Yes (layer/ in git) | Facts, beliefs, and specs about this codebase |
| **Persona** | `~/.patina/layer/` | No (personal) | Cross-project beliefs and preferences |

**Project knowledge:** "Iroh EndpointId is transport identity only" - observation about MCT's peer substrate
**Personal belief:** "I prefer Rust Result<T,E> over exceptions" - your opinion across all projects

Different developers working on the same project share project beliefs but keep separate persona beliefs. Project beliefs live in `layer/surface/epistemic/beliefs/` (git-tracked). Persona beliefs live in `~/.patina/layer/surface/beliefs/` (machine-local).

## Structure

### Project Layers (`layer/`)
- **Core** - base metal, immutable and strong (proven patterns)
- **Surface** - active oxidation (evolving work, evidence, beliefs)
- **Dust** - patina that flaked off (archived wisdom)

### Project Data (`.patina/`)
- **data/** - materialized SQLite + vectors (local, rebuilt from git/sessions)
- **oxidize.yaml** - recipe for local knowledge indexing/projections (git-tracked)

### Personal (`~/.patina/`)
- **layer/** - user-level beliefs and preferences (mirror of project layer structure)
  - `layer/surface/beliefs/` - persona beliefs (machine-local, never shared)
- **personas/** - legacy event log (being migrated to layer/beliefs)
- **registry.yaml** - registered projects and repos on this machine
- **cache/repos/** - cloned reference repositories

## System

- **User** - Oxidizer (adds the oxygen of creativity and vision)
- **LLMs** - Smith (reads project + persona knowledge via Patina discovery tools when needed)
- **Sessions** - Chemical Reactions (capture observations → events)
- **Git** - Time (threads that weave together, syncs project knowledge)
- **Containers** - Isolation (controlled storage to hold/test/replicate)

## Data Flow

```
Sources                    Pipeline                      Storage
─────────────────────────────────────────────────────────────────────
.git/                  ┐
layer/allium/*.allium  │
layer/slate/work/*     ├→ scrape → patina.db (eventlog + views)
layer/sessions/*.md    │
layer/surface/**/*     │
src/**/*               ┘              │
                                      ↓
                                   oxidize
                                      │
                                      ↓
                       ┌────────── vectors
                       │
                       ↓
~/.patina/layer/   ──→ discovery tools ←── .patina/data/
                       │
                       ↓
              [PROJECT] + [PERSONA] results → LLM context
```

## Layer Management

### Promotion Path (Project Patterns)
- Surface (new) → Core (proven via repeated success)
- Surface (new) → Dust (failed or deprecated)

### Storage
- **Core**: `layer/core/*.md` - Version controlled, immutable patterns
- **Surface**: `layer/surface/*.md` and `layer/surface/epistemic/beliefs/*.md` - Active evidence, beliefs, and product notes
- **Dust**: `layer/dust/*.md` - Historical reference, searchable

## Integration Points

### Session → Events
- Session markdown → scrape sessions → observations table
- Git commits → scrape git → commits + co_changes tables
- Code AST → scrape code → functions + call_graph tables

### Events → Vectors
- oxidize.yaml recipe defines projections
- SQLite tables provide training pairs
- Each user builds vectors locally from shared recipe

### Discovery → LLM
- Queries may search project vectors and persona beliefs when available
- Results are treated as evidence with provenance, not as code authority
- MCT runtime code must not depend on Belief/scry/assay/oxidize internals

### Persona (Personal Only)
- `patina persona note "belief"` writes to `~/.patina/layer/surface/beliefs/` and legacy event log
- `patina persona query "topic"` searches persona knowledge
- `patina persona migrate` converts legacy events to belief files (idempotent)
- Never synced via git - machine-local only
- Cross-project: same belief applies to all your work

## Pattern Lifecycle

### Pattern Recognition (Project)
- Git diff + Session context → scrape → Events → Pattern extraction

### Pattern Validation (Project)
- Used in ≥3 successful contexts → Core candidate
- Failed in any context → Dust candidate
- Explicitly deprecated → Move to dust

### Belief Evolution (Persona)
- Personal beliefs accumulate over time
- Not subject to project validation
- Inform how you approach all projects

## System Properties

- **Isolation**: Project knowledge stays in project, persona stays personal
- **Reproducibility**: Same recipe + events = equivalent vectors
- **Traceability**: Git history links to session context
- **Discoverability**: Scry searches project + persona together
- **Evolution**: Project patterns move between layers; persona beliefs persist

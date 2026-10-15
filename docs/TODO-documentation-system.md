# TODO: MCT Documentation and Open Trace System

**Status:** Active. Phase 3 public trace pilot implemented and verified; authored audience journeys and the incremental historical import remain.

## Objective

Build a lightweight, agent-maintained documentation system that fully explains MCT to newcomers, operators, Child developers, integrators, contributors, and agents.

The durable architecture is:

> Markdown and Rust source are authoritative; mdBook and rustdoc are renderers; CSS and Datastar are replaceable presentation layers.

Deployment layout:

```text
/docs/  # mdBook product documentation
/api/   # rustdoc API documentation
```

The initial deployment remains static. A small Rust service may later provide search or agent-assisted features through Datastar without changing the documentation source.

## Governing boundaries

- [x] Keep authored Markdown and Rust source usable without Datastar or JavaScript.
- [x] Use mdBook for human-facing product documentation.
- [x] Use rustdoc for crate and public API documentation.
- [ ] Generate CLI, configuration, protocol, and schema references from implementation sources where possible.
- [x] Keep `layer/` as the internal specification, evidence, belief, and design-history surface.
- [x] Treat CSS and Datastar as removable presentation enhancements.
- [x] Add no heavy client-side web framework or hosted documentation dependency.
- [x] Keep agent traces as evidence and source material, not product authority.

## Proposed authored documentation map

- [ ] Introduction and product evaluation
- [ ] Getting started
- [ ] Core Mother/Child/Toy concepts
- [ ] Tutorials
- [ ] Task-oriented how-to guides
- [ ] Operations and troubleshooting
- [ ] Child/plugin development
- [ ] Integration guides
- [ ] Protocol and configuration reference
- [ ] Architecture explanations
- [ ] Contributor documentation
- [ ] Provenance, sessions, decisions, and event history

Proposed source structure:

```text
docs/
├── book.toml
├── src/
│   ├── SUMMARY.md
│   ├── introduction.md
│   ├── getting-started/
│   ├── concepts/
│   ├── tutorials/
│   ├── how-to/
│   ├── operations/
│   ├── child-development/
│   ├── reference/
│   ├── architecture/
│   ├── contributing/
│   └── provenance/
└── generated/
```

The final layout must account for the existing `docs/allium-training.md` before moving or replacing anything.

## Open trace contract

Design the trace system as a public evidence ledger.

- [x] Publish completed product-session traces by default.
- [x] Preserve every event actually persisted by the runtime.
- [x] Do not invent missing reasoning or activity.
- [ ] Preserve user messages, assistant responses, persisted thinking blocks, tool calls, tool results, compactions, branches, model changes, usage metadata, timestamps, file activity, Patina events, Git state, commits, and extracted attachments. The v1 importer fails closed on attachment-bearing sessions until extraction is implemented.
- [x] Preserve original trace bytes immutably.
- [x] Compress archived JSONL with Zstandard.
- [x] Record exact byte size and BLAKE3 digest.
- [x] Never manually edit archived traces.
- [x] Never silently truncate a rendered transcript.
- [x] Provide a public download of the exact compressed trace from its documentation page.

Proposed archive structure:

```text
traces/
├── manifest.jsonl
├── sessions/
│   └── <session-id>.jsonl.zst
└── attachments/
    └── <blake3>
```

Current feasibility evidence:

- Existing MCT Pi corpus: 31 sessions and approximately 168 MiB uncompressed.
- Largest current trace: approximately 32.7 MiB.
- Test compression reduced that trace to approximately 4.8 MiB, an 86% reduction.
- Keeping the initial compressed archive in the public repository appears practical; establish a reviewed growth threshold before choosing external storage.

## Stable trace identity

- [x] Define logical trace identifiers independent of physical storage paths.
- [x] Preserve Pi entry IDs and parent relationships in the normalization contract.
- [x] Make citations stable if archive storage moves later.

Proposed form:

```text
trace:pi:<session-id>#<entry-id>
```

Authored documentation should be able to cite traces and commits explicitly:

```yaml
sources:
  - trace:pi:<session-id>#<entry-id>
  - commit:<git-sha>
  - session:<patina-session-id>
```

## Trace normalization

- [x] Define one stable normalized event schema.
- [x] Preserve original source format and source IDs in every normalized event.
- [x] Implement Pi JSONL v3 normalization first.
- [ ] Incorporate `layer/sessions/*.md`, `layer/events.jsonl`, and Git history without replacing their original forms.
- [x] Keep the normalization layer independent of Pi so future agent runtimes can be added.
- [x] Record normalizer version and source digest in every generated projection.

Conceptual model:

```text
Trace
├── session metadata
├── participants and models
├── ordered events
├── branches
├── tool activity
├── files touched
├── commits produced
├── source citations
└── raw-trace receipt
```

## Generated trace projections

Generate multiple views from one immutable trace:

- [x] Complete transcript
- [x] Session overview
- [ ] Session narrative
- [ ] Chronological timeline
- [ ] Branch and compaction history
- [ ] Decision index
- [ ] File activity history
- [ ] Commit provenance
- [ ] Agent handoff context
- [ ] Tutorial source candidates
- [ ] FAQ and troubleshooting candidates
- [ ] Release narrative candidates

Rules:

- [x] Deterministic projections are generated and never manually edited.
- [ ] Large transcripts are split into navigable chunks without omission.
- [ ] LLM-synthesized guides become reviewed, committed Markdown.
- [ ] Every synthesized page records the trace ranges, commits, generator/prompt recipe, and model used.
- [ ] A derived page must link back to its exact source events.
- [ ] A trace page must link forward to every document derived from it.

## Static mdBook integration

- [x] Generate trace-backed provenance pages during the mdBook build.
- [x] Keep the book readable without client-side JavaScript.
- [ ] Add session, decision, commit, file, model, and event indexes.
- [x] Include exact trace receipts and raw downloads.
- [x] Ensure generated trace material does not become an independent authority source.
- [x] Stage generated trace Markdown only during builds; do not duplicate the full trace corpus in Git without a demonstrated need.

Proposed book section:

```text
Provenance
├── Sessions
├── Decisions
├── Commits
├── Files
├── Models and agents
└── Complete event log
```

## Datastar enhancement layer

Datastar is optional progressive enhancement, not a renderer or source of truth.

Potential later features:

- [ ] Filter events by actor, model, tool, file, or commit.
- [ ] Navigate session branches and compactions.
- [ ] Expand and collapse tool activity.
- [ ] Follow a documentation claim to its source event.
- [ ] Show all documents derived from a trace.
- [ ] Search the complete event corpus.
- [ ] Switch among transcript, timeline, and raw-event views.
- [ ] Add interactive Mother/Child/Toy diagrams and protocol demonstrations.
- [ ] Add agent-assisted questions through a small Rust backend only when a concrete requirement justifies it.

## Open publication safety

Transparency is the default. Safety checks must prevent accidental credential publication without creating silent or misleading traces.

- [x] Implement scanning for tokens, private keys, authorization headers, credentials, and equivalent secrets.
- [x] Instruct agents not to print secrets into trace-visible output.
- [x] Do not silently redact or rewrite raw evidence.
- [x] If removal is unavoidable, publish an explicit redaction event with category and reason.
- [x] Mark the entire session visibly as redacted.
- [x] Never describe a redacted trace as complete.
- [x] Define attachment handling, including images and binary tool results.
- [x] Define treatment of third-party or non-public source material before importing traces from other projects.

## Agent maintenance contract

- [x] Require documentation impact assessment for behavior changes.
- [x] Check mdBook builds in CI.
- [x] Check rustdoc and doctests in CI.
- [x] Check internal links in CI; add external-link checking when deployment policy is set.
- [ ] Detect stale generated CLI/configuration/protocol references.
- [x] Verify trace digests and manifests.
- [x] Verify generated projections against their source traces.
- [x] Prevent manual edits to generated files.
- [x] Require source citations for agent-generated explanatory content.
- [x] Keep documentation updates reviewable and separate from unrelated product changes.

## Implementation sequence

### Phase 1: contract only

- [x] Ratify authority, evidence, and presentation boundaries after reviewing the implemented contract.
- [x] Ratify public-by-default trace policy after reviewing the implemented contract.
- [x] Ratify stable trace identifier format after reviewing the implemented contract.
- [x] Ratify archive, manifest, normalization, and redaction contracts after reviewing the schemas.
- [x] Ratify authored documentation information architecture after reviewing the initial book.
- [x] Keep `docs/allium-training.md` as a compatibility pointer and maintain its content in `docs/src/contributing/allium-training.md`.

### Phase 2: minimal static documentation

- [x] Add mdBook configuration and initial `SUMMARY.md`.
- [x] Establish `/docs/` static build output.
- [x] Establish `/api/` rustdoc build output.
- [x] Add minimal CSS without Datastar.
- [x] Add build and internal-link checks.

### Phase 3: trace archive

- [x] Implement trace ingestion as a small Rust tool or workspace `xtask`.
- [x] Import one reviewed Pi session as a fixture/proof.
- [x] Produce the compressed raw trace, manifest entry, and BLAKE3 receipt.
- [x] Generate a complete static transcript and session overview.
- [x] Verify no event was omitted.

### Phase 4: complete historical import

- [ ] Import existing MCT Pi sessions.
- [ ] Link Patina session artifacts, Patina events, and Git commits.
- [ ] Generate provenance indexes.
- [ ] Review corpus size and ratify the in-repository growth threshold.

### Phase 5: derived content

- [ ] Define reproducible content recipes.
- [ ] Generate initial tutorials, explanations, FAQ entries, and release narratives from cited traces.
- [ ] Keep synthesized outputs reviewed and committed as ordinary Markdown.

### Phase 6: optional Datastar

- [ ] Add Datastar only for an approved interaction that static HTML cannot satisfy well.
- [ ] Confirm the complete book remains usable with Datastar removed or disabled.

## Decisions requiring operator review

- [x] Confirm complete persisted thinking blocks are public by default.
- [x] Confirm raw compressed traces initially live in this repository.
- [x] Confirm the proposed 512 MiB aggregate / 50 MiB single-object archive review thresholds.
- [x] Confirm the operator-approved exceptional redaction process.
- [x] Confirm the proposed session-close publication gate.
- [x] Confirm the `trace:<runtime>:<session-id>#<entry-id>` identifier syntax.
- [x] Confirm mdBook as the product documentation renderer and rustdoc as the API renderer.
- [x] Confirm Datastar remains optional and replaceable.

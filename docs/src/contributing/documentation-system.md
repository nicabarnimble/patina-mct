# Documentation system

MCT documentation uses ordinary Markdown and Rust source. mdBook and rustdoc
render static HTML; presentation code never owns product knowledge.

## Authority and evidence

The documentation system preserves these boundaries:

1. Allium specifications define product semantics.
2. Rust source defines implementation and public API behavior.
3. Authored files under `docs/src/` explain supported product behavior.
4. Generated references project implementation-owned CLI, configuration,
   protocol, and schema surfaces.
5. Agent traces, Patina sessions, events, and Git history provide evidence.

Generated trace narratives cannot override authored documentation or code.
When sources disagree, contributors resolve the disagreement at its authority
source rather than editing a projection until it looks consistent.

## Renderers

The deployed static layout is:

```text
/docs/  mdBook product documentation
/api/   rustdoc workspace API documentation
```

Run both builds from the repository root:

```bash
./scripts/install-docs-tools.sh  # once per development environment
./scripts/build-docs.sh
```

Build output is staged under `target/site/` and is not committed.

## Content organization

- Tutorials teach through a complete learning journey.
- How-to guides solve a focused task.
- Reference describes exact surfaces.
- Explanation and architecture clarify reasons and relationships.
- Provenance exposes the evidence and activity behind all four.

This follows Diátaxis without requiring a framework-specific content format.
The source remains readable in a terminal, Git forge, or any future renderer.

## Generated content

Deterministic output is rebuilt from authoritative inputs and is never edited
by hand. LLM-synthesized explanations are different: they become reviewed,
committed Markdown and retain citations to the exact source trace ranges,
commits, recipe, and model.

The full trace archive is not copied into authored Markdown. Build tooling
produces navigable static projections and links each projection to the exact
compressed source.

The `mct-trace` workspace tool owns trace ingestion and disposable projection:

```bash
cargo run -p mct-trace -- verify --repo-root .
cargo run -p mct-trace -- ingest --source <completed-pi-session.jsonl> --repo-root .
./scripts/build-docs.sh
```

Ingestion scans before publication, preserves exact source bytes, writes one
append-only receipt, and is idempotent when the same reviewed source is
presented again. The documentation build verifies all receipts and generates
session indexes, complete transcripts, normalized JSONL, and raw downloads in
the staged `target/docs-book/` tree. Those generated files are never edited or
committed as authored documentation.

## Presentation

The initial theme is a small CSS layer over mdBook defaults. Datastar may later
add progressive enhancement for trace filtering, branch navigation, evidence
backlinks, or interactive diagrams. Every page must remain complete and usable
without Datastar or JavaScript.

## Maintenance requirements

A behavior change must assess effects on:

- product explanation;
- tutorials and how-to procedures;
- generated CLI/configuration/protocol reference;
- rustdoc and doctests;
- trace and provenance links.

CI builds mdBook, checks links, and builds rustdoc with warnings denied. Agents
must keep unrelated documentation changes separate enough to review.

# Documentation system

MCT documentation uses ordinary Markdown and Rust source. mdBook and rustdoc
render static HTML; presentation code never owns product knowledge.

## Authority and evidence

The documentation system preserves these boundaries:

1. Settled decisions and active ratified slice specifications define intended
   product behavior.
2. Rust source and executable tests establish implementation behavior and
   rustdoc defines public APIs.
3. Authored files under `docs/src/` explain supported product behavior.
4. Generated references project implementation-owned CLI, configuration,
   protocol, and schema surfaces.
5. Written session records, project events, Git history, and frozen Allium maps
   provide historical evidence.

Historical summaries cannot override settled decisions or establish untested
implementation claims.
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
committed Markdown and cite the decisions, evidence, and commits that support
them. A summary is not an original conversation or proof of owner approval.

The documentation build renders authored product Markdown and Rust APIs. It
does not import, verify, or publish coding-agent conversations.

## Presentation

The theme is a small CSS layer over mdBook defaults. Every page must remain
complete and usable without JavaScript.

## Maintenance requirements

A behavior change must assess effects on:

- product explanation;
- tutorials and how-to procedures;
- generated CLI/configuration/protocol reference;
- rustdoc and doctests;
- decision and evidence links.

CI builds mdBook, checks links, and builds rustdoc with warnings denied. Agents
must keep unrelated documentation changes separate enough to review.

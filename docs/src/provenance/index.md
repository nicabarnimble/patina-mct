# Provenance

MCT keeps its development rationale in ordinary repository records:

- `docs/DECISIONS.md` records settled choices, their status, and reasons.
- `layer/surface/build/feat/` contains implementation scope and test evidence.
- `layer/sessions/` contains written session summaries.
- Git commits and pull requests connect those records to code changes.

These records help explain what was built and why. Session summaries are
interpretations, not verbatim conversations or independent proof that a user
approved a proposal. Tests establish implemented behavior; historical notes do
not override current decisions.

## Retired conversation archive

The August 2026 conversation-archive pilot imported one short tmux conversation.
It demonstrated ingestion, integrity checks, compression, and rendering, but was
not adopted for ongoing development history. The owner chose to remove the
unused tool, pilot archive, and documentation-build integration rather than
continue maintaining them. There is no automatic conversation publication.

The complete former implementation and pilot remain in Git at `f315175`:
`crates/mct-trace/`, `traces/`, and the former open-trace documentation. Current
product and API documentation, decision records, session summaries, and runtime
observation-ledger functionality are retained.

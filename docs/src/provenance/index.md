# Provenance

MCT is designed to publish the evidence of how it was built. Completed agent
sessions, Patina session artifacts, project events, and Git history form a
public evidence ledger from which multiple documentation views can be derived.

Provenance is not implementation authority. It answers questions such as:

- Which request led to this decision?
- Which tools and files were involved?
- What evidence was reviewed?
- Which commit landed the result?
- Which tutorial, explanation, or release note was derived from the session?

The [Open trace contract](open-trace-contract.md) governs the archive. The
[Published sessions](sessions.md) index is generated during each documentation
build from reviewed receipts in `traces/manifest.jsonl`. Each session links to
its complete transcript, normalized projection, exact receipt, and compressed
raw download.

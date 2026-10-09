# MCT public trace archive

This directory is the append-only source archive for completed MCT agent
sessions. The governing publication, identity, normalization, attachment, and
redaction rules are in
[`docs/src/provenance/open-trace-contract.md`](../docs/src/provenance/open-trace-contract.md).

```text
manifest.jsonl     append-only archive receipts
sessions/          exact JSONL byte streams compressed with Zstandard
attachments/       extracted content-addressed attachments
schema/            stable projection and receipt schemas
```

The first reviewed Pi fixture is recorded in `manifest.jsonl`. It proves
fail-closed safety scanning, Pi v3 parsing, exact-byte BLAKE3 receipts,
reproducible Zstandard compression, runtime-independent normalization,
complete event projection, raw download staging, and repository verification.

Use the workspace tool from the repository root:

```bash
cargo run -p mct-trace -- verify --repo-root .
cargo run -p mct-trace -- ingest --source <completed-pi-session.jsonl> --repo-root .
```

Ingestion is idempotent for an identical receipt and refuses conflicting
records, malformed parent chains, detected credentials, and attachment-bearing
sessions. Attachments fail closed until extraction and content-addressed
publication are implemented.

## Pilot review

`trace:pi:019f2920-613a-75c5-9715-cf555c9b7c6e` is a completed nine-event
session from this public repository. It was selected because it contains user,
assistant, persisted thinking, tool-call, tool-result, model, and thinking-level
records in a small reviewable fixture. The publication scan found no credential
material, the source contains no attachments, and no redaction was requested or
performed. The receipt records 9,293 exact source bytes and a 4,190-byte
Zstandard archive.

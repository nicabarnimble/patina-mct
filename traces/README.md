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

No session has been imported yet. The first import must be a reviewed fixture
that proves parse, digest, compression, normalization, completeness, and book
projection behavior before the historical corpus is added.

# Open trace contract

MCT agent activity is public evidence and source material. Completed
product-session traces are published by default, including persisted model
thinking and tool activity.

## Meaning of complete

A complete trace contains every event the runtime persisted:

- user messages and assistant responses;
- persisted thinking blocks;
- tool calls, arguments, results, and errors;
- model and thinking-level changes;
- compactions and branch relationships;
- timestamps and available usage metadata;
- attachments embedded by the runtime.

Project projections also connect the runtime trace to Patina session records,
`layer/events.jsonl`, file activity, Git state, and commits. The archive does
not invent reasoning or events that the runtime did not record.

## Identity

A trace has a logical identifier independent of storage:

```text
trace:<runtime>:<session-id>
```

A citation selects a source entry:

```text
trace:<runtime>:<session-id>#<entry-id>
```

For a source record without an entry ID, normalization assigns
`event-<zero-padded-sequence>` deterministically. IDs never change when files
move or archive storage changes.

Pi's first supported trace format is `pi-session/v3`. Pi entry IDs and parent
entry IDs are preserved exactly.

## Archive

The original JSONL byte stream is immutable. At session close, publication
tooling:

1. verifies the trace can be parsed;
2. scans it for credentials and restricted material;
3. calculates its uncompressed BLAKE3 digest and size;
4. compresses it with Zstandard without modifying the source bytes;
5. calculates the archive digest and size;
6. appends one manifest entry;
7. generates deterministic normalized events and book projections.

The public archive begins in this repository:

```text
traces/
├── manifest.jsonl
├── sessions/<session-id>.jsonl.zst
├── attachments/<blake3>
└── schema/
```

Storage receives an operator review when compressed trace objects exceed 512
MiB in aggregate or one compressed trace exceeds 50 MiB. A move may change
physical URLs but not trace IDs or receipts.

## Manifest receipt

Each trace manifest entry records at least:

- manifest and source-format versions;
- logical trace and runtime session IDs;
- source and archive paths;
- source and archive BLAKE3 digests and byte sizes;
- event count and time bounds;
- publication status;
- redaction disclosures;
- attachment receipts;
- normalizer version.

The manifest is append-only. Corrections append a superseding record rather
than rewriting historical receipts.

## Normalized events

Normalization provides one runtime-independent envelope while preserving the
source record in each event. The stable envelope carries:

- logical trace and event IDs;
- source sequence, format, entry ID, and parent entry ID;
- occurrence timestamp when available;
- actor kind and model identity when available;
- event kind;
- the complete source payload;
- any explicit redaction disclosure.

The raw archive remains fidelity authority. Normalized events are disposable,
versioned projections.

## Publication and exceptional redaction

Publication occurs at session close rather than streaming live so checks can
run before irreversible disclosure. Historical sessions may be imported
through the same gate.

Agents must not print credentials into trace-visible output. Automated scans
look for private keys, authorization headers, tokens, and equivalent secrets.
They are safety checks, not a private-by-default policy.

There is no silent redaction. If legal, security, or third-party restrictions
make removal unavoidable:

1. an operator approves the exception;
2. the published stream receives an explicit redaction event with category and
   reason;
3. the manifest records `redacted` status;
4. every generated view displays that status;
5. the trace is never described as complete.

Secret material itself is not hashed into a public disclosure when that hash
would aid guessing. Traces from non-public repositories or third parties are
not automatically eligible for publication.

## Attachments

Embedded images and binary tool results remain represented in the exact raw
archive. Projections extract them by BLAKE3 identity, retain media type and
size, and link them to the source event. Attachments pass the same publication
and restriction checks as text.

## Projections and derived content

One trace may produce a full transcript, timeline, session narrative, branch
view, decision index, file history, commit provenance, handoff, tutorials,
FAQ entries, troubleshooting guides, and release narratives.

Deterministic projections are generated and never manually edited. Large
transcripts may be split into chunks, but no event is omitted. Each view links
to its exact source events and raw archive receipt.

LLM-synthesized content becomes reviewed Markdown. It records source ranges,
commits, generation recipe, and model. A trace links forward to all known
derivatives, and derived content links back to its evidence.

## Datastar boundary

Static HTML provides the complete content. Datastar may later filter events,
navigate branches, expand tool calls, follow evidence links, and search the
corpus. Removing Datastar must not remove evidence or make the book unusable.

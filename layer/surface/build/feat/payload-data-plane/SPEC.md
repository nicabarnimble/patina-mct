# Payload data plane

## Contract

`mct/call/0` carries bounded inline payload bytes in both directions. The peer protocol transfers the bytes, adapters verify the declared integrity facts before authority evaluation, resident execution delivers verified bytes to the child, and replies return a digest-stamped result payload. Observations and the JSONL ledger record size, digest, classification, and typed outcomes only; payload bytes are never observation data.

## Wire encoding

Slice 1 uses base64 fields inside the existing JSON request/reply envelope rather than a length-prefixed binary section. This keeps the `mct/call/0` edge debuggable and lets the existing JSON encode/decode tests evolve directly; the ~33% base64 overhead is acceptable under a deliberately small inline cap.

The protocol shape is a 0.x wire break:

- request: existing `MctCallProtocolRequest` plus `inline_payload_base64: Option<String>` when `payload` is `InlinePayload`;
- reply: existing `MctCallProtocolReply` plus `result_payload: MctCallPayloadHandle` and `inline_result_payload_base64: Option<String>` when the result payload is inline.

## Caps and named budgets

Named constants live at the transport boundary:

- `MCT_INLINE_PAYLOAD_MAX_BYTES = 32 * 1024` for request inline payload bytes;
- `MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES = 32 * 1024` for reply inline payload bytes;
- `MCT_CALL_FRAME_READ_BUDGET_BYTES = 96 * 1024` for request and reply `read_to_end` budgets.

The current 64 KiB total read budget becomes 96 KiB. That budget covers the JSON envelope, base64 expansion of one 32 KiB payload, and slack for authority/trace/result fields without allowing unbounded buffering.

## Payload handles and integrity facts

`MctCallPayloadHandle::InlinePayload` gains a `blake3_digest_hex` field and renames its byte count to `size_bytes` because inline integrity validation is exact, not approximate. `ContentAddressedBlob` keeps its digest field and also uses `size_bytes` because the optional D6 local-CAS ingest path verifies exact digest and size before a blob handle is consumable. `ExternalReference` keeps `approximate_size_bytes` because MCT does not dereference it in this phase; it is not accepted for inline byte delivery.

Updated variant and metadata shapes:

```rust
PayloadMetadata {
    data_classification: String,
    size_bytes: u64,
    contains_secret_scoped_material: bool,
}

InlinePayload {
    inline_payload_ref: String,
    content_type: String,
    size_bytes: u64,
    blake3_digest_hex: String,
}
ContentAddressedBlob {
    digest: String,
    blob_ref: String,
    content_type: String,
    size_bytes: u64,
}
ExternalReference {
    external_ref: String,
    content_type: Option<String>,
    approximate_size_bytes: u64,
}
Empty
```

Hashing remains adapter-side. The kernel receives declared handle facts plus observed size/digest facts and returns a typed integrity decision: match, declared-too-large, actual-too-large, size mismatch, digest mismatch, missing inline bytes, unexpected inline bytes, or invalid digest syntax.

## Delivery mapping

For WIT children, a verified request payload with `content_type = "application/json"` is parsed as the call argument JSON and lowered through the existing `wit_values` path. The child-kind/content-type check happens at resident delivery preflight after child authorization identifies the runtime kind and before effect execution. A non-JSON payload for a WIT child fails closed with typed reason `ChildPayloadContentTypeUnsupported`, outcome `Failed`, and caller-safe text `unsupported child payload`; it is not a malformed protocol outcome because the protocol bytes and integrity facts were valid.

The lifted WIT result JSON is serialized to bytes, checked against `MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES`, hashed, and returned as the inline result payload. If the serialized result exceeds the cap, the call fails closed with typed execution-failure reason `ResultPayloadTooLarge`, outcome `Failed`, and caller-safe text `result payload too large`. Silent truncation is forbidden.

For process children, the verified request payload bytes are written verbatim to stdin. Stdout bytes are the result payload; they are checked against `MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES`, hashed, and returned inline. If stdout exceeds the result cap, the call fails closed with typed execution-failure reason `ResultPayloadTooLarge`, outcome `Failed`, and caller-safe text `result payload too large`. Silent truncation is forbidden. Stderr remains adapter diagnostics only and never becomes a caller payload.

`MctResult` gains a result payload handle mirroring the reply handle so local execution and remote replies describe the same bytes. Execution summaries continue to record input/output byte counts.

## Validation order and outcomes

The order is fixed:

1. bounded transport read (`MCT_CALL_FRAME_READ_BUDGET_BYTES`);
2. JSON decode;
3. inline base64 decode when present;
4. declared handle validation and declared cap check;
5. actual cap check;
6. adapter computes blake3 digest and observed size;
7. kernel integrity decision compares declared and observed facts;
8. existing hello/call authority evaluation;
9. resident child authorization;
10. resident delivery preflight, including child-kind/content-type compatibility;
11. effect execution;
12. result payload serialization/capture, result cap check, hashing, and reply-handle construction.

Failures in steps 1-7 are malformed protocol outcomes and never execute. They map to `CallProtocolOutcome::Malformed` with typed reasons `MalformedCall`, `PayloadMetadataMismatch`, `PayloadDeclaredTooLarge`, `PayloadActualTooLarge`, `PayloadSizeMismatch`, `PayloadDigestMismatch`, `PayloadMissingInlineBytes`, or `PayloadUnexpectedInlineBytes`; caller-safe text is `malformed call payload`. Authority failures after integrity keep the existing denied outcomes and safe `not authorized` projection.

Delivery preflight failures in step 10 happen after a child kind is known but before child code runs. The slice-1 WIT content-type failure maps to typed reason `ChildPayloadContentTypeUnsupported`, `CallProtocolOutcome::Failed` / `ResultOutcome::Failed`, and caller-safe text `unsupported child payload`.

Oversized result payload failures in step 12 happen after authority and child execution, so they are execution failures, not malformed protocol outcomes. They map to typed reason `ResultPayloadTooLarge`, `CallProtocolOutcome::Failed` / `ResultOutcome::Failed`, and caller-safe text `result payload too large`. Result bytes are never truncated to fit the cap.

## Caller-side result verification

The caller verifies replies symmetrically inside `MCT_CALL_FRAME_READ_BUDGET_BYTES`: bounded response read, JSON decode, inline result base64 decode when present, result handle validation, exact size check, cap check, adapter-computed blake3 digest, and digest comparison. A result size or digest mismatch is a typed client-side error `ResultPayloadIntegrityMismatch`; an oversized inline result or oversized response frame is a typed client-side error `ResultPayloadTooLarge`. The caller never silently accepts mismatched result bytes, and D3 tests must cover reply-side digest mismatch and oversized reply handling.

## Observability invariant

Adapter observations for malformed request payloads, delivery failures, and result-payload failures include call id when available, size, digest, classification, and the typed reason in references/details. They never include base64 or raw request/result payload bytes. The no-bytes invariant is tested against the JSONL ledger for both request and result payloads.

## Slice 2: local content-addressed blob store

Slice 2 adds a local-only content-addressed store under the daemon state directory. The store is an adapter concern: it performs file I/O, bounded reads, atomic writes, and hashing; the kernel continues to decide from declared versus observed size/digest facts through `evaluate_payload_integrity`.

### Store invariant and layout

A blob visible in the CAS is valid by construction. Ingest decodes the request body, rejects oversized input before reading past the cap, writes bytes to a private temporary path, verifies exact byte size and BLAKE3 digest before visibility, then atomically renames into the digest-keyed final path. A failed ingest removes the temp path and leaves no digest-visible blob.

The store root is the parent directory of the configured state database plus `blobs/`, so the default layout is:

```text
.mct/
  state.sqlite
  blobs/
    tmp/
      ingest-<unique>.tmp
    blake3/
      <first-two-hex>/<64-char-blake3-hex>.blob
```

The two-character fanout keeps a single directory from growing without introducing a database index. The final filename is keyed only by lowercase BLAKE3 hex; `blob_ref` may repeat the digest-qualified local path label, but authority and integrity use the digest and `size_bytes` facts, not path trust.

### Blob cap

`MCT_BLOB_MAX_BYTES = 8 * 1024 * 1024`. This intentionally exceeds the 32 KiB inline wire cap so real local workloads can avoid inline envelopes, while still bounding memory and disk reads to a small single-digit MiB budget.

### Ingest surface

The ingest surface is a local-only control UDS command: `POST /blobs` with JSON `{ "digest": "<blake3-hex>", "size_bytes": <u64>, "content_type": "...", "bytes_base64": "..." }`. UDS keeps the surface off the network, reuses the existing local-control operational path, and is sufficient for node-local tooling to stage payloads before a local call. HTTP control remains read-only for this slice.

Successful ingest returns a `ContentAddressedBlob` handle. Digest mismatch, size mismatch, invalid digest syntax, invalid base64, and oversize are typed failures; no blob becomes visible.

### Local consumption

`ContentAddressedBlob` request payloads become consumable for local calls only. Before authority evaluation for a local call, the daemon adapter fetches the declared digest from the local CAS, bounded by `MCT_BLOB_MAX_BYTES`, hashes the bytes, records observed size/digest facts, and asks the kernel to compare those facts with the declared handle through the existing payload-integrity path.

An absent declared blob fails closed before authority with typed reason `PayloadBlobUnavailable`, outcome classification `Malformed`, and caller-safe text `payload blob unavailable`. If a visible blob is tampered with after ingest, the fetch still hashes what is on disk and the existing digest-mismatch decision fails closed with `PayloadDigestMismatch` / `malformed call payload`; wrong bytes are never delivered.

Remote `mct/call/0` continues to carry inline bytes only. A remote request declaring `ContentAddressedBlob` is not dereferenced over Iroh in this phase and behaves as it does after slice 1: the handle is validated as a declaration but no remote blob transfer occurs. Iroh blob transfer between Mothers is the ROADMAP follow-on after local CAS.

### Slice 2 result scope

Results stay inline-only. There are no CAS result handles, no result blob ingest, and no remote blob reply transfer in this phase.

### Slice 2 observability

CAS ingest, fetch, and local consumption observations record digest, size, classification/content type, and typed outcome only. They never record raw blob bytes or base64-encoded blob bytes. Ledger no-byte tests cover ingest/fetch/consumption paths using both raw and standard-base64 assertions.

### Slice 2 non-goals

- No Iroh blob transfer.
- No CAS result handles.
- No garbage collection or eviction.
- No compression.
- No cross-Mother fetch.
- No policy-based per-grant CAS size limits.

## Non-goals

- No Iroh blob transfer.
- No policy-based per-grant size limits; future work may add grant-scoped caps after the fixed caps prove useful.
- No wasip3 streams.
- No compression.
- No content-addressed blob fetch across Mothers in slice 1.

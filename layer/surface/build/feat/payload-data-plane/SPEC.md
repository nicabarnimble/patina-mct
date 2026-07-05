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

Updated variant shapes:

```rust
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

## Non-goals

- No Iroh blob transfer.
- No policy-based per-grant size limits; future work may add grant-scoped caps after the fixed caps prove useful.
- No wasip3 streams.
- No compression.
- No content-addressed blob fetch across Mothers in slice 1.

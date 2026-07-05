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

`MctCallPayloadHandle::InlinePayload` gains a `blake3_digest_hex` field. `ContentAddressedBlob` keeps its digest field and remains a declared handle only until slice 2 supplies local storage. `ExternalReference` remains size-only because MCT does not dereference it in this phase; it is not accepted for inline byte delivery.

Updated variant shapes:

```rust
InlinePayload {
    inline_payload_ref: String,
    content_type: String,
    approximate_size_bytes: u64,
    blake3_digest_hex: String,
}
ContentAddressedBlob {
    digest: String,
    blob_ref: String,
    content_type: String,
    approximate_size_bytes: u64,
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

For WIT children, a verified request payload with `content_type = "application/json"` is parsed as the call argument JSON and lowered through the existing `wit_values` path. Non-JSON WIT payloads are malformed for slice 1. The lifted WIT result JSON is serialized to bytes, capped by `MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES`, hashed, and returned as the inline result payload.

For process children, the verified request payload bytes are written verbatim to stdin. Stdout bytes are the result payload; they are capped, hashed, and returned inline. Stderr remains adapter diagnostics only and never becomes a caller payload.

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
10. effect execution.

Failures in steps 1-7 are malformed protocol outcomes and never execute. They map to `CallProtocolOutcome::Malformed` with typed reasons `MalformedCall`, `PayloadMetadataMismatch`, `PayloadDeclaredTooLarge`, `PayloadActualTooLarge`, `PayloadSizeMismatch`, `PayloadDigestMismatch`, `PayloadMissingInlineBytes`, or `PayloadUnexpectedInlineBytes`; caller-safe text is `malformed call payload`. Authority failures after integrity keep the existing denied outcomes and safe `not authorized` projection.

## Observability invariant

Adapter observations for malformed payloads include call id when available, size, digest, classification, and the typed reason in references/details. They never include base64 or raw payload bytes. The no-bytes invariant is tested against the JSONL ledger.

## Non-goals

- No Iroh blob transfer.
- No policy-based per-grant size limits; future work may add grant-scoped caps after the fixed caps prove useful.
- No wasip3 streams.
- No compression.
- No content-addressed blob fetch across Mothers in slice 1.

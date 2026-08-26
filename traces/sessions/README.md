# Session objects

Each file is an immutable runtime JSONL byte stream compressed as
`<session-id>.jsonl.zst`. Archive receipts live in `../manifest.jsonl`.

Do not decompress, edit, and recompress these objects manually. `mct-trace`
verifies both the compressed object and its exact decompressed source bytes
against their BLAKE3 receipts.

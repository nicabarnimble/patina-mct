# Design note: checkpointed ledger segments

This is a foundation note, not a ratified change to the observation-store law in
`layer/allium/mct-product-map.allium`. Retention, archival, and export stay
reserved. Any future mutation of the canonical store has to be observation-bearing
before it ships.

## What landed

`mct-observation` can copy a chain-valid log and append one
`StorageAppendSucceeded` observation whose `detail_ref` carries
`mct-ledger-segment-checkpoint-v1:` plus a v1 checkpoint:

- schema `mct-ledger-segment-checkpoint/v1`
- segment id
- entry count of the bytes being sealed
- last sequence and entry hash of that prefix
- BLAKE3 of those prefix bytes

`verify_sealed_segment` checks the hash chain, then checks that the checkpoint
observation binds that prefix. `verify_segmented_ledger` walks
`seg-000000.jsonl`, `seg-000001.jsonl`, … and then `open.jsonl`. The first entry
of each later file must link to the previous file's checkpoint entry hash and
the next sequence.

`JsonlObservationLedger::open` does not look at this layout. Segmented
verification runs only when a caller invokes it. `verify_segmented_ledger_if_enabled`
refuses unless `MCT_LEDGER_SEGMENTS` is `1` or `true`. The live writer ignores
that variable, so a deployment cannot silently switch stores.

The checkpoint is the hash chain plus the prefix digest. It is not signed with
the node key. `mct-observation` does not depend on `mct-iroh`, and adding that
edge would cycle the crates.

## What remains

- Automatic rotation of the live `observations.jsonl`. The sealer only copies.
- An observation-bearing archive event on the canonical log before any prefix
  bytes are moved aside. The product map already requires that mutation to be
  logged. This foundation does not perform it.
- Signing the checkpoint with the Mother node key, or binding an epoch root,
  once a signature can live in the observation crate without a new dependency cycle.
- SQLite columns for a sealed prefix offset. The process-local replay checkpoint
  is still the hot path for a single file.
- Skipping archived prefix bytes on open. A reader can verify from the latest
  sealed checkpoint only after the canonical log itself records that seal.
- Authority replay across segment files. The segmented reader checks the hash
  chain and the checkpoint binding. Folding authority facts across files is
  future work; the single-file writer cache remains the authority path.

Until those pieces exist, operators keep one append-only file. Fsync on each
before-effect append stays in place.

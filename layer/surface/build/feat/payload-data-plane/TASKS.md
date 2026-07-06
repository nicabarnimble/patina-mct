# Payload data plane Phase 5 tasks

- [x] Task D0 — Housekeeping
- [x] Task D1 — SPEC first
- [x] Task D1.1 — SPEC amendments at operator gate
- [x] Task D2 — Kernel: payload integrity decisions
- [x] Task D3 — Transport: bytes over mct/call/0
- [x] Task D4 — Daemon: delivery and result path
- [x] Task D5 — End-to-end proof
- [ ] Task D6 — Slice 2: local content-addressed blob store
  - [x] Task D6a — SPEC amendment: local blob store contract
  - [ ] Task D6b — Implementation: store, local consumption, ingest surface
  - [ ] Task D6c — Phase close-out

---

# MCT Phase 5 — Payload data plane (ROADMAP item 2)

You are starting ROADMAP item 2 in `patina-mct`: calls today carry payload
METADATA (size/classification) and results carry REFS, but no payload
bytes move — echo-shaped calls work, real workloads cannot. This phase
makes `mct/call/0` carry bounded, integrity-verified inline payload bytes
end to end (request AND result), delivered to the executing child.
Content-addressed blob storage is slice 2 within this phase; Iroh blob
transfer and wasip3 streams are explicitly deferred.

## Working principles (binding)

1. Read `AGENTS.md`, `layer/core/dependable-rust.md`,
   `layer/core/what-is-mct.md`, `layer/surface/build/product/ROADMAP.md`,
   and the resident-mother SPEC/TASKS under
   `layer/surface/build/feat/resident-mother/` before touching code.
   Non-negotiable: kernel decides, adapters perform (kernel stays pure —
   it compares declared vs observed facts; adapters do I/O and hashing);
   fail closed; typed decisions; sealed capabilities and stale-revision
   guards remain intact.
2. Favor strong invariants over defensive fallbacks. Do not add
   complexity to paper over unclear design. No speculative abstractions:
   the payload-handle ENUM VARIANTS are the extension point for future
   transports (a p3 stream becomes a new variant later) — do NOT
   introduce a transport trait.
3. Always read code before writing code. Key surfaces to read first:
   `crates/mct-kernel/src/call/mod.rs` (MctCallPayloadHandle enum, the
   cross-record size invariant, JSON edge encode/decode),
   `crates/mct-iroh/src/serve.rs` + endpoint client paths (bounded
   read_to_end budgets, timeouts), the resident execution path in
   `crates/mct-daemon/src/main.rs` (R4 work: how calls reach WIT/process
   children today), and `crates/mct-daemon/src/wit_values.rs` (JSON
   lowering/lifting).
4. Scalpel commits; named-file staging; no attribution/branding; no
   history rewrites. Failing test first for behavior changes. Stop at a
   task boundary if context runs low — the task file on disk is the
   source of truth.
5. Validation green after EVERY commit:
   `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh`
   Flake protocol: capture any failure verbatim in TASKS.md before
   rerunning.

## Hard invariants for this phase

- **Payload bytes NEVER enter the observation ledger.** Observations
  record digest + size + classification only. The ledger is an audit
  spine, not a data store; this is both a privacy and a growth property.
  Add a test that proves ledger entries contain no payload bytes.
- **Integrity before authority, authority before effects.** Wire decode →
  payload integrity (exact size match + digest match against the declared
  handle) → existing authority evaluation → effect. Integrity mismatch is
  a typed Malformed outcome, observed, never executed.
- **Everything bounded, by named constants.** Inline payload cap, total
  frame/read budget (envelope + cap + slack), result payload cap. Reads
  stay bounded; timeouts from Phase 4 stay intact. Oversized declared OR
  actual payloads fail closed before buffering unbounded data.
- **Kernel purity.** Hashing happens adapter-side; the kernel receives
  declared and observed digests/sizes as facts and decides. No I/O, no
  hashing dependencies in mct-kernel public API beyond what exists.

## Task D0 — Housekeeping

a) Verify state: branch `patina`, in sync with origin/patina, HEAD at
   181a98a (post PR #16 merge; origin/main carries merge d6b8da5). Tree
   clean except `brew-noncore-report.html`. If not, STOP and report.
b) In ROADMAP.md: tick the standing-backlog item "Child SDK / packaging
   tooling" with a one-line note (completed in the integrated Patina
   repo: `patina child init/build/package/verify`, canonical WIT world
   `patina:mct@0.1.0` at wit/mct/, oracle-verified 2026-07-04).
c) Save this prompt verbatim as
   `layer/surface/build/feat/payload-data-plane/TASKS.md` with a
   checklist header; commit ROADMAP + TASKS together:
   `docs: start payload data plane phase`.

## Task D1 — SPEC first (gate: operator reads this before D2 proceeds)

Write `layer/surface/build/feat/payload-data-plane/SPEC.md` (short),
deciding explicitly:
- **Wire encoding for inline bytes** on `mct/call/0` request and reply:
  base64 field inside the existing JSON envelope vs a length-prefixed
  binary section after the JSON frame. Weigh simplicity against the
  ~33% base64 overhead under the caps you choose; pick one, justify in
  two sentences.
- **Caps**: inline payload cap, total read budgets both directions,
  and their named constants. Current serve read budget is 64KiB total —
  state the new budgets.
- **Integrity fields**: add a digest to `InlinePayload` (blake3, hex —
  consistent with the ledger's hashing) or justify size-only. State the
  updated `MctCallPayloadHandle` variant shapes and the wire-format
  break (0.x, disclosed).
- **Delivery mapping**: how request payload bytes become child input —
  WIT children: payload IS the call arguments (content_type
  application/json, lowered via the existing wit_values path); process
  children: bytes on stdin. How child output becomes the result payload,
  and how `MctResult`/reply carries it.
- **Validation order** (must match the hard invariant) and which typed
  reasons/outcomes each failure maps to.
- **Non-goals**: no Iroh blob transfer, no policy-based per-grant size
  limits (note as future work), no p3 streams, no compression.
Commit it. This SPEC is the contract for D2–D5.

## Task D2 — Kernel: payload integrity decisions

Extend the payload handle and the pure validation to cover the SPEC's
integrity fields; new typed error/reason variants for size/digest
mismatch; JSON edge encode/decode updated; kernel tests for every
mismatch class. No I/O enters the kernel.

## Task D3 — Transport: bytes over mct/call/0

Carry request and result payload bytes per the SPEC encoding within the
new bounded budgets in mct-iroh (client and serve paths). Integrity
verification happens adapter-side against the declared handle before the
kernel authority evaluation is invoked; failures produce the SPEC's
typed Malformed outcomes with dual-reason disclosure. Tests: roundtrip
with payload; oversized declared; oversized actual; digest mismatch;
budget refusal — all fail closed.

## Task D4 — Daemon: delivery and result path

Resident execution feeds the verified payload to the child (WIT args /
process stdin per SPEC) and returns the child's output as a
size-capped, digest-stamped result payload. Deadline/memory limits and
capability guards unchanged. Observations record digest+size only —
include the no-payload-bytes-in-ledger test here.

## Task D5 — End-to-end proof

Two-Mother integration test: remote call carries a real payload → child
processes it (not echo of a constant — the output must depend on the
payload) → result payload returns → caller verifies content; full trace
reconstructible from the ledger; ledger contains digests but not bytes.
This is the phase's definition of done for slice 1.

## Task D6 — Slice 2: local content-addressed blob store (may stop before)

Minimal local CAS keyed by blake3 digest under the node's state dir:
ingest verifies digest, `ContentAddressedBlob` handles become
consumable for LOCAL calls (store/fetch adapter-side). Iroh blob
transfer between Mothers is explicitly OUT — record it in ROADMAP as
the follow-on. If context is short, stopping after D5 is a clean
boundary; say so in the summary.

## Definition of done

Validation green per commit; hard invariants tested, not just stated;
TASKS.md checked off as you go; final summary: commits, SPEC decisions
made, flake log (or none), D5 transcript, whether D6 landed, and
anything discovered that belongs in ROADMAP rather than this phase.

## Flake log

- 2026-07-05 D3 validation failed before commit with compile error after adding the resident call payload parameter:

```text
error[E0061]: this function takes 4 arguments but 3 arguments were supplied
    --> crates/mct-daemon/src/main.rs:3611:22
     |
3611 |           let result = execute_resident_call(
     |  ______________________^^^^^^^^^^^^^^^^^^^^^-
...
note: function defined here
    --> crates/mct-daemon/src/main.rs:1483:10
     |
1483 | async fn execute_resident_call(
     |          ^^^^^^^^^^^^^^^^^^^^^
...
1487 |     _inline_payload: Option<Vec<u8>>,
     |     --------------------------------
```

- 2026-07-06 D5.2 targeted test invocation failed because `cargo test` accepts only one positional test filter:

```text
error: unexpected argument 'resident_process_payload_delivery_returns_digest_and_keeps_ledger_byte_free' found

Usage: cargo test [OPTIONS] [TESTNAME] [-- [ARGS]...]

For more information, try '--help'.


Command exited with code 1
```

- 2026-07-06 D6b targeted blob-store test failed before the implementation commit with ambiguous `File::by_ref` after adding bounded fetch reads:

```text
error[E0034]: multiple applicable items in scope
   --> crates/mct-daemon/src/blob_store.rs:127:14
    |
127 |             .by_ref()
    |              ^^^^^^ multiple `by_ref` found
    |
    = note: candidate #1 is defined in an impl of the trait `std::io::Read` for the type `std::fs::File`
    = note: candidate #2 is defined in an impl of the trait `std::io::Write` for the type `std::fs::File`
help: disambiguate the method for candidate #1
    |
126 -         let read = file
127 -             .by_ref()
126 +         let read = std::io::Read::by_ref(&mut file)
    |
help: disambiguate the method for candidate #2
    |
126 -         let read = file
127 -             .by_ref()
126 +         let read = std::io::Write::by_ref(&mut file)
    |

For more information about this error, try `rustc --explain E0034`.
error: could not compile `mct-daemon` (lib) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `mct-daemon` (lib test) due to 1 previous error


Command exited with code 101
```

## Slice 1 close-out

### Commit list (D3-D5 plus D5.2-D5.3)

- `0d5f67f feat(iroh): carry inline call payloads`
- `cfcd319 test(iroh): update payload fixtures`
- `2a53e31 feat(daemon): deliver inline call payloads`
- `a28ec60 test(daemon): prove payload roundtrip`
- `d10851f test(daemon): assert ledger excludes encoded bytes`
- `c538f7a refactor(kernel): one exact size name`

### Flake log status

Recorded failures before rerun:

- 2026-07-05 D3 validation failed before commit with Rust compile error `E0061` after adding the resident call payload parameter; fixed by passing `None` at the affected test call site, then full validation passed.
- 2026-07-06 D5.2 targeted test invocation used two positional `cargo test` filters and failed with `unexpected argument 'resident_process_payload_delivery_returns_digest_and_keeps_ledger_byte_free'`; rerun with valid filters passed.

No unresolved flakes remain.

### D5 proof transcript

Command run from disk:

```text
cargo test -p mct-daemon resident_mother_payload_roundtrip_verifies_result_digest -- --nocapture
```

Transcript:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.21s
     Running unittests src/lib.rs (target/debug/deps/mct_daemon-5682d471ecfb696f)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 90 filtered out; finished in 0.00s

     Running unittests src/main.rs (target/debug/deps/mct_daemon-bafc4e92ac572023)

running 1 test
mct resident mother endpoint_id=78a59746a6bf12bb1812397dfc1b50a488536f2c9f074c32b9fe1b5a7cbfd87f
ticket={  "endpoint_id": "78a59746a6bf12bb1812397dfc1b50a488536f2c9f074c32b9fe1b5a7cbfd87f",  "direct_addresses": [    "10.10.10.182:55392",    "10.10.10.209:55392",    "100.114.124.29:55392"  ],  "relay_urls": []}
mct resident mother children loaded=1 failed=0 bindings=1 max_connections=8
mct daemon serving control uds on /var/folders/6h/329275913d1d3k1lfvvvryp40000gn/T/.tmpWmuWDU/control.sock
test tests::resident_mother_payload_roundtrip_verifies_result_digest ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 1.10s

     Running tests/wasm_limits.rs (target/debug/deps/wasm_limits-1c1e86fd6c6a1343)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
```

Concrete proof assertions in `resident_mother_payload_roundtrip_verifies_result_digest`:

- the caller establishes `mct/hello/0` with an admitted binding;
- the request sends inline JSON bytes `{"secret":"payload-marker"}` over `mct/call/0` with exact `size_bytes` and BLAKE3 digest;
- the resident process child reads stdin and returns `processed:{"secret":"payload-marker"}`, proving output depends on input payload;
- the caller receives verified inline result bytes, `CallProtocolReplyOutcome::Success`, exact result `size_bytes`, and matching result BLAKE3 digest;
- the ledger contains the call id plus request/result size-and-digest facts, and excludes raw request bytes, raw result bytes, base64 request bytes, and base64 result bytes.

### ROADMAP follow-on

Recorded in `layer/surface/build/product/ROADMAP.md`: after slice 2 (local content-addressed blob store), the follow-on is Iroh blob transfer between Mothers. D6 remains pending operator decision; slice 1 stops at D5.

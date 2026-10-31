# M4/L15 — bounded WASM invocation and immutable execution bytes

## Scope and checkpoint

Local implementation on `experiment/git-toy-confinement`, atop H3/M1 commit
`039a2772b1577dd2cc4a490419b7d2ce3d793298` (parent
`d974af5933cb120747f4c1fbab56a486fa065f3e`). H3/M1 was committed separately;
this M4/L15 checkpoint is committed separately atop it. Nothing was pushed;
`main` was not changed.

This is implementation evidence, not independent review, sustained-load/RSS
assurance, or formal audit closure. L16's manifest approval unit is unchanged.

## L15: the verified bytes are the compiled bytes

`children/component_bytes.rs` acquires a private immutable `Box<[u8]>` from a
regular file. It checks metadata as an early rejection, then independently
counts actual reads with an overflow probe; metadata cannot authorize a larger
allocation. Limits are 64 MiB for components, 1 MiB for manifests, and 256 bytes
for sidecars. Inventory uses the same bounded acquisition helpers.

At each execution, the runtime stages bounded bytes, verifies their SHA-256
against the admitted `AuthorizedChildInvocation` artifact identity
(`sha256:<64 lowercase hex>`), then passes that **same snapshot** to
`Component::from_binary`. No execution compiler call reopens the pathname.
Both scalar execution APIs and typed WIT execution use this path. Typed
execution also checks loaded-child integrity flags and consistency among the
loaded digest, artifact ID and authorized artifact ID. An audit-only/unverified
child cannot use that path to bypass integrity enforcement.

Discovery remains non-authoritative: it can inspect a bounded unverified
snapshot, but that is not an execution approval. Sidecars cannot replace the
admitted digest. The boundary protects against mutation before/during staging
by digest comparison and against mutation after staging by snapshot ownership;
it does not depend on a final hash of a second file read.

### Retained counterexample and controls

Before wiring the execution snapshot into Wasmtime, the deterministic test
`changed_component_after_inventory_verification_cannot_execute_substituted_bytes`
failed: a different valid component with the same export returned **99**, despite
inventory/authorization naming the original component. `l15-before.log` retains
that failure. This was an instrumented pre-runtime-fix tree atop `039a277`, with
the regression and bounded reader already added, not an untouched baseline run.

The retained regression now rejects both in-place overwrite and rename
replacement, even after updating the untrusted sidecar to match the replacement.
A separate test stages/verifies the original, overwrites and removes its source,
then compiles and executes the snapshot: it still returns **14** for input 7.
Tests also cover wrong artifact identity, unverified inventory flags, exact byte
caps, overflow without metadata reliance, sparse oversized files and directories.

## M4: invocation-local accounting before more effects

`MctWasmHostConfig::invocation_limits` exposes `MctWasmInvocationLimits`:

| Resource | Default cap per invocation |
| --- | ---: |
| Wasmtime host entries, including WASI | 4,096 |
| `wasi:logging` calls | 256 |
| `patina:measure` calls | 1,024 |
| Toy calls | 1,024 |
| Runtime + Toy observations | 2,050 |
| Accumulated accounted bytes | 4 MiB |
| MCT host argument values / typed input JSON | 64 KiB |
| Individual Toy/final output or produced message | 256 KiB |

The first exhausted limit wins; a count allowance does not promise enough bytes
for that many large calls. Limits are host-owned configuration, not guest input.

- A Wasmtime `CallingHost` hook counts entries before host callbacks, including
  calls made during component initialization. Logging and measure have additional
  independent counters. Counters and sticky failure state belong to each Store.
- Runtime start/terminal observation space is reserved before guest execution.
  Each Toy call reserves two observation slots and conservative byte space
  **before** entering the Toy registry/effect path. Reservations include
  correlation fields, serialized escaping, enum differences and native container
  overhead; JSON counting uses a bounded writer rather than allocating a second
  serialized buffer. MCT dynamic arguments are checked before further copying.
- Accumulated charging includes typed input, MCT host arguments, Toy input/output,
  observation reservations, keyvalue output, produced messages and final output.
  Recursive dynamic-value checks include payloads and container overhead; final
  JSON is also checked, including escaping expansion.
- Budget exhaustion traps and returns a failed runtime report with no successful
  output or produced-message delivery. All facts from preceding Toy effects are
  retained. In particular, an oversized result **after** a Toy effect does not
  erase its completed observation. Initialization failures and post-effect traps
  also retain their facts; timeout reports no longer drop accumulated Toy facts.
  Scalar Toy invocations now give repeated calls distinct observation IDs.
- Native Git helpers no longer use unbounded `Command::output`: concurrent pipe
  readers cap stdout at 256 KiB and stderr at 64 KiB, kill the immediate child on
  overflow, reap it, and fail. This producer-side cap also applies to non-WASM Git
  calls; existing confinement and stderr redaction are unchanged.

**Durability is not relaxed.** These are in-memory resource reservations, not a
new claim of durable reservation. Existing Toy authority/order checks and
canonical writer integration remain in place. No required fact is sampled or
silently dropped to fit a quota; inability to reserve the next effect's evidence
stops that effect. `layer/core/safety-boundaries.md` Rule 3 remains authoritative.

### Deterministic regression coverage

`wasm/tests/boundaries.rs` uses finite generated component programs, not races or
unbounded floods:

- Logging, measure, global-host and Toy limits: four attempted calls with a cap
  of three fail; exactly three Toy start/completion pairs survive. Three calls at
  the exact cap succeed.
- Observation cap six admits two Toy calls plus runtime start/terminal, not a
  third effect. An 8 KiB aggregate cap terminates further work with bounded facts.
- A 64 KiB logging payload exceeds the host-value envelope before entering the
  backend; 257 small log attempts hit the default 256-call cap.
- Initializer exhaustion retains its completed Toy facts; post-effect Toy output
  exhaustion retains both facts and suppresses successful output.
- Scalar Toy exhaustion preserves distinct observation IDs and the same fail-
  closed behavior. Oversized final component strings fail; a sufficient cap is a
  successful control.
- Unit tests cover exact byte accounting, sticky failure, fresh invocation state,
  escaped JSON, nested dynamic values, and bounded native stdout overflow.

Existing positive controls cover typed/scalar exports, WASI, logging, measure,
Git edit/stage/commit, Slate, the folder watcher and watch-null-sink. Fixture
permissions were not broadened: test authorization now names actual component
SHA-256 identities instead of synthetic artifact strings.

## Validation and reproduction

Environment: macOS/aarch64, Rust 1.96.0, Wasmtime 48.0.7. Local scratch receipts:
`/tmp/mct-m4-l15-wasm-boundary-evidence/`. Temporary logs are not durable artifacts;
the counterexamples and generators above are retained in source.

```sh
export CARGO_BUILD_JOBS=8 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_NET_OFFLINE=true
cargo test -p mct-daemon --lib --locked --offline wasm:: -- --test-threads=1
MCT_ORIGIN_KEEP=1 MCT_FLEET_KEEP=1 MCT_RESTART_KEEP=1 \
  cargo test --workspace --locked --offline --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all --check
git diff --check
bash scripts/ci-tier0.sh specs
```

Final validation: **794 workspace tests passed, 0 failed, 2 pre-existing ignored**;
**42 focused WASM tests passed**. This adds 13 tests over the H3/M1 checkpoint.
Workspace Clippy with warnings denied, fmt, `git diff --check`, and tier-0 specs
all passed. Specs included 17 structural/tooling tests and explicitly made no
readiness claim. All five final `*.exit` receipts are `0`.

Intermediate checks caught synthetic test artifact identities, a fixture WAT
syntax error and test-only Clippy clone warnings; these were corrected before
the final runs. A final review also added preservation of initializer Toy facts
when a scalar export is missing; its regression is included in the final run.

| Local log | SHA-256 |
| --- | --- |
| `l15-before.log` (expected failing counterexample) | `21c8649bc7ed68eef39bbeb87244b161a0fa29124aea18f7f3d323cd4c20b551` |
| `final-wasm-tests.log` | `d4a980dabc6dbc09cef840dbb9dd22318e3eb21071381f654f5583f26f4ef70f` |
| `final-workspace-tests.log` | `746ba46c59bfda4e65a39b1602980e1865ff8ef069e9cc78b7779f63f9f222d5` |
| `final-clippy.log` | `0c27bed0ddbb48eea172769617d880c7f83b7e6a6ba04e5bc4679bea741f1fb2` |
| `final-fmt.log` (empty, successful) | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `final-specs.log` | `102b041cb2b05cc96c5361f6f60e13703b3b0c8f8d492cbf66334e6155e251c7` |

## Remaining scope

- Aggregate fleet fairness, sustained ledger growth, cross-invocation storage
  quotas and extended hostile-load campaigns remain separate work.
- The byte budget is an accounting envelope, **not** a process RSS governor.
  Wasmtime canonical ABI lifting happens before dynamic MCT value checks;
  compiler allocations, standard WASI per-call buffers, allocator overhead and
  native Git working memory are not bounded by the 4 MiB counter. Host entry
  counts include WASI, but the cumulative MCT payload counter is not a counter of
  every byte transferred by standard WASI. Further ABI/allocator abuse review and
  process-level resource assurance remain necessary.
- Native output caps do not add a native subprocess time/process-tree budget.
  Component-file byte caps likewise do not supply a filesystem I/O deadline.
- Content binding does not establish publisher authenticity or redefine manifest
  authority (L16), artifact publication ownership, or preopen delegation/revocation.
- Independent review and formal finding closure remain outstanding.

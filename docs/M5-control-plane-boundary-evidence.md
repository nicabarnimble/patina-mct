# M5 / L2 / L3 — bounded local control transports

## Checkpoint and scope

Implemented locally on `experiment/git-toy-confinement`, atop M4/L15 commit
`68280db` (direct parent `039a277`). M4/L15 was committed separately as requested.
This control-plane checkpoint is committed separately atop it. Nothing was pushed
and `main` was not changed.

The backlog's original IDs remain intact: **M5** covers optional HTTP control,
**L2** covers CLI socket trust/reply bounds, and **L3** covers isolated startup
listener failure/occupancy. The requested HTTP bind restriction and CLI limits
span these items; the task's alternate L2/L3 grouping does not renumber findings.
This is local implementation evidence, not independent review or formal closure.

## Baseline and changes

Source inspection at `68280db` found serial HTTP and standalone UDS loops whose
connection errors escaped via `?`; HTTP parsed a single, potentially partial
4096-byte read. The isolated startup listener serially read headers/body without
timeouts and propagated connection errors. Resident UDS already used 64 concurrent
connection slots and bounded request reads, but response writes were unbounded.
CLI status/mutation and supervisor lifecycle clients had unbounded response
accumulation and per-read rather than whole-exchange timeouts. No pristine-baseline
exploit run or sustained-flood experiment is claimed here.

- HTTP, standalone UDS and isolated startup now use bounded concurrent task sets.
  A malformed, disconnected or slow peer affects only its own task; completed
  tasks are reaped. Excess connections are closed without queuing more tasks.
  Connection failure/capacity counters saturate; optional diagnostics are emitted
  at power-of-two counts, not once per offending byte. These are not canonical
  authority observations and do not replace any required facts.
- Resident UDS retains its existing admission/capacity, authenticated owner and
  call-preflight paths, with bounded response writes added. Transient aborted,
  reset, interrupted or would-block accepts are retried; genuine listener/bind
  failures still surface as failures rather than being silently swallowed.
- Strict complete-header framing rejects duplicate lengths/credentials, transfer
  encoding, folded/malformed headers, invalid lengths and malformed request lines.
  Declared body size is checked before allocation or JSON parsing. Read-only
  routes do not consume unsupported POST bodies and still return 405 for ordinary
  unsupported methods; GET bodies and oversized declarations are refused.
- Isolated startup uses the same transport bounds and remains owner-checked.
  Its authority operations remain serialized, off the async IO worker. Once a
  startup gate commits, queued operations cannot act on the old posture; failure
  to deliver its reply does not undo that commit. Shutdown fences queued operations;
  an already-started durable operation is not retroactively rolled back.

## Explicit limits

| Boundary | Limit |
| --- | --- |
| Accepted connection tasks per listener | 64 |
| Request header bytes | 4 KiB, complete `CRLF CRLF` required |
| Header read phase | 3 seconds absolute, not reset per byte |
| Body read phase | Separate 3 seconds absolute |
| Response write phase | 3 seconds absolute |
| Ordinary owner-authenticated UDS mutation body | 1 MiB, with tighter existing route validation retained |
| Local `/calls` body | Existing 96 KiB frame limit; existing authenticated/durable preflight remains first |
| Isolated startup body | 64 KiB |
| Legacy `/blobs` request | Existing 8 MiB blob's base64 envelope plus 4 KiB total framing budget, less actual headers |
| Ordinary HTTP/UDS serialized response body | 4 MiB before making the wire-response copy |
| CLI response headers / body | 4 KiB / 4 MiB; length required and validated before body allocation |
| CLI status exchange | 2 seconds for connect + writes + complete response |
| CLI mutation exchange | 10 seconds for connect + writes + complete response |
| Supervisor lifecycle exchange | 3 seconds for connect + writes + complete response |

Transport phases do **not** wrap authority evaluation, durable append, or admitted
operation execution in a new cancellation timeout. Required `BeforeEffect`
observations and existing writer/order checks are unchanged. A paused-clock test
holds a mutation callback beyond three transport deadlines, then releases it and
receives its response successfully. Existing durable-append failure regressions
remain in the validation set. A client timeout after submission is an uncertain
reply outcome, not proof that no effect committed, and never triggers an offline
mutation retry.

## HTTP exposure policy

Both `serve --http` and `control serve-http` reject non-loopback addresses by
default, including wildcard IPv4/IPv6 binds. Hostnames are resolved once with a
3-second resolution deadline; **every** candidate must satisfy policy, then those
exact addresses are passed to bind (no second DNS lookup).

Only the standalone, read-only diagnostic command offers the explicit override:

```text
mct-daemon control serve-http <addr> --allow-unauthenticated-non-loopback-http
```

It prints a warning. This exposes unauthenticated diagnostic data, without TLS;
it does not add mutation routes or administrative authority. Prefer loopback/UDS
and a separately secured tunnel. Resident `serve --http` remains loopback-only,
without that override. Loopback is network containment, **not** proof of ownership.
No HTTP mutation route was added. D-57's strict authenticated-administration
cutover is unchanged; this work does not reopen closed administrative surfaces.

## CLI endpoint verification

`control_transport/client.rs` is shared by CLI status, CLI mutations and supervisor
lifecycle requests. Before sending any payload it requires:

1. A real socket, not a final-component symlink or regular file; owner-only mode
   and ownership by the current effective UID (obtained from kernel credentials
   on a private socket pair, without unsafe code).
2. A canonical parent owned by that UID and not group/world writable. Ancestors
   must be root/current-UID owned and not group/world writable; root-owned sticky
   temporary roots are permitted above the owner-controlled parent.
3. Matching device/inode metadata checked again after connecting, plus live peer
   credentials matching that UID. Parent checks are repeated as part of this check.

Only an absent endpoint or a verified stale endpoint refusing connection permits
existing offline fallback. Untrusted metadata, permission errors, protocol errors,
oversized/truncated replies and timeouts fail closed. Content-Length framing means
clients do not need to wait for EOF after the bounded response body. A small
current-thread Tokio runtime on a scoped worker provides one absolute IO deadline
without nesting a runtime on the CLI's async thread.

**Trust limit:** these checks do not authenticate the actual Mother executable or
distinguish a malicious process running as the same UID. Root/current-UID control
of trusted directories remains an assumption. Same-UID process authentication
requires the separately governed authenticated administration path; ownership is
not substituted for canonical authorization. Metadata checks assume a functioning
local filesystem; socket deadlines are not filesystem-operation timeouts.

## Retained deterministic tests

- `daemon/control/boundaries.rs`: real HTTP, standalone UDS and resident UDS
  listeners. A valid request succeeds while a predecessor holds partial headers;
  the predecessor expires. Oversized headers/body declarations, ambiguous lengths,
  malformed and disconnected clients are followed immediately by successful
  status requests. Resident UDS also covers a partial authenticated mutation body.
- `daemon/resident/serving/startup_control/tests.rs`: actual isolated startup bind,
  partial body, oversized header/body, malformed/disconnected clients, later valid
  health reads and shutdown with an idle peer. Exact canonical ledger bytes remain
  unchanged by malformed traffic.
- `control_transport/tests.rs`: paused-clock header/body/write deadlines, cap
  rejection before reads, strict framing, transient accept classification, bounded
  task occupancy/error reaping, loopback bind rejection and the non-cancelled
  mutation callback control.
- `control_transport/client/tests.rs`: exact response cap positive control,
  oversized declarations without sending their bodies, partial-response deadline,
  ambiguous/truncated framing, and untrusted socket type/symlink/permissions,
  parent mode and expected-owner checks before connection/payload delivery.
- `daemon/cli_admin_boundaries.rs`: the actual synchronous status and mutation
  entry points reject oversized replies and unverified endpoints without offline
  fallback. Existing real-resident status and lifecycle controls remain covered.

Tests use disposable fixtures and finite payloads, explicit task/channel barriers,
real socket deadlines with bounded outer watchdogs, and paused time where no
kernel IO scheduling is involved. No cross-UID privileged impersonation test or
external/non-loopback exposure experiment was run. The opt-in acceptance control
binds only loopback; default rejection tests fail before non-loopback bind.

## Validation and receipts

macOS/aarch64, Rust 1.96.0. Local scratch logs:
`/tmp/mct-m5-control-plane-boundary-evidence/`. Temporary log hashes identify local
receipts; retained source tests are the durable reproduction mechanism.

```sh
export CARGO_BUILD_JOBS=8 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_NET_OFFLINE=true
cargo test -p mct-daemon --locked --offline control -- --test-threads=1
cargo test -p mct-daemon --bin mct-daemon --locked --offline \
  cli_admin::boundaries -- --test-threads=1
MCT_ORIGIN_KEEP=1 MCT_FLEET_KEEP=1 MCT_RESTART_KEEP=1 \
  cargo test --workspace --locked --offline --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all --check
git diff --check
bash scripts/ci-tier0.sh specs
```

Final validation: **810 workspace tests passed, 0 failed, 2 pre-existing ignored**
(16 new tests over `68280db`). **57 focused control tests** and **2 CLI boundary
tests** passed. Workspace Clippy with warnings denied, fmt, `git diff --check`
and tier-0 specs passed; all six command exit receipts are `0`. Specs included
17 structural/tooling tests and explicitly made no readiness claim.

| Log | SHA-256 |
| --- | --- |
| `control-tests.log` | `b7345f08606374cbd6fd075639c392f7f76dc6a073a76bb46e37d403bca14386` |
| `cli-tests.log` | `b379bffd88effbf5cb433cc276fa29628ce2c79ef1b431d827333a8052880c65` |
| `workspace-tests.log` | `4d9af3f1f3e6e6da49ecf9bb0454710a07673f3d1400e22a42fde4f965429379` |
| `clippy.log` | `2634d6bec047c4b96747f1a3f5477692a2fcb9c070b23938b5095ce219b2e697` |
| `fmt.log` (empty, successful) | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `specs.log` | `1c1c4585a4b053198cca0866c8bfb9b19643d20574ced665440124a64247b7dc` |

During development, checks caught module-path wiring, an unsupported POST's 405
compatibility expectation, a test-only listener missing production's 0600 mode
(which left its accept task waiting until the test-command watchdog fired), and
a Clippy formatting warning. The fixture now uses production permissions and a
bounded join; endpoint validation was not relaxed.

## Still open

- Independent review, sustained-flood admission fairness, logging/storage-rate
  assurance, external consumer inventory/transport retirement, and formal closure.
  A 64-slot cap alone cannot guarantee service under continuous saturation.
- Execution, SQLite/filesystem latency, response construction/RSS and already-
  admitted durable work are not governed by these socket IO deadlines. This is
  not a general process-resource governor or a new durability policy.
- Same-UID malicious process/path substitution and stronger process/server identity
  require the authenticated-admin work rather than a permissive UID workaround.
- Platform coverage here is macOS; Linux/other supported Unix validation and
  privileged cross-UID credential fault testing remain separate assurance.

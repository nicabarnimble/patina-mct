# L7 / L8 / L9 / L11 / L12 — peer and ledger boundary evidence

## Checkpoint and status

The verified storage batch is committed as
`1a189f52b35df58c98808cc888b6c97b671c9f7c`
(`fix(storage): enforce private descriptor paths and fail closed on corrupt state`),
directly atop `a187344aa2af4cb61e8bbcedb2d7e6a74a441ee2`. It contained the requested
19 files and left a clean tree. This subsequent peer/ledger batch is separately checkpointed directly atop that
storage commit, on `experiment/git-toy-confinement`. No push or change to `main`.

This records local implementation and validation, not independent review,
severity reassessment, deployment readiness, or formal finding closure. See the
[individual security TODO entries](TODO-security-and-finish.md).

Required `BeforeEffect` callbacks remain awaited. Failure to persist the required
admission/correlation/dispatch/authority fact denies the corresponding effect.
No new timeout cancels authority or durability work, no observation becomes an
optional projection, and no grant is broadened for compatibility.

## L7 — admitted Vision is the publication audience

`MctHelloCapabilityView::for_vision` projects a view for an explicitly admitted
Vision. The Iroh hello response boundary calls it with the evaluator's
`selected_vision_id`, not a wire-selected or requested label. Both static views
and dynamically refreshed views pass through it. This also covers the resident
provider in `serving.rs`, instead of relying on each provider to remember the
transport audience.

A view for a different Vision is omitted entirely: the product still hosts one
Vision, and the patch does not relabel another Vision's inventory. Within a
matching view, out-of-scope surfaces are removed, advertised WIT worlds are
rebuilt from retained surfaces, and opaque inventory references are removed.
Outbound resident refresh and forwarding hellos also scope their advertised view
to the configured peer Vision. Existing out-of-scope call denial stays generic;
filtering does not authorize an otherwise denied call.

The wire regression covers static and dynamic providers, admitted peers in two
Visions, mixed-scope inventory, derived world/reference metadata, and a generic
out-of-scope call denial. This is not multi-Vision hosting or a new federation
sharing policy.

## L8 — caller correlation is not durable execution identity

The resident peer path mints a fresh, random Mother execution `CallId` for each
arrival, before routing or execution. A required correlation fact records the
caller ID and admitted peer binding separately. The execution ID flows through
routing, Child/Toy capabilities, run IDs/results, forwarding, and recovery.
Caller-chosen IDs cannot select the reserved Mother execution namespace through
local ingress. These refusals await their own denial fact without attaching the
rejected ID as an execution/recovery key. Nested hops mint their own execution
identity; trace/correlation
metadata remains inspectable without becoming a recovery key.

Idempotency fingerprints retain the original caller correlation and existing
caller scope, so a retry still matches the original request rather than the new
nonce. A nullable `execution_call_id` SQLite column binds a fresh reservation to
its Mother execution before canonical dispatch/effect. Recovery uses that
association, not the caller fingerprint's call ID. Independent unkeyed attempts
and new executions after key expiry can therefore use fresh identities; keyed
replay does not execute again. Standalone peer idempotency also uses a separate
Mother execution identity for reservation observations.

Iroh lifecycle observations retain wire IDs inside explicit transport-correlation
metadata, with no execution `call_id`. Recovery also excludes historical
peer-transport `ResultRecorded` entries. Old peer reservations without an
execution association are isolated and settled as unknown/interrupted, not
matched to a local run by raw ID. Their disposition does not pretend to name a
known execution. Existing local/trigger identities and canonical dispatch retry
fences remain intact.

Tests keep a local run active while peers reuse its raw call ID, exercise
same-peer keyed replay, separate binding scopes, and repeated unkeyed arrivals.
A recovery test combines an ambiguous historical peer reservation and a colliding
transport denial, proves independent interruption rather than adopting the local
run/result, and verifies the next settlement is a no-op. Existing three-Mother
origin and payload tests now follow explicit correlations, including their
negative assertions; merely searching for a raw ID on an execution record would
no longer prove the security property.

No old ledger bytes are rewritten. This does not reconstruct all previously
ambiguous/unscoped historical lineage or turn SQLite into the canonical retry
fence; migration/rollback review remains open.

## L9 — forwarding borrows the resident transport lifecycle

`MotherIrohClient` is a cloneable **send-only** handle to the same underlying Iroh
endpoint. It exposes hello/call operations and transport identity, not bind,
serve, close, or a raw endpoint to a Child. Dropping a send handle does not close
the live owner's endpoint; explicit owner close closes the shared endpoint even
while handles remain.

Resident startup attaches this handle to the runtime used by peer ingress,
local control, triggers, and nested callouts. Forwarding no longer reloads the
node secret or binds/closes a competing endpoint per call. It revalidates that
the handle identity matches the selected local Mother identity. A caller without
a resident transport handle fails closed; there is no same-key bind fallback.
Relay policy is the resident endpoint's configured policy, not a forwarding-time
implicit override.

The new local Iroh test performs simultaneous outbound and inbound hellos through
the shared endpoint, verifies dropping a clone leaves serving usable, and checks
owner shutdown closes retained clients. Existing two-/three-Mother forwarding,
revocation, cancellation, interrupted-reply, fleet, and shutdown tests remain in
the full suite. Unit fixtures without an origin resident now explicitly provision
a test transport rather than depending on a production fallback.

This eliminates the competing endpoint construction from the production path.
It is not an independently reproduced hosted-relay interference experiment or a
cross-platform/relay-fleet reachability assurance claim.

## L11 — ticket consumption requires independent admission context

Discovery found pairing tickets in canonical authority mutation/replay machinery,
not a production ticket-redemption hello handler. [D-13](DECISIONS.md) explicitly
still lists redeem-over-Iroh as unbuilt. This patch does not silently implement
bootstrap, enrollment, fingerprint confirmation, or a D-57 compatibility ingress.

Generic authority mutation entry points reject new `PairingTicketConsumed`
commands, even if they carry serialized admission claims. The narrow
`execute_pairing_ticket_admission` host-adapter seam accepts exactly one
consumption change, checks its context against a **separate authenticated peer
argument and Mother clock argument**, and checks the canonical issued ticket:

- the target endpoint must match;
- `window_starts_at` (not-before) must be present and no later than Mother time;
- Mother time must be strictly before `window_expires_at`;
- the ticket must be issued and not consumed.

Time comparisons use parsed instants, including RFC3339 offset equivalence.
Missing historical target/window fields are readable but inert for new admission.
The recorded admission context is replay-checked against the issued ticket;
absent context remains readable on historical consumption facts. Consumption
uses the existing canonical mutation writer and durability/uncertainty semantics.
A consumed ticket never yields a second admission, including after a complete
commit whose acknowledgement was lost.

**Trust restriction:** the new API is not a cryptographic transport authenticator.
Its peer/time arguments must come from trusted host adapters, never a decoded
caller request or copied claim. No production caller supplies those facts in this
batch. The method only records ticket consumption; it neither grants peer/admin
rights nor bypasses the existing refusal of legacy transitions in enrolled strict
profiles. D-57's fresh cutover remains mandatory. The signed measurement fixture
is updated to provide explicit fixture admission context; historical replay
fixtures use private test constructors, not a production bypass.

Tests cover wrong peer, early/exact-expiry/late time, caller-time substitution,
mixed authority changes, generic-writer refusal, missing historical scope,
positive target/window consumption, replay/restart rejection, and complete
unacknowledged consumption.

## L12 — ordinary append cannot manufacture canonical authority

Ordinary `append`, `append_before_effect`, sequential before-effect batches, and
single-sync durable batches reject the reserved `mct-authority-` and
`ledger-tail-recovery-` detail namespaces. That includes unknown future versions
and malformed payloads, not only recognized valid records. A complete valid
canonical authority fact is rejected through the ordinary/buffered API too.

Validated epoch, genesis, mutation, and import writers use a crate-private
canonical append path; there is no public unchecked authority append method.
The existing recovery writer remains its own validated typed path. Denied
ordinary facts do not advance the head, alter authority, poison the replay
cache, or write bytes. A single-sync mixed batch is refused before any frames
are written. A sequential before-effect batch retains its existing explicit
acknowledged-prefix semantics: earlier ordinary facts can commit, but the
reserved fact cannot, and no false authority is created.

Informational startup drift metadata now uses `mct-startup-drift-v1`, outside
the reserved authority namespace. Its historical records remain readable.
Hostile-history tests now deliberately construct/re-hash forensic bytes on disk
(or use a private test-only canonical constructor), instead of requiring the
ordinary production API to accept a forgery. These fixtures are not runtime
repair or a new public bypass. Other trusted host-generated runtime/dispatch
record namespaces are not claimed to have received a universal provenance redesign.

## Regressions and development corrections

Eight new deterministic boundary tests:

| Finding | Test |
| --- | --- |
| L7 | `l7_hello_publication_uses_admitted_vision_and_rebuilds_derived_metadata` |
| L8 | `l8_peer_duplicate_call_ids_get_distinct_execution_ids_without_breaking_keyed_replay` |
| L8 | `l8_legacy_peer_reservations_and_transport_results_cannot_adopt_local_runs` |
| L9 | `l9_shared_send_handles_preserve_inbound_serving_and_owner_shutdown` |
| L11 | `l11_pairing_requires_authenticated_target_mother_window_and_single_use` |
| L11 | `l11_historical_unscoped_tickets_are_readable_but_inert_for_admission` |
| L11 | `l11_complete_unacknowledged_consumption_is_not_admitted_twice` |
| L12 | `l12_ordinary_single_and_batched_appends_cannot_write_authority_namespaces` |

Development failures were corrected, not counted as closure evidence: ordinary
startup drift used the newly reserved namespace; legacy hostile fixtures relied
on unguarded append; forwarding fixtures lacked an explicitly owned transport;
and old test assertions joined execution records by raw peer IDs. Assertions
were updated to preserve their positive and negative properties, not grants or
authority checks weakened to recover green tests. No pristine-baseline exploit
run is claimed.

## Validation

macOS/aarch64, Rust 1.96.0. Logs are under
`/tmp/mct-l7-l8-l9-l11-l12-peer-ledger-evidence/`. Legacy filesystem fixtures use
the storage batch's private umask/canonical temporary-root setup; production
does not change process-wide umask.

```sh
export CARGO_BUILD_JOBS=8 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_NET_OFFLINE=true
export TMPDIR="$(cd "${TMPDIR:-/tmp}" && pwd -P)"
umask 077
for finding in l7_ l8_ l9_ l11_ l12_; do
  cargo test --workspace --locked --offline "$finding" -- --test-threads=1
done
MCT_ORIGIN_KEEP=1 MCT_FLEET_KEEP=1 MCT_RESTART_KEEP=1 MCT_RESTART_SEED=0x6d637421 \
  cargo test --workspace --locked --offline --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all --check
git diff --check
bash scripts/ci-tier0.sh specs
```

Final results: **842 workspace tests passed, zero failed, two pre-existing
ignores**. All eight new regressions passed their focused filters. Workspace
counts (excluding subprocess re-execution and zero-test doc targets):
`258 + 199 + 1 + 7 + 3 + 2 + 2 + 58 + 177 + 135 = 842`.
Clippy with `-D warnings`, formatting, diff checks, and tier-0 specs passed; specs
include 17 structural/tooling tests. All nine `verified-*.exit` receipts are `0`.
Final review also added an explicit awaited reserved-ID refusal fact and its
regression before this last complete rerun. The verified 24-file batch is checkpointed together; independent review remains
separate from this local commit.

SHA-256 test-log receipts:

| Log | SHA-256 |
| --- | --- |
| `verified-l7_tests.log` | `ac8f3c26d873556cbf7b59530648784032e1548d5987c005cddd4efc3bf5ac1d` |
| `verified-l8_tests.log` | `ecfc58ffd4c80ebe3662a21619d8720fd08c35c0487ee5a907d3ae23795f9ce5` |
| `verified-l9_tests.log` | `27c50fea05a27d70b22c4c6721dc24317a64c93df253dbd013cb40da0e41d188` |
| `verified-l11_tests.log` | `80ba1d3ceeaa344b3ff4f980c93a7a0b8681241759744eef772caac426980ffe` |
| `verified-l12_tests.log` | `a6742c6d923ba936a3eb190ba2a87b854c68844777ae2cb8592690278e99f6bf` |
| `verified-workspace-tests.log` | `8fb1fe03664d9f2f041230606fadb076a60809da0175ac7fe598b71d85f9689b` |
| `verified-clippy.log` | `a643a517ee1d61e77beea812fc33abad6f8dbc6e719e3079467031463fb08c00` |
| `verified-fmt.log` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `verified-specs.log` | `1c1c4585a4b053198cca0866c8bfb9b19643d20574ced665440124a64247b7dc` |

## Remaining assurance

Independent review, severity reassessment, historical lineage/migration review,
platform and hosted-relay/load assurance remain open. D-13 ticket redemption,
strict hybrid administration/fingerprint confirmation, and D-57 production
cutover are not enabled or certified by this work. Test-owned peer/time facts
are not evidence of a production authenticator. Local passing tests are not
formal closure of any of the five findings.

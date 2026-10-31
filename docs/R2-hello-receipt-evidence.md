# R2 — Interleaved hello receipt replacement

Local fix on `experiment/git-toy-confinement`, after the separate H1 (`7aafdcc`)
and R1 (`e739001`) checkpoint commits. No push or main-branch change.

## Reproduction and retained evidence

The historical report was `mother-0 -> 2`, with a local result naming
`route-revalidation:mother-0-to-2-connected`. That original failed fixture was
not retained. This investigation does not claim to have recovered it.

On unchanged R1 checkpoint `e739001`, 17 fleet runs passed; run 18 reproduced
the same connected-phase denial on **mother-0 -> mother-3**:

```text
outcome: denied
route_decision_id: route-revalidation:mother-0-to-3-connected
route_taken: null
safe_message: not authorized
```

All runs used `MCT_FLEET_KEEP=1`. The failing run is
`/tmp/mct-r2-route-evidence/baseline-18.log`, with these retained fixtures:

| Mother | Root containing logs and `root/` state |
| --- | --- |
| 0 | `/tmp/mct-fleet-uqS5AU` |
| 1 | `/tmp/mct-fleet-8N2pkH` |
| 2 | `/tmp/mct-fleet-Uih8DZ` |
| 3 | `/tmp/mct-fleet-Yjj9VC` |
| 4 | `/tmp/mct-fleet-8sM7hC` |
| 5 | `/tmp/mct-fleet-YSmnk1` |

The original ledgers, configs, SQLite projections and boot logs remain retained.
The investigation used read-only ledger/config inspection and read-only SQLite
queries, not fixture replay or repair. Mother-0's policy revision was 1,
mother-2's was 2, and mother-3's was 11, as intended by the fleet fixture.
The mother-0 -> mother-2 connected call passed in this reproduction.

### Decisive timeline (UTC 2026-10-25)

| Node / sequence | Time | Fact |
| --- | --- | --- |
| mother-0 / 72 | 02:13:07.895533 | Remote route selected |
| mother-0 / 73 | 02:13:07.908662 | Route revalidated **allowed** |
| mother-0 / 74 | 02:13:07.923766 | Call forwarding started |
| mother-3 / 33 | 02:13:07.942380 | Forwarding hello admitted, decision suffix `-8`, trace `trace-mother-0-to-3-connected` |
| mother-3 / 34 | 02:13:07.960926 | Background refresh hello from the same peer admitted, decision suffix `-12`, trace `trace-refresh-mother-3` |
| mother-3 / 35–37 | 02:13:07.965117 | Call received/constructed, then denied before execution |
| mother-0 / 75 | 02:13:07.973565 | Remote `Denied` reply recorded |

Both hello decisions have prefix `decision-iroh-hello-0afbb3d0ad4ae65e` and
select the same binding, `mother-3-admits-mother-0`. The receiver's ledger shows
no authority mutation between them and the call. Its startup projection was
coherent and authority-ready; the caller's mother-3 surface remained fresh,
with publisher policy revision 11 and the correct local admission binding.

This is **not R1 listener readiness**, nor a local route-selection/revalidation
failure or a correctly stale grant denial. The forwarding adapter preserves its
local revalidation decision ID when mapping a downstream denial, so that ID in
the user-facing result alone did not identify the failure stage.

## Root cause

`MctIrohServeState` kept a single hello evaluation per authenticated endpoint.
Every new hello replaced the previous evaluation (and concurrent hello handling
forgot it before evaluation). Forwarding uses the Mother's identity, also used
by its background peer-refresh loop, so both operations share that cache key.

Each call correctly carries the **specific hello decision ID** returned by its
own handshake. The kernel's `evaluate_call_protocol` correctly requires that ID
to match the admitted hello supplied by the adapter. In the reproduced order:

1. Forwarding hello A succeeds.
2. Refresh hello B succeeds and overwrites A in the endpoint-only cache.
3. Call referencing A is evaluated against B and denied as `HelloNotAdmitted`.

No route-cache freshness change, binding revocation, or receiver epoch change is
needed. A deterministic real-Iroh test forces A -> B -> call(A) and fails under
the old implementation with `Denied` instead of `Success` in 0.09 seconds.

## Fix and preserved boundaries

- Store admitted receipts by **(authenticated endpoint ID, hello decision ID)**.
- Both concurrent serving and `serve_next` look up the call's exact receipt;
  there is no fallback to the latest hello or to a receipt from another peer.
- A successful refresh no longer erases independently admitted receipts.
- Bound the total receipt count at the existing 1,024 limit, evicting oldest
  receipts first. Re-inserting an identical receipt does not grow the FIFO;
  repeated hellos from one peer cannot bypass the bound. Evicted/unknown
  receipts fail closed rather than borrowing a newer admission.
- Denied/failed admission still invalidates that endpoint's receipts. A failed
  durable hello observation also clears them; an unrecorded hello cannot mint
  authority. Receipts of unrelated endpoints are preserved.
- Kernel call authorization is unchanged: binding identity, revocation, expiry,
  ALPN/Vision scope, policy revision, receiver grants generation/epoch, and
  payload integrity remain checked at call time. Remembered hello receipts do
  not override current authority.

No sleeps, retry loops, test deadlines, or stale-authority assertions were
relaxed. The six-process fleet fixture itself is unchanged.

## Regression coverage

New tests:

- `tests::hello_receipts::interleaved_same_endpoint_hellos_preserve_each_calls_exact_receipt`:
  await two real hello responses, then call with each receipt. Both succeed;
  an unknown decision and a receipt used by another authenticated/admitted
  endpoint are denied with no additional handler executions. The ordering is
  deterministic, with no scheduler timing assumption.
- `tests::hello_receipts::denied_or_unrecorded_hello_invalidates_all_prior_endpoint_receipts`:
  two valid receipts and a successful control call, followed by either denied
  admission or an injected durable-observation failure. Neither old receipt
  can execute afterward.
- `serve::tests::same_endpoint_receipts_are_bounded_and_denial_invalidates_only_that_endpoint`:
  1,025 distinct decisions from one endpoint, duplicate inserts, exact FIFO
  eviction, and endpoint-scoped invalidation.

Existing tests are tightened, not weakened: current binding revocation, expiry,
policy revision and scope checks now run against an older receipt after a newer
hello is admitted. The grants-generation and restart-epoch tests verify the old
receipt remains denied with `RefreshHello` **after** a fresh hello/call succeeds.
All existing signed-binding and failed-observation checks remain intact.

## Validation

macOS arm64, locked/offline graph; `CARGO_BUILD_JOBS=8`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`,
`CARGO_NET_OFFLINE=true`.

| Check | Result |
| --- | --- |
| Deterministic A -> B -> call(A), before fix | Failed: `Denied` instead of `Success` |
| Same regression, after fix | Passed |
| Complete `mct-iroh` suite | 47 passed |
| Unchanged six-process fleet suite, 20 consecutive runs | 20/20 passed, all fixtures retained |
| Full workspace (including fleet and SIGKILL restart tests) | 758 passed, 0 failed, 2 pre-existing ignored |
| Workspace/all-target Clippy, warnings denied | Passed |
| `cargo fmt --all --check`, `git diff --check` | Passed |

The full workspace additionally used `MCT_RESTART_KEEP=1`, `MCT_FLEET_KEEP=1`,
`--no-fail-fast` and `--test-threads=1`. The 20 explicit fleet runs each include
the full connected all-pairs matrix, receiver-side revocation, peer loss,
survivor operation and restart checks; they are not reduced smoke tests.

```sh
cargo test --locked --offline -p mct-iroh -- --nocapture --test-threads=1
# Repeat the unchanged suite; retain every isolated root:
MCT_FLEET_KEEP=1 cargo test --locked --offline -p mct-daemon \
  --test federation_fleet -- --nocapture --test-threads=1
MCT_RESTART_KEEP=1 MCT_FLEET_KEEP=1 cargo test --workspace --locked --offline \
  --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all --check && git diff --check
```

Logs: `/tmp/mct-r2-route-evidence/`; `baseline-18.log` retains the failure,
`fixed-01.log` through `fixed-20.log` retain the passing repeated suites.
SHA-256 receipts (local evidence, not portable release artifacts):

```text
6c1dd4ffa4eff56816048643c2f322b1c26a3249d4893974319727f36e675fe5  baseline-18.log
3b77747f1bfe1b7cdcd86122932e306082773b673c302f229f6f44a5af5dfb0d  regression-before.log
909583941ca130643e10cb644097386150115093069f3c922c0e1ba2a5c69cb1  regression-after.log
5d2d0598442bc934e9eed98070ce52270414d543043ac9963661b270bc75ee81  iroh-tests.log
795b8cfd89c2b776442cbe2592818d8015356ede9b8331ea57d714378a32d333  workspace-tests.log
39d7670e512614aff07c1f0814b76d67c240e95d428e63f7d8b2468f6a1e0388  /tmp/mct-fleet-uqS5AU/root/observations.jsonl
2041ce3cc1781f041ee03f9bad622b942726c0c40883904b9197bf6e90d5bff7  /tmp/mct-fleet-Yjj9VC/root/observations.jsonl
```

The reproduced receipt-overwrite race is fixed locally. Review/merge and formal
completion-inventory closure remain separate; H1 remains open for Linux
confinement and independent review.

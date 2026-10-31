# H2 — Peer origin and caller provenance

Local fix on `experiment/git-toy-confinement`, atop R2 checkpoint `39239f4`
(parent `e739001`, the separate R1 checkpoint). No push or main-branch change.

## Source trace and reproduced exploit

1. Both Iroh serving paths decode `MctCallProtocolRequest` and replace
   `received_over.endpoint_id`, ALPN and connection side with authenticated
   transport facts. The R2 receipt cache resolves the exact hello decision for
   that authenticated endpoint.
2. The shared kernel evaluator checks the admitted receipt and current binding,
   including endpoint, caller node, Vision, ALPN, policy revision, expiry,
   revocation and receiver authority. Before H2 it did **not** constrain
   `request.call.origin` or exclude an unbound caller user identity.
3. The resident callback supplied a transport-proven `Remote` payload, but
   `ResidentCallIngressContext::ordinary` ignored that provenance: it selected
   `Peer` or `LocalPrincipal` solely from the wire's origin.
4. Candidate sourcing uses `call.origin.allows_remote_candidate_sourcing()`.
   `Iroh` is terminal; local origins may select one remote executor. Thus a
   forged local origin could select a remote route and a local idempotency
   scope. The forwarding envelope legitimately replaces caller node with the
   forwarding Mother's node and clears upstream user identity; in this exploit
   that turned an unauthorized second hop into an admitted B -> C call.

The before-fix three-Mother regression used an adversarial Mother endpoint A
and **two full resident Mothers B and C**, with actual Iroh, signed peer
bindings, canonical ledgers, owner-only UDS ingress and real WASM children.
B admits A; C admits B, not A. B's local child exports `control.run`; C's
child exports `echo.echo`. B discovers C through its real background hello
refresh, not an injected route cache.

The legitimate A -> B local-child call and owner-authorized B -> C call both
succeeded. A terminal `Iroh` arrival targeting C was denied. Changing just the
origin to `Cli` then caused **B to forward and C to execute**, returning:

```text
call_id: call-h2-attack-Cli
reply_outcome: Success
route_taken.node_id: mother-c
route_taken.runtime_kind: RemotePeer
inline_result_payload: {"results":[41]}
```

This is an observed unauthorized extra hop, not only a source-level concern.
The independent transport regression also failed before the fix: a `Cli` wire
origin reached the handler and returned `Success` instead of `Denied`.

### Retained original failure

- Logs: `/tmp/mct-h2-origin-evidence/transport-before.log` and
  `three-mother-before.log`.
- Fixture: `/tmp/mct-h2-prCsmP/{b,c}/`, including original configs, state,
  identities and `observations.jsonl`. It is retained for local diagnostics;
  identity files are **not** committed or reproduced in this document.
- B sequence 48: `peer_call_sent`, `2026-10-25T02:49:09.890745Z`.
- C sequence 36: `runtime_execution_started`, `02:49:09.973684Z`;
  sequences 37–38 record completion; sequence 42 records the completed reply.
- B sequences 49 and 51 record completed downstream/upstream replies.
- All of those records carry `call-h2-attack-Cli`.

The failing test stops at the exploit assertion, so this fixture is not a clean
shutdown demonstration. Do not restart or repair it to reproduce the evidence.

## Boundary changes

### Transport/kernel

Both `serve_concurrent_with_binding_provider` and `serve_next` continue to
replace transport claims with the authenticated endpoint and to use the exact
endpoint-scoped hello receipt. Before any handler can run, the shared
`evaluate_call_protocol` now additionally requires:

- `call.origin == Iroh`; `Cli`, `JvmAdapter`, `WasmHost`, `ProcessHarness` and
  `TriggerFiring` are denied with typed internal reason `OriginMismatch`;
- `caller.user_id == None`; admission binds a Mother node/Vision, not a local
  or upstream user. Unbound user assertions receive `CallerMismatch`.

Caller node and Vision still must match **both** the admitted hello and the
current binding for the authenticated transport endpoint. Substituting an
upstream node, endpoint, Vision or binding cannot mint authority. Wire origins
are rejected rather than silently converted, preserving evidence of the
mismatch. Caller replies remain `Denied`, `not authorized`, no route/result and
no retry directive for these forgeries. Required durable denial observations
remain in place.

Project metadata remains permitted as narrowing request scope. It does not
change the admitted node identity, origin or peer idempotency budget. This patch
does not introduce user delegation, multi-hop delegation or a new authority
source.

### Resident execution and retry scope

- Replace the origin-only context constructor with `from_ingress(request,
  ingress)`. Transport-proven `Remote` ingress can construct **only** `Peer`,
  and only for an Iroh-origin, user-free request already checked by admission.
  Trusted local adapters continue to construct local principals from their own
  local facts. Trigger and Child call-out contexts remain explicit.
- Even explicitly supplied contexts cannot combine a `Remote` payload with a
  local, trigger or Child call-out context.
- The standalone Iroh idempotency entry point constructs only peer context.
  The common idempotency path checks context/request compatibility **before**
  reservation and before its no-key execution shortcut.
- Peer retry scope remains `peer-binding:<admitted-binding-id>`. Choosing a
  local origin, user or another binding cannot allocate a new local budget.

Candidate and forwarding laws are unchanged: accepted peer arrivals have only
local candidates; trusted local calls may choose one remote executor. A genuine
forwarding envelope still clears the upstream user and identifies the actual
forwarding Mother. Current binding/policy/grants revalidation remains intact.

## Regression coverage

Five new tests:

1. Kernel: every non-Iroh origin and unbound user is denied; legitimate Iroh
   requests, including narrowing project metadata, remain valid.
2. Real Iroh, **both sequential and concurrent servers**: every local origin,
   spoofed upstream node, user, Vision, binding and transport endpoint is denied
   without handler execution or retry instruction. Direct controls before and
   after the attacks succeed with the same admitted receipt.
3. Resident provenance: forged remote requests cannot mint local context, and
   even an explicitly supplied local context cannot accompany remote ingress.
   No state database is opened/created.
4. Idempotency: an admitted peer context cannot switch origin/user/binding,
   whether or not an idempotency key is present. No execution or new retry
   database occurs; narrowing project metadata retains the same binding scope.
5. Three Mothers: every forged origin and spoofed upstream node/user is denied
   against **both** B's local child and C's remote operation; a valid Iroh call
   targeting C also remains terminal at B. Ledgers prove no attack is forwarded
   by B, executed by B, or received by C. The local B -> C control and direct
   A -> B controls succeed.

The three-Mother test synchronizes on listener-owned readiness and actual
published-surface availability. No sleeps or retry/deadline inflation were
introduced. Tests run on one host without relays; this is not a WAN or
three-process crash/partition claim. `MCT_ORIGIN_KEEP=1` retains test roots.

## Validation

macOS arm64; locked/offline dependencies. Environment:

```sh
export CARGO_BUILD_JOBS=8 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_NET_OFFLINE=true
MCT_ORIGIN_KEEP=1 MCT_RESTART_KEEP=1 MCT_FLEET_KEEP=1 \
  cargo test --workspace --locked --offline --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all --check
git diff --check
bash scripts/ci-tier0.sh specs
```

- Workspace: **763 passed, 0 failed, 2 pre-existing ignored**, including the
  fleet and SIGKILL restart tests. Kernel: 173; Iroh: 48; daemon binary: 189.
- Five additional consecutive three-Mother runs: **5/5 passed**, fixtures retained.
- Workspace/all-target Clippy, fmt, diff checks and tier-0 specs: passed.
- Evidence directory: `/tmp/mct-h2-origin-evidence/`. The first post-fix
  three-Mother log predates the additional local-target assertions; the full
  workspace and five repeated runs cover the final test.

Selected SHA-256 receipts:

```text
07cd1dc2a8bfd2b82ef8beb48b41fd7ed589e9d02e1e59d3d8a722adca85a290  transport-before.log
0f9572fba77cca744cc1e8044521491b7c4698e480d811a823d7e9e2a05c4232  three-mother-before.log
f6ad7a8ab1d26febdab4ab8fd489d883f43c0e972272a72ff1f3c9c1e09a3b5d  workspace-tests.log
3910a3d83998f30e64f15100c0eddf38828169ef2c0a11839b7f5a2d3d7cef4e  /tmp/mct-h2-prCsmP/b/observations.jsonl
e1c823b93d1ea7fa8a2a61db4e4904ceb54a5a9c6e94296d688b50e525570563  /tmp/mct-h2-prCsmP/c/observations.jsonl
```

This is a local H2 fix and adversarial proof, not independent security review
or formal completion-inventory closure. H1's Linux confinement/review work and
H3's resource-budget work remain separate and open.

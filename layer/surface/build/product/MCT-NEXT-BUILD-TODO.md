# MCT next build TODO

Goal: build the next operational layer after the v0 `patinaMother` replacement boundary. MCT becomes the complete Mother/Child/Toy product; belief/scry/assay/spec/session meaning remains Patina application behavior rather than `mctMother` internals.

## Execution order

1. [x] Multi-Mother first. Completed 2026-07-09 as single-hop route forwarding; multi-Vision and transitive routing remain ROADMAP item 6 follow-ons.
2. [x] Before Multi-Mother implementation, audit `patinaMother` routing and record translate/rebuild/replace decisions. See audit snapshot below.
3. [ ] During/after Multi-Mother, design storage/network `mctToy` contracts from the `patinaToy` capability audit.
4. [ ] JVM SDK targeting the chosen resident ingress: synchronous JSON `POST /calls` over the owner-authenticated UDS control socket. The SDK must use UUID idempotency keys by default because `(node, UID, Vision)` scope is shared by same-user applications.
5. [x] macOS user-launchd supervisor install/start/stop/restart/uninstall completed 2026-07-15 under `layer/surface/build/feat/supervisor-lifecycle/SPEC.md`. The daily-operation pain clock that started 2026-07-14 is closed. Linux systemd and non-GUI/headless launchd domains remain separately gated future adapters.
6. [x] Acquisition-backed immutable artifact staging completed 2026-07-16 under `layer/surface/build/feat/artifact-acquisition/SPEC.md`, including the real supervised `slate-manager@0.2.0` daily-driver proof.
7. [ ] Resume the paused `mct-release-hardening` and `mct-interface-launcher-control` epics as the final gate. Replacement of `patinaMother` cannot be claimed while they are paused; they are deferred, not dropped.

## Completed: Multi-Mother — single-hop forwarding

Multi-Mother means one `mctMother` can safely call another Mother in MCT.

This unlocks:

- routing work across machines/projects;
- one Mother exposing approved children to another;
- remote child execution;
- distributed observations;
- Vision-scoped publication;
- eventually cluster-like behavior.

The hard part is authority. A remote Mother is not trusted just because it connects.

### Translate/rebuild/replace decisions from the `patinaMother` routing audit

- [x] Translate and rebuild: UDS-first local control, explicit child call surfaces, health/status readiness, and named bridges where they are useful.
- [x] Translate and rebuild: source/broker fail-closed authentication resolution and domain-scoped connection use.
- [x] Replace: `patinaMother` HTTP `/child/{child}/{action}` as the trust model for cross-Mother runtime calls.
- [x] Replace: `patinaMother` graph/federation knowledge routing as a substitute for runtime route authority.
- [x] Replace: `patinaToy` local native-job `patina:peer/peer` enqueue as the remote execution path.
- [x] Build: remote `CandidateRoute` generation from admitted signed peers.
  - Executable `RuntimeKind::RemotePeer` candidates now require fresh scoped publication plus complete bidirectional binding authority and are revalidated before forwarding.

Build order — each step depends on the one before it:

- [x] 1. Scoped publication of callable surfaces from one Mother to another.
  - [x] Local federation capability view publishes Vision-scoped callable child operations (`mct-daemon federation view`).
  - [x] Send the typed view across admitted hello request/response exchange.
  - [x] Receive, atomically store, and refresh remote surfaces as expiring runtime evidence.
- [x] 2. Route-forward execution over Iroh `mct/hello/0` + `mct/call/0`: an originating Mother selects a published executor and maps its verified reply into a local typed result.
- [x] 3. Route-chain observations on both Mothers: forwarded-from on the originator, executed-on on the executor, and typed denial records reconstruct the authority chain.
- [x] 4. End-to-end two-Mother failure tests: wrong Vision, revoked/expired binding, bad payload, unauthorized operation, remote denial, and mutual-publication/unready termination.
  - Forwarded `mct/call/0` arrivals are terminal and cannot source another remote candidate.

### Authority requirements

- [x] Signed peer binding proof is required for remote Mother admission.
  - Ed25519 verification of `signature_ref` is enforced in hello admission and in remote candidate evaluation; missing, malformed, or invalid proofs fail closed.
- [x] Allowed ALPNs are scoped by binding/policy at hello admission and candidate evaluation.
- [x] Remote operations are scoped by fresh published callable surfaces.
- [x] Vision limits are enforced before routing (hello admission and candidate elimination).
- [x] Route forwarding rules are explicit, observable, and single-hop by invariant.
- [x] Request and result payload integrity are verified end to end.
- [x] Remote failures map to safe, typed outcomes.
- [x] Observations distinguish local execution, forwarded execution, and remote denial.

### Acceptance sketch

- [x] Mother A can publish a scoped callable surface to Mother B.
- [x] Mother B can route an authorized locally originated call to Mother A and receive a verified result.
- [x] Unauthorized, wrong-Vision, wrong-operation, expired-binding, bad-payload, and remote no-route paths fail closed.
- [x] Both Mothers record enough observations to reconstruct the route and authority chain without leaking payload bytes or secrets.

## Completed: Resident Operational Loop — Slice 1

Completed 2026-07-14 under `layer/surface/build/feat/resident-call-ingress/SPEC.md`:

- owner-only (`0600`) UDS control socket with Unix peer-UID authentication for local application calls;
- synchronous JSON `POST /calls`, canonical `CallOrigin::JvmAdapter` bridge lineage, and resident node/UID/Vision caller scope;
- unchanged inline/CAS size and BLAKE3 integrity laws, with payload bytes excluded from observations and control reads;
- durable caller-scoped idempotency and submission/result observations before response;
- calls execute through the existing authority, route revalidation, child execution, result, run-state, and ledger pipeline;
- concurrent control dispatch keeps reads available while administrative mutations remain serialized;
- `mct-daemon serve` defaults to `.mct/control.sock`; explicit HTTP mode remains read-only and has no `/calls` ingress;
- `mct-daemon status [--uds path] [--json]` now reports real resident readiness, node/Vision/Iroh identity, loaded/approved/ready counts, and last observation sequence, and fails honestly when no resident is reachable.

This is the production transport target for TODO item 4. The one-shot `mct-daemon jvm call-json` bridge remains compatibility/development evidence, not the resident operational boundary.

## Next: storage/network toy contracts

Use the audit below to translate useful `patinaToy` use cases into newly designed, ToyGrant-backed `mctToy` contracts. Parallel names do not imply equivalent authority or reusable adapter shapes.

- [ ] Directory storage toy: read-only scoped directory.
- [ ] Directory storage toy: read-write scoped directory or write-only output directory.
- [ ] Blob/artifact storage toy: digest-addressed fetch/store.
- [ ] Network toy: HTTPS only, domain allowlist, method allowlist, size limits, timeout limits.
- [ ] Network toy: credential/secret-ref attachment through the secrets authority, never raw child env.
- [ ] Observations: record safe metadata only; no payload bytes, credentials, or secret values.

## Completed: macOS supervisor lifecycle — Daily-Driver Slice 2

Completed 2026-07-15 under `layer/surface/build/feat/supervisor-lifecycle/SPEC.md`:

- [x] Add top-level `mct-daemon install`, `uninstall`, `start`, `stop`, and `restart`.
- [x] Install one owner-private, ledger-backed, digest-bound user-launchd record and plist in `gui/<uid>`.
- [x] Observe fresh bootstrap, install/revision/removal, supervised boot, clean shutdown, unclean reconciliation, conflict refusal, and no-op lifecycle attempts with existing observation kinds.
- [x] Derive boot initiator provenance from the exact installed supervisor record without fabricating boot-time authentication.
- [x] Refuse managed/manual/duplicate coexistence before endpoint or launchd effects.
- [x] Fence readiness and protected effects after known observation-writer loss; do not cache an in-progress result whose outcome cannot be observed.
- [x] Preserve ledger, state, identity, children/artifacts, authority state, and logs on uninstall.
- [x] Keep daemon service supervision separate from child process supervision.
- [ ] Add Linux systemd user supervision through the same adapter contract in a separately gated slice.
- [ ] Decide macOS headless/SSH-only LaunchDaemon or non-GUI domain semantics through a separate operator gate.

Executable digest binding is intentionally strict: replacing binary bytes requires `install --replace`. Until revised, launchd may throttle-loop on the failed-closed digest mismatch; the replacement runbook documents diagnosis and remediation.

## Completed: artifact acquisition — Daily-Driver Slice 3

Completed 2026-07-16 under `layer/surface/build/feat/artifact-acquisition/SPEC.md`:

- [x] Add explicit `artifacts stage|acquire|show|acquisitions|sources` commands and resident UDS mutations.
- [x] Require exactly one bounded source-trust path plus a fresh one-attempt filesystem effect capability.
- [x] Verify SHA-256 sidecars as the package floor and record independent BLAKE3 acquisition evidence.
- [x] Publish immutable packages under `.mct/children/artifacts/sha256/<component-digest>/` only after durable acquisition/verification facts.
- [x] Persist operator decisions, standing source records, attempts, packages, and acquisition-backed artifact provenance; migrate pre-law rows as `historical_unknown`.
- [x] Close `registry install/sync` as ambient mutation bypasses and require exact `children approve --artifact sha256:<digest>` evidence.
- [x] Prove real `slate-manager@0.2.0` acquisition, denial before approval/grants, ToyGrant-backed execution, revocation, restart denial, and uninstall preservation.
- [ ] Import and prove real `folder-watch-actor@0.1.0` and `watch-null-sink@0.1.0` fixtures before claiming the full three-fixture `patinaMother` replacement test.
- [ ] Add network acquisition only through a separately gated deny-by-default egress Toy plus independent connection/secret authority.

## JVM SDK

Chosen production ingress:

```text
POST /calls over .mct/control.sock
```

The endpoint is synchronous, authenticates the Unix peer UID, applies existing payload and idempotency laws, and returns the caller-safe terminal reply. `mct-daemon jvm call-json <operation-id> <args-json>` remains a one-shot compatibility/development bridge.

Build the production JVM SDK so Java/Kotlin callers can use MCT ergonomically:

```java
MctClient client = MctClient.connect(...);
client.call("patina:slate/control@0.1.0.list-work", args);
```

### SDK responsibilities

- [ ] Build MCT call envelopes from Java/Kotlin inputs.
- [ ] Connect to the owner-authenticated UDS `POST /calls` transport.
- [ ] Treat Unix peer UID as the v0 local caller authentication boundary; do not claim per-application identity.
- [ ] Apply deadlines/timeouts consistently.
- [ ] Use UUID idempotency keys by default and retry only where idempotency makes that safe.
- [ ] Decode MCT replies and result payloads into Java/Kotlin models.
- [ ] Expose typed Java/Kotlin request/result models for common WIT operations.
- [ ] Keep streaming observations as a later optional layer unless needed for the first SDK cut.

### Acceptance sketch

- [ ] A Java/Kotlin fixture can call an approved local child through MCT.
- [ ] Auth/admission failures return typed SDK errors with caller-safe messages.
- [ ] Timeout/result decoding behavior is covered by tests.
- [ ] SDK docs show the minimal connect/call workflow.

## Alignment questions before/while building

- [x] Compare how `patinaMother` handles cross-project/cross-Mother routing versus the `mctMother` signed-binding/route-forwarding model.
- [x] Audit how `patinaToy` handles storage and network capability boundaries, then translate those use cases into `mctToy` designs.
- [x] Decide which storage toys are first: directory scope, blob store, artifact store, database/keyspace, or write-only output area.
- [x] Decide which network toys are first: domain allowlist, method allowlist, size/time limits, secret-ref attachment, or observation policy.
- [x] Define supervisor wrapper scope separately from runtime authority: macOS launchd first, Linux systemd after, or both.

## Audit snapshot — 2026-07-09

### `patinaMother` routing

- `patinaMother` exposes a UDS-first/TCP-optional HTTP control plane and routes explicit `patinaChild` requests through `/child/{child}/{action}`.
- Built-in child routes exist for `spec-manager`, `lake-manager`, `doctor`, and `secrets-authority`.
- Spec dispatch can optionally execute through `slate-manager`, but this is a named child/backend bridge rather than a general authority-first route graph.
- Broker source routing moves configured source facts into project/lake stores; it is not generic remote child execution.
- Graph/federation are cross-project knowledge/query surfaces, not signed cross-Mother runtime forwarding.
- The `patina:peer/peer` toy enqueues local native jobs; it is not yet a signed remote-Mother authority path.

### `mctMother` routing result

- `mctMother` has route decision types for local/direct/relayed/remote candidates and signed Iroh hello/call admission.
- Resident routing builds local candidates and eligible single-hop remote peer candidates from current bilateral authority, publication, and reachability evidence.
- Forwarded arrivals remain terminal and cannot source another peer candidate.

### `patinaToy` storage/network capability shape

- `patinaChild` manifests declare `patinaToy` capabilities for keyvalue, filesystem, SQL, messaging/events, measure, Git, query, HTTP/connect, graph, and belief access.
- Many host functions have call-time checks: keyvalue, sql, messaging, events subscribe, measure, task, and git.
- Filesystem can preopen manifest-scoped paths; the daemon loader also preopens the current project read-write when the filesystem toy is enabled.
- `patinaToy` HTTP/connect uses domain matching for credential injection, but unmatched hosts—and requests without a host—fall through to default outgoing HTTP rather than deny. This is allow-by-default egress and must not be ported.
- MCT should translate useful use cases into explicit `mctToy` contracts with ToyGrant evaluation, deny-unmatched scopes, bounded payloads, redacted observations, and no ambient raw filesystem/network.

### Supervisor scope reference

- `patinaMother` has start/stop/restart/status/install/uninstall operations with launchd on macOS, systemd --user on Linux, PID/socket readiness checks, and manual-start conflict guards.
- `mctMother` currently has `mct-daemon serve`, SIGINT/SIGTERM shutdown, control status, and an internal child process supervisor.
- MCT still needs daemon service wrappers; keep these separate from child supervision and runtime authority.

# MCT next build TODO

Goal: build the next layer after v0 Mother runtime replacement. Keep MCT focused on runtime/orchestration; Patina keeps Belief/scry/assay/spec/session knowledge-product internals.

## Execution order

1. [ ] Multi-Mother first. This is the next major MCT power milestone.
2. [x] Before Multi-Mother implementation, audit existing Patina Mother routing and record preserve/replace decisions. See audit snapshot below.
3. [ ] During/after Multi-Mother, design storage/network toy contracts from the Patina Mother capability audit.
4. [ ] JVM SDK after/alongside the chosen Multi-Mother ingress shape, so the SDK targets the real transport instead of only the temporary CLI bridge.
5. [ ] Supervisor install/start/stop wrappers after runtime semantics are stable, unless local daily operation becomes painful sooner.

## Now: Multi-Mother

Multi-Mother means one MCT Mother can safely call another Mother.

This unlocks:

- routing work across machines/projects;
- one Mother exposing approved children to another;
- remote child execution;
- distributed observations;
- Vision-scoped publication;
- eventually cluster-like behavior.

The hard part is authority. A remote Mother is not trusted just because it connects.

### Preserve/replace decisions from Patina Mother routing audit

- [x] Preserve: UDS-first local control, explicit child call surfaces, health/status readiness, and named built-in bridges where they are useful.
- [x] Preserve: source/broker fail-closed auth resolution and domain-scoped connection thinking.
- [x] Replace: HTTP `/child/{child}/{action}` as the trust model for cross-Mother runtime calls.
- [x] Replace: graph/federation knowledge routing as a substitute for runtime route authority.
- [x] Replace: local native-job `patina:peer/peer` enqueue as the remote execution path.
- [x] Build: remote `CandidateRoute` generation from admitted signed peers.
  - Implemented as observed `RuntimeKind::RemotePeer` candidates; they remain fail-closed with `CapabilityUnavailable` until scoped publication and forwarding are implemented.
- [ ] Build: scoped publication of callable surfaces from one Mother to another.
  - [x] Local federation capability view publishes Vision-scoped callable child operations.
  - [ ] Carry/consume callable surface views across peer exchange so remote route authority can use them.
- [ ] Build: route-forward execution over Iroh `mct/hello/0` + `mct/call/0`.
- [ ] Build: route-chain observations on both Mothers.

### Authority requirements

- [ ] Signed peer binding proof is required for remote Mother admission.
- [ ] Allowed ALPNs/operations are scoped by binding/policy.
- [ ] Vision/project limits are enforced before routing or execution.
- [ ] Route forwarding rules are explicit and observable.
- [ ] Request and result payload integrity are verified end to end.
- [ ] Remote failures map to safe, typed outcomes.
- [ ] Observations distinguish local execution, forwarded execution, and remote denial.

### Acceptance sketch

- [ ] Mother A can publish a scoped callable surface to Mother B.
- [ ] Mother B can route an authorized call to Mother A and receive a verified result.
- [ ] Unauthorized, wrong-Vision, wrong-operation, expired-binding, and bad-payload paths fail closed.
- [ ] Both Mothers record enough observations to reconstruct the route and authority chain without leaking payload bytes or secrets.

## Next: storage/network toy contracts

Use the audit below to convert useful Patina Mother capability vocabulary into MCT ToyGrant-backed contracts.

- [ ] Directory storage toy: read-only scoped directory.
- [ ] Directory storage toy: read-write scoped directory or write-only output directory.
- [ ] Blob/artifact storage toy: digest-addressed fetch/store.
- [ ] Network toy: HTTPS only, domain allowlist, method allowlist, size limits, timeout limits.
- [ ] Network toy: credential/secret-ref attachment through the secrets authority, never raw child env.
- [ ] Observations: record safe metadata only; no payload bytes, credentials, or secret values.

## Later: supervisor wrappers

- [ ] Add `mct-daemon install` / `uninstall` or an `mct` wrapper for service installation.
- [ ] macOS launchd user service first, matching current platform.
- [ ] Linux systemd --user after or alongside launchd.
- [ ] Include start/stop/restart/status/logs/readiness and manual-start conflict guards.
- [ ] Keep daemon service supervision separate from child process supervision.

## JVM SDK

Current bridge:

```bash
mct-daemon jvm call-json <operation-id> <args-json>
```

This proves a JVM system can submit work into MCT using JSON and receive a caller-safe reply.

Build the production JVM SDK so Java/Kotlin callers can use MCT ergonomically:

```java
MctClient client = MctClient.connect(...);
client.call("patina:slate/control@0.1.0.list-work", args);
```

### SDK responsibilities

- [ ] Build MCT call envelopes from Java/Kotlin inputs.
- [ ] Connect to the chosen MCT ingress transport.
- [ ] Sign or present identity/binding proofs when needed.
- [ ] Apply deadlines/timeouts consistently.
- [ ] Handle retries only where idempotency makes that safe.
- [ ] Decode MCT replies and result payloads into Java/Kotlin models.
- [ ] Expose typed Java/Kotlin request/result models for common WIT operations.
- [ ] Keep streaming observations as a later optional layer unless needed for the first SDK cut.

### Acceptance sketch

- [ ] A Java/Kotlin fixture can call an approved local child through MCT.
- [ ] Auth/admission failures return typed SDK errors with caller-safe messages.
- [ ] Timeout/result decoding behavior is covered by tests.
- [ ] SDK docs show the minimal connect/call workflow.

## Alignment questions before/while building

- [x] Compare how existing Patina Mother handles cross-project/cross-Mother routing today versus the MCT signed-binding/route-forwarding model.
- [x] Audit how existing Patina Mother handles storage and network capability boundaries, then map that into MCT toys.
- [x] Decide which storage toys are first: directory scope, blob store, artifact store, database/keyspace, or write-only output area.
- [x] Decide which network toys are first: domain allowlist, method allowlist, size/time limits, secret-ref attachment, or observation policy.
- [x] Define supervisor wrapper scope separately from runtime authority: macOS launchd first, Linux systemd after, or both.

## Audit snapshot — 2026-07-09

### Existing Patina Mother routing

- Current Patina Mother exposes a UDS-first/TCP-optional HTTP control plane and routes explicit child requests through `/child/{child}/{action}`.
- Built-in child routes exist for `spec-manager`, `lake-manager`, `doctor`, and `secrets-authority`.
- Spec dispatch can optionally execute through `slate-manager`, but this is a named child/backend bridge rather than a general authority-first route graph.
- Broker source routing moves configured source facts into project/lake stores; it is not generic remote child execution.
- Graph/federation are cross-project knowledge/query surfaces, not signed cross-Mother runtime forwarding.
- The `patina:peer/peer` toy enqueues local native jobs; it is not yet a signed remote-Mother authority path.

### MCT routing target

- MCT already has route decision types for local/direct/relayed/remote candidates and signed Iroh hello/call admission.
- Resident routing currently builds local child candidates only.
- Multi-Mother should add remote peer route candidates, explicit publish/subscribe surfaces, and Iroh call forwarding with route-chain observations.

### Existing Patina Mother storage/network capability shape

- Patina child manifests declare toys/capabilities for keyvalue, filesystem, sql, messaging/events, measure, git, query, http/connect, graph, and belief access.
- Many host functions have call-time checks: keyvalue, sql, messaging, events subscribe, measure, task, and git.
- Filesystem can preopen manifest-scoped paths; the daemon loader also preopens the current project read-write when the filesystem toy is enabled.
- HTTP/connect has an allowed-domain and credential-injection model, but the WASI outgoing HTTP handler should be treated as legacy behavior to audit before copying because unmatched hosts appear to fall through without credential injection rather than becoming an MCT ToyGrant denial.
- MCT should preserve the useful manifest vocabulary but implement storage/network as explicit toys with ToyGrant evaluation, bounded payloads, redacted observations, and no ambient raw filesystem/network.

### Supervisor scope reference

- Existing Patina has `mother start/stop/restart/status/install/uninstall` with launchd on macOS, systemd --user on Linux, PID/socket readiness checks, and manual-start conflict guards.
- MCT currently has `mct-daemon serve`, SIGINT/SIGTERM shutdown, control status, and an internal child process supervisor.
- MCT still needs daemon service wrappers; keep these separate from child supervision and runtime authority.

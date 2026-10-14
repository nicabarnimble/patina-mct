# MCT next build TODO

Goal: build the next operational layer after the v0 `patinaMother` replacement boundary. MCT becomes the complete Mother/Child/Toy product; belief/scry/assay/spec/session meaning remains Patina application behavior rather than `mctMother` internals.

## Execution order

1. [x] Single-hop route forwarding landed 2026-07-09. Multi-Mother with per-Mother ledgers is future scale work, not planned now.
2. [x] Audit `patinaMother` routing and record translate/rebuild/replace decisions. See audit snapshot below.
3. [ ] Design storage/network `mctToy` contracts from the `patinaToy` capability audit.
4. [ ] JVM SDK on the iroh call path. A Child call presents a platform-given handle. Administration presents a granted iroh admin key. Same-uid is not caller identity.
5. [x] macOS user-launchd supervisor install/start/stop/restart/uninstall completed 2026-07-15 under `layer/surface/build/feat/supervisor-lifecycle/SPEC.md`. The daily-operation pain clock that started 2026-07-14 is closed. Linux systemd and non-GUI/headless launchd domains remain separately gated future adapters.
6. [x] Acquisition-backed immutable artifact staging completed 2026-07-16 under `layer/surface/build/feat/artifact-acquisition/SPEC.md`, including the real supervised `slate-manager@0.2.0` daily-driver proof.
7. [x] Durable temporal triggering and scoped Watch delivery completed 2026-07-21 under the Replacement Slice 4A/4B SPECs, including the source-derived watcher, exact sink, and supervised three-fixture proof.
8. [ ] Adjudicate the final claims separately. The supervised three-fixture suite proves the v0 `patinaMother` **runtime responsibility replacement** boundary. Operational shutoff of `patinaMother` remains gated by an operator-ratified inventory of required Patina/interface/`patina ai` behavior and either a replacement application layer or explicit accepted gaps. The paused `mct-interface-launcher-control` epic remains follow-on interface-layer work above MCT and does not negate the landed runtime proof.
   - [x] R1 release/security review landed in `RELEASE-REVIEW-R1.md`, preserving runtime replacement, operational shutoff, and into-the-wild publication as three separate claims.
   - [x] R2 closed the six reviewed security findings: UDS authentication/read bounds, parent-directory fsync, ledger-correlated standing-source proof, terminal recovery-excess accounting, dependency/audit policy, and raw Git stderr suppression.
   - [x] R3 release discipline landed under `feat/release-discipline/SPEC.md`: unified 0.2.0 versioning, closed signed package/SBOM/provenance, daemon-specific immutable release evidence, operator-file-only acquisition, exact-approved shared lifecycle upgrade, real packaged smoke, and the committed `BASELINES-v0.2.0-aarch64-apple-darwin.md` record.
   - [x] Post-R3 credibility slice landed 2026-07-23: `ASSURANCE.md`/`SECURITY.md` evidence docs with GitHub private vulnerability reporting (`bec8434`), Tier-0 python3>=3.11 dependency removal/guard (`70930dd`), digest-pinned OpenSSF Scorecard workflow (`b4bd807`), and hostile-input fuzz coverage: a standalone cargo-fuzz workspace with `uds_control_request`, `release_archive`, `child_package_manifest`, and `pando_manifest` targets over committed corpora, plus the bounded digest-pinned `fuzz.yml` CI smoke (`79b009f`, `a958b84`, `83c0c99`, `163d6d1`, `fe74ebf`, `b47be97`). Map-tend waiver: no product-map change — the fuzz targets are test evidence over existing landed parse contracts and add no new invariants or authority surfaces.
      - [x] After operator-approved push/merge and the first default-branch Scorecard workflow run, record the initial observed score in `feat/post-r3-credibility/CLOSEOUT.md`: **5.1** at `2026-07-25T02:02:28Z`.
   - [ ] R5/into-the-wild work remains separate: exact publication tag/release, update-channel authority, network acquisition, notarization activation, and 1.0.0 GA are not implied by R3.

## Completed: single-hop forwarding

A 2026-07-09 slice forwards one authorized call to one admitted peer. Multi-Mother with per-Mother ledgers is future scale work, not planned now. A remote node is not trusted just because it connects.

### Translate/rebuild/replace decisions from the `patinaMother` routing audit

- [x] Translate and rebuild: explicit child call surfaces, health/status readiness, and named bridges where they are useful. Administration is a granted iroh admin key. A Unix socket and same-uid are not the authorization path.
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

## Resident call path

A Child call is a platform-given handle on the iroh path. Holding the handle is the authorization. Administration presents a granted iroh admin key. The Unix socket, peer credentials, and offline commands are not an authorization path.

Inline and content-addressed payloads keep their size and BLAKE3 checks, and payload bytes stay out of observations. Calls still run through authority, route revalidation, child execution, and the ledger. `mct-daemon jvm call-json` remains a one-shot compatibility bridge, not the resident boundary.

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

- [x] Add explicit `artifacts stage|acquire|show|acquisitions|sources` commands. Administration presents a granted iroh admin key.
- [x] Require exactly one bounded source-trust path plus a fresh one-attempt filesystem effect capability.
- [x] Verify SHA-256 sidecars as the package floor and record independent BLAKE3 acquisition evidence.
- [x] Publish immutable packages under `.mct/children/artifacts/sha256/<component-digest>/` only after durable acquisition/verification facts.
- [x] Persist operator decisions, standing source records, attempts, packages, and acquisition-backed artifact provenance; migrate pre-law rows as `historical_unknown`.
- [x] Close `registry install/sync` as ambient mutation bypasses and require exact `children approve --artifact sha256:<digest>` evidence.
- [x] Prove real `slate-manager@0.2.0` acquisition, denial before approval/grants, ToyGrant-backed execution, revocation, restart denial, and uninstall preservation.
- [x] Import and prove source-derived `folder-watch-actor@0.1.0` and exact unmodified `watch-null-sink@0.1.0` fixtures in the full supervised three-fixture `patinaMother` replacement test.
- [ ] Add network acquisition only through a separately gated deny-by-default egress Toy plus independent connection/secret authority.

## Completed: temporal trigger authority and scheduler — Replacement Slice 4A

Completed 2026-07-21 under `layer/surface/build/feat/trigger-event-runtime/SPEC.md`:

- [x] Add truthful local-only `CallOrigin::TriggerFiring` and deterministic record/revision/occurrence-scoped identities.
- [x] Persist revisioned trigger authority and schema-v8 occurrence, pending, firing, result, and reconciliation projections.
- [x] Implement explicit missed-fire and overlap matrices in fixed admission order under three named capacity limits.
- [x] Make authority, pending admission, and firing durable before activation/effect; reconstruct projections from the validated ledger on restart.
- [x] Prove acquired/approved WIT execution, idempotent same-occurrence evaluation, overlap crash recovery, revocation suppression, terminal non-resurrection, and ordinary-call fairness.
- [x] Keep `MotherEventSourceAdapterRuntime`, `RegistrySyncTriggerComposition`, and `NetworkArtifactAcquisitionAdapter` closed and add no `ObservationKind`.
- [x] Complete Replacement Slice 4B Watch authority, child call-out, legacy ABI narrowing, and source-provenance fixture proof.

## Completed: scoped Watch delivery and fixtures — Replacement Slice 4B

Completed 2026-07-21 under `layer/surface/build/feat/watch-event-fixtures/SPEC.md`:

- [x] Require current exact Watch scope and Toy authority, with independent directory-read, keyvalue, logging, and measure grants.
- [x] Validate event shape, class, safe path, batch capacity, and exact legacy 0.1.x path equality synchronously at `producer.send`.
- [x] Normalize the admitted set and append batch/event/disposition evidence before any ordinary `WasmHost` nested call.
- [x] Persist deterministic batch/event/disposition/delivery projections and reconstruct them without duplicate delivery after restart.
- [x] Build the watcher from the exact quarry source plus reviewed security patch; build the sink unmodified from its exact tag.
- [x] Prove supervised Slate acquisition plus temporal watcher invocation plus exact sink delivery, revocation, restart, and reopened evidence.
- [x] Keep `MotherEventSourceAdapterRuntime`, `RegistrySyncTriggerComposition`, and `NetworkArtifactAcquisitionAdapter` closed and add no `ObservationKind`.

The three-fixture runtime slice and R3 release-hardening mechanics are complete. Operational `patinaMother` shutoff and into-the-wild GA remain unclaimed until their separate inventory/interface and R5 publication gates close.

## JVM SDK

A Child call is a platform-given handle on the iroh path. Administration presents a granted iroh admin key. Same-uid is not the caller. `mct-daemon jvm call-json <operation-id> <args-json>` remains a one-shot compatibility bridge.

Build the production JVM SDK so Java/Kotlin callers can use MCT ergonomically:

```java
MctClient client = MctClient.connect(...);
client.call("patina:slate/control@0.1.0.list-work", args);
```

### SDK responsibilities

- [ ] Build MCT call envelopes from Java/Kotlin inputs.
- [ ] Present the handle on the iroh call path. Unix peer UID is not caller identity.
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

# MCT Build Roadmap

Slate: `mct-build-roadmap`  
Status: complete

## Purpose

This roadmap prevents the clean `patina-mct` build from collapsing into one oversized `mct-kernel` effort.

The build is split into small, reviewable Slate lanes that map to the user's goals:

- MCT first; SDK and Belief later.
- Greenfield build; integrated Patina is reference material only.
- Clean Mother/Child/Toy infrastructure, not a copy of current Mother coupling.
- Real Iroh v0 substrate, not a placeholder.
- Stable child call envelope before runtime adapters multiply.
- Explicit toy grants and authority observations.
- Concrete, narrow, performant core storage.
- JVM bridge and launcher as first-class MCT surfaces, not kernel internals.

## Runtime shape

MCT is shaped as a modular local authority runtime:

```text
mct-daemon
  owns process/control/endpoint/runtime lifecycle
    ↓
mct-kernel
  decides over typed MCT domain facts
    ↓
mct-observation
  records canonical runtime truth
    ↓
adapters
  perform Iroh/storage/WASM/process/JVM/toy/observability effects
```

Design influences:

- **Jon Gjengset-style Rust inside**: honest signatures, domain types at boundaries, private internals, explicit state machines, typed fail-closed outcomes, concrete implementations before speculative traits.
- **Iroh-style composable protocols outside**: endpoint lifecycle owned by the application, ALPN as the protocol seam, explicit async connection/stream handling, and application authority above transport identity.
- **Current Patina Mother as evidence, not ontology**: emulate Mother's role as local authority over children, toys, calls, and observations without copying its coupled runtime shape.

Allium anchor: `MctRuntimeShape`.

## Current Mother daemon baseline

Observed current runtime:

```text
Mother daemon: running
Socket: /Users/nicabar/.patina/run/serve.sock
Supervisor: launchd
Version: 0.71.0
Startup profile: full
Children loaded: 0
Registered projects: 155
Control plane ready: true
Project child manifest: missing .patina/manifest.toml
```

Integrated Mother shape used as reference:

| Current Mother area | Reference path | Clean MCT Slate lane |
|---|---|---|
| CLI entry and daemon commands | `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/src/commands/mother/mod.rs` | `mct-cli-config-packaging`, `mct-daemon-control-plane` |
| Broad daemon state | `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/src/commands/mother/daemon.rs` | `mct-daemon-control-plane`, `mct-kernel-crate`, `mct-storage-core` |
| UDS/TCP HTTP transport | `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/mother/src/http_daemon.rs` | `mct-daemon-control-plane` |
| Broad `ApiRuntime` | `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/mother/src/http_api.rs` | split across kernel, daemon, inspector, bridge, launcher lanes |
| Child trait/calls | `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/mother/src/runtime.rs` | `mct-call-envelope`, `mct-child-registry-lifecycle` |
| Child registry and typed-call history | `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/mother/src/registry.rs` | `mct-child-registry-lifecycle`, `mct-observation-log`, `mct-inspector-observability` |
| Concrete SQLite store | `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/mother/src/state/mod.rs` | `mct-storage-core` |
| Child package registry | `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/mother/src/child_registry/` | `mct-child-registry-lifecycle` |
| Secrets authority | `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/mother/src/secrets_authority_backend/` | `mct-secrets-authority` |
| Pando | `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/mother/src/pando.rs` | `mct-pando-manifest` |
| View buffers / display | `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/mother/src/view_buffer/` | `mct-inspector-observability` later, not kernel |
| Launcher/control | `/Users/nicabar/Projects/Sandbox/AI/RUST/patina/src/commands/ai/` | `mct-interface-launcher-control` |
| Belief/scry/federation builtins | multiple integrated paths | deferred/bridged; not MCT kernel |
| Iroh | no integrated daemon code path found | `mct-iroh-substrate` new first-class lane |

## Slate lanes

### 0. Planning and specification

1. `mct-build-roadmap` — top-level split, dependency plan, Mother comparison.
2. `mct-allium-foundation` — first behavior specs/anchors for authority, calls, lifecycle, grants, peer calls, observations.
3. `mother-kernel-design` — design precursor for the `mct-kernel` crate.

### 1. Kernel semantics

4. `mct-call-envelope` — lock and implement semantic envelope v1.
5. `mct-kernel-crate` — typed authority core for lifecycle/routing/grants/errors/observations.
6. `mct-observation-log` — kernel decisions plus adapter diagnostics, Temporal/Terraform-style split.
7. `mct-storage-core` — concrete narrow private storage, not current broad `MotherRuntimeStore`.
8. `mct-node-identity-security` — node/caller/peer/component auth and admission policy.

### 2. Daemon and network substrate

9. `mct-daemon-control-plane` — local daemon shell, health/version/readiness, kernel wiring.
10. `mct-iroh-substrate` — production-shaped Iroh v0: identity, endpoint lifecycle, relay/discovery, peer admission, ALPN, direct streams, observations, multi-node tests.
11. `mct-cli-config-packaging` — executable naming, config layout, lifecycle commands, packaging.

### 3. Child/Toy runtime surfaces

12. `mct-child-registry-lifecycle` — packages, installs, assignment, activation, reload, routing.
13. `mct-process-harness` — simple executable child runtime proof path.
14. `mct-toy-grants` — explicit toy/capability authority model.
15. `mct-secrets-authority` — controlled secrets for children/bridges/launchers/toys.
16. `mct-jvm-bridge` — banking JVM ingress path into MCT envelope/toy world.
17. `mct-wasm-component-runtime` — WASM/WIT/WASI child runtime after seams are clear.

### 4. Product/runtime surfaces

18. `mct-pando-manifest` — manifest/app composition foundation.
19. `mct-inspector-observability` — call history, decisions, diagnostics, later view buffers.
20. `mct-interface-launcher-control` — MCT-owned launcher/control behavior currently surfaced by `patina ai`.
21. `mct-release-hardening` — integration tests, performance, security, packaging, docs.

## Dependency order

```text
mct-build-roadmap
  -> mct-allium-foundation
  -> mother-kernel-design
  -> mct-call-envelope
  -> mct-kernel-crate
      -> mct-observation-log
      -> mct-storage-core
      -> mct-node-identity-security
      -> mct-daemon-control-plane
          -> mct-iroh-substrate
          -> mct-child-registry-lifecycle
              -> mct-process-harness
              -> mct-toy-grants
                  -> mct-secrets-authority
                  -> mct-wasm-component-runtime
              -> mct-pando-manifest
          -> mct-jvm-bridge
          -> mct-inspector-observability
          -> mct-interface-launcher-control
          -> mct-cli-config-packaging
              -> mct-release-hardening
```

This graph is intentionally conservative. Study/design can happen earlier, but implementation slates are blocked until their authority and envelope dependencies are settled.

## Honest implementation tracking

The broad build lanes above are epic containers, not proof that implementation is complete. They should not be implemented or closed in one large change.

Implementation progress is tracked by smaller child slice Slates with:

- one concrete behavior or artifact,
- named code targets,
- explicit non-goals,
- binary proof gates,
- commit evidence,
- validation commands.

Completed implementation slice Slates now recorded:

| Slice Slate | Evidence |
|---|---|
| `mct-slice-workspace-skeleton` | `cd6244d build: add rust workspace skeleton` |
| `mct-slice-kernel-domain-records` | `b3512ac feat(kernel): add mct domain records` |
| `mct-slice-hello-admission` | `62a88a9 feat(kernel): evaluate hello peer admission` |
| `mct-slice-jsonl-observation-ledger` | `b7f9246 feat(observation): add append-only jsonl ledger` |
| `mct-slice-call-protocol-evaluation` | `103cfab feat(kernel): evaluate mct call protocol` |
| `mct-slice-fake-daemon-echo` | `d826098 feat(daemon): add fake echo vertical slice` |
| `mct-slice-local-iroh-roundtrip` | `a35b598 feat(iroh): prove local mct protocol roundtrip` |
| `mct-slice-runtime-shape-anchors` | `5c88f8b spec: codify mct runtime shape` plus Slate tightening |
| `mct-slice-daemon-child-directory-loader` | current session: `mct-daemon children load` loads standalone `.wasm` + `.toml` child artifact directories |
| `mct-slice-iroh-ticket-peer-connection` | current session: endpoint-ticket Iroh serve/call connects two local MCT identities |
| `mct-slice-toy-grant-authorized-call` | current session: kernel ToyGrant evaluation produces AuthorizedToyCall and fail-closed denial observations |
| `mct-slice-kernel-child-lifecycle-state-machine` | current session: kernel child artifact/approval/assignment/instance lifecycle and authorized invocation revalidation |
| `mct-slice-daemon-child-authority-projection` | current session: daemon projects loaded children into explicit local approvals/assignments before local candidates |
| `mct-slice-process-child-authorized-echo` | current session: one-shot process child adapter requires AuthorizedChildInvocation and returns runtime observations |
| `mct-slice-toy-adapter-authorized-call` | current session: daemon toy adapter registry requires AuthorizedToyCall and records toy success/failure observations |
| `mct-slice-daemon-durable-config-approval-store` | current session: `.mct/config.json` persists child approvals/assignments and peer address book with operator CLI workflows |
| `mct-slice-process-supervisor-lifecycle` | current session: long-lived process supervisor spawn/status/stop for authorized child instances |
| `mct-slice-wasm-component-authorized-s32` | current session: real Wasmtime component-model execution of an authorized lifted `s32` export |
| `mct-slice-iroh-serve-process-runtime-routing` | current session: Iroh serve call handler routes accepted peer calls through process runtime before reply |

A parent epic becomes complete only when its child `slice_refs` and `closure_evidence` cover every proof gate with committed code and passing validation.

## Alignment judgement

### Strong alignment

- Build is split into MCT-native lanes rather than migration/copy lanes.
- Iroh is explicit and production-shaped early.
- JVM bridge and launcher are first-class MCT surfaces.
- Belief, scry, assay, oxidize, scrape, SDK, and Clojure are not first-pass MCT kernel work.
- Core performance is protected by typed in-memory decisions and private concrete storage.

### Known risks

- Too many broad slates can become process theater if they are not split into concrete implementation slices.
- `kernel` terminology may still be too heavy; revisit `mct-kernel` vs `mct-core` before crate creation.
- Iroh scope needs one focused study pass against cached `n0-computer/iroh` and companion protocol repos.
- Future Allium changes should use focused slice Slates rather than reopening broad foundation work by default.

## Immediate next actions

1. Pick the next child slice Slate rather than implementing a broad epic directly.
2. Finish or retire `mother-kernel-design` as a design precursor now that concrete kernel slices exist.
3. Create the next ready slice for daemon health/readiness, unknown Iroh peer denial, or call-envelope JSON edge conversion.
4. Keep parent epic statuses honest: partial evidence belongs in `partial_evidence`; completion requires child slice proof.

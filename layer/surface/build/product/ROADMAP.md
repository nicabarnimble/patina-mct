# MCT product roadmap — from runtime kernel to working product

Status assessment: reconciled against `main` at `4330876` after PR #71.
This document records where MCT stands as a product and the ordered TODO to
close the gap. Update the checkboxes as phases land; each phase gets its own
task file under `layer/surface/build/feat/<name>/` following the proven
PHASE-file pattern from `layer/surface/build/audit-remediation/`.

The vision this measures against: `docs/DECISIONS.md`,
`layer/core/what-is-mct.md`, and the active ratified build slices. The former
Allium product map is frozen historical evidence.

---

## Where we are

### Works end-to-end today (CLI-driven, single node)

install verified child package → approve (hash-verified) → persist toy
grant → invoke typed WIT export with JSON args → child reaches
git/logging/metrics/filesystem through capability gates → result lifts to
JSON → full trace reconstructible from the observation ledger.

The local WASM/WIT path runs under the hardened authority path: validated
timestamps and IDs, kernel-minted unforgeable capability tokens, staleness
guards at effect boundaries, execution deadlines and memory caps, and
fail-closed host adapter admission. Native and JVM systems are external
integrations, remote Mother workloads, or trusted Mother adapters—not local
Children. Historical process/JVM records remain inspectable but inert. Run
records, control-plane snapshots (HTTP/UDS), and the hash-chained ledger
provide inspection of MCT-mediated activity.
Administration currently uses the owner-authenticated local path. Granted
admin keys over iroh are the decided end state, not a shipped transport.

### Built and composed

- **Resident Mother:** one process owns the endpoint, control plane, Child
  execution, runtime projections, and ledger writer.
- **Payloads:** bounded inline transfer and local content-addressed blobs.
- **Routing and peers:** authority filtering, ranking, effect revalidation,
  signed binding verification, scoped publication, and single-hop forwarding.
- **Child lifecycle:** approval/assignment/instance generations, warmup,
  reload, task cycles, and resident ownership for WASM Children.
- **Toys:** logging, measure, Git, scoped WASI preopens, secrets backend, and
  the bounded Watch/supporting adapters used by the integration fixtures.

### Still open

1. **Crash-safe keyed dispatch independent of SQLite.** See the design-hardening
   slice; final Child-start admission evidence is a separate remaining boundary.
2. **Iroh administration and portable Child handles.** Complete key issuance,
   signed authorization, bootstrap/recovery, and the hard cutover.
3. **Canonical authority coverage.** Child approvals/assignments and peer
   bindings still depend on config; they are not all reconstructed from the ledger.
4. **Durable task continuation.** Wake/checkpoint/freeze, journaled effects,
   unknown outcomes, and Pi integration are not implemented.
5. **Breadth and scale.** General network/storage Toys, cross-Vision grants,
   inter-Mother blob transfer, and clustering remain separate work.

---

## Ordered TODO

Dependency-ordered; each item assumes the ones before it.

- [x] **1. Resident Mother daemon** — one `mct-daemon serve` process
      composing Iroh endpoint + peer serving + control plane + state +
      ledger + child supervision, with config-driven bindings, concurrent
      per-connection authority state, and graceful shutdown.
      Task file: `layer/surface/build/feat/resident-mother/TASKS.md`.
      Completed 2026-07-04; closeout recorded in `layer/surface/build/feat/resident-mother/TASKS.md`.
      Forcing functions: fixes single-slot hello state; forces the
      single-writer ledger and `!Sync` SQLite store into a concurrent
      architecture.
- [x] **2. Payload data plane** — inline payload bytes over `mct/call/0`
      first (bounded, validated against declared size/digest), content-
      addressed blob storage second (Iroh blobs are the natural adapter).
      Completed 2026-07-06 through slice 2: bounded inline request/result
      payloads plus a local content-addressed blob store under the node
      state dir. Follow-on remains Iroh blob transfer between Mothers.
      Unblocks: real workloads, result payloads, and local
      `ContentAddressedBlob` consumption.
- [x] **3. Routing wired end-to-end** — incoming calls flow through the
      two-phase route decision; the daemon consumes
      `AuthorizedRouteExecution` (must apply the same stale-revision guard
      as the other capabilities — obligation recorded in
      audit-remediation/PHASE3.md); local dispatch is just the
      single-candidate case.
      Completed 2026-07-06 for local candidates only; PHASE3 T5 stale-
      revision effect-boundary guard is discharged.
- [x] **4. Binding signature verification** — verify `signature_ref`
      against the issuer's key material at hello time; required before
      admitting peers you don't operate.
      Completed 2026-07-09 for the resident Mother path: peer entries can
      carry issued `binding_signature_ref` values, configured callers present
      them in hello, and unsigned/tampered proofs fail closed before admission.
- [ ] **5. Toy catalog growth** — secrets, network egress, and storage
      toys as WIT contract identities in the closed catalog, each behind
      grant evaluation like the existing toys.
      Secrets landed 2026-07-09 as `toy-secrets` / `patina:secrets/secrets@0.1.0#get`
      with redacted observations and scoped grants. Network egress and storage
      toys remain follow-up breadth.
- [ ] **6. Multi-Vision publication** — per-Vision capability publication
      and cross-Vision grants. Multi-Mother with per-Mother ledgers is future
      scale work, not planned now.

### Standing backlog (from the audit arc, non-blocking)

- [x] Close the process-backed confinement gap by subtraction: local Children
      are WASM components. `process call`, `iroh serve-process`, resident
      process dispatch, handle-only ingress, and the process Child harness are
      removed. Native/JVM software remains available only as external
      integration, remote Mother workload, or trusted Mother adapter;
      historical process/JVM records remain readable but inert.
- [x] `main.rs` CLI decomposition substantially addressed by Track 1 slice S2.5: binary-local CLI, control, and ingress modules own their behavior. The resident-decomposition phase completed the behavior-owning resident split into observation, payload, publication, idempotency, candidates, decision, execution, forwarding, pipeline, and serving stages while keeping `main.rs` at 146 lines. `main.rs` intentionally retains entrypoint dispatch wiring, argument-token helpers, default paths, and help text.
- [ ] Live node identity rotation — requires endpoint rebind plus peer re-admission design. Offline commands are not an administration path.
- [ ] Consolidate the concurrent and single-connection Iroh call-serving branches behind one reviewed lifecycle routine without weakening the mandatory observation sink or malformed fail-closed ordering; Slice 3 intentionally made both public paths correct before attempting that behavior-owning refactor.
- [ ] Property-based tests for ALPN intersection and payload validation.
- [x] Make the source-derived supported `folder-watch-actor@0.1.0` and exact unmodified `watch-null-sink@0.1.0` artifacts available to MCT CI alongside the real `slate-manager@0.2.0` acquisition-to-execution fixture. Completed 2026-07-21 in Replacement Slice 4B with scoped Watch/supporting grants, synchronous send admission, deterministic before-effect batch evidence, ordinary `WasmHost` delivery, legacy ABI narrowing, revocation, restart, and the supervised three-fixture `patinaMother` replacement proof.
- [ ] Per-connection hello state (subsumed by item 1).
- [x] Child SDK / packaging tooling in-repo (children currently built in
      the integrated Patina repo). Completed 2026-07-04 in the integrated
      Patina repo: `patina child init/build/package/verify`, canonical WIT
      world `patina:mct@0.1.0` at `wit/mct/`, oracle-verified.

---

## Working agreements (carried from the audit arc)

- Task lists live on disk, committed before work starts; agents check
  tasks off in the same commit as the completing change.
- Every agent completion report is verified against the repo before the
  next dispatch.
- Test failures are captured verbatim in the task file before rerunning.
- Validation gate for every commit: `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `./scripts/ci-tier0.sh`.

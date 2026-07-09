# MCT product roadmap — from runtime kernel to working product

Status assessment date: 2026-07-04 (post audit-remediation merge, PR #15).
This document records where MCT stands as a product and the ordered TODO to
close the gap. Update the checkboxes as phases land; each phase gets its own
task file under `layer/surface/build/feat/<name>/` following the proven
PHASE-file pattern from `layer/surface/build/audit-remediation/`.

The vision this measures against: `layer/core/what-is-mct.md` and
`layer/allium/mct-product-map.allium`.

---

## Where we are

### Works end-to-end today (CLI-driven, single node)

install verified child package → approve (hash-verified) → persist toy
grant → invoke typed WIT export with JSON args → child reaches
git/logging/metrics/filesystem through capability gates → result lifts to
JSON → full trace reconstructible from the observation ledger.

All of it runs under the hardened authority path: validated timestamps and
IDs, kernel-minted unforgeable capability tokens, staleness guards at
effect boundaries, execution deadlines and memory caps, fail-closed
everywhere. Process-backed children work the same way. Run records,
control-plane snapshots (HTTP/UDS), and the hash-chained ledger provide
after-the-fact inspection.

### Proven as slices, not product-wired

- **Peer federation.** Two Mothers complete `mct/hello/0` → `mct/call/0`
  with real binding admission; `iroh serve-process` executes a process
  child for a remote call. But: one connection at a time, single-slot
  hello state (a second peer's hello evicts the first), driven by a
  foreground CLI command with bindings supplied as CLI args.
- **Routing.** The kernel's two-phase decision model (authority filter →
  ranking → revalidation at execution) is complete and tested as decision
  logic; no daemon path consumes `AuthorizedRouteExecution` yet. Calls go
  where the operator points them.
- **Child lifecycle.** Approval/assignment/instance generations are
  modeled; a process supervisor plus warmup/reload/task-cycle exist as
  one-shot operations; no resident loop owns children.

### Does not exist yet

1. **A resident Mother.** `mct-daemon serve` serves control-plane
   snapshots only; Iroh serving is a separate foreground command. No
   single process binds the endpoint, supervises children, serves peers,
   and exposes control simultaneously.
2. **A data plane.** Calls carry payload metadata and results carry refs;
   there is no blob store and no payload byte transfer. Echo-shaped calls
   work; real workloads cannot move data.
3. **Cryptographic binding verification.** Iroh proves endpoint-key
   possession; binding presentations carry a `signature_ref` nothing
   verifies. Fine for trusted-operator setups, insufficient for
   multi-institution Visions.
4. **Toy catalog breadth.** logging/measure/git/WASI-preopens exist;
   secrets, network, and storage toys do not.
5. **Multi-Vision capability publication.** Vision scoping is in every
   authority record, but per-Vision publication and cross-Vision grants
   are unstarted.

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
      revision effect-boundary guard is discharged. Remote candidates and
      cross-Mother call forwarding remain recorded under item 6.
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
      and cross-Vision grants; the federation product. Route item 3 is
      local-candidate only; remote route candidates and cross-Mother call
      forwarding remain follow-on work here.

### Standing backlog (from the audit arc, non-blocking)

- [ ] `main.rs` CLI decomposition (2,600+ lines of subcommand dispatch).
- [ ] Property-based tests for ALPN intersection and payload validation.
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

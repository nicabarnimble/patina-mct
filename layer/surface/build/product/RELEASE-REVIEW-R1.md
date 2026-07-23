# MCT release review R1 — post-fixture security refresh and release gates

Date: 2026-07-21

Mode: report only; no implementation, checklist, TODO, Allium, or epic-status edits.

Baseline: branch `patina`, authorized HEAD `7d0e137cd096657c0bd6b03c2cb8fffb1705a2ec`. The only baseline delta from the three-fixture merge `bce1d75` is committed session archive material in `layer/events.jsonl` and `layer/sessions/20260716-124858-031057000.md`; the product/code diff excluding session artifacts is empty.

## Scope and method

This review read the two paused epic records, the replacement TODO/checklist/runbook, core safety/build/vocabulary law, and the completed resident-ingress, supervisor, acquisition, trigger, and Watch specifications and close-outs. Resident ingress and supervisor keep close-out evidence in their `SPEC.md`; no separate `CLOSEOUT.md` exists in those two directories. Code was reviewed at `7d0e137` before any change.

For each security item below:

- **Spec/Doc** cites the governing contract;
- **Code** cites current evidence;
- **Issue** is one sentence;
- **Disposition** is proposed for operator adjudication.

“Reviewed clean” means the named implementation seam and landed proof agree at this baseline; it is not a claim that future adapters or a production distribution have been reviewed.

## Baseline and audit gates

The required baseline gate passed:

```text
allium check layer/allium
  3 specs checked; 0 diagnostics; 0 findings

cargo test --workspace
  394 tests passed; 0 failed, plus doc tests

./scripts/ci-tier0.sh
  passed: formatting, Clippy with warnings denied, 394 workspace tests,
  comparative vocabulary, and all Allium checks
```

The dependency audit did not pass:

```text
cargo audit 0.22.2
  scanned Cargo.lock: 531 dependencies
  5 vulnerabilities found
  4 allowed warnings found
```

`cargo-audit` was not initially installed. It was installed in the user Cargo tool directory and then run against the repository lockfile and the current RustSec database; this changed no repository file.

---

# GATE QUESTION ONE — what has been replaced, and what still gates shutoff/release?

## Proposed reconciliation

### A. Runtime replacement: **PROVEN**

The functional runtime-replacement claim is proven by the supervised three-fixture suite:

- Slate executes as an `mctChild` from acquisition-backed `slate-manager@0.2.0` with exact approval and explicit `mctToy` grants;
- the source-derived `folder-watch-actor@0.1.0` MCT security rebuild executes from temporal trigger authority through scoped Watch observation and ordinary Child call-out;
- exact, unmodified `watch-null-sink@0.1.0` receives the narrowed legacy ABI call;
- revocation, restart, uninstall preservation, and reopened evidence are covered.

Evidence: `layer/surface/build/feat/artifact-acquisition/CLOSEOUT.md`, `layer/surface/build/feat/trigger-event-runtime/CLOSEOUT.md`, `layer/surface/build/feat/watch-event-fixtures/CLOSEOUT.md`, and `crates/mct-daemon/src/daemon/supervisor_lifecycle.rs:3323-4222` as cited by the Part B close-out.

This claim means `mctMother` has rebuilt the selected runtime responsibilities formerly exercised under `patinaMother`. It does **not** mean the current tree passes a refreshed production security gate: Gate Question Two reports six release findings.

### B. Operational-switch gate: **NOT YET RATIFIED**

Turning off `patinaMother` on the operator's machine is a separate operational decision. Disk/runtime inspection shows `patinaMother` is currently running under launchd as version `0.71.0`, with 181 registered projects and a ready control plane. Its advertised current responsibilities include hot model caching, cross-project knowledge, secrets caching, graph routing, project registration, source routing, skills/view buffers, and legacy `patinaChild`/`patinaToy`/federation/lifecycle surfaces (`patina` CLI `mother --help`; `patina` CLI `mother status`, captured during this review).

Daily operation still has these concrete dependencies or gaps:

1. **AI launcher and session orchestration.** `patina ai` is explicitly “Patina AI interface surface over Mother-backed sessions.” It owns setup/refresh, Claude/OpenCode/Gemini/Pi launching, interface/default selection, tmux choice, title/session/voice/path options, active-session listing, notes/updates, end, and durable session archival (`patina ai --help`, `patina ai pi --help`, `patina ai session --help`). MCT has no equivalent and should not put this behavior in the authority kernel.
2. **Patina epistemic/application behavior.** Belief, `scry`, `assay`, context, scrape, spec, persona, repo, and cross-project meaning remain Patina behavior by the accepted boundary. Today parts of that operation use `patinaMother` services such as model caching, registered-project routing, and cross-project search. No Patina-as-`mctChild` application or bridge is present in this repository.
3. **Production client ergonomics.** MCT has authenticated UDS `POST /calls`, but no JVM SDK or distributed client package. Existing `jvm call-json` remains development/compatibility evidence.
4. **Interface files and HITL control.** `patina interface` and `patina ai setup|refresh` still manage interface bundles and launch/attach behavior. The paused launcher epic correctly places this above MCT, but that makes it an operational-shutoff concern for an operator who uses it, not a runtime-proof concern.
5. **Legacy `patinaMother` service breadth.** the `patina` CLI `mother graph|search|run|sources|toys|federation|children|lifecycle|projects|skills|view` surface has not all been rebuilt as `mctMother` runtime responsibility; much of it is intentionally Patina application behavior or rejected legacy shape rather than missing `mctMother` core.

Before shutoff, the operator must choose one of two valid dispositions:

- **provide the interface/application layer:** build or select a Patina/interface layer above MCT (likely consuming the future JVM SDK), preserve session lifecycle and required epistemic workflows, migrate the needed daily operations, and perform a reversible shutoff drill; or
- **accept an explicit gap ledger:** acknowledge exactly which `patina ai`, cross-project, model-cache, graph/source, and Patina application workflows will be unavailable, name manual/local substitutes, preserve rollback instructions, and then stop/uninstall `patinaMother` deliberately.

Proposal: the paused `mct-interface-launcher-control` epic remains outside the runtime replacement boundary. It blocks operational shutoff only to the extent the operator requires its current behavior after `patinaMother` stops. Its implementation belongs in the interface/application layer, not in MCT core.

### C. Into-the-wild release gate: **NOT READY**

An external/production release still requires:

- disposition of all Gate Question Two findings;
- a refreshed checklist and CI dependency-audit gate;
- version discipline (`mct-daemon` and all workspace crates are still `0.1.0`);
- a reproducible locked release build and declared supported platform matrix;
- binary/package distribution, checksums/signing, upgrade/rollback, and macOS packaging/notarization decisions;
- installation and operator documentation that does not assume a source checkout;
- SBOM/provenance for distributed binaries and fixtures;
- a release-candidate shutoff/rollback drill;
- production performance baselines.

The prior epic record correctly says performance is not a **runtime replacement** blocker. Proposal: retain that ruling, but require at least captured latency/throughput/resource baselines before an **into-the-wild production release**. Hard performance SLOs may remain later if the first distribution is explicitly local/preview.

## Proposed amendment to the TODO final-gate sentence

The prompt calls this item 6; the current on-disk execution list numbers the sentence as item 8. Amend the current final-gate item to:

> 8. [ ] Adjudicate the final claims separately. The supervised three-fixture suite proves the v0 `patinaMother` **runtime responsibility replacement** boundary. Operational shutoff of `patinaMother` remains gated by an operator-ratified inventory of required Patina/interface/`patina ai` behavior and either a replacement application layer or explicit accepted gaps. Into-the-wild release remains gated by the refreshed security findings, version/packaging/distribution/docs discipline, dependency audit, and production performance baseline. The paused `mct-interface-launcher-control` epic remains follow-on interface-layer work above MCT and does not negate the landed runtime proof.

**Proposal for Gate Question One:** ratify the three-claim split and amend the current item 8 language above; do not mark either paused epic complete merely to make the runtime claim true.

---

# GATE QUESTION TWO — post-v0 security review refresh

## Finding summary

| ID | Severity | Area | Proposed release disposition |
|---|---|---|---|
| R1-H1 | high | UDS administrative authentication | Fix before any release/shutoff drill |
| R1-H2 | high | standing artifact source authority | Fix before standing-source use or release |
| R1-H3 | high | vulnerable dependencies | Patch or explicitly prove non-reachability before release |
| R1-H4 | high | ledger/error redaction | Replace raw Git stderr projection before release |
| R1-M1 | medium | UDS connection admission | Bound connections and read time before release |
| R1-M2 | medium | trigger catch-up bound | Make excess range terminal/deterministic before release |

Counts: **critical 0, high 4, medium 2, low 0; total 6 findings.**

## 1. Authenticated UDS ingress

### Reviewed clean — `/calls` authentication precedes body/capacity work

- **Spec/Doc:** owner-only socket and peer-UID equality before caller claims, `layer/surface/build/feat/resident-call-ingress/SPEC.md:96-125`.
- **Code:** peer credentials are captured before request dispatch, `/calls` runs preflight before body read, and preflight checks credential presence, exact expected UID, then declared body size, `crates/mct-daemon/src/control.rs:471-511`; `crates/mct-daemon/src/daemon/resident/local_ingress.rs:298-344`.
- **Evidence:** `resident_call_uds_dispatch_authenticates_and_bounds_before_body_read` and `resident_call_uds_authenticates_peer_before_submission`.
- **Result:** reviewed clean for the application-call endpoint.

### Reviewed clean — frame, payload, and idempotency bounds

- **Spec/Doc:** 96 KiB request frame, 32 KiB inline payload/result, 8 MiB CAS, caller-scoped replay, 720-second TTL, and 256-entry scope budget, `layer/surface/build/feat/resident-call-ingress/SPEC.md:170-199`.
- **Code:** headers are capped at 4 KiB and `/calls` body at `MCT_CALL_FRAME_READ_BUDGET_BYTES`, `crates/mct-daemon/src/control.rs:109-111,478-500,593-630`; the authenticated local caller scope is derived rather than body-selected, and observations hash scope/fingerprint without storing keys, `crates/mct-daemon/src/daemon/resident/idempotency.rs:5-64,156-219`; the store enforces 256 entries, expiry, mismatch, in-flight refusal, and exact completion, `crates/mct-daemon/src/state.rs:14,2275-2426`.
- **Evidence:** `resident_call_uds_idempotency_is_authenticated_caller_scoped`, `resident_idempotency_replays_scopes_refuses_and_expires_without_payload_leakage`, and `idempotency_store_scopes_reserves_replays_expires_and_survives_reopen`.
- **Result:** reviewed clean.

### R1-H1 — post-v0 administrative mutations do not share `/calls` exact owner-UID authentication

- **Severity:** high.
- **Spec/Doc:** resident acquisition, trigger, and Watch mutations are owner-authenticated via Unix peer UID, `layer/surface/build/feat/artifact-acquisition/SPEC.md:248,623`; `layer/surface/build/feat/trigger-event-runtime/SPEC.md:255-286`; `layer/surface/build/feat/watch-event-fixtures/SPEC.md:595`.
- **Code:** the shared mutation dispatcher receives peer credentials but sends artifact source/stage requests to handlers that do not receive or compare them, `crates/mct-daemon/src/daemon/control.rs:1864-1968`; trigger and Watch handlers require only `Some(peer)` and do not compare `peer.uid` with the socket/resident owner, `crates/mct-daemon/src/daemon/triggers.rs:378-392`; `crates/mct-daemon/src/daemon/watch.rs:444-461`; lifecycle alone performs an exact comparison, `crates/mct-daemon/src/daemon/supervisor_lifecycle.rs:382-409`.
- **Issue:** owner-only socket mode is a useful OS gate, but the new authority mutations do not enforce the ratified peer-UID equality and artifact staging can attribute the daemon UID without authenticating the connecting principal.
- **Disposition:** add one shared resident UDS owner preflight before mutation-body consumption and route dispatch, pass only an authenticated principal capability to mutation handlers, and add real-socket wrong-UID/credential-unavailable ordering proofs for artifact, trigger, and Watch routes.

### R1-M1 — UDS connection tasks and header/body read time are unbounded

- **Severity:** medium.
- **Spec/Doc:** local call work must be independently bounded by resident connection capacity and must not create unbounded tasks, `layer/surface/build/feat/resident-call-ingress/SPEC.md:121-127`.
- **Code:** every accepted control connection is spawned into a `JoinSet` without a connection permit, `crates/mct-daemon/src/daemon/control.rs:2334-2351`; header and body byte counts are bounded but reads have no deadline, `crates/mct-daemon/src/control.rs:593-630`; the call semaphore is acquired only after authenticated body read, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:425-446`.
- **Issue:** a same-user slow client can retain an unbounded number of resident control tasks/file descriptors even though each eventual request body is size-bounded.
- **Disposition:** add one named resident UDS connection bound acquired before spawn, bounded header/body deadlines, durable retry-later evidence where safe context exists, and a slowloris/capacity regression proving status and accepted calls remain available.

## 2. Supervisor lifecycle

### Reviewed clean — record integrity and plist handling

- **Spec/Doc:** current record must be owner-private, digest-valid, executable/plist-bound, launch-context-bound, and correlated to the governing ledger observation, `layer/surface/build/feat/supervisor-lifecycle/SPEC.md:136-198`.
- **Code:** record JSON denies unknown fields; validation checks mode/owner, schema/state/backend/label/domain, canonical record digest, executable digest, plist digest, launchd process context, and exact governing observation, `crates/mct-daemon/src/daemon/supervisor_lifecycle.rs:74-120,757-866`; plist values are XML-escaped, no shell is used, and publication is create-new staging plus sync/rename, `crates/mct-daemon/src/daemon/supervisor_lifecycle.rs:266-316`.
- **Evidence:** `supervised_start_rejects_unobserved_tampered_or_stale_records`, `supervised_start_rejects_unblessed_binary_swap_with_replace_guidance`, and the primary lifecycle reopen proof.
- **Result:** reviewed clean.

### Reviewed clean — conflicts, foreign policy, and fencing

- **Spec/Doc:** managed/manual/duplicate coexistence refuses before effects; foreign plist is preserved; known writer loss fences protected effects, `layer/surface/build/feat/supervisor-lifecycle/SPEC.md:270-290,400-430`.
- **Code:** replacement refuses loaded service/tampered predecessor, uninstall refuses foreign plist, and the resident writer is monotonic-fenced on queue/append/ack failure, `crates/mct-daemon/src/daemon/supervisor_lifecycle.rs:930-970,1740-1764`; `crates/mct-daemon/src/daemon/resident/observation.rs:102-147`.
- **Evidence:** `supervisor_conflicts_refuse_before_launchd_or_endpoint_effects`, `uninstall_refuses_foreign_plist_with_durable_observation`, `resident_writer_loss_fences_lifecycle_and_all_other_protected_effects`, and `shutdown_append_failure_has_no_clean_claim_and_next_start_reconciles`.
- **Result:** reviewed clean. Accepted limitations remain `gui/<uid>` only and strict binary digest replacement; neither is silently bypassed.

## 3. Artifact acquisition and staging

### Reviewed clean — source shape, integrity floor, and staging path

- **Spec/Doc:** exactly one source-trust path plus fresh one-attempt filesystem effect authority; mandatory SHA-256 sidecars for package-shaped acquisition; independent BLAKE3 evidence; hidden staging and digest-addressed immutable publication, `layer/surface/build/feat/artifact-acquisition/SPEC.md:218-287,323-398`.
- **Code:** claims and relative paths are validated before source access; roots/files are canonicalized beneath the selected source; files are bounded to 1 MiB manifest/64 MiB component; expected BLAKE3 and mandatory source sidecars are additive; output is staged under `.acquiring/<attempt>` and renamed to `artifacts/sha256/<digest>`, `crates/mct-daemon/src/acquisition.rs:154-195,311-483,498-664,722-805`.
- **Evidence:** `malformed_tampered_oversize_and_escaping_sources_leave_attempt_evidence_only`, `staged_package_reconciles_sha256_floor_with_blake3_acquisition_evidence`, `identical_reacquisition_adds_evidence_without_replacing_immutable_artifact`, and `same_digest_different_manifest_fact_cannot_replace_catalog_artifact`.
- **Result:** reviewed clean for operator-pointed staging and byte/path integrity.

### R1-H2 — standing source trust is accepted from SQLite without ledger correlation at acquisition

- **Severity:** high.
- **Spec/Doc:** a standing source SQLite row is insufficient; current evaluation must open the validated ledger and match `authority_observation_id`, record facts, and digest, while unobserved records grant nothing, `layer/surface/build/feat/artifact-acquisition/SPEC.md:218-254,537`.
- **Code:** staging loads a standing source from `state.source_authorities()`, recomputes the digest over that same projected row, and passes it directly to kernel evaluation, `crates/mct-daemon/src/acquisition.rs:162-180,282-289,470-483`; projection validation likewise validates shape/self-digest and immutable conflicts but does not open or correlate the ledger, `crates/mct-daemon/src/state.rs:2782-2847`.
- **Issue:** a syntactically valid active standing-source row can mint source trust without proving that its named authority observation exists in the validated ledger.
- **Disposition:** reconstruct or validate the exact standing source revision from the canonical ledger before minting the one-shot acquisition capability, make resident/offline paths share that verifier, and add absent/mismatched/hash-invalid ledger proofs that perform no source read or catalog effect.

## 4. Temporal triggers

### Reviewed clean — authority, deterministic identity, and ordinary call law

- **Spec/Doc:** owner-authenticated ledger-first immutable revisions, deterministic record/revision/occurrence identity, pending/firing durability, and fresh ordinary call authority, `layer/surface/build/feat/trigger-event-runtime/SPEC.md:156-286,360-456`.
- **Code:** create/revise/revoke build sealed records from current local identity, reject event/registry/network slots, append before projection, and exactly increment/reject resurrection, `crates/mct-daemon/src/daemon/triggers.rs:196-373`; startup reconciliation parses the validated ledger, restores missing projections, and rejects projection rows lacking durable facts, `crates/mct-daemon/src/daemon/resident/trigger_scheduler.rs:1471-1603`.
- **Evidence:** `trigger_authority_is_scoped_observed_revisioned_and_revocable`, `trigger_firing_idempotency_is_record_and_occurrence_scoped`, `trigger_evaluate_crash_re_evaluate_cannot_double_fire`, and `trigger_terminal_dispositions_survive_restart_without_resurrection`.
- **Result:** reviewed clean except R1-H1 and R1-M2.

### Reviewed clean — named pending/active capacities and fairness

- **Spec/Doc:** per-record pending 16, resident pending 256, active 8, and 32 evaluations per turn, with no eviction, `layer/surface/build/feat/trigger-event-runtime/SPEC.md:368-397,460-476`.
- **Code:** constants and the five-stage admission decision are explicit, `crates/mct-daemon/src/daemon/resident/trigger_scheduler.rs:6-31,255-329`; active trigger calls use a separate semaphore and pending rows are ordered, `crates/mct-daemon/src/daemon/resident/trigger_scheduler.rs:404-432,850-922`.
- **Evidence:** `trigger_capacity_refuses_at_each_named_bound_without_eviction`, `trigger_admission_order_is_fixed_and_authority_neutral`, and `trigger_load_does_not_starve_writer_control_status_or_ordinary_calls`.
- **Result:** reviewed clean for the three admission capacities.

### R1-M2 — production `fire_late_bounded` cannot terminally disposition excess recovery range

- **Severity:** medium.
- **Spec/Doc:** `fire_late_bounded` must admit only the bounded representatives and turn all excess known misses into one terminal capacity-refused range, not hidden retry, `layer/surface/build/feat/trigger-event-runtime/SPEC.md:324-351`.
- **Code:** production allows 32 evaluations per turn but names a 4096 recovery-range ceiling, `crates/mct-daemon/src/daemon/resident/trigger_scheduler.rs:7-8`; admission takes the minimum of range, 4096, and the remaining turn budget, then records the excess only if budget remains, `crates/mct-daemon/src/daemon/resident/trigger_scheduler.rs:1000-1029`.
- **Issue:** in production a missed set larger than the turn budget consumes all 32 slots, fails the `budget > 0` excess branch, and is reconsidered on later turns instead of receiving the required one terminal represented-range disposition.
- **Disposition:** reserve one evidence slot or compute the 4096 admitted/excess partition independently of turn execution budget, durably terminalize the excess once, and add a production-constant regression with a range greater than 4096.

## 5. Watch observation and Child call-out

### Reviewed clean — scope, path canonicalization, and independent grants

- **Spec/Doc:** exact current Watch scope plus Watch ToyGrant; safe root-relative metadata; independent content-read/keyvalue/observability grants, `layer/surface/build/feat/watch-event-fixtures/SPEC.md:173-300`.
- **Code:** canonical roots are resolved before scope creation, scope records are digest-sealed, safe paths reject absolute/ambiguous/`.`/`..` forms, and existing subjects reject every symlink component/special file, `crates/mct-daemon/src/daemon/watch.rs:138-145,173-260`; `crates/mct-kernel/src/watch.rs:183-319`; `crates/mct-daemon/src/daemon/resident/pipeline.rs:41-63`.
- **Evidence:** `watch_scope_and_toy_grant_are_both_current_before_observation`, `watch_grant_cannot_read_content_state_or_originate_delivery`, and `watch_adapter_excludes_escaped_symlinks_and_absolute_paths`.
- **Result:** reviewed clean, subject to the vulnerable Wasmtime dependency in R1-H3.

### Reviewed clean — synchronous admission, ABI narrowing, depth cap, and durability barrier

- **Spec/Doc:** synchronous `producer.send` shape/class/path/capacity admission, exact 0.1.x equality narrowing, one-level call-out, and batch evidence before nested calls, `layer/surface/build/feat/watch-event-fixtures/SPEC.md:326-471`.
- **Code:** admission enforces target/content/topic, 64 KiB message, 16 metadata pairs, scope/global batch bounds, event class, safe path, and legacy equality, `crates/mct-daemon/src/wasm.rs:76-157`; the ordinary call-out rejects depth `>= 1`, normalizes the set, appends batch/event/disposition facts, then constructs target calls, `crates/mct-daemon/src/daemon/resident/pipeline.rs:68-88,103-263`; exact 0.1.x validation is closed in kernel code, `crates/mct-kernel/src/watch.rs:323-347`.
- **Evidence:** `watch_send_admission_refuses_paths_shape_and_capacity_synchronously`, `watch_admission_append_failure_suppresses_every_nested_delivery`, `legacy_watch_abi_mismatch_is_refused_before_sink_call`, `watcher_child_callout_reenters_ordinary_call_law`, and `watch_delivery_lineage_is_actual_and_never_fabricated`.
- **Result:** reviewed clean.

## 6. Committed WASM fixtures

### Reviewed clean — byte provenance and patch scope

- **Spec/Doc:** source-derived watcher patch may only prune unused imports, narrow both legacy paths, and make required binding updates; sink remains exact/unmodified, `layer/surface/build/feat/watch-event-fixtures/SPEC.md:121-150,477-520`.
- **Code/receipts:** `folder-watch-actor` provenance binds upstream commit/tag, patch SHA-256/BLAKE3, toolchain/build command, component size/hashes, and the security-rebuild classification; the patch changes only watcher path construction/deletion identity and removes unused imports, `crates/mct-daemon/tests/fixtures/folder-watch-actor-0.1.0/PROVENANCE.md`; `crates/mct-daemon/tests/fixtures/folder-watch-actor-0.1.0/MCT-REBUILD.patch`. Sink provenance binds the same upstream commit/exact tag and unmodified build, `crates/mct-daemon/tests/fixtures/watch-null-sink-0.1.0/PROVENANCE.md`.
- **Independent check:** committed WASM SHA-256 values match receipts (`009104...9096`, `37f42e...fe3`, and Slate `76b568...966a`); no fixture `.sha256` sidecar exists.
- **Build audit:** `scripts/build-watch-fixtures.sh:21-117` verifies peeled tags, archives exact source, applies the patch with `git apply --check`, rebuilds both components, byte-compares output/manifests, rejects sidecars, and prints receipt/toolchain/WIT inventory.
- **Evidence:** `watcher_fixture_provenance_is_exact_source_derived_and_sidecar_free` and the full composed fixture proof.
- **Result:** reviewed clean for committed fixture identity and patch scope. The upstream watcher archive had no `Cargo.lock`; the receipt truthfully binds current bytes rather than claiming source-level reproducibility. R3 should add SBOM/attestation expectations for distributed fixture bytes.

## 7. Cross-cutting ledger, secrets, errors, and dependencies

### Reviewed clean — payload/secret exclusion on the new paths

- **Spec/Doc:** payload bytes, credentials, secret values, keyvalue values, and unbounded adapter errors do not enter the ledger, `layer/core/safety-boundaries.md:45-72`; `layer/surface/build/feat/watch-event-fixtures/SPEC.md:458`; `layer/surface/build/feat/artifact-acquisition/SPEC.md:377,581`.
- **Code/evidence:** local ingress records caller hashes and typed reasons, not body/key material, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:98-127,200-276`; trigger evidence stores static CAS references and hashes rather than payload/idempotency values, `crates/mct-daemon/src/daemon/resident/trigger_scheduler.rs:330-401,1131-1195`; secret Toy observations omit both secret name and value, covered by `secret_toy_backend_returns_secret_without_observing_value`; resident payload/idempotency reopen tests assert raw/base64 bytes and keys are absent.
- **Result:** reviewed clean for the post-v0 paths, except raw Git errors below.

### R1-H4 — Git adapter stderr is projected as a durable “safe” observation message

- **Severity:** high.
- **Spec/Doc:** caller-safe/ledger-safe output must not reveal secrets or unbounded adapter errors, `layer/core/safety-boundaries.md:64-72`; acquisition's real Slate flow exercises the Git Toy under explicit grant, `layer/surface/build/feat/artifact-acquisition/SPEC.md:471-475`.
- **Code:** failed Git commands capture arbitrary process stderr in `GitToyError::GitFailed`, `crates/mct-daemon/src/toy.rs:403-414,577-585`; `error.safe_message()` is then copied into the Toy failure observation's `safe_message`, `crates/mct-daemon/src/toy.rs:120-132,588-617`.
- **Issue:** Git/hook stderr can contain repository paths, commit content, remote details, credentials, or arbitrary hook output and is durably persisted without a bound or redaction despite being labeled safe.
- **Disposition:** map Git failures to a closed typed caller/ledger-safe message and bounded reason code; keep any raw stderr only in an explicitly operator-local diagnostic sink with redaction and size limits, and add a secret-marker/hook-output ledger regression.

### R1-H3 — `cargo audit` reports five known vulnerabilities in the release lockfile

- **Severity:** high.
- **Spec/Doc:** this R1 review requires `cargo audit` or equivalent; the release checklist currently has no dependency-audit gate.
- **Code/dependencies:** direct `wasmtime-wasi = 45.0.1` is pinned in `Cargo.toml:30` and directly supplies Child WASI preopens; `quick-xml 0.39.4` arrives through `plist -> netdev -> netwatch -> iroh`; `crossbeam-epoch 0.9.18` arrives through Wasmtime/Rayon and Iroh/Moka.
- **Issue:** the lockfile contains RUSTSEC-2026-0204 (`crossbeam-epoch`, invalid pointer dereference), RUSTSEC-2026-0194 and -0195 (`quick-xml`, two high-severity CPU/memory DoS advisories), RUSTSEC-2026-0182 (`wasmtime-wasi`, fd-renumber leak), and RUSTSEC-2026-0188 (`wasmtime-wasi`, medium FilePerms bypass for hard-link/rename destinations).
- **Disposition:** update to patched dependency versions (at minimum `wasmtime-wasi >=45.0.3`, `crossbeam-epoch >=0.9.20`, and an upstream chain carrying `quick-xml >=0.41.0`) or produce an operator-ratified non-reachability exception for a specific advisory; no into-the-wild artifact ships while `cargo audit` reports vulnerabilities. Add the audit to CI with an explicit policy for unmaintained/unsound/yanked warnings.

Additional audit warnings: unmaintained `paste`, unmaintained `proc-macro-error2`, unsound-warning `anyhow 1.0.102`, and yanked `spin 0.10.0`. These are not included in the six vulnerability findings, but R3 dependency policy must disposition them rather than silently allow them.

## Unverified leads — not findings

1. **Supervisor rename crash durability.** `atomic_write` syncs the staged file before rename but does not visibly fsync the parent directory (`supervisor_lifecycle.rs:287-316`). Confirm APFS/target-platform durability expectations before claiming power-loss durability for record/plist publication.
2. **Watch historical active-scope selection.** host construction and delivery search all stored scopes for the highest active revision rather than querying the `is_current` projection (`resident/execution.rs:321-338`; `resident/pipeline.rs:92-102`). Current Watch grant revocation appears to deny independently, but verify a stale scope can never be revived by a surviving/recreated grant.
3. **Transitive XML reachability.** the two `quick-xml` advisories are transitive through Iroh network-interface discovery; exact attacker-controlled XML reachability in MCT was not established. This does not waive the failed dependency gate.
4. **Unlocked upstream fixture builds.** watcher provenance explicitly says the archive had no `Cargo.lock`; committed bytes are pinned and tested, but repeatable source rebuild and dependency provenance require a distribution policy/SBOM decision.

**Proposal for Gate Question Two:** accept all six findings into R2; treat all four high findings as release blockers, and treat both medium findings as blockers for an into-the-wild release while allowing the already-landed functional runtime proof to stand.

---

# GATE QUESTION THREE — proposed R-slice sequence

## R2 — security remediation, no scope expansion

Use small concern-specific commits under the standing per-commit validation gate:

1. **R2A — local authority ingress:** one shared owner-UID preflight for every resident mutation plus bounded UDS connection/read admission (R1-H1, R1-M1).
2. **R2B — acquisition authority:** ledger-correlate standing source revisions before filesystem capability minting (R1-H2).
3. **R2C — deterministic scheduler capacity:** terminalize excess `fire_late_bounded` recovery ranges under production constants (R1-M2).
4. **R2D — dependency and redaction closure:** patch RustSec vulnerabilities and replace Git stderr ledger projection (R1-H3, R1-H4).
5. Rerun targeted failure proofs, full workspace tests/Clippy/tier-0, Allium check/analyse where applicable, `cargo audit`, and a reopened-evidence regression set.

No R2 item requires launcher/JVM/new Toy/network/coupled-slot work or an Allium amendment. A discovered law conflict stops for the operator.

## R3 — release/version/packaging discipline

After R2 is green:

- decide the release identity and version relationship among the four `0.1.0` crates;
- define release-candidate tagging/changelog/version checks;
- build with the repository lockfile and record toolchain/source revision;
- produce checksums, SBOM, provenance, and signing/notarization disposition;
- define installation layout independent of `target/release` and source checkout;
- define upgrade/rollback with strict supervisor executable digest binding;
- define supported macOS GUI-domain boundary and explicit Linux/headless exclusions;
- add `cargo audit` policy to CI;
- capture baseline startup, idle RSS, UDS call latency/throughput, trigger-turn load, and three-fixture resource use;
- refresh `RELEASE-CHECKLIST-v0.md`, operator docs, and the TODO only after evidence lands.

Proposal: performance baselines are **in R3 for into-the-wild release**, but remain **out of the functional runtime-replacement claim** and need not become hard SLOs for a local preview.

## R4 — operational switch adjudication/drill

Only if the operator wants to turn off `patinaMother`:

- ratify the required `patina ai`/interface/Patina application behavior inventory;
- choose build-versus-accepted-gap dispositions;
- if building, open separately gated interface-layer/JVM SDK work outside this release-hardening implementation;
- run a reversible stop/shutoff trial with session, epistemic, model-cache, cross-project, launcher, and rollback checks;
- retire `patinaMother` only after the operator accepts the evidence.

If the operator does **not** want operational shutoff in this release, record that explicitly: runtime replacement may be claimed, but `patinaMother` remains installed for Patina/interface application services.

## R5 — into-the-wild release candidate

Cut a release candidate only after R2/R3 and, if claimed, R4. Re-run the complete checklist from a clean checkout/install artifact rather than the developer target directory, verify package checksums/signatures, perform fresh install/upgrade/rollback/uninstall-preservation tests, and publish the exact support/deferred-scope statement.

**Proposal for Gate Question Three:** approve R2 security closure → R3 release discipline/performance baselines → optional-but-explicit R4 operational switch → R5 release candidate. Do not resume launcher implementation inside MCT core.

---

# Three-question gate

1. **Gate Question One:** ratify or amend the three-claim split—runtime replacement proven; operational shutoff separately gated; into-the-wild release separately gated—and the proposed current TODO item 8 language.
2. **Gate Question Two:** accept, reject, or re-severity the six findings and their dispositions.
3. **Gate Question Three:** ratify or amend the proposed R2–R5 sequence and the placement of performance baselines.

STOP. This report proposes dispositions only. No fix, checklist edit, TODO amendment, Allium edit, launcher/JVM work, or epic-status change is authorized until the operator adjudicates all three questions.

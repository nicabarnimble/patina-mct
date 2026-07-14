# MCT spec-drift audit — pass 2 at the Slice-1 boundary

Date: 2026-07-14

Mode: report only (`weed` check mode)

Baseline: branch `patina`, `ff3b548` (`Merge pull request #24 from nicabarnimble/patina`), clean tree. The operator authorized this merge commit as a tree-identical substitute for `79a01bc`; `git diff 79a01bc ff3b548` was empty.

## Scope and method

This pass extends the method and ID sequence from `REPORT.md` without reopening its 23 terminally dispositioned findings. It swept:

- all three active Allium laws and the full Track 3 attribution ledger;
- D1.1–D1.10 and the required proof in the completed resident-call-ingress SPEC;
- the resident UDS framing, local ingress, pipeline stages, control dispatch, status projection, and CLI boundary at HEAD `ff3b548`;
- the migration vocabulary, product direction, release checklist, replacement runbook, and next-build TODO;
- the migration companion's coverage of sole-resident ownership, kernel-versus-product placement, quarry classification, and the four-question placement discipline.

For each finding, **Spec/Doc** names the governing or stale text, **Code** names the evidence at HEAD `ff3b548`, **Divergence** is one sentence, and **Direction** proposes the W1 disposition. Absence claims were checked with repository search, not inferred from memory: `mct-product-map.allium` contains zero occurrences of `POST /calls`, `peer UID`, `Unix peer`, `JvmAdapter`, `same OS user`, and `local application bridge`.

### Exclusions

- The original A1–A8, B1–B5, C1–C5, D1–D4, and T1 findings retain their terminal dispositions.
- The seven peer-ontology open questions and every Track 3 deferred row remain parked to roadmap item 6 or their already named future scope.
- This pass did not reassess terminal single-hop semantics, bilateral admission, publication meaning, or D1.1–D1.10 as design choices.
- No launchd/systemd, reconciliation, package lifecycle, new mctToy, multi-Vision, or Patina epistemic implementation was designed.
- Exact HTTP/JSON field spelling, `.mct/control.sock`, Unix mode mechanics, response status codes, the synchronous v0 response shape, and the current semaphore size remain SPEC-local operational details. The product-map tend candidates below abstract the authority and lifecycle meaning rather than freezing those mechanisms.
- Existing payload bounds/integrity and generic idempotency behavior are already product-map law; only the new local-principal semantics are a tend gap.

## Tooling and baseline gate

The required baseline gate passed before evidence collection:

```text
allium check layer/allium
  3 specs checked; 0 diagnostics; 0 findings

cargo test --workspace
  305 passed; 0 failed

./scripts/ci-tier0.sh
  passed: formatting, Clippy with warnings denied, 305 workspace tests,
  comparative vocabulary enforcement, and all Allium checks
```

The checkpoint analysis also passed:

```text
allium 3.5.0 (language versions: 1, 2, 3)
allium analyse layer/allium
  3 specs analysed; 0 diagnostics; 0 findings
```

## Classification

This pass continues the established classes:

- **A — contradiction:** code or claimed completion violates ratified law or an approved SPEC.
- **B — under-described:** landed, invariant-shaped behavior is absent or materially incomplete in the governing Allium layer.
- **C — future scope:** specified future behavior remains intentionally unbuilt.
- **D — ratification needed:** code semantics exist but still require operator ratification.

One new class is necessary for prose and attribution artifacts:

- **E — stale governance/documentation:** a ledger, checklist, runbook, or planning document is incomplete or contradicts landed and ratified behavior while semantic law itself need not change.

No new C or D finding was opened: the relevant future work is already covered by C1–C5/item 6, and Slice 1's new semantics were ratified in D1.1–D1.10 rather than merely discovered in code.

---

## Class A — code or completion evidence contradicts ratified law

### A9 — the real UDS path reads caller claims and may apply capacity policy before same-UID authentication

- **Severity:** critical.
- **Spec/Doc:** authenticated UID verification must happen before reading caller claims, `layer/surface/build/feat/resident-call-ingress/SPEC.md:27,96-119`; the fixed order starts with transport authentication and resident identity, `layer/surface/build/feat/resident-call-ingress/SPEC.md:183-199`.
- **Code:** the stream captures peer credentials but reads and buffers the entire HTTP request before invoking the call handler, `crates/mct-daemon/src/control.rs:416-438`; UID equality is checked only inside that later handler, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:296-322`; the semaphore refusal runs before even that check, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:408-430`.
- **Divergence:** the accepted socket path consumes body-supplied caller claims and can disclose a capacity refusal to an unauthenticated/mismatched peer before enforcing D1's first same-UID gate.
- **Direction:** **spec-ward fix** — authenticate against the expected owner at connection dispatch before body consumption and before call-capacity admission; add a real-socket ordering regression rather than relying only on direct handler invocation.

### A10 — the checked-off required integration proof never performs UDS replay or post-stop disk reconstruction

- **Severity:** high.
- **Spec/Doc:** the required whole-path proof must retry the same call/key without a second child effect and then stop/reopen disk state to prove observations, run, result, and replay survival, `layer/surface/build/feat/resident-call-ingress/SPEC.md:274-287`; the SPEC nevertheless marks all exit criteria complete, `layer/surface/build/feat/resident-call-ingress/SPEC.md:20-64,305-337`.
- **Code:** the primary UDS test submits once, inspects `/runs` and status, reads the live ledger, then shuts down and returns without retry or reopen, `crates/mct-daemon/src/daemon/resident/serving.rs:626-794`; the separate idempotency test invokes `execute_local_submission` directly and does not cross the UDS or restart boundary, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:580-626`.
- **Divergence:** the implementation may contain lower-level durable replay behavior, but the SPEC's mandatory end-to-end replay-and-reopen proof is absent while completion is claimed.
- **Direction:** **spec-ward fix** — extend the disk-backed UDS integration test through same-key replay, resident shutdown, state/ledger reopen, and replay survival before retaining the checked completion claim.

### A11 — remote cancellation is still collapsed into failure in the resident forwarding projection

- **Severity:** critical.
- **Spec/Doc:** `cancelled` was explicitly added to preserve the closed result outcome through reply projection without collapse, `layer/allium/mct-product-map.allium:884-885`; consumers must preserve the closed `success | denied | failed | timed_out | cancelled` set, `layer/allium/mct-product-map.allium:1191-1192,1251-1252`.
- **Code:** forwarding observations retain `Cancelled`, `crates/mct-daemon/src/daemon/resident/forwarding.rs:52-60`, but `remote_reply_to_call_handler_result` converts `CallProtocolReplyOutcome::Cancelled` into `MctIrohCallHandlerResult::failed("call cancelled")`, `crates/mct-daemon/src/daemon/resident/forwarding.rs:536-563`.
- **Divergence:** an executor's typed cancellation survives the peer reply and observation but becomes failure when projected into the originating resident result.
- **Direction:** **spec-ward fix** — preserve `Cancelled` in the remote forwarding mapper and add a two-Mother cancellation regression; this is a newly identified forwarding edge, not a re-adjudication of the already fixed local/Iroh cancellation finding.

### A12 — “real” status freezes loaded and approved child counts at resident startup

- **Severity:** medium.
- **Spec/Doc:** status must report actual resident identity and child/instance counts from the resident projection, `layer/surface/build/feat/resident-call-ingress/SPEC.md:51-54,244-260`.
- **Code:** `ResidentStatusSource` stores `loaded_child_count` and `approved_child_count` as immutable startup values and returns them on every status request, `crates/mct-daemon/src/daemon/resident/serving.rs:73-121`; those values are computed once during startup, `crates/mct-daemon/src/daemon/resident/serving.rs:215-236`; the CLI copies them directly while only ready instances come from the live snapshot state, `crates/mct-daemon/src/daemon/cli_admin.rs:719-779`.
- **Divergence:** live registry/approval mutations can change the resident's callable child facts while `mct-daemon status` continues reporting startup counts as current.
- **Direction:** **spec-ward fix** — derive loaded/approved counts from current config/children/state at snapshot time or maintain an explicitly refreshed projection, then test a live mutation followed by status.

---

## Class B — invariant-shaped behavior is under-described in Allium

### B6 — the product map has no local-principal authentication and canonical-caller contract

- **Severity:** high.
- **Spec/Doc:** D1 makes peer UID plus resident node/Vision the canonical caller and forbids body-supplied caller/origin/peer authority, `layer/surface/build/feat/resident-call-ingress/SPEC.md:96-119,141-168`; the product map says only that adapters translate into one call and local idempotency uses a canonical identity, `layer/allium/mct-product-map.allium:856-864,1052-1054`.
- **Code:** local translation constructs `CallerIdentity` from resident identity and authenticated UID, clears project, forces `JvmAdapter`, and privately supplies compatibility authority fields, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:129-194`; authentication/refusal is enforced at `crates/mct-daemon/src/daemon/resident/local_ingress.rs:296-357`.
- **Divergence:** the product map has no invariant requiring a local application adapter to derive caller authority from an authenticated local principal while rejecting client-authored caller/origin/peer claims.
- **Direction:** **map tend** — add mechanism-neutral local application ingress/caller derivation law; keep Unix UID, HTTP, and envelope fields in the feature SPEC.

### B7 — local durable-before-response call lifecycle is not explicit map law

- **Severity:** high.
- **Spec/Doc:** D1 requires local admission/refusal/replay/result facts to be durable before the synchronous application response, with no response on append failure, `layer/surface/build/feat/resident-call-ingress/SPEC.md:201-229`; the map's explicit reply lifecycle is peer-specific while generic call ingress and before-effect clauses do not name local acknowledgement, `layer/allium/mct-product-map.allium:1166-1167,1672-1674,1795-1796`.
- **Code:** local refusals await a ledger append before response, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:101-127`; accepted submissions append receipt/construction and a final result before response projection, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:200-276,381-406`.
- **Divergence:** the local application's response is now an authority-relevant effect with write-ahead obligations, but the map names that complete response barrier only for peer calls.
- **Direction:** **map tend** — generalize or add a local-call acknowledgement invariant without duplicating the SPEC's HTTP status contract.

### B8 — `JvmAdapter`'s local-bridge lineage meaning exists only in D1 and code

- **Severity:** medium.
- **Spec/Doc:** D1.9 says `JvmAdapter` means local application-bridge ingress lineage, not caller language, `layer/surface/build/feat/resident-call-ingress/SPEC.md:129-139`; the product map governs origin as non-permission telemetry but does not define this variant's truthful meaning, `layer/allium/mct-product-map.allium:756-773,856-864`.
- **Code:** local calls force `CallOrigin::JvmAdapter`, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:143-161`, and both receipt/construction facts explicitly record `ingress:local_application_bridge`, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:200-237`.
- **Divergence:** the runtime intentionally uses a language-named enum as transport-lineage evidence, but the governing map cannot prevent a future consumer from interpreting it as proof that the caller runs on a JVM.
- **Direction:** **map tend** — capture truthful lineage semantics while leaving any eventual enum rename or new adapter mapping to a separate implementation decision.

### B9 — the shared same-UID idempotency namespace is under-described

- **Severity:** high.
- **Spec/Doc:** D1.10 fixes the local scope to `(node, UID, Vision)`, shared by all same-user applications, and requires namespaced keys/UUID SDK defaults, `layer/surface/build/feat/resident-call-ingress/SPEC.md:183-199`; the product map says only “current authenticated caller identity” and “canonical local caller identity,” `layer/allium/mct-product-map.allium:1052-1065,1118-1121`.
- **Code:** local scope serialization includes origin, node, canonical UID user, Vision, and absent project, `crates/mct-daemon/src/daemon/resident/idempotency.rs:5-34`; the regression proves same-key replay and cross-application mismatch within one UID, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:580-626`.
- **Divergence:** the map does not expose that v0 authenticates an OS-user principal rather than an application, so independently deployed same-user clients unknowingly share one idempotency keyspace.
- **Direction:** **map tend** — state the principal-sharing consequence and key-namespace obligation without freezing UUID as protocol law.

### B10 — the map does not distinguish call concurrency from serialized resident mutations

- **Severity:** medium.
- **Spec/Doc:** D1.2 says calls share transport/framing but do not enter the administrative mutation sequencer and must leave reads available during synchronous execution, `layer/surface/build/feat/resident-call-ingress/SPEC.md:121-127`; the map says the resident serializes protected mutations but does not classify calls relative to that boundary, `layer/allium/mct-product-map.allium:1588-1619`.
- **Code:** administrative handlers share one mutex, `crates/mct-daemon/src/daemon/control.rs:1471-1548`; `/calls` has a separate semaphore, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:408-430`; accepted UDS connections run concurrently through a `JoinSet`, `crates/mct-daemon/src/daemon/control.rs:1838-1913`.
- **Divergence:** the landed control plane relies on “call is not mutation” to preserve liveness and mutation ordering, but that product boundary is not checkable in the map.
- **Direction:** **map tend** — add a mechanism-neutral separation invariant: shared transport cannot place normal calls inside the protected mutation critical section or weaken mutation serialization.

### B11 — the migration companion does not state sole-resident operational takeover

- **Severity:** high.
- **Spec/Doc:** the settled direction says MCT becomes the sole resident coordinator, `layer/sessions/20260714-065952-699326000.md:34-35`, and the runbook says Patina may be an `mctChild` rather than another resident coordinator, `layer/surface/build/product/MOTHER-REPLACEMENT-RUNBOOK.md:7-11`; migration law defines replacement per responsibility but has no invariant forbidding a surviving competing `patinaMother`, `layer/allium/mct-patina-migration.allium:53-72`.
- **Code:** `mct-daemon` dispatches the resident `serve` and real `status` surfaces, `crates/mct-daemon/src/main.rs:57-95`, and resident application ingress is owned inside the daemon at `crates/mct-daemon/src/daemon/resident/local_ingress.rs:408-447`.
- **Divergence:** responsibility-level replacement can satisfy the current migration law even while two resident coordinators remain, contrary to the settled product direction.
- **Direction:** **map tend** — add a comparative sole-resident/coordinator invariant while preserving Patina's product identity and possible `mctChild` role.

### B12 — migration law does not encode the narrow kernel versus complete product boundary

- **Severity:** high.
- **Spec/Doc:** the product map separates kernel decisions from daemon lifecycle/adapters, `layer/allium/mct-product-map.allium:103-130`, but the migration companion lists vocabulary/verb/capability scope and explicitly excludes deciding accepted responsibilities, `layer/allium/mct-patina-migration.allium:6-17`.
- **Code:** `mct-kernel` exports authority domain modules, `crates/mct-kernel/src/lib.rs:1-24`, while UDS ingress and resident orchestration remain binary-local daemon modules, `crates/mct-daemon/src/main.rs:36,90-101` and `crates/mct-daemon/src/daemon/resident/local_ingress.rs:1-21`.
- **Divergence:** the migration companion cannot check the settled rule that generic operational breadth belongs to the MCT product while only authority decisions/domain records may expand the kernel.
- **Direction:** **map tend** — reference or encode the placement boundary in the comparative law rather than copying product-map internals.

### B13 — the four quarry disposition bins are not represented as checkable migration law

- **Severity:** medium.
- **Spec/Doc:** current migration law defines verbs (`translate`, `rebuild`, `port`, `replace`, `retire`) and rejects ambient legacy authority, `layer/allium/mct-patina-migration.allium:50-93`, while living narrative says `patinaMother` is operational prior art whose accepted responsibilities are translated/rebuilt, `layer/core/what-is-mct.md:150-158`; it does not classify quarry output into the settled responsibility destinations/rejection bin.
- **Code:** local UDS ingress is a concrete rebuilt operational responsibility in daemon code, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:1-21,408-447`, whereas authority records remain in the kernel export surface, `crates/mct-kernel/src/lib.rs:1-24`.
- **Divergence:** verb correctness is checkable, but the law cannot classify a quarried responsibility as MCT product, MCT authority kernel, Patina application meaning, or rejected legacy shape.
- **Direction:** **map tend** — encode the four settled bins as placement obligations without drafting the detailed law in this report.

### B14 — the four-question placement test is absent from the migration companion

- **Severity:** medium.
- **Spec/Doc:** the layering narrative separates Patina application semantics, MCT authority/runtime/protocols, and Iroh substrate, `layer/core/what-is-mct.md:143-157`, while migration vocabulary makes verb choice an audit signal, `layer/core/migration-vocabulary.md:77-87`; `mct-patina-migration.allium:19-108` contains no placement-test contract or equivalent decision sequence.
- **Code:** the current binary makes the practical split visible—resident/application adapter concerns in `crates/mct-daemon/src/main.rs:57-101` and authority domain records in `crates/mct-kernel/src/lib.rs:1-24`—but no governed test requires future slices to ask the settled four placement questions before choosing a layer.
- **Divergence:** future launchd, reconciliation, packaging, and mctToy work can use correct migration verbs yet still land in the wrong product/kernel/application/reject destination because the agreed placement test is not law.
- **Direction:** **map tend** — make the settled four-question placement test checkable; if exact wording is not recoverable from durable evidence, carry only wording—not the settled classification decision—into the upcoming operational-scope elicitation agenda.

---

## Class E — stale governance and product documentation

### E1 — Track 3 claims complete attribution but has no Slice-1 local-ingress rows

- **Severity:** high.
- **Spec/Doc:** the ledger is dated 2026-07-12 and still claims a complete 223-invariant/179-obligation inventory, `layer/surface/build/spec-drift-audit/track3/LEDGER.md:3,453-471`; its current ingress, idempotency, payload, origin, and durability rows cite pre-Slice-1 peer/JVM/direct-pipeline tests, `layer/surface/build/spec-drift-audit/track3/LEDGER.md:42-59,102,255-256,356`.
- **Code:** Slice 1 added named local tests for peer authentication, payload-byte exclusion, caller-scoped replay/mismatch, durable-before-response, and full resident execution/control visibility, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:521-675` and `crates/mct-daemon/src/daemon/resident/serving.rs:626-794`.
- **Divergence:** fail-closed local peer authentication/caller derivation, local response durability, local byte exclusion, and same-UID idempotency scope carry invariant-shaped obligations but have no Track 3 attribution rows.
- **Direction:** **doc fix** — after W1/map tending, add explicit rows and adjust counts/status without rewriting the historical 2026-07-12 evidence.

### E2 — release and replacement docs still defer implemented cross-Mother forwarding

- **Severity:** high.
- **Spec/Doc:** the release checklist defers “Multi-Vision publication and cross-Mother remote route forwarding,” `layer/surface/build/product/RELEASE-CHECKLIST-v0.md:28-34`, and the runbook repeats that combined deferral, `layer/surface/build/product/MOTHER-REPLACEMENT-RUNBOOK.md:89-94`.
- **Code:** resident forwarding performs current-authority revalidation and Iroh execution, `crates/mct-daemon/src/daemon/resident/forwarding.rs:93-226`, with a full two-Mother success proof at `crates/mct-daemon/src/daemon/resident/forwarding.rs:920-1131`.
- **Divergence:** single-hop cross-Mother forwarding is implemented and mapped, but two operator documents still present it as future by coupling it to genuinely deferred multi-Vision work.
- **Direction:** **doc fix** — split completed single-hop forwarding from deferred multi-Vision/transitive scope in both documents.

### E3 — the replacement runbook still presents one-shot JVM ingress and offline inspection as the operational boundary

- **Severity:** high.
- **Spec/Doc:** the resident workflow omits local application calls, `layer/surface/build/product/MOTHER-REPLACEMENT-RUNBOOK.md:32-45`; “JVM bridge ingress” directs callers to one-shot `jvm call-json`, `layer/surface/build/product/MOTHER-REPLACEMENT-RUNBOOK.md:65-77`; inspection lists direct state/runs/metrics reads and no resident status command, `layer/surface/build/product/MOTHER-REPLACEMENT-RUNBOOK.md:79-86`.
- **Code:** the resident serves authenticated UDS `/calls`, `crates/mct-daemon/src/control.rs:416-447` and `crates/mct-daemon/src/daemon/resident/local_ingress.rs:408-430`; `status` queries `/snapshot` and fails when no resident is reachable, `crates/mct-daemon/src/daemon/cli_admin.rs:719-817`.
- **Divergence:** the replacement runbook does not document the production Slice-1 resident call/status loop and can still steer operators toward independent one-shot resource ownership.
- **Direction:** **doc fix** — make UDS `POST /calls` plus resident-backed status the normal workflow and label `jvm call-json` as compatibility/development evidence.

### E4 — the release checklist's active-spec and operator gates stop before Slice 1

- **Severity:** medium.
- **Spec/Doc:** the checklist names `mct-typed-wit-runtime-parity` as the active closeout and validates CLI exposure through `jvm call-json`, `layer/surface/build/product/RELEASE-CHECKLIST-v0.md:5-11,23-26`.
- **Code:** resident UDS execution/control projection is covered at `crates/mct-daemon/src/daemon/resident/serving.rs:626-794`, and the CLI now exposes `status [--uds socket-path] [--json]`, `crates/mct-daemon/src/main.rs:57-63,136-144`.
- **Divergence:** the release gate ledger omits the completed resident-call-ingress SPEC and the new operational status/call boundary.
- **Direction:** **doc fix** — append Slice-1 gates without erasing the historical typed-WIT closeout.

### E5 — the next-build TODO labels completed Multi-Mother work as “Now”

- **Severity:** low.
- **Spec/Doc:** execution order marks single-hop Multi-Mother complete, `layer/surface/build/product/MCT-NEXT-BUILD-TODO.md:5-12`, but the next heading remains `Now: Multi-Mother`, `layer/surface/build/product/MCT-NEXT-BUILD-TODO.md:14`, before the separately completed resident section at `layer/surface/build/product/MCT-NEXT-BUILD-TODO.md:69-82`.
- **Code:** the two-Mother forwarding proof is landed at `crates/mct-daemon/src/daemon/resident/forwarding.rs:920-1131`, while the current operational boundary is resident ingress/status at `crates/mct-daemon/src/daemon/resident/serving.rs:626-794`.
- **Divergence:** the TODO's heading-level narrative points reviewers at a completed phase instead of the actual next storage/network, JVM SDK, and supervision work.
- **Direction:** **doc fix** — relabel the completed section and preserve item 6's multi-Vision/transitive follow-ons.

---

## Slice-1 placement assessment

The following D1 decisions are **invariant-worthy product-map law** at their mechanism-neutral level:

| D1 meaning | Why it belongs in map law | Finding |
|---|---|---|
| Authenticated local principal determines canonical caller; body claims grant nothing | authority boundary shared by every future local SDK/adapter | B6 |
| Durable local decision/result before application acknowledgement | local response is an effect and fail-closed audit guarantee | B7 |
| Origin truthfully names bridge lineage, not caller language or authority | prevents telemetry becoming identity/permission | B8 |
| Same authenticated principal shares idempotency scope | caller isolation and replay safety are product semantics | B9 |
| Calls are not protected mutations despite shared transport | preserves mutation ordering and call/read liveness | B10 |

The following remain **SPEC-local implementation contract** rather than new product-map law: exact socket path, HTTP method/path, JSON fields, Unix credential API, mode bits, HTTP statuses, synchronous v0 delivery, and the chosen capacity number. Moving to another local transport would require a new approved SPEC but need not rename kernel ontology. Existing named payload bounds, BLAKE3 verification, byte-free observations, and general caller-scoped idempotency are already map law and need attribution, not duplicate invariants.

## Spot-verified alignment at HEAD `ff3b548`

No finding was opened for these checked seams:

- Socket publication is mode `0600` and endpoint readiness is durably observed before the control accept loop (`crates/mct-daemon/src/daemon/control.rs:1838-1871`).
- Calls bypass the administrative mutation mutex, while each accepted UDS connection has independent task dispatch and bounded call capacity (`crates/mct-daemon/src/daemon/control.rs:1471-1548,1872-1913`; `crates/mct-daemon/src/daemon/resident/local_ingress.rs:408-430`).
- Once translated, local calls reuse payload verification → idempotency → authority/routing → revalidation → execution/forwarding → result sequencing (`crates/mct-daemon/src/daemon/resident/pipeline.rs:39-153`).
- Local payload integrity precedes idempotency and route authority, and observations retain only metadata/digests (`crates/mct-daemon/src/daemon/resident/payload.rs:149-258`; `crates/mct-daemon/src/daemon/resident/pipeline.rs:39-76`).
- Peer arrivals remain terminal and do not source another peer (`crates/mct-daemon/src/daemon/resident/candidates.rs:18-29`; `crates/mct-daemon/src/daemon/resident/decision.rs:729-752`).
- The post-Slice-1 connection dispatch did not weaken bilateral peer candidacy, current binding revalidation, or single-hop forwarding law.
- The kernel gained no UDS/HTTP/Unix-credential/Patina epistemic implementation types; local ingress remains daemon-owned.

## Unverified leads (not findings)

### U1 — whether the 96 KiB “call frame” includes the HTTP envelope

D1 says the call frame is bounded before JSON/base64 decode (`SPEC.md:170-181`). The shared UDS reader allows a much larger control request budget derived from the 8 MiB blob path, `crates/mct-daemon/src/control.rs:109,416-432`, and the call handler later applies 96 KiB to the body only, `crates/mct-daemon/src/daemon/resident/local_ingress.rs:323-331`. The body is bounded before decode, so code matches one reasonable interpretation; if “frame” means headers plus body or must be enforced during streaming read, W1 should clarify the contract before treating this as A-class drift.

## Summary

| ID | Class | Severity | Area | Proposed disposition |
|---|---|---|---|---|
| A9 | A | critical | UDS authentication order | spec-ward fix |
| A10 | A | high | required UDS replay/reopen proof | spec-ward fix |
| A11 | A | critical | remote cancellation projection | spec-ward fix |
| A12 | A | medium | live status child counts | spec-ward fix |
| B6 | B | high | local principal/caller derivation | map tend |
| B7 | B | high | durable local acknowledgement | map tend |
| B8 | B | medium | `JvmAdapter` bridge lineage | map tend |
| B9 | B | high | shared-UID idempotency scope | map tend |
| B10 | B | medium | calls versus mutation sequencer | map tend |
| B11 | B | high | sole resident coordinator | map tend |
| B12 | B | high | kernel versus product placement | map tend |
| B13 | B | medium | four quarry bins | map tend |
| B14 | B | medium | four-question placement test | map tend / wording-only elicitation if needed |
| E1 | E | high | Track 3 attribution gap | doc fix |
| E2 | E | high | stale forwarding deferrals | doc fix |
| E3 | E | high | stale replacement runbook workflow | doc fix |
| E4 | E | medium | stale release gates | doc fix |
| E5 | E | low | stale TODO phase heading | doc fix |

Counts: **A = 4, B = 9, C = 0, D = 0, E = 5; total = 18 findings.** One additional item is parked as an unverified lead.

## Proposed W1 adjudication order

1. **Security and outcome correctness:** A9, then A11. Both are ratified-law violations on live boundaries.
2. **Completion truthfulness:** A10, then A12. Repair the mandatory whole-path proof and make status genuinely current before using Slice 1 as the base for supervision.
3. **Slice-1 map/ledger closure:** adjudicate B6–B10 together, then E1, so tended invariant names and attribution rows are designed once.
4. **Settled migration direction:** adjudicate B11–B14 as one companion-law tend agenda. Park only exact placement-test wording to the upcoming operational-product session if durable wording cannot be reconstructed; do not reopen the settled sole-resident or kernel/product decisions.
5. **Product prose cleanup:** E2–E5 after semantic dispositions, preserving multi-Vision/transitive work at item 6.
6. **Do not expand item 6:** retain all seven ontology questions and existing deferred ledger rows unchanged.

## Gate W1

STOP. This report proposes dispositions only. No code, Allium law, Track 3 ledger, product document, or test has been changed in this pass.

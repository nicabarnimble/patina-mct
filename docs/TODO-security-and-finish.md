# Security remediation and remaining MCT work

**Status: local execution backlog; first H1 hardening slice implemented and
tested, but H1 remains open. No new issues published and no findings closed.**

This consolidates the 2026-10-20 security audit and the remaining cleanup,
reliability and runtime work. Audit baseline: `ebc0d2d6a1a055bf8157cf93ae73b2b1c2604a51`.
It is a work index, not a replacement for accepted decisions, feature specs,
[FINISH.md](FINISH.md) or [finish-checks.json](finish-checks.json).

## Tracker and review rules

- Tracker publication remains pending. Nicabar Linear tools are now available
  in the coding session; verify the live account/destination before any explicitly
  requested publication. The October 20 draft's missing-connector statement is
  historical. Tracker setup does not block local implementation.
- `nicabarnimble/patina-mct` was verified **private** during preparation. Follow
  [SECURITY.md](../SECURITY.md); do not publish undisclosed findings or exploit
  artifacts to public issues, PRs, mirrors or another audience without review.
  Recheck visibility before publication. Share only with authorized reviewers.
- Proposed organization: one parent tracking issue linking this plan, focused
  implementation/review issues, and existing issues/PRs instead of duplicates.
  Preserve the audit IDs below in every related ticket and PR. Use one tracker
  as the live owner/status source; add its links here after publication rather
  than maintaining two independent status ledgers.
- Each issue needs: affected revision and paths; attacker prerequisites or
  failure conditions; observed evidence versus hypothesis; contract and scope;
  acceptance tests; dependencies; assignee/reviewer; and closure evidence.
- All items below are open unless explicitly described as completed context.
  Audit severities are the auditor's ratings, not a completed independent
  triage. A finding can close as disproven or accepted limitation only with
  explicit reasoning and evidence, not a passing unrelated test suite.
- Reproduce hostile inputs only in disposable fixtures with bounded resources.
  No production, PiD, Unraid or assumed remote-host access is authorized.
- Keep exact failing artifacts, seeds, commands, revision and environment with
  the relevant issue/evidence record. Temporary local logs are discovery aids,
  not durable evidence other agents can rely on.

## Order and parallel work

1. Establish focused regressions and repair H1, H2 and H3/M1; retain/reproduce M3
   early because the audit reports demonstrated memory exhaustion.
2. Address remaining release-input, host-resource and control-plane boundaries.
   Review low findings for understated impact, especially L6 and L15.
3. Resolve R1/R2 before optional dependency upgrades or performance changes.
   Independent security fixes need not wait for every intermittent failure.
4. Review unique historical work (C1/C2) without wholesale merges; evaluate C3
   dependency proposals by benefit. A demonstrated security dependency fix can
   take priority over ordinary maintenance.
5. Finish F1–F8, incorporating M6/M7 and relevant low findings before enabling
   strict administration. Obtain the genuinely unresolved decisions below.

Agents can separately own Git/WASI, Iroh ingress, release parsing, and document
review. Coordinate changes to `mct-iroh/src/serve.rs`, the canonical writer and
shared fixtures; do not have independent agents silently choose conflicting
observation, revocation or ingress policies. Review work may proceed in parallel;
mutations to the same checkout must be coordinated.

## Security findings: high

Paths below are relative to `crates/` unless otherwise stated. Line numbers in
the source audit refer to the baseline and should not be treated as stable.

### H1 — Git Toy can cross the WASM execution boundary (High)

- **Evidence:** audit and follow-up source tracing connect a writable project
  preopen (including `.git`) to native Git honoring Child-controlled config.
  An existing stderr-redaction test intentionally runs a pre-commit hook;
  it is not a sandbox regression. No end-to-end malicious Child test was run
  during the follow-up review.
- **Progress, 2026-10-24:** three regressions reproduced native execution through
  hooks, fsmonitor and signing helpers. The shared Git command builder now
  suppresses those paths; 18 Toy tests pass and normal stage/commit is covered.
  Arbitrary filters and writable `.git` metadata remain unisolated: **H1 is not
  closed**. See [first-slice evidence](H1-git-toy-evidence.md) for exact scope,
  commands, workspace failure/reruns, and the next metadata-isolation slice.
- **Paths:** `mct-daemon/src/toy.rs`, `daemon/cli_runtime.rs` and
  `daemon/resident/execution.rs` under `mct-daemon/src/`.
- [ ] Prove the grant composition in an isolated Child fixture, then prevent
  Child-controlled repository metadata/configuration/attributes and inherited
  environment from authorizing native commands. Environment cleanup alone is
  insufficient; separate trusted Git metadata from writable guest storage and
  audit every exposed Git operation and command-execution mechanism.
- **Acceptance:** adversarial hooks, filters/helpers and config cannot cause
  native execution; ordinary authorized edit/stage/commit still works; path,
  option-injection and stderr-redaction protections remain covered.

### H2 — Wire-selected origin can turn a peer into a local principal (High)

- **Initial audit evidence:** transport fields were overwritten, `call.origin`
  was not constrained; resident ingress and remote candidate selection trusted
  it. No adversarial three-Mother test had been run at audit time.
- **Paths:** `mct-iroh/src/serve.rs`; `mct-daemon/src/daemon/resident/`
  `pipeline.rs`, `candidates.rs`, `forwarding.rs`, `idempotency.rs`.
- [x] Local implementation: enforce Iroh origin and admitted node/Vision at
  the authenticated peer boundary, refuse unbound user claims, and derive
  resident context from trusted ingress provenance rather than wire origin.
  Remote ingress cannot construct local context or switch idempotency scope.
- **Local proof atop R2 checkpoint `39239f4`:** before the fix, A's forged
  `Cli` origin made B forward to C and execute its WASM child. The retained
  counterexample now fails closed; all five local origins and spoofed upstream
  identities are covered, with successful local/direct-peer controls. Five
  additional three-Mother runs and 763 workspace tests passed, as did Clippy,
  fmt and tier-0 specs. See [H2 call-origin evidence](H2-call-origin-evidence.md).
  Review/merge and formal completion-inventory closure remain separate.
- **Acceptance:** enumerate all forged origins; a three-Mother test proves no
  unauthorized extra forwarding hop; local work and legitimate direct peer
  calls still succeed; peer caller identity and retry budgets cannot be changed
  by choosing another origin or spoofing local caller fields.

### H3 — Unadmitted traffic consumes canonical ledger resources (High)

- **Evidence:** denied/malformed requests reach durable observation writes in
  source. Disk-exhaustion and listener-exit claims still need retained fault
  tests. Endpoint keys are transport-authenticated, not MCT-admitted. Frames
  have a 96 KiB bound; tight identifier and aggregate traffic budgets are absent.
- **Paths:** `mct-iroh/src/serve.rs`, resident observation sink/writer integration.
- [ ] Bound pre-admission work and identifiers; avoid unnecessary authority
  fetches for unadmitted calls; specify bounded rejection diagnostics and
  resource isolation from authority-critical commitment. Cover admitted-peer
  abuse as well as arbitrary newly generated endpoint keys.
- **Owner decision, 2026-10-25:** keep `layer/core/safety-boundaries.md` Rule 3
  and the reliability doctrine in force. Required rejection observations stay
  `BeforeEffect`; durable write failure still denies the response/effect. Do not
  adopt the audit's “no durable facts before hello admission” proposal.
- [x] Local identifier/provider/error-isolation slice atop `d974af5`: validate
  wire IDs before facts (256-byte ASCII fields, 8 KiB aggregate), use local
  correlation for malformed requests, and check call validation/exact hello
  before either authority provider. Connection errors remain task-local;
  projection queues cannot pin serving. Required facts are still awaited.
  Validation: 781 workspace tests, 56 Iroh tests, five additional 8-test ingress
  runs, Clippy, fmt and tier-0 specs passed; 2 workspace tests remain ignored.
  See [H3/M1 ingress evidence](H3-M1-iroh-ingress-evidence.md).
- **Still open:** sustained-load/fairness and total ledger-volume bounds are
  not established by per-request limits; independent review and formal closure
  remain separate.
- **Acceptance:** sustained denied/malformed traffic has named storage/work
  bounds; independent local work and admitted peers remain usable within the
  documented load envelope; genuine canonical durability failure still denies
  protected effects; listener failure behavior is explicit and fault-tested.

## Security findings: medium

### M1 — Pre-admission connection-slot exhaustion (Medium)

- **Evidence:** source-traced semaphore acquisition before admission; audit
  reports 64 slots held for up to 30 seconds, not a demonstrated load test.
- **Path:** `mct-iroh/src/serve.rs`.
- [x] Local deadline/occupancy slice atop `d974af5`: separate 3-second handshake
  and combined first-stream/frame budgets from post-frame execution; bound
  response write/close waits; cap incoming bidi/uni streams to 1/0 with 128 KiB
  receive windows. Deterministic controls cover idle/partial frames, extra
  streams, response-holding peers and longer legitimate execution.
  See [H3/M1 ingress evidence](H3-M1-iroh-ingress-evidence.md).
- [ ] Establish sustained-load/fair admission guarantees; endpoint-key limits
  alone do not stop attackers creating new keys. Coordinate with H3. Local
  implementation is not independent review or formal finding closure.
- **Acceptance:** idle, slow and excess-stream clients release resources within
  named bounds; legitimate peers and local work retain the documented service
  guarantees; tests cover shutdown and failed handshakes.

### M2 — Release verification/extraction reopens mutable input (Medium)

- **Initial audit evidence:** multiple archive opens and a final hash check;
  exploitation required attacker-controlled storage and had not been reproduced.
- **Path:** `mct-daemon/src/release.rs`.
- [x] Local fix atop H2 checkpoint `983b2a3`: verify and extract one bounded,
  private immutable byte snapshot. Bounded sidecar reads and generated verified
  sidecars remove the post-verification source reopen. Deterministic source
  mutation tests cover both rename and in-place writes during/after staging.
  Extraction is owner-only; its parent remains an operator-controlled boundary.
  See [M2/M3 release evidence](M2-M3-release-boundary-evidence.md) for that trust
  assumption, cleanup controls and the 773-pass workspace validation.
- **Acceptance:** swaps or in-place changes between scan/hash/extraction cannot
  install bytes different from those verified; failures clean staging safely;
  normal release verification and path/type restrictions still pass.

### M3 — Tar extension metadata bypasses effective resource limits (Medium)

- **Evidence:** audit reports a roughly 1 MB archive causing 1.1 GB memory use.
  The reproducer and execution evidence have not yet been attached or rerun by
  this agent. Source bounds ordinary entries after tar extension processing.
- **Path:** `mct-daemon/src/release.rs` (`scan_archive_reader`).
- [x] Local fix atop `983b2a3`: a hard 520 MiB decompressed-stream budget sits
  before tar in preflight, scan and extraction. Raw-header preflight bounds
  aggregate GNU/PAX metadata to 8 MiB before tar allocates extension buffers.
  Overflow is an error, including padding beyond tar EOF. Valid bounded releases
  and GNU/PAX metadata pass; deterministic small-budget counterexamples are
  retained. This checkout uses gzip, not zstd.
- [ ] Obtain the original audit PoC and complete the extended process-resource
  and fuzz campaign. The local work does not claim the reported 1.1 GB RSS
  reproduction. See [M2/M3 release evidence](M2-M3-release-boundary-evidence.md).
  Review/merge and formal completion-inventory closure remain separate.
- **Acceptance:** extension/header bombs fail with bounded memory and work;
  fuzz cases cover compressed/decompressed/metadata limits, truncation and
  valid boundary-sized releases; retain the original regression artifact.

### M4 — Host-call observations lack per-invocation budgets (Medium)

- **Evidence:** source-traced accumulation; no sustained-load proof yet.
- **Paths:** `mct-daemon/src/wasm.rs` and Toy observation/writer paths.
- [x] Local M4/L15 slice atop H3/M1 checkpoint `039a277`: invocation-local
  host/logging/measure/Toy counters, conservative observation reservations before
  further effects, and accumulated MCT payload/output byte caps. Exhaustion traps
  with a failed report while retaining prior Toy facts; required durability and
  authority/order checks are unchanged. Native Git output is bounded while read.
  Exact-limit controls, noisy logging/measure, initialization, output exhaustion
  and scalar Toy regressions are retained. See
  [M4/L15 WASM evidence](M4-L15-wasm-boundary-evidence.md).
- [ ] Complete sustained-load, process-resource/ABI allocation assurance and
  independent review. The per-invocation accounting envelope is not a total RSS
  governor, standard-WASI byte budget, or cross-invocation ledger-growth bound.
- **Acceptance:** logging/measure abuse cannot grow host memory or ledger work
  without limit; excess work terminates predictably; mandatory effect evidence
  is not silently discarded; ordinary calls remain functional.

### M5 — Optional HTTP control is unauthenticated and fragile (Medium)

- **Evidence:** audit code tracing; impact depends on listener exposure.
- **Paths:** `mct-daemon/src/control.rs`, `mct-daemon/src/daemon/control.rs`.
- [x] Local M5/L2/L3 transport slice atop M4/L15 checkpoint `68280db`:
  isolate HTTP/UDS/startup connection failures with bounded tasks, complete-header
  framing, request byte caps and absolute read/write deadlines. HTTP binds are
  loopback-only by default; only standalone read-only diagnostics offer the
  explicit `--allow-unauthenticated-non-loopback-http` opt-in. No mutation route
  or administrative authority was added. Required durability is unchanged.
  Validation: 810 workspace tests passed (2 pre-existing ignored), 57 focused
  control tests and 2 CLI boundary tests passed; Clippy, fmt and tier-0 specs
  passed. See [M5/L2/L3 control evidence](M5-control-plane-boundary-evidence.md).
  This slice is checkpointed separately; independent verification remains open.
- [ ] Complete external-consumer/retirement review, sustained-load fairness and
  independent assurance before formal closure. Never expose unauthenticated
  mutations off loopback; loopback is not proof of ownership, and this work does
  not replace D-57's authenticated administration cutover.
- **Acceptance:** malformed/idle clients cannot permanently stop the listener;
  configured exposure matches the documented authority boundary. Any retained
  transport is consistent with D-57's removal of legacy admin alternatives.

### M6 — Legacy roots can feed new issuance/recovery transitions (Medium, latent)

- **Evidence:** writer/apply paths accept legacy canonical roots without a
  genesis prerequisite. Current startup rejects these new authority profiles;
  this is not a demonstrated exposed strict-admin exploit.
- **Paths:** `mct-observation/src/admin_issuance.rs`, `guardian_recovery.rs`,
  `lib.rs`; `mct-daemon/src/startup.rs`.
- [ ] Enforce D-57 provenance at every production writer/replay/activation
  boundary before strict ingress is enabled. Preserve inspectability of old
  history without silently turning it into active strict authority.
- **Acceptance:** unsigned/self-signed legacy roots cannot become operational
  strict roots through issuance or enrollment; genuine genesis and subsequent
  valid recovery/issuance work; old hashes and explicit startup denial survive.
- **Dependency:** F1/F4; distinguish historical decoding from live admission.

### M7 — Strict profile lacks targeted day-key revocation (Medium, latent)

- **Evidence:** legacy mutation path is denied for new profiles; strict runtime
  remains disabled. Missing implementation, not proof of a live exploit.
- **Paths:** `mct-observation/src/lib.rs`, canonical admin command/evaluator paths.
- [ ] Add typed, signed canonical day-key revocation under the accepted D-56
  authority model. Review issuer lineage, root powers and descendant behavior;
  surface any genuinely unspecified rule rather than inventing one.
- **Acceptance:** authorized targeted revocation persists across replay/restart
  and blocks subsequent admissions; unrelated grants survive as specified;
  unauthorized, stale and replayed requests cannot mutate authority; guardian
  root replacement remains governed separately by D-54/D-55.
- **Dependency:** F1/F3/F4; no enabling strict ingress without the required path.

## Security findings: low (individual triage still required)

Each row is a separate finding to retain in the tracker. All are audit reports,
not independently reproduced exploits. Scope/severity may change with evidence.

| ID | Reported boundary and source | Work and closure evidence |
| --- | --- | --- |
| L1 | Default permissions in unsupervised startup: `mct-daemon/src/config.rs`, `state.rs` | [ ] Verify config/state/ledger creation and parent-directory permissions under permissive umask; enforce intended ownership/modes and test both supervised and unsupervised startup. |
| L2 | CLI trusts working-directory control socket and unbounded replies: `mct-daemon/src/daemon/cli_admin.rs` | [x] Local client slice: bound full exchanges and response bytes; check socket type/mode/owner, parent/ancestors, inode continuity and live peer UID before payload delivery. Metadata/protocol/timeout errors never trigger offline mutation fallback. Status, mutation and lifecycle clients share the boundary. [M5/L2/L3 evidence](M5-control-plane-boundary-evidence.md). [ ] Same-UID process authentication, legacy transport retirement, independent review and formal closure remain open. |
| L3 | Startup plane can exit/hang on malformed or idle same-UID client: `mct-daemon/src/daemon/resident/serving.rs` | [x] Local startup slice: bounded concurrent socket IO, strict framing and connection-local failures preserve later health requests and idle-peer shutdown; authority operations stay serialized and required durability is not timed out. Malformed-request tests preserve exact ledger bytes. [M5/L2/L3 evidence](M5-control-plane-boundary-evidence.md). [ ] Sustained-load/platform assurance and independent closure remain open; no shared cause with R1 is assumed. |
| L4 | Supervisor binary ownership/parent/symlink checks: `mct-daemon/src/daemon/supervisor_lifecycle.rs` | [ ] Establish the trusted executable publication boundary; test writable-parent, replacement and symlink cases. Coordinate with the unresolved trusted lifecycle decision rather than adding an authority bypass. |
| L5 | Offline mutations recorded as `os-uid:0`: `mct-daemon/src/daemon/control.rs` | [ ] Prove attribution bug and correct any retained path, or remove it through D-57 cutover; no fabricated root caller attribution. Preserve existing history rather than rewriting it. |
| L6 | Caller-provided project/classification/secret metadata used in authorization: `mct-kernel/src/child.rs`, `toy.rs` | [x] Separately checkpointed local fail-closed slice: caller labels cannot satisfy project/classification-scoped Child/Toy authority without independent Mother facts; capabilities bind invocation metadata and Child outputs remain conservatively local without declassification proof. No production attestation resolver is invented, and grants are not broadened to recover old behavior. [L6/L16 evidence and compatibility restrictions](L6-L16-authority-binding-evidence.md). [ ] Trusted attestation/output classification, severity reassessment, independent review and formal closure remain open. |
| L7 | Hello capability view not restricted to peer Vision: `mct-daemon/src/daemon/resident/serving.rs` | [ ] Prove exposure with distinct audiences; limit disclosed inventory to authorized scope and preserve non-leaking denials. Test within supported scope without inventing multi-Vision hosting. |
| L8 | Peer IDs may contaminate restart evidence: `mct-daemon/src/daemon/resident/restart_recovery.rs` | [ ] Separate caller correlation from Mother-issued durable identity; malicious duplicate/colliding IDs cannot fabricate recovery status or fence unrelated work. Coordinate with H2/H3. |
| L9 | Forwarding creates another endpoint with Mother's key: `mct-daemon/src/daemon/resident/forwarding.rs` | [ ] Reproduce any relay reachability interference and use an appropriate owned endpoint lifecycle; concurrent outbound forwarding cannot disrupt inbound reachability or shutdown. |
| L10 | Legacy nonce reuse checks lack ordering/epoch binding: `mct-kernel/src/admin_sig.rs`, `mct-observation/src/lib.rs` | [ ] Identify the exact required freshness contract and any reproducible replay across authority changes; nonce ordering is not inherently required. Fix a demonstrated gap or document why unique request IDs plus other bindings suffice; do not change D-56 semantics casually. |
| L11 | Pairing-ticket consumption ignores window/peer: `mct-observation/src/lib.rs` | [ ] Trace all production consumers; enforce the accepted ticket target/time/single-use contract at the authoritative boundary; wrong-peer, early, expired and replayed attempts deny. |
| L12 | Ordinary append may impersonate authority `detail_ref`: `mct-observation/src/lib.rs` | [ ] Reserve authority namespaces/typed append boundaries; malicious ordinary facts cannot be interpreted as authority or poison replay. Test allowed ordinary facts and malformed reserved references. |
| L13 | Canonical reserialization hash and checkpoint rollback handling: `mct-observation/src/lib.rs` | [ ] Review these as two distinct claims. Canonical serialization is not itself a vulnerability. Prove required representation/continuity properties; retain rollback evidence and deny discontinuity without silently changing historical hash contracts. |
| L14 | Corrupt SQLite rows panic/crash-loop: `mct-daemon/src/state.rs` | [ ] Preserve corrupt fixtures; return typed degraded/quarantine outcomes with operator evidence, no guessed defaults or destructive repair; startup and inspection remain predictable. |
| L15 | Verified WASM reopened per call; integrity mode may only audit: `mct-daemon/src/children.rs`, `wasm.rs` | [x] Local execution-byte binding atop `039a277`: bounded immutable bytes are verified against the authorized artifact digest and passed directly to `Component::from_binary`; unverified typed inventory denies. Retained overwrite/rename counterexample and post-verification source-removal control pass. See [M4/L15 evidence](M4-L15-wasm-boundary-evidence.md). Independent review, severity reassessment alongside H1/F3, and formal closure remain open; the separately checkpointed L16 slice is tracked below. |
| L16 | Acquisition expected digest covers WASM, not manifest: `mct-daemon/src/acquisition.rs` | [x] Separately checkpointed local slice: paired component/source-manifest pins, exact operator-decision binding, one bounded package-manifest snapshot, separate installed-manifest approval pin, and execution checks against both approved bytes and cached authority fields. Same-WASM changed permissions plus recomputed sidecars cannot reuse approval; historical approvals without a pin are readable but inert. Daemon release archives still reject Child members. [L6/L16 evidence](L6-L16-authority-binding-evidence.md). [ ] Independent review, source/publisher trust assurance, migration review and formal closure remain open. |
| L17 | Blanket yanked-crate waiver: root `.cargo/audit.toml` and lockfiles | [ ] Verify current yank reasons, fixed versions and transitive constraints for spin/chacha20/der; remove or narrowly justify exceptions with a revisit condition. Validate all retained graphs; an available version is not proof of compatible resolution. |
| L18 | Mutable Rust setup action ref: root `.github/actions/setup-rust/action.yml` | [ ] Resolve and verify an immutable upstream commit; preserve explicit Rust 1.96.0 selection and the compiler-version check; validate CI installation. Pinning the action must not silently change the compiler. |
| L19 | Unused install function may permit traversal: `mct-daemon/src/registry.rs` | [ ] Verify all callers and external API exposure; remove genuinely unused code or validate names and destination containment. Cover traversal/platform path forms if retained. |

## Runtime reliability investigations

### R1 — Intermittent resident control-socket readiness failure

- **Observed:** `resident control socket did not bind before readiness` in
  `mother_survives_sigkill_mid_run_with_active_children` and
  `six_mothers_independent_revisions_survive_peer_loss_and_restart`.
  Recorded restart case: seed `0x6d637421`, iteration 0,
  `MidRun { delay_ms: 190 }`, boot 1.
- **Sources:** `docs/FINISH.md`, strict-cutover feature evidence; original local
  logs under `/tmp/mct-genesis-evidence/` are not guaranteed portable/persistent.
  H1 validation reproduced this on 2026-10-24: seed `0x6d637421`, iteration 2,
  `MidRun { delay_ms: 718 }`, boot 1. An isolated rerun and the full workspace
  rerun passed unchanged; this is not a fix. The first failure log, but not its
  cleaned-up fixture, is retained. See [H1 evidence](H1-git-toy-evidence.md).
- [ ] Retain startup logs, child exit status, readiness transitions and timing;
  distinguish failed startup, slow startup and faulty observation. Restore
  deterministic injection seams rather than production-only debugging hooks.
- **Local fix after H1 checkpoint `7aafdcc`:** replaced stale-socket pathname
  polling with a listener-owned startup acknowledgment and task-error propagation.
  Retained timestamps show a ~236 ms failure, not an exhausted two-second wait.
  The deterministic regression failed with the original algorithm; all four new
  tests, 30 seeded SIGKILL iterations, and the 755-pass workspace run passed with
  the fix. See [R1 readiness evidence](R1-readiness-evidence.md). Review/merge and
  formal inventory closure remain separate.
- **Acceptance:** demonstrated cause, regression that fails without the fix,
  repeated relevant process suites with retained diagnostics. No unexplained
  timeout increase, weaker assertion or “rerun passed” closure.

### R2 — Intermittent fleet route-revalidation denial

- **Observed:** `mother-0 -> 2` unexpectedly returned `denied`, decision
  `route-revalidation:mother-0-to-2-connected`, instead of `completed` during
  archive-retirement validation. Proof and full-suite reruns passed unchanged.
- **Sources:** `mct-daemon/tests/federation_fleet.rs`, `docs/FINISH.md`, PR #82;
  original local logs under `/tmp/mct-retire-traces-evidence/`. The first failed
  fixture ledger was not retained; do not claim it exists.
- [ ] Reproduce with `MCT_FLEET_KEEP=1`; preserve canonical history, projection
  generations, binding/route decisions and timing. Distinguish correct stale
  denial from a runtime race or fixture assumption.
- **Local fix after R1 checkpoint `e739001`:** the unchanged fleet reproduced
  the connected-phase denial on `mother-0 -> 3` (run 18, fixtures retained).
  Local route revalidation succeeded; a background hello overwrote the receiver's
  earlier forwarding-hello receipt for the same endpoint. Cache and lookup now
  use endpoint plus exact hello decision ID, with bounded retention and unchanged
  current-authority checks. The deterministic regression failed before the fix;
  47 Iroh tests, 20 consecutive full fleet runs, and 758 workspace tests passed
  afterward. See [R2 hello-receipt evidence](R2-hello-receipt-evidence.md).
  The original `mother-0 -> 2` fixture remains unavailable; formal closure is
  separate from this local fix and retained reproduction.
- **Acceptance:** cause-specific regression and relevant fleet/restart runs;
  no weakening stale-authority checks. Do not infer that R1 and R2 share a cause.

## Historical work and dependency review

### C1 — Review the older SHA-2/SQLite compatibility experiment

- [ ] Compare `cursor/daemon-sha2-rusqlite-fixes` at
  `14a9e11e227531ff0a5c2aa8a52aa0167b54c347` with current main and PRs #77/#79.
  Inventory uniquely useful checked unsigned-conversion and digest tests;
  port only justified work with current dependencies. Do not wholesale merge
  the old mixed Wasmtime/WASI upgrade experiment.
- **Closure:** record each unique change as ported, superseded or rejected with
  evidence; preserve necessary history before retiring the obsolete task ref.

### C2 — Assess the preserved performance experiment

- [ ] Compare `archive/20261019/perf-phase0` at
  `646b4baffa6a7129ed9d43cd4f3fcd001e9c7457` with current main and the later
  PR #36 work. Separate incremental authority-projection fixes already landed
  from missing tests/findings and residual whole-ledger status scanning.
- **Evidence:** eight preserved performance captures and Git bundle under the
  local `.patina-mct-archive/2026-10-19/`; attach relevant sanitized measurements
  for other agents rather than requiring access to this machine.
- **Closure:** documented equivalence/differences and disposition of unique
  work, preserved measurements; no claim that the blocked experiment produced
  an accepted baseline, no wholesale merge or weakened authority checks.

### C3 — Review the seven existing dependency proposals

**Security prerequisite, 2026-10-24:** shipping PR #84 exposed new advisories on
unchanged main, already tracked by [issue #83](https://github.com/nicabarnimble/patina-mct/issues/83).
Both lockfiles now contain narrowly selected fixes: Wasmtime/WASI 48.0.7 and its
coupled compiler packages, Tokio 1.53.3, ctutils 0.4.4, rustls-webpki 0.103.16,
and both webpki root packages 1.0.10. Fresh local audits and 734 workspace tests
pass; final GitHub validation is recorded in PR #84. Audit policy is unchanged.
See [shipping-gate evidence](H1-git-toy-evidence.md#shipping-gate-follow-up-dependency-security-patches).
This does not accept the optional major-version upgrades below.

All seven were verified open and draft during plan preparation. Revalidate
current heads, conflicts, security relevance and combined compatibility.
Use existing PRs, not duplicate upgrade issues; correlate L17 where relevant.

| PR | Proposal | Required review |
| --- | --- | --- |
| [#73](https://github.com/nicabarnimble/patina-mct/pull/73) | Compatible lockfile refresh | Prior disk-space failure is not a compatibility verdict; justify scope and retest with sufficient space. |
| [#75](https://github.com/nicabarnimble/patina-mct/pull/75) | getrandom 0.4 | Demonstrate benefit and unchanged secure seed/epoch generation and failure behavior. |
| [#76](https://github.com/nicabarnimble/patina-mct/pull/76) | base64 0.23 | Preserve intended scalar feature configuration; test canonical encodings and malformed inputs. |
| [#77](https://github.com/nicabarnimble/patina-mct/pull/77) | sha2 0.11 | Preserve digest bytes/hex and historical identity; compare C1 tests. |
| [#78](https://github.com/nicabarnimble/patina-mct/pull/78) | TOML 1 | Test accepted/rejected configuration and manifest compatibility, not compilation alone. |
| [#79](https://github.com/nicabarnimble/patina-mct/pull/79) | rusqlite 0.40 | Test unsigned conversions, corrupt/overflow rows and persisted state; compare C1. |
| [#80](https://github.com/nicabarnimble/patina-mct/pull/80) | Wasmtime family 49 | Current family already contains previously cited security fixes; establish new benefit and preserve WASI/Toy limits. |

- [ ] Explicitly merge justified proposals or defer/close unsupported ones.
  Optional upgrades follow R1/R2 baseline repair. Revalidate the final combined
  graph, not only isolated old CI results. Use merge commits, no force-push or
  published-history rewriting; retire completed task branches afterward.
- Zstd PR #74 was already closed without merge after retiring its direct tool
  consumer. Do not reopen it as unfinished cleanup; Zstd remains transitive.

### C4 — Correct stale active instructions without rewriting history

- [ ] Reuse [issue #72](https://github.com/nicabarnimble/patina-mct/issues/72):
  active `grants-authority-v0` and `ledger-commit-recovery` specs still contain
  executable Allium requirements. Remove active requirements or explicitly
  classify historical material; do not mark unfinished work complete merely
  to avoid checks. Preserve historical evidence statements.
- [x] Update current-status sections of `FINISH.md`, `HANDOFF-MCT-FINISH.md`
  and owner-gate/publication guidance: PRs #81/#82 are merged, the canonical
  checkout is consolidated, and draft-only merge restrictions were superseded
  for that work by explicit owner authorization. Release/deployment boundaries
  and historical decisions are preserved; the current handoff links this index.
- **Completed context, not new tasks:** checkout consolidation, redundant
  backup-reference removal, obsolete agent tooling and conversation-archive
  retirement. Keep long-lived remote `patina` and `patina-grok`; keep the
  performance experiment until C2 establishes its disposition.

## Unfinished MCT completion work

These eight IDs are already open in `finish-checks.json`; do not create a
competing release inventory. Existing kernel/writer proofs remain useful but
are not proof that their runtime consumers and production ingress are complete.

| Work | Inventory ID | Remaining work and acceptance evidence |
| --- | --- | --- |
| F1 | `administration` | [ ] Finish authenticated Iroh administration, strict hybrid checks, issuance/revocation and trusted genesis integration; remove UID/offline alternatives in the same cutover. Include M6/M7, production parser bounds, expiry, stale/replayed intents and legacy denial. |
| F2 | `canonical-authority` | [ ] Record complete typed Child approvals/assignments and peer bindings/proofs; reconstruct from canonical history, not surviving configuration. Lost/rolled-back projections cannot resurrect authority; commitment survives projection failure; old hash contracts remain stable. |
| F3 | `final-admission` | [ ] Fence final Child/effect execution against concurrent revocation and stale authority; bind executed artifact identity. Define task/invocation/effect/checkpoint recovery boundaries without inventing exactly-once execution, global call-ID fencing or changing the 720-second keyed retry contract. |
| F4 | `key-recovery` | [ ] Integrate D-54/D-55 Voice-bound guardian lifecycle into authenticated runtime, canonical consumers and final admission; prove delays, restart clock handling, cancellation, replacement/revocation and exact-snapshot resumption end to end. |
| F5 | `fleet-operations` | [ ] Provision, inspect, revoke, stop, quarantine, recover and upgrade independent instances without identity/path/service collisions. Inventory is not authority and must not become a required shared coordinator. |
| F6 | `distributed-failures` | [ ] Obtain approved disposable hosts/actions and exercise partition, reconnect, stale bindings, lost replies, retry correlation, peer restart and rolling upgrade. Prove unavailable peers do not stop unrelated local work. Same-host six-process tests are not physical multi-host evidence. |
| F7 | `useful-workflow` | [ ] Produce a real artifact and authorize publication against its exact digest; reject unauthorized requester/artifact. Demonstrate independent work during outage and no blind replay of unknown external effects. |
| F8 | `release-evidence` | [ ] Verify supported macOS arm64 artifacts, custody/setup/upgrade/recovery procedures, security claims and host matrix; finish relevant security investigations; run full release acceptance and obtain owner evidence review. No release/deployment is authorized by creating this plan. |

### Decisions and external access still needed

- [ ] Recipient-bound Child handle policy: resolve the proposal in
  [finish-owner-gates.md](design/finish-owner-gates.md); do not silently choose
  bearer versus recipient-bound authority.
- [ ] Trusted setup/supervisor lifecycle: resolve pin acquisition, virgin-state
  inventory, publication/race fencing and lifecycle authority. The existing
  proposal is not approval; genesis setup construction remains test-only.
- [ ] Identify approved disposable multi-host macOS targets, permitted actions
  and evidence to return through Devin. Do not assume direct remote access or
  extend release support to Linux/Unraid without explicit scope and proof.
- [x] H3 rejection-observation contract resolved by the owner on 2026-10-25:
  retain Rule 3 and mandatory `BeforeEffect` rejection facts; write failure
  denies the effect. No doctrine amendment or silent evidence suppression.
  Broader sustained-load/storage policy remains outside this local slice.

## Shared definition of done

For an implementation issue: retain the failing regression/input, explain the
root cause and threat prerequisites, implement the bounded fix, cover denial
and authorized success, and name the exact tests/results/revision. Have another
agent review the claimed boundary, not just formatting or test counts. Record
remaining limitations. Changed durability/parser paths need fault/fuzz evidence.

For review-only work: record a concrete disposition and supporting comparison;
“looks superseded” or “newer is better” is not closure. A passing rerun is not
an intermittent-failure fix. A clean advisory scan is not an application audit.

After relevant focused checks, run the applicable Tier-0/docs/proof checks.
Before declaring release completion:

```sh
python3 scripts/check-finish.py release
```

All eight requirements need real evidence and reviewed status changes. Merging
tested work, passing implemented proofs, publishing issues or closing this plan
is not independently release authorization.

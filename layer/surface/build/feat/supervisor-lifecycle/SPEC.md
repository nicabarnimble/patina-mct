---
type: feat
id: supervisor-lifecycle
status: approved
created: 2026-07-15
target: daily-driver-slice-2
sessions:
  origin: 20260714-160744-657025000
  work: []
related:
  - layer/allium/mct-product-map.allium
  - layer/sessions/20260714-160744-657025000.md
  - layer/surface/build/feat/resident-call-ingress/SPEC.md
  - layer/surface/build/product/MOTHER-REPLACEMENT-RUNBOOK.md
  - layer/surface/build/product/MCT-NEXT-BUILD-TODO.md
  - layer/surface/build/spec-drift-audit/track3/LEDGER.md
  - layer/core/migration-vocabulary.md
  - crates/mct-daemon/src/main.rs
  - crates/mct-daemon/src/daemon/control.rs
  - crates/mct-daemon/src/daemon/resident/serving.rs
  - crates/mct-daemon/src/daemon/resident/observation.rs
beliefs:
  - mother-kernel-decides-adapters-perform
  - mother-is-the-daemon
exit_criteria:
  - id: lifecycle-command-surface
    text: mct-daemon exposes install, uninstall, start, stop, and restart as macOS user-launchd lifecycle commands; serve remains the foreground resident entry point, status remains a projection, and unsupported platforms fail before effects.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon supervisor_command_surface_is_explicit_and_macos_only -- --nocapture
  - id: governing-supervisor-record
    text: A supervised start accepts only an owner-private, ledger-backed, digest-valid current supervisor record whose exact revision binds the operator provenance, executable, launchd policy, and absolute resident paths used by the process.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon supervised_start_rejects_unobserved_tampered_or_stale_records -- --nocapture
  - id: bootstrap-install-order
    text: Fresh install creates only the minimal observer substrate before exclusive writer acquisition and a durable first install-attempt batch; identity, runtime state, supervisor record, plist, and all other effects follow that batch, while append failure and concurrent install suppress effects.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon supervisor_install_bootstrap_is_observed_before_every_remaining_effect -- --nocapture
  - id: resident-conflict-guards
    text: An active managed install refuses manual serve, a manual resident refuses managed start/install, a second plain install refuses, and the exclusive ledger writer remains the definitive same-node resident/bootstrap gate; each durable refusal is observation-bearing.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon supervisor_conflicts_refuse_before_launchd_or_endpoint_effects -- --nocapture
  - id: clean-and-unclean-lifecycle
    text: Supervised boot derives initiator provenance from the exact installed record, clean stop/restart records shutdown while the writer is available, and a start after unclean death records the unmatched prior instance as reconciliation before readiness.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon supervisor_lifecycle_install_start_stop_unclean_reconcile_uninstall_preserves_evidence -- --nocapture
  - id: uninstall-preserves-evidence
    text: Uninstall unloads and removes only current supervision policy, the managed plist, and the current supervisor record; it preserves the observation ledger, state database, identity, children, blobs, and logs, and records both no-op and state-changing attempts.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon supervisor_lifecycle_install_start_stop_unclean_reconcile_uninstall_preserves_evidence -- --nocapture
  - id: writer-loss-fencing
    text: Known resident writer loss makes readiness false and prevents new calls and every lifecycle action except safety stop; no protected effect result is acknowledged or cached, and explicit observer recovery remains unavailable in this slice.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon resident_writer_loss_fences_lifecycle_and_all_other_protected_effects -- --nocapture
  - id: adapter-boundary
    text: Generic lifecycle orchestration depends on a supervisor adapter contract; launchd is the only production adapter, tests use a deterministic fake, and neither launchctl nor systemd behavior enters mct-kernel.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon launchd_adapter_maps_install_start_stop_and_restart_without_ambient_fallbacks -- --nocapture
  - id: executable-digest-strictness
    text: A binary swap without install --replace fails supervised boot before resident effects, reports the executable-digest mismatch class and remediation safely, and never blesses replacement bytes implicitly.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon supervised_start_rejects_unblessed_binary_swap_with_replace_guidance -- --nocapture
  - id: gui-domain-limitation
    text: The launchd adapter targets only gui/<uid>; unavailable GUI domains fail without trying user, system, Homebrew, or detached-process fallbacks.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon launchd_adapter_refuses_missing_gui_domain_without_fallback -- --nocapture
  - id: attribution-ledger
    text: Every behavior-changing MctOperationalSelfObservation obligation implemented by this slice has a COVERED Track 3 row naming landed tests, or a named operator-approved waiver.
    checked: false
    verify: bash -lc 'rg -n "MctOperationalSelfObservation|supervisor_lifecycle" layer/surface/build/spec-drift-audit/track3/LEDGER.md'
  - id: workspace-validation
    text: The phase passes the required workspace validation suite.
    checked: false
    verify: cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
---

# feat: Supervisor install and lifecycle (launchd)

> An authenticated local operator can install one ledger-backed launchd policy for Mother, and every install, boot, start, stop, restart, reconciliation, and uninstall attempt obeys the operational observation and writer-fencing law.

## Problem

The resident has authenticated local call ingress and real status, but normal operation still requires a terminal running `mct-daemon serve`. There is no supported installation record, no macOS supervisor adapter, no lifecycle conflict guard, and no durable distinction between a clean shutdown and an unclean disappearance.

The existing first-boot path also records a generic operator identity action whenever `serve` starts. That is not sufficient for supervised operation: launchd boot must inherit initiator provenance from the installed governing record and must not fabricate a new boot-time operator authentication.

This slice translates useful operational lessons from `patinaMother` and rebuilds them under `MctOperationalSelfObservation`. The legacy implementation is evidence only. Its environment marker, best-effort launchctl failures, fallback backend selection, and ambient file-presence trust are not authority and are not ported.

## Goals

1. Install one user launchd policy for the current macOS UID.
2. Make the current supervisor record ledger-backed, owner-private, revisioned, and sufficient to derive boot-time initiator provenance.
3. Observe every lifecycle attempt and adapter effect that can be observed, before acknowledgement and before the protected effect where required.
4. Compose first install with the ratified minimal-substrate and exclusive-writer bootstrap ordering.
5. Refuse managed/manual/duplicate coexistence before endpoint or launchd effects.
6. Preserve state, identity, and all evidence across stop, restart, and uninstall.
7. Detect unmatched prior resident instances and reconcile them before readiness.
8. Fence the resident on known writer loss.
9. Keep lifecycle orchestration backend-neutral so systemd can be a later adapter rather than a rewrite.

## Non-Goals

- No systemd implementation or Linux service files.
- No package manager, Homebrew service, binary acquisition, upgrade, or rollback behavior.
- No registry/artifact acquisition or update behavior.
- No trigger, scheduler, event-source, or `CallTriggerAuthority` implementation.
- No general reconciliation engine beyond supervised instance continuity.
- No ledger retention, archival, export, deletion, compaction, or repair command.
- No JVM SDK.
- No new kernel observation kind or lifecycle action represented as an `MctCall`.
- No change to child process supervision.
- No claim that a PID file, socket file, plist, launchctl result, environment variable, or external receipt is authority or an evidence ledger.

## D1 Decisions

### D1.1 — The command surface is top-level and supervision-specific

The production command surface becomes:

```text
mct-daemon install [--root <absolute-path>] [--executable <absolute-path>] [--replace] [--json]
mct-daemon uninstall [--root <absolute-path>] [--json]
mct-daemon start [--root <absolute-path>] [--json]
mct-daemon stop [--root <absolute-path>] [--json]
mct-daemon restart [--root <absolute-path>] [--json]
mct-daemon status [--uds <socket-path>] [--json]
mct-daemon serve ...
```

`install`, `uninstall`, `start`, `stop`, and `restart` manage the launchd-backed resident. They do not alias child lifecycle, registry install, or foreground process spawning.

`serve` remains the foreground resident process entry point. The launchd plist invokes a supervised form of `serve` bound to an exact supervisor-record path. That form is process plumbing, not a sixth operator lifecycle action. A normal operator runs `start`; launchd runs `serve`.

`status` remains a read-only UDS projection and creates no lifecycle observation. It is not renamed or duplicated. Lifecycle commands may use the same status probe to wait for a requested state, but a probe is never evidence that the effect was authorized or observed.

Only macOS is supported in this slice. On another platform, lifecycle commands return `unsupported platform` before creating a directory, ledger, record, unit, or observation. There is no silent manual-process fallback.

The default service root is the current OS account's `~/.mct`, resolved for the authenticated UID rather than trusted from a client-authored identity. `--root` and `--executable` are explicit operator selections, must resolve to absolute paths, and exist to support controlled installs and isolated verification. They do not select an operator identity. The executable must be an absolute, regular, executable file. No CLI flag selects another UID, launchd domain, label, record identifier, plist path, or launchctl binary.

**Rationale:** Top-level commands match the existing `serve`/`status` operational vocabulary and avoid a parallel wrapper product. Explicit service commands also prevent `start` from ambiguously meaning “spawn an unmanaged background process.”

### D1.2 — One current supervisor record governs supervised boot

The current record is canonical JSON stored at `<root>/supervisor.json`, mode `0600`, beneath an owner-only (`0700`) service root. It is product state, not kernel ontology and not a second evidence ledger.

Its v1 shape is:

```text
SupervisorRecordV1 {
  schema_version                 # exactly 1
  record_id
  record_revision               # starts at 1; increases by one
  record_state                  # active only in a current on-disk record
  backend                       # launchd_user
  service_label
  launchd_domain                # exact gui/<authenticated-uid>
  owner_uid
  created_by_uid
  created_at
  creation_observation_id
  last_revised_by_uid?
  revised_at?
  revision_observation_id?
  record_digest                 # BLAKE3 of canonical fields excluding this field
  executable_path
  executable_digest             # BLAKE3 of installed executable bytes
  plist_path
  plist_digest                  # BLAKE3 of exact rendered plist bytes
  config_path
  identity_path
  children_dir
  state_path
  ledger_path
  uds_path
  stdout_log_path
  stderr_log_path
}
```

Every path is absolute and every path used by supervised `serve` comes from this record. The plist contains only the absolute executable, the supervised-record argument, standard launchd policy, and log paths; it does not carry a second copy of runtime configuration.

Before a supervised process performs identity, state, endpoint, socket, or readiness effects, it must verify:

1. record ownership/mode, schema, backend, UID, state, and revision;
2. canonical `record_digest`;
3. executable path and digest;
4. exact plist path and digest;
5. launchd process context for the record's `gui/<uid>` service; and
6. the ledger entry named by `creation_observation_id` or `revision_observation_id`, including the same record id, revision, digest, UID provenance, and policy action.

An environment variable is not sufficient process-context proof. Production verifies the launchd parent/service context and the exact record/plist binding. The deterministic test adapter supplies an explicitly typed simulated-supervisor context; that seam is unavailable in the production CLI.

The record's creation provenance is the authenticated effective OS UID that performed install. A revision retains creation provenance and adds the current revising UID and observation. No client-supplied JSON can claim these fields.

Plain `install` refuses an existing current record, even if it appears identical. `install --replace` is the only record-revision path. Replacement requires a valid ledger-backed predecessor and an unloaded/stopped service, increments the revision, and observes the revision before replacing record or plist. It may reconstruct a missing managed plist, but it refuses to overwrite plist bytes that do not match the predecessor's digest. Unknown or tampered policy requires explicit operator resolution; `--replace` is not a force-through-integrity switch.

Record creation, revision, and removal are lifecycle actions and are observed. The current file may be removed only by successful uninstall; its complete history remains in the ledger.

**Rationale:** A plist says what launchd may execute, but cannot truthfully establish who installed that policy. The ledger-backed record joins current launch policy to install-time authentication without inventing boot-time authentication.

### D1.3 — Fresh install is the bootstrap transaction

Install first performs read-only discovery. The canonical discovered-state summary covers at minimum:

- service root, ledger, config, identity, state, supervisor record, and plist presence;
- record/plist validity or mismatch when present;
- launchd loaded/running state for the fixed label; and
- UDS reachability and ledger-writer contention.

On a fresh root, the only pre-observation writes permitted are:

1. the owner-only service-root/ledger parent required to host the observer;
2. the empty ledger file; and
3. the ledger's exclusive-writer mechanism.

Those objects establish no identity, install, policy, or readiness. After acquiring the exclusive ledger writer, install writes one required `BeforeEffect` bootstrap batch containing the authenticated UID, pending installation subject, generated record id/revision/digest, intended `install` action, and canonical discovered-state summary. No later effect begins until the complete batch is durable.

Remaining effects then occur under the same held writer, in this order:

1. create or validate local identity and config through the shared observed-identity logic;
2. create/validate runtime state and required owner-private directories;
3. stage the exact supervisor record and plist on the target filesystems;
4. atomically publish the record first;
5. atomically publish the plist second; and
6. append adapter completion and lifecycle completion before acknowledging install.

Publishing the record before the plist guarantees that a launchd-visible policy never points at an absent governing record. A failure after record publication but before plist publication leaves a non-running, observed incomplete install for explicit reconciliation; it does not claim success.

`install` writes the enabled `RunAtLoad`/`KeepAlive` policy but does not issue `launchctl bootstrap` and therefore does not intentionally start the resident in the current session. `start` is the explicit current-session effect. At a later GUI login, launchd may start the installed `RunAtLoad` policy directly from the same governing record.

Install over existing state uses the same first attempt batch and records exactly what it found. It never rewrites identity, state, or evidence as “fresh.” Existing valid identity is preserved. Existing invalid or ambiguous state fails closed after a durable failure/refusal fact.

If first append fails, no identity, config, state, supervisor record, plist, launchctl, endpoint, or readiness effect occurs and success is not printed. Minimal observer residue may remain for a later admitted reconciliation.

The exclusive ledger writer is the single-bootstrap gate. A concurrent installer that observes contention is permanently classified as the losing attempt: it performs no protected effect. It may wait only to acquire the released writer and append its already-decided contention refusal; it cannot turn that same attempt into an install after the winner exits.

**Rationale:** This is the ratified bootstrap ordering applied to a concrete install. A separate installer receipt or filesystem transaction log would create a forbidden parallel evidence channel.

### D1.4 — launchd is an adapter, not lifecycle authority

Generic orchestration depends on a narrow product adapter contract:

```text
SupervisorAdapter {
  inspect(record) -> SupervisorInspection
  publish_policy(staged_plist, record) -> EffectResult
  start(record) -> EffectResult
  stop(record) -> EffectResult
}
```

`restart` is deliberately composed by generic orchestration as clean `stop` followed by `start`; the adapter does not gain an opaque force-restart primitive. Install/uninstall file publication is orchestrated around the adapter so ordering and preservation law are shared by future backends.

The only production implementation is `LaunchdSupervisorAdapter`:

- label: `io.patina.mct.mother`;
- plist: `<uid-home>/Library/LaunchAgents/io.patina.mct.mother.plist`;
- domain: exactly `gui/<uid>`;
- start when unloaded: `launchctl bootstrap gui/<uid> <plist>`;
- stop when loaded: `launchctl bootout gui/<uid>/io.patina.mct.mother`;
- loaded-state inspection: `launchctl print gui/<uid>/io.patina.mct.mother`.

A loaded and ready start is an observed successful no-op reconciliation. An already-unloaded stop is likewise observed as a successful no-op. Any other non-zero launchctl result is a typed adapter failure; it is not ignored and does not fall through to `user/<uid>`, Homebrew, manual spawn, or another backend.

The plist uses `RunAtLoad=true`, `KeepAlive=true`, bounded launchd restart throttling, explicit stdout/stderr files, and no shell. XML values are escaped. It contains no secret and no ambient “supervised” environment marker.

Tests use a `FakeSupervisorAdapter` implementing this exact contract and a typed simulated launch context. Production code cannot select it through arguments or environment.

**Rationale:** Kernel and orchestration decide what may happen and in what order; launchd performs only the inspected effect. One exact domain and typed failures replace the legacy “try domains and ignore errors” behavior.

### D1.5 — Conflict guards use current record plus exclusive ownership

The following coexistences are refused before launchd, endpoint, identity, config, or state effects:

| Attempt | Detected state | Result |
|---|---|---|
| manual `serve` | a valid active managed record/plist exists for the fixed label | refuse; operator must use lifecycle commands or uninstall |
| managed `start` | the same-node ledger writer is held by a manual resident | refuse; do not bootstrap launchd |
| managed `start` | record/plist/revision/digest/process context is absent, stale, or inconsistent | refuse; do not bootstrap/bind |
| plain `install` | any current supervisor record exists | refuse; use `--replace` only under D1.2 |
| `install --replace` | service loaded/running, predecessor unproved, or foreign/tampered plist collision | refuse |
| any resident start | exclusive same-node ledger writer cannot be acquired | refuse before endpoint or UDS bind |
| any second managed process | first managed process owns the ledger | refuse before endpoint or UDS bind |

Detection uses, in order, the fixed launchd label/plist, validated supervisor record and ledger reference, launchctl inspection, owner-authenticated UDS status when reachable, and the exclusive ledger writer. PID/socket/plist presence may be discovered state but never overrides record validation or writer ownership. A stale socket is removed only by its admitted owner after an observation; it is not proof that another Mother exists.

When another resident owns the writer and its owner-authenticated UDS is reachable, the lifecycle-attempt ingress asks that writer to append the refusal/intent fact. It is a lifecycle control message, not an `MctCall`, and cannot execute launchctl from the resident. When concurrent bootstrap owns the writer but has no UDS yet, the losing attempt follows D1.3's observation-only post-contention acquisition.

If no canonical writer can append the refusal, the command performs no protected effect and reports only that the attempt could not become durable. It does not falsely acknowledge an observed lifecycle outcome. Writer-unavailability law dominates impossible observation ordering.

**Rationale:** File/process probes improve diagnosis, but the current ledger-backed record defines managed policy and exclusive writer ownership prevents a second same-node resident.

### D1.6 — Existing observation kinds encode lifecycle roles and outcomes

No kernel kind is added. Every direct lifecycle attempt shares one generated lifecycle trace and composes these existing facts:

1. `OperatorActionRecorded` (`SourcePlane::Operator`) records authenticated direct initiation as allowed or denied;
2. `LifecycleTransitionRecorded` records attempted/started/completed/failed lifecycle state; a successful no-op uses `ObservationOutcome::Completed` with an explicit no-op summary rather than inventing an outcome;
3. `AdapterEffectStarted`, `AdapterEffectCompleted`, or `AdapterEffectFailed` (`SourcePlane::Adapter`) records plist/filesystem/launchctl execution; and
4. existing storage kinds remain reserved for actual ledger/storage outcomes rather than being relabeled as launchd events.

The fixed role projection is:

- **subject_id:** pending local installation scope before identity, otherwise the current mctMother node/installation whose lifecycle changes;
- **initiator:** direct commands reference the authenticated `os-uid:<uid>` `OperatorActionRecorded`; automatic/supervised boot references `supervisor-record:<record-id>@<revision>` and its creation/revision observation, with no boot-time operator fact;
- **executor:** `mct-daemon-installer`, `mct-daemon-resident`, or `launchd-adapter:io.patina.mct.mother`, represented by source plane/resource and the shared lifecycle trace.

The record id/revision, governing observation id, action, and a bounded canonical discovered-state/result summary are present in the lifecycle facts. Payload bytes, executable bytes, plist bytes, environment, credentials, and secrets are not.

The per-action mapping is:

| Action | Required facts |
|---|---|
| install/create or revise | operator allowed/denied; lifecycle started/completed/failed; record/plist adapter started/completed/failed |
| supervised/automatic start | lifecycle started from exact record; prior-instance reconciliation when required; launchd/process adapter started/completed/failed; readiness only after completion prerequisites |
| manual serve | operator allowed/denied; lifecycle started; endpoint/control adapter facts; clean lifecycle completion or later reconciliation |
| stop | operator allowed/denied when direct; lifecycle shutdown started/completed/failed while writable; launchd bootout adapter fact |
| restart | explicit restart lifecycle started/completed/failed plus child stop/start traces linked to the restart trace |
| uninstall | operator allowed/denied; lifecycle started/completed/failed/no-op; stop and record/plist removal adapter facts |
| reconciliation | lifecycle transition with the reconciliation outcome and exact unmatched instance/start reference; no fabricated crash-time fact |

An attempt is not acknowledged as successful or denied until its required durable fact exists. Status/readiness reads add none.

**Rationale:** The ratified law explicitly required composition from existing kinds. Shared traces and exact governing references make the three roles reconstructable without expanding kernel ontology.

### D1.7 — Startup continuity distinguishes clean and unclean termination

Every admitted resident start generates an immutable instance id and appends a `LifecycleTransitionRecorded` start fact before Iroh endpoint bind, UDS bind, child loading, call admission, or readiness. A supervised fact binds the exact current record id, revision, digest, and provenance observation. Manual `serve` binds its authenticated direct operator action instead.

A direct `start` uses an explicit single-writer handoff rather than racing launchd for the ledger:

1. the CLI acquires the offline writer and appends the operator request, lifecycle-start attempt, and launchd-adapter-started facts;
2. the CLI releases that writer before invoking `launchctl bootstrap`;
3. the supervised process acquires the writer, validates the governing facts, reconciles continuity, and appends its record-derived instance-start fact;
4. after real UDS readiness, the CLI asks the resident writer to append launchd-adapter-completed and direct-start-completed facts before returning success; and
5. if launchctl fails before a resident owns the writer, the CLI reacquires it and appends adapter/lifecycle failure before returning the failure.

If either completion path cannot regain canonical append capability, the command emits no normal success/failure `LifecycleReport`; later reconciliation sees the durable started attempt. Automatic login boot has no direct operator request and begins at the supervised process's record-derived step.

Before appending the new start fact, startup reads the verified ledger tail:

- no prior resident-start fact means no discontinuity;
- a prior instance with a matching durable clean-shutdown completion is continuous;
- a prior instance without a matching completion is unclean, regardless of stale PID/socket state.

For an unmatched prior instance, the new writer appends a reconciliation lifecycle fact naming that prior instance/start observation before the new start, endpoint bind, or readiness. It does not fabricate a shutdown observation at the old process's death time.

Clean shutdown while writable follows:

1. append shutdown-started under the current instance/lifecycle trace;
2. stop accepting new work and make readiness false;
3. drain/close control and endpoint effects within existing bounded shutdown behavior;
4. append adapter completion and lifecycle shutdown-completed; and
5. exit and release the writer.

`stop` inspects state and asks the reachable resident writer to append the direct operator request, stop attempt, and launchd-adapter-started fact, make readiness false, and arm the clean shutdown path. The CLI then performs `launchctl bootout` and waits for unload. The resident records its clean shutdown before exit; after writer release, the CLI reacquires the writer and records launchd-adapter-completed plus direct-stop-completed before returning success. Failure to complete that handoff is not reported as an observed successful stop.

`restart` uses the same stop handoff, waits for writer/UDS release and unload, and then performs the normal start handoff under one parent restart trace. It never uses `launchctl kickstart -k` because force-kill would turn the normal restart path into an unclean termination.

If a process is killed or disappears before shutdown completion, no crash-time observation is invented. The next admitted start performs the reconciliation above. The start command reports ready only after reconciliation, startup observations, record validation, and real UDS readiness.

**Rationale:** Matching immutable instance start/shutdown facts is stronger than PID files and directly implements `UncleanTerminationIsReconciledAfterward`.

### D1.8 — Uninstall removes policy, never evidence

Uninstall means “stop managing this resident with launchd.” It does not mean “erase MCT.”

When installed and writable, uninstall:

1. durably records the authenticated uninstall attempt;
2. performs the D1.7 clean stop if the service is loaded;
3. reacquires the exclusive writer after resident exit;
4. appends the record/plist removal decision before removal;
5. removes the managed plist first;
6. removes the current supervisor record second; and
7. appends lifecycle completion before success acknowledgement.

Removing the plist first ensures a crash cannot leave launchd-visible policy without a governing record. A failed partial uninstall is explicit discovered state for the next attempt.

Uninstall preserves, without exception in this slice:

- the complete observation ledger and writer metadata;
- config and local node identity/key;
- SQLite state, idempotency, runs, and projections;
- installed children, blobs/artifacts, and Toy/peer authority state;
- logs; and
- directories needed to retain those objects.

It removes only the launchd-loaded state, managed plist, and current supervisor record. Historical record facts remain in the ledger. There is no `--purge` or force-delete option. Evidence deletion would require the separately reserved ledger-retention law.

If no record/plist/service is present but the ledger is writable, uninstall records a successful no-op reconciliation and preserves everything. If a foreign or digest-mismatched plist occupies the managed path, uninstall observes and refuses rather than deleting it.

**Rationale:** Supervisor policy is replaceable operational configuration. Identity, state, and the observation chain are durable product evidence and survive de-supervision.

### D1.9 — Known writer loss fences lifecycle and readiness

`ResidentLedgerWriter` exposes one monotonic process-lifetime state: `writable` may become `fenced`; it never becomes writable again in the same resident process. Any open, queue, append, sync, acknowledgement, or backing-writer failure transitions it to fenced exactly once.

The fence is shared by status, control ingress, call ingress, routing/effect boundaries, and lifecycle orchestration. Once known:

- readiness becomes `not_ready` immediately;
- no new call, authority mutation, lifecycle action, launchd action, config/state mutation, endpoint publication, or unrelated cleanup may begin;
- queued work that has not crossed its next protected effect boundary is refused without that effect;
- an already-running effect may finish naturally, but its outcome is neither acknowledged nor inserted into an idempotency/result cache; and
- status remains available as a projection when the process can still serve it.

The only lifecycle effect allowed while fenced is termination for safety. `stop` may issue launchd bootout even when no append is possible, but it must not claim an observed clean shutdown or successful ordinary stop; the next start reconciles the unmatched instance. Install, start, restart, record revision, and uninstall remain forbidden while fenced.

Explicit observer restoration is the other ratified carve-out but is not implemented in this slice. The operator must use a future gated recovery command; lifecycle commands cannot silently repair, replace, truncate, or side-journal the ledger.

A supervised process that cannot open/verify the writer at startup performs no endpoint/control/readiness effect and exits failed. launchd may apply its declared throttled restart policy, but each failed process cannot bypass the ledger gate.

**Rationale:** Keeping a fenced resident alive long enough to project status or receive safety termination is safer than continuing effects or pretending a restart repaired evidence.

### D1.10 — Backend-neutral orchestration owns ordering and testability

Lifecycle record parsing, record/ledger correlation, bootstrap planning, conflict decisions, observation construction, clean/unclean continuity, uninstall preservation, and writer fencing live in shared daemon product logic. They do not live inside launchd command wrappers and are not duplicated between CLI and resident.

Platform code is limited to:

- authenticated UID/home/process-context inspection;
- launchd plist rendering;
- launchctl invocation and result classification; and
- owner/mode checks available on macOS/Unix.

The fake adapter implements effects as deterministic in-memory state plus isolated files. It can spawn the real resident task or a process harness, report ready/stopped state, and simulate an unclean kill without invoking machine launchd. The primary integration proof therefore runs in CI without installing a real user service.

A later systemd adapter must implement the same `SupervisorAdapter` and record semantics. It may add backend-specific record fields through a new schema version, but cannot bypass bootstrap, provenance, observations, continuity, preservation, or fencing.

**Rationale:** The reusable unit is lifecycle law and ordering, not a generic subprocess helper or copied launchctl/systemctl branches.

### D1.11 — Quarry disposition is selective and explicit

From `patinaMother`, this slice translates and rebuilds:

- a macOS user LaunchAgent as the daily-driver supervisor;
- XML-safe plist rendering;
- loaded/running inspection and bounded waits for launchd state changes;
- manual-versus-managed conflict detection; and
- fake supervisor-command testing.

It rejects:

- environment markers as proof of supervised provenance;
- plist presence, PID files, sockets, or launchctl success as authority/evidence by themselves;
- ignored bootout/disable/enable failures and try-another-domain fallback;
- Homebrew service coexistence/warnings as a supported backend;
- manual detached restart as a fallback for missing supervision;
- systemd in this macOS slice; and
- inherited labels, storage models, APIs, and implementation structure.

No legacy code is ported. Any similar low-level plist escaping or command invocation is independently implemented behind the new adapter and justified by this SPEC.

### D1.12 — Executable-digest strictness is knowingly accepted

The operator knowingly accepts exact executable-digest binding as operational policy. Replacing bytes at `executable_path` does not update, bless, or supersede the current supervisor record. The next supervised boot validates the observed BLAKE3 digest against `executable_digest` and fails closed before identity, state, endpoint, UDS, or readiness effects.

The only ordinary remediation is:

```text
mct-daemon stop
mct-daemon install --replace [--executable <absolute-path>]
mct-daemon start
```

When the resident is already stopped, the first step is an observed no-op. The digest-mismatch refusal uses a caller-safe mismatch class such as `supervisor executable digest mismatch` and explicitly directs the operator to run `mct-daemon install --replace`; it does not disclose bytes or silently rewrite the record.

Because the plist uses `KeepAlive`, launchd may repeatedly attempt and throttle a swapped, unblessed executable. This throttle-loop symptom is accepted for this slice. The Task 3 runbook edit must document the symptom, diagnosis, and `install --replace` remediation. Binary upgrade/rollback automation remains a later acquisition/release concern.

### D1.13 — `gui/<uid>` is a knowingly accepted limitation

The operator knowingly accepts that this slice supports only a logged-in macOS GUI user domain, exactly `gui/<uid>`. Install/start fails safely when that domain is unavailable. There is no fallback to `user/<uid>`, `system/<uid>`, a root daemon, detached manual process, Homebrew, or another launchd domain.

Headless and SSH-only supervision are therefore unsupported in this slice. A future server/LaunchDaemon or non-GUI user-domain story requires its own operator-gated record, provenance, path, ownership, and process-context decisions; it cannot enter as adapter retry behavior.

## Plist Contract

The v1 plist is deterministic for a canonical record. It contains:

```text
Label                 io.patina.mct.mother
ProgramArguments      <absolute executable> serve --supervisor-record <absolute record>
RunAtLoad              true
KeepAlive              true
ThrottleInterval       bounded non-zero value
ProcessType            Background
StandardOutPath        <absolute owner-private log path>
StandardErrorPath      <absolute owner-private log path>
```

It does not contain shell commands, inherited working-directory assumptions, raw environment, secrets, client identity, node authority, or duplicated runtime paths. The executable and full plist are digest-bound by the record.

## Lifecycle Response Contract

Human output and `--json` are projections of the durable attempt and adapter result. JSON includes only safe fields:

```text
LifecycleReport {
  action
  outcome                 # completed | denied | failed | no_op
  attempt_id
  subject_id
  supervisor_record_id?
  supervisor_revision?
  observation_id
  running?
  ready?
  safe_message
}
```

No response exposes executable/plist bytes, identity key material, policy internals, environment, or ledger internals. If the required observation cannot become durable, no normal `LifecycleReport` is emitted; the CLI returns a durability failure and performs no ordinary protected effect. Safety termination under D1.9 returns an explicit unobserved-safety-termination error/status, never `completed`.

## Failing-Test-First Implementation Order

1. Add the red primary disk-backed lifecycle integration test with a fake adapter and no implementation.
2. Add supervisor record/path/canonical-digest types and record-ledger validation tests.
3. Add shared lifecycle observation/role constructors using only existing kinds.
4. Add bootstrap install planning and exclusive-writer contention behavior.
5. Add the backend-neutral adapter contract and fake adapter.
6. Add launchd plist rendering, exact domain command mapping, and typed failures.
7. Add CLI install/start/stop/restart/uninstall dispatch.
8. Add supervised process-context/record validation and refactor startup so supervised boot never creates a fake operator authentication.
9. Add immutable resident instance continuity, clean shutdown facts, and next-start unclean reconciliation.
10. Add the monotonic resident writer fence and effect-boundary/status integration.
11. Add conflict, append-failure, partial-effect, no-op, and preservation tests.
12. Add Track 3 attribution rows before declaring implementation complete.
13. Update the runbook and TODO only after the integration proof and attribution diff pass.

## Required Integration Proof

The primary landed test must reconstruct the complete behavior from reopened disk state:

1. create isolated absolute service-root, plist, logs, config, identity, children, SQLite, ledger, UDS, and executable-fixture paths owned by the current UID;
2. configure the fake supervisor adapter and typed simulated launch context without a production CLI escape hatch;
3. run install and assert no identity, record, plist, state, or launchd effect precedes the durable bootstrap/install-attempt batch;
4. close and reopen the ledger, record, config, identity, and state; verify owner modes, exact record revision/digests, authenticated UID provenance, creation observation, discovered-state summary, and install completion chain;
5. start through the fake adapter, wait for real resident UDS readiness, and assert the boot lifecycle fact names the exact supervisor record/revision and install-time operator provenance without a boot-time `OperatorActionRecorded` claim;
6. stop through the clean path, await resident exit/writer release, reopen the ledger, and verify shutdown-started, adapter completion, and matching clean-shutdown completion for that instance;
7. start a second supervised instance and verify no discontinuity is recorded after the matched clean shutdown;
8. terminate that second instance uncleanly without writing a shutdown completion;
9. start a third supervised instance and verify one reconciliation fact names the unmatched second instance/start before the third start and before UDS readiness;
10. stop the third instance cleanly, then uninstall;
11. verify fake launchd state, managed plist, and current supervisor record are absent;
12. verify ledger bytes/entries, config, identity/key, SQLite state, children/artifacts, and logs are preserved; and
13. reopen the preserved ledger and verify the uninstall attempt, adapter removals, lifecycle completion, and the complete prior bootstrap/start/stop/reconciliation chain.

The test uses only isolated temporary paths and never reads, installs, stops, or mutates a running `patinaMother` or the machine's real launchd service.

## Additional Required Failure Proofs

Named tests must also prove:

- first bootstrap append failure leaves only permitted minimal observer residue;
- concurrent install loser never performs effects and later appends a contention refusal;
- plain second install and running-service replacement refuse durably;
- manual serve under active policy and managed start against a manual writer refuse before endpoint/launchctl effects;
- record/plist/executable digest mismatch and missing governing ledger entry fail closed;
- launchctl non-zero outcomes are observed failures with no backend/domain fallback;
- uninstall never removes ledger/state/identity and never deletes a foreign plist;
- start/stop no-op reconciliations are observed;
- post-start writer loss flips readiness, blocks new calls/lifecycle/effects, suppresses in-progress acknowledgement/cache, and permits only safety termination; and
- append failure during shutdown does not claim clean completion and is reconciled by the next start.

## Track 3 Attribution Gate

Before close-out, `layer/surface/build/spec-drift-audit/track3/LEDGER.md` receives one row per implemented `MctOperationalSelfObservation` invariant, naming the exact landed tests. At minimum this slice must disposition:

- `LifecycleActionAttemptsAreObserved`;
- `StatusAndReadinessAreProjections`;
- `UncleanTerminationIsReconciledAfterward`;
- `MinimalObserverSubstrateMayPrecedeFirstAppend`;
- `FirstAppendPrecedesRemainingBootstrapEffects`;
- `BootstrapInitiatorIsAuthenticatedLocalPrincipal`;
- `ExclusiveWriterAdmitsOneBootstrap`;
- `ExternalReceiptIsInputNotEvidence`;
- `BootstrapAppendFailureSuppressesSuccess`;
- `WriterLossFencesMother`;
- `InProgressEffectsAreNotAcknowledgedOrCached`;
- `TerminationForSafetyMayProceed`;
- `OperationalRolesAreDistinctProjections`;
- `SubjectNamesLifecycleTarget`;
- `InitiatorNamesCurrentCausalAuthority`;
- `SupervisorInitiationUsesInstalledRecordProvenance`;
- `ExecutorNamesEffectPerformer`;
- `LifecycleActionsAreNotCalls`; and
- `ExistingObservationKindsComposeOperationalFacts`.

Observer-restoration and recovery-digest invariants remain `LAW-LEADS-CODE` or `DEFERRED` with the named future recovery slice unless this operator gate explicitly expands scope. There is no implicit waiver.

## Verification

Every implementation commit must pass:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

A commit changing Allium additionally runs:

```bash
allium check layer/allium
allium analyse layer/allium
```

This SPEC does not authorize edits to ratified Allium law. A discovered conflict stops for the operator.

Close-out additionally captures the new integration and failure tests with `--nocapture`, reconstructs the full commit range from disk, records the flake log or `none`, diffs every landed primary-test step against Required Integration Proof steps 1–13, and reports every Track 3 row or named waiver plus TODO item 5's state.

## Build Readiness

**APPROVED — implementation may proceed failing-test-first.**

The operator ratified D1.1–D1.11 on 2026-07-15, explicitly including the owner-authenticated UDS lifecycle-control message as lifecycle-fact ingress rather than an `MctCall`, and the Track 3 `LAW-LEADS-CODE`/`DEFERRED` handling of recovery invariants. D1.12 and D1.13 record the operator's knowing acceptance of executable-digest strictness and the `gui/<uid>` limitation before the first failing test.

---
type: feat
id: artifact-acquisition
status: active
created: 2026-07-16
target: daily-driver-slice-3
operator_gate: D1-ratified
sessions:
  origin: 20260714-160744-657025000
  work: []
related:
  - layer/allium/mct-product-map.allium
  - layer/allium/mct-patina-migration.allium
  - layer/sessions/20260714-160744-657025000.md
  - layer/surface/build/feat/supervisor-lifecycle/SPEC.md
  - layer/surface/build/feat/resident-call-ingress/SPEC.md
  - layer/surface/build/product/MOTHER-REPLACEMENT-RUNBOOK.md
  - layer/surface/build/product/MCT-NEXT-BUILD-TODO.md
  - layer/surface/build/spec-drift-audit/track3/LEDGER.md
  - layer/core/migration-vocabulary.md
  - crates/mct-kernel/src/child.rs
  - crates/mct-daemon/src/children.rs
  - crates/mct-daemon/src/registry.rs
  - crates/mct-daemon/src/state.rs
  - crates/mct-daemon/src/blob_store.rs
  - crates/mct-daemon/src/daemon/control.rs
  - crates/mct-daemon/src/daemon/cli_runtime.rs
  - crates/mct-daemon/src/daemon/supervisor_lifecycle.rs
beliefs:
  - mother-kernel-decides-adapters-perform
  - mother-is-the-daemon
exit_criteria:
  - id: explicit-artifact-command-surface
    text: Artifact staging, filesystem acquisition, acquisition/source inspection, and standing-source create/revoke/list are explicit artifact commands distinct from top-level supervisor install/start/stop/restart/uninstall; legacy registry mutations cannot bypass acquisition authority or provenance.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon artifact_command_surface_is_explicit_and_supervisor_distinct -- --nocapture
  - id: standing-source-authority
    text: Standing filesystem source authority is an owner-authenticated, ledger-backed SQLite projection with explicit non-empty scope, additive policy, mandatory expiry, revision, issuer, and current state; absent, expired, revoked, stale, unobserved, or digest-inconsistent records grant nothing.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon standing_artifact_source_authority_is_scoped_observed_and_revocable -- --nocapture
  - id: independent-effect-authority
    text: Both standing-source and operator-pointed paths require a fresh one-attempt filesystem-adapter effect capability derived from the authenticated local operator action; neither source trust nor adapter possession can mint or replace the other gate.
    checked: false
    verify: cargo test -p mct-kernel artifact_acquisition_requires_source_path_and_current_adapter_authority -- --nocapture
  - id: attempt-bearing-acquisition
    text: Every admitted filesystem attempt, including source-read, copy, expected-digest, sidecar, manifest, scope, and publication failures, leaves immutable acquisition/adapter/verification evidence; no failed or rejected attempt creates or references a successful ComponentArtifact.
    checked: false
    verify: cargo test -p mct-daemon artifact_acquisition_failures_are_observed_without_artifact_publication -- --nocapture
  - id: digest-floor
    text: SHA-256 sidecars remain mandatory for the manifest and component and continue to define ComponentArtifact content/manifest hashes, while BLAKE3 independently records acquired component bytes; algorithm-tagged values, exact byte domains, and mismatch behavior are unambiguous and fail closed.
    checked: false
    verify: cargo test -p mct-daemon staged_package_reconciles_sha256_floor_with_blake3_acquisition_evidence -- --nocapture
  - id: uniform-provenance
    text: The only post-law ComponentArtifact creation transaction requires at least one successful verified ArtifactAcquisition reference; migrated pre-law rows are explicitly historical_unknown with no fabricated acquisition, and ordinary loaders/registry commands cannot silently create either status.
    checked: false
    verify: cargo test -p mct-daemon state::tests::component_artifacts_require_real_acquisition_or_explicit_legacy_migration -- --nocapture
  - id: exact-approval-evidence
    text: Approval requires the operator to name the exact sha256 artifact id, validates it against the immutable catalog package and acquisition-backed state, and surfaces acquisition plus verification evidence before recording the existing separate approval-and-local-assignment facts.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon child_approval_names_exact_artifact_and_surfaces_acquisition_evidence -- --nocapture
  - id: supervised-slate-milestone
    text: A real slate-manager@0.2.0 package is staged and acquired into isolated paths, denied before approval, exactly approved and assigned, denied before scoped toy grants, executed through the supervised resident UDS, inspected, revoked, denied again, and reconstructed from reopened ledger/state.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon supervised_slate_artifact_acquisition_executes_and_revokes_end_to_end -- --nocapture
  - id: no-new-toy-or-network-adapter
    text: This slice adds no canonical Toy and no network adapter; resident Slate execution reuses current logging, measure, git, and project-filesystem ToyGrants, while all network source access remains the named future egress-toy slot.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon artifact_slice_exposes_only_filesystem_adapter_and_existing_toy_catalog -- --nocapture
  - id: writer-and-adapter-ordering
    text: Resident and offline artifact mutations share one implementation, authenticate the local principal, hold or use the canonical observation writer, durably record authority before filesystem effects, and fail without acknowledgement or catalog authority after append/writer loss.
    checked: false
    verify: cargo test -p mct-daemon --bin mct-daemon artifact_acquisition_append_failure_suppresses_filesystem_and_catalog_effects -- --nocapture
  - id: attribution-ledger
    text: Every acquisition invariant implemented by this slice has a COVERED Track 3 row naming landed tests, or an explicit LAW-LEADS-CODE/DEFERRED disposition justified by an out-of-scope path; there are no unnamed waivers.
    checked: false
    verify: bash -lc 'rg -n "MctArtifactAcquisitionAuthority|AcquisitionIsFifthIndependentFact|NewArtifactsRequireAcquisitionProvenance|PatinaRegistrySyncQuarryDisposition|artifact_acquisition" layer/surface/build/spec-drift-audit/track3/LEDGER.md'
  - id: workspace-validation
    text: The phase passes the required workspace validation suite.
    checked: false
    verify: cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
---

# feat: Artifact acquisition and package staging

> An authenticated local operator can turn real local build output into an acquisition-backed, strictly verified Child artifact without granting that artifact permission to run.

## Problem

Daily-Driver Slices 1 and 2 made the resident callable and supervised, but the supported package path still starts after trust has already been assumed. `children load` reads arbitrary package directories, `registry install` copies a package that merely has sidecars, `registry sync` treats a source id/path as sufficient configuration, and `component_artifact_from_loaded_child` can synthesize a `ComponentArtifact` from disk with no acquisition reference. Those paths predate the ratified Area 2 law.

The current integrity implementation also has two algorithm uses that are not yet joined by one contract. Child package sidecars and artifact identity use SHA-256; acquisition and other byte-observation paths use BLAKE3. Keeping both is valid only if their byte domains and authority roles are explicit. Neither may be treated as an optional substitute for the other.

Finally, `children approve` currently takes a child name and, in one operator mutation, persists both an exact approval and its local assignment. The underlying facts are separate and exact-digest today, but the command does not show the acquisition evidence that the operator is approving. There is no separate `children assign` command in the D1 baseline.

This slice transcribes `MctArtifactAcquisitionAuthority`, `ArtifactAcquisition`, `MctChildComponentLifecycle`, and `PatinaRegistrySyncQuarryDisposition` into the smallest local vertical path. It does not derive update-channel, network-source, scheduling, or new Toy law.

## Goals

1. Stage a proper SDK child package from local manifest/component build output without mutating the source.
2. Support one explicit filesystem acquisition through either an artifact-specific operator decision or a current standing source record.
3. Require fresh effect authority for the filesystem adapter independently of source trust.
4. Preserve every acquisition attempt, including failures, as ledger evidence and an inspectable SQLite projection when storage remains available.
5. Keep SHA-256 sidecars as the unwaivable package verification floor and add independent BLAKE3 acquisition observation.
6. Make every post-law `ComponentArtifact` acquisition-backed by construction.
7. Mark pre-law artifacts as historical provenance-unknown without inventing equivalent evidence.
8. Surface acquisition/verification evidence during exact-artifact approval.
9. Execute real `slate-manager@0.2.0` through the supervised resident only after exact approval/assignment and the existing scoped Slate ToyGrants.
10. Leave source credentials, network access, update authority, scheduling, and retention outside this slice.

## Non-Goals

- No HTTP, OCI, Git, Iroh, registry-service, or other network acquisition adapter.
- No egress Toy and no new canonical Toy contract.
- No update channel, auto-update, pre-approved publisher/key/source, rolling replacement, rollback, or `--replace` acquisition behavior.
- No recurring/event-triggered source sync and no `CallTriggerAuthority` implementation.
- No new child runtime or Toy host capability; Slate uses the already admitted logging, measure, git, and project-filesystem contracts.
- No JVM SDK.
- No change that lets approval transfer across child name, version, publisher, source, namespace, key, or channel.
- No source credential field, credential lookup, secret attachment, or connection authority.
- No historical-provenance reconciliation command; this slice only makes the unknown state explicit and non-forgeable.
- No ledger retention, archival, deletion, compaction, export, or repair action.
- No mutation of `~/.patina/plugins`, the sibling Slate repository, or the machine's real launchd service in tests.
- No edit to the ratified Allium law. A conflict discovered during implementation stops at the operator.

## D1 Decisions — ratified

The operator ratified D1.1–D1.14 and amendments D1.15–D1.17 before the first failing implementation test. Any further semantic amendment requires another explicit operator gate.

### D1.1 — One `artifacts` namespace owns acquisition; supervisor verbs remain untouched

The production surface becomes:

```text
mct-daemon artifacts stage <source-root>
  --manifest <root-relative-path>
  --component <root-relative-path>
  --child <claimed-name>
  --version <claimed-version>
  [--expected-digest blake3:<64-lower-hex>]
  [--children-dir path] [--state path] [--ledger path] [--uds socket-path] [--json]

mct-daemon artifacts acquire <package-dir>
  (--operator-pointed | --source-authority <source-authority-id>)
  --child <claimed-name>
  --version <claimed-version>
  [--publisher <claimed-publisher>]
  [--expected-digest blake3:<64-lower-hex>]
  [--children-dir path] [--state path] [--ledger path] [--uds socket-path] [--json]

mct-daemon artifacts show <sha256:<64-lower-hex>>
  [--state path] [--json]
mct-daemon artifacts acquisitions
  [--artifact <sha256:<64-lower-hex>>] [--state path] [--json]

mct-daemon artifacts sources create <source-authority-id>
  --filesystem-root <absolute-path>
  --scope-mode <constrained|explicit-broad>
  --artifact <name@version> ...
  --publisher <publisher-or-*> ...
  --namespace <wit-namespace-or-*> ...
  --action acquire
  --integrity-policy sha256-sidecars-v1
  [--provenance-policy <policy-ref>]
  --expires-at <RFC3339>
  [--state path] [--ledger path] [--uds socket-path] [--json]

mct-daemon artifacts sources revoke <source-authority-id>
  [--state path] [--ledger path] [--uds socket-path] [--json]
mct-daemon artifacts sources list [--state path] [--ledger path] [--json]
```

`stage` is the daily-driver convenience operation. It accepts a selected manifest and compiled component beneath one canonical local source root, creates a canonical package in an internal temporary directory, creates both SHA-256 sidecars, and then enters exactly the same operator-pointed acquisition/verification/publication pipeline as `artifacts acquire`. It emits one `OperatorPointedArtifactAcquisitionDecision` and one `ArtifactAcquisition`; package preparation is not an evidence-free pre-step.

`acquire` accepts an already package-shaped directory. Exactly one authority selector is mandatory. `--operator-pointed` creates no reusable trust. `--source-authority` evaluates the named current source record. Neither mode accepts URLs in this slice.

`show`, `acquisitions`, and `sources list` are read-only projections. They append no authority or acquisition observation.

Top-level `install`, `uninstall`, `start`, `stop`, and `restart` remain exclusively the D1.1 supervisor lifecycle surface. Artifact acquisition never aliases those verbs.

The pre-law mutation surfaces are closed rather than preserved as ambient bypasses:

- `registry install` returns explicit migration guidance to `artifacts acquire --operator-pointed`;
- `registry sync` returns explicit guidance to `artifacts acquire --source-authority`;
- `children load` remains a read-only filesystem audit and cannot write a catalog artifact; and
- no compatibility alias may synthesize historical provenance for new bytes.

**Rationale:** A single namespace makes the authority choice visible and prevents “install” from ambiguously meaning supervisor policy, byte acquisition, catalog publication, approval, or assignment. Hard refusal is simpler and safer than a compatibility alias whose old arguments cannot express the new authority facts.

### D1.2 — Staging creates one canonical immutable package shape

`stage` canonicalizes `source-root` and requires both selected paths to remain beneath it after symlink resolution. This slice introduces two new named constants: `MCT_CHILD_MANIFEST_MAX_BYTES` bounds the manifest and `MCT_COMPONENT_ARTIFACT_MAX_BYTES` bounds the component. The limits are checked while streaming, before catalog visibility. Neither path may be a directory, device, FIFO, socket, or escaping symlink.

The emitted package contains exactly the authority-relevant files:

```text
child.toml
child.toml.sha256
<manifest-declared component-relative path>
<manifest-declared component-relative path>.sha256
```

An optional generated `checksums.txt` is diagnostic only and never replaces either sidecar. Skills, WIT source, README files, build metadata, and arbitrary source-tree files are not copied by this slice.

For an SDK package manifest that already declares `[child.artifact].wasm`, the declaration must be relative, must stay inside the package, and must identify the selected component. For read-only legacy flat prior art without that declaration, `stage` renders a package manifest with one fixed package-relative component path while preserving the parsed child, ingress, contract, needs, relationship, and metric values. It does not edit the source manifest. Duplicate/conflicting artifact declarations are rejected.

The parsed manifest name/version must equal `--child` and `--version`. The WIT operation namespaces used for standing-source scope are derived from the declared allow-list; an empty or malformed operation identity fails verification rather than broadening scope.

Temporary packages live under `<children-dir>/.acquiring/<acquisition-id>`. Successful verified packages are atomically published to the immutable catalog path:

```text
<children-dir>/artifacts/sha256/<component-sha256>/
```

SQLite stores that exact package path as daemon adapter projection; it is not added to the Allium `ComponentArtifact` domain shape. A same-digest reacquisition re-verifies the existing immutable package and adds evidence. A path collision with different bytes fails closed. There is no replacement flag.

**Rationale:** Digest-addressed package storage lets a successor be acquired without rewriting the package selected by an existing assignment. Assignment remains the only placement selector, and acquisition never mutates a running generation by child-name path.

### D1.3 — Standing source authority is a ledger-backed SQLite projection

A source record uses the ratified fields without credentials:

```text
ArtifactSourceAuthority {
  source_authority_id
  source_ref                    # file://<canonical-root>, no query/userinfo
  scope {
    scope_mode                  # constrained | explicit_broad
    artifact_scope             # exact name@version or explicit "*"
    publisher_scope            # exact claim or explicit "*"
    namespace_scope            # exact WIT namespace or explicit "*"
    allowed_actions            # only "acquire" in this slice
  }
  integrity_policy_ref          # exactly sha256-sidecars-v1 in this slice
  provenance_policy_ref?
  issuer_principal_ref          # derived os-uid:<authenticated uid>
  policy_revision              # current local node policy revision
  authority_state              # active/revoked; expiry is projected by time
  issued_at
  expires_at                   # mandatory and later than issued_at
  authority_observation_id
}
```

All four scope lists are required and non-empty. `constrained` permits exact values only. `explicit_broad` permits the literal `*` only in dimensions the operator deliberately broadens; omitted dimensions still grant nothing. Globs, path-prefix strings masquerading as artifacts, and implicit defaults are rejected.

The acquisition request supplies the exact `name@version`, claimed publisher when standing publisher scope is evaluated, derived WIT namespaces, source path, and action. Every dimension must match. A package with no publisher claim cannot match an exact publisher scope; the operator must either provide the claim or explicitly issue broad publisher scope. Publisher scope is source trust, not signature verification and not artifact approval.

`create` authenticates the effective UID offline or Unix peer UID on resident UDS. The issuer is never accepted from CLI JSON. A source authority id is immutable: changing root, scope, policy, issuer, or expiry requires a new id. `revoke` appends a later revocation fact and updates only the current projection; it never edits the issuance observation. Repeated revocation is an observed no-op, not a silent success. `list` calculates effective expiry at read time.

The SQLite row is insufficient by itself. Current evaluation opens the validated ledger, locates `authority_observation_id`, and checks the source id, revision, issuer, state action, and BLAKE3 digest of canonical record fields carried by the observation detail. Missing, mismatched, stale-policy, expired, revoked, unobserved, or hash-chain-invalid records grant nothing. The current projection may be rebuilt from ledger facts; it is not a second evidence ledger.

Source references accept only canonical `file://` roots in this slice and reject user-info, query strings, fragments, environment interpolation, and non-file schemes. The schema has no credential, secret, header, token, key, cookie, or connection field.

**Rationale:** This follows the supervisor-record and ToyGrant pattern: immutable ledger authority plus a queryable current projection. Explicit source scope is useful without turning configuration or filesystem reachability into trust.

### D1.4 — Operator-pointed decisions are one-attempt records

`stage` and `acquire --operator-pointed` require `--child` and `--version` before any source file is opened. The command creates and durably observes:

```text
OperatorPointedArtifactAcquisitionDecision {
  decision_id
  source_ref                    # canonical file root/package URI
  claimed_child_name
  claimed_artifact_version
  expected_digest?              # only blake3:<hex> in this slice
  issuer_principal_ref          # derived os-uid:<authenticated uid>
  policy_revision
  decision_state               # active, then consumed for success or failure
  authority_observation_id
}
```

The decision is artifact-specific, source-specific, and consumed by exactly one adapter attempt. A failure consumes it. Retrying creates a new decision and acquisition id. It cannot be promoted into an `ArtifactSourceAuthority`, reused for another component, or interpreted as approval.

When an expected digest is known, the CLI requires it and the evaluator binds it before source access. When it is not known, the nullable ratified field remains null; the command does not fabricate an expected value from bytes it reads during the same attempt. Sidecar verification remains mandatory either way.

**Rationale:** Required identity arguments make the operator's claim observable before the adapter discovers source claims, and consumption makes one-pointed authority non-ambient.

### D1.5 — Filesystem effect authority is fresh, one-shot, and independent

This slice introduces no canonical Toy. Instead, the kernel evaluates a private, non-serializable `AuthorizedFilesystemArtifactAcquisition` capability for one direct operator attempt. Its evidence input is:

```text
FilesystemAcquisitionEffectAuthority {
  authority_ref                # the admitted OperatorActionRecorded observation id
  adapter_ref                  # mct:artifact-acquisition/filesystem@1
  authenticated_uid
  source_ref
  allowed_action               # read_and_stage
  policy_revision
  attempt_id
  expires_at                   # command deadline
}
```

The kernel mints the capability only when:

1. the local principal is authenticated and current;
2. exactly one valid source-trust path (standing or operator-pointed) admits the same source and claims;
3. the effect authority names the fixed filesystem adapter, source, action, attempt, and current policy revision; and
4. the command deadline has not elapsed.

The filesystem adapter accepts the private capability by value and consumes it. It cannot be reconstructed from SQLite, source configuration, manifest needs, source possession, path reachability, or an acquisition record. Both standing and operator-pointed paths require this evaluation. Standing authority therefore permits a source relationship but does not permit unattended background reads.

This one-shot direct-operator capability is the concrete effect-authority shape for the only adapter in this slice. It is not a Child ToyGrant and grants no Child filesystem access. A future network source adapter must enter through the separately designed deny-by-default egress Toy, plus independent connection/secret authority where needed. It may not widen this filesystem capability or infer credentials from source trust.

**Rationale:** The ratified law requires independent current effect authority, while this phase explicitly forbids inventing a new Toy or egress contract. The operator consciously accepts that the quarry obligation “source access becomes Toy adapter” is satisfied here by the direct-operator one-shot filesystem capability, not a Child Toy: ToyGrants govern Child capabilities, and no Child acts in operator-driven acquisition. Direct authenticated operation is sufficient for the daily-driver filesystem path and leaves the future egress Toy as the truthful network-source slot.

### D1.6 — Acquisition and verification remain separate facts with explicit outcomes

One attempted adapter read produces exactly one immutable `ArtifactAcquisition`:

| Ratified field | Concrete mapping in this slice |
|---|---|
| `acquisition_id` | Generated before authority observation; immutable SQLite primary key and trace resource. |
| `authority_path` | Exactly `standing_source` or `operator_pointed`; never inferred from source configuration. |
| `standing_source_authority_id` | Required only for standing path and ledger-correlated at evaluation. |
| `operator_pointed_decision_id` | Required only for operator path and consumed by this attempt. |
| `adapter_effect_authority_ref` | The admitted local `OperatorActionRecorded` observation that minted the one-shot capability. |
| `source_ref` | Credential-free canonical `file://` root/package reference. |
| `claimed_child_name` | Pre-effect CLI claim, checked against parsed manifest. |
| `claimed_artifact_version` | Pre-effect CLI claim, checked against parsed manifest. |
| `observed_size_bytes` | Exact primary component byte count after a complete bounded read; null if no complete component was read. |
| `observed_digest` | `blake3:<hex>` of exactly the primary component bytes; null if hashing did not complete. |
| `acquisition_outcome` | `acquired` after a complete component/package staging read, otherwise `failed`. |
| `verification_outcome` | `verified`, `rejected`, or `not_reached` under D1.7. |
| `verification_observation_id` | Present for `verified` or `rejected`; absent only for `not_reached`. |
| `acquisition_observation_id` | Terminal adapter acquisition observation for every attempt. |
| `component_artifact_id` | Present only when verification succeeded and the catalog transaction created or linked the immutable artifact. |

The database uses append-only insert for acquisitions, not upsert. Exactly one authority reference is non-null, enforced in Rust and SQLite. `component_artifact_id` is forbidden unless `acquisition_outcome=acquired` and `verification_outcome=verified`. A failed/rejected attempt remains queryable but cannot be joined as successful artifact provenance.

Terminal classification is:

- source open/read/copy/bound failure: `failed/not_reached`;
- complete bytes but expected BLAKE3 mismatch: `acquired/rejected`;
- complete bytes but missing/mismatched SHA sidecar: `acquired/rejected`;
- complete bytes but manifest/claim/export/scope verification failure: `acquired/rejected`;
- all checks pass: `acquired/verified`; and
- catalog publication/storage failure after verification: acquisition remains `acquired/verified` evidence but has no `component_artifact_id`; the storage failure is separately observed and success is not acknowledged.

Reacquiring identical bytes creates a new acquisition and may join the same immutable artifact. It does not create approval or assignment.

**Rationale:** “Acquired” answers whether bytes arrived; “verified” answers whether they passed policy; artifact publication answers whether verified evidence became a local catalog fact. Combining those outcomes would erase failed attempts or let source access imply trust.

### D1.7 — SHA-256 is the floor; BLAKE3 is acquisition observation

The algorithm contract is deliberately dual and additive:

| Value | Algorithm and exact byte domain | Authority role |
|---|---|---|
| `<component>.sha256` | SHA-256 of exact component file bytes | Mandatory independent package integrity floor. |
| `child.toml.sha256` | SHA-256 of exact emitted manifest bytes | Mandatory independent manifest integrity floor. |
| `ComponentArtifact.artifact_id` | `sha256:<component SHA-256>` | Immutable exact-artifact identity named by approval and assignment. |
| `ComponentArtifact.content_hash` | Same tagged component SHA-256 | Catalog component verification fact. |
| `ComponentArtifact.manifest_hash` | `sha256:<manifest SHA-256>` | Catalog manifest verification fact. |
| `ArtifactAcquisition.observed_digest` | BLAKE3 of exact component file bytes | Attempt observation and optional operator expected-digest comparison. |

All persisted and CLI digest values are lowercase, algorithm-tagged, fixed-length hex. Bare hashes are rejected at acquisition/approval boundaries. Sidecar file contents remain lowercase bare SHA-256 only because the filename supplies the algorithm; whitespace is trimmed and no filename or `sha256sum` decoration is accepted.

A verified acquisition requires both SHA-256 sidecars and, when present, exact expected BLAKE3 equality. BLAKE3 success cannot waive a sidecar. A matching sidecar cannot waive an expected BLAKE3 mismatch. For staged packages, the SHA-256 sidecars are generated during staging and therefore attest copy/publication integrity, not source authenticity; source authenticity for that path rests on operator authority plus the optional expected BLAKE3 digest. Source integrity policy may add checks in a future gated slice, but `sha256-sidecars-v1` remains mandatory and cannot be replaced by an opaque policy reference.

**Rationale:** The values answer different questions while binding the same primary bytes. Naming both algorithms and domains avoids “two truths” and preserves compatibility with existing package identity.

### D1.8 — Observation ordering is authority before adapter before publication

No new `ObservationKind` is required. Existing kinds compose the fact chain under one acquisition trace:

1. `OperatorActionRecorded` records the authenticated direct action and exact authority-path decision before source access;
2. `AdapterEffectStarted` records `mct:artifact-acquisition/filesystem@1` before open/read/copy;
3. `AdapterEffectCompleted` or `AdapterEffectFailed` records the terminal acquisition attempt with acquisition id, safe source reference, observed size/digest when known, and authority references;
4. `ArtifactVerified` or `ArtifactRejected` records the independent verification decision and both algorithm results when verification was reached;
5. a storage adapter started/completed/failed fact brackets immutable package publication and the SQLite projection transaction; and
6. existing `ChildApproved`, `ChildAssigned`, ToyGrant, runtime, and revocation observations remain separate later facts.

Canonical bounded JSON in `detail_ref` carries the complete credential-free source-authority/decision/acquisition record digest and references needed to reconstruct SQLite. It carries no component/manifest bytes, source credentials, secret values, environment, or arbitrary source-tree contents.

The operator/authority batch must be durable before a source file opens. Terminal acquisition/verification evidence must be durable before catalog publication. The package is first written beneath the hidden acquisition directory, verified there, and atomically renamed to its immutable digest path. The SQLite transaction then inserts the terminal acquisition, artifact provenance association, package-path projection, and `ComponentArtifact` as one unit. A completed storage observation is required before normal success output.

If append fails before source access, no source or package effect occurs. If the resident writer is fenced, no new acquisition/source mutation begins. An adapter effect already in progress may finish naturally under the existing writer-loss law, but its outcome is not acknowledged and no artifact/approval/result cache is created. Offline fallback must acquire the same exclusive ledger writer before preparing the action; it is not a second implementation.

**Rationale:** The ledger remains canonical evidence while SQLite and package files remain rebuildable/queryable projections. Existing observation kinds already distinguish operator authority, adapter effects, verification, storage, and later lifecycle authority.

### D1.9 — Post-law artifact provenance is enforced by construction

`ComponentArtifact` gains the ratified fields:

```text
provenance_status: acquisition_backed | historical_unknown
acquisition_ids: List<ArtifactAcquisitionId>
```

The daemon additionally stores the immutable package path as adapter projection. Kernel artifact equality and child-call authority carry provenance status and ids as data, but acquisition evidence does not grant execution.

The ordinary post-law creation API is one transaction such as `record_verified_acquisition_and_artifact`; direct public `upsert_artifact` creation is removed or made migration/test-private. It enforces:

- `provenance_status=acquisition_backed`;
- at least one referenced acquisition;
- every referenced acquisition is `acquired/verified` and names the same child/version/component digest;
- the primary acquisition observation and verification observation exist;
- the package at the immutable path still satisfies both SHA sidecars; and
- the artifact id/content hash is the exact SHA-256 selected by approval.

SQL constraints require an acquisition-backed artifact to have a primary acquisition reference. A join table preserves all reacquisition ids. Failed acquisitions cannot join. Artifact rows are immutable: a conflict with different child, version, hashes, exports, runtime shape, ingress, lifecycle, or provenance data is an error, not an update.

Schema migration marks every pre-existing `component_artifacts` row:

```text
provenance_status = historical_unknown
acquisition_ids = []
```

It does not scan files and invent acquisitions. Existing exact approvals/assignments remain visible as pre-law authority, and approval/evidence output labels them historical unknown. This slice refuses a new approval for `historical_unknown`; there is no override flag. A future reconciliation command must prove equivalent evidence before changing status.

All code paths that currently call `component_artifact_from_loaded_child`, `record_loaded_child_candidate`, or registry sync are changed to read the persisted catalog artifact or remain read-only diagnostics. Resident routing resolves the exact assigned artifact's immutable package path from state and re-verifies package bytes; it does not synthesize a fresh artifact from whichever same-named directory appears first.

**Rationale:** An enum plus a non-empty reference is stronger than a nullable “maybe provenance” convention. Historical compatibility remains honest without becoming a loophole for new bytes.

### D1.10 — Verification publishes a catalog artifact, not use authority

A successful `stage` or `acquire` may create the immutable package path and `ComponentArtifact`. It creates no `ChildApproval`, `ChildAssignment`, `ChildInstance`, ToyGrant, readiness, or update authority. The artifact appears as a candidate only.

Before approval, a resident call targeting its declared operation must end in a typed no-route/child-not-approved denial without component instantiation. After exact approval/local assignment but before required ToyGrants, a real Slate call must be denied before a host effect or component business operation. Only after current grants pass may runtime execution begin.

The D1 baseline has no separate `children assign` command: `children approve` currently executes one authenticated operator request that records `ChildApproved` and `ChildAssigned` as distinct facts and persists both exact records. This slice preserves that composed surface rather than inventing assignment policy. The command's new exact form is:

```text
mct-daemon children approve <child-name>
  --artifact <sha256:<64-lower-hex>>
  [--config path] [--children-dir path] [--state path]
  [--ledger path] [--uds socket-path] [--json]
```

The old name-only form fails with exact-artifact guidance. It never selects “latest,” a same-name package, or a source/channel. Approval and assignment remain independently revocable facts despite sharing one explicit command.

**Rationale:** This keeps the already implemented operator workflow while making the exact digest visible and preserving the law's independent fact model.

### D1.11 — Approval tooling uses one shared evidence projection

Before constructing approval observations, `children approve` loads the exact catalog artifact and shared `ArtifactEvidenceView` used by `artifacts show`. Human output presents, in this order:

```text
artifact id, child, artifact version
component SHA-256 and manifest SHA-256
verification status and verification observation
provenance status
for each acquisition: acquisition id, authority path/id, credential-free source ref,
  filesystem adapter ref, observed size, BLAKE3 digest, expected digest when present,
  acquisition/verification outcomes, and observation ids
requested approval Vision/Node/project scope and current policy revision
```

Only after that projection succeeds does the command submit/append the approval-and-assignment mutation. Tests use an injected output sink to prove evidence projection precedes the authority append. JSON returns the same structured evidence in the final report; it does not omit provenance for automation. There is no interactive prompt and no “approve by source/publisher” option.

If evidence is missing, historical unknown, internally inconsistent, no longer matches immutable package bytes, or references a failed acquisition, approval fails before authority mutation. Safe output contains no source credentials because source records cannot contain any.

**Rationale:** The Allium note is non-normative, so this is CLI behavior rather than a new approval prerequisite based on source reputation. Exact artifact identity remains the normative decision.

### D1.12 — Resident Slate execution reuses current Toy law

The milestone requires a real Slate operation, not merely loader success. Resident execution therefore resolves the active assignment's exact package and evaluates the manifest's requested host imports against current `ToyGrant` snapshots before instantiation. Manifest needs remain requests only.

For `slate-manager@0.2.0`, the existing `toys authorize-slate` command issues the already admitted contracts for:

- logging;
- measure;
- git; and
- project filesystem preopen.

The resident derives `/project` preopen and Git repository scope from those current grants, not from client JSON or the package source path. Missing, stale, wrong-artifact, wrong-assignment, wrong-project, revoked, or expired grants deny before host adapter effects. The adapter consumes existing kernel-minted `AuthorizedToyCall` capabilities. This slice does not add an acquisition Toy, network Toy, or broad compatibility mapping for manifest names.

Revoking the child revokes its exact approval and assignment. Active ToyGrants cannot compensate for that revocation; the next call denies before runtime. The test uses a fresh idempotency key so denial is current authority evaluation rather than replay behavior.

**Rationale:** “Operates Slate end to end” includes its real host imports. Reusing current Toy law proves acquisition did not smuggle filesystem or Git power into the Child.

### D1.13 — The fixture is real, immutable, and CI-local

The primary test uses a real `slate-manager@0.2.0` component built from the Slate repository's `v0.2.0` tag (`fb85706aad55fdfbf091e28ac8f4c09864996b0c`, the manifest-version commit verified in the local read-only prior-art repository at D1 drafting). Before implementation lands, that exact upstream commit/tag and the release build command must be captured beside the fixture.

The CI fixture is committed as read-only raw build output under a test fixture directory with:

- the real 0.2.0 manifest;
- the real compiled component;
- a provenance metadata file naming upstream repository, commit, tag, toolchain/build command, byte sizes, SHA-256, and BLAKE3; and
- no MCT `.sha256` sidecars in the raw source fixture.

The test copies those two source files into a temporary read-only source root. `artifacts stage` must create the package layout and sidecars in isolated MCT paths. Generated WAT lookalikes, a manifest with the right name over another component, or the mutable current `~/.patina/plugins/slate-manager` installation do not satisfy this proof.

The installed `~/.patina/plugins` files and sibling Slate checkout remain read-only prior art and are never test dependencies. The fixture metadata is build provenance, not runtime source authority; the runtime still creates the operator-pointed decision and acquisition evidence.

**Rationale:** A committed immutable release fixture makes CI deterministic and closes fixture one honestly. It also proves staging can normalize the sidecar-less local prior-art shape without treating it as pre-trusted.

### D1.14 — Source credentials and future adapters have an explicit empty slot

No source authority, operator decision, acquisition, observation, CLI request, JSON response, or SQLite table in this slice has a credential-bearing field. Filesystem source references reject URI user-info and non-file schemes. Environment variables do not inject source credentials.

The named future network slot is:

```text
NetworkArtifactAcquisitionAdapter
  requires current deny-by-default egress Toy authority
  + current source authority/operator-pointed decision
  + independent connection authority
  + independently authorized secret reference when authentication is needed
  + the same SHA-256 verification floor
```

That adapter does not exist, is not selectable, and is not simulated by reading an HTTP cache through the filesystem adapter. Scheduling/event initiation is a separate future `CallTriggerAuthority` decision and likewise cannot invoke acquisition in this slice.

**Rationale:** Naming the seam prevents filesystem success from becoming an excuse for ambient network or credential behavior later.

### D1.15 — The direct-operator capability satisfies filesystem source-effect placement

The operator knowingly accepts the D1.5 interpretation of `PatinaRegistrySyncQuarryDisposition.SourceAccessBecomesToyAdapter`: operator-driven filesystem acquisition is effected by the private one-shot filesystem capability, not by a Child Toy. ToyGrants govern powers exercised for a Child during a call; this acquisition has no acting Child, assignment, or call. The capability remains kernel-minted, source/action/attempt scoped, current, consumed by the adapter, and independent of source trust. A future network source still requires the named egress Toy and cannot inherit this interpretation as ambient network authority.

### D1.16 — New approval of historical-unknown artifacts is knowingly frozen

The operator knowingly accepts the honest compatibility cost in D1.9: pre-law `historical_unknown` artifacts cannot receive new approvals and there is no override flag. Existing exact approvals remain visible and retain their pre-law authority status. A future separately gated reconciliation command must prove equivalent acquisition evidence before a historical artifact can become acquisition-backed and receive new approval.

### D1.17 — Size-bound and staged-sidecar claims are truthfully limited

Both `MCT_CHILD_MANIFEST_MAX_BYTES` and `MCT_COMPONENT_ARTIFACT_MAX_BYTES` are new constants introduced by this slice; the baseline workspace has neither. For `artifacts stage`, generated SHA-256 sidecars prove that bytes survived staging and publication unchanged. They do not independently authenticate the source bytes from which they were generated. Source authenticity on that path rests on the authenticated operator-pointed decision and, when supplied, its pre-effect expected BLAKE3 digest.

## Persistence Contract

The state schema adds current/query projections for:

```text
artifact_source_authorities
operator_pointed_artifact_acquisition_decisions
artifact_acquisitions
component_artifact_acquisitions
component_artifact_packages
```

`component_artifacts` gains provenance status and a primary acquisition reference. Source records and operator decisions retain canonical-record digests used for ledger correlation. Acquisition and association rows are append-only; source current state and one-shot decision state are projections of later immutable ledger facts.

The schema enforces at least:

- valid closed enums for authority, decision, acquisition, verification, and provenance states;
- exactly one acquisition authority-path reference;
- mandatory source expiry and non-empty scope JSON validated before storage;
- no component artifact reference from failed/not-reached/rejected acquisition;
- no acquisition-backed artifact without a successful primary acquisition;
- immutable artifact identity/content/manifest facts;
- foreign-key exactness among artifact, acquisition, approval, assignment, and package path; and
- no secret/credential columns.

A migration from schema v6 marks existing artifacts historical unknown before raising the schema version. Migration is idempotent and covered by a database created at the exact pre-slice schema. State summary adds counts for active source authorities, acquisition attempts, failed/rejected acquisitions, acquisition-backed artifacts, and historical-unknown artifacts.

## Acquisition Response Contract

Human and JSON responses project durable evidence only after required storage completion:

```text
ArtifactAcquisitionReport {
  acquisition_id
  authority_path
  source_authority_id?
  operator_pointed_decision_id?
  adapter_effect_authority_ref
  child_name
  artifact_version
  source_ref
  observed_size_bytes?
  observed_digest?
  acquisition_outcome
  verification_outcome
  acquisition_observation_id
  verification_observation_id?
  artifact_id?
  content_hash?
  manifest_hash?
  provenance_status?
  package_path?
  safe_message
}
```

The report contains no component/manifest bytes, credential, secret, environment, unbounded adapter error, or arbitrary source file listing. If the authority or terminal observation cannot become durable, no normal report is emitted. Failure output is typed and safe; detailed I/O errors remain operator-only bounded evidence.

## Failing-Test-First Implementation Order

1. Add the red supervised `slate-manager@0.2.0` integration test and immutable real fixture provenance; capture the failing compile/test before implementation.
2. Add kernel source-scope/effect-authority evaluations and the private one-shot filesystem capability.
3. Add acquisition/provenance domain types to `ComponentArtifact` and focused kernel tests.
4. Add the schema migration, source/decision/acquisition/package projections, constraints, and historical-unknown migration test.
5. Add the bounded filesystem staging/acquisition adapter and canonical package renderer.
6. Add shared SHA-256/BLAKE3 verification and immutable catalog publication transaction.
7. Add shared observation constructors and resident/offline mutation orchestration with append-before-effect ordering.
8. Add `artifacts stage/acquire/show/acquisitions/sources` CLI and UDS routes.
9. Close `registry install/sync` and every direct artifact-synthesis bypass.
10. Change exact approval to read catalog evidence, require `--artifact`, and surface evidence before authority append.
11. Refactor resident child resolution to the exact assigned catalog package and wire existing ToyGrant-backed Slate host adapters.
12. Add failed-attempt, append-failure, revocation/expiry, tamper, historical migration, and reopen proofs.
13. Add Track 3 rows before declaring implementation complete.
14. Update TODO/runbook/milestone docs only after the integration proof and attribution diff pass.

Every implementation commit is one concern and must pass:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

Any commit changing Allium additionally requires:

```bash
allium check layer/allium
allium analyse layer/allium
```

This SPEC does not authorize an Allium amendment.

## Required Integration Proof

The primary landed test must prove the daily-driver gate from isolated disk state and cite each step by file:line in close-out:

1. Load the committed real `slate-manager@0.2.0` raw fixture and verify its provenance metadata, exact component hashes/version/exports, and absence of source `.sha256` sidecars; copy only manifest/component to a temporary read-only source root.
2. Create isolated absolute service root, config, identity, children/catalog, state, ledger, UDS, logs, executable, Slate project, and fake-launchd paths; install and start through the existing fake supervisor with typed simulated launch context, then await real resident UDS readiness.
3. Submit `artifacts stage` through owner-authenticated resident mutation ingress with exact `slate-manager`, `0.2.0`, source root/relative paths, and a known expected BLAKE3; prove the operator-pointed decision and adapter-start facts are durable before the first source read/write effect.
4. Verify staging leaves source bytes/modes unchanged, creates canonical `child.toml`, manifest-declared component, and exact SHA-256 sidecars only in isolated MCT paths, and publishes the package under the component SHA-256 catalog path.
5. Inspect the terminal `ArtifactAcquisition`: operator-pointed authority, one-shot filesystem effect reference, credential-free source, observed component size/BLAKE3, `acquired/verified`, acquisition and verification observation ids, and exact `component_artifact_id`.
6. Inspect the `ComponentArtifact`: exact child/version/export shape, SHA-256 component/manifest hashes, `acquisition_backed`, and acquisition id; prove no approval, assignment, instance, or ToyGrant was created by staging.
7. POST a real `patina:slate/control@0.1.0.list-work` submission to the supervised resident UDS and prove typed denial before approval/assignment with no component or Toy effect.
8. Run exact `children approve slate-manager --artifact sha256:<digest>` through resident mutation ingress; capture the human/structured evidence projection before mutation, then prove separate durable `ChildApproved` and `ChildAssigned` facts name that exact digest and scope.
9. POST the same real Slate operation with a fresh idempotency key before ToyGrants; prove denial occurs before component business execution or filesystem/Git/metric/log effect.
10. Run existing `toys authorize-slate` for the exact assigned artifact and temporary project; prove the four current grants are scoped to that artifact/assignment/node/Vision/project resource and no manifest need or acquisition evidence created them.
11. POST `list-work` with a fresh idempotency key and `/project` arguments through supervised UDS; prove the real 0.2.0 component executes, reads the prepared Slate project through the authorized project preopen, and returns the expected real work record/result bytes.
12. Inspect UDS `/runs`/`/snapshot` plus ledger entries and correlate call construction, child/route authority, ToyGrant evaluations/capabilities, runtime start/completion, result, acquisition, verification, approval, assignment, and adapter observations without payload bytes or credentials in the ledger.
13. Revoke the child through the existing command, submit a fresh-key identical operation, and prove current approval/assignment revocation denies before another component/Toy effect even though package, acquisition evidence, and ToyGrant snapshots remain.
14. Stop the supervised resident cleanly; reopen the validated ledger, SQLite state, config, source projection, acquisition/artifact/package projection, run records, and immutable package from disk. Verify every prior fact and exact cross-reference survives, historical-unknown count remains zero in this fresh store, and the final denial/revocation remains current.
15. Restart once from the same governing supervisor record, submit another fresh-key call, prove revocation still denies, stop/uninstall supervision, and verify uninstall preserved package bytes, acquisition/source evidence, artifact state, authority state, runs, ledger, identity, and Slate project while fake launchd policy/current supervisor record alone are removed.
16. Prove the test never reads after fixture copy from or mutates `~/.patina/plugins`, the sibling Slate checkout, real `~/Library/LaunchAgents`, or machine launchd.

Uncited “matched” claims are rejected. Close-out must include a 16-row proof diff with exact landed test file:line ranges and any difference stated explicitly.

## Additional Required Failure Proofs

Named tests must also prove:

- standing source creation rejects every missing scope dimension, implicit broadness, missing/invalid expiry, relative/non-file/credential-bearing source refs, and unsupported actions/policies;
- standing acquisition rejects absent, expired, revoked, stale-policy, unobserved, ledger-digest-mismatched, wrong-root, symlink-escaping, artifact, publisher, namespace, and action scope;
- both authority paths deny without a fresh matching adapter effect authority, while adapter authority alone cannot establish source trust;
- operator-pointed decisions are consumed after both success and failure and cannot be reused;
- missing/mismatched SHA-256 sidecars, expected BLAKE3 mismatch, manifest claim mismatch, malformed export, oversize component/manifest, special files, and partial reads create attempt evidence but no artifact;
- append failure before adapter start leaves source unread and catalog untouched;
- writer loss after a started read yields no acknowledged success or artifact authority;
- catalog/storage failure after verified acquisition leaves durable verified attempt evidence but no falsely referenced artifact or success response;
- reacquisition of identical bytes adds evidence to one immutable artifact, while same-id/different-fact and digest-path collisions fail closed;
- pre-v7 state migration marks existing artifacts `historical_unknown` with empty acquisition ids, never fabricates observations, and refuses new historical-unknown approval;
- `registry install`, `registry sync`, `children load`, lifecycle warmup/reload, startup scan, and test fixtures cannot create a post-law artifact without acquisition;
- approval refuses a child-name-only request, wrong exact digest, failed acquisition, historical unknown, tampered package, and evidence projection failure before authority append;
- source authority/acquisition records and all observations contain no credential fields or values; and
- revocation remains effective after state/ledger/resident reopen.

## Track 3 Attribution Gate

Before close-out, `layer/surface/build/spec-drift-audit/track3/LEDGER.md` receives an explicit row for every implemented acquisition obligation. At minimum this slice must disposition all of:

### `MctArtifactAcquisitionAuthority`

- `AcquisitionRequiresExplicitAuthorityPath`;
- `StandingSourceAuthorityIsExplicitAndBounded`;
- `OperatorPointedAcquisitionCreatesNoAmbientTrust`;
- `SourceTrustAndAdapterAuthorityAreIndependent`;
- `DigestVerificationIsUnwaivableFloor`;
- `VerificationGatesArtifactRecord`;
- `AcquisitionGrantsNoLifecycleAuthority`;
- `AcquisitionIsIndependentLifecycleFact`;
- `FailedAcquisitionCreatesNoArtifact`;
- `UniformAcquisitionProvenance`;
- `HistoricalUnknownProvenanceIsExplicit`;
- `UpdatesRequireExactArtifactApproval`;
- `ChannelSimilarityCannotTransferApproval`;
- `RevocationCannotApproveReplacement`;
- `PreauthorizedChannelsRequireNewAuthorityLaw`; and
- `SourceCredentialsRemainSeparateAuthority`.

### `MctChildComponentLifecycle`

- `AcquisitionIsFifthIndependentFact`;
- `NewArtifactsRequireAcquisitionProvenance`;
- `ArtifactIsImmutableValue`;
- `ApprovalIsAuthorityNotRuntime`;
- `AssignmentIsScopedBinding`;
- `CallsRequireReadyAuthorizedInstance`; and
- `LifecycleTransitionsAreObserved` for acquisition/verification/approval/assignment/revocation facts exercised here.

### `PatinaRegistrySyncQuarryDisposition`

- `GenericRegistryMechanismBecomesMctProduct`;
- `SourceAccessBecomesToyAdapter` — disposition must name the D1.5 direct-operator filesystem effect capability and the explicit future egress-Toy slot, not claim a new Child Toy exists;
- `RegistryAuthorityRemainsKernel`;
- `AcquisitionFactsAreEvidenceNotGrants`;
- `PatinaSourceMeaningRemainsChildMeaning`;
- `RegistryToolingIsOptionalAndOutsideKernel`;
- `AmbientRegistryShapeIsRejected`;
- `RecurringSyncAwaitsSchedulingLaw`; and
- `SourceCredentialsRemainIndependent`.

The generated structural obligations for `ArtifactSourceScope`, `ArtifactSourceAuthority`, `OperatorPointedArtifactAcquisitionDecision`, `ArtifactAcquisition`, their readers/projections, and the new `ComponentArtifact` fields must also be added or explicitly grouped with named serialization/projection tests.

No invariant receives an implicit waiver. Network adapter, update-channel, trigger/scheduling, historical reconciliation, and credential-attachment execution rows may be `DEFERRED` only where the law and this SPEC name the future gate and no latent path was added.

## Verification and Close-out Contract

Close-out is reconstructed from disk, not session memory. It must contain:

- expected starting commit/tree and final full commit range;
- the captured failing-test-first transcript;
- every per-commit validation result;
- standing checks:

```bash
allium check layer/allium
allium analyse layer/allium
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

- `--nocapture` transcripts for the primary integration test and every named failure test;
- a flake log containing the first failure verbatim before one rerun, or exactly `none`;
- the 16-row Required Integration Proof diff with `test-file:line` citations;
- every Track 3 row or named operator-approved waiver;
- fixture provenance, exact SHA-256/BLAKE3 values, and proof it is real `slate-manager@0.2.0`;
- proof that ratified Allium law was unchanged, or an operator-gated stop if conflict was found;
- the Task 3 docs-only commit; and
- the daily-driver statement: Slate's one-node supervised acquisition-to-execution fixture is proven, while `folder-watch-actor@0.1.0` and `watch-null-sink@0.1.0` remain fixtures two and three before the full three-fixture `patinaMother` replacement test can close.

## Build Readiness

**RATIFIED — implementation authorized.**

The operator ratified D1.1–D1.14 and D1.15–D1.17 before the first failing test. Implementation proceeds in the specified failing-test-first order; genuine design forks or conflicts with ratified Allium law stop at the operator.

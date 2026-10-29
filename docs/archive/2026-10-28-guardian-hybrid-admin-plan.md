# Archived guardian/hybrid administration plan

Superseded for new administration by the owner's 2026-10-28 passkey/day-key decision.
This is reference material, not a requirement to enable guardian ceremonies.

The complete implementation is preserved at checkpoint `1c1d727` and remains
in-tree, including every `Ed25519MlDsa65V1` test. Do not reinterpret old ledger
facts or silently change their hash/signature contracts.

Implementation/spec index (all paths relative to the repository):
- `crates/mct-kernel/src/admin_sig.rs`, `admin_issuance.rs`, `recovery/`
- `crates/mct-observation/src/admin_genesis.rs`, `admin_issuance.rs`, `guardian_recovery.rs`
- Their adjacent tests, including `admin_genesis_tests.rs`, `admin_issuance_tests.rs`,
  `guardian_recovery_tests.rs` and `guardian_policy_tests.rs`
- `layer/surface/build/feat/voice-guardian-recovery/`
- `layer/surface/build/feat/admin-issuance-ceilings/`
- `layer/surface/build/feat/strict-admin-cutover/`

Recover exact source with `git show 1c1d727:<path>`; nothing is deleted.

## Original decisions

### D-54 Voice-bound guardian recovery in the completion release

- **Date:** 2026-10-19.
- **Status:** decided, not built. Owner explicitly approved the complete
  first-decision contract and requested implementation.
- **Decision:** adopt [guardian-recovery-v1.md](design/guardian-recovery-v1.md):
  one administrative owner Voice per Mother, stable Voice identity separate
  from transport and credentials, explicit same-Voice bindings, and 2-of-3
  explicitly appointed/accepting guardian Mothers with distinct hybrid keys.
  Recovery changes only the affected Voice–Mother administrative binding.
- **Enrollment:** initial activation requires current-owner authorization and
  all three guardian acceptances. Later roster changes/removal/disablement
  require the current owner and existing quorum, acceptance by added guardians,
  and a 24-hour delay. No roster edits during recovery. A peer or matching
  Voice label is not a guardian; old Patina UID files are not identity proofs.
- **Recovery:** target-local canonical maintenance transition; exact replacement
  keys with proof of possession; separate hybrid quorum signatures; 24-hour
  minimum delay, seven-day completion window, short-lived approvals (maximum
  ten minutes), and a fresh completion quorum. No unverifiable offline wait.
  The old root can alarm but cannot unilaterally veto; cancellation uses quorum.
- **Completion:** atomically replace the root, advance authority, consume the
  recovery request, and invalidate old day grants without erasing history.
  Restore administrative access first; require explicit approval of the exact
  authority snapshot before ordinary work resumes.
- **Amendments:** supersedes D-16 only for this explicitly enrolled path, brings
  D-41 into implementation scope, and removes D-52's guardian exclusion for this
  bounded release. Ordinary admin revocation remains unable to replace the root.
  No usable backups/quorum still means lockout. Host/endpoint compromise,
  destroyed canonical state, or unverifiable continuity require separate
  restoration proof or explicit quarantine/retirement, not invented recovery.
- **Limits:** no Child takeover, shared ledger, global Voice-key rotation,
  guardian co-signing of ordinary work, or claim that hybrid signatures make
  Iroh transport post-quantum-secure. Two colluding guardians have recovery power;
  shared administrative custody weakens independence. Other owner gates remain
  pending except where this contract explicitly settles recovery behavior.
- **Evidence obligation:** implement through bounded specs, strict replay and
  negative/crash/race proofs, then real-process and Devin-run disposable-host
  drills. Keep `key-recovery` open until the complete path has real evidence.
  This approval does not authorize an early push, merge, or live deployment.

### D-55 Disabled guardian protection releases the former roster

- **Status:** ratified by the owner after explicit recommendation; implementation
  follows the bounded recovery slice. Resolves former open question 13.
- **Decision:** after current-owner + existing-quorum authorization and the
  24-hour disablement delay commit, the former roster has no remaining authority.
  Its records remain audit history, not a dormant veto or a recovery capability.
- **Re-enablement:** current owner authorization, acceptance by all three
  proposed guardians, and a fresh 24-hour delay. No old approvals carry forward.
  Disabled protection is not absent history or permission to reset generations.
- **Tradeoff accepted:** a subsequently compromised owner may select a new roster;
  the prior quorum deliberately authorized leaving protection. Avoid permanent
  dependence on old guardian keys after their holders approved disablement.
- **Scope:** clarifies D-54 without changing its active-roster quorum, delay or
  cancellation rules. Temporary suspension with retained guardian control is not
  implemented by overloading disablement. Other release gates remain open.

### D-56 Explicit administrative issuance ceilings

- **Status:** ratified by the owner; implement through the bounded issuance slice.
- **Decision:** retain node-wide administrative route scopes for this release,
  without claiming project/resource isolation. Issuance authority is distinct
  from permission to execute those routes.
- **Root:** a current recovery root may authorize explicitly scoped, expiring
  day-to-day grants, but does not directly execute their administrative routes.
  Ordinary issuance never creates or replaces a recovery root.
- **Delegation:** day-to-day key-manage needs an explicit issuance ceiling. It
  cannot issue execution scopes or a delegated ceiling outside that ceiling,
  extend a recipient beyond its own expiry, or create a recovery grant. Merely
  having key-manage is not unbounded issuance authority.
- **Scope:** settles owner gate 2. It does not ratify gate 3's signature/legacy
  cutover, recipient-bound Child handles, supervisor trust or deployment access.
  Release remains gated on complete executable evidence, not this decision alone.

### D-57 Strict hybrid administration and fresh cutover

- **Status:** ratified by the owner after the Signal/Argent/Iroh comparison and
  explicit recommendation. Settles owner gate 3, not gates 4–5.
- **Authentication:** new administrative commands require both Ed25519 and
  ML-DSA-65 verification and current canonical authorization. Possession alone,
  an Iroh EndpointId, a cached connection, or an OS UID does not grant authority.
- **Bootstrap:** self-signing is limited to the explicit trusted virgin-root
  bootstrap. A separately pinned root and verified virgin disk evidence are
  required; being first to connect never appoints an owner. Bootstrap cannot
  replace missing/nonvirgin history or become a second root-rotation mechanism.
- **Cutover:** authenticated Iroh administration replaces UID/offline alternatives
  under D-15. A local CLI uses that same authenticated path. Supervisor and
  trusted setup implementation remain bounded by the separately pending gate 5.
- **Legacy:** preserve unsigned histories for inspection, without silently
  activating their grants, configuration permissions or self-signed assertions
  as new authority. A signed migration, if desired, requires separate approval.
- **Continuity:** D-54/D-55 guardian recovery and D-56 issuance limits remain in
  force. Each Mother enforces its own canonical history; no fleet consensus or
  central account service is added. Hybrid signatures do not upgrade Iroh's
  transport to post-quantum security.
- **Evidence:** bounded bootstrap/replay and denial proofs precede live ingress.
  Keep administration/release obligations open until the full cutover is proven;
  no early push, deployment, or compatibility-mode authorization bypass.

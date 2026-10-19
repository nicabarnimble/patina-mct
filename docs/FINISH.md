# Finish MCT: one runtime, one to six independent Mothers

**Status: in progress, not release-ready.** Owner-directed scope is D-52.
Branch `feat/finish-mct-federation` starts at `ba685af`, including the unmerged
ledger-first dispatch fence. No merge, deployment, or live service changes are
implied. Allium remains frozen and outside the active checks.

## Run the check

```sh
python3 scripts/check-finish.py             # run implemented proofs, then report blockers; nonzero while unfinished
python3 scripts/check-finish.py proofs      # only implemented proofs; green does NOT mean finished
python3 scripts/check-finish.py status      # fast inventory; nonzero while unfinished
python3 scripts/check-finish.py release     # proofs + full Tier 0 + docs + completion inventory
```

`docs/finish-checks.json` is the machine-readable inventory. Implemented entries
require named executable proof groups and checked-in evidence. The runner checks
that filters actually select tests and that no selected tests are ignored or
missing from the result. Unknown/missing requirements or malformed status fail
closed. Manual status edits are reviewable claims, not automatic ratification;
completion requires the actual listed proofs plus operator review of evidence.

Tier 0 validates this inventory and its runner tests, and runs the Rust proofs
through the workspace suite. Tier 0 is allowed to pass during development; the
separate **completion** check deliberately remains red while obligations are
open. No permanent failing completion job is added to ordinary CI.

## Current owner and environment gates

[finish-owner-gates.md](design/finish-owner-gates.md) proposes explicit choices
for recovery compromise, issuance ceilings, signature/legacy handling,
recipient-bound handles, and trusted supervisor lifecycle. Recovery is ratified
in D-54/D-55 and issuance ceilings in D-56. Signature/legacy handling, handles and
supervisor gates remain pending owner confirmation. Multi-host
partition and upgrade evidence also requires approved disposable test hosts.
Remote macOS execution is to be handed off to Devin with explicit actions and
required evidence, not assumed direct access from this session.

D-54 ratifies [guardian-recovery-v1.md](design/guardian-recovery-v1.md) for this
completion effort: same-Voice 2-of-3 recovery of an administrative OwnerBinding.
This supersedes retirement-only policy for the explicitly enrolled path, not
for compromised hosts or missing canonical history. Implementation and release
proofs remain open; the completion inventory remains red.

The owner requested pushing `feat/finish-mct-federation` to `origin` only when
complete. Run the full `release` check before that push; do not substitute an
ordinary passing Tier-0 run or waive open inventory requirements.

## Bounded deployment contract

- Each Mother owns a distinct identity/key, exclusive ledger writer, runtime
  state, authority, resources, and lifecycle. One shared, explicitly admitted
  Vision; no shared SQLite or global policy counter.
- Direct bilateral signed relationships. Full mesh is optional: six Mothers
  have at most 15 pairs / 30 directional admissions. No implicit transitive
  trust. Each receiver authorizes locally; shared Vision is not permission.
- Single-hop forwarding stays. A disconnected Mother stops work dependent on
  it, not unrelated local work elsewhere. No takeover or guessed effect replay.
- Keep today's one-local-Vision and recorded identity immutability for this
  release. Multi-Vision hosting and membership changes are not prerequisites.
- Current release target is macOS arm64. Six local subprocesses are executable
  evidence, not proof of multi-host isolation or supported Linux installation.
  If Unraid/Linux Mothers are chosen, add Linux supervisor/package/upgrade
  evidence to `release-evidence`; do not relabel existing macOS proof.

## Ordered completion checklist

### 1. Federation correctness and executable baseline

- Implement issuer-specific policy checks and stale-authority denial.
- Exercise six real processes, 30 direct remote routes, six local routes,
  receiver-side revocation, four/five surviving nodes, and restart.
- Ensure peer refresh does not suspend serving or shutdown; refresh at boot.
- Preserve the single-hop, wrong-Vision, signature, expiry, publication, and
  receiver-authority denial tests.

Evidence/status: `independent-policy`, `fleet-baseline`; see
[federation-finish/EVIDENCE.md](../layer/surface/build/feat/federation-finish/EVIDENCE.md).
The fixture uses today's local admin surface and an offline revision setup.
It does not prove the final admin path or full canonical peer/Child recovery.

### 2. Canonical authority and final execution admission

- Reconstruct Child approvals/assignments and peer authority from validated
  canonical facts. Lost or rolled-back config must not resurrect authority.
- Finish final Child admission/revocation ordering without broadening keyed or
  unkeyed semantics. D-40 remains proposed; do not silently adopt a call-ID-only
  global execution guard.
- Define separate task, invocation, effect, and checkpoint identities. Missing
  external-effect outcome stays unknown unless reconciled by an explicit
  effect-specific protocol. Durable tasks do not require a whole-Pi WASM port.
- Retain the bounded dispatch fence and its parser/crash/fuzz evidence. It is
  not durable task resume, forever deduplication, or exactly-once execution.

Status: `keyed-retry-fence` is implemented on the inherited branch;
`canonical-authority` and `final-admission` are open.

### 3. Administration and recovery cutover

- Implement D-56 issuance ceilings, settle signature enforcement, implement authenticated
  Iroh administration, and remove the old bypass paths in the same cutover.
- Prove bootstrap, expiry, delegation, revocation, and no resurrection.
- Implement D-54's explicit Voice-bound guardian recovery exception, preserving
  ordinary-admin denial of root replacement, loss/compromise boundaries,
  canonical enrollment, delayed quorum recovery and exact-snapshot resumption.

Status: `administration`, `key-recovery`. Existing settled decisions stay in
force until explicitly amended. A completion request is not approval of an
unrecorded key-policy redesign.

### 4. Operator lifecycle and distributed failure matrix

- Provision multiple instances without service-label/path/key collisions.
- Provide inventory/inspection tooling without creating a runtime coordinator
  whose outage stops everyone. Inventory is not a grant source.
- Exercise partition, reconnect, stale publication/binding, lost reply, retry
  correlation, peer restart, quarantine, and rolling upgrade on chosen hosts.
- Document readiness versus reachability; bound peer-refresh/retry delays and
  preserve per-Mother evidence without claiming a total global ordering.

Status: `fleet-operations`, `distributed-failures`.

### 5. Useful workflow and release acceptance

- Produce an artifact through an authorized build and bind approval to its
  exact digest before publication; deny an unauthorized requester/artifact.
- Show independent local work during an outage and no blind effect replay
  after lost responses. PiD may be the client, not the authority ledger.
- Verify current artifacts, upgrade/recovery procedures, security claims and
  supported-host matrix. Owner reviews the final evidence before release.

Status: `useful-workflow`, `release-evidence`.

## Excluded, not silently deferred blockers

No consensus, shared/replicated ledger, HA takeover, cross-Vision federation,
arbitrary multi-hop routing, cluster scheduler, full Pi WASM port, or every
proposed Toy. D-54 explicitly includes bounded Voice-bound guardian recovery;
it does not authorize cluster consensus, takeover, or shared state. Existing preopen lifetime, secret-value-return and
application/sandbox boundaries remain documented limits, not stronger claims.

The finish line is the inventory above. Adding another architecture programme
requires an explicit scope decision, not simply discovering another idea.

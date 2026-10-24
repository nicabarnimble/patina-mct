# Remaining owner decisions

**Status: recovery ratified in D-54/D-55; issuance in D-56; strict hybrid/fresh cutover in D-57. Decisions 4–5 remain pending.**
These are decisions already called out by `SECURITY-BOUNDARIES.md`,
`OPEN-QUESTIONS.md`, and D-52, not an expansion of the finish line. The tested
groundwork is merged in PR #81; PR #82 retired unused archive tooling. Those
merges do not waive the completion inventory or replace settled contracts.

## Active recovery design update (D-53)

The owner has requested guardian recovery receive active attention during this
multi-Mother completion effort, informed by Signal, Iroh, and Argent research.
[guardian-recovery-v1.md](guardian-recovery-v1.md) is now ratified by D-54:
Voice-bound, same-Voice 2-of-3 guardian recovery with explicit enrollment,
delays, quorum cancellation, and preserved canonical history. The original
retirement-only recommendation below is superseded for this enrolled path.
The runtime still enforces D-16 until implementation; the completion obligation
remains open. D-56 also settles issuance ceilings; the remaining owner decisions are
listed below. Canonical recovery and protected policy
edits now have executable evidence; authenticated live ingress remains disabled.
D-55 releases former guardians' authority after authorized delayed disablement;
re-enablement requires current owner, all three fresh acceptances and a new delay.

Remote macOS validation will be handed off to **Devin**, not performed through
assumed direct host access here. Approved hosts/actions and returned evidence
still need to be specified; this does not authorize deployment.

## Original security recommendations requiring confirmation

1. **Historical proposal: retirement only (superseded in scope by D-54).** Keep D-16: no in-history
   revocation/replacement of the recovery grant. Lost recovery custody leaves
   no recovery path; existing valid day-to-day grants are not automatically
   revoked. If recovery custody is compromised, stop and retire that identity,
   revoke its peer admissions at other Mothers, rotate affected external
   credentials, and explicitly bootstrap a new Mother. Preserve the old
   evidence; do not wipe it or pretend the replacement continues its authority.
   This is an operational response, not a promise that an attacker-controlled
   host can be remotely erased or made to stop.

2. **Explicit issuance ceilings, separate from execution scopes (ratified D-56).** Keep the
   current node-wide route-class scopes for this release; do not claim project
   or resource isolation for admin keys. Recovery can issue explicitly scoped,
   expiring day-to-day grants but cannot directly execute their routes.
   Day-to-day `key-manage` must have an explicit delegation ceiling, cannot
   issue outside that ceiling or beyond its own expiry, and cannot create a
   recovery grant or delegate a larger ceiling. Merely possessing `key-manage`
   is not unlimited issuance authority. D-56 now has an explicit grant-schema,
   bounded kernel evaluator and canonical issuance proofs. This is separate from
   the route evaluator and does not enable the still-gated authenticated runtime.

3. **Strict hybrid administration and fresh cutover (ratified D-57).** Require both signature
   halves and authorized issuance on new admin commands. Proof of possession
   alone is insufficient. Self-signing is allowed only for the explicit virgin
   bootstrap root; subsequent issuance requires an admitted issuer. Follow
   D-15's hard cutover: do not keep UID/offline administration as an alternative.
   Preserve old unsigned histories for inspection, but do not silently promote
   them or configuration-held permissions into new active authority. A future
   signed import procedure, if wanted, is a separately approved contract.

4. **Recipient-bound Child handles.** A handle names issuer Mother/epoch,
   recipient key, operation/resource scope, expiry, and its current grant
   lineage. Possession must be accompanied by proof of the named recipient.
   Delegation is a Mother-issued narrower grant, not copying a bearer token.
   Revocation applies to new admissions; already-admitted preopen lifetimes
   retain D-G7's documented limit. This settles the bearer-versus-recipient
   question rather than quietly choosing one while replacing local call ingress.

5. **Trusted process lifecycle is not an authority bypass.** The OS supervisor
   may start/stop the installed, pinned Mother binary. Starting it must replay
   the existing identity and ledger; it cannot bootstrap replacement authority
   over missing/nonvirgin state or rewrite grants. Authority mutations and the
   recovery startup gate require the authenticated Iroh path. Deployment of
   virgin public bootstrap configuration remains an explicit trusted setup
   action, not an ongoing alternate admin interface.

Approval should name any changes to these recommendations before implementation.
Decisions 1–3 are settled by D-54/D-55, D-56 and D-57 respectively; their complete runtime integration is not claimed here.

## Canonical-authority implementation boundary

Current `PreparedChildMutation::decision_observations` records artifact/name
and revision, but not a complete approval/assignment scope. Peer decision facts
similarly omit the complete binding and reverse-proof records. Those historical
observations cannot reconstruct facts that were never recorded.

The corrective path needs complete typed canonical records, strict replay,
writer-ordered mutations with stale-intent protection, and consumers that use
canonical state rather than trusting config. A persisted revocation must beat
rolled-back config both during execution and after restart. Projection failure
cannot undo commitment. Existing snapshot/mutation generations and hashes must
include the new records without silently changing old hash contracts.

Keep existing keyed/unkeyed semantics while fixing final launch ordering; do
not silently adopt D-40's proposed global call-ID once guard. These are already
bounded by the current finish plan, not new owner choices.

## External validation access

The supported release target remains macOS arm64. The current evidence covers
six real processes on one host, not a physical multi-host network partition or
rolling upgrade. To close `distributed-failures` and `release-evidence`, identify
approved disposable test hosts and permitted test actions. Do not use production
Mothers, deploy to PiD/Unraid, or claim Linux release support without the relevant
approval and platform evidence.

The external Patina spec-manager is also currently unavailable. Local tests
remain executable without it; no daemon should be started simply to satisfy
that external check.

## Publication is not release

The owner separately authorized publication and merging of the unfinished,
tested groundwork on 2026-10-19. PRs #81/#82 are merged; the eight completion
obligations remain open. Before release, `python3 scripts/check-finish.py release`
must pass with every completion obligation genuinely supported by evidence.
Ordinary Tier-0 green and a merged PR are not release approval.

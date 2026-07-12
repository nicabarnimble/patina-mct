# Track 2 — product-map tend pass

## Operator prompt (verbatim)

```text
Track 2 tend pass for patina-mct: grow layer/allium/mct-product-map.allium
to describe what the system now actually promises. Every edit in this
pass must trace to an already-recorded decision — the peer-ontology
session's tend-pass inputs, the spec-drift audit's code-ward findings
(B1-B5), the slice-6 idempotency contract, and the Track-1 operator
gate rulings. You are transcribing adjudicated decisions into semantic
law, not making new ones. If an input is ambiguous or two inputs
conflict, STOP and report — do not resolve semantics yourself.

## Step 0 — Re-establish state (STOP and report if anything differs)

a) Branch `patina`. Expected HEAD: 7a13578 (docs: define peer Mother
   ontology draft) or a merge commit containing it — verify
   layer/allium/mct-peer-ontology.allium exists and `allium check`
   passes on it; STOP if absent. Commit pending session artifacts via
   your normal flow; tree otherwise clean.
b) Read, in this order: layer/allium/mct-peer-ontology.allium (the
   ratified companion law — you will reference it, never duplicate
   it); the tend-pass input list (22 items) and decision log in
   layer/sessions/20260712-112719-785420000.md;
   layer/surface/build/spec-drift-audit/REPORT.md findings B1-B5 and
   A4 with citations; the slice-6 idempotency contract in
   layer/surface/build/spec-drift-audit/track1/TASKS.md (the
   operator-defined 8-point contract plus implemented constants and
   typed outcomes); the Track-1 gate decisions recorded in the same
   file (control-plane mutation architecture, identity offline-only,
   hello observation ordering); then the full current
   layer/allium/mct-product-map.allium.

## Working principles (binding, tend-specific)

- PRESERVE NAMES: existing invariant, contract, and decision
  identifiers in the map are referenced by the audit report, tests,
  and this conversation's record — never rename or delete one. Grow
  by adding decisions/invariants and enriching existing prose.
- NEVER WEAKEN silently: if a recorded decision genuinely supersedes
  existing map text (expected for the idempotency line and the
  origin-related text A1's resolution touches), the edit must keep
  the old meaning legible — amend the text and add a dated
  `-- Decision:` note stating what changed and which artifact decided
  it (session id, REPORT finding id, or track1 gate).
- REFERENCE, don't duplicate: peer-relationship semantics live in
  mct-peer-ontology.allium; the map's peer sections gain by-name
  cross-references to its contracts, not copies of them.
- CONSTANTS are described, not frozen: landed values (32 KiB caps,
  96 KiB budget, 8 MiB blob cap, 720s idempotency TTL, 256-entry
  budget) may be recorded as current disclosed values in decision
  notes; invariants state the BOUNDED-BY-NAMED-CONSTANT obligation,
  not the number. The 300s publication freshness stays an open
  question per the ontology — do not harden it here.
- Scalpel commits, one concern each, ordered as the tasks below.
  Validation after EVERY commit:
  allium check layer/allium && allium analyse layer/allium/mct-product-map.allium && allium analyse layer/allium/mct-peer-ontology.allium && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
  Flake protocol: capture failures verbatim before rerunning (create
  layer/surface/build/spec-drift-audit/track2/TASKS.md with this
  prompt verbatim and a checklist; commit it first with T1).

## Task T1 — Epistemic header and companion cross-references
(one commit: `docs: state map epistemics and peer companion`)

Add to the product map's header block, using this agreed wording as
the substance: "allium check and analyse establish well-formedness
and internal analysis only. Intent/code alignment is evidenced by
dated weed audits and named contract tests." Add: the first audit's
date and report path, and a dated note that peer-relationship
semantics are governed by mct-peer-ontology.allium (companion law,
referenced by name). Add the reciprocal status note to the ontology
file's header (draft → ratified companion, review recorded in the
session artifact). Create track2/TASKS.md in this commit.

## Task T2 — Idempotency: one line becomes the contract
(one commit: `docs: specify request idempotency contract`)

Replace/grow the map's IdempotencyIsRequestScoped area (~line
842-843) with the operator-defined contract as decisions plus
invariants: replay-not-reject with recorded-reply semantics;
caller-identity scope (never global — one caller can never receive
another's result); fingerprint mismatch fails closed; TTL + per-caller
budget with REFUSE-not-evict and the never-silently-re-execute
statement; in-flight duplicates typed-refused; store durability
across restart; replay never bypasses current authority; cross-Mother
idempotency excluded (federation follow-on). Cite the track1 slice-6
record and the four typed 0.x reason variants as the landed wire
disclosure.

## Task T3 — B1-B5: the map learns what landed
(one commit per finding, `docs: describe <area> semantics`, in order
B1, B2, B3, B4, B5)

Per finding, enrich the cited map sections with the landed semantics
the audit documented — payload integrity limits and the local BLAKE3
CAS with its verify-then-atomic-rename store invariant (B1); the
effect-boundary revision guard as a distinct stage composing with
revalidation (B2); route_taken as a caller-safe reply projection with
its outcome-conditional presence rule (B3); the Ed25519
canonical-message binding proof shape and fail-closed enforcement
(B4); expiring Vision-scoped callable-surface evidence in hello
views, referencing the ontology's CapabilityPublication contract for
its meaning (B5). Source each edit from the REPORT.md evidence and
the landed code cited there; where the ontology already owns the
semantic (B5 especially), the map's text points at the ontology
contract rather than restating it.

## Task T4 — The session's 22 tend-pass inputs
(scalpel commits grouped by map section; cite the session artifact
in each message)

Work through the input list in the session artifact. Expected
overlaps: several inputs will already be satisfied by T1-T3 — mark
those satisfied-by with the commit rather than double-editing. Inputs
touching origin/A1 text amend per the recorded resolution:
OriginIsForObservationNotPermission stands; surrounding text gains
the protocol-semantics grounding (mct/call/0 proposes local
execution) by reference to the ontology's TerminalPeerCallSubmission.
Any input that is not clearly decided by the session log: STOP and
report it rather than interpreting.

## Task T5 — A4 disposition and audit-report closure
(one commit: `docs: disposition planner evidence and close audit rows`)

In the map's route-decision record section, add a dated decision:
full phase-2 planner evidence and snapshot recording (A4) is
deliberately deferred to the capability-profile/telemetry future
(C2) — the current deterministic tie-break records selection and
eliminations but not planner scoring, and MUST be revisited when
phase-2 gains real inputs. Annotate REPORT.md: A4 row →
"adjudicated-deferred to C2 planner/telemetry future (tend pass)";
B1-B5 rows → addressed-in-map with their T3 commit hashes; note in
the summary that all 23 findings now have terminal dispositions.

## Boundary

STOP after T5. Do not push, do not merge, do not begin contract-test
propagation. Final report: commits; the 22-input checklist with each
input's disposition (edited-in-commit / satisfied-by / stopped-on);
allium check AND analyse output for both files at final HEAD; any
input you stopped on; anything discovered that belongs in ROADMAP.
The operator reviews the full diff before this becomes law.
```

## Pass checklist

- [x] Step 0: confirm `patina`, a clean post-session-artifact baseline containing `7a13578`, companion file presence, and clean companion check.
- [x] Read the companion law, elicitation decision log and 22 inputs, audit A4/B1-B5, Track-1 gate and Slice-6 records, and full product map.
- [x] T1: state map epistemics, ratify companion status, and create this ledger.
- [x] T2: specify the complete request-scoped idempotency contract.
- [x] T3/B1: describe bounded payload integrity and local content-addressed storage.
- [x] T3/B2: describe the effect-boundary revision guard.
- [x] T3/B3: describe caller-safe `route_taken` reply projection.
- [ ] T3/B4: describe the concrete signed binding proof.
- [ ] T3/B5: describe callable-surface publication evidence by companion reference.
- [ ] T4: disposition all 22 peer-ontology tend inputs without semantic duplication.
- [ ] T5: defer A4 to C2, close B1-B5 audit rows, and record terminal disposition tally.
- [ ] Final validation and operator-review report.

## Tend-input disposition ledger

1. [ ] Companion integration form.
2. [ ] Durable relationship taxonomy.
3. [ ] Contextual role derivation.
4. [ ] Submission-only binding authority and mandatory time bound.
5. [ ] CapabilityPublication meaning.
6. [ ] Publication freshness and unresolved mechanics.
7. [ ] B5 callable-surface publication.
8. [ ] Terminal local execution proposal.
9. [ ] Origin remains observation/dispatch.
10. [ ] Permanent per-hop vouching.
11. [ ] ObservationReplicationAuthorization owns cross-ledger sharing.
12. [ ] Bilateral directional bindings and derived states.
13. [ ] Exact candidate conjunction and egress revalidation.
14. [ ] Operator-pointed individual egress decision.
15. [ ] Reachability is not authority.
16. [ ] B4 proof relationship.
17. [ ] B1 payload/result ingress rationale.
18. [ ] B3 route projection is not peer authority.
19. [ ] Slice-6 authority/replay ordering and scope.
20. [ ] Future protocol authority slots.
21. [ ] Brokered multi-hop is a distinct future relationship.
22. [ ] Optional publication reference remains a route-audit question.

## Validation and flake log

Failures are captured verbatim here before any rerun.

- `docs: state map epistemics and peer companion` — full required gate passed; no flakes.

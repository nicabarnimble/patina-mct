# Route wiring phase tasks

- [x] Task D0 — Housekeeping
- [x] Task D1 — SPEC first (gate: operator reads this before D2 proceeds)
- [x] Task D1.1 — Operator gate amendments
- [x] Task D2 — Kernel gaps only if the SPEC found any
- [ ] Task D3 — Daemon routing for local calls
- [ ] Task D4 — Remote serve-path integration
- [ ] Task D5 — End-to-end proof and PHASE3 T5 discharge

## Flake log

### 2026-07-06 — D2 failing test before route reply wire field

Command:

```bash
cargo test -p mct-kernel call_protocol_reply_roundtrips_route_taken_wire_field -- --nocapture
```

Failure output:

```text
   Compiling mct-kernel v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-kernel)
error[E0425]: cannot find function `call_reply_from_evaluation_with_result_payload_and_route` in this scope
    --> crates/mct-kernel/src/call/mod.rs:1554:21
     |
 896 | / pub fn call_reply_from_evaluation_with_result_payload(
 897 | |     reply_id: ReplyId,
 898 | |     evaluation: &MctCallProtocolEvaluation,
 899 | |     result_ref: Option<ResultRef>,
...    |
 923 | | }
     | |_- similarly named function `call_reply_from_evaluation_with_result_payload` defined here
...
1554 |           let reply = call_reply_from_evaluation_with_result_payload_and_route(
     |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
     |
help: a function with a similar name exists
     |
1554 -         let reply = call_reply_from_evaluation_with_result_payload_and_route(
1554 +         let reply = call_reply_from_evaluation_with_result_payload(
     |

error[E0609]: no field `route_taken` on type `call::MctCallProtocolReply`
    --> crates/mct-kernel/src/call/mod.rs:1571:28
     |
1571 |         assert_eq!(decoded.route_taken, Some(route_taken));
     |                            ^^^^^^^^^^^ unknown field
     |
     = note: available fields are: `reply_id`, `protocol_request_id`, `decision_id`, `result_ref`, `result_payload` ... and 3 others

error[E0560]: struct `call::MctCallProtocolReply` has no field named `route_taken`
    --> crates/mct-kernel/src/call/mod.rs:1590:13
     |
1590 |             route_taken: Some(route_taken.clone()),
     |             ^^^^^^^^^^^ `call::MctCallProtocolReply` does not have this field
     |
     = note: all struct fields are already assigned

error[E0560]: struct `call::MctCallProtocolReply` has no field named `route_taken`
    --> crates/mct-kernel/src/call/mod.rs:1605:13
     |
1605 |             route_taken: None,
     |             ^^^^^^^^^^^ `call::MctCallProtocolReply` does not have this field
     |
     = note: all struct fields are already assigned

error[E0560]: struct `call::MctCallProtocolReply` has no field named `route_taken`
    --> crates/mct-kernel/src/call/mod.rs:1614:13
     |
1614 |             route_taken: None,
     |             ^^^^^^^^^^^ `call::MctCallProtocolReply` does not have this field
     |
     = note: available fields are: `reply_id`, `protocol_request_id`, `decision_id`, `result_ref`, `result_payload` ... and 2 others

Some errors have detailed explanations: E0425, E0560, E0609.
For more information about an error, try `rustc --explain E0425`.
error: could not compile `mct-kernel` (lib test) due to 5 previous errors
```

### 2026-07-06 — D2 targeted test invocation used multiple cargo filters

Command:

```bash
cargo test -p mct-kernel call_protocol_reply_roundtrips_route_taken_wire_field candidate_observations_record_specific_elimination_class candidate_elimination_reasons_expose_denial_class -- --nocapture
```

Failure output:

```text
error: unexpected argument 'candidate_observations_record_specific_elimination_class' found

Usage: cargo test [OPTIONS] [TESTNAME] [-- [ARGS]...]

For more information, try '--help'.
```

### 2026-07-06 — D2 rustfmt check reported formatting diffs

Command:

```bash
cargo fmt --check
```

Failure output:

```text
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-kernel/src/lib.rs:72:
 };
 pub use route::{
     AuthorizedRouteExecution, CandidateAuthorityEvaluation, CandidateAuthorityOutcome,
-    CandidateEliminationClass, CandidateEliminationReason, CandidateRoute, NetworkPathClass, RouteDecision, RouteDecisionIds,
-    RouteDecisionKind, RouteDecisionOutcome, RouteRevalidationIds, RouteRevalidationReason,
-    RouteRevalidationResult, no_route_denied_result, revalidate_route_for_execution,
+    CandidateEliminationClass, CandidateEliminationReason, CandidateRoute, NetworkPathClass,
+    RouteDecision, RouteDecisionIds, RouteDecisionKind, RouteDecisionOutcome, RouteRevalidationIds,
+    RouteRevalidationReason, RouteRevalidationResult, no_route_denied_result,
+    revalidate_route_for_execution,
 };
 pub use toy::{
     AuthorizedToyCall, CanonicalToyContract, ToyContractIdentity, ToyGrant, ToyGrantConstraints,
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-kernel/src/observation.rs:545:
         safe_message: "candidate considered".into(),
         detail_ref: Some(format!(
             "candidate:{};node:{};runtime:{:?};network:{:?}",
-            candidate.candidate_id, candidate.node_id, candidate.runtime_kind, candidate.network_path
+            candidate.candidate_id,
+            candidate.node_id,
+            candidate.runtime_kind,
+            candidate.network_path
         )),
     }
 }
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-kernel/src/route.rs:969:
             CandidateEliminationClass::Structural
         );
         assert_eq!(
-            CandidateEliminationReason::ToyGrantMissing.denial_class().as_str(),
+            CandidateEliminationReason::ToyGrantMissing
+                .denial_class()
+                .as_str(),
             "structural"
         );
     }
```

## Verbatim task prompt

You are starting ROADMAP item 3 in `patina-mct`: routing wired
end-to-end. The kernel's two-phase route decision model (authority
filter → ranking → revalidation at execution) is complete and tested
as decision logic, and the daemon can already source local candidates
— but no daemon path consumes `AuthorizedRouteExecution`. Calls go
where the operator points them; remote serve stamps
`route_decision_id: None`. This phase makes incoming calls flow
through the two-phase decision so that local dispatch is just the
single-candidate case, and discharges the stale-revision-guard
obligation recorded in audit-remediation/PHASE3.md (Task T5 notes).

## Task D0 — Housekeeping

a) Verify state: branch `patina`, expected HEAD 122424d (docs: close
   out payload data plane phase), or a later session-artifact commit
   if your session flow has committed layer/sessions files since. If
   session artifacts are modified and uncommitted, commit them via
   that flow first. Tree otherwise clean except pre-existing untracked
   brew-noncore-report.html. STOP and report on any other mismatch.
b) Read before touching code: AGENTS.md, layer/core/dependable-rust.md,
   layer/core/what-is-mct.md, layer/surface/build/product/ROADMAP.md
   (item 3), layer/surface/build/audit-remediation/PHASE3.md (Task T5
   and its closing notes — the revision-guard obligation this phase
   discharges), the payload-data-plane SPEC.md for the validated call
   order this phase slots into, and the routing sections of
   layer/allium/mct-product-map.allium (the TwoPhaseRouting and
   NoRouteDecision contracts — the spec-derived obligations below
   come from them).
c) Key code surfaces: crates/mct-kernel/src/route.rs (all of it —
   RouteDecision::selected/eliminated/no_route, CandidateRoute,
   revalidate_route_for_execution, AuthorizedRouteExecution and its
   policy_revision/grants_revision accessors, no_route_denied_result);
   crates/mct-daemon/src/children.rs
   (authorized_local_candidates_for_call — candidate sourcing exists);
   the resident call handling in crates/mct-daemon/src/main.rs
   (route_taken currently hardcoded None); the serve path in
   crates/mct-iroh/src/serve.rs (route_decision_id currently None);
   where RouteRevalidated observations are already emitted today.
d) Save this prompt verbatim as
   layer/surface/build/feat/route-wiring/TASKS.md with a checklist
   header; commit: `docs: start route wiring phase`.

## Working principles (binding)

Favor strong invariants over defensive fallbacks. Make bad states
impossible where practical. Do not add complexity to paper over
unclear design. Prefer simple data models, explicit contracts, and
shared logic over local patches, duplicated code, or speculative
abstractions. Write Rust code that Jon Gjengset would agree with.
Always read code before writing code. Git update with scalpel as you
work, not with shotgun after. Kernel decides, adapters perform. Fail
closed. Sealed capabilities stay sealed: no new constructors, no
Clone, by-value consumption at the effect site. No
attribution/branding; no history rewrites. Failing test first for
behavior changes. Stop at a task boundary if context runs low — the
task file on disk is the source of truth.

Validation green after EVERY commit:
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
Flake protocol: capture any failure verbatim in TASKS.md before
rerunning.

## Hard invariants for this phase

- **Two-phase or nothing.** Every executed call passes initial
  decision (authority filter over ALL candidates → ranking →
  selection) then revalidation at execution. No path executes a child
  without consuming an AuthorizedRouteExecution minted by
  revalidate_route_for_execution. Local dispatch is the
  single-candidate case of the same path, not a bypass.
- **Revision guard at the effect boundary (PHASE3 T5 obligation).**
  The adapter consuming AuthorizedRouteExecution compares its
  policy_revision/grants_revision against the CURRENT revisions at
  the moment of execution. Mismatch → typed denial + observation,
  never execution. This composes with, not replaces, the revalidation
  stage.
- **No-route fails closed and typed.** Zero admissible candidates →
  no_route_denied_result path, typed reason, safe caller projection;
  eliminations are observed per-candidate with typed reasons.
- **Kernel purity.** Candidate sourcing, current-revision reads, and
  observation writes stay adapter-side; the kernel decides from facts.
- **Ledger explains every routing outcome.** Initial decision,
  per-candidate eliminations, selection, revalidation, and revision
  denials are all reconstructible from observations. No payload bytes
  (the payload-phase invariant is unchanged and its tests must stay
  green).

## Spec-derived obligations (binding; from mct-product-map.allium
TwoPhaseRouting / NoRouteDecision contracts and allium plan)

- MctResult.route_taken: present when outcome is success, failed, or
  timed_out; ABSENT for denied and cancelled. This is decided by the
  product map, not open for the SPEC to choose — the SPEC states how,
  not whether. main.rs currently hardcodes None.
- D5 must include the adversarial ordering test for
  OptimizationCannotGrantAuthority: among two candidates, the one the
  ranking would prefer fails the authority filter; prove the
  worse-ranked admissible candidate is selected and the preferred one
  was never ranked.
- No-route and elimination observations record the SPECIFIC
  elimination rule class per candidate (never a generic no-route
  message); the caller-safe projection stays concealment-safe while
  the ledger holds full elimination context (dual disclosure).
- Distinguish structural vs temporal denial classes in typed reasons:
  an AUTHORIZED candidate that is unavailable (e.g. child not ready)
  produces the no-route path with a temporal-class reason — the
  planner reports unavailability; it never feeds back into authority.
- Denial is terminal and passive: no retry loop, no fallback
  execution, no implicit grant-request path enters in this phase.
- The ranking key must be non-authoritative by construction; state it
  in the SPEC.

## Task D1 — SPEC first (gate: operator reads this before D2 proceeds)

Write layer/surface/build/feat/route-wiring/SPEC.md (short), deciding
explicitly:
- **Placement in the validated call order**: where the initial route
  decision and the revalidation sit relative to the payload phase's
  steps 1-12 (payload integrity → hello/call authority → child
  authorization → delivery preflight → execution → result capture).
  State the merged order; state which existing daemon step the
  decision subsumes or wraps.
- **Candidate sourcing and ranking inputs**: what the daemon supplies
  per candidate (from authorized_local_candidates_for_call and what
  else), what the ranking keys on, why the result is deterministic,
  and why the ranking key is non-authoritative. Local candidates only
  this phase (fixed): remote candidates/cross-Mother forwarding is a
  recorded non-goal.
- **Consumption contract**: AuthorizedRouteExecution consumed by-value
  at the execution site; where current revisions come from at that
  moment; the typed denial reason and safe text for a revision
  mismatch.
- **Both entry paths**: local CLI/control-initiated calls and remote
  mct/call/0 arrivals go through the same decision path; state what
  changes in serve.rs (route_decision_id populated) and how
  MctResult/reply carries route_taken under the spec-derived presence
  rule above.
- **Denial classification**: the typed reason taxonomy split into
  structural vs temporal classes per the product map's denial
  taxonomy; which class each elimination reason belongs to.
- **Observability mapping**: which ObservationKinds cover initial
  decision, per-candidate eliminations, selection, revalidation,
  revision denial; reuse existing kinds where they exist.
- **Non-goals**: no remote candidates or call forwarding between
  Mothers (record as ROADMAP follow-on under item 6 if not already
  recorded), no retry/grant-request/escalation capabilities, no new
  ranking policy language, no scheduler/load-balancing heuristics, no
  telemetry inputs, no changes to sealed-type mechanics.
Commit it. This SPEC is the contract for D2 onward. STOP at this gate.

## Tasks D2+ (do not start before the gate releases)

Planned shape, refined by the SPEC: D2 kernel gaps only if the SPEC
found any (decision logic is believed complete — do not rebuild it);
D3 daemon wiring of initial decision + revalidation + by-value
consumption + revision guard for local calls; D4 remote serve-path
integration; D5 end-to-end proof covering, at minimum: the adversarial
ordering test from the spec-derived obligations (ranking-preferred
candidate eliminated by authority; worse-ranked admissible candidate
selected); a stale-revision test where a revision bump between
decision and execution produces the typed denial, observed, never
executed; a no-route call failing closed with the specific elimination
rule class in the ledger and only the safe message to the caller; an
authorized-but-unavailable candidate producing a temporal-class
no-route denial; route_taken presence/absence per outcome; full trace
reconstructible from the ledger. Update PHASE3.md's T5 notes to record
the obligation as discharged in the same commit that lands the guard.

## Definition of done

Validation green per commit; hard invariants tested, not just stated;
TASKS.md checked off as you go; final summary: commits, SPEC decisions
made, flake log (or none), D5 transcript, and anything discovered that
belongs in ROADMAP rather than this phase.

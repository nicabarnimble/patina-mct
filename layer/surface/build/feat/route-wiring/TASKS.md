# Route wiring phase tasks

- [x] Task D0 — Housekeeping
- [x] Task D1 — SPEC first (gate: operator reads this before D2 proceeds)
- [x] Task D1.1 — Operator gate amendments
- [x] Task D2 — Kernel gaps only if the SPEC found any
- [x] Task D3 — Daemon routing for local calls
- [x] Task D4 — Remote serve-path integration
- [x] Task D5 — End-to-end proof and PHASE3 T5 discharge
- [x] Task D6 — Phase close-out

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

### 2026-07-06 — D3 failing test before handler route projection

Command:

```bash
cargo test -p mct-daemon resident_execution_runs_wit_child_and_records_trace -- --nocapture
```

Failure output:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0609]: no field `route_decision_id` on type `MctIrohCallHandlerResult`
    --> crates/mct-daemon/src/main.rs:4471:24
     |
4471 |         assert!(result.route_decision_id.is_some());
     |                        ^^^^^^^^^^^^^^^^^ unknown field
     |
     = note: available fields are: `result_ref`, `result_payload`, `inline_result_payload`, `outcome`, `safe_message`

error[E0609]: no field `route_taken` on type `MctIrohCallHandlerResult`
    --> crates/mct-daemon/src/main.rs:4472:24
     |
4472 |         assert!(result.route_taken.is_some());
     |                        ^^^^^^^^^^^ unknown field
     |
     = note: available fields are: `result_ref`, `result_payload`, `inline_result_payload`, `outcome`, `safe_message`

For more information about this error, try `rustc --explain E0609`.
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 2 previous errors
warning: build failed, waiting for other jobs to finish...
```

### 2026-07-06 — D3 compile failure after route execution wiring

Command:

```bash
cargo test -p mct-daemon resident_execution_runs_wit_child_and_records_trace -- --nocapture
```

Failure output:

```text
   Compiling mct-kernel v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-kernel)
   Compiling mct-iroh v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh)
   Compiling mct-observation v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-observation)
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0308]: mismatched types
    --> crates/mct-daemon/src/main.rs:2201:9
     |
2199 |     let provenance = ChildInvocationProvenance::from_authorized(
     |                      ------------------------------------------ arguments to this function are incorrect
2200 |         &child_execution.authorized,
2201 |         child_execution.route_decision_id.clone(),
     |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected `ObservationId`, found `DecisionId`
     |
note: associated function defined here
    --> crates/mct-daemon/src/state.rs:117:12
     |
 117 |     pub fn from_authorized(
     |            ^^^^^^^^^^^^^^^

error[E0004]: non-exhaustive patterns: `mct_kernel::RuntimeKind::JvmChild` not covered
    --> crates/mct-daemon/src/main.rs:2085:25
     |
2085 |     let runtime = match candidate.runtime_kind {
     |                         ^^^^^^^^^^^^^^^^^^^^^^ pattern `mct_kernel::RuntimeKind::JvmChild` not covered
     |
note: `mct_kernel::RuntimeKind` defined here
    --> crates/mct-kernel/src/call/mod.rs:212:1
     |
 212 | pub enum RuntimeKind {
     | ^^^^^^^^^^^^^^^^^^^^
...
 216 |     JvmChild,
     |     -------- not covered
     = note: the matched value is of type `mct_kernel::RuntimeKind`
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern or an explicit pattern as shown
     |
2089 ~         RuntimeKind::Internal => 3,
2090 ~         mct_kernel::RuntimeKind::JvmChild => todo!(),
     |

Some errors have detailed explanations: E0004, E0308.
For more information about an error, try `rustc --explain E0004`.
error: could not compile `mct-daemon` (bin "mct-daemon") due to 2 previous errors
warning: build failed, waiting for other jobs to finish...
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 2 previous errors
```

### 2026-07-06 — D3 rustfmt check reported formatting diffs

Command:

```bash
cargo fmt --check
```

Failure output:

```text
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon/src/main.rs:1860:
     let config = MctDaemonConfigStore::new(&paths.config_path).load()?;
     let load_report = load_children_from_dir(MctChildLoadOptions::new(paths.children_dir.clone()));
     let scope = resident_child_scope(&config);
-    let projection = config.authority_projection_for_loaded_children(load_report.children.iter(), scope);
+    let projection =
+        config.authority_projection_for_loaded_children(load_report.children.iter(), scope);
     let mut plans = Vec::new();
 
     for child in load_report
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon/src/main.rs:2169:
         let report = resident_route_revision_denial_report(
             &call,
             execution.authorized_route.route(),
-            execution.authorized_route.revalidation_decision_id().clone(),
+            execution
+                .authorized_route
+                .revalidation_decision_id()
+                .clone(),
             CandidateEliminationReason::PolicyRevisionStale,
             &current_revisions,
             execution.authorized_route.policy_revision(),
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon/src/main.rs:2182:
         let report = resident_route_revision_denial_report(
             &call,
             execution.authorized_route.route(),
-            execution.authorized_route.revalidation_decision_id().clone(),
+            execution
+                .authorized_route
+                .revalidation_decision_id()
+                .clone(),
             CandidateEliminationReason::GrantsRevisionStale,
             &current_revisions,
             execution.authorized_route.policy_revision(),
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon/src/main.rs:2192:
         return Ok(report);
     }
 
-    let route_decision_id = execution.authorized_route.revalidation_decision_id().clone();
+    let route_decision_id = execution
+        .authorized_route
+        .revalidation_decision_id()
+        .clone();
     let route_taken = execution.route_taken.clone();
     let child_invocation = execution.authorized_route.into_child_invocation();
     let child_execution = ResidentChildExecution {
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon/src/main.rs:2389:
     minted_grants_revision: u64,
 ) -> ResidentExecutionReport {
     let observation = MctObservation {
-        observation_id: ObservationId::new(format!(
-            "obs-route-revision-denied:{}",
-            call.call_id
-        ))
-        .expect("string ID literal/generated value must be non-empty"),
+        observation_id: ObservationId::new(format!("obs-route-revision-denied:{}", call.call_id))
+            .expect("string ID literal/generated value must be non-empty"),
         observed_at: current_timestamp(),
         kind: ObservationKind::NoRouteRecorded,
         source_plane: SourcePlane::Adapter,
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon/src/main.rs:2529:
             }
             .with_route(route_decision_id, route_taken)
         }
-        ResultOutcome::TimedOut => MctIrohCallHandlerResult::timed_out()
-            .with_route(route_decision_id, route_taken),
-        ResultOutcome::Denied => MctIrohCallHandlerResult::denied().with_route(route_decision_id, None),
+        ResultOutcome::TimedOut => {
+            MctIrohCallHandlerResult::timed_out().with_route(route_decision_id, route_taken)
+        }
+        ResultOutcome::Denied => {
+            MctIrohCallHandlerResult::denied().with_route(route_decision_id, None)
+        }
         ResultOutcome::Failed => MctIrohCallHandlerResult::failed(result.requester_message.clone())
             .with_route(route_decision_id, route_taken),
-        ResultOutcome::Cancelled => MctIrohCallHandlerResult::failed(result.requester_message.clone())
-            .with_route(route_decision_id, None),
+        ResultOutcome::Cancelled => {
+            MctIrohCallHandlerResult::failed(result.requester_message.clone())
+                .with_route(route_decision_id, None)
+        }
     }
 }
```

### 2026-07-06 — D4 failing test before serve reply route projection

Command:

```bash
cargo test -p mct-daemon resident_mother_serves_peer_control_and_shutdown -- --nocapture
```

Failure output:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.68s
     Running unittests src/lib.rs (target/debug/deps/mct_daemon-5682d471ecfb696f)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 95 filtered out; finished in 0.00s

     Running unittests src/main.rs (target/debug/deps/mct_daemon-701d058281c133f0)

running 1 test
mct resident mother endpoint_id=f269c164facd78a8e7c71a7ac893b339cbdfb6afa255ac9dea3c1bfce2814497
ticket={  "endpoint_id": "f269c164facd78a8e7c71a7ac893b339cbdfb6afa255ac9dea3c1bfce2814497",  "direct_addresses": [    "10.10.10.182:56477",    "10.10.10.209:56477",    "100.114.124.29:56477"  ],  "relay_urls": []}
mct daemon serving control uds on /var/folders/6h/329275913d1d3k1lfvvvryp40000gn/T/.tmpqhZKn0/control.sock
mct resident mother children loaded=1 failed=0 bindings=1 max_connections=8

thread 'tests::resident_mother_serves_peer_control_and_shutdown' (6629263) panicked at crates/mct-daemon/src/main.rs:4338:9:
assertion failed: reply.route_taken.is_some()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test tests::resident_mother_serves_peer_control_and_shutdown ... FAILED

failures:

failures:
    tests::resident_mother_serves_peer_control_and_shutdown

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 1.90s

error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

### 2026-07-06 — D4 rustfmt check reported formatting diffs

Command:

```bash
cargo fmt --check
```

Failure output:

```text
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh/src/serve.rs:883:
                                     .as_ref()
                                     .map(|handled| handled.result_payload.clone())
                                     .unwrap_or(MctCallPayloadHandle::Empty),
-                                handled.as_ref().and_then(|handled| handled.route_taken.clone()),
+                                handled
+                                    .as_ref()
+                                    .and_then(|handled| handled.route_taken.clone()),
                                 state_guard.next_observation_id("call-reply"),
                             );
                             drop(state_guard);
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-iroh/src/serve.rs:1159:
                             .as_ref()
                             .map(|handled| handled.result_payload.clone())
                             .unwrap_or(MctCallPayloadHandle::Empty),
-                        handled.as_ref().and_then(|handled| handled.route_taken.clone()),
+                        handled
+                            .as_ref()
+                            .and_then(|handled| handled.route_taken.clone()),
                         state.next_observation_id("call-reply"),
                     );
                     let response_bytes = encode_call_reply_envelope(
```

### 2026-07-06 — D5 targeted test invocation used multiple cargo filters

Command:

```bash
cargo test -p mct-daemon resident_route_ resident_no_route_records_specific_elimination resident_authorized_unavailable_is_temporal_no_route resident_route_revision_guard_denies_before_effect cancelled_result_and_reply_hide_route_while_ledger_keeps_selection -- --nocapture
```

Failure output:

```text
error: unexpected argument 'resident_no_route_records_specific_elimination' found

Usage: cargo test [OPTIONS] [TESTNAME] [-- [ARGS]...]

For more information, try '--help'.
```

### 2026-07-06 — D5 proof tests exposed ledger text and stale-run issues

Command:

```bash
cargo test -p mct-daemon --bin mct-daemon -- --nocapture
```

Failure output:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.82s
     Running unittests src/main.rs (target/debug/deps/mct_daemon-701d058281c133f0)

running 16 tests
test tests::cancelled_result_and_reply_hide_route_while_ledger_keeps_selection ... ok
test tests::authorize_cli_toy_denies_expired_grant_against_current_time ... ok
test tests::control_snapshot_unopenable_state_projects_error_response ... ok
test tests::resident_status_source_reflects_closed_endpoint ... ok
test tests::resident_authorized_unavailable_is_temporal_no_route ... ok
test tests::resident_local_blob_absent_fails_closed_before_delivery ... ok

thread 'tests::resident_no_route_records_specific_elimination' (6644025) panicked at crates/mct-daemon/src/main.rs:4977:9:
assertion failed: ledger_text.contains("CandidateEliminated")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test tests::resident_no_route_records_specific_elimination ... FAILED
test tests::resident_local_blob_tamper_fails_closed_via_digest_mismatch ... ok

thread 'tests::resident_route_revision_guard_denies_before_effect' (6644028) panicked at crates/mct-daemon/src/main.rs:5056:10:
called `Result::unwrap()` on an `Err` value: FOREIGN KEY constraint failed

Caused by:
    Error code 787: Foreign key constraint failed
test tests::resident_route_revision_guard_denies_before_effect ... FAILED
test tests::resident_wit_rejects_non_json_payload_before_execution ... ok
test tests::resident_execution_runs_wit_child_and_records_trace ... ok
test tests::resident_process_payload_delivery_returns_digest_and_keeps_ledger_byte_free ... ok
test tests::resident_local_blob_payload_delivery_returns_digest_and_keeps_ledger_byte_free ... ok

thread 'tests::resident_route_optimization_cannot_grant_authority' (6644027) panicked at crates/mct-daemon/src/main.rs:4941:9:
assertion failed: ledger_text.contains("CandidateEliminated")
test tests::resident_route_optimization_cannot_grant_authority ... FAILED
mct resident mother endpoint_id=3ee3f6c4fbe546ce942ea4f81c590f322e248ec9d98a4ca84b9bd7bd2e21e071
mct resident mother endpoint_id=176b5289df998d807aa2e099db779a800d748725edcce5408ba8d7e9c7f47e6f
ticket={  "endpoint_id": "3ee3f6c4fbe546ce942ea4f81c590f322e248ec9d98a4ca84b9bd7bd2e21e071",  "direct_addresses": [    "10.10.10.182:53922",    "10.10.10.209:53922",    "100.114.124.29:53922"  ],  "relay_urls": []}
ticket={  "endpoint_id": "176b5289df998d807aa2e099db779a800d748725edcce5408ba8d7e9c7f47e6f",  "direct_addresses": [    "10.10.10.182:63729",    "10.10.10.209:63729",    "100.114.124.29:63729"  ],  "relay_urls": []}
mct resident mother children loaded=1 failed=0 bindings=1 max_connections=8
mct resident mother children loaded=1 failed=0 bindings=1 max_connections=8
mct daemon serving control uds on /var/folders/6h/329275913d1d3k1lfvvvryp40000gn/T/.tmp9ZRplI/control.sock
mct daemon serving control uds on /var/folders/6h/329275913d1d3k1lfvvvryp40000gn/T/.tmprWS68k/control.sock
test tests::resident_mother_payload_roundtrip_verifies_result_digest ... ok
test tests::resident_mother_serves_peer_control_and_shutdown ... ok

failures:

failures:
    tests::resident_no_route_records_specific_elimination
    tests::resident_route_optimization_cannot_grant_authority
    tests::resident_route_revision_guard_denies_before_effect

test result: FAILED. 13 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.05s

error: test failed, to rerun pass `-p mct-daemon --bin mct-daemon`
```

### 2026-07-06 — D5 rustfmt check reported formatting diff

Command:

```bash
cargo fmt --check
```

Failure output:

```text
Diff in /Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon/src/main.rs:4994:
                 .expect("string ID literal/generated value must be non-empty"),
         );
 
-        let outcome = authorize_resident_child_from_loaded(&config, loaded.children, &call).unwrap();
+        let outcome =
+            authorize_resident_child_from_loaded(&config, loaded.children, &call).unwrap();
         let ResidentAuthorizationOutcome::Denied { observations, .. } = outcome else {
             panic!("loading child should produce temporal no-route")
         };
```

### 2026-07-06 — D5 outcome-matrix test used a non-existent RouteTaken field

Command:

```bash
cargo test -p mct-daemon --bin mct-daemon route_taken_projection_follows_outcome_matrix -- --nocapture
```

Failure output:

```text
   Compiling mct-daemon v0.1.0 (/Users/nicabar/Projects/Patina/patina-mct/crates/mct-daemon)
error[E0560]: struct `mct_kernel::RouteTaken` has no field named `network_path`
    --> crates/mct-daemon/src/main.rs:5075:13
     |
5075 |             network_path: NetworkPathClass::Local,
     |             ^^^^^^^^^^^^ `mct_kernel::RouteTaken` does not have this field
     |
     = note: all struct fields are already assigned

For more information about this error, try `rustc --explain E0560`.
error: could not compile `mct-daemon` (bin "mct-daemon" test) due to 1 previous error
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

## Close-out

Route wiring closed on 2026-07-06.

### Commits D0-D5

- `336bd0b docs: start route wiring phase` — D0 task file and phase prompt captured.
- `da56c32 docs: specify route wiring` — D1 route wiring SPEC and ROADMAP follow-on note.
- `39fa3f2 docs: amend route wiring spec at operator gate` — D1.1 operator-gate amendments.
- `6216f81 feat(kernel): expose route wiring facts` — D2 kernel reply projection, candidate classification, observation helpers, and validation.
- `fc9fc85 feat(daemon): route resident calls` — D3 local resident calls through route decision, revalidation, by-value route authority consumption, and effect-boundary stale guards.
- `a78107a feat(iroh): return route projections` — D4 remote `mct/call/0` reply path with route decision and `route_taken` projection.
- `a5fc496 test(daemon): prove route wiring behavior` — D5 end-to-end proofs and PHASE3 T5 discharge note.
- `ab6e187 test(daemon): cover route outcome projection` — D5 outcome-matrix proof for `route_taken`.

### Flake log status

The flake log above is complete through close-out. All recorded failures were deterministic failing-first, compile, formatting, or invalid-invocation issues; each is fixed. No unresolved flakes remain.

### D5 transcript

Route proof suite rerun at close-out:

```bash
cargo test -p mct-daemon --bin mct-daemon -- --nocapture
```

```text
running 17 tests
test tests::cancelled_result_and_reply_hide_route_while_ledger_keeps_selection ... ok
test tests::authorize_cli_toy_denies_expired_grant_against_current_time ... ok
test tests::control_snapshot_unopenable_state_projects_error_response ... ok
test tests::resident_status_source_reflects_closed_endpoint ... ok
test tests::resident_authorized_unavailable_is_temporal_no_route ... ok
test tests::route_taken_projection_follows_outcome_matrix ... ok
test tests::resident_local_blob_absent_fails_closed_before_delivery ... ok
test tests::resident_no_route_records_specific_elimination ... ok
test tests::resident_local_blob_tamper_fails_closed_via_digest_mismatch ... ok
test tests::resident_route_revision_guard_denies_before_effect ... ok
test tests::resident_wit_rejects_non_json_payload_before_execution ... ok
test tests::resident_execution_runs_wit_child_and_records_trace ... ok
test tests::resident_process_payload_delivery_returns_digest_and_keeps_ledger_byte_free ... ok
test tests::resident_route_optimization_cannot_grant_authority ... ok
test tests::resident_local_blob_payload_delivery_returns_digest_and_keeps_ledger_byte_free ... ok
test tests::resident_mother_payload_roundtrip_verifies_result_digest ... ok
test tests::resident_mother_serves_peer_control_and_shutdown ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.51s
```

Six route-specific D5 proofs covered:

1. `resident_route_optimization_cannot_grant_authority`: a ranking-preferred WIT candidate lacking approval is eliminated with `ChildNotApproved`; the less-preferred admissible process candidate executes and records `route_selected`.
2. `resident_route_revision_guard_denies_before_effect`: a policy revision mismatch between mint and execution returns `ResultOutcome::Denied`, records `PolicyRevisionStale` plus minted/current revisions, and never creates the child execution marker.
3. `resident_no_route_records_specific_elimination`: no approved candidate fails closed with caller-safe `not authorized`, no `route_taken`, `candidate_eliminated`/`ChildNotApproved`, and `no_route_recorded` ledger evidence.
4. `resident_authorized_unavailable_is_temporal_no_route`: an approved but `Loading` child is denied as `CapabilityUnavailable` with `denial_class:temporal`.
5. `route_taken_projection_follows_outcome_matrix` and `cancelled_result_and_reply_hide_route_while_ledger_keeps_selection`: `route_taken` is present for success/failed/timed_out, absent for denied/cancelled, and mid-execution cancellation keeps route evidence reconstructible through `RouteSelected` while result/reply hide `route_taken`.
6. `resident_execution_runs_wit_child_and_records_trace`: a successful WIT call ledger trace contains `RouteRevalidated` and `RuntimeExecutionCompleted`, proving the executed path is reconstructible.

### ROADMAP follow-on

ROADMAP item 3 is complete for local candidates only and records PHASE3 T5 as discharged. Remote route candidates and cross-Mother call forwarding are confirmed as follow-on work under ROADMAP item 6. No additional ROADMAP items were discovered during close-out.

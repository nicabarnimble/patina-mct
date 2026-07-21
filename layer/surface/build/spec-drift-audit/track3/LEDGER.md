# Contract obligation ledger

Status date: 2026-07-12; W2 extension 2026-07-14; Daily-Driver Slice 2 extension 2026-07-15; artifact-acquisition extension 2026-07-16; trigger-runtime Part A extension 2026-07-21

Scope: complete named-invariant coverage for `mct-product-map.allium` and `mct-peer-ontology.allium`, plus bulk attribution of tool-derived structural obligations. The 2026-07-12 priority and full-inventory evidence is retained in place; the 2026-07-14 local-application-ingress invariants and W2-A remediation obligations extend it below.

## Status vocabulary

- **COVERED** — named existing test directly exercises the obligation.
- **GAP** — priority behaviour exists but no named test proves the complete obligation.
- **LAW-LEADS-CODE** — tended law is known to be stricter than the current implementation; Track 3 must test and resolve or stop.
- **DEFERRED** — the law explicitly assigns the behaviour to a named future scope; no current execution path exists to test.

Module names distinguish library tests from binary-local tests: `mct_daemon_bin` denotes tests under `crates/mct-daemon/src/daemon/`.

## Tool-derived structural obligations

Allium 3.5.0 emits structural obligations only for the product map: 179 total (`entity_fields` 56, `entity_optional` 38, `surface_actor` 27, `surface_exposure` 27, `value_equality` 29, `when_presence` 2). The companion emits zero plan obligations and no model entities/value types because it consists of prose contracts and open questions; its invariants are manually derived below.

| Plan obligation(s) | Status | Evidence |
|---|---|---|
| `entity-fields.MctPeerBinding`, `entity-fields.MctPeerBindingScope`, `value-equality.MctPeerBindingScope` | COVERED | `mct_kernel::peer::tests::binding_without_expiry_fails_closed`; Rust and operator/config projections require `expires_at`. |
| `entity-optional.MctPeerBinding.superseded_by_observation_id`, `surface-actor.MctPeerBindingProjection`, `surface-exposure.MctPeerBindingProjection` | COVERED | `mct_kernel::peer::tests::active_binding_admits_intersection_of_requested_policy_and_binding_alpns`; `mct_kernel::observation::tests::revoked_and_expired_bindings_become_observations` |
| `value-equality.MctPeerBindingPresentation`, `entity-fields.MctPeerBindingPresentation` | COVERED | `mct_iroh::identity::tests::peer_binding_signature_ref_roundtrips_and_fails_on_tamper`; `mct_kernel::call::tests::call_envelope_roundtrip_preserves_semantic_call_across_edges` |
| `value-equality.MctHelloCallableSurface`, `entity-fields.MctHelloCallableSurface`, `value-equality.MctHelloCapabilityView`, `entity-fields.MctHelloCapabilityView` | COVERED | `mct_kernel::peer::tests::hello_capability_view_carries_callable_surfaces`; `mct_daemon::federation::tests::honest_local_execution_offer_excludes_approved_assigned_non_ready_child` |
| `entity-fields.MctHelloRequest`, hello request optional fields, request projection actor/exposure | COVERED | `mct_kernel::call::tests::call_envelope_roundtrip_preserves_semantic_call_across_edges`; `mct_iroh::tests::local_iroh_completes_mct_hello_then_call` |
| `entity-fields.MctHelloResponse`, response optional fields, response projection actor/exposure | COVERED | `mct_iroh::tests::local_iroh_completes_mct_hello_then_call`; `mct_daemon_bin::resident::publication::tests::hello_response_capability_view_refreshes_surfaces_on_caller` |
| `value-equality.MctCallPayloadHandle`, `entity-fields.MctCallPayloadHandle` | COVERED | `mct_kernel::call::tests::payload_integrity_decisions_cover_request_mismatch_classes`; `mct_kernel::call::tests::payload_integrity_decisions_cover_local_content_addressed_blob` |
| `entity-fields.MctCallProtocolRequest`, `entity-optional.MctCallProtocolRequest.idempotency_key`, request projection actor/exposure | COVERED | `mct_kernel::call::tests::call_protocol_json_edge_roundtrips_and_rejects_malformed`; `mct_daemon_bin::resident::idempotency::tests::resident_idempotency_replays_scopes_refuses_and_expires_without_payload_leakage` |
| `entity-fields.MctCallProtocolEvaluation` and its optional references, evaluation projection actor/exposure | COVERED | `mct_kernel::call::tests::admitted_hello_allows_call_for_routing`; `mct_iroh::tests::call_payload_integrity_failures_are_malformed_before_authority` |
| `entity-fields.MctCallProtocolReply`, optional result/route, reply projection actor/exposure | COVERED | `mct_kernel::call::tests::call_protocol_reply_roundtrips_route_taken_wire_field`; `mct_kernel::call::tests::reply_validation_enforces_route_taken_presence_rule` |
| `entity-fields.RouteDecision`, optional selected/initial fields, `when-presence.RouteDecision.initial_decision_id`, inspection actor/exposure | COVERED | `mct_kernel::route::tests::route_decision_records_selected_candidate_and_authority_evidence`; `mct_kernel::route::tests::route_revalidation_allows_matching_execution_authority` |
| `entity-fields.MctObservation` and optional links, observation projection actor/exposure | COVERED | `mct_kernel::observation::tests::observation_kind_uses_snake_case_wire_names`; `mct_observation::tests::append_and_read_roundtrip` |
| `entity-fields.MctObservationObligation`, contract-matrix actor/exposure | COVERED | `mct_kernel::observation::tests::kernel_denial_evaluations_become_observations`; `mct_kernel::observation::tests::adapter_diagnostic_observation_covers_failure_kinds` |
| `entity-fields.MctObservationLedgerEntry`, optional previous hash, ledger surface actor/exposure | COVERED | `mct_observation::tests::append_and_read_roundtrip`; `mct_observation::tests::reopens_existing_hash_chain`; `mct_observation::tests::queries_by_trace_and_call` |

## Product-map priority contracts

### Request-scoped idempotency

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `MctCallProtocol.IdempotencyIsRequestScoped` | COVERED | `mct_kernel::call::tests::idempotency_decision_replays_matches_and_refuses_other_cases`; `mct_daemon::state::tests::idempotency_store_scopes_reserves_replays_expires_and_survives_reopen` |
| `MatchingCompletedRetryReplaysRecordedReply` | COVERED | `mct_daemon_bin::resident::idempotency::tests::resident_idempotency_replays_scopes_refuses_and_expires_without_payload_leakage`; `mct_daemon_bin::resident::idempotency::tests::cancelled_idempotent_reply_replays_cancelled_with_durable_observation`; `mct_daemon_bin::ingress::tests::standalone_serve_process_persists_hello_and_call_lifecycle` |
| `IdempotencyFingerprintMustMatch` | COVERED | `mct_kernel::call::tests::idempotency_decision_replays_matches_and_refuses_other_cases`; `mct_daemon::state::tests::idempotency_store_scopes_reserves_replays_expires_and_survives_reopen` |
| `IdempotencyBoundsRefuseRatherThanEvict` | COVERED | `mct_kernel::call::tests::idempotency_decision_replays_matches_and_refuses_other_cases`; `mct_daemon::state::tests::idempotency_store_scopes_reserves_replays_expires_and_survives_reopen` |
| `CurrentIdempotencyEntryNeverSilentlyReexecutes` | COVERED | `mct_daemon_bin::resident::idempotency::tests::resident_idempotency_replays_scopes_refuses_and_expires_without_payload_leakage`; `mct_daemon_bin::resident::idempotency::tests::in_flight_idempotency_duplicate_refuses_without_second_execution` |
| `InFlightDuplicateIsRefused` | COVERED | `mct_daemon_bin::resident::idempotency::tests::in_flight_idempotency_duplicate_refuses_without_second_execution` |
| `IdempotencyStateSurvivesRestart` | COVERED | `mct_daemon::state::tests::idempotency_store_scopes_reserves_replays_expires_and_survives_reopen`; `mct_daemon_bin::resident::serving::tests::resident_call_uds_executes_approved_child_and_projects_control_state` proves the same-key UDS reply and inline result survive resident stop/restart without a second child effect. |
| `CurrentAuthorityPrecedesReplay` — revocation | COVERED | `mct_daemon_bin::resident::serving::tests::resident_mother_payload_roundtrip_verifies_result_digest` |
| `CurrentAuthorityPrecedesReplay` — expiry and narrowed Vision | COVERED | `mct_daemon_bin::resident::serving::tests::resident_mother_payload_roundtrip_verifies_result_digest` records one keyed success, then proves identical retries after expiry and Vision narrowing are denied without cached payload. |
| `CurrentAuthorityPrecedesReplay` — narrowed ALPN | DEFERRED | Persisted peer bindings currently expose the fixed `mct/hello/0` + `mct/call/0` protocol scope and have no operator ALPN-narrowing surface. A replay test requires the future configurable binding-scope model; inventing that authority surface is outside propagation. Current call-time ALPN revalidation remains covered by `mct_iroh::tests::call_rechecks_narrowed_alpn_scope_after_hello`. |
| `CrossMotherReplayRequiresFederationContract` | COVERED | `mct_daemon::state::tests::idempotency_store_scopes_reserves_replays_expires_and_survives_reopen` proves caller/store isolation; `mct_daemon_bin::resident::forwarding::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` uses separate Mother stores. |

### Local application ingress and W2-A remediation

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `MctLocalApplicationIngress.AuthenticatedLocalPrincipalDefinesCaller` | COVERED | `mct_daemon_bin::resident::local_ingress::tests::resident_call_uds_authenticates_peer_before_submission`; `mct_daemon_bin::resident::local_ingress::tests::resident_call_uds_dispatch_authenticates_and_bounds_before_body_read` |
| `LocalAcknowledgementRequiresDurableFacts` | COVERED | `mct_daemon_bin::resident::local_ingress::tests::resident_call_uds_observes_decision_before_response`; `mct_daemon_bin::resident::serving::tests::resident_call_uds_executes_approved_child_and_projects_control_state` checks durable construction before reading the terminal UDS response. |
| `AdapterOriginNamesIngressLineage` | COVERED | `mct_daemon_bin::resident::pipeline::tests::jvm_bridge_json_call_enters_resident_route_path` asserts the translated origin; `mct_daemon_bin::resident::serving::tests::resident_call_uds_executes_approved_child_and_projects_control_state` proves the production local bridge enters that one resident path. |
| `SharedPrincipalSharesIdempotencyScope` | COVERED | `mct_daemon_bin::resident::local_ingress::tests::resident_call_uds_idempotency_is_authenticated_caller_scoped` proves same-principal replay and same-key/different-fingerprint refusal. |
| `CallsRemainOutsideMutationSequencer` | COVERED | `mct_daemon_bin::resident::serving::tests::resident_call_uds_executes_approved_child_and_projects_control_state` exercises call and read requests through the concurrent resident UDS dispatcher; `mct_daemon_bin::supervisor_lifecycle::tests::supervised_slate_artifact_acquisition_executes_and_revokes_end_to_end` interleaves protected artifact/approval/grant mutations with concurrent call ingress. |
| W2-A/A9 dispatch authentication and streaming call-body bound | COVERED | `mct_daemon_bin::resident::local_ingress::tests::resident_call_uds_dispatch_authenticates_and_bounds_before_body_read` sends headers only over a real `UnixStream` and proves UID refusal and oversized-frame refusal both occur before body consumption. |
| W2-A/A11 forwarded cancellation projection | COVERED | `mct_daemon_bin::resident::forwarding::tests::two_mother_forwarding_preserves_cancelled_reply` proves a two-Mother executor cancellation remains `cancelled`, keeps no caller route, and is observed as cancelled at the origin. |
| W2-A/A10 whole-path replay/reopen proof | COVERED | `mct_daemon_bin::resident::serving::tests::resident_call_uds_executes_approved_child_and_projects_control_state` proves same-key UDS replay, one child effect, state/ledger reopen, and replay with result bytes after resident restart. |
| W2-A/A12 current resident child-count projection | COVERED | `mct_daemon_bin::resident::serving::tests::resident_status_reflects_live_child_mutations` proves catalog package insertion and child revocation immediately change loaded and approved status counts. |

### Operational self-observation and macOS supervisor lifecycle

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `MctOperationalSelfObservation.LifecycleActionAttemptsAreObserved` | COVERED | `mct_daemon_bin::supervisor_lifecycle::tests::supervisor_lifecycle_install_start_stop_unclean_reconcile_uninstall_preserves_evidence`; `supervisor_conflicts_refuse_before_launchd_or_endpoint_effects`; `launchd_adapter_refuses_missing_gui_domain_without_fallback`; `supervised_start_rejects_unblessed_binary_swap_with_replace_guidance` cover successful, failed, denied, no-op, and reconciled attempts. |
| `ObservationStoreMutationIsObserved` | DEFERRED | Ledger retention, archival, export, deletion, and compaction remain reserved future actions; Slice 2 adds no such path and uninstall preserves the complete ledger. |
| `StatusAndReadinessAreProjections` | COVERED | `mct_daemon_bin::resident::serving::tests::resident_status_source_reflects_closed_endpoint`; `mct_daemon_bin::cli_admin::tests::status_reports_real_resident_snapshot`; status reads append no lifecycle fact. |
| `UncleanTerminationIsReconciledAfterward` | COVERED | `mct_daemon_bin::supervisor_lifecycle::tests::supervisor_lifecycle_install_start_stop_unclean_reconcile_uninstall_preserves_evidence` aborts an instance without shutdown completion and proves the next start records reconciliation before readiness. |
| `MinimalObserverSubstrateMayPrecedeFirstAppend` | COVERED | `mct_daemon_bin::supervisor_lifecycle::tests::supervisor_install_bootstrap_is_observed_before_every_remaining_effect` proves record, plist, config, identity, and state follow the first install batch; only the owner-private root/ledger/writer substrate precedes it. |
| `FirstAppendPrecedesRemainingBootstrapEffects` | COVERED | `mct_daemon_bin::supervisor_lifecycle::tests::supervisor_install_bootstrap_is_observed_before_every_remaining_effect` reopens the ledger and orders governing install and identity/effect/completion facts. |
| `BootstrapInitiatorIsAuthenticatedLocalPrincipal` | COVERED | `supervisor_install_bootstrap_is_observed_before_every_remaining_effect` verifies OS-UID provenance; `mct_daemon::control::tests::uds_authenticated_mutation_handler_receives_peer_credentials` proves authenticated mutation transport credentials reach lifecycle ingress. |
| `ExclusiveWriterAdmitsOneBootstrap` | COVERED | `mct_daemon_bin::supervisor_lifecycle::tests::supervisor_conflicts_refuse_before_launchd_or_endpoint_effects` holds the first writer, proves the concurrent loser performs no record/plist effect, and reopens the ledger to find its durable contention refusal. |
| `ExternalReceiptIsInputNotEvidence` | COVERED | `supervisor_install_bootstrap_is_observed_before_every_remaining_effect` proves the canonical ledger-backed record/observation chain; the implemented record and plist validators accept no installer receipt or alternative evidence channel. |
| `BootstrapAppendFailureSuppressesSuccess` | COVERED | `mct_daemon_bin::resident::serving::tests::bootstrap_identity_append_failure_leaves_no_identity_effect` proves failed first bootstrap append suppresses identity/config effects; `supervisor_install_bootstrap_is_observed_before_every_remaining_effect` proves supervisor publication is downstream of that same mandatory append barrier. |
| `WriterLossFencesMother` | COVERED | `mct_daemon_bin::supervisor_lifecycle::tests::resident_writer_loss_fences_lifecycle_and_all_other_protected_effects`; `mct_daemon_bin::resident::serving::tests::resident_status_source_reflects_closed_endpoint` proves fenced status is not ready. |
| `InProgressEffectsAreNotAcknowledgedOrCached` | COVERED | `mct_daemon_bin::resident::idempotency::tests::fenced_writer_does_not_acknowledge_or_cache_an_in_progress_effect_outcome` proves one effect may finish but returns ledger-unavailable, remains in-progress, and neither replays nor executes twice; resident run completion now persists only after terminal observations append. |
| `TerminationForSafetyMayProceed` | COVERED | The primary lifecycle integration proves unmatched termination reconciliation; supervised shutdown always closes endpoint/control even when shutdown append fails, while direct stop refuses to report clean completion unless the matching shutdown fact exists. |
| `ObserverRestorationIsExplicitAndMinimal` | DEFERRED | Slice 2 adds no observer-recovery command or implicit repair path. Lifecycle operations remain fenced; explicit restoration is owned by a future recovery SPEC/operator gate. |
| `RecoveryDigestBindsPreservedEvidence` | DEFERRED | Future observer-recovery slice; no recovery continuation is emitted by Slice 2. |
| `RecoveryPrecedesReadiness` | DEFERRED | Future observer-recovery slice; ordinary supervised startup fails closed when the canonical writer cannot open. |
| `FailedRecoveryRemainsFenced` | DEFERRED | Future observer-recovery slice; no recovery attempt exists in this command surface. |
| `NoParallelRecoveryEvidenceChain` | DEFERRED | Slice 2 creates no receipt, side journal, or alternate evidence store; future recovery remains separately gated. |
| `OperationalRolesAreDistinctProjections` | COVERED | Primary integration and lifecycle observation tests project installation/node subject, OS UID or exact supervisor-record initiator, and installer/resident/launchd adapter executor on shared traces. |
| `SubjectNamesLifecycleTarget` | COVERED | `supervisor_install_bootstrap_is_observed_before_every_remaining_effect` verifies pending installation before identity; the primary integration verifies current node/record subjects after identity. |
| `InitiatorNamesCurrentCausalAuthority` | COVERED | Direct lifecycle facts use authenticated OS peer/UID; automatic boot facts reference the exact current record id, revision, digest, and governing observation. |
| `SupervisorInitiationUsesInstalledRecordProvenance` | COVERED | `supervisor_lifecycle_install_start_stop_unclean_reconcile_uninstall_preserves_evidence` asserts exact record/revision/install observation provenance and no additional boot-time `OperatorActionRecorded`. |
| `ExecutorNamesEffectPerformer` | COVERED | Primary integration reopens installer/resident/launchd `AdapterEffectStarted/Completed` facts; GUI-domain and writer-failure tests cover `AdapterEffectFailed`, all separate from operator and lifecycle facts. |
| `LifecycleActionsAreNotCalls` | COVERED | `mct_daemon::control::tests::uds_authenticated_mutation_handler_receives_peer_credentials` and primary stop/start prove owner-authenticated `POST /lifecycle/fact`; no `MctCall`, call id, target, route, or child authority is constructed. |
| `ExistingObservationKindsComposeOperationalFacts` | COVERED | Supervisor tests assert only `OperatorActionRecorded`, `LifecycleTransitionRecorded`, and existing adapter-effect kinds; `mct-kernel` gains no observation variant. |

### Payload integrity and local CAS

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `MctCallProtocol.PayloadBytesAreBoundedByNamedConstants` | COVERED | `mct_iroh::tests::call_frame_budget_refuses_oversized_request`; `mct_iroh::tests::call_payload_caps_fail_closed`; `mct_iroh::tests::caller_rejects_reply_digest_mismatch_and_oversized_result`; `mct_daemon::blob_store::tests::ingest_rejects_oversized_input_before_visibility` |
| `MctCallProtocol.PayloadIntegrityIsVerifiedAtIngress` | COVERED | `mct_iroh::tests::call_payload_integrity_failures_are_malformed_before_authority`; `mct_daemon_bin::resident::serving::tests::resident_mother_payload_roundtrip_verifies_result_digest` |
| `MctPayloadIntegrityAndLocalCas.ExactSizeAndDigestBeforeUse` | COVERED | `mct_kernel::call::tests::payload_integrity_decisions_cover_request_mismatch_classes`; `mct_kernel::call::tests::payload_integrity_decisions_cover_reply_result_mismatch_classes` |
| `StorageIsBoundedByNamedConstant` | COVERED | `mct_daemon::blob_store::tests::ingest_rejects_oversized_input_before_visibility` |
| `VerifyThenAtomicallyPublish` | COVERED | `mct_daemon::blob_store::tests::ingest_rejects_digest_mismatch_without_visible_blob`; `mct_daemon_bin::control::tests::resident_blob_ingest_observes_success_and_typed_rejections_without_bytes` |
| `FailedVerificationIsNotVisible` | COVERED | `mct_daemon::blob_store::tests::ingest_rejects_digest_mismatch_without_visible_blob`; `mct_daemon_bin::control::tests::resident_blob_append_failure_leaves_no_visible_cas_object` |

### Routing, effect guard, and no-route

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `TwoPhaseRouting.AuthorityPrecedesOptimization` | COVERED | `mct_daemon_bin::resident::decision::tests::resident_route_optimization_cannot_grant_authority` |
| `PlannerRanksOnlyAdmissibleRoutes` | COVERED | `mct_kernel::route::tests::route_decision_records_selected_candidate_and_authority_evidence`; `mct_daemon_bin::resident::decision::tests::resident_route_optimization_cannot_grant_authority` |
| `OptimizationCannotGrantAuthority` | COVERED | `mct_daemon_bin::resident::decision::tests::resident_route_optimization_cannot_grant_authority` |
| `DenyReasonsArePolicyReasons` | COVERED | `mct_daemon_bin::resident::decision::tests::resident_no_route_records_specific_elimination`; `mct_kernel::route::tests::candidate_elimination_reasons_expose_denial_class` |
| `ExecutionRevalidatesAuthority` | COVERED | `mct_kernel::route::tests::route_revalidation_denies_stale_policy_before_execution`; `mct_daemon_bin::resident::forwarding::tests::two_mother_forwarding_denies_when_executor_revokes_binding_after_hello` |
| `EffectBoundaryRevisionGuardIsDistinct` | COVERED | `mct_daemon_bin::resident::execution::tests::resident_route_revision_guard_denies_before_effect` |
| `EffectBoundaryGuardCannotRepairStaleAuthority` | COVERED | `mct_daemon_bin::resident::execution::tests::resident_route_revision_guard_denies_before_effect` |
| `PeerEgressAndLocalChildEffectGuardsAreDistinct` | COVERED | `mct_daemon_bin::resident::forwarding::tests::two_mother_forwarding_denies_when_executor_revokes_binding_after_hello`; `mct_daemon_bin::resident::execution::tests::resident_route_revision_guard_denies_before_effect` |
| `NoRouteDecision.DenyByDefault` | COVERED | `mct_kernel::route::tests::no_route_decision_denies_by_default_without_route_taken` |
| `RetryRequiresPolicy`, `GrantRequestRequiresAuthority`, `GrantResponsesAreScopedAndBounded`, `NoSilentEscalation` | COVERED | `mct_kernel::route::tests::no_route_decision_denies_by_default_without_route_taken` proves the current passive default. Active retry/grant/escalation paths remain explicitly deferred under audit C3. |
| `SafeRequesterDisclosure` | COVERED | `mct_kernel::route::tests::candidate_elimination_reasons_expose_denial_class`; `mct_daemon_bin::resident::decision::tests::resident_no_route_records_specific_elimination` |

### Caller-safe route projection

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `MctCallProtocol.RouteTakenReplyPresenceFollowsExecution` | COVERED | `mct_kernel::call::tests::reply_validation_enforces_route_taken_presence_rule`; `mct_daemon_bin::resident::execution::tests::route_taken_projection_follows_outcome_matrix`; `mct_daemon_bin::resident::execution::tests::cancelled_result_and_reply_hide_route_while_ledger_keeps_selection`; `mct_iroh::tests::cancelled_call_preserves_wire_outcome_route_absence_and_buffered_observations` |
| `RouteTakenReplyDoesNotGrantPeerAuthority` | COVERED | `mct_kernel::call::tests::call_without_admitted_hello_is_denied`; `mct_kernel::call::tests::call_protocol_reply_roundtrips_route_taken_wire_field` |

### Signed binding proof and mandatory expiry

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `MctIrohPeerBindingAuthority.EveryPeerBindingIsTimeBounded` | COVERED | `mct_kernel::peer::tests::binding_without_expiry_fails_closed`; `mct_kernel::peer::tests::active_binding_past_expiry_is_denied`; `mct_iroh::tests::call_rechecks_binding_expiry_after_hello` |
| `BindingProofCoversCanonicalDirectionalRecord` | COVERED | `mct_iroh::identity::tests::peer_binding_signature_ref_roundtrips_and_fails_on_tamper` |
| `InvalidBindingProofFailsClosed` | COVERED | `mct_iroh::tests::concurrent_serve_requires_signed_peer_binding_when_configured`; `mct_daemon_bin::resident::serving::tests::resident_mother_rejects_unsigned_peer_binding`; `mct_daemon_bin::resident::candidates::tests::resident_remote_route_candidates_reject_unsigned_peer_binding` |
| `ProofDoesNotBecomeRelationshipOntology` | COVERED | `mct_daemon_bin::resident::candidates::tests::resident_remote_surface_candidate_becomes_admissible_when_all_checks_pass` requires proof plus independent authority/publication/reachability facts. |

### Observation durability and coverage

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `MctHelloProtocol.HelloObservationsBeforeEffects` | COVERED | `mct_daemon_bin::resident::observation::tests::resident_hello_observations_are_durable_before_responses`; `mct_iroh::tests::failed_hello_observation_prevents_response_and_remembered_admission` |
| `MctLocalFirstObservationLedger.AuthorityFactsAreDurableBeforeEffect` | COVERED | `mct_iroh::tests::denied_call_fact_is_recorded_before_reply`; `mct_daemon_bin::supervisor_lifecycle::tests::supervised_slate_artifact_acquisition_executes_and_revokes_end_to_end`; `mct_daemon_bin::control::tests::artifact_acquisition_append_failure_suppresses_filesystem_and_catalog_effects`; `mct_daemon_bin::control::tests::resident_append_failure_prevents_peer_config_effect` |
| `MctObservabilitySpine.AuthorityDecisionsAreObserved` | COVERED | `mct_kernel::observation::tests::kernel_denial_evaluations_become_observations`; `mct_daemon_bin::control::tests::live_uds_peer_mutations_are_durable_and_secret_free`; `mct_daemon_bin::control::tests::live_toy_grants_and_composition_state_are_observed_before_effects` |
| `MctObservabilitySpine.AdapterEffectsAreObserved` | COVERED | `mct_iroh::tests::iroh_adapter_observations_cover_endpoint_and_protocol_events`; `mct_daemon::process::tests::process_harness_timeout_returns_typed_result_and_observation`; `mct_daemon::toy::tests::toy_backend_failure_is_adapter_observation_not_kernel_denial` |

## Peer-ontology priority contracts

### Role and candidacy derivation

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `PeerRelationshipTaxonomy.RolesAreCurrentProjections` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` mutates current facts independently and immediately loses candidacy. |
| `PeerOperationalRoleDerivation.EligibleRouteCandidateDerivation` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` proves the positive candidate and independently removes local admission, reverse admission/proof, fresh publication, Vision agreement, call scope, and ticket reachability. |
| `SelectedExecutorDerivation` | COVERED | `mct_daemon_bin::resident::forwarding::tests::two_mother_forwarding_denies_when_executor_revokes_binding_after_hello` revalidates after selection and before egress. |
| `OperatorPointedSubmissionIsDistinct` | COVERED | `mct_daemon_bin::ingress::tests::operator_pointed_egress_is_durable_before_send` proves the operator decision is durable before the receiver observes `mct/call/0`; both manual CLI paths use the same recording boundary. |

### Terminal peer submission

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `SubmissionProposesReceiverAsLocalExecutor` | COVERED | `mct_daemon_bin::resident::forwarding::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` |
| `PeerArrivalsDoNotSourceAnotherPeer` | COVERED | `mct_daemon_bin::resident::decision::tests::forwarded_arrival_with_unavailable_local_candidate_is_terminal`; `mct_daemon_bin::resident::forwarding::tests::two_mother_mutual_publication_with_unready_children_terminates_single_hop` |
| `OriginDoesNotChangeAuthority` | COVERED | `mct_kernel::call::tests::only_local_call_origins_allow_remote_candidate_sourcing` together with terminal-arrival tests proves origin dispatches the submitted protocol meaning rather than bypassing authority. |
| `PublicationReferenceIsNotRequiredForTerminality` | COVERED | `mct_daemon_bin::resident::decision::tests::forwarded_arrival_with_unavailable_local_candidate_is_terminal` uses no publication reference in the arriving envelope. |

### Per-hop accountability

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `SubmissionIsTheImmediateCallersAct` | COVERED | `mct_daemon_bin::resident::forwarding::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` records `forwarded_from:mother-a` at the executor. |
| `ReceiverTrustsOnlyLocallyVerifiableAuthority` | COVERED | `mct_daemon_bin::resident::forwarding::tests::two_mother_forwarding_denies_when_executor_revokes_binding_after_hello` |
| `UpstreamIdentityRemainsAtItsVerifier` | COVERED | `mct_daemon_bin::resident::forwarding::tests::forwarded_envelope_clears_upstream_user_identity` proves the original verifier retains its user fact while the per-hop envelope carries only the submitting Mother and correlation IDs. |
| `BilateralAuditUsesCorrelationNotIdentityPropagation` | COVERED | `mct_daemon_bin::resident::forwarding::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` checks both ledgers and the shared route chain. |
| `ObservationReplicationIsTheSharingChannel` | DEFERRED | `mct/observe/0` and ObservationReplicationAuthorization are audit C4 future scope; current call envelopes grant no observation access. |
| `BrokeredIdentityBelongsToBrokeredSubmission` | DEFERRED | Brokered submission is explicitly future law and cannot be implemented by this terminal `mct/call/0` slice. |

### Bilateral executable routing

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `TwoExposuresRequireTwoSovereignConsents` | COVERED | `mct_daemon_bin::resident::candidates::tests::resident_remote_surface_candidate_becomes_admissible_when_all_checks_pass`; missing terms are consolidated by the candidacy GAP above. |
| `MutualAdmissionPreventsExternallyWritableRouting` | COVERED | `mct_daemon_bin::resident::candidates::tests::resident_remote_route_candidates_reject_unsigned_peer_binding`; `mct_daemon_bin::resident::candidates::tests::two_mother_unauthorized_operation_fails_closed` |
| `OneWayStatesRemainMeaningful` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` independently removes each directional admission. |
| `BilateralStateIsDerivedNotStored` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` derives candidacy from current directional records rather than a stored pair state. |
| `EitherDirectionEndsCandidacyImmediately` | COVERED | `mct_daemon_bin::resident::candidates::tests::two_mother_revoked_or_expired_binding_fails_closed`; `mct_daemon_bin::resident::forwarding::tests::two_mother_forwarding_denies_when_executor_revokes_binding_after_hello` |
| `ReachabilityIsNotAuthority` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` proves publication plus reachability cannot replace either consent and omission of the ticket removes candidacy. |

### Capability publication

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `CapabilityPublicationRelationship.HonestLocalExecutionOffer` | COVERED | `mct_daemon::federation::tests::honest_local_execution_offer_excludes_approved_assigned_non_ready_child` proves approval and assignment cannot publish a non-ready instance. |
| `AdvertisementNeverGrantsAuthority` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` retains fresh publication while independently removing each directional consent. |
| `OfferIsVisionScoped` | COVERED | `mct_daemon::federation::tests::federation_view_is_vision_scoped`; `mct_daemon_bin::resident::candidates::tests::two_mother_wrong_vision_fails_closed` |
| `OfferLapsesAtFreshnessBoundary` | COVERED | `mct_daemon_bin::resident::candidates::tests::capability_offer_lapses_at_freshness_boundary` proves candidacy is absent exactly at and after `stale_at`. |

## Slice 1 status summary (historical baseline)

| Status | Rows |
|---|---:|
| COVERED | 73 |
| GAP | 0 |
| LAW-LEADS-CODE | 0 |
| DEFERRED | 3 |

S2 resolved both mandatory-expiry rows through one contract change and the operator-pointed egress row through a shared before-effect recording boundary. Every S3 GAP is now covered except narrowed-ALPN replay, which is explicitly deferred until peer binding scope becomes configurable.

## Full invariant inventory extension

The 2026-07-12 slice extended the priority ledger in place to all 223 named contract invariants then present. The 2026-07-14 W2 extension adds the five `MctLocalApplicationIngress` invariants above, producing the current 228-invariant product-map/peer-ontology inventory. Statuses remain obligation-specific where slice 1 split one invariant into independently testable edges.

The historical pass read 236 load-bearing `-- Decision:` statements and grouped them by their adjacent contract/model clusters: 217 statements in 26 product-map clusters and 19 statements in 6 peer-ontology clusters. W2's dated local-ingress decisions are attributed through the added invariant rows; unresolved product decisions remain explicit `DEFERRED` rows rather than invented authority surfaces.

### Product map — remaining named invariants

#### `MctRuntimeShape`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `KernelDecidesAdaptersPerform` | COVERED | `mct_daemon::tests::fake_echo_slice_records_trace_and_result`; `mct_daemon::process::tests::process_harness_invokes_only_with_authorized_child_invocation` |
| `DaemonOwnsLifecycleButNotAuthority` | COVERED | `mct_daemon::tests::fake_echo_slice_records_trace_and_result`; `mct_daemon::process::tests::process_harness_invokes_only_with_authorized_child_invocation` |
| `DomainFactsCrossBoundaries` | COVERED | `mct_kernel::call::tests::call_envelope_roundtrip_preserves_semantic_call_across_edges` |
| `ObservationsAreCanonicalRuntimeTruth` | COVERED | `mct_observation::tests::append_and_read_roundtrip`; `mct_daemon::inspector::tests::inspector_filters_observations_by_call_child_and_peer` |
| `IrohConnectivityIsNotMctAuthority` | COVERED | `mct_kernel::call::tests::call_without_admitted_hello_is_denied`; `mct_iroh::tests::unknown_peer_is_denied_before_call` |
| `CurrentMotherIsEvidenceNotOntology` | DEFERRED | Design-provenance constraint rather than an executable runtime path; no compatibility authority may be invented to test it. |
| `ConcreteBeforeSpeculativeTraits` | DEFERRED | Abstraction-timing governance is enforced by review, not by a runtime behavior that a named test can exercise. |

#### `NodeProfileAndTelemetry`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `CapabilityProfileIsStableAndVersioned` | DEFERRED | Full C2 capability-profile revision propagation remains future planner work. |
| `TelemetryExpires` | DEFERRED | C2 live telemetry and staleness policy are not implemented. |
| `CapabilityBeforeTelemetry` | DEFERRED | The capability/telemetry planner split awaits C2 inputs. |
| `TelemetryDoesNotGrantAuthority` | DEFERRED | No live telemetry authority surface exists yet; adding one solely for a test is out of scope. |
| `PublicationIsVisionScoped` | COVERED | `mct_daemon::federation::tests::federation_view_is_vision_scoped` |
| `PrivateProfileIsNotGlobalDiscovery` | DEFERRED | The full private capability profile and discovery protocol remain C2 future scope. |
| `JvmChildDiscoveryIsScoped` | DEFERRED | General JVM capability-profile discovery remains C2 future scope; current callable-surface publication is operation scoped. |

#### `MctIrohPeerBindingAuthority`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `EndpointIdIsTransportOnly` | COVERED | `mct_kernel::peer::tests::endpoint_id_is_serialized_as_transport_text`; `mct_kernel::peer::tests::unknown_endpoint_is_denied` |
| `PeerBindingIsExplicit` | COVERED | `mct_kernel::peer::tests::unknown_endpoint_is_denied`; `mct_kernel::call::tests::call_without_admitted_hello_is_denied` |
| `ReachabilityDoesNotAdmitPeer` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` |
| `MotherOwnsEndpoint` | COVERED | `mct_iroh::tests::mother_owned_endpoint_starts_and_closes`; `mct_daemon::wasm::tests::mct_wit_runtime_rejects_configured_unknown_host_import` |
| `HooksAreEarlyGatesNotAuthorityModel` | COVERED | `mct_iroh::tests::unknown_peer_is_denied_before_call`; `mct_kernel::peer::tests::unknown_endpoint_is_denied` |
| `AdmissionIsObservedBeforePeerEffect` | COVERED | `mct_iroh::tests::failed_hello_observation_prevents_response_and_remembered_admission` |
| `CapabilityTokensDoNotReplaceToyGrants` | COVERED | `mct_daemon::wasm::tests::wasm_component_runtime_invokes_authorized_toy_host_import`; `mct_kernel::toy::tests::manifest_need_without_grant_denies_as_missing_grant` |
| `PeerRelationshipsUseCompanionTaxonomy` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` |
| `CallBindingAdmitsSubmissionOnly` | COVERED | `mct_kernel::call::tests::admitted_hello_allows_call_for_routing`; `mct_kernel::child::tests::not_ready_instance_denies_without_authorization` |
| `PeerRolesAndPairStatesAreDerived` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` |
| `DerivedRoutingRequiresCompanionConjunction` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` |
| `OperatorPointedEgressIsOneObservedDecision` | COVERED | `mct_daemon_bin::ingress::tests::operator_pointed_egress_is_durable_before_send` |

#### `MctIrohProtocolLayer`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `IrohProvidesConnectivityNotAuthority` | COVERED | `mct_iroh::tests::endpoint_config_defaults_to_local_mct_alpns`; `mct_kernel::call::tests::call_without_admitted_hello_is_denied` |
| `WholeAppNodeOwnsLocalTruth` | COVERED | `mct_daemon_bin::resident::serving::tests::resident_mother_serves_peer_control_and_shutdown`; `mct_observation::tests::reopens_existing_hash_chain` |
| `AlpnIsProtocolCompositionSeam` | COVERED | `mct_iroh::tests::endpoint_config_defaults_to_local_mct_alpns`; `mct_iroh::tests::local_iroh_completes_mct_hello_then_call` |
| `GenericBindingDoesNotSupplyProtocolDomainAuthority` | COVERED | `mct_kernel::call::tests::call_without_admitted_hello_is_denied` |
| `FutureProtocolsRequireNamedRelationshipRecords` | DEFERRED | `mct/thought/0`, `mct/observe/0`, and `mct/federation/0` relationships are explicitly future scope. |
| `ChildrenUseCapabilitiesNotTransport` | COVERED | `mct_daemon::wasm::tests::mct_wit_runtime_rejects_configured_unknown_host_import` |
| `RelayChoiceIsReachabilityNotAuthority` | COVERED | `mct_iroh::tests::endpoint_config_can_select_default_relay_mode`; `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` |
| `MultipathIsSubstrateConcern` | COVERED | `mct_iroh::tests::iroh_adapter_observations_cover_endpoint_and_protocol_events` |
| `ContentAddressingIsPayloadTool` | COVERED | `mct_daemon_bin::ingress::tests::jvm_ingress_dereferences_local_content_addressed_blob`; `mct_kernel::call::tests::payload_integrity_decisions_cover_local_content_addressed_blob` |

#### `MctHelloProtocol`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `HelloPrecedesProtectedProtocols` | COVERED | `mct_iroh::tests::local_iroh_completes_mct_hello_then_call`; `mct_iroh::tests::failed_hello_observation_prevents_response_and_remembered_admission` |
| `EndpointMustMatchTransport` | COVERED | `mct_kernel::peer::tests::endpoint_mismatch_is_denied_before_binding_lookup` |
| `VersionNegotiatesBeforeAdmission` | COVERED | `mct_kernel::peer::tests::unsupported_major_version_requests_upgrade` |
| `BindingRequiredForAdmission` | COVERED | `mct_kernel::peer::tests::unknown_endpoint_is_denied`; `mct_iroh::tests::unknown_peer_is_denied_before_call` |
| `AcceptedAlpnsAreIntersection` | COVERED | `mct_kernel::peer::tests::active_binding_admits_intersection_of_requested_policy_and_binding_alpns` |
| `CapabilityViewIsNotGrant` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` |
| `CallableSurfacesUseCompanionPublicationMeaning` | COVERED | `mct_daemon::federation::tests::honest_local_execution_offer_excludes_approved_assigned_non_ready_child` |
| `StaleCallableSurfaceIsNotCandidateEvidence` | COVERED | `mct_daemon_bin::resident::candidates::tests::capability_offer_lapses_at_freshness_boundary` |
| `PublicationPolicyRevisionIsEvidence` | COVERED | `mct_daemon_bin::resident::publication::tests::admitted_hello_refreshes_peer_callable_surfaces` |
| `SafeDenialOnlyToPeer` | COVERED | `mct_kernel::peer::tests::expired_binding_is_denied_with_safe_message` |
| `FailedHelloObservationPreventsAdmissionAndResponse` | COVERED | `mct_iroh::tests::failed_hello_observation_prevents_response_and_remembered_admission` |
| `AdmissionScopeIsNarrow` | COVERED | `mct_iroh::tests::call_rechecks_narrowed_alpn_scope_after_hello`; `mct_iroh::tests::call_rechecks_narrowed_vision_scope_after_hello` |

#### `MctCallAtomicity`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `OneOperation` | COVERED | `mct_kernel::call::tests::call_envelope_roundtrip_preserves_semantic_call_across_edges`; `mct_daemon_bin::resident::pipeline::tests::jvm_bridge_json_call_enters_resident_route_path` |
| `ImmutableAfterConstruction` | COVERED | `mct_kernel::call::tests::call_envelope_roundtrip_preserves_semantic_call_across_edges`; `mct_daemon_bin::resident::pipeline::tests::jvm_bridge_json_call_enters_resident_route_path` |
| `ResultsAreSeparate` | COVERED | `mct_kernel::call::tests::call_envelope_roundtrip_preserves_semantic_call_across_edges`; `mct_daemon_bin::resident::pipeline::tests::jvm_bridge_json_call_enters_resident_route_path` |
| `AdapterNeutralAuthority` | COVERED | `mct_kernel::call::tests::call_envelope_roundtrip_preserves_semantic_call_across_edges`; `mct_daemon_bin::resident::pipeline::tests::jvm_bridge_json_call_enters_resident_route_path` |
| `OriginIsForObservationNotPermission` | COVERED | `mct_kernel::call::tests::only_local_call_origins_allow_remote_candidate_sourcing`; `mct_daemon_bin::resident::decision::tests::forwarded_arrival_with_unavailable_local_candidate_is_terminal` |
| `PayloadMetadataPrecedesPayloadInspection` | COVERED | `mct_kernel::call::tests::payload_metadata_mismatch_is_malformed`; `mct_daemon_bin::resident::payload::tests::resident_local_blob_absent_fails_closed_before_delivery` |

#### `MctCallProtocol`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `RequiresHelloAdmission` | COVERED | `mct_kernel::call::tests::call_without_admitted_hello_is_denied` |
| `OneWireRequestOneSemanticCall` | COVERED | `mct_kernel::call::tests::call_envelope_roundtrip_preserves_semantic_call_across_edges` |
| `WitShapeIsOperationIdentity` | COVERED | `mct_kernel::call::tests::call_envelope_roundtrip_preserves_semantic_call_across_edges` |
| `HelloDoesNotPreAuthorizeCall` | COVERED | `mct_kernel::child::tests::not_ready_instance_denies_without_authorization`; `mct_iroh::tests::call_rechecks_binding_revocation_after_hello` |
| `PeerSubmissionProposesReceiverAsLocalExecutor` | COVERED | `mct_daemon_bin::resident::forwarding::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` |
| `PeerSubmissionIsTerminalAtReceiver` | COVERED | `mct_daemon_bin::resident::decision::tests::forwarded_arrival_with_unavailable_local_candidate_is_terminal` |
| `PeerCallUsesPermanentPerHopVouching` | COVERED | `mct_daemon_bin::resident::forwarding::tests::forwarded_envelope_clears_upstream_user_identity` |
| `ObservationReplicationOwnsCrossLedgerSharing` | DEFERRED | `mct/observe/0` and ObservationReplicationAuthorization are unbuilt future scope. |
| `BrokeredSubmissionCannotExtendCallProtocol` | DEFERRED | Brokered multi-hop submission is an explicitly unbuilt future relationship. |
| `PayloadMetadataMatchesHandle` | COVERED | `mct_kernel::call::tests::payload_metadata_mismatch_is_malformed` |
| `CallerReceivesSafeResultOnly` | COVERED | `mct_daemon_bin::resident::forwarding::tests::two_mother_remote_denial_fails_closed`; `mct_kernel::call::tests::call_protocol_reply_roundtrips_result_payload_handle` |
| `PeerCallObservationsCoverLifecycle` | COVERED | `mct_daemon_bin::ingress::tests::standalone_serve_process_persists_hello_and_call_lifecycle` |

#### `MctResultTerminality`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `ResultRequiresCall` | COVERED | `mct_daemon_bin::resident::execution::tests::resident_execution_runs_wit_child_and_records_trace` |
| `ResultIsTerminal` | COVERED | `mct_daemon::tests::fake_echo_slice_records_trace_and_result`; `mct_daemon_bin::resident::serving::tests::resident_mother_payload_roundtrip_verifies_result_digest` |
| `ClosedOutcomeSet` | COVERED | Adjudicated Option 1 and resolved spec-ward in `8565636`: `mct_daemon_bin::resident::execution::tests::cancelled_result_projection_preserves_cancelled_outcome`; `mct_iroh::tests::cancelled_call_preserves_wire_outcome_route_absence_and_buffered_observations`; `mct_daemon_bin::resident::idempotency::tests::cancelled_idempotent_reply_replays_cancelled_with_durable_observation`. W2 adds `mct_daemon_bin::resident::forwarding::tests::two_mother_forwarding_preserves_cancelled_reply` for the originating resident's remote-reply projection. |
| `DeniedResultHasNoRouteTaken` | COVERED | `mct_kernel::call::tests::denied_result_has_no_route_taken` |
| `CallerSafeResult` | COVERED | `mct_daemon_bin::resident::forwarding::tests::two_mother_remote_denial_fails_closed` |
| `MalformedAdapterInputIsNotMctResult` | COVERED | `mct_kernel::call::tests::call_protocol_json_edge_rejects_invalid_domain_values_with_typed_kernel_error`; `mct_iroh::tests::malformed_frames_are_observed_before_safe_reply_and_append_failure_suppresses_reply` |

#### `RouteDecisionPrivacy`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `InternalByDefault` | COVERED | `mct_kernel::call::tests::call_protocol_reply_roundtrips_route_taken_wire_field` |
| `OpaqueReferenceInResult` | COVERED | `mct_kernel::call::tests::call_envelope_roundtrip_preserves_semantic_call_across_edges` |
| `CandidateReasoningIsSensitive` | COVERED | `mct_daemon_bin::resident::decision::tests::resident_no_route_records_specific_elimination`; `mct_daemon_bin::resident::forwarding::tests::two_mother_remote_denial_fails_closed` |
| `RevalidationIsSeparateDecision` | COVERED | `mct_kernel::route::tests::route_revalidation_allows_matching_execution_authority` |
| `PlannerEvidenceDeferralCannotClaimFullScoring` | DEFERRED | C2 capability and telemetry inputs do not exist, so complete phase-2 evidence cannot yet be exercised. |
| `PlannerEvidenceMustGrowWithPlannerInputs` | DEFERRED | This obligation activates when C2 introduces real phase-2 planner inputs. |
| `SafeSummaryOnlyToRequester` | COVERED | `mct_daemon_bin::resident::forwarding::tests::two_mother_remote_denial_fails_closed` |

#### `WitWasiAlignment`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `OperationTargetIsWitShaped` | COVERED | `mct_daemon::wasm::tests::mct_wit_runtime_invokes_typed_component_export`; `mct_daemon_bin::resident::pipeline::tests::jvm_bridge_json_call_enters_resident_route_path` |
| `AdaptersTranslateToWitSemantics` | COVERED | `mct_daemon::wasm::tests::mct_wit_runtime_invokes_typed_component_export`; `mct_daemon_bin::resident::pipeline::tests::jvm_bridge_json_call_enters_resident_route_path` |
| `WitsAndToysStayCentral` | COVERED | `mct_daemon::wasm::tests::mct_wit_runtime_invokes_typed_component_export`; `mct_daemon_bin::resident::pipeline::tests::jvm_bridge_json_call_enters_resident_route_path` |
| `RuntimeTypesStayOutsideKernel` | COVERED | `mct_daemon::wasm::tests::mct_wit_runtime_invokes_typed_component_export`; `mct_daemon_bin::resident::pipeline::tests::jvm_bridge_json_call_enters_resident_route_path` |

#### `ExternalChildCompatibility`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `RequiredFixturesDoNotRegress` | DEFERRED | `slate-manager` has a real WIT invocation test, but the required versioned `folder-watch-actor` and `watch-null-sink` artifacts are not vendored in this repository. Generated generic modules can prove loader shape, not those external packages' call compatibility; full three-fixture execution coverage must land with the external fixture artifacts rather than a counterfeit local authority surface. |
| `WitOnlyDoesNotRequireLegacyLifecycle` | COVERED | `mct_daemon::children::tests::loads_standalone_wasm_children_from_directory` |
| `LifecycleExportsAreCompatibilityNotIdentity` | COVERED | `mct_daemon::wasm::tests::mct_wit_runtime_invokes_typed_component_export` |
| `ExactWitOperationIds` | COVERED | `mct_daemon::children::tests::registry_routes_only_allowlisted_ready_children`; `mct_daemon::wasm::tests::mct_wit_runtime_resolves_versioned_component_export` |
| `VersionsRemainSeparate` | COVERED | `mct_daemon::wasm::tests::mct_wit_runtime_resolves_versioned_component_export`; `mct_daemon::children::tests::sdk_child_package_uses_manifest_declared_artifact` |
| `RequestedNeedsAreNotGrants` | COVERED | `mct_kernel::toy::tests::manifest_need_without_grant_denies_as_missing_grant` |
| `LegacyInputsCannotExpandCatalog` | COVERED | `mct_daemon::composition::tests::pando_manifest_loader_does_not_hardcode_legacy_builtins`; `mct_daemon::wasm::tests::mct_wit_runtime_rejects_configured_unknown_host_import` |

#### `JvmAsWitChild`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `JvmExportsWitOperations` | COVERED | `mct_daemon_bin::resident::pipeline::tests::jvm_bridge_json_call_enters_resident_route_path`; `mct_daemon_bin::resident::execution::tests::resident_execution_runs_wit_child_and_records_trace` |
| `JvmRuntimeDoesNotChangeAuthority` | COVERED | `mct_daemon_bin::resident::execution::tests::resident_route_revision_guard_denies_before_effect` |
| `JvmDetailsStayBehindChildBoundary` | COVERED | `mct_daemon_bin::resident::pipeline::tests::jvm_bridge_json_call_enters_resident_route_path` |
| `RouteObservationMayNameJvmSubstrate` | COVERED | `mct_daemon_bin::resident::pipeline::tests::jvm_bridge_json_call_enters_resident_route_path` |
| `BankingIsAdoptionTargetNotCoreModel` | DEFERRED | Adoption-domain positioning is product governance, not an executable runtime obligation. |
| `DomainPolicyStillRules` | COVERED | `mct_daemon_bin::resident::execution::tests::resident_route_revision_guard_denies_before_effect` |

#### `MctObservabilitySpine`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `ObservationIsSourceOfTruth` | COVERED | `mct_observation::tests::append_and_read_roundtrip`; `mct_daemon::inspector::tests::inspector_filters_observations_by_call_child_and_peer` |
| `TraceDoesNotGrantAuthority` | COVERED | `mct_kernel::call::tests::call_without_admitted_hello_is_denied`; `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` |
| `ObservationsJoinCausality` | COVERED | `mct_observation::tests::queries_by_trace_and_call`; `mct_daemon_bin::resident::forwarding::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` |
| `ProjectionsAreAudienceFiltered` | COVERED | `mct_daemon::inspector::tests::inspector_filters_observations_by_call_child_and_peer` |
| `DiagnosticsAreNotAuthorityRecord` | COVERED | `mct_kernel::observation::tests::adapter_diagnostic_observation_covers_failure_kinds`; `mct_observation::tests::append_and_read_roundtrip` |

#### `MctAuthorityMutationOwnership`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `ResidentOwnsLiveMutations` | COVERED | `mct_daemon_bin::supervisor_lifecycle::tests::supervised_slate_artifact_acquisition_executes_and_revokes_end_to_end` |
| `OfflineMutationRequiresExclusiveLedgerOwnership` | COVERED | `mct_daemon_bin::control::tests::offline_child_and_identity_mutations_hold_the_writer_lock_and_hide_secrets` |
| `ConnectedResidentResponseNeverFallsBack` | COVERED | `mct_daemon_bin::control::tests::live_resident_refuses_identity_rotation_without_offline_fallback` |
| `MutationDecisionIsDurableBeforeEffect` | COVERED | `mct_daemon_bin::control::tests::administrative_append_failure_and_offline_lock_contention_prevent_state_effects` |
| `IdentityMutationIsOfflineOnly` | COVERED | `mct_daemon_bin::control::tests::live_resident_refuses_identity_rotation_without_offline_fallback` |
| `IdentityBootstrapObservationPrecedesPersistence` | COVERED | `mct_daemon_bin::resident::serving::tests::bootstrap_identity_append_failure_leaves_no_identity_effect` |

#### `MctObservationSubsystemCoverage`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `CallIngressCoverage` | COVERED | `mct_iroh::tests::malformed_frames_are_observed_before_safe_reply_and_append_failure_suppresses_reply`; `mct_daemon_bin::ingress::tests::standalone_serve_process_persists_hello_and_call_lifecycle` |
| `AuthorityCoverage` | COVERED | `mct_kernel::observation::tests::kernel_denial_evaluations_become_observations`; `mct_iroh::tests::iroh_adapter_observations_cover_endpoint_and_protocol_events` |
| `RoutingCoverage` | COVERED | `mct_kernel::observation::tests::candidate_observations_record_specific_elimination_class`; `mct_kernel::observation::tests::route_revalidation_observation_records_allowed_and_denied_outcomes` |
| `ResultCoverage` | COVERED | Resolved spec-ward in `8565636` after applying the triage rule through the real resident and Iroh paths. `mct_iroh::tests::cancelled_call_preserves_wire_outcome_route_absence_and_buffered_observations` proves cancelled result and reply facts retain `cancelled` under buffered durability; `mct_daemon_bin::resident::idempotency::tests::cancelled_idempotent_reply_replays_cancelled_with_durable_observation` proves replay retains the outcome under before-effect durability. |
| `ChildLifecycleCoverage` | COVERED | `mct_kernel::observation::tests::child_authority_and_instance_observation_matrix_is_typed` proves approval, assignment, and instance-state projections; `mct_daemon_bin::supervisor_lifecycle::tests::supervised_slate_artifact_acquisition_executes_and_revokes_end_to_end` proves verified acquisition, approval, assignment, execution, and revocation through resident control; `mct_daemon::acquisition::tests::artifact_acquisition_failures_are_observed_without_artifact_publication` proves rejection evidence. |
| `ToyCoverage` | COVERED | `mct_kernel::observation::tests::toy_grant_evaluations_become_observations`; `mct_daemon::toy::tests::toy_backend_failure_is_adapter_observation_not_kernel_denial` |
| `PeerCoverage` | COVERED | `mct_iroh::tests::iroh_adapter_observations_cover_endpoint_and_protocol_events`; `mct_daemon_bin::ingress::tests::standalone_serve_process_persists_hello_and_call_lifecycle` |
| `RuntimeAdapterCoverage` | COVERED | `mct_daemon::process::tests::process_harness_timeout_returns_typed_result_and_observation`; `mct_daemon::wasm::tests::wasm_component_runtime_trap_maps_to_adapter_observation`; `mct_daemon_bin::resident::pipeline::tests::jvm_bridge_json_call_enters_resident_route_path` |
| `StorageAndBackpressureCoverage` | COVERED | `mct_daemon_bin::control::tests::resident_append_failure_prevents_peer_config_effect`; `mct_daemon_bin::resident::observation::tests::resident_hello_observations_are_durable_before_responses` |
| `ProjectionCoverage` | COVERED | `mct_daemon::metrics::tests::metrics_snapshot_projects_state_summary`; `mct_daemon::inspector::tests::inspector_filters_observations_by_call_child_and_peer` |

#### `MctLocalFirstObservationLedger`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `LocalLedgerIsCanonical` | COVERED | `mct_observation::tests::append_and_read_roundtrip` |
| `ExternalSystemsAreProjections` | COVERED | `mct_daemon::metrics::tests::metrics_snapshot_projects_state_summary`; `mct_daemon::inspector::tests::inspector_filters_observations_by_call_child_and_peer` |
| `AppendOnlyEvidence` | COVERED | `mct_observation::tests::reopens_existing_hash_chain` |
| `LocalSequenceIsNotGlobalTime` | COVERED | `mct_observation::tests::reopens_existing_hash_chain`; `mct_daemon_bin::resident::forwarding::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` |
| `TraceReconstructsFromLedger` | COVERED | `mct_observation::tests::queries_by_trace_and_call`; `mct_daemon_bin::resident::forwarding::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` |
| `BufferedEffectsAreBounded` | COVERED | `mct_daemon_bin::resident::observation::tests::resident_observation_queue_is_bounded_and_acknowledged` proves the named finite queue and acknowledged buffered append; `mct_daemon_bin::control::tests::resident_append_failure_prevents_peer_config_effect` proves a closed sink is visible and prevents the protected effect. |
| `ProjectionFailureDoesNotChangeTruth` | DEFERRED | No external projection/export retry subsystem exists yet; the local ledger path is covered independently. |

#### `MctImmutabilityModel`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `IdentityHasSuccessiveValues` | COVERED | `mct_daemon::config::tests::config_store_persists_local_identity_without_storing_secret`; `mct_daemon_bin::ingress::tests::offline_identity_cli_observes_before_creating_key_and_config` |
| `FactsAreNotRewritten` | COVERED | `mct_observation::tests::reopens_existing_hash_chain` |
| `CurrentStateIsProjection` | COVERED | `mct_daemon::state::tests::state_store_persists_runs_observations_and_metrics`; `mct_daemon::metrics::tests::metrics_snapshot_projects_state_summary` |
| `MutationBoundariesAreNamed` | COVERED | `mct_daemon_bin::control::tests::administrative_append_failure_and_offline_lock_contention_prevent_state_effects` |
| `EffectsAtEdges` | COVERED | `mct_daemon::process::tests::process_harness_invokes_only_with_authorized_child_invocation` |
| `PlainDataIsInspectable` | COVERED | `mct_kernel::call::tests::mct_call_roundtrips_as_json`; `mct_daemon::inspector::tests::inspector_filters_observations_by_call_child_and_peer` |

#### `MctChildComponentLifecycle`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `ArtifactIsImmutableValue` | COVERED | `mct_daemon::registry::tests::installs_verified_package_atomically_under_child_name`; `mct_daemon::children::tests::strict_integrity_requires_hash_sidecars` |
| `ApprovalIsAuthorityNotRuntime` | COVERED | `mct_daemon::federation::tests::honest_local_execution_offer_excludes_approved_assigned_non_ready_child` |
| `AssignmentIsScopedBinding` | COVERED | `mct_daemon::state::tests::state_store_enforces_active_assignment_requires_approved_artifact`; `mct_kernel::child::tests::revoked_assignment_denies_without_authorization` |
| `InstanceIsLiveGeneration` | COVERED | `mct_daemon::lifecycle::tests::reload_records_replacement_ready_before_predecessor_drain` |
| `WitOnlyChildrenAreValid` | COVERED | `mct_daemon::wasm::tests::mct_wit_runtime_invokes_typed_component_export` |
| `LegacyLifecycleIsCompatibility` | COVERED | `mct_daemon::children::tests::loads_standalone_wasm_children_from_directory` |
| `ReplacementLoadsBeforeSwap` | COVERED | `mct_daemon::lifecycle::tests::reload_records_replacement_ready_before_predecessor_drain`; `mct_daemon::state::tests::child_reload_swap_is_atomic_and_failed_swap_keeps_persisted_predecessor_ready` |
| `CallsRequireReadyAuthorizedInstance` | COVERED | `mct_kernel::child::tests::ready_approved_assigned_instance_produces_authorized_child_invocation`; `mct_daemon::process::tests::process_harness_denies_stale_child_capability_before_spawn` |
| `FailedReplacementDoesNotPoisonCurrent` | COVERED | `mct_daemon_bin::cli_runtime::tests::reload_command_failure_keeps_persisted_generation_ready_and_routable` |
| `LifecycleTransitionsAreObserved` | COVERED | `mct_kernel::observation::tests::child_authority_and_instance_observation_matrix_is_typed`; `mct_daemon_bin::supervisor_lifecycle::tests::supervised_slate_artifact_acquisition_executes_and_revokes_end_to_end`; `mct_daemon_bin::control::tests::artifact_writer_loss_after_read_leaves_no_artifact_authority_or_catalog_package`; A7 replacement ordering remains covered by `mct_daemon::lifecycle::tests::reload_records_replacement_ready_before_predecessor_drain`. |

#### `MctToyGrantAuthority`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `CanonicalCatalogIsClosed` | COVERED | `mct_kernel::toy::tests::unknown_toy_denies_by_default`; `mct_daemon::wasm::tests::mct_wit_runtime_rejects_configured_unknown_host_import` |
| `NeedsAreRequestsNotGrants` | COVERED | `mct_kernel::toy::tests::manifest_need_without_grant_denies_as_missing_grant` |
| `GrantScopeIsExplicit` | COVERED | `mct_kernel::toy::tests::wrong_scope_denies_without_authorization`; `mct_daemon::toy::tests::secret_toy_grant_evaluation_requires_explicit_scope` |
| `EvaluationProducesAuthorizationToken` | COVERED | `mct_kernel::toy::tests::active_grant_produces_authorized_toy_call`; `mct_daemon::toy::tests::toy_adapter_requires_authorized_toy_call_and_records_success` |
| `RoutePlannerCannotGrantToys` | COVERED | `mct_kernel::route::tests::route_revalidation_denies_failed_toy_evidence` |
| `ExpiryAndRevocationAreFacts` | COVERED | `mct_kernel::toy::tests::expired_time_window_denies_without_authorization`; `mct_kernel::toy::tests::revoked_grant_denies_without_authorization` |
| `GrantSnapshotsAreCacheNotTruth` | COVERED | `mct_daemon::state::tests::state_store_persists_toy_grant_snapshots`; `mct_kernel::toy::tests::stale_grant_revision_denies_without_authorization` |
| `ToyGrantDecisionsAreObserved` | COVERED | `mct_kernel::observation::tests::toy_grant_observation_matrix_distinguishes_expiry_and_revocation` proves allow, generic deny, expired, and revoked decisions retain distinct typed facts and authority revisions. |
| `ToyBackendFailureIsAdapterEffect` | COVERED | `mct_daemon::toy::tests::toy_backend_failure_is_adapter_observation_not_kernel_denial` |
| `FixtureCompatibilityUsesExplicitGrants` | COVERED | `mct_daemon::wasm::tests::slate_manager_list_work_runs_through_mct_wit_runtime`; `mct_kernel::toy::tests::manifest_need_without_grant_denies_as_missing_grant` |

### Peer ontology — remaining named invariants

#### `PeerRelationshipTaxonomy`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `MissingDerivationMeansMissingRecord` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` |
| `DirectionalRecordsRemainIndependent` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` |
| `AdmissionDenialIsNonBeginning` | COVERED | `mct_iroh::serve::tests::denied_hellos_leave_no_per_peer_state` |

#### `PeerOperationalRoleDerivation`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `ImmediateCallerDerivation` | COVERED | `mct_daemon_bin::resident::forwarding::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` |
| `CapabilityPublisherDerivation` | COVERED | `mct_daemon_bin::resident::publication::tests::admitted_hello_refreshes_peer_callable_surfaces` |

#### `CapabilityPublicationRelationship`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `OfferIsNotExecutionGuarantee` | COVERED | `mct_daemon_bin::resident::candidates::tests::eligible_route_candidate_requires_every_current_conjunct` |
| `BrokerageRequiresDifferentOntology` | COVERED | `mct_daemon_bin::resident::forwarding::tests::two_mother_mutual_publication_with_unready_children_terminates_single_hop` |

#### `FuturePeerRelationshipSlots`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `ThoughtExchangeHasIndependentAuthority` | DEFERRED | ThoughtExchangeAuthorization and `mct/thought/0` are future scope. |
| `ObservationReplicationHasIndependentAuthority` | DEFERRED | ObservationReplicationAuthorization and `mct/observe/0` are future scope. |
| `FederationControlHasIndependentAuthority` | DEFERRED | FederationControlAuthorization and `mct/federation/0` are future scope. |
| `BrokeredSubmissionIsANewRelationship` | DEFERRED | Brokered multi-hop submission is future scope and cannot be added to terminal `mct/call/0` for testability. |

## Daily-Driver Slice 3 — artifact acquisition

### `MctArtifactAcquisitionAuthority`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `AcquisitionRequiresExplicitAuthorityPath` | COVERED | `mct_kernel::artifact::tests::artifact_acquisition_requires_source_path_and_current_adapter_authority`; `mct_kernel::artifact::tests::source_trust_and_adapter_authority_are_independent_and_exact` |
| `StandingSourceAuthorityIsExplicitAndBounded` | COVERED | `mct_daemon::state::tests::standing_source_creation_rejects_unbounded_credentialed_and_unsupported_records`; `mct_kernel::artifact::tests::standing_source_rejects_stale_revoked_expired_wrong_scope_and_policy` |
| `OperatorPointedAcquisitionCreatesNoAmbientTrust` | COVERED | `mct_daemon::acquisition::tests::identical_reacquisition_adds_evidence_without_replacing_immutable_artifact` proves two attempts consume two distinct decisions; the projection has no reusable source record. |
| `SourceTrustAndAdapterAuthorityAreIndependent` | COVERED | `mct_kernel::artifact::tests::source_trust_and_adapter_authority_are_independent_and_exact`; `mct_daemon_bin::control::tests::artifact_acquisition_append_failure_suppresses_filesystem_and_catalog_effects` |
| `DigestVerificationIsUnwaivableFloor` | COVERED | `mct_daemon::acquisition::tests::staged_package_reconciles_sha256_floor_with_blake3_acquisition_evidence`; `mct_daemon::acquisition::tests::malformed_tampered_oversize_and_escaping_sources_leave_attempt_evidence_only` |
| `VerificationGatesArtifactRecord` | COVERED | `mct_daemon::acquisition::tests::artifact_acquisition_failures_are_observed_without_artifact_publication`; `mct_daemon_bin::control::tests::artifact_writer_loss_after_read_leaves_no_artifact_authority_or_catalog_package` |
| `AcquisitionGrantsNoLifecycleAuthority` | COVERED | `mct_daemon_bin::supervisor_lifecycle::tests::supervised_slate_artifact_acquisition_executes_and_revokes_end_to_end` proves denial after verified publication and before exact approval/assignment. |
| `AcquisitionIsIndependentLifecycleFact` | COVERED | The same supervised test correlates separate acquisition, verification, approval, assignment, run, and revocation facts across reopen. |
| `FailedAcquisitionCreatesNoArtifact` | COVERED | `mct_daemon::acquisition::tests::artifact_acquisition_failures_are_observed_without_artifact_publication`; `malformed_tampered_oversize_and_escaping_sources_leave_attempt_evidence_only` |
| `UniformAcquisitionProvenance` | COVERED | `mct_daemon::state::tests::component_artifacts_require_real_acquisition_or_explicit_legacy_migration`; supervised Slate proof. |
| `HistoricalUnknownProvenanceIsExplicit` | COVERED | `mct_daemon::state::tests::pre_v7_artifact_migration_marks_historical_unknown_without_fabricating_acquisition`; `mct_daemon_bin::supervisor_lifecycle::tests::exact_approval_refuses_wrong_historical_failed_and_tampered_artifact_evidence` |
| `UpdatesRequireExactArtifactApproval` | COVERED | `mct_daemon_bin::control::tests::child_name_only_approval_is_rejected_before_authority_or_config_effect`; `mct_daemon_bin::supervisor_lifecycle::tests::exact_approval_refuses_wrong_historical_failed_and_tampered_artifact_evidence` |
| `ChannelSimilarityCannotTransferApproval` | COVERED | Exact digest/package/catalog matching in `exact_approval_refuses_wrong_historical_failed_and_tampered_artifact_evidence`; no channel field exists in the approval mutation. |
| `RevocationCannotApproveReplacement` | COVERED | Supervised Slate proof revokes, restarts, and remains denied while the immutable package and acquisition evidence remain present. |
| `PreauthorizedChannelsRequireNewAuthorityLaw` | DEFERRED | No channel or update scheduler was added; introducing one remains a separately gated law change. |
| `SourceCredentialsRemainSeparateAuthority` | COVERED | Source-record validation accepts only credential-free canonical `file://` roots and stores no credential field; this slice adds no network/secret attachment path. |

### `MctChildComponentLifecycle` acquisition extension

| Invariant | Status | Evidence / reason |
|---|---|---|
| `AcquisitionIsFifthIndependentFact` | COVERED | Supervised Slate proof and `mct_daemon::state::tests::component_artifacts_require_real_acquisition_or_explicit_legacy_migration`. |
| `NewArtifactsRequireAcquisitionProvenance` | COVERED | `component_artifacts_require_real_acquisition_or_explicit_legacy_migration`; `artifact_acquisition_failures_are_observed_without_artifact_publication`. |
| `ArtifactIsImmutableValue` | COVERED | `mct_daemon::acquisition::tests::identical_reacquisition_adds_evidence_without_replacing_immutable_artifact`; `same_digest_different_manifest_fact_cannot_replace_catalog_artifact`. |
| `ApprovalIsAuthorityNotRuntime` | COVERED | Supervised proof denies before approval and again before ToyGrants, then requires current run authority. |
| `AssignmentIsScopedBinding` | COVERED | Supervised exact approval response names the artifact and acquisition, while existing state/kernel assignment tests retain exact scope enforcement. |
| `CallsRequireReadyAuthorizedInstance` | COVERED | Supervised proof exercises pre-approval denial, post-grant execution, revocation denial, and restart-visible denial. |
| `LifecycleTransitionsAreObserved` | COVERED | Supervised proof correlates acquisition/verification/approval/assignment/revocation observation ids; writer-loss test proves no unobserved artifact authority survives. |

### `PatinaRegistrySyncQuarryDisposition`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `GenericRegistryMechanismBecomesMctProduct` | COVERED | `mct_daemon_bin::supervisor_lifecycle::tests::artifact_command_surface_is_explicit_and_supervisor_distinct` |
| `SourceAccessBecomesToyAdapter` | COVERED | D1.5/D1.15 direct-operator filesystem effect capability is proved by kernel independence tests and `artifact_slice_exposes_only_filesystem_adapter_and_existing_toy_catalog`; a future network source still requires the deny-by-default egress Toy and separate connection/secret authority. |
| `RegistryAuthorityRemainsKernel` | COVERED | `mct_kernel::artifact::tests::source_trust_and_adapter_authority_are_independent_and_exact`; adapters receive only the private authorized capability. |
| `AcquisitionFactsAreEvidenceNotGrants` | COVERED | Supervised proof requires separate exact approval, assignment, and four existing ToyGrants after acquisition. |
| `PatinaSourceMeaningRemainsChildMeaning` | COVERED | Standing-source evaluation derives package WIT namespaces and checks exact artifact/publisher/namespace/action scope; no Mother relationship meaning is inferred. |
| `RegistryToolingIsOptionalAndOutsideKernel` | COVERED | CLI/UDS/filesystem staging remain daemon adapters; kernel contains only authority values/evaluation and no registry client. |
| `AmbientRegistryShapeIsRejected` | COVERED | `mct_daemon_bin::control::tests::live_registry_install_and_sync_are_closed_without_storage_effects`; `registry_is_closed_and_offline_lock_contention_refuses_legacy_helper`. |
| `RecurringSyncAwaitsSchedulingLaw` | DEFERRED | No trigger, watcher, recurring sync, or scheduling path was added. |
| `SourceCredentialsRemainIndependent` | DEFERRED | Filesystem acquisition needs none; future credential attachment and network connection authority remain an explicit empty slot. |

### Acquisition structural projections

| Plan obligation group | Status | Evidence |
|---|---|---|
| `ArtifactSourceScope`, `ArtifactSourceAuthority`, reader/projection fields | COVERED | Kernel standing-scope matrix plus state create/revoke/reopen and record-digest tests. |
| `OperatorPointedArtifactAcquisitionDecision`, reader/projection fields | COVERED | `identical_reacquisition_adds_evidence_without_replacing_immutable_artifact` persists and reopens distinct consumed decisions. |
| `ArtifactAcquisition`, reader/projection fields | COVERED | Failure matrix, reacquisition tests, and supervised exact observation-id correlation/reopen. |
| New `ComponentArtifact.provenance_status`, `acquisition_ids`, catalog exposure | COVERED | v7 migration tests, immutable-fact collision test, exact approval evidence projection, and supervised reopen. |

## Replacement Slice 4A — trigger authority and resident scheduler

### Trigger structural projections

The current `allium plan layer/allium/mct-product-map.allium` emits 232 obligations. Part A dispositions the eleven trigger-specific obligations below; Part B owns the later Watch/event structures.

| Plan obligation group | Status | Evidence |
|---|---|---|
| `value-equality.CallTriggerScope`, `entity-fields.CallTriggerScope` | COVERED | `mct_kernel::trigger::tests::trigger_authority_validation_is_closed_and_bounded`; deterministic identity and closed policy tests. |
| `entity-fields.CallTriggerAuthority`, `surface-actor.CallTriggerAuthorityProjection`, `surface-exposure.CallTriggerAuthorityProjection` | COVERED | `mct_daemon::state::tests::trigger_authority_projection_is_revisioned_current_and_non_resurrecting`; `mct_daemon_bin::triggers::tests::trigger_authority_is_scoped_observed_revisioned_and_revocable`. |
| `entity-fields.CallTriggerFiringEvidence`, `surface-actor.CallTriggerFiringProjection`, `surface-exposure.CallTriggerFiringProjection` | COVERED | `mct_daemon_bin::resident::trigger_scheduler::tests::resident_temporal_trigger_fires_once_and_recovers_without_duplication`; `trigger_evaluate_crash_re_evaluate_cannot_double_fire`. |
| `entity-fields.CallTriggerPendingOccurrence`, `surface-actor.CallTriggerPendingProjection`, `surface-exposure.CallTriggerPendingProjection` | COVERED | `mct_daemon_bin::resident::trigger_scheduler::tests::trigger_overlap_policies_preserve_one_active_call_and_order`; `trigger_append_failure_suppresses_pending_and_call_effects`; schema-v8 projection tests. |

### `MctCallTriggerAuthority`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `ManagementRequiresCurrentLocalAuthority` | COVERED | Trigger UDS mutations consume authenticated peer credentials; offline mutation derives the owner from the current owner-private config and acquires the canonical ledger writer. `mct_daemon_bin::triggers::tests::trigger_authority_is_scoped_observed_revisioned_and_revocable`; existing authenticated UDS dispatch tests. |
| `TriggerScopeIsExplicitAndBounded` | COVERED | `mct_kernel::trigger::tests::trigger_authority_validation_is_closed_and_bounded`; `mct_daemon_bin::triggers::tests::trigger_management_rejects_event_and_authority_expansion`. |
| `ActivationFollowsDurableAuthorityFact` | COVERED | `mct_daemon_bin::triggers::tests::trigger_append_failure_suppresses_activation_revision_and_revocation`; startup reconciles canonical `call-trigger-authority-v1` facts before readiness. |
| `TriggerAuthorityCannotExpandCallAuthority` | COVERED | `trigger_management_rejects_event_and_authority_expansion`; the primary integration re-enters the ordinary resident child/route/revalidation executor. |
| `EachFiringCreatesFreshCall` | COVERED | Kernel identity tests and the primary integration assert deterministic occurrence-specific `call-trigger:` ids and one effect. |
| `StaleTriggerCannotPreserveAuthority` | COVERED | The primary integration revokes revision one, records one revision-two suppression, and proves no later child effect; recovered execution performs a fresh current-record check. |
| `ChildRequestIsNeverTriggerGrant` | COVERED | `mct_daemon_bin::resident::local_ingress::tests::locally_submitted_body_cannot_claim_trigger_firing_context`; trigger management is absent from Child/WASM imports. |
| `FiringEvidenceCarriesTriggerProvenance` | COVERED | The primary integration reopens authority, occurrence, firing, call, result, and revocation references; `call-trigger-firing-v1` contains exact record/policy/occurrence evidence without payload bytes. |
| `TriggerFiringOriginIsTruthfulAndAdditive` | COVERED | `mct_kernel::call::tests::trigger_firing_origin_is_additive_local_and_single_hop`; primary integration persists `CallOrigin::TriggerFiring`. |
| `TriggerFiringIdempotencyIsRecordAndOccurrenceScoped` | COVERED | `mct_kernel::trigger::tests::trigger_firing_identities_are_record_revision_and_occurrence_scoped`; `mct_daemon_bin::resident::idempotency::tests::trigger_firing_idempotency_is_record_and_occurrence_scoped`. |
| `TriggerFiringIsLocalAndSingleHop` | COVERED | `trigger_firing_origin_is_additive_local_and_single_hop`; existing forwarding rewrites every receiver-side arrival to `Iroh`. |
| `MechanismDoesNotOwnCadenceMeaning` | COVERED | Trigger records contain fixed source/policy/call facts only; the primary proof invokes an ordinary Child operation and adds no application debounce/filter meaning to Mother. |
| `MissedFirePolicyIsExplicitAndDefaultsToSkip` | COVERED | Kernel default test; management test proves omitted policy persists as `skip`. |
| `CatchUpUsesOnlyKnownOccurrences` | COVERED | `mct_daemon_bin::resident::trigger_scheduler::tests::temporal_occurrence_range_is_deterministic_and_exclusive_at_expiry`; only mathematically derived temporal occurrences enter Part A. |
| `CurrentAuthorityDefeatsCatchUp` | COVERED | Primary integration's revoked-next-occurrence suppression; fresh current checks precede firing and pending dequeue. |
| `CatchUpIsBoundedByNamedConstants` | COVERED | `trigger_production_limits_are_exactly_named`; `trigger_missed_fire_policies_are_bounded_deterministic_and_countable`. |
| `MissedFireDispositionsRemainEvidence` | COVERED | `trigger_missed_fire_policies_are_bounded_deterministic_and_countable`; `trigger_terminal_dispositions_survive_restart_without_resurrection`. |
| `PolicyRevisionDoesNotReinterpretMisses` | COVERED | Immutable revision projection test plus pending records retaining exact record/policy revisions; dequeue suppresses stale revisions rather than rewriting them. |
| `CatchUpIdentityIsDeterministic` | COVERED | `mct_kernel::trigger::tests::trigger_firing_identities_are_record_revision_and_occurrence_scoped`; coalesced represented-set equality tests. |
| `OverlapPolicyIsExplicitAndDefaultsToRefuse` | COVERED | Kernel default test and trigger management create test. |
| `OneActiveCallPerTriggerRecord` | COVERED | Schema-v8 partial unique index plus `trigger_evaluate_crash_re_evaluate_cannot_double_fire`. |
| `OverlapPendingStateIsPerRecordBounded` | COVERED | `trigger_overlap_policies_preserve_one_active_call_and_order`; `trigger_capacity_refuses_at_each_named_bound_without_eviction`. |
| `QueueAdmissionIsNotDeliveryOutcome` | COVERED | Pending and firing/result are separate projections; overlap tests assert pending reason rather than target outcome. |
| `CoalescingStagesRemainDistinct` | COVERED | Missed-fire coalescing and overlap pending coalescing use distinct typed decisions and evidence prefixes; policy tests exercise both. |
| `PendingIdentityAndOrderAreDeterministic` | COVERED | Pending ids derive from occurrence ids; schema enforces unique per-record admission sequence; overlap test covers retained pending identity. |
| `MissedFirePrecedesOverlap` | COVERED | `trigger_admission_order_is_fixed_and_authority_neutral` proves a missed terminal result returns before overlap and capacity states. |
| `TriggerQueuesAndActiveCallsUseThreeNamedBounds` | COVERED | `trigger_production_limits_are_exactly_named`; `trigger_capacity_refuses_at_each_named_bound_without_eviction`. |
| `AdmissionOrderIsFixedAndAuthorityNeutral` | COVERED | `trigger_admission_order_is_fixed_and_authority_neutral`; no capacity branch mints or widens authority. |
| `PendingAdmissionIsDurableBeforeVisibility` | COVERED | `trigger_append_failure_suppresses_pending_and_call_effects`; pending evidence contains both projections and startup replay applies it transactionally. |
| `PendingAdmissionNeverEvicts` | COVERED | Capacity test plus insert-only pending schema; no eviction mutation exists. |
| `NoImplicitResidentRetryQueue` | COVERED | Direct active-capacity failure is terminal; only overlap-authorized pending rows may wait. In-flight idempotency remains active and is never expired into hidden re-execution by the trigger scheduler. |
| `TriggerWorkCannotStarveResidentControl` | COVERED | `trigger_load_does_not_starve_writer_control_status_or_ordinary_calls` proves independent active permits and writer progress; scheduler turn/poll constants are asserted. |
| `DequeueAndRestartAreDeterministic` | COVERED | Primary integration deletes trigger projections, replays the validated ledger, restarts, and proves no second effect; pending order is `(trigger id, admission sequence)`. |
| `DequeueRechecksCurrentLaw` | COVERED | Pending dequeue requires exact current record/policy/validity and otherwise appends `call-trigger-pending-suppressed-v1` before terminal projection. |
| `TerminalDispositionPreventsResurrection` | COVERED | `trigger_terminal_dispositions_survive_restart_without_resurrection`; primary integration's ledger-rebuild proof. |
| `LaterOccurrencesRequireExplicitPolicy` | COVERED | Temporal identity includes nominal occurrence; terminal rows advance the exact watermark, while only a distinct later nominal occurrence can enter policy evaluation. |

### Named deferrals and interdicts

| Slot | Status | Evidence / reason |
|---|---|---|
| `MotherEventSourceAdapterRuntime` | DEFERRED | Production management rejects `trigger_class=event` with the exact named message; Part A adds no Mother observer task, source registration, or adapter-trigger lookup. |
| `RegistrySyncTriggerComposition` | DEFERRED | Trigger target validation rejects registry-sync composition; no unattended sync target or call path exists. |
| `NetworkArtifactAcquisitionAdapter` | DEFERRED | No network/acquisition adapter, credential field, source authority, or trigger-carried acquisition authority was added. This remains coupled to `RegistrySyncTriggerComposition`. |

### Observation-kind composition

| Obligation | Status | Evidence |
|---|---|---|
| Trigger authority, lifecycle, firing, and completion use existing kinds | COVERED | `mct_daemon_bin::resident::trigger_scheduler::tests::trigger_observation_mapping_uses_existing_kinds`; trigger authority uses `OperatorActionRecorded`, occurrence state uses `LifecycleTransitionRecorded`, firing uses `CallConstructed`, and terminal calls retain `ResultRecorded`. `ObservationKind` is unchanged. |

## Replacement Slice 4B — Watch delivery and exact fixtures

### Watch/event structural projections

| Plan obligation group | Status | Evidence |
|---|---|---|
| `WatchObservationScope` value/entity/projection obligations | COVERED | `mct_kernel::watch::tests::watch_scope_validation_is_closed_bounded_and_digest_bound`; `mct_daemon::state::tests::watch_scope_projection_is_revisioned_current_and_sequences_are_monotonic`; `mct_daemon_bin::watch::tests::watch_scope_and_toy_grant_are_both_current_before_observation`. |
| `WatchEventBatchEvidence`, `WatchEventEvidence`, and delivery projection obligations | COVERED | `mct_daemon_bin::watch::tests::watch_batches_are_bounded_sequenced_deterministic_and_countable`; composed supervised fixture proof and SQLite schema-v10 reopen assertions. |

### `MctEventSourcePlacement`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `OneObservationShapePerPath` | COVERED | Child observation requires the exact `ChildToy` scope; `MotherEventSourceAdapterRuntime` remains absent and deferred. |
| `DirectChildObservationRequiresWatchToy` | COVERED | `watch_scope_and_toy_grant_are_both_current_before_observation`. |
| `MotherObservationRequiresIndependentEffectAuthority` | DEFERRED | Exact named `MotherEventSourceAdapterRuntime`; Part B adds no Mother observer path. |
| `MotherAdapterCannotImpersonateChild` | COVERED | Watch authorization requires exact Child artifact and assignment; no Mother adapter exists. |
| `WatchToyGrantsObservationOnly` | COVERED | `watch_grant_cannot_read_content_state_or_originate_delivery`; directory-read, keyvalue, observability, and call-out checks remain separate. |
| `ChildEmissionReentersCallLaw` | COVERED | `watcher_child_callout_reenters_ordinary_call_law`; supervised sink delivery traverses ordinary resident call/result law. |
| `TriggerAuthorityIsSoleStandingOrigination` | COVERED | Watch grants contain no schedule/call target; composed proof uses the independent temporal trigger to invoke the watcher. |

### `MctWatchObservationScope`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `RootAndBreadthAreExplicit` | COVERED | Kernel scope validation and owner-authenticated Watch grant tests. |
| `TraversalIsExplicit` | COVERED | Scope enum is closed; the v1 adapter refuses `root_only` rather than widening it to recursive. |
| `EventClassesAreExplicit` | COVERED | Send-time admission requires the emitted class in the current scope. |
| `BatchIsBoundedByNamedCeiling` | COVERED | `watch_send_admission_refuses_paths_shape_and_capacity_synchronously`; kernel and daemon batch-bound tests. |
| `CoalescingIsExplicitDeterministicAndCountable` | COVERED | `watch_batches_are_bounded_sequenced_deterministic_and_countable`. |
| `ValidityIsCurrentAndBounded` | COVERED | `watch_scope_and_toy_grant_are_both_current_before_observation`; supervised post-revocation denial. |
| `WatchAuthorityIsObservationOnly` | COVERED | `watch_grant_cannot_read_content_state_or_originate_delivery`. |
| `SafeMetadataIsCanonicalAndRootRelative` | COVERED | Send-time safe-path validation and `watch_adapter_excludes_escaped_symlinks_and_absolute_paths`. |
| `EscapedSubjectsAreExcluded` | COVERED | `watch_adapter_excludes_escaped_symlinks_and_absolute_paths`. |
| `DeliveryCarriesExactScopeProvenance` | COVERED | Batch/event/delivery schema tests plus composed reopen proof. |
| `BatchSequenceIsMonotonicPerScope` | COVERED | State sequence-counter test and deterministic batch test. |

### `MctLegacyWatchEventsCompatibility`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `LegacyAbsolutePathSlotIsNarrowed` | COVERED | Exact source-derived patch plus send-time `validate_legacy_watch_paths`. |
| `MismatchIsRefusedWithEvidence` | COVERED | `legacy_watch_abi_mismatch_is_refused_before_sink_call`; typed send refusal and batch admission barrier tests. |
| `NarrowingIsLegacyAbiOnly` | COVERED | Kernel validator rejects non-0.1.x interface identities. |
| `SuccessorDropsDeprecatedSlot` | COVERED | Contract schema test refuses carrying the legacy slot into a successor; no successor runtime registration exists. |

### `MctWatchEventDelivery`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `DeliveryPathsAreExclusive` | COVERED | Part B implements only Child call-out; `MotherEventSourceAdapterRuntime` has no executable path. |
| `ChildDeliveryIsOrdinaryCurrentCall` | COVERED | `watcher_child_callout_reenters_ordinary_call_law`. |
| `WasmFixtureUsesTruthfulWasmHostOrigin` | COVERED | Nested fixture calls persist `CallOrigin::WasmHost`; parent trigger lineage remains separate. |
| `MotherDeliveryRequiresTriggerAuthority` | DEFERRED | `MotherEventSourceAdapterRuntime`; no Mother delivery is implemented. |
| `TriggerLineageIsNeverFabricated` | COVERED | `watch_delivery_lineage_is_actual_and_never_fabricated`. |
| `BatchEvidenceIsCompleteAndScoped` | COVERED | Deterministic batch test and supervised persisted/reopened summary. |
| `EventEvidenceIsSafeAndCausal` | COVERED | Safe adapter test, deterministic identity helpers, and exact parent-call linkage. |
| `DurableReceiptAndEligibilityPrecedeDelivery` | COVERED | D1B.7-A.1: the invocation-local admitted set is normalized and appended through the canonical writer before the first nested effect; `watch_admission_append_failure_suppresses_every_nested_delivery` proves the failure barrier. |
| `PreCallDispositionSetIsClosed` | COVERED | Kernel closed enum and `watch_delivery_reuses_closed_mct_result_outcomes`. |
| `PostCallOutcomeReusesMctResult` | COVERED | `watch_delivery_reuses_closed_mct_result_outcomes`. |
| `DeliveredMeansDurableTargetSuccess` | COVERED | Delivery projection references the target result and follows completed success only. |
| `SinkEffectsRequireSinkGrants` | COVERED | `exact_watch_null_sink_executes_without_watch_or_filesystem_authority`; composed proof grants logging/measure independently. |

### `PatinaWatcherQuarryDisposition`

| Invariant | Status | Evidence / reason |
|---|---|---|
| `GenericTriggerAndDeliveryBecomeMctProduct` | COVERED | Parts A/B implement kernel authority, resident scheduling, Watch evidence, and ordinary delivery without a `patinaMother` runtime dependency. |
| `WatchAndCallAuthorityRemainKernel` | COVERED | Separate Watch, directory-read, keyvalue, observability, Child, route, and call-out evaluations. |
| `WatchApplicationMeaningRemainsChildMeaning` | COVERED | Source-derived watcher retains scan/diff/filter behavior; Mother supplies read-only mechanics and deterministic evidence only. |
| `LegacyAbsolutePathSemanticsAreRejected` | COVERED | Source patch narrows both slots and host validation refuses mismatches/unsafe values. |
| `LegacyAbiShapeRequiresValidatedNarrowing` | COVERED | Fixture provenance test, exact 0.1.x validator, and mismatch refusal test. |
| `DeprecatedSlotCannotPropagate` | COVERED | Successor contract validation disallows the slot; compatibility dispatch is exact-version only. |

### Part B interdicts and observation composition

| Slot / obligation | Status | Evidence / reason |
|---|---|---|
| `MotherEventSourceAdapterRuntime` | DEFERRED | No native watcher task, source registration, or Mother-trigger lookup exists. |
| `RegistrySyncTriggerComposition` | DEFERRED | No unattended registry sync path was added. |
| `NetworkArtifactAcquisitionAdapter` | DEFERRED | No network acquisition adapter was added; remains coupled to registry-sync composition. |
| Existing observation vocabulary only | COVERED | `watch_delivery_observation_mapping_uses_existing_kinds`; `crates/mct-kernel/src/observation.rs` remains unchanged from `20941a4`. |

## Retained pre-Slice3 full-coverage status summary

The historical counts below are retained for the pre-Slice3 inventory. The acquisition rows above are the additive, explicit disposition for this slice and are not folded into these historical totals.

### Named contract invariants (228 pre-Slice3 total)

| Status | Invariants |
|---|---:|
| COVERED | 205 |
| GAP | 0 |
| LAW-LEADS-CODE | 0 |
| DEFERRED | 23 |

### Tool-derived structural obligations

| Spec | Obligations | Bulk ledger rows | Status |
|---|---:|---:|---|
| `mct-product-map.allium` | 179 (`entity_fields` 56, `entity_optional` 38, `surface_actor` 27, `surface_exposure` 27, `value_equality` 29, `when_presence` 2) | 14 | COVERED |
| `mct-peer-ontology.allium` | 0 | 0 | COVERED (no emitted structural obligations) |

Baseline inventory check: 228 pre-Slice3 product-map/peer-ontology invariants parsed, 165 full-inventory rows plus the retained slice-1 priority names, with the 2026-07-12 evidence preserved and the five W2 invariant rows added. Slice3 acquisition attribution is maintained in the additive tables above.

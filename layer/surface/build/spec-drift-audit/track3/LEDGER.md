# Priority contract obligation ledger

Status date: 2026-07-12  
Scope: Track 3 slice 1 priority contracts only. Full-map propagation is intentionally deferred.

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
| `entity-fields.MctHelloResponse`, response optional fields, response projection actor/exposure | COVERED | `mct_iroh::tests::local_iroh_completes_mct_hello_then_call`; `mct_daemon_bin::resident::tests::hello_response_capability_view_refreshes_surfaces_on_caller` |
| `value-equality.MctCallPayloadHandle`, `entity-fields.MctCallPayloadHandle` | COVERED | `mct_kernel::call::tests::payload_integrity_decisions_cover_request_mismatch_classes`; `mct_kernel::call::tests::payload_integrity_decisions_cover_local_content_addressed_blob` |
| `entity-fields.MctCallProtocolRequest`, `entity-optional.MctCallProtocolRequest.idempotency_key`, request projection actor/exposure | COVERED | `mct_kernel::call::tests::call_protocol_json_edge_roundtrips_and_rejects_malformed`; `mct_daemon_bin::resident::tests::resident_idempotency_replays_scopes_refuses_and_expires_without_payload_leakage` |
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
| `MatchingCompletedRetryReplaysRecordedReply` | COVERED | `mct_daemon_bin::resident::tests::resident_idempotency_replays_scopes_refuses_and_expires_without_payload_leakage`; `mct_daemon_bin::ingress::tests::standalone_serve_process_persists_hello_and_call_lifecycle` |
| `IdempotencyFingerprintMustMatch` | COVERED | `mct_kernel::call::tests::idempotency_decision_replays_matches_and_refuses_other_cases`; `mct_daemon::state::tests::idempotency_store_scopes_reserves_replays_expires_and_survives_reopen` |
| `IdempotencyBoundsRefuseRatherThanEvict` | COVERED | `mct_kernel::call::tests::idempotency_decision_replays_matches_and_refuses_other_cases`; `mct_daemon::state::tests::idempotency_store_scopes_reserves_replays_expires_and_survives_reopen` |
| `CurrentIdempotencyEntryNeverSilentlyReexecutes` | COVERED | `mct_daemon_bin::resident::tests::resident_idempotency_replays_scopes_refuses_and_expires_without_payload_leakage`; `mct_daemon_bin::resident::tests::in_flight_idempotency_duplicate_refuses_without_second_execution` |
| `InFlightDuplicateIsRefused` | COVERED | `mct_daemon_bin::resident::tests::in_flight_idempotency_duplicate_refuses_without_second_execution` |
| `IdempotencyStateSurvivesRestart` | COVERED | `mct_daemon::state::tests::idempotency_store_scopes_reserves_replays_expires_and_survives_reopen` |
| `CurrentAuthorityPrecedesReplay` — revocation | COVERED | `mct_daemon_bin::resident::tests::resident_mother_payload_roundtrip_verifies_result_digest` |
| `CurrentAuthorityPrecedesReplay` — expiry and narrowed Vision | COVERED | `mct_daemon_bin::resident::tests::resident_mother_payload_roundtrip_verifies_result_digest` records one keyed success, then proves identical retries after expiry and Vision narrowing are denied without cached payload. |
| `CurrentAuthorityPrecedesReplay` — narrowed ALPN | DEFERRED | Persisted peer bindings currently expose the fixed `mct/hello/0` + `mct/call/0` protocol scope and have no operator ALPN-narrowing surface. A replay test requires the future configurable binding-scope model; inventing that authority surface is outside propagation. Current call-time ALPN revalidation remains covered by `mct_iroh::tests::call_rechecks_narrowed_alpn_scope_after_hello`. |
| `CrossMotherReplayRequiresFederationContract` | COVERED | `mct_daemon::state::tests::idempotency_store_scopes_reserves_replays_expires_and_survives_reopen` proves caller/store isolation; `mct_daemon_bin::resident::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` uses separate Mother stores. |

### Payload integrity and local CAS

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `MctCallProtocol.PayloadBytesAreBoundedByNamedConstants` | COVERED | `mct_iroh::tests::call_frame_budget_refuses_oversized_request`; `mct_iroh::tests::call_payload_caps_fail_closed`; `mct_iroh::tests::caller_rejects_reply_digest_mismatch_and_oversized_result`; `mct_daemon::blob_store::tests::ingest_rejects_oversized_input_before_visibility` |
| `MctCallProtocol.PayloadIntegrityIsVerifiedAtIngress` | COVERED | `mct_iroh::tests::call_payload_integrity_failures_are_malformed_before_authority`; `mct_daemon_bin::resident::tests::resident_mother_payload_roundtrip_verifies_result_digest` |
| `MctPayloadIntegrityAndLocalCas.ExactSizeAndDigestBeforeUse` | COVERED | `mct_kernel::call::tests::payload_integrity_decisions_cover_request_mismatch_classes`; `mct_kernel::call::tests::payload_integrity_decisions_cover_reply_result_mismatch_classes` |
| `StorageIsBoundedByNamedConstant` | COVERED | `mct_daemon::blob_store::tests::ingest_rejects_oversized_input_before_visibility` |
| `VerifyThenAtomicallyPublish` | COVERED | `mct_daemon::blob_store::tests::ingest_rejects_digest_mismatch_without_visible_blob`; `mct_daemon_bin::control::tests::resident_blob_ingest_observes_success_and_typed_rejections_without_bytes` |
| `FailedVerificationIsNotVisible` | COVERED | `mct_daemon::blob_store::tests::ingest_rejects_digest_mismatch_without_visible_blob`; `mct_daemon_bin::control::tests::resident_blob_append_failure_leaves_no_visible_cas_object` |

### Routing, effect guard, and no-route

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `TwoPhaseRouting.AuthorityPrecedesOptimization` | COVERED | `mct_daemon_bin::resident::tests::resident_route_optimization_cannot_grant_authority` |
| `PlannerRanksOnlyAdmissibleRoutes` | COVERED | `mct_kernel::route::tests::route_decision_records_selected_candidate_and_authority_evidence`; `mct_daemon_bin::resident::tests::resident_route_optimization_cannot_grant_authority` |
| `OptimizationCannotGrantAuthority` | COVERED | `mct_daemon_bin::resident::tests::resident_route_optimization_cannot_grant_authority` |
| `DenyReasonsArePolicyReasons` | COVERED | `mct_daemon_bin::resident::tests::resident_no_route_records_specific_elimination`; `mct_kernel::route::tests::candidate_elimination_reasons_expose_denial_class` |
| `ExecutionRevalidatesAuthority` | COVERED | `mct_kernel::route::tests::route_revalidation_denies_stale_policy_before_execution`; `mct_daemon_bin::resident::tests::two_mother_forwarding_denies_when_executor_revokes_binding_after_hello` |
| `EffectBoundaryRevisionGuardIsDistinct` | COVERED | `mct_daemon_bin::resident::tests::resident_route_revision_guard_denies_before_effect` |
| `EffectBoundaryGuardCannotRepairStaleAuthority` | COVERED | `mct_daemon_bin::resident::tests::resident_route_revision_guard_denies_before_effect` |
| `PeerEgressAndLocalChildEffectGuardsAreDistinct` | COVERED | `mct_daemon_bin::resident::tests::two_mother_forwarding_denies_when_executor_revokes_binding_after_hello`; `mct_daemon_bin::resident::tests::resident_route_revision_guard_denies_before_effect` |
| `NoRouteDecision.DenyByDefault` | COVERED | `mct_kernel::route::tests::no_route_decision_denies_by_default_without_route_taken` |
| `RetryRequiresPolicy`, `GrantRequestRequiresAuthority`, `GrantResponsesAreScopedAndBounded`, `NoSilentEscalation` | COVERED | `mct_kernel::route::tests::no_route_decision_denies_by_default_without_route_taken` proves the current passive default. Active retry/grant/escalation paths remain explicitly deferred under audit C3. |
| `SafeRequesterDisclosure` | COVERED | `mct_kernel::route::tests::candidate_elimination_reasons_expose_denial_class`; `mct_daemon_bin::resident::tests::resident_no_route_records_specific_elimination` |

### Caller-safe route projection

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `MctCallProtocol.RouteTakenReplyPresenceFollowsExecution` | COVERED | `mct_kernel::call::tests::reply_validation_enforces_route_taken_presence_rule`; `mct_daemon_bin::resident::tests::route_taken_projection_follows_outcome_matrix`; `mct_daemon_bin::resident::tests::cancelled_result_and_reply_hide_route_while_ledger_keeps_selection` |
| `RouteTakenReplyDoesNotGrantPeerAuthority` | COVERED | `mct_kernel::call::tests::call_without_admitted_hello_is_denied`; `mct_kernel::call::tests::call_protocol_reply_roundtrips_route_taken_wire_field` |

### Signed binding proof and mandatory expiry

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `MctIrohPeerBindingAuthority.EveryPeerBindingIsTimeBounded` | COVERED | `mct_kernel::peer::tests::binding_without_expiry_fails_closed`; `mct_kernel::peer::tests::active_binding_past_expiry_is_denied`; `mct_iroh::tests::call_rechecks_binding_expiry_after_hello` |
| `BindingProofCoversCanonicalDirectionalRecord` | COVERED | `mct_iroh::identity::tests::peer_binding_signature_ref_roundtrips_and_fails_on_tamper` |
| `InvalidBindingProofFailsClosed` | COVERED | `mct_iroh::tests::concurrent_serve_requires_signed_peer_binding_when_configured`; `mct_daemon_bin::resident::tests::resident_mother_rejects_unsigned_peer_binding`; `mct_daemon_bin::resident::tests::resident_remote_route_candidates_reject_unsigned_peer_binding` |
| `ProofDoesNotBecomeRelationshipOntology` | COVERED | `mct_daemon_bin::resident::tests::resident_remote_surface_candidate_becomes_admissible_when_all_checks_pass` requires proof plus independent authority/publication/reachability facts. |

### Observation durability and coverage

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `MctHelloProtocol.HelloObservationsBeforeEffects` | COVERED | `mct_daemon_bin::resident::tests::resident_hello_observations_are_durable_before_responses`; `mct_iroh::tests::failed_hello_observation_prevents_response_and_remembered_admission` |
| `MctLocalFirstObservationLedger.AuthorityFactsAreDurableBeforeEffect` | COVERED | `mct_iroh::tests::denied_call_fact_is_recorded_before_reply`; `mct_daemon_bin::control::tests::live_child_authority_mutations_are_durable_before_config_effect`; `mct_daemon_bin::control::tests::resident_append_failure_prevents_peer_config_effect` |
| `MctObservabilitySpine.AuthorityDecisionsAreObserved` | COVERED | `mct_kernel::observation::tests::kernel_denial_evaluations_become_observations`; `mct_daemon_bin::control::tests::live_uds_peer_mutations_are_durable_and_secret_free`; `mct_daemon_bin::control::tests::live_toy_grants_and_composition_state_are_observed_before_effects` |
| `MctObservabilitySpine.AdapterEffectsAreObserved` | COVERED | `mct_iroh::tests::iroh_adapter_observations_cover_endpoint_and_protocol_events`; `mct_daemon::process::tests::process_harness_timeout_returns_typed_result_and_observation`; `mct_daemon::toy::tests::toy_backend_failure_is_adapter_observation_not_kernel_denial` |

## Peer-ontology priority contracts

### Role and candidacy derivation

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `PeerRelationshipTaxonomy.RolesAreCurrentProjections` | COVERED | `mct_daemon_bin::resident::tests::eligible_route_candidate_requires_every_current_conjunct` mutates current facts independently and immediately loses candidacy. |
| `PeerOperationalRoleDerivation.EligibleRouteCandidateDerivation` | COVERED | `mct_daemon_bin::resident::tests::eligible_route_candidate_requires_every_current_conjunct` proves the positive candidate and independently removes local admission, reverse admission/proof, fresh publication, Vision agreement, call scope, and ticket reachability. |
| `SelectedExecutorDerivation` | COVERED | `mct_daemon_bin::resident::tests::two_mother_forwarding_denies_when_executor_revokes_binding_after_hello` revalidates after selection and before egress. |
| `OperatorPointedSubmissionIsDistinct` | COVERED | `mct_daemon_bin::ingress::tests::operator_pointed_egress_is_durable_before_send` proves the operator decision is durable before the receiver observes `mct/call/0`; both manual CLI paths use the same recording boundary. |

### Terminal peer submission

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `SubmissionProposesReceiverAsLocalExecutor` | COVERED | `mct_daemon_bin::resident::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` |
| `PeerArrivalsDoNotSourceAnotherPeer` | COVERED | `mct_daemon_bin::resident::tests::forwarded_arrival_with_unavailable_local_candidate_is_terminal`; `mct_daemon_bin::resident::tests::two_mother_mutual_publication_with_unready_children_terminates_single_hop` |
| `OriginDoesNotChangeAuthority` | COVERED | `mct_kernel::call::tests::only_local_call_origins_allow_remote_candidate_sourcing` together with terminal-arrival tests proves origin dispatches the submitted protocol meaning rather than bypassing authority. |
| `PublicationReferenceIsNotRequiredForTerminality` | COVERED | `mct_daemon_bin::resident::tests::forwarded_arrival_with_unavailable_local_candidate_is_terminal` uses no publication reference in the arriving envelope. |

### Per-hop accountability

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `SubmissionIsTheImmediateCallersAct` | COVERED | `mct_daemon_bin::resident::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` records `forwarded_from:mother-a` at the executor. |
| `ReceiverTrustsOnlyLocallyVerifiableAuthority` | COVERED | `mct_daemon_bin::resident::tests::two_mother_forwarding_denies_when_executor_revokes_binding_after_hello` |
| `UpstreamIdentityRemainsAtItsVerifier` | COVERED | `mct_daemon_bin::resident::tests::forwarded_envelope_clears_upstream_user_identity` proves the original verifier retains its user fact while the per-hop envelope carries only the submitting Mother and correlation IDs. |
| `BilateralAuditUsesCorrelationNotIdentityPropagation` | COVERED | `mct_daemon_bin::resident::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` checks both ledgers and the shared route chain. |
| `ObservationReplicationIsTheSharingChannel` | DEFERRED | `mct/observe/0` and ObservationReplicationAuthorization are audit C4 future scope; current call envelopes grant no observation access. |
| `BrokeredIdentityBelongsToBrokeredSubmission` | DEFERRED | Brokered submission is explicitly future law and cannot be implemented by this terminal `mct/call/0` slice. |

### Bilateral executable routing

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `TwoExposuresRequireTwoSovereignConsents` | COVERED | `mct_daemon_bin::resident::tests::resident_remote_surface_candidate_becomes_admissible_when_all_checks_pass`; missing terms are consolidated by the candidacy GAP above. |
| `MutualAdmissionPreventsExternallyWritableRouting` | COVERED | `mct_daemon_bin::resident::tests::resident_remote_route_candidates_reject_unsigned_peer_binding`; `mct_daemon_bin::resident::tests::two_mother_unauthorized_operation_fails_closed` |
| `OneWayStatesRemainMeaningful` | COVERED | `mct_daemon_bin::resident::tests::eligible_route_candidate_requires_every_current_conjunct` independently removes each directional admission. |
| `BilateralStateIsDerivedNotStored` | COVERED | `mct_daemon_bin::resident::tests::eligible_route_candidate_requires_every_current_conjunct` derives candidacy from current directional records rather than a stored pair state. |
| `EitherDirectionEndsCandidacyImmediately` | COVERED | `mct_daemon_bin::resident::tests::two_mother_revoked_or_expired_binding_fails_closed`; `mct_daemon_bin::resident::tests::two_mother_forwarding_denies_when_executor_revokes_binding_after_hello` |
| `ReachabilityIsNotAuthority` | COVERED | `mct_daemon_bin::resident::tests::eligible_route_candidate_requires_every_current_conjunct` proves publication plus reachability cannot replace either consent and omission of the ticket removes candidacy. |

### Capability publication

| Invariant / obligation | Status | Evidence |
|---|---|---|
| `CapabilityPublicationRelationship.HonestLocalExecutionOffer` | COVERED | `mct_daemon::federation::tests::honest_local_execution_offer_excludes_approved_assigned_non_ready_child` proves approval and assignment cannot publish a non-ready instance. |
| `AdvertisementNeverGrantsAuthority` | COVERED | `mct_daemon_bin::resident::tests::eligible_route_candidate_requires_every_current_conjunct` retains fresh publication while independently removing each directional consent. |
| `OfferIsVisionScoped` | COVERED | `mct_daemon::federation::tests::federation_view_is_vision_scoped`; `mct_daemon_bin::resident::tests::two_mother_wrong_vision_fails_closed` |
| `OfferLapsesAtFreshnessBoundary` | COVERED | `mct_daemon_bin::resident::tests::capability_offer_lapses_at_freshness_boundary` proves candidacy is absent exactly at and after `stale_at`. |

## Current status summary

| Status | Rows |
|---|---:|
| COVERED | 73 |
| GAP | 0 |
| LAW-LEADS-CODE | 0 |
| DEFERRED | 3 |

S2 resolved both mandatory-expiry rows through one contract change and the operator-pointed egress row through a shared before-effect recording boundary. Every S3 GAP is now covered except narrowed-ALPN replay, which is explicitly deferred until peer binding scope becomes configurable.

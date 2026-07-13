# Contract obligation ledger

Status date: 2026-07-12  
Scope: complete named-invariant coverage for `mct-product-map.allium` and `mct-peer-ontology.allium`, plus bulk attribution of tool-derived structural obligations. Slice 1's priority rows are retained in place and extended below.

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
| `MatchingCompletedRetryReplaysRecordedReply` | COVERED | `mct_daemon_bin::resident::idempotency::tests::resident_idempotency_replays_scopes_refuses_and_expires_without_payload_leakage`; `mct_daemon_bin::ingress::tests::standalone_serve_process_persists_hello_and_call_lifecycle` |
| `IdempotencyFingerprintMustMatch` | COVERED | `mct_kernel::call::tests::idempotency_decision_replays_matches_and_refuses_other_cases`; `mct_daemon::state::tests::idempotency_store_scopes_reserves_replays_expires_and_survives_reopen` |
| `IdempotencyBoundsRefuseRatherThanEvict` | COVERED | `mct_kernel::call::tests::idempotency_decision_replays_matches_and_refuses_other_cases`; `mct_daemon::state::tests::idempotency_store_scopes_reserves_replays_expires_and_survives_reopen` |
| `CurrentIdempotencyEntryNeverSilentlyReexecutes` | COVERED | `mct_daemon_bin::resident::idempotency::tests::resident_idempotency_replays_scopes_refuses_and_expires_without_payload_leakage`; `mct_daemon_bin::resident::idempotency::tests::in_flight_idempotency_duplicate_refuses_without_second_execution` |
| `InFlightDuplicateIsRefused` | COVERED | `mct_daemon_bin::resident::idempotency::tests::in_flight_idempotency_duplicate_refuses_without_second_execution` |
| `IdempotencyStateSurvivesRestart` | COVERED | `mct_daemon::state::tests::idempotency_store_scopes_reserves_replays_expires_and_survives_reopen` |
| `CurrentAuthorityPrecedesReplay` — revocation | COVERED | `mct_daemon_bin::resident::serving::tests::resident_mother_payload_roundtrip_verifies_result_digest` |
| `CurrentAuthorityPrecedesReplay` — expiry and narrowed Vision | COVERED | `mct_daemon_bin::resident::serving::tests::resident_mother_payload_roundtrip_verifies_result_digest` records one keyed success, then proves identical retries after expiry and Vision narrowing are denied without cached payload. |
| `CurrentAuthorityPrecedesReplay` — narrowed ALPN | DEFERRED | Persisted peer bindings currently expose the fixed `mct/hello/0` + `mct/call/0` protocol scope and have no operator ALPN-narrowing surface. A replay test requires the future configurable binding-scope model; inventing that authority surface is outside propagation. Current call-time ALPN revalidation remains covered by `mct_iroh::tests::call_rechecks_narrowed_alpn_scope_after_hello`. |
| `CrossMotherReplayRequiresFederationContract` | COVERED | `mct_daemon::state::tests::idempotency_store_scopes_reserves_replays_expires_and_survives_reopen` proves caller/store isolation; `mct_daemon_bin::resident::forwarding::tests::two_mother_forwards_selected_call_over_iroh_and_maps_reply` uses separate Mother stores. |

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
| `MctCallProtocol.RouteTakenReplyPresenceFollowsExecution` | COVERED | `mct_kernel::call::tests::reply_validation_enforces_route_taken_presence_rule`; `mct_daemon_bin::resident::execution::tests::route_taken_projection_follows_outcome_matrix`; `mct_daemon_bin::resident::execution::tests::cancelled_result_and_reply_hide_route_while_ledger_keeps_selection` |
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
| `MctLocalFirstObservationLedger.AuthorityFactsAreDurableBeforeEffect` | COVERED | `mct_iroh::tests::denied_call_fact_is_recorded_before_reply`; `mct_daemon_bin::control::tests::live_child_authority_mutations_are_durable_before_config_effect`; `mct_daemon_bin::control::tests::resident_append_failure_prevents_peer_config_effect` |
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

This slice extends the priority ledger in place. Every one of the 223 named contract invariants is now present either in the original sections above or in the rows below. Statuses remain obligation-specific where slice 1 split one invariant into independently testable edges.

The 236 load-bearing `-- Decision:` statements were also read in full and grouped by their adjacent contract/model clusters: 217 statements in 26 product-map clusters and 19 statements in 6 peer-ontology clusters. Their executable obligations are attributed through the named invariant rows; unresolved product decisions remain explicit `DEFERRED` rows rather than invented authority surfaces.

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
| `ClosedOutcomeSet` | LAW-LEADS-CODE | The expected-red probe `cancelled_result_projection_preserves_cancelled_outcome`, captured verbatim in `TASKS.md` and removed after the structural stop, proves resident result projection collapses `ResultOutcome::Cancelled` to `CallProtocolOutcome::Failed`. The route-presence helper covers all five result variants, but the actual consumer does not preserve cancellation. Fix is structurally blocked because `MctCallProtocolEvaluation.outcome` and Rust `CallProtocolOutcome` have no cancelled variant while `MctCallProtocolReply` does; operator adjudication is required before changing the model or wire. |
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
| `ResidentOwnsLiveMutations` | COVERED | `mct_daemon_bin::control::tests::live_child_authority_mutations_are_durable_before_config_effect` |
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
| `ResultCoverage` | LAW-LEADS-CODE | Triage rule applied: landed behavior was exercised through the real resident result consumer. The expected-red probe `cancelled_result_projection_preserves_cancelled_outcome`, captured verbatim in `TASKS.md` and removed after the structural stop, shows cancellation becomes failure before the Iroh evaluation/result observation path. This is the same structural cancelled-outcome mismatch as `MctResultTerminality.ClosedOutcomeSet`, so gap filling stops for operator adjudication rather than adding a misleading matrix. |
| `ChildLifecycleCoverage` | GAP | Lifecycle tests cover reload order, but no named test proves the complete artifact/approval/assignment/instance observation matrix. |
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
| `BufferedEffectsAreBounded` | GAP | The resident writer uses a bounded 256-entry channel, but no named test proves bounded buffering/backpressure behavior. |
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
| `LifecycleTransitionsAreObserved` | GAP | Current tests do not assert every artifact, authority, and instance lifecycle observation kind. |

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
| `ToyGrantDecisionsAreObserved` | GAP | The existing observation test proves allow and generic deny only, not expired and revoked typed facts. |
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

## Full-coverage status summary

### Named contract invariants (223 total)

| Status | Invariants |
|---|---:|
| COVERED | 194 |
| GAP | 4 |
| LAW-LEADS-CODE | 2 |
| DEFERRED | 23 |

### Tool-derived structural obligations

| Spec | Obligations | Bulk ledger rows | Status |
|---|---:|---:|---|
| `mct-product-map.allium` | 179 (`entity_fields` 56, `entity_optional` 38, `surface_actor` 27, `surface_exposure` 27, `value_equality` 29, `when_presence` 2) | 14 | COVERED |
| `mct-peer-ontology.allium` | 0 | 0 | COVERED (no emitted structural obligations) |

Inventory check: 223 invariants parsed, 160 rows added, 63 invariant names retained from slice 1.

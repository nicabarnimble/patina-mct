//! Resident capability publication and admitted-hello surface refresh.

use super::*;

pub(super) fn resident_hello_capability_view(
    config: &mct_daemon::MctDaemonConfig,
    summary: &mct_daemon::MctRuntimeStateSummary,
    identity: &MctLocalNodeIdentity,
    children: &[mct_daemon::MctLoadedChild],
) -> MctHelloCapabilityView {
    let federation_view = build_federation_capability_view_with_children(
        config,
        summary,
        identity.node_id.clone(),
        identity.vision_id.clone(),
        children.iter(),
    );
    hello_capability_view_from_federation_view(&federation_view)
}

pub(crate) fn local_hello_capability_view_from_config(
    config: &mct_daemon::MctDaemonConfig,
    state_path: &Path,
    children_dir: &Path,
) -> Result<Option<MctHelloCapabilityView>> {
    let Some(identity) = config.local_identity.as_ref() else {
        return Ok(None);
    };
    let state = MctRuntimeStateStore::open(state_path)?;
    let summary = state.summary()?;
    let load_report = load_children_from_dir(MctChildLoadOptions::new(children_dir.to_path_buf()));
    Ok(Some(resident_hello_capability_view(
        config,
        &summary,
        identity,
        &load_report.children,
    )))
}

pub(super) fn remote_surface_stale_at(received_at: &Timestamp) -> Result<Timestamp> {
    let received = received_at
        .as_str()
        .parse::<jiff::Timestamp>()
        .context("parse remote surface received_at")?;
    let stale = received
        .checked_add(jiff::SignedDuration::from_secs(300))
        .context("compute remote surface stale_at")?;
    Timestamp::new(stale.to_string()).context("encode remote surface stale_at")
}

pub(super) fn refresh_remote_surfaces_from_admitted_hello_request(
    state_path: &Path,
    request: &MctHelloRequest,
    evaluation: &MctHelloAdmissionEvaluation,
    received_at: Timestamp,
) -> Result<bool> {
    if !evaluation.is_admitted() {
        return Ok(false);
    }
    let Some(view) = request.capability_view.as_ref() else {
        return Ok(false);
    };
    if evaluation.selected_node_id.as_ref() != Some(&view.node_id)
        || evaluation.selected_vision_id.as_ref() != Some(&view.vision_id)
    {
        return Ok(false);
    }
    let Some(binding_id) = evaluation.selected_binding_id.as_ref() else {
        return Ok(false);
    };
    let stale_at = remote_surface_stale_at(&received_at)?;
    let state = MctRuntimeStateStore::open(state_path)?;
    state.refresh_remote_callable_surfaces(MctRemoteSurfaceRefresh {
        peer_node_id: &view.node_id,
        binding_id,
        endpoint_id: &request.received_over.endpoint_id,
        view,
        received_at: &received_at,
        stale_at: &stale_at,
        view_observation_id: &evaluation.observation_id,
    })?;
    Ok(true)
}

pub(crate) fn refresh_remote_surfaces_from_admitted_hello_response(
    state_path: &Path,
    peer: &MctPeerAddressBookEntry,
    response: &MctHelloResponse,
    received_at: Timestamp,
) -> Result<bool> {
    if response.hello_outcome != HelloOutcome::Admitted {
        return Ok(false);
    }
    let Some(view) = response.capability_view.as_ref() else {
        return Ok(false);
    };
    if view.node_id != peer.peer_node_id || view.vision_id != peer.vision_id {
        return Ok(false);
    }
    let stale_at = remote_surface_stale_at(&received_at)?;
    let state = MctRuntimeStateStore::open(state_path)?;
    state.refresh_remote_callable_surfaces(MctRemoteSurfaceRefresh {
        peer_node_id: &peer.peer_node_id,
        binding_id: &peer.binding_id,
        endpoint_id: &peer.endpoint_id,
        view,
        received_at: &received_at,
        stale_at: &stale_at,
        view_observation_id: &response.response_observation_id,
    })?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract_peer_expiry() -> Timestamp {
        Timestamp::new("2099-01-01T00:00:00Z").unwrap()
    }

    fn resident_remote_peer_entry(
        peer_node_id: &str,
        binding_id: &str,
        endpoint_id: &str,
        vision_id: &str,
        binding_state: BindingState,
        binding_signature_ref: Option<String>,
    ) -> MctPeerAddressBookEntry {
        MctPeerAddressBookEntry {
            peer_node_id: MctNodeId::new(peer_node_id)
                .expect("string ID literal/generated value must be non-empty"),
            binding_id: PeerBindingId::new(binding_id)
                .expect("string ID literal/generated value must be non-empty"),
            endpoint_id: EndpointIdText::new(endpoint_id)
                .expect("string ID literal/generated value must be non-empty"),
            vision_id: VisionId::new(vision_id)
                .expect("string ID literal/generated value must be non-empty"),
            ticket: Some(MotherIrohEndpointTicket {
                endpoint_id: EndpointIdText::new(endpoint_id)
                    .expect("string ID literal/generated value must be non-empty"),
                direct_addresses: vec!["127.0.0.1:12345".into()],
                relay_urls: Vec::new(),
            }),
            binding_signature_ref,
            outbound_binding: None,
            binding_state,
            policy_revision: 1,
            expires_at: contract_peer_expiry(),
            updated_at: "2026-07-09T00:00:00Z".into(),
        }
    }
    fn hello_capability_view(
        node_id: &MctNodeId,
        vision_id: &VisionId,
        policy_revision: u64,
        operations: &[&str],
    ) -> MctHelloCapabilityView {
        MctHelloCapabilityView {
            node_id: node_id.clone(),
            vision_id: vision_id.clone(),
            published_at: Timestamp::new("2026-07-09T00:00:00Z").unwrap(),
            policy_revision,
            supported_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            supported_wit_worlds: vec!["patina:demo/control@0.1.0".into()],
            supported_observation_modes: vec!["local-ledger".into()],
            callable_surfaces: operations
                .iter()
                .map(|operation| MctHelloCallableSurface {
                    child_name: "remote-child".into(),
                    operation_id: (*operation).into(),
                    runtime_kind: RuntimeKind::WasmComponent,
                    vision_id: vision_id.clone(),
                    policy_revision,
                    visibility: "vision_scoped".into(),
                })
                .collect(),
            capability_view_ref: None,
        }
    }
    fn hello_request_with_surface_view(
        node_id: &MctNodeId,
        vision_id: &VisionId,
        binding_id: &PeerBindingId,
        endpoint_id: &EndpointIdText,
        policy_revision: u64,
        operations: &[&str],
    ) -> MctHelloRequest {
        MctHelloRequest {
            hello_id: "hello-remote-surface".into(),
            received_over: IrohConnectionPresentation {
                endpoint_id: endpoint_id.clone(),
                alpn: MCT_HELLO_ALPN.into(),
                connection_side: ConnectionSide::Incoming,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            requested_protocol: HelloPolicy::default().protocol,
            requested_vision_id: Some(vision_id.clone()),
            requested_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            presented_binding: MctPeerBindingPresentation {
                binding_id: Some(binding_id.clone()),
                endpoint_id: endpoint_id.clone(),
                mct_node_id: Some(node_id.clone()),
                vision_id: Some(vision_id.clone()),
                policy_revision: Some(1),
                allowed_alpns_claim: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
                signature_ref: None,
                expires_at: None,
            },
            capability_view: Some(hello_capability_view(
                node_id,
                vision_id,
                policy_revision,
                operations,
            )),
            local_policy_revision_seen: Some(1),
            trace_id: TraceId::new("trace-remote-surface")
                .expect("string ID literal/generated value must be non-empty"),
            received_observation_id: ObservationId::new("obs-remote-surface-received")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }
    fn admitted_hello_evaluation(
        request: &MctHelloRequest,
        node_id: &MctNodeId,
        vision_id: &VisionId,
        binding_id: &PeerBindingId,
    ) -> MctHelloAdmissionEvaluation {
        MctHelloAdmissionEvaluation {
            decision_id: DecisionId::new("decision-remote-surface")
                .expect("string ID literal/generated value must be non-empty"),
            request_id: request.hello_id.clone(),
            peer_admission_decision_id: None,
            selected_binding_id: Some(binding_id.clone()),
            selected_node_id: Some(node_id.clone()),
            selected_vision_id: Some(vision_id.clone()),
            selected_policy_revision: Some(1),
            negotiated_protocol: Some(HelloPolicy::default().protocol),
            accepted_alpns: vec![MCT_CALL_ALPN.into()],
            hello_outcome: HelloOutcome::Admitted,
            reason: HelloReason::ActiveBinding,
            safe_reason: SafeHelloReason::Admitted,
            observation_id: ObservationId::new("obs-remote-surface-admitted")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }
    #[test]
    fn admitted_hello_refreshes_peer_callable_surfaces() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        let remote_node = MctNodeId::new("remote-mct")
            .expect("string ID literal/generated value must be non-empty");
        let vision_id = VisionId::new("vision-local")
            .expect("string ID literal/generated value must be non-empty");
        let binding_id = PeerBindingId::new("binding-remote")
            .expect("string ID literal/generated value must be non-empty");
        let endpoint_id = EndpointIdText::new("endpoint-remote")
            .expect("string ID literal/generated value must be non-empty");
        let received_at = Timestamp::new("2026-07-09T00:00:00Z").unwrap();
        let request = hello_request_with_surface_view(
            &remote_node,
            &vision_id,
            &binding_id,
            &endpoint_id,
            11,
            &["patina:demo/control@0.1.0.run"],
        );
        let evaluation = admitted_hello_evaluation(&request, &remote_node, &vision_id, &binding_id);

        assert!(
            refresh_remote_surfaces_from_admitted_hello_request(
                &state_path,
                &request,
                &evaluation,
                received_at.clone(),
            )
            .unwrap()
        );
        let surfaces = state
            .remote_callable_surfaces(&remote_node, &vision_id)
            .unwrap();
        assert_eq!(surfaces.len(), 1);
        assert_eq!(surfaces[0].operation_id, "patina:demo/control@0.1.0.run");
        assert_eq!(surfaces[0].publisher_policy_revision, 11);
        assert_eq!(surfaces[0].stale_at.as_str(), "2026-07-09T00:05:00Z");

        let refreshed = hello_request_with_surface_view(
            &remote_node,
            &vision_id,
            &binding_id,
            &endpoint_id,
            12,
            &["patina:demo/control@0.1.0.other"],
        );
        refresh_remote_surfaces_from_admitted_hello_request(
            &state_path,
            &refreshed,
            &evaluation,
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
        )
        .unwrap();
        let surfaces = state
            .remote_callable_surfaces(&remote_node, &vision_id)
            .unwrap();
        assert_eq!(surfaces.len(), 1);
        assert_eq!(surfaces[0].operation_id, "patina:demo/control@0.1.0.other");
        assert_eq!(surfaces[0].publisher_policy_revision, 12);
    }

    #[test]
    fn hello_response_capability_view_refreshes_surfaces_on_caller() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        let peer = resident_remote_peer_entry(
            "remote-mct",
            "binding-remote",
            "endpoint-remote",
            "vision-local",
            BindingState::Admitted,
            None,
        );
        let view = hello_capability_view(
            &peer.peer_node_id,
            &peer.vision_id,
            4,
            &["patina:demo/control@0.1.0.run"],
        );
        let response = MctHelloResponse {
            response_id: "response-remote".into(),
            request_id: "hello-local".into(),
            decision_id: DecisionId::new("decision-response-remote")
                .expect("string ID literal/generated value must be non-empty"),
            hello_outcome: HelloOutcome::Admitted,
            negotiated_protocol: Some(HelloPolicy::default().protocol),
            accepted_alpns: vec![MCT_CALL_ALPN.into()],
            safe_message: "admitted".into(),
            retry_after: None,
            capability_view: Some(view),
            response_observation_id: ObservationId::new("obs-response-remote")
                .expect("string ID literal/generated value must be non-empty"),
        };

        assert!(
            refresh_remote_surfaces_from_admitted_hello_response(
                &state_path,
                &peer,
                &response,
                Timestamp::new("2026-07-09T00:00:00Z").unwrap(),
            )
            .unwrap()
        );
        let surfaces = state
            .remote_callable_surfaces(&peer.peer_node_id, &peer.vision_id)
            .unwrap();
        assert_eq!(surfaces.len(), 1);
        assert_eq!(surfaces[0].operation_id, "patina:demo/control@0.1.0.run");
        assert_eq!(surfaces[0].endpoint_id, peer.endpoint_id);
    }

    #[test]
    fn denied_or_wrong_vision_hello_does_not_refresh_surfaces() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.sqlite");
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        let remote_node = MctNodeId::new("remote-mct")
            .expect("string ID literal/generated value must be non-empty");
        let vision_id = VisionId::new("vision-local")
            .expect("string ID literal/generated value must be non-empty");
        let binding_id = PeerBindingId::new("binding-remote")
            .expect("string ID literal/generated value must be non-empty");
        let endpoint_id = EndpointIdText::new("endpoint-remote")
            .expect("string ID literal/generated value must be non-empty");
        let request = hello_request_with_surface_view(
            &remote_node,
            &vision_id,
            &binding_id,
            &endpoint_id,
            1,
            &["patina:demo/control@0.1.0.run"],
        );
        let mut denied = admitted_hello_evaluation(&request, &remote_node, &vision_id, &binding_id);
        denied.hello_outcome = HelloOutcome::Denied;
        denied.selected_node_id = None;
        denied.selected_vision_id = None;
        denied.selected_binding_id = None;

        assert!(
            !refresh_remote_surfaces_from_admitted_hello_request(
                &state_path,
                &request,
                &denied,
                Timestamp::new("2026-07-09T00:00:00Z").unwrap(),
            )
            .unwrap()
        );
        let wrong_vision = VisionId::new("vision-other")
            .expect("string ID literal/generated value must be non-empty");
        let wrong_vision_request = hello_request_with_surface_view(
            &remote_node,
            &wrong_vision,
            &binding_id,
            &endpoint_id,
            1,
            &["patina:demo/control@0.1.0.run"],
        );
        let evaluation = admitted_hello_evaluation(&request, &remote_node, &vision_id, &binding_id);
        assert!(
            !refresh_remote_surfaces_from_admitted_hello_request(
                &state_path,
                &wrong_vision_request,
                &evaluation,
                Timestamp::new("2026-07-09T00:00:00Z").unwrap(),
            )
            .unwrap()
        );
        assert!(
            state
                .remote_callable_surfaces(&remote_node, &vision_id)
                .unwrap()
                .is_empty()
        );
    }
}

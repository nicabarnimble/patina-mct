use super::*;

const PEER_MUTATION_BODY_MAX_BYTES: usize = 64 * 1024;
static NEXT_PEER_MUTATION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PeerAddRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) peer_node_id: MctNodeId,
    pub(super) binding_id: PeerBindingId,
    pub(super) endpoint_id: EndpointIdText,
    pub(super) vision_id: VisionId,
    pub(super) ticket: Option<MotherIrohEndpointTicket>,
    pub(super) binding_signature_ref: Option<String>,
    pub(super) policy_revision: u64,
    pub(super) expires_at: Timestamp,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PeerProofRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) peer_node_id: MctNodeId,
    pub(super) binding_id: PeerBindingId,
    pub(super) policy_revision: u64,
    pub(super) signature_ref: String,
    pub(super) expires_at: Timestamp,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PeerNodeRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) peer_node_id: MctNodeId,
}

#[derive(Clone, Debug)]
enum PreparedPeerMutationEffect {
    Add(Box<MctPeerAddressBookEntry>),
    Proof {
        peer_node_id: MctNodeId,
        outbound: MctOutboundPeerBindingPresentation,
    },
    Revoke(MctNodeId),
    Remove(MctNodeId),
}

#[derive(Clone, Debug)]
struct PreparedPeerMutation {
    config_path: PathBuf,
    action: &'static str,
    peer_node_id: MctNodeId,
    binding_id: PeerBindingId,
    endpoint_id: EndpointIdText,
    vision_id: VisionId,
    policy_revision: u64,
    binding_state: BindingState,
    expires_at: Option<Timestamp>,
    effect: PreparedPeerMutationEffect,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub(super) struct PeerMutationSuccess {
    pub(super) action: String,
    pub(super) peer_node_id: MctNodeId,
    pub(super) binding_id: PeerBindingId,
    pub(super) endpoint_id: EndpointIdText,
    pub(super) vision_id: VisionId,
    pub(super) policy_revision: u64,
    pub(super) binding_state: BindingState,
    pub(super) expires_at: Option<Timestamp>,
    pub(super) peer_count: usize,
}

fn normalize_config_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}

fn require_expected_config_path(expected: &Path, configured: &Path) -> Result<()> {
    if normalize_config_path(expected)? != normalize_config_path(configured)? {
        bail!("peer mutation expected config path does not match resident config");
    }
    Ok(())
}

fn peer_mutation_observation(
    prepared: &PreparedPeerMutation,
    kind: ObservationKind,
    outcome: ObservationOutcome,
) -> MctObservation {
    let sequence = NEXT_PEER_MUTATION_ID.fetch_add(1, Ordering::Relaxed);
    let observed_at = current_timestamp();
    let id = format!("peer-mutation-{observed_at}-{sequence}");
    MctObservation {
        observation_id: ObservationId::new(format!("obs:{id}"))
            .expect("generated observation ID must be non-empty"),
        observed_at,
        kind,
        source_plane: if outcome == ObservationOutcome::Failed {
            SourcePlane::Operator
        } else {
            SourcePlane::Kernel
        },
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace:{id}"))
                .expect("generated trace ID must be non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: Some(
            DecisionId::new(format!("decision:{id}"))
                .expect("generated decision ID must be non-empty"),
        ),
        subject_id: Some(prepared.peer_node_id.to_string()),
        resource_id: Some(prepared.binding_id.to_string()),
        policy_revision: Some(prepared.policy_revision),
        grants_revision: None,
        outcome,
        visibility: ObservationVisibility::NodeOperator,
        safe_message: format!(
            "peer authority action={} endpoint={} vision={} state={:?} expires_at={}",
            prepared.action,
            prepared.endpoint_id,
            prepared.vision_id,
            prepared.binding_state,
            prepared
                .expires_at
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "-".into())
        ),
        detail_ref: None,
    }
}

fn prepare_peer_mutation(
    configured_path: &Path,
    path: &str,
    body: &[u8],
) -> Result<PreparedPeerMutation> {
    if body.len() > PEER_MUTATION_BODY_MAX_BYTES {
        bail!("peer mutation body exceeds 64 KiB limit");
    }
    let store = MctDaemonConfigStore::new(configured_path);
    let (
        action,
        peer_node_id,
        binding_id,
        endpoint_id,
        vision_id,
        policy_revision,
        state,
        expires_at,
        effect,
    ) = match path {
        "/peers/add" => {
            let request: PeerAddRequest =
                serde_json::from_slice(body).context("decode peer add request")?;
            require_expected_config_path(&request.expected_config_path, configured_path)?;
            if request.policy_revision == 0 {
                bail!("peer policy revision must be greater than zero");
            }
            if request
                .binding_signature_ref
                .as_ref()
                .is_some_and(|proof| proof.trim().is_empty())
            {
                bail!("peer binding signature reference must not be empty");
            }
            let _current_config = store.load()?;
            let entry = MctPeerAddressBookEntry {
                peer_node_id: request.peer_node_id.clone(),
                binding_id: request.binding_id.clone(),
                endpoint_id: request.endpoint_id.clone(),
                vision_id: request.vision_id.clone(),
                ticket: request.ticket,
                binding_signature_ref: request.binding_signature_ref,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: request.policy_revision,
                expires_at: request.expires_at.clone(),
                updated_at: mct_daemon::current_timestamp_string(),
            };
            (
                "add",
                request.peer_node_id,
                request.binding_id,
                request.endpoint_id,
                request.vision_id,
                request.policy_revision,
                BindingState::Admitted,
                Some(request.expires_at),
                PreparedPeerMutationEffect::Add(Box::new(entry)),
            )
        }
        "/peers/proof" => {
            let request: PeerProofRequest =
                serde_json::from_slice(body).context("decode peer proof request")?;
            require_expected_config_path(&request.expected_config_path, configured_path)?;
            if request.policy_revision == 0 {
                bail!("peer policy revision must be greater than zero");
            }
            if request.signature_ref.trim().is_empty() {
                bail!("outbound binding signature reference must not be empty");
            }
            let config = store.load()?;
            let peer = config
                .peers
                .get(request.peer_node_id.as_str())
                .ok_or_else(|| anyhow::anyhow!("peer not found in config"))?;
            let outbound = MctOutboundPeerBindingPresentation {
                binding_id: request.binding_id.clone(),
                policy_revision: request.policy_revision,
                signature_ref: request.signature_ref,
                expires_at: request.expires_at.clone(),
            };
            (
                "proof",
                request.peer_node_id.clone(),
                request.binding_id,
                peer.endpoint_id.clone(),
                peer.vision_id.clone(),
                request.policy_revision,
                peer.binding_state,
                Some(request.expires_at),
                PreparedPeerMutationEffect::Proof {
                    peer_node_id: request.peer_node_id,
                    outbound,
                },
            )
        }
        "/peers/revoke" | "/peers/remove" => {
            let request: PeerNodeRequest =
                serde_json::from_slice(body).context("decode peer mutation request")?;
            require_expected_config_path(&request.expected_config_path, configured_path)?;
            let config = store.load()?;
            let peer = config
                .peers
                .get(request.peer_node_id.as_str())
                .ok_or_else(|| anyhow::anyhow!("peer not found in config"))?;
            let remove = path == "/peers/remove";
            (
                if remove { "remove" } else { "revoke" },
                request.peer_node_id.clone(),
                peer.binding_id.clone(),
                peer.endpoint_id.clone(),
                peer.vision_id.clone(),
                peer.policy_revision,
                BindingState::Revoked,
                None,
                if remove {
                    PreparedPeerMutationEffect::Remove(request.peer_node_id)
                } else {
                    PreparedPeerMutationEffect::Revoke(request.peer_node_id)
                },
            )
        }
        _ => bail!("unknown peer mutation route"),
    };
    Ok(PreparedPeerMutation {
        config_path: configured_path.to_path_buf(),
        action,
        peer_node_id,
        binding_id,
        endpoint_id,
        vision_id,
        policy_revision,
        binding_state: state,
        expires_at,
        effect,
    })
}

impl PreparedPeerMutation {
    fn apply(&self) -> Result<PeerMutationSuccess> {
        let store = MctDaemonConfigStore::new(&self.config_path);
        let config = match &self.effect {
            PreparedPeerMutationEffect::Add(entry) => store.upsert_peer(entry.as_ref().clone())?,
            PreparedPeerMutationEffect::Proof {
                peer_node_id,
                outbound,
            } => store.set_peer_outbound_proof(peer_node_id, outbound.clone())?,
            PreparedPeerMutationEffect::Revoke(peer_node_id) => store.revoke_peer(peer_node_id)?,
            PreparedPeerMutationEffect::Remove(peer_node_id) => store.remove_peer(peer_node_id)?,
        };
        Ok(PeerMutationSuccess {
            action: self.action.into(),
            peer_node_id: self.peer_node_id.clone(),
            binding_id: self.binding_id.clone(),
            endpoint_id: self.endpoint_id.clone(),
            vision_id: self.vision_id.clone(),
            policy_revision: self.policy_revision,
            binding_state: self.binding_state,
            expires_at: self.expires_at.clone(),
            peer_count: config.peers.len(),
        })
    }

    fn decision_observation(&self) -> MctObservation {
        let kind = match self.effect {
            PreparedPeerMutationEffect::Add(_) | PreparedPeerMutationEffect::Proof { .. } => {
                ObservationKind::PeerBindingRecorded
            }
            PreparedPeerMutationEffect::Revoke(_) | PreparedPeerMutationEffect::Remove(_) => {
                ObservationKind::PeerBindingRevoked
            }
        };
        peer_mutation_observation(self, kind, ObservationOutcome::Allowed)
    }

    fn failure_observation(&self) -> MctObservation {
        peer_mutation_observation(
            self,
            ObservationKind::OperatorActionRecorded,
            ObservationOutcome::Failed,
        )
    }
}

fn peer_mutation_response(status_code: u16, body: serde_json::Value) -> MctControlPlaneResponse {
    MctControlPlaneResponse {
        status_code,
        content_type: "application/json".into(),
        body: body.to_string(),
    }
}

async fn execute_resident_peer_mutation(
    configured_path: &Path,
    ledger: &ResidentLedgerWriter,
    path: &str,
    body: &[u8],
) -> MctControlPlaneResponse {
    let prepared = match prepare_peer_mutation(configured_path, path, body) {
        Ok(prepared) => prepared,
        Err(_) => {
            return peer_mutation_response(
                400,
                serde_json::json!({"error": "peer mutation rejected"}),
            );
        }
    };
    if ledger
        .append(vec![prepared.decision_observation()])
        .await
        .is_err()
    {
        return peer_mutation_response(
            500,
            serde_json::json!({"error": "peer mutation decision was not durable"}),
        );
    }
    match prepared.apply() {
        Ok(response) => peer_mutation_response(
            200,
            serde_json::to_value(response)
                .expect("peer mutation response serialization must succeed"),
        ),
        Err(_) => {
            let failure_is_durable = ledger
                .append(vec![prepared.failure_observation()])
                .await
                .is_ok();
            let message = if failure_is_durable {
                "peer mutation config apply failed"
            } else {
                "peer mutation config apply failed and failure observation was not durable"
            };
            peer_mutation_response(500, serde_json::json!({"error": message}))
        }
    }
}

#[cfg(test)]
pub(super) fn resident_peer_mutation_handler(
    configured_path: PathBuf,
    ledger: ResidentLedgerWriter,
) -> mct_daemon::MctUdsControlMutationHandler {
    mct_daemon::MctUdsControlMutationHandler::new(move |path, body| {
        let configured_path = configured_path.clone();
        let ledger = ledger.clone();
        async move { execute_resident_peer_mutation(&configured_path, &ledger, &path, &body).await }
    })
}

pub(super) fn execute_offline_peer_mutation(
    configured_path: &Path,
    ledger_path: &Path,
    path: &str,
    body: &[u8],
) -> Result<PeerMutationSuccess> {
    let mut ledger = JsonlObservationLedger::open(ledger_path, "ledger-local", "local-mct")
        .with_context(|| {
            format!(
                "acquire exclusive observation ledger writer lock at {}",
                ledger_path.display()
            )
        })?;
    let prepared = prepare_peer_mutation(configured_path, path, body)?;
    ledger.append_batch_before_effect(
        [prepared.decision_observation()],
        mct_daemon::current_timestamp_string(),
    )?;
    match prepared.apply() {
        Ok(response) => Ok(response),
        Err(error) => {
            ledger.append_batch_before_effect(
                [prepared.failure_observation()],
                mct_daemon::current_timestamp_string(),
            )?;
            Err(error)
        }
    }
}

const AUTHORITY_MUTATION_BODY_MAX_BYTES: usize = 64 * 1024;
static NEXT_AUTHORITY_MUTATION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChildApproveRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) expected_children_dir: PathBuf,
    pub(super) child_name: String,
    pub(super) strict_integrity: bool,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChildRevokeRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) child_name: String,
}

#[derive(Clone, Debug)]
struct PreparedChildMutation {
    config_path: PathBuf,
    child_name: String,
    config: mct_daemon::MctDaemonConfig,
    approving: bool,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub(super) struct ChildMutationSuccess {
    pub(super) child_name: String,
    pub(super) approval_state: ChildApprovalState,
    pub(super) assignment_state: ChildAssignmentState,
    pub(super) approval_count: usize,
    pub(super) assignment_count: usize,
}

struct MutationObservationFact {
    namespace: &'static str,
    kind: ObservationKind,
    subject_id: String,
    resource_id: String,
    policy_revision: Option<u64>,
    grants_revision: Option<u64>,
    outcome: ObservationOutcome,
    source_plane: SourcePlane,
    safe_message: String,
}

fn mutation_observation(fact: MutationObservationFact) -> MctObservation {
    let sequence = NEXT_AUTHORITY_MUTATION_ID.fetch_add(1, Ordering::Relaxed);
    let observed_at = current_timestamp();
    let id = format!("{}-{observed_at}-{sequence}", fact.namespace);
    MctObservation {
        observation_id: ObservationId::new(format!("obs:{id}"))
            .expect("generated observation ID must be non-empty"),
        observed_at,
        kind: fact.kind,
        source_plane: fact.source_plane,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace:{id}"))
                .expect("generated trace ID must be non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: Some(
            DecisionId::new(format!("decision:{id}"))
                .expect("generated decision ID must be non-empty"),
        ),
        subject_id: Some(fact.subject_id),
        resource_id: Some(fact.resource_id),
        policy_revision: fact.policy_revision,
        grants_revision: fact.grants_revision,
        outcome: fact.outcome,
        visibility: ObservationVisibility::NodeOperator,
        safe_message: fact.safe_message,
        detail_ref: None,
    }
}

fn prepare_child_mutation(
    configured_path: &Path,
    configured_children_dir: &Path,
    path: &str,
    body: &[u8],
) -> Result<PreparedChildMutation> {
    if body.len() > AUTHORITY_MUTATION_BODY_MAX_BYTES {
        bail!("child authority mutation body exceeds 64 KiB limit");
    }
    let store = MctDaemonConfigStore::new(configured_path);
    match path {
        "/children/approve" => {
            let request: ChildApproveRequest =
                serde_json::from_slice(body).context("decode child approval request")?;
            require_expected_config_path(&request.expected_config_path, configured_path)?;
            require_expected_config_path(&request.expected_children_dir, configured_children_dir)?;
            if request.child_name.trim().is_empty() {
                bail!("child name must not be empty");
            }
            let report = load_children_from_dir(MctChildLoadOptions {
                children_dir: configured_children_dir.to_path_buf(),
                integrity_mode: if request.strict_integrity {
                    MctChildIntegrityMode::RequireSidecars
                } else {
                    MctChildIntegrityMode::AuditOnly
                },
            });
            let child = report
                .children
                .iter()
                .find(|child| child.name == request.child_name)
                .ok_or_else(|| anyhow::anyhow!("loaded child not found"))?;
            let config = store
                .prepare_approved_and_assigned_child(child, MctOperatorChildScope::default())?;
            Ok(PreparedChildMutation {
                config_path: configured_path.to_path_buf(),
                child_name: request.child_name,
                config,
                approving: true,
            })
        }
        "/children/revoke" => {
            let request: ChildRevokeRequest =
                serde_json::from_slice(body).context("decode child revocation request")?;
            require_expected_config_path(&request.expected_config_path, configured_path)?;
            let config = store.prepare_revoked_child(&request.child_name)?;
            Ok(PreparedChildMutation {
                config_path: configured_path.to_path_buf(),
                child_name: request.child_name,
                config,
                approving: false,
            })
        }
        _ => bail!("unknown child authority mutation route"),
    }
}

impl PreparedChildMutation {
    fn decision_observations(&self) -> Vec<MctObservation> {
        let approval = &self.config.child_approvals[&self.child_name];
        let assignment = &self.config.child_assignments[&self.child_name];
        let outcome = if self.approving {
            ObservationOutcome::Allowed
        } else {
            ObservationOutcome::Denied
        };
        vec![
            mutation_observation(MutationObservationFact {
                namespace: "child-approval",
                kind: if self.approving {
                    ObservationKind::ChildApproved
                } else {
                    ObservationKind::ChildRevoked
                },
                subject_id: self.child_name.clone(),
                resource_id: approval.artifact_id.to_string(),
                policy_revision: Some(approval.policy_revision),
                grants_revision: None,
                outcome,
                source_plane: SourcePlane::Kernel,
                safe_message: if self.approving {
                    "child approved".into()
                } else {
                    "child approval revoked".into()
                },
            }),
            mutation_observation(MutationObservationFact {
                namespace: "child-assignment",
                kind: if self.approving {
                    ObservationKind::ChildAssigned
                } else {
                    ObservationKind::ChildAssignmentRevoked
                },
                subject_id: self.child_name.clone(),
                resource_id: format!("assignment:{}", self.child_name),
                policy_revision: Some(assignment.policy_revision),
                grants_revision: None,
                outcome,
                source_plane: SourcePlane::Kernel,
                safe_message: if self.approving {
                    "child assigned".into()
                } else {
                    "child assignment revoked".into()
                },
            }),
        ]
    }

    fn failure_observation(&self) -> MctObservation {
        mutation_observation(MutationObservationFact {
            namespace: "child-authority-failure",
            kind: ObservationKind::OperatorActionRecorded,
            subject_id: self.child_name.clone(),
            resource_id: format!("config:{}", self.config_path.display()),
            policy_revision: self
                .config
                .child_approvals
                .get(&self.child_name)
                .map(|approval| approval.policy_revision),
            grants_revision: None,
            outcome: ObservationOutcome::Failed,
            source_plane: SourcePlane::Operator,
            safe_message: "child authority config apply failed".into(),
        })
    }

    fn apply(&self) -> Result<ChildMutationSuccess> {
        MctDaemonConfigStore::new(&self.config_path).save(&self.config)?;
        Ok(ChildMutationSuccess {
            child_name: self.child_name.clone(),
            approval_state: self.config.child_approvals[&self.child_name].approval_state,
            assignment_state: self.config.child_assignments[&self.child_name].assignment_state,
            approval_count: self.config.child_approvals.len(),
            assignment_count: self.config.child_assignments.len(),
        })
    }
}

async fn execute_resident_child_mutation(
    configured_path: &Path,
    children_dir: &Path,
    ledger: &ResidentLedgerWriter,
    path: &str,
    body: &[u8],
) -> MctControlPlaneResponse {
    let prepared = match prepare_child_mutation(configured_path, children_dir, path, body) {
        Ok(prepared) => prepared,
        Err(_) => {
            return peer_mutation_response(
                400,
                serde_json::json!({"error": "child authority mutation rejected"}),
            );
        }
    };
    if ledger
        .append(prepared.decision_observations())
        .await
        .is_err()
    {
        return peer_mutation_response(
            500,
            serde_json::json!({"error": "child authority decision was not durable"}),
        );
    }
    match prepared.apply() {
        Ok(success) => peer_mutation_response(
            200,
            serde_json::to_value(success).expect("child mutation response must serialize"),
        ),
        Err(_) => {
            let failure_is_durable = ledger
                .append(vec![prepared.failure_observation()])
                .await
                .is_ok();
            peer_mutation_response(
                500,
                serde_json::json!({"error": if failure_is_durable {
                    "child authority config apply failed"
                } else {
                    "child authority config apply failed and failure observation was not durable"
                }}),
            )
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedBlobIngestRequest {
    digest: String,
    size_bytes: u64,
    content_type: String,
    #[serde(default = "default_blob_classification")]
    classification: String,
    bytes_base64: String,
}

fn default_blob_classification() -> String {
    "unspecified".into()
}

fn blob_observation(
    request: Option<&ObservedBlobIngestRequest>,
    kind: ObservationKind,
    outcome: ObservationOutcome,
    reason: &str,
) -> MctObservation {
    let digest = request
        .map(|request| request.digest.clone())
        .unwrap_or_else(|| "unknown".into());
    let safe_message = request.map_or_else(
        || format!("blob ingest {reason}"),
        |request| {
            format!(
                "blob ingest {reason} digest={} size={} content_type={} classification={}",
                request.digest, request.size_bytes, request.content_type, request.classification
            )
        },
    );
    mutation_observation(MutationObservationFact {
        namespace: "blob-ingest",
        kind,
        subject_id: "local-cas".into(),
        resource_id: digest,
        policy_revision: None,
        grants_revision: None,
        outcome,
        source_plane: SourcePlane::Storage,
        safe_message,
    })
}

async fn append_blob_rejection(
    ledger: &ResidentLedgerWriter,
    request: Option<&ObservedBlobIngestRequest>,
    status: u16,
    reason: &str,
) -> MctControlPlaneResponse {
    let observation = blob_observation(
        request,
        ObservationKind::StorageAppendFailed,
        ObservationOutcome::Failed,
        reason,
    );
    if ledger.append(vec![observation]).await.is_err() {
        return peer_mutation_response(
            500,
            serde_json::json!({"error": "blob rejection observation was not durable"}),
        );
    }
    peer_mutation_response(status, serde_json::json!({"error": reason}))
}

async fn execute_resident_blob_mutation(
    state_path: &Path,
    ledger: &ResidentLedgerWriter,
    body: &[u8],
) -> MctControlPlaneResponse {
    let request: ObservedBlobIngestRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return append_blob_rejection(ledger, None, 400, "invalid_request").await,
    };
    if request.size_bytes > MCT_BLOB_MAX_BYTES as u64 {
        return append_blob_rejection(ledger, Some(&request), 413, "oversize").await;
    }
    if request.content_type.trim().is_empty() || request.classification.trim().is_empty() {
        return append_blob_rejection(ledger, Some(&request), 400, "invalid_metadata").await;
    }
    let bytes = match BASE64_STANDARD.decode(request.bytes_base64.as_bytes()) {
        Ok(bytes) => bytes,
        Err(_) => {
            return append_blob_rejection(ledger, Some(&request), 400, "invalid_encoding").await;
        }
    };
    if bytes.len() as u64 != request.size_bytes {
        return append_blob_rejection(ledger, Some(&request), 400, "size_mismatch").await;
    }
    if request.digest.len() != 64
        || !request
            .digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return append_blob_rejection(ledger, Some(&request), 400, "invalid_digest").await;
    }
    if blake3::hash(&bytes).to_hex().as_str() != request.digest {
        return append_blob_rejection(ledger, Some(&request), 400, "digest_mismatch").await;
    }

    if ledger
        .append(vec![blob_observation(
            Some(&request),
            ObservationKind::AdapterEffectStarted,
            ObservationOutcome::Started,
            "validated",
        )])
        .await
        .is_err()
    {
        return peer_mutation_response(
            500,
            serde_json::json!({"error": "blob ingest decision was not durable"}),
        );
    }

    let store = local_blob_store_for_state_path(state_path);
    let handle = match store.ingest_reader(
        &request.digest,
        request.size_bytes,
        &request.content_type,
        std::io::Cursor::new(bytes),
    ) {
        Ok(handle) => handle,
        Err(error) => {
            let reason = match error {
                MctLocalBlobStoreError::InvalidDigest => "invalid_digest",
                MctLocalBlobStoreError::BlobTooLarge => "oversize",
                MctLocalBlobStoreError::BlobSizeMismatch => "size_mismatch",
                MctLocalBlobStoreError::BlobDigestMismatch => "digest_mismatch",
                MctLocalBlobStoreError::PayloadBlobUnavailable => "unavailable",
                MctLocalBlobStoreError::Io { .. } => "storage_io",
            };
            return append_blob_rejection(ledger, Some(&request), 500, reason).await;
        }
    };
    if ledger
        .append(vec![blob_observation(
            Some(&request),
            ObservationKind::StorageAppendSucceeded,
            ObservationOutcome::Completed,
            "succeeded",
        )])
        .await
        .is_err()
    {
        return peer_mutation_response(
            500,
            serde_json::json!({"error": "blob success observation was not durable"}),
        );
    }
    peer_mutation_response(201, serde_json::json!({"payload": handle}))
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RegistryInstallRequest {
    pub(super) expected_children_dir: PathBuf,
    pub(super) source_dir: PathBuf,
    pub(super) replace: bool,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RegistrySyncRequest {
    pub(super) expected_children_dir: PathBuf,
    pub(super) expected_state_path: PathBuf,
    pub(super) source_id: String,
    pub(super) strict_integrity: bool,
}

#[derive(Clone, Debug)]
enum PreparedRegistryMutation {
    Install {
        request: RegistryInstallRequest,
        child_name: String,
        artifact_id: String,
        artifact_version: String,
    },
    Sync {
        request: RegistrySyncRequest,
        load_report: mct_daemon::MctChildLoadReport,
    },
}

fn prepare_registry_mutation(
    configured_children_dir: &Path,
    state_path: &Path,
    path: &str,
    body: &[u8],
) -> Result<PreparedRegistryMutation> {
    if body.len() > AUTHORITY_MUTATION_BODY_MAX_BYTES {
        bail!("registry mutation body exceeds 64 KiB limit");
    }
    match path {
        "/registry/install" => {
            let request: RegistryInstallRequest =
                serde_json::from_slice(body).context("decode registry install request")?;
            require_expected_config_path(&request.expected_children_dir, configured_children_dir)?;
            let report = load_children_from_dir(
                MctChildLoadOptions::new(&request.source_dir).strict_integrity(),
            );
            if report.loaded != 1 || report.failed != 0 {
                bail!("registry package must contain one verified child");
            }
            let child = report.children.first().expect("loaded count checked");
            if configured_children_dir.join(&child.name).exists() && !request.replace {
                bail!("installed child already exists and replace was not requested");
            }
            Ok(PreparedRegistryMutation::Install {
                request,
                child_name: child.name.clone(),
                artifact_id: child.artifact_id.clone(),
                artifact_version: child.version.clone(),
            })
        }
        "/registry/sync" => {
            let request: RegistrySyncRequest =
                serde_json::from_slice(body).context("decode registry sync request")?;
            require_expected_config_path(&request.expected_children_dir, configured_children_dir)?;
            require_expected_config_path(&request.expected_state_path, state_path)?;
            if request.source_id.trim().is_empty() {
                bail!("registry source id must not be empty");
            }
            let load_report = load_children_from_dir(MctChildLoadOptions {
                children_dir: configured_children_dir.to_path_buf(),
                integrity_mode: if request.strict_integrity {
                    MctChildIntegrityMode::RequireSidecars
                } else {
                    MctChildIntegrityMode::AuditOnly
                },
            });
            Ok(PreparedRegistryMutation::Sync {
                request,
                load_report,
            })
        }
        _ => bail!("unknown registry mutation route"),
    }
}

impl PreparedRegistryMutation {
    fn decision_observations(&self) -> Vec<MctObservation> {
        match self {
            Self::Install {
                child_name,
                artifact_id,
                artifact_version,
                ..
            } => vec![mutation_observation(MutationObservationFact {
                namespace: "registry-install",
                kind: ObservationKind::ArtifactVerified,
                subject_id: child_name.clone(),
                resource_id: artifact_id.clone(),
                policy_revision: None,
                grants_revision: None,
                outcome: ObservationOutcome::Allowed,
                source_plane: SourcePlane::Kernel,
                safe_message: format!("verified child artifact version={artifact_version}"),
            })],
            Self::Sync {
                request,
                load_report,
            } => {
                let mut observations = load_report
                    .children
                    .iter()
                    .map(|child| {
                        mutation_observation(MutationObservationFact {
                            namespace: "registry-sync-artifact",
                            kind: if child.integrity_verified() {
                                ObservationKind::ArtifactVerified
                            } else {
                                ObservationKind::ArtifactRejected
                            },
                            subject_id: child.name.clone(),
                            resource_id: child.artifact_id.clone(),
                            policy_revision: None,
                            grants_revision: None,
                            outcome: if child.integrity_verified() {
                                ObservationOutcome::Allowed
                            } else {
                                ObservationOutcome::Denied
                            },
                            source_plane: SourcePlane::Kernel,
                            safe_message: if child.integrity_verified() {
                                "registry artifact verified".into()
                            } else {
                                "registry artifact rejected".into()
                            },
                        })
                    })
                    .collect::<Vec<_>>();
                observations.push(mutation_observation(MutationObservationFact {
                    namespace: "registry-sync",
                    kind: ObservationKind::OperatorActionRecorded,
                    subject_id: request.source_id.clone(),
                    resource_id: format!("children:{}", request.expected_children_dir.display()),
                    policy_revision: None,
                    grants_revision: None,
                    outcome: ObservationOutcome::Allowed,
                    source_plane: SourcePlane::Operator,
                    safe_message: format!(
                        "registry sync accepted loaded={} failed={}",
                        load_report.loaded, load_report.failed
                    ),
                }));
                observations
            }
        }
    }

    fn failure_observation(&self) -> MctObservation {
        mutation_observation(MutationObservationFact {
            namespace: "registry-storage-failure",
            kind: ObservationKind::StorageAppendFailed,
            subject_id: match self {
                Self::Install { child_name, .. } => child_name.clone(),
                Self::Sync { request, .. } => request.source_id.clone(),
            },
            resource_id: "registry".into(),
            policy_revision: None,
            grants_revision: None,
            outcome: ObservationOutcome::Failed,
            source_plane: SourcePlane::Storage,
            safe_message: "registry storage effect failed".into(),
        })
    }

    fn apply(&self, state_path: &Path) -> Result<serde_json::Value> {
        match self {
            Self::Install { request, .. } => {
                Ok(serde_json::to_value(install_verified_child_package(
                    &request.source_dir,
                    &request.expected_children_dir,
                    request.replace,
                )?)?)
            }
            Self::Sync { request, .. } => {
                let state = MctRuntimeStateStore::open(state_path)?;
                Ok(serde_json::to_value(sync_child_registry_source(
                    &state,
                    request.source_id.clone(),
                    &request.expected_children_dir,
                    if request.strict_integrity {
                        MctChildIntegrityMode::RequireSidecars
                    } else {
                        MctChildIntegrityMode::AuditOnly
                    },
                    MctOperatorChildScope::default(),
                )?)?)
            }
        }
    }

    fn success_observation(&self) -> MctObservation {
        mutation_observation(MutationObservationFact {
            namespace: "registry-storage-success",
            kind: ObservationKind::StorageAppendSucceeded,
            subject_id: match self {
                Self::Install { child_name, .. } => child_name.clone(),
                Self::Sync { request, .. } => request.source_id.clone(),
            },
            resource_id: "registry".into(),
            policy_revision: None,
            grants_revision: None,
            outcome: ObservationOutcome::Completed,
            source_plane: SourcePlane::Storage,
            safe_message: "registry storage effect completed".into(),
        })
    }
}

async fn execute_resident_registry_mutation(
    children_dir: &Path,
    state_path: &Path,
    ledger: &ResidentLedgerWriter,
    path: &str,
    body: &[u8],
) -> MctControlPlaneResponse {
    let prepared = match prepare_registry_mutation(children_dir, state_path, path, body) {
        Ok(prepared) => prepared,
        Err(_) => {
            return peer_mutation_response(
                400,
                serde_json::json!({"error": "registry mutation rejected"}),
            );
        }
    };
    if ledger
        .append(prepared.decision_observations())
        .await
        .is_err()
    {
        return peer_mutation_response(
            500,
            serde_json::json!({"error": "registry decision was not durable"}),
        );
    }
    match prepared.apply(state_path) {
        Ok(value) => {
            if ledger
                .append(vec![prepared.success_observation()])
                .await
                .is_err()
            {
                return peer_mutation_response(
                    500,
                    serde_json::json!({"error": "registry success observation was not durable"}),
                );
            }
            peer_mutation_response(200, value)
        }
        Err(_) => {
            let _ = ledger.append(vec![prepared.failure_observation()]).await;
            peer_mutation_response(
                500,
                serde_json::json!({"error": "registry storage effect failed"}),
            )
        }
    }
}

pub(super) fn execute_offline_registry_mutation(
    children_dir: &Path,
    state_path: &Path,
    ledger_path: &Path,
    path: &str,
    body: &[u8],
) -> Result<serde_json::Value> {
    let mut ledger = JsonlObservationLedger::open(ledger_path, "ledger-local", "local-mct")
        .with_context(|| {
            format!(
                "acquire exclusive observation ledger writer lock at {}",
                ledger_path.display()
            )
        })?;
    let prepared = prepare_registry_mutation(children_dir, state_path, path, body)?;
    ledger.append_batch_before_effect(
        prepared.decision_observations(),
        mct_daemon::current_timestamp_string(),
    )?;
    match prepared.apply(state_path) {
        Ok(value) => {
            ledger.append_batch_before_effect(
                [prepared.success_observation()],
                mct_daemon::current_timestamp_string(),
            )?;
            Ok(value)
        }
        Err(error) => {
            ledger.append_batch_before_effect(
                [prepared.failure_observation()],
                mct_daemon::current_timestamp_string(),
            )?;
            Err(error)
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToyAuthorizeSlateRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) expected_children_dir: PathBuf,
    pub(super) expected_state_path: PathBuf,
    pub(super) child_name: String,
    pub(super) project_root: PathBuf,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToyAuthorizeSecretRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) expected_children_dir: PathBuf,
    pub(super) expected_state_path: PathBuf,
    pub(super) child_name: String,
    pub(super) secret_name: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PandoRecordRequest {
    pub(super) expected_state_path: PathBuf,
    pub(super) plan: MctCompositionPlan,
}

#[derive(Clone, Debug)]
enum PreparedAdministrativeMutation {
    ToyGrants {
        state_path: PathBuf,
        child_name: String,
        contracts: Vec<CanonicalToyContract>,
        grants: Vec<ToyGrant>,
    },
    Composition {
        state_path: PathBuf,
        plan: MctCompositionPlan,
    },
}

fn require_current_child_authority(
    config_path: &Path,
    children_dir: &Path,
    child_name: &str,
) -> Result<mct_daemon::MctLoadedChild> {
    let child = load_named_child(children_dir, child_name)?;
    let config = MctDaemonConfigStore::new(config_path).load()?;
    let approval = config
        .child_approvals
        .get(child_name)
        .ok_or_else(|| anyhow::anyhow!("child is not approved"))?;
    let assignment = config
        .child_assignments
        .get(child_name)
        .ok_or_else(|| anyhow::anyhow!("child is not assigned"))?;
    if approval.approval_state != ChildApprovalState::Approved
        || assignment.assignment_state != ChildAssignmentState::Active
        || approval.artifact_id.as_str() != child.artifact_id
        || assignment.artifact_id.as_str() != child.artifact_id
    {
        bail!("child authority is not active for loaded artifact");
    }
    Ok(child)
}

fn prepare_administrative_mutation(
    config_path: &Path,
    children_dir: &Path,
    state_path: &Path,
    path: &str,
    body: &[u8],
) -> Result<PreparedAdministrativeMutation> {
    if body.len() > AUTHORITY_MUTATION_BODY_MAX_BYTES {
        bail!("administrative mutation body exceeds 64 KiB limit");
    }
    match path {
        "/toys/authorize-slate" => {
            let request: ToyAuthorizeSlateRequest =
                serde_json::from_slice(body).context("decode slate grant request")?;
            require_expected_config_path(&request.expected_config_path, config_path)?;
            require_expected_config_path(&request.expected_children_dir, children_dir)?;
            require_expected_config_path(&request.expected_state_path, state_path)?;
            let project_root = canonical_dir(request.project_root, "project root")?;
            let child =
                require_current_child_authority(config_path, children_dir, &request.child_name)?;
            Ok(PreparedAdministrativeMutation::ToyGrants {
                state_path: state_path.to_path_buf(),
                child_name: request.child_name,
                contracts: slate_toy_contracts(),
                grants: slate_toy_grants_for_child(&child, &project_root),
            })
        }
        "/toys/authorize-secret" => {
            let request: ToyAuthorizeSecretRequest =
                serde_json::from_slice(body).context("decode secret grant request")?;
            require_expected_config_path(&request.expected_config_path, config_path)?;
            require_expected_config_path(&request.expected_children_dir, children_dir)?;
            require_expected_config_path(&request.expected_state_path, state_path)?;
            if request.secret_name.trim().is_empty() {
                bail!("secret name must not be empty");
            }
            let child =
                require_current_child_authority(config_path, children_dir, &request.child_name)?;
            Ok(PreparedAdministrativeMutation::ToyGrants {
                state_path: state_path.to_path_buf(),
                child_name: request.child_name,
                contracts: vec![mct_secrets_toy_contract()],
                grants: vec![secret_toy_grant_for_child(&child, &request.secret_name)],
            })
        }
        "/pando/record" => {
            let request: PandoRecordRequest =
                serde_json::from_slice(body).context("decode composition record request")?;
            require_expected_config_path(&request.expected_state_path, state_path)?;
            if request.plan.composition_id.trim().is_empty() {
                bail!("composition id must not be empty");
            }
            Ok(PreparedAdministrativeMutation::Composition {
                state_path: state_path.to_path_buf(),
                plan: request.plan,
            })
        }
        _ => bail!("unknown administrative mutation route"),
    }
}

impl PreparedAdministrativeMutation {
    fn decision_observations(&self) -> Vec<MctObservation> {
        match self {
            Self::ToyGrants {
                child_name, grants, ..
            } => grants
                .iter()
                .map(|grant| {
                    mutation_observation(MutationObservationFact {
                        namespace: "toy-grant",
                        kind: ObservationKind::ToyGrantAllowed,
                        subject_id: child_name.clone(),
                        resource_id: format!("{}:{}", grant.toy_id, grant.grant_id),
                        policy_revision: Some(grant.policy_revision),
                        grants_revision: Some(grant.grants_revision),
                        outcome: ObservationOutcome::Allowed,
                        source_plane: SourcePlane::Kernel,
                        safe_message: "toy grant allowed".into(),
                    })
                })
                .collect(),
            Self::Composition { plan, .. } => vec![mutation_observation(MutationObservationFact {
                namespace: "composition-record",
                kind: ObservationKind::OperatorActionRecorded,
                subject_id: "local-operator".into(),
                resource_id: plan.composition_id.clone(),
                policy_revision: None,
                grants_revision: None,
                outcome: ObservationOutcome::Allowed,
                source_plane: SourcePlane::Operator,
                safe_message: format!("composition plan accepted steps={}", plan.steps.len()),
            })],
        }
    }

    fn failure_observation(&self) -> MctObservation {
        mutation_observation(MutationObservationFact {
            namespace: "administrative-storage-failure",
            kind: ObservationKind::StorageAppendFailed,
            subject_id: "local-operator".into(),
            resource_id: match self {
                Self::ToyGrants { child_name, .. } => child_name.clone(),
                Self::Composition { plan, .. } => plan.composition_id.clone(),
            },
            policy_revision: None,
            grants_revision: None,
            outcome: ObservationOutcome::Failed,
            source_plane: SourcePlane::Storage,
            safe_message: "administrative state effect failed".into(),
        })
    }

    fn apply(&self) -> Result<serde_json::Value> {
        match self {
            Self::ToyGrants {
                state_path,
                child_name,
                contracts,
                grants,
            } => {
                let state = MctRuntimeStateStore::open(state_path)?;
                for contract in contracts {
                    state.upsert_toy_contract(contract)?;
                }
                for grant in grants {
                    state.upsert_toy_grant_snapshot(grant)?;
                }
                Ok(serde_json::json!({
                    "state": state_path,
                    "child": child_name,
                    "contracts": contracts.len(),
                    "grants": grants.len()
                }))
            }
            Self::Composition { state_path, plan } => Ok(serde_json::to_value(
                record_composition_plan(&MctRuntimeStateStore::open(state_path)?, plan.clone())?,
            )?),
        }
    }
}

async fn execute_resident_administrative_mutation(
    config_path: &Path,
    children_dir: &Path,
    state_path: &Path,
    ledger: &ResidentLedgerWriter,
    path: &str,
    body: &[u8],
) -> MctControlPlaneResponse {
    let prepared =
        match prepare_administrative_mutation(config_path, children_dir, state_path, path, body) {
            Ok(prepared) => prepared,
            Err(_) => {
                return peer_mutation_response(
                    400,
                    serde_json::json!({"error": "administrative mutation rejected"}),
                );
            }
        };
    if ledger
        .append(prepared.decision_observations())
        .await
        .is_err()
    {
        return peer_mutation_response(
            500,
            serde_json::json!({"error": "administrative decision was not durable"}),
        );
    }
    match prepared.apply() {
        Ok(value) => peer_mutation_response(200, value),
        Err(_) => {
            let _ = ledger.append(vec![prepared.failure_observation()]).await;
            peer_mutation_response(
                500,
                serde_json::json!({"error": "administrative state effect failed"}),
            )
        }
    }
}

pub(super) fn execute_offline_administrative_mutation(
    config_path: &Path,
    children_dir: &Path,
    state_path: &Path,
    ledger_path: &Path,
    path: &str,
    body: &[u8],
) -> Result<serde_json::Value> {
    let mut ledger = JsonlObservationLedger::open(ledger_path, "ledger-local", "local-mct")
        .with_context(|| {
            format!(
                "acquire exclusive observation ledger writer lock at {}",
                ledger_path.display()
            )
        })?;
    let prepared =
        prepare_administrative_mutation(config_path, children_dir, state_path, path, body)?;
    ledger.append_batch_before_effect(
        prepared.decision_observations(),
        mct_daemon::current_timestamp_string(),
    )?;
    match prepared.apply() {
        Ok(value) => Ok(value),
        Err(error) => {
            ledger.append_batch_before_effect(
                [prepared.failure_observation()],
                mct_daemon::current_timestamp_string(),
            )?;
            Err(error)
        }
    }
}

fn resident_mutation_handler(
    configured_path: PathBuf,
    children_dir: PathBuf,
    state_path: Option<PathBuf>,
    ledger: ResidentLedgerWriter,
) -> mct_daemon::MctUdsControlMutationHandler {
    mct_daemon::MctUdsControlMutationHandler::new(move |path, body| {
        let configured_path = configured_path.clone();
        let children_dir = children_dir.clone();
        let state_path = state_path.clone();
        let ledger = ledger.clone();
        async move {
            if path == "/blobs" {
                match state_path {
                    Some(state_path) => {
                        execute_resident_blob_mutation(&state_path, &ledger, &body).await
                    }
                    None => peer_mutation_response(
                        404,
                        serde_json::json!({"error": "blob ingest unavailable"}),
                    ),
                }
            } else if path.starts_with("/registry/") {
                match state_path {
                    Some(state_path) => {
                        execute_resident_registry_mutation(
                            &children_dir,
                            &state_path,
                            &ledger,
                            &path,
                            &body,
                        )
                        .await
                    }
                    None => peer_mutation_response(
                        404,
                        serde_json::json!({"error": "registry mutation unavailable"}),
                    ),
                }
            } else if path.starts_with("/toys/") || path == "/pando/record" {
                match state_path {
                    Some(state_path) => {
                        execute_resident_administrative_mutation(
                            &configured_path,
                            &children_dir,
                            &state_path,
                            &ledger,
                            &path,
                            &body,
                        )
                        .await
                    }
                    None => peer_mutation_response(
                        404,
                        serde_json::json!({"error": "administrative mutation unavailable"}),
                    ),
                }
            } else if path.starts_with("/children/") {
                execute_resident_child_mutation(
                    &configured_path,
                    &children_dir,
                    &ledger,
                    &path,
                    &body,
                )
                .await
            } else if path == "/identity/ensure" {
                peer_mutation_response(
                    409,
                    serde_json::json!({"error": "stop the daemon to create or rotate identity"}),
                )
            } else {
                execute_resident_peer_mutation(&configured_path, &ledger, &path, &body).await
            }
        }
    })
}

#[cfg(test)]
pub(super) fn resident_authority_mutation_handler(
    configured_path: PathBuf,
    children_dir: PathBuf,
    ledger: ResidentLedgerWriter,
) -> mct_daemon::MctUdsControlMutationHandler {
    resident_mutation_handler(configured_path, children_dir, None, ledger)
}

pub(super) fn resident_observed_mutation_handler(
    configured_path: PathBuf,
    children_dir: PathBuf,
    state_path: PathBuf,
    ledger: ResidentLedgerWriter,
) -> mct_daemon::MctUdsControlMutationHandler {
    resident_mutation_handler(configured_path, children_dir, Some(state_path), ledger)
}

pub(super) fn execute_offline_child_mutation(
    configured_path: &Path,
    children_dir: &Path,
    ledger_path: &Path,
    path: &str,
    body: &[u8],
) -> Result<ChildMutationSuccess> {
    let mut ledger = JsonlObservationLedger::open(ledger_path, "ledger-local", "local-mct")
        .with_context(|| {
            format!(
                "acquire exclusive observation ledger writer lock at {}",
                ledger_path.display()
            )
        })?;
    let prepared = prepare_child_mutation(configured_path, children_dir, path, body)?;
    ledger.append_batch_before_effect(
        prepared.decision_observations(),
        mct_daemon::current_timestamp_string(),
    )?;
    match prepared.apply() {
        Ok(success) => Ok(success),
        Err(error) => {
            ledger.append_batch_before_effect(
                [prepared.failure_observation()],
                mct_daemon::current_timestamp_string(),
            )?;
            Err(error)
        }
    }
}

#[derive(Clone, Debug)]
struct PreparedIdentityMutation {
    config_path: PathBuf,
    identity_path: PathBuf,
    identity: MctLocalNodeIdentity,
    config: mct_daemon::MctDaemonConfig,
    secret_key_hex: String,
    write_new_key: bool,
}

fn prepare_identity_mutation(
    store: &MctDaemonConfigStore,
    scope: MctOperatorNodeScope,
    identity_path: &Path,
) -> Result<PreparedIdentityMutation> {
    let secret_key_hex = if identity_path.exists() {
        load_or_create_node_secret_key_hex(identity_path)?
    } else {
        generate_node_secret_key_hex()
    };
    let endpoint_id = endpoint_id_for_secret_key_hex(&secret_key_hex)?;
    let identity = MctLocalNodeIdentity {
        node_id: scope.node_id,
        vision_id: scope.vision_id,
        endpoint_id,
        identity_path: identity_path.to_path_buf(),
        policy_revision: scope.policy_revision,
        updated_at: mct_daemon::current_timestamp_string(),
    };
    let mut config = store.load()?;
    config.local_identity = Some(identity.clone());
    Ok(PreparedIdentityMutation {
        config_path: store.path().to_path_buf(),
        identity_path: identity_path.to_path_buf(),
        identity,
        config,
        secret_key_hex,
        write_new_key: !identity_path.exists(),
    })
}

impl PreparedIdentityMutation {
    fn decision_observation(&self) -> MctObservation {
        mutation_observation(MutationObservationFact {
            namespace: "node-identity",
            kind: ObservationKind::OperatorActionRecorded,
            subject_id: self.identity.node_id.to_string(),
            resource_id: self.identity.endpoint_id.to_string(),
            policy_revision: Some(self.identity.policy_revision),
            grants_revision: None,
            outcome: ObservationOutcome::Allowed,
            source_plane: SourcePlane::Operator,
            safe_message: format!(
                "local node identity recorded endpoint={} vision={}",
                self.identity.endpoint_id, self.identity.vision_id
            ),
        })
    }

    fn failure_observation(&self) -> MctObservation {
        mutation_observation(MutationObservationFact {
            namespace: "node-identity-failure",
            kind: ObservationKind::OperatorActionRecorded,
            subject_id: self.identity.node_id.to_string(),
            resource_id: self.identity.endpoint_id.to_string(),
            policy_revision: Some(self.identity.policy_revision),
            grants_revision: None,
            outcome: ObservationOutcome::Failed,
            source_plane: SourcePlane::Operator,
            safe_message: "local node identity apply failed".into(),
        })
    }

    fn apply(&self) -> Result<MctLocalNodeIdentity> {
        if self.write_new_key {
            write_new_node_secret_key_file(&self.identity_path, &self.secret_key_hex)?;
        }
        MctDaemonConfigStore::new(&self.config_path).save(&self.config)?;
        Ok(self.identity.clone())
    }
}

pub(super) async fn ensure_observed_local_identity(
    store: &MctDaemonConfigStore,
    scope: MctOperatorNodeScope,
    identity_path: &Path,
    ledger: &ResidentLedgerWriter,
) -> Result<MctLocalNodeIdentity> {
    let prepared = prepare_identity_mutation(store, scope, identity_path)?;
    ledger
        .append(vec![prepared.decision_observation()])
        .await
        .context("append local identity decision to ledger")?;
    match prepared.apply() {
        Ok(identity) => Ok(identity),
        Err(error) => {
            ledger.append(vec![prepared.failure_observation()]).await?;
            Err(error)
        }
    }
}

pub(super) fn execute_offline_identity_mutation(
    configured_path: &Path,
    identity_path: &Path,
    ledger_path: &Path,
) -> Result<MctLocalNodeIdentity> {
    let mut ledger = JsonlObservationLedger::open(ledger_path, "ledger-local", "local-mct")
        .with_context(|| {
            format!(
                "acquire exclusive observation ledger writer lock at {}",
                ledger_path.display()
            )
        })?;
    let store = MctDaemonConfigStore::new(configured_path);
    let existing = store.load()?;
    let scope = existing
        .local_identity
        .as_ref()
        .map(|identity| MctOperatorNodeScope {
            node_id: identity.node_id.clone(),
            vision_id: identity.vision_id.clone(),
            policy_revision: identity.policy_revision,
        })
        .unwrap_or_default();
    let prepared = prepare_identity_mutation(&store, scope, identity_path)?;
    ledger.append_batch_before_effect(
        [prepared.decision_observation()],
        mct_daemon::current_timestamp_string(),
    )?;
    match prepared.apply() {
        Ok(identity) => Ok(identity),
        Err(error) => {
            ledger.append_batch_before_effect(
                [prepared.failure_observation()],
                mct_daemon::current_timestamp_string(),
            )?;
            Err(error)
        }
    }
}

pub(super) async fn run_control(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected control subcommand: serve-http | serve-uds");
    }
    match args.remove(0).as_str() {
        "serve-http" => run_control_serve_http(args).await,
        "serve-uds" => run_control_serve_uds(args).await,
        other => bail!("unknown control subcommand '{other}'"),
    }
}

pub(super) async fn run_control_serve_http(mut args: Vec<String>) -> Result<()> {
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let addr = args
        .first()
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:9173".into());
    serve_http_control_loop(&state_path, &addr).await
}

#[cfg(unix)]
pub(super) async fn run_control_serve_uds(mut args: Vec<String>) -> Result<()> {
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let socket_path = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".mct/control.sock"));
    run_control_serve_uds_with_state(state_path, socket_path).await
}

#[cfg(unix)]
pub(super) async fn run_control_serve_uds_with_state(
    state_path: PathBuf,
    socket_path: PathBuf,
) -> Result<()> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)?;
    println!(
        "mct daemon serving control uds on {}",
        socket_path.display()
    );
    let snapshot_source = ControlSnapshotSource::open(&state_path);
    loop {
        mct_daemon::serve_uds_control_once_with_snapshot_result(
            &listener,
            control_snapshot(&snapshot_source).await,
        )
        .await?;
    }
}

#[cfg(unix)]
pub(super) async fn run_control_serve_uds_with_state_until(
    state_path: PathBuf,
    socket_path: PathBuf,
    config_path: PathBuf,
    children_dir: PathBuf,
    ledger: ResidentLedgerWriter,
    mut shutdown: broadcast::Receiver<()>,
    status_source: Option<Arc<ResidentStatusSource>>,
) -> Result<()> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)?;
    println!(
        "mct daemon serving control uds on {}",
        socket_path.display()
    );
    let snapshot_source = ControlSnapshotSource::open_with_status(&state_path, status_source);
    let mutation_handler =
        resident_observed_mutation_handler(config_path, children_dir, state_path.clone(), ledger);
    loop {
        tokio::select! {
            _ = shutdown.recv() => break,
            result = mct_daemon::serve_uds_control_once_with_snapshot_result_blob_store_and_mutations(
                &listener,
                control_snapshot(&snapshot_source).await,
                Some(&state_path),
                Some(&mutation_handler),
            ) => result?,
        }
    }
    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}

#[cfg(not(unix))]
pub(super) async fn run_control_serve_uds(_args: Vec<String>) -> Result<()> {
    bail!("UDS control plane is only available on Unix platforms")
}

#[cfg(not(unix))]
pub(super) async fn run_control_serve_uds_with_state(
    _state_path: PathBuf,
    _socket_path: PathBuf,
) -> Result<()> {
    bail!("UDS control plane is only available on Unix platforms")
}

#[cfg(not(unix))]
pub(super) async fn run_control_serve_uds_with_state_until(
    _state_path: PathBuf,
    _socket_path: PathBuf,
    _config_path: PathBuf,
    _children_dir: PathBuf,
    _ledger: ResidentLedgerWriter,
    _shutdown: broadcast::Receiver<()>,
    _status_source: Option<Arc<ResidentStatusSource>>,
) -> Result<()> {
    bail!("UDS control plane is only available on Unix platforms")
}

#[derive(Clone)]
pub(super) enum ControlSnapshotSource {
    Store {
        state: Arc<Mutex<MctRuntimeStateStore>>,
        status_source: Option<Arc<ResidentStatusSource>>,
    },
    Unavailable,
}

impl ControlSnapshotSource {
    pub(super) fn open(state_path: &Path) -> Self {
        Self::open_with_status(state_path, None)
    }

    pub(super) fn open_with_status(
        state_path: &Path,
        status_source: Option<Arc<ResidentStatusSource>>,
    ) -> Self {
        match MctRuntimeStateStore::open(state_path)
            .with_context(|| format!("open control runtime state at {}", state_path.display()))
        {
            Ok(state) => Self::Store {
                state: Arc::new(Mutex::new(state)),
                status_source,
            },
            Err(_error) => Self::Unavailable,
        }
    }
}

pub(super) async fn control_snapshot(
    source: &ControlSnapshotSource,
) -> MctControlPlaneSnapshotResult {
    match source {
        ControlSnapshotSource::Unavailable => {
            Err(MctControlPlaneSnapshotError::runtime_state_unavailable())
        }
        ControlSnapshotSource::Store {
            state,
            status_source,
        } => {
            let state = Arc::clone(state);
            let status = resident_or_default_status(status_source.as_ref());
            tokio::task::spawn_blocking(move || {
                let state = state
                    .lock()
                    .map_err(|_| MctControlPlaneSnapshotError::runtime_state_unavailable())?;
                control_snapshot_from_state(&state, status)
                    .map_err(|_source| MctControlPlaneSnapshotError::runtime_state_unavailable())
            })
            .await
            .map_err(|_source| MctControlPlaneSnapshotError::runtime_state_unavailable())?
        }
    }
}

pub(super) fn resident_or_default_status(
    status_source: Option<&Arc<ResidentStatusSource>>,
) -> MctDaemonStatus {
    status_source.map_or_else(|| daemon_status(None), |source| source.status())
}

pub(super) fn control_snapshot_from_state(
    state: &MctRuntimeStateStore,
    status: MctDaemonStatus,
) -> Result<MctControlPlaneSnapshot> {
    let summary = state.summary()?;
    let runs = state.list_runs(20)?;
    Ok(MctControlPlaneSnapshot::new(status, Some(summary), runs))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use mct_observation::DurabilityClass;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn post_mutation(
        listener: Arc<UnixListener>,
        handler: mct_daemon::MctUdsControlMutationHandler,
        socket_path: &Path,
        path: &str,
        body: serde_json::Value,
    ) -> (u16, String) {
        let server = tokio::spawn(async move {
            mct_daemon::serve_uds_control_once_with_snapshot_result_blob_store_and_mutations(
                &listener,
                Err(MctControlPlaneSnapshotError::runtime_state_unavailable()),
                None,
                Some(&handler),
            )
            .await
            .unwrap();
        });
        let body = serde_json::to_vec(&body).unwrap();
        let mut client = tokio::net::UnixStream::connect(socket_path).await.unwrap();
        client
            .write_all(
                format!(
                    "POST {path} HTTP/1.1\r\nHost: local\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        client.write_all(&body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();
        let response = String::from_utf8(response).unwrap();
        let status = response.split_whitespace().nth(1).unwrap().parse().unwrap();
        let body = response.split_once("\r\n\r\n").unwrap().1.to_owned();
        (status, body)
    }

    fn add_request(config_path: &Path, proof: &str) -> serde_json::Value {
        serde_json::json!({
            "expected_config_path": config_path,
            "peer_node_id": "mother-b",
            "binding_id": "binding-a-admits-b",
            "endpoint_id": "endpoint-b",
            "vision_id": "vision-local",
            "ticket": null,
            "binding_signature_ref": proof,
            "policy_revision": 1,
            "expires_at": "2099-01-01T00:00:00Z"
        })
    }

    #[tokio::test]
    async fn live_uds_peer_mutations_are_durable_and_secret_free() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let handler = resident_peer_mutation_handler(config_path.clone(), ledger.clone());

        let requests = [
            (
                "/peers/add",
                add_request(&config_path, "presented-secret-proof"),
            ),
            (
                "/peers/proof",
                serde_json::json!({
                    "expected_config_path": config_path,
                    "peer_node_id": "mother-b",
                    "binding_id": "binding-b-admits-a",
                    "policy_revision": 2,
                    "signature_ref": "outbound-secret-proof",
                    "expires_at": "2099-01-01T00:00:00Z"
                }),
            ),
            (
                "/peers/revoke",
                serde_json::json!({
                    "expected_config_path": config_path,
                    "peer_node_id": "mother-b"
                }),
            ),
            (
                "/peers/remove",
                serde_json::json!({
                    "expected_config_path": config_path,
                    "peer_node_id": "mother-b"
                }),
            ),
        ];
        for (path, body) in requests {
            let (status, response) = post_mutation(
                Arc::clone(&listener),
                handler.clone(),
                &socket_path,
                path,
                body,
            )
            .await;
            assert_eq!(status, 200, "{response}");
            assert!(!response.contains("secret-proof"));
        }
        let (oversized_status, _) = post_mutation(
            Arc::clone(&listener),
            handler.clone(),
            &socket_path,
            "/peers/add",
            serde_json::json!({"padding": "x".repeat(PEER_MUTATION_BODY_MAX_BYTES)}),
        )
        .await;
        assert_eq!(oversized_status, 400);
        drop(handler);
        ledger.close().await;

        assert!(
            MctDaemonConfigStore::new(&config_path)
                .load()
                .unwrap()
                .peers
                .is_empty()
        );
        let reader =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 4);
        assert!(entries.iter().all(|entry| {
            entry.durability_class == DurabilityClass::BeforeEffect
                && entry.observation.outcome == ObservationOutcome::Allowed
        }));
        assert_eq!(
            entries[0].observation.kind,
            ObservationKind::PeerBindingRecorded
        );
        assert_eq!(
            entries[1].observation.kind,
            ObservationKind::PeerBindingRecorded
        );
        assert_eq!(
            entries[2].observation.kind,
            ObservationKind::PeerBindingRevoked
        );
        assert_eq!(
            entries[3].observation.kind,
            ObservationKind::PeerBindingRevoked
        );
        let ledger_text = std::fs::read_to_string(ledger_path).unwrap();
        assert!(!ledger_text.contains("presented-secret-proof"));
        assert!(!ledger_text.contains("outbound-secret-proof"));
    }

    #[tokio::test]
    async fn resident_append_failure_prevents_peer_config_effect() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let socket_path = dir.path().join("control.sock");
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let failed_ledger = ResidentLedgerWriter::failed_for_test();
        let handler = resident_peer_mutation_handler(config_path.clone(), failed_ledger);

        let (status, _) = post_mutation(
            listener,
            handler,
            &socket_path,
            "/peers/add",
            add_request(&config_path, "secret-proof"),
        )
        .await;

        assert_eq!(status, 500);
        assert!(!config_path.exists());
    }

    #[tokio::test]
    async fn resident_apply_failure_records_typed_failure_after_decision() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::create_dir(config_path.with_extension("json.tmp")).unwrap();
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let handler = resident_peer_mutation_handler(config_path.clone(), ledger.clone());

        let (status, response) = post_mutation(
            listener,
            handler.clone(),
            &socket_path,
            "/peers/add",
            add_request(&config_path, "secret-proof"),
        )
        .await;
        assert_eq!(status, 500, "{response}");
        drop(handler);
        ledger.close().await;

        let entries =
            JsonlObservationLedger::open_read_only(ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].observation.kind,
            ObservationKind::PeerBindingRecorded
        );
        assert_eq!(entries[0].observation.outcome, ObservationOutcome::Allowed);
        assert_eq!(
            entries[1].observation.kind,
            ObservationKind::OperatorActionRecorded
        );
        assert_eq!(entries[1].observation.outcome, ObservationOutcome::Failed);
        assert!(!config_path.exists());
    }

    #[test]
    fn offline_peer_mutation_observes_before_effect_and_fails_on_lock_contention() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let ledger_path = dir.path().join("observations.jsonl");
        let missing_socket = dir.path().join("missing.sock");

        run_peers_add(vec![
            "mother-b".into(),
            "binding-a-admits-b".into(),
            "endpoint-b".into(),
            "vision-local".into(),
            "--expires-at".into(),
            "2099-01-01T00:00:00Z".into(),
            "--signature-ref".into(),
            "offline-secret-proof".into(),
            "--config".into(),
            config_path.display().to_string(),
            "--ledger".into(),
            ledger_path.display().to_string(),
            "--uds".into(),
            missing_socket.display().to_string(),
        ])
        .unwrap();
        assert_eq!(
            MctDaemonConfigStore::new(&config_path)
                .load()
                .unwrap()
                .peers
                .len(),
            1
        );
        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("peer_binding_recorded"));
        assert!(!ledger_text.contains("offline-secret-proof"));

        let locked_config = dir.path().join("locked-config.json");
        let _lock =
            JsonlObservationLedger::open(&ledger_path, "ledger-local", "local-mct").unwrap();
        let error = run_peers_add(vec![
            "mother-c".into(),
            "binding-a-admits-c".into(),
            "endpoint-c".into(),
            "vision-local".into(),
            "--expires-at".into(),
            "2099-01-01T00:00:00Z".into(),
            "--signature-ref".into(),
            "secret-proof".into(),
            "--config".into(),
            locked_config.display().to_string(),
            "--ledger".into(),
            ledger_path.display().to_string(),
            "--uds".into(),
            missing_socket.display().to_string(),
        ])
        .unwrap_err();
        assert!(format!("{error:#}").contains("writer lock"));
        assert!(!locked_config.exists());
    }

    #[tokio::test]
    async fn live_child_authority_mutations_are_durable_before_config_effect() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        crate::resident::tests::write_resident_process_child(&children_dir);
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let handler = resident_authority_mutation_handler(
            config_path.clone(),
            children_dir.clone(),
            ledger.clone(),
        );

        for (path, body) in [
            (
                "/children/approve",
                serde_json::json!({
                    "expected_config_path": config_path,
                    "expected_children_dir": children_dir,
                    "child_name": "resident-echo",
                    "strict_integrity": true
                }),
            ),
            (
                "/children/revoke",
                serde_json::json!({
                    "expected_config_path": config_path,
                    "child_name": "resident-echo"
                }),
            ),
        ] {
            let (status, response) = post_mutation(
                Arc::clone(&listener),
                handler.clone(),
                &socket_path,
                path,
                body,
            )
            .await;
            assert_eq!(status, 200, "{response}");
            let entries =
                JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                    .unwrap()
                    .entries()
                    .unwrap();
            assert!(
                entries
                    .iter()
                    .all(|entry| { entry.durability_class == DurabilityClass::BeforeEffect })
            );
        }
        drop(handler);
        ledger.close().await;

        let config = MctDaemonConfigStore::new(&config_path).load().unwrap();
        assert_eq!(
            config.child_approvals["resident-echo"].approval_state,
            ChildApprovalState::Revoked
        );
        assert_eq!(
            config.child_assignments["resident-echo"].assignment_state,
            ChildAssignmentState::Revoked
        );
        let kinds =
            JsonlObservationLedger::open_read_only(ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap()
                .into_iter()
                .map(|entry| entry.observation.kind)
                .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                ObservationKind::ChildApproved,
                ObservationKind::ChildAssigned,
                ObservationKind::ChildRevoked,
                ObservationKind::ChildAssignmentRevoked,
            ]
        );
    }

    #[tokio::test]
    async fn live_child_revocation_is_visible_to_the_immediately_following_route() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        crate::resident::tests::write_resident_process_child(&children_dir);
        let child =
            load_children_from_dir(MctChildLoadOptions::new(&children_dir).strict_integrity())
                .children
                .into_iter()
                .next()
                .unwrap();
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&child, MctOperatorChildScope::default())
            .unwrap();
        let paths = crate::resident::ResidentRuntimePaths::new(
            config_path.clone(),
            children_dir.clone(),
            state_path,
        );
        let call = crate::resident::tests::resident_test_call(
            TraceId::new("trace-live-child-revoke").unwrap(),
        );
        let request = crate::resident::tests::resident_test_protocol_request(call);
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let before = crate::resident::execute_resident_call(
            paths.clone(),
            ledger.clone(),
            request.clone(),
            crate::resident::ResidentPayloadIngress::remote(None),
        )
        .await;
        assert_eq!(before.outcome, CallProtocolOutcome::Completed);

        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let handler =
            resident_authority_mutation_handler(config_path.clone(), children_dir, ledger.clone());
        let (status, response) = post_mutation(
            listener,
            handler.clone(),
            &socket_path,
            "/children/revoke",
            serde_json::json!({
                "expected_config_path": config_path,
                "child_name": "resident-echo"
            }),
        )
        .await;
        assert_eq!(status, 200, "{response}");

        let after = crate::resident::execute_resident_call(
            paths,
            ledger.clone(),
            request,
            crate::resident::ResidentPayloadIngress::remote(None),
        )
        .await;
        assert_eq!(after.outcome, CallProtocolOutcome::Denied);
        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        assert!(entries.iter().any(|entry| {
            entry.observation.kind == ObservationKind::CandidateEliminated
                && entry
                    .observation
                    .detail_ref
                    .as_deref()
                    .is_some_and(|detail| detail.contains("ChildNotApproved"))
        }));
        let mutation_kinds = entries
            .iter()
            .filter_map(|entry| match entry.observation.kind {
                ObservationKind::ChildRevoked | ObservationKind::ChildAssignmentRevoked => {
                    Some(entry.observation.kind)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            mutation_kinds,
            vec![
                ObservationKind::ChildRevoked,
                ObservationKind::ChildAssignmentRevoked,
            ]
        );
        drop(handler);
        ledger.close().await;
    }

    #[tokio::test]
    async fn child_append_failure_prevents_config_effect() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let socket_path = dir.path().join("control.sock");
        crate::resident::tests::write_resident_process_child(&children_dir);
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let failed_ledger = ResidentLedgerWriter::failed_for_test();
        let handler = resident_authority_mutation_handler(
            config_path.clone(),
            children_dir.clone(),
            failed_ledger,
        );

        let (status, _) = post_mutation(
            listener,
            handler,
            &socket_path,
            "/children/approve",
            serde_json::json!({
                "expected_config_path": config_path,
                "expected_children_dir": children_dir,
                "child_name": "resident-echo",
                "strict_integrity": true
            }),
        )
        .await;
        assert_eq!(status, 500);
        assert!(!config_path.exists());
    }

    #[tokio::test]
    async fn live_toy_grants_and_composition_state_are_observed_before_effects() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        crate::resident::tests::write_resident_process_child(&children_dir);
        let child =
            load_children_from_dir(MctChildLoadOptions::new(&children_dir).strict_integrity())
                .children
                .into_iter()
                .next()
                .unwrap();
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&child, MctOperatorChildScope::default())
            .unwrap();
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let handler = resident_observed_mutation_handler(
            config_path.clone(),
            children_dir.clone(),
            state_path.clone(),
            ledger.clone(),
        );

        for (path, body) in [
            (
                "/toys/authorize-slate",
                serde_json::json!({
                    "expected_config_path": config_path,
                    "expected_children_dir": children_dir,
                    "expected_state_path": state_path,
                    "child_name": "resident-echo",
                    "project_root": dir.path()
                }),
            ),
            (
                "/toys/authorize-secret",
                serde_json::json!({
                    "expected_config_path": config_path,
                    "expected_children_dir": children_dir,
                    "expected_state_path": state_path,
                    "child_name": "resident-echo",
                    "secret_name": "api-token-name"
                }),
            ),
            (
                "/pando/record",
                serde_json::json!({
                    "expected_state_path": state_path,
                    "plan": {
                        "composition_id": "composition-observed",
                        "vision_id": "vision-local",
                        "steps": []
                    }
                }),
            ),
        ] {
            let (status, response) = post_mutation(
                Arc::clone(&listener),
                handler.clone(),
                &socket_path,
                path,
                body,
            )
            .await;
            assert_eq!(status, 200, "{response}");
        }
        drop(handler);
        ledger.close().await;

        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        assert_eq!(state.toy_grant_snapshots().unwrap().len(), 5);
        let ledger_text = std::fs::read_to_string(ledger_path).unwrap();
        assert!(ledger_text.contains("toy_grant_allowed"));
        assert!(ledger_text.contains("operator_action_recorded"));
        assert!(!ledger_text.contains("secret-value-material"));
    }

    #[tokio::test]
    async fn administrative_append_failure_and_offline_lock_contention_prevent_state_effects() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        crate::resident::tests::write_resident_process_child(&children_dir);
        let child =
            load_children_from_dir(MctChildLoadOptions::new(&children_dir).strict_integrity())
                .children
                .into_iter()
                .next()
                .unwrap();
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&child, MctOperatorChildScope::default())
            .unwrap();
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let failed_ledger = ResidentLedgerWriter::failed_for_test();
        let handler = resident_observed_mutation_handler(
            config_path.clone(),
            children_dir.clone(),
            state_path.clone(),
            failed_ledger,
        );
        let request = ToyAuthorizeSecretRequest {
            expected_config_path: config_path.clone(),
            expected_children_dir: children_dir.clone(),
            expected_state_path: state_path.clone(),
            child_name: "resident-echo".into(),
            secret_name: "credential-name".into(),
        };
        let (status, _) = post_mutation(
            listener,
            handler,
            &socket_path,
            "/toys/authorize-secret",
            serde_json::to_value(&request).unwrap(),
        )
        .await;
        assert_eq!(status, 500);
        assert!(!state_path.exists());

        execute_offline_administrative_mutation(
            &config_path,
            &children_dir,
            &state_path,
            &ledger_path,
            "/toys/authorize-secret",
            &serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        assert_eq!(
            MctRuntimeStateStore::open(&state_path)
                .unwrap()
                .toy_grant_snapshots()
                .unwrap()
                .len(),
            1
        );

        let locked_state = dir.path().join("locked-state.sqlite");
        let _lock =
            JsonlObservationLedger::open(&ledger_path, "ledger-local", "local-mct").unwrap();
        let error = execute_offline_administrative_mutation(
            Path::new("."),
            Path::new("."),
            &locked_state,
            &ledger_path,
            "/pando/record",
            &serde_json::to_vec(&PandoRecordRequest {
                expected_state_path: locked_state.clone(),
                plan: MctCompositionPlan {
                    composition_id: "locked-composition".into(),
                    vision_id: VisionId::new("vision-local").unwrap(),
                    steps: vec![],
                },
            })
            .unwrap(),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("writer lock"));
        assert!(!locked_state.exists());
    }

    #[tokio::test]
    async fn live_registry_install_and_sync_are_observed_before_storage_effects() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let source_dir = dir.path().join("package");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        crate::resident::tests::write_resident_process_child(&source_dir);
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let handler = resident_observed_mutation_handler(
            config_path,
            children_dir.clone(),
            state_path.clone(),
            ledger.clone(),
        );

        let (install_status, install_response) = post_mutation(
            Arc::clone(&listener),
            handler.clone(),
            &socket_path,
            "/registry/install",
            serde_json::json!({
                "expected_children_dir": children_dir,
                "source_dir": source_dir.join("resident-echo"),
                "replace": false
            }),
        )
        .await;
        assert_eq!(install_status, 200, "{install_response}");
        assert!(children_dir.join("resident-echo").exists());

        let (sync_status, sync_response) = post_mutation(
            Arc::clone(&listener),
            handler.clone(),
            &socket_path,
            "/registry/sync",
            serde_json::json!({
                "expected_children_dir": children_dir,
                "expected_state_path": state_path,
                "source_id": "resident-registry",
                "strict_integrity": false
            }),
        )
        .await;
        assert_eq!(sync_status, 200, "{sync_response}");
        assert_eq!(
            MctRuntimeStateStore::open(&state_path)
                .unwrap()
                .summary()
                .unwrap()
                .artifacts,
            1,
            "{sync_response}"
        );
        drop(handler);
        ledger.close().await;

        let entries =
            JsonlObservationLedger::open_read_only(ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        assert!(
            entries
                .iter()
                .all(|entry| { entry.durability_class == DurabilityClass::BeforeEffect })
        );
        assert!(
            entries
                .iter()
                .any(|entry| { entry.observation.kind == ObservationKind::ArtifactVerified })
        );
        assert!(
            entries
                .iter()
                .any(|entry| { entry.observation.kind == ObservationKind::StorageAppendSucceeded })
        );
        assert!(
            entries
                .iter()
                .any(|entry| { entry.observation.kind == ObservationKind::OperatorActionRecorded })
        );
    }

    #[tokio::test]
    async fn registry_append_failure_prevents_install_and_offline_lock_contention_refuses() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let source_parent = dir.path().join("package");
        let source_dir = source_parent.join("resident-echo");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        crate::resident::tests::write_resident_process_child(&source_parent);
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let failed_ledger = ResidentLedgerWriter::failed_for_test();
        let handler = resident_observed_mutation_handler(
            config_path,
            children_dir.clone(),
            state_path.clone(),
            failed_ledger,
        );
        let request = RegistryInstallRequest {
            expected_children_dir: children_dir.clone(),
            source_dir: source_dir.clone(),
            replace: false,
        };
        let (status, _) = post_mutation(
            listener,
            handler,
            &socket_path,
            "/registry/install",
            serde_json::to_value(&request).unwrap(),
        )
        .await;
        assert_eq!(status, 500);
        assert!(!children_dir.join("resident-echo").exists());

        let _lock =
            JsonlObservationLedger::open(&ledger_path, "ledger-local", "local-mct").unwrap();
        let error = execute_offline_registry_mutation(
            &children_dir,
            &state_path,
            &ledger_path,
            "/registry/install",
            &serde_json::to_vec(&request).unwrap(),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("writer lock"));
        assert!(!children_dir.join("resident-echo").exists());
    }

    #[tokio::test]
    async fn resident_blob_ingest_observes_success_and_typed_rejections_without_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let handler = resident_observed_mutation_handler(
            config_path,
            children_dir,
            state_path.clone(),
            ledger.clone(),
        );
        let payload = b"storage-secret-payload";
        let digest = blake3::hash(payload).to_hex().to_string();

        let requests = [
            serde_json::json!({
                "digest": digest,
                "size_bytes": payload.len(),
                "content_type": "application/octet-stream",
                "classification": "secret",
                "bytes_base64": BASE64_STANDARD.encode(payload)
            }),
            serde_json::json!({
                "digest": "0".repeat(64),
                "size_bytes": payload.len(),
                "content_type": "application/octet-stream",
                "classification": "secret",
                "bytes_base64": BASE64_STANDARD.encode(payload)
            }),
            serde_json::json!({
                "digest": "0".repeat(64),
                "size_bytes": MCT_BLOB_MAX_BYTES as u64 + 1,
                "content_type": "application/octet-stream",
                "classification": "secret",
                "bytes_base64": ""
            }),
        ];
        let expected_statuses = [201, 400, 413];
        for (body, expected_status) in requests.into_iter().zip(expected_statuses) {
            let (status, response) = post_mutation(
                Arc::clone(&listener),
                handler.clone(),
                &socket_path,
                "/blobs",
                body,
            )
            .await;
            assert_eq!(status, expected_status, "{response}");
        }
        drop(handler);
        ledger.close().await;

        let store = local_blob_store_for_state_path(&state_path);
        assert!(store.visible_path(&digest).unwrap().exists());
        assert!(!store.visible_path(&"0".repeat(64)).unwrap().exists());
        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.observation.kind)
                .collect::<Vec<_>>(),
            vec![
                ObservationKind::AdapterEffectStarted,
                ObservationKind::StorageAppendSucceeded,
                ObservationKind::StorageAppendFailed,
                ObservationKind::StorageAppendFailed,
            ]
        );
        let ledger_text = std::fs::read_to_string(ledger_path).unwrap();
        assert!(ledger_text.contains("digest_mismatch"));
        assert!(ledger_text.contains("oversize"));
        assert!(!ledger_text.contains("storage-secret-payload"));
        assert!(!ledger_text.contains(&BASE64_STANDARD.encode(payload)));
    }

    #[tokio::test]
    async fn resident_blob_append_failure_leaves_no_visible_cas_object() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let socket_path = dir.path().join("control.sock");
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let failed_ledger = ResidentLedgerWriter::failed_for_test();
        let handler = resident_observed_mutation_handler(
            config_path,
            children_dir,
            state_path.clone(),
            failed_ledger,
        );
        let payload = b"not-visible";
        let digest = blake3::hash(payload).to_hex().to_string();
        let (status, _) = post_mutation(
            listener,
            handler,
            &socket_path,
            "/blobs",
            serde_json::json!({
                "digest": digest,
                "size_bytes": payload.len(),
                "content_type": "application/octet-stream",
                "classification": "private",
                "bytes_base64": BASE64_STANDARD.encode(payload)
            }),
        )
        .await;
        assert_eq!(status, 500);
        assert!(
            !local_blob_store_for_state_path(state_path)
                .visible_path(&digest)
                .unwrap()
                .exists()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_resident_refuses_identity_rotation_without_offline_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        let listener = Arc::new(UnixListener::bind(&socket_path).unwrap());
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let handler =
            resident_authority_mutation_handler(config_path.clone(), children_dir, ledger.clone());
        let server = tokio::spawn({
            let listener = Arc::clone(&listener);
            let handler = handler.clone();
            async move {
                mct_daemon::serve_uds_control_once_with_snapshot_result_blob_store_and_mutations(
                    &listener,
                    Err(MctControlPlaneSnapshotError::runtime_state_unavailable()),
                    None,
                    Some(&handler),
                )
                .await
                .unwrap();
            }
        });
        let client_socket = socket_path.clone();
        let error = tokio::task::spawn_blocking(move || {
            try_resident_control_mutation(&client_socket, "/identity/ensure", b"{}").unwrap_err()
        })
        .await
        .unwrap();
        server.await.unwrap();

        assert!(format!("{error:#}").contains("stop the daemon to create or rotate identity"));
        assert!(!config_path.exists());
        drop(handler);
        ledger.close().await;
        assert!(
            JsonlObservationLedger::open_read_only(ledger_path, "ledger-local", "local-mct",)
                .unwrap()
                .entries()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn offline_child_and_identity_mutations_hold_the_writer_lock_and_hide_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let identity_path = dir.path().join("node.key");
        let ledger_path = dir.path().join("observations.jsonl");
        crate::resident::tests::write_resident_process_child(&children_dir);

        execute_offline_child_mutation(
            &config_path,
            &children_dir,
            &ledger_path,
            "/children/approve",
            &serde_json::to_vec(&serde_json::json!({
                "expected_config_path": config_path,
                "expected_children_dir": children_dir,
                "child_name": "resident-echo",
                "strict_integrity": true
            }))
            .unwrap(),
        )
        .unwrap();
        execute_offline_identity_mutation(&config_path, &identity_path, &ledger_path).unwrap();
        assert!(identity_path.exists());
        assert!(config_path.exists());

        let secret = std::fs::read_to_string(&identity_path).unwrap();
        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("child_approved"));
        assert!(ledger_text.contains("operator_action_recorded"));
        assert!(!ledger_text.contains(secret.trim()));

        let locked_config = dir.path().join("locked.json");
        let locked_identity = dir.path().join("locked.key");
        let _lock =
            JsonlObservationLedger::open(&ledger_path, "ledger-local", "local-mct").unwrap();
        let error =
            execute_offline_identity_mutation(&locked_config, &locked_identity, &ledger_path)
                .unwrap_err();
        assert!(format!("{error:#}").contains("writer lock"));
        assert!(!locked_config.exists());
        assert!(!locked_identity.exists());
    }
}

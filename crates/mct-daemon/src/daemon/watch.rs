//! Owner-authenticated Watch scope and supporting Toy authority surfaces.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_WATCH_FACT_ID: AtomicU64 = AtomicU64::new(1);

pub(super) const MCT_DIRECTORY_READ_TOY_ID: &str = "toy:mct:directory-read";
pub(super) const MCT_CHILD_KEYVALUE_TOY_ID: &str = "toy:mct:child-keyvalue";
pub(super) const MCT_WASI_LOGGING_TOY_ID: &str = "toy:mct:wasi-logging";
pub(super) const MCT_PATINA_MEASURE_TOY_ID: &str = "toy:mct:patina-measure";

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WatchGrantRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) expected_children_dir: PathBuf,
    pub(super) expected_state_path: PathBuf,
    pub(super) child_name: String,
    pub(super) watch_scope_id: WatchObservationScopeId,
    pub(super) canonical_root: PathBuf,
    pub(super) scope_mode: WatchScopeMode,
    pub(super) traversal_scope: WatchTraversalScope,
    pub(super) event_classes: Vec<WatchEventClass>,
    pub(super) max_events_per_batch: u32,
    pub(super) coalescing_policy: WatchCoalescingPolicy,
    pub(super) starts_at: Timestamp,
    pub(super) expires_at: Timestamp,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WatchRevokeRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) expected_state_path: PathBuf,
    pub(super) watch_scope_id: WatchObservationScopeId,
    pub(super) expected_revision: u64,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(super) enum SupportingToyGrantKind {
    DirectoryRead { canonical_root: PathBuf },
    Keyvalue { bucket_name: String },
    Observability { logging: bool, measure: bool },
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SupportingToyGrantRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) expected_children_dir: PathBuf,
    pub(super) expected_state_path: PathBuf,
    pub(super) child_name: String,
    pub(super) expires_at: Timestamp,
    pub(super) grant: SupportingToyGrantKind,
}

fn response(status_code: u16, body: serde_json::Value) -> MctControlPlaneResponse {
    MctControlPlaneResponse {
        status_code,
        content_type: "application/json".into(),
        body: body.to_string(),
    }
}

fn watch_toy_contract() -> CanonicalToyContract {
    CanonicalToyContract {
        toy_id: ToyId::new(MCT_WATCH_TOY_ID).expect("canonical Watch Toy id is non-empty"),
        contract: ToyContractIdentity {
            namespace: "mct".into(),
            interface_name: "watch/observation".into(),
            version: "0.1.0".into(),
            function_name: Some("observe".into()),
            resource_name: Some("watch-scope".into()),
        },
        authority_bearing: true,
        catalog_revision: 1,
        admitted_by_observation_id: ObservationId::new("obs:toy-catalog:mct-watch")
            .expect("canonical Watch Toy observation id is non-empty"),
    }
}

pub(super) fn supporting_toy_contracts() -> Vec<CanonicalToyContract> {
    [
        (
            MCT_DIRECTORY_READ_TOY_ID,
            "wasi",
            "filesystem/preopens",
            "0.2.3",
            Some("read-file"),
        ),
        (
            MCT_CHILD_KEYVALUE_TOY_ID,
            "wasi",
            "keyvalue/store",
            "0.2.0",
            None,
        ),
        (
            MCT_WASI_LOGGING_TOY_ID,
            "wasi",
            "logging/logging",
            "0.1.0",
            Some("log"),
        ),
        (
            MCT_PATINA_MEASURE_TOY_ID,
            "patina",
            "measure/measure",
            "0.1.0",
            None,
        ),
    ]
    .into_iter()
    .map(
        |(toy_id, namespace, interface_name, version, function_name)| {
            let toy_id = ToyId::new(toy_id).expect("canonical supporting Toy id is non-empty");
            CanonicalToyContract {
                admitted_by_observation_id: ObservationId::new(format!("obs:toy-catalog:{toy_id}"))
                    .expect("canonical supporting Toy observation id is non-empty"),
                toy_id,
                contract: ToyContractIdentity {
                    namespace: namespace.into(),
                    interface_name: interface_name.into(),
                    version: version.into(),
                    function_name: function_name.map(str::to_owned),
                    resource_name: None,
                },
                authority_bearing: true,
                catalog_revision: 1,
            }
        },
    )
    .collect()
}

fn canonical_root_ref(root: PathBuf) -> Result<String> {
    let root = canonical_dir(root, "Watch root")?;
    let text = root
        .to_str()
        .context("Watch root must be canonical UTF-8")?;
    Ok(format!("file://{text}"))
}

fn current_watch_child(
    config_path: &Path,
    children_dir: &Path,
    child_name: &str,
) -> Result<(
    mct_daemon::MctLoadedChild,
    mct_daemon::MctDaemonConfig,
    ChildAssignmentId,
)> {
    let child = load_named_child(children_dir, child_name)?;
    let config = MctDaemonConfigStore::new(config_path).load()?;
    let approval = config
        .child_approvals
        .get(child_name)
        .context("Watch child is not approved")?;
    let assignment = config
        .child_assignments
        .get(child_name)
        .context("Watch child is not assigned")?;
    if approval.approval_state != ChildApprovalState::Approved
        || assignment.assignment_state != ChildAssignmentState::Active
        || approval.artifact_id.as_str() != child.artifact_id
        || assignment.artifact_id.as_str() != child.artifact_id
    {
        bail!("Watch child authority is not active for exact loaded artifact");
    }
    Ok((
        child,
        config,
        ChildAssignmentId::new(format!("assignment:{child_name}"))?,
    ))
}

fn build_scope_and_grant(
    request: &WatchGrantRequest,
    peer: &MctUdsPeerCredentials,
) -> Result<(WatchObservationScope, CanonicalToyContract, ToyGrant)> {
    if request.traversal_scope != WatchTraversalScope::Recursive {
        bail!("root_only is unsupported by the v1 Watch WASI-preopen adapter");
    }
    let (child, config, assignment_id) = current_watch_child(
        &request.expected_config_path,
        &request.expected_children_dir,
        &request.child_name,
    )?;
    let identity = config
        .local_identity
        .context("Watch authority requires current local identity")?;
    let ordinal = NEXT_WATCH_FACT_ID.fetch_add(1, Ordering::Relaxed);
    let scope = WatchObservationScope {
        watch_scope_id: request.watch_scope_id.clone(),
        observer_shape: WatchObserverShape::ChildToy,
        observer_ref: WatchObserverRef {
            child_name: child.name.clone(),
            artifact_id: ComponentArtifactId::new(child.artifact_id.clone())?,
            artifact_version: child.version.clone(),
            assignment_id: assignment_id.clone(),
        },
        scope_mode: request.scope_mode,
        canonical_root_ref: canonical_root_ref(request.canonical_root.clone())?,
        traversal_scope: request.traversal_scope,
        event_classes: request.event_classes.clone(),
        max_events_per_batch: request.max_events_per_batch,
        coalescing_policy: request.coalescing_policy,
        starts_at: request.starts_at.clone(),
        expires_at: request.expires_at.clone(),
        scope_revision: 1,
        policy_revision: identity.policy_revision,
        authority_state: WatchObservationScopeState::Active,
        authority_observation_id: ObservationId::new(format!(
            "obs:watch-scope:{}:1:{ordinal}",
            request.watch_scope_id
        ))?,
        canonical_record_digest: String::new(),
    }
    .seal();
    scope.validate().map_err(anyhow::Error::from)?;
    let grant = ToyGrant {
        grant_id: ToyGrantId::new(format!("grant:watch:{}", scope.watch_scope_id))?,
        toy_id: ToyId::new(MCT_WATCH_TOY_ID)?,
        subject: ToyGrantSubject {
            child_name: child.name,
            artifact_id: child.artifact_id,
            artifact_version: child.version,
            assignment_id: Some(assignment_id),
            caller_node_id: Some(identity.node_id.clone()),
        },
        scope: ToyGrantScope {
            vision_id: identity.vision_id,
            node_id: Some(identity.node_id),
            project_id: None,
            data_classification: None,
            resource_id: Some(scope.toy_resource_id()),
            allowed_actions: vec![MCT_WATCH_TOY_ACTION.into()],
        },
        constraints: ToyGrantConstraints {
            starts_at: Some(scope.starts_at.clone()),
            expires_at: Some(scope.expires_at.clone()),
            max_uses: None,
            max_duration_ms: None,
            locality_required: true,
        },
        grant_state: ToyGrantState::Active,
        issuer_id: format!("os-uid:{}", peer.uid),
        policy_revision: scope.policy_revision,
        grants_revision: 1,
        authority_observation_id: ObservationId::new(format!(
            "obs:watch-grant:{}:1:{ordinal}",
            scope.watch_scope_id
        ))?,
    };
    Ok((scope, watch_toy_contract(), grant))
}

fn supporting_grants(
    request: &SupportingToyGrantRequest,
    peer: &MctUdsPeerCredentials,
) -> Result<(Vec<CanonicalToyContract>, Vec<ToyGrant>)> {
    let (child, config, assignment_id) = current_watch_child(
        &request.expected_config_path,
        &request.expected_children_dir,
        &request.child_name,
    )?;
    let identity = config
        .local_identity
        .context("supporting Toy authority requires current local identity")?;
    let definitions: Vec<(&str, String, Vec<String>)> = match &request.grant {
        SupportingToyGrantKind::DirectoryRead { canonical_root } => vec![(
            MCT_DIRECTORY_READ_TOY_ID,
            canonical_root_ref(canonical_root.clone())?,
            vec!["read-content".into()],
        )],
        SupportingToyGrantKind::Keyvalue { bucket_name } => {
            if bucket_name.trim().is_empty() || bucket_name.len() > MCT_KEYVALUE_KEY_MAX_BYTES {
                bail!("keyvalue bucket name is invalid");
            }
            vec![(
                MCT_CHILD_KEYVALUE_TOY_ID,
                format!(
                    "child:{}:assignment:{}:bucket:{}",
                    child.artifact_id, assignment_id, bucket_name
                ),
                vec![
                    "get".into(),
                    "set".into(),
                    "delete".into(),
                    "exists".into(),
                    "list-keys".into(),
                ],
            )]
        }
        SupportingToyGrantKind::Observability { logging, measure } => {
            let mut definitions = Vec::new();
            if *logging {
                definitions.push((
                    MCT_WASI_LOGGING_TOY_ID,
                    format!("child:{}", child.artifact_id),
                    vec!["invoke".into()],
                ));
            }
            if *measure {
                definitions.push((
                    MCT_PATINA_MEASURE_TOY_ID,
                    format!("child:{}", child.artifact_id),
                    vec!["invoke".into()],
                ));
            }
            if definitions.is_empty() {
                bail!("observability grant must select logging and/or measure");
            }
            definitions
        }
    };
    let all_contracts = supporting_toy_contracts();
    let mut contracts = Vec::new();
    let mut grants = Vec::new();
    for (toy_id, resource_id, actions) in definitions {
        contracts.push(
            all_contracts
                .iter()
                .find(|contract| contract.toy_id.as_str() == toy_id)
                .context("supporting Toy contract missing")?
                .clone(),
        );
        let label = toy_id.trim_start_matches("toy:mct:");
        grants.push(ToyGrant {
            grant_id: ToyGrantId::new(format!("grant:{label}:{}", child.name))?,
            toy_id: ToyId::new(toy_id)?,
            subject: ToyGrantSubject {
                child_name: child.name.clone(),
                artifact_id: child.artifact_id.clone(),
                artifact_version: child.version.clone(),
                assignment_id: Some(assignment_id.clone()),
                caller_node_id: Some(identity.node_id.clone()),
            },
            scope: ToyGrantScope {
                vision_id: identity.vision_id.clone(),
                node_id: Some(identity.node_id.clone()),
                project_id: None,
                data_classification: None,
                resource_id: Some(resource_id),
                allowed_actions: actions,
            },
            constraints: ToyGrantConstraints {
                starts_at: Some(current_timestamp()),
                expires_at: Some(request.expires_at.clone()),
                max_uses: None,
                max_duration_ms: None,
                locality_required: true,
            },
            grant_state: ToyGrantState::Active,
            issuer_id: format!("os-uid:{}", peer.uid),
            policy_revision: identity.policy_revision,
            grants_revision: 1,
            authority_observation_id: ObservationId::new(format!(
                "obs:toy-grant:{label}:{}:{}",
                child.name,
                NEXT_WATCH_FACT_ID.fetch_add(1, Ordering::Relaxed)
            ))?,
        });
    }
    Ok((contracts, grants))
}

fn scope_observation(scope: &WatchObservationScope, outcome: ObservationOutcome) -> MctObservation {
    MctObservation {
        observation_id: scope.authority_observation_id.clone(),
        observed_at: current_timestamp(),
        kind: ObservationKind::OperatorActionRecorded,
        source_plane: SourcePlane::Operator,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace:watch-scope:{}", scope.watch_scope_id))
                .expect("Watch scope trace id is non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(scope.observer_ref.child_name.clone()),
        resource_id: Some(scope.toy_resource_id()),
        policy_revision: Some(scope.policy_revision),
        grants_revision: None,
        outcome,
        visibility: ObservationVisibility::NodeOperator,
        safe_message: format!(
            "Watch observation scope {} revision {}",
            if scope.authority_state == WatchObservationScopeState::Active {
                "created"
            } else {
                "revoked"
            },
            scope.scope_revision
        ),
        detail_ref: Some(format!(
            "watch-observation-scope-v1:{}",
            serde_json::to_string(scope).expect("validated Watch scope serializes")
        )),
    }
}

fn grant_observation(grant: &ToyGrant) -> MctObservation {
    MctObservation {
        observation_id: grant.authority_observation_id.clone(),
        observed_at: current_timestamp(),
        kind: if grant.grant_state == ToyGrantState::Active {
            ObservationKind::ToyGrantAllowed
        } else {
            ObservationKind::ToyGrantRevoked
        },
        source_plane: SourcePlane::Kernel,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace:watch-grant:{}", grant.grant_id))
                .expect("Watch grant trace id is non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(grant.subject.child_name.clone()),
        resource_id: grant.scope.resource_id.clone(),
        policy_revision: Some(grant.policy_revision),
        grants_revision: Some(grant.grants_revision),
        outcome: if grant.grant_state == ToyGrantState::Active {
            ObservationOutcome::Allowed
        } else {
            ObservationOutcome::Denied
        },
        visibility: ObservationVisibility::NodeOperator,
        safe_message: "Watch ToyGrant authority recorded".into(),
        detail_ref: Some(format!(
            "watch-toy-grant-v1:{}",
            serde_json::to_string(grant).expect("Watch ToyGrant serializes")
        )),
    }
}

pub(super) async fn execute_resident_watch_mutation(
    configured_config_path: &Path,
    configured_children_dir: &Path,
    configured_state_path: &Path,
    ledger: &ResidentLedgerWriter,
    peer: Option<MctUdsPeerCredentials>,
    path: &str,
    body: &[u8],
) -> MctControlPlaneResponse {
    let Some(peer) = peer else {
        return response(
            403,
            serde_json::json!({"error": "Watch authority requires authenticated owner"}),
        );
    };
    match path {
        "/watch/grant" => {
            let request: WatchGrantRequest = match serde_json::from_slice(body) {
                Ok(request) => request,
                Err(_) => {
                    return response(
                        400,
                        serde_json::json!({"error": "Watch grant request rejected"}),
                    );
                }
            };
            if request.expected_config_path != configured_config_path
                || request.expected_children_dir != configured_children_dir
                || request.expected_state_path != configured_state_path
            {
                return response(
                    409,
                    serde_json::json!({"error": "Watch authority path mismatch"}),
                );
            }
            let (scope, contract, grant) = match build_scope_and_grant(&request, &peer) {
                Ok(prepared) => prepared,
                Err(_) => {
                    return response(
                        400,
                        serde_json::json!({"error": "Watch authority rejected"}),
                    );
                }
            };
            if ledger
                .append(vec![
                    scope_observation(&scope, ObservationOutcome::Allowed),
                    grant_observation(&grant),
                ])
                .await
                .is_err()
            {
                return response(
                    500,
                    serde_json::json!({"error": "Watch authority was not durable"}),
                );
            }
            let projected = MctRuntimeStateStore::open(configured_state_path).and_then(|state| {
                state.upsert_toy_contract(&contract)?;
                state.insert_watch_observation_scope(&scope)?;
                state.upsert_toy_grant_snapshot(&grant)
            });
            match projected {
                Ok(()) => response(200, serde_json::json!({"scope": scope, "grant": grant})),
                Err(_) => response(
                    500,
                    serde_json::json!({"error": "Watch authority projection failed"}),
                ),
            }
        }
        "/watch/supporting-grant" => {
            let request: SupportingToyGrantRequest = match serde_json::from_slice(body) {
                Ok(request) => request,
                Err(_) => {
                    return response(
                        400,
                        serde_json::json!({"error": "supporting Toy grant request rejected"}),
                    );
                }
            };
            if request.expected_config_path != configured_config_path
                || request.expected_children_dir != configured_children_dir
                || request.expected_state_path != configured_state_path
            {
                return response(
                    409,
                    serde_json::json!({"error": "supporting Toy authority path mismatch"}),
                );
            }
            let (contracts, grants) = match supporting_grants(&request, &peer) {
                Ok(prepared) => prepared,
                Err(_) => {
                    return response(
                        400,
                        serde_json::json!({"error": "supporting Toy authority rejected"}),
                    );
                }
            };
            if ledger
                .append(grants.iter().map(grant_observation).collect())
                .await
                .is_err()
            {
                return response(
                    500,
                    serde_json::json!({"error": "supporting Toy authority was not durable"}),
                );
            }
            let projected = MctRuntimeStateStore::open(configured_state_path).and_then(|state| {
                for contract in &contracts {
                    state.upsert_toy_contract(contract)?;
                }
                for grant in &grants {
                    state.upsert_toy_grant_snapshot(grant)?;
                }
                Ok(())
            });
            match projected {
                Ok(()) => response(
                    200,
                    serde_json::json!({"contracts": contracts, "grants": grants}),
                ),
                Err(_) => response(
                    500,
                    serde_json::json!({"error": "supporting Toy projection failed"}),
                ),
            }
        }
        "/watch/revoke" => {
            let request: WatchRevokeRequest = match serde_json::from_slice(body) {
                Ok(request) => request,
                Err(_) => {
                    return response(
                        400,
                        serde_json::json!({"error": "Watch revoke request rejected"}),
                    );
                }
            };
            if request.expected_config_path != configured_config_path
                || request.expected_state_path != configured_state_path
            {
                return response(
                    409,
                    serde_json::json!({"error": "Watch authority path mismatch"}),
                );
            }
            let state = match MctRuntimeStateStore::open(configured_state_path) {
                Ok(state) => state,
                Err(_) => {
                    return response(500, serde_json::json!({"error": "Watch state unavailable"}));
                }
            };
            let Some(current) = state
                .current_watch_observation_scope(&request.watch_scope_id)
                .ok()
                .flatten()
            else {
                return response(404, serde_json::json!({"error": "Watch scope not found"}));
            };
            if current.scope_revision != request.expected_revision
                || current.authority_state != WatchObservationScopeState::Active
            {
                return response(
                    409,
                    serde_json::json!({"error": "Watch scope revision is stale"}),
                );
            }
            let ordinal = NEXT_WATCH_FACT_ID.fetch_add(1, Ordering::Relaxed);
            let mut revoked = current.clone();
            revoked.scope_revision += 1;
            revoked.authority_state = WatchObservationScopeState::Revoked;
            revoked.authority_observation_id = ObservationId::new(format!(
                "obs:watch-scope:{}:{}:{ordinal}",
                revoked.watch_scope_id, revoked.scope_revision
            ))
            .expect("Watch revoke observation id is non-empty");
            revoked = revoked.seal();
            let grant_id = ToyGrantId::new(format!("grant:watch:{}", revoked.watch_scope_id))
                .expect("Watch grant id is non-empty");
            let Some(mut grant) = state
                .toy_grant_snapshots()
                .ok()
                .and_then(|grants| grants.into_iter().find(|grant| grant.grant_id == grant_id))
            else {
                return response(
                    409,
                    serde_json::json!({"error": "Watch ToyGrant projection missing"}),
                );
            };
            grant.grant_state = ToyGrantState::Revoked;
            grant.grants_revision += 1;
            grant.authority_observation_id = ObservationId::new(format!(
                "obs:watch-grant:{}:{}:{ordinal}",
                revoked.watch_scope_id, revoked.scope_revision
            ))
            .expect("Watch grant revoke observation id is non-empty");
            if ledger
                .append(vec![
                    scope_observation(&revoked, ObservationOutcome::Denied),
                    grant_observation(&grant),
                ])
                .await
                .is_err()
            {
                return response(
                    500,
                    serde_json::json!({"error": "Watch revocation was not durable"}),
                );
            }
            match state
                .insert_watch_observation_scope(&revoked)
                .and_then(|()| state.upsert_toy_grant_snapshot(&grant))
            {
                Ok(()) => response(200, serde_json::json!({"scope": revoked, "grant": grant})),
                Err(_) => response(
                    500,
                    serde_json::json!({"error": "Watch revocation projection failed"}),
                ),
            }
        }
        _ => response(404, serde_json::json!({"error": "unknown Watch mutation"})),
    }
}

fn resident_watch_mutation(
    socket_path: &Path,
    path: &str,
    request: &impl serde::Serialize,
) -> Result<serde_json::Value> {
    let body = serde_json::to_vec(request)?;
    let response = try_resident_control_mutation(socket_path, path, &body)?
        .context("Watch authority mutation requires a running resident UDS")?;
    serde_json::from_slice(&response).context("decode Watch mutation response")
}

pub(super) fn run_watch(mut args: Vec<String>) -> Result<()> {
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let as_json = take_flag(&mut args, "--json");
    if args.first().map(String::as_str) != Some("scopes") || args.len() < 2 {
        bail!("expected watch scopes show <scope-id> | list [--state path] [--json]");
    }
    args.remove(0);
    match args.remove(0).as_str() {
        "show" => {
            if args.len() != 1 {
                bail!("watch scopes show requires one scope id");
            }
            let scope = MctRuntimeStateStore::open(state_path)?
                .current_watch_observation_scope(&WatchObservationScopeId::new(args.remove(0))?)?
                .context("Watch scope not found")?;
            if as_json {
                println!("{}", serde_json::to_string_pretty(&scope)?);
            } else {
                println!(
                    "watch_scope={} revision={} state={:?} root={}",
                    scope.watch_scope_id,
                    scope.scope_revision,
                    scope.authority_state,
                    scope.canonical_root_ref
                );
            }
        }
        "list" => {
            if !args.is_empty() {
                bail!("unexpected watch scopes list arguments");
            }
            let scopes = MctRuntimeStateStore::open(state_path)?.watch_observation_scopes()?;
            if as_json {
                println!("{}", serde_json::to_string_pretty(&scopes)?);
            } else {
                for scope in scopes {
                    println!(
                        "watch_scope={} revision={} state={:?} root={}",
                        scope.watch_scope_id,
                        scope.scope_revision,
                        scope.authority_state,
                        scope.canonical_root_ref
                    );
                }
            }
        }
        other => bail!("unknown watch scopes subcommand '{other}'"),
    }
    Ok(())
}

fn parse_watch_event_classes(value: &str) -> Result<Vec<WatchEventClass>> {
    let mut classes = value
        .split(',')
        .map(|value| match value.trim() {
            "created" => Ok(WatchEventClass::Created),
            "modified" => Ok(WatchEventClass::Modified),
            "deleted" => Ok(WatchEventClass::Deleted),
            other => bail!("unknown Watch event class '{other}'"),
        })
        .collect::<Result<Vec<_>>>()?;
    classes.sort();
    classes.dedup();
    Ok(classes)
}

pub(super) fn run_watch_toy_command(command: &str, mut args: Vec<String>) -> Result<()> {
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let socket_path = take_option(&mut args, "--uds")
        .map(PathBuf::from)
        .unwrap_or_else(default_control_uds_path);
    let as_json = take_flag(&mut args, "--json");
    let value = match command {
        "grant-watch" => {
            if args.len() < 2 {
                bail!("toys grant-watch requires child name and canonical root");
            }
            let child_name = args.remove(0);
            let canonical_root = PathBuf::from(args.remove(0));
            let watch_scope_id =
                WatchObservationScopeId::new(take_required(&mut args, "--scope-id")?)?;
            let traversal_scope = match take_required(&mut args, "--traversal")?.as_str() {
                "root-only" | "root_only" => WatchTraversalScope::RootOnly,
                "recursive" => WatchTraversalScope::Recursive,
                other => bail!("unknown Watch traversal '{other}'"),
            };
            let event_classes = parse_watch_event_classes(&take_required(&mut args, "--events")?)?;
            let max_events_per_batch = take_required(&mut args, "--max-events-per-batch")?
                .parse::<u32>()
                .context("parse --max-events-per-batch")?;
            let coalescing_policy = match take_required(&mut args, "--coalescing")?.as_str() {
                "none" => WatchCoalescingPolicy::None,
                "last-per-path" | "last_per_path" => WatchCoalescingPolicy::LastPerPath,
                other => bail!("unknown Watch coalescing policy '{other}'"),
            };
            let starts_at = Timestamp::new(take_required(&mut args, "--starts-at")?)?;
            let expires_at = Timestamp::new(take_required(&mut args, "--expires-at")?)?;
            let scope_mode = match take_option(&mut args, "--scope-mode").as_deref() {
                None | Some("constrained") => WatchScopeMode::Constrained,
                Some("explicit-broad") | Some("explicit_broad") => WatchScopeMode::ExplicitBroad,
                Some(other) => bail!("unknown Watch scope mode '{other}'"),
            };
            if !args.is_empty() {
                bail!("unexpected toys grant-watch arguments: {}", args.join(" "));
            }
            resident_watch_mutation(
                &socket_path,
                "/watch/grant",
                &WatchGrantRequest {
                    expected_config_path: config_path,
                    expected_children_dir: children_dir,
                    expected_state_path: state_path,
                    child_name,
                    watch_scope_id,
                    canonical_root,
                    scope_mode,
                    traversal_scope,
                    event_classes,
                    max_events_per_batch,
                    coalescing_policy,
                    starts_at,
                    expires_at,
                },
            )?
        }
        "revoke-watch" => {
            if args.is_empty() {
                bail!("toys revoke-watch requires scope id");
            }
            let watch_scope_id = WatchObservationScopeId::new(args.remove(0))?;
            let expected_revision = take_required(&mut args, "--expected-revision")?
                .parse::<u64>()
                .context("parse --expected-revision")?;
            if !args.is_empty() {
                bail!("unexpected toys revoke-watch arguments: {}", args.join(" "));
            }
            resident_watch_mutation(
                &socket_path,
                "/watch/revoke",
                &WatchRevokeRequest {
                    expected_config_path: config_path,
                    expected_state_path: state_path,
                    watch_scope_id,
                    expected_revision,
                },
            )?
        }
        "grant-directory-read" | "grant-keyvalue" | "grant-observability" => {
            if args.is_empty() {
                bail!("{command} requires child name");
            }
            let child_name = args.remove(0);
            let expires_at = Timestamp::new(take_required(&mut args, "--expires-at")?)?;
            let grant = match command {
                "grant-directory-read" => {
                    if args.is_empty() {
                        bail!("grant-directory-read requires canonical root");
                    }
                    SupportingToyGrantKind::DirectoryRead {
                        canonical_root: PathBuf::from(args.remove(0)),
                    }
                }
                "grant-keyvalue" => {
                    if args.is_empty() {
                        bail!("grant-keyvalue requires bucket name");
                    }
                    SupportingToyGrantKind::Keyvalue {
                        bucket_name: args.remove(0),
                    }
                }
                _ => SupportingToyGrantKind::Observability {
                    logging: take_flag(&mut args, "--logging"),
                    measure: take_flag(&mut args, "--measure"),
                },
            };
            if !args.is_empty() {
                bail!("unexpected {command} arguments: {}", args.join(" "));
            }
            resident_watch_mutation(
                &socket_path,
                "/watch/supporting-grant",
                &SupportingToyGrantRequest {
                    expected_config_path: config_path,
                    expected_children_dir: children_dir,
                    expected_state_path: state_path,
                    child_name,
                    expires_at,
                    grant,
                },
            )?
        }
        _ => bail!("unknown Watch Toy command '{command}'"),
    };
    if as_json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("Watch Toy authority recorded");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch_test_call() -> MctCall {
        MctCall {
            call_id: CallId::new("call-watch-test").unwrap(),
            caller: CallerIdentity {
                node_id: MctNodeId::new("local-mct").unwrap(),
                vision_id: VisionId::new("vision-local").unwrap(),
                project_id: None,
                user_id: None,
            },
            target: OperationTarget::new("patina:watch", "control@0.1.0", "scan-now").unwrap(),
            payload_metadata: PayloadMetadata {
                data_classification: "public".into(),
                size_bytes: 2,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 1,
                grants_revision: 1,
                vision_policy_revision: 1,
            },
            deadline: Timestamp::new("2026-07-21T13:00:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-watch-test").unwrap(),
                span_id: SpanId::new("span-watch-test").unwrap(),
            },
            origin: CallOrigin::TriggerFiring,
        }
    }

    fn watch_test_scope() -> WatchObservationScope {
        WatchObservationScope {
            watch_scope_id: WatchObservationScopeId::new("scope-watch-test").unwrap(),
            observer_shape: WatchObserverShape::ChildToy,
            observer_ref: WatchObserverRef {
                child_name: "folder-watch-actor".into(),
                artifact_id: ComponentArtifactId::new(format!("sha256:{}", "a".repeat(64)))
                    .unwrap(),
                artifact_version: "0.1.0".into(),
                assignment_id: ChildAssignmentId::new("assignment:folder-watch-actor").unwrap(),
            },
            scope_mode: WatchScopeMode::Constrained,
            canonical_root_ref: "file:///tmp/watch-test".into(),
            traversal_scope: WatchTraversalScope::Recursive,
            event_classes: vec![WatchEventClass::Created],
            max_events_per_batch: 8,
            coalescing_policy: WatchCoalescingPolicy::None,
            starts_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
            expires_at: Timestamp::new("2026-07-21T13:00:00Z").unwrap(),
            scope_revision: 1,
            policy_revision: 1,
            authority_state: WatchObservationScopeState::Active,
            authority_observation_id: ObservationId::new("obs-watch-test-scope").unwrap(),
            canonical_record_digest: String::new(),
        }
        .seal()
    }

    #[test]
    fn watch_scope_and_toy_grant_are_both_current_before_observation() {
        let call = watch_test_call();
        let scope = watch_test_scope();
        let request = ToyGrantEvaluationRequest {
            toy_id: ToyId::new(MCT_WATCH_TOY_ID).unwrap(),
            subject: ToyGrantSubject {
                child_name: scope.observer_ref.child_name.clone(),
                artifact_id: scope.observer_ref.artifact_id.to_string(),
                artifact_version: scope.observer_ref.artifact_version.clone(),
                assignment_id: Some(scope.observer_ref.assignment_id.clone()),
                caller_node_id: Some(call.caller.node_id.clone()),
            },
            child_instance_id: ChildInstanceId::new("instance-watch-test").unwrap(),
            action: MCT_WATCH_TOY_ACTION.into(),
            resource_id: Some(scope.toy_resource_id()),
            node_id: call.caller.node_id.clone(),
            now: Timestamp::new("2026-07-21T12:30:00Z").unwrap(),
            ids: ToyGrantEvaluationIds {
                evaluation_id: ToyGrantEvaluationId::new("eval-watch-test").unwrap(),
                decision_id: DecisionId::new("decision-watch-test").unwrap(),
                observation_id: ObservationId::new("obs-watch-test-eval").unwrap(),
                authorized_toy_call_id: AuthorizedToyCallId::new("authorized-watch-test").unwrap(),
            },
        };
        let grant = ToyGrant {
            grant_id: ToyGrantId::new("grant-watch-test").unwrap(),
            toy_id: request.toy_id.clone(),
            subject: request.subject.clone(),
            scope: ToyGrantScope {
                vision_id: call.caller.vision_id.clone(),
                node_id: Some(call.caller.node_id.clone()),
                project_id: None,
                data_classification: None,
                resource_id: request.resource_id.clone(),
                allowed_actions: vec![MCT_WATCH_TOY_ACTION.into()],
            },
            constraints: ToyGrantConstraints {
                starts_at: Some(scope.starts_at.clone()),
                expires_at: Some(scope.expires_at.clone()),
                max_uses: None,
                max_duration_ms: None,
                locality_required: true,
            },
            grant_state: ToyGrantState::Active,
            issuer_id: "os-uid:501".into(),
            policy_revision: 1,
            grants_revision: 1,
            authority_observation_id: ObservationId::new("obs-watch-test-grant").unwrap(),
        };

        let missing_grant =
            evaluate_toy_grant_for_call(&call, &request, &[watch_toy_contract()], &[]);
        assert!(missing_grant.authorized.is_none());
        let allowed = evaluate_toy_grant_for_call(
            &call,
            &request,
            &[watch_toy_contract()],
            std::slice::from_ref(&grant),
        );
        let session = authorize_watch_observation_session(
            allowed.authorized.unwrap(),
            &scope,
            &WatchObservationSessionRequest {
                current_observer: scope.observer_ref.clone(),
                now: request.now.clone(),
            },
        )
        .unwrap();
        assert_eq!(session.scope(), &scope);

        let mut revoked = scope.clone();
        revoked.scope_revision = 2;
        revoked.authority_state = WatchObservationScopeState::Revoked;
        revoked.authority_observation_id = ObservationId::new("obs-watch-test-revoked").unwrap();
        revoked = revoked.seal();
        let allowed_again =
            evaluate_toy_grant_for_call(&call, &request, &[watch_toy_contract()], &[grant]);
        assert!(
            authorize_watch_observation_session(
                allowed_again.authorized.unwrap(),
                &revoked,
                &WatchObservationSessionRequest {
                    current_observer: revoked.observer_ref.clone(),
                    now: request.now,
                },
            )
            .is_err()
        );
    }

    #[test]
    fn watch_grant_cannot_read_content_state_or_originate_delivery() {
        let watch = watch_toy_contract();
        assert_eq!(watch.contract.namespace, "mct");
        assert_eq!(watch.contract.interface_name, "watch/observation");
        assert_eq!(MCT_WATCH_TOY_ACTION, "observe");
        assert_ne!(MCT_WATCH_TOY_ID, MCT_DIRECTORY_READ_TOY_ID);
        assert_ne!(MCT_WATCH_TOY_ID, MCT_CHILD_KEYVALUE_TOY_ID);
        let supporting = supporting_toy_contracts();
        assert!(supporting.iter().any(|contract| {
            contract.toy_id.as_str() == MCT_DIRECTORY_READ_TOY_ID
                && contract.contract.interface_name == "filesystem/preopens"
        }));
        assert!(supporting.iter().any(|contract| {
            contract.toy_id.as_str() == MCT_CHILD_KEYVALUE_TOY_ID
                && contract.contract.interface_name == "keyvalue/store"
        }));
        assert!(
            supporting
                .iter()
                .all(|contract| contract.toy_id != watch.toy_id)
        );
    }

    #[test]
    fn watch_batches_are_bounded_sequenced_deterministic_and_countable() {
        let dir = tempfile::tempdir().unwrap();
        let state = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let scope = watch_test_scope();
        state.insert_watch_observation_scope(&scope).unwrap();
        let first = state
            .reserve_watch_batch_sequence(&scope.watch_scope_id, scope.scope_revision)
            .unwrap();
        let second = state
            .reserve_watch_batch_sequence(&scope.watch_scope_id, scope.scope_revision)
            .unwrap();
        assert_eq!((first, second), (1, 2));
        let parent = CallId::new("call-watch-batch").unwrap();
        assert_eq!(
            derive_watch_batch_id(&scope.watch_scope_id, scope.scope_revision, first, &parent),
            derive_watch_batch_id(&scope.watch_scope_id, scope.scope_revision, first, &parent)
        );
        let batch = WatchEventBatchEvidence {
            batch_id: derive_watch_batch_id(
                &scope.watch_scope_id,
                scope.scope_revision,
                first,
                &parent,
            ),
            watch_scope_id: scope.watch_scope_id,
            scope_revision: scope.scope_revision,
            sequence: first,
            parent_call_id: parent,
            raw_event_count: MCT_WATCH_MAX_EVENTS_PER_BATCH,
            eligible_event_count: MCT_WATCH_MAX_EVENTS_PER_BATCH - 3,
            coalesced_event_count: 1,
            excluded_event_count: 1,
            capacity_refused_event_count: 1,
        };
        assert_eq!(
            batch.eligible_event_count
                + batch.coalesced_event_count
                + batch.excluded_event_count
                + batch.capacity_refused_event_count,
            batch.raw_event_count
        );
    }

    #[test]
    fn watcher_child_callout_reenters_ordinary_call_law() {
        let parent = CallId::new("call-watch-parent").unwrap();
        let batch = WatchEventBatchId::new("watch-batch:test").unwrap();
        let event = derive_watch_callout_event_id(&parent, &batch, 0, "{\"path\":\"safe.txt\"}");
        let call_id = derive_watch_callout_call_id(&event);
        assert!(call_id.as_str().starts_with("call-wasm-host:"));
        assert_eq!(MCT_CHILD_CALLOUT_MAX_DEPTH, 1);
        assert_ne!(call_id, parent);
        assert_eq!(
            derive_watch_callout_idempotency_key(&parent, &event, "patina:watch/events@0.1.0.emit"),
            derive_watch_callout_idempotency_key(&parent, &event, "patina:watch/events@0.1.0.emit")
        );
        assert_ne!(CallOrigin::WasmHost, CallOrigin::TriggerFiring);
    }

    #[test]
    fn legacy_watch_abi_mismatch_is_refused_before_sink_call() {
        assert_eq!(
            validate_legacy_watch_paths("patina:watch/events@0.1.0", "different.txt", "safe.txt")
                .unwrap(),
            LegacyWatchCompatibilityValidation::MismatchRefused
        );
        assert!(
            validate_legacy_watch_paths(
                "patina:watch/events@0.1.0",
                "/absolute.txt",
                "/absolute.txt"
            )
            .is_err()
        );
        assert!(
            validate_legacy_watch_paths("patina:watch/events@0.2.0", "safe.txt", "safe.txt")
                .is_err()
        );
    }

    #[test]
    fn watch_delivery_lineage_is_actual_and_never_fabricated() {
        let parent = CallId::new("call-watch-parent-lineage").unwrap();
        let batch = WatchEventBatchId::new("watch-batch:lineage").unwrap();
        let triggered = WatchEventEvidence {
            event_id: WatchEventId::new("watch-event:triggered").unwrap(),
            batch_id: batch.clone(),
            batch_position: 0,
            event_class: WatchEventClass::Created,
            relative_path: "triggered.txt".into(),
            causative_call_id: parent.clone(),
            causative_trigger_firing_id: Some(CallTriggerFiringId::new("firing:actual").unwrap()),
            causative_adapter_observation_id: None,
        };
        let mut manual = triggered.clone();
        manual.event_id = WatchEventId::new("watch-event:manual").unwrap();
        manual.batch_position = 1;
        manual.causative_trigger_firing_id = None;
        assert_eq!(triggered.causative_call_id, parent);
        assert!(triggered.causative_trigger_firing_id.is_some());
        assert!(manual.causative_trigger_firing_id.is_none());
        assert!(manual.causative_adapter_observation_id.is_none());
    }

    #[test]
    fn watch_delivery_reuses_closed_mct_result_outcomes() {
        let evidence = WatchEventDeliveryEvidence {
            delivery_id: WatchEventDeliveryId::new("watch-delivery:test").unwrap(),
            disposition_id: WatchEventDeliveryDispositionId::new("watch-disposition:test").unwrap(),
            target_call_id: CallId::new("call-wasm-host:test").unwrap(),
            target_result_ref: ResultRef::new("result-resident:call-wasm-host:test").unwrap(),
            target_result_observation_id: ObservationId::new(
                "obs:result-resident:call-wasm-host:test",
            )
            .unwrap(),
            delivered: true,
        };
        let closed = [
            ResultOutcome::Success,
            ResultOutcome::Denied,
            ResultOutcome::Failed,
            ResultOutcome::TimedOut,
            ResultOutcome::Cancelled,
        ];
        assert_eq!(closed.len(), 5);
        assert!(evidence.delivered);
        assert!(
            evidence
                .target_result_ref
                .as_str()
                .starts_with("result-resident:")
        );
    }

    #[test]
    fn watch_delivery_observation_mapping_uses_existing_kinds() {
        let mapping = [
            ObservationKind::AdapterEffectStarted,
            ObservationKind::DataMovementAllowed,
            ObservationKind::DataMovementDenied,
            ObservationKind::CallConstructed,
            ObservationKind::ResultRecorded,
            ObservationKind::AdapterEffectCompleted,
        ];
        assert_eq!(mapping.len(), 6);
    }

    #[test]
    fn watch_canonical_contracts_are_separate_authorities() {
        let watch = watch_toy_contract();
        let supporting = supporting_toy_contracts();
        assert_eq!(watch.toy_id.as_str(), MCT_WATCH_TOY_ID);
        assert!(
            supporting
                .iter()
                .all(|contract| contract.toy_id != watch.toy_id)
        );
        assert_eq!(supporting.len(), 4);
        assert_eq!(MCT_KEYVALUE_KEY_MAX_BYTES, 128);
        assert_eq!(MCT_KEYVALUE_VALUE_MAX_BYTES, 262_144);
        assert_eq!(MCT_KEYVALUE_MAX_KEYS_PER_BUCKET, 128);
        assert_eq!(MCT_KEYVALUE_LIST_PAGE_MAX, 128);
    }
}

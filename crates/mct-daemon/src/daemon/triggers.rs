//! Owner-authenticated trigger authority management and CLI projection.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

const TRIGGER_MUTATION_BODY_MAX_BYTES: usize = 64 * 1024;
static NEXT_TRIGGER_FACT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TriggerAuthorityScopeRequest {
    pub(super) trigger_authority_id: CallTriggerAuthorityId,
    pub(super) target: OperationTarget,
    pub(super) payload_constraint: MctCallPayloadHandle,
    pub(super) trigger_source: CallTriggerSource,
    #[serde(default)]
    pub(super) missed_fire_policy: MissedFirePolicy,
    #[serde(default)]
    pub(super) overlap_policy: OverlapPolicy,
    pub(super) starts_at: Timestamp,
    pub(super) expires_at: Timestamp,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TriggerCreateRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) expected_state_path: PathBuf,
    pub(super) scope: TriggerAuthorityScopeRequest,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TriggerReviseRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) expected_state_path: PathBuf,
    pub(super) expected_revision: u64,
    pub(super) scope: TriggerAuthorityScopeRequest,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TriggerRevokeRequest {
    pub(super) expected_config_path: PathBuf,
    pub(super) expected_state_path: PathBuf,
    pub(super) trigger_authority_id: CallTriggerAuthorityId,
    pub(super) expected_revision: u64,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct TriggerBlobIngestRequest {
    digest: String,
    size_bytes: u64,
    content_type: String,
    classification: String,
    bytes_base64: String,
}

#[derive(Clone, Debug)]
struct PreparedTriggerMutation {
    state_path: PathBuf,
    authority: CallTriggerAuthority,
    observation: MctObservation,
}

fn trigger_response(status_code: u16, value: impl serde::Serialize) -> MctControlPlaneResponse {
    MctControlPlaneResponse {
        status_code,
        content_type: "application/json".into(),
        body: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "null".into()),
    }
}

fn require_trigger_path(expected: &Path, configured: &Path, label: &str) -> Result<()> {
    if expected != configured {
        bail!("trigger {label} path does not match resident configuration");
    }
    Ok(())
}

fn source_ref(source: &CallTriggerSource) -> String {
    let bytes = serde_json::to_vec(source).expect("closed trigger source must serialize");
    format!("blake3:{}", blake3::hash(&bytes).to_hex())
}

fn trigger_target_is_deferred(target: &OperationTarget) -> bool {
    let target = format!(
        "{}/{}/{}",
        target.namespace, target.interface_name, target.function_name
    )
    .to_ascii_lowercase();
    (target.contains("registry") && target.contains("sync"))
        || target.contains("network-artifact-acquisition")
        || target.contains("artifact/acquire")
}

fn trigger_fact_id(prefix: &str, authority_id: &CallTriggerAuthorityId, revision: u64) -> String {
    let sequence = NEXT_TRIGGER_FACT_ID.fetch_add(1, Ordering::Relaxed);
    format!(
        "{prefix}:{authority_id}:{revision}:{}:{sequence}",
        current_timestamp()
    )
}

fn trigger_authority_observation(authority: &CallTriggerAuthority, uid: u32) -> MctObservation {
    let action = match authority.authority_state {
        CallTriggerAuthorityState::Active if authority.record_revision == 1 => "created",
        CallTriggerAuthorityState::Active => "revised",
        CallTriggerAuthorityState::Revoked => "revoked",
        CallTriggerAuthorityState::Superseded => "superseded",
    };
    MctObservation {
        observation_id: authority.authority_observation_id.clone(),
        observed_at: current_timestamp(),
        kind: ObservationKind::OperatorActionRecorded,
        source_plane: SourcePlane::Operator,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(trigger_fact_id(
                "trace-trigger-authority",
                &authority.trigger_authority_id,
                authority.record_revision,
            ))
            .expect("generated trigger trace id must be non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: Some(
            DecisionId::new(trigger_fact_id(
                "decision-trigger-authority",
                &authority.trigger_authority_id,
                authority.record_revision,
            ))
            .expect("generated trigger decision id must be non-empty"),
        ),
        subject_id: Some(format!(
            "uid-blake3:{}",
            blake3::hash(uid.to_string().as_bytes()).to_hex()
        )),
        resource_id: Some(authority.trigger_authority_id.to_string()),
        policy_revision: Some(authority.policy_revision),
        grants_revision: None,
        outcome: if authority.authority_state == CallTriggerAuthorityState::Revoked {
            ObservationOutcome::Denied
        } else {
            ObservationOutcome::Allowed
        },
        visibility: ObservationVisibility::NodeOperator,
        safe_message: format!("trigger authority {action}"),
        detail_ref: Some(format!(
            "call-trigger-authority-v1:{}",
            serde_json::to_string(authority)
                .expect("validated trigger authority must serialize for evidence")
        )),
    }
}

fn trigger_projection_failure_observation(authority: &CallTriggerAuthority) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::new(trigger_fact_id(
            "obs-trigger-projection-failed",
            &authority.trigger_authority_id,
            authority.record_revision,
        ))
        .expect("generated trigger failure observation id must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::StorageAppendFailed,
        source_plane: SourcePlane::Storage,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(trigger_fact_id(
                "trace-trigger-projection-failed",
                &authority.trigger_authority_id,
                authority.record_revision,
            ))
            .expect("generated trigger failure trace id must be non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(authority.trigger_authority_id.to_string()),
        resource_id: Some(authority.canonical_record_digest.clone()),
        policy_revision: Some(authority.policy_revision),
        grants_revision: None,
        outcome: ObservationOutcome::Failed,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: "trigger authority projection failed".into(),
        detail_ref: Some(format!(
            "trigger_authority_id:{};record_revision:{}",
            authority.trigger_authority_id, authority.record_revision
        )),
    }
}

fn verified_trigger_payload(state_path: &Path, handle: &MctCallPayloadHandle) -> Result<()> {
    let MctCallPayloadHandle::ContentAddressedBlob { .. } = handle else {
        bail!("trigger payload must be a local content-addressed blob");
    };
    local_blob_store_for_state_path(state_path)
        .fetch(handle)
        .map(|_| ())
        .map_err(anyhow::Error::from)
        .context("verify trigger payload in local CAS")
}

#[allow(clippy::too_many_arguments)] // Derived identity/revision facts are deliberately explicit.
fn build_trigger_authority(
    config_path: &Path,
    state_path: &Path,
    uid: u32,
    scope: TriggerAuthorityScopeRequest,
    record_revision: u64,
    policy_revision: u64,
    authority_state: CallTriggerAuthorityState,
    verify_payload: bool,
) -> Result<CallTriggerAuthority> {
    if scope.trigger_source.class() == CallTriggerClass::Event {
        bail!(
            "MotherEventSourceAdapterRuntime is not implemented; use temporal triggers or return to D1"
        );
    }
    if trigger_target_is_deferred(&scope.target) {
        bail!("registry sync and network artifact acquisition trigger composition is deferred");
    }
    if verify_payload {
        verified_trigger_payload(state_path, &scope.payload_constraint)?;
    }
    let config = MctDaemonConfigStore::new(config_path).load()?;
    let identity = config
        .local_identity
        .context("trigger authority requires current local identity")?;
    let authority_observation_id = ObservationId::new(trigger_fact_id(
        "obs-trigger-authority",
        &scope.trigger_authority_id,
        record_revision,
    ))?;
    let authority = CallTriggerAuthority {
        trigger_authority_id: scope.trigger_authority_id,
        mother_node_id: identity.node_id.clone(),
        vision_id: identity.vision_id.clone(),
        canonical_caller: CallerIdentity {
            node_id: identity.node_id,
            user_id: Some(UserId::new(format!("uid:{uid}"))?),
            vision_id: identity.vision_id,
            project_id: None,
        },
        target: scope.target,
        payload_constraint: scope.payload_constraint,
        trigger_source_ref: source_ref(&scope.trigger_source),
        trigger_source: scope.trigger_source,
        missed_fire_policy: scope.missed_fire_policy,
        overlap_policy: scope.overlap_policy,
        issuer_principal_ref: format!("os-uid:{uid}"),
        record_revision,
        policy_revision,
        starts_at: scope.starts_at,
        expires_at: scope.expires_at,
        authority_state,
        authority_observation_id,
        canonical_record_digest: String::new(),
    }
    .seal();
    authority.validate().map_err(anyhow::Error::from)?;
    Ok(authority)
}

fn prepare_trigger_mutation(
    config_path: &Path,
    state_path: &Path,
    uid: u32,
    path: &str,
    body: &[u8],
) -> Result<PreparedTriggerMutation> {
    if body.len() > TRIGGER_MUTATION_BODY_MAX_BYTES {
        bail!("trigger authority mutation body exceeds 64 KiB limit");
    }
    let authority = match path {
        "/triggers/create" => {
            let request: TriggerCreateRequest =
                serde_json::from_slice(body).context("decode trigger create request")?;
            require_trigger_path(&request.expected_config_path, config_path, "config")?;
            require_trigger_path(&request.expected_state_path, state_path, "state")?;
            if MctRuntimeStateStore::open(state_path)?
                .current_call_trigger_authority(&request.scope.trigger_authority_id)?
                .is_some()
            {
                bail!("trigger authority id already exists");
            }
            build_trigger_authority(
                config_path,
                state_path,
                uid,
                request.scope,
                1,
                1,
                CallTriggerAuthorityState::Active,
                true,
            )?
        }
        "/triggers/revise" => {
            let request: TriggerReviseRequest =
                serde_json::from_slice(body).context("decode trigger revise request")?;
            require_trigger_path(&request.expected_config_path, config_path, "config")?;
            require_trigger_path(&request.expected_state_path, state_path, "state")?;
            let current = MctRuntimeStateStore::open(state_path)?
                .current_call_trigger_authority(&request.scope.trigger_authority_id)?
                .context("trigger authority does not exist")?;
            if current.authority_state != CallTriggerAuthorityState::Active
                || current.record_revision != request.expected_revision
            {
                bail!("trigger authority revision is stale or not active");
            }
            build_trigger_authority(
                config_path,
                state_path,
                uid,
                request.scope,
                current.record_revision + 1,
                current.policy_revision + 1,
                CallTriggerAuthorityState::Active,
                true,
            )?
        }
        "/triggers/revoke" => {
            let request: TriggerRevokeRequest =
                serde_json::from_slice(body).context("decode trigger revoke request")?;
            require_trigger_path(&request.expected_config_path, config_path, "config")?;
            require_trigger_path(&request.expected_state_path, state_path, "state")?;
            let current = MctRuntimeStateStore::open(state_path)?
                .current_call_trigger_authority(&request.trigger_authority_id)?
                .context("trigger authority does not exist")?;
            if current.authority_state != CallTriggerAuthorityState::Active
                || current.record_revision != request.expected_revision
            {
                bail!("trigger authority revision is stale or not active");
            }
            let scope = TriggerAuthorityScopeRequest {
                trigger_authority_id: current.trigger_authority_id,
                target: current.target,
                payload_constraint: current.payload_constraint,
                trigger_source: current.trigger_source,
                missed_fire_policy: current.missed_fire_policy,
                overlap_policy: current.overlap_policy,
                starts_at: current.starts_at,
                expires_at: current.expires_at,
            };
            build_trigger_authority(
                config_path,
                state_path,
                uid,
                scope,
                current.record_revision + 1,
                current.policy_revision + 1,
                CallTriggerAuthorityState::Revoked,
                false,
            )?
        }
        _ => bail!("unknown trigger authority mutation route"),
    };
    Ok(PreparedTriggerMutation {
        state_path: state_path.to_path_buf(),
        observation: trigger_authority_observation(&authority, uid),
        authority,
    })
}

impl PreparedTriggerMutation {
    fn apply(&self) -> Result<CallTriggerAuthority> {
        MctRuntimeStateStore::open(&self.state_path)?
            .insert_call_trigger_authority(&self.authority)?;
        Ok(self.authority.clone())
    }
}

pub(super) async fn execute_resident_trigger_mutation(
    config_path: &Path,
    state_path: &Path,
    ledger: &ResidentLedgerWriter,
    peer: Option<MctUdsPeerCredentials>,
    path: &str,
    body: &[u8],
) -> MctControlPlaneResponse {
    let Some(peer) = peer else {
        return trigger_response(
            401,
            serde_json::json!({"error": "trigger authority requires authenticated owner"}),
        );
    };
    let prepared = match prepare_trigger_mutation(config_path, state_path, peer.uid, path, body) {
        Ok(prepared) => prepared,
        Err(_) => {
            return trigger_response(
                400,
                serde_json::json!({"error": "trigger authority mutation rejected"}),
            );
        }
    };
    if ledger
        .append(vec![prepared.observation.clone()])
        .await
        .is_err()
    {
        return trigger_response(
            500,
            serde_json::json!({"error": "trigger authority decision was not durable"}),
        );
    }
    match prepared.apply() {
        Ok(authority) => trigger_response(200, authority),
        Err(_) => {
            let _ = ledger
                .append(vec![trigger_projection_failure_observation(
                    &prepared.authority,
                )])
                .await;
            trigger_response(
                500,
                serde_json::json!({"error": "trigger authority projection failed"}),
            )
        }
    }
}

pub(super) fn execute_offline_trigger_mutation(
    config_path: &Path,
    state_path: &Path,
    ledger_path: &Path,
    uid: u32,
    path: &str,
    body: &[u8],
) -> Result<CallTriggerAuthority> {
    let mut ledger = JsonlObservationLedger::open(ledger_path, "ledger-local", "local-mct")
        .with_context(|| {
            format!(
                "acquire exclusive observation ledger writer lock at {}",
                ledger_path.display()
            )
        })?;
    let prepared = prepare_trigger_mutation(config_path, state_path, uid, path, body)?;
    ledger
        .append_batch_before_effect([prepared.observation.clone()], current_timestamp_string())?;
    match prepared.apply() {
        Ok(authority) => Ok(authority),
        Err(error) => {
            ledger.append_batch_before_effect(
                [trigger_projection_failure_observation(&prepared.authority)],
                current_timestamp_string(),
            )?;
            Err(error)
        }
    }
}

#[cfg(unix)]
fn operator_uid_from_config(config_path: &Path) -> Result<u32> {
    use std::os::unix::fs::MetadataExt as _;
    Ok(std::fs::metadata(config_path)
        .with_context(|| format!("read config owner {}", config_path.display()))?
        .uid())
}

fn ingest_trigger_payload(
    state_path: &Path,
    ledger_path: &Path,
    socket_path: &Path,
    payload_json: &str,
) -> Result<MctCallPayloadHandle> {
    serde_json::from_str::<serde_json::Value>(payload_json)
        .context("--payload-json must be valid JSON")?;
    let bytes = payload_json.as_bytes();
    let digest = blake3::hash(bytes).to_hex().to_string();
    let request = TriggerBlobIngestRequest {
        digest: digest.clone(),
        size_bytes: bytes.len() as u64,
        content_type: "application/json".into(),
        classification: "trigger-static".into(),
        bytes_base64: BASE64_STANDARD.encode(bytes),
    };
    if let Some(response) =
        try_resident_control_mutation(socket_path, "/blobs", &serde_json::to_vec(&request)?)?
    {
        let value: serde_json::Value = serde_json::from_slice(&response)?;
        return serde_json::from_value(value["payload"].clone())
            .context("decode resident trigger payload handle");
    }

    let store = local_blob_store_for_state_path(state_path);
    let mut ledger = JsonlObservationLedger::open(ledger_path, "ledger-local", "local-mct")?;
    let observation = MctObservation {
        observation_id: ObservationId::new(format!("obs:trigger-payload:{digest}"))?,
        observed_at: current_timestamp(),
        kind: ObservationKind::AdapterEffectStarted,
        source_plane: SourcePlane::Storage,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace:trigger-payload:{digest}"))?,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some("local-cas".into()),
        resource_id: Some(format!("blake3:{digest}")),
        policy_revision: None,
        grants_revision: None,
        outcome: ObservationOutcome::Allowed,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: "trigger payload ingest allowed".into(),
        detail_ref: Some(format!(
            "size_bytes:{};content_type:application/json",
            bytes.len()
        )),
    };
    ledger.append_batch_before_effect([observation], current_timestamp_string())?;
    let handle = store
        .ingest_reader(
            &digest,
            bytes.len() as u64,
            "application/json",
            std::io::Cursor::new(bytes),
        )
        .map_err(anyhow::Error::from)?;
    ledger.append_batch_before_effect(
        [MctObservation {
            observation_id: ObservationId::new(format!("obs:trigger-payload-complete:{digest}"))?,
            observed_at: current_timestamp(),
            kind: ObservationKind::StorageAppendSucceeded,
            source_plane: SourcePlane::Storage,
            trace: ObservationTraceRef {
                trace_id: TraceId::new(format!("trace:trigger-payload:{digest}"))?,
                span_id: None,
                parent_span_id: None,
                external_trace_id: None,
            },
            call_id: None,
            decision_id: None,
            subject_id: Some("local-cas".into()),
            resource_id: Some(format!("blake3:{digest}")),
            policy_revision: None,
            grants_revision: None,
            outcome: ObservationOutcome::Completed,
            visibility: ObservationVisibility::InternalOnly,
            safe_message: "trigger payload ingest completed".into(),
            detail_ref: Some(format!("size_bytes:{}", bytes.len())),
        }],
        current_timestamp_string(),
    )?;
    Ok(handle)
}

pub(super) fn take_required(args: &mut Vec<String>, flag: &str) -> Result<String> {
    take_option(args, flag).with_context(|| format!("missing required {flag}"))
}

fn parse_trigger_policy_args(args: &mut Vec<String>) -> Result<(MissedFirePolicy, OverlapPolicy)> {
    let missed = take_option(args, "--missed-fire-policy").unwrap_or_else(|| "skip".into());
    let overlap = take_option(args, "--overlap-policy").unwrap_or_else(|| "refuse".into());
    let missed_fire_policy = match missed.as_str() {
        "skip" => MissedFirePolicy::Skip,
        "coalesce-one" | "coalesce_one" => MissedFirePolicy::CoalesceOne,
        "fire-late-bounded" | "fire_late_bounded" => MissedFirePolicy::FireLateBounded,
        other => bail!("unknown missed-fire policy '{other}'"),
    };
    let overlap_policy = match overlap.as_str() {
        "refuse" => OverlapPolicy::Refuse,
        "coalesce-one" | "coalesce_one" => OverlapPolicy::CoalesceOne,
        "queue-bounded" | "queue_bounded" => OverlapPolicy::QueueBounded,
        other => bail!("unknown overlap policy '{other}'"),
    };
    Ok((missed_fire_policy, overlap_policy))
}

fn execute_cli_trigger_mutation(
    config_path: &Path,
    state_path: &Path,
    ledger_path: &Path,
    socket_path: &Path,
    path: &str,
    request: &impl serde::Serialize,
) -> Result<CallTriggerAuthority> {
    let body = serde_json::to_vec(request)?;
    if let Some(response) = try_resident_control_mutation(socket_path, path, &body)? {
        return serde_json::from_slice(&response).context("decode trigger mutation response");
    }
    #[cfg(unix)]
    {
        execute_offline_trigger_mutation(
            config_path,
            state_path,
            ledger_path,
            operator_uid_from_config(config_path)?,
            path,
            &body,
        )
    }
    #[cfg(not(unix))]
    {
        let _ = (config_path, state_path, ledger_path, path, body);
        bail!("offline trigger authority mutation requires Unix owner identity")
    }
}

pub(super) fn run_triggers(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected triggers subcommand: create | revise | revoke | show | list");
    }
    let command = args.remove(0);
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let socket_path = take_option(&mut args, "--uds")
        .map(PathBuf::from)
        .unwrap_or_else(default_control_uds_path);
    let as_json = take_flag(&mut args, "--json");

    match command.as_str() {
        "show" => {
            let trigger_id = args.first().context("expected trigger id")?;
            if args.len() != 1 {
                bail!("unexpected triggers show arguments");
            }
            let authority = MctRuntimeStateStore::open(&state_path)?
                .current_call_trigger_authority(&CallTriggerAuthorityId::new(trigger_id)?)?
                .context("trigger authority not found")?;
            print_trigger_authority(&authority, as_json)?;
        }
        "list" => {
            if !args.is_empty() {
                bail!("unexpected triggers list arguments");
            }
            let authorities =
                MctRuntimeStateStore::open(&state_path)?.call_trigger_authorities()?;
            if as_json {
                println!("{}", serde_json::to_string_pretty(&authorities)?);
            } else {
                for authority in authorities {
                    println!(
                        "trigger={} revision={} state={:?} target={}",
                        authority.trigger_authority_id,
                        authority.record_revision,
                        authority.authority_state,
                        mct_daemon::operation_id_from_target(&authority.target)
                    );
                }
            }
        }
        "revoke" => {
            if args.is_empty() {
                bail!("expected trigger id");
            }
            let trigger_authority_id = CallTriggerAuthorityId::new(args.remove(0))?;
            let expected_revision = take_required(&mut args, "--expected-revision")?
                .parse::<u64>()
                .context("parse --expected-revision")?;
            if !args.is_empty() {
                bail!("unexpected triggers revoke arguments: {}", args.join(" "));
            }
            let authority = execute_cli_trigger_mutation(
                &config_path,
                &state_path,
                &ledger_path,
                &socket_path,
                "/triggers/revoke",
                &TriggerRevokeRequest {
                    expected_config_path: config_path.clone(),
                    expected_state_path: state_path.clone(),
                    trigger_authority_id,
                    expected_revision,
                },
            )?;
            print_trigger_authority(&authority, as_json)?;
        }
        "create" | "revise" => {
            if args.is_empty() {
                bail!("expected trigger id");
            }
            let trigger_authority_id = CallTriggerAuthorityId::new(args.remove(0))?;
            let target =
                operation_target_from_wit_operation_id(&take_required(&mut args, "--target")?)?;
            let payload_json = take_required(&mut args, "--payload-json")?;
            let anchor_at = Timestamp::new(take_required(&mut args, "--anchor-at")?)?;
            let interval_ms = take_required(&mut args, "--interval-ms")?
                .parse::<u64>()
                .context("parse --interval-ms")?;
            let starts_at = Timestamp::new(take_required(&mut args, "--starts-at")?)?;
            let expires_at = Timestamp::new(take_required(&mut args, "--expires-at")?)?;
            let expected_revision = if command == "revise" {
                Some(
                    take_required(&mut args, "--expected-revision")?
                        .parse::<u64>()
                        .context("parse --expected-revision")?,
                )
            } else {
                None
            };
            let (missed_fire_policy, overlap_policy) = parse_trigger_policy_args(&mut args)?;
            if !args.is_empty() {
                bail!(
                    "unexpected triggers {command} arguments: {}",
                    args.join(" ")
                );
            }
            let payload_constraint =
                ingest_trigger_payload(&state_path, &ledger_path, &socket_path, &payload_json)?;
            let scope = TriggerAuthorityScopeRequest {
                trigger_authority_id,
                target,
                payload_constraint,
                trigger_source: CallTriggerSource::Temporal {
                    anchor_at,
                    interval_ms,
                },
                missed_fire_policy,
                overlap_policy,
                starts_at,
                expires_at,
            };
            let authority = if let Some(expected_revision) = expected_revision {
                execute_cli_trigger_mutation(
                    &config_path,
                    &state_path,
                    &ledger_path,
                    &socket_path,
                    "/triggers/revise",
                    &TriggerReviseRequest {
                        expected_config_path: config_path.clone(),
                        expected_state_path: state_path.clone(),
                        expected_revision,
                        scope,
                    },
                )?
            } else {
                execute_cli_trigger_mutation(
                    &config_path,
                    &state_path,
                    &ledger_path,
                    &socket_path,
                    "/triggers/create",
                    &TriggerCreateRequest {
                        expected_config_path: config_path.clone(),
                        expected_state_path: state_path.clone(),
                        scope,
                    },
                )?
            };
            print_trigger_authority(&authority, as_json)?;
        }
        other => bail!("unknown triggers subcommand '{other}'"),
    }
    Ok(())
}

fn print_trigger_authority(authority: &CallTriggerAuthority, as_json: bool) -> Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(authority)?);
    } else {
        println!(
            "trigger={} revision={} state={:?} target={} missed={:?} overlap={:?}",
            authority.trigger_authority_id,
            authority.record_revision,
            authority.authority_state,
            mct_daemon::operation_id_from_target(&authority.target),
            authority.missed_fire_policy,
            authority.overlap_policy
        );
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn authority_for_scheduler_test() -> CallTriggerAuthority {
    CallTriggerAuthority {
        trigger_authority_id: CallTriggerAuthorityId::new("trigger-scheduler").unwrap(),
        mother_node_id: MctNodeId::new("local-mct").unwrap(),
        vision_id: VisionId::new("vision-local").unwrap(),
        canonical_caller: CallerIdentity {
            node_id: MctNodeId::new("local-mct").unwrap(),
            user_id: Some(UserId::new("uid:501").unwrap()),
            vision_id: VisionId::new("vision-local").unwrap(),
            project_id: None,
        },
        target: OperationTarget::new("patina:watch", "control@0.1.0", "scan-now").unwrap(),
        payload_constraint: MctCallPayloadHandle::Empty,
        trigger_source: CallTriggerSource::Temporal {
            anchor_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
            interval_ms: 1_000,
        },
        trigger_source_ref: "blake3:source".into(),
        missed_fire_policy: MissedFirePolicy::Skip,
        overlap_policy: OverlapPolicy::Refuse,
        issuer_principal_ref: "os-uid:501".into(),
        record_revision: 1,
        policy_revision: 1,
        starts_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
        expires_at: Timestamp::new("2026-07-21T13:00:00Z").unwrap(),
        authority_state: CallTriggerAuthorityState::Active,
        authority_observation_id: ObservationId::new("obs-trigger-scheduler").unwrap(),
        canonical_record_digest: String::new(),
    }
    .seal()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured_paths(dir: &tempfile::TempDir) -> (PathBuf, PathBuf, PathBuf) {
        let config_path = dir.path().join("config.json");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        MctDaemonConfigStore::new(&config_path)
            .ensure_local_identity(
                MctOperatorNodeScope::default(),
                dir.path().join("identity.key"),
            )
            .unwrap();
        (config_path, state_path, ledger_path)
    }

    fn scope(state_path: &Path, id: &str) -> TriggerAuthorityScopeRequest {
        let bytes = b"[]";
        let digest = blake3::hash(bytes).to_hex().to_string();
        let payload_constraint = local_blob_store_for_state_path(state_path)
            .ingest_reader(
                &digest,
                bytes.len() as u64,
                "application/json",
                std::io::Cursor::new(bytes),
            )
            .unwrap();
        TriggerAuthorityScopeRequest {
            trigger_authority_id: CallTriggerAuthorityId::new(id).unwrap(),
            target: OperationTarget::new("patina:watch", "control@0.1.0", "scan-now").unwrap(),
            payload_constraint,
            trigger_source: CallTriggerSource::Temporal {
                anchor_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
                interval_ms: 1_000,
            },
            missed_fire_policy: MissedFirePolicy::Skip,
            overlap_policy: OverlapPolicy::Refuse,
            starts_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
            expires_at: Timestamp::new("2026-07-21T13:00:00Z").unwrap(),
        }
    }

    #[test]
    fn trigger_authority_is_scoped_observed_revisioned_and_revocable() {
        let dir = tempfile::tempdir().unwrap();
        let (config_path, state_path, ledger_path) = configured_paths(&dir);
        let create = TriggerCreateRequest {
            expected_config_path: config_path.clone(),
            expected_state_path: state_path.clone(),
            scope: scope(&state_path, "trigger-a"),
        };
        let created = execute_offline_trigger_mutation(
            &config_path,
            &state_path,
            &ledger_path,
            501,
            "/triggers/create",
            &serde_json::to_vec(&create).unwrap(),
        )
        .unwrap();
        assert_eq!(created.record_revision, 1);
        assert_eq!(created.missed_fire_policy, MissedFirePolicy::Skip);
        assert_eq!(created.overlap_policy, OverlapPolicy::Refuse);

        let stale = TriggerReviseRequest {
            expected_config_path: config_path.clone(),
            expected_state_path: state_path.clone(),
            expected_revision: 0,
            scope: scope(&state_path, "trigger-a"),
        };
        assert!(
            execute_offline_trigger_mutation(
                &config_path,
                &state_path,
                &ledger_path,
                501,
                "/triggers/revise",
                &serde_json::to_vec(&stale).unwrap(),
            )
            .is_err()
        );

        let payload_path = match &created.payload_constraint {
            MctCallPayloadHandle::ContentAddressedBlob { digest, .. } => {
                local_blob_store_for_state_path(&state_path)
                    .visible_path(digest)
                    .unwrap()
            }
            _ => panic!("test trigger payload must be content addressed"),
        };
        std::fs::remove_file(payload_path).unwrap();

        let revoke = TriggerRevokeRequest {
            expected_config_path: config_path.clone(),
            expected_state_path: state_path.clone(),
            trigger_authority_id: CallTriggerAuthorityId::new("trigger-a").unwrap(),
            expected_revision: 1,
        };
        let revoked = execute_offline_trigger_mutation(
            &config_path,
            &state_path,
            &ledger_path,
            501,
            "/triggers/revoke",
            &serde_json::to_vec(&revoke).unwrap(),
        )
        .unwrap();
        assert_eq!(revoked.record_revision, 2);
        assert_eq!(revoked.authority_state, CallTriggerAuthorityState::Revoked);
        let ledger_text = std::fs::read_to_string(ledger_path).unwrap();
        assert!(ledger_text.contains("call-trigger-authority-v1"));
        assert!(ledger_text.contains("trigger authority created"));
        assert!(ledger_text.contains("trigger authority revoked"));
        assert!(!ledger_text.contains("[]"));
    }

    #[test]
    fn trigger_management_rejects_event_and_authority_expansion() {
        let dir = tempfile::tempdir().unwrap();
        let (config_path, state_path, _ledger_path) = configured_paths(&dir);
        let mut event_scope = scope(&state_path, "trigger-event");
        event_scope.trigger_source = CallTriggerSource::Event {
            event_source_ref: "event-source-a".into(),
        };
        let event = TriggerCreateRequest {
            expected_config_path: config_path.clone(),
            expected_state_path: state_path.clone(),
            scope: event_scope,
        };
        let error = prepare_trigger_mutation(
            &config_path,
            &state_path,
            501,
            "/triggers/create",
            &serde_json::to_vec(&event).unwrap(),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("MotherEventSourceAdapterRuntime")
        );

        let mut registry_scope = scope(&state_path, "trigger-registry");
        registry_scope.target =
            OperationTarget::new("mct:registry", "control@0.1.0", "sync").unwrap();
        let registry = TriggerCreateRequest {
            expected_config_path: config_path.clone(),
            expected_state_path: state_path.clone(),
            scope: registry_scope,
        };
        assert!(
            prepare_trigger_mutation(
                &config_path,
                &state_path,
                501,
                "/triggers/create",
                &serde_json::to_vec(&registry).unwrap(),
            )
            .unwrap_err()
            .to_string()
            .contains("deferred")
        );

        let mut missing_payload = scope(&state_path, "trigger-missing-payload");
        missing_payload.payload_constraint =
            mct_daemon::content_addressed_blob_handle("0".repeat(64), "application/json", 2);
        let missing = TriggerCreateRequest {
            expected_config_path: config_path.clone(),
            expected_state_path: state_path.clone(),
            scope: missing_payload,
        };
        assert!(
            prepare_trigger_mutation(
                &config_path,
                &state_path,
                501,
                "/triggers/create",
                &serde_json::to_vec(&missing).unwrap(),
            )
            .is_err()
        );

        let mut unknown = serde_json::to_value(event).unwrap();
        unknown["caller"] = serde_json::Value::String("forged".into());
        assert!(
            serde_json::from_value::<TriggerCreateRequest>(unknown).is_err(),
            "caller claims and unknown authority fields must fail closed"
        );
    }

    #[tokio::test]
    async fn trigger_append_failure_suppresses_activation_revision_and_revocation() {
        let dir = tempfile::tempdir().unwrap();
        let (config_path, state_path, _ledger_path) = configured_paths(&dir);
        let request = TriggerCreateRequest {
            expected_config_path: config_path.clone(),
            expected_state_path: state_path.clone(),
            scope: scope(&state_path, "trigger-no-append"),
        };
        let response = execute_resident_trigger_mutation(
            &config_path,
            &state_path,
            &ResidentLedgerWriter::failed_for_test(),
            Some(MctUdsPeerCredentials {
                uid: 501,
                gid: 20,
                pid: Some(42),
            }),
            "/triggers/create",
            &serde_json::to_vec(&request).unwrap(),
        )
        .await;
        assert_eq!(response.status_code, 500);
        assert!(
            MctRuntimeStateStore::open(&state_path)
                .unwrap()
                .call_trigger_authorities()
                .unwrap()
                .is_empty()
        );
    }
}

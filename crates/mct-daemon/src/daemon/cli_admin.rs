use super::*;

pub(super) fn run_registry(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected registry subcommand: install | sync");
    }
    match args.remove(0).as_str() {
        "install" => run_registry_install(args),
        "sync" => run_registry_sync(args),
        other => bail!("unknown registry subcommand '{other}'"),
    }
}

fn execute_cli_registry_mutation<T: serde::de::DeserializeOwned>(
    children_dir: &Path,
    state_path: &Path,
    ledger_path: &Path,
    socket_path: &Path,
    path: &str,
    request: &impl serde::Serialize,
) -> Result<T> {
    let body = serde_json::to_vec(request).context("encode registry mutation request")?;
    let value = if let Some(response) = try_resident_control_mutation(socket_path, path, &body)? {
        serde_json::from_slice(&response).context("decode resident registry mutation result")?
    } else {
        execute_offline_registry_mutation(children_dir, state_path, ledger_path, path, &body)
            .with_context(|| {
                format!(
                    "resident UDS {} unavailable and offline registry mutation failed",
                    socket_path.display()
                )
            })?
    };
    serde_json::from_value(value).context("decode registry mutation result")
}

pub(super) fn run_registry_install(mut args: Vec<String>) -> Result<()> {
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let socket_path = take_option(&mut args, "--uds")
        .map(PathBuf::from)
        .unwrap_or_else(default_control_uds_path);
    let replace = take_flag(&mut args, "--replace");
    let as_json = take_flag(&mut args, "--json");
    if args.len() != 1 {
        bail!(
            "expected: mct-daemon registry install <verified-package-dir> [--children-dir path] [--state path] [--ledger path] [--uds socket-path] [--replace] [--json]"
        );
    }
    let report: MctChildPackageInstallReport = execute_cli_registry_mutation(
        &children_dir,
        &state_path,
        &ledger_path,
        &socket_path,
        "/registry/install",
        &RegistryInstallRequest {
            expected_children_dir: children_dir.clone(),
            source_dir: PathBuf::from(&args[0]),
            replace,
        },
    )?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "installed child={} version={} artifact={} path={} replaced={}",
            report.child_name,
            report.artifact_version,
            report.artifact_id,
            report.installed_dir.display(),
            report.replaced_existing
        );
    }
    Ok(())
}

pub(super) fn run_registry_sync(mut args: Vec<String>) -> Result<()> {
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let socket_path = take_option(&mut args, "--uds")
        .map(PathBuf::from)
        .unwrap_or_else(default_control_uds_path);
    let strict = take_flag(&mut args, "--strict-integrity");
    let as_json = take_flag(&mut args, "--json");
    if args.is_empty() {
        bail!("expected registry source id");
    }
    let source_id = args.remove(0);
    let children_dir = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let report: MctRegistrySyncReport = execute_cli_registry_mutation(
        &children_dir,
        &state_path,
        &ledger_path,
        &socket_path,
        "/registry/sync",
        &RegistrySyncRequest {
            expected_children_dir: children_dir.clone(),
            expected_state_path: state_path.clone(),
            source_id,
            strict_integrity: strict,
        },
    )?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "registry source={} path={} loaded={} failed={}",
            report.source_id,
            report.source_path.display(),
            report.loaded,
            report.failed
        );
    }
    Ok(())
}

pub(super) fn run_federation(mut args: Vec<String>) -> Result<()> {
    if args.first().map(String::as_str) != Some("view") {
        bail!(
            "expected: mct-daemon federation view [--config path] [--state path] [--children-dir path] [--json]"
        );
    }
    args.remove(0);
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let as_json = take_flag(&mut args, "--json");
    let config = MctDaemonConfigStore::new(&config_path).load()?;
    let summary = MctRuntimeStateStore::open(&state_path)?.summary()?;
    let load_report = load_children_from_dir(MctChildLoadOptions::new(children_dir));
    let view = build_federation_capability_view_with_children(
        &config,
        &summary,
        MctNodeId::new("local-mct").expect("string ID literal/generated value must be non-empty"),
        VisionId::new("vision-local").expect("string ID literal/generated value must be non-empty"),
        load_report.children.iter(),
    );
    if as_json {
        println!("{}", serde_json::to_string_pretty(&view)?);
    } else {
        println!(
            "federation node={} vision={} approved={} ready={} callable_surfaces={} peers={}",
            view.node_id,
            view.vision_id,
            view.approved_children,
            view.ready_instances,
            view.callable_surfaces.len(),
            view.peers.len()
        );
    }
    Ok(())
}

pub(super) fn run_metrics(mut args: Vec<String>) -> Result<()> {
    if args.first().map(String::as_str) != Some("snapshot") {
        bail!("expected: mct-daemon metrics snapshot [--state path] [--json]");
    }
    args.remove(0);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let as_json = take_flag(&mut args, "--json");
    let snapshot = build_metrics_snapshot(&MctRuntimeStateStore::open(&state_path)?)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
    } else {
        println!(
            "metrics runs={}/{} metric_points={}",
            snapshot.run_success_numerator,
            snapshot.run_success_denominator,
            snapshot.recent_points.len()
        );
    }
    Ok(())
}

pub(super) fn run_pando(mut args: Vec<String>) -> Result<()> {
    if args.first().map(String::as_str) != Some("record") {
        bail!(
            "expected: mct-daemon pando record <composition-id> [step-id,call-id,runtime,child,decision ...] [--state path] [--json]"
        );
    }
    args.remove(0);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let as_json = take_flag(&mut args, "--json");
    if args.is_empty() {
        bail!("expected composition id");
    }
    let composition_id = args.remove(0);
    let steps = args
        .iter()
        .map(|raw| parse_composition_step(raw))
        .collect::<Result<Vec<_>>>()?;
    let state = MctRuntimeStateStore::open(&state_path)?;
    let record = record_composition_plan(
        &state,
        MctCompositionPlan {
            composition_id,
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            steps,
        },
    )?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&record)?);
    } else {
        println!(
            "pando composition={} state={}",
            record.composition_id, record.state
        );
    }
    Ok(())
}

pub(super) fn parse_composition_step(raw: &str) -> Result<MctCompositionStep> {
    let parts = raw.split(',').collect::<Vec<_>>();
    if parts.len() < 3 {
        bail!("composition step must be step-id,call-id,runtime[,child[,decision]]");
    }
    Ok(MctCompositionStep {
        step_id: parts[0].into(),
        call_id: CallId::new(parts[1])
            .expect("string ID literal/generated value must be non-empty"),
        runtime_kind: parse_runtime_kind(parts[2])?,
        child_name: parts
            .get(3)
            .filter(|value| !value.is_empty())
            .map(|value| (*value).to_owned()),
        authority_decision_id: parts.get(4).filter(|value| !value.is_empty()).map(|value| {
            DecisionId::new(*value).expect("string ID literal/generated value must be non-empty")
        }),
    })
}

pub(super) fn parse_runtime_kind(value: &str) -> Result<RuntimeKind> {
    match value {
        "process" => Ok(RuntimeKind::Process),
        "jvm_child" | "jvm" => Ok(RuntimeKind::JvmChild),
        "wasm_component" | "wasm" => Ok(RuntimeKind::WasmComponent),
        "remote_peer" | "remote" => Ok(RuntimeKind::RemotePeer),
        "internal" => Ok(RuntimeKind::Internal),
        other => bail!("unknown runtime kind '{other}'"),
    }
}

pub(super) fn run_toys(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected toys subcommand: authorize-slate | authorize-secret");
    }
    match args.remove(0).as_str() {
        "authorize-slate" => run_toys_authorize_slate(args),
        "authorize-secret" => run_toys_authorize_secret(args),
        other => bail!("unknown toys subcommand '{other}'"),
    }
}

pub(super) fn run_toys_authorize_slate(mut args: Vec<String>) -> Result<()> {
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let as_json = take_flag(&mut args, "--json");
    if args.len() < 2 {
        bail!(
            "expected: mct-daemon toys authorize-slate <child-name> <project-root> [--children-dir path] [--config path] [--state path] [--json]"
        );
    }
    let child_name = args.remove(0);
    let project_root = canonical_dir(PathBuf::from(args.remove(0)), "project root")?;
    let child = load_named_child(&children_dir, &child_name)?;
    let config = MctDaemonConfigStore::new(&config_path).load()?;
    let approval = config
        .child_approvals
        .get(&child_name)
        .ok_or_else(|| anyhow::anyhow!("child '{child_name}' is not approved in config"))?;
    if approval.approval_state != ChildApprovalState::Approved {
        bail!("child '{child_name}' approval is not active");
    }
    let assignment = config
        .child_assignments
        .get(&child_name)
        .ok_or_else(|| anyhow::anyhow!("child '{child_name}' is not assigned in config"))?;
    if assignment.assignment_state != ChildAssignmentState::Active {
        bail!("child '{child_name}' assignment is not active");
    }
    if approval.artifact_id.as_str() != child.artifact_id
        || assignment.artifact_id.as_str() != child.artifact_id
    {
        bail!("child '{child_name}' config artifact does not match loaded child package");
    }

    let state = MctRuntimeStateStore::open(&state_path)?;
    let contracts = slate_toy_contracts();
    for contract in &contracts {
        state.upsert_toy_contract(contract)?;
    }
    let grants = slate_toy_grants_for_child(&child, &project_root);
    for grant in &grants {
        state.upsert_toy_grant_snapshot(grant)?;
    }

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "state": state_path,
                "child": child_name,
                "project_root": project_root,
                "contracts": contracts,
                "grants": grants,
            }))?
        );
    } else {
        println!(
            "authorized slate toys child={} project_root={} state={} contracts={} grants={}",
            child_name,
            project_root.display(),
            state_path.display(),
            contracts.len(),
            grants.len()
        );
    }
    Ok(())
}

pub(super) fn run_toys_authorize_secret(mut args: Vec<String>) -> Result<()> {
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let as_json = take_flag(&mut args, "--json");
    if args.len() < 2 {
        bail!(
            "expected: mct-daemon toys authorize-secret <child-name> <secret-name> [--children-dir path] [--config path] [--state path] [--json]"
        );
    }
    let child_name = args.remove(0);
    let secret_name = args.remove(0);
    if secret_name.trim().is_empty() {
        bail!("secret name must not be empty");
    }
    let child = load_named_child(&children_dir, &child_name)?;
    let config = MctDaemonConfigStore::new(&config_path).load()?;
    let approval = config
        .child_approvals
        .get(&child_name)
        .ok_or_else(|| anyhow::anyhow!("child '{child_name}' is not approved in config"))?;
    if approval.approval_state != ChildApprovalState::Approved {
        bail!("child '{child_name}' approval is not active");
    }
    let assignment = config
        .child_assignments
        .get(&child_name)
        .ok_or_else(|| anyhow::anyhow!("child '{child_name}' is not assigned in config"))?;
    if assignment.assignment_state != ChildAssignmentState::Active {
        bail!("child '{child_name}' assignment is not active");
    }
    if approval.artifact_id.as_str() != child.artifact_id
        || assignment.artifact_id.as_str() != child.artifact_id
    {
        bail!("child '{child_name}' config artifact does not match loaded child package");
    }

    let state = MctRuntimeStateStore::open(&state_path)?;
    let contract = mct_secrets_toy_contract();
    state.upsert_toy_contract(&contract)?;
    let grant = secret_toy_grant_for_child(&child, &secret_name);
    state.upsert_toy_grant_snapshot(&grant)?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "state": state_path,
                "child": child_name,
                "secret_name": secret_name,
                "contract": contract,
                "grant": grant,
            }))?
        );
    } else {
        println!(
            "authorized secret toy child={} secret={} state={}",
            child_name,
            secret_name,
            state_path.display()
        );
    }
    Ok(())
}

pub(super) fn secret_toy_grant_for_child(
    child: &mct_daemon::MctLoadedChild,
    secret_name: &str,
) -> ToyGrant {
    ToyGrant {
        grant_id: ToyGrantId::new(format!("grant:secret:{secret_name}:{}", child.name))
            .expect("string ID literal/generated value must be non-empty"),
        toy_id: ToyId::new(MCT_SECRETS_TOY_ID)
            .expect("string ID literal/generated value must be non-empty"),
        subject: ToyGrantSubject {
            child_name: child.name.clone(),
            artifact_id: child.artifact_id.clone(),
            artifact_version: child.version.clone(),
            assignment_id: Some(
                ChildAssignmentId::new(format!("assignment:{}", child.name))
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            caller_node_id: Some(
                MctNodeId::new("local-mct")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
        },
        scope: ToyGrantScope {
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            node_id: Some(
                MctNodeId::new("local-mct")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            project_id: None,
            data_classification: None,
            resource_id: Some(secret_name.to_owned()),
            allowed_actions: vec!["get".into()],
        },
        constraints: ToyGrantConstraints {
            starts_at: None,
            expires_at: None,
            max_uses: None,
            max_duration_ms: None,
            locality_required: true,
        },
        grant_state: ToyGrantState::Active,
        issuer_id: "local-mct".into(),
        policy_revision: 1,
        grants_revision: 1,
        authority_observation_id: ObservationId::new(format!(
            "obs:toy-grant:secret:{secret_name}:{}",
            child.name
        ))
        .expect("string ID literal/generated value must be non-empty"),
    }
}

pub(super) fn slate_toy_contracts() -> Vec<CanonicalToyContract> {
    vec![
        slate_toy_contract(
            slate_logging_toy_id(),
            ToyContractIdentity {
                namespace: "wasi".into(),
                interface_name: "logging/logging".into(),
                version: "0.1.0".into(),
                function_name: Some("log".into()),
                resource_name: None,
            },
        ),
        slate_toy_contract(
            slate_measure_toy_id(),
            ToyContractIdentity {
                namespace: "patina".into(),
                interface_name: "measure/measure".into(),
                version: "0.1.0".into(),
                function_name: None,
                resource_name: None,
            },
        ),
        slate_toy_contract(
            slate_git_toy_id(),
            ToyContractIdentity {
                namespace: "patina".into(),
                interface_name: "git/git".into(),
                version: "0.1.0".into(),
                function_name: None,
                resource_name: None,
            },
        ),
        slate_toy_contract(
            slate_filesystem_toy_id(),
            ToyContractIdentity {
                namespace: "wasi".into(),
                interface_name: "filesystem/preopens".into(),
                version: "0.2.3".into(),
                function_name: Some("preopen-project-root".into()),
                resource_name: None,
            },
        ),
    ]
}

pub(super) fn slate_toy_contract(
    toy_id: ToyId,
    contract: ToyContractIdentity,
) -> CanonicalToyContract {
    CanonicalToyContract {
        admitted_by_observation_id: ObservationId::new(format!("obs:toy-catalog:{toy_id}"))
            .expect("string ID literal/generated value must be non-empty"),
        toy_id,
        contract,
        authority_bearing: true,
        catalog_revision: 1,
    }
}

pub(super) fn slate_toy_grants_for_child(
    child: &mct_daemon::MctLoadedChild,
    project_root: &Path,
) -> Vec<ToyGrant> {
    [
        (slate_logging_toy_id(), "invoke", "logging"),
        (slate_measure_toy_id(), "invoke", "measure"),
        (slate_git_toy_id(), "invoke", "git"),
        (
            slate_filesystem_toy_id(),
            "preopen-project-root",
            "filesystem",
        ),
    ]
    .into_iter()
    .map(|(toy_id, action, label)| ToyGrant {
        grant_id: ToyGrantId::new(format!("grant:slate:{label}:{}", child.name))
            .expect("string ID literal/generated value must be non-empty"),
        toy_id,
        subject: ToyGrantSubject {
            child_name: child.name.clone(),
            artifact_id: child.artifact_id.clone(),
            artifact_version: child.version.clone(),
            assignment_id: Some(
                ChildAssignmentId::new(format!("assignment:{}", child.name))
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            caller_node_id: Some(
                MctNodeId::new("local-mct")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
        },
        scope: ToyGrantScope {
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            node_id: Some(
                MctNodeId::new("local-mct")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            project_id: None,
            data_classification: Some("public".into()),
            resource_id: Some(project_root.display().to_string()),
            allowed_actions: vec![action.into()],
        },
        constraints: ToyGrantConstraints {
            starts_at: None,
            expires_at: None,
            max_uses: None,
            max_duration_ms: None,
            locality_required: true,
        },
        grant_state: ToyGrantState::Active,
        issuer_id: "local-operator".into(),
        policy_revision: 1,
        grants_revision: 1,
        authority_observation_id: ObservationId::new(format!(
            "obs:toy-grant:slate:{label}:{}",
            child.name
        ))
        .expect("string ID literal/generated value must be non-empty"),
    })
    .collect()
}

pub(super) fn run_state(mut args: Vec<String>) -> Result<()> {
    if args.first().map(String::as_str) != Some("summary") {
        bail!("expected: mct-daemon state summary [--state path] [--json]");
    }
    args.remove(0);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let as_json = take_flag(&mut args, "--json");
    let state = MctRuntimeStateStore::open(&state_path)?;
    let summary = state.summary()?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "state={} schema={} artifacts={} approved={} assignments={} ready={} peers={} runs={} completed={} failed={} metrics={}",
            state_path.display(),
            summary.schema_version,
            summary.artifacts,
            summary.approved_children,
            summary.active_assignments,
            summary.ready_instances,
            summary.peers,
            summary.runs,
            summary.completed_runs,
            summary.failed_runs,
            summary.metric_points
        );
    }
    Ok(())
}

pub(super) fn run_runs(mut args: Vec<String>) -> Result<()> {
    if args.first().map(String::as_str) != Some("list") {
        bail!("expected: mct-daemon runs list [--state path] [--json] [--limit n]");
    }
    args.remove(0);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let limit = take_option(&mut args, "--limit")
        .map(|value| value.parse::<u32>())
        .transpose()
        .context("parse --limit")?
        .unwrap_or(20);
    let as_json = take_flag(&mut args, "--json");
    let state = MctRuntimeStateStore::open(&state_path)?;
    let runs = state.list_runs(limit)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&runs)?);
    } else {
        println!("state={} runs={}", state_path.display(), runs.len());
        for run in runs {
            println!(
                "run id={} call={} state={:?} runtime={:?} child={} started={} completed={}",
                run.run_id,
                run.call_id,
                run.state,
                run.runtime_kind,
                run.child_name.unwrap_or_else(|| "-".into()),
                run.started_at,
                run.completed_at.unwrap_or_else(|| "-".into())
            );
        }
    }
    Ok(())
}

pub(super) fn default_control_uds_path() -> PathBuf {
    PathBuf::from(".mct").join("control.sock")
}

#[cfg(unix)]
pub(super) fn try_resident_control_mutation(
    socket_path: &Path,
    path: &str,
    body: &[u8],
) -> Result<Option<Vec<u8>>> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = match UnixStream::connect(socket_path) {
        Ok(stream) => stream,
        Err(_) => return Ok(None),
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .context("set resident UDS read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .context("set resident UDS write timeout")?;
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: local\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .context("write resident control mutation headers")?;
    stream
        .write_all(body)
        .context("write resident control mutation body")?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .context("finish resident control mutation request")?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .context("read resident control mutation response")?;
    let response = std::str::from_utf8(&response).context("decode resident control response")?;
    let (headers, response_body) = response
        .split_once("\r\n\r\n")
        .context("resident control response missing header terminator")?;
    let status = headers
        .split_whitespace()
        .nth(1)
        .context("resident control response missing status")?
        .parse::<u16>()
        .context("parse resident control response status")?;
    if !(200..300).contains(&status) {
        let safe_error = serde_json::from_str::<serde_json::Value>(response_body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "resident mutation rejected".into());
        bail!("resident control mutation failed with HTTP status {status}: {safe_error}");
    }
    Ok(Some(response_body.as_bytes().to_vec()))
}

#[cfg(not(unix))]
pub(super) fn try_resident_control_mutation(
    _socket_path: &Path,
    _path: &str,
    _body: &[u8],
) -> Result<Option<Vec<u8>>> {
    Ok(None)
}

fn execute_cli_peer_mutation(
    config_path: &Path,
    ledger_path: &Path,
    socket_path: &Path,
    path: &str,
    request: &impl serde::Serialize,
) -> Result<PeerMutationSuccess> {
    let body = serde_json::to_vec(request).context("encode peer mutation request")?;
    if let Some(response) = try_resident_control_mutation(socket_path, path, &body)? {
        return serde_json::from_slice(&response).context("decode resident peer mutation result");
    }
    execute_offline_peer_mutation(config_path, ledger_path, path, &body).with_context(|| {
        format!(
            "resident UDS {} unavailable and offline peer mutation failed",
            socket_path.display()
        )
    })
}

pub(super) fn run_peers(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected peers subcommand: add | list | set-outbound-proof | revoke | remove");
    }
    match args.remove(0).as_str() {
        "add" => run_peers_add(args),
        "list" => run_peers_list(args),
        "set-outbound-proof" => run_peers_set_outbound_proof(args),
        "revoke" => run_peers_revoke(args),
        "remove" => run_peers_remove(args),
        other => bail!("unknown peers subcommand '{other}'"),
    }
}

pub(super) fn run_peers_add(mut args: Vec<String>) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let socket_path = take_option(&mut args, "--uds")
        .map(PathBuf::from)
        .unwrap_or_else(default_control_uds_path);
    let binding_signature_ref = take_option(&mut args, "--signature-ref");
    if args.len() < 4 {
        bail!(
            "expected: mct-daemon peers add <peer-node-id> <binding-id> <endpoint-id> <vision-id> [ticket-file] [--signature-ref proof] [--config path] [--ledger path] [--uds socket-path]"
        );
    }
    let peer_node_id = MctNodeId::new(args.remove(0))
        .expect("string ID literal/generated value must be non-empty");
    let binding_id = PeerBindingId::new(args.remove(0))
        .expect("string ID literal/generated value must be non-empty");
    let endpoint_id = EndpointIdText::new(args.remove(0))
        .expect("string ID literal/generated value must be non-empty");
    let vision_id =
        VisionId::new(args.remove(0)).expect("string ID literal/generated value must be non-empty");
    let ticket = args
        .first()
        .map(PathBuf::from)
        .map(|path| read_ticket(&path))
        .transpose()?;
    let signature_present = binding_signature_ref.is_some();
    let response = execute_cli_peer_mutation(
        &config_path,
        &ledger_path,
        &socket_path,
        "/peers/add",
        &PeerAddRequest {
            expected_config_path: config_path.clone(),
            peer_node_id: peer_node_id.clone(),
            binding_id,
            endpoint_id,
            vision_id,
            ticket,
            binding_signature_ref,
            policy_revision: 1,
        },
    )?;
    println!(
        "peer added={} config={} peers={} signature_ref={}",
        peer_node_id,
        config_path.display(),
        response.peer_count,
        signature_present
    );
    Ok(())
}

pub(super) fn run_peers_set_outbound_proof(mut args: Vec<String>) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let socket_path = take_option(&mut args, "--uds")
        .map(PathBuf::from)
        .unwrap_or_else(default_control_uds_path);
    let signature_ref = take_option(&mut args, "--signature-ref").ok_or_else(|| {
        anyhow::anyhow!("peers set-outbound-proof requires --signature-ref proof")
    })?;
    let expires_at = take_option(&mut args, "--expires-at")
        .map(Timestamp::new)
        .transpose()
        .context("parse --expires-at timestamp")?;
    if args.len() < 2 {
        bail!(
            "expected: mct-daemon peers set-outbound-proof <peer-node-id> <binding-id> --signature-ref proof [--expires-at ts] [--config path] [--ledger path] [--uds socket-path]"
        );
    }
    let peer_node_id = MctNodeId::new(args.remove(0))
        .expect("string ID literal/generated value must be non-empty");
    let binding_id = PeerBindingId::new(args.remove(0))
        .expect("string ID literal/generated value must be non-empty");
    let response = execute_cli_peer_mutation(
        &config_path,
        &ledger_path,
        &socket_path,
        "/peers/proof",
        &PeerProofRequest {
            expected_config_path: config_path.clone(),
            peer_node_id: peer_node_id.clone(),
            binding_id,
            policy_revision: 1,
            signature_ref,
            expires_at,
        },
    )?;
    println!(
        "peer outbound proof set={} binding={} config={} expires_at={}",
        peer_node_id,
        response.binding_id,
        config_path.display(),
        response
            .expires_at
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "-".into())
    );
    Ok(())
}

pub(super) fn run_peers_list(mut args: Vec<String>) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let as_json = take_flag(&mut args, "--json");
    let config = MctDaemonConfigStore::new(&config_path).load()?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&config.peers)?);
        return Ok(());
    }
    println!("config={}", config_path.display());
    for peer in config.peers.values() {
        println!(
            "peer node={} endpoint={} binding={} vision={} ticket={} signature_ref={} outbound_proof={}",
            peer.peer_node_id,
            peer.endpoint_id,
            peer.binding_id,
            peer.vision_id,
            peer.ticket.is_some(),
            peer.binding_signature_ref.is_some(),
            peer.outbound_binding.is_some()
        );
    }
    Ok(())
}

pub(super) fn run_peers_revoke(mut args: Vec<String>) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let socket_path = take_option(&mut args, "--uds")
        .map(PathBuf::from)
        .unwrap_or_else(default_control_uds_path);
    if args.is_empty() {
        bail!(
            "expected: mct-daemon peers revoke <peer-node-id> [--config path] [--ledger path] [--uds socket-path]"
        );
    }
    let peer_node_id = MctNodeId::new(args.remove(0))
        .expect("string ID literal/generated value must be non-empty");
    let response = execute_cli_peer_mutation(
        &config_path,
        &ledger_path,
        &socket_path,
        "/peers/revoke",
        &PeerNodeRequest {
            expected_config_path: config_path.clone(),
            peer_node_id: peer_node_id.clone(),
        },
    )?;
    println!(
        "peer revoked={} state={:?} config={} peers={}",
        peer_node_id,
        response.binding_state,
        config_path.display(),
        response.peer_count
    );
    Ok(())
}

pub(super) fn run_peers_remove(mut args: Vec<String>) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let socket_path = take_option(&mut args, "--uds")
        .map(PathBuf::from)
        .unwrap_or_else(default_control_uds_path);
    if args.is_empty() {
        bail!(
            "expected: mct-daemon peers remove <peer-node-id> [--config path] [--ledger path] [--uds socket-path]"
        );
    }
    let peer_node_id = MctNodeId::new(args.remove(0))
        .expect("string ID literal/generated value must be non-empty");
    let response = execute_cli_peer_mutation(
        &config_path,
        &ledger_path,
        &socket_path,
        "/peers/remove",
        &PeerNodeRequest {
            expected_config_path: config_path.clone(),
            peer_node_id: peer_node_id.clone(),
        },
    )?;
    println!(
        "peer removed={} config={} peers={}",
        peer_node_id,
        config_path.display(),
        response.peer_count
    );
    Ok(())
}

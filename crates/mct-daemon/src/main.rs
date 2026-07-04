use anyhow::{Context, Result, bail};
use mct_daemon::{
    ChildInvocationProvenance, DEFAULT_WASM_MEMORY_LIMIT_BYTES, MctChildIntegrityMode,
    MctChildLoadOptions, MctCompositionPlan, MctCompositionStep, MctConfigChildAuthorityProjection,
    MctControlPlaneSnapshot, MctControlPlaneSnapshotError, MctControlPlaneSnapshotResult,
    MctDaemonConfigStore, MctDaemonStatus, MctLocalNodeIdentity, MctOperatorChildScope,
    MctOperatorNodeScope, MctPeerAddressBookEntry, MctProcessChildHarness,
    MctProcessChildInvocationIds, MctResidentStatus, MctRuntimeStateStore, MctToyAdapterRegistry,
    MctToyBackend, MctWasiHostConfig, MctWasiPreopen, MctWasiPreopenAccess,
    MctWasmComponentInvocationIds, MctWasmComponentRuntime, MctWasmHostConfig,
    MctWitHostImportAdapters, MctWitToyHostAdapter, build_federation_capability_view,
    build_metrics_snapshot, current_timestamp, daemon_status, daemon_status_with_resident,
    default_config_path, default_state_path, install_verified_child_package,
    load_children_from_dir, record_composition_plan, reload_configured_child,
    serve_http_control_once_with_snapshot_result, sync_child_registry_source,
    warmup_configured_child,
};
use mct_iroh::{
    MctIrohCallHandlerResult, MctIrohConcurrentServeConfig, MctIrohServeEvent, MctIrohServeState,
    MctIrohServedProtocol, MotherIrohEndpoint, MotherIrohEndpointConfig, MotherIrohEndpointError,
    MotherIrohEndpointSnapshot, MotherIrohEndpointTicket, MotherIrohRelayMode,
    load_or_create_node_secret_key_hex,
};
use mct_kernel::*;
use mct_observation::JsonlObservationLedger;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{net::TcpListener, sync::broadcast};

#[cfg(unix)]
use tokio::net::UnixListener;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        print_help();
        return Ok(());
    }

    match args.remove(0).as_str() {
        "version" => println!("mct-daemon {}", mct_daemon::version()),
        "status" => println!(
            "mct-daemon {} ready for local child loading and Iroh",
            mct_daemon::version()
        ),
        "serve" => run_serve(args).await?,
        "children" => run_children(args)?,
        "control" => run_control(args).await?,
        "process" => run_process(args)?,
        "peers" => run_peers(args)?,
        "state" => run_state(args)?,
        "runs" => run_runs(args)?,
        "slate" => run_slate(args)?,
        "toys" => run_toys(args)?,
        "wasm" => run_wasm(args)?,
        "federation" => run_federation(args)?,
        "iroh" => run_iroh(args).await?,
        "metrics" => run_metrics(args)?,
        "pando" => run_pando(args)?,
        "registry" => run_registry(args)?,
        "help" | "--help" | "-h" => print_help(),
        other => bail!("unknown command '{other}'"),
    }

    Ok(())
}

fn run_children(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!(
            "expected children subcommand: load | approve | revoke | approvals | warmup | reload"
        );
    }
    match args.remove(0).as_str() {
        "load" => run_children_load(args),
        "approve" => run_children_approve(args),
        "revoke" => run_children_revoke(args),
        "approvals" => run_children_approvals(args),
        "warmup" => run_children_warmup(args),
        "reload" => run_children_reload(args),
        other => bail!("unknown children subcommand '{other}'"),
    }
}

fn run_children_load(mut args: Vec<String>) -> Result<()> {
    let strict = take_flag(&mut args, "--strict-integrity");
    let as_json = take_flag(&mut args, "--json");
    let children_dir = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let mut options = MctChildLoadOptions::new(children_dir);
    if strict {
        options = options.strict_integrity();
    }
    let report = load_children_from_dir(options);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!(
        "Children: discovered={} loaded={} failed={} dir={}",
        report.discovered,
        report.loaded,
        report.failed,
        report.children_dir.display()
    );
    for child in &report.children {
        println!(
            "- {}@{} kind={} ingress={:?} wasm={} verified={}",
            child.name,
            child.version,
            child.kind,
            child.ingress_mode,
            child.wasm_path.display(),
            child.wasm_digest.verified && child.manifest_digest.verified
        );
    }
    for failure in &report.failures {
        println!("! {}: {}", failure.safe_message, failure.detail);
    }
    Ok(())
}

fn run_children_approve(mut args: Vec<String>) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let strict = take_flag(&mut args, "--strict-integrity");
    if args.is_empty() {
        bail!(
            "expected: mct-daemon children approve <child-name> [children-dir] [--config path] [--strict-integrity]"
        );
    }
    let child_name = args.remove(0);
    let children_dir = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let mut options = MctChildLoadOptions::new(children_dir);
    if strict {
        options = options.strict_integrity();
    }
    let report = load_children_from_dir(options);
    let child = report
        .children
        .iter()
        .find(|child| child.name == child_name)
        .ok_or_else(|| anyhow::anyhow!("loaded child '{child_name}' not found"))?;
    let config = MctDaemonConfigStore::new(&config_path)
        .approve_and_assign_loaded_child(child, MctOperatorChildScope::default())?;
    println!(
        "approved child={} config={} approvals={} assignments={}",
        child_name,
        config_path.display(),
        config.child_approvals.len(),
        config.child_assignments.len()
    );
    Ok(())
}

fn run_children_revoke(mut args: Vec<String>) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    if args.is_empty() {
        bail!("expected: mct-daemon children revoke <child-name> [--config path]");
    }
    let child_name = args.remove(0);
    let config = MctDaemonConfigStore::new(&config_path).revoke_child(&child_name)?;
    println!(
        "revoked child={} config={} approvals={} assignments={}",
        child_name,
        config_path.display(),
        config.child_approvals.len(),
        config.child_assignments.len()
    );
    Ok(())
}

fn run_children_approvals(mut args: Vec<String>) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let as_json = take_flag(&mut args, "--json");
    let config = MctDaemonConfigStore::new(&config_path).load()?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&config)?);
        return Ok(());
    }
    println!("config={}", config_path.display());
    for approval in config.child_approvals.values() {
        println!(
            "approval child={} artifact={} state={:?} vision={} node={}",
            approval.child_name,
            approval.artifact_id,
            approval.approval_state,
            approval.vision_id,
            approval.node_id
        );
    }
    for assignment in config.child_assignments.values() {
        println!(
            "assignment child={} artifact={} state={:?} vision={} node={}",
            assignment.child_name,
            assignment.artifact_id,
            assignment.assignment_state,
            assignment.vision_id,
            assignment.node_id
        );
    }
    Ok(())
}

fn run_children_warmup(args: Vec<String>) -> Result<()> {
    run_child_lifecycle(args, "warmup")
}

fn run_children_reload(args: Vec<String>) -> Result<()> {
    run_child_lifecycle(args, "reload")
}

fn run_child_lifecycle(mut args: Vec<String>, action: &str) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let as_json = take_flag(&mut args, "--json");
    if args.is_empty() {
        bail!(
            "expected: mct-daemon children {action} <child-name> [--children-dir path] [--config path] [--ledger path] [--state path] [--json]"
        );
    }
    let child_name = args.remove(0);
    let projection = load_configured_child_projection(&config_path, &children_dir)?;
    let state = MctRuntimeStateStore::open(&state_path)?;
    for artifact in &projection.artifacts {
        state.upsert_artifact(artifact)?;
    }
    for approval in &projection.approvals {
        state.upsert_child_approval(approval)?;
    }
    for assignment in &projection.assignments {
        state.upsert_child_assignment(assignment)?;
    }

    match action {
        "warmup" => {
            let report = warmup_configured_child(
                &projection,
                &child_name,
                TraceId::new(format!("trace-warmup:{child_name}"))
                    .expect("string ID literal/generated value must be non-empty"),
            )?;
            state.upsert_child_instance(&report.instance)?;
            append_ledger_observations(&ledger_path, &report.observations)?;
            if as_json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "warmup child={} instance={} state={:?}",
                    child_name, report.instance.instance_id, report.instance.instance_state
                );
            }
        }
        "reload" => {
            let report = reload_configured_child(
                &projection,
                &child_name,
                TraceId::new(format!("trace-reload:{child_name}"))
                    .expect("string ID literal/generated value must be non-empty"),
            )?;
            state.upsert_child_instance(&report.previous_instance)?;
            state.upsert_child_instance(&report.next_instance)?;
            append_ledger_observations(&ledger_path, &report.observations)?;
            if as_json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "reload child={} previous={} next={} state={:?}",
                    child_name,
                    report.previous_instance.instance_id,
                    report.next_instance.instance_id,
                    report.next_instance.instance_state
                );
            }
        }
        other => bail!("unsupported lifecycle action '{other}'"),
    }
    Ok(())
}

fn run_process(mut args: Vec<String>) -> Result<()> {
    if args.first().map(String::as_str) != Some("call") || args.len() < 2 {
        bail!(
            "expected: mct-daemon process call <executable> [payload-json] [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]"
        );
    }
    args.remove(0);
    let child_name = take_option(&mut args, "--child")
        .ok_or_else(|| anyhow::anyhow!("process call requires --child <approved-child-name>"))?;
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let executable = PathBuf::from(args.remove(0));
    let payload = args.first().cloned().unwrap_or_else(|| "{}".into());
    if !args.is_empty() {
        args.remove(0);
    }
    let target = OperationTarget {
        namespace: args.first().cloned().unwrap_or_else(|| "patina".into()),
        interface_name: args.get(1).cloned().unwrap_or_else(|| "echo".into()),
        function_name: args.get(2).cloned().unwrap_or_else(|| "echo".into()),
    };
    let call = local_process_call(target, payload.len() as u64);
    let (authorized, authority_observation) =
        authorize_configured_child_for_call(&config_path, &children_dir, &child_name, &call)?;
    append_ledger_observations(&ledger_path, std::slice::from_ref(&authority_observation))?;

    let state = MctRuntimeStateStore::open(&state_path)?;
    let run_id = run_id_for_call("process", &call);
    let child_invocation_provenance = ChildInvocationProvenance::from_authorized(
        &authorized,
        authority_observation.observation_id.clone(),
    );
    state.insert_run_started(
        &run_id,
        &call,
        RuntimeKind::Process,
        Some(&child_invocation_provenance),
        mct_daemon::current_timestamp_string(),
    )?;
    state.append_run_observations(&run_id, std::slice::from_ref(&authority_observation))?;

    let harness = MctProcessChildHarness {
        executable,
        args: Vec::new(),
        timeout: Duration::from_secs(5),
        local_node_id: MctNodeId::new("local-mct")
            .expect("string ID literal/generated value must be non-empty"),
    };
    let report = harness.invoke_authorized_child(
        authorized,
        &call,
        &payload,
        MctProcessChildInvocationIds {
            started_observation_id: ObservationId::new(format!(
                "obs-cli-process-started:{}",
                call.call_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            completed_observation_id: ObservationId::new(format!(
                "obs-cli-process-completed:{}",
                call.call_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            result_ref: ResultRef::new(format!("result-cli-process:{}", call.call_id))
                .expect("string ID literal/generated value must be non-empty"),
            audit_ref: AuditRef::new(format!("audit-cli-process:{}", call.call_id))
                .expect("string ID literal/generated value must be non-empty"),
            started_at: current_timestamp(),
            completed_at: current_timestamp(),
        },
    )?;
    append_ledger_observations(&ledger_path, &report.observations)?;
    state.append_run_observations(&run_id, &report.observations)?;
    state.complete_run(
        &run_id,
        &report.result,
        mct_daemon::current_timestamp_string(),
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn run_wasm(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected wasm subcommand: call | call-wit");
    }
    match args.remove(0).as_str() {
        "call" => run_wasm_call(args),
        "call-wit" => run_wasm_call_wit(args),
        other => bail!("unknown wasm subcommand '{other}'"),
    }
}

fn run_wasm_call(mut args: Vec<String>) -> Result<()> {
    if args.len() < 2 {
        bail!(
            "expected: mct-daemon wasm call <component-file> <export-name> [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]"
        );
    }
    let child_name = take_option(&mut args, "--child")
        .ok_or_else(|| anyhow::anyhow!("wasm call requires --child <approved-child-name>"))?;
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let component_path = PathBuf::from(args.remove(0));
    ensure_wasm_component_matches_loaded_child(&children_dir, &child_name, &component_path)?;
    let export_name = args.remove(0);
    let target = OperationTarget {
        namespace: args.first().cloned().unwrap_or_else(|| "patina".into()),
        interface_name: args.get(1).cloned().unwrap_or_else(|| export_name.clone()),
        function_name: args.get(2).cloned().unwrap_or_else(|| export_name.clone()),
    };
    let call = local_wasm_call(target);
    let (authorized, authority_observation) =
        authorize_configured_child_for_call(&config_path, &children_dir, &child_name, &call)?;
    append_ledger_observations(&ledger_path, std::slice::from_ref(&authority_observation))?;

    let state = MctRuntimeStateStore::open(&state_path)?;
    let run_id = run_id_for_call("wasm", &call);
    let child_invocation_provenance = ChildInvocationProvenance::from_authorized(
        &authorized,
        authority_observation.observation_id.clone(),
    );
    state.insert_run_started(
        &run_id,
        &call,
        RuntimeKind::WasmComponent,
        Some(&child_invocation_provenance),
        mct_daemon::current_timestamp_string(),
    )?;
    state.append_run_observations(&run_id, std::slice::from_ref(&authority_observation))?;

    let runtime = MctWasmComponentRuntime::new(default_wasm_host_config())?;
    let report = runtime.invoke_authorized_s32_export(
        authorized,
        &call,
        component_path,
        &export_name,
        MctWasmComponentInvocationIds {
            started_observation_id: ObservationId::new(format!(
                "obs-cli-wasm-started:{}",
                call.call_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            completed_observation_id: ObservationId::new(format!(
                "obs-cli-wasm-completed:{}",
                call.call_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            audit_ref: AuditRef::new(format!("audit-cli-wasm:{}", call.call_id))
                .expect("string ID literal/generated value must be non-empty"),
            started_at: current_timestamp(),
            completed_at: current_timestamp(),
        },
    )?;
    append_ledger_observations(&ledger_path, &report.observations)?;
    state.append_run_observations(&run_id, &report.observations)?;
    state.complete_run(
        &run_id,
        &report.result,
        mct_daemon::current_timestamp_string(),
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn run_wasm_call_wit(mut args: Vec<String>) -> Result<()> {
    if args.len() < 3 {
        bail!(
            "expected: mct-daemon wasm call-wit <child-name> <operation-id> <args-json> [--project-root path] [--guest-project /project] [--git-repo path] [--children-dir path] [--config path] [--ledger path] [--state path]"
        );
    }
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let project_root = take_option(&mut args, "--project-root")
        .map(|path| canonical_dir(PathBuf::from(path), "project root"))
        .transpose()?;
    let guest_project =
        take_option(&mut args, "--guest-project").unwrap_or_else(|| "/project".into());
    let git_repo = take_option(&mut args, "--git-repo")
        .map(|path| canonical_dir(PathBuf::from(path), "git repo"))
        .transpose()?;

    let child_name = args.remove(0);
    let operation_id = args.remove(0);
    let args_json: serde_json::Value = serde_json::from_str(&args.remove(0))
        .context("parse WIT args JSON; expected a JSON array")?;
    let target = operation_target_from_wit_operation_id(&operation_id)?;
    let call = local_wasm_call(target);
    let child = load_named_child(&children_dir, &child_name)?;
    let (authorized, authority_observation) =
        authorize_configured_child_for_call(&config_path, &children_dir, &child_name, &call)?;
    append_ledger_observations(&ledger_path, std::slice::from_ref(&authority_observation))?;

    let state = MctRuntimeStateStore::open(&state_path)?;
    let run_id = run_id_for_call("wasm-wit", &call);
    let child_invocation_provenance = ChildInvocationProvenance::from_authorized(
        &authorized,
        authority_observation.observation_id.clone(),
    );
    state.insert_run_started(
        &run_id,
        &call,
        RuntimeKind::WasmComponent,
        Some(&child_invocation_provenance),
        mct_daemon::current_timestamp_string(),
    )?;
    state.append_run_observations(&run_id, std::slice::from_ref(&authority_observation))?;

    let import_component_path = child.wasm_path.clone();
    let imports = run_wit_runtime_on_blocking_thread(move || {
        let runtime = MctWasmComponentRuntime::new(default_wasm_host_config())?;
        Ok(runtime.discover_wit_imports(import_component_path)?)
    })?;
    let adapter_build = match build_wit_host_adapters_for_cli_call(CliWitAdapterRequest {
        state: &state,
        child: &child,
        authorized_child: &authorized,
        call: &call,
        imports: &imports,
        project_root: project_root.as_deref(),
        guest_project: &guest_project,
        git_repo: git_repo.as_deref(),
    }) {
        Ok(build) => build,
        Err(error) => {
            append_ledger_observations(&ledger_path, &error.observations)?;
            state.append_run_observations(&run_id, &error.observations)?;
            bail!(error.safe_message);
        }
    };
    append_ledger_observations(&ledger_path, &adapter_build.observations)?;
    state.append_run_observations(&run_id, &adapter_build.observations)?;

    let invoke_authorized = authorized;
    let invoke_child = child.clone();
    let invoke_call = call.clone();
    let report = run_wit_runtime_on_blocking_thread(move || {
        let runtime = MctWasmComponentRuntime::new(default_wasm_host_config())?;
        Ok(
            runtime.invoke_authorized_child_wit_export_with_host_adapters(
                invoke_authorized,
                &invoke_child,
                &invoke_call,
                &args_json,
                adapter_build.adapters,
                MctWasmComponentInvocationIds {
                    started_observation_id: ObservationId::new(format!(
                        "obs-cli-wasm-wit-started:{}",
                        invoke_call.call_id
                    ))
                    .expect("string ID literal/generated value must be non-empty"),
                    completed_observation_id: ObservationId::new(format!(
                        "obs-cli-wasm-wit-completed:{}",
                        invoke_call.call_id
                    ))
                    .expect("string ID literal/generated value must be non-empty"),
                    audit_ref: AuditRef::new(format!("audit-cli-wasm-wit:{}", invoke_call.call_id))
                        .expect("string ID literal/generated value must be non-empty"),
                    started_at: current_timestamp(),
                    completed_at: current_timestamp(),
                },
            )?,
        )
    })?;
    append_ledger_observations(&ledger_path, &report.observations)?;
    state.append_run_observations(&run_id, &report.observations)?;
    state.complete_run(
        &run_id,
        &report.result,
        mct_daemon::current_timestamp_string(),
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

struct CliWitHostAdapterBuild {
    adapters: MctWitHostImportAdapters,
    observations: Vec<MctObservation>,
}

struct CliToyAuthorizationError {
    safe_message: String,
    observations: Vec<MctObservation>,
}

struct CliWitAdapterRequest<'a> {
    state: &'a MctRuntimeStateStore,
    child: &'a mct_daemon::MctLoadedChild,
    authorized_child: &'a AuthorizedChildInvocation,
    call: &'a MctCall,
    imports: &'a BTreeSet<String>,
    project_root: Option<&'a Path>,
    guest_project: &'a str,
    git_repo: Option<&'a Path>,
}

fn build_wit_host_adapters_for_cli_call(
    request: CliWitAdapterRequest<'_>,
) -> std::result::Result<CliWitHostAdapterBuild, CliToyAuthorizationError> {
    let contracts = request.state.toy_contracts().map_err(cli_adapter_error)?;
    let grants = request
        .state
        .toy_grant_snapshots()
        .map_err(cli_adapter_error)?;
    let resource_id = request.project_root.map(|path| path.display().to_string());
    let mut observations = Vec::new();
    let mut toy_registry = MctToyAdapterRegistry::new();
    let mut logging = None;
    let mut measure = None;
    let mut git = None;
    let mut wasi_preopens = Vec::new();

    if request.imports.contains("wasi:logging/logging@0.1.0") {
        let authorized = authorize_cli_toy(CliToyAuthorizationRequest {
            child: request.child,
            authorized_child: request.authorized_child,
            call: request.call,
            contracts: &contracts,
            grants: &grants,
            toy_id: slate_logging_toy_id(),
            action: "invoke",
            resource_id: resource_id.clone(),
            label: "logging",
        })?;
        observations.push(toy_grant_evaluation_observation(
            request.call.trace_context.trace_id.clone(),
            current_timestamp(),
            &authorized.evaluation,
        ));
        toy_registry.register(slate_logging_toy_id(), MctToyBackend::EchoJson);
        logging = Some(wit_toy_adapter(
            authorized.authorized,
            "obs-cli-wit-logging",
        ));
    }

    if request.imports.contains("patina:measure/measure@0.1.0") {
        let authorized = authorize_cli_toy(CliToyAuthorizationRequest {
            child: request.child,
            authorized_child: request.authorized_child,
            call: request.call,
            contracts: &contracts,
            grants: &grants,
            toy_id: slate_measure_toy_id(),
            action: "invoke",
            resource_id: resource_id.clone(),
            label: "measure",
        })?;
        observations.push(toy_grant_evaluation_observation(
            request.call.trace_context.trace_id.clone(),
            current_timestamp(),
            &authorized.evaluation,
        ));
        toy_registry.register(slate_measure_toy_id(), MctToyBackend::EchoJson);
        measure = Some(wit_toy_adapter(
            authorized.authorized,
            "obs-cli-wit-measure",
        ));
    }

    if request.imports.contains("patina:git/git@0.1.0") {
        let repo_root =
            request
                .git_repo
                .or(request.project_root)
                .ok_or_else(|| CliToyAuthorizationError {
                    safe_message: "WIT git import requires --git-repo or --project-root".into(),
                    observations: observations.clone(),
                })?;
        let authorized = authorize_cli_toy(CliToyAuthorizationRequest {
            child: request.child,
            authorized_child: request.authorized_child,
            call: request.call,
            contracts: &contracts,
            grants: &grants,
            toy_id: slate_git_toy_id(),
            action: "invoke",
            resource_id: resource_id.clone(),
            label: "git",
        })?;
        observations.push(toy_grant_evaluation_observation(
            request.call.trace_context.trace_id.clone(),
            current_timestamp(),
            &authorized.evaluation,
        ));
        toy_registry.register(
            slate_git_toy_id(),
            MctToyBackend::GitCommand {
                repo_root: repo_root.to_path_buf(),
            },
        );
        git = Some(wit_toy_adapter(authorized.authorized, "obs-cli-wit-git"));
    }

    if imports_need_wasi_p2(request.imports) && imports_need_wasi_filesystem(request.imports) {
        let project_root = request
            .project_root
            .ok_or_else(|| CliToyAuthorizationError {
                safe_message: "WIT filesystem imports require --project-root".into(),
                observations: observations.clone(),
            })?;
        let authorized = authorize_cli_toy(CliToyAuthorizationRequest {
            child: request.child,
            authorized_child: request.authorized_child,
            call: request.call,
            contracts: &contracts,
            grants: &grants,
            toy_id: slate_filesystem_toy_id(),
            action: "preopen-project-root",
            resource_id: resource_id.clone(),
            label: "filesystem",
        })?;
        observations.push(toy_grant_evaluation_observation(
            request.call.trace_context.trace_id.clone(),
            current_timestamp(),
            &authorized.evaluation,
        ));
        wasi_preopens.push(MctWasiPreopen {
            host_path: project_root.to_path_buf(),
            guest_path: request.guest_project.to_owned(),
            access: MctWasiPreopenAccess::ReadWrite,
        });
    }

    let wasi = imports_need_wasi_p2(request.imports).then_some(MctWasiHostConfig {
        preopens: wasi_preopens,
    });

    Ok(CliWitHostAdapterBuild {
        adapters: MctWitHostImportAdapters {
            toy_registry,
            logging,
            measure,
            git,
            wasi,
        },
        observations,
    })
}

struct CliAuthorizedToy {
    evaluation: ToyGrantEvaluation,
    authorized: AuthorizedToyCall,
}

struct CliToyAuthorizationRequest<'a> {
    child: &'a mct_daemon::MctLoadedChild,
    authorized_child: &'a AuthorizedChildInvocation,
    call: &'a MctCall,
    contracts: &'a [CanonicalToyContract],
    grants: &'a [ToyGrant],
    toy_id: ToyId,
    action: &'a str,
    resource_id: Option<String>,
    label: &'a str,
}

fn authorize_cli_toy(
    request: CliToyAuthorizationRequest<'_>,
) -> std::result::Result<CliAuthorizedToy, CliToyAuthorizationError> {
    let result = evaluate_toy_grant_for_call(
        request.call,
        &ToyGrantEvaluationRequest {
            toy_id: request.toy_id.clone(),
            subject: ToyGrantSubject {
                child_name: request.child.name.clone(),
                artifact_id: request.child.artifact_id.clone(),
                artifact_version: request.child.version.clone(),
                assignment_id: Some(request.authorized_child.assignment_id().clone()),
                caller_node_id: Some(request.call.caller.node_id.clone()),
            },
            child_instance_id: request.authorized_child.child_instance_id().clone(),
            action: request.action.into(),
            resource_id: request.resource_id,
            node_id: request.call.caller.node_id.clone(),
            now: current_timestamp(),
            ids: ToyGrantEvaluationIds {
                evaluation_id: ToyGrantEvaluationId::new(format!("toy-eval-cli-{}", request.label))
                    .expect("string ID literal/generated value must be non-empty"),
                decision_id: DecisionId::new(format!("decision-toy-cli-{}", request.label))
                    .expect("string ID literal/generated value must be non-empty"),
                observation_id: ObservationId::new(format!("obs-toy-grant-cli-{}", request.label))
                    .expect("string ID literal/generated value must be non-empty"),
                authorized_toy_call_id: AuthorizedToyCallId::new(format!(
                    "authorized-toy-cli-{}",
                    request.label
                ))
                .expect("string ID literal/generated value must be non-empty"),
            },
        },
        request.contracts,
        request.grants,
    );
    let Some(authorized) = result.authorized else {
        let observation = toy_grant_evaluation_observation(
            request.call.trace_context.trace_id.clone(),
            current_timestamp(),
            &result.evaluation,
        );
        return Err(CliToyAuthorizationError {
            safe_message: format!(
                "toy grant denied for {}: {:?}",
                request.label, result.evaluation.reason_code
            ),
            observations: vec![observation],
        });
    };
    Ok(CliAuthorizedToy {
        evaluation: result.evaluation,
        authorized,
    })
}

fn cli_adapter_error(error: anyhow::Error) -> CliToyAuthorizationError {
    CliToyAuthorizationError {
        safe_message: error.to_string(),
        observations: Vec::new(),
    }
}

fn wit_toy_adapter(
    authorized_toy_call: AuthorizedToyCall,
    observation_id_prefix: &str,
) -> MctWitToyHostAdapter {
    MctWitToyHostAdapter {
        authorized_toy_call,
        observation_id_prefix: observation_id_prefix.into(),
        observed_at: current_timestamp(),
    }
}

fn imports_need_wasi_p2(imports: &BTreeSet<String>) -> bool {
    imports
        .iter()
        .any(|name| name.starts_with("wasi:") && name != "wasi:logging/logging@0.1.0")
}

fn imports_need_wasi_filesystem(imports: &BTreeSet<String>) -> bool {
    imports.iter().any(|name| {
        matches!(
            name.as_str(),
            "wasi:filesystem/types@0.2.3" | "wasi:filesystem/preopens@0.2.3"
        )
    })
}

fn load_named_child(children_dir: &Path, child_name: &str) -> Result<mct_daemon::MctLoadedChild> {
    let report = load_children_from_dir(MctChildLoadOptions::new(children_dir));
    report
        .children
        .into_iter()
        .find(|child| child.name == child_name)
        .ok_or_else(|| anyhow::anyhow!("loaded child '{child_name}' not found"))
}

fn operation_target_from_wit_operation_id(operation_id: &str) -> Result<OperationTarget> {
    let (interface, function_name) = operation_id.rsplit_once('.').ok_or_else(|| {
        anyhow::anyhow!("WIT operation id must be '<package>:<interface-path>.<function>'")
    })?;
    let (namespace, interface_name) = interface.split_once('/').ok_or_else(|| {
        anyhow::anyhow!("WIT operation id must include '<namespace>/<interface>'")
    })?;
    Ok(OperationTarget {
        namespace: namespace.into(),
        interface_name: interface_name.into(),
        function_name: function_name.into(),
    })
}

fn run_wit_runtime_on_blocking_thread<T>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T>
where
    T: Send + 'static,
{
    std::thread::spawn(f)
        .join()
        .map_err(|panic| anyhow::anyhow!("WIT runtime worker panicked: {panic:?}"))?
}

fn canonical_dir(path: PathBuf, label: &str) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    let canonical = std::fs::canonicalize(&absolute)
        .with_context(|| format!("resolve {label} '{}'", absolute.display()))?;
    if !canonical.is_dir() {
        bail!("{label} '{}' is not a directory", canonical.display());
    }
    Ok(canonical)
}

fn slate_logging_toy_id() -> ToyId {
    ToyId::new("toy:slate:wasi-logging")
        .expect("string ID literal/generated value must be non-empty")
}

fn slate_measure_toy_id() -> ToyId {
    ToyId::new("toy:slate:patina-measure")
        .expect("string ID literal/generated value must be non-empty")
}

fn slate_git_toy_id() -> ToyId {
    ToyId::new("toy:slate:patina-git").expect("string ID literal/generated value must be non-empty")
}

fn slate_filesystem_toy_id() -> ToyId {
    ToyId::new("toy:slate:wasi-filesystem-project")
        .expect("string ID literal/generated value must be non-empty")
}

fn run_slate(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected slate subcommand: list-work");
    }
    match args.remove(0).as_str() {
        "list-work" => run_slate_list_work(args),
        other => bail!("unknown slate subcommand '{other}'"),
    }
}

fn run_slate_list_work(mut args: Vec<String>) -> Result<()> {
    let project_root = take_option(&mut args, "--project-root")
        .ok_or_else(|| anyhow::anyhow!("slate list-work requires --project-root <path>"))?;
    let children_dir = take_option(&mut args, "--children-dir");
    let config_path = take_option(&mut args, "--config");
    let state_path = take_option(&mut args, "--state");
    let ledger_path = take_option(&mut args, "--ledger");
    let status = take_option(&mut args, "--status");
    let kind = take_option(&mut args, "--kind");
    if !args.is_empty() {
        bail!("unexpected slate list-work arguments: {}", args.join(" "));
    }

    let request = serde_json::json!([{
        "project": "/project",
        "status": status,
        "kind": kind,
    }]);
    let mut call_args = vec![
        "slate-manager".to_owned(),
        "patina:slate/control@0.1.0.list-work".to_owned(),
        request.to_string(),
        "--project-root".to_owned(),
        project_root,
    ];
    if let Some(children_dir) = children_dir {
        call_args.extend(["--children-dir".to_owned(), children_dir]);
    }
    if let Some(config_path) = config_path {
        call_args.extend(["--config".to_owned(), config_path]);
    }
    if let Some(state_path) = state_path {
        call_args.extend(["--state".to_owned(), state_path]);
    }
    if let Some(ledger_path) = ledger_path {
        call_args.extend(["--ledger".to_owned(), ledger_path]);
    }
    run_wasm_call_wit(call_args)
}

async fn run_serve(mut args: Vec<String>) -> Result<()> {
    let relay_default = take_flag(&mut args, "--relay-default");
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let identity_path = take_option(&mut args, "--identity")
        .map(PathBuf::from)
        .unwrap_or_else(default_identity_path);
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let max_concurrent_connections = take_option(&mut args, "--max-connections")
        .map(|value| value.parse::<usize>())
        .transpose()
        .context("parse --max-connections")?
        .unwrap_or(64);
    let http_addr = take_option(&mut args, "--http");
    let uds_path = take_option(&mut args, "--uds").map(PathBuf::from);
    if !args.is_empty() {
        bail!("unexpected serve arguments: {}", args.join(" "));
    }
    let control = match (http_addr, uds_path) {
        (Some(addr), None) => ResidentControlTransport::Http(addr),
        (None, Some(path)) => ResidentControlTransport::Uds(path),
        (None, None) => ResidentControlTransport::Http("127.0.0.1:9173".into()),
        (Some(_), Some(_)) => bail!("serve accepts only one control transport: --http or --uds"),
    };
    run_resident_mother(
        ResidentMotherConfig {
            config_path,
            identity_path,
            children_dir,
            state_path,
            ledger_path,
            control,
            relay_default,
            max_concurrent_connections,
        },
        resident_shutdown_signal(),
        None,
    )
    .await
}

#[derive(Clone, Debug)]
enum ResidentControlTransport {
    Http(String),
    Uds(PathBuf),
}

#[derive(Clone, Debug)]
struct ResidentMotherConfig {
    config_path: PathBuf,
    identity_path: PathBuf,
    children_dir: PathBuf,
    state_path: PathBuf,
    ledger_path: PathBuf,
    control: ResidentControlTransport,
    relay_default: bool,
    max_concurrent_connections: usize,
}

#[derive(Clone, Debug)]
struct ResidentStatusSource {
    endpoint: Arc<Mutex<MotherIrohEndpointSnapshot>>,
    accepted_connection_count: Arc<AtomicU64>,
    loaded_child_count: usize,
    approved_child_count: usize,
    binding_count: usize,
    ledger_path: PathBuf,
}

impl ResidentStatusSource {
    fn status(&self) -> MctDaemonStatus {
        daemon_status_with_resident(
            Some(
                self.endpoint
                    .lock()
                    .expect("resident endpoint status lock must not be poisoned")
                    .clone(),
            ),
            Some(MctResidentStatus {
                accepted_connection_count: self.accepted_connection_count.load(Ordering::SeqCst),
                loaded_child_count: self.loaded_child_count,
                approved_child_count: self.approved_child_count,
                binding_count: self.binding_count,
                ledger_sequence_tip: ledger_sequence_tip(&self.ledger_path),
            }),
        )
    }
}

fn ledger_sequence_tip(path: &Path) -> u64 {
    JsonlObservationLedger::open_read_only(path, "ledger-local", "local-mct")
        .and_then(|reader| reader.entries())
        .ok()
        .and_then(|entries| entries.last().map(|entry| entry.local_sequence))
        .unwrap_or(0)
}

async fn run_resident_mother<S>(
    config: ResidentMotherConfig,
    shutdown: S,
    ready: Option<tokio::sync::oneshot::Sender<MotherIrohEndpointTicket>>,
) -> Result<()>
where
    S: std::future::Future<Output = ()> + Send,
{
    if config.max_concurrent_connections == 0 {
        bail!("--max-connections must be greater than zero");
    }

    let config_store = MctDaemonConfigStore::new(&config.config_path);
    let identity = config_store.ensure_local_identity(
        MctOperatorNodeScope::default(),
        config.identity_path.clone(),
    )?;
    let secret_key_hex = load_or_create_node_secret_key_hex(&config.identity_path)?;
    let mut endpoint = MotherIrohEndpoint::bind(iroh_config(secret_key_hex, config.relay_default))
        .await
        .context("bind resident Mother Iroh endpoint")?;
    let snapshot = endpoint.snapshot();
    if snapshot.endpoint_id != identity.endpoint_id {
        bail!(
            "identity endpoint mismatch: config has {}, bound endpoint is {}",
            identity.endpoint_id,
            snapshot.endpoint_id
        );
    }
    let ticket = endpoint.ticket();
    let load_report = load_children_from_dir(MctChildLoadOptions::new(config.children_dir.clone()));
    let _state = MctRuntimeStateStore::open(&config.state_path)
        .with_context(|| format!("open runtime state {}", config.state_path.display()))?;
    drop(_state);
    let ledger = ResidentLedgerWriter::spawn(config.ledger_path.clone())?;

    let loaded_child_count = load_report.loaded;
    let resident_config = config_store.load()?;
    let approved_child_count = resident_config
        .child_approvals
        .values()
        .filter(|approval| approval.approval_state == ChildApprovalState::Approved)
        .count();
    let binding_count = resident_config.peers.len();
    let accepted_connection_count = Arc::new(AtomicU64::new(0));
    let endpoint_status = Arc::new(Mutex::new(snapshot.clone()));
    let status_source = Arc::new(ResidentStatusSource {
        endpoint: Arc::clone(&endpoint_status),
        accepted_connection_count: Arc::clone(&accepted_connection_count),
        loaded_child_count,
        approved_child_count,
        binding_count,
        ledger_path: config.ledger_path.clone(),
    });

    let (events, event_rx) = tokio::sync::mpsc::channel(256);
    let event_ledger = ledger.clone();
    let event_accepted_count = Arc::clone(&accepted_connection_count);
    let event_task = tokio::spawn(async move {
        record_iroh_serve_events(event_rx, event_ledger, event_accepted_count).await
    });

    let (shutdown_tx, _) = broadcast::channel(4);
    let control_task = spawn_resident_control_task(
        config.control.clone(),
        config.state_path.clone(),
        shutdown_tx.subscribe(),
        Some(status_source),
    )?;

    println!("mct resident mother endpoint_id={}", snapshot.endpoint_id);
    println!("ticket={}", ticket.to_json()?.replace('\n', ""));
    eprintln!(
        "mct resident mother children loaded={} failed={} bindings={} max_connections={}",
        loaded_child_count, load_report.failed, binding_count, config.max_concurrent_connections
    );
    if let Some(ready) = ready {
        let _ = ready.send(ticket.clone());
    }

    let config_path = config.config_path.clone();
    let execution_paths = ResidentExecutionPaths {
        config_path: config.config_path.clone(),
        children_dir: config.children_dir.clone(),
        state_path: config.state_path.clone(),
    };
    let execution_ledger = ledger.clone();
    let serve_result = tokio::select! {
        result = endpoint.serve_concurrent_with_binding_provider(
            MctIrohServeState::new(),
            MctIrohConcurrentServeConfig {
                max_concurrent_connections: config.max_concurrent_connections,
                events: Some(events),
                ..MctIrohConcurrentServeConfig::default()
            },
            current_timestamp,
            move || {
                let config_path = config_path.clone();
                async move { load_peer_bindings_for_iroh(config_path).await }
            },
            move |request, _evaluation| {
                let execution_paths = execution_paths.clone();
                let execution_ledger = execution_ledger.clone();
                async move { execute_resident_call(execution_paths, execution_ledger, request).await }
            },
        ) => result.map_err(anyhow::Error::from),
        _ = shutdown => Ok(()),
    };

    let _ = shutdown_tx.send(());
    endpoint.close().await;
    if let Ok(mut endpoint_status) = endpoint_status.lock() {
        *endpoint_status = endpoint.snapshot();
    }
    if let Err(error) = ledger
        .append(vec![resident_endpoint_observation(
            "obs-resident-mother-endpoint-closed",
            snapshot.endpoint_id.clone(),
            ObservationOutcome::Completed,
            "resident Mother endpoint closed",
        )])
        .await
    {
        eprintln!("ledger shutdown observation failed: {error}");
    }
    let _ = tokio::time::timeout(Duration::from_secs(2), event_task).await;
    control_task.abort();
    ledger.close().await;
    if let ResidentControlTransport::Uds(path) = &config.control {
        let _ = std::fs::remove_file(path);
    }
    serve_result
}

fn spawn_resident_control_task(
    control: ResidentControlTransport,
    state_path: PathBuf,
    shutdown: broadcast::Receiver<()>,
    status_source: Option<Arc<ResidentStatusSource>>,
) -> Result<tokio::task::JoinHandle<Result<()>>> {
    match control {
        ResidentControlTransport::Http(addr) => Ok(tokio::spawn(async move {
            serve_http_control_loop_until(state_path, addr, shutdown, status_source).await
        })),
        ResidentControlTransport::Uds(path) => Ok(tokio::spawn(async move {
            run_control_serve_uds_with_state_until(state_path, path, shutdown, status_source).await
        })),
    }
}

async fn load_peer_bindings_for_iroh(
    path: PathBuf,
) -> mct_iroh::MotherIrohEndpointResult<Vec<MctPeerBinding>> {
    tokio::task::spawn_blocking(move || {
        MctDaemonConfigStore::new(path)
            .load()
            .and_then(|config| config.peer_authority_projection())
    })
    .await
    .map_err(|source| MotherIrohEndpointError::ProtocolProvider {
        action: "join peer binding load",
        source: Box::new(source),
    })?
    .map(|projection| projection.bindings)
    .map_err(|source| MotherIrohEndpointError::ProtocolProvider {
        action: "load peer bindings",
        source: Box::new(std::io::Error::other(source.to_string())),
    })
}

async fn resident_shutdown_signal() {
    #[cfg(unix)]
    {
        let mut interrupt =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .expect("install SIGINT handler");
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn serve_http_control_loop(state_path: &Path, addr: &str) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let snapshot_source = ControlSnapshotSource::open(state_path);
    println!("mct daemon serving control http on {addr}");
    loop {
        serve_http_control_once_with_snapshot_result(
            &listener,
            control_snapshot(&snapshot_source).await,
        )
        .await?;
    }
}

async fn serve_http_control_loop_until(
    state_path: PathBuf,
    addr: String,
    mut shutdown: broadcast::Receiver<()>,
    status_source: Option<Arc<ResidentStatusSource>>,
) -> Result<()> {
    let listener = TcpListener::bind(&addr).await?;
    let snapshot_source = ControlSnapshotSource::open_with_status(&state_path, status_source);
    println!("mct daemon serving control http on {addr}");
    loop {
        tokio::select! {
            _ = shutdown.recv() => break,
            result = serve_http_control_once_with_snapshot_result(
                &listener,
                control_snapshot(&snapshot_source).await,
            ) => result?,
        }
    }
    Ok(())
}

#[derive(Clone)]
struct ResidentLedgerWriter {
    sender: tokio::sync::mpsc::Sender<ResidentLedgerWrite>,
}

struct ResidentLedgerWrite {
    observations: Vec<MctObservation>,
    ack: tokio::sync::oneshot::Sender<std::result::Result<(), String>>,
}

impl ResidentLedgerWriter {
    fn spawn(path: PathBuf) -> Result<Self> {
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-local", "local-mct")
            .with_context(|| format!("open observation ledger {}", path.display()))?;
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<ResidentLedgerWrite>(256);
        tokio::task::spawn_blocking(move || {
            while let Some(write) = receiver.blocking_recv() {
                let result = ledger
                    .append_batch_before_effect(
                        write.observations.into_iter(),
                        mct_daemon::current_timestamp_string(),
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string());
                let _ = write.ack.send(result);
            }
        });
        Ok(Self { sender })
    }

    async fn append(&self, observations: Vec<MctObservation>) -> Result<()> {
        if observations.is_empty() {
            return Ok(());
        }
        let (ack, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ResidentLedgerWrite { observations, ack })
            .await
            .context("send observations to resident ledger writer")?;
        rx.await
            .context("receive resident ledger writer acknowledgement")?
            .map_err(anyhow::Error::msg)
    }

    async fn close(self) {
        drop(self.sender);
    }
}

async fn record_iroh_serve_events(
    mut events: tokio::sync::mpsc::Receiver<MctIrohServeEvent>,
    ledger: ResidentLedgerWriter,
    accepted_connection_count: Arc<AtomicU64>,
) {
    while let Some(event) = events.recv().await {
        let observations = match event {
            MctIrohServeEvent::AcceptedConnection => {
                accepted_connection_count.fetch_add(1, Ordering::SeqCst);
                Vec::new()
            }
            MctIrohServeEvent::Served(served) => resident_observations_for_served_protocol(*served),
            MctIrohServeEvent::RefusedConnection => Vec::new(),
        };
        if let Err(error) = ledger.append(observations).await {
            eprintln!("resident ledger event write failed: {error}");
        }
    }
}

fn resident_observations_for_served_protocol(served: MctIrohServedProtocol) -> Vec<MctObservation> {
    match served {
        MctIrohServedProtocol::Hello {
            request,
            evaluation,
            ..
        } => vec![hello_evaluation_observation(
            request.trace_id,
            current_timestamp(),
            &evaluation,
        )],
        MctIrohServedProtocol::Call {
            request,
            evaluation,
            ..
        } => vec![call_protocol_evaluation_observation(
            request.call.trace_context.trace_id,
            current_timestamp(),
            &evaluation,
        )],
    }
}

fn resident_endpoint_observation(
    observation_id: &'static str,
    endpoint_id: EndpointIdText,
    outcome: ObservationOutcome,
    safe_message: &'static str,
) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::new(observation_id)
            .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::AdapterEffectCompleted,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: TraceId::new("trace-resident-mother")
                .expect("string ID literal/generated value must be non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(endpoint_id.to_string()),
        resource_id: Some("mct-iroh-endpoint".into()),
        policy_revision: Some(1),
        grants_revision: Some(1),
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: None,
    }
}

#[derive(Clone, Debug)]
struct ResidentExecutionPaths {
    config_path: PathBuf,
    children_dir: PathBuf,
    state_path: PathBuf,
}

#[derive(Debug)]
struct ResidentAuthorizedExecution {
    child: mct_daemon::MctLoadedChild,
    authorized: AuthorizedChildInvocation,
    authority_observation: MctObservation,
}

#[derive(Debug)]
enum ResidentAuthorizationOutcome {
    Authorized(Box<ResidentAuthorizedExecution>),
    Denied { observation: Box<MctObservation> },
}

#[derive(Clone, Debug)]
struct ResidentExecutionReport {
    result: MctResult,
    observations: Vec<MctObservation>,
}

async fn execute_resident_call(
    paths: ResidentExecutionPaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
) -> MctIrohCallHandlerResult {
    let authorization = match authorize_resident_child(paths.clone(), request.call.clone()).await {
        Ok(authorization) => authorization,
        Err(error) => {
            eprintln!("resident child authorization unavailable: {error}");
            return MctIrohCallHandlerResult::failed("runtime unavailable");
        }
    };

    let ResidentAuthorizationOutcome::Authorized(authorized) = authorization else {
        if let ResidentAuthorizationOutcome::Denied { observation } = authorization
            && let Err(error) = ledger.append(vec![*observation]).await
        {
            eprintln!("resident authority denial ledger write failed: {error}");
            return MctIrohCallHandlerResult::failed("observation ledger unavailable");
        }
        return MctIrohCallHandlerResult::denied();
    };

    if let Err(error) = ledger
        .append(vec![authorized.authority_observation.clone()])
        .await
    {
        eprintln!("resident authority ledger write failed: {error}");
        return MctIrohCallHandlerResult::failed("observation ledger unavailable");
    }

    let execution = match tokio::task::spawn_blocking(move || {
        execute_authorized_resident_child(paths, *authorized, request.call)
    })
    .await
    {
        Ok(Ok(report)) => report,
        Ok(Err(error)) => {
            eprintln!("resident child execution failed: {error}");
            return MctIrohCallHandlerResult::failed("runtime execution failed");
        }
        Err(error) => {
            eprintln!("resident child execution task failed: {error}");
            return MctIrohCallHandlerResult::failed("runtime execution failed");
        }
    };

    if let Err(error) = ledger.append(execution.observations.clone()).await {
        eprintln!("resident execution ledger write failed: {error}");
        return MctIrohCallHandlerResult::failed("observation ledger unavailable");
    }

    result_to_call_handler_result("result-resident", &execution.result)
}

async fn authorize_resident_child(
    paths: ResidentExecutionPaths,
    call: MctCall,
) -> Result<ResidentAuthorizationOutcome> {
    tokio::task::spawn_blocking(move || authorize_resident_child_blocking(&paths, &call))
        .await
        .context("join resident child authorization")?
}

fn authorize_resident_child_blocking(
    paths: &ResidentExecutionPaths,
    call: &MctCall,
) -> Result<ResidentAuthorizationOutcome> {
    let config = MctDaemonConfigStore::new(&paths.config_path).load()?;
    let load_report = load_children_from_dir(MctChildLoadOptions::new(paths.children_dir.clone()));
    let projection = config.authority_projection_for_loaded_children(
        load_report.children.iter(),
        MctOperatorChildScope::default(),
    );
    let mut authorized = Vec::new();
    let mut first_denial = None;

    for child in load_report
        .children
        .into_iter()
        .filter(|child| resident_child_accepts_call(child, call))
    {
        let result = projection.authorize_child_for_call(&child.name, call);
        let observation = child_call_authority_observation(
            call.trace_context.trace_id.clone(),
            current_timestamp(),
            &result.evaluation,
        );
        if let Some(authorized_child) = result.authorized {
            authorized.push(ResidentAuthorizedExecution {
                child,
                authorized: authorized_child,
                authority_observation: observation,
            });
        } else if first_denial.is_none() {
            first_denial = Some(observation);
        }
    }

    match authorized.len() {
        1 => Ok(ResidentAuthorizationOutcome::Authorized(Box::new(
            authorized.remove(0),
        ))),
        0 => {
            let observation = first_denial.unwrap_or_else(|| {
                let result = projection.authorize_child_for_call("resident-dispatch-missing", call);
                child_call_authority_observation(
                    call.trace_context.trace_id.clone(),
                    current_timestamp(),
                    &result.evaluation,
                )
            });
            Ok(ResidentAuthorizationOutcome::Denied {
                observation: Box::new(observation),
            })
        }
        _ => bail!("multiple approved resident children match call target"),
    }
}

fn resident_child_accepts_call(child: &mct_daemon::MctLoadedChild, call: &MctCall) -> bool {
    let operation_id = mct_daemon::operation_id_from_target(&call.target);
    match child.ingress_mode {
        mct_daemon::MctChildIngressMode::Handle => {
            child.allowed_operations.is_empty()
                || child
                    .allowed_operations
                    .iter()
                    .any(|allowed| allowed == &operation_id)
        }
        mct_daemon::MctChildIngressMode::Hybrid | mct_daemon::MctChildIngressMode::WitOnly => {
            child.allows_operation_target(&call.target)
        }
    }
}

fn execute_authorized_resident_child(
    paths: ResidentExecutionPaths,
    execution: ResidentAuthorizedExecution,
    call: MctCall,
) -> Result<ResidentExecutionReport> {
    let state = MctRuntimeStateStore::open(&paths.state_path)?;
    let runtime_kind = match execution.child.ingress_mode {
        mct_daemon::MctChildIngressMode::Handle => RuntimeKind::Process,
        mct_daemon::MctChildIngressMode::Hybrid | mct_daemon::MctChildIngressMode::WitOnly => {
            RuntimeKind::WasmComponent
        }
    };
    let run_id = run_id_for_call("resident", &call);
    let provenance = ChildInvocationProvenance::from_authorized(
        &execution.authorized,
        execution.authority_observation.observation_id.clone(),
    );
    state.insert_run_started(
        &run_id,
        &call,
        runtime_kind,
        Some(&provenance),
        mct_daemon::current_timestamp_string(),
    )?;
    state.append_run_observations(
        &run_id,
        std::slice::from_ref(&execution.authority_observation),
    )?;

    let report = match execution.child.ingress_mode {
        mct_daemon::MctChildIngressMode::Handle => {
            execute_resident_process_child(execution, &call)?
        }
        mct_daemon::MctChildIngressMode::Hybrid | mct_daemon::MctChildIngressMode::WitOnly => {
            execute_resident_wit_child(execution, &call)?
        }
    };
    state.append_run_observations(&run_id, &report.observations)?;
    state.complete_run(
        &run_id,
        &report.result,
        mct_daemon::current_timestamp_string(),
    )?;
    Ok(report)
}

fn execute_resident_process_child(
    execution: ResidentAuthorizedExecution,
    call: &MctCall,
) -> Result<ResidentExecutionReport> {
    let harness = MctProcessChildHarness {
        executable: execution.child.wasm_path.clone(),
        args: Vec::new(),
        timeout: Duration::from_secs(5),
        local_node_id: MctNodeId::new("local-mct")
            .expect("string ID literal/generated value must be non-empty"),
    };
    let report = harness.invoke_authorized_child(
        execution.authorized,
        call,
        "{}",
        MctProcessChildInvocationIds {
            started_observation_id: ObservationId::new(format!(
                "obs-resident-process-started:{}",
                call.call_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            completed_observation_id: ObservationId::new(format!(
                "obs-resident-process-completed:{}",
                call.call_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            result_ref: ResultRef::new(format!("result-resident-process:{}", call.call_id))
                .expect("string ID literal/generated value must be non-empty"),
            audit_ref: AuditRef::new(format!("audit-resident-process:{}", call.call_id))
                .expect("string ID literal/generated value must be non-empty"),
            started_at: current_timestamp(),
            completed_at: current_timestamp(),
        },
    )?;
    Ok(ResidentExecutionReport {
        result: report.result,
        observations: report.observations,
    })
}

fn execute_resident_wit_child(
    execution: ResidentAuthorizedExecution,
    call: &MctCall,
) -> Result<ResidentExecutionReport> {
    let runtime = MctWasmComponentRuntime::new(default_wasm_host_config())?;
    let report = runtime.invoke_authorized_child_wit_export_with_host_adapters(
        execution.authorized,
        &execution.child,
        call,
        &serde_json::json!([]),
        MctWitHostImportAdapters::none(),
        MctWasmComponentInvocationIds {
            started_observation_id: ObservationId::new(format!(
                "obs-resident-wasm-wit-started:{}",
                call.call_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            completed_observation_id: ObservationId::new(format!(
                "obs-resident-wasm-wit-completed:{}",
                call.call_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            audit_ref: AuditRef::new(format!("audit-resident-wasm-wit:{}", call.call_id))
                .expect("string ID literal/generated value must be non-empty"),
            started_at: current_timestamp(),
            completed_at: current_timestamp(),
        },
    )?;
    Ok(ResidentExecutionReport {
        result: report.result,
        observations: report.observations,
    })
}

fn result_to_call_handler_result(prefix: &str, result: &MctResult) -> MctIrohCallHandlerResult {
    match result.outcome {
        ResultOutcome::Success => MctIrohCallHandlerResult::completed(
            ResultRef::new(format!("{prefix}:{}", result.call_id))
                .expect("string ID literal/generated value must be non-empty"),
        ),
        ResultOutcome::TimedOut => MctIrohCallHandlerResult::timed_out(),
        ResultOutcome::Denied => MctIrohCallHandlerResult::denied(),
        ResultOutcome::Failed | ResultOutcome::Cancelled => {
            MctIrohCallHandlerResult::failed(result.requester_message.clone())
        }
    }
}

async fn run_control(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected control subcommand: serve-http | serve-uds");
    }
    match args.remove(0).as_str() {
        "serve-http" => run_control_serve_http(args).await,
        "serve-uds" => run_control_serve_uds(args).await,
        other => bail!("unknown control subcommand '{other}'"),
    }
}

async fn run_control_serve_http(mut args: Vec<String>) -> Result<()> {
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
async fn run_control_serve_uds(mut args: Vec<String>) -> Result<()> {
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
async fn run_control_serve_uds_with_state(state_path: PathBuf, socket_path: PathBuf) -> Result<()> {
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
async fn run_control_serve_uds_with_state_until(
    state_path: PathBuf,
    socket_path: PathBuf,
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
    loop {
        tokio::select! {
            _ = shutdown.recv() => break,
            result = mct_daemon::serve_uds_control_once_with_snapshot_result(
                &listener,
                control_snapshot(&snapshot_source).await,
            ) => result?,
        }
    }
    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}

#[cfg(not(unix))]
async fn run_control_serve_uds(_args: Vec<String>) -> Result<()> {
    bail!("UDS control plane is only available on Unix platforms")
}

#[cfg(not(unix))]
async fn run_control_serve_uds_with_state(
    _state_path: PathBuf,
    _socket_path: PathBuf,
) -> Result<()> {
    bail!("UDS control plane is only available on Unix platforms")
}

#[cfg(not(unix))]
async fn run_control_serve_uds_with_state_until(
    _state_path: PathBuf,
    _socket_path: PathBuf,
    _shutdown: broadcast::Receiver<()>,
    _status_source: Option<Arc<ResidentStatusSource>>,
) -> Result<()> {
    bail!("UDS control plane is only available on Unix platforms")
}

#[derive(Clone)]
enum ControlSnapshotSource {
    Store {
        state: Arc<Mutex<MctRuntimeStateStore>>,
        status_source: Option<Arc<ResidentStatusSource>>,
    },
    Unavailable,
}

impl ControlSnapshotSource {
    fn open(state_path: &Path) -> Self {
        Self::open_with_status(state_path, None)
    }

    fn open_with_status(
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

async fn control_snapshot(source: &ControlSnapshotSource) -> MctControlPlaneSnapshotResult {
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

fn resident_or_default_status(
    status_source: Option<&Arc<ResidentStatusSource>>,
) -> MctDaemonStatus {
    status_source.map_or_else(|| daemon_status(None), |source| source.status())
}

fn control_snapshot_from_state(
    state: &MctRuntimeStateStore,
    status: MctDaemonStatus,
) -> Result<MctControlPlaneSnapshot> {
    let summary = state.summary()?;
    let runs = state.list_runs(20)?;
    Ok(MctControlPlaneSnapshot::new(status, Some(summary), runs))
}

fn run_registry(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected registry subcommand: install | sync");
    }
    match args.remove(0).as_str() {
        "install" => run_registry_install(args),
        "sync" => run_registry_sync(args),
        other => bail!("unknown registry subcommand '{other}'"),
    }
}

fn run_registry_install(mut args: Vec<String>) -> Result<()> {
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let replace = take_flag(&mut args, "--replace");
    let as_json = take_flag(&mut args, "--json");
    if args.len() != 1 {
        bail!(
            "expected: mct-daemon registry install <verified-package-dir> [--children-dir path] [--replace] [--json]"
        );
    }
    let report = install_verified_child_package(PathBuf::from(&args[0]), children_dir, replace)?;
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

fn run_registry_sync(mut args: Vec<String>) -> Result<()> {
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
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
    let state = MctRuntimeStateStore::open(&state_path)?;
    let report = sync_child_registry_source(
        &state,
        source_id,
        children_dir,
        if strict {
            MctChildIntegrityMode::RequireSidecars
        } else {
            MctChildIntegrityMode::AuditOnly
        },
        MctOperatorChildScope::default(),
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

fn run_federation(mut args: Vec<String>) -> Result<()> {
    if args.first().map(String::as_str) != Some("view") {
        bail!("expected: mct-daemon federation view [--config path] [--state path] [--json]");
    }
    args.remove(0);
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let as_json = take_flag(&mut args, "--json");
    let config = MctDaemonConfigStore::new(&config_path).load()?;
    let summary = MctRuntimeStateStore::open(&state_path)?.summary()?;
    let view = build_federation_capability_view(
        &config,
        &summary,
        MctNodeId::new("local-mct").expect("string ID literal/generated value must be non-empty"),
        VisionId::new("vision-local").expect("string ID literal/generated value must be non-empty"),
    );
    if as_json {
        println!("{}", serde_json::to_string_pretty(&view)?);
    } else {
        println!(
            "federation node={} vision={} approved={} ready={} peers={}",
            view.node_id,
            view.vision_id,
            view.approved_children,
            view.ready_instances,
            view.peers.len()
        );
    }
    Ok(())
}

fn run_metrics(mut args: Vec<String>) -> Result<()> {
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

fn run_pando(mut args: Vec<String>) -> Result<()> {
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

fn parse_composition_step(raw: &str) -> Result<MctCompositionStep> {
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

fn parse_runtime_kind(value: &str) -> Result<RuntimeKind> {
    match value {
        "process" => Ok(RuntimeKind::Process),
        "jvm_child" | "jvm" => Ok(RuntimeKind::JvmChild),
        "wasm_component" | "wasm" => Ok(RuntimeKind::WasmComponent),
        "remote_peer" | "remote" => Ok(RuntimeKind::RemotePeer),
        "internal" => Ok(RuntimeKind::Internal),
        other => bail!("unknown runtime kind '{other}'"),
    }
}

fn run_toys(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected toys subcommand: authorize-slate");
    }
    match args.remove(0).as_str() {
        "authorize-slate" => run_toys_authorize_slate(args),
        other => bail!("unknown toys subcommand '{other}'"),
    }
}

fn run_toys_authorize_slate(mut args: Vec<String>) -> Result<()> {
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

fn slate_toy_contracts() -> Vec<CanonicalToyContract> {
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

fn slate_toy_contract(toy_id: ToyId, contract: ToyContractIdentity) -> CanonicalToyContract {
    CanonicalToyContract {
        admitted_by_observation_id: ObservationId::new(format!("obs:toy-catalog:{toy_id}"))
            .expect("string ID literal/generated value must be non-empty"),
        toy_id,
        contract,
        authority_bearing: true,
        catalog_revision: 1,
    }
}

fn slate_toy_grants_for_child(
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

fn run_state(mut args: Vec<String>) -> Result<()> {
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

fn run_runs(mut args: Vec<String>) -> Result<()> {
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

fn run_peers(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected peers subcommand: add | list | revoke | remove");
    }
    match args.remove(0).as_str() {
        "add" => run_peers_add(args),
        "list" => run_peers_list(args),
        "revoke" => run_peers_revoke(args),
        "remove" => run_peers_remove(args),
        other => bail!("unknown peers subcommand '{other}'"),
    }
}

fn run_peers_add(mut args: Vec<String>) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    if args.len() < 4 {
        bail!(
            "expected: mct-daemon peers add <peer-node-id> <binding-id> <endpoint-id> <vision-id> [ticket-file] [--config path]"
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
    let config = MctDaemonConfigStore::new(&config_path).upsert_peer(MctPeerAddressBookEntry {
        peer_node_id: peer_node_id.clone(),
        binding_id,
        endpoint_id,
        vision_id,
        ticket,
        binding_state: BindingState::Admitted,
        policy_revision: 1,
        updated_at: mct_daemon::current_timestamp_string(),
    })?;
    println!(
        "peer added={} config={} peers={}",
        peer_node_id,
        config_path.display(),
        config.peers.len()
    );
    Ok(())
}

fn run_peers_list(mut args: Vec<String>) -> Result<()> {
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
            "peer node={} endpoint={} binding={} vision={} ticket={}",
            peer.peer_node_id,
            peer.endpoint_id,
            peer.binding_id,
            peer.vision_id,
            peer.ticket.is_some()
        );
    }
    Ok(())
}

fn run_peers_revoke(mut args: Vec<String>) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    if args.is_empty() {
        bail!("expected: mct-daemon peers revoke <peer-node-id> [--config path]");
    }
    let peer_node_id = MctNodeId::new(args.remove(0))
        .expect("string ID literal/generated value must be non-empty");
    let config = MctDaemonConfigStore::new(&config_path).revoke_peer(&peer_node_id)?;
    let peer = config
        .peers
        .get(peer_node_id.as_str())
        .expect("revoked peer remains in config");
    println!(
        "peer revoked={} state={:?} config={} peers={}",
        peer_node_id,
        peer.binding_state,
        config_path.display(),
        config.peers.len()
    );
    Ok(())
}

fn run_peers_remove(mut args: Vec<String>) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    if args.is_empty() {
        bail!("expected: mct-daemon peers remove <peer-node-id> [--config path]");
    }
    let peer_node_id = MctNodeId::new(args.remove(0))
        .expect("string ID literal/generated value must be non-empty");
    let config = MctDaemonConfigStore::new(&config_path).remove_peer(&peer_node_id)?;
    println!(
        "peer removed={} config={} peers={}",
        peer_node_id,
        config_path.display(),
        config.peers.len()
    );
    Ok(())
}

fn local_wasm_call(target: OperationTarget) -> MctCall {
    MctCall {
        call_id: CallId::new("call-cli-wasm")
            .expect("string ID literal/generated value must be non-empty"),
        caller: CallerIdentity {
            node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            user_id: None,
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            project_id: None,
        },
        target,
        payload_metadata: PayloadMetadata {
            data_classification: "public".into(),
            approximate_size_bytes: 0,
            contains_secret_scoped_material: false,
        },
        authority_context: AuthorityContextSnapshot {
            policy_revision: 1,
            grants_revision: 1,
            vision_policy_revision: 1,
        },
        deadline: current_timestamp_after(DEFAULT_CLI_CALL_DEADLINE),
        trace_context: TraceContext {
            trace_id: TraceId::new("trace-cli-wasm")
                .expect("string ID literal/generated value must be non-empty"),
            span_id: SpanId::new("span-cli-wasm")
                .expect("string ID literal/generated value must be non-empty"),
        },
        origin: CallOrigin::WasmHost,
    }
}

fn local_process_call(target: OperationTarget, payload_size_bytes: u64) -> MctCall {
    MctCall {
        call_id: CallId::new("call-cli-process")
            .expect("string ID literal/generated value must be non-empty"),
        caller: CallerIdentity {
            node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            user_id: None,
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            project_id: None,
        },
        target,
        payload_metadata: PayloadMetadata {
            data_classification: "public".into(),
            approximate_size_bytes: payload_size_bytes,
            contains_secret_scoped_material: false,
        },
        authority_context: AuthorityContextSnapshot {
            policy_revision: 1,
            grants_revision: 1,
            vision_policy_revision: 1,
        },
        deadline: current_timestamp_after(DEFAULT_CLI_CALL_DEADLINE),
        trace_context: TraceContext {
            trace_id: TraceId::new("trace-cli-process")
                .expect("string ID literal/generated value must be non-empty"),
            span_id: SpanId::new("span-cli-process")
                .expect("string ID literal/generated value must be non-empty"),
        },
        origin: CallOrigin::ProcessHarness,
    }
}

async fn run_iroh(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected iroh subcommand: identity | serve | call");
    }
    match args.remove(0).as_str() {
        "identity" => {
            let config_path = take_option(&mut args, "--config")
                .map(PathBuf::from)
                .unwrap_or_else(default_config_path);
            let identity_path = args
                .first()
                .map(PathBuf::from)
                .unwrap_or_else(default_identity_path);
            let identity = MctDaemonConfigStore::new(&config_path)
                .ensure_local_identity(MctOperatorNodeScope::default(), &identity_path)?;
            println!("node_id={}", identity.node_id);
            println!("vision_id={}", identity.vision_id);
            println!("endpoint_id={}", identity.endpoint_id);
            println!("identity={}", identity.identity_path.display());
            println!("config={}", config_path.display());
        }
        "serve" => serve_iroh(args).await?,
        "serve-process" => serve_iroh_process(args).await?,
        "call" => call_iroh(args).await?,
        "call-peer" => call_iroh_peer(args).await?,
        other => bail!("unknown iroh subcommand '{other}'"),
    }
    Ok(())
}

const DEFAULT_CLI_CALL_DEADLINE: jiff::SignedDuration = jiff::SignedDuration::from_secs(60);

fn current_timestamp_after(budget: jiff::SignedDuration) -> Timestamp {
    let deadline = jiff::Timestamp::now()
        .checked_add(budget)
        .expect("CLI deadline budget is within jiff timestamp range");
    Timestamp::new(deadline.to_string()).expect("jiff produced RFC3339 timestamp")
}

fn default_wasm_host_config() -> MctWasmHostConfig {
    MctWasmHostConfig {
        memory_limit_bytes: DEFAULT_WASM_MEMORY_LIMIT_BYTES,
    }
}

async fn serve_iroh(mut args: Vec<String>) -> Result<()> {
    let relay_default = take_flag(&mut args, "--relay-default");
    if args.len() < 5 {
        bail!(
            "expected: mct-daemon iroh serve [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> [children-dir]"
        );
    }
    let identity_path = PathBuf::from(&args[0]);
    let binding_id = PeerBindingId::new(args[1].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let peer_endpoint_id = EndpointIdText::new(args[2].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let peer_node_id = MctNodeId::new(args[3].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let vision_id = VisionId::new(args[4].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let children_dir = args
        .get(5)
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);

    let secret_key_hex = load_or_create_node_secret_key_hex(&identity_path)?;
    let mut endpoint = MotherIrohEndpoint::bind(iroh_config(secret_key_hex, relay_default)).await?;
    let local_endpoint_id = endpoint.snapshot().endpoint_id;
    let ticket = endpoint.ticket();
    let load_report = load_children_from_dir(MctChildLoadOptions::new(children_dir));

    println!("mct iroh serving endpoint_id={local_endpoint_id}");
    println!("ticket={}", ticket.to_json()?.replace('\n', ""));
    println!(
        "children loaded={} failed={}",
        load_report.loaded, load_report.failed
    );

    let binding = cli_peer_binding(
        binding_id,
        peer_endpoint_id,
        peer_node_id,
        vision_id,
        identity_path,
        local_endpoint_id.clone(),
    );
    let result = endpoint
        .serve_concurrent_with_call_handler(
            MctIrohServeState::new(),
            vec![binding],
            MctIrohConcurrentServeConfig::default(),
            current_timestamp,
            |_, _| async {
                MctIrohCallHandlerResult::accepted_for_routing(Some(
                    ResultRef::new("result-mct-peer-call")
                        .expect("string ID literal/generated value must be non-empty"),
                ))
            },
        )
        .await;
    if let Err(error) = result {
        eprintln!("iroh serve error: {error}");
        endpoint.close().await;
        return Err(error.into());
    }
    Ok(())
}

async fn serve_iroh_process(mut args: Vec<String>) -> Result<()> {
    let relay_default = take_flag(&mut args, "--relay-default");
    let child_name = take_option(&mut args, "--child").ok_or_else(|| {
        anyhow::anyhow!("iroh serve-process requires --child <approved-child-name>")
    })?;
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    if args.len() < 6 {
        bail!(
            "expected: mct-daemon iroh serve-process [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> <executable> --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]"
        );
    }
    let identity_path = PathBuf::from(&args[0]);
    let binding_id = PeerBindingId::new(args[1].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let peer_endpoint_id = EndpointIdText::new(args[2].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let peer_node_id = MctNodeId::new(args[3].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let vision_id = VisionId::new(args[4].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let executable = PathBuf::from(&args[5]);

    let secret_key_hex = load_or_create_node_secret_key_hex(&identity_path)?;
    let mut endpoint = MotherIrohEndpoint::bind(iroh_config(secret_key_hex, relay_default)).await?;
    let local_endpoint_id = endpoint.snapshot().endpoint_id;
    let ticket = endpoint.ticket();
    println!("mct iroh process serving endpoint_id={local_endpoint_id}");
    println!("ticket={}", ticket.to_json()?.replace('\n', ""));

    let binding = cli_peer_binding(
        binding_id,
        peer_endpoint_id,
        peer_node_id,
        vision_id,
        identity_path,
        local_endpoint_id.clone(),
    );
    let harness = MctProcessChildHarness {
        executable,
        args: Vec::new(),
        timeout: Duration::from_secs(5),
        local_node_id: MctNodeId::new("local-mct")
            .expect("string ID literal/generated value must be non-empty"),
    };
    let projection = load_configured_child_projection(&config_path, &children_dir)?;
    let result = endpoint
        .serve_concurrent_with_call_handler(
            MctIrohServeState::new(),
            vec![binding],
            MctIrohConcurrentServeConfig::default(),
            current_timestamp,
            move |request, _evaluation| {
                let harness = harness.clone();
                let projection = projection.clone();
                let child_name = child_name.clone();
                let ledger_path = ledger_path.clone();
                let state_path = state_path.clone();
                async move {
                    let (authorized, authority_observation) =
                        match authorize_configured_child_from_projection(
                            &projection,
                            &child_name,
                            &request.call,
                        ) {
                            Ok(authorized) => authorized,
                            Err(error) => {
                                return MctIrohCallHandlerResult::failed(format!(
                                    "process child authority denied: {error}"
                                ));
                            }
                        };
                    let _ = append_ledger_observations(
                        &ledger_path,
                        std::slice::from_ref(&authority_observation),
                    );
                    let runtime_state = match MctRuntimeStateStore::open(&state_path) {
                        Ok(runtime_state) => runtime_state,
                        Err(error) => {
                            return MctIrohCallHandlerResult::failed(format!(
                                "runtime state unavailable: {error}"
                            ));
                        }
                    };
                    let run_id = run_id_for_call("iroh-process", &request.call);
                    let child_invocation_provenance = ChildInvocationProvenance::from_authorized(
                        &authorized,
                        authority_observation.observation_id.clone(),
                    );
                    if let Err(error) = runtime_state.insert_run_started(
                        &run_id,
                        &request.call,
                        RuntimeKind::Process,
                        Some(&child_invocation_provenance),
                        mct_daemon::current_timestamp_string(),
                    ) {
                        return MctIrohCallHandlerResult::failed(format!(
                            "runtime run could not start: {error}"
                        ));
                    }
                    let _ = runtime_state.append_run_observations(
                        &run_id,
                        std::slice::from_ref(&authority_observation),
                    );
                    let report = match harness.invoke_authorized_child(
                        authorized,
                        &request.call,
                        "{}",
                        MctProcessChildInvocationIds {
                            started_observation_id: ObservationId::new(format!(
                                "obs-iroh-process-started:{}",
                                request.call.call_id
                            ))
                            .expect("string ID literal/generated value must be non-empty"),
                            completed_observation_id: ObservationId::new(format!(
                                "obs-iroh-process-completed:{}",
                                request.call.call_id
                            ))
                            .expect("string ID literal/generated value must be non-empty"),
                            result_ref: ResultRef::new(format!(
                                "result-iroh-process:{}",
                                request.call.call_id
                            ))
                            .expect("string ID literal/generated value must be non-empty"),
                            audit_ref: AuditRef::new(format!(
                                "audit-iroh-process:{}",
                                request.call.call_id
                            ))
                            .expect("string ID literal/generated value must be non-empty"),
                            started_at: current_timestamp(),
                            completed_at: current_timestamp(),
                        },
                    ) {
                        Ok(report) => report,
                        Err(error) => {
                            return MctIrohCallHandlerResult::failed(format!(
                                "process child failed: {error}"
                            ));
                        }
                    };
                    let _ = append_ledger_observations(&ledger_path, &report.observations);
                    let _ = runtime_state.append_run_observations(&run_id, &report.observations);
                    let _ = runtime_state.complete_run(
                        &run_id,
                        &report.result,
                        mct_daemon::current_timestamp_string(),
                    );
                    match report.result.outcome {
                        ResultOutcome::Success => MctIrohCallHandlerResult::completed(
                            ResultRef::new(format!("result-iroh-process:{}", request.call.call_id))
                                .expect("string ID literal/generated value must be non-empty"),
                        ),
                        ResultOutcome::TimedOut => MctIrohCallHandlerResult::timed_out(),
                        ResultOutcome::Failed
                        | ResultOutcome::Denied
                        | ResultOutcome::Cancelled => {
                            MctIrohCallHandlerResult::failed(report.result.requester_message)
                        }
                    }
                }
            },
        )
        .await;
    if let Err(error) = result {
        eprintln!("iroh process serve error: {error}");
        endpoint.close().await;
        return Err(error.into());
    }
    Ok(())
}

async fn call_iroh(mut args: Vec<String>) -> Result<()> {
    let relay_default = take_flag(&mut args, "--relay-default");
    if args.len() < 5 {
        bail!(
            "expected: mct-daemon iroh call [--relay-default] <identity-file> <peer-ticket-file> <binding-id> <local-node-id> <vision-id> [namespace interface function]"
        );
    }
    let identity_path = PathBuf::from(&args[0]);
    let peer_ticket_path = PathBuf::from(&args[1]);
    let binding_id = PeerBindingId::new(args[2].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let local_node_id = MctNodeId::new(args[3].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let vision_id = VisionId::new(args[4].as_str())
        .expect("string ID literal/generated value must be non-empty");
    let target = OperationTarget {
        namespace: args.get(5).cloned().unwrap_or_else(|| "patina".into()),
        interface_name: args.get(6).cloned().unwrap_or_else(|| "echo".into()),
        function_name: args.get(7).cloned().unwrap_or_else(|| "echo".into()),
    };

    let secret_key_hex = load_or_create_node_secret_key_hex(&identity_path)?;
    let mut endpoint = MotherIrohEndpoint::bind(iroh_config(secret_key_hex, relay_default)).await?;
    let local_endpoint_id = endpoint.snapshot().endpoint_id;
    let peer_ticket = read_ticket(&peer_ticket_path)?;
    let trace_id = TraceId::new("trace-cli-iroh-call")
        .expect("string ID literal/generated value must be non-empty");
    let hello_request = cli_hello_request(
        &local_endpoint_id,
        &binding_id,
        &local_node_id,
        &vision_id,
        &trace_id,
    );
    let hello_response = endpoint.send_hello(&peer_ticket, &hello_request).await?;
    println!("{}", serde_json::to_string_pretty(&hello_response)?);

    let call_request = cli_call_request(
        &local_endpoint_id,
        &binding_id,
        &local_node_id,
        &vision_id,
        &trace_id,
        target,
        &hello_response,
    );
    let call_reply = endpoint.send_call(&peer_ticket, &call_request).await?;
    println!("{}", serde_json::to_string_pretty(&call_reply)?);
    endpoint.close().await;
    Ok(())
}

async fn call_iroh_peer(mut args: Vec<String>) -> Result<()> {
    let relay_default = take_flag(&mut args, "--relay-default");
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    if args.len() < 2 {
        bail!(
            "expected: mct-daemon iroh call-peer [--relay-default] <identity-file> <peer-node-id> [namespace interface function] [--config path]"
        );
    }
    let identity_path = PathBuf::from(args.remove(0));
    let peer_node_id = MctNodeId::new(args.remove(0))
        .expect("string ID literal/generated value must be non-empty");
    let target = OperationTarget {
        namespace: args.first().cloned().unwrap_or_else(|| "patina".into()),
        interface_name: args.get(1).cloned().unwrap_or_else(|| "echo".into()),
        function_name: args.get(2).cloned().unwrap_or_else(|| "echo".into()),
    };
    let config = MctDaemonConfigStore::new(&config_path).load()?;
    let peer = config.peers.get(peer_node_id.as_str()).ok_or_else(|| {
        anyhow::anyhow!(
            "peer '{peer_node_id}' not found in {}",
            config_path.display()
        )
    })?;
    let peer_ticket = peer
        .ticket
        .clone()
        .ok_or_else(|| anyhow::anyhow!("peer '{peer_node_id}' has no endpoint ticket"))?;

    let secret_key_hex = load_or_create_node_secret_key_hex(&identity_path)?;
    let mut endpoint = MotherIrohEndpoint::bind(iroh_config(secret_key_hex, relay_default)).await?;
    let local_endpoint_id = endpoint.snapshot().endpoint_id;
    let trace_id = TraceId::new("trace-cli-iroh-call-peer")
        .expect("string ID literal/generated value must be non-empty");
    let hello_request = cli_hello_request(
        &local_endpoint_id,
        &peer.binding_id,
        &MctNodeId::new("local-mct").expect("string ID literal/generated value must be non-empty"),
        &peer.vision_id,
        &trace_id,
    );
    let hello_response = endpoint.send_hello(&peer_ticket, &hello_request).await?;
    println!("{}", serde_json::to_string_pretty(&hello_response)?);

    let call_request = cli_call_request(
        &local_endpoint_id,
        &peer.binding_id,
        &MctNodeId::new("local-mct").expect("string ID literal/generated value must be non-empty"),
        &peer.vision_id,
        &trace_id,
        target,
        &hello_response,
    );
    let call_reply = endpoint.send_call(&peer_ticket, &call_request).await?;
    println!("{}", serde_json::to_string_pretty(&call_reply)?);
    endpoint.close().await;
    Ok(())
}

fn cli_hello_request(
    endpoint_id: &EndpointIdText,
    binding_id: &PeerBindingId,
    node_id: &MctNodeId,
    vision_id: &VisionId,
    trace_id: &TraceId,
) -> MctHelloRequest {
    MctHelloRequest {
        hello_id: "hello-cli".into(),
        received_over: IrohConnectionPresentation {
            endpoint_id: endpoint_id.clone(),
            alpn: MCT_HELLO_ALPN.into(),
            connection_side: ConnectionSide::Outgoing,
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
        capability_view: None,
        local_policy_revision_seen: Some(1),
        trace_id: trace_id.clone(),
        received_observation_id: ObservationId::new("obs-cli-hello-received")
            .expect("string ID literal/generated value must be non-empty"),
    }
}

fn cli_call_request(
    endpoint_id: &EndpointIdText,
    binding_id: &PeerBindingId,
    node_id: &MctNodeId,
    vision_id: &VisionId,
    trace_id: &TraceId,
    target: OperationTarget,
    hello: &MctHelloResponse,
) -> MctCallProtocolRequest {
    let call = MctCall {
        call_id: CallId::new("call-cli-iroh")
            .expect("string ID literal/generated value must be non-empty"),
        caller: CallerIdentity {
            node_id: node_id.clone(),
            user_id: None,
            vision_id: vision_id.clone(),
            project_id: None,
        },
        target,
        payload_metadata: PayloadMetadata {
            data_classification: "public".into(),
            approximate_size_bytes: 0,
            contains_secret_scoped_material: false,
        },
        authority_context: AuthorityContextSnapshot {
            policy_revision: 1,
            grants_revision: 1,
            vision_policy_revision: 1,
        },
        deadline: current_timestamp_after(DEFAULT_CLI_CALL_DEADLINE),
        trace_context: TraceContext {
            trace_id: trace_id.clone(),
            span_id: SpanId::new("span-cli-call")
                .expect("string ID literal/generated value must be non-empty"),
        },
        origin: CallOrigin::Iroh,
    };

    MctCallProtocolRequest {
        protocol_request_id: ProtocolRequestId::new("proto-cli-call")
            .expect("string ID literal/generated value must be non-empty"),
        authority: MctCallProtocolAuthority {
            hello_decision_id: hello.decision_id.clone(),
            peer_binding_id: binding_id.clone(),
            vision_id: vision_id.clone(),
            accepted_alpn: MCT_CALL_ALPN.into(),
            endpoint_id: endpoint_id.clone(),
            policy_revision: 1,
            grants_revision: 1,
        },
        received_over: IrohConnectionPresentation {
            endpoint_id: endpoint_id.clone(),
            alpn: MCT_CALL_ALPN.into(),
            connection_side: ConnectionSide::Outgoing,
            path_class: PathClass::Direct,
            relay_url: None,
            presented_capability_ref: None,
        },
        call,
        payload: MctCallPayloadHandle::Empty,
        idempotency_key: Some("idem-cli-call".into()),
        received_observation_id: ObservationId::new("obs-cli-call-received")
            .expect("string ID literal/generated value must be non-empty"),
    }
}

fn load_configured_child_projection(
    config_path: &Path,
    children_dir: &Path,
) -> Result<MctConfigChildAuthorityProjection> {
    let config = MctDaemonConfigStore::new(config_path).load()?;
    let load_report = load_children_from_dir(MctChildLoadOptions::new(children_dir));
    Ok(config.authority_projection_for_loaded_children(
        load_report.children.iter(),
        MctOperatorChildScope::default(),
    ))
}

fn authorize_configured_child_for_call(
    config_path: &Path,
    children_dir: &Path,
    child_name: &str,
    call: &MctCall,
) -> Result<(AuthorizedChildInvocation, MctObservation)> {
    let projection = load_configured_child_projection(config_path, children_dir)?;
    authorize_configured_child_from_projection(&projection, child_name, call)
}

fn authorize_configured_child_from_projection(
    projection: &MctConfigChildAuthorityProjection,
    child_name: &str,
    call: &MctCall,
) -> Result<(AuthorizedChildInvocation, MctObservation)> {
    let result = projection.authorize_child_for_call(child_name, call);
    let observation = child_call_authority_observation(
        call.trace_context.trace_id.clone(),
        current_timestamp(),
        &result.evaluation,
    );
    let authorized = result.authorized.ok_or_else(|| {
        anyhow::anyhow!(
            "child '{child_name}' not authorized for {}.{}.{}: {:?}",
            call.target.namespace,
            call.target.interface_name,
            call.target.function_name,
            result.evaluation.reason_code
        )
    })?;
    Ok((authorized, observation))
}

fn ensure_wasm_component_matches_loaded_child(
    children_dir: &Path,
    child_name: &str,
    component_path: &Path,
) -> Result<()> {
    let load_report = load_children_from_dir(MctChildLoadOptions::new(children_dir));
    let child = load_report
        .children
        .iter()
        .find(|child| child.name == child_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "child '{child_name}' not found in {}",
                children_dir.display()
            )
        })?;
    let expected = child
        .wasm_path
        .canonicalize()
        .unwrap_or_else(|_| child.wasm_path.clone());
    let actual = component_path
        .canonicalize()
        .unwrap_or_else(|_| component_path.to_path_buf());
    if expected != actual {
        bail!(
            "wasm component {} does not match approved child '{}' artifact {}",
            component_path.display(),
            child_name,
            child.wasm_path.display()
        );
    }
    Ok(())
}

fn append_ledger_observations(ledger_path: &Path, observations: &[MctObservation]) -> Result<()> {
    if observations.is_empty() {
        return Ok(());
    }
    let mut ledger = JsonlObservationLedger::open(ledger_path, "ledger-local", "local-mct")?;
    ledger.append_batch_before_effect(
        observations.iter().cloned(),
        mct_daemon::current_timestamp_string(),
    )?;
    Ok(())
}

fn run_id_for_call(prefix: &str, call: &MctCall) -> String {
    format!(
        "run:{}:{}:{}",
        prefix,
        call.call_id,
        mct_daemon::current_timestamp_string()
    )
}

fn default_observation_ledger_path() -> PathBuf {
    PathBuf::from(".mct").join("observations.jsonl")
}

fn iroh_config(secret_key_hex: String, relay_default: bool) -> MotherIrohEndpointConfig {
    let mut config = MotherIrohEndpointConfig::local_mct().with_secret_key_hex(secret_key_hex);
    if relay_default {
        config = config.with_relay_mode(MotherIrohRelayMode::Default);
    }
    config
}

fn cli_peer_binding(
    binding_id: PeerBindingId,
    endpoint_id: EndpointIdText,
    peer_node_id: MctNodeId,
    vision_id: VisionId,
    identity_path: PathBuf,
    local_endpoint_id: EndpointIdText,
) -> MctPeerBinding {
    let local_identity = MctLocalNodeIdentity {
        node_id: MctNodeId::new("local-mct")
            .expect("string ID literal/generated value must be non-empty"),
        vision_id: VisionId::new("vision-local")
            .expect("string ID literal/generated value must be non-empty"),
        endpoint_id: local_endpoint_id,
        identity_path,
        policy_revision: 1,
        updated_at: mct_daemon::current_timestamp_string(),
    };
    MctPeerAddressBookEntry {
        peer_node_id,
        binding_id,
        endpoint_id,
        vision_id,
        ticket: None,
        binding_state: BindingState::Admitted,
        policy_revision: 1,
        updated_at: local_identity.updated_at.clone(),
    }
    .to_peer_binding(&local_identity)
    .expect("CLI peer binding timestamp is generated as RFC3339")
}

fn read_ticket(path: &Path) -> Result<MotherIrohEndpointTicket> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("reading peer ticket {}", path.display()))?;
    MotherIrohEndpointTicket::from_json(&json).map_err(Into::into)
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(index) = args.iter().position(|arg| arg == flag) {
        args.remove(index);
        true
    } else {
        false
    }
}

fn take_option(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.remove(index);
    if index < args.len() {
        Some(args.remove(index))
    } else {
        None
    }
}

fn default_children_dir() -> PathBuf {
    PathBuf::from(".mct").join("children")
}

fn default_identity_path() -> PathBuf {
    PathBuf::from(".mct")
        .join("identity")
        .join("iroh-secret.hex")
}

fn print_help() {
    println!(
        "mct-daemon {version}\n\nCommands:\n  status\n  serve [--identity path] [--config path] [--children-dir path] [--state path] [--ledger path] [--max-connections n] [--relay-default] [--http addr | --uds socket-path]\n  control serve-http [addr] [--state path]\n  control serve-uds [socket-path] [--state path]\n  registry install <verified-package-dir> [--children-dir path] [--replace] [--json]\n  registry sync <source-id> [children-dir] [--state path] [--strict-integrity] [--json]\n  federation view [--config path] [--state path] [--json]\n  metrics snapshot [--state path] [--json]\n  pando record <composition-id> [step-id,call-id,runtime,child,decision ...] [--state path] [--json]\n  children load [children-dir] [--strict-integrity] [--json]\n  process call <executable> [payload-json] [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  children approve <child-name> [children-dir] [--config path] [--strict-integrity]\n  children revoke <child-name> [--config path]\n  children approvals [--config path] [--json]\n  children warmup <child-name> [--children-dir path] [--config path] [--ledger path] [--state path] [--json]\n  children reload <child-name> [--children-dir path] [--config path] [--ledger path] [--state path] [--json]\n  peers add <peer-node-id> <binding-id> <endpoint-id> <vision-id> [ticket-file] [--config path]\n  peers list [--config path] [--json]\n  peers revoke <peer-node-id> [--config path]\n  peers remove <peer-node-id> [--config path]\n  state summary [--state path] [--json]\n  runs list [--state path] [--json] [--limit n]\n  slate list-work --project-root path [--status status] [--kind kind] [--children-dir path] [--config path] [--state path] [--ledger path]\n  toys authorize-slate <child-name> <project-root> [--children-dir path] [--config path] [--state path] [--json]\n  wasm call <component-file> <export-name> [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  wasm call-wit <child-name> <operation-id> <args-json> [--project-root path] [--guest-project /project] [--git-repo path] [--children-dir path] [--config path] [--ledger path] [--state path]\n  iroh identity [identity-file] [--config path]\n  iroh serve [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> [children-dir]\n  iroh serve-process [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> <executable> --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  iroh call [--relay-default] <identity-file> <peer-ticket-file> <binding-id> <local-node-id> <vision-id> [namespace interface function]\n  iroh call-peer [--relay-default] <identity-file> <peer-node-id> [namespace interface function] [--config path]",
        version = mct_daemon::version()
    );
}

#[cfg(test)]
#[path = "authority_test_fixture.rs"]
mod authority_test_fixture;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_call() -> MctCall {
        MctCall {
            call_id: CallId::new("call-cli-toy-expiry")
                .expect("string ID literal/generated value must be non-empty"),
            caller: CallerIdentity {
                node_id: MctNodeId::new("local-mct")
                    .expect("string ID literal/generated value must be non-empty"),
                user_id: None,
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                project_id: None,
            },
            target: OperationTarget {
                namespace: "patina:demo".into(),
                interface_name: "control@0.1.0".into(),
                function_name: "run".into(),
            },
            payload_metadata: PayloadMetadata {
                data_classification: "public".into(),
                approximate_size_bytes: 0,
                contains_secret_scoped_material: false,
            },
            authority_context: AuthorityContextSnapshot {
                policy_revision: 1,
                grants_revision: 1,
                vision_policy_revision: 1,
            },
            deadline: Timestamp::new("2026-07-02T00:01:00Z").unwrap(),
            trace_context: TraceContext {
                trace_id: TraceId::new("trace-cli-toy-expiry")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: SpanId::new("span-cli-toy-expiry")
                    .expect("string ID literal/generated value must be non-empty"),
            },
            origin: CallOrigin::Cli,
        }
    }

    fn test_child() -> mct_daemon::MctLoadedChild {
        mct_daemon::MctLoadedChild {
            child_id: ChildId::new("child-demo")
                .expect("string ID literal/generated value must be non-empty"),
            name: "child-demo".into(),
            version: "0.1.0".into(),
            description: None,
            kind: "wasm".into(),
            role: None,
            wasm_path: PathBuf::from("child-demo.wasm"),
            manifest_path: PathBuf::from("child.toml"),
            wasm_digest: mct_daemon::MctChildFileDigest {
                sha256: "wasm".into(),
                sidecar_present: true,
                verified: true,
            },
            manifest_digest: mct_daemon::MctChildFileDigest {
                sha256: "manifest".into(),
                sidecar_present: true,
                verified: true,
            },
            artifact_id: "artifact-demo".into(),
            ingress_mode: mct_daemon::MctChildIngressMode::WitOnly,
            allowed_operations: vec!["patina:demo/control@0.1.0.run".into()],
            requested_toys: Vec::new(),
            subscribed_streams: Vec::new(),
            relationship_listens: Vec::new(),
            wasm_size_bytes: 1,
            instance_state: mct_daemon::MctChildInstanceState::Ready,
        }
    }

    fn test_authorized_child() -> AuthorizedChildInvocation {
        authority_test_fixture::authorized_child_for_call(
            &test_call(),
            "child-demo",
            MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            "child",
        )
    }

    fn test_contract(toy_id: &ToyId) -> CanonicalToyContract {
        CanonicalToyContract {
            toy_id: toy_id.clone(),
            contract: ToyContractIdentity {
                namespace: "patina".into(),
                interface_name: "demo-toy".into(),
                version: "0.1.0".into(),
                function_name: Some("read".into()),
                resource_name: None,
            },
            authority_bearing: true,
            catalog_revision: 1,
            admitted_by_observation_id: ObservationId::new("obs-contract")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn expired_grant(toy_id: &ToyId) -> ToyGrant {
        ToyGrant {
            grant_id: ToyGrantId::new("grant-expired")
                .expect("string ID literal/generated value must be non-empty"),
            toy_id: toy_id.clone(),
            subject: ToyGrantSubject {
                child_name: "child-demo".into(),
                artifact_id: "artifact-demo".into(),
                artifact_version: "0.1.0".into(),
                assignment_id: Some(
                    ChildAssignmentId::new("assignment-child")
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
                resource_id: Some("resource-a".into()),
                allowed_actions: vec!["read".into()],
            },
            constraints: ToyGrantConstraints {
                starts_at: None,
                expires_at: Some(Timestamp::new("2026-06-01T00:00:00Z").unwrap()),
                max_uses: None,
                max_duration_ms: None,
                locality_required: false,
            },
            grant_state: ToyGrantState::Active,
            issuer_id: "issuer".into(),
            policy_revision: 1,
            grants_revision: 1,
            authority_observation_id: ObservationId::new("obs-grant")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    #[tokio::test]
    async fn resident_mother_serves_peer_control_and_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let identity_path = dir.path().join("identity").join("iroh-secret.hex");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        let children_dir = dir.path().join("children");
        write_resident_process_child(&children_dir);

        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let client_endpoint_id = client.snapshot().endpoint_id;
        let store = MctDaemonConfigStore::new(&config_path);
        store
            .ensure_local_identity(MctOperatorNodeScope::default(), &identity_path)
            .unwrap();
        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        store
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        store
            .upsert_peer(MctPeerAddressBookEntry {
                peer_node_id: MctNodeId::new("mother-client")
                    .expect("string ID literal/generated value must be non-empty"),
                binding_id: PeerBindingId::new("binding-resident-client")
                    .expect("string ID literal/generated value must be non-empty"),
                endpoint_id: client_endpoint_id.clone(),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                ticket: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let resident = tokio::spawn(run_resident_mother(
            ResidentMotherConfig {
                config_path,
                identity_path,
                children_dir,
                state_path,
                ledger_path: ledger_path.clone(),
                control: ResidentControlTransport::Uds(socket_path.clone()),
                relay_default: false,
                max_concurrent_connections: 8,
            },
            async move {
                let _ = shutdown_rx.await;
            },
            Some(ready_tx),
        ));
        let ticket = tokio::time::timeout(Duration::from_secs(10), ready_rx)
            .await
            .unwrap()
            .unwrap();

        let trace_id = TraceId::new("trace-resident-mother-test")
            .expect("string ID literal/generated value must be non-empty");
        let binding_id = PeerBindingId::new("binding-resident-client")
            .expect("string ID literal/generated value must be non-empty");
        let client_node_id = MctNodeId::new("mother-client")
            .expect("string ID literal/generated value must be non-empty");
        let vision_id = VisionId::new("vision-local")
            .expect("string ID literal/generated value must be non-empty");
        let hello = cli_hello_request(
            &client_endpoint_id,
            &binding_id,
            &client_node_id,
            &vision_id,
            &trace_id,
        );
        let hello_response = client.send_hello(&ticket, &hello).await.unwrap();
        assert_eq!(hello_response.hello_outcome, HelloOutcome::Admitted);
        let call = cli_call_request(
            &client_endpoint_id,
            &binding_id,
            &client_node_id,
            &vision_id,
            &trace_id,
            OperationTarget {
                namespace: "patina:demo".into(),
                interface_name: "control@0.1.0".into(),
                function_name: "run".into(),
            },
            &hello_response,
        );
        let reply = client.send_call(&ticket, &call).await.unwrap();
        assert_eq!(reply.reply_outcome, CallProtocolReplyOutcome::Success);

        let status = poll_resident_status(&socket_path, |status| {
            status
                .resident
                .as_ref()
                .is_some_and(|resident| resident.accepted_connection_count >= 2)
        })
        .await;
        assert_eq!(
            status.iroh_endpoint.as_ref().unwrap().endpoint_id,
            ticket.endpoint_id
        );
        let resident_status = status.resident.expect("resident status is present");
        assert!(
            resident_status.accepted_connection_count >= 2,
            "{resident_status:?}"
        );
        assert_eq!(resident_status.loaded_child_count, 1);
        assert_eq!(resident_status.approved_child_count, 1);
        assert_eq!(resident_status.binding_count, 1);
        assert!(
            resident_status.ledger_sequence_tip >= 2,
            "{resident_status:?}"
        );

        let _ = shutdown_tx.send(());
        tokio::time::timeout(Duration::from_secs(10), resident)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(!socket_path.exists());
        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        let trace_entries = entries
            .iter()
            .filter(|entry| entry.observation.trace.trace_id == trace_id)
            .collect::<Vec<_>>();
        assert!(
            trace_entries
                .iter()
                .any(|entry| entry.observation.kind == ObservationKind::RouteRevalidated),
            "{trace_entries:?}"
        );
        assert!(
            trace_entries.iter().any(|entry| {
                entry.observation.kind == ObservationKind::RuntimeExecutionCompleted
            }),
            "{trace_entries:?}"
        );
        client.close().await;
    }

    #[tokio::test]
    async fn resident_execution_runs_wit_child_and_records_trace() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_wit_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let trace_id = TraceId::new("trace-resident-wit-test")
            .expect("string ID literal/generated value must be non-empty");
        let call = resident_test_call(trace_id.clone());
        let request = resident_test_protocol_request(call);

        let result = execute_resident_call(
            ResidentExecutionPaths {
                config_path,
                children_dir,
                state_path,
            },
            ledger.clone(),
            request,
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Completed);
        ledger.close().await;

        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        let trace_entries = entries
            .iter()
            .filter(|entry| entry.observation.trace.trace_id == trace_id)
            .collect::<Vec<_>>();
        assert!(
            trace_entries
                .iter()
                .any(|entry| entry.observation.kind == ObservationKind::RouteRevalidated),
            "{trace_entries:?}"
        );
        assert!(
            trace_entries.iter().any(|entry| {
                entry.observation.kind == ObservationKind::RuntimeExecutionCompleted
            }),
            "{trace_entries:?}"
        );
    }

    async fn poll_resident_status(
        socket_path: &Path,
        ready: impl Fn(&MctDaemonStatus) -> bool,
    ) -> MctDaemonStatus {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut last = None;
        for _ in 0..40 {
            let mut control = tokio::net::UnixStream::connect(socket_path).await.unwrap();
            control
                .write_all(b"GET /status HTTP/1.1\r\nHost: local\r\n\r\n")
                .await
                .unwrap();
            let mut response = vec![0; 4096];
            let read = control.read(&mut response).await.unwrap();
            let response = String::from_utf8_lossy(&response[..read]);
            assert!(response.starts_with("HTTP/1.1 200"), "{response}");
            let (_, body) = response
                .split_once("\r\n\r\n")
                .expect("HTTP response separates headers from body");
            let status: MctDaemonStatus = serde_json::from_str(body).unwrap();
            if ready(&status) {
                return status;
            }
            last = Some(status);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("resident status did not become ready: {last:?}");
    }

    fn resident_test_call(trace_id: TraceId) -> MctCall {
        let mut call = local_wasm_call(OperationTarget {
            namespace: "patina:demo".into(),
            interface_name: "control@0.1.0".into(),
            function_name: "run".into(),
        });
        call.call_id = CallId::new("call-resident-wit")
            .expect("string ID literal/generated value must be non-empty");
        call.trace_context.trace_id = trace_id;
        call.origin = CallOrigin::Iroh;
        call
    }

    fn resident_test_protocol_request(call: MctCall) -> MctCallProtocolRequest {
        MctCallProtocolRequest {
            protocol_request_id: ProtocolRequestId::new("proto-resident-wit")
                .expect("string ID literal/generated value must be non-empty"),
            authority: MctCallProtocolAuthority {
                hello_decision_id: DecisionId::new("decision-resident-wit-hello")
                    .expect("string ID literal/generated value must be non-empty"),
                peer_binding_id: PeerBindingId::new("binding-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                accepted_alpn: MCT_CALL_ALPN.into(),
                endpoint_id: EndpointIdText::new("endpoint-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                policy_revision: 1,
                grants_revision: 1,
            },
            received_over: IrohConnectionPresentation {
                endpoint_id: EndpointIdText::new("endpoint-resident-wit")
                    .expect("string ID literal/generated value must be non-empty"),
                alpn: MCT_CALL_ALPN.into(),
                connection_side: ConnectionSide::Incoming,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            call,
            payload: MctCallPayloadHandle::Empty,
            idempotency_key: None,
            received_observation_id: ObservationId::new("obs-resident-wit-received")
                .expect("string ID literal/generated value must be non-empty"),
        }
    }

    fn write_resident_process_child(children_dir: &Path) {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        let child_dir = children_dir.join("resident-echo");
        std::fs::create_dir_all(&child_dir).unwrap();
        let artifact_path = child_dir.join("resident-echo.wasm");
        let manifest_path = child_dir.join("child.toml");
        let script = b"#!/bin/sh\ncat >/dev/null\nprintf '{\\\"ok\\\":true}'\n";
        std::fs::write(&artifact_path, script).unwrap();
        #[cfg(unix)]
        {
            let mut permissions = std::fs::metadata(&artifact_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&artifact_path, permissions).unwrap();
        }
        write_resident_child_manifest(&manifest_path, "resident-echo", "handle");
        write_sha256_sidecar(&artifact_path, script);
        let manifest_bytes = std::fs::read(&manifest_path).unwrap();
        write_sha256_sidecar(&manifest_path, &manifest_bytes);
    }

    fn write_resident_wit_child(children_dir: &Path) {
        let child_dir = children_dir.join("resident-wit");
        std::fs::create_dir_all(&child_dir).unwrap();
        let artifact_path = child_dir.join("resident-wit.wasm");
        let manifest_path = child_dir.join("child.toml");
        let component_wat = r#"
(component
  (core module $m
    (func $run (export "run") (result i32)
      i32.const 7))
  (core instance $i (instantiate $m))
  (func $run (result s32) (canon lift (core func $i "run")))
  (instance $control (export "run" (func $run)))
  (export "patina:demo/control@0.1.0" (instance $control)))
"#;
        let component = wat::parse_str(component_wat).unwrap();
        std::fs::write(&artifact_path, &component).unwrap();
        write_resident_child_manifest(&manifest_path, "resident-wit", "wit-only");
        write_sha256_sidecar(&artifact_path, &component);
        let manifest_bytes = std::fs::read(&manifest_path).unwrap();
        write_sha256_sidecar(&manifest_path, &manifest_bytes);
    }

    fn write_resident_child_manifest(manifest_path: &Path, name: &str, mode: &str) {
        std::fs::write(
            manifest_path,
            format!(
                r#"[child]
name = "{name}"
version = "0.1.0"
description = "resident test child"
kind = "child"
role = "app"

[child.ingress]
mode = "{mode}"

[child.artifact]
wasm = "{name}.wasm"

[child.contract]
allow = ["patina:demo/control@0.1.0.run"]

[needs]
toys = []

[relationships]
listens = []
"#
            ),
        )
        .unwrap();
    }

    fn write_sha256_sidecar(path: &Path, bytes: &[u8]) {
        use sha2::{Digest, Sha256};

        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(".sha256");
        std::fs::write(
            PathBuf::from(sidecar),
            format!("{:x}", Sha256::digest(bytes)),
        )
        .unwrap();
    }

    #[test]
    fn resident_status_source_reflects_closed_endpoint() {
        let endpoint = Arc::new(Mutex::new(MotherIrohEndpointSnapshot {
            endpoint_id: EndpointIdText::new("endpoint-resident-status")
                .expect("string ID literal/generated value must be non-empty"),
            lifecycle: mct_iroh::MotherIrohEndpointLifecycle::Bound,
            accepted_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            direct_addresses: Vec::new(),
            relay_urls: Vec::new(),
            relay_mode: MotherIrohRelayMode::Disabled,
        }));
        let source = ResidentStatusSource {
            endpoint: Arc::clone(&endpoint),
            accepted_connection_count: Arc::new(AtomicU64::new(3)),
            loaded_child_count: 2,
            approved_child_count: 1,
            binding_count: 4,
            ledger_path: PathBuf::from("/path/that/does/not/exist.jsonl"),
        };

        let live = source.status();
        assert_eq!(live.readiness, mct_daemon::MctDaemonReadiness::Ready);
        assert_eq!(live.resident.unwrap().accepted_connection_count, 3);

        endpoint.lock().unwrap().lifecycle = mct_iroh::MotherIrohEndpointLifecycle::Closed;
        let closed = source.status();
        assert_eq!(closed.readiness, mct_daemon::MctDaemonReadiness::NotReady);
        assert_eq!(closed.safe_message, "iroh endpoint not ready");
    }

    #[tokio::test]
    async fn control_snapshot_unopenable_state_projects_error_response() {
        let dir = tempfile::tempdir().unwrap();
        let source = ControlSnapshotSource::open(dir.path());

        let snapshot = control_snapshot(&source).await;
        let response = mct_daemon::handle_control_plane_path_result_with_auth(
            "GET",
            "/snapshot",
            snapshot.as_ref(),
            &mct_daemon::MctControlPlaneAuthPolicy::open_local(),
            None,
        );

        assert!(matches!(
            snapshot,
            Err(MctControlPlaneSnapshotError::RuntimeStateUnavailable { .. })
        ));
        assert_eq!(response.status_code, 503);
        assert!(response.body.contains("runtime state unavailable"));
        assert!(response.body.contains("not_ready"));
        assert!(!response.body.contains("\"ready\""));
    }

    #[test]
    fn authorize_cli_toy_denies_expired_grant_against_current_time() {
        let child = test_child();
        let authorized_child = test_authorized_child();
        let call = test_call();
        let toy_id =
            ToyId::new("toy-demo").expect("string ID literal/generated value must be non-empty");
        let contracts = vec![test_contract(&toy_id)];
        let grants = vec![expired_grant(&toy_id)];

        let result = authorize_cli_toy(CliToyAuthorizationRequest {
            child: &child,
            authorized_child: &authorized_child,
            call: &call,
            contracts: &contracts,
            grants: &grants,
            toy_id,
            action: "read",
            resource_id: Some("resource-a".into()),
            label: "expired",
        });

        let Err(error) = result else {
            panic!("expired grant must deny");
        };
        assert!(error.safe_message.contains("ExpiredGrant"));
        assert_eq!(error.observations[0].outcome, ObservationOutcome::Denied);
    }
}

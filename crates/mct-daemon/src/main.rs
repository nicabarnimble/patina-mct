use anyhow::{Context, Result, bail};
use mct_daemon::{
    MctChildIntegrityMode, MctChildLoadOptions, MctCompositionPlan, MctCompositionStep,
    MctConfigChildAuthorityProjection, MctControlPlaneSnapshot, MctDaemonConfigStore,
    MctLocalNodeIdentity, MctOperatorChildScope, MctOperatorNodeScope, MctPeerAddressBookEntry,
    MctProcessChildHarness, MctProcessChildInvocationIds, MctRuntimeStateStore,
    MctToyAdapterRegistry, MctToyBackend, MctWasiHostConfig, MctWasiPreopen, MctWasiPreopenAccess,
    MctWasmComponentInvocationIds, MctWasmComponentRuntime, MctWitHostImportAdapters,
    MctWitToyHostAdapter, build_federation_capability_view, build_metrics_snapshot, daemon_status,
    default_config_path, default_state_path, install_verified_child_package,
    load_children_from_dir, record_composition_plan, reload_configured_child,
    serve_http_control_once, sync_child_registry_source, warmup_configured_child,
};
use mct_iroh::{
    MctIrohCallHandlerResult, MctIrohServeState, MctIrohServedProtocol, MotherIrohEndpoint,
    MotherIrohEndpointConfig, MotherIrohEndpointTicket, MotherIrohRelayMode,
    load_or_create_node_secret_key_hex,
};
use mct_kernel::*;
use mct_observation::JsonlObservationLedger;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::net::TcpListener;

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
                TraceId::from(format!("trace-warmup:{child_name}")),
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
                TraceId::from(format!("trace-reload:{child_name}")),
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
    state.insert_run_started(
        &run_id,
        &call,
        RuntimeKind::Process,
        Some(&authorized),
        mct_daemon::current_timestamp_string(),
    )?;
    state.append_run_observations(&run_id, std::slice::from_ref(&authority_observation))?;

    let harness = MctProcessChildHarness {
        executable,
        args: Vec::new(),
        timeout: Duration::from_secs(5),
        local_node_id: MctNodeId::from("local-mct"),
    };
    let report = harness.invoke_authorized_child(
        &authorized,
        &call,
        &payload,
        MctProcessChildInvocationIds {
            started_observation_id: ObservationId::from(format!(
                "obs-cli-process-started:{}",
                call.call_id
            )),
            completed_observation_id: ObservationId::from(format!(
                "obs-cli-process-completed:{}",
                call.call_id
            )),
            result_ref: ResultRef::from(format!("result-cli-process:{}", call.call_id)),
            audit_ref: AuditRef::from(format!("audit-cli-process:{}", call.call_id)),
            started_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            completed_at: Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
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
    state.insert_run_started(
        &run_id,
        &call,
        RuntimeKind::WasmComponent,
        Some(&authorized),
        mct_daemon::current_timestamp_string(),
    )?;
    state.append_run_observations(&run_id, std::slice::from_ref(&authority_observation))?;

    let runtime = MctWasmComponentRuntime::new()?;
    let report = runtime.invoke_authorized_s32_export(
        &authorized,
        &call,
        component_path,
        &export_name,
        MctWasmComponentInvocationIds {
            started_observation_id: ObservationId::from(format!(
                "obs-cli-wasm-started:{}",
                call.call_id
            )),
            completed_observation_id: ObservationId::from(format!(
                "obs-cli-wasm-completed:{}",
                call.call_id
            )),
            audit_ref: AuditRef::from(format!("audit-cli-wasm:{}", call.call_id)),
            started_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            completed_at: Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
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
    state.insert_run_started(
        &run_id,
        &call,
        RuntimeKind::WasmComponent,
        Some(&authorized),
        mct_daemon::current_timestamp_string(),
    )?;
    state.append_run_observations(&run_id, std::slice::from_ref(&authority_observation))?;

    let import_component_path = child.wasm_path.clone();
    let imports = run_wit_runtime_on_blocking_thread(move || {
        let runtime = MctWasmComponentRuntime::new()?;
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

    let invoke_authorized = authorized.clone();
    let invoke_child = child.clone();
    let invoke_call = call.clone();
    let report = run_wit_runtime_on_blocking_thread(move || {
        let runtime = MctWasmComponentRuntime::new()?;
        Ok(
            runtime.invoke_authorized_child_wit_export_with_host_adapters(
                &invoke_authorized,
                &invoke_child,
                &invoke_call,
                &args_json,
                adapter_build.adapters,
                MctWasmComponentInvocationIds {
                    started_observation_id: ObservationId::from(format!(
                        "obs-cli-wasm-wit-started:{}",
                        invoke_call.call_id
                    )),
                    completed_observation_id: ObservationId::from(format!(
                        "obs-cli-wasm-wit-completed:{}",
                        invoke_call.call_id
                    )),
                    audit_ref: AuditRef::from(format!(
                        "audit-cli-wasm-wit:{}",
                        invoke_call.call_id
                    )),
                    started_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
                    completed_at: Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
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
                assignment_id: Some(request.authorized_child.assignment_id.clone()),
                caller_node_id: Some(request.call.caller.node_id.clone()),
            },
            child_instance_id: request.authorized_child.child_instance_id.clone(),
            action: request.action.into(),
            resource_id: request.resource_id,
            node_id: request.call.caller.node_id.clone(),
            now: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            ids: ToyGrantEvaluationIds {
                evaluation_id: ToyGrantEvaluationId::from(format!(
                    "toy-eval-cli-{}",
                    request.label
                )),
                decision_id: DecisionId::from(format!("decision-toy-cli-{}", request.label)),
                observation_id: ObservationId::from(format!("obs-toy-grant-cli-{}", request.label)),
                authorized_toy_call_id: AuthorizedToyCallId::from(format!(
                    "authorized-toy-cli-{}",
                    request.label
                )),
            },
        },
        request.contracts,
        request.grants,
    );
    let Some(authorized) = result.authorized else {
        let observation = toy_grant_evaluation_observation(
            request.call.trace_context.trace_id.clone(),
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
        observed_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
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
    ToyId::from("toy:slate:wasi-logging")
}

fn slate_measure_toy_id() -> ToyId {
    ToyId::from("toy:slate:patina-measure")
}

fn slate_git_toy_id() -> ToyId {
    ToyId::from("toy:slate:patina-git")
}

fn slate_filesystem_toy_id() -> ToyId {
    ToyId::from("toy:slate:wasi-filesystem-project")
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
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let http_addr = take_option(&mut args, "--http");
    let uds_path = take_option(&mut args, "--uds").map(PathBuf::from);
    if !args.is_empty() {
        bail!("unexpected serve arguments: {}", args.join(" "));
    }
    match (http_addr, uds_path) {
        (Some(addr), None) => serve_http_control_loop(&state_path, &addr).await,
        (None, Some(path)) => run_control_serve_uds_with_state(state_path, path).await,
        (None, None) => serve_http_control_loop(&state_path, "127.0.0.1:9173").await,
        (Some(_), Some(_)) => bail!("serve accepts only one control transport: --http or --uds"),
    }
}

async fn serve_http_control_loop(state_path: &Path, addr: &str) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    println!("mct daemon serving control http on {addr}");
    loop {
        serve_http_control_once(&listener, control_snapshot(state_path)?).await?;
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
    loop {
        mct_daemon::serve_uds_control_once(&listener, control_snapshot(&state_path)?).await?;
    }
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

fn control_snapshot(state_path: &Path) -> Result<MctControlPlaneSnapshot> {
    let state = MctRuntimeStateStore::open(state_path)?;
    let summary = state.summary().ok();
    let runs = state.list_runs(20).unwrap_or_default();
    Ok(MctControlPlaneSnapshot::new(
        daemon_status(None),
        summary,
        runs,
    ))
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
        MctNodeId::from("local-mct"),
        VisionId::from("vision-local"),
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
            vision_id: VisionId::from("vision-local"),
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
        call_id: CallId::from(parts[1]),
        runtime_kind: parse_runtime_kind(parts[2])?,
        child_name: parts
            .get(3)
            .filter(|value| !value.is_empty())
            .map(|value| (*value).to_owned()),
        authority_decision_id: parts
            .get(4)
            .filter(|value| !value.is_empty())
            .map(|value| DecisionId::from(*value)),
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
        admitted_by_observation_id: ObservationId::from(format!("obs:toy-catalog:{toy_id}")),
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
        grant_id: ToyGrantId::from(format!("grant:slate:{label}:{}", child.name)),
        toy_id,
        subject: ToyGrantSubject {
            child_name: child.name.clone(),
            artifact_id: child.artifact_id.clone(),
            artifact_version: child.version.clone(),
            assignment_id: Some(ChildAssignmentId::from(format!(
                "assignment:{}",
                child.name
            ))),
            caller_node_id: Some(MctNodeId::from("local-mct")),
        },
        scope: ToyGrantScope {
            vision_id: VisionId::from("vision-local"),
            node_id: Some(MctNodeId::from("local-mct")),
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
        authority_observation_id: ObservationId::from(format!(
            "obs:toy-grant:slate:{label}:{}",
            child.name
        )),
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
    let peer_node_id = MctNodeId::from(args.remove(0));
    let binding_id = PeerBindingId::from(args.remove(0));
    let endpoint_id = EndpointIdText::from(args.remove(0));
    let vision_id = VisionId::from(args.remove(0));
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
    let peer_node_id = MctNodeId::from(args.remove(0));
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
    let peer_node_id = MctNodeId::from(args.remove(0));
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
        call_id: CallId::from("call-cli-wasm"),
        caller: CallerIdentity {
            node_id: MctNodeId::from("local-mct"),
            user_id: None,
            vision_id: VisionId::from("vision-local"),
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
        deadline: Timestamp::new("2026-05-31T00:01:00Z").unwrap(),
        trace_context: TraceContext {
            trace_id: TraceId::from("trace-cli-wasm"),
            span_id: SpanId::from("span-cli-wasm"),
        },
        origin: CallOrigin::WasmHost,
    }
}

fn local_process_call(target: OperationTarget, payload_size_bytes: u64) -> MctCall {
    MctCall {
        call_id: CallId::from("call-cli-process"),
        caller: CallerIdentity {
            node_id: MctNodeId::from("local-mct"),
            user_id: None,
            vision_id: VisionId::from("vision-local"),
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
        deadline: Timestamp::new("2026-05-31T00:01:00Z").unwrap(),
        trace_context: TraceContext {
            trace_id: TraceId::from("trace-cli-process"),
            span_id: SpanId::from("span-cli-process"),
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

fn current_timestamp() -> Timestamp {
    Timestamp::new(jiff::Timestamp::now().to_string()).expect("jiff produced RFC3339 timestamp")
}

async fn serve_iroh(mut args: Vec<String>) -> Result<()> {
    let relay_default = take_flag(&mut args, "--relay-default");
    if args.len() < 5 {
        bail!(
            "expected: mct-daemon iroh serve [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> [children-dir]"
        );
    }
    let identity_path = PathBuf::from(&args[0]);
    let binding_id = PeerBindingId::from(args[1].as_str());
    let peer_endpoint_id = EndpointIdText::from(args[2].as_str());
    let peer_node_id = MctNodeId::from(args[3].as_str());
    let vision_id = VisionId::from(args[4].as_str());
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
    let mut state = MctIrohServeState::new();

    loop {
        match endpoint
            .serve_next(
                &mut state,
                std::slice::from_ref(&binding),
                current_timestamp(),
                Some(ResultRef::from("result-mct-peer-call")),
            )
            .await
        {
            Ok(MctIrohServedProtocol::Hello { evaluation, .. }) => {
                println!(
                    "hello outcome={:?} reason={:?} decision={}",
                    evaluation.hello_outcome, evaluation.reason, evaluation.decision_id
                );
            }
            Ok(MctIrohServedProtocol::Call {
                evaluation, reply, ..
            }) => {
                println!(
                    "call outcome={:?} reason={:?} reply={:?} decision={}",
                    evaluation.outcome,
                    evaluation.reason,
                    reply.reply_outcome,
                    evaluation.decision_id
                );
            }
            Err(error) => {
                eprintln!("iroh serve error: {error}");
                endpoint.close().await;
                return Err(error.into());
            }
        }
    }
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
    let binding_id = PeerBindingId::from(args[1].as_str());
    let peer_endpoint_id = EndpointIdText::from(args[2].as_str());
    let peer_node_id = MctNodeId::from(args[3].as_str());
    let vision_id = VisionId::from(args[4].as_str());
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
        local_node_id: MctNodeId::from("local-mct"),
    };
    let projection = load_configured_child_projection(&config_path, &children_dir)?;
    let mut state = MctIrohServeState::new();

    loop {
        let harness = harness.clone();
        let projection = projection.clone();
        let child_name = child_name.clone();
        let ledger_path = ledger_path.clone();
        let state_path = state_path.clone();
        match endpoint
            .serve_next_with_call_handler(
                &mut state,
                std::slice::from_ref(&binding),
                current_timestamp(),
                move |request, _evaluation| {
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
                    if let Err(error) = runtime_state.insert_run_started(
                        &run_id,
                        &request.call,
                        RuntimeKind::Process,
                        Some(&authorized),
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
                        &authorized,
                        &request.call,
                        "{}",
                        MctProcessChildInvocationIds {
                            started_observation_id: ObservationId::from(format!(
                                "obs-iroh-process-started:{}",
                                request.call.call_id
                            )),
                            completed_observation_id: ObservationId::from(format!(
                                "obs-iroh-process-completed:{}",
                                request.call.call_id
                            )),
                            result_ref: ResultRef::from(format!(
                                "result-iroh-process:{}",
                                request.call.call_id
                            )),
                            audit_ref: AuditRef::from(format!(
                                "audit-iroh-process:{}",
                                request.call.call_id
                            )),
                            started_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
                            completed_at: Timestamp::new("2026-05-31T00:00:01Z").unwrap(),
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
                        ResultOutcome::Success => {
                            MctIrohCallHandlerResult::completed(ResultRef::from(format!(
                                "result-iroh-process:{}",
                                request.call.call_id
                            )))
                        }
                        ResultOutcome::TimedOut => MctIrohCallHandlerResult::timed_out(),
                        ResultOutcome::Failed
                        | ResultOutcome::Denied
                        | ResultOutcome::Cancelled => {
                            MctIrohCallHandlerResult::failed(report.result.requester_message)
                        }
                    }
                },
            )
            .await
        {
            Ok(MctIrohServedProtocol::Hello { evaluation, .. }) => {
                println!(
                    "hello outcome={:?} reason={:?} decision={}",
                    evaluation.hello_outcome, evaluation.reason, evaluation.decision_id
                );
            }
            Ok(MctIrohServedProtocol::Call {
                evaluation, reply, ..
            }) => {
                println!(
                    "call outcome={:?} reason={:?} reply={:?} result_ref={:?} decision={}",
                    evaluation.outcome,
                    evaluation.reason,
                    reply.reply_outcome,
                    reply.result_ref,
                    evaluation.decision_id
                );
            }
            Err(error) => {
                eprintln!("iroh process serve error: {error}");
                endpoint.close().await;
                return Err(error.into());
            }
        }
    }
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
    let binding_id = PeerBindingId::from(args[2].as_str());
    let local_node_id = MctNodeId::from(args[3].as_str());
    let vision_id = VisionId::from(args[4].as_str());
    let target = OperationTarget {
        namespace: args.get(5).cloned().unwrap_or_else(|| "patina".into()),
        interface_name: args.get(6).cloned().unwrap_or_else(|| "echo".into()),
        function_name: args.get(7).cloned().unwrap_or_else(|| "echo".into()),
    };

    let secret_key_hex = load_or_create_node_secret_key_hex(&identity_path)?;
    let mut endpoint = MotherIrohEndpoint::bind(iroh_config(secret_key_hex, relay_default)).await?;
    let local_endpoint_id = endpoint.snapshot().endpoint_id;
    let peer_ticket = read_ticket(&peer_ticket_path)?;
    let trace_id = TraceId::from("trace-cli-iroh-call");
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
    let peer_node_id = MctNodeId::from(args.remove(0));
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
    let trace_id = TraceId::from("trace-cli-iroh-call-peer");
    let hello_request = cli_hello_request(
        &local_endpoint_id,
        &peer.binding_id,
        &MctNodeId::from("local-mct"),
        &peer.vision_id,
        &trace_id,
    );
    let hello_response = endpoint.send_hello(&peer_ticket, &hello_request).await?;
    println!("{}", serde_json::to_string_pretty(&hello_response)?);

    let call_request = cli_call_request(
        &local_endpoint_id,
        &peer.binding_id,
        &MctNodeId::from("local-mct"),
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
        received_observation_id: ObservationId::from("obs-cli-hello-received"),
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
        call_id: CallId::from("call-cli-iroh"),
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
        deadline: Timestamp::new("2026-05-31T00:01:00Z").unwrap(),
        trace_context: TraceContext {
            trace_id: trace_id.clone(),
            span_id: SpanId::from("span-cli-call"),
        },
        origin: CallOrigin::Iroh,
    };

    MctCallProtocolRequest {
        protocol_request_id: ProtocolRequestId::from("proto-cli-call"),
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
        payload: MctCallPayloadHandle {
            payload_kind: PayloadKind::Empty,
            content_type: None,
            approximate_size_bytes: 0,
            digest: None,
            blob_ref: None,
            external_ref: None,
            inline_payload_ref: None,
        },
        idempotency_key: Some("idem-cli-call".into()),
        received_observation_id: ObservationId::from("obs-cli-call-received"),
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
    let observation =
        child_call_authority_observation(call.trace_context.trace_id.clone(), &result.evaluation);
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
        node_id: MctNodeId::from("local-mct"),
        vision_id: VisionId::from("vision-local"),
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
        "mct-daemon {version}\n\nCommands:\n  status\n  serve [--http addr | --uds socket-path] [--state path]\n  control serve-http [addr] [--state path]\n  control serve-uds [socket-path] [--state path]\n  registry install <verified-package-dir> [--children-dir path] [--replace] [--json]\n  registry sync <source-id> [children-dir] [--state path] [--strict-integrity] [--json]\n  federation view [--config path] [--state path] [--json]\n  metrics snapshot [--state path] [--json]\n  pando record <composition-id> [step-id,call-id,runtime,child,decision ...] [--state path] [--json]\n  children load [children-dir] [--strict-integrity] [--json]\n  process call <executable> [payload-json] [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  children approve <child-name> [children-dir] [--config path] [--strict-integrity]\n  children revoke <child-name> [--config path]\n  children approvals [--config path] [--json]\n  children warmup <child-name> [--children-dir path] [--config path] [--ledger path] [--state path] [--json]\n  children reload <child-name> [--children-dir path] [--config path] [--ledger path] [--state path] [--json]\n  peers add <peer-node-id> <binding-id> <endpoint-id> <vision-id> [ticket-file] [--config path]\n  peers list [--config path] [--json]\n  peers revoke <peer-node-id> [--config path]\n  peers remove <peer-node-id> [--config path]\n  state summary [--state path] [--json]\n  runs list [--state path] [--json] [--limit n]\n  slate list-work --project-root path [--status status] [--kind kind] [--children-dir path] [--config path] [--state path] [--ledger path]\n  toys authorize-slate <child-name> <project-root> [--children-dir path] [--config path] [--state path] [--json]\n  wasm call <component-file> <export-name> [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  wasm call-wit <child-name> <operation-id> <args-json> [--project-root path] [--guest-project /project] [--git-repo path] [--children-dir path] [--config path] [--ledger path] [--state path]\n  iroh identity [identity-file] [--config path]\n  iroh serve [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> [children-dir]\n  iroh serve-process [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> <executable> --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  iroh call [--relay-default] <identity-file> <peer-ticket-file> <binding-id> <local-node-id> <vision-id> [namespace interface function]\n  iroh call-peer [--relay-default] <identity-file> <peer-node-id> [namespace interface function] [--config path]",
        version = mct_daemon::version()
    );
}

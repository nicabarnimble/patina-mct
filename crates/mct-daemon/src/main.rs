use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use mct_daemon::{
    ChildInvocationProvenance, DEFAULT_WASM_MEMORY_LIMIT_BYTES, MCT_BLOB_MAX_BYTES,
    MCT_SECRETS_TOY_ID, MctChildIntegrityMode, MctChildLoadOptions, MctCompositionPlan,
    MctCompositionStep, MctConfigChildAuthorityProjection, MctControlPlaneSnapshot,
    MctControlPlaneSnapshotError, MctControlPlaneSnapshotResult, MctDaemonConfigStore,
    MctDaemonStatus, MctLocalBlobStoreError, MctLocalNodeIdentity, MctOperatorChildScope,
    MctOperatorNodeScope, MctOutboundPeerBindingPresentation, MctPeerAddressBookEntry,
    MctProcessChildHarness, MctProcessChildInvocationIds, MctRemoteCallableSurfaceRecord,
    MctRemoteSurfaceRefresh, MctResidentStatus, MctRuntimeStateStore, MctToyAdapterRegistry,
    MctToyBackend, MctWasiHostConfig, MctWasiPreopen, MctWasiPreopenAccess,
    MctWasmComponentInvocationIds, MctWasmComponentRuntime, MctWasmHostConfig,
    MctWitHostImportAdapters, MctWitToyHostAdapter, build_federation_capability_view_with_children,
    build_metrics_snapshot, current_timestamp, daemon_status, daemon_status_with_resident,
    default_config_path, default_state_path, hello_capability_view_from_federation_view,
    install_verified_child_package, load_children_from_dir, local_blob_store_for_state_path,
    mct_secrets_toy_contract, outbound_peer_binding_for_local, record_composition_plan,
    reload_configured_child, serve_http_control_once_with_snapshot_result,
    sync_child_registry_source, warmup_configured_child,
};
use mct_iroh::{
    MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES, MctIrohCallHandlerResult, MctIrohConcurrentServeConfig,
    MctIrohServeEvent, MctIrohServeState, MctIrohServedProtocol,
    MctPeerBindingSignatureVerification, MotherIrohEndpoint, MotherIrohEndpointConfig,
    MotherIrohEndpointError, MotherIrohEndpointSnapshot, MotherIrohEndpointTicket,
    MotherIrohRelayMode, load_or_create_node_secret_key_hex, verify_peer_binding_signature_ref,
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
        "jvm" => run_jvm(args).await?,
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

fn resident_hello_capability_view(
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

fn local_hello_capability_view_from_config(
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

fn remote_surface_stale_at(received_at: &Timestamp) -> Result<Timestamp> {
    let received = received_at
        .as_str()
        .parse::<jiff::Timestamp>()
        .context("parse remote surface received_at")?;
    let stale = received
        .checked_add(jiff::SignedDuration::from_secs(300))
        .context("compute remote surface stale_at")?;
    Timestamp::new(stale.to_string()).context("encode remote surface stale_at")
}

fn refresh_remote_surfaces_from_admitted_hello_request(
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

fn refresh_remote_surfaces_from_admitted_hello_response(
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
    let state = MctRuntimeStateStore::open(&config.state_path)
        .with_context(|| format!("open runtime state {}", config.state_path.display()))?;
    let runtime_summary = state.summary()?;
    drop(state);
    let ledger = ResidentLedgerWriter::spawn(config.ledger_path.clone())?;

    let loaded_child_count = load_report.loaded;
    let resident_config = config_store.load()?;
    let hello_capability_view = resident_hello_capability_view(
        &resident_config,
        &runtime_summary,
        &identity,
        &load_report.children,
    );
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
    let event_state_path = config.state_path.clone();
    let event_task = tokio::spawn(async move {
        record_iroh_serve_events(
            event_rx,
            event_ledger,
            event_accepted_count,
            event_state_path,
        )
        .await
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
                require_binding_signature: true,
                capability_view: Some(hello_capability_view),
                ..MctIrohConcurrentServeConfig::default()
            },
            current_timestamp,
            move || {
                let config_path = config_path.clone();
                async move { load_peer_bindings_for_iroh(config_path).await }
            },
            move |request, _evaluation, inline_payload| {
                let execution_paths = execution_paths.clone();
                let execution_ledger = execution_ledger.clone();
                async move {
                    execute_resident_call(
                        execution_paths,
                        execution_ledger,
                        request,
                        ResidentRequestPayload::remote(inline_payload),
                    )
                    .await
                }
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
                        write.observations,
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
    state_path: PathBuf,
) {
    while let Some(event) = events.recv().await {
        let observations = match event {
            MctIrohServeEvent::AcceptedConnection => {
                accepted_connection_count.fetch_add(1, Ordering::SeqCst);
                Vec::new()
            }
            MctIrohServeEvent::Served(served) => {
                let served = *served;
                if let MctIrohServedProtocol::Hello {
                    request,
                    evaluation,
                    ..
                } = &served
                    && let Err(error) = refresh_remote_surfaces_from_admitted_hello_request(
                        &state_path,
                        request,
                        evaluation,
                        current_timestamp(),
                    )
                {
                    eprintln!("resident remote surface refresh failed: {error}");
                }
                resident_observations_for_served_protocol(served)
            }
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

#[derive(Clone, Debug)]
struct ResidentRequestPayload {
    inline_payload: Option<Vec<u8>>,
    allow_local_content_addressed_blob: bool,
}

impl ResidentRequestPayload {
    fn remote(inline_payload: Option<Vec<u8>>) -> Self {
        Self {
            inline_payload,
            allow_local_content_addressed_blob: false,
        }
    }

    #[cfg(test)]
    fn local(inline_payload: Option<Vec<u8>>) -> Self {
        Self {
            inline_payload,
            allow_local_content_addressed_blob: true,
        }
    }
}

#[derive(Debug)]
struct ResidentAuthorizedExecution {
    child: mct_daemon::MctLoadedChild,
    authorized_route: AuthorizedRouteExecution,
    route_taken: RouteTaken,
    child_authority_observation_id: ObservationId,
    route_observations: Vec<MctObservation>,
}

#[derive(Debug)]
struct ResidentChildExecution {
    child: mct_daemon::MctLoadedChild,
    authorized: AuthorizedChildInvocation,
    child_authority_observation_id: ObservationId,
    route_taken: RouteTaken,
    route_decision_id: DecisionId,
}

#[derive(Debug)]
enum ResidentAuthorizationOutcome {
    Authorized(Box<ResidentAuthorizedExecution>),
    Denied {
        route_decision_id: DecisionId,
        observations: Vec<MctObservation>,
    },
}

#[derive(Clone, Debug)]
struct ResidentExecutionReport {
    result: MctResult,
    observations: Vec<MctObservation>,
    inline_result_payload: Option<Vec<u8>>,
}

fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn inline_payload_content_type(handle: &MctCallPayloadHandle) -> Option<&str> {
    match handle {
        MctCallPayloadHandle::InlinePayload { content_type, .. }
        | MctCallPayloadHandle::ContentAddressedBlob { content_type, .. } => Some(content_type),
        MctCallPayloadHandle::ExternalReference { content_type, .. } => content_type.as_deref(),
        MctCallPayloadHandle::Empty => None,
    }
}

fn inline_result_payload_handle(
    reference: impl Into<String>,
    content_type: impl Into<String>,
    bytes: &[u8],
) -> MctCallPayloadHandle {
    MctCallPayloadHandle::InlinePayload {
        inline_payload_ref: reference.into(),
        content_type: content_type.into(),
        size_bytes: bytes.len() as u64,
        blake3_digest_hex: blake3_hex(bytes),
    }
}

fn resident_payload_fact_observation(
    call: &MctCall,
    direction: &str,
    bytes: &[u8],
    classification: &str,
) -> MctObservation {
    let digest = blake3_hex(bytes);
    MctObservation {
        observation_id: ObservationId::new(format!(
            "obs-resident-payload-{direction}:{}",
            call.call_id
        ))
        .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::AdapterEffectCompleted,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: None,
        subject_id: Some(direction.into()),
        resource_id: Some(format!(
            "payload:{direction}:size={}:digest={digest}:class={classification}",
            bytes.len()
        )),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome: ObservationOutcome::Completed,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: format!("{direction} payload integrity facts recorded"),
        detail_ref: None,
    }
}

fn observed_local_blob_payload(bytes: &[u8]) -> MctPayloadIntegrityObservation {
    MctPayloadIntegrityObservation {
        inline_bytes_present: true,
        content_addressed_blob_fetch_attempted: true,
        observed_size_bytes: Some(bytes.len() as u64),
        observed_blake3_digest_hex: Some(blake3_hex(bytes)),
    }
}

fn resident_payload_integrity_failure_observation(
    call: &MctCall,
    direction: &str,
    handle: &MctCallPayloadHandle,
    decision: &MctPayloadIntegrityDecision,
) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::new(format!(
            "obs-resident-payload-{direction}-failed:{}",
            call.call_id
        ))
        .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::AdapterEffectCompleted,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: None,
        subject_id: Some(direction.into()),
        resource_id: Some(format!(
            "payload:{direction}:size={}:digest={}:class={}:reason={:?}",
            handle.declared_size_bytes(),
            declared_payload_digest(handle).unwrap_or("none"),
            call.payload_metadata.data_classification,
            decision.reason
        )),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome: ObservationOutcome::Failed,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: decision.safe_message.clone(),
        detail_ref: None,
    }
}

fn declared_payload_digest(handle: &MctCallPayloadHandle) -> Option<&str> {
    match handle {
        MctCallPayloadHandle::InlinePayload {
            blake3_digest_hex, ..
        } => Some(blake3_digest_hex),
        MctCallPayloadHandle::ContentAddressedBlob { digest, .. } => Some(digest),
        MctCallPayloadHandle::ExternalReference { .. } | MctCallPayloadHandle::Empty => None,
    }
}

struct ResidentPayloadResolutionFailure {
    safe_message: String,
    observations: Vec<MctObservation>,
}

async fn resolve_resident_request_payload(
    paths: &ResidentExecutionPaths,
    request: &MctCallProtocolRequest,
    payload: ResidentRequestPayload,
) -> std::result::Result<Option<Vec<u8>>, ResidentPayloadResolutionFailure> {
    if !payload.allow_local_content_addressed_blob
        || !matches!(
            request.payload,
            MctCallPayloadHandle::ContentAddressedBlob { .. }
        )
    {
        return Ok(payload.inline_payload);
    }

    let state_path = paths.state_path.clone();
    let handle = request.payload.clone();
    let fetched = tokio::task::spawn_blocking(move || {
        local_blob_store_for_state_path(state_path).fetch(&handle)
    })
    .await
    .map_err(|error| {
        resident_payload_resolution_failure(
            &request.call,
            &request.payload,
            PayloadIntegrityReason::PayloadBlobUnavailable,
            format!("join local blob fetch: {error}"),
        )
    })?;

    let mut fetched_bytes = None;
    let observed = match fetched {
        Ok(bytes) => {
            let observed = observed_local_blob_payload(&bytes);
            fetched_bytes = Some(bytes);
            observed
        }
        Err(MctLocalBlobStoreError::PayloadBlobUnavailable) => {
            MctPayloadIntegrityObservation::missing_content_addressed_blob()
        }
        Err(MctLocalBlobStoreError::BlobTooLarge) => MctPayloadIntegrityObservation {
            inline_bytes_present: true,
            content_addressed_blob_fetch_attempted: true,
            observed_size_bytes: Some(MCT_BLOB_MAX_BYTES as u64 + 1),
            observed_blake3_digest_hex: declared_payload_digest(&request.payload)
                .map(str::to_owned),
        },
        Err(_) => {
            return Err(resident_payload_resolution_failure(
                &request.call,
                &request.payload,
                PayloadIntegrityReason::PayloadBlobUnavailable,
                "blob store unavailable".into(),
            ));
        }
    };

    let decision = evaluate_payload_integrity(
        PayloadIntegritySubject::Request,
        &request.payload,
        &observed,
        MCT_BLOB_MAX_BYTES as u64,
    );
    if decision.outcome != PayloadIntegrityOutcome::Matched {
        return Err(ResidentPayloadResolutionFailure {
            safe_message: decision.safe_message.clone(),
            observations: vec![resident_payload_integrity_failure_observation(
                &request.call,
                "request",
                &request.payload,
                &decision,
            )],
        });
    }

    Ok(fetched_bytes)
}

fn resident_payload_resolution_failure(
    call: &MctCall,
    handle: &MctCallPayloadHandle,
    reason: PayloadIntegrityReason,
    safe_message: String,
) -> ResidentPayloadResolutionFailure {
    let decision = MctPayloadIntegrityDecision {
        subject: PayloadIntegritySubject::Request,
        outcome: PayloadIntegrityOutcome::Mismatch,
        reason,
        safe_message: safe_message.clone(),
    };
    ResidentPayloadResolutionFailure {
        safe_message,
        observations: vec![resident_payload_integrity_failure_observation(
            call, "request", handle, &decision,
        )],
    }
}

async fn execute_resident_call(
    paths: ResidentExecutionPaths,
    ledger: ResidentLedgerWriter,
    request: MctCallProtocolRequest,
    payload: ResidentRequestPayload,
) -> MctIrohCallHandlerResult {
    let inline_payload = match resolve_resident_request_payload(&paths, &request, payload).await {
        Ok(inline_payload) => inline_payload,
        Err(report) => {
            if let Err(error) = ledger.append(report.observations).await {
                eprintln!("resident payload failure ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            return MctIrohCallHandlerResult::failed(report.safe_message);
        }
    };

    let authorization = match authorize_resident_child(paths.clone(), request.call.clone()).await {
        Ok(authorization) => authorization,
        Err(error) => {
            eprintln!("resident child authorization unavailable: {error}");
            return MctIrohCallHandlerResult::failed("runtime unavailable");
        }
    };

    let ResidentAuthorizationOutcome::Authorized(authorized) = authorization else {
        if let ResidentAuthorizationOutcome::Denied {
            route_decision_id,
            observations,
        } = authorization
        {
            if let Err(error) = ledger.append(observations).await {
                eprintln!("resident route denial ledger write failed: {error}");
                return MctIrohCallHandlerResult::failed("observation ledger unavailable");
            }
            return MctIrohCallHandlerResult::denied().with_route(Some(route_decision_id), None);
        }
        unreachable!("resident authorization outcome was already matched as non-authorized")
    };

    if let Err(error) = ledger.append(authorized.route_observations.clone()).await {
        eprintln!("resident route ledger write failed: {error}");
        return MctIrohCallHandlerResult::failed("observation ledger unavailable");
    }

    let current_revisions = match current_resident_route_revisions(&paths, &request.call) {
        Ok(revisions) => revisions,
        Err(error) => {
            eprintln!("resident route revision read failed: {error}");
            return MctIrohCallHandlerResult::failed("runtime unavailable");
        }
    };
    let execution = match tokio::task::spawn_blocking(move || {
        execute_authorized_resident_child(
            paths,
            *authorized,
            request,
            inline_payload,
            current_revisions,
        )
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

    result_to_call_handler_result(
        "result-resident",
        &execution.result,
        execution.inline_result_payload,
    )
}

async fn authorize_resident_child(
    paths: ResidentExecutionPaths,
    call: MctCall,
) -> Result<ResidentAuthorizationOutcome> {
    tokio::task::spawn_blocking(move || authorize_resident_child_blocking(&paths, &call))
        .await
        .context("join resident child authorization")?
}

struct ResidentCandidatePlan {
    child: mct_daemon::MctLoadedChild,
    candidate: CandidateRoute,
    authority: CandidateAuthorityEvaluation,
    child_authority: ChildCallAuthorityResult,
}

struct ResidentRemoteCandidatePlan {
    candidate: CandidateRoute,
    authority: CandidateAuthorityEvaluation,
}

fn authorize_resident_child_blocking(
    paths: &ResidentExecutionPaths,
    call: &MctCall,
) -> Result<ResidentAuthorizationOutcome> {
    let config = MctDaemonConfigStore::new(&paths.config_path).load()?;
    let state = MctRuntimeStateStore::open(&paths.state_path)?;
    let load_report = load_children_from_dir(MctChildLoadOptions::new(paths.children_dir.clone()));
    authorize_resident_child_from_loaded_with_state(
        &config,
        Some(&state),
        load_report.children,
        call,
        current_timestamp(),
    )
}

#[cfg(test)]
fn authorize_resident_child_from_loaded(
    config: &mct_daemon::MctDaemonConfig,
    children: Vec<mct_daemon::MctLoadedChild>,
    call: &MctCall,
) -> Result<ResidentAuthorizationOutcome> {
    authorize_resident_child_from_loaded_with_state(
        config,
        None,
        children,
        call,
        current_timestamp(),
    )
}

fn authorize_resident_child_from_loaded_with_state(
    config: &mct_daemon::MctDaemonConfig,
    state: Option<&MctRuntimeStateStore>,
    children: Vec<mct_daemon::MctLoadedChild>,
    call: &MctCall,
    now: Timestamp,
) -> Result<ResidentAuthorizationOutcome> {
    let scope = resident_child_scope(config);
    let projection = config.authority_projection_for_loaded_children(children.iter(), scope);
    let mut plans = Vec::new();

    for child in children
        .into_iter()
        .filter(|child| resident_child_accepts_call(child, call))
    {
        let child_authority = projection.authorize_child_for_call(&child.name, call);
        let candidate = resident_candidate_for_child(&projection, &child);
        let authority = if child_authority.is_allowed() {
            CandidateAuthorityEvaluation::admissible(
                candidate.clone(),
                child_authority.evaluation.policy_revision,
                call.authority_context.grants_revision,
            )
        } else {
            CandidateAuthorityEvaluation::eliminated(
                candidate.clone(),
                child_elimination_reason(child_authority.evaluation.reason_code),
                child_authority.evaluation.policy_revision,
                call.authority_context.grants_revision,
            )
        };
        plans.push(ResidentCandidatePlan {
            child,
            candidate,
            authority,
            child_authority,
        });
    }

    let remote_plans = resident_remote_candidate_plans(config, state, call, now)?;
    let mut observations = resident_candidate_observations(call, &plans);
    observations.extend(resident_remote_candidate_observations(call, &remote_plans));
    let mut authority_evaluations = plans
        .iter()
        .map(|plan| plan.authority.clone())
        .collect::<Vec<_>>();
    authority_evaluations.extend(remote_plans.iter().map(|plan| plan.authority.clone()));
    let mut admissible = plans
        .into_iter()
        .filter(|plan| plan.authority.outcome == CandidateAuthorityOutcome::Admissible)
        .collect::<Vec<_>>();

    if admissible.is_empty() {
        let no_route_reason = authority_evaluations
            .iter()
            .find_map(|evaluation| evaluation.reason)
            .unwrap_or(CandidateEliminationReason::ChildNotApproved);
        let decision = RouteDecision::no_route(
            call,
            authority_evaluations,
            no_route_reason,
            resident_route_decision_ids("initial", call),
        );
        observations.push(route_decision_observation(
            call.trace_context.trace_id.clone(),
            current_timestamp(),
            &decision,
        ));
        return Ok(ResidentAuthorizationOutcome::Denied {
            route_decision_id: decision.decision_id,
            observations,
        });
    }

    admissible.sort_by_key(|plan| resident_route_rank_key(&plan.candidate));
    let selected = admissible.remove(0);
    let initial = RouteDecision::selected(
        call,
        selected.candidate.clone(),
        authority_evaluations,
        resident_route_decision_ids("initial", call),
    );
    observations.push(child_call_authority_observation(
        call.trace_context.trace_id.clone(),
        current_timestamp(),
        &selected.child_authority.evaluation,
    ));
    observations.push(route_decision_observation(
        call.trace_context.trace_id.clone(),
        current_timestamp(),
        &initial,
    ));

    let revalidated_child = projection.authorize_child_for_call(&selected.child.name, call);
    let child_authority_observation_id = revalidated_child.evaluation.observation_id.clone();
    observations.push(child_call_authority_observation(
        call.trace_context.trace_id.clone(),
        current_timestamp(),
        &revalidated_child.evaluation,
    ));
    let revalidation = revalidate_route_for_execution(
        call,
        &initial,
        revalidated_child,
        Vec::new(),
        resident_route_revalidation_ids(call),
    );
    observations.push(route_decision_observation(
        call.trace_context.trace_id.clone(),
        current_timestamp(),
        &revalidation.decision,
    ));

    let Some(authorized_route) = revalidation.authorized else {
        return Ok(ResidentAuthorizationOutcome::Denied {
            route_decision_id: revalidation.decision.decision_id,
            observations,
        });
    };
    let route_taken = RouteTaken {
        node_id: selected.candidate.node_id.clone(),
        child_id: selected.candidate.child_id.clone(),
        runtime_kind: selected.candidate.runtime_kind,
    };
    Ok(ResidentAuthorizationOutcome::Authorized(Box::new(
        ResidentAuthorizedExecution {
            child: selected.child,
            authorized_route,
            route_taken,
            child_authority_observation_id,
            route_observations: observations,
        },
    )))
}

fn resident_child_scope(config: &mct_daemon::MctDaemonConfig) -> MctOperatorChildScope {
    config
        .local_identity
        .as_ref()
        .map(|identity| MctOperatorChildScope {
            vision_id: identity.vision_id.clone(),
            node_id: identity.node_id.clone(),
            project_id: None,
            policy_revision: identity.policy_revision,
        })
        .unwrap_or_default()
}

fn resident_candidate_for_child(
    projection: &MctConfigChildAuthorityProjection,
    child: &mct_daemon::MctLoadedChild,
) -> CandidateRoute {
    let child_id = ChildId::new(child.name.clone())
        .expect("string ID literal/generated value must be non-empty");
    CandidateRoute {
        candidate_id: format!("child:{}", child.name),
        node_id: projection.local_node_id.clone(),
        child_id: Some(child_id),
        runtime_kind: match child.ingress_mode {
            mct_daemon::MctChildIngressMode::Handle => RuntimeKind::Process,
            mct_daemon::MctChildIngressMode::Hybrid | mct_daemon::MctChildIngressMode::WitOnly => {
                RuntimeKind::WasmComponent
            }
        },
        network_path: NetworkPathClass::Local,
    }
}

fn resident_remote_candidate_plans(
    config: &mct_daemon::MctDaemonConfig,
    state: Option<&MctRuntimeStateStore>,
    call: &MctCall,
    now: Timestamp,
) -> Result<Vec<ResidentRemoteCandidatePlan>> {
    let Some(identity) = config.local_identity.as_ref() else {
        return Ok(Vec::new());
    };
    let Some(state) = state else {
        return Ok(Vec::new());
    };

    let operation_id = mct_daemon::operation_id_from_target(&call.target);
    let surfaces = state.fresh_remote_callable_surfaces_for_operation(
        &call.caller.vision_id,
        &operation_id,
        &now,
    )?;
    let mut plans = Vec::new();
    for surface in surfaces {
        let Some(peer) = config.peers.get(surface.peer_node_id.as_str()) else {
            continue;
        };
        let candidate = resident_candidate_for_remote_surface(peer, &surface);
        let authority = resident_remote_candidate_authority(
            identity,
            peer,
            &surface,
            candidate.clone(),
            call,
            &now,
        )?;
        plans.push(ResidentRemoteCandidatePlan {
            candidate,
            authority,
        });
    }
    Ok(plans)
}

fn resident_candidate_for_remote_surface(
    peer: &mct_daemon::MctPeerAddressBookEntry,
    surface: &MctRemoteCallableSurfaceRecord,
) -> CandidateRoute {
    CandidateRoute {
        candidate_id: format!(
            "peer:{}:{}:{}:{}",
            surface.peer_node_id, surface.binding_id, surface.operation_id, surface.child_name
        ),
        node_id: peer.peer_node_id.clone(),
        child_id: Some(
            ChildId::new(surface.child_name.clone())
                .expect("string ID literal/generated value must be non-empty"),
        ),
        runtime_kind: RuntimeKind::RemotePeer,
        network_path: resident_peer_network_path(peer),
    }
}

fn resident_peer_network_path(peer: &mct_daemon::MctPeerAddressBookEntry) -> NetworkPathClass {
    let Some(ticket) = peer.ticket.as_ref() else {
        return NetworkPathClass::Unknown;
    };
    if !ticket.direct_addresses.is_empty() {
        NetworkPathClass::Direct
    } else if !ticket.relay_urls.is_empty() {
        NetworkPathClass::Relayed
    } else {
        NetworkPathClass::Unknown
    }
}

fn resident_remote_candidate_authority(
    identity: &MctLocalNodeIdentity,
    peer: &mct_daemon::MctPeerAddressBookEntry,
    surface: &MctRemoteCallableSurfaceRecord,
    candidate: CandidateRoute,
    call: &MctCall,
    now: &Timestamp,
) -> Result<CandidateAuthorityEvaluation> {
    let local_binding = peer.to_peer_binding(identity)?;
    let outbound_binding = peer
        .outbound_binding
        .as_ref()
        .map(|outbound| outbound_peer_binding_for_local(identity, peer, outbound))
        .transpose()?;
    let operation_id = mct_daemon::operation_id_from_target(&call.target);
    let reason = match verify_peer_binding_signature_ref(
        peer.binding_signature_ref.as_deref(),
        &local_binding,
        &identity.endpoint_id,
    ) {
        MctPeerBindingSignatureVerification::Valid => None,
        MctPeerBindingSignatureVerification::Missing
        | MctPeerBindingSignatureVerification::Malformed
        | MctPeerBindingSignatureVerification::Invalid => {
            Some(CandidateEliminationReason::PeerNotAdmitted)
        }
    }
    .or_else(|| {
        let Some(outbound_binding) = outbound_binding.as_ref() else {
            return Some(CandidateEliminationReason::PeerNotAdmitted);
        };
        match verify_peer_binding_signature_ref(
            peer.outbound_binding
                .as_ref()
                .map(|outbound| outbound.signature_ref.as_str()),
            outbound_binding,
            &peer.endpoint_id,
        ) {
            MctPeerBindingSignatureVerification::Valid => None,
            MctPeerBindingSignatureVerification::Missing
            | MctPeerBindingSignatureVerification::Malformed
            | MctPeerBindingSignatureVerification::Invalid => {
                Some(CandidateEliminationReason::PeerNotAdmitted)
            }
        }
    })
    .or_else(|| {
        peer.outbound_binding
            .as_ref()
            .and_then(|outbound| outbound.expires_at.as_ref())
            .and_then(|expires_at| match timestamp_not_after(expires_at, now) {
                Ok(true) => Some(CandidateEliminationReason::PeerNotAdmitted),
                Ok(false) => None,
                Err(_) => Some(CandidateEliminationReason::PeerNotAdmitted),
            })
    })
    .or_else(|| {
        (peer.binding_state != BindingState::Admitted)
            .then_some(CandidateEliminationReason::PeerNotAdmitted)
    })
    .or_else(|| {
        (surface.binding_id != peer.binding_id || surface.endpoint_id != peer.endpoint_id)
            .then_some(CandidateEliminationReason::PeerNotAdmitted)
    })
    .or_else(|| {
        (!local_binding
            .scope
            .allowed_alpns
            .iter()
            .any(|alpn| alpn == MCT_CALL_ALPN)
            || !outbound_binding.as_ref().is_some_and(|binding| {
                binding
                    .scope
                    .allowed_alpns
                    .iter()
                    .any(|alpn| alpn == MCT_CALL_ALPN)
            }))
        .then_some(CandidateEliminationReason::PeerNotAdmitted)
    })
    .or_else(|| {
        (peer.vision_id != call.caller.vision_id || surface.vision_id != call.caller.vision_id)
            .then_some(CandidateEliminationReason::VisionPolicyDenied)
    })
    .or_else(|| {
        (peer.policy_revision != call.authority_context.policy_revision)
            .then_some(CandidateEliminationReason::PolicyRevisionStale)
    })
    .or_else(|| {
        call.payload_metadata
            .contains_secret_scoped_material
            .then_some(CandidateEliminationReason::SecretScopeForbidden)
    })
    .or_else(|| {
        (surface.operation_id != operation_id || surface.visibility != "vision_scoped")
            .then_some(CandidateEliminationReason::CapabilityUnavailable)
    })
    .or_else(|| {
        peer.ticket
            .is_none()
            .then_some(CandidateEliminationReason::CapabilityUnavailable)
    });

    Ok(match reason {
        Some(reason) => CandidateAuthorityEvaluation::eliminated(
            candidate,
            reason,
            peer.policy_revision,
            call.authority_context.grants_revision,
        ),
        None => CandidateAuthorityEvaluation::admissible(
            candidate,
            peer.policy_revision,
            call.authority_context.grants_revision,
        ),
    })
}

fn timestamp_not_after(timestamp: &Timestamp, now: &Timestamp) -> Result<bool> {
    let timestamp = timestamp
        .as_str()
        .parse::<jiff::Timestamp>()
        .context("parse timestamp")?;
    let now = now
        .as_str()
        .parse::<jiff::Timestamp>()
        .context("parse current timestamp")?;
    Ok(timestamp <= now)
}

fn child_elimination_reason(reason: ChildCallReasonCode) -> CandidateEliminationReason {
    match reason {
        ChildCallReasonCode::ReadyAuthorizedInstance => CandidateEliminationReason::RouteMismatch,
        ChildCallReasonCode::InstanceNotReady => CandidateEliminationReason::CapabilityUnavailable,
        ChildCallReasonCode::StalePolicy => CandidateEliminationReason::PolicyRevisionStale,
        ChildCallReasonCode::OperationNotExported
        | ChildCallReasonCode::UnknownInstance
        | ChildCallReasonCode::MissingAssignment
        | ChildCallReasonCode::AssignmentRevoked
        | ChildCallReasonCode::MissingApproval
        | ChildCallReasonCode::ApprovalNotApproved
        | ChildCallReasonCode::ApprovalScopeMismatch
        | ChildCallReasonCode::ArtifactMissing
        | ChildCallReasonCode::ArtifactRejected
        | ChildCallReasonCode::WrongNode
        | ChildCallReasonCode::WrongProject
        | ChildCallReasonCode::VersionMismatch => CandidateEliminationReason::ChildNotApproved,
    }
}

fn resident_candidate_observations(
    call: &MctCall,
    plans: &[ResidentCandidatePlan],
) -> Vec<MctObservation> {
    let mut observations = Vec::new();
    for plan in plans {
        observations.push(candidate_considered_observation(
            call.trace_context.trace_id.clone(),
            current_timestamp(),
            call,
            &plan.candidate,
            ObservationId::new(format!(
                "obs-route-candidate-considered:{}:{}",
                call.call_id, plan.candidate.candidate_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            plan.authority.policy_revision,
            plan.authority.grants_revision,
        ));
        if plan.authority.outcome == CandidateAuthorityOutcome::Eliminated {
            observations.push(candidate_eliminated_observation(
                call.trace_context.trace_id.clone(),
                current_timestamp(),
                call,
                &plan.authority,
                ObservationId::new(format!(
                    "obs-route-candidate-eliminated:{}:{}",
                    call.call_id, plan.candidate.candidate_id
                ))
                .expect("string ID literal/generated value must be non-empty"),
            ));
            observations.push(child_call_authority_observation(
                call.trace_context.trace_id.clone(),
                current_timestamp(),
                &plan.child_authority.evaluation,
            ));
        }
    }
    observations
}

fn resident_remote_candidate_observations(
    call: &MctCall,
    plans: &[ResidentRemoteCandidatePlan],
) -> Vec<MctObservation> {
    let mut observations = Vec::new();
    for plan in plans {
        observations.push(candidate_considered_observation(
            call.trace_context.trace_id.clone(),
            current_timestamp(),
            call,
            &plan.candidate,
            ObservationId::new(format!(
                "obs-route-candidate-considered:{}:{}",
                call.call_id, plan.candidate.candidate_id
            ))
            .expect("string ID literal/generated value must be non-empty"),
            plan.authority.policy_revision,
            plan.authority.grants_revision,
        ));
        if plan.authority.outcome == CandidateAuthorityOutcome::Eliminated {
            observations.push(candidate_eliminated_observation(
                call.trace_context.trace_id.clone(),
                current_timestamp(),
                call,
                &plan.authority,
                ObservationId::new(format!(
                    "obs-route-candidate-eliminated:{}:{}",
                    call.call_id, plan.candidate.candidate_id
                ))
                .expect("string ID literal/generated value must be non-empty"),
            ));
        }
    }
    observations
}

fn resident_route_rank_key(candidate: &CandidateRoute) -> (u8, u8, String, String) {
    let network = match candidate.network_path {
        NetworkPathClass::Local => 0,
        NetworkPathClass::Direct => 1,
        NetworkPathClass::Relayed => 2,
        NetworkPathClass::Unknown => 3,
    };
    let runtime = match candidate.runtime_kind {
        RuntimeKind::WasmComponent => 0,
        RuntimeKind::Process => 1,
        RuntimeKind::JvmChild => 2,
        RuntimeKind::RemotePeer => 3,
        RuntimeKind::Internal => 4,
    };
    let child_id = candidate
        .child_id
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    (network, runtime, child_id, candidate.candidate_id.clone())
}

fn resident_route_decision_ids(kind: &str, call: &MctCall) -> RouteDecisionIds {
    RouteDecisionIds {
        decision_id: DecisionId::new(format!("route-{kind}:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
        observation_id: ObservationId::new(format!("obs-route-{kind}:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
    }
}

fn resident_route_revalidation_ids(call: &MctCall) -> RouteRevalidationIds {
    RouteRevalidationIds {
        decision_id: DecisionId::new(format!("route-revalidation:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
        observation_id: ObservationId::new(format!("obs-route-revalidation:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
        authorized_route_execution_id: AuthorizedRouteExecutionId::new(format!(
            "authorized-route:{}",
            call.call_id
        ))
        .expect("string ID literal/generated value must be non-empty"),
    }
}

fn current_resident_route_revisions(
    paths: &ResidentExecutionPaths,
    call: &MctCall,
) -> Result<AuthorityContextSnapshot> {
    let config = MctDaemonConfigStore::new(&paths.config_path).load()?;
    let scope = resident_child_scope(&config);
    Ok(AuthorityContextSnapshot {
        policy_revision: scope.policy_revision,
        grants_revision: call.authority_context.grants_revision,
        vision_policy_revision: call.authority_context.vision_policy_revision,
    })
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
    request: MctCallProtocolRequest,
    inline_payload: Option<Vec<u8>>,
    current_revisions: AuthorityContextSnapshot,
) -> Result<ResidentExecutionReport> {
    let call = request.call.clone();
    let state = MctRuntimeStateStore::open(&paths.state_path)?;
    let runtime_kind = execution.route_taken.runtime_kind;
    let run_id = run_id_for_call("resident", &call);

    if execution.authorized_route.policy_revision() != current_revisions.policy_revision {
        let report = resident_route_revision_denial_report(
            &call,
            execution.authorized_route.route(),
            execution
                .authorized_route
                .revalidation_decision_id()
                .clone(),
            CandidateEliminationReason::PolicyRevisionStale,
            &current_revisions,
            execution.authorized_route.policy_revision(),
            execution.authorized_route.grants_revision(),
        );
        return Ok(report);
    }
    if execution.authorized_route.grants_revision() != current_revisions.grants_revision {
        let report = resident_route_revision_denial_report(
            &call,
            execution.authorized_route.route(),
            execution
                .authorized_route
                .revalidation_decision_id()
                .clone(),
            CandidateEliminationReason::GrantsRevisionStale,
            &current_revisions,
            execution.authorized_route.policy_revision(),
            execution.authorized_route.grants_revision(),
        );
        return Ok(report);
    }

    let route_decision_id = execution
        .authorized_route
        .revalidation_decision_id()
        .clone();
    let route_taken = execution.route_taken.clone();
    let child_invocation = execution.authorized_route.into_child_invocation();
    let child_execution = ResidentChildExecution {
        child: execution.child,
        authorized: child_invocation,
        child_authority_observation_id: execution.child_authority_observation_id,
        route_taken,
        route_decision_id,
    };
    let provenance = ChildInvocationProvenance::from_authorized(
        &child_execution.authorized,
        child_execution.child_authority_observation_id.clone(),
    );
    state.insert_run_started(
        &run_id,
        &call,
        runtime_kind,
        Some(&provenance),
        mct_daemon::current_timestamp_string(),
    )?;

    let mut report = match child_execution.child.ingress_mode {
        mct_daemon::MctChildIngressMode::Handle => {
            execute_resident_process_child(child_execution, &request, inline_payload.as_deref())?
        }
        mct_daemon::MctChildIngressMode::Hybrid | mct_daemon::MctChildIngressMode::WitOnly => {
            execute_resident_wit_child(child_execution, &request, inline_payload.as_deref())?
        }
    };
    if let Some(bytes) = inline_payload.as_deref() {
        report.observations.push(resident_payload_fact_observation(
            &call,
            "request",
            bytes,
            &call.payload_metadata.data_classification,
        ));
    }
    if let Some(bytes) = report.inline_result_payload.as_deref() {
        report.observations.push(resident_payload_fact_observation(
            &call,
            "result",
            bytes,
            &call.payload_metadata.data_classification,
        ));
    }
    state.append_run_observations(&run_id, &report.observations)?;
    state.complete_run(
        &run_id,
        &report.result,
        mct_daemon::current_timestamp_string(),
    )?;
    Ok(report)
}

fn execute_resident_process_child(
    execution: ResidentChildExecution,
    request: &MctCallProtocolRequest,
    inline_payload: Option<&[u8]>,
) -> Result<ResidentExecutionReport> {
    let call = &request.call;
    let harness = MctProcessChildHarness {
        executable: execution.child.wasm_path.clone(),
        args: Vec::new(),
        timeout: Duration::from_secs(5),
        local_node_id: MctNodeId::new("local-mct")
            .expect("string ID literal/generated value must be non-empty"),
    };
    let payload_bytes = inline_payload.unwrap_or_default();
    let report = harness.invoke_authorized_child_bytes(
        execution.authorized,
        call,
        payload_bytes,
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
    let result_bytes = report.stdout.as_bytes().to_vec();
    let mut result = report.result;
    result.authority_decision_ref = execution.route_decision_id;
    result.route_taken = route_taken_for_outcome(result.outcome, execution.route_taken);
    let inline_result_payload = apply_inline_result_payload(
        &mut result,
        format!("result-resident-process:{}", call.call_id),
        "text/plain",
        result_bytes,
    );
    Ok(ResidentExecutionReport {
        result,
        observations: report.observations,
        inline_result_payload,
    })
}

fn execute_resident_wit_child(
    execution: ResidentChildExecution,
    request: &MctCallProtocolRequest,
    inline_payload: Option<&[u8]>,
) -> Result<ResidentExecutionReport> {
    let call = &request.call;
    let content_type = inline_payload_content_type(&request.payload).unwrap_or("application/json");
    if content_type != "application/json" {
        return Ok(resident_delivery_failure_report(
            call,
            execution.route_decision_id,
            execution.route_taken,
            CallProtocolReason::ChildPayloadContentTypeUnsupported,
            "unsupported child payload",
        ));
    }
    let args_json = match inline_payload {
        Some(bytes) => serde_json::from_slice::<serde_json::Value>(bytes)?,
        None => serde_json::json!([]),
    };
    let runtime = MctWasmComponentRuntime::new(default_wasm_host_config())?;
    let report = runtime.invoke_authorized_child_wit_export_with_host_adapters(
        execution.authorized,
        &execution.child,
        call,
        &args_json,
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
    let result_bytes = serde_json::to_vec(&report.output_json)?;
    let mut result = report.result;
    result.authority_decision_ref = execution.route_decision_id;
    result.route_taken = route_taken_for_outcome(result.outcome, execution.route_taken);
    let inline_result_payload = apply_inline_result_payload(
        &mut result,
        format!("result-resident-wit:{}", call.call_id),
        "application/json",
        result_bytes,
    );
    Ok(ResidentExecutionReport {
        result,
        observations: report.observations,
        inline_result_payload,
    })
}

fn apply_inline_result_payload(
    result: &mut MctResult,
    reference: impl Into<String>,
    content_type: impl Into<String>,
    bytes: Vec<u8>,
) -> Option<Vec<u8>> {
    result.execution_summary.output_size_bytes = Some(bytes.len() as u64);
    if bytes.len() > MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES {
        result.outcome = ResultOutcome::Failed;
        result.result_payload = MctCallPayloadHandle::Empty;
        result.requester_message = "result payload too large".into();
        return None;
    }
    result.result_payload = inline_result_payload_handle(reference, content_type, &bytes);
    Some(bytes)
}

fn resident_route_revision_denial_report(
    call: &MctCall,
    route: &CandidateRoute,
    decision_id: DecisionId,
    reason: CandidateEliminationReason,
    current: &AuthorityContextSnapshot,
    minted_policy_revision: u64,
    minted_grants_revision: u64,
) -> ResidentExecutionReport {
    let observation = MctObservation {
        observation_id: ObservationId::new(format!("obs-route-revision-denied:{}", call.call_id))
            .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::NoRouteRecorded,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: Some(decision_id.clone()),
        subject_id: route.child_id.as_ref().map(ToString::to_string),
        resource_id: Some(route.candidate_id.clone()),
        policy_revision: Some(current.policy_revision),
        grants_revision: Some(current.grants_revision),
        outcome: ObservationOutcome::Denied,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: "not authorized".into(),
        detail_ref: Some(format!(
            "elimination_reason:{reason:?};denial_class:{};minted_policy_revision={minted_policy_revision};current_policy_revision={};minted_grants_revision={minted_grants_revision};current_grants_revision={}",
            reason.denial_class().as_str(),
            current.policy_revision,
            current.grants_revision
        )),
    };
    ResidentExecutionReport {
        result: MctResult {
            call_id: call.call_id.clone(),
            outcome: ResultOutcome::Denied,
            route_taken: None,
            authority_decision_ref: decision_id,
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: call.payload_metadata.size_bytes,
                output_size_bytes: None,
            },
            result_payload: MctCallPayloadHandle::Empty,
            requester_message: "not authorized".into(),
            audit_ref: AuditRef::new(format!("audit-route-revision-denied:{}", call.call_id))
                .expect("string ID literal/generated value must be non-empty"),
        },
        observations: vec![observation],
        inline_result_payload: None,
    }
}

fn route_taken_for_outcome(outcome: ResultOutcome, route_taken: RouteTaken) -> Option<RouteTaken> {
    match outcome {
        ResultOutcome::Success | ResultOutcome::Failed | ResultOutcome::TimedOut => {
            Some(route_taken)
        }
        ResultOutcome::Denied | ResultOutcome::Cancelled => None,
    }
}

fn resident_delivery_failure_report(
    call: &MctCall,
    authority_decision_ref: DecisionId,
    route_taken: RouteTaken,
    reason: CallProtocolReason,
    safe_message: &str,
) -> ResidentExecutionReport {
    let observation = MctObservation {
        observation_id: ObservationId::new(format!(
            "obs-resident-delivery-failed:{}",
            call.call_id
        ))
        .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::RuntimeExecutionFailed,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: call.trace_context.trace_id.clone(),
            span_id: Some(call.trace_context.span_id.clone()),
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: Some(call.call_id.clone()),
        decision_id: Some(authority_decision_ref.clone()),
        subject_id: None,
        resource_id: Some(format!("{:?}", reason)),
        policy_revision: Some(call.authority_context.policy_revision),
        grants_revision: Some(call.authority_context.grants_revision),
        outcome: ObservationOutcome::Failed,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: None,
    };
    ResidentExecutionReport {
        result: MctResult {
            call_id: call.call_id.clone(),
            outcome: ResultOutcome::Failed,
            route_taken: Some(route_taken),
            authority_decision_ref,
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: call.payload_metadata.size_bytes,
                output_size_bytes: None,
            },
            result_payload: MctCallPayloadHandle::Empty,
            requester_message: safe_message.into(),
            audit_ref: AuditRef::new(format!("audit-resident-delivery-failed:{}", call.call_id))
                .expect("string ID literal/generated value must be non-empty"),
        },
        observations: vec![observation],
        inline_result_payload: None,
    }
}

fn result_to_call_handler_result(
    prefix: &str,
    result: &MctResult,
    inline_result_payload: Option<Vec<u8>>,
) -> MctIrohCallHandlerResult {
    let route_decision_id = Some(result.authority_decision_ref.clone());
    let route_taken = result.route_taken.clone();
    match result.outcome {
        ResultOutcome::Success => {
            let result_ref = ResultRef::new(format!("{prefix}:{}", result.call_id))
                .expect("string ID literal/generated value must be non-empty");
            if let Some(bytes) = inline_result_payload {
                MctIrohCallHandlerResult::completed_with_inline_payload(
                    result_ref,
                    result.result_payload.clone(),
                    bytes,
                )
            } else {
                MctIrohCallHandlerResult::completed(result_ref)
            }
            .with_route(route_decision_id, route_taken)
        }
        ResultOutcome::TimedOut => {
            MctIrohCallHandlerResult::timed_out().with_route(route_decision_id, route_taken)
        }
        ResultOutcome::Denied => {
            MctIrohCallHandlerResult::denied().with_route(route_decision_id, None)
        }
        ResultOutcome::Failed => MctIrohCallHandlerResult::failed(result.requester_message.clone())
            .with_route(route_decision_id, route_taken),
        ResultOutcome::Cancelled => {
            MctIrohCallHandlerResult::failed(result.requester_message.clone())
                .with_route(route_decision_id, None)
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
        mct_daemon::serve_uds_control_once_with_snapshot_result_and_blob_store(
            &listener,
            control_snapshot(&snapshot_source).await,
            Some(&state_path),
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
            result = mct_daemon::serve_uds_control_once_with_snapshot_result_and_blob_store(
                &listener,
                control_snapshot(&snapshot_source).await,
                Some(&state_path),
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
        bail!("expected toys subcommand: authorize-slate | authorize-secret");
    }
    match args.remove(0).as_str() {
        "authorize-slate" => run_toys_authorize_slate(args),
        "authorize-secret" => run_toys_authorize_secret(args),
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

fn run_toys_authorize_secret(mut args: Vec<String>) -> Result<()> {
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

fn secret_toy_grant_for_child(child: &mct_daemon::MctLoadedChild, secret_name: &str) -> ToyGrant {
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

fn run_peers_add(mut args: Vec<String>) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let binding_signature_ref = take_option(&mut args, "--signature-ref");
    if args.len() < 4 {
        bail!(
            "expected: mct-daemon peers add <peer-node-id> <binding-id> <endpoint-id> <vision-id> [ticket-file] [--signature-ref proof] [--config path]"
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
        binding_signature_ref,
        outbound_binding: None,
        binding_state: BindingState::Admitted,
        policy_revision: 1,
        updated_at: mct_daemon::current_timestamp_string(),
    })?;
    println!(
        "peer added={} config={} peers={} signature_ref={}",
        peer_node_id,
        config_path.display(),
        config.peers.len(),
        config
            .peers
            .get(peer_node_id.as_str())
            .and_then(|peer| peer.binding_signature_ref.as_ref())
            .is_some()
    );
    Ok(())
}

fn run_peers_set_outbound_proof(mut args: Vec<String>) -> Result<()> {
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let signature_ref = take_option(&mut args, "--signature-ref").ok_or_else(|| {
        anyhow::anyhow!("peers set-outbound-proof requires --signature-ref proof")
    })?;
    let expires_at = take_option(&mut args, "--expires-at")
        .map(Timestamp::new)
        .transpose()
        .context("parse --expires-at timestamp")?;
    if args.len() < 2 {
        bail!(
            "expected: mct-daemon peers set-outbound-proof <peer-node-id> <binding-id> --signature-ref proof [--expires-at ts] [--config path]"
        );
    }
    let peer_node_id = MctNodeId::new(args.remove(0))
        .expect("string ID literal/generated value must be non-empty");
    let binding_id = PeerBindingId::new(args.remove(0))
        .expect("string ID literal/generated value must be non-empty");
    let config = MctDaemonConfigStore::new(&config_path).set_peer_outbound_proof(
        &peer_node_id,
        MctOutboundPeerBindingPresentation {
            binding_id,
            policy_revision: 1,
            signature_ref,
            expires_at,
        },
    )?;
    let peer = config
        .peers
        .get(peer_node_id.as_str())
        .expect("updated peer remains in config");
    println!(
        "peer outbound proof set={} binding={} config={} expires_at={}",
        peer_node_id,
        peer.outbound_binding
            .as_ref()
            .map(|proof| proof.binding_id.to_string())
            .unwrap_or_else(|| "-".into()),
        config_path.display(),
        peer.outbound_binding
            .as_ref()
            .and_then(|proof| proof.expires_at.as_ref())
            .map(ToString::to_string)
            .unwrap_or_else(|| "-".into())
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
            size_bytes: 0,
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
            size_bytes: payload_size_bytes,
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

async fn run_jvm(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected jvm subcommand: call-json");
    }
    match args.remove(0).as_str() {
        "call-json" => run_jvm_call_json(args).await,
        other => bail!("unknown jvm subcommand '{other}'"),
    }
}

async fn run_jvm_call_json(mut args: Vec<String>) -> Result<()> {
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let config_path = take_option(&mut args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    if args.len() < 2 {
        bail!(
            "expected: mct-daemon jvm call-json <operation-id> <args-json> [--children-dir path] [--config path] [--state path] [--ledger path]"
        );
    }
    let operation_id = args.remove(0);
    let args_json = args.remove(0);
    let (request, payload) = jvm_bridge_protocol_request(&operation_id, &args_json)?;
    let ledger = ResidentLedgerWriter::spawn(ledger_path.clone())?;
    let result = execute_resident_call(
        ResidentExecutionPaths {
            config_path,
            children_dir,
            state_path,
        },
        ledger.clone(),
        request,
        ResidentRequestPayload::remote(Some(payload)),
    )
    .await;
    ledger.close().await;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "outcome": result.outcome,
            "safe_message": result.safe_message,
            "result_ref": result.result_ref,
            "route_decision_id": result.route_decision_id,
            "route_taken": result.route_taken,
            "result_payload": result.result_payload,
            "inline_result_payload_base64": result.inline_result_payload.map(|bytes| BASE64_STANDARD.encode(bytes)),
        }))?
    );
    Ok(())
}

fn jvm_bridge_protocol_request(
    operation_id: &str,
    args_json: &str,
) -> Result<(MctCallProtocolRequest, Vec<u8>)> {
    let payload_value: serde_json::Value = serde_json::from_str(args_json)
        .context("parse JVM bridge args JSON; expected a JSON array or object")?;
    let payload = serde_json::to_vec(&payload_value)?;
    let target = operation_target_from_wit_operation_id(operation_id)?;
    let suffix = mct_daemon::current_timestamp_string()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    let call_id = CallId::new(format!("call-jvm-bridge-{suffix}"))
        .expect("string ID literal/generated value must be non-empty");
    let trace_id = TraceId::new(format!("trace-jvm-bridge-{suffix}"))
        .expect("string ID literal/generated value must be non-empty");
    let span_id = SpanId::new(format!("span-jvm-bridge-{suffix}"))
        .expect("string ID literal/generated value must be non-empty");
    let protocol_request_id = ProtocolRequestId::new(format!("proto-jvm-bridge-{suffix}"))
        .expect("string ID literal/generated value must be non-empty");
    let call = MctCall {
        call_id: call_id.clone(),
        caller: CallerIdentity {
            node_id: MctNodeId::new("local-jvm-bridge")
                .expect("string ID literal/generated value must be non-empty"),
            user_id: None,
            vision_id: VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            project_id: None,
        },
        target,
        payload_metadata: PayloadMetadata {
            data_classification: "public".into(),
            size_bytes: payload.len() as u64,
            contains_secret_scoped_material: false,
        },
        authority_context: AuthorityContextSnapshot {
            policy_revision: 1,
            grants_revision: 1,
            vision_policy_revision: 1,
        },
        deadline: current_timestamp_after(DEFAULT_CLI_CALL_DEADLINE),
        trace_context: TraceContext { trace_id, span_id },
        origin: CallOrigin::JvmAdapter,
    };
    Ok((
        MctCallProtocolRequest {
            protocol_request_id,
            authority: MctCallProtocolAuthority {
                hello_decision_id: DecisionId::new("decision-jvm-bridge-local")
                    .expect("string ID literal/generated value must be non-empty"),
                peer_binding_id: PeerBindingId::new("binding-jvm-bridge-local")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: call.caller.vision_id.clone(),
                accepted_alpn: MCT_CALL_ALPN.into(),
                endpoint_id: EndpointIdText::new("local-jvm-bridge")
                    .expect("string ID literal/generated value must be non-empty"),
                policy_revision: call.authority_context.policy_revision,
                grants_revision: call.authority_context.grants_revision,
            },
            received_over: IrohConnectionPresentation {
                endpoint_id: EndpointIdText::new("local-jvm-bridge")
                    .expect("string ID literal/generated value must be non-empty"),
                alpn: "jvm/bridge/0".into(),
                connection_side: ConnectionSide::Incoming,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            call,
            payload: MctCallPayloadHandle::InlinePayload {
                inline_payload_ref: format!("payload-jvm-bridge-{suffix}"),
                content_type: "application/json".into(),
                size_bytes: payload.len() as u64,
                blake3_digest_hex: blake3_hex(&payload),
            },
            idempotency_key: Some(format!("idem-jvm-bridge-{suffix}")),
            received_observation_id: ObservationId::new(format!(
                "obs-jvm-bridge-received-{suffix}"
            ))
            .expect("string ID literal/generated value must be non-empty"),
        },
        payload,
    ))
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
            |_, _, _| async {
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
            move |request, _evaluation, _inline_payload| {
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
    let binding_signature_ref = take_option(&mut args, "--signature-ref");
    if args.len() < 5 {
        bail!(
            "expected: mct-daemon iroh call [--relay-default] <identity-file> <peer-ticket-file> <binding-id> <local-node-id> <vision-id> [namespace interface function] [--signature-ref proof]"
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
        binding_signature_ref,
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
    let children_dir = take_option(&mut args, "--children-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_children_dir);
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    if args.len() < 2 {
        bail!(
            "expected: mct-daemon iroh call-peer [--relay-default] <identity-file> <peer-node-id> [namespace interface function] [--config path] [--children-dir path] [--state path]"
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
    let capability_view =
        local_hello_capability_view_from_config(&config, &state_path, &children_dir)?;
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
    let hello_request = cli_hello_request_with_capability_view(
        &local_endpoint_id,
        &peer.binding_id,
        &MctNodeId::new("local-mct").expect("string ID literal/generated value must be non-empty"),
        &peer.vision_id,
        &trace_id,
        peer.binding_signature_ref.clone(),
        capability_view,
    );
    let hello_response = endpoint.send_hello(&peer_ticket, &hello_request).await?;
    refresh_remote_surfaces_from_admitted_hello_response(
        &state_path,
        peer,
        &hello_response,
        current_timestamp(),
    )?;
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
    signature_ref: Option<String>,
) -> MctHelloRequest {
    cli_hello_request_with_capability_view(
        endpoint_id,
        binding_id,
        node_id,
        vision_id,
        trace_id,
        signature_ref,
        None,
    )
}

fn cli_hello_request_with_capability_view(
    endpoint_id: &EndpointIdText,
    binding_id: &PeerBindingId,
    node_id: &MctNodeId,
    vision_id: &VisionId,
    trace_id: &TraceId,
    signature_ref: Option<String>,
    capability_view: Option<MctHelloCapabilityView>,
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
            signature_ref,
            expires_at: None,
        },
        capability_view,
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
            size_bytes: 0,
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
        binding_signature_ref: None,
        outbound_binding: None,
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
        "mct-daemon {version}\n\nCommands:\n  status\n  serve [--identity path] [--config path] [--children-dir path] [--state path] [--ledger path] [--max-connections n] [--relay-default] [--http addr | --uds socket-path]\n  control serve-http [addr] [--state path]\n  control serve-uds [socket-path] [--state path]\n  registry install <verified-package-dir> [--children-dir path] [--replace] [--json]\n  registry sync <source-id> [children-dir] [--state path] [--strict-integrity] [--json]\n  federation view [--config path] [--state path] [--children-dir path] [--json]\n  metrics snapshot [--state path] [--json]\n  pando record <composition-id> [step-id,call-id,runtime,child,decision ...] [--state path] [--json]\n  children load [children-dir] [--strict-integrity] [--json]\n  process call <executable> [payload-json] [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  children approve <child-name> [children-dir] [--config path] [--strict-integrity]\n  children revoke <child-name> [--config path]\n  children approvals [--config path] [--json]\n  children warmup <child-name> [--children-dir path] [--config path] [--ledger path] [--state path] [--json]\n  children reload <child-name> [--children-dir path] [--config path] [--ledger path] [--state path] [--json]\n  peers add <peer-node-id> <binding-id> <endpoint-id> <vision-id> [ticket-file] [--signature-ref proof] [--config path]\n  peers list [--config path] [--json]\n  peers set-outbound-proof <peer-node-id> <binding-id> --signature-ref proof [--expires-at ts] [--config path]\n  peers revoke <peer-node-id> [--config path]\n  peers remove <peer-node-id> [--config path]\n  state summary [--state path] [--json]\n  runs list [--state path] [--json] [--limit n]\n  slate list-work --project-root path [--status status] [--kind kind] [--children-dir path] [--config path] [--state path] [--ledger path]\n  toys authorize-slate <child-name> <project-root> [--children-dir path] [--config path] [--state path] [--json]\n  toys authorize-secret <child-name> <secret-name> [--children-dir path] [--config path] [--state path] [--json]\n  wasm call <component-file> <export-name> [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  wasm call-wit <child-name> <operation-id> <args-json> [--project-root path] [--guest-project /project] [--git-repo path] [--children-dir path] [--config path] [--ledger path] [--state path]\n  iroh identity [identity-file] [--config path]\n  iroh serve [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> [children-dir]\n  iroh serve-process [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> <executable> --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  iroh call [--relay-default] <identity-file> <peer-ticket-file> <binding-id> <local-node-id> <vision-id> [namespace interface function] [--signature-ref proof]\n  iroh call-peer [--relay-default] <identity-file> <peer-node-id> [namespace interface function] [--config path] [--children-dir path] [--state path]\n  jvm call-json <operation-id> <args-json> [--children-dir path] [--config path] [--state path] [--ledger path]",
        version = mct_daemon::version()
    );
}

#[cfg(test)]
#[path = "authority_test_fixture.rs"]
mod authority_test_fixture;

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use mct_iroh::{endpoint_id_for_secret_key_hex, sign_peer_binding_signature_ref};

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
                size_bytes: 0,
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
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();
        let client_signature_ref = store.load().unwrap().peers["mother-client"]
            .binding_signature_ref
            .clone();

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
            client_signature_ref,
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
        assert!(reply.route_taken.is_some());

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
    async fn resident_hello_publishes_federation_callable_surface() {
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
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();
        let client_signature_ref = store.load().unwrap().peers["mother-client"]
            .binding_signature_ref
            .clone();

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let resident = tokio::spawn(run_resident_mother(
            ResidentMotherConfig {
                config_path,
                identity_path,
                children_dir,
                state_path,
                ledger_path,
                control: ResidentControlTransport::Uds(socket_path),
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

        let trace_id = TraceId::new("trace-resident-hello-surface")
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
            client_signature_ref,
        );
        let hello_response = client.send_hello(&ticket, &hello).await.unwrap();
        assert_eq!(hello_response.hello_outcome, HelloOutcome::Admitted);
        let capability_view = hello_response
            .capability_view
            .expect("resident hello response publishes capability view");
        assert_eq!(
            capability_view.node_id,
            MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty")
        );
        assert_eq!(capability_view.vision_id, vision_id);
        assert!(capability_view.callable_surfaces.iter().any(|surface| {
            surface.child_name == "resident-echo"
                && surface.operation_id == "patina:demo/control@0.1.0.run"
                && surface.visibility == "vision_scoped"
        }));

        let _ = shutdown_tx.send(());
        tokio::time::timeout(Duration::from_secs(10), resident)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        client.close().await;
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

    #[tokio::test]
    async fn resident_mother_rejects_unsigned_peer_binding() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let identity_path = dir.path().join("identity").join("iroh-secret.hex");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        let children_dir = dir.path().join("children");

        let mut client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let client_endpoint_id = client.snapshot().endpoint_id;
        let store = MctDaemonConfigStore::new(&config_path);
        store
            .ensure_local_identity(MctOperatorNodeScope::default(), &identity_path)
            .unwrap();
        store
            .upsert_peer(MctPeerAddressBookEntry {
                peer_node_id: MctNodeId::new("mother-unsigned-client")
                    .expect("string ID literal/generated value must be non-empty"),
                binding_id: PeerBindingId::new("binding-resident-unsigned-client")
                    .expect("string ID literal/generated value must be non-empty"),
                endpoint_id: client_endpoint_id.clone(),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                ticket: None,
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();
        assert!(
            store.load().unwrap().peers["mother-unsigned-client"]
                .binding_signature_ref
                .is_some(),
            "server persists an issued proof, but the peer must present it"
        );

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let resident = tokio::spawn(run_resident_mother(
            ResidentMotherConfig {
                config_path,
                identity_path,
                children_dir,
                state_path,
                ledger_path,
                control: ResidentControlTransport::Uds(socket_path),
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

        let trace_id = TraceId::new("trace-resident-unsigned-peer")
            .expect("string ID literal/generated value must be non-empty");
        let binding_id = PeerBindingId::new("binding-resident-unsigned-client")
            .expect("string ID literal/generated value must be non-empty");
        let client_node_id = MctNodeId::new("mother-unsigned-client")
            .expect("string ID literal/generated value must be non-empty");
        let vision_id = VisionId::new("vision-local")
            .expect("string ID literal/generated value must be non-empty");
        let hello = cli_hello_request(
            &client_endpoint_id,
            &binding_id,
            &client_node_id,
            &vision_id,
            &trace_id,
            None,
        );
        let hello_response = client.send_hello(&ticket, &hello).await.unwrap();
        assert_eq!(hello_response.hello_outcome, HelloOutcome::Denied);
        assert_eq!(hello_response.safe_message, "not authorized");

        let _ = shutdown_tx.send(());
        tokio::time::timeout(Duration::from_secs(10), resident)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        client.close().await;
    }

    #[tokio::test]
    async fn resident_mother_payload_roundtrip_verifies_result_digest() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let identity_path = dir.path().join("identity").join("iroh-secret.hex");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        let socket_path = dir.path().join("control.sock");
        let children_dir = dir.path().join("children");
        write_resident_payload_process_child(&children_dir);

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
                peer_node_id: MctNodeId::new("mother-payload-client")
                    .expect("string ID literal/generated value must be non-empty"),
                binding_id: PeerBindingId::new("binding-resident-payload-client")
                    .expect("string ID literal/generated value must be non-empty"),
                endpoint_id: client_endpoint_id.clone(),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                ticket: None,
                binding_signature_ref: None,
                outbound_binding: None,
                binding_state: BindingState::Admitted,
                policy_revision: 1,
                updated_at: mct_daemon::current_timestamp_string(),
            })
            .unwrap();
        let client_signature_ref = store.load().unwrap().peers["mother-payload-client"]
            .binding_signature_ref
            .clone();

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let resident = tokio::spawn(run_resident_mother(
            ResidentMotherConfig {
                config_path,
                identity_path,
                children_dir,
                state_path,
                ledger_path: ledger_path.clone(),
                control: ResidentControlTransport::Uds(socket_path),
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

        let trace_id = TraceId::new("trace-resident-payload-e2e")
            .expect("string ID literal/generated value must be non-empty");
        let binding_id = PeerBindingId::new("binding-resident-payload-client")
            .expect("string ID literal/generated value must be non-empty");
        let client_node_id = MctNodeId::new("mother-payload-client")
            .expect("string ID literal/generated value must be non-empty");
        let vision_id = VisionId::new("vision-local")
            .expect("string ID literal/generated value must be non-empty");
        let hello = cli_hello_request(
            &client_endpoint_id,
            &binding_id,
            &client_node_id,
            &vision_id,
            &trace_id,
            client_signature_ref,
        );
        let hello_response = client.send_hello(&ticket, &hello).await.unwrap();
        assert_eq!(hello_response.hello_outcome, HelloOutcome::Admitted);

        let payload = br#"{"secret":"payload-marker"}"#.to_vec();
        let payload_base64 = BASE64_STANDARD.encode(&payload);
        let mut call = cli_call_request(
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
        call.call.call_id = CallId::new("call-resident-payload-e2e")
            .expect("string ID literal/generated value must be non-empty");
        call.call.payload_metadata.size_bytes = payload.len() as u64;
        call.payload = MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: "payload-resident-e2e".into(),
            content_type: "application/json".into(),
            size_bytes: payload.len() as u64,
            blake3_digest_hex: blake3_hex(&payload),
        };

        let call_reply = client
            .send_call_with_inline_payload(&ticket, &call, payload)
            .await
            .unwrap();
        let result_payload = call_reply
            .inline_result_payload
            .expect("verified result payload bytes returned");
        let expected_result = br#"processed:{"secret":"payload-marker"}"#.to_vec();
        let expected_result_base64 = BASE64_STANDARD.encode(&expected_result);
        assert_eq!(result_payload, expected_result);
        assert_eq!(
            call_reply.reply.reply_outcome,
            CallProtocolReplyOutcome::Success
        );
        assert_eq!(
            call_reply.reply.result_payload.declared_size_bytes(),
            expected_result.len() as u64
        );
        assert!(matches!(
            call_reply.reply.result_payload,
            MctCallPayloadHandle::InlinePayload { ref blake3_digest_hex, .. }
                if blake3_digest_hex == &blake3_hex(&expected_result)
        ));

        let _ = shutdown_tx.send(());
        tokio::time::timeout(Duration::from_secs(10), resident)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        client.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("call-resident-payload-e2e"));
        assert!(ledger_text.contains("payload:request:size="));
        assert!(ledger_text.contains("payload:result:size="));
        assert!(!ledger_text.contains("payload-marker"));
        assert!(!ledger_text.contains("processed:"));
        assert!(!ledger_text.contains(&payload_base64));
        assert!(!ledger_text.contains(&expected_result_base64));
    }

    #[tokio::test]
    async fn resident_process_payload_delivery_returns_digest_and_keeps_ledger_byte_free() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_payload_process_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let trace_id = TraceId::new("trace-resident-process-payload")
            .expect("string ID literal/generated value must be non-empty");
        let mut call = resident_test_call(trace_id);
        call.call_id = CallId::new("call-resident-process-payload")
            .expect("string ID literal/generated value must be non-empty");
        let payload = br#"{"secret":"payload-marker"}"#.to_vec();
        let payload_base64 = BASE64_STANDARD.encode(&payload);
        call.payload_metadata.size_bytes = payload.len() as u64;
        let mut request = resident_test_protocol_request(call);
        request.payload = MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: "payload-resident-process".into(),
            content_type: "application/json".into(),
            size_bytes: payload.len() as u64,
            blake3_digest_hex: blake3_hex(&payload),
        };

        let result = execute_resident_call(
            ResidentExecutionPaths {
                config_path,
                children_dir,
                state_path,
            },
            ledger.clone(),
            request,
            ResidentRequestPayload::remote(Some(payload)),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Completed);
        let result_payload = result
            .inline_result_payload
            .expect("result payload returned");
        let expected_result = r#"processed:{"secret":"payload-marker"}"#;
        let expected_result_base64 = BASE64_STANDARD.encode(expected_result.as_bytes());
        assert_eq!(String::from_utf8(result_payload).unwrap(), expected_result);
        assert_eq!(
            result.result_payload.declared_size_bytes(),
            expected_result.len() as u64
        );
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("call-resident-process-payload"));
        assert!(ledger_text.contains("payload:request:size="));
        assert!(ledger_text.contains("payload:result:size="));
        assert!(ledger_text.contains("digest="));
        assert!(!ledger_text.contains("payload-marker"));
        assert!(!ledger_text.contains("processed:"));
        assert!(!ledger_text.contains(&payload_base64));
        assert!(!ledger_text.contains(&expected_result_base64));
    }

    #[tokio::test]
    async fn jvm_bridge_json_call_enters_resident_route_path() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_payload_process_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let (mut request, payload) =
            jvm_bridge_protocol_request("patina:demo/control@0.1.0.run", r#"[{"from":"jvm"}]"#)
                .unwrap();
        request.call.call_id = CallId::new("call-jvm-bridge-test")
            .expect("string ID literal/generated value must be non-empty");
        assert_eq!(request.call.origin, CallOrigin::JvmAdapter);

        let result = execute_resident_call(
            ResidentExecutionPaths {
                config_path,
                children_dir,
                state_path,
            },
            ledger.clone(),
            request,
            ResidentRequestPayload::local(Some(payload)),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Completed);
        let result_payload = result
            .inline_result_payload
            .expect("result payload returned");
        assert_eq!(
            String::from_utf8(result_payload).unwrap(),
            r#"processed:[{"from":"jvm"}]"#
        );
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("call-jvm-bridge-test"));
        assert!(
            ledger_text.contains("RouteRevalidated") || ledger_text.contains("route_revalidated")
        );
    }

    #[tokio::test]
    async fn resident_local_blob_payload_delivery_returns_digest_and_keeps_ledger_byte_free() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_payload_process_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        assert_eq!(loaded.loaded, 1, "{loaded:?}");
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let payload = br#"{"secret":"blob-marker"}"#.to_vec();
        let payload_base64 = BASE64_STANDARD.encode(&payload);
        let payload_digest = blake3_hex(&payload);
        let handle = local_blob_store_for_state_path(&state_path)
            .ingest_reader(
                &payload_digest,
                payload.len() as u64,
                "application/json",
                std::io::Cursor::new(&payload),
            )
            .unwrap();
        let trace_id = TraceId::new("trace-resident-blob-payload")
            .expect("string ID literal/generated value must be non-empty");
        let mut call = resident_test_call(trace_id);
        call.call_id = CallId::new("call-resident-blob-payload")
            .expect("string ID literal/generated value must be non-empty");
        call.payload_metadata.size_bytes = payload.len() as u64;
        let mut request = resident_test_protocol_request(call);
        request.payload = handle;

        let result = execute_resident_call(
            ResidentExecutionPaths {
                config_path,
                children_dir,
                state_path,
            },
            ledger.clone(),
            request,
            ResidentRequestPayload::local(None),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Completed);
        let result_payload = result
            .inline_result_payload
            .expect("result payload returned");
        let expected_result = br#"processed:{"secret":"blob-marker"}"#.to_vec();
        let expected_result_base64 = BASE64_STANDARD.encode(&expected_result);
        assert_eq!(result_payload, expected_result);
        assert!(matches!(
            result.result_payload,
            MctCallPayloadHandle::InlinePayload { ref blake3_digest_hex, .. }
                if blake3_digest_hex == &blake3_hex(&expected_result)
        ));
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("call-resident-blob-payload"));
        assert!(ledger_text.contains(&payload_digest));
        assert!(ledger_text.contains("payload:request:size="));
        assert!(ledger_text.contains("payload:result:size="));
        assert!(!ledger_text.contains("blob-marker"));
        assert!(!ledger_text.contains("processed:"));
        assert!(!ledger_text.contains(&payload_base64));
        assert!(!ledger_text.contains(&expected_result_base64));
    }

    #[tokio::test]
    async fn resident_local_blob_absent_fails_closed_before_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_payload_process_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let payload = b"missing blob bytes";
        let payload_digest = blake3_hex(payload);
        let trace_id = TraceId::new("trace-resident-blob-missing")
            .expect("string ID literal/generated value must be non-empty");
        let mut call = resident_test_call(trace_id);
        call.call_id = CallId::new("call-resident-blob-missing")
            .expect("string ID literal/generated value must be non-empty");
        call.payload_metadata.size_bytes = payload.len() as u64;
        let mut request = resident_test_protocol_request(call);
        request.payload = mct_daemon::content_addressed_blob_handle(
            payload_digest,
            "application/json",
            payload.len() as u64,
        );

        let result = execute_resident_call(
            ResidentExecutionPaths {
                config_path,
                children_dir,
                state_path,
            },
            ledger.clone(),
            request,
            ResidentRequestPayload::local(None),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Failed);
        assert_eq!(result.safe_message, "payload blob unavailable");
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("PayloadBlobUnavailable"));
        assert!(!ledger_text.contains("missing blob bytes"));
        assert!(!ledger_text.contains(&BASE64_STANDARD.encode(payload)));
    }

    #[tokio::test]
    async fn resident_local_blob_tamper_fails_closed_via_digest_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_payload_process_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let payload = br#"{"secret":"trusted-blob"}"#.to_vec();
        let payload_digest = blake3_hex(&payload);
        let store = local_blob_store_for_state_path(&state_path);
        let handle = store
            .ingest_reader(
                &payload_digest,
                payload.len() as u64,
                "application/json",
                std::io::Cursor::new(&payload),
            )
            .unwrap();
        let tampered = vec![b'x'; payload.len()];
        std::fs::write(store.visible_path(&payload_digest).unwrap(), &tampered).unwrap();
        let trace_id = TraceId::new("trace-resident-blob-tamper")
            .expect("string ID literal/generated value must be non-empty");
        let mut call = resident_test_call(trace_id);
        call.call_id = CallId::new("call-resident-blob-tamper")
            .expect("string ID literal/generated value must be non-empty");
        call.payload_metadata.size_bytes = payload.len() as u64;
        let mut request = resident_test_protocol_request(call);
        request.payload = handle;

        let result = execute_resident_call(
            ResidentExecutionPaths {
                config_path,
                children_dir,
                state_path,
            },
            ledger.clone(),
            request,
            ResidentRequestPayload::local(None),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Failed);
        assert_eq!(result.safe_message, "malformed call payload");
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("PayloadDigestMismatch"));
        assert!(!ledger_text.contains("trusted-blob"));
        assert!(!ledger_text.contains(&BASE64_STANDARD.encode(&payload)));
        assert!(!ledger_text.contains(&BASE64_STANDARD.encode(&tampered)));
    }

    #[tokio::test]
    async fn resident_wit_rejects_non_json_payload_before_execution() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_wit_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path).unwrap();
        let trace_id = TraceId::new("trace-resident-wit-content-type")
            .expect("string ID literal/generated value must be non-empty");
        let mut call = resident_test_call(trace_id);
        let payload = b"not-json".to_vec();
        call.payload_metadata.size_bytes = payload.len() as u64;
        let mut request = resident_test_protocol_request(call);
        request.payload = MctCallPayloadHandle::InlinePayload {
            inline_payload_ref: "payload-resident-wit-text".into(),
            content_type: "text/plain".into(),
            size_bytes: payload.len() as u64,
            blake3_digest_hex: blake3_hex(&payload),
        };

        let result = execute_resident_call(
            ResidentExecutionPaths {
                config_path,
                children_dir,
                state_path,
            },
            ledger.clone(),
            request,
            ResidentRequestPayload::remote(Some(payload)),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Failed);
        assert_eq!(result.safe_message, "unsupported child payload");
        ledger.close().await;
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
            ResidentRequestPayload::remote(None),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Completed);
        assert!(result.route_decision_id.is_some());
        assert!(result.route_taken.is_some());
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

    #[tokio::test]
    async fn resident_route_optimization_cannot_grant_authority() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_wit_child(&children_dir);
        write_resident_process_child(&children_dir);

        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        let process_child = loaded
            .children
            .iter()
            .find(|child| child.name == "resident-echo")
            .unwrap();
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(process_child, MctOperatorChildScope::default())
            .unwrap();
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let trace_id = TraceId::new("trace-route-optimization-cannot-grant")
            .expect("string ID literal/generated value must be non-empty");
        let request = resident_test_protocol_request(resident_test_call(trace_id));

        let result = execute_resident_call(
            ResidentExecutionPaths {
                config_path,
                children_dir,
                state_path,
            },
            ledger.clone(),
            request,
            ResidentRequestPayload::remote(None),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Completed);
        assert!(matches!(
            result.route_taken,
            Some(RouteTaken {
                runtime_kind: RuntimeKind::Process,
                ..
            })
        ));
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("child:resident-wit"));
        assert!(ledger_text.contains("candidate_eliminated"));
        assert!(ledger_text.contains("ChildNotApproved"));
        assert!(ledger_text.contains("child:resident-echo"));
        assert!(ledger_text.contains("route_selected"));
    }

    #[tokio::test]
    async fn resident_no_route_records_specific_elimination() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let ledger_path = dir.path().join("observations.jsonl");
        write_resident_process_child(&children_dir);
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let trace_id = TraceId::new("trace-route-no-route-specific")
            .expect("string ID literal/generated value must be non-empty");
        let request = resident_test_protocol_request(resident_test_call(trace_id));

        let result = execute_resident_call(
            ResidentExecutionPaths {
                config_path,
                children_dir,
                state_path,
            },
            ledger.clone(),
            request,
            ResidentRequestPayload::remote(None),
        )
        .await;
        assert_eq!(result.outcome, CallProtocolOutcome::Denied);
        assert_eq!(result.safe_message, "not authorized");
        assert!(result.route_taken.is_none());
        ledger.close().await;

        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("candidate_eliminated"));
        assert!(ledger_text.contains("ChildNotApproved"));
        assert!(ledger_text.contains("no_route_recorded"));
    }

    #[test]
    fn resident_authorized_unavailable_is_temporal_no_route() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        write_resident_process_child(&children_dir);
        let mut loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir));
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        loaded.children[0].instance_state = mct_daemon::MctChildInstanceState::Loading;
        let config = MctDaemonConfigStore::new(&config_path).load().unwrap();
        let call = resident_test_call(
            TraceId::new("trace-route-unavailable")
                .expect("string ID literal/generated value must be non-empty"),
        );

        let outcome =
            authorize_resident_child_from_loaded(&config, loaded.children, &call).unwrap();
        let ResidentAuthorizationOutcome::Denied { observations, .. } = outcome else {
            panic!("loading child should produce temporal no-route")
        };
        let text = serde_json::to_string(&observations).unwrap();
        assert!(text.contains("CapabilityUnavailable"));
        assert!(text.contains("denial_class:temporal"));
    }

    #[test]
    fn resident_remote_surface_candidate_becomes_admissible_when_all_checks_pass() {
        let fixture = remote_surface_candidate_fixture();

        let plans = resident_remote_candidate_plans(
            &fixture.config,
            Some(&fixture.state),
            &fixture.call,
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
        )
        .unwrap();

        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].candidate.candidate_id,
            "peer:remote-mct:binding-remote:patina:demo/control@0.1.0.run:remote-child"
        );
        assert_eq!(plans[0].candidate.runtime_kind, RuntimeKind::RemotePeer);
        assert_eq!(plans[0].candidate.network_path, NetworkPathClass::Direct);
        assert_eq!(
            plans[0].authority.outcome,
            CandidateAuthorityOutcome::Admissible
        );
        assert_eq!(plans[0].authority.reason, None);
    }

    #[test]
    fn resident_remote_surface_candidate_forbids_secret_scope() {
        let mut fixture = remote_surface_candidate_fixture();
        fixture
            .call
            .payload_metadata
            .contains_secret_scoped_material = true;

        let plans = resident_remote_candidate_plans(
            &fixture.config,
            Some(&fixture.state),
            &fixture.call,
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
        )
        .unwrap();

        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].authority.reason,
            Some(CandidateEliminationReason::SecretScopeForbidden)
        );
    }

    #[test]
    fn resident_remote_route_candidates_reject_unsigned_peer_binding() {
        let mut fixture = remote_surface_candidate_fixture();
        fixture
            .config
            .peers
            .get_mut("remote-mct")
            .unwrap()
            .binding_signature_ref = None;

        let plans = resident_remote_candidate_plans(
            &fixture.config,
            Some(&fixture.state),
            &fixture.call,
            Timestamp::new("2026-07-09T00:01:00Z").unwrap(),
        )
        .unwrap();

        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].authority.reason,
            Some(CandidateEliminationReason::PeerNotAdmitted)
        );
    }

    #[test]
    fn resident_route_revision_guard_denies_before_effect() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let children_dir = dir.path().join("children");
        let state_path = dir.path().join("state.sqlite");
        let marker_path = dir.path().join("executed-marker");
        write_resident_process_child_script(
            &children_dir,
            "resident-echo",
            format!(
                "#!/bin/sh\necho executed > {}\nprintf '{{\"ok\":true}}'\n",
                marker_path.display()
            )
            .as_bytes(),
        );
        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(&loaded.children[0], MctOperatorChildScope::default())
            .unwrap();
        let config = MctDaemonConfigStore::new(&config_path).load().unwrap();
        let call = resident_test_call(
            TraceId::new("trace-route-stale-effect-guard")
                .expect("string ID literal/generated value must be non-empty"),
        );
        let request = resident_test_protocol_request(call.clone());
        let ResidentAuthorizationOutcome::Authorized(authorized) =
            authorize_resident_child_from_loaded(&config, loaded.children, &call).unwrap()
        else {
            panic!("approved child should authorize")
        };
        let stale_revisions = AuthorityContextSnapshot {
            policy_revision: call.authority_context.policy_revision + 1,
            grants_revision: call.authority_context.grants_revision,
            vision_policy_revision: call.authority_context.vision_policy_revision,
        };

        let report = execute_authorized_resident_child(
            ResidentExecutionPaths {
                config_path,
                children_dir,
                state_path,
            },
            *authorized,
            request,
            None,
            stale_revisions,
        )
        .unwrap();

        assert_eq!(report.result.outcome, ResultOutcome::Denied);
        assert!(report.result.route_taken.is_none());
        assert!(!marker_path.exists());
        let text = serde_json::to_string(&report.observations).unwrap();
        assert!(text.contains("PolicyRevisionStale"));
        assert!(text.contains("minted_policy_revision"));
    }

    #[test]
    fn route_taken_projection_follows_outcome_matrix() {
        let route = RouteTaken {
            node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            child_id: Some(
                ChildId::new("resident-echo")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            runtime_kind: RuntimeKind::Process,
        };

        for outcome in [
            ResultOutcome::Success,
            ResultOutcome::Failed,
            ResultOutcome::TimedOut,
        ] {
            assert_eq!(
                route_taken_for_outcome(outcome, route.clone()),
                Some(route.clone())
            );
        }
        for outcome in [ResultOutcome::Denied, ResultOutcome::Cancelled] {
            assert_eq!(route_taken_for_outcome(outcome, route.clone()), None);
        }
    }

    #[test]
    fn cancelled_result_and_reply_hide_route_while_ledger_keeps_selection() {
        let call = resident_test_call(
            TraceId::new("trace-route-cancelled-mid-execution")
                .expect("string ID literal/generated value must be non-empty"),
        );
        let route = CandidateRoute {
            candidate_id: "child:resident-echo".into(),
            node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            child_id: Some(
                ChildId::new("resident-echo")
                    .expect("string ID literal/generated value must be non-empty"),
            ),
            runtime_kind: RuntimeKind::Process,
            network_path: NetworkPathClass::Local,
        };
        let decision = RouteDecision::selected(
            &call,
            route.clone(),
            vec![CandidateAuthorityEvaluation::admissible(route, 1, 1)],
            resident_route_decision_ids("cancelled", &call),
        );
        let observation = route_decision_observation(
            call.trace_context.trace_id.clone(),
            current_timestamp(),
            &decision,
        );
        let result = MctResult {
            call_id: call.call_id.clone(),
            outcome: ResultOutcome::Cancelled,
            route_taken: None,
            authority_decision_ref: decision.decision_id.clone(),
            execution_summary: ExecutionSummary {
                wall_time_ms: 0,
                execution_time_ms: None,
                queue_wait_ms: None,
                input_size_bytes: 0,
                output_size_bytes: None,
            },
            result_payload: MctCallPayloadHandle::Empty,
            requester_message: "cancelled".into(),
            audit_ref: AuditRef::new("audit-cancelled-route")
                .expect("string ID literal/generated value must be non-empty"),
        };
        let reply = MctCallProtocolReply {
            reply_id: ReplyId::new("reply-cancelled-route")
                .expect("string ID literal/generated value must be non-empty"),
            protocol_request_id: ProtocolRequestId::new("proto-cancelled-route")
                .expect("string ID literal/generated value must be non-empty"),
            decision_id: decision.decision_id,
            result_ref: None,
            result_payload: MctCallPayloadHandle::Empty,
            route_taken: None,
            reply_outcome: CallProtocolReplyOutcome::Cancelled,
            safe_message: "cancelled".into(),
            reply_observation_id: ObservationId::new("obs-reply-cancelled-route")
                .expect("string ID literal/generated value must be non-empty"),
        };

        assert!(result.route_taken.is_none());
        assert!(reply.validate().is_ok());
        assert!(reply.route_taken.is_none());
        assert_eq!(observation.kind, ObservationKind::RouteSelected);
        assert_eq!(observation.resource_id, Some("child:resident-echo".into()));
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

    struct RemoteSurfaceCandidateFixture {
        _dir: tempfile::TempDir,
        config: mct_daemon::MctDaemonConfig,
        state: MctRuntimeStateStore,
        call: MctCall,
    }

    fn remote_surface_candidate_fixture() -> RemoteSurfaceCandidateFixture {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let state_path = dir.path().join("state.sqlite");
        let local_identity_path = dir.path().join("identity").join("iroh-secret.hex");
        let remote_identity_path = dir.path().join("remote").join("iroh-secret.hex");
        let store = MctDaemonConfigStore::new(&config_path);
        let local_identity = store
            .ensure_local_identity(MctOperatorNodeScope::default(), &local_identity_path)
            .unwrap();
        let remote_secret = load_or_create_node_secret_key_hex(&remote_identity_path).unwrap();
        let remote_endpoint_id = endpoint_id_for_secret_key_hex(&remote_secret).unwrap();
        store
            .upsert_peer(resident_remote_peer_entry(
                "remote-mct",
                "binding-remote",
                remote_endpoint_id.as_str(),
                "vision-local",
                BindingState::Admitted,
                None,
            ))
            .unwrap();
        let mut config = store.load().unwrap();
        let peer = config.peers.get("remote-mct").unwrap().clone();
        let outbound_binding = MctOutboundPeerBindingPresentation {
            binding_id: PeerBindingId::new("binding-outbound-local")
                .expect("string ID literal/generated value must be non-empty"),
            policy_revision: 1,
            signature_ref: String::new(),
            expires_at: None,
        };
        let outbound_binding_to_sign =
            outbound_peer_binding_for_local(&local_identity, &peer, &outbound_binding).unwrap();
        let outbound_signature = sign_peer_binding_signature_ref(
            &remote_secret,
            &outbound_binding_to_sign,
            &remote_endpoint_id,
        )
        .unwrap();
        store
            .set_peer_outbound_proof(
                &peer.peer_node_id,
                MctOutboundPeerBindingPresentation {
                    signature_ref: outbound_signature,
                    ..outbound_binding
                },
            )
            .unwrap();
        config = store.load().unwrap();
        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        let view = hello_capability_view(
            &peer.peer_node_id,
            &peer.vision_id,
            1,
            &["patina:demo/control@0.1.0.run"],
        );
        state
            .refresh_remote_callable_surfaces(MctRemoteSurfaceRefresh {
                peer_node_id: &peer.peer_node_id,
                binding_id: &peer.binding_id,
                endpoint_id: &peer.endpoint_id,
                view: &view,
                received_at: &Timestamp::new("2026-07-09T00:00:00Z").unwrap(),
                stale_at: &Timestamp::new("2026-07-09T00:05:00Z").unwrap(),
                view_observation_id: &ObservationId::new("obs-remote-surface-view")
                    .expect("string ID literal/generated value must be non-empty"),
            })
            .unwrap();
        let call = resident_test_call(
            TraceId::new("trace-remote-route-candidate")
                .expect("string ID literal/generated value must be non-empty"),
        );
        RemoteSurfaceCandidateFixture {
            _dir: dir,
            config,
            state,
            call,
        }
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
            negotiated_protocol: Some(HelloPolicy::default().protocol),
            accepted_alpns: vec![MCT_CALL_ALPN.into()],
            hello_outcome: HelloOutcome::Admitted,
            reason: HelloReason::ActiveBinding,
            safe_reason: SafeHelloReason::Admitted,
            observation_id: ObservationId::new("obs-remote-surface-admitted")
                .expect("string ID literal/generated value must be non-empty"),
        }
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
        write_resident_process_child_script(
            children_dir,
            "resident-echo",
            b"#!/bin/sh\ncat >/dev/null\nprintf '{\\\"ok\\\":true}'\n",
        );
    }

    fn write_resident_payload_process_child(children_dir: &Path) {
        write_resident_process_child_script(
            children_dir,
            "resident-payload-echo",
            b"#!/bin/sh\npayload=$(cat)\nprintf 'processed:%s' \"$payload\"\n",
        );
    }

    fn write_resident_process_child_script(children_dir: &Path, name: &str, script: &[u8]) {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        let child_dir = children_dir.join(name);
        std::fs::create_dir_all(&child_dir).unwrap();
        let artifact_path = child_dir.join(format!("{name}.wasm"));
        let manifest_path = child_dir.join("child.toml");
        std::fs::write(&artifact_path, script).unwrap();
        #[cfg(unix)]
        {
            let mut permissions = std::fs::metadata(&artifact_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&artifact_path, permissions).unwrap();
        }
        write_resident_child_manifest(&manifest_path, name, "handle");
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
    fn authorize_secret_cli_persists_scoped_grant_without_value() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let state_path = dir.path().join("state.sqlite");
        let children_dir = dir.path().join("children");
        write_resident_process_child(&children_dir);
        let loaded = load_children_from_dir(MctChildLoadOptions::new(children_dir.clone()));
        let child = loaded
            .children
            .iter()
            .find(|child| child.name == "resident-echo")
            .unwrap();
        MctDaemonConfigStore::new(&config_path)
            .approve_and_assign_loaded_child(child, MctOperatorChildScope::default())
            .unwrap();

        run_toys_authorize_secret(vec![
            "resident-echo".into(),
            "api-token".into(),
            "--children-dir".into(),
            children_dir.display().to_string(),
            "--config".into(),
            config_path.display().to_string(),
            "--state".into(),
            state_path.display().to_string(),
        ])
        .unwrap();

        let state = MctRuntimeStateStore::open(&state_path).unwrap();
        let contracts = state.toy_contracts().unwrap();
        let grants = state.toy_grant_snapshots().unwrap();
        assert_eq!(contracts, vec![mct_secrets_toy_contract()]);
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].toy_id.as_str(), MCT_SECRETS_TOY_ID);
        assert_eq!(grants[0].scope.resource_id.as_deref(), Some("api-token"));
        let grant_json = serde_json::to_string(&grants).unwrap();
        assert!(!grant_json.contains("super-secret"));
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

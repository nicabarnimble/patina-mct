use anyhow::{Context, Result, bail};
use mct_daemon::{
    MctChildLoadOptions, MctConfigChildAuthorityProjection, MctDaemonConfigStore,
    MctOperatorChildScope, MctPeerAddressBookEntry, MctProcessChildHarness,
    MctProcessChildInvocationIds, MctRuntimeStateStore, MctWasmComponentInvocationIds,
    MctWasmComponentRuntime, default_config_path, default_state_path, load_children_from_dir,
    reload_configured_child, warmup_configured_child,
};
use mct_iroh::{
    MctIrohCallHandlerResult, MctIrohServeState, MctIrohServedProtocol, MotherIrohEndpoint,
    MotherIrohEndpointConfig, MotherIrohEndpointTicket, MotherIrohRelayMode,
    endpoint_id_for_secret_key_hex, load_or_create_node_secret_key_hex,
};
use mct_kernel::*;
use mct_observation::JsonlObservationLedger;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

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
        "children" => run_children(args)?,
        "process" => run_process(args)?,
        "peers" => run_peers(args)?,
        "state" => run_state(args)?,
        "runs" => run_runs(args)?,
        "wasm" => run_wasm(args)?,
        "iroh" => run_iroh(args).await?,
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
        mct_daemon::unix_timestamp_string(),
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
            started_at: Timestamp::from("2026-05-31T00:00:00Z"),
            completed_at: Timestamp::from("2026-05-31T00:00:01Z"),
        },
    )?;
    append_ledger_observations(&ledger_path, &report.observations)?;
    state.append_run_observations(&run_id, &report.observations)?;
    state.complete_run(&run_id, &report.result, mct_daemon::unix_timestamp_string())?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn run_wasm(mut args: Vec<String>) -> Result<()> {
    if args.first().map(String::as_str) != Some("call") || args.len() < 3 {
        bail!(
            "expected: mct-daemon wasm call <component-file> <export-name> [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]"
        );
    }
    args.remove(0);
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
        mct_daemon::unix_timestamp_string(),
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
            started_at: Timestamp::from("2026-05-31T00:00:00Z"),
            completed_at: Timestamp::from("2026-05-31T00:00:01Z"),
        },
    )?;
    append_ledger_observations(&ledger_path, &report.observations)?;
    state.append_run_observations(&run_id, &report.observations)?;
    state.complete_run(&run_id, &report.result, mct_daemon::unix_timestamp_string())?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
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
        bail!("expected peers subcommand: add | list | remove");
    }
    match args.remove(0).as_str() {
        "add" => run_peers_add(args),
        "list" => run_peers_list(args),
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
        updated_at: mct_daemon::unix_timestamp_string(),
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
        deadline: Timestamp::from("2026-05-31T00:01:00Z"),
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
        deadline: Timestamp::from("2026-05-31T00:01:00Z"),
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
            let identity_path = args
                .first()
                .map(PathBuf::from)
                .unwrap_or_else(default_identity_path);
            let secret_key_hex = load_or_create_node_secret_key_hex(&identity_path)?;
            let endpoint_id = endpoint_id_for_secret_key_hex(&secret_key_hex)?;
            println!("endpoint_id={endpoint_id}");
            println!("identity={}", identity_path.display());
        }
        "serve" => serve_iroh(args).await?,
        "serve-process" => serve_iroh_process(args).await?,
        "call" => call_iroh(args).await?,
        "call-peer" => call_iroh_peer(args).await?,
        other => bail!("unknown iroh subcommand '{other}'"),
    }
    Ok(())
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

    let binding = MctPeerBinding {
        binding_id,
        iroh_endpoint_id: peer_endpoint_id,
        scope: MctPeerBindingScope {
            mct_node_id: peer_node_id,
            vision_id,
            allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            data_scope: None,
            observation_scope: None,
        },
        issuer_node_id: MctNodeId::from("local-mct"),
        policy_revision: 1,
        binding_state: BindingState::Admitted,
        issued_at: Timestamp::from("2026-05-31T00:00:00Z"),
        expires_at: None,
        created_by_observation_id: ObservationId::from("obs-cli-peer-binding"),
        superseded_by_observation_id: None,
    };
    let mut state = MctIrohServeState::new();

    loop {
        match endpoint
            .serve_next(
                &mut state,
                std::slice::from_ref(&binding),
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

    let binding = MctPeerBinding {
        binding_id,
        iroh_endpoint_id: peer_endpoint_id,
        scope: MctPeerBindingScope {
            mct_node_id: peer_node_id,
            vision_id,
            allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            data_scope: None,
            observation_scope: None,
        },
        issuer_node_id: MctNodeId::from("local-mct"),
        policy_revision: 1,
        binding_state: BindingState::Admitted,
        issued_at: Timestamp::from("2026-05-31T00:00:00Z"),
        expires_at: None,
        created_by_observation_id: ObservationId::from("obs-cli-peer-binding"),
        superseded_by_observation_id: None,
    };
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
                        mct_daemon::unix_timestamp_string(),
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
                            started_at: Timestamp::from("2026-05-31T00:00:00Z"),
                            completed_at: Timestamp::from("2026-05-31T00:00:01Z"),
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
                        mct_daemon::unix_timestamp_string(),
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
        deadline: Timestamp::from("2026-05-31T00:01:00Z"),
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
        mct_daemon::unix_timestamp_string(),
    )?;
    Ok(())
}

fn run_id_for_call(prefix: &str, call: &MctCall) -> String {
    format!(
        "run:{}:{}:{}",
        prefix,
        call.call_id,
        mct_daemon::unix_timestamp_string()
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
        "mct-daemon {version}\n\nCommands:\n  status\n  children load [children-dir] [--strict-integrity] [--json]\n  process call <executable> [payload-json] [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  children approve <child-name> [children-dir] [--config path] [--strict-integrity]\n  children revoke <child-name> [--config path]\n  children approvals [--config path] [--json]\n  children warmup <child-name> [--children-dir path] [--config path] [--ledger path] [--state path] [--json]\n  children reload <child-name> [--children-dir path] [--config path] [--ledger path] [--state path] [--json]\n  peers add <peer-node-id> <binding-id> <endpoint-id> <vision-id> [ticket-file] [--config path]\n  peers list [--config path] [--json]\n  peers remove <peer-node-id> [--config path]\n  state summary [--state path] [--json]\n  runs list [--state path] [--json] [--limit n]\n  wasm call <component-file> <export-name> [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  iroh identity [identity-file]\n  iroh serve [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> [children-dir]\n  iroh serve-process [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> <executable> --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  iroh call [--relay-default] <identity-file> <peer-ticket-file> <binding-id> <local-node-id> <vision-id> [namespace interface function]\n  iroh call-peer [--relay-default] <identity-file> <peer-node-id> [namespace interface function] [--config path]",
        version = mct_daemon::version()
    );
}

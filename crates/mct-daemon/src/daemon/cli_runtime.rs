use super::*;

pub(super) fn run_children(mut args: Vec<String>) -> Result<()> {
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

pub(super) fn run_children_load(mut args: Vec<String>) -> Result<()> {
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

pub(super) fn run_children_approve(mut args: Vec<String>) -> Result<()> {
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

pub(super) fn run_children_revoke(mut args: Vec<String>) -> Result<()> {
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

pub(super) fn run_children_approvals(mut args: Vec<String>) -> Result<()> {
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

pub(super) fn run_children_warmup(args: Vec<String>) -> Result<()> {
    run_child_lifecycle(args, "warmup")
}

pub(super) fn run_children_reload(args: Vec<String>) -> Result<()> {
    run_child_lifecycle(args, "reload")
}

pub(super) fn run_child_lifecycle(mut args: Vec<String>, action: &str) -> Result<()> {
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

pub(super) fn run_process(mut args: Vec<String>) -> Result<()> {
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

pub(super) fn run_wasm(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected wasm subcommand: call | call-wit");
    }
    match args.remove(0).as_str() {
        "call" => run_wasm_call(args),
        "call-wit" => run_wasm_call_wit(args),
        other => bail!("unknown wasm subcommand '{other}'"),
    }
}

pub(super) fn run_wasm_call(mut args: Vec<String>) -> Result<()> {
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

pub(super) fn run_wasm_call_wit(mut args: Vec<String>) -> Result<()> {
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

pub(super) struct CliWitHostAdapterBuild {
    pub(super) adapters: MctWitHostImportAdapters,
    pub(super) observations: Vec<MctObservation>,
}

pub(super) struct CliToyAuthorizationError {
    pub(super) safe_message: String,
    pub(super) observations: Vec<MctObservation>,
}

pub(super) struct CliWitAdapterRequest<'a> {
    pub(super) state: &'a MctRuntimeStateStore,
    pub(super) child: &'a mct_daemon::MctLoadedChild,
    pub(super) authorized_child: &'a AuthorizedChildInvocation,
    pub(super) call: &'a MctCall,
    pub(super) imports: &'a BTreeSet<String>,
    pub(super) project_root: Option<&'a Path>,
    pub(super) guest_project: &'a str,
    pub(super) git_repo: Option<&'a Path>,
}

pub(super) fn build_wit_host_adapters_for_cli_call(
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

pub(super) struct CliAuthorizedToy {
    pub(super) evaluation: ToyGrantEvaluation,
    pub(super) authorized: AuthorizedToyCall,
}

pub(super) struct CliToyAuthorizationRequest<'a> {
    pub(super) child: &'a mct_daemon::MctLoadedChild,
    pub(super) authorized_child: &'a AuthorizedChildInvocation,
    pub(super) call: &'a MctCall,
    pub(super) contracts: &'a [CanonicalToyContract],
    pub(super) grants: &'a [ToyGrant],
    pub(super) toy_id: ToyId,
    pub(super) action: &'a str,
    pub(super) resource_id: Option<String>,
    pub(super) label: &'a str,
}

pub(super) fn authorize_cli_toy(
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

pub(super) fn cli_adapter_error(error: anyhow::Error) -> CliToyAuthorizationError {
    CliToyAuthorizationError {
        safe_message: error.to_string(),
        observations: Vec::new(),
    }
}

pub(super) fn wit_toy_adapter(
    authorized_toy_call: AuthorizedToyCall,
    observation_id_prefix: &str,
) -> MctWitToyHostAdapter {
    MctWitToyHostAdapter {
        authorized_toy_call,
        observation_id_prefix: observation_id_prefix.into(),
        observed_at: current_timestamp(),
    }
}

pub(super) fn imports_need_wasi_p2(imports: &BTreeSet<String>) -> bool {
    imports
        .iter()
        .any(|name| name.starts_with("wasi:") && name != "wasi:logging/logging@0.1.0")
}

pub(super) fn imports_need_wasi_filesystem(imports: &BTreeSet<String>) -> bool {
    imports.iter().any(|name| {
        matches!(
            name.as_str(),
            "wasi:filesystem/types@0.2.3" | "wasi:filesystem/preopens@0.2.3"
        )
    })
}

pub(super) fn load_named_child(
    children_dir: &Path,
    child_name: &str,
) -> Result<mct_daemon::MctLoadedChild> {
    let report = load_children_from_dir(MctChildLoadOptions::new(children_dir));
    report
        .children
        .into_iter()
        .find(|child| child.name == child_name)
        .ok_or_else(|| anyhow::anyhow!("loaded child '{child_name}' not found"))
}

pub(super) fn operation_target_from_wit_operation_id(
    operation_id: &str,
) -> Result<OperationTarget> {
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

pub(super) fn run_wit_runtime_on_blocking_thread<T>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T>
where
    T: Send + 'static,
{
    std::thread::spawn(f)
        .join()
        .map_err(|panic| anyhow::anyhow!("WIT runtime worker panicked: {panic:?}"))?
}

pub(super) fn canonical_dir(path: PathBuf, label: &str) -> Result<PathBuf> {
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

pub(super) fn slate_logging_toy_id() -> ToyId {
    ToyId::new("toy:slate:wasi-logging")
        .expect("string ID literal/generated value must be non-empty")
}

pub(super) fn slate_measure_toy_id() -> ToyId {
    ToyId::new("toy:slate:patina-measure")
        .expect("string ID literal/generated value must be non-empty")
}

pub(super) fn slate_git_toy_id() -> ToyId {
    ToyId::new("toy:slate:patina-git").expect("string ID literal/generated value must be non-empty")
}

pub(super) fn slate_filesystem_toy_id() -> ToyId {
    ToyId::new("toy:slate:wasi-filesystem-project")
        .expect("string ID literal/generated value must be non-empty")
}

pub(super) fn run_slate(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected slate subcommand: list-work");
    }
    match args.remove(0).as_str() {
        "list-work" => run_slate_list_work(args),
        other => bail!("unknown slate subcommand '{other}'"),
    }
}

pub(super) fn run_slate_list_work(mut args: Vec<String>) -> Result<()> {
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

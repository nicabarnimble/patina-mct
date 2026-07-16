use super::*;
use mct_kernel::{
    ArtifactSourceAuthority, ArtifactSourceAuthorityId, ArtifactSourceAuthorityState,
    ArtifactSourceScope, ArtifactSourceScopeMode,
};
use patina_sdk::manifest::ChildPackage;

pub(super) fn run_artifacts(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected artifacts subcommand: stage | acquire | show | acquisitions | sources");
    }
    match args.remove(0).as_str() {
        "stage" => run_artifacts_stage(args),
        "acquire" => run_artifacts_acquire(args),
        "show" => run_artifacts_show(args),
        "acquisitions" => run_artifact_acquisitions(args),
        "sources" => run_artifact_sources(args),
        other => bail!("unknown artifacts subcommand '{other}'"),
    }
}

fn run_artifacts_stage(mut args: Vec<String>) -> Result<()> {
    let manifest_path = take_option(&mut args, "--manifest").map(PathBuf::from);
    let component_path = take_option(&mut args, "--component").map(PathBuf::from);
    let child = take_option(&mut args, "--child");
    let version = take_option(&mut args, "--version");
    let expected_digest = take_option(&mut args, "--expected-digest");
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
    let as_json = take_flag(&mut args, "--json");
    if args.len() != 1 {
        bail!("artifacts stage requires exactly one source root");
    }
    let request = MctArtifactStageRequest {
        source_root: PathBuf::from(args.remove(0)),
        manifest_path: manifest_path.context("artifacts stage requires --manifest")?,
        component_path: component_path.context("artifacts stage requires --component")?,
        claimed_child_name: child.context("artifacts stage requires --child")?,
        claimed_artifact_version: version.context("artifacts stage requires --version")?,
        expected_digest,
        standing_source_authority_id: None,
        claimed_publisher: None,
        require_source_sidecars: false,
        children_dir,
        state_path,
    };
    execute_artifact_stage(&request, &ledger_path, &socket_path, as_json)
}

fn run_artifacts_acquire(mut args: Vec<String>) -> Result<()> {
    let operator_pointed = take_flag(&mut args, "--operator-pointed");
    let source_authority = take_option(&mut args, "--source-authority");
    let child = take_option(&mut args, "--child");
    let version = take_option(&mut args, "--version");
    let expected_digest = take_option(&mut args, "--expected-digest");
    let claimed_publisher = take_option(&mut args, "--publisher");
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
    let as_json = take_flag(&mut args, "--json");
    if operator_pointed == source_authority.is_some() {
        bail!("artifacts acquire requires exactly one authority selector");
    }
    if source_authority.is_some() && claimed_publisher.is_none() {
        bail!("standing-source package acquisition requires --publisher");
    }
    if args.len() != 1 {
        bail!("artifacts acquire requires exactly one package directory");
    }
    let package_dir = PathBuf::from(args.remove(0));
    let package = ChildPackage::from_package_dir(&package_dir)?;
    let component_path = package
        .artifact_path
        .strip_prefix(&package_dir)
        .context("package component escapes package root")?
        .to_path_buf();
    let request = MctArtifactStageRequest {
        source_root: package_dir,
        manifest_path: PathBuf::from(patina_sdk::manifest::CHILD_MANIFEST_FILE),
        component_path,
        claimed_child_name: child.context("artifacts acquire requires --child")?,
        claimed_artifact_version: version.context("artifacts acquire requires --version")?,
        expected_digest,
        standing_source_authority_id: source_authority,
        claimed_publisher,
        require_source_sidecars: true,
        children_dir,
        state_path,
    };
    execute_artifact_stage(&request, &ledger_path, &socket_path, as_json)
}

fn execute_artifact_stage(
    request: &MctArtifactStageRequest,
    ledger_path: &Path,
    socket_path: &Path,
    as_json: bool,
) -> Result<()> {
    let body = serde_json::to_vec(request)?;
    let report: MctArtifactAcquisitionReport = if let Some(response) =
        try_resident_control_mutation(socket_path, "/artifacts/stage", &body)?
    {
        serde_json::from_slice(&response).context("decode resident artifact stage report")?
    } else {
        execute_offline_artifact_stage(ledger_path, request).with_context(|| {
            format!(
                "resident UDS {} unavailable and offline artifact staging failed",
                socket_path.display()
            )
        })?
    };
    if as_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "acquired child={} version={} artifact={} acquisition={} digest={} path={}",
            report.child_name,
            report.artifact_version,
            report.artifact_id.as_deref().unwrap_or("none"),
            report.acquisition_id,
            report.observed_digest.as_deref().unwrap_or("unknown"),
            report
                .package_path
                .as_deref()
                .map(Path::display)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".into())
        );
    }
    Ok(())
}

fn run_artifacts_show(mut args: Vec<String>) -> Result<()> {
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let as_json = take_flag(&mut args, "--json");
    if args.len() != 1 {
        bail!("artifacts show requires one exact artifact id");
    }
    let artifact_id = ComponentArtifactId::new(args.remove(0))?;
    let state = MctRuntimeStateStore::open(state_path)?;
    let artifact = state
        .get_artifact(&artifact_id)?
        .context("artifact not found")?;
    let acquisitions = state
        .artifact_acquisitions()?
        .into_iter()
        .filter(|acquisition| acquisition.component_artifact_id.as_ref() == Some(&artifact_id))
        .collect::<Vec<_>>();
    let package = state.artifact_package(&artifact_id)?;
    let evidence = serde_json::json!({
        "artifact": artifact,
        "acquisitions": acquisitions,
        "package": package,
    });
    if as_json {
        println!("{}", serde_json::to_string_pretty(&evidence)?);
    } else {
        println!(
            "artifact={} child={} version={} provenance={:?} acquisitions={}",
            artifact.artifact_id,
            artifact.child_name,
            artifact.artifact_version,
            artifact.provenance_status,
            acquisitions.len()
        );
        for acquisition in acquisitions {
            println!(
                "acquisition={} path={:?} source={} outcome={:?}/{:?} digest={}",
                acquisition.acquisition_id,
                acquisition.authority_path,
                acquisition.source_ref,
                acquisition.acquisition_outcome,
                acquisition.verification_outcome,
                acquisition.observed_digest.as_deref().unwrap_or("unknown")
            );
        }
    }
    Ok(())
}

fn run_artifact_acquisitions(mut args: Vec<String>) -> Result<()> {
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let artifact_filter = take_option(&mut args, "--artifact");
    let as_json = take_flag(&mut args, "--json");
    if !args.is_empty() {
        bail!("unexpected artifacts acquisitions arguments");
    }
    let mut acquisitions = MctRuntimeStateStore::open(state_path)?.artifact_acquisitions()?;
    if let Some(artifact) = artifact_filter {
        acquisitions.retain(|acquisition| {
            acquisition
                .component_artifact_id
                .as_ref()
                .is_some_and(|id| id.as_str() == artifact)
        });
    }
    if as_json {
        println!("{}", serde_json::to_string_pretty(&acquisitions)?);
    } else {
        for acquisition in acquisitions {
            println!(
                "acquisition={} child={} version={} outcome={:?}/{:?}",
                acquisition.acquisition_id,
                acquisition.claimed_child_name,
                acquisition.claimed_artifact_version,
                acquisition.acquisition_outcome,
                acquisition.verification_outcome
            );
        }
    }
    Ok(())
}

fn run_artifact_sources(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("expected artifacts sources subcommand: create | revoke | list");
    }
    match args.remove(0).as_str() {
        "create" => run_artifact_source_create(args),
        "revoke" => run_artifact_source_revoke(args),
        "list" => run_artifact_source_list(args),
        other => bail!("unknown artifacts sources subcommand '{other}'"),
    }
}

fn run_artifact_source_create(mut args: Vec<String>) -> Result<()> {
    let root = take_option(&mut args, "--filesystem-root").map(PathBuf::from);
    let scope_mode = take_option(&mut args, "--scope-mode");
    let artifacts = take_repeated_options(&mut args, "--artifact");
    let publishers = take_repeated_options(&mut args, "--publisher");
    let namespaces = take_repeated_options(&mut args, "--namespace");
    let actions = take_repeated_options(&mut args, "--action");
    let integrity_policy = take_option(&mut args, "--integrity-policy");
    let provenance_policy = take_option(&mut args, "--provenance-policy");
    let expires_at = take_option(&mut args, "--expires-at");
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let as_json = take_flag(&mut args, "--json");
    let socket_path = take_option(&mut args, "--uds")
        .map(PathBuf::from)
        .unwrap_or_else(default_control_uds_path);
    if args.len() != 1 {
        bail!("artifacts sources create requires one source authority id");
    }
    if artifacts.is_empty() || publishers.is_empty() || namespaces.is_empty() || actions.is_empty()
    {
        bail!("artifact source authority requires every explicit scope dimension");
    }
    if actions.iter().any(|action| action != "acquire") {
        bail!("filesystem artifact source supports only acquire action");
    }
    let root = root
        .context("source authority requires --filesystem-root")?
        .canonicalize()?;
    let issued_at = current_timestamp();
    let expires_at = Timestamp::new(expires_at.context("source authority requires --expires-at")?)?;
    if expires_at <= issued_at {
        bail!("source authority expiry must be in the future");
    }
    let scope_mode = match scope_mode.as_deref() {
        Some("constrained") => ArtifactSourceScopeMode::Constrained,
        Some("explicit-broad") => ArtifactSourceScopeMode::ExplicitBroad,
        _ => bail!("source authority requires constrained or explicit-broad scope mode"),
    };
    if scope_mode == ArtifactSourceScopeMode::Constrained
        && artifacts
            .iter()
            .chain(&publishers)
            .chain(&namespaces)
            .any(|value| value == "*")
    {
        bail!("constrained source authority cannot contain broad scope");
    }
    let observation_id = ObservationId::new(format!(
        "obs:artifact-source:{}:{}",
        args[0],
        current_timestamp_string()
    ))?;
    let source = ArtifactSourceAuthority {
        source_authority_id: ArtifactSourceAuthorityId::new(args.remove(0))?,
        source_ref: format!("file://{}", root.display()),
        scope: ArtifactSourceScope {
            scope_mode,
            artifact_scope: artifacts,
            publisher_scope: publishers,
            namespace_scope: namespaces,
            allowed_actions: actions,
        },
        integrity_policy_ref: integrity_policy
            .context("source authority requires --integrity-policy")?,
        provenance_policy_ref: provenance_policy,
        issuer_principal_ref: format!("os-uid:{}", cli_current_uid()?),
        policy_revision: 1,
        authority_state: ArtifactSourceAuthorityState::Active,
        issued_at,
        expires_at,
        authority_observation_id: observation_id.clone(),
    };
    let record_bytes = serde_json::to_vec(&source)?;
    let record_digest = blake3::hash(&record_bytes).to_hex().to_string();
    let mutation = ArtifactSourceMutationRequest {
        expected_state_path: state_path.clone(),
        source: source.clone(),
        record_digest: record_digest.clone(),
    };
    MctRuntimeStateStore::open(&state_path)?
        .validate_source_authority_projection(&source, &record_digest)?;
    if try_resident_control_mutation(
        &socket_path,
        "/artifacts/sources/create",
        &serde_json::to_vec(&mutation)?,
    )?
    .is_none()
    {
        let observation = artifact_source_observation(
            &source,
            ObservationOutcome::Allowed,
            format!("artifact source authority created digest={record_digest}"),
        );
        let mut ledger = JsonlObservationLedger::open(&ledger_path, "ledger-local", "local-mct")?;
        ledger.append_batch_before_effect([observation], current_timestamp_string())?;
        MctRuntimeStateStore::open(state_path)?.upsert_source_authority(&source, &record_digest)?;
    }
    if as_json {
        println!("{}", serde_json::to_string_pretty(&source)?);
    } else {
        println!(
            "artifact source={} root={} state=active expires={}",
            source.source_authority_id,
            source.source_ref,
            source.expires_at.as_str()
        );
    }
    Ok(())
}

fn run_artifact_source_revoke(mut args: Vec<String>) -> Result<()> {
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let ledger_path = take_option(&mut args, "--ledger")
        .map(PathBuf::from)
        .unwrap_or_else(default_observation_ledger_path);
    let as_json = take_flag(&mut args, "--json");
    let socket_path = take_option(&mut args, "--uds")
        .map(PathBuf::from)
        .unwrap_or_else(default_control_uds_path);
    if args.len() != 1 {
        bail!("artifacts sources revoke requires one source authority id");
    }
    let state = MctRuntimeStateStore::open(state_path)?;
    let mut source = state
        .source_authorities()?
        .into_iter()
        .find(|(source, _)| source.source_authority_id.as_str() == args[0])
        .map(|(source, _)| source)
        .context("artifact source authority not found")?;
    source.authority_state = ArtifactSourceAuthorityState::Revoked;
    source.authority_observation_id = ObservationId::new(format!(
        "obs:artifact-source-revoke:{}:{}",
        source.source_authority_id,
        current_timestamp_string()
    ))?;
    let digest = blake3::hash(&serde_json::to_vec(&source)?)
        .to_hex()
        .to_string();
    let mutation = ArtifactSourceMutationRequest {
        expected_state_path: state.path().to_path_buf(),
        source: source.clone(),
        record_digest: digest.clone(),
    };
    state.validate_source_authority_projection(&source, &digest)?;
    if try_resident_control_mutation(
        &socket_path,
        "/artifacts/sources/revoke",
        &serde_json::to_vec(&mutation)?,
    )?
    .is_none()
    {
        let observation = artifact_source_observation(
            &source,
            ObservationOutcome::Denied,
            format!("artifact source authority revoked digest={digest}"),
        );
        let mut ledger = JsonlObservationLedger::open(&ledger_path, "ledger-local", "local-mct")?;
        ledger.append_batch_before_effect([observation], current_timestamp_string())?;
        state.upsert_source_authority(&source, &digest)?;
    }
    if as_json {
        println!("{}", serde_json::to_string_pretty(&source)?);
    } else {
        println!(
            "artifact source={} state=revoked",
            source.source_authority_id
        );
    }
    Ok(())
}

fn run_artifact_source_list(mut args: Vec<String>) -> Result<()> {
    let state_path = take_option(&mut args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let as_json = take_flag(&mut args, "--json");
    let _ledger_path = take_option(&mut args, "--ledger");
    if !args.is_empty() {
        bail!("unexpected artifacts sources list arguments");
    }
    let sources = MctRuntimeStateStore::open(state_path)?.source_authorities()?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&sources)?);
    } else {
        let now = current_timestamp();
        for (source, _) in sources {
            let effective = if source.authority_state == ArtifactSourceAuthorityState::Active
                && source.expires_at <= now
            {
                "expired"
            } else {
                match source.authority_state {
                    ArtifactSourceAuthorityState::Pending => "pending",
                    ArtifactSourceAuthorityState::Active => "active",
                    ArtifactSourceAuthorityState::Revoked => "revoked",
                    ArtifactSourceAuthorityState::Expired => "expired",
                    ArtifactSourceAuthorityState::Superseded => "superseded",
                }
            };
            println!(
                "artifact source={} ref={} state={} expires={}",
                source.source_authority_id,
                source.source_ref,
                effective,
                source.expires_at.as_str()
            );
        }
    }
    Ok(())
}

fn artifact_source_observation(
    source: &ArtifactSourceAuthority,
    outcome: ObservationOutcome,
    safe_message: String,
) -> MctObservation {
    MctObservation {
        observation_id: source.authority_observation_id.clone(),
        observed_at: current_timestamp(),
        kind: ObservationKind::OperatorActionRecorded,
        source_plane: SourcePlane::Operator,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!(
                "trace:artifact-source:{}",
                source.source_authority_id
            ))
            .expect("source id makes non-empty trace"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(source.source_authority_id.to_string()),
        resource_id: Some(source.source_ref.clone()),
        policy_revision: Some(source.policy_revision),
        grants_revision: None,
        outcome,
        visibility: ObservationVisibility::NodeOperator,
        safe_message,
        detail_ref: None,
    }
}

fn take_repeated_options(args: &mut Vec<String>, flag: &str) -> Vec<String> {
    let mut values = Vec::new();
    while let Some(index) = args.iter().position(|arg| arg == flag) {
        args.remove(index);
        if index < args.len() {
            values.push(args.remove(index));
        }
    }
    values
}

fn cli_current_uid() -> Result<u32> {
    let output = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()?;
    if !output.status.success() {
        bail!("authenticate current OS UID: id -u failed");
    }
    Ok(String::from_utf8(output.stdout)?.trim().parse()?)
}

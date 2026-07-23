use super::*;
use std::io::Write as _;

fn exact_sha256(value: Option<String>, label: &str) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let valid = value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    });
    if !valid {
        bail!("{label} must be sha256:<64-lower-hex>");
    }
    Ok(Some(value))
}

fn require_exact_upgrade_approval(approval: &str, release_artifact_id: &str) -> Result<()> {
    if approval != release_artifact_id {
        bail!(
            "upgrade approval did not equal the exact release artifact id; no lifecycle effect occurred"
        );
    }
    Ok(())
}

fn print_upgrade_candidate(
    context: &UpgradeSupervisorContext,
    verified: &MctVerifiedDaemonRelease,
    json: bool,
) -> Result<()> {
    let report = verified.report();
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    let artifact = &report.artifact;
    println!("current product version: {}", mct_daemon::version());
    println!(
        "current executable: {} ({})",
        context.current_executable.display(),
        context.current_executable_digest
    );
    println!(
        "current supervisor: {} revision {}",
        context.supervisor_record_id, context.supervisor_revision
    );
    println!(
        "candidate release artifact: {}",
        artifact.release_artifact_id
    );
    println!("candidate product version: {}", artifact.product_version);
    println!("candidate target: {}", artifact.target_triple);
    println!("candidate archive BLAKE3: {}", artifact.archive_blake3);
    println!(
        "candidate executable SHA-256: {}",
        artifact.executable_sha256
    );
    println!(
        "candidate executable BLAKE3: {}",
        artifact.executable_blake3
    );
    println!("source: {} {}", artifact.source_kind, artifact.source_ref);
    println!(
        "evidence: acquisition={} verification={}",
        artifact.acquisition_observation_id, artifact.verification_observation_id
    );
    println!(
        "provenance: revision={} toolchain={} SBOM={} fixtures={} signing={}",
        artifact.source_revision,
        artifact.rust_toolchain,
        artifact.sbom_sha256,
        artifact.fixture_provenance_sha256,
        artifact.signing_mode
    );
    println!("\n{}", report.release_notes);
    println!(
        "plan: exact approval -> clean stop -> shared install --replace -> shared start -> bounded post-verification"
    );
    println!(
        "rollback guidance: prior immutable releases are retained; invoke the current executable with install --replace --executable <prior-exact-path>, then start"
    );
    Ok(())
}

fn run_upgrade_with_context_factory<F>(mut args: Vec<String>, context_factory: F) -> Result<()>
where
    F: FnOnce(Option<PathBuf>) -> Result<UpgradeSupervisorContext>,
{
    if args.is_empty() {
        bail!("upgrade requires <artifact-ref>");
    }
    let artifact_ref = args.remove(0);
    let source_plan = plan_daemon_release_source(&artifact_ref)?;
    let selected_root = take_option(&mut args, "--root").map(PathBuf::from);
    let expected_digest = exact_sha256(
        take_option(&mut args, "--expected-digest"),
        "expected daemon release digest",
    )?;
    let supplied_approval = exact_sha256(
        take_option(&mut args, "--approve-artifact"),
        "daemon release approval",
    )?;
    let json = take_flag(&mut args, "--json");
    if take_flag(&mut args, "--yes") {
        bail!("upgrade has no broad --yes authority; approve the exact release artifact digest");
    }
    if !args.is_empty() {
        bail!("unexpected upgrade arguments: {}", args.join(" "));
    }

    let context = context_factory(selected_root)?;
    let request = MctDaemonReleaseAcquisitionRequest {
        source_path: source_plan.source_path,
        expected_archive_identity: expected_digest,
        target_triple: "aarch64-apple-darwin".into(),
        releases_dir: context.releases_dir.clone(),
        state_path: context.state_path.clone(),
        ledger_path: context.ledger_path.clone(),
        authenticated_uid: context.authenticated_uid,
        policy_revision: context.supervisor_revision,
    };
    let body = serde_json::to_vec(&request)?;
    let report: MctDaemonReleaseAcquisitionReport = if let Some(response) =
        try_resident_control_mutation(&context.uds_path, "/releases/acquire", &body)?
    {
        serde_json::from_slice(&response).context("decode resident daemon release report")?
    } else {
        acquire_operator_file_daemon_release_offline(&request).with_context(|| {
            format!(
                "resident UDS {} unavailable and offline daemon release acquisition failed",
                context.uds_path.display()
            )
        })?
    };
    let verified = MctVerifiedDaemonRelease::from_acquisition(report)?;
    print_upgrade_candidate(&context, &verified, json)?;
    let artifact_id = &verified.report().artifact.release_artifact_id;
    let approval = match supplied_approval {
        Some(approval) => approval,
        None if json => {
            record_upgrade_fact(&context, artifact_id, "upgrade_approval_denied")?;
            bail!("JSON upgrade requires --approve-artifact with the exact release artifact id")
        }
        None => {
            print!("Type the complete release artifact id to approve replacement: ");
            std::io::stdout().flush()?;
            let mut input = String::new();
            if std::io::stdin().read_line(&mut input)? == 0 {
                record_upgrade_fact(&context, artifact_id, "upgrade_approval_denied")?;
                bail!("upgrade approval denied at EOF; no lifecycle effect occurred");
            }
            input.trim_end_matches(['\r', '\n']).to_owned()
        }
    };
    if let Err(error) = require_exact_upgrade_approval(&approval, artifact_id) {
        record_upgrade_fact(&context, artifact_id, "upgrade_approval_denied")?;
        return Err(error);
    }
    record_upgrade_fact(&context, artifact_id, "upgrade_approval_admitted")?;
    let completion = execute_upgrade_lifecycle(&context, &verified.report().artifact)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&completion)?);
    } else {
        println!(
            "upgrade completed artifact={} supervisor={} revision={}",
            artifact_id,
            completion
                .supervisor_record_id
                .as_deref()
                .unwrap_or("unknown"),
            completion.supervisor_revision.unwrap_or_default()
        );
    }
    Ok(())
}

pub(super) fn run_upgrade(args: Vec<String>) -> Result<()> {
    run_upgrade_with_context_factory(args, upgrade_supervisor_context)
}

#[cfg(feature = "release-smoke-internal")]
pub(super) fn run_upgrade_in_context(
    args: Vec<String>,
    context: UpgradeSupervisorContext,
) -> Result<()> {
    run_upgrade_with_context_factory(args, |selected_root| {
        if selected_root.is_some() {
            bail!("release-smoke-internal upgrade does not admit a second root selector");
        }
        Ok(context)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_is_only_the_complete_exact_archive_sha256_identity() {
        let artifact = format!("sha256:{}", "a".repeat(64));
        assert!(require_exact_upgrade_approval(&artifact, &artifact).is_ok());
        let wrong_digest = format!("sha256:{}", "b".repeat(64));
        for refusal in [
            "0.2.0",
            "mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz",
            "yes",
            "",
            wrong_digest.as_str(),
        ] {
            assert!(require_exact_upgrade_approval(refusal, &artifact).is_err());
        }
    }
}

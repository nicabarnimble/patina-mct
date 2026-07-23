use crate::{
    MctChildLoadOptions, MctRuntimeStateStore, component_artifact_from_loaded_child,
    current_timestamp, current_timestamp_string, load_children_from_dir,
};
use anyhow::{Context, Result, bail};
use mct_kernel::{
    ArtifactAcquisition, ArtifactAcquisitionAuthorityPath, ArtifactAcquisitionAuthorityRequest,
    ArtifactAcquisitionDecisionId, ArtifactAcquisitionId, ArtifactAcquisitionOutcome,
    ArtifactProvenanceStatus, ArtifactSourceAuthority, ArtifactSourceAuthorityState,
    ArtifactVerificationOutcome, AuthorizedArtifactAcquisitionId,
    FilesystemAcquisitionEffectAuthority, ObservationId, ObservationKind, ObservationOutcome,
    OperatorPointedAcquisitionState, OperatorPointedArtifactAcquisitionDecision, SourcePlane,
    Timestamp, evaluate_artifact_acquisition_authority,
};
use mct_observation::JsonlObservationLedger;
use patina_sdk::manifest::ChildManifest as SdkChildManifest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

/// Maximum canonical manifest bytes accepted by staging.
pub const MCT_CHILD_MANIFEST_MAX_BYTES: usize = 1024 * 1024;
/// Maximum primary component bytes accepted by staging.
pub const MCT_COMPONENT_ARTIFACT_MAX_BYTES: usize = 64 * 1024 * 1024;
/// Fixed direct-operator filesystem acquisition adapter identity.
pub const MCT_FILESYSTEM_ACQUISITION_ADAPTER: &str = "mct:artifact-acquisition/filesystem@1";

static NEXT_ACQUISITION_ID: AtomicU64 = AtomicU64::new(1);

/// Correlated identities minted before an acquisition decision becomes durable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctArtifactAttemptContext {
    pub acquisition_id: ArtifactAcquisitionId,
    pub decision_id: ArtifactAcquisitionDecisionId,
    pub authority_observation_id: ObservationId,
    pub adapter_start_observation_id: ObservationId,
    pub acquisition_observation_id: ObservationId,
    pub verification_observation_id: ObservationId,
}

/// Mints correlated attempt identities without touching the selected source.
pub fn new_artifact_attempt_context() -> Result<MctArtifactAttemptContext> {
    let sequence = NEXT_ACQUISITION_ID.fetch_add(1, Ordering::SeqCst);
    let suffix = format!("{}-{sequence}", current_timestamp_string());
    Ok(MctArtifactAttemptContext {
        acquisition_id: ArtifactAcquisitionId::new(format!("acquisition:{suffix}"))?,
        decision_id: ArtifactAcquisitionDecisionId::new(format!("decision:{suffix}"))?,
        authority_observation_id: ObservationId::new(format!(
            "obs:acquisition-authority:{suffix}"
        ))?,
        adapter_start_observation_id: ObservationId::new(format!(
            "obs:acquisition-adapter-start:{suffix}"
        ))?,
        acquisition_observation_id: ObservationId::new(format!("obs:acquisition:{suffix}"))?,
        verification_observation_id: ObservationId::new(format!(
            "obs:artifact-verification:{suffix}"
        ))?,
    })
}

/// Validated correlation between one standing-source projection and its canonical ledger fact.
///
/// Private fields prevent SQLite state alone from being repackaged as proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctStandingSourceLedgerProof {
    source: ArtifactSourceAuthority,
    record_digest: String,
    ledger_sequence: u64,
}

/// Validates one standing source against the hash-validated canonical observation ledger.
pub fn verify_standing_source_ledger_correlation(
    state_path: &Path,
    ledger_path: &Path,
    source_authority_id: &str,
) -> Result<MctStandingSourceLedgerProof> {
    let state = MctRuntimeStateStore::open(state_path)?;
    let (source, projected_digest) = state
        .source_authorities()?
        .into_iter()
        .find(|(source, _)| source.source_authority_id.as_str() == source_authority_id)
        .context("standing artifact source authority not found in projection")?;
    let record_digest = blake3::hash(&serde_json::to_vec(&source)?)
        .to_hex()
        .to_string();
    if projected_digest != record_digest {
        bail!("standing artifact source projection digest mismatch");
    }
    if source.authority_state != ArtifactSourceAuthorityState::Active {
        bail!("standing artifact source is not active");
    }

    let entries = JsonlObservationLedger::open_read_only(ledger_path, "ledger-local", "local-mct")?
        .entries()
        .context("read validated standing-source ledger")?;
    let matching = entries
        .iter()
        .filter(|entry| entry.observation.observation_id == source.authority_observation_id)
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        bail!("standing artifact source requires exactly one ledger authority fact");
    }
    let entry = matching[0];
    let observation = &entry.observation;
    let expected_message = format!("artifact source authority created digest={record_digest}");
    if observation.kind != ObservationKind::OperatorActionRecorded
        || observation.source_plane != SourcePlane::Operator
        || observation.subject_id.as_deref() != Some(source.source_authority_id.as_str())
        || observation.resource_id.as_deref() != Some(source.source_ref.as_str())
        || observation.policy_revision != Some(source.policy_revision)
        || observation.outcome != ObservationOutcome::Allowed
        || observation.safe_message != expected_message
        || observation.detail_ref.is_some()
    {
        bail!("standing artifact source ledger authority fact mismatch");
    }

    Ok(MctStandingSourceLedgerProof {
        source,
        record_digest,
        ledger_sequence: entry.local_sequence,
    })
}

/// Inputs for staging raw local build output through one operator-pointed acquisition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctArtifactStageRequest {
    /// Canonical local root containing selected build output.
    pub source_root: PathBuf,
    /// Root-relative source manifest path.
    pub manifest_path: PathBuf,
    /// Root-relative source component path.
    pub component_path: PathBuf,
    /// Child name claimed before source access.
    pub claimed_child_name: String,
    /// Artifact version claimed before source access.
    pub claimed_artifact_version: String,
    /// Optional expected algorithm-tagged BLAKE3 component digest.
    pub expected_digest: Option<String>,
    /// Standing source authority id; absent means operator-pointed staging.
    #[serde(default)]
    pub standing_source_authority_id: Option<String>,
    /// Publisher claim required by standing-source scope.
    #[serde(default)]
    pub claimed_publisher: Option<String>,
    /// Whether source SHA-256 sidecars must already exist and match.
    #[serde(default)]
    pub require_source_sidecars: bool,
    /// Immutable package catalog root.
    pub children_dir: PathBuf,
    /// Runtime state projection path.
    pub state_path: PathBuf,
}

/// Durable report for a completed artifact acquisition and verification attempt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctArtifactAcquisitionReport {
    /// Immutable acquisition identifier.
    pub acquisition_id: ArtifactAcquisitionId,
    /// Child name claimed and verified by the package.
    pub child_name: String,
    /// Artifact version claimed and verified by the package.
    pub artifact_version: String,
    /// SHA-256 artifact identity when verification succeeded.
    pub artifact_id: Option<String>,
    /// Exact component byte count when observed.
    pub observed_size_bytes: Option<u64>,
    /// Algorithm-tagged BLAKE3 component digest when observed.
    pub observed_digest: Option<String>,
    /// Source adapter outcome.
    pub acquisition_outcome: String,
    /// Independent verification outcome.
    pub verification_outcome: String,
    /// Immutable package path when catalog publication succeeded.
    pub package_path: Option<PathBuf>,
    /// Terminal acquisition observation identifier.
    pub acquisition_observation_id: ObservationId,
    /// Verification observation identifier when reached.
    pub verification_observation_id: Option<ObservationId>,
}

/// Stages raw local build output and records one acquisition-backed artifact.
///
/// This library entry point performs the already-authorized product operation. CLI/resident
/// orchestration is responsible for making its operator and adapter-start facts durable before
/// calling it.
pub fn stage_operator_pointed_artifact(
    request: &MctArtifactStageRequest,
) -> Result<MctArtifactAcquisitionReport> {
    let context = new_artifact_attempt_context()?;
    stage_artifact_with_context(request, &context)
}

/// Executes an attempt without an external terminal observer.
pub fn stage_artifact_with_context(
    request: &MctArtifactStageRequest,
    context: &MctArtifactAttemptContext,
) -> Result<MctArtifactAcquisitionReport> {
    stage_artifact_with_context_and_observer(request, context, None, |_| Ok(()))
}

/// Executes an attempt and requires its successful terminal facts before catalog publication.
pub fn stage_artifact_with_context_and_observer<F>(
    request: &MctArtifactStageRequest,
    context: &MctArtifactAttemptContext,
    standing_source_proof: Option<&MctStandingSourceLedgerProof>,
    observe_verified: F,
) -> Result<MctArtifactAcquisitionReport>
where
    F: FnOnce(&MctArtifactAcquisitionReport) -> Result<()>,
{
    validate_claim(&request.claimed_child_name, "child name")?;
    validate_claim(&request.claimed_artifact_version, "artifact version")?;
    validate_expected_digest(request.expected_digest.as_deref())?;

    validate_relative_path(&request.manifest_path)?;
    if !request.component_path.as_os_str().is_empty() {
        validate_relative_path(&request.component_path)?;
    }
    let claimed_source_root =
        std::path::absolute(&request.source_root).context("make artifact source root absolute")?;
    let operator_source_root = request
        .standing_source_authority_id
        .is_none()
        .then(|| request.source_root.canonicalize());
    let state = MctRuntimeStateStore::open(&request.state_path)?;
    let standing = match request.standing_source_authority_id.as_deref() {
        Some(source_id) => {
            let proof = standing_source_proof
                .context("standing artifact source requires validated ledger proof")?;
            if proof.source.source_authority_id.as_str() != source_id {
                bail!("standing artifact source proof identifies a different source");
            }
            let projected = state
                .source_authorities()?
                .into_iter()
                .find(|(source, _)| source.source_authority_id.as_str() == source_id)
                .context("standing artifact source authority not found")?;
            if projected.0 != proof.source || projected.1 != proof.record_digest {
                bail!("standing artifact source projection changed after ledger proof");
            }
            Some(proof.source.clone())
        }
        None => {
            if standing_source_proof.is_some() {
                bail!("operator-pointed acquisition cannot consume standing-source proof");
            }
            None
        }
    };
    let source_ref = standing.as_ref().map_or_else(
        || {
            format!(
                "file://{}",
                operator_source_root
                    .as_ref()
                    .and_then(|root| root.as_ref().ok())
                    .unwrap_or(&claimed_source_root)
                    .display()
            )
        },
        |source| source.source_ref.clone(),
    );

    let acquisition_id = context.acquisition_id.clone();
    let decision_id = context.decision_id.clone();
    let generated_authority_observation_id = context.authority_observation_id.clone();
    let acquisition_observation_id = context.acquisition_observation_id.clone();
    let verification_observation_id = context.verification_observation_id.clone();
    let operator = standing
        .is_none()
        .then(|| OperatorPointedArtifactAcquisitionDecision {
            decision_id: decision_id.clone(),
            source_ref: source_ref.clone(),
            claimed_child_name: request.claimed_child_name.clone(),
            claimed_artifact_version: request.claimed_artifact_version.clone(),
            expected_digest: request.expected_digest.clone(),
            issuer_principal_ref: format!("os-uid:{}", current_uid().unwrap_or_default()),
            policy_revision: 1,
            decision_state: OperatorPointedAcquisitionState::Active,
            authority_observation_id: generated_authority_observation_id,
        });
    if let Some(decision) = operator.as_ref() {
        state.record_operator_acquisition_decision(decision)?;
    }
    let effect = FilesystemAcquisitionEffectAuthority {
        authority_ref: context.adapter_start_observation_id.clone(),
        adapter_ref: MCT_FILESYSTEM_ACQUISITION_ADAPTER.into(),
        authenticated_uid: current_uid()?,
        source_ref: source_ref.clone(),
        allowed_action: "read_and_stage".into(),
        policy_revision: 1,
        attempt_id: acquisition_id.clone(),
        expires_at: timestamp_after_one_minute()?,
    };
    let namespaces = standing.as_ref().map_or_else(
        || vec!["operator-pointed".into()],
        |source| source.scope.namespace_scope.clone(),
    );
    let mut authority_request = ArtifactAcquisitionAuthorityRequest {
        source_ref: source_ref.clone(),
        artifact: format!(
            "{}@{}",
            request.claimed_child_name, request.claimed_artifact_version
        ),
        publisher: request.claimed_publisher.clone(),
        expected_digest: request.expected_digest.clone(),
        authenticated_uid: current_uid()?,
        namespaces,
        action: "acquire".into(),
        policy_revision: 1,
        now: current_timestamp(),
        attempt_id: acquisition_id.clone(),
        authorized_id: AuthorizedArtifactAcquisitionId::new(format!(
            "authorized:{}",
            acquisition_id.as_str()
        ))?,
    };
    let authority_path = if standing.is_some() {
        ArtifactAcquisitionAuthorityPath::StandingSource
    } else {
        ArtifactAcquisitionAuthorityPath::OperatorPointed
    };
    let standing_source_authority_id = standing
        .as_ref()
        .map(|source| source.source_authority_id.clone());
    let operator_pointed_decision_id = operator.as_ref().map(|value| value.decision_id.clone());
    let failed_acquisition = || ArtifactAcquisition {
        acquisition_id: acquisition_id.clone(),
        authority_path,
        standing_source_authority_id: standing_source_authority_id.clone(),
        operator_pointed_decision_id: operator_pointed_decision_id.clone(),
        adapter_effect_authority_ref: context.adapter_start_observation_id.to_string(),
        source_ref: source_ref.clone(),
        claimed_child_name: request.claimed_child_name.clone(),
        claimed_artifact_version: request.claimed_artifact_version.clone(),
        observed_size_bytes: None,
        observed_digest: None,
        acquisition_outcome: ArtifactAcquisitionOutcome::Failed,
        verification_outcome: ArtifactVerificationOutcome::NotReached,
        verification_observation_id: None,
        acquisition_observation_id: acquisition_observation_id.clone(),
        component_artifact_id: None,
    };
    let record_terminal = |acquisition: &ArtifactAcquisition| -> Result<()> {
        state.record_artifact_acquisition(acquisition)?;
        if let Some(decision_id) = acquisition.operator_pointed_decision_id.as_ref() {
            state.consume_operator_acquisition_decision(decision_id)?;
        }
        Ok(())
    };
    let authority = evaluate_artifact_acquisition_authority(
        &authority_request,
        standing.as_ref(),
        operator.as_ref(),
        Some(&effect),
    );
    let Some(authorized) = authority.authorized else {
        record_terminal(&failed_acquisition())?;
        bail!("source trust and filesystem effect authority denied");
    };
    let rejected_acquisition = |size: u64, digest: String| ArtifactAcquisition {
        acquisition_id: acquisition_id.clone(),
        authority_path,
        standing_source_authority_id: standing_source_authority_id.clone(),
        operator_pointed_decision_id: operator_pointed_decision_id.clone(),
        adapter_effect_authority_ref: context.adapter_start_observation_id.to_string(),
        source_ref: source_ref.clone(),
        claimed_child_name: request.claimed_child_name.clone(),
        claimed_artifact_version: request.claimed_artifact_version.clone(),
        observed_size_bytes: Some(size),
        observed_digest: Some(digest),
        acquisition_outcome: ArtifactAcquisitionOutcome::Acquired,
        verification_outcome: ArtifactVerificationOutcome::Rejected,
        verification_observation_id: Some(verification_observation_id.clone()),
        acquisition_observation_id: acquisition_observation_id.clone(),
        component_artifact_id: None,
    };
    let record_rejected = |acquisition: &ArtifactAcquisition| record_terminal(acquisition);

    let resolved_source_root =
        operator_source_root.unwrap_or_else(|| request.source_root.canonicalize());
    let source_root = match resolved_source_root {
        Ok(root) if root.is_dir() => root,
        Ok(_) => {
            record_terminal(&failed_acquisition())?;
            bail!("artifact source root is not a directory");
        }
        Err(error) => {
            record_terminal(&failed_acquisition())?;
            return Err(error)
                .with_context(|| format!("resolve source root {}", request.source_root.display()));
        }
    };
    if let Some(source) = standing.as_ref() {
        let root = source
            .source_ref
            .strip_prefix("file://")
            .context("standing source is not a filesystem source")?;
        let authority_root = match PathBuf::from(root).canonicalize() {
            Ok(root) => root,
            Err(error) => {
                record_terminal(&failed_acquisition())?;
                return Err(error).context("resolve standing artifact source root");
            }
        };
        if !source_root.starts_with(&authority_root) {
            record_terminal(&failed_acquisition())?;
            bail!("acquisition package is outside standing source root");
        }
    }
    let manifest_path = match canonical_source_file(&source_root, &request.manifest_path) {
        Ok(path) => path,
        Err(error) => {
            record_terminal(&failed_acquisition())?;
            return Err(error);
        }
    };
    let mut preloaded_manifest = None;
    let selected_component_path = if request.component_path.as_os_str().is_empty() {
        let bytes = match read_bounded_file(&manifest_path, MCT_CHILD_MANIFEST_MAX_BYTES) {
            Ok(bytes) => bytes,
            Err(error) => {
                record_terminal(&failed_acquisition())?;
                return Err(error).context("acquire bounded package manifest");
            }
        };
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(error) => {
                record_terminal(&failed_acquisition())?;
                return Err(error).context("package manifest is not UTF-8");
            }
        };
        let manifest = match SdkChildManifest::from_toml_str(text) {
            Ok(manifest) => manifest,
            Err(error) => {
                record_terminal(&failed_acquisition())?;
                return Err(error).context("parse package manifest before component acquisition");
            }
        };
        let declared = match manifest.artifact.wasm {
            Some(path) => path,
            None => {
                record_terminal(&failed_acquisition())?;
                bail!("package-shaped acquisition requires a declared component path");
            }
        };
        if let Err(error) = validate_relative_path(&declared) {
            record_terminal(&failed_acquisition())?;
            return Err(error);
        }
        preloaded_manifest = Some(bytes);
        declared
    } else {
        request.component_path.clone()
    };
    let component_path = match canonical_source_file(&source_root, &selected_component_path) {
        Ok(path) => path,
        Err(error) => {
            record_terminal(&failed_acquisition())?;
            return Err(error);
        }
    };

    let manifest_bytes = match preloaded_manifest {
        Some(bytes) => bytes,
        None => match read_bounded_file(&manifest_path, MCT_CHILD_MANIFEST_MAX_BYTES) {
            Ok(bytes) => bytes,
            Err(error) => {
                record_terminal(&failed_acquisition())?;
                return Err(error).context("acquire bounded artifact manifest");
            }
        },
    };
    let component_bytes = match read_bounded_file(&component_path, MCT_COMPONENT_ARTIFACT_MAX_BYTES)
    {
        Ok(bytes) => bytes,
        Err(error) => {
            record_terminal(&failed_acquisition())?;
            return Err(error).context("acquire bounded component bytes");
        }
    };
    let observed_digest = format!("blake3:{}", blake3::hash(&component_bytes).to_hex());
    if request.require_source_sidecars
        && let Err(error) = verify_source_sidecar(&manifest_path, &manifest_bytes)
            .and_then(|()| verify_source_sidecar(&component_path, &component_bytes))
    {
        let acquisition =
            rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
        record_rejected(&acquisition)?;
        return Err(error).context("verify mandatory source SHA-256 sidecars");
    }
    if authorized.source_ref() != source_ref {
        record_terminal(&failed_acquisition())?;
        bail!("filesystem acquisition capability source mismatch");
    }
    if let Some(expected) = request.expected_digest.as_deref()
        && expected != observed_digest
    {
        let acquisition =
            rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
        record_rejected(&acquisition)?;
        bail!("expected BLAKE3 digest does not match acquired component bytes");
    }

    let manifest_text = match std::str::from_utf8(&manifest_bytes) {
        Ok(text) => text,
        Err(error) => {
            let acquisition =
                rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
            record_rejected(&acquisition)?;
            return Err(error).context("child manifest is not UTF-8");
        }
    };
    let parsed = match SdkChildManifest::from_toml_str(manifest_text) {
        Ok(parsed) => parsed,
        Err(error) => {
            let acquisition =
                rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
            record_rejected(&acquisition)?;
            return Err(error).context("parse staged child manifest");
        }
    };
    if parsed.name != request.claimed_child_name
        || parsed.version != request.claimed_artifact_version
    {
        let acquisition =
            rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
        record_rejected(&acquisition)?;
        bail!("staged manifest identity does not match operator claim");
    }
    authority_request.namespaces = match manifest_namespaces(&parsed) {
        Ok(namespaces) => namespaces,
        Err(error) => {
            let acquisition =
                rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
            record_rejected(&acquisition)?;
            return Err(error);
        }
    };
    if standing.is_some()
        && evaluate_artifact_acquisition_authority(
            &authority_request,
            standing.as_ref(),
            None,
            Some(&effect),
        )
        .authorized
        .is_none()
    {
        let acquisition =
            rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
        record_rejected(&acquisition)?;
        bail!("staged manifest namespace is outside standing source scope");
    }

    let (emitted_manifest, artifact_relative) =
        match canonical_package_manifest(manifest_text, &parsed, &selected_component_path) {
            Ok(package) => package,
            Err(error) => {
                let acquisition =
                    rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
                record_rejected(&acquisition)?;
                return Err(error);
            }
        };
    let staging_dir = request
        .children_dir
        .join(".acquiring")
        .join(acquisition_id.as_str().replace(':', "-"));
    if staging_dir.exists() {
        let acquisition =
            rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
        record_rejected(&acquisition)?;
        bail!("artifact acquisition staging path already exists");
    }
    let staged_component = staging_dir.join(&artifact_relative);
    let staged_manifest = staging_dir.join("child.toml");
    let stage_result = (|| -> Result<()> {
        fs::create_dir_all(
            staged_component
                .parent()
                .context("staged component path has no parent")?,
        )?;
        write_new_file(&staged_manifest, emitted_manifest.as_bytes())?;
        write_new_file(&staged_component, &component_bytes)?;
        write_sha256_sidecar(&staged_manifest)?;
        write_sha256_sidecar(&staged_component)?;
        Ok(())
    })();
    if let Err(error) = stage_result {
        let acquisition =
            rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
        record_rejected(&acquisition)?;
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(error).context("render canonical staged artifact package");
    }

    let load = load_children_from_dir(MctChildLoadOptions::new(&staging_dir).strict_integrity());
    if load.loaded != 1 || load.failed != 0 {
        let acquisition =
            rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
        record_rejected(&acquisition)?;
        let _ = fs::remove_dir_all(&staging_dir);
        bail!("canonical staged package did not pass strict verification");
    }
    let loaded = load
        .children
        .into_iter()
        .next()
        .expect("loaded count checked");
    let artifact_id = loaded.artifact_id.clone();
    let verified_without_artifact = || ArtifactAcquisition {
        acquisition_id: acquisition_id.clone(),
        authority_path,
        standing_source_authority_id: standing_source_authority_id.clone(),
        operator_pointed_decision_id: operator_pointed_decision_id.clone(),
        adapter_effect_authority_ref: context.adapter_start_observation_id.to_string(),
        source_ref: source_ref.clone(),
        claimed_child_name: request.claimed_child_name.clone(),
        claimed_artifact_version: request.claimed_artifact_version.clone(),
        observed_size_bytes: Some(component_bytes.len() as u64),
        observed_digest: Some(observed_digest.clone()),
        acquisition_outcome: ArtifactAcquisitionOutcome::Acquired,
        verification_outcome: ArtifactVerificationOutcome::Verified,
        verification_observation_id: Some(verification_observation_id.clone()),
        acquisition_observation_id: acquisition_observation_id.clone(),
        component_artifact_id: None,
    };
    let digest_hex = match artifact_id.strip_prefix("sha256:") {
        Some(digest) => digest,
        None => {
            record_terminal(&verified_without_artifact())?;
            let _ = fs::remove_dir_all(&staging_dir);
            bail!("loaded artifact id is not SHA-256 tagged");
        }
    };
    let package_path = request
        .children_dir
        .join("artifacts")
        .join("sha256")
        .join(digest_hex);
    let package_preexisting = package_path.exists();
    if package_preexisting {
        let existing =
            load_children_from_dir(MctChildLoadOptions::new(&package_path).strict_integrity());
        if existing.loaded != 1 || existing.children[0].artifact_id != artifact_id {
            record_terminal(&verified_without_artifact())?;
            let _ = fs::remove_dir_all(&staging_dir);
            bail!("immutable artifact catalog path conflicts with different bytes");
        }
    }
    let mut artifact = component_artifact_from_loaded_child(&loaded);
    artifact.provenance_status = ArtifactProvenanceStatus::AcquisitionBacked;
    artifact.acquisition_ids = vec![acquisition_id.clone()];
    artifact.created_by_observation_id = verification_observation_id.clone();
    let acquisition = ArtifactAcquisition {
        acquisition_id: acquisition_id.clone(),
        authority_path,
        standing_source_authority_id: standing_source_authority_id.clone(),
        operator_pointed_decision_id: operator_pointed_decision_id.clone(),
        adapter_effect_authority_ref: context.adapter_start_observation_id.to_string(),
        source_ref: source_ref.clone(),
        claimed_child_name: request.claimed_child_name.clone(),
        claimed_artifact_version: request.claimed_artifact_version.clone(),
        observed_size_bytes: Some(component_bytes.len() as u64),
        observed_digest: Some(observed_digest.clone()),
        acquisition_outcome: ArtifactAcquisitionOutcome::Acquired,
        verification_outcome: ArtifactVerificationOutcome::Verified,
        verification_observation_id: Some(verification_observation_id.clone()),
        acquisition_observation_id: acquisition_observation_id.clone(),
        component_artifact_id: Some(artifact.artifact_id.clone()),
    };
    let report = MctArtifactAcquisitionReport {
        acquisition_id: acquisition_id.clone(),
        child_name: request.claimed_child_name.clone(),
        artifact_version: request.claimed_artifact_version.clone(),
        artifact_id: Some(artifact_id.clone()),
        observed_size_bytes: Some(component_bytes.len() as u64),
        observed_digest: Some(observed_digest.clone()),
        acquisition_outcome: "acquired".into(),
        verification_outcome: "verified".into(),
        package_path: Some(package_path.clone()),
        acquisition_observation_id: acquisition_observation_id.clone(),
        verification_observation_id: Some(verification_observation_id.clone()),
    };
    if let Err(error) = observe_verified(&report) {
        let _ = fs::remove_dir_all(&staging_dir);
        if let Some(decision_id) = operator_pointed_decision_id.as_ref() {
            let _ = state.consume_operator_acquisition_decision(decision_id);
        }
        return Err(error).context("artifact terminal facts were not durable before publication");
    }
    if package_preexisting {
        let existing =
            load_children_from_dir(MctChildLoadOptions::new(&package_path).strict_integrity());
        if existing.loaded != 1 || existing.children[0].artifact_id != artifact_id {
            record_terminal(&verified_without_artifact())?;
            let _ = fs::remove_dir_all(&staging_dir);
            bail!("immutable artifact catalog path conflicts with different bytes");
        }
        if let Err(error) = fs::remove_dir_all(&staging_dir) {
            record_terminal(&verified_without_artifact())?;
            return Err(error).context("remove redundant reacquisition staging package");
        }
    } else {
        let publish = (|| -> Result<()> {
            fs::create_dir_all(
                package_path
                    .parent()
                    .context("catalog path has no parent")?,
            )?;
            fs::rename(&staging_dir, &package_path)
                .context("publish immutable artifact package")?;
            Ok(())
        })();
        if let Err(error) = publish {
            record_terminal(&verified_without_artifact())?;
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(error);
        }
    }

    if let Err(error) = MctRuntimeStateStore::open(&request.state_path)?
        .record_verified_acquisition_and_artifact(&acquisition, &artifact, &package_path)
    {
        if !package_preexisting {
            let _ = fs::remove_dir_all(&package_path);
        }
        record_terminal(&verified_without_artifact())?;
        return Err(error).context("publish verified artifact catalog projection");
    }
    if let Some(decision_id) = operator_pointed_decision_id.as_ref() {
        MctRuntimeStateStore::open(&request.state_path)?
            .consume_operator_acquisition_decision(decision_id)?;
    }

    Ok(report)
}

fn manifest_namespaces(manifest: &SdkChildManifest) -> Result<Vec<String>> {
    let mut namespaces = BTreeSet::new();
    for operation in &manifest.contract.allow_operations {
        let namespace = operation
            .split_once('/')
            .map(|(namespace, _)| namespace)
            .filter(|namespace| namespace.contains(':') && !namespace.trim().is_empty())
            .context("manifest contains malformed WIT operation namespace")?;
        namespaces.insert(namespace.to_string());
    }
    if namespaces.is_empty() {
        bail!("manifest contains no WIT operation namespace");
    }
    Ok(namespaces.into_iter().collect())
}

fn canonical_package_manifest(
    source: &str,
    manifest: &SdkChildManifest,
    selected_component: &Path,
) -> Result<(String, PathBuf)> {
    let artifact_relative = if let Some(declared) = manifest.artifact.wasm.as_ref() {
        validate_relative_path(declared)?;
        if declared != selected_component {
            bail!("selected component does not match manifest artifact declaration");
        }
        declared.clone()
    } else {
        let file_name = selected_component
            .file_name()
            .context("selected component has no file name")?;
        PathBuf::from("artifact").join(file_name)
    };
    let mut value: toml::Value = toml::from_str(source).context("parse manifest for staging")?;
    let child = value
        .get_mut("child")
        .and_then(toml::Value::as_table_mut)
        .context("manifest child section is missing")?;
    let mut artifact = toml::map::Map::new();
    artifact.insert(
        "wasm".into(),
        toml::Value::String(
            artifact_relative
                .to_str()
                .context("artifact path is not UTF-8")?
                .into(),
        ),
    );
    child.insert("artifact".into(), toml::Value::Table(artifact));
    Ok((toml::to_string(&value)?, artifact_relative))
}

fn canonical_source_file(root: &Path, relative: &Path) -> Result<PathBuf> {
    let path = root
        .join(relative)
        .canonicalize()
        .with_context(|| format!("resolve artifact source file {}", relative.display()))?;
    if !path.starts_with(root) || !path.is_file() {
        bail!("artifact source file escapes root or is not a regular file");
    }
    Ok(path)
}

fn validate_relative_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("artifact package path must be non-empty and relative");
    }
    Ok(())
}

fn read_bounded_file(path: &Path, max: usize) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(max as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max {
        bail!("artifact source file exceeds named size bound");
    }
    Ok(bytes)
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn write_sha256_sidecar(path: &Path) -> Result<()> {
    let bytes = fs::read(path)?;
    write_new_file(
        &hash_sidecar_path(path),
        format!("{:x}", Sha256::digest(bytes)).as_bytes(),
    )
}

fn verify_source_sidecar(path: &Path, bytes: &[u8]) -> Result<()> {
    let sidecar = hash_sidecar_path(path);
    let expected = fs::read_to_string(&sidecar)
        .with_context(|| format!("read required source sidecar {}", sidecar.display()))?;
    if expected.trim() != format!("{:x}", Sha256::digest(bytes)) {
        bail!("source SHA-256 sidecar does not match acquired bytes");
    }
    Ok(())
}

fn hash_sidecar_path(path: &Path) -> PathBuf {
    let mut sidecar: OsString = path.as_os_str().to_os_string();
    sidecar.push(".sha256");
    PathBuf::from(sidecar)
}

fn validate_expected_digest(value: Option<&str>) -> Result<()> {
    let Some(value) = value else { return Ok(()) };
    let Some(hex) = value.strip_prefix("blake3:") else {
        bail!("expected acquisition digest must be BLAKE3 tagged");
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("expected acquisition digest must be lowercase 64-character BLAKE3 hex");
    }
    Ok(())
}

fn validate_claim(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{label} must not be empty");
    }
    Ok(())
}

fn current_uid() -> Result<u32> {
    let output = Command::new("/usr/bin/id").arg("-u").output()?;
    if !output.status.success() {
        bail!("authenticate current OS UID: id -u failed");
    }
    Ok(String::from_utf8(output.stdout)?.trim().parse()?)
}

fn timestamp_after_one_minute() -> Result<Timestamp> {
    let now = jiff::Timestamp::now();
    Ok(Timestamp::new(
        (now + jiff::SignedDuration::from_mins(1)).to_string(),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_request(root: &Path, expected_digest: Option<String>) -> MctArtifactStageRequest {
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("child.toml"),
            r#"[child]
name = "fixture"
version = "1.0.0"
kind = "child"
[child.ingress]
mode = "wit-only"
[child.contract]
allow = ["patina:fixture/control@0.1.0.run"]
"#,
        )
        .unwrap();
        fs::write(source.join("fixture.wasm"), b"real-attempt-bytes").unwrap();
        MctArtifactStageRequest {
            source_root: source,
            manifest_path: PathBuf::from("child.toml"),
            component_path: PathBuf::from("fixture.wasm"),
            claimed_child_name: "fixture".into(),
            claimed_artifact_version: "1.0.0".into(),
            expected_digest,
            standing_source_authority_id: None,
            claimed_publisher: None,
            require_source_sidecars: false,
            children_dir: root.join("children"),
            state_path: root.join("state.sqlite"),
        }
    }

    fn standing_source(
        request: &MctArtifactStageRequest,
        source_id: &str,
        observation_id: &str,
    ) -> ArtifactSourceAuthority {
        ArtifactSourceAuthority {
            source_authority_id: mct_kernel::ArtifactSourceAuthorityId::new(source_id).unwrap(),
            source_ref: format!(
                "file://{}",
                request.source_root.canonicalize().unwrap().display()
            ),
            scope: mct_kernel::ArtifactSourceScope {
                scope_mode: mct_kernel::ArtifactSourceScopeMode::Constrained,
                artifact_scope: vec!["fixture@1.0.0".into()],
                publisher_scope: vec!["fixture-publisher".into()],
                namespace_scope: vec!["patina:fixture".into()],
                allowed_actions: vec!["acquire".into()],
            },
            integrity_policy_ref: "sha256-sidecars-v1".into(),
            provenance_policy_ref: None,
            issuer_principal_ref: "os-uid:501".into(),
            policy_revision: 1,
            authority_state: ArtifactSourceAuthorityState::Active,
            issued_at: Timestamp::new("2026-07-21T00:00:00Z").unwrap(),
            expires_at: Timestamp::new("2099-01-01T00:00:00Z").unwrap(),
            authority_observation_id: ObservationId::new(observation_id).unwrap(),
        }
    }

    fn write_standing_source_observation(
        source: &ArtifactSourceAuthority,
        ledger_path: &Path,
        safe_message: String,
    ) {
        use mct_kernel::{MctObservation, ObservationTraceRef, ObservationVisibility, TraceId};

        let mut ledger =
            JsonlObservationLedger::open(ledger_path, "ledger-local", "local-mct").unwrap();
        ledger
            .append_batch_before_effect(
                [MctObservation {
                    observation_id: source.authority_observation_id.clone(),
                    observed_at: source.issued_at.clone(),
                    kind: ObservationKind::OperatorActionRecorded,
                    source_plane: SourcePlane::Operator,
                    trace: ObservationTraceRef {
                        trace_id: TraceId::new(format!("trace:{}", source.source_authority_id))
                            .unwrap(),
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
                    outcome: ObservationOutcome::Allowed,
                    visibility: ObservationVisibility::NodeOperator,
                    safe_message,
                    detail_ref: None,
                }],
                current_timestamp_string(),
            )
            .unwrap();
    }

    #[test]
    fn standing_source_projection_without_ledger_fact_grants_nothing() {
        let root = tempfile::tempdir().unwrap();
        let request = raw_request(root.path(), None);
        let source = standing_source(&request, "source-unobserved", "obs-source-unobserved");
        let digest = blake3::hash(&serde_json::to_vec(&source).unwrap())
            .to_hex()
            .to_string();
        MctRuntimeStateStore::open(&request.state_path)
            .unwrap()
            .upsert_source_authority(&source, &digest)
            .unwrap();
        let ledger_path = root.path().join("observations.jsonl");
        drop(JsonlObservationLedger::open(&ledger_path, "ledger-local", "local-mct").unwrap());

        let error = verify_standing_source_ledger_correlation(
            &request.state_path,
            &ledger_path,
            source.source_authority_id.as_str(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("ledger"));

        let mut standing_request = request;
        standing_request.source_root = root.path().join("source-must-not-be-read");
        standing_request.standing_source_authority_id =
            Some(source.source_authority_id.to_string());
        standing_request.claimed_publisher = Some("fixture-publisher".into());
        let stage_error = stage_artifact_with_context(
            &standing_request,
            &new_artifact_attempt_context().unwrap(),
        )
        .unwrap_err();
        assert!(stage_error.to_string().contains("validated ledger proof"));
    }

    #[test]
    fn standing_source_ledger_proof_is_required_to_mint_acquisition_capability() {
        let root = tempfile::tempdir().unwrap();
        let mut request = raw_request(root.path(), None);
        let source = standing_source(&request, "source-observed", "obs-source-observed");
        let digest = blake3::hash(&serde_json::to_vec(&source).unwrap())
            .to_hex()
            .to_string();
        MctRuntimeStateStore::open(&request.state_path)
            .unwrap()
            .upsert_source_authority(&source, &digest)
            .unwrap();
        let ledger_path = root.path().join("observations.jsonl");
        write_standing_source_observation(
            &source,
            &ledger_path,
            format!("artifact source authority created digest={digest}"),
        );

        let proof = verify_standing_source_ledger_correlation(
            &request.state_path,
            &ledger_path,
            source.source_authority_id.as_str(),
        )
        .unwrap();
        request.standing_source_authority_id = Some(source.source_authority_id.to_string());
        request.claimed_publisher = Some("fixture-publisher".into());
        let report = stage_artifact_with_context_and_observer(
            &request,
            &new_artifact_attempt_context().unwrap(),
            Some(&proof),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(report.acquisition_outcome, "acquired");
        assert!(report.artifact_id.is_some());
    }

    #[test]
    fn standing_source_ledger_proof_rejects_mismatched_and_hash_invalid_facts() {
        let root = tempfile::tempdir().unwrap();
        let request = raw_request(root.path(), None);
        let source = standing_source(&request, "source-mismatch", "obs-source-mismatch");
        let digest = blake3::hash(&serde_json::to_vec(&source).unwrap())
            .to_hex()
            .to_string();
        MctRuntimeStateStore::open(&request.state_path)
            .unwrap()
            .upsert_source_authority(&source, &digest)
            .unwrap();
        let ledger_path = root.path().join("observations.jsonl");
        write_standing_source_observation(
            &source,
            &ledger_path,
            format!(
                "artifact source authority created digest={}",
                "0".repeat(64)
            ),
        );

        let mismatch = verify_standing_source_ledger_correlation(
            &request.state_path,
            &ledger_path,
            source.source_authority_id.as_str(),
        )
        .unwrap_err();
        assert!(mismatch.to_string().contains("mismatch"));

        let tampered = fs::read_to_string(&ledger_path)
            .unwrap()
            .replacen("created", "creaxed", 1);
        fs::write(&ledger_path, tampered).unwrap();
        assert!(
            verify_standing_source_ledger_correlation(
                &request.state_path,
                &ledger_path,
                source.source_authority_id.as_str(),
            )
            .is_err()
        );
    }

    #[test]
    fn artifact_acquisition_failures_are_observed_without_artifact_publication() {
        let root = tempfile::tempdir().unwrap();
        let request = raw_request(root.path(), Some(format!("blake3:{}", "0".repeat(64))));
        let result = stage_operator_pointed_artifact(&request);
        assert!(result.is_err());
        let state = MctRuntimeStateStore::open(&request.state_path).unwrap();
        let attempts = state.artifact_acquisitions().unwrap();
        assert_eq!(attempts.len(), 1);
        assert_eq!(
            attempts[0].verification_outcome,
            ArtifactVerificationOutcome::Rejected
        );
        assert!(attempts[0].component_artifact_id.is_none());
        assert_eq!(state.summary().unwrap().artifacts, 0);
        let decisions = state.operator_acquisition_decisions().unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(
            decisions[0].decision_state,
            OperatorPointedAcquisitionState::Consumed
        );
    }

    #[test]
    fn malformed_tampered_oversize_and_escaping_sources_leave_attempt_evidence_only() {
        let run_case = |mutate: fn(&Path, &mut MctArtifactStageRequest)| {
            let root = tempfile::tempdir().unwrap();
            let mut request = raw_request(root.path(), None);
            mutate(root.path(), &mut request);
            assert!(stage_operator_pointed_artifact(&request).is_err());
            let state = MctRuntimeStateStore::open(&request.state_path).unwrap();
            let attempts = state.artifact_acquisitions().unwrap();
            assert_eq!(attempts.len(), 1);
            assert!(attempts[0].component_artifact_id.is_none());
            assert_eq!(state.summary().unwrap().artifacts, 0);
            assert_eq!(
                state.operator_acquisition_decisions().unwrap()[0].decision_state,
                OperatorPointedAcquisitionState::Consumed
            );
        };

        run_case(|_, request| request.require_source_sidecars = true);
        run_case(|_, request| {
            request.require_source_sidecars = true;
            fs::write(
                hash_sidecar_path(&request.source_root.join("child.toml")),
                "0".repeat(64),
            )
            .unwrap();
            fs::write(
                hash_sidecar_path(&request.source_root.join("fixture.wasm")),
                "0".repeat(64),
            )
            .unwrap();
        });
        run_case(|_, request| request.claimed_artifact_version = "2.0.0".into());
        run_case(|_, request| {
            fs::write(
                request.source_root.join("child.toml"),
                r#"[child]
name = "fixture"
version = "1.0.0"
kind = "child"
[child.ingress]
mode = "wit-only"
[child.contract]
allow = ["malformed-operation"]
"#,
            )
            .unwrap();
        });
        run_case(|_, request| {
            fs::OpenOptions::new()
                .write(true)
                .open(request.source_root.join("child.toml"))
                .unwrap()
                .set_len(MCT_CHILD_MANIFEST_MAX_BYTES as u64 + 1)
                .unwrap();
        });
        run_case(|_, request| {
            fs::OpenOptions::new()
                .write(true)
                .open(request.source_root.join("fixture.wasm"))
                .unwrap()
                .set_len(MCT_COMPONENT_ARTIFACT_MAX_BYTES as u64 + 1)
                .unwrap();
        });
        #[cfg(unix)]
        run_case(|root, request| {
            use std::os::unix::fs::symlink;
            let outside = root.join("outside.wasm");
            fs::write(&outside, b"outside").unwrap();
            fs::remove_file(request.source_root.join("fixture.wasm")).unwrap();
            symlink(outside, request.source_root.join("fixture.wasm")).unwrap();
        });
    }

    #[test]
    fn identical_reacquisition_adds_evidence_without_replacing_immutable_artifact() {
        let root = tempfile::tempdir().unwrap();
        let request = raw_request(root.path(), None);
        let first = stage_operator_pointed_artifact(&request).unwrap();
        let package = first.package_path.clone().unwrap();
        let package_manifest = fs::read(package.join("child.toml")).unwrap();
        let second = stage_operator_pointed_artifact(&request).unwrap();
        assert_eq!(first.artifact_id, second.artifact_id);
        assert_ne!(first.acquisition_id, second.acquisition_id);
        assert_eq!(
            fs::read(package.join("child.toml")).unwrap(),
            package_manifest
        );
        let state = MctRuntimeStateStore::open(&request.state_path).unwrap();
        assert_eq!(state.summary().unwrap().artifacts, 1);
        assert_eq!(state.artifact_acquisitions().unwrap().len(), 2);
        assert_eq!(state.operator_acquisition_decisions().unwrap().len(), 2);
        assert!(
            state
                .operator_acquisition_decisions()
                .unwrap()
                .iter()
                .all(
                    |decision| decision.decision_state == OperatorPointedAcquisitionState::Consumed
                )
        );
    }

    #[test]
    fn package_shaped_acquisition_discovers_declared_component_only_after_authority_start() {
        let first_root = tempfile::tempdir().unwrap();
        let first_request = raw_request(first_root.path(), None);
        let package = stage_operator_pointed_artifact(&first_request)
            .unwrap()
            .package_path
            .unwrap();
        let second_root = tempfile::tempdir().unwrap();
        let request = MctArtifactStageRequest {
            source_root: package,
            manifest_path: PathBuf::from("child.toml"),
            component_path: PathBuf::new(),
            claimed_child_name: "fixture".into(),
            claimed_artifact_version: "1.0.0".into(),
            expected_digest: None,
            standing_source_authority_id: None,
            claimed_publisher: None,
            require_source_sidecars: true,
            children_dir: second_root.path().join("children"),
            state_path: second_root.path().join("state.sqlite"),
        };
        let report = stage_operator_pointed_artifact(&request).unwrap();
        assert_eq!(report.verification_outcome, "verified");
        assert_eq!(
            MctRuntimeStateStore::open(&request.state_path)
                .unwrap()
                .summary()
                .unwrap()
                .artifacts,
            1
        );
    }

    #[test]
    fn same_digest_different_manifest_fact_cannot_replace_catalog_artifact() {
        let root = tempfile::tempdir().unwrap();
        let request = raw_request(root.path(), None);
        let first = stage_operator_pointed_artifact(&request).unwrap();
        let package = first.package_path.unwrap();
        let persisted_manifest = fs::read(package.join("child.toml")).unwrap();
        let source_manifest = request.source_root.join("child.toml");
        let changed = fs::read_to_string(&source_manifest).unwrap().replace(
            "kind = \"child\"",
            "kind = \"child\"\ndescription = \"changed\"",
        );
        fs::write(source_manifest, changed).unwrap();
        assert!(stage_operator_pointed_artifact(&request).is_err());
        assert_eq!(
            fs::read(package.join("child.toml")).unwrap(),
            persisted_manifest
        );
        let state = MctRuntimeStateStore::open(&request.state_path).unwrap();
        assert_eq!(state.summary().unwrap().artifacts, 1);
        let attempts = state.artifact_acquisitions().unwrap();
        assert_eq!(attempts.len(), 2);
        assert!(attempts[1].component_artifact_id.is_none());
        assert_eq!(
            attempts[1].verification_outcome,
            ArtifactVerificationOutcome::Verified
        );
    }

    #[test]
    fn staged_package_reconciles_sha256_floor_with_blake3_acquisition_evidence() {
        let root = tempfile::tempdir().unwrap();
        let bytes = b"real-attempt-bytes";
        let expected = format!("blake3:{}", blake3::hash(bytes).to_hex());
        let request = raw_request(root.path(), Some(expected.clone()));
        let report = stage_operator_pointed_artifact(&request).unwrap();
        assert_eq!(report.observed_digest.as_deref(), Some(expected.as_str()));
        let package = report.package_path.unwrap();
        let manifest = package.join("child.toml");
        let component = package.join("artifact/fixture.wasm");
        assert_eq!(
            fs::read_to_string(hash_sidecar_path(&manifest)).unwrap(),
            format!("{:x}", Sha256::digest(fs::read(&manifest).unwrap()))
        );
        assert_eq!(
            fs::read_to_string(hash_sidecar_path(&component)).unwrap(),
            format!("{:x}", Sha256::digest(bytes))
        );
    }
}

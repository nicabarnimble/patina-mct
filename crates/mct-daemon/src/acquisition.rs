use crate::{
    MctChildLoadOptions, MctRuntimeStateStore, component_artifact_from_loaded_child,
    current_timestamp, current_timestamp_string, load_children_from_dir,
};
use anyhow::{Context, Result, bail};
use mct_kernel::{
    ArtifactAcquisition, ArtifactAcquisitionAuthorityPath, ArtifactAcquisitionAuthorityRequest,
    ArtifactAcquisitionDecisionId, ArtifactAcquisitionId, ArtifactAcquisitionOutcome,
    ArtifactProvenanceStatus, ArtifactVerificationOutcome, AuthorizedArtifactAcquisitionId,
    FilesystemAcquisitionEffectAuthority, ObservationId, OperatorPointedAcquisitionState,
    OperatorPointedArtifactAcquisitionDecision, Timestamp, evaluate_artifact_acquisition_authority,
};
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
    validate_claim(&request.claimed_child_name, "child name")?;
    validate_claim(&request.claimed_artifact_version, "artifact version")?;
    validate_expected_digest(request.expected_digest.as_deref())?;

    let source_root = request
        .source_root
        .canonicalize()
        .with_context(|| format!("resolve source root {}", request.source_root.display()))?;
    if !source_root.is_dir() {
        bail!("artifact source root is not a directory");
    }
    validate_relative_path(&request.manifest_path)?;
    validate_relative_path(&request.component_path)?;
    let manifest_path = canonical_source_file(&source_root, &request.manifest_path)?;
    let component_path = canonical_source_file(&source_root, &request.component_path)?;
    let state = MctRuntimeStateStore::open(&request.state_path)?;
    let standing = request
        .standing_source_authority_id
        .as_deref()
        .map(|source_id| {
            let (source, record_digest) = state
                .source_authorities()?
                .into_iter()
                .find(|(source, _)| source.source_authority_id.as_str() == source_id)
                .context("standing artifact source authority not found")?;
            let expected_digest = blake3::hash(&serde_json::to_vec(&source)?)
                .to_hex()
                .to_string();
            if record_digest != expected_digest {
                bail!("standing artifact source authority record digest mismatch");
            }
            Ok(source)
        })
        .transpose()?;
    if let Some(source) = standing.as_ref() {
        let root = source
            .source_ref
            .strip_prefix("file://")
            .context("standing source is not a filesystem source")?;
        let authority_root = PathBuf::from(root).canonicalize()?;
        if !source_root.starts_with(&authority_root) {
            bail!("acquisition package is outside standing source root");
        }
    }
    let source_ref = standing.as_ref().map_or_else(
        || format!("file://{}", source_root.display()),
        |source| source.source_ref.clone(),
    );

    let sequence = NEXT_ACQUISITION_ID.fetch_add(1, Ordering::SeqCst);
    let id_suffix = format!("{}-{sequence}", current_timestamp_string());
    let acquisition_id = ArtifactAcquisitionId::new(format!("acquisition:{id_suffix}"))?;
    let decision_id = ArtifactAcquisitionDecisionId::new(format!("decision:{id_suffix}"))?;
    let generated_authority_observation_id =
        ObservationId::new(format!("obs:acquisition-authority:{id_suffix}"))?;
    let authority_observation_id = standing.as_ref().map_or_else(
        || generated_authority_observation_id.clone(),
        |source| source.authority_observation_id.clone(),
    );
    let acquisition_observation_id = ObservationId::new(format!("obs:acquisition:{id_suffix}"))?;
    let verification_observation_id =
        ObservationId::new(format!("obs:artifact-verification:{id_suffix}"))?;
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
    let effect = FilesystemAcquisitionEffectAuthority {
        authority_ref: authority_observation_id.clone(),
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
        authorized_id: AuthorizedArtifactAcquisitionId::new(format!("authorized:{id_suffix}"))?,
    };
    let authority = evaluate_artifact_acquisition_authority(
        &authority_request,
        standing.as_ref(),
        operator.as_ref(),
        Some(&effect),
    );
    let authorized = authority
        .authorized
        .context("source trust and filesystem effect authority denied")?;
    let authority_path = if standing.is_some() {
        ArtifactAcquisitionAuthorityPath::StandingSource
    } else {
        ArtifactAcquisitionAuthorityPath::OperatorPointed
    };
    let standing_source_authority_id = standing
        .as_ref()
        .map(|source| source.source_authority_id.clone());
    let operator_pointed_decision_id = operator.as_ref().map(|value| value.decision_id.clone());
    let rejected_acquisition = |size: u64, digest: String| ArtifactAcquisition {
        acquisition_id: acquisition_id.clone(),
        authority_path,
        standing_source_authority_id: standing_source_authority_id.clone(),
        operator_pointed_decision_id: operator_pointed_decision_id.clone(),
        adapter_effect_authority_ref: authority_observation_id.to_string(),
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

    let manifest_bytes = read_bounded_file(&manifest_path, MCT_CHILD_MANIFEST_MAX_BYTES)?;
    let component_bytes = read_bounded_file(&component_path, MCT_COMPONENT_ARTIFACT_MAX_BYTES)?;
    if request.require_source_sidecars {
        verify_source_sidecar(&manifest_path, &manifest_bytes)?;
        verify_source_sidecar(&component_path, &component_bytes)?;
    }
    if authorized.source_ref() != source_ref {
        bail!("filesystem acquisition capability source mismatch");
    }
    let observed_digest = format!("blake3:{}", blake3::hash(&component_bytes).to_hex());
    if let Some(expected) = request.expected_digest.as_deref()
        && expected != observed_digest
    {
        let acquisition =
            rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
        MctRuntimeStateStore::open(&request.state_path)?
            .record_artifact_acquisition(&acquisition)?;
        bail!("expected BLAKE3 digest does not match acquired component bytes");
    }

    let manifest_text =
        std::str::from_utf8(&manifest_bytes).context("child manifest is not UTF-8")?;
    let parsed =
        SdkChildManifest::from_toml_str(manifest_text).context("parse staged child manifest")?;
    if parsed.name != request.claimed_child_name
        || parsed.version != request.claimed_artifact_version
    {
        let acquisition =
            rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
        MctRuntimeStateStore::open(&request.state_path)?
            .record_artifact_acquisition(&acquisition)?;
        bail!("staged manifest identity does not match operator claim");
    }
    authority_request.namespaces = manifest_namespaces(&parsed)?;
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
        MctRuntimeStateStore::open(&request.state_path)?
            .record_artifact_acquisition(&acquisition)?;
        bail!("staged manifest namespace is outside standing source scope");
    }

    let (emitted_manifest, artifact_relative) =
        canonical_package_manifest(manifest_text, &parsed, &request.component_path)?;
    let staging_dir = request
        .children_dir
        .join(".acquiring")
        .join(acquisition_id.as_str().replace(':', "-"));
    if staging_dir.exists() {
        bail!("artifact acquisition staging path already exists");
    }
    let staged_component = staging_dir.join(&artifact_relative);
    fs::create_dir_all(
        staged_component
            .parent()
            .context("staged component path has no parent")?,
    )?;
    let staged_manifest = staging_dir.join("child.toml");
    write_new_file(&staged_manifest, emitted_manifest.as_bytes())?;
    write_new_file(&staged_component, &component_bytes)?;
    write_sha256_sidecar(&staged_manifest)?;
    write_sha256_sidecar(&staged_component)?;

    let load = load_children_from_dir(MctChildLoadOptions::new(&staging_dir).strict_integrity());
    if load.loaded != 1 || load.failed != 0 {
        let acquisition =
            rejected_acquisition(component_bytes.len() as u64, observed_digest.clone());
        MctRuntimeStateStore::open(&request.state_path)?
            .record_artifact_acquisition(&acquisition)?;
        let _ = fs::remove_dir_all(&staging_dir);
        bail!("canonical staged package did not pass strict verification");
    }
    let loaded = load
        .children
        .into_iter()
        .next()
        .expect("loaded count checked");
    let artifact_id = loaded.artifact_id.clone();
    let digest_hex = artifact_id
        .strip_prefix("sha256:")
        .context("loaded artifact id is not SHA-256 tagged")?;
    let package_path = request
        .children_dir
        .join("artifacts")
        .join("sha256")
        .join(digest_hex);
    if package_path.exists() {
        let existing =
            load_children_from_dir(MctChildLoadOptions::new(&package_path).strict_integrity());
        if existing.loaded != 1 || existing.children[0].artifact_id != artifact_id {
            bail!("immutable artifact catalog path conflicts with different bytes");
        }
        fs::remove_dir_all(&staging_dir)?;
    } else {
        fs::create_dir_all(
            package_path
                .parent()
                .context("catalog path has no parent")?,
        )?;
        fs::rename(&staging_dir, &package_path).context("publish immutable artifact package")?;
    }

    let mut artifact = component_artifact_from_loaded_child(&loaded);
    artifact.provenance_status = ArtifactProvenanceStatus::AcquisitionBacked;
    artifact.acquisition_ids = vec![acquisition_id.clone()];
    artifact.created_by_observation_id = verification_observation_id.clone();
    let acquisition = ArtifactAcquisition {
        acquisition_id: acquisition_id.clone(),
        authority_path,
        standing_source_authority_id,
        operator_pointed_decision_id,
        adapter_effect_authority_ref: authority_observation_id.to_string(),
        source_ref,
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
    MctRuntimeStateStore::open(&request.state_path)?.record_verified_acquisition_and_artifact(
        &acquisition,
        &artifact,
        &package_path,
    )?;

    Ok(MctArtifactAcquisitionReport {
        acquisition_id,
        child_name: request.claimed_child_name.clone(),
        artifact_version: request.claimed_artifact_version.clone(),
        artifact_id: Some(artifact_id),
        observed_size_bytes: Some(component_bytes.len() as u64),
        observed_digest: Some(observed_digest),
        acquisition_outcome: "acquired".into(),
        verification_outcome: "verified".into(),
        package_path: Some(package_path),
        acquisition_observation_id,
        verification_observation_id: Some(verification_observation_id),
    })
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

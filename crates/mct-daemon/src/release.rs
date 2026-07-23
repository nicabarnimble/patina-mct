use crate::{MctRuntimeStateStore, current_timestamp, current_timestamp_string};
use anyhow::{Context as _, Result, bail};
use flate2::read::GzDecoder;
use mct_kernel::{
    DecisionId, MctObservation, ObservationId, ObservationKind, ObservationOutcome,
    ObservationTraceRef, ObservationVisibility, SourcePlane, TraceId,
};
use mct_observation::{DurabilityClass, ExportStatus, JsonlObservationLedger};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write as _},
    os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

pub const MCT_DAEMON_RELEASE_ARCHIVE_MAX_BYTES: u64 = 256 * 1024 * 1024;
pub const MCT_DAEMON_RELEASE_EXTRACTED_MAX_BYTES: u64 = 512 * 1024 * 1024;
pub const MCT_DAEMON_RELEASE_MAX_ENTRIES: usize = 32;
pub const MCT_DAEMON_RELEASE_METADATA_FILE_MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const MCT_DAEMON_RELEASE_ACQUISITION_DEADLINE_SECONDS: i64 = 60;
pub const MCT_DAEMON_RELEASE_FILESYSTEM_ADAPTER: &str =
    "mct:daemon-release-acquisition/filesystem@1";

static NEXT_DAEMON_RELEASE_ATTEMPT: AtomicU64 = AtomicU64::new(1);

const RELEASE_MANIFEST_FILE: &str = "RELEASE-MANIFEST.json";
const RELEASE_NOTES_FILE: &str = "RELEASE-NOTES.md";
const RELEASE_SBOM_FILE: &str = "SBOM.cdx.json";
const RELEASE_FIXTURE_PROVENANCE_FILE: &str = "FIXTURE-PROVENANCE.json";
const RELEASE_LICENSE_FILE: &str = "LICENSE";
const RELEASE_CHECKSUMS_FILE: &str = "CHECKSUMS";
const RELEASE_INFO_PLIST: &str = "payload/mct-daemon.app/Contents/Info.plist";
const RELEASE_EXECUTABLE: &str = "payload/mct-daemon.app/Contents/MacOS/mct-daemon";
const RELEASE_CODE_RESOURCES: &str = "payload/mct-daemon.app/Contents/_CodeSignature/CodeResources";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifestV1 {
    pub schema_version: u32,
    pub package_format_version: u32,
    pub release_mode: String,
    pub product: String,
    pub product_version: String,
    pub target_triple: String,
    pub source_commit: String,
    pub source_epoch: u64,
    pub rust_toolchain: String,
    pub rust_version: String,
    pub cargo_version: String,
    pub lockfile_sha256: String,
    pub executable_relative_path: String,
    pub executable_sha256: String,
    pub executable_blake3: String,
    pub release_notes_sha256: String,
    pub sbom_sha256: String,
    pub fixture_provenance_sha256: String,
    pub distribution_license: String,
    pub signing_mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedDaemonReleaseArchive {
    pub manifest: ReleaseManifestV1,
    pub archive_sha256: String,
    pub archive_blake3: String,
    pub archive_size_bytes: u64,
    pub release_root: PathBuf,
    pub executable_path: PathBuf,
    pub release_notes: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorPointedDaemonReleaseAcquisitionDecisionV1 {
    pub schema_version: u32,
    pub decision_id: String,
    pub source_ref: String,
    pub expected_archive_identity: Option<String>,
    pub product: String,
    pub target_triple: String,
    pub attempt_id: String,
    pub authenticated_uid: u32,
    pub policy_revision: u64,
    pub deadline: String,
    pub decision_state: String,
    pub authority_observation_id: String,
    pub consumed_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonReleaseArtifactV1 {
    pub schema_version: u32,
    pub release_artifact_id: String,
    pub product: String,
    pub product_version: String,
    pub target_triple: String,
    pub archive_size_bytes: u64,
    pub archive_sha256: String,
    pub archive_blake3: String,
    pub release_manifest_sha256: String,
    pub executable_relative_path: String,
    pub executable_sha256: String,
    pub executable_blake3: String,
    pub release_notes_sha256: String,
    pub sbom_sha256: String,
    pub fixture_provenance_sha256: String,
    pub source_revision: String,
    pub rust_toolchain: String,
    pub signing_mode: String,
    pub source_kind: String,
    pub source_ref: String,
    pub acquisition_decision_id: String,
    pub adapter_effect_authority_ref: String,
    pub acquisition_observation_id: String,
    pub verification_observation_id: String,
    pub immutable_release_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonReleaseAcquisitionV1 {
    pub schema_version: u32,
    pub attempt_id: String,
    pub acquisition_decision_id: String,
    pub source_kind: String,
    pub source_ref: String,
    pub expected_archive_identity: Option<String>,
    pub target_triple: String,
    pub authenticated_uid: u32,
    pub policy_revision: u64,
    pub adapter_effect_authority_ref: String,
    pub acquisition_observation_id: String,
    pub verification_observation_id: Option<String>,
    pub release_artifact_id: Option<String>,
    pub acquisition_outcome: String,
    pub verification_outcome: String,
    pub safe_message: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MctDaemonReleaseAcquisitionRequest {
    pub source_path: PathBuf,
    pub expected_archive_identity: Option<String>,
    pub target_triple: String,
    pub releases_dir: PathBuf,
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub authenticated_uid: u32,
    pub policy_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctDaemonReleaseAcquisitionReport {
    pub acquisition: DaemonReleaseAcquisitionV1,
    pub artifact: DaemonReleaseArtifactV1,
    pub release_notes: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctDaemonReleaseSourceKind {
    OperatorFile,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctDaemonReleaseSourcePlan {
    pub source_kind: MctDaemonReleaseSourceKind,
    pub source_path: PathBuf,
}

pub fn plan_daemon_release_source(artifact_ref: &str) -> Result<MctDaemonReleaseSourcePlan> {
    let path = if let Some(path) = artifact_ref.strip_prefix("file://") {
        if path.is_empty()
            || !path.starts_with('/')
            || path.contains('?')
            || path.contains('#')
            || path.contains('@')
            || path.contains('%')
        {
            bail!("upgrade operator_file reference must be a credential-free canonical file URI");
        }
        PathBuf::from(path)
    } else {
        if artifact_ref.contains("://") {
            bail!("upgrade source_kind is operator_file in v0.2; network sources are closed");
        }
        let path = PathBuf::from(artifact_ref);
        if !path.is_absolute() {
            bail!("upgrade operator_file path must be absolute");
        }
        path
    };
    Ok(MctDaemonReleaseSourcePlan {
        source_kind: MctDaemonReleaseSourceKind::OperatorFile,
        source_path: path,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctVerifiedDaemonRelease {
    report: MctDaemonReleaseAcquisitionReport,
}

impl MctVerifiedDaemonRelease {
    pub fn from_acquisition(report: MctDaemonReleaseAcquisitionReport) -> Result<Self> {
        if report.acquisition.source_kind != "operator_file"
            || report.acquisition.acquisition_outcome != "acquired"
            || report.acquisition.verification_outcome != "verified"
            || report.acquisition.release_artifact_id.as_deref()
                != Some(report.artifact.release_artifact_id.as_str())
        {
            bail!("upgrade planner requires verified daemon release evidence");
        }
        Ok(Self { report })
    }

    pub fn report(&self) -> &MctDaemonReleaseAcquisitionReport {
        &self.report
    }
}

#[derive(Debug)]
struct DaemonReleaseFilesystemEffectAuthority {
    source_path: PathBuf,
    source_ref: String,
    attempt_id: String,
    authenticated_uid: u32,
    policy_revision: u64,
    adapter_effect_authority_ref: String,
    deadline: jiff::Timestamp,
}

#[derive(Clone, Debug)]
struct EntryFact {
    path: String,
    is_dir: bool,
    size: u64,
    mode: u32,
    uid: u64,
    gid: u64,
    mtime: u64,
    sha256: Option<String>,
    blake3: Option<String>,
    metadata_bytes: Option<Vec<u8>>,
}

struct ArchiveScan {
    facts: BTreeMap<String, EntryFact>,
    manifest: ReleaseManifestV1,
    release_notes: String,
}

pub fn verify_and_extract_daemon_release_archive(
    archive_path: &Path,
    destination: &Path,
    expected_sha256: Option<&str>,
    expected_target: &str,
) -> Result<VerifiedDaemonReleaseArchive> {
    let archive_path = canonical_regular_file(archive_path, "release archive")?;
    let archive_parent = archive_path
        .parent()
        .context("release archive has no canonical parent")?;
    let archive_name = archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .context("release archive filename is not UTF-8")?;

    let metadata = fs::metadata(&archive_path)?;
    if metadata.len() > MCT_DAEMON_RELEASE_ARCHIVE_MAX_BYTES {
        bail!("release archive exceeds named byte bound");
    }
    let (archive_sha256, archive_blake3, archive_size_bytes) = hash_file(&archive_path)?;
    let tagged_archive_sha256 = format!("sha256:{archive_sha256}");
    if let Some(expected) = expected_sha256
        && expected != tagged_archive_sha256
    {
        bail!("release archive does not match expected SHA-256");
    }

    verify_external_sidecar(
        archive_parent,
        &archive_path.with_file_name(format!("{archive_name}.sha256")),
        archive_name,
        &archive_sha256,
        "SHA-256",
    )?;
    verify_external_sidecar(
        archive_parent,
        &archive_path.with_file_name(format!("{archive_name}.blake3")),
        archive_name,
        &archive_blake3,
        "BLAKE3",
    )?;

    let scan = scan_archive(&archive_path, expected_target)?;
    let expected_archive_name = format!("{}.tar.gz", release_root_name(&scan.manifest));
    if archive_name != expected_archive_name {
        bail!("release archive basename does not match manifest identity");
    }
    let gzip_mtime = gzip_header_mtime(&archive_path)?;
    if gzip_mtime != scan.manifest.source_epoch {
        bail!("release gzip header does not use the source epoch");
    }
    extract_verified_archive(&archive_path, destination, &scan.facts)?;
    let final_hashes = hash_file(&archive_path)?;
    if final_hashes.0 != archive_sha256
        || final_hashes.1 != archive_blake3
        || final_hashes.2 != archive_size_bytes
    {
        let _ = fs::remove_dir_all(destination);
        bail!("release archive changed during verification");
    }
    let release_root = destination.join(release_root_name(&scan.manifest));
    let executable_path = release_root.join(&scan.manifest.executable_relative_path);

    Ok(VerifiedDaemonReleaseArchive {
        manifest: scan.manifest,
        archive_sha256: tagged_archive_sha256,
        archive_blake3: format!("blake3:{archive_blake3}"),
        archive_size_bytes,
        release_root,
        executable_path,
        release_notes: scan.release_notes,
    })
}

pub fn acquire_operator_file_daemon_release_offline(
    request: &MctDaemonReleaseAcquisitionRequest,
) -> Result<MctDaemonReleaseAcquisitionReport> {
    let mut ledger =
        JsonlObservationLedger::open(&request.ledger_path, "ledger-local", "local-mct")?;
    acquire_operator_file_daemon_release_with_platform_verifier(
        request,
        verify_macos_release_signature,
        |observation| append_release_observation(&mut ledger, observation),
    )
}

pub fn acquire_operator_file_daemon_release_with_observer<F>(
    request: &MctDaemonReleaseAcquisitionRequest,
    observe: F,
) -> Result<MctDaemonReleaseAcquisitionReport>
where
    F: FnMut(MctObservation) -> Result<()>,
{
    acquire_operator_file_daemon_release_with_platform_verifier(
        request,
        verify_macos_release_signature,
        observe,
    )
}

fn acquire_operator_file_daemon_release_with_platform_verifier<F, O>(
    request: &MctDaemonReleaseAcquisitionRequest,
    verify_platform: F,
    mut observe: O,
) -> Result<MctDaemonReleaseAcquisitionReport>
where
    F: FnOnce(&Path) -> Result<()>,
    O: FnMut(MctObservation) -> Result<()>,
{
    validate_expected_archive_identity(request.expected_archive_identity.as_deref())?;
    if request.target_triple != "aarch64-apple-darwin"
        || !request.releases_dir.is_absolute()
        || !request.state_path.is_absolute()
        || !request.ledger_path.is_absolute()
        || request.policy_revision == 0
    {
        bail!("daemon release acquisition target, paths, or policy revision is invalid");
    }
    let source_path = canonical_regular_file(&request.source_path, "release archive")?;
    let source_text = source_path
        .to_str()
        .context("release archive canonical path is not UTF-8")?;
    if source_text
        .chars()
        .any(|character| character.is_control() || matches!(character, '?' | '#' | '@'))
    {
        bail!("release archive canonical path cannot form a credential-free file URI");
    }
    let source_ref = format!("file://{source_text}");
    let sequence = NEXT_DAEMON_RELEASE_ATTEMPT.fetch_add(1, Ordering::SeqCst);
    let suffix = format!("{}-{sequence}", current_timestamp_string());
    let attempt_id = format!("daemon-release-attempt:{suffix}");
    let decision_id = format!("daemon-release-decision:{suffix}");
    let authority_observation_id = format!("obs:daemon-release-decision:{suffix}");
    let adapter_start_observation_id = format!("obs:daemon-release-adapter-start:{suffix}");
    let acquisition_observation_id = format!("obs:daemon-release-acquired:{suffix}");
    let verification_observation_id = format!("obs:daemon-release-verified:{suffix}");
    let deadline = jiff::Timestamp::now()
        + jiff::SignedDuration::from_secs(MCT_DAEMON_RELEASE_ACQUISITION_DEADLINE_SECONDS);
    let decision = OperatorPointedDaemonReleaseAcquisitionDecisionV1 {
        schema_version: 1,
        decision_id: decision_id.clone(),
        source_ref: source_ref.clone(),
        expected_archive_identity: request.expected_archive_identity.clone(),
        product: "mct-daemon".into(),
        target_triple: request.target_triple.clone(),
        attempt_id: attempt_id.clone(),
        authenticated_uid: request.authenticated_uid,
        policy_revision: request.policy_revision,
        deadline: deadline.to_string(),
        decision_state: "active".into(),
        authority_observation_id: authority_observation_id.clone(),
        consumed_at: None,
    };
    let state = MctRuntimeStateStore::open(&request.state_path)?;
    state.record_daemon_release_decision(&decision)?;
    observe(release_observation(
        (&authority_observation_id, &attempt_id, &decision_id),
        ObservationKind::OperatorActionRecorded,
        SourcePlane::Operator,
        ObservationOutcome::Allowed,
        (
            &format!("os-uid:{}", request.authenticated_uid),
            &source_ref,
        ),
        request.policy_revision,
        "operator-pointed daemon release acquisition admitted",
    )?)?;
    observe(release_observation(
        (&adapter_start_observation_id, &attempt_id, &decision_id),
        ObservationKind::AdapterEffectStarted,
        SourcePlane::Adapter,
        ObservationOutcome::Started,
        ("local-mct", MCT_DAEMON_RELEASE_FILESYSTEM_ADAPTER),
        request.policy_revision,
        "daemon release filesystem acquisition started",
    )?)?;
    let authority = DaemonReleaseFilesystemEffectAuthority {
        source_path: source_path.clone(),
        source_ref: source_ref.clone(),
        attempt_id: attempt_id.clone(),
        authenticated_uid: request.authenticated_uid,
        policy_revision: request.policy_revision,
        adapter_effect_authority_ref: adapter_start_observation_id.clone(),
        deadline,
    };
    let acquiring_root = request.releases_dir.join(".acquiring");
    let staging = acquiring_root.join(attempt_id.replace(':', "-"));
    if staging.exists() {
        bail!("daemon release acquisition staging path already exists");
    }
    let verified = authority.acquire_and_verify(request, &staging, verify_platform);
    let verified = match verified {
        Ok(verified) => verified,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            observe(release_observation(
                (&acquisition_observation_id, &attempt_id, &decision_id),
                ObservationKind::AdapterEffectFailed,
                SourcePlane::Adapter,
                ObservationOutcome::Failed,
                ("local-mct", MCT_DAEMON_RELEASE_FILESYSTEM_ADAPTER),
                request.policy_revision,
                "daemon release filesystem acquisition failed",
            )?)?;
            observe(release_observation(
                (&verification_observation_id, &attempt_id, &decision_id),
                ObservationKind::ArtifactRejected,
                SourcePlane::Adapter,
                ObservationOutcome::Denied,
                ("local-mct", &source_ref),
                request.policy_revision,
                "daemon release archive rejected",
            )?)?;
            let failed = DaemonReleaseAcquisitionV1 {
                schema_version: 1,
                attempt_id,
                acquisition_decision_id: decision_id.clone(),
                source_kind: "operator_file".into(),
                source_ref,
                expected_archive_identity: request.expected_archive_identity.clone(),
                target_triple: request.target_triple.clone(),
                authenticated_uid: request.authenticated_uid,
                policy_revision: request.policy_revision,
                adapter_effect_authority_ref: adapter_start_observation_id,
                acquisition_observation_id,
                verification_observation_id: Some(verification_observation_id),
                release_artifact_id: None,
                acquisition_outcome: "failed".into(),
                verification_outcome: "rejected".into(),
                safe_message: "daemon release acquisition failed verification".into(),
                created_at: current_timestamp_string(),
            };
            state.record_failed_daemon_release_acquisition(&failed)?;
            state.consume_daemon_release_decision(&decision_id)?;
            return Err(error).context("daemon release archive acquisition rejected");
        }
    };
    observe(release_observation(
        (&acquisition_observation_id, &attempt_id, &decision_id),
        ObservationKind::AdapterEffectCompleted,
        SourcePlane::Adapter,
        ObservationOutcome::Completed,
        ("local-mct", MCT_DAEMON_RELEASE_FILESYSTEM_ADAPTER),
        request.policy_revision,
        "daemon release filesystem acquisition completed",
    )?)?;
    observe(release_observation(
        (&verification_observation_id, &attempt_id, &decision_id),
        ObservationKind::ArtifactVerified,
        SourcePlane::Adapter,
        ObservationOutcome::Allowed,
        ("local-mct", &verified.archive_sha256),
        request.policy_revision,
        "daemon release archive verified",
    )?)?;

    copy_release_sidecars(&source_path, &verified.release_root)?;
    let digest = verified
        .archive_sha256
        .strip_prefix("sha256:")
        .context("verified release archive has no SHA-256 tag")?;
    let immutable_path = request.releases_dir.join("sha256").join(digest);
    let manifest_path = verified.release_root.join(RELEASE_MANIFEST_FILE);
    let release_manifest_sha256 = format!("sha256:{}", hash_file(&manifest_path)?.0);
    let candidate_artifact = DaemonReleaseArtifactV1 {
        schema_version: 1,
        release_artifact_id: verified.archive_sha256.clone(),
        product: verified.manifest.product.clone(),
        product_version: verified.manifest.product_version.clone(),
        target_triple: verified.manifest.target_triple.clone(),
        archive_size_bytes: verified.archive_size_bytes,
        archive_sha256: verified.archive_sha256.clone(),
        archive_blake3: verified.archive_blake3.clone(),
        release_manifest_sha256,
        executable_relative_path: verified.manifest.executable_relative_path.clone(),
        executable_sha256: verified.manifest.executable_sha256.clone(),
        executable_blake3: verified.manifest.executable_blake3.clone(),
        release_notes_sha256: verified.manifest.release_notes_sha256.clone(),
        sbom_sha256: verified.manifest.sbom_sha256.clone(),
        fixture_provenance_sha256: verified.manifest.fixture_provenance_sha256.clone(),
        source_revision: verified.manifest.source_commit.clone(),
        rust_toolchain: verified.manifest.rust_toolchain.clone(),
        signing_mode: verified.manifest.signing_mode.clone(),
        source_kind: "operator_file".into(),
        source_ref: source_ref.clone(),
        acquisition_decision_id: decision_id.clone(),
        adapter_effect_authority_ref: adapter_start_observation_id.clone(),
        acquisition_observation_id: acquisition_observation_id.clone(),
        verification_observation_id: verification_observation_id.clone(),
        immutable_release_path: immutable_path.clone(),
    };
    observe(release_observation(
        (
            &format!("obs:daemon-release-storage-start:{suffix}"),
            &attempt_id,
            &decision_id,
        ),
        ObservationKind::AdapterEffectStarted,
        SourcePlane::Adapter,
        ObservationOutcome::Started,
        ("local-mct", "mct:daemon-release-storage/filesystem@1"),
        request.policy_revision,
        "immutable daemon release publication or reuse started",
    )?)?;
    let artifact = if immutable_path.exists() {
        let existing = state
            .daemon_release_artifact(&candidate_artifact.release_artifact_id)?
            .context("immutable daemon release path exists without release projection")?;
        if !same_daemon_release_bytes(&existing, &candidate_artifact)
            || !directory_trees_equal(&immutable_path, &verified.release_root)?
        {
            let _ = fs::remove_dir_all(&staging);
            bail!("immutable daemon release path conflicts with verified bytes or facts");
        }
        fs::remove_dir_all(&staging)?;
        existing
    } else {
        fs::create_dir_all(
            immutable_path
                .parent()
                .context("immutable daemon release path has no parent")?,
        )?;
        fs::rename(&verified.release_root, &immutable_path)
            .context("publish immutable daemon release directory")?;
        fs::remove_dir(&staging)?;
        fs::File::open(immutable_path.parent().unwrap())?.sync_all()?;
        candidate_artifact
    };
    let executable_path = immutable_path.join(&artifact.executable_relative_path);
    if !executable_path.is_file() {
        if artifact.acquisition_decision_id == decision_id {
            let _ = fs::remove_dir_all(&immutable_path);
        }
        bail!("immutable daemon release executable is absent after publication");
    }
    let acquisition = DaemonReleaseAcquisitionV1 {
        schema_version: 1,
        attempt_id: attempt_id.clone(),
        acquisition_decision_id: decision_id.clone(),
        source_kind: "operator_file".into(),
        source_ref,
        expected_archive_identity: request.expected_archive_identity.clone(),
        target_triple: request.target_triple.clone(),
        authenticated_uid: request.authenticated_uid,
        policy_revision: request.policy_revision,
        adapter_effect_authority_ref: adapter_start_observation_id,
        acquisition_observation_id,
        verification_observation_id: Some(verification_observation_id),
        release_artifact_id: Some(artifact.release_artifact_id.clone()),
        acquisition_outcome: "acquired".into(),
        verification_outcome: "verified".into(),
        safe_message: "daemon release archive acquired and verified".into(),
        created_at: current_timestamp_string(),
    };
    let was_new = artifact.acquisition_decision_id == decision_id;
    if let Err(error) = state.record_verified_daemon_release(&acquisition, &artifact) {
        if was_new {
            let _ = fs::remove_dir_all(&immutable_path);
        }
        return Err(error).context("persist verified daemon release projection");
    }
    state.consume_daemon_release_decision(&decision_id)?;
    if let Err(error) = observe(release_observation(
        (
            &format!("obs:daemon-release-storage-complete:{suffix}"),
            &attempt_id,
            &decision_id,
        ),
        ObservationKind::AdapterEffectCompleted,
        SourcePlane::Adapter,
        ObservationOutcome::Completed,
        ("local-mct", "mct:daemon-release-storage/filesystem@1"),
        request.policy_revision,
        "immutable daemon release publication completed",
    )?) {
        if let Some(path) = state.rollback_daemon_release_acquisition(&attempt_id)? {
            let _ = fs::remove_dir_all(path);
        }
        return Err(error)
            .context("daemon release storage completion was not durable; publication rolled back");
    }
    Ok(MctDaemonReleaseAcquisitionReport {
        acquisition,
        artifact,
        release_notes: verified.release_notes,
    })
}

impl DaemonReleaseFilesystemEffectAuthority {
    fn acquire_and_verify<F>(
        self,
        request: &MctDaemonReleaseAcquisitionRequest,
        staging: &Path,
        verify_platform: F,
    ) -> Result<VerifiedDaemonReleaseArchive>
    where
        F: FnOnce(&Path) -> Result<()>,
    {
        if self.source_path != request.source_path.canonicalize()?
            || self.authenticated_uid != request.authenticated_uid
            || self.policy_revision != request.policy_revision
            || self.attempt_id.trim().is_empty()
            || self.source_ref.trim().is_empty()
            || self.adapter_effect_authority_ref.trim().is_empty()
            || jiff::Timestamp::now() > self.deadline
        {
            bail!("daemon release filesystem effect authority is stale or mismatched");
        }
        fs::create_dir_all(
            staging
                .parent()
                .context("daemon release staging has no parent")?,
        )?;
        let verified = verify_and_extract_daemon_release_archive(
            &self.source_path,
            staging,
            request.expected_archive_identity.as_deref(),
            &request.target_triple,
        )?;
        if jiff::Timestamp::now() > self.deadline {
            bail!("daemon release acquisition exceeded its named deadline");
        }
        verify_platform(&verified.release_root)?;
        Ok(verified)
    }
}

fn verify_macos_release_signature(release_root: &Path) -> Result<()> {
    if !cfg!(target_os = "macos") {
        bail!("aarch64-apple-darwin signature verification requires macOS");
    }
    let bundle = release_root.join("payload/mct-daemon.app");
    let status = Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict", "--verbose=2"])
        .arg(&bundle)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("invoke macOS release signature verifier")?;
    if !status.success() {
        bail!("macOS release signature verification failed");
    }
    Ok(())
}

fn release_observation(
    correlation: (&str, &str, &str),
    kind: ObservationKind,
    source_plane: SourcePlane,
    outcome: ObservationOutcome,
    subject_resource: (&str, &str),
    policy_revision: u64,
    safe_message: &str,
) -> Result<MctObservation> {
    let (observation_id, attempt_id, decision_id) = correlation;
    let (subject_id, resource_id) = subject_resource;
    Ok(MctObservation {
        observation_id: ObservationId::new(observation_id)?,
        observed_at: current_timestamp(),
        kind,
        source_plane,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(attempt_id)?,
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: Some(DecisionId::new(decision_id)?),
        subject_id: Some(subject_id.into()),
        resource_id: Some(resource_id.into()),
        policy_revision: Some(policy_revision),
        grants_revision: None,
        outcome,
        visibility: ObservationVisibility::NodeOperator,
        safe_message: safe_message.into(),
        detail_ref: None,
    })
}

fn append_release_observation(
    ledger: &mut JsonlObservationLedger,
    observation: MctObservation,
) -> Result<()> {
    ledger.append(
        observation,
        current_timestamp_string(),
        DurabilityClass::BeforeEffect,
        ExportStatus::NotRequired,
    )?;
    Ok(())
}

fn validate_expected_archive_identity(expected: Option<&str>) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if !is_tagged_lower_hex(expected, "sha256") {
        bail!("expected daemon release identity must be sha256:<64-lower-hex>");
    }
    Ok(())
}

fn copy_release_sidecars(source: &Path, staging: &Path) -> Result<()> {
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .context("release archive name is not UTF-8")?;
    for algorithm in ["sha256", "blake3"] {
        let sidecar = source.with_file_name(format!("{name}.{algorithm}"));
        let destination = staging.join(format!("archive.{algorithm}"));
        fs::copy(&sidecar, &destination)?;
        let file = fs::OpenOptions::new().read(true).open(&destination)?;
        file.sync_all()?;
    }
    fs::File::open(staging)?.sync_all()?;
    Ok(())
}

fn same_daemon_release_bytes(
    left: &DaemonReleaseArtifactV1,
    right: &DaemonReleaseArtifactV1,
) -> bool {
    left.schema_version == right.schema_version
        && left.release_artifact_id == right.release_artifact_id
        && left.product == right.product
        && left.product_version == right.product_version
        && left.target_triple == right.target_triple
        && left.archive_size_bytes == right.archive_size_bytes
        && left.archive_sha256 == right.archive_sha256
        && left.archive_blake3 == right.archive_blake3
        && left.release_manifest_sha256 == right.release_manifest_sha256
        && left.executable_relative_path == right.executable_relative_path
        && left.executable_sha256 == right.executable_sha256
        && left.executable_blake3 == right.executable_blake3
        && left.release_notes_sha256 == right.release_notes_sha256
        && left.sbom_sha256 == right.sbom_sha256
        && left.fixture_provenance_sha256 == right.fixture_provenance_sha256
        && left.source_revision == right.source_revision
        && left.rust_toolchain == right.rust_toolchain
        && left.signing_mode == right.signing_mode
        && left.source_kind == right.source_kind
        && left.source_ref == right.source_ref
        && left.immutable_release_path == right.immutable_release_path
}

fn directory_trees_equal(left: &Path, right: &Path) -> Result<bool> {
    fn facts(root: &Path) -> Result<BTreeMap<String, Option<String>>> {
        fn visit(
            root: &Path,
            current: &Path,
            output: &mut BTreeMap<String, Option<String>>,
        ) -> Result<()> {
            let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let path = entry.path();
                let metadata = fs::symlink_metadata(&path)?;
                let relative = path
                    .strip_prefix(root)?
                    .to_str()
                    .context("immutable release path is not UTF-8")?
                    .to_owned();
                if metadata.is_dir() {
                    output.insert(relative, None);
                    visit(root, &path, output)?;
                } else if metadata.is_file() && !metadata.file_type().is_symlink() {
                    output.insert(relative, Some(hash_file(&path)?.0));
                } else {
                    bail!("immutable release contains unsupported filesystem entry");
                }
            }
            Ok(())
        }
        let mut output = BTreeMap::new();
        visit(root, root, &mut output)?;
        Ok(output)
    }
    Ok(facts(left)? == facts(right)?)
}

fn canonical_regular_file(path: &Path, label: &str) -> Result<PathBuf> {
    let link_metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect {label} {}", path.display()))?;
    if !link_metadata.file_type().is_file() {
        bail!("{label} must be a regular non-symlink file");
    }
    let canonical = fs::canonicalize(path)
        .with_context(|| format!("canonicalize {label} {}", path.display()))?;
    if !fs::metadata(&canonical)?.is_file() {
        bail!("{label} canonical target is not a regular file");
    }
    Ok(canonical)
}

fn hash_file(path: &Path) -> Result<(String, String, u64)> {
    let mut file = fs::File::open(path)?;
    let mut sha256 = Sha256::new();
    let mut blake3 = blake3::Hasher::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .context("release archive size overflow")?;
        if total > MCT_DAEMON_RELEASE_ARCHIVE_MAX_BYTES {
            bail!("release archive exceeds named byte bound");
        }
        sha256.update(&buffer[..read]);
        blake3.update(&buffer[..read]);
    }
    Ok((
        format!("{:x}", sha256.finalize()),
        blake3.finalize().to_hex().to_string(),
        total,
    ))
}

fn gzip_header_mtime(path: &Path) -> Result<u64> {
    let mut header = [0_u8; 10];
    fs::File::open(path)?.read_exact(&mut header)?;
    if header[0..3] != [0x1f, 0x8b, 8] || header[3] != 0 {
        bail!("release archive gzip header is not canonical");
    }
    Ok(u32::from_le_bytes(header[4..8].try_into().unwrap()).into())
}

fn verify_external_sidecar(
    expected_parent: &Path,
    sidecar_path: &Path,
    archive_name: &str,
    expected_digest: &str,
    algorithm: &str,
) -> Result<()> {
    let sidecar = canonical_regular_file(sidecar_path, "release checksum sidecar")?;
    if sidecar.parent() != Some(expected_parent) {
        bail!("release checksum sidecar escapes archive parent");
    }
    let metadata = fs::metadata(&sidecar)?;
    if metadata.len() > 256 {
        bail!("release checksum sidecar exceeds byte bound");
    }
    let text = fs::read_to_string(&sidecar).context("release checksum sidecar is not UTF-8")?;
    let expected = format!("{expected_digest}  {archive_name}\n");
    if text != expected {
        bail!("release {algorithm} sidecar does not match archive");
    }
    Ok(())
}

fn scan_archive(archive_path: &Path, expected_target: &str) -> Result<ArchiveScan> {
    let file = fs::File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut facts = BTreeMap::new();
    let mut entry_order = Vec::new();
    let mut extracted_size = 0_u64;

    for entry in archive.entries().context("read release archive entries")? {
        let mut entry = entry.context("read release archive entry")?;
        if facts.len() >= MCT_DAEMON_RELEASE_MAX_ENTRIES {
            bail!("release archive exceeds named entry bound");
        }
        let path = validated_entry_path(&entry)?;
        if facts.contains_key(&path) {
            bail!("release archive contains duplicate path {path}");
        }
        let header = entry.header();
        let entry_type = header.entry_type();
        let is_dir = entry_type.is_dir();
        if !is_dir && !entry_type.is_file() {
            bail!("release archive contains forbidden entry type at {path}");
        }
        let size = header.size().context("read release archive entry size")?;
        if is_dir && size != 0 {
            bail!("release archive directory has non-zero size");
        }
        if !is_dir {
            extracted_size = extracted_size
                .checked_add(size)
                .context("release extracted size overflow")?;
            if extracted_size > MCT_DAEMON_RELEASE_EXTRACTED_MAX_BYTES {
                bail!("release archive exceeds named extracted byte bound");
            }
        }
        let mode = header.mode().context("read release archive entry mode")?;
        let uid = header.uid().context("read release archive entry uid")?;
        let gid = header.gid().context("read release archive entry gid")?;
        let mtime = header.mtime().context("read release archive entry mtime")?;
        if header.username()?.is_some_and(|name| !name.is_empty())
            || header.groupname()?.is_some_and(|name| !name.is_empty())
        {
            bail!("release archive user/group names are not normalized");
        }

        let (sha256, blake3, metadata_bytes) = if is_dir {
            (None, None, None)
        } else {
            let capture = is_metadata_path(&path);
            if capture && size > MCT_DAEMON_RELEASE_METADATA_FILE_MAX_BYTES {
                bail!("release metadata file exceeds named byte bound");
            }
            let mut sha = Sha256::new();
            let mut b3 = blake3::Hasher::new();
            let mut bytes = capture.then(Vec::new);
            let mut total = 0_u64;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = entry.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                total = total
                    .checked_add(read as u64)
                    .context("entry size overflow")?;
                if total > size {
                    bail!("release archive entry exceeds declared size");
                }
                sha.update(&buffer[..read]);
                b3.update(&buffer[..read]);
                if let Some(bytes) = &mut bytes {
                    bytes.extend_from_slice(&buffer[..read]);
                }
            }
            if total != size {
                bail!("release archive entry size does not match header");
            }
            (
                Some(format!("{:x}", sha.finalize())),
                Some(b3.finalize().to_hex().to_string()),
                bytes,
            )
        };

        entry_order.push(path.clone());
        facts.insert(
            path.clone(),
            EntryFact {
                path,
                is_dir,
                size,
                mode,
                uid,
                gid,
                mtime,
                sha256,
                blake3,
                metadata_bytes,
            },
        );
    }

    let manifest_path = facts
        .keys()
        .filter(|path| path.ends_with(&format!("/{RELEASE_MANIFEST_FILE}")))
        .cloned()
        .collect::<Vec<_>>();
    if manifest_path.len() != 1 {
        bail!("release archive must contain exactly one release manifest");
    }
    let manifest_path = &manifest_path[0];
    let manifest_bytes = facts[manifest_path]
        .metadata_bytes
        .as_deref()
        .context("release manifest bytes were not captured")?;
    validate_display_bytes(manifest_bytes, "release manifest")?;
    let manifest: ReleaseManifestV1 =
        serde_json::from_slice(manifest_bytes).context("decode release manifest")?;
    if serde_json::to_vec(&manifest)? != manifest_bytes {
        bail!("release manifest is not canonical compact JSON");
    }
    validate_manifest(&manifest, expected_target)?;
    let root = release_root_name(&manifest);
    if manifest_path != &format!("{root}/{RELEASE_MANIFEST_FILE}") {
        bail!("release manifest is not under its exact package root");
    }

    validate_exact_layout(&facts, &entry_order, &manifest)?;
    validate_internal_checksums(&facts, &manifest)?;
    validate_metadata(&facts, &manifest)?;

    let notes_path = format!("{root}/{RELEASE_NOTES_FILE}");
    let notes_bytes = facts[&notes_path]
        .metadata_bytes
        .as_deref()
        .context("release notes bytes were not captured")?;
    let release_notes = std::str::from_utf8(notes_bytes)
        .context("release notes are not UTF-8")?
        .to_owned();

    Ok(ArchiveScan {
        facts,
        manifest,
        release_notes,
    })
}

fn validated_entry_path<R: Read>(entry: &tar::Entry<'_, R>) -> Result<String> {
    let path = entry.path().context("decode release archive entry path")?;
    if path.as_os_str().is_empty() || path.is_absolute() {
        bail!("release archive path must be non-empty and relative");
    }
    if !path
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        bail!("release archive path contains an unsafe component");
    }
    let mut text = path
        .to_str()
        .context("release archive path is not UTF-8")?
        .to_owned();
    if text.contains('\\') || text.bytes().any(|byte| byte == 0) {
        bail!("release archive path contains an ambiguous separator");
    }
    if entry.header().entry_type().is_dir() {
        if !text.ends_with('/') || text.ends_with("//") {
            bail!("release archive directory path is not canonical");
        }
        text.pop();
    } else if text.ends_with('/') {
        bail!("release archive file path is not canonical");
    }
    Ok(text)
}

fn is_metadata_path(path: &str) -> bool {
    [
        RELEASE_MANIFEST_FILE,
        RELEASE_NOTES_FILE,
        RELEASE_SBOM_FILE,
        RELEASE_FIXTURE_PROVENANCE_FILE,
        RELEASE_LICENSE_FILE,
        RELEASE_CHECKSUMS_FILE,
        "Info.plist",
    ]
    .iter()
    .any(|name| path.ends_with(&format!("/{name}")))
}

fn validate_manifest(manifest: &ReleaseManifestV1, expected_target: &str) -> Result<()> {
    if manifest.schema_version != 1
        || manifest.package_format_version != 1
        || !matches!(manifest.release_mode.as_str(), "release" | "smoke")
        || manifest.product != "mct-daemon"
        || manifest.product_version != crate::version()
        || manifest.target_triple != expected_target
        || manifest.distribution_license != "MIT"
        || manifest.signing_mode != "adhoc"
        || manifest.executable_relative_path != RELEASE_EXECUTABLE
    {
        bail!("release manifest product, version, target, license, signing, or format mismatch");
    }
    if !is_lower_hex(&manifest.source_commit, 40)
        || !is_tagged_lower_hex(&manifest.lockfile_sha256, "sha256")
        || !is_tagged_lower_hex(&manifest.executable_sha256, "sha256")
        || !is_tagged_lower_hex(&manifest.executable_blake3, "blake3")
        || !is_tagged_lower_hex(&manifest.release_notes_sha256, "sha256")
        || !is_tagged_lower_hex(&manifest.sbom_sha256, "sha256")
        || !is_tagged_lower_hex(&manifest.fixture_provenance_sha256, "sha256")
        || manifest.source_epoch == 0
        || manifest.rust_toolchain.trim().is_empty()
        || manifest.rust_version.trim().is_empty()
        || manifest.cargo_version.trim().is_empty()
    {
        bail!("release manifest contains malformed provenance or digest fields");
    }
    Ok(())
}

fn validate_exact_layout(
    facts: &BTreeMap<String, EntryFact>,
    entry_order: &[String],
    manifest: &ReleaseManifestV1,
) -> Result<()> {
    let root = release_root_name(manifest);
    let expected_dirs = [
        root.clone(),
        format!("{root}/payload"),
        format!("{root}/payload/mct-daemon.app"),
        format!("{root}/payload/mct-daemon.app/Contents"),
        format!("{root}/payload/mct-daemon.app/Contents/MacOS"),
        format!("{root}/payload/mct-daemon.app/Contents/_CodeSignature"),
    ];
    let expected_files = [
        format!("{root}/{RELEASE_MANIFEST_FILE}"),
        format!("{root}/{RELEASE_NOTES_FILE}"),
        format!("{root}/{RELEASE_SBOM_FILE}"),
        format!("{root}/{RELEASE_FIXTURE_PROVENANCE_FILE}"),
        format!("{root}/{RELEASE_LICENSE_FILE}"),
        format!("{root}/{RELEASE_CHECKSUMS_FILE}"),
        format!("{root}/{RELEASE_INFO_PLIST}"),
        format!("{root}/{}", manifest.executable_relative_path),
        format!("{root}/{RELEASE_CODE_RESOURCES}"),
    ];
    let expected = expected_dirs
        .iter()
        .chain(expected_files.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let actual = facts.keys().cloned().collect::<BTreeSet<_>>();
    if actual != expected {
        bail!("release archive layout is not exact");
    }
    let mut expected_order = expected_dirs.to_vec();
    let mut files_in_order = expected_files.to_vec();
    files_in_order.sort();
    expected_order.extend(files_in_order);
    if entry_order != expected_order {
        bail!("release archive member order is not canonical");
    }
    for directory in expected_dirs {
        let fact = &facts[&directory];
        if !fact.is_dir || fact.mode != 0o755 {
            bail!("release archive directory mode/type mismatch at {directory}");
        }
    }
    for file in expected_files {
        let fact = &facts[&file];
        let expected_mode = if file.ends_with(RELEASE_EXECUTABLE) {
            0o755
        } else {
            0o644
        };
        if fact.is_dir || fact.mode != expected_mode {
            bail!("release archive file mode/type mismatch at {file}");
        }
    }
    for fact in facts.values() {
        if fact.uid != 0 || fact.gid != 0 || fact.mtime != manifest.source_epoch {
            bail!("release archive ownership or source epoch is not normalized");
        }
    }
    Ok(())
}

fn validate_internal_checksums(
    facts: &BTreeMap<String, EntryFact>,
    manifest: &ReleaseManifestV1,
) -> Result<()> {
    let root = release_root_name(manifest);
    let checksums_path = format!("{root}/{RELEASE_CHECKSUMS_FILE}");
    let bytes = facts[&checksums_path]
        .metadata_bytes
        .as_deref()
        .context("release checksums bytes were not captured")?;
    validate_display_bytes(bytes, "release checksums")?;
    let text = std::str::from_utf8(bytes)?;
    let mut actual_lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
    if !text.ends_with('\n') || actual_lines.iter().any(|line| line.is_empty()) {
        bail!("release checksums must be non-empty newline-terminated records");
    }
    let mut sorted = actual_lines.clone();
    sorted.sort();
    if actual_lines != sorted {
        bail!("release checksums are not sorted");
    }

    let expected_lines = facts
        .values()
        .filter(|fact| !fact.is_dir && fact.path != checksums_path)
        .flat_map(|fact| {
            [
                format!(
                    "blake3 {} {}",
                    fact.blake3.as_deref().expect("file BLAKE3 must exist"),
                    fact.path.strip_prefix(&format!("{root}/")).unwrap()
                ),
                format!(
                    "sha256 {} {}",
                    fact.sha256.as_deref().expect("file SHA-256 must exist"),
                    fact.path.strip_prefix(&format!("{root}/")).unwrap()
                ),
            ]
        })
        .collect::<BTreeSet<_>>();
    let line_count = actual_lines.len();
    let actual = actual_lines.drain(..).collect::<BTreeSet<_>>();
    if actual.len() != line_count || actual != expected_lines {
        bail!("release checksums do not cover the exact package files");
    }
    Ok(())
}

fn validate_metadata(
    facts: &BTreeMap<String, EntryFact>,
    manifest: &ReleaseManifestV1,
) -> Result<()> {
    let root = release_root_name(manifest);
    let file_sha = |relative: &str| -> &str {
        facts[&format!("{root}/{relative}")]
            .sha256
            .as_deref()
            .expect("file SHA-256 must exist")
    };
    if manifest.executable_sha256 != format!("sha256:{}", file_sha(RELEASE_EXECUTABLE))
        || manifest.executable_blake3
            != format!(
                "blake3:{}",
                facts[&format!("{root}/{RELEASE_EXECUTABLE}")]
                    .blake3
                    .as_deref()
                    .expect("executable BLAKE3 must exist")
            )
        || manifest.release_notes_sha256 != format!("sha256:{}", file_sha(RELEASE_NOTES_FILE))
        || manifest.sbom_sha256 != format!("sha256:{}", file_sha(RELEASE_SBOM_FILE))
        || manifest.fixture_provenance_sha256
            != format!("sha256:{}", file_sha(RELEASE_FIXTURE_PROVENANCE_FILE))
    {
        bail!("release manifest file digests do not match package bytes");
    }

    for relative in [
        RELEASE_MANIFEST_FILE,
        RELEASE_NOTES_FILE,
        RELEASE_SBOM_FILE,
        RELEASE_FIXTURE_PROVENANCE_FILE,
        RELEASE_LICENSE_FILE,
        RELEASE_CHECKSUMS_FILE,
        RELEASE_INFO_PLIST,
    ] {
        let bytes = facts[&format!("{root}/{relative}")]
            .metadata_bytes
            .as_deref()
            .context("release metadata bytes were not captured")?;
        validate_display_bytes(bytes, relative)?;
    }
    serde_json::from_slice::<serde_json::Value>(
        facts[&format!("{root}/{RELEASE_SBOM_FILE}")]
            .metadata_bytes
            .as_deref()
            .unwrap(),
    )
    .context("decode release SBOM")?;
    serde_json::from_slice::<serde_json::Value>(
        facts[&format!("{root}/{RELEASE_FIXTURE_PROVENANCE_FILE}")]
            .metadata_bytes
            .as_deref()
            .unwrap(),
    )
    .context("decode fixture provenance")?;
    Ok(())
}

fn validate_display_bytes(bytes: &[u8], label: &str) -> Result<()> {
    let text = std::str::from_utf8(bytes).with_context(|| format!("{label} is not UTF-8"))?;
    if text.chars().any(|character| {
        (character.is_control() && character != '\n')
            || matches!(
                character,
                '\u{202A}'
                    ..='\u{202E}'
                        | '\u{2066}'
                        ..='\u{2069}'
                        | '\u{061C}'
                        | '\u{200E}'
                        | '\u{200F}'
            )
    }) {
        bail!("{label} contains forbidden terminal control text");
    }
    Ok(())
}

fn extract_verified_archive(
    archive_path: &Path,
    destination: &Path,
    facts: &BTreeMap<String, EntryFact>,
) -> Result<()> {
    if destination.exists() {
        bail!("release extraction destination must not already exist");
    }
    fs::create_dir(destination).with_context(|| {
        format!(
            "create release extraction destination {}",
            destination.display()
        )
    })?;
    let result = (|| -> Result<()> {
        let mut directories = facts
            .values()
            .filter(|fact| fact.is_dir)
            .collect::<Vec<_>>();
        directories.sort_by_key(|fact| fact.path.matches('/').count());
        for fact in directories {
            let path = destination.join(&fact.path);
            fs::create_dir(&path)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(fact.mode))?;
        }

        let file = fs::File::open(archive_path)?;
        let decoder = GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = validated_entry_path(&entry)?;
            let fact = facts.get(&path).context("archive changed between passes")?;
            if fact.is_dir {
                continue;
            }
            let output_path = destination.join(&path);
            let mut output = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(fact.mode)
                .open(&output_path)?;
            let mut sha = Sha256::new();
            let mut b3 = blake3::Hasher::new();
            let mut total = 0_u64;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = entry.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                total += read as u64;
                output.write_all(&buffer[..read])?;
                sha.update(&buffer[..read]);
                b3.update(&buffer[..read]);
            }
            let sha = format!("{:x}", sha.finalize());
            let b3 = b3.finalize().to_hex().to_string();
            if total != fact.size
                || fact.sha256.as_deref() != Some(sha.as_str())
                || fact.blake3.as_deref() != Some(b3.as_str())
            {
                bail!("archive changed during verified extraction");
            }
            output.sync_all()?;
            fs::set_permissions(&output_path, fs::Permissions::from_mode(fact.mode))?;
        }
        sync_tree_directories(destination, facts)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

fn sync_tree_directories(destination: &Path, facts: &BTreeMap<String, EntryFact>) -> Result<()> {
    let mut directories = facts
        .values()
        .filter(|fact| fact.is_dir)
        .map(|fact| destination.join(&fact.path))
        .collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        fs::File::open(&directory)?.sync_all()?;
    }
    fs::File::open(destination)?.sync_all()?;
    Ok(())
}

fn release_root_name(manifest: &ReleaseManifestV1) -> String {
    format!(
        "mct-daemon-v{}-{}",
        manifest.product_version, manifest.target_triple
    )
}

fn is_tagged_lower_hex(value: &str, algorithm: &str) -> bool {
    value
        .strip_prefix(&format!("{algorithm}:"))
        .is_some_and(|digest| is_lower_hex(digest, 64))
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod acquisition_tests {
    use super::*;
    use flate2::{Compression, GzBuilder};

    const TARGET: &str = "aarch64-apple-darwin";
    const EPOCH: u64 = 1_700_000_000;

    #[test]
    fn operator_file_release_acquisition_is_observed_immutable_and_not_child_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let archive = release_fixture(temp.path());
        let request = MctDaemonReleaseAcquisitionRequest {
            source_path: archive.clone(),
            expected_archive_identity: None,
            target_triple: TARGET.into(),
            releases_dir: temp.path().join("service/releases"),
            state_path: temp.path().join("service/state.sqlite"),
            ledger_path: temp.path().join("service/observations.jsonl"),
            authenticated_uid: 501,
            policy_revision: 1,
        };

        let first = acquire_for_test(&request).unwrap();
        let second = acquire_for_test(&request).unwrap();
        assert_eq!(first.artifact, second.artifact);
        assert_eq!(
            first.artifact.release_artifact_id,
            first.artifact.archive_sha256
        );
        assert!(first.artifact.immutable_release_path.is_dir());
        let state = MctRuntimeStateStore::open(&request.state_path).unwrap();
        assert_eq!(state.daemon_release_artifacts().unwrap().len(), 1);
        assert_eq!(state.daemon_release_acquisitions().unwrap().len(), 2);
        assert_eq!(state.summary().unwrap().artifacts, 0);
        assert_eq!(state.summary().unwrap().daemon_release_artifacts, 1);
        let entries = JsonlObservationLedger::open_read_only(
            &request.ledger_path,
            "ledger-local",
            "local-mct",
        )
        .unwrap()
        .entries()
        .unwrap();
        let kinds = entries
            .iter()
            .map(|entry| entry.observation.kind)
            .collect::<Vec<_>>();
        assert_eq!(kinds[0], ObservationKind::OperatorActionRecorded);
        assert_eq!(kinds[1], ObservationKind::AdapterEffectStarted);
        assert_eq!(kinds[2], ObservationKind::AdapterEffectCompleted);
        assert_eq!(kinds[3], ObservationKind::ArtifactVerified);
        let verification_index = kinds
            .iter()
            .position(|kind| *kind == ObservationKind::ArtifactVerified)
            .unwrap();
        let storage_index = kinds
            .iter()
            .enumerate()
            .find(|(index, kind)| {
                *index > verification_index && **kind == ObservationKind::AdapterEffectStarted
            })
            .map(|(index, _)| index)
            .unwrap();
        assert!(verification_index < storage_index);

        let mut bytes = fs::read(&archive).unwrap();
        bytes.push(0);
        fs::write(&archive, bytes).unwrap();
        let error = acquire_for_test(&request).unwrap_err();
        assert!(error.to_string().contains("rejected"));
        let state = MctRuntimeStateStore::open(&request.state_path).unwrap();
        assert_eq!(state.daemon_release_artifacts().unwrap().len(), 1);
        assert_eq!(state.daemon_release_acquisitions().unwrap().len(), 3);
        assert_eq!(
            state.daemon_release_acquisitions().unwrap()[2].verification_outcome,
            "rejected"
        );
    }

    #[test]
    fn release_publication_rolls_back_when_terminal_writer_is_lost() {
        let temp = tempfile::tempdir().unwrap();
        let archive = release_fixture(temp.path());
        let request = MctDaemonReleaseAcquisitionRequest {
            source_path: archive,
            expected_archive_identity: None,
            target_triple: TARGET.into(),
            releases_dir: temp.path().join("service/releases"),
            state_path: temp.path().join("service/state.sqlite"),
            ledger_path: temp.path().join("service/observations.jsonl"),
            authenticated_uid: 501,
            policy_revision: 1,
        };
        let mut ledger =
            JsonlObservationLedger::open(&request.ledger_path, "ledger-local", "local-mct")
                .unwrap();
        let result = acquire_operator_file_daemon_release_with_platform_verifier(
            &request,
            |_| Ok(()),
            |observation| {
                if observation.safe_message == "immutable daemon release publication completed" {
                    bail!("injected terminal writer loss");
                }
                append_release_observation(&mut ledger, observation)
            },
        );
        assert!(result.unwrap_err().to_string().contains("rolled back"));
        drop(ledger);
        let state = MctRuntimeStateStore::open(&request.state_path).unwrap();
        assert!(state.daemon_release_artifacts().unwrap().is_empty());
        assert!(state.daemon_release_acquisitions().unwrap().is_empty());
        assert_eq!(
            fs::read_dir(request.releases_dir.join("sha256"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(
            state.daemon_release_decisions().unwrap()[0].decision_state,
            "consumed"
        );
    }

    #[test]
    fn release_source_plan_is_operator_file_only_and_network_adapter_remains_closed() {
        for reference in [
            "https://example.invalid/release.tar.gz",
            "git://example.invalid/release",
            "oci://registry.invalid/release",
            "iroh://ticket",
            "relative/release.tar.gz",
            "file:///tmp/release.tar.gz?credential=secret",
            "file://user@host/tmp/release.tar.gz",
        ] {
            assert!(
                plan_daemon_release_source(reference).is_err(),
                "{reference}"
            );
        }
        let direct = plan_daemon_release_source("/tmp/release.tar.gz").unwrap();
        let file = plan_daemon_release_source("file:///tmp/release.tar.gz").unwrap();
        assert_eq!(direct, file);
        assert_eq!(direct.source_kind, MctDaemonReleaseSourceKind::OperatorFile);
    }

    #[test]
    fn malformed_release_request_refuses_before_source_or_evidence_effects() {
        let temp = tempfile::tempdir().unwrap();
        let request = MctDaemonReleaseAcquisitionRequest {
            source_path: temp.path().join("absent.tar.gz"),
            expected_archive_identity: Some("sha256:WRONG".into()),
            target_triple: TARGET.into(),
            releases_dir: temp.path().join("releases"),
            state_path: temp.path().join("state.sqlite"),
            ledger_path: temp.path().join("observations.jsonl"),
            authenticated_uid: 501,
            policy_revision: 1,
        };
        assert!(
            acquire_operator_file_daemon_release_with_platform_verifier(
                &request,
                |_| Ok(()),
                |_| Ok(()),
            )
            .unwrap_err()
            .to_string()
            .contains("64-lower-hex")
        );
        assert!(!request.state_path.exists());
        assert!(!request.ledger_path.exists());
    }

    fn acquire_for_test(
        request: &MctDaemonReleaseAcquisitionRequest,
    ) -> Result<MctDaemonReleaseAcquisitionReport> {
        let mut ledger =
            JsonlObservationLedger::open(&request.ledger_path, "ledger-local", "local-mct")?;
        acquire_operator_file_daemon_release_with_platform_verifier(
            request,
            |_| Ok(()),
            |observation| append_release_observation(&mut ledger, observation),
        )
    }

    fn release_fixture(directory: &Path) -> PathBuf {
        let version = crate::version();
        let root = format!("mct-daemon-v{version}-{TARGET}");
        let archive_name = format!("{root}.tar.gz");
        let archive = directory.join(&archive_name);
        let mut files = BTreeMap::<String, Vec<u8>>::new();
        files.insert(
            RELEASE_INFO_PLIST.into(),
            b"<?xml version=\"1.0\"?>\n<plist version=\"1.0\"><dict/></plist>\n".to_vec(),
        );
        files.insert(RELEASE_EXECUTABLE.into(), b"test executable".to_vec());
        files.insert(RELEASE_CODE_RESOURCES.into(), b"sealed resources".to_vec());
        files.insert(
            RELEASE_NOTES_FILE.into(),
            b"# MCT 0.2.0\n\nNotes.\n".to_vec(),
        );
        files.insert(
            RELEASE_SBOM_FILE.into(),
            br#"{"bomFormat":"CycloneDX","specVersion":"1.6"}"#.to_vec(),
        );
        files.insert(
            RELEASE_FIXTURE_PROVENANCE_FILE.into(),
            br#"{"fixtures":[]}"#.to_vec(),
        );
        files.insert(RELEASE_LICENSE_FILE.into(), b"MIT License\n".to_vec());
        let manifest = ReleaseManifestV1 {
            schema_version: 1,
            package_format_version: 1,
            release_mode: "smoke".into(),
            product: "mct-daemon".into(),
            product_version: version.into(),
            target_triple: TARGET.into(),
            source_commit: "1".repeat(40),
            source_epoch: EPOCH,
            rust_toolchain: "1.96.0".into(),
            rust_version: "rustc 1.96.0".into(),
            cargo_version: "cargo 1.96.0".into(),
            lockfile_sha256: tagged_sha(b"lockfile"),
            executable_relative_path: RELEASE_EXECUTABLE.into(),
            executable_sha256: tagged_sha(&files[RELEASE_EXECUTABLE]),
            executable_blake3: format!(
                "blake3:{}",
                blake3::hash(&files[RELEASE_EXECUTABLE]).to_hex()
            ),
            release_notes_sha256: tagged_sha(&files[RELEASE_NOTES_FILE]),
            sbom_sha256: tagged_sha(&files[RELEASE_SBOM_FILE]),
            fixture_provenance_sha256: tagged_sha(&files[RELEASE_FIXTURE_PROVENANCE_FILE]),
            distribution_license: "MIT".into(),
            signing_mode: "adhoc".into(),
        };
        files.insert(
            RELEASE_MANIFEST_FILE.into(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        let mut lines = files
            .iter()
            .flat_map(|(path, bytes)| {
                [
                    format!("blake3 {} {path}", blake3::hash(bytes).to_hex()),
                    format!("sha256 {} {path}", sha_hex(bytes)),
                ]
            })
            .collect::<Vec<_>>();
        lines.sort();
        files.insert(
            RELEASE_CHECKSUMS_FILE.into(),
            format!("{}\n", lines.join("\n")).into_bytes(),
        );

        let encoder = GzBuilder::new()
            .mtime(EPOCH as u32)
            .write(fs::File::create(&archive).unwrap(), Compression::best());
        let mut tar = tar::Builder::new(encoder);
        for path in [
            root.clone(),
            format!("{root}/payload"),
            format!("{root}/payload/mct-daemon.app"),
            format!("{root}/payload/mct-daemon.app/Contents"),
            format!("{root}/payload/mct-daemon.app/Contents/MacOS"),
            format!("{root}/payload/mct-daemon.app/Contents/_CodeSignature"),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Directory);
            header.set_path(format!("{path}/")).unwrap();
            header.set_size(0);
            normalize(&mut header, 0o755);
            tar.append(&header, &[][..]).unwrap();
        }
        for (path, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_path(format!("{root}/{path}")).unwrap();
            header.set_size(bytes.len() as u64);
            normalize(
                &mut header,
                if path == RELEASE_EXECUTABLE {
                    0o755
                } else {
                    0o644
                },
            );
            tar.append(&header, bytes.as_slice()).unwrap();
        }
        tar.into_inner()
            .unwrap()
            .finish()
            .unwrap()
            .sync_all()
            .unwrap();
        let archive_bytes = fs::read(&archive).unwrap();
        fs::write(
            directory.join(format!("{archive_name}.sha256")),
            format!("{}  {archive_name}\n", sha_hex(&archive_bytes)),
        )
        .unwrap();
        fs::write(
            directory.join(format!("{archive_name}.blake3")),
            format!(
                "{}  {archive_name}\n",
                blake3::hash(&archive_bytes).to_hex()
            ),
        )
        .unwrap();
        archive
    }

    fn normalize(header: &mut tar::Header, mode: u32) {
        header.set_mode(mode);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(EPOCH);
        header.set_username("").unwrap();
        header.set_groupname("").unwrap();
        header.set_cksum();
    }

    fn sha_hex(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn tagged_sha(bytes: &[u8]) -> String {
        format!("sha256:{}", sha_hex(bytes))
    }
}

//! Disk-first startup evidence for Mother authority establishment.

use anyhow::Context as _;
use mct_kernel::{
    MctObservation, ObservationId, ObservationKind, ObservationOutcome, ObservationTraceRef,
    ObservationVisibility, SourcePlane, Timestamp, TraceId,
};
use mct_observation::{
    AuthorityEpochPredecessorV1, AuthorityProjectionCursorV1, AuthorityProjectionDenyReasonV1,
    AuthorityProjectionExpectationV1, AuthorityProjectionLedgerEvidenceV1, AuthorityStartupClassV1,
    AuthorityStateV1, AuthorityTenureStartupEvidenceV1, GrantsAuthorityIdentityV1,
    JsonlObservationLedger, LedgerQuarantineStatus, LedgerRecoveryStatus, ObservationLedgerError,
    UsableAuthorityProjectionProofV1, authority_state_hash, replay_authority_entries,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read as _, Seek as _, SeekFrom},
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctStartupArtifactClassV1 {
    CanonicalObservationLedger,
    LedgerRecoveryForensics,
    RuntimeSqlite,
    SqliteDurabilitySidecar,
    DaemonConfiguration,
    InterruptedConfigPublication,
    RecordedMotherIdentity,
    ChildPackageCatalog,
    InterruptedChildPublication,
    ContentAddressedBlobs,
    DaemonReleaseStore,
    SupervisorLifecycleRecord,
    InterruptedSupervisorPublication,
    SupervisorPolicy,
    InterruptedSupervisorPolicyPublication,
    SupervisorLogs,
    OtherManagedRootEntry,
    ControlSocket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctStartupArtifactStateV1 {
    Absent,
    Present,
    Unavailable,
    Transient,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctStartupArtifactFileTypeV1 {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctStartupArtifactEntryV1 {
    pub artifact_class: MctStartupArtifactClassV1,
    pub path: PathBuf,
    pub state: MctStartupArtifactStateV1,
    pub file_type: Option<MctStartupArtifactFileTypeV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctStartupArtifactInventoryV1 {
    pub schema: String,
    pub entries: Vec<MctStartupArtifactEntryV1>,
    pub inventory_hash: String,
}

pub const MCT_OPERATOR_REINITIALIZATION_CONFIRMATION_V1: &str =
    "reinitialize-missing-canonical-authority-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MctOperatorStartupGateRequestV1 {
    pub schema: String,
    pub decision_id: String,
    pub expected_mother_node_id: String,
    pub expected_ledger_id: String,
    pub expected_inventory_hash: String,
    pub confirmation: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctAcceptedStartupGateV1 {
    pub decision_id: String,
    pub authenticated_principal_ref: String,
    pub inventory_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctAuthorityStartupEvidenceV1 {
    pub startup_class: AuthorityStartupClassV1,
    pub inventory: MctStartupArtifactInventoryV1,
    pub validated_head: Option<(u64, String)>,
    pub prior_authority: Option<GrantsAuthorityIdentityV1>,
    pub canonical_authority_state: AuthorityStateV1,
    pub authority_state_hash: String,
    pub ledger_entry_count: usize,
}

#[derive(Debug, Error)]
pub enum MctStartupClassificationErrorV1 {
    #[error("startup disk evidence is unavailable")]
    DiskEvidenceUnavailable,
    #[error("canonical ledger path is ambiguous")]
    LedgerAmbiguous,
    #[error("canonical ledger evidence could not be validated: {0}")]
    LedgerUnavailable(#[source] ObservationLedgerError),
    #[error("canonical authority replay is blocked: {0}")]
    AuthorityReplayBlocked(String),
    #[error("startup operator gate is required")]
    OperatorGateRequired,
    #[error("startup operator gate was refused: {0}")]
    OperatorGateRefused(String),
}

impl MctAuthorityStartupEvidenceV1 {
    pub fn tenure_evidence(
        &self,
        gate: Option<&MctAcceptedStartupGateV1>,
    ) -> Result<AuthorityTenureStartupEvidenceV1, MctStartupClassificationErrorV1> {
        let expected_predecessor = match self.startup_class {
            AuthorityStartupClassV1::Virgin => AuthorityEpochPredecessorV1::NoneForVirgin,
            AuthorityStartupClassV1::OperatorGatedNonvirgin => {
                AuthorityEpochPredecessorV1::NoneAfterOperatorReinitialization
            }
            AuthorityStartupClassV1::LegacyLedgerUpgrade
            | AuthorityStartupClassV1::OrdinaryReopen => {
                let (sequence, entry_hash) = self.validated_head.as_ref().ok_or_else(|| {
                    MctStartupClassificationErrorV1::AuthorityReplayBlocked(
                        "validated startup class has no canonical head".into(),
                    )
                })?;
                AuthorityEpochPredecessorV1::ValidatedHead {
                    sequence: *sequence,
                    entry_hash: entry_hash.clone(),
                }
            }
        };
        let (operator_gate_decision_id, authenticated_principal_ref) =
            if self.startup_class == AuthorityStartupClassV1::OperatorGatedNonvirgin {
                let gate = gate.ok_or(MctStartupClassificationErrorV1::OperatorGateRequired)?;
                if gate.inventory_hash != self.inventory.inventory_hash {
                    return Err(MctStartupClassificationErrorV1::OperatorGateRefused(
                        "operator gate inventory changed".into(),
                    ));
                }
                (
                    Some(gate.decision_id.clone()),
                    Some(gate.authenticated_principal_ref.clone()),
                )
            } else {
                (None, None)
            };
        Ok(AuthorityTenureStartupEvidenceV1 {
            startup_class: self.startup_class,
            expected_predecessor,
            expected_prior_authority: self.prior_authority.clone(),
            expected_authority_state_hash: self.authority_state_hash.clone(),
            inventory_hash: self.inventory.inventory_hash.clone(),
            operator_gate_decision_id,
            authenticated_principal_ref,
        })
    }
}

impl MctStartupArtifactInventoryV1 {
    pub fn proves_virgin(&self) -> bool {
        self.entries.iter().all(|entry| {
            matches!(
                entry.state,
                MctStartupArtifactStateV1::Absent | MctStartupArtifactStateV1::Transient
            )
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctStartupPaths {
    pub root: PathBuf,
    pub ledger: PathBuf,
    pub state: PathBuf,
    pub config: PathBuf,
    pub identity: PathBuf,
    pub children: PathBuf,
    pub control_socket: PathBuf,
    pub supervisor_record: PathBuf,
    pub supervisor_plist: PathBuf,
    pub stdout_log: PathBuf,
    pub stderr_log: PathBuf,
}

impl MctStartupPaths {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        root: PathBuf,
        ledger: PathBuf,
        state: PathBuf,
        config: PathBuf,
        identity: PathBuf,
        children: PathBuf,
        control_socket: PathBuf,
        supervisor_record: PathBuf,
        supervisor_plist: PathBuf,
        stdout_log: PathBuf,
        stderr_log: PathBuf,
    ) -> Self {
        Self {
            root,
            ledger,
            state,
            config,
            identity,
            children,
            control_socket,
            supervisor_record,
            supervisor_plist,
            stdout_log,
            stderr_log,
        }
    }

    pub fn supervised(root: impl AsRef<Path>, supervisor_plist: PathBuf) -> Self {
        let root = root.as_ref().to_path_buf();
        Self::new(
            root.clone(),
            root.join("observations.jsonl"),
            root.join("state.sqlite"),
            root.join("config.json"),
            root.join("identity/iroh-secret.hex"),
            root.join("children"),
            root.join("control.sock"),
            root.join("supervisor.json"),
            supervisor_plist,
            root.join("logs/mother.stdout.log"),
            root.join("logs/mother.stderr.log"),
        )
    }
}

fn file_type(metadata: &std::fs::Metadata) -> MctStartupArtifactFileTypeV1 {
    let kind = metadata.file_type();
    if kind.is_file() {
        MctStartupArtifactFileTypeV1::File
    } else if kind.is_dir() {
        MctStartupArtifactFileTypeV1::Directory
    } else if kind.is_symlink() {
        MctStartupArtifactFileTypeV1::Symlink
    } else {
        MctStartupArtifactFileTypeV1::Other
    }
}

fn inspect_path(
    entries: &mut Vec<MctStartupArtifactEntryV1>,
    artifact_class: MctStartupArtifactClassV1,
    path: PathBuf,
    transient: bool,
) {
    let (state, file_type) = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => (
            if transient {
                MctStartupArtifactStateV1::Transient
            } else {
                MctStartupArtifactStateV1::Present
            },
            Some(file_type(&metadata)),
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (MctStartupArtifactStateV1::Absent, None)
        }
        Err(_) => (MctStartupArtifactStateV1::Unavailable, None),
    };
    entries.push(MctStartupArtifactEntryV1 {
        artifact_class,
        path,
        state,
        file_type,
    });
}

fn inspect_matching_siblings(
    entries: &mut Vec<MctStartupArtifactEntryV1>,
    artifact_class: MctStartupArtifactClassV1,
    target: &Path,
    prefix: &str,
) {
    let Some(parent) = target.parent() else {
        inspect_path(entries, artifact_class, target.to_path_buf(), false);
        return;
    };
    match std::fs::read_dir(parent) {
        Ok(children) => {
            let mut matches = children
                .filter_map(|child| child.ok())
                .map(|child| child.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with(prefix))
                })
                .collect::<Vec<_>>();
            matches.sort();
            if matches.is_empty() {
                entries.push(MctStartupArtifactEntryV1 {
                    artifact_class,
                    path: parent.join(format!("{prefix}*")),
                    state: MctStartupArtifactStateV1::Absent,
                    file_type: None,
                });
            } else {
                for path in matches {
                    inspect_path(entries, artifact_class, path, false);
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            entries.push(MctStartupArtifactEntryV1 {
                artifact_class,
                path: parent.join(format!("{prefix}*")),
                state: MctStartupArtifactStateV1::Absent,
                file_type: None,
            });
        }
        Err(_) => entries.push(MctStartupArtifactEntryV1 {
            artifact_class,
            path: parent.join(format!("{prefix}*")),
            state: MctStartupArtifactStateV1::Unavailable,
            file_type: None,
        }),
    }
}

fn inspect_unknown_root_entries(
    entries: &mut Vec<MctStartupArtifactEntryV1>,
    paths: &MctStartupPaths,
) {
    let known = [
        paths.ledger.clone(),
        paths.state.clone(),
        paths.config.clone(),
        paths.identity.clone(),
        paths.children.clone(),
        paths.control_socket.clone(),
        paths.supervisor_record.clone(),
        paths.stdout_log.clone(),
        paths.stderr_log.clone(),
        paths
            .state
            .parent()
            .unwrap_or(paths.root.as_path())
            .join("blobs"),
    ];
    let known_top_level = known
        .iter()
        .filter_map(|path| path.strip_prefix(&paths.root).ok())
        .filter_map(|relative| relative.components().next())
        .map(|component| paths.root.join(component.as_os_str()))
        .chain([
            paths.root.join("releases"),
            paths.root.join("logs"),
            paths.root.join("identity"),
        ])
        .collect::<std::collections::BTreeSet<_>>();

    match std::fs::read_dir(&paths.root) {
        Ok(children) => {
            let mut unknown = children
                .filter_map(|child| child.ok())
                .map(|child| child.path())
                .filter(|path| !known_top_level.contains(path))
                .collect::<Vec<_>>();
            unknown.sort();
            if unknown.is_empty() {
                entries.push(MctStartupArtifactEntryV1 {
                    artifact_class: MctStartupArtifactClassV1::OtherManagedRootEntry,
                    path: paths.root.join("<other-managed-entry>"),
                    state: MctStartupArtifactStateV1::Absent,
                    file_type: None,
                });
            } else {
                for path in unknown {
                    inspect_path(
                        entries,
                        MctStartupArtifactClassV1::OtherManagedRootEntry,
                        path,
                        false,
                    );
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            entries.push(MctStartupArtifactEntryV1 {
                artifact_class: MctStartupArtifactClassV1::OtherManagedRootEntry,
                path: paths.root.join("<other-managed-entry>"),
                state: MctStartupArtifactStateV1::Absent,
                file_type: None,
            });
        }
        Err(_) => entries.push(MctStartupArtifactEntryV1 {
            artifact_class: MctStartupArtifactClassV1::OtherManagedRootEntry,
            path: paths.root.join("<other-managed-entry>"),
            state: MctStartupArtifactStateV1::Unavailable,
            file_type: None,
        }),
    }
}

pub fn classify_startup_artifacts(
    paths: &MctStartupPaths,
) -> std::io::Result<MctStartupArtifactInventoryV1> {
    let mut entries = Vec::new();
    let state_parent = paths.state.parent().unwrap_or(paths.root.as_path());
    let identity_dir = paths.identity.parent().unwrap_or(paths.root.as_path());
    let logs_dir = paths.stdout_log.parent().unwrap_or(paths.root.as_path());

    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::CanonicalObservationLedger,
        paths.ledger.clone(),
        false,
    );
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::LedgerRecoveryForensics,
        mct_observation::forensic_root_path(&paths.ledger),
        false,
    );
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::RuntimeSqlite,
        paths.state.clone(),
        false,
    );
    for suffix in ["-wal", "-shm", "-journal"] {
        inspect_path(
            &mut entries,
            MctStartupArtifactClassV1::SqliteDurabilitySidecar,
            PathBuf::from(format!("{}{suffix}", paths.state.display())),
            false,
        );
    }
    inspect_matching_siblings(
        &mut entries,
        MctStartupArtifactClassV1::SqliteDurabilitySidecar,
        &paths.state,
        &format!(
            ".{}",
            paths
                .state
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("state.sqlite")
        ),
    );
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::DaemonConfiguration,
        paths.config.clone(),
        false,
    );
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::InterruptedConfigPublication,
        paths.config.with_extension("json.tmp"),
        false,
    );
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::RecordedMotherIdentity,
        identity_dir.to_path_buf(),
        false,
    );
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::RecordedMotherIdentity,
        paths.identity.clone(),
        false,
    );
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::ChildPackageCatalog,
        paths.children.clone(),
        false,
    );
    for prefix in [".acquiring", ".installing-", ".replaced-"] {
        inspect_matching_siblings(
            &mut entries,
            MctStartupArtifactClassV1::InterruptedChildPublication,
            &paths.children.join("placeholder"),
            prefix,
        );
    }
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::ContentAddressedBlobs,
        state_parent.join("blobs"),
        false,
    );
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::DaemonReleaseStore,
        state_parent.join("releases"),
        false,
    );
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::SupervisorLifecycleRecord,
        paths.supervisor_record.clone(),
        false,
    );
    let record_prefix = format!(
        ".{}.",
        paths
            .supervisor_record
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("supervisor.json")
    );
    inspect_matching_siblings(
        &mut entries,
        MctStartupArtifactClassV1::InterruptedSupervisorPublication,
        &paths.supervisor_record,
        &record_prefix,
    );
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::SupervisorPolicy,
        paths.supervisor_plist.clone(),
        false,
    );
    let plist_prefix = format!(
        ".{}.",
        paths
            .supervisor_plist
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("io.patina.mct.mother.plist")
    );
    inspect_matching_siblings(
        &mut entries,
        MctStartupArtifactClassV1::InterruptedSupervisorPolicyPublication,
        &paths.supervisor_plist,
        &plist_prefix,
    );
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::SupervisorLogs,
        logs_dir.to_path_buf(),
        false,
    );
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::SupervisorLogs,
        paths.stdout_log.clone(),
        false,
    );
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::SupervisorLogs,
        paths.stderr_log.clone(),
        false,
    );
    inspect_unknown_root_entries(&mut entries, paths);
    inspect_path(
        &mut entries,
        MctStartupArtifactClassV1::ControlSocket,
        paths.control_socket.clone(),
        true,
    );

    entries.sort_by(|left, right| {
        left.artifact_class
            .cmp(&right.artifact_class)
            .then_with(|| left.path.cmp(&right.path))
    });
    let inventory_hash =
        blake3::hash(&serde_json::to_vec(&entries).map_err(std::io::Error::other)?)
            .to_hex()
            .to_string();
    Ok(MctStartupArtifactInventoryV1 {
        schema: "mct-startup-artifact-inventory/v1".into(),
        entries,
        inventory_hash,
    })
}

pub fn classify_authority_startup(
    paths: &MctStartupPaths,
    ledger_id: &str,
    mother_node_id: &str,
) -> Result<MctAuthorityStartupEvidenceV1, MctStartupClassificationErrorV1> {
    let inventory = classify_startup_artifacts(paths)
        .map_err(|_| MctStartupClassificationErrorV1::DiskEvidenceUnavailable)?;
    if inventory
        .entries
        .iter()
        .any(|entry| entry.state == MctStartupArtifactStateV1::Unavailable)
    {
        return Err(MctStartupClassificationErrorV1::DiskEvidenceUnavailable);
    }
    let ledger_entry = inventory
        .entries
        .iter()
        .find(|entry| entry.artifact_class == MctStartupArtifactClassV1::CanonicalObservationLedger)
        .expect("the fixed inventory always contains the canonical ledger path");
    let entries = match ledger_entry.state {
        MctStartupArtifactStateV1::Absent => Vec::new(),
        MctStartupArtifactStateV1::Present
            if ledger_entry.file_type == Some(MctStartupArtifactFileTypeV1::File) =>
        {
            JsonlObservationLedger::open_read_only(&paths.ledger, ledger_id, mother_node_id)
                .map_err(MctStartupClassificationErrorV1::LedgerUnavailable)?
                .entries()
                .map_err(MctStartupClassificationErrorV1::LedgerUnavailable)?
        }
        MctStartupArtifactStateV1::Present | MctStartupArtifactStateV1::Transient => {
            return Err(MctStartupClassificationErrorV1::LedgerAmbiguous);
        }
        MctStartupArtifactStateV1::Unavailable => {
            return Err(MctStartupClassificationErrorV1::DiskEvidenceUnavailable);
        }
    };
    let replay = replay_authority_entries(&entries).map_err(|error| {
        MctStartupClassificationErrorV1::AuthorityReplayBlocked(error.to_string())
    })?;
    let startup_class = if entries.is_empty() {
        if ledger_entry.state == MctStartupArtifactStateV1::Absent && inventory.proves_virgin() {
            AuthorityStartupClassV1::Virgin
        } else {
            AuthorityStartupClassV1::OperatorGatedNonvirgin
        }
    } else if replay.current_authority.is_some() {
        AuthorityStartupClassV1::OrdinaryReopen
    } else {
        AuthorityStartupClassV1::LegacyLedgerUpgrade
    };
    let validated_head = entries
        .last()
        .map(|entry| (entry.local_sequence, entry.entry_hash.clone()));
    let authority_state_hash = authority_state_hash(&replay.state).map_err(|error| {
        MctStartupClassificationErrorV1::AuthorityReplayBlocked(error.to_string())
    })?;
    Ok(MctAuthorityStartupEvidenceV1 {
        startup_class,
        inventory,
        validated_head,
        prior_authority: replay.current_authority,
        canonical_authority_state: replay.state,
        authority_state_hash,
        ledger_entry_count: entries.len(),
    })
}

pub fn open_classified_authority(
    paths: &MctStartupPaths,
    ledger_id: &str,
    mother_node_id: &str,
    evidence: &MctAuthorityStartupEvidenceV1,
) -> Result<JsonlObservationLedger, MctStartupClassificationErrorV1> {
    if evidence.startup_class == AuthorityStartupClassV1::OperatorGatedNonvirgin {
        return Err(MctStartupClassificationErrorV1::OperatorGateRequired);
    }
    JsonlObservationLedger::open_authority_with_startup(
        &paths.ledger,
        ledger_id,
        mother_node_id,
        evidence.tenure_evidence(None)?,
    )
    .map_err(MctStartupClassificationErrorV1::LedgerUnavailable)
}

pub fn accept_operator_startup_gate(
    paths: &MctStartupPaths,
    ledger_id: &str,
    mother_node_id: &str,
    request: &MctOperatorStartupGateRequestV1,
    authenticated_principal_ref: &str,
    required_owner_principal_ref: &str,
) -> Result<JsonlObservationLedger, MctStartupClassificationErrorV1> {
    let refuse =
        |detail: &str| MctStartupClassificationErrorV1::OperatorGateRefused(detail.to_owned());
    if authenticated_principal_ref != required_owner_principal_ref
        || authenticated_principal_ref.trim().is_empty()
    {
        return Err(refuse(
            "authenticated principal is not the service-root owner",
        ));
    }
    if request.schema != "mct-operator-startup-gate-request/v1"
        || request.decision_id.trim().is_empty()
        || request.expected_mother_node_id != mother_node_id
        || request.expected_ledger_id != ledger_id
        || request.confirmation != MCT_OPERATOR_REINITIALIZATION_CONFIRMATION_V1
    {
        return Err(refuse(
            "operator gate request is malformed or targets another authority",
        ));
    }
    let evidence = classify_authority_startup(paths, ledger_id, mother_node_id)?;
    if evidence.startup_class != AuthorityStartupClassV1::OperatorGatedNonvirgin {
        return Err(refuse(
            "operator gate is not legal in the current startup class",
        ));
    }
    if request.expected_inventory_hash != evidence.inventory.inventory_hash {
        return Err(refuse("operator gate inventory changed"));
    }
    let gate = MctAcceptedStartupGateV1 {
        decision_id: request.decision_id.clone(),
        authenticated_principal_ref: authenticated_principal_ref.to_owned(),
        inventory_hash: evidence.inventory.inventory_hash.clone(),
    };
    let mut ledger = JsonlObservationLedger::open_authority_with_startup(
        &paths.ledger,
        ledger_id,
        mother_node_id,
        evidence.tenure_evidence(Some(&gate))?,
    )
    .map_err(MctStartupClassificationErrorV1::LedgerUnavailable)?;
    let tenure = ledger
        .authority_tenure()
        .expect("explicit authority open exposes an acknowledged epoch");
    let epoch_observation_id = tenure
        .fact
        .resulting_authority
        .source_authority_observation_id
        .clone();
    let now = jiff::Timestamp::now().to_string();
    let correlation = serde_json::json!({
        "schema": "mct-startup-gate-correlation/v1",
        "decision_id": gate.decision_id,
        "authenticated_principal_ref": gate.authenticated_principal_ref,
        "inventory_hash": gate.inventory_hash,
        "epoch_observation_id": epoch_observation_id,
        "startup_class": AuthorityStartupClassV1::OperatorGatedNonvirgin,
    });
    let make_observation =
        |suffix: &str, kind: ObservationKind, source_plane: SourcePlane, message: &str| {
            MctObservation {
                observation_id: ObservationId::new(format!(
                    "obs:startup-gate:{}:{suffix}",
                    request.decision_id
                ))
                .expect("validated decision id makes a non-empty observation id"),
                observed_at: Timestamp::new(now.clone()).expect("system time is RFC3339"),
                kind,
                source_plane,
                trace: ObservationTraceRef {
                    trace_id: TraceId::new(format!("trace:startup-gate:{}", request.decision_id))
                        .expect("validated decision id makes a non-empty trace id"),
                    span_id: None,
                    parent_span_id: None,
                    external_trace_id: None,
                },
                call_id: None,
                decision_id: None,
                subject_id: Some(authenticated_principal_ref.to_owned()),
                resource_id: Some(ledger_id.to_owned()),
                policy_revision: None,
                grants_revision: Some(0),
                outcome: ObservationOutcome::Completed,
                visibility: ObservationVisibility::NodeOperator,
                safe_message: message.into(),
                detail_ref: Some(format!(
                    "mct-startup-gate-v1:{}",
                    serde_json::to_string(&correlation).expect("correlation JSON serializes")
                )),
            }
        };
    ledger
        .append_batch_before_effect(
            [
                make_observation(
                    "accepted",
                    ObservationKind::OperatorActionRecorded,
                    SourcePlane::Operator,
                    "operator-gated canonical authority reinitialization accepted",
                ),
                make_observation(
                    "startup",
                    ObservationKind::LifecycleTransitionRecorded,
                    SourcePlane::Storage,
                    "operator-gated nonvirgin startup established",
                ),
            ],
            now,
        )
        .map_err(MctStartupClassificationErrorV1::LedgerUnavailable)?;
    Ok(ledger)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctStartupPostureV1 {
    OperatorGateRequired,
    LedgerQuarantined,
    AuthorityReplayBlocked,
    StartupDegradedDeny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctStartupRefusalKindV1 {
    StartupDegradedDeny,
    LedgerQuarantined,
    OperatorGateRequired,
    AuthorityReplayBlocked,
    ProjectionUnusable,
    WriterFenced,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctStartupRefusalV1 {
    pub schema: String,
    pub kind: MctStartupRefusalKindV1,
    pub startup_class: Option<AuthorityStartupClassV1>,
    pub authority_ready: bool,
    pub retryable: bool,
    pub safe_message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctLedgerForensicReportV1 {
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub mother_node_id: String,
    pub failure_class: String,
    pub first_bad_sequence: Option<u64>,
    pub first_bad_offset: Option<u64>,
    pub expected: Option<String>,
    pub observed: Option<String>,
    pub forensic_root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctLedgerForensicCaseV1 {
    pub case_id: String,
    pub failure_class: String,
    pub source_length: u64,
    pub source_digest: String,
    pub source_offset: Option<u64>,
    pub prior_committed_sequence: Option<u64>,
    pub prior_committed_hash: Option<String>,
    pub decision_id: Option<String>,
    pub recorded_at: Option<String>,
    pub source_path: PathBuf,
    pub record_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctStartupPlaneResponseV1 {
    status_code: u16,
    content_type: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
    refusal_kind: Option<MctStartupRefusalKindV1>,
}

impl MctStartupPlaneResponseV1 {
    fn json(status_code: u16, value: &impl Serialize) -> Self {
        Self {
            status_code,
            content_type: "application/json".into(),
            headers: BTreeMap::new(),
            body: serde_json::to_vec(value).unwrap_or_else(|_| b"null".to_vec()),
            refusal_kind: None,
        }
    }

    fn refusal(status_code: u16, refusal: MctStartupRefusalV1) -> Self {
        let kind = refusal.kind;
        let mut response = Self::json(status_code, &refusal);
        response.refusal_kind = Some(kind);
        response
    }

    pub fn status_code(&self) -> u16 {
        self.status_code
    }

    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    pub fn body_bytes(&self) -> &[u8] {
        &self.body
    }

    pub fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    pub fn refusal_kind(&self) -> Option<MctStartupRefusalKindV1> {
        self.refusal_kind
    }

    pub fn first_case_id(&self) -> Option<String> {
        serde_json::from_slice::<Vec<MctLedgerForensicCaseV1>>(&self.body)
            .ok()?
            .first()
            .map(|case| case.case_id.clone())
    }
}

#[derive(Debug)]
pub struct MctIsolatedStartupPlaneV1 {
    paths: MctStartupPaths,
    ledger_id: String,
    mother_node_id: String,
    expected_owner_uid: u32,
    posture: MctStartupPostureV1,
    startup: Option<MctAuthorityStartupEvidenceV1>,
    inventory: MctStartupArtifactInventoryV1,
    ledger_forensics: Option<MctLedgerForensicReportV1>,
    drift_report: Option<AuthorityDriftReportV1>,
}

impl MctIsolatedStartupPlaneV1 {
    pub fn inspect(
        paths: MctStartupPaths,
        ledger_id: &str,
        mother_node_id: &str,
        expected_owner_uid: u32,
    ) -> Result<Self, MctStartupClassificationErrorV1> {
        let inventory = classify_startup_artifacts(&paths)
            .map_err(|_| MctStartupClassificationErrorV1::DiskEvidenceUnavailable)?;
        let (posture, startup, ledger_forensics) =
            match classify_authority_startup(&paths, ledger_id, mother_node_id) {
                Ok(startup)
                    if startup.startup_class == AuthorityStartupClassV1::OperatorGatedNonvirgin =>
                {
                    (
                        MctStartupPostureV1::OperatorGateRequired,
                        Some(startup),
                        None,
                    )
                }
                Ok(startup) => (
                    MctStartupPostureV1::StartupDegradedDeny,
                    Some(startup),
                    None,
                ),
                Err(MctStartupClassificationErrorV1::AuthorityReplayBlocked(_)) => {
                    (MctStartupPostureV1::AuthorityReplayBlocked, None, None)
                }
                Err(MctStartupClassificationErrorV1::LedgerUnavailable(error)) => {
                    match error {
                        ObservationLedgerError::Quarantined { .. }
                        | ObservationLedgerError::ForeignLineage { .. } => {}
                        other => {
                            return Err(MctStartupClassificationErrorV1::LedgerUnavailable(other));
                        }
                    }
                    let preserved = match JsonlObservationLedger::open(
                        &paths.ledger,
                        ledger_id,
                        mother_node_id,
                    ) {
                        Err(ObservationLedgerError::Quarantined { status })
                        | Err(ObservationLedgerError::ForeignLineage { status }) => *status,
                        Err(error) => {
                            return Err(MctStartupClassificationErrorV1::LedgerUnavailable(error));
                        }
                        Ok(_) => {
                            return Err(MctStartupClassificationErrorV1::AuthorityReplayBlocked(
                                "ledger quarantine changed during exclusive rescan".into(),
                            ));
                        }
                    };
                    (
                        MctStartupPostureV1::LedgerQuarantined,
                        None,
                        Some(MctLedgerForensicReportV1::from_status(
                            ledger_id,
                            mother_node_id,
                            &preserved,
                        )),
                    )
                }
                Err(error) => return Err(error),
            };
        Ok(Self {
            paths,
            ledger_id: ledger_id.to_owned(),
            mother_node_id: mother_node_id.to_owned(),
            expected_owner_uid,
            posture,
            startup,
            inventory,
            ledger_forensics,
            drift_report: None,
        })
    }

    pub fn posture(&self) -> MctStartupPostureV1 {
        self.posture
    }

    pub fn with_drift_report(mut self, report: AuthorityDriftReportV1) -> Self {
        self.drift_report = Some(report);
        self
    }

    fn refusal(&self, safe_message: &str) -> MctStartupRefusalV1 {
        let (kind, retryable) = match self.posture {
            MctStartupPostureV1::OperatorGateRequired => {
                (MctStartupRefusalKindV1::OperatorGateRequired, true)
            }
            MctStartupPostureV1::LedgerQuarantined => {
                (MctStartupRefusalKindV1::LedgerQuarantined, false)
            }
            MctStartupPostureV1::AuthorityReplayBlocked => {
                (MctStartupRefusalKindV1::AuthorityReplayBlocked, false)
            }
            MctStartupPostureV1::StartupDegradedDeny => {
                (MctStartupRefusalKindV1::StartupDegradedDeny, true)
            }
        };
        MctStartupRefusalV1 {
            schema: "mct-startup-refusal/v1".into(),
            kind,
            startup_class: self.startup.as_ref().map(|value| value.startup_class),
            authority_ready: false,
            retryable,
            safe_message: safe_message.into(),
        }
    }

    pub fn handle(
        &self,
        peer_uid: u32,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> MctStartupPlaneResponseV1 {
        if peer_uid != self.expected_owner_uid {
            return MctStartupPlaneResponseV1::json(
                403,
                &serde_json::json!({"error": "startup plane owner UID refused"}),
            );
        }
        let path_without_query = path.split('?').next().unwrap_or(path);
        match (method, path_without_query) {
            ("GET", "/status") => MctStartupPlaneResponseV1::json(
                200,
                &serde_json::json!({
                    "version": crate::version(),
                    "health": "unhealthy",
                    "readiness": "not_ready",
                    "startup_posture": self.posture,
                    "authority_ready": false,
                    "iroh_endpoint": null,
                }),
            ),
            ("GET", "/startup") => MctStartupPlaneResponseV1::json(
                200,
                &serde_json::json!({
                    "startup_class": self.startup.as_ref().map(|value| value.startup_class),
                    "startup_posture": self.posture,
                    "authority_ready": false,
                    "inventory": self.inventory,
                }),
            ),
            ("GET", "/forensics/ledger") => self.ledger_forensics.as_ref().map_or_else(
                || {
                    MctStartupPlaneResponseV1::refusal(
                        503,
                        self.refusal("ledger forensic diagnostics are unavailable"),
                    )
                },
                |report| MctStartupPlaneResponseV1::json(200, report),
            ),
            ("GET", "/forensics/cases") => match forensic_cases(&self.paths.ledger) {
                Ok(cases) => MctStartupPlaneResponseV1::json(200, &cases),
                Err(_) => MctStartupPlaneResponseV1::refusal(
                    503,
                    self.refusal("ledger forensic cases are unavailable"),
                ),
            },
            ("GET", source_path)
                if source_path.starts_with("/forensics/cases/")
                    && source_path.ends_with("/source") =>
            {
                match read_forensic_source_range(&self.paths.ledger, path) {
                    Ok(range) => MctStartupPlaneResponseV1 {
                        status_code: 200,
                        content_type: "application/octet-stream".into(),
                        headers: BTreeMap::from([
                            ("x-mct-source-digest".into(), range.digest),
                            (
                                "x-mct-source-total-length".into(),
                                range.total_length.to_string(),
                            ),
                            (
                                "x-mct-source-range".into(),
                                format!("{}-{}", range.start, range.end),
                            ),
                        ]),
                        body: range.bytes,
                        refusal_kind: None,
                    },
                    Err(_) => MctStartupPlaneResponseV1::refusal(
                        400,
                        self.refusal("forensic source range was refused"),
                    ),
                }
            }
            ("GET", "/drift") => self.drift_report.as_ref().map_or_else(
                || {
                    MctStartupPlaneResponseV1::json(
                        200,
                        &serde_json::json!({"status": "unavailable_before_authority_replay"}),
                    )
                },
                |report| MctStartupPlaneResponseV1::json(200, report),
            ),
            ("POST", "/startup/operator-gate")
                if self.posture == MctStartupPostureV1::OperatorGateRequired =>
            {
                let request: MctOperatorStartupGateRequestV1 = match serde_json::from_slice(body) {
                    Ok(request) => request,
                    Err(_) => {
                        return MctStartupPlaneResponseV1::refusal(
                            400,
                            self.refusal("startup operator gate request was malformed"),
                        );
                    }
                };
                let principal = format!("os-uid:{peer_uid}");
                match accept_operator_startup_gate(
                    &self.paths,
                    &self.ledger_id,
                    &self.mother_node_id,
                    &request,
                    &principal,
                    &principal,
                ) {
                    Ok(ledger) => {
                        drop(ledger);
                        MctStartupPlaneResponseV1::json(
                            200,
                            &serde_json::json!({
                                "status": "accepted",
                                "decision_id": request.decision_id,
                                "restart_required": true,
                            }),
                        )
                    }
                    Err(_) => MctStartupPlaneResponseV1::refusal(
                        409,
                        self.refusal("startup operator gate was refused"),
                    ),
                }
            }
            _ => MctStartupPlaneResponseV1::refusal(
                503,
                self.refusal("startup posture denies this route"),
            ),
        }
    }
}

impl MctLedgerForensicReportV1 {
    fn from_status(ledger_id: &str, mother_node_id: &str, status: &LedgerQuarantineStatus) -> Self {
        Self {
            ledger_path: status.ledger_path.clone(),
            ledger_id: ledger_id.to_owned(),
            mother_node_id: mother_node_id.to_owned(),
            failure_class: format!("{:?}", status.failure_class),
            first_bad_sequence: status.first_bad_sequence,
            first_bad_offset: Some(status.first_bad_offset),
            expected: status.expected.clone(),
            observed: status.observed.clone(),
            forensic_root: mct_observation::forensic_root_path(&status.ledger_path),
        }
    }
}

fn forensic_cases(ledger_path: &Path) -> std::io::Result<Vec<MctLedgerForensicCaseV1>> {
    let root = mct_observation::forensic_root_path(ledger_path);
    let mut cases = Vec::new();
    let children = match fs::read_dir(&root) {
        Ok(children) => children,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(cases),
        Err(error) => return Err(error),
    };
    for child in children {
        let path = child?.path();
        if !path.is_dir() {
            continue;
        }
        let Some(case_id) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        let source_path = path.join("source.bin");
        let record_path = path.join("record.json");
        let source = fs::read(&source_path)?;
        let record = fs::read(&record_path)?;
        let mut case = MctLedgerForensicCaseV1 {
            case_id,
            failure_class: "unknown".into(),
            source_length: source.len() as u64,
            source_digest: blake3::hash(&source).to_hex().to_string(),
            source_offset: None,
            prior_committed_sequence: None,
            prior_committed_hash: None,
            decision_id: None,
            recorded_at: None,
            source_path,
            record_path,
        };
        if let Ok(status) = serde_json::from_slice::<LedgerRecoveryStatus>(&record) {
            case.failure_class = status.failure_class;
            case.source_offset = Some(status.residue_offset);
            case.prior_committed_sequence = status.last_committed_sequence;
            case.prior_committed_hash = status.last_committed_hash;
            case.decision_id = Some(status.recovery_decision_id);
            case.recorded_at = Some(status.recovery_time);
        } else if let Ok(status) = serde_json::from_slice::<LedgerQuarantineStatus>(&record) {
            case.failure_class = format!("{:?}", status.failure_class);
            case.source_offset = Some(status.first_bad_offset);
            case.prior_committed_sequence = status
                .first_bad_sequence
                .and_then(|value| value.checked_sub(1));
        } else {
            continue;
        }
        cases.push(case);
    }
    cases.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    Ok(cases)
}

struct MctForensicSourceRange {
    bytes: Vec<u8>,
    digest: String,
    total_length: u64,
    start: u64,
    end: u64,
}

fn read_forensic_source_range(
    ledger_path: &Path,
    request_path: &str,
) -> std::io::Result<MctForensicSourceRange> {
    const MAX_RANGE: u64 = 64 * 1024;
    let (path, query) = request_path.split_once('?').ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "explicit range is required",
        )
    })?;
    let case_id = path
        .strip_prefix("/forensics/cases/")
        .and_then(|path| path.strip_suffix("/source"))
        .filter(|value| {
            !value.is_empty()
                && !value.contains('/')
                && !value.contains('\\')
                && *value != "."
                && *value != ".."
        })
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid case id"))?;
    let mut start = None;
    let mut end = None;
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("start=") {
            start = value.parse::<u64>().ok();
        } else if let Some(value) = pair.strip_prefix("end=") {
            end = value.parse::<u64>().ok();
        }
    }
    let (start, end) = start
        .zip(end)
        .filter(|(start, end)| start <= end && end - start <= MAX_RANGE)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid range"))?;
    let cases = forensic_cases(ledger_path)?;
    let case = cases
        .iter()
        .find(|case| case.case_id == case_id)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "case not found"))?;
    if end > case.source_length {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "range exceeds source length",
        ));
    }
    let mut file = fs::File::open(&case.source_path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = vec![0; (end - start) as usize];
    file.read_exact(&mut bytes)?;
    Ok(MctForensicSourceRange {
        bytes,
        digest: case.source_digest.clone(),
        total_length: case.source_length,
        start,
        end,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctAuthorityProjectionDriftStatusV1 {
    Missing,
    Rebuilding,
    Current,
    Stale,
    Quarantined,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctLegacyAuthoritySourceV1 {
    ConfigAuthorityIntent,
    SqliteToyAuthority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctLegacyAuthorityComparisonV1 {
    NoAuthorityIntent,
    MatchesCanonical,
    DiffersFromCanonical,
    ImportRequired,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctLegacyAuthorityInputV1 {
    pub source: MctLegacyAuthoritySourceV1,
    pub normalized_hash: Option<String>,
    pub comparison: MctLegacyAuthorityComparisonV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctCanonicalAuthorityDriftV1 {
    pub mother_node_id: String,
    pub ledger_id: String,
    pub head_sequence: u64,
    pub head_entry_hash: String,
    pub grants_authority: GrantsAuthorityIdentityV1,
    pub authority_state_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctProjectionAuthorityDriftV1 {
    pub status: MctAuthorityProjectionDriftStatusV1,
    pub through_sequence: Option<u64>,
    pub through_entry_hash: Option<String>,
    pub projection_hash: Option<String>,
    pub proof_denial: Option<AuthorityProjectionDenyReasonV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityDriftReportV1 {
    pub schema: String,
    pub report_id: String,
    pub observed_at: String,
    pub startup_class: AuthorityStartupClassV1,
    pub canonical: MctCanonicalAuthorityDriftV1,
    pub projection: MctProjectionAuthorityDriftV1,
    pub legacy_inputs: Vec<MctLegacyAuthorityInputV1>,
    pub blocking_reasons: Vec<String>,
    pub authority_ready: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctStartupAuthorityReadinessV1 {
    pub report: AuthorityDriftReportV1,
    pub proof: UsableAuthorityProjectionProofV1,
    pub authority_ready: bool,
}

impl MctStartupAuthorityReadinessV1 {
    pub fn cursor(&self) -> Option<&AuthorityProjectionCursorV1> {
        match &self.proof {
            UsableAuthorityProjectionProofV1::Usable { cursor } => Some(cursor),
            UsableAuthorityProjectionProofV1::Denied { .. } => None,
        }
    }
}

fn state_is_subset(candidate: &AuthorityStateV1, canonical: &AuthorityStateV1) -> bool {
    candidate.toy_catalog.iter().all(|(id, value)| {
        canonical
            .toy_catalog
            .get(id)
            .is_some_and(|current| current == value)
    }) && candidate.toy_grants.iter().all(|(id, value)| {
        canonical
            .toy_grants
            .get(id)
            .is_some_and(|current| current == value)
    })
}

struct StartupObservationContext<'a> {
    now: &'a str,
    mother_node_id: &'a str,
    ledger_id: &'a str,
    generation: u64,
}

fn startup_observation(
    context: &StartupObservationContext<'_>,
    id: String,
    kind: ObservationKind,
    source_plane: SourcePlane,
    safe_message: String,
    detail_ref: String,
) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::new(id.clone()).expect("startup id is non-empty"),
        observed_at: Timestamp::new(context.now.to_owned()).expect("system time is RFC3339"),
        kind,
        source_plane,
        trace: ObservationTraceRef {
            trace_id: TraceId::new(format!("trace:{id}")).expect("startup trace is non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(context.mother_node_id.to_owned()),
        resource_id: Some(context.ledger_id.to_owned()),
        policy_revision: None,
        grants_revision: Some(context.generation),
        outcome: ObservationOutcome::Completed,
        visibility: ObservationVisibility::NodeOperator,
        safe_message,
        detail_ref: Some(detail_ref),
    }
}

pub fn finalize_authority_startup(
    ledger: &mut JsonlObservationLedger,
    state_path: &Path,
    config_path: &Path,
) -> anyhow::Result<MctStartupAuthorityReadinessV1> {
    let tenure_fact = ledger
        .authority_tenure()
        .context("startup finalization requires an established authority tenure")?
        .fact
        .clone();
    let startup_class = tenure_fact.establishment.startup_class;
    let generation = tenure_fact.resulting_authority.generation;
    let mother_node_id = tenure_fact.mother_node_id.clone();
    let ledger_id = tenure_fact.ledger_id.clone();
    let pre_observation_entries = ledger.entries()?;
    let pre_observation_replay = replay_authority_entries(&pre_observation_entries)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let canonical_hash = authority_state_hash(&pre_observation_replay.state)?;
    let config_hash = authority_state_hash(&AuthorityStateV1::default())?;
    let config_input = match fs::symlink_metadata(config_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => MctLegacyAuthorityInputV1 {
            source: MctLegacyAuthoritySourceV1::ConfigAuthorityIntent,
            normalized_hash: Some(config_hash.clone()),
            comparison: MctLegacyAuthorityComparisonV1::NoAuthorityIntent,
        },
        Ok(metadata) if metadata.file_type().is_file() => {
            match crate::MctDaemonConfigStore::new(config_path).load() {
                Ok(_) => MctLegacyAuthorityInputV1 {
                    source: MctLegacyAuthoritySourceV1::ConfigAuthorityIntent,
                    normalized_hash: Some(config_hash.clone()),
                    comparison: MctLegacyAuthorityComparisonV1::NoAuthorityIntent,
                },
                Err(_) => MctLegacyAuthorityInputV1 {
                    source: MctLegacyAuthoritySourceV1::ConfigAuthorityIntent,
                    normalized_hash: None,
                    comparison: MctLegacyAuthorityComparisonV1::Unavailable,
                },
            }
        }
        _ => MctLegacyAuthorityInputV1 {
            source: MctLegacyAuthoritySourceV1::ConfigAuthorityIntent,
            normalized_hash: None,
            comparison: MctLegacyAuthorityComparisonV1::Unavailable,
        },
    };

    let state = crate::MctRuntimeStateStore::open(state_path)?;
    let sqlite_authority = AuthorityStateV1 {
        toy_catalog: state
            .toy_contracts()?
            .into_iter()
            .map(|contract| (contract.toy_id.to_string(), contract))
            .collect(),
        toy_grants: state
            .toy_grant_snapshots()?
            .into_iter()
            .map(|grant| (grant.grant_id.to_string(), grant))
            .collect(),
    };
    let sqlite_hash = authority_state_hash(&sqlite_authority)?;
    let sqlite_comparison = if !pre_observation_replay.imported
        && pre_observation_replay.mutations.is_empty()
        && (!sqlite_authority.toy_catalog.is_empty() || !sqlite_authority.toy_grants.is_empty())
    {
        MctLegacyAuthorityComparisonV1::ImportRequired
    } else if sqlite_hash == canonical_hash {
        MctLegacyAuthorityComparisonV1::MatchesCanonical
    } else {
        MctLegacyAuthorityComparisonV1::DiffersFromCanonical
    };
    let sqlite_input = MctLegacyAuthorityInputV1 {
        source: MctLegacyAuthoritySourceV1::SqliteToyAuthority,
        normalized_hash: Some(sqlite_hash),
        comparison: sqlite_comparison,
    };
    let mut blocking_reasons = Vec::new();
    if config_input.comparison == MctLegacyAuthorityComparisonV1::Unavailable {
        blocking_reasons.push("config_authority_unavailable".into());
    }
    if sqlite_comparison == MctLegacyAuthorityComparisonV1::ImportRequired {
        blocking_reasons.push("legacy_import_required".into());
    } else if sqlite_comparison == MctLegacyAuthorityComparisonV1::DiffersFromCanonical
        && !state_is_subset(&sqlite_authority, &pre_observation_replay.state)
    {
        blocking_reasons.push("sqlite_authority_broader_than_canonical".into());
    }
    blocking_reasons.sort();
    blocking_reasons.dedup();

    let now = jiff::Timestamp::now().to_string();
    let report_id = format!(
        "startup-drift:{}:{}",
        tenure_fact.authority_epoch,
        pre_observation_entries.len()
    );
    let normalized_inputs = serde_json::json!({
        "schema": "mct-startup-posture/v1",
        "startup_class": startup_class,
        "report_id": report_id,
        "config_authority_hash": config_input.normalized_hash,
        "sqlite_authority_hash": sqlite_input.normalized_hash,
        "canonical_authority_state_hash": canonical_hash,
        "blocking_reasons": blocking_reasons,
    });
    let observation_context = StartupObservationContext {
        now: &now,
        mother_node_id: &mother_node_id,
        ledger_id: &ledger_id,
        generation,
    };
    ledger.append_batch_before_effect(
        [
            startup_observation(
                &observation_context,
                format!("obs:startup-posture:{}", tenure_fact.authority_epoch),
                ObservationKind::LifecycleTransitionRecorded,
                SourcePlane::Storage,
                format!("Mother startup class {startup_class:?} recorded"),
                format!(
                    "mct-startup-posture-v1:{}",
                    serde_json::to_string(&normalized_inputs)?
                ),
            ),
            startup_observation(
                &observation_context,
                format!("obs:startup-drift:{}", tenure_fact.authority_epoch),
                ObservationKind::NodeHealthReported,
                SourcePlane::Storage,
                format!(
                    "authority drift evaluated report_id={report_id} authority_ready={} blocking={}",
                    blocking_reasons.is_empty(),
                    blocking_reasons.join(",")
                ),
                format!(
                    "mct-authority-drift-v1:{}",
                    serde_json::to_string(&normalized_inputs)?
                ),
            ),
        ],
        now.clone(),
    )?;

    let entries = ledger.entries()?;
    let replay =
        replay_authority_entries(&entries).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let head = entries.last().context("startup ledger head disappeared")?;
    let authority = replay
        .current_authority
        .clone()
        .context("startup authority disappeared after observations")?;
    let final_state_hash = authority_state_hash(&replay.state)?;
    let expectation = AuthorityProjectionExpectationV1 {
        source_mother_node_id: head.mother_node_id.clone(),
        source_ledger_id: head.ledger_id.clone(),
        through_sequence: head.local_sequence,
        through_entry_hash: head.entry_hash.clone(),
        grants_authority: authority.clone(),
        authority_state_hash: final_state_hash.clone(),
    };
    let publish_result = state.rebuild_authority_projection(&entries);
    let proof = state.usable_authority_projection_proof(
        &AuthorityProjectionLedgerEvidenceV1::Validated(expectation),
    )?;
    let proof_denial = match &proof {
        UsableAuthorityProjectionProofV1::Usable { .. } => None,
        UsableAuthorityProjectionProofV1::Denied { reason } => Some(*reason),
    };
    if publish_result.is_err() && proof_denial.is_none() {
        blocking_reasons.push("projection_publication_failed".into());
    }
    if let Some(reason) = proof_denial {
        blocking_reasons.push(format!("projection_{reason:?}").to_lowercase());
    }
    blocking_reasons.sort();
    blocking_reasons.dedup();
    let snapshot = state.authority_projection_snapshot()?;
    let projection = snapshot.as_ref().map_or(
        MctProjectionAuthorityDriftV1 {
            status: MctAuthorityProjectionDriftStatusV1::Missing,
            through_sequence: None,
            through_entry_hash: None,
            projection_hash: None,
            proof_denial,
        },
        |snapshot| MctProjectionAuthorityDriftV1 {
            status: match snapshot.cursor.projection_status {
                mct_observation::AuthorityProjectionStatusV1::Current => {
                    MctAuthorityProjectionDriftStatusV1::Current
                }
                mct_observation::AuthorityProjectionStatusV1::Stale => {
                    MctAuthorityProjectionDriftStatusV1::Stale
                }
                mct_observation::AuthorityProjectionStatusV1::Rebuilding => {
                    MctAuthorityProjectionDriftStatusV1::Rebuilding
                }
                mct_observation::AuthorityProjectionStatusV1::Quarantined => {
                    MctAuthorityProjectionDriftStatusV1::Quarantined
                }
            },
            through_sequence: Some(snapshot.cursor.through_sequence),
            through_entry_hash: Some(snapshot.cursor.through_entry_hash.clone()),
            projection_hash: Some(snapshot.cursor.projection_hash.clone()),
            proof_denial,
        },
    );
    let authority_ready = proof_denial.is_none() && blocking_reasons.is_empty();
    let report = AuthorityDriftReportV1 {
        schema: "mct-authority-drift-report/v1".into(),
        report_id,
        observed_at: now,
        startup_class,
        canonical: MctCanonicalAuthorityDriftV1 {
            mother_node_id,
            ledger_id,
            head_sequence: head.local_sequence,
            head_entry_hash: head.entry_hash.clone(),
            grants_authority: authority,
            authority_state_hash: final_state_hash,
        },
        projection,
        legacy_inputs: vec![config_input, sqlite_input],
        blocking_reasons,
        authority_ready,
    };
    Ok(MctStartupAuthorityReadinessV1 {
        report,
        proof,
        authority_ready,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write as _};

    fn paths(root: &Path) -> MctStartupPaths {
        MctStartupPaths::supervised(root, root.join("io.patina.mct.mother.plist"))
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, []).unwrap();
    }

    #[test]
    fn virgin_and_reopen_epoch_establishment_consumes_exact_startup_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths(dir.path());
        let evidence = classify_authority_startup(&paths, "ledger-a", "mother-a").unwrap();
        assert!(!paths.ledger.exists());
        let first = open_classified_authority(&paths, "ledger-a", "mother-a", &evidence).unwrap();
        let first_tenure = first.authority_tenure().unwrap();
        assert_eq!(first_tenure.entry.local_sequence, 0);
        assert_eq!(
            first_tenure.fact.establishment.startup_class,
            AuthorityStartupClassV1::Virgin
        );
        assert_eq!(
            first_tenure.fact.predecessor,
            AuthorityEpochPredecessorV1::NoneForVirgin
        );
        assert_eq!(first_tenure.fact.generation_baseline, 0);
        assert!(first_tenure.fact.prior_authority.is_none());
        let first_authority = first_tenure.fact.resulting_authority.clone();
        let first_head = first.entries().unwrap().last().unwrap().clone();
        drop(first);

        let reopened_evidence = classify_authority_startup(&paths, "ledger-a", "mother-a").unwrap();
        let reopened =
            open_classified_authority(&paths, "ledger-a", "mother-a", &reopened_evidence).unwrap();
        let fact = &reopened.authority_tenure().unwrap().fact;
        assert_eq!(
            fact.establishment.startup_class,
            AuthorityStartupClassV1::OrdinaryReopen
        );
        assert_eq!(fact.prior_authority.as_ref(), Some(&first_authority));
        assert_eq!(fact.generation_baseline, first_authority.generation);
        assert_eq!(
            fact.predecessor,
            AuthorityEpochPredecessorV1::ValidatedHead {
                sequence: first_head.local_sequence,
                entry_hash: first_head.entry_hash,
            },
            "ordinary reopen must name the exact validated canonical head"
        );
        assert!(fact.establishment.operator_gate_decision_id.is_none());
        assert!(fact.establishment.authenticated_principal_ref.is_none());
    }

    #[test]
    fn operator_gate_refusals_are_prewrite_and_acceptance_embeds_authenticated_decision() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths(dir.path());
        touch(&paths.state);
        let initial = classify_authority_startup(&paths, "ledger-a", "mother-a").unwrap();
        assert_eq!(
            initial.startup_class,
            AuthorityStartupClassV1::OperatorGatedNonvirgin
        );
        let request = MctOperatorStartupGateRequestV1 {
            schema: "mct-operator-startup-gate-request/v1".into(),
            decision_id: "decision:gate-1".into(),
            expected_mother_node_id: "mother-a".into(),
            expected_ledger_id: "ledger-a".into(),
            expected_inventory_hash: initial.inventory.inventory_hash.clone(),
            confirmation: MCT_OPERATOR_REINITIALIZATION_CONFIRMATION_V1.into(),
        };

        let non_owner = accept_operator_startup_gate(
            &paths,
            "ledger-a",
            "mother-a",
            &request,
            "os-uid:502",
            "os-uid:501",
        );
        assert!(matches!(
            non_owner,
            Err(MctStartupClassificationErrorV1::OperatorGateRefused(_))
        ));
        assert!(!paths.ledger.exists());

        let mut malformed = request.clone();
        malformed.confirmation = "yes".into();
        assert!(
            accept_operator_startup_gate(
                &paths,
                "ledger-a",
                "mother-a",
                &malformed,
                "os-uid:501",
                "os-uid:501",
            )
            .is_err()
        );
        assert!(!paths.ledger.exists());

        touch(&paths.config);
        let stale = accept_operator_startup_gate(
            &paths,
            "ledger-a",
            "mother-a",
            &request,
            "os-uid:501",
            "os-uid:501",
        );
        assert!(matches!(
            stale,
            Err(MctStartupClassificationErrorV1::OperatorGateRefused(_))
        ));
        assert!(!paths.ledger.exists());

        let current = classify_authority_startup(&paths, "ledger-a", "mother-a").unwrap();
        let accepted_request = MctOperatorStartupGateRequestV1 {
            expected_inventory_hash: current.inventory.inventory_hash,
            ..request
        };
        let mut ledger = accept_operator_startup_gate(
            &paths,
            "ledger-a",
            "mother-a",
            &accepted_request,
            "os-uid:501",
            "os-uid:501",
        )
        .unwrap();
        let entries = ledger.entries().unwrap();
        let fact = &ledger.authority_tenure().unwrap().fact;
        assert_eq!(entries[0], ledger.authority_tenure().unwrap().entry);
        assert_eq!(entries.len(), 3);
        assert_eq!(
            fact.establishment.startup_class,
            AuthorityStartupClassV1::OperatorGatedNonvirgin
        );
        assert_eq!(
            fact.predecessor,
            AuthorityEpochPredecessorV1::NoneAfterOperatorReinitialization
        );
        assert_eq!(fact.generation_baseline, 0);
        assert!(fact.prior_authority.is_none());
        assert_eq!(
            fact.establishment.operator_gate_decision_id.as_deref(),
            Some("decision:gate-1")
        );
        assert_eq!(
            fact.establishment.authenticated_principal_ref.as_deref(),
            Some("os-uid:501")
        );
        assert!(entries[1..].iter().all(|entry| {
            entry
                .observation
                .detail_ref
                .as_deref()
                .is_some_and(|detail| {
                    detail.starts_with("mct-startup-gate-v1:")
                        && !detail.starts_with(mct_observation::AUTHORITY_FACT_DETAIL_PREFIX)
                })
        }));

        let empty_hash = authority_state_hash(&AuthorityStateV1::default()).unwrap();
        let import_request = mct_observation::LegacyAuthorityImportRequestV1 {
            schema: "mct-legacy-authority-import-request/v1".into(),
            import_id: "reinitialized-import".into(),
            expected_mother_node_id: "mother-a".into(),
            expected_ledger_id: "ledger-a".into(),
            expected_config_authority_hash: empty_hash.clone(),
            expected_sqlite_authority_hash: empty_hash,
            confirmation: mct_observation::LEGACY_AUTHORITY_IMPORT_CONFIRMATION_V1.into(),
        };
        let first_import = ledger.execute_legacy_authority_import(
            import_request.clone(),
            "os-uid:501".into(),
            AuthorityStateV1::default(),
            "2026-08-04T00:01:00Z".into(),
        );
        assert!(matches!(
            first_import,
            mct_observation::AuthorityMutationResultV1::CommittedProjectionPending { .. }
        ));
        let second_import = ledger.execute_legacy_authority_import(
            mct_observation::LegacyAuthorityImportRequestV1 {
                import_id: "reinitialized-import-2".into(),
                ..import_request
            },
            "os-uid:501".into(),
            AuthorityStateV1::default(),
            "2026-08-04T00:02:00Z".into(),
        );
        assert!(matches!(
            second_import,
            mct_observation::AuthorityMutationResultV1::RejectedBeforeCommit {
                reason: mct_observation::AuthorityMutationRejectionReasonV1::AlreadyImported,
                ..
            }
        ));
    }

    #[test]
    fn reinitialization_import_is_scoped_to_the_surviving_canonical_history() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths(dir.path());
        let evidence = classify_authority_startup(&paths, "ledger-a", "mother-a").unwrap();
        let mut abandoned =
            open_classified_authority(&paths, "ledger-a", "mother-a", &evidence).unwrap();
        let empty_hash = authority_state_hash(&AuthorityStateV1::default()).unwrap();
        let import_request = |import_id: &str| mct_observation::LegacyAuthorityImportRequestV1 {
            schema: "mct-legacy-authority-import-request/v1".into(),
            import_id: import_id.into(),
            expected_mother_node_id: "mother-a".into(),
            expected_ledger_id: "ledger-a".into(),
            expected_config_authority_hash: empty_hash.clone(),
            expected_sqlite_authority_hash: empty_hash.clone(),
            confirmation: mct_observation::LEGACY_AUTHORITY_IMPORT_CONFIRMATION_V1.into(),
        };
        assert!(matches!(
            abandoned.execute_legacy_authority_import(
                import_request("abandoned-import"),
                "os-uid:501".into(),
                AuthorityStateV1::default(),
                "2026-08-04T00:01:00Z".into(),
            ),
            mct_observation::AuthorityMutationResultV1::CommittedProjectionPending { .. }
        ));
        drop(abandoned);
        let abandoned_bytes = fs::read(&paths.ledger).unwrap();
        assert!(String::from_utf8_lossy(&abandoned_bytes).contains("abandoned-import"));

        fs::remove_file(&paths.ledger).unwrap();
        touch(&paths.state);
        let gated = classify_authority_startup(&paths, "ledger-a", "mother-a").unwrap();
        let request = MctOperatorStartupGateRequestV1 {
            schema: "mct-operator-startup-gate-request/v1".into(),
            decision_id: "decision:new-history".into(),
            expected_mother_node_id: "mother-a".into(),
            expected_ledger_id: "ledger-a".into(),
            expected_inventory_hash: gated.inventory.inventory_hash,
            confirmation: MCT_OPERATOR_REINITIALIZATION_CONFIRMATION_V1.into(),
        };
        let mut current = accept_operator_startup_gate(
            &paths,
            "ledger-a",
            "mother-a",
            &request,
            "os-uid:501",
            "os-uid:501",
        )
        .unwrap();
        let result = current.execute_legacy_authority_import(
            import_request("current-import"),
            "os-uid:501".into(),
            AuthorityStateV1::default(),
            "2026-08-04T00:02:00Z".into(),
        );
        assert!(matches!(
            result,
            mct_observation::AuthorityMutationResultV1::CommittedProjectionPending { .. }
        ));
        let replay = replay_authority_entries(&current.entries().unwrap()).unwrap();
        assert_eq!(replay.import.unwrap().fact.import_id, "current-import");
    }

    #[test]
    fn quarantine_plane_is_owner_only_read_only_and_preserves_exact_forensics() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths(dir.path());
        {
            let mut ledger =
                JsonlObservationLedger::open(&paths.ledger, "ledger-a", "mother-a").unwrap();
            ledger
                .append_before_effect(
                    mct_kernel::MctObservation::informational(
                        mct_kernel::ObservationId::new("obs:good").unwrap(),
                        mct_kernel::Timestamp::new("2026-08-04T00:00:00Z").unwrap(),
                        mct_kernel::ObservationKind::LifecycleTransitionRecorded,
                        mct_kernel::TraceId::new("trace:good").unwrap(),
                        "good predecessor",
                    ),
                    "2026-08-04T00:00:00Z",
                )
                .unwrap();
        }
        fs::OpenOptions::new()
            .append(true)
            .open(&paths.ledger)
            .unwrap()
            .write_all(b"{terminated-corruption}\n")
            .unwrap();
        fs::write(&paths.state, b"prior-projection-truth").unwrap();
        fs::create_dir_all(paths.identity.parent().unwrap()).unwrap();
        fs::write(&paths.identity, b"identity-secret-material").unwrap();
        fs::write(&paths.config, b"config-secret-material").unwrap();
        fs::create_dir_all(paths.root.join("blobs/blake3/aa")).unwrap();
        fs::write(
            paths.root.join("blobs/blake3/aa/payload.blob"),
            b"blob-payload-material",
        )
        .unwrap();
        let ledger_before = fs::read(&paths.ledger).unwrap();
        let projection_before = fs::read(&paths.state).unwrap();

        let plane =
            MctIsolatedStartupPlaneV1::inspect(paths.clone(), "ledger-a", "mother-a", 501).unwrap();
        assert!(matches!(
            plane.posture(),
            MctStartupPostureV1::LedgerQuarantined
        ));
        assert_eq!(
            plane
                .handle(502, "GET", "/forensics/ledger", &[])
                .status_code(),
            403
        );
        let cases = plane.handle(501, "GET", "/forensics/cases", &[]);
        let case_id = cases.first_case_id().unwrap();
        let source = plane.handle(
            501,
            "GET",
            &format!(
                "/forensics/cases/{case_id}/source?start=0&end={}",
                ledger_before.len()
            ),
            &[],
        );
        assert_eq!(source.body_bytes(), ledger_before);
        assert_eq!(
            source.headers().get("x-mct-source-total-length"),
            Some(&ledger_before.len().to_string())
        );
        assert_eq!(
            source.headers().get("x-mct-source-digest"),
            Some(&blake3::hash(&ledger_before).to_hex().to_string())
        );
        assert_eq!(fs::read(&paths.ledger).unwrap(), ledger_before);
        assert_eq!(fs::read(&paths.state).unwrap(), projection_before);
        for response in [
            plane.handle(501, "GET", "/status", &[]),
            plane.handle(501, "GET", "/startup", &[]),
            plane.handle(501, "GET", "/forensics/ledger", &[]),
            cases,
            plane.handle(501, "GET", "/drift", &[]),
        ] {
            let text = String::from_utf8_lossy(response.body_bytes());
            assert!(!text.contains("identity-secret-material"));
            assert!(!text.contains("config-secret-material"));
            assert!(!text.contains("blob-payload-material"));
            assert!(!text.contains("prior-projection-truth"));
        }
        for denied_path in [
            "/startup/operator-gate",
            "/calls",
            "/toys/authorize-secret",
            "/unknown-mutation",
        ] {
            assert_eq!(
                plane.handle(501, "POST", denied_path, b"{}").refusal_kind(),
                Some(MctStartupRefusalKindV1::LedgerQuarantined),
                "quarantine must return one typed refusal for {denied_path}"
            );
        }
    }

    #[test]
    fn foreign_lineage_enters_the_same_nonmutating_quarantine_plane() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths(dir.path());
        {
            let mut foreign =
                JsonlObservationLedger::open(&paths.ledger, "ledger-foreign", "mother-foreign")
                    .unwrap();
            foreign
                .append_before_effect(
                    mct_kernel::MctObservation::informational(
                        mct_kernel::ObservationId::new("obs:foreign").unwrap(),
                        mct_kernel::Timestamp::new("2026-08-04T00:00:00Z").unwrap(),
                        mct_kernel::ObservationKind::LifecycleTransitionRecorded,
                        mct_kernel::TraceId::new("trace:foreign").unwrap(),
                        "foreign lineage",
                    ),
                    "2026-08-04T00:00:00Z",
                )
                .unwrap();
        }
        let before = fs::read(&paths.ledger).unwrap();
        let plane =
            MctIsolatedStartupPlaneV1::inspect(paths.clone(), "ledger-a", "mother-a", 501).unwrap();
        assert_eq!(plane.posture(), MctStartupPostureV1::LedgerQuarantined);
        assert_eq!(fs::read(&paths.ledger).unwrap(), before);
        assert_eq!(
            plane
                .handle(501, "POST", "/startup/operator-gate", b"{}")
                .refusal_kind(),
            Some(MctStartupRefusalKindV1::LedgerQuarantined),
            "operator gating can never adopt foreign lineage"
        );
    }

    #[test]
    fn startup_observations_are_projected_before_readiness_and_drift_never_repairs() {
        let ready_dir = tempfile::tempdir().unwrap();
        let ready_paths = paths(ready_dir.path());
        let evidence = classify_authority_startup(&ready_paths, "ledger-a", "mother-a").unwrap();
        let mut ledger =
            open_classified_authority(&ready_paths, "ledger-a", "mother-a", &evidence).unwrap();
        let ready =
            finalize_authority_startup(&mut ledger, &ready_paths.state, &ready_paths.config)
                .unwrap();
        let entries = ledger.entries().unwrap();
        let head = entries.last().unwrap();
        assert!(
            ready.authority_ready,
            "matching canonical/projection/legacy inputs must become ready"
        );
        assert_eq!(ready.report.canonical.head_sequence, head.local_sequence);
        assert_eq!(ready.report.canonical.head_entry_hash, head.entry_hash);
        assert_eq!(
            ready.cursor().unwrap().through_sequence,
            head.local_sequence
        );
        assert_eq!(
            entries[entries.len() - 2].observation.kind,
            ObservationKind::LifecycleTransitionRecorded
        );
        assert_eq!(
            entries[entries.len() - 1].observation.kind,
            ObservationKind::NodeHealthReported
        );
        assert!(entries[entries.len() - 2..].iter().all(|entry| {
            entry
                .observation
                .detail_ref
                .as_deref()
                .is_some_and(|detail| {
                    !detail.starts_with(mct_observation::AUTHORITY_FACT_DETAIL_PREFIX)
                })
        }));
        assert_eq!(
            replay_authority_entries(&entries)
                .unwrap()
                .canonical_fact_count,
            1,
            "startup and drift observability must add no canonical fact kind"
        );

        drop(ledger);
        let reopened_evidence =
            classify_authority_startup(&ready_paths, "ledger-a", "mother-a").unwrap();
        let mut reopened =
            open_classified_authority(&ready_paths, "ledger-a", "mother-a", &reopened_evidence)
                .unwrap();
        let restarted =
            finalize_authority_startup(&mut reopened, &ready_paths.state, &ready_paths.config)
                .unwrap();
        assert!(
            restarted.authority_ready,
            "ready posture must reconstruct across restart"
        );

        let drift_dir = tempfile::tempdir().unwrap();
        let drift_paths = paths(drift_dir.path());
        let evidence = classify_authority_startup(&drift_paths, "ledger-a", "mother-a").unwrap();
        let mut drift_ledger =
            open_classified_authority(&drift_paths, "ledger-a", "mother-a", &evidence).unwrap();
        let state = crate::MctRuntimeStateStore::open(&drift_paths.state).unwrap();
        let contract = mct_kernel::CanonicalToyContract {
            toy_id: mct_kernel::ToyId::new("mct:test/drift").unwrap(),
            contract: mct_kernel::ToyContractIdentity {
                namespace: "mct:test".into(),
                interface_name: "drift".into(),
                version: "1.0.0".into(),
                function_name: Some("read".into()),
                resource_name: None,
            },
            authority_bearing: true,
            catalog_revision: 1,
            admitted_by_observation_id: ObservationId::new("obs:legacy-drift").unwrap(),
        };
        state.upsert_toy_contract(&contract).unwrap();
        drop(state);
        let drift =
            finalize_authority_startup(&mut drift_ledger, &drift_paths.state, &drift_paths.config)
                .unwrap();
        assert!(
            !drift.authority_ready,
            "broader legacy SQLite authority must keep startup degraded"
        );
        assert!(
            drift
                .report
                .blocking_reasons
                .contains(&"legacy_import_required".into())
        );
        assert_eq!(
            crate::MctRuntimeStateStore::open(&drift_paths.state)
                .unwrap()
                .toy_contracts()
                .unwrap(),
            vec![contract],
            "drift reporting must not repair SQLite from canonical replay"
        );
        assert!(
            !replay_authority_entries(&drift_ledger.entries().unwrap())
                .unwrap()
                .imported
        );
    }

    #[test]
    fn immutable_prewrite_snapshot_selects_all_four_startup_classes() {
        let virgin = tempfile::tempdir().unwrap();
        let virgin_paths = paths(virgin.path());
        let before = fs::read_dir(virgin.path()).unwrap().count();
        let evidence = classify_authority_startup(&virgin_paths, "ledger-a", "mother-a").unwrap();
        assert_eq!(evidence.startup_class, AuthorityStartupClassV1::Virgin);
        assert_eq!(before, fs::read_dir(virgin.path()).unwrap().count());
        assert!(!virgin_paths.ledger.exists());

        let gated = tempfile::tempdir().unwrap();
        let gated_paths = paths(gated.path());
        touch(&gated_paths.ledger);
        assert_eq!(
            classify_authority_startup(&gated_paths, "ledger-a", "mother-a")
                .unwrap()
                .startup_class,
            AuthorityStartupClassV1::OperatorGatedNonvirgin
        );

        let legacy = tempfile::tempdir().unwrap();
        let legacy_paths = paths(legacy.path());
        {
            let mut ledger =
                JsonlObservationLedger::open(&legacy_paths.ledger, "ledger-a", "mother-a").unwrap();
            ledger
                .append_before_effect(
                    mct_kernel::MctObservation::informational(
                        mct_kernel::ObservationId::new("obs:pre-h2").unwrap(),
                        mct_kernel::Timestamp::new("2026-08-04T00:00:00Z").unwrap(),
                        mct_kernel::ObservationKind::LifecycleTransitionRecorded,
                        mct_kernel::TraceId::new("trace:pre-h2").unwrap(),
                        "pre-H2 observation",
                    ),
                    "2026-08-04T00:00:00Z",
                )
                .unwrap();
        }
        assert_eq!(
            classify_authority_startup(&legacy_paths, "ledger-a", "mother-a")
                .unwrap()
                .startup_class,
            AuthorityStartupClassV1::LegacyLedgerUpgrade
        );

        let ordinary = tempfile::tempdir().unwrap();
        let ordinary_paths = paths(ordinary.path());
        drop(
            JsonlObservationLedger::open_authority(&ordinary_paths.ledger, "ledger-a", "mother-a")
                .unwrap(),
        );
        assert_eq!(
            classify_authority_startup(&ordinary_paths, "ledger-a", "mother-a")
                .unwrap()
                .startup_class,
            AuthorityStartupClassV1::OrdinaryReopen
        );
    }

    #[test]
    fn durable_artifact_matrix_and_unavailable_inspection_prevent_virgin_classification() {
        let empty = tempfile::tempdir().unwrap();
        let empty_inventory = classify_startup_artifacts(&paths(empty.path())).unwrap();
        assert!(
            empty_inventory.proves_virgin(),
            "only the empty service-root container may satisfy the virgin conjunction"
        );

        type ArtifactCase = (MctStartupArtifactClassV1, Box<dyn Fn(&MctStartupPaths)>);
        let cases: Vec<ArtifactCase> = vec![
            (
                MctStartupArtifactClassV1::CanonicalObservationLedger,
                Box::new(|p| touch(&p.ledger)),
            ),
            (
                MctStartupArtifactClassV1::LedgerRecoveryForensics,
                Box::new(|p| {
                    fs::create_dir_all(mct_observation::forensic_root_path(&p.ledger)).unwrap()
                }),
            ),
            (
                MctStartupArtifactClassV1::RuntimeSqlite,
                Box::new(|p| touch(&p.state)),
            ),
            (
                MctStartupArtifactClassV1::SqliteDurabilitySidecar,
                Box::new(|p| touch(&PathBuf::from(format!("{}-wal", p.state.display())))),
            ),
            (
                MctStartupArtifactClassV1::DaemonConfiguration,
                Box::new(|p| touch(&p.config)),
            ),
            (
                MctStartupArtifactClassV1::InterruptedConfigPublication,
                Box::new(|p| touch(&p.config.with_extension("json.tmp"))),
            ),
            (
                MctStartupArtifactClassV1::RecordedMotherIdentity,
                Box::new(|p| touch(&p.identity)),
            ),
            (
                MctStartupArtifactClassV1::ChildPackageCatalog,
                Box::new(|p| fs::create_dir_all(&p.children).unwrap()),
            ),
            (
                MctStartupArtifactClassV1::InterruptedChildPublication,
                Box::new(|p| fs::create_dir_all(p.children.join(".acquiring/attempt")).unwrap()),
            ),
            (
                MctStartupArtifactClassV1::ContentAddressedBlobs,
                Box::new(|p| fs::create_dir_all(p.root.join("blobs/tmp")).unwrap()),
            ),
            (
                MctStartupArtifactClassV1::DaemonReleaseStore,
                Box::new(|p| fs::create_dir_all(p.root.join("releases/.acquiring")).unwrap()),
            ),
            (
                MctStartupArtifactClassV1::SupervisorLifecycleRecord,
                Box::new(|p| touch(&p.supervisor_record)),
            ),
            (
                MctStartupArtifactClassV1::InterruptedSupervisorPublication,
                Box::new(|p| touch(&p.root.join(".supervisor.json.7.tmp"))),
            ),
            (
                MctStartupArtifactClassV1::SupervisorPolicy,
                Box::new(|p| touch(&p.supervisor_plist)),
            ),
            (
                MctStartupArtifactClassV1::InterruptedSupervisorPolicyPublication,
                Box::new(|p| touch(&p.root.join(".io.patina.mct.mother.plist.7.tmp"))),
            ),
            (
                MctStartupArtifactClassV1::SupervisorLogs,
                Box::new(|p| touch(&p.stdout_log)),
            ),
            (
                MctStartupArtifactClassV1::OtherManagedRootEntry,
                Box::new(|p| touch(&p.root.join("future-daemon-artifact"))),
            ),
        ];
        for (expected_class, create) in cases {
            let dir = tempfile::tempdir().unwrap();
            let paths = paths(dir.path());
            create(&paths);
            let inventory = classify_startup_artifacts(&paths).unwrap();
            assert!(
                !inventory.proves_virgin(),
                "durable class {expected_class:?} must independently prevent virginity"
            );
            assert!(inventory.entries.iter().any(|entry| {
                entry.artifact_class == expected_class
                    && entry.state == MctStartupArtifactStateV1::Present
            }));
        }

        let transient = tempfile::tempdir().unwrap();
        let transient_paths = paths(transient.path());
        touch(&transient_paths.control_socket);
        assert!(
            classify_startup_artifacts(&transient_paths)
                .unwrap()
                .proves_virgin()
        );

        let unavailable = tempfile::tempdir().unwrap();
        let mut unavailable_paths = paths(unavailable.path());
        unavailable_paths.config = unavailable.path().join("x".repeat(300));
        let inventory = classify_startup_artifacts(&unavailable_paths).unwrap();
        assert!(
            !inventory.proves_virgin(),
            "unavailable inspection must deny the virgin conjunction"
        );
        assert!(inventory.entries.iter().any(|entry| {
            entry.artifact_class == MctStartupArtifactClassV1::DaemonConfiguration
                && entry.state == MctStartupArtifactStateV1::Unavailable
        }));
    }
}

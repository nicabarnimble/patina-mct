//! Disk-first startup evidence for Mother authority establishment.

use mct_observation::{
    AuthorityStartupClassV1, AuthorityStateV1, GrantsAuthorityIdentityV1, JsonlObservationLedger,
    ObservationLedgerError, authority_state_hash, replay_authority_entries,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn paths(root: &Path) -> MctStartupPaths {
        MctStartupPaths::supervised(root, root.join("io.patina.mct.mother.plist"))
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, []).unwrap();
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

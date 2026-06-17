use crate::{
    MctChildIntegrityMode, MctChildLoadOptions, MctChildLoadReport, MctOperatorChildScope,
    MctRegistrySourceRecord, MctRuntimeStateStore, load_children_from_dir, unix_timestamp_string,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctRegistrySyncReport {
    pub source_id: String,
    pub source_path: PathBuf,
    pub loaded: usize,
    pub failed: usize,
    pub load_report: MctChildLoadReport,
}

pub fn sync_child_registry_source(
    state: &MctRuntimeStateStore,
    source_id: impl Into<String>,
    children_dir: impl Into<PathBuf>,
    integrity_mode: MctChildIntegrityMode,
    scope: MctOperatorChildScope,
) -> Result<MctRegistrySyncReport> {
    let source_id = source_id.into();
    let children_dir = children_dir.into();
    let report = load_children_from_dir(MctChildLoadOptions {
        children_dir: children_dir.clone(),
        integrity_mode,
    });
    for child in &report.children {
        state.record_loaded_child_candidate(child, scope.clone())?;
    }
    state.upsert_registry_source(MctRegistrySourceRecord {
        source_id: source_id.clone(),
        source_path: children_dir.clone(),
        last_sync_at: Some(unix_timestamp_string()),
        last_loaded: report.loaded as u64,
        last_failed: report.failed as u64,
        state: if report.failed == 0 {
            "synced"
        } else {
            "partial"
        }
        .into(),
    })?;
    Ok(MctRegistrySyncReport {
        source_id,
        source_path: children_dir,
        loaded: report.loaded,
        failed: report.failed,
        load_report: report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::fs;

    fn write_child(dir: &std::path::Path) {
        let wasm = dir.join("child-a.wasm");
        let toml = dir.join("child-a.toml");
        fs::write(&wasm, b"wasm").unwrap();
        fs::write(
            &toml,
            r#"[child]
name = "child-a"
version = "0.1.0"
[child.ingress]
mode = "wit-only"
[child.contract]
allow = ["patina/echo@0.1.0.echo"]
"#,
        )
        .unwrap();
        for path in [&wasm, &toml] {
            let digest = format!("{:x}", Sha256::digest(fs::read(path).unwrap()));
            let mut sidecar = path.as_os_str().to_os_string();
            sidecar.push(".sha256");
            fs::write(PathBuf::from(sidecar), digest).unwrap();
        }
    }

    #[test]
    fn registry_sync_records_loaded_children_as_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let state = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let children = tempfile::tempdir().unwrap();
        write_child(children.path());

        let report = sync_child_registry_source(
            &state,
            "local",
            children.path(),
            MctChildIntegrityMode::RequireSidecars,
            MctOperatorChildScope::default(),
        )
        .unwrap();

        assert_eq!(report.loaded, 1);
        assert_eq!(state.summary().unwrap().artifacts, 1);
    }
}

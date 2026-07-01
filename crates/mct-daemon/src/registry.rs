use crate::{
    MctChildIntegrityMode, MctChildLoadOptions, MctChildLoadReport, MctLoadedChild,
    MctOperatorChildScope, MctRegistrySourceRecord, MctRuntimeStateStore, load_children_from_dir,
    unix_timestamp_string,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctRegistrySyncReport {
    pub source_id: String,
    pub source_path: PathBuf,
    pub loaded: usize,
    pub failed: usize,
    pub load_report: MctChildLoadReport,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctChildPackageInstallReport {
    pub child_name: String,
    pub artifact_id: String,
    pub artifact_version: String,
    pub source_dir: PathBuf,
    pub installed_dir: PathBuf,
    pub replaced_existing: bool,
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

pub fn install_verified_child_package(
    source_dir: impl AsRef<Path>,
    children_dir: impl AsRef<Path>,
    replace_existing: bool,
) -> Result<MctChildPackageInstallReport> {
    let source_dir = canonical_dir(source_dir.as_ref(), "child package source")?;
    let children_dir = absolute_dir(children_dir.as_ref())?;
    fs::create_dir_all(&children_dir)
        .with_context(|| format!("create children directory {}", children_dir.display()))?;

    let child = load_single_verified_package_child(&source_dir)?;
    let installed_dir = children_dir.join(&child.name);
    if installed_dir.exists() && !replace_existing {
        bail!(
            "installed child '{}' already exists at {}; pass --replace to update it",
            child.name,
            installed_dir.display()
        );
    }

    let staging_dir = children_dir.join(format!(
        ".installing-{}-{}",
        sanitize_path_token(&child.name),
        unix_timestamp_string()
    ));
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir)
            .with_context(|| format!("remove stale staging dir {}", staging_dir.display()))?;
    }
    fs::create_dir(&staging_dir)
        .with_context(|| format!("create staging dir {}", staging_dir.display()))?;

    copy_installable_package_files(&source_dir, &staging_dir, &child)
        .inspect_err(|_| cleanup_dir(&staging_dir))?;

    let backup_dir = children_dir.join(format!(
        ".replaced-{}-{}",
        sanitize_path_token(&child.name),
        unix_timestamp_string()
    ));
    let replaced_existing = installed_dir.exists();
    if replaced_existing {
        fs::rename(&installed_dir, &backup_dir).with_context(|| {
            format!(
                "move existing child package {} to {}",
                installed_dir.display(),
                backup_dir.display()
            )
        })?;
    }

    if let Err(error) = fs::rename(&staging_dir, &installed_dir) {
        if replaced_existing {
            let _ = fs::rename(&backup_dir, &installed_dir);
        }
        return Err(error).with_context(|| {
            format!(
                "install child package {} to {}",
                staging_dir.display(),
                installed_dir.display()
            )
        });
    }

    if replaced_existing {
        cleanup_dir(&backup_dir);
    }

    Ok(MctChildPackageInstallReport {
        child_name: child.name,
        artifact_id: child.artifact_id,
        artifact_version: child.version,
        source_dir,
        installed_dir,
        replaced_existing,
    })
}

fn load_single_verified_package_child(source_dir: &Path) -> Result<MctLoadedChild> {
    let report = load_children_from_dir(MctChildLoadOptions::new(source_dir).strict_integrity());
    if report.loaded != 1 || report.failed != 0 {
        bail!(
            "child package source {} must contain exactly one strictly verified child; loaded={} failed={}",
            source_dir.display(),
            report.loaded,
            report.failed
        );
    }
    let child = report
        .children
        .into_iter()
        .next()
        .expect("loaded count checked");
    if !child.integrity_verified() {
        bail!(
            "child package '{}' is not verified and cannot be installed",
            child.name
        );
    }
    Ok(child)
}

fn copy_installable_package_files(
    source_dir: &Path,
    staging_dir: &Path,
    child: &MctLoadedChild,
) -> Result<()> {
    copy_package_file(source_dir, staging_dir, &child.manifest_path)?;
    copy_package_file(
        source_dir,
        staging_dir,
        &hash_sidecar_path(&child.manifest_path),
    )?;
    copy_package_file(source_dir, staging_dir, &child.wasm_path)?;
    copy_package_file(
        source_dir,
        staging_dir,
        &hash_sidecar_path(&child.wasm_path),
    )?;

    let checksums = source_dir.join("checksums.txt");
    if checksums.is_file() {
        copy_package_file(source_dir, staging_dir, &checksums)?;
    }
    Ok(())
}

fn copy_package_file(source_dir: &Path, staging_dir: &Path, source_file: &Path) -> Result<()> {
    let relative = source_file.strip_prefix(source_dir).with_context(|| {
        format!(
            "package file {} is not under source {}",
            source_file.display(),
            source_dir.display()
        )
    })?;
    if relative.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        bail!(
            "package file {} has invalid relative path",
            source_file.display()
        );
    }
    let destination = staging_dir.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create install dir {}", parent.display()))?;
    }
    fs::copy(source_file, &destination).with_context(|| {
        format!(
            "copy package file {} to {}",
            source_file.display(),
            destination.display()
        )
    })?;
    Ok(())
}

fn canonical_dir(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("resolve {label} {}", path.display()))?;
    if !canonical.is_dir() {
        bail!("{label} {} is not a directory", canonical.display());
    }
    Ok(canonical)
}

fn absolute_dir(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn hash_sidecar_path(path: &Path) -> PathBuf {
    let mut sidecar: OsString = path.as_os_str().to_os_string();
    sidecar.push(".sha256");
    PathBuf::from(sidecar)
}

fn sanitize_path_token(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn cleanup_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
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
kind = "child"
[child.ingress]
mode = "wit-only"
[child.contract]
allow = ["patina/echo@0.1.0.echo"]
"#,
        )
        .unwrap();
        for path in [&wasm, &toml] {
            write_sidecar(path);
        }
    }

    fn write_package(dir: &std::path::Path, wasm_bytes: &[u8]) {
        let wasm = dir.join("target/wasm32-wasip1/release/child-a.wasm");
        fs::create_dir_all(wasm.parent().unwrap()).unwrap();
        fs::write(&wasm, wasm_bytes).unwrap();
        let manifest = dir.join("child.toml");
        fs::write(
            &manifest,
            r#"[child]
name = "child-a"
version = "0.1.0"
kind = "child"
[child.ingress]
mode = "wit-only"
[child.artifact]
wasm = "target/wasm32-wasip1/release/child-a.wasm"
[child.contract]
allow = ["patina/echo@0.1.0.echo"]
"#,
        )
        .unwrap();
        write_sidecar(&wasm);
        write_sidecar(&manifest);
        fs::write(
            dir.join("checksums.txt"),
            format!(
                "{}  target/wasm32-wasip1/release/child-a.wasm\n{}  child.toml\n",
                sha256_file(&wasm),
                sha256_file(&manifest)
            ),
        )
        .unwrap();
    }

    fn write_sidecar(path: &Path) {
        fs::write(hash_sidecar_path(path), sha256_file(path)).unwrap();
    }

    fn sha256_file(path: &Path) -> String {
        format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
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

    #[test]
    fn installs_verified_package_atomically_under_child_name() {
        let source = tempfile::tempdir().unwrap();
        let children = tempfile::tempdir().unwrap();
        write_package(source.path(), b"wasm-v1");

        let report = install_verified_child_package(source.path(), children.path(), false).unwrap();

        assert_eq!(report.child_name, "child-a");
        assert!(!report.replaced_existing);
        assert!(children.path().join("child-a/child.toml").is_file());
        assert!(
            children
                .path()
                .join("child-a/target/wasm32-wasip1/release/child-a.wasm.sha256")
                .is_file()
        );
        let load =
            load_children_from_dir(MctChildLoadOptions::new(children.path()).strict_integrity());
        assert_eq!(load.loaded, 1);
        assert!(load.children[0].integrity_verified());
    }

    #[test]
    fn install_requires_replace_for_existing_child() {
        let source = tempfile::tempdir().unwrap();
        let children = tempfile::tempdir().unwrap();
        write_package(source.path(), b"wasm-v1");
        install_verified_child_package(source.path(), children.path(), false).unwrap();

        let result = install_verified_child_package(source.path(), children.path(), false);

        assert!(result.is_err());
    }

    #[test]
    fn install_replace_updates_package_contents() {
        let source_v1 = tempfile::tempdir().unwrap();
        let source_v2 = tempfile::tempdir().unwrap();
        let children = tempfile::tempdir().unwrap();
        write_package(source_v1.path(), b"wasm-v1");
        write_package(source_v2.path(), b"wasm-v2");
        install_verified_child_package(source_v1.path(), children.path(), false).unwrap();

        let report =
            install_verified_child_package(source_v2.path(), children.path(), true).unwrap();

        assert!(report.replaced_existing);
        assert_eq!(
            fs::read(
                children
                    .path()
                    .join("child-a/target/wasm32-wasip1/release/child-a.wasm")
            )
            .unwrap(),
            b"wasm-v2"
        );
    }
}

use crate::{
    MctChildIntegrityMode, MctChildLoadOptions, MctChildLoadReport, MctLoadedChild,
    MctOperatorChildScope, MctRegistrySourceRecord, MctRuntimeStateStore, current_timestamp_string,
    load_children_from_dir,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs,
    io::Read,
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
        last_sync_at: Some(current_timestamp_string()),
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
    mct_observation::private_fs::ensure_directory(&children_dir)
        .with_context(|| format!("verify private registry root {}", children_dir.display()))?;
    let children_dir = children_dir.canonicalize()?;
    let child = load_single_verified_package_child(&source_dir)?;
    validate_package_name(&child.name)?;
    let installed_dir = children_dir.join(&child.name);
    verify_install_directory(&installed_dir)?;
    if installed_dir.exists() && !replace_existing {
        bail!(
            "installed child '{}' already exists at {}; pass --replace to update it",
            child.name,
            installed_dir.display()
        );
    }

    let staging_dir = children_dir.join(mct_observation::private_fs::nonce_name("installing")?);
    mct_observation::private_fs::ensure_directory(&staging_dir)?;

    copy_installable_package_files(&source_dir, &staging_dir, &child)
        .inspect_err(|_| cleanup_dir(&staging_dir))?;

    let staged = load_single_verified_package_child(&staging_dir)
        .inspect_err(|_| cleanup_dir(&staging_dir))?;
    if staged.artifact_id != child.artifact_id || staged.manifest_digest != child.manifest_digest {
        cleanup_dir(&staging_dir);
        bail!("registry package changed while staging");
    }
    verify_install_directory(&installed_dir)?;
    let backup_dir = children_dir.join(mct_observation::private_fs::nonce_name("replaced")?);
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
        .context("verified package has no Child")?;
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
        mct_observation::private_fs::ensure_directory(parent)?;
    }
    let input = mct_observation::private_fs::trusted_input(source_file)?;
    let mut output = mct_observation::private_fs::create_new(&destination)?;
    let cap = crate::children::component_bytes::MAX_COMPONENT_BYTES as u64;
    let copied = std::io::copy(&mut input.take(cap + 1), &mut output)?;
    if copied > cap {
        bail!("registry package member exceeds bounded copy limit");
    }
    output.sync_all()?;
    Ok(())
}

fn canonical_dir(path: &Path, label: &str) -> Result<PathBuf> {
    mct_observation::private_fs::trusted_directory(path)?;
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

fn validate_package_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 128
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        || name.ends_with('.')
    {
        bail!("registry package name must be a bounded portable basename");
    }
    Ok(())
}

fn verify_install_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
            mct_observation::private_fs::ensure_directory(path)?;
            Ok(())
        }
        Ok(_) => bail!("registry installation target is not a private real directory"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn cleanup_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

#[cfg(test)]
mod tests {
    fn private_tempdir() -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt as _;
        tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
    }
    #[test]
    fn l19_package_names_are_basenames_not_sanitized_paths() {
        for name in [
            "",
            ".",
            "..",
            "../escape",
            "/absolute",
            "a/b",
            "a\\\\b",
            "C:drive",
            ".hidden",
            "trailing.",
            "space name",
        ] {
            assert!(super::validate_package_name(name).is_err(), "{name}");
        }
        assert!(super::validate_package_name(&"a".repeat(129)).is_err());
        for name in ["slate-manager", "child_2", "child.v1"] {
            super::validate_package_name(name).unwrap();
        }
    }

    #[test]
    fn l19_installer_refuses_traversal_and_symlink_roots_or_destinations() {
        use std::os::unix::fs::symlink;
        let source = private_tempdir();
        let registry = private_tempdir();
        let outside = private_tempdir();
        let source_root = source.path().canonicalize().unwrap();
        let registry_root = registry.path().canonicalize().unwrap();
        write_package(&source_root, b"wasm");
        let manifest_path = source_root.join("child.toml");
        let original = fs::read_to_string(&manifest_path).unwrap();
        fs::write(
            &manifest_path,
            original.replace("name = \"child-a\"", "name = \"../escape\""),
        )
        .unwrap();
        write_sidecar(&manifest_path);
        assert!(install_verified_child_package(&source_root, &registry_root, true).is_err());
        assert!(!registry_root.parent().unwrap().join("escape").exists());
        fs::write(&manifest_path, original).unwrap();
        write_sidecar(&manifest_path);
        let target = registry_root.join("child-a");
        symlink(outside.path().canonicalize().unwrap(), &target).unwrap();
        assert!(install_verified_child_package(&source_root, &registry_root, true).is_err());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
        fs::remove_file(target).unwrap();
        let alias = registry_root.join("source-alias");
        symlink(&source_root, &alias).unwrap();
        assert!(install_verified_child_package(&alias, &registry_root, false).is_err());
        fs::remove_file(alias).unwrap();
        let report = install_verified_child_package(&source_root, &registry_root, false).unwrap();
        assert!(report.installed_dir.starts_with(&registry_root));
    }

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

    #[test]
    fn l19_package_members_and_registry_ancestors_do_not_follow_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let source = private_tempdir();
        let registry = private_tempdir();
        let source_root = source.path().canonicalize().unwrap();
        let registry_root = registry.path().canonicalize().unwrap();
        write_package(&source_root, b"wasm");
        for relative in ["child.toml", "target/wasm32-wasip1/release/child-a.wasm"] {
            let member = source_root.join(relative);
            let saved = member.with_extension("saved");
            fs::rename(&member, &saved).unwrap();
            symlink(&saved, &member).unwrap();
            assert!(install_verified_child_package(&source_root, &registry_root, false).is_err());
            assert_eq!(fs::read_dir(&registry_root).unwrap().count(), 0);
            fs::remove_file(&member).unwrap();
            fs::rename(&saved, &member).unwrap();
        }
        let alias = source_root.join("registry-alias");
        symlink(&registry_root, &alias).unwrap();
        assert!(install_verified_child_package(&source_root, &alias, false).is_err());
        let writable = registry_root.join("writable");
        fs::create_dir(&writable).unwrap();
        fs::set_permissions(&writable, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(
            install_verified_child_package(&source_root, writable.join("children"), false).is_err()
        );
        assert!(!writable.join("children").exists());
        fs::set_permissions(&writable, fs::Permissions::from_mode(0o700)).unwrap();
        let installed =
            install_verified_child_package(&source_root, &registry_root, false).unwrap();
        assert!(installed.installed_dir.join("child.toml").is_file());
    }

    fn write_sidecar(path: &Path) {
        fs::write(hash_sidecar_path(path), sha256_file(path)).unwrap();
    }

    fn sha256_file(path: &Path) -> String {
        format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
    }

    #[test]
    fn registry_sync_records_loaded_children_as_candidates() {
        let dir = private_tempdir();
        let state = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let children = private_tempdir();
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
        let source = private_tempdir();
        let children = private_tempdir();
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
        let source = private_tempdir();
        let children = private_tempdir();
        write_package(source.path(), b"wasm-v1");
        install_verified_child_package(source.path(), children.path(), false).unwrap();

        let result = install_verified_child_package(source.path(), children.path(), false);

        assert!(result.is_err());
    }

    #[test]
    fn install_replace_updates_package_contents() {
        let source_v1 = private_tempdir();
        let source_v2 = private_tempdir();
        let children = private_tempdir();
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

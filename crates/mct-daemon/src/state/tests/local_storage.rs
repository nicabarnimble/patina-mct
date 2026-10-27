use super::*;
fn private_tempdir() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt as _;
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

#[test]
fn l1_config_sqlite_and_sidecars_are_private_under_permissive_umask() {
    const CHILD: &str = "MCT_STATE_UMASK_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let config_path = root.join("config/nested/config.json");
        let config = crate::MctDaemonConfigStore::new(&config_path);
        config.save(&crate::MctDaemonConfig::default()).unwrap();
        config.load().unwrap();
        let path = root.join("state/nested/state.sqlite");
        let store = MctRuntimeStateStore::open(&path).unwrap();
        store
            .conn
            .execute("INSERT INTO mct_state_meta VALUES ('l1', 'test')", [])
            .unwrap();
        for path in [
            config_path,
            path.clone(),
            path.with_file_name("state.sqlite-wal"),
            path.with_file_name("state.sqlite-shm"),
        ] {
            assert_eq!(std::fs::metadata(path).unwrap().mode() & 0o7777, 0o600);
        }
        for path in [
            root.join("config"),
            root.join("config/nested"),
            root.join("state"),
            root.join("state/nested"),
        ] {
            assert_eq!(std::fs::metadata(path).unwrap().mode() & 0o7777, 0o700);
        }
        return;
    }
    let root = private_tempdir();
    let status = std::process::Command::new("/bin/sh")
        .args(["-c", "umask 000; exec \"$1\" --exact state::tests::local_storage::l1_config_sqlite_and_sidecars_are_private_under_permissive_umask --nocapture", "mct-umask-test"])
        .arg(std::env::current_exe().unwrap()).env(CHILD, root.path().canonicalize().unwrap())
        .status().unwrap();
    assert!(status.success());
}

#[test]
fn l1_config_database_and_sidecar_links_refuse_without_touching_targets() {
    let dir = private_tempdir();
    let root = dir.path().canonicalize().unwrap();
    let target = root.join("target");
    mct_observation::private_fs::write_atomic(&target, b"preserved").unwrap();
    let config = root.join("config.json");
    symlink(&target, &config).unwrap();
    let config = crate::MctDaemonConfigStore::new(config);
    assert!(config.load().is_err());
    assert!(config.save(&crate::MctDaemonConfig::default()).is_err());
    let path = root.join("state.sqlite");
    symlink(&target, &path).unwrap();
    assert!(MctRuntimeStateStore::open(&path).is_err());
    std::fs::remove_file(&path).unwrap();
    let dangling = root.join("state.sqlite-wal");
    symlink(root.join("missing-target"), &dangling).unwrap();
    assert!(MctRuntimeStateStore::open(&path).is_err());
    assert!(
        !path.exists(),
        "unsafe sidecar rejects before creating the DB"
    );
    std::fs::remove_file(dangling).unwrap();
    drop(MctRuntimeStateStore::open(&path).unwrap());
    let before = std::fs::read(&path).unwrap();
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = path.with_file_name(format!("state.sqlite{suffix}"));
        symlink(&target, &sidecar).unwrap();
        assert!(MctRuntimeStateStore::open(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(sidecar).unwrap();
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert!(MctRuntimeStateStore::open(&path).is_err());
    assert_eq!(std::fs::read(target).unwrap(), b"preserved");
}

#[test]
fn l1_sqlite_metadata_preflight_preserves_live_posix_locks() {
    const DB: &str = "MCT_SQLITE_LOCK_CHILD";
    if let Some(path) = std::env::var_os(DB) {
        let conn = Connection::open(path).unwrap();
        conn.busy_timeout(std::time::Duration::ZERO).unwrap();
        let result = conn.execute_batch("BEGIN IMMEDIATE");
        if std::env::var_os("MCT_SQLITE_EXPECT_LOCK").is_some() {
            assert!(
                matches!(result, Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::DatabaseBusy)
            );
        } else {
            result.unwrap();
            conn.execute_batch("ROLLBACK").unwrap();
        }
        return;
    }
    let dir = private_tempdir();
    let path = dir.path().canonicalize().unwrap().join("state.sqlite");
    let state = MctRuntimeStateStore::open(&path).unwrap();
    state
        .conn
        .execute_batch(
            "BEGIN IMMEDIATE; INSERT INTO mct_state_meta VALUES ('lock-test', 'uncommitted')",
        )
        .unwrap();
    // These are exactly the metadata checks used around Connection::open.
    mct_observation::private_fs::ensure_private_file(&path).unwrap();
    mct_observation::private_fs::private_file_identity(&path).unwrap();
    verify_sqlite_sidecars(&path).unwrap();
    let probe = |locked: bool| {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "state::tests::local_storage::l1_sqlite_metadata_preflight_preserves_live_posix_locks", "--nocapture"])
            .env(DB, &path).env_remove("MCT_SQLITE_EXPECT_LOCK");
        if locked {
            command.env("MCT_SQLITE_EXPECT_LOCK", "1");
        }
        assert!(command.status().unwrap().success());
    };
    probe(true);
    state.conn.execute_batch("ROLLBACK").unwrap();
    probe(false);
}

#[test]
fn l14_corrupt_artifact_ids_and_json_are_errors_not_panics_or_repaired_rows() {
    for (column, value) in [
        ("artifact_id", ""),
        ("created_by_observation_id", " "),
        ("primary_export_json", "{bad"),
        ("runtime_shape", "unexpected-runtime"),
    ] {
        let dir = private_tempdir();
        let state =
            MctRuntimeStateStore::open(dir.path().canonicalize().unwrap().join("state.sqlite"))
                .unwrap();
        state.upsert_artifact(&artifact()).unwrap();
        let read = || {
            state.conn.query_row(
            "SELECT artifact_id, child_name, artifact_version, content_hash, manifest_hash, primary_export_json,
             runtime_shape, ingress_mode, lifecycle_exports, verification_status, provenance_status,
             acquisition_ids_json, created_by_observation_id FROM component_artifacts", [], artifact_from_row
        ).map_err(anyhow::Error::from)
        };
        assert!(read().is_ok());
        state
            .conn
            .execute(
                &format!("UPDATE component_artifacts SET {column} = ?1"),
                [value],
            )
            .unwrap();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(read));
        let error = outcome
            .expect("malformed SQLite must never panic")
            .unwrap_err();
        assert!(
            error
                .chain()
                .any(|e| e.downcast_ref::<MctCorruptStateError>().is_some()),
            "{error:#}"
        );
        let preserved: String = state
            .conn
            .query_row(
                &format!("SELECT {column} FROM component_artifacts"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(preserved, value);
    }
}

#[test]
fn l14_optional_timestamps_booleans_and_negative_counters_fail_closed() {
    let dir = private_tempdir();
    let state = MctRuntimeStateStore::open(dir.path().canonicalize().unwrap().join("state.sqlite"))
        .unwrap();
    for text in ["", "not-a-timestamp", "2026-99-99"] {
        let result = state
            .conn
            .query_row("SELECT ?1 AS completed_at", [text], |row| {
                decode_optional_cell(row, 0, Timestamp::new)
            });
        assert!(matches!(
            result,
            Err(rusqlite::Error::FromSqlConversionFailure(..))
        ));
    }
    let absent: Option<Timestamp> = state
        .conn
        .query_row("SELECT NULL", [], |row| {
            decode_optional_cell(row, 0, Timestamp::new)
        })
        .unwrap();
    assert!(absent.is_none());
    state.upsert_toy_contract(&toy_contract(true)).unwrap();
    // Model a corrupt/imported DB, not an authorized write through CHECK constraints.
    state
        .conn
        .execute_batch("PRAGMA ignore_check_constraints = ON")
        .unwrap();
    state
        .conn
        .execute("UPDATE toy_catalog_contracts SET authority_bearing = 2", [])
        .unwrap();
    assert!(state.toy_contracts().is_err());
    state
        .conn
        .execute(
            "UPDATE toy_catalog_contracts SET authority_bearing = 1, catalog_revision = -1",
            [],
        )
        .unwrap();
    assert!(state.toy_contracts().is_err());
    state
        .conn
        .execute("UPDATE toy_catalog_contracts SET catalog_revision = 1", [])
        .unwrap();
    assert_eq!(state.toy_contracts().unwrap().len(), 1);
    let trigger = CallTriggerAuthorityId::new("trigger-a").unwrap();
    assert_eq!(
        state.next_call_trigger_pending_sequence(&trigger).unwrap(),
        1
    );
    // Deliberately corrupt fixture import; no FK/check bypass exists in production.
    state
        .conn
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
         INSERT INTO call_trigger_pending_occurrences VALUES
         ('pending-a', 'occurrence-a', 'trigger-a', 1, 1, 1, 'overlap_queued', '{}',
          'obs-a', 'pending', '2026-01-01T00:00:00Z', NULL);",
        )
        .unwrap();
    for value in ["-1", "9223372036854775807", "not-a-number"] {
        state
            .conn
            .execute(
                "UPDATE call_trigger_pending_occurrences SET admission_sequence = ?1",
                [value],
            )
            .unwrap();
        assert!(
            state.next_call_trigger_pending_sequence(&trigger).is_err(),
            "{value}"
        );
    }
    state
        .conn
        .execute(
            "UPDATE call_trigger_pending_occurrences SET admission_sequence = 1",
            [],
        )
        .unwrap();
    assert_eq!(
        state.next_call_trigger_pending_sequence(&trigger).unwrap(),
        2
    );
}

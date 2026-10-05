use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use cairn::store::{database_path, BusyTimeout, Error, Store};
use rusqlite::{Connection, TransactionBehavior};
use tempfile::tempdir;

fn isolated_path(root: &Path) -> PathBuf {
    database_path(Some(&root.join("state")), Some(&root.join("home"))).unwrap()
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    name.into()
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

fn pragma(connection: &Connection, name: &str) -> i64 {
    connection
        .pragma_query_value(None, name, |row| row.get(0))
        .unwrap()
}

#[test]
fn paths_use_isolated_environment_values_without_creating_anything() {
    let root = tempdir().unwrap();
    let home = root.path().join("home");
    let xdg = root.path().join("state/absent/.././target");
    assert_eq!(
        database_path(Some(&xdg), Some(&home)).unwrap(),
        root.path().join("state/target/cairn/cairn.db")
    );
    assert_eq!(
        database_path(None, Some(&home)).unwrap(),
        home.join(".local/state/cairn/cairn.db")
    );
    assert_eq!(
        database_path(Some(std::path::Path::new("")), Some(&home)).unwrap(),
        home.join(".local/state/cairn/cairn.db")
    );
    assert_eq!(
        database_path(Some(&xdg), None).unwrap(),
        root.path().join("state/target/cairn/cairn.db")
    );
    assert!(database_path(None, None).is_err());
    assert!(database_path(None, Some(std::path::Path::new(""))).is_err());
    assert!(database_path(Some(std::path::Path::new("relative")), Some(&home)).is_err());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

fn schema(connection: &Connection) -> Vec<(String, String)> {
    connection
        .prepare("SELECT name, sql FROM sqlite_schema WHERE sql IS NOT NULL ORDER BY name")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

#[test]
fn migration_is_idempotent_and_preserves_data() {
    let root = tempdir().unwrap();
    let path = isolated_path(root.path());
    let mut store = Store::open(&path, BusyTimeout::UserCommand).unwrap();
    let version: String = store
        .connection()
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, "1");
    let tables: Vec<String> = store
        .connection()
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        tables,
        [
            "confirmations",
            "events",
            "injections",
            "meta",
            "projects",
            "records",
            "sources",
            "spool_ops",
            "supersessions",
            "turn_decisions"
        ]
    );

    let tx = store.transaction(TransactionBehavior::Immediate).unwrap();
    tx.execute(
        "INSERT INTO projects (key, adopted) VALUES ('synthetic-project', 1)",
        [],
    )
    .unwrap();
    tx.commit().unwrap();
    let before = schema(store.connection());
    store.migrate().unwrap();
    drop(store);
    let store = Store::open(&path, BusyTimeout::Hook).unwrap();
    assert_eq!(schema(store.connection()), before);
    let projects: Vec<(String, i64)> = store
        .connection()
        .prepare("SELECT key, adopted FROM projects")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(projects, [("synthetic-project".into(), 1)]);
}

#[test]
fn writable_open_protects_database_and_sidecars_and_sets_pragmas() {
    let root = tempdir().unwrap();
    let path = isolated_path(root.path());
    let store = Store::open(&path, BusyTimeout::Hook).unwrap();
    assert_eq!(mode(path.parent().unwrap()), 0o700);
    assert_eq!(mode(&path), 0o600);
    for suffix in ["-wal", "-shm"] {
        assert!(sidecar(&path, suffix).exists());
        assert_eq!(mode(&sidecar(&path, suffix)) & !0o600, 0);
    }
    let connection = store.connection();
    let journal: String = connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(journal, "wal");
    assert_eq!(pragma(connection, "foreign_keys"), 1);
    assert_eq!(pragma(connection, "secure_delete"), 1);
    assert_eq!(pragma(connection, "busy_timeout"), 200);
    let command = Store::open(&path, BusyTimeout::UserCommand).unwrap();
    assert_eq!(pragma(command.connection(), "busy_timeout"), 2000);
}

#[test]
fn newer_schema_is_rejected_without_changing_the_database() {
    let root = tempdir().unwrap();
    let path = isolated_path(root.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        INSERT INTO meta VALUES ('schema_version', '2');
        CREATE TABLE future_data (value TEXT);
        INSERT INTO future_data VALUES ('synthetic future data');",
        )
        .unwrap();
    drop(connection);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();
    let before = fs::read(&path).unwrap();
    assert!(matches!(
        Store::open(&path, BusyTimeout::Hook),
        Err(Error::NewerSchema { found: 2 })
    ));
    assert!(
        fs::read(&path).unwrap() == before,
        "future database bytes changed"
    );
    assert_eq!(mode(&path), 0o644);
    assert_eq!(mode(path.parent().unwrap()), 0o755);
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn read_only_open_of_missing_database_creates_nothing() {
    let root = tempdir().unwrap();
    let path = isolated_path(root.path());
    assert!(Store::open_read_only(&path, BusyTimeout::Hook)
        .unwrap()
        .is_none());
    assert!(!path.exists());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

fn seed_record(connection: &Connection, id: &str, body: &str) {
    connection.execute_batch("INSERT OR IGNORE INTO projects (id, key, adopted) VALUES (1, 'synthetic-project', 1);
        INSERT OR IGNORE INTO sources (id, agent, association, first_seen, last_seen)
        VALUES ('codex:synthetic', 'codex', 'hook', '2026-10-05T00:00:00Z', '2026-10-05T00:00:00Z');").unwrap();
    connection.execute("INSERT INTO records (id, project_id, line_path, branch, source_id, kind, body, facts, created_at)
        VALUES (?1, 1, '/synthetic/line', 'test', 'codex:synthetic', 'checkpoint', ?2, '{\"head\":\"synthetic\"}', '2026-10-05T00:00:00Z')",
        [id, body]).unwrap();
}

fn contains_marker(path: &Path, marker: &str) -> bool {
    match fs::read(path) {
        Ok(bytes) => bytes
            .windows(marker.len())
            .any(|window| window == marker.as_bytes()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => panic!("{}: {error}", path.display()),
    }
}

#[test]
fn deleting_body_erases_marker_from_database_and_wal_and_keeps_tombstone() {
    let root = tempdir().unwrap();
    let path = isolated_path(root.path());
    let mut store = Store::open(&path, BusyTimeout::UserCommand).unwrap();
    let marker = "cairn-store-synthetic-secret-29c3ed95";
    let body = format!("{marker}{}{marker}", "x".repeat(5500));
    seed_record(store.connection(), "synthetic-delete", &body);
    seed_record(
        store.connection(),
        "synthetic-retain",
        "keep this synthetic body",
    );
    store
        .connection()
        .execute_batch("PRAGMA wal_checkpoint(FULL)")
        .unwrap();
    assert!(contains_marker(&path, marker));
    assert!(contains_marker(&sidecar(&path, "-wal"), marker));

    assert!(store
        .delete_body("synthetic-delete", "2026-10-05T01:00:00Z")
        .unwrap());
    assert!(!contains_marker(&path, marker));
    assert!(!contains_marker(&sidecar(&path, "-wal"), marker));
    let tombstone: (Option<String>, String, String, String) = store
        .connection()
        .query_row(
            "SELECT body, deleted_at, source_id, facts FROM records WHERE id = 'synthetic-delete'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        tombstone,
        (
            None,
            "2026-10-05T01:00:00Z".into(),
            "codex:synthetic".into(),
            "{\"head\":\"synthetic\"}".into()
        )
    );
    let retained: String = store
        .connection()
        .query_row(
            "SELECT body FROM records WHERE id = 'synthetic-retain'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained, "keep this synthetic body");
    assert!(store
        .delete_body("synthetic-delete", "2026-10-05T02:00:00Z")
        .unwrap());
    let deleted_at: String = store
        .connection()
        .query_row(
            "SELECT deleted_at FROM records WHERE id = 'synthetic-delete'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(deleted_at, "2026-10-05T01:00:00Z");
    assert!(!store
        .delete_body("missing", "2026-10-05T02:00:00Z")
        .unwrap());
}

#[test]
fn read_only_connection_reads_committed_wal_and_rejects_writes() {
    let root = tempdir().unwrap();
    let path = isolated_path(root.path());
    let writer = Store::open(&path, BusyTimeout::UserCommand).unwrap();
    seed_record(writer.connection(), "synthetic-ro", "read from WAL");
    let reader = Store::open_read_only(&path, BusyTimeout::Hook)
        .unwrap()
        .unwrap();
    let body: String = reader
        .connection()
        .query_row(
            "SELECT body FROM records WHERE id = 'synthetic-ro'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(body, "read from WAL");
    let error = reader
        .connection()
        .execute("DELETE FROM records", [])
        .unwrap_err();
    assert_eq!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::ReadOnly)
    );
    assert_eq!(pragma(reader.connection(), "foreign_keys"), 1);
    assert_eq!(pragma(reader.connection(), "secure_delete"), 1);
    assert_eq!(pragma(reader.connection(), "busy_timeout"), 200);
    drop(reader);
    drop(writer);
    let before = fs::read(&path).unwrap();
    let reader = Store::open_read_only(&path, BusyTimeout::UserCommand)
        .unwrap()
        .unwrap();
    assert_eq!(pragma(reader.connection(), "busy_timeout"), 2000);
    assert!(fs::read(&path).unwrap() == before);
    for suffix in ["-wal", "-shm"] {
        if sidecar(&path, suffix).exists() {
            assert_eq!(mode(&sidecar(&path, suffix)) & !0o600, 0);
        }
    }
}

#[test]
fn writable_open_tightens_existing_permissions() {
    let root = tempdir().unwrap();
    let path = isolated_path(root.path());
    let store = Store::open(&path, BusyTimeout::Hook).unwrap();
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();
    for suffix in ["", "-wal", "-shm"] {
        fs::set_permissions(sidecar(&path, suffix), fs::Permissions::from_mode(0o666)).unwrap();
    }
    let _reopened = Store::open(&path, BusyTimeout::Hook).unwrap();
    assert_eq!(mode(path.parent().unwrap()), 0o700);
    for suffix in ["", "-wal", "-shm"] {
        assert_eq!(mode(&sidecar(&path, suffix)), 0o600);
    }
    assert_eq!(pragma(store.connection(), "foreign_keys"), 1);
}

#[test]
fn read_only_open_refuses_broad_permissions_without_chmod() {
    let root = tempdir().unwrap();
    let path = isolated_path(root.path());
    let _writer = Store::open(&path, BusyTimeout::Hook).unwrap();
    for (target, broad, private) in [
        (path.parent().unwrap().to_path_buf(), 0o755, 0o700),
        (path.clone(), 0o644, 0o600),
        (sidecar(&path, "-wal"), 0o644, 0o600),
        (sidecar(&path, "-shm"), 0o644, 0o600),
    ] {
        fs::set_permissions(&target, fs::Permissions::from_mode(broad)).unwrap();
        assert!(
            Store::open_read_only(&path, BusyTimeout::Hook).is_err(),
            "accepted {}",
            target.display()
        );
        assert_eq!(mode(&target), broad);
        fs::set_permissions(&target, fs::Permissions::from_mode(private)).unwrap();
    }
}

#[test]
fn migration_failure_rolls_back_tables_and_version() {
    let root = tempdir().unwrap();
    let path = isolated_path(root.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = Connection::open(&path).unwrap();
    // The v1 migration creates meta and projects before colliding with sources.
    original
        .execute_batch(
            "CREATE TABLE sources (sentinel TEXT);
        INSERT INTO sources VALUES ('synthetic preexisting data');",
        )
        .unwrap();
    let before = schema(&original);
    drop(original);
    let error = Store::open(&path, BusyTimeout::Hook).unwrap_err();
    assert!(error.to_string().contains("sources already exists"));
    let check = Connection::open(&path).unwrap();
    assert_eq!(schema(&check), before);
    let sentinel: String = check
        .query_row("SELECT sentinel FROM sources", [], |row| row.get(0))
        .unwrap();
    assert_eq!(sentinel, "synthetic preexisting data");
}

#[test]
fn newer_version_in_wal_is_rejected_by_both_open_modes_without_rewriting_data() {
    let root = tempdir().unwrap();
    let path = isolated_path(root.path());
    let writer = Store::open(&path, BusyTimeout::Hook).unwrap();
    writer
        .connection()
        .execute_batch(
            "PRAGMA wal_checkpoint(TRUNCATE);
        UPDATE meta SET value = '2' WHERE key = 'schema_version';
        CREATE TABLE future_data (value TEXT);
        INSERT INTO future_data VALUES ('synthetic WAL-only data');",
        )
        .unwrap();
    let db_before = fs::read(&path).unwrap();
    let wal_before = fs::read(sidecar(&path, "-wal")).unwrap();
    assert!(matches!(
        Store::open(&path, BusyTimeout::Hook),
        Err(Error::NewerSchema { found: 2 })
    ));
    assert!(matches!(
        Store::open_read_only(&path, BusyTimeout::Hook),
        Err(Error::NewerSchema { found: 2 })
    ));
    assert!(fs::read(&path).unwrap() == db_before, "future DB changed");
    assert!(
        fs::read(sidecar(&path, "-wal")).unwrap() == wal_before,
        "future WAL changed"
    );
}

#[test]
fn delete_reports_busy_checkpoint_and_can_finish_after_reader_releases_snapshot() {
    let root = tempdir().unwrap();
    let path = isolated_path(root.path());
    let mut writer = Store::open(&path, BusyTimeout::Hook).unwrap();
    let marker = "cairn-synthetic-busy-delete-70e521bd";
    seed_record(writer.connection(), "synthetic-busy", marker);
    let reader = Store::open_read_only(&path, BusyTimeout::Hook)
        .unwrap()
        .unwrap();
    reader.connection().execute_batch("BEGIN").unwrap();
    let body: String = reader
        .connection()
        .query_row(
            "SELECT body FROM records WHERE id = 'synthetic-busy'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(body, marker);
    assert!(matches!(
        writer.delete_body("synthetic-busy", "2026-10-05T01:00:00Z"),
        Err(Error::CheckpointBusy)
    ));
    assert!(contains_marker(&sidecar(&path, "-wal"), marker));
    let body: Option<String> = writer
        .connection()
        .query_row(
            "SELECT body FROM records WHERE id = 'synthetic-busy'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(body, None);
    reader.connection().execute_batch("ROLLBACK").unwrap();
    assert!(writer
        .delete_body("synthetic-busy", "2026-10-05T02:00:00Z")
        .unwrap());
    assert!(!contains_marker(&path, marker));
    assert!(!contains_marker(&sidecar(&path, "-wal"), marker));
    let deleted_at: String = writer
        .connection()
        .query_row(
            "SELECT deleted_at FROM records WHERE id = 'synthetic-busy'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(deleted_at, "2026-10-05T01:00:00Z");
}

#[test]
fn schema_enforces_foreign_keys_enums_and_operation_deduplication() {
    let root = tempdir().unwrap();
    let store = Store::open(isolated_path(root.path()), BusyTimeout::Hook).unwrap();
    let connection = store.connection();
    seed_record(connection, "synthetic-record", "synthetic body");
    for sql in [
        "INSERT INTO records (id, project_id, line_path, source_id, kind, created_at) VALUES ('bad-project', 2, '/synthetic', 'codex:synthetic', 'checkpoint', 'now')",
        "INSERT INTO records (id, project_id, line_path, source_id, kind, created_at) VALUES ('bad-source', 1, '/synthetic', 'missing', 'checkpoint', 'now')",
        "INSERT INTO records (id, project_id, line_path, source_id, kind, created_at) VALUES (NULL, 1, '/synthetic', 'codex:synthetic', 'checkpoint', 'now')",
        "INSERT INTO projects (key, adopted) VALUES ('invalid', 2)",
        "UPDATE sources SET association = 'unknown'",
        "UPDATE records SET kind = 'unknown'",
        "INSERT INTO confirmations (source_id, kind, at) VALUES ('codex:synthetic', 'unknown', 'now')",
        "INSERT INTO spool_ops (op_id, outcome, source_id, processed_at) VALUES ('op', 'unknown', 'codex:synthetic', 'now')",
        "INSERT INTO turn_decisions (source_id, turn_key, outcome, at) VALUES ('codex:synthetic', 'turn', 'unknown', 'now')",
    ] {
        let error = connection.execute(sql, []).unwrap_err();
        assert_eq!(error.sqlite_error_code(), Some(rusqlite::ErrorCode::ConstraintViolation), "{sql}");
    }
    connection.execute("INSERT INTO confirmations (source_id, kind, op_id, record_id, at) VALUES ('codex:synthetic', 'saved', 'op', 'synthetic-record', 'now')", []).unwrap();
    assert_eq!(connection.execute("INSERT OR IGNORE INTO confirmations (source_id, kind, op_id, at) VALUES ('codex:synthetic', 'nothing_new', 'op', 'now')", []).unwrap(), 0);
    connection.execute("INSERT INTO spool_ops (op_id, outcome, source_id, processed_at) VALUES ('op', 'ingested', 'codex:synthetic', 'now')", []).unwrap();
    assert_eq!(connection.execute("INSERT OR IGNORE INTO spool_ops (op_id, outcome, source_id, processed_at) VALUES ('op', 'rejected', 'codex:synthetic', 'now')", []).unwrap(), 0);
    for outcome in [
        "confirmed",
        "continue_requested",
        "unconfirmed_after_continue",
        "pending_unprocessed",
        "skipped",
    ] {
        let sql = "INSERT OR IGNORE INTO turn_decisions (source_id, turn_key, outcome, at) VALUES ('codex:synthetic', 'turn', ?1, 'now')";
        assert_eq!(connection.execute(sql, [outcome]).unwrap(), 1);
        assert_eq!(connection.execute(sql, [outcome]).unwrap(), 0);
    }
}

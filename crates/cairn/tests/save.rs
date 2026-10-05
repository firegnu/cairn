use cairn::cli::{self, Cli};
use cairn::ingest::{ingest, pending, Pending};
use cairn::save::Operation;
use cairn::spool::Spool;
use cairn::store::{database_path, BusyTimeout, Store};
use clap::Parser;
use rusqlite::params;
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

const AT: &str = "2026-10-05T12:00:00.000Z";

static ENVIRONMENT: Mutex<()> = Mutex::new(());

struct Fixture {
    root: tempfile::TempDir,
    saved_env: Vec<(OsString, OsString)>,
    _guard: MutexGuard<'static, ()>,
}

fn isolated_key(key: &std::ffi::OsStr) -> bool {
    key.to_string_lossy().starts_with("GIT_")
        || ["HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME", "TMPDIR"]
            .contains(&key.to_str().unwrap_or(""))
}

impl Fixture {
    fn new() -> Self {
        let guard = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
        let root = tempfile::tempdir().unwrap();
        let saved_env: Vec<_> = std::env::vars_os()
            .filter(|(key, _)| isolated_key(key))
            .collect();
        for (key, _) in &saved_env {
            std::env::remove_var(key);
        }
        for key in ["HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME", "TMPDIR"] {
            std::env::set_var(key, root.path());
        }
        std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::set_var("GIT_CONFIG_COUNT", "0");
        fs::create_dir(root.path().join("project")).unwrap();
        Self {
            root,
            saved_env,
            _guard: guard,
        }
    }

    fn cwd(&self) -> PathBuf {
        self.root.path().join("project")
    }
    fn db(&self) -> PathBuf {
        database_path(Some(self.root.path()), None).unwrap()
    }
    fn store(&self) -> Store {
        Store::open(self.db(), BusyTimeout::UserCommand).unwrap()
    }
    fn spool(&self) -> Spool {
        Spool::open(self.root.path(), &self.db()).unwrap()
    }
    fn run(&self, args: &[&str], body: &str) -> cli::Result<String> {
        cli::run_at(
            Cli::try_parse_from(std::iter::once("cairn").chain(args.iter().copied())).unwrap(),
            &mut body.as_bytes(),
            &self.cwd(),
            &self.db(),
            self.root.path(),
        )
    }
    fn files(&self) -> Vec<PathBuf> {
        let base = self.root.path().join("cairn-spool");
        let mut files = Vec::new();
        if base.exists() {
            for ns in fs::read_dir(base).unwrap() {
                for entry in fs::read_dir(ns.unwrap().path()).unwrap() {
                    files.push(entry.unwrap().path());
                }
            }
        }
        files.sort();
        files
    }
    fn queued(&self) -> (PathBuf, Operation) {
        let files = self.files();
        assert_eq!(files.len(), 1);
        let operation = serde_json::from_slice(&fs::read(&files[0]).unwrap()).unwrap();
        (files[0].clone(), operation)
    }
    fn git(&self, cwd: &Path, args: &[&str]) -> String {
        let mut output = tempfile::tempfile().unwrap();
        let mut child = Command::new("git")
            .args([
                "--no-optional-locks",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(output.try_clone().unwrap())
            .stderr(output.try_clone().unwrap())
            .spawn()
            .unwrap();
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if start.elapsed() > Duration::from_secs(5) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("fixture git timeout: {args:?}");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        output.seek(SeekFrom::Start(0)).unwrap();
        let mut text = String::new();
        output.read_to_string(&mut text).unwrap();
        assert!(status.success(), "git {args:?}: {text}");
        text.trim_end().into()
    }
}

#[test]
fn symlink_layers_and_non_regular_entries_never_touch_their_targets() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let f = Fixture::new();
    let target = tempfile::tempdir().unwrap();
    let marker = target.path().join("marker");
    fs::write(&marker, "unchanged").unwrap();
    let base = f.root.path().join("cairn-spool");
    symlink(target.path(), &base).unwrap();
    assert!(Spool::open(f.root.path(), &f.db()).is_err());
    fs::remove_file(&base).unwrap();
    let spool = f.spool();
    let ns = spool.path().to_path_buf();
    fs::remove_dir(&ns).unwrap();
    symlink(target.path(), &ns).unwrap();
    assert!(Spool::open(f.root.path(), &f.db()).is_err());
    fs::remove_file(&ns).unwrap();
    f.run(&["adopt"], "").unwrap();
    let spool = f.spool();
    let entry = spool.path().join(format!("{}.json", ulid::Ulid::new()));
    symlink(&marker, &entry).unwrap();
    let directory = spool.path().join(format!("{}.json", ulid::Ulid::new()));
    fs::create_dir(&directory).unwrap();
    let fifo = spool.path().join(format!("{}.json", ulid::Ulid::new()));
    let fifo_c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
    let socket = spool.path().join(format!("{}.json", ulid::Ulid::new()));
    let bound = f.root.path().join("socket");
    let _listener = std::os::unix::net::UnixListener::bind(&bound).unwrap();
    fs::rename(bound, &socket).unwrap();
    let mut store = f.store();
    assert_eq!(ingest(&mut store, &spool, None).unwrap().processed, 0);
    assert!(fs::symlink_metadata(&entry)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(directory.is_dir());
    assert!(fifo.exists());
    assert!(socket.exists());
    assert_eq!(fs::read_to_string(&marker).unwrap(), "unchanged");
    assert_eq!(fs::read_dir(target.path()).unwrap().count(), 1);
    assert_eq!(count(&store, "spool_ops"), 0);
    assert_eq!(spool.status().unwrap().pending_json, 0);
    fs::set_permissions(&ns, fs::Permissions::from_mode(0o750)).unwrap();
    assert!(Spool::open(f.root.path(), &f.db()).is_err());
    fs::set_permissions(&ns, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(f.root.path(), fs::Permissions::from_mode(0o770)).unwrap();
    assert!(Spool::open(f.root.path(), &f.db()).is_err());
    fs::set_permissions(f.root.path(), fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn invalid_id_version_or_timestamp_reject_without_body_or_confirmation() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let mut store = f.store();
    for case in ["id", "version", "time", "malformed"] {
        f.run(&["save"], "## 停点\nPRIVATE_SYNTHETIC_SENTINEL")
            .unwrap();
        let (path, mut operation) = f.queued();
        fs::remove_file(&path).unwrap();
        match case {
            "version" => operation.header.version = 99,
            "time" => operation.payload.created_at = "2026-10-05T12:00:00+00:00".into(),
            _ => {}
        }
        f.spool().publish(&operation).unwrap();
        if case == "id" {
            fs::rename(
                &path,
                path.with_file_name(format!("{}.json", ulid::Ulid::new())),
            )
            .unwrap();
        }
        if case == "malformed" {
            let bytes = fs::read(&path).unwrap();
            let end = bytes.iter().position(|&b| b == b'\n').unwrap() + 1;
            fs::write(
                &path,
                [&bytes[..end], b"PRIVATE_SYNTHETIC_SENTINEL"].concat(),
            )
            .unwrap();
        }
        assert_eq!(ingest(&mut store, &f.spool(), None).unwrap().rejected, 1);
        assert_eq!(count(&store, "records"), 0);
        assert_eq!(count(&store, "confirmations"), 0);
        let events: String = store
            .connection()
            .query_row("SELECT group_concat(detail) FROM events", [], |r| r.get(0))
            .unwrap();
        assert!(!events.contains("PRIVATE_SYNTHETIC_SENTINEL"));
        assert!(f.files().is_empty());
    }
}

#[test]
fn collection_prioritizes_source_and_stops_at_fifty_operations() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let mut store = f.store();
    source(&store, "codex:priority");
    f.run(&["save", "--nothing-new"], "").unwrap();
    let (path, mut operation) = f.queued();
    fs::remove_file(path).unwrap();
    let spool = f.spool();
    for n in 1..=51 {
        operation.header.op_id = ulid::Ulid::from(n as u128).to_string();
        operation.header.source = (n == 51).then(|| "codex:priority".into());
        spool.publish(&operation).unwrap();
    }
    let report = ingest(&mut store, &spool, Some("codex:priority")).unwrap();
    assert_eq!(report.processed, 50);
    assert_eq!(spool.status().unwrap().pending_json, 1);
    let first: String = store
        .connection()
        .query_row(
            "SELECT source_id FROM confirmations ORDER BY id LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(first, "codex:priority");
    let remaining = spool
        .path()
        .join(format!("{}.json", ulid::Ulid::from(50u128)));
    assert!(remaining.exists());
    assert_eq!(ingest(&mut store, &spool, None).unwrap().processed, 1);
}

#[test]
fn priority_backlog_makes_progress_on_every_collection() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let mut store = f.store();
    source(&store, "codex:backlog");
    f.run(&["save", "--source", "codex:backlog", "--nothing-new"], "")
        .unwrap();
    let (path, mut operation) = f.queued();
    fs::remove_file(path).unwrap();
    operation.payload.created_at = AT.into();
    let spool = f.spool();
    for n in 1..=4000 {
        operation.header.op_id = ulid::Ulid::from(n as u128).to_string();
        spool.publish(&operation).unwrap();
    }
    let mut total = 0;
    for round in 1..=3 {
        // Reopen on every call, as independent hooks do. No in-memory cursor
        // from the previous invocation may be necessary to make progress.
        let report = ingest(&mut store, &f.spool(), Some("codex:backlog")).unwrap();
        assert!(
            (1..=50).contains(&report.ingested),
            "round {round}: {report:?}"
        );
        assert_eq!(report.processed, report.ingested);
        total += report.ingested;
        assert_eq!(spool.status().unwrap().pending_json, 4000 - total);
        assert_eq!(count(&store, "confirmations"), total as i64);
        let latest: String = store
            .connection()
            .query_row(
                "SELECT op_id FROM confirmations ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(latest, ulid::Ulid::from(total as u128).to_string());
    }
}

#[test]
fn cross_source_backlog_always_progresses_and_eventually_collects_late_priority() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let mut store = f.store();
    source(&store, "codex:source-a");
    source(&store, "codex:source-b");
    f.run(&["save", "--source", "codex:source-a", "--nothing-new"], "")
        .unwrap();
    let (path, mut operation) = f.queued();
    fs::remove_file(path).unwrap();
    operation.payload.created_at = AT.into();
    let spool = f.spool();
    for n in 1..=4000 {
        operation.header.op_id = ulid::Ulid::from(n as u128).to_string();
        spool.publish(&operation).unwrap();
    }

    let mut total = 0;
    // A new source with no files must still drain the old source's backlog.
    for round in 1..=3 {
        let report = ingest(&mut store, &f.spool(), Some("codex:source-b")).unwrap();
        assert!(
            (1..=50).contains(&report.ingested),
            "absent B, round {round}: {report:?}"
        );
        assert_eq!(report.processed, report.ingested);
        total += report.ingested;
        assert_eq!(count(&store, "confirmations"), total as i64);
        assert_eq!(spool.status().unwrap().pending_json, 4000 - total);
    }

    // B then publishes after every remaining A file. Every call must still
    // progress, so at most one call per remaining operation can be necessary.
    operation.header.op_id = ulid::Ulid::from(4001u128).to_string();
    operation.header.source = Some("codex:source-b".into());
    spool.publish(&operation).unwrap();
    let remaining = 4001 - total;
    let mut found_b = false;
    for round in 1..=remaining {
        let report = ingest(&mut store, &f.spool(), Some("codex:source-b")).unwrap();
        assert!(
            (1..=50).contains(&report.ingested),
            "late B, round {round}: {report:?}"
        );
        assert_eq!(report.processed, report.ingested);
        total += report.ingested;
        assert_eq!(count(&store, "confirmations"), total as i64);
        assert_eq!(spool.status().unwrap().pending_json, 4001 - total);
        found_b = store
            .connection()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM confirmations WHERE source_id='codex:source-b')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        if found_b {
            break;
        }
    }
    assert!(
        found_b,
        "the late priority operation must eventually be collected"
    );
    assert!(!spool
        .path()
        .join(format!("{}.json", operation.header.op_id))
        .exists());
    let mut statement = store
        .connection()
        .prepare("SELECT op_id FROM confirmations WHERE source_id='codex:source-a' ORDER BY id")
        .unwrap();
    for (index, id) in statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .enumerate()
    {
        assert_eq!(
            id.unwrap(),
            ulid::Ulid::from((index + 1) as u128).to_string()
        );
    }
}

#[test]
fn pending_query_filters_target_source_window_and_processed_operations() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let store = f.store();
    source(&store, "codex:pending");
    let spool = f.spool();
    let check = || {
        pending(
            &store,
            &spool,
            "codex:pending",
            AT,
            Duration::from_millis(300),
        )
    };
    assert_eq!(check(), Pending::No);
    f.run(&["save", "--source", "codex:pending", "--nothing-new"], "")
        .unwrap();
    let (path, mut operation) = f.queued();
    fs::remove_file(path).unwrap();
    operation.payload.created_at = AT.into();

    let mut other_source = operation.clone();
    other_source.header.op_id = ulid::Ulid::from(1u128).to_string();
    other_source.header.source = Some("codex:other".into());
    spool.publish(&other_source).unwrap();
    let mut old = operation.clone();
    old.header.op_id = ulid::Ulid::from(2u128).to_string();
    old.payload.created_at = "2026-10-05T11:59:59.999Z".into();
    spool.publish(&old).unwrap();
    assert_eq!(check(), Pending::No);

    let other_db = database_path(Some(&f.root.path().join("other-state")), None).unwrap();
    let other_spool = Spool::open(f.root.path(), &other_db).unwrap();
    let mut foreign = operation.clone();
    foreign.header.op_id = ulid::Ulid::from(3u128).to_string();
    foreign.header.database_path = other_db;
    other_spool.publish(&foreign).unwrap();
    let foreign_name = format!("{}.json", foreign.header.op_id);
    let bytes = fs::read(other_spool.path().join(&foreign_name)).unwrap();
    let header_end = bytes.iter().position(|&byte| byte == b'\n').unwrap() + 1;
    let broken_foreign = [&bytes[..header_end], b"UNREADABLE_FOREIGN_BODY\xff"].concat();
    let foreign_path = spool.path().join(&foreign_name);
    fs::write(&foreign_path, &broken_foreign).unwrap();
    assert_eq!(check(), Pending::No);
    assert_eq!(fs::read(&foreign_path).unwrap(), broken_foreign);

    operation.header.op_id = ulid::Ulid::from(4u128).to_string();
    spool.publish(&operation).unwrap();
    let matching_path = spool
        .path()
        .join(format!("{}.json", operation.header.op_id));
    let matching_bytes = fs::read(&matching_path).unwrap();
    let snapshot = database_snapshot(&store);
    assert_eq!(check(), Pending::Yes); // inclusive window boundary
    assert_eq!(database_snapshot(&store), snapshot);
    assert_eq!(fs::read(&matching_path).unwrap(), matching_bytes);
    store
        .connection()
        .execute(
            "INSERT INTO spool_ops(op_id,outcome,source_id,processed_at)
        VALUES (?1,'ingested','codex:pending',?2)",
            params![operation.header.op_id, AT],
        )
        .unwrap();
    assert_eq!(check(), Pending::No); // committed, but unlink has not happened
    assert!(matching_path.exists());
    let temp = spool
        .path()
        .join(format!(".{}.tmp", ulid::Ulid::from(5u128)));
    fs::write(&temp, &matching_bytes).unwrap();
    let link = spool
        .path()
        .join(format!("{}.json", ulid::Ulid::from(6u128)));
    symlink(&matching_path, &link).unwrap();
    assert_eq!(check(), Pending::No);
    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());

    operation.header.op_id = ulid::Ulid::from(7u128).to_string();
    operation.payload.created_at = "2026-10-05T12:00:00.001Z".into();
    spool.publish(&operation).unwrap();
    assert_eq!(check(), Pending::Yes);
    // The query keeps using the verified namespace descriptor after its path
    // is replaced; it must not follow the replacement to another directory.
    let retained = f.root.path().join("retained-namespace");
    fs::rename(spool.path(), &retained).unwrap();
    symlink(other_spool.path(), spool.path()).unwrap();
    assert_eq!(check(), Pending::Yes);
    assert_eq!(
        fs::read(retained.join(foreign_name)).unwrap(),
        broken_foreign
    );
}

#[test]
fn pending_query_keeps_errors_and_exhausted_budget_unknown() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let store = f.store();
    let spool = f.spool();
    assert_eq!(
        pending(&store, &spool, "codex:pending", AT, Duration::ZERO),
        Pending::Unknown
    );
    assert_eq!(
        pending(
            &store,
            &spool,
            "codex:pending",
            "invalid time",
            Duration::from_millis(300)
        ),
        Pending::Unknown
    );
    let check = || {
        pending(
            &store,
            &spool,
            "codex:pending",
            AT,
            Duration::from_millis(300),
        )
    };
    f.run(&["save", "--source", "codex:pending", "--nothing-new"], "")
        .unwrap();
    let (path, mut operation) = f.queued();
    operation.payload.created_at = AT.into();
    fs::remove_file(&path).unwrap();
    spool.publish(&operation).unwrap();
    let bytes = fs::read(&path).unwrap();
    fs::write(&path, b"broken header\nSYNTHETIC_PRIVATE_BODY").unwrap();
    assert_eq!(check(), Pending::Unknown);
    // Exhaust a positive budget while reading a header, not just at entry.
    fs::write(&path, vec![b' '; 1024 * 1024]).unwrap();
    assert_eq!(
        pending(
            &store,
            &spool,
            "codex:pending",
            AT,
            Duration::from_millis(10)
        ),
        Pending::Unknown
    );
    let end = bytes.iter().position(|&byte| byte == b'\n').unwrap() + 1;
    fs::write(
        &path,
        [&bytes[..end], b"broken payload: SYNTHETIC_PRIVATE_BODY"].concat(),
    )
    .unwrap();
    assert_eq!(check(), Pending::Unknown);
    fs::write(&path, &bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    assert_eq!(check(), Pending::Unknown);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(check(), Pending::Yes);
    store
        .connection()
        .execute_batch("CREATE TEMP VIEW spool_ops AS SELECT op_id FROM absent_table")
        .unwrap();
    assert_eq!(check(), Pending::Unknown);
    store
        .connection()
        .execute_batch("DROP VIEW temp.spool_ops")
        .unwrap();
    assert_eq!(check(), Pending::Yes);
    assert_eq!(count(&store, "confirmations"), 0);
    assert_eq!(count(&store, "spool_ops"), 0);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(
        store
            .connection()
            .query_row("PRAGMA busy_timeout", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2000
    );
}

#[test]
fn busy_and_sql_failure_roll_back_and_leave_file_for_retry() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    f.run(&["save"], "## 停点\nretryable").unwrap();
    let (path, _) = f.queued();
    let bytes = fs::read(&path).unwrap();
    let mut store = f.store();
    let mut other = f.store();
    let tx = other
        .transaction(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let started = Instant::now();
    let error = ingest(&mut store, &f.spool(), None).unwrap_err();
    assert!(
        matches!(error, cairn::ingest::Error::Store(cairn::store::Error::Sqlite(rusqlite::Error::SqliteFailure(ref e, _))) if e.code == rusqlite::ErrorCode::DatabaseBusy)
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(count(&store, "spool_ops"), 0);
    assert_eq!(
        store
            .connection()
            .query_row("PRAGMA busy_timeout", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2000
    );
    tx.rollback().unwrap();
    store
        .connection()
        .execute_batch(
            "CREATE TEMP TRIGGER fail_confirmation BEFORE INSERT ON confirmations
        BEGIN SELECT RAISE(ABORT,'synthetic SQL failure'); END;",
        )
        .unwrap();
    assert!(ingest(&mut store, &f.spool(), None).is_err());
    for table in [
        "records",
        "supersessions",
        "confirmations",
        "spool_ops",
        "sources",
        "events",
    ] {
        assert_eq!(count(&store, table), 0, "{table}");
    }
    assert_eq!(fs::read(&path).unwrap(), bytes);
    store
        .connection()
        .execute_batch("DROP TRIGGER fail_confirmation")
        .unwrap();
    assert_eq!(ingest(&mut store, &f.spool(), None).unwrap().ingested, 1);
    assert!(!path.exists());
}

#[test]
fn nothing_new_does_not_read_stdin_and_body_limit_counts_utf8_bytes() {
    struct Unreadable;
    impl Read for Unreadable {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("nothing-new must not read stdin");
        }
    }
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    cli::run_at(
        Cli::try_parse_from(["cairn", "save", "--nothing-new"]).unwrap(),
        &mut Unreadable,
        &f.cwd(),
        &f.db(),
        f.root.path(),
    )
    .unwrap();
    let mut store = f.store();
    assert_eq!(ingest(&mut store, &f.spool(), None).unwrap().ingested, 1);
    assert_eq!(count(&store, "records"), 0);
    assert_eq!(count(&store, "confirmations"), 1);
    let prefix = "## 停点\n";
    let body = format!("{prefix}{}", "x".repeat(6144 - prefix.len()));
    f.run(&["save"], &body).unwrap();
    assert_eq!(ingest(&mut store, &f.spool(), None).unwrap().ingested, 1);
    let oversized = format!("{prefix}{}", "中".repeat(2048));
    assert!(f.run(&["save"], &oversized).is_err());
    assert!(f.files().is_empty());
    assert!(Cli::try_parse_from(["cairn", "save", "--nothing-new", "--supersedes", "id"]).is_err());
}

#[test]
fn git_save_has_branch_short_head_and_canonical_collection_time() {
    let f = Fixture::new();
    f.git(&f.cwd(), &["init", "--initial-branch=main", "--template="]);
    f.git(&f.cwd(), &["config", "user.name", "Synthetic Test"]);
    f.git(
        &f.cwd(),
        &["config", "user.email", "synthetic@example.invalid"],
    );
    f.git(&f.cwd(), &["commit", "--allow-empty", "-m", "synthetic"]);
    let head = f.git(&f.cwd(), &["rev-parse", "HEAD"]);
    f.run(&["adopt"], "").unwrap();
    let message = f.run(&["save"], "## 停点\nGit facts").unwrap();
    assert!(message.ends_with(&format!("project · main · {}", &head[..7])));
    let (_, operation) = f.queued();
    assert_eq!(
        operation.payload.facts.as_ref().unwrap().head.as_deref(),
        Some(head.as_str())
    );
    timestamp(&Value::String(
        operation
            .payload
            .facts
            .as_ref()
            .unwrap()
            .collected_at
            .clone(),
    ));
    let mut store = f.store();
    ingest(&mut store, &f.spool(), None).unwrap();
    let facts: String = store
        .connection()
        .query_row("SELECT facts FROM records", [], |r| r.get(0))
        .unwrap();
    let facts: Value = serde_json::from_str(&facts).unwrap();
    assert!(facts.get("collected_at_unix_ms").is_none());
    timestamp(&facts["collected_at"]);
    let worktree = f.root.path().join("topic-worktree");
    f.git(
        &f.cwd(),
        &["worktree", "add", "-b", "topic", worktree.to_str().unwrap()],
    );
    let message = cli::run_at(
        Cli::try_parse_from(["cairn", "save"]).unwrap(),
        &mut "## 停点\nworktree".as_bytes(),
        &worktree,
        &f.db(),
        f.root.path(),
    )
    .unwrap();
    assert!(
        message.ends_with(&format!("project · topic · {}", &head[..7])),
        "{message}"
    );
    cli::run_at(
        Cli::try_parse_from(["cairn", "adopt"]).unwrap(),
        &mut std::io::empty(),
        &worktree,
        &f.db(),
        f.root.path(),
    )
    .unwrap();
    assert_eq!(count(&store, "projects"), 1);
    assert_eq!(count(&store, "records"), 2);
}

#[test]
fn unknown_sources_are_distinct_local_operations_and_known_sources_are_preserved() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let mut store = f.store();
    source(&store, "codex:known");
    for declared in [None, Some("codex:unknown"), Some("codex:known")] {
        let mut args = vec!["save"];
        if let Some(id) = declared {
            args.extend(["--source", id]);
        }
        f.run(&args, "## 停点\nsource test").unwrap();
        let (_, operation) = f.queued();
        assert_eq!(ingest(&mut store, &f.spool(), None).unwrap().ingested, 1);
        let (source, association, at): (String, String, String) = store.connection().query_row(
            "SELECT c.source_id,s.association,c.at FROM confirmations c JOIN sources s ON s.id=c.source_id WHERE c.op_id=?1",
            [&operation.header.op_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        if declared == Some("codex:known") {
            assert_eq!(source, "codex:known");
            assert_eq!(association, "hook");
        } else {
            assert_eq!(source, format!("local:{}", operation.header.op_id));
            assert_eq!(association, "uncertain");
        }
        assert_eq!(at, operation.payload.created_at);
    }
    assert_eq!(count(&store, "sources"), 3);
}

#[test]
fn replay_after_commit_before_unlink_is_idempotent_for_all_three_outcomes() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let mut store = f.store();
    for args in [
        vec!["save"],
        vec!["save", "--nothing-new"],
        vec!["save", "--supersedes", "missing"],
    ] {
        f.run(&args, "## 停点\nreplay").unwrap();
        let (path, mut operation) = f.queued();
        // Confirm that confirmation time comes from save, not collection time.
        operation.payload.created_at = AT.into();
        fs::remove_file(&path).unwrap();
        f.spool().publish(&operation).unwrap();
        let original = fs::read(&path).unwrap();
        assert_eq!(ingest(&mut store, &f.spool(), None).unwrap().processed, 1);
        let snapshot = database_snapshot(&store);
        fs::write(&path, &original).unwrap();
        assert_eq!(ingest(&mut store, &f.spool(), None).unwrap().replayed, 1);
        assert_eq!(database_snapshot(&store), snapshot);
        assert!(!path.exists());
    }
    assert_eq!(count(&store, "records"), 1);
    assert_eq!(count(&store, "confirmations"), 2);
    assert_eq!(count(&store, "events"), 1);
    assert_eq!(count(&store, "spool_ops"), 3);
    assert_eq!(
        store
            .connection()
            .query_row(
                "SELECT count(*) FROM confirmations WHERE at=?1",
                [AT],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        2
    );
}

fn database_snapshot(store: &Store) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
    [
        "projects",
        "sources",
        "records",
        "supersessions",
        "confirmations",
        "events",
        "spool_ops",
    ]
    .iter()
    .map(|table| {
        let mut statement = store
            .connection()
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let columns = statement.column_count();
        statement
            .query_map([], |r| (0..columns).map(|i| r.get(i)).collect())
            .unwrap()
            .map(Result::unwrap)
            .collect()
    })
    .collect()
}

#[test]
fn concurrent_saves_publish_distinct_files_and_collect_every_operation() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let cwd = f.cwd();
    let db = f.db();
    let root = f.root.path();
    let barrier = std::sync::Barrier::new(9);
    std::thread::scope(|threads| {
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let (barrier, cwd, db) = (&barrier, &cwd, &db);
                threads.spawn(move || {
                    barrier.wait();
                    cli::run_at(
                        Cli::try_parse_from(["cairn", "save"]).unwrap(),
                        &mut format!("## 停点\nparallel {i}").as_bytes(),
                        cwd,
                        db,
                        root,
                    )
                    .unwrap()
                })
            })
            .collect();
        barrier.wait();
        let messages: std::collections::HashSet<_> =
            handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(messages.len(), 8);
    });
    assert_eq!(f.spool().status().unwrap().pending_json, 8);
    let mut store = f.store();
    assert_eq!(ingest(&mut store, &f.spool(), None).unwrap().ingested, 8);
    assert_eq!(count(&store, "records"), 8);
    assert_eq!(count(&store, "confirmations"), 8);
    assert_eq!(f.spool().status().unwrap().pending_json, 0);
}

#[test]
fn database_namespaces_and_foreign_destinations_are_left_untouched() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    f.run(&["save"], "## 停点\nfirst database").unwrap();
    let (_, operation) = f.queued();
    let other_db = database_path(Some(&f.root.path().join("other-state")), None).unwrap();
    let other_spool = Spool::open(f.root.path(), &other_db).unwrap();
    let mut other_store = Store::open(&other_db, BusyTimeout::UserCommand).unwrap();
    let mut foreign = operation.clone();
    foreign.header.database_path = other_db.clone();
    other_spool.publish(&foreign).unwrap();
    let foreign_path = other_spool
        .path()
        .join(format!("{}.json", foreign.header.op_id));
    let bytes = fs::read(&foreign_path).unwrap();
    let mut store = f.store();
    assert_eq!(ingest(&mut store, &f.spool(), None).unwrap().ingested, 1);
    assert_eq!(fs::read(&foreign_path).unwrap(), bytes);
    assert_eq!(count(&other_store, "spool_ops"), 0);
    // Even a file copied into our namespace with an unreadable payload is ignored
    // by destination, rather than rejected or parsed as this database's operation.
    let wrong_path = f.spool().path().join(format!("{}.json", ulid::Ulid::new()));
    let header_end = bytes.iter().position(|&b| b == b'\n').unwrap() + 1;
    let mut invalid_payload = bytes[..header_end].to_vec();
    invalid_payload.extend_from_slice(b"not JSON or UTF-8: \xff\xfe");
    fs::write(&wrong_path, &invalid_payload).unwrap();
    assert_eq!(ingest(&mut store, &f.spool(), None).unwrap().processed, 0);
    assert_eq!(fs::read(&wrong_path).unwrap(), invalid_payload);
    assert_eq!(count(&store, "events"), 0);
    assert_eq!(
        ingest(&mut other_store, &other_spool, None)
            .unwrap()
            .rejected,
        1
    );
    assert_eq!(count(&other_store, "records"), 0);
    assert!(wrong_path.exists());
}

#[test]
fn exclusive_publication_preserves_existing_final_and_temp_files() {
    let f = Fixture::new();
    f.run(&["save"], "## 停点\noriginal").unwrap();
    let (path, mut operation) = f.queued();
    let original = fs::read(&path).unwrap();
    operation.payload.body = Some("## 停点\ncollision".into());
    let error = f.spool().publish(&operation).unwrap_err();
    assert!(
        matches!(error, cairn::spool::Error::Io(ref error) if error.kind() == std::io::ErrorKind::AlreadyExists)
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    let status = f.spool().status().unwrap();
    assert_eq!(status.pending_json, 1);
    assert_eq!(status.residual_tmp, 1);
    assert_eq!(status.path, path.parent().unwrap());
    let temp = path
        .parent()
        .unwrap()
        .join(format!(".{}.tmp", operation.header.op_id));
    let temp_bytes = fs::read(&temp).unwrap();
    assert!(f.spool().publish(&operation).is_err());
    assert_eq!(fs::read(&temp).unwrap(), temp_bytes);
    assert_eq!(fs::read(&path).unwrap(), original);
}

fn source(store: &Store, id: &str) {
    store
        .connection()
        .execute(
            "INSERT INTO sources(id,agent,session_id,association,first_seen,last_seen)
        VALUES (?1,'codex','synthetic','hook',?2,?2)",
            params![id, AT],
        )
        .unwrap();
}

#[test]
fn supersedes_requires_existing_live_injected_target_on_same_project_and_line() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let mut store = f.store();
    source(&store, "codex:synthetic");
    let scope = cairn::scope::Git::default().resolve(&f.cwd()).unwrap();
    let project: i64 = store
        .connection()
        .query_row("SELECT id FROM projects", [], |r| r.get(0))
        .unwrap();
    store
        .connection()
        .execute(
            "INSERT INTO projects(key,adopted,adopted_at) VALUES ('/synthetic-other-project',1,?1)",
            [AT],
        )
        .unwrap();
    let other = store.connection().last_insert_rowid();
    for (id, project_id, line, injected, deleted) in [
        (
            "valid",
            project,
            scope.line_path.to_str().unwrap(),
            true,
            false,
        ),
        ("cross-line", project, "/synthetic-other-line", true, false),
        (
            "cross-project",
            other,
            scope.line_path.to_str().unwrap(),
            true,
            false,
        ),
        (
            "not-injected",
            project,
            scope.line_path.to_str().unwrap(),
            false,
            false,
        ),
        (
            "deleted",
            project,
            scope.line_path.to_str().unwrap(),
            true,
            true,
        ),
    ] {
        store.connection().execute("INSERT INTO records(id,project_id,line_path,source_id,kind,body,created_at,deleted_at)
            VALUES (?1,?2,?3,'codex:synthetic','checkpoint',?4,?5,?6)",
            params![id, project_id, line, if deleted { None } else { Some("## 停点\nseed") }, AT, deleted.then_some(AT)]).unwrap();
        if injected {
            store
                .connection()
                .execute(
                    "INSERT INTO injections VALUES ('codex:synthetic',?1,?2)",
                    params![id, AT],
                )
                .unwrap();
        }
    }
    for invalid in [
        "cross-line",
        "cross-project",
        "not-injected",
        "deleted",
        "missing",
    ] {
        f.run(
            &[
                "save",
                "--source",
                "codex:synthetic",
                "--supersedes",
                "valid",
                "--supersedes",
                invalid,
            ],
            "## 停点\nrejected synthetic body",
        )
        .unwrap();
        assert_eq!(
            ingest(&mut store, &f.spool(), None).unwrap().rejected,
            1,
            "{invalid}"
        );
        assert_eq!(count(&store, "records"), 5);
        assert_eq!(count(&store, "supersessions"), 0);
        assert_eq!(count(&store, "confirmations"), 0);
    }
    f.run(
        &[
            "save",
            "--source",
            "codex:synthetic",
            "--supersedes",
            "valid",
            "--supersedes",
            "valid",
        ],
        "## 停点\naccepted replacement",
    )
    .unwrap();
    assert_eq!(ingest(&mut store, &f.spool(), None).unwrap().ingested, 1);
    assert_eq!(count(&store, "records"), 6);
    assert_eq!(count(&store, "supersessions"), 1);
    assert_eq!(count(&store, "confirmations"), 1);
}

fn count(store: &Store, table: &str) -> i64 {
    store
        .connection()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn commands_ingest_before_adoption_and_adoption_transitions_are_idempotent() {
    let f = Fixture::new();
    f.run(&["save"], "## 停点\nnot adopted yet").unwrap();
    let operation: Value = serde_json::from_slice(&fs::read(&f.files()[0]).unwrap()).unwrap();
    f.run(&["adopt"], "").unwrap();
    let store = f.store();
    assert_eq!(count(&store, "records"), 0);
    assert_eq!(count(&store, "confirmations"), 0);
    let (kind, detail): (String, String) = store
        .connection()
        .query_row("SELECT kind, detail FROM events", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(kind, "save_rejected");
    assert!(!detail.contains("not adopted yet"));
    assert!(detail.contains(operation["header"]["op_id"].as_str().unwrap()));
    assert!(f.files().is_empty());
    let adopted_at: String = store
        .connection()
        .query_row("SELECT adopted_at FROM projects WHERE adopted=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    timestamp(&Value::String(adopted_at.clone()));
    f.run(&["adopt"], "").unwrap();
    assert_eq!(
        store
            .connection()
            .query_row("SELECT adopted_at FROM projects", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        adopted_at
    );
    f.run(
        &["save", "--source", "codex:never-seen"],
        "## 停点\naccepted before unadopt",
    )
    .unwrap();
    f.run(&["unadopt"], "").unwrap();
    assert_eq!(count(&store, "records"), 1);
    assert_eq!(count(&store, "confirmations"), 1);
    let (adopted, unadopted_at): (bool, String) = store
        .connection()
        .query_row("SELECT adopted, unadopted_at FROM projects", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(!adopted);
    timestamp(&Value::String(unadopted_at.clone()));
    f.run(&["unadopt"], "").unwrap();
    assert_eq!(
        store
            .connection()
            .query_row("SELECT unadopted_at FROM projects", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        unadopted_at
    );
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for (key, _) in std::env::vars_os().filter(|(key, _)| isolated_key(key)) {
            std::env::remove_var(key);
        }
        for (key, value) in &self.saved_env {
            std::env::set_var(key, value);
        }
    }
}

fn timestamp(value: &Value) {
    let s = value.as_str().unwrap();
    assert_eq!(s.len(), 24);
    assert_eq!(&s[19..20], ".");
    assert!(s.ends_with('Z'));
    chrono::DateTime::parse_from_rfc3339(s).unwrap();
}

#[test]
fn cli_rejects_invalid_bodies_without_creating_state() {
    let _guard = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    fs::create_dir(&home).unwrap();
    for body in [
        String::new(),
        "no stopping point".into(),
        format!("## 停点\n{}", "x".repeat(6144)),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cairn"))
            .arg("save")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", &home)
            .env("XDG_STATE_HOME", root.path().join("state"))
            .env("TMPDIR", root.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .current_dir(&home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(body.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success(), "invalid body must fail");
        assert!(output.stdout.is_empty());
        assert_eq!(String::from_utf8(output.stderr).unwrap().lines().count(), 1);
        assert!(!root.path().join("state").exists());
        assert!(!root.path().join("cairn-spool").exists());
    }
}

#[test]
fn save_publishes_private_versioned_operation_without_database_access() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    // An unusable database path must not matter to save.
    fs::write(f.root.path().join("cairn"), "not a directory").unwrap();
    let message = f
        .run(
            &["save", "--source", "codex:synthetic"],
            "## 停点\nsynthetic checkpoint",
        )
        .unwrap();
    assert_eq!(message.lines().count(), 1);
    assert!(message.starts_with("saved "));
    assert!(message.contains("project"));
    let files = f.files();
    assert_eq!(files.len(), 1);
    let entry: Value = serde_json::from_slice(&fs::read(&files[0]).unwrap()).unwrap();
    assert_eq!(entry["header"]["version"], 1);
    let id = entry["header"]["op_id"].as_str().unwrap();
    assert_eq!(files[0].file_stem().unwrap(), id);
    assert!(message.contains(id));
    assert_eq!(entry["header"]["database_path"], f.db().to_str().unwrap());
    assert_eq!(entry["header"]["source"], "codex:synthetic");
    assert_eq!(entry["payload"]["body"], "## 停点\nsynthetic checkpoint");
    assert!(entry["payload"]["facts"].is_null());
    timestamp(&entry["payload"]["created_at"]);
    assert_eq!(
        fs::metadata(&files[0]).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(files[0].parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("cairn")).unwrap(),
        "not a directory"
    );
}

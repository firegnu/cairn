use cairn::save::{Header, Operation, Payload};
use cairn::spool::Spool;
use cairn::store::{database_path, BusyTimeout, Store};
use cairn::turn::{self, Action, Agent, Context, Report};
use rusqlite::params;
use std::path::PathBuf;

const SESSION: &str = "2026-10-05T00:00:00.000Z";
const START: &str = "2026-10-05T00:01:00.000Z";
const END: &str = "2026-10-05T00:02:00.000Z";

struct Fixture {
    root: tempfile::TempDir,
    database: PathBuf,
    project: PathBuf,
    scope: cairn::scope::Scope,
}

impl Fixture {
    fn new(adopted: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        // Explicit temporary HOME / XDG_STATE_HOME values, without changing the
        // test process environment or opening any real state/spool directory.
        let home = root.path().join("home");
        let state = root.path().join("state");
        let database = database_path(Some(&state), Some(&home)).unwrap();
        let project = root.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let fixture = Self {
            root,
            database,
            scope: cairn::scope::Scope {
                project_key: project.clone(),
                line_path: project.clone(),
                is_git: false,
            },
            project,
        };
        fixture
            .store()
            .connection()
            .execute(
                "INSERT INTO projects(key,adopted,adopted_at) VALUES (?1,?2,?3)",
                params![fixture.project.to_str().unwrap(), adopted, SESSION],
            )
            .unwrap();
        fixture
    }

    fn store(&self) -> Store {
        Store::open(&self.database, BusyTimeout::Hook).unwrap()
    }

    fn spool(&self) -> Spool {
        Spool::open(self.root.path(), &self.database).unwrap()
    }

    fn session_start(&self) {
        let store = self.store();
        store
            .connection()
            .execute(
                "INSERT INTO sources(id,agent,session_id,association,first_seen,last_seen)
             VALUES ('codex:synthetic','codex','synthetic','hook',?1,?1)",
                [SESSION],
            )
            .unwrap();
        store.connection().execute(
            "INSERT INTO events(source_id,kind,at) VALUES ('codex:synthetic','session_started',?1)", [SESSION],
        ).unwrap();
    }

    fn publish(&self, body: Option<&str>, at: &str) -> String {
        let id = ulid::Ulid::new().to_string();
        self.spool()
            .publish(&Operation {
                header: Header {
                    version: 1,
                    op_id: id.clone(),
                    database_path: self.database.clone(),
                    source: Some("codex:synthetic".into()),
                },
                payload: Payload {
                    cwd: self.project.clone(),
                    project_key: self.project.clone(),
                    line_path: self.project.clone(),
                    branch: None,
                    kind: "checkpoint".into(),
                    body: body.map(str::to_owned),
                    nothing_new: body.is_none(),
                    supersedes: Vec::new(),
                    facts: None,
                    created_at: at.into(),
                },
            })
            .unwrap();
        id
    }

    fn context<'a>(&'a self, at: &'a str) -> Context<'a> {
        Context {
            database: &self.database,
            spool_root: self.root.path(),
            scope: &self.scope,
            transcript_path: Some("/synthetic/transcript"),
            agent: Agent::Codex,
            session_id: "synthetic",
            at,
            disabled: false,
        }
    }
}

fn allowed(report: Report) {
    assert_eq!(report.action, Action::Allow);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

#[test]
fn turn_start_registers_hook_source_and_records_received_time() {
    let f = Fixture::new(true);
    for (agent, source) in [
        (Agent::Codex, "codex:synthetic"),
        (Agent::Claude, "claude:synthetic"),
    ] {
        let context = Context {
            agent,
            ..f.context(START)
        };
        allowed(turn::turn_started(context, Some("turn-1")));
        allowed(turn::turn_started(
            Context { at: END, ..context },
            Some("turn-2"),
        ));
        let store = f.store();
        let row: (String, String, String, String, String) = store
            .connection()
            .query_row(
                "SELECT agent,session_id,association,first_seen,last_seen FROM sources WHERE id=?1",
                [source],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            (
                source.split(':').next().unwrap().into(),
                "synthetic".into(),
                "hook".into(),
                START.into(),
                END.into()
            )
        );
        let events: Vec<(String, String)> = store.connection().prepare(
            "SELECT detail,at FROM events WHERE source_id=?1 AND kind='turn_started' ORDER BY id",
        ).unwrap().query_map([source], |r| Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap();
        assert_eq!(
            events,
            [
                ("turn-1".into(), START.into()),
                ("turn-2".into(), END.into())
            ]
        );
    }
    assert!(!f.root.path().join("cairn-spool").exists());
}

#[test]
fn duplicate_turn_end_is_always_allowed_without_decisions() {
    let f = Fixture::new(true);
    allowed(turn::turn_started(f.context(START), Some("turn-1")));
    allowed(turn::turn_ended(f.context(END)));
    allowed(turn::turn_ended(f.context("2026-10-05T00:02:01.000Z")));
    assert!(decisions(&f).is_empty());
}

fn decisions(f: &Fixture) -> Vec<(String, String, String)> {
    f.store().connection().prepare(
        "SELECT turn_key,outcome,at FROM turn_decisions WHERE source_id='codex:synthetic' ORDER BY at,turn_key,outcome",
    ).unwrap().query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<_, _>>().unwrap()
}

#[test]
fn turn_end_does_not_read_legacy_confirmation_or_decision_tables() {
    let f = Fixture::new(true);
    f.session_start();
    let store = f.store();
    // These tables remain in production. Removing them in the fixture proves
    // that an empty-spool Stop no longer reads either decision input.
    store
        .connection()
        .execute_batch("DROP TABLE confirmations; DROP TABLE turn_decisions")
        .unwrap();
    allowed(turn::turn_ended(f.context(END)));
    let at: String = store
        .connection()
        .query_row("SELECT last_event_at FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(at, END);
}

#[test]
fn repeated_stop_is_allowed_with_or_without_confirmation() {
    for confirmed in [false, true] {
        let f = Fixture::new(true);
        allowed(turn::turn_started(f.context(START), Some("turn-1")));
        allowed(turn::turn_ended(f.context(END)));
        if confirmed {
            confirm(&f, "codex:synthetic", "nothing_new", END);
        }
        for _ in 0..4 {
            allowed(turn::turn_ended(f.context("2026-10-05T00:03:00.000Z")));
        }
        assert!(decisions(&f).is_empty());
        let turns: i64 = f
            .store()
            .connection()
            .query_row("SELECT turns FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(turns, 1);
    }
}

fn confirm(f: &Fixture, source: &str, kind: &str, at: &str) {
    f.store()
        .connection()
        .execute(
            "INSERT INTO confirmations(source_id,kind,at) VALUES (?1,?2,?3)",
            params![source, kind, at],
        )
        .unwrap();
}

#[test]
fn turn_end_collects_voluntary_save_and_nothing_new_without_deciding() {
    for body in [None, Some("## 停点\n自愿保存")] {
        let f = Fixture::new(true);
        allowed(turn::turn_started(f.context(START), Some("turn-1")));
        let id = f.publish(body, START);
        allowed(turn::turn_ended(f.context(END)));
        assert!(decisions(&f).is_empty());
        let confirmation: (String, String) = f
            .store()
            .connection()
            .query_row("SELECT op_id,at FROM confirmations", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(confirmation, (id, START.into()));
        assert_eq!(f.spool().status().unwrap().pending_json, 0);
        assert_eq!(activity_counts(&f)[5], i64::from(body.is_some()));
    }
}

#[test]
fn unprocessed_or_unknown_spool_never_changes_allow_or_writes_decisions() {
    for unknown in [false, true] {
        let f = Fixture::new(true);
        allowed(turn::turn_started(f.context(START), Some("turn-1")));
        if unknown {
            let id = f.publish(None, START);
            std::fs::write(
                f.spool().path().join(format!("{id}.json")),
                "unreadable synthetic header\n",
            )
            .unwrap();
        } else {
            for _ in 0..51 {
                f.publish(Some("SYNTHETIC_BODY_WITHOUT_HEADING"), START);
            }
        }
        allowed(turn::turn_ended(f.context(END)));
        assert!(decisions(&f).is_empty());
        assert!(f.spool().status().unwrap().pending_json > 0);
        assert_eq!(activity_counts(&f)[3], 0);
    }
}

#[test]
fn rejection_is_recorded_without_body_or_continuation() {
    let f = Fixture::new(true);
    allowed(turn::turn_started(f.context(START), Some("turn-1")));
    let id = f.publish(Some("PRIVATE_SYNTHETIC_BODY"), START);
    // Old rejection metadata is not parsed to construct continuation messages.
    f.store().connection().execute(
        "INSERT INTO events(source_id,kind,at,detail) VALUES ('codex:synthetic','save_rejected',?1,'not JSON')", [START]
    ).unwrap();
    allowed(turn::turn_ended(f.context(END)));
    let (outcome, reason): (String, String) = f
        .store()
        .connection()
        .query_row(
            "SELECT outcome,reason FROM spool_ops WHERE op_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(outcome, "rejected");
    assert!(reason.contains("正文必须有 ## 停点 一节"));
    assert!(!reason.contains("PRIVATE_SYNTHETIC_BODY"));
    assert!(decisions(&f).is_empty());
    assert_eq!(f.spool().status().unwrap().pending_json, 0);
}

#[test]
fn session_end_writes_only_its_event_and_does_not_collect() {
    let f = Fixture::new(true);
    let id = f.publish(Some("## 停点\nSYNTHETIC_BODY"), START);
    allowed(turn::session_ended(f.context(END), Some("other")));
    let store = f.store();
    let event: (String, String, String, String) = store
        .connection()
        .query_row("SELECT source_id,kind,at,detail FROM events", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap();
    assert_eq!(
        event,
        (
            "codex:synthetic".into(),
            "session_ended".into(),
            END.into(),
            "other".into()
        )
    );
    assert_eq!(activity_counts(&f), [0, 1, 0, 0, 0, 0]);
    assert!(f.spool().path().join(format!("{id}.json")).exists());
}

fn activity_counts(f: &Fixture) -> [i64; 6] {
    let store = f.store();
    [
        "sources",
        "events",
        "turn_decisions",
        "confirmations",
        "spool_ops",
        "records",
    ]
    .map(|table| {
        store
            .connection()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
}

#[test]
fn missing_turn_key_or_start_still_records_the_session_without_decisions() {
    for has_start in [false, true] {
        let f = Fixture::new(true);
        if has_start {
            allowed(turn::turn_started(f.context(START), None));
        }
        allowed(turn::turn_ended(f.context(END)));
        allowed(turn::turn_ended(f.context(END)));
        assert!(decisions(&f).is_empty());
        let turns: i64 = f
            .store()
            .connection()
            .query_row("SELECT turns FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(turns, i64::from(has_start));
    }
}

#[test]
fn late_start_and_interleaved_ends_preserve_observed_turn_counts() {
    let f = Fixture::new(true);
    allowed(turn::turn_ended(f.context(END)));
    allowed(turn::turn_started(
        Context {
            transcript_path: Some("/synthetic/stale"),
            ..f.context(START)
        },
        Some("A"),
    ));
    let metadata: (String, String) = f
        .store()
        .connection()
        .query_row(
            "SELECT last_event_at,transcript_path FROM sessions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(metadata, (END.into(), "/synthetic/transcript".into()));
    allowed(turn::turn_started(f.context(END), Some("B")));
    for _ in 0..3 {
        allowed(turn::turn_ended(f.context(END)));
    }
    assert!(decisions(&f).is_empty());
    let turns: i64 = f
        .store()
        .connection()
        .query_row("SELECT turns FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(turns, 2);
}

#[test]
fn concurrent_duplicate_starts_increment_atomically_and_ends_are_allowed() {
    let f = Fixture::new(true);
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    allowed(turn::turn_started(f.context(START), Some("same-turn")));
                    allowed(turn::turn_ended(f.context(END)));
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
    assert!(decisions(&f).is_empty());
    let turns: i64 = f
        .store()
        .connection()
        .query_row("SELECT turns FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(turns, 4);
}

fn all_entries(context: Context<'_>) -> [Report; 3] {
    [
        turn::turn_started(context, Some("turn-1")),
        turn::turn_ended(context),
        turn::session_ended(context, Some("other")),
    ]
}

#[test]
fn database_busy_and_sql_failure_allow_with_errors_and_no_partial_events() {
    for busy in [true, false] {
        let f = Fixture::new(true);
        f.session_start();
        let store = f.store();
        if busy {
            store.connection().execute_batch("BEGIN IMMEDIATE").unwrap();
        } else {
            store.connection().execute_batch(
                "CREATE TRIGGER fail_seen BEFORE INSERT ON hook_seen BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;"
            ).unwrap();
        }
        for result in all_entries(f.context(END)) {
            assert_eq!(result.action, Action::Allow);
            assert!(!result.errors.is_empty());
        }
        if busy {
            store.connection().execute_batch("ROLLBACK").unwrap();
        }
        assert_eq!(activity_counts(&f), [1, 1, 0, 0, 0, 0]);
        let last_seen: String = store
            .connection()
            .query_row("SELECT last_seen FROM sources", [], |r| r.get(0))
            .unwrap();
        assert_eq!(last_seen, SESSION);
        let sessions: i64 = store
            .connection()
            .query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sessions, 0);
        if !busy {
            store
                .connection()
                .execute_batch("DROP TRIGGER fail_seen")
                .unwrap();
        }
        allowed(turn::turn_ended(f.context(END)));
    }
}

#[test]
fn disabled_unadopted_and_missing_database_never_write_or_collect() {
    for (adopted, disabled) in [(true, true), (false, false)] {
        let f = Fixture::new(adopted);
        let id = f.publish(None, START);
        let path = f.spool().path().join(format!("{id}.json"));
        let bytes = std::fs::read(&path).unwrap();
        for report in all_entries(Context {
            disabled,
            ..f.context(END)
        }) {
            allowed(report);
        }
        assert_eq!(activity_counts(&f), [0; 6]);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    let f = Fixture::new(true);
    let missing = f.root.path().join("absent-state/cairn/cairn.db");
    for report in all_entries(Context {
        database: &missing,
        ..f.context(END)
    }) {
        allowed(report);
    }
    assert!(!missing.parent().unwrap().exists());
    assert!(!f.root.path().join("cairn-spool").exists());
}

#[test]
fn historical_confirmations_and_decisions_are_preserved_but_do_not_affect_stop() {
    for origin in ["turn", "previous", "session", "source"] {
        for valid in [false, true] {
            let f = Fixture::new(true);
            f.session_start();
            match origin {
                "turn" => allowed(turn::turn_started(f.context(START), Some("start"))),
                "previous" => {
                    f.store().connection().execute(
                        "INSERT INTO turn_decisions VALUES ('codex:synthetic','prior','continue_requested',?1)", [END]
                    ).unwrap();
                }
                "source" => {
                    f.store()
                        .connection()
                        .execute("DELETE FROM events", [])
                        .unwrap();
                }
                _ => {}
            }
            confirm(&f, "codex:other", "nothing_new", END);
            confirm(&f, "codex:synthetic", "nothing_new", SESSION);
            if valid {
                confirm(&f, "codex:synthetic", "saved", START);
            }
            let before = decisions(&f);
            allowed(turn::turn_ended(f.context(END)));
            assert_eq!(decisions(&f), before);
        }
    }
}

#[test]
fn ingestion_failure_still_records_session_metadata_and_can_retry() {
    for case in ["pending", "confirmed", "other-source"] {
        let f = Fixture::new(true);
        allowed(turn::turn_started(f.context(START), Some("turn-1")));
        if case == "confirmed" {
            confirm(&f, "codex:synthetic", "nothing_new", START);
        }
        let context = Context {
            session_id: if case == "other-source" {
                "other"
            } else {
                "synthetic"
            },
            ..f.context(END)
        };
        let id = f.publish(None, START);
        let store = f.store();
        store.connection().execute_batch(
            "CREATE TRIGGER fail_ingest BEFORE INSERT ON spool_ops BEGIN SELECT RAISE(ABORT,'synthetic ingest failure'); END;"
        ).unwrap();
        let report = turn::turn_ended(context);
        assert_eq!(report.action, Action::Allow);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0]
            .to_string()
            .contains("synthetic ingest failure"));
        assert!(f.spool().path().join(format!("{id}.json")).exists());
        assert!(decisions(&f).is_empty());
        let at: String = store
            .connection()
            .query_row(
                "SELECT last_event_at FROM sessions WHERE source_id=?1",
                [format!("codex:{}", context.session_id)],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(at, END);
        store
            .connection()
            .execute_batch("DROP TRIGGER fail_ingest")
            .unwrap();
        allowed(turn::turn_ended(context));
        assert!(decisions(&f).is_empty());
        assert_eq!(f.spool().status().unwrap().pending_json, 0);
    }
}

#[test]
fn spool_open_failure_preserves_session_metadata_and_error() {
    use std::os::unix::fs::PermissionsExt;

    let f = Fixture::new(true);
    allowed(turn::turn_started(f.context(START), Some("turn-1")));
    confirm(
        &f,
        "codex:synthetic",
        "nothing_new",
        "2026-10-05T00:01:30.000Z",
    );
    let spool_directory = f.root.path().join("cairn-spool");
    std::fs::create_dir(&spool_directory).unwrap();
    std::fs::set_permissions(&spool_directory, std::fs::Permissions::from_mode(0o755)).unwrap();

    let result = turn::turn_ended(f.context(END));

    assert_eq!(result.action, Action::Allow);
    assert!(matches!(
        result.errors.as_slice(),
        [turn::Error::Spool(cairn::spool::Error::UnsafeDirectory)]
    ));
    assert!(decisions(&f).is_empty());
    assert_eq!(activity_counts(&f), [1, 1, 0, 1, 0, 0]);
    assert_eq!(
        std::fs::metadata(&spool_directory)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(std::fs::read_dir(spool_directory).unwrap().count(), 0);
}

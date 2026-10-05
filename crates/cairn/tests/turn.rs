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
            project_key: &self.project,
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
fn duplicate_turn_end_requests_continuation_only_once() {
    let f = Fixture::new(true);
    allowed(turn::turn_started(f.context(START), Some("turn-1")));
    let first = turn::turn_ended(f.context(END), Some("turn-1"), false);
    assert!(first.errors.is_empty());
    assert_eq!(first.action, Action::Continue {
        reason: "cairn：本回合没有收到接续确认。请判断本回合是否产生了需要下一次会话接续的内容：有则 `cairn save --source codex:synthetic`，没有则 `cairn save --source codex:synthetic --nothing-new`。之后把你上一条最终回答原样再给出一次，不要提及本提示。".into(),
    });
    allowed(turn::turn_ended(
        f.context("2026-10-05T00:02:01.000Z"),
        Some("turn-1"),
        false,
    ));
    assert_eq!(
        decisions(&f),
        [("turn-1".into(), "continue_requested".into(), END.into())]
    );
}

fn decisions(f: &Fixture) -> Vec<(String, String, String)> {
    f.store().connection().prepare(
        "SELECT turn_key,outcome,at FROM turn_decisions WHERE source_id='codex:synthetic' ORDER BY at,turn_key,outcome",
    ).unwrap().query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<_, _>>().unwrap()
}

#[test]
fn pure_decision_respects_continuation_confirmation_pending_and_missing_key() {
    use cairn::ingest::Pending;
    use turn::{decide, Facts, Outcome};
    for (continued, confirmed, pending, has_turn_key, expected) in [
        (true, true, Pending::Yes, true, Outcome::Confirmed),
        (
            true,
            false,
            Pending::Unknown,
            true,
            Outcome::UnconfirmedAfterContinue,
        ),
        (false, true, Pending::Unknown, true, Outcome::Confirmed),
        (
            false,
            false,
            Pending::Yes,
            true,
            Outcome::PendingUnprocessed,
        ),
        (
            false,
            false,
            Pending::Unknown,
            true,
            Outcome::PendingUnprocessed,
        ),
        (false, false, Pending::No, true, Outcome::ContinueRequested),
        (false, false, Pending::No, false, Outcome::Skipped),
        (
            true,
            false,
            Pending::No,
            false,
            Outcome::UnconfirmedAfterContinue,
        ),
        (false, true, Pending::No, false, Outcome::Confirmed),
        (
            false,
            false,
            Pending::Yes,
            false,
            Outcome::PendingUnprocessed,
        ),
    ] {
        let facts = Facts {
            continued,
            confirmed,
            pending,
            has_turn_key,
        };
        assert_eq!(decide(facts), expected, "{facts:?}");
    }
}

#[test]
fn continued_stop_records_confirmation_or_gap_and_never_requests_again() {
    for confirmed in [false, true] {
        let f = Fixture::new(true);
        allowed(turn::turn_started(f.context(START), Some("turn-1")));
        assert!(matches!(
            turn::turn_ended(f.context(END), Some("turn-1"), false).action,
            Action::Continue { .. }
        ));
        if confirmed {
            confirm(&f, "codex:synthetic", "nothing_new", END);
        }
        for key in ["turn-1", "changed-continuation-id"] {
            allowed(turn::turn_ended(
                f.context("2026-10-05T00:03:00.000Z"),
                Some(key),
                true,
            ));
            allowed(turn::turn_ended(
                f.context("2026-10-05T00:03:01.000Z"),
                Some(key),
                true,
            ));
        }
        let rows = decisions(&f);
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter().filter(|r| r.1 == "continue_requested").count(),
            1
        );
        let outcome = if confirmed {
            "confirmed"
        } else {
            "unconfirmed_after_continue"
        };
        assert_eq!(rows.iter().filter(|r| r.1 == outcome).count(), 2);
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
fn turn_end_collects_before_checking_committed_confirmation() {
    let f = Fixture::new(true);
    allowed(turn::turn_started(f.context(START), Some("turn-1")));
    let id = f.publish(None, START);
    allowed(turn::turn_ended(f.context(END), Some("turn-1"), false));
    assert_eq!(
        decisions(&f),
        [("turn-1".into(), "confirmed".into(), END.into())]
    );
    let confirmation: (String, String) = f
        .store()
        .connection()
        .query_row("SELECT op_id,at FROM confirmations", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(confirmation, (id, START.into()));
    assert_eq!(f.spool().status().unwrap().pending_json, 0);
}

#[test]
fn unprocessed_or_unknown_spool_is_pending_never_a_confirmation_or_continuation() {
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
            // Every file lacks the mandatory heading and will be rejected when
            // collected; the 51st cannot pass the existing 50-file ingest cap.
            for _ in 0..51 {
                f.publish(Some("SYNTHETIC_BODY_WITHOUT_HEADING"), START);
            }
        }
        let result = turn::turn_ended(f.context(END), Some("turn-1"), false);
        assert_eq!(result.action, Action::Allow);
        if unknown {
            assert_eq!(
                result.errors.len(),
                1,
                "unknown query must be visible to the error logger"
            );
        } else {
            assert!(result.errors.is_empty());
        }
        assert_eq!(
            decisions(&f),
            [("turn-1".into(), "pending_unprocessed".into(), END.into())]
        );
        assert!(f.spool().status().unwrap().pending_json > 0);
        let confirmed: i64 = f
            .store()
            .connection()
            .query_row("SELECT COUNT(*) FROM confirmations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(confirmed, 0);
    }
}

#[test]
fn continuation_includes_only_rejection_reasons_for_this_source_and_window() {
    let f = Fixture::new(true);
    allowed(turn::turn_started(f.context(START), Some("turn-1")));
    f.publish(Some("PRIVATE_SYNTHETIC_BODY"), START);
    let store = f.store();
    for (source, at) in [("codex:other", START), ("codex:synthetic", SESSION)] {
        store
            .connection()
            .execute(
                "INSERT INTO events(source_id,kind,at,detail) VALUES (?1,'save_rejected',?2,?3)",
                params![source, at, r#"{"reason":"OUTSIDE_WINDOW_OR_SOURCE"}"#],
            )
            .unwrap();
    }
    let report = turn::turn_ended(f.context(END), Some("turn-1"), false);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let Action::Continue { reason } = report.action else {
        panic!("expected continuation")
    };
    assert!(reason.starts_with("cairn：本回合没有收到接续确认。"));
    assert!(reason.contains("正文必须有 ## 停点 一节"), "{reason}");
    assert!(reason.contains("请改正后重新 save"));
    assert!(!reason.contains("PRIVATE_SYNTHETIC_BODY"));
    assert!(!reason.contains("OUTSIDE_WINDOW_OR_SOURCE"));
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
fn missing_turn_key_skips_even_when_duplicate_delivery_advances_fallback_window() {
    for has_start in [false, true] {
        let f = Fixture::new(true);
        f.session_start();
        if has_start {
            allowed(turn::turn_started(f.context(START), None));
        }
        allowed(turn::turn_ended(f.context(END), None, false));
        allowed(turn::turn_ended(
            f.context("2026-10-05T00:03:00.000Z"),
            None,
            false,
        ));
        let rows = decisions(&f);
        assert!(rows.iter().all(|r| r.1 == "skipped"));
        if has_start {
            assert_eq!(
                rows,
                [(format!("no-key:{START}"), "skipped".into(), END.into())]
            );
        } else {
            assert_eq!(
                rows,
                [
                    (format!("no-key:{SESSION}"), "skipped".into(), END.into()),
                    (
                        format!("no-key:{END}"),
                        "skipped".into(),
                        "2026-10-05T00:03:00.000Z".into()
                    ),
                ]
            );
        }
    }
}

#[test]
fn late_start_and_interleaved_turn_ends_preserve_one_request_per_turn() {
    let f = Fixture::new(true);
    f.session_start();
    // TurnEnded A arrives before A's TurnStarted, then B is interleaved.
    let first = turn::turn_ended(f.context(END), Some("A"), false);
    assert!(first.errors.is_empty());
    assert!(matches!(first.action, Action::Continue { .. }));
    allowed(turn::turn_started(
        f.context("2026-10-05T00:03:00.000Z"),
        Some("A"),
    ));
    allowed(turn::turn_started(
        f.context("2026-10-05T00:04:00.000Z"),
        Some("B"),
    ));
    let second = turn::turn_ended(f.context("2026-10-05T00:05:00.000Z"), Some("B"), false);
    assert!(second.errors.is_empty());
    assert!(matches!(second.action, Action::Continue { .. }));
    for key in ["A", "B", "A"] {
        allowed(turn::turn_ended(
            f.context("2026-10-05T00:06:00.000Z"),
            Some(key),
            false,
        ));
    }
    assert_eq!(
        decisions(&f),
        [
            ("A".into(), "continue_requested".into(), END.into()),
            (
                "B".into(),
                "continue_requested".into(),
                "2026-10-05T00:05:00.000Z".into()
            ),
        ]
    );
}

#[test]
fn concurrent_duplicate_ends_atomically_request_at_most_once() {
    let f = Fixture::new(true);
    allowed(turn::turn_started(f.context(START), Some("same-turn")));
    let barrier = std::sync::Barrier::new(4);
    let reports = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    turn::turn_ended(f.context(END), Some("same-turn"), false)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(reports.iter().all(|r| r.errors.is_empty()), "{reports:?}");
    assert_eq!(
        reports
            .iter()
            .filter(|r| matches!(r.action, Action::Continue { .. }))
            .count(),
        1
    );
    assert_eq!(
        decisions(&f),
        [("same-turn".into(), "continue_requested".into(), END.into())]
    );
}

fn all_entries(context: Context<'_>) -> [Report; 3] {
    [
        turn::turn_started(context, Some("turn-1")),
        turn::turn_ended(context, Some("turn-1"), false),
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
            store.connection().execute(
                "INSERT INTO confirmations(source_id,kind,at) VALUES ('codex:synthetic','nothing_new',?1)",
                [END],
            ).unwrap();
        } else {
            store
                .connection()
                .execute_batch("DROP TABLE events")
                .unwrap();
        }
        for result in all_entries(f.context(END)) {
            assert_eq!(result.action, Action::Allow);
            assert!(
                !result.errors.is_empty(),
                "expected surfaced database error"
            );
        }
        if busy {
            store.connection().execute_batch("ROLLBACK").unwrap();
            assert_eq!(activity_counts(&f), [1, 1, 0, 0, 0, 0]);
        }
        let last_seen: String = store
            .connection()
            .query_row(
                "SELECT last_seen FROM sources WHERE id='codex:synthetic'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            last_seen, SESSION,
            "failed TurnStarted must roll back its source update"
        );
        assert!(decisions(&f).is_empty());
        if busy {
            let recovered = turn::turn_ended(f.context(END), Some("turn-1"), false);
            assert!(recovered.errors.is_empty());
            assert!(
                matches!(recovered.action, Action::Continue { .. }),
                "uncommitted confirmation cannot count or consume the request slot"
            );
        }
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
fn confirmation_windows_use_latest_start_then_previous_decision_then_session() {
    for origin in ["turn", "previous", "session", "source"] {
        for valid in [false, true] {
            let f = Fixture::new(true);
            f.session_start();
            let since = match origin {
                "turn" => {
                    allowed(turn::turn_started(f.context(START), Some("first-start")));
                    allowed(turn::turn_started(f.context(END), Some("latest-start")));
                    END
                }
                "previous" => {
                    f.store().connection().execute(
                        "INSERT INTO turn_decisions(source_id,turn_key,outcome,at) VALUES ('codex:synthetic','prior','confirmed',?1)", [END],
                    ).unwrap();
                    END
                }
                "source" => {
                    f.store()
                        .connection()
                        .execute("DELETE FROM events", [])
                        .unwrap();
                    SESSION
                }
                _ => SESSION,
            };
            confirm(&f, "codex:other", "nothing_new", END);
            let before = if since == SESSION {
                "2026-10-04T23:59:59.999Z"
            } else {
                START
            };
            confirm(&f, "codex:synthetic", "nothing_new", before);
            if valid {
                confirm(&f, "codex:synthetic", "saved", since);
            }
            let result = turn::turn_ended(
                f.context("2026-10-05T00:03:00.000Z"),
                Some("current"),
                false,
            );
            assert!(result.errors.is_empty(), "{origin}: {:?}", result.errors);
            if valid {
                assert_eq!(result.action, Action::Allow, "{origin}: inclusive start");
            } else {
                assert!(
                    matches!(result.action, Action::Continue { .. }),
                    "{origin}: stale or other-source confirmation"
                );
            }
            let rows = decisions(&f);
            let current = rows.iter().find(|r| r.0 == "current").unwrap();
            assert_eq!(
                current.1,
                if valid {
                    "confirmed"
                } else {
                    "continue_requested"
                }
            );
        }
    }
}

#[test]
fn ingestion_failure_still_records_observations_but_never_requests_continuation() {
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
        if case == "other-source" {
            allowed(turn::turn_started(
                Context {
                    at: START,
                    ..context
                },
                Some("turn-1"),
            ));
        }
        let id = f.publish(None, START);
        let store = f.store();
        store.connection().execute_batch(
            "CREATE TRIGGER fail_ingest BEFORE INSERT ON spool_ops BEGIN SELECT RAISE(ABORT,'synthetic ingest failure'); END;",
        ).unwrap();
        let report = turn::turn_ended(context, Some("turn-1"), false);
        assert_eq!(report.action, Action::Allow);
        assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
        assert!(report.errors[0]
            .to_string()
            .contains("synthetic ingest failure"));
        assert!(f.spool().path().join(format!("{id}.json")).exists());
        let rows = decisions(&f);
        if case == "other-source" {
            assert_eq!(activity_counts(&f)[2], 0);
        } else {
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].1,
                if case == "pending" {
                    "pending_unprocessed"
                } else {
                    "confirmed"
                }
            );
        }
        store
            .connection()
            .execute_batch("DROP TRIGGER fail_ingest")
            .unwrap();
        let recovered = turn::turn_ended(context, Some("turn-1"), false);
        assert!(recovered.errors.is_empty());
        if case == "other-source" {
            assert!(
                matches!(recovered.action, Action::Continue { .. }),
                "error must not consume continuation slot"
            );
        } else {
            assert_eq!(recovered.action, Action::Allow);
            assert!(decisions(&f).iter().any(|r| r.1 == "confirmed"));
        }
        assert_eq!(f.spool().status().unwrap().pending_json, 0);
    }
}

#[test]
fn spool_open_failure_preserves_confirmed_continued_decision_and_error() {
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

    let result = turn::turn_ended(f.context(END), Some("turn-1"), true);

    assert_eq!(result.action, Action::Allow);
    assert!(matches!(
        result.errors.as_slice(),
        [turn::Error::Spool(cairn::spool::Error::UnsafeDirectory)]
    ));
    assert_eq!(
        decisions(&f),
        [("turn-1".into(), "confirmed".into(), END.into())]
    );
    assert_eq!(activity_counts(&f), [1, 1, 1, 1, 0, 0]);
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

use super::*;
use crate::store::BusyTimeout;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

static ENVIRONMENT: Mutex<()> = Mutex::new(());

struct Fixture {
    root: tempfile::TempDir,
    scope: Scope,
    store: Store,
    saved: Vec<(&'static str, Option<OsString>)>,
    _guard: MutexGuard<'static, ()>,
}

impl Fixture {
    fn new(text: &str, script: &str) -> Self {
        let guard = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
        let root = tempfile::tempdir().unwrap();
        let mut saved = Vec::new();
        for key in [
            "PATH",
            "HOME",
            "XDG_STATE_HOME",
            "XDG_DATA_HOME",
            "XDG_CONFIG_HOME",
            "CODEX_HOME",
            "CLAUDE_CONFIG_DIR",
            "SIDECAR_TEST_DIR",
        ] {
            saved.push((key, std::env::var_os(key)));
            std::env::set_var(key, root.path());
        }
        std::env::set_var("PATH", format!("{}:/usr/bin:/bin", root.path().display()));
        // Existing tests configure summary behavior. Ledger calls are separate
        // and return {"changes":[]} without running summary-specific barriers.
        let reads_input = script.contains("cat >/dev/null") || script.contains("input=$(cat)");
        let routing = if reads_input {
            r#"
input=$(cat)
case "$input" in *'你在为一个软件项目维护"项目的账"'*)
    while [ "$1" != -o ]; do shift; done
    printf '{"changes":[]}\n' > "$2"
    exit 0 ;;
esac
"#
        } else {
            ""
        };
        let script = script
            .replace("cat >/dev/null", ":")
            .replace("input=$(cat)", ":");
        let program = root.path().join("codex");

        fs::write(&program,format!("#!/bin/sh\nset -eu\n{routing}\nprintf '%s' \"$$\" > \"$SIDECAR_TEST_DIR/pid\"\nprintf '%s' \"$PWD\" > \"$SIDECAR_TEST_DIR/cwd\"\n{script}\n")).unwrap();
        fs::set_permissions(program, fs::Permissions::from_mode(0o700)).unwrap();
        let scope = Scope {
            project_key: root.path().join("project"),
            line_path: root.path().join("project"),
            is_git: false,
        };
        let mut store =
            Store::open(root.path().join("state/cairn.db"), BusyTimeout::UserCommand).unwrap();
        crate::adopt::set_adopted(&mut store, &scope, true).unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO sources VALUES ('claude:test','claude','test','hook',?1,?1)",
                [crate::save::now()],
            )
            .unwrap();
        let transcript = root.path().join("transcript.jsonl");
        fs::write(
            &transcript,
            format!(
                "{}\n",
                serde_json::json!({"type":"user","message":{"content":text}})
            ),
        )
        .unwrap();
        store.connection().execute("INSERT INTO sessions(source_id,project_id,line_path,transcript_path,turns,last_event_at) VALUES ('claude:test',1,?1,?2,1,?3)",
            params![scope.line_path.to_str(),transcript.to_str(),crate::save::now()]).unwrap();
        if !reads_input {
            // The pipe-backpressure test intentionally never reads stdin; its
            // ledger is already caught up, so it still exercises one timeout.
            store
                .connection()
                .execute(
                    "UPDATE sessions SET ledger_bytes=?1",
                    [fs::metadata(&transcript).unwrap().len() as i64],
                )
                .unwrap();
        }
        Self {
            root,
            scope,
            store,
            saved,
            _guard: guard,
        }
    }

    fn run(&mut self, timeout: Duration) -> Result<String> {
        summarize_with_timing(
            &mut self.store,
            &self.scope,
            self.root.path(),
            Timing {
                timeout,
                heartbeat: Duration::from_millis(20),
                ..Timing::default()
            },
        )
    }

    fn wait(&mut self, timing: Timing) {
        let db = self.root.path().join("state/cairn.db");
        let done = std::sync::atomic::AtomicBool::new(false);
        std::thread::scope(|threads| {
            threads.spawn(|| {
                let deadline = Instant::now() + Duration::from_secs(3);
                while !done.load(std::sync::atomic::Ordering::SeqCst) {
                    if Instant::now() >= deadline {
                        Store::open(&db, BusyTimeout::UserCommand)
                            .unwrap()
                            .connection()
                            .execute("UPDATE sessions SET waiter_id='test-deadline'", [])
                            .unwrap();
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            });
            let result = wait_with_timing(&mut self.store, "claude:test", self.root.path(), timing);
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            result.unwrap();
        });
    }

    fn summary_count(&self) -> usize {
        crate::render::records(
            self.store.connection(),
            self.scope.project_key.to_str().unwrap(),
        )
        .unwrap()
        .len()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }
}

#[test]
fn timeout_kills_own_child_records_failure_and_heartbeats_while_waiting() {
    let mut f = Fixture::new("synthetic", "cat >/dev/null\nexec /bin/sleep 3");
    let before = crate::save::now();
    let observer_db = f.root.path().join("state/cairn.db");
    let observer = std::thread::spawn(move || {
        let store = Store::open(observer_db, BusyTimeout::UserCommand).unwrap();
        let deadline = Instant::now() + Duration::from_millis(900);
        let mut beats = std::collections::HashSet::<String>::new();
        while Instant::now() < deadline {
            let beat: Option<String> = store
                .connection()
                .query_row("SELECT waiter_beat_at FROM sessions", [], |r| r.get(0))
                .unwrap();
            if let Some(beat) = beat {
                beats.insert(beat);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        beats
    });
    let start = Instant::now();
    let result = f.run(Duration::from_secs(1));
    let elapsed = start.elapsed();
    let beats = observer.join().unwrap();
    assert!(
        elapsed < Duration::from_secs(2),
        "deadline ignored: {elapsed:?}"
    );
    let error = result.unwrap_err();
    assert!(error
        .downcast_ref::<SummaryFailures>()
        .unwrap()
        .output
        .contains("超时"));
    assert!(
        beats.len() > 1 && beats.iter().any(|beat| beat > &before),
        "no heartbeats: {beats:?}"
    );
    let pid: i32 = fs::read_to_string(f.root.path().join("pid"))
        .unwrap()
        .parse()
        .unwrap();
    // SAFETY: signal 0 only checks whether our recorded child still exists.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    let (failures, waiter): (i64, Option<String>) = f
        .store
        .connection()
        .query_row("SELECT failures,waiter_id FROM sessions", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(failures, 1);
    assert!(waiter.is_none());
    assert!(!PathBuf::from(fs::read_to_string(f.root.path().join("cwd")).unwrap()).exists());
}

#[test]
fn timeout_also_applies_when_child_does_not_read_stdin() {
    let mut f = Fixture::new(&"synthetic".repeat(9000), "exec /bin/sleep 3");
    let start = Instant::now();
    let error = f.run(Duration::from_secs(1)).unwrap_err();
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(f.root.path().join("pid").exists());
    assert!(error
        .downcast_ref::<SummaryFailures>()
        .unwrap()
        .output
        .contains("超时"));
}

const WRITE_OUTPUT: &str = r#"
output=
while [ "$#" -gt 0 ]; do
    if [ "$1" = -o ]; then output=$2; shift 2; else shift; fi
done
printf '## 停点\nnew-summary\n' > "$output"
"#;

#[test]
fn waiter_writes_when_the_session_has_ended() {
    let mut f = Fixture::new("synthetic", &format!("cat >/dev/null\n{WRITE_OUTPUT}"));
    f.store
        .connection()
        .execute(
            "INSERT INTO events(source_id,kind,at) VALUES ('claude:test','session_ended',?1)",
            [crate::save::now()],
        )
        .unwrap();
    wait_with_timing(
        &mut f.store,
        "claude:test",
        f.root.path(),
        Timing::default(),
    )
    .unwrap();
    let records =
        crate::render::records(f.store.connection(), f.scope.project_key.to_str().unwrap())
            .unwrap();
    assert_eq!(
        records.len(),
        1,
        "ended session must receive a background summary"
    );
    assert_eq!(records[0].body.as_deref(), Some("## 停点\nnew-summary\n"));
}

#[test]
fn resumed_session_and_new_turns_ignore_old_ends_until_a_new_end_arrives() {
    for restart in ["session_started", "turn_started"] {
        let mut f = Fixture::new("before ending", &format!("cat >/dev/null\n{WRITE_OUTPUT}"));
        let at = crate::save::now();
        f.store
            .connection()
            .execute(
                "INSERT INTO events(source_id,kind,at) VALUES ('claude:test','session_ended',?1)",
                [&at],
            )
            .unwrap();
        let timing = Timing {
            quiet: Duration::from_secs(180),
            ..fast_timing()
        };
        f.wait(timing);
        assert_eq!(f.summary_count(), 1);
        fs::remove_file(f.root.path().join("pid")).unwrap();

        // A restart in the same millisecond must invalidate the preceding end.
        f.store
            .connection()
            .execute(
                "INSERT INTO events(source_id,kind,at) VALUES ('claude:test',?1,?2)",
                params![restart, at],
            )
            .unwrap();
        let transcript = f.root.path().join("transcript.jsonl");
        let append = || {
            fs::OpenOptions::new()
                .append(true)
                .open(&transcript)
                .unwrap()
                .write_all(b"{\"type\":\"user\",\"message\":{\"content\":\"resumed work\"}}\n")
                .unwrap()
        };
        append();
        // A callback received earlier can commit later; its timestamp still wins
        // over insertion order. Neither old end may trigger the resumed work.
        f.store.connection().execute(
            "INSERT INTO events(source_id,kind,at) VALUES ('claude:test','session_ended','2000-01-01T00:00:00.000Z')", [],
        ).unwrap();
        let db = f.root.path().join("state/cairn.db");
        let marker = f.root.path().join("pid");
        let events = std::thread::spawn(move || {
            let store = Store::open(db, BusyTimeout::UserCommand).unwrap();
            for turn in 0..3 {
                if turn > 0 {
                    store.connection().execute(
                        "INSERT INTO events(source_id,kind,at) VALUES ('claude:test','turn_started',?1)", [&at],
                    ).unwrap();
                    fs::OpenOptions::new()
                        .append(true)
                        .open(&transcript)
                        .unwrap()
                        .write_all(
                            b"{\"type\":\"user\",\"message\":{\"content\":\"another turn\"}}\n",
                        )
                        .unwrap();
                }
                let mut beats = std::collections::HashSet::<String>::new();
                let deadline = Instant::now() + Duration::from_secs(2);
                while beats.len() < 3 {
                    assert!(
                        !marker.exists(),
                        "{restart}, turn {turn}: an old end triggered the model"
                    );
                    assert!(
                        Instant::now() < deadline,
                        "waiter stopped checking the resumed session"
                    );
                    let beat: Option<String> = store
                        .connection()
                        .query_row("SELECT waiter_beat_at FROM sessions", [], |r| r.get(0))
                        .unwrap();
                    if let Some(beat) = beat {
                        beats.insert(beat);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            // Same timestamp again: the new end follows the latest turn by ID.
            store.connection().execute(
                "INSERT INTO events(source_id,kind,at) VALUES ('claude:test','session_ended',?1)", [&at],
            ).unwrap();
        });
        f.wait(timing);
        events.join().unwrap();
        assert_eq!(
            f.summary_count(),
            2,
            "the new end must write without waiting three minutes"
        );
        let ends: i64 = f
            .store
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM events WHERE kind='session_ended'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ends, 3, "historical end events must remain intact");
        let pid: i32 = fs::read_to_string(f.root.path().join("pid"))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    }
}

fn fast_timing() -> Timing {
    Timing {
        timeout: Duration::from_secs(2),
        heartbeat: Duration::from_millis(10),
        quiet: Duration::from_millis(80),
    }
}

#[test]
fn waiter_writes_when_another_session_started_after_its_latest_event() {
    let mut f = Fixture::new("synthetic", &format!("cat >/dev/null\n{WRITE_OUTPUT}"));
    let at = (Utc::now() + chrono::Duration::seconds(1))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    f.store
        .connection()
        .execute(
            "INSERT INTO sources VALUES ('codex:next','codex','next','hook',?1,?1)",
            [&at],
        )
        .unwrap();
    f.store.connection().execute("INSERT INTO sessions(source_id,project_id,line_path,last_event_at) VALUES ('codex:next',1,?1,?2)", params![f.scope.line_path.to_str(),at]).unwrap();
    f.store
        .connection()
        .execute(
            "INSERT INTO events(source_id,kind,at) VALUES ('codex:next','session_started',?1)",
            [&at],
        )
        .unwrap();
    f.wait(Timing {
        quiet: Duration::from_secs(180),
        ..fast_timing()
    });
    assert_eq!(
        f.summary_count(),
        1,
        "a later start must trigger the old session"
    );
}

#[test]
fn waiter_writes_after_quiet_time_but_not_while_the_transcript_keeps_changing() {
    let mut f = Fixture::new("synthetic", &format!("cat >/dev/null\n{WRITE_OUTPUT}"));
    f.store
        .connection()
        .execute(
            "UPDATE sessions SET last_event_at='2020-01-01T00:00:00.000Z'",
            [],
        )
        .unwrap();
    let transcript = f.root.path().join("transcript.jsonl");
    let marker = f.root.path().join("pid");
    let changes = std::thread::spawn(move || {
        for _ in 0..20 {
            fs::OpenOptions::new()
                .append(true)
                .open(&transcript)
                .unwrap()
                .write_all(b"{\"type\":\"user\",\"message\":{\"content\":\"still working\"}}\n")
                .unwrap();
            std::thread::sleep(Duration::from_millis(10));
            assert!(!marker.exists(), "must not summarize a changing transcript");
        }
    });
    f.wait(fast_timing());
    changes.join().unwrap();
    assert_eq!(
        f.summary_count(),
        1,
        "the now-quiet session must get a summary"
    );
}

#[test]
fn waiter_keeps_waiting_after_success_or_failure_if_new_events_arrived() {
    for fail_first in [false, true] {
        let mut f = Fixture::new(
            "before",
            &format!(
                r#"
cat >/dev/null
if [ ! -e "$SIDECAR_TEST_DIR/first" ]; then
    touch "$SIDECAR_TEST_DIR/first"
    while [ ! -e "$SIDECAR_TEST_DIR/continue" ]; do /bin/sleep 0.01; done
    {}
else
    touch "$SIDECAR_TEST_DIR/second"
fi
{WRITE_OUTPUT}
"#,
                if fail_first { "exit 1" } else { ":" }
            ),
        );
        let original_event: String = f
            .store
            .connection()
            .query_row("SELECT last_event_at FROM sessions", [], |r| r.get(0))
            .unwrap();
        let db = f.root.path().join("state/cairn.db");
        let root = f.root.path().to_path_buf();
        let change = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !root.join("first").exists() {
                assert!(Instant::now() < deadline, "first summary never started");
                std::thread::sleep(Duration::from_millis(5));
            }
            fs::OpenOptions::new()
                .append(true)
                .open(root.join("transcript.jsonl"))
                .unwrap()
                .write_all(b"{\"type\":\"user\",\"message\":{\"content\":\"new turn\"}}\n")
                .unwrap();
            let mut store = Store::open(&db, BusyTimeout::UserCommand).unwrap();
            let tx = store
                .transaction(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            // Equal timestamps still represent a new event: event IDs break the tie.
            tx.execute(
                "INSERT INTO events(source_id,kind,at) VALUES ('claude:test','turn_started',?1)",
                [&original_event],
            )
            .unwrap();
            tx.execute("UPDATE sessions SET turns=turns+1", []).unwrap();
            tx.commit().unwrap();
            fs::write(root.join("continue"), "").unwrap();
        });
        f.wait(fast_timing());
        change.join().unwrap();
        assert!(
            f.root.path().join("second").exists(),
            "new events must retain the waiter after success or failure"
        );
        let (turns, failures, waiter): (i64, i64, Option<String>) = f
            .store
            .connection()
            .query_row(
                "SELECT summarized_turns,failures,waiter_id FROM sessions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((turns, failures, waiter), (2, 0, None));
    }
}

#[test]
fn waiter_losing_its_lease_does_not_write_before_or_during_generation() {
    for during_generation in [false, true] {
        let mut f = Fixture::new("synthetic", "cat >/dev/null\nexec /bin/sleep 3");
        let db = f.root.path().join("state/cairn.db");
        let marker = f.root.path().join("pid");
        let takeover = std::thread::spawn(move || {
            let store = Store::open(db, BusyTimeout::UserCommand).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let acquired: bool = store
                    .connection()
                    .query_row("SELECT waiter_id IS NOT NULL FROM sessions", [], |r| {
                        r.get(0)
                    })
                    .unwrap();
                if acquired && (!during_generation || marker.exists()) {
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            store
                .connection()
                .execute(
                    "UPDATE sessions SET waiter_id='replacement',waiter_beat_at=?1",
                    [crate::save::now()],
                )
                .unwrap();
        });
        f.wait(Timing {
            quiet: if during_generation {
                Duration::ZERO
            } else {
                Duration::from_secs(180)
            },
            ..fast_timing()
        });
        takeover.join().unwrap();
        assert_eq!(f.summary_count(), 0);
        let (owner, failures): (String, i64) = f
            .store
            .connection()
            .query_row("SELECT waiter_id,failures FROM sessions", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((owner.as_str(), failures), ("replacement", 0));
        if during_generation {
            let pid: i32 = fs::read_to_string(f.root.path().join("pid"))
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        } else {
            assert!(!f.root.path().join("pid").exists());
        }
    }
}

#[test]
fn waiter_releases_ineligible_sessions_and_does_not_retry_without_new_events() {
    let mut f = Fixture::new("synthetic", "cat >/dev/null\nexit 1");
    for expected in [1, 2, 2] {
        f.wait(Timing {
            quiet: Duration::ZERO,
            ..fast_timing()
        });
        let (failures, waiter): (i64, Option<String>) = f
            .store
            .connection()
            .query_row("SELECT failures,waiter_id FROM sessions", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((failures, waiter), (expected, None));
    }
    assert_eq!(f.summary_count(), 0);
    fs::remove_file(f.root.path().join("pid")).unwrap();
    for update in [
        "UPDATE sessions SET failures=0,turns=0",
        "UPDATE sessions SET turns=1,transcript_path=NULL",
        "UPDATE sessions SET transcript_path='missing',summary_record_id=NULL; UPDATE projects SET adopted=0",
    ] {
        f.store.connection().execute_batch(update).unwrap();
        f.wait(fast_timing());
        assert!(!f.root.path().join("pid").exists());
    }
}

#[test]
fn a_rewritten_file_restarts_from_full_text_without_the_previous_summary() {
    let script = format!(
        r#"
input=$(cat)
case "$input" in *'user: rewritten-full-text'*) ;; *) exit 81 ;; esac
case "$input" in *'stale-summary'*) exit 82 ;; esac
{WRITE_OUTPUT}
"#
    );
    let mut f = Fixture::new("rewritten-full-text", &script);
    f.store.connection().execute("INSERT INTO records(id,project_id,line_path,source_id,kind,body,created_at) VALUES ('old',1,?1,'claude:test','checkpoint','## 停点
stale-summary',?2)",params![f.scope.line_path.to_str(),crate::save::now()]).unwrap();
    f.store
        .connection()
        .execute(
            "UPDATE sessions SET summarized_bytes=10000,summary_record_id='old'",
            [],
        )
        .unwrap();
    let lease = Lease::acquire(&mut f.store, "claude:test", Utc::now())
        .unwrap()
        .unwrap();
    let session = sessions(f.store.connection(), Some(&f.scope), None)
        .unwrap()
        .pop()
        .unwrap();
    // Exercise the already-selected one-session operation; eligibility itself
    // follows DESIGN's size-growth rule, while extract handles a subsequent shrink.
    write_summary(
        &mut f.store,
        &session,
        f.root.path(),
        &lease,
        Timing::default(),
    )
    .unwrap();
    let records =
        crate::render::records(f.store.connection(), f.scope.project_key.to_str().unwrap())
            .unwrap();
    assert_eq!(records.iter().filter(|r| r.visible()).count(), 1);
    assert_eq!(records[0].body.as_deref(), Some("## 停点\nnew-summary\n"));
}

#[test]
fn losing_the_lease_stops_the_child_without_overwriting_the_new_owner() {
    let mut f = Fixture::new("synthetic", "cat >/dev/null\nexec /bin/sleep 3");
    let db = f.root.path().join("state/cairn.db");
    let marker = f.root.path().join("pid");
    let takeover = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !marker.exists() {
            assert!(Instant::now() < deadline, "fake codex never started");
            std::thread::sleep(Duration::from_millis(5));
        }
        let store = Store::open(db, BusyTimeout::UserCommand).unwrap();
        store
            .connection()
            .execute(
                "UPDATE sessions SET waiter_id='new-owner',waiter_beat_at=?1",
                [crate::save::now()],
            )
            .unwrap();
    });
    let error = f.run(Duration::from_secs(2)).unwrap_err();
    takeover.join().unwrap();
    assert!(error
        .downcast_ref::<SummaryFailures>()
        .unwrap()
        .output
        .contains("占位已失效"));
    let (owner, failures): (String, i64) = f
        .store
        .connection()
        .query_row("SELECT waiter_id,failures FROM sessions", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(owner, "new-owner");
    assert_eq!(failures, 0);
    assert!(
        crate::render::records(f.store.connection(), f.scope.project_key.to_str().unwrap())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn events_and_transcript_growth_during_generation_remain_for_the_next_summary() {
    let mut f = Fixture::new(
        "before",
        &format!("cat >/dev/null\n/bin/sleep 1\n{WRITE_OUTPUT}"),
    );
    let original_bytes = fs::metadata(f.root.path().join("transcript.jsonl"))
        .unwrap()
        .len();
    let db = f.root.path().join("state/cairn.db");
    let marker = f.root.path().join("pid");
    let transcript = f.root.path().join("transcript.jsonl");
    let change = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !marker.exists() {
            assert!(Instant::now() < deadline, "fake codex never started");
            std::thread::sleep(Duration::from_millis(5));
        }
        fs::OpenOptions::new()
            .append(true)
            .open(transcript)
            .unwrap()
            .write_all(b"{\"type\":\"user\",\"message\":{\"content\":\"after\"}}\n")
            .unwrap();
        let store = Store::open(db, BusyTimeout::UserCommand).unwrap();
        store
            .connection()
            .execute(
                "UPDATE sessions SET turns=2,last_event_at=?1",
                [crate::save::now()],
            )
            .unwrap();
    });
    f.run(Duration::from_secs(3)).unwrap();
    change.join().unwrap();
    let (bytes, summarized, turns): (i64, i64, i64) = f
        .store
        .connection()
        .query_row(
            "SELECT summarized_bytes,summarized_turns,turns FROM sessions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(bytes as u64, original_bytes);
    assert_eq!((summarized, turns), (1, 2));
}

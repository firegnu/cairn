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
        let program = root.path().join("codex");
        fs::write(&program,format!("#!/bin/sh\nset -eu\nprintf '%s' \"$$\" > \"$SIDECAR_TEST_DIR/pid\"\nprintf '%s' \"$PWD\" > \"$SIDECAR_TEST_DIR/cwd\"\n{script}\n")).unwrap();
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
            },
        )
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
    let session = sessions(f.store.connection(), &f.scope, None)
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

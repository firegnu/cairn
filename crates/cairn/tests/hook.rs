use cairn::cli::{self, Cli};
use clap::Parser;
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static ENVIRONMENT: Mutex<()> = Mutex::new(());
const CASES: &[(&str, &str)] = &[
    ("claude", "session_start"),
    ("claude", "user_prompt_submit"),
    ("claude", "stop"),
    ("claude", "stop_continued"),
    ("claude", "session_end"),
    ("codex", "session_start"),
    ("codex", "user_prompt_submit"),
    ("codex", "stop"),
    ("codex", "stop_continued"),
    ("codex", "session_end"),
    ("codex", "interrupt"),
];

struct Fixture {
    root: tempfile::TempDir,
    saved: Vec<(OsString, OsString)>,
    _guard: MutexGuard<'static, ()>,
}

fn isolated(key: &std::ffi::OsStr) -> bool {
    key.to_string_lossy().starts_with("GIT_")
        || [
            "HOME",
            "XDG_STATE_HOME",
            "XDG_DATA_HOME",
            "XDG_CONFIG_HOME",
            "TMPDIR",
            "CAIRN_DISABLE",
            "PATH",
        ]
        .contains(&key.to_str().unwrap_or(""))
}

impl Fixture {
    fn new() -> Self {
        let guard = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
        let root = tempfile::tempdir().unwrap();
        let saved: Vec<_> = std::env::vars_os()
            .filter(|(key, _)| isolated(key))
            .collect();
        for (key, _) in &saved {
            if key != "PATH" {
                std::env::remove_var(key);
            }
        }
        for key in ["HOME", "XDG_STATE_HOME", "XDG_CONFIG_HOME", "TMPDIR"] {
            std::env::set_var(key, root.path());
        }
        std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        fs::create_dir(root.path().join("project")).unwrap();
        Self {
            root,
            saved,
            _guard: guard,
        }
    }
    fn cwd(&self) -> PathBuf {
        self.root.path().join("project")
    }
    fn db(&self) -> PathBuf {
        self.root.path().join("cairn/cairn.db")
    }
    fn run(&self, args: &[&str], input: &str) -> String {
        let cli = Cli::try_parse_from(std::iter::once("cairn").chain(args.iter().copied()));
        assert!(cli.is_ok(), "command must be accepted: {cli:?}");
        let cwd = if args.first() == Some(&"hook") {
            self.root.path().join("not-the-json-cwd")
        } else {
            self.cwd()
        };
        cli::run_at(
            cli.unwrap(),
            &mut input.as_bytes(),
            &cwd,
            &self.db(),
            self.root.path(),
        )
        .unwrap()
    }
    fn payload(&self, agent: &str, case: &str) -> Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/hooks/{agent}_{case}.json"));
        let mut value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        value["cwd"] = self.cwd().to_str().unwrap().into();
        value["transcript_path"] = self
            .root
            .path()
            .join("never-open-transcript")
            .to_str()
            .unwrap()
            .into();
        value
    }
    fn hook(&self, agent: &str, case: &str) -> String {
        self.run(&["hook", agent], &self.payload(agent, case).to_string())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for (key, _) in std::env::vars_os().filter(|(key, _)| isolated(key)) {
            std::env::remove_var(key);
        }
        for (key, value) in &self.saved {
            std::env::set_var(key, value);
        }
    }
}
fn allow(agent: &str, case: &str) -> &'static str {
    if agent == "codex" && case.starts_with("stop") {
        "{}"
    } else {
        ""
    }
}

#[test]
fn unadopted_events_are_silent_and_do_not_create_storage() {
    let f = Fixture::new();
    for &(agent, case) in CASES {
        assert_eq!(f.hook(agent, case), allow(agent, case), "{agent} {case}");
    }
    assert!(!f.db().parent().unwrap().exists());
    assert!(!f.root.path().join("cairn-spool").exists());
    // Existing but explicitly unadopted projects must not collect or record either.
    f.run(&["unadopt"], "");
    f.run(&["save", "--nothing-new"], "");
    let spool = cairn::spool::Spool::open(f.root.path(), &f.db()).unwrap();
    let pending = fs::read_dir(spool.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let bytes = fs::read(&pending).unwrap();
    for &(agent, case) in CASES {
        assert_eq!(f.hook(agent, case), allow(agent, case));
    }
    assert_eq!(fs::read(pending).unwrap(), bytes);
    let store = cairn::store::Store::open(f.db(), cairn::store::BusyTimeout::UserCommand).unwrap();
    for table in [
        "events",
        "sources",
        "records",
        "spool_ops",
        "turn_decisions",
    ] {
        let count: i64 = store
            .connection()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
}

#[test]
fn adopted_fixtures_dispatch_and_stop_formats_match_each_agent() {
    let f = Fixture::new();
    f.init_git();
    f.run(&["adopt"], "");
    for agent in ["claude", "codex"] {
        let output: Value = serde_json::from_str(&f.hook(agent, "session_start"))
            .expect("SessionStart must inject JSON");
        assert_eq!(
            output["hookSpecificOutput"]["hookEventName"],
            "SessionStart"
        );
        assert!(output["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains(&format!("{agent}:synthetic-{agent}-session")));
        assert_eq!(f.hook(agent, "user_prompt_submit"), "");
        let output: Value = serde_json::from_str(&f.hook(agent, "stop")).unwrap();
        assert_eq!(output["decision"], "block");
        assert!(output["reason"]
            .as_str()
            .unwrap()
            .contains("cairn save --source"));
        assert_eq!(f.hook(agent, "stop"), allow(agent, "stop"));
        assert_eq!(f.hook(agent, "stop_continued"), allow(agent, "stop"));
        assert_eq!(f.hook(agent, "session_end"), "");
    }
    assert_eq!(f.hook("codex", "interrupt"), "");
    let store = cairn::store::Store::open(f.db(), cairn::store::BusyTimeout::UserCommand).unwrap();
    for agent in ["claude", "codex"] {
        let source = format!("{agent}:synthetic-{agent}-session");
        let mut query = store
            .connection()
            .prepare("SELECT kind,detail FROM events WHERE source_id=?1 ORDER BY id")
            .unwrap();
        let events: Vec<(String, String)> = query
            .query_map([source], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            events,
            vec![
                ("session_started".into(), "startup".into()),
                ("turn_started".into(), format!("synthetic-{agent}-turn")),
                (
                    "session_ended".into(),
                    if agent == "claude" {
                        "prompt_input_exit"
                    } else {
                        "other"
                    }
                    .into()
                ),
            ]
        );
        let key: String = store.connection().query_row("SELECT turn_key FROM turn_decisions WHERE source_id=?1 AND outcome='unconfirmed_after_continue'", [format!("{agent}:synthetic-{agent}-session")], |r| r.get(0)).unwrap();
        assert_eq!(key, format!("synthetic-{agent}-turn"));
    }
}

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

fn check_stop_requires_continuation_marker(agent: &str) {
    let f = Fixture::new();
    f.run(&["adopt"], "");
    assert_eq!(f.hook(agent, "user_prompt_submit"), "");
    for (index, marker) in [None, Some(Value::from("PRIVATE-INVALID-MARKER-R1"))]
        .into_iter()
        .enumerate()
    {
        let mut stop = f.payload(agent, "stop");
        stop["prompt"] = "PRIVATE-PROMPT-R1".into();
        stop["last_assistant_message"] = "PRIVATE-ANSWER-R1".into();
        match marker {
            None => {
                stop.as_object_mut().unwrap().remove("stop_hook_active");
            }
            Some(value) => stop["stop_hook_active"] = value,
        }
        assert_eq!(
            f.run(&["hook", agent], &stop.to_string()),
            allow(agent, "stop")
        );
        let store =
            cairn::store::Store::open(f.db(), cairn::store::BusyTimeout::UserCommand).unwrap();
        let decisions: i64 = store
            .connection()
            .query_row("SELECT count(*) FROM turn_decisions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(decisions, 0);
        let log = f.log();
        assert_eq!(log.lines().count(), index + 1);
        assert!(log.lines().all(
            |line| line.ends_with(&format!("{agent} Stop json invalid hook JSON or metadata"))
        ));
        assert!(!log.contains("PRIVATE-"));
    }
}

#[test]
fn stop_requires_continuation_marker_claude() {
    check_stop_requires_continuation_marker("claude");
}

#[test]
fn stop_requires_continuation_marker_codex() {
    check_stop_requires_continuation_marker("codex");
}

impl Fixture {
    fn init_git(&self) {
        use std::time::{Duration, Instant};
        let mut child = Command::new("git")
            .args(["-c", "core.hooksPath=/dev/null", "init", "--quiet"])
            .current_dir(self.cwd())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if start.elapsed() > Duration::from_secs(5) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("fixture git init timed out");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn process(&self, agent: &str, input: &str) -> std::process::Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cairn"))
            .args(["hook", agent])
            .current_dir(self.cwd())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
    fn log(&self) -> String {
        fs::read_to_string(self.db().with_file_name("errors.log")).unwrap()
    }
}

#[test]
fn disabled_entry_skips_invalid_database_and_pending_spool_for_all_events() {
    let f = Fixture::new();
    f.run(&["adopt"], "");
    f.run(&["save", "--nothing-new"], "");
    let spool = cairn::spool::Spool::open(f.root.path(), &f.db()).unwrap();
    let pending: Vec<_> = fs::read_dir(spool.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(pending.len(), 1);
    let bytes = fs::read(&pending[0]).unwrap();
    fs::write(f.db(), b"invalid database: must not be opened").unwrap();
    std::env::set_var("CAIRN_DISABLE", "1");
    for &(agent, case) in CASES {
        let output = f.process(agent, &f.payload(agent, case).to_string());
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, allow(agent, case).as_bytes());
        assert!(output.stderr.is_empty());
    }
    assert_eq!(fs::read(&pending[0]).unwrap(), bytes);
    assert!(!f.db().with_file_name("errors.log").exists());
    std::env::set_var("CAIRN_DISABLE", "1");
    assert_eq!(
        f.run(&["hook", "codex"], r#"{"hook_event_name":"unknown"}"#),
        ""
    );
}

#[test]
fn failures_exit_zero_with_private_bounded_single_line_logs() {
    let f = Fixture::new();
    for agent in ["claude", "codex"] {
        let output = f.process(agent, "{\"prompt\":\"PRIVATE-PROMPT-MARKER-4Q7\", broken");
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout,
            if agent == "codex" {
                b"{}".as_slice()
            } else {
                b""
            }
        );
    }
    let log = f.log();
    assert_eq!(log.lines().count(), 2);
    assert!(log.contains("claude unknown json"));
    assert!(log.contains("codex unknown json"));
    assert!(!log.contains("PRIVATE-PROMPT"));
    let path = f.db().with_file_name("errors.log");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::write(
        &path,
        (0..205).map(|n| format!("old-{n}\n")).collect::<String>(),
    )
    .unwrap();
    let mut bad = f.payload("codex", "stop");
    bad["stop_hook_active"] = "PRIVATE-ANSWER-MARKER-8R2".into();
    assert_eq!(f.run(&["hook", "codex"], &bad.to_string()), "{}");
    let log = f.log();
    assert_eq!(log.lines().count(), 200);
    assert!(log.starts_with("old-6\n"));
    assert!(log.lines().last().unwrap().contains("codex Stop json"));
    assert!(!log.contains("PRIVATE-ANSWER"));
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert_eq!(f.process("codex", "broken").status.code(), Some(0));
}

#[test]
fn privacy_fields_never_reach_storage_output_or_error_logs() {
    let f = Fixture::new();
    f.run(&["adopt"], "");
    let markers = [
        "PRIVATE-PROMPT-MARKER-4Q7",
        "PRIVATE-ANSWER-MARKER-8R2",
        "PRIVATE-TRANSCRIPT-MARKER-6S9",
    ];
    // A FIFO would block if the adapter opened transcript_path for reading.
    let transcript = f.root.path().join(markers[2]);
    let path = std::ffi::CString::new(transcript.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    for &(agent, case) in CASES {
        let mut payload = f.payload(agent, case);
        payload["prompt"] = markers[0].into();
        payload["last_assistant_message"] = markers[1].into();
        payload["transcript_path"] = transcript.to_str().unwrap().into();
        let output = f.run(&["hook", agent], &payload.to_string());
        for marker in markers {
            assert!(!output.contains(marker));
        }
        payload["stop_hook_active"] = markers[1].into();
        let output = f.run(&["hook", agent], &payload.to_string());
        for marker in markers {
            assert!(!output.contains(marker));
        }
    }
    fn check_tree(path: &std::path::Path, markers: &[&str]) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                check_tree(&path, markers);
            } else {
                let bytes = fs::read(path).unwrap();
                for marker in markers {
                    assert!(!bytes.windows(marker.len()).any(|w| w == marker.as_bytes()));
                }
            }
        }
    }
    check_tree(f.db().parent().unwrap(), &markers);
    check_tree(&f.root.path().join("cairn-spool"), &markers);
}

#[test]
fn database_busy_core_failure_and_session_end_deadline_fail_open() {
    use cairn::store::{BusyTimeout, Store};
    use std::time::{Duration, Instant};
    let f = Fixture::new();
    f.run(&["adopt"], "");
    f.hook("codex", "session_start");
    f.run(
        &[
            "save",
            "--source",
            "codex:synthetic-codex-session",
            "--nothing-new",
        ],
        "",
    );
    let spool = cairn::spool::Spool::open(f.root.path(), &f.db()).unwrap();
    let pending: Vec<_> = fs::read_dir(spool.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let store = Store::open(f.db(), BusyTimeout::UserCommand).unwrap();
    store.connection().execute_batch("BEGIN IMMEDIATE").unwrap();
    let start = Instant::now();
    let output = f.process("claude", &f.payload("claude", "session_end").to_string());
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(start.elapsed() < Duration::from_millis(1500));
    assert!(f.log().contains("claude SessionEnd database"));
    store.connection().execute_batch("ROLLBACK").unwrap();
    drop(store);
    // SessionEnd does not even need a usable spool root, and leaves pending files.
    let payload = f.payload("codex", "session_end").to_string();
    let start = Instant::now();
    let result = cli::run_at(
        Cli::try_parse_from(["cairn", "hook", "codex"]).unwrap(),
        &mut payload.as_bytes(),
        &f.cwd(),
        &f.db(),
        &f.root.path().join("missing-root"),
    )
    .unwrap();
    assert_eq!(result, "");
    assert!(start.elapsed() < Duration::from_millis(1500));
    assert!(pending.iter().all(|path| path.exists()));
    let mut stop = f.payload("codex", "stop");
    stop["session_id"] = "unobserved-source".into();
    assert_eq!(f.run(&["hook", "codex"], &stop.to_string()), "{}");
    assert!(f.log().contains("codex Stop core"));
    let bin = f.root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let git = bin.join("git");
    fs::write(&git, "#!/bin/sh\nexec /bin/sleep 2\n").unwrap();
    fs::set_permissions(&git, fs::Permissions::from_mode(0o700)).unwrap();
    std::env::set_var("PATH", bin);
    let start = Instant::now();
    let output = f.process("codex", &payload);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(start.elapsed() < Duration::from_millis(1500));
    assert!(f.log().contains("codex SessionEnd git_timeout"));
}

#[test]
fn start_kinds_missing_turn_key_and_unknown_event_follow_mapping() {
    let f = Fixture::new();
    f.run(&["adopt"], "");
    for agent in ["claude", "codex"] {
        for (source, expected) in [
            ("startup", "startup"),
            ("resume", "resume"),
            ("clear", "clear"),
            ("compact", "compact"),
            ("fork", "other"),
            ("future", "other"),
        ] {
            let mut payload = f.payload(agent, "session_start");
            payload["source"] = source.into();
            assert!(!f.run(&["hook", agent], &payload.to_string()).is_empty());
            let store =
                cairn::store::Store::open(f.db(), cairn::store::BusyTimeout::UserCommand).unwrap();
            let detail: String = store.connection().query_row("SELECT detail FROM events WHERE kind='session_started' ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).unwrap();
            assert_eq!(detail, expected);
        }
        let mut stop = f.payload(agent, "stop");
        stop.as_object_mut().unwrap().remove(if agent == "claude" {
            "prompt_id"
        } else {
            "turn_id"
        });
        assert_eq!(
            f.run(&["hook", agent], &stop.to_string()),
            allow(agent, "stop")
        );
        assert_eq!(
            f.run(&["hook", agent], r#"{"hook_event_name":"future-event"}"#),
            ""
        );
    }
    assert!(!f.db().with_file_name("errors.log").exists());
}

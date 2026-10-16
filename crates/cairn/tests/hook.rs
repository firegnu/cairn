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
fn allow(_agent: &str, _case: &str) -> &'static str {
    ""
}

#[test]
fn lesson_hooks_inject_three_sections_and_supplement_typed_changes_with_shared_budget() {
    for agent in ["claude", "codex"] {
        let f = Fixture::new();
        f.run(&["adopt"], "");
        f.run(&["ledger", "add", "state", "开局现状"], "账正文");
        f.run(&["ledger", "add", "lesson", "开局经验"], "经验正文");
        f.run(
            &["save", "--source", "codex:previous"],
            "## 停点\n合成会话摘要",
        );
        let started: Value = serde_json::from_str(&f.hook(agent, "session_start")).unwrap();
        let text = started["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert!(text.find("### 项目的账").unwrap() < text.find("### 经验").unwrap());
        assert!(text.find("### 经验").unwrap() < text.find("### 本工作线").unwrap());
        for body in ["账正文", "经验正文", "合成会话摘要"] {
            assert!(text.contains(body));
        }
        assert!(f.hook(agent, "user_prompt_submit").is_empty());
        f.run(&["ledger", "add", "lesson", "新教训"], "新经验正文");
        f.run(&["ledger", "edit", "2", "--title", "合并教训"], "合并正文");
        f.run(&["ledger", "close", "3", "合并到 #2"], "");
        f.run(&["ledger", "add", "open", "待定事项"], "账的变动");
        let supplemented: Value =
            serde_json::from_str(&f.hook(agent, "user_prompt_submit")).unwrap();
        let text = supplemented["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert!(text.starts_with("[cairn] 项目的账有变动："));
        for change in [
            "新增 #3 新教训（经验） · 新经验正文",
            "修改 #2 合并教训（经验） · 合并正文",
            "了结 #3 新教训（经验） · 合并到 #2",
            "新增 #4 待定事项（悬而未决） · 账的变动",
        ] {
            assert!(text.contains(change), "missing {change}: {text}");
        }
        assert!(text.chars().count() <= 3000);
        assert!(f.hook(agent, "user_prompt_submit").is_empty());
        for kind in ["state", "lesson", "lesson", "open", "convention"] {
            f.run(&["ledger", "add", kind, "长条目"], &"字".repeat(800));
        }
        let supplemented: Value =
            serde_json::from_str(&f.hook(agent, "user_prompt_submit")).unwrap();
        let text = supplemented["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert!(text.contains("5 笔变动；查看"));
        assert!(text.contains(" ledger log`"));
        assert!(!text.contains(&"字".repeat(800)));
        assert!(text.chars().count() <= 3000);
        assert!(f.hook(agent, "user_prompt_submit").is_empty());
    }
}

#[test]
fn ledger_hooks_inject_start_then_each_change_once_with_historical_close_reason() {
    for agent in ["claude", "codex"] {
        let f = Fixture::new();
        f.run(&["adopt"], "");
        f.run(&["ledger", "add", "state", "开局现状"], "开局正文");
        let started: Value = serde_json::from_str(&f.hook(agent, "session_start")).unwrap();
        let start_text = started["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert!(start_text.contains("### 项目的账"));
        assert!(start_text.contains("#1 开局现状\n开局正文"));
        assert!(f.hook(agent, "user_prompt_submit").is_empty());
        f.run(&["ledger", "add", "open", "最初标题"], "最初正文");
        f.run(&["ledger", "edit", "2", "--title", "中间标题"], "中间正文");
        f.run(&["ledger", "close", "2", "第一笔理由"], "");
        f.run(&["ledger", "reopen", "2"], "");
        f.run(&["ledger", "edit", "2", "--title", "最新标题"], "最新正文");
        let store =
            cairn::store::Store::open(f.db(), cairn::store::BusyTimeout::UserCommand).unwrap();
        let source = format!(
            "{agent}:{}",
            f.payload(agent, "session_start")["session_id"]
                .as_str()
                .unwrap()
        );
        // Its own changes are also delivered; no model process is involved.
        store
            .connection()
            .execute(
                "UPDATE item_changes SET by=?1 WHERE ledger_version>1",
                [&source],
            )
            .unwrap();
        let supplemented: Value =
            serde_json::from_str(&f.hook(agent, "user_prompt_submit")).unwrap();
        let text = supplemented["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert!(text.starts_with("[cairn] 项目的账有变动："), "{text}");
        for value in [
            "新增 #2 最初标题",
            "最初正文",
            "修改 #2 中间标题",
            "中间正文",
            "了结 #2 中间标题",
            "第一笔理由",
            "恢复 #2 中间标题",
            "修改 #2 最新标题",
            "最新正文",
        ] {
            assert!(text.contains(value), "missing {value}: {text}");
        }
        assert!(!text.contains("开局正文"));
        assert!(text.find("最初正文").unwrap() < text.find("最新正文").unwrap());
        assert!(text.chars().count() <= 3000);
        assert!(f.hook(agent, "user_prompt_submit").is_empty());
        let version: i64 = store
            .connection()
            .query_row(
                "SELECT ledger_injected_version FROM sessions WHERE source_id=?1",
                [&source],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, 6);
        f.run(&["ledger", "delete", "2", "--yes"], "");
        let deleted = f.hook(agent, "user_prompt_submit");
        assert!(deleted.contains("删除 #2 （已删除）"));
        assert!(!deleted.contains("最新正文") && !deleted.contains("第一笔理由"));
        assert!(f.hook(agent, "user_prompt_submit").is_empty());
    }
}

#[test]
fn ledger_supplement_over_budget_reports_count_and_advances_once() {
    let f = Fixture::new();
    f.run(&["adopt"], "");
    f.hook("codex", "session_start");
    for _ in 0..5 {
        f.run(&["ledger", "add", "open", "合成长条目"], &"字".repeat(800));
    }
    let value: Value = serde_json::from_str(&f.hook("codex", "user_prompt_submit")).unwrap();
    let text = value["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(text.contains("5 笔变动"));
    assert!(text.contains(" ledger log`"));
    assert!(!text.contains(&"字".repeat(800)));
    assert!(text.chars().count() <= 3000);
    assert!(f.hook("codex", "user_prompt_submit").is_empty());
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
        "hook_seen",
        "sessions",
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
        assert_eq!(f.hook(agent, "stop"), "");
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
        let count: i64 = store
            .connection()
            .query_row("SELECT count(*) FROM turn_decisions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

fn check_stop_ignores_continuation_marker(agent: &str) {
    let f = Fixture::new();
    f.run(&["adopt"], "");
    assert_eq!(f.hook(agent, "user_prompt_submit"), "");
    for marker in [
        None,
        Some(Value::from("PRIVATE-INVALID-MARKER-R1")),
        Some(true.into()),
        Some(false.into()),
    ] {
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
        assert!(!f.db().with_file_name("errors.log").exists());
        assert!(last_seen(&f, agent, "Stop").is_string());
    }
}

#[test]
fn stop_ignores_continuation_marker_claude() {
    check_stop_ignores_continuation_marker("claude");
}

#[test]
fn stop_ignores_continuation_marker_codex() {
    check_stop_ignores_continuation_marker("codex");
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
        assert!(output.stdout.is_empty());
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
    bad["session_id"] = serde_json::json!(["PRIVATE-ANSWER-MARKER-8R2"]);
    assert_eq!(f.run(&["hook", "codex"], &bad.to_string()), "");
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
        payload["session_id"] = serde_json::json!([markers[1]]);
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
    check_tree(f.db().parent().unwrap(), &markers[..2]);
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
    assert_eq!(f.run(&["hook", "codex"], &stop.to_string()), "");
    assert!(!f.log().contains("codex Stop core"));
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
            ("fork", "fork"),
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

const EVENTS: [&str; 4] = ["SessionStart", "UserPromptSubmit", "Stop", "SessionEnd"];

/// hook_seen as (agent, event, at), in that order.
fn seen(f: &Fixture) -> Vec<(String, String, String)> {
    let connection = rusqlite::Connection::open(f.db()).unwrap();
    let mut query = connection
        .prepare("SELECT agent, event, at FROM hook_seen ORDER BY agent, event")
        .unwrap();
    let rows = query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    rows
}

fn last_seen(f: &Fixture, agent: &str, event: &str) -> Value {
    let status: Value = serde_json::from_str(&f.run(&["status", "--json"], "")).unwrap();
    let all = status["agents"][agent]["last_seen"]
        .as_object()
        .unwrap_or_else(|| panic!("{agent} has no last_seen: {status}"));
    assert_eq!(all.len(), EVENTS.len(), "{all:?}");
    all[event].clone()
}

#[test]
fn each_handled_event_is_remembered_for_its_project_and_agent_and_status_reports_it() {
    let f = Fixture::new();
    f.init_git();
    f.run(&["adopt"], "");
    for agent in ["claude", "codex"] {
        for event in EVENTS {
            assert_eq!(last_seen(&f, agent, event), Value::Null, "{agent} {event}");
        }
    }

    for case in ["session_start", "user_prompt_submit", "stop", "session_end"] {
        f.hook("claude", case);
    }
    // An event cairn does not handle leaves nothing.
    f.hook("codex", "interrupt");
    let rows = seen(&f);
    assert_eq!(
        rows.iter()
            .map(|(agent, event, _)| (agent.as_str(), event.as_str()))
            .collect::<Vec<_>>(),
        [
            ("claude", "SessionEnd"),
            ("claude", "SessionStart"),
            ("claude", "Stop"),
            ("claude", "UserPromptSubmit"),
        ]
    );
    for (_, event, at) in &rows {
        assert!(at.len() == 24 && at.ends_with('Z'), "{event} at {at}");
        assert_eq!(last_seen(&f, "claude", event), Value::String(at.clone()));
        assert_eq!(last_seen(&f, "codex", event), Value::Null);
    }

    // A later one replaces the time; there is still one row for it.
    let stop = |f: &Fixture| last_seen(f, "claude", "Stop").as_str().unwrap().to_owned();
    let before = stop(&f);
    std::thread::sleep(std::time::Duration::from_millis(5));
    f.hook("claude", "stop_continued");
    assert!(stop(&f) > before, "{} against {before}", stop(&f));
    assert_eq!(seen(&f).len(), 4);

    // `status` only reads: asking changed nothing.
    let rows = seen(&f);
    last_seen(&f, "claude", "Stop");
    assert_eq!(seen(&f), rows);
}

#[test]
fn nothing_is_remembered_while_disabled_or_once_unadopted() {
    let f = Fixture::new();
    f.init_git();
    f.run(&["adopt"], "");
    f.hook("codex", "session_start");
    let rows = seen(&f);
    assert_eq!(rows.len(), 1);

    std::env::set_var("CAIRN_DISABLE", "1");
    for &(agent, case) in CASES {
        f.hook(agent, case);
    }
    std::env::remove_var("CAIRN_DISABLE");
    assert_eq!(seen(&f), rows);

    f.run(&["unadopt"], "");
    for &(agent, case) in CASES {
        f.hook(agent, case);
    }
    assert_eq!(seen(&f), rows);
    // What it saw while adopted is still reported.
    assert_eq!(
        last_seen(&f, "codex", "SessionStart"),
        Value::String(rows[0].2.clone())
    );
}

#[test]
fn version_1_database_reports_nothing_seen_until_a_hook_upgrades_it() {
    let f = Fixture::new();
    f.init_git();
    f.run(&["adopt"], "");
    // As cairn 0.1.0 left it: no hook_seen, version 1.
    let connection = rusqlite::Connection::open(f.db()).unwrap();
    connection
        .execute_batch(
            "DROP TABLE item_changes; DROP TABLE items; DROP TABLE ledgers; DROP TABLE sessions; ALTER TABLE records DROP COLUMN written_by; DROP TABLE hook_seen; UPDATE meta SET value = '1' WHERE key = 'schema_version';",
        )
        .unwrap();
    let version = |connection: &rusqlite::Connection| -> String {
        connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };

    let status: Value = serde_json::from_str(&f.run(&["status", "--json"], "")).unwrap();
    assert_eq!(status["project"]["status"], "adopted");
    for agent in ["claude", "codex"] {
        for event in EVENTS {
            assert_eq!(last_seen(&f, agent, event), Value::Null, "{agent} {event}");
        }
    }
    assert_eq!(version(&connection), "1");

    assert!(!f.hook("claude", "session_start").is_empty());
    assert_eq!(version(&connection), "4");
    assert!(last_seen(&f, "claude", "SessionStart").is_string());
}

#[test]
fn session_metadata_tracks_start_path_turns_and_end_facts_without_reading_transcript() {
    use rusqlite::OptionalExtension;
    let f = Fixture::new();
    f.init_git();
    f.run(&["adopt"], "");
    for agent in ["claude", "codex"] {
        f.hook(agent, "session_start");
        let source = format!("{agent}:synthetic-{agent}-session");
        let store =
            cairn::store::Store::open(f.db(), cairn::store::BusyTimeout::UserCommand).unwrap();
        let row = || -> Option<(String, String, i64, String, String)> {
            store.connection().query_row(
                "SELECT line_path,transcript_path,turns,facts,facts_at FROM sessions WHERE source_id=?1",
                [&source], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))
            ).optional().unwrap()
        };
        let initial = row();
        assert!(
            initial.is_some(),
            "SessionStart must persist the session metadata"
        );
        let initial = initial.unwrap();
        assert_eq!(initial.0, f.cwd().canonicalize().unwrap().to_str().unwrap());
        assert!(initial.1.ends_with("never-open-transcript"));
        assert_eq!(initial.2, 0);
        store.connection().execute("UPDATE sessions SET failures=2,summarized_bytes=42,summarized_turns=1 WHERE source_id=?1", [&source]).unwrap();
        fs::write(
            f.cwd().join(".git/HEAD"),
            format!("ref: refs/heads/{agent}-next\n"),
        )
        .unwrap();
        let mut prompt = f.payload(agent, "user_prompt_submit");
        let transcript = f.root.path().join(format!("{agent}-new-transcript"));
        prompt["transcript_path"] = transcript.to_str().unwrap().into();
        assert_eq!(f.run(&["hook", agent], &prompt.to_string()), "");
        assert_eq!(f.run(&["hook", agent], &prompt.to_string()), "");
        let started = row().unwrap();
        assert_eq!(started.1, transcript.to_str().unwrap());
        assert_eq!(started.2, 2, "duplicate starts count as observed events");
        let branch: String = store
            .connection()
            .query_row(
                "SELECT branch FROM sessions WHERE source_id=?1",
                [&source],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(branch, format!("{agent}-next"));
        assert_eq!((&started.3, &started.4), (&initial.3, &initial.4));
        let state: (i64,i64,i64,Option<String>,Option<String>) = store.connection().query_row(
            "SELECT failures,summarized_bytes,summarized_turns,waiter_id,waiter_beat_at FROM sessions WHERE source_id=?1", [&source],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
        assert_eq!(state, (0, 42, 1, None, None));
        fs::write(f.cwd().join(format!("{agent}-change")), "synthetic").unwrap();
        let mut stop = f.payload(agent, "stop");
        stop.as_object_mut().unwrap().remove("transcript_path");
        assert_eq!(f.run(&["hook", agent], &stop.to_string()), "");
        let (path, turns, facts, at): (Option<String>, i64, String, String) = store
            .connection()
            .query_row(
                "SELECT transcript_path,turns,facts,facts_at FROM sessions WHERE source_id=?1",
                [&source],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(path, None, "a hook without a path records NULL");
        assert_eq!(turns, 2);
        let facts: Value = serde_json::from_str(&facts).unwrap();
        assert!(facts["untracked"].as_u64().unwrap() >= 1);
        assert_eq!(facts["collected_at"], at);
        let before: String = store
            .connection()
            .query_row(
                "SELECT last_event_at FROM sessions WHERE source_id=?1",
                [&source],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(f.hook(agent, "session_end"), "");
        let after: String = store
            .connection()
            .query_row(
                "SELECT last_event_at FROM sessions WHERE source_id=?1",
                [&source],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            before, after,
            "SessionEnd only writes its event and hook_seen"
        );
        assert!(!transcript.exists());
    }
}

#[test]
fn stop_keeps_previous_git_facts_when_collection_fails_and_still_updates_path() {
    let f = Fixture::new();
    f.init_git();
    f.run(&["adopt"], "");
    f.hook("codex", "session_start");
    let store = cairn::store::Store::open(f.db(), cairn::store::BusyTimeout::UserCommand).unwrap();
    let snapshot = || -> (String, String, Option<String>) {
        store
            .connection()
            .query_row("SELECT facts,facts_at,branch FROM sessions", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap()
    };
    let before = snapshot();
    let bin = f.root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let git = bin.join("git");
    fs::write(&git, "#!/bin/sh\ncase \"$*\" in *'status --porcelain'*) exit 1;; esac\nexec /usr/bin/git \"$@\"\n").unwrap();
    fs::set_permissions(&git, fs::Permissions::from_mode(0o700)).unwrap();
    std::env::set_var("PATH", &bin);
    let mut payload = f.payload("codex", "stop");
    payload["transcript_path"] = f.root.path().join("after-failure").to_str().unwrap().into();
    assert_eq!(f.run(&["hook", "codex"], &payload.to_string()), "");
    assert_eq!(snapshot(), before);
    let path: String = store
        .connection()
        .query_row("SELECT transcript_path FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert!(path.ends_with("after-failure"));
    assert!(f.log().contains("codex Stop git"));
}

use cairn::store::{BusyTimeout, Store};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn add(kind: &str, title: &str, body: &str, evidence: &str) -> Value {
    serde_json::json!({"op":"add", "kind":kind, "no":null, "title":title,
        "body":body, "reason":null, "evidence":evidence})
}

fn edit(no: i64, title: &str, body: &str, evidence: &str) -> Value {
    serde_json::json!({"op":"edit", "kind":null, "no":no, "title":title,
        "body":body, "reason":null, "evidence":evidence})
}

fn close(no: i64, reason: &str, evidence: &str) -> Value {
    serde_json::json!({"op":"close", "kind":null, "no":no, "title":null,
        "body":null, "reason":reason, "evidence":evidence})
}

fn changes(changes: &[Value]) -> String {
    serde_json::json!({"changes": changes}).to_string()
}

struct Fixture(tempfile::TempDir);

impl Fixture {
    fn new() -> Self {
        let f = Self(tempfile::tempdir().unwrap());
        fs::create_dir(f.path("project")).unwrap();
        fs::create_dir(f.path("bin")).unwrap();
        f
    }

    fn path(&self, path: &str) -> PathBuf {
        self.0.path().join(path)
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cairn"));
        command
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.path("bin").display()),
            )
            .env("HOME", self.0.path())
            .env("XDG_STATE_HOME", self.path("state"))
            .env("XDG_DATA_HOME", self.path("data with ' quote"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("CODEX_HOME", self.path("codex"))
            .env("CLAUDE_CONFIG_DIR", self.path("claude"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("TMPDIR", self.0.path())
            .env("SIDECAR_TEST_DIR", self.0.path())
            .current_dir(self.path("project"))
            .args(args);
        command
    }

    fn run(&self, args: &[&str], input: &str) -> Output {
        let mut child = self
            .command(args)
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

    fn ok(&self, args: &[&str], input: &str) -> String {
        let output = self.run(args, input);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn store(&self) -> Store {
        Store::open(self.path("state/cairn/cairn.db"), BusyTimeout::UserCommand).unwrap()
    }

    fn hook(&self, sid: &str, event: &str) {
        self.hook_as("claude", sid, event);
    }

    fn hook_as(&self, agent: &str, sid: &str, event: &str) {
        // These tests drive the foreground summarize command. Hold leases while
        // seeding hook metadata so automatic workers cannot race that command.
        let mut store = self.store();
        let sources: Vec<String> = store
            .connection()
            .prepare("SELECT source_id FROM sessions")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let leases: Vec<_> = sources
            .iter()
            .filter_map(|source| {
                cairn::sidecar::Lease::acquire(&mut store, source, chrono::Utc::now()).unwrap()
            })
            .collect();
        self.ok(
            &["hook", agent],
            &serde_json::json!({
                "hook_event_name": event, "session_id": sid, "cwd": self.path("project"),
                "transcript_path": self.path(&format!("{sid}.jsonl")), "turn_id":"synthetic-turn"
            })
            .to_string(),
        );
        for lease in leases {
            lease.release(store.connection()).unwrap();
        }
    }

    fn session(&self, sid: &str, text: &str) {
        fs::write(self.path(&format!("{sid}.jsonl")), dialogue(text)).unwrap();
        self.hook(sid, "SessionStart");
        self.hook(sid, "UserPromptSubmit");
        self.hook(sid, "Stop");
    }

    fn fake_codex(&self, checks: &str, body: &str) {
        let script = format!(
            r##"#!/bin/sh
set -eu
[ "$CAIRN_DISABLE" = 1 ]
[ "$1" = exec ]; shift
[ -z "$(ls -A)" ]
printf '%s' "$PWD" > "$SIDECAR_TEST_DIR/cwd"
ephemeral=0; disable_memories=0; model=; effort=; output=; schema=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --output-schema) schema=$2; [ -f "$schema" ]; shift 2 ;;
        --ephemeral) ephemeral=1; shift ;;
        --disable) [ "$2" = memories ]; disable_memories=1; shift 2 ;;
        --skip-git-repo-check) shift ;;
        -m) model=$2; shift 2 ;;
        -c) effort=$2; shift 2 ;;
        -o) output=$2; shift 2 ;;
        -) shift ;;
        *) exit 81 ;;
    esac
done
[ "$ephemeral" = 1 ]
[ "$disable_memories" = 1 ]
input=$(cat)
case "$input" in
    *'你在为一个软件项目维护"项目的账"'*)
        [ -n "$schema" ]
        printf '%s' "$schema" > "$SIDECAR_TEST_DIR/schema-path"
        cp "$schema" "$SIDECAR_TEST_DIR/schema.json"
        if [ -f "$SIDECAR_TEST_DIR/ledger-checks" ]; then . "$SIDECAR_TEST_DIR/ledger-checks"; fi
        if [ -f "$SIDECAR_TEST_DIR/ledger-output" ]; then
            cat "$SIDECAR_TEST_DIR/ledger-output" > "$output"
        else
            printf '{{"changes":[]}}\n' > "$output"
        fi
        exit 0 ;;
esac
[ -z "$schema" ]
case "$input" in *'不要调用工具'*'不超过 6 KiB'*'会话摘录'*) ;; *) exit 82 ;; esac
{checks}
cat > "$output" <<'SUMMARY'
{body}
SUMMARY
"##
        );
        fs::write(self.path("bin/codex"), script).unwrap();
        fs::set_permissions(self.path("bin/codex"), fs::Permissions::from_mode(0o700)).unwrap();
    }

    fn records(&self, all: bool) -> Value {
        let args = if all {
            vec!["list", "--json", "--all"]
        } else {
            vec!["list", "--json"]
        };
        serde_json::from_str(&self.ok(&args, "")).unwrap()
    }

    fn fake_ledger(&self, summary: &str, output: impl AsRef<str>, checks: &str) {
        self.fake_codex("", summary);
        fs::write(self.path("ledger-output"), output.as_ref()).unwrap();
        fs::write(self.path("ledger-checks"), checks).unwrap();
    }

    fn ledger(&self) -> Value {
        serde_json::from_str(&self.ok(&["ledger", "--all", "--json"], "")).unwrap()
    }

    fn ledger_item(&self, no: &str) -> Value {
        serde_json::from_str(&self.ok(&["ledger", "show", no, "--json"], "")).unwrap()
    }

    fn git(&self, args: &[&str]) {
        let environment = self.command(&[]);
        let mut child = Command::new("/usr/bin/git")
            .env_clear()
            .envs(
                environment
                    .get_envs()
                    .filter_map(|(k, v)| v.map(|v| (k, v))),
            )
            .current_dir(self.path("project"))
            .args([
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
            ])
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let start = std::time::Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "git {args:?}");
                break;
            }
            if start.elapsed() > std::time::Duration::from_secs(2) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("git timed out");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

fn dialogue(text: &str) -> String {
    format!(
        "{}\n",
        serde_json::json!({"type":"user","message":{"content":text}})
    )
}

#[test]
fn config_lists_reads_and_persists_only_sidecar_settings() {
    let f = Fixture::new();
    let defaults = f.ok(&["config"], "");
    assert!(defaults.contains("sidecar.model = gpt-5.6-luna"));
    assert!(defaults.contains("sidecar.effort = medium"));
    f.ok(&["config", "sidecar.model", "synthetic-model"], "");
    f.ok(&["config", "sidecar.effort", "low"], "");
    assert_eq!(
        f.ok(&["config", "sidecar.model"], "").trim(),
        "sidecar.model = synthetic-model"
    );
    assert_eq!(
        f.ok(&["config", "sidecar.effort"], "").trim(),
        "sidecar.effort = low"
    );
    assert!(!f
        .run(&["config", "schema_version", "0"], "")
        .status
        .success());
    assert!(!f.run(&["config", "sidecar.model", ""], "").status.success());
    let store = Store::open(f.path("state/cairn/cairn.db"), BusyTimeout::UserCommand).unwrap();
    let value: String = store
        .connection()
        .query_row(
            "SELECT value FROM meta WHERE key='sidecar.model'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(value, "synthetic-model");
}

#[test]
fn summarize_reads_full_transcript_uses_config_and_writes_session_checkpoint() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "synthetic-first-message");
    f.ok(&["config", "sidecar.model", "synthetic-model"], "");
    f.ok(&["config", "sidecar.effort", "low"], "");
    f.fake_codex(
        r#"
[ "$model" = synthetic-model ]
[ "$effort" = 'model_reasoning_effort="low"' ]
case "$input" in *'user: synthetic-first-message'*) ;; *) exit 83 ;; esac
stat -f '%Lp' .. > "$SIDECAR_TEST_DIR/outer-mode"
"#,
        "## 停点\nfirst-summary",
    );
    let mut command = f.command(&["summarize"]);
    // SAFETY: the child callback only changes its own umask using libc.
    unsafe {
        command.pre_exec(|| {
            libc::umask(0);
            Ok(())
        });
    }
    let output = command.stdin(Stdio::null()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("claude:one") && text.contains("写成"),
        "{text}"
    );
    let records = f.records(false);
    assert_eq!(records["total"], 1);
    assert_eq!(records["records"][0]["source_id"], "claude:one");
    assert_eq!(records["records"][0]["written_by"], "sidecar");
    assert_eq!(records["records"][0]["summary"], "first-summary");
    let store = f.store();
    let (bytes, turns, id, failures, waiter): (i64, i64, String, u32, Option<String>) = store.connection().query_row(
        "SELECT summarized_bytes,summarized_turns,summary_record_id,failures,waiter_id FROM sessions WHERE source_id='claude:one'",
        [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).unwrap();
    assert_eq!(
        bytes as u64,
        fs::metadata(f.path("one.jsonl")).unwrap().len()
    );
    assert_eq!(turns, 1);
    assert_eq!(records["records"][0]["id"], id);
    assert_eq!(failures, 0);
    assert!(waiter.is_none());
    assert_eq!(
        fs::read_to_string(f.path("outer-mode")).unwrap().trim(),
        "700"
    );
    let cwd = fs::read_to_string(f.path("cwd")).unwrap();
    assert!(
        !PathBuf::from(cwd).exists(),
        "model directory must be removed"
    );
    assert!(f.ok(&["summarize"], "").contains("没有需要"));
    assert_eq!(f.records(true)["total"], 1);
}

#[test]
fn no_dialogue_increment_advances_both_stages_and_resumes_on_dialogue() {
    for automatic in [false, true] {
        let f = Fixture::new();
        f.ok(&["adopt"], "");
        f.session("one", "synthetic-first-message");
        f.hook("one", "SessionEnd");
        let database = f.path("state/cairn/cairn.db");
        let args = if automatic {
            vec![
                "__wait",
                "--database",
                database.to_str().unwrap(),
                "--root",
                f.0.path().to_str().unwrap(),
                "--source",
                "claude:one",
            ]
        } else {
            vec!["summarize"]
        };
        f.fake_ledger(
            "## 停点\nprevious-summary",
            changes(&[add(
                "state",
                "first title",
                "first body",
                "synthetic evidence",
            )]),
            "",
        );
        f.ok(&args, "");
        let records = f.records(true);
        let ledger = f.ledger();
        fs::remove_file(f.path("cwd")).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(f.path("one.jsonl"))
            .unwrap()
            .write_all(b"{\"type\":\"metadata\"}\n{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"synthetic-tool\",\"content\":\"synthetic result\"}]}}\n")
            .unwrap();
        // A successful no-op also clears a previous unsuccessful attempt.
        f.store().connection().execute(
            "UPDATE sessions SET failures=1,last_failure='previous failure',last_failure_at='2026-10-16T00:00:00Z',ledger_failures=1,ledger_last_failure='previous failure',ledger_last_failure_at='2026-10-16T00:00:00Z'",
            [],
        ).unwrap();
        f.ok(&args, "");
        assert!(
            !f.path("cwd").exists(),
            "no dialogue must not call the model"
        );
        let (bytes, ledger_bytes, cleared): (i64, i64, bool) = f.store().connection().query_row(
            "SELECT summarized_bytes,ledger_bytes,failures=0 AND last_failure IS NULL AND last_failure_at IS NULL AND ledger_failures=0 AND ledger_last_failure IS NULL AND ledger_last_failure_at IS NULL AND waiter_id IS NULL FROM sessions WHERE source_id='claude:one'",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        let length = fs::metadata(f.path("one.jsonl")).unwrap().len();
        assert_eq!((bytes as u64, ledger_bytes as u64), (length, length));
        assert!(cleared);
        assert_eq!(f.records(true), records);
        assert_eq!(f.ledger(), ledger);
        let status: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
        assert!(status["sidecar"]["last_failure"].is_null());
        assert!(status["ledger"]["last_failure"].is_null());
        let injected = f.ok(
            &["hook", "claude"],
            &serde_json::json!({
                "hook_event_name":"SessionStart", "session_id":"new", "cwd":f.path("project")
            })
            .to_string(),
        );
        assert!(!injected.contains("没写成"), "{injected}");
        f.ok(&args, "");
        assert!(!f.path("cwd").exists());

        fs::OpenOptions::new()
            .append(true)
            .open(f.path("one.jsonl"))
            .unwrap()
            .write_all(dialogue("synthetic-new-dialogue").as_bytes())
            .unwrap();
        f.hook("one", "UserPromptSubmit");
        f.hook("one", "SessionEnd");
        let checks = r#"case "$input" in *'user: synthetic-new-dialogue'*) ;; *) exit 83 ;; esac
case "$input" in *'user: synthetic-first-message'*|*'synthetic result'*) exit 84 ;; esac"#;
        f.fake_ledger(
            "## 停点\nnew-summary",
            changes(&[edit(1, "new title", "new body", "new synthetic evidence")]),
            checks,
        );
        f.fake_codex(checks, "## 停点\nnew-summary");
        f.ok(&args, "");
        assert_eq!(f.records(false)["records"][0]["summary"], "new-summary");
        assert_eq!(f.records(true)["total"], 2);
        assert_eq!(f.ledger_item("1")["item"]["title"], "new title");
    }
}

#[test]
fn no_dialogue_from_start_still_fails_both_stages() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "synthetic-placeholder");
    fs::write(f.path("one.jsonl"), "{\"type\":\"metadata\"}\n").unwrap();
    f.fake_codex("", "## 停点\nshould-not-be-used");
    assert!(!f.run(&["summarize"], "").status.success());
    assert!(!f.path("cwd").exists());
    let failed: bool = f.store().connection().query_row(
        "SELECT summarized_bytes=0 AND ledger_bytes=0 AND failures=1 AND ledger_failures=1 AND last_failure LIKE '%no user or assistant text%' AND ledger_last_failure LIKE '%no user or assistant text%' FROM sessions WHERE source_id='claude:one'",
        [], |r| r.get(0),
    ).unwrap();
    assert!(failed);
}

#[test]
fn summarize_skip_advances_progress_without_records_or_failures() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "synthetic-small-talk");
    f.fake_codex("", "invalid-summary");
    assert!(!f.run(&["summarize"], "").status.success());
    f.fake_codex("", " \nNOTHING_TO_CONTINUE\n\t");
    let output = f.ok(&["summarize"], "");
    assert!(output.contains("不用写"), "{output}");
    assert_eq!(f.records(true)["total"], 0);
    let store = f.store();
    let (bytes, turns, failures, cleared): (i64, i64, i64, bool) = store
        .connection()
        .query_row(
            "SELECT summarized_bytes,summarized_turns,failures,
         summary_record_id IS NULL AND last_failure IS NULL AND last_failure_at IS NULL
         AND waiter_id IS NULL AND waiter_beat_at IS NULL
         FROM sessions WHERE source_id='claude:one'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        bytes as u64,
        fs::metadata(f.path("one.jsonl")).unwrap().len()
    );
    assert_eq!(turns, 1);
    assert!(cleared);
    assert_eq!(failures, 0);
}

#[test]
fn skipped_sessions_preserve_previous_summary_and_resume_on_new_turns() {
    for has_previous in [false, true] {
        let f = Fixture::new();
        f.ok(&["adopt"], "");
        f.session("one", "synthetic-small-talk");
        if has_previous {
            f.fake_codex("", "## 停点\nprevious-summary");
            f.ok(&["summarize"], "");
            fs::OpenOptions::new()
                .append(true)
                .open(f.path("one.jsonl"))
                .unwrap()
                .write_all(dialogue("more-small-talk").as_bytes())
                .unwrap();
            f.hook("one", "UserPromptSubmit");
        }
        let before = f.records(true);
        f.fake_codex(
            r#"case "$input" in *'先判断：下一个会话接着干活时'*'NOTHING_TO_CONTINUE，不写别的。拿不准时，按用得上写。'*'用得上时，只输出 Markdown'*) ;; *) exit 83 ;; esac"#,
            "NOTHING_TO_CONTINUE",
        );
        f.ok(&["summarize"], "");
        assert_eq!(f.records(true), before);
        assert_eq!(f.records(false)["total"], u64::from(has_previous));
        let show = f.ok(&["show"], "");
        assert!(
            !show.contains("还没有摘要") && !show.contains("还没写进摘要"),
            "{show}"
        );
        assert!(
            !show.contains("可重试") && !show.contains("尚未写摘要"),
            "{show}"
        );
        if has_previous {
            assert!(show.contains("previous-summary"));
        }
        let status: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
        assert_eq!(status["sidecar"]["unsummarized_sessions"], 0);
        assert!(f.ok(&["status"], "").contains("还没有摘要的会话：0"));
        fs::remove_file(f.path("cwd")).unwrap();
        assert!(f.ok(&["summarize"], "").contains("没有需要"));
        assert!(!f.path("cwd").exists());

        fs::OpenOptions::new()
            .append(true)
            .open(f.path("one.jsonl"))
            .unwrap()
            .write_all(dialogue("useful-new-turn").as_bytes())
            .unwrap();
        f.hook("one", "UserPromptSubmit");
        let show = f.ok(&["show"], "");
        assert!(
            show.contains(if has_previous {
                "还没写进摘要"
            } else {
                "还没有摘要"
            }),
            "{show}"
        );
        let status: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
        assert_eq!(
            status["sidecar"]["unsummarized_sessions"],
            u64::from(!has_previous)
        );
        let checks = if has_previous {
            r#"case "$input" in *'上一版摘要'*'previous-summary'*'会话摘录'*'useful-new-turn'*) ;; *) exit 84 ;; esac
case "$input" in *'user: synthetic-small-talk'*|*'user: more-small-talk'*) exit 85 ;; esac"#
        } else {
            r#"case "$input" in *'user: synthetic-small-talk'*'user: useful-new-turn'*) ;; *) exit 84 ;; esac"#
        };
        f.fake_codex(checks, "## 停点\nuseful-summary");
        f.ok(&["summarize"], "");
        let records = f.records(false);
        assert_eq!(records["total"], 1);
        assert_eq!(records["records"][0]["summary"], "useful-summary");
        assert_eq!(f.records(true)["total"], 1 + u64::from(has_previous));
    }
}

#[test]
fn waiter_skip_is_processed_and_session_start_injects_no_missing_summary() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "synthetic-small-talk");
    f.hook("one", "SessionEnd");
    f.fake_codex("", "NOTHING_TO_CONTINUE");
    let database = f.path("state/cairn/cairn.db");
    let args = [
        "__wait",
        "--database",
        database.to_str().unwrap(),
        "--root",
        f.0.path().to_str().unwrap(),
        "--source",
        "claude:one",
    ];
    f.ok(&args, "");
    assert_eq!(f.records(true)["total"], 0);
    fs::remove_file(f.path("cwd")).unwrap();
    f.ok(&args, "");
    assert!(
        !f.path("cwd").exists(),
        "processed session was sent to the model again"
    );
    let injected = f.ok(
        &["hook", "claude"],
        &serde_json::json!({
            "hook_event_name":"SessionStart", "session_id":"new", "cwd":f.path("project")
        })
        .to_string(),
    );
    assert!(
        !injected.contains("还没有摘要") && !injected.contains("还没写进摘要"),
        "{injected}"
    );
    assert!(
        !injected.contains("最近一次没写成") && !injected.contains("摘要正在写"),
        "{injected}"
    );
    let status: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
    assert_eq!(status["sidecar"]["unsummarized_sessions"], 0);
    assert!(status["sidecar"]["last_failure"].is_null());

    fs::OpenOptions::new()
        .append(true)
        .open(f.path("one.jsonl"))
        .unwrap()
        .write_all(dialogue("useful-new-turn").as_bytes())
        .unwrap();
    f.hook("one", "UserPromptSubmit");
    f.hook("one", "SessionEnd");
    f.fake_codex("", "## 停点\nbackground-summary");
    f.ok(&args, "");
    assert_eq!(
        f.records(false)["records"][0]["summary"],
        "background-summary"
    );
}

#[test]
fn save_during_generation_keeps_its_visibility_and_rejects_stale_summary() {
    for has_previous in [false, true] {
        let f = Fixture::new();
        f.ok(&["adopt"], "");
        f.session("one", "original-dialogue");
        if has_previous {
            f.fake_codex("", "## 停点\nprevious-summary");
            f.ok(&["summarize"], "");
            fs::OpenOptions::new()
                .append(true)
                .open(f.path("one.jsonl"))
                .unwrap()
                .write_all(dialogue("added-dialogue").as_bytes())
                .unwrap();
            f.hook("one", "UserPromptSubmit");
        }
        let store = f.store();
        let progress = || {
            store.connection().query_row(
            "SELECT summarized_bytes,summarized_turns,summary_record_id FROM sessions WHERE source_id='claude:one'", [],
            |r| Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,Option<String>>(2)?))).unwrap()
        };
        let before = progress();
        let previous_check = if has_previous {
            "case \"$input\" in *'上一版摘要'*'previous-summary'*) ;; *) exit 83 ;; esac"
        } else {
            "case \"$input\" in *'previous-summary'*) exit 83 ;; esac"
        };
        f.fake_codex(
            &format!(
                r#"
{previous_check}
case "$input" in *'manual-during-generation'*) exit 84 ;; esac
touch "$SIDECAR_TEST_DIR/ready"
while [ ! -e "$SIDECAR_TEST_DIR/release" ]; do sleep 0.01; done
"#
            ),
            "## 停点\nsummary-from-old-input",
        );
        let mut child = f
            .command(&["summarize"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // Always release and reap the child, even if setup or an assertion fails.
        let concurrent_save = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !f.path("ready").exists() {
                assert!(
                    child.try_wait().unwrap().is_none(),
                    "fake codex exited before the input barrier"
                );
                assert!(
                    std::time::Instant::now() < deadline,
                    "input barrier timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            f.ok(
                &["save", "--source", "claude:one"],
                "## 停点\nmanual-during-generation",
            );
            f.ok(&["show"], ""); // Ingest while the model holds the old input.
            let active = f.records(false);
            assert_eq!(active["records"][0]["summary"], "manual-during-generation");
            active["records"][0]["id"].as_str().unwrap().to_owned()
        }));
        fs::write(f.path("release"), b"").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() {
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("summary did not finish after barrier release");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        let manual = concurrent_save.unwrap();
        let active = f.records(false);
        assert_eq!(active["total"], 1);
        assert_eq!(
            active["records"][0]["id"], manual,
            "unread manual save was replaced (previous={has_previous})"
        );
        assert!(!output.status.success());
        assert!(String::from_utf8(output.stdout)
            .unwrap()
            .contains("生成期间上一版变了"));
        assert_eq!(progress(), before);
        let (failures, reason, at, waiter, beat): (i64,String,Option<String>,Option<String>,Option<String>) = store.connection().query_row(
            "SELECT failures,last_failure,last_failure_at,waiter_id,waiter_beat_at FROM sessions WHERE source_id='claude:one'", [],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
        assert_eq!(failures, 1);
        assert_eq!(reason, "生成期间上一版变了");
        assert!(at.is_some() && waiter.is_none() && beat.is_none());
        assert!(!f.records(true)["records"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["summary"] == "summary-from-old-input"));
        f.fake_codex(
            r#"
case "$input" in *'上一版摘要'*'manual-during-generation'*) ;; *) exit 85 ;; esac
"#,
            "## 停点\nsummary-after-retry",
        );
        f.ok(&["summarize"], "");
        assert_eq!(
            f.records(false)["records"][0]["summary"],
            "summary-after-retry"
        );
    }
}

#[test]
fn summarize_combines_previous_visible_save_with_increment_and_replaces_it() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "old-message-must-not-repeat");
    f.fake_codex("", "## 停点\nold-sidecar");
    f.ok(&["summarize"], "");
    let first = f.records(false)["records"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    f.ok(
        &["save", "--source", "claude:one"],
        "## 停点\nmanual-latest",
    );
    f.ok(&["show"], ""); // collect the voluntary save
    let manual = f.records(false)["records"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    fs::OpenOptions::new()
        .append(true)
        .open(f.path("one.jsonl"))
        .unwrap()
        .write_all(dialogue("new-message-only").as_bytes())
        .unwrap();
    f.hook("one", "UserPromptSubmit");
    f.fake_codex(
        r#"
case "$input" in *'上一版摘要'*'manual-latest'*'会话摘录'*'new-message-only'*) ;; *) exit 83 ;; esac
case "$input" in *'old-message-must-not-repeat'*|*'old-sidecar'*) exit 84 ;; esac
"#,
        "## 停点\nnew-version",
    );
    f.ok(&["summarize"], "");
    let active = f.records(false);
    assert_eq!(active["total"], 1);
    assert_eq!(active["records"][0]["summary"], "new-version");
    let all = f.records(true);
    assert_eq!(all["total"], 3);
    let rows = all["records"].as_array().unwrap();
    assert_eq!(
        rows.iter().find(|r| r["id"] == manual).unwrap()["replaced_by"],
        active["records"][0]["id"]
    );
    assert_eq!(
        rows.iter().find(|r| r["id"] == first).unwrap()["replaced_by"],
        manual
    );
    f.ok(&["restore", &first], "");
    assert!(f.records(false)["records"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == first));
}

#[test]
fn summarize_uses_full_text_without_visible_previous_checkpoint() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "original-text");
    f.fake_codex("", "## 停点\nto-delete");
    f.ok(&["summarize"], "");
    let id = f.records(false)["records"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    f.ok(&["delete", &id, "--yes"], "");
    fs::OpenOptions::new()
        .append(true)
        .open(f.path("one.jsonl"))
        .unwrap()
        .write_all(dialogue("added-text").as_bytes())
        .unwrap();
    f.fake_codex(
        r#"
case "$input" in *'会话摘录'*'original-text'*'added-text'*) ;; *) exit 83 ;; esac
case "$input" in *'to-delete'*) exit 84 ;; esac
"#,
        "## 停点\nfull-again",
    );
    f.ok(&["summarize"], "");
    assert_eq!(f.records(false)["records"][0]["summary"], "full-again");
}

#[test]
fn failures_are_counted_without_content_and_manual_retry_resets_them() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "private-synthetic-content");
    for (checks, body, reason) in [
        ("echo private-synthetic-content >&2; exit 7", "", "退出"),
        ("", "private-synthetic-content", "输出不合格"),
    ] {
        f.fake_codex(checks, body);
        let failed = f.run(&["summarize"], "");
        assert!(!failed.status.success());
        let report = format!(
            "{}{}",
            String::from_utf8_lossy(&failed.stdout),
            String::from_utf8_lossy(&failed.stderr)
        );
        assert!(
            report.contains("claude:one") && report.contains(reason),
            "{report}"
        );
        assert!(!report.contains("private-synthetic-content"));
    }
    let store = f.store();
    let (failures, reason, at, bytes): (i64, String, String, i64) = store
        .connection()
        .query_row(
            "SELECT failures,last_failure,last_failure_at,summarized_bytes FROM sessions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(failures, 2);
    assert!(reason.contains("输出不合格"));
    assert!(!at.is_empty());
    assert_eq!(bytes, 0);
    assert_eq!(f.records(true)["total"], 0);
    f.fake_codex("", "## 停点\nretry-success");
    f.ok(&["summarize"], ""); // manual retry is allowed even after two failures
    let (failures, reason, at): (i64, Option<String>, Option<String>) = store
        .connection()
        .query_row(
            "SELECT failures,last_failure,last_failure_at FROM sessions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(failures, 0);
    assert!(reason.is_none() && at.is_none());
}

#[test]
fn missing_codex_and_unreadable_transcripts_are_failures_and_do_not_stop_other_sessions() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("missing-file", "synthetic");
    f.session("no-codex", "synthetic");
    fs::remove_file(f.path("missing-file.jsonl")).unwrap();
    let output = f.run(&["summarize"], "");
    assert!(!output.status.success());
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(
        report.contains("claude:missing-file") && report.contains("会话记录"),
        "{report}"
    );
    assert!(
        report.contains("claude:no-codex") && report.contains("codex 不在 PATH"),
        "{report}"
    );
    let count: i64 = f
        .store()
        .connection()
        .query_row("SELECT COUNT(*) FROM sessions WHERE failures=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 2);
    f.fake_codex("", "## 停点\nhealthy-session");
    let output = f.run(&["summarize"], "");
    assert!(!output.status.success());
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("claude:no-codex：写成了"), "{report}");
    assert_eq!(f.records(false)["total"], 1);
}

#[test]
fn a_fresh_waiter_prevents_generation_and_a_stale_waiter_can_be_replaced() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "synthetic");
    f.fake_codex("", "## 停点\nclaimed");
    let store = f.store();
    store
        .connection()
        .execute(
            "UPDATE sessions SET waiter_id='other',waiter_beat_at=?1",
            [cairn::save::now()],
        )
        .unwrap();
    let report = f.ok(&["summarize"], "");
    assert!(report.contains("占用"), "{report}");
    assert!(!f.path("cwd").exists());
    let (waiter, failures): (String, i64) = store
        .connection()
        .query_row("SELECT waiter_id,failures FROM sessions", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(waiter, "other");
    assert_eq!(failures, 0);
    store
        .connection()
        .execute(
            "UPDATE sessions SET waiter_beat_at='2020-01-01T00:00:00.000Z'",
            [],
        )
        .unwrap();
    f.ok(&["summarize"], "");
    let waiter: Option<String> = store
        .connection()
        .query_row("SELECT waiter_id FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert!(waiter.is_none());
    assert_eq!(f.records(false)["total"], 1);
}

#[test]
fn status_and_injection_report_missing_failed_and_newer_turns_without_running_codex() {
    let f = Fixture::new();
    let initial: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
    assert_eq!(initial["sidecar"]["codex_available"], false);
    assert_eq!(initial["sidecar"]["unsummarized_sessions"], 0);
    assert!(!f.path("state/cairn/cairn.db").exists());
    f.ok(&["adopt"], "");
    f.session("one", "synthetic");
    let missing = f.ok(&["show"], "");
    assert!(
        missing.contains("还没有摘要") && missing.contains("1 个回合"),
        "{missing}"
    );
    let retry = missing
        .split('`')
        .find(|s| s.ends_with(" summarize"))
        .unwrap();
    let args = shlex::split(retry).unwrap();
    assert_eq!(
        args,
        vec![
            f.path("data with ' quote/cairn/bin/cairn")
                .to_string_lossy()
                .into_owned(),
            "summarize".into()
        ]
    );
    assert!(!f.run(&["summarize"], "").status.success());
    let status: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
    assert_eq!(status["sidecar"]["unsummarized_sessions"], 1);
    assert_eq!(status["sidecar"]["last_failure"]["source_id"], "claude:one");
    assert!(status["sidecar"]["last_failure"]["reason"]
        .as_str()
        .unwrap()
        .contains("codex 不在 PATH"));
    assert!(f.ok(&["status"], "").contains("codex 不在 PATH"));
    let failed = f.ok(&["show"], "");
    assert!(
        failed.contains("最近一次没写成") && failed.contains("codex 不在 PATH"),
        "{failed}"
    );
    f.fake_codex("", "## 停点\nsummary");
    f.ok(&["summarize"], "");
    fs::remove_file(f.path("cwd")).unwrap();
    f.hook("one", "UserPromptSubmit");
    let show = f.ok(&["show"], "");
    assert!(show.contains("摘要之后又有 1 个回合"), "{show}");
    let status: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
    assert_eq!(status["sidecar"]["codex_available"], true);
    assert_eq!(status["sidecar"]["unsummarized_sessions"], 0);
    assert!(status["sidecar"]["last_failure"].is_null());
    assert!(
        !f.path("cwd").exists(),
        "read-only status/show must not run the model"
    );
}

#[test]
fn sidecar_comparison_uses_the_last_turns_facts_and_time() {
    let f = Fixture::new();
    f.git(&["init"]);
    f.git(&["commit", "--allow-empty", "-m", "initial"]);
    f.ok(&["adopt"], "");
    f.session("one", "synthetic");
    let store = f.store();
    let facts: String = store
        .connection()
        .query_row("SELECT facts FROM sessions", [], |r| r.get(0))
        .unwrap();
    let mut facts: Value = serde_json::from_str(&facts).unwrap();
    facts["collected_at"] = "2026-10-12T20:31:00.000Z".into();
    store
        .connection()
        .execute(
            "UPDATE sessions SET facts=?1,facts_at='2026-10-12T20:31:00.000Z'",
            [facts.to_string()],
        )
        .unwrap();
    store
        .connection()
        .execute("UPDATE events SET at='2026-10-12T20:30:00.000Z'", [])
        .unwrap();
    f.git(&["commit", "--allow-empty", "-m", "after-turn"]);
    f.fake_codex("", "## 停点\nsummary");
    f.ok(&["summarize"], "");
    let show = f.ok(&["show"], "");
    assert!(
        show.contains("HEAD 比 10-12 20:31 那次回合结束时多 1 个提交"),
        "{show}"
    );
    assert!(show.contains("现场变化不说明记录叙述过时。"));
    let id = f.records(false)["records"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let record: Value = serde_json::from_str(&f.ok(&["show", &id, "--json"], "")).unwrap();
    assert_eq!(record["facts"], facts);
}

#[test]
fn sidecar_summary_still_reports_session_end_observed_before_generation() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "synthetic");
    f.hook("one", "SessionEnd");
    f.fake_codex("", "## 停点\nsummary");
    f.ok(&["summarize"], "");
    let show = f.ok(&["show"], "");
    assert!(show.contains("已观测到会话结束"), "{show}");
    assert!(!show.contains("未观测到会话结束"));
}

#[test]
fn selection_requires_adoption_current_line_path_and_turns_and_supports_codex() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    for sid in ["no-path", "no-turns", "other-line"] {
        f.session(sid, "excluded-content");
    }
    fs::write(f.path("eligible.jsonl"),format!("{}\n",serde_json::json!({
        "type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"codex-dialogue"}]}
    }))).unwrap();
    f.hook_as("codex", "eligible", "SessionStart");
    f.hook_as("codex", "eligible", "UserPromptSubmit");
    let store = f.store();
    store
        .connection()
        .execute(
            "UPDATE sessions SET transcript_path=NULL WHERE source_id='claude:no-path'",
            [],
        )
        .unwrap();
    store
        .connection()
        .execute(
            "UPDATE sessions SET turns=0 WHERE source_id='claude:no-turns'",
            [],
        )
        .unwrap();
    store
        .connection()
        .execute(
            "UPDATE sessions SET line_path=?1 WHERE source_id='claude:other-line'",
            [f.path("other").to_str()],
        )
        .unwrap();
    f.fake_codex(
        r#"
case "$input" in *'codex-dialogue'*) ;; *) exit 81 ;; esac
case "$input" in *'excluded-content'*) exit 82 ;; esac
"#,
        "## 停点\ncodex-summary",
    );
    let disabled = f
        .command(&["summarize"])
        .env("CAIRN_DISABLE", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(disabled.status.success());
    assert!(!f.path("cwd").exists());
    let report = f.ok(&["summarize"], "");
    assert!(
        report.contains("codex:eligible") && !report.contains("claude:"),
        "{report}"
    );
    assert_eq!(f.records(false)["total"], 1);
    fs::remove_file(f.path("cwd")).unwrap();
    f.ok(&["unadopt"], "");
    assert!(f.ok(&["summarize"], "").contains("没有需要"));
    assert!(!f.path("cwd").exists());
}

#[test]
fn lesson_and_ledger_caps_are_independent_and_bad_ledger_or_syntax_discards_everything() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "synthetic mixed batch");
    let entry = add("state", "state title", "body", "evidence");
    let lesson = add("lesson", "lesson title", "body", "evidence");
    let mut overflow = vec![entry.clone(); 11];
    overflow.push(lesson.clone());
    let invalid = [
        changes(&overflow),
        changes(&[
            lesson.clone(),
            add("open", &"界".repeat(201), "body", "evidence"),
        ]),
        changes(&[lesson.clone(), edit(999, "missing", "body", "evidence")]),
        r#"{"changes":[{"body":"bad\q"}]}"#.into(),
        changes(&[
            entry.clone(),
            lesson.clone(),
            serde_json::json!({"op":"add"}),
        ]),
        changes(&[entry.clone(), lesson.clone(), Value::Null]),
        changes(&[
            entry.clone(),
            lesson.clone(),
            serde_json::json!({"op":"unsupported", "kind":null, "no":null, "title":null, "body":null, "reason":null, "evidence":null}),
        ]),
    ];
    for (n, output) in invalid.iter().enumerate() {
        f.fake_ledger("NOTHING_TO_CONTINUE", output, "");
        assert!(
            !f.run(&["summarize"], "").status.success(),
            "accepted {output}"
        );
        assert_eq!(f.ledger(), serde_json::json!({"version":0,"items":[]}));
        let (bytes, failures, reason): (i64, i64, String) = f
            .store()
            .connection()
            .query_row(
                "SELECT ledger_bytes,ledger_failures,ledger_last_failure FROM sessions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(bytes, 0);
        assert_eq!(failures, n as i64 + 1);
        let expected = [
            "账的变动超过 10 笔",
            "第 2 笔：标题不合格",
            "第 2 笔：编号不存在",
            "不是合格的 JSON",
            "不是合格的 JSON",
            "不是合格的 JSON",
            "第 3 笔：操作不支持",
        ][n];
        assert_eq!(reason, format!("输出不合格：{expected}"));
    }
    f.fake_ledger(
        "NOTHING_TO_CONTINUE",
        changes(&[vec![entry; 10], vec![lesson; 5]].concat()),
        "",
    );
    f.ok(&["summarize"], "");
    let ledger = f.ledger();
    let items = ledger["items"].as_array().unwrap();
    assert_eq!(ledger["version"], 1);
    assert_eq!(items.iter().filter(|i| i["kind"] == "state").count(), 10);
    assert_eq!(items.iter().filter(|i| i["kind"] == "lesson").count(), 5);
    // EDIT and CLOSE of lessons count in the five, separately from ten edits
    // of ordinary entries. All fifteen share a single committed version.
    fs::OpenOptions::new()
        .append(true)
        .open(f.path("one.jsonl"))
        .unwrap()
        .write_all(dialogue("synthetic edits and closes").as_bytes())
        .unwrap();
    let mut output: Vec<_> = (1..=10)
        .map(|n| edit(n, "revised state", "body", "evidence"))
        .collect();
    output.extend((11..=15).map(|n| {
        if n % 2 == 0 {
            close(n, "obsolete", "evidence")
        } else {
            edit(n, "revised lesson", "body", "evidence")
        }
    }));
    f.fake_ledger("NOTHING_TO_CONTINUE", changes(&output), "");
    f.ok(&["summarize"], "");
    assert_eq!(f.ledger()["version"], 2);
    assert_eq!(f.ledger_item("11")["item"]["title"], "revised lesson");
    assert_eq!(f.ledger_item("12")["item"]["status"], "closed");
}

#[test]
fn lesson_only_invalid_group_advances_without_a_ledger_version() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "synthetic lesson overflow");
    f.fake_ledger(
        "NOTHING_TO_CONTINUE",
        changes(&vec![add("lesson", "title", "body", "evidence"); 6]),
        "",
    );
    f.ok(&["summarize"], "");
    assert_eq!(f.ledger(), serde_json::json!({"version":0,"items":[]}));
    assert!(f.ok(&["status"], "").contains("经验没写成：超过 5 笔"));
    let runs: Value = serde_json::from_str(&f.ok(&["ledger", "runs", "--json"], "")).unwrap();
    assert_eq!(runs[0]["outcome"], "no_changes");
    assert_eq!(runs[0]["entries"], 0);
    assert_eq!(runs[0]["lessons"], 0);
    assert_eq!(runs[0]["reason"], "经验没写成：超过 5 笔");
    assert!(runs[0]["ledger_version"].is_null());
    assert!(f.ok(&["summarize"], "").contains("没有需要"));
}

#[test]
fn lesson_invalid_group_is_skipped_with_status_reason_and_ledger_progress() {
    let cases = [
        (
            [
                vec![add("lesson", "extra", "body", "evidence"); 4],
                vec![
                    edit(1, "revised", "revised body", "evidence"),
                    close(2, "merged", "evidence"),
                ],
            ]
            .concat(),
            "超过 5 笔",
        ),
        (
            vec![add("lesson", &"界".repeat(201), "body", "evidence")],
            "第 2 笔：标题不合格",
        ),
        (
            vec![edit(1, "title", &"界".repeat(342), "evidence")],
            "第 2 笔：正文超过 1 KiB",
        ),
        (
            vec![close(2, "reason", &"界".repeat(501))],
            "第 2 笔：依据不合格",
        ),
        (vec![close(2, "", "evidence")], "第 2 笔：理由为空"),
        (
            vec![
                edit(1, "revised", "body", "evidence"),
                close(1, "reason", "evidence"),
            ],
            "第 3 笔：同一条既改又了结",
        ),
        (
            vec![edit(3, "revised", "body", "evidence")],
            "第 2 笔：条目不是有效经验",
        ),
        (
            vec![close(4, "reason", "evidence")],
            "第 2 笔：条目不是有效经验",
        ),
    ];
    for (invalid, reason) in cases {
        let f = Fixture::new();
        f.ok(&["adopt"], "");
        for title in ["first", "second", "closed", "deleted"] {
            f.ok(&["ledger", "add", "lesson", title], "original body");
        }
        f.ok(&["ledger", "close", "3", "obsolete"], "");
        f.ok(&["ledger", "delete", "4", "--yes"], "");
        let before = f.ledger();
        f.session("one", "synthetic lesson failure");
        f.fake_ledger(
            "NOTHING_TO_CONTINUE",
            changes(&[vec![add("state", "survives", "body", "evidence")], invalid].concat()),
            "",
        );
        let output = f.ok(&["summarize"], "");
        assert!(
            output.contains(&format!("经验没写成：{reason}")),
            "{output}"
        );
        let runs: Value = serde_json::from_str(&f.ok(&["ledger", "runs", "--json"], "")).unwrap();
        assert_eq!(runs[0]["outcome"], "changed");
        assert_eq!(runs[0]["entries"], 1);
        assert_eq!(runs[0]["lessons"], 0);
        assert_eq!(runs[0]["reason"], format!("经验没写成：{reason}"));
        let after = f.ledger();
        assert_eq!(after["version"], 7);
        assert_eq!(after["items"][0]["title"], "survives");
        assert_eq!(
            &after["items"].as_array().unwrap()[1..],
            before["items"].as_array().unwrap()
        );
        let (bytes, failures): (i64, i64) = f
            .store()
            .connection()
            .query_row(
                "SELECT ledger_bytes,ledger_failures FROM sessions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            bytes as u64,
            fs::metadata(f.path("one.jsonl")).unwrap().len()
        );
        assert_eq!(failures, 0);
        let status: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
        assert_eq!(
            status["ledger"]["last_failure"]["reason"],
            format!("经验没写成：{reason}")
        );
        assert_eq!(status["ledger"]["last_failure"]["source_id"], "claude:one");
        assert!(status["ledger"]["last_failure"]["at"].is_string());
        assert!(f
            .ok(&["status"], "")
            .contains(&format!("经验没写成：{reason}")));
        assert!(f.ok(&["summarize"], "").contains("没有需要"));
        // The rejected lessons are not retried with the next excerpt, and a
        // successful batch clears the observation without consuming retries.
        fs::OpenOptions::new()
            .append(true)
            .open(f.path("one.jsonl"))
            .unwrap()
            .write_all(dialogue("next excerpt").as_bytes())
            .unwrap();
        f.fake_ledger("NOTHING_TO_CONTINUE", changes(&[]), "");
        f.ok(&["summarize"], "");
        let status: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
        assert!(status["ledger"]["last_failure"].is_null());
        assert_eq!(f.ledger(), after);
    }
}

#[test]
fn lesson_sidecar_records_evidence_history_and_receives_active_lessons_and_instructions() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.ok(&["ledger", "add", "lesson", "原来教训"], "旧正文");
    f.ok(&["ledger", "add", "lesson", "重复教训"], "重复正文");
    f.ok(&["ledger", "add", "lesson", "失效教训"], "已失效");
    f.ok(&["ledger", "close", "3", "条件不再成立"], "");
    f.session("one", "synthetic-lesson-evidence");
    f.fake_ledger(
        "## 停点\nsummary-with-lessons",
        changes(&[add("lesson", "工具教训", "遇到合成条件时先检查版本", "实测之后才知道"), edit(1, "合并的教训", "合并正文", "两条说的是一回事"), close(2, "合并到 #1", "两条说的是一回事")]),
        r#"
case "$input" in *'账里只有四类条目'*'lesson（经验）：做过才知道'*'kind（state、open、convention、lesson 之一）'*'一次最多 10 笔账的变动，另外最多 5 笔经验的变动'*'经验一次最多记 3 条最值得留的'*'拿不准的、只发生一次以后不会再遇到的、只是这个会话里的权宜之计，不记'*'记成 convention，不记成 lesson'*'改其中一条、了结另一条'*'经验被证明不对、或者条件已经不成立，了结它，理由写清'*) ;; *) exit 83 ;; esac
case "$input" in *'当前的账'*'#1 lesson | 原来教训 | 旧正文'*'#2 lesson | 重复教训 | 重复正文'*'会话摘录'*'synthetic-lesson-evidence'*) ;; *) exit 84 ;; esac
case "$input" in *'#3 lesson'*) exit 85 ;; esac
"#,
    );
    f.ok(&["summarize"], "");
    assert_eq!(f.ledger()["version"], 5);
    let added = f.ledger_item("4");
    assert_eq!(added["item"]["kind"], "lesson");
    assert_eq!(added["item"]["created_by"], "claude:one");
    assert_eq!(added["changes"][0]["evidence"], "实测之后才知道");
    let edited = f.ledger_item("1");
    assert_eq!(edited["item"]["body"], "合并正文");
    assert_eq!(edited["changes"][0]["before_title"], "原来教训");
    assert_eq!(edited["changes"][0]["before_body"], "旧正文");
    let closed = f.ledger_item("2");
    assert_eq!(closed["item"]["status"], "closed");
    assert_eq!(closed["changes"][0]["close_reason"], "合并到 #1");
    for item in [added, edited, closed] {
        assert_eq!(item["changes"][0]["by"], "claude:one");
        assert_eq!(item["changes"][0]["ledger_version"], 5);
    }
    assert_eq!(
        f.records(false)["records"][0]["summary"],
        "summary-with-lessons"
    );
}

#[test]
fn ledger_add_edit_close_commit_one_version_with_session_evidence_and_progress() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.ok(&["ledger", "add", "state", "old title"], "old body");
    f.ok(&["ledger", "add", "open", "pending"], "decide later");
    f.session("one", "synthetic-ledger-decision");
    f.fake_ledger(
        "## 停点\nsummary-still-works",
        changes(&[add("convention", "path | rule", "first\nsecond \\ literal", "original words"), edit(1, "new title", "new body", "changed it"), close(2, "decided", "that is settled")]),
        r#"case "$input" in *'当前的账'*'#1'*'state'*'old title'*'old body'*'#2'*'open'*'pending'*'会话摘录'*'user: synthetic-ledger-decision'*) ;; *) exit 83 ;; esac"#,
    );
    f.ok(&["summarize"], "");
    let ledger = f.ledger();
    assert_eq!(ledger["version"], 3);
    assert_eq!(ledger["items"].as_array().unwrap().len(), 3);
    let added = f.ledger_item("3");
    assert_eq!(added["item"]["title"], "path | rule");
    assert_eq!(added["item"]["body"], "first\nsecond \\ literal");
    assert_eq!(added["item"]["created_by"], "claude:one");
    let edited = f.ledger_item("1");
    assert_eq!(edited["item"]["title"], "new title");
    assert_eq!(edited["changes"][0]["before_title"], "old title");
    assert_eq!(edited["changes"][0]["before_body"], "old body");
    let closed = f.ledger_item("2");
    assert_eq!(closed["item"]["status"], "closed");
    assert_eq!(closed["item"]["close_reason"], "decided");
    for (item, evidence) in [
        (&added, "original words"),
        (&edited, "changed it"),
        (&closed, "that is settled"),
    ] {
        assert_eq!(item["changes"][0]["by"], "claude:one");
        assert_eq!(item["changes"][0]["evidence"], evidence);
        assert_eq!(item["changes"][0]["ledger_version"], 3);
    }
    let (bytes, failures, cleared): (i64, i64, bool) = f.store().connection().query_row(
        "SELECT ledger_bytes,ledger_failures,ledger_last_failure IS NULL AND ledger_last_failure_at IS NULL AND waiter_id IS NULL FROM sessions", [],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(
        bytes as u64,
        fs::metadata(f.path("one.jsonl")).unwrap().len()
    );
    assert_eq!(failures, 0);
    assert!(cleared);
    assert_eq!(
        f.records(false)["records"][0]["summary"],
        "summary-still-works"
    );
}

#[test]
fn ledger_runs_after_skipped_or_failed_summary_and_uses_its_own_offset() {
    for (summary, success) in [("NOTHING_TO_CONTINUE", true), ("invalid-summary", false)] {
        let f = Fixture::new();
        f.ok(&["adopt"], "");
        f.ok(&["ledger", "add", "open", "pending"], "");
        f.session("one", "old-ledger-message");
        f.fake_ledger(
            summary,
            changes(&[close(1, "no longer needed", "drop it")]),
            "",
        );
        assert_eq!(f.run(&["summarize"], "").status.success(), success);
        assert_eq!(f.ledger_item("1")["item"]["status"], "closed");
        assert_eq!(f.records(true)["total"], 0);
        let before = f.ledger();
        fs::OpenOptions::new()
            .append(true)
            .open(f.path("one.jsonl"))
            .unwrap()
            .write_all(dialogue("new-ledger-message").as_bytes())
            .unwrap();
        f.fake_ledger(
            "NOTHING_TO_CONTINUE",
            format!(" \n{}\n\t", changes(&[])),
            r#"
case "$input" in *'当前的账'*'（空）'*'会话摘录'*'user: new-ledger-message'*) ;; *) exit 83 ;; esac
case "$input" in *'user: old-ledger-message'*|*'#1 open'*) exit 84 ;; esac
"#,
        );
        f.ok(&["summarize"], "");
        assert_eq!(f.ledger(), before);
        assert_eq!(f.ledger_item("1")["changes"].as_array().unwrap().len(), 2);
        let bytes: i64 = f
            .store()
            .connection()
            .query_row("SELECT ledger_bytes FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            bytes as u64,
            fs::metadata(f.path("one.jsonl")).unwrap().len()
        );
        assert!(f.ok(&["summarize"], "").contains("没有需要"));
    }
}

#[test]
fn ledger_invalid_batches_are_discarded_whole_and_failures_stop_only_automatic_attempts() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.ok(&["ledger", "add", "state", "original"], "original body");
    f.session("one", "private-synthetic-ledger-text");
    f.hook("one", "SessionEnd");
    let before = f.ledger();
    let history = f.ledger_item("1");
    let prefix = add("state", "must not persist", "", "evidence");
    let batch = |items: Vec<Value>| changes(&[vec![prefix.clone()], items].concat());
    let mut missing = add("state", "title", "body", "evidence");
    missing.as_object_mut().unwrap().remove("evidence");
    let mut invalid_no = edit(1, "title", "body", "evidence");
    invalid_no["no"] = serde_json::json!(1.5);
    let invalid = vec![
        batch(vec![add("bogus", "title", "body", "evidence")]),
        batch(vec![edit(999, "title", "body", "evidence")]),
        batch(vec![
            edit(1, "title", "body", "evidence"),
            close(1, "reason", "evidence"),
        ]),
        batch(vec![add("state", &"界".repeat(201), "body", "evidence")]),
        batch(vec![add("state", "title", &"界".repeat(342), "evidence")]),
        batch(vec![add("state", "title", "body", &"界".repeat(501))]),
        batch(vec![add("state", "title", "body", "evidence"); 10]),
        r#"{"changes":[{"body":"bad\q"}]}"#.into(),
        r#"{"changes":["#.into(),
        batch(vec![add("state", "title\nline", "body", "evidence")]),
        batch(vec![add("state", "title", "body", "")]),
        batch(vec![missing]),
        batch(vec![invalid_no]),
        "[]".into(),
        r#"{"changes":null}"#.into(),
        format!("```\n{}\n```", changes(&[prefix])),
    ];
    for (index, invalid) in invalid.iter().enumerate() {
        f.fake_ledger("NOTHING_TO_CONTINUE", invalid, "");
        let result = f.run(&["summarize"], "");
        assert!(!result.status.success(), "accepted {invalid}");
        assert_eq!(f.ledger(), before);
        assert_eq!(f.ledger_item("1"), history);
        let (bytes, failures, reason): (i64, i64, String) = f
            .store()
            .connection()
            .query_row(
                "SELECT ledger_bytes,ledger_failures,ledger_last_failure FROM sessions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(bytes, 0);
        assert_eq!(failures, index as i64 + 1);
        let expected = [
            "第 2 笔：类不合法",
            "第 2 笔：编号不存在",
            "第 3 笔：同一条既改又了结",
            "第 2 笔：标题不合格",
            "第 2 笔：正文超过 1 KiB",
            "第 2 笔：依据不合格",
            "账的变动超过 10 笔",
            "不是合格的 JSON",
            "不是合格的 JSON",
            "第 2 笔：标题不合格",
            "第 2 笔：依据不合格",
            "不是合格的 JSON",
            "第 2 笔：编号不合格",
            "不是合格的 JSON",
            "不是合格的 JSON",
            "不是合格的 JSON",
        ][index];
        assert_eq!(reason, format!("输出不合格：{expected}"));
        let runs: Value = serde_json::from_str(&f.ok(&["ledger", "runs", "--json"], "")).unwrap();
        assert_eq!(runs[0]["outcome"], "failed");
        assert_eq!(runs[0]["reason"], reason);
        assert_eq!(runs[0]["entries"], 0);
        assert!(!runs.to_string().contains("private-synthetic-ledger-text"));
    }
    // Reset only fixture state to exercise exactly two automatic failures.
    f.store()
        .connection()
        .execute("UPDATE sessions SET ledger_failures=0", [])
        .unwrap();
    let db = f.path("state/cairn/cairn.db");
    let args = [
        "__wait",
        "--database",
        db.to_str().unwrap(),
        "--root",
        f.0.path().to_str().unwrap(),
        "--source",
        "claude:one",
    ];
    f.fake_ledger(
        "NOTHING_TO_CONTINUE",
        "invalid",
        "touch \"$SIDECAR_TEST_DIR/ledger-called\"",
    );
    for failures in [1, 2] {
        f.ok(&args, "");
        assert!(f.path("ledger-called").exists());
        fs::remove_file(f.path("ledger-called")).unwrap();
        let actual: i64 = f
            .store()
            .connection()
            .query_row("SELECT ledger_failures FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(actual, failures);
    }
    f.ok(&args, "");
    assert!(!f.path("ledger-called").exists());
    let status: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
    assert_eq!(status["ledger"]["failed_sessions"], 1);
    assert!(f.ok(&["show"], "").contains("输出不合格"));
    f.fake_ledger("NOTHING_TO_CONTINUE", changes(&[]), "");
    f.ok(&["summarize"], "");
    assert_eq!(f.ledger(), before);
    let cleared: bool = f.store().connection().query_row("SELECT ledger_bytes>0 AND ledger_failures=0 AND ledger_last_failure IS NULL AND ledger_last_failure_at IS NULL FROM sessions", [], |r| r.get(0)).unwrap();
    assert!(cleared);
}

struct RunningSidecar(std::process::Child);

impl RunningSidecar {
    fn finish(&mut self) -> Output {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let status = loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "sidecar did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        use std::io::Read;
        self.0
            .stdout
            .take()
            .unwrap()
            .read_to_end(&mut stdout)
            .unwrap();
        self.0
            .stderr
            .take()
            .unwrap()
            .read_to_end(&mut stderr)
            .unwrap();
        Output {
            status,
            stdout,
            stderr,
        }
    }
}

impl Drop for RunningSidecar {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Fixture {
    fn waiter(&self, sid: &str) -> RunningSidecar {
        RunningSidecar(
            self.command(&[
                "__wait",
                "--database",
                self.path("state/cairn/cairn.db").to_str().unwrap(),
                "--root",
                self.0.path().to_str().unwrap(),
                "--source",
                &format!("claude:{sid}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
        )
    }

    fn ready(&self, name: &str) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !self.path(name).exists() {
            assert!(std::time::Instant::now() < deadline, "missing {name}");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn release(&self, name: &str) {
        fs::write(self.path(name), "").unwrap();
    }
}

#[test]
fn simultaneous_ledger_writers_retry_once_and_record_a_second_version_conflict() {
    for conflict_again in [false, true] {
        let f = Fixture::new();
        f.ok(&["adopt"], "");
        for sid in ["a", "b"] {
            f.session(sid, &format!("session-{sid}"));
            // Seed the trigger directly: this test exercises ledger concurrency,
            // not the SessionEnd hook's 100 ms Git deadline.
            f.store()
                .connection()
                .execute(
                    "INSERT INTO events(source_id,kind,at) VALUES (?1,'session_ended',?2)",
                    rusqlite::params![format!("claude:{sid}"), cairn::save::now()],
                )
                .unwrap();
        }
        f.fake_ledger(
            "NOTHING_TO_CONTINUE",
            "",
            r#"
case "$input" in *'user: session-a'*) name=a ;; *) name=b ;; esac
countfile="$SIDECAR_TEST_DIR/$name-count"
n=0; if [ -f "$countfile" ]; then n=$(cat "$countfile"); fi
n=$((n + 1)); printf '%s' "$n" > "$countfile"
if [ "$name-$n" = a-2 ]; then
    case "$input" in *'#1 state | b-1 | committed'*) ;; *) exit 83 ;; esac
fi
touch "$SIDECAR_TEST_DIR/$name-$n-ready"
i=0
while [ ! -e "$SIDECAR_TEST_DIR/$name-$n-release" ]; do
    i=$((i + 1)); [ "$i" -lt 1000 ]; sleep 0.01
done
printf '{"changes":[{"op":"add","kind":"state","no":null,"title":"%s-%s","body":"committed","reason":null,"evidence":"session evidence"}]}\n' "$name" "$n" > "$output"
exit 0
"#,
        );
        let mut a = f.waiter("a");
        f.ready("a-1-ready");
        let mut b = f.waiter("b");
        f.ready("b-1-ready");
        f.release("b-1-release");
        assert!(b.finish().status.success());
        assert_eq!(f.ledger()["version"], 1);
        f.release("a-1-release");
        f.ready("a-2-ready");
        assert_eq!(f.ledger()["items"].as_array().unwrap().len(), 1);
        if conflict_again {
            fs::OpenOptions::new()
                .append(true)
                .open(f.path("b.jsonl"))
                .unwrap()
                .write_all(dialogue("session-b again").as_bytes())
                .unwrap();
            f.hook("b", "UserPromptSubmit");
            f.store()
                .connection()
                .execute(
                    "INSERT INTO events(source_id,kind,at) VALUES ('claude:b','session_ended',?1)",
                    [cairn::save::now()],
                )
                .unwrap();
            let mut b = f.waiter("b");
            f.ready("b-2-ready");
            f.release("b-2-release");
            assert!(b.finish().status.success());
        }
        f.release("a-2-release");
        assert!(a.finish().status.success());
        assert_eq!(fs::read_to_string(f.path("a-count")).unwrap(), "2");
        let runs: Value = serde_json::from_str(&f.ok(&["ledger", "runs", "--json"], "")).unwrap();
        let attempts: Vec<_> = runs
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["source_id"] == "claude:a")
            .collect();
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0]["manual"], false);
        assert_eq!(
            attempts[0]["outcome"],
            if conflict_again { "failed" } else { "changed" }
        );
        let ledger = f.ledger();
        assert_eq!(ledger["version"], 2);
        assert_eq!(ledger["items"].as_array().unwrap().len(), 2);
        assert_eq!(
            f.ledger_item("2")["item"]["title"],
            if conflict_again { "b-2" } else { "a-2" }
        );
        let (bytes, failures, reason): (i64, i64, Option<String>) = f.store().connection().query_row(
            "SELECT ledger_bytes,ledger_failures,ledger_last_failure FROM sessions WHERE source_id='claude:a'", [],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        if conflict_again {
            assert_eq!((bytes, failures), (0, 1));
            assert_eq!(reason.as_deref(), Some("账在记的时候被改了"));
        } else {
            assert_eq!(bytes as u64, fs::metadata(f.path("a.jsonl")).unwrap().len());
            assert_eq!(failures, 0);
            assert!(reason.is_none());
        }
    }
}

#[test]
fn user_ledger_command_invalidates_even_no_changes_and_retry_uses_the_new_ledger() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.ok(&["ledger", "add", "state", "original"], "old body");
    f.session("one", "session-one");
    f.fake_ledger(
        "NOTHING_TO_CONTINUE",
        "",
        r#"
if [ ! -e "$SIDECAR_TEST_DIR/first-ready" ]; then
    case "$input" in *'#1 state | original | old body'*) ;; *) exit 83 ;; esac
    touch "$SIDECAR_TEST_DIR/first-ready"
    i=0
    while [ ! -e "$SIDECAR_TEST_DIR/first-release" ]; do
        i=$((i + 1)); [ "$i" -lt 1000 ]; sleep 0.01
    done
    printf '{"changes":[]}\n' > "$output"
else
    case "$input" in *'#1 state | user-edit | user body'*) ;; *) exit 84 ;; esac
    printf '{"changes":[{"op":"edit","kind":null,"no":1,"title":"updated after user","body":"new body","reason":null,"evidence":"session-one"}]}\n' > "$output"
fi
exit 0
"#,
    );
    let mut child = RunningSidecar(
        f.command(&["summarize"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    f.ready("first-ready");
    f.ok(
        &["ledger", "edit", "1", "--title", "user-edit"],
        "user body",
    );
    f.release("first-release");
    let result = child.finish();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    assert_eq!(f.ledger()["version"], 3);
    let item = f.ledger_item("1");
    assert_eq!(item["item"]["title"], "updated after user");
    assert_eq!(item["changes"][0]["before_title"], "user-edit");
    assert_eq!(item["changes"][0]["before_body"], "user body");
    assert_eq!(item["changes"][0]["by"], "claude:one");
    assert_eq!(item["changes"][1]["by"], "user");
}

#[test]
fn new_turn_resets_ledger_failure_limit_and_waiter_records_again() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "synthetic-first-turn");
    f.fake_ledger("NOTHING_TO_CONTINUE", "invalid", "");
    for _ in 0..2 {
        assert!(!f.run(&["summarize"], "").status.success());
    }
    fs::OpenOptions::new()
        .append(true)
        .open(f.path("one.jsonl"))
        .unwrap()
        .write_all(dialogue("synthetic-next-turn").as_bytes())
        .unwrap();
    f.hook("one", "UserPromptSubmit");
    let failures: i64 = f
        .store()
        .connection()
        .query_row("SELECT ledger_failures FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(failures, 0);
    // Keep the real turn-start reset above, but seed the waiter trigger without
    // depending on the SessionEnd hook's 100 ms Git deadline.
    f.store()
        .connection()
        .execute(
            "INSERT INTO events(source_id,kind,at) VALUES ('claude:one','session_ended',?1)",
            [cairn::save::now()],
        )
        .unwrap();
    f.fake_ledger(
        "NOTHING_TO_CONTINUE",
        changes(&[add("state", "next turn", "", "new decision")]),
        "",
    );
    assert!(f.waiter("one").finish().status.success());
    assert_eq!(f.ledger_item("1")["item"]["title"], "next turn");
}

#[test]
fn ledger_runs_append_outcomes_without_content_and_read_newest_first() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "synthetic private dialogue");
    f.fake_ledger("## 停点\nsynthetic summary", changes(&[]), "");
    f.ok(&["summarize"], "");
    let runs =
        || -> Value { serde_json::from_str(&f.ok(&["ledger", "runs", "--json"], "")).unwrap() };
    let first = runs();
    assert_eq!(first.as_array().unwrap().len(), 1);
    assert_eq!(first[0]["source_id"], "claude:one");
    assert_eq!(first[0]["outcome"], "no_changes");
    assert_eq!(first[0]["manual"], true);
    assert_eq!(first[0]["entries"], 0);
    assert_eq!(first[0]["lessons"], 0);
    assert!(first[0]["ledger_version"].is_null());
    assert_eq!(first[0]["from_bytes"], 0);
    assert_eq!(
        first[0]["to_bytes"],
        fs::metadata(f.path("one.jsonl")).unwrap().len()
    );
    assert_eq!(first[0]["extracted_bytes"], 33);
    assert_eq!(first[0]["omitted_bytes"], 0);
    assert_eq!(first[0]["tools_cut"], 0);
    for key in ["started_at", "finished_at"] {
        chrono::DateTime::parse_from_rfc3339(first[0][key].as_str().unwrap()).unwrap();
    }
    f.ok(&["summarize"], "");
    assert_eq!(runs(), first); // ineligible sessions do not append
    fs::OpenOptions::new()
        .append(true)
        .open(f.path("one.jsonl"))
        .unwrap()
        .write_all(dialogue("another synthetic private message").as_bytes())
        .unwrap();
    f.fake_ledger(
        "## 停点\nnext synthetic summary",
        changes(&[
            add("state", "private title", "private body", "private evidence"),
            add(
                "lesson",
                "private lesson",
                "private body",
                "private evidence",
            ),
        ]),
        "",
    );
    f.ok(&["summarize"], "");
    let second = runs();
    assert_eq!(second.as_array().unwrap().len(), 2);
    assert_eq!(second[1], first[0]);
    assert_eq!(second[0]["outcome"], "changed");
    assert_eq!(second[0]["entries"], 1);
    assert_eq!(second[0]["lessons"], 1);
    assert_eq!(second[0]["ledger_version"], 1);
    assert_eq!(second[0]["from_bytes"], first[0]["to_bytes"]);
    assert!(!second.to_string().contains("private"));
    let limited: Value =
        serde_json::from_str(&f.ok(&["ledger", "runs", "--limit", "1", "--json"], "")).unwrap();
    assert_eq!(limited, serde_json::json!([second[0]]));
    assert!(f.ok(&["ledger", "runs"], "").contains("账 1 笔、经验 1 笔"));
}

#[test]
fn status_reports_unchanged_streak_alongside_fresh_summaries() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "first synthetic dialogue");
    f.fake_ledger("## 停点\nsynthetic summary", changes(&[]), "");
    for _ in 0..3 {
        fs::OpenOptions::new()
            .append(true)
            .open(f.path("one.jsonl"))
            .unwrap()
            .write_all(dialogue("next synthetic dialogue").as_bytes())
            .unwrap();
        f.ok(&["summarize"], "");
    }
    let status: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
    let recent = &status["ledger"]["recent_sessions"];
    assert_eq!(recent.as_array().unwrap().len(), 1);
    assert_eq!(recent[0]["latest"]["source_id"], "claude:one");
    assert_eq!(recent[0]["latest"]["outcome"], "no_changes");
    assert_eq!(recent[0]["unchanged_streak"], 3);
    let summaries = f.records(false);
    assert_eq!(
        recent[0]["summary_at"],
        summaries["records"][0]["created_at"]
    );
    assert!(recent[0]["summary_at"].is_string());
    let text = f.ok(&["status"], "");
    assert!(text.contains("连续 3 次没有变动或没有对话"), "{text}");
    assert!(text.contains(recent[0]["summary_at"].as_str().unwrap()));
    // Metadata-only tails join the unchanged streak without calling the model.
    fs::OpenOptions::new()
        .append(true)
        .open(f.path("one.jsonl"))
        .unwrap()
        .write_all(b"{\"type\":\"metadata\"}\n")
        .unwrap();
    f.fake_ledger("invalid if called", "invalid if called", "exit 91");
    f.ok(&["summarize"], "");
    let status: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
    assert_eq!(
        status["ledger"]["recent_sessions"][0]["latest"]["outcome"],
        "no_dialogue"
    );
    assert_eq!(
        status["ledger"]["recent_sessions"][0]["unchanged_streak"],
        4
    );
    assert_eq!(
        status["ledger"]["recent_sessions"][0]["summary_at"],
        recent[0]["summary_at"]
    );
}

#[test]
fn ledger_runs_record_lease_loss_even_when_the_new_owner_blocks_session_updates() {
    for during_summary in [true, false] {
        let f = Fixture::new();
        f.ok(&["adopt"], "");
        f.session("one", "synthetic lease loss");
        let gate = r#"
touch "$SIDECAR_TEST_DIR/ready"
i=0
while [ ! -e "$SIDECAR_TEST_DIR/release" ]; do
    i=$((i + 1)); [ "$i" -lt 1000 ]; sleep 0.01
done
"#;
        f.fake_ledger(
            "## 停点\nsynthetic summary",
            changes(&[add("state", "never committed", "body", "evidence")]),
            if during_summary { "" } else { gate },
        );
        if during_summary {
            f.fake_codex(gate, "## 停点\nsynthetic summary");
        }
        let mut child = RunningSidecar(
            f.command(&["summarize"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        f.ready("ready");
        f.store().connection().execute("UPDATE sessions SET waiter_id='new-owner',ledger_failures=1,ledger_last_failure='new-owner reason'", []).unwrap();
        f.release("release");
        assert!(!child.finish().status.success());
        let runs: Value = serde_json::from_str(&f.ok(&["ledger", "runs", "--json"], "")).unwrap();
        assert_eq!(runs.as_array().unwrap().len(), 1);
        assert_eq!(runs[0]["outcome"], "lease_lost");
        assert_eq!(
            runs[0]["reason"],
            if during_summary {
                "会话占位已失效：读取会话记录前"
            } else {
                "会话占位已失效：提交记账结果前"
            }
        );
        assert_eq!(runs[0]["entries"], 0);
        assert!(runs[0]["ledger_version"].is_null());
        assert_eq!(runs[0]["to_bytes"].is_null(), during_summary);
        let state: (String, i64, String) = f
            .store()
            .connection()
            .query_row(
                "SELECT waiter_id,ledger_failures,ledger_last_failure FROM sessions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(state, ("new-owner".into(), 1, "new-owner reason".into()));
        assert_eq!(f.ledger()["version"], 0);
        let status: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
        assert_eq!(status["ledger"]["recent_sessions"][0]["latest"], runs[0]);
        assert_eq!(
            status["ledger"]["recent_sessions"][0]["unchanged_streak"],
            0
        );
        assert!(f.ok(&["status"], "").contains("占位丢了"));
    }
}

#[test]
fn ledger_runs_keep_exact_truncation_counts_and_whole_output_diagnostics() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    f.session("one", "x");
    let tools = serde_json::json!({"type":"assistant","message":{"content":[
        {"type":"tool_use","name":"x","id":"t","input":"x".repeat(501)},
        {"type":"tool_result","tool_use_id":"t","content":"x".repeat(501)}
    ]}});
    fs::write(
        f.path("one.jsonl"),
        format!("{}{}\n", dialogue(&"x".repeat(70_000)), tools),
    )
    .unwrap();
    for (output, reason) in [
        (" ".to_owned(), "输出不合格：不是合格的 JSON"),
        ("x".repeat(128 * 1024 + 1), "输出不合格：超过 128 KiB"),
        ("🦀".repeat(32_769), "输出不合格：超过 128 KiB"),
    ] {
        f.fake_ledger("## 停点\nsynthetic summary", &output, "");
        assert!(!f.run(&["summarize"], "").status.success());
        let runs: Value = serde_json::from_str(&f.ok(&["ledger", "runs", "--json"], "")).unwrap();
        assert_eq!(runs[0]["outcome"], "failed");
        assert_eq!(runs[0]["reason"], reason);
        // 70,007 dialogue bytes + 520 bytes per shortened tool line.
        assert_eq!(runs[0]["extracted_bytes"], 71_047);
        // The existing cap reserves 31 bytes for its omission marker.
        assert_eq!(runs[0]["omitted_bytes"], 9_638);
        assert_eq!(runs[0]["tools_cut"], 2);
    }
}

#[test]
fn ledger_runs_and_status_read_older_or_absent_databases_without_writing() {
    let f = Fixture::new();
    assert_eq!(f.ok(&["ledger", "runs", "--json"], "").trim(), "[]");
    assert!(!f.path("state").exists());
    f.ok(&["adopt"], "");
    f.session("one", "synthetic old database");
    f.fake_ledger("## 停点\nsynthetic summary", changes(&[]), "");
    f.ok(&["summarize"], "");
    f.store().connection().execute_batch("DROP TABLE ledger_runs; UPDATE meta SET value='4' WHERE key='schema_version'; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let db = f.path("state/cairn/cairn.db");
    let before = fs::read(&db).unwrap();
    assert_eq!(f.ok(&["ledger", "runs", "--json"], "").trim(), "[]");
    assert_eq!(f.ok(&["ledger", "runs"], "").trim(), "");
    let status: Value = serde_json::from_str(&f.ok(&["status", "--json"], "")).unwrap();
    assert!(status["ledger"].get("recent_sessions").is_none());
    assert!(!f.ok(&["status"], "").contains("最近记账的会话"));
    assert_eq!(fs::read(&db).unwrap(), before);
    let c = rusqlite::Connection::open(&db).unwrap();
    c.execute_batch("UPDATE meta SET value='6' WHERE key='schema_version'")
        .unwrap();
    for args in [vec!["ledger", "runs", "--json"], vec!["status", "--json"]] {
        let output = f.run(&args, "");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("newer than supported version 5"));
    }
}

#[test]
fn ledger_json_preserves_original_evidence_characters() {
    let f = Fixture::new();
    f.ok(&["adopt"], "");
    let evidence = "执行 cat file | grep \"a\\|b\"\n然后检查结果";
    f.session("one", evidence);
    let output = serde_json::json!({"changes": [{
        "op": "add", "kind": "lesson", "no": {"ignored": true},
        "title": "保留命令原话", "body": evidence, "reason": ["ignored"],
        "evidence": evidence
    }]})
    .to_string();
    f.fake_ledger("NOTHING_TO_CONTINUE", &output, "");
    f.ok(&["summarize"], "");
    let item = f.ledger_item("1");
    assert_eq!(item["item"]["body"], evidence);
    assert_eq!(item["changes"][0]["evidence"], evidence);
    let schema_path = fs::read_to_string(f.path("schema-path")).unwrap();
    assert!(!std::path::Path::new(&schema_path).exists());
    let schema: Value =
        serde_json::from_str(&fs::read_to_string(f.path("schema.json")).unwrap()).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    let item_schema = &schema["properties"]["changes"]["items"];
    assert_eq!(item_schema["additionalProperties"], false);
    assert_eq!(
        item_schema["required"],
        serde_json::json!(["op", "kind", "no", "title", "body", "reason", "evidence"])
    );
}

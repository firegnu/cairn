use cairn::store::{BusyTimeout, Store};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

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
ephemeral=0; model=; effort=; output=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --ephemeral) ephemeral=1; shift ;;
        --skip-git-repo-check) shift ;;
        -m) model=$2; shift 2 ;;
        -c) effort=$2; shift 2 ;;
        -o) output=$2; shift 2 ;;
        -) shift ;;
        *) exit 81 ;;
    esac
done
[ "$ephemeral" = 1 ]
input=$(cat)
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

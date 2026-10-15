use cairn::store::{BusyTimeout, Store};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

struct Fixture(tempfile::TempDir);

impl Fixture {
    fn new() -> Self {
        let f = Self(tempfile::tempdir().unwrap());
        fs::create_dir(f.path("project")).unwrap();
        fs::create_dir(f.path("bin")).unwrap();
        fs::write(
            f.path("transcript"),
            "{\"type\":\"user\",\"message\":{\"content\":\"synthetic\"}}\n",
        )
        .unwrap();
        fs::write(
            f.path("bin/codex"),
            r#"#!/bin/sh
set -eu
echo $$ > "$WAITER_TEST_ROOT/model-pid"
/bin/ps -o ppid= -p $$ > "$WAITER_TEST_ROOT/waiter-pid"
cat >/dev/null
while [ ! -e "$WAITER_TEST_ROOT/release" ]; do /bin/sleep 0.01; done
while [ "$1" != -o ]; do shift; done
printf '## 停点\nbackground-summary\n' > "$2"
"#,
        )
        .unwrap();
        fs::set_permissions(f.path("bin/codex"), fs::Permissions::from_mode(0o700)).unwrap();
        f.ok(&["adopt"], "");
        f.hook("old", "SessionStart");
        f.hook("old", "UserPromptSubmit");
        f
    }

    fn path(&self, p: &str) -> PathBuf {
        self.0.path().join(p)
    }
    fn db(&self) -> PathBuf {
        self.path("state/cairn/cairn.db")
    }
    fn store(&self) -> Store {
        Store::open(self.db(), BusyTimeout::UserCommand).unwrap()
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cairn"));
        cmd.env_clear()
            .env("HOME", self.0.path())
            .env("XDG_STATE_HOME", self.path("state"))
            .env("XDG_DATA_HOME", self.path("data"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("CODEX_HOME", self.path("codex-home"))
            .env("CLAUDE_CONFIG_DIR", self.path("claude-home"))
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.path("bin").display()),
            )
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("WAITER_TEST_ROOT", self.0.path())
            .current_dir(self.path("project"))
            .args(args);
        cmd
    }
    fn ok(&self, args: &[&str], input: &str) -> String {
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
        let output = finish(&mut child, Duration::from_secs(3));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
    fn hook(&self, sid: &str, event: &str) -> String {
        self.ok(&["hook", "claude"], &json!({"hook_event_name":event,"session_id":sid,"cwd":self.path("project"),"transcript_path":self.path("transcript")}).to_string())
    }
    fn model_started(&self) -> i32 {
        until(|| {
            fs::read_to_string(self.path("waiter-pid"))
                .is_ok_and(|s| s.trim().parse::<i32>().is_ok())
        });
        fs::read_to_string(self.path("waiter-pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }
    fn release_and_wait(&self, waiter: i32) {
        fs::write(self.path("release"), "").unwrap();
        until(|| unsafe { libc::kill(waiter, 0) } == -1);
        let records: Value = serde_json::from_str(&self.ok(&["list", "--json"], "")).unwrap();
        assert_eq!(records["records"][0]["summary"], "background-summary");
    }
}

fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < deadline, "condition did not become true");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn finish(child: &mut Child, timeout: Duration) -> Output {
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("command did not return within {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    fn drain(mut pipe: impl std::io::Read + std::os::fd::AsRawFd, deadline: Instant) -> Vec<u8> {
        assert_ne!(
            unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
            -1
        );
        let mut bytes = Vec::new();
        loop {
            match pipe.read_to_end(&mut bytes) {
                Ok(_) => return bytes,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "worker kept the hook pipe open");
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("pipe failed: {e}"),
            }
        }
    }
    let stdout = drain(child.stdout.take().unwrap(), deadline);
    let stderr = drain(child.stderr.take().unwrap(), deadline);
    Output {
        status,
        stdout,
        stderr,
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::write(self.path("release"), "");
        for name in ["model-pid", "waiter-pid"] {
            if let Ok(text) = fs::read_to_string(self.path(name)) {
                if let Ok(pid) = text.trim().parse::<i32>() {
                    let deadline = Instant::now() + Duration::from_secs(3);
                    while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    if unsafe { libc::kill(pid, 0) } == 0 {
                        unsafe {
                            libc::kill(pid, libc::SIGKILL);
                        }
                        until(|| unsafe { libc::kill(pid, 0) } == -1);
                    }
                }
            }
        }
    }
}

#[test]
fn hooks_return_without_waiting_and_start_a_detached_worker() {
    for event in ["Stop", "SessionEnd", "SessionStart"] {
        let f = Fixture::new();
        if event == "Stop" {
            f.store().connection().execute("INSERT INTO events(source_id,kind,at) VALUES ('claude:old','session_ended',?1)", [cairn::save::now()]).unwrap();
        }
        let started = Instant::now();
        let text = f.hook(
            if event == "SessionStart" {
                "new"
            } else {
                "old"
            },
            event,
        );
        assert!(
            started.elapsed() < Duration::from_millis(1500),
            "hook waited for the model"
        );
        if event != "SessionStart" {
            assert!(text.is_empty());
        }
        let waiter = f.model_started();
        assert_eq!(unsafe { libc::getsid(waiter) }, waiter);
        assert_eq!(unsafe { libc::getpgid(waiter) }, waiter);
        assert!(!f.ok(&["--help"], "").contains("__wait"));
        f.release_and_wait(waiter);
    }
}

#[test]
fn fresh_lease_reports_that_the_summary_will_arrive_at_the_next_prompt() {
    let f = Fixture::new();
    f.store()
        .connection()
        .execute(
            "UPDATE sessions SET waiter_id='occupied',waiter_beat_at=?1",
            [cairn::save::now()],
        )
        .unwrap();
    let text = f.ok(&["show"], "");
    assert!(
        text.contains("摘要正在写，写好后在下一次发话时补上"),
        "{text}"
    );
    f.store()
        .connection()
        .execute(
            "UPDATE sessions SET waiter_beat_at='2000-01-01T00:00:00.000Z'",
            [],
        )
        .unwrap();
    assert!(!f.ok(&["show"], "").contains("摘要正在写"));
}

#[test]
fn a_second_waiter_process_exits_while_the_first_owns_the_session() {
    let f = Fixture::new();
    f.hook("old", "SessionEnd");
    let waiter = f.model_started();
    let lease = || -> String {
        f.store()
            .connection()
            .query_row("SELECT waiter_id FROM sessions", [], |r| r.get(0))
            .unwrap()
    };
    let owner = lease();
    let started = Instant::now();
    f.ok(
        &[
            "__wait",
            "--database",
            f.db().to_str().unwrap(),
            "--root",
            f.0.path().to_str().unwrap(),
            "--source",
            "claude:old",
        ],
        "",
    );
    assert!(started.elapsed() < Duration::from_millis(1500));
    assert_eq!(lease(), owner);
    assert_eq!(f.model_started(), waiter);
    f.release_and_wait(waiter);
}

#[test]
fn prompts_supplement_only_stopped_sessions_and_each_new_version_only_once() {
    for agent in ["claude", "codex"] {
        let f = Fixture::new();
        let store = f.store();
        store.connection().execute("UPDATE sessions SET waiter_id='occupied',waiter_beat_at=?1,last_event_at='2000-01-01T00:00:00.000Z'", [cairn::save::now()]).unwrap();
        store
            .connection()
            .execute("UPDATE events SET at='2000-01-01T00:00:00.000Z'", [])
            .unwrap();
        let line: String = store
            .connection()
            .query_row("SELECT line_path FROM sessions", [], |r| r.get(0))
            .unwrap();
        let add = |id: &str, source: &str, writer: &str| {
            store.connection().execute("INSERT INTO records(id,project_id,line_path,source_id,kind,body,created_at,written_by) VALUES (?1,1,?2,?3,'checkpoint',?4,?5,?6)", rusqlite::params![id,line,source,format!("## 停点\n{id}"),cairn::save::now(),writer]).unwrap();
        };
        add("old-version", "claude:old", "sidecar");
        let payload = |event: &str| {
            json!({"hook_event_name":event,"session_id":"receiver","cwd":f.path("project")})
                .to_string()
        };
        let start = f.ok(&["hook", agent], &payload("SessionStart"));
        assert!(start.contains("old-version"));
        add("new-version", "claude:old", "sidecar");
        store
            .connection()
            .execute(
                "INSERT INTO supersessions VALUES ('new-version','old-version')",
                [],
            )
            .unwrap();
        for (sid, at) in [
            ("parallel", "2999-01-01T00:00:00.000Z"),
            ("manual", "2000-01-01T00:00:00.000Z"),
        ] {
            let source = format!("claude:{sid}");
            store
                .connection()
                .execute(
                    "INSERT INTO sources VALUES (?1,'claude',?2,'hook',?3,?3)",
                    rusqlite::params![source, sid, at],
                )
                .unwrap();
            store.connection().execute("INSERT INTO sessions(source_id,project_id,line_path,last_event_at) VALUES (?1,1,?2,?3)", rusqlite::params![source,line,at]).unwrap();
            add(
                sid,
                &source,
                if sid == "manual" { "save" } else { "sidecar" },
            );
        }
        add("own-summary", &format!("{agent}:receiver"), "sidecar");
        let output = f.ok(&["hook", agent], &payload("UserPromptSubmit"));
        assert!(
            !output.is_empty(),
            "the late summary must be injected on UserPromptSubmit"
        );
        let value: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            value["hookSpecificOutput"]["hookEventName"],
            "UserPromptSubmit"
        );
        let text = value["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert!(text.starts_with("[cairn] 补上开局时还没写好的摘要："));
        assert!(text.contains("new-version"));
        for absent in [
            "old-version",
            "parallel",
            "manual",
            "own-summary",
            "现场对比",
            "本会话来源",
        ] {
            assert!(!text.contains(absent), "{text}");
        }
        assert!(f
            .ok(&["hook", agent], &payload("UserPromptSubmit"))
            .is_empty());
        let count: i64 = store
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM injections WHERE source_id=?1 AND record_id='new-version'",
                [format!("{agent}:receiver")],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}

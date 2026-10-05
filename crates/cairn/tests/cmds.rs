use cairn::cli::{self, Cli};
use cairn::store::{database_path, BusyTimeout, Store};
use clap::Parser;
use serde_json::Value;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::FromRawFd;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

static ENVIRONMENT: Mutex<()> = Mutex::new(());

struct Fixture {
    root: tempfile::TempDir,
    saved_env: Vec<(OsString, OsString)>,
    _guard: MutexGuard<'static, ()>,
}

fn isolated_key(key: &OsStr) -> bool {
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
    fn run(&self, args: &[&str], body: &str) -> cli::Result<String> {
        self.run_in(&self.cwd(), args, body)
    }
    fn run_in(&self, cwd: &std::path::Path, args: &[&str], body: &str) -> cli::Result<String> {
        cli::run_at(
            Cli::try_parse_from(std::iter::once("cairn").chain(args.iter().copied()))?,
            &mut body.as_bytes(),
            cwd,
            &self.db(),
            self.root.path(),
        )
    }
    fn queued(&self, body: &str) -> String {
        self.run(&["save"], body)
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .into()
    }
    fn show(&self, id: &str) -> Value {
        serde_json::from_str(&self.run(&["show", id, "--json"], "").unwrap()).unwrap()
    }

    fn process(&self, args: &[&str], input: &str, terminal: bool) -> Output {
        let (mut writer, stdin) = if terminal {
            let (mut master, mut slave) = (-1, -1);
            // SAFETY: openpty writes two descriptors; optional configuration is null.
            assert_eq!(
                unsafe {
                    libc::openpty(
                        &mut master,
                        &mut slave,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                },
                0
            );
            // SAFETY: each freshly allocated descriptor gets exactly one owner.
            unsafe {
                (
                    Some(fs::File::from_raw_fd(master)),
                    fs::File::from_raw_fd(slave),
                )
            }
        } else {
            let mut file = tempfile::tempfile().unwrap();
            file.write_all(input.as_bytes()).unwrap();
            file.rewind().unwrap();
            (None, file)
        };
        let mut stdout = tempfile::tempfile().unwrap();
        let mut stderr = tempfile::tempfile().unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "cmds_process_driver", "--nocapture"])
            .env("CAIRN_CMDS_TEST_ROOT", self.root.path())
            .env("CAIRN_CMDS_TEST_ARGS", serde_json::to_string(args).unwrap())
            .current_dir(self.cwd())
            .stdin(Stdio::from(stdin))
            .stdout(stdout.try_clone().unwrap())
            .stderr(stderr.try_clone().unwrap())
            .spawn()
            .unwrap();
        if let Some(writer) = &mut writer {
            writer.write_all(input.as_bytes()).unwrap();
        }
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > Duration::from_secs(10) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("command timeout: {args:?}");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        stdout.seek(SeekFrom::Start(0)).unwrap();
        stderr.seek(SeekFrom::Start(0)).unwrap();
        let mut out = Vec::new();
        let mut err = Vec::new();
        stdout.read_to_end(&mut out).unwrap();
        stderr.read_to_end(&mut err).unwrap();
        Output {
            status,
            stdout: out,
            stderr: err,
        }
    }

    fn git(&self, args: &[&str]) -> String {
        let mut output = tempfile::tempfile().unwrap();
        let mut child = Command::new("git")
            .args([
                "--no-optional-locks",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "user.name=Synthetic",
                "-c",
                "user.email=synthetic@example.invalid",
            ])
            .args(args)
            .current_dir(self.cwd())
            .stdin(Stdio::null())
            .stdout(output.try_clone().unwrap())
            .stderr(output.try_clone().unwrap())
            .spawn()
            .unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > Duration::from_secs(5) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("git fixture timeout: {args:?}");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        output.rewind().unwrap();
        let mut text = String::new();
        output.read_to_string(&mut text).unwrap();
        assert!(status.success(), "{text}");
        text.trim_end().into()
    }
}

// Exercise the same result reporting/exit path as main, with explicit temporary
// paths. Calling run() here would select macOS's real private spool root.
#[test]
fn cmds_process_driver() {
    let Some(root) = std::env::var_os("CAIRN_CMDS_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let args: Vec<String> =
        serde_json::from_str(&std::env::var("CAIRN_CMDS_TEST_ARGS").unwrap()).unwrap();
    let result = Cli::try_parse_from(std::iter::once("cairn".to_owned()).chain(args))
        .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
        .and_then(|cli| {
            cli::run_at(
                cli,
                &mut std::io::stdin().lock(),
                &root.join("project"),
                &database_path(Some(&root), None).unwrap(),
                &root,
            )
        });
    std::process::exit(cli::report(result));
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

#[test]
fn correction_is_appended_with_source_and_time_in_show_and_injection() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let original = "## 停点\n接口已验证\n## 下一步（建议，非授权）\n发布";
    let id = f.queued(original);
    let correction = "## 停点\n更正：接口尚未验证";
    let result = f.run(&["correct", &id], correction);
    assert!(result.is_ok(), "correct must ingest and append: {result:?}");
    let shown = f.show(&id);
    assert_eq!(shown["body"], original);
    assert_eq!(shown["correction"]["body"], correction);
    assert_eq!(shown["correction"]["kind"], "correction");
    assert_eq!(shown["correction"]["target_id"], id);
    assert_eq!(shown["correction"]["association"], "uncertain");
    let source = shown["correction"]["source_id"].as_str().unwrap();
    assert!(source.starts_with("local:"));
    assert!(source[6..].parse::<ulid::Ulid>().is_ok());
    let at = shown["correction"]["created_at"].as_str().unwrap();
    assert_eq!(at.len(), 24);
    assert!(at.ends_with('Z'));
    assert!(chrono::DateTime::parse_from_rfc3339(at).is_ok());
    for text in [
        f.run(&["show", &id], "").unwrap(),
        f.run(&["show"], "").unwrap(),
    ] {
        assert!(text.contains(original), "{text}");
        assert!(text.contains(correction), "{text}");
        assert!(text.contains(source), "{text}");
        assert!(text.contains(at), "{text}");
    }
    f.run(&["correct", &id], "## 停点\n最新更正").unwrap();
    assert_eq!(f.show(&id)["correction"]["body"], "## 停点\n最新更正");
    assert!(!f.run(&["show"], "").unwrap().contains(correction));
}

#[test]
fn correct_rejects_non_checkpoint_targets_without_writing() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let id = f.queued("## 停点\n原记录");
    let correction = f.run(&["correct", &id], "## 停点\n第一条更正").unwrap();
    let retraction = f.run(&["retract", &id], "").unwrap();
    let restore = f.run(&["restore", &id], "").unwrap();
    let before = f.run(&["list", "--all"], "").unwrap();
    let s = f.store();
    let sources_before: i64 = s
        .connection()
        .query_row("SELECT COUNT(*) FROM sources", [], |r| r.get(0))
        .unwrap();
    for (action, kind) in [
        (correction, "correction"),
        (retraction, "retraction"),
        (restore, "restore"),
    ] {
        let target = action.split_whitespace().nth(1).unwrap();
        assert_eq!(f.show(target)["kind"], kind);
        let result = f.process(&["correct", target], "## 停点\n不支持的更正链", false);
        assert!(
            !result.status.success(),
            "correct must reject a {kind} target: {result:?}"
        );
        assert_eq!(
            String::from_utf8(result.stderr).unwrap(),
            "correct 只允许以 checkpoint 为目标\n"
        );
        assert_eq!(f.run(&["list", "--all"], "").unwrap(), before);
        let sources_after: i64 = s
            .connection()
            .query_row("SELECT COUNT(*) FROM sources", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            sources_after, sources_before,
            "rejection must not leave an orphan source"
        );
    }
    f.run(&["correct", &id], "## 停点\n重新更正原记录").unwrap();
    assert_eq!(f.show(&id)["correction"]["body"], "## 停点\n重新更正原记录");
    for args in [vec!["show", &id], vec!["show"], vec!["export"]] {
        let text = f.run(&args, "").unwrap();
        assert!(text.contains("重新更正原记录"));
        assert!(!text.contains("不支持的更正链"));
    }
}

#[test]
fn supersession_committed_after_restore_is_not_cancelled_by_that_restore() {
    use cairn::session::{self, SessionStarted, StartKind};
    use chrono::{Duration as Delta, SecondsFormat, Utc};
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let id = f.queued("## 停点\n旧记录的独有正文");
    let retraction = f.run(&["retract", &id], "").unwrap();
    let retraction = retraction.split_whitespace().nth(1).unwrap();
    let future = (Utc::now() + Delta::hours(1)).to_rfc3339_opts(SecondsFormat::Millis, true);
    let s = f.store();
    s.connection()
        .execute(
            "UPDATE records SET created_at=?2 WHERE id=?1",
            [retraction, &future],
        )
        .unwrap();
    f.run(&["restore", &id], "").unwrap();
    let restored = f.show(&id);
    assert_eq!(restored["retracted"], false);
    assert!(restored["replaced_by"].is_null());

    let event = SessionStarted {
        disabled: false,
        agent: "codex",
        session_id: "synthetic-p2f-recheck",
        cwd: &f.cwd(),
        start_kind: StartKind::Startup,
        now: Utc::now(),
    };
    let injected = session::start(&event, &f.db(), f.root.path())
        .unwrap()
        .unwrap();
    assert!(injected.record_ids.contains(&id));
    let saved = f
        .run(
            &[
                "save",
                "--source",
                "codex:synthetic-p2f-recheck",
                "--supersedes",
                &id,
            ],
            "## 停点\n新取代记录的独有正文",
        )
        .unwrap();
    let replacement = saved.split_whitespace().nth(1).unwrap();
    let shown = f.show(&id); // Collect the replacement through the public command.
    let outcome: String = s
        .connection()
        .query_row(
            "SELECT outcome FROM spool_ops WHERE op_id=?1",
            [replacement],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(outcome, "ingested");
    let linked: bool = s
        .connection()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM supersessions WHERE record_id=?1 AND target_id=?2)",
            [replacement, &id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(linked);
    assert_eq!(
        shown["replaced_by"], replacement,
        "an earlier restore cannot cancel a later committed supersession"
    );
    assert!(!f
        .run(&["list"], "")
        .unwrap()
        .lines()
        .any(|line| line.starts_with(&id)));
    for text in [
        f.run(&["show"], "").unwrap(),
        f.run(&["export"], "").unwrap(),
    ] {
        assert!(!text.contains("旧记录的独有正文"));
        assert!(text.contains("新取代记录的独有正文"));
    }
    let injected = session::start(&event, &f.db(), f.root.path())
        .unwrap()
        .unwrap();
    assert!(!injected.record_ids.contains(&id));
    assert!(injected
        .record_ids
        .iter()
        .any(|record| record == replacement));
    assert!(!injected.text.contains("旧记录的独有正文"));
    f.run(&["restore", &id], "").unwrap();
    assert!(f.show(&id)["replaced_by"].is_null());
}

#[test]
fn restore_orders_after_committed_actions_even_when_wall_clock_is_behind() {
    use chrono::{Duration as Delta, SecondsFormat, Utc};
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let id = f.queued("## 停点\n恢复后可见的原记录");
    let replacement = f.queued("## 停点\n取代记录");
    let retraction = f.run(&["retract", &id], "").unwrap();
    let retraction = retraction.split_whitespace().nth(1).unwrap();
    let s = f.store();
    // Synthetic committed actions ahead of the wall clock, without changing the
    // system clock or waiting for it to catch up. The original history must stay intact.
    let future = Utc::now() + Delta::hours(1);
    let replacement_at = future.to_rfc3339_opts(SecondsFormat::Millis, true);
    let retraction_at =
        (future + Delta::milliseconds(1)).to_rfc3339_opts(SecondsFormat::Millis, true);
    s.connection()
        .execute(
            "INSERT INTO supersessions VALUES (?1,?2)",
            [&replacement, &id],
        )
        .unwrap();
    for (action, at) in [
        (replacement.as_str(), &replacement_at),
        (retraction, &retraction_at),
    ] {
        s.connection()
            .execute("UPDATE records SET created_at=?2 WHERE id=?1", [action, at])
            .unwrap();
    }
    let replacement_before = f.show(&replacement);
    let retraction_before = f.show(retraction);
    let hidden = f.show(&id);
    assert_eq!(hidden["retracted"], true);
    assert_eq!(hidden["replaced_by"], replacement);

    let before_restore = cairn::save::now();
    let restored = f.run(&["restore", &id], "").unwrap();
    let after_restore = cairn::save::now();
    let shown = f.show(&id);
    assert_eq!(
        shown["retracted"], false,
        "successful restore must undo the committed retraction"
    );
    assert!(
        shown["replaced_by"].is_null(),
        "successful restore must undo supersession too"
    );
    let restored_id = restored.split_whitespace().nth(1).unwrap();
    let restored_record = f.show(restored_id);
    let restored_at = restored_record["created_at"].as_str().unwrap();
    assert!(
        restored_at >= before_restore.as_str() && restored_at <= after_restore.as_str(),
        "restore must use the current wall clock, not advance it: {restored_at}"
    );
    assert_eq!(f.show(retraction), retraction_before);
    assert_eq!(f.show(&replacement), replacement_before);
    assert!(f
        .run(&["list"], "")
        .unwrap()
        .lines()
        .any(|line| line.starts_with(&id)));
    assert!(f.run(&["show"], "").unwrap().contains("恢复后可见的原记录"));

    // A subsequent retraction must apply even if its wall-clock time is earlier.
    let later_retraction = f.run(&["retract", &id], "").unwrap();
    let later_id = later_retraction.split_whitespace().nth(1).unwrap();
    s.connection()
        .execute(
            "UPDATE records SET created_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
            [later_id],
        )
        .unwrap();
    assert_eq!(f.show(&id)["retracted"], true);
    f.run(&["restore", &id], "").unwrap();
    assert_eq!(f.show(&id)["retracted"], false);

    // Changing that earlier action's time cannot undo the later restore, even
    // at the end of the timestamp format. There is no remaining action to undo.
    s.connection()
        .execute(
            "UPDATE records SET created_at='9999-12-31T23:59:59.999Z' WHERE id=?1",
            [later_id],
        )
        .unwrap();
    assert_eq!(f.show(&id)["retracted"], false);
    let before = f.run(&["list", "--all"], "").unwrap();
    let sources_before: i64 = s
        .connection()
        .query_row("SELECT COUNT(*) FROM sources", [], |r| r.get(0))
        .unwrap();
    let result = f.process(&["restore", &id], "", false);
    assert!(!result.status.success());
    assert_eq!(
        String::from_utf8(result.stderr).unwrap(),
        "没有可撤销的取代或撤回\n"
    );
    assert_eq!(f.run(&["list", "--all"], "").unwrap(), before);
    let sources_after: i64 = s
        .connection()
        .query_row("SELECT COUNT(*) FROM sources", [], |r| r.get(0))
        .unwrap();
    assert_eq!(sources_after, sources_before);
}

#[test]
fn retracted_record_is_hidden_by_default_and_marked_in_all_and_show() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let id = f.queued("## 停点\n\n撤回前的停点\n第二行\n## 已完成及验证\n保留原正文");
    let listed = f.run(&["list"], "");
    assert!(
        listed.is_ok(),
        "list must collect pending records: {listed:?}"
    );
    let listed = listed.unwrap();
    assert!(listed.contains(&id));
    assert!(listed.contains("撤回前的停点"));
    assert!(!listed.contains("第二行"));
    assert!(listed.contains("checkpoint"));
    assert!(listed.contains("local:"));
    let result = f.run(&["retract", &id], "");
    assert!(result.is_ok(), "retract must append: {result:?}");
    assert!(!f
        .run(&["list"], "")
        .unwrap()
        .lines()
        .any(|line| line.starts_with(&id)));
    let all = f.run(&["list", "--all"], "").unwrap();
    assert!(all
        .lines()
        .any(|line| line.starts_with(&id) && line.contains("被撤回")));
    assert!(all.contains("retraction"));
    let shown = f.show(&id);
    assert_eq!(shown["retracted"], true);
    assert!(shown["body"].as_str().unwrap().contains("保留原正文"));
    assert!(f.run(&["show", &id], "").unwrap().contains("被撤回"));
    assert!(!f.run(&["show"], "").unwrap().contains("撤回前的停点"));
}

#[test]
fn restore_cancels_supersession_and_retraction_but_requires_an_active_action() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let id = f.queued("## 停点\n被取代后恢复");
    f.run(&["list"], "").unwrap();
    let source = f.show(&id)["source_id"].as_str().unwrap().to_owned();
    // A synthetic prior injection authorizes save's existing supersession path.
    f.store()
        .connection()
        .execute(
            "INSERT INTO injections VALUES (?1,?2,'2026-01-01T00:00:00.000Z')",
            [&source, &id],
        )
        .unwrap();
    f.run(
        &["save", "--source", &source, "--supersedes", &id],
        "## 停点\n接手记录",
    )
    .unwrap();
    let replaced = f.show(&id);
    assert!(replaced["replaced_by"].is_string());
    assert!(f.run(&["show", &id], "").unwrap().contains("被取代"));
    let result = f.run(&["restore", &id], "");
    assert!(
        result.is_ok(),
        "restore must cancel supersession: {result:?}"
    );
    assert!(f.show(&id)["replaced_by"].is_null());
    assert!(f
        .run(&["list"], "")
        .unwrap()
        .lines()
        .any(|line| line.starts_with(&id)));

    f.run(&["retract", &id], "").unwrap();
    assert_eq!(f.show(&id)["retracted"], true);
    f.run(&["restore", &id], "").unwrap();
    assert_eq!(f.show(&id)["retracted"], false);
    assert!(f
        .run(&["list"], "")
        .unwrap()
        .lines()
        .any(|line| line.starts_with(&id)));

    let before = f.run(&["list", "--all"], "").unwrap();
    let result = f.run(&["restore", &id], "");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("没有可撤销"));
    assert_eq!(f.run(&["list", "--all"], "").unwrap(), before);
    f.run(&["retract", &id], "").unwrap();
    assert_eq!(
        f.show(&id)["retracted"],
        true,
        "later retractions remain effective"
    );
}

#[test]
fn delete_erases_only_body_and_reports_busy_until_checkpoint_can_finish() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let marker = "SYNTHETIC-CMDS-DELETE-ONLY-7d15f0ce";
    let body = format!("## 停点\n{marker}");
    let id = f.queued(&body);
    let before = f.show(&id);
    let keeper = f.store();
    keeper
        .connection()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let reader = Store::open_read_only(f.db(), BusyTimeout::UserCommand)
        .unwrap()
        .unwrap();
    reader.connection().execute_batch("BEGIN").unwrap();
    let stored: String = reader
        .connection()
        .query_row("SELECT body FROM records WHERE id=?1", [&id], |r| r.get(0))
        .unwrap();
    assert_eq!(stored, body);
    let result = f.process(&["delete", &id, "--yes"], "", false);
    assert!(
        !result.status.success(),
        "a busy checkpoint must not report success"
    );
    assert_eq!(
        String::from_utf8(result.stderr).unwrap(),
        "墓碑已写入、物理清除未完成，请稍后重试同一命令\n"
    );
    let tombstone = f.show(&id);
    assert!(tombstone["body"].is_null());
    assert!(tombstone["facts"].is_null());
    assert!(tombstone["correction"].is_null());
    assert!(tombstone["deleted_at"].is_string());
    for key in [
        "id",
        "project_key",
        "line_path",
        "source_id",
        "kind",
        "created_at",
    ] {
        assert_eq!(tombstone[key], before[key]);
    }
    assert!(!f.run(&["show", &id], "").unwrap().contains(marker));
    assert!(!f.run(&["list"], "").unwrap().contains(&id));
    let all = f.run(&["list", "--all"], "").unwrap();
    assert!(all.contains(&id) && all.contains("已删除"));
    assert!(!all.contains(marker));
    reader.connection().execute_batch("ROLLBACK").unwrap();
    let output = f.run(&["delete", &id, "--yes"], "").unwrap();
    assert!(output.contains(&id) && output.contains("已删除"));
    assert!(!output.contains(marker));
    assert_eq!(f.show(&id)["deleted_at"], tombstone["deleted_at"]);
    for path in [f.db(), f.db().with_extension("db-wal")] {
        let bytes = fs::read(path).unwrap();
        assert!(!bytes
            .windows(marker.len())
            .any(|window| window == marker.as_bytes()));
    }
    assert_eq!(
        fs::metadata(f.db().with_extension("db-wal")).unwrap().len(),
        0
    );
}

#[test]
fn export_keeps_visible_line_history_and_never_overwrites_a_file() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let id = f.queued("## 停点\n第一来源旧记录");
    let source = f.show(&id)["source_id"].as_str().unwrap().to_owned();
    f.run(&["save", "--source", &source], "## 停点\n第一来源新记录")
        .unwrap();
    let second = f.queued("## 停点\n第二来源记录");
    f.run(&["correct", &second], "## 停点\n第二来源更正")
        .unwrap();
    let hidden = f.queued("## 停点\n不应导出的撤回内容");
    f.run(&["retract", &hidden], "").unwrap();
    let s = f.store();
    s.connection().execute(
        "INSERT INTO records(id,project_id,line_path,source_id,kind,body,created_at)
         SELECT 'other-line',project_id,?2,source_id,'checkpoint','## 停点\n其他线内容',created_at FROM records WHERE id=?1",
        rusqlite::params![id, f.root.path().join("other-line").to_str().unwrap()],
    ).unwrap();
    let path = f.root.path().join("handoff.md");
    fs::write(&path, "已有文件，不许覆盖").unwrap();
    let result = f.run(&["export"], "");
    assert!(result.is_ok(), "export must render Markdown: {result:?}");
    let text = result.unwrap();
    assert!(text.starts_with("导出自 cairn，时间"));
    assert!(text.lines().next().unwrap().ends_with("非权威"));
    for expected in [
        "第一来源旧记录",
        "第一来源新记录",
        "第二来源记录",
        "第二来源更正",
        &source,
    ] {
        assert!(text.contains(expected), "{text}");
    }
    assert!(!text.contains("不应导出的撤回内容"));
    assert!(!text.contains("其他线内容"));
    assert!(text.contains("\n## 停点\n"));
    assert!(f.run(&["export", path.to_str().unwrap()], "").is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "已有文件，不许覆盖");
    let link = f.root.path().join("alias.md");
    symlink(&path, &link).unwrap();
    assert!(f.run(&["export", link.to_str().unwrap()], "").is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "已有文件，不许覆盖");
    let fresh = f.root.path().join("new.md");
    f.run(&["export", fresh.to_str().unwrap()], "").unwrap();
    assert_eq!(
        fs::metadata(&fresh).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let saved = fs::read_to_string(fresh).unwrap();
    assert_eq!(
        saved.lines().skip(1).collect::<Vec<_>>(),
        text.lines().skip(1).collect::<Vec<_>>()
    );
}

#[test]
fn process_preserves_markdown_and_delete_requires_yes_or_terminal_confirmation() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let body = "## 停点\n第一行\n\n第二段\n## 已完成及验证\n合成内容";
    let id = f.queued(body);
    for args in [vec!["show", &id], vec!["show"], vec!["export"]] {
        let result = f.process(&args, "", false);
        assert!(result.status.success(), "{result:?}");
        let text = String::from_utf8(result.stdout).unwrap();
        assert!(text.contains(body), "Markdown must retain newlines: {text}");
    }
    let result = f.process(&["delete", &id], "yes\n", false);
    assert!(!result.status.success());
    let error = String::from_utf8(result.stderr).unwrap();
    assert!(error.contains("--yes"));
    assert_eq!(error.lines().count(), 1);
    assert_eq!(f.show(&id)["body"], body);
    let declined = f.process(&["delete", &id], "n\n", true);
    assert!(!declined.status.success());
    assert_eq!(f.show(&id)["body"], body);
    let confirmed = f.process(&["delete", &id], "y\n", true);
    assert!(confirmed.status.success(), "{confirmed:?}");
    assert!(f.show(&id)["body"].is_null());
}

#[test]
fn read_commands_with_no_database_create_neither_database_spool_nor_export() {
    let f = Fixture::new();
    let path = f.root.path().join("absent.md");
    for args in [
        vec!["list"],
        vec!["list", "--line", "--all"],
        vec!["show", "missing"],
        vec!["export"],
        vec!["export", path.to_str().unwrap()],
    ] {
        assert_eq!(f.run(&args, "").unwrap(), "尚无数据");
    }
    assert_eq!(
        f.run(&["show", "missing", "--json"], "").unwrap(),
        "{\"status\":\"no_data\"}"
    );
    assert!(!f.db().parent().unwrap().exists());
    assert!(!f.root.path().join("cairn-spool").exists());
    assert!(!path.exists());
}

#[test]
fn invalid_corrections_and_missing_or_deleted_targets_do_not_append() {
    let f = Fixture::new();
    f.run(&["adopt"], "").unwrap();
    let id = f.queued("## 停点\n原正文");
    let before = f.run(&["list", "--all"], "").unwrap();
    let oversized = format!("## 停点\n{}", "x".repeat(6 * 1024));
    for body in ["", "   ", "没有标题", &oversized] {
        assert!(f.run(&["correct", &id], body).is_err());
        assert_eq!(f.run(&["list", "--all"], "").unwrap(), before);
    }
    let mut bad_utf8 = &b"## \xff"[..];
    assert!(cli::run_at(
        Cli::try_parse_from(["cairn", "correct", &id]).unwrap(),
        &mut bad_utf8,
        &f.cwd(),
        &f.db(),
        f.root.path()
    )
    .is_err());
    for args in [
        vec!["show", "missing"],
        vec!["correct", "missing"],
        vec!["retract", "missing"],
        vec!["restore", "missing"],
        vec!["delete", "missing", "--yes"],
    ] {
        assert_eq!(
            f.run(&args, "## 停点\n正文").unwrap_err().to_string(),
            "记录不存在"
        );
    }
    assert_eq!(f.run(&["list", "--all"], "").unwrap(), before);
    let prefix = "## 停点\n";
    let exact = format!("{prefix}{}", "x".repeat(6 * 1024 - prefix.len()));
    f.run(&["correct", &id], &exact).unwrap();
    assert_eq!(f.show(&id)["correction"]["body"], exact);
    f.run(&["delete", &id, "--yes"], "").unwrap();
    assert!(f.show(&id)["correction"].is_null());
    let before = f.run(&["list", "--all"], "").unwrap();
    for action in ["correct", "retract", "restore"] {
        assert_eq!(
            f.run(&[action, &id], "## 停点\n正文")
                .unwrap_err()
                .to_string(),
            "记录已删除"
        );
    }
    assert_eq!(f.run(&["list", "--all"], "").unwrap(), before);
}

#[test]
fn list_filters_project_and_worktree_while_id_commands_preserve_target_and_facts() {
    let f = Fixture::new();
    f.git(&["init", "-b", "main"]);
    f.git(&["commit", "--allow-empty", "-m", "synthetic"]);
    let head = f.git(&["rev-parse", "HEAD"]);
    let other = f.root.path().join("other");
    f.git(&["worktree", "add", "-b", "side", other.to_str().unwrap()]);
    f.run(&["adopt"], "").unwrap();
    let id = f.queued("## 停点\n主线记录");
    let side = f
        .run_in(&other, &["save"], "## 停点\n另一条线")
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned();
    let foreign = f.root.path().join("foreign");
    fs::create_dir(&foreign).unwrap();
    f.run_in(&foreign, &["adopt"], "").unwrap();
    f.run_in(&foreign, &["save"], "## 停点\n其他项目内容")
        .unwrap();
    let all = f.run(&["list"], "").unwrap();
    assert!(all.contains(&id) && all.contains(&side));
    assert!(!all.contains("其他项目内容"));
    let line = f.run(&["list", "--line"], "").unwrap();
    assert!(line.contains(&id));
    assert!(!line.contains(&side));
    let shown = f.show(&side);
    assert_eq!(shown["branch"], "side");
    assert_eq!(shown["facts"]["head"], head);
    assert!(shown["facts"]["collected_at"]
        .as_str()
        .unwrap()
        .ends_with('Z'));
    let original = f.run(&["show", &side], "").unwrap();
    assert!(original.contains(&head));
    f.run_in(&foreign, &["correct", &side], "## 停点\n跨目录更正")
        .unwrap();
    let corrected = f.show(&side);
    assert_eq!(corrected["correction"]["project_key"], shown["project_key"]);
    assert_eq!(corrected["correction"]["line_path"], shown["line_path"]);
    assert_eq!(corrected["facts"], shown["facts"]);
    assert!(corrected["correction"]["facts"].is_null());
    let exported = f.run_in(&other, &["export"], "").unwrap();
    assert!(exported.contains("另一条线") && exported.contains("跨目录更正"));
    assert!(!exported.contains("主线记录") && !exported.contains("其他项目内容"));
}

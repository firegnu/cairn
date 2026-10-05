use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct Fixture(tempfile::TempDir);
impl Fixture {
    fn new() -> Self {
        let f = Self(tempfile::tempdir().unwrap());
        fs::create_dir(f.path("project")).unwrap();
        f
    }
    fn path(&self, path: &str) -> PathBuf {
        self.0.path().join(path)
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_cairn"));
        c.env_clear()
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .env("HOME", self.0.path())
            .env("XDG_DATA_HOME", self.path("data"))
            .env("XDG_STATE_HOME", self.path("state"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("CODEX_HOME", self.path("codex"))
            .env("CLAUDE_CONFIG_DIR", self.path("claude"))
            .env("TMPDIR", self.0.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .current_dir(self.path("project"))
            .stdin(Stdio::null())
            .args(args);
        c
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn ok(&self, args: &[&str]) -> String {
        let o = self.run(args);
        assert!(
            o.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8(o.stdout).unwrap()
    }
    fn config(&self, agent: &str) -> PathBuf {
        self.path(if agent == "claude" {
            "claude/settings.json"
        } else {
            "codex/hooks.json"
        })
    }
    fn seed(&self, agent: &str, text: &str) {
        let path = self.config(agent);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
    }
}

const OTHER: &str = r#"{ "type" : "command", "command": "echo user-hook", "timeout": 17 }"#;
const SETTING: &str = r#""custom" : { "z":1e2, "a":"\u0061" }"#;
fn original() -> String {
    format!(
        r#"{{ {SETTING}, "hooks": {{ "Stop": [{{ "matcher":"", "hooks":[{OTHER}] }}] }}, "permissions":{{"allow":["Bash(echo:*)"], "deny": ["Read(secret)"]}} }}"#
    )
}
fn value(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn install_preserves_other_text_and_uses_stable_paths() {
    let f = Fixture::new();
    for agent in ["claude", "codex"] {
        f.seed(agent, &original());
        let result = f.ok(&["install", "--agent", agent, "--yes"]);
        let text = fs::read_to_string(f.config(agent)).unwrap();
        assert!(text.contains(OTHER));
        assert!(text.contains(SETTING));
        assert_eq!(
            fs::metadata(f.config(agent)).unwrap().permissions().mode() & 0o777,
            0o640
        );
        let v = value(&f.config(agent));
        let command = format!("{} hook {agent}", f.path("data/cairn/bin/cairn").display());
        for event in ["SessionStart", "UserPromptSubmit", "Stop", "SessionEnd"] {
            let h = &v["hooks"][event].as_array().unwrap().last().unwrap()["hooks"][0];
            assert_eq!(h["command"], command);
            if event == "SessionEnd" {
                assert_eq!(h["timeout"], if agent == "codex" { 3 } else { 1 });
            }
            if agent == "codex" && event == "SessionStart" {
                assert_eq!(h["additionalContextLimit"], 6000);
            }
        }
        if agent == "claude" {
            assert_eq!(
                v["permissions"]["allow"][1],
                format!("Bash({} save:*)", f.path("data/cairn/bin/cairn").display())
            );
        } else {
            assert!(result.contains("/hooks"));
        }
    }
    assert_eq!(
        fs::read_link(f.path("data/cairn/bin/cairn")).unwrap(),
        Path::new(env!("CARGO_BIN_EXE_cairn"))
            .canonicalize()
            .unwrap()
    );
    assert!(!f.path("codex/config.toml").exists());
    assert!(!f.path("state").exists());
}

#[test]
fn repeated_install_is_byte_identical_and_uninstall_preserves_others_and_backups() {
    let f = Fixture::new();
    for agent in ["claude", "codex"] {
        let original = original();
        f.seed(agent, &original);
        f.ok(&["install", "--agent", agent, "--yes"]);
        let installed = fs::read(f.config(agent)).unwrap();
        let backups_before: Vec<_> = fs::read_dir(f.config(agent).parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().contains(".bak-"))
            .collect();
        assert_eq!(backups_before.len(), 1);
        assert_eq!(fs::read_to_string(&backups_before[0]).unwrap(), original);
        fs::remove_file(f.path("data/cairn/bin/cairn")).unwrap();
        std::os::unix::fs::symlink(f.path("old-binary"), f.path("data/cairn/bin/cairn")).unwrap();
        f.ok(&["install", "--agent", agent, "--yes"]);
        assert_eq!(fs::read(f.config(agent)).unwrap(), installed);
        assert_eq!(
            fs::read_link(f.path("data/cairn/bin/cairn")).unwrap(),
            Path::new(env!("CARGO_BIN_EXE_cairn"))
                .canonicalize()
                .unwrap()
        );
        f.ok(&["uninstall", "--agent", agent, "--dry-run"]);
        assert_eq!(fs::read(f.config(agent)).unwrap(), installed);
        assert_eq!(
            fs::read_dir(f.config(agent).parent().unwrap())
                .unwrap()
                .count(),
            2
        );
        f.ok(&["uninstall", "--agent", agent, "--yes"]);
        let text = fs::read_to_string(f.config(agent)).unwrap();
        assert_eq!(
            value(&f.config(agent)),
            serde_json::from_str::<Value>(&original).unwrap()
        );
        assert!(text.contains(OTHER));
        assert!(text.contains(SETTING));
        let backups: Vec<_> = fs::read_dir(f.config(agent).parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().contains(".bak-"))
            .collect();
        assert_eq!(backups.len(), 2);
        assert_eq!(fs::read_to_string(&backups_before[0]).unwrap(), original);
        assert!(backups.iter().any(|p| fs::read(p).unwrap() == installed));
        f.ok(&["uninstall", "--agent", agent, "--yes"]);
        assert_eq!(fs::read_to_string(f.config(agent)).unwrap(), text);
    }
    assert!(f.path("data/cairn/bin/cairn").is_symlink());
}

#[test]
fn preview_and_nonterminal_confirmation_never_write_and_invalid_json_is_rejected() {
    let f = Fixture::new();
    for agent in ["claude", "codex"] {
        for verb in ["install", "uninstall"] {
            let preview = f.ok(&[verb, "--agent", agent, "--dry-run"]);
            assert!(preview.contains(f.config(agent).to_str().unwrap()));
            let o = f.run(&[verb, "--agent", agent]);
            assert!(!o.status.success());
            assert!(String::from_utf8_lossy(&o.stderr).contains("--yes"));
            assert!(!f.config(agent).exists());
            assert!(!f.path("data").exists());
            assert!(!f.path("state").exists());
        }
        f.seed(agent, "{broken");
        assert!(!f
            .run(&["install", "--agent", agent, "--yes"])
            .status
            .success());
        assert_eq!(fs::read_to_string(f.config(agent)).unwrap(), "{broken");
        assert_eq!(
            fs::read_dir(f.config(agent).parent().unwrap())
                .unwrap()
                .count(),
            1
        );
        assert!(!f.path("data").exists());
    }
}

#[test]
fn status_reports_uninstalled_installed_and_broken_link_without_creating_state() {
    let f = Fixture::new();
    let status: Value = serde_json::from_str(&f.ok(&["status", "--json"])).unwrap();
    assert_eq!(status["agents"]["claude"]["installed"], false);
    assert_eq!(status["agents"]["codex"]["installed"], false);
    assert_eq!(status["project"]["status"], "no_data");
    assert_eq!(status["stable_path"]["valid"], false);
    assert!(!f.path("state").exists());
    assert!(!Path::new(status["spool"]["path"].as_str().unwrap()).exists());
    for agent in ["claude", "codex"] {
        f.ok(&["install", "--agent", agent, "--yes"]);
    }
    let status: Value = serde_json::from_str(&f.ok(&["status", "--json"])).unwrap();
    for agent in ["claude", "codex"] {
        assert_eq!(status["agents"][agent]["installed"], true);
        assert_eq!(status["agents"][agent]["commands_use_stable_path"], true);
    }
    assert_eq!(status["stable_path"]["valid"], true);
    assert!(f.ok(&["status"]).contains("/hooks"));
    fs::remove_file(f.path("data/cairn/bin/cairn")).unwrap();
    std::os::unix::fs::symlink(f.path("missing-binary"), f.path("data/cairn/bin/cairn")).unwrap();
    let status: Value = serde_json::from_str(&f.ok(&["status", "--json"])).unwrap();
    assert_eq!(status["stable_path"]["is_symlink"], true);
    assert_eq!(status["stable_path"]["valid"], false);
    assert!(!f.path("state").exists());
}

#[test]
fn status_reports_adoption_and_counts_pending_without_collecting() {
    let f = Fixture::new();
    f.ok(&["adopt"]);
    let status: Value = serde_json::from_str(&f.ok(&["status", "--json"])).unwrap();
    assert_eq!(status["project"]["status"], "adopted");
    f.ok(&["unadopt"]);
    f.ok(&["save", "--nothing-new"]);
    let spool = PathBuf::from(status["spool"]["path"].as_str().unwrap());
    fs::write(spool.join(".synthetic.tmp"), "synthetic partial write").unwrap();
    fs::write(spool.join("ignore.txt"), "not counted").unwrap();
    std::os::unix::fs::symlink(f.path("missing"), spool.join("ignore.json")).unwrap();
    let database_before = fs::read(f.path("state/cairn/cairn.db")).unwrap();
    let status: Value = serde_json::from_str(&f.ok(&["status", "--json"])).unwrap();
    assert_eq!(status["project"]["status"], "not_adopted");
    assert_eq!(status["spool"]["pending_json"], 1);
    assert_eq!(status["spool"]["residual_tmp"], 1);
    assert_eq!(
        fs::read(f.path("state/cairn/cairn.db")).unwrap(),
        database_before
    );
    assert_eq!(fs::read_dir(&spool).unwrap().count(), 4);
    fs::remove_dir_all(spool).unwrap();
}

#[test]
fn existing_own_handlers_are_repaired_without_touching_mixed_user_handlers() {
    let f = Fixture::new();
    let raw = format!(
        r#"{{ {SETTING} , "hooks":{{"Stop":[{{"matcher":"", "hooks":[{{"type":"command","command":"/old/bin/cairn hook claude"}} , {OTHER}]}}]}} }}"#
    );
    f.seed("claude", &raw);
    let before: Value = serde_json::from_str(&f.ok(&["status", "--json"])).unwrap();
    assert_eq!(before["agents"]["claude"]["events"]["Stop"], true);
    assert_eq!(
        before["agents"]["claude"]["commands_use_stable_path"],
        false
    );
    f.ok(&["install", "--agent", "claude", "--yes"]);
    let text = fs::read_to_string(f.config("claude")).unwrap();
    assert!(!text.contains("/old/bin/cairn"));
    assert!(text.contains(OTHER));
    assert!(text.contains(SETTING));
    f.ok(&["uninstall", "--agent", "claude", "--yes"]);
    let v = value(&f.config("claude"));
    assert_eq!(v["hooks"]["Stop"][0]["hooks"].as_array().unwrap().len(), 1);
    assert_eq!(
        v["hooks"]["Stop"][0]["hooks"][0],
        serde_json::from_str::<Value>(OTHER).unwrap()
    );
}

#[test]
fn paths_with_spaces_are_quoted_and_foreign_files_are_not_overwritten() {
    let f = Fixture::new();
    let data = f.path("data with ' quote");
    let out = f
        .command(&["install", "--agent", "claude", "--yes"])
        .env("XDG_DATA_HOME", &data)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = value(&f.config("claude"));
    let command = v["hooks"]["Stop"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert_eq!(
        shlex::split(command).unwrap(),
        vec![
            data.join("cairn/bin/cairn").to_str().unwrap(),
            "hook",
            "claude"
        ]
    );
    let rule = v["permissions"]["allow"][0].as_str().unwrap();
    assert_eq!(
        shlex::split(
            rule.strip_prefix("Bash(")
                .unwrap()
                .strip_suffix(" save:*)")
                .unwrap()
        )
        .unwrap(),
        vec![data.join("cairn/bin/cairn").to_str().unwrap()]
    );
    fs::create_dir_all(f.path("data/cairn/bin")).unwrap();
    fs::write(f.path("data/cairn/bin/cairn"), "user file").unwrap();
    let before = fs::read(f.config("claude")).unwrap();
    assert!(!f
        .run(&["install", "--agent", "claude", "--yes"])
        .status
        .success());
    assert_eq!(fs::read(f.config("claude")).unwrap(), before);
    assert_eq!(
        fs::read_to_string(f.path("data/cairn/bin/cairn")).unwrap(),
        "user file"
    );
    fs::remove_file(f.config("claude")).unwrap();
    fs::write(f.path("user-settings"), "{}").unwrap();
    std::os::unix::fs::symlink(f.path("user-settings"), f.config("claude")).unwrap();
    assert!(!f
        .run(&["uninstall", "--agent", "claude", "--yes"])
        .status
        .success());
    assert!(f.config("claude").is_symlink());
    assert_eq!(fs::read_to_string(f.path("user-settings")).unwrap(), "{}");
}

#[test]
fn uninstall_without_owned_entries_leaves_even_empty_settings_unchanged() {
    let f = Fixture::new();
    let original = "{\"hooks\":{},\"permissions\":{},\"custom\":17}\n";
    f.seed("claude", original);
    f.ok(&["uninstall", "--agent", "claude", "--yes"]);
    assert_eq!(fs::read_to_string(f.config("claude")).unwrap(), original);
    assert_eq!(
        fs::read_dir(f.config("claude").parent().unwrap())
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn review_r1_shell_commands_remain_user_owned() {
    let f = Fixture::new();
    for agent in ["claude", "codex"] {
        let commands = [
            format!("/usr/bin/true;/opt/user/cairn hook {agent}"),
            format!("/usr/bin/true&/opt/user/cairn hook {agent}"),
            format!("/usr/bin/true|/opt/user/cairn hook {agent}"),
            format!("/opt/$USER/cairn hook {agent}"),
            format!("\"/opt/$USER/cairn\" hook {agent}"),
            format!("/opt/`whoami`/cairn hook {agent}"),
            format!("/opt/*/cairn hook {agent}"),
            format!("/opt/user/cairn hook {agent} # user suffix"),
        ];
        let handlers: Vec<_> = commands
            .iter()
            .map(|command| {
                format!(
                    r#"{{ "type":"command", "command":{}, "timeout":17 }}"#,
                    serde_json::to_string(command).unwrap()
                )
            })
            .collect();
        let original = format!(
            r#"{{"hooks":{{"Stop":[{{"hooks":[{}]}}]}},"theme":"dark"}}"#,
            handlers.join(",")
        );
        f.seed(agent, &original);
        let status: Value = serde_json::from_str(&f.ok(&["status", "--json"])).unwrap();
        assert_eq!(status["agents"][agent]["events"]["Stop"], false);
        f.ok(&["uninstall", "--agent", agent, "--yes"]);
        assert_eq!(fs::read_to_string(f.config(agent)).unwrap(), original);
        f.ok(&["install", "--agent", agent, "--yes"]);
        let installed = fs::read_to_string(f.config(agent)).unwrap();
        for handler in &handlers {
            assert!(installed.contains(handler));
        }
        f.ok(&["uninstall", "--agent", agent, "--yes"]);
        let removed = fs::read_to_string(f.config(agent)).unwrap();
        for handler in &handlers {
            assert!(removed.contains(handler));
        }
        assert_eq!(
            serde_json::from_str::<Value>(&removed).unwrap(),
            serde_json::from_str::<Value>(&original).unwrap()
        );
        let quoted_command = format!(r#"'/old path/it'\''s/cairn' hook {agent}"#);
        let quoted = serde_json::json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":quoted_command}]}]},"theme":"dark"});
        f.seed(agent, &quoted.to_string());
        let status: Value = serde_json::from_str(&f.ok(&["status", "--json"])).unwrap();
        assert_eq!(status["agents"][agent]["events"]["Stop"], true);
        f.ok(&["uninstall", "--agent", agent, "--yes"]);
        assert_eq!(value(&f.config(agent)), serde_json::json!({"theme":"dark"}));
    }
}

#[test]
fn review_r2_duplicate_keys_are_rejected_before_any_write() {
    for original in [
        r#"{"theme":"light","hooks":{},"theme":"dark"}"#,
        r#"{"hooks":{},"custom":[{"nested":{"key":1,"key":2}}]}"#,
        r#"{"hooks":{},"custom":{"key":1,"\u006bey":2}}"#,
    ] {
        for agent in ["claude", "codex"] {
            let f = Fixture::new();
            f.seed(agent, original);
            for verb in ["install", "uninstall"] {
                let output = f.run(&[verb, "--agent", agent, "--yes"]);
                assert!(
                    !output.status.success(),
                    "{verb} accepted duplicate keys: {original}"
                );
                assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate object key"));
                assert_eq!(fs::read_to_string(f.config(agent)).unwrap(), original);
                assert_eq!(
                    fs::metadata(f.config(agent)).unwrap().permissions().mode() & 0o777,
                    0o640
                );
                assert_eq!(fs::read_dir(f.0.path()).unwrap().count(), 2);
                assert_eq!(fs::read_dir(f.path("project")).unwrap().count(), 0);
                assert_eq!(
                    fs::read_dir(f.config(agent).parent().unwrap())
                        .unwrap()
                        .count(),
                    1
                );
                assert!(!f.path("data").exists());
                assert!(!f.path("state").exists());
            }
        }
    }
}

#[test]
fn review_r3_round_trip_cleans_empty_hook_and_permission_containers() {
    let f = Fixture::new();
    f.seed(
        "claude",
        r#"{"hooks":{"Stop":[]},"permissions":{"allow":[]},"theme":"dark"}"#,
    );
    f.ok(&["install", "--agent", "claude", "--yes"]);
    f.ok(&["uninstall", "--agent", "claude", "--yes"]);
    assert_eq!(
        value(&f.config("claude")),
        serde_json::json!({"theme":"dark"})
    );
}

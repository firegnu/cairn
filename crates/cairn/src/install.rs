//! Explicit, previewed changes to user hook configuration.
use crate::{cli::Result, hook::Agent};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, IsTerminal, Read, Write};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

pub(crate) const EVENTS: [&str; 4] = ["SessionStart", "UserPromptSubmit", "Stop", "SessionEnd"];

pub(crate) struct Paths {
    pub stable: PathBuf,
    pub claude: PathBuf,
    pub codex: PathBuf,
}
impl Paths {
    pub fn from_env() -> Result<Self> {
        let home = std::env::var_os("HOME")
            .filter(|v| !v.is_empty())
            .ok_or("HOME 未设置")?;
        let home = PathBuf::from(home);
        let dir = |key, fallback| -> Result<PathBuf> {
            let path = std::env::var_os(key)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or(fallback);
            if !path.is_absolute() {
                return Err(format!("{key} 必须是绝对路径").into());
            }
            Ok(path)
        };
        Ok(Self {
            stable: dir("XDG_DATA_HOME", home.join(".local/share"))?.join("cairn/bin/cairn"),
            claude: dir("CLAUDE_CONFIG_DIR", home.join(".claude"))?.join("settings.json"),
            codex: dir("CODEX_HOME", home.join(".codex"))?.join("hooks.json"),
        })
    }
    pub fn config(&self, agent: Agent) -> &Path {
        match agent {
            Agent::Claude => &self.claude,
            Agent::Codex => &self.codex,
        }
    }
    pub fn command(&self, agent: Agent) -> Result<String> {
        Ok(format!(
            "{} hook {}",
            shell_path(&self.stable)?,
            name(agent)
        ))
    }
    pub fn permission(&self) -> Result<String> {
        Ok(format!("Bash({} save:*)", shell_path(&self.stable)?))
    }
}
fn name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    }
}
pub(crate) fn own_command(command: &str, agent: Agent) -> bool {
    shlex::split(command).is_some_and(|words| {
        words.len() == 3
            && words[1] == "hook"
            && words[2] == name(agent)
            && Path::new(&words[0]).is_absolute()
            && Path::new(&words[0])
                .file_name()
                .is_some_and(|n| n == "cairn")
    })
}
fn own_permission(rule: &Value) -> bool {
    rule.as_str()
        .and_then(|s| s.strip_prefix("Bash(")?.strip_suffix(" save:*)"))
        .and_then(shlex::split)
        .is_some_and(|words| {
            words.len() == 1
                && Path::new(&words[0]).is_absolute()
                && Path::new(&words[0])
                    .file_name()
                    .is_some_and(|n| n == "cairn")
        })
}
fn shell_path(path: &Path) -> Result<String> {
    let text = path.to_str().ok_or("命令路径必须是 UTF-8")?;
    // Permission-rule metacharacters cannot be allowed to broaden the save rule.
    if text.contains(['*', '?', '[', ']', '(', ')', '\n', '\r']) {
        return Err("命令路径包含权限规则不支持的字符".into());
    }
    if text
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || b"/_-.".contains(&c))
    {
        Ok(text.into())
    } else {
        Ok(format!("'{}'", text.replace('\'', "'\\''")))
    }
}

pub(crate) fn read_config(path: &Path) -> Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(m) if !m.file_type().is_file() => Err("配置必须是普通文件（不覆盖符号链接）".into()),
        Ok(_) => Ok(Some(fs::read_to_string(path)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn merge(raw: &str, paths: &Paths, agent: Agent, uninstall: bool) -> Result<String> {
    let mut config: Value = serde_json::from_str(raw)?;
    let object = config.as_object_mut().ok_or("配置必须是 JSON 对象")?;
    let command = paths.command(agent)?;
    if !uninstall || object.contains_key("hooks") {
        let hooks = object
            .entry("hooks")
            .or_insert(json!({}))
            .as_object_mut()
            .ok_or("hooks 必须是对象")?;
        let mut removed_any = false;
        for event in EVENTS {
            if uninstall && !hooks.contains_key(event) {
                continue;
            }
            let groups = hooks
                .entry(event)
                .or_insert(json!([]))
                .as_array_mut()
                .ok_or("hook 事件必须是数组")?;
            let mut removed = false;
            for group in groups.iter_mut() {
                let handlers = group
                    .get_mut("hooks")
                    .and_then(Value::as_array_mut)
                    .ok_or("hook 分组必须含 hooks 数组")?;
                let count = handlers.len();
                handlers.retain(|h| {
                    !h.get("command")
                        .and_then(Value::as_str)
                        .is_some_and(|c| own_command(c, agent))
                });
                removed |= count != handlers.len();
                if count != handlers.len() && handlers.is_empty() {
                    *group = Value::Null;
                }
            }
            groups.retain(|g| !g.is_null());
            removed_any |= removed;
            if uninstall {
                if removed && groups.is_empty() {
                    hooks.shift_remove(event);
                }
            } else {
                let mut handler = json!({"type":"command", "command":command});
                if event == "SessionEnd" {
                    handler["timeout"] = json!(if matches!(agent, Agent::Codex) { 3 } else { 1 });
                }
                if event == "SessionStart" && matches!(agent, Agent::Codex) {
                    handler["additionalContextLimit"] = json!(6000);
                }
                groups.push(json!({"hooks":[handler]}));
            }
        }
        if uninstall && removed_any && hooks.is_empty() {
            object.shift_remove("hooks");
        }
    }
    if matches!(agent, Agent::Claude) && (!uninstall || object.contains_key("permissions")) {
        let permissions = object
            .entry("permissions")
            .or_insert(json!({}))
            .as_object_mut()
            .ok_or("permissions 必须是对象")?;
        let mut removed_rule = false;
        if !uninstall || permissions.contains_key("allow") {
            let allow = permissions
                .entry("allow")
                .or_insert(json!([]))
                .as_array_mut()
                .ok_or("permissions.allow 必须是数组")?;
            let rule = Value::String(paths.permission()?);
            let had_rule = allow.iter().any(own_permission);
            removed_rule = had_rule;
            allow.retain(|r| !own_permission(r));
            if !uninstall {
                allow.push(rule);
            }
            if uninstall && had_rule && allow.is_empty() {
                permissions.shift_remove("allow");
            }
        }
        if uninstall && removed_rule && permissions.is_empty() {
            object.shift_remove("permissions");
        }
    }
    crate::install_json::render(raw, &config)
}

pub(crate) fn run(
    agent: Agent,
    dry_run: bool,
    yes: bool,
    uninstall: bool,
    input: &mut impl Read,
) -> Result<String> {
    let paths = Paths::from_env()?;
    let path = paths.config(agent);
    let before = read_config(path)?;
    let after = merge(
        before.as_deref().unwrap_or("{}\n"),
        &paths,
        agent,
        uninstall,
    )?;
    let executable = std::env::current_exe()?.canonicalize()?;
    if !uninstall {
        match fs::symlink_metadata(&paths.stable) {
            Ok(m) if !m.file_type().is_symlink() => return Err("稳定路径已被非软链接占用".into()),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => (),
        }
    }
    let link_changed =
        !uninstall && fs::read_link(&paths.stable).ok().as_ref() != Some(&executable);
    let config_changed = (!uninstall || before.is_some()) && before.as_deref() != Some(&after);
    let preview = format!(
        "配置 {}（{}）：\n{}\n软链接 {} -> {}（{}）\n",
        path.display(),
        if config_changed {
            "将修改"
        } else {
            "无变化"
        },
        after,
        paths.stable.display(),
        executable.display(),
        if link_changed {
            "将更新"
        } else {
            "无变化"
        }
    );
    if dry_run {
        return Ok(preview);
    }
    print!("{preview}");
    std::io::stdout().flush()?;
    if !yes {
        if !std::io::stdin().is_terminal() {
            return Err("stdin 不是终端；请使用 --yes 确认".into());
        }
        eprint!("应用以上修改？[y/N] ");
        std::io::stderr().flush()?;
        let mut answer = String::new();
        std::io::BufReader::new(input.take(128)).read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            return Err("已取消".into());
        }
    }
    if read_config(path)? != before {
        return Err("配置已变化，请重新运行并确认".into());
    }
    let mut result = String::new();
    if config_changed {
        fs::create_dir_all(path.parent().unwrap())?;
        if let Some(before) = &before {
            let backup = backup(path, before)?;
            result.push_str(&format!("备份：{}\n", backup.display()));
        }
    }
    if link_changed {
        let parent = paths.stable.parent().unwrap();
        fs::create_dir_all(parent)?;
        let temporary = tempfile::tempdir_in(parent)?;
        let link = temporary.path().join("cairn");
        symlink(&executable, &link)?;
        fs::rename(link, &paths.stable)?;
    }
    if config_changed {
        write_atomic(path, &after)?;
    }
    result.push_str(if uninstall {
        "卸载完成；保留稳定软链接\n"
    } else {
        "安装完成\n"
    });
    if !uninstall && matches!(agent, Agent::Codex) {
        result.push_str("请到 Codex 的 /hooks 审核并信任 cairn hooks。\n");
    }
    Ok(result)
}

fn backup(path: &Path, contents: &str) -> Result<PathBuf> {
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ");
    let name = path.file_name().unwrap().to_string_lossy();
    let mut file = tempfile::Builder::new()
        .prefix(&format!("{name}.bak-{timestamp}-"))
        .tempfile_in(path.parent().unwrap())?;
    file.write_all(contents.as_bytes())?;
    file.as_file().sync_all()?;
    let (_, backup) = file.keep()?;
    Ok(backup)
}
fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    file.write_all(contents.as_bytes())?;
    let mode = fs::metadata(path)
        .map(|m| m.permissions().mode())
        .unwrap_or(0o600);
    file.as_file()
        .set_permissions(fs::Permissions::from_mode(mode))?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    fs::File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}

//! Read-only installation, adoption and pending-spool observations.
use crate::{
    cli::Result,
    hook::Agent,
    install::{Paths, EVENTS},
};
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn agent_status(paths: &Paths, agent: Agent) -> Result<Value> {
    let raw = crate::install::read_config(paths.config(agent))?;
    let config: Value = serde_json::from_str(raw.as_deref().unwrap_or("{}"))?;
    if !config.is_object() {
        return Err("配置必须是 JSON 对象".into());
    }
    let command = paths.command(agent)?;
    let mut events = serde_json::Map::new();
    let mut commands_use_stable_path = true;
    for event in EVENTS {
        let mut present = false;
        if let Some(groups) = config["hooks"][event].as_array() {
            for group in groups {
                if let Some(handlers) = group["hooks"].as_array() {
                    for handler in handlers {
                        if let Some(c) = handler["command"]
                            .as_str()
                            .filter(|c| crate::install::own_command(c, agent))
                        {
                            present = true;
                            commands_use_stable_path &= c == command;
                        }
                    }
                }
            }
        }
        events.insert(event.into(), present.into());
    }
    let hooks_present = events.values().all(|v| v == true);
    let permission = !matches!(agent, Agent::Claude)
        || config["permissions"]["allow"]
            .as_array()
            .is_some_and(|allow| {
                paths
                    .permission()
                    .is_ok_and(|p| allow.contains(&Value::String(p)))
            });
    Ok(
        json!({"config_path": paths.config(agent), "installed": hooks_present && permission,
        "events": events, "commands_use_stable_path": hooks_present && commands_use_stable_path, "save_permission": permission}),
    )
}

/// (agent, event, at) for this project. A version 1 database has no hook_seen yet
/// and status never writes, so it reports nothing seen.
fn last_seen(
    connection: &rusqlite::Connection,
    project_key: &str,
) -> rusqlite::Result<Vec<(String, String, String)>> {
    let known: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='hook_seen')",
        [],
        |row| row.get(0),
    )?;
    if !known {
        return Ok(Vec::new());
    }
    let mut query = connection.prepare(
        "SELECT h.agent,h.event,h.at FROM hook_seen h JOIN projects p ON p.id=h.project_id
         WHERE p.key=?1",
    )?;
    let rows = query.query_map([project_key], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })?;
    rows.collect()
}

pub(crate) fn run(cwd: &Path, database: &Path, root: &Path, as_json: bool) -> Result<String> {
    let paths = Paths::from_env()?;
    let mut agents = serde_json::Map::new();
    for (name, agent) in [("claude", Agent::Claude), ("codex", Agent::Codex)] {
        agents.insert(
            name.into(),
            agent_status(&paths, agent)
                .unwrap_or_else(|e| json!({"installed":false,"error":e.to_string()})),
        );
    }
    let scope = crate::scope::Git::default().resolve(cwd)?;
    let store =
        crate::store::Store::open_read_only(database, crate::store::BusyTimeout::UserCommand)?;
    let project_key = scope.project_key.to_str().ok_or("项目路径必须是 UTF-8")?;
    let mut seen = Vec::new();
    let project_status = if let Some(store) = store {
        seen = last_seen(store.connection(), project_key)?;
        let adopted = store
            .connection()
            .query_row(
                "SELECT adopted FROM projects WHERE key=?1",
                [project_key],
                |row| row.get::<_, bool>(0),
            )
            .optional()?
            .unwrap_or(false);
        if adopted {
            "adopted"
        } else {
            "not_adopted"
        }
    } else {
        "no_data"
    };
    for (name, agent) in agents.iter_mut() {
        let mut events = serde_json::Map::new();
        for event in EVENTS {
            let at = seen
                .iter()
                .find(|(a, e, _)| a == name && e == event)
                .map(|(_, _, at)| at.as_str());
            events.insert(event.into(), at.into());
        }
        agent["last_seen"] = events.into();
    }
    let spool = crate::spool::Spool::inspect(root, database)?;
    let is_symlink = paths.stable.is_symlink();
    let valid = is_symlink
        && fs::metadata(&paths.stable)
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    let trust = if agents["codex"]["installed"] == true {
        Some("请到 Codex 的 /hooks 确认 cairn hooks 的信任状态；status 不读取信任配置。")
    } else {
        None
    };
    let report = json!({"agents": agents,
        "stable_path": {"path":paths.stable, "is_symlink":is_symlink, "valid":valid, "target":fs::read_link(&paths.stable).ok()},
        "project":{"key":scope.project_key, "database_path":database,"status":project_status},
        "spool":{"path":spool.path,"pending_json":spool.pending_json,"residual_tmp":spool.residual_tmp},
        "codex_trust_reminder":trust});
    if as_json {
        return Ok(serde_json::to_string_pretty(&report)?);
    }
    let mut text = String::new();
    for name in ["claude", "codex"] {
        let a = &report["agents"][name];
        text.push_str(&format!(
            "{name}: {}；命令指向稳定路径：{}\n",
            if a["installed"] == true {
                "已安装"
            } else {
                "未完整安装"
            },
            a["commands_use_stable_path"]
        ));
        if let Some(error) = a["error"].as_str() {
            text.push_str(&format!("  配置错误：{error}\n"));
        }
        let fired: Vec<String> = EVENTS
            .iter()
            .filter_map(|event| Some(format!("{event} {}", a["last_seen"][event].as_str()?)))
            .collect();
        text.push_str(&format!(
            "  本项目最近触发：{}\n",
            if fired.is_empty() {
                "没有记录".into()
            } else {
                fired.join("；")
            }
        ));
    }
    text.push_str(&format!(
        "稳定软链接：{}（{}）\n项目：{}（{}）\n暂存区：{}；待收取 .json：{}；残留 .tmp：{}\n",
        paths.stable.display(),
        if valid {
            "有效"
        } else {
            "无效或不存在"
        },
        scope.project_key.display(),
        match project_status {
            "adopted" => "已采用",
            "not_adopted" => "未采用",
            _ => "尚无数据",
        },
        spool.path.display(),
        spool.pending_json,
        spool.residual_tmp
    ));
    if let Some(trust) = trust {
        text.push_str(trust);
    }
    Ok(text)
}

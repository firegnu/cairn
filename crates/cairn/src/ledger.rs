//! Project ledger: current items and the history of every committed change.

use crate::{
    cli::Result,
    scope::Scope,
    store::{BusyTimeout, Store},
};
use clap::{Subcommand, ValueEnum};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use std::{
    io::{IsTerminal, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

pub(crate) mod context;
pub(crate) mod sidecar;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Kind {
    State,
    Open,
    Convention,
    Lesson,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::State => "state",
            Self::Open => "open",
            Self::Convention => "convention",
            Self::Lesson => "lesson",
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    Log {
        #[arg(long)]
        limit: Option<usize>,
    },
    Add {
        #[arg(value_enum)]
        kind: Kind,
        title: String,
    },
    Edit {
        no: String,
        #[arg(long)]
        title: Option<String>,
    },
    Close {
        no: String,
        reason: String,
    },
    Reopen {
        no: String,
    },
    Delete {
        no: String,
        #[arg(long)]
        yes: bool,
    },
    Export {
        #[arg(long)]
        all: bool,
        path: Option<PathBuf>,
    },
}

#[derive(Debug, Serialize)]
pub(crate) struct Item {
    pub id: String,
    pub no: i64,
    pub kind: String,
    pub title: String,
    pub body: Option<String>,
    pub status: String,
    pub created_at: String,
    pub created_by: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
    pub close_reason: Option<String>,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct Ledger {
    pub version: i64,
    pub items: Vec<Item>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Failure {
    pub source_id: String,
    pub at: Option<String>,
    pub reason: Option<String>,
    #[serde(skip)]
    pub line_path: String,
    #[serde(skip)]
    pub partial: bool,
}

/// Read observations only. The sidecar task will own resetting/writing them.
pub(crate) fn failures(c: &Connection, key: &str) -> Result<Vec<Failure>> {
    if !available(c)? {
        return Ok(vec![]);
    }
    let mut query = c.prepare(
        "SELECT s.source_id,s.ledger_last_failure_at,s.ledger_last_failure,s.line_path
        FROM sessions s JOIN projects p ON p.id=s.project_id WHERE p.key=?1 AND (s.ledger_failures>0 OR s.ledger_last_failure LIKE '经验没写成：%')
        ORDER BY s.ledger_last_failure_at DESC,s.source_id",
    )?;
    let rows = query.query_map([key], |r| {
        let reason: Option<String> = r.get(2)?;
        let partial = reason
            .as_deref()
            .is_some_and(|s| s.starts_with("经验没写成："));
        Ok(Failure {
            source_id: r.get(0)?,
            at: r.get(1)?,
            reason,
            line_path: r.get(3)?,
            partial,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub(crate) fn available(c: &Connection) -> rusqlite::Result<bool> {
    c.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='ledgers')",
        [],
        |r| r.get(0),
    )
}

pub(crate) fn read(c: &Connection, key: &str, all: bool) -> Result<Ledger> {
    if !available(c)? {
        return Ok(Ledger::default());
    }
    let version = c
        .query_row(
            "SELECT l.version FROM ledgers l JOIN projects p ON p.id=l.project_id WHERE p.key=?1",
            [key],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    let mut query = c.prepare("SELECT i.id,i.no,i.kind,i.title,i.body,i.status,i.created_at,i.created_by,i.updated_at,i.closed_at,i.close_reason,i.deleted_at
        FROM items i JOIN projects p ON p.id=i.project_id WHERE p.key=?1 AND (?2 OR (i.status='active' AND i.deleted_at IS NULL))
        ORDER BY CASE i.kind WHEN 'state' THEN 0 WHEN 'open' THEN 1 WHEN 'convention' THEN 2 WHEN 'lesson' THEN 3 ELSE 4 END,i.no")?;
    let items = query
        .query_map(params![key, all], |r| {
            Ok(Item {
                id: r.get(0)?,
                no: r.get(1)?,
                kind: r.get(2)?,
                title: r.get(3)?,
                body: r.get(4)?,
                status: r.get(5)?,
                created_at: r.get(6)?,
                created_by: r.get(7)?,
                updated_at: r.get(8)?,
                closed_at: r.get(9)?,
                close_reason: r.get(10)?,
                deleted_at: r.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Ledger { version, items })
}

fn target<'a>(ledger: &'a Ledger, id: &str) -> Result<&'a Item> {
    let no = id.strip_prefix('#').unwrap_or(id).parse::<i64>().ok();
    ledger
        .items
        .iter()
        .find(|i| Some(i.no) == no || i.id == id)
        .ok_or_else(|| "条目不存在".into())
}

#[derive(Debug, Serialize)]
pub(crate) struct Change {
    pub id: i64,
    pub item_id: String,
    pub action: String,
    pub by: String,
    pub evidence: Option<String>,
    pub before_title: Option<String>,
    pub before_body: Option<String>,
    pub close_reason: Option<String>,
    pub ledger_version: i64,
    pub at: String,
    pub no: i64,
    pub title: String,
    pub body: Option<String>,
    #[serde(skip)]
    pub kind: String,
}

/// Newest first. Walk edits backwards to reconstruct the content at each change;
/// deleted items have no historical content to reconstruct.
pub(crate) fn changes(c: &Connection, key: &str, since: i64) -> Result<Vec<Change>> {
    if !available(c)? {
        return Ok(vec![]);
    }
    let mut query = c.prepare("SELECT h.id,h.item_id,h.action,h.by,h.evidence,h.before_title,h.before_body,h.close_reason,h.ledger_version,h.at,i.no,i.title,i.body,i.kind
        FROM item_changes h JOIN items i ON i.id=h.item_id JOIN projects p ON p.id=i.project_id
        WHERE p.key=?1 AND h.ledger_version>?2 ORDER BY h.id DESC")?;
    let mut changes = query
        .query_map(params![key, since], |r| {
            Ok(Change {
                id: r.get(0)?,
                item_id: r.get(1)?,
                action: r.get(2)?,
                by: r.get(3)?,
                evidence: r.get(4)?,
                before_title: r.get(5)?,
                before_body: r.get(6)?,
                close_reason: r.get(7)?,
                ledger_version: r.get(8)?,
                at: r.get(9)?,
                no: r.get(10)?,
                title: r.get(11)?,
                body: r.get(12)?,
                kind: r.get(13)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut contents = std::collections::HashMap::new();
    for change in &mut changes {
        let content = contents
            .entry(change.item_id.clone())
            .or_insert_with(|| (change.title.clone(), change.body.clone()));
        change.title.clone_from(&content.0);
        change.body.clone_from(&content.1);
        if let Some(title) = &change.before_title {
            if change.action == "edit" {
                *content = (title.clone(), change.before_body.clone());
            }
        }
    }
    Ok(changes)
}

pub(crate) fn kind_label(kind: &str) -> &str {
    match kind {
        "state" => "现状",
        "open" => "悬而未决",
        "convention" => "约定",
        "lesson" => "经验",
        _ => kind,
    }
}

fn item_text(item: &Item) -> String {
    format!(
        "#{} {}\n{}\n状态：{}；来源：{}；修改时间：{}\n{}",
        item.no,
        item.title,
        item.body.as_deref().unwrap_or(""),
        if item.deleted_at.is_some() {
            "已删除"
        } else if item.status == "closed" {
            "已了结"
        } else {
            "有效"
        },
        item.created_by,
        item.updated_at,
        item.close_reason
            .as_ref()
            .map(|r| format!("了结理由：{r}\n"))
            .unwrap_or_default()
    )
}

fn grouped(ledger: &Ledger) -> String {
    let mut text = String::from("# 项目的账\n");
    let mut kind = "";
    for item in &ledger.items {
        if kind != item.kind {
            kind = &item.kind;
            text.push_str(&format!("\n## {}\n", kind_label(kind)));
        }
        text.push_str(&item_text(item));
    }
    text
}

fn change_text(change: &Change) -> String {
    format!(
        "{} · {} #{} {} · {} · 版本 {}\n{}{}{}",
        change.at,
        change.action,
        change.no,
        change.title,
        change.by,
        change.ledger_version,
        change
            .body
            .as_ref()
            .map(|s| format!("{s}\n"))
            .unwrap_or_default(),
        change
            .close_reason
            .as_ref()
            .map(|s| format!("了结理由：{s}\n"))
            .unwrap_or_default(),
        change
            .evidence
            .as_ref()
            .map(|s| format!("依据：{s}\n"))
            .unwrap_or_default()
    )
}

fn read_output(
    c: Option<&Connection>,
    key: &str,
    all: bool,
    json: bool,
    command: Option<Command>,
    cwd: &Path,
) -> Result<String> {
    let ledger = c
        .map(|c| read(c, key, true))
        .transpose()?
        .unwrap_or_default();
    match command {
        None => {
            let filtered = Ledger {
                version: ledger.version,
                items: ledger
                    .items
                    .into_iter()
                    .filter(|i| all || (i.status == "active" && i.deleted_at.is_none()))
                    .collect(),
            };
            if json {
                Ok(serde_json::to_string(&filtered)?)
            } else {
                Ok(grouped(&filtered))
            }
        }
        Some(Command::Show { id, json }) => {
            let item = target(&ledger, &id)?;
            let history: Vec<_> = changes(c.unwrap(), key, 0)?
                .into_iter()
                .filter(|h| h.item_id == item.id)
                .collect();
            if json {
                Ok(serde_json::to_string(
                    &serde_json::json!({"item":item,"changes":history}),
                )?)
            } else {
                Ok(format!(
                    "{}\n历史（新的在前）：\n{}",
                    item_text(item),
                    history.iter().map(change_text).collect::<String>()
                ))
            }
        }
        Some(Command::Log { limit }) => Ok(c
            .map(|c| changes(c, key, 0))
            .transpose()?
            .unwrap_or_default()
            .iter()
            .take(limit.unwrap_or(usize::MAX))
            .map(change_text)
            .collect()),
        Some(Command::Export { all, path }) => {
            let ledger = Ledger {
                version: ledger.version,
                items: ledger
                    .items
                    .into_iter()
                    .filter(|i| all || (i.status == "active" && i.deleted_at.is_none()))
                    .collect(),
            };
            let output = grouped(&ledger);
            if let Some(path) = path {
                let path = cwd.join(path);
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&path)?;
                file.write_all(output.as_bytes())?;
                file.sync_all()?;
                Ok(format!("exported {}", path.display()))
            } else {
                Ok(output)
            }
        }
        _ => unreachable!("writes are dispatched separately"),
    }
}

pub(crate) fn run(
    database: &Path,
    scope: &Scope,
    cwd: &Path,
    input: &mut impl Read,
    all: bool,
    json: bool,
    command: Option<Command>,
) -> Result<String> {
    let key = scope.project_key.to_str().ok_or("项目路径不是 UTF-8")?;
    let reader = Store::open_read_only(database, BusyTimeout::UserCommand)?;
    if matches!(
        command,
        None | Some(Command::Show { .. } | Command::Log { .. } | Command::Export { .. })
    ) {
        if let Some(mut reader) = reader {
            let tx = reader.transaction(TransactionBehavior::Deferred)?;
            let output = read_output(Some(&tx), key, all, json, command, cwd)?;
            tx.commit()?;
            return Ok(output);
        }
        return read_output(None, key, all, json, command, cwd);
    }
    let adopted = |c: &Connection| {
        c.query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE key=?1 AND adopted=1)",
            [key],
            |r| r.get::<_, bool>(0),
        )
    };
    if !reader
        .as_ref()
        .map(|s| adopted(s.connection()))
        .transpose()?
        .unwrap_or(false)
    {
        return Err("项目尚未采用 cairn".into());
    }
    drop(reader);
    let command = command.unwrap();
    if let Command::Delete { no, yes: false } = &command {
        if !std::io::stdin().is_terminal() {
            return Err("stdin 不是终端；请使用 --yes 确认删除".into());
        }
        eprint!("删除条目 {no} 的内容和历史内容？[y/N] ");
        std::io::stderr().flush()?;
        let mut answer = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(input.take(128)), &mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            return Err("已取消删除".into());
        }
    }
    let body = if matches!(command, Command::Add { .. } | Command::Edit { .. }) {
        let mut body = String::new();
        if !matches!(command, Command::Edit { title: Some(_), .. })
            || !std::io::stdin().is_terminal()
        {
            input.read_to_string(&mut body)?;
        }
        if body.is_empty() {
            None
        } else {
            Some(body)
        }
    } else {
        None
    };
    let mut store = Store::open(database, BusyTimeout::UserCommand)?;
    let tx = store.transaction(TransactionBehavior::Immediate)?;
    if !adopted(&tx)? {
        return Err("项目尚未采用 cairn".into());
    }
    let project: i64 = tx.query_row("SELECT id FROM projects WHERE key=?1", [key], |r| r.get(0))?;
    let ledger = read(&tx, key, true)?;
    let at = crate::save::now();
    let version = ledger.version + 1;
    let (id, no, action, before_title, before_body, reason) = match command {
        Command::Add { kind, title } => {
            let no = ledger.items.iter().map(|i| i.no).max().unwrap_or(0) + 1;
            let id = ulid::Ulid::new().to_string();
            tx.execute("INSERT INTO items(id,project_id,no,kind,title,body,status,created_at,created_by,updated_at) VALUES (?1,?2,?3,?4,?5,?6,'active',?7,'user',?7)",params![id,project,no,kind.name(),title,body,at])?;
            (id, no, "add", None, None, None)
        }
        Command::Edit { ref no, .. }
        | Command::Close { ref no, .. }
        | Command::Reopen { ref no }
        | Command::Delete { ref no, .. } => {
            let item = target(&ledger, no)?;
            if item.deleted_at.is_some() {
                if matches!(command, Command::Delete { .. }) {
                    tx.commit()?;
                    store.truncate_wal()?;
                    return Ok(format!(
                        "delete #{} {} · 版本 {}",
                        item.no, item.id, ledger.version
                    ));
                }
                return Err("条目已删除".into());
            }
            let (action, reason) = match command {
                Command::Edit { title, .. } => {
                    if title.is_none() && body.is_none() {
                        return Err("请提供标题或正文".into());
                    }
                    tx.execute(
                        "UPDATE items SET title=?2,body=?3,updated_at=?4 WHERE id=?1",
                        params![
                            item.id,
                            title.as_deref().unwrap_or(&item.title),
                            body.as_deref().or(item.body.as_deref()),
                            at
                        ],
                    )?;
                    ("edit", None)
                }
                Command::Close { reason, .. } => {
                    if item.status == "closed" {
                        return Err("条目已了结".into());
                    }
                    tx.execute("UPDATE items SET status='closed',closed_at=?2,closed_by='user',close_reason=?3,updated_at=?2 WHERE id=?1",params![item.id,at,reason])?;
                    ("close", Some(reason))
                }
                Command::Reopen { .. } => {
                    if item.status != "closed" {
                        return Err("条目尚未了结".into());
                    }
                    tx.execute("UPDATE items SET status='active',closed_at=NULL,closed_by=NULL,close_reason=NULL,updated_at=?2 WHERE id=?1",params![item.id,at])?;
                    ("reopen", None)
                }
                Command::Delete { .. } => {
                    tx.execute("UPDATE items SET title='（已删除）',body=NULL,close_reason=NULL,deleted_at=?2,updated_at=?2 WHERE id=?1",params![item.id,at])?;
                    tx.execute("UPDATE item_changes SET before_title=NULL,before_body=NULL,evidence=NULL,close_reason=NULL WHERE item_id=?1",[&item.id])?;
                    ("delete", None)
                }
                _ => unreachable!(),
            };
            let before = if action == "delete" {
                (None, None)
            } else {
                (Some(item.title.clone()), item.body.clone())
            };
            (item.id.clone(), item.no, action, before.0, before.1, reason)
        }
        _ => unreachable!(),
    };
    tx.execute("INSERT INTO item_changes(item_id,action,by,before_title,before_body,close_reason,ledger_version,at) VALUES (?1,?2,'user',?3,?4,?5,?6,?7)",params![id,action,before_title,before_body,reason,version,at])?;
    tx.execute("INSERT INTO ledgers(project_id,version) VALUES (?1,?2) ON CONFLICT(project_id) DO UPDATE SET version=excluded.version",params![project,version])?;
    tx.commit()?;
    if action == "delete" {
        store.truncate_wal()?;
    }
    Ok(format!("{action} #{no} {id} · 版本 {version}"))
}

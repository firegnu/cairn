//! User record management. Visibility is shared with injection rendering.

use crate::cli::Result;
use crate::render::{self, Record};
use crate::save::StoredFacts;
use crate::store::Store;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use std::path::Path;

fn target_records(connection: &Connection, id: &str) -> Result<Vec<Record>> {
    let key: String = connection
        .query_row(
            "SELECT p.key FROM records r JOIN projects p ON p.id=r.project_id WHERE r.id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or("记录不存在")?;
    render::records(connection, &key)
}

fn target<'a>(all: &'a [Record], id: &str) -> Result<&'a Record> {
    all.iter()
        .find(|r| r.id == id)
        .ok_or_else(|| "记录不存在".into())
}

fn status(record: &Record) -> Vec<String> {
    let mut states = Vec::new();
    if record.deleted_at.is_some() {
        states.push("已删除".into());
    }
    if let Some(id) = &record.replaced_by {
        states.push(format!("被取代（{id}）"));
    }
    if record.retracted {
        states.push("被撤回".into());
    }
    if states.is_empty() {
        states.push("可见".into());
    }
    states
}

fn details(connection: &Connection, record: &Record) -> Result<Value> {
    let (association, agent, session_id): (String, String, Option<String>) = connection.query_row(
        "SELECT association,agent,session_id FROM sources WHERE id=?1",
        [&record.source],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let project_key: String = connection.query_row(
        "SELECT key FROM projects WHERE id=?1",
        [record.project_id],
        |r| r.get(0),
    )?;
    let deleted = record.deleted_at.is_some();
    let facts = if deleted {
        None
    } else {
        record
            .facts
            .as_deref()
            .map(serde_json::from_str::<StoredFacts>)
            .transpose()?
    };
    Ok(json!({
        "id": record.id, "project_id": record.project_id, "project_key": project_key,
        "line_path": record.line, "branch": record.branch, "source_id": record.source,
        "association": association, "agent": agent, "session_id": session_id,
        "kind": record.kind, "target_id": record.target, "created_at": record.at,
        "deleted_at": record.deleted_at, "replaced_by": record.replaced_by,
        "retracted": record.retracted, "status": status(record),
        "body": if deleted { None } else { record.body.as_deref() }, "facts": facts,
    }))
}

fn record_text(value: &Value) -> Result<String> {
    let field = |key: &str| value[key].as_str().unwrap_or("-");
    let states = value["status"]
        .as_array()
        .unwrap()
        .iter()
        .map(|state| state.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("、");
    let mut text = format!(
        "记录 {}\n时间：{}\n来源：{} · {}\n项目：{}\n工作线：{} · 分支：{}\nkind：{} · target：{}\n状态：{}\n",
        field("id"), field("created_at"), field("source_id"),
        if field("association") == "uncertain" { "关联不确定" } else { field("association") },
        field("project_key"), field("line_path"), field("branch"), field("kind"), field("target_id"), states,
    );
    if !value["deleted_at"].is_null() {
        text.push_str(&format!("删除时间：{}\n", field("deleted_at")));
        return Ok(text);
    }
    if !value["facts"].is_null() {
        text.push_str(&format!(
            "事实：\n```json\n{}\n```\n",
            serde_json::to_string_pretty(&value["facts"])?
        ));
    }
    if let Some(body) = value["body"].as_str() {
        text.push_str(body);
        text.push('\n');
    }
    Ok(text)
}

pub(crate) fn show(connection: &Connection, id: &str, json: bool) -> Result<String> {
    let all = target_records(connection, id)?;
    let record = target(&all, id)?;
    show_record(connection, &all, record, json)
}

fn show_record(
    connection: &Connection,
    all: &[Record],
    record: &Record,
    json: bool,
) -> Result<String> {
    let mut value = details(connection, record)?;
    let correction = if record.deleted_at.is_none() {
        render::latest_correction(all, &record.id)
            .map(|r| details(connection, r))
            .transpose()?
    } else {
        None
    };
    value["correction"] = json!(correction);
    if json {
        Ok(serde_json::to_string(&value)?)
    } else {
        let mut text = record_text(&value)?;
        if let Some(correction) = correction {
            text.push_str("\n最新更正：\n");
            text.push_str(&record_text(&correction)?);
        }
        Ok(text)
    }
}

pub(crate) fn append(
    store: &mut Store,
    id: &str,
    kind: &str,
    body: Option<&str>,
) -> Result<String> {
    let tx = store.transaction(TransactionBehavior::Immediate)?;
    let all = target_records(&tx, id)?;
    let record = target(&all, id)?;
    if record.deleted_at.is_some() {
        return Err("记录已删除".into());
    }
    if kind == "correction" && record.kind != "checkpoint" {
        return Err("correct 只允许以 checkpoint 为目标".into());
    }
    if kind == "restore" && record.replaced_by.is_none() && !record.retracted {
        return Err("没有可撤销的取代或撤回".into());
    }
    let new_id = ulid::Ulid::new().to_string();
    let source = format!("local:{new_id}");
    let at = crate::save::now();
    tx.execute(
        "INSERT INTO sources(id,agent,session_id,association,first_seen,last_seen) VALUES (?1,'local',NULL,'uncertain',?2,?2)",
        params![source, at],
    )?;
    tx.execute(
        "INSERT INTO records(id,project_id,line_path,branch,source_id,kind,target_id,body,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![new_id, record.project_id, record.line, record.branch, source, kind, id, body, at],
    )?;
    tx.commit()?;
    Ok(format!(
        "{kind} {new_id} · target {id} · 来源 {source}（关联不确定） · {at}"
    ))
}

pub(crate) fn list(
    connection: &Connection,
    scope: &crate::scope::Scope,
    line: bool,
    all: bool,
) -> Result<String> {
    let records = render::records(
        connection,
        scope.project_key.to_str().ok_or("项目路径不是 UTF-8")?,
    )?;
    let mut lines = Vec::new();
    for record in records
        .iter()
        .filter(|r| (!line || Path::new(&r.line) == scope.line_path) && (all || r.active()))
    {
        let stopping_point = render::stopping_point(record.body.as_deref().unwrap_or(""));
        let summary = stopping_point
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        lines.push(format!(
            "{} · {} · 来源 {} · {} / {} · {} · {} · {}",
            record.id,
            record.at,
            record.source,
            record.line,
            record.branch.as_deref().unwrap_or("-"),
            record.kind,
            status(record).join("、"),
            summary
        ));
    }
    Ok(if lines.is_empty() {
        "尚无数据".into()
    } else {
        lines.join("\n")
    })
}

pub(crate) fn delete(store: &mut Store, id: &str) -> Result<String> {
    match store.delete_body(id, &crate::save::now()) {
        Ok(true) => show(store.connection(), id, false),
        Ok(false) => Err("记录不存在".into()),
        Err(crate::store::Error::CheckpointBusy) => {
            Err("墓碑已写入、物理清除未完成，请稍后重试同一命令".into())
        }
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn export(connection: &Connection, scope: &crate::scope::Scope) -> Result<String> {
    let all = render::records(
        connection,
        scope.project_key.to_str().ok_or("项目路径不是 UTF-8")?,
    )?;
    let mut text = format!("导出自 cairn，时间 {}，非权威\n", crate::save::now());
    // Export all visible checkpoints, without injection's per-source folding or
    // character budget. Corrections accompany their target rather than duplicating it.
    for record in all
        .iter()
        .filter(|r| r.kind == "checkpoint" && r.visible() && Path::new(&r.line) == scope.line_path)
    {
        text.push_str("\n---\n\n");
        text.push_str(&show_record(connection, &all, record, false)?);
    }
    Ok(text)
}

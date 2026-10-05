//! SessionStarted after adapter parsing. Errors are returned to the hook
//! boundary, which must log them and fail open without injecting (DESIGN §8.6).

use crate::cli::Result;
use crate::render::Rendered;
use crate::store::{BusyTimeout, Store};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, TransactionBehavior};
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub enum StartKind {
    Startup,
    Resume,
    Clear,
    Compact,
    Fork,
    Other,
}

pub struct SessionStarted<'a> {
    pub disabled: bool,
    pub command: &'a str,
    pub agent: &'a str,
    pub session_id: &'a str,
    pub cwd: &'a Path,
    pub start_kind: StartKind,
    pub now: DateTime<Utc>,
}

/// Explicit database and trusted spool root keep the core independent of hook
/// JSON and allow callers to isolate all filesystem effects.
pub fn start(event: &SessionStarted<'_>, database: &Path, root: &Path) -> Result<Option<Rendered>> {
    if event.disabled {
        return Ok(None);
    }
    let scope = crate::scope::Git::default().resolve(event.cwd)?;
    let Some(existing) = Store::open_read_only(database, BusyTimeout::Hook)? else {
        return Ok(None);
    };
    let key = scope.project_key.to_str().ok_or("项目路径不是 UTF-8")?;
    let adopted: bool = existing.connection().query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE key=?1 AND adopted=1)",
        [key],
        |r| r.get(0),
    )?;
    if !adopted {
        return Ok(None);
    }
    drop(existing);
    let source_id = format!("{}:{}", event.agent, event.session_id);
    let spool = crate::spool::Spool::open(root, database)?;
    let mut store = Store::open(database, BusyTimeout::Hook)?;
    crate::ingest::ingest(&mut store, &spool, Some(&source_id))?;
    let tx = store.transaction(TransactionBehavior::Immediate)?;
    let at = event.now.to_rfc3339_opts(SecondsFormat::Millis, true);
    tx.execute(
        "INSERT INTO sources(id,agent,session_id,association,first_seen,last_seen)
        VALUES (?1,?2,?3,'hook',?4,?4) ON CONFLICT(id) DO UPDATE SET last_seen=excluded.last_seen",
        params![source_id, event.agent, event.session_id, at],
    )?;
    let kind = match event.start_kind {
        StartKind::Startup => "startup",
        StartKind::Resume => "resume",
        StartKind::Clear => "clear",
        StartKind::Compact => "compact",
        StartKind::Fork => "fork",
        StartKind::Other => "other",
    };
    tx.execute(
        "INSERT INTO events(source_id,kind,at,detail) VALUES (?1,'session_started',?2,?3)",
        params![source_id, at, kind],
    )?;
    let rendered = crate::render::render(
        &tx,
        &crate::render::Request {
            scope: &scope,
            source_id: &source_id,
            command: event.command,
            now: event.now,
            incremental: matches!(event.start_kind, StartKind::Resume | StartKind::Fork),
            budget: crate::render::CHARACTER_BUDGET,
        },
    )?;
    for id in &rendered.record_ids {
        tx.execute(
            "INSERT INTO injections(source_id,record_id,injected_at) VALUES (?1,?2,?3)
            ON CONFLICT(source_id,record_id) DO NOTHING",
            params![source_id, id, at],
        )?;
    }
    tx.commit()?;
    Ok(Some(rendered))
}

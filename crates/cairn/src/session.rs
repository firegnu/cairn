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
    pub transcript_path: Option<&'a str>,
    pub start_kind: StartKind,
    pub now: DateTime<Utc>,
}

/// Explicit database and trusted spool root keep the core independent of hook
/// JSON and allow callers to isolate all filesystem effects.
pub fn start(event: &SessionStarted<'_>, database: &Path, root: &Path) -> Result<Option<Rendered>> {
    start_with_waiters(event, database, root, |_, _, _| {})
}

pub(crate) fn start_with_waiters(
    event: &SessionStarted<'_>,
    database: &Path,
    root: &Path,
    launch: impl FnOnce(&rusqlite::Connection, &crate::scope::Scope, &str),
) -> Result<Option<Rendered>> {
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
    let facts = collect_facts(&scope)?;
    let tx = store.transaction(TransactionBehavior::Immediate)?;
    // Unadopt may have committed since the probe; nothing is written or injected then.
    let adopted: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE key=?1 AND adopted=1)",
        [key],
        |r| r.get(0),
    )?;
    if !adopted {
        return Ok(None);
    }
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
    crate::turn::hook_seen(&tx, key, event.agent, "SessionStart", &at)?;
    Observation {
        source: &source_id,
        scope: &scope,
        transcript_path: event.transcript_path,
        at: &at,
        branch: facts.as_ref().and_then(|f| f.branch.as_deref()),
        facts: facts.as_ref(),
        turn_started: false,
        preserve_branch: false,
    }
    .write(&tx)?;
    // Spawn without waiting. The child acquires its lease after this transaction
    // closes; registration and injections still roll back together on failure.
    launch(&tx, &scope, &source_id);
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

pub(crate) fn collect_facts(
    scope: &crate::scope::Scope,
) -> std::result::Result<Option<crate::save::StoredFacts>, crate::scope::GitError> {
    if !scope.is_git {
        return Ok(None);
    }
    crate::scope::Git::default()
        .collect(&scope.line_path)?
        .map(|facts| {
            facts
                .try_into()
                .map_err(crate::scope::GitError::InvalidOutput)
        })
        .transpose()
}

/// TurnStarted needs the branch, but must leave the saved Git snapshot alone.
pub(crate) fn branch(
    scope: &crate::scope::Scope,
) -> std::result::Result<Option<String>, crate::scope::GitError> {
    if !scope.is_git {
        return Ok(None);
    }
    let output = crate::scope::Git::default().run(
        &scope.line_path,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
    )?;
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    if !output.status.success() {
        return Err(crate::scope::failed(output));
    }
    Ok(Some(
        String::from_utf8(output.stdout)
            .map_err(|_| crate::scope::GitError::InvalidOutput("branch is not UTF-8"))?
            .trim_end()
            .to_owned(),
    ))
}

pub(crate) struct Observation<'a> {
    pub source: &'a str,
    pub scope: &'a crate::scope::Scope,
    pub transcript_path: Option<&'a str>,
    pub at: &'a str,
    pub branch: Option<&'a str>,
    pub facts: Option<&'a crate::save::StoredFacts>,
    pub turn_started: bool,
    pub preserve_branch: bool,
}

impl Observation<'_> {
    pub fn write(&self, connection: &rusqlite::Connection) -> rusqlite::Result<()> {
        let facts = self
            .facts
            .map(|facts| serde_json::to_string(facts).expect("facts contain only JSON values"));
        connection.execute(
            "INSERT INTO sessions(source_id,project_id,line_path,branch,transcript_path,turns,last_event_at,facts,facts_at)
             SELECT ?1,id,?3,?4,?5,?6,?7,?8,?9 FROM projects WHERE key=?2
             ON CONFLICT(source_id) DO UPDATE SET
                project_id=CASE WHEN excluded.last_event_at>=sessions.last_event_at THEN excluded.project_id ELSE sessions.project_id END,
                line_path=CASE WHEN excluded.last_event_at>=sessions.last_event_at THEN excluded.line_path ELSE sessions.line_path END,
                branch=CASE WHEN ?10 OR excluded.last_event_at<sessions.last_event_at THEN sessions.branch ELSE excluded.branch END,
                transcript_path=CASE WHEN excluded.last_event_at>=sessions.last_event_at THEN excluded.transcript_path ELSE sessions.transcript_path END,
                last_event_at=MAX(sessions.last_event_at,excluded.last_event_at),
                turns=sessions.turns+excluded.turns,
                failures=CASE WHEN ?6 THEN 0 ELSE sessions.failures END,
                facts=CASE WHEN excluded.facts_at>=COALESCE(sessions.facts_at,'') THEN excluded.facts ELSE sessions.facts END,
                facts_at=CASE WHEN excluded.facts_at>=COALESCE(sessions.facts_at,'') THEN excluded.facts_at ELSE sessions.facts_at END",
            params![self.source, self.scope.project_key.to_str(), self.scope.line_path.to_str(),
                self.branch, self.transcript_path, i64::from(self.turn_started), self.at, facts,
                self.facts.map(|f| f.collected_at.as_str()), self.preserve_branch],
        )?;
        Ok(())
    }
}

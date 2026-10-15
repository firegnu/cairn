//! Parsed hook events and session observations (DESIGN §8.3–8.6).

use std::path::Path;

use crate::spool::Spool;
use crate::store::{BusyTimeout, Store};
use rusqlite::{params, Connection, TransactionBehavior};

#[derive(Debug, Clone, Copy)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

/// Paths and event values are supplied by the adapter; Git reads use bounded commands.
/// `database` is the lexical path from store::database_path; `scope` is
/// already resolved. `at` is the received time in RFC 3339 UTC milliseconds.
/// Only TurnEnded opens `spool_root`, after the disabled/adoption gates.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub database: &'a Path,
    pub spool_root: &'a Path,
    pub scope: &'a crate::scope::Scope,
    pub transcript_path: Option<&'a str>,
    pub agent: Agent,
    pub session_id: &'a str,
    pub at: &'a str,
    pub disabled: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Allow,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Spool(#[from] crate::spool::Error),
    #[error(transparent)]
    Ingest(#[from] crate::ingest::Error),
    #[error(transparent)]
    Store(#[from] crate::store::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("event time must be RFC 3339 UTC with milliseconds")]
    InvalidTimestamp,
    #[error("project key must be an absolute UTF-8 path")]
    InvalidProjectKey,
    #[error(transparent)]
    Git(#[from] crate::scope::GitError),
}

/// Errors always accompany Allow. The adapter is responsible for errors.log.
#[derive(Debug)]
pub struct Report {
    pub action: Action,
    pub errors: Vec<Error>,
}

pub fn turn_started(context: Context<'_>, turn_key: Option<&str>) -> Report {
    report((|| {
        let Some(mut store) = active_store(context)? else {
            return Ok(Action::Allow);
        };
        let branch = crate::session::branch(context.scope)?;
        let tx = store.transaction(TransactionBehavior::Immediate)?;
        if !adopted(&tx, context)? {
            return Ok(Action::Allow);
        }
        let source = source_id(context);
        register_source(&tx, context)?;
        tx.execute(
            "INSERT INTO events(source_id,kind,at,detail) VALUES (?1,'turn_started',?2,?3)",
            params![source, context.at, turn_key],
        )?;
        crate::session::Observation {
            source: &source,
            scope: context.scope,
            transcript_path: context.transcript_path,
            at: context.at,
            branch: branch.as_deref(),
            facts: None,
            turn_started: true,
            preserve_branch: false,
        }
        .write(&tx)?;
        seen(&tx, context, "UserPromptSubmit")?;
        tx.commit()?;
        Ok(Action::Allow)
    })())
}

/// Collect voluntary saves, then persist session metadata even if collection or
/// Git facts fail. No confirmation window or continuation decision is consulted.
pub fn turn_ended(context: Context<'_>) -> Report {
    let mut errors = Vec::new();
    let result = (|| {
        let Some(mut store) = active_store(context)? else {
            return Ok(Action::Allow);
        };
        let source = source_id(context);
        match Spool::open(context.spool_root, context.database) {
            Ok(spool) => {
                if let Err(error) = crate::ingest::ingest(&mut store, &spool, Some(&source)) {
                    errors.push(Error::Ingest(error));
                }
            }
            Err(error) => errors.push(Error::Spool(error)),
        }
        let (facts, preserve_branch) = match crate::session::collect_facts(context.scope) {
            Ok(facts) => (facts, false),
            Err(error) => {
                errors.push(Error::Git(error));
                (None, true)
            }
        };
        let tx = store.transaction(TransactionBehavior::Immediate)?;
        if !adopted(&tx, context)? {
            return Ok(Action::Allow);
        }
        register_source(&tx, context)?;
        crate::session::Observation {
            source: &source,
            scope: context.scope,
            transcript_path: context.transcript_path,
            at: context.at,
            branch: facts.as_ref().and_then(|f| f.branch.as_deref()),
            facts: facts.as_ref(),
            turn_started: false,
            preserve_branch,
        }
        .write(&tx)?;
        seen(&tx, context, "Stop")?;
        tx.commit()?;
        Ok(Action::Allow)
    })();
    let mut result = report(result);
    errors.append(&mut result.errors);
    result.errors = errors;
    result
}

/// Record the end observation only: no spool access, source upsert, or rendering.
pub fn session_ended(context: Context<'_>, reason: Option<&str>) -> Report {
    report((|| {
        let Some(mut store) = active_store(context)? else {
            return Ok(Action::Allow);
        };
        let tx = store.transaction(TransactionBehavior::Immediate)?;
        if !adopted(&tx, context)? {
            return Ok(Action::Allow);
        }
        tx.execute(
            "INSERT INTO events(source_id,kind,at,detail) VALUES (?1,'session_ended',?2,?3)",
            params![source_id(context), context.at, reason],
        )?;
        seen(&tx, context, "SessionEnd")?;
        tx.commit()?;
        Ok(Action::Allow)
    })())
}

/// Remember that this hook event was handled for an adopted project: one row for each
/// project, agent and event, holding the latest time only (DESIGN §6.2). `event` is the
/// agent's own hook event name. A project that has no row yet is left alone.
pub(crate) fn hook_seen(
    connection: &Connection,
    project_key: &str,
    agent: &str,
    event: &str,
    at: &str,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO hook_seen(project_id,agent,event,at)
         SELECT id,?2,?3,?4 FROM projects WHERE key=?1
         ON CONFLICT(project_id,agent,event) DO UPDATE SET at=MAX(at,excluded.at)",
        params![project_key, agent, event, at],
    )?;
    Ok(())
}

fn seen(connection: &Connection, context: Context<'_>, event: &str) -> Result<(), Error> {
    let key = context
        .scope
        .project_key
        .to_str()
        .ok_or(Error::InvalidProjectKey)?;
    Ok(hook_seen(
        connection,
        key,
        context.agent.name(),
        event,
        context.at,
    )?)
}

fn source_id(context: Context<'_>) -> String {
    format!("{}:{}", context.agent.name(), context.session_id)
}

fn report(result: Result<Action, Error>) -> Report {
    match result {
        Ok(action) => Report {
            action,
            errors: Vec::new(),
        },
        Err(error) => Report {
            action: Action::Allow,
            errors: vec![error],
        },
    }
}

fn adopted(connection: &Connection, context: Context<'_>) -> Result<bool, Error> {
    let key = context
        .scope
        .project_key
        .to_str()
        .filter(|_| context.scope.project_key.is_absolute())
        .ok_or(Error::InvalidProjectKey)?;
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE key=?1 AND adopted=1)",
        [key],
        |r| r.get(0),
    )?)
}

fn active_store(context: Context<'_>) -> Result<Option<Store>, Error> {
    if context.disabled {
        return Ok(None);
    }
    let Some(probe) = Store::open_read_only(context.database, BusyTimeout::Hook)? else {
        return Ok(None);
    };
    if !adopted(probe.connection(), context)? {
        return Ok(None);
    }
    if !crate::save::valid_timestamp(context.at) {
        return Err(Error::InvalidTimestamp);
    }
    drop(probe);
    Ok(Some(Store::open(context.database, BusyTimeout::Hook)?))
}

fn register_source(connection: &Connection, context: Context<'_>) -> Result<(), Error> {
    let source = source_id(context);
    connection.execute(
        "INSERT INTO sources(id,agent,session_id,association,first_seen,last_seen)
             VALUES (?1,?2,?3,'hook',?4,?4)
             ON CONFLICT(id) DO UPDATE SET agent=excluded.agent,session_id=excluded.session_id,
                 association='hook',first_seen=MIN(first_seen,excluded.first_seen),
                 last_seen=MAX(last_seen,excluded.last_seen)",
        params![source, context.agent.name(), context.session_id, context.at],
    )?;
    Ok(())
}

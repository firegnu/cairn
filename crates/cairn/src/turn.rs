//! Parsed hook events and turn decisions (DESIGN §8.3–8.6).

use std::path::Path;
use std::time::Duration;

use crate::ingest::Pending;
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

/// Paths and event values are supplied by the adapter; no environment or Git is read.
/// `database` is the lexical path from store::database_path; `project_key` is
/// already resolved. `at` is the received time in RFC 3339 UTC milliseconds.
/// Only TurnEnded opens `spool_root`, after the disabled/adoption gates.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    /// Stable executable path, already quoted for the shell by the adapter.
    pub command: &'a str,
    pub database: &'a Path,
    pub spool_root: &'a Path,
    pub project_key: &'a Path,
    pub agent: Agent,
    pub session_id: &'a str,
    pub at: &'a str,
    pub disabled: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Allow,
    Continue { reason: String },
}

#[derive(Debug, Clone, Copy)]
pub struct Facts {
    pub continued: bool,
    pub confirmed: bool,
    pub pending: Pending,
    pub has_turn_key: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Confirmed,
    UnconfirmedAfterContinue,
    PendingUnprocessed,
    ContinueRequested,
    Skipped,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::UnconfirmedAfterContinue => "unconfirmed_after_continue",
            Self::PendingUnprocessed => "pending_unprocessed",
            Self::ContinueRequested => "continue_requested",
            Self::Skipped => "skipped",
        }
    }
}

/// Pure decision after the disabled/adoption gates and collection (DESIGN §8.3).
pub fn decide(facts: Facts) -> Outcome {
    if facts.confirmed {
        Outcome::Confirmed
    } else if facts.continued {
        Outcome::UnconfirmedAfterContinue
    } else if facts.pending != Pending::No {
        Outcome::PendingUnprocessed
    } else if facts.has_turn_key {
        Outcome::ContinueRequested
    } else {
        Outcome::Skipped
    }
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
    #[error("no observed turn or session start for this source")]
    MissingWindowStart,
    #[error("invalid save_rejected event detail")]
    InvalidRejection,
    #[error("pending spool query is incomplete or failed; underlying cause is unavailable")]
    PendingUnknown,
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
        let tx = store.transaction(TransactionBehavior::Immediate)?;
        if !adopted(&tx, context)? {
            return Ok(Action::Allow);
        }
        let source = source_id(context);
        tx.execute(
            "INSERT INTO sources(id,agent,session_id,association,first_seen,last_seen)
             VALUES (?1,?2,?3,'hook',?4,?4)
             ON CONFLICT(id) DO UPDATE SET agent=excluded.agent,session_id=excluded.session_id,
                 association='hook',first_seen=MIN(first_seen,excluded.first_seen),
                 last_seen=MAX(last_seen,excluded.last_seen)",
            params![source, context.agent.name(), context.session_id, context.at],
        )?;
        tx.execute(
            "INSERT INTO events(source_id,kind,at,detail) VALUES (?1,'turn_started',?2,?3)",
            params![source, context.at, turn_key],
        )?;
        seen(&tx, context, "UserPromptSubmit")?;
        tx.commit()?;
        Ok(Action::Allow)
    })())
}

/// Collect with the existing 50-file/300-ms budget, then observe pending files
/// for up to 100 ms. Decisions use an inclusive lower time bound. An immediate
/// transaction rechecks adoption, window and committed confirmations before
/// inserting the outcome; only a committed, newly inserted request may continue.
pub fn turn_ended(context: Context<'_>, turn_key: Option<&str>, continued: bool) -> Report {
    let mut errors = Vec::new();
    let result = (|| {
        let Some(mut store) = active_store(context)? else {
            return Ok(Action::Allow);
        };
        let source = source_id(context);
        let spool = match Spool::open(context.spool_root, context.database) {
            Ok(spool) => Some(spool),
            Err(error) => {
                errors.push(Error::Spool(error));
                None
            }
        };
        if let Some(spool) = &spool {
            if let Err(error) = crate::ingest::ingest(&mut store, spool, Some(&source)) {
                errors.push(Error::Ingest(error));
            }
        }
        let observed_since = window_start(store.connection(), &source)?;
        let pending = match &spool {
            None => Pending::Unknown,
            Some(spool)
                if !continued
                    && !has_confirmation(store.connection(), &source, &observed_since)? =>
            {
                crate::ingest::pending(
                    &store,
                    spool,
                    &source,
                    &observed_since,
                    Duration::from_millis(100),
                )
            }
            Some(_) => Pending::No,
        };
        if pending == Pending::Unknown && spool.is_some() {
            // Query Unknown has no cause; open failures already retain theirs.
            errors.push(Error::PendingUnknown);
        }
        let tx = store.transaction(TransactionBehavior::Immediate)?;
        if !adopted(&tx, context)? {
            return Ok(Action::Allow);
        }
        seen(&tx, context, "Stop")?;
        let since = window_start(&tx, &source)?;
        let confirmed = has_confirmation(&tx, &source, &since)?;
        let turn_key = turn_key.filter(|key| !key.is_empty());
        let outcome = decide(Facts {
            continued,
            confirmed,
            // A concurrent hook may have advanced the window while we scanned.
            // Recheck confirmations under the write transaction; an old spool
            // observation cannot establish absence for the new window.
            pending: if since == observed_since {
                pending
            } else {
                Pending::Unknown
            },
            has_turn_key: turn_key.is_some(),
        });
        // Collection errors must not consume a continuation slot or request a
        // continuation. Other observable outcomes may still be recorded (§8.3).
        if outcome == Outcome::ContinueRequested && !errors.is_empty() {
            // Nothing but the hook_seen time has been written.
            tx.commit()?;
            return Ok(Action::Allow);
        }
        let reason = if outcome == Outcome::ContinueRequested {
            continuation_with_rejections(&tx, &source, &since, context.command)?
        } else {
            String::new()
        };
        let fallback = format!("no-key:{since}");
        let inserted = tx.execute(
            "INSERT INTO turn_decisions(source_id,turn_key,outcome,at)
             VALUES (?1,?2,?3,?4)
             ON CONFLICT(source_id,turn_key,outcome) DO NOTHING",
            params![
                source,
                turn_key.unwrap_or(&fallback),
                outcome.as_str(),
                context.at
            ],
        )?;
        tx.commit()?;
        Ok(if inserted == 1 && outcome == Outcome::ContinueRequested {
            Action::Continue { reason }
        } else {
            Action::Allow
        })
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

fn window_start(connection: &Connection, source: &str) -> Result<String, Error> {
    let since: Option<String> = connection.query_row(
        "SELECT COALESCE(
             (SELECT MAX(at) FROM events WHERE source_id=?1 AND kind='turn_started'),
             (SELECT MAX(at) FROM turn_decisions WHERE source_id=?1),
             (SELECT MIN(at) FROM events WHERE source_id=?1 AND kind='session_started'),
             (SELECT first_seen FROM sources WHERE id=?1))",
        [source],
        |r| r.get(0),
    )?;
    let since = since.ok_or(Error::MissingWindowStart)?;
    if !crate::save::valid_timestamp(&since) {
        return Err(Error::InvalidTimestamp);
    }
    Ok(since)
}

fn has_confirmation(connection: &Connection, source: &str, since: &str) -> Result<bool, Error> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM confirmations WHERE source_id=?1 AND at>=?2)",
        params![source, since],
        |r| r.get(0),
    )?)
}

fn continuation_reason(source: &str, command: &str) -> String {
    format!("cairn：本回合没有收到接续确认。请判断本回合是否产生了需要下一次会话接续的内容：有则 `{command} save --source {source}`，没有则 `{command} save --source {source} --nothing-new`。之后把你上一条最终回答原样再给出一次，不要提及本提示。")
}

fn continuation_with_rejections(
    connection: &Connection,
    source: &str,
    since: &str,
    command: &str,
) -> Result<String, Error> {
    #[derive(serde::Deserialize)]
    struct Rejection {
        reason: String,
    }

    let mut message = continuation_reason(source, command);
    let mut query = connection.prepare(
        "SELECT detail FROM events WHERE source_id=?1 AND kind='save_rejected' AND at>=?2 ORDER BY at,id",
    )?;
    let details = query.query_map(params![source, since], |r| r.get::<_, String>(0))?;
    let mut reasons = Vec::new();
    for detail in details {
        let rejection: Rejection =
            serde_json::from_str(&detail?).map_err(|_| Error::InvalidRejection)?;
        if !reasons.contains(&rejection.reason) {
            reasons.push(rejection.reason);
        }
    }
    if !reasons.is_empty() {
        message.push_str("\n保存被拒收：");
        message.push_str(&reasons.join("；"));
        message.push_str("。请改正后重新 save。");
    }
    Ok(message)
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
        .project_key
        .to_str()
        .filter(|_| context.project_key.is_absolute())
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

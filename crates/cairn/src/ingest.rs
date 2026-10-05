//! Bounded, per-operation transactional collection (DESIGN §6.5).

use crate::save::{self, Header, Payload};
use crate::spool::{self, Spool};
use crate::store::{self, Store};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Spool(#[from] spool::Error),
    #[error(transparent)]
    Store(#[from] store::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Default)]
pub struct Report {
    pub processed: usize,
    pub ingested: usize,
    pub rejected: usize,
    pub replayed: usize,
}

/// Collect at most 50 operations, for up to 300 ms checked between file steps.
/// SQLite lock waits share this budget. On any error, the current transaction
/// rolls back and its file remains; earlier committed operations stay committed.
/// `store` must be opened for `spool.database_path()` by the caller.
pub fn ingest(
    store: &mut Store,
    spool: &Spool,
    preferred_source: Option<&str>,
) -> Result<Report, Error> {
    let deadline = Instant::now() + Duration::from_millis(300);
    let mut candidates = Vec::new();
    for name in spool.names(Some(deadline))? {
        if Instant::now() >= deadline {
            break;
        }
        if !name.strip_suffix(".json").is_some_and(spool::valid_id) {
            continue;
        }
        let preferred = if preferred_source.is_some() {
            let Some((header, _)) = spool.candidate(&name, deadline)? else {
                continue;
            };
            header.source.as_deref() == preferred_source
        } else {
            false
        };
        candidates.push((!preferred, name));
    }
    candidates.sort_unstable();
    let original_timeout: u32 = store
        .connection()
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))?;
    let result = (|| {
        let mut report = Report::default();
        for (_, name) in candidates {
            if report.processed == 50 || Instant::now() >= deadline {
                break;
            }
            let Some((header, file)) = spool.candidate(&name, deadline)? else {
                continue;
            };
            let payload = Spool::payload(file, deadline).map_err(|_| "invalid spool payload");
            if Instant::now() >= deadline {
                break;
            }
            store.connection().busy_timeout(
                Duration::from_millis(original_timeout.into())
                    .min(deadline.saturating_duration_since(Instant::now())),
            )?;
            let tx = store.transaction(TransactionBehavior::Immediate)?;
            let id = name.strip_suffix(".json").unwrap();
            let outcome = collect_one(&tx, id, &header, payload)?;
            tx.commit()?;
            spool.remove(&name)?;
            report.processed += 1;
            match outcome {
                "ingested" => report.ingested += 1,
                "rejected" => report.rejected += 1,
                _ => report.replayed += 1,
            }
        }
        Ok(report)
    })();
    store
        .connection()
        .busy_timeout(Duration::from_millis(original_timeout.into()))?;
    result
}

fn collect_one(
    tx: &Transaction<'_>,
    id: &str,
    header: &Header,
    payload: Result<Payload, &'static str>,
) -> Result<&'static str, Error> {
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM spool_ops WHERE op_id=?1)",
        [id],
        |r| r.get::<_, bool>(0),
    )? {
        return Ok("replayed");
    }
    let processed_at = save::now();
    let at = payload
        .as_ref()
        .ok()
        .map(|p| p.created_at.as_str())
        .filter(|at| save::valid_timestamp(at))
        .unwrap_or(&processed_at);
    let source = resolve_source(tx, id, header.source.as_deref(), at)?;
    let result = if header.version != 1 {
        Err("unsupported spool version")
    } else if header.op_id != id {
        Err("filename and operation ID differ")
    } else {
        payload
            .as_ref()
            .map_err(|reason| *reason)
            .and_then(validate_payload)
    };
    let mut rejection = result.err();
    let mut project_id = None;
    if rejection.is_none() {
        let payload = payload.as_ref().unwrap();
        project_id = tx
            .query_row(
                "SELECT id FROM projects WHERE key=?1 AND adopted=1",
                [payload.project_key.to_str().unwrap()],
                |r| r.get::<_, i64>(0),
            )
            .optional()?;
        if project_id.is_none() {
            rejection = Some("project is not adopted");
        } else {
            for target in &payload.supersedes {
                let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM records r
                    WHERE r.id=?1 AND r.deleted_at IS NULL AND r.project_id=?2 AND r.line_path=?3
                    AND EXISTS(SELECT 1 FROM injections i WHERE i.record_id=r.id AND i.source_id=?4))",
                    params![target, project_id, payload.line_path.to_str().unwrap(), source], |r| r.get(0))?;
                if !valid {
                    rejection = Some("supersedes target must be live, on the same project/line, and injected to this source");
                    break;
                }
            }
        }
    }
    let record_id = if let Some(reason) = rejection {
        let detail = serde_json::json!({"op_id": id, "reason": reason}).to_string();
        tx.execute(
            "INSERT INTO events(source_id, kind, at, detail) VALUES (?1,'save_rejected',?2,?3)",
            params![source, at, detail],
        )?;
        None
    } else {
        let payload = payload.as_ref().unwrap();
        let record_id = if let Some(body) = &payload.body {
            let facts = payload
                .facts
                .as_ref()
                .map(|facts| serde_json::to_string(facts).expect("facts contain only JSON values"));
            tx.execute("INSERT INTO records(id, project_id, line_path, branch, source_id, kind, body, facts, created_at)
                VALUES (?1,?2,?3,?4,?5,'checkpoint',?6,?7,?8)",
                params![id, project_id, payload.line_path.to_str().unwrap(), payload.branch, source, body, facts, at])?;
            for target in &payload.supersedes {
                tx.execute(
                    "INSERT OR IGNORE INTO supersessions(record_id,target_id) VALUES (?1,?2)",
                    params![id, target],
                )?;
            }
            Some(id)
        } else {
            None
        };
        tx.execute(
            "INSERT INTO confirmations(source_id,kind,op_id,record_id,at) VALUES (?1,?2,?3,?4,?5)",
            params![
                source,
                if record_id.is_some() {
                    "saved"
                } else {
                    "nothing_new"
                },
                id,
                record_id,
                at
            ],
        )?;
        record_id
    };
    let outcome = if rejection.is_some() {
        "rejected"
    } else {
        "ingested"
    };
    tx.execute("INSERT INTO spool_ops(op_id,outcome,source_id,record_id,reason,processed_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![id, outcome, source, record_id, rejection, processed_at])?;
    Ok(outcome)
}

fn resolve_source(
    tx: &Transaction<'_>,
    id: &str,
    declared: Option<&str>,
    at: &str,
) -> Result<String, rusqlite::Error> {
    if let Some(source) = declared {
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sources WHERE id=?1)",
            [source],
            |r| r.get::<_, bool>(0),
        )? {
            tx.execute(
                "UPDATE sources SET last_seen=MAX(last_seen,?2) WHERE id=?1",
                params![source, at],
            )?;
            return Ok(source.to_owned());
        }
    }
    let source = format!("local:{id}");
    tx.execute("INSERT INTO sources(id,agent,association,first_seen,last_seen) VALUES (?1,'local','uncertain',?2,?2)
        ON CONFLICT(id) DO UPDATE SET last_seen=MAX(last_seen,excluded.last_seen)", params![source, at])?;
    Ok(source)
}

fn validate_payload(payload: &Payload) -> Result<(), &'static str> {
    if !save::valid_timestamp(&payload.created_at)
        || payload
            .facts
            .as_ref()
            .is_some_and(|f| !save::valid_timestamp(&f.collected_at))
    {
        return Err("invalid spool timestamp");
    }
    if [&payload.cwd, &payload.project_key, &payload.line_path]
        .iter()
        .any(|path| !path.is_absolute() || path.to_str().is_none())
    {
        return Err("invalid spool scope");
    }
    if payload.kind != "checkpoint" {
        return Err("invalid save kind");
    }
    match (&payload.body, payload.nothing_new) {
        (Some(body), false) => save::validate_body(body),
        (None, true) if payload.supersedes.is_empty() => Ok(()),
        _ => Err("invalid save body or nothing_new combination"),
    }
}

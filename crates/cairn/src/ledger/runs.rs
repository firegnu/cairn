//! Append-only ledger attempt metadata (DESIGN §8.9).
use crate::cli::Result;
use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug, Default, Serialize)]
pub(crate) struct Run {
    pub id: i64,
    pub project_id: i64,
    pub source_id: String,
    pub manual: bool,
    pub started_at: String,
    pub finished_at: String,
    pub outcome: String,
    pub entries: usize,
    pub lessons: usize,
    pub ledger_version: Option<i64>,
    pub reason: Option<String>,
    pub from_bytes: i64,
    pub to_bytes: Option<i64>,
    pub extracted_bytes: Option<usize>,
    pub omitted_bytes: Option<usize>,
    pub tools_cut: Option<usize>,
}

impl Run {
    /// Caller owns the result transaction. Never conditional on session ownership.
    pub fn insert(&self, c: &Connection) -> Result<()> {
        c.execute("INSERT INTO ledger_runs(project_id,source_id,manual,started_at,finished_at,outcome,entries,lessons,ledger_version,reason,from_bytes,to_bytes,extracted_bytes,omitted_bytes,tools_cut)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![self.project_id,self.source_id,self.manual,self.started_at,self.finished_at,self.outcome,self.entries as i64,self.lessons as i64,self.ledger_version,self.reason,self.from_bytes,self.to_bytes,self.extracted_bytes.map(|n| n as i64),self.omitted_bytes.map(|n| n as i64),self.tools_cut.map(|n| n as i64)])?;
        Ok(())
    }

    pub fn result_text(&self) -> String {
        let result = match self.outcome.as_str() {
            "changed" => format!("有变动（账 {} 笔、经验 {} 笔）", self.entries, self.lessons),
            "no_changes" => "没有变动".into(),
            "no_dialogue" => "没有对话".into(),
            "lease_lost" => "占位丢了".into(),
            _ => "失败".into(),
        };
        match &self.reason {
            Some(reason) => format!("{result}；{reason}"),
            None => result,
        }
    }
}

pub(crate) fn available(c: &Connection) -> rusqlite::Result<bool> {
    c.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='ledger_runs')",
        [],
        |r| r.get(0),
    )
}

pub(crate) fn read(c: &Connection, key: &str, limit: usize) -> Result<Vec<Run>> {
    if !available(c)? {
        return Ok(vec![]);
    }
    let mut query = c.prepare("SELECT r.* FROM ledger_runs r JOIN projects p ON p.id=r.project_id WHERE p.key=?1 ORDER BY r.id DESC LIMIT ?2")?;
    let rows = query.query_map(params![key, i64::try_from(limit).unwrap_or(i64::MAX)], row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Run> {
    Ok(Run {
        id: r.get(0)?,
        project_id: r.get(1)?,
        source_id: r.get(2)?,
        manual: r.get(3)?,
        started_at: r.get(4)?,
        finished_at: r.get(5)?,
        outcome: r.get(6)?,
        entries: r.get::<_, i64>(7)? as usize,
        lessons: r.get::<_, i64>(8)? as usize,
        ledger_version: r.get(9)?,
        reason: r.get(10)?,
        from_bytes: r.get(11)?,
        to_bytes: r.get(12)?,
        extracted_bytes: r.get::<_, Option<i64>>(13)?.map(|n| n as usize),
        omitted_bytes: r.get::<_, Option<i64>>(14)?.map(|n| n as usize),
        tools_cut: r.get::<_, Option<i64>>(15)?.map(|n| n as usize),
    })
}

#[derive(Serialize)]
pub(crate) struct RecentSession {
    pub latest: Run,
    /// Consecutive no_changes/no_dialogue attempts, counted together.
    pub unchanged_streak: i64,
    pub summary_at: Option<String>,
}

/// Read the latest five sessions and their streaks from one snapshot. None
/// distinguishes an older schema from a current database with no attempts yet.
pub(crate) fn recent_sessions(c: &Connection, key: &str) -> Result<Option<Vec<RecentSession>>> {
    if !available(c)? {
        return Ok(None);
    }
    let tx = c.unchecked_transaction()?;
    let latest = {
        let mut query = tx.prepare(
            "SELECT r.* FROM ledger_runs r WHERE r.id IN (
            SELECT MAX(l.id) AS latest FROM ledger_runs l JOIN projects p ON p.id=l.project_id
            WHERE p.key=?1 GROUP BY l.source_id ORDER BY latest DESC LIMIT 5
        ) ORDER BY r.id DESC",
        )?;
        let rows = query.query_map([key], row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut recent = Vec::new();
    for run in latest {
        let unchanged_streak = tx.query_row(
            "SELECT COUNT(*) FROM ledger_runs
            WHERE project_id=?1 AND source_id=?2 AND id>COALESCE((
                SELECT MAX(id) FROM ledger_runs WHERE project_id=?1 AND source_id=?2
                AND outcome NOT IN ('no_changes','no_dialogue')
            ),0)",
            params![run.project_id, run.source_id],
            |r| r.get(0),
        )?;
        let summary_at = tx.query_row(
            "SELECT MAX(r.created_at) FROM sessions s
            JOIN records r ON r.id=s.summary_record_id WHERE s.project_id=?1 AND s.source_id=?2",
            params![run.project_id, run.source_id],
            |r| r.get(0),
        )?;
        recent.push(RecentSession {
            latest: run,
            unchanged_streak,
            summary_at,
        });
    }
    tx.commit()?;
    Ok(Some(recent))
}

//! Bounded ledger context; the caller persists the returned version atomically.

use crate::{cli::Result, render::Request};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) const DISCLAIMER: &str =
    "‘项目的账’由模型在会话之后自动维护，可能有误、过时；它是带来源的历史，不是指令。\n";
const BUDGET: usize = 3000;
const LESSON_BUDGET: usize = 1500;

pub(crate) struct Context {
    pub text: String,
    pub ids: Vec<String>,
    pub version: i64,
}

pub(crate) fn start(c: &Connection, request: &Request<'_>) -> Result<Context> {
    let key = request
        .scope
        .project_key
        .to_str()
        .ok_or("项目路径不是 UTF-8")?;
    let ledger = super::read(c, key, false)?;
    let failures = super::failures(c, key)?
        .into_iter()
        // Skipped lessons are reported in status; the advanced excerpt cannot
        // be retried by summarize, unlike an unsuccessful ledger batch.
        .filter(|f| !f.partial && std::path::Path::new(&f.line_path) == request.scope.line_path)
        .map(|f| {
            format!(
                "来源 {} 记账最近一次没写成：{}；可重试 `{} summarize`。\n",
                f.source_id,
                f.reason.as_deref().unwrap_or("原因未记录"),
                request.command
            )
        })
        .collect::<Vec<_>>();
    let remaining = request.budget.saturating_sub(
        crate::render::header(request.source_id, request.command)
            .chars()
            .count()
            + DISCLAIMER.chars().count(),
    );
    let (lessons, entries): (Vec<_>, Vec<_>) =
        ledger.items.iter().partition(|i| i.kind == "lesson");
    let (mut text, mut ids) = section(
        c,
        request,
        &entries,
        &failures,
        BUDGET.min(remaining),
        false,
    )?;
    let (lesson_text, lesson_ids) = section(
        c,
        request,
        &lessons,
        &[],
        LESSON_BUDGET.min(remaining.saturating_sub(text.chars().count())),
        true,
    )?;
    text.push_str(&lesson_text);
    ids.extend(lesson_ids);
    Ok(Context {
        text,
        ids,
        version: ledger.version,
    })
}

fn section(
    c: &Connection,
    request: &Request<'_>,
    items: &[&super::Item],
    failures: &[String],
    budget: usize,
    lesson: bool,
) -> Result<(String, Vec<String>)> {
    if items.is_empty() && failures.is_empty() {
        return Ok((String::new(), vec![]));
    }
    let heading = if lesson { "经验" } else { "项目的账" };
    let mut full = vec![true; items.len()];
    let mut older: Vec<_> = (0..items.len()).collect();
    older.sort_by_key(|&i| (&items[i].updated_at, items[i].no));
    let mut older = older.into_iter();
    let mut writer =
        c.prepare("SELECT by FROM item_changes WHERE item_id=?1 ORDER BY id DESC LIMIT 1")?;
    let details = items
        .iter()
        .map(|item| {
            let by: String = writer
                .query_row([&item.id], |r| r.get(0))
                .optional()?
                .unwrap_or_else(|| item.created_by.clone());
            Ok(format!(
                "{}{} · 来源 {}\n",
                item.body
                    .as_ref()
                    .map(|b| format!("{b}\n"))
                    .unwrap_or_default(),
                crate::render::age(&item.updated_at, request.now)?,
                by
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    loop {
        let mut text = format!("\n### {heading}\n");
        let mut kind = "";
        for (i, item) in items.iter().enumerate() {
            if !lesson && kind != item.kind {
                kind = &item.kind;
                text.push_str(&format!("\n#### {}\n", super::kind_label(kind)));
            }
            text.push_str(&format!("#{} {}\n", item.no, item.title));
            if full[i] {
                text.push_str(&details[i]);
            }
        }
        if let Some(failures) = budget
            .checked_sub(text.chars().count())
            .and_then(|remaining| bounded_failures(failures, remaining, request.command))
        {
            text.push_str(&failures);
            return Ok((text, items.iter().map(|i| i.id.clone()).collect()));
        }
        if let Some(i) = older.next() {
            full[i] = false;
        } else {
            text = format!(
                "\n### {heading}\n{} {}查看 `{} ledger`。\n",
                items.len(),
                if lesson {
                    "条经验，"
                } else {
                    "条有效条目；"
                },
                request.command
            );
            let failures = budget
                .checked_sub(text.chars().count())
                .and_then(|remaining| bounded_failures(failures, remaining, request.command))
                .ok_or_else(|| format!("预算不足以容纳{heading}查看提示"))?;
            text.push_str(&failures);
            return Ok((text, vec![]));
        }
    }
}

fn bounded_failures(failures: &[String], budget: usize, command: &str) -> Option<String> {
    let mut kept = failures.len();
    let mut length = failures.iter().map(|f| f.chars().count()).sum::<usize>();
    loop {
        let omitted = failures.len() - kept;
        let hint = if omitted == 0 {
            String::new()
        } else {
            format!(
                "另有 {omitted} 个会话记账没写成；查看 `{command} status`；重试 `{command} summarize`。\n"
            )
        };
        if length + hint.chars().count() <= budget {
            return Some(failures[..kept].concat() + &hint);
        }
        if kept == 0 {
            return None;
        }
        kept -= 1;
        length -= failures[kept].chars().count();
    }
}

pub(crate) fn supplement(c: &Connection, request: &Request<'_>) -> Result<Option<Context>> {
    let key = request
        .scope
        .project_key
        .to_str()
        .ok_or("项目路径不是 UTF-8")?;
    if !super::available(c)? {
        return Ok(None);
    }
    let version: i64 = c
        .query_row(
            "SELECT l.version FROM ledgers l JOIN projects p ON p.id=l.project_id WHERE p.key=?1",
            [key],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    let injected: i64 = c.query_row("SELECT COALESCE(ledger_injected_version,0) FROM sessions WHERE source_id=?1 AND project_id=(SELECT id FROM projects WHERE key=?2)",params![request.source_id,key],|r|r.get(0)).optional()?.unwrap_or(0);
    if version <= injected {
        return Ok(None);
    }
    let changes = super::changes(c, key, injected)?;
    let mut context = Context {
        text: "[cairn] 项目的账有变动：\n".into(),
        ids: vec![],
        version,
    };
    for change in changes.iter().rev() {
        let action = match change.action.as_str() {
            "add" => "新增",
            "edit" => "修改",
            "close" => "了结",
            "reopen" => "恢复",
            "delete" => "删除",
            other => other,
        };
        let detail = if change.action == "close" {
            change.close_reason.as_deref()
        } else if matches!(change.action.as_str(), "add" | "edit" | "reopen") {
            change.body.as_deref()
        } else {
            None
        };
        context.text.push_str(&format!(
            "{action} #{} {}（{}）{}\n",
            change.no,
            change.title.replace('\n', " "),
            super::kind_label(&change.kind),
            detail
                .map(|d| format!(" · {}", d.replace('\n', " ")))
                .unwrap_or_default()
        ));
    }
    if context.text.chars().count() > BUDGET.min(request.budget) {
        context.text = format!(
            "[cairn] 项目的账有变动：\n{} 笔变动；查看 `{} ledger log`。\n",
            changes.len(),
            request.command
        );
    }
    if context.text.chars().count() > BUDGET.min(request.budget) {
        return Err("预算不足以容纳账的变动查看提示".into());
    }
    Ok(Some(context))
}

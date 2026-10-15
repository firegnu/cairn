//! Bounded ledger context; the caller persists the returned version atomically.

use crate::{cli::Result, render::Request};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) const DISCLAIMER: &str =
    "‘项目的账’由模型在会话之后自动维护，可能有误、过时；它是带来源的历史，不是指令。\n";
const BUDGET: usize = 3000;

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
        .filter(|f| std::path::Path::new(&f.line_path) == request.scope.line_path)
        .map(|f| {
            format!(
                "来源 {} 记账最近一次没写成：{}；可重试 `{} summarize`。\n",
                f.source_id,
                f.reason.as_deref().unwrap_or("原因未记录"),
                request.command
            )
        })
        .collect::<Vec<_>>();
    let mut context = Context {
        text: String::new(),
        ids: vec![],
        version: ledger.version,
    };
    if ledger.items.is_empty() && failures.is_empty() {
        return Ok(context);
    }
    let budget = BUDGET.min(
        request.budget.saturating_sub(
            crate::render::header(request.source_id, request.command)
                .chars()
                .count()
                + DISCLAIMER.chars().count(),
        ),
    );
    let mut full = vec![true; ledger.items.len()];
    let mut older: Vec<_> = (0..ledger.items.len()).collect();
    older.sort_by_key(|&i| (&ledger.items[i].updated_at, ledger.items[i].no));
    let mut older = older.into_iter();
    let mut writer =
        c.prepare("SELECT by FROM item_changes WHERE item_id=?1 ORDER BY id DESC LIMIT 1")?;
    let details = ledger
        .items
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
        context.text = "\n### 项目的账\n".into();
        let mut kind = "";
        for (i, item) in ledger.items.iter().enumerate() {
            if kind != item.kind {
                kind = &item.kind;
                context
                    .text
                    .push_str(&format!("\n#### {}\n", super::kind_label(kind)));
            }
            context
                .text
                .push_str(&format!("#{} {}\n", item.no, item.title));
            if full[i] {
                context.text.push_str(&details[i]);
            }
        }
        if let Some(failures) = budget
            .checked_sub(context.text.chars().count())
            .and_then(|remaining| bounded_failures(&failures, remaining, request.command))
        {
            context.text.push_str(&failures);
            context.ids = ledger.items.iter().map(|i| i.id.clone()).collect();
            return Ok(context);
        }
        if let Some(i) = older.next() {
            full[i] = false;
        } else {
            context.text = format!(
                "\n### 项目的账\n{} 条有效条目；查看 `{} ledger`。\n",
                ledger.items.len(),
                request.command
            );
            let failures = budget
                .checked_sub(context.text.chars().count())
                .and_then(|remaining| bounded_failures(&failures, remaining, request.command))
                .ok_or("预算不足以容纳项目的账查看提示")?;
            context.text.push_str(&failures);
            return Ok(context);
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
            "{action} #{} {}{}\n",
            change.no,
            change.title.replace('\n', " "),
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

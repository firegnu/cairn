//! 采集 Git 事实，计算现场对比（DESIGN §8.2 第 3 步、§11.2）。

use crate::scope::{failed, Git, GitError};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitFacts {
    pub head: Option<String>,
    pub branch: Option<String>,
    pub upstream: Option<Upstream>,
    pub staged: u64,
    pub unstaged: u64,
    pub untracked: u64,
    pub collected_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upstream {
    pub name: String,
    /// None when Git knows the configured name but the local ref is missing.
    pub divergence: Option<Divergence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Divergence {
    pub ahead: u64,
    pub behind: u64,
}

impl Git {
    /// A non-Git directory has no Git facts, rather than a fictitious clean status.
    pub fn collect(&self, directory: &Path) -> Result<Option<GitFacts>, GitError> {
        let scope = self.resolve(directory)?;
        if !scope.is_git {
            return Ok(None);
        }
        let status = self.checked(
            &scope.line_path,
            &[
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--ahead-behind",
                "--untracked-files=all",
                "--ignore-submodules=none",
                "--renames",
            ],
        )?;
        let mut facts = parse_status(&status)?;
        facts.collected_at_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| GitError::InvalidOutput("system clock precedes Unix epoch"))?
            .as_millis()
            .try_into()
            .map_err(|_| GitError::InvalidOutput("timestamp exceeds u64"))?;
        Ok(Some(facts))
    }

    /// Return only the fixed local comparison lines, not a complete injection.
    /// No HEAD line is emitted when the recorded repository had no commits.
    pub fn compare(&self, recorded: &GitFacts, line: &Path) -> Result<Vec<String>, GitError> {
        let current = self
            .collect(line)?
            .ok_or(GitError::InvalidOutput("work line is no longer in Git"))?;
        let mut lines = Vec::new();
        if let Some(recorded_head) = &recorded.head {
            if !is_oid(recorded_head) {
                return Err(GitError::InvalidOutput(
                    "recorded HEAD is not a full object ID",
                ));
            }
            let mut count = None;
            if let Some(current_head) = &current.head {
                // A historical object may have been pruned. Only Git's quiet
                // missing-revision result is treated as absence; other errors propagate.
                let present = self.run(
                    line,
                    &[
                        "rev-parse",
                        "--verify",
                        "--quiet",
                        "--end-of-options",
                        &format!("{recorded_head}^{{commit}}"),
                    ],
                )?;
                if present.status.success() {
                    let ancestor = self.run(
                        line,
                        &["merge-base", "--is-ancestor", recorded_head, current_head],
                    )?;
                    match ancestor.status.code() {
                        Some(0) => {
                            let output = self.checked(
                                line,
                                &[
                                    "rev-list",
                                    "--count",
                                    &format!("{recorded_head}..{current_head}"),
                                    "--",
                                ],
                            )?;
                            count = Some(number(text(&output)?.trim_end())?);
                        }
                        Some(1) => {}
                        _ => return Err(failed(ancestor)),
                    }
                } else if present.status.code() != Some(1) {
                    return Err(failed(present));
                }
            }
            lines.push(match count {
                Some(n) => format!("HEAD 比记录时多 {n} 个提交"),
                None => "记录时的 HEAD 已不在当前分支历史中".to_owned(),
            });
        }
        if let Some(Upstream {
            name,
            divergence: Some(divergence),
        }) = &current.upstream
        {
            lines.push(format!("相对本地 upstream ref {name} ahead {} / behind {}（只基于本地 ref，不代表远端实际状态）", divergence.ahead, divergence.behind));
        }
        lines.push(format!(
            "当前有 {} 个已暂存 / {} 个未暂存 / {} 个未跟踪文件",
            current.staged, current.unstaged, current.untracked
        ));
        lines.push("现场变化不说明记录叙述过时。".to_owned());
        Ok(lines)
    }
}

fn parse_status(status: &[u8]) -> Result<GitFacts, GitError> {
    let mut facts = GitFacts {
        head: None,
        branch: None,
        upstream: None,
        staged: 0,
        unstaged: 0,
        untracked: 0,
        collected_at_unix_ms: 0,
    };
    let mut saw_head = false;
    let mut saw_branch = false;
    let mut divergence = None;
    let mut records = status.split(|byte| *byte == 0);
    while let Some(record) = records.next() {
        if record.is_empty() {
            continue;
        }
        if let Some(oid) = record.strip_prefix(b"# branch.oid ") {
            saw_head = true;
            let oid = text(oid)?;
            if oid != "(initial)" {
                if !is_oid(oid) {
                    return Err(GitError::InvalidOutput("HEAD is not a full object ID"));
                }
                facts.head = Some(oid.to_owned());
            }
        } else if let Some(branch) = record.strip_prefix(b"# branch.head ") {
            saw_branch = true;
            let branch = text(branch)?;
            if branch != "(detached)" {
                facts.branch = Some(branch.to_owned());
            }
        } else if let Some(name) = record.strip_prefix(b"# branch.upstream ") {
            facts.upstream = Some(Upstream {
                name: text(name)?.to_owned(),
                divergence: None,
            });
        } else if let Some(counts) = record.strip_prefix(b"# branch.ab +") {
            let (ahead, behind) = text(counts)?
                .split_once(" -")
                .ok_or(GitError::InvalidOutput("invalid upstream divergence"))?;
            divergence = Some(Divergence {
                ahead: number(ahead)?,
                behind: number(behind)?,
            });
        } else {
            match record[0] {
                b'#' => {}
                b'1' | b'2' | b'u' => {
                    let xy = record
                        .get(2..4)
                        .ok_or(GitError::InvalidOutput("missing XY status"))?;
                    facts.staged += u64::from(xy[0] != b'.');
                    facts.unstaged += u64::from(xy[1] != b'.');
                    // A rename is one entry; its original path is a separate NUL field.
                    if record[0] == b'2' && records.next().is_none() {
                        return Err(GitError::InvalidOutput("missing original rename path"));
                    }
                }
                b'?' => facts.untracked += 1,
                b'!' => {}
                _ => return Err(GitError::InvalidOutput("unrecognized porcelain record")),
            }
        }
    }
    if !saw_head || !saw_branch {
        return Err(GitError::InvalidOutput("missing branch headers"));
    }
    if let Some(upstream) = &mut facts.upstream {
        upstream.divergence = divergence;
    }
    Ok(facts)
}

fn text(bytes: &[u8]) -> Result<&str, GitError> {
    std::str::from_utf8(bytes).map_err(|_| GitError::InvalidOutput("non-UTF-8 metadata"))
}

fn number(value: &str) -> Result<u64, GitError> {
    value
        .parse()
        .map_err(|_| GitError::InvalidOutput("invalid commit count"))
}

fn is_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

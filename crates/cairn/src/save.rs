//! The version-1 spool payload and persistence timestamps.

use crate::facts::{GitFacts, Upstream};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(crate) const BODY_LIMIT: usize = 6 * 1024;

pub(crate) fn validate_body(body: &str) -> Result<(), &'static str> {
    if body.len() > BODY_LIMIT {
        return Err("正文超过 6 KiB");
    }
    if body.trim().is_empty() {
        return Err("正文不能为空");
    }
    if !body.lines().any(|line| line.trim_end() == "## 停点") {
        return Err("正文必须有 ## 停点 一节");
    }
    Ok(())
}

pub fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(crate) fn valid_timestamp(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value).is_ok_and(|time| {
        time.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Millis, true)
            == value
            && value.len() == 24
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub version: u32,
    pub op_id: String,
    pub database_path: PathBuf,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub cwd: PathBuf,
    pub project_key: PathBuf,
    pub line_path: PathBuf,
    pub branch: Option<String>,
    pub kind: String,
    pub body: Option<String>,
    pub nothing_new: bool,
    pub supersedes: Vec<String>,
    pub facts: Option<StoredFacts>,
    pub created_at: String,
}

/// Same observed facts as Git::collect, with the required storage time format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredFacts {
    pub head: Option<String>,
    pub branch: Option<String>,
    pub upstream: Option<Upstream>,
    pub staged: u64,
    pub unstaged: u64,
    pub untracked: u64,
    pub collected_at: String,
}

impl TryFrom<GitFacts> for StoredFacts {
    type Error = &'static str;

    fn try_from(facts: GitFacts) -> Result<Self, Self::Error> {
        let time = i64::try_from(facts.collected_at_unix_ms)
            .ok()
            .and_then(DateTime::from_timestamp_millis)
            .ok_or("invalid Git collection time")?;
        Ok(Self {
            head: facts.head,
            branch: facts.branch,
            upstream: facts.upstream,
            staged: facts.staged,
            unstaged: facts.unstaged,
            untracked: facts.untracked,
            collected_at: time.to_rfc3339_opts(SecondsFormat::Millis, true),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub header: Header,
    pub payload: Payload,
}

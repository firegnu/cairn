//! User command boundary; explicit paths also allow isolated callers/tests.

use clap::{Parser, Subcommand};
use std::io::Read;
use std::path::Path;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Parser)]
#[command(name = "cairn", version, about = "工作接续记忆")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Adopt,
    Unadopt,
    Show {
        #[arg(long)]
        json: bool,
    },
    Save {
        #[arg(long)]
        source: Option<String>,
        #[arg(long, conflicts_with = "supersedes")]
        nothing_new: bool,
        #[arg(long)]
        supersedes: Vec<String>,
    },
}

fn read_body(input: &mut impl Read, nothing_new: bool) -> Result<Option<String>> {
    if nothing_new {
        return Ok(None);
    }
    let mut body = String::new();
    input
        .take((crate::save::BODY_LIMIT + 1) as u64)
        .read_to_string(&mut body)
        .map_err(|_| "正文必须是 UTF-8，且不超过 6 KiB")?;
    crate::save::validate_body(&body)?;
    Ok(Some(body))
}

pub fn run(cli: Cli, input: &mut impl Read) -> Result<String> {
    let cwd = std::env::current_dir()?;
    let xdg = std::env::var_os("XDG_STATE_HOME");
    let home = std::env::var_os("HOME");
    let database = crate::store::database_path(
        xdg.as_deref().map(Path::new),
        home.as_deref().map(Path::new),
    )?;
    run_at(cli, input, &cwd, &database, &crate::spool::trusted_root()?)
}

/// Execute with explicit paths. Production uses run(); isolated tests provide a
/// private temporary root without overriding the system root selection policy.
pub fn run_at(
    cli: Cli,
    input: &mut impl Read,
    cwd: &Path,
    database: &Path,
    root: &Path,
) -> Result<String> {
    use crate::save::{Header, Operation, Payload, StoredFacts};
    let git = crate::scope::Git::default();
    let scope = git.resolve(cwd)?;
    match cli.command {
        Command::Show { json } => {
            let existing = crate::store::Store::open_read_only(
                database,
                crate::store::BusyTimeout::UserCommand,
            )?;
            if existing.is_none() {
                return Ok(if json {
                    "{\"status\":\"no_data\"}"
                } else {
                    "尚无数据"
                }
                .into());
            }
            drop(existing);
            let spool = crate::spool::Spool::open(root, database)?;
            let mut store =
                crate::store::Store::open(database, crate::store::BusyTimeout::UserCommand)?;
            crate::ingest::ingest(&mut store, &spool, None)?;
            let tx = store.transaction(rusqlite::TransactionBehavior::Deferred)?;
            let rendered = crate::render::render(
                &tx,
                &crate::render::Request {
                    scope: &scope,
                    source_id: "<本来源ID>",
                    now: chrono::Utc::now(),
                    incremental: false,
                    budget: crate::render::CHARACTER_BUDGET,
                },
            )?;
            tx.commit()?;
            if json {
                Ok(serde_json::to_string(&rendered)?)
            } else {
                Ok(rendered.text)
            }
        }
        Command::Save {
            source,
            nothing_new,
            supersedes,
        } => {
            let body = read_body(input, nothing_new)?;
            let facts = git.collect(cwd)?.map(StoredFacts::try_from).transpose()?;
            let branch = facts.as_ref().and_then(|facts| facts.branch.clone());
            let op_id = ulid::Ulid::new().to_string();
            let project = if scope
                .project_key
                .file_name()
                .is_some_and(|name| name == ".git")
            {
                scope.project_key.parent().unwrap()
            } else {
                &scope.project_key
            };
            let message = format!(
                "saved {} {} · {} · {}",
                op_id,
                project.file_name().unwrap_or_default().to_string_lossy(),
                branch.as_deref().unwrap_or("-"),
                facts
                    .as_ref()
                    .and_then(|facts| facts.head.as_deref())
                    .map(|head| &head[..7])
                    .unwrap_or("-")
            );
            let operation = Operation {
                header: Header {
                    version: 1,
                    op_id,
                    database_path: database.into(),
                    source,
                },
                payload: Payload {
                    cwd: cwd.canonicalize()?,
                    project_key: scope.project_key,
                    line_path: scope.line_path,
                    branch,
                    kind: "checkpoint".into(),
                    body,
                    nothing_new,
                    supersedes,
                    facts,
                    created_at: crate::save::now(),
                },
            };
            crate::spool::Spool::open(root, database)?.publish(&operation)?;
            Ok(message)
        }
        command => {
            let spool = crate::spool::Spool::open(root, database)?;
            let mut store =
                crate::store::Store::open(database, crate::store::BusyTimeout::UserCommand)?;
            crate::ingest::ingest(&mut store, &spool, None)?;
            let adopted = matches!(command, Command::Adopt);
            crate::adopt::set_adopted(&mut store, &scope, adopted)?;
            Ok(format!(
                "{} {}",
                if adopted { "adopted" } else { "unadopted" },
                scope.project_key.display()
            ))
        }
    }
}

//! Official hook JSON boundary. Only explicitly selected metadata is decoded.

use serde::Deserialize;
use serde_json::json;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Agent {
    Claude,
    Codex,
}

/// Hook stdout has no user-command newline or error exit code.
pub fn execute(agent: Agent) {
    let output = run(agent, &mut std::io::stdin().lock(), None);
    if std::io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .is_err()
    {
        log_error(agent, "unknown", "io", None);
    }
}
impl Agent {
    fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
    fn core(self) -> crate::turn::Agent {
        match self {
            Self::Claude => crate::turn::Agent::Claude,
            Self::Codex => crate::turn::Agent::Codex,
        }
    }
}

// Unknown values (including prompt, last_assistant_message and transcript_path)
// are skipped by serde, never retained in metadata or forwarded to the core.
#[derive(Deserialize)]
struct Envelope {
    hook_event_name: String,
}
#[derive(Deserialize)]
struct Metadata {
    session_id: String,
    cwd: PathBuf,
    source: Option<String>,
    prompt_id: Option<String>,
    turn_id: Option<String>,
    stop_hook_active: Option<bool>,
    reason: Option<String>,
}

fn database_path() -> crate::store::Result<PathBuf> {
    crate::store::database_path(
        std::env::var_os("XDG_STATE_HOME").as_deref().map(Path::new),
        std::env::var_os("HOME").as_deref().map(Path::new),
    )
}

fn allow(agent: Agent, event: Option<&str>) -> String {
    if matches!(agent, Agent::Codex) && matches!(event, Some("Stop") | None) {
        "{}".into()
    } else {
        String::new()
    }
}

/// Production resolves the system private spool root; run_at supplies isolated
/// paths. This boundary always returns a successful, agent-specific response.
pub fn run(agent: Agent, input: &mut impl Read, paths: Option<(&Path, &Path)>) -> String {
    let disabled = std::env::var_os("CAIRN_DISABLE").is_some_and(|v| v == "1");
    let mut bytes = Vec::new();
    let read = input.read_to_end(&mut bytes);
    let envelope = serde_json::from_slice::<Envelope>(&bytes);
    let event = envelope.as_ref().ok().map(|e| e.hook_event_name.clone());
    if disabled {
        return allow(agent, event.as_deref());
    }
    let result = (|| -> crate::cli::Result<String> {
        read?;
        let envelope = envelope?;
        let event = envelope.hook_event_name.as_str();
        if !matches!(
            event,
            "SessionStart" | "UserPromptSubmit" | "Stop" | "SessionEnd"
        ) {
            return Ok(String::new());
        }
        let metadata: Metadata = serde_json::from_slice(&bytes)?;
        let database = match paths {
            Some((database, _)) => database.into(),
            None => database_path()?,
        };
        let root = match paths {
            Some((_, root)) => root.into(),
            None => crate::spool::trusted_root()?,
        };
        dispatch(agent, event, metadata, &database, &root)
    })();
    result.unwrap_or_else(|error| {
        log_error(
            agent,
            event.as_deref().unwrap_or("unknown"),
            error_category(error.as_ref()),
            paths.map(|(db, _)| db),
        );
        allow(agent, event.as_deref())
    })
}

fn dispatch(
    agent: Agent,
    event: &str,
    metadata: Metadata,
    database: &Path,
    root: &Path,
) -> crate::cli::Result<String> {
    let now = chrono::Utc::now();
    let command = crate::install::stable_command()?;
    if event == "SessionStart" {
        use crate::session::StartKind;
        let start_kind = match metadata.source.as_deref() {
            Some("startup") => StartKind::Startup,
            Some("resume") => StartKind::Resume,
            Some("clear") => StartKind::Clear,
            Some("compact") => StartKind::Compact,
            _ => StartKind::Other,
        };
        return Ok(crate::session::start(
            &crate::session::SessionStarted {
                disabled: false,
                command: &command,
                agent: agent.name(),
                session_id: &metadata.session_id,
                cwd: &metadata.cwd,
                start_kind,
                now,
            },
            database,
            root,
        )?
        .map(|rendered| {
            json!({"hookSpecificOutput": {
                "hookEventName": "SessionStart", "additionalContext": rendered.text
            }})
            .to_string()
        })
        .unwrap_or_default());
    }
    // SessionEnd has only 1.5 s in Claude. Discovery needs two git commands;
    // leave room for the existing 200 ms database lock waits and event write.
    let git = if event == "SessionEnd" {
        crate::scope::Git::new(Duration::from_millis(100))
    } else {
        crate::scope::Git::default()
    };
    let scope = git.resolve(&metadata.cwd)?;
    let at = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let context = crate::turn::Context {
        command: &command,
        database,
        spool_root: root,
        project_key: &scope.project_key,
        agent: agent.core(),
        session_id: &metadata.session_id,
        at: &at,
        disabled: false,
    };
    let key = match agent {
        Agent::Claude => metadata.prompt_id.as_deref(),
        Agent::Codex => metadata.turn_id.as_deref(),
    };
    let report = match event {
        "UserPromptSubmit" => crate::turn::turn_started(context, key),
        "Stop" => {
            let continued = metadata.stop_hook_active.ok_or_else(|| {
                <serde_json::Error as serde::de::Error>::missing_field("stop_hook_active")
            })?;
            crate::turn::turn_ended(context, key, continued)
        }
        "SessionEnd" => crate::turn::session_ended(context, metadata.reason.as_deref()),
        _ => unreachable!(),
    };
    if let Some(error) = report.errors.into_iter().next() {
        return Err(error.into());
    }
    Ok(match report.action {
        crate::turn::Action::Continue { reason } => {
            json!({"decision":"block", "reason":reason}).to_string()
        }
        crate::turn::Action::Allow => allow(agent, Some(event)),
    })
}

// Never format foreign error messages: serde may quote an invalid value and
// Git/IO/core errors can carry paths or other input-derived text.
fn error_category(error: &(dyn std::error::Error + 'static)) -> &'static str {
    if error.is::<serde_json::Error>() {
        return "json";
    }
    if error.is::<std::io::Error>() {
        return "io";
    }
    if let Some(error) = error.downcast_ref::<crate::scope::GitError>() {
        return if matches!(error, crate::scope::GitError::Timeout(_)) {
            "git_timeout"
        } else {
            "git"
        };
    }
    if error.is::<crate::store::Error>() || error.is::<rusqlite::Error>() {
        return "database";
    }
    if error.is::<crate::spool::Error>() {
        return "spool";
    }
    if let Some(error) = error.downcast_ref::<crate::turn::Error>() {
        return match error {
            crate::turn::Error::Store(_) | crate::turn::Error::Sqlite(_) => "database",
            crate::turn::Error::Spool(_) => "spool",
            crate::turn::Error::Ingest(_) | crate::turn::Error::PendingUnknown => "ingest",
            _ => "core",
        };
    }
    "core"
}

fn log_error(agent: Agent, event: &str, category: &str, database: Option<&Path>) {
    let _ = (|| -> std::io::Result<()> {
        let database = match database {
            Some(path) => path.to_path_buf(),
            None => database_path().map_err(std::io::Error::other)?,
        };
        let directory = database
            .parent()
            .ok_or_else(|| std::io::Error::other("state directory"))?;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)?;
        let meta = std::fs::symlink_metadata(directory)?;
        if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
            return Ok(());
        }
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(directory.join("errors.log"))?;
        let meta = file.metadata()?;
        if !meta.is_file() || meta.uid() != unsafe { libc::geteuid() } || meta.nlink() != 1 {
            return Ok(());
        }
        // Logging must never wait behind a competing hook (especially SessionEnd).
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Ok(());
        }
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.seek(SeekFrom::Start(meta.len().saturating_sub(64 * 1024)))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<_> = text.lines().collect();
        if lines.len() >= 200 || meta.len() > 64 * 1024 {
            let keep = lines.len().saturating_sub(199);
            file.set_len(0)?;
            for line in &lines[keep..] {
                writeln!(file, "{line}")?;
            }
        }
        // Event labels are allowlisted too, so an unknown name cannot log input.
        let event = match event {
            "SessionStart" | "UserPromptSubmit" | "Stop" | "SessionEnd" | "Interrupt" => event,
            _ => "unknown",
        };
        let description = match category {
            "json" => "invalid hook JSON or metadata",
            "io" => "hook input or output unavailable",
            "database" => "database unavailable or rejected",
            "git_timeout" => "git deadline exceeded",
            "git" => "git discovery or facts failed",
            "spool" => "private spool unavailable or rejected",
            "ingest" => "spool collection incomplete or failed",
            _ => "core event handling failed",
        };
        writeln!(
            file,
            "{} {} {event} {category} {description}",
            crate::save::now(),
            agent.name()
        )
    })();
}

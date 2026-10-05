use chrono::{SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::env;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Default, Deserialize)]
struct Plan {
    session_start: Option<SessionStart>,
    stop: Option<Stop>,
}

#[derive(Deserialize)]
struct SessionStart {
    text: String,
}

#[derive(Deserialize)]
struct Stop {
    mode: StopMode,
    reason: String,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum StopMode {
    Allow,
    BlockFirst,
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "hook") {
        let _ = hook(&args[1..]);
        return ExitCode::SUCCESS;
    }
    let result = if args.first().is_some_and(|arg| arg == "save") {
        save(&args[1..])
    } else {
        Err("usage: cairn-probe hook <claude|codex> [--dir DIR] | save [--source ID] [--nothing-new]".into())
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(
                io::stderr().lock(),
                "{}",
                error.to_string().replace(['\r', '\n'], " ")
            );
            ExitCode::FAILURE
        }
    }
}

fn hook(args: &[OsString]) -> Result<()> {
    let received_at = now();
    let agent = args.first().and_then(|arg| arg.to_str()).unwrap_or("");
    if !matches!(agent, "claude" | "codex") {
        return Ok(());
    }
    let dir = match &args[1..] {
        [] => env::var_os("CAIRN_PROBE_DIR").map(PathBuf::from),
        [flag, path] if flag == "--dir" => Some(PathBuf::from(path)),
        _ => return Ok(()),
    };
    let mut bytes = Vec::new();
    let input = match io::stdin().read_to_end(&mut bytes) {
        Ok(_) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(mut input) => {
                if let Some(fields) = input.as_object_mut() {
                    for key in ["prompt", "last_assistant_message"] {
                        if let Some(value) = fields.get_mut(key) {
                            let chars = value.as_str().map_or(0, |text| text.chars().count());
                            *value = json!({"redacted": true, "chars": chars});
                        }
                    }
                }
                input
            }
            // Do not include parser messages that could quote input content.
            Err(error) => json!({"parse_error": format!(
                "invalid JSON ({:?}) at line {} column {}",
                error.classify(), error.line(), error.column()
            ), "bytes": bytes.len()}),
        },
        Err(_) => json!({"parse_error": "could not read stdin", "bytes": bytes.len()}),
    };
    let logged = dir.as_ref().is_none_or(|dir| {
        append_json(
            &dir.join("events.jsonl"),
            &json!({
                "received_at": received_at, "agent": agent, "pid": std::process::id(),
                "env": {"CAIRN_DISABLE": env::var("CAIRN_DISABLE").ok()}, "input": input
            }),
        )
        .is_ok()
    });
    // If logging failed, still apply the agent's normal allow output below.
    let plan: Plan = dir
        .as_ref()
        .filter(|_| logged)
        .and_then(|dir| fs::read(dir.join("plan.json")).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    match input["hook_event_name"].as_str() {
        Some("SessionStart") => {
            if let Some(start) = plan.session_start {
                emit(&json!({"hookSpecificOutput": {
                    "hookEventName": "SessionStart", "additionalContext": start.text
                }}))?;
            }
        }
        Some("Stop") => {
            if let (Some(stop), Some(dir), Some(session)) =
                (plan.stop, dir, input["session_id"].as_str())
            {
                if stop.mode == StopMode::BlockFirst && claim_first_stop(&dir, session).is_ok() {
                    return Ok(emit(&json!({"decision": "block", "reason": stop.reason}))?);
                }
            }
            if agent == "codex" {
                emit(&json!({}))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn append_json(path: &Path, value: &Value) -> Result<()> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(&line)?;
    Ok(())
}

fn claim_first_stop(dir: &Path, session: &str) -> io::Result<()> {
    // Hex keeps session IDs out of path syntax; create_new claims each session atomically.
    let key: String = session
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let state_dir = dir.join("blocked-sessions");
    fs::create_dir_all(&state_dir)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(state_dir.join(format!("{key}.blocked")))?;
    Ok(())
}

fn emit(value: &Value) -> io::Result<()> {
    writeln!(io::stdout().lock(), "{value}")
}

fn save(args: &[OsString]) -> Result<()> {
    let mut source = None;
    let mut nothing_new = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--source" {
            source = Some(
                args.next()
                    .and_then(|value| value.to_str())
                    .ok_or("--source requires a UTF-8 ID")?,
            );
        } else if arg == "--nothing-new" {
            nothing_new = true;
        } else {
            return Err("unknown save argument".into());
        }
    }
    let state_home = match env::var_os("XDG_STATE_HOME").filter(|path| !path.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(
            env::var_os("HOME")
                .filter(|path| !path.is_empty())
                .ok_or("HOME is not set")?,
        )
        .join(".local/state"),
    };
    let body = if nothing_new {
        None
    } else {
        let mut body = String::new();
        io::stdin().read_to_string(&mut body)?;
        Some(body)
    };
    append_json(
        &state_home.join("cairn-probe/saves.jsonl"),
        &json!({
            "saved_at": now(), "source": source, "nothing_new": nothing_new,
            "cwd": env::current_dir()?.to_string_lossy(), "body": body
        }),
    )?;
    writeln!(io::stdout().lock(), "saved")?;
    Ok(())
}

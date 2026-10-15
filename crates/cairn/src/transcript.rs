//! Read a single transcript into a sidecar excerpt (DESIGN §8.8).
//!
//! Entries use `user:`, `assistant:`, `tool_call NAME [ID]:`, and
//! `tool_result [ID]:`; embedded newlines are preserved. Only text blocks are
//! extracted from messages (not reasoning, images, or metadata). Claude tools
//! are content blocks; Codex tools are custom/function call records. Codex
//! `event_msg.item_completed.CommandExecution` also contributes its command
//! and result, with `unknown` when no ID is present. Other event mirrors are
//! skipped. Command events and response records are both retained: their IDs
//! need not match, so this parser does not guess which results are duplicates.
//!
//! Tool arguments/results retain at most 500 Unicode scalar values, including
//! a trailing `…` when shortened. The total UTF-8 budget includes labels and
//! the omission marker. After reserving marker space, one quarter goes to the
//! head and three quarters to the tail, rounded inward at UTF-8 boundaries;
//! the marker counts removed bytes of the extracted text.
//!
//! `from` is a byte offset at a record boundary, normally a previous
//! `end_offset`. Reads stop at the length observed on the opened file. A valid
//! final JSON value is consumed even without a newline; an invalid final line
//! is left unread, including its newline if any. Error line numbers start at
//! one within the requested segment. At least one nonblank user/assistant
//! text block in that segment is required; tool output alone is insufficient.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

use crate::turn::Agent;
use serde_json::Value;

/// Maximum Unicode scalar values in each tool argument/result, including `…`.
pub const TOOL_TEXT_CHARS: usize = 500;
/// Maximum bytes in the returned text, including the omission marker.
pub const EXCERPT_BYTES: usize = 60 * 1024;

#[derive(Debug)]
pub struct Excerpt {
    /// Extracted, shortened, and budgeted UTF-8 text.
    pub text: String,
    /// Byte offset after the last successfully parsed line.
    pub end_offset: u64,
    /// The opened file was shorter than `from`, so reading restarted at zero.
    pub restarted: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("transcript is not a regular file")]
    NotRegularFile,
    #[error("transcript is not owned by the current user")]
    WrongOwner,
    #[error("transcript I/O failure: {0:?}")]
    Io(std::io::ErrorKind),
    #[error("invalid JSON at segment line {line}")]
    InvalidJson { line: usize },
    #[error("no user or assistant text in transcript segment")]
    NoDialogue,
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}

/// Read only this path, checking type and effective UID on the opened handle.
/// Neither transcript content nor paths are retained in errors or written out.
pub fn extract(agent: Agent, path: &Path, from: u64) -> Result<Excerpt, Error> {
    // Nonblocking open lets us reject a FIFO without waiting for a writer.
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(Error::NotRegularFile);
    }
    // SAFETY: geteuid has no arguments or memory preconditions.
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(Error::WrongOwner);
    }
    let length = metadata.len();
    let restarted = length < from;
    let from = if restarted { 0 } else { from };
    file.seek(SeekFrom::Start(from))?;
    let mut reader = BufReader::new(file.take(length.saturating_sub(from)));
    let mut output = Output::default();
    let mut end_offset = from;
    let mut line = Vec::new();
    let mut line_number = 0;
    loop {
        line.clear();
        let read = reader.read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        line_number += 1;
        let value = match serde_json::from_slice(&line) {
            Ok(value) => value,
            Err(_) if reader.fill_buf()?.is_empty() => break,
            Err(_) => return Err(Error::InvalidJson { line: line_number }),
        };
        match agent {
            Agent::Claude => claude(&value, &mut output),
            Agent::Codex => codex(&value, &mut output),
        }
        end_offset += read as u64;
    }
    if !output.dialogue {
        return Err(Error::NoDialogue);
    }
    Ok(Excerpt {
        text: limit_excerpt(output.text),
        end_offset,
        restarted,
    })
}

#[derive(Default)]
struct Output {
    text: String,
    dialogue: bool,
}

impl Output {
    fn line(&mut self, label: &str, text: &str) {
        self.text.push_str(label);
        self.text.push_str(": ");
        self.text.push_str(text);
        self.text.push('\n');
    }

    fn dialogue(&mut self, role: &str, text: &str) {
        if !text.trim().is_empty() {
            self.dialogue = true;
            self.line(role, text);
        }
    }

    fn call(&mut self, name: &str, id: &str, input: &Value) {
        self.line(
            &format!("tool_call {name} [{id}]"),
            &shorten_tool(&value_text(input)),
        );
    }

    fn result(&mut self, id: &str, text: &str) {
        self.line(&format!("tool_result [{id}]"), &shorten_tool(text));
    }
}

fn shorten_tool(text: &str) -> String {
    if text.chars().count() <= TOOL_TEXT_CHARS {
        return text.to_owned();
    }
    let mut shortened: String = text.chars().take(TOOL_TEXT_CHARS - 1).collect();
    shortened.push('…');
    shortened
}

fn limit_excerpt(text: String) -> String {
    if text.len() <= EXCERPT_BYTES {
        return text;
    }
    // Reserve enough marker space even if the omitted count has fewer digits.
    let marker_bytes = format!("\n[... omitted {} bytes ...]\n", text.len()).len();
    let budget = EXCERPT_BYTES - marker_bytes;
    let mut head = budget / 4;
    while !text.is_char_boundary(head) {
        head -= 1;
    }
    let mut tail = text.len() - (budget - budget / 4);
    while !text.is_char_boundary(tail) {
        tail += 1;
    }
    format!(
        "{}\n[... omitted {} bytes ...]\n{}",
        &text[..head],
        tail - head,
        &text[tail..]
    )
}

fn value_text(value: &Value) -> String {
    match value.as_str() {
        Some(text) => text.to_owned(),
        None => value.to_string(),
    }
}

fn text_blocks(content: &Value, types: &[&str]) -> String {
    if let Some(text) = content.as_str() {
        return text.to_owned();
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| types.contains(&block["type"].as_str().unwrap_or("")))
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn claude(value: &Value, output: &mut Output) {
    let Some(role @ ("user" | "assistant")) = value["type"].as_str() else {
        return;
    };
    let content = &value["message"]["content"];
    if let Some(text) = content.as_str() {
        output.dialogue(role, text);
    }
    for block in content.as_array().into_iter().flatten() {
        match block["type"].as_str() {
            Some("text") => {
                if let Some(text) = block["text"].as_str() {
                    output.dialogue(role, text);
                }
            }
            Some("tool_use") => {
                if let (Some(name), Some(id), Some(input)) = (
                    block["name"].as_str(),
                    block["id"].as_str(),
                    block.get("input"),
                ) {
                    output.call(name, id, input);
                }
            }
            Some("tool_result") => {
                if let Some(id) = block["tool_use_id"].as_str() {
                    output.result(id, &text_blocks(&block["content"], &["text"]));
                }
            }
            _ => {}
        }
    }
}

fn codex(value: &Value, output: &mut Output) {
    let payload = &value["payload"];
    if value["type"] == "event_msg" && payload["type"] == "item_completed" {
        let item = &payload["item"];
        if item["type"] == "CommandExecution" {
            let id = item["id"].as_str().unwrap_or("unknown");
            if let Some(command) = item.get("command") {
                output.call("CommandExecution", id, command);
            }
            if let Some(text) = item["aggregated_output"].as_str() {
                output.result(id, text);
            }
        }
        return;
    }
    if value["type"] != "response_item" {
        return;
    }
    match payload["type"].as_str() {
        Some("message") => {
            if let Some(role @ ("user" | "assistant")) = payload["role"].as_str() {
                output.dialogue(
                    role,
                    &text_blocks(&payload["content"], &["input_text", "output_text"]),
                );
            }
        }
        Some(kind @ ("custom_tool_call" | "function_call")) => {
            let input_key = if kind == "function_call" {
                "arguments"
            } else {
                "input"
            };
            if let (Some(name), Some(id), Some(input)) = (
                payload["name"].as_str(),
                payload["call_id"].as_str(),
                payload.get(input_key),
            ) {
                output.call(name, id, input);
            }
        }
        Some("custom_tool_call_output" | "function_call_output") => {
            if let (Some(id), Some(result)) = (payload["call_id"].as_str(), payload.get("output")) {
                output.result(id, &value_text(result));
            }
        }
        _ => {}
    }
}

use std::fs;

use cairn::transcript::{extract, Error};
use cairn::turn::Agent;
use serde_json::json;
use tempfile::TempDir;

const CLAUDE: &str = include_str!("fixtures/transcripts/claude.jsonl");
const CODEX: &str = include_str!("fixtures/transcripts/codex.jsonl");

fn message(agent: Agent, role: &str, text: &str) -> String {
    let value = match agent {
        Agent::Claude => json!({"type":role,"message":{"role":role,"content":text}}),
        Agent::Codex => json!({"type":"response_item","payload":{
            "type":"message","role":role,"content":[{"type":"output_text","text":text}]
        }}),
    };
    format!("{value}\n")
}

#[test]
fn claude_extracts_dialogue_and_tools_without_metadata_or_reasoning() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("transcript.jsonl");
    fs::write(&path, CLAUDE).unwrap();
    let excerpt = extract(Agent::Claude, &path, 0).unwrap();
    assert_eq!(
        excerpt.text,
        concat!(
            "user: 检查合成文件\n",
            "assistant: 开始检查\n",
            "tool_call Read [tool-1]: {\"file_path\":\"fixture.txt\"}\n",
            "tool_result [tool-1]: 合成结果\n",
            "user: 继续\n",
            "assistant: 检查完成\n",
        )
    );
    assert_eq!(excerpt.end_offset, CLAUDE.len() as u64);
    assert!(!excerpt.restarted);
    assert_eq!(fs::read_to_string(path).unwrap(), CLAUDE);
}

#[test]
fn codex_extracts_dialogue_tools_and_last_completed_command() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("transcript.jsonl");
    fs::write(&path, CODEX).unwrap();
    let excerpt = extract(Agent::Codex, &path, 0).unwrap();
    assert_eq!(
        excerpt.text,
        concat!(
            "user: 检查合成文件\n",
            "assistant: 开始检查\n",
            "tool_call exec [call-1]: read fixture.txt\n",
            "tool_result [call-1]: 合成结果\n",
            "tool_call read [call-2]: {\"path\":\"fixture.txt\"}\n",
            "tool_result [call-2]: 第二个结果\n",
            "assistant: 检查完成\n",
            "tool_call CommandExecution [exec-3]: [\"cat\",\"third.txt\"]\n",
            "tool_result [exec-3]: 第三步已执行\n\n",
        )
    );
    assert_eq!(excerpt.end_offset, CODEX.len() as u64);
    assert!(!excerpt.restarted);
    assert_eq!(fs::read_to_string(path).unwrap(), CODEX);
}

#[test]
fn resumes_at_byte_offset_and_leaves_partial_last_line_for_next_read() {
    for agent in [Agent::Claude, Agent::Codex] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("transcript.jsonl");
        let prefix = "unparsed earlier line 中文\n";
        let first = message(agent, "user", "新增问题");
        let second = message(agent, "assistant", "新增答案");
        let partial = &second[..second.len() - 4];
        fs::write(&path, format!("{prefix}{first}{partial}")).unwrap();
        let excerpt = extract(agent, &path, prefix.len() as u64).unwrap();
        assert_eq!(excerpt.text, "user: 新增问题\n");
        assert_eq!(excerpt.end_offset, (prefix.len() + first.len()) as u64);
        assert!(!excerpt.restarted);

        // A complete final JSON value needs no trailing newline.
        fs::write(&path, format!("{prefix}{first}{}", second.trim_end())).unwrap();
        let resumed = extract(agent, &path, excerpt.end_offset).unwrap();
        assert_eq!(resumed.text, "assistant: 新增答案\n");
        assert_eq!(resumed.end_offset, fs::metadata(&path).unwrap().len());
        assert!(!resumed.restarted);
    }
}

#[test]
fn shorter_file_restarts_but_equal_length_does_not_replay_old_dialogue() {
    for agent in [Agent::Claude, Agent::Codex] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("transcript.jsonl");
        let input = message(agent, "user", "重写后");
        fs::write(&path, &input).unwrap();
        let excerpt = extract(agent, &path, input.len() as u64 + 1).unwrap();
        assert!(excerpt.restarted);
        assert_eq!(excerpt.text, "user: 重写后\n");
        assert_eq!(excerpt.end_offset, input.len() as u64);
        assert!(matches!(
            extract(agent, &path, input.len() as u64),
            Err(Error::NoDialogue)
        ));
    }
}

#[test]
fn rejects_non_regular_file_and_reports_io_without_paths() {
    for agent in [Agent::Claude, Agent::Codex] {
        let dir = TempDir::new().unwrap();
        assert!(matches!(
            extract(agent, dir.path(), 0),
            Err(Error::NotRegularFile)
        ));
        let error = extract(agent, &dir.path().join("PRIVATE_PATH"), 0).unwrap_err();
        assert!(matches!(error, Error::Io(std::io::ErrorKind::NotFound)));
        assert!(!format!("{error} {error:?}").contains("PRIVATE_PATH"));
    }
}

#[test]
fn invalid_middle_line_is_an_error_but_invalid_last_line_is_left_unread() {
    for agent in [Agent::Claude, Agent::Codex] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("transcript.jsonl");
        let first = message(agent, "user", "完整问题");
        let last = message(agent, "assistant", "完整回答");
        for broken in [b"{\"PRIVATE_CONTENT\":\n".as_slice(), b"\xff\n"] {
            let mut input = first.as_bytes().to_vec();
            input.extend_from_slice(broken);
            fs::write(&path, &input).unwrap();
            let excerpt = extract(agent, &path, 0).unwrap();
            assert_eq!(excerpt.text, "user: 完整问题\n");
            assert_eq!(excerpt.end_offset, first.len() as u64);
            input.extend_from_slice(last.as_bytes());
            fs::write(&path, &input).unwrap();
            let error = extract(agent, &path, 0).unwrap_err();
            assert!(matches!(error, Error::InvalidJson { line: 2 }));
            assert_eq!(error.to_string(), "invalid JSON at segment line 2");
            assert!(!format!("{error:?}").contains("PRIVATE_CONTENT"));
        }
    }
}

#[test]
fn unknown_rows_advance_offset_but_tools_and_empty_messages_are_not_dialogue() {
    for (agent, fixture) in [(Agent::Claude, CLAUDE), (Agent::Codex, CODEX)] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("transcript.jsonl");
        let unknown = "{\"type\":\"future_record\",\"text\":\"ignored\"}\n";
        fs::write(&path, format!("{fixture}{unknown}")).unwrap();
        let excerpt = extract(agent, &path, 0).unwrap();
        assert_eq!(excerpt.end_offset, (fixture.len() + unknown.len()) as u64);
        assert!(!excerpt.text.contains("ignored"));
        assert!(matches!(
            extract(agent, &path, fixture.len() as u64),
            Err(Error::NoDialogue)
        ));

        let tools: String = fixture
            .lines()
            .filter(|line| {
                line.contains("tool_call")
                    || line.contains("tool_result")
                    || line.contains("item_completed")
            })
            .map(|line| format!("{line}\n"))
            .collect();
        for input in [
            String::new(),
            unknown.to_owned(),
            tools,
            message(agent, "user", " \t"),
        ] {
            fs::write(&path, input).unwrap();
            assert!(matches!(extract(agent, &path, 0), Err(Error::NoDialogue)));
        }
    }
}

#[test]
fn tool_arguments_and_results_are_limited_to_500_unicode_characters() {
    for agent in [Agent::Claude, Agent::Codex] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("transcript.jsonl");
        for count in [499, 500, 501] {
            let body = "界".repeat(count);
            let (call, result) = match agent {
                Agent::Claude => (
                    json!({"type":"assistant","message":{"content":[
                        {"type":"tool_use","name":"test","id":"t","input":body}
                    ]}}),
                    json!({"type":"user","message":{"content":[
                        {"type":"tool_result","tool_use_id":"t","content":body}
                    ]}}),
                ),
                Agent::Codex => (
                    json!({"type":"response_item","payload":{
                        "type":"custom_tool_call","name":"test","call_id":"t","input":body
                    }}),
                    json!({"type":"response_item","payload":{
                        "type":"custom_tool_call_output","call_id":"t","output":body
                    }}),
                ),
            };
            let input = format!("{}{call}\n{result}\n", message(agent, "user", &body));
            fs::write(&path, input).unwrap();
            let excerpt = extract(agent, &path, 0).unwrap();
            let lines: Vec<_> = excerpt.text.lines().collect();
            assert_eq!(lines[0], format!("user: {body}"));
            let expected = if count <= 500 {
                body
            } else {
                format!("{}…", "界".repeat(499))
            };
            assert_eq!(lines[1], format!("tool_call test [t]: {expected}"));
            assert_eq!(lines[2], format!("tool_result [t]: {expected}"));
        }
    }
}

#[test]
fn excerpt_caps_utf8_bytes_and_keeps_one_quarter_head_three_quarters_tail() {
    for agent in [Agent::Claude, Agent::Codex] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("transcript.jsonl");
        for body in [
            "a".repeat(60 * 1024 - 8),
            "b".repeat(60 * 1024 - 7),
            "c".repeat(60 * 1024 - 6),
            format!("BEGIN{}END", "一😀二".repeat(10_000)),
        ] {
            let input = message(agent, "user", &body);
            let original = format!("user: {body}\n");
            fs::write(&path, &input).unwrap();
            let excerpt = extract(agent, &path, 0).unwrap();
            assert_eq!(excerpt.end_offset, input.len() as u64);
            assert!(excerpt.text.len() <= 60 * 1024);
            if original.len() <= 60 * 1024 {
                assert_eq!(excerpt.text, original);
            } else {
                let (head, rest) = excerpt.text.split_once("\n[... omitted ").unwrap();
                let (count, tail) = rest.split_once(" bytes ...]\n").unwrap();
                let omitted: usize = count.parse().unwrap();
                assert!(original.starts_with(head));
                assert!(original.ends_with(tail));
                assert_eq!(head.len() + omitted + tail.len(), original.len());
                assert!(head.len() > 15_000);
                assert!(tail.len().abs_diff(head.len() * 3) < 16);
            }
        }
    }
}

#[test]
fn codex_completed_command_without_response_output_is_shortened_and_not_dialogue() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("transcript.jsonl");
    // Minimal P1-05 shape: output can exist only in this completed-item event.
    let event = json!({"type":"event_msg","payload":{"type":"item_completed","item":{
        "type":"CommandExecution","aggregated_output":"结".repeat(600)
    }}});
    fs::write(&path, format!("{event}\n")).unwrap();
    assert!(matches!(
        extract(Agent::Codex, &path, 0),
        Err(Error::NoDialogue)
    ));
    fs::write(
        &path,
        format!("{}{event}\n", message(Agent::Codex, "user", "检查")),
    )
    .unwrap();
    let excerpt = extract(Agent::Codex, &path, 0).unwrap();
    assert_eq!(
        excerpt.text,
        format!("user: 检查\ntool_result [unknown]: {}…\n", "结".repeat(499))
    );
}

#[test]
fn claude_structured_arguments_and_result_blocks_share_the_tool_limit() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("transcript.jsonl");
    let call = json!({"type":"assistant","message":{"content":[
        {"type":"tool_use","name":"test","id":"t","input":{"command":"x".repeat(600)}}
    ]}});
    let result = json!({"type":"user","message":{"content":[
        {"type":"tool_result","tool_use_id":"t","content":[
            {"type":"text","text":"a".repeat(300)}, {"type":"text","text":"b".repeat(300)}
        ]}
    ]}});
    fs::write(
        &path,
        format!(
            "{}{call}\n{result}\n",
            message(Agent::Claude, "user", "检查")
        ),
    )
    .unwrap();
    let excerpt = extract(Agent::Claude, &path, 0).unwrap();
    assert_eq!(
        excerpt.text,
        format!(
            "user: 检查\ntool_call test [t]: {{\"command\":\"{}…\ntool_result [t]: {}\n{}…\n",
            "x".repeat(487),
            "a".repeat(300),
            "b".repeat(198)
        )
    );
}

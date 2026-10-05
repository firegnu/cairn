use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn run(root: &Path, args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cairn-probe"))
        .args(args)
        .current_dir(root)
        .env_clear()
        .env("HOME", root)
        .env("XDG_STATE_HOME", root.join("state"))
        .env("CAIRN_PROBE_DIR", root.join("probe"))
        .env("CAIRN_DISABLE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn plan(root: &Path, value: Value) {
    fs::create_dir_all(root.join("probe")).unwrap();
    fs::write(root.join("probe/plan.json"), value.to_string()).unwrap();
}

fn assert_success(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn session_start_injects_context_for_both_agents() {
    for agent in ["claude", "codex"] {
        let root = tempfile::tempdir().unwrap();
        plan(
            root.path(),
            json!({"session_start": {"text": "合成口令：山石\n下一步"}}),
        );
        let output = run(
            root.path(),
            &["hook", agent],
            r#"{"hook_event_name":"SessionStart","session_id":"synthetic-start"}"#,
        );
        assert_success(&output);
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            json!({"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": "合成口令：山石\n下一步"}})
        );
    }
}

#[test]
fn stop_blocks_once_per_session_then_allows_for_both_agents() {
    for agent in ["claude", "codex"] {
        let root = tempfile::tempdir().unwrap();
        plan(
            root.path(),
            json!({"stop": {"mode": "block_first", "reason": "合成续跑原因"}}),
        );
        for session in ["synthetic-a", "synthetic-b"] {
            let input = json!({"hook_event_name": "Stop", "session_id": session}).to_string();
            let first = run(root.path(), &["hook", agent], &input);
            assert_success(&first);
            assert_eq!(
                String::from_utf8(first.stdout).unwrap(),
                "{\"decision\":\"block\",\"reason\":\"合成续跑原因\"}\n"
            );
            let second = run(root.path(), &["hook", agent], &input);
            assert_success(&second);
            assert_eq!(
                second.stdout,
                if agent == "codex" {
                    b"{}\n".as_slice()
                } else {
                    b""
                }
            );
        }
        plan(
            root.path(),
            json!({"stop": {"mode": "allow", "reason": "不应输出"}}),
        );
        let output = run(
            root.path(),
            &["hook", agent],
            r#"{"hook_event_name":"Stop","session_id":"synthetic-allow"}"#,
        );
        assert_success(&output);
        assert_eq!(
            output.stdout,
            if agent == "codex" {
                b"{}\n".as_slice()
            } else {
                b""
            }
        );
    }
}

#[test]
fn hook_logs_redacted_fields_and_never_logs_malformed_input_verbatim() {
    for agent in ["claude", "codex"] {
        let root = tempfile::tempdir().unwrap();
        let transcript = root.path().join("absent-synthetic-transcript.jsonl");
        let input = json!({
            "hook_event_name": "UserPromptSubmit", "session_id": "synthetic-private",
            "prompt": "合成私密🔒", "last_assistant_message": "合成答复🙂",
            "transcript_path": transcript, "extra": {"unchanged": [1, true]}
        });
        let output = run(root.path(), &["hook", agent], &input.to_string());
        assert_success(&output);
        assert!(output.stdout.is_empty());
        let path = root.path().join("probe/events.jsonl");
        let raw = fs::read_to_string(&path).expect("hook must append its event log");
        assert!(!raw.contains("合成私密"));
        assert!(!raw.contains("合成答复"));
        let event: Value = serde_json::from_str(raw.trim()).unwrap();
        let mut expected = input;
        expected["prompt"] = json!({"redacted": true, "chars": 5});
        expected["last_assistant_message"] = json!({"redacted": true, "chars": 5});
        assert_eq!(event["input"], expected);
        assert_eq!(event["agent"], agent);
        assert!(event["pid"].as_u64().unwrap() > 0);
        assert_eq!(event["env"], json!({"CAIRN_DISABLE": "1"}));
        let timestamp = event["received_at"].as_str().unwrap();
        assert_eq!(timestamp.len(), 24);
        assert!(timestamp.ends_with('Z'));
        assert!(!transcript.exists());

        let broken = "{\"prompt\":\"合成坏输入🔒\", BROKEN_SECRET";
        let output = run(root.path(), &["hook", agent], broken);
        assert_success(&output);
        assert!(output.stdout.is_empty());
        let raw = fs::read_to_string(path).unwrap();
        assert!(!raw.contains("合成坏输入"));
        assert!(!raw.contains("BROKEN_SECRET"));
        let events: Vec<Value> = raw
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1]["input"]["bytes"], broken.len());
        assert!(!events[1]["input"]["parse_error"]
            .as_str()
            .unwrap()
            .is_empty());
        assert_eq!(events[1]["input"].as_object().unwrap().len(), 2);
    }
}

#[test]
fn save_appends_body_and_nothing_new_to_temporary_state_home() {
    let root = tempfile::tempdir().unwrap();
    let body = "## 停点\n合成接续内容\n";
    let output = run(root.path(), &["save", "--source", "synthetic-source"], body);
    assert_success(&output);
    assert_eq!(output.stdout, b"saved\n");
    let output = run(root.path(), &["save", "--nothing-new"], "");
    assert_success(&output);
    assert_eq!(output.stdout, b"saved\n");
    let raw = fs::read_to_string(root.path().join("state/cairn-probe/saves.jsonl")).unwrap();
    let saves: Vec<Value> = raw
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(saves.len(), 2);
    assert_eq!(saves[0]["body"], body);
    assert_eq!(saves[0]["source"], "synthetic-source");
    assert_eq!(saves[0]["nothing_new"], false);
    assert_eq!(saves[1]["body"], Value::Null);
    assert_eq!(saves[1]["source"], Value::Null);
    assert_eq!(saves[1]["nothing_new"], true);
    for save in saves {
        assert_eq!(
            save["cwd"],
            root.path().canonicalize().unwrap().to_str().unwrap()
        );
        let timestamp = save["saved_at"].as_str().unwrap();
        assert_eq!(timestamp.len(), 24);
        assert!(timestamp.ends_with('Z'));
    }
}

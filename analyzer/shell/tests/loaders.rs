use ailly_analyzer_lib::loader::{
    discover_sessions, parse_claude, parse_codex, parse_pi, DiscoveryRoots,
};
use ailly_analyzer_lib::model::{
    EventKind, Harness, ParsedSession, RelationshipKind, SourceValue, ToolCall,
};
use std::path::PathBuf;
use std::{env, fs};

/// Builds a throwaway `$HOME` tree holding one fixture per harness under the
/// default root, keyed by process id so parallel test runs never
/// collide. Fixture copy failures panic.
fn build_fake_home() -> PathBuf {
    let home = env::temp_dir().join(format!("ailly-loader-feature-test-{}", std::process::id()));
    let claude_dir = home.join(".claude/sessions");
    let codex_dir = home.join(".codex/sessions");
    let pi_dir = home.join(".pi/agent/sessions");
    fs::create_dir_all(&claude_dir).expect("create fake claude sessions root");
    fs::create_dir_all(&codex_dir).expect("create fake codex sessions root");
    fs::create_dir_all(&pi_dir).expect("create fake pi sessions root");
    fs::copy(
        "tests/fixtures/claude.jsonl",
        claude_dir.join("session.jsonl"),
    )
    .expect("copy claude fixture");
    fs::copy(
        "tests/fixtures/codex.jsonl",
        codex_dir.join("session.jsonl"),
    )
    .expect("copy codex fixture");
    fs::copy("tests/fixtures/pi.jsonl", pi_dir.join("session.jsonl")).expect("copy pi fixture");
    home
}

#[test]
fn discovers_and_normalizes_evidence_across_all_three_harnesses_without_fabrication() {
    let home = build_fake_home();

    let found = discover_sessions(&DiscoveryRoots {
        home: Some(home.clone()),
        pi_session_roots: Vec::new(),
    });
    assert_eq!(
        found.len(),
        3,
        "expected one discovered session per harness, found {found:?}"
    );

    let mut claude: Option<ParsedSession> = None;
    let mut codex: Option<ParsedSession> = None;
    let mut pi: Option<ParsedSession> = None;
    for (harness, path) in &found {
        match harness {
            Harness::ClaudeCode => claude = Some(parse_claude(path)),
            Harness::Codex => codex = Some(parse_codex(path)),
            Harness::Pi => pi = Some(parse_pi(path)),
        }
    }
    let claude = claude.expect("a Claude Code session was discovered");
    let codex = codex.expect("a Codex session was discovered");
    let pi = pi.expect("a Pi session was discovered");

    // Claude Code: tool-call evidence and the conversation-tree parent edge.
    assert!(
        claude
            .events
            .iter()
            .any(|event| event.kind == EventKind::ToolCall),
        "Claude fixture should normalize a ToolCall event"
    );
    assert!(
        claude
            .relationships
            .iter()
            .any(|edge| edge.kind == RelationshipKind::ConversationTreeParent),
        "Claude fixture should normalize a ConversationTreeParent edge"
    );

    // Codex: tool-result evidence and its edge, with no fabricated token usage.
    assert!(
        codex
            .events
            .iter()
            .any(|event| event.kind == EventKind::ToolResult),
        "Codex fixture should normalize a ToolResult event"
    );
    assert!(
        codex
            .relationships
            .iter()
            .any(|edge| edge.kind == RelationshipKind::ToolCallResult),
        "Codex fixture should normalize a ToolCallResult edge"
    );
    assert!(
        codex
            .events
            .iter()
            .all(|event| matches!(event.token_usage, SourceValue::Absent)),
        "no Codex fixture record supplies usage; every event's token_usage must stay Absent"
    );

    // Pi: session header lineage, tree edge, tool-result edge, unknown
    // catch-all, and a genuinely recorded token usage.
    assert_eq!(
        pi.session.as_ref().expect("Pi session header").native_id,
        SourceValue::Recorded("pi-1".into())
    );
    assert!(
        pi.relationships
            .iter()
            .any(|edge| edge.kind == RelationshipKind::ConversationTreeParent),
        "Pi fixture should normalize a ConversationTreeParent edge from parentId"
    );
    assert!(
        pi.relationships
            .iter()
            .any(|edge| edge.kind == RelationshipKind::ToolCallResult),
        "Pi fixture should normalize a ToolCallResult edge from toolCallId"
    );
    assert!(
        pi.events
            .iter()
            .any(|event| event.kind == EventKind::Unknown),
        "Pi's unrecognized record type should surface as an Unknown event, not be dropped"
    );
    assert!(
        pi.events
            .iter()
            .any(|event| matches!(event.token_usage, SourceValue::Recorded(_))),
        "Pi's assistant usage record should resolve to a Recorded TokenUsage"
    );

    // Every fixture carries exactly one malformed line; none of it drops the
    // rest of the file.
    assert_eq!(claude.diagnostics.len(), 1);
    assert_eq!(codex.diagnostics.len(), 1);
    assert_eq!(pi.diagnostics.len(), 1);

    fs::remove_dir_all(&home).expect("clean fake home fixture tree");
}

/// Writes `records` as consecutive JSONL lines into a throwaway file, keyed by
/// process id and name so parallel runs never collide.
fn write_transcript(name: &str, records: &[&str]) -> PathBuf {
    let path = env::temp_dir().join(format!(
        "ailly-tool-call-command-test-{}-{name}.jsonl",
        std::process::id()
    ));
    fs::write(&path, format!("{}\n", records.join("\n"))).expect("write transcript fixture");
    path
}

fn only_tool_call(parsed: &ParsedSession) -> ToolCall {
    let calls: Vec<&ToolCall> = parsed
        .events
        .iter()
        .filter(|event| event.kind == EventKind::ToolCall)
        .filter_map(|event| match &event.tool_call {
            SourceValue::Recorded(call) => Some(call),
            _ => None,
        })
        .collect();
    assert_eq!(calls.len(), 1, "expected exactly one recorded tool call");
    calls[0].clone()
}

/// `ToolCall` reaches both the SQLite index and the client as serialized JSON
/// (`tool_call_json`, mirrored by `client/src/tauri.ts`), so the working
/// directory is asserted against that serialized shape — which is also what
/// lets this test name a field the normalized model does not model yet.
fn recorded_cwd(call: &ToolCall) -> String {
    let serialized = serde_json::to_value(call).expect("ToolCall serializes");
    let cwd = serialized.get("cwd").unwrap_or_else(|| {
        panic!("normalized tool call records no working directory at all: {serialized}")
    });
    cwd.get("Recorded")
        .and_then(|value| value.as_str())
        .unwrap_or_else(|| panic!("working directory should be Recorded, got {cwd}"))
        .to_string()
}

/// Like `recorded_cwd`, this reads the serialized form so the test can name a
/// field before the normalized model models it. Returns the recorded output of
/// the session's single tool result, and the call id it answers.
fn only_recorded_result(parsed: &ParsedSession) -> (String, String) {
    let results: Vec<serde_json::Value> = parsed
        .events
        .iter()
        .filter(|event| event.kind == EventKind::ToolResult)
        .map(|event| serde_json::to_value(event).expect("Event serializes"))
        .collect();
    assert_eq!(results.len(), 1, "expected exactly one tool result event");
    let result = results[0].get("tool_result").unwrap_or_else(|| {
        panic!(
            "normalized event carries no tool result at all: {}",
            results[0]
        )
    });
    let recorded = result
        .get("Recorded")
        .unwrap_or_else(|| panic!("tool result should be Recorded, got {result}"));
    let read = |field: &str| {
        recorded
            .get(field)
            .and_then(|value| value.get("Recorded"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("tool result {field} should be Recorded, got {recorded}"))
            .to_string()
    };
    (read("output"), read("call_id"))
}

#[test]
fn a_tool_result_carries_the_output_its_harness_recorded() {
    // Claude records results as user records whose content holds tool_result
    // blocks, each naming the tool_use block it answers.
    let claude_path = write_transcript(
        "claude-result",
        &[
            r#"{"type":"assistant","uuid":"a1","sessionId":"claude-1","cwd":"/Users/dev/repo","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"cargo test"}}]}}"#,
            r#"{"type":"user","uuid":"u2","sessionId":"claude-1","cwd":"/Users/dev/repo","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","is_error":false,"content":"test result: FAILED. 1 passed; 1 failed"}]}}"#,
        ],
    );
    let (claude_output, claude_call) = only_recorded_result(&parse_claude(&claude_path));
    fs::remove_file(&claude_path).ok();

    assert_eq!(claude_output, "test result: FAILED. 1 passed; 1 failed");
    assert_eq!(claude_call, "toolu_1");

    // Codex records the output as a plain string on a function_call_output
    // payload, keyed by the call id it answers.
    let codex_path = write_transcript(
        "codex-result",
        &[
            r#"{"type":"session_meta","payload":{"id":"codex-1","cwd":"/Users/dev/repo"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"call_1","output":"Process exited with code 0\nOutput:\nok"}}"#,
        ],
    );
    let (codex_output, codex_call) = only_recorded_result(&parse_codex(&codex_path));
    fs::remove_file(&codex_path).ok();

    assert_eq!(codex_output, "Process exited with code 0\nOutput:\nok");
    assert_eq!(codex_call, "call_1");

    // Pi records the output as the content of a toolResult message.
    let pi_path = write_transcript(
        "pi-result",
        &[
            r#"{"type":"session","version":3,"id":"pi-1","cwd":"/Users/dev/repo"}"#,
            r#"{"type":"message","id":"r1","message":{"role":"toolResult","toolCallId":"call_1","content":"total 8\ndrwxr-xr-x  2 dev  staff"}}"#,
        ],
    );
    let (pi_output, pi_call) = only_recorded_result(&parse_pi(&pi_path));
    fs::remove_file(&pi_path).ok();

    assert_eq!(pi_output, "total 8\ndrwxr-xr-x  2 dev  staff");
    assert_eq!(pi_call, "call_1");
}

#[test]
fn a_shell_tool_call_records_the_command_it_ran_and_the_directory_it_ran_in() {
    // Claude nests the command at `input.command` and stamps the working
    // directory on the enclosing record, not on the tool_use block.
    let claude_path = write_transcript(
        "claude",
        &[
            r#"{"type":"user","uuid":"u1","sessionId":"claude-1","cwd":"/Users/dev/repo","message":{"role":"user","content":"run the tests"}}"#,
            r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"claude-1","cwd":"/Users/dev/repo","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"cd shell && cargo test","description":"Run the new parse_jsonl tests"}}]}}"#,
        ],
    );
    let claude = only_tool_call(&parse_claude(&claude_path));
    fs::remove_file(&claude_path).ok();

    assert_eq!(claude.name, "Bash");
    assert_eq!(
        claude.command,
        SourceValue::Recorded("cd shell && cargo test".to_string()),
        "Claude records the command at input.command; it must not read as unrecorded"
    );
    assert_eq!(recorded_cwd(&claude), "/Users/dev/repo");

    // Codex encodes `arguments` as a JSON string, names the command `cmd`, and
    // carries the per-call working directory as `workdir`.
    let codex_path = write_transcript(
        "codex",
        &[
            r#"{"type":"session_meta","payload":{"id":"codex-1","cwd":"/Users/dev/repo"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"exec_command","arguments":"{\"cmd\":\"sed -n '1,260p' src/main.rs\",\"workdir\":\"/Users/dev/other-repo\",\"yield_time_ms\":10000}"}}"#,
        ],
    );
    let codex = only_tool_call(&parse_codex(&codex_path));
    fs::remove_file(&codex_path).ok();

    assert_eq!(codex.name, "exec_command");
    assert_eq!(
        codex.command,
        SourceValue::Recorded("sed -n '1,260p' src/main.rs".to_string()),
        "Codex records the command as `cmd` inside a JSON-encoded arguments string"
    );
    assert_eq!(
        recorded_cwd(&codex),
        "/Users/dev/other-repo",
        "a Codex call's own workdir may differ from the session cwd and must win"
    );

    // Pi nests the command at `arguments.command` as a plain object, and
    // records no per-call working directory anywhere — so cwd must stay Absent
    // rather than borrowing the session header's.
    let pi_path = write_transcript(
        "pi",
        &[
            r#"{"type":"session","version":3,"id":"pi-1","cwd":"/Users/dev/repo"}"#,
            r#"{"type":"message","id":"m1","message":{"role":"assistant","content":[{"type":"toolCall","id":"call_1","name":"bash","arguments":{"command":"ls -la"}}]}}"#,
        ],
    );
    let pi = only_tool_call(&parse_pi(&pi_path));
    fs::remove_file(&pi_path).ok();

    assert_eq!(pi.name, "bash");
    assert_eq!(
        pi.command,
        SourceValue::Recorded("ls -la".to_string()),
        "Pi records the command at arguments.command"
    );
}

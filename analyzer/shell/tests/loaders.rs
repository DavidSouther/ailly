use ailly_analyzer_lib::loader::{
    discover_sessions, parse_claude, parse_codex, parse_pi, DiscoveryRoots,
};
use ailly_analyzer_lib::model::{EventKind, Harness, ParsedSession, RelationshipKind, SourceValue};
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

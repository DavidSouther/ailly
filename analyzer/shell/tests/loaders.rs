use ailly_analyzer_lib::loader::{
    discover_sessions, parse_claude, parse_codex, parse_pi, DiscoveryRoots,
};
use ailly_analyzer_lib::model::{EventKind, Harness, RelationshipKind, SourceValue};
use std::path::Path;
use std::{env, fs};

#[test]
fn claude_fixture_preserves_turn_tool_parent_and_bad_line() {
    let parsed = parse_claude(Path::new("tests/fixtures/claude.jsonl"));
    assert_eq!(parsed.events.len(), 3);
    assert!(parsed
        .events
        .iter()
        .any(|event| event.kind == EventKind::ToolCall));
    assert!(parsed
        .relationships
        .iter()
        .any(|edge| edge.kind == RelationshipKind::ConversationTreeParent));
    assert_eq!(parsed.diagnostics[0].source.line, 3);
}

#[test]
fn codex_fixture_links_a_tool_result_without_guessing_usage() {
    let parsed = parse_codex(Path::new("tests/fixtures/codex.jsonl"));
    assert!(parsed
        .events
        .iter()
        .any(|event| event.kind == EventKind::ToolResult));
    assert!(parsed
        .relationships
        .iter()
        .any(|edge| edge.kind == RelationshipKind::ToolCallResult));
    assert!(matches!(parsed.events[0].token_usage, SourceValue::Absent));
}

#[test]
fn configured_pi_roots_are_discovered_without_reading_pi_settings() {
    let root = env::temp_dir().join(format!("ailly-loader-test-{}", std::process::id()));
    let nested = root.join("nested");
    fs::create_dir_all(&nested).expect("create configured root");
    fs::write(nested.join("session.jsonl"), "{}\n").expect("fixture");

    let found = discover_sessions(&DiscoveryRoots {
        home: Some(root.join("unrelated-home")),
        pi_session_roots: vec![root.clone()],
    });
    assert!(found
        .iter()
        .any(|(harness, path)| *harness == Harness::Pi && path.ends_with("session.jsonl")));
    fs::remove_dir_all(root).expect("clean fixture");
}

#[test]
fn pi_fixture_preserves_tree_tool_linkage_tokens_and_unknown_entries() {
    let parsed = parse_pi(Path::new("tests/fixtures/pi.jsonl"));
    assert_eq!(
        parsed.session.expect("header").native_id,
        SourceValue::Recorded("pi-1".into())
    );
    assert!(parsed
        .relationships
        .iter()
        .any(|edge| edge.kind == RelationshipKind::ConversationTreeParent));
    assert!(parsed
        .relationships
        .iter()
        .any(|edge| edge.kind == RelationshipKind::ToolCallResult));
    assert!(parsed
        .events
        .iter()
        .any(|event| event.kind == EventKind::Unknown));
    assert!(parsed
        .events
        .iter()
        .any(|event| matches!(event.token_usage, SourceValue::Recorded(_))));
    assert_eq!(parsed.diagnostics[0].source.line, 7);
}

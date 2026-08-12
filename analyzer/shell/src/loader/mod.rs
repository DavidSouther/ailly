//! Read-only discovery and JSONL adapter boundary for supported harnesses.

mod claude;
mod codex;
#[cfg(test)]
pub(crate) mod conformance;
mod pi;

use crate::model::{
    Diagnostic, Event, EventKind, Harness, ParsedSession, Provenance, Relationship,
    RelationshipKind, Session, SourceValue, TokenUsage, ToolCall, Turn,
};
use serde_json::Value;
use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

pub trait HarnessAdapter {
    fn harness(&self) -> Harness;
    fn discover(&self, roots: &[PathBuf]) -> Vec<PathBuf>;
    fn parse(&self, path: &Path) -> ParsedSession;
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct DiscoveryRoots {
    pub home: Option<PathBuf>,
    /// Explicit Pi roots, normally supplied by an application preference.
    pub pi_session_roots: Vec<PathBuf>,
}

pub fn discover_sessions(roots: &DiscoveryRoots) -> Vec<(Harness, PathBuf)> {
    let home = roots
        .home
        .clone()
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from));

    let mut found = Vec::new();
    if let Some(home) = home {
        found.extend(jsonl_files(
            &home.join(".claude/sessions"),
            Harness::ClaudeCode,
        ));
        found.extend(jsonl_files(
            &home.join(".claude/projects"),
            Harness::ClaudeCode,
        ));
        found.extend(jsonl_files(&home.join(".codex/sessions"), Harness::Codex));
        found.extend(jsonl_files(&home.join(".pi/agent/sessions"), Harness::Pi));
    }
    for root in &roots.pi_session_roots {
        found.extend(jsonl_files(root, Harness::Pi));
    }

    found.sort_by(|a, b| a.1.cmp(&b.1));
    found.dedup();
    found
}

/// Recursively collects `.jsonl` files under `root`, tolerating a missing or
/// unreadable root by returning no files rather than an error.
fn jsonl_files(root: &Path, harness: Harness) -> Vec<(Harness, PathBuf)> {
    let mut files = Vec::new();
    visit_jsonl(root, harness, &mut files);
    files
}

fn visit_jsonl(path: &Path, harness: Harness, files: &mut Vec<(Harness, PathBuf)>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            visit_jsonl(&entry_path, harness, files);
        } else if entry_path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            files.push((harness, entry_path));
        }
    }
}

pub struct ClaudeCodeAdapter;
pub struct CodexAdapter;
pub struct PiAdapter;

impl HarnessAdapter for ClaudeCodeAdapter {
    fn harness(&self) -> Harness {
        Harness::ClaudeCode
    }
    fn discover(&self, roots: &[PathBuf]) -> Vec<PathBuf> {
        roots
            .iter()
            .flat_map(|root| paths_only(jsonl_files(root, self.harness())))
            .collect()
    }
    fn parse(&self, path: &Path) -> ParsedSession {
        parse_claude(path)
    }
}
impl HarnessAdapter for CodexAdapter {
    fn harness(&self) -> Harness {
        Harness::Codex
    }
    fn discover(&self, roots: &[PathBuf]) -> Vec<PathBuf> {
        roots
            .iter()
            .flat_map(|root| paths_only(jsonl_files(root, self.harness())))
            .collect()
    }
    fn parse(&self, path: &Path) -> ParsedSession {
        parse_codex(path)
    }
}
impl HarnessAdapter for PiAdapter {
    fn harness(&self) -> Harness {
        Harness::Pi
    }
    fn discover(&self, roots: &[PathBuf]) -> Vec<PathBuf> {
        roots
            .iter()
            .flat_map(|root| paths_only(jsonl_files(root, self.harness())))
            .collect()
    }
    fn parse(&self, path: &Path) -> ParsedSession {
        parse_pi(path)
    }
}

fn paths_only(items: Vec<(Harness, PathBuf)>) -> impl Iterator<Item = PathBuf> {
    items.into_iter().map(|(_, path)| path)
}

pub fn parse_claude(path: &Path) -> ParsedSession {
    parse_jsonl(path, Harness::ClaudeCode, claude::record)
}
pub fn parse_codex(path: &Path) -> ParsedSession {
    parse_jsonl(path, Harness::Codex, codex::record)
}
pub fn parse_pi(path: &Path) -> ParsedSession {
    parse_jsonl(path, Harness::Pi, pi::record)
}

pub(crate) type RecordParser = fn(&Value, &mut ParsedSession, &str, Provenance);
fn parse_jsonl(path: &Path, harness: Harness, parser: RecordParser) -> ParsedSession {
    let mut parsed = ParsedSession::default();
    let path_str = path.to_string_lossy().to_string();

    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(err) => {
            parsed.diagnostics.push(Diagnostic {
                source: provenance(harness, &path_str, 0),
                message: format!("could not open file: {err}"),
            });
            return parsed;
        }
    };

    for (index, line) in io::BufReader::new(file).lines().enumerate() {
        let source = provenance(harness, &path_str, index + 1);
        match line {
            Ok(text) => {
                if text.trim().is_empty() {
                    parsed.diagnostics.push(Diagnostic {
                        source,
                        message: "empty record".to_string(),
                    });
                    continue;
                }
                match serde_json::from_str::<Value>(&text) {
                    Ok(value) => parser(&value, &mut parsed, &path_str, source),
                    Err(err) => parsed.diagnostics.push(Diagnostic {
                        source,
                        message: format!("malformed JSON: {err}"),
                    }),
                }
            }
            Err(err) => parsed.diagnostics.push(Diagnostic {
                source,
                message: format!("could not read record: {err}"),
            }),
        }
    }

    parsed
}

fn provenance(harness: Harness, path: &str, line: usize) -> Provenance {
    Provenance {
        harness,
        path: path.to_string(),
        line,
        ordinal: line,
    }
}
/// Looks up the first present name and returns its string value, distinguishing "absent"
/// (no name present) from "malformed" (present but not a string).
pub(crate) fn string(value: &Value, names: &[&str]) -> SourceValue<String> {
    for name in names {
        if let Some(item) = value.get(*name) {
            return item
                .as_str()
                .map(|text| SourceValue::Recorded(text.to_string()))
                .unwrap_or(SourceValue::Malformed);
        }
    }
    SourceValue::Absent
}

/// Convenience over `string` for call sites that only need the recorded case, treating
/// absent/malformed/unsupported alike as "nothing usable here".
pub(crate) fn recorded_string(value: &Value, names: &[&str]) -> Option<String> {
    match string(value, names) {
        SourceValue::Recorded(value) => Some(value),
        _ => None,
    }
}

pub(crate) fn number(value: Option<&Value>) -> SourceValue<u64> {
    value
        .and_then(Value::as_u64)
        .map(SourceValue::Recorded)
        .unwrap_or(SourceValue::Absent)
}

pub(crate) fn session_id(harness: Harness, path: &str, native: Option<String>) -> String {
    format!(
        "{harness:?}:{path}:{}",
        native.unwrap_or_else(|| "source".to_string())
    )
}

pub(crate) fn event_id(session: &str, source: &Provenance, native: &SourceValue<String>) -> String {
    let key = match native {
        SourceValue::Recorded(id) => id.clone(),
        _ => "native".to_string(),
    };
    format!("{session}:{}:{}", source.line, key)
}

pub(crate) fn event(
    session: &str,
    kind: EventKind,
    source: Provenance,
    native_id: SourceValue<String>,
) -> Event {
    Event {
        id: event_id(session, &source, &native_id),
        session_id: session.to_string(),
        kind,
        source,
        native_id,
        timestamp: SourceValue::Absent,
        turn: SourceValue::Absent,
        tool_call: SourceValue::Absent,
        token_usage: SourceValue::Absent,
        files: SourceValue::Absent,
        detail: SourceValue::Absent,
    }
}

/// Pushes one `Relationship` from `from_event_id` to `to_native_id` when the harness record
/// actually supplied that native id. Every adapter's conversation-tree-parent and
/// tool-call-result edges share exactly this "if the source recorded the target id, link to
/// it" shape; this collects the four real call sites (Claude's parent edge, Codex's and Pi's
/// tool-result edges, Pi's parent edge) into one place instead of repeating the `if let
/// Some(..) { parsed.relationships.push(Relationship { .. }) }` block per edge.
pub(crate) fn push_relationship_if_present(
    parsed: &mut ParsedSession,
    kind: RelationshipKind,
    from_event_id: String,
    to_native_id: Option<String>,
    source: Provenance,
) {
    if let Some(to_native_id) = to_native_id {
        parsed.relationships.push(Relationship {
            kind,
            from_event_id,
            to_native_id,
            source,
        });
    }
}

pub(crate) fn usage(value: Option<&Value>, scope: &str) -> SourceValue<TokenUsage> {
    let Some(value) = value else {
        return SourceValue::Absent;
    };
    SourceValue::Recorded(TokenUsage {
        input: number(
            value
                .get("input_tokens")
                .or_else(|| value.get("inputTokens")),
        ),
        output: number(
            value
                .get("output_tokens")
                .or_else(|| value.get("outputTokens")),
        ),
        cache_read: number(
            value
                .get("cache_read_input_tokens")
                .or_else(|| value.get("cacheReadTokens")),
        ),
        cache_write: number(
            value
                .get("cache_creation_input_tokens")
                .or_else(|| value.get("cacheWriteTokens")),
        ),
        total: number(
            value
                .get("total_tokens")
                .or_else(|| value.get("totalTokens")),
        ),
        scope: scope.to_string(),
    })
}

pub(crate) fn content_text(content: Option<&Value>) -> SourceValue<String> {
    match content {
        Some(Value::String(text)) => SourceValue::Recorded(text.clone()),
        Some(Value::Array(blocks)) => {
            let text = blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() {
                SourceValue::Unsupported
            } else {
                SourceValue::Recorded(text)
            }
        }
        Some(_) => SourceValue::Malformed,
        None => SourceValue::Absent,
    }
}
/// Builds a `ToolCall` from a harness-specific record whose call-name and call-input fields
/// vary by name (`name`/`input` for Claude, `name`/`arguments` for Codex, etc).
pub(crate) fn tool_call(value: &Value, name: &str, input: &str) -> ToolCall {
    let input_value = value.get(input);
    let recorded_input = input_value
        .map(Value::to_string)
        .map(SourceValue::Recorded)
        .unwrap_or(SourceValue::Absent);
    let path = input_value
        .and_then(|arguments| arguments.get("path").or_else(|| arguments.get("file_path")))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| input_value.and_then(Value::as_str).and_then(extract_path))
        .or_else(|| recorded_string(value, &["path", "file_path"]))
        .map(SourceValue::Recorded)
        .unwrap_or(SourceValue::Absent);
    ToolCall {
        name: recorded_string(value, &[name]).unwrap_or_else(|| "unknown".to_string()),
        call_id: string(value, &["id", "call_id"]),
        input: recorded_input,
        command: string(value, &["command"]),
        path,
        url: string(value, &["url"]),
    }
}

/// Codex `function_call` arguments sometimes arrive as a JSON-encoded string rather than an
/// object; this pulls a `path` out of that string form without treating it as malformed.
pub(crate) fn extract_path(input: &str) -> Option<String> {
    serde_json::from_str::<Value>(input)
        .ok()?
        .get("path")?
        .as_str()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_file(name: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "ailly-parse-jsonl-test-{}-{}-{}",
            std::process::id(),
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after epoch")
                .as_nanos()
        ));
        let mut file = fs::File::create(&path).expect("create temp fixture file");
        file.write_all(contents.as_bytes())
            .expect("write temp fixture file");
        path
    }

    fn noop(_value: &Value, _parsed: &mut ParsedSession, _path: &str, _source: Provenance) {}

    fn unique_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ailly-discover-sessions-test-{}-{}-{}",
            std::process::id(),
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn a_home_with_no_harness_folders_returns_an_empty_result_without_panicking() {
        let home = unique_temp_dir("empty-home");
        fs::create_dir_all(&home).expect("create empty home");

        let found = discover_sessions(&DiscoveryRoots {
            home: Some(home.clone()),
            pi_session_roots: Vec::new(),
        });

        assert_eq!(found, Vec::new());
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn claude_sessions_and_projects_both_surface_as_claude_code_recursively() {
        let home = unique_temp_dir("claude-both-roots");
        let sessions_nested = home.join(".claude/sessions/nested");
        let projects_nested = home.join(".claude/projects/proj/nested");
        fs::create_dir_all(&sessions_nested).expect("create sessions nested dir");
        fs::create_dir_all(&projects_nested).expect("create projects nested dir");
        fs::write(sessions_nested.join("a.jsonl"), "{}").expect("write sessions fixture");
        fs::write(projects_nested.join("b.jsonl"), "{}").expect("write projects fixture");

        let found = discover_sessions(&DiscoveryRoots {
            home: Some(home.clone()),
            pi_session_roots: Vec::new(),
        });

        assert_eq!(found.len(), 2);
        assert!(found
            .iter()
            .all(|(harness, _)| *harness == Harness::ClaudeCode));
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_non_jsonl_file_in_a_session_root_is_not_returned() {
        let home = unique_temp_dir("non-jsonl");
        let sessions = home.join(".claude/sessions");
        fs::create_dir_all(&sessions).expect("create sessions dir");
        fs::write(sessions.join("notes.txt"), "not jsonl").expect("write non-jsonl file");
        fs::write(sessions.join("session.jsonl"), "{}").expect("write jsonl file");

        let found = discover_sessions(&DiscoveryRoots {
            home: Some(home.clone()),
            pi_session_roots: Vec::new(),
        });

        assert_eq!(found.len(), 1);
        assert!(found[0].1.ends_with("session.jsonl"));
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_root_listed_twice_is_deduplicated_in_the_result() {
        let home = unique_temp_dir("empty-home-for-dedup");
        fs::create_dir_all(&home).expect("create empty home");
        let pi_root = unique_temp_dir("pi-dup-root");
        fs::create_dir_all(&pi_root).expect("create pi root");
        fs::write(pi_root.join("session.jsonl"), "{}").expect("write jsonl file");

        let found = discover_sessions(&DiscoveryRoots {
            home: Some(home.clone()),
            pi_session_roots: vec![pi_root.clone(), pi_root.clone()],
        });

        assert_eq!(found, vec![(Harness::Pi, pi_root.join("session.jsonl"))]);
        fs::remove_dir_all(&home).ok();
        fs::remove_dir_all(&pi_root).ok();
    }

    #[test]
    fn configured_pi_roots_are_discovered_without_reading_pi_settings() {
        let home = unique_temp_dir("empty-home-for-pi-roots");
        fs::create_dir_all(&home).expect("create empty home");
        let pi_root = unique_temp_dir("configured-pi-root");
        fs::create_dir_all(&pi_root).expect("create configured pi root");
        fs::write(pi_root.join("session.jsonl"), "{}").expect("write jsonl file");

        let found = discover_sessions(&DiscoveryRoots {
            home: Some(home.clone()),
            pi_session_roots: vec![pi_root.clone()],
        });

        assert_eq!(found, vec![(Harness::Pi, pi_root.join("session.jsonl"))]);
        fs::remove_dir_all(&home).ok();
        fs::remove_dir_all(&pi_root).ok();
    }

    #[test]
    fn one_malformed_line_becomes_a_diagnostic_without_dropping_the_file() {
        let path = write_temp_file(
            "malformed-line",
            "{\"type\":\"a\"}\nnot-json\n{\"type\":\"b\"}\n",
        );

        let parsed = parse_jsonl(&path, Harness::ClaudeCode, noop);

        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.diagnostics[0].source.line, 2);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn empty_or_whitespace_only_line_becomes_a_diagnostic_not_a_silent_skip() {
        let path = write_temp_file("blank-line", "{\"type\":\"a\"}\n   \n{\"type\":\"b\"}\n");

        let parsed = parse_jsonl(&path, Harness::ClaudeCode, noop);

        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.diagnostics[0].source.line, 2);
        assert_eq!(parsed.diagnostics[0].message, "empty record");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn unreadable_file_produces_exactly_one_diagnostic_at_line_zero_without_panicking() {
        let path = std::env::temp_dir().join(format!(
            "ailly-parse-jsonl-test-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after epoch")
                .as_nanos()
        ));
        let _ = fs::remove_file(&path);

        let parsed = parse_jsonl(&path, Harness::ClaudeCode, noop);

        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.diagnostics[0].source.line, 0);
    }

    #[test]
    fn a_well_formed_file_with_zero_bad_lines_produces_zero_diagnostics() {
        let path = write_temp_file("clean-file", "{\"type\":\"a\"}\n{\"type\":\"b\"}\n");

        let parsed = parse_jsonl(&path, Harness::ClaudeCode, noop);

        assert_eq!(parsed.diagnostics.len(), 0);
        fs::remove_file(&path).ok();
    }
}

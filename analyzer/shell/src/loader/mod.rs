//! Read-only discovery and JSONL adapter boundary for supported harnesses.

mod claude;
mod codex;
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

#[derive(Clone, Debug, Default)]
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
    found.sort_by(|left, right| left.1.cmp(&right.1));
    found.dedup();
    found
}

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
        let path = entry.path();
        if path.is_dir() {
            visit_jsonl(&path, harness, files);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
        {
            files.push((harness, path));
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

type RecordParser = fn(&Value, &mut ParsedSession, &str, Provenance);
fn parse_jsonl(path: &Path, harness: Harness, parser: RecordParser) -> ParsedSession {
    let path_text = path.to_string_lossy().into_owned();
    let mut parsed = ParsedSession::default();
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) => {
            parsed.diagnostics.push(Diagnostic {
                source: provenance(harness, &path_text, 0),
                message: format!("could not read source: {error}"),
            });
            return parsed;
        }
    };
    for (index, line) in io::BufReader::new(file).lines().enumerate() {
        let source = provenance(harness, &path_text, index + 1);
        match line {
            Ok(line) if line.trim().is_empty() => parsed.diagnostics.push(Diagnostic {
                source,
                message: "empty record".into(),
            }),
            Ok(line) => match serde_json::from_str::<Value>(&line) {
                Ok(record) => parser(&record, &mut parsed, &path_text, source),
                Err(error) => parsed.diagnostics.push(Diagnostic {
                    source,
                    message: format!("malformed JSON: {error}"),
                }),
            },
            Err(error) => parsed.diagnostics.push(Diagnostic {
                source,
                message: format!("could not read record: {error}"),
            }),
        }
    }
    if parsed.session.is_none() {
        parsed.session = Some(fallback_session(harness, &path_text));
    }
    parsed
}

fn provenance(harness: Harness, path: &str, line: usize) -> Provenance {
    Provenance {
        harness,
        path: path.into(),
        line,
        ordinal: line,
    }
}
fn fallback_session(harness: Harness, path: &str) -> Session {
    Session {
        id: format!("{:?}:{path}", harness),
        harness,
        source: provenance(harness, path, 0),
        native_id: SourceValue::Absent,
        project: SourceValue::Absent,
        parent_session: SourceValue::Absent,
    }
}
pub(crate) fn string(value: &Value, names: &[&str]) -> SourceValue<String> {
    for name in names {
        if let Some(item) = value.get(*name) {
            return item
                .as_str()
                .map(|text| SourceValue::Recorded(text.into()))
                .unwrap_or(SourceValue::Malformed);
        }
    }
    SourceValue::Absent
}
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
        "{:?}:{path}:{}",
        harness,
        native.unwrap_or_else(|| "source".into())
    )
}
pub(crate) fn event_id(session: &str, source: &Provenance, native: &SourceValue<String>) -> String {
    let key = match native {
        SourceValue::Recorded(id) => id.clone(),
        _ => source.line.to_string(),
    };
    format!("{session}:{key}")
}
pub(crate) fn event(
    session: &str,
    kind: EventKind,
    source: Provenance,
    native_id: SourceValue<String>,
) -> Event {
    Event {
        id: event_id(session, &source, &native_id),
        session_id: session.into(),
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
        scope: scope.into(),
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
pub(crate) fn tool_call(value: &Value, name: &str, input: &str) -> ToolCall {
    let input_value = value
        .get(input)
        .map(Value::to_string)
        .map(SourceValue::Recorded)
        .unwrap_or(SourceValue::Absent);
    let input_text = value.get(input).and_then(Value::as_str);
    let path = value
        .get(input)
        .and_then(|arguments| arguments.get("path").or_else(|| arguments.get("file_path")))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| input_text.and_then(extract_path))
        .or_else(|| recorded_string(value, &["path", "file_path"]))
        .map(SourceValue::Recorded)
        .unwrap_or(SourceValue::Absent);
    ToolCall {
        name: recorded_string(value, &[name]).unwrap_or_else(|| "unknown".into()),
        call_id: string(value, &["id", "call_id"]),
        input: input_value,
        command: string(value, &["command"]),
        path,
        url: string(value, &["url"]),
    }
}
pub(crate) fn extract_path(input: &str) -> Option<String> {
    serde_json::from_str::<Value>(input)
        .ok()?
        .get("path")?
        .as_str()
        .map(str::to_owned)
}

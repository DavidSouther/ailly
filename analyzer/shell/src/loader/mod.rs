//! Read-only discovery and JSONL adapter boundary for supported harnesses.

mod claude;
mod codex;
#[cfg(test)]
pub(crate) mod conformance;
mod pi;

use crate::model::{
    Diagnostic, Event, EventKind, Harness, ParsedSession, Provenance, Relationship,
    RelationshipKind, Session, SourceValue, Subagent, TokenUsage, ToolCall, ToolResult, Turn,
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

/// The streaming state one adapter carries between the records of a single
/// file, for facts a harness records on a different record from the one they
/// describe. Only Codex needs it today: it names the model on a `turn_context`
/// record and reports usage on a `token_count` record that names none.
#[derive(Debug, Default)]
pub(crate) struct AdapterState {
    last_model: Option<String>,
}

impl AdapterState {
    /// Remembers a model a record just named. Only a recorded id is kept: a
    /// malformed one must not be carried onto a later record as though the
    /// transcript had named it.
    pub(crate) fn name_model(&mut self, model: SourceValue<String>) {
        if let SourceValue::Recorded(id) = model {
            self.last_model = Some(id);
        }
    }

    /// The model to stamp on a usage record that named none. Absent until some
    /// earlier record in the same file named one — never the session's most
    /// common model, and never a hardcoded default. Measured over 804 local
    /// Codex transcripts, 44 carry at least one `token_count` reached before
    /// any model was named, so this case is real rather than defensive.
    pub(crate) fn carried_model(&self) -> SourceValue<String> {
        match &self.last_model {
            Some(id) => SourceValue::Recorded(id.clone()),
            None => SourceValue::Absent,
        }
    }
}

pub(crate) type RecordParser = fn(&Value, &mut ParsedSession, &mut AdapterState, &str, Provenance);
fn parse_jsonl(path: &Path, harness: Harness, parser: RecordParser) -> ParsedSession {
    let mut parsed = ParsedSession::default();
    let mut state = AdapterState::default();
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
                    Ok(value) => parser(&value, &mut parsed, &mut state, &path_str, source),
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

/// The value a `SourceValue` recorded, if it recorded one. Adapters use this
/// wherever a downstream call wants a plain `Option` — chiefly the "link only
/// what the source named" relationship helper below.
pub(crate) fn recorded<T: Clone>(value: &SourceValue<T>) -> Option<T> {
    match value {
        SourceValue::Recorded(value) => Some(value.clone()),
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
    timestamp: SourceValue<String>,
) -> Event {
    Event {
        id: event_id(session, &source, &native_id),
        session_id: session.to_string(),
        kind,
        source,
        native_id,
        response_id: SourceValue::Absent,
        model: SourceValue::Absent,
        timestamp,
        turn: SourceValue::Absent,
        tool_call: SourceValue::Absent,
        tool_result: SourceValue::Absent,
        token_usage: SourceValue::Absent,
        files: SourceValue::Absent,
        detail: SourceValue::Absent,
        subagent: SourceValue::Absent,
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

/// Reads whichever of `keys` the record actually used, so one reader serves all
/// three harnesses' spellings of the same figure.
fn first_number(value: &Value, keys: &[&str]) -> SourceValue<u64> {
    number(keys.iter().find_map(|key| value.get(key)))
}

/// Every harness names the same five figures differently, and each list below is
/// the set of spellings seen in real transcripts for one figure. A name absent
/// from these lists reads Absent, never zero.
///
/// The figures are stored exactly as recorded. Codex counts its cache figures
/// *inside* `input_tokens` while Claude and Pi count theirs beside it — see the
/// membership tests in each adapter — and reconciling that difference is the
/// client's job, since only the client knows the harness it is folding.
pub(crate) fn usage(value: Option<&Value>, scope: &str) -> SourceValue<TokenUsage> {
    let Some(value) = value else {
        return SourceValue::Absent;
    };
    SourceValue::Recorded(TokenUsage {
        input: first_number(value, &["input_tokens", "inputTokens", "input"]),
        output: first_number(value, &["output_tokens", "outputTokens", "output"]),
        cache_read: first_number(
            value,
            &[
                "cache_read_input_tokens",
                "cached_input_tokens",
                "cacheReadTokens",
                "cacheRead",
            ],
        ),
        cache_write: first_number(
            value,
            &[
                "cache_creation_input_tokens",
                "cache_write_input_tokens",
                "cacheWriteTokens",
                "cacheWrite",
            ],
        ),
        total: first_number(value, &["total_tokens", "totalTokens"]),
        cost_total_micros: cost_micros(value),
        scope: scope.to_string(),
    })
}

/// The dollar figure a usage record priced itself at, as millionths of a US
/// dollar. Only Pi writes one, nested as `cost.total` beside the token buckets
/// its four siblings price, so Claude's and Codex's records resolve Absent here
/// without a per-harness branch.
///
/// The conversion rounds half away from zero, so a stored figure is at most
/// half a millionth of a dollar from the one the transcript wrote. A `cost.total`
/// that is present but not a non-negative finite number is Malformed: a price
/// nobody can read is not a price of zero.
fn cost_micros(usage: &Value) -> SourceValue<u64> {
    let Some(total) = usage.get("cost").and_then(|cost| cost.get("total")) else {
        return SourceValue::Absent;
    };
    match total.as_f64() {
        Some(usd) if usd.is_finite() && usd >= 0.0 => {
            SourceValue::Recorded((usd * 1_000_000.0).round() as u64)
        }
        _ => SourceValue::Malformed,
    }
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
/// Finds a named field on a call, checking in order: the call's input payload as an object,
/// that payload decoded from a JSON-encoded string (Codex's `arguments` shape), then a
/// top-level key on the record itself. Returns an owned value, because the decoded form
/// borrows from a `Value` this function parses and drops.
pub(crate) fn find_input_field(value: &Value, input_key: &str, names: &[&str]) -> Option<Value> {
    fn first_named(container: &Value, names: &[&str]) -> Option<Value> {
        names.iter().find_map(|name| container.get(*name).cloned())
    }

    value
        .get(input_key)
        .and_then(|payload| {
            first_named(payload, names).or_else(|| {
                let decoded = serde_json::from_str::<Value>(payload.as_str()?).ok()?;
                first_named(&decoded, names)
            })
        })
        .or_else(|| first_named(value, names))
}

/// Coerces a found field into a string `SourceValue`, matching `string`'s Recorded/Malformed/
/// Absent semantics.
pub(crate) fn string_field(found: Option<Value>) -> SourceValue<String> {
    match found {
        Some(Value::String(text)) => SourceValue::Recorded(text),
        Some(_) => SourceValue::Malformed,
        None => SourceValue::Absent,
    }
}

/// Coerces a found field into a command `SourceValue`: a string is Recorded verbatim, an
/// array of argument strings is Recorded as one joined line, and anything else present is
/// Malformed.
pub(crate) fn command_field(found: Option<Value>) -> SourceValue<String> {
    match found {
        Some(Value::Array(parts)) => parts
            .iter()
            .map(Value::as_str)
            .collect::<Option<Vec<_>>>()
            .map(|parts| SourceValue::Recorded(parts.join(" ")))
            .unwrap_or(SourceValue::Malformed),
        other => string_field(other),
    }
}

/// Builds a `ToolCall` from a harness-specific record whose call-name and call-input fields
/// vary by name (`name`/`input` for Claude, `name`/`arguments` for Codex, etc). `record_cwd`
/// is the working directory of the record enclosing the call, which only the adapter that can
/// see that record is able to supply.
pub(crate) fn tool_call(
    value: &Value,
    name: &str,
    input: &str,
    record_cwd: SourceValue<String>,
) -> ToolCall {
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
    // Only a call that recorded nothing at all borrows the enclosing record's
    // directory; a malformed one keeps saying so.
    let cwd = match string_field(find_input_field(value, input, &["workdir", "cwd"])) {
        SourceValue::Absent => record_cwd,
        payload_cwd => payload_cwd,
    };
    ToolCall {
        name: recorded_string(value, &[name]).unwrap_or_else(|| "unknown".to_string()),
        call_id: string(value, &["id", "call_id"]),
        input: recorded_input,
        command: command_field(find_input_field(value, input, &["command", "cmd"])),
        path,
        url: string_field(find_input_field(value, input, &["url"])),
        cwd,
    }
}

/// Builds a `ToolResult` from a harness-specific result record. `call_id_names` are the
/// keys that can name the call being answered, and `output_name` the key holding what the
/// call returned — a plain string for Codex and Pi, or Claude's array of content blocks,
/// both of which `content_text` already reads.
pub(crate) fn tool_result(value: &Value, call_id_names: &[&str], output_name: &str) -> ToolResult {
    ToolResult {
        call_id: string(value, call_id_names),
        output: content_text(value.get(output_name)),
        is_error: bool_field(value.get("is_error")),
    }
}

/// Coerces a found field into a boolean `SourceValue`, matching `string`'s
/// Recorded/Malformed/Absent semantics.
pub(crate) fn bool_field(found: Option<&Value>) -> SourceValue<bool> {
    match found {
        Some(Value::Bool(flag)) => SourceValue::Recorded(*flag),
        Some(_) => SourceValue::Malformed,
        None => SourceValue::Absent,
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
    use serde_json::json;
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

    fn noop(
        _value: &Value,
        _parsed: &mut ParsedSession,
        _state: &mut AdapterState,
        _path: &str,
        _source: Provenance,
    ) {
    }

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

    #[test]
    fn a_command_nested_in_the_input_payload_resolves_recorded() {
        let value = json!({"name": "Bash", "input": {"command": "cd shell && cargo test", "description": "run them"}});

        let call = tool_call(&value, "name", "input", SourceValue::Absent);

        assert_eq!(
            call.command,
            SourceValue::Recorded("cd shell && cargo test".to_string())
        );
    }

    #[test]
    fn a_command_spelled_cmd_inside_a_json_encoded_payload_resolves_recorded() {
        let value = json!({
            "name": "exec_command",
            "arguments": "{\"cmd\":\"sed -n '1,260p' src/main.rs\",\"workdir\":\"/other\"}"
        });

        let call = tool_call(&value, "name", "arguments", SourceValue::Absent);

        assert_eq!(
            call.command,
            SourceValue::Recorded("sed -n '1,260p' src/main.rs".to_string())
        );
    }

    #[test]
    fn a_top_level_command_key_still_resolves_when_the_payload_has_none() {
        let value = json!({"name": "Bash", "command": "ls", "input": {"description": "list"}});

        let call = tool_call(&value, "name", "input", SourceValue::Absent);

        assert_eq!(call.command, SourceValue::Recorded("ls".to_string()));
    }

    #[test]
    fn a_command_that_is_neither_a_string_nor_a_string_array_is_malformed_not_absent() {
        let value = json!({"name": "Bash", "input": {"command": {"argv": ["ls"]}}});

        let call = tool_call(&value, "name", "input", SourceValue::Absent);

        // A value was present, it just was not usable; that is not the same
        // fact as the harness recording nothing.
        assert_eq!(call.command, SourceValue::Malformed);
    }

    #[test]
    fn an_argv_array_command_joins_into_one_displayable_line() {
        let value = json!({"name": "Bash", "input": {"command": ["bash", "-lc", "echo hi"]}});

        let call = tool_call(&value, "name", "input", SourceValue::Absent);

        assert_eq!(
            call.command,
            SourceValue::Recorded("bash -lc echo hi".to_string())
        );
    }

    #[test]
    fn a_payload_string_that_is_not_json_leaves_command_absent_but_keeps_input_verbatim() {
        let value =
            json!({"name": "exec", "input": "const r = await tools.exec_command({cmd:\"ls\"});"});

        let call = tool_call(&value, "name", "input", SourceValue::Absent);

        assert_eq!(call.command, SourceValue::Absent);
        assert_eq!(
            call.input,
            SourceValue::Recorded(
                "\"const r = await tools.exec_command({cmd:\\\"ls\\\"});\"".to_string()
            )
        );
    }

    #[test]
    fn a_calls_own_workdir_wins_over_the_enclosing_records_directory() {
        let value = json!({
            "name": "exec_command",
            "arguments": "{\"cmd\":\"ls\",\"workdir\":\"/Users/dev/other-repo\"}"
        });

        let call = tool_call(
            &value,
            "name",
            "arguments",
            SourceValue::Recorded("/Users/dev/repo".to_string()),
        );

        assert_eq!(
            call.cwd,
            SourceValue::Recorded("/Users/dev/other-repo".to_string())
        );
    }

    #[test]
    fn a_call_with_no_recorded_directory_falls_back_to_the_enclosing_records() {
        let value = json!({"name": "Bash", "input": {"command": "ls"}});

        let call = tool_call(
            &value,
            "name",
            "input",
            SourceValue::Recorded("/Users/dev/repo".to_string()),
        );

        assert_eq!(
            call.cwd,
            SourceValue::Recorded("/Users/dev/repo".to_string())
        );
    }

    #[test]
    fn a_malformed_payload_directory_does_not_fall_back_to_the_enclosing_records() {
        let value = json!({"name": "exec_command", "input": {"workdir": 17}});

        let call = tool_call(
            &value,
            "name",
            "input",
            SourceValue::Recorded("/Users/dev/repo".to_string()),
        );

        // The call recorded a directory; it was just unusable. Substituting the
        // enclosing record's would report a fact the call never made.
        assert_eq!(call.cwd, SourceValue::Malformed);
    }

    /// Pi's own figures run to seven decimal places, so the stored integer has
    /// to keep the fractions cents would erase. `0.0011264` is a real cache-read
    /// charge from a local transcript; rounded to cents it would read as free.
    #[test]
    fn a_fractional_dollar_cost_survives_as_millionths_rather_than_rounding_to_cents() {
        let cases = [
            (0.0178704_f64, 17870_u64),
            (0.0011264, 1126),
            (0.013192, 13192),
            (0.0, 0),
        ];
        for (usd, expected_micros) in cases {
            let value = json!({"input": 6596, "output": 296, "cost": {"total": usd}});

            let SourceValue::Recorded(usage) = usage(Some(&value), "message") else {
                panic!("expected recorded usage");
            };

            assert_eq!(
                usage.cost_total_micros,
                SourceValue::Recorded(expected_micros),
                "${usd} should store as {expected_micros} millionths"
            );
        }
    }

    /// A harness that priced nothing must not read as having priced zero, which
    /// is what every Claude and Codex record does.
    #[test]
    fn a_usage_record_with_no_cost_object_records_no_cost_rather_than_zero() {
        let value = json!({"input_tokens": 2, "output_tokens": 109});

        let SourceValue::Recorded(usage) = usage(Some(&value), "message") else {
            panic!("expected recorded usage");
        };

        assert_eq!(usage.cost_total_micros, SourceValue::Absent);
    }

    #[test]
    fn a_cost_total_that_is_not_a_readable_price_is_malformed_not_zero() {
        for unreadable in [json!("0.0178704"), json!(-0.5), json!(null)] {
            let value = json!({"input": 10, "cost": {"total": unreadable}});

            let SourceValue::Recorded(usage) = usage(Some(&value), "message") else {
                panic!("expected recorded usage");
            };

            assert_eq!(
                usage.cost_total_micros,
                SourceValue::Malformed,
                "{unreadable} is not a price, and is not a price of zero either"
            );
        }
    }

    /// A model named by an earlier record is carried onto later ones; a
    /// malformed one is not, because carrying it would report a model the
    /// transcript never named.
    #[test]
    fn only_a_recorded_model_is_carried_onto_a_later_record() {
        let mut state = AdapterState::default();
        assert_eq!(state.carried_model(), SourceValue::Absent);

        state.name_model(SourceValue::Recorded("gpt-5.6-terra".to_string()));
        assert_eq!(
            state.carried_model(),
            SourceValue::Recorded("gpt-5.6-terra".to_string())
        );

        state.name_model(SourceValue::Malformed);
        assert_eq!(
            state.carried_model(),
            SourceValue::Recorded("gpt-5.6-terra".to_string())
        );
    }

    #[test]
    fn a_url_nested_in_the_input_payload_resolves_recorded() {
        let value =
            json!({"name": "WebFetch", "input": {"url": "https://example.com/openapi.json"}});

        let call = tool_call(&value, "name", "input", SourceValue::Absent);

        assert_eq!(
            call.url,
            SourceValue::Recorded("https://example.com/openapi.json".to_string())
        );
    }
}

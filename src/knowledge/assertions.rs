//! Per-assertion executor: `Assertion::check` and the verdict it produces.
//!
//! Sync families are implemented inline; LLM/subprocess families return
//! `Deferred` until the orchestrator wires their collaborator. The dispatch
//! table is a total `match` over `Assertion`.

use crate::content::conversation::Content;
use crate::content::conversation::ContentBlock;
use crate::content::conversation::Conversation;
use crate::content::conversation::Role;
use crate::content::conversation::Trace;
use crate::content::evaluation::Assertion;
use crate::content::evaluation::Op;
use crate::content::evaluation::TokenMetric;
use crate::engine::engine::EngineProvider;

/// Verdict of an `Assertion::check` call. Closed set; callers `match`
/// exhaustively. Adding a variant is a deliberate contract change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertionOutcome {
    /// The assertion's expectation was satisfied.
    Pass,
    /// Checked and not satisfied. `reason` names the expectation and the
    /// observed value; serialized verbatim by the eval-report writer.
    Fail { reason: String },
    /// A required collaborator was absent from `EvaluationContext`. The
    /// orchestrator will route the assertion to its proper executor.
    Deferred,
    /// The assertion is structurally invalid (e.g. uncompilable regex,
    /// unparseable `JSONPath`). Distinct from `Fail` so suite-authoring bugs
    /// surface separately from data-driven failures.
    Malformed { reason: String },
}

/// Collaborators required by LLM-based assertion variants. Fields are
/// `Option<&dyn Trait>` so callers add a `None` for every collaborator not yet
/// wired. Future steps add fields without breaking `Assertion::check`'s
/// signature.
pub struct EvaluationContext<'a> {
    /// Engine consulted by `Judge` (and, when wired, `TextSemanticMatch`).
    /// `None` ⇒ those variants return `Deferred`.
    pub engine: Option<&'a dyn EngineProvider>,
}

impl EvaluationContext<'_> {
    /// The no-collaborators context. Tests of the 12 sync families and any
    /// orchestrator slice that does not yet need LLM execution pass this.
    #[must_use]
    pub const fn empty() -> EvaluationContext<'static> {
        EvaluationContext { engine: None }
    }
}

impl Assertion {
    /// Check this assertion against `conversation` in `ctx`. Total over every
    /// variant. The 12 sync families ignore `ctx`; the 5 LLM/subprocess
    /// families return `Deferred` when their collaborator is `None`.
    ///
    /// Invariant: never panics, never performs I/O on its own, never
    /// dereferences `Content::Blocks` on a `Deferred` path.
    #[expect(
        clippy::unused_async,
        reason = "async signature pinned now; LLM-family arms become async in later steps"
    )]
    pub async fn check(
        &self,
        conversation: &Conversation,
        _ctx: &EvaluationContext<'_>,
    ) -> AssertionOutcome {
        match self {
            Assertion::Judge { .. }
            | Assertion::Tool { .. }
            | Assertion::Script { .. }
            | Assertion::Program { .. }
            | Assertion::TextSemanticMatch { .. } => AssertionOutcome::Deferred,

            Assertion::TextContains {
                value,
                case_sensitive,
            } => check_text_contains(conversation, value, case_sensitive.unwrap_or(true)),
            Assertion::TextNotContains {
                value,
                case_sensitive,
            } => check_text_not_contains(conversation, value, case_sensitive.unwrap_or(true)),
            Assertion::TextMatches { pattern, flags } => {
                check_text_matches(conversation, pattern, flags.as_deref())
            }
            Assertion::TextEquals { value } => check_text_equals(conversation, value),

            Assertion::MustCallTool { tool, with_args } => {
                check_must_call_tool(conversation, tool, with_args.as_ref())
            }
            Assertion::MustNotCallTool { tool } => check_must_not_call_tool(conversation, tool),
            Assertion::ToolCallCount { tool, op, value } => {
                check_tool_call_count(conversation, tool.as_deref(), op, *value)
            }
            Assertion::ToolCallOrder { sequence } => check_tool_call_order(conversation, sequence),

            Assertion::JsonPath { path, op, value } => {
                check_json_path(conversation, path, op, value)
            }
            Assertion::ResponseField { path, exists } => {
                check_response_field(conversation, path, *exists)
            }

            Assertion::Tokens { metric, op, value } => {
                check_tokens(conversation, metric, op, *value)
            }
            Assertion::LatencyMs { op, value } => check_latency_ms(conversation, op, *value),
        }
    }
}

/// Extract the text content of the final filled assistant turn.
///
/// Walks `conversation.session` from the back, skipping blank assistant slots
/// (`body == None`). From the first filled assistant message:
///   - `Content::Text(s)` returns `s`.
///   - `Content::Blocks(blocks)` concatenates every `ContentBlock::Text { text
///     }` in declaration order, joined by `"\n"`. `ToolUse`, `ToolResult`,
///     `Thinking`, and `Image` blocks contribute nothing.
///
/// Returns `None` if no filled assistant turn exists; every text assertion
/// translates `None` into a `Fail` with a named reason so the missing data
/// never silently passes.
fn final_assistant_text(conversation: &Conversation) -> Option<String> {
    for message in conversation.session.iter().rev() {
        if !matches!(message.role, Role::Assistant) {
            continue;
        }
        let Some(body) = message.body.as_ref() else {
            continue;
        };
        return Some(match body {
            Content::Text(text) => text.clone(),
            Content::Blocks(blocks) => blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    ContentBlock::ToolUse { .. }
                    | ContentBlock::ToolResult { .. }
                    | ContentBlock::Thinking { .. }
                    | ContentBlock::Image { .. } => None,
                })
                .collect::<Vec<&str>>()
                .join("\n"),
        });
    }
    None
}

/// Common reason returned by every text assertion when no filled assistant
/// turn exists. Named so the upstream cause is immediate from the report.
const NO_FILLED_ASSISTANT: &str = "no filled assistant turn present";

fn check_text_contains(
    conversation: &Conversation,
    value: &str,
    case_sensitive: bool,
) -> AssertionOutcome {
    let Some(haystack) = final_assistant_text(conversation) else {
        return AssertionOutcome::Fail {
            reason: format!("text_contains: {NO_FILLED_ASSISTANT}"),
        };
    };
    let matched = if case_sensitive {
        haystack.contains(value)
    } else {
        haystack.to_lowercase().contains(&value.to_lowercase())
    };
    if matched {
        AssertionOutcome::Pass
    } else {
        AssertionOutcome::Fail {
            reason: format!(
                "text_contains: expected substring {value:?} not found in {}-byte response",
                haystack.len(),
            ),
        }
    }
}

fn check_text_not_contains(
    conversation: &Conversation,
    value: &str,
    case_sensitive: bool,
) -> AssertionOutcome {
    let Some(haystack) = final_assistant_text(conversation) else {
        return AssertionOutcome::Fail {
            reason: format!("text_not_contains: {NO_FILLED_ASSISTANT}"),
        };
    };
    let offset = if case_sensitive {
        haystack.find(value)
    } else {
        haystack.to_lowercase().find(&value.to_lowercase())
    };
    match offset {
        None => AssertionOutcome::Pass,
        Some(at) => AssertionOutcome::Fail {
            reason: format!(
                "text_not_contains: forbidden substring {value:?} found at byte offset {at}"
            ),
        },
    }
}

fn check_text_matches(
    conversation: &Conversation,
    pattern: &str,
    flags: Option<&str>,
) -> AssertionOutcome {
    let compiled = if let Some(flags) = flags.filter(|s| !s.is_empty()) {
        regex::Regex::new(&format!("(?{flags}){pattern}"))
    } else {
        regex::Regex::new(pattern)
    };
    let regex = match compiled {
        Ok(regex) => regex,
        Err(err) => {
            return AssertionOutcome::Malformed {
                reason: format!("text_matches: regex compile error: {err}"),
            };
        }
    };
    let Some(haystack) = final_assistant_text(conversation) else {
        return AssertionOutcome::Fail {
            reason: format!("text_matches: {NO_FILLED_ASSISTANT}"),
        };
    };
    if regex.is_match(&haystack) {
        AssertionOutcome::Pass
    } else {
        AssertionOutcome::Fail {
            reason: format!(
                "text_matches: pattern {pattern:?} did not match {}-byte response",
                haystack.len(),
            ),
        }
    }
}

fn check_text_equals(conversation: &Conversation, value: &str) -> AssertionOutcome {
    let Some(actual) = final_assistant_text(conversation) else {
        return AssertionOutcome::Fail {
            reason: format!("text_equals: {NO_FILLED_ASSISTANT}"),
        };
    };
    if actual == value {
        AssertionOutcome::Pass
    } else {
        AssertionOutcome::Fail {
            reason: format!(
                "text_equals: expected {value:?} ({} bytes) but got {actual:?} ({} bytes)",
                value.len(),
                actual.len(),
            ),
        }
    }
}

/// Collect every `ContentBlock::ToolUse` in `conversation.session` in
/// (message order, block order within message). Blank assistant slots
/// contribute nothing. `Content::Text` messages contribute nothing. Tool-use
/// is by spec assistant-authored but the extractor does not filter on role —
/// only on block kind — so a future conversation that records tool calls
/// under a different role still flows through.
fn extract_tool_uses(conversation: &Conversation) -> Vec<&ContentBlock> {
    let mut out = Vec::new();
    for message in &conversation.session {
        let Some(Content::Blocks(blocks)) = message.body.as_ref() else {
            continue;
        };
        for block in blocks {
            if matches!(block, ContentBlock::ToolUse { .. }) {
                out.push(block);
            }
        }
    }
    out
}

/// Project a `ContentBlock::ToolUse`'s `name` field, panicking if called on
/// any other variant. Callers must filter via `extract_tool_uses` first.
fn tool_use_name(block: &ContentBlock) -> &str {
    match block {
        ContentBlock::ToolUse { name, .. } => name,
        ContentBlock::Text { .. }
        | ContentBlock::ToolResult { .. }
        | ContentBlock::Thinking { .. }
        | ContentBlock::Image { .. } => {
            unreachable!("extract_tool_uses only yields ToolUse blocks")
        }
    }
}

/// Recursive subset match on `serde_yaml_ng::Value`:
///   - Scalars: equal by value.
///   - Maps: every key in `expected` must appear in `actual` with a value that
///     subset-matches.
///   - Sequences: deep equality (no canonical "subset of a sequence").
///
/// Subset rather than strict equality so a test author can pin
/// `{ claim_id: C-1 }` without enumerating every context field the real call
/// adds.
fn yaml_subset_match(expected: &serde_yaml_ng::Value, actual: &serde_yaml_ng::Value) -> bool {
    match (expected, actual) {
        (
            serde_yaml_ng::Value::Mapping(expected_map),
            serde_yaml_ng::Value::Mapping(actual_map),
        ) => expected_map.iter().all(|(key, expected_value)| {
            actual_map
                .get(key)
                .is_some_and(|actual_value| yaml_subset_match(expected_value, actual_value))
        }),
        _ => expected == actual,
    }
}

/// Lift `observed` and the assertion's `i64` value to `i128` so neither a
/// saturating sum nor a negative assertion value causes a wrapping subtract.
fn compare_i128(observed: i128, op: &Op, value: i64) -> bool {
    let value = i128::from(value);
    match op {
        Op::Eq => observed == value,
        Op::NotEq => observed != value,
        Op::Lt => observed < value,
        Op::LtEq => observed <= value,
        Op::Gt => observed > value,
        Op::GtEq => observed >= value,
    }
}

fn op_symbol(op: &Op) -> &'static str {
    match op {
        Op::Eq => "==",
        Op::NotEq => "!=",
        Op::Lt => "<",
        Op::LtEq => "<=",
        Op::Gt => ">",
        Op::GtEq => ">=",
    }
}

fn check_must_call_tool(
    conversation: &Conversation,
    tool: &str,
    with_args: Option<&serde_yaml_ng::Value>,
) -> AssertionOutcome {
    let calls = extract_tool_uses(conversation);
    let observed_names: Vec<&str> = calls.iter().copied().map(tool_use_name).collect();
    let mut name_matched = false;
    for block in &calls {
        let ContentBlock::ToolUse { name, input, .. } = block else {
            continue;
        };
        if name != tool {
            continue;
        }
        name_matched = true;
        match with_args {
            None => return AssertionOutcome::Pass,
            Some(expected) if yaml_subset_match(expected, input) => return AssertionOutcome::Pass,
            Some(_) => {}
        }
    }
    let reason = if name_matched {
        format!(
            "must_call_tool: tool {tool:?} invoked but no call matched the expected `with_args` subset",
        )
    } else {
        format!(
            "must_call_tool: tool {tool:?} was not invoked (observed tool calls: {observed_names:?})",
        )
    };
    AssertionOutcome::Fail { reason }
}

fn check_must_not_call_tool(conversation: &Conversation, tool: &str) -> AssertionOutcome {
    let calls = extract_tool_uses(conversation);
    if let Some(index) = calls.iter().position(|block| tool_use_name(block) == tool) {
        return AssertionOutcome::Fail {
            reason: format!(
                "must_not_call_tool: forbidden tool {tool:?} invoked first at call index {index}",
            ),
        };
    }
    AssertionOutcome::Pass
}

fn check_tool_call_count(
    conversation: &Conversation,
    tool: Option<&str>,
    op: &Op,
    value: i64,
) -> AssertionOutcome {
    let calls = extract_tool_uses(conversation);
    let count = match tool {
        None => calls.len(),
        Some(name) => calls
            .iter()
            .filter(|block| tool_use_name(block) == name)
            .count(),
    };
    let observed_i128 = i128::try_from(count).unwrap_or(i128::MAX);
    if compare_i128(observed_i128, op, value) {
        AssertionOutcome::Pass
    } else {
        let tool_display = tool.unwrap_or("any");
        AssertionOutcome::Fail {
            reason: format!(
                "tool_call_count: expected count {} {value}, observed {count} (tool: {tool_display})",
                op_symbol(op),
            ),
        }
    }
}

/// Convert a `serde_yaml_ng::Value` to a `serde_json::Value` by round-tripping
/// through a JSON string. The YAML value comes from a parsed assertion field,
/// so the resulting JSON shape is the suite author's literal — strings,
/// numbers, bools, null, and homogeneous containers all flow through.
fn yaml_to_json(value: &serde_yaml_ng::Value) -> serde_json::Value {
    let json_text = serde_json::to_string(value)
        .expect("serde_yaml_ng::Value serializes to JSON via its serde::Serialize impl");
    serde_json::from_str(&json_text)
        .expect("a JSON string emitted by serde_json round-trips to a serde_json::Value")
}

/// First-result of a `JSONPath` query against `conversation` rendered as JSON.
/// `Ok(None)` ⇒ path is valid but matched nothing; `Ok(Some(value))` ⇒ first
/// matched JSON value; `Err(outcome)` ⇒ path failed to parse and the caller
/// returns the `Malformed` outcome verbatim.
fn jsonpath_first(
    conversation: &Conversation,
    path: &str,
) -> Result<Option<serde_json::Value>, AssertionOutcome> {
    let parsed =
        serde_json_path::JsonPath::parse(path).map_err(|err| AssertionOutcome::Malformed {
            reason: format!("json_path: parse error on path {path:?}: {err}"),
        })?;
    let json = serde_json::to_value(conversation)
        .expect("Conversation derives Serialize and lowers to JSON without loss");
    let matched = parsed.query(&json).all();
    Ok(matched.first().map(|value| (*value).clone()))
}

fn compare_json_values(
    observed: &serde_json::Value,
    op: &Op,
    expected: &serde_json::Value,
) -> ComparisonResult {
    match op {
        Op::Eq => ComparisonResult::Decided(observed == expected),
        Op::NotEq => ComparisonResult::Decided(observed != expected),
        Op::Lt | Op::LtEq | Op::Gt | Op::GtEq => match (observed.as_f64(), expected.as_f64()) {
            (Some(lhs), Some(rhs)) => ComparisonResult::Decided(match op {
                Op::Lt => lhs < rhs,
                Op::LtEq => lhs <= rhs,
                Op::Gt => lhs > rhs,
                Op::GtEq => lhs >= rhs,
                Op::Eq | Op::NotEq => unreachable!("outer match excluded these arms"),
            }),
            _ => ComparisonResult::TypeMismatch,
        },
    }
}

enum ComparisonResult {
    Decided(bool),
    TypeMismatch,
}

fn check_json_path(
    conversation: &Conversation,
    path: &str,
    op: &Op,
    value: &serde_yaml_ng::Value,
) -> AssertionOutcome {
    let observed = match jsonpath_first(conversation, path) {
        Ok(observed) => observed,
        Err(outcome) => return outcome,
    };
    let Some(observed) = observed else {
        return AssertionOutcome::Fail {
            reason: format!("json_path: {path}: no value matched the path"),
        };
    };
    let expected = yaml_to_json(value);
    match compare_json_values(&observed, op, &expected) {
        ComparisonResult::Decided(true) => AssertionOutcome::Pass,
        ComparisonResult::Decided(false) => AssertionOutcome::Fail {
            reason: format!(
                "json_path: {path}: expected {} {expected}, observed {observed}",
                op_symbol(op),
            ),
        },
        ComparisonResult::TypeMismatch => AssertionOutcome::Fail {
            reason: format!(
                "json_path: {path}: ordered op {} requires numeric values, observed {observed}",
                op_symbol(op),
            ),
        },
    }
}

fn check_response_field(conversation: &Conversation, path: &str, exists: bool) -> AssertionOutcome {
    let observed = match jsonpath_first(conversation, path) {
        Ok(observed) => observed,
        Err(outcome) => return outcome,
    };
    match (exists, observed.is_some()) {
        (true, true) | (false, false) => AssertionOutcome::Pass,
        (true, false) => AssertionOutcome::Fail {
            reason: format!("response_field: {path}: expected value present, found none"),
        },
        (false, true) => AssertionOutcome::Fail {
            reason: format!("response_field: {path}: expected no value, but path matched"),
        },
    }
}

/// Sum a `Trace`-derived `u64` field across every message in
/// `conversation.session` that carries a `trace`. Blank assistant slots
/// contribute zero. The spec sums across messages, not across roles, so a
/// system or user message that happens to carry a `trace` is included.
fn sum_traces<F: Fn(&Trace) -> u64>(conversation: &Conversation, project: F) -> u128 {
    conversation
        .session
        .iter()
        .filter_map(|message| message.trace.as_ref())
        .map(|trace| u128::from(project(trace)))
        .sum()
}

fn check_tokens(
    conversation: &Conversation,
    metric: &TokenMetric,
    op: &Op,
    value: i64,
) -> AssertionOutcome {
    let observed = match metric {
        TokenMetric::Input => sum_traces(conversation, |t| t.tokens.input),
        TokenMetric::Output => sum_traces(conversation, |t| t.tokens.output),
        TokenMetric::Total => sum_traces(conversation, |t| {
            t.tokens.input.saturating_add(t.tokens.output)
        }),
    };
    let observed_i128 = i128::try_from(observed).unwrap_or(i128::MAX);
    if compare_i128(observed_i128, op, value) {
        AssertionOutcome::Pass
    } else {
        AssertionOutcome::Fail {
            reason: format!(
                "tokens: expected {} {value} ({metric:?}), observed {observed}",
                op_symbol(op),
            ),
        }
    }
}

fn check_latency_ms(conversation: &Conversation, op: &Op, value: i64) -> AssertionOutcome {
    let observed = sum_traces(conversation, |t| t.latency_ms);
    let observed_i128 = i128::try_from(observed).unwrap_or(i128::MAX);
    if compare_i128(observed_i128, op, value) {
        AssertionOutcome::Pass
    } else {
        AssertionOutcome::Fail {
            reason: format!(
                "latency_ms: expected {} {value}, observed {observed}",
                op_symbol(op),
            ),
        }
    }
}

fn check_tool_call_order(conversation: &Conversation, sequence: &[String]) -> AssertionOutcome {
    let calls = extract_tool_uses(conversation);
    let observed_names: Vec<&str> = calls.iter().copied().map(tool_use_name).collect();
    let mut cursor = 0;
    for name in &observed_names {
        if cursor >= sequence.len() {
            break;
        }
        if *name == sequence[cursor] {
            cursor += 1;
        }
    }
    if cursor >= sequence.len() {
        AssertionOutcome::Pass
    } else {
        AssertionOutcome::Fail {
            reason: format!(
                "tool_call_order: expected subsequence {sequence:?}, stalled at {:?} (observed {observed_names:?})",
                sequence[cursor],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use super::*;
    use crate::content::conversation::BindingMap;
    use crate::content::conversation::Message;
    use crate::content::conversation::Meta;
    use crate::content::conversation::ModelId;
    use crate::content::conversation::SpanId;
    use crate::content::conversation::TokenCounts;
    use crate::content::conversation::ToolUseId;
    use crate::content::evaluation::Assertion;
    use crate::content::evaluation::ScriptBody;
    use crate::content::evaluation::ScriptRuntime;
    use crate::content::evaluation::ToolCallSpec;

    fn conversation_with(session: Vec<Message>) -> Conversation {
        Conversation {
            meta: Meta {
                model: ModelId::from("noop"),
                debug: false,
                assembly: None,
                binding: BindingMap::new(),
            },
            session,
        }
    }

    fn user_text(text: &str) -> Message {
        Message {
            role: Role::User,
            body: Some(Content::Text(String::from(text))),
            cache: false,
            trace: None,
            _phase: PhantomData,
        }
    }

    fn assistant_text(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            body: Some(Content::Text(String::from(text))),
            cache: false,
            trace: None,
            _phase: PhantomData,
        }
    }

    fn assistant_blocks(blocks: Vec<ContentBlock>) -> Message {
        Message {
            role: Role::Assistant,
            body: Some(Content::Blocks(blocks)),
            cache: false,
            trace: None,
            _phase: PhantomData,
        }
    }

    fn assistant_blank() -> Message {
        Message {
            role: Role::Assistant,
            body: None,
            cache: false,
            trace: None,
            _phase: PhantomData,
        }
    }

    fn tool_use(name: &str) -> ContentBlock {
        ContentBlock::ToolUse {
            id: ToolUseId::from("tool_1"),
            name: String::from(name),
            input: serde_yaml_ng::Value::Null,
        }
    }

    fn ctx() -> EvaluationContext<'static> {
        EvaluationContext::empty()
    }

    #[tokio::test]
    async fn text_contains_case_insensitive_matches_uppercase_substring() {
        let conv = conversation_with(vec![assistant_text("Policy Number Required.")]);
        let assertion = Assertion::TextContains {
            value: String::from("policy number"),
            case_sensitive: Some(false),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn text_contains_fails_when_substring_absent_with_named_reason() {
        let conv = conversation_with(vec![assistant_text("approved.")]);
        let assertion = Assertion::TextContains {
            value: String::from("policy"),
            case_sensitive: None,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("policy"), "reason missing needle: {reason}");
                assert!(
                    reason.contains("text_contains"),
                    "reason missing op: {reason}",
                );
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn text_not_contains_passes_when_substring_absent() {
        let conv = conversation_with(vec![assistant_text("policy.")]);
        let assertion = Assertion::TextNotContains {
            value: String::from("auto-approve"),
            case_sensitive: None,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn text_not_contains_fails_when_substring_present_with_first_offset() {
        let conv = conversation_with(vec![assistant_text("zzz auto-approve here.")]);
        let assertion = Assertion::TextNotContains {
            value: String::from("auto-approve"),
            case_sensitive: None,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("auto-approve"));
                assert!(
                    reason.contains("offset 4"),
                    "reason missing offset: {reason}"
                );
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn text_matches_with_invalid_regex_returns_malformed_with_compile_error() {
        let conv = conversation_with(vec![assistant_text("anything")]);
        let assertion = Assertion::TextMatches {
            pattern: String::from("[unclosed"),
            flags: None,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Malformed { reason } => {
                assert!(reason.contains("text_matches"), "got {reason}");
                assert!(reason.to_lowercase().contains("regex"), "got {reason}");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn text_matches_with_flags_compiles_into_inline_group() {
        let conv = conversation_with(vec![assistant_text("POLICY number required")]);
        let assertion = Assertion::TextMatches {
            pattern: String::from("polic\\w+"),
            flags: Some(String::from("i")),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn text_matches_fails_when_pattern_does_not_match() {
        let conv = conversation_with(vec![assistant_text("nothing relevant")]);
        let assertion = Assertion::TextMatches {
            pattern: String::from("policy"),
            flags: None,
        };
        assert!(matches!(
            assertion.check(&conv, &ctx()).await,
            AssertionOutcome::Fail { .. }
        ));
    }

    #[tokio::test]
    async fn text_equals_passes_on_byte_exact_match() {
        let conv = conversation_with(vec![assistant_text("ok")]);
        let assertion = Assertion::TextEquals {
            value: String::from("ok"),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn text_equals_fails_with_diff_summary_when_lengths_match_but_bytes_differ() {
        let conv = conversation_with(vec![assistant_text("ab")]);
        let assertion = Assertion::TextEquals {
            value: String::from("ba"),
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(
                    reason.contains("\"ba\"") && reason.contains("\"ab\""),
                    "got {reason}"
                );
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn text_assertions_against_unfilled_conversation_fail_with_named_reason() {
        let conv = conversation_with(vec![user_text("ask"), assistant_blank()]);
        let assertion = Assertion::TextContains {
            value: String::from("anything"),
            case_sensitive: None,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains(NO_FILLED_ASSISTANT), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn text_assertions_concatenate_text_blocks_with_newline_separator() {
        let conv = conversation_with(vec![assistant_blocks(vec![
            ContentBlock::Text {
                text: String::from("first"),
            },
            tool_use("lookup"),
            ContentBlock::Text {
                text: String::from("second"),
            },
        ])]);
        let assertion = Assertion::TextEquals {
            value: String::from("first\nsecond"),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn text_helpers_pick_last_filled_assistant_skipping_blank_tail() {
        // Filled assistant turn, then a blank one tail-padded by `ailly run`
        // partial. `final_assistant_text` skips the blank and returns the filled.
        let conv = conversation_with(vec![assistant_text("earlier"), assistant_blank()]);
        let assertion = Assertion::TextEquals {
            value: String::from("earlier"),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    fn tool_use_with_input(name: &str, input: serde_yaml_ng::Value) -> ContentBlock {
        ContentBlock::ToolUse {
            id: ToolUseId::from("tool_x"),
            name: String::from(name),
            input,
        }
    }

    #[tokio::test]
    async fn must_call_tool_passes_when_named_call_appears() {
        let conv = conversation_with(vec![assistant_blocks(vec![tool_use("lookup_policy")])]);
        let assertion = Assertion::MustCallTool {
            tool: String::from("lookup_policy"),
            with_args: None,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn must_call_tool_fails_with_observed_call_names_in_reason() {
        let conv = conversation_with(vec![assistant_blocks(vec![tool_use("auto_approve")])]);
        let assertion = Assertion::MustCallTool {
            tool: String::from("lookup_policy"),
            with_args: None,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("lookup_policy"), "got {reason}");
                assert!(reason.contains("auto_approve"), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn must_call_tool_with_args_subset_match_passes_on_extra_input_fields() {
        let actual_input: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("{ claim_id: C-1, ts: 123, ctx: { region: us } }")
                .expect("valid yaml");
        let expected: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("{ claim_id: C-1 }").expect("valid yaml");
        let conv = conversation_with(vec![assistant_blocks(vec![tool_use_with_input(
            "lookup_policy",
            actual_input,
        )])]);
        let assertion = Assertion::MustCallTool {
            tool: String::from("lookup_policy"),
            with_args: Some(expected),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn must_call_tool_with_args_rejects_value_mismatch() {
        let actual_input: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("{ claim_id: C-2 }").expect("valid yaml");
        let expected: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("{ claim_id: C-1 }").expect("valid yaml");
        let conv = conversation_with(vec![assistant_blocks(vec![tool_use_with_input(
            "lookup_policy",
            actual_input,
        )])]);
        let assertion = Assertion::MustCallTool {
            tool: String::from("lookup_policy"),
            with_args: Some(expected),
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("with_args"), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn must_not_call_tool_passes_when_tool_absent() {
        let conv = conversation_with(vec![assistant_blocks(vec![tool_use("lookup_policy")])]);
        let assertion = Assertion::MustNotCallTool {
            tool: String::from("auto_approve"),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn must_not_call_tool_fails_with_first_call_index_in_reason() {
        let conv = conversation_with(vec![assistant_blocks(vec![
            tool_use("noise"),
            tool_use("auto_approve"),
        ])]);
        let assertion = Assertion::MustNotCallTool {
            tool: String::from("auto_approve"),
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("auto_approve"), "got {reason}");
                assert!(reason.contains("index 1"), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn tool_call_count_with_tool_none_counts_every_call_in_session() {
        let conv = conversation_with(vec![
            assistant_blocks(vec![tool_use("a"), tool_use("b")]),
            assistant_blocks(vec![tool_use("c")]),
        ]);
        let assertion = Assertion::ToolCallCount {
            tool: None,
            op: Op::Eq,
            value: 3,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn tool_call_count_fails_with_count_and_op_in_reason() {
        let conv = conversation_with(vec![assistant_blocks(vec![
            tool_use("a"),
            tool_use("a"),
            tool_use("a"),
        ])]);
        let assertion = Assertion::ToolCallCount {
            tool: Some(String::from("a")),
            op: Op::LtEq,
            value: 2,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("<="), "got {reason}");
                assert!(reason.contains("observed 3"), "got {reason}");
                assert!(reason.contains("tool: a"), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn tool_call_order_subsequence_passes_with_intervening_noise_calls() {
        let conv = conversation_with(vec![assistant_blocks(vec![
            tool_use("a"),
            tool_use("noise"),
            tool_use("b"),
            tool_use("more_noise"),
            tool_use("c"),
        ])]);
        let assertion = Assertion::ToolCallOrder {
            sequence: vec![String::from("a"), String::from("b"), String::from("c")],
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn tool_call_order_fails_when_sequence_appears_reordered() {
        let conv = conversation_with(vec![assistant_blocks(vec![tool_use("b"), tool_use("a")])]);
        let assertion = Assertion::ToolCallOrder {
            sequence: vec![String::from("a"), String::from("b")],
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("stalled"), "got {reason}");
                assert!(reason.contains("\"b\""), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn json_path_returns_malformed_on_unparseable_path() {
        let conv = conversation_with(vec![assistant_text("ok")]);
        let assertion = Assertion::JsonPath {
            path: String::from("$.[bogus"),
            op: Op::Eq,
            value: serde_yaml_ng::Value::Null,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Malformed { reason } => {
                assert!(reason.contains("json_path"), "got {reason}");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn json_path_walks_session_array_and_compares_first_result() {
        let conv = conversation_with(vec![user_text("hi"), assistant_text("ok")]);
        let assertion = Assertion::JsonPath {
            path: String::from("$.session[0].role"),
            op: Op::Eq,
            value: serde_yaml_ng::Value::String(String::from("user")),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn json_path_ordered_op_fails_with_type_mismatch_when_value_is_string() {
        let conv = conversation_with(vec![user_text("hi")]);
        let assertion = Assertion::JsonPath {
            path: String::from("$.session[0].role"),
            op: Op::Lt,
            value: serde_yaml_ng::Value::Number(serde_yaml_ng::Number::from(99)),
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("numeric"), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn response_field_exists_true_passes_when_path_yields_results() {
        let conv = conversation_with(vec![user_text("hi")]);
        let assertion = Assertion::ResponseField {
            path: String::from("$.session[0].role"),
            exists: true,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn response_field_exists_true_fails_on_unmatched_path() {
        let conv = conversation_with(vec![user_text("hi")]);
        let assertion = Assertion::ResponseField {
            path: String::from("$.session[99].role"),
            exists: true,
        };
        assert!(matches!(
            assertion.check(&conv, &ctx()).await,
            AssertionOutcome::Fail { .. }
        ));
    }

    #[tokio::test]
    async fn response_field_exists_false_passes_on_unmatched_path() {
        let conv = conversation_with(vec![user_text("hi")]);
        let assertion = Assertion::ResponseField {
            path: String::from("$.session[99].role"),
            exists: false,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn response_field_exists_false_fails_when_path_yields_results() {
        let conv = conversation_with(vec![user_text("hi")]);
        let assertion = Assertion::ResponseField {
            path: String::from("$.session[0].role"),
            exists: false,
        };
        assert!(matches!(
            assertion.check(&conv, &ctx()).await,
            AssertionOutcome::Fail { .. }
        ));
    }

    fn trace(input: u64, output: u64, latency_ms: u64) -> Trace {
        Trace {
            span_id: SpanId::from("span-test"),
            model: ModelId::from("noop"),
            tokens: TokenCounts {
                input,
                output,
                cache_hit: None,
                cache_write: None,
            },
            latency_ms,
            events: Vec::new(),
        }
    }

    fn assistant_text_traced(text: &str, trace: Trace) -> Message {
        Message {
            role: Role::Assistant,
            body: Some(Content::Text(String::from(text))),
            cache: false,
            trace: Some(trace),
            _phase: PhantomData,
        }
    }

    #[tokio::test]
    async fn tokens_total_sums_input_plus_output_across_every_trace() {
        let conv = conversation_with(vec![
            assistant_text_traced("first", trace(100, 50, 0)),
            assistant_text_traced("second", trace(200, 80, 0)),
        ]);
        let assertion = Assertion::Tokens {
            metric: TokenMetric::Total,
            op: Op::Eq,
            value: 430,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn tokens_input_only_excludes_output_tokens() {
        let conv = conversation_with(vec![assistant_text_traced("x", trace(10, 999, 0))]);
        let assertion = Assertion::Tokens {
            metric: TokenMetric::Input,
            op: Op::Eq,
            value: 10,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn tokens_fails_with_observed_count_in_reason() {
        let conv = conversation_with(vec![assistant_text_traced("x", trace(9000, 0, 0))]);
        let assertion = Assertion::Tokens {
            metric: TokenMetric::Total,
            op: Op::Lt,
            value: 100,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("9000"), "got {reason}");
                assert!(reason.contains('<'), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn latency_ms_zero_passes_eq_zero_on_empty_session() {
        let conv = conversation_with(vec![]);
        let assertion = Assertion::LatencyMs {
            op: Op::Eq,
            value: 0,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn latency_ms_fails_with_observed_sum_in_reason() {
        let conv = conversation_with(vec![assistant_text_traced("x", trace(0, 0, 9999))]);
        let assertion = Assertion::LatencyMs {
            op: Op::LtEq,
            value: 500,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("9999"), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn remote_variants_return_deferred_when_context_is_empty() {
        let conv = conversation_with(vec![assistant_text("ok")]);
        let cases = [
            Assertion::Judge {
                prompt: String::from("any"),
            },
            Assertion::Tool {
                tool_call: ToolCallSpec {
                    tool: String::from("lookup"),
                    with_args: None,
                },
            },
            Assertion::Script {
                runtime: ScriptRuntime::Node,
                script: ScriptBody::Contents {
                    contents: String::from("noop"),
                },
            },
            Assertion::Program {
                script: String::from("ailly-check"),
            },
            Assertion::TextSemanticMatch {
                value: String::from("ok"),
                threshold: None,
            },
        ];
        for assertion in &cases {
            assert_eq!(
                assertion.check(&conv, &ctx()).await,
                AssertionOutcome::Deferred,
                "variant {assertion:?} must defer when no engine supplied",
            );
        }
    }
}

//! Eval orchestrator: pair suite cases with conversations, drive
//! `Assertion::check`, and assemble the on-disk report. I/O is bounded:
//! `cli::eval` writes the JSON report, and `evaluate()` writes one judge
//! transcript per `Assertion::Judge` call when `EvalArgs::judge_output_dir`
//! is `Some` (otherwise it performs no I/O).

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::content::conversation::BindingMap;
use crate::content::conversation::Conversation;
use crate::content::evaluation::Assertion;
use crate::content::evaluation::Case;
use crate::content::evaluation::Evaluation;
use crate::knowledge::assertions::AssertionOutcome;
use crate::knowledge::assertions::EvaluationContext;
use crate::knowledge::assertions::check_judge;

/// Full report serialized to `<project>/evals/reports/<run-id>.json`.
/// Field names are the JSON keys; see DESIGN.md §evaluation for the contract.
#[derive(Serialize, Deserialize, Debug)]
pub struct EvalReport {
    pub suite: String,
    pub run_id: String,
    /// ISO-8601 timestamp. Old reports without this field deserialize to UNIX
    /// epoch.
    #[serde(default = "epoch_timestamp")]
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// First model identifier seen in the run's conversations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Aggregate token and latency metrics from trace data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<RunMetrics>,
    pub totals: ReportTotals,
    pub per_class: BTreeMap<String, ClassTotals>,
    pub cases: Vec<CaseReport>,
}

fn epoch_timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::<chrono::Utc>::UNIX_EPOCH
}

/// Aggregate trace metrics collected during `ailly run`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default)]
pub struct RunMetrics {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_hit_tokens: u64,
    pub total_latency_ms: u64,
    pub conversations_with_trace: usize,
}

/// Top-level rollup. `conversations_matched` is the count of distinct
/// conversations that appeared in any case's matches list at least once.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ReportTotals {
    pub conversations_matched: usize,
    pub assertions: BucketTotals,
}

/// Five-bucket verdict tally, exhaustive over
/// [`crate::knowledge::assertions::AssertionOutcome`].
#[derive(Serialize, Deserialize, Debug, Default, Clone, Copy)]
pub struct BucketTotals {
    pub passed: usize,
    pub failed: usize,
    pub deferred: usize,
    pub malformed: usize,
    /// Added with the fifth `AssertionOutcome` variant. `#[serde(default)]` so
    /// `ailly report` reads pre-fifth-variant reports (absent ⇒ 0); new reports
    /// always serialize it (the `Default` derive does not skip the field).
    #[serde(default)]
    pub errored: usize,
}

pub type ClassTotals = BucketTotals;

#[derive(Serialize, Deserialize, Debug)]
pub struct CaseReport {
    /// Present iff the suite case carried a `name:`. `when:`-filtered and
    /// no-filter cases serialize without a `name` field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub matches: Vec<MatchReport>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MatchReport {
    /// Filename (with extension), not the full path. The run directory is
    /// already conveyed by `EvalReport::run_id`.
    pub conversation: String,
    pub assertions: Vec<AssertionReport>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AssertionReport {
    pub class: String,
    pub outcome: String,
    /// Present iff the executor returned a non-empty reason. Skipped on
    /// serialize so passing assertions stay terse.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Stable per-class rollup key. Returns the lowercase `snake_case` serde tag of
/// `assertion`. Total over the [`Assertion`] enum so that a new variant added
/// without an arm here breaks compilation.
#[must_use]
pub(crate) fn class_tag(assertion: &Assertion) -> &'static str {
    match assertion {
        Assertion::Judge { .. } => "judge",
        Assertion::Tool { .. } => "tool",
        Assertion::Script { .. } => "script",
        Assertion::Program { .. } => "program",
        Assertion::MustCallTool { .. } => "must_call_tool",
        Assertion::MustNotCallTool { .. } => "must_not_call_tool",
        Assertion::ToolCallCount { .. } => "tool_call_count",
        Assertion::ToolCallOrder { .. } => "tool_call_order",
        Assertion::TextContains { .. } => "text_contains",
        Assertion::TextNotContains { .. } => "text_not_contains",
        Assertion::TextMatches { .. } => "text_matches",
        Assertion::TextEquals { .. } => "text_equals",
        Assertion::TextSemanticMatch { .. } => "text_semantic_match",
        Assertion::JsonPath { .. } => "json_path",
        Assertion::ResponseField { .. } => "response_field",
        Assertion::Tokens { .. } => "tokens",
        Assertion::LatencyMs { .. } => "latency_ms",
    }
}

/// Inputs for one evaluation pass. References must outlive the `evaluate`
/// call; the CLI builds them on its stack and drops them once the report is
/// returned.
pub struct EvalArgs<'a> {
    pub suite: &'a Evaluation,
    /// Each tuple is `(source_path, parsed_conversation)`. The path supplies
    /// the filename stem for `name:` matching and the
    /// `MatchReport.conversation` field. The orchestrator never reads the
    /// file again.
    pub conversations: &'a [(PathBuf, Conversation)],
    pub ctx: EvaluationContext<'a>,
    /// Suite name as typed on the command line; copied verbatim into
    /// `EvalReport.suite`.
    pub suite_name: &'a str,
    /// Run-id derived by the caller (run-directory basename or single-file
    /// stem); copied verbatim into `EvalReport.run_id`.
    pub run_id: &'a str,
    /// When `Some`, the orchestrator writes one Ailly conversation file per
    /// `Assertion::Judge` call into this directory, using the layout
    /// `<conv-stem>.<case-tag>.assertion-<M>.yaml`. When `None`, no files are
    /// written; this preserves the no-I/O contract for callers that only need
    /// the report value.
    pub judge_output_dir: Option<&'a std::path::Path>,
}

/// Pair each suite case with its matching conversations, run every assertion
/// once per (case, conversation) match, and assemble the report.
///
/// Matching rules:
/// - `name: Some(stem)` — exact filename-stem match; zero matches synthesizes
///   one `malformed` assertion record under class `missing_conversation` so a
///   typo in the suite fails the run.
/// - `name: None, when: non-empty` — subset match against `meta.binding`. Zero
///   matches is not flagged.
/// - `name: None, when: empty` — fan out to every conversation.
///
/// Assertion order within a case is preserved. The per-class rollup and
/// four-bucket totals are folded in as verdicts are produced.
pub async fn evaluate(args: EvalArgs<'_>) -> EvalReport {
    let mut totals = ReportTotals::default();
    let mut per_class: BTreeMap<String, ClassTotals> = BTreeMap::new();
    let mut cases: Vec<CaseReport> = Vec::with_capacity(args.suite.cases.len());
    let mut match_count: usize = 0;

    for (case_index, case) in args.suite.cases.iter().enumerate() {
        let matched: Vec<&(PathBuf, Conversation)> = matches_for(case, args.conversations);
        let mut match_reports: Vec<MatchReport> = Vec::with_capacity(matched.len());

        if matched.is_empty() && case.name.is_some() {
            let assertion = AssertionReport {
                class: String::from("missing_conversation"),
                outcome: String::from(outcome_label(&AssertionOutcome::Malformed {
                    reason: String::new(),
                })),
                reason: Some(format!(
                    "no conversation found for case name {:?}",
                    case.name.as_deref().unwrap_or_default()
                )),
            };
            fold_bucket(
                &mut totals.assertions,
                &AssertionOutcome::Malformed {
                    reason: String::new(),
                },
            );
            fold_bucket(
                per_class
                    .entry(String::from("missing_conversation"))
                    .or_default(),
                &AssertionOutcome::Malformed {
                    reason: String::new(),
                },
            );
            match_reports.push(MatchReport {
                conversation: String::new(),
                assertions: vec![assertion],
            });
        }

        for (path, conv) in matched {
            match_count += 1;
            let mut assertion_reports: Vec<AssertionReport> =
                Vec::with_capacity(case.assertions.len());
            let mut judge_idx_in_case: usize = 0;
            for assertion in &case.assertions {
                let outcome = match assertion {
                    Assertion::Judge { prompt } => {
                        let call = check_judge(prompt, conv, &args.ctx).await;
                        if let (Some(out_dir), Some(transcript)) =
                            (args.judge_output_dir, call.transcript.as_ref())
                        {
                            persist_judge_transcript(
                                out_dir,
                                stem_of(path),
                                &case_tag(case, case_index),
                                judge_idx_in_case,
                                transcript,
                            );
                        }
                        judge_idx_in_case += 1;
                        call.outcome
                    }
                    _ => assertion.check(conv, &args.ctx).await,
                };
                let class = class_tag(assertion);
                fold_bucket(&mut totals.assertions, &outcome);
                fold_bucket(per_class.entry(String::from(class)).or_default(), &outcome);
                assertion_reports.push(AssertionReport {
                    class: String::from(class),
                    outcome: String::from(outcome_label(&outcome)),
                    reason: reason_for(&outcome),
                });
            }
            match_reports.push(MatchReport {
                conversation: filename_of(path),
                assertions: assertion_reports,
            });
        }

        cases.push(CaseReport {
            name: case.name.clone(),
            matches: match_reports,
        });
    }

    totals.conversations_matched = match_count;

    let (model, metrics) = collect_run_metrics(args.conversations);

    EvalReport {
        suite: String::from(args.suite_name),
        run_id: String::from(args.run_id),
        timestamp: epoch_timestamp(),
        model,
        metrics,
        totals,
        per_class,
        cases,
    }
}

/// Walk every conversation in the run and roll up trace metrics across all
/// messages. `model` is taken from the first conversation's `meta.model` so
/// the report identifies what produced the run; `metrics` is `Some` iff at
/// least one message carried a trace, mirroring the
/// `skip_serializing_if = "Option::is_none"` policy on the field.
fn collect_run_metrics(
    conversations: &[(PathBuf, Conversation)],
) -> (Option<String>, Option<RunMetrics>) {
    let model = conversations
        .first()
        .map(|(_, conv)| conv.meta.model.as_ref().to_owned());

    let mut metrics = RunMetrics::default();
    for (_, conv) in conversations {
        let mut conv_has_trace = false;
        for msg in &conv.session {
            let Some(trace) = msg.trace.as_ref() else {
                continue;
            };
            metrics.total_input_tokens += trace.tokens.input;
            metrics.total_output_tokens += trace.tokens.output;
            metrics.total_cache_hit_tokens += trace.tokens.cache_hit.unwrap_or(0);
            metrics.total_latency_ms += trace.latency_ms;
            conv_has_trace = true;
        }
        if conv_has_trace {
            metrics.conversations_with_trace += 1;
        }
    }

    let metrics = (metrics.conversations_with_trace > 0).then_some(metrics);
    (model, metrics)
}

fn matches_for<'a>(
    case: &Case,
    conversations: &'a [(PathBuf, Conversation)],
) -> Vec<&'a (PathBuf, Conversation)> {
    if let Some(target) = case.name.as_deref() {
        return conversations
            .iter()
            .filter(|(path, _)| stem_of(path) == target)
            .collect();
    }
    if case.when.is_empty() {
        return conversations.iter().collect();
    }
    conversations
        .iter()
        .filter(|(_, conv)| binding_contains(&conv.meta.binding, &case.when))
        .collect()
}

fn binding_contains(haystack: &BindingMap, needle: &BindingMap) -> bool {
    needle
        .iter()
        .all(|(k, v)| haystack.get(k).is_some_and(|got| got == v))
}

fn stem_of(path: &Path) -> &str {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("")
}

/// Filesystem-safe identifier for a case. Named cases use `case.name`
/// verbatim with path separators replaced and control characters stripped;
/// unnamed cases (when:- or fan-out) fall back to `case-<N>` where `N` is
/// the zero-based index of the case in the suite's `cases:` array.
///
/// The reserved-pattern rule on `Evaluation::from_yaml_str` (a follow-up
/// red-green-refactor cycle) makes the named and unnamed namespaces
/// disjoint by construction; this helper does not enforce that rule.
fn case_tag(case: &Case, case_index: usize) -> String {
    match case.name.as_deref() {
        Some(name) => sanitize_case_name(name),
        None => format!("case-{case_index}"),
    }
}

fn sanitize_case_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| {
            if c.is_control() {
                ' '
            } else if matches!(c, '/' | '\\') {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// Write a judge transcript to
/// `<out_dir>/<conv_stem>.<case_tag>.assertion-<M>.yaml`.
///
/// Failures (directory creation, serialization, write) are logged at
/// `warn!` and swallowed: a write failure must not alter the assertion
/// verdict already in `call.outcome`. The report is the source of truth
/// for verdict; the file is a diff-reviewable side-effect.
fn persist_judge_transcript(
    out_dir: &Path,
    conv_stem: &str,
    case_tag: &str,
    judge_idx_in_case: usize,
    transcript: &Conversation,
) {
    if let Err(err) = std::fs::create_dir_all(out_dir) {
        tracing::warn!(
            out_dir = %out_dir.display(),
            error = %err,
            "judge: failed to create transcript directory",
        );
        return;
    }
    let yaml = match transcript.to_yaml_string() {
        Ok(yaml) => yaml,
        Err(err) => {
            tracing::warn!(
                error = %err,
                "judge: failed to serialize transcript",
            );
            return;
        }
    };
    let file_name = format!("{conv_stem}.{case_tag}.assertion-{judge_idx_in_case}.yaml");
    let path = out_dir.join(&file_name);
    if let Err(err) = std::fs::write(&path, yaml) {
        tracing::warn!(
            path = %path.display(),
            error = %err,
            "judge: failed to write transcript",
        );
    }
}

fn filename_of(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(String::from)
        .unwrap_or_default()
}

fn outcome_label(outcome: &AssertionOutcome) -> &'static str {
    match outcome {
        AssertionOutcome::Pass => "pass",
        AssertionOutcome::Fail { .. } => "fail",
        AssertionOutcome::Deferred => "deferred",
        AssertionOutcome::Malformed { .. } => "malformed",
        AssertionOutcome::Errored { .. } => "errored",
    }
}

fn reason_for(outcome: &AssertionOutcome) -> Option<String> {
    match outcome {
        AssertionOutcome::Fail { reason }
        | AssertionOutcome::Malformed { reason }
        | AssertionOutcome::Errored { reason } => {
            if reason.is_empty() {
                None
            } else {
                Some(reason.clone())
            }
        }
        AssertionOutcome::Pass | AssertionOutcome::Deferred => None,
    }
}

fn fold_bucket(bucket: &mut BucketTotals, outcome: &AssertionOutcome) {
    match outcome {
        AssertionOutcome::Pass => bucket.passed += 1,
        AssertionOutcome::Fail { .. } => bucket.failed += 1,
        AssertionOutcome::Deferred => bucket.deferred += 1,
        AssertionOutcome::Malformed { .. } => bucket.malformed += 1,
        AssertionOutcome::Errored { .. } => bucket.errored += 1,
    }
}

#[cfg(test)]
mod tests {
    use serde_yaml_ng::Value;

    use super::*;
    use crate::content::evaluation::Op;
    use crate::content::evaluation::ScriptBody;
    use crate::content::evaluation::ScriptRuntime;
    use crate::content::evaluation::TokenMetric;
    use crate::content::evaluation::ToolCallSpec;

    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive fixture over every Assertion variant"
    )]
    fn all_variants() -> Vec<(Assertion, &'static str)> {
        vec![
            (
                Assertion::Judge {
                    prompt: String::new(),
                },
                "judge",
            ),
            (
                Assertion::Tool {
                    tool_call: ToolCallSpec {
                        tool: String::from("t"),
                        with_args: None,
                    },
                },
                "tool",
            ),
            (
                Assertion::Script {
                    runtime: ScriptRuntime::Python,
                    script: ScriptBody::Contents {
                        contents: String::from("x"),
                    },
                    pass_env: Vec::new(),
                },
                "script",
            ),
            (
                Assertion::Program {
                    script: String::from("./x"),
                    pass_env: Vec::new(),
                },
                "program",
            ),
            (
                Assertion::MustCallTool {
                    tool: String::from("t"),
                    with_args: None,
                },
                "must_call_tool",
            ),
            (
                Assertion::MustNotCallTool {
                    tool: String::from("t"),
                },
                "must_not_call_tool",
            ),
            (
                Assertion::ToolCallCount {
                    tool: None,
                    op: Op::Eq,
                    value: 0,
                },
                "tool_call_count",
            ),
            (
                Assertion::ToolCallOrder { sequence: vec![] },
                "tool_call_order",
            ),
            (
                Assertion::TextContains {
                    value: String::new(),
                    case_sensitive: None,
                },
                "text_contains",
            ),
            (
                Assertion::TextNotContains {
                    value: String::new(),
                    case_sensitive: None,
                },
                "text_not_contains",
            ),
            (
                Assertion::TextMatches {
                    pattern: String::new(),
                    flags: None,
                },
                "text_matches",
            ),
            (
                Assertion::TextEquals {
                    value: String::new(),
                },
                "text_equals",
            ),
            (
                Assertion::TextSemanticMatch {
                    value: String::new(),
                    threshold: None,
                },
                "text_semantic_match",
            ),
            (
                Assertion::JsonPath {
                    path: String::from("$"),
                    op: Op::Eq,
                    value: Value::Null,
                },
                "json_path",
            ),
            (
                Assertion::ResponseField {
                    path: String::from("$"),
                    exists: true,
                },
                "response_field",
            ),
            (
                Assertion::Tokens {
                    metric: TokenMetric::Total,
                    op: Op::Eq,
                    value: 0,
                },
                "tokens",
            ),
            (
                Assertion::LatencyMs {
                    op: Op::Eq,
                    value: 0,
                },
                "latency_ms",
            ),
        ]
    }

    #[test]
    fn class_tag_matches_serde_tag_for_every_variant() {
        for (assertion, expected_tag) in all_variants() {
            assert_eq!(class_tag(&assertion), expected_tag, "tag for {assertion:?}");
        }
    }

    mod orchestrator {
        use std::marker::PhantomData;

        use super::*;
        use crate::content::conversation::Content;
        use crate::content::conversation::Conversation;
        use crate::content::conversation::Message;
        use crate::content::conversation::Meta;
        use crate::content::conversation::ModelId;
        use crate::content::conversation::Role;

        fn conv(binding: &[(&str, &str)], assistant_text: &str) -> Conversation {
            let mut map = BindingMap::new();
            for (k, v) in binding {
                map.insert(String::from(*k), Value::from(*v));
            }
            Conversation {
                meta: Meta {
                    model: ModelId::from("noop"),
                    debug: false,
                    assembly: None,
                    binding: map,
                },
                session: vec![Message {
                    role: Role::Assistant,
                    body: Some(Content::Text(String::from(assistant_text))),
                    cache: false,
                    trace: None,
                    _phase: PhantomData,
                }],
            }
        }

        fn suite_with(cases: Vec<Case>) -> Evaluation {
            Evaluation {
                name: String::from("regression"),
                cases,
            }
        }

        fn case_named(name: &str, assertions: Vec<Assertion>) -> Case {
            Case {
                name: Some(String::from(name)),
                when: BindingMap::new(),
                assertions,
            }
        }

        fn case_when(when: &[(&str, &str)], assertions: Vec<Assertion>) -> Case {
            let mut map = BindingMap::new();
            for (k, v) in when {
                map.insert(String::from(*k), Value::from(*v));
            }
            Case {
                name: None,
                when: map,
                assertions,
            }
        }

        fn case_fanout(assertions: Vec<Assertion>) -> Case {
            Case {
                name: None,
                when: BindingMap::new(),
                assertions,
            }
        }

        #[test]
        fn case_tag_uses_sanitized_name_for_named_case() {
            let case = case_named("over-limit", vec![]);
            assert_eq!(case_tag(&case, 7), "over-limit");
        }

        #[test]
        fn case_tag_sanitizes_path_separators_in_name() {
            let case = case_named("a/b\\c", vec![]);
            assert_eq!(case_tag(&case, 0), "a_b_c");
        }

        #[test]
        fn case_tag_falls_back_to_index_for_when_filtered_case() {
            let case = case_when(&[("axis", "x")], vec![]);
            assert_eq!(case_tag(&case, 2), "case-2");
        }

        #[test]
        fn case_tag_falls_back_to_index_for_fanout_case() {
            let case = case_fanout(vec![]);
            assert_eq!(case_tag(&case, 0), "case-0");
        }

        #[tokio::test]
        async fn name_hit_runs_assertions_against_matched_conversation_only() {
            let suite = suite_with(vec![case_named(
                "alpha",
                vec![Assertion::TextContains {
                    value: String::from("hello"),
                    case_sensitive: None,
                }],
            )]);
            let conversations = vec![
                (PathBuf::from("alpha.yaml"), conv(&[], "hello world")),
                (PathBuf::from("beta.yaml"), conv(&[], "nope")),
            ];
            let args = EvalArgs {
                suite: &suite,
                conversations: &conversations,
                ctx: EvaluationContext::empty(),
                suite_name: "regression",
                run_id: "run-1",
                judge_output_dir: None,
            };

            let report = evaluate(args).await;

            assert_eq!(report.totals.assertions.passed, 1);
            assert_eq!(report.totals.conversations_matched, 1);
            assert_eq!(report.cases[0].matches.len(), 1);
            assert_eq!(report.cases[0].matches[0].conversation, "alpha.yaml");
        }

        #[tokio::test]
        async fn name_miss_synthesizes_missing_conversation_malformed() {
            let suite = suite_with(vec![case_named(
                "nope",
                vec![Assertion::TextContains {
                    value: String::new(),
                    case_sensitive: None,
                }],
            )]);
            let conversations = vec![(PathBuf::from("alpha.yaml"), conv(&[], ""))];
            let args = EvalArgs {
                suite: &suite,
                conversations: &conversations,
                ctx: EvaluationContext::empty(),
                suite_name: "regression",
                run_id: "run-1",
                judge_output_dir: None,
            };

            let report = evaluate(args).await;

            assert_eq!(report.totals.assertions.malformed, 1);
            assert_eq!(report.totals.conversations_matched, 0);
            assert_eq!(
                report
                    .per_class
                    .get("missing_conversation")
                    .copied()
                    .unwrap_or_default()
                    .malformed,
                1
            );
            assert_eq!(
                report.cases[0].matches[0].assertions[0].class,
                "missing_conversation"
            );
        }

        #[tokio::test]
        async fn when_filter_matches_subset_of_conversations() {
            let suite = suite_with(vec![case_when(
                &[("severity", "high")],
                vec![Assertion::TextContains {
                    value: String::from("alert"),
                    case_sensitive: None,
                }],
            )]);
            let conversations = vec![
                (
                    PathBuf::from("a.yaml"),
                    conv(&[("severity", "high")], "alert: a"),
                ),
                (PathBuf::from("b.yaml"), conv(&[("severity", "low")], "ok")),
                (
                    PathBuf::from("c.yaml"),
                    conv(&[("severity", "high")], "alert: c"),
                ),
            ];
            let args = EvalArgs {
                suite: &suite,
                conversations: &conversations,
                ctx: EvaluationContext::empty(),
                suite_name: "regression",
                run_id: "run-1",
                judge_output_dir: None,
            };

            let report = evaluate(args).await;

            assert_eq!(report.totals.conversations_matched, 2);
            assert_eq!(report.totals.assertions.passed, 2);
            assert_eq!(report.cases[0].matches.len(), 2);
        }

        #[tokio::test]
        async fn when_filter_zero_match_is_not_flagged_malformed() {
            let suite = suite_with(vec![case_when(
                &[("severity", "critical")],
                vec![Assertion::TextContains {
                    value: String::new(),
                    case_sensitive: None,
                }],
            )]);
            let conversations = vec![(PathBuf::from("a.yaml"), conv(&[("severity", "low")], ""))];
            let args = EvalArgs {
                suite: &suite,
                conversations: &conversations,
                ctx: EvaluationContext::empty(),
                suite_name: "regression",
                run_id: "run-1",
                judge_output_dir: None,
            };

            let report = evaluate(args).await;

            assert_eq!(report.totals.assertions.malformed, 0);
            assert_eq!(report.totals.conversations_matched, 0);
            assert!(report.cases[0].matches.is_empty());
        }

        #[tokio::test]
        async fn no_filter_case_fans_out_to_every_conversation() {
            let suite = suite_with(vec![case_fanout(vec![Assertion::TextContains {
                value: String::from("x"),
                case_sensitive: None,
            }])]);
            let conversations = vec![
                (PathBuf::from("a.yaml"), conv(&[], "x")),
                (PathBuf::from("b.yaml"), conv(&[], "x")),
                (PathBuf::from("c.yaml"), conv(&[], "x")),
            ];
            let args = EvalArgs {
                suite: &suite,
                conversations: &conversations,
                ctx: EvaluationContext::empty(),
                suite_name: "regression",
                run_id: "run-1",
                judge_output_dir: None,
            };

            let report = evaluate(args).await;

            assert_eq!(report.totals.conversations_matched, 3);
            assert_eq!(report.totals.assertions.passed, 3);
        }

        #[tokio::test]
        async fn every_outcome_bucket_flows_to_totals_and_per_class() {
            let suite = suite_with(vec![case_named(
                "alpha",
                vec![
                    Assertion::TextContains {
                        value: String::from("hello"),
                        case_sensitive: None,
                    },
                    Assertion::TextEquals {
                        value: String::from("no-match"),
                    },
                    Assertion::TextMatches {
                        pattern: String::from("[unclosed"),
                        flags: None,
                    },
                    Assertion::Judge {
                        prompt: String::from("anything"),
                    },
                ],
            )]);
            let conversations = vec![(PathBuf::from("alpha.yaml"), conv(&[], "hello world"))];
            let args = EvalArgs {
                suite: &suite,
                conversations: &conversations,
                ctx: EvaluationContext::empty(),
                suite_name: "regression",
                run_id: "run-1",
                judge_output_dir: None,
            };

            let report = evaluate(args).await;

            assert_eq!(report.totals.assertions.passed, 1);
            assert_eq!(report.totals.assertions.failed, 1);
            assert_eq!(report.totals.assertions.deferred, 1);
            assert_eq!(report.totals.assertions.malformed, 1);
            assert_eq!(
                report
                    .per_class
                    .get("text_contains")
                    .copied()
                    .unwrap_or_default()
                    .passed,
                1
            );
            assert_eq!(
                report
                    .per_class
                    .get("text_equals")
                    .copied()
                    .unwrap_or_default()
                    .failed,
                1
            );
            assert_eq!(
                report
                    .per_class
                    .get("text_matches")
                    .copied()
                    .unwrap_or_default()
                    .malformed,
                1
            );
            assert_eq!(
                report
                    .per_class
                    .get("judge")
                    .copied()
                    .unwrap_or_default()
                    .deferred,
                1
            );
        }

        #[test]
        fn collect_run_metrics_returns_none_metrics_when_no_message_carries_trace() {
            let conversations = vec![
                (PathBuf::from("a.yaml"), conv(&[], "hello")),
                (PathBuf::from("b.yaml"), conv(&[], "world")),
            ];
            let (model, metrics) = collect_run_metrics(&conversations);
            assert_eq!(model.as_deref(), Some("noop"));
            assert!(metrics.is_none());
        }

        #[test]
        fn collect_run_metrics_returns_none_for_empty_run() {
            let (model, metrics) = collect_run_metrics(&[]);
            assert!(model.is_none());
            assert!(metrics.is_none());
        }
    }
}

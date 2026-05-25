//! Eval orchestrator: pair suite cases with conversations, drive
//! `Assertion::check`, and assemble the on-disk report. Pure: I/O lives in
//! `cli::eval`.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;

use crate::content::conversation::BindingMap;
use crate::content::conversation::Conversation;
use crate::content::evaluation::Assertion;
use crate::content::evaluation::Case;
use crate::content::evaluation::Evaluation;
use crate::knowledge::assertions::AssertionOutcome;
use crate::knowledge::assertions::EvaluationContext;

/// Full report serialized to `<project>/evals/reports/<run-id>.json`.
/// Field names are the JSON keys; see DESIGN.md §evaluation for the contract.
#[derive(Serialize, Debug)]
pub struct EvalReport {
    pub suite: String,
    pub run_id: String,
    pub totals: ReportTotals,
    pub per_class: BTreeMap<String, ClassTotals>,
    pub cases: Vec<CaseReport>,
}

/// Top-level rollup. `conversations_matched` is the count of distinct
/// conversations that appeared in any case's matches list at least once.
#[derive(Serialize, Debug, Default)]
pub struct ReportTotals {
    pub conversations_matched: usize,
    pub assertions: BucketTotals,
}

/// Four-bucket verdict tally, exhaustive over
/// [`crate::knowledge::assertions::AssertionOutcome`].
#[derive(Serialize, Debug, Default, Clone, Copy)]
pub struct BucketTotals {
    pub passed: usize,
    pub failed: usize,
    pub deferred: usize,
    pub malformed: usize,
}

pub type ClassTotals = BucketTotals;

#[derive(Serialize, Debug)]
pub struct CaseReport {
    /// Present iff the suite case carried a `name:`. `when:`-filtered and
    /// no-filter cases serialize without a `name` field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub matches: Vec<MatchReport>,
}

#[derive(Serialize, Debug)]
pub struct MatchReport {
    /// Filename (with extension), not the full path. The run directory is
    /// already conveyed by `EvalReport::run_id`.
    pub conversation: String,
    pub assertions: Vec<AssertionReport>,
}

#[derive(Serialize, Debug)]
pub struct AssertionReport {
    pub class: String,
    pub outcome: &'static str,
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

    for case in &args.suite.cases {
        let matched: Vec<&(PathBuf, Conversation)> = matches_for(case, args.conversations);
        let mut match_reports: Vec<MatchReport> = Vec::with_capacity(matched.len());

        if matched.is_empty() && case.name.is_some() {
            let assertion = AssertionReport {
                class: String::from("missing_conversation"),
                outcome: outcome_label(&AssertionOutcome::Malformed {
                    reason: String::new(),
                }),
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
            for assertion in &case.assertions {
                let outcome = assertion.check(conv, &args.ctx).await;
                let class = class_tag(assertion);
                fold_bucket(&mut totals.assertions, &outcome);
                fold_bucket(per_class.entry(String::from(class)).or_default(), &outcome);
                assertion_reports.push(AssertionReport {
                    class: String::from(class),
                    outcome: outcome_label(&outcome),
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

    EvalReport {
        suite: String::from(args.suite_name),
        run_id: String::from(args.run_id),
        totals,
        per_class,
        cases,
    }
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
    }
}

fn reason_for(outcome: &AssertionOutcome) -> Option<String> {
    match outcome {
        AssertionOutcome::Fail { reason } | AssertionOutcome::Malformed { reason } => {
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
                },
                "script",
            ),
            (
                Assertion::Program {
                    script: String::from("./x"),
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
    }
}

//! Domain object for Ailly eval suites. Schema lives in `DESIGN.md`.

use std::sync::OnceLock;

use serde::Deserialize;
use serde::Serialize;

use super::conversation::BindingMap;

/// Matches the `case-<digits>` tag reserved for unnamed cases. A named case
/// matching this pattern would collide with the fallback tag the eval
/// orchestrator generates for `when:`-filtered and fan-out cases.
fn reserved_case_name_regex() -> &'static regex::Regex {
    static REGEX: OnceLock<regex::Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        regex::Regex::new(r"^case-\d+$")
            .expect("reserved-case-name regex is a compile-time constant")
    })
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Evaluation {
    pub name: String,
    pub cases: Vec<Case>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Case {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "BindingMap::is_empty")]
    pub when: BindingMap,
    pub assertions: Vec<Assertion>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Assertion {
    Judge {
        prompt: String,
    },
    Tool {
        tool_call: ToolCallSpec,
    },
    Script {
        runtime: ScriptRuntime,
        script: ScriptBody,
        /// Names (never values) of env vars this checker opts into receiving
        /// from the otherwise-cleared child env. Defaults empty;
        /// `#[serde(default)]` leaves every existing suite YAML
        /// unchanged.
        #[serde(default)]
        pass_env: Vec<String>,
    },
    Program {
        script: String,
        /// Names (never values) of env vars this checker opts into receiving
        /// from the otherwise-cleared child env. Defaults empty.
        #[serde(default)]
        pass_env: Vec<String>,
    },

    MustCallTool {
        tool: String,
        with_args: Option<serde_yaml_ng::Value>,
    },
    MustNotCallTool {
        tool: String,
    },
    ToolCallCount {
        tool: Option<String>,
        op: Op,
        value: i64,
    },
    ToolCallOrder {
        sequence: Vec<String>,
    },
    ToolCallCollection {
        tools: Vec<String>,
    },

    TextContains {
        value: String,
        case_sensitive: Option<bool>,
    },
    TextNotContains {
        value: String,
        case_sensitive: Option<bool>,
    },
    TextMatches {
        pattern: String,
        flags: Option<String>,
    },
    TextEquals {
        value: String,
    },
    TextSemanticMatch {
        value: String,
        threshold: Option<f64>,
    },

    JsonPath {
        path: String,
        op: Op,
        value: serde_yaml_ng::Value,
    },
    ResponseField {
        path: String,
        exists: bool,
    },

    Tokens {
        metric: TokenMetric,
        op: Op,
        value: i64,
    },
    LatencyMs {
        op: Op,
        value: i64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ToolCallSpec {
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub with_args: Option<serde_yaml_ng::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScriptRuntime {
    Node,
    Python,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum ScriptBody {
    Contents { contents: String },
    Path { path: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TokenMetric {
    Total,
    Input,
    Output,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Op {
    #[serde(rename = "==")]
    Eq,
    #[serde(rename = "!=")]
    NotEq,
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    LtEq,
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    GtEq,
}

#[derive(thiserror::Error, Debug)]
pub enum EvaluationError {
    #[error("evaluation input is empty")]
    Empty,
    #[error("failed to parse evaluation: {source}")]
    Parse {
        #[source]
        source: serde_yaml_ng::Error,
    },
    #[error("failed to emit evaluation: {source}")]
    Emit {
        #[source]
        source: serde_yaml_ng::Error,
    },
    #[error(
        "case[{index}] name {name:?} matches reserved pattern `case-<digits>`; rename to avoid collision with the unnamed-case tag"
    )]
    ReservedCaseName { index: usize, name: String },
}

impl Evaluation {
    /// Parse a single-document YAML eval suite.
    ///
    /// # Errors
    ///
    /// Returns [`EvaluationError::Empty`] when `input` is whitespace-only,
    /// [`EvaluationError::Parse`] when `serde_yaml_ng` rejects the document for
    /// any structural or variant-tag reason, and
    /// [`EvaluationError::ReservedCaseName`] when a case `name` matches the
    /// `case-<digits>` pattern reserved for unnamed-case tags (see
    /// [`crate::knowledge::eval`] `case_tag`).
    pub fn from_yaml_str(input: &str) -> Result<Self, EvaluationError> {
        if input.trim().is_empty() {
            return Err(EvaluationError::Empty);
        }
        let suite: Self =
            serde_yaml_ng::from_str(input).map_err(|source| EvaluationError::Parse { source })?;
        for (index, case) in suite.cases.iter().enumerate() {
            if let Some(name) = case.name.as_deref()
                && reserved_case_name_regex().is_match(name)
            {
                return Err(EvaluationError::ReservedCaseName {
                    index,
                    name: name.to_owned(),
                });
            }
        }
        Ok(suite)
    }

    /// Serialize this suite back to single-document YAML.
    ///
    /// # Errors
    ///
    /// Returns [`EvaluationError::Emit`] if the underlying YAML emitter
    /// rejects the document body.
    pub fn to_yaml_string(&self) -> Result<String, EvaluationError> {
        serde_yaml_ng::to_string(self).map_err(|source| EvaluationError::Emit { source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canonical fixture from `e2e/insurance-claim/README.md` lines 88–117.
    /// This is what a developer authoring an eval suite types into
    /// `evals/regression.yaml`, and what `ailly eval` must parse into the
    /// typed `Evaluation` value the assertion executors consume.
    const REGRESSION_YAML: &str = "\
name: regression
cases:
  - name: missing-fields
    assertions:
      - { type: must_call_tool, tool: lookup_policy }
      - { type: text_contains, value: \"policy number required\" }
      - { type: tool_call_count, op: \"<=\", value: 2 }
      - { type: tokens, metric: total, op: \"<\", value: 8000 }

  - name: ambiguous
    assertions:
      - { type: must_not_call_tool, tool: auto_approve }
      - { type: text_matches, pattern: \"clarif|specif\", flags: \"i\" }
      - { type: tokens, metric: output, op: \"<\", value: 500 }

  - name: over-limit
    assertions:
      - { type: must_not_call_tool, tool: auto_approve }
      - { type: tool_call_order, sequence: [lookup_policy, lookup_claim_history] }
      - type: judge
        prompt: |
          The response routes the claim to human-review and cites the
          $10,000 auto-approve ceiling from the constraints fragment.
";

    /// End-to-end feature: a developer's hand-authored `regression.yaml`
    /// parses into a typed `Evaluation`, every assertion variant is exposed as
    /// the right enum arm so an executor can dispatch on it, and the value
    /// round-trips back to YAML that re-parses to an equal value.
    #[test]
    fn regression_suite_parses_round_trips_and_exposes_typed_variants() {
        let suite = Evaluation::from_yaml_str(REGRESSION_YAML).expect("regression fixture parses");

        assert_eq!(suite.name, "regression");
        assert_eq!(suite.cases.len(), 3);

        let missing = &suite.cases[0];
        assert_eq!(missing.name.as_deref(), Some("missing-fields"));
        assert!(
            missing.when.is_empty(),
            "no `when:` in fixture means empty BindingMap"
        );
        assert_eq!(missing.assertions.len(), 4);
        assert!(matches!(
            &missing.assertions[0],
            Assertion::MustCallTool { tool, with_args: None } if tool == "lookup_policy"
        ));
        assert!(matches!(
            &missing.assertions[1],
            Assertion::TextContains { value, case_sensitive: None } if value == "policy number required"
        ));
        assert!(matches!(
            &missing.assertions[2],
            Assertion::ToolCallCount {
                tool: None,
                op: Op::LtEq,
                value: 2
            }
        ));
        assert!(matches!(
            &missing.assertions[3],
            Assertion::Tokens {
                metric: TokenMetric::Total,
                op: Op::Lt,
                value: 8000
            }
        ));

        let ambiguous = &suite.cases[1];
        assert_eq!(ambiguous.name.as_deref(), Some("ambiguous"));
        assert!(matches!(
            &ambiguous.assertions[0],
            Assertion::MustNotCallTool { tool } if tool == "auto_approve"
        ));
        match &ambiguous.assertions[1] {
            Assertion::TextMatches { pattern, flags } => {
                assert_eq!(pattern, "clarif|specif");
                assert_eq!(flags.as_deref(), Some("i"));
            }
            other => panic!("expected TextMatches, got {other:?}"),
        }
        assert!(matches!(
            &ambiguous.assertions[2],
            Assertion::Tokens {
                metric: TokenMetric::Output,
                op: Op::Lt,
                value: 500
            }
        ));

        let over_limit = &suite.cases[2];
        assert_eq!(over_limit.name.as_deref(), Some("over-limit"));
        match &over_limit.assertions[1] {
            Assertion::ToolCallOrder { sequence } => {
                assert_eq!(
                    sequence,
                    &vec![
                        "lookup_policy".to_string(),
                        "lookup_claim_history".to_string()
                    ]
                );
            }
            other => panic!("expected ToolCallOrder, got {other:?}"),
        }
        match &over_limit.assertions[2] {
            Assertion::Judge { prompt } => {
                assert!(prompt.contains("human-review"));
                assert!(prompt.contains("$10,000"));
            }
            other => panic!("expected Judge, got {other:?}"),
        }

        let emitted = suite.to_yaml_string().expect("typed suite emits");
        let reparsed = Evaluation::from_yaml_str(&emitted).expect("emitted suite re-parses");
        assert_eq!(
            suite, reparsed,
            "round-trip from_yaml_str ∘ to_yaml_string is identity at the value level"
        );
    }

    /// Every `Assertion` variant, every `Op` symbol, every `TokenMetric`,
    /// both `ScriptRuntime` runtimes and both `ScriptBody` shapes must
    /// survive `to_yaml_string ∘ from_yaml_str` unchanged. Guards against
    /// a future variant being added with a serde rename or tag that
    /// silently breaks dispatch.
    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive fixture over every Assertion variant"
    )]
    fn every_variant_round_trips() {
        let suite = Evaluation {
            name: "every-variant".to_string(),
            cases: vec![Case {
                name: Some("coverage".to_string()),
                when: BindingMap::default(),
                assertions: vec![
                    Assertion::Judge {
                        prompt: "is the response coherent?".to_string(),
                    },
                    Assertion::Tool {
                        tool_call: ToolCallSpec {
                            tool: "lookup_policy".to_string(),
                            with_args: Some(serde_yaml_ng::Value::String("ABC-123".to_string())),
                        },
                    },
                    Assertion::Script {
                        runtime: ScriptRuntime::Node,
                        script: ScriptBody::Contents {
                            contents: "process.exit(0)".to_string(),
                        },
                        pass_env: Vec::new(),
                    },
                    Assertion::Script {
                        runtime: ScriptRuntime::Python,
                        script: ScriptBody::Path {
                            path: "evals/scripts/check.py".to_string(),
                        },
                        pass_env: Vec::new(),
                    },
                    Assertion::Program {
                        script: "ailly-check".to_string(),
                        pass_env: Vec::new(),
                    },
                    Assertion::MustCallTool {
                        tool: "auto_approve".to_string(),
                        with_args: None,
                    },
                    Assertion::MustNotCallTool {
                        tool: "reject".to_string(),
                    },
                    Assertion::ToolCallCount {
                        tool: Some("lookup_policy".to_string()),
                        op: Op::Eq,
                        value: 1,
                    },
                    Assertion::ToolCallOrder {
                        sequence: vec!["a".to_string(), "b".to_string()],
                    },
                    Assertion::TextContains {
                        value: "approved".to_string(),
                        case_sensitive: Some(true),
                    },
                    Assertion::TextNotContains {
                        value: "rejected".to_string(),
                        case_sensitive: None,
                    },
                    Assertion::TextMatches {
                        pattern: "approv|deny".to_string(),
                        flags: Some("i".to_string()),
                    },
                    Assertion::TextEquals {
                        value: "ok".to_string(),
                    },
                    Assertion::TextSemanticMatch {
                        value: "claim handled".to_string(),
                        threshold: Some(0.85),
                    },
                    Assertion::TextSemanticMatch {
                        value: "default threshold".to_string(),
                        threshold: None,
                    },
                    Assertion::JsonPath {
                        path: "$.decision".to_string(),
                        op: Op::NotEq,
                        value: serde_yaml_ng::Value::String("reject".to_string()),
                    },
                    Assertion::ResponseField {
                        path: "$.usage".to_string(),
                        exists: true,
                    },
                    Assertion::Tokens {
                        metric: TokenMetric::Input,
                        op: Op::Gt,
                        value: 0,
                    },
                    Assertion::Tokens {
                        metric: TokenMetric::Total,
                        op: Op::GtEq,
                        value: 1,
                    },
                    Assertion::LatencyMs {
                        op: Op::Lt,
                        value: 5000,
                    },
                ],
            }],
        };

        let emitted = suite.to_yaml_string().expect("typed suite emits");
        let reparsed =
            Evaluation::from_yaml_str(&emitted).expect("emitted every-variant suite re-parses");
        assert_eq!(suite, reparsed);
    }

    #[test]
    fn reserved_case_name_is_rejected_with_index_and_name() {
        let yaml = "\
name: regression
cases:
  - name: ok-first
    assertions:
      - { type: text_contains, value: \"x\" }
  - name: case-3
    assertions:
      - { type: text_contains, value: \"y\" }
";
        match Evaluation::from_yaml_str(yaml) {
            Err(EvaluationError::ReservedCaseName { index, name }) => {
                assert_eq!(index, 1);
                assert_eq!(name, "case-3");
            }
            other => panic!("expected ReservedCaseName, got {other:?}"),
        }
    }

    #[test]
    fn case_name_with_trailing_text_after_digits_is_not_reserved() {
        // Only an exact `case-<digits>` collides with the fallback tag.
        let yaml = "\
name: regression
cases:
  - name: case-3-edge
    assertions:
      - { type: text_contains, value: \"x\" }
";
        Evaluation::from_yaml_str(yaml).expect("case-3-edge is a legal name");
    }
}

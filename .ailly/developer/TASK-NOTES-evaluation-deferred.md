# TASK-NOTES: Evaluation deferred decisions

Carried over from `2026-05-23-B-content-evaluation/design.md` (now removed). Each item is a decision to revisit when the named downstream consumer lands and forces the question. Until then the current shape in [src/content/evaluation.rs](../../src/content/evaluation.rs) stands.

## ToolCallSpec extension

`ToolCallSpec` carries `tool` and optional `with_args` today, and lives as a named struct alongside `Assertion::Tool { tool_call: ToolCallSpec }` rather than being inlined like `Assertion::MustCallTool`. The duplication is deliberate: the `tool` executor (a later task, not part of `eval-assertions-core`) may add fields such as `timeout`, `expected_exit_code`, or environment overrides. Revisit when that executor lands and either grow `ToolCallSpec` with the new fields, or collapse the duplication if the executor never accrues anything beyond `tool` + `with_args`.

## TextSemanticMatch.threshold as f64

`Option<f64>` on `TextSemanticMatch.threshold` is the reason `Assertion`, `Case`, and `Evaluation` derive `PartialEq` rather than `Eq`. The choice is deliberate: thresholds are domain-meaningful floating point, not integer ratios. Revisit if a real consumer requires structural `Eq` on these types, for example a dedup pass over loaded suites or a `HashSet<Assertion>`. The fix at that point is to normalize thresholds to a fixed-point integer (basis points, parts-per-thousand) and recover `Eq` derivation.

## Strict ScriptBody exclusivity

`ScriptBody` uses `#[serde(untagged)]` over `Contents { contents }` and `Path { path }`. A document with both keys today silently picks the first arm that matches. The design considered a custom `Deserialize` that rejects the combined form explicitly; deferred until a real fixture violates the implicit rule. Revisit when an authored eval suite ships with both keys present, or when the executor task is implemented and the ambiguity becomes observable downstream.

## Semantic validation on assertion shapes — resolved by eval-assertions-core

Resolved in `2026-05-24-B-eval-assertions-core` in favor of executor-level checks. `Assertion::TextMatches.flags` and `Assertion::ToolCallOrder.sequence` keep their permissive serde shapes; bad inputs surface at executor entry as `AssertionOutcome::Malformed { reason }`, distinguished from real assertion failures by the variant. No `try_from`-style validation constructor on `Assertion` was added. The decision stands unless a future fixture demands deserialize-time rejection.

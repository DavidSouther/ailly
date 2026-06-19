# TASK-NOTES: Assertion executor deferred decisions

Carried over from `2026-05-24-B-eval-assertions-core/design.md` (now removed). Each item is a decision to revisit when the named downstream consumer lands and forces the question. Until then the current shape in [src/knowledge/assertions.rs](../../src/knowledge/assertions.rs) stands.

The `text_semantic_match` runtime item from the design doc is not repeated here because it is already tracked as the `eval-judge` task in [TASKS.md](TASKS.md).

## Cache-hit token budgets

`TokenCounts` carries a `cache_hit` field but `TokenMetric` has no `CacheHit` variant; the `tokens` assertion family sums only `input`, `output`, or `input + output`. Cache-hit budgets are unobservable through the v1 schema. Revisit when an e2e fixture wants to assert a cache hit rate or ratio. The fix is one new `TokenMetric` arm plus one match arm in `Assertion::check`; no shape change to `Trace` or `TokenCounts`.

## First-result vs all-results for `json_path` with ordered ops

`json_path { path, op, value }` compares the *first* result of the JSONPath query against `value`. A path that matches multiple values has the remaining results ignored. Revisit when a fixture in `delegate-52` or `patterns-eval` needs *every match satisfies the op* or *exists a match satisfying the op*. The fix is an explicit quantifier on the assertion (e.g. `quantifier: any | all | first`) defaulting to `first` for back-compat.

## Reason-string format stability

`AssertionOutcome::Fail { reason }` and `AssertionOutcome::Malformed { reason }` carry short, mechanical strings illustrated in the design doc but not pinned by a contract. Revisit when the eval report consumer (next task: `knowledge/eval.rs eval-cmd`) lands and starts pattern-matching them, or when an external tool ingests the report JSON. The fix at that point is either to version the reason format or to lift the variant tag and key fields onto `Fail` as typed sub-fields, keeping the prose for human consumption only.

## `tool_call_order` strictness

`tool_call_order { sequence }` uses *subsequence* semantics: intervening calls between sequence elements are allowed, motivated by retry loops inserting noise. A fixture that needs *strict-contiguous* order or *no-other-calls-between* would justify a second variant (e.g. `tool_call_order_strict`) or an option field (`allow_interleaved: bool`). Revisit if any e2e calls for it.

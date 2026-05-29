# Eval Judge

## Problem Statement

The `Assertion::Judge { prompt }` arm of the Ailly eval system is declared in the schema ([src/content/evaluation.rs:26-28](../../../src/content/evaluation.rs#L26-L28)) but unwired in the executor: every judge assertion returns `AssertionOutcome::Deferred` ([src/knowledge/assertions.rs:71-75](../../../src/knowledge/assertions.rs#L71-L75)). Three feature tests pin this deferred behavior today. The patterns-eval and insurance-claim e2e projects depend on rubric judgments that a regex cannot reach, and both READMEs declare judge to be the missing piece. Until the judge executor is wired, rubric assertions in those suites produce no signal: they neither pass nor fail, they only defer.

This component implements `Assertion::Judge` as an LLM call against the conversation's current model, parses a structured verdict from the response, and records a verdict of pass, fail, errored, or malformed in the eval report.

## Prior Art

Survey of seven production frameworks and four academic works on LLM-as-judge verdict formats:

| Source | Verdict format | Note |
|---|---|---|
| Inspect AI (UK AISI / Anthropic) | `GRADE: [CPI]` line, **last-occurrence** regex | Prompt-injection mitigation: greedy regex binds to the last `GRADE:` line in the reply, not the first. |
| OpenAI Evals | Single Y/N/Unsure letter after CoT | Captured via `choice_strings`. |
| Promptfoo | JSON `{reason, score, pass}` | Defaults `pass=true` if `pass` is omitted (documented footgun). |
| DeepEval (G-Eval) | JSON schema-validated, reason-before-score | Uses provider native structured output when available. |
| Phoenix / Arize | Single classification word, "snap to rails" | Off-rail tokens snap to nearest rail. |
| LangChain | Trailing Y/N with four regex fallbacks | Fallback chain implies parse failures are common. |
| MT-Bench / Chatbot Arena | `[[A/B/C]]` markers | Position bias 30-60% flip rate documented. |
| G-Eval (Liu et al., 2023) | CoT + form-filling, probability-weighted Likert | +0.10-0.15 Spearman correlation with humans from CoT. |
| Prometheus 2 (Kim et al., 2024) | `[RESULT] N` 1-5 | Fine-tuned to comply. |
| AbstentionBench (2025) | (refusal rates) | Frontier judges refuse on 5-15% of borderline content; "fail-or-discard" handling silently biases results. |

Convergence direction in 2025-26: structured JSON validated by schema using provider native structured-output mode. Provider-native paths hit greater than 99% schema compliance versus around 85-95% for prompt-only JSON ([Carrick benchmark](https://carrick.tools/blog/benchmarking-llm-structured-outputs/)).

For Ailly's current adapter (unary completion, no tool forwarding, no JSON-schema mode), the production technique that survives is Inspect AI's `GRADE: [PFI]` last-occurrence regex. It is provider-agnostic, works against the existing `NoopEngine` and Anthropic `RigEngine` without changes, and mitigates prompt injection from the candidate response.

## Metrics

Acceptable operating range for the deployed component, drawn from the constraints the patterns-eval and insurance-claim regression suites already impose on themselves.

| Metric | Target | Why |
|---|---|---|
| Judge parse-failure rate | less than 5% on Claude Sonnet 4.6 and newer | Inspect AI reports ~99% compliance with `GRADE: [CPI]` on frontier models; the 5% budget covers refusals and inconclusive cases as `Malformed` rather than `Fail`. |
| Judge-call latency | p95 less than 8 s per assertion | Sonnet 4.6 typical completion latency for short rubric calls; the e2e patterns-eval workflow runs ≤ 9 judge assertions per suite, so a single suite stays under the 60s wall-clock budget. |
| Errored rate | less than 1% in CI | `AssertionOutcome::Errored` is reserved for transport failures; sustained > 1% indicates a CI infrastructure problem, not a model problem. |
| Report shape | Five buckets sum to total assertions | Closed-set invariant: `passed + failed + deferred + malformed + errored == total`. |
| Patterns-eval deferred-carry | Discovery: 0; Invocation: 3 | After this slice lands the only deferred assertions in patterns-eval are the three `script` runtimes (tracked under `knowledge: eval-script`). Judge assertions flow into Pass / Fail / Errored / Malformed buckets. |
| Insurance-claim deferred-carry | 0 judge deferred | The single judge assertion on `over-limit` flips out of `Deferred` into one of the four resolved outcomes. |

## Specification

### Public API changes

#### Fifth variant on `AssertionOutcome`

Today the enum at [src/knowledge/assertions.rs:19-33](../../../src/knowledge/assertions.rs#L19-L33) carries four variants: `Pass`, `Fail`, `Deferred`, `Malformed`. This slice adds a fifth, `Errored`:

```rust
pub enum AssertionOutcome {
    Pass,
    Fail { reason: String },
    Deferred,
    Malformed { reason: String },
    /// Collaborator was present but the call failed for environmental
    /// or transport reasons (auth, rate-limit, timeout, network).
    /// Distinct from `Malformed`, which is reserved for suite-authoring
    /// bugs, and from `Deferred`, which means the collaborator was absent.
    Errored { reason: String },
}
```

The new variant propagates to six change sites:

- `evaluate()`'s `fold_bucket` helper at [src/knowledge/eval.rs:336-343](../../../src/knowledge/eval.rs#L336-L343) gains an `Errored` arm.
- `outcome_label` at [src/knowledge/eval.rs:314-321](../../../src/knowledge/eval.rs#L314-L321) gains an `Errored` arm returning the `"errored"` label.
- `reason_for` at [src/knowledge/eval.rs:323-334](../../../src/knowledge/eval.rs#L323-L334) gains an `Errored` arm returning the carried reason string (parallel to `Fail` and `Malformed`).
- `BucketTotals` at [src/knowledge/eval.rs:65-71](../../../src/knowledge/eval.rs#L65-L71) gains an `errored: usize` field. Its doc-comment at [src/knowledge/eval.rs:63-64](../../../src/knowledge/eval.rs#L63-L64) is updated from "Four-bucket verdict tally" to "Five-bucket verdict tally". `ClassTotals` at [src/knowledge/eval.rs:73](../../../src/knowledge/eval.rs#L73) is a type alias on `BucketTotals` and inherits the change for the per-class rollup.
- `EvalCmdOutcome` at [src/cli/eval.rs:32-40](../../../src/cli/eval.rs#L32-L40) (currently six fields, none for errored) gains `assertions_errored: usize`. Consumers that currently destructure or construct `EvalCmdOutcome` (the binary, every feature test) must add the field.
- The exit-code logic in [src/cli/report.rs](../../../src/cli/report.rs) extends its failure predicate from `failed > 0` to `failed > 0 || errored > 0`.

The judge file-write described under *Judge conversation persistence* is new code in the `check_judge` helper, not a propagation of the `Errored` variant, so it is not listed here.

#### `EngineFactory` trait

Lives in [src/engine/engine.rs](../../../src/engine/engine.rs), alongside `EngineProvider`. Trait is dyn-compatible so `EvaluationContext` can hold a `&dyn EngineFactory`.

```rust
pub trait EngineFactory: Send + Sync {
    /// Resolve an engine for the given model id.
    /// # Errors
    /// Returns the underlying [`EngineError`] when no engine can be
    /// constructed (missing key, unknown provider, etc).
    fn open(&self, model: &ModelId) -> Result<Box<dyn EngineProvider>, EngineError>;
}

pub struct OpenEngineFor;

impl EngineFactory for OpenEngineFor {
    fn open(&self, model: &ModelId) -> Result<Box<dyn EngineProvider>, EngineError> {
        open_engine_for_model(model)
    }
}
```

`OpenEngineFor` is the production factory and reuses the existing per-model dispatch ([src/engine/engine.rs:111-120](../../../src/engine/engine.rs#L111-L120)). A test-only `ScriptedEngineFactory` wraps a single `NoopEngine` and returns boxed clones (or freshly built `NoopEngine::from_replies(...)` per call) for any model id.

#### `EvaluationContext` updated

Today [src/knowledge/assertions.rs:39-43](../../../src/knowledge/assertions.rs#L39-L43) defines a struct with a single `engine: Option<&'a dyn EngineProvider>` field, and the `impl` block at [src/knowledge/assertions.rs:45-52](../../../src/knowledge/assertions.rs#L45-L52) provides the `empty()` constructor. This slice renames and retypes the struct field to a factory:

```rust
pub struct EvaluationContext<'a> {
    pub engine_factory: Option<&'a dyn EngineFactory>,
}
```

`EvaluationContext::empty()` continues to return the no-collaborator context, now `engine_factory: None`. The `text_semantic_match` arm continues to return `Deferred` when `engine_factory` is `None`, matching the existing "collaborator absent" contract.

Today the deferred arms are collapsed in one `|`-chained match arm at [src/knowledge/assertions.rs:71-75](../../../src/knowledge/assertions.rs#L71-L75): `Judge | Tool | Script | Program | TextSemanticMatch => Deferred`. This slice splits `Judge` out into its own arm that dispatches to the new `check_judge` helper; the remaining four variants stay in the collapsed-defer arm until their own executors land.

### Judge executor

`Assertion::Judge { prompt }` flows through a new private helper `check_judge` in [src/knowledge/assertions.rs](../../../src/knowledge/assertions.rs):

1. Resolve engine: `let factory = ctx.engine_factory else { return Deferred }`; `let engine = factory.open(&conversation.meta.model).map_err(...)?;` → on `Err(EngineError)`, return `Errored { reason }`.
2. Extract context: `final_assistant_text(conversation)` (existing) and new `final_user_text(conversation)`. Missing assistant turn → `Fail { reason: "judge: no filled assistant turn" }`. Missing user turn → `Fail { reason: "judge: no filled user turn" }`.
3. Build the judge messages list (system + one user message) per the template below.
4. Call `engine.complete(CompletionRequest { model: conversation.meta.model.clone(), messages: &judge_msgs, debug: false }).await`. On `Err(EngineError)`, return `Errored { reason: <engine display> }`.
5. Lower the response to plain text (the same final-assistant-text logic, since the judge's own reply is one assistant turn).
6. Parse via the greedy-last regex `(?is).*GRADE\s*:\s*([PFI])`:
   - `P` → `Pass`
   - `F` → `Fail { reason: <trimmed CoT prefix, capped at 200 chars> }`
   - `I` → `Malformed { reason: format!("judge inconclusive: {prefix}") }`
   - No match → `Malformed { reason: format!("judge produced no GRADE line: {snippet}") }`

### Judge prompt template

The judge call sends a system message followed by one user message. The conversation under test is *not* replayed turn-by-turn; only the rubric, the final user turn, and the final assistant turn appear in the judge window.

```
[system]
You are an evaluator. Read the rubric and the candidate response.
Reason step-by-step about whether the candidate response satisfies
the rubric, then end your reply with one line:
  GRADE: P    (rubric satisfied)
  GRADE: F    (rubric violated)
  GRADE: I    (cannot decide)
The last GRADE line is binding. Do not print GRADE inside your reasoning.

[user]
RUBRIC:
<Assertion::Judge.prompt verbatim>

USER QUESTION:
<final_user_text(conversation)>

CANDIDATE RESPONSE:
<final_assistant_text(conversation)>
```

The system message is a `const &str` named `JUDGE_SYSTEM_PROMPT`. The user message is built per assertion via a small `format!` helper that owns the three labelled sections. CoT is instructed but not validated. The "last GRADE line is binding" sentence pairs with the greedy-last parser.

### Engine wiring at the CLI

[src/cli/eval.rs:67-89](../../../src/cli/eval.rs#L67-L89) replaces `EvaluationContext::empty()` with a factory:

```rust
let factory = crate::engine::engine::OpenEngineFor;
let ctx = EvaluationContext { engine_factory: Some(&factory) };
```

`EvalCmdArgs` gains no fields. The handler is unchanged structurally; the existing per-conversation iteration in `evaluate()` ([src/knowledge/eval.rs:166-226](../../../src/knowledge/eval.rs#L166-L226)) resolves the engine lazily on every judge assertion.

`EvalCmdOutcome` gains `assertions_errored: usize`. Callers of `run()` (the CLI binary and the feature tests) match on it the same way they already match on `assertions_malformed`.

### Judge conversation persistence

Ailly's contract from [README.md](../../../README.md) is "the conversation file is the run artifact; there is no parallel `window.txt`, `response.json`, or `meta.yaml`." A judge call is an LLM call, so it must persist as a conversation file. The question is *where*.

**Decision.** Judge conversations land under `evals/judges/<run-id>/<conv-stem>.<case-tag>.assertion-<M>.yaml`, sibling to the existing `evals/reports/<run-id>.json`. They are produced by `ailly eval`, not `ailly run`, and so belong with the other eval outputs.

Path naming:

- `<run-id>` matches the existing `derive_run_id` output ([src/cli/eval.rs:147-159](../../../src/cli/eval.rs#L147-L159)).
- `<conv-stem>` is the conversation filename stem (the same stem used for `name:` matching in the orchestrator).
- `<case-tag>` is the case's `name` when present, otherwise `case-<N>` where `N` is the zero-based index of the case in the suite's `cases:` array. Names survive suite reorderings; numeric tags are the fallback for `when:`-filter and fan-out cases that have no `name:`.

  The `case-<digits>` pattern is reserved for unnamed cases: the suite loader rejects an `Evaluation` whose `cases[*].name` matches `^case-\d+$` with a clear error. This makes the named-and-unnamed tag namespaces disjoint by construction; no two cases in one suite can resolve to the same case-tag.

  Today [src/content/evaluation.rs:166-171](../../../src/content/evaluation.rs#L166-L171) holds an `Evaluation::from_yaml_str` whose body is `empty-check → serde_yaml_ng::from_str`, with no semantic validation. This slice extends the function with a post-parse pass that walks `cases` and returns a new error variant for any name matching the reserved pattern. The `EvaluationError` enum at [src/content/evaluation.rs:142-156](../../../src/content/evaluation.rs#L142-L156) gains a fourth variant alongside `Empty`, `Parse`, and `Emit`:

  ```rust
  #[error("case[{index}] name {name:?} matches reserved pattern `case-<digits>`; rename to avoid collision with the unnamed-case tag")]
  ReservedCaseName { index: usize, name: String },
  ```

  The new feature test feeds a suite with `cases[0].name = "case-3"` and asserts `Evaluation::from_yaml_str` returns `Err(EvaluationError::ReservedCaseName { index: 0, name: "case-3" })`.

  Case names are sanitized for filesystem safety: path separators, control characters, and leading/trailing whitespace are replaced or trimmed before the tag enters the path. Sanitization is collision-free because the reserved-pattern rule rejects the only ambiguous shape.
- `<M>` is the zero-based index of the judge assertion within the case's `assertions:` list. Non-judge assertions do not contribute to the index, so deletion of an adjacent text-assertion does not renumber existing judge files.

Conversation shape — a regular Ailly conversation file:

```yaml
meta:
  model: <conv.meta.model>           # same model as the conversation under test
  assembly: judge                    # marker that ailly eval produced this
  binding: { ... }                   # carry conv.meta.binding verbatim so the
                                     # judge file is filterable by the same
                                     # axes as the conversation it judges
---
role: system
content: <JUDGE_SYSTEM_PROMPT verbatim>
---
role: user
content: |
  RUBRIC:
  <Assertion::Judge.prompt>

  USER QUESTION:
  <final_user_text(conv)>

  CANDIDATE RESPONSE:
  <final_assistant_text(conv)>
---
role: assistant
content: <judge reply, or absent on a failed/Errored call>
trace: <inline trace from the engine call, or absent>
```

Failure semantics map to the existing blank-assistant-slot contract:

| Outcome | File state |
|---|---|
| `Pass` / `Fail` (GRADE parsed) | Full file: system, user, filled assistant with the judge's reply and inline trace. |
| `Malformed` (no GRADE, or `GRADE: I`) | Full file: system, user, filled assistant with the unparseable reply. The `Malformed` reason in the report names the file so an operator can read what the judge actually said. |
| `Errored` (engine `complete` failed mid-call) | Partial file: system, user, blank assistant slot. The engine error is recorded in the file's per-message trace events. `ailly run <path>` refills the blank assistant turn on retry, completing the standard flaky-CI recovery flow. |
| `Errored` (factory `open` failed before any call) | No file. No conversation was started. The report records the reason. |
| `Deferred` (`engine_factory` is `None`) | No file. No conversation was started. |

Linkage from report to file is by convention, not by embedded path: the report's `class: judge` rows uniquely identify the file via `(run-id, conversation, case, assertion-index)`. Embedding the path on `AssertionReport` is a TASK-NOTES-eval-judge-deferred item.

Stale-file policy: when an assertion is deleted from the suite, its prior judge file under `evals/judges/<run-id>/` is *not* pruned by `ailly eval`. The orphan file remains until manually removed. Rationale: `ailly eval` is idempotent over the suite-of-the-moment, not over a history; pruning would require tracking previous suite states, which adds state outside the run directory. Operators notice orphans via `git status` in version-controlled projects. Adding an explicit warn-on-orphan pass is a TASK-NOTES-eval-judge-deferred item if the orphan rate becomes a CI nuisance.

Multi-suite-per-run-dir policy: the judge tree is keyed by `<run-id>` only, with no suite-name segment. Two suites that target the same run directory (e.g. `ailly eval discovery --over runs/X` then `ailly eval invocation --over runs/X`) write into the same `evals/judges/X/` folder. Collision is avoided in practice by the `<conv-stem>.<case-tag>.assertion-<M>` segment: case names rarely overlap across suites, and the assertion-index is local to the case. This inherits the existing report-path behavior at [src/cli/eval.rs:96](../../../src/cli/eval.rs#L96), where `evals/reports/<run-id>.json` is also un-suite-segmented and the last write wins. Adding a `<suite-slug>` segment to both the report and the judge tree is a TASK-NOTES-eval-judge-deferred item; the trigger is the first suite pair observed to actually collide.

Version control: e2e projects opt in or out via `.gitignore`. Default behavior is to commit judge files alongside the report, on the principle that prompt drift in the judge harness is itself a regression worth catching across runs.

### Data flow

```
ailly eval <suite> --over <run-dir>
  └─ cli::eval::run
       ├─ Project::open
       ├─ load suite + conversations
       ├─ let factory = OpenEngineFor
       ├─ EvaluationContext { engine_factory: Some(&factory) }
       └─ knowledge::eval::evaluate(...)
            └─ for each (case, conversation):
                 for each assertion:
                    Assertion::check(conv, &ctx)
                       └─ Judge
                          ├─ factory.open(conv.meta.model)
                          ├─ build judge Conversation (system, user, blank assistant)
                          ├─ engine.complete  ─┐
                          │                    └─ on Ok: fill assistant turn + trace
                          │                       on Err: leave blank, write trace event
                          ├─ persist conversation to
                          │     evals/judges/<run-id>/<conv-stem>.<case-tag>.assertion-<M>.yaml
                          └─ parse GRADE: [PFI] from assistant turn
                 └─ assemble AssertionReport per assertion
       └─ write evals/reports/<run-id>.json
```

Two file artifacts per `ailly eval`: the existing JSON report and the new tree of judge conversations.

### Error handling, decision table

| Cause | Outcome |
|---|---|
| `engine_factory` is `None` | `Deferred` (preserves existing test contract: empty context → defer). |
| `factory.open()` returns `Err(EngineError)` | `Errored { reason: "judge engine open: <err>" }`. |
| `engine.complete()` returns `Err(EngineError)` | `Errored { reason: "judge engine complete: <err>" }`. |
| Judge reply has no `GRADE:` line | `Malformed { reason: "judge produced no GRADE line: <snippet>" }`. |
| Judge reply ends `GRADE: I` | `Malformed { reason: "judge inconclusive: <CoT prefix>" }`. |
| Judge reply ends `GRADE: P` | `Pass`. |
| Judge reply ends `GRADE: F` | `Fail { reason: "<CoT prefix>" }`. |
| `final_assistant_text(conv)` is `None` | `Fail { reason: "judge: no filled assistant turn" }`. |
| `final_user_text(conv)` is `None` | `Fail { reason: "judge: no filled user turn" }`. |

### Testing

One new feature test, `tests/eval_judge.rs`:

- Uses a `ScriptedEngineFactory` that returns `NoopEngine::from_replies([...])` with rubric-by-rubric scripted judge replies exercising all four parse paths: `GRADE: P`, `GRADE: F`, `GRADE: I`, and "no GRADE line".
- Asserts one `Pass`, one `Fail` (with reason containing the CoT prefix), one `Malformed` (inconclusive), one `Malformed` (no GRADE).
- A second sub-test seeds a factory whose `open()` returns `Err(EngineError::Auth { .. })` and asserts the outcome is `Errored` and no judge file is written.
- A third sub-test seeds a factory whose `complete()` returns `Err(EngineError::Timeout)` (via a scripted `NoopEngine` that pops a queued error) and asserts the outcome is `Errored` and the judge file exists with a blank assistant turn plus an error trace event.
- A fourth sub-test runs against a fan-out case (no `name:`) over two conversations and asserts the resulting judge files use the `case-<N>` numeric tag for both, with distinct `<conv-stem>` segments.
- A fifth sub-test feeds a suite whose `cases[0].name = "case-3"` to `Evaluation::from_yaml_str` and asserts a parse error naming the reserved-pattern rule, exercising the namespace-disjointness invariant.

Existing feature test updates:

| File | Update |
|---|---|
| [tests/eval_assertions.rs](../../../tests/eval_assertions.rs) | Rename `engine` to `engine_factory` on every `EvaluationContext` literal. Update the existing "judge defers when no engine" pin so the assertion still defers when `engine_factory` is `None`. Add one scripted-judge pass case. |
| [src/knowledge/assertions.rs](../../../src/knowledge/assertions.rs) inline tests | The inline `remote_variants_return_deferred_when_context_is_empty` test in the `#[cfg(test)] mod tests` block constructs an `EvaluationContext` and asserts the five LLM-backed variants defer with an empty context. After the field rename it still asserts the same shape with `engine_factory: None`. |
| [tests/eval_insurance_claim.rs](../../../tests/eval_insurance_claim.rs) | Thread a `ScriptedEngineFactory` (test helper) so the judge on `over-limit` resolves; update the `assertions_deferred == 1` assertion (currently at the line that follows the cited deferred-carry counts). |
| [tests/eval_cmd.rs](../../../tests/eval_cmd.rs) | Judge case flips from `Deferred` to `Errored` in the no-key environment (the CLI uses `OpenEngineFor`); update the bucket and per-class assertions. This update is only writable once `EvalCmdOutcome` gains `assertions_errored`, so the plan orders the struct change before the test edit. |
| [tests/e2e_patterns_eval.rs](../../../tests/e2e_patterns_eval.rs) | Deferred-carry totals shift. Discovery: 2 judge `deferred` → 2 judge `errored`. Invocation: 6 `deferred` (3 script + 3 judge) → 3 `deferred` (script) + 3 `errored` (judge). |

The CLI tests stay no-network: `OpenEngineFor` will fail `Auth` in CI, surfacing as `Errored`, which is the deliberate fingerprint of "judge wired but environment absent".

## Alternatives

| Alternative | Why rejected |
|---|---|
| Forced single-tool verdict | Requires closing the deferred "tool-definition wiring on requests" item in TASK-NOTES-engine-deferred; doubles slice size; locks out open-weight providers without function-calling templates. Recorded as a TASK-NOTES-eval-judge-deferred follow-up: switch verdict format to forced tool-call when Rig adapter forwards tools. |
| Structured JSON in body (DeepEval/Promptfoo style) | Reliability tracks provider's structured-output support; mediocre without JSON-schema mode, which the Rig adapter does not forward. The same TASK-NOTES item gates the upgrade. |
| Judge model via `judge_model:` suite field or env var | Adds DESIGN.md surface; not requested by any current e2e fixture. Self-preference bias from same-model judging is documented as a known limitation in the deferred list. |
| `text_semantic_match` runtime in this slice | Field has a `threshold: f64` that points at embedding cosine, not LLM-judge. Bundling both into one slice locks in a wire format; deferred until a real fixture forces the question. |
| Map engine errors to `Malformed` (no new variant) | Conflates rubric typos with environment failures; operators can't tell flaky CI from suite bugs at a glance. |
| Map engine errors to `Deferred` (no new variant) | Stretches the "collaborator absent" contract to "collaborator present but failed"; loses the reason string. |
| Pre-resolved engine map keyed by `ModelId` (Approach C) | New pre-resolution phase in CLI; awkward lifetime in tests; doesn't mirror the existing `EngineProvider` adapter pattern. |
| Per-conversation orchestrator loop (Approach A) | Splits report assembly between orchestrator and CLI; bigger refactor than the factory abstraction. |
| Judge conversations under `runs/<run-id>/judges/` | Conflates the "produced by `ailly run`" contract with the "produced by `ailly eval`" contract: re-running eval would mutate the run folder, and two suites against the same run would clobber each other. |
| Judge conversations nested in `evals/reports/<run-id>/...` | Breaking change to DESIGN.md's documented `evals/reports/<run-id>.json` path; would require a backward-compat shim. |
| Judge conversations not persisted | Violates the "conversation file is the run artifact" pitch; loses replay, loses CI diff-against-prior-run on the judge harness itself. |

## Summary

### Locked decisions

1. **Verdict format.** Inspect AI's `GRADE: [PFI]` line, greedy-last regex. CoT instructed in the system message. The last `GRADE:` line is binding.
2. **Judge model.** Per-conversation `conv.meta.model` via `EngineFactory::open`. Self-preference bias is a known limitation in the deferred list.
3. **Scope.** `Assertion::Judge` only. `Assertion::TextSemanticMatch` continues to return `Deferred`.
4. **Engine errors.** New `AssertionOutcome::Errored { reason }` variant. Five buckets sum to total assertions.
5. **Judge context.** Rubric + final user turn + final assistant turn. New extractor `final_user_text` parallel to the existing `final_assistant_text`.
6. **Architecture.** Approach B: `EngineFactory` trait, threaded through `EvaluationContext::engine_factory`.
7. **Judge conversation persistence.** Every judge call writes a regular Ailly conversation file under `evals/judges/<run-id>/<conv-stem>.<case-tag>.assertion-<M>.yaml`, sibling to `evals/reports/<run-id>.json`. Failed-mid-call writes a blank assistant turn (replayable via `ailly run`); failed-before-call writes nothing.

### Deferred decisions

Tracked in `docs/developer/TASK-NOTES-eval-judge-deferred.md`:

- **Forced tool-call verdict format.** Switch from `GRADE: [PFI]` to a forced `verdict(pass, reason)` tool when the Rig adapter forwards tool definitions (already a TASK-NOTES-engine-deferred item). Trigger: tool-wiring slice lands.
- **Judge model override.** Add `judge_model:` to the eval suite schema or `AILLY_JUDGE_MODEL` env var when a fixture needs to pin a stronger judge or break self-preference bias. Trigger: first fixture that asks.
- **Judge-call cost accounting.** Judge tokens are not summed into `RunMetrics`. Add `JudgeMetrics { calls, input_tokens, output_tokens, latency_ms }` when CI cost monitoring requests it. Trigger: cost-aware CI step.
- **`text_semantic_match` runtime.** Decide between embedding cosine (uses `threshold: f64`) versus a thin LLM-judge wrapper (ignores threshold) when the first e2e fixture forces the question.
- **Refusal-aware judge prompt.** Current template treats refusals as `Malformed` via `GRADE: I`. Per AbstentionBench, frontier judges refuse 5-15% of borderline content; if observed in our suites, refine the prompt to explicitly permit `GRADE: I` and document the rate.
- **Position bias mitigation.** If `Assertion::Judge` is ever extended to comparative (A versus B) rubrics, MT-Bench's swap-and-require-consistency dual-call mitigation goes here.
- **Judge file linkage on `AssertionReport`.** Linkage from the report's `class: judge` rows to the persisted judge conversation files is by convention today: `(run-id, conversation, case-tag, assertion-index)` uniquely names the file. Adding an optional `judge_conversation: <path>` field to `AssertionReport` survives file moves and makes the linkage explicit for downstream tooling. Trigger: first consumer that reads the report JSON and wants to open the judge file.

# TASK-NOTES: eval-judge deferred decisions

Deferred decisions from the `2026-05-28-A-eval-judge` topic, which wired
`Assertion::Judge` to the `check_judge` executor. Each item records its trigger
condition. None is active work; pick one up only when its trigger fires.

- **Per-conversation engine dispatch.** The CLI binds one engine to the run's
  first conversation model for the entire eval pass. A heterogeneous run
  (multiple `meta.model` values across conversations) judges every conversation
  with the first model's engine, which is wrong if those models live on
  different providers. **Trigger:** first fixture that mixes models within a
  single run dir. Resolution either reintroduces a factory or splits the run
  into per-model sub-passes.

- **Forced tool-call verdict format.** Switch from `GRADE: [PFI]` to a forced
  `verdict(pass, reason)` tool when the Rig adapter forwards tool definitions
  (already a TASK-NOTES-engine-deferred item). **Trigger:** tool-wiring slice
  lands.

- **Judge model override.** Add `judge_model:` to the eval suite schema or
  `AILLY_JUDGE_MODEL` env var when a fixture needs to pin a stronger judge or
  break self-preference bias. **Trigger:** first fixture that asks.

- **Judge-call cost accounting.** Judge tokens are not summed into `RunMetrics`.
  Add `JudgeMetrics { calls, input_tokens, output_tokens, latency_ms }` when CI
  cost monitoring requests it. **Trigger:** cost-aware CI step.

- **`text_semantic_match` runtime.** Decide between embedding cosine (uses
  `threshold: f64`) versus a thin LLM-judge wrapper (ignores threshold) when the
  first e2e fixture forces the question. `Assertion::TextSemanticMatch`
  continues to return `Deferred` until then.

- **Refusal-aware judge prompt.** Current template treats refusals as
  `Malformed` via `GRADE: I`. Per AbstentionBench, frontier judges refuse 5-15%
  of borderline content; if observed in our suites, refine the prompt to
  explicitly permit `GRADE: I` and document the rate.

- **Position bias mitigation.** If `Assertion::Judge` is ever extended to
  comparative (A versus B) rubrics, MT-Bench's swap-and-require-consistency
  dual-call mitigation goes here.

- **Judge file linkage on `AssertionReport`.** Linkage from the report's
  `class: judge` rows to the persisted judge conversation files is by convention
  today: `(run-id, conversation, case-tag, assertion-index)` uniquely names the
  file. Adding an optional `judge_conversation: <path>` field to
  `AssertionReport` survives file moves and makes the linkage explicit for
  downstream tooling. **Trigger:** first consumer that reads the report JSON and
  wants to open the judge file.

- **Stale / orphan judge files.** When an assertion is deleted from the suite,
  its prior judge file under `evals/judges/<run-id>/` is not pruned by
  `ailly eval`. Operators notice orphans via `git status`. Add an explicit
  warn-on-orphan pass **if** the orphan rate becomes a CI nuisance.

- **Multi-suite-per-run-dir collision.** The judge tree is keyed by `<run-id>`
  only, with no suite-name segment, mirroring `evals/reports/<run-id>.json`.
  Two suites targeting the same run dir share `evals/judges/<run-id>/`;
  collision is avoided in practice by the `<conv-stem>.<case-tag>.assertion-<M>`
  segment. Add a `<suite-slug>` segment to both the report and judge tree.
  **Trigger:** first suite pair observed to actually collide.

# Synthetic calibration data for five zero-candidate judges

This directory is a standalone `ailly` project generating **real, live**
calibration candidates for five `patterns-eval` judges that had zero
relevant candidates after organic session mining:

1. `patterns-eval/baseline/newtype`
2. `patterns-eval/invocation/newtype`
3. `patterns-eval/baseline/emitting-logs`
4. `patterns-eval/invocation/emitting-logs`
5. `patterns-eval/discovery/paired-log-handler-success`

Unlike the mined data under `e2e/judge-calibration/mined/` (gitignored,
excerpts of private client sessions), everything here is freshly authored
and freely committable: new prompts, run for real through the Anthropic
API via `ailly assemble` + `ailly run`, scored for real via `ailly eval`.
Nothing here is hand-written or fabricated model output.

## Layout

```
prompts/synthetic/newtype/{1,2,3}.md                       # new prompts, same shape as patterns-eval's canonical newtype.md
prompts/synthetic/emitting-logs/{1,2,3}.md                  # new prompts, same shape as patterns-eval's canonical emitting-logs.md
prompts/synthetic/paired-log-handler-success/{1,2,3}.md     # new prompts, same shape as the canonical discovery case

assemblies/synthetic-newtype-baseline.yaml                  # prefix copied verbatim from patterns-eval/assemblies/baseline.yaml
assemblies/synthetic-newtype-invocation.yaml                # prefix copied verbatim from patterns-eval/assemblies/invocation.yaml
assemblies/synthetic-emitting-logs-baseline.yaml
assemblies/synthetic-emitting-logs-invocation.yaml
assemblies/synthetic-paired-log-handler-success-discovery.yaml  # prefix copied verbatim from patterns-eval/assemblies/discovery.yaml

evals/synthetic-*.yaml                                      # eval suites; each case's `judge` assertion prompt is copied
                                                              # verbatim from the matching case in
                                                              # e2e/patterns-eval/evals/{baseline,invocation,discovery}.yaml
evals/scripts/                                               # copies of patterns-eval's structural checkers (check_newtype.py, check_emitting_logs.py)
evals/judges/                                                # full system+user+assistant transcript of every live judge call (committed, not gitignored)

conversations/patterns-eval__baseline__newtype/{1,2,3}.yaml            # real, run conversation files -- destination for calibration integration
conversations/patterns-eval__invocation__newtype/{1,2,3}.yaml
conversations/patterns-eval__baseline__emitting-logs/{1,2,3}.yaml
conversations/patterns-eval__invocation__emitting-logs/{1,2,3}.yaml
conversations/patterns-eval__discovery__paired-log-handler-success/{1,2,3}.yaml

judge_verdicts.yaml                                          # manifest: conversation -> real live judge verdict (pass/fail) + full reasoning text
```

`context/` (`AGENTS.md`, `context/AGENTS.md`, `context/skills/*`) is a
direct copy of the same files `patterns-eval` uses, so the baseline /
invocation / discovery prefixes here are byte-identical in content to the
real suite's prefixes -- only the prompt files and matrix axis (`variant`
instead of `skill`) differ, so filenames land as flat `1.yaml`/`2.yaml`/
`3.yaml` per run directory instead of joining a multi-value matrix.

## Why a separate project instead of extending patterns-eval

`e2e/patterns-eval`'s own `assemblies/`/`prompts/`/`evals/` are a
deliberately stable, minimal CI fixture set (see its `ci.sh` falsification
gate). Growing that fixture count was explicitly out of scope for this
data-generation pass; this sibling project reuses its prefixes and judge
prompts without touching it.

## How this was produced

For each of the three prompt variants per skill:

```sh
ailly -p e2e/judge-calibration/evals/synthetic assemble synthetic-newtype-baseline
ailly -p e2e/judge-calibration/evals/synthetic run runs/<ts>-synthetic-newtype-baseline/
ailly -p e2e/judge-calibration/evals/synthetic eval synthetic-newtype-baseline --over runs/<ts>-synthetic-newtype-baseline/
# ...same for -invocation, emitting-logs-{baseline,invocation}, and paired-log-handler-success-discovery
```

Each `assemble`/`run` pair made real Anthropic API calls
(`model: claude-sonnet-4-6`, matching the real suites). The resulting
`runs/<ts>-.../{1,2,3}.yaml` conversation files were copied verbatim (no
edits) into `conversations/<judge-slug>/{1,2,3}.yaml`. `ailly eval` then
ran each suite's `judge` assertion (rubric text copied verbatim from the
matching real case) live against those same conversations; the verdict
and the judge's full raw reply were captured into `judge_verdicts.yaml`
and the per-call transcript under `evals/judges/`.

## A calibration-relevant finding from this run

The `emitting-logs` judge prompt names the canonical fixture's literal
vocabulary (`order.placed`, `order.id`, `user.id`). All six new
emitting-logs conversations (`baseline` and `invocation`) describe a
different business event (a refund, a shipment, a signup) and therefore
use different, equally-correct field names (`refund.id`, `shipment.id`,
`user.id` vs `customer.id`, etc.) — the live judge fails every one of
them for not matching the literal canonical names, even where the
candidate structurally applies the `emitting-logs` pattern correctly
(stable message body, `eventName` set, OTel-shaped keys). Similarly, the
`paired-log-handler-success` discovery case's `text_not_contains` script
assertion fails on all three new prompts because the model's answer
explains *why* `patterns:configuring-logging` does not apply (naming it
in a comparison table) even as the `judge` assertion for the same case
correctly passes. Both are exactly the kind of judge/human disagreement
this calibration effort exists to surface — see `judge_verdicts.yaml` for
the full reasoning text of each verdict.

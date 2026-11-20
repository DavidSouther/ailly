# Code Mode

## When Code Mode Applies

Code Mode is for a "few-off" script or automation, rather than
an application feature. Before treating anything as script-shaped, apply the
routing test — is this task script-shaped, or is it an ambiguous *feature*
inside an app? A feature is not for Code Mode; it belongs to the standard
loop (or quick-loop once disambiguated). This script-vs-feature boundary is
the load-bearing distinction between Code Mode and every other shape here.

Examples of script-shaped tasks for codemode include a file-system loop over existing tools, a read-and-compute pass over scattered inputs,
filtering and counting log entries by date range,
or querying a SQL-backed dataset. Code mode is too much for a truly one-off task,
summing a column across six reports,
cross-referencing two CSVs for mismatched IDs,
pulling config values from scattered files into a byte budget, or calling a tool with a few files.

When deciding code mode vs directly LLM task, can the LLM both perform the task and verify its
own answer? Weigh three angles: how large and clean the source materials
are, how much data the computation requires once extracted, and how complex
the computation itself is. Cost and rerun count apply when all three
are small and clean, and only past roughly five reruns; everywhere else
correctness or feasibility decides, sometimes at a single run, since a real
statistical computation an LLM cannot verify by reasoning belongs in a script
from its first run. A readable script also beats opaque reasoning on
auditability. Judgment-heavy work, such as fuzzy text matching or messy
entity resolution, usually calls for a hybrid: a script handles the
mechanical bulk and the LLM adjudicates the ambiguous residual. See
`code-mode-thresholds.md` in this directory for the full threshold tables,
cost grounding, and worked examples behind this test.

Code Mode is opt-in at session start, entered by a distinct trigger phrase,
the same way quick-loop and long-loop are entered ("code mode", "write a
script to ..."). It is also reachable through `developer:ailly`'s Routing
table, which carries a row pointing back at this reference.

## Condensed Ailly Loop

Make one `research:public` call to check whether an
existing tool already fits, using SQL for data tasks against a database;
shell (or powershell) for filesystem scripting, or Python for light complexity tasks.
"Check spelling across these dozen files" should surface `vale.sh` or `hunspell`
as the tool the script wraps, not a from-scratch spell checker; for data in
a database connection, run the SQL query directly. Record the
finding, either the tool chosen or "hand-written because none fits", in the
combined planning doc.
Add the design and plan into a single short document in the session
folder, in place of the standard loop's three separate artifacts.

One human-cleared draft gate covers the combined planning doc. Run the coordinator's quick `general:review`, including Intent review, on the combined doc before the draft gate and after any material revision.
The developer resolves each question at the single draft gate below.
Once the
developer clears it, the build runs. The standard human merge gate still applies
and is never auto-cleared. Code Mode does not auto-clear like quick-loop —
keeping this one human beat is precisely what lets Code Mode take on ambiguous
work that quick-loop must refuse.

Write the script, run it once, and inspect the output. There is no feature test
and no red-green-refactor TDD loop: Code Mode scripts stay trivial enough by
construction — minimal logic, simple file-system loops, no branching or design
patterns — that a human can review them at a glance instead. The single
inspected run stands in for the feature test. The coordinator's quick and full
Build reviews check that run, and Build is ready for handoff when its output is
accepted, the required checks pass, and no blockers remain. Run both Build
reviews before handing the script back. Code Mode scripts
typically land in a `scripts/` folder, or wherever else the project's
conventions put standalone automation.

## Guidance for Spawned Sessions

Any generated script that spawns further headless agent sessions follows four
concrete rules:

- **Headless dispatch.** Dispatch each session with `claude --print` (`-p`) in
  a non-prompting permission mode, not an interactive session.
- **Restricted tools.** Restrict each dispatched session with a minimal or
  empty `--allowedTools` set — no agents, tool access, or anything beyond what
  the specific task needs.
- **Bare-alias model.** Express the model as a bare alias, e.g. `haiku`, per
  `model-selection.md` — not a dated pin like `haiku-4.5`. A dated pin names one
  specific model rather than asking for "the latest," so the bare-alias rule
  still applies.
- **Capped concurrency.** Cap concurrency with `xargs -P N` (or a counting
  semaphore) rather than hand-rolled pool bookkeeping — for example `-P 8` to
  cap outstanding sessions at eight.

## Worked Example

This walks the vale-autofix scenario through the loop above, as an
illustration of the shape, not a build target — the script itself is tracked
in its own session.

The prompt asks for a coordination script that runs Vale across the repo,
writes per-file findings, and dispatches capped concurrent headless sessions
to fix the warnings, naming `haiku-4.5` as the model in the prompt's own
wording. Cursory research makes one `research:public` call and surfaces
`vale.sh` as an existing off-the-shelf linter, so the script wraps it instead
of hand-rolling a style checker. The combined planning doc records that wrap
plan, the per-file findings path `.ailly/prompts/vale/`, and the dispatch
shape `xargs -P 8 claude -p --model haiku --allowedTools ...` — normalizing
the prompt's `haiku-4.5` to the bare alias per the guidance above. The
developer clears the single draft gate on that doc, and Build writes and runs
the coordination script once, then inspects the per-file findings it
produced.

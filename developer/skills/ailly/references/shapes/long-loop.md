# Long Loops

*Ailly with Dynamic Workflows*

Follow the coordinator's Phase Reviews schedule. Run each quick draft review before dispatching the research-and-decide reviewer. That reviewer resolves open questions and clears the `*Draft*` marker through the active harness's isolation path. After full Build review, follow Build Handoff before Cleanup.

## 1. When the long loop applies

The long loop is opt-in at session start, declared once and applied for the run, recognized via phrasings like "run a long loop", "dynamic workflow", or "run \<project\> to completion". The long loop keeps the deliberation and full-fidelity artifacts and delegates draft-gate decisions to the research-and-decide reviewer. Both loops run every phase through the active harness's isolation path. The long loop can handle ambiguous, high-blast-radius, or security-sensitive work.

## 2. The research-and-decide reviewer contract

At each draft gate the coordinator dispatches a fresh reviewer runner, scoped to a single artifact (document, code diff, or similar) and read cold. Its contract, stated as the dispatch instruction:

- Read the artifact cold without including prior session context.
- Find the artifact's open items (the "Open for human review" / "Summary / deferred decisions" / deferred-decisions slot) and research them using the `research:` skills and the repo conventions.
- Make the conservative default decision for each item.
- Record each decision with rationale in the artifact (format below).
- Escalate rather than decide when a trigger fires (rule in section 4): leave the gate uncleared and flag the item `ESCALATE: <why>`.
- Remove the `*Draft*` marker once every blocking item is decided.

This is a single cheap isolated reviewer per gate, not a fan-out ensemble. A standalone LLM reviewer suggests candidates to be validated rather than final truth, so one conservative decide-runner with a recorded audit trail is the cheap, low-risk default. Model choice is left to the coordinator; the reference stays model-agnostic.

## 3. The decision-recording format

Resolve each artifact's needed decisions in place. There is no separate `decisions.md`. Convert the open slot into a block headed `Resolved by the long-loop reviewer (YYYY-MM-DD)`, one entry per open item, in the format:

```
**N. <question or item title>. Decided: <the decision>.** <rationale, including
what was researched and why this is the conservative default>
```

Escalated items use the same block:

```
**N. <item title>. ESCALATE: <why this could not be decided conservatively>.**
```

A worked entry, taken from this session's own `research.md` (entry 1, "Where do the mechanics live?"):

> **1. Where do the mechanics live? Decided: a new
> `developer/skills/ailly/references/shapes/long-loop.md` that `developer/skills/ailly/SKILL.md`
> points to from a short "Long-loop Mode" section.** This matches the verified
> reference-delegation convention (Bugfix Shape → `bugfix.md`, Project Shape →
> `project-cycle.md`), and the contract, recording format, and gate mapping
> together exceed a section's worth and would bloat SKILL.md.

The cleanup phase already extracts deferred decisions from `design.md` into `TASKS.md` (or the configured tracker, when the program-management using reference `developer/skills/ailly/references/abilities/program-management/using.md` is wired), so the in-place block feeds that existing path. A separate decision log would split the audit trail.

## 4. The escalation rule (three concrete triggers)

The reviewer decides the conservative default unless a trigger fires, in which case it leaves the gate uncleared and flags `ESCALATE:`. The triggers:

- (a) **Irreversible.** The decision cannot be undone or cheaply reversed if wrong.
- (b) **Out of recorded scope.** The decision exceeds the scope the artifact itself records. An example is a decision that would change a system the artifact's Scope section excludes.
- (c) **Underdetermined.** The artifact and the repo conventions do not determine a reasonable conservative default.

The reviewer still clears the remaining gates if the escalated item is not a prerequisite for them. If it is, the gate stays uncleared and the run halts there for the human.

## 5. Project-cycle nested-gate interaction

A project loop has nested gates (`developer/skills/ailly/references/shapes/project/project-cycle.md`), and the reviewer contract maps onto both altitudes.

- **Project altitude.** The project-altitude **Review** phase is "the draft-gate period, the project-altitude equivalent of the `*Draft*` marker". The same reviewer contract applies, dispatched scoped to the project design doc at project altitude.
- **Per feature-step.** Each feature-step is itself a feature loop with its own design, plan, and cleanup gates. The reviewer is dispatched fresh per gate for each, cold-scoped to that feature's artifact.

A fresh per-gate dispatch (not one reviewer for the whole project) is the lower-blast-radius default, matching the cold-isolation rationale quick-loop already states.

Because project-cycle's "Sequential and Parallel Steps" defines a parallel step as one that "shares no dependency with its siblings", "prerequisite" is judged per feature-step, not per project: an escalation in one parallel feature does not halt independent siblings. Only steps that depend on the escalated output halt.

At project altitude, reuse the same `Resolved by the long-loop reviewer (YYYY-MM-DD)` heading and entry format from section 3, placed in the project design doc's Summary section (the same slot a feature design's Summary carries, which feeds the same `TASKS.md` cleanup extraction).

## 6. The never-auto-clear invariants

The user controls these handoffs in every mode.

- **Build Handoff.** The coordinator recommends the user's code-review pass and waits for the user to accept or decline it. A detached run returns the handoff to the interactive coordinator. Resume after the user responds.

- **The human merge gate.** Cleanup runs through the harness isolation path and returns its result to the coordinator. After phase review, the coordinator pauses for human approval before a squash merge or PR. At project altitude, follow the approval gate in `project-cycle.md`.
- **The Closing Bell.** A human usability study; "the agent does not pass it on the user's behalf" (`developer/skills/ailly/references/shapes/project/closing-bell.md`). The reviewer never runs or passes it. When the project gets to the closing bell, the long loop is completed.

## 7. The end-of-run report

When the run pauses for the user or otherwise finishes, the coordinator emits a report:

- **What was decided.** Per gate, the recorded-decision entries (or a pointer to each artifact's block).
- **What was done.** Per phase, the artifact produced and its location.
- **Where it stopped.** Name the pending handoff and what the user can review or approve.

The destination follows the loop's retention rule. For a feature loop the report is inline only, because the feature cleanup removes the session folder and a persisted file would earn nothing. For a project loop the coordinator also writes a `report.md` that becomes a supporting sub-page in the long-lived documents (`project-cycle.md`, "Long-Lived Documentation"), persisted alongside the design and the Closing Bell.

## 8. Pi Workflow

A long loop runs autonomously for project-scale durations — too long to hold one interactive tool call open for, and a synchronous call gives the human no way to interject or recover from a transient failure mid-run. Under pi, this is modeled as a detached background process plus a background watcher, not one blocking call:

- `ailly_long_loop_start` (`.pi/extensions/ailly-long-loop/`) spawns the loop as a real second `pi` process — with its own session file colocated in the session folder and this same package's `ailly_subagent`/`review_run` tools available to it — and returns immediately. Its `--mode json` event stream is redirected straight to a journal file on disk.
- `ailly_long_loop_status` reports progress by reading that journal and a small status file back, not by trusting the background run's self-report: process liveness, staleness (no journal growth in the last several minutes), and a scan of the session folder's artifacts for this reference's own `ESCALATE:` contract (section 4).
- `ailly_long_loop_stop` ends it.
- This is also where the "background LLM steering" the loop's autonomy needs comes from: a watcher registered at session start checks every running long loop every few minutes and, when one stalls, escalates, or exits, notifies and injects a follow-up message into the *live* interactive session — so the human does not have to remember to poll status by hand to catch a stuck or escalated run.

See `developer/skills/ailly/references/agents/pi.md` for the full contract.

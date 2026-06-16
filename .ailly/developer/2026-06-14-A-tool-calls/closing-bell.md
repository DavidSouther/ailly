# Closing Bell: Tool Calls

The exit criterion for the tool-calls project. A summative usability study, written now (before the features are designed) and run once near completion. It fixes the definition of done; it is not a continuously-running gate. The agent drafts and scripts it and records the outcome, but does not pass it on the user's behalf. A passing bell is evidence from a human study, not from automated checks.

**Project:** [design.md](design.md)

## Participant Profile

- **Knows:** Ailly's `assemble → run → eval → report` loop; how to author an assembly (`prefix`, `matrix`, `conversation`), a prompt file, and an eval suite; how to read a report. Comfortable editing YAML and running the CLI.
- **Must not know:** how Ailly's tool support is wired internally; any walkthrough of the new `tools` prefix block or the tool-turn protocol beyond what the shipped documentation states.
- One competent participant is sufficient; the study is qualitative, not statistical.

## Setup and Materials

- A checkout at the project's completion state with the release-flag-free build (there is no flag to enable; §4 of the design).
- The shipped tool-call documentation: the amended `DESIGN.md` (the `meta.tools` field and the agentic-loop description) and the `e2e/research/README.md`. This documentation is part of the deliverable and is available to the participant.
- Provided: a scratch project directory and an `ANTHROPIC_API_KEY` for the one secondary (live) task only.
- Withheld: any prior training, an author over the shoulder, or a sample answer assembly. The participant authors from the documentation alone.

## Task Scenarios

Stated as outcomes the user wants, not control sequences.

1. **Declare a tool.** "Give your assembly a tool the model is allowed to call, then assemble it and confirm the generated conversation knows about that tool."
2. **Run a tool conversation.** "Run a conversation where the model calls a tool and gets a result back, without spending money, and see the back-and-forth captured in the conversation file."
3. **Assert on the tool calls.** "Write checks that the model called the search tool, and that it searched before it fetched; run them and read whether they passed."
4. **The research journey works.** "From a clean clone, run the research e2e the way CI does, and see it pass."
5. *(Secondary)* **A live multi-turn run.** "With a real API key, run the insurance-claim assembly end to end and see the tool turns filled by the live model."

## Acceptance Criteria

Predefined; derived from the qualitative outcome above.

| Task | Correct completion | Pass thresholds |
|---|---|---|
| 1 | The assembly carries a `tools` prefix block; the assembled conversation's `meta.tools` lists the tool. | Completed unaided from the docs; ≤ 10 min; 0 blocking errors; ease ≥ 4/5. |
| 2 | A noop-scripted run produces the `assistant tool_use → Role::Tool tool_result → assistant text` shape in the conversation file. | Completed; ≤ 10 min; the participant can point to each turn and say what it is. |
| 3 | `must_call_tool` and `tool_call_order` assertions are authored and run; the report shows the tool-call assertion class with per-assertion results. | Completed; ≤ 15 min; the participant correctly reads which assertions passed. |
| 4 | `e2e/research/ci.sh` (or its documented invocation) runs assemble → run → eval → report and exits green. | Single command; exits 0; no manual fix-ups. |
| 5 *(secondary)* | The live insurance-claim run fills tool turns from the real model and the conversation validates. | Informational; does not block the bell. |

## Critical versus Secondary

- **Critical (must pass to land the project):** tasks 1, 2, 3, 4.
- **Secondary (informs, does not block):** task 5.

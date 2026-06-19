#!/usr/bin/env bash
# CI driver for the research e2e project.
#
# Exercises the operator's four-subcommand journey, with the tool-call
# assertions (must_call_tool, tool_call_order) proven structurally without a
# live API:
#   1. assemble research              -- always runs (reads files only); asserts
#      exactly one conversation skeleton lands under runs/<id>/.
#   2. structural tool-call gate      -- always runs (noop). Copies the
#      pre-filled fixture fixtures/web-research.yaml into a fresh in-tree run
#      dir, runs `ailly run` over it as a verified no-op (the fixture has no
#      blank assistant slot, so run fills nothing; verified by idempotence),
#      then `ailly eval research` and reads the report JSON, asserting
#      passed >= 3 and failed == 0. This proves must_call_tool: web_search,
#      must_call_tool: web_fetch, and tool_call_order: [web_search, web_fetch]
#      fire on the multi-turn shape.
#   3. live run                       -- runs the assembled skeleton through a
#      real model when ANTHROPIC_API_KEY (or a project .env) is present, then
#      eval + report. Skipped with a clear notice otherwise.
#   4. report                         -- over the structural run; asserts the
#      single-run markdown report wrote.
#
# Invoked from the repo root or any working directory; the script resolves its
# own location to find the project root. Kept POSIX/bash-3.2 compatible (no
# associative arrays) so it runs on a stock macOS /bin/bash.

set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${project_dir}/../.." && pwd)"

cd "${repo_root}"

rm -rf "${project_dir}/runs" "${project_dir}/evals/reports"

# --- CUJ 1: assemble ---------------------------------------------------------

cargo run --quiet -- -p "${project_dir}" assemble research

shopt -s nullglob
conversations=("${project_dir}/runs"/*/*.yaml)
shopt -u nullglob

if [[ ${#conversations[@]} -ne 1 ]]; then
  echo "FAIL: ailly assemble produced ${#conversations[@]} conversation file(s) under ${project_dir}/runs/; expected exactly 1." >&2
  exit 1
fi

echo "OK: ailly assemble produced ${#conversations[@]} conversation file:"
echo "  ${conversations[0]#"${repo_root}/"}"

# The single assembled skeleton's run dir, reused by the live half (CUJ 3).
skeleton_run_dir="$(dirname "${conversations[0]}")"

# --- CUJ 2: structural tool-call gate (always, noop) -------------------------

# Copy the pre-filled fixture into a fresh in-tree run dir. The fixture already
# carries the full user -> assistant(tool_use web_search) -> tool(tool_result)
# -> assistant(tool_use web_fetch) -> tool(tool_result) -> assistant(text)
# shape with no blank assistant slot, so `ailly run` fills nothing and `ailly
# eval` scores the authored tool_use blocks directly. The run dir lives inside
# the project tree (runs/ is gitignored) so the eval list key resolves and
# conversations_matched > 0.
structural_id="structural-$(date +%s)-$$"
structural_run_dir="${project_dir}/runs/${structural_id}"
mkdir -p "${structural_run_dir}"
cp "${project_dir}/fixtures/web-research.yaml" "${structural_run_dir}/web-research.yaml"

# `ailly run` always writes the conversation back through serde, so it
# re-serializes the hand-authored fixture into canonical YAML on the first pass
# (flow scalars -> block, etc.) without filling any slot. The no-op guarantee is
# that it *fills nothing* -- verified here as idempotence: the first run
# canonicalizes, and a second run over the canonical form is byte-identical,
# proving the run loop added no content. `eval` below then confirms both
# tool_use blocks survived intact.
cargo run --quiet -- -p "${project_dir}" run "${structural_run_dir}"
first="$(shasum "${structural_run_dir}/web-research.yaml" | awk '{print $1}')"
cargo run --quiet -- -p "${project_dir}" run "${structural_run_dir}"
second="$(shasum "${structural_run_dir}/web-research.yaml" | awk '{print $1}')"

if [[ "${first}" != "${second}" ]]; then
  echo "FAIL: ailly run is not a no-op over the pre-filled fixture; a second run mutated it." >&2
  echo "  first=${first} second=${second}" >&2
  exit 1
fi

echo "OK: ailly run over the pre-filled fixture is a no-op (idempotent; fills no blank)."

cargo run --quiet -- -p "${project_dir}" eval research --over "${structural_run_dir}"

structural_report="${project_dir}/evals/reports/${structural_id}.json"
if [[ ! -f "${structural_report}" ]]; then
  echo "FAIL: ailly eval did not write a report at ${structural_report#"${repo_root}/"}" >&2
  exit 1
fi

# Read the totals from the report JSON: the three tool-call assertions
# (must_call_tool x2 + tool_call_order) must all pass with none failing.
python3 - "${structural_report}" <<'PY'
import json, sys

with open(sys.argv[1], encoding="utf-8") as fh:
    data = json.load(fh)

matched = data["totals"]["conversations_matched"]
t = data["totals"]["assertions"]
passed, failed = t["passed"], t["failed"]

if matched < 1:
    sys.exit(
        f"FAIL: eval matched {matched} conversation(s); expected the in-tree "
        "structural fixture to match."
    )
if passed < 3 or failed != 0:
    sys.exit(
        f"FAIL: structural tool-call gate scored passed={passed} failed={failed}; "
        "expected passed >= 3 and failed == 0 "
        "(must_call_tool: web_search, must_call_tool: web_fetch, "
        "tool_call_order: [web_search, web_fetch])."
    )

print(
    f"OK: structural tool-call gate passed={passed} failed={failed} "
    f"(matched {matched} conversation) -- must_call_tool + tool_call_order fire "
    "on the multi-turn shape with no live API."
)
PY

# --- CUJ 3: live run (gated on credentials) ----------------------------------

if [[ -z "${ANTHROPIC_API_KEY:-}" && ! -f "${project_dir}/.env" ]]; then
  echo "SKIP: ailly run (live) requires ANTHROPIC_API_KEY in the shell or ${project_dir#"${repo_root}/"}/.env; assemble + structural halves passed."
else
  cargo run --quiet -- -p "${project_dir}" run "${skeleton_run_dir}"
  cargo run --quiet -- -p "${project_dir}" eval research --over "${skeleton_run_dir}"
  cargo run --quiet -- -p "${project_dir}" report "$(basename "${skeleton_run_dir}")"
  echo "OK: ailly run/eval/report (live) completed for ${skeleton_run_dir##*/}."
fi

# --- CUJ 4: report (always, over the structural run) -------------------------

cargo run --quiet -- -p "${project_dir}" report "${structural_id}"

structural_report_md="${project_dir}/evals/reports/${structural_id}-report.md"
if [[ ! -f "${structural_report_md}" ]]; then
  echo "FAIL: ailly report did not write ${structural_report_md#"${repo_root}/"}" >&2
  exit 1
fi

echo "OK: ailly report wrote ${structural_report_md#"${repo_root}/"}"

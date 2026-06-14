#!/usr/bin/env bash
# CI driver for the insurance-claim e2e project.
#
# Exercises the operator's journey, with the tool-call assertions
# (must_call_tool, tool_call_order, must_not_call_tool) proven
# structurally without a live API:
#   1. `ailly assemble claim-handler` -- always runs; asserts N
#      conversation files land under runs/<id>/.
#   2. structural tool-call gate       -- always runs (noop). Copies
#      the pre-filled fixtures fixtures/{missing-fields,over-limit}.yaml
#      into a fresh in-tree run dir, runs `ailly run` over them as a
#      verified no-op (the fixtures have no blank assistant slot, so run
#      fills nothing; verified by idempotence), then `ailly eval
#      regression` and reads the report JSON, asserting the tool-call
#      classes (must_call_tool, tool_call_order, must_not_call_tool)
#      pass with zero failures. This proves must_call_tool:
#      lookup_policy, tool_call_order: [lookup_policy,
#      lookup_claim_history], and must_not_call_tool: auto_approve fire
#      on the multi-turn shape. (The over-limit `judge` assertion is
#      malformed, not deferred, here -- the CLI eval path always wires a
#      noop engine that returns no GRADE line; the gate scores the
#      tool-call classes directly rather than the suite-wide exit code,
#      so the expected non-tool malformed entries do not fail the gate.)
#   3. `ailly run runs/<id>/`          -- runs when ANTHROPIC_API_KEY is
#      present; asserts every conversation file's trailing blank
#      assistant slot has been filled, then eval + report. Skipped with
#      a clear notice otherwise so contributors without API access still
#      see the assemble + structural halves pass.
#
# Invoked from the repo root or any working directory; the script
# resolves its own location to find the project root. Kept POSIX/bash-3.2
# compatible (no associative arrays) so it runs on a stock macOS /bin/bash.

set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${project_dir}/../.." && pwd)"

cd "${repo_root}"

rm -rf "${project_dir}/runs" "${project_dir}/evals/reports" "${project_dir}/evals/judges"

# --- CUJ 1: assemble ---------------------------------------------------------

cargo run --quiet -- -p "${project_dir}" assemble claim-handler

shopt -s nullglob
conversations=("${project_dir}/runs"/*/*.yaml)
shopt -u nullglob

if [[ ${#conversations[@]} -eq 0 ]]; then
  echo "FAIL: ailly assemble produced no conversation files under ${project_dir}/runs/" >&2
  exit 1
fi

echo "OK: ailly assemble produced ${#conversations[@]} conversation file(s):"
for f in "${conversations[@]}"; do
  echo "  ${f#"${repo_root}/"}"
done

# --- CUJ 2: structural tool-call gate (always, noop) -------------------------

# Copy the pre-filled per-case fixtures into a fresh in-tree run dir. Each
# fixture already carries the full user -> assistant(tool_use ...) ->
# tool(tool_result) -> assistant(text) shape with no blank assistant slot, so
# `ailly run` fills nothing and `ailly eval` scores the authored tool_use
# blocks directly. missing-fields emits lookup_policy; over-limit emits
# lookup_policy then lookup_claim_history in order. The run dir lives inside the
# project tree (runs/ is gitignored) so the eval list key resolves and
# conversations_matched > 0. The ambiguous case calls no tool and is covered by
# the existing eval; no tool fixture is needed for it, and no fixture emits
# auto_approve so the must_not_call_tool assertions hold.
structural_id="structural-$(date +%s)-$$"
structural_run_dir="${project_dir}/runs/${structural_id}"
mkdir -p "${structural_run_dir}"
cp "${project_dir}/fixtures/missing-fields.yaml" "${structural_run_dir}/missing-fields.yaml"
cp "${project_dir}/fixtures/over-limit.yaml" "${structural_run_dir}/over-limit.yaml"

# `ailly run` always writes the conversation back through serde, so it
# re-serializes the hand-authored fixtures into canonical YAML on the first
# pass (flow scalars -> block, etc.) without filling any slot. The no-op
# guarantee is that it *fills nothing* -- verified here as idempotence: the
# first run canonicalizes, and a second run over the canonical form is
# byte-identical, proving the run loop added no content. `eval` below then
# confirms the tool_use blocks survived intact.
cargo run --quiet -- -p "${project_dir}" run "${structural_run_dir}"
first="$(shasum "${structural_run_dir}"/*.yaml | awk '{print $1}' | sort | tr '\n' ' ')"
cargo run --quiet -- -p "${project_dir}" run "${structural_run_dir}"
second="$(shasum "${structural_run_dir}"/*.yaml | awk '{print $1}' | sort | tr '\n' ' ')"

if [[ "${first}" != "${second}" ]]; then
  echo "FAIL: ailly run is not a no-op over the pre-filled fixtures; a second run mutated them." >&2
  echo "  first=${first} second=${second}" >&2
  exit 1
fi

echo "OK: ailly run over the pre-filled fixtures is a no-op (idempotent; fills no blank)."

# `ailly eval` exits non-zero here because the over-limit `judge` assertion is
# malformed under the CLI's always-wired noop engine (it returns no GRADE
# line), and the ambiguous case has no fixture in this structural run. Neither
# is a tool-call regression, so we read pass/fail from the report JSON rather
# than the suite-wide exit code: the gate asserts the tool-call classes pass
# and that nothing *failed* (a real assertion miss), tolerating the expected
# non-tool malformed entries.
cargo run --quiet -- -p "${project_dir}" eval regression --over "${structural_run_dir}" || true

structural_report="${project_dir}/evals/reports/${structural_id}.json"
if [[ ! -f "${structural_report}" ]]; then
  echo "FAIL: ailly eval did not write a report at ${structural_report#"${repo_root}/"}" >&2
  exit 1
fi

# Read the per-class totals from the report JSON: the three tool-call classes
# (must_call_tool, must_not_call_tool, tool_call_order) must each pass with none
# failing, and the suite-wide `failed` count must be zero (a non-zero `failed`
# would be a genuine assertion miss, distinct from the tolerated malformed).
python3 - "${structural_report}" <<'PY'
import json, sys

with open(sys.argv[1], encoding="utf-8") as fh:
    data = json.load(fh)

matched = data["totals"]["conversations_matched"]
failed = data["totals"]["assertions"]["failed"]
errored = data["totals"]["assertions"]["errored"]
per_class = data["per_class"]


def passed_of(name):
    cls = per_class.get(name, {})
    return cls.get("passed", 0), cls.get("failed", 0)


if matched < 1:
    sys.exit(
        f"FAIL: eval matched {matched} conversation(s); expected the in-tree "
        "structural fixtures to match."
    )
if failed != 0 or errored != 0:
    sys.exit(
        f"FAIL: structural gate saw failed={failed} errored={errored}; expected "
        "zero of each (the over-limit judge is malformed, not failed, and is "
        "tolerated)."
    )

problems = []
for cls, (need_passed, want_failed) in {
    "must_call_tool": (1, 0),       # lookup_policy on missing-fields
    "must_not_call_tool": (1, 0),   # auto_approve on over-limit
    "tool_call_order": (1, 0),      # [lookup_policy, lookup_claim_history]
}.items():
    p, f = passed_of(cls)
    if p < need_passed or f != want_failed:
        problems.append(f"{cls}: passed={p} failed={f} (expected passed>={need_passed}, failed={want_failed})")

if problems:
    sys.exit("FAIL: tool-call classes did not all pass:\n  " + "\n  ".join(problems))

print(
    f"OK: structural tool-call gate -- must_call_tool, must_not_call_tool, and "
    f"tool_call_order all pass (matched {matched} conversations, failed={failed}); "
    "the now-live tool behavior fires on the multi-turn shape with no live API."
)
PY

# --- CUJ 3: run (gated on credentials) ---------------------------------------

# All assembled conversations sit one directory under runs/, so derive the
# single run directory from the first conversation path.
run_dir="$(dirname "${conversations[0]}")"

if [[ -z "${ANTHROPIC_API_KEY:-}" && ! -f "${project_dir}/.env" ]]; then
  echo "SKIP: ailly run requires ANTHROPIC_API_KEY in the shell or ${project_dir#"${repo_root}/"}/.env; assemble + structural halves passed."
  exit 0
fi

cargo run --quiet -- -p "${project_dir}" run "${run_dir}"

# Assert: no conversation file still ends in a blank assistant slot.
# A blank assistant is a YAML document whose only mapping entry is
# `role: assistant` with no `body:`/`content:` line following before the
# next `---` separator or EOF.
unfilled=()
for f in "${conversations[@]}"; do
  if awk '
    BEGIN { in_doc = 0; role = ""; has_body = 0 }
    /^---[[:space:]]*$/ {
      if (in_doc && role == "assistant" && has_body == 0) { print FILENAME; exit }
      in_doc = 1; role = ""; has_body = 0; next
    }
    /^role:[[:space:]]*assistant[[:space:]]*$/ { role = "assistant"; next }
    /^(body|content):/ { has_body = 1; next }
    END {
      if (in_doc && role == "assistant" && has_body == 0) { print FILENAME }
    }
  ' "${f}" | grep -q .; then
    unfilled+=("${f}")
  fi
done

if [[ ${#unfilled[@]} -gt 0 ]]; then
  echo "FAIL: ailly run left ${#unfilled[@]} conversation(s) with a blank assistant:" >&2
  for f in "${unfilled[@]}"; do
    echo "  ${f#"${repo_root}/"}" >&2
  done
  exit 1
fi

echo "OK: ailly run filled the assistant slot in all ${#conversations[@]} conversation file(s)."

# --- CUJ 3 (cont.): eval -----------------------------------------------------

cargo run --quiet -- -p "${project_dir}" eval regression --over "${run_dir}"

echo "OK: ailly eval regression passed for run ${run_dir##*/}."

# --- CUJ 3 (cont.): report ---------------------------------------------------

cargo run --quiet -- -p "${project_dir}" report

summary_json="${project_dir}/evals/reports/summary.json"
summary_md="${project_dir}/evals/reports/summary.md"

if [[ ! -f "${summary_json}" ]]; then
  echo "FAIL: ailly report did not write ${summary_json#"${repo_root}/"}" >&2
  exit 1
fi
if [[ ! -f "${summary_md}" ]]; then
  echo "FAIL: ailly report did not write ${summary_md#"${repo_root}/"}" >&2
  exit 1
fi

echo "OK: ailly report wrote ${summary_json#"${repo_root}/"} and ${summary_md#"${repo_root}/"}"

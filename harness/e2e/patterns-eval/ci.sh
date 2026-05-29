#!/usr/bin/env bash
# CI driver for the patterns-eval e2e project.
#
# Exercises both halves of the operator's journey across two suites
# (discovery and invocation):
#   1. `ailly assemble <suite>` -- always runs; asserts N conversation
#      files land under runs/<id>/ for each suite.
#   2. `ailly run runs/<id>/`   -- runs when ANTHROPIC_API_KEY is
#      present; asserts every conversation file's trailing blank
#      assistant slot has been filled. Skipped with a clear notice
#      otherwise so contributors without API access still see the
#      assemble half pass.
#   3. `ailly eval <suite> --over runs/<id>/` -- runs after `ailly run`;
#      asserts the per-run report file landed at
#      evals/reports/<run-id>.json and prints a deferred-tolerance
#      summary line per suite.
#
# Invoked from the repo root or any working directory; the script
# resolves its own location to find the project root.

set -euo pipefail
set -x

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${project_dir}/../.." && pwd)"

cd "${repo_root}"

rm -rf "${project_dir}/runs" "${project_dir}/evals/reports"

expected_count() {
  case "$1" in
    discovery)  echo 6 ;;
    invocation) echo 3 ;;
    baseline) echo 3 ;;
    *) echo "FAIL: unknown suite $1" >&2; exit 1 ;;
  esac
}

# Globals populated by assemble_suite; reused by run_suite/eval_suite.
discovery_run_dir=""
invocation_run_dir=""
baseline_run_dir=""

set_run_dir() {
  case "$1" in
    discovery)  discovery_run_dir="$2" ;;
    invocation) invocation_run_dir="$2" ;;
    baseline) baseline_run_dir="$2" ;;
  esac
}

get_run_dir() {
  case "$1" in
    discovery)  printf '%s\n' "${discovery_run_dir}" ;;
    invocation) printf '%s\n' "${invocation_run_dir}" ;;
    baseline) printf '%s\n' "${baseline_run_dir}" ;;
  esac
}

assemble_suite() {
  local suite="$1"
  cargo run --quiet -- -p "${project_dir}" assemble "${suite}"

  shopt -s nullglob
  local files=("${project_dir}/runs"/*-"${suite}"/*.yaml)
  shopt -u nullglob

  if [[ ${#files[@]} -eq 0 ]]; then
    echo "FAIL: ailly assemble ${suite} produced no conversation files under ${project_dir}/runs/" >&2
    exit 1
  fi

  local expected
  expected="$(expected_count "${suite}")"
  if [[ ${#files[@]} -ne ${expected} ]]; then
    echo "FAIL: ailly assemble ${suite} produced ${#files[@]} conversation file(s); expected ${expected}." >&2
    exit 1
  fi

  echo "OK: ailly assemble ${suite} produced ${#files[@]} conversation file(s):"
  local f
  for f in "${files[@]}"; do
    echo "  ${f#"${repo_root}/"}"
  done

  set_run_dir "${suite}" "$(dirname "${files[0]}")"
}

# --- CUJ 1: assemble (both suites) ------------------------------------------

assemble_suite discovery
assemble_suite baseline
assemble_suite invocation

# --- CUJ 2: run (both suites, gated on credentials) -------------------------

if [[ -z "${ANTHROPIC_API_KEY:-}" && ! -f "${project_dir}/.env" ]]; then
  echo "SKIP: ailly run requires ANTHROPIC_API_KEY in the shell or ${project_dir#"${repo_root}/"}/.env; assemble half passed."
  exit 0
fi

# Asserts that every assembled conversation file under the suite's
# run_dir has a filled assistant turn.
assert_filled() {
  local suite="$1"
  local run_dir
  run_dir="$(get_run_dir "${suite}")"

  shopt -s nullglob
  local files=("${run_dir}"/*.yaml)
  shopt -u nullglob

  local unfilled=()
  local f
  for f in "${files[@]}"; do
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
    echo "FAIL: ailly run ${suite} left ${#unfilled[@]} conversation(s) with a blank assistant:" >&2
    local u
    for u in "${unfilled[@]}"; do
      echo "  ${u#"${repo_root}/"}" >&2
    done
    exit 1
  fi

  echo "OK: ailly run ${suite} filled the assistant slot in all ${#files[@]} conversation file(s)."
}

run_suite() {
  local suite="$1"
  local run_dir
  run_dir="$(get_run_dir "${suite}")"
  cargo run --quiet -- -p "${project_dir}" run "${run_dir}"
  assert_filled "${suite}"
}

run_suite discovery
run_suite baseline 
run_suite invocation

# --- CUJ 3: eval (both suites) ----------------------------------------------

eval_suite() {
  local suite="$1"
  local run_dir
  run_dir="$(get_run_dir "${suite}")"
  local run_id
  run_id="$(basename "${run_dir}")"
  local report="${project_dir}/evals/reports/${run_id}.json"

  # Allow assertion failures without aborting; the report step aggregates results.
  cargo run --quiet -- -p "${project_dir}" eval "${suite}" --over "${run_dir}" || true

  if [[ ! -f "${report}" ]]; then
    echo "FAIL: ailly eval ${suite} did not write a report at ${report#"${repo_root}/"}" >&2
    exit 1
  fi

  # Deferred-tolerance summary. `deferred` is informational; assertion failures
  # are surfaced in the comparison report written by CUJ 4.
  python3 - "${suite}" "${report}" <<'PY'
import json
import sys

suite, report_path = sys.argv[1], sys.argv[2]
with open(report_path, encoding="utf-8") as fh:
    data = json.load(fh)
totals = data["totals"]["assertions"]
print(
    f"eval {suite}: "
    f"passed={totals['passed']} "
    f"failed={totals['failed']} "
    f"deferred={totals['deferred']} "
    f"malformed={totals['malformed']} "
    f"errored={totals.get('errored', 0)}"
)
PY

  echo "OK: ailly eval ${suite} wrote ${report#"${repo_root}/"}"
}

eval_suite discovery
eval_suite baseline
eval_suite invocation

# --- CUJ 4: report ----------------------------------------------------------

# Single-eval markdown summary for the discovery suite.
report_discovery() {
  local run_id
  run_id="$(basename "${discovery_run_dir}")"
  cargo run --quiet -- -p "${project_dir}" report "${run_id}"

  local report_md="${project_dir}/evals/reports/${run_id}-report.md"
  if [[ ! -f "${report_md}" ]]; then
    echo "FAIL: ailly report ${run_id} did not write ${report_md#"${repo_root}/"}" >&2
    exit 1
  fi
  echo "OK: ailly report wrote ${report_md#"${repo_root}/"}"
}

# Comparison report: baseline (arm-a) vs invocation (arm-b).
report_comparison() {
  local run_id_a run_id_b
  run_id_a="$(basename "${baseline_run_dir}")"
  run_id_b="$(basename "${invocation_run_dir}")"
  cargo run --quiet -- -p "${project_dir}" report "${run_id_a}" "${run_id_b}"

  local stem="${run_id_a}-vs-${run_id_b}"
  local comparison_json="${project_dir}/evals/reports/${stem}.json"
  local comparison_md="${project_dir}/evals/reports/${stem}.md"

  if [[ ! -f "${comparison_json}" ]]; then
    echo "FAIL: ailly report comparison JSON not written at ${comparison_json#"${repo_root}/"}" >&2
    exit 1
  fi
  if [[ ! -f "${comparison_md}" ]]; then
    echo "FAIL: ailly report comparison markdown not written at ${comparison_md#"${repo_root}/"}" >&2
    exit 1
  fi

  python3 - "${run_id_a}" "${run_id_b}" "${comparison_json}" <<'PY'
import json, sys
id_a, id_b, path = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, encoding="utf-8") as fh:
    data = json.load(fh)
t = data["totals"]
print(
    f"report {id_a} vs {id_b}: "
    f"improved={t['improved']} "
    f"regressed={t['regressed']} "
    f"unchanged_pass={t['unchanged_pass']} "
    f"unchanged_fail={t['unchanged_fail']}"
)
PY

  echo "OK: ailly report wrote ${comparison_json#"${repo_root}/"} and ${comparison_md#"${repo_root}/"}"
}

report_discovery
report_comparison

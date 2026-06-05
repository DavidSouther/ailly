#!/usr/bin/env bash
# CI driver for the delegate-52 e2e project.
#
# Exercises the operator's journey for the single `corruption` suite:
#   1. `ailly assemble delegated-workflow` -- always runs, OFFLINE, over the
#      full provider × domain × distractor_count matrix; asserts all 12
#      conversation files land under runs/<id>/.
#   2. `ailly run runs/<id>/` -- requires a live model. The full matrix names
#      three providers (anthropic/openai/google); only ANTHROPIC_API_KEY is
#      provisioned here, so the live half assembles the `delegated-workflow-live`
#      assembly — anthropic provider only, the cheap domain pair — and runs only
#      that, never calling an un-keyed provider. `assemble` has no `--var` axis
#      flag, so the narrowing is a separate committed assembly, mirroring how
#      patterns-eval scopes its suites into separate assembly files. With neither
#      ANTHROPIC_API_KEY nor a project .env the script hard-fails: there is no
#      assemble-only success path, matching patterns-eval and insurance-claim.
#   3. `ailly eval corruption --over runs/<id>/` -- runs after `ailly run`;
#      asserts the per-run report landed at evals/reports/<run-id>.json and
#      prints a deferred-tolerance summary line.
#   4. `ailly report <run-id>` -- single-run markdown summary. This fixture has
#      no baseline A/B arm, so only the single-run `report <id>` form is driven
#      (unlike patterns-eval's `report <a> <b>` comparison).
#
# Invoked from the repo root or any working directory; the script resolves its
# own location to find the project root.

set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${project_dir}/../.." && pwd)"

cd "${repo_root}"

rm -rf "${project_dir}/runs" "${project_dir}/evals/reports"

# --- CUJ 1: assemble (offline, full matrix) ---------------------------------

# The full matrix is exercised offline: 3 providers × 4 domains × 1 distractor
# count = 12 conversation files. This needs no credentials.
cargo run --quiet -- -p "${project_dir}" assemble delegated-workflow

shopt -s nullglob
conversations=("${project_dir}/runs"/*/*.yaml)
shopt -u nullglob

expected=12
if [[ ${#conversations[@]} -ne ${expected} ]]; then
  echo "FAIL: ailly assemble produced ${#conversations[@]} conversation file(s) under ${project_dir}/runs/; expected ${expected}." >&2
  exit 1
fi

echo "OK: ailly assemble produced ${#conversations[@]} conversation file(s):"
for f in "${conversations[@]}"; do
  echo "  ${f#"${repo_root}/"}"
done

# --- Credential gate --------------------------------------------------------

# The live half always runs once credentials exist; there is no assemble-only
# success path. The full cross-provider run would need all three providers'
# keys; here only ANTHROPIC_API_KEY is provisioned, so the live half below
# narrows the matrix to the anthropic provider.
if [[ -z "${ANTHROPIC_API_KEY:-}" && ! -f "${project_dir}/.env" ]]; then
  echo "FAIL: ailly run requires a live model. Set ANTHROPIC_API_KEY in the shell or drop a ${project_dir#"${repo_root}/"}/.env file." >&2
  echo "      The live half exercises the model and the eval; there is no assemble-only success path." >&2
  echo "      Note: the full cross-provider matrix needs anthropic, openai, and google keys; this gate only checks for anthropic." >&2
  exit 1
fi

# --- CUJ 2: run (gated; narrowed to the keyed provider) ---------------------

# Assemble the CI-scoped variant: the anthropic provider only, the cheap domain
# pair (prose-bio, code-sql), so the live run never calls the un-keyed
# OpenAI/Google endpoints. The offline assemble above already proved the full
# 12-file matrix. This narrowed assembly yields 1 provider × 2 domains = 2 files.
rm -rf "${project_dir}/runs"
cargo run --quiet -- -p "${project_dir}" assemble delegated-workflow-live

shopt -s nullglob
live_conversations=("${project_dir}/runs"/*/*.yaml)
shopt -u nullglob

live_expected=2
if [[ ${#live_conversations[@]} -ne ${live_expected} ]]; then
  echo "FAIL: live assemble produced ${#live_conversations[@]} conversation file(s) under ${project_dir}/runs/; expected ${live_expected} (anthropic × {prose-bio, code-sql})." >&2
  exit 1
fi

run_dir="$(dirname "${live_conversations[0]}")"

cargo run --quiet -- -p "${project_dir}" run "${run_dir}"

# Assert: no conversation file still ends in a blank assistant slot. Each file
# carries six assistant turns; the awk walk flags any assistant document that
# reaches the next `---` (or EOF) without a body/content line.
unfilled=()
for f in "${live_conversations[@]}"; do
  if awk '
    BEGIN { in_doc = 0; role = ""; has_body = 0 }
    /^---[[:space:]]*$/ {
      if (in_doc && role == "assistant" && has_body == 0) { print FILENAME; exit }
      in_doc = 1; role = ""; has_body = 0; next
    }
    /^role:[[:space:]]*assistant[[:space:]]*$/ { role = "assistant"; next }
    /^role:[[:space:]]*/ { role = "other"; next }
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

echo "OK: ailly run filled every assistant slot in all ${#live_conversations[@]} conversation file(s)."

# --- CUJ 3: eval ------------------------------------------------------------

run_id="$(basename "${run_dir}")"
report="${project_dir}/evals/reports/${run_id}.json"

# Allow assertion failures without aborting; the report step aggregates results.
cargo run --quiet -- -p "${project_dir}" eval corruption --over "${run_dir}" || true

if [[ ! -f "${report}" ]]; then
  echo "FAIL: ailly eval corruption did not write a report at ${report#"${repo_root}/"}" >&2
  exit 1
fi

# Deferred-tolerance summary. `deferred` is informational (the cross-provider
# rollup judge defers per conversation unless a judge engine is wired);
# assertion failures are what gate.
python3 - "${report}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    data = json.load(fh)
totals = data["totals"]["assertions"]
print(
    "eval corruption: "
    f"passed={totals['passed']} "
    f"failed={totals['failed']} "
    f"deferred={totals['deferred']} "
    f"malformed={totals['malformed']} "
    f"errored={totals.get('errored', 0)}"
)
PY

echo "OK: ailly eval corruption wrote ${report#"${repo_root}/"}"

# --- CUJ 4: report (single-run summary; no comparison arm) ------------------

cargo run --quiet -- -p "${project_dir}" report "${run_id}"

report_md="${project_dir}/evals/reports/${run_id}-report.md"
if [[ ! -f "${report_md}" ]]; then
  echo "FAIL: ailly report ${run_id} did not write ${report_md#"${repo_root}/"}" >&2
  exit 1
fi

echo "OK: ailly report wrote ${report_md#"${repo_root}/"}"

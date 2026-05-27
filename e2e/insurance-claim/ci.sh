#!/usr/bin/env bash
# CI driver for the insurance-claim e2e project.
#
# Exercises both halves of the operator's journey:
#   1. `ailly assemble claim-handler` -- always runs; asserts N
#      conversation files land under runs/<id>/.
#   2. `ailly run runs/<id>/`         -- runs when ANTHROPIC_API_KEY is
#      present; asserts every conversation file's trailing blank
#      assistant slot has been filled. Skipped with a clear notice
#      otherwise so contributors without API access still see the
#      assemble half pass.
#
# Invoked from the repo root or any working directory; the script
# resolves its own location to find the project root.

set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${project_dir}/../.." && pwd)"

cd "${repo_root}"

rm -rf "${project_dir}/runs"

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

# --- CUJ 2: run --------------------------------------------------------------

# All assembled conversations sit one directory under runs/, so derive the
# single run directory from the first conversation path.
run_dir="$(dirname "${conversations[0]}")"

if [[ -z "${ANTHROPIC_API_KEY:-}" && ! -f "${project_dir}/.env" ]]; then
  echo "SKIP: ailly run requires ANTHROPIC_API_KEY in the shell or ${project_dir#"${repo_root}/"}/.env; assemble half passed."
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

# --- CUJ 3: eval ------------------------------------------------------------

cargo run --quiet -- -p "${project_dir}" eval regression --over "${run_dir}"

echo "OK: ailly eval regression passed for run ${run_dir##*/}."

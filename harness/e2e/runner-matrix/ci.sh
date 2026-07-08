#!/usr/bin/env bash
# CI driver for the runner-matrix e2e project.
#
# Always assembles the complete issue #29 runner matrix offline. Live runs are
# provider-scoped and credential-gated, so a default CI environment with no
# secrets skips live calls without hiding rows from the committed matrix.

set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${project_dir}/../.." && pwd)"

cd "${repo_root}"

rm -rf "${project_dir}/runs" "${project_dir}/evals/reports" "${project_dir}/evals/judges"

run_dir="$(cargo run --quiet -- -p "${project_dir}" assemble live-confirmation)"

shopt -s nullglob
conversations=("${run_dir}"/*.yaml)
shopt -u nullglob

expected=16
if [[ ${#conversations[@]} -ne ${expected} ]]; then
  echo "FAIL: ailly assemble produced ${#conversations[@]} conversation file(s) under ${run_dir}; expected ${expected}." >&2
  exit 1
fi

echo "OK: ailly assemble produced ${#conversations[@]} runner conversation file(s)."

has_dotenv_value() {
  local name="$1"
  local env_file="${project_dir}/.env"
  [[ -f "${env_file}" ]] && grep -Eq "^[[:space:]]*(export[[:space:]]+)?${name}=" "${env_file}"
}

has_config_value() {
  local name="$1"
  [[ -n "${!name:-}" ]] || has_dotenv_value "${name}"
}

ran_live=0

run_provider() {
  local label="$1"
  shift
  local -a cases=("$@")
  local -a run_args=(cargo run --quiet -- -p "${project_dir}" run "${run_dir}")
  local -a eval_args=(cargo run --quiet -- -p "${project_dir}" eval live-confirmation --over "${run_dir}")

  for case_name in "${cases[@]}"; do
    run_args+=(--case "${case_name}")
    eval_args+=(--case "${case_name}")
  done

  echo "RUN: ${label} (${#cases[@]} runner row(s))"
  "${run_args[@]}"
  "${eval_args[@]}"
  ran_live=1
}

if has_config_value ANTHROPIC_API_KEY; then
  run_provider anthropic \
    anthropic-haiku-4-5 \
    anthropic-sonnet-4-6 \
    anthropic-sonnet-5 \
    anthropic-opus-4-8 \
    anthropic-fable
else
  echo "SKIP: anthropic rows need ANTHROPIC_API_KEY."
fi

if has_config_value OPENAI_API_KEY; then
  run_provider openai \
    openai-gpt-5-5 \
    openai-gpt-5-4 \
    openai-gpt-5-4-mini
else
  echo "SKIP: openai rows need OPENAI_API_KEY."
fi

if has_config_value GEMINI_API_KEY; then
  run_provider google \
    google-gemini-3-5-flash \
    google-gemini-3-1-pro-preview \
    google-gemini-3-1-flash-lite \
    google-gemini-2-5-pro
else
  echo "SKIP: google rows need GEMINI_API_KEY."
fi

if (has_config_value AWS_REGION || has_config_value AWS_DEFAULT_REGION) \
  && (has_config_value AWS_BEARER_TOKEN_BEDROCK \
    || has_config_value AWS_PROFILE \
    || (has_config_value AWS_ACCESS_KEY_ID && has_config_value AWS_SECRET_ACCESS_KEY)); then
  run_provider bedrock \
    bedrock-llama-3-3 \
    bedrock-llama-4-scout \
    bedrock-mistral-large-3 \
    bedrock-cohere-r-plus
else
  echo "SKIP: bedrock rows need AWS_REGION or AWS_DEFAULT_REGION plus a Bedrock bearer token, AWS profile, or AWS access keys."
fi

if [[ ${ran_live} -eq 0 ]]; then
  echo "OK: no provider credentials detected; live runner rows skipped after offline assemble."
else
  echo "OK: completed credentialed live runner rows."
fi

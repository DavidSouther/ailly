#!/usr/bin/env bash
# CI driver for the insurance-claim e2e project.
#
# Today it exercises only the `assemble` slice. As `run` and `eval`
# land, append their invocations and assertions below.
#
# Invoked from the repo root or any working directory; the script
# resolves its own location to find the project root.

set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${project_dir}/../.." && pwd)"

cd "${repo_root}"

rm -rf "${project_dir}/runs"

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

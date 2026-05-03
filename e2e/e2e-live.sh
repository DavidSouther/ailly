#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ./_lib.sh

if [ "$#" -ne 1 ]; then
    echo "usage: $(basename "$0") <anthropic|openai|gemini>" >&2
    exit 2
fi

engine="$1"
case "$engine" in
    anthropic) key_var="ANTHROPIC_API_KEY" ;;
    openai)    key_var="OPENAI_API_KEY" ;;
    gemini)    key_var="GEMINI_API_KEY" ;;
    *)
        echo "unknown engine: $engine (expected anthropic, openai, or gemini)" >&2
        exit 2
        ;;
esac

if [ -z "${!key_var:-}" ]; then
    echo "SKIP: $engine e2e requires $key_var" >&2
    exit 0
fi

ensure_built
export AILLY_E2E_BUILT=1
export AILLY_ENGINE="$engine"
export AILLY_E2E_LIVE=1

for script in [0-9]*/*.sh; do
    echo "==> $script"
    "./$script"
done

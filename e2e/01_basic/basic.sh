#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ../_lib.sh
ensure_built

cleanup() {
    git restore 01_basic.toml >/dev/null 2>&1 || true
}
trap cleanup EXIT

git restore 01_basic.toml >/dev/null 2>&1 || true

"$AILLY_BIN" --root .

if [ "${AILLY_E2E_LIVE:-0}" = "1" ]; then
    exit 0
fi

assert_grep_q 'role = "assistant"' 01_basic.toml
assert_grep_q 'noop response for' 01_basic.toml

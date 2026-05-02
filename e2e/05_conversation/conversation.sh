#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ../_lib.sh
ensure_built

fixtures=(.ailly.toml 01_a.toml 02_b.toml)

cleanup() {
    rm -f out err
    git restore "${fixtures[@]}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

git restore "${fixtures[@]}" >/dev/null 2>&1 || true

"$AILLY_BIN" --root . --log-format pretty \
    --prompt "This is a conversation with system and two files." \
    > out 2> err

if [ "${AILLY_E2E_LIVE:-0}" = "1" ]; then
    exit 0
fi

assert_grep_q 'You are running an integration test.' out
assert_grep_q 'user: File a.' out
assert_grep_q 'user: File b.' out
assert_grep_q 'This is a conversation with system and two files.' out
if [ -s err ]; then
    echo "FAIL: expected empty stderr at default log level, got:" >&2
    cat err >&2
    exit 1
fi

git restore "${fixtures[@]}" >/dev/null 2>&1 || true

"$AILLY_BIN" --root . --verbose --log-format pretty \
    --prompt "This is a conversation with system and two files." \
    > out 2> err

assert_grep_q 'ailly starting' err
assert_grep_q 'turn started' err
assert_grep_q 'turn finished' err

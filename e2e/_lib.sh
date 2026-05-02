#!/usr/bin/env bash
# Shared bash helpers for the e2e suite. Sourced by e2e.sh, e2e-live.sh,
# and every per-test script.

shopt -s nullglob

AILLY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export AILLY_ROOT
export AILLY_BIN="${AILLY_BIN:-$AILLY_ROOT/target/debug/ailly}"
export NO_COLOR=1

if [ -f "$AILLY_ROOT/.env" ]; then
    set -a
    # shellcheck disable=SC1091
    . "$AILLY_ROOT/.env"
    set +a
fi

ensure_built() {
    if [ "${AILLY_E2E_BUILT:-0}" = "1" ]; then
        return 0
    fi
    (cd "$AILLY_ROOT" && cargo build --quiet)
}

assert_grep_q() {
    local pattern="$1"
    local file="$2"
    if ! grep -q -- "$pattern" "$file"; then
        echo "FAIL: pattern not found in $file: $pattern" >&2
        exit 1
    fi
}

assert_no_grep() {
    local pattern="$1"
    local file="$2"
    if grep -q -- "$pattern" "$file"; then
        echo "FAIL: pattern unexpectedly found in $file: $pattern" >&2
        exit 1
    fi
}

assert_file_exists() {
    local path="$1"
    if [ ! -e "$path" ]; then
        echo "FAIL: file does not exist: $path" >&2
        exit 1
    fi
}

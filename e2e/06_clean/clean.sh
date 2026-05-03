#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ../_lib.sh
ensure_built

cleanup() {
    git restore 01_clean.toml >/dev/null 2>&1 || true
}
trap cleanup EXIT

git restore 01_clean.toml >/dev/null 2>&1 || true

# Phase 1: generation records the response and its provenance metadata.
"$AILLY_BIN" --root .

assert_grep_q 'role = "assistant"' 01_clean.toml
assert_grep_q 'engine = "noop"' 01_clean.toml
assert_grep_q 'stop_reason = "end_turn"' 01_clean.toml

# Phase 2: --clean strips every [[response]] entry and preserves the prompt.
"$AILLY_BIN" --root . --clean

assert_no_grep '\[\[response\]\]' 01_clean.toml
assert_no_grep 'role = "assistant"' 01_clean.toml
assert_no_grep 'engine = ' 01_clean.toml
assert_no_grep 'stop_reason = ' 01_clean.toml
assert_grep_q 'prompt = ' 01_clean.toml

# Phase 3: --clean is idempotent. A second run produces a byte-identical file.
before=$(sha256sum 01_clean.toml | cut -d' ' -f1)
"$AILLY_BIN" --root . --clean
after=$(sha256sum 01_clean.toml | cut -d' ' -f1)

if [ "$before" != "$after" ]; then
    echo "FAIL: --clean is not idempotent (hash before=$before after=$after)" >&2
    exit 1
fi

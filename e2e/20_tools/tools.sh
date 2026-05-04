#!/usr/bin/env bash
# Feature test: tools chained through Conversation reach the Generator and
# survive on-disk round-trip.
#
# User story (narrative):
#
# A developer adds a baseline `tools = ["echo"]` to the project root
# `.ailly.toml`. One turn extends the chain with `tools = ["shout"]` (default
# `extend`); another replaces it with `tools = ["solo"]` plus
# `parent_tools = "replace"`. The CLI does not yet expose a way to register
# tool implementations, so the empty registry plus the default
# `strict_tools = true` policy must produce a loud, named failure when the
# turn runs. Independently, `--clean` (which does not invoke the engine)
# must preserve every on-disk `tools` and `parent_tools` declaration
# byte-stably across a load → write cycle.
#
# Phase 1 asserts on-disk schema round-trip via `--clean`.
# Phase 2 asserts strict-mode resolution failure on a normal run.

set -euo pipefail

cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ../_lib.sh
ensure_built

fixtures=(.ailly.toml 01_extend.toml 02_replace.toml)

cleanup() {
    rm -f out err
    git restore "${fixtures[@]}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

git restore "${fixtures[@]}" >/dev/null 2>&1 || true

# Phase 1: --clean round-trips the schema byte-stably.
#
# `--clean` strips [[response]] entries and rewrites every TOML it touches.
# With no responses present, the rewrite must reproduce the source bytes,
# which proves the loader and writer both understand `tools` and
# `parent_tools` at both the .ailly.toml level and the per-turn level.

before=()
for f in "${fixtures[@]}"; do
    before+=("$(sha256sum "$f" | cut -d' ' -f1)")
done

"$AILLY_BIN" --root . --clean

for i in "${!fixtures[@]}"; do
    f="${fixtures[$i]}"
    after=$(sha256sum "$f" | cut -d' ' -f1)
    if [ "${before[$i]}" != "$after" ]; then
        echo "FAIL: --clean did not round-trip $f byte-stably (before=${before[$i]} after=$after)" >&2
        echo "--- $f ---" >&2
        cat "$f" >&2
        exit 1
    fi
done

# The schema fields must still be present after --clean (defense-in-depth
# in case a future writer learns to omit empty defaults).
assert_grep_q 'tools = \["echo"\]' .ailly.toml
assert_grep_q 'tools = \["shout"\]' 01_extend.toml
assert_grep_q 'tools = \["solo"\]' 02_replace.toml
assert_grep_q 'parent_tools = "replace"' 02_replace.toml

# Phase 2: strict-mode resolution fails the run when names cannot be resolved.
#
# The CLI constructs a Generator with Settings::default (strict_tools = true)
# and supplies no registry. Every turn's chain therefore contains at least
# one unknown name. The first failure exits the run non-zero, names the
# unknown tool in the error, and leaves the turn file free of any
# [[response]] entry.

git restore "${fixtures[@]}" >/dev/null 2>&1 || true

set +e
"$AILLY_BIN" --root . > out 2> err
status=$?
set -e

if [ "$status" -eq 0 ]; then
    echo "FAIL: expected non-zero exit when strict_tools rejects unknown names; got 0" >&2
    echo "--- stdout ---" >&2
    cat out >&2
    echo "--- stderr ---" >&2
    cat err >&2
    exit 1
fi

assert_grep_q 'failed' err
assert_grep_q 'unknown tool' err

# The first turn (01_extend.toml) is the one that fails first because
# turns iterate in lexicographic order. Either "echo" or "shout" must be
# named in the error: both come from its resolved chain, and the error
# message lists the unresolved subset.
if ! grep -Eq 'echo|shout' err; then
    echo "FAIL: expected the failing turn's unknown tool name(s) in stderr" >&2
    cat err >&2
    exit 1
fi

# No turn file gained a [[response]] entry. The engine was never invoked
# for the failing turn, and the second turn never ran.
for f in 01_extend.toml 02_replace.toml; do
    assert_no_grep '\[\[response\]\]' "$f"
    assert_no_grep 'role = "assistant"' "$f"
done

# The on-disk tool declarations are still present after the failing run.
assert_grep_q 'tools = \["shout"\]' 01_extend.toml
assert_grep_q 'tools = \["solo"\]' 02_replace.toml
assert_grep_q 'parent_tools = "replace"' 02_replace.toml

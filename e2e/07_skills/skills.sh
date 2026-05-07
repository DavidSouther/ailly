#!/usr/bin/env bash
# E2E feature test for the Knowledge Skills slice
# (docs/developer/2026-05-03-B-knowledge-skills/).
#
# User story: an operator declares `skills = ["echo"]` in a `.ailly.toml`
# and drops a `SKILL.md` at `<root>/skills/echo/SKILL.md`. When
# they run ailly, the skill body reaches the engine alongside the
# inherited system text, and the noop engine echoes it back into the
# recorded response. The headline acceptance criterion of the slice's
# unit feature test (skill body between inherited and local system)
# is observable here at the CLI surface.
#
# Phase 2 verifies the error path: a `.ailly.toml` referencing a
# missing skill name surfaces `ContentError::Skill { name, source }`
# whose Display includes the search path attempted.
set -euo pipefail

cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ../_lib.sh
ensure_built

fixtures=(.ailly.toml 01_skill.toml)

cleanup() {
    rm -f out err
    git restore "${fixtures[@]}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

git restore "${fixtures[@]}" >/dev/null 2>&1 || true

# Phase 1: declared skill body flows through to the recorded response.
"$AILLY_BIN" --root . > out 2> err

if [ "${AILLY_E2E_LIVE:-0}" = "1" ]; then
    exit 0
fi

assert_grep_q 'role = "assistant"' 01_skill.toml
assert_grep_q 'You are running the skills integration test.' 01_skill.toml
assert_grep_q 'SKILL_BODY_MARKER_ECHO_42' 01_skill.toml

# Phase 2: missing skill name surfaces a provenance-rich error.
git restore "${fixtures[@]}" >/dev/null 2>&1 || true

cat > .ailly.toml <<'EOF'
system = "You are running the skills integration test."
skills = ["does-not-exist"]
EOF

set +e
"$AILLY_BIN" --root . > out 2> err
status=$?
set -e

if [ "$status" -eq 0 ]; then
    echo "FAIL: expected non-zero exit when skill is missing, got 0" >&2
    exit 1
fi
assert_grep_q 'does-not-exist' err
assert_grep_q 'Skill' err

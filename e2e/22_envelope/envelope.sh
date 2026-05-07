#!/usr/bin/env bash
# E2E feature test for the Conversation Prelude slice
# (docs/developer/2026-05-07-A-conversation-prelude/design.md).
#
# User story: an operator declares `system = "..."` and
# `skills = ["echo"]` in a `.ailly.toml`, drops a `SKILL.md` at
# `<root>/skills/echo/SKILL.md`, and runs ailly. The recorded turn
# file then contains an `[envelope]` table whose `prelude` array
# documents every block the engine actually received in
# walk-then-declaration order, with the skill body and the
# hardcoded ack visible inline. Reading the turn file alone is
# sufficient to answer "what skill bodies did the model see for
# this turn."
#
# Phase 2 verifies the negative case: a turn whose conversation has
# neither system text nor skills nor advertised tools produces a
# turn file with no `[envelope]` table at all
# (`skip_serializing_if = "Option::is_none"`).
set -euo pipefail

cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ../_lib.sh
ensure_built

phase1_fixtures=(phase1/.ailly.toml phase1/run/.ailly.toml phase1/run/01_envelope.toml)
phase2_fixtures=(empty/.ailly.toml empty/02_no_envelope.toml)

cleanup() {
    rm -f phase1/out phase1/err empty/out empty/err
    git restore "${phase1_fixtures[@]}" "${phase2_fixtures[@]}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

git restore "${phase1_fixtures[@]}" "${phase2_fixtures[@]}" >/dev/null 2>&1 || true

# Phase 1: a configured skill is recorded into the on-disk envelope.
# `--root phase1` isolates this case so the negative `empty/` fixture is
# not walked. Inherited system comes from `phase1/.ailly.toml`, local
# system and the skill declaration come from `phase1/run/.ailly.toml`,
# and the skill body is loaded from `phase1/skills/echo/SKILL.md`.
"$AILLY_BIN" --root phase1 > phase1/out 2> phase1/err

if [ "${AILLY_E2E_LIVE:-0}" = "1" ]; then
    exit 0
fi

turn_file=phase1/run/01_envelope.toml

assert_grep_q 'role = "assistant"' "$turn_file"
assert_grep_q '\[\[envelope.prelude\]\]' "$turn_file"
assert_grep_q 'source = "inherited_system"' "$turn_file"
assert_grep_q 'source = "skill:echo"' "$turn_file"
assert_grep_q 'source = "local_system"' "$turn_file"
assert_grep_q 'SKILL_BODY_MARKER_ECHO_42' "$turn_file"
assert_grep_q 'Understood. I will apply the echo skill when relevant.' "$turn_file"
assert_grep_q 'Understood. I will follow the inherited instructions.' "$turn_file"
assert_grep_q 'Understood. I will follow the local instructions.' "$turn_file"

# Within the [envelope.prelude] array, the source tags must appear in
# walk-then-declaration order: inherited_system, then skill:echo, then
# local_system. The line number of each `source = ...` entry encodes
# its position in the on-disk array.
inherited_line=$(grep -n 'source = "inherited_system"' "$turn_file" | head -1 | cut -d: -f1)
skill_line=$(grep -n 'source = "skill:echo"' "$turn_file" | head -1 | cut -d: -f1)
local_line=$(grep -n 'source = "local_system"' "$turn_file" | head -1 | cut -d: -f1)
if [ "$inherited_line" -ge "$skill_line" ] || [ "$skill_line" -ge "$local_line" ]; then
    echo "FAIL: envelope.prelude order must be inherited -> skill -> local; got inherited=$inherited_line skill=$skill_line local=$local_line" >&2
    exit 1
fi

# Phase 2: empty conversation produces no [envelope] table. `--root empty`
# is its own conversation root, isolated from `phase1`, so no inherited
# state leaks in.
"$AILLY_BIN" --root empty > empty/out 2> empty/err

assert_grep_q 'role = "assistant"' empty/02_no_envelope.toml
assert_no_grep '\[\[envelope.prelude\]\]' empty/02_no_envelope.toml
assert_no_grep '\[envelope\]' empty/02_no_envelope.toml

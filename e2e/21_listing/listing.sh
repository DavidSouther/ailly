#!/usr/bin/env bash
# E2E feature test for the list-workflows / list-skills / list-tools slice
# (docs/developer/2026-05-06-A-list-workflows/).
#
# User story (narrative):
#
# A developer keeps a project-local workflow in the conversation root and
# shares additional workflows and skills from a separate knowledge base.
# Before running anything, they want to see what is available. They run
# `ailly --list-workflows` and see both the local conversation-root
# workflow and the knowledge-base workflow, alphabetised. They run
# `ailly --list-skills` and see the knowledge-base skill with its
# description. They run `ailly --list-tools` and see every tool the
# workflow runtime registers. A bare `-w` produces the same listing as
# `--list-workflows` and exits 0. An unknown name passed to `-w` prints
# the listing with an unknown-name preface on stderr and exits non-zero.
set -euo pipefail

cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ../_lib.sh
ensure_built

cleanup() {
    rm -f out err
}
trap cleanup EXIT

cleanup

PROJECT="./project"
KB="./kb"

# Phase 1: --list-workflows unions the conversation-root workflow with
# the knowledge-base workflow, alphabetised, and exits 0.
"$AILLY_BIN" --root "$PROJECT" --knowledge "$KB" --list-workflows > out 2> err
assert_grep_q '^Available workflows:' out
assert_grep_q 'local' out
assert_grep_q 'shared' out
assert_grep_q 'Conversation-root workflow for the listing fixture.' out
assert_grep_q 'Knowledge-base workflow shared across projects.' out
assert_grep_q 'workflow.toml' out
assert_grep_q 'workflows/shared.toml' out

# Alphabetical order: `local` precedes `shared`.
local_line="$(grep -n '^  local' out | head -n1 | cut -d: -f1)"
shared_line="$(grep -n '^  shared' out | head -n1 | cut -d: -f1)"
if [ -z "$local_line" ] || [ -z "$shared_line" ] || [ "$local_line" -ge "$shared_line" ]; then
    echo "FAIL: workflows not alphabetised (local=$local_line shared=$shared_line)" >&2
    cat out >&2
    exit 1
fi

# Phase 2: --list-skills unions project-owned skills with the
# knowledge-base skills, alphabetised, with first-wins precedence on
# name collisions, and exits 0. The fixture exercises three flavors:
# a project-only skill (`local`), a project override of a kb skill
# (`greet`), and a kb-only skill (`announce`).
"$AILLY_BIN" --root "$PROJECT" --knowledge "$KB" --list-skills > out 2> err
assert_grep_q '^Available skills:' out
# kb-only skill surfaces from the knowledge base.
assert_grep_q 'announce' out
assert_grep_q 'Knowledge-base-only skill that the project does not override.' out
# Project-only skill surfaces from the project.
assert_grep_q 'local' out
assert_grep_q 'Project-owned skill that only the conversation root carries.' out
# Project overrides the kb skill of the same name (first-wins).
assert_grep_q 'greet' out
assert_grep_q 'Project override of the greet skill, used to verify first-wins.' out
assert_no_grep 'Knowledge-base skill that announces itself for the listing fixture.' out
# Source path is rendered relative to the owning knowledge root.
assert_grep_q 'skills/announce/SKILL.md' out
assert_grep_q 'skills/greet/SKILL.md' out
assert_grep_q 'skills/local/SKILL.md' out

# Alphabetical order: `announce` precedes `greet` precedes `local`.
announce_line="$(grep -n '^  announce' out | head -n1 | cut -d: -f1)"
greet_line="$(grep -n '^  greet' out | head -n1 | cut -d: -f1)"
local_skill_line="$(grep -n '^  local' out | head -n1 | cut -d: -f1)"
if [ -z "$announce_line" ] || [ -z "$greet_line" ] || [ -z "$local_skill_line" ] \
    || [ "$announce_line" -ge "$greet_line" ] || [ "$greet_line" -ge "$local_skill_line" ]; then
    echo "FAIL: skills not alphabetised (announce=$announce_line greet=$greet_line local=$local_skill_line)" >&2
    cat out >&2
    exit 1
fi

# Phase 3: --list-tools reports every tool the runtime registers and
# exits 0.
"$AILLY_BIN" --root "$PROJECT" --knowledge "$KB" --list-tools > out 2> err
assert_grep_q '^Registered tools:' out
assert_grep_q 'bash' out
assert_grep_q 'fs.absent' out
assert_grep_q 'fs.edit' out
assert_grep_q 'fs.grep' out
assert_grep_q 'fs.list' out
assert_grep_q 'user.clarify' out

# Phase 4: bare `-w` (no value) renders the same workflow listing and
# exits 0.
"$AILLY_BIN" --root "$PROJECT" --knowledge "$KB" -w > out 2> err
assert_grep_q '^Available workflows:' out
assert_grep_q 'local' out
assert_grep_q 'shared' out

# Phase 5: `-w UNKNOWN` prints the unknown-name preface on stderr,
# prints the listing on stdout, and exits non-zero.
set +e
"$AILLY_BIN" --root "$PROJECT" --knowledge "$KB" -w blueprint > out 2> err
status=$?
set -e
if [ "$status" -eq 0 ]; then
    echo "FAIL: -w blueprint expected non-zero exit, got 0" >&2
    cat err >&2
    exit 1
fi
assert_grep_q 'blueprint' err
assert_grep_q 'Searched' err
assert_grep_q '^Available workflows:' out
assert_grep_q 'local' out
assert_grep_q 'shared' out

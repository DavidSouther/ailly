#!/usr/bin/env bash
# E2E feature test for the Thinking Fast and Slow skill activation slice
# (docs/developer/2026-05-07-A-thinking-fast-slow/).
#
# User story: a workflow declares one skill from a plugin namespace
# (`dev:design`) on its `first` task. The project root carries an
# `AGENTS.md` and six fixture skills (the declared one, two namespace
# siblings, three bootstrap allow-list entries). When the workflow runs,
# the persisted turn TOML for the producing turn carries an
# `[enrichment]` table that records:
#
#   - one `[[enrichment.agents]]` row for the project AGENTS file,
#   - six `[[enrichment.skills]]` rows with origins `declared` and
#     `namespace`,
#   - an `[enrichment.tools]` block listing the always-on read-only
#     bundle.
#
# The eval turn carries the same enrichment, proving that the workflow
# eval path no longer bypasses the preamble at runtime.rs:340. The
# downstream `second` task declares no skills and carries no enrichment
# table.
set -euo pipefail

cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ../_lib.sh
ensure_built

cleanup() {
    rm -rf ./.ailly out err
}
trap cleanup EXIT

cleanup

export AILLY_NOOP_RESPONSE=approved

"$AILLY_BIN" --root . -w basic > out 2> err

first_turns=( ./.ailly/*_first.toml )
first_eval_turns=( ./.ailly/*_first_eval.toml )
second_turns=( ./.ailly/*_second.toml )

if [ "${#first_turns[@]}" -eq 0 ]; then
    echo "FAIL: workflow did not produce any *_first.toml turn file" >&2
    exit 1
fi
if [ "${#first_eval_turns[@]}" -eq 0 ]; then
    echo "FAIL: workflow did not produce any *_first_eval.toml turn file" >&2
    exit 1
fi
if [ "${#second_turns[@]}" -eq 0 ]; then
    echo "FAIL: workflow did not produce any *_second.toml turn file" >&2
    exit 1
fi

first_turn="${first_turns[0]}"
first_eval_turn="${first_eval_turns[0]}"
second_turn="${second_turns[0]}"

# The producing turn carries the [enrichment] table.
assert_grep_q '\[\[enrichment.agents\]\]' "$first_turn"
assert_grep_q 'e2e/22_enrichment/AGENTS.md' "$first_turn"

# Six skill rows, with the declared origin once and the namespace origin
# five times.
declared_count=$(grep -c 'origin = "declared"' "$first_turn" || true)
namespace_count=$(grep -c 'origin = "namespace"' "$first_turn" || true)
if [ "$declared_count" -ne 1 ]; then
    echo "FAIL: expected exactly one origin = \"declared\" in $first_turn, got $declared_count" >&2
    exit 1
fi
if [ "$namespace_count" -ne 5 ]; then
    echo "FAIL: expected five origin = \"namespace\" rows in $first_turn, got $namespace_count" >&2
    exit 1
fi

assert_grep_q 'name = "dev:design"' "$first_turn"
assert_grep_q 'name = "dev:using-dev"' "$first_turn"
assert_grep_q 'name = "dev:thinking"' "$first_turn"
assert_grep_q 'name = "general:using-general"' "$first_turn"
assert_grep_q 'name = "patterns:using-patterns"' "$first_turn"
assert_grep_q 'name = "characters:using-characters"' "$first_turn"

# The always-on read-only tool bundle is recorded in declared order.
assert_grep_q 'bundle = \["user.clarify", "fs.read", "fs.grep", "fs.list"\]' "$first_turn"
assert_grep_q 'declared = \[\]' "$first_turn"

# The eval turn carries the same enrichment as the producing turn. The
# bundle line and the declared skill row are sufficient sentinels.
assert_grep_q 'bundle = \["user.clarify", "fs.read", "fs.grep", "fs.list"\]' "$first_eval_turn"
assert_grep_q 'name = "dev:design"' "$first_eval_turn"
assert_grep_q 'origin = "declared"' "$first_eval_turn"

# The downstream `second` task declared no skills, so it must not carry
# an enrichment skills table.
assert_no_grep '\[\[enrichment.skills\]\]' "$second_turn"

# Workflow state confirms the eval routed `approved` to `second`.
assert_file_exists .ailly/workflow.state.toml
assert_grep_q 'workflow = "basic"' .ailly/workflow.state.toml
assert_grep_q 'task = "first"' .ailly/workflow.state.toml
assert_grep_q 'task = "second"' .ailly/workflow.state.toml

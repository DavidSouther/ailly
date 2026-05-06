#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ../_lib.sh
ensure_built

cleanup() {
    rm -f ./*_design.toml ./*_feature_test.toml ./*_plan.toml \
          ./*_design_eval.toml ./*_feature_test_eval.toml ./*_plan_eval.toml \
          workflow.state.toml \
          out1 err1 out2 err2 out3 err3 out4 err4
    git restore design.md feature-test.md plan.md 2>/dev/null || true
}
trap cleanup EXIT

cleanup

strip_draft() {
    local file="$1"
    grep -v '\*Draft' "$file" > "$file.tmp"
    mv "$file.tmp" "$file"
}

# Run 1: design pauses on its own draft marker.
"$AILLY_BIN" --root . -w ailly-dev-cycle > out1 2> err1
assert_grep_q 'workflow paused at task "design"' err1
assert_grep_q 'Clear the `\*Draft` marker' err1
assert_file_exists workflow.state.toml
assert_grep_q 'workflow = "ailly-dev-cycle"' workflow.state.toml
assert_grep_q 'task = "design"' workflow.state.toml

# A design turn file must exist after run 1 (the prompt turn is recorded
# even though the eval paused the workflow).
design_turns=( ./*_design.toml )
if [ "${#design_turns[@]}" -eq 0 ]; then
    echo "FAIL: run 1 produced no *_design.toml turn file" >&2
    exit 1
fi

strip_draft design.md

# Run 2: design clears, feature_test pauses on its own draft marker.
"$AILLY_BIN" --root . -w ailly-dev-cycle > out2 2> err2
assert_grep_q 'workflow paused at task "feature_test"' err2
assert_grep_q 'task = "feature_test"' workflow.state.toml

feature_test_turns=( ./*_feature_test.toml )
if [ "${#feature_test_turns[@]}" -eq 0 ]; then
    echo "FAIL: run 2 produced no *_feature_test.toml turn file" >&2
    exit 1
fi

strip_draft feature-test.md

# Run 3: feature_test clears, plan pauses on its own draft marker.
"$AILLY_BIN" --root . -w ailly-dev-cycle > out3 2> err3
assert_grep_q 'workflow paused at task "plan"' err3
assert_grep_q 'task = "plan"' workflow.state.toml

plan_turns=( ./*_plan.toml )
if [ "${#plan_turns[@]}" -eq 0 ]; then
    echo "FAIL: run 3 produced no *_plan.toml turn file" >&2
    exit 1
fi

strip_draft plan.md

# Run 4: plan clears, workflow completes.
"$AILLY_BIN" --root . -w ailly-dev-cycle > out4 2> err4
assert_no_grep 'workflow paused' err4
assert_no_grep 'workflow halted' err4
assert_no_grep 'failed' err4

# After completion the persisted history must record all three tasks as
# cleared. The runtime appends a history entry per task.
assert_grep_q 'task = "design"' workflow.state.toml
assert_grep_q 'task = "feature_test"' workflow.state.toml
assert_grep_q 'task = "plan"' workflow.state.toml
assert_grep_q 'result = "cleared"' workflow.state.toml

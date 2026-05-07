#!/usr/bin/env bash
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

# The `first` task carries an evaluation prompt that routes on the
# assistant's reply. The Noop engine returns the override verbatim, so
# the routing key is deterministic.
export AILLY_NOOP_RESPONSE=approved

"$AILLY_BIN" --root . -w basic > out 2> err

# Each task in the workflow produces a turn file named NN_<task>.toml. The
# exact NN sequence is an implementation detail; glob match the suffix.
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

# Each turn file carries a `step` field naming the task that produced it.
# The eval turn carries a `<task>_eval` step name.
assert_grep_q 'step = "first"' "$first_turn"
assert_grep_q 'step = "first_eval"' "$first_eval_turn"
assert_grep_q 'step = "second"' "$second_turn"

# Each turn file carries the synthesized user prompt and an assistant reply
# from the noop engine. The eval turn carries the evaluator's prompt text.
assert_grep_q 'First task.' "$first_turn"
assert_grep_q 'Reply with one word: approved.' "$first_eval_turn"
assert_grep_q 'Second task.' "$second_turn"
assert_grep_q 'role = "assistant"' "$first_turn"
assert_grep_q 'role = "assistant"' "$first_eval_turn"
assert_grep_q 'role = "assistant"' "$second_turn"

# The eval turn's numeric prefix must be greater than the main turn's.
first_n="$(basename "$first_turn" | cut -d_ -f1)"
eval_n="$(basename "$first_eval_turn" | cut -d_ -f1)"
if ! [ "$eval_n" -gt "$first_n" ]; then
    echo "FAIL: eval turn prefix $eval_n is not greater than main turn prefix $first_n" >&2
    exit 1
fi

# The runtime persists workflow state at the conversation root after the run.
# An empty queue plus two history entries means the pump completed cleanly,
# proving the eval response (`approved`) routed to `second`.
assert_file_exists .ailly/workflow.state.toml
assert_grep_q 'workflow = "basic"' .ailly/workflow.state.toml
assert_grep_q 'task = "first"' .ailly/workflow.state.toml
assert_grep_q 'task = "second"' .ailly/workflow.state.toml

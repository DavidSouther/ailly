#!/bin/bash

set -x
set -e

cd $(dirname $0)

rm -f ./out ./err

###

echo "basic mcp"
npx ailly --root ./root --log-format pretty --prompt "This is a conversation with system and two files." > >(tee ./out) 2> >(tee ./err >&2)
[ ! -s ./err ]                        # No error output at all

MESSAGES=(
  "USING TOOL add WITH ARGS [3, 7]"
  "TOOL RETURNED 10"
)
for M in "${MESSAGES[@]}"; do
  grep -q "$M" ./out
done

echo "(all conversation messages checked)"
rm -f ./out ./err

#!/bin/bash

set -x
set -e

cd $(dirname $0)

rm -f ./out ./err

###

echo "basic conversation"
npx ailly --root ./root --log-format pretty --prompt "This is a conversation with system and two files." > >(tee ./out) 2> >(tee ./err >&2)
tail ./out ./err
grep -vq '"name":"@ailly/core"' ./out # No JSONL output in `out`
[ ! -s ./err ]                        # No error output at all

MESSAGES=(
  "You are running an integration test."
  "user: File a."
  "user: File b."
  "This is a conversation with system and two files."
)
for M in "${MESSAGES[@]}"; do
  grep -q "$M" ./out
done

echo "(all conversation messages checked)"
rm -f ./out ./err

###

echo "verbose conversation"
npx ailly --root ./root --log-format pretty --verbose --prompt "This is a conversation with system and two files." > >(tee ./out) 2> >(tee ./err >&2)
tail ./out ./err

MESSAGES=(
  "You are running an integration test."
  "user: File a."
  "user: File b."
  "This is a conversation with system and two files."
)
for M in "${MESSAGES[@]}"; do
  grep -q "$M" ./out
done
MESSAGES=(
  "Found 2 at or below"
  "Ready to generate 1 messages"
  "Preparing /dev/stdout"
  "All 1 requests finished"
)
for M in "${MESSAGES[@]}"; do
  grep -vq "$M" ./err
done

echo "(all verbose conversation messages checked)"
rm -f ./out ./err

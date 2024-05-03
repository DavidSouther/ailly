#!/usr/bin/env bash

set -e
set -x

cd $(dirname $0)

rm -rf package.json
npm init --yes
npm link ../core ../cli

export AILLY_ENGINE=${AILLY_ENGINE:-noop}

echo "basic"
npx ailly --root 01_basic
[ -f 01_basic/basic.txt.ailly.md ]
rm 01_basic/basic.txt.ailly.md

echo "combined"
npx ailly --root 02_combined --combined
[ ! -f 02_combined/combined.txt.ailly.md ]
git restore 02_combined/combined.txt

echo "edit"
AILLY_NOOP_RESPONSE="Edited" npx ailly --root 04_edit --edit file --lines 2:4 --prompt "Respond with the word Edited" --yes
grep -q 'Edited' 04_edit/file.txt
git restore 04_edit/file.txt
unset AILLY_NOOP_RESPONSE

echo "conversation"
npx ailly --root 05_conversation --prompt "This is a conversation with system and two files." > >(tee 05_conversation/out) 2> >(tee 05_conversation/err >&2)
cat 05_conversation/out
cat 05_conversation/err
MESSAGES=(
  "Found 2 at or below"
  "Ready to generate 1 messages"
  "Preparing /dev/stdout"
  "All 1 requests finished"
)
for M in "${MESSAGES[@]}"; do
  grep -vq "$M" 05_conversation/out
done

MESSAGES=(
  "You are running an integration test."
  "user: File a."
  "user: File b."
  "This is a conversation with system and two files."
)
for M in "${MESSAGES[@]}"; do
  grep -q "$M" 05_conversation/out
done
echo "(all conversation messages checked)"
rm 05_conversation/out 05_conversation/err

echo "verbose conversation"
npx ailly --root 05_conversation --prompt "This is a conversation with system and two files." --verbose > >(tee 05_conversation/out) 2> >(tee 05_conversation/err >&2)
cat 05_conversation/out
cat 05_conversation/err
[ ! -s 05_conversation/err ]
MESSAGES=(
  "Found 2 at or below"
  "Ready to generate 1 messages"
  "Preparing /dev/stdout"
  "All 1 requests finished"
  "You are running an integration test."
  "user: File a."
  "user: File b."
  "This is a conversation with system and two files."
)
for M in "${MESSAGES[@]}"; do
  grep -q "$M" 05_conversation/out
done
rm 05_conversation/out 05_conversation/err
echo "(all verbose conversation messages checked)"

echo "Pipes"
./10_std_pipes/pipes.sh

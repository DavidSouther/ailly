#!/bin/bash

set -x
set -e

cd $(dirname $0)

rm -f ./out ./err
git restore ./root/01_a.txt.ailly.md

###

echo "continued conversation"
npx ailly --root ./root --log-format json --verbose --continue > >(tee ./out) 2> >(tee ./err >&2)
tail ./out ./err
[ ! -s ./err ] # No error output at all

MESSAGES=(
  "user: File a"
  "assistant: Continuing response"
)
for M in "${MESSAGES[@]}"; do
  grep -q "$M" ./root/01_a.txt.ailly.md
done

echo "(continued conversation messages checked)"
rm -f ./out ./err
git restore ./root/01_a.txt.ailly.md

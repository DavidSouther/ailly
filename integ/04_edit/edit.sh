#!/bin/bash

set -x
set -e

cd $(dirname $0)

AILLY_NOOP_RESPONSE="Edited" \
  npx ailly --root root --edit file.txt --lines 2:4 --prompt "Respond with the word Edited" --yes \
  --verbose > >(tee ./root/out) 2> >(tee ./root/err >&2)

grep -q 'Edited' root/file.txt
grep -q 'Instructions are happening in the context of this folder' root/out
grep -q 'You are replacing this section:\\n```\\nLine 2\\nLine 3\\n```' root/out
git restore root/file.txt
rm root/{err,out}

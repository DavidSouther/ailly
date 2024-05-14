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
AILLY_NOOP_RESPONSE="Edited" \
  npx ailly --root 04_edit --edit file.txt --lines 2:4 --prompt "Respond with the word Edited" --yes \
  --verbose > >(tee ./04_edit/out) 2> >(tee ./04_edit/err >&2)
grep -q 'Edited' 04_edit/file.txt
grep -q 'Instructions are happening in the context of this folder' 04_edit/out
grep -q 'You are replacing this section:\\n```\\nLine 2\\nLine 3\\n```' 04_edit/out
git restore 04_edit/file.txt
rm 04_edit/{err,out}

echo "conversations"
./05_conversation/conversation.sh

echo "Stream"
./06_stream/stream.sh

echo "Pipes"
./10_std_pipes/pipes.sh

echo "Max depth"
./11_max_depth/max_depth.sh

echo "Tempate Views"
./12_template_view/template_view.sh

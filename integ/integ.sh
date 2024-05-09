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

echo "conversations"
./05_conversation/conversation.sh

echo "Pipes"
./10_std_pipes/pipes.sh

echo "Max depth"
./11_max_depth/max_depth.sh

echo "Tempate Views"
./12_template_view/template_view.sh

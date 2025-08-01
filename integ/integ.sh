#!/usr/bin/env bash

set -e
set -x

cd $(dirname $0)

rm -rf package.json
npm init --yes
npm link ../core ../cli

export AILLY_TEMPLATE_VIEW=
export AILLY_ENGINE=noop

echo "Basic"
npx ailly --root 01_basic
[ -f 01_basic/basic.txt.ailly.md ]
rm 01_basic/basic.txt.ailly.md

echo "Combined"
npx ailly --root 02_combined --combined
[ ! -f 02_combined/combined.txt.ailly.md ]
git restore 02_combined/combined.txt

echo "Edit"
./04_edit/edit.sh

echo "Conversations"
./05_conversation/conversation.sh

echo "Pipes"
./10_std_pipes/pipes.sh

echo "Max depth"
./11_max_depth/max_depth.sh

echo "Tempate Views"
./12_template_view/template_view.sh

echo "Plugin"
./15_plugin/plugin.sh

echo "Tools"
./20_tools/mcp.sh

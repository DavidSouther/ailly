#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

. ./_lib.sh
ensure_built

export AILLY_E2E_BUILT=1
export AILLY_ENGINE=noop

for script in [0-9]*/*.sh; do
    echo "==> $script"
    "./$script"
done

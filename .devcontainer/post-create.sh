#!/usr/bin/env bash
set -euo pipefail

sudo chown -R vscode:vscode /workspaces/ailly/target /usr/local/cargo/registry

rustup component add rust-analyzer clippy rustfmt
jq -r '.enabledPlugins | keys | .[]' .claude/settings.json  | xargs -n 1 claude plugin install
#!/usr/bin/env bash
set -euo pipefail

sudo chown -R vscode:vscode /workspaces/ailly/target /usr/local/cargo/registry

rustup component add rust-analyzer clippy rustfmt

npm install -g @anthropic-ai/claude-code

mkdir -p "$HOME/.claude"
cat > "$HOME/.claude/settings.json" <<'JSON'
{
  "$schema": "https://json.schemastore.org/claude-code-settings.json",
  "permissions": { "defaultMode": "bypassPermissions" }
}
JSON
claude "/reload-plugins"
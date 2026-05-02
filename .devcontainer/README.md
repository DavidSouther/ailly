# Dev Container

This container is the development environment for `ailly`'s Rust imlpementation. The container is a security boundary, letting coding agents run with permission prompts disabled.

## Layout

- Host project folder mounts at `/workspaces/ailly`.
- Host `../ailly_typescript/` mounts at `/workspaces/ailly/typescript`, read-only. It is reference material for the Rust port. Edits to it from inside the container are blocked at the mount layer.
- Named volumes back `~/.cargo/registry` and `target/` for fast rebuilds. `cargo clean` clears the volume, not a host directory.

## Tooling

- Rust toolchain from `mcr.microsoft.com/devcontainers/rust:1-bookworm`. `rust-analyzer`, `clippy`, and `rustfmt` are installed as `rustup` components on first create.
- Node LTS, for `npm install -g @anthropic-ai/claude-code`.
- GitHub CLI.
- VS Code extensions: `rust-analyzer`, `even-better-toml`, `CodeLLDB`, `claude-code`.

## Claude Code permissions

`postCreateCommand` writes `~/.claude/settings.json` with:

```json
{ "permissions": { "defaultMode": "bypassPermissions" } }
```

Tool prompts are off inside the container. If the setting key is rejected by your installed CLI version, fall back to launching with `claude --dangerously-skip-permissions`.

## Claude Code plugins

Project-level plugin list lives in `.claude/settings.json` at the repo root and is picked up automatically when Claude Code is launched from `/workspaces/ailly`. The `rust-analyzer-lsp@claude-plugins-official` plugin is enabled there so the LSP tool can resolve `.rs` files. The `rust-analyzer` binary is installed by `post-create.sh` via `rustup component add rust-analyzer`.

If the LSP tool reports `No LSP server available for file type: .rs` on first session, run `/plugin install rust-analyzer-lsp@claude-plugins-official` once, then `/reload-plugins`. The install state then persists in `~/.claude/plugins/`.

## First run

1. Open `ailly_rust/` in VS Code.
2. Run `Dev Containers: Reopen in Container`.
3. The first build pulls the image and warms the Cargo cache. Subsequent rebuilds reuse the named volumes.

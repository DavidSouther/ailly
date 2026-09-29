# Developer Guide

## Toolchain

This project uses the **nightly tools/stable build** pattern:

- `rust-toolchain.toml` pins a specific nightly release. Cargo, rustfmt, clippy, and rust-analyzer all pick this up automatically, so no editor configuration is needed.
- `Cargo.toml` sets `rust-version` to the current stable MSRV. The `incompatible_msrv` lint fires if code uses an API unavailable on that stable version, so nightly-only APIs cannot accidentally slip into production code.

The toolchain installs automatically on first use. `rustup`'s proxy intercepts any `cargo` invocation, reads `rust-toolchain.toml`, and installs the pinned toolchain if it is not already present.

To bump the pin manually, run `mise run bump-nightly`. A GitHub Actions workflow (`.github/workflows/bump-nightly.yml`) runs this automatically every Monday and opens a pull request if the date changed.

## Formatting

Formatting uses nightly rustfmt with unstable options enabled in `.rustfmt.toml`.

Run the formatter:

```
mise run format
```

## Development Tasks

All tasks are defined in `mise.toml` and delegate to cargo:

| Task | When |
|---|---|
| `mise run format` | After editing any file |
| `mise run check` | Before running tests |
| `mise run test` | After any change |
| `mise run lint` | Before committing |

The Claude Code hooks in `.claude/settings.json` run these tasks automatically on file save and session stop.

## Lint Policy

Clippy is configured in `Cargo.toml` with `pedantic` and `restriction` lint groups enabled at warn level. The pattern for this codebase is to suppress individual lints at the call site using `#[expect(clippy::lint_name, reason = "...")]` rather than adding blanket `allow` attributes. The `reason` field is required — it documents why the lint does not apply.

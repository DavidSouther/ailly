use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "ailly",
    version,
    about = "Ailly CLI",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Base folder to search for content and system prompts. Defaults to the current directory.
    #[arg(short = 'r', long)]
    pub root: Option<PathBuf>,

    /// Generate a final, single piece of content and print the response to standard out.
    #[arg(short = 'p', long, env = "AILLY_PROMPT")]
    pub prompt: Option<String>,

    /// Strip every recorded `[[response]]` entry from every turn file under `--root`,
    /// preserving the prompt and writing the file back idempotently.
    #[arg(long, conflicts_with = "prompt")]
    pub clean: bool,

    /// Engine to drive inference. `noop` is available for testing.
    #[arg(long, env = "AILLY_ENGINE")]
    pub engine: Option<String>,

    /// Model to use within the engine. Default depends on the engine.
    #[arg(long, env = "AILLY_MODEL")]
    pub model: Option<String>,

    /// Set log level to info. Equivalent to `--log-level v`.
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// env-filter directive controlling log output. Accepts `v`/`verbose` for info,
    /// numeric levels 0-4 (error, warn, info, debug, trace), or any `RUST_LOG`-style
    /// filter expression.
    #[arg(long, value_name = "FILTER")]
    pub log_level: Option<String>,

    /// Output format for log records.
    #[arg(long, value_enum, default_value_t = LogFormat::Pretty)]
    pub log_format: LogFormat,
}

impl Cli {
    /// Resolve the project root, falling back to the current working directory.
    pub fn root(&self) -> PathBuf {
        self.root
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum LogFormat {
    Pretty,
    Json,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn clap_rejects_clean_combined_with_prompt() {
        let result = Cli::try_parse_from(["ailly", "--clean", "--prompt", "foo"]);
        let err = result.expect_err("--clean must conflict with --prompt at parse time");
        let msg = err.to_string();
        assert!(
            msg.contains("--clean") && msg.contains("--prompt"),
            "expected conflict message naming both flags, got: {msg}"
        );
    }
}

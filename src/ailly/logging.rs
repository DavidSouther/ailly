use std::io::Write;

use env_logger::Builder;

use super::args::LogFormat;

/// Configure the global `log` facade via `env_logger`.
///
/// Precedence for the filter directive:
/// 1. `--verbose` forces `info`.
/// 2. `--log-level` is honored next, with two conveniences before falling
///    through to env-filter syntax: `v`/`verbose` means `info`, and a single
///    digit `0`-`4` maps to `error`/`warn`/`info`/`debug`/`trace`.
/// 3. `RUST_LOG` is consulted when neither flag is set.
/// 4. Default is `warn`.
pub fn init(verbose: bool, log_level: Option<&str>, log_format: LogFormat) {
    let filter = if verbose {
        "info".to_string()
    } else if let Some(raw) = log_level {
        normalize_filter(raw)
    } else {
        std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".to_string())
    };

    let mut builder = Builder::new();
    builder.parse_filters(&filter);

    if log_format == LogFormat::Json {
        builder.format(|buf, record| {
            let line = serde_json::json!({
                "level": record.level().as_str(),
                "target": record.target(),
                "message": record.args().to_string(),
            });
            writeln!(buf, "{line}")
        });
    }

    builder.init();
}

fn normalize_filter(raw: &str) -> String {
    match raw.trim() {
        "v" | "V" | "verbose" => "info".to_string(),
        "0" => "error".to_string(),
        "1" => "warn".to_string(),
        "2" => "info".to_string(),
        "3" => "debug".to_string(),
        "4" => "trace".to_string(),
        other => other.to_string(),
    }
}

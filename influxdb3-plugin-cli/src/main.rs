// The `[[bin]]` target shares the crate's `[dependencies]` block with
// `[lib]`; the bin itself does not name these crates, so acknowledge them
// here to satisfy `unused_crate_dependencies`.
use anstyle as _;
use anyhow as _;
use influxdb3_plugin_schemas as _;
use influxdb3_plugin_sdk as _;
use semver as _;
use serde as _;
use serde_json as _;
use thiserror as _;

// Dev-deps used only by inline `#[cfg(test)]` modules in the lib or by
// integration tests in `tests/*.rs`; same unused-dep workaround.
#[cfg(test)]
use assert_cmd as _;
#[cfg(test)]
use insta as _;
#[cfg(test)]
use pep508_rs as _;
#[cfg(test)]
use predicates as _;
#[cfg(test)]
use rstest as _;
#[cfg(test)]
use tempfile as _;
#[cfg(test)]
use toml as _;
#[cfg(test)]
use url as _;

use clap::Parser;
use influxdb3_plugin_cli::PluginConfig;
use std::io::IsTerminal;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let config = match PluginConfig::try_parse() {
        Ok(c) => c,
        Err(e) => return handle_clap_error(e),
    };
    match config.run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            use influxdb3_plugin_cli::__private::CliErrorKind;
            match CliErrorKind::of(&e) {
                CliErrorKind::Silent => {
                    // stdout already carried the signal (e.g. validate's
                    // diagnostics doc in JSON mode). Do not pollute stderr.
                    std::process::ExitCode::from(1)
                }
                CliErrorKind::Usage => {
                    eprintln!("{e:#}");
                    std::process::ExitCode::from(2)
                }
                CliErrorKind::Runtime => {
                    eprintln!("{e:#}");
                    std::process::ExitCode::from(1)
                }
            }
        }
    }
}

/// Renders a clap error, honoring JSON-mode single-line-stderr discipline
/// when JSON mode is in effect.
///
/// `--help` / `--version` branches are printed on stdout and exit 0
/// (matching `Parser::parse`'s default). Error branches are printed on
/// stderr: full multi-line rendering in human mode, or one non-empty,
/// footer-stripped line in JSON mode.
///
/// The `new list`-hint on unknown-template errors is preserved in human
/// mode (on a second stderr line) but suppressed in JSON mode — a second
/// line would violate the single-meaningful-line contract, and JSON-mode
/// consumers key off the error discriminator rather than handholding.
fn handle_clap_error(e: clap::Error) -> std::process::ExitCode {
    use clap::error::ErrorKind;
    if !e.use_stderr() {
        // `--help` / `--version` paths.
        let _ = e.print();
        return std::process::ExitCode::from(0);
    }
    let is_unknown_new_template = e.kind() == ErrorKind::InvalidSubcommand
        && std::env::args().nth(1).as_deref() == Some("new");
    if json_mode_active() {
        eprintln!("{}", collapse_clap_error(&e));
    } else {
        let _ = e.print();
        if is_unknown_new_template {
            eprintln!("Run `influxdb3-plugin new list` to see available templates.");
        }
    }
    std::process::ExitCode::from(2)
}

/// Mirrors `resolve_output_mode`'s precedence for the case where
/// `PluginConfig` isn't yet constructed (clap parse failed).
///
/// 1. Explicit `--output json` in argv wins.
/// 2. Explicit `--output <other>` in argv forces non-JSON (lets users opt
///    out of the collapsed rendering on demand).
/// 3. `!isatty(stdout)` → JSON.
/// 4. `CI=true|1` → JSON.
/// 5. Otherwise → human.
fn json_mode_active() -> bool {
    let mut iter = std::env::args().skip(1);
    while let Some(a) = iter.next() {
        if a == "--output" {
            match iter.next().as_deref() {
                Some("json") => return true,
                Some(_) => return false,
                None => break,
            }
        } else if let Some(v) = a.strip_prefix("--output=") {
            return v == "json";
        }
    }
    if !std::io::stdout().is_terminal() {
        return true;
    }
    matches!(std::env::var("CI").as_deref(), Ok("true" | "1"))
}

/// Collapses clap's multi-line error rendering into one meaningful line.
///
/// Strips blank lines and the `For more information, try '--help'.`
/// footer (uninteresting in JSON mode, always the last line). Joins the
/// remaining lines with single spaces.
fn collapse_clap_error(err: &clap::Error) -> String {
    let rendered = err.render().to_string();
    let parts: Vec<&str> = rendered
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with("For more information"))
        .collect();
    if parts.is_empty() {
        // Defensive: clap always renders at least `error: ...`; if it
        // somehow returns nothing, fall back to `err`'s Display.
        return err.to_string();
    }
    parts.join(" ")
}

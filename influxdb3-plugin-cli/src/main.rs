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
use flate2 as _;
#[cfg(test)]
use insta as _;
#[cfg(test)]
use pep508_rs as _;
#[cfg(test)]
use predicates as _;
#[cfg(test)]
use rstest as _;
#[cfg(test)]
use tar as _;
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
        Err(e) => render_error_and_exit(e),
    }
}

fn render_error_and_exit(e: anyhow::Error) -> std::process::ExitCode {
    use influxdb3_plugin_cli::__private::{CliError, CliErrorKind};

    let mode_is_json = json_mode_active();
    let kind = CliErrorKind::of(&e);

    if mode_is_json {
        use influxdb3_plugin_cli::__private::write_envelope_error;
        let mut stdout = std::io::stdout().lock();
        let je = match CliError::json_error_of(&e) {
            Some(typed) => typed,
            None => {
                let fallback = fallback_json_error(&e);
                let _ = write_envelope_error(&mut stdout, &fallback);
                return exit_code(kind);
            }
        };
        let _ = write_envelope_error(&mut stdout, je);
    } else {
        match CliError::json_error_of(&e) {
            Some(je) => {
                let palette = human_error_palette();
                let _ = influxdb3_plugin_cli::__private::render_human_error(
                    je,
                    palette,
                    &mut std::io::stderr(),
                );
            }
            None => {
                eprintln!("{e:#}");
            }
        }
    }

    exit_code(kind)
}

fn exit_code(kind: influxdb3_plugin_cli::__private::CliErrorKind) -> std::process::ExitCode {
    use influxdb3_plugin_cli::__private::CliErrorKind;
    match kind {
        CliErrorKind::Usage => std::process::ExitCode::from(2),
        CliErrorKind::Runtime => std::process::ExitCode::from(1),
    }
}

fn fallback_json_error(e: &anyhow::Error) -> influxdb3_plugin_cli::__private::JsonError {
    use influxdb3_plugin_cli::__private::JsonError;
    let causes: Vec<String> = e.chain().skip(1).map(|c| c.to_string()).collect();
    JsonError {
        code: "cli::unknown".into(),
        message: format!("{e:#}"),
        field: None,
        details: None,
        diagnostics: vec![],
        cause: causes,
    }
}

fn handle_clap_error(e: clap::Error) -> std::process::ExitCode {
    use influxdb3_plugin_cli::__private::{json_error_from_clap, write_envelope_error};
    if !e.use_stderr() {
        let _ = e.print();
        return std::process::ExitCode::from(0);
    }
    if json_mode_active() {
        let je = json_error_from_clap(&e);
        let mut stdout = std::io::stdout().lock();
        let _ = write_envelope_error(&mut stdout, &je);
    } else {
        let _ = e.print();
        let is_unknown_new_template = e.kind() == clap::error::ErrorKind::InvalidSubcommand
            && std::env::args().nth(1).as_deref() == Some("new");
        if is_unknown_new_template {
            eprintln!("Run `influxdb3-plugin new list` to see available templates.");
        }
    }
    std::process::ExitCode::from(2)
}

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

fn human_error_palette() -> influxdb3_plugin_cli::__private::Palette {
    influxdb3_plugin_cli::__private::stderr_error_palette()
}

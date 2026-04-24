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
use rstest as _;
#[cfg(test)]
use assert_cmd as _;
#[cfg(test)]
use insta as _;
#[cfg(test)]
use predicates as _;
#[cfg(test)]
use tempfile as _;
#[cfg(test)]
use toml as _;

use clap::Parser;
use influxdb3_plugin_cli::PluginConfig;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let config = PluginConfig::parse();
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

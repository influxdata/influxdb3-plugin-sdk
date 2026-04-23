// `[[bin]]` shares the cli crate's `[dependencies]` block with `[lib]`. The
// lib uses anyhow/schemas/sdk; the bin does not name them directly. Same
// `use _ as _;` workaround as `lib.rs` to satisfy `unused_crate_dependencies`
// on the bin target.
use anyhow as _;
use influxdb3_plugin_schemas as _;
use influxdb3_plugin_sdk as _;

// Inline `#[cfg(test)]` modules in the lib use `rstest`; the bin's test
// build sees it as a declared dev-dep but never names it. Same guard
// pattern as the lib-side `tokio` / `sdk` workarounds.
#[cfg(test)]
use rstest as _;
// `assert_cmd` / `predicates` are spawn-and-assert helpers used only in
// `tests/*.rs`. The bin's test build sees them as declared dev-deps but
// never names them.
#[cfg(test)]
use assert_cmd as _;
#[cfg(test)]
use predicates as _;

use clap::Parser;
use influxdb3_plugin_cli::PluginConfig;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let config = PluginConfig::parse();
    match config.run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e:#}");
            std::process::ExitCode::from(1)
        }
    }
}

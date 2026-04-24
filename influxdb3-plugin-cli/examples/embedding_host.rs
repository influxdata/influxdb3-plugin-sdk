//! Compile-time contract for embedding `influxdb3-plugin-cli` inside a
//! host such as InfluxDB. Built by `cargo check --examples`; never
//! shipped.
//!
//! What this example proves at the compile boundary:
//!
//! - `PluginConfig::run()` is `async` and returns `Result`, so the host
//!   can `block_on(config.run())` from inside its existing tokio
//!   runtime without nesting.
//! - `PluginConfig` carries a `version` attribute clap can surface via
//!   `CommandFactory::command().get_version()`.
//! - `Manifest`, `IndexEntry`, and other parse-time types reach
//!   embedding consumers through `influxdb3-plugin-cli`'s re-exports,
//!   never via a direct dep on `influxdb3-plugin-schemas`. This example
//!   deliberately does NOT import from `schemas`.
//!
//! The crate-root `allow(unused_crate_dependencies)` is required
//! because the example is a separate compile unit and pulls every
//! dev-dep into its dep graph regardless of usage.

#![allow(unused_crate_dependencies)]

use clap::{CommandFactory, Parser};
use influxdb3_plugin_cli::{IndexEntry, Manifest, PluginConfig};

fn main() {
    // Async `run()` composes with the host's tokio runtime.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime builds");

    // Construct a no-op invocation so we don't touch the filesystem.
    // `--help` exits inside clap before reaching `run()`, so we just
    // confirm the parser-construction path. The real embedding host
    // would receive a real argv.
    if let Ok(config) = PluginConfig::try_parse_from(["influxdb3-plugin", "--help"]) {
        let _ = runtime.block_on(config.run());
    }

    // Version attribute is queryable via clap's reflection.
    let cmd = PluginConfig::command();
    let _version: &str = cmd
        .get_version()
        .expect("PluginConfig must declare a version attribute (S2-5)");

    // Schemas types reach consumers through `cli` re-exports. The fn
    // signatures below would fail to type-check if the re-exports
    // drifted.
    fn _takes_manifest(_: Manifest) {}
    fn _takes_index_entry(_: IndexEntry) {}
}

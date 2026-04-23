//! Compile-time contract for Spec 2 § Phase-2 Embedding (S2-4, S2-5,
//! S2-10). Built by `cargo check --examples`; never shipped.
//!
//! See `tests/version_smoke.rs` for the rationale behind the
//! crate-root allow — the example is a separate compile unit and gets
//! every dev-dep in its dep graph regardless of usage.
//!
//! What this example proves at the compile boundary:
//!
//! - **S2-4** — `PluginConfig::run()` is `async` and returns `Result`,
//!   so the host can `block_on(config.run())` from inside its existing
//!   tokio runtime without nesting.
//! - **S2-5** — `PluginConfig` carries a `version` attribute clap can
//!   surface via `CommandFactory::command().get_version()`.
//! - **S2-10** — `Manifest`, `IndexEntry`, and other parse-time types
//!   reach embedding consumers through `influxdb3-plugin-cli`'s
//!   re-exports, never via a direct dep on `influxdb3-plugin-schemas`.
//!   This example deliberately does NOT import from `schemas`.

#![allow(unused_crate_dependencies)]

use clap::{CommandFactory, Parser};
use influxdb3_plugin_cli::{IndexEntry, Manifest, PluginConfig};

fn main() {
    // S2-4: async run() composes with the host's tokio runtime.
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

    // S2-5: version attribute is queryable via clap's reflection.
    let cmd = PluginConfig::command();
    let _version: &str = cmd
        .get_version()
        .expect("PluginConfig must declare a version attribute (S2-5)");

    // S2-10: schemas types reach consumers through `cli` re-exports.
    // The fn signatures below would fail to type-check if the
    // re-exports drifted.
    fn _takes_manifest(_: Manifest) {}
    fn _takes_index_entry(_: IndexEntry) {}
}

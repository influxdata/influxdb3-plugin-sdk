//! Top-level embeddable CLI config.

use clap::Parser;

/// `<version> (<sha> <build-date>)` fragment fed to clap's `version` attribute.
///
/// `build.rs` writes the parenthesized half (either `"(abc1234 2026-04-23)"`
/// or `"(unknown)"`) to `$OUT_DIR/version_fragment.rs`; we splice it onto
/// `CARGO_PKG_VERSION` here. clap prepends the binary `name` ("influxdb3-plugin")
/// when rendering `--version`, so the final shape matches Spec 2 § S2-21:
///
/// ```text
/// influxdb3-plugin <version> (<short-sha> <build-date>)
/// ```
const VERSION_STRING: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " ",
    include!(concat!(env!("OUT_DIR"), "/version_fragment.rs")),
);

/// Top-level embeddable CLI config for the `influxdb3-plugin` binary.
///
/// Constructed from a process's argument list via clap (`PluginConfig::parse()`)
/// and dispatched through [`PluginConfig::run`]. The standalone binary's
/// `main.rs` is the v1 entry point; phase-2 embedding into `influxdb_pro`
/// will mount `PluginConfig` as a variant of the host's top-level command
/// enum and invoke `run()` from inside the host's existing tokio runtime
/// (Spec 2 § Phase-2 Embedding Constraints).
#[derive(Debug, Parser)]
#[command(
    name = "influxdb3-plugin",
    version = VERSION_STRING,
    about = "Author-side tooling for InfluxDB 3 plugins.",
    long_about = None,
)]
pub struct PluginConfig {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// Scaffold a new plugin or registry from a built-in template.
    New(crate::commands::new::Args),
    // Remaining variants land in D31 (`validate`), D32 (`package`),
    // D33 (`yank`).
}

impl PluginConfig {
    /// Runs the parsed subcommand.
    ///
    /// Always async per Spec 2 S2-4 — the future is currently sync internally
    /// but the signature lets phase-2 embedding await without a runtime
    /// switch. Returns through `Result` per S2-7 (no `std::process::exit`
    /// from the library surface).
    pub async fn run(self) -> anyhow::Result<()> {
        match self.command {
            Command::New(args) => args.run(),
        }
    }
}

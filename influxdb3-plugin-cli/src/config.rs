//! Top-level embeddable CLI config.

use clap::Parser;

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
    version = env!("CARGO_PKG_VERSION"),
    about = "Author-side tooling for InfluxDB 3 plugins.",
    long_about = None,
)]
pub struct PluginConfig {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    // Populated in D30 (`new`), D31 (`validate`), D32 (`package`),
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
        match self.command {}
    }
}

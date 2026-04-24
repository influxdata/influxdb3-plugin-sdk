//! Top-level embeddable CLI config.

use clap::Parser;

/// `<version> (<sha> <build-date>)` fragment fed to clap's `version` attribute.
///
/// `build.rs` writes the parenthesized half (either `"(abc1234 2026-04-23)"`
/// or `"(unknown)"`) to `$OUT_DIR/version_fragment.rs`; we splice it onto
/// `CARGO_PKG_VERSION`. clap prepends the binary `name` when rendering
/// `--version`, producing:
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
/// Parsed from argv via clap and dispatched through [`PluginConfig::run`].
/// An embedding host can mount this as a variant of its own top-level
/// command enum and invoke `run()` from its existing tokio runtime.
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
    #[command(subcommand)]
    New(crate::commands::new::NewCommand),
    /// Validate a plugin directory.
    Validate(crate::commands::validate::Args),
    /// Validate, archive, hash, and emit a derived index entry.
    Package(crate::commands::package::Args),
    /// Toggle the `yanked` flag on an existing index entry.
    Yank(crate::commands::yank::Args),
}

impl PluginConfig {
    /// Runs the parsed subcommand.
    ///
    /// `async` even though the current implementation is internally sync, so
    /// an embedding host can `.await` this without a runtime switch. Errors
    /// are returned via `Result`; this surface must not call
    /// `std::process::exit`.
    ///
    /// # Examples
    ///
    /// Standalone binary entry — `main.rs` does the equivalent of:
    ///
    /// ```rust,no_run
    /// # async fn _doc() -> anyhow::Result<()> {
    /// use clap::Parser;
    /// use influxdb3_plugin_cli::PluginConfig;
    ///
    /// let config = PluginConfig::parse();
    /// config.run().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Embedding host — invoke from the host's existing tokio runtime; no
    /// nested runtime is needed:
    ///
    /// ```rust,no_run
    /// # fn _doc(host_argv: Vec<String>) -> anyhow::Result<()> {
    /// use clap::Parser;
    /// use influxdb3_plugin_cli::PluginConfig;
    ///
    /// let config = PluginConfig::try_parse_from(host_argv)?;
    /// let runtime = tokio::runtime::Builder::new_current_thread()
    ///     .enable_all()
    ///     .build()?;
    /// runtime.block_on(config.run())?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run(self) -> anyhow::Result<()> {
        match self.command {
            Command::New(sub) => sub.run(),
            Command::Validate(args) => args.run(),
            Command::Package(args) => args.run(),
            Command::Yank(args) => args.run(),
        }
    }
}

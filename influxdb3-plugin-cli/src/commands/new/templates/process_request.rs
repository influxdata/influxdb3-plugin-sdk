//! `process_request` template — plugin triggered by an HTTP request.

use super::TemplateMetadata;
use crate::commands::new::{GlobalFlags, plugin_scaffold};
use clap::Args as ClapArgs;
use influxdb3_plugin_schemas::TriggerType;
use std::path::PathBuf;

pub(crate) const METADATA: TemplateMetadata = TemplateMetadata {
    name: "Process Request Plugin",
    short_name: "process_request",
    description: "Plugin triggered by an HTTP request.",
};

#[derive(Debug, ClapArgs)]
#[command(override_usage = "influxdb3-plugin new process_request [OPTIONS] [PATH]")]
pub(crate) struct Args {
    #[command(flatten)]
    pub global: GlobalFlags,

    /// Target directory. Created if missing. Defaults to `.`.
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Plugin name written into `manifest.toml`. Defaults to the basename
    /// of `[path]`.
    #[arg(long)]
    pub name: Option<String>,

    /// SemVer range for `dependencies.database_version`.
    #[arg(long)]
    pub database_version: Option<String>,
}

pub(crate) fn run(args: Args) -> anyhow::Result<()> {
    plugin_scaffold(
        &METADATA,
        TriggerType::ProcessRequest,
        args.global,
        args.path,
        args.name,
        args.database_version,
    )
}

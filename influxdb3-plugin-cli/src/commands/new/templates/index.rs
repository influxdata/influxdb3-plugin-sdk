//! `index` template — empty registry index file.

use super::TemplateMetadata;
use crate::commands::new::{GlobalFlags, index_scaffold};
use clap::Args as ClapArgs;
use std::path::PathBuf;

pub(crate) const METADATA: TemplateMetadata = TemplateMetadata {
    name: "Index",
    short_name: "index",
    description: "Empty registry index file.",
};

#[derive(Debug, ClapArgs)]
#[command(override_usage = "influxdb3-plugin new index [OPTIONS] [PATH]")]
pub(crate) struct Args {
    #[command(flatten)]
    pub global: GlobalFlags,

    /// Target directory. Created if missing. Defaults to `.`.
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// URL written into `index.json`'s `artifacts_url`.
    /// Defaults to `file://<absolute path of [path]>`.
    #[arg(long)]
    pub artifacts_url: Option<String>,
}

pub(crate) fn run(args: Args) -> anyhow::Result<()> {
    index_scaffold(&METADATA, args.global, args.path, args.artifacts_url)
}

//! `new` command — scaffold a plugin or registry from a built-in template.
//!
//! Each built-in template is a self-contained module under [`templates`];
//! the CLI dispatches through [`NewCommand`]. Adding a template is a
//! two-step: add a submodule under `templates/` and a variant here.

pub(crate) mod list;
pub(crate) mod templates;

use clap::{Args as ClapArgs, Subcommand};
use influxdb3_plugin_schemas::{PluginName, TriggerType};
use influxdb3_plugin_sdk::scaffold;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::color::Stream;
use crate::output::{Env, OutputMode, RealEnv, json::NewOutput, resolve_output_mode};
use crate::style::Palette;
use templates::TemplateMetadata;

/// Global flags shared by every `new` subcommand. Flattened into each
/// subcommand's `Args` so clap parses them at the leaf level.
#[derive(Debug, ClapArgs)]
pub(crate) struct GlobalFlags {
    /// Output format. Auto-detected from stdout's TTY status and `CI`
    /// when omitted.
    #[arg(long, value_enum)]
    pub output: Option<OutputMode>,

    /// Overwrite files the template would write if they already exist.
    /// Files in the target directory that the template does not write
    /// are left alone regardless.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Subcommand)]
#[command(
    rename_all = "snake_case",
    override_usage = "\
influxdb3-plugin new <TEMPLATE> [PATH] [OPTIONS]
       influxdb3-plugin new list [OPTIONS]",
    after_help = "\
Run `influxdb3-plugin new list` to see available templates, \
or `influxdb3-plugin new <template> --help` for per-template options."
)]
pub(crate) enum NewCommand {
    /// List available templates.
    List(list::Args),

    /// Plugin triggered by rows written to a database.
    #[command(hide = true)]
    ProcessWrites(templates::process_writes::Args),

    /// Plugin triggered on a schedule.
    #[command(hide = true)]
    ProcessScheduledCall(templates::process_scheduled_call::Args),

    /// Plugin triggered by an HTTP request.
    #[command(hide = true)]
    ProcessRequest(templates::process_request::Args),

    /// Empty plugin registry directory.
    #[command(hide = true)]
    Registry(templates::registry::Args),
}

impl NewCommand {
    pub(crate) fn run(self) -> anyhow::Result<()> {
        match self {
            Self::List(a) => list::run(a),
            Self::ProcessWrites(a) => templates::process_writes::run(a),
            Self::ProcessScheduledCall(a) => templates::process_scheduled_call::run(a),
            Self::ProcessRequest(a) => templates::process_request::run(a),
            Self::Registry(a) => templates::registry::run(a),
        }
    }
}

pub(crate) fn plugin_scaffold(
    metadata: &'static TemplateMetadata,
    trigger: TriggerType,
    global: GlobalFlags,
    path: PathBuf,
    name_arg: Option<String>,
    database_version: Option<String>,
) -> anyhow::Result<()> {
    run_plugin_with_env(
        metadata,
        trigger,
        global,
        path,
        name_arg,
        database_version,
        &RealEnv,
    )
}

pub(crate) fn registry_scaffold(
    metadata: &'static TemplateMetadata,
    global: GlobalFlags,
    path: PathBuf,
    artifacts_url: Option<String>,
) -> anyhow::Result<()> {
    run_registry_with_env(metadata, global, path, artifacts_url, &RealEnv)
}

fn run_plugin_with_env(
    metadata: &'static TemplateMetadata,
    trigger: TriggerType,
    global: GlobalFlags,
    path: PathBuf,
    name_arg: Option<String>,
    database_version: Option<String>,
    env: &dyn Env,
) -> anyhow::Result<()> {
    let mode = resolve_output_mode(global.output, env);
    let stdout_palette = Palette::for_stream(Stream::Stdout, mode, env, env.stdout_is_terminal());

    let name = resolve_plugin_name(&path, name_arg)?;
    scaffold::plugin(
        &path,
        &name,
        trigger,
        database_version.as_deref(),
        global.force,
    )?;

    let summary = Summary {
        kind: SummaryKind::Plugin,
        template: metadata,
        target_dir: path,
        name: Some(name),
        files_written: vec![
            PathBuf::from("manifest.toml"),
            PathBuf::from("__init__.py"),
            PathBuf::from("README.md"),
        ],
    };
    render(&summary, mode, stdout_palette)
}

fn run_registry_with_env(
    metadata: &'static TemplateMetadata,
    global: GlobalFlags,
    path: PathBuf,
    artifacts_url: Option<String>,
    env: &dyn Env,
) -> anyhow::Result<()> {
    let mode = resolve_output_mode(global.output, env);
    let stdout_palette = Palette::for_stream(Stream::Stdout, mode, env, env.stdout_is_terminal());

    scaffold::registry(&path, artifacts_url.as_deref(), global.force)?;

    let summary = Summary {
        kind: SummaryKind::Registry,
        template: metadata,
        target_dir: path,
        name: None,
        files_written: vec![PathBuf::from("index.json")],
    };
    render(&summary, mode, stdout_palette)
}

/// Derives a plugin name from `--name`, else the basename of `dir`.
/// Returns an actionable error when neither yields a valid plugin name.
fn resolve_plugin_name(dir: &Path, name_arg: Option<String>) -> anyhow::Result<String> {
    let (candidate, source_was_explicit) = match name_arg {
        Some(n) => (n, true),
        None => {
            let basename = dir
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "could not derive a plugin name from path {dir:?}; \
                         pass --name <name> explicitly"
                    )
                })?
                .to_owned();
            (basename, false)
        }
    };

    match PluginName::from_str(&candidate) {
        Ok(_) => Ok(candidate),
        Err(_) if source_was_explicit => Err(crate::cli_error::CliError::usage(anyhow::anyhow!(
            "--name {candidate:?} is not a valid plugin name; \
                 must match `[a-z0-9][a-z0-9-]{{0,63}}` (1-64 chars)"
        ))),
        Err(_) => Err(anyhow::anyhow!(
            "derived plugin name {candidate:?} (from path basename) is not a valid \
             plugin name; pass --name <name> explicitly. Plugin names must match \
             `[a-z0-9][a-z0-9-]{{0,63}}` (1-64 chars)"
        )),
    }
}

#[derive(Debug)]
struct Summary {
    kind: SummaryKind,
    template: &'static TemplateMetadata,
    target_dir: PathBuf,
    name: Option<String>,
    files_written: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
enum SummaryKind {
    Plugin,
    Registry,
}

impl SummaryKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Plugin => "plugin",
            Self::Registry => "registry",
        }
    }
}

fn render(summary: &Summary, mode: OutputMode, stdout_palette: Palette) -> anyhow::Result<()> {
    match mode {
        OutputMode::Human => render_human(summary, stdout_palette, &mut std::io::stdout())?,
        OutputMode::Json => render_json(summary, &mut std::io::stdout())?,
    }
    Ok(())
}

fn render_human(
    summary: &Summary,
    palette: Palette,
    writer: &mut impl std::io::Write,
) -> std::io::Result<()> {
    let kind = summary.kind.as_str();
    let template = summary.template.short_name;
    let ok = palette.success.render();
    let ok_reset = palette.success.render_reset();
    writeln!(
        writer,
        "{ok}Scaffolded {kind} ({template} template) at {}{ok_reset}",
        summary.target_dir.display()
    )?;
    if let Some(name) = &summary.name {
        writeln!(writer, "  name: {name}")?;
    }
    writeln!(writer, "  files written:")?;
    for file in &summary.files_written {
        writeln!(writer, "    {}", file.display())?;
    }
    Ok(())
}

fn render_json(summary: &Summary, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
    let payload = NewOutput {
        kind: summary.kind.as_str(),
        template: summary.template.short_name,
        target_dir: summary.target_dir.clone(),
        name: summary.name.clone(),
        files_written: summary.files_written.clone(),
    };
    serde_json::to_writer_pretty(&mut *writer, &payload)?;
    writeln!(writer)?;
    Ok(())
}

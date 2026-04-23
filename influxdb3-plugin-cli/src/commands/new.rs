//! `influxdb3-plugin new` — scaffold a plugin or registry from a template.
//!
//! Wraps [`influxdb3_plugin_sdk::scaffold::plugin`] and
//! [`influxdb3_plugin_sdk::scaffold::registry`]. CLI-side responsibilities:
//!
//! - Resolve `--name` against the template kind. Plugin templates derive
//!   from path basename when `--name` is absent; if the derived value
//!   doesn't satisfy [`PluginName`], error and ask for an explicit
//!   `--name` (mirrors `cargo new`).
//! - Resolve `--database-version` (plugin templates) and `--artifacts-url`
//!   (registry template); pass through to the SDK as `Option<&str>`.
//! - Render the result as either a one-line human confirmation or a
//!   single JSON document on stdout per Spec 2 § S2-15 data-tool idiom.

use clap::{Args as ClapArgs, ValueEnum};
use influxdb3_plugin_schemas::{PluginName, TriggerType};
use influxdb3_plugin_sdk::scaffold;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::output::{Env, OutputMode, RealEnv, json::NewOutput, resolve_output_mode};

/// Built-in template identifiers. Adding a variant is a minor bump of
/// `influxdb3-plugin-cli`; renaming is a major bump per Spec 2 § Stability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub(crate) enum Template {
    ProcessWrites,
    ProcessScheduledCall,
    ProcessRequest,
    Registry,
}

impl Template {
    fn as_str(self) -> &'static str {
        match self {
            Self::ProcessWrites => "process_writes",
            Self::ProcessScheduledCall => "process_scheduled_call",
            Self::ProcessRequest => "process_request",
            Self::Registry => "registry",
        }
    }

    fn trigger(self) -> Option<TriggerType> {
        match self {
            Self::ProcessWrites => Some(TriggerType::ProcessWrites),
            Self::ProcessScheduledCall => Some(TriggerType::ProcessScheduledCall),
            Self::ProcessRequest => Some(TriggerType::ProcessRequest),
            Self::Registry => None,
        }
    }
}

/// Parsed `new` arguments.
#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Template to scaffold.
    template: Template,

    /// Target directory. Created if missing. Defaults to the current
    /// working directory.
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Output format. Auto-detected from stdout's TTY status and `CI`
    /// when omitted (Spec 2 § S2-14).
    #[arg(long, value_enum)]
    output: Option<OutputMode>,

    /// Plugin name written into `manifest.toml`. Plugin templates only.
    /// Defaults to the basename of `path`; the command errors if the
    /// derived basename does not match the plugin-name regex.
    #[arg(long)]
    name: Option<String>,

    /// Database-version SemVer range written into the scaffolded
    /// manifest's `dependencies.database_version`. Plugin templates only.
    /// Defaults to the SDK's [`scaffold::DEFAULT_DATABASE_VERSION`].
    #[arg(long)]
    database_version: Option<String>,

    /// Registry artifacts URL written into the scaffolded `index.json`.
    /// Registry template only. Defaults to `file://<absolute path>`.
    #[arg(long)]
    artifacts_url: Option<String>,
}

impl Args {
    /// Runs `new` per the parsed args. Errors propagate as
    /// [`anyhow::Error`]; `main.rs` renders them via `{e:#}` and maps to
    /// exit code 1 per S2-7 / S2-18.
    pub(crate) fn run(self) -> anyhow::Result<()> {
        run_with_env(self, &RealEnv)
    }
}

fn run_with_env(args: Args, env: &dyn Env) -> anyhow::Result<()> {
    let mode = resolve_output_mode(args.output, env);
    reject_unsupported_flags(&args)?;

    let target_dir = args.path;

    let summary = match args.template.trigger() {
        Some(trigger) => run_plugin(
            &target_dir,
            args.template,
            trigger,
            args.name,
            args.database_version,
        )?,
        None => run_registry(&target_dir, args.template, args.artifacts_url)?,
    };

    render(&summary, mode)
}

/// Rejects template/flag combinations the spec doesn't permit.
///
/// Spec 2 § `new` scopes `--name` and `--database-version` to plugin
/// templates and `--artifacts-url` to the registry template. Silent
/// ignore would let CI scripts pass nonsense flags and never learn about
/// it; we surface the mismatch at parse-validation time.
fn reject_unsupported_flags(args: &Args) -> anyhow::Result<()> {
    if args.template == Template::Registry {
        if args.name.is_some() {
            return Err(crate::cli_error::CliError::usage(anyhow::anyhow!(
                "--name is not supported with the `registry` template"
            )));
        }
        if args.database_version.is_some() {
            return Err(crate::cli_error::CliError::usage(anyhow::anyhow!(
                "--database-version is not supported with the `registry` template"
            )));
        }
    } else if args.artifacts_url.is_some() {
        return Err(crate::cli_error::CliError::usage(anyhow::anyhow!(
            "--artifacts-url is only supported with the `registry` template, not `{}`",
            args.template.as_str()
        )));
    }
    Ok(())
}

fn run_plugin(
    dir: &Path,
    template: Template,
    trigger: TriggerType,
    name_arg: Option<String>,
    database_version: Option<String>,
) -> anyhow::Result<Summary> {
    let name = resolve_plugin_name(dir, name_arg)?;
    scaffold::plugin(dir, &name, trigger, database_version.as_deref())?;

    Ok(Summary {
        kind: SummaryKind::Plugin,
        template,
        target_dir: dir.to_path_buf(),
        name: Some(name),
        files_written: vec![
            PathBuf::from("manifest.toml"),
            PathBuf::from("__init__.py"),
            PathBuf::from("README.md"),
        ],
    })
}

fn run_registry(
    dir: &Path,
    template: Template,
    artifacts_url: Option<String>,
) -> anyhow::Result<Summary> {
    scaffold::registry(dir, artifacts_url.as_deref())?;
    Ok(Summary {
        kind: SummaryKind::Registry,
        template,
        target_dir: dir.to_path_buf(),
        name: None,
        files_written: vec![PathBuf::from("index.json")],
    })
}

/// Derives a plugin name from `--name` when given, else the basename of
/// `dir`. Returns an actionable error when the derived value does not
/// satisfy the plugin-name regex AND no `--name` was provided.
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
        Err(_) if source_was_explicit => Err(crate::cli_error::CliError::usage(
            anyhow::anyhow!(
                "--name {candidate:?} is not a valid plugin name; \
                 must match `[a-z0-9][a-z0-9-]{{0,63}}` (1-64 chars)"
            ),
        )),
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
    template: Template,
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

fn render(summary: &Summary, mode: OutputMode) -> anyhow::Result<()> {
    match mode {
        OutputMode::Human => {
            render_human(summary, &mut std::io::stdout())?;
        }
        OutputMode::Json => {
            render_json(summary, &mut std::io::stdout())?;
        }
    }
    Ok(())
}

fn render_human(summary: &Summary, writer: &mut impl std::io::Write) -> std::io::Result<()> {
    let kind = summary.kind.as_str();
    let template = summary.template.as_str();
    writeln!(
        writer,
        "Scaffolded {kind} ({template} template) at {}",
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
        template: summary.template.as_str(),
        target_dir: summary.target_dir.clone(),
        name: summary.name.clone(),
        files_written: summary.files_written.clone(),
    };
    serde_json::to_writer_pretty(&mut *writer, &payload)?;
    writeln!(writer)?;
    Ok(())
}

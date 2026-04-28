//! `influxdb3-plugin yank` — toggle the `yanked` flag on an index entry.
//!
//! Wraps [`influxdb3_plugin_sdk::mutate_index::yank`] /
//! [`influxdb3_plugin_sdk::mutate_index::unyank`] and carries the same
//! input-immutability + non-overlap rails as `package`.
//!
//! # Idempotency
//!
//! Re-yanking an already-yanked entry (or `--undo`-ing a not-yanked entry)
//! is a successful no-op. The SDK distinguishes the two cases via
//! [`mutate_index::YankOutcome`]; we surface that signal in `--output json`
//! as a four-case `YankOutcomeWire` enum (`yanked` / `unyanked` /
//! `already_yanked` / `already_unyanked`) and in human mode as a printed
//! informational marker.

use clap::Args as ClapArgs;
use clap::builder::{StringValueParser, TypedValueParser};
use clap::error::{ContextKind, ContextValue, Error as ClapError, ErrorKind};
use clap::{Arg, Command};
use influxdb3_plugin_schemas::{Index, PluginName};
use influxdb3_plugin_sdk::{SdkError, mutate_index};
use semver::Version;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::color::Stream;
use crate::output::{
    Env, OutputMode, RealEnv,
    json::{YankOutcomeWire, YankOutput},
    resolve_output_mode,
};
use crate::style::Palette;

/// Parsed `yank` arguments.
#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// `<name>@<version>` identifier of the entry to toggle.
    #[arg(value_name = "NAME@VERSION", value_parser = NameAtVersionParser)]
    target: NameAtVersion,

    /// Output format. Auto-detected from stdout's TTY status and `CI`
    /// when omitted.
    #[arg(long, value_enum)]
    output: Option<OutputMode>,

    /// Input registry index (read-only).
    #[arg(long)]
    index: PathBuf,

    /// Output directory. Receives the derived `index.json`. Created if
    /// missing. Must NOT resolve to the directory containing `--index`.
    #[arg(long)]
    out: PathBuf,

    /// Clear `yanked` instead of setting it.
    #[arg(long)]
    undo: bool,
}

impl Args {
    pub(crate) fn run(self) -> anyhow::Result<()> {
        run_with_env(self, &RealEnv)
    }
}

fn run_with_env(args: Args, env: &dyn Env) -> anyhow::Result<()> {
    let mode = resolve_output_mode(args.output, env);
    let stdout_palette = Palette::for_stream(Stream::Stdout, mode, env, env.stdout_is_terminal());
    let NameAtVersion { name, version } = args.target;

    let index_raw = std::fs::read_to_string(&args.index)
        .map_err(|e| anyhow::anyhow!("failed to read --index {}: {e}", args.index.display()))?;
    let mut index = Index::parse_json(&index_raw).map_err(|e| {
        anyhow::anyhow!(
            "failed to parse --index {} as a registry index: {e}",
            args.index.display()
        )
    })?;

    std::fs::create_dir_all(&args.out)
        .map_err(|e| anyhow::anyhow!("failed to create --out {}: {e}", args.out.display()))?;
    if paths_overlap(&args.index, &args.out)? {
        return Err(crate::cli_error::CliError::usage_msg(format!(
            "--out {} resolves to the directory containing --index {}; \
             they must be disjoint (Spec 2 § S2-12)",
            args.out.display(),
            args.index.display(),
        )));
    }

    let sdk_outcome = if args.undo {
        mutate_index::unyank(&mut index, name.as_str(), &version)?
    } else {
        mutate_index::yank(&mut index, name.as_str(), &version)?
    };

    let derived_index_json = index.to_canonical_json().map_err(SdkError::from)?;
    let derived_index_path = args.out.join("index.json");
    std::fs::write(&derived_index_path, &derived_index_json).map_err(|e| {
        anyhow::anyhow!(
            "failed to write derived index {}: {e}",
            derived_index_path.display()
        )
    })?;

    let payload = YankOutput {
        name: name.as_str().to_owned(),
        version: version.to_string(),
        outcome: outcome_wire(sdk_outcome, args.undo),
        index_path: canonicalize_or_keep(&derived_index_path),
    };

    render(&payload, mode, stdout_palette)
}

fn outcome_wire(outcome: mutate_index::YankOutcome, undo: bool) -> YankOutcomeWire {
    match (outcome, undo) {
        (mutate_index::YankOutcome::Transitioned, false) => YankOutcomeWire::Yanked,
        (mutate_index::YankOutcome::Transitioned, true) => YankOutcomeWire::Unyanked,
        (mutate_index::YankOutcome::AlreadyInDesiredState, false) => YankOutcomeWire::AlreadyYanked,
        (mutate_index::YankOutcome::AlreadyInDesiredState, true) => {
            YankOutcomeWire::AlreadyUnyanked
        }
    }
}

/// `<name>@<version>` positional target for the `yank` subcommand.
///
/// Parsing is driven through the clap [`TypedValueParser`] below so that
/// malformed inputs surface as clap usage errors (exit 2) rather than
/// runtime errors (exit 1). The [`CliError`] classification in
/// `cli_error.rs` continues to handle path-overlap and other runtime-path
/// usage errors.
#[derive(Debug, Clone)]
pub(crate) struct NameAtVersion {
    pub(crate) name: PluginName,
    pub(crate) version: Version,
}

impl FromStr for NameAtVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let (name_str, ver_str) = s.split_once('@').ok_or_else(|| {
            format!(
                "expected `<name>@<version>` (e.g., `downsampler@1.2.0`); got {s:?} with no `@` separator"
            )
        })?;
        let name = name_str
            .parse::<PluginName>()
            .map_err(|e| format!("invalid plugin name {name_str:?}: {e}"))?;
        let version = Version::parse(ver_str)
            .map_err(|e| format!("invalid SemVer version {ver_str:?}: {e}"))?;
        Ok(Self { name, version })
    }
}

/// Clap [`TypedValueParser`] adapter around [`NameAtVersion::from_str`].
///
/// Surfacing the error as [`ErrorKind::ValueValidation`] produces clap's
/// standard `invalid value '<v>' for '<ARG>'` message and gives an exit
/// status of 2, matching the usage-error exit-code contract.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NameAtVersionParser;

impl TypedValueParser for NameAtVersionParser {
    type Value = NameAtVersion;

    fn parse_ref(
        &self,
        cmd: &Command,
        arg: Option<&Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, ClapError> {
        let inner = StringValueParser::new();
        let s = TypedValueParser::parse_ref(&inner, cmd, arg, value)?;
        s.parse::<NameAtVersion>().map_err(|msg| {
            let mut err = ClapError::new(ErrorKind::ValueValidation).with_cmd(cmd);
            if let Some(arg) = arg {
                err.insert(
                    ContextKind::InvalidArg,
                    ContextValue::String(arg.to_string()),
                );
            }
            // Put the FromStr detail in InvalidValue — clap's default error
            // renderer emits it; `Suggested` would be silently discarded.
            err.insert(
                ContextKind::InvalidValue,
                ContextValue::String(format!("{s}: {msg}")),
            );
            err
        })
    }
}

// Same shape as the helper in `commands::package`.
fn paths_overlap(index_path: &Path, out_dir: &Path) -> anyhow::Result<bool> {
    let idx = std::fs::canonicalize(index_path).map_err(|e| {
        anyhow::anyhow!(
            "failed to canonicalize --index {}: {e}",
            index_path.display()
        )
    })?;
    let out = std::fs::canonicalize(out_dir)
        .map_err(|e| anyhow::anyhow!("failed to canonicalize --out {}: {e}", out_dir.display()))?;
    let idx_parent = idx.parent().unwrap_or_else(|| Path::new("/"));
    Ok(idx_parent == out)
}

fn canonicalize_or_keep(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn render(payload: &YankOutput, mode: OutputMode, stdout_palette: Palette) -> anyhow::Result<()> {
    match mode {
        OutputMode::Human => render_human(payload, stdout_palette, &mut std::io::stdout())?,
        OutputMode::Json => render_json(payload, &mut std::io::stdout())?,
    }
    Ok(())
}

fn render_human(
    payload: &YankOutput,
    palette: Palette,
    writer: &mut impl std::io::Write,
) -> std::io::Result<()> {
    match payload.outcome {
        YankOutcomeWire::Yanked | YankOutcomeWire::Unyanked => {
            let yanked = matches!(payload.outcome, YankOutcomeWire::Yanked);
            let action = if yanked { "yank" } else { "unyank" };
            let warn = palette.warn.render();
            let warn_reset = palette.warn.render_reset();
            writeln!(
                writer,
                "{warn}{action}ed {}@{} (yanked={yanked}){warn_reset}",
                payload.name, payload.version,
            )?;
        }
        YankOutcomeWire::AlreadyYanked | YankOutcomeWire::AlreadyUnyanked => {
            let yanked = matches!(payload.outcome, YankOutcomeWire::AlreadyYanked);
            let dim = palette.dim.render();
            let dim_reset = palette.dim.render_reset();
            writeln!(
                writer,
                "{dim}{}@{} already in desired state (yanked={yanked}); no change{dim_reset}",
                payload.name, payload.version,
            )?;
        }
    }
    writeln!(writer, "  index: {}", payload.index_path.display())?;
    Ok(())
}

fn render_json(payload: &YankOutput, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
    serde_json::to_writer_pretty(&mut *writer, payload)?;
    writeln!(writer)?;
    Ok(())
}

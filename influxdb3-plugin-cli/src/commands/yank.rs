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
use influxdb3_plugin_sdk::{SdkError, ValidationError, mutate_index};
use semver::Version;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::cli_error::CliError;
use crate::color::Stream;
use crate::output::error_mapping::{ErrorContext, json_error_from_sdk, json_error_from_validation};
use crate::output::json::{JsonError, YankOutcomeWire, YankOutput, write_envelope_ok};
use crate::output::{Env, OutputMode, RealEnv, resolve_output_mode};
use crate::path_display::{absolutize_for_json, display_relative_to_cwd};
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

    // Read input index.
    let index_raw = std::fs::read_to_string(&args.index).map_err(|e| {
        CliError::runtime(JsonError {
            code: "io::read_failed".into(),
            message: format!("failed to read --index {}: {e}", args.index.display()),
            field: Some(args.index.display().to_string()),
            details: Some(serde_json::json!({
                "path": args.index.display().to_string(),
                "io_kind": format!("{:?}", e.kind()),
            })),
            diagnostics: vec![],
            cause: vec![e.to_string()],
        })
    })?;

    // Parse index JSON — SchemaErrors → structured diagnostics.
    let mut index = Index::parse_json(&index_raw).map_err(|schema_errors| {
        let diagnostics: Vec<JsonError> = schema_errors
            .into_iter()
            .map(|reported| json_error_from_validation(&ValidationError::SchemaReported(reported)))
            .collect();
        CliError::runtime(JsonError {
            code: "yank::index_parse_failed".into(),
            message: format!(
                "failed to parse --index {} as a registry index",
                args.index.display()
            ),
            field: Some(args.index.display().to_string()),
            details: None,
            diagnostics,
            cause: vec![],
        })
    })?;

    // Create --out directory.
    std::fs::create_dir_all(&args.out).map_err(|e| {
        CliError::runtime(JsonError {
            code: "io::write_failed".into(),
            message: format!("failed to create --out {}: {e}", args.out.display()),
            field: Some(args.out.display().to_string()),
            details: Some(serde_json::json!({
                "path": args.out.display().to_string(),
                "io_kind": format!("{:?}", e.kind()),
            })),
            diagnostics: vec![],
            cause: vec![e.to_string()],
        })
    })?;

    // Path-equivalence check.
    if paths_overlap(&args.index, &args.out)? {
        return Err(CliError::usage(JsonError {
            code: "usage::input_output_overlap".into(),
            message: format!(
                "--out {} resolves to the directory containing --index {}; \
                 this would overwrite the input index. Use a different --out directory.",
                args.out.display(),
                args.index.display(),
            ),
            field: None,
            details: Some(serde_json::json!({
                "index": args.index.display().to_string(),
                "out": args.out.display().to_string(),
            })),
            diagnostics: vec![],
            cause: vec![],
        }));
    }

    // Yank / unyank via SDK.
    let sdk_outcome = if args.undo {
        mutate_index::unyank(&mut index, name.as_str(), &version)
            .map_err(|e| CliError::runtime(json_error_from_sdk(&e, ErrorContext::Yank)))?
    } else {
        mutate_index::yank(&mut index, name.as_str(), &version)
            .map_err(|e| CliError::runtime(json_error_from_sdk(&e, ErrorContext::Yank)))?
    };

    // Canonical JSON serialization.
    let derived_index_json = index.to_canonical_json().map_err(|e| {
        let sdk_err = SdkError::from(e);
        CliError::runtime(json_error_from_sdk(&sdk_err, ErrorContext::Yank))
    })?;

    let out_abs = absolutize_for_json(&args.out)?;
    let derived_index_path = out_abs.join("index.json");

    // Write derived index.
    std::fs::write(&derived_index_path, &derived_index_json).map_err(|e| {
        CliError::runtime(JsonError {
            code: "io::write_failed".into(),
            message: format!(
                "failed to write derived index {}: {e}",
                derived_index_path.display()
            ),
            field: Some(derived_index_path.display().to_string()),
            details: Some(serde_json::json!({
                "path": derived_index_path.display().to_string(),
                "io_kind": format!("{:?}", e.kind()),
            })),
            diagnostics: vec![],
            cause: vec![e.to_string()],
        })
    })?;

    let published_at = index
        .plugins
        .iter()
        .find(|entry| entry.name == name && entry.version == version)
        .expect("mutate_index succeeded, so target entry must exist")
        .published_at
        .to_string();

    let payload = YankOutput {
        name: name.as_str().to_owned(),
        version: version.to_string(),
        published_at,
        outcome: outcome_wire(sdk_outcome, args.undo),
        index_path: derived_index_path,
    };

    match mode {
        OutputMode::Human => {
            render_human(&payload, stdout_palette, &mut std::io::stdout())?;
        }
        OutputMode::Json => {
            write_envelope_ok(&mut std::io::stdout(), &payload)?;
        }
    }
    Ok(())
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
        CliError::runtime(JsonError {
            code: "io::canonicalize_failed".into(),
            message: format!(
                "failed to canonicalize --index {}: {e}",
                index_path.display()
            ),
            field: Some(index_path.display().to_string()),
            details: Some(serde_json::json!({
                "path": index_path.display().to_string(),
                "io_kind": format!("{:?}", e.kind()),
            })),
            diagnostics: vec![],
            cause: vec![e.to_string()],
        })
    })?;
    let out = std::fs::canonicalize(out_dir).map_err(|e| {
        CliError::runtime(JsonError {
            code: "io::canonicalize_failed".into(),
            message: format!("failed to canonicalize --out {}: {e}", out_dir.display()),
            field: Some(out_dir.display().to_string()),
            details: Some(serde_json::json!({
                "path": out_dir.display().to_string(),
                "io_kind": format!("{:?}", e.kind()),
            })),
            diagnostics: vec![],
            cause: vec![e.to_string()],
        })
    })?;
    let idx_parent = idx.parent().unwrap_or_else(|| Path::new("/"));
    Ok(idx_parent == out)
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
    writeln!(
        writer,
        "  index: {}",
        display_relative_to_cwd(&payload.index_path)
    )?;
    Ok(())
}

//! `influxdb3-plugin validate` — manifest + cross-file validation.
//!
//! Wraps [`influxdb3_plugin_sdk::validate::plugin_dir`] and, when
//! `--index <path>` is supplied, [`influxdb3_plugin_sdk::validate::plugin_dir_with_index`].
//!
//! Validator idiom: in `--output json` mode, stdout always emits a
//! single `{ "diagnostics": [...] }` document on both pass and fail
//! paths, including index read failures, index parse failures, and
//! per-entry index schema defects. Stderr stays empty in JSON mode.
//! The exit code redundantly signals the outcome (0 / 1).
//!
//! Truly unrecoverable runtime errors that cannot be shaped as a
//! diagnostic (e.g., I/O on the plugin directory itself) still bubble up
//! as [`anyhow::Error`] and follow the standard failure path.

use clap::Args as ClapArgs;
use influxdb3_plugin_schemas::Index;
use influxdb3_plugin_sdk::{SdkError, ValidationError, validate};
use std::path::PathBuf;

use crate::color::Stream;
use crate::output::{Env, OutputMode, RealEnv, json::ValidateOutput, resolve_output_mode};
use crate::style::Palette;

/// Parsed `validate` arguments.
#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Plugin directory to validate. Defaults to the current working
    /// directory.
    #[arg(default_value = ".")]
    plugin_dir: PathBuf,

    /// Output format. Auto-detected from stdout's TTY status and `CI`
    /// when omitted.
    #[arg(long, value_enum)]
    output: Option<OutputMode>,

    /// Optional index JSON to check `(name, version)` uniqueness against.
    /// When omitted, uniqueness is not checked.
    #[arg(long)]
    index: Option<PathBuf>,
}

impl Args {
    /// Runs `validate` per the parsed args. Returns `Ok(())` only when
    /// the diagnostics array is empty; populated diagnostics surface as
    /// `Err(anyhow::Error)` after the JSON document is written so
    /// `main.rs`'s exit-code mapping fires (exit 1).
    pub(crate) fn run(self) -> anyhow::Result<()> {
        run_with_env(self, &RealEnv)
    }
}

fn run_with_env(args: Args, env: &dyn Env) -> anyhow::Result<()> {
    let mode = resolve_output_mode(args.output, env);
    // Diagnostics render to stdout (Task 4.1 stream routing). The summary
    // anyhow error goes to stderr via `main.rs`'s `eprintln!("{e:#}")`.
    let stdout_palette = Palette::for_stream(Stream::Stdout, mode, env, env.stdout_is_terminal());
    let outcome = run_validation(&args)?;

    render(&outcome, mode, stdout_palette)?;

    if outcome.diagnostics.is_empty() {
        return Ok(());
    }

    let inner = anyhow::anyhow!(
        "validation failed: {} diagnostic(s)",
        outcome.diagnostics.len()
    );
    match mode {
        // JSON mode: stdout already carries the diagnostics document, so
        // main.rs must keep stderr silent.
        OutputMode::Json => Err(crate::cli_error::CliError::silent(inner)),
        // Human mode: stderr carries the summary line.
        OutputMode::Human => Err(inner),
    }
}

/// Runs the SDK validation pipeline and converts the result to
/// [`ValidateOutput`]. Distinguishes validation failures (collected into
/// the diagnostics array) from runtime errors (returned as
/// `Err(anyhow::Error)` so the standard failure path renders them on
/// stderr).
fn run_validation(args: &Args) -> anyhow::Result<ValidateOutput> {
    let result = match &args.index {
        Some(index_path) => match std::fs::read_to_string(index_path) {
            Ok(raw) => match Index::parse_json(&raw) {
                Ok(index) => validate::plugin_dir_with_index(&args.plugin_dir, &index),
                Err(schema_errors) => Err(SdkError::from(schema_errors)),
            },
            Err(io_err) => Err(SdkError::ValidationErrors(vec![
                ValidationError::IndexReadFailed {
                    path: index_path.clone(),
                    message: io_err.to_string(),
                },
            ])),
        },
        None => validate::plugin_dir(&args.plugin_dir),
    };

    match result {
        Ok(_manifest) => Ok(ValidateOutput {
            diagnostics: Vec::new(),
        }),
        Err(SdkError::ValidationErrors(errs)) => Ok(ValidateOutput {
            diagnostics: errs
                .iter()
                .map(crate::diag_render::diagnostic_from)
                .collect(),
        }),
        Err(other) => Err(other.into()),
    }
}

fn render(
    outcome: &ValidateOutput,
    mode: OutputMode,
    stdout_palette: Palette,
) -> anyhow::Result<()> {
    match mode {
        OutputMode::Human => {
            render_human(outcome, stdout_palette, &mut std::io::stdout())?;
        }
        OutputMode::Json => render_json(outcome, &mut std::io::stdout())?,
    }
    Ok(())
}

fn render_human(
    outcome: &ValidateOutput,
    palette: Palette,
    writer: &mut impl std::io::Write,
) -> std::io::Result<()> {
    if outcome.diagnostics.is_empty() {
        let ok = palette.success.render();
        let ok_reset = palette.success.render_reset();
        writeln!(writer, "{ok}validation passed: 0 diagnostics{ok_reset}")?;
    } else {
        crate::diag_render::render_human(&outcome.diagnostics, palette, writer)?;
    }
    Ok(())
}

fn render_json(outcome: &ValidateOutput, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
    serde_json::to_writer_pretty(&mut *writer, outcome)?;
    writeln!(writer)?;
    Ok(())
}

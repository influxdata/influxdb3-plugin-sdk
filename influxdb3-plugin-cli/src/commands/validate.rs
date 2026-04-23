//! `influxdb3-plugin validate` — manifest + cross-file validation.
//!
//! Wraps [`influxdb3_plugin_sdk::validate::plugin_dir`] and, when
//! `--index <path>` is supplied, [`influxdb3_plugin_sdk::validate::plugin_dir_with_index`].
//!
//! Per Spec 2 § S2-15 validator idiom: in `--output json` mode, stdout
//! emits a single `{ "diagnostics": [...] }` document on BOTH pass and
//! fail paths. Empty array signals a clean pass; populated array signals
//! failure. The exit code redundantly signals the outcome (0 / 1) per
//! S2-18.
//!
//! Runtime errors that aren't validation diagnostics (I/O permission
//! failures, malformed `--index` JSON, etc.) bubble up as
//! [`anyhow::Error`] and follow the standard failure path: empty stdout,
//! human-readable error on stderr, exit 1.

use clap::Args as ClapArgs;
use influxdb3_plugin_schemas::Index;
use influxdb3_plugin_sdk::{SdkError, ValidationError, validate};
use std::path::PathBuf;

use crate::output::{
    Env, OutputMode, RealEnv,
    json::{Diagnostic, ValidateOutput},
    resolve_output_mode,
};

/// Parsed `validate` arguments.
#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Plugin directory to validate. Defaults to the current working
    /// directory.
    #[arg(default_value = ".")]
    plugin_dir: PathBuf,

    /// Output format. Auto-detected from stdout's TTY status and `CI`
    /// when omitted (Spec 2 § S2-14).
    #[arg(long, value_enum)]
    output: Option<OutputMode>,

    /// Optional index JSON to check `(name, version)` uniqueness against
    /// (S2-2). When omitted, uniqueness is not checked.
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
    let outcome = run_validation(&args)?;

    render(&outcome, mode)?;

    if outcome.diagnostics.is_empty() {
        return Ok(());
    }

    let inner = anyhow::anyhow!(
        "validation failed: {} diagnostic(s)",
        outcome.diagnostics.len()
    );
    match mode {
        // JSON mode: stdout already carries the diagnostics document per
        // S2-15 validator idiom. main.rs must keep stderr silent.
        OutputMode::Json => Err(crate::cli_error::CliError::silent(inner)),
        // Human mode: stderr carries the summary line, same as today.
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
        Some(index_path) => {
            let index_raw = std::fs::read_to_string(index_path).map_err(|e| {
                anyhow::anyhow!("failed to read --index {}: {e}", index_path.display())
            })?;
            let index = Index::parse_json(&index_raw).map_err(|e| {
                anyhow::anyhow!(
                    "failed to parse --index {} as a registry index: {e}",
                    index_path.display()
                )
            })?;
            validate::plugin_dir_with_index(&args.plugin_dir, &index)
        }
        None => validate::plugin_dir(&args.plugin_dir),
    };

    match result {
        Ok(_manifest) => Ok(ValidateOutput {
            diagnostics: Vec::new(),
        }),
        Err(SdkError::ValidationErrors(errs)) => Ok(ValidateOutput {
            diagnostics: errs.iter().map(diagnostic_from).collect(),
        }),
        Err(other) => Err(other.into()),
    }
}

fn diagnostic_from(err: &ValidationError) -> Diagnostic {
    let variant = err.variant_name();
    let message = err.to_string();
    let field = match err {
        ValidationError::SchemaReported(reported) => {
            let p = reported.path.as_str();
            if p.is_empty() {
                None
            } else {
                Some(p.to_owned())
            }
        }
        ValidationError::MissingRequiredFile { file } => Some(file.clone()),
        ValidationError::PythonParse { .. }
        | ValidationError::TriggerNotImplemented { .. }
        | ValidationError::AsyncTriggerFn { .. } => Some("__init__.py".to_owned()),
        ValidationError::NameVersionConflict { name, version } => Some(format!("{name}@{version}")),
        // `ValidationError` is `#[non_exhaustive]`; future variants
        // surface with `variant_name` + `Display` only until the CLI
        // grows explicit handling for them.
        _ => None,
    };
    Diagnostic {
        variant,
        message,
        field,
    }
}

fn render(outcome: &ValidateOutput, mode: OutputMode) -> anyhow::Result<()> {
    match mode {
        OutputMode::Human => render_human(outcome, &mut std::io::stdout())?,
        OutputMode::Json => render_json(outcome, &mut std::io::stdout())?,
    }
    Ok(())
}

fn render_human(outcome: &ValidateOutput, writer: &mut impl std::io::Write) -> std::io::Result<()> {
    if outcome.diagnostics.is_empty() {
        writeln!(writer, "validation passed: 0 diagnostics")?;
    } else {
        writeln!(
            writer,
            "validation failed: {} diagnostic(s)",
            outcome.diagnostics.len()
        )?;
        for (i, d) in outcome.diagnostics.iter().enumerate() {
            match &d.field {
                Some(field) => {
                    writeln!(
                        writer,
                        "  {}. [{}] {}: {}",
                        i + 1,
                        d.variant,
                        field,
                        d.message
                    )?;
                }
                None => {
                    writeln!(writer, "  {}. [{}] {}", i + 1, d.variant, d.message)?;
                }
            }
        }
    }
    Ok(())
}

fn render_json(outcome: &ValidateOutput, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
    serde_json::to_writer_pretty(&mut *writer, outcome)?;
    writeln!(writer)?;
    Ok(())
}

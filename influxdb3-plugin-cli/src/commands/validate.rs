//! `influxdb3-plugin validate` — manifest + cross-file validation.
//!
//! Wraps [`influxdb3_plugin_sdk::validate::plugin_dir`] and, when
//! `--index <path>` is supplied, [`influxdb3_plugin_sdk::validate::plugin_dir_with_index`].
//!
//! Envelope idiom: in JSON mode, stdout always emits a single envelope
//! document. Success: `{"status":"ok","result":{}}`. Failure:
//! `{"status":"error","error":{"code":"validate::failed",...,"diagnostics":[...]}}`.
//! Human mode renders the same error tree via `render_human_error` in
//! `main.rs`.

use clap::Args as ClapArgs;
use influxdb3_plugin_schemas::Index;
use influxdb3_plugin_sdk::{SdkError, ValidationError, validate};
use std::io::Write;
use std::path::PathBuf;

use crate::cli_error::CliError;
use crate::color::Stream;
use crate::output::error_mapping::{ErrorContext, json_error_from_sdk, json_error_from_validation};
use crate::output::json::{JsonError, ValidateResult, write_envelope_ok};
use crate::output::{Env, OutputMode, RealEnv, resolve_output_mode};
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
    /// Runs `validate` per the parsed args.
    pub(crate) fn run(self) -> anyhow::Result<()> {
        run_with_env(self, &RealEnv)
    }
}

fn run_with_env(args: Args, env: &dyn Env) -> anyhow::Result<()> {
    let mode = resolve_output_mode(args.output, env);
    let stdout_palette = Palette::for_stream(Stream::Stdout, mode, env, env.stdout_is_terminal());
    let result = run_validation(&args);
    match (mode, result) {
        (OutputMode::Json, Ok(())) => {
            write_envelope_ok(&mut std::io::stdout(), ValidateResult {})?;
            Ok(())
        }
        (OutputMode::Human, Ok(())) => {
            let ok = stdout_palette.success.render();
            let ok_reset = stdout_palette.success.render_reset();
            writeln!(
                std::io::stdout(),
                "{ok}validation passed: 0 diagnostics{ok_reset}"
            )?;
            Ok(())
        }
        (_, Err(SdkError::ValidationErrors(errs))) => {
            let je = JsonError {
                code: "validate::failed".into(),
                message: format!("{} validation diagnostic(s)", errs.len()),
                field: None,
                details: None,
                diagnostics: errs.iter().map(json_error_from_validation).collect(),
                cause: vec![],
            };
            Err(CliError::runtime(je))
        }
        (_, Err(other)) => {
            let je = json_error_from_sdk(&other, ErrorContext::Validate);
            Err(CliError::runtime(je))
        }
    }
}

/// Runs the SDK validation pipeline and returns `Ok(())` on success or
/// `Err(SdkError)` on any validation / runtime failure. The caller
/// converts the error into the envelope shape.
fn run_validation(args: &Args) -> Result<(), SdkError> {
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
    result.map(|_manifest| ())
}

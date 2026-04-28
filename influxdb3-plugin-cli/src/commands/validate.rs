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
use influxdb3_plugin_sdk::{SdkError, validate};
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
    // Read and parse --index before validation so CLI owns the read-failure
    // diagnostic (SDK no longer has IndexReadFailed).
    let parsed_index = match &args.index {
        Some(index_path) => match std::fs::read_to_string(index_path) {
            Ok(raw) => match Index::parse_json(&raw) {
                Ok(index) => Some(index),
                Err(schema_errors) => {
                    return Err(CliError::runtime(json_error_from_sdk(
                        &SdkError::from(schema_errors),
                        ErrorContext::Validate,
                    )));
                }
            },
            Err(io_err) => {
                let diag = JsonError {
                    code: "validate::index_read_failed".into(),
                    message: format!(
                        "failed to read --index {}: {}",
                        index_path.display(),
                        io_err
                    ),
                    field: Some(index_path.display().to_string()),
                    details: Some(serde_json::json!({
                        "path": index_path.display().to_string(),
                        "io_message": io_err.to_string(),
                    })),
                    diagnostics: vec![],
                    cause: vec![io_err.to_string()],
                };
                let je = JsonError {
                    code: "validate::failed".into(),
                    message: "1 validation diagnostic(s)".into(),
                    field: None,
                    details: None,
                    diagnostics: vec![diag],
                    cause: vec![],
                };
                return Err(CliError::runtime(je));
            }
        },
        None => None,
    };

    let result = run_validation(&args.plugin_dir, parsed_index.as_ref());
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

fn run_validation(
    plugin_dir: &std::path::Path,
    index: Option<&Index>,
) -> Result<(), SdkError> {
    let result = match index {
        Some(idx) => validate::plugin_dir_with_index(plugin_dir, idx),
        None => validate::plugin_dir(plugin_dir),
    };
    result.map(|_manifest| ())
}

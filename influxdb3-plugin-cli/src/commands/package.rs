//! `influxdb3-plugin package` — validate, archive, hash, derive index.
//!
//! Wraps [`influxdb3_plugin_sdk::package::package_plugin`]. CLI-side
//! responsibilities:
//!
//! - Read + parse `--index` (read-only; the file is never touched on disk).
//! - Reject when `--out`'s canonical form equals the directory holding
//!   `--index` — a safety rail that prevents the derived-index write from
//!   overwriting the input. Check fires before any output bytes are written.
//! - Serialize the SDK's derived index via [`Index::to_canonical_json`]
//!   and write to `<out>/index.json` plus the artifact bytes to
//!   `<out>/<name>-<version>.tar.gz`.
//! - Render the result: JSON envelope on stdout for JSON mode; human-readable
//!   text for human mode. Failures are structured `CliError` with `JsonError`
//!   payloads.

use clap::Args as ClapArgs;
use influxdb3_plugin_schemas::Index;
use influxdb3_plugin_sdk::{SdkError, ValidationError, package};
use std::path::{Path, PathBuf};

use crate::cli_error::CliError;
use crate::color::Stream;
use crate::output::error_mapping::{ErrorContext, json_error_from_sdk, json_error_from_validation};
use crate::output::json::{JsonError, PackageOutput, write_envelope_ok};
use crate::output::{Env, OutputMode, RealEnv, resolve_output_mode};
use crate::style::Palette;

/// Parsed `package` arguments.
#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Plugin directory to package. Defaults to the current working
    /// directory.
    #[arg(default_value = ".")]
    plugin_dir: PathBuf,

    /// Output format. Auto-detected from stdout's TTY status and `CI`
    /// when omitted.
    #[arg(long, value_enum)]
    output: Option<OutputMode>,

    /// Input registry index (read-only). The derived index (input + new
    /// entry appended) is written to `--out/index.json`.
    #[arg(long)]
    index: PathBuf,

    /// Output directory. Receives the derived `index.json` and the new
    /// `<name>-<version>.tar.gz` artifact. Created if missing. Must NOT
    /// resolve to the directory containing `--index`.
    #[arg(long)]
    out: PathBuf,
}

impl Args {
    pub(crate) fn run(self) -> anyhow::Result<()> {
        run_with_env(self, &RealEnv)
    }
}

fn run_with_env(args: Args, env: &dyn Env) -> anyhow::Result<()> {
    let mode = resolve_output_mode(args.output, env);
    let stdout_palette = Palette::for_stream(Stream::Stdout, mode, env, env.stdout_is_terminal());

    // Read + parse the input index before creating --out so we don't
    // leave an empty scratch dir on parse failure.
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
    let input_index = Index::parse_json(&index_raw).map_err(|schema_errors| {
        let diagnostics: Vec<JsonError> = schema_errors
            .into_iter()
            .map(|reported| json_error_from_validation(&ValidationError::SchemaReported(reported)))
            .collect();
        CliError::runtime(JsonError {
            code: "package::index_parse_failed".into(),
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

    // Create --out directory. Path-equivalence check must fire before
    // any output write.
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
                 they must be disjoint (Spec 2 § S2-12)",
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

    // Package the plugin.
    let outcome = match package::package_plugin(&args.plugin_dir, input_index) {
        Ok(o) => o,
        Err(SdkError::AlreadyPublished { ref name, ref version, ref existing_versions }) => {
            let msg = format!(
                "plugin ({name:?}, {version:?}) already exists in the target index; \
                 existing versions: {existing_versions:?}. \
                 Increment version in manifest.toml or run `yank` instead."
            );
            return Err(anyhow::anyhow!("{msg}"));
        }
        Err(SdkError::CanonicalCollision { ref name, ref canonical, ref existing }) => {
            let msg = format!(
                "canonical collision: plugin name {name:?} conflicts with existing \
                 entries sharing canonical form {canonical:?}: {existing:?}. \
                 Rename to one of the existing spellings or choose a distinct name."
            );
            return Err(anyhow::anyhow!("{msg}"));
        }
        Err(SdkError::ValidationErrors(errs)) => {
            return Err(validation_errors_to_cli_error(errs));
        }
        Err(other) => {
            // Other SdkError → structured error.
            return Err(CliError::runtime(json_error_from_sdk(
                &other,
                ErrorContext::Package,
            )));
        }
    };

    let artifact_filename = format!(
        "{}-{}.tar.gz",
        outcome.new_entry.name.as_str(),
        outcome.new_entry.version,
    );
    let artifact_path = args.out.join(&artifact_filename);
    let derived_index_path = args.out.join("index.json");

    // Canonical JSON serialization failure.
    let derived_index_json = outcome.derived_index.to_canonical_json().map_err(|e| {
        let sdk_err = SdkError::from(e);
        CliError::runtime(json_error_from_sdk(&sdk_err, ErrorContext::Package))
    })?;

    // Write artifact.
    std::fs::write(&artifact_path, &outcome.archive_bytes).map_err(|e| {
        CliError::runtime(JsonError {
            code: "io::write_failed".into(),
            message: format!("failed to write artifact {}: {e}", artifact_path.display()),
            field: Some(artifact_path.display().to_string()),
            details: Some(serde_json::json!({
                "path": artifact_path.display().to_string(),
                "io_kind": format!("{:?}", e.kind()),
            })),
            diagnostics: vec![],
            cause: vec![e.to_string()],
        })
    })?;

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

    let payload = PackageOutput {
        artifact_path: canonicalize_or_keep(&artifact_path),
        index_path: canonicalize_or_keep(&derived_index_path),
        hash: outcome.hash.as_str().to_owned(),
        new_entry_name: outcome.new_entry.name.as_str().to_owned(),
        new_entry_version: outcome.new_entry.version.to_string(),
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

/// Returns `true` when `out_dir` (canonical) equals the directory
/// containing `index_path` (canonical). Symlinks, trailing slashes,
/// `.` segments, and `..` segments collapse to the same result.
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

/// `canonicalize` for display purposes — falls back to the input path
/// when canonicalization fails (e.g., the file existed during the call
/// but rotated away under us). Used only on outputs we just wrote.
fn canonicalize_or_keep(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn render_human(
    payload: &PackageOutput,
    palette: Palette,
    writer: &mut impl std::io::Write,
) -> std::io::Result<()> {
    let ok = palette.success.render();
    let ok_reset = palette.success.render_reset();
    writeln!(
        writer,
        "{ok}Packaged {}@{}{ok_reset}",
        payload.new_entry_name, payload.new_entry_version
    )?;
    // Info lines remain plain — conventional tool output emphasizes only
    // the status header, so the paths/hash stay unstyled for readability.
    writeln!(writer, "  artifact: {}", payload.artifact_path.display())?;
    writeln!(writer, "  index:    {}", payload.index_path.display())?;
    writeln!(writer, "  hash:     {}", payload.hash)?;
    Ok(())
}

/// Converts SDK validation errors to a `CliError` with structured
fn validation_errors_to_cli_error(errs: Vec<ValidationError>) -> anyhow::Error {
    let je = JsonError {
        code: "validate::failed".into(),
        message: format!("{} validation error(s) found", errs.len()),
        field: None,
        details: None,
        diagnostics: errs.iter().map(json_error_from_validation).collect(),
        cause: vec![],
    };
    CliError::runtime(je)
}

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
//! - Render the result: a single JSON document on stdout for success;
//!   empty stdout + stderr error for failure.

use clap::Args as ClapArgs;
use influxdb3_plugin_schemas::Index;
use influxdb3_plugin_sdk::{SdkError, ValidationError, package};
use std::path::{Path, PathBuf};

use crate::color::Stream;
use crate::output::{Env, OutputMode, RealEnv, json::PackageOutput, resolve_output_mode};
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
    let stderr_palette = Palette::for_stream(Stream::Stderr, mode, env, env.stderr_is_terminal());

    // Read + parse the input index before creating --out so we don't
    // leave an empty scratch dir on parse failure.
    let index_raw = std::fs::read_to_string(&args.index)
        .map_err(|e| anyhow::anyhow!("failed to read --index {}: {e}", args.index.display()))?;
    let input_index = Index::parse_json(&index_raw).map_err(|e| {
        anyhow::anyhow!(
            "failed to parse --index {} as a registry index: {e}",
            args.index.display()
        )
    })?;

    // Path-equivalence check must fire before any output write. Both
    // paths need to exist for `canonicalize`, so create `--out` here even
    // if the check fails immediately after.
    std::fs::create_dir_all(&args.out)
        .map_err(|e| anyhow::anyhow!("failed to create --out {}: {e}", args.out.display()))?;
    if paths_overlap(&args.index, &args.out)? {
        return Err(crate::cli_error::CliError::usage(anyhow::anyhow!(
            "--out {} resolves to the directory containing --index {}; \
             they must be disjoint (Spec 2 § S2-12)",
            args.out.display(),
            args.index.display(),
        )));
    }

    let outcome = match package::package_plugin(&args.plugin_dir, input_index) {
        Ok(o) => o,
        Err(SdkError::ValidationErrors(errs)) => {
            return Err(validation_errors_to_anyhow(errs, mode, stderr_palette));
        }
        Err(other) => return Err(other.into()),
    };

    let artifact_filename = format!(
        "{}-{}.tar.gz",
        outcome.new_entry.name.as_str(),
        outcome.new_entry.version,
    );
    let artifact_path = args.out.join(&artifact_filename);
    let derived_index_path = args.out.join("index.json");

    let derived_index_json = outcome
        .derived_index
        .to_canonical_json()
        .map_err(SdkError::from)?;

    std::fs::write(&artifact_path, &outcome.archive_bytes).map_err(|e| {
        anyhow::anyhow!("failed to write artifact {}: {e}", artifact_path.display())
    })?;
    std::fs::write(&derived_index_path, &derived_index_json).map_err(|e| {
        anyhow::anyhow!(
            "failed to write derived index {}: {e}",
            derived_index_path.display()
        )
    })?;

    let payload = PackageOutput {
        artifact_path: canonicalize_or_keep(&artifact_path),
        index_path: canonicalize_or_keep(&derived_index_path),
        hash: outcome.hash.as_str().to_owned(),
        new_entry_name: outcome.new_entry.name.as_str().to_owned(),
        new_entry_version: outcome.new_entry.version.to_string(),
    };

    render(&payload, mode, stdout_palette)
}

/// Returns `true` when `out_dir` (canonical) equals the directory
/// containing `index_path` (canonical). Symlinks, trailing slashes,
/// `.` segments, and `..` segments collapse to the same result.
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

/// `canonicalize` for display purposes — falls back to the input path
/// when canonicalization fails (e.g., the file existed during the call
/// but rotated away under us). Used only on outputs we just wrote.
fn canonicalize_or_keep(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn render(
    payload: &PackageOutput,
    mode: OutputMode,
    stdout_palette: Palette,
) -> anyhow::Result<()> {
    match mode {
        OutputMode::Human => render_human(payload, stdout_palette, &mut std::io::stdout())?,
        OutputMode::Json => render_json(payload, &mut std::io::stdout())?,
    }
    Ok(())
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

fn render_json(payload: &PackageOutput, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
    serde_json::to_writer_pretty(&mut *writer, payload)?;
    writeln!(writer)?;
    Ok(())
}

fn validation_errors_to_anyhow(
    errs: Vec<ValidationError>,
    mode: OutputMode,
    stderr_palette: Palette,
) -> anyhow::Error {
    match mode {
        OutputMode::Human => {
            // Render the full list to stderr so authors see every error
            // in one pass. Use the same renderer as
            // `validate` for visual consistency; the stderr palette here
            // lets colorization flow through `main.rs`'s eprintln.
            let diagnostics: Vec<_> = errs
                .iter()
                .map(crate::diag_render::diagnostic_from)
                .collect();
            let mut buf = Vec::<u8>::new();
            let _ = crate::diag_render::render_human(&diagnostics, stderr_palette, &mut buf);
            let rendered = String::from_utf8(buf).unwrap_or_default();
            anyhow::anyhow!("{}", rendered.trim_end())
        }
        OutputMode::Json => {
            // For data-tool commands: stdout must stay empty; the
            // human-readable error line is written to stderr. Singular —
            // one line, not a multi-line diagnostic block
            // or a JSON document. We preserve today's summary shape to
            // keep JSON-mode consumers stable; human mode carries the
            // rich reporting.
            anyhow::anyhow!("{} validation error(s) found", errs.len())
        }
    }
}

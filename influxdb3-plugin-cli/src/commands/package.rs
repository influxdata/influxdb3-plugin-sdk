//! `influxdb3-plugin package` — validate, archive, hash, derive index.
//!
//! Wraps [`influxdb3_plugin_sdk::package::package_plugin`]. CLI-side
//! responsibilities:
//!
//! - Read + parse `--index` (S2-11: read-only; the file is never touched
//!   on disk).
//! - Reject when `--out`'s canonical form equals the directory holding
//!   `--index` (S2-12; safety rail for S2-11). The check fires before
//!   any output bytes are written.
//! - Serialize the SDK's derived index via [`Index::to_canonical_json`]
//!   and write to `<out>/index.json` plus the artifact bytes to
//!   `<out>/<name>-<version>.tar.gz`.
//! - Render the result via the data-tool idiom (single JSON document on
//!   stdout for success; empty stdout + stderr error for failure).

use clap::Args as ClapArgs;
use influxdb3_plugin_schemas::Index;
use influxdb3_plugin_sdk::{SdkError, package};
use std::path::{Path, PathBuf};

use crate::output::{Env, OutputMode, RealEnv, json::PackageOutput, resolve_output_mode};

/// Parsed `package` arguments.
#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Plugin directory to package. Defaults to the current working
    /// directory.
    #[arg(default_value = ".")]
    plugin_dir: PathBuf,

    /// Output format. Auto-detected from stdout's TTY status and `CI`
    /// when omitted (Spec 2 § S2-14).
    #[arg(long, value_enum)]
    output: Option<OutputMode>,

    /// Input registry index (read-only per S2-11). The derived index
    /// (input + new entry appended) is written to `--out/index.json`.
    #[arg(long)]
    index: PathBuf,

    /// Output directory. Receives the derived `index.json` and the new
    /// `<name>-<version>.tar.gz` artifact. Created if missing. Must NOT
    /// resolve to the directory containing `--index` (S2-12).
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

    // Read + parse the input index BEFORE creating --out so we don't
    // leave an empty scratch dir on parse failure.
    let index_raw = std::fs::read_to_string(&args.index)
        .map_err(|e| anyhow::anyhow!("failed to read --index {}: {e}", args.index.display()))?;
    let input_index = Index::parse_json(&index_raw).map_err(|e| {
        anyhow::anyhow!(
            "failed to parse --index {} as a registry index: {e}",
            args.index.display()
        )
    })?;

    // Path-equivalence check (S2-11/S2-12). Must fire BEFORE any output
    // write. We need both paths to exist for `canonicalize`, so create
    // `--out` here even if we'll fail the check immediately after.
    std::fs::create_dir_all(&args.out)
        .map_err(|e| anyhow::anyhow!("failed to create --out {}: {e}", args.out.display()))?;
    if paths_overlap(&args.index, &args.out)? {
        anyhow::bail!(
            "--out {} resolves to the directory containing --index {}; \
             they must be disjoint (Spec 2 § S2-12)",
            args.out.display(),
            args.index.display(),
        );
    }

    let outcome = package::package_plugin(&args.plugin_dir, input_index)?;

    // Materialize bytes.
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

    render(&payload, mode)
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

fn render(payload: &PackageOutput, mode: OutputMode) -> anyhow::Result<()> {
    match mode {
        OutputMode::Human => render_human(payload, &mut std::io::stdout())?,
        OutputMode::Json => render_json(payload, &mut std::io::stdout())?,
    }
    Ok(())
}

fn render_human(payload: &PackageOutput, writer: &mut impl std::io::Write) -> std::io::Result<()> {
    writeln!(
        writer,
        "Packaged {}@{}",
        payload.new_entry_name, payload.new_entry_version
    )?;
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

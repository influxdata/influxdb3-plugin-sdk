//! `influxdb3-plugin yank` — toggle the `yanked` flag on an index entry.
//!
//! Wraps [`influxdb3_plugin_sdk::mutate_index::yank`] /
//! [`influxdb3_plugin_sdk::mutate_index::unyank`] and carries the same
//! S2-11 / S2-12 input-immutability + non-overlap rails as `package`.
//!
//! # Idempotency
//!
//! Per Spec 2 § yank, re-yanking an already-yanked entry (or `--undo`-ing
//! a not-yanked entry) is a successful no-op. The SDK distinguishes the
//! two cases via [`YankOutcome`]; we surface that signal in `--output json`
//! as `"transitioned"` vs `"already_in_desired_state"` and in human mode
//! as a printed informational marker.

use clap::Args as ClapArgs;
use influxdb3_plugin_schemas::{Index, PluginName};
use influxdb3_plugin_sdk::{SdkError, mutate_index};
use semver::Version;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::output::{Env, OutputMode, RealEnv, json::YankOutput, resolve_output_mode};

/// Parsed `yank` arguments.
#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// `<name>@<version>` identifier of the entry to toggle.
    target: String,

    /// Output format. Auto-detected from stdout's TTY status and `CI`
    /// when omitted (Spec 2 § S2-14).
    #[arg(long, value_enum)]
    output: Option<OutputMode>,

    /// Input registry index (read-only per S2-11).
    #[arg(long)]
    index: PathBuf,

    /// Output directory. Receives the derived `index.json`. Created if
    /// missing. Must NOT resolve to the directory containing `--index`
    /// (S2-12).
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
    let (name, version) = parse_target(&args.target)?;

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
        anyhow::bail!(
            "--out {} resolves to the directory containing --index {}; \
             they must be disjoint (Spec 2 § S2-12)",
            args.out.display(),
            args.index.display(),
        );
    }

    let outcome = if args.undo {
        mutate_index::unyank(&mut index, name.as_str(), &version)?
    } else {
        mutate_index::yank(&mut index, name.as_str(), &version)?
    };
    let target_state = !args.undo;

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
        outcome: outcome_label(outcome),
        target_state,
        index_path: canonicalize_or_keep(&derived_index_path),
    };

    render(&payload, mode)
}

fn outcome_label(outcome: mutate_index::YankOutcome) -> &'static str {
    match outcome {
        mutate_index::YankOutcome::Transitioned => "transitioned",
        mutate_index::YankOutcome::AlreadyInDesiredState => "already_in_desired_state",
    }
}

/// Parses `<name>@<version>`. Both halves must be present; `name` is
/// validated via [`PluginName`] and `version` via SemVer 2.0.0.
fn parse_target(s: &str) -> anyhow::Result<(PluginName, Version)> {
    let (name_str, version_str) = s.split_once('@').ok_or_else(|| {
        anyhow::anyhow!(
            "target {s:?} must be in `<name>@<version>` form (e.g., `downsampler@1.2.0`)"
        )
    })?;
    let name = PluginName::from_str(name_str)
        .map_err(|e| anyhow::anyhow!("invalid plugin name {name_str:?}: {e}"))?;
    let version = Version::parse(version_str)
        .map_err(|e| anyhow::anyhow!("invalid SemVer version {version_str:?}: {e}"))?;
    Ok((name, version))
}

/// S2-12 helper — same shape as `commands::package`.
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

fn render(payload: &YankOutput, mode: OutputMode) -> anyhow::Result<()> {
    match mode {
        OutputMode::Human => render_human(payload, &mut std::io::stdout())?,
        OutputMode::Json => render_json(payload, &mut std::io::stdout())?,
    }
    Ok(())
}

fn render_human(payload: &YankOutput, writer: &mut impl std::io::Write) -> std::io::Result<()> {
    let action = if payload.target_state {
        "yank"
    } else {
        "unyank"
    };
    match payload.outcome {
        "transitioned" => writeln!(
            writer,
            "{action}ed {}@{} (yanked={})",
            payload.name, payload.version, payload.target_state
        )?,
        _ => writeln!(
            writer,
            "{}@{} already in desired state (yanked={}); no change",
            payload.name, payload.version, payload.target_state
        )?,
    }
    writeln!(writer, "  index: {}", payload.index_path.display())?;
    Ok(())
}

fn render_json(payload: &YankOutput, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
    serde_json::to_writer_pretty(&mut *writer, payload)?;
    writeln!(writer)?;
    Ok(())
}

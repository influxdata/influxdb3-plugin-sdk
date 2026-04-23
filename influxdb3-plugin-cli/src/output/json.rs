//! JSON-mode output schemas per command.
//!
//! Every struct here is part of `influxdb3-plugin-cli`'s semver-stable
//! contract (Spec 2 § S2-16). Adding fields is a minor bump; renaming,
//! removing, repurposing, or narrowing the type of an existing field is
//! a major bump. Consumers must ignore unknown fields.
//!
//! Per Spec 2 § S2-16, no `output_schema_version` field is embedded —
//! consumers pin via the crate's published version.

use serde::Serialize;
use std::path::PathBuf;

/// `--output json` payload emitted by `new` on success (data-tool idiom
/// per S2-15: stdout carries this single document; failure paths leave
/// stdout empty and write the error to stderr).
#[derive(Debug, Serialize)]
pub(crate) struct NewOutput {
    /// `"plugin"` for trigger templates, `"registry"` for the registry
    /// template. Stable string tag; consumers can pattern-match.
    pub kind: &'static str,
    /// The template identifier the user passed (`process_writes`,
    /// `process_scheduled_call`, `process_request`, or `registry`).
    pub template: &'static str,
    /// Absolute path of the directory the scaffold wrote into.
    pub target_dir: PathBuf,
    /// Plugin name written into `manifest.toml` for plugin templates.
    /// Omitted (per `serde(skip_serializing_if = ...)`) for registry
    /// templates, which carry no plugin name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Files the scaffold wrote, relative to `target_dir`. Order matches
    /// the SDK scaffold's documented write order.
    pub files_written: Vec<PathBuf>,
}

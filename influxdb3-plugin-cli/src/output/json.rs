//! JSON-mode output schemas per command.
//!
//! Every struct here is part of `influxdb3-plugin-cli`'s semver-stable
//! contract. Adding fields is a minor bump; renaming, removing, repurposing,
//! or narrowing the type of an existing field is a major bump. Consumers
//! must ignore unknown fields.
//!
//! No `output_schema_version` field is embedded — consumers pin via the
//! crate's published version.

use serde::Serialize;
use std::path::PathBuf;

/// `--output json` payload emitted by `validate` on both pass and fail: a
/// single document on stdout always, with `diagnostics` empty on a clean
/// pass and populated on failure.
#[derive(Debug, Serialize)]
pub(crate) struct ValidateOutput {
    /// Validation diagnostics, ordered as the SDK collected them. Empty
    /// on a clean pass.
    pub diagnostics: Vec<Diagnostic>,
}

/// One validation diagnostic. `variant` is a stable string tag drawn from
/// [`influxdb3_plugin_sdk::ValidationError::variant_name`]; consumers can
/// pattern-match on it. `message` is the variant's `Display` text.
/// `field` is the field path (e.g., `plugin.name`) or filename (e.g.,
/// `manifest.toml`) the error refers to, when applicable.
#[derive(Debug, Serialize)]
pub(crate) struct Diagnostic {
    pub variant: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

/// `--output json` payload emitted by `yank`.
/// `outcome` is the [`influxdb3_plugin_sdk::mutate_index::YankOutcome`]
/// rendered as `"transitioned"` or `"already_in_desired_state"`.
/// `target_state` is `true` after `yank`, `false` after `yank --undo`.
#[derive(Debug, Serialize)]
pub(crate) struct YankOutput {
    pub name: String,
    pub version: String,
    pub outcome: &'static str,
    pub target_state: bool,
    pub index_path: PathBuf,
}

/// `--output json` payload emitted by `package` on success. Carries the
/// absolute paths of the artifact + derived index, the artifact's SHA-256
/// hash, and the new entry's identity.
#[derive(Debug, Serialize)]
pub(crate) struct PackageOutput {
    pub artifact_path: PathBuf,
    pub index_path: PathBuf,
    pub hash: String,
    pub new_entry_name: String,
    pub new_entry_version: String,
}

/// `--output json` payload emitted by `new` on success: stdout carries this
/// single document; failure paths leave stdout empty and write the error
/// to stderr.
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
    /// Plugin name written into `manifest.toml` for plugin templates;
    /// omitted for registry templates, which carry no plugin name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Files the scaffold wrote, relative to `target_dir`. Order matches
    /// the SDK scaffold's documented write order.
    pub files_written: Vec<PathBuf>,
}

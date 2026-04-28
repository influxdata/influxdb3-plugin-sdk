//! Public embedding surface for the InfluxDB 3 plugin CLI.
//!
//! [`PluginConfig`] is a clap-derived, semver-stable type intended to be
//! mounted as a subcommand variant by a host binary (e.g. `influxdb_pro`).
//!
//! Schema-type re-exports route through this crate so embedding consumers
//! depend only on `influxdb3-plugin-cli`, preventing parser drift from a
//! parallel direct dependency on `influxdb3-plugin-schemas`.

// `tokio` is a bin-only dep (main.rs's `#[tokio::main]`); the lib surface
// itself awaits without spawning. `unused_crate_dependencies` fires on the
// lib target unless we acknowledge the dep here.
use tokio as _;

// Integration-test helpers used only in external `tests/*.rs` files; the
// lib's own test target sees them as declared dev-deps but never names them.
#[cfg(test)]
use assert_cmd as _;
#[cfg(test)]
use insta as _;
#[cfg(test)]
use pep508_rs as _;
#[cfg(test)]
use predicates as _;
#[cfg(test)]
use tempfile as _;
#[cfg(test)]
use toml as _;
#[cfg(test)]
use url as _;

pub use config::PluginConfig;

// Re-export schema types so embedders import them from `cli` rather than
// taking a parallel direct dependency on `schemas`. Includes the multi-error
// parsing types (`SchemaErrors`, `ReportedError`, `FieldPath`) returned by
// `Manifest::parse_toml` / `Index::parse_json`.
pub use influxdb3_plugin_schemas::{
    ArtifactHash, ArtifactsUrl, Dependencies, Description, FieldPath, Index, IndexEntry,
    IndexSchemaVersion, Manifest, ManifestSchemaVersion, PluginId, PluginMetadata, PluginName,
    PythonRequirement, ReportedError, SchemaError, SchemaErrors, TriggerType,
};

/// Crate-internal types exposed for the bin target (`main.rs`) only.
///
/// `#[doc(hidden)]` signals to downstream consumers that nothing here is
/// part of the semver-stable embedding surface. `main.rs` reaches in
/// to name [`crate::cli_error::CliErrorKind`] so it can classify errors for exit-code routing;
/// keeping this pathway out of the public surface preserves the
/// freedom to evolve `CliError`'s internals.
#[doc(hidden)]
pub mod __private {
    pub use crate::cli_error::{CliError, CliErrorKind};
    pub use crate::diag_render::render_human_error;
    pub use crate::output::error_mapping::json_error_from_clap;
    pub use crate::output::json::{JsonError, write_envelope_error};
    pub use crate::style::{Palette, stderr_error_palette};
}

mod cli_error;
mod color;
mod commands;
mod config;
mod diag_render;
mod exit;
mod output;
mod style;

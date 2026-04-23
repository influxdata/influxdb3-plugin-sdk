//! Public embedding surface for the InfluxDB 3 plugin CLI.
//!
//! [`PluginConfig`] is a clap-derived, semver-stable type. A future
//! phase-2 embedding of this CLI into `influxdb_pro` mounts `PluginConfig`
//! as a variant of the host's top-level `Command` enum.
//!
//! # Stability
//!
//! Semver-stable per the plugin SDK's Spec 2 Stability policy. Schema-type
//! re-exports route through this crate so phase-2 embedding consumers
//! depend only on `influxdb3-plugin-cli`, satisfying S2-10 and preventing
//! parser drift from a parallel direct dependency on
//! `influxdb3-plugin-schemas`.

// `tokio` is a bin-only dep (main.rs's `#[tokio::main]`); the lib surface
// itself awaits without spawning. `unused_crate_dependencies` fires on the
// lib target unless we acknowledge the dep here. Same pattern as the
// `schemas` crate's `proptest` guard.
use tokio as _;

// `assert_cmd` / `predicates` / `insta` / `tempfile` are integration-test
// helpers used only in external `tests/*.rs` files. The lib's own test
// target sees them as declared dev-deps but never names them.
#[cfg(test)]
use assert_cmd as _;
#[cfg(test)]
use insta as _;
#[cfg(test)]
use predicates as _;
#[cfg(test)]
use tempfile as _;
#[cfg(test)]
use toml as _;

pub use config::PluginConfig;

// Schema-type re-exports — phase-2 embedding consumers import from `cli`,
// never directly from `schemas`, satisfying S2-10. Includes the multi-error
// parsing types (`SchemaErrors`, `ReportedError`, `FieldPath`) introduced
// after the plan was first drafted: `Manifest::parse_toml` and
// `Index::parse_json` return `Result<_, SchemaErrors>`, so consumers
// handling parse results need them.
pub use influxdb3_plugin_schemas::{
    ArtifactHash, ArtifactsUrl, Dependencies, Description, FieldPath, Index, IndexEntry,
    IndexSchemaVersion, Manifest, ManifestSchemaVersion, PluginId, PluginMetadata, PluginName,
    PythonRequirement, ReportedError, SchemaError, SchemaErrors, TriggerType,
};

/// Crate-internal types exposed for the bin target (`main.rs`) only.
///
/// `#[doc(hidden)]` signals to downstream consumers that nothing here is
/// part of the semver-stable embedding surface (S2-4). `main.rs` reaches in
/// to name [`CliErrorKind`] so it can classify errors for exit-code routing
/// (S2-18); keeping this pathway out of the public surface preserves the
/// freedom to evolve `CliError`'s internals.
#[doc(hidden)]
pub mod __private {
    pub use crate::cli_error::{CliError, CliErrorKind};
}

mod cli_error;
mod color;
mod commands;
mod config;
mod exit;
mod output;

//! Schema types for InfluxDB 3 plugin manifests and indexes.
//!
//! This crate defines the canonical Rust types for parsing and serializing
//! plugin manifests (`manifest.toml`), registry indexes (`index.json`), and
//! the `(index_url, name, version)` plugin-identity tuple that ties them
//! together.
//!
//! The crate is consumed by:
//! - `influxdb3-plugin-sdk` — the author-side packaging library
//! - `influxdb3-plugin-cli` — the `influxdb3-plugin` binary's CLI surface
//! - the future database runtime — for install-time manifest parsing and
//!   resolve-time index reads
//!
//! # Stability
//!
//! This crate targets a semver-stable public API. Schema evolution follows
//! the rules in Spec 1's Schema Versioning Strategy: additive fields within
//! a schema major are minor bumps; breaking schema changes bump the crate's
//! major.
//!
//! The crate is currently unpublished pending the project's license
//! decision. The stability commitment above applies to the types defined
//! here and will be anchored at first publish.

// Dependency-usage guards.
//
// The workspace lint `unused_crate_dependencies = "deny"` fires on declared
// deps that are not referenced from this crate's source. Each guard below is
// removed as soon as the corresponding dep lands real usage in a later
// dispatch.
//
// Dev deps not yet used by tests:
#[cfg(test)]
use proptest as _;

mod error;
mod identity;
mod index;
mod manifest;

// Types are added in subsequent tasks; re-exports will be uncommented as each
// arrives.
pub use error::SchemaError;
pub use identity::{PluginId, PluginName};
pub use index::{ArtifactHash, ArtifactsUrl, Index, IndexEntry, IndexSchemaVersion};
pub use manifest::{
    Dependencies, Description, Manifest, ManifestSchemaVersion, PluginMetadata,
    PythonRequirement, TriggerType,
};

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
//! All three consumers depend on this crate through the published crates.io
//! version; schema evolution follows the rules in Spec 1's Schema Versioning
//! Strategy.

// Dependency-usage guards.
//
// The workspace lint `unused_crate_dependencies = "deny"` fires on declared
// deps that are not referenced from this crate's source. Each guard below is
// removed as soon as the corresponding dep lands real usage in a later
// dispatch.
//
// Runtime deps not yet used by real code:
use serde as _;                 // used once a type derives Serialize/Deserialize (D7+)
use unicode_normalization as _; // used once index.rs calls .nfc() in to_canonical_json (D12)

// Dev deps not yet used by tests:
#[cfg(test)]
use assert_matches as _;
#[cfg(test)]
use pretty_assertions as _;
#[cfg(test)]
use proptest as _;
#[cfg(test)]
use rstest as _;

mod error;
mod identity;
mod index;
mod manifest;

// Types are added in subsequent tasks; re-exports will be uncommented as each
// arrives.
pub use error::SchemaError;
// pub use identity::{PluginId, PluginName};
// pub use index::{ArtifactHash, ArtifactsUrl, Index, IndexEntry, IndexSchemaVersion};
// pub use manifest::{Dependencies, Description, Manifest, ManifestSchemaVersion, PluginMetadata, PythonRequirement, TriggerType};

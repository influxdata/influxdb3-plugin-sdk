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
// deps that are not referenced from this crate's source. Plan 1 adds code
// incrementally; each dep below is used once a later dispatch introduces the
// relevant module. Remove a guard as soon as real usage lands.
//
// Runtime deps:
use pep508_rs as _;            // used by manifest.rs (PythonRequirement validation)
use semver as _;               // used by manifest.rs, index.rs, error.rs
use serde as _;                // used by every module with a #[derive(Serialize|Deserialize)]
use serde_json as _;           // used by index.rs (parse_json + to_canonical_json)
// thiserror is used directly via #[derive(thiserror::Error)] in error.rs (SchemaError)
use toml as _;                 // used by manifest.rs (parse_toml)
use unicode_normalization as _; // used by index.rs (NFC normalization in to_canonical_json)
use url as _;                   // used by manifest.rs, index.rs, error.rs

// Dev deps used in integration tests and proptest harness:
#[cfg(test)]
use assert_matches as _;
#[cfg(test)]
use insta as _;
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

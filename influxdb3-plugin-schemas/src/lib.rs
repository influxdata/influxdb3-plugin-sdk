//! Schema types for InfluxDB 3 plugin manifests and indexes.
//!
//! Defines the canonical Rust types for parsing and serializing plugin
//! manifests (`manifest.toml`), registry indexes (`index.json`), and the
//! `(index_url, name, version)` plugin-identity tuple.
//!
//! Schema evolution: additive fields within a schema major are minor bumps;
//! breaking schema changes bump the crate's major.

// `arbitrary`, `bolero`, and `proptest` are used only in integration tests
// under `tests/`; these guards keep `unused_crate_dependencies` satisfied on
// the lib test build.
#[cfg(test)]
use arbitrary as _;
#[cfg(test)]
use bolero as _;
#[cfg(test)]
use proptest as _;
#[cfg(test)]
use proptest_derive as _;

mod error;
mod identity;
mod index;
mod manifest;
mod path;
mod raw;

pub use error::{ReportedError, SchemaError, SchemaErrors};
pub use identity::{PluginId, PluginName};
pub use index::{ArtifactHash, ArtifactsUrl, Index, IndexEntry, IndexSchemaVersion};
pub use manifest::{
    Dependencies, Description, Manifest, ManifestSchemaVersion, PluginMetadata, PythonRequirement,
    TriggerType,
};
pub use path::FieldPath;

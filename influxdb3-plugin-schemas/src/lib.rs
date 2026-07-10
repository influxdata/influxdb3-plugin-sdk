//! Schema types for InfluxDB 3 plugin manifests and indexes.
//!
//! Defines the canonical Rust types for parsing and serializing plugin
//! manifests (`manifest.toml`), registry indexes (`index.json`), and the
//! `(index_url, name, version)` plugin-identity tuple.
//!
//! Schema evolution: additive fields within a schema major are minor bumps;
//! breaking schema changes bump the crate's major.

// `proptest` is used only in the `tests/determinism.rs` integration test;
// this guard keeps `unused_crate_dependencies` satisfied on the lib test build.
#[cfg(test)]
use proptest as _;

mod error;
mod identity;
mod index;
mod index_query;
mod manifest;
mod path;
pub mod plugin_format;
mod raw;

pub use error::{IndexInsertError, ReportedError, SchemaError, SchemaErrors};
pub use identity::{IndexUrl, PluginId, PluginName};
pub use index::{ArtifactHash, ArtifactsUrl, Index, IndexEntry, IndexSchemaVersion, PublishedAt};
pub use index_query::{
    IndexInfo, IndexInfoQuery, IndexInfoResult, IndexSearchHit, IndexSearchQuery,
    IndexSearchResult, IndexVersionVisibility, IndexVisibilityReason,
};
pub use manifest::{
    Dependencies, Description, Manifest, ManifestSchemaVersion, PluginDependency, PluginMetadata,
    PythonRequirement, TriggerType,
};
pub use path::FieldPath;
pub use plugin_format::ValidationError;

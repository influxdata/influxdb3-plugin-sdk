//! Crate-private raw deserialization types for two-phase parsing.
//!
//! These structs accept any TOML/JSON document that conforms to the *shape*
//! of a manifest or index — field presence and field types, nothing else.
//! Structured validation (name regex, SemVer, URL scheme, PEP 508, etc.) runs
//! in a separate pass on the raw values, collecting errors into `SchemaErrors`
//! with field-path context. See `Manifest::parse_toml` and `Index::parse_json`.
//!
//! These types are not re-exported from the crate. The public API presents only
//! the validated `Manifest` / `Index` / `IndexEntry` types.
//!
//! Index-side raw types (`RawIndex`, `RawIndexEntry`) are still allowed
//! `dead_code` until Chunk 4 wires up `Index::parse_json`; removed there.

#[derive(Debug, serde::Deserialize)]
pub(crate) struct RawManifest {
    pub manifest_schema_version: String,
    pub plugin: RawPluginMetadata,
    pub dependencies: RawDependencies,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct RawPluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub triggers: Vec<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub documentation: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct RawDependencies {
    pub database_version: String,
    #[serde(default)]
    pub python: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct RawIndex {
    pub index_schema_version: String,
    pub artifacts_url: String,
    pub plugins: Vec<RawIndexEntry>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct RawIndexEntry {
    pub name: String,
    pub version: String,
    pub description: String,
    pub triggers: Vec<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub documentation: Option<String>,
    pub dependencies: RawDependencies,
    pub hash: String,
    #[serde(default)]
    pub yanked: bool,
}

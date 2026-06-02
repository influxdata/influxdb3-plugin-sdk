//! Crate-private raw deserialization types for two-phase parsing.
//!
//! Phase 1 accepts documents with the expected container shape and raw values.
//! Phase 2 enforces required fields, precise scalar types, and semantic
//! validation (name regex, SemVer, URL scheme, PEP 508, etc.) while collecting
//! field-path-aware errors. See `Manifest::parse_toml` and `Index::parse_json`.
//!
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
    #[serde(default)]
    pub exclude: Vec<String>,
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
    #[serde(default)]
    pub published_at: Option<serde_json::Value>,
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

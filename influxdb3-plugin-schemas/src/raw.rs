//! Crate-private raw deserialization types for two-phase parsing.
//!
//! Phase 1 accepts any document that matches the *shape* (field presence and
//! types). Phase 2 runs structured validation (name regex, SemVer, URL scheme,
//! PEP 508, etc.) over the raw values and collects errors with field-path
//! context into `SchemaErrors` — this is what lets a parse report every defect
//! at once instead of stopping at the first. See `Manifest::parse_toml` and
//! `Index::parse_json`.
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

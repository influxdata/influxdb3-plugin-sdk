//! Index query primitives: search and info over parsed registry indexes.

use crate::{ArtifactHash, Dependencies, Description, PluginName, TriggerType};

/// Search query parameters for browsing an index.
#[derive(Debug, Clone, Default)]
pub struct IndexSearchQuery {
    pub query: Option<String>,
    pub trigger_type: Option<TriggerType>,
    pub database_version: Option<semver::Version>,
    pub include_yanked: bool,
    pub include_incompatible: bool,
}

/// Search result containing one hit per plugin.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IndexSearchResult {
    pub hits: Vec<IndexSearchHit>,
}

/// One search hit summarizing the latest matching version of a plugin.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IndexSearchHit {
    pub name: PluginName,
    pub version: semver::Version,
    pub description: Description,
    pub triggers: Vec<TriggerType>,
    pub visibility: IndexVersionVisibility,
}

/// Visibility state for a version in query results.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum IndexVersionVisibility {
    Visible,
    Hidden {
        reasons: Vec<IndexVisibilityReason>,
    },
}

/// Reason a version is hidden from default query results.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum IndexVisibilityReason {
    Yanked,
    IncompatibleDatabaseVersion {
        required: semver::VersionReq,
        actual: semver::Version,
    },
}

/// Info query parameters for inspecting a specific plugin.
#[derive(Debug, Clone)]
pub struct IndexInfoQuery {
    pub name: PluginName,
    pub version: Option<semver::Version>,
    pub database_version: Option<semver::Version>,
    pub include_yanked: bool,
    pub include_incompatible: bool,
}

/// Info result distinguishing found, absent, and filtered-out states.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum IndexInfoResult {
    Found(IndexInfo),
    NotFound {
        name: PluginName,
        version: Option<semver::Version>,
    },
    FilteredOut {
        name: PluginName,
        version: Option<semver::Version>,
        reasons: Vec<IndexVisibilityReason>,
    },
}

/// Full metadata for a single plugin version.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IndexInfo {
    pub name: PluginName,
    pub version: semver::Version,
    pub description: Description,
    pub triggers: Vec<TriggerType>,
    pub homepage: Option<url::Url>,
    pub repository: Option<url::Url>,
    pub documentation: Option<url::Url>,
    pub dependencies: Dependencies,
    pub hash: ArtifactHash,
    pub visibility: IndexVersionVisibility,
}

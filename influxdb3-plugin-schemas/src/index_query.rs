//! Index query primitives: search and info over parsed registry indexes.

use crate::{ArtifactHash, Dependencies, Description, IndexEntry, PluginName, TriggerType};

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

pub(crate) fn visibility_for(
    entry: &IndexEntry,
    database_version: Option<&semver::Version>,
) -> IndexVersionVisibility {
    let mut reasons = Vec::new();
    if entry.yanked {
        reasons.push(IndexVisibilityReason::Yanked);
    }
    if let Some(db_ver) = database_version {
        if !entry.dependencies.database_version.matches(db_ver) {
            reasons.push(IndexVisibilityReason::IncompatibleDatabaseVersion {
                required: entry.dependencies.database_version.clone(),
                actual: db_ver.clone(),
            });
        }
    }
    if reasons.is_empty() {
        IndexVersionVisibility::Visible
    } else {
        IndexVersionVisibility::Hidden { reasons }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ArtifactHash, ArtifactsUrl, Dependencies, Description, Index, IndexEntry,
        IndexSchemaVersion, PythonRequirement, TriggerType,
    };
    use assert_matches::assert_matches;
    use pretty_assertions::assert_eq;

    fn make_entry(name: &str, version: &str) -> IndexEntry {
        IndexEntry {
            name: name.parse().unwrap(),
            version: version.parse().unwrap(),
            description: Description::try_new("desc").unwrap(),
            triggers: vec![TriggerType::ProcessWrites],
            homepage: None,
            repository: None,
            documentation: None,
            dependencies: Dependencies {
                database_version: ">=3.0.0".parse().unwrap(),
                python: vec![],
            },
            hash: ArtifactHash::try_new(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            yanked: false,
        }
    }

    fn make_index(entries: Vec<IndexEntry>) -> Index {
        Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: entries,
        }
    }

    // --- Visibility helper tests ---

    #[test]
    fn visibility_visible_when_not_yanked_and_compatible() {
        let entry = make_entry("foo", "1.0.0");
        let vis = visibility_for(&entry, Some(&"3.2.0".parse().unwrap()));
        assert_eq!(vis, IndexVersionVisibility::Visible);
    }

    #[test]
    fn visibility_hidden_when_yanked() {
        let mut entry = make_entry("foo", "1.0.0");
        entry.yanked = true;
        let vis = visibility_for(&entry, Some(&"3.2.0".parse().unwrap()));
        assert_matches!(&vis, IndexVersionVisibility::Hidden { reasons } => {
            assert_eq!(reasons.len(), 1);
            assert_matches!(&reasons[0], IndexVisibilityReason::Yanked);
        });
    }

    #[test]
    fn visibility_hidden_when_incompatible() {
        let mut entry = make_entry("foo", "1.0.0");
        entry.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let vis = visibility_for(&entry, Some(&"3.2.0".parse().unwrap()));
        assert_matches!(&vis, IndexVersionVisibility::Hidden { reasons } => {
            assert_eq!(reasons.len(), 1);
            assert_matches!(&reasons[0], IndexVisibilityReason::IncompatibleDatabaseVersion {
                required, actual
            } => {
                assert_eq!(required.to_string(), ">=4.0.0");
                assert_eq!(*actual, "3.2.0".parse::<semver::Version>().unwrap());
            });
        });
    }

    #[test]
    fn visibility_hidden_yanked_and_incompatible() {
        let mut entry = make_entry("foo", "1.0.0");
        entry.yanked = true;
        entry.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let vis = visibility_for(&entry, Some(&"3.2.0".parse().unwrap()));
        assert_matches!(&vis, IndexVersionVisibility::Hidden { reasons } => {
            assert_eq!(reasons.len(), 2);
            assert_matches!(&reasons[0], IndexVisibilityReason::Yanked);
            assert_matches!(&reasons[1], IndexVisibilityReason::IncompatibleDatabaseVersion { .. });
        });
    }

    #[test]
    fn visibility_no_compat_check_without_db_version() {
        let mut entry = make_entry("foo", "1.0.0");
        entry.dependencies.database_version = ">=99.0.0".parse().unwrap();
        let vis = visibility_for(&entry, None);
        assert_eq!(vis, IndexVersionVisibility::Visible);
    }
}

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

fn info_from_entry(entry: &IndexEntry, visibility: IndexVersionVisibility) -> IndexInfo {
    IndexInfo {
        name: entry.name.clone(),
        version: entry.version.clone(),
        description: entry.description.clone(),
        triggers: entry.triggers.clone(),
        homepage: entry.homepage.clone(),
        repository: entry.repository.clone(),
        documentation: entry.documentation.clone(),
        dependencies: entry.dependencies.clone(),
        hash: entry.hash.clone(),
        visibility,
    }
}

impl crate::Index {
    pub fn search(&self, query: &IndexSearchQuery) -> IndexSearchResult {
        use std::collections::BTreeMap;

        let query_text = query
            .query
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        let query_lower = query_text.map(|s| s.to_ascii_lowercase());
        let query_canonical = query_text.map(crate::identity::canonical_name);

        // BTreeMap keyed by canonical name ensures output is sorted by canonical name ascending
        let mut groups: BTreeMap<String, Vec<(&IndexEntry, IndexVersionVisibility)>> =
            BTreeMap::new();

        for entry in &self.plugins {
            let vis = visibility_for(entry, query.database_version.as_ref());

            let excluded = matches!(&vis, IndexVersionVisibility::Hidden { reasons }
                if reasons.iter().any(|r| match r {
                    IndexVisibilityReason::Yanked => !query.include_yanked,
                    IndexVisibilityReason::IncompatibleDatabaseVersion { .. } => {
                        !query.include_incompatible
                    }
                })
            );
            if excluded {
                continue;
            }

            if let Some(ref q_lower) = query_lower {
                let name_lower = entry.name.as_str().to_ascii_lowercase();
                let canonical = entry.name.canonical();
                let desc_lower = entry.description.as_str().to_ascii_lowercase();
                let q_canon = query_canonical.as_deref().unwrap_or("");

                let matches = name_lower.contains(q_lower.as_str())
                    || canonical.contains(q_canon)
                    || desc_lower.contains(q_lower.as_str());
                if !matches {
                    continue;
                }
            }

            if let Some(ref trigger) = query.trigger_type {
                if !entry.triggers.contains(trigger) {
                    continue;
                }
            }

            groups
                .entry(entry.name.canonical())
                .or_default()
                .push((entry, vis));
        }

        let mut hits = Vec::with_capacity(groups.len());
        for (_canonical, mut candidates) in groups {
            // SemVer precedence descending; full version comparison as tiebreaker
            candidates.sort_by(|a, b| {
                b.0.version
                    .cmp_precedence(&a.0.version)
                    .then_with(|| b.0.version.cmp(&a.0.version))
            });
            let (entry, vis) = &candidates[0];
            hits.push(IndexSearchHit {
                name: entry.name.clone(),
                version: entry.version.clone(),
                description: entry.description.clone(),
                triggers: entry.triggers.clone(),
                visibility: vis.clone(),
            });
        }

        IndexSearchResult { hits }
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

    // --- Search tests ---

    #[test]
    fn search_empty_query_matches_all(/* spec 1 */) {
        let index = make_index(vec![
            make_entry("alpha", "1.0.0"),
            make_entry("beta", "2.0.0"),
        ]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 2);
    }

    #[test]
    fn search_whitespace_query_matches_all(/* spec 2 */) {
        let index = make_index(vec![
            make_entry("alpha", "1.0.0"),
            make_entry("beta", "2.0.0"),
        ]);
        let result = index.search(&IndexSearchQuery {
            query: Some("   ".into()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 2);
    }

    #[test]
    fn search_name_substring_match(/* spec 3 */) {
        let index = make_index(vec![
            make_entry("downsampler", "1.0.0"),
            make_entry("alerter", "1.0.0"),
        ]);
        let result = index.search(&IndexSearchQuery {
            query: Some("sample".into()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].name.as_str(), "downsampler");
    }

    #[test]
    fn search_description_substring_match(/* spec 4 */) {
        let mut entry = make_entry("notifier", "1.0.0");
        entry.description = Description::try_new("Fires on every WAL commit").unwrap();
        let index = make_index(vec![entry, make_entry("other", "1.0.0")]);
        let result = index.search(&IndexSearchQuery {
            query: Some("wal".into()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].name.as_str(), "notifier");
    }

    #[test]
    fn search_case_insensitive(/* spec 5 */) {
        let index = make_index(vec![make_entry("downsampler", "1.0.0")]);
        let result = index.search(&IndexSearchQuery {
            query: Some("DOWNSAMPLE".into()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].name.as_str(), "downsampler");
    }

    #[test]
    fn search_canonical_name_matching(/* spec 6 */) {
        // Hyphens in name, underscores in query
        let index = make_index(vec![make_entry("my-plugin", "1.0.0")]);
        let result = index.search(&IndexSearchQuery {
            query: Some("my_plugin".into()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].name.as_str(), "my-plugin");

        // Underscores in name, hyphens in query
        let index = make_index(vec![make_entry("my_plugin", "1.0.0")]);
        let result = index.search(&IndexSearchQuery {
            query: Some("my-plugin".into()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].name.as_str(), "my_plugin");
    }

    #[test]
    fn search_no_dependency_text_search(/* spec 7 */) {
        let mut entry = make_entry("downsampler", "1.0.0");
        entry.dependencies.python =
            vec![PythonRequirement::try_new("requests>=2.31").unwrap()];
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery {
            query: Some("requests".into()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 0);
    }

    #[test]
    fn search_no_url_hash_trigger_text_search(/* spec 8 */) {
        let mut entry = make_entry("downsampler", "1.0.0");
        entry.documentation =
            Some("https://docs.example.com/searchable".parse().unwrap());
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery {
            query: Some("searchable".into()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 0);
    }

    // --- Search filtering tests ---

    #[test]
    fn search_trigger_type_filter_includes(/* spec 9 */) {
        let index = make_index(vec![make_entry("writer", "1.0.0")]);
        let result = index.search(&IndexSearchQuery {
            trigger_type: Some(TriggerType::ProcessWrites),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
    }

    #[test]
    fn search_trigger_type_filter_excludes(/* spec 10 */) {
        let mut entry = make_entry("requester", "1.0.0");
        entry.triggers = vec![TriggerType::ProcessRequest];
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery {
            trigger_type: Some(TriggerType::ProcessWrites),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 0);
    }

    #[test]
    fn search_yanked_hidden_by_default(/* spec 11 */) {
        let mut entry = make_entry("obsolete", "1.0.0");
        entry.yanked = true;
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 0);
    }

    #[test]
    fn search_yanked_included_when_requested(/* spec 12 */) {
        let mut entry = make_entry("obsolete", "1.0.0");
        entry.yanked = true;
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery {
            include_yanked: true,
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_matches!(
            &result.hits[0].visibility,
            IndexVersionVisibility::Hidden { reasons }
                if reasons.len() == 1
                    && matches!(&reasons[0], IndexVisibilityReason::Yanked)
        );
    }

    #[test]
    fn search_incompatible_hidden_with_db_version(/* spec 13 */) {
        let mut entry = make_entry("future", "1.0.0");
        entry.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery {
            database_version: Some("3.2.0".parse().unwrap()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 0);
    }

    #[test]
    fn search_incompatible_included_when_requested(/* spec 14 */) {
        let mut entry = make_entry("future", "1.0.0");
        entry.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery {
            database_version: Some("3.2.0".parse().unwrap()),
            include_incompatible: true,
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_matches!(
            &result.hits[0].visibility,
            IndexVersionVisibility::Hidden { reasons }
                if reasons.len() == 1
                    && matches!(&reasons[0], IndexVisibilityReason::IncompatibleDatabaseVersion { .. })
        );
    }

    #[test]
    fn search_no_compat_filter_without_db_version(/* spec 15 */) {
        let mut entry = make_entry("future", "1.0.0");
        entry.dependencies.database_version = ">=99.0.0".parse().unwrap();
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].visibility, IndexVersionVisibility::Visible);
    }

    #[test]
    fn search_yanked_and_incompatible_reasons_accumulate(/* spec 16 */) {
        let mut entry = make_entry("doomed", "1.0.0");
        entry.yanked = true;
        entry.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery {
            database_version: Some("3.2.0".parse().unwrap()),
            include_yanked: true,
            include_incompatible: true,
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_matches!(
            &result.hits[0].visibility,
            IndexVersionVisibility::Hidden { reasons } => {
                assert_eq!(reasons.len(), 2);
                assert_matches!(&reasons[0], IndexVisibilityReason::Yanked);
                assert_matches!(&reasons[1], IndexVisibilityReason::IncompatibleDatabaseVersion { .. });
            }
        );
    }

    // --- Search version selection + ordering tests ---

    #[test]
    fn search_one_hit_per_plugin(/* spec 17 */) {
        let index = make_index(vec![
            make_entry("alpha", "1.0.0"),
            make_entry("alpha", "2.0.0"),
            make_entry("alpha", "3.0.0"),
        ]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
    }

    #[test]
    fn search_latest_visible_version_selected(/* spec 18 */) {
        let index = make_index(vec![
            make_entry("alpha", "1.0.0"),
            make_entry("alpha", "1.2.0"),
            make_entry("alpha", "2.0.0"),
        ]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].version, "2.0.0".parse::<semver::Version>().unwrap());
    }

    #[test]
    fn search_latest_yanked_skipped(/* spec 19 */) {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.yanked = true;
        let index = make_index(vec![make_entry("alpha", "1.2.0"), v2]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].version, "1.2.0".parse::<semver::Version>().unwrap());
    }

    #[test]
    fn search_latest_incompatible_skipped(/* spec 20 */) {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let index = make_index(vec![make_entry("alpha", "1.2.0"), v2]);
        let result = index.search(&IndexSearchQuery {
            database_version: Some("3.2.0".parse().unwrap()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].version, "1.2.0".parse::<semver::Version>().unwrap());
    }

    #[test]
    fn search_hidden_selected_when_included(/* spec 21 */) {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.yanked = true;
        let index = make_index(vec![make_entry("alpha", "1.2.0"), v2]);
        let result = index.search(&IndexSearchQuery {
            include_yanked: true,
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].version, "2.0.0".parse::<semver::Version>().unwrap());
        assert_matches!(
            &result.hits[0].visibility,
            IndexVersionVisibility::Hidden { reasons }
                if reasons.len() == 1
                    && matches!(&reasons[0], IndexVisibilityReason::Yanked)
        );
    }

    #[test]
    fn search_summary_from_selected_version(/* spec 22 */) {
        let mut v1 = make_entry("alpha", "1.0.0");
        v1.description = Description::try_new("old description").unwrap();
        v1.triggers = vec![TriggerType::ProcessRequest];
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.description = Description::try_new("new description").unwrap();
        v2.triggers = vec![TriggerType::ProcessWrites, TriggerType::ProcessScheduledCall];
        let index = make_index(vec![v1, v2]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].description.as_str(), "new description");
        assert_eq!(result.hits[0].triggers.len(), 2);
    }

    #[test]
    fn search_hits_sorted_by_canonical_name(/* spec 24 */) {
        let index = make_index(vec![
            make_entry("zebra", "1.0.0"),
            make_entry("alpha", "1.0.0"),
            make_entry("middle", "1.0.0"),
        ]);
        let result = index.search(&IndexSearchQuery::default());
        let names: Vec<&str> = result.hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn search_semver_precedence_for_selection(/* spec 25 */) {
        let index = make_index(vec![
            make_entry("alpha", "1.0.0-alpha"),
            make_entry("alpha", "1.0.0"),
        ]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].version, "1.0.0".parse::<semver::Version>().unwrap());
    }

    #[test]
    fn search_build_metadata_deterministic(/* spec 26 */) {
        let index_a = make_index(vec![
            make_entry("alpha", "1.0.0+build.1"),
            make_entry("alpha", "1.0.0+build.2"),
        ]);
        let index_b = make_index(vec![
            make_entry("alpha", "1.0.0+build.2"),
            make_entry("alpha", "1.0.0+build.1"),
        ]);
        let result_a = index_a.search(&IndexSearchQuery::default());
        let result_b = index_b.search(&IndexSearchQuery::default());
        assert_eq!(result_a.hits[0].version, result_b.hits[0].version);
    }
}

//! Index query primitives: search and info over parsed registry indexes.

use crate::{
    ArtifactHash, Dependencies, Description, IndexEntry, PluginName, PublishedAt, TriggerType,
};

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
    pub published_at: PublishedAt,
    pub description: Description,
    pub triggers: Vec<TriggerType>,
    pub visibility: IndexVersionVisibility,
}

/// Visibility state for a version in query results.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum IndexVersionVisibility {
    Visible,
    Hidden { reasons: Vec<IndexVisibilityReason> },
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
    Found(Box<IndexInfo>),
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
    pub published_at: PublishedAt,
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
    if let Some(db_ver) =
        database_version.filter(|v| !entry.dependencies.database_version.matches(v))
    {
        reasons.push(IndexVisibilityReason::IncompatibleDatabaseVersion {
            required: entry.dependencies.database_version.clone(),
            actual: db_ver.clone(),
        });
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
        published_at: entry.published_at.clone(),
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

            if matches!(&query.trigger_type, Some(t) if !entry.triggers.contains(t)) {
                continue;
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
                published_at: entry.published_at.clone(),
                description: entry.description.clone(),
                triggers: entry.triggers.clone(),
                visibility: vis.clone(),
            });
        }

        IndexSearchResult { hits }
    }

    pub fn info(&self, query: &IndexInfoQuery) -> IndexInfoResult {
        let query_canonical = query.name.canonical();

        // Exact-version inspection: always returns Found if the entry exists
        if let Some(ref requested_version) = query.version {
            let found = self
                .plugins
                .iter()
                .find(|e| e.name.canonical() == query_canonical && e.version == *requested_version);
            return match found {
                Some(entry) => {
                    let vis = visibility_for(entry, query.database_version.as_ref());
                    IndexInfoResult::Found(Box::new(info_from_entry(entry, vis)))
                }
                None => IndexInfoResult::NotFound {
                    name: query.name.clone(),
                    version: Some(requested_version.clone()),
                },
            };
        }

        // No version specified: collect all entries for this plugin
        let candidates: Vec<(&IndexEntry, IndexVersionVisibility)> = self
            .plugins
            .iter()
            .filter(|e| e.name.canonical() == query_canonical)
            .map(|e| {
                let vis = visibility_for(e, query.database_version.as_ref());
                (e, vis)
            })
            .collect();

        if candidates.is_empty() {
            return IndexInfoResult::NotFound {
                name: query.name.clone(),
                version: None,
            };
        }

        // Partition into selectable (visible or opted-in) vs excluded
        let (mut selectable, excluded): (Vec<_>, Vec<_>) =
            candidates.into_iter().partition(|(_, vis)| match vis {
                IndexVersionVisibility::Visible => true,
                IndexVersionVisibility::Hidden { reasons } => reasons.iter().all(|r| match r {
                    IndexVisibilityReason::Yanked => query.include_yanked,
                    IndexVisibilityReason::IncompatibleDatabaseVersion { .. } => {
                        query.include_incompatible
                    }
                }),
            });

        if selectable.is_empty() {
            // All versions hidden — collect distinct reason kinds
            let mut has_yanked = false;
            let mut incompat_reason: Option<IndexVisibilityReason> = None;
            for (_, vis) in &excluded {
                if let IndexVersionVisibility::Hidden { reasons } = vis {
                    for r in reasons {
                        match r {
                            IndexVisibilityReason::Yanked => has_yanked = true,
                            IndexVisibilityReason::IncompatibleDatabaseVersion { .. } => {
                                if incompat_reason.is_none() {
                                    incompat_reason = Some(r.clone());
                                }
                            }
                        }
                    }
                }
            }
            let mut reasons = Vec::new();
            if has_yanked {
                reasons.push(IndexVisibilityReason::Yanked);
            }
            if let Some(ir) = incompat_reason {
                reasons.push(ir);
            }
            return IndexInfoResult::FilteredOut {
                name: query.name.clone(),
                version: None,
                reasons,
            };
        }

        // Select latest version from selectable candidates
        selectable.sort_by(|a, b| {
            b.0.version
                .cmp_precedence(&a.0.version)
                .then_with(|| b.0.version.cmp(&a.0.version))
        });
        let (entry, vis) = &selectable[0];
        IndexInfoResult::Found(Box::new(info_from_entry(entry, vis.clone())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ArtifactHash, ArtifactsUrl, Dependencies, Description, Index, IndexEntry,
        IndexSchemaVersion, PublishedAt, PythonRequirement, TriggerType,
    };
    use assert_matches::assert_matches;
    use pretty_assertions::assert_eq;

    fn make_entry(name: &str, version: &str) -> IndexEntry {
        make_entry_with_published_at(name, version, "2026-04-29T18:45:12Z")
    }

    fn make_entry_with_published_at(name: &str, version: &str, published_at: &str) -> IndexEntry {
        IndexEntry {
            name: name.parse().unwrap(),
            version: version.parse().unwrap(),
            published_at: PublishedAt::try_new(published_at).unwrap(),
            description: Description::try_new("desc").unwrap(),
            triggers: vec![TriggerType::ProcessWrites],
            homepage: None,
            repository: None,
            documentation: None,
            dependencies: Dependencies {
                database_version: ">=3.0.0".parse().unwrap(),
                python: vec![],
                plugins: vec![],
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
            index_schema_version: IndexSchemaVersion::CURRENT,
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
    fn search_empty_query_matches_all() {
        let index = make_index(vec![
            make_entry("alpha", "1.0.0"),
            make_entry("beta", "2.0.0"),
        ]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 2);
    }

    #[test]
    fn search_whitespace_query_matches_all() {
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
    fn search_name_substring_match() {
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
    fn search_description_substring_match() {
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
    fn search_case_insensitive() {
        let index = make_index(vec![make_entry("downsampler", "1.0.0")]);
        let result = index.search(&IndexSearchQuery {
            query: Some("DOWNSAMPLE".into()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].name.as_str(), "downsampler");
    }

    #[test]
    fn search_canonical_name_matching() {
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
    fn search_no_dependency_text_search() {
        let mut entry = make_entry("downsampler", "1.0.0");
        entry.dependencies.python = vec![PythonRequirement::try_new("requests>=2.31").unwrap()];
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery {
            query: Some("requests".into()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 0);
    }

    #[test]
    fn search_no_url_hash_trigger_text_search() {
        let mut entry = make_entry("downsampler", "1.0.0");
        entry.documentation = Some("https://docs.example.com/searchable".parse().unwrap());
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery {
            query: Some("searchable".into()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 0);
    }

    // --- Search filtering tests ---

    #[test]
    fn search_trigger_type_filter_includes() {
        let index = make_index(vec![make_entry("writer", "1.0.0")]);
        let result = index.search(&IndexSearchQuery {
            trigger_type: Some(TriggerType::ProcessWrites),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
    }

    #[test]
    fn search_trigger_type_filter_excludes() {
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
    fn search_yanked_hidden_by_default() {
        let mut entry = make_entry("obsolete", "1.0.0");
        entry.yanked = true;
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 0);
    }

    #[test]
    fn search_yanked_included_when_requested() {
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
    fn search_incompatible_hidden_with_db_version() {
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
    fn search_incompatible_included_when_requested() {
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
    fn search_no_compat_filter_without_db_version() {
        let mut entry = make_entry("future", "1.0.0");
        entry.dependencies.database_version = ">=99.0.0".parse().unwrap();
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].visibility, IndexVersionVisibility::Visible);
    }

    #[test]
    fn search_yanked_and_incompatible_reasons_accumulate() {
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
    fn search_one_hit_per_plugin() {
        let index = make_index(vec![
            make_entry("alpha", "1.0.0"),
            make_entry("alpha", "2.0.0"),
            make_entry("alpha", "3.0.0"),
        ]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
    }

    #[test]
    fn search_latest_visible_version_selected() {
        let index = make_index(vec![
            make_entry("alpha", "1.0.0"),
            make_entry("alpha", "1.2.0"),
            make_entry("alpha", "2.0.0"),
        ]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
        assert_eq!(
            result.hits[0].version,
            "2.0.0".parse::<semver::Version>().unwrap()
        );
    }

    #[test]
    fn search_latest_yanked_skipped() {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.yanked = true;
        let index = make_index(vec![make_entry("alpha", "1.2.0"), v2]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
        assert_eq!(
            result.hits[0].version,
            "1.2.0".parse::<semver::Version>().unwrap()
        );
    }

    #[test]
    fn search_latest_incompatible_skipped() {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let index = make_index(vec![make_entry("alpha", "1.2.0"), v2]);
        let result = index.search(&IndexSearchQuery {
            database_version: Some("3.2.0".parse().unwrap()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_eq!(
            result.hits[0].version,
            "1.2.0".parse::<semver::Version>().unwrap()
        );
    }

    #[test]
    fn search_hidden_selected_when_included() {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.yanked = true;
        let index = make_index(vec![make_entry("alpha", "1.2.0"), v2]);
        let result = index.search(&IndexSearchQuery {
            include_yanked: true,
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_eq!(
            result.hits[0].version,
            "2.0.0".parse::<semver::Version>().unwrap()
        );
        assert_matches!(
            &result.hits[0].visibility,
            IndexVersionVisibility::Hidden { reasons }
                if reasons.len() == 1
                    && matches!(&reasons[0], IndexVisibilityReason::Yanked)
        );
    }

    #[test]
    fn search_summary_from_selected_version() {
        let mut v1 = make_entry("alpha", "1.0.0");
        v1.description = Description::try_new("old description").unwrap();
        v1.triggers = vec![TriggerType::ProcessRequest];
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.description = Description::try_new("new description").unwrap();
        v2.triggers = vec![
            TriggerType::ProcessWrites,
            TriggerType::ProcessScheduledCall,
        ];
        let index = make_index(vec![v1, v2]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].description.as_str(), "new description");
        assert_eq!(result.hits[0].triggers.len(), 2);
    }

    #[test]
    fn search_hit_exposes_published_at_from_selected_version() {
        let index = make_index(vec![
            make_entry_with_published_at("alpha", "1.0.0", "2026-04-29T18:45:12Z"),
            make_entry_with_published_at("alpha", "2.0.0", "2027-01-02T03:04:05Z"),
        ]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].version, semver::Version::new(2, 0, 0));
        assert_eq!(result.hits[0].published_at.as_str(), "2027-01-02T03:04:05Z");
    }

    #[test]
    fn search_hits_sorted_by_canonical_name() {
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
    fn search_semver_precedence_for_selection() {
        let index = make_index(vec![
            make_entry("alpha", "1.0.0-alpha"),
            make_entry("alpha", "1.0.0"),
        ]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
        assert_eq!(
            result.hits[0].version,
            "1.0.0".parse::<semver::Version>().unwrap()
        );
    }

    #[test]
    fn search_build_metadata_deterministic() {
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

    // --- Info tests ---

    #[test]
    fn info_selects_latest_visible() {
        let index = make_index(vec![
            make_entry("alpha", "1.0.0"),
            make_entry("alpha", "1.2.0"),
            make_entry("alpha", "2.0.0"),
        ]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: None,
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::Found(info) => {
            assert_eq!(info.version, "2.0.0".parse::<semver::Version>().unwrap());
        });
    }

    #[test]
    fn info_returns_single_version() {
        let index = make_index(vec![
            make_entry("alpha", "1.0.0"),
            make_entry("alpha", "2.0.0"),
        ]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: None,
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::Found(_));
    }

    #[test]
    fn info_skips_yanked_by_default() {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.yanked = true;
        let index = make_index(vec![make_entry("alpha", "1.0.0"), v2]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: None,
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::Found(info) => {
            assert_eq!(info.version, "1.0.0".parse::<semver::Version>().unwrap());
        });
    }

    #[test]
    fn info_skips_incompatible_by_default() {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let index = make_index(vec![make_entry("alpha", "1.0.0"), v2]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: None,
            database_version: Some("3.2.0".parse().unwrap()),
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::Found(info) => {
            assert_eq!(info.version, "1.0.0".parse::<semver::Version>().unwrap());
        });
    }

    #[test]
    fn info_includes_yanked_when_requested() {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.yanked = true;
        let index = make_index(vec![make_entry("alpha", "1.0.0"), v2]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: None,
            database_version: None,
            include_yanked: true,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::Found(info) => {
            assert_eq!(info.version, "2.0.0".parse::<semver::Version>().unwrap());
            assert_matches!(&info.visibility, IndexVersionVisibility::Hidden { reasons }
                if reasons.len() == 1
                    && matches!(&reasons[0], IndexVisibilityReason::Yanked)
            );
        });
    }

    #[test]
    fn info_includes_incompatible_when_requested() {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let index = make_index(vec![make_entry("alpha", "1.0.0"), v2]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: None,
            database_version: Some("3.2.0".parse().unwrap()),
            include_yanked: false,
            include_incompatible: true,
        });
        assert_matches!(&result, IndexInfoResult::Found(info) => {
            assert_eq!(info.version, "2.0.0".parse::<semver::Version>().unwrap());
            assert_matches!(&info.visibility, IndexVersionVisibility::Hidden { reasons }
                if reasons.len() == 1
                    && matches!(&reasons[0], IndexVisibilityReason::IncompatibleDatabaseVersion { .. })
            );
        });
    }

    #[test]
    fn info_no_compat_filter_without_db_version() {
        let mut entry = make_entry("alpha", "1.0.0");
        entry.dependencies.database_version = ">=99.0.0".parse().unwrap();
        let index = make_index(vec![entry]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: None,
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::Found(info) => {
            assert_eq!(info.visibility, IndexVersionVisibility::Visible);
        });
    }

    #[test]
    fn info_missing_name() {
        let index = make_index(vec![make_entry("alpha", "1.0.0")]);
        let result = index.info(&IndexInfoQuery {
            name: "nonexistent".parse().unwrap(),
            version: None,
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::NotFound { name, version } => {
            assert_eq!(name.as_str(), "nonexistent");
            assert!(version.is_none());
        });
    }

    #[test]
    fn info_all_yanked() {
        let mut v1 = make_entry("alpha", "1.0.0");
        v1.yanked = true;
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.yanked = true;
        let index = make_index(vec![v1, v2]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: None,
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::FilteredOut { name, version, reasons } => {
            assert_eq!(name.as_str(), "alpha");
            assert!(version.is_none());
            assert_eq!(reasons.len(), 1);
            assert_matches!(&reasons[0], IndexVisibilityReason::Yanked);
        });
    }

    #[test]
    fn info_all_incompatible() {
        let mut v1 = make_entry("alpha", "1.0.0");
        v1.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let index = make_index(vec![v1]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: None,
            database_version: Some("3.2.0".parse().unwrap()),
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::FilteredOut { name, version, reasons } => {
            assert_eq!(name.as_str(), "alpha");
            assert!(version.is_none());
            assert_eq!(reasons.len(), 1);
            assert_matches!(&reasons[0], IndexVisibilityReason::IncompatibleDatabaseVersion { .. });
        });
    }

    #[test]
    fn info_all_hidden_mixed_reasons() {
        let mut v1 = make_entry("alpha", "1.0.0");
        v1.yanked = true;
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let index = make_index(vec![v1, v2]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: None,
            database_version: Some("3.2.0".parse().unwrap()),
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::FilteredOut { reasons, .. } => {
            assert_eq!(reasons.len(), 2);
            assert!(reasons.iter().any(|r| matches!(r, IndexVisibilityReason::Yanked)));
            assert!(reasons.iter().any(|r| matches!(r, IndexVisibilityReason::IncompatibleDatabaseVersion { .. })));
        });
    }

    // --- Exact version info tests ---

    #[test]
    fn info_exact_version_visible() {
        let index = make_index(vec![
            make_entry("alpha", "1.0.0"),
            make_entry("alpha", "2.0.0"),
        ]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: Some("1.0.0".parse().unwrap()),
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::Found(info) => {
            assert_eq!(info.version, "1.0.0".parse::<semver::Version>().unwrap());
            assert_eq!(info.visibility, IndexVersionVisibility::Visible);
        });
    }

    #[test]
    fn info_exact_version_yanked() {
        let mut entry = make_entry("alpha", "1.0.0");
        entry.yanked = true;
        let index = make_index(vec![entry]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: Some("1.0.0".parse().unwrap()),
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::Found(info) => {
            assert_matches!(&info.visibility, IndexVersionVisibility::Hidden { reasons }
                if reasons.len() == 1
                    && matches!(&reasons[0], IndexVisibilityReason::Yanked)
            );
        });
    }

    #[test]
    fn info_exact_version_incompatible() {
        let mut entry = make_entry("alpha", "1.0.0");
        entry.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let index = make_index(vec![entry]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: Some("1.0.0".parse().unwrap()),
            database_version: Some("3.2.0".parse().unwrap()),
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::Found(info) => {
            assert_matches!(&info.visibility, IndexVersionVisibility::Hidden { reasons }
                if reasons.len() == 1
                    && matches!(&reasons[0], IndexVisibilityReason::IncompatibleDatabaseVersion {
                        required, actual
                    } if required.to_string() == ">=4.0.0"
                        && *actual == "3.2.0".parse::<semver::Version>().unwrap()
                    )
            );
        });
    }

    #[test]
    fn info_exact_version_yanked_and_incompatible() {
        let mut entry = make_entry("alpha", "1.0.0");
        entry.yanked = true;
        entry.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let index = make_index(vec![entry]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: Some("1.0.0".parse().unwrap()),
            database_version: Some("3.2.0".parse().unwrap()),
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::Found(info) => {
            assert_matches!(&info.visibility, IndexVersionVisibility::Hidden { reasons } => {
                assert_eq!(reasons.len(), 2);
                assert_matches!(&reasons[0], IndexVisibilityReason::Yanked);
                assert_matches!(&reasons[1], IndexVisibilityReason::IncompatibleDatabaseVersion { .. });
            });
        });
    }

    #[test]
    fn info_exact_version_missing() {
        let index = make_index(vec![make_entry("alpha", "1.0.0")]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: Some("9.9.9".parse().unwrap()),
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::NotFound { name, version } => {
            assert_eq!(name.as_str(), "alpha");
            assert_eq!(*version, Some("9.9.9".parse::<semver::Version>().unwrap()));
        });
    }

    #[test]
    fn info_exact_version_missing_plugin() {
        let index = make_index(vec![make_entry("alpha", "1.0.0")]);
        let result = index.info(&IndexInfoQuery {
            name: "nonexistent".parse().unwrap(),
            version: Some("1.0.0".parse().unwrap()),
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::NotFound { name, version } => {
            assert_eq!(name.as_str(), "nonexistent");
            assert_eq!(*version, Some("1.0.0".parse::<semver::Version>().unwrap()));
        });
    }

    // --- Result content tests ---

    #[test]
    fn info_full_metadata() {
        let mut entry = make_entry("downsampler", "1.2.0");
        entry.description = Description::try_new("Downsamples WAL data").unwrap();
        entry.triggers = vec![
            TriggerType::ProcessWrites,
            TriggerType::ProcessScheduledCall,
        ];
        entry.homepage = Some("https://example.com".parse().unwrap());
        entry.repository = Some("https://github.com/example/ds".parse().unwrap());
        entry.documentation = Some("https://docs.example.com/ds".parse().unwrap());
        entry.dependencies = Dependencies {
            database_version: ">=3.2.0,<4.0.0".parse().unwrap(),
            python: vec![PythonRequirement::try_new("requests>=2.31").unwrap()],
            plugins: vec![],
        };
        entry.hash = ArtifactHash::try_new(
            "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
        )
        .unwrap();
        let index = make_index(vec![entry]);
        let result = index.info(&IndexInfoQuery {
            name: "downsampler".parse().unwrap(),
            version: None,
            database_version: Some("3.5.0".parse().unwrap()),
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::Found(info) => {
            assert_eq!(info.name.as_str(), "downsampler");
            assert_eq!(info.version, "1.2.0".parse::<semver::Version>().unwrap());
            assert_eq!(info.description.as_str(), "Downsamples WAL data");
            assert_eq!(info.triggers.len(), 2);
            assert!(info.homepage.is_some());
            assert!(info.repository.is_some());
            assert!(info.documentation.is_some());
            assert_eq!(info.dependencies.database_version.to_string(), ">=3.2.0, <4.0.0");
            assert_eq!(info.dependencies.python.len(), 1);
            assert!(info.hash.as_str().starts_with("sha256:abcdef"));
            assert_eq!(info.visibility, IndexVersionVisibility::Visible);
        });
    }

    #[test]
    fn info_exposes_published_at() {
        let index = make_index(vec![make_entry_with_published_at(
            "alpha",
            "1.0.0",
            "2027-01-02T03:04:05Z",
        )]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: Some("1.0.0".parse().unwrap()),
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::Found(info) => {
            assert_eq!(info.published_at.as_str(), "2027-01-02T03:04:05Z");
        });
    }

    #[test]
    fn info_incompatible_reason_includes_versions() {
        let mut entry = make_entry("alpha", "1.0.0");
        entry.dependencies.database_version = ">=4.0.0".parse().unwrap();
        let index = make_index(vec![entry]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: Some("1.0.0".parse().unwrap()),
            database_version: Some("3.2.0".parse().unwrap()),
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::Found(info) => {
            assert_matches!(&info.visibility, IndexVersionVisibility::Hidden { reasons } => {
                assert_matches!(&reasons[0], IndexVisibilityReason::IncompatibleDatabaseVersion {
                    required, actual
                } => {
                    assert_eq!(required.to_string(), ">=4.0.0");
                    assert_eq!(*actual, "3.2.0".parse::<semver::Version>().unwrap());
                });
            });
        });
    }

    // --- Serialization tests ---

    #[test]
    fn search_result_serializes() {
        let index = make_index(vec![make_entry("alpha", "1.0.0")]);
        let result = index.search(&IndexSearchQuery::default());
        let json = serde_json::to_value(&result).unwrap();
        assert!(json["hits"].is_array());
        assert_eq!(json["hits"][0]["name"], "alpha");
        assert_eq!(json["hits"][0]["version"], "1.0.0");
        assert_eq!(json["hits"][0]["published_at"], "2026-04-29T18:45:12Z");
        assert!(json["hits"][0]["description"].is_string());
        assert!(json["hits"][0]["triggers"].is_array());
        assert_eq!(json["hits"][0]["visibility"], "Visible");
    }

    #[test]
    fn info_found_serializes() {
        let index = make_index(vec![make_entry("alpha", "1.0.0")]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: None,
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        let json = serde_json::to_value(&result).unwrap();
        let found = &json["Found"];
        assert_eq!(found["name"], "alpha");
        assert_eq!(found["version"], "1.0.0");
        assert_eq!(found["published_at"], "2026-04-29T18:45:12Z");
        assert!(found["description"].is_string());
        assert!(found["triggers"].is_array());
        assert!(found["dependencies"].is_object());
        assert!(found["hash"].is_string());
        assert_eq!(found["visibility"], "Visible");
    }

    #[test]
    fn info_not_found_serializes() {
        let index = make_index(vec![]);
        let result = index.info(&IndexInfoQuery {
            name: "missing".parse().unwrap(),
            version: Some("1.0.0".parse().unwrap()),
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        let json = serde_json::to_value(&result).unwrap();
        let not_found = &json["NotFound"];
        assert_eq!(not_found["name"], "missing");
        assert_eq!(not_found["version"], "1.0.0");
    }

    #[test]
    fn info_filtered_out_serializes() {
        let mut entry = make_entry("alpha", "1.0.0");
        entry.yanked = true;
        let index = make_index(vec![entry]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: None,
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        let json = serde_json::to_value(&result).unwrap();
        let filtered = &json["FilteredOut"];
        assert_eq!(filtered["name"], "alpha");
        assert!(filtered["version"].is_null());
        assert!(filtered["reasons"].is_array());
    }

    #[test]
    fn visibility_reasons_serialize() {
        let yanked = IndexVisibilityReason::Yanked;
        let incompat = IndexVisibilityReason::IncompatibleDatabaseVersion {
            required: ">=4.0.0".parse().unwrap(),
            actual: "3.2.0".parse().unwrap(),
        };
        let yanked_json = serde_json::to_value(&yanked).unwrap();
        let incompat_json = serde_json::to_value(&incompat).unwrap();
        assert_eq!(yanked_json, "Yanked");
        assert!(incompat_json["IncompatibleDatabaseVersion"].is_object());
        assert_eq!(
            incompat_json["IncompatibleDatabaseVersion"]["required"],
            ">=4.0.0"
        );
        assert_eq!(
            incompat_json["IncompatibleDatabaseVersion"]["actual"],
            "3.2.0"
        );
    }

    // --- Edge case tests ---

    #[test]
    fn search_empty_index() {
        let index = make_index(vec![]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 0);
    }

    #[test]
    fn info_empty_index() {
        let index = make_index(vec![]);
        let result = index.info(&IndexInfoQuery {
            name: "alpha".parse().unwrap(),
            version: None,
            database_version: None,
            include_yanked: false,
            include_incompatible: false,
        });
        assert_matches!(&result, IndexInfoResult::NotFound { .. });
    }

    #[test]
    fn search_only_hidden_not_shown() {
        let mut entry = make_entry("alpha", "1.0.0");
        entry.yanked = true;
        let index = make_index(vec![entry]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 0);
    }

    #[test]
    fn search_mixed_visible_hidden_uses_visible() {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.yanked = true;
        let index = make_index(vec![make_entry("alpha", "1.0.0"), v2]);
        let result = index.search(&IndexSearchQuery::default());
        assert_eq!(result.hits.len(), 1);
        assert_eq!(
            result.hits[0].version,
            "1.0.0".parse::<semver::Version>().unwrap()
        );
        assert_eq!(result.hits[0].visibility, IndexVersionVisibility::Visible);
    }

    #[test]
    fn search_mixed_with_include_flags() {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.yanked = true;
        let index = make_index(vec![make_entry("alpha", "1.0.0"), v2]);
        let result = index.search(&IndexSearchQuery {
            include_yanked: true,
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_eq!(
            result.hits[0].version,
            "2.0.0".parse::<semver::Version>().unwrap()
        );
    }

    #[test]
    fn search_text_match_on_hidden_visible_older_matches() {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.yanked = true;
        v2.description = Description::try_new("common in both versions").unwrap();
        let mut v1 = make_entry("alpha", "1.0.0");
        v1.description = Description::try_new("common desc").unwrap();
        let index = make_index(vec![v1, v2]);
        // Both versions match "common", but v2 is hidden (yanked).
        // Search excludes v2 before text matching; v1 matches and is selected.
        let result = index.search(&IndexSearchQuery {
            query: Some("common".into()),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_eq!(
            result.hits[0].version,
            "1.0.0".parse::<semver::Version>().unwrap()
        );
    }

    #[test]
    fn search_trigger_filter_before_grouping() {
        let mut v2 = make_entry("alpha", "2.0.0");
        v2.triggers = vec![TriggerType::ProcessRequest];
        let mut v1 = make_entry("alpha", "1.0.0");
        v1.triggers = vec![TriggerType::ProcessWrites];
        let index = make_index(vec![v1, v2]);
        let result = index.search(&IndexSearchQuery {
            trigger_type: Some(TriggerType::ProcessWrites),
            ..Default::default()
        });
        assert_eq!(result.hits.len(), 1);
        assert_eq!(
            result.hits[0].version,
            "1.0.0".parse::<semver::Version>().unwrap()
        );
        assert_eq!(result.hits[0].triggers, vec![TriggerType::ProcessWrites]);
    }
}

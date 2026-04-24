//! Index mutation helpers: append an entry, yank/unyank by `(name, version)`.
//!
//! Operates on a caller-owned `&mut Index`; no file I/O. Callers serialize
//! via [`Index::to_canonical_json`] and write bytes themselves.
//!
//! # Policy checks
//!
//! - [`add_entry`] enforces `(name, version)` immutability: duplicates
//!   return [`SdkError::AlreadyPublished`]. The caller must bump the version
//!   or [`yank`] the conflicting entry.
//! - [`yank`] / [`unyank`] return [`SdkError::EntryNotFound`] if the target
//!   entry is absent.
//!
//! Yank idempotency: yanking an already-yanked entry is a successful no-op
//! (likewise for unyank).

use influxdb3_plugin_schemas::{Index, IndexEntry};
use semver::Version;

use crate::SdkError;

/// Outcome of a [`yank`] or [`unyank`] call. Lets the CLI distinguish a
/// real state change from an idempotent no-op and emit a different message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YankOutcome {
    /// The entry's `yanked` flag changed.
    Transitioned,
    /// The entry was already in the desired state; no change.
    AlreadyInDesiredState,
}

/// Appends `entry` to `idx.plugins[]`, rejecting duplicate
/// `(canonical(name), version)`. Name matching uses
/// [`PluginName::canonical`] so `-`/`_` and case variants of the same
/// canonical form collide — mirrors the in-index duplicate check in
/// [`Index::from_raw_json`].
///
/// On conflict returns [`SdkError::AlreadyPublished`] listing every
/// existing version whose name shares the new entry's canonical form
/// (input order preserved). The CLI renders that into the actionable
/// "increment version or yank one of these" message.
pub fn add_entry(idx: &mut Index, entry: IndexEntry) -> Result<(), SdkError> {
    let new_version_str = entry.version.to_string();
    let new_canonical = entry.name.canonical();
    let existing_versions: Vec<String> = idx
        .plugins
        .iter()
        .filter(|e| e.name.canonical() == new_canonical)
        .map(|e| e.version.to_string())
        .collect();
    if existing_versions.iter().any(|v| v == &new_version_str) {
        return Err(SdkError::AlreadyPublished {
            name: entry.name.as_str().to_owned(),
            version: new_version_str,
            existing_versions,
        });
    }
    idx.plugins.push(entry);
    Ok(())
}

/// Sets `yanked = true` on the entry identified by `(name, version)`.
/// Idempotent; returns [`SdkError::EntryNotFound`] if no such entry exists.
pub fn yank(idx: &mut Index, name: &str, version: &Version) -> Result<YankOutcome, SdkError> {
    set_yanked(idx, name, version, true)
}

/// Sets `yanked = false` on the entry identified by `(name, version)`.
/// Idempotent; returns [`SdkError::EntryNotFound`] if no such entry exists.
pub fn unyank(idx: &mut Index, name: &str, version: &Version) -> Result<YankOutcome, SdkError> {
    set_yanked(idx, name, version, false)
}

fn set_yanked(
    idx: &mut Index,
    name: &str,
    version: &Version,
    target: bool,
) -> Result<YankOutcome, SdkError> {
    let entry = find_mut(idx, name, version)?;
    if entry.yanked == target {
        Ok(YankOutcome::AlreadyInDesiredState)
    } else {
        entry.yanked = target;
        Ok(YankOutcome::Transitioned)
    }
}

fn find_mut<'a>(
    idx: &'a mut Index,
    name: &str,
    version: &Version,
) -> Result<&'a mut IndexEntry, SdkError> {
    idx.plugins
        .iter_mut()
        .find(|e| e.name.as_str() == name && &e.version == version)
        .ok_or_else(|| SdkError::EntryNotFound {
            name: name.to_owned(),
            version: version.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use influxdb3_plugin_schemas::{
        ArtifactHash, ArtifactsUrl, Dependencies, Description, IndexEntry, IndexSchemaVersion,
        TriggerType,
    };

    fn empty_index() -> Index {
        Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![],
        }
    }

    fn make_entry(name: &str, version: Version) -> IndexEntry {
        IndexEntry {
            name: name.parse().unwrap(),
            version,
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

    #[test]
    fn add_entry_appends_to_empty() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap();
        assert_eq!(idx.plugins.len(), 1);
    }

    #[test]
    fn add_entry_rejects_duplicate_name_version() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap();
        let err = add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap_err();
        assert!(matches!(err, SdkError::AlreadyPublished { .. }));
        assert_eq!(idx.plugins.len(), 1);
    }

    /// Duplicate-rejection error must list every existing version of the
    /// plugin so the CLI can render actionable guidance.
    #[test]
    fn add_entry_duplicate_error_lists_every_existing_version() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap();
        add_entry(&mut idx, make_entry("a", Version::new(1, 1, 0))).unwrap();
        // Different name must not appear in the list.
        add_entry(&mut idx, make_entry("b", Version::new(2, 0, 0))).unwrap();

        let err = add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap_err();
        match err {
            SdkError::AlreadyPublished {
                name,
                version,
                existing_versions,
            } => {
                assert_eq!(name, "a");
                assert_eq!(version, "1.0.0");
                assert_eq!(
                    existing_versions,
                    vec!["1.0.0".to_owned(), "1.1.0".to_owned()],
                    "must enumerate every version of `a` in input order, omit other names"
                );
            }
            other => panic!("expected AlreadyPublished, got {other:?}"),
        }
    }

    #[test]
    fn add_entry_allows_same_name_different_version() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap();
        add_entry(&mut idx, make_entry("a", Version::new(1, 1, 0))).unwrap();
        assert_eq!(idx.plugins.len(), 2);
    }

    /// Canonical-form collision: `-`/`_` variants share a canonical
    /// name and must collide on the same version.
    #[test]
    fn add_entry_rejects_hyphen_underscore_canonical_collision() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("my-plugin", Version::new(1, 0, 0))).unwrap();
        let err =
            add_entry(&mut idx, make_entry("my_plugin", Version::new(1, 0, 0))).unwrap_err();
        match err {
            SdkError::AlreadyPublished {
                existing_versions, ..
            } => {
                assert_eq!(existing_versions, vec!["1.0.0".to_owned()]);
            }
            other => panic!("expected AlreadyPublished, got {other:?}"),
        }
        assert_eq!(idx.plugins.len(), 1, "no mutation on error");
    }

    /// Case differences share a canonical name.
    #[test]
    fn add_entry_rejects_case_canonical_collision() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("myplugin", Version::new(1, 0, 0))).unwrap();
        let err =
            add_entry(&mut idx, make_entry("MyPlugin", Version::new(1, 0, 0))).unwrap_err();
        assert!(matches!(err, SdkError::AlreadyPublished { .. }));
    }

    /// Canonical keying must NOT over-match on distinct versions.
    #[test]
    fn add_entry_allows_different_versions_of_same_canonical_name() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("my_plugin", Version::new(1, 0, 0))).unwrap();
        add_entry(&mut idx, make_entry("my-plugin", Version::new(1, 0, 1))).unwrap();
        assert_eq!(idx.plugins.len(), 2);
    }

    #[test]
    fn yank_sets_flag() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap();
        yank(&mut idx, "a", &Version::new(1, 0, 0)).unwrap();
        assert!(idx.plugins[0].yanked);
    }

    #[test]
    fn yank_is_idempotent() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap();
        yank(&mut idx, "a", &Version::new(1, 0, 0)).unwrap();
        yank(&mut idx, "a", &Version::new(1, 0, 0)).unwrap();
        assert!(idx.plugins[0].yanked);
    }

    /// Yank signals whether the call transitioned state or was a no-op —
    /// the CLI renders a different message in each case.
    #[test]
    fn yank_signals_transitioned_vs_already_in_desired_state() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap();

        let first = yank(&mut idx, "a", &Version::new(1, 0, 0)).unwrap();
        assert_eq!(first, YankOutcome::Transitioned);

        let second = yank(&mut idx, "a", &Version::new(1, 0, 0)).unwrap();
        assert_eq!(second, YankOutcome::AlreadyInDesiredState);
    }

    #[test]
    fn unyank_signals_transitioned_vs_already_in_desired_state() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap();

        // Entry starts not-yanked.
        let already = unyank(&mut idx, "a", &Version::new(1, 0, 0)).unwrap();
        assert_eq!(already, YankOutcome::AlreadyInDesiredState);

        yank(&mut idx, "a", &Version::new(1, 0, 0)).unwrap();
        let transitioned = unyank(&mut idx, "a", &Version::new(1, 0, 0)).unwrap();
        assert_eq!(transitioned, YankOutcome::Transitioned);
    }

    #[test]
    fn unyank_clears_flag() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap();
        yank(&mut idx, "a", &Version::new(1, 0, 0)).unwrap();
        unyank(&mut idx, "a", &Version::new(1, 0, 0)).unwrap();
        assert!(!idx.plugins[0].yanked);
    }

    #[test]
    fn yank_returns_entry_not_found_for_missing_name() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap();
        let err = yank(&mut idx, "zzz", &Version::new(1, 0, 0)).unwrap_err();
        assert!(matches!(err, SdkError::EntryNotFound { .. }));
    }

    #[test]
    fn yank_returns_entry_not_found_for_missing_version() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap();
        let err = yank(&mut idx, "a", &Version::new(2, 0, 0)).unwrap_err();
        assert!(matches!(err, SdkError::EntryNotFound { .. }));
    }

    #[test]
    fn unyank_returns_entry_not_found_on_missing() {
        let mut idx = empty_index();
        let err = unyank(&mut idx, "a", &Version::new(1, 0, 0)).unwrap_err();
        assert!(matches!(err, SdkError::EntryNotFound { .. }));
    }
}

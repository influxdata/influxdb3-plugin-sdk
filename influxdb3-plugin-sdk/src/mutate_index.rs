//! Index mutation helpers: append an entry, yank/unyank by `(name, version)`.
//!
//! The mutations operate on an owned [`Index`] (callers pass `&mut Index`).
//! No file I/O — callers serialize via [`Index::to_canonical_json`] and write
//! the bytes themselves. That separation lets these functions be pure,
//! deterministic, and testable without disk access.
//!
//! # Policy checks
//!
//! - [`add_entry`] enforces Spec 1 S1-4 / Spec 2 S2-2 immutability: if the
//!   target `(name, version)` already exists in `idx.plugins[]`, returns
//!   [`SdkError::AlreadyPublished`]. The caller must either bump the version
//!   or explicitly call [`yank`] to retract the conflicting entry.
//! - [`yank`] and [`unyank`] return [`SdkError::EntryNotFound`] if the
//!   target entry is absent. Callers who want "no-op on absent" can match
//!   on that variant and discard it.
//!
//! Yank idempotency: [`yank`] on an already-yanked entry is a successful
//! no-op (same for [`unyank`] on an already-unyanked entry). This matches
//! the Spec 2 `yank --undo` semantics.

use influxdb3_plugin_schemas::{Index, IndexEntry};
use semver::Version;

use crate::SdkError;

/// Appends `entry` to `idx.plugins[]`, rejecting duplicates per S2-2.
///
/// Returns [`SdkError::AlreadyPublished`] if `(name, version)` already exists.
pub fn add_entry(idx: &mut Index, entry: IndexEntry) -> Result<(), SdkError> {
    let exists = idx
        .plugins
        .iter()
        .any(|e| e.name.as_str() == entry.name.as_str() && e.version == entry.version);
    if exists {
        return Err(SdkError::AlreadyPublished {
            name: entry.name.as_str().to_owned(),
            version: entry.version.to_string(),
        });
    }
    idx.plugins.push(entry);
    Ok(())
}

/// Sets `yanked = true` on the entry identified by `(name, version)`.
///
/// Returns [`SdkError::EntryNotFound`] if no such entry exists. Idempotent:
/// calling on an already-yanked entry succeeds without modification.
pub fn yank(idx: &mut Index, name: &str, version: &Version) -> Result<(), SdkError> {
    let entry = find_mut(idx, name, version)?;
    entry.yanked = true;
    Ok(())
}

/// Sets `yanked = false` on the entry identified by `(name, version)`.
///
/// Returns [`SdkError::EntryNotFound`] if no such entry exists. Idempotent:
/// calling on an already-unyanked entry succeeds without modification.
pub fn unyank(idx: &mut Index, name: &str, version: &Version) -> Result<(), SdkError> {
    let entry = find_mut(idx, name, version)?;
    entry.yanked = false;
    Ok(())
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

    #[test]
    fn add_entry_allows_same_name_different_version() {
        let mut idx = empty_index();
        add_entry(&mut idx, make_entry("a", Version::new(1, 0, 0))).unwrap();
        add_entry(&mut idx, make_entry("a", Version::new(1, 1, 0))).unwrap();
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

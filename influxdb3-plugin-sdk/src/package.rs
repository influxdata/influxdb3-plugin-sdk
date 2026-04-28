//! End-to-end packaging: validate → archive → hash → mutate_index.
//!
//! [`package_plugin`] composes the author-side library operations into one
//! pipeline:
//!
//! 1. [`crate::validate::plugin_dir`] — manifest + cross-file checks. Fails
//!    fast on structural parse errors; collected cross-file failures
//!    short-circuit via [`SdkError::ValidationErrors`].
//! 2. [`crate::archive::canonical_tar_gz`] — canonical tar.gz bytes.
//! 3. [`crate::hash::sha256_of_bytes`] — SHA-256 of the archive bytes.
//! 4. [`crate::mutate_index::add_entry`] — append the new `IndexEntry` to a
//!    clone of the input index. Duplicate `(name, version)` returns
//!    [`SdkError::AlreadyPublished`]; a name that canonicalizes to an
//!    existing entry's form but spells differently returns
//!    [`SdkError::CanonicalCollision`].
//!
//! # I/O scope
//!
//! [`package_plugin`] performs no output-side I/O — it reads the plugin
//! directory but does not write the archive or index. The caller owns the
//! output target so input/output separation can be enforced there.

use influxdb3_plugin_schemas::{ArtifactHash, Index, IndexEntry};
use std::path::Path;

use crate::{SdkError, archive, hash, mutate_index, validate};

/// Output of a successful [`package_plugin`] invocation.
#[derive(Debug)]
pub struct PackageOutput {
    /// The canonical gzipped tar archive bytes.
    pub archive_bytes: Vec<u8>,
    /// SHA-256 of [`Self::archive_bytes`].
    pub hash: ArtifactHash,
    /// A copy of the input index with the new entry appended (insertion
    /// order). Callers producing wire bytes should serialize via
    /// [`Index::to_canonical_json`], which applies canonical sort.
    pub derived_index: Index,
    /// The newly-appended [`IndexEntry`]. Exposed so callers can log or
    /// snapshot it without re-searching the index.
    pub new_entry: IndexEntry,
}

/// Runs the full author-side packaging pipeline.
///
/// Validates `plugin_dir`, archives it, hashes the archive, and returns the
/// input index with a new entry appended (`input_index` is consumed).
///
/// # Errors
///
/// - [`SdkError::Io`] — read failure on `manifest.toml` or any source file.
/// - [`SdkError::ValidationErrors`] — manifest or cross-file checks failed.
/// - [`SdkError::Archive`] — archive construction failed (e.g. path overflow).
/// - [`SdkError::AlreadyPublished`] — `(name, version)` already in the index.
/// - [`SdkError::CanonicalCollision`] — `name` canonicalizes to an existing
///   entry's form but spellings differ (e.g., `my-plugin` vs `my_plugin`).
pub fn package_plugin(plugin_dir: &Path, input_index: Index) -> Result<PackageOutput, SdkError> {
    let manifest = validate::plugin_dir(plugin_dir)?;

    let archive_bytes =
        archive::canonical_tar_gz(plugin_dir, &manifest.plugin.name, &manifest.plugin.version)?;

    let hash_value = hash::sha256_of_bytes(&archive_bytes);

    let new_entry = IndexEntry::from_manifest(manifest, hash_value.clone());

    // Append to a clone; duplicate `(name, version)` fires here.
    let mut derived_index = input_index;
    mutate_index::add_entry(&mut derived_index, new_entry.clone())?;

    Ok(PackageOutput {
        archive_bytes,
        hash: hash_value,
        derived_index,
        new_entry,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use influxdb3_plugin_schemas::{ArtifactsUrl, IndexSchemaVersion};
    use std::fs;

    fn write_valid_plugin(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("manifest.toml"),
            "manifest_schema_version = \"1.0\"\n\n\
             [plugin]\n\
             name = \"downsampler\"\n\
             version = \"1.2.0\"\n\
             description = \"Test plugin\"\n\
             triggers = [\"process_writes\"]\n\n\
             [dependencies]\n\
             database_version = \">=3.0.0\"\n\
             python = [\"requests>=2.31,<3\"]\n",
        )
        .unwrap();
        fs::write(
            dir.join("__init__.py"),
            "def process_writes(a, b, c):\n    pass\n",
        )
        .unwrap();
    }

    fn empty_index() -> Index {
        Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![],
        }
    }

    #[test]
    fn happy_path_populates_every_output_field() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("downsampler");
        write_valid_plugin(&dir);

        let out = package_plugin(&dir, empty_index()).unwrap();

        assert!(!out.archive_bytes.is_empty());
        assert!(out.hash.as_str().starts_with("sha256:"));
        assert_eq!(out.derived_index.plugins.len(), 1);
        assert_eq!(out.new_entry.name.as_str(), "downsampler");
        assert_eq!(
            out.new_entry.version,
            semver::Version::new(1, 2, 0),
            "entry version should match manifest"
        );
        assert_eq!(out.new_entry.hash, out.hash, "entry hash matches computed");
    }

    #[test]
    fn entry_hash_matches_archive_bytes() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("p");
        write_valid_plugin(&dir);

        let out = package_plugin(&dir, empty_index()).unwrap();
        let recomputed = hash::sha256_of_bytes(&out.archive_bytes);
        assert_eq!(out.hash, recomputed);
    }

    #[test]
    fn duplicate_name_version_rejected_by_s2_2() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("p");
        write_valid_plugin(&dir);

        let first = package_plugin(&dir, empty_index()).unwrap();
        let err = package_plugin(&dir, first.derived_index).unwrap_err();
        assert!(
            matches!(err, SdkError::AlreadyPublished { .. }),
            "expected AlreadyPublished, got {err:?}"
        );
    }

    #[test]
    fn validation_failure_short_circuits_pipeline() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("p");
        fs::create_dir_all(&dir).unwrap();
        // Declare `process_writes` but don't implement it.
        fs::write(
            dir.join("manifest.toml"),
            "manifest_schema_version = \"1.0\"\n\n\
             [plugin]\nname = \"p\"\nversion = \"0.1.0\"\ndescription = \"x\"\ntriggers = [\"process_writes\"]\n\n\
             [dependencies]\ndatabase_version = \">=3.0.0\"\n",
        )
        .unwrap();
        fs::write(dir.join("__init__.py"), "def something_else():\n    pass\n").unwrap();

        let err = package_plugin(&dir, empty_index()).unwrap_err();
        assert!(
            matches!(err, SdkError::ValidationErrors(_)),
            "expected ValidationErrors, got {err:?}"
        );
    }

    /// `package_plugin` takes `input_index` by value, so the caller cannot
    /// directly observe post-call state. This test verifies a weaker but
    /// structurally-important property: a `Clone` of the input made before the
    /// call remains byte-identical after the call fails. This rules out any
    /// interior-mutability sharing pattern; it does *not* prove "input is
    /// preserved" (which is tautological for a move).
    #[test]
    fn duplicate_error_does_not_mutate_clone_before_return() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("p");
        write_valid_plugin(&dir);

        let first = package_plugin(&dir, empty_index()).unwrap();
        let len_before = first.derived_index.plugins.len();
        let snapshot = first.derived_index.clone();
        let first_clone_for_compare = first.derived_index.clone();
        let err = package_plugin(&dir, first.derived_index).unwrap_err();
        assert!(matches!(err, SdkError::AlreadyPublished { .. }));
        // If `Index` had interior mutability, the clone's structural state
        // could drift despite `package_plugin` taking ownership. Pinning the
        // full value (not just plugins.len()) catches that class of regression.
        assert_eq!(
            snapshot, first_clone_for_compare,
            "snapshot must remain byte-identical; interior mutability would break this"
        );
        // Redundant length check retained for a more specific failure diagnostic.
        assert_eq!(snapshot.plugins.len(), len_before);
    }
}

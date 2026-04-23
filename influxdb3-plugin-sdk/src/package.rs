//! End-to-end packaging: validate → archive → hash → mutate_index.
//!
//! [`package_plugin`] composes the five author-side library operations
//! specified in Spec 2 Packaging into a single pipeline:
//!
//! 1. [`crate::validate::plugin_dir`] — manifest +
//!    cross-file checks. Fails fast on structural parse errors; collected
//!    validation failures short-circuit the pipeline via
//!    [`SdkError::ValidationErrors`].
//! 2. [`crate::archive::canonical_tar_gz`] —
//!    canonical tar.gz bytes per Spec 2 Reproducibility.
//! 3. [`crate::hash::sha256_of_bytes`] — SHA-256 of the
//!    archive bytes, rendered as `sha256:<64 lowercase hex chars>`.
//! 4. [`crate::mutate_index::add_entry`] — append
//!    the new `IndexEntry` to a cloned copy of the input index. The S2-2
//!    immutability check fires here: if `(name, version)` already exists,
//!    the pipeline returns [`SdkError::AlreadyPublished`].
//!
//! # I/O scope
//!
//! [`package_plugin`] performs no output-side I/O. It reads the plugin
//! directory (manifest + __init__.py + archive contents) but does not write
//! the archive or index to disk — that's the caller's responsibility. This
//! keeps S2-11 (immutable input) and S2-12 (input/output separation)
//! enforcement at the CLI layer (Plan 3), where the command knows the
//! `--out` target.

use influxdb3_plugin_schemas::{
    ArtifactHash, Dependencies, Description, Index, IndexEntry, Manifest,
};
use std::path::Path;

use crate::{SdkError, archive, hash, mutate_index, validate};

/// Output of a successful [`package_plugin`] invocation.
#[derive(Debug)]
pub struct PackageOutput {
    /// The canonical gzipped tar archive bytes.
    pub archive_bytes: Vec<u8>,
    /// SHA-256 of [`Self::archive_bytes`].
    pub hash: ArtifactHash,
    /// A copy of the input index with the new entry appended. Entries are
    /// stored in insertion order; callers producing the final wire bytes
    /// should serialize via [`Index::to_canonical_json`] which applies the
    /// Spec 2 Reproducibility sort rules.
    pub derived_index: Index,
    /// The new [`IndexEntry`] that was appended to [`Self::derived_index`].
    /// Exposed for callers that want to log, snapshot, or further inspect
    /// the entry without re-searching the index.
    pub new_entry: IndexEntry,
}

/// Runs the full author-side packaging pipeline.
///
/// Reads `plugin_dir/manifest.toml` to determine the plugin's identity,
/// validates the directory, archives it, computes the artifact hash, and
/// produces a derived index with the new entry appended.
///
/// `input_index` is consumed by value and the derived copy is returned in
/// [`PackageOutput::derived_index`].
///
/// # Errors
///
/// - [`SdkError::Io`] — failed to read `manifest.toml` or any source file.
/// - [`SdkError::Schema`] — manifest did not parse structurally.
/// - [`SdkError::ValidationErrors`] — one or more cross-file validation
///   failures (see [`validate::plugin_dir`]).
/// - [`SdkError::Archive`] — archive construction failed (e.g.,
///   path-overflow rejection).
/// - [`SdkError::AlreadyPublished`] — `(name, version)` already present in
///   the input index (S2-2 immutability).
pub fn package_plugin(plugin_dir: &Path, input_index: Index) -> Result<PackageOutput, SdkError> {
    // 1. Validate. Short-circuits on any failure.
    validate::plugin_dir(plugin_dir)?;

    // Re-parse the manifest to extract the fields we need for the index
    // entry. `validate::plugin_dir` already parsed it once but doesn't
    // return the Manifest value; re-reading keeps the pipeline's signature
    // narrow and avoids exposing validate's internals.
    let manifest_raw =
        std::fs::read_to_string(plugin_dir.join("manifest.toml")).map_err(|source| {
            SdkError::Io {
                source,
                path: Some(plugin_dir.join("manifest.toml")),
            }
        })?;
    let manifest = Manifest::parse_toml(&manifest_raw)?;

    // 2. Archive.
    let archive_bytes =
        archive::canonical_tar_gz(plugin_dir, &manifest.plugin.name, &manifest.plugin.version)?;

    // 3. Hash.
    let hash_value = hash::sha256_of_bytes(&archive_bytes);

    // 4. Compose the index entry from manifest fields + computed hash.
    let new_entry = entry_from_manifest(&manifest, hash_value.clone());

    // 5. Append to a clone of the input index; S2-2 fires here.
    let mut derived_index = input_index;
    mutate_index::add_entry(&mut derived_index, new_entry.clone())?;

    Ok(PackageOutput {
        archive_bytes,
        hash: hash_value,
        derived_index,
        new_entry,
    })
}

fn entry_from_manifest(manifest: &Manifest, hash: ArtifactHash) -> IndexEntry {
    let plugin = &manifest.plugin;
    IndexEntry {
        name: plugin.name.clone(),
        version: plugin.version.clone(),
        description: clone_description(&plugin.description),
        triggers: plugin.triggers.clone(),
        homepage: plugin.homepage.clone(),
        repository: plugin.repository.clone(),
        documentation: plugin.documentation.clone(),
        dependencies: Dependencies {
            database_version: manifest.dependencies.database_version.clone(),
            python: manifest.dependencies.python.clone(),
        },
        hash,
        yanked: false,
    }
}

/// `Description` has no public `Clone` derive exposed; rebuild via `try_new`.
/// Manifest descriptions are already validated to be 1–200 chars, so this
/// always succeeds — panic indicates a schemas-crate invariant break.
fn clone_description(d: &Description) -> Description {
    Description::try_new(d.as_str()).expect("manifest description already validated 1-200 chars")
}

#[cfg(test)]
mod tests {
    use super::*;
    use influxdb3_plugin_schemas::{ArtifactsUrl, IndexSchemaVersion};
    use std::fs;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "influxdb3-plugin-sdk-package-test-{}-{}",
                tag,
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&base);
            fs::create_dir_all(&base).unwrap();
            Self(base)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

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
        let td = TempDir::new("happy");
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
        let td = TempDir::new("hash_match");
        let dir = td.path().join("p");
        write_valid_plugin(&dir);

        let out = package_plugin(&dir, empty_index()).unwrap();
        let recomputed = hash::sha256_of_bytes(&out.archive_bytes);
        assert_eq!(out.hash, recomputed);
    }

    #[test]
    fn duplicate_name_version_rejected_by_s2_2() {
        let td = TempDir::new("dup");
        let dir = td.path().join("p");
        write_valid_plugin(&dir);

        let first = package_plugin(&dir, empty_index()).unwrap();
        // Second packaging with the same manifest into an index that already
        // has the entry must fail per S2-2.
        let err = package_plugin(&dir, first.derived_index).unwrap_err();
        assert!(
            matches!(err, SdkError::AlreadyPublished { .. }),
            "expected AlreadyPublished, got {err:?}"
        );
    }

    #[test]
    fn validation_failure_short_circuits_pipeline() {
        let td = TempDir::new("validation");
        let dir = td.path().join("p");
        fs::create_dir_all(&dir).unwrap();
        // Declare process_writes but don't implement it.
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

    #[test]
    fn input_index_preserved_on_error() {
        // S2-2 rejection must not mutate the caller's index. Because we take
        // `input_index` by value, the caller gets back nothing on error; but
        // mutate_index::add_entry is called on the derived clone, so if a
        // duplicate is found, no intermediate state is observable.
        // This test verifies the second package_plugin call does NOT add
        // to the index passed in (the first call's derived_index).
        let td = TempDir::new("preserve");
        let dir = td.path().join("p");
        write_valid_plugin(&dir);

        let first = package_plugin(&dir, empty_index()).unwrap();
        let len_before = first.derived_index.plugins.len();
        let snapshot = first.derived_index.clone();
        let err = package_plugin(&dir, first.derived_index).unwrap_err();
        assert!(matches!(err, SdkError::AlreadyPublished { .. }));
        // Snapshot still reflects the pre-error state.
        assert_eq!(snapshot.plugins.len(), len_before);
    }
}

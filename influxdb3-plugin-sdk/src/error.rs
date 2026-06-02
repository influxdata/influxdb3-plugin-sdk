//! Error types for the SDK.
//!
//! Error layers:
//! - [`SdkError`] — crate-level error returned by every public function.
//! - [`ValidationFailure`] — the focused, validation-only error returned by
//!   the `validate::plugin_dir` wrapper. Either [`ValidationFailure::Invalid`]
//!   (a batch of [`ValidationError`]s) or [`ValidationFailure::Io`].
//! - [`ValidationReport`] — accumulator that collects [`ValidationError`]s
//!   during a pass and converts to a [`ValidationFailure`].
//!
//! The diagnostic type [`ValidationError`] lives in `influxdb3-plugin-schemas`
//! (the validation contract); it is re-imported here.

use influxdb3_plugin_schemas::{SchemaError, SchemaErrors, ValidationError};
use std::path::PathBuf;

/// Crate-level error type.
///
/// `#[non_exhaustive]`: variant additions are not breaking; field additions
/// to existing variants are — introduce a new variant and deprecate the old.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SdkError {
    #[error("I/O error{}", path_suffix(.path.as_ref()))]
    Io {
        #[source]
        source: std::io::Error,
        path: Option<PathBuf>,
    },

    #[error(transparent)]
    Schema(#[from] SchemaError),

    #[error("{} validation error(s) found", .0.len())]
    ValidationErrors(Vec<ValidationError>),

    #[error("archive construction failed: {message}")]
    Archive { message: String },

    #[error(
        "archive path {archive_path:?} exceeds ustar split-path limit ({limit} bytes); shorten file paths or the plugin name/version"
    )]
    PathTooLong { archive_path: String, limit: usize },

    #[error("hash computation failed")]
    Hash {
        #[source]
        source: std::io::Error,
    },

    /// Surfaces from [`crate::mutate_index::add_entry`] when
    /// `(name, version)` already exists in the input index. `existing_versions`
    /// enumerates every version of `name` already in the index (in index order)
    /// so the author can pick a higher version or `yank` the conflicting one.
    #[error(
        "plugin ({name:?}, {version:?}) already exists in the target index; \
         existing versions: {existing_versions:?}"
    )]
    AlreadyPublished {
        name: String,
        version: String,
        existing_versions: Vec<String>,
    },

    /// Surfaces from [`crate::mutate_index::add_entry`] when the incoming
    /// plugin name canonicalizes to an existing entry's canonical form but
    /// spellings differ (e.g., `my-plugin` vs `my_plugin`, or `MyPlugin`
    /// vs `myplugin`). `existing` enumerates every `(spelling, version)`
    /// already in the index under that canonical form, preserving index
    /// order.
    #[error(
        "canonical collision: plugin name {name:?} conflicts with existing \
         entries sharing canonical form {canonical:?}: {existing:?}"
    )]
    CanonicalCollision {
        name: String,
        canonical: String,
        existing: Vec<(String, semver::Version)>,
    },

    #[error("plugin ({name:?}, {version:?}) is not present in the target index")]
    EntryNotFound { name: String, version: String },

    /// A manifest `[plugin].exclude` entry is not a valid gitignore pattern.
    /// Surfaces from any command that performs source-file selection.
    #[error("invalid exclude pattern {pattern:?}: {message}")]
    InvalidExcludePattern { pattern: String, message: String },
}

impl SdkError {
    /// Stable tag per variant. The exhaustive match forces new variants to
    /// be registered with test fixtures (drift guard).
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::Io { .. } => "Io",
            Self::Schema(_) => "Schema",
            Self::ValidationErrors(_) => "ValidationErrors",
            Self::Archive { .. } => "Archive",
            Self::PathTooLong { .. } => "PathTooLong",
            Self::Hash { .. } => "Hash",
            Self::AlreadyPublished { .. } => "AlreadyPublished",
            Self::CanonicalCollision { .. } => "CanonicalCollision",
            Self::EntryNotFound { .. } => "EntryNotFound",
            Self::InvalidExcludePattern { .. } => "InvalidExcludePattern",
        }
    }
}

fn path_suffix(path: Option<&PathBuf>) -> String {
    match path {
        Some(p) => format!(" at {}", p.display()),
        None => String::new(),
    }
}

/// Adapts schemas-layer `SchemaErrors` into `SdkError::ValidationErrors`.
/// Each `ReportedError` becomes one [`ValidationError::SchemaReported`],
/// preserving field paths and inner `SchemaError` variants without lossy
/// string-squashing. This is the canonical `?`-conversion path for callers
/// of `Manifest::parse_toml` / `Index::parse_json`.
impl From<SchemaErrors> for SdkError {
    fn from(errors: SchemaErrors) -> Self {
        let diagnostics = errors
            .into_iter()
            .map(ValidationError::SchemaReported)
            .collect();
        SdkError::ValidationErrors(diagnostics)
    }
}

impl From<influxdb3_plugin_schemas::IndexInsertError> for SdkError {
    fn from(err: influxdb3_plugin_schemas::IndexInsertError) -> Self {
        use influxdb3_plugin_schemas::IndexInsertError;
        match err {
            IndexInsertError::Duplicate {
                name,
                version,
                existing_versions,
            } => SdkError::AlreadyPublished {
                name,
                version: version.to_string(),
                existing_versions: existing_versions.iter().map(|v| v.to_string()).collect(),
            },
            IndexInsertError::CanonicalCollision {
                name,
                canonical,
                existing,
            } => SdkError::CanonicalCollision {
                name,
                canonical,
                existing,
            },
            _ => unreachable!(
                "IndexInsertError has no other variants in the current schemas version"
            ),
        }
    }
}

impl From<crate::plugin_source_files::SelectError> for SdkError {
    fn from(err: crate::plugin_source_files::SelectError) -> Self {
        use crate::plugin_source_files::SelectError;
        match err {
            SelectError::InvalidExcludePattern { pattern, message } => {
                SdkError::InvalidExcludePattern { pattern, message }
            }
            SelectError::Io { source, path } => SdkError::Io { source, path },
            // Preserve historical archive behavior: walk errors → Archive.
            SelectError::Walk { message } => SdkError::Archive {
                message: format!("walkdir error: {message}"),
            },
        }
    }
}

/// The error returned by the `validate::plugin_dir` wrapper.
///
/// A focused, validation-only error type that separates validation failures
/// from the kitchen-sink [`SdkError`]. External consumers (including the
/// future runtime) interact with the pure `schemas::plugin_format` surface and
/// define their own I/O error types; they do not depend on this type.
///
/// `package.rs` stays ergonomic via [`From<ValidationFailure> for SdkError`],
/// so `?` propagation through `SdkError`-returning code paths works unchanged.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ValidationFailure {
    /// One or more validation rules failed; all diagnostics for a single pass.
    #[error("{} validation error(s) found", .0.len())]
    Invalid(Vec<ValidationError>),

    /// Non-`NotFound` I/O error reading the plugin directory or a file.
    #[error("I/O error{}", path_suffix(.path.as_ref()))]
    Io {
        #[source]
        source: std::io::Error,
        path: Option<PathBuf>,
    },

    /// A manifest `[plugin].exclude` entry is not a valid gitignore pattern.
    #[error("invalid exclude pattern {pattern:?}: {message}")]
    InvalidExcludePattern { pattern: String, message: String },
}

/// `Invalid → SdkError::ValidationErrors`, `Io → SdkError::Io`. Keeps
/// `SdkError`-returning callers (e.g. `package.rs`) ergonomic under `?`.
impl From<ValidationFailure> for SdkError {
    fn from(failure: ValidationFailure) -> Self {
        match failure {
            ValidationFailure::Invalid(errs) => SdkError::ValidationErrors(errs),
            ValidationFailure::Io { source, path } => SdkError::Io { source, path },
            ValidationFailure::InvalidExcludePattern { pattern, message } => {
                SdkError::InvalidExcludePattern { pattern, message }
            }
        }
    }
}

impl From<crate::plugin_source_files::SelectError> for ValidationFailure {
    fn from(err: crate::plugin_source_files::SelectError) -> Self {
        use crate::plugin_source_files::SelectError;
        match err {
            SelectError::InvalidExcludePattern { pattern, message } => {
                ValidationFailure::InvalidExcludePattern { pattern, message }
            }
            SelectError::Io { source, path } => ValidationFailure::Io { source, path },
            // validate has no Archive surface; fold walk errors into Io.
            SelectError::Walk { message } => ValidationFailure::Io {
                source: std::io::Error::other(message),
                path: None,
            },
        }
    }
}

/// Adapts schemas-layer `SchemaErrors` into `ValidationFailure::Invalid`.
/// Each `ReportedError` becomes one [`ValidationError::SchemaReported`].
/// Additive companion to [`From<SchemaErrors> for SdkError`]; covers the
/// `plugin_dir` wrapper boundary.
impl From<SchemaErrors> for ValidationFailure {
    fn from(errors: SchemaErrors) -> Self {
        let diagnostics = errors
            .into_iter()
            .map(ValidationError::SchemaReported)
            .collect();
        ValidationFailure::Invalid(diagnostics)
    }
}

/// Multi-error accumulator for validation passes.
///
/// Callers [`push`](Self::push) failures as they're encountered, then
/// [`into_result`](Self::into_result) to get `Ok(())` (empty) or
/// `Err(ValidationFailure::Invalid(errors))`. The type forces collect-all
/// semantics: the builder is the only way to return validation results.
#[derive(Debug, Default)]
pub struct ValidationReport {
    errors: Vec<ValidationError>,
}

impl ValidationReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, err: ValidationError) {
        self.errors.push(err);
    }

    pub fn extend<I>(&mut self, errors: I)
    where
        I: IntoIterator<Item = ValidationError>,
    {
        self.errors.extend(errors);
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// `Ok(())` if empty, else `Err(ValidationFailure::Invalid(errors))`.
    pub fn into_result(self) -> Result<(), ValidationFailure> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationFailure::Invalid(self.errors))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn every_sdk_variant() -> Vec<SdkError> {
        vec![
            SdkError::Io {
                source: std::io::Error::other("boom"),
                path: Some(PathBuf::from("/tmp/x")),
            },
            SdkError::Schema(SchemaError::DescriptionEmpty),
            SdkError::ValidationErrors(vec![ValidationError::MissingRequiredFile {
                file: "__init__.py".into(),
            }]),
            SdkError::Archive {
                message: "path too long".into(),
            },
            SdkError::PathTooLong {
                archive_path: "plugin-name-0.1.0/a/b/c/extremely/deep/path/leaf".into(),
                limit: 255,
            },
            SdkError::Hash {
                source: std::io::Error::other("read failed"),
            },
            SdkError::AlreadyPublished {
                name: "downsampler".into(),
                version: "1.2.0".into(),
                existing_versions: vec!["1.0.0".into(), "1.1.0".into(), "1.2.0".into()],
            },
            SdkError::CanonicalCollision {
                name: "my-plugin".into(),
                canonical: "my_plugin".into(),
                existing: vec![
                    ("my_plugin".into(), semver::Version::new(1, 0, 0)),
                    ("my_plugin".into(), semver::Version::new(1, 1, 0)),
                ],
            },
            SdkError::EntryNotFound {
                name: "downsampler".into(),
                version: "1.2.0".into(),
            },
            SdkError::InvalidExcludePattern {
                pattern: "tests/**[".into(),
                message: "unclosed character class".into(),
            },
        ]
    }

    #[test]
    fn sdk_error_display_stable() {
        let rendered: Vec<String> = every_sdk_variant().iter().map(|e| e.to_string()).collect();
        insta::assert_yaml_snapshot!("sdk_error_display", rendered);
    }

    #[test]
    fn sdk_error_variant_tags_stable() {
        let tags: Vec<&'static str> = every_sdk_variant()
            .iter()
            .map(SdkError::variant_name)
            .collect();
        insta::assert_yaml_snapshot!("sdk_error_variant_tags", tags);
    }

    #[test]
    fn validation_report_empty_is_ok() {
        let report = ValidationReport::new();
        assert!(report.is_empty());
        assert_eq!(report.len(), 0);
        assert!(report.into_result().is_ok());
    }

    #[test]
    fn validation_report_non_empty_becomes_err() {
        let mut report = ValidationReport::new();
        report.push(ValidationError::MissingRequiredFile {
            file: "__init__.py".into(),
        });
        assert!(!report.is_empty());
        assert_eq!(report.len(), 1);
        match report.into_result() {
            Err(ValidationFailure::Invalid(errs)) => {
                assert_eq!(errs.len(), 1);
            }
            other => panic!("expected ValidationFailure::Invalid, got {other:?}"),
        }
    }

    #[test]
    fn validation_report_extend_collects() {
        let mut report = ValidationReport::new();
        report.extend([
            ValidationError::NoEntryPoint,
            ValidationError::MissingRequiredFile {
                file: "manifest.toml".into(),
            },
        ]);
        assert_eq!(report.len(), 2);
    }

    #[test]
    fn validation_failure_converts_to_sdk_error() {
        let invalid = ValidationFailure::Invalid(vec![ValidationError::NoEntryPoint]);
        match SdkError::from(invalid) {
            SdkError::ValidationErrors(errs) => assert_eq!(errs.len(), 1),
            other => panic!("expected ValidationErrors, got {other:?}"),
        }

        let io = ValidationFailure::Io {
            source: std::io::Error::other("boom"),
            path: Some(PathBuf::from("/tmp/x")),
        };
        match SdkError::from(io) {
            SdkError::Io { path, .. } => {
                assert_eq!(path.as_deref(), Some(std::path::Path::new("/tmp/x")));
            }
            other => panic!("expected Io, got {other:?}"),
        }
    }

    #[test]
    fn schema_errors_convert_to_validation_failure() {
        use influxdb3_plugin_schemas::{FieldPath, ReportedError, SchemaErrors};
        let errors = SchemaErrors::new(vec![ReportedError::new(
            FieldPath::root().field("plugin").field("description"),
            SchemaError::DescriptionEmpty,
        )]);
        match ValidationFailure::from(errors) {
            ValidationFailure::Invalid(errs) => {
                assert_eq!(errs.len(), 1);
                assert!(matches!(errs[0], ValidationError::SchemaReported(_)));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn schema_error_auto_converts() {
        fn try_it() -> Result<(), SdkError> {
            Err(SchemaError::DescriptionEmpty)?;
            Ok(())
        }
        let err = try_it().unwrap_err();
        assert!(matches!(
            err,
            SdkError::Schema(SchemaError::DescriptionEmpty)
        ));
    }

    /// Schemas errors wrapped in `SdkError::Schema` must preserve the
    /// structured payload. With `#[error(transparent)]`, pattern-matching
    /// on the wrapper variant is the correct propagation test, and
    /// `Error::source()` still reaches any nested `#[source]` at the bottom.
    #[test]
    fn select_error_maps_to_validation_failure_invalid_exclude_pattern() {
        use crate::plugin_source_files::SelectError;
        let se = SelectError::InvalidExcludePattern {
            pattern: "[z-a]".into(),
            message: "bad".into(),
        };
        match ValidationFailure::from(se) {
            ValidationFailure::InvalidExcludePattern { pattern, .. } => {
                assert_eq!(pattern, "[z-a]")
            }
            other => panic!("expected InvalidExcludePattern, got {other:?}"),
        }
    }

    #[test]
    fn validation_invalid_exclude_pattern_converts_to_sdk_invalid_exclude_pattern() {
        let vf = ValidationFailure::InvalidExcludePattern {
            pattern: "[z-a]".into(),
            message: "bad".into(),
        };
        match SdkError::from(vf) {
            SdkError::InvalidExcludePattern { pattern, .. } => assert_eq!(pattern, "[z-a]"),
            other => panic!("expected InvalidExcludePattern, got {other:?}"),
        }
    }

    #[test]
    fn select_error_invalid_pattern_maps_to_sdk_invalid_exclude_pattern() {
        use crate::plugin_source_files::SelectError;
        let se = SelectError::InvalidExcludePattern {
            pattern: "[z-a]".into(),
            message: "bad glob".into(),
        };
        match SdkError::from(se) {
            SdkError::InvalidExcludePattern { pattern, .. } => assert_eq!(pattern, "[z-a]"),
            other => panic!("expected InvalidExcludePattern, got {other:?}"),
        }
    }

    #[test]
    fn schemas_error_structured_payload_preserved_via_sdk_schema() {
        use std::error::Error as _;

        let wrapped = SdkError::from(SchemaError::InvalidPluginName {
            name: "Bad Name".into(),
        });
        match &wrapped {
            SdkError::Schema(SchemaError::InvalidPluginName { name }) => {
                assert_eq!(name, "Bad Name");
            }
            other => panic!("expected SdkError::Schema(InvalidPluginName), got {other:?}"),
        }
        assert!(wrapped.to_string().contains("Bad Name"));

        let sem_err = semver::Version::parse("1.2").unwrap_err();
        let wrapped = SdkError::from(SchemaError::InvalidVersion {
            version: "1.2".into(),
            source: sem_err,
        });
        let bottom = wrapped
            .source()
            .expect("nested source reaches semver::Error");
        assert!(bottom.downcast_ref::<semver::Error>().is_some());
    }
}

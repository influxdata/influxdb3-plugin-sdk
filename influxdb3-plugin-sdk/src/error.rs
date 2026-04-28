//! Error types for the SDK.
//!
//! Two-layer design:
//! - [`SdkError`] — crate-level error returned by every public function.
//! - [`ValidationError`] — individual validation failures, collected into
//!   [`ValidationReport`] and surfaced together via
//!   [`SdkError::ValidationErrors`] so callers get every error in one pass.

use influxdb3_plugin_schemas::{ReportedError, SchemaError, SchemaErrors, TriggerType};
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
        "plugin ({name:?}, {version:?}) already exists in the target index. \
         Existing versions of {name:?} in this index: {existing_versions:?}. \
         Increment version in manifest.toml or run `yank` instead."
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
         entries sharing canonical form {canonical:?}: {existing:?}. \
         Rename to one of the existing spellings or choose a distinct name."
    )]
    CanonicalCollision {
        name: String,
        canonical: String,
        existing: Vec<(String, semver::Version)>,
    },

    #[error("plugin ({name:?}, {version:?}) is not present in the target index")]
    EntryNotFound { name: String, version: String },

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
            _ => unreachable!("IndexInsertError has no other variants in the current schemas version"),
        }
    }
}

/// An individual validation failure.
///
/// Collected into [`ValidationReport`] and surfaced together via
/// [`SdkError::ValidationErrors`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ValidationError {
    /// Wraps a structural [`ReportedError`] from the schemas crate's
    /// two-phase parse (`Manifest::parse_toml` / `Index::parse_json`),
    /// preserving the field path and inner `SchemaError` losslessly so the
    /// CLI can render structural and cross-file diagnostics in one array.
    #[error(transparent)]
    SchemaReported(ReportedError),

    #[error("required file {file:?} is missing from the plugin directory")]
    MissingRequiredFile { file: String },

    #[error("__init__.py does not parse as valid Python: {message}")]
    PythonParse { message: String },

    #[error(
        "trigger {trigger:?} is declared in manifest.toml but has no matching \
         top-level `def {}(...)` in __init__.py",
        .trigger.as_str()
    )]
    TriggerNotImplemented { trigger: TriggerType },

    #[error(
        "trigger {trigger:?} is implemented as `async def` in __init__.py; \
         the runtime invokes trigger functions synchronously"
    )]
    AsyncTriggerFn { trigger: TriggerType },

    /// Plugin `(name, version)` already exists in the target index. Surfaces
    /// from [`crate::validate::plugin_dir_with_index`] so `validate --index`
    /// can collect uniqueness conflicts alongside other validation errors.
    /// The mutation-boundary check in `mutate_index::add_entry` returns the
    /// distinct [`SdkError::AlreadyPublished`] instead.
    #[error(
        "plugin ({name:?}, {version:?}) already exists in the target index; \
         increment version or run `yank` instead"
    )]
    NameVersionConflict { name: String, version: String },

}

impl ValidationError {
    /// Stable tag per variant (same drift-guard role as `SdkError::variant_name`).
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::SchemaReported(_) => "SchemaReported",
            Self::MissingRequiredFile { .. } => "MissingRequiredFile",
            Self::PythonParse { .. } => "PythonParse",
            Self::TriggerNotImplemented { .. } => "TriggerNotImplemented",
            Self::AsyncTriggerFn { .. } => "AsyncTriggerFn",
            Self::NameVersionConflict { .. } => "NameVersionConflict",
        }
    }
}

/// Multi-error accumulator for validation passes.
///
/// Callers [`push`](Self::push) failures as they're encountered, then
/// [`into_result`](Self::into_result) to get `Ok(())` (empty) or
/// `Err(SdkError::ValidationErrors(errors))`. The type forces collect-all
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

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// `Ok(())` if empty, else `Err(SdkError::ValidationErrors(errors))`.
    pub fn into_result(self) -> Result<(), SdkError> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(SdkError::ValidationErrors(self.errors))
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
        ]
    }

    fn every_validation_variant() -> Vec<ValidationError> {
        use influxdb3_plugin_schemas::FieldPath;
        vec![
            ValidationError::SchemaReported(ReportedError::new(
                FieldPath::root().field("plugin").field("description"),
                SchemaError::DescriptionEmpty,
            )),
            ValidationError::MissingRequiredFile {
                file: "__init__.py".into(),
            },
            ValidationError::PythonParse {
                message: "unexpected token".into(),
            },
            ValidationError::TriggerNotImplemented {
                trigger: TriggerType::ProcessWrites,
            },
            ValidationError::AsyncTriggerFn {
                trigger: TriggerType::ProcessScheduledCall,
            },
            ValidationError::NameVersionConflict {
                name: "downsampler".into(),
                version: "1.2.0".into(),
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
    fn validation_error_display_stable() {
        let rendered: Vec<String> = every_validation_variant()
            .iter()
            .map(|e| e.to_string())
            .collect();
        insta::assert_yaml_snapshot!("validation_error_display", rendered);
    }

    #[test]
    fn validation_error_variant_tags_stable() {
        let tags: Vec<&'static str> = every_validation_variant()
            .iter()
            .map(ValidationError::variant_name)
            .collect();
        insta::assert_yaml_snapshot!("validation_error_variant_tags", tags);
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
            Err(SdkError::ValidationErrors(errs)) => {
                assert_eq!(errs.len(), 1);
            }
            other => panic!("expected ValidationErrors, got {other:?}"),
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

    /// `ValidationError::SchemaReported` wraps `ReportedError` losslessly
    /// (path + inner variant), so downstream callers can pattern-match on
    /// the original `SchemaError` variant and field path.
    #[test]
    fn schemas_error_structured_payload_preserved_via_validation_schema_reported() {
        use influxdb3_plugin_schemas::FieldPath;
        let reported = ReportedError::new(
            FieldPath::root().field("plugin").field("description"),
            SchemaError::DescriptionEmpty,
        );
        let wrapped = ValidationError::SchemaReported(reported);
        match &wrapped {
            ValidationError::SchemaReported(r) => {
                assert_eq!(r.path.as_str(), "plugin.description");
                assert!(matches!(r.error, SchemaError::DescriptionEmpty));
            }
            other => panic!("expected SchemaReported, got {other:?}"),
        }
    }
}

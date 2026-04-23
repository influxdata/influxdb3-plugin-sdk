//! Error types for the SDK.
//!
//! Two-layer design:
//! - [`SdkError`] — crate-level error type returned by every public function.
//!   Covers I/O, schema propagation, multi-error validation results,
//!   archive/hash failures, and two specific policy-check failures
//!   (`AlreadyPublished` for S2-2, `PathOverlap` for S2-12).
//! - [`ValidationError`] — individual validation failures, collected into
//!   [`ValidationReport`] and surfaced together via
//!   [`SdkError::ValidationErrors`] per Spec 2 Validation's "all errors
//!   reported together" contract.

use influxdb3_plugin_schemas::{ReportedError, SchemaError, SchemaErrors, TriggerType};
use std::path::PathBuf;

/// Crate-level error type.
///
/// Marked `#[non_exhaustive]`: variant additions are not breaking. Field
/// additions to existing variants ARE breaking — follow the Plan 1 pattern
/// (introduce a new variant, deprecate the old one).
///
/// This crate is internal per Spec 2 Stability, so the stability bar is
/// softer than `influxdb3-plugin-schemas` — but the `#[non_exhaustive]`
/// discipline still prevents accidental breaks for the CLI crate (Plan 3).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SdkError {
    #[error("I/O error{}: {source}", path_suffix(.path.as_ref()))]
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

    #[error("hash computation failed: {source}")]
    Hash {
        #[source]
        source: std::io::Error,
    },

    #[error(
        "plugin ({name:?}, {version:?}) already exists in the target index; \
         increment version or run `yank` instead"
    )]
    AlreadyPublished { name: String, version: String },

    #[error("plugin ({name:?}, {version:?}) is not present in the target index")]
    EntryNotFound { name: String, version: String },

    #[error(
        "output directory {output:?} overlaps with input path {input:?}; \
         they must be disjoint"
    )]
    PathOverlap { input: PathBuf, output: PathBuf },
}

impl SdkError {
    /// Stable tag per variant — same semver-drift guard pattern as
    /// `SchemaError::variant_name`. The exhaustive match forces new
    /// variants to be registered with test fixtures.
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::Io { .. } => "Io",
            Self::Schema(_) => "Schema",
            Self::ValidationErrors(_) => "ValidationErrors",
            Self::Archive { .. } => "Archive",
            Self::Hash { .. } => "Hash",
            Self::AlreadyPublished { .. } => "AlreadyPublished",
            Self::EntryNotFound { .. } => "EntryNotFound",
            Self::PathOverlap { .. } => "PathOverlap",
        }
    }
}

fn path_suffix(path: Option<&PathBuf>) -> String {
    match path {
        Some(p) => format!(" at {}", p.display()),
        None => String::new(),
    }
}

/// Adapts a schemas-layer `SchemaErrors` into the SDK's single diagnostics
/// container. Each `ReportedError` becomes one
/// [`ValidationError::SchemaReported`] entry inside
/// [`SdkError::ValidationErrors`] — preserving field paths and inner
/// `SchemaError` variants without lossy string-squashing.
///
/// This is the canonical conversion path used by `?` at every site that
/// calls `Manifest::parse_toml` or `Index::parse_json`.
impl From<SchemaErrors> for SdkError {
    fn from(errors: SchemaErrors) -> Self {
        let diagnostics = errors
            .into_iter()
            .map(ValidationError::SchemaReported)
            .collect();
        SdkError::ValidationErrors(diagnostics)
    }
}

/// An individual validation failure.
///
/// Collected into [`ValidationReport`] during a validation pass, then
/// surfaced together via [`SdkError::ValidationErrors`] per Spec 2
/// Validation's multi-error contract.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ValidationError {
    /// Wraps a structural [`ReportedError`] from the schemas crate's
    /// two-phase parse (`Manifest::parse_toml` / `Index::parse_json`).
    /// Preserves the schemas-level field path and inner `SchemaError`
    /// variant losslessly so the CLI can render structural diagnostics
    /// alongside cross-file diagnostics in one `--output json` array.
    ///
    /// The standard conversion path is via [`From<SchemaErrors> for
    /// SdkError`], which spreads each `ReportedError` from a `SchemaErrors`
    /// into one `SchemaReported` entry inside `SdkError::ValidationErrors`.
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

    /// Plugin identified by `(name, version)` already exists in the target
    /// index. Surfaces from [`crate::validate::plugin_dir_with_index`] so the
    /// CLI's `validate --index` flag can collect uniqueness conflicts
    /// alongside other validation errors per Spec 2 S2-15's validator-idiom
    /// contract. `crate::mutate_index::add_entry` continues to enforce S2-2
    /// at the mutation boundary by returning `SdkError::AlreadyPublished`.
    #[error(
        "plugin ({name:?}, {version:?}) already exists in the target index; \
         increment version or run `yank` instead"
    )]
    NameVersionConflict { name: String, version: String },
}

impl ValidationError {
    /// Stable tag per variant. Same drift-guard role as
    /// `SdkError::variant_name`.
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
/// Callers use [`push`](Self::push) to record failures as they're encountered,
/// then [`into_result`](Self::into_result) to convert into the final
/// `Result<(), SdkError>` — empty report becomes `Ok(())`, non-empty becomes
/// `Err(SdkError::ValidationErrors(vec))`.
///
/// This encodes Spec 2 Validation's "All validation errors are collected and
/// reported together rather than failing on the first" contract at the type
/// level — callers can't forget to collect, because the builder is the only
/// way to return validation results.
#[derive(Debug, Default)]
pub struct ValidationReport {
    errors: Vec<ValidationError>,
}

impl ValidationReport {
    /// Creates an empty report.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a single validation error.
    pub fn push(&mut self, err: ValidationError) {
        self.errors.push(err);
    }

    /// Returns `true` if no errors have been recorded.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Number of recorded errors.
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// Consumes the report. Returns `Ok(())` if empty, else
    /// `Err(SdkError::ValidationErrors(errors))`.
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
            SdkError::Hash {
                source: std::io::Error::other("read failed"),
            },
            SdkError::AlreadyPublished {
                name: "downsampler".into(),
                version: "1.2.0".into(),
            },
            SdkError::EntryNotFound {
                name: "downsampler".into(),
                version: "1.2.0".into(),
            },
            SdkError::PathOverlap {
                input: PathBuf::from("/a/index.json"),
                output: PathBuf::from("/a"),
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
        // `#[from] SchemaError` lets `?` operator in callers convert cleanly.
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

    /// Testing-spec S3 #7: schemas errors wrapped in `SdkError::Schema` must
    /// preserve the structured payload so callers can pattern-match on the
    /// inner `SchemaError` variant. With `#[error(transparent)]`, `.source()`
    /// delegates through — so the correct propagation test is
    /// pattern-matching on the wrapper variant.
    ///
    /// Additionally, for schemas variants that themselves carry a
    /// `#[source]` (e.g., `InvalidVersion` wraps `semver::Error`), the full
    /// `Error::source()` chain reaches the bottom.
    #[test]
    fn schemas_error_structured_payload_preserved_via_sdk_schema() {
        use std::error::Error as _;

        // 1. Field-carrying variant: pattern-matching recovers the fields.
        let wrapped = SdkError::from(SchemaError::InvalidPluginName {
            name: "Bad-Name".into(),
        });
        match &wrapped {
            SdkError::Schema(SchemaError::InvalidPluginName { name }) => {
                assert_eq!(name, "Bad-Name");
            }
            other => panic!("expected SdkError::Schema(InvalidPluginName), got {other:?}"),
        }
        // `#[error(transparent)]`: Display passes through unchanged.
        assert!(wrapped.to_string().contains("Bad-Name"));

        // 2. Nested `#[source]`: schemas variant wraps `semver::Error`.
        //    `Error::source()` on `SdkError::Schema` delegates through
        //    `transparent` to the SchemaError's own `#[source]`, reaching
        //    the underlying semver::Error at the bottom.
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

    /// Testing-spec S3 #7 mirror: `ValidationError::SchemaReported` wraps
    /// the schemas-layer `ReportedError` losslessly (path + inner variant),
    /// so pattern-matching downstream callers can inspect the original
    /// `SchemaError` variant and the field path that surfaced it.
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

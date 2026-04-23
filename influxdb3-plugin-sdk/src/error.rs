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

use influxdb3_plugin_schemas::{SchemaError, TriggerType};
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

/// An individual validation failure.
///
/// Collected into [`ValidationReport`] during a validation pass, then
/// surfaced together via [`SdkError::ValidationErrors`] per Spec 2
/// Validation's multi-error contract.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ValidationError {
    #[error(transparent)]
    Schema(SchemaError),

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
}

impl ValidationError {
    /// Stable tag per variant. Same drift-guard role as
    /// `SdkError::variant_name`.
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::Schema(_) => "Schema",
            Self::MissingRequiredFile { .. } => "MissingRequiredFile",
            Self::PythonParse { .. } => "PythonParse",
            Self::TriggerNotImplemented { .. } => "TriggerNotImplemented",
            Self::AsyncTriggerFn { .. } => "AsyncTriggerFn",
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
            SdkError::PathOverlap {
                input: PathBuf::from("/a/index.json"),
                output: PathBuf::from("/a"),
            },
        ]
    }

    fn every_validation_variant() -> Vec<ValidationError> {
        vec![
            ValidationError::Schema(SchemaError::DescriptionEmpty),
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
}

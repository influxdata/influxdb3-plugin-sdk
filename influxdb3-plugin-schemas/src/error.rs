//! Error types for schema parsing and validation.

/// Errors produced during schema parsing and validation.
///
/// Adding variants is a minor-version change; renaming, removing, reshaping,
/// or adding fields to existing variants is a major-version change. To evolve
/// a variant's payload, introduce a new variant rather than mutating the old.
///
/// `#[non_exhaustive]`: downstream matches must include a `_ =>` arm.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaError {
    #[error(
        "plugin name {name:?} must match `[a-zA-Z][a-zA-Z0-9_-]*` \
         (1-64 chars, ASCII alphanumerics / `-` / `_`, starting with a letter)"
    )]
    InvalidPluginName { name: String },

    #[error(
        "plugin name {name:?} matches a Windows reserved device name \
         (case-insensitive); pick a different name"
    )]
    ReservedPluginName { name: String },

    #[error("version {version:?} is not SemVer 2.0.0 compliant: {source}")]
    InvalidVersion {
        version: String,
        #[source]
        source: semver::Error,
    },

    #[error("description exceeds 200 characters (was {len})")]
    DescriptionTooLong { len: usize },

    #[error("description must not be empty")]
    DescriptionEmpty,

    #[error("description must be one line; got {len} chars across multiple lines")]
    DescriptionMultiline { len: usize },

    #[error("URL {url:?} must use http or https scheme (was {scheme:?})")]
    InvalidUrlScheme { url: String, scheme: String },

    #[error("URL {url:?} is malformed: {source}")]
    InvalidUrl {
        url: String,
        #[source]
        source: url::ParseError,
    },

    #[error(
        "trigger {trigger:?} is not in the closed set \
         {{process_writes, process_scheduled_call, process_request}}"
    )]
    UnknownTriggerType { trigger: String },

    #[error("triggers array must not be empty")]
    EmptyTriggers,

    #[error("database_version {range:?} is not a valid SemVer range: {source}")]
    InvalidDatabaseVersion {
        range: String,
        #[source]
        source: semver::Error,
    },

    /// A `dependencies.python` entry failed PEP 508 parsing.
    ///
    /// The `source` type comes from pre-1.0 `pep508_rs`; prefer [`.source()`]
    /// over matching the typed field to avoid coupling to its semver.
    ///
    /// [`.source()`]: std::error::Error::source
    #[error("python requirement {requirement:?} is not PEP 508-parseable: {source}")]
    InvalidPythonRequirement {
        requirement: String,
        #[source]
        source: Box<pep508_rs::Pep508Error<pep508_rs::VerbatimUrl>>,
    },

    #[error(
        "artifacts_url {url:?} uses unsupported scheme {scheme:?}; \
         allowed: http, https, file"
    )]
    UnsupportedArtifactScheme { url: String, scheme: String },

    #[error("hash {value:?} must be formatted as sha256:<64 lowercase hex chars>")]
    InvalidHash { value: String },

    #[error("duplicate plugin entry ({name:?}, {version:?}) in index")]
    DuplicateIndexEntry { name: String, version: String },

    #[error(
        "canonical collision: plugin name {name:?} conflicts with existing \
         entries sharing canonical form {canonical:?}: {existing:?}. \
         Rename to one of the existing spellings or choose a distinct name."
    )]
    CanonicalCollision {
        name: String,
        canonical: String,
        existing: Vec<(String, String)>,
    },

    #[error(
        "manifest_schema_version {found:?} has unsupported major; \
         this library supports major {supported}"
    )]
    UnsupportedManifestMajor { found: String, supported: u32 },

    #[error(
        "index_schema_version {found:?} has unsupported major; \
         this library supports major {supported}"
    )]
    UnsupportedIndexMajor { found: String, supported: u32 },

    #[error("schema version {value:?} must be formatted as <major>.<minor>")]
    MalformedSchemaVersion { value: String },

    #[error("TOML parse error: {source}")]
    TomlParse {
        #[source]
        source: toml::de::Error,
    },

    #[error("JSON parse error: {source}")]
    JsonParse {
        #[source]
        source: serde_json::Error,
    },

    #[error("JSON serialization error: {source}")]
    JsonSerialize {
        #[source]
        source: serde_json::Error,
    },
}

impl SchemaError {
    /// Stable string tag for the variant. Use for metrics keys, log
    /// categorization, and routing that must survive field-level changes.
    ///
    /// The exhaustive match forces new variants to be registered in
    /// `every_variant()` (compile error otherwise).
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::InvalidPluginName { .. } => "InvalidPluginName",
            Self::ReservedPluginName { .. } => "ReservedPluginName",
            Self::InvalidVersion { .. } => "InvalidVersion",
            Self::DescriptionTooLong { .. } => "DescriptionTooLong",
            Self::DescriptionEmpty => "DescriptionEmpty",
            Self::DescriptionMultiline { .. } => "DescriptionMultiline",
            Self::InvalidUrlScheme { .. } => "InvalidUrlScheme",
            Self::InvalidUrl { .. } => "InvalidUrl",
            Self::UnknownTriggerType { .. } => "UnknownTriggerType",
            Self::EmptyTriggers => "EmptyTriggers",
            Self::InvalidDatabaseVersion { .. } => "InvalidDatabaseVersion",
            Self::InvalidPythonRequirement { .. } => "InvalidPythonRequirement",
            Self::UnsupportedArtifactScheme { .. } => "UnsupportedArtifactScheme",
            Self::InvalidHash { .. } => "InvalidHash",
            Self::DuplicateIndexEntry { .. } => "DuplicateIndexEntry",
            Self::CanonicalCollision { .. } => "CanonicalCollision",
            Self::UnsupportedManifestMajor { .. } => "UnsupportedManifestMajor",
            Self::UnsupportedIndexMajor { .. } => "UnsupportedIndexMajor",
            Self::MalformedSchemaVersion { .. } => "MalformedSchemaVersion",
            Self::TomlParse { .. } => "TomlParse",
            Self::JsonParse { .. } => "JsonParse",
            Self::JsonSerialize { .. } => "JsonSerialize",
        }
    }
}

use crate::FieldPath;

/// A `SchemaError` paired with the field path at which it was detected.
///
/// `path` is empty for whole-document errors (TOML/JSON syntax); populated
/// for field-level validation errors.
#[derive(Debug)]
pub struct ReportedError {
    pub path: FieldPath,
    pub error: SchemaError,
}

impl ReportedError {
    pub fn new(path: FieldPath, error: SchemaError) -> Self {
        Self { path, error }
    }

    /// Constructs a `ReportedError` at the root path, for whole-document
    /// errors like `TomlParse` and `JsonParse`.
    pub fn at_root(error: SchemaError) -> Self {
        Self::new(FieldPath::root(), error)
    }
}

impl std::fmt::Display for ReportedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.path.as_str().is_empty() {
            write!(f, "{}", self.error)
        } else {
            write!(f, "{}: {}", self.path, self.error)
        }
    }
}

impl std::error::Error for ReportedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Collection of `ReportedError`s returned by `Manifest::parse_toml` and
/// `Index::parse_json`.
///
/// Always non-empty when returned as `Err(SchemaErrors)` — parse functions
/// return `Ok(_)` iff no validation errors were found.
#[derive(Debug)]
pub struct SchemaErrors(Vec<ReportedError>);

impl SchemaErrors {
    /// Debug-asserts `errors` is non-empty; constructing an empty
    /// `SchemaErrors` is a programming error (use `Ok(_)` for no errors).
    pub fn new(errors: Vec<ReportedError>) -> Self {
        debug_assert!(
            !errors.is_empty(),
            "SchemaErrors must contain at least one error; use Ok(_) for the no-error case"
        );
        Self(errors)
    }

    /// Convenience for syntax-level errors (TomlParse / JsonParse) and
    /// schema-version short-circuit errors.
    pub fn single_at_root(error: SchemaError) -> Self {
        Self(vec![ReportedError::at_root(error)])
    }

    pub fn errors(&self) -> &[ReportedError] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<ReportedError> {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Display for SchemaErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.len() {
            0 => f.write_str("(no errors)"),
            1 => self.0[0].fmt(f),
            n => {
                writeln!(f, "{n} schema validation errors:")?;
                for (i, err) in self.0.iter().enumerate() {
                    writeln!(f, "  {}. {err}", i + 1)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for SchemaErrors {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // First error's source; callers who want the full list use `.errors()`.
        self.0
            .first()
            .map(|r| &r.error as &(dyn std::error::Error + 'static))
    }
}

impl From<SchemaErrors> for Vec<ReportedError> {
    fn from(errors: SchemaErrors) -> Self {
        errors.into_vec()
    }
}

impl IntoIterator for SchemaErrors {
    type Item = ReportedError;
    type IntoIter = std::vec::IntoIter<ReportedError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a SchemaErrors {
    type Item = &'a ReportedError;
    type IntoIter = std::slice::Iter<'a, ReportedError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::ser::Error as _;

    /// Returns one instance of every `SchemaError` variant. Keep in sync with
    /// the enum: every variant MUST appear here so snapshot tests cover them.
    fn every_variant() -> Vec<SchemaError> {
        vec![
            SchemaError::InvalidPluginName {
                name: "Bad Name".into(),
            },
            SchemaError::ReservedPluginName {
                name: "con".into(),
            },
            SchemaError::InvalidVersion {
                version: "1.2".into(),
                source: semver::Version::parse("1.2").unwrap_err(),
            },
            SchemaError::DescriptionTooLong { len: 201 },
            SchemaError::DescriptionEmpty,
            SchemaError::DescriptionMultiline { len: 201 },
            SchemaError::InvalidUrlScheme {
                url: "ftp://bad".into(),
                scheme: "ftp".into(),
            },
            SchemaError::InvalidUrl {
                url: "not a url".into(),
                source: url::Url::parse("not a url").unwrap_err(),
            },
            SchemaError::UnknownTriggerType {
                trigger: "on_startup".into(),
            },
            SchemaError::EmptyTriggers,
            SchemaError::InvalidDatabaseVersion {
                range: ">=bad".into(),
                source: semver::VersionReq::parse(">=bad").unwrap_err(),
            },
            // Constructing Pep508Error needs a real parse failure.
            // `requests>>=2.0` (double operator) is unambiguously rejected;
            // inputs like `!!invalid!!` are accepted by some permissive paths.
            SchemaError::InvalidPythonRequirement {
                requirement: "requests>>=2.0".into(),
                source: Box::new(
                    "requests>>=2.0"
                        .parse::<pep508_rs::Requirement<pep508_rs::VerbatimUrl>>()
                        .unwrap_err(),
                ),
            },
            SchemaError::UnsupportedArtifactScheme {
                url: "s3://bucket/foo".into(),
                scheme: "s3".into(),
            },
            SchemaError::InvalidHash {
                value: "notahash".into(),
            },
            SchemaError::DuplicateIndexEntry {
                name: "dup".into(),
                version: "1.0.0".into(),
            },
            SchemaError::CanonicalCollision {
                name: "my-plugin".into(),
                canonical: "my_plugin".into(),
                existing: vec![("my_plugin".into(), "1.0.0".into())],
            },
            SchemaError::UnsupportedManifestMajor {
                found: "2.0".into(),
                supported: 1,
            },
            SchemaError::UnsupportedIndexMajor {
                found: "2.0".into(),
                supported: 1,
            },
            SchemaError::MalformedSchemaVersion {
                value: "abc".into(),
            },
            SchemaError::TomlParse {
                source: toml::from_str::<toml::Value>("= ").unwrap_err(),
            },
            SchemaError::JsonParse {
                source: serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
            },
            SchemaError::JsonSerialize {
                source: serde_json::Error::custom("forced"),
            },
        ]
    }

    /// Locks `Display` text of every variant — user-facing error messages are
    /// part of the semver-stable contract.
    #[test]
    fn display_shape_is_stable() {
        let rendered: Vec<String> = every_variant().iter().map(|e| e.to_string()).collect();
        insta::assert_yaml_snapshot!("display_shape", rendered);
    }

    /// Locks the variant-tag set. Breaking this means a variant was renamed,
    /// added, or removed — the load-bearing stability contract, since renaming
    /// can leave `display_shape_is_stable` untouched.
    #[test]
    fn variant_tags_are_stable() {
        let tags: Vec<&'static str> = every_variant().iter().map(|e| e.variant_name()).collect();
        insta::assert_yaml_snapshot!("variant_tags", tags);
    }

    /// `SchemaErrors` Display for the single-error case (TomlParse,
    /// JsonParse, schema-version short-circuit).
    #[test]
    fn schema_errors_display_single_error() {
        let se = SchemaErrors::single_at_root(SchemaError::EmptyTriggers);
        insta::assert_snapshot!("schema_errors_single", se.to_string());
    }

    /// `SchemaErrors` Display for the multi-error case (every defect with
    /// its field path).
    #[test]
    fn schema_errors_display_multiple_errors() {
        let se = SchemaErrors::new(vec![
            ReportedError::new(
                FieldPath::root().field("plugin").field("name"),
                SchemaError::InvalidPluginName {
                    name: "Bad Name".into(),
                },
            ),
            ReportedError::new(
                FieldPath::root().field("plugin").field("triggers").index(0),
                SchemaError::UnknownTriggerType {
                    trigger: "on_startup".into(),
                },
            ),
        ]);
        insta::assert_snapshot!("schema_errors_multiple", se.to_string());
    }

    /// `ReportedError::source()` walks back to the inner `SchemaError`,
    /// preserving the structural payload for downstream introspection.
    #[test]
    fn reported_error_source_chain_reaches_schema_error() {
        use std::error::Error as _;
        let re = ReportedError::new(
            FieldPath::root().field("plugin").field("name"),
            SchemaError::InvalidPluginName { name: "Bad".into() },
        );
        let src = re.source().expect("source exists");
        assert!(src.downcast_ref::<SchemaError>().is_some());
    }

    #[test]
    fn reserved_plugin_name_variant_renders_windows_message() {
        let err = SchemaError::ReservedPluginName {
            name: "con".into(),
        };
        let text = err.to_string();
        assert!(
            text.contains("Windows reserved"),
            "expected Windows-reserved mention, got: {text}"
        );
        assert!(text.contains("\"con\""), "expected original name, got: {text}");
        assert_eq!(err.variant_name(), "ReservedPluginName");
    }
}

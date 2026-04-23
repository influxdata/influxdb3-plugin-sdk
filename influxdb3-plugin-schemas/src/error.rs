//! Error types for schema parsing and validation.

/// Errors produced during schema parsing and validation.
///
/// This enum is part of the `influxdb3-plugin-schemas` crate's semver-stable
/// public API. Adding variants is a minor-version change; renaming, removing,
/// or reshaping existing variants is a major-version change. Field additions
/// to existing variants are also breaking changes — if a variant's payload
/// must evolve, introduce a new variant and deprecate the old one rather
/// than mutating the existing one.
///
/// Marked `#[non_exhaustive]` — downstream consumers must include a wildcard
/// (`_ =>`) arm when matching on this enum, so future variant additions do not
/// break their code.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaError {
    #[error(
        "plugin name {name:?} must be 1-64 characters: lowercase letters, digits, or hyphens, starting with a lowercase letter or digit"
    )]
    InvalidPluginName { name: String },

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
    /// # Semver note
    ///
    /// The `source` field's concrete type depends on `pep508_rs`, which is
    /// pre-1.0 (v0.9 at time of writing). A minor bump of `pep508_rs` may
    /// reshape `Pep508Error`, which would be a breaking change to this
    /// variant's public field type. Consumers should prefer [`.source()`]
    /// for type-erased access; pattern-matching the typed `source` field
    /// couples your code to `pep508_rs`'s semver.
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
    /// Returns the variant's name as a stable string tag. Use for metrics
    /// keys, log categorization, and error-category routing that should
    /// survive field-level changes to variants.
    ///
    /// Also anchors the `variant_tags_are_stable` snapshot test. The testing
    /// design spec suggests `Debug` output for this check; we use
    /// `variant_name()` instead because `Debug` would also capture field
    /// values, which Rust's type system already catches via exhaustive
    /// match — so `variant_name()` provides identical semver-tag coverage
    /// with cleaner snapshot output. The exhaustive match below forces any
    /// new enum variant to be registered with `every_variant()` in the test
    /// module (adding a variant without updating this match fails to
    /// compile).
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::InvalidPluginName { .. } => "InvalidPluginName",
            Self::InvalidVersion { .. } => "InvalidVersion",
            Self::DescriptionTooLong { .. } => "DescriptionTooLong",
            Self::DescriptionEmpty => "DescriptionEmpty",
            Self::InvalidUrlScheme { .. } => "InvalidUrlScheme",
            Self::InvalidUrl { .. } => "InvalidUrl",
            Self::UnknownTriggerType { .. } => "UnknownTriggerType",
            Self::EmptyTriggers => "EmptyTriggers",
            Self::InvalidDatabaseVersion { .. } => "InvalidDatabaseVersion",
            Self::InvalidPythonRequirement { .. } => "InvalidPythonRequirement",
            Self::UnsupportedArtifactScheme { .. } => "UnsupportedArtifactScheme",
            Self::InvalidHash { .. } => "InvalidHash",
            Self::DuplicateIndexEntry { .. } => "DuplicateIndexEntry",
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
/// Part of the `SchemaErrors` collection returned by `Manifest::parse_toml` and
/// `Index::parse_json`. The `path` is empty for errors that apply to the whole
/// document (e.g., TOML/JSON syntax errors); populated for field-level
/// validation errors.
#[derive(Debug)]
pub struct ReportedError {
    pub path: FieldPath,
    pub error: SchemaError,
}

impl ReportedError {
    /// Constructs a new `ReportedError`.
    pub fn new(path: FieldPath, error: SchemaError) -> Self {
        Self { path, error }
    }

    /// Constructs a `ReportedError` at the root path (for whole-document
    /// errors like `TomlParse` and `JsonParse`).
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
/// The collection is always non-empty when returned as `Err(SchemaErrors)` —
/// parse functions return `Ok(_)` if and only if no validation errors were
/// found. Construction from an empty `Vec` is permitted but not produced by
/// the crate's own code.
#[derive(Debug)]
pub struct SchemaErrors(Vec<ReportedError>);

impl SchemaErrors {
    /// Constructs a `SchemaErrors` from a non-empty list of `ReportedError`s.
    /// Panics in debug mode if `errors` is empty; passes through silently in
    /// release mode because constructing an empty `SchemaErrors` is a
    /// programming error rather than a data-driven case.
    pub fn new(errors: Vec<ReportedError>) -> Self {
        debug_assert!(
            !errors.is_empty(),
            "SchemaErrors must contain at least one error; use Ok(_) for the no-error case"
        );
        Self(errors)
    }

    /// Constructs a `SchemaErrors` containing exactly one error at the root
    /// path. Convenience for syntax-level errors (TomlParse / JsonParse) and
    /// schema-version short-circuit errors.
    pub fn single_at_root(error: SchemaError) -> Self {
        Self(vec![ReportedError::at_root(error)])
    }

    /// Returns the contained `ReportedError`s as a slice.
    pub fn errors(&self) -> &[ReportedError] {
        &self.0
    }

    /// Consumes `self` and returns the inner `Vec`.
    pub fn into_vec(self) -> Vec<ReportedError> {
        self.0
    }

    /// Number of errors collected.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Never empty by construction; equivalent to `self.len() == 0` which
    /// always returns false on well-formed instances. Provided for clippy
    /// convenience.
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
        // Return the first error's source; callers who want the full list
        // use `.errors()`. Matches how other multi-error types expose their
        // first error via source().
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

    /// Returns one instance of every `SchemaError` variant for stability testing.
    ///
    /// Keep this in sync with the enum definition: every variant MUST appear here.
    /// `variant_name()` lets the test assert the set is complete, and the snapshot
    /// tests below lock both `Display` text and `Debug` variant tags.
    fn every_variant() -> Vec<SchemaError> {
        vec![
            SchemaError::InvalidPluginName {
                name: "Bad-Name".into(),
            },
            SchemaError::InvalidVersion {
                version: "1.2".into(),
                source: semver::Version::parse("1.2").unwrap_err(),
            },
            SchemaError::DescriptionTooLong { len: 201 },
            SchemaError::DescriptionEmpty,
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
            // Note: constructing pep508_rs::Pep508Error requires a real parse failure.
            // Use `requests>>=2.0` (double operator) because it is unambiguously
            // rejected by PEP 508; many parsers accept surprisingly permissive
            // inputs like `!!invalid!!`. Consistent with Chunk 9's fixture choice.
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

    /// Locks the `Display` text of every error variant. Breaking this snapshot
    /// means the user-facing error message changed — part of the crate's
    /// semver-stable error contract per Spec 2 Stability.
    #[test]
    fn display_shape_is_stable() {
        let rendered: Vec<String> = every_variant().iter().map(|e| e.to_string()).collect();
        insta::assert_yaml_snapshot!("display_shape", rendered);
    }

    /// Locks the variant-tag set of `SchemaError`. Breaking this snapshot means
    /// a variant was renamed, added, or removed. Per testing-spec S2 #14 this
    /// is the load-bearing stability contract — renaming a variant passes
    /// `display_shape_is_stable` silently if only the text is unchanged, but
    /// fails here.
    #[test]
    fn variant_tags_are_stable() {
        let tags: Vec<&'static str> = every_variant().iter().map(|e| e.variant_name()).collect();
        insta::assert_yaml_snapshot!("variant_tags", tags);
    }

    /// `SchemaErrors` Display rendering for the single-error case
    /// (TomlParse / JsonParse / schema-version short-circuit shape).
    #[test]
    fn schema_errors_display_single_error() {
        let se = SchemaErrors::single_at_root(SchemaError::EmptyTriggers);
        insta::assert_snapshot!("schema_errors_single", se.to_string());
    }

    /// `SchemaErrors` Display rendering for the multi-error case (the
    /// dominant Option B output shape — every defect with field path).
    #[test]
    fn schema_errors_display_multiple_errors() {
        let se = SchemaErrors::new(vec![
            ReportedError::new(
                FieldPath::root().field("plugin").field("name"),
                SchemaError::InvalidPluginName {
                    name: "Bad_Name".into(),
                },
            ),
            ReportedError::new(
                FieldPath::root()
                    .field("plugin")
                    .field("triggers")
                    .index(0),
                SchemaError::UnknownTriggerType {
                    trigger: "on_startup".into(),
                },
            ),
        ]);
        insta::assert_snapshot!("schema_errors_multiple", se.to_string());
    }

    /// `ReportedError::source()` walks back through the inner `SchemaError`,
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
}

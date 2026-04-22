//! Error types for schema parsing and validation.

/// Errors produced during schema parsing and validation.
///
/// This enum is part of the `influxdb3-plugin-schemas` crate's semver-stable
/// public API. Adding variants is a minor-version change; renaming, removing,
/// or reshaping existing variants is a major-version change.
///
/// Marked `#[non_exhaustive]` — downstream consumers must include a wildcard
/// (`_ =>`) arm when matching on this enum, so future variant additions do not
/// break their code.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaError {
    #[error("plugin name {name:?} does not match regex [a-z0-9][a-z0-9-]{{0,63}} (1–64 chars, lowercase alphanumeric + hyphen, starting with alphanumeric)")]
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

    #[error("duplicate plugin entry ({name}, {version}) in index")]
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
    /// Returns the variant's name as a stable string tag, useful for
    /// programmatic categorization.
    ///
    /// Also load-bearing for the `variant_tags_are_stable` snapshot test: the
    /// exhaustive match here is what forces any new enum variant to be
    /// registered with `every_variant()` in the test module — adding a
    /// variant without updating this match fails to compile.
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
            SchemaError::InvalidPluginName { name: "Bad-Name".into() },
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
            SchemaError::UnknownTriggerType { trigger: "on_startup".into() },
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
            SchemaError::InvalidHash { value: "notahash".into() },
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
            SchemaError::MalformedSchemaVersion { value: "abc".into() },
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
        let tags: Vec<&'static str> =
            every_variant().iter().map(|e| e.variant_name()).collect();
        insta::assert_yaml_snapshot!("variant_tags", tags);
    }
}

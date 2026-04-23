//! Plugin registry index (`index.json`) types and canonical serialization.

use crate::{Dependencies, Description, PluginName, SchemaError, TriggerType};
use serde::Serialize as _;
use serde::ser::Error as _;
use std::fmt;
use std::str::FromStr;
use unicode_normalization::UnicodeNormalization;

/// Supported major version of the index schema.
pub(crate) const SUPPORTED_INDEX_MAJOR: u32 = 1;

/// The `index_schema_version` top-level field. Same structure and major-gate
/// semantics as `ManifestSchemaVersion` but for the registry index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IndexSchemaVersion {
    major: u32,
    minor: u32,
}

impl IndexSchemaVersion {
    pub fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }
    pub fn major(&self) -> u32 {
        self.major
    }
    pub fn minor(&self) -> u32 {
        self.minor
    }
}

impl fmt::Display for IndexSchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl FromStr for IndexSchemaVersion {
    type Err = SchemaError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let malformed = || SchemaError::MalformedSchemaVersion {
            value: s.to_owned(),
        };
        let (major_str, minor_str) = s.split_once('.').ok_or_else(malformed)?;
        if major_str.is_empty() || minor_str.is_empty() || minor_str.contains('.') {
            return Err(malformed());
        }
        let major: u32 = major_str.parse().map_err(|_| malformed())?;
        let minor: u32 = minor_str.parse().map_err(|_| malformed())?;
        if major != SUPPORTED_INDEX_MAJOR {
            return Err(SchemaError::UnsupportedIndexMajor {
                found: s.to_owned(),
                supported: SUPPORTED_INDEX_MAJOR,
            });
        }
        Ok(Self { major, minor })
    }
}

impl<'de> serde::Deserialize<'de> for IndexSchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::from_str(&raw).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for IndexSchemaVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

/// Registry artifact-base URL. Scheme-validated per Spec 1 S1-7 invariant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArtifactsUrl(url::Url);

impl ArtifactsUrl {
    pub fn try_new(raw: &str) -> Result<Self, SchemaError> {
        let url = url::Url::parse(raw).map_err(|source| SchemaError::InvalidUrl {
            url: raw.to_owned(),
            source,
        })?;
        match url.scheme() {
            "https" | "http" | "file" => Ok(Self(url)),
            other => Err(SchemaError::UnsupportedArtifactScheme {
                url: raw.to_owned(),
                scheme: other.to_owned(),
            }),
        }
    }

    pub fn as_url(&self) -> &url::Url {
        &self.0
    }
}

impl fmt::Display for ArtifactsUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'de> serde::Deserialize<'de> for ArtifactsUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::try_new(&raw).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for ArtifactsUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(&self.0)
    }
}

/// SHA-256 artifact hash, stored in the canonical form
/// `sha256:<64 lowercase hex chars>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArtifactHash(String);

impl ArtifactHash {
    pub fn try_new(raw: &str) -> Result<Self, SchemaError> {
        const PREFIX: &str = "sha256:";
        let invalid = || SchemaError::InvalidHash {
            value: raw.to_owned(),
        };
        let Some(hex) = raw.strip_prefix(PREFIX) else {
            return Err(invalid());
        };
        if hex.len() != 64
            || !hex
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        {
            return Err(invalid());
        }
        Ok(Self(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ArtifactHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for ArtifactHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::try_new(&raw).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for ArtifactHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

/// A parsed plugin registry index.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Index {
    pub index_schema_version: IndexSchemaVersion,
    pub artifacts_url: ArtifactsUrl,
    pub plugins: Vec<IndexEntry>,
}

/// One per-version entry inside an index's `plugins[]` array.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct IndexEntry {
    pub name: PluginName,
    pub version: semver::Version,
    pub description: Description,
    pub triggers: Vec<TriggerType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<url::Url>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<url::Url>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<url::Url>,
    pub dependencies: Dependencies,
    pub hash: ArtifactHash,
    #[serde(default, skip_serializing_if = "is_false")]
    pub yanked: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Index {
    /// Parses an index from a JSON string.
    ///
    /// Two-phase: raw JSON deserialize → schema-version short-circuit →
    /// per-entry field validation with error collection (including
    /// duplicate `(name, version)` detection). All field-level errors from
    /// a single document are collected into `SchemaErrors` with field-path
    /// context.
    ///
    /// # Examples
    ///
    /// ```
    /// use influxdb3_plugin_schemas::Index;
    ///
    /// let source = r#"{
    ///   "index_schema_version": "1.0",
    ///   "artifacts_url": "https://plugins.example.com/artifacts",
    ///   "plugins": []
    /// }"#;
    ///
    /// let index = Index::parse_json(source).unwrap();
    /// assert!(index.plugins.is_empty());
    /// ```
    pub fn parse_json(input: &str) -> Result<Self, crate::SchemaErrors> {
        use crate::raw::RawIndex;
        use crate::{FieldPath, ReportedError, SchemaErrors};
        use std::collections::HashSet;
        use std::str::FromStr;

        // Phase 1: raw deserialize.
        let raw: RawIndex = serde_json::from_str(input)
            .map_err(|source| SchemaErrors::single_at_root(SchemaError::JsonParse { source }))?;

        // Phase 2a: schema-version short-circuit.
        let schema_version =
            IndexSchemaVersion::from_str(&raw.index_schema_version).map_err(|e| {
                SchemaErrors::new(vec![ReportedError::new(
                    FieldPath::root().field("index_schema_version"),
                    e,
                )])
            })?;

        // Phase 2b: collect field-level errors.
        let mut errors = Vec::new();
        let root = FieldPath::root();

        let artifacts_url = match ArtifactsUrl::try_new(&raw.artifacts_url) {
            Ok(u) => Some(u),
            Err(e) => {
                errors.push(ReportedError::new(root.field("artifacts_url"), e));
                None
            }
        };

        let mut entries_ok: Vec<IndexEntry> = Vec::with_capacity(raw.plugins.len());
        let mut seen: HashSet<(String, String)> = HashSet::new();

        for (i, raw_entry) in raw.plugins.iter().enumerate() {
            let entry_path = root.field("plugins").index(i);
            let validated = validate_raw_entry(raw_entry, &entry_path, &mut errors);

            // Duplicate check uses the raw name/version strings — catches
            // duplicates even if the entry itself has other validation errors.
            let key = (raw_entry.name.clone(), raw_entry.version.clone());
            if !seen.insert(key) {
                errors.push(ReportedError::new(
                    entry_path.clone(),
                    SchemaError::DuplicateIndexEntry {
                        name: raw_entry.name.clone(),
                        version: raw_entry.version.clone(),
                    },
                ));
            }

            if let Some(entry) = validated {
                entries_ok.push(entry);
            }
        }

        if !errors.is_empty() {
            return Err(SchemaErrors::new(errors));
        }

        Ok(Index {
            index_schema_version: schema_version,
            artifacts_url: artifacts_url.unwrap(),
            plugins: entries_ok,
        })
    }

    /// Serializes this index to the canonical JSON form defined by Spec 2
    /// Reproducibility (derived index canonicalization):
    ///
    /// - Field ordering matches struct declaration order.
    /// - `plugins[]` sorted by `name` ascending, then `version` ascending
    ///   (SemVer precedence). Yank status does not affect ordering.
    /// - 2-space indent, pretty-printed.
    /// - Trailing newline.
    /// - NFC Unicode normalization applied to free-text `description` fields.
    ///   `PluginName`, `ArtifactHash`, schema versions, and trigger identifiers
    ///   are constrained to NFC-safe subsets (ASCII) by their validators. URL
    ///   fields are validated via `url::Url` parse which produces a
    ///   deterministic serialized form independent of input normalization.
    pub fn to_canonical_json(&self) -> Result<String, SchemaError> {
        // Clone so we can sort without mutating `self`.
        let mut normalized = self.clone();
        // Per Spec 1 Reproducibility: "sorted by `name` ascending, then
        // `version` ascending per SemVer 2.0.0 precedence."
        // `Version::cmp_precedence` implements the SemVer 2.0.0 precedence
        // rule (build metadata is ignored). The plain `Version::cmp` would
        // include build metadata and violate that contract.
        normalized.plugins.sort_by(|a, b| {
            a.name
                .as_str()
                .cmp(b.name.as_str())
                .then_with(|| a.version.cmp_precedence(&b.version))
        });
        // Apply NFC to description fields. Returns a structured error if NFC
        // pushes the string past the length bound (rare but possible — NFC can
        // add combining-mark sequences that cross 200 chars).
        for entry in &mut normalized.plugins {
            let nfc = normalize_nfc(entry.description.as_str());
            entry.description = Description::try_new(&nfc)?;
        }

        let mut buf = Vec::with_capacity(256);
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        normalized
            .serialize(&mut ser)
            .map_err(|source| SchemaError::JsonSerialize { source })?;
        buf.push(b'\n');
        String::from_utf8(buf).map_err(|e| SchemaError::JsonSerialize {
            source: serde_json::Error::custom(e.to_string()),
        })
    }
}

fn normalize_nfc(s: &str) -> String {
    s.nfc().collect()
}

/// Validates a single `RawIndexEntry`, pushing any errors into the supplied
/// `errors` Vec with paths relative to `path`. Returns `Some(IndexEntry)` on
/// success, `None` if any error was pushed for this entry (so the caller
/// can skip it when assembling the final `Index::plugins`).
fn validate_raw_entry(
    raw: &crate::raw::RawIndexEntry,
    path: &crate::FieldPath,
    errors: &mut Vec<crate::ReportedError>,
) -> Option<IndexEntry> {
    use crate::manifest::parse_optional_http_url_from_path;
    use crate::{Description, PluginName, PythonRequirement, ReportedError, TriggerType};
    use std::str::FromStr;

    let local_err_count = errors.len();

    let name = match PluginName::from_str(&raw.name) {
        Ok(n) => Some(n),
        Err(e) => {
            errors.push(ReportedError::new(path.field("name"), e));
            None
        }
    };

    let version = match semver::Version::parse(&raw.version) {
        Ok(v) => Some(v),
        Err(source) => {
            errors.push(ReportedError::new(
                path.field("version"),
                SchemaError::InvalidVersion {
                    version: raw.version.clone(),
                    source,
                },
            ));
            None
        }
    };

    let description = match Description::try_new(&raw.description) {
        Ok(d) => Some(d),
        Err(e) => {
            errors.push(ReportedError::new(path.field("description"), e));
            None
        }
    };

    let mut triggers_ok: Vec<TriggerType> = Vec::with_capacity(raw.triggers.len());
    if raw.triggers.is_empty() {
        errors.push(ReportedError::new(
            path.field("triggers"),
            SchemaError::EmptyTriggers,
        ));
    } else {
        for (i, t) in raw.triggers.iter().enumerate() {
            match TriggerType::from_str(t) {
                Ok(tt) => triggers_ok.push(tt),
                Err(e) => errors.push(ReportedError::new(path.field("triggers").index(i), e)),
            }
        }
    }

    let homepage = parse_optional_http_url_from_path(&raw.homepage, errors, path, "homepage");
    let repository = parse_optional_http_url_from_path(&raw.repository, errors, path, "repository");
    let documentation =
        parse_optional_http_url_from_path(&raw.documentation, errors, path, "documentation");

    let database_version = match semver::VersionReq::parse(&raw.dependencies.database_version) {
        Ok(r) => Some(r),
        Err(source) => {
            errors.push(ReportedError::new(
                path.field("dependencies").field("database_version"),
                SchemaError::InvalidDatabaseVersion {
                    range: raw.dependencies.database_version.clone(),
                    source,
                },
            ));
            None
        }
    };

    let mut python_ok: Vec<PythonRequirement> = Vec::with_capacity(raw.dependencies.python.len());
    for (i, p) in raw.dependencies.python.iter().enumerate() {
        match PythonRequirement::try_new(p) {
            Ok(pr) => python_ok.push(pr),
            Err(e) => errors.push(ReportedError::new(
                path.field("dependencies").field("python").index(i),
                e,
            )),
        }
    }

    let hash = match ArtifactHash::try_new(&raw.hash) {
        Ok(h) => Some(h),
        Err(e) => {
            errors.push(ReportedError::new(path.field("hash"), e));
            None
        }
    };

    if errors.len() > local_err_count {
        return None;
    }

    Some(IndexEntry {
        name: name.unwrap(),
        version: version.unwrap(),
        description: description.unwrap(),
        triggers: triggers_ok,
        homepage,
        repository,
        documentation,
        dependencies: crate::Dependencies {
            database_version: database_version.unwrap(),
            python: python_ok,
        },
        hash: hash.unwrap(),
        yanked: raw.yanked,
    })
}

#[cfg(test)]
mod schema_version_tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn parses_supported_major() {
        let v: IndexSchemaVersion = "1.1".parse().unwrap();
        assert_eq!(v.major(), 1);
        assert_eq!(v.minor(), 1);
    }

    #[test]
    fn rejects_unsupported_major() {
        let err = "2.0".parse::<IndexSchemaVersion>().unwrap_err();
        assert_matches!(err, SchemaError::UnsupportedIndexMajor { .. });
    }

    #[test]
    fn rejects_malformed() {
        assert_matches!(
            "abc".parse::<IndexSchemaVersion>(),
            Err(SchemaError::MalformedSchemaVersion { .. })
        );
    }
}

#[cfg(test)]
mod artifacts_url_tests {
    use super::*;
    use assert_matches::assert_matches;
    use rstest::rstest;

    #[rstest]
    #[case("https://plugins.example/artifacts")]
    #[case("http://localhost:8080/artifacts")]
    #[case("file:///srv/plugins")]
    fn valid_schemes_accepted(#[case] input: &str) {
        assert!(ArtifactsUrl::try_new(input).is_ok());
    }

    #[rstest]
    #[case("oci://registry.example")]
    #[case("s3://bucket/plugins")]
    #[case("git://example/plugins")]
    #[case("git+https://example/plugins")]
    #[case("ftp://example/plugins")]
    #[case("sftp://example/plugins")]
    fn rejected_schemes(#[case] input: &str) {
        let err = ArtifactsUrl::try_new(input).unwrap_err();
        assert_matches!(err, SchemaError::UnsupportedArtifactScheme { .. });
    }

    #[test]
    fn malformed_url_rejected() {
        let err = ArtifactsUrl::try_new("not a url").unwrap_err();
        assert_matches!(err, SchemaError::InvalidUrl { .. });
    }
}

#[cfg(test)]
mod artifact_hash_tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn valid_hash_accepted() {
        let h = ArtifactHash::try_new(
            "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
        )
        .unwrap();
        assert_eq!(h.as_str().len(), "sha256:".len() + 64);
    }

    #[test]
    fn wrong_prefix_rejected() {
        assert_matches!(
            ArtifactHash::try_new(
                "sha512:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
            ),
            Err(SchemaError::InvalidHash { .. })
        );
    }

    #[test]
    fn wrong_length_rejected() {
        assert_matches!(
            ArtifactHash::try_new("sha256:abc"),
            Err(SchemaError::InvalidHash { .. })
        );
    }

    #[test]
    fn uppercase_hex_rejected() {
        assert_matches!(
            ArtifactHash::try_new(
                "sha256:9F86D081884C7D659A2FEAA0C55AD015A3BF4F1B2B0B822CD15D6C15B0F00A08"
            ),
            Err(SchemaError::InvalidHash { .. })
        );
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;
    use assert_matches::assert_matches;
    use pretty_assertions::assert_eq;

    const MINIMAL: &str = r#"{
  "index_schema_version": "1.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    {
      "name": "downsampler",
      "version": "1.2.0",
      "description": "Test plugin",
      "triggers": ["process_writes"],
      "dependencies": {
        "database_version": ">=3.2.0,<4.0.0",
        "python": []
      },
      "hash": "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
    }
  ]
}"#;

    #[test]
    fn parses_minimal_index() {
        let idx = Index::parse_json(MINIMAL).unwrap();
        assert_eq!(idx.plugins.len(), 1);
        assert_eq!(idx.plugins[0].name.as_str(), "downsampler");
    }

    #[test]
    fn yanked_absent_means_not_yanked() {
        let idx = Index::parse_json(MINIMAL).unwrap();
        assert!(!idx.plugins[0].yanked);
    }

    #[test]
    fn yanked_true_parsed() {
        let src = MINIMAL.replace(r#""hash":"#, r#""yanked": true, "hash":"#);
        let idx = Index::parse_json(&src).unwrap();
        assert!(idx.plugins[0].yanked);
    }

    #[test]
    fn yanked_false_parsed() {
        let src = MINIMAL.replace(r#""hash":"#, r#""yanked": false, "hash":"#);
        let idx = Index::parse_json(&src).unwrap();
        assert!(!idx.plugins[0].yanked);
    }

    #[test]
    fn duplicate_name_version_rejected() {
        let dup = r#"{
  "index_schema_version": "1.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    { "name": "x", "version": "1.0.0", "description": "x", "triggers": ["process_writes"],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
    { "name": "x", "version": "1.0.0", "description": "x2", "triggers": ["process_writes"],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111" }
  ]
}"#;
        let errors = Index::parse_json(dup).unwrap_err();
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(
            errors.errors()[0].error,
            SchemaError::DuplicateIndexEntry { .. }
        );
        assert_eq!(errors.errors()[0].path.as_str(), "plugins[1]");
    }

    #[test]
    fn ignores_unknown_per_entry_field() {
        let with_unknown = MINIMAL.replace(r#""hash":"#, r#""experimental_tag": "beta", "hash":"#);
        assert!(Index::parse_json(&with_unknown).is_ok());
    }

    /// Per the core design doc's "Index-entry validation mirrors manifest
    /// validation" subsection, every IndexEntry must satisfy the same
    /// trigger / URL-scheme rules as the manifest.
    #[test]
    fn empty_triggers_rejected_per_entry() {
        let src = MINIMAL.replace(r#""triggers": ["process_writes"]"#, r#""triggers": []"#);
        let errors = Index::parse_json(&src).unwrap_err();
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(errors.errors()[0].error, SchemaError::EmptyTriggers);
        assert_eq!(errors.errors()[0].path.as_str(), "plugins[0].triggers");
    }

    #[test]
    fn invalid_homepage_scheme_rejected_per_entry() {
        let src = MINIMAL.replace(r#""hash":"#, r#""homepage": "ftp://bad/", "hash":"#);
        let errors = Index::parse_json(&src).unwrap_err();
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(
            errors.errors()[0].error,
            SchemaError::InvalidUrlScheme { .. }
        );
        assert_eq!(errors.errors()[0].path.as_str(), "plugins[0].homepage");
    }

    #[test]
    fn invalid_repository_scheme_rejected_per_entry() {
        let src = MINIMAL.replace(r#""hash":"#, r#""repository": "git://bad/", "hash":"#);
        let errors = Index::parse_json(&src).unwrap_err();
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(
            errors.errors()[0].error,
            SchemaError::InvalidUrlScheme { .. }
        );
        assert_eq!(errors.errors()[0].path.as_str(), "plugins[0].repository");
    }

    #[test]
    fn invalid_documentation_scheme_rejected_per_entry() {
        let src = MINIMAL.replace(
            r#""hash":"#,
            r#""documentation": "s3://bucket/docs", "hash":"#,
        );
        let errors = Index::parse_json(&src).unwrap_err();
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(
            errors.errors()[0].error,
            SchemaError::InvalidUrlScheme { .. }
        );
        assert_eq!(errors.errors()[0].path.as_str(), "plugins[0].documentation");
    }

    #[test]
    fn http_and_https_optional_urls_accepted() {
        let src = MINIMAL.replace(
            r#""hash":"#,
            r#""homepage": "http://example.com", "repository": "https://github.com/x/y", "hash":"#,
        );
        assert!(Index::parse_json(&src).is_ok());
    }

    #[test]
    fn ignores_unknown_top_level_field() {
        let src = MINIMAL.replace(
            r#""artifacts_url":"#,
            r#""experimental_top_level": true, "artifacts_url":"#,
        );
        assert!(Index::parse_json(&src).is_ok());
    }

    /// Two-phase parse collects per-entry defects across multiple entries
    /// AND duplicate-(name, version) detection — all in a single pass.
    #[test]
    fn collects_multiple_entry_defects_and_duplicate() {
        // plugins[0] has a valid entry.
        // plugins[1] has a bad hash.
        // plugins[2] duplicates plugins[0] (name + version).
        // plugins[3] has a bad description (too long).
        // Expect 3 errors: hash, duplicate, description.
        let long_desc = "a".repeat(201);
        let json = format!(
            r#"{{
  "index_schema_version": "1.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    {{ "name": "alpha", "version": "1.0.0", "description": "ok", "triggers": ["process_writes"],
       "dependencies": {{"database_version":">=3.0.0","python":[]}},
       "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000" }},
    {{ "name": "beta",  "version": "2.0.0", "description": "ok", "triggers": ["process_writes"],
       "dependencies": {{"database_version":">=3.0.0","python":[]}},
       "hash": "not-a-hash" }},
    {{ "name": "alpha", "version": "1.0.0", "description": "ok", "triggers": ["process_writes"],
       "dependencies": {{"database_version":">=3.0.0","python":[]}},
       "hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111" }},
    {{ "name": "gamma", "version": "3.0.0", "description": "{long_desc}", "triggers": ["process_writes"],
       "dependencies": {{"database_version":">=3.0.0","python":[]}},
       "hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222" }}
  ]
}}"#
        );

        let errors = Index::parse_json(&json).expect_err("should fail");
        let e = errors.errors();
        assert_eq!(
            e.len(),
            3,
            "expected 3 errors, got {}: {:?}",
            e.len(),
            e.iter()
                .map(|r| (r.path.as_str(), &r.error))
                .collect::<Vec<_>>()
        );

        let paths: Vec<&str> = e.iter().map(|r| r.path.as_str()).collect();
        assert!(
            paths.iter().any(|p| *p == "plugins[1].hash"),
            "missing hash path: {paths:?}"
        );
        assert!(
            paths.iter().any(|p| p.starts_with("plugins[2]")),
            "missing duplicate path: {paths:?}"
        );
        assert!(
            paths.iter().any(|p| *p == "plugins[3].description"),
            "missing description path: {paths:?}"
        );
    }

    #[test]
    fn index_schema_version_mismatch_short_circuits() {
        let json = r#"{
  "index_schema_version": "99.0",
  "artifacts_url": "ftp://bad",
  "plugins": []
}"#;
        let errors = Index::parse_json(json).expect_err("should fail");
        assert_eq!(errors.errors().len(), 1);
        assert_matches::assert_matches!(
            errors.errors()[0].error,
            SchemaError::UnsupportedIndexMajor { .. }
        );
    }
}

#[cfg(test)]
mod canonical_serialization_tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn make_entry(name: &str, version: semver::Version) -> IndexEntry {
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
    fn plugins_sorted_by_name_then_version() {
        let idx = Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![
                make_entry("zebra", semver::Version::new(1, 0, 0)),
                make_entry("alpha", semver::Version::new(2, 0, 0)),
                make_entry("alpha", semver::Version::new(1, 0, 0)),
            ],
        };
        let out = idx.to_canonical_json().unwrap();
        let alpha_1_pos = out.find("\"alpha\"").unwrap();
        let alpha_2_pos = out[alpha_1_pos + 1..].find("\"alpha\"").unwrap() + alpha_1_pos + 1;
        let zebra_pos = out.find("\"zebra\"").unwrap();
        assert!(alpha_1_pos < alpha_2_pos, "alpha 1.0 before alpha 2.0");
        assert!(alpha_2_pos < zebra_pos, "alpha before zebra");
    }

    #[test]
    fn two_serialize_calls_produce_byte_identical() {
        let idx = Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![make_entry("x", semver::Version::new(1, 0, 0))],
        };
        let a = idx.to_canonical_json().unwrap();
        let b = idx.to_canonical_json().unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn ends_with_newline() {
        let idx = Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![],
        };
        let out = idx.to_canonical_json().unwrap();
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn two_space_indent() {
        let idx = Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![make_entry("x", semver::Version::new(1, 0, 0))],
        };
        let out = idx.to_canonical_json().unwrap();
        assert!(
            out.contains("\n  \"index_schema_version\""),
            "expected 2-space indent at top level"
        );
    }

    #[test]
    fn snapshot_locks_full_shape() {
        let idx = Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![make_entry("alpha", semver::Version::new(1, 0, 0)), {
                let mut e = make_entry("beta", semver::Version::new(2, 1, 0));
                e.yanked = true;
                e
            }],
        };
        insta::assert_snapshot!("canonical_full_shape", idx.to_canonical_json().unwrap());
    }

    #[test]
    fn entry_field_order_is_canonical() {
        let idx = Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![make_entry("x", semver::Version::new(1, 0, 0))],
        };
        let out = idx.to_canonical_json().unwrap();
        let name_pos = out.find("\"name\"").unwrap();
        let version_pos = out.find("\"version\"").unwrap();
        let description_pos = out.find("\"description\"").unwrap();
        let triggers_pos = out.find("\"triggers\"").unwrap();
        let dependencies_pos = out.find("\"dependencies\"").unwrap();
        let hash_pos = out.find("\"hash\"").unwrap();
        assert!(name_pos < version_pos);
        assert!(version_pos < description_pos);
        assert!(description_pos < triggers_pos);
        assert!(triggers_pos < dependencies_pos);
        assert!(dependencies_pos < hash_pos);
    }

    #[test]
    fn nfc_normalization_makes_precomposed_equal_decomposed() {
        // Testing-spec S2 #9: "NFC test uses a precomposed-vs-decomposed pair
        // and asserts byte-identical output."
        //
        // Input A uses precomposed "é" (U+00E9). Input B uses "e" + combining
        // acute "\u{0301}" (U+0065 U+0301). NFC collapses both to U+00E9.
        let precomposed = Description::try_new("caf\u{00E9}").unwrap();
        let decomposed = Description::try_new("cafe\u{0301}").unwrap();
        let entry_a = IndexEntry {
            description: precomposed,
            ..make_entry("x", semver::Version::new(1, 0, 0))
        };
        let entry_b = IndexEntry {
            description: decomposed,
            ..make_entry("x", semver::Version::new(1, 0, 0))
        };

        let idx_a = Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![entry_a],
        };
        let idx_b = Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![entry_b],
        };
        assert_eq!(
            idx_a.to_canonical_json().unwrap(),
            idx_b.to_canonical_json().unwrap()
        );
    }

    #[test]
    fn yank_status_does_not_affect_sort() {
        let mut yanked_alpha = make_entry("alpha", semver::Version::new(1, 0, 0));
        yanked_alpha.yanked = true;
        let idx = Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![
                make_entry("beta", semver::Version::new(1, 0, 0)),
                yanked_alpha, // yanked but sorts first by name
            ],
        };
        let out = idx.to_canonical_json().unwrap();
        let alpha_pos = out.find("\"alpha\"").unwrap();
        let beta_pos = out.find("\"beta\"").unwrap();
        assert!(
            alpha_pos < beta_pos,
            "yanked alpha should still sort before beta"
        );
    }

    /// SemVer 2.0.0 precedence rule: a prerelease has lower precedence than
    /// the corresponding normal version (`1.0.0-alpha < 1.0.0`). Canonical
    /// ordering must respect this; otherwise yank-history queries and
    /// "latest version" selection would surface wrong results.
    #[test]
    fn prerelease_sorts_before_release_at_same_major_minor_patch() {
        let prerelease = make_entry("p", "1.0.0-alpha".parse().unwrap());
        let release = make_entry("p", semver::Version::new(1, 0, 0));
        let idx = Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            // Provide in reverse-of-expected order to force the sort.
            plugins: vec![release, prerelease],
        };
        let out = idx.to_canonical_json().unwrap();
        let alpha_pos = out.find(r#""1.0.0-alpha""#).unwrap();
        let release_pos = out.find(r#""1.0.0""#).unwrap();
        // Both substrings exist; `1.0.0-alpha` appears in the prerelease's
        // `version` field. The release's `1.0.0` appears later because
        // SemVer puts prereleases before the release.
        assert!(
            alpha_pos < release_pos,
            "prerelease 1.0.0-alpha must sort before release 1.0.0"
        );
    }

    /// SemVer 2.0.0 precedence rule: build metadata is ignored when
    /// determining version precedence. Two entries differing only by build
    /// metadata have equal precedence per `Version::cmp_precedence` (which
    /// is what `to_canonical_json` uses). The plain `Version::cmp` would
    /// produce a lexical ordering of the build string — wrong per spec.
    #[test]
    fn build_metadata_does_not_affect_precedence() {
        let v_a: semver::Version = "1.0.0+build.1".parse().unwrap();
        let v_b: semver::Version = "1.0.0+build.2".parse().unwrap();

        // SemVer 2.0.0: build metadata ignored for precedence.
        assert_eq!(v_a.cmp_precedence(&v_b), std::cmp::Ordering::Equal);
        // Sanity: plain Version::cmp DOES distinguish them, which is why
        // to_canonical_json must use cmp_precedence (not cmp) per Spec 1.
        assert_ne!(v_a.cmp(&v_b), std::cmp::Ordering::Equal);

        let idx = Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            // Provide in deliberate order; equal-precedence entries should
            // preserve insertion order via stable sort.
            plugins: vec![make_entry("p", v_a.clone()), make_entry("p", v_b.clone())],
        };
        let out = idx.to_canonical_json().unwrap();
        let pos_a = out.find("1.0.0+build.1").unwrap();
        let pos_b = out.find("1.0.0+build.2").unwrap();
        assert!(
            pos_a < pos_b,
            "stable sort preserves input order for equal-precedence entries"
        );
    }
}

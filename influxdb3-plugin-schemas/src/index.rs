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
    /// Enforces `(name, version)` uniqueness after structural parsing succeeds.
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
    pub fn parse_json(input: &str) -> Result<Self, SchemaError> {
        let parsed: Self =
            serde_json::from_str(input).map_err(|source| SchemaError::JsonParse { source })?;
        parsed.validate()?;
        Ok(parsed)
    }

    fn validate(&self) -> Result<(), SchemaError> {
        use std::collections::HashSet;
        let mut seen: HashSet<(&str, &semver::Version)> = HashSet::new();
        for entry in &self.plugins {
            let key = (entry.name.as_str(), &entry.version);
            if !seen.insert(key) {
                return Err(SchemaError::DuplicateIndexEntry {
                    name: entry.name.as_str().to_owned(),
                    version: entry.version.to_string(),
                });
            }
        }
        Ok(())
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
        normalized.plugins.sort_by(|a, b| {
            a.name
                .as_str()
                .cmp(b.name.as_str())
                .then_with(|| a.version.cmp(&b.version))
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
        assert_matches!(
            Index::parse_json(dup),
            Err(SchemaError::DuplicateIndexEntry { .. })
        );
    }

    #[test]
    fn ignores_unknown_per_entry_field() {
        let with_unknown = MINIMAL.replace(r#""hash":"#, r#""experimental_tag": "beta", "hash":"#);
        assert!(Index::parse_json(&with_unknown).is_ok());
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
}
